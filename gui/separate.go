package main

// Splitting the voice off a recording: the audio server decomposes a mixture
// into the voice AND everything else (the stems add back up). The result is
// two ordinary files and the session's row becomes two ordinary rows -- itself
// without the voice, and the voice -- so no reader downstream is taught about
// stems. A flag on the row, not a button: the work is minutes of GPU, spent
// by ▶.

import (
	"encoding/base64"
	"encoding/json"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"

	"github.com/diamondburned/gotk4/pkg/glib/v2"
)

const (
	// the same shape as the ASR chunking, and for the same reason: one request
	// per five minutes rather than one per recording. The answer here is the
	// audio itself, base64 in a JSON body -- a whole session in one reply is
	// hundreds of megabytes of it -- so the ceiling matters more, not less.
	sepChunkMax = 300.0
	sepCutSeek  = 20.0
	// what goes up. The separation models are music models: 44.1 kHz stereo is
	// what they were trained on, and handing them the 16 kHz mono the ASR uses
	// would be asking them to separate something they have never heard.
	sepRate     = "44100"
	sepChannels = "2"
)

// sepDir is where the halves are kept. Beside the steps rather than inside
// one: they are sources now, and a source has to survive the step that made it
// being re-run.
func (a *App) sepDir() string { return filepath.Join(a.outDir, "stems") }

// sepNames are the two files a source becomes. The name keeps the original's,
// timestamp and all, because that stamp is how every source is placed on the
// session clock -- a split recording has to land exactly where it did.
//
// The voiceless half of a video is matroska: it carries the original picture
// stream copied, untouched, beside an audio codec of its own choosing, so
// giving a recording a new soundtrack never costs it a re-encode.
func sepNames(dir, src string) (rest, voice string) {
	base := baseName(src)
	rest = filepath.Join(dir, base+".split-novoice.wav")
	if isVideo(src) {
		rest = filepath.Join(dir, base+".split-novoice.mkv")
	}
	return rest, filepath.Join(dir, base+".split-voice.wav")
}

// splitProduct is whether a path is one half of a split: the name says so,
// and the name is the only thing a stem carries (see the file header). It is
// what keeps the scissors off a row that is already a half -- splitting the
// voice off a voice is a wish that produces x.split-voice.split-voice.wav.
// The old spellings are read too, for a project split before they changed.
func splitProduct(path string) bool {
	b := baseName(path)
	for _, s := range []string{".split-novoice", ".split-voice", ".novoice", ".voice"} {
		if strings.HasSuffix(b, s) {
			return true
		}
	}
	return false
}

// sepResult is one granted wish: the row that asked, and the two files it is
// now.
type sepResult struct{ src, rest, voice string }

// sepApply rewrites a source list for what the separation produced: the row
// points at itself-without-voice and the voice follows as its own row. Pure,
// and the one place the rule lives (run snapshot and page list both go through
// it). The voice inherits the narrator tag; the wish is cleared by granting it.
func sepApply(items []sourceItem, res []sepResult) []sourceItem {
	by := map[string]sepResult{}
	for _, r := range res {
		by[r.src] = r
	}
	out := make([]sourceItem, 0, len(items)+len(res))
	for _, it := range items {
		r, ok := by[it.path]
		if !ok {
			out = append(out, it)
			continue
		}
		out = append(out,
			sourceItem{path: r.rest, footage: it.footage},
			sourceItem{path: r.voice, narrator: it.narrator})
	}
	return out
}

// ---- what the server sends back ---------------------------------------------

// sepStem is one stem of the answer: a whole wav, base64, under the name the
// model gave it.
type sepStem struct {
	ID    string `json:"id"`
	Audio string `json:"audio"`
}

// sepStems reads the stems out of a separation answer. The field is the
// server's own named_audio_outputs, which is how it returns any task with more
// than one sound in the result.
func sepStems(body []byte) ([]sepStem, error) {
	var out struct {
		Stems []sepStem `json:"named_audio_outputs"`
	}
	if err := json.Unmarshal(body, &out); err != nil {
		return nil, fmt.Errorf("unreadable answer: %w", err)
	}
	if len(out.Stems) == 0 {
		return nil, fmt.Errorf("no stems in the answer: %.200s", strings.TrimSpace(string(body)))
	}
	return out.Stems, nil
}

