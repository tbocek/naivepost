package main

// Which lanes a scene is heard on.
//
// The choice used to be one answer for the whole project and is now one answer
// per scene, made on the lanes themselves: take a scene in hand and every audio
// row says whether that scene hears it, with a badge that turns it off and on.
// Two lanes on at once is a legal answer and a common one -- game plus voice --
// which is why the render sums them at the level each was recorded at and puts
// a ceiling on the finished clip rather than ducking one to fit the other
// (clipCeil, produce.go, and cut_quiet_test.go for the arithmetic).
//
// What is tested here is the page's half: where the badges are, which lane a
// press lands on, that turning one off is one undo step that does not reach
// back into the history, and that a project written before any of this opens
// sounding the way it was left.

import (
	"encoding/json"
	"os"
	"regexp"
	"strings"
	"testing"
)

// heardTracks is two cameras -- the first stopped and started again, so it is
// two recordings on one row -- and a recorder running under both.
func heardTracks() ([]tlVideo, []tlAudio) {
	vids := []tlVideo{
		{base: "a1", path: "/f/a1.mp4", start: 0, dur: 50},
		{base: "b", path: "/f/b.mp4", start: 10, dur: 90},
		{base: "a2", path: "/f/a2.mp4", start: 60, dur: 40},
	}
	assignLanes(vids, nil)
	auds := append(masterOf(vids), tlAudio{base: "mic", path: "/f/mic.wav", start: 5, dur: 120})
	return vids, auds
}

// masterOf is masterLanes without the probe: the real one reads each file for
// its channel count, and these files are not on disk.
func masterOf(vids []tlVideo) []tlAudio {
	var out []tlAudio
	for _, v := range vids {
		out = append(out, tlAudio{base: v.base, path: v.path, start: v.start,
			dur: v.dur, chans: 1, master: true})
	}
	return out
}

// hearEd is that session on the page, with one scene shot on the first camera
// held in hand. 20 s at 4 px/s is 80 px of scene, comfortably over hearMin.
func hearEd(t *testing.T) *cutEditor {
	t.Helper()
	ed := newTestEd(t)
	ed.vids, ed.auds = heardTracks()
	ed.relayout()
	ed.segs = []cutSeg{{S: 20, E: 40}}
	ed.segOn, ed.segSel = true, 0
	return ed
}

// ---- where the badges are ---------------------------------------------------

func TestEveryLaneAHeldSceneCanHearWearsABadge(t *testing.T) {
	ed := hearEd(t)
	// a second recorder, and a stereo first one: the badge is per RECORDING
	// and not per channel -- a stereo microphone is one microphone -- and the
	// lane below it has to be found past both of its waveforms
	ed.auds[3].chans = 2
	ed.auds = append(ed.auds, tlAudio{base: "room", path: "/f/room.wav", start: 0, dur: 120, chans: 1})

	got := ed.hearBadgesAud()
	if len(got) != 2 {
		t.Fatalf("the recorders' band drew %d badges, want one per recording: %+v", len(got), got)
	}
	x0 := ed.xOf(20)
	for i, w := range []struct {
		base  string
		cy, h float64
	}{
		// the stereo one is two waveforms deep and its badge is the middle of
		// both: one recording, one answer, and a wash that covers all of it
		{"mic", wavePad + waveLaneH, 2 * waveLaneH},
		{"room", wavePad + 2*waveLaneH + waveGap + waveLaneH/2, waveLaneH},
	} {
		if got[i].base != w.base || got[i].cx != x0+hearIn || got[i].cy != w.cy || got[i].h != w.h {
			t.Errorf("badge %d is %q at (%.1f, %.1f) %.1f deep, want %q at (%.1f, %.1f) %.1f deep",
				i, got[i].base, got[i].cx, got[i].cy, got[i].h, w.base, x0+hearIn, w.cy, w.h)
		}
		if !got[i].on {
			t.Errorf("%s starts silent, and an untouched scene hears everything", w.base)
		}
	}

	// the camera's own sound wears one too, on the strip paired under its
	// pictures -- and only the camera this scene is SHOWN from, because no
	// other row's track reaches this clip to be silenced
	src := ed.hearBadgesSrc()
	if len(src) != 1 || src[0].base != "a1" {
		t.Fatalf("the picture band drew %+v, want a1 alone", src)
	}
	if cy := ed.laneTop(0) + ed.laneH() + waveLaneH/2; src[0].cx != x0+hearIn ||
		src[0].cy != cy || src[0].h != waveLaneH {
		t.Errorf("a1's badge is at (%.1f, %.1f) %.1f deep, want (%.1f, %.1f) %.1f deep",
			src[0].cx, src[0].cy, src[0].h, x0+hearIn, cy, waveLaneH)
	}
	// ...and a stereo camera's strip washes both of its waveforms too
	ed.auds[0].chans = 2
	if b := ed.hearBadgesSrc(); b[0].h != 2*waveLaneH ||
		b[0].cy != ed.laneTop(0)+ed.laneH()+waveLaneH {
		t.Errorf("a stereo camera's badge is at %.1f and %.1f deep, want %.1f and %.1f",
			b[0].cy, b[0].h, ed.laneTop(0)+ed.laneH()+waveLaneH, 2*waveLaneH)
	}
	ed.auds[0].chans = 1

	// and what they say follows the scene
	ed.segs[0].Quiet = []string{"mic", "a1"}
	if b := ed.hearBadgesAud(); b[0].on || !b[1].on {
		t.Errorf("with mic silenced the band says mic=%v room=%v", b[0].on, b[1].on)
	}
	if b := ed.hearBadgesSrc(); b[0].on {
		t.Error("with the camera silenced its own strip still says it is heard")
	}
}

