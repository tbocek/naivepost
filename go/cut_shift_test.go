package main

// Putting a recording where it actually belongs.
//
// sourceStart reads a file's clock off its name, and its name is written by
// whatever wrote the file: a camera that was set once in 2019, a recorder that
// stamps the moment the card was closed, a phone that syncs with the network
// and a phone that does not. It gets the ORDER right and it gets the seconds
// wrong, and the seconds are what the lanes are drawn against -- so the same
// shout in two waveforms sits a column apart and there is nothing on the page
// that can move it.
//
// This is that thing. The right button drags a row, or a lane, along the
// timeline; the offset is per source, saved, and re-applied by the render. And
// because the same button over a selection means the other kind of "wrong place
// in time" -- the scenes, not the camera -- it slides the green there instead.
//
// The trap the whole file circles: the rows are DERIVED from which recordings
// overlap (assignLanes), and a shift changes which recordings overlap. Two
// cameras dragged apart would silently collapse onto one row, and cutSeg.Cam is
// a row number.

import (
	"encoding/json"
	"math"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// shiftEd is two cameras over the same minutes with a separate recorder beside
// them, laid out. Camera a is one file; camera b is a camera stopped and
// started again, so it is two files on one row -- the case a per-file shift
// gets wrong.
func shiftEd(t *testing.T) *cutEditor {
	t.Helper()
	ed := newTestEd(t)
	ed.vids = []tlVideo{
		{base: "a", path: "/f/a.mp4", start: 0, dur: 100},
		{base: "b1", path: "/f/b1.mp4", start: 10, dur: 40},
		{base: "b2", path: "/f/b2.mp4", start: 60, dur: 30},
	}
	ed.auds = []tlAudio{
		{base: "a", path: "/f/a.mp4", start: 0, dur: 100, chans: 2, master: true},
		{base: "b1", path: "/f/b1.mp4", start: 10, dur: 40, chans: 2, master: true},
		{base: "b2", path: "/f/b2.mp4", start: 60, dur: 30, chans: 2, master: true},
		{base: "mic", path: "/f/mic.wav", start: 5, dur: 90, chans: 1},
	}
	ed.gaps = map[string][]float64{"a": {20, 40}, "b1": {15}}
	ed.relayout()
	if ed.laneN != 2 {
		t.Fatalf("the fixture came out on %d row(s), want 2", ed.laneN)
	}
	return ed
}

// ---- moving a recording -----------------------------------------------------

// Everything that came out of one file moves together: the pictures, the sound
// on the same track, and the snap points worked out against it. A shift that
// moved the footage and left its own waveform behind would put the page into a
// state no session can be in.
func TestOneSourceMovesInOnePiece(t *testing.T) {
	ed := shiftEd(t)
	ed.shiftTo([]string{"a"}, nil, 7)

	if v := ed.vids[0]; v.base != "a" || v.start != 7 {
		t.Errorf("the footage is at %g on %s, want a at 7", v.start, v.base)
	}
	for _, au := range ed.auds {
		if au.base == "a" && au.start != 7 {
			t.Errorf("the footage's own track stayed at %g", au.start)
		}
		if au.base == "mic" && au.start != 5 {
			t.Errorf("the recorder moved to %g and nobody asked it to", au.start)
		}
	}
	if g := ed.gaps["a"]; len(g) != 2 || g[0] != 27 || g[1] != 47 {
		t.Errorf("the snap points came out %v, want 27 and 47", g)
	}
	if ed.gaps["b1"][0] != 15 {
		t.Error("another camera's snap points moved with this one")
	}
}

// A camera stopped and started again is several files and ONE clock. Correcting
// it means correcting all of them by the same seconds, or the correction opens
// a hole in the middle of that camera's own coverage.
func TestARowMovesAsOneCamera(t *testing.T) {
	ed := shiftEd(t)
	srcs := ed.laneSrcs(1)
	if len(srcs) != 2 || !contains(srcs, "b1") || !contains(srcs, "b2") {
		t.Fatalf("row 1 came out as %v, want both of camera b's files", srcs)
	}
	ed.shiftTo(srcs, nil, -4)
	for _, v := range ed.vids {
		want := map[string]float64{"a": 0, "b1": 6, "b2": 56}[v.base]
		if v.start != want {
			t.Errorf("%s starts at %g, want %g", v.base, v.start, want)
		}
	}
}

// The correction is absolute, not a running total: every update of a drag
// carries the whole gesture so far. Dragged out and back means back, exactly,
// with nothing saved -- a map that remembered a zero would be a project file
// that says a correction was made when none was.
func TestADragThatEndsWhereItBeganLeavesNothing(t *testing.T) {
	ed := shiftEd(t)
	from := copyShift(ed.shift)
	for _, d := range []float64{3, 9, 25, 4, 0} { // one drag, five updates
		ed.shiftTo([]string{"mic"}, from, d)
	}
	for _, au := range ed.auds {
		if au.base == "mic" && au.start != 5 {
			t.Errorf("the recording came to rest at %g, want 5", au.start)
		}
	}
	if len(ed.shift) != 0 {
		t.Errorf("the file would say %v, want no correction at all", ed.shift)
	}
}

// The bound. A drag is pixels and a session is minutes, so no single gesture
// can do this -- but drags add up, and a recording past the end of everything
// there is has nothing left to line up against.
func TestACorrectionCannotLeaveTheSession(t *testing.T) {
	ed := shiftEd(t)
	ed.shiftTo([]string{"mic"}, nil, 1e6)
	got := ed.shift["mic"]
	if got <= 0 || got >= 1e6 {
		t.Errorf("a mile of drag came out as %g s, want it bounded and forward", got)
	}
	ed.shiftTo([]string{"mic"}, nil, -1e6)
	if -ed.shift["mic"] != got {
		t.Errorf("the bound is %g one way and %g the other", got, -ed.shift["mic"])
	}
}

// ---- the rows -----------------------------------------------------------------

// The trap, stated as a test. Two cameras that overlap are two rows; drag one
// clear of the other and the arithmetic says one row. The rows are written down
// at the first drag so they cannot say that, because every scene shot on camera
// 2 is a scene that names row 1.
func TestDraggingTwoCamerasApartKeepsThemOnTheirOwnRows(t *testing.T) {
	ed := shiftEd(t)
	if ed.rows != nil {
		t.Fatal("the rows were pinned before anything was dragged")
	}
	ed.shiftTo(ed.laneSrcs(1), nil, 95) // camera b now starts after camera a ends
	if ed.laneN != 2 {
		t.Fatalf("the band collapsed to %d row(s)", ed.laneN)
	}
	for _, v := range ed.vids {
		want := 0
		if strings.HasPrefix(v.base, "b") {
			want = 1
		}
		if v.lane != want {
			t.Errorf("%s is drawn on row %d, want %d", v.base, v.lane, want)
		}
	}
	if ed.rows["b1"] != 1 || ed.rows["a"] != 0 {
		t.Errorf("the written-down rows are %v", ed.rows)
	}
}

// Until something is dragged they stay derived, so a camera added on
// Prepare still lands where the arithmetic puts it. Pinning them from the
// start would freeze the first session anyone ever opened.
func TestTheRowsStayDerivedUntilTheFirstDrag(t *testing.T) {
	ed := shiftEd(t)
	ed.vids = append(ed.vids, tlVideo{base: "c", path: "/f/c.mp4", start: 20, dur: 30})
	ed.relayout()
	if ed.rows != nil || ed.laneN != 3 {
		t.Errorf("a third camera came out on %d row(s) with pins %v", ed.laneN, ed.rows)
	}
}

// A pinned row is kept whatever it now overlaps, and the rest are coloured
// around it -- including a pinned row that starts LATER than an unpinned one,
// which is the case a single greedy pass gets wrong.
func TestAPinnedRowIsKeptAndTheRestColourAroundIt(t *testing.T) {
	vids := []tlVideo{
		{base: "late", start: 50, dur: 40},
		{base: "early", start: 0, dur: 40},
	}
	if n := assignLanes(vids, map[string]int{"late": 0}); n != 1 {
		t.Fatalf("two recordings that do not overlap came out on %d rows", n)
	}
	for _, v := range vids {
		if v.lane != 0 {
			t.Errorf("%s went to row %d", v.base, v.lane)
		}
	}
	// ...and now one that does overlap the pin has to go elsewhere
	vids = []tlVideo{
		{base: "late", start: 50, dur: 40},
		{base: "over", start: 40, dur: 40},
	}
	assignLanes(vids, map[string]int{"late": 0})
	if vids[0].lane != 0 || vids[1].lane != 1 {
		t.Errorf("the rows came out %d and %d, want the pin kept and the other moved",
			vids[0].lane, vids[1].lane)
	}
}

// ---- moving the green ---------------------------------------------------------

// Inside a selection the footage stays and the CUT travels: same scene, a beat
// later. Whole scenes only -- one half in the selection would have to be split
// to move, and a drag that quietly cut the cut in two is not what was asked.
func TestTheGreenSlidesAndTheFootageStaysPut(t *testing.T) {
	ed := shiftEd(t)
	ed.segs = []cutSeg{{S: 20, E: 30}, {S: 35, E: 40}, {S: 55, E: 70}}
	ed.sel.active, ed.sel.t0, ed.sel.t1 = true, 15, 45
	from := append([]cutSeg(nil), ed.segs...)

	if !ed.slideGreen(from, 5) {
		t.Fatal("the green did not move")
	}
	want := []cutSeg{{S: 25, E: 35}, {S: 40, E: 45}, {S: 55, E: 70}}
	for i, s := range ed.segs {
		if s.S != want[i].S || s.E != want[i].E {
			t.Errorf("scene %d is %g–%g, want %g–%g", i, s.S, s.E, want[i].S, want[i].E)
		}
	}
	if ed.vids[0].start != 0 {
		t.Errorf("the footage moved to %g; only the cut was supposed to", ed.vids[0].start)
	}
}

// Every update of the drag is computed from the cut as it was at the press, not
// from the cut as it is. The clipping below is not reversible, and a drag
// applied a hundred times over would eat the scenes it merely swept across.
// A card takes no session time of its own (S == E), so it has nothing to be
// half in the selection WITH -- it is either in or it is not. In, it walks with
// the scenes: the whole reason it is at 32 s is that it sits between those two
// scenes, and a card that stayed put while they moved would come out of the
// render somewhere else in the video than where it was put.
func TestACardTravelsWithTheScenesAroundIt(t *testing.T) {
	ed := shiftEd(t)
	ed.segs = []cutSeg{
		{S: 20, E: 30, Cam: 1},
		{S: 32, E: 32, Ins: "card.svg", Dur: 3, Cam: 1}, // between them
		{S: 70, E: 80, Cam: 1},                          // and one outside, on b2
		{S: 85, E: 85, Ins: "other.svg", Dur: 3, Cam: 1},
	}
	ed.sel.active, ed.sel.t0, ed.sel.t1 = true, 15, 50
	from := append([]cutSeg(nil), ed.segs...)

	if !ed.slideGreen(from, 5) {
		t.Fatal("the green did not move")
	}
	at := func(ins string) float64 {
		for _, s := range ed.segs {
			if s.Ins == ins {
				return s.S
			}
		}
		t.Fatalf("%s fell out of the cut: %v", ins, ed.segs)
		return 0
	}
	if got := at("card.svg"); got != 37 {
		t.Errorf("the card inside the selection is at %g s, want 37 -- it stayed where it was", got)
	}
	if got := at("other.svg"); got != 85 {
		t.Errorf("a card outside the selection moved to %g s", got)
	}
}

// ...and a selection holding nothing but a card still moves it. There is no
// footage to clamp against, which is not the same as there being nowhere to go.
func TestACardOnItsOwnStillSlides(t *testing.T) {
	ed := shiftEd(t)
	ed.segs = []cutSeg{{S: 32, E: 32, Ins: "card.svg", Dur: 3, Cam: 1}}
	ed.sel.active, ed.sel.t0, ed.sel.t1 = true, 15, 50
	from := append([]cutSeg(nil), ed.segs...)
	if !ed.slideGreen(from, 5) || ed.segs[0].S != 37 {
		t.Errorf("the card came out at %v", ed.segs)
	}
	// but an empty selection is a drag over nothing, and rewrites nothing
	ed.segs = []cutSeg{{S: 70, E: 80, Cam: 1}}
	if ed.slideGreen(append([]cutSeg(nil), ed.segs...), 5) {
		t.Error("a drag over a selection with nothing in it reported a move")
	}
}

func TestTheGreenIsMeasuredFromWhereTheDragStarted(t *testing.T) {
	ed := shiftEd(t)
	ed.segs = []cutSeg{{S: 20, E: 30}}
	ed.sel.active, ed.sel.t0, ed.sel.t1 = true, 0, 100
	from := append([]cutSeg(nil), ed.segs...)
	for _, d := range []float64{40, 80, 200, 6} {
		ed.slideGreen(from, d)
	}
	if len(ed.segs) != 1 || ed.segs[0].S != 26 || ed.segs[0].E != 36 {
		t.Errorf("the scene came to rest at %v, want 26–36", ed.segs)
	}
}

// A scene may not land where its own camera was not rolling: there would be no
// picture to render from it. It is clipped to that camera's coverage, and a
// drag that would leave nothing of it leaves it alone instead.
func TestTheGreenCannotSlideOffItsOwnCamera(t *testing.T) {
	ed := shiftEd(t)
	ed.segs = []cutSeg{{S: 20, E: 40, Cam: 1}} // camera b1 covers 10..50
	ed.sel.active, ed.sel.t0, ed.sel.t1 = true, 0, 100
	from := append([]cutSeg(nil), ed.segs...)

	// it stops at the end of b1 and keeps its twenty seconds: a right drag
	// moves the cut, it does not trim it
	for _, d := range []float64{20, 500} {
		ed.slideGreen(from, d)
		if ed.segs[0].S != 30 || ed.segs[0].E != 50 {
			t.Errorf("a drag of %g left the scene at %g–%g, want 30–50",
				d, ed.segs[0].S, ed.segs[0].E)
		}
	}
}

// And they move together. Two scenes with a hole between them are a rhythm, and
// one of them stopping at the end of its camera while the other walked on would
// not be moving the cut -- it would be rewriting it.
func TestTheSelectedScenesTravelAsOne(t *testing.T) {
	ed := shiftEd(t)
	ed.segs = []cutSeg{{S: 20, E: 30, Cam: 1}, {S: 35, E: 45, Cam: 1}} // b1 ends at 50
	ed.sel.active, ed.sel.t0, ed.sel.t1 = true, 0, 100
	from := append([]cutSeg(nil), ed.segs...)

	ed.slideGreen(from, 30) // the second one may only go 5
	want := []cutSeg{{S: 25, E: 35, Cam: 1}, {S: 40, E: 50, Cam: 1}}
	for i, s := range ed.segs {
		if s.S != want[i].S || s.E != want[i].E {
			t.Errorf("scene %d is %g–%g, want %g–%g", i, s.S, s.E, want[i].S, want[i].E)
		}
	}
}

// A copy is stored as the session second its footage starts at, and that second
// only means anything relative to where the recording sits. Correcting the
// camera and leaving the copy behind would silently repoint it at whatever is
// under that second now.
func TestACopiedStretchFollowsTheCameraItWasTakenFrom(t *testing.T) {
	ed := shiftEd(t)
	ed.segs = []cutSeg{
		{S: 5, E: 5, Ins: copyScheme + "20.000", Dur: 4, Cam: 1}, // off camera b
		{S: 8, E: 8, Ins: copyScheme + "30.000", Dur: 4, Cam: 0}, // off camera a
		{S: 12, E: 12, Ins: "/x/sting.mp4", Dur: 2, Cam: 1},      // a real card: stays
	}
	ed.shiftTo(ed.laneSrcs(1), nil, 6)

	if at, _ := copySrc(ed.segs[0].Ins); at != 26 {
		t.Errorf("the copy off the moved camera points at %g s, want 26", at)
	}
	if at, _ := copySrc(ed.segs[1].Ins); at != 30 {
		t.Errorf("the copy off the other camera moved to %g s", at)
	}
	if ed.segs[2].Ins != "/x/sting.mp4" {
		t.Errorf("a spliced file was rewritten as %q", ed.segs[2].Ins)
	}
	// ...and moving a separate recorder is not about the picture at all
	ed.shiftTo([]string{"mic"}, nil, 9)
	if at, _ := copySrc(ed.segs[0].Ins); at != 26 {
		t.Errorf("moving the recorder moved a copied stretch to %g s", at)
	}
}

// ---- undo, saving, and the render ----------------------------------------------

// A drag is an edit like any other, and Undo is the whole gesture in one step.
// The rows come back with it, because they were frozen BY the drag: an undo
// that put the seconds back and left the pins would leave the project nailed to
// a shape it no longer has.
func TestUndoPutsAMovedCameraBack(t *testing.T) {
	ed := shiftEd(t)
	ed.pushUndo()
	ed.shiftTo(ed.laneSrcs(1), copyShift(ed.shift), 12)
	if ed.vids[1].start == 10 {
		t.Fatal("nothing moved")
	}
	ed.undoLast()
	for _, v := range ed.vids {
		want := map[string]float64{"a": 0, "b1": 10, "b2": 60}[v.base]
		if v.start != want {
			t.Errorf("after undo %s is at %g, want %g", v.base, v.start, want)
		}
	}
	if len(ed.shift) != 0 || ed.rows != nil {
		t.Errorf("undo left shift=%v rows=%v", ed.shift, ed.rows)
	}
}

// A lane dragged into place is a change to the project even with no scenes cut,
// so Revert has something to revert and the render prefers the editor over the
// file it has not written yet.
func TestAMovedLaneCountsAsAnEdit(t *testing.T) {
	ed := shiftEd(t)
	was := ed.snapshot()
	ed.shiftTo([]string{"mic"}, nil, 3)
	if sameState(ed.snapshot(), was) {
		t.Error("moving a recording reads as no change at all")
	}
	ed.a.ed = ed
	if got := ed.a.produceCut().Shift["mic"]; got != 3 {
		t.Errorf("the render would place the recording %g s out", 3-got)
	}
}

// The corrections are the project's, so they are in the project's file, and
// they come back on the way in -- reload builds every start from sourceStart,
// which has never heard of them.
func TestTheCorrectionsAreSavedAndComeBack(t *testing.T) {
	ed := shiftEd(t)
	ed.shiftTo(ed.laneSrcs(1), nil, -6)
	ed.persist()

	b, err := os.ReadFile(ed.a.cutPath())
	if err != nil {
		t.Fatal(err)
	}
	var c cutFile
	if err := json.Unmarshal(b, &c); err != nil {
		t.Fatal(err)
	}
	if c.Shift["b1"] != -6 || c.Shift["b2"] != -6 || c.Rows["b1"] != 1 {
		t.Fatalf("cut.json says shift=%v rows=%v", c.Shift, c.Rows)
	}

	// the way in: a freshly built timeline plus what was saved
	fresh := shiftEd(t)
	fresh.shift, fresh.rows = c.Shift, c.Rows
	fresh.applyShift()
	fresh.relayout()
	for _, v := range fresh.vids {
		want := map[string]float64{"a": 0, "b1": 4, "b2": 54}[v.base]
		if v.start != want {
			t.Errorf("%s reopened at %g, want %g", v.base, v.start, want)
		}
	}
	if !strings.Contains(funcBody(t, "cut.go", `func \(ed \*cutEditor\) reload\(\) error \{`),
		"ed.applyShift()") {
		t.Error("reload no longer re-applies the corrections, so they die with the window")
	}
}

// The render places its own tracks off the same raw timestamps the hand was
// dragging to correct, so it has to read the corrections too -- and the rows,
// or cutSeg.Cam means one thing on the page and another in the file.
func TestTheRenderPlacesTheRecordingsWhereTheHandPutThem(t *testing.T) {
	a := insertApp(t)
	dir := t.TempDir()
	one := filepath.Join(dir, "one_2026-01-01_10-00-00.mp4")
	two := filepath.Join(dir, "two_2026-01-01_10-00-02.mp4")
	for _, p := range []string{one, two} {
		mustFFmpeg(t, "-f", "lavfi", "-t", "4", "-i", "testsrc=size=160x120:rate=15",
			"-c:v", "libx264", "-preset", "ultrafast", "-pix_fmt", "yuv420p", p)
	}
	os.MkdirAll(a.cutDir(), 0o755)
	b, _ := json.Marshal(cutFile{
		Segs:  []cutSeg{{S: 0, E: 2}},
		Shift: map[string]float64{"two": 1.5},
		Rows:  map[string]int{"one": 0, "two": 1},
	})
	if err := os.WriteFile(a.cutPath(), b, 0o644); err != nil {
		t.Fatal(err)
	}

	vids, _, err := a.sessionTracks([]string{one, two}, nil)
	if err != nil {
		t.Fatal(err)
	}
	for _, v := range vids {
		switch v.base {
		case "one":
			if v.start != 0 || v.lane != 0 {
				t.Errorf("one is at %g on row %d, want 0 and 0", v.start, v.lane)
			}
		case "two":
			// named 2 s after the first, dragged 1.5 s later again
			if v.start != 3.5 || v.lane != 1 {
				t.Errorf("two is at %g on row %d, want 3.5 and 1", v.start, v.lane)
			}
		}
	}
}

// Everything downstream of the cut matches transcript lines against clips, and
// the transcript was written against the raw clocks. A corrected clock has to be
// applied to the lines too, or Suggest is told the wrong seconds and Narrate
// gathers the wrong lines under each clip.
func TestTheSessionTimelineMovesWithTheRecording(t *testing.T) {
	a := &App{outDir: t.TempDir()}
	os.MkdirAll(a.transcriptDir(), 0o755)
	tsv := "0.00\t2.00\tcam\tSPEAKER_00\tfirst\n" +
		"1.00\t3.00\tmic\tSPEAKER_01\tsecond\n"
	if err := os.WriteFile(filepath.Join(a.transcriptDir(), "session.tsv"), []byte(tsv), 0o644); err != nil {
		t.Fatal(err)
	}
	os.MkdirAll(a.cutDir(), 0o755)
	b, _ := json.Marshal(cutFile{Segs: []cutSeg{{S: 0, E: 1}}, Shift: map[string]float64{"mic": -1.5}})
	if err := os.WriteFile(a.cutPath(), b, 0o644); err != nil {
		t.Fatal(err)
	}

	rows := a.sessionRows()
	if len(rows) != 2 {
		t.Fatalf("the timeline came back as %d line(s)", len(rows))
	}
	// the corrected line is earlier now, so it also comes first
	if rows[0].src != "mic" || rows[0].s != -0.5 || rows[0].e != 1.5 {
		t.Errorf("the recorder's line is %s %g–%g, want mic -0.5–1.5", rows[0].src, rows[0].s, rows[0].e)
	}
	if rows[1].src != "cam" || rows[1].s != 0 {
		t.Errorf("the camera's line moved to %g and nobody asked it to", rows[1].s)
	}
	// and every step that matches lines against clips reads it through there
	for _, f := range []string{"cut_suggest.go", "narrate.go", "publish.go"} {
		if !strings.Contains(readSrc(t, f), "a.sessionRows()") {
			t.Errorf("%s still reads session.tsv straight off the disk", f)
		}
	}
}

// A corrected clock is otherwise invisible: the lane simply sits where it sits,
// and nothing distinguishes "the recorder was two seconds out" from "this is
// where the file says it starts". Both plates say it, because a session can
// have a camera with no sound at all and a recorder with no picture.
func TestTheCorrectionIsWrittenOnTheThingThatMoved(t *testing.T) {
	ed := shiftEd(t)
	if ed.shiftOf("a") != 0 {
		t.Error("an untouched source claims a correction")
	}
	ed.shiftTo([]string{"a"}, nil, 1.25)
	if ed.shiftOf("a") != 1.25 || ed.shiftOf("mic") != 0 {
		t.Errorf("the plates would read %+.2f and %+.2f", ed.shiftOf("a"), ed.shiftOf("mic"))
	}
	for _, c := range []struct{ file, want string }{
		{"cut.go", `if d := ed.shiftOf(v.base); d != 0 {`},        // the camera's own row
		{"cut_audio.go", `if d := ed.shiftOf(au.base); d != 0 {`}, // and every waveform lane
	} {
		if !strings.Contains(readSrc(t, c.file), c.want) {
			t.Errorf("%s draws no sign that a clock was corrected", c.file)
		}
	}
}

// ---- the gesture ----------------------------------------------------------------

// The wiring, which cannot be called without a display. The right button is the
// TIMELINE's: nothing else on the page can move a recording, and everything else
// on the page is measured against where the recordings sit.
func TestTheRightButtonSlidesTheTimeline(t *testing.T) {
	src := readSrc(t, "cut.go")
	for _, want := range []string{
		"slide.SetButton(gdk.BUTTON_SECONDARY)",
		// on a lane it is that one recording; there is no green down there
		"slideSrcs = []string{ed.audAtY(y)}",
		// inside the selection, on the pictures, it is the scenes
		"case ed.sel.active && ed.sel.aud == \"\" && t >= a0 && t < a1 &&",
		"slideSegs = append([]cutSeg(nil), ed.segs...)",
		// anywhere else on the pictures it is the whole row
		"slideSrcs = ed.laneSrcs(l)",
		// pixels over the zoom, NOT tAtView: unfilmed time takes no width, so a
		// drag whose pixels cross a seam would read as a jump of minutes
		"d := ox / ed.pps",
		"ed.pushUndo() // the whole gesture is one step back",
		"ed.shiftTo(slideSrcs, slideFrom, d)",
		"ed.slideGreen(slideSegs, d)",
		"ed.persist()",
	} {
		if !strings.Contains(src, want) {
			t.Errorf("the timeline drag no longer contains %q", want)
		}
	}
}

// ---- snapping the drag ------------------------------------------------------

// A right-drag lands exactly on the things worth landing on. The seconds are
// worked out once at the press (slideSnapSet) and the pull itself is pure
// arithmetic (slideSnap): every moving edge is offered to every still one and
// the closest pair inside the tolerance wins.
func TestTheDragSnapsOntoTheStillEdges(t *testing.T) {
	ed := shiftEd(t)
	// the case the whole thing exists for: a selection drawn on the row above,
	// and camera b dragged until its first file starts where the selection does
	ed.sel.active, ed.sel.t0, ed.sel.t1 = true, 30, 70

	edges, targets := ed.slideSnapSet([]string{"b1", "b2"}, false)
	// b's own four edges move; a's and the mic's stay, and the selection's two
	// borders are still ones too
	if len(edges) != 4 { // b1 and b2's pictures; their master sound is the
		// same file and is not counted twice
		t.Fatalf("edges = %v, want b's own four", edges)
	}
	has := func(xs []float64, want float64) bool {
		for _, x := range xs {
			if math.Abs(x-want) < 1e-9 {
				return true
			}
		}
		return false
	}
	for _, want := range []float64{30, 70, 0, 100, 5, 95} {
		if !has(targets, want) {
			t.Errorf("targets %v are missing %v", targets, want)
		}
	}
	if has(targets, 10) || has(targets, 50) {
		t.Error("a moving edge of b1 is offered as a target -- the drag would snag on itself")
	}

	// b1 starts at 10; dragged 19.7 s it is 0.3 s short of the selection's 30,
	// inside a 0.5 s reach: the drag becomes exactly 20
	if d := slideSnap(19.7, edges, targets, 0.5); math.Abs(d-20) > 1e-9 {
		t.Errorf("slideSnap(19.7) = %v, want pulled to 20", d)
	}
	// too far from everything: the drag is left alone
	if d := slideSnap(3.7, edges, targets, 0.1); d != 3.7 {
		t.Errorf("slideSnap(3.7) = %v, want untouched", d)
	}
	// the closest pair wins: at d≈-4.9, b1's start (10) is 0.1 from the mic's
	// start (5), nearer than anything else
	if d := slideSnap(-4.9, edges, targets, 0.5); math.Abs(d-(-5)) > 1e-9 {
		t.Errorf("slideSnap(-4.9) = %v, want -5 (b1 flush with the mic)", d)
	}
}

// On a green drag it is the scenes that move, so their borders are the moving
// edges and everything else -- the selection's own borders included -- stays
// still to be landed on.
func TestTheDragComesBackFlushToTheSessionStart(t *testing.T) {
	// dragging the very recording that anchors 0 turns its start into a MOVING
	// edge, so the origin used to vanish as a destination exactly when a hand
	// was trying to put something back on it. The ruler's zero is a still
	// target in its own right, whatever moves.
	ed := shiftEd(t)
	edges, targets := ed.slideSnapSet([]string{"a"}, false)
	found := false
	for _, x := range targets {
		if math.Abs(x) < 1e-9 {
			found = true
		}
	}
	if !found {
		t.Fatalf("targets %v do not offer the session start", targets)
	}
	// a starts at 0; nudged 0.3 s off and within reach, it comes back flush
	if d := slideSnap(0.3, edges, targets, 0.5); math.Abs(d) > 1e-9 {
		t.Errorf("slideSnap(0.3) = %v, want pulled back to 0", d)
	}
}

func TestTheGreenDragSnapsByItsScenes(t *testing.T) {
	ed := shiftEd(t)
	ed.segs = []cutSeg{{S: 12, E: 18}, {S: 40, E: 48}, {S: 80, E: 90}}
	ed.sel.active, ed.sel.t0, ed.sel.t1 = true, 35, 60

	edges, targets := ed.slideSnapSet(nil, true)
	if len(edges) != 2 || edges[0] != 40 || edges[1] != 48 {
		t.Fatalf("edges = %v, want the wholly-inside scene's 40 and 48", edges)
	}
	// the scene dragged until its start is a hair from the outside scene's end
	if d := slideSnap(-21.8, edges, targets, 0.5); math.Abs(d-(-22)) > 1e-9 {
		t.Errorf("slideSnap(-21.8) = %v, want -22 (flush against the scene at 18)", d)
	}
	// ...and the selection's own border is a landing too
	if d := slideSnap(-5.2, edges, targets, 0.5); math.Abs(d-(-5)) > 1e-9 {
		t.Errorf("slideSnap(-5.2) = %v, want -5 (scene start on the selection's 35)", d)
	}
}

// The gesture actually asks. Live coordinates, so the seam is pinned.
func TestTheDragSnapIsWired(t *testing.T) {
	src := readSrc(t, "cut.go")
	for _, want := range []string{
		// the sets are frozen at the press...
		"slideEdges, slideTargs = ed.slideSnapSet(slideSrcs, slideSegs != nil)",
		// ...and every update passes through the snap, with the same fixed
		// pixel reach every other snap on the page has
		"d = slideSnap(d, slideEdges, slideTargs, snapPx/math.Max(ed.pps, 0.001))",
	} {
		if !strings.Contains(src, want) {
			t.Errorf("cut.go no longer contains %q", want)
		}
	}
}

// ---- moving a part onto another row -----------------------------------------

// The same right button, straight up or down: a part goes onto another row when
// that row has room for it, the kept scenes that showed it come along, and a
// part dragged somewhere it will not fit stays exactly where it is.
func TestAPartMovesToARowWithRoomAndItsScenesFollow(t *testing.T) {
	ed := newTestEd(t)
	ed.vids = []tlVideo{
		{base: "a", path: "/f/a.mp4", start: 0, dur: 40},
		{base: "b", path: "/f/b.mp4", start: 10, dur: 40},
		{base: "c", path: "/f/c.mp4", start: 60, dur: 20},
	}
	ed.relayout()
	if ed.laneN != 2 {
		t.Fatalf("fixture came out on %d rows, want 2 (c fits after a)", ed.laneN)
	}
	// a scene showing c, one showing a, and a card standing on c's row
	ed.segs = []cutSeg{
		{S: 5, E: 15, Cam: 0},
		{S: 62, E: 70, Cam: 0},
		{S: 71, E: 75, Cam: 0, Ins: "card.png"},
	}

	// no room: b lies over a for 30 of its seconds
	if ed.moveRow([]string{"b"}, 0) {
		t.Error("b moved onto a row a is filling")
	}
	// no row: the band has two
	if ed.moveRow([]string{"c"}, 2) {
		t.Error("c moved onto a row the band does not have")
	}
	// already there is not a move
	if ed.moveRow([]string{"c"}, 0) {
		t.Error("moving c onto its own row claimed to have done something")
	}

	if !ed.moveRow([]string{"c"}, 1) {
		t.Fatal("c would not move to row 1, where 50..80 is empty")
	}
	if l := videoOn(ed.vids, 1, 65); l == nil || l.base != "c" {
		t.Error("c is not on row 1 after the move")
	}
	if ed.rows["c"] != 1 {
		t.Error("the move is not pinned -- the next relayout would put c back")
	}
	// the scene inside c's stretch came along; the one on a's footage and the
	// card stayed
	if ed.segs[1].Cam != 1 {
		t.Errorf("the scene showing c still names row %d, want 1", ed.segs[1].Cam)
	}
	if ed.segs[0].Cam != 0 || ed.segs[2].Cam != 0 {
		t.Errorf("scenes that never showed c moved rows: %v", ed.segs)
	}

	// ...and back again, scenes and all
	if !ed.moveRow([]string{"c"}, 0) {
		t.Fatal("c would not move back")
	}
	if ed.segs[1].Cam != 0 {
		t.Error("the scene did not come back with its footage")
	}
}

// The gesture's half of it. Live pointer coordinates, so the seams are pinned:
// what rows are offered, how the gates open, and that a purely vertical drag
// never lets the snap move the clock sideways.
func TestTheRowMoveIsWired(t *testing.T) {
	src := readSrc(t, "cut.go")
	for _, want := range []string{
		// only sources on the picture band have rows
		"slideRows = slideSrcs != nil && area == ed.srcArea",
		// the pointer's row is the ask, strips counting as their row
		"if r := ed.rowAt(slideY0 + oy); ed.rowFits(slideSrcs, r) {",
		"if to >= 0 && ed.moveRow(slideSrcs, to) {",
		// the TIME gate stays sideways-only; a vertical drag opens the
		// gesture without it
		"slideTimeOn = slideTimeOn || math.Abs(ox) >= 3",
		"if !slideTimeOn && to < 0 {",
	} {
		if !strings.Contains(src, want) {
			t.Errorf("cut.go no longer contains %q", want)
		}
	}
}
