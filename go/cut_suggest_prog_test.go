package main

// Suggest cut is two calls that think for minutes each, and the bar over them
// pulsed for the whole run: alive, but saying nothing about how far along it
// was. Both replies are countable while they arrive -- the segments and the
// checks close one object at a time -- so the bar can say something true.
//
// The denominators are different, and that is the interesting part. The audit
// has one: it was asked for one check per proposed segment. Choosing has none,
// because how many moments a session has is the model's decision. What it has
// instead is order: the timeline is walked from the front, so the end time of
// the last finished segment is how far through the session, and through the
// job, it has got.

import (
	"fmt"
	"math"
	"os"
	"strings"
	"testing"
)

func TestASegmentCountsOnlyOnceItIsWritten(t *testing.T) {
	// a reply as it arrives, piece by piece
	for _, c := range []struct {
		name   string
		in     string
		want   int
		though float64
	}{
		{"nothing yet", `Let me think about which moments matter.`, 0, 0},
		{"the key, no items", `{"segments":[`, 0, 0},
		{"one half-written", `{"segments":[{"start":10,"end":`, 0, 0},
		{"one closed", `{"segments":[{"start":10,"end":48}`, 1, 48},
		{"two closed, one open", `{"segments":[{"start":10,"end":48},{"start":90,"end":140},{"start":300`, 2, 140},
		{"whole reply", `{"segments":[{"start":10,"end":48},{"start":90,"end":140}]}`, 2, 140},
	} {
		n, through := jsonItemsDone(c.in, "segments")
		if n != c.want || through != c.though {
			t.Errorf("%s: %d segment(s) up to %gs, want %d up to %gs", c.name, n, through, c.want, c.though)
		}
	}
}

// The model thinks out loud before it answers, and it thinks about the shape of
// the answer -- so the words and the punctuation of the real thing are in there
// first. The count belongs to the last one, or a bar reads a worked example in
// the reasoning as the work.
func TestTheCountBelongsToTheAnswerNotTheThinkingAboutIt(t *testing.T) {
	s := `I should return {"segments":[{"start":0,"end":30}]} shaped output.` +
		` Something like {"segments": [ {"start":1,"end":2} ] } is the format.` +
		"\n" + `{"segments":[{"start":600,"end":650},{"start":900,"end":960}]}`
	n, through := jsonItemsDone(s, "segments")
	if n != 2 || through != 960 {
		t.Errorf("counted %d up to %gs, want the 2 of the real answer up to 960s", n, through)
	}
}

// The audit answers a different shape with the same arithmetic, and its objects
// carry prose that is full of the punctuation the scanner reads.
func TestTheAuditIsCountedTheSameWay(t *testing.T) {
	s := `{"checks":[` +
		`{"i":1,"verdict":"ok","start":10,"end":48,"why":""},` +
		`{"i":2,"verdict":"fix","start":90,"end":175,"why":"ends before the chest is opened {see 2:55}"},` +
		`{"i":3,"verdict":"drop"`
	n, through := jsonItemsDone(s, "checks")
	if n != 2 {
		t.Errorf("counted %d finished checks, want 2 -- the third is still being written", n)
	}
	if through != 175 {
		t.Errorf("the last finished check reaches %gs, want 175", through)
	}
}

// Narration entries have no end time in them, and the bar over the writing
// counts clips instead. Sharing the scanner must not have cost that.
func TestNarrationStillCountsWithoutAnEndTime(t *testing.T) {
	s := `{"entries":[{"i":1,"text":"here we go"},{"i":2,"text":"and \"then\" {this}"}]}`
	if got := narrEntriesDone(s); got != 2 {
		t.Errorf("narrEntriesDone read %d clips, want 2", got)
	}
	if _, through := jsonItemsDone(s, "entries"); through != 0 {
		t.Errorf("clips with no end time placed themselves at %gs", through)
	}
}