func TestTheBadgeLeavesTheSceneItselfPressable(t *testing.T) {
	ed := hearEd(t)
	// the badge is the scene's left end and nothing more: at the narrowest
	// scene that wears one there is still plain timeline to press, or putting
	// the line on the clip would switch a lane instead
	if gap := hearMin - 2*(hearIn+hearHit); gap <= 0 {
		t.Fatalf("at hearMin the badge's target covers the scene's middle by %.1f px", -gap)
	}
	if got, ok := ed.hearX(); !ok || got != ed.xOf(20)+hearIn {
		t.Errorf("the badges sit at %.1f (%v), want %.1f in from the left border",
			got, ok, ed.xOf(20)+hearIn)
	}
	// ...and below that width nothing is drawn, rather than two targets on top
	// of one another
	ed.segs[0].E = 20 + (hearMin-1)/ed.pps
	if _, ok := ed.hearX(); ok {
		t.Errorf("a scene %.1f px wide still wears badges", hearMin-1)
	}
}

// With nothing in hand the badges are the scene under the LINE's -- the one
// the preview is hushing (syncHush), so the badge is the thing you hear -- and
// with the line in no scene at all there are none. An insert wears none
// either way: its sound is its own.
func TestNothingInHandMeansTheLinesSceneWearsTheBadges(t *testing.T) {
	ed := hearEd(t)
	held := ed.hearBadgesAud()
	ed.segOn = false
	ed.playhead = (ed.segs[0].S + ed.segs[0].E) / 2
	if got := ed.hearBadgesAud(); len(got) != len(held) || got[0].on != held[0].on || got[0].cx != held[0].cx {
		t.Errorf("with the line in the scene and nothing held the badges are %+v, want the scene's own %+v", got, held)
	}
	lane := held[0].base
	ed.segs[0].Quiet = []string{lane}
	if got := ed.hearBadgesAud(); len(got) == 0 || got[0].on {
		t.Errorf("a lane the line's scene silences is drawn on: %+v", got)
	}
	ed.toggleHear(lane) // the badge press acts on that scene too
	if len(ed.segs[0].Quiet) != 0 {
		t.Errorf("pressing the badge with nothing held did not change the line's scene: %v", ed.segs[0].Quiet)
	}
	for _, c := range []struct {
		what string
		set  func()
	}{
		{"nothing held and the line in no scene", func() { ed.segOn = false; ed.playhead = ed.segs[0].S - 1 }},
		{"an insert held", func() { ed.segs[0].Ins = "sting.mp4" }},
	} {
		ed = hearEd(t)
		c.set()
		if a, s := ed.hearBadgesAud(), ed.hearBadgesSrc(); len(a) > 0 || len(s) > 0 {
			t.Errorf("%s: drew %d band badges and %d picture ones", c.what, len(a), len(s))
		}
	}

	// nor does a camera that filmed no sound: there is no strip under its
	// pictures to draw on, and nothing there to silence
	ed = hearEd(t)
	ed.auds = ed.auds[1:] // a1's own track gone, the rest of the session intact
	if s := ed.hearBadgesSrc(); len(s) > 0 {
		t.Errorf("a silent camera wears %+v", s)
	}
	if a := ed.hearBadgesAud(); len(a) != 1 {
		t.Errorf("and the recorders' band drew %d badges, want the one recorder", len(a))
	}
}

func TestTheWashCoversEveryChannelOfTheRecording(t *testing.T) {
	// the badge carries the lane's whole depth, and the wash has to spend it.
	// Painting one waveLaneH leaves the second half of a stereo recording
	// unmarked, which reads as a lane nobody has decided about yet sitting
	// under one that has been -- and the arithmetic above cannot see it,
	// because the badge it checks was right all along
	src := readSrc(t, "cut_hear.go")
	if !strings.Contains(src, "cr.Rectangle(x0, b.cy-b.h/2, x1-x0, b.h)") {
		t.Error("the wash no longer covers the whole lane it belongs to")
	}
}

