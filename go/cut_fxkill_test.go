package main

// Removing an effect from its band.
//
// The only door used to be ⌦ with the effect in hand, and the lane never said
// so: the band offered nothing to press, which the user read -- correctly --
// as "I cannot remove the effects". The band now wears the same ✕ a kept
// scene does, and the press removes the effect it is on. ⌦ still works, and
// comes through the same killFx, because two copies of the index surgery
// would be two chances for the hold to end up naming the wrong effect.

import (
	"strings"
	"testing"
)

// fxKillEd is one recording with a wide title, a wide zoom over it (second
// row), and a sliver of a zoom too narrow for a ✕ of its own.
func fxKillEd(t *testing.T) *cutEditor {
	t.Helper()
	ed := axisEd(t, tlVideo{base: "a", path: "/f/a.mp4", start: 0, dur: 100})
	ed.fx = []cutFx{
		{Kind: "text", T: 10, Dur: 20, Text: "hello"},
		{Kind: "zoom", T: 15, Dur: 20},
		{Kind: "zoom", T: 60, Dur: 5},
	}
	rows, n := fxRows(ed.fx)
	if n != 2 || rows[0] != 0 || rows[1] != 1 {
		t.Fatalf("the fixture is wrong: rows=%v depth=%d", rows, n)
	}
	return ed
}

func TestTheEffectsXTakesThatEffectAndOnlyThatEffect(t *testing.T) {
	ed := fxKillEd(t)
	ed.fxOn, ed.fxSel = true, 1 // the zoom is in hand
	ed.killFx(0)                // and the ✕ pressed is the title's
	if len(ed.fx) != 2 || ed.fx[0].Kind != "zoom" {
		t.Fatalf("removing the title left %v", ed.fx)
	}
	if !ed.fxOn || ed.fxSel != 0 {
		t.Errorf("the hand was holding effect 1 and the indices moved: fxOn=%v fxSel=%d",
			ed.fxOn, ed.fxSel)
	}
	if len(ed.undo) != 1 {
		t.Error("removing an effect is an edit, and it left nothing to undo")
	}
	ed.undoLast()
	if len(ed.fx) != 3 || ed.fx[0].Kind != "text" {
		t.Errorf("↶ Undo did not bring the title back: %v", ed.fx)
	}
}

func TestRemovingTheHeldEffectOpensTheHand(t *testing.T) {
	ed := fxKillEd(t)
	ed.fxOn, ed.fxSel = true, 1
	ed.killFx(1)
	if ed.fxOn {
		t.Error("the hand is still closed around an effect that no longer exists")
	}
	// and ⌦ is the same door: held again, removeHeldFx comes through killFx
	ed.fxOn, ed.fxSel = true, 0
	ed.removeHeldFx()
	if len(ed.fx) != 1 || ed.fxOn || len(ed.undo) != 2 {
		t.Errorf("⌦ on a held effect: %d left, fxOn=%v, %d undos",
			len(ed.fx), ed.fxOn, len(ed.undo))
	}
}

// The ✕ sits in the MIDDLE of the band, on the band's own row -- the green
// bar's place for it (cut_selband.go), because a band's two ends are the grips
// that move its start and its end.
func TestTheXSitsInTheMiddleOfTheBandOnItsOwnRow(t *testing.T) {
	ed := fxKillEd(t)
	x0, x1 := ed.fxSpanPx(ed.fx[1])
	mid := (x0 + x1) / 2
	cx, cy, ok := ed.fxKillCentre(1)
	if !ok || cx != mid || cy != ed.fxLaneTop()+1.5*fxLaneH {
		t.Errorf("the zoom's ✕ is at (%.0f, %.0f) ok=%v, want (%.0f, %.0f) on row 1",
			cx, cy, ok, mid, ed.fxLaneTop()+1.5*fxLaneH)
	}
	if i := ed.fxKillAt(cx, cy); i != 1 {
		t.Errorf("a press on that spot answers effect %d, want 1", i)
	}
	// a refusal changes nothing
	ed.killFx(-1)
	ed.killFx(7)
	if len(ed.fx) != 3 || len(ed.undo) != 0 {
		t.Errorf("killFx out of range must be a no-op: %d effects, %d undos",
			len(ed.fx), len(ed.undo))
	}
}

func TestABandTooNarrowForTheXKeepsItsCorner(t *testing.T) {
	ed := fxKillEd(t)
	// the sliver: 5 s at 4 px/s is 20 px, and the ✕'s target alone reaches
	// killIn+segKillHit in from the right -- a badge there would cover the
	// band's middle, so a press meant to slide it would remove it
	if _, _, ok := ed.fxKillCentre(2); ok {
		t.Error("a 20 px band was given a ✕ wider than its own middle")
	}
	_, x1 := ed.fxSpanPx(ed.fx[2])
	if i := ed.fxKillAt(x1-killIn, ed.fxLaneTop()+0.5*fxLaneH); i != -1 {
		t.Errorf("a press on the sliver's corner answers %d, want -1 (it holds instead)", i)
	}
}

// The wiring: the press asks the ✕ before the band, the hover lights it, the
// draw paints it, and ⌦ shares the surgery.
func TestTheEffectsXIsWired(t *testing.T) {
	cut := readSrc(t, "cut.go")
	if !strings.Contains(cut, "ed.killFx(i)") ||
		!strings.Contains(cut, "ed.drawFxKill(cr, vx0, vx1)") {
		t.Error("cut.go does not press or draw the effect ✕")
	}
	lane := cut[strings.Index(cut, "ed.fxHitLane(y) {"):]
	if k, h := strings.Index(lane, "ed.fxKillAt("), strings.Index(lane, "ed.fxIndexAt("); k < 0 || h < 0 || k > h {
		t.Error("the fx-lane press does not ask the ✕ before taking hold of the band")
	}
	if !strings.Contains(readSrc(t, "cut_selband.go"), "ed.hoverFxKill(x, y)") {
		t.Error("the pointer never lights the effect ✕")
	}
	fx := readSrc(t, "cut_fx.go")
	if !strings.Contains(fx, "ed.killFx(ed.fxSel)") || strings.Contains(fx, "ed.fx = append(ed.fx[:ed.fxSel]") {
		t.Error("⌦ grew its own copy of the removal instead of coming through killFx")
	}
}
