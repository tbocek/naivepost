package main

// More than one camera.
//
// A session shot on two cameras has two pictures for the same second, and the
// picture band cannot draw both on one line. So it stacks: a row per camera,
// the same second above the same second, and the green that says what the cut
// keeps drawn down through all of them as one band -- because the cut is one
// thing, not one thing per camera.
//
// The rows are not a setting. They are worked out from the footage: recordings
// that follow one another in time share a row, recordings that overlap cannot.
// A one-camera session therefore comes out as exactly the single row this page
// had before rows existed, at the same y, which is the first thing pinned here.

import (
	"math"
	"strings"
	"testing"
)

// ---- who is on which row ------------------------------------------------------

func TestRecordingsShareARowUnlessTheyOverlap(t *testing.T) {
	for _, c := range []struct {
		what  string
		vids  []tlVideo
		lanes []int
		rows  int
	}{
		{"one recording", []tlVideo{{start: 0, dur: 60}}, []int{0}, 1},
		// one camera's files follow one another, so they are one row: this is
		// every session this page has ever drawn
		{"one camera, three files",
			[]tlVideo{{start: 0, dur: 60}, {start: 60, dur: 60}, {start: 200, dur: 30}},
			[]int{0, 0, 0}, 1},
		{"two cameras through each other",
			[]tlVideo{{start: 0, dur: 60}, {start: 30, dur: 60}}, []int{0, 1}, 2},
		{"three at once",
			[]tlVideo{{start: 0, dur: 60}, {start: 10, dur: 60}, {start: 20, dur: 60}},
			[]int{0, 1, 2}, 3},
		// the second camera stops, and what comes after it drops back to the
		// first free row rather than piling up a third
		{"a second camera that stops",
			[]tlVideo{{start: 0, dur: 30}, {start: 10, dur: 40}, {start: 60, dur: 20}},
			[]int{0, 1, 0}, 2},
		// touching is not overlapping: one ends exactly where the next begins
		{"one stopping as the next starts",
			[]tlVideo{{start: 0, dur: 30}, {start: 30, dur: 30}}, []int{0, 0}, 1},
		// the list is sorted by start on load, but a row shifted in time is not
		{"out of order",
			[]tlVideo{{start: 100, dur: 20}, {start: 0, dur: 60}}, []int{0, 0}, 1},
	} {
		vids := append([]tlVideo(nil), c.vids...)
		if got := assignLanes(vids, nil); got != c.rows {
			t.Errorf("%s: %d row(s), want %d", c.what, got, c.rows)
		}
		for i, v := range vids {
			if v.lane != c.lanes[i] {
				t.Errorf("%s: recording %d is on row %d, want %d", c.what, i, v.lane, c.lanes[i])
			}
		}
	}
	// nothing loaded still has a row: the band is drawn empty, not zero-high
	if got := assignLanes(nil, nil); got != 1 {
		t.Errorf("an empty session has %d rows, want 1", got)
	}
}

// ---- where the rows are -------------------------------------------------------

// The compatibility claim, in pixels. One camera is one thumbnail's worth of
// band -- and the effects lane is above it now, under the green bar, where its
// y depends on nothing but the two fixed bands over it (cut_fx.go).
func TestOneCameraLeavesTheBandExactlyWhereItWas(t *testing.T) {
	ed := axisEd(t, tlVideo{start: 0, dur: 60})
	if ed.laneN != 1 {
		t.Fatalf("one camera makes %d rows", ed.laneN)
	}
	if got, want := ed.picBottom(), ed.picTop()+float64(ed.thumbHt)+4; got != want {
		t.Errorf("the band ends at %.0f, want %.0f", got, want)
	}
	if got, want := ed.fxLaneTop(), float64(rulerH)+float64(selBandH); got != want {
		t.Errorf("the effects lane starts at %.0f, want %.0f", got, want)
	}
	if got, want := ed.picTop(), ed.fxLaneTop()+ed.fxLaneHeight(); got != want {
		t.Errorf("the pictures start at %.0f, want %.0f — under the lane", got, want)
	}
	// and an editor that has never been laid out -- no widgets, no relayout --
	// measures the same, because half the page asks these before any of that
	bare := newTestEd(t)
	if bare.picBottom() != ed.picBottom() || bare.fxLaneTop() != ed.fxLaneTop() {
		t.Errorf("an unlaid-out editor measures %.0f/%.0f, want %.0f/%.0f",
			bare.picBottom(), bare.fxLaneTop(), ed.picBottom(), ed.fxLaneTop())
	}
}

