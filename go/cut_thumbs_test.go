package main

// How far apart the thumbnails on a video row stand.
//
// A row is painted by drawing one frame every step frames, and the step has to
// be one thumbnail's width: any wider and the band's own ground shows between
// the pictures as black stripes. The width is the row's height times whatever
// shape the source is -- the thumbnails are scaled to the row height and keep
// their aspect (cut.go, ed.thumb) -- and the step used to be measured as if
// every source were 16:9. A 4:3 capture, a phone held upright, a square: all
// of them came out striped, because the step was measured for a picture wider
// than the one that got drawn.

import (
	"strings"
	"testing"
)

// The one thing that matters: whatever the source's shape, two drawn frames
// are never further apart than the picture between them is wide.
func TestNoShapeOfSourceLeavesTheRowStriped(t *testing.T) {
	const th, pps = 64.0, 4.0
	for _, c := range []struct {
		what string
		w, h int
	}{
		{"16:9, the usual camera", 1920, 1080},
		{"4:3, an older one", 640, 480},
		{"9:16, a phone held upright", 1080, 1920},
		{"1:1, square", 720, 720},
		{"21:9, ultrawide", 2560, 1080},
	} {
		v := &tlVideo{w: c.w, h: c.h, interval: 0.5}
		gap := float64(v.thumbStep(th, pps)) * v.interval * pps
		wide := th * float64(c.w) / float64(c.h)
		if gap > wide {
			t.Errorf("%s: thumbnails %.1f px apart and %.1f px wide — %.1f px of stripe",
				c.what, gap, wide, gap-wide)
		}
	}
}

// A source that never said what shape it is -- a lane whose file would not
// probe, a row built before the size was read -- still gets thumbnails, spaced
// for the shape most cameras are. A zero there must not divide.
func TestASourceThatNeverSaidItsShapeIsStillDrawn(t *testing.T) {
	const th, pps = 64.0, 4.0
	wide := &tlVideo{w: 1920, h: 1080, interval: 0.5}
	for _, v := range []*tlVideo{
		{interval: 0.5},                // nothing known
		{w: 1920, interval: 0.5},       // half known, which is no better
		{w: 0, h: 1080, interval: 0.5}, //
	} {
		if got, want := v.thumbStep(th, pps), wide.thumbStep(th, pps); got != want {
			t.Errorf("an unmeasured source steps %d frames, want %d — 16:9's", got, want)
		}
	}
	// and the step is a stride, so it is never nought however far in the
	// timeline is zoomed: a nought there would not draw the row slowly, it
	// would not draw it at all
	zoomed := &tlVideo{w: 1080, h: 1920, interval: 1}
	if got := zoomed.thumbStep(4, 60); got != 1 {
		t.Errorf("a row zoomed right in steps %d frames, want 1", got)
	}
}

// The row is drawn from this and not from a shape of its own.
func TestTheRowsThumbnailsAreSpacedByTheSource(t *testing.T) {
	body := funcBody(t, "cut.go", `func \(ed \*cutEditor\) drawTrack`)
	if !strings.Contains(body, "v.thumbStep(th, ed.pps)") {
		t.Error("drawTrack no longer spaces its thumbnails by the source's shape")
	}
	if strings.Contains(body, "1.78") {
		t.Error("drawTrack still has 16:9 written into it")
	}
}
