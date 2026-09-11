package main

// A row nobody filmed.
//
// Everything else on the picture band arrived by being a source: Prepare
// was pointed at a file and the row is that file. A cut lane is the other kind
// -- a shot copied from elsewhere in the session, or a video that was never a
// source at all, given a band of its own so the green can cut to it. It lives
// in cut.json and nowhere else.
//
// Three things have to be true of one, and everything here is one of the three.
// It has to be a ROW, with its own number that the cameras beside it do not
// lose. It has to be a WINDOW on a file, so a scene cut to it plays the seconds
// that were meant and not the file's first seconds. And it has to be an EDIT --
// saved, undone, and taken away again -- because a row that could only be
// removed by editing cut.json by hand is a row nobody should be able to add.

import (
	"encoding/json"
	"os"
	"strings"
	"testing"
)

// rowEd is one camera, a hundred seconds long, and nothing else.
func rowEd(t *testing.T) *cutEditor {
	t.Helper()
	ed := newTestEd(t)
	ed.vids = []tlVideo{{base: "a", path: "/f/a.mp4", start: 0, dur: 100, wall: 1000,
		interval: 1, fps: 30, frames: make([]string, 100)}}
	ed.auds = []tlAudio{{base: "a", path: "/f/a.mp4", start: 0, dur: 100, chans: 2, master: true}}
	ed.a.ed = ed // pasteLane reaches back through the App
	ed.relayout()
	return ed
}

// ---- a row of its own -------------------------------------------------------

func TestALaneGetsItsOwnRowAndTheCameraKeepsHers(t *testing.T) {
	ed := rowEd(t)
	name := ed.addLane("/f/a.mp4", 20, 60, 10)
	if name == "" {
		t.Fatal("the lane was not made")
	}
	if ed.laneN != 2 {
		t.Fatalf("the band came out on %d row(s), want 2", ed.laneN)
	}
	// the camera is still row nought, which is what every scene already
	// written says. A new row that pushed her down one would have repointed
	// the whole cut without touching a single scene
	if v := videoOn(ed.vids, 0, 50); v == nil || v.base != "a" {
		t.Errorf("row 0 is %v, want the camera", v)
	}
	if v := videoOn(ed.vids, 1, 65); v == nil || v.base != name {
		t.Errorf("row 1 is %v, want the new lane", v)
	}
	// ...and it is pinned there, not worked out afresh: the lane overlaps
	// nothing at 60-70 that a colouring pass would have to keep it clear of
	if ed.rows[name] != 1 {
		t.Errorf("the lane is pinned to row %d, want 1", ed.rows[name])
	}
	if ed.rows["a"] != 0 {
		t.Errorf("the camera is pinned to row %d, want 0", ed.rows["a"])
	}
}

func TestASecondLaneGoesUnderTheFirst(t *testing.T) {
	ed := rowEd(t)
	one := ed.addLane("/f/a.mp4", 0, 10, 10)
	two := ed.addLane("/f/a.mp4", 30, 40, 10)
	if ed.laneN != 3 {
		t.Fatalf("two lanes made %d row(s), want 3", ed.laneN)
	}
	if ed.rows[one] != 1 || ed.rows[two] != 2 {
		t.Errorf("the lanes landed on rows %d and %d, want 1 and 2", ed.rows[one], ed.rows[two])
	}
	// and they are told apart, because the row pins and the corrections are
	// both held under the name: two rows sharing one would move together
	if one == two {
		t.Errorf("both lanes are called %q", one)
	}
}

// ---- a window on a file -----------------------------------------------------

func TestALaneShowsTheSecondsItWasCutFrom(t *testing.T) {
	ed := rowEd(t)
	// twenty seconds in, played back at session second sixty
	name := ed.addLane("/f/a.mp4", 20, 60, 10)
	v := videoOn(ed.vids, 1, 65)
	if v == nil {
		t.Fatalf("%s is not on row 1", name)
	}
	// the whole of what a lane is: the row starts at 60 and shows second 20
	for _, c := range []struct{ t, want float64 }{{60, 20}, {65, 25}, {69.5, 29.5}} {
		if got := v.at(c.t); got != c.want {
			t.Errorf("at %g s the lane plays second %g of the file, want %g", c.t, got, c.want)
		}
	}
	// and back the other way, which is what the thumbnails and the camera
	// rectangle read
	if got := v.sessionAt(25); got != 65 {
		t.Errorf("second 25 of the file is on screen at %g s, want 65", got)
	}
	// a recording is the same sum with the window wide open
	if got := ed.vids[0].at(40); got != 40 {
		t.Errorf("a camera at 40 s plays second %g of its own file", got)
	}
}

