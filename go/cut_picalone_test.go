package main

// An insert that brings no sound of its own.
//
// ⧉ Insert can put picture into the cut without sound: a sting laid over
// footage replaces the frames and leaves what is heard running underneath, and
// the same sting spliced in goes silent.
//
// One flag on the segment (cutSeg.Mute) says the sentence both cases share --
// this insert brings no sound of its own -- and the MODE decides which of the
// two it comes out as, since the mode is already the thing that says whether
// there is footage underneath. It used to be set before the chooser opened, by
// a strip above the tracks that scoped the selection to "the picture alone";
// it is the insert form's own tick now, asked of the file that came back
// (soundOpen). These tests hold that split, the path from the tick to the
// flag, and the only witness that can settle what the render did: the tones in
// the finished clip.

import (
	"fmt"
	"math"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"strconv"
	"strings"
	"testing"
)

// The two readings, and that everything else on the timeline is deaf to the
// flag: footage does not carry one, and an audio insert is already the case
// where the sound comes from somewhere other than the picture.
func TestMuteReadsTwoWaysByMode(t *testing.T) {
	for _, c := range []struct {
		what          string
		seg           cutSeg
		under, silent bool
	}{
		{"footage", cutSeg{S: 0, E: 10}, false, false},
		{"a sting over the footage", cutSeg{S: 0, E: 4, Ins: "sting.mp4"}, false, false},
		{"a sting over the picture alone", cutSeg{S: 0, E: 4, Ins: "sting.mp4", Mute: true}, true, false},
		{"a card spliced in", cutSeg{S: 4, E: 4, Ins: "card.png", Dur: 3}, false, false},
		{"a card spliced in silent", cutSeg{S: 4, E: 4, Ins: "card.png", Dur: 3, Mute: true}, false, true},
	} {
		if got := c.seg.keepsSoundUnder(); got != c.under {
			t.Errorf("%s: keepsSoundUnder is %v, want %v", c.what, got, c.under)
		}
		if got := c.seg.playsSilent(); got != c.silent {
			t.Errorf("%s: playsSilent is %v, want %v", c.what, got, c.silent)
		}
	}
	// the two never both hold: an insert cannot keep a sound that is not there
	s := cutSeg{S: 0, E: 4, Ins: "sting.mp4", Mute: true}
	if s.keepsSoundUnder() && s.playsSilent() {
		t.Error("one insert both keeps the sound under it and plays silent")
	}
}

// The path the hand takes now: ⧉ Copy, put the red line somewhere else,
// ⧉ Paste. The copy carries the footage it was taken from -- picture and what
// was filmed with it, because that is what a selection drawn on the pictures
// IS -- and the pasted clip is where "play it silent" is asked, of the clip
// that exists rather than of a scope set before it did (soundOpen).
func TestACopyPastesTheFootageItWasTakenFrom(t *testing.T) {
	a, ed := selEd(t)
	ed.sel.t0, ed.sel.t1, ed.sel.active = 10, 20, true
	a.copyClicked()
	// the band is cleared and pointed somewhere else entirely before the
	// paste: the hand still holds what it took
	ed.sel.active = false
	ed.playhead, ed.hasPlay = 100, true
	a.pasteCopy()

	var pasted *cutSeg
	for i := range ed.segs {
		if ed.segs[i].isCopy() {
			pasted = &ed.segs[i]
		}
	}
	if pasted == nil {
		t.Fatalf("nothing was pasted into %+v", ed.segs)
	}
	if pasted.Mute {
		t.Errorf("the pasted copy arrived silent: %+v", *pasted)
	}
	// ...and its form asks: a copy is footage, so it has a sound to decide
	// about, and a splice has nothing underneath -- which is the silent
	// reading of the one flag
	m := insMode{splice: true, dur: pasted.length(), mute: pasted.Mute}
	if !ed.soundOpen(pasted.Ins, pasted.S, m.dur, m) {
		t.Error("the pasted copy's form does not ask whether it plays silent")
	}
	pasted.Mute = true
	if !pasted.playsSilent() {
		t.Errorf("a muted spliced copy is not the silent reading: %+v", *pasted)
	}
}

// A sound is the one thing that cannot be laid over frames while leaving the
// sound under them alone, so the chooser does not offer one to a picture-alone
// selection -- the mirror of a selection in a lane, which is offered nothing
// else.
func TestTheChooserOffersNoSoundToAPictureAloneSelection(t *testing.T) {
	aud := map[string]bool{}
	for _, e := range audExts {
		aud[e] = true
	}
	for _, e := range picExts {
		if aud[e] {
			t.Errorf("%q is offered to a picture-alone selection and is a sound", e)
		}
	}
	// and the full list is still both halves, or an ordinary insert lost a type
	have := map[string]bool{}
	for _, e := range insExts {
		have[e] = true
	}
	for _, e := range append(append([]string{}, picExts...), audExts...) {
		if !have[e] {
			t.Errorf("%q is in neither half of what ⧉ Insert admits", e)
		}
	}
}

