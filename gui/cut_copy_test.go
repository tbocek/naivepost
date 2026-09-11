package main

// Copying a selection and pasting it at the red line.
//
// A copy is spelled as an insert whose "file" is the session itself --
// copy:SECONDS with its length in Dur -- which is what buys it every behaviour
// a spliced card already has: the violet marker with the hatching over it,
// pick up and move, Edit, Undo, cut.json. These tests hold the spelling, the
// two button verbs that make and place one, and the render's one special case:
// a copy is cut from its recording like footage, not stretched into a slot
// like a file.

import (
	"math"
	"os"
	"strings"
	"testing"
)

// TestACopyIsSpelledAsASplicedInsert: the scheme parses, nothing else does,
// and a pasted copy answers yes to exactly the questions a spliced card does
// -- spliced() is the one the hatching is drawn from.
func TestACopyIsSpelledAsASplicedInsert(t *testing.T) {
	if from, ok := copySrc("copy:12.300"); !ok || math.Abs(from-12.3) > 1e-9 {
		t.Errorf("copy:12.300 parsed as %v %v", from, ok)
	}
	for _, notACopy := range []string{"card.svg", "sting.mp4?title=Later", "copy:", "copy:x", "copy:-3", ""} {
		if _, ok := copySrc(notACopy); ok {
			t.Errorf("%q parsed as a copy", notACopy)
		}
	}
	c := cutSeg{S: 40, E: 40, Ins: "copy:12.300", Dur: 5}
	if !c.isCopy() || !c.isInsert() || !c.spliced() {
		t.Errorf("a pasted copy answers isCopy=%v isInsert=%v spliced=%v, want true to all three "+
			"— spliced is what the hatching and the point-marker are drawn from",
			c.isCopy(), c.isInsert(), c.spliced())
	}
	// the marker names the seconds, not the pseudo-path: there is no file for
	// the eye to recognize, and the time is how it finds the original
	if got := insName(c); got != "copy of 0:12" {
		t.Errorf("the copy's marker reads %q, want %q", got, "copy of 0:12")
	}
	if got := insName(cutSeg{Ins: "assets/tier.svg?a=b"}); got != "tier.svg" {
		t.Errorf("a card's marker reads %q — insName must not touch real inserts", got)
	}
}

// TestCopyTakesTheSelectionInHand: ⧉ Copy measures the selection and keeps it;
// the cut is untouched. A selection dragged right-to-left is the same stretch
// of footage, and one under the minimum is refused rather than pasted as
// nothing.
func TestCopyTakesTheSelectionInHand(t *testing.T) {
	a := &App{root: t.TempDir()}
	ed := moveEd(t)
	ed.a, a.ed = a, ed
	ed.segs = []cutSeg{{S: 0, E: 60}}

	ed.sel.t0, ed.sel.t1, ed.sel.active = 25, 15, true // dragged backwards
	a.copyClicked()
	if !ed.copyOn || ed.copyFrom != 15 || ed.copyLen != 10 {
		t.Errorf("copy of a backwards selection = on=%v from=%v len=%v, want 15 for 10 s",
			ed.copyOn, ed.copyFrom, ed.copyLen)
	}
	if len(ed.segs) != 1 || len(ed.undo) != 0 {
		t.Error("taking a copy edited the cut — copying is reading, not editing")
	}

	// too short to be worth a clip: the hand stays as it was
	ed.copyOn = false
	ed.sel.t0, ed.sel.t1 = 15, 15.4
	a.copyClicked()
	if ed.copyOn {
		t.Errorf("a %.1f s selection was copied — under minSegLn there is nothing to paste", 0.4)
	}
}

// TestPasteSplicesTheCopiedFootageAtTheRedLine: one press, one undoable edit,
// and the copy lands as a point at the playhead with the copied seconds in
// Dur. Pasting consumes the copy -- the button has to go back to being Insert,
// or the file chooser is unreachable.
func TestPasteSplicesTheCopiedFootageAtTheRedLine(t *testing.T) {
	a := &App{root: t.TempDir()}
	ed := moveEd(t)
	ed.a, a.ed = a, ed
	ed.segs = []cutSeg{{S: 0, E: 60}}
	ed.copyFrom, ed.copyLen, ed.copyOn = 15, 10, true

	// no red line, no paste: the copy stays in hand for when there is one
	ed.hasPlay = false
	a.pasteCopy()
	if len(ed.segs) != 1 || !ed.copyOn {
		t.Fatal("pasting without a playhead edited the cut or dropped the copy")
	}

	ed.playhead, ed.hasPlay = 40, true
	before := ed.cutLen()
	a.pasteCopy()
	var got *cutSeg
	for i := range ed.segs {
		if ed.segs[i].isCopy() {
			got = &ed.segs[i]
		}
	}
	if got == nil {
		t.Fatal("no copy segment in the cut after pasting")
	}
	if got.S != 40 || got.E != 40 || got.Dur != 10 || got.Ins != "copy:15.000" {
		t.Errorf("pasted %+v, want a point at 40 playing copy:15.000 for 10 s", *got)
	}
	if ed.copyOn {
		t.Error("the copy is still in hand after pasting — Insert would stay Paste forever")
	}
	if len(ed.undo) != 1 {
		t.Errorf("pasting pushed %d undo state(s), want exactly 1", len(ed.undo))
	}
	if grew := ed.cutLen() - before; math.Abs(grew-10) > 1e-9 {
		t.Errorf("the cut grew by %.1f s, want the copied 10 — a splice costs no footage", grew)
	}
}

