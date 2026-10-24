package main

// Time, as the finished video counts it.
//
// Every number this page prints about how long something is used to be read
// straight off the segment: end minus start, the session's own seconds. That
// answer stopped being true when speed effects arrived -- a stretch under a ×2
// is half as many seconds of video as it is of footage -- and the readings
// drifted apart in the one place it mattered most: the ▶✂ clock, which claims
// to be the finished video's own, while the preview beside it was already
// playing at the effect's rate.
//
// What is pinned here is the arithmetic and who uses it: a speed effect moves
// the number, a stop does not (the picture stands still and the footage under
// it plays on, so a stop costs the video nothing), and the clock, the total and
// the status lines all read the same pipe the render does.

import (
	"math"
	"strings"
	"testing"
)

func runEd(t *testing.T) *cutEditor {
	t.Helper()
	ed := newTestEd(t)
	ed.vids = []tlVideo{{base: "a", path: "a.mkv", start: 0, dur: 300, interval: 5, fps: 30}}
	ed.relayout()
	ed.segs = []cutSeg{{S: 0, E: 100}}
	return ed
}

func TestRunLenCountsTheSpeedAndNotTheStop(t *testing.T) {
	ed := runEd(t)
	// no effects: the span is its own seconds
	if got := ed.runLen(10, 30); math.Abs(got-20) > 1e-9 {
		t.Errorf("a plain 20 s span runs %g s", got)
	}
	// ×2 over ten of those twenty seconds: five seconds of video for ten of
	// footage, the other ten untouched
	ed.fx = []cutFx{{Kind: "speed", T: 10, Dur: 10, Rate: 2}}
	if got := ed.runLen(10, 30); math.Abs(got-15) > 1e-9 {
		t.Errorf("10 s at ×2 and 10 s plain runs %g s, want 15", got)
	}
	// half speed the other way, and a span that only clips the effect
	ed.fx = []cutFx{{Kind: "speed", T: 10, Dur: 10, Rate: 0.5}}
	if got := ed.runLen(10, 20); math.Abs(got-20) > 1e-9 {
		t.Errorf("10 s at ×0.5 runs %g s, want 20", got)
	}
	if got := ed.runLen(15, 25); math.Abs(got-15) > 1e-9 {
		t.Errorf("5 s at ×0.5 and 5 s plain runs %g s, want 15", got)
	}
	// a stop is the ×0 in the same arithmetic and costs the video nothing:
	// the still stands over footage that keeps running (cut_fxstill.go)
	ed.fx = []cutFx{{Kind: "speed", T: 10, Dur: 10, Rate: 0}}
	if got := ed.runLen(10, 30); math.Abs(got-20) > 1e-9 {
		t.Errorf("a 10 s stop inside a 20 s span runs %g s, want the 20 it always did", got)
	}
	// and nothing at all for an empty or backwards span
	if got := ed.runLen(30, 30) + ed.runLen(30, 10); got != 0 {
		t.Errorf("an empty span runs %g s", got)
	}
}

// The status lines say both numbers when they differ, and only one when they
// do not: a cut with no effects reads exactly as it always did.
func TestSpanSecsNamesTheVideosSecondsOnlyWhenTheyDiffer(t *testing.T) {
	ed := runEd(t)
	if got := ed.spanSecs(10, 30); got != "20.0 s" {
		t.Errorf("a plain span reads %q", got)
	}
	ed.fx = []cutFx{{Kind: "speed", T: 10, Dur: 20, Rate: 2}}
	got := ed.spanSecs(10, 30)
	if !strings.Contains(got, "20.0 s") || !strings.Contains(got, "10.0 s in the video") {
		t.Errorf("a span at ×2 reads %q, want both its own seconds and the video's", got)
	}
}