func TestGreenIsHeardAndGreyIsNot(t *testing.T) {
	// the whole of what the page says at a glance, and the one thing a swap
	// would leave working perfectly while meaning the opposite. Colours are
	// not reachable from a test, so the branch itself is the claim
	src := readSrc(t, "cut_hear.go")
	for _, w := range []string{
		"if b.on {\n\t\t\tcr.SetSourceRGBA(0.2, 0.85, 0.35, 0.22)",              // the wash over the scene
		"case on:\n\t\tplate(cr, cx, cy, hearR+hearPad, 0.15, 0.65, 0.3, 0.95)", // and the plate both controls share
	} {
		if !strings.Contains(src, w) {
			t.Errorf("heard is no longer the green branch:\n%s", w)
		}
	}
}

func TestPressingABadgeFindsTheLaneUnderIt(t *testing.T) {
	ed := hearEd(t)
	b := ed.hearBadgesAud()[0]
	if got := ed.hearAt(b.cx, b.cy, false); got != "mic" {
		t.Errorf("the middle of mic's badge answered %q", got)
	}
	if got := ed.hearAt(b.cx+hearHit+1, b.cy, false); got != "" {
		t.Errorf("a press %.0f px away answered %q", hearHit+1, got)
	}
	// the two areas draw their lanes in different y, so each only answers for
	// its own: the band's badge is not reachable from the picture area, where
	// that same y is somebody's thumbnail
	if got := ed.hearAt(b.cx, b.cy, true); got == "mic" {
		t.Error("the picture band answered for a badge drawn in the recorders' band")
	}
	src := ed.hearBadgesSrc()[0]
	if got := ed.hearAt(src.cx, src.cy, true); got != "a1" {
		t.Errorf("the middle of a1's badge answered %q", got)
	}
}

// ---- what pressing one does -------------------------------------------------

func TestSilencingALaneIsOneUndoStepThatStays(t *testing.T) {
	ed := hearEd(t)
	ed.toggleHear("mic")
	ed.toggleHear("room")
	if q := ed.segs[0].Quiet; len(q) != 2 || q[0] != "mic" || q[1] != "room" {
		t.Fatalf("two presses left %v, want both silenced", q)
	}
	// pressing a silenced lane turns it back on: the badge is a toggle, not a
	// one-way door, and it is the same badge either way round
	ed.toggleHear("mic")
	if q := ed.segs[0].Quiet; len(q) != 1 || q[0] != "room" {
		t.Fatalf("pressing mic again left %v, want room alone silenced", q)
	}
	if !ed.segs[0].hears("mic") || ed.segs[0].hears("room") {
		t.Error("the scene disagrees with its own list")
	}

	// three presses, three undo steps, and every one of them still says what
	// it said: the snapshots share the segment slice, so a toggle that edited
	// the list in place rather than building a new one would rewrite the
	// history behind it and Undo would put back the state it was already in
	if len(ed.undo) != 3 {
		t.Fatalf("three presses pushed %d undo steps", len(ed.undo))
	}
	for i, want := range [][]string{nil, {"mic"}, {"mic", "room"}} {
		got := ed.undo[i].segs[0].Quiet
		if len(got) != len(want) {
			t.Fatalf("undo step %d says %v, want %v", i, got, want)
		}
		for j := range want {
			if got[j] != want[j] {
				t.Fatalf("undo step %d says %v, want %v", i, got, want)
			}
		}
	}
}

func TestSilencingALaneIsSavedWithTheCut(t *testing.T) {
	ed := hearEd(t)
	ed.toggleHear("mic")
	b, err := os.ReadFile(ed.a.cutPath())
	if err != nil {
		t.Fatalf("read back the cut: %v", err)
	}
	var c cutFile
	if err := json.Unmarshal(b, &c); err != nil {
		t.Fatalf("the saved cut is not JSON: %v", err)
	}
	if len(c.Segs) != 1 || len(c.Segs[0].Quiet) != 1 || c.Segs[0].Quiet[0] != "mic" {
		t.Errorf("the file says %+v", c.Segs)
	}
	// nothing writes the old whole-project field any more: that is what makes
	// the migration below a one-way door rather than something that runs on
	// every load and fights the scenes
	if strings.Contains(string(b), `"sound"`) {
		t.Errorf("the saved cut still carries a whole-project sound choice:\n%s", b)
	}
}

func TestTheBadgesAreDrawnAndPressedInBothBands(t *testing.T) {
	// the wiring, which no arithmetic here can reach: both areas draw them,
	// and both offer a press to hearAt before anything else can claim the
	// same ground -- a badge under a thumbnail or a waveform is not a control
	aud := readSrc(t, "cut_audio.go")
	if !strings.Contains(aud, "ed.drawHearBadges(cr, ed.hearBadgesAud(), vx0, vx1)") {
		t.Error("the recorders' band does not draw its badges")
	}
	cut := readSrc(t, "cut.go")
	if !strings.Contains(cut, "ed.drawHearBadges(cr, ed.hearBadgesSrc(), vx0, vx1)") {
		t.Error("the picture band does not draw its badges")
	}
	press := strings.Index(cut, `if base := ed.hearAt(x+ed.viewX, y, area == ed.srcArea); base != "" {`)
	pics := strings.Index(cut, "if area == ed.srcArea && ed.hitPics(y) {")
	if press < 0 || pics < 0 || press > pics {
		t.Errorf("the badges are offered the press at %d, the thumbnails at %d", press, pics)
	}
}

