package main

// The label: a name for a moment, and the one effect that changes nothing.
//
// It was a field on every effect for an afternoon -- a Label box on all five
// forms -- which put the same question on five dialogs and tied a name to
// something that had to be a zoom or a speed before it could be a name. A
// moment worth telling the narration writer about is usually not a moment you
// wanted an effect on.
//
// So it is its own kind. No picture, no sound, nothing in the render at all: it
// exists to be read in the brief the narration writer is given, where it is the
// only line that is neither what was said nor what the frames showed. It is the
// editor saying what they think is happening, at the second they think it
// happens, and a note on Prepare can point at it by name.

import (
	"strings"
	"testing"
)

func TestALabelIsAnEffectOfItsOwn(t *testing.T) {
	f := cutFx{Kind: "label", T: 65, Dur: 4, Text: "the reveal"}
	if got := f.fxLabel(); !strings.Contains(got, "the reveal") || !strings.Contains(got, "1:05") {
		t.Errorf("a label introduces itself as %q", got)
	}
	// the lane says the name and nothing else: there is nothing else about it
	if mark, label := laneLabel(f, 20); mark != "label" || label != "the reveal" {
		t.Errorf("the lane calls it %q/%q", mark, label)
	}
	// it covers seconds like everything else on the lane, so it can be picked
	// up, dragged and thrown away by the same gestures
	if t0, t1 := f.fxSpan(); t0 != 65 || t1 != 69 {
		t.Errorf("a label covers %g–%g s", t0, t1)
	}
	// the words are the effect's own Text, so there is no second field on
	// every other kind carrying a name it never has
	src := readSrc(t, "cut_fx.go")
	if strings.Contains(src, "Label string") || strings.Contains(src, "f.Label") {
		t.Error("cutFx carries a Label field again")
	}
	if n := strings.Count(src, "fxWordsRow(f, live)"); n != 1 {
		t.Errorf("%d dialogs ask for a name, want the label's alone", n)
	}
	// a name is a name: the box is the width of one, not of the panel
	if !strings.Contains(src, "e.SetWidthChars(fxWordChars)") || !strings.Contains(src, "fxWordChars = 10") {
		t.Error("the name box is not a name's width")
	}
}

// The render never reads it, and the page places it like the other effect that
// has no picture to point at.
func TestALabelChangesNothingInTheVideo(t *testing.T) {
	fx := []cutFx{{Kind: "label", T: 10, Dur: 5, Text: "x"}}
	segs := []cutSeg{{S: 0, E: 60}}
	// the clock is untouched: no rate, no split, no window of silence
	got := applyFx(segs, fx)
	if len(got) != 1 || got[0].S != 0 || got[0].E != 60 || got[0].Rate != 0 {
		t.Errorf("a label changed the render's segments: %+v", got)
	}
	if len(speedsOf(fx)) != 0 || len(hushCues(fx, 0, 60, 1, 60)) != 0 {
		t.Error("a label reached the sound")
	}
	if len(gainCues(fx, 0, 60, 1, 60)) != 0 || len(textCues(fx, 0, 60, 1, 60)) != 0 {
		t.Error("a label reached the picture")
	}
	if fxHasCamera("", fx) {
		t.Error("a label turned the camera machinery on")
	}
	// the menu offers it, the button places it, and editing one reopens it
	src := readSrc(t, "cut.go")
	for _, want := range []string{`"🏷 Label"`, "case 6:\n\t\t\ta.labelClicked()"} {
		if !strings.Contains(src, want) {
			t.Errorf("cut.go no longer contains %q", want)
		}
	}
	fxsrc := readSrc(t, "cut_fx.go")
	if !strings.Contains(fxsrc, `case "label":`+"\n\t\ta.askLabelParams(was, false,") {
		t.Error("a placed label cannot be edited again")
	}
	// nothing is placed until it has a name -- an unnamed label is a mark
	// nobody can act on, and it is the words that make it one
	if !strings.Contains(fxsrc, `if strings.TrimSpace(f.Text) == "" {`) {
		t.Error("an unnamed label is placed anyway")
	}
}

// What it is for: the narration writer is given it, on the clip it falls in,
// stamped like everything else in that block.
func TestTheNarrationBriefNamesTheMarkedMoments(t *testing.T) {
	segs := []cutSeg{{S: 100, E: 160}, {S: 300, E: 320}}
	rows := []tsvRow{{s: 105, e: 108, spk: "SPEAKER_01", text: "here we go", src: "cap"}}
	fx := []cutFx{
		{Kind: "label", T: 112, Dur: 4, Text: "the reveal"},
		{Kind: "zoom", T: 305, Dur: 3},                       // not a label: not in the brief
		{Kind: "label", T: 500, Dur: 2, Text: "off the cut"}, // no clip: not in the brief
		{Kind: "label", T: 130, Dur: 2, Text: "  "},          // no name: nothing to say
	}
	brief := clipBriefs(segs, rows, fx, "")
	if !strings.Contains(brief, "[+12s] MARKED: the reveal") {
		t.Errorf("the brief does not name the marked moment:\n%s", brief)
	}
	if strings.Contains(brief, "off the cut") {
		t.Errorf("a label outside every clip reached the brief:\n%s", brief)
	}
	if strings.Count(brief, "MARKED") != 1 {
		t.Errorf("something that is not a named moment was announced:\n%s", brief)
	}
	// the run hands the cut's own effects over
	if !strings.Contains(readSrc(t, "narrate.go"), "clipBriefs(segs, rows, a.produceCut().Fx, a.narratorMic())") {
		t.Error("the narration run no longer gives the writer the labels")
	}
	// ...and the system context says what such a line is, since it is neither
	// something said nor something the picture showed
	if !strings.Contains(readSrc(t, "syscontext.go"), "may also carry MARKED") {
		t.Error("the system context does not explain a MARKED line")
	}
}
