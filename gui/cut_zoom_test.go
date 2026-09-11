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
	ed.pps = 120
	ed.layoutPx()
	was := ed.totalW
	ed.zoomAt(400, 2)
	if ed.pps != 120 || ed.totalW != was {
		t.Errorf("zooming in at the ceiling changed pps to %g and width to %g", ed.pps, ed.totalW)
	}
}
