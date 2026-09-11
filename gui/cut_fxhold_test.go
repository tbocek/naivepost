package main

// A held effect and the red line have to be in the same place.
//
// Holding a zoom puts the whole frame on the preview with the zoom's box drawn
// on it -- the aiming picture. Nothing about that picture is true once the line
// has moved somewhere else: the box is one moment's framing over another
// moment's frame, and the framed preview stays switched off, so the framing
// really in force at the line is the one thing NOT shown. Reported as "I set
// the first framing to the middle, the second to the right, and at the start it
// keeps the last effect".

import (
	"math"
	"os"
	"strings"
	"testing"
)

// The report, as a test. Two framings: the middle one at 0:10, the right one at
// 0:48 with a second of fade. Pick up the second, then put the line back at
// 0:14.9 -- and the second must be out of your hands, because the framing at
// 0:14.9 belongs to the first.
func TestTheLineLeavingAnEffectPutsItDown(t *testing.T) {
	mid := cutFx{Kind: "zoom", T: 10, Stay: true, Cx: 0.5, Cy: 0.5, Hf: 1}
	right := cutFx{Kind: "zoom", T: 48, Trans: 1, Dur: 1, Stay: true, Cx: 0.82, Cy: 0.5, Hf: 1}
	ed := &cutEditor{fx: []cutFx{mid, right}, fxOn: true, fxSel: 1}

	ed.playhead = 48 // where holdFx put it
	if ed.fxHoldLost() {
		t.Error("the zoom was let go while the line was standing on it")
	}
	ed.playhead = 14.9 // and where the click put it
	if !ed.fxHoldLost() {
		t.Error("the line moved to 0:14.9 and the zoom at 0:48 is still held — " +
			"its box will be drawn over the first framing's footage")
	}

	// and what the picture shows there once it is let go: the FIRST framing,
	// which is the whole point of letting go
	r := fxRectAt(ed.fx, 14.9, 16.0/9, 9.0/16)
	if math.Abs(r.cx-0.5) > 1e-9 {
		t.Errorf("at 0:14.9 the camera is at cx=%.3f, want the first framing's 0.5", r.cx)
	}
	if r2 := fxRectAt(ed.fx, 49, 16.0/9, 9.0/16); math.Abs(r2.cx-0.82) > 1e-9 {
		t.Errorf("at 0:49 the camera is at cx=%.3f, want the second framing's 0.82", r2.cx)
	}
}

// The line is allowed to stand anywhere in the effect's own span -- a zoom
// fading in, holding and coming back out owns those seconds, and scrubbing
// across them to watch the move is not walking away from it. Nor is a seek
// that lands a frame short of where it was aimed.
func TestAHoldSurvivesItsOwnSpan(t *testing.T) {
	// a 4 s band from 0:10 -- 1 s in, 2 s held, 1 s back out: it owns 10..14
	v := cutFx{Kind: "zoom", T: 10, Trans: 1, Dur: 4, Tout: 1, Cx: 0.5, Cy: 0.5, Hf: 1}
	ed := &cutEditor{fx: []cutFx{v}, fxOn: true, fxSel: 0}
	for _, at := range []float64{10, 10.5, 12, 13.9, 14, 10 - fxHoldSlack/2, 14 + fxHoldSlack/2} {
		ed.playhead = at
		if ed.fxHoldLost() {
			t.Errorf("the zoom was let go at %.3fs, inside its own 10–14s", at)
		}
	}
	for _, at := range []float64{0, 9, 15, 60} {
		ed.playhead = at
		if !ed.fxHoldLost() {
			t.Errorf("the zoom is still held at %.1fs, well clear of its 10–14s", at)
		}
	}
	// a point effect is a point: standing anywhere else is standing off it
	ed.fx = []cutFx{{Kind: "zoom", T: 10, Stay: true, Cx: 0.5, Cy: 0.5, Hf: 1}}
	for _, c := range []struct {
		at   float64
		lost bool
	}{{10, false}, {10.02, false}, {11, true}, {9, true}} {
		ed.playhead = c.at
		if ed.fxHoldLost() != c.lost {
			t.Errorf("a hard reframing at 0:10 with the line at %.2fs: lost=%v, want %v",
				c.at, ed.fxHoldLost(), c.lost)
		}
	}
}

// Dragging the marker along the lane moves the effect AND the line, but the
// line is carried by a throttled call and lags the effect between scrubs. If
// that lag counted as walking away, the effect would be let go mid-drag and
// the drag would die under the pointer.
func TestADraggedEffectIsNotAnAbandonedOne(t *testing.T) {
	ed := &cutEditor{
		fx:    []cutFx{{Kind: "zoom", T: 30, Stay: true, Cx: 0.5, Cy: 0.5, Hf: 1}},
		fxOn:  true,
		fxSel: 0,
	}
	ed.playhead = 10 // the line, still back where the last scrub left it
	if !ed.fxHoldLost() {
		t.Fatal("the guard is not being asked at all — the rest of this proves nothing")
	}
	ed.fxMoving = true
	if ed.fxHoldLost() {
		t.Error("the effect was let go while it was being dragged")
	}
}

// The wiring: both paths that move the line ask, and picking up a clip or an
// edge puts an effect down the way picking up an effect already puts them down.
func TestTheHoldFollowsTheLine(t *testing.T) {
	b, err := os.ReadFile("cut.go")
	if err != nil {
		t.Fatal(err)
	}
	src := string(b)
	// pinned without their trailing comments: gofmt aligns those with whatever
	// line lands next to them, so a comment is a moving target and the code is
	// not
	for _, want := range []string{
		"ed.cancelHold()\n\ted.syncFxHold()",
		"ed.segOn, ed.fxOn = false, false",
		"ed.edgeOn, ed.fxOn = false, false",
		// and the drag flag is the editor's, not the closure's, or syncFxHold
		// could not see it
		"ed.fxMoving = true",
		"if ed.fxMoving {",
	} {
		if !strings.Contains(src, want) {
			t.Errorf("cut.go no longer contains %q", want)
		}
	}
	// the two paths that move the line: a seek and playback's own clock
	if n := strings.Count(src, "ed.syncFxHold()"); n != 2 {
		t.Errorf("the line is checked against the held effect in %d places, want setPlayhead and followPlayback", n)
	}
}
