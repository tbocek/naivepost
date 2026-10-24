package main

// The picture comes from the camera the SCENE says, not from the clock.
//
// With one camera the two are the same question and this whole file is about
// nothing: exactly one recording was rolling at any second, so "the footage at
// 4:10" has one answer and pickVideo has always given it. With two cameras
// rolling at once it has two, and the cut is the only thing that knows which
// one was chosen -- it is written on the scene (cutSeg.Cam) and every place
// that turns a second into a picture has to read it.
//
// There are three such places and they must not disagree: the render's clip
// planner, the preview, and the thumbnails on the band. A render that shows one
// camera where the page showed the other is the worst kind of wrong -- nothing
// on the page is a lie, it is just answering a different question.

import (
	"strings"
	"testing"
)

// twoCams is two cameras rolling over the same minutes, on their own rows.
func twoCams() []tlVideo {
	vids := []tlVideo{
		{base: "a", path: "/f/a.mp4", start: 0, dur: 100},
		{base: "b", path: "/f/b.mp4", start: 10, dur: 80},
	}
	assignLanes(vids, nil)
	return vids
}

func TestTheRenderAsksForTheScenesCamera(t *testing.T) {
	// the clip planner turns each scene into a stretch of a file, and every
	// one of those lookups carries the scene's row now. Wiring inside a
	// several-hundred-line planner, so what is pinned is the source
	src := readSrc(t, "produce.go")
	for _, want := range []string{
		"v := pickVideoOn(vids, s.Cam, from)", // a copied stretch
		"v := pickVideoOn(vids, s.Cam, s.S)",  // an audio insert's picture, and footage
		"v := pickVideoOn(vids, s.Cam, s.S)",
	} {
		if !strings.Contains(src, want) {
			t.Errorf("the render no longer resolves footage by the scene's camera: %q", want)
		}
	}
	// ...and the rows themselves, which the render works out for itself: it
	// walks a snapshot with no editor behind it, so without this every Cam
	// would be measured against a slice where every lane is nought
	if !strings.Contains(src, "assignLanes(out, c.Rows)") {
		t.Error("the render's timeline has no rows, so every scene resolves to row nought")
	}
	// ...and nothing is resolved on the clock alone any more. The last one was
	// a stop effect's frozen frame: effects have no row of their own, so it
	// took whatever recording was rolling, and on a two-camera session that
	// froze camera 1's frame over a scene showing camera 2. An effect still has
	// no row, but the SCENE it stands on does, and that is the answer -- which
	// is what cutVideoOn is: the covering scene's camera, no editor behind it.
	if n := strings.Count(src, "pickVideo(vids,"); n != 0 {
		t.Errorf("produce.go has %d clock-only lookups left; every one belongs to the scene it stands on", n)
	}
	if !strings.Contains(src, "v := cutVideoOn(segs, vids, cue.fx.T)") {
		t.Error("a stop's frozen frame no longer comes off the camera its scene shows")
	}
}

func TestTheSoundUnderAnInsertComesFromTheRowItCovers(t *testing.T) {
	vids := twoCams()
	// an insert covering the picture ALONE keeps what is heard underneath, and
	// with two cameras rolling that is two different microphones. The row the
	// card was placed on is the one it covers, and the one it keeps.
	//
	// Neither file exists, so what comes back is the "has no sound" note --
	// which names the recording that was asked, and that is the claim here.
	for _, c := range []struct {
		cam  int
		want string
	}{{0, "a"}, {1, "b"}} {
		s := cutSeg{S: 40, E: 44, Ins: "card.svg", Mute: true, Cam: c.cam}
		_, _, note := soundUnder(s, vids)
		if !strings.Contains(note, c.want+" has no sound") {
			t.Errorf("a card on row %d kept: %q, want the sound of %s", c.cam, note, c.want)
		}
	}
}

func TestTheSubstitutedSoundFillsASpedUpSlot(t *testing.T) {
	// the trim is in source seconds and the slot is in output ones, so a clip
	// played at double speed eats twice its length of the file. Without the
	// speed the lane would run out halfway and apad would finish the slot in
	// silence -- which sounds like the render dropping the audio
	src := readSrc(t, "produce.go")
	if !strings.Contains(src, `args = append(args, "-t", fmt.Sprintf("%.3f", c.length*math.Max(1, c.speed())),`) {
		t.Error("a substituted lane is trimmed to the slot rather than to what the slot eats")
	}
}

// ---- the preview ------------------------------------------------------------

func TestThePreviewShowsTheCameraTheCutChose(t *testing.T) {
	ed := axisEd(t, twoCams()...)
	ed.segs = []cutSeg{{S: 20, E: 30}, {S: 30, E: 40, Cam: 1}, {S: 60, E: 70}}
	for _, c := range []struct {
		t    float64
		want string
	}{
		{25, "a"}, // first scene: the camera it says
		{35, "b"}, // ...and the second, which says the other one
		{65, "a"},
	} {
		if v := ed.videoAt(c.t); v == nil || v.base != c.want {
			t.Errorf("at %.0f s the preview would load %v, want %s", c.t, v, c.want)
		}
	}
}

