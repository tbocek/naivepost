package main

// The effects lane, once it has more than one row.
//
// The rule being pinned is that rows are for OVERLAP and not for effects: an
// effect goes in the first row it fits in, so a cut whose effects do not
// collide is one row deep however many of them there are, and a cut with a
// pile-up is exactly as deep as the pile. That is worth a test of its own
// because the obvious implementation -- one row each -- passes every test about
// "they no longer overlap" and is unusable at ten effects.

import (
	"math"
	"os"
	"strings"
	"testing"
)

// Nothing overlaps, so nothing needs a second row. This is the common case and
// the one that must not regress: a lane that grew a row per effect would push
// the audio lanes down the page for no reason at all.
func TestEffectsThatDoNotCollideShareOneRow(t *testing.T) {
	fx := []cutFx{
		{Kind: "zoom", T: 0, Dur: 5},
		{Kind: "text", T: 10, Dur: 4},
		{Kind: "zoom", T: 30, Stay: true},
		{Kind: "speed", T: 40, Dur: 6, Rate: 0.5},
	}
	rows, n := fxRows(fx)
	if n != 1 {
		t.Errorf("four effects that never overlap took %d rows, want 1", n)
	}
	for i, r := range rows {
		if r != 0 {
			t.Errorf("effect %d (%s at %.0f s) went to row %d with nothing to avoid",
				i, fx[i].Kind, fx[i].T, r)
		}
	}
}

// Two effects live at the same instant cannot be drawn in the same place, which
// is the whole complaint this answers.
func TestEffectsThatCollideGetTheirOwnRows(t *testing.T) {
	// a zoom that fades in for a second, holds for ten and comes back out,
	// with a title over the middle of it
	fx := []cutFx{
		{Kind: "zoom", T: 10, Trans: 1, Dur: 13, Tout: 2},
		{Kind: "text", T: 14, Dur: 5},
	}
	rows, n := fxRows(fx)
	if n != 2 {
		t.Fatalf("a title over a held zoom took %d rows, want 2", n)
	}
	if rows[0] == rows[1] {
		t.Errorf("both effects went to row %d — they are drawn through each other", rows[0])
	}
}

// ...and no deeper than that. Five effects with at most three alive at any one
// instant is a three-row lane, because first-fit in start order colours an
// interval graph with the fewest colours there are.
func TestTheLaneIsOnlyAsDeepAsTheDeepestPile(t *testing.T) {
	fx := []cutFx{
		{Kind: "zoom", T: 0, Dur: 20},  // 0..20
		{Kind: "text", T: 5, Dur: 20},  // 5..25   two deep
		{Kind: "zoom", T: 10, Dur: 20}, // 10..30  three deep
		{Kind: "text", T: 40, Dur: 5},  // 40..45  alone again
		{Kind: "zoom", T: 50, Dur: 5},  // 50..55  alone again
	}
	rows, n := fxRows(fx)
	if n != 3 {
		t.Errorf("a pile three deep took %d rows, want 3", n)
	}
	// and the two that are alone went back to the top row rather than staying
	// in the rows their neighbours in the list happened to use
	if rows[3] != 0 || rows[4] != 0 {
		t.Errorf("the effects with nothing to avoid landed in rows %d and %d, want 0 and 0",
			rows[3], rows[4])
	}
}

// Two hard reframings at the same moment have no span between them at all, so
// "do these overlap" answers no and they would be stacked in the same place. A
// zoom that cuts straight to its region and stays there is exactly the effect
// this is most likely to happen to -- it is a point in time.
func TestTwoEffectsAtTheSameInstantAreTwoRows(t *testing.T) {
	pt := cutFx{Kind: "zoom", T: 12, Stay: true}
	rows, n := fxRows([]cutFx{pt, pt})
	if n != 2 || rows[0] == rows[1] {
		t.Errorf("two reframings at the same second took %d rows (%v), want 2 apart", n, rows)
	}
	// but a comfortable distance apart is one row: the floor is a nudge, not a
	// rule that every effect needs elbow room
	late := cutFx{Kind: "zoom", T: 30, Stay: true}
	if _, n := fxRows([]cutFx{pt, late}); n != 1 {
		t.Errorf("two reframings eighteen seconds apart took %d rows, want 1", n)
	}
}

