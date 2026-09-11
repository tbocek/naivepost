package main

// The first half of Prepare: STT for every input and frames at an interval,
// into inputs/. ffmpeg via os/exec, ASR/diarization via audio.cpp over HTTP
// (audiocpp.go).
//
// inputs/
//   <input-basename>/  voice16k.wav, transcript.{txt,tsv,srt}, words.json, turns.json
//   frames/<input-basename>/2026-08-08_19-59-00.jpg   one per interval, named for
//                      the wall-clock second; frame n is t = (n-1) * interval
//   meta.env           chosen inputs + interval
//
// Finished stages are skipped, so re-running resumes.

import (
	"bufio"
	"bytes"
	"encoding/json"
	"errors"
	"fmt"
	"math"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"sort"
	"strconv"
	"strings"
	"sync"
	"time"
)

var errStopped = errors.New("stopped by user")

// Where the local audio.cpp stack lives, what it runs on and what language it
// expects are settings now, not constants -- see appConf in setup.go. What
// stays here is what is measured rather than chosen: change these and the
// models misbehave, which is not a preference anyone can hold.
const (
	sampleRate = 16000 // what the ASR and diarization models are trained on

	// Sortformer refuses requests past its encoder position table (measured:
	// 90 s passes, 150 s does not), and its slot names mean nothing across
	// requests. Every window therefore carries the same short anchor clip of
	// known voices in front; whichever slot owns an anchor block IS that voice.
	diarWin     = 90.0
	diarScanHop = 60.0 // pass 1 stride: only has to see each voice once
	anchorPer   = 12.0 // seconds of each voice in the anchor
	anchorMin   = 4.0  // speech that makes a slot count as a voice
	minAnchorOv = 0.5  // anchor-block overlap to claim a slot
	diarTurnGap = 0.5  // merge same-speaker turns closer than this

	// The ASR encoders have a position table a session is far past (Nemotron
	// refuses well before 12 minutes; others run full context over the whole
	// recording). Long audio goes in as chunks, cut where nobody is talking.
	asrChunkMax  = 300.0 // longest audio in one ASR request
	asrChunkQwen = 60.0  // ...and for a model that allocates by the second (asrChunk)
	asrCutSeek   = 20.0  // how far from an even cut a silence is worth taking
	asrQuietDB   = -35   // what counts as quiet, in dBFS
	asrQuietMin  = 0.4   // and for how long

	// segment building
	mergeGap     = 0.7  // silence that ends a segment
	mergeMaxLen  = 12.0 // hard cap on segment length
	mergeMaxWord = 2.0  // Parakeet stretches word ends across silence; clamp
	mergeNear    = 1.0  // attribute a word to a turn this far away
)

type span struct {
	s, e float64
	slot string
}

// ---- run control -----------------------------------------------------------

// checkpoint is where pause and stop take effect: between subprocesses, never
// inside one -- a GPU job cannot be meaningfully frozen halfway.
func (a *App) checkpoint() error {
	for {
		if a.stopFlag.Load() {
			return errStopped
		}
		if !a.pauseFlag.Load() {
			return nil
		}
		time.Sleep(200 * time.Millisecond)
	}
}

// ---- process plumbing ------------------------------------------------------

// runCmd runs a subprocess and remembers it, so the stop button can kill it.
// A kill while stopFlag is set reports as errStopped, not as a failure.
//
// The command goes in the log BEFORE it runs, every time. Every subprocess this
// app starts is an ffmpeg, and what it is asked to do is the whole of what
// comes out: a flag in the wrong place is a video that plays two frames out of
// sync in one player and fine in another, and the answer to "what did it
// actually run" should not be reading the source. Written as a shell would
// take it, so it can be pasted, edited and run by hand.
func (a *App) runCmd(name string, args ...string) error {
	a.logCmd(name, args)
	cmd := exec.Command(name, args...)
	var out bytes.Buffer
	cmd.Stdout = &out
	cmd.Stderr = &out
	a.ctlMu.Lock()
	a.curCmds[cmd] = true
	a.ctlMu.Unlock()
	err := cmd.Run()
	a.ctlMu.Lock()
	delete(a.curCmds, cmd)
	a.ctlMu.Unlock()
	if err != nil {
		if a.stopFlag.Load() {
			return errStopped
		}
		tail := out.String()
		if len(tail) > 400 {
			tail = tail[len(tail)-400:]
		}
		// the whole command in the error as well as in the log: an error is
		// read where it lands -- a status line, a FAILED line at the end of a
		// run -- and hunting back up the log for the line that goes with it is
		// the part nobody does
		return fmt.Errorf("%s: %w\n%s\n%s", name, err, cmdLine(name, args), tail)
	}
	return nil
}

// logCmd puts one command in the log, at the detail indent the steps use for
// what they are doing rather than what they decided.
func (a *App) logCmd(name string, args []string) {
	a.logfIdle("    $ %s", cmdLine(name, args))
}

// cmdLine is a command as a shell would take it: every argument that needs
// quoting quoted, and the rest left alone. The paths in this app have spaces in
// them ("2026-09-10 15-04-39.mkv"), so a line printed with bare %v is a line
// that looks runnable and is not.
func cmdLine(name string, args []string) string {
	out := make([]string, 0, len(args)+1)
	out = append(out, shellArg(name))
	for _, a := range args {
		out = append(out, shellArg(a))
	}
	return strings.Join(out, " ")
}

// shellArg quotes one argument the way sh wants it: single quotes, with any
// single quote inside closed, escaped and reopened.
func shellArg(s string) string {
	if s != "" && !strings.ContainsFunc(s, needsQuote) {
		return s
	}
	return "'" + strings.ReplaceAll(s, "'", `'\''`) + "'"
}

func needsQuote(r rune) bool {
	switch {
	case r >= 'a' && r <= 'z', r >= 'A' && r <= 'Z', r >= '0' && r <= '9':
		return false
	}
	return !strings.ContainsRune("_@%+=:,./-", r)
}