// ---- a project from before all this -----------------------------------------

func TestAnOldWholeProjectChoiceBecomesEverySceneSilencingTheRest(t *testing.T) {
	vids, auds := heardTracks()
	segs := []cutSeg{{S: 20, E: 30}, {S: 70, E: 80}} // both on the first camera's row

	// chosen: the recorder. Every scene hears it and nothing else
	got, note := migrateSound(segs, "mic", vids, auds)
	for i, s := range got {
		if !s.hears("mic") {
			t.Errorf("scene %d does not hear the lane the project was heard on", i)
		}
		for _, q := range []string{"a1", "a2", "b"} {
			if s.hears(q) {
				t.Errorf("scene %d still hears %s", i, q)
			}
		}
	}
	if !strings.Contains(note, "mic") {
		t.Errorf("the note does not say which lane: %q", note)
	}

	// chosen: a camera, which named the ROW. The first is two recordings in a
	// line and each scene keeps the one it is actually shown from -- silencing
	// a1 in the scene cut from a2 would silence nothing and say the opposite
	got, _ = migrateSound(segs, "a1", vids, auds)
	for i, want := range []string{"a1", "a2"} {
		if !got[i].hears(want) || got[i].hears("mic") || got[i].hears("b") {
			t.Errorf("scene %d silenced %v, want everything but %s", i, got[i].Quiet, want)
		}
	}

	// the original is left alone: reload assigns the result, and a migration
	// that wrote through the slice it was handed would be a second, invisible
	// edit of whatever else still points at it
	if len(segs[0].Quiet) != 0 {
		t.Errorf("the segments handed in came back silenced: %v", segs[0].Quiet)
	}
}

func TestASceneShownFromAnotherCameraKeepsItsOwnSound(t *testing.T) {
	vids, auds := heardTracks()
	// scene two is cut from the second camera, and the project was heard on
	// the first. No arrangement of silences can put one camera's sound under
	// another's picture -- only b's own track reaches a clip cut from b -- so
	// it keeps what it has rather than being migrated into silence
	segs := []cutSeg{{S: 20, E: 30}, {S: 20, E: 30, Cam: 1}}
	got, note := migrateSound(segs, "a1", vids, auds)
	if !got[0].hears("a1") || got[0].hears("b") {
		t.Errorf("the scene on the chosen camera silenced %v", got[0].Quiet)
	}
	if !got[1].hears("b") || got[1].hears("a1") || got[1].hears("mic") {
		t.Errorf("the scene on the other camera silenced %v, want everything but b", got[1].Quiet)
	}
	if !strings.Contains(note, "except 1, which scene is") {
		t.Errorf("the note does not own up to the one scene it could not honour: %q", note)
	}
	if _, n := migrateSound(segs[:1], "a1", vids, auds); strings.Contains(n, "except") {
		t.Errorf("a cut with nothing to own up to still apologises: %q", n)
	}
}

func TestTheMigrationLeavesAnythingItIsNotFor(t *testing.T) {
	vids, auds := heardTracks()
	for _, c := range []struct {
		what string
		snd  string
		segs []cutSeg
	}{
		{"a project that never made the choice", "", []cutSeg{{S: 20, E: 30}}},
		{"a project with the choice but no cut", "mic", nil},
	} {
		got, note := migrateSound(c.segs, c.snd, vids, auds)
		if note != "" || len(got) != len(c.segs) {
			t.Errorf("%s: %d scenes and the note %q", c.what, len(got), note)
		}
		for i, s := range got {
			if len(s.Quiet) != 0 {
				t.Errorf("%s: scene %d silenced %v", c.what, i, s.Quiet)
			}
		}
	}

	// a scene that already answers for itself is the truth, and the old field
	// beside it is a leftover from a file written before the move. Only a cut
	// saved by hand-editing can be in that state, and it must survive it
	segs := []cutSeg{{S: 20, E: 30, Quiet: []string{"a1"}}, {S: 70, E: 80}}
	got, _ := migrateSound(segs, "mic", vids, auds)
	if len(got[0].Quiet) != 1 || got[0].Quiet[0] != "a1" {
		t.Errorf("a scene that had already been told silenced %v", got[0].Quiet)
	}
	if got[1].hears("a2") {
		t.Errorf("the scene beside it was skipped too: %v", got[1].Quiet)
	}
}

func TestTheOldChoiceIsReadOnceAndNeverWritten(t *testing.T) {
	// the field survives only so an old project opens the way it was left. It
	// is read in reload, spread across the scenes, and left out of every save
	// -- which is what makes this a migration rather than a second model of
	// the same thing running alongside the first
	cut := readSrc(t, "cut.go")
	prod := readSrc(t, "produce.go")
	if !strings.Contains(cut, "migrateSound(ed.segs, c.Sound, ed.vids, ed.auds)") {
		t.Error("reload no longer migrates the old choice")
	}
	for _, f := range []struct{ name, src string }{{"cut.go", cut}, {"produce.go", prod}} {
		for _, bad := range []string{`\bSound:`, `\bed\.snd\b`, `cutFile\{ed\.segs`} {
			if m := regexp.MustCompile(bad).FindString(f.src); m != "" {
				t.Errorf("%s still writes the whole-project choice (%s)", f.name, m)
			}
		}
	}
}

