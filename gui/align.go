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

// ---- what the ASR already knew ------------------------------------------------
//
// The ASR does its own chunking, and each request comes back with the text of
// exactly the seconds it was handed. That split used to be thrown away --
// asrLongAt joined the texts with a space and kept only the join -- so the
// aligner had to GUESS how much of the transcript belonged in each of its own
// windows. It guessed at four words a second.
//
// Measured on a 42-minute lecture whose speaker says 1.85: two windows of one
// take drew 89 and 100 words for twenty seconds that held about 37 each. A
// forced aligner cannot decline, so it compressed them; compressed words end
// nowhere near the window's far edge, so the carry that was meant to hand the
// overflow on handed nothing on and counted all of them as spoken. The text ran
// out with 38 s of audio still to place, the loop stopped, and those 38 s had
// no word over them. The cut then deleted them as footage with nothing said --
// a fifth of that session, gone, with nothing logged.
//
// So the split is kept rather than re-derived. Same seconds, same text, no
// estimate anywhere.

// asrPiece is one ASR request: the seconds of the recording it covered, and
// the words that came back for exactly those seconds.
type asrPiece struct {
	S    float64 `json:"s"`
	E    float64 `json:"e"`
	Text string  `json:"text"`
}

// asrPieces live beside words.json and never inside it: that file is the
// server's own document and nothing here writes into it.
func asrPiecesFile(dir string) string { return filepath.Join(dir, "asrchunks.json") }

func saveASRPieces(dir string, pieces []asrPiece) error {
	if len(pieces) == 0 {
		return nil
	}
	b, err := json.Marshal(pieces)
	if err != nil {
		return err
	}
	return os.WriteFile(asrPiecesFile(dir), b, 0o644)
}

// loadASRPieces is the split as the ASR made it, or nothing for a recording
// transcribed before it was kept -- which still aligns, by the share-out in
// alignPieces, only without the exactness.
func loadASRPieces(dir string) []asrPiece {
	b, err := os.ReadFile(asrPiecesFile(dir))
	if err != nil {
		return nil
	}
	var out []asrPiece
	if json.Unmarshal(b, &out) != nil {
		return nil
	}
	return out
}

// alignSpan is one stretch of a recording and the words said in it: what one
// align request is made of.
type alignSpan struct {
	s, e  float64
	words []string
}

// alignPieces is the recording cut into stretches that carry their own words.
//
// From the ASR's own split where there is one: its pieces are in order and
// cover the recording, and the transcript was joined from them in that order,
// so a cursor walking the transcript hands each piece back exactly what it
// heard. The last piece takes whatever remains, so every word is placed
// somewhere even if a count disagrees by one.
//
// Where there is no split -- a project transcribed before this was kept -- the
// recording is cut the way the ASR cuts it and the words are shared out by how
// much talking each stretch holds. Still an estimate, but one that cannot run
// out of text early, because the shares add up to the whole transcript by
// construction rather than by a rate per second.
func alignPieces(pieces []asrPiece, text []string, dur float64, quiet []span) []alignSpan {
	if out, ok := piecesFromASR(pieces, text); ok {
		return out
	}
	edges := append(append([]float64{0}, asrCuts(dur, quiet, alignChunkMax, alignCutSeek)...), dur)
	total := 0.0
	for i := 0; i+1 < len(edges); i++ {
		total += voicedIn(quiet, edges[i], edges[i+1])
	}
	var out []alignSpan
	at, seen := 0, 0.0
	for i := 0; i+1 < len(edges); i++ {
		seen += voicedIn(quiet, edges[i], edges[i+1])
		want := len(text) // the last stretch takes the rest, whatever rounding did
		if total > 0 && i+2 < len(edges) {
			want = int(math.Round(float64(len(text)) * seen / total))
		}
		want = min(max(want, at), len(text))
		out = append(out, alignSpan{s: edges[i], e: edges[i+1], words: text[at:want]})
		at = want
	}
	return out
}

// piecesFromASR is the exact mapping, or false when the pieces on disk do not
// account for the transcript -- an older project, or a file written by a
// different shape of answer. Wrong is worse than absent here: a mapping that is
// off by a piece puts every word after it in the wrong place.
func piecesFromASR(pieces []asrPiece, text []string) ([]alignSpan, bool) {
	if len(pieces) == 0 {
		return nil, false
	}
	n := 0
	for _, p := range pieces {
		n += len(strings.Fields(p.Text))
	}
	if n != len(text) {
		return nil, false
	}
	out := make([]alignSpan, 0, len(pieces))
	at := 0
	for i, p := range pieces {
		k := at + len(strings.Fields(p.Text))
		if i == len(pieces)-1 {
			k = len(text)
		}
		out = append(out, alignSpan{s: p.S, e: p.E, words: text[at:k]})
		at = k
	}
	return out, true
}

