package main

// Two buttons, two verbs.
//
// The left button used to do four things, told apart by where it landed: draw
// a selection, trim a clip's border, slide a clip already in hand, and cue the
// red line. Three of those shared their pixels with the first, so a selection
// begun a few px from a border trimmed instead, and the only way to know which
// was about to happen was to have learned the tolerance. The way out of a clip
// you had picked up was to put it down somewhere else.
//
// So the page is two verbs now. The LEFT button says which seconds -- a drag
// is a selection wherever it is pressed, a click is the red line -- and the
// RIGHT button moves what is under it: the green (a scene, or a border of one)
// and, clear of the green, the recordings themselves, which is the timeline
// correction it already did.

import (
	"strings"
	"testing"
)

// closure is one indented callback's body: from its header line to the line
// that closes it at the same indent. funcBody cannot be used on these -- it
// stops at the first line that is a lone "}", which inside a page-building
// function is a hundred lines past the gesture being read.
func closure(t *testing.T, file, head string) string {
	t.Helper()
	src := readSrc(t, file)
	i := strings.Index(src, head)
	if i < 0 {
		t.Fatalf("%s: no callback opening with %s", file, head)
	}
	indent := src[strings.LastIndex(src[:i], "\n")+1 : i]
	j := strings.Index(src[i:], "\n"+indent+"})")
	if j < 0 {
		t.Fatalf("%s: the callback opening with %s never closes", file, head)
	}
	return src[i : i+j]
}

// What the left press may still do on the pictures and in the band: press a
// badge, drop a scene, and start a selection. Nothing that edits the cut.
func TestTheLeftButtonOnlySelects(t *testing.T) {
	body := closure(t, "cut.go", "drag.ConnectDragBegin(func(x, y float64) {")
	for _, gone := range []string{
		"trimming = true",
		"moving = true",
		"ed.pickAt(x+ed.viewX, false)",
		"ed.onHeldSeg(",
		"ed.holdBandClip(",
	} {
		if strings.Contains(body, gone) {
			t.Errorf("the left press still edits the cut: %q", gone)
		}
	}
	// what it does instead, at the end of the same begin
	for _, want := range []string{
		"ed.dropEdge() // any other left click puts a held edge or clip down",
		"ed.sel.active = true",
	} {
		if !strings.Contains(body, want) {
			t.Errorf("the left press no longer starts a selection: %q is gone", want)
		}
	}
	// the bar's ✕ is the one thing on the green it still answers: dropping a
	// scene is a press, not a drag, and it is where the mark is drawn
	if !strings.Contains(body, "if i := ed.bandKillAt(x + ed.viewX); i >= 0 {") {
		t.Error("the bar's ✕ is no longer pressed with the left button")
	}
	// and the left drag's update has nothing to drag but the selection, the
	// effects lane and the blue band
	up := closure(t, "cut.go", "drag.ConnectDragUpdate(func(ox, oy float64) {")
	for _, gone := range []string{"ed.moveEdgeTo(", "ed.moveSegTo("} {
		if strings.Contains(up, gone) {
			t.Errorf("the left drag still moves the cut: %q", gone)
		}
	}
}

// ...and the right press picks up what it landed on: a border, a scene, or the
// recordings under them. The selection's own case comes first -- a right-drag
// inside a selection moves every scene in it, which is the same verb over more
// of the cut -- and only then the green under the pointer.
func TestTheRightButtonMovesTheGreenAndThenTheTimeline(t *testing.T) {
	body := closure(t, "cut.go", "slide.ConnectDragBegin(func(x, y float64) {")
	for _, want := range []string{
		"i, part := ed.bandClipPartAt(px)",          // the bar: its ends and its middle
		"if ed.onHeldEdge(px) || ed.grabEdge(px) {", // the pictures: a border
		"if ed.segOnGreen(px, y) < 0 {",             // ...and only where the cut keeps something
		"case green():",
	} {
		if !strings.Contains(body, want) {
			t.Errorf("the right press no longer takes the green: %q is gone", want)
		}
	}
	sel := strings.Index(body, `slideWhat = "the selected scenes"`)
	grn := strings.Index(body, "case green():")
	shift := strings.Index(body, `slideWhat = fmt.Sprintf("camera %d"`)
	if sel < 0 || grn < 0 || shift < 0 {
		t.Fatalf("the right press no longer chooses between its three jobs:\n%s", body)
	}
	if sel > grn {
		t.Error("a scene inside a selection is taken alone, so the selection's own drag is unreachable")
	}
	if grn > shift {
		t.Error("the recordings are asked about before the green, so a scene cannot be moved")
	}
	// the drags themselves are the picture band's own machinery, not a second
	// copy of it, and they are asked before anything that shifts a recording
	up := closure(t, "cut.go", "slide.ConnectDragUpdate(func(ox, oy float64) {")
	for _, want := range []string{
		"ed.moveEdgeTo(ed.tAtView(slideX0+ox), true)",
		"ed.moveSegTo(ed.tAtView(slideX0+ox)-slideGrab, true)",
	} {
		if !strings.Contains(up, want) {
			t.Errorf("the right drag no longer runs the cut's own move: %q", want)
		}
	}
	if i, j := strings.Index(up, "if trimming || moving {"), strings.Index(up, "ed.shiftTo("); i < 0 || (j >= 0 && i > j) {
		t.Error("the right drag shifts recordings before it asks whether it is holding a scene")
	}
	// one binding of the right button, still: the cut's drags are cases of
	// the same gesture rather than a second one over the same pixels
	if n := strings.Count(readSrc(t, "cut.go"), "gdk.BUTTON_SECONDARY"); n != 1 {
		t.Errorf("the right button is bound %d times, want 1", n)
	}
}