// ---- the whole lane ---------------------------------------------------------

// The switch on a lane's name plate is the badges written to every scene at
// once. It shows on while any scene still hears the lane, because that scene
// is what pressing it would take away, and off only when none does.
func TestALanesSwitchIsWhetherTheCutHearsItAnywhere(t *testing.T) {
	ed := hearEd(t)
	ed.segs = []cutSeg{{S: 20, E: 40}, {S: 50, E: 70}, {S: 80, E: 90}}
	if !ed.laneHeard("mic") {
		t.Error("a cut that silences nothing does not hear the mic")
	}
	ed.segs[0].Quiet = []string{"mic"}
	ed.segs[1].Quiet = []string{"mic"}
	if !ed.laneHeard("mic") {
		t.Error("one scene out of three still hearing it reads as off")
	}
	ed.segs[2].Quiet = []string{"mic"}
	if ed.laneHeard("mic") {
		t.Error("every scene silences it and the switch still reads on")
	}
	// an insert has no say either way: it brings its own sound
	ed.segs = append(ed.segs, cutSeg{S: 95, E: 99, Ins: "card.svg"})
	if ed.laneHeard("mic") {
		t.Error("an insert was read as a scene that hears the lane")
	}
	// and an empty cut hears nothing, which is what the switch shows
	ed.segs = nil
	if ed.laneHeard("mic") {
		t.Error("a cut with no scenes hears a lane")
	}
}

// Pressing it rewrites every scene, in one undo step, with a fresh list each
// -- the snapshot shares the strings, so growing one in place would rewrite
// the history it was taken from.
func TestTheLaneSwitchTurnsEverySceneAtOnce(t *testing.T) {
	ed := hearEd(t)
	ed.segs = []cutSeg{{S: 20, E: 40}, {S: 50, E: 70},
		{S: 75, E: 78, Ins: "card.svg"}, {S: 80, E: 90}}
	ed.segs[1].Quiet = []string{"mic"} // one already off: the press still means off
	before := len(ed.undo)

	ed.toggleLaneAll("mic")
	for i, s := range ed.segs {
		if s.isInsert() {
			if len(s.Quiet) != 0 {
				t.Errorf("the insert was given a silence: %v", s.Quiet)
			}
			continue
		}
		if s.hears("mic") {
			t.Errorf("scene %d still hears the mic after the switch went off", i+1)
		}
	}
	if got := len(ed.undo) - before; got != 1 {
		t.Errorf("one press pushed %d undo entries, want 1", got)
	}
	// ...and back on, every scene, including the one that was off to start
	ed.toggleLaneAll("mic")
	for i, s := range ed.segs {
		if !s.isInsert() && !s.hears("mic") {
			t.Errorf("scene %d is still silent after the switch came back on", i+1)
		}
	}
	// the other lane was never touched
	ed.segs[0].Quiet = append(ed.segs[0].Quiet, "a1")
	ed.toggleLaneAll("mic")
	if ed.segs[0].hears("a1") {
		t.Errorf("switching the mic took another lane with it: %v", ed.segs[0].Quiet)
	}

	// undo puts the whole press back, which is what one entry for it means
	ed = hearEd(t)
	ed.segs = []cutSeg{{S: 20, E: 40}, {S: 50, E: 70}}
	ed.toggleLaneAll("mic")
	ed.undoLast()
	for i, s := range ed.segs {
		if !s.hears("mic") {
			t.Errorf("after undo scene %d is still silent: %v", i+1, s.Quiet)
		}
	}

	// an empty cut has nothing to switch, and says so rather than doing
	// nothing quietly
	ed.segs = nil
	ed.toggleLaneAll("mic")
	if len(ed.undo) != 0 {
		t.Error("an empty cut pushed an undo entry for a switch that changed nothing")
	}
}