func TestALaneBorrowsTheRecordingsThumbnailsAndOnlyItsOwn(t *testing.T) {
	ed := rowEd(t)
	// a source has its frames extracted once, and a lane cut from it is that
	// same recording seen twice -- there is nothing to extract again
	name := ed.addLane("/f/a.mp4", 20, 60, 10)
	v := videoOn(ed.vids, 1, 65)
	if v == nil || len(v.frames) != 100 {
		t.Fatalf("%s has %v frames, want the recording's own", name, v)
	}
	// ...but only the ten seconds it shows. The rest of the folder is the
	// camera's row, and painting it here would draw the whole recording twice
	ed.relayout()
	first, last := v.frameRange(ed.pps, -1e6, 1e6, 1)
	if first != 20 || last != 31 {
		t.Errorf("the lane paints frames [%d,%d), want [20,31)", first, last)
	}
	// and a whole recording still paints all of itself
	if f, l := ed.vids[0].frameRange(ed.pps, -1e6, 1e6, 1); f != 0 || l != 100 {
		t.Errorf("the camera paints frames [%d,%d), want all 100", f, l)
	}
}

// A lane's picture starts where the lane does. The thumbnails are drawn every
// step'th frame and the step is a whole thumbnail wide, and the stride used to
// be counted from the FILE's frame nought -- which is the row's own start for a
// camera and is not for a lane, a window opening partway into a file. The first
// stride to land inside the window could be most of a thumbnail past the row's
// left edge, so the row opened with a bite of bare band and the picture did not
// begin where the yellow line said the lane did.
func TestALanesPictureBeginsWhereTheLaneDoes(t *testing.T) {
	ed := rowEd(t)
	ed.addLane("/f/a.mp4", 37, 60, 20) // an odd second, so no stride lands on it
	v := videoOn(ed.vids, 1, 65)
	if v == nil {
		t.Fatal("no lane")
	}
	ed.relayout()
	first, _ := v.frameRange(ed.pps, -1e6, 1e6, v.thumbStep(64, ed.pps))
	if got := v.sessionAt(float64(first) * v.interval); got != v.start {
		t.Errorf("the first thumbnail is drawn at %g s and the lane starts at %g s",
			got, v.start)
	}
}

// ...and a SHORT lane has a picture at all. The lane button makes one out of a
// copy, and
// a copy is a few seconds: measured from the file's frame nought there was no
// stride inside such a window whatsoever, so the row came out empty -- a lane
// that was there, correctly placed, and showed nothing.
func TestAShortLaneIsNotAnEmptyRow(t *testing.T) {
	ed := rowEd(t)
	// frames every two seconds, so a four-second window holds two of them and
	// the stride between thumbnails is wider than the whole window
	ed.vids[0].interval, ed.vids[0].frames = 2, make([]string, 50)
	ed.addLane("/f/a.mp4", 60, 10, 4)
	v := videoOn(ed.vids, 1, 12)
	if v == nil {
		t.Fatal("no lane")
	}
	ed.relayout()
	step := v.thumbStep(64, ed.pps)
	first, last := v.frameRange(ed.pps, -1e6, 1e6, step)
	if last <= first {
		t.Fatalf("a %g s lane paints frames [%d,%d) with a stride of %d — nothing at all",
			v.dur, first, last, step)
	}
	if got := v.sessionAt(float64(first) * v.interval); got != v.start {
		t.Errorf("its one thumbnail is at %g s, and the lane starts at %g s", got, v.start)
	}
}

// A row with nothing to walk is not a crash. An editor built for a test has
// videos with no frames and no interval between them, and the arithmetic here
// divides by that interval.
func TestARowWithNoFramesPaintsNothing(t *testing.T) {
	v := &tlVideo{start: 0, dur: 60} // no frames, no interval
	if first, last := v.frameRange(4, -1e6, 1e6, 1); first != 0 || last != 0 {
		t.Errorf("a row with no frames paints [%d,%d)", first, last)
	}
	// nor is a stride of nought, which is what a thumbnail wider than the whole
	// timeline would ask for if thumbStep did not floor it
	full := &tlVideo{interval: 1, frames: make([]string, 10), dur: 10}
	if first, last := full.frameRange(4, -1e6, 1e6, 0); first != 0 || last != 0 {
		t.Errorf("a stride of nought paints [%d,%d)", first, last)
	}
}

