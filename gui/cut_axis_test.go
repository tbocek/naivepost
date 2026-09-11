package main

// What the x-axis MEANS.
//
// It used to mean "the files, one after another": each recording laid down
// after the last with a fixed hole between them, and every x measured from its
// own file's origin. That is indistinguishable from a clock right up until two
// cameras roll at the same time, and then it is nonsense -- the same minute
// filmed twice is drawn as two minutes, and session-second 3:00 is at two
// places at once. Everything that has to line up with something else (the
// sound lanes, the effects lane, the green) lines up with a different story.
//
// So the axis is session time, and what is drawn is the union of what the
// cameras covered: the filmed runs. Time nobody filmed is collapsed to the one
// hole width, because an hour of nothing between two clips is an hour of dead
// pixels to scroll past.
//
// The load-bearing claim, and the first thing pinned here: for one recording,
// or several that do not overlap, this is the OLD layout to the pixel.

import (
	"math"
	"strings"
	"testing"
)

// axisEd is an editor at 4 px per session second, laid out over vids.
func axisEd(t *testing.T, vids ...tlVideo) *cutEditor {
	t.Helper()
	ed := newTestEd(t)
	ed.vids = vids
	ed.relayout()
	return ed
}

// ---- the runs ----------------------------------------------------------------

func TestTheFilmedRunsAreTheUnionOfTheRecordings(t *testing.T) {
	for _, c := range []struct {
		what string
		vids []tlVideo
		want []tlSpan
	}{
		{"nothing loaded", nil, nil},
		{"one recording", []tlVideo{{start: 10, dur: 50}}, []tlSpan{{t0: 10, t1: 60}}},
		{"a hole between two", []tlVideo{{start: 0, dur: 30}, {start: 90, dur: 10}},
			[]tlSpan{{t0: 0, t1: 30}, {t0: 90, t1: 100}}},
		// abutting is not a hole: the second starts where the first stopped,
		// and there is nothing between them to hatch
		{"one stopping as the next starts", []tlVideo{{start: 0, dur: 30}, {start: 30, dur: 10}},
			[]tlSpan{{t0: 0, t1: 40}}},
		{"two cameras overlapping", []tlVideo{{start: 0, dur: 60}, {start: 40, dur: 50}},
			[]tlSpan{{t0: 0, t1: 90}}},
		// one long camera and a short one inside it: the short one adds no
		// timeline at all, which the naive "extend to the newest end" merge
		// gets wrong by cutting the run short at 50
		{"one swallowed by another", []tlVideo{{start: 0, dur: 100}, {start: 20, dur: 30}},
			[]tlSpan{{t0: 0, t1: 100}}},
		// the list is sorted by start on load, but a lane the user has shifted
		// in time is not, and an unsorted merge would leave two runs here
		{"out of order", []tlVideo{{start: 50, dur: 20}, {start: 0, dur: 60}},
			[]tlSpan{{t0: 0, t1: 70}}},
		// a recording that probed as zero-length is not a run
		{"an empty file", []tlVideo{{start: 0, dur: 0}, {start: 10, dur: 5}},
			[]tlSpan{{t0: 10, t1: 15}}},
	} {
		got := timeSpans(c.vids)
		if len(got) != len(c.want) {
			t.Errorf("%s: %d runs %v, want %d %v", c.what, len(got), got, len(c.want), c.want)
			continue
		}
		for i := range got {
			if got[i].t0 != c.want[i].t0 || got[i].t1 != c.want[i].t1 {
				t.Errorf("%s: run %d is %.0f–%.0f, want %.0f–%.0f",
					c.what, i, got[i].t0, got[i].t1, c.want[i].t0, c.want[i].t1)
			}
		}
	}
}

// ---- the old layout, unchanged -----------------------------------------------

// One recording is drawn from the head of the tape at the zoom, and a second
// recording after a hole starts exactly where the first one ended: the four
// and a half minutes nobody filmed are not on the timeline at all, so the two
// takes touch and each wears its own border. The head is gutterPx rather than
// 0 since the switches at the left of every band were given a strip of their
// own to stand in (cut_gutter.go).
func TestUnfilmedTimeTakesNoWidth(t *testing.T) {
	ed := axisEd(t, tlVideo{start: 0, dur: 60}, tlVideo{start: 300, dur: 40})
	firstW := 60 * ed.pps
	if ed.xOf(0) != gutterPx || ed.xOf(60) != gutterPx+firstW {
		t.Errorf("the first recording runs %.0f–%.0f px, want %.0f–%.0f",
			ed.xOf(0), ed.xOf(60), gutterPx, gutterPx+firstW)
	}
	if got, want := ed.xOf(300), gutterPx+firstW; got != want {
		t.Errorf("the second recording starts at %.0f px, want %.0f", got, want)
	}
	if got, want := ed.totalW, gutterPx+firstW+40*ed.pps; got != want {
		t.Errorf("the timeline is %.0f px wide, want %.0f", got, want)
	}
	// the per-file origins the thumbnails are walked from still agree with the
	// map, which is the whole reason they are kept
	for _, v := range ed.vids {
		if v.pxOrigin != ed.xOf(v.start) {
			t.Errorf("a recording's origin is %.0f px but its start reads as %.0f", v.pxOrigin, ed.xOf(v.start))
		}
	}
	if ed.totalW > gutterPx+(60+40)*ed.pps+0.5 {
		t.Errorf("the unfilmed stretch is being drawn: %.0f px for 100 s of footage", ed.totalW)
	}
}

