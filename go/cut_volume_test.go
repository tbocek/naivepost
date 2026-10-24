package main

// The volume effect: a stretch of the session played louder or quieter than it
// was recorded. It is the only effect that touches nothing you can see, which
// makes one thing worth pinning above all others -- the seconds the lane draws,
// the seconds the preview plays and the seconds ffmpeg is told about are the
// same seconds, at the same gain.

import (
	"fmt"
	"math"
	"strings"
	"testing"
)

// The envelope: nothing outside the band, the full gain across the middle, and
// a straight ramp over each fade -- the same shape the lane draws.
func TestAVolumeEffectIsHeardAtTheGainItWasGiven(t *testing.T) {
	fx := []cutFx{{Kind: "volume", T: 10, Dur: 4, Gain: 3, Trans: 1, Tout: 1}}
	for _, tc := range []struct {
		at   float64
		want float64
		why  string
	}{
		{9.9, 1, "before it starts, the footage is as recorded"},
		{10, 1, "the first moment is the bottom of the fade in"},
		{10.5, 2, "half way up the fade in is half way to the gain"},
		{11, 3, "the fade in is over"},
		{12, 3, "the middle holds"},
		{13.5, 2, "half way down the fade out"},
		{14, 1, "the far end is the footage again"},
		{20, 1, "well past it, nothing"},
	} {
		if got := fxGainAt(fx, tc.at); math.Abs(got-tc.want) > 1e-9 {
			t.Errorf("at %.1fs the gain is %.3f, want %.3f — %s", tc.at, got, tc.want, tc.why)
		}
	}
}

// Two of them over the same second do both things, not the louder of the two:
// twice as loud and then twice again is four times, which is what a hand that
// placed two of them asked for. Rates average (rateSpans); gains do not.
func TestTwoVolumeEffectsOverOneSecondMultiply(t *testing.T) {
	fx := []cutFx{
		{Kind: "volume", T: 0, Dur: 10, Gain: 2},
		{Kind: "volume", T: 4, Dur: 2, Gain: 2},
	}
	if got := fxGainAt(fx, 2); math.Abs(got-2) > 1e-9 {
		t.Errorf("under one effect the gain is %.3f, want 2", got)
	}
	if got := fxGainAt(fx, 5); math.Abs(got-4) > 1e-9 {
		t.Errorf("under both the gain is %.3f, want 4", got)
	}
	// and the pair cannot go somewhere neither of them could
	loud := []cutFx{
		{Kind: "volume", T: 0, Dur: 10, Gain: fxMaxGain},
		{Kind: "volume", T: 0, Dur: 10, Gain: fxMaxGain},
	}
	if got := fxGainAt(loud, 5); got != fxMaxGain {
		t.Errorf("two effects at the ceiling reach %.3f, want the ceiling %.1f", got, fxMaxGain)
	}
}

// Everything else in the cut is deaf to it, and a band with no seconds does
// nothing -- a gain over no time is not silence, it is an effect that was never
// really placed.
func TestOnlyAVolumeEffectChangesTheVolume(t *testing.T) {
	for _, f := range []cutFx{
		{Kind: "zoom", T: 0, Dur: 4, Hf: 0.5},
		{Kind: "speed", T: 0, Dur: 4, Rate: 0.5},
		{Kind: "text", T: 0, Dur: 4, Text: "hello"},
		{Kind: "volume", T: 0, Dur: 0, Gain: 4},
	} {
		if got := fxGainAt([]cutFx{f}, 2); got != 1 {
			t.Errorf("a %s of %.1fs moved the gain to %.3f", f.Kind, f.Dur, got)
		}
	}
	// silence is a gain like any other, and it has to survive being the zero
	// value: an effect that exists was placed by a hand that typed 0
	if got := fxGainAt([]cutFx{{Kind: "volume", T: 0, Dur: 4}}, 2); got != 0 {
		t.Errorf("a 0%% effect plays at %.3f, want silence", got)
	}
}

// The render's half. gainCues maps a band onto one clip exactly as textCues
// maps a title onto it: the clip's own output seconds, divided by its rate.
func TestTheRenderTurnsUpTheSameSecondsTheLaneShows(t *testing.T) {
	fx := []cutFx{{Kind: "volume", T: 12, Dur: 4, Gain: 2, Trans: 1, Tout: 1}}
	// a clip starting at session 10 running at x2: four session seconds are
	// two clip seconds, and the fades halve with them
	cues := gainCues(fx, 10, 10, 2, 5)
	if len(cues) != 1 {
		t.Fatalf("%d cues on the clip the band falls in, want 1", len(cues))
	}
	c := cues[0]
	for _, tc := range []struct {
		got, want float64
		name      string
	}{{c.s, 1, "start"}, {c.e, 3, "end"}, {c.fin, 0.5, "fade in"}, {c.fout, 0.5, "fade out"}} {
		if math.Abs(tc.got-tc.want) > 1e-9 {
			t.Errorf("the cue's %s is %.3f clip seconds, want %.3f", tc.name, tc.got, tc.want)
		}
	}
	// a clip the band never reaches gets nothing...
	if got := gainCues(fx, 100, 10, 1, 10); len(got) != 0 {
		t.Errorf("%d cues on a clip the band misses entirely", len(got))
	}
	// ...and neither does one held session moment: a card or a freeze has no
	// stretch of the session under it for part of a band to cover
	if got := gainCues(fx, 12, 0, 1, 10); len(got) != 0 {
		t.Errorf("%d cues on a held frame", len(got))
	}
}

