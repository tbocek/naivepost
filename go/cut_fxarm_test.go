package main

// ⊕ Zoom does not open a form. It ARMS the next drag on the preview, and until
// now the only thing that said so was setStatus -- a dim, ellipsized line in
// the log header at the bottom of the window. Mark a stretch, pick ⊕ Zoom, and
// from the chair nothing whatever happens: no dialog, no change to the picture,
// nothing on the timeline. The instruction goes in the column now, where the
// forms it is an instruction about already are.
//
// And the marked stretch is what the effect covers, which is the rule ⏩ Speed
// already followed alone: marking the seconds and then framing them is two
// halves of one sentence, and typing the length again afterwards is the join
// showing.

import (
	"math"
	"strings"
	"testing"
)

func TestAnArmedDragSaysSoInTheColumn(t *testing.T) {
	// every path that arms or disarms goes through syncFxCursor -- Esc, Cancel,
	// picking the kind twice, and the release that places the effect -- so the
	// note is hung there rather than on each of them
	if !strings.Contains(funcBody(t, "cut_fxview.go", `func \(ed \*cutEditor\) syncFxCursor\(`),
		"ed.syncFxArm()") {
		t.Error("arming no longer puts its instruction in the column")
	}
	body := funcBody(t, "cut_fx.go", `func \(ed \*cutEditor\) syncFxArm\(`)
	for _, want := range []string{
		"if ed.formBox == nil {",      // headless, and every test is
		"if ed.fxArm == \"\" {",       // disarmed: the note comes out
		"if ed.formArm == ed.fxArm {", // and is not rebuilt under the hand every redraw
		"ed.showFormFoot(title, body, foot, func() { ed.formArm = \"\" })",
	} {
		if !strings.Contains(body, want) {
			t.Errorf("syncFxArm no longer does %s", want)
		}
	}
	// the flag is set AFTER the show: showing takes the last form out, and if
	// that was this note, its gone would clear the flag we had just set
	if i, j := strings.Index(body, "ed.showFormFoot("), strings.Index(body, "ed.formArm = ed.fxArm"); i < 0 || j < i {
		t.Error("syncFxArm claims the column before showing in it, so the outgoing note " +
			"disowns the incoming one")
	}
	// Cancel is the visible Esc: the note is the one place the armed state is
	// visible, so it is also the one place it can be put down by hand
	if !strings.Contains(body, "cancel.ConnectClicked(func() { ed.disarmFx() })") {
		t.Error("the armed note cannot be cancelled from the column")
	}
	dis := funcBody(t, "cut_fx.go", `func \(ed \*cutEditor\) disarmFx\(`)
	if !strings.Contains(dis, `ed.fxArm = ""`) || !strings.Contains(dis, "ed.syncFxCursor()") {
		t.Errorf("disarmFx no longer disarms and re-syncs:\n%s", dis)
	}
}

func TestAnArmedEffectCoversTheMarkedStretch(t *testing.T) {
	ed := &cutEditor{playhead: 82.3}

	// nothing marked: the red line and the caller's default
	if t0, dur := ed.fxSpanNow(3); t0 != 82.3 || dur != 3 {
		t.Errorf("fxSpanNow with no band = %g,%g want 82.3,3", t0, dur)
	}
	// marked: the band, whichever way round it was drawn
	ed.sel.active, ed.sel.t0, ed.sel.t1 = true, 119.5, 82.3
	t0, dur := ed.fxSpanNow(3)
	if t0 != 82.3 || math.Abs(dur-37.2) > 1e-9 {
		t.Errorf("a right-to-left band gives %g,%g want 82.3,37.2", t0, dur)
	}
	// a band too narrow to be meant is a click that shivered
	ed.sel.t0, ed.sel.t1 = 82.3, 82.4
	if t0, dur := ed.fxSpanNow(3); t0 != 82.3 || dur != 3 {
		t.Errorf("a %g s band was taken as a stretch: %g,%g", fxMinSel, t0, dur)
	}
	// and a band that is not up is not a band
	ed.sel.active, ed.sel.t0, ed.sel.t1 = false, 119.5, 82.3
	if _, dur := ed.fxSpanNow(3); dur != 3 {
		t.Errorf("a dropped band still placed the effect over %g s", dur)
	}

	// what the column says about it, which is the only place the link between
	// the band and the effect is stated
	ed.sel.active, ed.sel.t0, ed.sel.t1 = true, 82.3, 119.5
	ed.fxArm = "zoom"
	w := ed.fxArmWords()
	for _, want := range []string{"1:22 – 1:59", "37.2 s", "Drag a box on the video"} {
		if !strings.Contains(w, want) {
			t.Errorf("the armed zoom does not mention %q:\n%s", want, w)
		}
	}
	ed.sel.active = false
	if w := ed.fxArmWords(); !strings.Contains(w, "red line") || strings.Contains(w, "marked stretch") {
		t.Errorf("an unmarked zoom claims a stretch it does not have:\n%s", w)
	}
	// each kind says what it is waiting for in its own words
	ed.fxArm = "text"
	if w := ed.fxArmWords(); !strings.Contains(w, "the words go in") {
		t.Errorf("an armed text asks for a zoom's box:\n%s", w)
	}
	ed.fxArm, ed.fxSrc = "svg", "/tmp/logo.svg"
	if w := ed.fxArmWords(); !strings.Contains(w, "logo.svg") {
		t.Errorf("an armed svg does not name the drawing:\n%s", w)
	}

	// the two places an armed drag lands, and ⏩ Speed, all read the band the
	// same way -- one rule, so a zoom and a speed cannot disagree about what
	// "this stretch" means
	view := readSrc(t, "cut_fxview.go")
	if n := strings.Count(view, "ed.fxSpanNow(3)"); n != 2 {
		t.Errorf("%d of the 2 armed releases place the effect on the marked stretch", n)
	}
	if !strings.Contains(funcBody(t, "cut_fx.go", `func \(a \*App\) speedClicked\(`), "ed.fxMarked()") {
		t.Error("⏩ Speed decides for itself what counts as a marked stretch")
	}
}
