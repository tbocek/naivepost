package main

// A speed effect is measured on the finished video, not on the footage. At ×8
// a second of screen time costs eight seconds of footage, so every floor the
// effect answers to -- how short a band may be, how much one stair of a ramp
// costs -- has to be counted after the rate has had the footage.
//
// Counted before it, which is how it used to be, a fast effect quietly came
// apart: its stairs each came out a fraction of a second, the render dropped
// every one of them, and the seconds under them left the video altogether.

import (
	"fmt"
	"math"
	"strings"
	"testing"
)

// dialogSpeed is a speed effect as the dialog stores it: the rate held to what
// the render can make of the band, the fades held inside it.
func dialogSpeed(f cutFx) cutFx {
	f.Rate, f.Dur = clampSpeed(f.Rate, f.Dur)
	clampFades(&f)
	return f
}

// The case this came from: a ×8 over about three seconds at 2:30 came out as
// clips of 0.06, 0.47, 0.24 and 0.12 s, every one of them dropped, and three
// seconds of footage went missing from the video.
func TestAFastEffectOverAShortBandReachesTheVideo(t *testing.T) {
	f := dialogSpeed(cutFx{Kind: "speed", T: 150, Dur: 3.2, Rate: 8, Trans: 1, Tout: 1})
	steps := speedSteps(f)
	if len(steps) == 0 {
		t.Fatal("the effect makes no stretches at all")
	}
	for i, st := range steps {
		out := (st.t1 - st.t0) / st.rate
		if out < minClipLn-1e-9 {
			t.Errorf("stretch %d covers %.2f s at ×%.2f — %.2f s on screen, which the "+
				"render drops", i+1, st.t1-st.t0, st.rate, out)
		}
	}
	// the band is covered end to end: whatever it does with the seconds, none
	// of them may be left to no stretch at all
	if math.Abs(steps[0].t0-f.T) > 1e-9 {
		t.Errorf("the first stretch starts at %.2f, want the band's %.2f", steps[0].t0, f.T)
	}
	if last := steps[len(steps)-1]; math.Abs(last.t1-(f.T+f.Dur)) > 1e-9 {
		t.Errorf("the last stretch ends at %.2f, want the band's %.2f", last.t1, f.T+f.Dur)
	}
	for i := 1; i < len(steps); i++ {
		if math.Abs(steps[i].t0-steps[i-1].t1) > 1e-9 {
			t.Errorf("a gap between stretch %d and %d: %.2f to %.2f",
				i, i+1, steps[i-1].t1, steps[i].t0)
		}
	}
}

// The rule, over the whole range of rates and lengths the dialog will accept:
// nothing a speed effect makes is too short for the render to keep. It is the
// one property that matters, because the alternative is not a worse effect,
// it is footage that is on the timeline and not in the video.
func TestNoSpeedEffectMakesAStretchTheRenderWouldDrop(t *testing.T) {
	for _, rate := range []float64{0.05, 0.25, 0.5, 1, 1.5, 2, 3, 4, 8, 20, 50, 100} {
		for _, dur := range []float64{0.2, 0.5, 1, 1.5, 2, 3.2, 5, 8, 12, 30, 120} {
			for _, fade := range []float64{0, 0.2, 0.6, 1, 2, 4, dur / 2, dur} {
				f := dialogSpeed(cutFx{Kind: "speed", T: 150, Dur: dur,
					Rate: rate, Trans: fade, Tout: fade})
				for _, st := range speedSteps(f) {
					if out := (st.t1 - st.t0) / st.rate; out < minClipLn-1e-9 {
						t.Fatalf("×%g over %g s with %g s ramps: a %.3f s stretch at "+
							"×%.2f comes out %.3f s, under the render's %.2f",
							rate, dur, fade, st.t1-st.t0, st.rate, out, minClipLn)
					}
				}
			}
		}
	}
}