// The lane asks the page for the height its rows need, and gives it back.
func TestTheLaneGrowsAndShrinksWithThePile(t *testing.T) {
	ed := newTestEd(t)
	if got := ed.fxLaneHeight(); got != fxLaneH {
		t.Errorf("an empty lane wants %g px, want one row's %g — a lane that appears "+
			"with the first effect moves everything under it", got, float64(fxLaneH))
	}
	ed.fx = []cutFx{{Kind: "zoom", T: 0, Dur: 20}, {Kind: "text", T: 5, Dur: 20}}
	if got := ed.fxLaneHeight(); got != 2*fxLaneH {
		t.Errorf("two overlapping effects want %g px of lane, want %g", got, 2*float64(fxLaneH))
	}
	ed.fx = ed.fx[:1]
	if got := ed.fxLaneHeight(); got != fxLaneH {
		t.Errorf("the lane kept %g px after the overlap was removed, want %g", got, float64(fxLaneH))
	}
}

// A press picks the effect in the row it landed in. This is the point of rows
// for the hand rather than for the eye: two effects that overlap in time used
// to be told apart by which of them was narrower, which is a rule you cannot
// see. Now they are in different places.
func TestAPressTakesTheEffectInItsOwnRow(t *testing.T) {
	ed := newTestEd(t)
	ed.vids = []tlVideo{{base: "v", path: "v.mkv", start: 0, dur: 300, interval: 5, fps: 30}}
	ed.relayout()
	ed.fx = []cutFx{
		{Kind: "zoom", T: 10, Dur: 20}, // row 0
		{Kind: "text", T: 12, Dur: 5},  // row 1: it overlaps the zoom
	}
	rows, _ := fxRows(ed.fx)
	px := ed.xOf(14) // inside both of them
	for i := range ed.fx {
		y := ed.fxLaneTop() + float64(rows[i])*fxLaneH + fxLaneH/2
		if got := ed.fxIndexAt(px, y); got != i {
			t.Errorf("a press in row %d at 14 s took effect %d, want %d", rows[i], got, i)
		}
	}
	// and a press below the last row takes nothing, rather than the nearest
	// thing above it: an empty row is empty
	below := ed.fxLaneTop() + 2*fxLaneH + fxLaneH/2
	if got := ed.fxIndexAt(px, below); got != -1 {
		t.Errorf("a press under the lane took effect %d, want nothing", got)
	}
}

func TestTheRowsAreWired(t *testing.T) {
	fxsrc, err := os.ReadFile("cut_fx.go")
	if err != nil {
		t.Fatal(err)
	}
	src, err := os.ReadFile("cut.go")
	if err != nil {
		t.Fatal(err)
	}
	for _, want := range []string{
		// the lane is drawn a row at a time...
		"rows, nrows := fxRows(ed.fx)",
		"y := top + float64(rows[i])*fxLaneH",
		// ...and hit-tested the same way
		"rows, n := fxRows(ed.fx)",
	} {
		if !strings.Contains(string(fxsrc), want) {
			t.Errorf("the effects lane no longer contains %q", want)
		}
	}
	// the page asks for the height the rows need, and only when it changed:
	// a SetSizeRequest from inside a draw is a resize loop
	for _, want := range []string{
		"h := int(ed.picBottom()) + 4",
		"if h == ed.srcHt {",
		"ed.fitSrc() // the effects lane is as deep as the effects pile up",
	} {
		if !strings.Contains(string(src), want) {
			t.Errorf("the cut page no longer contains %q", want)
		}
	}
}

// ---- the ends of an effect's band -------------------------------------------