// Footage is picture and the sound filmed with it in one piece, so a row of
// pictures comes with a row of sound. A lane without one was a row half of
// whose content the page would not show: the shot played its own audio in the
// preview and in the render, and the only place it did not exist was the page
// where the cutting is done -- nothing to see it against, nothing to scope a
// selection to, nothing to copy out of.
func TestALaneBringsItsSoundWithIt(t *testing.T) {
	ed := rowEd(t)
	name := ed.addLane("/f/a.mp4", 20, 60, 10)
	au := ed.audByBase(name)
	if au == nil {
		t.Fatalf("the sound band came out with %d lane(s) and none of them is %s", len(ed.auds), name)
	}
	// the same window its pictures are, and its own track rather than a
	// recording of its own
	if au.start != 60 || au.off != 20 || au.dur != 10 || !au.master {
		t.Errorf("%s sounds from %g s, file second %g, for %g s (master %v), want 60/20/10/true",
			name, au.start, au.off, au.dur, au.master)
	}
	// how many channels is the source's answer: a copied shot is one recording
	// seen twice and the probe was paid for the first time round
	if au.chans != ed.auds[0].chans {
		t.Errorf("%s came out with %d channels and the recording has %d",
			name, au.chans, ed.auds[0].chans)
	}
	if !strings.Contains(funcBody(t, "cut_lane.go", "func laneAudios"), "au.path == v.path") {
		t.Error("laneAudios probes for a channel count the source's lane already knows")
	}
}

// And it is that window when asked what second of the file is heard: the lane
// starts at second 20 of the file, so its first second is 20 and not nought.
func TestALanesSoundIsTheSecondsItShows(t *testing.T) {
	ed := rowEd(t)
	name := ed.addLane("/f/a.mp4", 20, 60, 10)
	au := ed.audByBase(name)
	if au == nil {
		t.Fatal("no lane")
	}
	for _, c := range []struct{ at, want float64 }{{60, 20}, {65, 25}, {70, 30}} {
		if got := au.at(c.at); got != c.want {
			t.Errorf("%s at %g s plays file second %g, want %g", name, c.at, got, c.want)
		}
	}
	// a recording is the same sum with the window wide open
	if got := ed.auds[0].at(40); got != 40 {
		t.Errorf("a recording at 40 s plays its own second %g", got)
	}
}

// A row taken away takes its sound with it, and an undo brings both back. Both
// are setLanes rebuilding the band, which is the whole reason it rebuilds.
func TestALaneTakenAwayTakesItsSoundWithIt(t *testing.T) {
	ed := rowEd(t)
	name := ed.addLane("/f/a.mp4", 20, 60, 10)
	if len(ed.auds) != 2 {
		t.Fatalf("the sound band has %d lanes, want the recording and the new row", len(ed.auds))
	}
	ed.killLane(name)
	if len(ed.auds) != 1 || ed.audByBase(name) != nil {
		t.Errorf("%s is gone from the pictures and its sound is still there: %d lanes",
			name, len(ed.auds))
	}
	ed.undoLast()
	if ed.audByBase(name) == nil {
		t.Errorf("undo brought %s's pictures back without its sound", name)
	}
}

// And a row right-dragged in time takes its sound along, because a waveform
// that stayed where it was would be the page saying the shot is heard at
// seconds it is no longer seen at.
func TestALaneDraggedInTimeTakesItsSoundWithIt(t *testing.T) {
	ed := rowEd(t)
	name := ed.addLane("/f/a.mp4", 20, 60, 10)
	ed.setShift(name, 12)
	au, v := ed.audByBase(name), videoOn(ed.vids, 1, 72)
	if au == nil || v == nil {
		t.Fatalf("%s is not on the band after a shift", name)
	}
	if au.start != v.start || au.off != v.off {
		t.Errorf("%s is seen from %g s (file %g) and heard from %g s (file %g)",
			name, v.start, v.off, au.start, au.off)
	}
}

// ---- an edit like any other -------------------------------------------------

