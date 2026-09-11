package main

// Taking a clip's border with one button.
//
// It used to take two: the right button named the border, and only then would a
// left drag move it. The reason was real -- "drag this border" and "drag a
// region" are the same press over the same pixels, told apart only by landing
// within edgeGrab of a green line -- but the cure was worse than the disease.
// You could see the border under the pointer and still not be able to grab it,
// and no other object on the page works that way: an effect, a selection, the
// ends of either, are all hovered and then pressed.
//
// What makes one button enough is the hover. The border lights up and the
// pointer becomes a resize arrow BEFORE the press, so the hand knows which of
// the two things the press is about to mean. These pin that: what is
// highlighted, where it may be highlighted, what the pointer says, and that a
// single press still cannot walk off with a whole clip -- over the cut a plain
// click is how you put the red line somewhere, and a gesture that both
// navigates and picks things up is a gesture you can use for neither.

import (
	"os"
	"strings"
	"testing"
)

// The offer: within edgeGrab of a border, that border is the highlighted one.
func TestTheBorderUnderThePointerIsTheOneOffered(t *testing.T) {
	ed := moveEd(t)
	ed.segs = []cutSeg{{S: 10, E: 20}, {S: 30, E: 40}}
	y := ed.picTop() + 4

	for _, c := range []struct {
		at   float64
		want float64
		what string
	}{
		{ed.xOf(20) - edgeGrab/2, 20, "just inside the first clip's end"},
		{ed.xOf(30) + edgeGrab/2, 30, "just inside the second clip's start"},
		{ed.xOf(40), 40, "on the second clip's end"},
	} {
		ed.hoverTracks(c.at-ed.viewX, y)
		if !ed.edgeHovOn || ed.edgeHovT != c.want {
			t.Errorf("hovering %s offered on=%v t=%.2f, want the border at %.2f",
				c.what, ed.edgeHovOn, ed.edgeHovT, c.want)
		}
	}

	// and the middle of a clip offers nothing: a press there is a press on the
	// timeline, which is how the red line gets where it is going
	ed.hoverTracks(ed.xOf(15)-ed.viewX, y)
	if ed.edgeHovOn {
		t.Errorf("the middle of a clip offered the border at %.2f", ed.edgeHovT)
	}
	// nor does the pointer leaving keep the last one lit
	ed.hoverTracks(ed.xOf(20)-ed.viewX, y)
	ed.hoverTracks(-1, -1)
	if ed.edgeHovOn {
		t.Error("the pointer left the page and the border stayed lit")
	}
}

// Only over the pictures. The two rows above and the lane below are their own
// objects with their own handles, and a border highlighted while the pointer is
// over the selection row would be an offer the press there does not honour.
func TestOnlyThePictureBandOffersABorder(t *testing.T) {
	ed := moveEd(t)
	ed.segs = []cutSeg{{S: 10, E: 20}}
	at := ed.xOf(20) - ed.viewX

	for _, c := range []struct {
		y    float64
		want bool
		what string
	}{
		{2, false, "the ruler"},
		{ed.selBandTop() + 4, false, "the selection row"},
		{ed.picTop() + 4, true, "the pictures"},
		{ed.picTop() + float64(ed.thumbHt) - 2, true, "the bottom of the pictures"},
		{ed.fxLaneTop() + 4, false, "the effects lane"},
	} {
		ed.hoverTracks(at, c.y)
		if ed.edgeHovOn != c.want {
			t.Errorf("hovering %s (y=%g) offered a border: %v, want %v",
				c.what, c.y, ed.edgeHovOn, c.want)
		}
	}

	// the lanes are the same timeline seen as sound, and the cut runs through
	// them, so a border is offered anywhere in them
	ed.hoverLanes(at, 4)
	if !ed.edgeHovOn {
		t.Error("the waveform lanes did not offer the border under the pointer — " +
			"the cut points are drawn through them and can be trimmed there")
	}
}

// The pointer says which of the two meanings the press has. This is the half
// that cannot be drawn on the band: a resize arrow over a border and nothing
// over the footage beside it is the difference between a handle and a hazard.
func TestThePointerSaysWhetherAPressWouldTrim(t *testing.T) {
	ed := moveEd(t)
	ed.segs = []cutSeg{{S: 10, E: 20}}
	y := ed.picTop() + 4

	if got := ed.wantCursor(ed.xOf(20)-ed.viewX, y); got != "ew-resize" {
		t.Errorf("over a border the pointer is %q, want %q", got, "ew-resize")
	}
	if got := ed.wantCursor(ed.xOf(15)-ed.viewX, y); got != "" {
		t.Errorf("over the middle of a clip the pointer is %q, want the page's own", got)
	}
	// the lanes answer the same way, through their own handler: down there
	// every row is the same row, and a border is a border wherever in the
	// sound the pointer is
	ed.hoverLanes(ed.xOf(20)-ed.viewX, 4)
	if !ed.edgeHovOn || ed.edgeHovT != 20 {
		t.Errorf("the lanes offered on=%v t=%.2f over the border at 20", ed.edgeHovOn, ed.edgeHovT)
	}
	ed.hoverLanes(ed.xOf(15)-ed.viewX, 4)
	if ed.edgeHovOn {
		t.Errorf("the sound between borders offered the border at %.2f", ed.edgeHovT)
	}
}