// The expression handed to ffmpeg IS the envelope: the band, the gain, and a
// straight ramp over each fade. Pinned as text because that text is what the
// render actually does.
func TestTheVolumeExpressionSaysWhatTheEnvelopeDoes(t *testing.T) {
	plain := gainExpr(textCue{fx: cutFx{Kind: "volume", Gain: 2}, s: 1, e: 3})
	if want := "if(between(t,1.000,3.000),2.0000,1)"; plain != want {
		t.Errorf("a gain with no fades renders as %q, want %q", plain, want)
	}
	both := gainExpr(textCue{fx: cutFx{Kind: "volume", Gain: 2}, s: 1, e: 3, fin: 0.5, fout: 0.5})
	if want := "if(between(t,1.000,3.000),1+1.0000*min(1,min((t-1.000)/0.500,(3.000-t)/0.500)),1)"; both != want {
		t.Errorf("a faded gain renders as %q, want %q", both, want)
	}
	// a quieting effect -- which is every gain under 100% -- is written with
	// its sign rather than as "1+-0.5000"
	down := gainExpr(textCue{fx: cutFx{Kind: "volume", Gain: 0.5}, s: 0, e: 2, fin: 0.5})
	if want := "if(between(t,0.000,2.000),1-0.5000*min(1,(t-0.000)/0.500),1)"; down != want {
		t.Errorf("a quieting gain renders as %q, want %q", down, want)
	}
	if strings.Contains(down, "+-") {
		t.Errorf("the expression spells a fall as a rise by a negative amount: %q", down)
	}
	// and the ceiling is enforced where the number leaves the app, not only
	// where it was typed
	if got := gainExpr(textCue{fx: cutFx{Kind: "volume", Gain: 1e6}, s: 0, e: 1}); !strings.Contains(got,
		fmt.Sprintf("%.4f", fxMaxGain)) {
		t.Errorf("an absurd gain reaches ffmpeg as %q", got)
	}
}

// Two cues on one clip are two filters in a row, so the render multiplies them
// exactly as the preview does.
func TestTheVolumeChainAppliesEveryCueInTurn(t *testing.T) {
	fc, lab := gainChain(nil, "game")
	if fc != "" || lab != "game" {
		t.Errorf("a clip with no volume effects rewrote its sound: %q -> %q", fc, lab)
	}
	fc, lab = gainChain([]textCue{
		{fx: cutFx{Kind: "volume", Gain: 2}, s: 0, e: 1},
		{fx: cutFx{Kind: "volume", Gain: 2}, s: 0, e: 1},
	}, "game")
	if lab != "gv1" {
		t.Errorf("the sound leaves on %q, want gv1", lab)
	}
	if !strings.Contains(fc, "[game]volume=") || !strings.Contains(fc, "[gv0]volume=") {
		t.Errorf("the second gain does not read the first one's output: %q", fc)
	}
	if n := strings.Count(fc, ":eval=frame"); n != 2 {
		t.Errorf("%d of the 2 gains are evaluated per frame; a constant one cannot fade", n)
	}
}

// It goes on the bed -- the capture and every separate recording that was
// running -- and before the narration, which is written on a later page and has
// a level of its own. A cut effect has no business over a voice the cut never
// heard.
func TestTheVolumeEffectLandsOnWhatWasRecorded(t *testing.T) {
	body := funcBody(t, "produce.go", `func \(a \*App\) encodeClip\(`)
	bed := strings.Index(body, `game = "bed"`)
	gain := strings.Index(body, "gainChain(c.gains, game)")
	spoken := strings.Index(body, "if len(spoken) > 0 {")
	if bed < 0 || gain < 0 || spoken < 0 {
		t.Fatalf("encodeClip no longer mixes a bed, a gain and the narration in that order")
	}
	if !(bed < gain && gain < spoken) {
		t.Errorf("the gain is applied at %d, outside bed (%d) and narration (%d)", gain, bed, spoken)
	}
}

