package main

// The stop effect on its new terms: a still standing over footage that keeps
// running. The frame at the marker covers exactly the session seconds its bar
// covers, faded on and off like a title, and the cut is the same length with
// or without it -- no segments are cut open, no clock is slowed. The preview
// shows the still whenever the playhead is INSIDE the bar (freezeNow), which
// covers playing into it, seeking into it and parking in it alike; the render
// composites the same frame over the same seconds (freezeCues, encodeClip).
// The arithmetic is exercised; the GTK and ffmpeg wiring is pinned.

import (
	"encoding/json"
	"math"
	"os"
	"strings"
	"testing"
)

// A stop no longer touches the cut: the segments come through untouched, the
// clock under the still runs at full speed, and the bar spans [T, T+Dur].
func TestAStopIsAnOverlayNotACut(t *testing.T) {
	fx := []cutFx{{Kind: "speed", T: 100, Dur: 2, Rate: 0, Trans: 0.5, Tout: 0.5}}
	segs := []cutSeg{{S: 90, E: 110}}
	out := applyFx(segs, fx)
	if len(out) != 1 || !sameSeg(out[0], segs[0]) {
		t.Fatalf("a stop cut the segments open: %v", out)
	}
	// the footage -- and its sound -- run on underneath the still
	for _, at := range []float64{99, 100, 101, 102.5} {
		if r := fxRateAt(fx, at); r != 1 {
			t.Errorf("at %.1f the clock runs at %.2f, want full speed under the still", at, r)
		}
	}
	// the bar IS the seconds the still is on screen
	s, e := fx[0].fxSpan()
	if s != 100 || e != 102 {
		t.Errorf("fxSpan = [%.1f, %.1f], want the bar [100, 102]", s, e)
	}
	// a slow-motion effect still rates the clock; only Rate 0 is a stop
	if r := fxRateAt([]cutFx{{Kind: "speed", T: 100, Dur: 2, Rate: 0.5}}, 101); r != 0.5 {
		t.Errorf("slow motion rates the clock at %.2f, want 0.5", r)
	}
}

// The preview's trigger is position, not a crossing: the still is owed to the
// screen whenever the playhead is inside the bar, however it got there.
func TestTheStillFollowsThePlayheadPosition(t *testing.T) {
	fx := []cutFx{{Kind: "speed", T: 100, Dur: 2, Rate: 0}}
	if got := freezeNow(fx, 100.5); got == nil || got.T != 100 {
		t.Fatalf("inside the bar freezeNow found %v, want the stop", got)
	}
	// a seek that lands anywhere in the bar shows the still -- the old
	// crossing-based hold missed exactly this
	if got := freezeNow(fx, 101.9); got == nil {
		t.Error("a seek landing late in the bar found no still")
	}
	if got := freezeNow(fx, 100); got == nil {
		t.Error("the bar's own first moment found no still")
	}
	if got := freezeNow(fx, 99.9); got != nil {
		t.Error("before the bar the still was already up")
	}
	if got := freezeNow(fx, 102); got != nil {
		t.Error("at the bar's end the still was still up")
	}
	if got := freezeNow([]cutFx{{Kind: "speed", T: 100, Dur: 2, Rate: 0.5}}, 101); got != nil {
		t.Error("a slow-motion effect put a still up")
	}

	// the fades ride the widget's opacity, evaluated exactly as the render's
	// fade filters are: textAlpha reads the same Trans/Tout bargain
	f := cutFx{Kind: "speed", T: 100, Dur: 2, Rate: 0, Trans: 0.5, Tout: 0.5}
	if a := textAlpha(f, 100.25); math.Abs(a-0.5) > 1e-6 {
		t.Errorf("mid fade-in the still stands at %.2f, want 0.5", a)
	}
	if a := textAlpha(f, 101); a != 1 {
		t.Errorf("on the plateau the still stands at %.2f, want 1", a)
	}
	if a := textAlpha(f, 101.75); math.Abs(a-0.5) > 1e-6 {
		t.Errorf("mid fade-out the still stands at %.2f, want 0.5", a)
	}
}