func TestALaneIsSavedAndComesBack(t *testing.T) {
	ed := rowEd(t)
	name := ed.addLane("/f/a.mp4", 20, 60, 10)
	b, err := os.ReadFile(ed.a.cutPath())
	if err != nil {
		t.Fatal(err)
	}
	var c cutFile
	if err := json.Unmarshal(b, &c); err != nil {
		t.Fatal(err)
	}
	if len(c.Lanes) != 1 {
		t.Fatalf("cut.json holds %d lane(s): %s", len(c.Lanes), b)
	}
	if l := c.Lanes[0]; l.Name != name || l.At != 60 || l.Off != 20 || l.Dur != 10 {
		t.Errorf("the lane was written as %+v", l)
	}
	// and the row it is on, which is the number every scene cut to it carries
	if c.Rows[name] != 1 {
		t.Errorf("the pinned rows came out as %v", c.Rows)
	}
	// what comes back is a row on the band again, in the right place
	back := rowEd(t)
	back.a.outDir = ed.a.outDir
	back.setLanes(c.Lanes)
	back.rows = c.Rows
	back.relayout()
	if v := videoOn(back.vids, 1, 65); v == nil || v.at(65) != 25 {
		t.Errorf("the reloaded lane is %v", v)
	}
}

func TestUndoTakesALaneBackOffTheBand(t *testing.T) {
	ed := rowEd(t)
	name := ed.addLane("/f/a.mp4", 20, 60, 10)
	if ed.laneN != 2 {
		t.Fatalf("the lane was not made")
	}
	ed.undoLast()
	if len(ed.cutLanes) != 0 {
		t.Errorf("undo left %d lane(s) behind", len(ed.cutLanes))
	}
	if videoOn(ed.vids, 1, 65) != nil {
		t.Error("undo left the row on the band")
	}
	if _, ok := ed.rows[name]; ok {
		t.Error("undo left the row pinned")
	}
	// and it comes back, because a lane is an edit and redo is the other half
	ed.redoLast()
	if len(ed.cutLanes) != 1 || videoOn(ed.vids, 1, 65) == nil {
		t.Errorf("redo did not put the lane back: %v", ed.cutLanes)
	}
}

func TestRemovingALaneTakesTheScenesCutToItAndBringsTheRowsUp(t *testing.T) {
	ed := rowEd(t)
	one := ed.addLane("/f/a.mp4", 0, 10, 20) // row 1
	ed.addLane("/f/a.mp4", 50, 50, 20)       // row 2
	ed.segs = []cutSeg{{S: 2, E: 6}, {S: 12, E: 18, Cam: 1}, {S: 55, E: 60, Cam: 2}}
	ed.killLane(one)

	if len(ed.cutLanes) != 1 {
		t.Fatalf("%d lane(s) left, want 1", len(ed.cutLanes))
	}
	// the scene cut to it goes with it: a scene left pointing at a row that is
	// no longer there falls back to whatever was rolling, which is a different
	// picture in the render for a reason nobody could see on this page
	if len(ed.segs) != 2 {
		t.Fatalf("the cut kept %v", ed.segs)
	}
	// ...and the row below comes up one, with its scene
	if ed.segs[1].Cam != 1 {
		t.Errorf("the scene on the row below is on camera %d, want 1", ed.segs[1].Cam)
	}
	if ed.rows[ed.cutLanes[0].Name] != 1 {
		t.Errorf("the remaining lane is pinned to row %d, want 1", ed.rows[ed.cutLanes[0].Name])
	}
	if ed.segs[0].Cam != 0 {
		t.Errorf("the camera's own scene moved to row %d", ed.segs[0].Cam)
	}
}

func TestALaneThatNeverReachedTheBandTakesNothingWithIt(t *testing.T) {
	// a cut.json holding a lane too short to draw. It has no row, so it has no
	// scenes and nothing below it -- and reading its missing pin as row nought
	// would take the first camera's whole cut away with it
	ed := rowEd(t)
	ed.freezeRows()
	ed.cutLanes = []cutLane{{Name: "ghost", Src: "/f/a.mp4", At: 10, Dur: 0.1}}
	ed.segs = []cutSeg{{S: 2, E: 6}, {S: 20, E: 30}}
	ed.killLane("ghost")
	if len(ed.cutLanes) != 0 {
		t.Errorf("the lane is still there: %v", ed.cutLanes)
	}
	if len(ed.segs) != 2 || ed.rows["a"] != 0 {
		t.Errorf("it took %d scene(s) and the camera's row (%d) with it",
			2-len(ed.segs), ed.rows["a"])
	}
}