// The wiring, which needs a display and an LLM to run and so is read instead:
// both calls streamed, both reporting, and the pulse ending on the first real
// fraction rather than on a flag -- Pulse and SetFraction drive the same needle.
func TestSuggestReportsWhileItRunsInsteadOfOnlyPulsing(t *testing.T) {
	b, err := os.ReadFile("cut_suggest.go")
	if err != nil {
		t.Fatal(err)
	}
	src := string(b)
	for _, want := range []struct{ frag, why string }{
		{`a.llmChatRetryTools("suggest", msgs, think, tools, a.webRunner("suggest", ffx), onText)`,
			"a call that is not streamed cannot be counted while it runs"},
		{`jsonItemsDone(s, "segments")`, "choosing has to count its own segments"},
		{`a.progParts[trackSTT] > 0`,
			"the pulse has to end on the first real fraction, or it fights the bar it shares"},
	} {
		if !strings.Contains(src, want.frag) {
			t.Errorf("suggest no longer contains %q -- %s", want.frag, want.why)
		}
	}
	// one streamed call: the cut is the long one and the only one with
	// anything to count while it runs; the two passes after it answer in a
	// minute from a brief of a few clips
	if strings.Count(src, ", onText)") != 1 {
		t.Errorf("%d suggest calls are streamed, want the cut alone",
			strings.Count(src, ", onText)"))
	}
	// the cut's share leaves room for the passes after it, or a finished run stops short
	if suggestChooseShare <= 0 || suggestChooseShare >= 1 {
		t.Errorf("suggestChooseShare is %g -- the cut owns all of the bar or none of it", suggestChooseShare)
	}
}

// The target crosses the wire as a RANGE, not a number.
//
// The wording asks for a total near the target and suggestWindow is what the
// gate actually accepts, and for a long time only the number was sent: a model
// told "300 seconds" treats 300 as the answer and spends the call trying to
// hit it exactly. One 11-minute call came back with 85 kB of arithmetic --
// "Total 134!! Over 100 by 34. I keep ballooning." -- and no JSON at all. Both
// numbers now come from the one function the gate uses, so the prompt and the
// gate cannot drift apart again.
func TestTheModelIsToldTheRangeItWillBeJudgedBy(t *testing.T) {
	src := readSrc(t, "cut_suggest.go")
	for _, want := range []string{
		"lo, hi := a.footageWindow(target)",
		`"between %.0f and %.0f seconds of footage, in at most %d segments. Stop at the "`,
		"cutLen(applyFx(segs, fx))", // and what the video RUNS is measured the same way
	} {
		if !strings.Contains(src, want) {
			t.Errorf("cut_suggest.go no longer contains %q", want)
		}
	}
	// the wording's own check counts what the video RUNS, effects included --
	// the sum it used to ask for was raw seconds, which a speed effect makes a
	// different number from the one the gate measures
	cut := readSrc(t, "cut.go")
	for _, want := range []string{
		"the footage they come to, end minus start added up, lands in it",
		"Anywhere inside it is right; do not trim towards its middle.",
	} {
		if !strings.Contains(cut, want) {
			t.Errorf("the cut wording no longer says %q", want)
		}
	}
	if strings.Contains(cut, "within a tenth of the target length") {
		t.Error("the wording asks for a tenth of the target again, which is tighter " +
			"than the gate accepts — that difference is what the model spends the call on")
	}
	// and a retry stops searching: the moments are in the timeline it has
	if !strings.Contains(src, "tools = nil") {
		t.Error("a rejected attempt still has the web tools, and a retry that searches " +
			"is a retry that does not answer")
	}
}