// A ✕ is a button, and the pointer says so. Over an effect's band the cursor
// is the open hand that means "this whole thing moves", and the ✕ sits ON the
// band -- so without asking about it first, the one mark that removes the
// effect promised to drag it instead.
func TestThePointerOverAKillBadgeIsAPointer(t *testing.T) {
	ed := fxKillEd(t)
	cx, cy, ok := ed.fxKillCentre(0)
	if !ok {
		t.Fatal("the fixture's first effect wears no ✕")
	}
	if got := ed.wantCursor(cx-ed.viewX, cy); got != "pointer" {
		t.Errorf("over an effect's ✕ the cursor is %q, want a pointer", got)
	}
	// ...and a hair off it, the band's own answer: the ✕ takes its target and
	// nothing more
	if got := ed.wantCursor(cx-ed.viewX-3*segKillHit, cy); got == "pointer" {
		t.Error("the whole effect band reads as a button")
	}
}

// The live effect form: the first answer places the effect (or opens the undo
// step), and every answer after it writes onto that same effect without
// another step. A visit to a form is one thing to undo, not one per burst of
// typing -- which is what Cancel used to be, minus the promise that nothing
// had happened yet.
func TestALiveFormPlacesOnceAndWritesAfter(t *testing.T) {
	ed := newTestEd(t)
	a := ed.a
	a.ed = ed
	adds := 0
	ok := a.fxLiveOk(func(nf cutFx) { adds++; ed.addFx(nf) })

	f := cutFx{Kind: "text", T: 10, Dur: 3, Text: "a"}
	ok(f)
	f.Text = "ab"
	ok(f)
	f.Text = "abc"
	ok(f)
	if len(ed.fx) != 1 || ed.fx[0].Text != "abc" {
		t.Errorf("three answers left %+v", ed.fx)
	}
	if adds != 1 {
		t.Errorf("the form placed %d effects, want 1", adds)
	}
	if len(ed.undo) != 1 {
		t.Errorf("typing three letters left %d undo step(s), want 1", len(ed.undo))
	}

	// a refusal is not a placement: the caller keeps its own guard (an svg
	// with no file, a caption with no words), and until it lets one through
	// the next answer is a first answer again
	ed.fxLiveOn = nil
	ok = a.fxLiveOk(func(nf cutFx) {
		if nf.Text == "" {
			return
		}
		ed.addFx(nf)
	})
	ok(cutFx{Kind: "text", T: 40})
	if len(ed.fx) != 1 {
		t.Errorf("an empty caption was placed: %+v", ed.fx)
	}
	ok(cutFx{Kind: "text", T: 40, Dur: 2, Text: "hi"})
	if len(ed.fx) != 2 {
		t.Fatalf("the answer after the refusal was not placed: %+v", ed.fx)
	}
	ok(cutFx{Kind: "text", T: 40, Dur: 2, Text: "hi there"})
	if len(ed.fx) != 2 || ed.fx[1].Text != "hi there" {
		t.Errorf("the answer after the placement placed a second effect: %+v", ed.fx)
	}
}