func TestALaneCountsAsAnEditWorthReverting(t *testing.T) {
	ed := rowEd(t)
	ed.setBase()
	if !sameState(ed.snapshot(), ed.base) {
		t.Fatal("the fixture starts out edited")
	}
	ed.addLane("/f/a.mp4", 20, 60, 10)
	if sameState(ed.snapshot(), ed.base) {
		t.Error("adding a row to the band does not read as an edit")
	}
}

func TestRemovingALaneMovesTheGreenAndTheCopyWithTheRows(t *testing.T) {
	// the two row numbers that are NOT written in the cut: what is selected,
	// and what is in hand. Renumbering the rows and leaving these behind is the
	// same silent repointing the pins prevent for scenes -- here it would put
	// the next ＋ Add, or the next paste, on a camera nobody chose
	ed := rowEd(t)
	one := ed.addLane("/f/a.mp4", 0, 10, 20) // row 1
	two := ed.addLane("/f/a.mp4", 50, 50, 20)
	ed.sel.active, ed.sel.lane = true, 2
	ed.copyOn, ed.copyCam, ed.copyFrom, ed.copyLen = true, 2, 55, 3

	ed.killLane(one)
	if !ed.sel.active || ed.sel.lane != 1 {
		t.Errorf("the selection is now on row %d (active %v), want row 1", ed.sel.lane, ed.sel.active)
	}
	if !ed.copyOn || ed.copyCam != 1 {
		t.Errorf("the copy now names row %d (in hand %v), want row 1", ed.copyCam, ed.copyOn)
	}
	// and what named the row that has gone lets go rather than sliding onto
	// its neighbour
	ed.killLane(two)
	if ed.sel.active {
		t.Error("the green is still on a row that is no longer there")
	}
	if ed.copyOn {
		t.Error("the copy is still in hand, naming a row that is no longer there")
	}
}

func TestAnUndoThatMovesTheRowsDropsTheCopyInHand(t *testing.T) {
	// the copy is the one row number the snapshot does not hold. Taken while
	// the band had one shape and pasted after an undo put it back into another,
	// it would splice footage off a camera nobody chose -- silently, which is
	// the whole failure this row's pins exist to prevent
	ed := rowEd(t)
	one := ed.addLane("/f/a.mp4", 0, 10, 20)
	ed.addLane("/f/a.mp4", 50, 50, 20)
	ed.killLane(one) // the second lane comes up to row 1
	ed.copyOn, ed.copyCam, ed.copyFrom, ed.copyLen = true, 1, 55, 3

	ed.undoLast() // ...and goes back down to row 2, without the copy
	if ed.copyOn {
		t.Error("the copy is still in hand after the rows moved under it")
	}
	// an undo that did not move the rows leaves it alone: dropping a copy
	// because a segment was taken back would be a press lost for nothing
	ed.copyOn, ed.copyCam = true, 0
	ed.pushUndo() // an ordinary edit: a scene added and taken back again
	ed.addRange(5, 9)
	ed.undoLast()
	if !ed.copyOn {
		t.Error("an ordinary undo dropped the copy in hand")
	}
}

func TestALaneCannotClaimMoreFileThanThereIs(t *testing.T) {
	ed := rowEd(t) // one recording, a hundred seconds of it
	// the selection ran past the end of the footage, so the copy is longer than
	// what is actually there. A row drawing band past its own last frame is a
	// row the green can be laid on over nothing
	name := ed.addLane("/f/a.mp4", 95, 10, 30)
	if name == "" || len(ed.cutLanes) != 1 {
		t.Fatalf("the lane was not made: %v", ed.cutLanes)
	}
	if d := ed.cutLanes[0].Dur; d != 5 {
		t.Errorf("the lane runs %g s, want the 5 s the file has left", d)
	}
	// no length asked for means all of it, which is what choosing a file rather
	// than copying a stretch says
	ed.addLane("/f/a.mp4", 30, 200, 0)
	if d := ed.cutLanes[1].Dur; d != 70 {
		t.Errorf("a lane of the whole rest of the file runs %g s, want 70", d)
	}
	// ...and what is left of the file may be nothing worth a row
	if ed.addLane("/f/a.mp4", 99.5, 10, 30) != "" {
		t.Errorf("half a second of file became a lane: %v", ed.cutLanes)
	}
}

