package main

import (
	"strings"
	"testing"
)

// Narrate and Produce used to wait for a cut to exist before their tabs could
// be opened. That is true of RUNNING them and not of opening them: the
// resolution, the container, which languages the subtitles are translated into
// and whether there is a narration at all are answers you give BEFORE the run,
// and a tab that cannot be opened is a setting that cannot be reached until
// the thing it governs has already happened.
func TestOnlyTheCutWaitsForItsInput(t *testing.T) {
	body := funcBody(t, "main.go", `func \(a \*App\) updateGates\(`)
	if !strings.Contains(body, "a.cutLocked = !a.canCut()") {
		t.Error("the cut no longer waits for a session to cut")
	}
	if !strings.Contains(body, "a.narrateLocked, a.produceLocked = false, false") {
		t.Error("Narrate or Produce is still locked until a cut exists")
	}
	// ...and both refuse on their own, which is the honest place to say it
	for _, c := range []struct{ file, fn string }{
		{"narrate.go", `func \(a \*App\) narrateRun\(`},
		{"produce.go", `func \(a \*App\) produceRun\(`},
	} {
		if !strings.Contains(funcBody(t, c.file, c.fn), "no cut yet") {
			t.Errorf("%s runs with no cut instead of saying so", c.file)
		}
	}
}
