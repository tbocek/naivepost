package main

// Removing a row that has nothing on it.
//
// A right-drag that moves a part onto another row leaves the old row standing
// empty on purpose -- closing it there would renumber every scene's camera as
// a side effect of a drag that meant something else (moveRow). The ✕ the empty
// row wears is where that renumbering is asked for by name: the row goes, the
// rows below come up one, and everything that names a row -- pins, scenes, the
// selection, a copy in hand, the watch -- is brought along or let go, exactly
// as a cut lane's ✕ does it. One shared closeRow, because two copies of that
// surgery would be two chances for a scene to end up showing a camera nobody
// chose.

import (
	"encoding/json"
	"os"
	"strings"
	"testing"
)

// gapEd is two recordings pinned to rows 0 and 2, so row 1 stands empty --
// the shape a cross-row drag leaves behind.
func gapEd(t *testing.T) *cutEditor {
	t.Helper()
	ed := axisEd(t,
		tlVideo{base: "a", path: "/f/a.mp4", start: 0, dur: 100},
		tlVideo{base: "b", path: "/f/b.mp4", start: 10, dur: 80})
	ed.rows = map[string]int{"a": 0, "b": 2}
	ed.relayout()
	if ed.laneN != 3 || !ed.rowEmpty(1) {
		t.Fatalf("the fixture is wrong: %d rows, row 1 empty=%v", ed.laneN, ed.rowEmpty(1))
	}
	return ed
}

func TestAnEmptyRowsXTakesItAwayAndTheRowsBelowComeUp(t *testing.T) {
	ed := gapEd(t)
	ed.segs = []cutSeg{
		{S: 0, E: 5, Cam: 0},
		{S: 20, E: 30, Cam: 2},
		{S: 40, E: 50, Cam: 1}, // green pointing at the empty row: it goes with it
	}
	ed.sel.active, ed.sel.lane = true, 2
	ed.monRow = 3 // watching row 2, which is about to become row 1
	ed.killRow(1)
	if ed.laneN != 2 {
		t.Fatalf("removing the empty row left %d rows, want 2", ed.laneN)
	}
	if v := videoOn(ed.vids, 1, 20); v == nil || v.base != "b" {
		t.Error("the recording below the gap did not come up onto row 1")
	}
	if len(ed.segs) != 2 || ed.segs[1].Cam != 1 {
		t.Errorf("the scenes did not follow their cameras: %+v", ed.segs)
	}
	if ed.sel.lane != 1 || ed.monRow != 2 {
		t.Errorf("a live row number was left naming the old shape: sel.lane=%d monRow=%d",
			ed.sel.lane, ed.monRow)
	}
	if len(ed.undo) == 0 {
		t.Error("removing a row is an edit, and it left nothing to undo")
	}
}

func TestTheXRefusesARowWithFootageOnIt(t *testing.T) {
	ed := gapEd(t)
	ed.killRow(0) // a has footage there
	ed.killRow(5) // and there is no such row
	if ed.laneN != 3 || len(ed.undo) != 0 {
		t.Errorf("a refusal must change nothing: %d rows, %d undos", ed.laneN, len(ed.undo))
	}
}

func TestTheXSitsAtTheEmptyRowsLeftEdge(t *testing.T) {
	ed := gapEd(t)
	if r := ed.rowKillAt(ed.viewX+killIn, ed.laneTop(1)+segKillTop); r != 1 {
		t.Errorf("the badge on the empty row answers %d, want 1", r)
	}
	// and it is DRAWN where it is pressed. It was not: the draw was handed
	// drawTrack's culling edge, which sits a margin left of the view so a
	// thumbnail reaching in still gets painted -- so the badge went 80 px off
	// the side of the widget while the press worked at the edge you can see.
	at := renderTrack(t, ed, 620, int(ed.picBottom())+8)
	plate := false
	for dx := -3; dx <= 3 && !plate; dx++ {
		for dy := -3; dy <= 3 && !plate; dy++ {
			r, g, b := at(int(ed.viewX)+killIn+dx, int(ed.laneTop(1)+segKillTop)+dy)
			if r > 200 && g > 200 && b > 200 {
				plate = true
			}
		}
	}
	if !plate {
		t.Error("nothing is drawn at the empty row's ✕, where a press removes the row")
	}
	// the same spot on a row with footage offers nothing: that row's removal
	// has to say what happens to the footage, and this ✕ has no answer
	if r := ed.rowKillAt(ed.viewX+killIn, ed.laneTop(0)+segKillTop); r != -1 {
		t.Errorf("a full row grew a removal badge: %d", r)
	}
}

func TestARowEmptiedByADragWaitsForItsX(t *testing.T) {
	// an empty row BETWEEN two full ones survives relayout because the pins
	// hold the gap open; the bottom row has nothing under it and used to fold
	// up the moment a drag vacated it -- an auto-removal nobody asked for,
	// and the ✕ never got its chance. The floor (nRows) is what holds it now.
	ed := axisEd(t,
		tlVideo{base: "a", path: "/f/a.mp4", start: 0, dur: 50},
		tlVideo{base: "b", path: "/f/b.mp4", start: 60, dur: 30})
	ed.rows = map[string]int{"a": 0, "b": 1}
	ed.relayout()
	if !ed.moveRow([]string{"b"}, 0) {
		t.Fatal("the move has room and was refused")
	}
	if ed.laneN != 2 || !ed.rowEmpty(1) {
		t.Fatalf("the vacated bottom row folded up on its own: %d rows", ed.laneN)
	}
	// the floor survives a save, so the row still waits after a restart
	ed.persist()
	b, err := os.ReadFile(ed.a.cutPath())
	if err != nil {
		t.Fatal(err)
	}
	var c cutFile
	if json.Unmarshal(b, &c) != nil || c.NRows != 2 {
		t.Errorf("the row floor is not in cut.json: %d", c.NRows)
	}
	ed.killRow(1)
	if ed.laneN != 1 {
		t.Fatalf("✕ did not take the emptied row away: %d rows", ed.laneN)
	}
	ed.undoLast()
	if ed.laneN != 2 || !ed.rowEmpty(1) {
		t.Error("↶ Undo did not bring the empty row back")
	}
}

func TestTheEmptyRowsXIsWired(t *testing.T) {
	// press, paint and hover all go through the same two lookups; what is
	// pinned is that each seam still asks them
	src := readSrc(t, "cut.go")
	for _, want := range []string{
		"ed.killRow(r)",
		"ed.drawRowKill(cr)",
	} {
		if !strings.Contains(src, want) {
			t.Errorf("the empty row's ✕ came unwired: %q", want)
		}
	}
	if !strings.Contains(readSrc(t, "cut_lane.go"), "row = ed.rowKillAt(x+ed.viewX, y)") {
		t.Error("the badge no longer lights under the pointer")
	}
	// the floor comes back off disk, or a restart quietly folds the row up
	if !strings.Contains(src, "ed.nRows = c.NRows") {
		t.Error("cut.json's row floor is written but never read back")
	}
	// killLane closes its row through the same surgery; a second copy would
	// let the two ✕s renumber differently
	if strings.Count(readSrc(t, "cut_lane.go"), "ed.closeRow(") != 2 {
		t.Error("killLane and killRow no longer share closeRow")
	}
}
