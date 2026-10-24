package main

// Which camera a scene is shown from.
//
// cutSeg.Cam has always been in the file and nothing on the page could change
// it: a scene took the row the hand was dragging on when ＋ Add was pressed,
// and that was that. These pin the control that fixes it -- the same column of
// marks the lanes wear, one per row, lit on the row in use -- and the one rule
// that makes it a different control from the sound's: a scene has one picture,
// so the badges are a radio and none of them can be pressed off.

import (
	"strings"
	"testing"
)

// camEd is two cameras with a held scene both of them were rolling for.
func camEd(t *testing.T) *cutEditor {
	t.Helper()
	ed := pairEd(t) // "a" on row 0 for 0-100, "b" on row 1 for 10-90
	ed.segs = []cutSeg{{S: 20, E: 70}}
	ed.segOn, ed.segSel = true, 0
	return ed
}

func TestEveryRowThatCouldShowTheSceneWearsALens(t *testing.T) {
	ed := camEd(t)
	b := ed.camBadges()
	if len(b) != 2 || b[0].row != 0 || b[1].row != 1 {
		t.Fatalf("the badges are %+v, want one per camera row", b)
	}
	if !b[0].on || b[1].on {
		t.Errorf("the scene is shown from row %d and the lit badge is %+v", ed.segs[0].Cam, b)
	}
	// they stand in the sound's column, at the scene's left edge
	cx, _ := ed.hearX()
	if b[0].cx != cx || b[1].cx != cx {
		t.Errorf("the lenses are at %g/%g, the speakers at %g -- not one column",
			b[0].cx, b[1].cx, cx)
	}
	// ...and each on its own row's pictures
	for _, x := range b {
		if x.cy <= ed.laneTop(x.row) || x.cy >= ed.laneTop(x.row)+ed.laneH() {
			t.Errorf("row %d's lens sits at %g, off its own pictures", x.row, x.cy)
		}
	}
}

// A camera that was not rolling is not an option: the render falls back to
// another row rather than show a hole, so the press would appear to work and
// change nothing you could see.
func TestARowThatFilmedNothingHereWearsNoLens(t *testing.T) {
	ed := camEd(t)
	ed.segs = []cutSeg{{S: 2, E: 8}} // before "b" started, and under hearMin too
	ed.segs = []cutSeg{{S: 2, E: 60}}
	if b := ed.camBadges(); len(b) != 0 {
		t.Errorf("a scene starting before the second camera rolled got %+v", b)
	}
	// one row is not a choice either
	ed.segs = []cutSeg{{S: 20, E: 70}}
	ed.laneN = 1
	if b := ed.camBadges(); len(b) != 0 {
		t.Errorf("a one-camera session drew a camera choice: %+v", b)
	}
	// nor is a scene with nothing in hand and no line, or an insert
	ed.laneN = 2
	ed.segOn, ed.hasPlay = false, false
	if b := ed.camBadges(); len(b) != 0 {
		t.Errorf("badges with no scene in hand: %+v", b)
	}
	ed.segs = []cutSeg{{S: 20, E: 70, Ins: "card.png"}}
	ed.segOn, ed.segSel = true, 0
	if b := ed.camBadges(); len(b) != 0 {
		t.Errorf("a card wears a camera choice: %+v", b)
	}
}

// The press moves the picture and nothing else, once, undoably.
func TestPressingALensShowsTheSceneFromThatRow(t *testing.T) {
	ed := camEd(t)
	ed.segs[0].Quiet = []string{"mic"}
	b := ed.camBadges()

	if got := ed.camBadgeAt(b[1].cx, b[1].cy); got != 1 {
		t.Fatalf("the press on row 1's lens answered %d", got)
	}
	ed.setSegCam(1)
	if ed.segs[0].Cam != 1 {
		t.Fatalf("the scene is still shown from row %d", ed.segs[0].Cam)
	}
	// the seconds and the sound are untouched: this is one field
	if ed.segs[0].S != 20 || ed.segs[0].E != 70 || len(ed.segs[0].Quiet) != 1 {
		t.Errorf("the switch changed more than the camera: %+v", ed.segs[0])
	}
	if len(ed.undo) != 1 {
		t.Errorf("the switch left %d undo step(s), want 1", len(ed.undo))
	}
	ed.undoLast()
	if ed.segs[0].Cam != 0 {
		t.Error("↶ did not put the camera back")
	}

	// a radio, not a toggle: pressing the row it is already on is not an edit.
	// Undo rebuilt the list, so the scene is taken in hand again -- the badges
	// are the held scene's and there is no held scene after an undo.
	ed.segOn, ed.segSel = true, 0
	ed.undo = nil
	ed.setSegCam(0)
	if len(ed.undo) != 0 || ed.segs[0].Cam != 0 {
		t.Errorf("pressing the lit lens left %d undo step(s) and cam %d",
			len(ed.undo), ed.segs[0].Cam)
	}
	// and there is no "off": nothing in the badge column can leave a scene
	// without a picture
	if got := ed.camBadgeAt(b[0].cx, b[0].cy); got != 0 {
		t.Errorf("the lit lens answers %d, so it cannot be the one that is on", got)
	}
}

// It is drawn over the pictures with the sound's badges, and the press is asked
// where theirs is -- while a scene is in hand, a press on one of its marks is
// the only thing that press can mean.
func TestTheCameraBadgesAreWired(t *testing.T) {
	src := readSrc(t, "cut.go")
	for _, want := range []string{
		"ed.drawCamBadges(cr, vx0, vx1)",
		"if r := ed.camBadgeAt(x+ed.viewX, y); r >= 0 {",
		"ed.setSegCam(r)",
	} {
		if !strings.Contains(src, want) {
			t.Errorf("cut.go no longer contains %q", want)
		}
	}
	// drawn before the sound's, so the two read as one column top to bottom
	i := strings.Index(src, "ed.drawCamBadges(cr, vx0, vx1)")
	j := strings.Index(src, "ed.drawHearBadges(cr, ed.hearBadgesSrc(), vx0, vx1)")
	if i < 0 || j < 0 || i > j {
		t.Error("the camera badges are drawn after the sound's")
	}
}

// In pixels: the row in use wears a lit plate, the row it could move to a dark
// one, and the row it could move to says so with a wash the eye can find at any
// zoom -- the same wash a silenced lane wears.
func TestTheLensesAreDrawnLitAndUnlit(t *testing.T) {
	ed := camEd(t)
	b := ed.camBadges()
	at := renderTrack(t, ed, 620, 300)
	// on the PLATE, clear of what is drawn on it: the ring and, on the row in
	// use, the filled dot inside it are white either way, so the centre pixel
	// says nothing about which of the two this is (drawLens)
	const onPlate = 6 // plate radius is hearR+hearPad = 8; the ring sits at ~3.7
	lr, lg, lb := at(int(b[0].cx+onPlate), int(b[0].cy))
	dr, dg, db := at(int(b[1].cx+onPlate), int(b[1].cy))
	if int(lg) < int(lr)+40 || int(lg) < int(lb)+40 {
		t.Errorf("the row in use reads rgb(%d,%d,%d) — not a lit plate", lr, lg, lb)
	}
	if dr > 90 || dg > 90 || db > 90 {
		t.Errorf("the row it could move to reads rgb(%d,%d,%d) — not a dark plate", dr, dg, db)
	}
}