// The lane draws the ramps the render builds. When a staircase is dropped for
// being unbuildable the marker has to stop drawing its triangles too, or the
// bar shows a ramp the video does not have.
func TestTheLaneDrawsTheRampsTheRenderActuallyBuilds(t *testing.T) {
	for _, f := range []cutFx{
		{Kind: "speed", T: 150, Dur: 3.2, Rate: 8, Trans: 1, Tout: 1},  // no room to ramp
		{Kind: "speed", T: 150, Dur: 60, Rate: 8, Trans: 10, Tout: 10}, // room to spare
		{Kind: "speed", T: 150, Dur: 20, Rate: 2, Trans: 4, Tout: 4},   // a gentle one
		{Kind: "speed", T: 150, Dur: 12, Rate: 0.5, Trans: 3, Tout: 3}, // slow motion
	} {
		f = dialogSpeed(f)
		in, out := f.speedRamps()
		steps := speedSteps(f)
		flat := len(steps) == 1 && steps[0].rate == f.Rate
		if flat != (in <= 0 && out <= 0) {
			t.Errorf("×%g over %g s: the lane draws ramps of %.2f/%.2f and the render "+
				"builds %d stretch(es)", f.Rate, f.Dur, in, out, len(steps))
		}
		if in+out > f.Dur+1e-9 {
			t.Errorf("×%g over %g s: ramps of %.2f/%.2f overrun the bar",
				f.Rate, f.Dur, in, out)
		}
	}
}

// A ramp costs footage in proportion to the rate it climbs to, so the same
// seconds of fade buy a ramp at ×2 and buy none at ×20. This is the rule the
// dialog's own hint describes, checked where it is enforced.
func TestARampCostsMoreFootageTheFasterItGoes(t *testing.T) {
	const band = 120 // long enough that the band itself is never the limit
	slow := dialogSpeed(cutFx{Kind: "speed", T: 0, Dur: band, Rate: 2, Trans: 2, Tout: 2})
	if in, _ := slow.speedRamps(); in <= 0 {
		t.Errorf("2 s of ramp at ×2 bought nothing, want a ramp")
	}
	fast := dialogSpeed(cutFx{Kind: "speed", T: 0, Dur: band, Rate: 20, Trans: 2, Tout: 2})
	if in, _ := fast.speedRamps(); in > 0 {
		t.Errorf("2 s of ramp at ×20 bought a ramp of %.2f — one stair of it costs "+
			"%.2f s of footage", in, rampStep*math.Sqrt(20))
	}
	// and the same rate over more seconds does buy one
	paid := dialogSpeed(cutFx{Kind: "speed", T: 0, Dur: band, Rate: 20, Trans: 10, Tout: 10})
	if in, _ := paid.speedRamps(); in <= 0 {
		t.Errorf("10 s of ramp at ×20 still bought nothing")
	}
}

// The floors upstream are the render's own, not numbers of their own. Two
// floors that drift apart is how a clip the editor thought was fine reaches a
// render that drops it.
func TestTheSpeedFloorsAreTheRendersFloor(t *testing.T) {
	if fxMinPlay != minClipLn {
		t.Errorf("the speed clamp allows %g s on screen where the render keeps %g s",
			fxMinPlay, minClipLn)
	}
	// clampSpeed gives up rate rather than seconds, and gives up exactly
	// enough that what is left renders
	for _, c := range []struct{ rate, dur float64 }{{8, 3.2}, {100, 1}, {20, 2}, {4, 0.5}} {
		r, d := clampSpeed(c.rate, c.dur)
		if d/r < minClipLn-1e-9 {
			t.Errorf("×%g over %g s clamped to ×%g over %g s — %.3f s on screen",
				c.rate, c.dur, r, d, d/r)
		}
		if r > c.rate+1e-9 {
			t.Errorf("×%g over %g s was made FASTER, to ×%g", c.rate, c.dur, r)
		}
	}
}

