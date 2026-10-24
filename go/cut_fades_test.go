package main

// One shape for every effect. A zoom, a caption, a speed and a stop are four
// different things to do to a video, but they are drawn, timed, named and
// clamped as one: a bar from T to T+Dur, a fade in at its start and a fade out
// at its end, both INSIDE the bar, and both named the same in the dialog that
// asks for them. This is that claim, kind by kind, so a fifth effect cannot be
// added with a lead-in triangle hanging off the front or a fade-in and no fade
// out.

import (
	"math"
	"os"
	"strings"
	"testing"
)

// Every kind's bar is exactly the seconds it covers -- no kind spends time
// outside it, so two 3 s effects are the same width whatever their fades.
func TestEveryEffectsBarIsItsOwnSeconds(t *testing.T) {
	for _, f := range []cutFx{
		{Kind: "zoom", T: 10, Dur: 3, Trans: 1, Tout: 1},
		{Kind: "zoom", T: 10, Dur: 3, Trans: 1, Stay: true},
		{Kind: "text", T: 10, Dur: 3, Trans: 1, Tout: 1, Text: "hi"},
		{Kind: "speed", T: 10, Dur: 3, Rate: 0.5, Trans: 1, Tout: 1},
		{Kind: "speed", T: 10, Dur: 3, Rate: 0, Trans: 1, Tout: 1}, // a stop
	} {
		s, e := f.fxSpan()
		if s != 10 || e != 13 {
			t.Errorf("%s: the bar is [%.1f, %.1f], want [10, 13]", f.fxLabel(), s, e)
		}
	}
}

// Both sides, for every kind, and both inside the bar. A fade asked to be
// longer than the effect is trimmed rather than allowed to run past its end.
func TestEveryEffectFadesBothWaysInsideItsBar(t *testing.T) {
	for _, c := range []struct {
		name  string
		fades func(cutFx) (float64, float64)
	}{
		{"zoom", cutFx.zoomGlides},
		{"text", cutFx.textFades},
		{"speed", cutFx.speedRamps},
	} {
		f := cutFx{Dur: 4, Trans: 1, Tout: 1}
		in, out := c.fades(f)
		if in != 1 || out != 1 {
			t.Errorf("%s: fades %.2f/%.2f, want the 1s each side it asked for", c.name, in, out)
		}
		// asked for more than there is: trimmed to the bar, the way in first
		f = cutFx{Dur: 4, Trans: 3, Tout: 3}
		in, out = c.fades(f)
		if in+out > 4+1e-9 {
			t.Errorf("%s: fades %.2f/%.2f run past the %gs bar", c.name, in, out, f.Dur)
		}
		if in <= 0 || out <= 0 {
			t.Errorf("%s: trimming took a whole side away: %.2f/%.2f", c.name, in, out)
		}
	}
	// a staying zoom is the one thing with no fade out, and for a reason a
	// number cannot express: it never ends. Its fade in is still its own.
	stay := cutFx{Kind: "zoom", T: 0, Dur: 4, Trans: 1, Tout: 1, Stay: true}
	if got := stay.fxLabel(); strings.Contains(got, "out") {
		t.Errorf("fxLabel = %q — a zoom that never ends reports coming back off", got)
	}
}

// The clamp shrinks fades with the bar for every kind, so an effect trimmed to
// the footage still has a middle between its two fades.
func TestTrimmingAnEffectShrinksBothItsFades(t *testing.T) {
	segs := []cutSeg{{S: 0, E: 12}} // every band below straddles the end
	for _, f := range clampFxToSegs([]cutFx{
		{Kind: "zoom", T: 10, Dur: 6, Trans: 2, Tout: 2},
		{Kind: "text", T: 10, Dur: 6, Trans: 2, Tout: 2, Text: "hi"},
		{Kind: "speed", T: 10, Dur: 6, Rate: 0.5, Trans: 2, Tout: 2},
		{Kind: "speed", T: 10, Dur: 6, Rate: 0, Trans: 2, Tout: 2},
	}, segs) {
		if f.Dur > 2+1e-9 {
			t.Errorf("%s: kept %.2fs of a 2s overlap", f.Kind, f.Dur)
		}
		// 2 s of fade on a 6 s band is a third of it spent arriving, so a
		// third of it is what stays when the band comes down to 2
		if want := 2 * f.Dur / 6; math.Abs(f.Trans-want) > 1e-9 || math.Abs(f.Tout-want) > 1e-9 {
			t.Errorf("%s: fades %.2f/%.2f over a %.2fs bar, want %.2f each — "+
				"the same share of it they had of the 6 s", f.Kind, f.Trans, f.Tout, f.Dur, want)
		}
		if f.Trans+f.Tout >= f.Dur {
			t.Errorf("%s: fades %.2f/%.2f fill the whole %.2fs bar — it arrives "+
				"and leaves without ever holding", f.Kind, f.Trans, f.Tout, f.Dur)
		}
	}
}

