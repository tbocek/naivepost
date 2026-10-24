package main

// Two speed effects over the same seconds.
//
// "A video is paused but overlaps with 2x? I think, add those 2 and divide by
// 2, same with 3 overlapping etc." -- so the mean, with a stop counting as the
// ×0 it is. Before this the later effect simply won, which made a stop under a
// ×2 mean nothing at all and a ×2 under a ×4 mean ×4.

import (
	"math"
	"testing"
)

func rateAt(fx []cutFx, t float64) float64 { return fxMeanRate(fx, t) }

// The case that has to keep working: one effect on its own comes out exactly
// what it asked for. A mean of one number is that number.
func TestOneEffectComesOutExactlyWhatItAskedFor(t *testing.T) {
	fast := []cutFx{{Kind: "speed", T: 10, Dur: 10, Rate: 2}}
	for _, at := range []float64{10, 12.5, 15, 19.9} {
		if got := rateAt(fast, at); math.Abs(got-2) > 1e-9 {
			t.Errorf("a lone ×2 reads ×%g at %g s", got, at)
		}
	}
	if got := rateAt(fast, 9.9); got != 1 {
		t.Errorf("before the band the footage runs at ×%g, want ×1", got)
	}
	if got := rateAt(fast, 20); got != 1 {
		t.Errorf("after the band the footage runs at ×%g, want ×1", got)
	}
	// a lone stop is ×0 -- and the footage under its still runs at ×1,
	// because nothing can encode ×0 and nothing can see it either
	stop := []cutFx{{Kind: "speed", T: 10, Dur: 3, Rate: 0}}
	if got := rateAt(stop, 11); got != 0 {
		t.Errorf("a lone stop reads ×%g, want the ×0 that freezes the picture", got)
	}
	if got := fxRateAt(stop, 11); got != 1 {
		t.Errorf("the footage under a lone stop runs at ×%g, want ×1", got)
	}
	if !sameSeg(applyFx([]cutSeg{{S: 0, E: 60}}, stop)[0], cutSeg{S: 0, E: 60}) {
		t.Error("a lone stop cut the segments open")
	}
}

// The arithmetic, in the words it was asked for.
func TestTwoEffectsOverTheSameSecondsShareThem(t *testing.T) {
	for _, c := range []struct {
		what string
		fx   []cutFx
		want float64
	}{
		{"a stop under a ×2", []cutFx{
			{Kind: "speed", T: 10, Dur: 10, Rate: 2},
			{Kind: "speed", T: 12, Dur: 3, Rate: 0},
		}, 1},
		{"a ×2 under a ×4", []cutFx{
			{Kind: "speed", T: 10, Dur: 10, Rate: 2},
			{Kind: "speed", T: 12, Dur: 3, Rate: 4},
		}, 3},
		{"slow motion under a stop", []cutFx{
			{Kind: "speed", T: 10, Dur: 10, Rate: 0.5},
			{Kind: "speed", T: 12, Dur: 3, Rate: 0},
		}, 0.25},
		{"three at once", []cutFx{
			{Kind: "speed", T: 10, Dur: 10, Rate: 0},
			{Kind: "speed", T: 11, Dur: 8, Rate: 2},
			{Kind: "speed", T: 12, Dur: 3, Rate: 4},
		}, 2},
	} {
		if got := rateAt(c.fx, 13); math.Abs(got-c.want) > 1e-9 {
			t.Errorf("%s comes out ×%g, want ×%g", c.what, got, c.want)
		}
		// and outside the overlap the first effect is alone again
		if got := rateAt(c.fx, 11.5); len(c.fx) == 2 && math.Abs(got-c.fx[0].Rate) > 1e-9 {
			t.Errorf("%s: outside the overlap the footage runs at ×%g, want ×%g",
				c.what, got, c.fx[0].Rate)
		}
	}
}

// A stop is diluted rather than obeyed: where something else crosses it the
// mean is no longer nought, so the picture runs for exactly those seconds and
// the still comes back after them.
func TestAStopStandsOverExactlyTheSecondsItIsTheOnlyThingOn(t *testing.T) {
	fx := []cutFx{
		{Kind: "speed", T: 10, Dur: 6, Rate: 0, Trans: 0.5, Tout: 0.5},
		{Kind: "speed", T: 12, Dur: 2, Rate: 2},
	}
	for _, at := range []float64{10.1, 11.9, 14.1, 15.9} {
		if freezeNow(fx, at) == nil {
			t.Errorf("no still at %g s, which the stop has to itself", at)
		}
	}
	for _, at := range []float64{12, 13, 13.9} {
		if f := freezeNow(fx, at); f != nil {
			t.Errorf("a still at %g s, where a ×2 crosses the stop and the mean is ×%g",
				at, rateAt(fx, at))
		}
	}
	// the band comes apart into exactly the two stretches it has to itself
	got := frozenSpans(fx, fx[0])
	want := [][2]float64{{10, 12}, {14, 16}}
	if len(got) != len(want) {
		t.Fatalf("the stop stands over %v, want %v", got, want)
	}
	for i := range want {
		if math.Abs(got[i][0]-want[i][0]) > 1e-9 || math.Abs(got[i][1]-want[i][1]) > 1e-9 {
			t.Errorf("stretch %d is %v, want %v", i, got[i], want[i])
		}
	}
	// with nothing over it, a stop stands over the whole of its own bar
	alone := []cutFx{fx[0]}
	if got := frozenSpans(alone, fx[0]); len(got) != 1 || got[0] != [2]float64{10, 16} {
		t.Errorf("a stop with nothing over it stands over %v, want the whole bar 10..16", got)
	}
}