// A ramp has to ramp. The case this came from: ×8 over 32 s with 8 s either
// way came out of the render as ×1, ×2.83, ×8, ×2.83, ×1 -- one stair per
// ramp, so the "ramp up" was a single constant speed and the video stepped
// three times instead of climbing. The sound stepped with it, which is how it
// was noticed: atempo follows the rate exactly, so a rate that does not climb
// is a sound that does not climb.
//
// The cause was the price. A stair was charged the rate at the ramp's TOP,
// but the fastest stair runs half a stair short of the top -- and asked for
// one stair, that stair sits at the geometric middle, ×2.83 rather than ×8.
// The ramp was told it could afford one stair when it could plainly afford
// two.
func TestARampClimbsInMoreThanOneStep(t *testing.T) {
	f := dialogSpeed(cutFx{Kind: "speed", T: 152.0593, Dur: 32, Rate: 8, Trans: 8, Tout: 8})
	steps := speedSteps(f)
	var up []float64
	for _, st := range steps {
		if st.rate >= f.Rate-1e-9 {
			break
		}
		up = append(up, st.rate)
	}
	if len(up) < 2 {
		t.Errorf("×8 over 32 s with 8 s ramps climbs in %d step(s) — %v, so the ramp "+
			"is one constant speed and not a ramp", len(up), up)
	}
	for i := 1; i < len(up); i++ {
		if up[i] <= up[i-1] {
			t.Errorf("the way up goes %v, which does not climb", up)
			break
		}
	}
	// and the last stair of the way up is nearer ×8 than the middle of the
	// ramp: a staircase that stops at the geometric middle never arrives
	if n := len(up); n > 0 && up[n-1] <= math.Sqrt(f.Rate)+1e-9 {
		t.Errorf("the way up stops at ×%.3f, which is no further than the ramp's "+
			"middle ×%.3f", up[n-1], math.Sqrt(f.Rate))
	}
}

// The count is honest in both directions: a ramp takes every stair it can pay
// for, and no stair it cannot. The second half is the one that matters -- a
// stair under the floor is not a coarser ramp, it is footage the render drops.
func TestARampTakesEveryStairItCanPayForAndNoMore(t *testing.T) {
	for _, rate := range []float64{0.05, 0.25, 0.5, 1.5, 2, 4, 8, 20, 100} {
		for _, d := range []float64{0.5, 1, 2, 4, 8, 20, 60} {
			n := rampStairs(d, 1, rate)
			if n < 1 {
				t.Fatalf("a ramp to ×%g over %g s got %d stairs", rate, d, n)
			}
			if n > 1 && !rampFits(d, 1, rate, n) {
				t.Errorf("a ramp to ×%g over %g s took %d stairs it cannot pay for",
					rate, d, n)
			}
			if rampFits(d, 1, rate, n+1) {
				t.Errorf("a ramp to ×%g over %g s took %d stairs where %d fit",
					rate, d, n, n+1)
			}
			// the way down costs exactly what the way up costs
			if m := rampStairs(d, rate, 1); m != n {
				t.Errorf("a ramp to ×%g over %g s climbs in %d stairs and comes back "+
					"in %d", rate, d, n, m)
			}
		}
	}
}

// The sound rides the same staircase as the picture. Nothing in the render
// gives audio a rate of its own: each clip is retimed by atempoChain of the
// clip's own speed, so a ramp the picture climbs is a ramp the sound climbs,
// and a ramp the picture takes in one step is one the sound takes in one step
// too. That is why a staircase too coarse to see is still coarse enough to
// hear -- and why the fix is in the stairs and not in the filter.
func TestTheSoundRidesTheSameStaircaseAsThePicture(t *testing.T) {
	f := dialogSpeed(cutFx{Kind: "speed", T: 10, Dur: 32, Rate: 8, Trans: 8, Tout: 8})
	segs := applyFx(splitSpliced([]cutSeg{{S: 0, E: 60}}), []cutFx{f})
	var rates []float64
	for _, s := range segs {
		if s.Rate > 0 && s.Rate != 1 {
			rates = append(rates, s.Rate)
		}
	}
	if len(rates) < 5 {
		t.Fatalf("the cut came back with %d sped-up clips — %v", len(rates), rates)
	}
	for _, r := range rates {
		chain := atempoChain(r)
		if chain == "" {
			t.Errorf("a clip at ×%.3f gets no atempo, so its sound plays at ×1 under a "+
				"picture at ×%.3f", r, r)
			continue
		}
		got := 1.0
		for _, part := range strings.Split(strings.TrimPrefix(chain, ","), ",") {
			var v float64
			if _, err := fmt.Sscanf(part, "atempo=%g", &v); err != nil {
				t.Fatalf("a clip at ×%.3f gets %q, which is not an atempo chain", r, chain)
			}
			got *= v
		}
		if math.Abs(got-r) > 1e-6*r {
			t.Errorf("a clip at ×%.3f has its sound retimed by ×%.6f", r, got)
		}
	}
}