func TestASecondCameraPushesTheEffectsLaneDown(t *testing.T) {
	one := axisEd(t, tlVideo{start: 0, dur: 60})
	ed := axisEd(t, tlVideo{start: 0, dur: 60}, tlVideo{start: 30, dur: 60})
	if ed.laneN != 2 {
		t.Fatalf("two overlapping cameras make %d rows, want 2", ed.laneN)
	}
	if got, want := ed.laneTop(1), ed.picTop()+ed.laneH()+laneGap; got != want {
		t.Errorf("the second row starts at %.0f, want %.0f", got, want)
	}
	if got, want := ed.picBottom()-one.picBottom(), ed.laneH()+laneGap; got != want {
		t.Errorf("a second row added %.0f px, want %.0f", got, want)
	}
	// ...and the effects lane does not move at all: it is above the pictures,
	// so how many cameras there are is none of its business
	if ed.fxLaneTop() != one.fxLaneTop() {
		t.Errorf("a second camera moved the effects lane from %.0f to %.0f",
			one.fxLaneTop(), ed.fxLaneTop())
	}
	// which row a press landed on. The three px between two rows are on
	// neither, which is why laneAt can say so -- a drag started there has no
	// camera to be about
	for _, c := range []struct {
		y    float64
		lane int
	}{
		{ed.picTop(), 0},
		{ed.picTop() + ed.laneH() - 1, 0},
		{ed.picTop() + ed.laneH() + 1, -1}, // the gap between the rows
		{ed.laneTop(1), 1},
		{ed.picBottom() - 1, 1},
		{ed.picBottom(), -1}, // past the stack altogether
	} {
		if got := ed.laneAt(c.y); got != c.lane {
			t.Errorf("y %.0f is on row %d, want %d", c.y, got, c.lane)
		}
	}
	// the band itself is the whole stack, gap included: everything drawn across
	// it -- the green, the scrim, the markers -- is one object down all the rows
	for _, y := range []float64{ed.picTop(), ed.picTop() + ed.laneH() + 1, ed.picBottom() - 1} {
		if !ed.hitPics(y) {
			t.Errorf("y %.0f is not on the picture band", y)
		}
	}
	if ed.hitPics(ed.picTop()-1) || ed.hitPics(ed.picBottom()) {
		t.Error("the picture band reaches past its own ends")
	}
	// the effects lane must not start inside the pictures, whatever the row
	// count -- a press cannot be on both
	if ed.fxHitLane(ed.picBottom() - 1) {
		t.Error("the last row of pictures is also the effects lane")
	}
}

// ---- one camera at a time -----------------------------------------------------

// The green says what will be SHOWN. With two cameras up, that is a claim about
// one of them, so it is drawn on that camera's row and nowhere else -- a tint
// down both rows would be the page saying the finished video shows two pictures
// at the same second.
func TestTheGreenIsOnTheCameraItShows(t *testing.T) {
	ed := axisEd(t, tlVideo{start: 0, dur: 60}, tlVideo{start: 0, dur: 60})
	ed.segs = []cutSeg{{S: 10, E: 50, Cam: 1}}
	const w, h = 400, 300
	at := renderTrack(t, ed, w, h)
	green := func(y float64) bool {
		r, g, b := at(int(ed.xOf(30)), int(y))
		return g > r+10 && g > b+10
	}
	if !green(ed.laneTop(1) + 4) {
		t.Error("the second camera is not tinted at a second it is kept for")
	}
	if green(ed.picTop() + 4) {
		t.Error("the first camera is tinted at a second the second one is shown")
	}
	if green(ed.fxLaneTop() + 2) {
		t.Error("the green ran on into the effects lane")
	}
	// and on one camera it is the whole band, as it always was
	one := axisEd(t, tlVideo{start: 0, dur: 60})
	one.segs = []cutSeg{{S: 10, E: 50}}
	at1 := renderTrack(t, one, w, h)
	for _, y := range []float64{one.picTop() + 2, one.picBottom() - 2} {
		r, g, b := at1(int(one.xOf(30)), int(y))
		if !(g > r+10 && g > b+10) {
			t.Errorf("a one-camera cut is not tinted at y %.0f", y)
		}
	}
}

