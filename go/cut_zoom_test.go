package main

// Zooming with the wheel, and why it was laggy.
//
// A repaint of the whole timeline is a few milliseconds. What made the wheel
// lag was how many times one notch asked for it: a touchpad delivers a notch
// as a run of fractional deltas, and every delta was a whole zoom -- a
// relayout, five writes to the scrollbar's adjustment that could each redraw,
// a redraw of four areas, and a re-sync of the preview widget's camera layer,
// which is a size request and a transform on the largest widget on the page.
// These hold the three things that fixed it: the deltas are banked and applied
// once per frame, the adjustment is quiet while it is being set, and a pan or
// a zoom repaints the tracks without touching the preview.

import (
	"strings"
	"testing"
)

func TestAWheelGestureIsOneZoom(t *testing.T) {
	src := readSrc(t, "cut.go")
	for _, want := range []string{
		"ed.zoomWheel(dy)",                         // the controller banks, it does not zoom
		"ed.zoomPend += dy",                        // ...into one pending factor
		"ed.zoomBook = true",                       // one idle booked, however many deltas
		"ed.zoomAt(ed.lastX, math.Pow(1.25, -dy))", // applied once
	} {
		if !strings.Contains(src, want) {
			t.Errorf("cut.go no longer contains %q", want)
		}
	}
	if strings.Contains(src, "ed.zoomAt(ed.lastX, math.Pow(1.25, -dy))\n\t\t\t}\n\t\t\treturn true") {
		t.Error("the scroll controller zooms on every delta again")
	}
}

// The zoom lays the pixels out and draws once. relayout is for a change of
// what is on the timeline and re-syncs the preview's camera layer; a zoom
// changes where things are drawn and nothing the preview shows.
func TestAZoomDrawsOnceAndLeavesThePreviewAlone(t *testing.T) {
	src := readSrc(t, "cut.go")
	i := strings.Index(src, "func (ed *cutEditor) zoomAt(")
	if i < 0 {
		t.Fatal("zoomAt is gone")
	}
	body := src[i : strings.Index(src[i:], "\n}\n")+i]
	for _, want := range []string{"ed.layoutPx()", "ed.syncScroll()", "ed.setOff(ed.xOf(t) - viewX)",
		// ...and draws itself. The adjustment only fires value-changed when
		// its value actually MOVES, and a zoom anchored with the view already
		// hard against either end of the timeline clamps to the offset it
		// already had: every pixel underneath had changed and nothing
		// repainted them, so the wheel did nothing until the pointer moved and
		// the hover queued a draw of its own.
		"ed.queueTracks()"} {
		if !strings.Contains(body, want) {
			t.Errorf("zoomAt no longer does %q", want)
		}
	}
	if strings.Contains(body, "ed.relayout()") {
		t.Error("zoomAt goes through relayout, which re-syncs the preview widget for a wheel notch")
	}
	// the adjustment's handler: quiet while syncScroll writes it, and a pan
	// queues the tracks without the preview sync
	for _, want := range []string{
		"if ed.scrollMut {",
		"ed.queueTracks()\n\t})",
		"ed.scrollMut = true\n\ted.hadj.SetUpper(ed.totalW)",
	} {
		if !strings.Contains(src, want) {
			t.Errorf("cut.go no longer contains %q", want)
		}
	}
	// and redrawTracks is still the one an EDIT uses: draws plus the syncs
	j := strings.Index(src, "func (ed *cutEditor) redrawTracks() {")
	rb := src[j : strings.Index(src[j:], "\n}\n")+j]
	for _, want := range []string{"ed.queueTracks()", "ed.syncPreviewZoom()"} {
		if !strings.Contains(rb, want) {
			t.Errorf("redrawTracks no longer does %q", want)
		}
	}
	// zooming against the stop does nothing at all, rather than laying out and
	// drawing a timeline that did not change
	ed := newTestEd(t)
	ed.vids = []tlVideo{{base: "a", path: "a.mkv", start: 0, dur: 60, interval: 5, fps: 30}}
	ed.viewW = 800
	ed.relayout()
	ed.pps = maxPps
	ed.layoutPx()
	was := ed.totalW
	ed.zoomAt(400, 2)
	if ed.pps != maxPps || ed.totalW != was {
		t.Errorf("zooming in at the ceiling changed pps to %g and width to %g", ed.pps, ed.totalW)
	}
}

// How far in the zoom goes, and what has to keep up with it.
//
// Two things are drawn from something coarser than pixels at the top zoom: the
// waveform, off an envelope at waveHz, and the ruler, off tickStep. Raising the
// ceiling without raising those turns the wave into stair-steps of one bucket
// each and leaves the ruler with four marks across the window -- a zoom that
// shows more pixels and no more information.
func TestTheTopZoomHasSomethingToDrawAtIt(t *testing.T) {
	// at least one envelope bucket per pixel, or the wave is drawn in blocks
	if waveHz < maxPps/2 {
		t.Errorf("the envelope is %g Hz against a %g px/s ceiling: %.1f px a bucket",
			waveHz, maxPps, maxPps/waveHz)
	}
	// the decode has to divide into whole samples per bucket
	if waveRate%int(waveHz) != 0 {
		t.Errorf("%d Hz audio does not divide into %g buckets a second", waveRate, waveHz)
	}
	// and the ruler goes below a second, because that is what the zoom is for
	if step := tickStep(maxPps); step >= 1 {
		t.Errorf("at the top zoom the ruler still steps %gs -- %.0f marks across an 800 px window",
			step, 800/(step*maxPps))
	}
}

// A ruler mark below a second says which fraction it is. Two marks 400 ms apart
// both reading 0:37 are worse than no marks: the eye reads them as the same
// instant drawn twice.
func TestSubSecondRulerMarksSayWhichFraction(t *testing.T) {
	for _, c := range []struct {
		t, step float64
		want    string
	}{
		{37, 1, "0:37"},
		{97, 5, "1:37"},
		{37.4, 0.2, "0:37.4"},
		{37.5, 0.5, "0:37.5"},
		{125.5, 0.5, "2:05.5"},
		{60, 0.5, "1:00.0"},
	} {
		if got := tickLabel(c.t, c.step); got != c.want {
			t.Errorf("%.1f s on a %gs step reads %q, want %q", c.t, c.step, got, c.want)
		}
	}
}