// sepPick decides which stem is the voice and which are everything else.
// RoFormers give "vocals" and "instrumental"; HTDemucs gives four instruments,
// where the rest is the other three. Both required: a model that gave neither
// fails here, not at the render.
func sepPick(ids []string) (voice string, rest []string, err error) {
	for _, id := range ids {
		if strings.EqualFold(id, "vocals") || strings.EqualFold(id, "voice") {
			voice = id
			break
		}
	}
	if voice == "" {
		return "", nil, fmt.Errorf("stems %s -- none of them is the voice",
			strings.Join(ids, ", "))
	}
	for _, id := range ids {
		if strings.EqualFold(id, "instrumental") {
			return voice, []string{id}, nil
		}
	}
	for _, id := range ids {
		if id != voice {
			rest = append(rest, id)
		}
	}
	if len(rest) == 0 {
		return "", nil, fmt.Errorf("only %q came back -- the recording without the voice "+
			"is the other half of this, and there is no other half", voice)
	}
	return voice, rest, nil
}

// ---- the run ----------------------------------------------------------------

// separateVoices grants the session's split wishes and hands back the source
// lists as they are once it has. It runs before anything else on ▶: its files
// are what the frames and transcripts come from. The snapshot is rewritten in
// place; the page is told separately.
func (a *App) separateVoices(videos, audios []string) ([]string, []string, error) {
	want := a.sepWanted()
	if len(want) == 0 {
		return videos, audios, nil
	}
	a.qJob(trackSTT, "voice split", 0, 0)
	a.qPush(trackSTT, len(want), "recording")
	a.qDone(trackFrames, 0.5) // one job in this phase, and it is not this half's

	unit := 0.5 / float64(len(want))
	var res []sepResult
	for i, src := range want {
		if err := a.checkpoint(); err != nil {
			return nil, nil, err
		}
		a.qTake(trackSTT)
		r, err := a.separateOne(src, float64(i)*unit, unit)
		if err != nil {
			return nil, nil, fmt.Errorf("splitting the voice off %s: %w", filepath.Base(src), err)
		}
		res = append(res, r)
	}
	a.qDone(trackSTT, 0.5)

	items := sepApply(a.snappedItems(), res)
	vids, auds := a.snapItems(items)
	// the page catches up when it can, and saves: the session is two rows
	// bigger than the project on disk says it is until it does
	glib.IdleAdd(func() {
		if a.srcList == nil {
			return
		}
		a.srcList.items = sepApply(a.srcList.items, res)
		a.srcList.changed()
		a.saveProjectNow()
	})
	return vids, auds, nil
}