// The other end of the same gate: an answer of hundreds of segments is not a
// long cut, it is a model that stopped choosing moments and started counting.
// One run answered with 548 of them -- five real, then a march of 24-second
// blocks every 32 seconds running eleven times past the end of the session --
// and nothing rejected it for its shape.
func TestARunawayAnswerIsRejectedForItsShape(t *testing.T) {
	if got := maxSuggestSegs(300); got < 3*(300/20) {
		t.Errorf("the ceiling for a 300 s target is %d, tighter than the wording's own guide", got)
	}
	if maxSuggestSegs(20) < 40 {
		t.Error("a Short's ceiling is under the floor, so a normal answer would be refused")
	}
	if maxSuggestSegs(3000) <= maxSuggestSegs(300) {
		t.Error("a longer target does not get more room")
	}
	src := readSrc(t, "cut_suggest.go")
	for _, want := range []string{
		"} else if n := maxSuggestSegs(target); len(out.Segments) > n {",
		`"%d segments, which is not a cut -- keep it under %d, "`,
		"maxSuggestSegs(target))", // and the model is told the ceiling
	} {
		if !strings.Contains(src, want) {
			t.Errorf("cut_suggest.go no longer contains %q", want)
		}
	}
	// and the wording says how a dull stretch is meant to be expressed, which
	// is the thing the runaway was a broken attempt at
	if !strings.Contains(readSrc(t, "cut.go"),
		"keep such a stretch whole, as ONE segment") {
		t.Error("the effect rules no longer say how to show a stretch without watching it")
	}
}

// A segment under a speed effect is allowed to be long, because that is what
// makes a dull stretch affordable: it costs the video its seconds divided by
// the rate, so two minutes at 4 spends thirty of the target. Without that the
// only way to show half a session inside a five-minute target is to cut
// between every line of speech, which is the answer that ran to 548 segments.
func TestTheWordingsAllowALongSegmentUnderSpeed(t *testing.T) {
	src := readSrc(t, "cut.go")
	if !strings.Contains(src, "keep such a stretch whole, as ONE segment") {
		t.Error("the cut's rules no longer say a stretch to be shown fast stays one segment")
	}
	// the lengths are guides and say so once, in the tail every wording ends
	// with: a model told 45 seconds as a limit packs a long stretch into a row
	// of small segments instead
	if strings.Contains(src, "8 to 45 seconds") {
		t.Error("a wording still states the old fixed 8-to-45-second segment")
	}
	if !strings.Contains(cutReply, "about 8 seconds to a minute, longer where a stretch has to be shown but not watched") {
		t.Error("nothing says how long a segment runs")
	}
	// said once, in the tail every wording ends with, and nowhere else
	if n := strings.Count(src, "8 seconds to a minute"); n != 1 {
		t.Errorf("the segment length is stated %d times in cut.go, want once (in cutReply)", n)
	}
}

// The speed pass is not handed a target. It was -- footage, target, a formula
// with a worked example, and a length gate on the answer -- and that made it
// speed clips up because the sum said so, whether or not anything in them was
// dull. What the editor wants is in the user context, in one of two shapes or
// in neither: a measure the model spends on the right clips, or nothing, in
// which case speed goes only where a viewer would otherwise sit through
// nothing. And the longer the nothing, the faster it runs.
func TestTheSpeedPassJudgesRatherThanSums(t *testing.T) {
	for _, want := range []string{
		"Where the USER CONTEXT says how much to speed up",
		"Where it says nothing, speed only what would bore",
		"a cut with nothing dull in it gets no rates at all",
		"The longer the dull stretch, the higher the rate",
	} {
		if !strings.Contains(speedSystem, want) {
			t.Errorf("the speed pass's wording no longer says %q", want)
		}
	}
	src := readSrc(t, "cut_suggest.go")
	// the request carries the footage and the clips, and no target or range
	if !strings.Contains(src, `"FOOTAGE: %.0f seconds over %d clips.\n\n"`) {
		t.Error("the speed request no longer says how much footage the clips come to")
	}
	for _, gone := range []string{"TARGET: %.0f seconds of finished video", "lo, hi := a.suggestWindow(target)\n\tuser :="} {
		if strings.Contains(src, gone) {
			t.Errorf("the speed pass is handed a target again: %q", gone)
		}
	}
	// and no length gate on its answer: "nothing runs fast" is a right answer
	body := funcBody(t, "cut_suggest.go", `func \(a \*App\) speedCut\(`)
	if strings.Contains(body, "is accepted") {
		t.Error("the speed pass's answer is gated on a length again")
	}
	if !strings.Contains(body, "return fx") {
		t.Error("the speed pass keeps no answer")
	}
}

