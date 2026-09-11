package main

// Effects used to land beside the cut instead of inside it: the Shorts reply
// proposes segments and fx together, and the audit then fixes, drops and adds
// segments under them. The audit is asked to revisit the effects too
// (fxchecks), but that is a model being asked -- clampFxToSegs is the
// guarantee, holding whatever was proposed to the segments as applied,
// trimmed to fit or dropped. The clamp is what these test.

import (
	"math"
	"testing"
)

// two footage segments with a gap between them, and an insert that brings its
// own picture and may not host anything
var fxAlignSegs = []cutSeg{
	{S: 10, E: 40},
	{S: 60, E: 90},
	{S: 100, E: 110, Ins: "card.png"},
}

func TestAnEffectInsideTheCutIsKeptExactly(t *testing.T) {
	in := cutFx{Kind: "zoom", T: 15, Dur: 3, Cx: 0.5, Cy: 0.5, Hf: 0.6, Trans: 1, Tout: 1}
	got := clampFxToSegs([]cutFx{in}, fxAlignSegs)
	if len(got) != 1 || got[0] != in {
		t.Errorf("an effect already inside a segment came back as %v, want it untouched", got)
	}
}

func TestAnEffectOutsideTheCutIsDropped(t *testing.T) {
	for _, f := range []cutFx{
		{Kind: "zoom", T: 45, Dur: 5, Trans: 1, Tout: 1}, // the gap between the segments
		{Kind: "text", T: 95, Dur: 3, Text: "NO WAY"},    // past the last footage
		{Kind: "speed", T: 0, Dur: 8, Rate: 2},           // before the first
	} {
		if got := clampFxToSegs([]cutFx{f}, fxAlignSegs); len(got) != 0 {
			t.Errorf("a %s over nothing the cut keeps survived as %v", f.Kind, got)
		}
	}
}

// A zoom hanging over a segment's end is trimmed to the segment, and its
// glides shrink with it -- a 2 s zoom that kept its two full seconds of glide
// would have no hold left between them.
func TestAStraddlingZoomIsTrimmedAndItsGlidesShrink(t *testing.T) {
	got := clampFxToSegs([]cutFx{
		{Kind: "zoom", T: 38, Dur: 3, Cx: 0.5, Cy: 0.5, Hf: 0.6, Trans: 1, Tout: 1},
	}, fxAlignSegs)
	if len(got) != 1 {
		t.Fatalf("the straddling zoom did not survive: %v", got)
	}
	z := got[0]
	if z.T != 38 || z.Dur != 2 {
		t.Errorf("trimmed to [%g, %g+%g], want its segment's end at 40", z.T, z.T, z.Dur)
	}
	if z.Trans > z.Dur/3+1e-9 || z.Tout > z.Dur/3+1e-9 {
		t.Errorf("glides %g/%g over a %g s zoom — no hold left between them", z.Trans, z.Tout, z.Dur)
	}
	if z.Cx != 0.5 || z.Cy != 0.5 || z.Hf != 0.6 {
		t.Errorf("trimming rewrote the camera: %+v", z)
	}
}

// A caption pushed to a segment's edge keeps its words and shortens its
// fades; a speed keeps its seconds and gives up rate, exactly as the fx
// dialog would (clampSpeed).
func TestTrimmedCaptionsAndSpeedsStayPlayable(t *testing.T) {
	got := clampFxToSegs([]cutFx{
		{Kind: "text", T: 57, Dur: 4, Trans: 0.3, Tout: 0.3, Text: "HERE IT COMES"},
		{Kind: "speed", T: 39, Dur: 2, Rate: 100},
	}, fxAlignSegs)
	if len(got) != 2 {
		t.Fatalf("kept %d of the 2 straddling effects: %v", len(got), got)
	}
	tx := got[0]
	if tx.T != 60 || tx.Dur != 1 || tx.Text != "HERE IT COMES" {
		t.Errorf("the caption came out %+v, want its 1 s inside the segment with its words", tx)
	}
	if tx.Trans > tx.Dur/4+1e-9 || tx.Tout > tx.Dur/4+1e-9 {
		t.Errorf("fades %g/%g over a %g s caption", tx.Trans, tx.Tout, tx.Dur)
	}
	sp := got[1]
	if sp.T != 39 || sp.Dur != 1 {
		t.Errorf("the speed came out at [%g, %g+%g], want its segment's last second", sp.T, sp.T, sp.Dur)
	}
	// 1 s at rate 100 would play for 0.01 s -- nothing. The rate gives way.
	if want := sp.Dur / fxMinPlay; math.Abs(sp.Rate-want) > 1e-9 {
		t.Errorf("rate came out %g, want clampSpeed's %g over the trimmed second", sp.Rate, want)
	}
}

