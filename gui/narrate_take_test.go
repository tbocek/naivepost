package main

// The seconds a voice is cloned from, chosen by hand.
//
// The automatic pick (narrate_ref.go) has its own tests and they are about
// ranking. These are about the other thing: what happens when somebody has
// listened to the clone, disagreed, and drawn a box on the wave instead.

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
)

// A set of takes is a set of SECONDS, not a list of gestures. Two drags that
// overlap are one stretch, two that touch are one stretch, and the order they
// were made in is not the order they are cut in -- a reference assembled from
// the raw list would say the overlap twice and jump backwards in the middle.
func TestTakesAreOneStretchWhereverTheyMeet(t *testing.T) {
	for _, c := range []struct {
		name string
		in   []voiceTake
		want []voiceTake
	}{
		{"already apart", []voiceTake{{2, 6}, {10, 15}}, []voiceTake{{2, 6}, {10, 15}}},
		{"out of order", []voiceTake{{10, 15}, {2, 6}}, []voiceTake{{2, 6}, {10, 15}}},
		{"overlapping", []voiceTake{{2, 8}, {6, 12}}, []voiceTake{{2, 12}}},
		{"touching", []voiceTake{{2, 6}, {6, 11}}, []voiceTake{{2, 11}}},
		{"one inside another", []voiceTake{{2, 20}, {6, 9}}, []voiceTake{{2, 20}}},
		{"a click that travelled", []voiceTake{{2, 6}, {9, 9.05}}, []voiceTake{{2, 6}}},
		{"a negative start", []voiceTake{{-4, 6}}, nil},
	} {
		if got := cleanTakes(c.in); !sameTakes(got, c.want) {
			t.Errorf("%s: %v became %v, wanted %v", c.name, c.in, got, c.want)
		}
	}
}

// ＋ goes through the same rule, so a stretch added across two existing ones
// leaves one behind rather than three overlapping.
func TestAddingATakeAcrossTwoLeavesOne(t *testing.T) {
	got := addTake([]voiceTake{{2, 6}, {12, 16}}, 5, 13)
	if !sameTakes(got, []voiceTake{{2, 16}}) {
		t.Errorf("adding 5-13 to 2-6 and 12-16 gave %v", got)
	}
}

// － takes SECONDS back out, which is not the same as removing the takes that
// touch them. A selection dragged over the middle of a long take means "not
// that bit", and dropping the whole take would throw away the two good halves
// either side of it -- which is exactly the stretch somebody is trimming a
// cough out of.
func TestRemovingTheMiddleOfATakeKeepsBothHalves(t *testing.T) {
	got := dropTakes([]voiceTake{{0, 30}}, 10, 12)
	if !sameTakes(got, []voiceTake{{0, 10}, {12, 30}}) {
		t.Errorf("cutting 10-12 out of 0-30 gave %v", got)
	}
	// ...and what is left too short to be a take is not left behind as one
	if got := dropTakes([]voiceTake{{0, 30}}, 0.2, 30); !sameTakes(got, nil) {
		t.Errorf("a 0.2 s remainder survived as %v", got)
	}
	// a selection over nothing changes nothing
	if got := dropTakes([]voiceTake{{2, 6}}, 40, 50); !sameTakes(got, []voiceTake{{2, 6}}) {
		t.Errorf("removing seconds nobody picked gave %v", got)
	}
}