// The length the run aims at is the one the user context names. It was a box
// on the Cut page as well, and for a week of runs the two disagreed -- the box
// quietly winning at 300 while the sentence beside it read 12 minutes, and
// every attempt refused for a length nobody had asked for.
func TestTheLengthComesFromTheUserContext(t *testing.T) {
	for _, c := range []struct {
		ctx  string
		want float64
		ok   bool
	}{
		{"Show half of the video (~12min), but speed the rest", 720, true},
		{"about 15 min of it", 900, true},
		{"keep it to 5 minutes", 300, true},
		{"a 90 s teaser", 90, true},
		{"you get a bonus at 500 wins, and level 2 is where it starts", 0, false},
		{"", 0, false},
	} {
		got, ok := ctxLength(c.ctx)
		if ok != c.ok || (ok && got != c.want) {
			t.Errorf("ctxLength(%q) = %g, %v; want %g, %v", c.ctx, got, ok, c.want, c.ok)
		}
	}
	src := readSrc(t, "cut_suggest.go")
	for _, want := range []string{
		"target := 0.0", // no length named is no length, not a default one
		"if want, ok := ctxLength(a.sessionCtx()); ok {",
		"the user context names no length",
		"named in the user context",
	} {
		if !strings.Contains(src, want) {
			t.Errorf("cut_suggest.go no longer contains %q", want)
		}
	}
	// and there is no second place to say it
	if strings.Contains(readSrc(t, "cut.go"), "ed.target") {
		t.Error("the target box is back on the Cut page's toolbar")
	}
}

// A session whose context names no length has no length.
//
// It used to fall back to 300 s and then tell the model, in the request, that
// 300 was "the length named in the user context" -- which nobody had named. A
// 12.7-minute script read out in full came back cut to a third of itself, the
// rest left to a speed pass that the same context had forbidden. The default
// is gone: no length named, no target, no range, and nothing to add up.
func TestNoLengthNamedMeansNoTarget(t *testing.T) {
	src := readSrc(t, "cut_suggest.go")
	for _, want := range []string{
		"NO TARGET LENGTH",                     // the request says so in as many words
		"if target > 0 {",                      // ...and the range paragraph is the other branch
		"target > 0 && (raw < lo || raw > hi)", // the gate has nothing to judge by
		"everything worth keeping goes in",     // and the log says which way it went
	} {
		if !strings.Contains(src, want) {
			t.Errorf("cut_suggest.go no longer contains %q", want)
		}
	}
	// the promise of a speed pass belongs to the target, not to every cut: it
	// is what makes a model leave a dull stretch in, and a session with no
	// target has no reason to hear it
	j := strings.Index(src, "NO TARGET LENGTH")
	k := strings.Index(src[j:], "if target > 0 {")
	if j < 0 || k < 0 || strings.Contains(src[j:j+k], "fast") {
		t.Error("the no-target request still promises the speed pass that makes a model " +
			"leave a dull stretch in")
	}
	// and the wording stops assuming there is always a range
	if !strings.Contains(readSrc(t, "cut.go"), "Given no range, there is nothing to add up") {
		t.Error("the cut wording still asks for a total against a range that may not exist")
	}
}