func TestTheFirstLaneOnAnEmptyBandIsTheTopRow(t *testing.T) {
	// nothing was filmed and everything in the cut is going to be a lane. Read
	// off laneN this would leave an empty row above it: laneN is floored at one
	// so that a session with one camera still has a row to draw on
	ed := newTestEd(t)
	ed.relayout()
	name := ed.addLane("/f/only.mp4", 0, 0, 10)
	if name == "" {
		t.Fatal("no lane was made")
	}
	if ed.rows[name] != 0 {
		t.Errorf("the only row on the band is row %d, want 0", ed.rows[name])
	}
}

func TestTheWallClockOfSecondNoughtDoesNotDependOnTheOrderOfTheList(t *testing.T) {
	// the page holds its recordings in timeline order and the render hands them
	// over in the order they were chosen. With a corrected clock, "the first
	// one's" wall clock is two different numbers -- and the clip cut from a
	// lane would be named for a different moment in the render than on the page
	a := &App{}
	early := tlVideo{base: "a", path: "/f/a.mp4", start: 0, wall: 1000, dur: 100}
	late := tlVideo{base: "b", path: "/f/b.mp4", start: 25, wall: 1030, dur: 60} // dragged -5 s
	l := []cutLane{{Name: "sting", Src: "/f/sting.mp4", At: 40, Dur: 10}}

	one := a.laneVideos(l, []tlVideo{early, late})
	two := a.laneVideos(l, []tlVideo{late, early})
	if len(one) != 1 || len(two) != 1 {
		t.Fatalf("laneVideos drew %v and %v", one, two)
	}
	if one[0].wall != two[0].wall {
		t.Errorf("the same lane is at wall %g one way round and %g the other",
			one[0].wall, two[0].wall)
	}
	// read off the recording that starts earliest, which is the one the page
	// holds first
	if one[0].wall != 1040 {
		t.Errorf("the lane's wall clock is %g, want 1040", one[0].wall)
	}
}

func TestALaneWithNoFileBehindItIsNotARow(t *testing.T) {
	// an empty src resolves to the project folder, which is not a video: a row
	// that draws nothing, plays nothing, and cannot be told apart from one that
	// simply has no thumbnails yet
	ed := rowEd(t)
	if got := ed.a.laneVideos([]cutLane{{Name: "ghost", At: 10, Dur: 10}}, ed.vids); len(got) != 0 {
		t.Errorf("a lane with no file came out as %v", got)
	}
}

// ---- a copy is what a lane is usually made of -------------------------------

func TestACopyPutOnALaneOpensOntoTheSecondsItWasTakenFrom(t *testing.T) {
	ed := rowEd(t)
	// the camera was dragged five seconds later to line its waveform up, so
	// session second 62 is second 57 of the file. A lane is a window on the
	// FILE, and one that stored the session second would open five seconds off
	ed.shiftTo([]string{"a"}, map[string]float64{}, 5)
	ed.copyOn, ed.copyCam, ed.copyFrom, ed.copyLen = true, 0, 62, 3
	ed.playhead, ed.hasPlay = 80, true
	ed.a.pasteLane()

	if len(ed.cutLanes) != 1 {
		t.Fatalf("the copy did not become a lane: %v", ed.cutLanes)
	}
	if l := ed.cutLanes[0]; l.Off != 57 || l.At != 80 || l.Dur != 3 || l.Src == "" {
		t.Errorf("the copy came out as %+v, want 3 s of file second 57 at 80 s", l)
	}
	// and it plays those seconds: the red line is where it starts
	if v := videoOn(ed.vids, 1, 81); v == nil || v.at(81) != 58 {
		t.Errorf("the pasted lane plays %v at 81 s", v)
	}
	// the copy is spent, so the next press is not a second silent paste
	if ed.copyOn {
		t.Error("the copy is still in hand after being put on a lane")
	}
}

