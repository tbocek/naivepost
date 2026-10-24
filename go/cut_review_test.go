package main

// The ▶✂✂ review: every join, reviewPad seconds either side, one after the
// other. The decision is pure (reviewStep), so the sequence is checked here
// without a player; the wiring that needs a display is checked as text.

import (
	"strings"
	"testing"
)

// Three clips, two joins. The line is walked through the review the way the
// tick would walk it and every move is the one the button promises.
func TestTheReviewHearsEveryJoinOnceInOrder(t *testing.T) {
	segs := []cutSeg{{S: 0, E: 60}, {S: 80, E: 140}, {S: 200, E: 260}}

	// the first join: the run-up begins reviewPad before it
	if from, to := reviewWindow(segs, 0); from != 50 || to != 90 {
		t.Errorf("seam 0 is heard %v–%v, want 50–90", from, to)
	}
	// still in the run-up, and in the removed stretch skipGap is about to
	// jump: the review stays out of the way
	for _, at := range []float64{50, 59.9, 65} {
		if i, m := reviewStep(segs, 0, at, true); i != 0 || m != reviewStay {
			t.Errorf("at %v the review moved (%d, %v); it should have stayed on seam 0", at, i, m)
		}
	}
	// the seconds after the join have played: seek to the next join's run-up
	if i, m := reviewStep(segs, 0, 90, true); i != 1 || m != reviewSeek {
		t.Errorf("at 90 the review did (%d, %v), want a seek to seam 1", i, m)
	}
	// ...and after that one, the review is over
	if i, m := reviewStep(segs, 1, 210, true); i != 1 || m != reviewDone {
		t.Errorf("at 210 the review did (%d, %v), want done", i, m)
	}
	// a hand that moved the line ends it rather than dragging the line back...
	if _, m := reviewStep(segs, 1, 20, true); m != reviewLost {
		t.Errorf("the line moved to 20 while seam 1 was under review and the review did %v, want lost", m)
	}
	// ...but only once the line has been seen in the window: before that a
	// line behind it is the review's own seek still landing, and the review
	// waits. It used to end itself on its first tick over exactly this.
	if _, m := reviewStep(segs, 1, 20, false); m != reviewStay {
		t.Errorf("a stale read of 20 before seam 1's seek landed did %v, want stay", m)
	}
}

// A review starts from the red line: inside a window it plays on from there,
// between windows it goes to the next join ahead, and past the last it wraps
// to the first. It used to start from the first join every time, so a review
// paused to look at a join could only be resumed by hearing every join
// before it again.
func TestTheReviewStartsFromTheRedLine(t *testing.T) {
	segs := []cutSeg{{S: 0, E: 60}, {S: 80, E: 140}, {S: 200, E: 260}}
	for _, c := range []struct {
		at   float64
		seam int
		seek bool
	}{
		{10, 0, true},   // before the first window: to its run-up
		{55, 0, false},  // inside it: play on from here
		{85, 0, false},  // just after the join, still its window
		{100, 1, true},  // between the windows: the next join ahead
		{135, 1, false}, // in the last window
		{250, 0, true},  // past everything: round to the first
	} {
		if i, seek := reviewSeamAt(segs, c.at); i != c.seam || seek != c.seek {
			t.Errorf("a review pressed at %v starts (%d, seek=%v), want (%d, seek=%v)", c.at, i, seek, c.seam, c.seek)
		}
	}
}

