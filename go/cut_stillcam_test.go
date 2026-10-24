package main

// The camera moves over a stop's frozen frame.
//
// "Here it should zoom to the lower right, but it does not. Zoom should work
// even though there is a screen pause." -- and a screen pause is exactly that:
// the VIDEO stops, and everything else on the session clock keeps going. The
// title still comes up over it, the camera still moves across it.
//
// The render always did this. encodeClip splices the stop's stills into the
// filter chain BEFORE the camera filters, in the clip's own time but in the
// SOURCE frame, so a zoom crops the frozen frame exactly as it crops the
// footage running under it. The PREVIEW did not: the still hung straight on
// the GtkOverlay, a sibling of the transformed layer rather than a passenger
// on it, so it was the one thing on the picture the camera could not reach.
// The layer under it zoomed and the frame being looked at did not.

import (
	"math"
	"os"
	"strings"
	"testing"
)

// stopAndZoom is the case from the report: a stop, and a camera move to the
// lower right over exactly the same seconds.
func stopAndZoom() []cutFx {
	return []cutFx{
		{Kind: "speed", T: 10, Dur: 3, Rate: 0},
		{Kind: "zoom", T: 10, Dur: 3, Cx: 0.75, Cy: 0.75, Hf: 0.5},
	}
}

// The invariant, in one line: the frozen frame is on the same mapping as the
// footage beneath it. Anything else and the two layers disagree about where
// the picture is, which is what a still that will not zoom looks like.
func TestTheStillIsOnTheSameMappingAsTheFootageUnderIt(t *testing.T) {
	const W, H, sw, sh = 960, 540, 1920, 1080
	fx := stopAndZoom()
	outA := 16.0 / 9
	// mid-band, past the glide in: the camera is where it was sent
	r := fxRectAt(fx, 12.5, sw/sh, outA)
	ws, wtx, wty := liveZoom(W, H, sw, sh, outA, r)
	s, tx, ty := stillFit(W, H, sw, sh, outA, true, r)
	if s != ws || tx != wtx || ty != wty {
		t.Errorf("the still is on %g/%g/%g and the footage under it on %g/%g/%g",
			s, tx, ty, ws, wtx, wty)
	}
}

// And what that mapping actually does, said without reference to liveZoom: the
// point the camera is aimed at lands in the middle of the output box. Left on
// the overlay, as it was, the still's centre landed there instead -- the whole
// frame, letterboxed, no matter where the camera was pointing.
func TestAStopsFrameIsCroppedWhereTheCameraIsAimed(t *testing.T) {
	const W, H, sw, sh = 960, 540, 1920, 1080
	outA := 16.0 / 9
	r := fxRectAt(stopAndZoom(), 12.5, sw/sh, outA)
	if r.cx < 0.6 || r.cy < 0.6 {
		t.Fatalf("the camera is at %.2f,%.2f, want it down and to the right", r.cx, r.cy)
	}
	s, tx, ty := stillFit(W, H, sw, sh, outA, true, r)
	bx, by, bw, bh := fxDisp(W, H, outA)
	// where the aimed-at source pixel comes out on the widget
	gx, gy := tx+r.cx*sw*s, ty+r.cy*sh*s
	if math.Abs(gx-(bx+bw/2)) > 1e-6 || math.Abs(gy-(by+bh/2)) > 1e-6 {
		t.Errorf("the camera's point lands at %.1f,%.1f on the widget, want the middle "+
			"of the output box at %.1f,%.1f", gx, gy, bx+bw/2, by+bh/2)
	}
	// and the middle of the frame does NOT, because that is the failure: a
	// still that ignores the camera shows its own centre in the middle
	cx, cy := tx+0.5*sw*s, ty+0.5*sh*s
	if math.Abs(cx-(bx+bw/2)) < 1 && math.Abs(cy-(by+bh/2)) < 1 {
		t.Error("the middle of the frozen frame is still in the middle of the box — " +
			"the camera did not move over it")
	}
}