// ffmpegProgress runs ffmpeg reporting completion against a known duration,
// for the long single-invocation phases (frame extraction).
func (a *App) ffmpegProgress(dur float64, cb func(float64), args ...string) error {
	full := append([]string{"-progress", "pipe:1", "-nostats"}, args...)
	a.logCmd(ffTool("ffmpeg"), full)
	cmd := exec.Command(ffTool("ffmpeg"), full...)
	var errBuf bytes.Buffer
	cmd.Stderr = &errBuf
	out, err := cmd.StdoutPipe()
	if err != nil {
		return err
	}
	a.ctlMu.Lock()
	a.curCmds[cmd] = true
	a.ctlMu.Unlock()
	if err := cmd.Start(); err != nil {
		a.ctlMu.Lock()
		delete(a.curCmds, cmd)
		a.ctlMu.Unlock()
		return err
	}
	sc := bufio.NewScanner(out)
	for sc.Scan() {
		if v, ok := strings.CutPrefix(sc.Text(), "out_time_us="); ok {
			var us float64
			fmt.Sscanf(v, "%f", &us)
			if dur > 0 {
				cb(us / 1e6 / dur)
			}
		}
	}
	err = cmd.Wait()
	a.ctlMu.Lock()
	delete(a.curCmds, cmd)
	a.ctlMu.Unlock()
	if err != nil {
		if a.stopFlag.Load() {
			return errStopped
		}
		tail := errBuf.String()
		if len(tail) > 400 {
			tail = tail[len(tail)-400:]
		}
		return fmt.Errorf("ffmpeg: %w\n%s", err, tail)
	}
	return nil
}

func ffprobeDur(f string) (float64, error) {
	out, err := exec.Command(ffTool("ffprobe"), "-v", "error",
		"-show_entries", "format=duration", "-of", "csv=p=0", f).Output()
	d := strings.TrimSpace(string(out))
	if err != nil || d == "" || d == "N/A" {
		// stream-to-disk recorders never finalize the header; decode and count
		out, err = exec.Command("bash", "-c",
			fmt.Sprintf(`ffmpeg -v error -progress /dev/stdout -i %q -f null - 2>/dev/null | awk -F= '/^out_time_us/ { t = $2 } END { printf "%%.2f", t / 1e6 }'`, f)).Output()
		if err != nil {
			return 0, fmt.Errorf("duration of %s: %w", f, err)
		}
		d = strings.TrimSpace(string(out))
	}
	var v float64
	fmt.Sscanf(d, "%f", &v)
	if v <= 0 {
		return 0, fmt.Errorf("cannot determine duration of %s", f)
	}
	return v, nil
}

// ffprobeSize is a still's pixel size. The thumbnail stage needs it because the
// title is drawn at a fraction of the picture's height, and the picture's
// height is the image server's decision, not ours: it is asked for 1280x720
// and a model with a fixed latent size may hand back something else.
func ffprobeSize(f string) (w, h int, err error) {
	out, err := exec.Command(ffTool("ffprobe"), "-v", "error", "-select_streams", "v:0",
		"-show_entries", "stream=width,height", "-of", "csv=p=0:s=x", f).Output()
	if err != nil {
		return 0, 0, fmt.Errorf("size of %s: %w", f, err)
	}
	if _, err := fmt.Sscanf(strings.TrimSpace(string(out)), "%dx%d", &w, &h); err != nil || w <= 0 || h <= 0 {
		return 0, 0, fmt.Errorf("cannot read the size of %s (ffprobe said %q)", f, strings.TrimSpace(string(out)))
	}
	return w, h, nil
}

// walk any decoded JSON, visiting every object -- survives the CLI and server
// wrapping payloads differently
func walkObjects(v any, fn func(map[string]any)) {
	switch t := v.(type) {
	case map[string]any:
		fn(t)
		for _, vv := range t {
			walkObjects(vv, fn)
		}
	case []any:
		for _, vv := range t {
			walkObjects(vv, fn)
		}
	}
}

func loadJSON(path string) (any, error) {
	b, err := os.ReadFile(path)
	if err != nil {
		return nil, err
	}
	var v any
	return v, json.Unmarshal(b, &v)
}

// spans from a diarization JSON: sample counts -> seconds
func loadSpans(path string) ([]span, error) {
	b, err := os.ReadFile(path)
	if err != nil {
		return nil, err
	}
	return spansFrom(b)
}

// spansFrom is the same for an answer that never reached disk -- which is what
// a window of the diarization scan is.
func spansFrom(b []byte) ([]span, error) {
	var v any
	if err := json.Unmarshal(b, &v); err != nil {
		return nil, err
	}
	var out []span
	walkObjects(v, func(m map[string]any) {
		slot, ok := m["speaker_id"].(string)
		if !ok {
			slot, ok = m["speaker"].(string)
		}
		ss, okS := m["start_sample"].(float64)
		es, okE := m["end_sample"].(float64)
		if ok && okS && okE {
			out = append(out, span{ss / sampleRate, es / sampleRate, slot})
		}
	})
	return out, nil
}

// ---- the transcripts and the frames -----------------------------------------

// ingest is the first half of Prepare: every source transcribed, and a
// frame out of the footage every few seconds. It is called by prepare
// (prep.go), which owns the goroutine, the run controls, the preflight on the
// server's models and the log lines for both halves -- this is only the work.