// bandAt is how loud one tone is in a file: two band passes back to back,
// because one biquad lets enough of a tone a decade away through to blur the
// verdict.
func bandAt(t *testing.T, file string, hz int) float64 {
	t.Helper()
	filter := fmt.Sprintf("bandpass=f=%d:width_type=h:w=200,bandpass=f=%d:width_type=h:w=200,volumedetect", hz, hz)
	out, err := exec.Command("ffmpeg", "-v", "info", "-i", file,
		"-af", filter, "-f", "null", "-").CombinedOutput()
	if err != nil {
		t.Fatalf("volumedetect: %v\n%s", err, out)
	}
	m := regexp.MustCompile(`mean_volume: (-?[0-9.]+) dB`).FindStringSubmatch(string(out))
	if m == nil {
		t.Fatalf("no mean_volume in\n%s", out)
	}
	v, _ := strconv.ParseFloat(m[1], 64)
	return v
}

// The whole point, settled end to end by the only witness that can. The
// recording carries a 200 Hz tone and the sting a 2000 Hz one, and the two
// readings of Mute have to come out as two different sounds:
//
//	over the picture alone -- 200 Hz, and no 2000 Hz: the frames changed and
//	                          what was being heard carried on
//	spliced in silent      -- neither: the cut was opened, so there was nothing
//	                          underneath to keep and the file's own is dropped
//
// The unmuted sting is rendered beside them as the control. If Mute were read
// as plain "drop the audio stream" the first row would come out silent too, and
// the two rows are what tells those apart.
func TestAnInsertOverThePictureAloneKeepsWhatIsHeard(t *testing.T) {
	a := insertApp(t)
	dir := t.TempDir()
	footage := filepath.Join(dir, "footage.mp4")
	mustFFmpeg(t, "-f", "lavfi", "-t", "6", "-i", "testsrc=size=1280x720:rate=30",
		"-f", "lavfi", "-t", "6", "-i", "sine=frequency=200:sample_rate=48000",
		"-c:v", "libx264", "-preset", "ultrafast", "-pix_fmt", "yuv420p", "-c:a", "aac", footage)
	sting := filepath.Join(dir, "sting.mp4")
	mustFFmpeg(t, "-f", "lavfi", "-t", "3", "-i", "testsrc2=size=640x480:rate=30",
		"-f", "lavfi", "-t", "3", "-i", "sine=frequency=2000:sample_rate=48000",
		"-c:v", "libx264", "-preset", "ultrafast", "-pix_fmt", "yuv420p", "-c:a", "aac", sting)

	st := prodSettings{
		Container: "mp4", Codec: "h264", CRF: 30, Preset: "ultrafast",
		Height: 360, FPS: 24, AudioKbps: 96, GameVol: 0.22, Subs: "none",
	}
	for _, c := range []struct {
		name string
		clip prodClip
		// whether each tone should be audible in the finished clip
		rec, ins bool
	}{
		// what the planner builds for a muted overwrite: the picture is the
		// file's, and the recording goes in the sound slot an audio insert's
		// file would have taken (produce.go, keepsSoundUnder)
		{"over the picture alone",
			prodClip{idx: 0, ins: sting, length: 2, rate: 1, tempo: 1, boxW: 640, boxH: 360,
				mute: true, snd: footage, sndAt: 1},
			true, false},
		{"spliced in silent",
			prodClip{idx: 1, ins: sting, length: 2, rate: 1, tempo: 1, boxW: 640, boxH: 360,
				mute: true},
			false, false},
		{"over the footage, sound and all",
			prodClip{idx: 2, ins: sting, length: 2, rate: 1, tempo: 1, boxW: 640, boxH: 360},
			false, true},
	} {
		out := filepath.Join(dir, fmt.Sprintf("c%d.mp4", c.clip.idx))
		if err := a.encodeClip(c.clip, out, "", st); err != nil {
			t.Fatalf("%s: %v", c.name, err)
		}
		d, err := ffprobeDur(out)
		if err != nil {
			t.Fatalf("%s produced nothing readable: %v", c.name, err)
		}
		if math.Abs(d-2) > 0.15 {
			t.Errorf("%s runs %.2f s, want 2", c.name, d)
		}
		// -40 dB is well clear of both: a tone that is there reads around -25
		// on this material and one that is not reads below -70
		const floor = -40
		rec, ins := bandAt(t, out, 200), bandAt(t, out, 2000)
		if got := rec > floor; got != c.rec {
			t.Errorf("%s: the recording's 200 Hz reads %.1f dB (audible %v), want audible %v",
				c.name, rec, got, c.rec)
		}
		if got := ins > floor; got != c.ins {
			t.Errorf("%s: the sting's 2000 Hz reads %.1f dB (audible %v), want audible %v",
				c.name, ins, got, c.ins)
		}
	}
}

