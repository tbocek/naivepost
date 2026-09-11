package main

// Redo is Undo's other half. The undo pile already made every edit a try
// rather than a commitment; without a redo pile the try itself was one --
// press Undo once too often and the edit is simply gone. What these pin is
// the arithmetic of the two piles: Undo feeds Redo, Redo feeds Undo, and a
// fresh edit empties Redo, because what Undo took back no longer leads to the
// cut being edited and grafting it on would interleave two histories.

import (
	"os"
	"strings"
	"testing"
)

func TestRedoPutsBackWhatUndoTook(t *testing.T) {
	ed := newTestEd(t)
	ed.segs = []cutSeg{{S: 10, E: 20}}
	ed.pushUndo()
	ed.segs = []cutSeg{{S: 10, E: 20}, {S: 30, E: 40}}

	ed.undoLast()
	if len(ed.segs) != 1 || len(ed.redo) != 1 {
		t.Fatalf("undo left %d segment(s) and %d redo state(s), want 1 and 1", len(ed.segs), len(ed.redo))
	}
	ed.redoLast()
	if len(ed.segs) != 2 || ed.segs[1].S != 30 {
		t.Fatalf("redo restored %v, want the two-segment cut back", ed.segs)
	}
	if len(ed.undo) != 1 || len(ed.redo) != 0 {
		t.Errorf("after a redo the piles are undo=%d redo=%d, want 1 and 0 — "+
			"a redo must itself be undoable", len(ed.undo), len(ed.redo))
	}

	// undo, then a new edit: the taken-back cut no longer leads here
	ed.undoLast()
	ed.pushUndo()
	ed.segs = append(ed.segs, cutSeg{S: 50, E: 60})
	if len(ed.redo) != 0 {
		t.Errorf("an edit after Undo left %d redo state(s) — two histories would interleave", len(ed.redo))
	}
	// and with nothing to redo, nothing happens
	before := len(ed.segs)
	ed.redoLast()
	if len(ed.segs) != before {
		t.Error("an empty redo pile changed the cut")
	}
}

// One snapshot, not three: what Redo puts back includes the effects, exactly
// as Undo takes them (cutState).
func TestRedoRestoresTheEffectsToo(t *testing.T) {
	ed := newTestEd(t)
	ed.segs = []cutSeg{{S: 0, E: 60}}
	ed.fx = []cutFx{{Kind: "zoom", T: 10, Dur: 3, Cx: 0.5, Cy: 0.5, Hf: 0.6}}
	ed.pushUndo()
	ed.fx = nil

	ed.undoLast()
	if len(ed.fx) != 1 {
		t.Fatalf("undo left %d effect(s), want the zoom back", len(ed.fx))
	}
	ed.redoLast()
	if len(ed.fx) != 0 {
		t.Errorf("redo left %d effect(s), want the deletion back", len(ed.fx))
	}
}

// ...and the wiring: the button beside Undo, its shortcut, the sensitivity
// that follows the pile, and the line in pushUndo that forks history.
func TestTheRedoIsWired(t *testing.T) {
	b, err := os.ReadFile("cut.go")
	if err != nil {
		t.Fatal(err)
	}
	src := string(b)
	for _, want := range []string{
		`ed.redoBtn = gtk.NewButtonFromIconName("edit-redo-symbolic")`,
		"bar.Append(linked(ed.undoBtn, ed.redoBtn, ed.revertBtn, ed.clearBtn))",
		"ed.redoBtn.SetSensitive(len(ed.redo) > 0)",
		"ed.redo = nil", // pushUndo: a fresh edit forks history
		"case (keyval == gdk.KEY_Z || keyval == gdk.KEY_y) && state&gdk.ControlMask != 0:",
	} {
		if !strings.Contains(src, want) {
			t.Errorf("cut.go does not contain %q — the redo came unwired", want)
		}
	}
}