// On the lane it is a speaker and a percentage, and in the status line it says
// which way it went. "louder" and "quieter" rather than a bare number, because
// the number is on the lane and the sentence is what tells you what happened.
func TestAVolumeEffectSaysWhichWayItWent(t *testing.T) {
	for _, tc := range []struct {
		gain float64
		mark string
		lane string
		word string
	}{
		{2, "vol", "200%", "louder"},
		{0.4, "vol", "40%", "quieter"},
		{0, "vol", "0%", "silent"},
	} {
		f := cutFx{Kind: "volume", T: 65, Dur: 3, Gain: tc.gain}
		mark, lane := laneLabel(f, 0)
		if mark != tc.mark || lane != tc.lane {
			t.Errorf("a %v gain draws (%q, %q), want (%q, %q)", tc.gain, mark, lane, tc.mark, tc.lane)
		}
		if lbl := f.fxLabel(); !strings.Contains(lbl, tc.word) || !strings.Contains(lbl, tc.lane) {
			t.Errorf("a %v gain introduces itself as %q, want %q and %q in it",
				tc.gain, lbl, tc.word, tc.lane)
		}
	}
	// and it owns its band on the timeline like every other kind
	f := cutFx{Kind: "volume", T: 10, Dur: 4}
	if t0, t1 := f.fxSpan(); t0 != 10 || t1 != 14 {
		t.Errorf("a volume effect covers %.1f-%.1f, want 10-14", t0, t1)
	}
}

// The toolbar's half: the sixth item on the effect dropdown opens the volume
// form, and it opens it on a stretch that already sounds like something --
// twice as loud, with a fade at each end, because a gain that arrives on one
// sample is a click.
func TestTheEffectDropdownOpensTheVolumeForm(t *testing.T) {
	src := readSrc(t, "cut.go")
	if !strings.Contains(src, "case 5:\n\t\t\ta.volumeClicked()") {
		t.Error("the sixth effect on the dropdown no longer opens the volume form")
	}
	body := funcBody(t, "cut_fx.go", `func \(a \*App\) volumeClicked\(`)
	for _, want := range []string{
		`cutFx{Kind: "volume", T: t0, Dur: t1 - t0, Gain: 2, Trans: 0.25, Tout: 0.25}`,
		"a.askVolumeParams(f, true,",
		"ed.addFx(f)",
	} {
		if !strings.Contains(body, want) {
			t.Errorf("volumeClicked no longer contains %q:\n%s", want, body)
		}
	}
	// and it will not place one over nothing: an effect needs seconds
	if !strings.Contains(body, "if !ed.hasPlay {") {
		t.Error("volumeClicked places an effect with neither a selection nor a playhead")
	}
	// ✎ Edit reopens the same form on an existing one, so the numbers are read
	// back and written by the one place that knows what they mean
	if !strings.Contains(funcBody(t, "cut_fx.go", `func \(a \*App\) editFx\(`),
		"case \"volume\":\n\t\ta.askVolumeParams(was, false,") {
		t.Error("editing a volume effect no longer reopens the volume form")
	}
}

// The form speaks percent and stores a factor, and the two ends of the range
// are held where the render and the preview can both make something of them.
func TestTheVolumeFormAsksInPercentAndStoresAFactor(t *testing.T) {
	body := funcBody(t, "cut_fx.go", `func \(a \*App\) askVolumeParams\(`)
	for _, want := range []string{
		`fxNumRow("Volume %"`,
		"clampGain(fxNumOf(g, clampGain(f.Gain)*100) / 100)",
		"clampFades(&f)",
	} {
		if !strings.Contains(body, want) {
			t.Errorf("askVolumeParams no longer contains %q:\n%s", want, body)
		}
	}
	for _, tc := range []struct{ in, want float64 }{
		{-1, 0}, {0, 0}, {1, 1}, {fxMaxGain, fxMaxGain}, {fxMaxGain + 5, fxMaxGain},
	} {
		if got := clampGain(tc.in); got != tc.want {
			t.Errorf("clampGain(%v) = %v, want %v", tc.in, got, tc.want)
		}
	}
	if got := gainPct(1.755); got != "176%" {
		t.Errorf("gainPct(1.755) = %q, want 176%%", got)
	}
}

// A cut moving under it takes it with it, the way it takes a title: the band is
// trimmed to the footage that is left, and the fades shrink in proportion.
func TestAVolumeEffectFollowsTheCutUnderIt(t *testing.T) {
	fx := []cutFx{{Kind: "volume", T: 5, Dur: 10, Gain: 2, Trans: 2, Tout: 2}}
	segs := []cutSeg{{S: 0, E: 10}}
	out := clampFxToSegs(fx, segs)
	if len(out) != 1 {
		t.Fatalf("the volume effect was dropped by the cut: %+v", out)
	}
	if out[0].T != 5 || math.Abs(out[0].Dur-5) > 1e-9 {
		t.Errorf("trimmed to %.1f for %.1fs, want 5 for 5s", out[0].T, out[0].Dur)
	}
	if out[0].Gain != 2 {
		t.Errorf("the trim changed the gain to %v", out[0].Gain)
	}
	if out[0].Trans+out[0].Tout > out[0].Dur {
		t.Errorf("fades %.2f/%.2f do not fit the %.2fs left", out[0].Trans, out[0].Tout, out[0].Dur)
	}
}