func (a *App) ingest(videos, audios []string, interval float64, scaleName, scaleVF string) error {
	inDir := a.inputsDir()
	if err := os.MkdirAll(inDir, 0o755); err != nil {
		return err
	}
	// progress plan: half the bar each. Speech recognition (GPU, server) and
	// frame extraction (CPU ffmpeg) do not contend, so they run as parallel
	// tracks; weighting by file count let one job take most of the bar and then
	// sit still. Each queues a task per file, and the speech side queues more as
	// it goes.
	inputs := append(append([]string{}, videos...), audios...)
	a.qJob(trackSTT, "speech", 0, 0)
	a.qPush(trackSTT, len(inputs), "recording")
	a.qJob(trackFrames, "frames", 0, 0)
	a.qPush(trackFrames, len(videos), "video")
	var unit, funit float64
	if len(inputs) > 0 {
		unit = 0.5 / float64(len(inputs))
	} else {
		a.qDone(trackSTT, 0.5) // a job with nothing to do is done, or its
	}
	if len(videos) > 0 {
		funit = 0.5 / float64(len(videos))
	} else {
		a.qDone(trackFrames, 0.5) // half would never fill and neither would the bar
	}

	var wg sync.WaitGroup
	var framesErr error
	wg.Add(1)
	go func() {
		defer wg.Done()
		if len(videos) == 0 {
			return
		}
		fb := 0.0
		for _, v := range videos {
			if framesErr = a.checkpoint(); framesErr != nil {
				return
			}
			a.qTake(trackFrames)
			if framesErr = a.extractFrames(v, interval, scaleName, scaleVF, inDir, fb, funit); framesErr != nil {
				return
			}
			fb += funit
		}
		a.qDone(trackFrames, fb)
	}()

	var sttErr error
	base := 0.0
	for _, in := range inputs {
		if sttErr = a.checkpoint(); sttErr != nil {
			break
		}
		a.qTake(trackSTT)
		if sttErr = a.transcribe(in, inDir, base, unit); sttErr != nil {
			if !errors.Is(sttErr, errStopped) {
				sttErr = fmt.Errorf("%s: %w", filepath.Base(in), sttErr)
			}
			break
		}
		base += unit
	}
	if sttErr == nil {
		a.qDone(trackSTT, base)
	}
	wg.Wait()

	// one stop is one stop, not two errors; otherwise report whatever failed
	if errors.Is(sttErr, errStopped) && errors.Is(framesErr, errStopped) {
		return errStopped
	}
	if err := errors.Join(sttErr, framesErr); err != nil {
		return err
	}

	// primary pair for the single-source consumers (review page, align); the
	// full ordered lists live in project.json. Either half can be absent now:
	// a session may be one screen recording that is both, or recordings with no
	// footage at all. So each is written only if there is one -- and what says
	// "the sources have been read" is that this file exists, not what is in it.
	var lines []string
	if len(videos) > 0 {
		lines = append(lines, "VIDEO_FILE="+videos[0], "VIDEO_BASE="+baseName(videos[0]))
	}
	// the narrator, not the first recording: that tag is who this session speaks
	// as, and untagged it falls back to exactly the file audios[0] used to be
	if voice := a.narratorSource(1); voice != "" {
		lines = append(lines, "AUDIO_FILE="+voice, "AUDIO_BASE="+baseName(voice))
	}
	lines = append(lines, fmt.Sprintf("INTERVAL=%g", interval), "SCALE="+scaleName)
	return os.WriteFile(filepath.Join(inDir, "meta.env"),
		[]byte(strings.Join(lines, "\n")+"\n"), 0o644)
}

// baseName is what a file is called with the folders and the extension taken
// off -- the name a source, a lane, a frame folder and a logo are all known by.
func baseName(p string) string {
	b := filepath.Base(p)
	return strings.TrimSuffix(b, filepath.Ext(b))
}

func exists(p string) bool { _, err := os.Stat(p); return err == nil }

// ---- one input: 16 kHz -> ASR -> diarization -> segments -------------------

func (a *App) transcribe(input, inDir string, base, unit float64) error {
	name := baseName(input)
	out := filepath.Join(inDir, name)
	if err := os.MkdirAll(out, 0o755); err != nil {
		return err
	}

	wav := filepath.Join(out, "voice16k.wav")
	if !exists(wav) {
		a.prog(trackSTT, base+0.01*unit, "extracting audio")
		if err := a.runCmd(ffTool("ffmpeg"), "-v", "error", "-y", "-i", input,
			"-vn", "-ac", "1", "-ar", "16000", "-c:a", "pcm_s16le", wav); err != nil {
			return err
		}
	}
	dur, err := ffprobeDur(wav)
	if err != nil {
		return err
	}
	a.logfIdle(">>> [%s] %.1f s of audio", name, dur)

	if err := a.checkpoint(); err != nil {
		return err
	}
	if !exists(filepath.Join(out, "words.json")) {
		a.prog(trackSTT, base+0.05*unit, "recognising speech")
		a.logfIdle(">>> [%s] ASR (%s)", name, a.readConf().ASRModel)
		body, text, err := a.asrLong(wav, dur, name, base, unit)
		if err != nil {
			return fmt.Errorf("ASR: %w", err)
		}
		if strings.TrimSpace(text) == "" {
			// a real case, not an error: a screen capture with no mic behind
			// it. The empty transcript is written and the step carries on
			a.logfIdle(">>> [%s] no speech found -- an empty transcript", name)
		}
		if err := os.WriteFile(filepath.Join(out, "transcript.txt"),
			[]byte(strings.TrimRight(text, "\n")+"\n"), 0o644); err != nil {
			return err
		}
		// words.json last, and whole: it is this stage's resume marker, so it
		// must not exist until the answer it stands for is on disk
		if err := os.WriteFile(filepath.Join(out, "words.json"), body, 0o644); err != nil {
			return err
		}
	} else {
		a.logfIdle(">>> [%s] ASR already done", name)
	}

	// ...and when it was said, which is a different question and a better
	// answer (align.go). It runs here because the segments below are built
	// from word times: an ASR that returns none -- the best of the three does
	// -- has no transcript at all without this.
	if models := a.alignModels(); len(models) > 0 {
		if !exists(alignedWords(out)) {
			a.prog(trackSTT, base+0.9*unit, "timing the words")
			a.logfIdle(">>> [%s] aligning (%s)", name, a.alignModel())
		}
		if err := a.alignInput(out, wav, models); err != nil {
			// what a failure costs depends on what the ASR gave. With its own
			// word times, alignment is an improvement and losing it is a
			// warning: the times stand where they stood before this existed.
			//
			// With none -- which is what the best transcriber answers -- it is
			// the only source of times there is, and the transcript below is
			// built from times. Failing here with the aligner's own words beats
			// failing two steps later with a sentence about a missing aligner
			// that is plainly registered.
			if len(wordTimes(out)) > 0 {
				a.logfIdle("!!! [%s] align: %v -- the ASR's own times stand", name, err)
			} else if strings.TrimSpace(readFileString(filepath.Join(out, "transcript.txt"))) != "" {
				return fmt.Errorf("align: %w\n\n%s answers with no word times of its own, so the "+
					"aligner is the only thing that can time them and this recording cannot be "+
					"transcribed without it", err, a.readConf().ASRModel)
			}
		}
	}

	if err := a.checkpoint(); err != nil {
		return err
	}
	if !exists(filepath.Join(out, "turns.json")) {
		if err := a.diarize(out, dur, name, base, unit); err != nil {
			if errors.Is(err, errStopped) {
				return err
			}
			return fmt.Errorf("diarization: %w", err)
		}
	} else {
		a.logfIdle(">>> [%s] diarization already done", name)
	}

	a.prog(trackSTT, base+0.98*unit, "building segments")
	if err := a.mergeSegments(out); err != nil {
		return fmt.Errorf("merge: %w", err)
	}
	return nil
}

