package main

// A click on an effect opens its numbers -- the dialog ✎ Edit opens.
//
// It was a DOUBLE click, on the reasoning that the single press was already
// taken by hold-slide-resize and the second click was therefore free. What that
// missed is that a double click is a thing you have to be TOLD about: a bar you
// can pick up, drag and resize but not open reads as an effect with no settings
// at all, and the numbers behind it are the whole effect. So the plain click
// does it, at the end of the press rather than its beginning -- see below for
// why that is not the same thing.
//
// The wiring is a closure over GTK gestures, so what is pinned is the source.

import (
	"strings"
	"testing"
)

func TestAClickOnAnEffectOpensItsNumbers(t *testing.T) {
	src := readSrc(t, "cut.go")
	body := funcBody(t, "cut.go", `drag\.ConnectDragEnd\(func\(ox, oy float64\) \{`)
	if !strings.Contains(body, "ed.fxMoving {") {
		t.Fatalf("the drag no longer ends an effect's press:\n%s", body)
	}
	// on an idle: opened inline, the release would never reach the drag
	// gesture underneath and the press would stick
	if !strings.Contains(body, "glib.IdleAdd(func() { ed.a.editFx() })") {
		t.Error("a click on an effect no longer opens its numbers")
	}
	// and only when the effect did not move. The form is opened on the effect
	// as it was at the press and saved by looking that effect up again by those
	// numbers (updateFx); after a drag they are last second's numbers and the
	// save finds nothing to write to
	if !strings.Contains(body, "if !moved && math.Abs(ox) < dragSlop && math.Abs(oy) < dragSlop {") {
		t.Error("a drag along the lane opens a form on numbers that are already stale")
	}
	// ...and the update holds the band still under exactly that threshold, or
	// the two disagree about what a click is: the band moved on the press and
	// the form opened on where it used to be
	up := funcBody(t, "cut.go", `drag\.ConnectDragUpdate\(func\(ox, oy float64\) \{`)
	if !strings.Contains(up, "if !ed.fxDirty && math.Abs(ox) < dragSlop && math.Abs(oy) < dragSlop {") {
		t.Error("a press on an effect slides it before it has travelled, which on a lane " +
			"that snaps means jumping to the nearest cut")
	}
	if i, j := strings.Index(body, "moved := ed.fxDirty"), strings.Index(body, "ed.fxDirty = false"); i < 0 || i > j {
		t.Error("`moved` is read after the drag has cleared it, so every click reads as a move")
	}

	// the double click has nothing left to mean in the lane, and does not open
	// a second form on top of the one the press already opened
	pick := funcBody(t, "cut.go", `pick\.ConnectPressed\(func\(n int, x, y float64\) \{`)
	if strings.Contains(pick, "editFx") {
		t.Error("the double click opens the effect form a second time")
	}
	// but it still has to keep its hands off the playhead: without the branch,
	// a second click in the lane would fall through to pickAt and cue the
	// footage from wherever in the lane the hand happened to be
	if !strings.Contains(pick, "if area == ed.srcArea && ed.fxHitLane(y) {") {
		t.Error("a double click in the effects lane falls through to the playhead")
	}
	// the toolbar button is the other way to the same dialog, and stays
	if !strings.Contains(src, "if ed.heldFx() != nil {") || !strings.Contains(src, "a.editFx()") {
		t.Error("the held effect no longer turns ⧉ Insert into ✎ Edit")
	}
}
