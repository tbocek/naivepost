package main

// The Cut page is the one page that is built empty and filled from somewhere
// else. buildCut makes the tracks; only updateCutInfo puts recordings on
// them, and it was called from exactly two places -- the launch, and Rescan.
//
// So: run Describe on a fresh session, watch the Cut tab unlock, open it, and
// find the empty box the page had been built as. Restart and it was all there.
// Nothing about that looks like a page that has not looked; it looks like a run
// that did not work.
//
// What these pin is the plumbing that fixes it, all of it source-level, since
// the symptom lives in a widget: that the things which change what the tracks
// are drawn from say so, and that saying so is what the page acts on.

import (
	"os"
	"strings"
	"testing"
)

func staleEd(t *testing.T) *App {
	t.Helper()
	a := &App{outDir: t.TempDir()}
	a.ed = &cutEditor{a: a, pps: 4, thumbHt: 64}
	return a
}

// refreshCut is a note, not a rebuild: the page is usually not the one on
// screen when a run finishes, and probing every recording for a page nobody is
// looking at is work done to be thrown away.
func TestARunLeavesTheCutPageANoteThatItIsBehind(t *testing.T) {
	a := staleEd(t)
	if a.ed.stale {
		t.Fatal("a freshly built editor claims to be behind")
	}
	a.refreshCut()
	if !a.ed.stale {
		t.Error("refreshCut did not mark the tracks stale -- the page will go on showing the old session")
	}
	if a.ed.pending {
		t.Error("a rebuild was queued for a page that is not on screen")
	}

	// and a nil editor is the launch, before the page exists
	b := &App{outDir: t.TempDir()}
	b.refreshCut() // must not panic
}

// The note is cleared by the only thing that acts on it, so that a page which
// has caught up is not rebuilt again on the next tab click.
func TestCatchingUpClearsTheNote(t *testing.T) {
	a := staleEd(t)
	a.ed.vids = []tlVideo{{base: "v", path: "v.mkv", dur: 60}}
	a.ed.stale = true

	a.updateCutInfo() // no frames under this outDir: nothing to load
	if a.ed.stale {
		t.Error("the page rebuilt itself and still says it is behind")
	}
	// ...and the recordings of the session that is over are off the tracks. A
	// project opened over another one is the case: its folder holds no cut, and
	// the previous project's timeline sitting there is the most convincing wrong
	// thing this page can show.
	if len(a.ed.vids) != 0 {
		t.Errorf("the tracks still hold %d recording(s) after the session behind them went away", len(a.ed.vids))
	}
}

func TestClearTracksLeavesNothingBehind(t *testing.T) {
	a := staleEd(t)
	ed := a.ed
	ed.vids = []tlVideo{{base: "v", path: "v.mkv", dur: 60}}
	ed.segs = []cutSeg{{S: 1, E: 2}}
	ed.undo = []cutState{{segs: []cutSeg{{S: 0, E: 1}}}}
	ed.base = cutState{segs: []cutSeg{{S: 1, E: 2}}}
	ed.sel.active, ed.hasPlay, ed.hasIn, ed.edgeOn = true, true, true, true

	ed.clearTracks()

	if len(ed.vids) != 0 || len(ed.segs) != 0 || len(ed.undo) != 0 || len(ed.base.segs) != 0 {
		t.Errorf("clearTracks left state behind: %d vids, %d segs, %d undo, %d base",
			len(ed.vids), len(ed.segs), len(ed.undo), len(ed.base.segs))
	}
	// a selection, a playhead or a held mark that outlives the timeline it was
	// measured against points into a recording that is no longer on the page
	if ed.sel.active || ed.hasPlay || ed.hasIn || ed.edgeOn {
		t.Errorf("clearTracks kept a mark on an empty timeline: sel=%v play=%v in=%v edge=%v",
			ed.sel.active, ed.hasPlay, ed.hasIn, ed.edgeOn)
	}
	if ed.totalW != 0 {
		t.Errorf("the timeline is still %g px wide with nothing on it", ed.totalW)
	}
}

// The four things that move what the tracks are made of. This is a
// source-reading test because every one of them is a line in a callback that
// only runs with a display -- and a missing line is invisible until someone
// runs Describe and finds an empty page, which is where this started.
func TestEverythingThatMovesTheTracksSaysSo(t *testing.T) {
	for _, c := range []struct{ file, what, why string }{
		{"prep.go", "Prepare finishing",
			"it writes the frames the strip is drawn from and the session timeline printed on them -- the first Cut anyone opens is the one after this run"},
		{"project.go", "a project being applied",
			"Open and New Project replace the sources wholesale, and the tracks are the sources"},
	} {
		b, err := os.ReadFile(c.file)
		if err != nil {
			t.Fatal(err)
		}
		if !strings.Contains(string(b), "a.refreshCut()") {
			t.Errorf("%s: %s does not call refreshCut -- %s", c.file, c.what, c.why)
		}
	}

	// and the page acts on the note when it is opened. Without this the note is
	// a field nobody reads and the fix is only half there.
	b, err := os.ReadFile("main.go")
	if err != nil {
		t.Fatal(err)
	}
	src := string(b)
	i := strings.Index(src, `if name == "cut"`)
	if i < 0 {
		t.Fatal("showStep no longer has a Cut arrival branch")
	}
	arrival := src[i:min(i+400, len(src))]
	if !strings.Contains(arrival, "stale") || !strings.Contains(arrival, "updateCutInfo") {
		t.Errorf("arriving at Cut does not rebuild stale tracks:\n%s", arrival)
	}
}

// The launch still fills the page itself: it is the one moment where nothing
// ran and nothing was opened, and the tracks have to come from the project that
// was already there.
func TestTheLaunchStillFillsTheCutPage(t *testing.T) {
	b, err := os.ReadFile("main.go")
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(b), "a.updateCutInfo()") {
		t.Error("build() no longer loads the cut editor at startup")
	}
	// the gate and the loader have to agree on what "there is something to cut"
	// means, or the tab unlocks onto a page that declines to fill itself
	if !strings.Contains(string(b), "a.cutLocked = !a.canCut()") {
		t.Error("updateGates no longer unlocks the Cut tab on canCut")
	}
	src, err := os.ReadFile("cut.go")
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(src), "if !a.canCut() {") {
		t.Error("updateCutInfo no longer checks what the Cut tab is unlocked by")
	}
}