// A suggested effect gets the same treatment whatever its kind: a fade either
// side, sized to the stretch, or none at all where it would not survive the
// render. A speed used to be the one kind handed a hard cut.
func TestSuggestedEffectsAllFadeBothWays(t *testing.T) {
	out := fxFromReply([]sugFx{
		{Kind: "zoom", Start: 0, End: 9},
		{Kind: "text", Start: 10, End: 19, Text: "hello"},
		{Kind: "speed", Start: 20, End: 29, Rate: 0.5},
	})
	if len(out) != 3 {
		t.Fatalf("got %d effects, want one of each kind: %+v", len(out), out)
	}
	for _, f := range out {
		if f.Trans <= 0 || f.Tout <= 0 {
			t.Errorf("%s came back with fades %.2f/%.2f, want both sides", f.Kind, f.Trans, f.Tout)
		}
		if f.Trans != f.Tout {
			t.Errorf("%s fades in over %.2fs and out over %.2fs", f.Kind, f.Trans, f.Tout)
		}
		if f.Trans+f.Tout > f.Dur {
			t.Errorf("%s: fades %.2f/%.2f run past its %.2fs", f.Kind, f.Trans, f.Tout, f.Dur)
		}
	}
	// too short to ramp: the render cannot build stairs under rampStep, so a
	// brief speed gets none rather than getting one it cannot honour
	short := fxFromReply([]sugFx{{Kind: "speed", Start: 0, End: 2, Rate: 0.5}})
	if len(short) != 1 {
		t.Fatalf("the short speed was dropped: %+v", short)
	}
	if short[0].Trans != 0 || short[0].Tout != 0 {
		t.Errorf("a %.1fs speed was given ramps of %.2f/%.2f the render would round away",
			short[0].Dur, short[0].Trans, short[0].Tout)
	}
	if math.Abs(short[0].Rate-0.5) > 1e-9 {
		t.Errorf("the short speed lost its rate: %+v", short[0])
	}
}

// The lane draws one envelope, and every kind draws it: full height where the
// effect holds, rising and falling across its fades. Nothing hangs off the
// front of a bar any more -- the old speed lead-in and the old view marker
// both did, and neither matched what the bar said it covered.
func TestTheLaneDrawsOneEnvelopeForEveryKind(t *testing.T) {
	b, err := os.ReadFile("cut_fx.go")
	if err != nil {
		t.Fatal(err)
	}
	s := string(b)
	// zoom, plain speed, stop, text, volume
	if n := strings.Count(s, "laneBand(cr, x0, x1,"); n != 5 {
		t.Errorf("%d kinds draw the shared envelope, want all 5", n)
	}
	// the envelope itself: a trapezium, rising over the fade in and falling
	// over the fade out, both clamped inside the bar
	for _, want := range []string{
		"inW = math.Max(0, math.Min(inW, x1-x0))",
		"outW = math.Max(0, math.Min(outW, x1-x0-inW))",
		"cr.MoveTo(x0, y+fxLaneH-2)",
		"cr.LineTo(x0+inW, y+2)",
		"cr.LineTo(x1-outW, y+2)",
		"cr.LineTo(x1, y+fxLaneH-2)",
	} {
		if !strings.Contains(s, want) {
			t.Errorf("laneBand no longer contains %q", want)
		}
	}
}