// freezeCues maps a stop onto a clip on exactly textCues' terms, so a still
// and a title placed at the same second come and go together in the render.
func TestFreezeCuesMapLikeTextCues(t *testing.T) {
	fx := []cutFx{
		{Kind: "speed", T: 100, Dur: 2, Rate: 0, Trans: 0.5, Tout: 0.5},
		{Kind: "speed", T: 300, Dur: 3, Rate: 0.5}, // slow motion, not a stop
		{Kind: "text", T: 100, Dur: 2, Text: "hi"}, // a title is not a still
	}
	// a clip from 90 for 30 s at full speed holds the whole bar
	cues := freezeCues(fx, 90, 30, 1, 30)
	if len(cues) != 1 {
		t.Fatalf("got %d cues, want the one stop: %+v", len(cues), cues)
	}
	c := cues[0]
	if math.Abs(c.s-10) > 1e-6 || math.Abs(c.e-12) > 1e-6 {
		t.Errorf("the still stands over [%.2f, %.2f] of the clip, want [10, 12]", c.s, c.e)
	}
	if math.Abs(c.fin-0.5) > 1e-6 || math.Abs(c.fout-0.5) > 1e-6 {
		t.Errorf("fades %.2f/%.2f, want 0.5/0.5", c.fin, c.fout)
	}

	// a bar hanging over the clip's start is already fully up when the clip
	// begins: the fade it keeps is the part still owing
	cues = freezeCues(fx, 100.25, 10, 1, 10)
	if len(cues) != 1 {
		t.Fatalf("got %d cues across the clip edge, want 1", len(cues))
	}
	if c := cues[0]; c.s != 0 || math.Abs(c.fin-0.25) > 1e-6 {
		t.Errorf("across the edge: s=%.2f fin=%.2f, want 0 and the owing 0.25", c.s, c.fin)
	}

	// a clip the bar misses entirely gets no cue
	if cues := freezeCues(fx, 200, 30, 1, 30); len(cues) != 0 {
		t.Errorf("a clip far from every stop got cues: %+v", cues)
	}
	// a held clip (span 0 -- a card, an audio insert's held frame) gets none:
	// there is no footage running under it for a still to stand over
	if cues := freezeCues(fx, 100, 0, 1, 5); len(cues) != 0 {
		t.Errorf("a held clip got a still: %+v", cues)
	}
}

// The clamp trims a stop's band to the kept footage, and its fades shrink
// with the band instead of the stop being handed a rate (clampSpeed's job is
// real slow motion, not a stop frame).
func TestClampTrimsAStopsFadesNotItsRate(t *testing.T) {
	fx := clampFxToSegs([]cutFx{
		{Kind: "speed", T: 100, Dur: 2, Rate: 0, Trans: 1, Tout: 1},
	}, []cutSeg{{S: 90, E: 101}}) // the band [100, 102] hangs over the cut's end
	if len(fx) != 1 {
		t.Fatalf("the stop was dropped: %+v", fx)
	}
	f := fx[0]
	if f.Rate != 0 {
		t.Errorf("the clamp handed the stop a rate: %v", f.Rate)
	}
	if math.Abs(f.Dur-1) > 1e-6 {
		t.Errorf("the band kept %.2f s, want the 1 s of footage under it", f.Dur)
	}
	// the band halved, so the fades halve with it: 1/1 over 2 s was a stop
	// that was all fade and no hold, and it stays one at half the length
	if math.Abs(f.Trans-0.5) > 1e-6 || math.Abs(f.Tout-0.5) > 1e-6 {
		t.Errorf("fades %.2f/%.2f over the halved band, want 0.5 each", f.Trans, f.Tout)
	}
	// untrimmed, the stop keeps exactly what was drawn
	fx = clampFxToSegs([]cutFx{
		{Kind: "speed", T: 100, Dur: 2, Rate: 0, Trans: 0.5, Tout: 0.5},
	}, []cutSeg{{S: 90, E: 110}})
	if len(fx) != 1 || fx[0].Dur != 2 || fx[0].Trans != 0.5 {
		t.Errorf("a stop already inside the cut was reshaped: %+v", fx)
	}
}