// The rule that keeps the single press usable: it may take a border, and it may
// not take a clip. Clicking the timeline over the cut has always been how you
// put the red line somewhere, and there is nowhere else to do it from.
func TestASinglePressTakesABorderButNeverAWholeClip(t *testing.T) {
	ed := moveEd(t)
	ed.segs = []cutSeg{{S: 10, E: 20}, {S: 30, E: 40}}

	if got := ed.pickAt(ed.xOf(20)-edgeGrab/2, false); got != pickEdge {
		t.Errorf("a press beside a border took %d, want the border", got)
	}
	if !ed.edgeOn || ed.edgeSeg != 0 || !ed.edgeEnd {
		t.Errorf("the border in hand is on=%v seg=%d end=%v, want the first clip's end",
			ed.edgeOn, ed.edgeSeg, ed.edgeEnd)
	}
	ed.dropEdge()

	if got := ed.pickAt(ed.xOf(35), false); got != pickNone {
		t.Errorf("a single press on the middle of a clip took %d, want nothing — that "+
			"press is the one that moves the red line", got)
	}
	if ed.heldSeg() != nil {
		t.Error("a single press walked off with a whole clip")
	}
	// the second click of a double click is the one that may
	if got := ed.pickAt(ed.xOf(35), true); got != pickSeg {
		t.Errorf("the second click on a clip took %d, want the clip", got)
	}
	if s := ed.heldSeg(); s == nil || s.S != 30 {
		t.Errorf("the held clip is %v, want the one starting at 30", s)
	}
}

// A held border is offered back with a wider tolerance than a fresh one, and
// before any other border: by then you are aiming at a white bar you can see,
// and it may sit a few px from the border of the clip next door.
func TestTheHeldBorderIsOfferedFirstAndWider(t *testing.T) {
	ed := moveEd(t)
	ed.segs = []cutSeg{{S: 10, E: 20}, {S: 30, E: 40}}
	ed.grabEdge(ed.xOf(20))
	if !ed.edgeOn || ed.edgeEnd != true {
		t.Fatalf("the first clip's end did not come up: on=%v end=%v", ed.edgeOn, ed.edgeEnd)
	}

	// a press edgeMove/2 away is nowhere near edgeGrab, and still takes it
	at := ed.xOf(20) + edgeMove/2
	if got := ed.pickAt(at, false); got != pickEdge {
		t.Errorf("a press %g px from the held bar took %d, want the bar", edgeMove/2, got)
	}
	if ed.edgeSeg != 0 || !ed.edgeEnd {
		t.Errorf("the press swapped the held border for seg=%d end=%v", ed.edgeSeg, ed.edgeEnd)
	}
}

// The highlight is ink, not just state: the border under the pointer is drawn
// pale over the green one, so a column that was green becomes a column that is
// mostly white.
func TestTheOfferedBorderIsDrawn(t *testing.T) {
	ed := moveEd(t)
	ed.segs = []cutSeg{{S: 10, E: 40}}
	ed.thumbs = nil

	white := func() int {
		at := renderTrack(t, ed, 400, int(ed.picTop())+ed.thumbHt+8+int(ed.fxLaneHeight()))
		x := int(ed.xOf(40))
		y := int(ed.picTop()) + 6
		n := 0
		for dx := -1; dx <= 1; dx++ {
			if r, g, b := at(x+dx, y); r > 180 && g > 180 && b > 180 {
				n++
			}
		}
		return n
	}

	if n := white(); n != 0 {
		t.Errorf("%d px of the border are white with nothing hovered — the highlight "+
			"is an offer, and an offer nobody made should not be on screen", n)
	}
	ed.hoverTracks(ed.xOf(40)-ed.viewX, ed.picTop()+4)
	if n := white(); n < 2 {
		t.Errorf("the hovered border came out %d px of white, want the green line "+
			"covered — a tolerance you cannot see is a tolerance you cannot aim at", n)
	}
}

// Choosing is not moving, and a press that only chose must not cue the picture:
// the release that follows it has nothing to show.
func TestAPressThatOnlyTookABorderLeavesTheRedLine(t *testing.T) {
	ed := moveEd(t)
	ed.segs = []cutSeg{{S: 10, E: 20}}
	ed.playhead, ed.hasPlay = 5, true
	ed.pickAt(ed.xOf(20), false)
	if ed.playhead != 5 {
		t.Errorf("taking a border moved the red line to %.2f", ed.playhead)
	}
	if ed.edgeDirty {
		t.Error("taking a border marked the cut dirty — nothing has moved yet")
	}
}

func TestTheHoverIsWired(t *testing.T) {
	b, err := os.ReadFile("cut.go")
	if err != nil {
		t.Fatal(err)
	}
	src := string(b)
	for _, want := range []string{
		// hover on both bands, and the pointer follows it
		"hover.ConnectMotion(func(x, y float64) { ed.hoverTracks(x, y) })",
		"hover.ConnectMotion(func(x, y float64) { ed.hoverLanes(x, y) })",
		// the right press takes the border, the drag trims it
		"if ed.onHeldEdge(px) || ed.grabEdge(px) {",
		"ed.moveEdgeTo(ed.tAtView(slideX0+ox), true)",
		// the second click is the only way to a whole clip
		"pick.SetButton(gdk.BUTTON_PRIMARY)",
		"if n < 2 {",
		"ed.pickAt(x+ed.viewX, true)",
	} {
		if !strings.Contains(src, want) {
			t.Errorf("the cut page no longer contains %q", want)
		}
	}
	sb, err := os.ReadFile("cut_selband.go")
	if err != nil {
		t.Fatal(err)
	}
	for _, want := range []string{
		"func (ed *cutEditor) hoverEdge(x float64, cut bool) {",
		"ed.hoverEdge(x, x >= 0 && ed.hitPics(y))", // the pictures, and no other row
		"ed.setCursor(ed.srcArea, ed.wantCursor(x, y))",
		"ed.hoverEdge(x, x >= 0)", // the lanes, all of which are the cut
	} {
		if !strings.Contains(string(sb), want) {
			t.Errorf("the selection band file no longer contains %q", want)
		}
	}
}
