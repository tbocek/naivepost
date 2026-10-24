package main

// The preview's camera clock.
//
// Everything on the cut page is driven by one 100ms timer, which is the right
// rate for a red line and the wrong rate for a glide: ten samples a second is
// a visibly stepped transition. The render has no such problem -- zoompan
// evaluates the same path per frame -- so the choppiness was the preview's
// alone, and it made a transition look worse than it would be. livePlayhead
// and a frame-clock callback close that gap.

import (
	"math"
	"os"
	"strings"
	"testing"
	"time"
)

func TestTheCameraRunsOnTheDisplaysClock(t *testing.T) {
	ed := &cutEditor{playhead: 20}
	// nothing to extrapolate from: the playhead is the answer
	if got := ed.livePlayhead(); got != 20 {
		t.Errorf("with no player the live clock reads %v, want the playhead 20", got)
	}
	ed.player = &Player{}
	if got := ed.livePlayhead(); got != 20 {
		t.Errorf("paused, the live clock reads %v, want the playhead 20", got)
	}
	ed.player.playing = true
	if got := ed.livePlayhead(); got != 20 {
		t.Errorf("before the first position is read the live clock reads %v, want 20", got)
	}

	// a position read 30ms ago is 30ms stale, and the camera is shown where
	// the picture is rather than where it was
	ed.posT, ed.posAt = 20, time.Now().Add(-30*time.Millisecond)
	got := ed.livePlayhead()
	if got < 20.03 || got > 20+float64(playTick)/1000 {
		t.Errorf("30ms after a read the live clock is %.4f, want a little past 20.03", got)
	}

	// capped at one tick: a stall stops the picture instead of sailing the
	// camera on and snapping it back when the real position lands
	ed.posAt = time.Now().Add(-5 * time.Second)
	if got, want := ed.livePlayhead(), 20+float64(playTick)/1000; math.Abs(got-want) > 1e-9 {
		t.Errorf("after a five second stall the live clock is %.4f, want it capped at %.4f", got, want)
	}

	// and it advances at the rate the FOOTAGE is playing at, not the wall's:
	// under a ×0.5 speed effect half a tick of session time passes per tick.
	// A rate change rides a seek, and a seek drops playback until the new
	// position prerolls -- which re-arms the no-backward clamp; mirrored
	// here, or the clamp would rightly hold the ×1 answer just given
	ed.player.playing = false
	ed.livePlayhead()
	ed.player.playing = true
	ed.player.seekRate = 0.5
	if got, want := ed.livePlayhead(), 20+float64(playTick)/2000; math.Abs(got-want) > 1e-9 {
		t.Errorf("at ×0.5 the live clock is %.4f, want %.4f", got, want)
	}
}

// The live clock never runs backward while the stream plays. At ×4 the
// pipeline can fall behind the extrapolation -- four seconds of footage
// decoded every second is real work -- and each fresh position read then
// lands BEHIND the value already handed out. Passed on raw, that sawtooth
// reached the titles ten times a second: textsAt flicking a text on and off
// across its edges, textAlpha jittering its fade, the camera juddering
// mid-glide. The clamp holds the last answer until the real clock passes it.
func TestTheLiveClockNeverRunsBackward(t *testing.T) {
	ed := &cutEditor{playhead: 20}
	ed.player = &Player{playing: true, seekRate: 4}
	ed.posT, ed.posAt = 20, time.Now().Add(-50*time.Millisecond)
	v1 := ed.livePlayhead()
	if v1 <= 20.1 {
		t.Fatalf("50ms after a read at ×4 the live clock is %.4f, want well past 20", v1)
	}
	// the next read lands behind the extrapolation, as a lagging pipeline's do
	ed.posT, ed.posAt = 20.01, time.Now()
	if got := ed.livePlayhead(); got < v1 {
		t.Errorf("a stale read pulled the live clock back from %.4f to %.4f", v1, got)
	}
	// but it is a clamp, not a ratchet on the world: once the real clock is
	// past the held answer, the real clock is the answer again
	ed.posT, ed.posAt = 25, time.Now()
	if got := ed.livePlayhead(); got < 25 {
		t.Errorf("the stream reached 25 and the live clock still reads %.4f", got)
	}
	// pausing re-arms it, so a line put back to an earlier second is not held
	// at the old high-water mark when playback resumes there
	ed.player.playing = false
	ed.playhead = 10
	if got := ed.livePlayhead(); got != 10 {
		t.Errorf("paused at 10 the live clock reads %.4f", got)
	}
	ed.player.playing = true
	ed.posT, ed.posAt = 10, time.Now()
	if got := ed.livePlayhead(); got >= 25 || got < 10 {
		t.Errorf("re-based at 10 the live clock reads %.4f, want a little past 10", got)
	}
}

// The clamp's other re-basing: the line placed by hand. A deliberate jump
// backward -- a click on the timeline, a gap skipped to a clip's start --
// must move the live clock with it, or the camera and the titles would stand
// at the old high-water mark until the stream caught up.
func TestTheClampIsReBasedWhereTheLineIsPlaced(t *testing.T) {
	b, err := os.ReadFile("cut.go")
	if err != nil {
		t.Fatal(err)
	}
	src := string(b)
	for _, want := range []string{
		"ed.reLive(t) // the live clock is re-based with the line; see livePlayhead",
		"return playhead, playhead // nothing to smooth; re-arm on the line itself",
	} {
		if !strings.Contains(src, want) {
			t.Errorf("cut.go no longer contains %q", want)
		}
	}
	// the re-basing itself is on the shared screen, so the Narrate preview is
	// re-based by the same line
	if want := "s.liveMax, s.posT, s.posAt = t, t, time.Now()"; !strings.Contains(readSrc(t, "cut_fxscreen.go"), want) {
		t.Errorf("cut_fxscreen.go no longer contains %q", want)
	}
}

// The wiring: the frame clock asks, and what it asks about is the live clock.
func TestTheGlideIsDrivenByTheFrameClock(t *testing.T) {
	for file, wants := range map[string][]string{
		"cut_fxview.go": {
			"area.AddTickCallback(func(_ gtk.Widgetter, _ gdk.FrameClocker) bool {",
			"if ed.livePreview() && ed.player != nil && ed.player.playing {",
			"ed.syncPreviewZoom()",
			"area.QueueDraw()",
		},
		// both halves of the playing picture are on the shared screen: the
		// transformed layer, and the mask and titles drawn over it, each
		// asking the live clock rather than the ten-a-second playhead
		"cut_fxscreen.go": {
			"fxLiveFit(W, H, sw, sh, s.outAspect(), s.fx(), s.livePlayhead())",
			"now := s.livePlayhead()",
			"fxLiveFit(W, H, lw, lh, outA, fx, now)",
			"s.ed().drawFxOverlaysAt(cr, fx, now, ox, oy, ow, oh)",
		},
		"cut_fxdraw.go": {
			// and the shared side asks about the clock it was handed
			"liveZoom(W, H, sw, sh, outA, fxRectAt(fx, t, sw/sh, outA))",
			"for _, i := range textsAt(fx, t) {",
		},
		"cut.go": {
			"ed.posT, ed.posAt = ed.playhead, time.Now()",
			"glib.TimeoutAdd(playTick, ed.followPlayback)",
		},
	} {
		b, err := os.ReadFile(file)
		if err != nil {
			t.Fatal(err)
		}
		for _, want := range wants {
			if !strings.Contains(string(b), want) {
				t.Errorf("%s no longer contains %q", file, want)
			}
		}
	}
}
