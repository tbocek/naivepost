package main

import (
	"strings"
	"testing"
)

// The thumb is geared only when it needs to be: at a zoom where a pixel of
// drag is less than a fortieth of the view, the stock mapping stands and the
// thumb stays under the pointer; past that, a screenful is forty pixels of
// drag however far in the zoom goes.
func TestTheThumbIsGearedOnlyWhenZoomedIn(t *testing.T) {
	const view = 1600.0
	if g := scrollGear(3, view); g != 3 {
		t.Errorf("zoomed out, a thumb pixel moved %v timeline px, want the stock 3", g)
	}
	if g := scrollGear(200, view); g != view/gearPx {
		t.Errorf("zoomed in, a thumb pixel moved %v timeline px, want %v (a screenful per %v px)", g, view/gearPx, gearPx)
	}
	// and the bar has the gesture
	if !strings.Contains(readSrc(t, "cut.go"), "ed.gearScrollbar()") {
		t.Error("the scrollbar is not geared")
	}
}