// The wiring runs on a live player and a real ffmpeg; pin it to the source.
func TestTheStopWiringIsInPlace(t *testing.T) {
	pins := map[string][]string{
		// every path that moves the playhead settles the still layer too, and
		// a card keeps owning the whole picture while it is up
		"cut_insview.go": {"defer ed.syncFxStill()", "if s == nil || s.audioIns() {"},
		// the still is position-triggered, rendered by ffmpeg from the
		// recording itself, faded on the widget's opacity, and its layer sits
		// in the preview's overlay stack on a Fixed of its own so the camera
		// moves over it (cut_stillcam_test.go) -- all of it on the shared
		// screen, so the Narrate preview freezes where the Cut one does
		"cut_fxscreen.go": {"freezeNow(s.fx(), at)", "ffmpegPNG(", "textAlpha(*f, at)",
			"s.fxStillBox.Put(s.fxStillPic, 0, 0)", "over.AddOverlay(s.fxStillBox)"},
		// the render composites the same frame over the same seconds: one
		// decoded frame cloned out over the clip, faded on its alpha, laid on
		// BEFORE the camera so a zoom crops the still like the footage
		"produce.go": {"freezeCues(fx, c.sessS", "trim=end_frame=1,setpts=PTS-STARTPTS",
			"fade=t=in:st=%.3f:d=%.3f:alpha=1", "overlay=x=0:y=0:eof_action=pass:enable=between"},
	}
	for file, want := range pins {
		b, err := os.ReadFile(file)
		if err != nil {
			t.Fatal(err)
		}
		for _, w := range want {
			if !strings.Contains(string(b), w) {
				t.Errorf("%s lost the stop wiring pinned by %q", file, w)
			}
		}
	}
}

func TestTheLaneDrawsWhatTheEffectDoes(t *testing.T) {
	src, err := os.ReadFile("cut_fx.go")
	if err != nil {
		t.Fatal(err)
	}
	s := string(src)
	// zoom, plain speed, text AND the stop all draw the same envelope: full
	// height where the effect fully holds, rising and falling over its fades
	// -- which is what makes two 3 s effects the same width whatever their
	// transitions
	if strings.Count(s, "laneBand(cr, x0, x1,") < 5 {
		t.Error("an effect kind stopped drawing its envelope through laneBand")
	}
	// the lead-in triangle belonged to the slow-down ramp, which is gone: a
	// stop's fades live INSIDE its bar now, like a text's
	if strings.Contains(s, "x0-f.Trans*ed.pps") {
		t.Error("the freeze grew its lead-in triangle back")
	}
	if strings.Contains(s, `"Slow-down seconds"`) {
		t.Error("the stop dialog grew its slow-down field back")
	}
	// the dialog offers the fades, and the label reports them. One dialog for
	// the whole kind now: a stop is a speed of x0, so there is exactly one
	// pair of fade rows and no stop-only branch to keep in step
	// -- and all four dialogs in this file name them the same, in the same
	// order, with the length between them: a zoom, a text, a speed and a
	// volume differ in what they do, not in what their transitions are called
	// the label asks for a length too, and for nothing else: it has no fades
	// to arrive or leave on (cut_fxlabel_test.go)
	for row, n0 := range map[string]int{"Fade in (s)": 4, "Length (s)": 5, "Fade out (s)": 4} {
		if n := strings.Count(s, `fxNumRow("`+row+`"`); n != n0 {
			t.Errorf("%d dialogs ask for %q, want %d", n, row, n0)
		}
	}
	for _, gone := range []string{"Ramp in seconds", "Ramp out seconds", "Glide in seconds",
		"Glide out seconds", "Seconds on screen", `fxNumRow("Seconds"`} {
		if strings.Contains(s, gone) {
			t.Errorf("a dialog names its rows %q again instead of the shared names", gone)
		}
	}
	got := cutFx{Kind: "speed", T: 724, Dur: 3, Rate: 0, Trans: 1, Tout: 0.5}.fxLabel()
	if !strings.Contains(got, "1.0s in") || !strings.Contains(got, "0.5s out") {
		t.Errorf("fxLabel = %q — a faded stop does not say so", got)
	}
	if got := (cutFx{Kind: "speed", T: 724, Dur: 3, Rate: 0}).fxLabel(); strings.Contains(got, "in") {
		t.Errorf("fxLabel = %q — an unfaded stop reports fades it does not have", got)
	}
}