// Painting camera B over camera A's green takes those seconds off A. Two rows
// green over one second would leave the render to pick a camera behind your
// back, so the newer green wins and the older one gives way -- which is also
// what makes switching camera the same gesture as choosing the footage.
func TestASecondCameraTakesTheSecondsOffTheFirst(t *testing.T) {
	ed := axisEd(t, tlVideo{start: 0, dur: 100}, tlVideo{start: 0, dur: 100})
	ed.sel.lane = 0
	ed.addRange(0, 100)
	if len(ed.segs) != 1 || ed.segs[0].Cam != 0 {
		t.Fatalf("the first camera was not kept whole: %v", ed.segs)
	}
	// the second camera takes the middle
	ed.sel.lane = 1
	ed.addRange(40, 60)
	if len(ed.segs) != 3 {
		t.Fatalf("switching camera mid-scene left %d scenes, want 3: %v", len(ed.segs), ed.segs)
	}
	for i, want := range []cutSeg{{S: 0, E: 40, Cam: 0}, {S: 40, E: 60, Cam: 1}, {S: 60, E: 100, Cam: 0}} {
		if ed.segs[i].S != want.S || ed.segs[i].E != want.E || ed.segs[i].Cam != want.Cam {
			t.Errorf("scene %d is %.0f-%.0f on camera %d, want %.0f-%.0f on %d",
				i, ed.segs[i].S, ed.segs[i].E, ed.segs[i].Cam, want.S, want.E, want.Cam)
		}
	}
	// no second is claimed twice, which is the property the list above is one
	// example of
	for i := range ed.segs {
		for j := i + 1; j < len(ed.segs); j++ {
			a, b := ed.segs[i], ed.segs[j]
			if a.S < b.E && b.S < a.E {
				t.Errorf("two cameras are both shown over %.0f-%.0f", math.Max(a.S, b.S), math.Min(a.E, b.E))
			}
		}
	}
	// ...and the two halves of camera 0 do NOT merge back together when the
	// switch is taken out from between them by a later edit: they touch camera
	// 1, not each other
	ed.coalesce()
	if len(ed.segs) != 3 {
		t.Errorf("a camera switch was merged away: %v", ed.segs)
	}
}

// Two scenes of the same camera that touch are one scene, exactly as before --
// the rule that keeps cameras apart must not stop a camera merging with itself.
func TestOneCameraStillMergesWithItself(t *testing.T) {
	ed := axisEd(t, tlVideo{start: 0, dur: 100}, tlVideo{start: 0, dur: 100})
	ed.segs = []cutSeg{{S: 0, E: 40, Cam: 1}, {S: 40, E: 80, Cam: 1}}
	ed.coalesce()
	if len(ed.segs) != 1 || ed.segs[0].S != 0 || ed.segs[0].E != 80 {
		t.Errorf("two touching scenes of one camera came out as %v", ed.segs)
	}
	if ed.segs[0].Cam != 1 {
		t.Errorf("the merged scene forgot its camera: %v", ed.segs[0])
	}
}

// A scene split by an edit keeps saying which camera it is. It used to be
// rebuilt as a bare pair of times, which on one camera said nothing and on two
// would quietly move half of it back to the first row.
func TestSplittingASceneKeepsItsCamera(t *testing.T) {
	ed := axisEd(t, tlVideo{start: 0, dur: 100}, tlVideo{start: 0, dur: 100})
	ed.segs = []cutSeg{{S: 0, E: 100, Cam: 1}}
	ed.removeRange(40, 60)
	if len(ed.segs) != 2 {
		t.Fatalf("removing the middle left %d scenes: %v", len(ed.segs), ed.segs)
	}
	for _, s := range ed.segs {
		if s.Cam != 1 {
			t.Errorf("a half of the scene came back on camera %d, want 1: %v", s.Cam, s)
		}
	}
}

// ---- which file a scene shows ---------------------------------------------------