// A band you can only move is half an object. What is pinned here is that its
// ends move on their own, that what they change is the HOLD rather than the
// glides, and that a marker too narrow to have a middle keeps its old
// move-only behaviour instead of stretching every time it is picked up.
func TestOnlyABandWideEnoughToHaveAMiddleHasEnds(t *testing.T) {
	ed := newTestEd(t) // 4 px/s
	ed.vids = []tlVideo{{base: "v", path: "v.mkv", start: 0, dur: 300, interval: 5, fps: 30}}
	ed.relayout()
	// a hard reframing: a flag ten px wide, all of it middle
	ed.fx = []cutFx{{Kind: "zoom", T: 20, Stay: true}}
	x0, x1 := ed.fxSpanPx(ed.fx[0])
	if x1-x0 >= fxMinBand {
		t.Fatalf("a bare reframing is %g px wide at this zoom; the test needs it under %g", x1-x0, fxMinBand)
	}
	for _, px := range []float64{x0, x1, (x0 + x1) / 2} {
		if got := ed.fxPartAt(0, px); got != fxWhole {
			t.Errorf("a press at %g on a flag took part %d, want the whole of it", px, got)
		}
	}
	// a zoom held for twenty seconds is eighty px: two ends and a middle
	ed.fx = []cutFx{{Kind: "zoom", T: 20, Dur: 20}}
	x0, x1 = ed.fxSpanPx(ed.fx[0])
	for _, c := range []struct {
		px   float64
		want int
		what string
	}{
		{x0, fxStart, "its start"},
		{x1, fxEnd, "its end"},
		{(x0 + x1) / 2, fxWhole, "its middle"},
	} {
		if got := ed.fxPartAt(0, c.px); got != c.want {
			t.Errorf("a press on %s took part %d, want %d", c.what, got, c.want)
		}
	}
}

// The rule that makes dragging an end mean something: the fades keep their
// seconds and the length absorbs the change. Dragging the end of a zoom means
// "hold it longer", never "take longer getting there" -- the travel time is a
// number you chose, and an edit that silently rewrote it would be an edit you
// did not ask for.
func TestDraggingAnEndChangesTheHoldAndNotTheGlides(t *testing.T) {
	ed := newTestEd(t)
	ed.vids = []tlVideo{{base: "v", path: "v.mkv", start: 0, dur: 300, interval: 5, fps: 30}}
	ed.relayout()
	ed.fx = []cutFx{{Kind: "zoom", T: 10, Trans: 1, Dur: 8, Tout: 2}} // the band is 10..18
	ed.fxOn, ed.fxSel = true, 0
	ed.resizeFxTo(true, 25) // pull the end out to 25
	f := ed.fx[0]
	if f.Trans != 1 || f.Tout != 2 {
		t.Errorf("the glides became %.2f in and %.2f out, want 1 and 2 — dragging an end "+
			"rewrote how the camera travels", f.Trans, f.Tout)
	}
	if f.T != 10 {
		t.Errorf("the effect moved to %.2f while its END was dragged", f.T)
	}
	if want := 25.0 - 10; math.Abs(f.Dur-want) > 1e-9 {
		t.Errorf("the band became %.2f s, want %.2f", f.Dur, want)
	}
	if _, e := f.fxSpan(); math.Abs(e-25) > 1e-9 {
		t.Errorf("the band now ends at %.2f, want where it was dragged to (25)", e)
	}
}

// Dragging the START moves the effect and leaves the far end where it was: a
// band that begins later begins later.
func TestDraggingTheStartMovesTheEffectAndKeepsTheFarEnd(t *testing.T) {
	ed := newTestEd(t)
	ed.vids = []tlVideo{{base: "v", path: "v.mkv", start: 0, dur: 300, interval: 5, fps: 30}}
	ed.relayout()
	ed.fx = []cutFx{{Kind: "zoom", T: 10, Dur: 20}} // 10..30
	ed.fxOn, ed.fxSel = true, 0
	ed.resizeFxTo(false, 17)
	f := ed.fx[0]
	if f.T != 17 {
		t.Errorf("the start went to %.2f, want 17", f.T)
	}
	if s, e := f.fxSpan(); math.Abs(s-17) > 1e-9 || math.Abs(e-30) > 1e-9 {
		t.Errorf("the band is now %.2f..%.2f, want 17.00..30.00", s, e)
	}
}