// ---- one second, one place ---------------------------------------------------

func TestOverlappingSourcesShareOneAxis(t *testing.T) {
	ed := axisEd(t, tlVideo{start: 0, dur: 60}, tlVideo{start: 40, dur: 50})
	// the whole session is one run, so x is the clock times the zoom all the
	// way across -- including the twenty seconds both cameras saw
	for _, tt := range []float64{0, 20, 40, 50, 60, 90} {
		if got, want := ed.xOf(tt), gutterPx+tt*ed.pps; math.Abs(got-want) > 1e-9 {
			t.Errorf("session second %.0f is at %.1f px, want %.1f", tt, got, want)
		}
	}
	if got, want := ed.totalW, gutterPx+90*ed.pps; math.Abs(got-want) > 1e-9 {
		t.Errorf("two overlapping recordings measure %.0f px, want %.0f", got, want)
	}
	// the second camera is drawn where its footage actually is, not after the
	// first one -- the bug this replaced
	if ed.vids[1].pxOrigin != gutterPx+40*ed.pps {
		t.Errorf("the second camera starts at %.0f px, want %.0f", ed.vids[1].pxOrigin, gutterPx+40*ed.pps)
	}
}

// ---- the round trip ----------------------------------------------------------

func TestAPixelAndASecondAgree(t *testing.T) {
	ed := axisEd(t, tlVideo{start: 0, dur: 60}, tlVideo{start: 300, dur: 40})
	for _, tt := range []float64{0, 1, 30, 300, 320, 340} {
		if got := ed.tAt(ed.xOf(tt)); math.Abs(got-tt) > 1e-9 {
			t.Errorf("%.0f s reads back as %.3f s", tt, got)
		}
	}
	// 60 is the one second that does not read back, and cannot: with the four
	// unfilmed minutes taking no width, the x where the first take stops IS
	// the x where the second one starts. It reads as the take you can see
	// there, which is the later one.
	if got := ed.tAt(ed.xOf(60)); got != 300 {
		t.Errorf("the seam reads as %.0f s, want 300 -- the take drawn there", got)
	}
	// off the right-hand end is the end of the session
	if got := ed.tAt(ed.totalW + 500); got != 340 {
		t.Errorf("past the end reads as %.0f s, want 340", got)
	}
	// an empty timeline has no time on it at all
	if got := newTestEd(t).tAt(123); got != 0 {
		t.Errorf("an x on an empty track reads as %.0f s, want 0", got)
	}
}

// ---- what the session measures -----------------------------------------------

func TestTheSessionsEndAndLengthCountTimeNotFiles(t *testing.T) {
	// sorted by start, so the LAST entry is the last to begin -- and here it is
	// the first to finish. Reading the end off it would cut fourteen minutes
	// off the session, and every effect would be clamped into the first camera
	ed := axisEd(t, tlVideo{start: 0, dur: 900}, tlVideo{start: 600, dur: 60})
	if got := ed.sessEnd(); got != 900 {
		t.Errorf("the session ends at %.0f s, want 900", got)
	}
	// twenty-five minutes of footage, fifteen minutes of session: the zoom is
	// fitted to the timeline, not to the sum of the files
	if got := ed.filmedDur(); got != 900 {
		t.Errorf("the filmed stretch measures %.0f s, want 900", got)
	}
	ed.viewW = 1800
	// the gutter comes off the width the footage may use: fitted to the window
	// itself the fully zoomed-out timeline would be wider than its window by
	// that strip, and the scrollbar would stay
	if got, want := ed.minPps(), fitPps(1800-gutterPx, 900); got != want {
		t.Errorf("zoom-to-fit is %.4f px/s, want %.4f", got, want)
	}
	if newTestEd(t).sessEnd() != 0 {
		t.Error("an empty session does not end at zero")
	}
}

// ---- what the seam looks like -------------------------------------------------

