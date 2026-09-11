package main

// Forced alignment: which second each word was actually said in.
//
// The ASR's own stamps are 80 ms slots placed near where the model DECIDED a
// token, and measured against the waveform on one session they run 0.5 to 1.0 s
// behind the sound, with no constant offset to correct for. That is fine for
// reading a transcript and useless for cutting on a word boundary.
//
// The waveform fixes half of it: it says exactly where sound starts and stops,
// which is enough wherever a join falls in a silence -- between two takes, most
// of them. It cannot help inside a phrase. "...state of the art, and the whole
// run..." is one connected stream, and cutting between "art" and "and" means
// knowing which syllable is which. The envelope sees the syllables and cannot
// name them.
//
// An aligner is told the words AND the audio and answers where each word is,
// which is the only signal that can. This is the client for it: a task like any
// other on the audio server (task "align"), asked for by TASK and not by model
// id, so whichever aligner that server has registered -- a wav2vec2 CTC one, a
// transformer one -- is the one that answers, and swapping is an entry in
// config-audiocpp.json rather than a change here.
//
// It is optional by design. A server with no aligner registered is not an
// error: the edges fall back to the envelope (retake_edge.go), which is where
// they were before this existed.

import (
	"encoding/json"
	"errors"
	"fmt"
	"math"
	"os"
	"path/filepath"
	"sort"
	"strings"
)

// alignWord is one word as the aligner places it, in the clip's own seconds.
type alignWord struct {
	s, e float64
	w    string
}

// alignModels is the models that place cut points on the word, best first, or
// nothing when the server has none.
//
// A LIST and not a choice, because a catalog entry is a claim rather than a
// working model: a server will list an aligner whose family the engine it is
// running was not built with, and answer every request for it with "unsupported
// model family hint". Registered, listed, and unusable. Picking one id by name
// and reporting its failure leaves the second one -- which does work -- sitting
// there untried, so the caller walks the list instead (alignInput).
//
// Named in the settings, that name is the whole list: a box exists to be obeyed,
// and quietly using a different model than the one asked for is worse than
// doing nothing.
func (a *App) alignModels() []string {
	cat, err := audioCatalog(a.audioURL(), a.readConf().TTSKey)
	if err != nil {
		return nil
	}
	if want := strings.TrimSpace(a.readConf().AlignModel); want != "" {
		if m, ok := cat[want]; ok && m.Task == "align" {
			return []string{want}
		}
		a.logfIdle("!!! align: %s does not serve %q for alignment -- cut points come off the waveform",
			a.audioURL(), want)
		return nil
	}
	var ids []string
	for id, m := range cat {
		if m.Task == "align" {
			ids = append(ids, id)
		}
	}
	alignOrder(ids)
	return ids
}

// alignOrder is the order the aligners are tried in: defAlignModel first where
// the server has it, then the rest by name so one server answers the same way
// every run.
//
// The preference is the whole point of the default. This stack registers two
// aligners -- mms-aligner and qwen3-aligner -- and a plain sort put the MMS one
// first on every machine with both, which is picking the aligner by alphabet.
// It stays a preference and not a rule: a server with only the other one keeps
// aligning, where a hard default would have it fall back to the waveform and
// say so in red.
func alignOrder(ids []string) {
	sort.Slice(ids, func(i, j int) bool {
		if (ids[i] == defAlignModel) != (ids[j] == defAlignModel) {
			return ids[i] == defAlignModel
		}
		return ids[i] < ids[j]
	})
}

// alignModel is the one that answered last, or the first worth trying.
func (a *App) alignModel() string {
	if a.alignPick != "" {
		return a.alignPick
	}
	if ids := a.alignModels(); len(ids) > 0 {
		return ids[0]
	}
	return ""
}

// alignClip asks the aligner where each word of text falls in one wav. The
// times come back in the clip's own seconds, from zero.
func (a *App) alignClip(model, wav, text string) ([]alignWord, error) {
	up, err := a.serverFile(wav)
	if err != nil {
		return nil, err
	}
	body, err := a.audioRun(model, map[string]any{
		"audio": up, "text": text, "language": a.asrLanguage()})
	if err != nil {
		return nil, err
	}
	words, err := alignWords(body)
	if err != nil {
		// the shape is the one thing this cannot be sure of until a server
		// answers, so what came back is written down rather than guessed at
		a.logfIdle("!!! align: %v -- the answer began: %s", err, head(string(body), 300))
		return nil, err
	}
	return words, nil
}