// A clip shorter than two pads has both its joins inside one stretch: it is
// played through once, not rewound for the second join. And a window never
// reaches into a removed stretch: it is clamped to the clips on both sides.
func TestShortClipsArePlayedThroughNotRewound(t *testing.T) {
	segs := []cutSeg{{S: 0, E: 60}, {S: 70, E: 75}, {S: 100, E: 160}}
	if from, to := reviewWindow(segs, 0); from != 50 || to != 75 {
		t.Errorf("seam 0 is heard %v–%v, want 50–75: the window stops at the short clip's end", from, to)
	}
	if from, to := reviewWindow(segs, 1); from != 70 || to != 110 {
		t.Errorf("seam 1 is heard %v–%v, want 70–110: the run-up cannot begin before the clip", from, to)
	}
	// at the short clip's end the first join is done and the second's run-up
	// is already behind the line: play on, no seek
	if i, m := reviewStep(segs, 0, 75, true); i != 1 || m != reviewStay {
		t.Errorf("at 75 the review did (%d, %v), want to stay on into seam 1", i, m)
	}
	// one clip is no join at all
	ed := &cutEditor{segs: segs[:1]}
	if ed.reviewSeams() != 0 {
		t.Errorf("one clip has %d joins to review, want 0", ed.reviewSeams())
	}
}

// The wiring: the tick asks the review before the gap skip, so the review
// can end a window that stops in a removed stretch; every pause and every
// switch to the recording ends the review; and the button is on the bar
// beside the two ▶s it belongs with.
func TestTheReviewIsWiredIntoTheTransport(t *testing.T) {
	tick := funcBody(t, "cut.go", `func \(ed \*cutEditor\) followPlayback\(\) bool \{`)
	i, j := strings.Index(tick, "if ed.reviewTick() {"), strings.Index(tick, "if ed.skipGap() {")
	if i < 0 || j < 0 || i > j {
		t.Error("followPlayback does not ask the review before the gap skip")
	}
	for fn, want := range map[string]string{
		`func \(ed \*cutEditor\) toggle\(\) \{`:             "ed.reviewOff()",
		`func \(ed \*cutEditor\) stop\(\) \{`:               "ed.reviewOff()",
		`func \(ed \*cutEditor\) setCutOnly\(cut bool\) \{`: "ed.reviewOff()",
		`func \(ed \*cutEditor\) syncPlayBtn\(\) \{`:        "ed.syncReviewPlay()",
		`func \(a \*App\) syncPlayIcons\(\) \{`:             "ed.syncReviewPlay()",
	} {
		file := "cut.go"
		if strings.HasPrefix(fn, `func \(a`) {
			file = "runbar.go"
		}
		if !strings.Contains(funcBody(t, file, fn), want) {
			t.Errorf("%s does not reach %s", fn, want)
		}
	}
	// a review starting or ending redraws ALL three faces, not its own: the
	// ⏸ and the lamp move between ▶✂ and ▶✂✂ with no transport change to
	// redraw them for
	review := readSrc(t, "cut_review.go")
	for _, fn := range []string{`func \(ed \*cutEditor\) playReview\(\) \{`, `func \(ed \*cutEditor\) reviewOff\(\) \{`} {
		if !strings.Contains(funcBody(t, "cut_review.go", fn), "ed.syncFaces()") {
			t.Errorf("%s does not redraw all three ▶s", fn)
		}
	}
	if !strings.Contains(review, "ed.a.syncPlayIcons()") {
		t.Error("syncFaces does not go through syncPlayIcons, which is the one place all three are drawn")
	}
	page := readSrc(t, "cut.go")
	if !strings.Contains(page, "linked(ed.playBtn, ed.cutPlayBtn, reviewBtn,") {
		t.Error("the review button is not on the bar beside the two ▶s")
	}
	// one lamp across the three: each lit by its own preview and by nothing else
	for file, want := range map[string]string{
		"runbar.go":     "lamp(a.ed.playBtn, !a.ed.cutOnly)",
		"cut.go":        "lamp(ed.cutPlayBtn, ed.cutOnly && !ed.reviewOn)",
		"cut_review.go": "lamp(ed.reviewBtn, ed.reviewOn)",
	} {
		if !strings.Contains(readSrc(t, file), want) {
			t.Errorf("%s does not light its ▶ by the shared rule: %q is gone", file, want)
		}
	}
}