// It sits on the name plate, one per recording, in widget coordinates -- the
// band scrolls under it and the switch does not -- centred on the recording's
// whole depth exactly as its badge is.
func TestTheLaneSwitchIsOnTheNamePlate(t *testing.T) {
	ed := hearEd(t)
	ed.auds[3].chans = 2 // the mic in stereo: two waveforms, one switch
	ed.auds = append(ed.auds, tlAudio{base: "room", path: "/f/room.wav", start: 0, dur: 120, chans: 1})

	sw := ed.laneSwitches()
	if len(sw) != 2 {
		t.Fatalf("%d switches for two recordings: %+v", len(sw), sw)
	}
	if sw[0].base != "mic" || sw[1].base != "room" {
		t.Errorf("the switches are %q and %q", sw[0].base, sw[1].base)
	}
	// the mic's two lanes, then the gap, then the room's one
	if want := wavePad + waveLaneH; sw[0].cy != want {
		t.Errorf("the stereo mic's switch is at y %g, want %g -- the middle of both its lanes", sw[0].cy, want)
	}
	if want := wavePad + 2*waveLaneH + waveGap + waveLaneH/2; sw[1].cy != want {
		t.Errorf("the room's switch is at y %g, want %g", sw[1].cy, want)
	}
	for _, s := range sw {
		if s.cx != laneSwX {
			t.Errorf("%s's switch is at x %g, want the plate's %g", s.base, s.cx, laneSwX)
		}
		if !s.on {
			t.Errorf("%s reads off on a cut that silences nothing", s.base)
		}
	}
	// the press lands on it, and misses cleanly beside it
	if got := ed.laneSwitchAt(sw[0].cx, sw[0].cy); got != "mic" {
		t.Errorf("a press on the mic's switch found %q", got)
	}
	if got := ed.laneSwitchAt(sw[0].cx+hearHit+1, sw[0].cy); got != "" {
		t.Errorf("a press clear of every switch found %q", got)
	}
	// the names are indented past it, or one would be drawn over the other
	if laneNameX <= laneSwX+hearR+hearPad {
		t.Errorf("a lane's name starts at %g, over its switch at %g", laneNameX, laneSwX)
	}
	src := readSrc(t, "cut_audio.go")
	if !strings.Contains(src, "ed.drawLaneSwitches(cr)") {
		t.Error("the band does not draw the switches")
	}
	if !strings.Contains(src, "plateText(cr, laneNameX, y+12, name)") {
		t.Error("a lane's name is not indented past its switch")
	}
	// and the press asks for it in the band, before the scene badge that can
	// share the same pixels at the left of the view
	cut := readSrc(t, "cut.go")
	i := strings.Index(cut, "if base := ed.laneSwitchAt(x+ed.viewX, y); base != \"\" {")
	j := strings.Index(cut, "if base := ed.hearAt(x+ed.viewX, y, area == ed.srcArea); base != \"\" {")
	if i < 0 || j < 0 || i > j {
		t.Errorf("the whole-lane switch is not asked in the band before the scene's badge (switch %d, badge %d)", i, j)
	}
	if !strings.Contains(cut, "ed.toggleLaneAll(base)") {
		t.Error("nothing presses the switch")
	}
	// both controls draw the one plate, so the mark cannot mean two things
	hear := readSrc(t, "cut_hear.go")
	if strings.Count(hear, "func litPlate(") != 1 || !strings.Contains(hear, "hearPlate(cr, b.cx, b.cy, b.on,") {
		t.Error("the scene badge and the lane switch no longer share hearPlate")
	}
}

// ---- and every one of them lights under the pointer ---------------------------

// The speakers and the lens are buttons, and until now they were the only
// plated controls on the page that did not say so: the ✕ badges light red under
// the pointer and the fold's + lights blue, while a switch looked exactly the
// same whether the hand was on it or a hand's width away.
func TestEverySpeakerAndLensLightsUnderThePointer(t *testing.T) {
	ed := hearEd(t)

	// the recorders' band: its switch, and the held scene's own badge
	sw := ed.laneSwitches()[0]
	ed.hoverBadges(sw.cx-ed.viewX, sw.cy, false)
	if !ed.badgeHot(badgeID{kind: badgeSwitch, base: sw.base}) {
		t.Errorf("the pointer on %s's switch lit %+v", sw.base, ed.badgeHov)
	}
	b := ed.hearBadgesAud()[0]
	ed.hoverBadges(b.cx-ed.viewX, b.cy, false)
	if !ed.badgeHot(badgeID{kind: badgeHear, base: b.base}) {
		t.Errorf("the pointer on the scene's badge for %s lit %+v", b.base, ed.badgeHov)
	}
	// and the switch is a button to the pointer as well, like the row's own
	// (wantCursor): it stands in the gutter, where nothing on the tape is
	if got := ed.wantLaneCursor(sw.cx-ed.viewX, sw.cy); got != "pointer" {
		t.Errorf("the pointer over a lane's switch is %q, want %q", got, "pointer")
	}

	// the picture band: the row's switch, and the lens that says which camera
	ps := ed.pairSwitches()[0]
	ed.hoverBadges(ps.cx-ed.viewX, ps.cy, true)
	if !ed.badgeHot(badgeID{kind: badgePair, base: ps.bases[0]}) {
		t.Errorf("the pointer on row 0's switch lit %+v", ed.badgeHov)
	}
	cb := ed.camBadges()[1]
	ed.hoverBadges(cb.cx-ed.viewX, cb.cy, true)
	if !ed.badgeHot(badgeID{kind: badgeCam, row: cb.row}) {
		t.Errorf("the pointer on row %d's lens lit %+v", cb.row, ed.badgeHov)
	}

	// clear of them all, and off the band, nothing is lit: a badge left
	// standing blue after the pointer has gone is a button that looks pressed
	ed.hoverBadges(sw.cx-ed.viewX+3*hearHit, sw.cy, false)
	if ed.badgeHov != (badgeID{}) {
		t.Errorf("a point clear of every badge lit %+v", ed.badgeHov)
	}
	ed.hoverBadges(ps.cx-ed.viewX, ps.cy, true)
	ed.hoverBadges(-1, -1, true)
	if ed.badgeHov != (badgeID{}) {
		t.Errorf("the pointer left the band and %+v stayed lit", ed.badgeHov)
	}

	// one blue for all of them, the fold's included, so a lit badge means the
	// same thing wherever it is
	hear := readSrc(t, "cut_hear.go")
	if strings.Count(hear, "func hotPlate(") != 1 {
		t.Error("the hover colour has been copied instead of shared")
	}
	if !strings.Contains(readSrc(t, "cut_fold.go"), "hotPlate(cr, cx, cy, segKillR+segKillPad)") {
		t.Error("the fold's badge no longer wears the shared hover plate")
	}
	// and every one of the four draws asks whether it is the one
	for _, want := range []string{
		"hearPlate(cr, s.cx, s.cy, s.on, ed.badgeHot(badgeID{kind: badgeSwitch",
		"hearPlate(cr, s.cx, s.cy, s.on, ed.badgeHot(badgeID{kind: badgePair",
		"hearPlate(cr, b.cx, b.cy, b.on, ed.badgeHot(badgeID{kind: badgeHear",
		"camPlate(cr, b.cx, b.cy, b.on, ed.badgeHot(badgeID{kind: badgeCam",
	} {
		if !strings.Contains(hear, want) {
			t.Errorf("a badge is drawn without asking whether the pointer is on it: %q", want)
		}
	}
}