// voicedIn is how much of t0..t1 has sound in it: the measure everything here
// shares out by, because a stretch of silence holds no words however long it is.
func voicedIn(quiet []span, t0, t1 float64) float64 {
	at, v := t0, 0.0
	for _, q := range quiet {
		if q.e <= at || q.s >= t1 {
			continue
		}
		if q.s > at {
			v += q.s - at
		}
		if at = math.Max(at, q.e); at >= t1 {
			return v
		}
	}
	if t1 > at {
		v += t1 - at
	}
	return v
}

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
	quiet := a.quietSpots(wav)
	tmp, err := os.MkdirTemp("", "naivepost-align-*")
	if err != nil {
		return err
	}
	defer os.RemoveAll(tmp)

	var out []asrToken
	for _, sp := range alignPieces(loadASRPieces(dir), text, dur, quiet) {
		got, err := a.alignSpanAt(models, wav, tmp, sp, quiet, alignChunkMax)
		if err != nil {
			return err
		}
		out = append(out, got...)
	}
	if len(out) == 0 {
		return fmt.Errorf("no words came back")
	}
	a.warnIfBare(out, dur, quiet)
	b, err := json.Marshal(map[string]any{"words": out})
	if err != nil {
		return err
	}
	return os.WriteFile(alignedWords(dir), b, 0o644)
}

// alignSpanAt times one stretch, in the recording's own seconds.
//
// A stretch too long for one request is cut in two and each half timed with
// its share of the stretch's words, so halving costs precision inside that
// stretch and nothing at all outside it: the next piece still starts on the
// word the ASR heard there. That is the whole difference from the carry this
// replaced, which let one bad window spoil every window after it.
func (a *App) alignSpanAt(models []string, wav, tmp string, sp alignSpan, quiet []span, limit float64) ([]asrToken, error) {
	if len(sp.words) == 0 {
		return nil, nil // nothing was heard here, so there is nothing to place
	}
	// a forced aligner spreads the text it is given across the audio it is
	// given, and it cannot decline: hand it a stretch that is half silence and
	// it puts words in the silence. One recording ran 13.5 s past the last
	// thing said -- the camera left rolling -- and its last four words came
	// back at 30.9 s of a 31.2 s file when they had been said before 17.8.
	s0, s1, any := soundSpan(quiet, sp.s, sp.e)
	if !any {
		// words with no sound under them: nothing to trim to, so send the
		// stretch as it stands rather than dropping what was heard in it
		s0, s1 = sp.s, sp.e
	}
	if s1-s0 > limit && s1-s0 > alignChunkMin {
		return a.alignHalves(models, wav, tmp, alignSpan{s: s0, e: s1, words: sp.words}, quiet, limit)
	}
	part := filepath.Join(tmp, fmt.Sprintf("w%09d.wav", int(s0*1000)))
	if err := a.runCmd(ffTool("ffmpeg"), "-v", "error", "-y", "-ss", fmt.Sprint(s0),
		"-t", fmt.Sprint(s1-s0), "-i", wav, "-c:a", "pcm_s16le", part); err != nil {
		return nil, err
	}
	got, err := a.alignOne(models, part, strings.Join(sp.words, " "))
	if err != nil {
		// down the same ladder the ASR and diarization walk: a graph the
		// machine cannot find room for, it can find room for in two halves
		if noRoom(err) && s1-s0 > alignChunkMin {
			a.logfIdle("!!! align: no room for %.0f s of audio (%v) -- halving", s1-s0, err)
			half := math.Max(alignChunkMin, (s1-s0)/2)
			return a.alignHalves(models, wav, tmp, alignSpan{s: s0, e: s1, words: sp.words}, quiet, half)
		}
		return nil, err
	}
	out := make([]asrToken, 0, len(got))
	for _, w := range got {
		out = append(out, asrToken{Word: w.w,
			Start: int64((w.s + s0) * sampleRate), End: int64((w.e + s0) * sampleRate)})
	}
	return out, nil
}

// alignHalves times both halves of a stretch that would not go in one request.
func (a *App) alignHalves(models []string, wav, tmp string, sp alignSpan, quiet []span, limit float64) ([]asrToken, error) {
	lo, hi := splitSpan(sp, quiet)
	first, err := a.alignSpanAt(models, wav, tmp, lo, quiet, limit)
	if err != nil {
		return nil, err
	}
	second, err := a.alignSpanAt(models, wav, tmp, hi, quiet, limit)
	if err != nil {
		return nil, err
	}
	return append(first, second...), nil
}