// asrLong is one recording through the ASR in as many requests as its length
// needs. A recording that fits goes in whole and words.json stays the server's
// own document; a long one is cut into pieces, stitched back into one answer
// of the same shape. The cuts slide to the middle of a silence, where a
// decoder losing its context costs nothing.
func (a *App) asrLong(wav string, dur float64, name string, base, unit float64) ([]byte, string, error) {
	limit := a.asrChunk()
	if dur <= limit {
		return a.asrJSON(wav)
	}
	seek := math.Min(asrCutSeek, limit/3)
	edges := append(append([]float64{0}, asrCuts(dur, a.quietSpots(wav), limit, seek)...), dur)
	n := len(edges) - 1
	a.logfIdle(">>> [%s] too long for one request -- %d ASR chunks", name, n)

	dir := filepath.Join(filepath.Dir(wav), "asr")
	os.RemoveAll(dir)
	if err := os.MkdirAll(dir, 0o755); err != nil {
		return nil, "", err
	}
	// the chunks are scratch and go when the answer is stitched -- but only
	// then. A failed run keeps them, because the one thing worth having after
	// "the server could not open c00.wav" is c00.wav.
	done := false
	defer func() {
		if done {
			os.RemoveAll(dir)
		}
	}()

	var words []any
	var texts []string
	for i := 0; i < n; i++ {
		if err := a.checkpoint(); err != nil {
			return nil, "", err
		}
		a.prog(trackSTT, base+(0.05+0.45*float64(i)/float64(n))*unit,
			"recognising speech %d/%d", i+1, n)
		part := filepath.Join(dir, fmt.Sprintf("c%02d.wav", i))
		// -ss ahead of -i seeks the input, which on the pcm this stage wrote
		// is exact rather than a keyframe away
		if err := a.runCmd(ffTool("ffmpeg"), "-v", "error", "-y",
			"-ss", fmt.Sprint(edges[i]), "-t", fmt.Sprint(edges[i+1]-edges[i]),
			"-i", wav, "-c:a", "pcm_s16le", part); err != nil {
			return nil, "", err
		}
		body, text, err := a.asrJSON(part)
		if err != nil {
			return nil, "", err
		}
		words = append(words, shiftWords(body, edges[i])...)
		if t := strings.TrimSpace(text); t != "" {
			texts = append(texts, t)
		}
	}
	text := strings.Join(texts, " ")
	b, err := json.MarshalIndent(map[string]any{"text": text, "words": words}, "", "  ")
	if err != nil {
		return nil, "", err
	}
	done = true
	return append(b, '\n'), text, nil
}

// asrChunk is how much audio one ASR request may hold, which is not one number
// for every model.
//
// The encoders differ in what they accept, and on a shared GPU in what they can
// GET: Qwen3-ASR allocates a classification graph sized by the clip in front of
// it -- about 1.5 GB per 20 s, measured here -- so a 96 s take asks for seven
// gigabytes in one piece and dies with "failed to allocate Qwen3 ASR thinker
// classification graph", on a machine where Nemotron reads the same take whole.
// The model that will do the reading decides how much it is handed.
func (a *App) asrChunk() float64 {
	c := a.readConf()
	if cat, err := audioCatalog(a.audioURL(), c.TTSKey); err == nil {
		if strings.HasPrefix(cat[c.ASRModel].Family, "qwen3") {
			return asrChunkQwen
		}
	}
	return asrChunkMax
}

// asrCuts divides dur into pieces and returns the times between them. Even
// pieces, not full ones with a runt. Each cut slides up to seek seconds to the
// nearest silence, and the pieces are sized so two cuts sliding apart cannot
// push the piece between them past max.
func asrCuts(dur float64, quiet []span, max, seek float64) []float64 {
	if dur <= max || max <= 0 {
		return nil
	}
	if seek < 0 || 2*seek >= max {
		seek = 0
	}
	n := int(math.Ceil(dur / (max - 2*seek)))
	step := dur / float64(n)
	if lim := step / 3; seek > lim {
		seek = lim // cuts stay in order, and each stays inside its own piece
	}
	var out []float64
	for i := 1; i < n; i++ {
		want, at, best := step*float64(i), step*float64(i), seek
		for _, q := range quiet {
			if mid := (q.s + q.e) / 2; math.Abs(mid-want) < best {
				at, best = mid, math.Abs(mid-want)
			}
		}
		out = append(out, at)
	}
	return out
}

// shiftWords moves one chunk's words to where the chunk was in the recording,
// rewriting only the two sample counts so whatever else a word carries rides
// along. A word whose times cannot be read travels unshifted -- the merge is
// what has to notice an unknown shape.
func shiftWords(body []byte, off float64) []any {
	var v any
	if json.Unmarshal(body, &v) != nil {
		return nil
	}
	var out []any
	walkObjects(v, func(m map[string]any) {
		if _, ok := m["word"].(string); !ok {
			return
		}
		for _, k := range []string{"start_sample", "end_sample"} {
			if n, ok := m[k].(float64); ok {
				m[k] = n + off*sampleRate
			}
		}
		out = append(out, m)
	})
	return out
}

