package main

import (
	"strings"
	"testing"
)

// The jumps the tick will make are known ahead of time, and only when they
// are near: a clip's end under ▶✂, a seam's window closing under a review
// when the next run-up is ahead rather than already under the line, and
// whichever of the two comes first.
func TestTheNextJumpIsKnownAhead(t *testing.T) {
	segs := []cutSeg{{S: 0, E: 60}, {S: 80, E: 140}, {S: 200, E: 260}}
	// the recording: nothing is coming
	if _, _, ok := jumpAhead(segs, 58, false, false, 0, 3); ok {
		t.Error("plain ▶ has no jump of the cut's own, and one was announced")
	}
	// the cut: the clip's end, once it is within the lead
	if _, _, ok := jumpAhead(segs, 50, true, false, 0, 3); ok {
		t.Error("a clip end 10 s away was announced with a 3 s lead")
	}
	if at, to, ok := jumpAhead(segs, 58, true, false, 0, 3); !ok || at != 60 || to != 80 {
		t.Errorf("2 s before the clip end the jump is (%v -> %v, %v), want 60 -> 80", at, to, ok)
	}
	// the last clip has nothing after it
	if _, _, ok := jumpAhead(segs, 259, true, false, 0, 3); ok {
		t.Error("the last clip's end was announced as a jump")
	}
	// the review: the window closes at 90 and the next run-up is 130, ahead
	if at, to, ok := jumpAhead(segs, 88, true, true, 0, 3); !ok || at != 90 || to != 130 {
		t.Errorf("2 s before seam 0's window closes the jump is (%v -> %v, %v), want 90 -> 130", at, to, ok)
	}
	// ...but a run-up already behind the line is played into, not sought
	short := []cutSeg{{S: 0, E: 60}, {S: 70, E: 75}, {S: 100, E: 160}}
	if at, to, ok := jumpAhead(short, 74, true, true, 0, 3); !ok || at != 75 || to != 100 {
		t.Errorf("at the short clip's end the jump is (%v -> %v, %v), want the cut's own 75 -> 100", at, to, ok)
	}
	// and the wiring: the tick asks, and a seek prefers the swap
	tick := funcBody(t, "cut.go", `func \(ed \*cutEditor\) followPlayback\(\) bool \{`)
	if !strings.Contains(tick, "ed.preloadAhead()") {
		t.Error("the tick does not preload the next jump")
	}
	if !strings.Contains(funcBody(t, "cut.go", `func \(ed \*cutEditor\) setPlayhead\(t float64\) \{`), "ed.player.Preloaded(v.path, v.at(t))") {
		t.Error("a seek within the file does not prefer the prerolled spare")
	}
}