// The switches are drawn over the lane names. A name is a label and a switch is
// a button, and the label was painted last -- so a name that reached the gutter
// was drawn across the button it names.
func TestTheSwitchesArePaintedOverTheNames(t *testing.T) {
	src := readSrc(t, "cut_audio.go")
	name := strings.Index(src, "plateText(cr, laneNameX, y+12, name)")
	sw := strings.Index(src, "ed.drawLaneSwitches(cr)")
	if name < 0 || sw < 0 || sw < name {
		t.Errorf("the band paints its names over its switches (name %d, switch %d)", name, sw)
	}
	// the picture band already did, and has to go on doing it
	cut := readSrc(t, "cut.go")
	if i, j := strings.Index(cut, "plateText(cr, ed.viewX+laneNameX,"), strings.Index(cut, "ed.drawPairSwitches(cr)"); i < 0 || j < 0 || j < i {
		t.Errorf("the picture band paints its names over its switches (name %d, switch %d)", i, j)
	}
}

// ---- and the same switch for the footage's own sound -------------------------

// The strip under a row of pictures wears one too. It did not at first, and the
// gap was one-sided: a recorder beside the camera could be taken out of the
// video in a press, while the camera's own sound could only be silenced scene
// by scene. One question, one control, both bands.
//
// It is a switch per ROW, over every recording whose pictures are on it: a
// camera stopped and started again is several files in a line, and "this
// camera's sound, off" must not mean "off until it restarted".
func TestTheFootagesOwnSoundHasAWholeCutSwitch(t *testing.T) {
	ed := hearEd(t) // a1 and a2 share row 0; b is row 1; mic is a recorder
	sw := ed.pairSwitches()
	if len(sw) != 2 {
		t.Fatalf("%d switches for two camera rows: %+v", len(sw), sw)
	}
	if got := sw[0].bases; len(got) != 2 || got[0] != "a1" || got[1] != "a2" {
		t.Errorf("row 0's switch stands for %v, want both of its recordings", got)
	}
	if got := sw[1].bases; len(got) != 1 || got[0] != "b" {
		t.Errorf("row 1's switch stands for %v, want b alone", got)
	}
	// on the strip, not on the pictures: under the row's thumbnails, inside
	// the strip's own depth, and at the same x a recorded lane's sits at
	for i, s := range sw {
		top := ed.laneTop(i) + ed.laneH()
		if s.cy <= top || s.cy >= top+ed.pairH(i) {
			t.Errorf("row %d's switch is at y %g, outside its strip %g–%g",
				i, s.cy, top, top+ed.pairH(i))
		}
		if s.cx != laneSwX {
			t.Errorf("row %d's switch is at x %g, want the band's own %g", i, s.cx, laneSwX)
		}
		if !s.on {
			t.Errorf("row %d reads off on a cut that silences nothing", i)
		}
	}
	// a row whose footage filmed no sound has no strip and no switch
	silent := axisEd(t, tlVideo{base: "q", path: "/f/q.mp4", start: 0, dur: 30})
	if got := silent.pairSwitches(); len(got) != 0 {
		t.Errorf("a silent camera grew %d switch(es)", len(got))
	}

	// the press lands on it, and misses cleanly beside it
	if got := ed.pairSwitchAt(sw[0].cx, sw[0].cy); len(got) != 2 {
		t.Errorf("a press on row 0's switch found %v", got)
	}
	if got := ed.pairSwitchAt(sw[0].cx+hearHit+1, sw[0].cy); got != nil {
		t.Errorf("a press clear of every switch found %v", got)
	}
}