// quietSpots asks ffmpeg where the recording goes quiet. Best effort: finding
// none, or ffmpeg refusing the file, means the cuts land on the clock, which
// is a worse place to cut but never a reason to fail the step.
func (a *App) quietSpots(wav string) []span {
	cmd := exec.Command(ffTool("ffmpeg"), "-v", "info", "-nostats", "-i", wav,
		"-af", fmt.Sprintf("silencedetect=n=%ddB:d=%g", asrQuietDB, asrQuietMin),
		"-f", "null", "-")
	var buf bytes.Buffer
	cmd.Stdout, cmd.Stderr = &buf, &buf
	a.ctlMu.Lock()
	a.curCmds[cmd] = true
	a.ctlMu.Unlock()
	err := cmd.Run()
	a.ctlMu.Lock()
	delete(a.curCmds, cmd)
	a.ctlMu.Unlock()
	if err != nil {
		return nil
	}
	return parseSilence(buf.String())
}

// parseSilence reads the report silencedetect writes to stderr:
//
//	[silencedetect @ 0x..] silence_start: 12.345
//	[silencedetect @ 0x..] silence_end: 15.678 | silence_duration: 3.333
//
// A silence still open when the file ends has no end line and is dropped: the
// end of the recording is not a place anything needs to be cut.
func parseSilence(report string) []span {
	var out []span
	from, open := 0.0, false
	for _, ln := range strings.Split(report, "\n") {
		if i := strings.Index(ln, "silence_start:"); i >= 0 {
			v, err := strconv.ParseFloat(strings.TrimSpace(ln[i+len("silence_start:"):]), 64)
			from, open = v, err == nil
			continue
		}
		i := strings.Index(ln, "silence_end:")
		if i < 0 || !open {
			continue
		}
		f := strings.TrimSpace(ln[i+len("silence_end:"):])
		if j := strings.Index(f, "|"); j >= 0 {
			f = strings.TrimSpace(f[:j])
		}
		if v, err := strconv.ParseFloat(f, 64); err == nil && v > from {
			out = append(out, span{s: from, e: v})
		}
		open = false
	}
	return out
}

// concatLine is one row of an ffmpeg concat list. The quoting is the reason it
// is a function: the demuxer reads 'file' arguments as single-quoted, so a path
// with an apostrophe in it -- a project called "tom's cut", which names the
// whole data folder -- ends the quote early and the rest of the list is read as
// something else. ffmpeg spells an escaped quote the shell way: close, a
// backslashed quote, open again.
func concatLine(path string) string {
	return "file '" + strings.ReplaceAll(path, "'", `'\''`) + "'\n"
}