// With no camera on the session the still sits exactly where the raw picture
// sits: contain-fit, centred, which is what GtkPicture does with the video
// underneath and therefore the only place the still may be.
func TestAStopWithNoCameraSitsWhereThePictureDoes(t *testing.T) {
	for _, c := range []struct{ W, H, sw, sh float64 }{
		{960, 540, 1920, 1080}, // the same shape: it fills
		{960, 540, 1080, 1920}, // a tall source in a wide widget
		{400, 900, 1920, 1080}, // a wide source in a tall widget
	} {
		s, tx, ty := stillFit(c.W, c.H, c.sw, c.sh, 16.0/9, false, fxRect{cx: 0.5, cy: 0.5, hf: 1})
		if want := math.Min(c.W/c.sw, c.H/c.sh); s != want {
			t.Errorf("%gx%g source in a %gx%g widget scales by %g, want the contain fit %g",
				c.sw, c.sh, c.W, c.H, s, want)
		}
		// centred, and inside the widget on both axes
		if math.Abs((tx+c.sw*s/2)-c.W/2) > 1e-9 || math.Abs((ty+c.sh*s/2)-c.H/2) > 1e-9 {
			t.Errorf("%gx%g source in a %gx%g widget is centred at %.1f,%.1f, want %.1f,%.1f",
				c.sw, c.sh, c.W, c.H, tx+c.sw*s/2, ty+c.sh*s/2, c.W/2, c.H/2)
		}
		if tx < -1e-9 || ty < -1e-9 {
			t.Errorf("%gx%g source in a %gx%g widget starts at %.1f,%.1f, outside it",
				c.sw, c.sh, c.W, c.H, tx, ty)
		}
	}
}

// A stop with no camera anywhere near it must not be moved by one that is: the
// mapping is read at the playhead, so a zoom later in the session leaves the
// frozen frame alone (camRectAt breaks on the first zoom past t).
func TestAZoomLaterInTheSessionDoesNotMoveAnEarlierStop(t *testing.T) {
	const W, H, sw, sh = 960, 540, 1920, 1080
	outA := 16.0 / 9
	fx := []cutFx{
		{Kind: "speed", T: 10, Dur: 3, Rate: 0},
		{Kind: "zoom", T: 40, Dur: 3, Cx: 0.75, Cy: 0.75, Hf: 0.5},
	}
	s, tx, ty := stillFit(W, H, sw, sh, outA, true, fxRectAt(fx, 11, sw/sh, outA))
	fs, ftx, fty := stillFit(W, H, sw, sh, outA, true, fullFill(sw/sh, outA))
	if s != fs || tx != ftx || ty != fty {
		t.Errorf("a stop at 11 s is framed %g/%g/%g by a zoom that starts at 40 s, want "+
			"the full frame %g/%g/%g", s, tx, ty, fs, ftx, fty)
	}
}

// The wiring: the still rides its own Fixed, which is what a transform can be
// hung on, and every path that settles the camera settles the still with it.
func TestTheStillRidesTheCameraTransform(t *testing.T) {
	pins := map[string][]string{
		"cut_fxscreen.go": {
			"s.fxStillBox = gtk.NewFixed()",
			"s.fxStillBox.SetOverflow(gtk.OverflowHidden)",
			"s.fxStillBox.Put(s.fxStillPic, 0, 0)",
			"over.AddOverlay(s.fxStillBox)",
			"s.syncCamLayer()",
			"s.fitStill() // whatever the camera just did, the still does too",
			"box.SetChildTransform(pic, zoomTransform(sc, tx, ty))",
			"s.fitStill() // under whatever camera is over the footage right now",
			"box.SetOpacity(textAlpha(*f, at))",
		},
		// and the render, which has been doing this all along -- the comment is
		// the contract the preview now keeps
		"produce.go": {
			"// the stop stills go on between here and the camera:",
		},
	}
	for file, wants := range pins {
		b, err := os.ReadFile(file)
		if err != nil {
			t.Fatal(err)
		}
		for _, want := range wants {
			if !strings.Contains(string(b), want) {
				t.Errorf("%s does not contain %q", file, want)
			}
		}
	}
	// nothing hangs the still straight on the overlay any more
	b, err := os.ReadFile("cut_fxscreen.go")
	if err != nil {
		t.Fatal(err)
	}
	if strings.Contains(string(b), "over.AddOverlay(s.fxStillPic)") {
		t.Error("the still is still an overlay child — the camera cannot reach it there")
	}
}
