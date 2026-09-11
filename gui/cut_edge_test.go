package main

// Trimming a clip by its edge. Add and Remove work in regions, which is the
// wrong shape for the last thing you do to a scene -- half a second off the
// front -- so a green border is hovered, taken with the left button and moved
// by the same drag or by the frame buttons. What these pin is the arithmetic
// under that: how far an edge may go, that one hold is one Undo, that a press
// knows the difference between the edge and the track it sits on, and that the
// lower track paints the frame the boundary falls inside instead of leaving a
// grey bar there.

import (
	"math"
	"os"
	"strings"
	"testing"
)

func edgeEd(t *testing.T) *cutEditor {
	t.Helper()
	ed := newTestEd(t)
	ed.vids = []tlVideo{{base: "v", path: "v.mkv", start: 0, dur: 600, interval: 5, fps: 30}}
	ed.relayout() // pps 4, so a session second is four timeline px
	ed.segs = []cutSeg{{S: 100, E: 130}, {S: 200, E: 260}}
	return ed
}

func TestAnEdgeIsPickedUpNearItsBorder(t *testing.T) {
	ed := edgeEd(t)

	// the px below are the timeline's own, which start a strip in from the
	// widget's edge now (cut_gutter.go): xOf is what says where a second is
	if !ed.grabEdge(ed.xOf(100)+2) || !ed.edgeOn || ed.edgeSeg != 0 || ed.edgeEnd {
		t.Fatalf("a press 2 px from clip 1's start did not pick it up: on=%v seg=%d end=%v",
			ed.edgeOn, ed.edgeSeg, ed.edgeEnd)
	}
	if got := ed.edgeTime(); got != 100 {
		t.Errorf("the held edge reads as %g, want 100", got)
	}
	// the other three borders, each by its own px
	for _, c := range []struct {
		px   float64
		seg  int
		end  bool
		want float64
	}{{ed.xOf(130), 0, true, 130}, {ed.xOf(200), 1, false, 200}, {ed.xOf(260), 1, true, 260}} {
		if !ed.grabEdge(c.px) || ed.edgeSeg != c.seg || ed.edgeEnd != c.end || ed.edgeTime() != c.want {
			t.Errorf("px %g picked up seg %d end=%v at %g, want seg %d end=%v at %g",
				c.px, ed.edgeSeg, ed.edgeEnd, ed.edgeTime(), c.seg, c.end, c.want)
		}
	}
	// ...and a press out in the middle of a clip is not an edge at all
	if ed.grabEdge(ed.xOf(115)) {
		t.Error("a press 15 s from any border picked up an edge")
	}
}

// One hold is one Undo: a drag is a single act, and a snapshot per motion event
// would be the whole history.
func TestOneHoldIsOneUndo(t *testing.T) {
	ed := edgeEd(t)
	ed.grabEdge(ed.xOf(100))
	for _, to := range []float64{104, 108, 112, 105} {
		ed.moveEdgeTo(to, true)
	}
	if len(ed.undo) != 1 {
		t.Fatalf("a four-move drag left %d undo entries, want 1", len(ed.undo))
	}
	if ed.segs[0].S != 105 {
		t.Fatalf("the edge ended at %g, want 105", ed.segs[0].S)
	}
	ed.undoLast()
	if ed.segs[0].S != 100 {
		t.Fatalf("undo left the edge at %g, want the 100 it started at", ed.segs[0].S)
	}
	if ed.edgeOn {
		t.Error("the edge is still held after an Undo replaced the cut it indexes into")
	}
	// picking an edge up and putting it down again is not an edit
	ed.grabEdge(400)
	ed.moveEdgeTo(100, true)
	if len(ed.undo) != 0 {
		t.Errorf("a hold that moved nothing pushed %d undo entry/entries", len(ed.undo))
	}
}