// separateOne is one recording. Resumable like every other minute this step
// spends: both halves already on disk is the work already done, and a re-run
// after a stop picks up at the recording it did not reach.
func (a *App) separateOne(src string, base, unit float64) (sepResult, error) {
	dir := a.sepDir()
	rest, voice := sepNames(dir, src)
	name := filepath.Base(src)
	if exists(rest) && exists(voice) {
		a.logfIdle(">>> [%s] already split", baseName(src))
		return sepResult{src, rest, voice}, nil
	}
	if err := os.MkdirAll(dir, 0o755); err != nil {
		return sepResult{}, err
	}
	work := filepath.Join(dir, baseName(src)+".work")
	os.RemoveAll(work) // a half-finished attempt is not a resume point
	if err := os.MkdirAll(work, 0o755); err != nil {
		return sepResult{}, err
	}
	// the chunks and stems go when the two halves are written -- but only then.
	// What is worth having after "the server refused c02.wav" is c02.wav.
	done := false
	defer func() {
		if done {
			os.RemoveAll(work)
		}
	}()

	a.prog(trackSTT, base+0.02*unit, "reading %s", name)
	mix := filepath.Join(work, "mix.wav")
	if err := a.runCmd(ffTool("ffmpeg"), "-v", "error", "-y", "-i", src,
		"-vn", "-ac", sepChannels, "-ar", sepRate, "-c:a", "pcm_s16le", mix); err != nil {
		return sepResult{}, err
	}
	dur, err := ffprobeDur(mix)
	if err != nil {
		return sepResult{}, err
	}
	edges := []float64{0, dur}
	if dur > sepChunkMax {
		edges = append(append([]float64{0},
			asrCuts(dur, a.quietSpots(mix), sepChunkMax, sepCutSeek)...), dur)
	}
	n := len(edges) - 1
	a.logfIdle(">>> [%s] splitting the voice off %.1f s in %d part(s) (%s)",
		baseName(src), dur, n, a.readConf().SepModel)

	var voiceParts, restParts []string
	for i := 0; i < n; i++ {
		if err := a.checkpoint(); err != nil {
			return sepResult{}, err
		}
		a.prog(trackSTT, base+(0.05+0.8*float64(i)/float64(n))*unit,
			"splitting %s %d/%d", name, i+1, n)
		part := mix
		if n > 1 {
			part = filepath.Join(work, fmt.Sprintf("c%02d.wav", i))
			// -ss ahead of -i seeks the input, which on the pcm just written is
			// exact rather than a keyframe away
			if err := a.runCmd(ffTool("ffmpeg"), "-v", "error", "-y",
				"-ss", fmt.Sprint(edges[i]), "-t", fmt.Sprint(edges[i+1]-edges[i]),
				"-i", mix, "-c:a", "pcm_s16le", part); err != nil {
				return sepResult{}, err
			}
		}
		v, r, err := a.sepChunk(part, work, i)
		if err != nil {
			return sepResult{}, err
		}
		voiceParts, restParts = append(voiceParts, v), append(restParts, r)
	}

	a.prog(trackSTT, base+0.9*unit, "writing the two halves of %s", name)
	if err := a.sepJoin(voiceParts, voice); err != nil {
		return sepResult{}, err
	}
	restWav := rest
	if isVideo(src) {
		restWav = filepath.Join(work, "rest.wav")
	}
	if err := a.sepJoin(restParts, restWav); err != nil {
		return sepResult{}, err
	}
	if isVideo(src) {
		// the picture is copied, never re-encoded: this file exists to carry a
		// different soundtrack, and re-compressing the frames to do it would
		// cost the session a generation of quality for nothing
		if err := a.runCmd(ffTool("ffmpeg"), "-v", "error", "-y", "-i", src, "-i", restWav,
			"-map", "0:v:0", "-map", "1:a:0", "-c:v", "copy", "-c:a", "flac",
			"-shortest", rest); err != nil {
			return sepResult{}, err
		}
	}
	done = true
	a.logfIdle(">>> [%s] split into %s and %s", baseName(src),
		filepath.Base(rest), filepath.Base(voice))
	// and how the sound fell between them, said now: a residual 13 dB under
	// the mix is a recording the model heard as voice from end to end --
	// other players talking in game chat, say -- and the half named for the
	// room then has next to nothing in it. Found by ear a day later, that
	// reads as "the footage is silent", and it is not the footage.
	if m, v, r := sepLoudness(mix), sepLoudness(voice), sepLoudness(restWav); m != "" {
		a.logfIdle(">>> [%s] the mix averaged %s: the voice half %s, the rest %s%s",
			baseName(src), m, v, r, sepLopsided(mix, restWav))
	}
	return sepResult{src, rest, voice}, nil
}

// sepLoudness is a file's mean level, as ffmpeg's volumedetect reports it, or
// "" when it cannot be measured -- a line of the log, not a step of the run.
func sepLoudness(path string) string {
	out, err := exec.Command(ffTool("ffmpeg"), "-v", "info", "-t", "600", "-i", path, "-vn",
		"-af", "volumedetect", "-f", "null", "-").CombinedOutput()
	if err != nil {
		return ""
	}
	for _, l := range strings.Split(string(out), "\n") {
		if i := strings.Index(l, "mean_volume:"); i >= 0 {
			return strings.TrimSpace(l[i+len("mean_volume:"):])
		}
	}
	return ""
}