func TestACopyTakenOffALaneStillLandsOnTheRightSeconds(t *testing.T) {
	// the sum twice over. The lane at row 1 is already a window, so the file
	// second under session 62 is neither 62 nor 62 minus the row's start
	ed := rowEd(t)
	ed.addLane("/f/a.mp4", 20, 60, 10) // file 20-30 shown at 60-70
	ed.copyOn, ed.copyCam, ed.copyFrom, ed.copyLen = true, 1, 62, 3
	ed.playhead, ed.hasPlay = 85, true
	ed.a.pasteLane()

	if len(ed.cutLanes) != 2 {
		t.Fatalf("copying off a lane made %d lane(s)", len(ed.cutLanes))
	}
	if l := ed.cutLanes[1]; l.Off != 22 || l.At != 85 || l.Dur != 3 {
		t.Errorf("the copy of a copy came out as %+v, want file second 22", l)
	}
	// two rows off the same file, told apart by name -- the row pins and the
	// corrections are both held under it
	if a, b := ed.cutLanes[0].Name, ed.cutLanes[1].Name; a == b {
		t.Errorf("both lanes are called %q", a)
	}
}

func TestACopyOfNothingIsNotALane(t *testing.T) {
	ed := rowEd(t)
	// the recording it was taken from has since been dragged out from under
	// that second, so there is nothing to be a window on
	ed.copyOn, ed.copyCam, ed.copyFrom, ed.copyLen = true, 0, 400, 3
	ed.playhead, ed.hasPlay = 80, true
	ed.a.pasteLane()
	if len(ed.cutLanes) != 0 {
		t.Errorf("a copy off the end of everything made a lane: %v", ed.cutLanes)
	}
	// and without a red line there is no answer to where it would start
	ed.copyFrom, ed.hasPlay = 20, false
	ed.a.pasteLane()
	if len(ed.cutLanes) != 0 {
		t.Errorf("a lane was made with no red line to start it: %v", ed.cutLanes)
	}
}

func TestARowOfNoSecondsIsNotDrawn(t *testing.T) {
	ed := rowEd(t)
	// shorter than the shortest scene anyone can cut, and a row with no name
	// has no key for its pin or its correction to be held under
	got := ed.a.laneVideos([]cutLane{
		{Name: "blink", Src: "/f/a.mp4", At: 10, Dur: 0.2},
		{Name: "", Src: "/f/a.mp4", At: 20, Dur: 10},
		{Name: "ok", Src: "/f/a.mp4", At: 30, Off: 5, Dur: 10},
	}, ed.vids)
	if len(got) != 1 || got[0].base != "ok" {
		t.Fatalf("laneVideos drew %v", got)
	}
	// it is the recording seen twice, so it borrows what was already extracted
	if len(got[0].frames) != 100 || got[0].fps != 30 {
		t.Errorf("the lane did not borrow the recording's frames: %+v", got[0])
	}
	// ...and its own wall clock, so a clip named off it is named for when the
	// footage was shot rather than for where the row was put
	if got[0].wall != ed.vids[0].wall {
		t.Errorf("the lane's wall clock is %g, want the recording's %g", got[0].wall, ed.vids[0].wall)
	}
}

func TestAFileFromOutsideTheSessionGetsARowAndNoPictures(t *testing.T) {
	// the other kind of lane: a sting nobody filmed here, so there is no frame
	// folder to borrow. Extracting one would be exactly the Prepare pass
	// this row exists to skip -- it draws its name, and the preview plays it
	ed := rowEd(t)
	got := ed.a.laneVideos([]cutLane{{Name: "sting", Src: "/f/nope.mp4", At: 30, Dur: 10}}, ed.vids)
	if len(got) != 1 {
		t.Fatalf("laneVideos drew %v", got)
	}
	v := got[0]
	if len(v.frames) != 0 || v.interval != 0 {
		t.Errorf("a file with no frame folder came back with %d frames", len(v.frames))
	}
	// and it is still a row, of the right seconds, in the right place
	if v.base != "sting" || v.start != 30 || v.dur != 10 {
		t.Errorf("the outside file came out as %+v", v)
	}
	// its wall clock is worked out from where the row was put, because the
	// file has no stamp of its own on this session's clock
	if v.wall != 1030 {
		t.Errorf("the outside file's wall clock is %g, want 1030", v.wall)
	}
}

// ---- against the rest of the page -------------------------------------------