// The wait: a burst of typing is one edit and one file write, and a form taken
// out of the column carries its last keystroke over rather than losing it.
func TestTheLiveFormWaitsOutTheTypingAndFlushesOnTheWayOut(t *testing.T) {
	l := &fxLive{}
	fired := 0
	l.fire = func() { fired++ }
	var armed func() bool
	l.d.arm = func(_ uint, f func() bool) { armed = f }

	l.touch()
	l.touch()
	l.touch()
	if fired != 0 {
		t.Errorf("typing applied %d times before the burst was over", fired)
	}
	armed()
	if fired != 1 {
		t.Errorf("a burst of three keystrokes applied %d times, want 1", fired)
	}
	// what is owed when the panel is taken away is run, not dropped
	l.touch()
	l.d.flush()
	if fired != 2 {
		t.Error("a form closed a moment after the last letter loses it")
	}
	// a form with nothing behind it yet is safe to touch: the rows are built
	// before the form that applies them
	(&fxLive{}).touch()

	// dropForm is where the flush happens, and it lets go of the form's state
	// in the same breath
	body := funcBody(t, "cut_form.go", `func \(ed \*cutEditor\) dropForm\(\) \{`)
	for _, want := range []string{"l.d.flush()", "ed.fxLiveCur, ed.fxLiveOn = nil, nil"} {
		if !strings.Contains(body, want) {
			t.Errorf("dropForm no longer settles the live form: %q", want)
		}
	}
	// and a keystroke that outlived its form cannot land on the one that
	// replaced it
	win := funcBody(t, "cut_fx.go", `func \(a \*App\) fxWin\(`)
	if !strings.Contains(win, "if form.fxLiveCur == live {") {
		t.Error("an older form's last keystroke can still be applied to the form after it")
	}
}

// Every bar wears its handles, not just the one the red line is in.
//
// The ends were drawn as handles on that one bar alone, so a clip you could
// trim looked like a clip you could not until the line was put inside it
// first -- while the picture band drew every kept stretch with both its
// borders and let you take either. Same row, same cut, two answers.
func TestEveryGreenBarWearsItsHandles(t *testing.T) {
	ed := greenEd(t) // clips 20-60 and 100-140; the line is in the second
	if got := ed.bandClipIdx(); got != 1 {
		t.Fatalf("the fixture's reachable bar is %d, want 1", got)
	}
	at := renderTrack(t, ed, 1300, 200)
	midY := int(ed.selBandTop()) + selBandH/2
	for _, x := range []float64{20, 60} {
		r, g, _ := at(int(ed.xOf(x)), midY)
		if g < 200 || r > 180 {
			t.Errorf("the end at %gs of the bar the line is not in reads r=%d g=%d — no handle", x, r, g)
		}
	}
	// ...and the bar in reach is still the brighter of the two, because the
	// arrow keys and the toolbar are about that one
	_, dim, _ := at(int(ed.xOf(40)), midY)
	_, lit, _ := at(int(ed.xOf(120)), midY)
	if dim >= lit {
		t.Errorf("the reachable bar's fill reads %d and the other's %d — the row no longer says which is which", lit, dim)
	}
}

// Nothing on the timeline pops a tooltip over the timeline.
//
// Both bands carried one -- a paragraph about the right button, which has no
// hover state of its own to advertise with. A tooltip over a timeline is a
// paragraph laid across the thing it describes: it arrives on the pause the
// hand makes while reading a waveform or lining a border up by eye, it covers
// the lane's name and the frames and the seconds under it, and it leaves only
// when the pointer moves, which is what you were trying not to do.
//
// What the tracks have to say, they say in themselves: the border under the
// pointer lights up, the badge under it goes red, the cursor changes shape.
// What needs words goes in the status line, which is a row that exists to be
// read and covers nothing.
func TestTheTracksSayNothingOverThemselves(t *testing.T) {
	src := readSrc(t, "cut.go")
	for _, area := range []string{"ed.srcArea", "ed.audArea", "ed.fxArea", "ed.lineArea"} {
		if strings.Contains(src, area+".SetTooltipText(") {
			t.Errorf("%s pops a tooltip over the tracks", area)
		}
	}
	// a drag says what it DID, and only that: "camera 1 −0.42 s". The list of
	// what else the right button can be dragged on used to ride on the end of
	// it -- four clauses, on every drag, about verbs the cursor is already
	// advertising under the hand (wantCursor).
	if !strings.Contains(src, "ed.a.setStatus(shiftLabel(slideWhat, slideD))") {
		t.Error("a drag no longer says what it moved")
	}
	if strings.Contains(src, "right-drag the green to move a scene") {
		t.Error("the status line is a lesson on the right button again")
	}
}