// An effect over two segments belongs to the one it shares the most time
// with; a sliver of overlap is not the moment that was meant, and goes.
func TestTheBiggerOverlapWinsAndSliversAreDropped(t *testing.T) {
	got := clampFxToSegs([]cutFx{
		{Kind: "zoom", T: 35, Dur: 35, Trans: 1, Tout: 1}, // 5 s in the first, 10 s in the second
	}, fxAlignSegs)
	if len(got) != 1 || got[0].T != 60 || got[0].Dur != 10 {
		t.Errorf("the two-segment zoom came out %v, want [60, 70] in the segment it mostly covers", got)
	}
	got = clampFxToSegs([]cutFx{
		{Kind: "zoom", T: 39.6, Dur: 5, Trans: 1, Tout: 1}, // 0.4 s of it is in the cut
	}, fxAlignSegs)
	if len(got) != 0 {
		t.Errorf("a zoom with 0.4 s inside the cut survived as %v", got)
	}
}

func TestAnInsertNeverHostsAnEffect(t *testing.T) {
	got := clampFxToSegs([]cutFx{
		{Kind: "zoom", T: 102, Dur: 3, Trans: 1, Tout: 1}, // squarely inside the card
	}, fxAlignSegs)
	if len(got) != 0 {
		t.Errorf("an effect inside an insert survived as %v — the card brings its own picture", got)
	}
}

// A point effect -- a hard reframing, with no fades and no seconds -- has no
// width to trim, so being inside a segment is the whole question. Everything
// with a band is trimmed to the footage under it, camera moves included: they
// all keep the same T-to-T+Dur shape now, so there is one rule for all of them.
func TestPointAndCameraEffectsAreClampedToTheFootage(t *testing.T) {
	got := clampFxToSegs([]cutFx{
		{Kind: "speed", T: 70, Dur: 2, Rate: 0},          // a freeze, inside
		{Kind: "speed", T: 50, Dur: 2, Rate: 0},          // a freeze, in the gap
		{Kind: "zoom", T: 30, Stay: true},                // a point, inside
		{Kind: "zoom", T: 50, Stay: true},                // a point, in the gap
		{Kind: "zoom", T: 20, Trans: 1, Dur: 4, Tout: 1}, // a band, inside
		{Kind: "zoom", T: 38, Trans: 1, Dur: 4, Tout: 1}, // a band over the edge
	}, fxAlignSegs)
	if len(got) != 4 {
		t.Fatalf("kept %d effects, want 4: %v", len(got), got)
	}
	if got[0].Kind != "speed" || got[0].T != 70 || got[0].Rate != 0 {
		t.Errorf("the inside freeze came out %+v", got[0])
	}
	if got[1].Kind != "zoom" || got[1].T != 30 || got[1].Dur != 0 {
		t.Errorf("the inside reframing came out %+v", got[1])
	}
	if got[2].Kind != "zoom" || got[2].T != 20 || got[2].Dur != 4 {
		t.Errorf("the band that lies inside was reshaped: %+v", got[2])
	}
	// [38, 42] over footage that ends at 40: two seconds of it are real, and
	// its fades shrink into what is left rather than hanging off the end
	c := got[3]
	if c.T != 38 || math.Abs(c.Dur-2) > 1e-9 {
		t.Errorf("the straddling band came out %+v, want 38 s for 2 s", c)
	}
	if c.Trans > c.Dur/3+1e-9 || c.Tout > c.Dur/3+1e-9 {
		t.Errorf("its fades %.2f/%.2f outgrew the trimmed band", c.Trans, c.Tout)
	}
}
