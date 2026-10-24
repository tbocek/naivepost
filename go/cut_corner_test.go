package main

// The corners of the cut workflow: the presses that land where there is
// nothing to press on, and the three places that each had their own idea of
// what a fade or a too-short selection is. Every test here started as a bug
// found by walking the workflow rather than by using it, which is why the
// footage in them is deliberately awkward -- an empty session, a hole between
// two recordings, an effect dragged down to a sliver.

import (
	"math"
	"os"
	"strings"
	"testing"
)

// Before any recording is loaded the source track is a blank strip that still
// takes clicks, and a click asks what time it landed on. That walk used to run
// off the end of an empty list and take the window with it. The same blank
// strip is what Revert leaves behind, so it is reachable twice.
func TestClickingAnEmptyTimelineAsksForTimeZero(t *testing.T) {
	ed := &cutEditor{pps: 4, totalW: 800}
	for _, x := range []float64{0, 1, 400, 1e6, -20} {
		if got := ed.tAt(x); got != 0 {
			t.Errorf("tAt(%v) on an empty timeline gave %v, want 0", x, got)
		}
		if got := ed.tAtView(x); got != 0 {
			t.Errorf("tAtView(%v) on an empty timeline gave %v, want 0", x, got)
		}
	}
}

// Dragging the end of a band inwards used to leave the fades at the lengths
// they had when the band was long, so a ten-second zoom pulled down to one
// second still claimed two seconds of glide either side. The dialog went on
// showing 2 and 2; the render, which clamps what it is given, played one
// second of arrival and no departure at all.
func TestShrinkingABandBringsItsFadesInWithIt(t *testing.T) {
	ed := newTestEd(t)
	ed.vids = []tlVideo{{base: "v", path: "v.mkv", start: 0, dur: 300, interval: 5, fps: 30}}
	ed.relayout()
	ed.fx = []cutFx{{Kind: "zoom", T: 10, Trans: 2, Dur: 10, Tout: 2}} // the band is 10..20
	ed.fxOn, ed.fxSel = true, 0
	ed.resizeFxTo(true, 11) // pull the end back to 11: a one-second band

	f := ed.fx[0]
	if f.Dur > 1+1e-9 {
		t.Fatalf("the band kept %.2f s of the drag to 11, want 1", f.Dur)
	}
	if f.Trans+f.Tout > f.Dur+1e-9 {
		t.Errorf("fades %.2f/%.2f over a %.2f s band — more fading than there are "+
			"seconds to fade in", f.Trans, f.Tout, f.Dur)
	}
	// and what is stored is what plays: the render clamps again, and must
	// find nothing left to clamp
	if in, out := f.zoomGlides(); math.Abs(in-f.Trans) > 1e-9 || math.Abs(out-f.Tout) > 1e-9 {
		t.Errorf("the dialog would show %.2f/%.2f and the render plays %.2f/%.2f",
			f.Trans, f.Tout, in, out)
	}
}

// ‹f and f› move an effect a frame at a time, and a frame at a time used to be
// enough to walk one clean off the end of the session: past the last recording
// there is no lane to draw it on and no press that can reach it again, so the
// effect was gone while the file still said it was there. A clip and an edge
// have always been held to their recording; an effect is held to the session.
func TestAnEffectCannotBeNudgedOffTheEndOfTheSession(t *testing.T) {
	ed := newTestEd(t)
	ed.vids = []tlVideo{{base: "v", path: "v.mkv", start: 0, dur: 30, interval: 5, fps: 30}}
	ed.relayout()
	ed.fx = []cutFx{{Kind: "text", T: 25, Dur: 3, Text: "hello"}}
	ed.fxOn, ed.fxSel = true, 0

	for i := 0; i < 200; i++ {
		ed.nudgeFx(+1)
	}
	if f := ed.fx[0]; f.T+f.Dur > 30+1e-9 {
		t.Errorf("200 nudges put the band at %.2f..%.2f, past the 30 s session",
			f.T, f.T+f.Dur)
	}
	for i := 0; i < 500; i++ {
		ed.nudgeFx(-1)
	}
	if f := ed.fx[0]; f.T < 0 {
		t.Errorf("500 nudges back put the effect at %.2f, before the session began", f.T)
	}
	// dropped straight past the end by a drag, it lands on the last seconds
	// rather than beyond them
	ed.moveFxTo(900, false)
	if f := ed.fx[0]; math.Abs(f.T-27) > 1e-9 {
		t.Errorf("a drag to 900 s left the band at %.2f, want its 3 s against the "+
			"30 s end", f.T)
	}
}