// A scene names a row, and a row is several files in a line, so which file it
// shows is a question about a second. A row that has nothing at that second
// falls back to whatever was rolling -- a cut.json written before the rows
// existed says row 0 for everything, and a scene that draws no picture at all
// would be a black hole in the render for a reason nobody can see on this page.
func TestASceneFindsItsOwnCamerasFile(t *testing.T) {
	vids := []tlVideo{
		{base: "a1", start: 0, dur: 50}, {base: "a2", start: 50, dur: 50},
		{base: "b", start: 20, dur: 40},
	}
	assignLanes(vids, nil)
	for _, c := range []struct {
		cam  int
		t    float64
		want string
	}{
		{0, 10, "a1"},
		{0, 60, "a2"}, // one row, the next file along
		{1, 30, "b"},
		{1, 80, "a2"}, // row 1 stopped at 60: fall back rather than draw nothing
	} {
		v := pickVideoOn(vids, c.cam, c.t)
		if v == nil || v.base != c.want {
			t.Errorf("camera %d at %.0fs shows %v, want %s", c.cam, c.t, v, c.want)
		}
	}
	if pickVideoOn(vids, 0, 500) != nil {
		t.Error("a second nobody filmed found a file to show")
	}
}

// A row's name stays where the recorders' band keeps its own.
//
// It was drawn at the recording's start, so it travelled with the tape: scroll
// a minute into a file and the row's name was a minute off the left of the
// widget, while the lane names below it -- pinned at laneNameX, un-translated
// -- sat still. One page, two rules, and the one that moves is the one you
// have to scroll back to read.
//
// Which recording it names is the one under the view's left edge, so a row of
// several files still says which of them is on screen.
func TestARowsNameIsPinnedAndNamesWhatIsUnderTheEdge(t *testing.T) {
	ed := newTestEd(t) // 4 px per second
	ed.vids = []tlVideo{
		{base: "a", path: "/f/a.mkv", start: 0, dur: 60, lane: 0},
		{base: "b", path: "/f/b.mkv", start: 90, dur: 60, lane: 0},
	}
	ed.laneN = 1
	ed.relayout()
	name := func(row int) *tlVideo { return ed.rowNameVid(row, ed.viewX-80, ed.viewX+900) }

	// the view's left edge inside the first file: that is the name
	ed.viewX = 40 // 10 s in
	if v := name(0); v == nil || v.base != "a" {
		t.Errorf("over the first file the row is named %v, want a", v)
	}
	// scrolled into the second: the name follows what is under the edge
	ed.viewX = ed.xOf(100)
	if v := name(0); v == nil || v.base != "b" {
		t.Errorf("over the second file the row is named %v, want b", v)
	}
	// the edge over the gap between them: the first file in view, not nothing
	ed.viewX = ed.xOf(70)
	if v := name(0); v == nil || v.base != "b" {
		t.Errorf("over the gap the row is named %v, want the next one in view", v)
	}
	// a row with nothing on screen has nothing to name
	ed.viewX = 5000 // px, well past the end of both files
	if v := name(0); v != nil {
		t.Errorf("a row scrolled past is still named %q", v.base)
	}
	// a row nobody filmed on
	ed.viewX = 0
	if v := name(3); v != nil {
		t.Errorf("an empty row is named %q", v.base)
	}

	// and it is drawn at the fixed x, the same one the lanes below use
	src := readSrc(t, "cut.go")
	if !strings.Contains(src, "plateText(cr, ed.viewX+laneNameX, ed.laneTop(r)+12, name)") {
		t.Error("the row's name is no longer pinned where the lane names are")
	}
	if strings.Contains(src, "plateText(cr, v.pxOrigin+4") {
		t.Error("the row's name travels with the tape again")
	}
	// the yellow border is still the file's own start -- where a recording
	// BEGINS is a place on the tape, where its name is drawn is not -- and it
	// is drawn just inside the footage, so the mark is on the picture and
	// never on the hole beside it
	if !strings.Contains(src, "x0, x1 := v.pxOrigin+srcEdgeIn, ed.xOf(v.start+v.dur)-srcEdgeIn") {
		t.Error("the file boundary no longer marks where the file starts, inset into it")
	}
	if srcEdgeIn <= 0 {
		t.Error("the border is drawn on the seam again")
	}
}
