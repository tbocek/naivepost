package main

import (
	"strings"
	"testing"
)

// The form and the drag both write the same effect, and they were fighting.
//
// An effect form opens in the page's column, not in a window, so the preview
// stays live beside it -- and the box on that preview is draggable while the
// form is open. Nothing in any dialog can set that box: a camera window and a
// caption's rectangle are only ever drawn with the hand. So the form has no
// business carrying one, and the snapshot it took when it opened is stale the
// moment the box is dragged. Saving used to write that snapshot back whole,
// which is what the report was: a box pulled bigger, then Save, then the box
// springing back to the size it had before it was touched.
func TestABoxDraggedWhileTheFormIsOpenSurvivesTheSave(t *testing.T) {
	root := t.TempDir()
	a := &App{root: root, outDir: root}
	ed := &cutEditor{a: a, pps: 4, thumbHt: 64}
	ed.a, a.ed = a, ed
	ed.fx = []cutFx{{Kind: "text", T: 30, Dur: 3, Text: "hello",
		Cx: 0.5, Cy: 0.78, Wf: 0.8, Hf: 0.16}}

	// what the form saw when it opened
	was := ed.fx[0]
	// ...and then the hand pulled the box out to the full width of the frame
	ed.fx[0].Cx, ed.fx[0].Cy, ed.fx[0].Wf, ed.fx[0].Hf = 0.5, 0.4, 1, 0.3

	// Save: the words and the seconds are the form's, the box is not
	nf := was
	nf.Text, nf.Dur = "hello there", 5
	ed.updateFx(was, nf)

	got := ed.fx[0]
	if got.Wf != 1 || got.Hf != 0.3 || got.Cy != 0.4 {
		t.Errorf("the save put the old box back: %+v", got)
	}
	if got.Text != "hello there" || got.Dur != 5 {
		t.Errorf("the form's own fields did not land: %+v", got)
	}

	// the same for a camera window, which has three of the four numbers and
	// no dialog field for any of them either
	ed.fx = []cutFx{{Kind: "zoom", T: 10, Dur: 2, Cx: 0.5, Cy: 0.5, Hf: 0.5}}
	wasZ := ed.fx[0]
	ed.fx[0].Cx, ed.fx[0].Hf = 0.2, 0.25
	nz := wasZ
	nz.Dur = 6
	ed.updateFx(wasZ, nz)
	if z := ed.fx[0]; z.Cx != 0.2 || z.Hf != 0.25 || z.Dur != 6 {
		t.Errorf("the camera window sprang back on save: %+v", z)
	}
}

// and the rule written down where it is applied
func TestTheSaveTakesTheBoxFromTheCutAndNotTheForm(t *testing.T) {
	body := funcBody(t, "cut_fx.go", `func \(ed \*cutEditor\) writeFx\(`)
	pin := "nf.Cx, nf.Cy, nf.Wf, nf.Hf = ed.fx[i].Cx, ed.fx[i].Cy, ed.fx[i].Wf, ed.fx[i].Hf"
	if !strings.Contains(body, pin) {
		t.Errorf("writeFx no longer carries the live box forward:\n%s", body)
	}
	if strings.Index(body, pin) > strings.Index(body, "ed.fx[i] = nf") {
		t.Error("the box is carried over after the effect has already been overwritten")
	}
}