// There is one fade rule, and the numbers a dialog stores have to survive it,
// because the render applies it again on the way out. Anything the render
// would trim is a number the box shows you and the video does not play.
func TestWhatTheFadeBoxesStoreIsWhatThePlayerPlays(t *testing.T) {
	for _, f := range []cutFx{
		{Kind: "zoom", Dur: 3, Trans: 5, Tout: 5},     // twice the band, both ends
		{Kind: "zoom", Dur: 3, Trans: 4, Tout: 0},     // all of it on one end
		{Kind: "text", Dur: 2, Trans: 1.5, Tout: 1.5}, // exactly fills it: allowed
		{Kind: "text", Dur: 2, Trans: 0, Tout: 9},
		{Kind: "speed", Dur: 4, Rate: 0, Trans: 3, Tout: 1},
		{Kind: "speed", Dur: 0, Trans: 1, Tout: 1}, // no band at all
	} {
		got := f
		clampFades(&got)
		if got.Trans < 0 || got.Tout < 0 {
			t.Errorf("%+v: clamped to a negative fade %.2f/%.2f", f, got.Trans, got.Tout)
		}
		if got.Trans+got.Tout > math.Max(got.Dur, 0)+1e-9 {
			t.Errorf("%+v: kept %.2f/%.2f over a %.2f s band", f, got.Trans, got.Tout, got.Dur)
		}
		if in, out := got.zoomGlides(); math.Abs(in-got.Trans) > 1e-9 || math.Abs(out-got.Tout) > 1e-9 {
			t.Errorf("%+v: stored %.2f/%.2f, the camera glides %.2f/%.2f",
				f, got.Trans, got.Tout, in, out)
		}
		if in, out := got.textFades(); math.Abs(in-got.Trans) > 1e-9 || math.Abs(out-got.Tout) > 1e-9 {
			t.Errorf("%+v: stored %.2f/%.2f, the words fade %.2f/%.2f",
				f, got.Trans, got.Tout, in, out)
		}
	}
	// asked for the same on both ends and given less than that, both ends
	// still get the same: a fade drawn symmetric stays symmetric
	sym := cutFx{Kind: "zoom", Dur: 3, Trans: 5, Tout: 5}
	clampFades(&sym)
	if math.Abs(sym.Trans-sym.Tout) > 1e-9 {
		t.Errorf("5/5 over 3 s came back as %.2f/%.2f — lopsided", sym.Trans, sym.Tout)
	}
}

// All three dialogs go through the one rule on the way out. Without this they
// drift apart again the moment a fourth field is added to one of them, and the
// drift is invisible until a render comes out wrong.
func TestEveryEffectDialogAnswersToTheOneFadeRule(t *testing.T) {
	src, err := os.ReadFile("cut_fx.go")
	if err != nil {
		t.Fatal(err)
	}
	s := string(src)
	for _, ask := range []string{"askZoomParams", "askTextParams", "askSpeedParams"} {
		i := strings.Index(s, "func (a *App) "+ask+"(")
		if i < 0 {
			t.Fatalf("%s is gone", ask)
		}
		body := s[i:]
		if j := strings.Index(body[1:], "\nfunc "); j >= 0 {
			body = body[:j]
		}
		if !strings.Contains(body, "clampFades(&f)") {
			t.Errorf("%s stores its fades without clampFades — what it writes is not "+
				"what the render plays", ask)
		}
	}
}

// ＋ Add over a stretch that keeps nothing used to say "added" and push an undo
// step over a cut that had not moved. The two ways to select nothing are the
// hole between two recordings, which nobody filmed, and a sliver shorter than
// a scene, which addRange drops on the floor.
func TestAddOverNothingKeepableRefusesInsteadOfPretending(t *testing.T) {
	ed := newTestEd(t)
	a := ed.a
	a.ed = ed
	ed.vids = []tlVideo{
		{base: "a", path: "a.mkv", start: 0, dur: 20, interval: 5, fps: 30},
		{base: "b", path: "b.mkv", start: 40, dur: 20, interval: 5, fps: 30},
	}
	ed.relayout()

	for _, c := range []struct {
		what   string
		t0, t1 float64
	}{
		{"the hole between two recordings", 25, 35},
		{"a sliver shorter than a scene", 5, 5.4},
	} {
		ed.segs, ed.undo = nil, nil
		ed.sel.t0, ed.sel.t1, ed.sel.active = c.t0, c.t1, true
		a.addSelClicked()
		if len(ed.segs) != 0 {
			t.Errorf("%s: added %v", c.what, ed.segs)
		}
		if len(ed.undo) != 0 {
			t.Errorf("%s: left %d undo step(s) over a cut that did not move",
				c.what, len(ed.undo))
		}
		if !ed.sel.active {
			t.Errorf("%s: cleared the band, so there is nothing left to widen", c.what)
		}
	}

	// and the press still works where there is footage to keep
	ed.segs, ed.undo = nil, nil
	ed.sel.t0, ed.sel.t1, ed.sel.active = 5, 15, true
	a.addSelClicked()
	if len(ed.segs) != 1 || ed.segs[0].E-ed.segs[0].S < 9 {
		t.Fatalf("a 10 s selection on real footage added %v", ed.segs)
	}
	if len(ed.undo) != 1 {
		t.Errorf("the add that worked left %d undo steps, want 1", len(ed.undo))
	}
	if ed.sel.active {
		t.Error("the band stayed up after the add it belonged to")
	}
}

// The render drops a clip too short to encode. Silently, it reads as a piece
// of the cut going missing for no reason -- and the cause is nearly always an
// effect boundary landing a fraction short of a cut, which is a thing the hand
// can move once it is told where.
func TestADroppedSliverSaysWhyItWentAndWhere(t *testing.T) {
	src, err := os.ReadFile("produce.go")
	if err != nil {
		t.Fatal(err)
	}
	i := strings.Index(string(src), "if c.length < minClipLn {")
	if i < 0 {
		t.Fatal("the half-second floor is gone from the planner")
	}
	body := string(src)[i : i+600]
	if !strings.Contains(body, "a.logfIdle(") {
		t.Error("a clip is dropped from the render without a line in the log")
	}
	if !strings.Contains(body, "mmss(s.S)") {
		t.Error("the log does not say where the dropped stretch was")
	}
}