// clipInput is where the silent reading is decided: an insert that brings no
// sound must report its input as having none, or encodeClip maps the file's own
// track and the splice comes out with the sting still in it.
func TestASilentInsertReportsNoAudioToTake(t *testing.T) {
	a := insertApp(t)
	dir := t.TempDir()
	sting := filepath.Join(dir, "sting.mp4")
	mustFFmpeg(t, "-f", "lavfi", "-t", "2", "-i", "testsrc=size=320x240:rate=30",
		"-f", "lavfi", "-t", "2", "-i", "sine=frequency=2000:sample_rate=48000",
		"-c:v", "libx264", "-preset", "ultrafast", "-pix_fmt", "yuv420p", "-c:a", "aac", sting)

	st := prodSettings{FPS: 24, Height: 360}
	if _, sound, err := a.clipInput(prodClip{idx: 0, ins: sting, length: 2}, st); err != nil {
		t.Fatal(err)
	} else if !sound {
		t.Fatal("this test's premise changed — the sting has no audio stream to drop")
	}
	if _, sound, err := a.clipInput(prodClip{idx: 1, ins: sting, length: 2, mute: true}, st); err != nil {
		t.Fatal(err)
	} else if sound {
		t.Error("a muted insert still offers its own sound, and the splice would not be silent")
	}
}

// The preview has to say the same thing the render will, and here the two are
// opposites that share a flag: a silent splice hushes the session because the
// cut is open under it, and an overwrite covering the picture alone must NOT,
// because the recording underneath is exactly what is meant to still be heard.
// Neither takes the file's own sound, and a copy pasted from a picture-alone
// selection must not fall through to the copy's rule and get the recording it
// was taken without.
// Where the kept sound comes from, and the two ways asking for it can come to
// nothing. The recording is picked by the second the insert covers and the
// offset is into that recording, not into the session -- get either wrong and
// the sting plays over the wrong words, which is a mistake no test that only
// asks "is there sound" would catch.
//
// The two empty-handed answers each carry a note, because both look exactly
// like the flag was quietly ignored and neither is visible on the timeline. The
// two silent-on-purpose answers carry none.
func TestSoundUnderPicksTheRecordingItCovers(t *testing.T) {
	if _, err := exec.LookPath("ffmpeg"); err != nil {
		t.Skip("no ffmpeg on this machine")
	}
	dir := t.TempDir()
	talk := filepath.Join(dir, "talk.mp4")
	mustFFmpeg(t, "-f", "lavfi", "-i", "testsrc=size=64x64:rate=15:duration=10",
		"-f", "lavfi", "-i", "sine=frequency=200:duration=10",
		"-c:v", "libx264", "-c:a", "aac", "-shortest", talk)
	silent := filepath.Join(dir, "silent.mp4")
	mustFFmpeg(t, "-f", "lavfi", "-i", "testsrc=size=64x64:rate=15:duration=10",
		"-c:v", "libx264", silent)

	vids := []tlVideo{
		{base: "talk.mp4", path: talk, start: 0, dur: 10},
		{base: "silent.mp4", path: silent, start: 20, dur: 10},
	}
	over := func(t0 float64) cutSeg { return cutSeg{S: t0, E: t0 + 2, Ins: "sting.mp4", Mute: true} }

	for _, c := range []struct {
		what string
		s    cutSeg
		path string
		at   float64
		note string
	}{
		{"over the picture alone", over(4), talk, 4, ""},
		{"over the footage, sound and all", cutSeg{S: 4, E: 6, Ins: "sting.mp4"}, "", 0, ""},
		{"spliced in silent", cutSeg{S: 4, E: 4, Dur: 2, Ins: "sting.mp4", Mute: true}, "", 0, ""},
		{"over a gap between recordings", over(14), "", 0, "fall in no recording"},
		{"over a recording with no sound", over(24), "", 0, "has no sound"},
	} {
		path, at, note := soundUnder(c.s, vids)
		if path != c.path || math.Abs(at-c.at) > 0.001 {
			t.Errorf("%s takes its sound from %q at %.1f s, want %q at %.1f s",
				c.what, filepath.Base(path), at, filepath.Base(c.path), c.at)
		}
		if (c.note == "") != (note == "") || (c.note != "" && !strings.Contains(note, c.note)) {
			t.Errorf("%s was logged %q, want a note saying %q", c.what, note, c.note)
		}
	}

	// the offset is into the recording, not into the session: an insert on the
	// third minute of a recording that started at minute two is one minute in
	vids[0].start, vids[0].dur = 120, 300
	if _, at, _ := soundUnder(over(180), vids); at != 60 {
		t.Errorf("an insert 60 s into a recording took its sound from %.0f s in", at)
	}
	vids[0].start, vids[0].dur = 0, 10

	// and the clip the planner builds carries that answer, not just knows it:
	// the flag reaches the render twice over, once as the sound to take and
	// once as the permission to take the insert's own
	c, _ := insClip(3, over(4), "/x/sting.mp4", vids)
	if c.snd != talk || math.Abs(c.sndAt-4) > 0.001 || !c.mute {
		t.Errorf("the clip takes %q at %.1f s (muted %v), want talk.mp4 at 4.0 s muted",
			filepath.Base(c.snd), c.sndAt, c.mute)
	}
	if c.ins != "/x/sting.mp4" || c.length != 2 || c.idx != 3 {
		t.Errorf("clip %d shows %q for %.1f s, want 3, the sting, 2.0 s", c.idx, c.ins, c.length)
	}
	if c, _ := insClip(0, cutSeg{S: 4, E: 6, Ins: "sting.mp4"}, "/x/sting.mp4", vids); c.snd != "" || c.mute {
		t.Errorf("an insert with its own sound was given %q to play (muted %v)", c.snd, c.mute)
	}
}