func (a *App) diarize(out string, dur float64, name string, base, unit float64) error {
	dir := filepath.Join(out, "diar")
	os.RemoveAll(dir)
	if err := os.MkdirAll(dir, 0o755); err != nil {
		return err
	}
	defer os.RemoveAll(dir)
	turnsPath := filepath.Join(out, "turns.json")

	// -- pass 1: where are the voices? --------------------------------------
	nwin := int(math.Ceil(dur / diarScanHop))
	scan := map[int][]span{}
	for i := 0; i < nwin; i++ {
		if err := a.checkpoint(); err != nil {
			return err
		}
		a.prog(trackSTT, base+(0.55+0.20*float64(i)/float64(nwin))*unit,
			"finding voices %d/%d", i+1, nwin)
		start := float64(i) * diarScanHop
		if err := a.runCmd(ffTool("ffmpeg"), "-v", "error", "-y",
			"-ss", fmt.Sprint(start), "-t", fmt.Sprint(diarWin),
			"-i", filepath.Join(out, "voice16k.wav"),
			"-c:a", "pcm_s16le", filepath.Join(dir, "s.wav")); err != nil {
			return err
		}
		spans, err := a.diarSpans(filepath.Join(dir, "s.wav"))
		if err != nil {
			return err
		}
		for _, sp := range spans {
			scan[i] = append(scan[i], span{sp.s + start, sp.e + start, sp.slot})
		}
	}
	if len(scan) == 0 {
		return os.WriteFile(turnsPath, []byte("[]\n"), 0o644)
	}

	// -- pick the anchor window ---------------------------------------------
	// Most distinct voices; on a tie, the window whose QUIETEST voice is best
	// represented -- ranking by total speech picks whoever talks most and
	// leaves the other voice too thin to anchor.
	best, bestN, bestLo := -1, 0, 0.0
	for w, spans := range scan {
		durBy := map[string]float64{}
		for _, sp := range spans {
			durBy[sp.slot] += sp.e - sp.s
		}
		n, lo := 0, math.MaxFloat64
		for _, d := range durBy {
			if d >= anchorMin {
				n++
				if d < lo {
					lo = d
				}
			}
		}
		if n > bestN || (n == bestN && n > 0 && lo > bestLo) {
			best, bestN, bestLo = w, n, lo
		}
	}
	if best < 0 {
		for w := range scan {
			if best < 0 || w < best {
				best = w
			}
		}
	}

	// -- build the anchor: up to anchorPer seconds of each voice ------------
	rows := append([]span(nil), scan[best]...)
	sort.Slice(rows, func(i, j int) bool {
		if rows[i].slot != rows[j].slot {
			return rows[i].slot < rows[j].slot
		}
		return rows[i].s < rows[j].s
	})
	type piece struct {
		src, len float64
		slot     string
	}
	var pieces []piece
	acc := map[string]float64{}
	for _, r := range rows {
		if acc[r.slot] >= anchorPer {
			continue
		}
		d := r.e - r.s
		if d > anchorPer-acc[r.slot] {
			d = anchorPer - acc[r.slot]
		}
		if d < 0.3 { // too short to carry a voice
			continue
		}
		acc[r.slot] += d
		pieces = append(pieces, piece{r.s, d, r.slot})
	}
	var list strings.Builder
	for i, p := range pieces {
		f := filepath.Join(dir, fmt.Sprintf("a%02d.wav", i))
		if err := a.runCmd(ffTool("ffmpeg"), "-v", "error", "-y",
			"-ss", fmt.Sprint(p.src), "-t", fmt.Sprint(p.len),
			"-i", filepath.Join(out, "voice16k.wav"),
			"-c:a", "pcm_s16le", f); err != nil {
			return err
		}
		list.WriteString(concatLine(f))
	}
	if err := os.WriteFile(filepath.Join(dir, "anchor.list"), []byte(list.String()), 0o644); err != nil {
		return err
	}
	if err := a.runCmd(ffTool("ffmpeg"), "-v", "error", "-y", "-f", "concat", "-safe", "0",
		"-i", filepath.Join(dir, "anchor.list"),
		"-c:a", "pcm_s16le", filepath.Join(dir, "anchor.wav")); err != nil {
		return err
	}
	// block b of the anchor is voice b: [bs, be) in anchor-local seconds
	type block struct{ s, e float64 }
	var blocks []block
	t := 0.0
	for i, p := range pieces {
		if i == 0 || p.slot != pieces[i-1].slot {
			blocks = append(blocks, block{t, t})
		}
		t += p.len
		blocks[len(blocks)-1].e = t
	}
	alen := t
	hop := math.Floor(diarWin - alen - 1)
	if hop < 15 {
		return fmt.Errorf("anchor too long (%.1f s of %.0f s window)", alen, diarWin)
	}
	a.logfIdle(">>> [%s] anchor: %d voice(s) in %.1f s, from window %d -- %.0f s of new audio per window",
		name, len(blocks), alen, best, hop)

	// -- pass 2: every window carries the anchor ----------------------------
	nwin = int(math.Ceil(dur / hop))
	all := map[int][]span{}
	for i := 0; i < nwin; i++ {
		if err := a.checkpoint(); err != nil {
			return err
		}
		a.prog(trackSTT, base+(0.75+0.22*float64(i)/float64(nwin))*unit,
			"placing speakers %d/%d", i+1, nwin)
		start := float64(i) * hop
		if err := a.runCmd(ffTool("ffmpeg"), "-v", "error", "-y",
			"-ss", fmt.Sprint(start), "-t", fmt.Sprint(hop),
			"-i", filepath.Join(out, "voice16k.wav"),
			"-c:a", "pcm_s16le", filepath.Join(dir, "seg.wav")); err != nil {
			return err
		}
		cc := concatLine(filepath.Join(dir, "anchor.wav")) +
			concatLine(filepath.Join(dir, "seg.wav"))
		if err := os.WriteFile(filepath.Join(dir, "cc.list"), []byte(cc), 0o644); err != nil {
			return err
		}
		if err := a.runCmd(ffTool("ffmpeg"), "-v", "error", "-y", "-f", "concat", "-safe", "0",
			"-i", filepath.Join(dir, "cc.list"),
			"-c:a", "pcm_s16le", filepath.Join(dir, "win.wav")); err != nil {
			return err
		}
		spans, err := a.diarSpans(filepath.Join(dir, "win.wav"))
		if err != nil {
			return err
		}
		all[i] = spans
	}

	// -- resolve slots against the anchor, one-to-one per window ------------
	// Strongest claim first, each block claimed once: independent argmax per
	// slot would let two slots claim the same voice. A slot silent throughout
	// the anchor is a voice the anchor does not carry -- it gets its own id
	// rather than a guess, and surfaces as an extra speaker.
	type gspan struct {
		s, e float64
		g    int
	}
	var glob []gspan
	nunk := 0
	for w := 0; w < nwin; w++ {
		spans := all[w]
		if len(spans) == 0 {
			continue
		}
		var slots []string
		ov := map[string][]float64{}
		for _, sp := range spans {
			if _, seen := ov[sp.slot]; !seen {
				ov[sp.slot] = make([]float64, len(blocks))
				slots = append(slots, sp.slot)
			}
			for b, bl := range blocks {
				o := math.Min(sp.e, bl.e) - math.Max(sp.s, bl.s)
				if o > 0 {
					ov[sp.slot][b] += o
				}
			}
		}
		gid := map[string]int{}
		tookSlot := map[string]bool{}
		tookBlock := make([]bool, len(blocks))
		for {
			bo, bs, bb := minAnchorOv, "", -1
			for _, sl := range slots {
				if tookSlot[sl] {
					continue
				}
				for b := range blocks {
					if !tookBlock[b] {
						if ov[sl][b] > bo {
							bo, bs, bb = ov[sl][b], sl, b
						}
					}
				}
			}
			if bb < 0 {
				break
			}
			gid[bs] = bb
			tookSlot[bs] = true
			tookBlock[bb] = true
		}
		for _, sl := range slots {
			if !tookSlot[sl] {
				gid[sl] = len(blocks) + nunk
				nunk++
			}
		}
		for _, sp := range spans {
			if sp.s < alen { // the anchor itself, not content
				continue
			}
			glob = append(glob, gspan{
				float64(w)*hop + sp.s - alen,
				float64(w)*hop + sp.e - alen,
				gid[sp.slot]})
		}
	}
	sort.Slice(glob, func(i, j int) bool { return glob[i].s < glob[j].s })

	// glue same-speaker runs: sortformer reports frame-level bursts
	var outSpans []gspan
	for _, g := range glob {
		n := len(outSpans)
		if n > 0 && outSpans[n-1].g == g.g && g.s-outSpans[n-1].e <= diarTurnGap {
			if g.e > outSpans[n-1].e {
				outSpans[n-1].e = g.e
			}
			continue
		}
		outSpans = append(outSpans, g)
	}
	var sb strings.Builder
	sb.WriteString("[")
	for i, g := range outSpans {
		if i > 0 {
			sb.WriteString(",")
		}
		fmt.Fprintf(&sb, `{"start_sample":%d,"end_sample":%d,"speaker_id":"SPEAKER_%02d"}`,
			int64(g.s*sampleRate), int64(g.e*sampleRate), g.g)
	}
	sb.WriteString("]\n")
	unident := ""
	if nunk > 0 {
		unident = fmt.Sprintf(" (%d slot(s) the anchor could not identify)", nunk)
	}
	a.logfIdle(">>> [%s] %d speaker(s), %d turns%s", name, len(blocks)+nunk, len(outSpans), unident)
	return os.WriteFile(turnsPath, []byte(sb.String()), 0o644)
}