// The ends snap where a moved effect snaps -- the borders of the cut -- for the
// same reason: an effect that stops a third of a second before its clip does is
// a mistake nobody makes on purpose.
func TestAResizedBandLandsOnTheCuts(t *testing.T) {
	ed := newTestEd(t)
	ed.vids = []tlVideo{{base: "v", path: "v.mkv", start: 0, dur: 300, interval: 5, fps: 30}}
	ed.relayout()
	ed.segs = []cutSeg{{S: 10, E: 40}}
	ed.fx = []cutFx{{Kind: "text", T: 12, Dur: 10}}
	ed.fxOn, ed.fxSel = true, 0
	ed.resizeFxTo(true, 40+snapPx/ed.pps/2) // let go just past the cut at 40
	if _, e := ed.fx[0].fxSpan(); math.Abs(e-40) > 1e-9 {
		t.Errorf("an end let go beside the cut at 40 landed at %.2f", e)
	}
}

// A band cannot be dragged out of existence -- except a view's, whose hold
// going to zero is the plain "put the camera here" flag it has always been.
func TestABandCannotBeDraggedToNothingUnlessItIsAView(t *testing.T) {
	ed := newTestEd(t)
	ed.vids = []tlVideo{{base: "v", path: "v.mkv", start: 0, dur: 300, interval: 5, fps: 30}}
	ed.relayout()

	ed.fx = []cutFx{{Kind: "zoom", T: 10, Dur: 20}}
	ed.fxOn, ed.fxSel = true, 0
	ed.resizeFxTo(true, 5) // dragged back past its own start
	if ed.fx[0].Dur < fxMinDur {
		t.Errorf("a zoom was dragged down to %.3f s — an effect of no length is "+
			"indistinguishable from one that was deleted", ed.fx[0].Dur)
	}

}

// An edit, so it goes on the undo stack -- and only once per drag, because a
// drag is one thing the hand did.
func TestResizingAnEffectIsOneUndoableEdit(t *testing.T) {
	ed := newTestEd(t)
	ed.vids = []tlVideo{{base: "v", path: "v.mkv", start: 0, dur: 300, interval: 5, fps: 30}}
	ed.relayout()
	ed.fx = []cutFx{{Kind: "zoom", T: 10, Dur: 20}}
	ed.fxOn, ed.fxSel = true, 0
	for _, to := range []float64{25, 26, 27} { // one drag, three motion events
		ed.resizeFxTo(true, to)
	}
	if len(ed.undo) != 1 {
		t.Errorf("a drag over three motion events pushed %d undo steps, want 1", len(ed.undo))
	}
	if ed.undo[0].fx[0].Dur != 20 {
		t.Errorf("the undo step holds a %.2f s zoom, want the 20 s it was before the drag",
			ed.undo[0].fx[0].Dur)
	}
}

func TestTheBandEndsAreWired(t *testing.T) {
	b, err := os.ReadFile("cut.go")
	if err != nil {
		t.Fatal(err)
	}
	src := string(b)
	for _, want := range []string{
		"fxPart = ed.fxPartAt(i, x+ed.viewX)",
		"if fxPart == fxStart || fxPart == fxEnd {",
		"ed.resizeFxTo(fxPart == fxEnd, ed.tAtView(dragStartX+ox))",
		"ed.fxMoving, fxPart = false, fxWhole",
	} {
		if !strings.Contains(src, want) {
			t.Errorf("the cut page no longer contains %q", want)
		}
	}
}