func TestALaneMovesWhenItsRowIsDragged(t *testing.T) {
	// a lane is a row, and a row can be right-dragged along the timeline like
	// any other (cut_shift.go). Its correction is held under the same key
	ed := rowEd(t)
	name := ed.addLane("/f/a.mp4", 20, 60, 10)
	ed.shiftTo([]string{name}, map[string]float64{}, 5)
	v := videoOn(ed.vids, 1, 70)
	if v == nil || v.start != 65 {
		t.Fatalf("the dragged lane is %v, want it starting at 65", v)
	}
	// what it SHOWS did not change -- the footage stayed put and the row moved
	if got := v.at(70); got != 25 {
		t.Errorf("the moved lane plays second %g at 70 s, want 25", got)
	}
	// and it comes back to the same place on the next load, because a lane is
	// placed from cut.json and then corrected, not corrected twice
	ed.setLanes(ed.cutLanes)
	ed.relayout()
	if v := videoOn(ed.vids, 1, 70); v == nil || v.start != 65 {
		t.Errorf("reloading the lanes put the row at %v", v)
	}
}

func TestTheRenderIsToldAboutTheRowsTheCutPutThere(t *testing.T) {
	// sessionTracks walks the SOURCES, and a cut lane is in no source list:
	// without this every scene cut to one resolves to whatever recording was
	// rolling at that second, and the render quietly shows the wrong angle
	src := readSrc(t, "produce.go")
	for _, want := range []string{
		"lanes := a.laneVideos(c.Lanes, out)",
		"lanes[i].start += c.Shift[lanes[i].base]",
		"out = append(out, lanes...)",
	} {
		if !strings.Contains(src, want) {
			t.Errorf("the render no longer builds the cut's own rows: %q", want)
		}
	}
	// and the editor's own load does the same thing in the same order:
	// corrections first, then the rows, because setLanes places them itself
	body := funcBody(t, "cut.go", `func \(ed \*cutEditor\) reload\(\) error \{`)
	i, j := strings.Index(body, "ed.applyShift()"), strings.Index(body, "ed.setLanes(c.Lanes)")
	if i < 0 || j < 0 || i > j {
		t.Error("reload no longer lays the cut's own rows over the sources after the corrections")
	}
}

func TestTheLaneControlsAreWired(t *testing.T) {
	src := readSrc(t, "cut.go")
	for _, want := range []string{
		// the third way a file can sit in the cut, beside over and between
		`own.SetVisible(insKind(path) == "video")`,
		"case m.asLane:\n\t\t// a row, not a scene.",
		"name := a.ed.addLane(file, 0, at, m.dur)",
		// a copy's other destination, on the bar only while one is in hand
		`ed.laneBtn = gtk.NewButtonFromIconName("go-down-symbolic")`,
		"ed.laneBtn.SetSensitive(ed.copyOn && ed.copyAud == \"\")",
		"name := ed.addLane(v.path, v.at(ed.copyFrom), ed.playhead, ed.copyLen)",
		// and the ✕ that takes a row away, asked before the clip border it can
		// sit on: a resize arrow over a button is a lie
		"if name := ed.laneKillAt(x+ed.viewX, y); name != \"\" {",
	} {
		if !strings.Contains(src, want) {
			t.Errorf("cut.go no longer contains %q", want)
		}
	}
	// the ✕ is the left button's and the border is the right button's, so
	// they cannot race each other for a press at all any more -- what has to
	// hold is that the ✕ is still asked before anything else the left press
	// could mean on the pictures
	i := strings.Index(src, "ed.laneKillAt(x+ed.viewX, y); name != \"\"")
	j := strings.Index(src, "ed.dropEdge() // any other left click puts a held edge or clip down")
	if i < 0 || j < 0 || i > j {
		t.Error("the lane's ✕ is no longer asked before the press falls through to a selection")
	}
}

func TestTheLanesKillBadgeIsOnTheRowsOwnStart(t *testing.T) {
	ed := rowEd(t)
	name := ed.addLane("/f/a.mp4", 20, 60, 10)
	ed.relayout()
	v := videoOn(ed.vids, 1, 65)
	cx, cy := ed.laneKillCentre(v)
	if got := ed.laneKillAt(cx, cy); got != name {
		t.Errorf("the badge at the lane's start answers %q, want %q", got, name)
	}
	// only a cut lane wears one: a recording is a source, and the place to
	// stop using a source is the page that chose it
	c := &ed.vids[0]
	if c.base != "a" {
		c = videoOn(ed.vids, 0, 50)
	}
	rx, ry := ed.laneKillCentre(c)
	if got := ed.laneKillAt(rx, ry); got != "" {
		t.Errorf("the camera's row wears a ✕ too, answering %q", got)
	}
}