// The web tools are for a fact that gets written down, not for reading up on
// the session. One run spent 11m33s thinking, asked for three searches about
// the topics, and then did the whole 13 minutes of thinking again with the
// results in hand -- for a job whose answer is a list of numbers.
func TestTheToolsAreForFactsThatGetWrittenDown(t *testing.T) {
	sys := readSrc(t, "syscontext.go")
	for _, want := range []string{
		"about to WRITE DOWN",
		"never on a job whose answer is numbers rather than words",
		"the reasoning that led to it is done again from the start",
	} {
		if !strings.Contains(sys, want) {
			t.Errorf("the TOOLS section no longer says %q", want)
		}
	}
	if !strings.Contains(readSrc(t, "websearch.go"), "only when the user context asks for a detail you do not have") {
		t.Error("the tool's own description no longer defers to the user context")
	}
}

// The cut will not take out a silence, and the retake pass cannot: a man who
// stops talking for half a minute and carries on has said nothing twice, so
// there is no repeat to find. One run answered with five segments running end
// to end over the whole session -- not a cut at all -- and 31 s of nobody
// speaking mid-sentence went into the video with it.
func TestTheLongSilencesComeOutOfTheClips(t *testing.T) {
	// one clip with a 31 s hole in it, the shape that survived three runs
	talk := [][2]float64{{100, 162.5}, {193.9, 250}}
	segs := []cutSeg{{S: 100, E: 250}}
	gone := dropDeadAir(&segs, talk)
	if len(segs) != 2 {
		t.Fatalf("the hole left the clip in %d piece(s), want 2: %+v", len(segs), segs)
	}
	if math.Abs(segs[0].E-162.75) > 1e-9 || math.Abs(segs[1].S-193.65) > 1e-9 {
		t.Errorf("the clip breaks at %.2f and resumes at %.2f, want 162.75 and 193.65", segs[0].E, segs[1].S)
	}
	// a beat is left where it was, half on each side: a cut from the last word
	// straight to the next is a jump, and the pause was a breath before it
	// went on too long. In the VIDEO, which is the two clips played one after
	// the other -- the session seconds between them are the part that went.
	if left := (segs[0].E - 162.5) + (193.9 - segs[1].S); math.Abs(left-deadAirKeep) > 1e-9 {
		t.Errorf("%.2fs of the pause is left in the video, want %.2f", left, deadAirKeep)
	}
	if math.Abs(gone-(193.9-162.5-deadAirKeep)) > 1e-9 {
		t.Errorf("it reports %.2fs taken out, want %.2f", gone, 193.9-162.5-deadAirKeep)
	}

	// a pause between two sentences is not touched, whatever else is -- and
	// on a read to camera that pause is 3.7 s between WORDS, breath included,
	// which is what this measures. At 2.5 s this cut a session into
	// twenty-eight pieces.
	for _, pause := range []float64{1.9, 3.7, 5.0} {
		segs = []cutSeg{{S: 0, E: 60}}
		if gone := dropDeadAir(&segs, [][2]float64{{0, 29}, {29 + pause, 60}}); gone != 0 || len(segs) != 1 {
			t.Errorf("a %.1fs pause between sentences was cut out too: %+v", pause, segs)
		}
	}
	// ...and a card is not footage and has no silence in it to find
	segs = []cutSeg{{S: 5, E: 5, Ins: "a.png", Dur: 3}}
	if dropDeadAir(&segs, nil); len(segs) != 1 {
		t.Error("a spliced card was thrown away as dead air")
	}
	// the complement itself, which is where an off-by-one would put the breaks
	got := quietWithin([][2]float64{{10, 20}, {30, 40}}, 5, 50)
	want := [][2]float64{{5, 10}, {20, 30}, {40, 50}}
	if fmt.Sprint(got) != fmt.Sprint(want) {
		t.Errorf("the quiet inside 5..50 reads %v, want %v", got, want)
	}
	// and it runs where the suggestion is assembled, after the marks
	src := readSrc(t, "cut_suggest.go")
	if !strings.Contains(src, "dropDeadAir(&a.ed.segs, a.ed.talk)") {
		t.Error("the suggestion never has its silences taken out")
	}
}