// An edge may not turn its clip inside out, walk onto the next one, or leave
// the recording it was cut from.
func TestAnEdgeStopsWhereItMust(t *testing.T) {
	segs := []cutSeg{{S: 100, E: 130}, {S: 200, E: 260}}
	for _, c := range []struct {
		name   string
		i      int
		end    bool
		to     float64
		lo, hi float64
		want   float64
	}{
		{"start dragged past its own end (a clip keeps minSegLn)", 0, false, 400, 0, 600, 129},
		{"end dragged past its own start", 0, true, 0, 0, 600, 101},
		{"end dragged onto the next clip (clips never overlap)", 0, true, 240, 0, 600, 200},
		{"start dragged onto the one before", 1, false, 50, 0, 600, 130},
		{"end dragged past the recording", 1, true, 900, 0, 600, 600},
		{"start dragged before the recording", 0, false, -50, 0, 600, 0},
		{"a move well inside every stop", 1, false, 210, 0, 600, 210},
	} {
		if got := clampEdge(segs, c.i, c.end, c.to, c.lo, c.hi); got != c.want {
			t.Errorf("%s: to %g gave %g, want %g", c.name, c.to, got, c.want)
		}
	}
}

// The frame buttons are the edge's when one is held -- ‹f on a boundary you
// have just picked up cannot mean "move the playhead somewhere else" -- and
// they move it by whole frames of the recording it belongs to.
func TestTheFrameButtonsMoveTheHeldEdge(t *testing.T) {
	ed := edgeEd(t)
	ed.grabEdge(ed.xOf(100))
	ed.frameStep(-30) // 30 fps: one second
	if got := ed.segs[0].S; got != 99 {
		t.Fatalf("‹‹f moved the edge to %g, want 99", got)
	}
	ed.frameStep(+15)
	if got := ed.segs[0].S; got != 99.5 {
		t.Fatalf("f› moved the edge to %g, want 99.5", got)
	}
	// the whole trim is still one entry
	if len(ed.undo) != 1 {
		t.Errorf("two nudges of one held edge left %d undo entries, want 1", len(ed.undo))
	}
	// with nothing held they are the playhead's again, and with no preview
	// loaded that is a no-op rather than a trim
	ed.dropEdge()
	ed.frameStep(+30)
	if got := ed.segs[0].S; got != 99.5 {
		t.Errorf("a frame step with no edge held moved the cut to %g", got)
	}
}

// The page draws the footage once. There used to be a second band under it
// holding the same thumbnails with the dropped stretches cut out of them, which
// is the green overlay said twice -- and it was the band that kept going wrong
// on its own (a frame was painted only when its sample time fell inside the
// cut, so a clip starting between two samples showed a grey bar at its head).
// Nothing about the cut is only readable from a second row of pictures.
func TestThereIsOneThumbnailBand(t *testing.T) {
	b, err := os.ReadFile("cut.go")
	if err != nil {
		t.Fatal(err)
	}
	src := string(b)
	for _, gone := range []string{"cutArea", "isCut", "keptSpans"} {
		if strings.Contains(src, gone) {
			t.Errorf("the second thumbnail band is back (%q) — the green overlay already says which parts are kept", gone)
		}
	}
	if !strings.Contains(src, "cr.SetSourceRGBA(0.2, 0.8, 0.3, 0.30)") {
		t.Error("the green overlay is gone, and with the second band gone too nothing says what the cut keeps")
	}
}

// ...and the wiring that puts the whole of it on the right button: hover to see
// which border is under the pointer, right-press to take it, drag to trim.
func TestTheEdgeToolIsWired(t *testing.T) {
	b, err := os.ReadFile("cut.go")
	if err != nil {
		t.Fatal(err)
	}
	src := string(b)
	for _, want := range []string{
		"pick.SetButton(gdk.BUTTON_PRIMARY)", // one button for the whole page...
		"pick.ConnectPressed(",               // ...and the second click of it takes a whole clip
		"case ed.grabEdge(px):",              // press: pick up the border under the cursor
		// the RIGHT press takes the border and the same drag trims it: the
		// left button draws selections and nothing else now
		"if ed.onHeldEdge(px) || ed.grabEdge(px) {",
		"case ed.onHeldEdge(px):", // ...and the held one is offered first, and wider
		// the border under the pointer is highlighted before any of that
		"if ed.edgeHovOn {",
		"ed.moveEdgeTo(ed.tAtView(slideX0+ox), true)", // drag: move it, without writing the file per motion
		"ed.showEdge(true) // the picture comes with it",
		"ed.persist() // the drag is over", // release: this is the cut that goes on disk
		"ed.dropEdge() // any other left click puts a held edge or clip down",
		// arrows nudge, but only while something is held
		"case (ed.edgeOn || ed.segOn || ed.fxOn) && (keyval == gdk.KEY_Left || keyval == gdk.KEY_Right):",
		"|| ed.selOn || ed.copyOn || ed.fxArm != \"\") && keyval == gdk.KEY_Escape:",
		"if ed.edgeOn && ed.nudgeEdge(n) {",           // ‹f and f› are the edge's while one is held
		"if ed.edgeOn && ed.edgeSeg < len(ed.segs) {", // ...and the held edge is drawn
	} {
		if !strings.Contains(src, want) {
			t.Errorf("the cut page no longer contains %q", want)
		}
	}
	// one gesture per button, still: the cut's own drags are cases of the
	// slide gesture rather than a third and fourth controller over the same
	// pixels, which is what the two below were.
	for _, gone := range []string{"edge.ConnectDragUpdate(", "seg.ConnectDragUpdate("} {
		if strings.Contains(src, gone) {
			t.Errorf("the timeline is back on two buttons (%q) — hovering a border and "+
				"pressing it is the whole gesture now", gone)
		}
	}
	if n := strings.Count(src, "gdk.BUTTON_SECONDARY"); n != 1 {
		t.Errorf("the right button is bound %d times, want exactly the one that slides "+
			"the timeline — trimming and moving a clip are the left button's", n)
	}
}