// A stop is a speed of x0 and nothing else: one dropdown entry, one dialog,
// one bar on the lane. The entry works from a marked stretch or, with nothing
// marked, from the line -- which is how a stop is usually asked for.
func TestAStopIsJustASpeedOfZero(t *testing.T) {
	if !(cutFx{Kind: "speed", Rate: 0, Dur: 2}).frozenFx() {
		t.Error("x0 is no longer a stop")
	}
	if (cutFx{Kind: "speed", Rate: 0.5, Dur: 2}).frozenFx() {
		t.Error("half speed reads as a stop")
	}
	src, err := os.ReadFile("cut_fx.go")
	if err != nil {
		t.Fatal(err)
	}
	s := string(src)
	if strings.Contains(s, "func (a *App) freezeClicked()") {
		t.Error("the stop grew an entry point of its own back")
	}
	// the one entry falls back to the playhead when nothing is marked, and
	// what it places there is a stop
	for _, want := range []string{
		"t0, t1 = ed.playhead, ed.playhead+2",
		"f.Rate, f.Trans, f.Tout = 0, 0.5, 0.5",
	} {
		if !strings.Contains(s, want) {
			t.Errorf("speedClicked no longer contains %q", want)
		}
	}
	b, err := os.ReadFile("cut.go")
	if err != nil {
		t.Fatal(err)
	}
	if strings.Contains(string(b), "⏸ Stop") {
		t.Error("the dropdown grew its ⏸ Stop entry back")
	}
}

// The sound under a still is the stop's own choice. Left alone the footage --
// and its sound -- run on underneath, which is every cut written before this
// one; asked to, those seconds come out silent instead. Nothing else about the
// clip changes either way.
func TestAStopChoosesWhatItsSoundDoes(t *testing.T) {
	// the default is the old behaviour, and the JSON of a cut without the
	// choice is byte for byte the JSON it always was
	loud := cutFx{Kind: "speed", T: 100, Dur: 2, Rate: 0}
	if loud.Mute {
		t.Error("a stop is silent unless it was asked to be")
	}
	j, err := json.Marshal(loud)
	if err != nil {
		t.Fatal(err)
	}
	if strings.Contains(string(j), "mute") {
		t.Errorf("a stop that keeps its sound writes it into cut.json: %s", j)
	}
	var back cutFx
	if err := json.Unmarshal([]byte(`{"kind":"speed","t":100,"dur":2,"mute":true}`), &back); err != nil {
		t.Fatal(err)
	}
	if !back.Mute || !back.frozenFx() {
		t.Errorf("a silent stop did not survive the round trip: %+v", back)
	}
	// the label says so, and says nothing when there is nothing to say
	if got := (cutFx{Kind: "speed", T: 724, Dur: 3, Rate: 0, Mute: true}).fxLabel(); !strings.Contains(got, "silent") {
		t.Errorf("fxLabel = %q — a silent stop does not say so", got)
	}
	if got := loud.fxLabel(); strings.Contains(got, "silent") {
		t.Errorf("fxLabel = %q — a stop that keeps its sound claims to be silent", got)
	}

	// the render takes exactly the still's own seconds out, in the clip's own
	// output seconds, and leaves a stop that keeps its sound alone
	stills := []stillCue{
		{s: 4, e: 6, mute: true},
		{s: 10, e: 12},             // this one keeps its sound
		{s: 20, e: 20, mute: true}, // and a window with no width is not a window
	}
	if got := stillMute(stills); got != "between(t,4.000,6.000)" {
		t.Errorf("stillMute = %q, want just the silent stop's own seconds", got)
	}
	if got := stillMute([]stillCue{{s: 4, e: 6, mute: true}, {s: 10, e: 12, mute: true}}); got != "between(t,4.000,6.000)+between(t,10.000,12.000)" {
		t.Errorf("two silent stops = %q, want both windows ored together", got)
	}
	if got := stillMute([]stillCue{{s: 4, e: 6}}); got != "" {
		t.Errorf("a clip with nothing to silence built the filter anyway: %q", got)
	}

	// and the preview: the same seconds, muted the way a card is
	fx := []cutFx{{Kind: "speed", T: 100, Dur: 2, Rate: 0, Snd: sndMute}}
	for _, c := range []struct {
		t    float64
		want bool
	}{{99.9, false}, {100, true}, {101.9, true}, {102, false}} {
		if got := fxHush(fx, c.t); got != c.want {
			t.Errorf("at %gs the preview reads hush=%v, wanted %v", c.t, got, c.want)
		}
	}
	if fxHush([]cutFx{{Kind: "speed", T: 100, Dur: 2, Rate: 0}}, 101) {
		t.Error("a stop that keeps its sound muted the preview")
	}
	// a cut written before the answer was a word still reads: the old tick is
	// the silent answer (migrateFx does it once on load, and sound() reads it
	// either way)
	if !fxHush([]cutFx{{Kind: "speed", T: 100, Dur: 2, Rate: 0, Mute: true}}, 101) {
		t.Error("an older cut's silent stop is heard again")
	}
}

