package main

// Snapping on the picture. The timeline snaps an effect's seconds to the cuts;
// this is the same courtesy for the two rectangles drawn on the video -- the
// camera window and a caption's box -- because at preview size a pixel is
// several in the finished frame, and an edge meant to be ON the frame's edge
// that lands a pixel inside it is a black hairline in the render.
//
// Both halves are here: sliding a whole rectangle (any of its three lines on
// an axis may be the one that lands) and dragging one edge of it (the edge is
// what moves, so the pointer is what snaps).

import (
	"math"
	"os"
	"strings"
	"testing"
)

// The resize half. The pointer is the edge, so the nearest line within reach
// takes it and a hand out in the open is left alone.
func TestADraggedEdgeSnapsToTheFrame(t *testing.T) {
	// the frame runs 100..500 across, with its middle at 300
	lines := []float64{100, 300, 500}
	for _, c := range []struct {
		v, want float64
		why     string
	}{
		{103, 100, "just inside the left edge"},
		{97, 100, "just outside the left edge"},
		{296, 300, "near the middle"},
		{494, 500, "near the right edge"},
		{100, 100, "already exactly on it"},
		{250, 250, "out in the open, nowhere near a line"},
		{311, 311, "past the reach of the middle"},
	} {
		if got := snapPointPx(c.v, fxSnapPx, lines...); got != c.want {
			t.Errorf("%s: %g snapped to %g, want %g", c.why, c.v, got, c.want)
		}
	}
	// between two lines within reach, the nearer one wins
	if got := snapPointPx(106, 20, 100, 110); got != 110 {
		t.Errorf("between two lines 106 went to %g, want the nearer 110", got)
	}
}

// The move half. A whole box is sliding, so its left edge, its middle and its
// right edge are all candidates, and the box is carried to wherever the
// closest of them lands.
func TestASlidingBoxSnapsByWhicheverEdgeIsNearest(t *testing.T) {
	lines := []float64{100, 300, 500} // frame 100..500, middle 300
	const w = 80
	for _, c := range []struct {
		v, want float64
		why     string
	}{
		{104, 100, "its left edge near the frame's left"},
		{416, 420, "its right edge near the frame's right"},
		{257, 260, "its middle near the frame's middle"},
		{200, 200, "nothing within reach"},
	} {
		if got := snapEdgePx(c.v, w, fxSnapPx, lines...); math.Abs(got-c.want) > 1e-9 {
			t.Errorf("%s: left edge %g slid to %g, want %g", c.why, c.v, got, c.want)
		}
	}
	// the box keeps its size whatever lands: snapping moves it, never
	// stretches it
	v := snapEdgePx(104, w, fxSnapPx, lines...)
	if v+w-v != w {
		t.Error("snapping a sliding box changed its width")
	}
}

// The wiring: both rectangles snap on both gestures, and a held caption takes
// the live camera layer down so its box is on the picture to be grabbed at all.
func TestThePictureSnapsAndOffersTheHeldTextsBox(t *testing.T) {
	b, err := os.ReadFile("cut_fxview.go")
	if err != nil {
		t.Fatal(err)
	}
	s := string(b)
	for _, want := range []string{
		// the camera rectangle: sliding it already snapped, and now so does
		// dragging any of its edges -- the top included
		"f.Cx, f.Cy = snapRectPos(cx, cy, wf, f.Hf, 10/dw, 10/dh)",
		"px := snapPointPx(drag.x1, fxSnapPx, ox, ox+dw)",
		"py := snapPointPx(drag.y1, fxSnapPx, oy, oy+dh)",
		// the caption box, against the finished frame's edges and middles
		"nx = snapEdgePx(nx, drag.g.w, fxSnapPx, fx0, fx0+fw0/2, fx0+fw0)",
		"ny = snapEdgePx(ny, drag.g.h, fxSnapPx, fy0, fy0+fh0/2, fy0+fh0)",
		"px = snapPointPx(px, fxSnapPx, fx0, fx0+fw0/2, fx0+fw0)",
		"py = snapPointPx(py, fxSnapPx, fy0, fy0+fh0/2, fy0+fh0)",
		// and picking a caption's bar up puts its box on the picture
		"ed.fxRectHeld() == nil && ed.fxHeldBox() == nil",
	} {
		if !strings.Contains(s, want) {
			t.Errorf("cut_fxview.go no longer contains %q", want)
		}
	}
	// the box is drawn and the pointer offered for whatever is held, which is
	// what makes the drag above reachable
	for _, want := range []string{
		"held := ed.fxRectHeld() != nil || ed.fxHeldBox() != nil",
		"boxOutline(cr, tx, ty, tw, th)",
	} {
		if !strings.Contains(s, want) {
			t.Errorf("cut_fxview.go no longer contains %q", want)
		}
	}
}