// ---- words + turns -> speaker-tagged segments ------------------------------

func (a *App) mergeSegments(out string) error {
	type word struct {
		s, e float64
		w    string
	}
	var words []word
	seen := 0
	for _, w := range wordTimes(out) {
		seen++
		if w.End > w.Start {
			words = append(words, word{float64(w.Start) / sampleRate, float64(w.End) / sampleRate, w.Word})
		}
	}
	// no words at all is silence, which becomes an empty transcript below; words
	// whose times cannot be read is an answer changing shape, which has to stop
	// here rather than quietly emptying every transcript after it
	if len(words) == 0 && seen > 0 {
		return fmt.Errorf("%d words carry no usable start_sample/end_sample -- the answer changed shape", seen)
	}
	// ...and no words where there IS speech is the case this used to make
	// impossible: an ASR that answers with text alone (Qwen3-ASR) and no
	// aligner registered to time it. Said plainly, because every transcript
	// after it would otherwise come out empty for no visible reason.
	if len(words) == 0 {
		if b, err := os.ReadFile(filepath.Join(out, "transcript.txt")); err == nil && strings.TrimSpace(string(b)) != "" {
			how := "no aligner is registered to time them -- register one (task \"align\")"
			if len(a.alignModels()) > 0 {
				how = "and the aligner left no times either, which the line above this one says why"
			}
			return fmt.Errorf("%s transcribed this recording but timed no words, %s -- or use an "+
				"ASR that answers with word timings", a.readConf().ASRModel, how)
		}
	}

	turns, _ := loadSpans(filepath.Join(out, "turns.json"))
	// glue same-speaker turns (idempotent over what diarize wrote)
	var ts []span
	for _, t := range turns {
		n := len(ts)
		if n > 0 && ts[n-1].slot == t.slot && t.s-ts[n-1].e <= diarTurnGap {
			if t.e > ts[n-1].e {
				ts[n-1].e = t.e
			}
			continue
		}
		ts = append(ts, t)
	}

	// speaker whose turn shares the most time with the word; else the nearest
	// turn within mergeNear -- diarization edges are not exact
	who := func(ws, we float64) string {
		best, bi := 0.0, -1
		for i, t := range ts {
			o := math.Min(we, t.e) - math.Max(ws, t.s)
			if o > best {
				best, bi = o, i
			}
		}
		if bi >= 0 {
			return ts[bi].slot
		}
		bd := mergeNear
		for i, t := range ts {
			d := 0.0
			if t.s > we {
				d = t.s - we
			} else if t.e < ws {
				d = ws - t.e
			}
			if d < bd {
				bd, bi = d, i
			}
		}
		if bi >= 0 {
			return ts[bi].slot
		}
		return "?"
	}

	type seg struct {
		s, e float64
		spk  string
		text string
	}
	var segs []seg
	var cur *seg
	prev := 0.0
	for _, w := range words {
		ws := w.s
		we := math.Min(w.e, ws+mergeMaxWord) // see mergeMaxWord
		spk := who(ws, we)
		if spk == "?" && cur != nil {
			spk = cur.spk // unknown keeps the running speaker
		}
		if cur != nil && (spk != cur.spk || ws-prev > mergeGap || we-cur.s > mergeMaxLen) {
			segs = append(segs, *cur)
			cur = nil
		}
		if cur == nil {
			cur = &seg{s: ws, spk: spk, text: w.w}
		} else {
			if cur.spk == "?" && spk != "?" {
				cur.spk = spk // upgrade once identified
			}
			cur.text += " " + w.w
		}
		cur.e = we
		prev = we
	}
	if cur != nil {
		segs = append(segs, *cur)
	}

	srt := func(t float64) string {
		if t < 0 {
			t = 0
		}
		h := int(t) / 3600
		m := (int(t) % 3600) / 60
		s := int(t) % 60
		ms := int((t-math.Floor(t))*1000 + 0.5)
		return fmt.Sprintf("%02d:%02d:%02d,%03d", h, m, s, ms)
	}
	var tsv, srtb strings.Builder
	for i, g := range segs {
		fmt.Fprintf(&tsv, "%.2f\t%.2f\t%s\t%s\n", g.s, g.e, g.spk, g.text)
		tag := ""
		if g.spk != "?" {
			tag = "[" + g.spk + "] "
		}
		fmt.Fprintf(&srtb, "%d\n%s --> %s\n%s%s\n\n", i+1, srt(g.s), srt(g.e), tag, g.text)
	}
	if err := os.WriteFile(filepath.Join(out, "transcript.tsv"), []byte(tsv.String()), 0o644); err != nil {
		return err
	}
	if err := os.WriteFile(filepath.Join(out, "transcript.srt"), []byte(srtb.String()), 0o644); err != nil {
		return err
	}
	a.logfIdle(">>> [%s] %d segments", filepath.Base(out), len(segs))
	return nil
}

// ---- frames ----------------------------------------------------------------