func TestOutsideAKeptSceneThePreviewFollowsTheHand(t *testing.T) {
	ed := axisEd(t, twoCams()...)
	ed.segs = []cutSeg{{S: 20, E: 30}}
	// 50 s is in no scene: every row's thumbnails are on the band at once and
	// the preview has to show one of them
	if v := ed.videoAt(50); v == nil || v.base != "a" {
		t.Errorf("with the hand on the first row the preview showed %v", v)
	}
	ed.sel.lane = 1
	if v := ed.videoAt(50); v == nil || v.base != "b" {
		t.Errorf("dragging on the second row the preview still showed %v", v)
	}
	// but inside a scene the cut still wins: the hand is not what is being cut
	if v := ed.videoAt(25); v == nil || v.base != "a" {
		t.Errorf("the hand overrode the scene's own camera: %v", v)
	}
}

func TestTheCameraTheCutAsksForMayNotBeRolling(t *testing.T) {
	// a cut.json written before the rows existed says row nought for
	// everything, and a session re-shot with the cameras the other way up
	// says a row that is not there at all. Neither may render a black hole
	ed := axisEd(t, twoCams()...)
	ed.segs = []cutSeg{{S: 5, E: 8, Cam: 1}, {S: 95, E: 99, Cam: 3}}
	for _, c := range []struct{ t, want float64 }{{6, 0}, {97, 0}} {
		if v := ed.videoAt(c.t); v == nil {
			t.Errorf("at %.0f s a scene on a camera that was not rolling draws nothing", c.t)
		}
	}
}

func TestPlaybackFollowsTheCutOntoTheOtherCamera(t *testing.T) {
	// playing across a change of camera means loading the other file, and
	// nothing else in the tick does it: the player runs one file to its end.
	// The reload is a visible hiccup and an accepted one
	body := funcBody(t, "cut.go", `func \(ed \*cutEditor\) followPlayback\(\) bool \{`)
	if !strings.Contains(body, "if v := ed.videoAt(ed.playhead); v != nil && v != ed.playVideo {") ||
		!strings.Contains(body, "ed.setPlayhead(ed.playhead)") {
		t.Errorf("playback no longer follows the cut onto another camera:\n%s", body)
	}
	// ...but not out from under a card, which is the picture just now: the
	// footage underneath is not on screen and re-cueing it only jogs the sound
	if !strings.Contains(body, "if card, _ := ed.cardNow(); card == nil || card.audioIns() {") {
		t.Error("a change of camera re-cues the footage under a card that is covering it")
	}
	// after the gap skip, which does its own setPlayhead and has already
	// returned by then
	if i, j := strings.Index(body, "if ed.skipGap() {"), strings.Index(body, "ed.cardNow()"); i < 0 || i > j {
		t.Error("the camera check runs before the gap skip, so both cue the preview in one tick")
	}
}

// ---- a copy is a copy OF something ------------------------------------------

func TestACopyRemembersWhichCameraItWasTakenFrom(t *testing.T) {
	ed := newTestEd(t)
	ed.vids = twoCams()
	ed.addSplice("copy:40.000", 70, 5, false, 1)
	if len(ed.segs) != 1 || ed.segs[0].Cam != 1 {
		t.Fatalf("the pasted copy came out as %v", ed.segs)
	}
	// which is the whole point: without it the render reads "the footage at
	// 40 s" and gets whichever recording is first in the list
	from, ok := copySrc(ed.segs[0].Ins)
	if !ok {
		t.Fatal("the paste is not a copy any more")
	}
	if v := pickVideoOn(ed.vids, ed.segs[0].Cam, from); v == nil || v.base != "b" {
		t.Errorf("the copy plays %v again, want the second camera", v)
	}
}

// ---- watching one row -------------------------------------------------------

// Inside a kept scene camAt answers with the scene's camera, which left no way
// at all to see what another camera saw at the same second -- and that is most
// of how a scene gets stolen for it. A click on a row asks exactly that: the
// preview WATCHES the clicked row, scene or no scene, until ▶ hands the answer
// back to the cut.
func TestAClickedRowIsWatchedEvenInsideAScene(t *testing.T) {
	ed := axisEd(t, twoCams()...)
	ed.segs = []cutSeg{{S: 20, E: 30}} // a scene on the first camera
	ed.monRow = 2                      // a click on the second row
	if v := ed.videoAt(25); v == nil || v.base != "b" {
		t.Errorf("watching the second row inside the scene the preview showed %v, want b", v)
	}
	// withdrawn, the scene's own camera is back -- monRow's zero value is
	// exactly "the cut answers", so an editor nobody clicked in behaves
	ed.monRow = 0
	if v := ed.videoAt(25); v == nil || v.base != "a" {
		t.Errorf("with nothing watched the preview showed %v, want the scene's a", v)
	}
	// a watch outlives the rows it points at: clamped, not trusted, since a
	// reload can take cameras away between the click and the asking -- and
	// clamped ONCE (watchRow), so the preview, the dashed outline and the
	// status line cannot each clamp differently and disagree
	ed.monRow = 9
	if v := ed.videoAt(25); v == nil || v.base != "b" {
		t.Errorf("watching a row that left, the preview showed %v, want the last row's b", v)
	}
	if got := ed.watchRow(); got != 1 {
		t.Errorf("watchRow() = %d, want the last row 1", got)
	}
	// and a session with no footage at all has no rows to watch, whatever a
	// stale click left behind
	ed.vids = nil
	ed.relayout()
	if got := ed.watchRow(); got != -1 {
		t.Errorf("watchRow() with no footage = %d, want -1", got)
	}
}

