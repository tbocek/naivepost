package main

import (
	"strings"
	"testing"
)

// The line comes back where it was: written once a second while it moves,
// whatever the tick's ten calls say, and read back on the next reload of
// the same project -- once, so a reload that a run triggers mid-session
// does not put the line back where it was an hour ago.
func TestTheRedLineIsRememberedAcrossSessions(t *testing.T) {
	a := &App{outDir: t.TempDir()}
	ed := &cutEditor{a: a, pps: 4, thumbHt: 64, rowHov: -1, fxKillHov: -1, bandKillHov: -1, foldHov: -1}
	ed.vids = []tlVideo{{base: "v", path: "v.mkv", start: 0, dur: 600, fps: 30}}
	var fire func() bool
	armed := 0
	ed.lineArm = func(_ uint, fn func() bool) { armed++; fire = fn }

	// ten moves inside the second are one write, of the last position
	for _, at := range []float64{10, 11, 12, 13, 14, 15, 16, 17, 18, 19} {
		ed.setPlayhead(at)
	}
	if armed != 1 {
		t.Fatalf("ten moves armed %d writes, want 1", armed)
	}
	fire()
	// ...and the next move after the write arms the next one
	ed.setPlayhead(42.5)
	if armed != 2 {
		t.Fatalf("a move after the write armed nothing (%d writes armed)", armed)
	}
	fire()

	// a fresh editor on the same project opens on that second
	ed2 := &cutEditor{a: a, pps: 4, thumbHt: 64, rowHov: -1, fxKillHov: -1, bandKillHov: -1, foldHov: -1}
	ed2.vids = ed.vids
	ed2.restoreLine()
	if !ed2.hasPlay || ed2.playhead != 42.5 {
		t.Errorf("the line came back at %v (placed %v), want 42.5", ed2.playhead, ed2.hasPlay)
	}
	// moved since, a second reload of the same project leaves it be
	ed2.setPlayhead(7)
	ed2.restoreLine()
	if ed2.playhead != 7 {
		t.Errorf("a reload mid-session moved the line back to %v", ed2.playhead)
	}
	// a second no recording covers is not a place to put the line
	ed3 := &cutEditor{a: a, pps: 4, thumbHt: 64, rowHov: -1, fxKillHov: -1, bandKillHov: -1, foldHov: -1}
	ed3.vids = []tlVideo{{base: "v", path: "v.mkv", start: 0, dur: 30, fps: 30}}
	ed3.restoreLine()
	if ed3.hasPlay {
		t.Errorf("the line was put at a second past the recording's end")
	}

	// and the wiring: every hand on the line says so, the reload asks, and
	// closing the window does not lose the last second
	src := readSrc(t, "cut.go")
	if n := strings.Count(src, "ed.noteLine()"); n != 3 {
		t.Errorf("noteLine is called %d times in cut.go, want 3: setPlayhead, frameStep and the tick", n)
	}
	if !strings.Contains(funcBody(t, "cut.go", `func \(ed \*cutEditor\) reload\(\) error \{`), "ed.restoreLine()") {
		t.Error("reload does not restore the line")
	}
	if !strings.Contains(readSrc(t, "project.go"), "a.ed.flushLine()") {
		t.Error("closing the window does not flush the line")
	}
}