// extractFrames dumps one video's frames into inputs/frames/<basename>/.
// A marker file records interval + size, so re-runs skip until either changes.
func (a *App) extractFrames(video string, interval float64, scaleName, scaleVF, inDir string, base, unit float64) error {
	name := baseName(video)
	fdir := filepath.Join(inDir, "frames", name)
	marker := filepath.Join(fdir, ".interval")
	want := fmt.Sprintf("%g|%s", interval, scaleName)
	if b, err := os.ReadFile(marker); err == nil && strings.TrimSpace(string(b)) == want {
		// the pixels are already right; only a folder extracted before frames
		// were named for their second has anything left to do, and that is a
		// rename rather than the minutes of decoding a re-extract would cost
		if n, err := stampFrames(fdir, video, a.sourceStart(video), interval); err != nil {
			return err
		} else if n > 0 {
			a.logfIdle(">>> [%s] %d frames renamed to the second they were shot in", name, n)
		}
		a.logfIdle(">>> [%s] frames already extracted (%gs, %s), skipping", name, interval, scaleName)
		a.prog(trackFrames, base+unit, "already extracted")
		return nil
	}
	if interval == 0 {
		a.logfIdle(">>> [%s] extracting EVERY frame -- gigabytes", name)
	} else {
		a.logfIdle(">>> [%s] extracting a frame every %gs at %s", name, interval, scaleName)
	}
	os.RemoveAll(fdir)
	if err := os.MkdirAll(fdir, 0o755); err != nil {
		return err
	}
	var filters []string
	if interval > 0 {
		filters = append(filters, fmt.Sprintf("fps=%f", 1/interval))
	}
	if scaleVF != "" {
		filters = append(filters, scaleVF)
	}
	vf := strings.Join(filters, ",")
	pattern := filepath.Join(fdir, "f%06d.jpg")
	dur, err := ffprobeDur(video)
	if err != nil {
		return err
	}

	// Extraction is decode-bound: to keep one frame per second the decoder
	// still chews through every source frame. Splitting the timeline into
	// chunks and running one ffmpeg per chunk spreads that over the cores;
	// chunk lengths are multiples of the interval so the global mapping
	// frame n <-> t=(n-1)*interval stays exact across chunk borders.
	workers := runtime.NumCPU() / 4
	workers = max(2, min(8, workers))
	totalFrames := 0
	if interval > 0 {
		totalFrames = int(math.Ceil(dur / interval))
		if totalFrames < workers*4 {
			workers = 1
		}
	} else {
		workers = 1 // every-frame mode: frame count per chunk is not knowable
	}

	if workers == 1 {
		args := []string{"-v", "error", "-y", "-i", video}
		if vf != "" { // "-vf" with an empty graph is an ffmpeg error
			args = append(args, "-vf", vf)
		}
		args = append(args, "-q:v", "4", "-start_number", "1", pattern)
		if err := a.ffmpegProgress(dur, func(f float64) {
			a.prog(trackFrames, base+f*unit, "extracting %.0f%%", f*100)
		}, args...); err != nil {
			return err
		}
	} else {
		chunkFrames := (totalFrames + workers - 1) / workers
		chunkDur := float64(chunkFrames) * interval
		var mu sync.Mutex
		fracs := make([]float64, workers)
		report := func() {
			mu.Lock()
			sum := 0.0
			for _, f := range fracs {
				sum += f
			}
			mu.Unlock()
			a.prog(trackFrames, base+sum*unit, "extracting %.0f%%", sum*100)
		}
		var wg sync.WaitGroup
		errs := make([]error, workers)
		for k := 0; k < workers; k++ {
			n := chunkFrames
			if k == workers-1 {
				n = totalFrames - k*chunkFrames
			}
			if n <= 0 {
				continue
			}
			wg.Add(1)
			go func(k, n int) {
				defer wg.Done()
				weight := float64(n) / float64(totalFrames)
				errs[k] = a.ffmpegProgress(chunkDur, func(f float64) {
					mu.Lock()
					fracs[k] = math.Min(1, f) * weight
					mu.Unlock()
					report()
				}, "-v", "error", "-y",
					"-ss", fmt.Sprintf("%f", float64(k)*chunkDur),
					"-t", fmt.Sprintf("%f", chunkDur),
					"-i", video, "-vf", vf, "-q:v", "4",
					"-start_number", fmt.Sprint(k*chunkFrames+1),
					"-frames:v", fmt.Sprint(n), pattern)
			}(k, n)
		}
		wg.Wait()
		stopped := false
		for _, e := range errs {
			if errors.Is(e, errStopped) {
				stopped = true
			} else if e != nil {
				return e
			}
		}
		if stopped {
			return errStopped
		}
	}
	if _, err := stampFrames(fdir, video, a.sourceStart(video), interval); err != nil {
		return err
	}
	if err := os.WriteFile(marker, []byte(want+"\n"), 0o644); err != nil {
		return err
	}
	ents, _ := os.ReadDir(fdir)
	a.logfIdle(">>> [%s] %d frames extracted", name, len(ents)-1)
	return nil
}

// stampFrames renames ffmpeg's numbering into the wall-clock second each frame
// was shot in (several in one second get -1, -2). The name comes from the
// frame NUMBER, never the sorted position, so t = (n-1) * interval survives a
// gap. start is the session's placement of this video, passed in by the caller
// who holds the whole source list. Nothing numbered left means nothing to do,
// which is also what renames a folder extracted before stamping existed.
func stampFrames(fdir, video string, start, interval float64) (int, error) {
	ents, err := os.ReadDir(fdir)
	if err != nil {
		return 0, err
	}
	type frame struct {
		n    int
		name string
	}
	var fs []frame
	for _, e := range ents {
		n, ok := frameNum(e.Name())
		if e.IsDir() || !ok {
			continue
		}
		fs = append(fs, frame{n, e.Name()})
	}
	if len(fs) == 0 {
		return 0, nil
	}
	if interval <= 0 {
		// every-frame mode: the spacing is the video's own. Without a frame rate
		// there is no time to name a frame after, so it keeps its number.
		fps := ffprobeFPS(video)
		if fps <= 0 {
			return 0, nil
		}
		interval = 1 / fps
	}
	sort.Slice(fs, func(i, j int) bool { return fs[i].n < fs[j].n })
	var seq stampSeq
	for _, f := range fs {
		to := seq.name(start+float64(f.n-1)*interval, ".jpg")
		if err := os.Rename(filepath.Join(fdir, f.name), filepath.Join(fdir, to)); err != nil {
			return 0, err
		}
	}
	return len(fs), nil
}

// frameNum reads ffmpeg's f000001.jpg back. Frames are renamed the moment they
// are extracted, so this only ever sees a folder mid-extraction -- or one made
// before the frames carried their timestamp.
func frameNum(name string) (int, bool) {
	if !strings.HasPrefix(name, "f") || !strings.HasSuffix(name, ".jpg") {
		return 0, false
	}
	n, err := strconv.Atoi(strings.TrimSuffix(name[1:], ".jpg"))
	return n, err == nil && n >= 1
}