// sepLopsided is the note for a rest that is sepLopsidedDB or more under the
// mix: what it means, and what to do about it.
func sepLopsided(mix, rest string) string {
	m, r := sepMeanDB(sepLoudness(mix)), sepMeanDB(sepLoudness(rest))
	if m == 0 && r == 0 || m-r < sepLopsidedDB {
		return ""
	}
	return fmt.Sprintf(" -- the rest is %.0f dB under the mix: the model heard this recording as "+
		"voice nearly throughout, so the half named for the room has little in it. Hear the voice "+
		"half where the game is wanted, or cut from the original instead of the split.", m-r)
}

// sepLopsidedDB is how far under the mix the rest has to fall before the log
// says so. 10 dB is a third of the loudness: past it the rest is a residue.
const sepLopsidedDB = 10.0

// sepMeanDB reads volumedetect's "-49.3 dB" back as a number; 0 for "".
func sepMeanDB(s string) float64 {
	var v float64
	fmt.Sscanf(strings.TrimSuffix(strings.TrimSpace(s), " dB"), "%g", &v)
	return v
}

// sepChunk runs one piece through the model and writes its two halves, giving
// back the paths. Everything the model may have called its stems is resolved
// here, so the caller only ever handles a voice file and a rest file.
func (a *App) sepChunk(part, work string, i int) (voice, rest string, err error) {
	up, err := a.serverFile(part)
	if err != nil {
		return "", "", err
	}
	body, err := a.audioRun(a.readConf().SepModel, map[string]any{"audio": up})
	if err != nil {
		return "", "", err
	}
	stems, err := sepStems(body)
	if err != nil {
		return "", "", err
	}
	wav := map[string][]byte{}
	var ids []string
	for _, s := range stems {
		b, err := base64.StdEncoding.DecodeString(s.Audio)
		if err != nil {
			return "", "", fmt.Errorf("stem %q is not audio: %w", s.ID, err)
		}
		wav[s.ID], ids = b, append(ids, s.ID)
	}
	vID, restIDs, err := sepPick(ids)
	if err != nil {
		return "", "", err
	}
	voice = filepath.Join(work, fmt.Sprintf("v%02d.wav", i))
	if err := os.WriteFile(voice, wav[vID], 0o644); err != nil {
		return "", "", err
	}
	rest = filepath.Join(work, fmt.Sprintf("r%02d.wav", i))
	if len(restIDs) == 1 {
		return voice, rest, os.WriteFile(rest, wav[restIDs[0]], 0o644)
	}
	// more than one stem is not the voice, which is the four-instrument
	// models: everything that is not the voice IS the recording without it, so
	// they go back together rather than the caller picking one to keep
	args := []string{"-v", "error", "-y"}
	for j, id := range restIDs {
		p := filepath.Join(work, fmt.Sprintf("r%02d_%d.wav", i, j))
		if err := os.WriteFile(p, wav[id], 0o644); err != nil {
			return "", "", err
		}
		args = append(args, "-i", p)
	}
	// normalize=0: these stems were one recording a moment ago and summing them
	// has to give that recording back, not an eighth of it
	args = append(args, "-filter_complex",
		fmt.Sprintf("amix=inputs=%d:normalize=0", len(restIDs)),
		"-c:a", "pcm_s16le", rest)
	return voice, rest, a.runCmd(ffTool("ffmpeg"), args...)
}

// sepJoin puts the pieces back in order. One piece is the whole thing and is
// simply moved: a rename is not worth a decode.
func (a *App) sepJoin(parts []string, dst string) error {
	if len(parts) == 1 {
		return os.Rename(parts[0], dst)
	}
	lf := filepath.Join(filepath.Dir(parts[0]), "join_"+filepath.Base(dst)+".txt")
	var b strings.Builder
	for _, p := range parts {
		b.WriteString(concatLine(p))
	}
	if err := os.WriteFile(lf, []byte(b.String()), 0o644); err != nil {
		return err
	}
	return a.runCmd(ffTool("ffmpeg"), "-v", "error", "-y",
		"-f", "concat", "-safe", "0", "-i", lf, "-c:a", "pcm_s16le", dst)
}