// The takes belong to the RECORDING, not to the narrator slot: a slot is a tag
// on the Prepare page and can be moved to somebody else, and the seconds a
// person sounds clearest in must not be handed to whoever is tagged next.
// They also outlive the App that wrote them -- this is a project's answer, and
// it is read back from the folder.
func TestTheTakesBelongToTheRecordingAndSurviveARestart(t *testing.T) {
	ownConfig(t)
	dir := t.TempDir()
	a := &App{outDir: dir}
	a.selNarr[0] = "/recordings/tom.wav"

	if err := a.setTakesFor("tom", []voiceTake{{12, 20}, {40, 46}}); err != nil {
		t.Fatal(err)
	}
	if !exists(filepath.Join(dir, "narrate", "takes.json")) {
		t.Fatal("nothing was written")
	}
	// a fresh App, which is what a restart is
	b := &App{outDir: dir}
	b.selNarr[0] = "/recordings/tom.wav"
	if got := b.takesFor("tom"); !sameTakes(got, []voiceTake{{12, 20}, {40, 46}}) {
		t.Errorf("after a restart the takes read back as %v", got)
	}
	// the chosen voice resolves through the slot to that recording
	if got := b.voiceTakes(); !sameTakes(got, []voiceTake{{12, 20}, {40, 46}}) {
		t.Errorf("narrator 1's voice takes are %v", got)
	}
	// re-tagging the slot to somebody else does not hand them over
	b.selNarr[0] = "/recordings/ann.wav"
	if got := b.voiceTakes(); len(got) != 0 {
		t.Errorf("ann inherited tom's takes: %v", got)
	}
}

// The status line after every ＋ and － is a number of seconds, and "is that
// enough to clone from" is the only question the picture cannot answer -- so it
// has to be seconds and not a count of drags.
func TestThePickedSecondsAddUpToSeconds(t *testing.T) {
	if got := takesTotal([]voiceTake{{2, 6}, {10, 15}}); got != 9 {
		t.Errorf("4 s and 5 s came to %v", got)
	}
	if got := takesTotal(nil); got != 0 {
		t.Errorf("nothing picked came to %v", got)
	}
	// and the same seconds cut differently are still the same seconds, which is
	// NOT what tells a ＋ or a － that something happened
	a, b := []voiceTake{{0, 4}, {6, 10}}, []voiceTake{{0, 2}, {4, 10}}
	if takesTotal(a) != takesTotal(b) {
		t.Fatal("the case this is about needs two sets that add up the same")
	}
	if sameTakes(a, b) {
		t.Errorf("%v and %v were called the same takes", a, b)
	}
	if !sameTakes(a, []voiceTake{{0, 4}, {6, 10}}) {
		t.Error("a set is not the same as itself")
	}
	if sameTakes(nil, []voiceTake{{0, 1}}) {
		t.Error("nothing was called the same as a take")
	}
	// and it is what － asks before it throws the built reference away: a drag
	// over seconds nobody picked must say so, not rewrite the same set and send
	// the clone back to ffmpeg
	if body := funcBody(t, "narrate_takeband.go", `func \(b \*takeBand\) delClicked\(`); !strings.Contains(body, "sameTakes(next, b.takes)") {
		t.Errorf("－ no longer checks whether anything changed:\n%s", body)
	}
}

// Clearing the picks puts the recording back to having none, which is not the
// same as having an empty list: takes.json is the project's answer about every
// recording it names, and a name left behind with nothing under it says
// somebody picked seconds here and they are gone, when nobody ever did.
func TestClearingThePicksLeavesTheRecordingUnnamed(t *testing.T) {
	ownConfig(t)
	dir := t.TempDir()
	a := &App{outDir: dir}

	if err := a.setTakesFor("tom", []voiceTake{{12, 20}}); err != nil {
		t.Fatal(err)
	}
	if err := a.setTakesFor("tom", nil); err != nil {
		t.Fatal(err)
	}
	b, err := os.ReadFile(filepath.Join(dir, "narrate", "takes.json"))
	if err != nil {
		t.Fatal(err)
	}
	if strings.Contains(string(b), "tom") {
		t.Errorf("the recording is still named after its last take went:\n%s", b)
	}
	if got := (&App{outDir: dir}).takesFor("tom"); len(got) != 0 {
		t.Errorf("after a restart it reads back as %v", got)
	}
}

