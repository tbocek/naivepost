package main

// The aligner's answer, read whatever shape it arrives in.
//
// Two aligners are in play -- a wav2vec2 CTC one and a transformer one -- and
// the app asks for the TASK rather than for either of them, so it has to read
// what both write. What they agree on is a list of words with two numbers each;
// what they do not agree on is what the list is called or what unit the numbers
// are in, and getting the unit wrong puts a cut somewhere in the next decade.

import (
	"fmt"
	"math"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestAnAlignersAnswerIsReadInAnyOfItsShapes(t *testing.T) {
	for _, c := range []struct {
		what string
		body string
	}{
		{"words, seconds", `{"words":[{"word":"art","start":1.25,"end":1.44},{"word":"and","start":1.62,"end":1.71}]}`},
		{"alignment, times", `{"alignment":[{"text":"art","start_time":1.25,"end_time":1.44},{"text":"and","start_time":1.62,"end_time":1.71}]}`},
		{"nested under result", `{"result":{"words":[{"word":"art","start":1.25,"end":1.44},{"word":"and","start":1.62,"end":1.71}]}}`},
		{"samples", `{"words":[{"word":"art","start_sample":20000,"end_sample":23040},{"word":"and","start_sample":25920,"end_sample":27360}]}`},
		{"milliseconds", `{"words":[{"word":"art","start_ms":1250,"end_ms":1440},{"word":"and","start_ms":1620,"end_ms":1710}]}`},
	} {
		got, err := alignWords([]byte(c.body))
		if err != nil {
			t.Errorf("%s: %v", c.what, err)
			continue
		}
		if len(got) != 2 || got[0].w != "art" || got[1].w != "and" {
			t.Errorf("%s: read as %+v", c.what, got)
			continue
		}
		if got[0].e < 1.43 || got[0].e > 1.45 || got[1].s < 1.61 || got[1].s > 1.63 {
			t.Errorf("%s: times came out %.3f-%.3f / %.3f-%.3f, want ~1.44 / ~1.62",
				c.what, got[0].s, got[0].e, got[1].s, got[1].e)
		}
	}

	// a sample count in a field that says seconds: a clip is never two hours
	// long, so the number tells on itself
	got, err := alignWords([]byte(`{"words":[{"word":"art","start":20000,"end":23040}]}`))
	if err != nil || len(got) != 1 || got[0].s < 1.2 || got[0].s > 1.3 {
		t.Errorf("an unlabelled sample count was taken as seconds: %+v (%v)", got, err)
	}

	// and an answer with nothing usable in it says so rather than passing
	// empty times along to something that cuts video with them
	for _, bad := range []string{`{}`, `{"words":[]}`, `{"words":[{"word":"art"}]}`, `not json`} {
		if got, err := alignWords([]byte(bad)); err == nil {
			t.Errorf("%q was read as an answer: %+v", bad, got)
		}
	}
}

// The model is chosen by what it DOES. The catalog is the truth about a server,
// and a settings box naming a model it does not have would be a second answer
// that can only disagree.
func TestTheAlignerIsAskedForByTask(t *testing.T) {
	src := readSrc(t, "align.go")
	if !strings.Contains(src, `m.Task == "align"`) {
		t.Error("the aligner is picked by something other than the task it performs")
	}
	// a list, not a choice: a server lists aligners it cannot serve, and the
	// working one must not be left untried behind a broken one
	if !strings.Contains(src, "func (a *App) alignModels() []string") {
		t.Error("only one aligner is ever tried")
	}
	if !strings.Contains(src, "a.alignPick = model") {
		t.Error("the aligner that answered is not remembered, so every source pays to find it again")
	}
	// the setting, when there is one, and the catalog otherwise: a server can
	// serve two aligners, and it can serve one the engine will not load
	if !strings.Contains(src, "a.readConf().AlignModel); want != \"\"") {
		t.Error("a named aligner is ignored, so a server with two cannot be steered")
	}
	if !strings.Contains(src, "does not serve %q for alignment") {
		t.Error("a named aligner the server does not have fails silently")
	}
	// and no aligner is not an error: the edges fall back to the envelope
	// no aligner is not a failure anywhere: the ASR's own times stand
	pipe := readSrc(t, "pipeline.go")
	if !strings.Contains(pipe, "if models := a.alignModels(); len(models) > 0 {") {
		t.Error("the aligner is not asked for in the inputs stage, where the segments need it")
	}
	if !strings.Contains(pipe, "the ASR's own times stand") {
		t.Error("an aligner that fails takes the step down with it")
	}
	body := funcBody(t, "retake.go", `func \(a \*App\) findRetakes\(`)
	if strings.Contains(body, "a.alignEdges(") {
		t.Error("the joins are aligned one at a time again -- the times they are built from " +
			"are already the aligner's, timed once in Prepare")
	}
}

// Settings says which aligner will be used, and has no box to type one into.
//
// Every other model on that page is asked for by id, because the request
// carries one. This one is asked for by task, so the catalog is the whole of
// the answer and a box beside it could only disagree with the server — the
// same deal the drawing server gets.
func TestSettingsNamesTheAlignerRatherThanAskingForOne(t *testing.T) {
	src := readSrc(t, "setup.go")
	if !strings.Contains(src, `grid.Attach(lbl("Forced aligner:")`) {
		t.Error("Settings does not say which model places the cut points")
	}
	if !strings.Contains(src, "testAligner(url, k, id)") {
		t.Error("the aligner row has no Test")
	}
	// a box that is EMPTY and not blank: the preferred aligner is a
	// placeholder, never text, because a name written into the box is a name
	// the server is held to (alignModels), and no aligner at all is still a
	// working setup -- a filled box would put a red badge on a row allowed to
	// be empty
	if !strings.Contains(src, "alignModel := entry(c.AlignModel, defAlignModel+") {
		t.Error("the aligner row does not offer the preferred aligner as a placeholder")
	}
	if strings.Contains(src, "alignModel := entry(defAlignModel") ||
		strings.Contains(src, "c.AlignModel = or(c.AlignModel") {
		t.Error("the aligner box arrives with a name already IN it, which holds every " +
			"server to that one id")
	}

	// and it says what it means when there is none, rather than failing: the
	// waveform still places every join that has a silence in it
	body := funcBody(t, "setup.go", `func testAligner\(`)
	if !strings.Contains(body, `is declared task %q, and cannot align`) {
		t.Error("a box naming a model that does something else is accepted")
	}
	for _, want := range []string{"no model on %s does forced alignment", "waveform"} {
		if !strings.Contains(body, want) {
			t.Errorf("a server with no aligner is not explained: want %q", want)
		}
	}
	// two aligners is a warning, not a silent pick
	if !strings.Contains(body, "the first is used") {
		t.Error("with two aligners registered, nothing says which one answers")
	}
	// ...and "first" is the run's own order, not the alphabet
	if !strings.Contains(body, "alignOrder(ids)") {
		t.Error("Test orders the aligners its own way, so it can name a different " +
			"first than the run will use")
	}
}

// Which aligner an empty box means: the one this stack is built around, where
// the server has it. The stack registers two -- mms-aligner and qwen3-aligner
// -- and the list used to be plain sort.Strings, so every machine with both
// aligned through the MMS one because "mms" sorts first. That is picking the
// aligner by alphabet, and the aligner is what decides whether a cut lands on
// the word or 200 ms into it.
func TestThePreferredAlignerIsTriedFirst(t *testing.T) {
	ids := []string{"whisperx-align", "mms-aligner", defAlignModel}
	alignOrder(ids)
	if ids[0] != defAlignModel {
		t.Errorf("the aligners are tried %v, want %q first", ids, defAlignModel)
	}
	if ids[1] != "mms-aligner" || ids[2] != "whisperx-align" {
		t.Errorf("the rest are not in name order: %v -- one server has to try them the "+
			"same way every run", ids)
	}
	// a preference and not a rule: a server without it still aligns, through
	// whatever it does have
	other := []string{"whisperx-align", "mms-aligner"}
	alignOrder(other)
	if other[0] != "mms-aligner" {
		t.Errorf("a server with no %s aligns through %v, want mms-aligner first", defAlignModel, other)
	}
	// and nothing writes the preference into the settings file, which would
	// hold such a server to a name it cannot answer to
	if (appConf{}).withDefaults().AlignModel != "" {
		t.Error("withDefaults names an aligner; no aligner at all is a working setup")
	}
}

// The times come from one place, whichever answered.
//
// The ASR is asked what was said and the aligner when — and the second answer
// wins, because it is better by an order of magnitude: nemotron's stamps sit
// 0.29 s behind the sound where the aligner's sit 0.02 s. That split is also
// what lets a timing-less ASR be used at all, which the best one is.
func TestWordTimesPreferTheAlignerAndFallBackToTheASR(t *testing.T) {
	dir := t.TempDir()
	os.WriteFile(filepath.Join(dir, "words.json"), []byte(
		`{"words":[{"word":"art","start_sample":16000,"end_sample":24000}]}`), 0o644)
	got := wordTimes(dir)
	if len(got) != 1 || got[0].Start != 16000 {
		t.Fatalf("with only the ASR's answer, wordTimes gave %+v", got)
	}
	os.WriteFile(alignedWords(dir), []byte(
		`{"words":[{"word":"art","start_sample":12000,"end_sample":20000}]}`), 0o644)
	got = wordTimes(dir)
	if len(got) != 1 || got[0].Start != 12000 {
		t.Fatalf("the aligner's answer did not win: %+v", got)
	}
	// an aligned file with nothing in it is not an answer, and must not shadow
	// the one that is
	os.WriteFile(alignedWords(dir), []byte(`{"words":[]}`), 0o644)
	if got := wordTimes(dir); len(got) != 1 || got[0].Start != 16000 {
		t.Errorf("an empty aligned file hid the ASR's times: %+v", got)
	}
	if got := wordTimes(t.TempDir()); got != nil {
		t.Errorf("a source with no answer at all gave %+v", got)
	}
	// it is written beside the ASR's answer and never over it: words.json is
	// what the server said, and a file we wrote is not
	if filepath.Base(alignedWords(dir)) == "words.json" {
		t.Error("the aligner writes over the ASR's own answer")
	}
}

// A transcript is built from word times, so an ASR that returns none and no
// aligner to supply them is not an empty transcript — it is a setup that
// cannot work, and it has to say so.
func TestATimelessASRWithNoAlignerSaysSo(t *testing.T) {
	body := funcBody(t, "pipeline.go", `func \(a \*App\) mergeSegments\(`)
	for _, want := range []string{"timed no words", `register one (task \"align\")`} {
		if !strings.Contains(body, want) {
			t.Errorf("mergeSegments does not explain a timing-less ASR: want %q", want)
		}
	}
	// and silence is still silence: no words and no text is an empty
	// transcript, which is what a screen capture with no microphone is
	if !strings.Contains(body, `strings.TrimSpace(string(b)) != ""`) {
		t.Error("a recording with no speech is now treated as a broken setup")
	}
}

// What an alignment failure costs depends on what the ASR gave.
//
// With its own word times, alignment is an improvement and losing it is a
// warning. With none — which is what the best transcriber answers — it is the
// only source of times there is, and a transcript is built from times. Then it
// has to fail where it failed, carrying the aligner's own words: one run died
// two steps later saying "no aligner is registered" about a server with two of
// them, because it had run out of memory on the longest take.
func TestAFailedAlignmentFailsWhereItFailed(t *testing.T) {
	src := readSrc(t, "pipeline.go")
	if !strings.Contains(src, "if len(wordTimes(out)) > 0 {") {
		t.Error("an alignment failure costs the same whether or not the ASR timed anything")
	}
	for _, want := range []string{
		"answers with no word times of its own", // says which model, and why it matters
		"this recording cannot be ",             // and what that means for this recording
	} {
		if !strings.Contains(src, want) {
			t.Errorf("the failure does not explain itself: want %q", want)
		}
	}
	// ...and the message at the far end stops claiming there is none when the
	// truth is that one failed
	body := funcBody(t, "pipeline.go", `func \(a \*App\) mergeSegments\(`)
	if !strings.Contains(body, "if len(a.alignModels()) > 0 {") {
		t.Error("mergeSegments reports a registered aligner as a missing one")
	}
}

// One request's memory is the recording's length times a constant, so a long
// take is not slow, it is impossible: the run that prompted this died on a 96 s
// take with "failed to allocate Qwen3 ASR thinker classification graph" while
// 30 s and 45 s of the same audio answered fine. Both halves have to be bounded
// -- the reading and the timing -- or the other one dies instead.
func TestOneRequestCannotBeAskedForTheWholeTake(t *testing.T) {
	// the two passes chunk by the same number, which is what lets a piece
	// carry its own words from one to the other (alignPieces)
	if alignChunkMax != asrChunkQwen {
		t.Errorf("the aligner chunks by %gs and the ASR by %gs; the pieces have to line up",
			alignChunkMax, asrChunkQwen)
	}
	if asrChunkQwen > 90 {
		t.Errorf("asrChunkQwen is %gs; a 96s take is what failed", asrChunkQwen)
	}
	// 60 s was measured not to come back on the stack this was written on, so
	// the ceiling is only ever a starting point: what will not go in one
	// request is halved until it does
	body := funcBody(t, "align.go", `func \(a \*App\) alignSpanAt\(`)
	if !strings.Contains(body, "noRoom(err)") || !strings.Contains(body, "alignHalves") {
		t.Error("an align request too big for the machine is not halved and retried")
	}
	if !strings.Contains(funcBody(t, "align.go", `func alignPieces\(`), "asrCuts(") {
		t.Error("a recording with no ASR split is sent whole")
	}
	// the ASR's limit follows the model, not the machine: Nemotron reads five
	// minutes whole and Qwen3 cannot, and making them share a number costs one
	// of them
	if !strings.Contains(funcBody(t, "pipeline.go", `func \(a \*App\) asrLong\(`), "a.asrChunk()") {
		t.Error("asrLong uses one chunk size for every model")
	}
}

// Every stretch sent to the aligner carries the words the ASR heard in exactly
// those seconds. Nothing counts, nothing estimates, nothing is handed on.
func TestEveryPieceGetsTheWordsTheASRHeardInIt(t *testing.T) {
	text := strings.Fields("one two three four five six seven eight nine ten")
	pieces := []asrPiece{
		{S: 0, E: 20, Text: "one two three"},
		{S: 20, E: 40, Text: "four five six seven"},
		{S: 40, E: 60, Text: "eight nine ten"},
	}
	got := alignPieces(pieces, text, 60, nil)
	if len(got) != 3 {
		t.Fatalf("%d stretches from 3 ASR pieces", len(got))
	}
	for i, want := range [][]string{{"one", "two", "three"},
		{"four", "five", "six", "seven"}, {"eight", "nine", "ten"}} {
		if strings.Join(got[i].words, " ") != strings.Join(want, " ") {
			t.Errorf("stretch %d holds %q, want %q", i, got[i].words, want)
		}
		if got[i].s != pieces[i].S || got[i].e != pieces[i].E {
			t.Errorf("stretch %d covers %.0f..%.0f, want %.0f..%.0f",
				i, got[i].s, got[i].e, pieces[i].S, pieces[i].E)
		}
	}
	// pieces that do not account for the transcript are not trusted at all: a
	// mapping off by one piece puts every word after it in the wrong place,
	// which is worse than having no mapping
	short := alignPieces([]asrPiece{{S: 0, E: 60, Text: "one two"}}, text, 60, nil)
	if len(short) == 1 && len(short[0].words) == 2 {
		t.Error("a piece list missing eight words was used as the split anyway")
	}
}

// The bug this whole shape exists to stop. The aligner used to be told a window
// holds four words a second and to carry the leftovers on; a speaker saying
// 1.85 handed two windows 89 and 100 words for twenty seconds holding about 37
// each, and the transcript ran out with 38 s of audio left to place. That audio
// had no word over it, so the cut deleted it as footage with nothing said.
//
// Whatever the share-out does, every word has to land somewhere and the last
// stretch has to be reached.
func TestTheWordsNeverRunOutBeforeTheAudioDoes(t *testing.T) {
	text := make([]string, 570) // the take that broke: 570 words, 308.5 s
	for i := range text {
		text[i] = fmt.Sprintf("w%d", i)
	}
	// talking throughout, with the pauses a lecture has
	quiet := []span{{s: 0, e: 1.3}, {s: 32.8, e: 34}, {s: 86.8, e: 90.6},
		{s: 120.8, e: 122}, {s: 162, e: 163.2}, {s: 203.6, e: 204.4},
		{s: 227, e: 227.8}, {s: 277.5, e: 278.2}, {s: 306.6, e: 308.5}}
	got := alignPieces(nil, text, 308.5, quiet)
	if len(got) < 2 {
		t.Fatalf("308.5 s came back as %d stretches", len(got))
	}
	at := 0
	for i, sp := range got {
		for _, w := range sp.words {
			if w != text[at] {
				t.Fatalf("stretch %d starts on %q, want %q -- the words are out of step", i, w, text[at])
			}
			at++
		}
	}
	if at != len(text) {
		t.Errorf("%d of %d words were placed; %d had nowhere to go", at, len(text), len(text)-at)
	}
	if n := len(got[len(got)-1].words); n == 0 {
		t.Error("the last stretch of the recording got no words -- the text ran out first")
	}
}

// Halving a stretch the server cannot hold divides its words with it. The cost
// is precision inside that stretch; it must not be a word going missing, and it
// must not be a cut that cannot be halved again.
func TestAHalvedStretchKeepsAllOfItsWords(t *testing.T) {
	words := strings.Fields("a b c d e f g h i j k l")
	quiet := []span{{s: 28, e: 32}}
	lo, hi := splitSpan(alignSpan{s: 0, e: 60, words: words}, quiet)
	if len(lo.words)+len(hi.words) != len(words) {
		t.Errorf("halves hold %d and %d of %d words", len(lo.words), len(hi.words), len(words))
	}
	if strings.Join(append(append([]string{}, lo.words...), hi.words...), " ") != strings.Join(words, " ") {
		t.Error("the halves do not read as the whole")
	}
	if lo.e != hi.s {
		t.Errorf("the halves meet at %.1f and %.1f", lo.e, hi.s)
	}
	if lo.e != 30 {
		t.Errorf("the cut went to %.1f s, want 30 -- the middle of the silence beside it", lo.e)
	}
	// and never onto an edge, or halving a stretch would not make it smaller
	for _, c := range []struct {
		what  string
		quiet []span
	}{{"a silence at the very start", []span{{s: 0, e: 1}}},
		{"a silence at the very end", []span{{s: 59, e: 60}}},
		{"no silence at all", nil}} {
		a, b := splitSpan(alignSpan{s: 0, e: 60, words: words}, c.quiet)
		if a.e-a.s < 1 || b.e-b.s < 1 {
			t.Errorf("%s: halves of %.1f s and %.1f s", c.what, a.e-a.s, b.e-b.s)
		}
	}
}

// A forced aligner spreads the text it is given across the audio it is given,
// and it cannot decline. Hand it a stretch that is half silence and it puts
// words in the silence: one recording ran 13.5 s past the last thing said --
// the camera left rolling after the take -- and its last four words came back
// at 30.9 s of a 31.2 s file, 80 ms each, when they had been said before 17.8.
// The page then drew a 2.4 s pause and a 13 s pause into one unbroken sentence,
// and the cut placed its boundaries against them.
func TestTheAlignerIsNeverHandedSilence(t *testing.T) {
	// the recording that broke: speech to 17.79, then nothing to the end -- and
	// the end as silencedetect measures it, a microsecond short of the length
	// ffprobe reports, which is what defeated the first attempt at this
	quiet := []span{{s: 0, e: 1.67}, {s: 17.79, e: 31.253312}}
	for _, c := range []struct {
		what   string
		t0, t1 float64
		s0, s1 float64
		any    bool
	}{
		{"the window with the tail in it", 15.6, 31.253313, 15.6, 18.04, true},
		{"the window that opens in silence", 0, 15.6, 1.42, 15.6, true},
		{"a window of nothing at all", 20, 31.253313, 0, 0, false},
		{"a window that is all speech", 5, 10, 5, 10, true},
	} {
		s0, s1, any := soundSpan(quiet, c.t0, c.t1)
		if any != c.any {
			t.Errorf("%s: has sound = %v, want %v", c.what, any, c.any)
			continue
		}
		if any && (math.Abs(s0-c.s0) > 1e-9 || math.Abs(s1-c.s1) > 1e-9) {
			t.Errorf("%s: sends %.2f..%.2f, want %.2f..%.2f", c.what, s0, s1, c.s0, c.s1)
		}
	}
	// the padding is real: a word fades below the threshold before it is over,
	// and a window cut on the threshold clips the very word this exists to keep
	if alignSoundPad <= 0 {
		t.Error("a window is cut exactly on the silence threshold")
	}
	// ...and the times that come back are offset by what was SENT, not by the
	// window it was cut from -- the one arithmetic slip that would move every
	// word of every window
	body := funcBody(t, "align.go", `func \(a \*App\) alignSpanAt\(`)
	for _, want := range []string{"soundSpan(quiet, sp.s, sp.e)", "(w.s + s0) * sampleRate", "-ss\", fmt.Sprint(s0)"} {
		if !strings.Contains(body, want) {
			t.Errorf("the window's own start is not carried through: want %q", want)
		}
	}
}