// The wiring for the choice, on the live player and the real ffmpeg.
func TestTheSilentStopWiringIsInPlace(t *testing.T) {
	pins := map[string][]string{
		// the dialog asks it of every rate now, as one of five answers about
		// the sound (cut_fxsound.go) -- the stop's tick was two of them --
		// and the line under it reads the rate on every way the rate can
		// change, which since the rate became a list with a typed box under
		// it is two ways (newRatePick)
		"cut_fx.go": {`sd := gtk.NewDropDownFromStrings(sndNames)`,
			"sd.SetSelected(sndIndex(f.sound()))", "rp := newRatePick(f.Rate, func() {",
			"f.Snd, f.Mute = sndKindOf(sd.Selected()), false",
			"msg := sndNote(sndKindOf(sd.Selected()), rp.rate(f.Rate), fxNumOf(l, f.Dur))"},
		// the render silences the window with a volume filter on the clip's
		// own sound, before the mix and after the atempo
		"produce.go": {`volume=0:enable='%s'[hush];`, "mute: cue.fx.sound() == sndMute"},
		// the preview mutes the session the way a card does
		"cut_insview.go": {"ed.player.SetMuted(cardHush(s) || fxHush(ed.fx, ed.playhead))"},
	}
	for file, want := range pins {
		b, err := os.ReadFile(file)
		if err != nil {
			t.Fatal(err)
		}
		for _, w := range want {
			if !strings.Contains(string(b), w) {
				t.Errorf("%s lost the silent-stop wiring pinned by %q", file, w)
			}
		}
	}
}

// A stop's bar can hang across a cut, and the frame it holds is the one the
// stop started on -- so on a session shot on two cameras the held frame and
// the footage it is laid over come from different recordings, at different
// sizes. The overlay has no size and no scaling in it, so a 1280x720 webcam
// frame over 3840x2160 gameplay sat in the top-left corner at a third of the
// size, with the moving picture running around it.
func TestAHeldFrameIsBroughtToTheSizeOfTheFootageItCovers(t *testing.T) {
	big := &tlVideo{base: "game", w: 3840, h: 2160}
	small := &tlVideo{base: "cam", w: 1280, h: 720}
	if w, h := stillSize(small, big); w != 3840 || h != 2160 {
		t.Errorf("a webcam frame over gameplay comes out %dx%d, want the gameplay's 3840x2160", w, h)
	}
	if w, h := stillSize(big, small); w != 1280 || h != 720 {
		t.Errorf("and the other way round: %dx%d, want 1280x720", w, h)
	}
	// the same recording, which is every single-camera session: nothing to do,
	// and saying so costs a scale pass on every stop in the render
	if w, h := stillSize(big, big); w != 0 || h != 0 {
		t.Errorf("a frame already the right size is rescaled to %dx%d", w, h)
	}
	// a size nobody probed is not a size to scale to -- a made-up one would
	// shrink the frame to nothing
	if w, h := stillSize(&tlVideo{w: 640}, big); w != 0 || h != 0 {
		t.Errorf("a half-probed recording scaled to %dx%d", w, h)
	}
	if w, h := stillSize(small, &tlVideo{}); w != 0 || h != 0 {
		t.Errorf("footage of unknown size took a still to %dx%d", w, h)
	}
	if w, h := stillSize(nil, big); w != 0 || h != 0 {
		t.Errorf("a still from nowhere sized to %dx%d", w, h)
	}
	// and the encode acts on it: fitted and centred, not stretched, on
	// transparency so the footage shows around a frame of another shape
	body := funcBody(t, "produce.go", `func \(a \*App\) encodeClip\(`)
	for _, want := range []string{
		"if sc.w > 0 && sc.h > 0 {",
		"scale=%d:%d:force_original_aspect_ratio=decrease,",
		"pad=%d:%d:(ow-iw)/2:(oh-ih)/2:color=#00000000",
	} {
		if !strings.Contains(body, want) {
			t.Errorf("the still overlay no longer does %q", want)
		}
	}
	if !strings.Contains(readSrc(t, "produce.go"), "sc.w, sc.h = stillSize(v, c.video)") {
		t.Error("the plan stopped sizing a held frame against the clip it lands on")
	}
}