// The render's side of that: the still is composited over the stretches it
// stands on, and the fades belong to the bar's own ends. An edge made by a
// crossing effect is hard -- the picture there does not fade into motion.
func TestTheStillIsCompositedOverExactlyThoseStretches(t *testing.T) {
	fx := []cutFx{
		{Kind: "speed", T: 10, Dur: 6, Rate: 0, Trans: 0.5, Tout: 0.5},
		{Kind: "speed", T: 12, Dur: 2, Rate: 2},
	}
	// one clip covering session 0..60 at ×1
	cues := freezeCues(fx, 0, 60, 1, 60)
	if len(cues) != 2 {
		t.Fatalf("the render composites %d stills, want the two stretches the stop has", len(cues))
	}
	if math.Abs(cues[0].s-10) > 1e-9 || math.Abs(cues[0].e-12) > 1e-9 {
		t.Errorf("the first still runs %.2f..%.2f, want 10..12", cues[0].s, cues[0].e)
	}
	if math.Abs(cues[1].s-14) > 1e-9 || math.Abs(cues[1].e-16) > 1e-9 {
		t.Errorf("the second still runs %.2f..%.2f, want 14..16", cues[1].s, cues[1].e)
	}
	// the bar's own ends fade; the edges the ×2 made do not
	if cues[0].fin <= 0 {
		t.Error("the still does not fade in at the start of the bar")
	}
	if cues[0].fout != 0 {
		t.Errorf("the still fades out over %.2f s where the ×2 cuts in, want a hard edge",
			cues[0].fout)
	}
	if cues[1].fin != 0 {
		t.Errorf("the still fades in over %.2f s where the ×2 lets go, want a hard edge",
			cues[1].fin)
	}
	if cues[1].fout <= 0 {
		t.Error("the still does not fade out at the end of the bar")
	}
}

// The hole this opens, and the guard on it. clampSpeed holds ONE effect to a
// rate its band can pay for; two bands that overlap by a sliver slice a
// stretch out of each other that no clamp saw, and a clip under minClipLn is
// not encoded at all -- the footage under it leaves the video in silence.
func TestNoOverlapMakesAStretchTheRenderWouldDrop(t *testing.T) {
	rates := []float64{0, 0.25, 0.5, 1, 2, 4, 8, 20, 100}
	offs := []float64{-4, -0.05, 0, 0.05, 1, 2.5, 4.95, 5}
	for _, ra := range rates {
		for _, rb := range rates {
			for _, off := range offs {
				fx := []cutFx{
					dialogSpeed(cutFx{Kind: "speed", T: 10, Dur: 5, Rate: ra}),
					dialogSpeed(cutFx{Kind: "speed", T: 10 + off, Dur: 5, Rate: rb}),
				}
				sp := rateSpans(fx)
				for i, st := range sp {
					if !spanCuts(sp, i) {
						continue // it makes no clip of its own; nothing to drop
					}
					if st.outLn() < minClipLn-1e-9 {
						t.Fatalf("×%g and ×%g overlapping by %g s leave a %.3f s stretch "+
							"at ×%.2f — %.3f s on screen, under the render's %.2f",
							ra, rb, 5-math.Abs(off), st.t1-st.t0, st.rate, st.outLn(), minClipLn)
					}
				}
				// and whatever the healing did, the stretches stay in order
				// and never overlap each other
				for i := 1; i < len(sp); i++ {
					if sp[i].t0 < sp[i-1].t1-1e-9 {
						t.Fatalf("×%g and ×%g at %g: stretch %d starts at %.3f, inside the "+
							"one before it ending at %.3f", ra, rb, off, i, sp[i].t0, sp[i-1].t1)
					}
				}
			}
		}
	}
}

// The seam: the preview's clock and the render's clips read the same
// stretches, so what you hear scrubbing is what comes out.
func TestThePreviewAndTheRenderReadTheSameStretches(t *testing.T) {
	fx := []cutFx{
		{Kind: "speed", T: 10, Dur: 10, Rate: 2},
		{Kind: "speed", T: 12, Dur: 3, Rate: 0},
	}
	segs := applyFx([]cutSeg{{S: 0, E: 60}}, fx)
	for _, s := range segs {
		if s.E <= s.S {
			continue
		}
		mid := (s.S + s.E) / 2
		want := s.Rate
		if want == 0 {
			want = 1
		}
		if got := fxRateAt(fx, mid); math.Abs(got-want) > 1e-9 {
			t.Errorf("the render runs %.1f..%.1f at ×%g and the preview at ×%g",
				s.S, s.E, want, got)
		}
	}
}