// splitSpan cuts a stretch at the quietest moment nearest its middle and
// divides its words by how much talking falls each side of the cut. The cut is
// kept away from both edges, because a half that cannot be made smaller is a
// recursion that does not end.
func splitSpan(sp alignSpan, quiet []span) (alignSpan, alignSpan) {
	mid, cut, best := (sp.s+sp.e)/2, (sp.s+sp.e)/2, math.Inf(1)
	for _, q := range quiet {
		m := (math.Max(q.s, sp.s) + math.Min(q.e, sp.e)) / 2
		if m <= sp.s || m >= sp.e {
			continue
		}
		if d := math.Abs(m - mid); d < best {
			cut, best = m, d
		}
	}
	room := (sp.e - sp.s) / 8
	cut = math.Min(math.Max(cut, sp.s+room), sp.e-room)
	n := len(sp.words)
	left := voicedIn(quiet, sp.s, cut)
	k := n / 2
	if total := left + voicedIn(quiet, cut, sp.e); total > 0 {
		k = int(math.Round(float64(n) * left / total))
	}
	k = min(max(k, 0), n)
	return alignSpan{s: sp.s, e: cut, words: sp.words[:k]},
		alignSpan{s: cut, e: sp.e, words: sp.words[k:]}
}

// warnIfBare says so when the times do not cover the talking.
//
// This failure is silent by nature. Words placed in the wrong second are still
// words, every one of them is present, and the only visible sign is voiced
// audio with no word over it -- which the cut reads as footage where nothing
// was said and deletes. It went unnoticed for a whole session. It does not get
// to go unnoticed again.
func (a *App) warnIfBare(out []asrToken, dur float64, quiet []span) {
	voiced := voicedIn(quiet, 0, dur)
	if voiced <= 0 {
		return
	}
	bare := bareVoiced(out, dur, quiet)
	if bare < alignBareWarn || bare < alignBareShare*voiced {
		return
	}
	a.logfIdle("!!! align: %.0f s of the %.0f s spoken has no word over it -- the times are wrong, and the cut will drop that footage",
		bare, voiced)
}

// bareVoiced is how many of the recording's talking seconds no word covers.
func bareVoiced(out []asrToken, dur float64, quiet []span) float64 {
	var cov []span
	for _, w := range out {
		s, e := float64(w.Start)/sampleRate-alignSoundPad, float64(w.End)/sampleRate+alignSoundPad
		if n := len(cov); n > 0 && s <= cov[n-1].e {
			cov[n-1].e = math.Max(cov[n-1].e, e)
			continue
		}
		cov = append(cov, span{s: s, e: e})
	}
	sort.Slice(cov, func(i, j int) bool { return cov[i].s < cov[j].s })
	bare, at := 0.0, 0.0
	for _, c := range cov {
		if c.s > at {
			bare += voicedIn(quiet, at, c.s)
		}
		at = math.Max(at, c.e)
	}
	if at < dur {
		bare += voicedIn(quiet, at, dur)
	}
	return bare
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

const (
	// how much audio goes into one align request -- the same number the ASR
	// chunks by (asrChunkQwen), so the pieces line up and each one carries its
	// own words. Where the machine cannot hold that much graph at once the
	// request is halved and halved again (alignSpanAt), which is a cost inside
	// one piece rather than a size everything pays for all the time.
	alignChunkMax = 60.0
	// and the shortest it is worth halving to. Below this a stretch is shorter
	// than a sentence, and an aligner given a fragment with no phrase around it
	// places it no better than the silence detector would.
	alignChunkMin = 15.0
	// how far a window edge may slide to find a silence. Small, because the
	// window is: asrCuts gives up on sliding at all once the seek is half the
	// piece, and a window that never slides ends mid-word.
	alignCutSeek = 4.0
	// how much of the quiet either side of a window's speech is sent with it
	// (soundSpan). A word fades out below the silence threshold before it is
	// over, and a window cut exactly on the threshold clips it.
	alignSoundPad = 0.25
	// talking with no word over it worth saying out loud, in seconds and as a
	// share of the talking. Both, because ten bare seconds of a four-hour
	// session is rounding and ten bare seconds of a twenty-second clip is the
	// whole clip (warnIfBare).
	alignBareWarn  = 10.0
	alignBareShare = 0.08
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