// alignWords reads the answer.
//
// Every aligner family writes it a little differently, and the one thing they
// agree on is a list of words with two numbers each. So the list is looked for
// under the names they use, and the numbers are taken in whichever unit they
// came: samples where the field says samples, seconds otherwise -- and a
// "second" over an hour is samples that forgot to say so, which is the one
// mistake that would put a cut somewhere in the next decade.
func alignWords(body []byte) ([]alignWord, error) {
	var doc struct {
		Words     []alignEntry `json:"words"`
		Alignment []alignEntry `json:"alignment"`
		Segments  []alignEntry `json:"segments"`
		Result    struct {
			Words     []alignEntry `json:"words"`
			Alignment []alignEntry `json:"alignment"`
		} `json:"result"`
	}
	if err := json.Unmarshal(body, &doc); err != nil {
		return nil, fmt.Errorf("unreadable answer: %w", err)
	}
	for _, list := range [][]alignEntry{doc.Words, doc.Alignment, doc.Segments,
		doc.Result.Words, doc.Result.Alignment} {
		if len(list) == 0 {
			continue
		}
		var out []alignWord
		for _, e := range list {
			s, ok1 := e.start()
			t, ok2 := e.end()
			if !ok1 || !ok2 {
				continue
			}
			w := strings.TrimSpace(e.text())
			if w == "" {
				continue
			}
			out = append(out, alignWord{s: s, e: t, w: strings.ToLower(w)})
		}
		if len(out) > 0 {
			return out, nil
		}
	}
	return nil, fmt.Errorf("no words in the answer")
}

// alignEntry is one word however the server spells it.
type alignEntry struct {
	Word  string   `json:"word"`
	Text  string   `json:"text"`
	Label string   `json:"label"`
	S     *float64 `json:"start"`
	E     *float64 `json:"end"`
	SS    *float64 `json:"start_sample"`
	ES    *float64 `json:"end_sample"`
	ST    *float64 `json:"start_time"`
	ET    *float64 `json:"end_time"`
	SMS   *float64 `json:"start_ms"`
	EMS   *float64 `json:"end_ms"`
}

func (e alignEntry) text() string {
	for _, s := range []string{e.Word, e.Text, e.Label} {
		if strings.TrimSpace(s) != "" {
			return s
		}
	}
	return ""
}

func (e alignEntry) start() (float64, bool) { return pickTime(e.S, e.SS, e.ST, e.SMS) }
func (e alignEntry) end() (float64, bool)   { return pickTime(e.E, e.ES, e.ET, e.EMS) }

// pickTime is the first of the four that is there, in seconds: a plain one, a
// sample count, a time, or milliseconds.
func pickTime(plain, samples, secs, ms *float64) (float64, bool) {
	switch {
	case samples != nil:
		return *samples / sampleRate, true
	case ms != nil:
		return *ms / 1000, true
	case secs != nil:
		return *secs, true
	case plain != nil:
		// "start" alone is seconds in every aligner that writes it -- unless it
		// is a sample count that did not say so, which a clip of a few seconds
		// makes obvious
		if *plain > alignClipMax {
			return *plain / sampleRate, true
		}
		return *plain, true
	}
	return 0, false
}

// alignClipMax is comfortably past the longest clip this sends (alignChunkMax).
// A "second" beyond it did not mean seconds.
const alignClipMax = 120.0

// head is the first n characters of s, for a log line about an answer nobody
// expected.
func head(s string, n int) string {
	s = strings.Join(strings.Fields(s), " ")
	if len(s) > n {
		return s[:n] + "..."
	}
	return s
}

// clipWav cuts t0..t1 of a source into a 16 kHz mono wav in dir, which is what
// every listening model on that server is fed.
func (a *App) clipWav(src string, t0, t1 float64, dir, name string) (string, error) {
	wav := filepath.Join(dir, name+".wav")
	if err := os.MkdirAll(dir, 0o755); err != nil {
		return "", err
	}
	if err := a.runCmd(ffTool("ffmpeg"), "-v", "error", "-y",
		"-ss", fmt.Sprintf("%.3f", t0), "-t", fmt.Sprintf("%.3f", t1-t0), "-i", src,
		"-vn", "-ac", "1", "-ar", fmt.Sprintf("%d", sampleRate), "-c:a", "pcm_s16le", wav); err != nil {
		return "", err
	}
	return wav, nil
}

// ---- the times every step reads ----------------------------------------------
//
// The ASR is asked for words and the aligner is asked when they were said, and
// the second answer is the one everything downstream uses. That split is what
// lets the best transcriber be used even though it returns no timings at all:
// Qwen3-ASR reads 85% of a take against the script where Nemotron reads 82%,
// and answers with text alone. Nemotron's own stamps, measured, sit 0.29 s
// behind the sound; the aligner's sit 0.02 s. So there is no case where the
// ASR's times are the ones to keep, and one where they are all there is -- a
// server with no aligner registered, which still works exactly as it did.