// Editing the takes has to throw the built reference away, or the next line
// spoken is cloned from the seconds that were just replaced.
func TestChangingTheTakesDropsTheReferenceBuiltFromTheOldOnes(t *testing.T) {
	ownConfig(t)
	dir := t.TempDir()
	a := &App{outDir: dir}
	if err := os.MkdirAll(a.narrateDir(), 0o755); err != nil {
		t.Fatal(err)
	}
	for _, f := range []string{a.refBase(), a.refPath()} {
		if err := os.WriteFile(f, []byte("old"), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	if err := a.setTakesFor("tom", []voiceTake{{1, 9}}); err != nil {
		t.Fatal(err)
	}
	for _, f := range []string{a.refBase(), a.refPath()} {
		if exists(f) {
			t.Errorf("%s survived a change of takes", filepath.Base(f))
		}
	}
}

// The takes are part of WHO IS SPEAKING, so they are part of the cache key --
// the same argument the pitch slider makes. Without this, picking new seconds
// and pressing ▶ replays the sample spoken from the old ones, which is a
// control that appears to do nothing.
func TestHandPickingSecondsIsADifferentVoiceToTheCache(t *testing.T) {
	ownConfig(t)
	dir := t.TempDir()
	a := &App{outDir: dir}
	a.selNarr[0] = "/recordings/tom.wav"

	// a project that has never touched this keeps exactly the key it had
	// before the feature existed
	if got := a.voiceKey(); got != ownVoice {
		t.Errorf("an untouched project's voice key is %q, not %q", got, ownVoice)
	}
	if err := a.setTakesFor("tom", []voiceTake{{12, 20}}); err != nil {
		t.Fatal(err)
	}
	one := a.voiceKey()
	if one == ownVoice || !strings.HasPrefix(one, ownVoice+"#") {
		t.Fatalf("hand-picked takes did not move the key: %q", one)
	}
	// and moving an edge moves it again
	if err := a.setTakesFor("tom", []voiceTake{{12, 21}}); err != nil {
		t.Fatal(err)
	}
	if two := a.voiceKey(); two == one {
		t.Errorf("widening a take left the key at %q", two)
	}
	// while putting them back gives the old key back, so switching away and
	// back does not orphan what was spoken
	if err := a.setTakesFor("tom", []voiceTake{{12, 20}}); err != nil {
		t.Fatal(err)
	}
	if back := a.voiceKey(); back != one {
		t.Errorf("the same takes gave a different key: %q vs %q", back, one)
	}
	if err := a.setTakesFor("tom", nil); err != nil {
		t.Fatal(err)
	}
	if got := a.voiceKey(); got != ownVoice {
		t.Errorf("clearing the takes left the key at %q", got)
	}
}

// There is nothing to pick from unless the voice is one of the session's own
// recordings: a wav in the voices folder IS a reference and is used whole, and
// "no audio" has no voice at all. The band and its buttons are absent in both
// cases rather than present and inert.
func TestOnlyANarratorHasSecondsToPickFrom(t *testing.T) {
	ownConfig(t)
	a := &App{outDir: t.TempDir()}
	a.selNarr[0] = "/recordings/tom.wav"
	a.voiceSel = ownVoice
	if got := a.takeSource(); got != "/recordings/tom.wav" {
		t.Errorf("narrator 1 is cut from %q", got)
	}
	for _, id := range []string{captionsVoice, "some-cc0-sample"} {
		a.voiceSel = id
		if got := a.takeSource(); got != "" {
			t.Errorf("voice %q offered %q to pick from", id, got)
		}
	}
}

// A hand-picked set is taken WHOLE. The caps that keep a guess honest --
// fourteen seconds over three pieces -- are there because a guess needs
// keeping honest, and there is nothing left to keep honest once somebody has
// listened to the result. It also needs no diarization, which is what lets a
// recording nobody has run Prepare over be cloned from at all.
func TestAHandPickedReferenceIsTakenWholeAndNeedsNoPrepare(t *testing.T) {
	ownConfig(t)
	dir, src := takeFixture(t, 40)
	a := &App{outDir: filepath.Join(dir, "out"), curCmds: map[*exec.Cmd]bool{}}
	a.selNarr[0] = src
	a.voiceSel = ownVoice

	// five pieces of five seconds: more than refTakeMax, and 25 s against a
	// refWant of 14. Nothing about it survives the automatic budget.
	var ts []voiceTake
	for i := 0; i < 5; i++ {
		ts = append(ts, voiceTake{S: float64(i) * 8, E: float64(i)*8 + 5})
	}
	if err := a.setTakesFor("tom", ts); err != nil {
		t.Fatal(err)
	}
	// no inputs/ anywhere: there is no diarization to fall back on, so a
	// reference at all is the claim
	if err := a.ensureVoiceBase(); err != nil {
		t.Fatalf("no reference from hand-picked takes: %v", err)
	}
	got, err := ffprobeDur(a.refBase())
	if err != nil {
		t.Fatal(err)
	}
	if got < 24 || got > 26 {
		t.Errorf("the reference is %.1f s; all 25 s of the takes should be in it", got)
	}
}

// ▶ walks the takes: each one's end is what cues the next, and the walk lets
// go of the player the moment anything else is in it -- which is how speaking
// a sample cancels it without the sample having to know this band exists.
func TestPlayingTheTakesWalksThemAndLetsGoWhenTheSampleTakesOver(t *testing.T) {
	const src = "/recordings/tom.wav"
	if got := takeNext(src, src, false, 0, 3); got != 0 {
		t.Errorf("a take still sounding moved the walk to %d", got)
	}
	if got := takeNext(src, src, true, 0, 3); got != 1 {
		t.Errorf("the first take ending gave %d, wanted the second", got)
	}
	if got := takeNext(src, src, true, 2, 3); got != -1 {
		t.Errorf("the last take ending gave %d, wanted a stop", got)
	}
	// the sample is in the player now, still playing: the walk is over anyway
	if got := takeNext("/narrate/samples/x.wav", src, false, 1, 3); got != -1 {
		t.Errorf("the walk held on to a player showing something else: %d", got)
	}
}

// The red bar is where ▶ starts. It does not replace what ▶ plays -- the takes,
// back to back -- it says which of them to start at, and the one it lands
// inside starts there rather than at its own beginning.
//
// The other half is the case the picking has not happened in yet: with nothing
// picked after the bar there is only one thing the press can mean, and playing
// the recording from there is how the seconds worth picking get found at all.
func TestPlayStartsAtTheRedBar(t *testing.T) {
	ts := []voiceTake{{10, 20}, {30, 40}, {50, 60}}
	for _, c := range []struct {
		name  string
		at    float64
		want  []voiceTake
		takes bool
	}{
		{"no bar plays them all", -1, ts, true},
		{"a bar at the start plays them all", 0, ts, true},
		{"a bar between two starts at the next", 25, []voiceTake{{30, 40}, {50, 60}}, true},
		{"a bar inside one starts THERE", 35, []voiceTake{{35, 40}, {50, 60}}, true},
		{"a bar on an edge does not replay the one behind it", 20, []voiceTake{{30, 40}, {50, 60}}, true},
		{"past the last take, the recording from there", 70, []voiceTake{{70, 90}}, false},
		{"a bar at the very end plays nothing", 90, nil, false},
	} {
		got, takes := takeQueue(ts, c.at, 90)
		if !sameTakes(got, c.want) || takes != c.takes {
			t.Errorf("%s: bar at %v gave %v (takes=%v), wanted %v (takes=%v)",
				c.name, c.at, got, takes, c.want, c.takes)
		}
	}
	// nothing picked at all: the bar is the only thing ▶ can be about, and
	// without one there is nothing for it to do
	if got, takes := takeQueue(nil, 12, 90); !sameTakes(got, []voiceTake{{12, 90}}) || takes {
		t.Errorf("with no takes, a bar at 12 gave %v (takes=%v)", got, takes)
	}
	if got, _ := takeQueue(nil, -1, 90); got != nil {
		t.Errorf("with no takes and no bar, ▶ found %v to play", got)
	}
	// and a recording whose length has not been probed yet has no seconds to
	// play from, so a click landing before the decode cannot start anything
	if got, _ := takeQueue(nil, 5, 0); got != nil {
		t.Errorf("a file of unknown length offered %v", got)
	}
}

// One button, two faces: ▶ while nothing is walking, ⏹ while the takes play,
// and a press on the ⏹ ends the walk instead of starting it over. The face is
// read off the QUEUE rather than off the player, because the player pauses at
// every join between takes and a face drawn from that would blink.
func TestTheBandsPlayButtonBecomesAStopWhileTheTakesPlay(t *testing.T) {
	play := funcBody(t, "narrate_takeband.go", `func \(b \*takeBand\) playClicked\(`)
	if !strings.Contains(play, "if len(b.queue) > 0 {\n\t\tb.stopWalk()") {
		t.Errorf("a press while the takes play does not stop them:\n%s", play)
	}
	// and neither face waits for the player to report it. A segment that never
	// prerolls would otherwise leave a ▶ up over a queue that the next press
	// then STOPS, which is the one thing a transport button must never do.
	if !strings.Contains(play, "b.syncPlayBtn()") {
		t.Errorf("the ⏹ waits for the player to say the takes started:\n%s", play)
	}
	face := funcBody(t, "narrate_takeband.go", `func \(b \*takeBand\) syncPlayBtn\(`)
	for _, pin := range []string{
		"if len(b.queue) > 0 {", // the queue is the walk; the player is not
		`b.playBtn.SetIconName("media-playback-stop-symbolic")`,
		`b.playBtn.SetIconName("media-playback-start-symbolic")`,
		"b.playBtn.SetTooltipText(takePlayTip)", // and the ▶ says what it did before
	} {
		if !strings.Contains(face, pin) {
			t.Errorf("the button no longer has %q:\n%s", pin, face)
		}
	}
	if strings.Contains(face, "vp.playing()") || strings.Contains(face, "Playing()") {
		t.Errorf("the face is drawn from the player, which pauses between takes:\n%s", face)
	}
	// stopping lands back in chainOn through OnState: the queue has to be gone
	// before the player is told, or that re-entry reports the walk as finished
	stop := funcBody(t, "narrate_takeband.go", `func \(b \*takeBand\) stopWalk\(`)
	if i, j := strings.Index(stop, "b.queue, b.qAt = nil, 0"), strings.Index(stop, "player.Stop()"); i < 0 || j < 0 || i > j {
		t.Errorf("stopWalk stops the player before emptying the queue:\n%s", stop)
	}
	if !strings.Contains(stop, "b.syncPlayBtn()") {
		t.Errorf("the ▶ waits for the player to say the takes stopped:\n%s", stop)
	}
	// every way a walk can end has to redraw the button. The ones nobody
	// clicked for arrive through the player, which reports to syncPlayIcons.
	icons := funcBody(t, "pipeline.go", `func \(a \*App\) syncPlayIcons\(`)
	if !strings.Contains(icons, "vp.band.syncPlayBtn()") {
		t.Errorf("a walk that simply ran out leaves a ⏹ nothing is playing behind:\n%s", icons)
	}
	// ...and a new recording, which empties the queue without the player
	// noticing anything at all
	sync := funcBody(t, "narrate_takeband.go", `func \(b \*takeBand\) sync\(`)
	if !strings.Contains(sync, "b.syncPlayBtn()") {
		t.Errorf("switching voice mid-walk leaves the ⏹ up:\n%s", sync)
	}
}

// A second in one recording means nothing in the next, so pointing the band at
// a new voice puts away everything that was measured in seconds -- the picked
// stretch, the red bar, the zoom and the scroll, the queue ▶ was walking --
// rather than leaving them aimed at a place in a file nobody is looking at.
func TestANewRecordingLeavesNothingOfTheOldOneBehind(t *testing.T) {
	b := &takeBand{
		src: "/old/rec.wav", base: "rec",
		wf:   &waveform{hz: waveHz, chans: [][]uint8{{1, 2, 3}}},
		dur:  120,
		sel0: 10, sel1: 20,
		viewX: 400, pps: 8,
		at:    55,
		hover: true, hoverX: 300,
		queue: []voiceTake{{10, 20}, {30, 40}}, qAt: 1,
	}
	b.forget("/new/other.wav")
	if b.src != "/new/other.wav" || b.base != "other" {
		t.Errorf("the band is drawing %q (%q)", b.src, b.base)
	}
	if b.at >= 0 {
		t.Errorf("▶ still starts at second %v, which is in the other recording", b.at)
	}
	if b.queue != nil || b.qAt != 0 {
		t.Errorf("still walking %v at %d", b.queue, b.qAt)
	}
	if b.wf != nil || b.dur != 0 {
		t.Errorf("still holding the old envelope (%v) and length %v", b.wf, b.dur)
	}
	if _, _, ok := b.sel(); ok {
		t.Error("the old selection is still picked out on the new wave")
	}
	if b.viewX != 0 || b.hover {
		t.Errorf("scrolled to %v, hover %v", b.viewX, b.hover)
	}
	// and sync is the one way in, so it has to be the one that asks
	body := funcBody(t, "narrate_takeband.go", `func \(b \*takeBand\) sync\(`)
	if !strings.Contains(body, "b.forget(src)") {
		t.Errorf("sync changes recording without forgetting the old one:\n%s", body)
	}
}

// The band draws one recording on its OWN clock, whole. So the zoom cannot go
// below the one that fits the file in the width -- there is nothing either
// side of it to scroll into -- and the wheel leaves the second under the
// cursor where it was, which is the only zoom that does not lose your place.
func TestTheBandNeverScrollsPastTheRecording(t *testing.T) {
	b := &takeBand{src: "/x.wav", dur: 100, w: 500, pps: 5}
	b.clampView()
	if b.pps != 5 || b.viewX != 0 {
		t.Fatalf("a file that exactly fits was moved to pps %v, x %v", b.pps, b.viewX)
	}
	b.viewX = 900
	b.clampView()
	if b.viewX != 0 {
		t.Errorf("scrolled to %v with nothing off screen", b.viewX)
	}
	// zoom in around second 50 (the middle of the band) and it stays there
	b.zoomAt(250, 4)
	if b.pps != 20 {
		t.Fatalf("zooming by 4 gave pps %v", b.pps)
	}
	if at := b.tAt(250); at < 49.9 || at > 50.1 {
		t.Errorf("the second under the cursor became %v", at)
	}
	// ...and back out never shows less than the whole file
	b.zoomAt(250, 0.01)
	if b.pps != 5 || b.viewX != 0 {
		t.Errorf("zoomed out to pps %v at x %v, past the whole file", b.pps, b.viewX)
	}
}

// A drag reads the same either way round and never runs off the ends of the
// file: dragging past the left edge means "from the start", not a negative
// second the reference would then be cut at.
func TestADragIsClippedToTheRecordingEitherWayRound(t *testing.T) {
	b := &takeBand{src: "/x.wav", dur: 100, w: 500, pps: 5}
	b.dragTo(400, 100) // right to left
	if s, e, ok := b.sel(); !ok || s != 20 || e != 80 {
		t.Errorf("a backwards drag read as %v-%v (%v)", s, e, ok)
	}
	b.dragTo(-200, 9000)
	if s, e, ok := b.sel(); !ok || s != 0 || e != 100 {
		t.Errorf("a drag off both ends read as %v-%v (%v)", s, e, ok)
	}
	b.sel0, b.sel1 = 30, 30
	if _, _, ok := b.sel(); ok {
		t.Error("a nought-length selection counts as one")
	}
}

// Where the controls are: ＋ － ▶ on the dropdown's own row rather than a row
// of their own, and the band under that row -- which voice, then the recording
// it is cut from, then the sentence it will speak.
func TestTheTakeControlsSitOnTheVoiceRowAndTheBandUnderIt(t *testing.T) {
	src := funcBody(t, "narrate_voice.go", `func \(a \*App\) buildVoicePicker\(`)
	for _, pin := range []string{
		"bandFrame, bandBtns := vp.buildTakeBand()",
		"who.Append(bandBtns)", // the buttons are on the dropdown's row...
		"box.Append(who)",
		"box.Append(bandFrame)",
		"box.Append(tune)",
	} {
		if !strings.Contains(src, pin) {
			t.Errorf("the voice picker no longer has %q", pin)
		}
	}
	if strings.Index(src, "box.Append(bandFrame)") < strings.Index(src, "box.Append(who)") {
		t.Error("the band is drawn above the dropdown it belongs to")
	}
	if strings.Index(src, "box.Append(bandFrame)") > strings.Index(src, "box.Append(tune)") {
		t.Error("the band is below the sample rather than between it and the voice")
	}
	// and the handle above it all: the picture and the voice picker share the
	// column, and which of them gets the height is the hand's to say
	nar := readSrc(t, "narrate.go")
	for _, want := range []string{
		"shown := gtk.NewPaned(gtk.OrientationVertical)",
		"shown.SetStartChild(top)",
		"shown.SetEndChild(voice)",
	} {
		if !strings.Contains(nar, want) {
			t.Errorf("the left column has no handle between the picture and the voice: %q", want)
		}
	}
}

// Zoomed in, the wave on screen is a window onto the recording -- and a window
// whose only handle is the wheel is one you can lose your place in. So: a
// scrollbar under the band, gone again at the zoom that fits the whole file,
// and a bar under the pointer that says where a click would land before it
// lands there.
func TestZoomingInGivesTheBandAWayBack(t *testing.T) {
	build := funcBody(t, "narrate_takeband.go", `func \(vp \*voicePicker\) buildTakeBand\(`)
	for _, pin := range []string{
		"gtk.NewScrollbar(gtk.OrientationHorizontal, b.adj)",
		"inner.Append(b.bar)",                               // under the wave, inside the band's frame
		"b.area.ConnectResize(",                             // the width the bar is a fraction OF
		"motion.ConnectLeave(",                              // ...and the hover bar goes when the pointer does
		"b.at = math.Max(0, math.Min(b.dur, b.tAt(fromX)))", // a click sets it
	} {
		if !strings.Contains(build, pin) {
			t.Errorf("the band no longer has %q", pin)
		}
	}
	sync := funcBody(t, "narrate_takeband.go", `func \(b \*takeBand\) syncScroll\(`)
	if !strings.Contains(sync, "b.bar.SetVisible(b.dur*b.pps > b.w+0.5)") {
		t.Errorf("the scrollbar no longer goes away when the whole file is shown:\n%s", sync)
	}
	// every way the view can move has to tell the bar, or it points somewhere
	// the wave no longer is
	for _, fn := range []string{
		`func \(b \*takeBand\) zoomAt\(`,
		`func \(b \*takeBand\) load\(`,
	} {
		if body := funcBody(t, "narrate_takeband.go", fn); !strings.Contains(body, "b.syncScroll()") {
			t.Errorf("%s moves the view without telling the scrollbar:\n%s", fn, body)
		}
	}
	// the drawing says it: the set bar solid, the hover bar behind it
	draw := funcBody(t, "narrate_takeband.go", `func \(b \*takeBand\) draw\(`)
	for _, pin := range []string{"if b.at >= 0 {", "if b.hover {"} {
		if !strings.Contains(draw, pin) {
			t.Errorf("the band no longer draws %q", pin)
		}
	}
}

// takeFixture is a recording to pick takes out of: secs seconds of tone as
// "tom.wav", and the folder it sits in for the project to be built beside it.
func takeFixture(t *testing.T, secs int) (dir, src string) {
	t.Helper()
	if _, err := exec.LookPath("ffmpeg"); err != nil {
		t.Skip("no ffmpeg")
	}
	dir = t.TempDir()
	src = filepath.Join(dir, "tom.wav")
	if out, err := exec.Command("ffmpeg", "-v", "error", "-y", "-f", "lavfi",
		"-i", fmt.Sprintf("aevalsrc='0.5*sin(2*PI*300*t)':d=%d:s=44100", secs),
		"-ac", "1", src).CombinedOutput(); err != nil {
		t.Fatalf("ffmpeg: %v\n%s", err, out)
	}
	return dir, src
}