// The watched row is told apart on the page -- a dashed outline around its
// pictures and its strip -- because a preview quietly showing something other
// than the cut is a lie waiting to be rendered.
func TestTheWatchedRowWearsItsOutline(t *testing.T) {
	ed := axisEd(t, twoCams()...)
	h := int(ed.picBottom()) + 80
	plain := renderTrack(t, ed, 400, h)
	ed.monRow = 2
	dashed := renderTrack(t, ed, 400, h)
	lt := int(ed.laneTop(1))
	changed := 0
	for x := 2; x < 398; x++ {
		pr, pg, pb := plain(x, lt+1)
		dr, dg, db := dashed(x, lt+1)
		if pr != dr || pg != dg || pb != db {
			changed++
		}
	}
	if changed < 50 {
		t.Errorf("watching the second row changed %d px along its top edge, want a dashed line's worth", changed)
	}
	// and only that row: the first row's edge is untouched
	lt0 := int(ed.laneTop(0))
	for x := 2; x < 398; x++ {
		pr, pg, pb := plain(x, lt0+1)
		if dr, dg, db := dashed(x, lt0+1); pr != dr || pg != dg || pb != db {
			t.Errorf("the outline leaked onto the first row at x=%d", x)
			break
		}
	}
}

// The seams: the click that starts a watch, the ▶ that ends it, and the
// sentence that says the preview and the cut disagree on purpose.
func TestTheWatchIsWired(t *testing.T) {
	src := readSrc(t, "cut.go")
	for _, want := range []string{
		// a click on a row's pictures or on its wave strip starts the watch;
		// a click anywhere else moves the line and changes no minds
		"ed.monRow = l + 1",
		// ...and only from a standstill: followPlayback re-cues from camAt
		// every tick, so a watch started mid-play would change what PLAYS
		"if area == ed.srcArea && !ed.playing() && len(ed.vids) > 0 {",
		// ▶ plays the CUT: the watch is withdrawn on the way into playing
		"ed.monRow = 0",
		// and the disagreement is said out loud, only when there is one
		"watching camera %d — the cut shows camera %d here; ",
	} {
		if !strings.Contains(src, want) {
			t.Errorf("cut.go no longer contains %q", want)
		}
	}
}

func TestAStandstillOnAnEmptyRowShowsItsEmptiness(t *testing.T) {
	// videoAt is charitable on purpose -- a scene must never render a hole --
	// but the paused preview borrowing another camera's frame looks exactly
	// like the row HAVING that footage. videoShown is the same question told
	// to answer honestly: the watched row's own recording, or nothing.
	ed := axisEd(t, twoCams()...)
	ed.monRow = 2 // watch row 1, where b rolls from 10 to 90
	if v := ed.videoShown(20); v == nil || v.base != "b" {
		t.Fatal("the watched row's own footage is not what the standstill shows")
	}
	if v := ed.videoShown(5); v != nil {
		t.Errorf("row 1 has nothing at 5 s, yet the standstill would show %q", v.base)
	}
	if ed.videoAt(5) == nil {
		t.Error("the charitable lookup lost its fallback; scenes would render holes")
	}
	// the same honesty when the row is the selection's, not a watched one
	ed.monRow = 0
	ed.sel.lane = 1
	if v := ed.videoShown(5); v != nil {
		t.Errorf("the selected row has nothing at 5 s, yet it would show %q", v.base)
	}
}

func TestTheBlackFrameIsWired(t *testing.T) {
	// the choice lives in showInsert, the one place every path that moves the
	// playhead already settles the picture -- a second settling place would be
	// a second chance to disagree
	src := readSrc(t, "cut_insview.go")
	for _, want := range []string{
		"if !ed.player.playing && ed.videoShown(ed.playhead) == nil {",
		"ed.player.ShowStill(blackStill())",
		"pb.Fill(0x000000ff)",
	} {
		if !strings.Contains(src, want) {
			t.Errorf("the empty row no longer goes black at a standstill: %q", want)
		}
	}
	// pausing is a standstill playback's own ticks do not re-settle, so ⏯
	// settles it by hand
	if !strings.Contains(readSrc(t, "cut.go"),
		"ed.showInsert()\n\ted.started = ed.started || ed.player.Playing()") {
		t.Error("pausing over an empty stretch leaves the borrowed frame up")
	}
}
