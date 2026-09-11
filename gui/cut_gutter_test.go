package main

import (
	"math"
	"strings"
	"testing"
)

// The timeline starts a strip in from the widget's edge, and that strip is
// black: scroll home and you scroll PAST the start of the session into space
// that belongs to the controls alone.
func TestTheTapeStartsPastTheGutter(t *testing.T) {
	ed := foldEd(t) // one recording 0-300 at 4 px a second
	if got := ed.xOf(0); got != gutterPx {
		t.Errorf("session second 0 is at %g, want the head of the tape at %g", got, gutterPx)
	}
	// and the strip is not time: an x in it reads as the first second, the way
	// an x in an unfilmed hole reads as the second the next run starts on
	if got := ed.tAt(gutterMid); got != 0 {
		t.Errorf("a press in the gutter reads as %g s, want 0", got)
	}
	// it costs the footage its width, exactly as a hole does -- fitted to the
	// window itself, the zoomed-out timeline would be wider than its window by
	// this strip and the scrollbar would stay for nothing
	ed.viewW = 1200
	if got, want := ed.minPps(), fitPps(1200-gutterPx, ed.filmedDur()); got != want {
		t.Errorf("zoom-to-fit is %g px/s, want %g", got, want)
	}
	// an empty page has no tape, and so no strip in front of one
	empty := newTestEd(t)
	empty.layoutPx()
	if empty.totalW != 0 {
		t.Errorf("the timeline is %g px wide with nothing on it", empty.totalW)
	}
}

// What stands in it: the switches that used to be pinned to the widget's left
// edge and drawn ON the footage -- a speaker plate in the middle of a waveform
// covers the reading the band exists to show.
func TestTheSwitchesStandInTheGutterRatherThanOnTheFootage(t *testing.T) {
	if laneSwX != gutterMid {
		t.Errorf("a lane's switch is at %g, not in the gutter at %g", laneSwX, gutterMid)
	}
	if laneNameX < gutterPx {
		t.Errorf("a lane's name starts at %g, inside the gutter that ends at %g", laneNameX, gutterPx)
	}
	// they are timeline x now, so nothing puts them back on the tape's scale
	// by the view's left edge at the draw
	hear := readSrc(t, "cut_hear.go")
	for _, gone := range []string{"hearPlate(cr, ed.viewX+s.cx", "laneSwX   = 12.0"} {
		if strings.Contains(hear, gone) {
			t.Errorf("a switch is still pinned to the widget: %q", gone)
		}
	}
	// ...and the press asks in the same coordinates it is drawn in
	cut := readSrc(t, "cut.go")
	for _, want := range []string{
		"if base := ed.laneSwitchAt(x+ed.viewX, y); base != \"\" {",
		"if bases := ed.pairSwitchAt(x+ed.viewX, y); len(bases) > 0 {",
	} {
		if !strings.Contains(cut, want) {
			t.Errorf("a switch is pressed in the wrong coordinates: %q", want)
		}
	}
	// the strip is drawn under them in both bands, black
	if !strings.Contains(cut, "ed.drawGutter(cr, top, bandH)") {
		t.Error("the picture band draws no gutter")
	}
	if !strings.Contains(readSrc(t, "cut_audio.go"), "ed.drawGutter(cr, 0, fh)") {
		t.Error("the recorders' band draws no gutter")
	}
	if !strings.Contains(readSrc(t, "cut_fold.go"), "cr.SetSourceRGB(0, 0, 0)") {
		t.Error("the gutter is not black, so it reads as more timeline with nothing on it")
	}
}

// The one control in it that belongs to the whole page rather than to a row:
// fold every dropped stretch away, or bring them all back.
func TestTheGutterFoldsTheLot(t *testing.T) {
	ed := foldEd(t) // gaps 0-20, 60-100, 140-300
	y := ed.foldAllY()
	if !ed.foldAllAt(gutterMid, y) {
		t.Fatal("the fold-all control is not where the gutter puts it")
	}
	if ed.foldAllOn() {
		t.Error("nothing is folded and the control says otherwise")
	}
	ed.toggleFoldAll()
	if len(ed.folds) != 3 {
		t.Fatalf("one press folded %d of three gaps", len(ed.folds))
	}
	if !ed.foldAllOn() {
		t.Error("everything is folded and the control still offers to fold")
	}
	// the three clips-worth of dropped time is gone from the page
	if got := ed.totalW; math.Abs(got-(gutterPx+80*ed.pps)) > 0.01 {
		t.Errorf("with every gap folded the page is %g px, want %g",
			got, gutterPx+80*ed.pps)
	}
	// and one press brings the lot back -- with anything folded, that is what
	// it does, because getting the page back is worth more than folding the
	// last gap
	ed.toggleFoldAll()
	if len(ed.folds) != 0 {
		t.Errorf("the second press left %d folds", len(ed.folds))
	}
	// a cut with no dropped stretch at all has nothing to fold, and offers
	// nothing: no badge, no press
	ed.segs = []cutSeg{{S: 0, E: 300}}
	if ed.foldAllAt(gutterMid, y) {
		t.Error("a cut that drops nothing still offers to fold it")
	}
	// it is asked before the bands, since it stands in front of second zero
	cut := readSrc(t, "cut.go")
	i := strings.Index(cut, "if area == ed.srcArea && ed.foldAllAt(x+ed.viewX, y) {")
	j := strings.Index(cut, "if area == ed.srcArea && ed.hitSelBand(y) {")
	if i < 0 || j < 0 || i > j {
		t.Errorf("the fold-all control is not asked before the selection band (%d, %d)", i, j)
	}
}

// A press on a control in the gutter is a press on that control, and nothing
// else. Every second in the strip is second zero, so a click there cued the
// red line to the start of the session -- switching a lane's sound off threw
// the playhead to 0:00 as a side effect. Only the black between the controls
// is a place to put the line.
func TestAPressOnAGutterControlLeavesTheLineWhereItIs(t *testing.T) {
	ed := foldEd(t)
	y := ed.foldAllY()
	if !ed.gutterCtl(gutterMid, y) {
		t.Error("the fold-all badge does not count as a gutter control")
	}
	// the empty black beside it is not a control: a click there is a click on
	// the timeline like any other
	if ed.gutterCtl(gutterMid, y+40) {
		t.Error("empty gutter counts as a control, so the line can never be put at the start")
	}
	// ...and neither is anything on the tape itself
	if ed.gutterCtl(ed.xOf(30), y) {
		t.Error("a press on the tape reads as a gutter control")
	}
	// the click that cues the line asks first
	if !strings.Contains(readSrc(t, "cut.go"), "if !ed.gutterCtl(dragStartX+ed.viewX, dragStartY) {") {
		t.Error("a click on a gutter control still throws the red line to second zero")
	}
}
