package main

// Two speed effects over the same seconds: the rate is the MEAN of what they
// ask for. A stop is ×0 in that arithmetic, so the picture freezes only where
// the mean is nought -- a stop with nothing over it -- and a stop under a ×2
// comes out ×1.

import (
	"math"
	"sort"
)

// applied is the rate the render actually runs a stretch at. A frozen stretch
// has no observable rate -- the still covers it -- and ×0 is not a clip any
// encoder can build, so the footage under it runs at ×1, which is what it has
// always done (see cut_fxstill.go).
func (st rateStep) applied() float64 {
	if st.rate <= 0 {
		return 1
	}
	return st.rate
}

// outLn is how long a stretch comes out on screen, which is the length the
// render measures it by.
func (st rateStep) outLn() float64 { return (st.t1 - st.t0) / st.applied() }

// rateSpans is the whole cut's speed as flat stretches: every boundary any
// speed effect draws, and between two boundaries the rate they agree on.
// Seconds no effect covers are not in the list at all -- they run at ×1 by
// simply being left alone.
func rateSpans(fx []cutFx) []rateStep {
	var all []rateStep
	for _, f := range speedsOf(fx) {
		if f.Dur <= 0 {
			continue
		}
		if f.frozenFx() {
			// a stop is one flat ×0 over its band; it has no stairs, because
			// there is no ramping into and out of standing still
			all = append(all, rateStep{f.T, f.T + f.Dur, 0})
			continue
		}
		all = append(all, speedSteps(f)...)
	}
	if len(all) == 0 {
		return nil
	}
	cuts := make([]float64, 0, 2*len(all))
	for _, st := range all {
		cuts = append(cuts, st.t0, st.t1)
	}
	sort.Float64s(cuts)
	var out []rateStep
	for i := 1; i < len(cuts); i++ {
		t0, t1 := cuts[i-1], cuts[i]
		if t1-t0 < 1e-9 {
			continue
		}
		mid := (t0 + t1) / 2
		sum, n := 0.0, 0
		for _, st := range all {
			if mid >= st.t0 && mid < st.t1 {
				sum, n = sum+st.rate, n+1
			}
		}
		if n == 0 {
			continue // plain footage between two effects
		}
		out = append(out, rateStep{t0, t1, sum / float64(n)})
	}
	return healSpans(joinSpans(out))
}

// joinSpans folds together neighbours that ask for the same thing, so a
// boundary two effects happen to share does not become a cut in the video.
func joinSpans(sp []rateStep) []rateStep {
	var out []rateStep
	for _, st := range sp {
		if n := len(out); n > 0 && math.Abs(out[n-1].t1-st.t0) < 1e-9 &&
			math.Abs(out[n-1].rate-st.rate) < 1e-9 {
			out[n-1].t1 = st.t1
			continue
		}
		out = append(out, st)
	}
	return out
}

// healSpans gives away any stretch the render would drop: two bands
// overlapping by a tenth of a second slice a sliver under minClipLn out of
// each other, and such a clip is not encoded (planClips). The sliver goes to
// a neighbour -- slightly the wrong speed for a tenth of a second beats
// footage going missing.
func healSpans(sp []rateStep) []rateStep {
	for pass := 0; pass < len(sp)+1 && len(sp) > 0; pass++ {
		k := -1
		for i, st := range sp {
			if st.outLn() >= minClipLn-1e-9 || !spanCuts(sp, i) {
				continue
			}
			if k < 0 || st.outLn() < sp[k].outLn() {
				k = i // the thinnest one first; the rest may heal with it
			}
		}
		if k < 0 {
			return sp
		}
		sp = absorbSpan(sp, k)
	}
	return sp
}

// spanCuts is whether a stretch is a cut in the finished video at all. One
// that runs at the same speed as both its neighbours -- and the footage
// outside the list runs at ×1 -- makes no clip of its own, so it has no length
// for the render to find too short. This is what keeps a very short stop from
// being healed away: on its own it splits nothing.
func spanCuts(sp []rateStep, i int) bool {
	left, right := 1.0, 1.0
	if i > 0 {
		left = sp[i-1].applied()
	}
	if i < len(sp)-1 {
		right = sp[i+1].applied()
	}
	a := sp[i].applied()
	return math.Abs(a-left) > 1e-9 || math.Abs(a-right) > 1e-9
}

// absorbSpan hands stretch k's seconds to a neighbour -- the longer one, which
// is the one least changed by taking them -- or drops it when it stands alone.
func absorbSpan(sp []rateStep, k int) []rateStep {
	switch {
	case len(sp) == 1:
		return nil
	case k == 0:
		sp[1].t0 = sp[0].t0
		return joinSpans(sp[1:])
	case k == len(sp)-1:
		sp[k-1].t1 = sp[k].t1
		return joinSpans(sp[:k])
	}
	if sp[k-1].outLn() >= sp[k+1].outLn() {
		sp[k-1].t1 = sp[k].t1
	} else {
		sp[k+1].t0 = sp[k].t0
	}
	return joinSpans(append(sp[:k:k], sp[k+1:]...))
}

// fxMeanRate is what the effects covering session time t ask for between them,
// ×0 included: nought where the picture is frozen, ×1 where nothing covers t.
func fxMeanRate(fx []cutFx, t float64) float64 {
	for _, st := range rateSpans(fx) {
		if t >= st.t0 && t < st.t1 {
			return st.rate
		}
	}
	return 1
}

// frozenSpans is the session seconds a stop effect actually stands over: the
// stretches of its own band where the mean is nought. With nothing else over
// it that is the whole band, which is the ordinary case; where a ×2 crosses it
// the mean there is ×1 and the footage runs, so the still steps aside for
// exactly those seconds and comes back after them.
func frozenSpans(fx []cutFx, f cutFx) [][2]float64 {
	if !f.frozenFx() || f.Dur <= 0 {
		return nil
	}
	t0, t1 := f.T, f.T+f.Dur
	var out [][2]float64
	for _, st := range rateSpans(fx) {
		if st.rate > 0 || st.t1 <= t0 || st.t0 >= t1 {
			continue
		}
		s, e := math.Max(st.t0, t0), math.Min(st.t1, t1)
		if e-s < 1e-9 {
			continue
		}
		if n := len(out); n > 0 && math.Abs(out[n-1][1]-s) < 1e-9 {
			out[n-1][1] = e
			continue
		}
		out = append(out, [2]float64{s, e})
	}
	return out
}