// TestTheRenderCutsACopyFromItsRecording: splitSpliced opens the footage at
// the paste point exactly as it does for a card, so the sequence the renderer
// sees is footage, the copy, the rest of the footage.
func TestTheRenderCutsACopyFromItsRecording(t *testing.T) {
	segs := []cutSeg{{S: 0, E: 60}, {S: 40, E: 40, Ins: "copy:15.000", Dur: 10}}
	out := splitSpliced(segs)
	if len(out) != 3 {
		t.Fatalf("splitSpliced gave %d clips, want footage|copy|footage: %v", len(out), out)
	}
	if out[0].E != 40 || !out[1].isCopy() || out[2].S != 40 || out[2].E != 60 {
		t.Errorf("the render's sequence is %v — the footage is not opened at the paste point", out)
	}
}

// TestTheCopyPreviewReadsTheRecording: what the preview shows and plays for a
// copy comes from the session's own recording -- there is no file to open. The
// film runs at the card rate because footage moves, unlike a still.
func TestTheCopyPreviewReadsTheRecording(t *testing.T) {
	a := &App{root: t.TempDir()}
	ed := &cutEditor{a: a, vids: []tlVideo{{base: "v", path: "/rec/session.mkv", start: 5, dur: 120}}}
	a.ed = ed
	ed.hold.on = true // something is playing; sound follows the transport

	c := cutSeg{S: 70, E: 70, Ins: "copy:15.000", Dur: 10}
	if got := ed.cardVoice(&c); got != "/rec/session.mkv" {
		t.Errorf("a copy is heard from %q, want the recording it copies", got)
	}
	orphan := cutSeg{S: 70, E: 70, Ins: "copy:500.000", Dur: 10}
	if got := ed.cardVoice(&orphan); got != "" {
		t.Errorf("a copy of unfilmed seconds is heard from %q, want silence", got)
	}
	if f := a.newFilm("copy:15.000"); f.fps != insPreviewFPS {
		t.Errorf("a copy's film runs at %v fps, want the card rate — its footage moves", f.fps)
	}
}

// TestAPastedCopyWearsTheHatching: the same picture test the spliced card
// passes, on a copy. Violet says something goes in here; the hatch says the
// footage is cut open for it.
func TestAPastedCopyWearsTheHatching(t *testing.T) {
	ed := moveEd(t)
	ed.segs = []cutSeg{{S: 0, E: 60}, {S: 20, E: 20, Ins: "copy:5.000", Dur: 4}}
	const w, h = 400, 200
	at := renderTrack(t, ed, w, h)

	x := int(ed.xOf(20))
	band, ground := 0, 0
	for y := int(ed.picTop()) + 2; y < int(ed.picTop())+ed.thumbHt; y++ {
		for dx := -int(splicePx/2) + 2; dx <= int(splicePx/2)-2; dx++ {
			r, g, b := at(x+dx, y)
			if r > 90 && g > 70 && b < 90 {
				band++ // a yellow hatch stroke
			}
			if int(b) > int(r)+20 && b > 60 {
				ground++ // the violet the marker is drawn in
			}
		}
	}
	if band == 0 {
		t.Error("a pasted copy has no hatching — nothing says the footage is cut here")
	}
	if ground == 0 {
		t.Error("a pasted copy is not violet — nothing says something goes in here")
	}
}

// TestTheCopyIsWired pins the seams: the button and its place in the bar, the
// Insert button's Paste face, Esc emptying the hand, and the two files that
// read the scheme -- the preview cutting frames from the recording and the
// render cutting the clip from it.
func TestTheCopyIsWired(t *testing.T) {
	b, err := os.ReadFile("cut.go")
	if err != nil {
		t.Fatal(err)
	}
	src := string(b)
	for _, want := range []string{
		`ed.copyBtn = gtk.NewButtonFromIconName("edit-copy-symbolic")`,
		`ed.copyBtn.ConnectClicked(func() { a.copyClicked() })`,
		"bar.Append(linked(add, ed.splitBtn, ed.remBtn, ed.copyBtn, ed.pasteBtn, ins, ed.laneBtn))",
		// Paste is a button of its own, greyed with nothing in hand: it used
		// to be the Insert button relabelled, so one press meant "choose a
		// file" or "put the copy down" depending on a state nothing showed
		`ed.pasteBtn = gtk.NewButtonFromIconName("edit-paste-symbolic")`,
		"ed.pasteBtn.SetSensitive(ed.copyOn)",
		"ed.pasteBtn.ConnectClicked(func() { a.pasteCopy() })",
		// Esc empties the hand along with every other hold
		`(ed.edgeOn || ed.segOn || ed.fxOn || ed.selOn || ed.copyOn || ed.fxArm != "") && keyval == gdk.KEY_Escape:`,
		// the marker is named by insName, which is what says "copy of 0:12",
		// under a drawn card mark -- see cut_marks.go for why it is drawn
		`markPlate(cr, tx, top+th-2, "card", fmt.Sprintf("%s  %.1fs", insName(s), s.Dur))`,
	} {
		if !strings.Contains(src, want) {
			t.Errorf("cut.go does not contain %q", want)
		}
	}
	iv, err := os.ReadFile("cut_insview.go")
	if err != nil {
		t.Fatal(err)
	}
	for _, want := range []string{
		"if from, ok := copySrc(s.Ins); ok {", // the sound comes from the recording
		"if from, ok := copySrc(ins); ok {",   // and so does the picture
	} {
		if !strings.Contains(string(iv), want) {
			t.Errorf("cut_insview.go does not contain %q", want)
		}
	}
	prod, err := os.ReadFile("produce.go")
	if err != nil {
		t.Fatal(err)
	}
	for _, want := range []string{
		"case s.isCopy():", // before the generic insert case: a copy IS an insert
		"c.sessS = from",   // its cues follow the copied footage, not the paste point
	} {
		if !strings.Contains(string(prod), want) {
			t.Errorf("produce.go does not contain %q", want)
		}
	}
}