// alignedWords is where a source's aligned timings live, beside the ASR's own
// answer and never over it: words.json is what the server said, and a file this
// wrote is not.
func alignedWords(dir string) string { return filepath.Join(dir, "words.aligned.json") }

// alignInput times one source's transcript, and is the reason a timing-less ASR
// can be used at all: the segments a transcript is built from come from word
// times (mergeSegments), so with neither the ASR's nor the aligner's there is
// no transcript to build.
//
// In WINDOWS, because the graph the server allocates for one request grows with
// the audio in it. Measured on this stack: 30 s and 45 s answer, 60 s and 96 s
// come back "failed to allocate Qwen3 ASR thinker classification graph", and a
// machine with four model servers resident has no more to give. A window is a
// bounded allocation whatever the recording's length, which is the difference
// between a take that aligns and a take that cannot.
//
// Skipped when it has been done: the file is written whole at the end, so it
// existing means it is complete, the same resume marker words.json is.
func (a *App) alignInput(dir, wav string, models []string) error {
	if len(models) == 0 || exists(alignedWords(dir)) {
		return nil
	}
	text := strings.Fields(readFileString(filepath.Join(dir, "transcript.txt")))
	if len(text) == 0 {
		return nil // no speech is nothing to align, and not a failure
	}
	dur, err := ffprobeDur(wav)
	if err != nil {
		return err
	}
	// cut where nobody is talking, like the ASR's own chunking: a window that
	// ends mid-word hands the aligner half a word to place
	quiet := a.quietSpots(wav)
	edges := append(append([]float64{0}, asrCuts(dur, quiet, alignChunkMax, alignCutSeek)...), dur)
	tmp, err := os.MkdirTemp("", "naivepost-align-*")
	if err != nil {
		return err
	}
	defer os.RemoveAll(tmp)

	var out []asrToken
	rest := text
	for i := 0; i+1 < len(edges); i++ {
		t0, t1 := edges[i], edges[i+1]
		if len(rest) == 0 {
			break
		}
		// ...and then shrink the window to the part of it with sound in it.
		//
		// A forced aligner spreads the text it is given across the audio it is
		// given, and it has no way to decline: hand it a stretch that is half
		// silence and it will put words in the silence. One recording here ran
		// 13.5 s past the last thing said -- the camera left rolling -- and the
		// last four words of it came back at 30.9 s of a 31.2 s file, each
		// 80 ms long, when they had been said before 17.8. Every gap the page
		// then drew between them was a gap nobody took.
		s0, s1, any := soundSpan(quiet, t0, t1)
		if !any {
			continue // no sound in this window: no words were said in it
		}
		part := filepath.Join(tmp, fmt.Sprintf("w%02d.wav", i))
		if err := a.runCmd(ffTool("ffmpeg"), "-v", "error", "-y", "-ss", fmt.Sprint(s0),
			"-t", fmt.Sprint(s1-s0), "-i", wav, "-c:a", "pcm_s16le", part); err != nil {
			return err
		}
		// what this window plausibly holds, generously: an aligner spreads
		// whatever text it is given across whatever audio it is given, so too
		// much text distorts the whole window and too little leaves words for
		// the next one to pick up -- which the carry below does anyway.
		take := min(len(rest), int((s1-s0)*alignWordsPerSec)+8)
		got, err := a.alignOne(models, part, strings.Join(rest[:take], " "))
		if err != nil {
			return err
		}
		// words that land clear of the far edge are this window's; the rest are
		// the next window's problem, and its audio starts before them. The last
		// window keeps everything, since there is no next one to carry into.
		keep := len(got)
		if i+2 < len(edges) {
			keep = 0
			for _, w := range got {
				if w.e > (s1-s0)-alignEdgeTrim {
					break
				}
				keep++
			}
			if keep == 0 {
				keep = len(got) // nothing landed clear: take it rather than stall
			}
		}
		for _, w := range got[:keep] {
			out = append(out, asrToken{Word: w.w,
				Start: int64((w.s + s0) * sampleRate), End: int64((w.e + s0) * sampleRate)})
		}
		rest = rest[consumed(rest, got[:keep]):]
	}
	if len(out) == 0 {
		return fmt.Errorf("no words came back")
	}
	b, err := json.Marshal(map[string]any{"words": out})
	if err != nil {
		return err
	}
	return os.WriteFile(alignedWords(dir), b, 0o644)
}

