package main

// The wheel as a transport, and the view's duty to the red line. Stepping and
// playback move the line without moving the view, so either could walk it
// silently off the page -- and a transport whose subject is off screen is a
// transport you operate blind. What these pin is the one rule for bringing it
// back (centered, and only when actually out of view) and that every mover --
// the wheel over the bar, the wheel over the picture, the frame buttons,
// playback -- goes through it.

import (
	"os"
	"strings"
	"testing"
)

func TestTheViewComesBackToTheRedLine(t *testing.T) {
	// on screen: the view stays put -- a view that creeps on every step
	// cannot be read
	for _, x := range []float64{100, 250, 400} {
		if off, out := revealOff(x, 100, 300); out || off != 100 {
			t.Errorf("revealOff(%g, 100, 300) = (%g, %v), want (100, false): the line is visible", x, off, out)
		}
	}
	// off either side: centered, not nudged just inside the edge -- a line
	// brought back to the very edge is one more step from leaving again
	if off, out := revealOff(500, 100, 300); !out || off != 350 {
		t.Errorf("line off the right: revealOff = (%g, %v), want (350, true)", off, out)
	}
	if off, out := revealOff(50, 100, 300); !out || off != -100 {
		t.Errorf("line off the left: revealOff = (%g, %v), want (-100, true) -- setOff clamps to 0", off, out)
	}
}

func TestEveryTransportShowsTheLine(t *testing.T) {
	b, err := os.ReadFile("cut.go")
	if err != nil {
		t.Fatal(err)
	}
	src := string(b)
	for _, want := range []string{
		// the frame buttons and the wheel both land in frameStep; playback in
		// followPlayback -- each brings the line back on screen itself
		"ed.revealPlayhead() // a step must never move the line somewhere you cannot see",
		"ed.revealPlayhead() // playback runs the line off the view; recenter and follow",
		// the wheel over the toolbar is the transport too
		"bar.AddController(ed.wheelFrames())",
		// a notch is a frame, five with Shift, back with the wheel's other way
		// -- the arrow keys' spelling
		"sc.CurrentEventState()&gdk.ShiftMask != 0",
	} {
		if !strings.Contains(src, want) {
			t.Errorf("cut.go no longer contains %q -- the wheel transport came unwired", want)
		}
	}

	// and the picture answers the same gesture: the preview is where you look
	// while hunting for a frame
	b, err = os.ReadFile("cut_fxview.go")
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(b), "over.AddController(ed.wheelFrames())") {
		t.Error("cut_fxview.go no longer scrolls frames over the picture")
	}
}