// Pressing it takes the whole row out of the video: every scene, both of its
// recordings, one undo step -- and back on the same way.
func TestThePairSwitchTakesTheRowOutOfTheWholeCut(t *testing.T) {
	ed := hearEd(t)
	ed.segs = []cutSeg{{S: 20, E: 40}, {S: 60, E: 80},
		{S: 85, E: 88, Ins: "card.svg"}}
	bases := ed.pairSwitches()[0].bases
	before := len(ed.undo)

	ed.toggleLanesAll(bases, pairSwitchName(bases))
	for i, s := range ed.segs {
		if s.isInsert() {
			if len(s.Quiet) != 0 {
				t.Errorf("the insert was given a silence: %v", s.Quiet)
			}
			continue
		}
		for _, b := range bases {
			if s.hears(b) {
				t.Errorf("scene %d still hears %s after the row went off", i+1, b)
			}
		}
	}
	if got := len(ed.undo) - before; got != 1 {
		t.Errorf("one press pushed %d undo entries, want 1", got)
	}
	if ed.pairSwitches()[0].on {
		t.Error("the switch still reads on with every scene silencing the row")
	}
	// the recorder beside the camera was not touched
	if !ed.segs[0].hears("mic") {
		t.Errorf("switching the row took the mic with it: %v", ed.segs[0].Quiet)
	}
	// half a row silenced by hand still reads on, and the press finishes it
	// rather than turning the rest off and this half back on
	ed.undoLast()
	ed.segs[0].Quiet = []string{"a1"}
	if !ed.pairSwitches()[0].on {
		t.Error("a row heard in one scene and not another reads off")
	}
	ed.toggleLanesAll(bases, pairSwitchName(bases))
	for i, s := range ed.segs {
		if !s.isInsert() && (s.hears("a1") || s.hears("a2")) {
			t.Errorf("scene %d survived the press: %v", i+1, s.Quiet)
		}
	}
	// and back on puts every scene back, the half-off one included
	ed.toggleLanesAll(bases, pairSwitchName(bases))
	for i, s := range ed.segs {
		if !s.isInsert() && (!s.hears("a1") || !s.hears("a2")) {
			t.Errorf("scene %d is still silent after the switch came back on: %v", i+1, s.Quiet)
		}
	}
	// one lane's press is still the one-lane sentence: the row's name is only
	// for a row of several
	if got := pairSwitchName([]string{"b"}); got != "b" {
		t.Errorf("a single recording's switch calls itself %q", got)
	}
}

// On screen, and pressable, at the left of the strip: lit where the sound is
// in the video, dark where no scene keeps it. The permanent control is drawn
// over the per-scene badge and asked before it, because the two can want the
// same pixels when a scene starts at the very left of the view.
func TestThePairSwitchIsDrawnOnTheStrip(t *testing.T) {
	ed := pairEd(t) // one stereo camera on row 0, one silent on row 1
	ed.segs = []cutSeg{{S: 5, E: 40}, {S: 55, E: 90}}
	ed.playhead, ed.hasPlay = 20, true
	sw := ed.pairSwitches()[0]
	const w, h = 620, 220

	lit := func() (uint8, uint8, uint8) {
		// off the centre of the plate: the speaker itself is white on both
		return renderTrack(t, ed, w, h)(int(sw.cx), int(sw.cy)-5)
	}
	r, g, b := lit()
	if !(int(g) > int(r)+40 && int(g) > int(b)+40) {
		t.Errorf("the switch reads rgb(%d,%d,%d) on a cut that hears the camera, want the lit plate", r, g, b)
	}
	for i := range ed.segs {
		ed.segs[i].Quiet = []string{"a"}
	}
	r, g, b = lit()
	if int(g) > int(r)+40 {
		t.Errorf("the switch is still lit at rgb(%d,%d,%d) with the camera silenced everywhere", r, g, b)
	}

	src := readSrc(t, "cut.go")
	if !strings.Contains(src, "ed.drawPairSwitches(cr)") {
		t.Error("the picture band does not draw the strip's switches")
	}
	i := strings.Index(src, "if bases := ed.pairSwitchAt(x+ed.viewX, y); len(bases) > 0 {")
	j := strings.Index(src, "if base := ed.hearAt(x+ed.viewX, y, area == ed.srcArea); base != \"\" {")
	if i < 0 || j < 0 || i > j {
		t.Errorf("the strip's switch is not asked before the scene's badge (switch %d, badge %d)", i, j)
	}
	if !strings.Contains(src, "ed.toggleLanesAll(bases, pairSwitchName(bases))") {
		t.Error("nothing presses the strip's switch")
	}
	// one plate for all three controls: a scene's badge, a lane's switch and
	// a row's, or the mark would mean three things
	hear := readSrc(t, "cut_hear.go")
	if strings.Count(hear, "func litPlate(") != 1 {
		t.Error("the plate has been copied instead of shared")
	}
}