// soundSpan is the part of t0..t1 that has sound in it, padded, or false when
// the whole of it is quiet. quiet is the silences of the whole recording
// (quietSpots), in order and not overlapping.
//
// Built by subtracting the silences rather than by asking whether one reaches
// the window's edge, because it does not quite: silencedetect measures the end
// of a file at 31.253312 and ffprobe calls the same file 31.253313 long, and a
// silence tested for reaching the end fails by a microsecond. That microsecond
// used to be the whole fix -- the window went to the aligner untrimmed and the
// last four words landed in thirteen seconds of nothing.
func soundSpan(quiet []span, t0, t1 float64) (float64, float64, bool) {
	// shorter than this is not a piece of speech, it is the seam between two
	// measurements of the same instant
	const bit = 0.05
	first, last, found := 0.0, 0.0, false
	take := func(a, b float64) {
		if b-a < bit {
			return
		}
		if !found {
			first, found = a, true
		}
		last = b
	}
	at := t0
	for _, q := range quiet {
		if q.e <= at || q.s >= t1 {
			continue
		}
		take(at, math.Min(q.s, t1))
		if at = math.Max(at, q.e); at >= t1 {
			break
		}
	}
	take(at, t1)
	if !found {
		return 0, 0, false
	}
	// a hair of the quiet on each side, because a word's first and last
	// moments are quieter than the threshold and cutting them off is the very
	// thing this is here to stop
	return math.Max(t0, first-alignSoundPad), math.Min(t1, last+alignSoundPad), true
}

// consumed is how many of the text's words an answer accounts for, and is what
// makes the next window start on the right one.
//
// Counting the answer would do if a forced aligner always handed back one word
// per word it was given, but the families do not agree on that either: some
// answer in phrases. So the answer is read back against the text instead, a
// word at a time, and a word that cannot be found within the next few is
// passed over rather than allowed to drag the count backwards.
func consumed(text []string, got []alignWord) int {
	i := 0
	for _, w := range got {
		for _, f := range strings.Fields(w.w) {
			for j := i; j < len(text) && j < i+4; j++ {
				if sameWord(bareWord(text[j]), bareWord(f)) {
					i = j + 1
					break
				}
			}
		}
	}
	if i == 0 {
		return min(len(got), len(text)) // nothing matched: trust the count
	}
	return i
}

const (
	// how much audio goes into one align request. Not a preference: 45 s
	// answers on this stack and 60 s does not, so this is the size that fits
	// with room to spare on a machine whose GPU memory is shared with
	// everything else running.
	alignChunkMax = 30.0
	// how far a window edge may slide to find a silence. Small, because the
	// window is: asrCuts gives up on sliding at all once the seek is half the
	// piece, and a window that never slides ends mid-word.
	alignCutSeek = 4.0
	// how many words a window is assumed to hold, before the carry corrects
	// it. Fast speech is about 3 a second; the aligner is given more than it
	// needs rather than less, because words left over are picked up by the
	// next window and words forced in are not.
	alignWordsPerSec = 4.0
	// how close to a window's far edge a word may end and still be trusted.
	// The last word before a cut is the one most likely to be half in the next
	// window, and it is cheaper to let the next window place it.
	alignEdgeTrim = 2.0
	// how much of the quiet either side of a window's speech is sent with it
	// (soundSpan). A word fades out below the silence threshold before it is
	// over, and a window cut exactly on the threshold clips it.
	alignSoundPad = 0.25
)

// alignOne aligns one window, walking the aligners until one answers. A
// catalog entry is a claim rather than a promise -- mms-aligner is listed by a
// server whose engine cannot serve that family -- so the first that answers is
// remembered and the rest are never tried again.
func (a *App) alignOne(models []string, wav, text string) ([]alignWord, error) {
	if a.alignPick != "" {
		models = []string{a.alignPick}
	}
	var last error
	for i, model := range models {
		words, err := a.alignClip(model, wav, text)
		if err == nil {
			if a.alignPick == "" && i > 0 {
				a.logfIdle(">>> align: %s answers where %s does not -- using it for the rest of the run",
					model, strings.Join(models[:i], ", "))
			}
			a.alignPick = model
			return words, nil
		}
		last = fmt.Errorf("%s: %w", model, err)
		if errors.Is(err, errStopped) {
			return nil, last
		}
	}
	return nil, last
}

// wordTimes is one source's words, aligned where they have been and the ASR's
// own otherwise. Everything that asks when a word was said comes through here.
func wordTimes(dir string) []asrToken {
	for _, f := range []string{alignedWords(dir), filepath.Join(dir, "words.json")} {
		b, err := os.ReadFile(f)
		if err != nil {
			continue
		}
		var doc struct {
			Words []asrToken `json:"words"`
		}
		if json.Unmarshal(b, &doc) == nil && len(doc.Words) > 0 {
			return doc.Words
		}
	}
	return nil
}

// readFileString is a file's contents, or "" when there is none: for the places
// where a missing file and an empty one mean the same thing.
func readFileString(path string) string {
	b, err := os.ReadFile(path)
	if err != nil {
		return ""
	}
	return string(b)
}
