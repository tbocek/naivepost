package main

// A click on the green takes that scene in hand.
//
// The kept clip is drawn twice: as the green over the thumbnails, and as the
// green bar in the selection band. One click on the bar picked it up; over the
// thumbnails it took two, because a single press there was the seek and taking
// a clip was deliberately behind a double click. The drawn scene is much the
// bigger target and the one the hand goes to first, so it now answers the same
// single click -- and the seek it shares that click with still happens.

import (
	"os"
	"strings"
	"testing"
)

// greenClickEd is two kept scenes on two rows: 20-60 on camera 0 and 100-140 on
// camera 1. Two rows, because the scene at a second and the row under the
// pointer are different questions and this is where they part.
func greenClickEd(t *testing.T) *cutEditor {
	t.Helper()
	ed := newTestEd(t) // pps 4: a session second is four px
	ed.vids = []tlVideo{
		{base: "a", path: "a.mkv", start: 0, dur: 300, interval: 5, fps: 30, lane: 0},
		{base: "b", path: "b.mkv", start: 0, dur: 300, interval: 5, fps: 30, lane: 1},
	}
	ed.laneN = 2
	ed.relayout()
	ed.segs = []cutSeg{{S: 20, E: 60, Cam: 0}, {S: 100, E: 140, Cam: 1}}
	return ed
}

// rowY is a y inside row i's pictures.
func (ed *cutEditor) rowY(i int) float64 { return ed.laneTop(i) + ed.laneH()/2 }

func TestTheGreenUnderTheHandIsTheOneItIsDrawnOn(t *testing.T) {
	ed := greenClickEd(t)
	for _, tc := range []struct {
		name string
		t    float64
		row  int
		want int
	}{
		{"on the first scene, on its own row", 40, 0, 0},
		{"on the second scene, on its own row", 120, 1, 1},
		// the cut keeps camera 1 here and the eye is on camera 0: what is
		// under the pointer is plain footage, and pressing it must not take
		// a scene that is drawn a row away
		{"on the second scene's second, a row above it", 120, 0, -1},
		{"on the first scene's second, a row below it", 40, 1, -1},
		// between the two scenes: a dropped stretch on either row
		{"in a dropped stretch", 80, 0, -1},
		{"past the last scene", 200, 1, -1},
	} {
		got := ed.segOnGreen(ed.xOf(tc.t), ed.rowY(tc.row))
		if got != tc.want {
			t.Errorf("%s: the press lands on scene %d, want %d", tc.name, got, tc.want)
		}
	}

	// and off the band entirely there is no scene to press: the ruler, the
	// selection row and the effects lane are their own objects
	if got := ed.segOnGreen(ed.xOf(40), 0); got >= 0 {
		t.Errorf("a press on the ruler took scene %d", got)
	}
	if got := ed.segOnGreen(ed.xOf(40), ed.picBottom()+10); got >= 0 {
		t.Errorf("a press below the pictures took scene %d", got)
	}
}

// The scene picked up is the one the green says it is, and picking it up is
// the ordinary hold: the same state the double click and the band's bar reach,
// so the badges, ⌦, the arrows and right-drag all follow it.
func TestClickingTheGreenIsHoldingTheClip(t *testing.T) {
	ed := greenClickEd(t)
	if ed.segOn {
		t.Fatal("a scene is in hand before anything was pressed")
	}
	px := ed.xOf(120)
	if ed.segOnGreen(px, ed.rowY(1)) != 1 {
		t.Fatal("the fixture does not draw scene 2 where this test presses")
	}
	if !ed.grabSeg(px) {
		t.Fatal("pressing the green did not pick the scene up")
	}
	if !ed.segOn || ed.segSel != 1 {
		t.Errorf("the scene in hand is %d (on=%v), want 1", ed.segSel, ed.segOn)
	}
	// one thing is held at a time: taking a scene lets go of a border and an
	// effect, which is what makes ⌦ and the arrows unambiguous
	if ed.edgeOn || ed.fxOn {
		t.Error("taking a scene left a border or an effect in hand as well")
	}
	// and a press clear of the green takes nothing, so the drop at the press
	// is what that click meant
	if ed.grabSeg(ed.xOf(80)) {
		t.Error("a press in a dropped stretch picked a scene up")
	}
}

func TestTheClickThatTakesASceneIsAlsoStillTheSeek(t *testing.T) {
	src, err := os.ReadFile("cut.go")
	if err != nil {
		t.Fatal(err)
	}
	body := funcBody(t, "cut.go", `drag\.ConnectDragEnd\(func\(ox, oy float64\) \{`)

	// the whole of it lives in the branch a press WITHOUT movement takes, so a
	// real drag still draws a selection over the footage and a press on a scene
	// already in hand still moves it (the moving branch, above and earlier)
	if !strings.Contains(body, "if math.Abs(ox) >= 5 || math.Abs(oy) >= 5 {\n\t\t\t\treturn") {
		t.Fatal("the click branch no longer starts by letting a real drag through")
	}
	for _, pin := range []string{
		"ed.setPlayhead(ed.tAtView(dragStartX))",
		"if px := dragStartX + ed.viewX; ed.segOnGreen(px, dragStartY) >= 0 {",
		"ed.grabSeg(px)",
	} {
		if !strings.Contains(body, pin) {
			t.Errorf("the click on the footage lost %q", pin)
		}
	}
	// the seek comes first: a scene picked up before the line moved would
	// report the numbers, then be overwritten by monStatus
	if i, j := strings.Index(body, "ed.monStatus()"), strings.Index(body, "ed.grabSeg(px)"); i < 0 || j < 0 || i > j {
		t.Error("the scene is taken before the line moves, so its status line does not survive")
	}
	// only on the pictures: the recorders' band below has no scenes drawn on
	// it, and the band and the lanes have their own gestures
	if !strings.Contains(body, "if area == ed.srcArea && ed.hitPics(dragStartY) {") {
		t.Error("a click anywhere at all now takes a scene")
	}
	// the press still puts down whatever was held, so a click clear of the
	// green is a drop and not a no-op
	if !strings.Contains(string(src), "ed.dropEdge() // any other left click puts a held edge or clip down") {
		t.Error("a click clear of the green no longer puts the held scene down")
	}
}
