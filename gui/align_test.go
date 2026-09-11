package main

// The aligner's answer, read whatever shape it arrives in.
//
// Two aligners are in play -- a wav2vec2 CTC one and a transformer one -- and
// the app asks for the TASK rather than for either of them, so it has to read
// what both write. What they agree on is a list of words with two numbers each;
// what they do not agree on is what the list is called or what unit the numbers
// are in, and getting the unit wrong puts a cut somewhere in the next decade.

import (
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
	// a box, empty by default: no aligner is a working setup, so a default
	// naming one would put a red badge on a row allowed to be empty
	if !strings.Contains(src, `alignModel := entry(c.AlignModel, ""`) {
		t.Error("the aligner row has no box, or the box arrives with a name already in it")
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
	if alignChunkMax > 45 {
		t.Errorf("alignChunkMax is %gs; 60s did not come back on the stack this was measured on", alignChunkMax)
	}
	if asrChunkQwen > 90 {
		t.Errorf("asrChunkQwen is %gs; a 96s take is what failed", asrChunkQwen)
	}
	body := funcBody(t, "align.go", `func \(a \*App\) alignInput\(`)
	if !strings.Contains(body, "asrCuts(") {
		t.Error("alignment sends the whole recording in one request")
	}
	// the ASR's limit follows the model, not the machine: Nemotron reads five
	// minutes whole and Qwen3 cannot, and making them share a number costs one
	// of them
	if !strings.Contains(funcBody(t, "pipeline.go", `func \(a \*App\) asrLong\(`), "a.asrChunk()") {
		t.Error("asrLong uses one chunk size for every model")
	}
}

// A window's text has to end where the audio does, and the only handle on that
// is the answer. Counting it works until a family answers in phrases instead of
// words, and then every window after the first is reading the wrong text.
func TestTheNextWindowStartsOnTheRightWord(t *testing.T) {
	text := strings.Fields("the whole run took roughly nine minutes and the cuts are better")
	for _, c := range []struct {
		name string
		got  []alignWord
		want int
	}{
		{"one word per word", []alignWord{{w: "the"}, {w: "whole"}, {w: "run"}}, 3},
		{"punctuation and case", []alignWord{{w: "The"}, {w: "whole"}, {w: "run,"}}, 3},
		{"phrases", []alignWord{{w: "the whole run"}, {w: "took roughly"}}, 5},
		{"a word it could not place", []alignWord{{w: "the"}, {w: "xxxx"}, {w: "run"}}, 3},
		{"nothing it was given", []alignWord{{w: "zzz"}, {w: "qqq"}}, 2},
		{"more than the text", []alignWord{{w: "zzz"}}, 1},
	} {
		if got := consumed(text, c.got); got != c.want {
			t.Errorf("%s: consumed %d of the text, want %d", c.name, got, c.want)
		}
	}
	if got := consumed([]string{"one"}, []alignWord{{w: "zzz"}, {w: "qqq"}}); got != 1 {
		t.Errorf("consumed %d words of a one-word text", got)
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
	body := funcBody(t, "align.go", `func \(a \*App\) alignInput\(`)
	for _, want := range []string{"soundSpan(quiet, t0, t1)", "(w.s + s0) * sampleRate", "-ss\", fmt.Sprint(s0)"} {
		if !strings.Contains(body, want) {
			t.Errorf("the window's own start is not carried through: want %q", want)
		}
	}
}