// The complaint this came from: the camera stops, minutes pass, the camera
// starts again, and the page answered with a band of hatching between the two
// takes -- the emptiest part of the page wearing the loudest mark on it, and
// the amber that says "this recording begins here" reading as a frame around
// the nothing rather than a mark on the footage.
//
// So the nothing is not laid out, and each take wears its own mark just inside
// its own pictures: amber diagonals with the border line down the outer side.
// Where two takes meet that is two striped bands back to back, and nothing
// between them.
func TestTwoTakesMeetAsTwoStripedBandsAndNoGap(t *testing.T) {
	ed := axisEd(t, tlVideo{base: "one", path: "/f/one.mp4", start: 0, dur: 60},
		tlVideo{base: "two", path: "/f/two.mp4", start: 300, dur: 40})
	const w, h = 600, 220
	ed.viewW, ed.viewX = w, 0
	at := renderTrack(t, ed, w, h)
	seam := int(math.Round(ed.xOf(300)))
	top := int(ed.picTop())
	y := top + 4

	solid := func(x, y int) bool {
		r, g, b := at(x, y)
		return r > 200 && g > 150 && b < 90
	}
	// the border on each take: two px for the take that stops here, two for
	// the take that starts, and nothing wider than that
	for _, x := range []int{seam - 2, seam - 1, seam, seam + 1} {
		if !solid(x, y) {
			r, g, b := at(x, y)
			t.Errorf("x=%d (seam%+d) is rgb(%d,%d,%d), want the border's amber", x, x-seam, r, g, b)
		}
	}
	for _, x := range []int{seam - 4, seam + 3} {
		if solid(x, y) {
			t.Errorf("x=%d (seam%+d) is solid amber too -- the border is a band", x, x-seam)
		}
	}
	// ...and the stripes behind it, on the footage of BOTH takes: a mark that
	// only said "here" left the break itself unsaid, now that the minutes the
	// camera was off take no width to say it with
	for _, side := range []struct {
		what   string
		x0, x1 int
	}{{"the take that stops", seam - int(srcEdgeW), seam - 3}, {"the take that starts", seam + 2, seam + int(srcEdgeW)}} {
		n := 0
		for x := side.x0; x <= side.x1; x++ {
			for dy := 2; dy < 16; dy++ {
				if r, g, b := at(x, top+dy); r > 110 && g > 80 && b < 90 && !solid(x, top+dy) {
					n++
				}
			}
		}
		if n < 8 {
			t.Errorf("%s wears %d striped pixels beside the seam, want a hatch", side.what, n)
		}
	}
	// and nothing of the old hatch's dark ground, which is what used to stand
	// between the two takes
	for x := seam - 20; x <= seam+20; x++ {
		if r, g, b := at(x, y); r > 40 && r < 75 && g > 35 && g < 70 && b > 25 && b < 60 {
			t.Fatalf("x=%d (seam%+d) is rgb(%d,%d,%d) -- the hatched ground is back", x, x-seam, r, g, b)
		}
	}
}

// ---- a boundary nobody filmed -------------------------------------------------

// A cut names seconds off a timeline where the minutes between two takes are
// written down as plainly as the rest, so sooner or later it names one nobody
// filmed. It did: a clip began at 203.5 in a 4.5 s hole between two recordings,
// and playback ran into it and STOPPED -- setPlayhead found no recording under
// the line, left the player rolling on the old file, and read back a position
// inside the same gap on every tick after that. The line sat there for good.
func TestAnEdgeIsPulledOutOfUnfilmedTime(t *testing.T) {
	// two takes with a hole between them, exactly the shape that froze it
	ed := axisEd(t, tlVideo{base: "a", path: "/f/a.mp4", start: 193, dur: 6.5},
		tlVideo{base: "b", path: "/f/b.mp4", start: 204, dur: 100})
	if ed.videoAt(203.5) != nil {
		t.Fatal("203.5 is filmed after all -- this test proves nothing")
	}
	// a start moves forward onto the next take, an end back onto the last one:
	// an edge never crosses its own footage on the way out of a hole
	for _, c := range []struct {
		at   float64
		st   bool
		want float64
	}{
		{203.5, true, 204},    // a clip that begins in the hole
		{200.5, false, 199.5}, // ...and one that ends in it
		{193, true, 193},      // filmed already: untouched
	} {
		if got := ed.snapEdge(c.at, c.st); math.Abs(got-c.want) > 1e-9 {
			t.Errorf("snapEdge(%.1f, start=%v) = %.2f, want %.2f", c.at, c.st, got, c.want)
		}
	}
	// and the player is given somewhere it can actually seek to
	if got := ed.playable(203.5); got != 204 {
		t.Errorf("playable(203.5) = %.2f, want 204 -- the first second there is footage for", got)
	}
	if got := ed.playable(250); got != 250 {
		t.Errorf("playable(250) = %.2f, want it left alone -- that second is filmed", got)
	}
	// the freeze itself: the guard that stops skipGap fighting a seek in
	// flight must not stop it retrying a seek that can never land
	body := funcBody(t, "cut.go", `func \(ed \*cutEditor\) skipGap\(`)
	if !strings.Contains(body, "default:") || !strings.Contains(body, "ed.playable(ed.segs[next].S)") {
		t.Error("skipGap still has one answer for a jump that did not move the line: nothing")
	}
}