func TestThePreviewHushesTheSameSecondsTheRenderDoes(t *testing.T) {
	for _, c := range []struct {
		what  string
		seg   cutSeg
		hush  bool   // the session's own sound muted under the card
		voice string // ...and the file the card plays instead
	}{
		{"a sting over the footage", cutSeg{S: 0, E: 4, Ins: "sting.mp4"}, true, "sting.mp4"},
		{"a sting over the picture alone",
			cutSeg{S: 0, E: 4, Ins: "sting.mp4", Mute: true}, false, ""},
		{"a card spliced in silent",
			cutSeg{S: 4, E: 4, Ins: "card.png", Dur: 3, Mute: true}, true, ""},
		{"footage pasted silent",
			cutSeg{S: 4, E: 4, Ins: copyScheme + "12.000", Dur: 3, Mute: true}, true, ""},
	} {
		_, ed := selEd(t)
		ed.hold.on = true // cardVoice answers nothing while the preview is parked
		s := c.seg
		if got := cardHush(&s); got != c.hush {
			t.Errorf("%s: the session is hushed %v, want %v", c.what, got, c.hush)
		}
		want := ""
		if c.voice != "" {
			want = ed.a.loadPath(c.voice) // the preview opens it where it lies
		}
		if got := ed.cardVoice(&s); got != want {
			t.Errorf("%s: the card plays %q, want %q", c.what, got, want)
		}
	}
}

// The seams: the one flag reaching the render, and the one place the question
// behind it is asked -- the insert's own form, of the file that came back.
// It used to be the selection's scope, a strip above the tracks with a third
// rung for "the picture alone"; the strip is gone and this is where the
// question went (soundOpen, askInsertParams).
func TestTheSilentInsertIsWired(t *testing.T) {
	pins := map[string][]string{
		"cut.go": {
			"m.mute = !insHasSound(path)",
			"m.askMute = ed.soundOpen(path, at, m.dur, m)",
			"if m.askMute && !out.asLane {",
			"out.mute = keep.Active()",
			"func (s cutSeg) keepsSoundUnder() bool { return s.isInsert() && s.Mute && !s.spliced() }",
		},
		"cut_audio.go": {
			"func (ed *cutEditor) soundOpen(path string, at, dur float64, m insMode) bool {",
			"func insHasSound(path string) bool {",
		},
		"cut_insview.go": {
			"ed.player.SetMuted(cardHush(s) || fxHush(ed.fx, ed.playhead))",
		},
		"produce.go": {
			"c, note = insClip(i, s, path+q.suffix(), vids)",
			"hasAudioStream(file) && !c.mute, nil",
		},
	}
	for file, want := range pins {
		src, err := os.ReadFile(file)
		if err != nil {
			t.Fatal(err)
		}
		for _, w := range want {
			if !strings.Contains(string(src), w) {
				t.Errorf("%s no longer contains %q", file, w)
			}
		}
	}
}