// A left press takes hold of the edge only where the edge is drawn. Anywhere
// else it is the selection it has always been -- which is also what puts a held
// edge down, so the two readings of a left press never overlap.
func TestALeftPressTakesTheEdgeOnlyOnTheEdge(t *testing.T) {
	ed := edgeEd(t)
	if ed.onHeldEdge(400) {
		t.Error("a press took hold of an edge while none was held")
	}
	x100 := ed.xOf(100)
	ed.grabEdge(x100) // clip 1's start
	for _, c := range []struct {
		px   float64
		want bool
	}{
		{x100, true},       // on it
		{x100 + 9, true},   // on the handle's head
		{x100 + 12, true},  // the far side of the grab margin
		{x100 + 14, false}, // clear of it: a selection, and the edge goes down
		{x100 + 60, false}, // well clear
	} {
		if got := ed.onHeldEdge(c.px); got != c.want {
			t.Errorf("a left press at px %g takes the edge: %v, want %v", c.px, got, c.want)
		}
	}
	// and the edge it follows is the one being moved, not the one it was
	// picked up at: a drag that has travelled keeps its grip
	ed.moveEdgeTo(120, true)
	if ed.onHeldEdge(x100) || !ed.onHeldEdge(ed.xOf(120)) {
		t.Error("the grip stayed at the px the edge was picked up at rather than following it")
	}
}

// The preview follows the edge, because an edge is judged by the frame it cuts
// on. An end edge shows the last frame the cut KEEPS, which is a frame short of
// the boundary itself -- the boundary's own frame is the first one dropped.
func TestThePictureFollowsTheEdge(t *testing.T) {
	ed := edgeEd(t) // one 30 fps recording
	ed.grabEdge(ed.xOf(100))
	ed.showEdge(false)
	if ed.playhead != 100 || !ed.hasPlay {
		t.Errorf("a start edge at 100 s previewed %g", ed.playhead)
	}
	ed.grabEdge(ed.xOf(130)) // clip 1's end
	ed.showEdge(false)
	if want := 130 - 1.0/30; math.Abs(ed.playhead-want) > 1e-9 {
		t.Errorf("an end edge at 130 s previewed %g, want %g — the last frame kept, not the first dropped",
			ed.playhead, want)
	}
	// a live drag is thinned: the seeks would otherwise queue up behind the
	// mouse. The first one goes through, the ones in the same instant do not
	ed.moveEdgeTo(140, true)
	ed.showEdge(true)
	first := ed.playhead
	ed.moveEdgeTo(150, true)
	ed.showEdge(true)
	if ed.playhead != first {
		t.Error("every motion event seeks — the preview is not thinned during a drag")
	}
	// ...and the end of the drag always lands, throttle or no throttle
	ed.showEdge(false)
	if want := 150 - 1.0/30; math.Abs(ed.playhead-want) > 1e-9 {
		t.Errorf("the drag ended at %g and the picture stayed at %g", want, ed.playhead)
	}
}