// The ▶✂ clock is the finished video's, so it counts in the video's seconds --
// which is what it was not doing: a line halfway through a ×2 read the session
// seconds it had walked through, so the number ran ahead of the picture.
func TestTheCutClockReadsThroughTheEffects(t *testing.T) {
	ed := runEd(t)
	ed.segs = []cutSeg{{S: 0, E: 40}, {S: 60, E: 100}}
	ed.fx = []cutFx{{Kind: "speed", T: 0, Dur: 40, Rate: 2}}

	// the first clip is 40 s of footage at ×2: 20 s of video, and the second
	// clip starts there rather than at 40
	if got := ed.cutLen(); math.Abs(got-60) > 1e-9 {
		t.Errorf("the cut is %g s of video, want 60 (20 at ×2 + 40 plain)", got)
	}
	if got := ed.cutPos(20); math.Abs(got-10) > 1e-9 {
		t.Errorf("20 s of session inside the ×2 reads %g s into the video, want 10", got)
	}
	if got := ed.cutPos(60); math.Abs(got-20) > 1e-9 {
		t.Errorf("the second clip starts %g s into the video, want 20", got)
	}
	if got := ed.cutPos(80); math.Abs(got-40) > 1e-9 {
		t.Errorf("halfway through the second clip reads %g s, want 40", got)
	}
	// with no effects at all it is the plain sum, as it has always been
	ed.fx = nil
	if got, want := ed.cutLen(), 80.0; math.Abs(got-want) > 1e-9 {
		t.Errorf("a cut with no effects is %g s, want %g", got, want)
	}
	if got := ed.cutPos(80); math.Abs(got-60) > 1e-9 {
		t.Errorf("without effects the clock reads %g s, want 60", got)
	}
}

// The one pipe: the clock, the total and the status lines all read the cut the
// way the render will run it, so they cannot disagree with each other or with
// the video.
func TestEveryTimeOnThePageReadsTheSamePipe(t *testing.T) {
	src := readSrc(t, "cut.go")
	for _, want := range []string{
		"func (ed *cutEditor) fxSegs() []cutSeg { return applyFx(splitSpliced(ed.segs), ed.fx) }",
		"for _, s := range ed.fxSegs() {",
		"t = ed.cutPos(ed.playhead)",
		"ed.spanSecs(s.S, s.E)",
	} {
		if !strings.Contains(src, want) {
			t.Errorf("cut.go no longer contains %q", want)
		}
	}
	// the removal says how much shorter the VIDEO is, which is the number the
	// total under the tracks moved by
	rm := readSrc(t, "cut_edit.go")
	if !strings.Contains(rm, "a.setStatus(removedMsg(was-ed.cutLen(), before, len(ed.segs)))") {
		t.Error("－ Remove reports session seconds again, which the total beside it will not agree with")
	}
	// and the model is measured against its target the same way
	sg := readSrc(t, "cut_suggest.go")
	for _, want := range []string{
		"mmss(cutLen(applyFx(segs, fx))), mmss(raw))", // what the speed pass reports
	} {
		if !strings.Contains(sg, want) {
			t.Errorf("cut_suggest.go no longer contains %q", want)
		}
	}
}

// Both lengths, because both are asked about: how much footage the cut keeps,
// and how long the video made of it runs. A cut with a minute at 4 is fifteen
// seconds of video, and a page showing only one of those numbers cannot say
// whether a target was missed by keeping too much or by speeding too little.
func TestThePageSaysTheCutWithAndWithoutTheSpeed(t *testing.T) {
	ed := newTestEd(t)
	ed.segs = []cutSeg{{S: 0, E: 60}, {S: 100, E: 160}}
	ed.fx = []cutFx{{Kind: "speed", T: 100, Dur: 60, Rate: 4}}
	if got := ed.rawLen(); got != 120 {
		t.Errorf("the footage kept measures %g s, want 120", got)
	}
	if got := ed.cutLen(); math.Abs(got-75) > 0.01 {
		t.Errorf("the video runs %g s, want 75 — 60 at 1 and 60 at 4", got)
	}
	// a card takes no session time and still runs for its own length, in both
	ed.segs = append(ed.segs, cutSeg{S: 200, E: 200, Ins: "card.svg", Dur: 5})
	if got := ed.rawLen(); got != 125 {
		t.Errorf("a 5 s card left the kept footage at %g s, want 125", got)
	}
	// and both are on the page, named apart
	src := readSrc(t, "cut.go")
	for _, want := range []string{
		`ed.formIdle.Append(idleRow("Cut", ed.total))`,
		`ed.formIdle.Append(idleRow("Cut at 1×", ed.totalRaw))`,
		"ed.totalRaw.SetText(mmss(ed.rawLen()))",
	} {
		if !strings.Contains(src, want) {
			t.Errorf("the page no longer reads both lengths: %q", want)
		}
	}
}
