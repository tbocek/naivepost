package main

// What survives an insert.
//
// A session has more than one thing to hear at any second: the sound that came
// glued to the picture, and every separately-recorded file that was rolling at
// the same time -- a mic on the table, the room. The cut page draws them as
// lanes and a selection drawn in one is about that recording, but for a long
// time the
// render had no such idea. It had "the bed" and "not the bed", and any insert
// at all emptied it: lay a two-second card over the footage and the voice that
// was talking underneath went with it.
//
// The rule these tests hold is one sentence:
//
//	an insert replaces what it BRINGS, and nothing else
//
// A picture-only insert brings a picture, so the picture goes and every lane
// plays on. A sound laid in one recording's lane brings a sound for that
// recording, so that one goes and the others -- and the capture's own track --
// play on. A clip that brings both replaces both. And
// anything SPLICED in replaces nothing at all, because it is time added to the
// session rather than a stretch of it, and nothing was ever recorded under it.

import (
	"fmt"
	"math"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// laneEd is a session with two things to hear at every second: the footage's
// own track and a mic that was rolling the whole time.
func laneEd() ([]tlVideo, []tlAudio) {
	return []tlVideo{{base: "game.mp4", path: "/x/game.mp4", start: 0, dur: 100}},
		[]tlAudio{
			{base: "mic", path: "/x/mic.wav", start: 0, dur: 100},
			{base: "room", path: "/x/room.wav", start: 0, dur: 100},
		}
}

// The rule as arithmetic, before any of it reaches ffmpeg. Every row is a thing
// somebody can do to ten seconds of a session with a mic running under them.
func TestAnInsertReplacesWhatItBringsAndNothingElse(t *testing.T) {
	vids, recs := laneEd()
	over := func(mute bool) cutSeg {
		return cutSeg{S: 10, E: 12, Ins: "sting.mp4", Mute: mute}
	}
	spliced := func(mute bool) cutSeg {
		return cutSeg{S: 10, E: 10, Dur: 2, Ins: "sting.mp4", Mute: mute}
	}
	for _, c := range []struct {
		what string
		clip prodClip
		want []string // the lanes left playing under it, in order
	}{
		{"plain footage",
			prodClip{video: &vids[0], local: 10, sessS: 10, length: 2, rate: 1, tempo: 1},
			[]string{"mic", "room"}},
		{"a card that keeps the sound under it",
			first(insClip(0, over(true), "/x/sting.mp4", vids)),
			[]string{"mic", "room"}},
		{"a card that brings its own sound",
			first(insClip(0, over(false), "/x/sting.mp4", vids)),
			nil},
		{"a card spliced in silent",
			first(insClip(0, spliced(true), "/x/sting.mp4", vids)),
			nil},
		{"a card spliced in with its own sound",
			first(insClip(0, spliced(false), "/x/sting.mp4", vids)),
			nil},
		{"a stretch of footage pasted in",
			withSess(copyClip(0, cutSeg{S: 40, E: 42}, 40, &vids[0]), 40),
			[]string{"mic", "room"}},
		{"a picture pasted without the sound filmed with it",
			withSess(copyClip(0, cutSeg{S: 40, E: 42, Mute: true}, 40, &vids[0]), 40),
			nil},
		{"a re-record over the mic alone (▼)",
			withSess(sndClip(0, cutSeg{S: 10, E: 12, Ins: "redo.wav", Lane: "mic"}, "/x/redo.wav", &vids[0], recs), 10),
			[]string{"redo", "room"}},
		{"a re-record over everything audible",
			withSess(sndClip(0, cutSeg{S: 10, E: 12, Ins: "redo.wav"}, "/x/redo.wav", &vids[0], recs), 10),
			nil},
		{"a re-record over the capture's own track (▼ on the top lane)",
			withSess(sndClip(0, cutSeg{S: 10, E: 12, Ins: "redo.wav", Lane: "game"}, "/x/redo.wav", &vids[0], recs), 10),
			[]string{"mic", "room"}},
		{"a re-record spliced in beside a lane's name",
			withSess(sndClip(0, cutSeg{S: 10, E: 10, Dur: 2, Ins: "redo.wav", Lane: "mic"}, "/x/redo.wav", &vids[0], recs), 10),
			nil},
	} {
		var got []string
		for _, m := range clipMixes(c.clip, recs) {
			got = append(got, m.base)
		}
		if strings.Join(got, ",") != strings.Join(c.want, ",") {
			t.Errorf("under %s the lanes are %v, want %v", c.what, got, c.want)
		}
	}
}

// first is insClip's clip without its note, so a table row stays one line.
func first(c prodClip, _ string) prodClip { return c }

// ▼ is the other half of the rule, and the one that needs a name to do it: a
// sound laid over a selection drawn in ONE recording's lane stands in for that
// recording, in the mix, where it was -- rather than in the capture's slot,
// which is what "these seconds sound like the file" has always meant.
func TestASoundOverOneLaneLeavesTheOthersPlaying(t *testing.T) {
	vids, recs := laneEd()
	c := withSess(sndClip(0, cutSeg{S: 10, E: 12, Ins: "redo.wav", Ss: 3, Lane: "mic"},
		"/x/redo.wav", &vids[0], recs), 10)

	mix := clipMixes(c, recs)
	var got []string
	for _, m := range mix {
		got = append(got, m.base)
	}
	if want := []string{"redo", "room"}; strings.Join(got, ",") != strings.Join(want, ",") {
		t.Fatalf("the mix is %v, want %v — the file in mic's place and room untouched", got, want)
	}
	// and it plays from where it was told to, for exactly the slot
	if m := mix[0]; m.path != "/x/redo.wav" || m.at != 0 || m.ss != 3 || m.dur != 2 {
		t.Errorf("the inserted sound came out as %+v, want it from 3 s of itself for 2 s at 0", m)
	}

	// a lane the session no longer has takes nothing out, and the capture's own
	// track is not in recs at all -- naming it is simply not naming one of these
	c.dropLane = "gone"
	if mix := clipMixes(c, recs); len(mix) != 3 {
		t.Errorf("naming a lane nobody has dropped one anyway: %d in the mix, want 3", len(mix))
	}

	// laneRecorded is what the planner asks to tell those two apart
	for _, c := range []struct {
		base string
		want bool
	}{{"mic", true}, {"room", true}, {"game", false}, {"", false}, {"gone", false}} {
		if got := laneRecorded(recs, c.base); got != c.want {
			t.Errorf("laneRecorded(%q) = %v, want %v", c.base, got, c.want)
		}
	}
}

// The witness: real files, real ffmpeg, and three tones that can only be told
// apart by listening. The footage hums at 200 Hz, the mic at 800, and the
// sting -- when it has one -- at 2000.
func TestTheLanesAreStillThereUnderTheFinishedClip(t *testing.T) {
	a := insertApp(t)
	dir := t.TempDir()
	tone := func(name string, hz int) string {
		p := filepath.Join(dir, name)
		mustFFmpeg(t, "-f", "lavfi", "-t", "8", "-i", "testsrc=size=640x360:rate=30",
			"-f", "lavfi", "-t", "8", "-i",
			fmt.Sprintf("sine=frequency=%d:sample_rate=48000", hz),
			"-c:v", "libx264", "-preset", "ultrafast", "-pix_fmt", "yuv420p", "-c:a", "aac", p)
		return p
	}
	footage := tone("game.mp4", 200)
	mic := filepath.Join(dir, "mic.wav")
	mustFFmpeg(t, "-f", "lavfi", "-t", "8", "-i", "sine=frequency=800:sample_rate=48000", mic)
	redo := filepath.Join(dir, "redo.wav")
	mustFFmpeg(t, "-f", "lavfi", "-t", "8", "-i", "sine=frequency=2000:sample_rate=48000", redo)
	card := filepath.Join(dir, "card.mp4") // a picture with no sound at all
	mustFFmpeg(t, "-f", "lavfi", "-t", "4", "-i", "testsrc2=size=640x360:rate=30",
		"-c:v", "libx264", "-preset", "ultrafast", "-pix_fmt", "yuv420p", card)

	vids := []tlVideo{{base: "game.mp4", path: footage, start: 0, dur: 8}}
	recs := []tlAudio{{base: "mic", path: mic, start: 0, dur: 8}}
	st := prodSettings{
		Container: "mp4", Codec: "h264", CRF: 30, Preset: "ultrafast",
		Height: 360, FPS: 24, AudioKbps: 96, GameVol: 0.22, Subs: "none",
	}
	seg := cutSeg{S: 2, E: 4, Ins: "card.mp4"}
	muted := seg
	muted.Mute = true

	for i, c := range []struct {
		what string
		clip prodClip
		// 200 Hz footage, 800 Hz mic, 2000 Hz the replacement sound
		game, mic, redo bool
	}{
		{"plain footage",
			prodClip{video: &vids[0], local: 2, sessS: 2, length: 2, rate: 1, tempo: 1},
			true, true, false},
		// the whole point: the card takes the picture and NOTHING else
		{"a silent card that keeps the sound under it",
			withSess(first(insClip(0, muted, card, vids)), 2), true, true, false},
		{"the same card bringing its own silence",
			withSess(first(insClip(0, seg, card, vids)), 2), false, false, false},
		{"a re-record over the mic's lane (▼)",
			withSess(sndClip(0, cutSeg{S: 2, E: 4, Ins: "redo.wav", Lane: "mic"}, redo, &vids[0], recs), 2),
			true, false, true},
		{"a re-record over the capture's own track (▼ on the top lane)",
			withSess(sndClip(0, cutSeg{S: 2, E: 4, Ins: "redo.wav", Lane: "game"}, redo, &vids[0], recs), 2),
			false, true, true},
		{"a sound over everything audible (▲▼ on a lane selection)",
			withSess(sndClip(0, cutSeg{S: 2, E: 4, Ins: "redo.wav"}, redo, &vids[0], recs), 2),
			false, false, true},
	} {
		clip := c.clip
		clip.idx, clip.boxW, clip.boxH = i, 640, 360
		clip.mix = clipMixes(clip, recs)
		out := filepath.Join(dir, fmt.Sprintf("c%d.mp4", i))
		if err := a.encodeClip(clip, out, "", st); err != nil {
			t.Fatalf("%s: %v", c.what, err)
		}
		if d, err := ffprobeDur(out); err != nil || math.Abs(d-2) > 0.15 {
			t.Errorf("%s runs %.2f s (%v), want 2", c.what, d, err)
		}
		const floor = -40
		for _, tn := range []struct {
			name string
			hz   int
			want bool
		}{{"the footage's 200 Hz", 200, c.game}, {"the mic's 800 Hz", 800, c.mic},
			{"the replacement's 2000 Hz", 2000, c.redo}} {
			db := bandAt(t, out, tn.hz)
			if got := db > floor; got != tn.want {
				t.Errorf("%s: %s reads %.1f dB (audible %v), want audible %v",
					c.what, tn.name, db, got, tn.want)
			}
		}
	}
}

// withSess puts a clip on the session clock, which the planner does for every
// clip in one place and a table row has to do for itself.
func withSess(c prodClip, at float64) prodClip { c.sessS = at; return c }

// A picture copied WITHOUT the sound filmed with it pastes silent -- and until
// this it did so only in the preview. The render's copy branch runs before the
// insert branch and never looked at the flag, so the two disagreed: scrubbing
// over the paste was silent and the finished video had the old sound in it.
func TestAPastedPictureIsSilentInTheRenderToo(t *testing.T) {
	a := insertApp(t)
	dir := t.TempDir()
	footage := filepath.Join(dir, "game.mp4")
	mustFFmpeg(t, "-f", "lavfi", "-t", "6", "-i", "testsrc=size=640x360:rate=30",
		"-f", "lavfi", "-t", "6", "-i", "sine=frequency=200:sample_rate=48000",
		"-c:v", "libx264", "-preset", "ultrafast", "-pix_fmt", "yuv420p", "-c:a", "aac", footage)
	v := tlVideo{base: "game.mp4", path: footage, start: 0, dur: 6}
	st := prodSettings{
		Container: "mp4", Codec: "h264", CRF: 30, Preset: "ultrafast",
		Height: 360, FPS: 24, AudioKbps: 96, GameVol: 0.22, Subs: "none",
	}
	for _, c := range []struct {
		what  string
		mute  bool
		heard bool
	}{
		{"a stretch of footage pasted in", false, true},
		{"a picture pasted without its sound", true, false},
	} {
		clip := copyClip(0, cutSeg{S: 4, E: 6, Ins: copyScheme + "1.000", Mute: c.mute}, 1, &v)
		clip.sessS = 4
		// what clipInput hands encodeClip is where it is decided
		_, sound, err := a.clipInput(clip, st)
		if err != nil {
			t.Fatal(err)
		}
		if sound != c.heard {
			t.Errorf("%s reports its input as having sound=%v, want %v", c.what, sound, c.heard)
		}
		out := filepath.Join(dir, fmt.Sprintf("p%v.mp4", c.mute))
		if err := a.encodeClip(clip, out, "", st); err != nil {
			t.Fatal(err)
		}
		if got := bandAt(t, out, 200) > -40; got != c.heard {
			t.Errorf("%s came out with the 200 Hz audible=%v, want %v", c.what, got, c.heard)
		}
	}
}

// The dialog asks about the sound in exactly one case, and every other row here
// is a reason not to: an answer that is already given, or one that would change
// nothing whatever it was.
func TestTheDialogAsksOnlyWhenTheScopeCouldNotAnswer(t *testing.T) {
	if _, err := os.Stat("/dev/null"); err != nil {
		t.Skip("no filesystem to write fixtures on")
	}
	dir := t.TempDir()
	silent := filepath.Join(dir, "card.mp4")
	mustFFmpeg(t, "-f", "lavfi", "-t", "2", "-i", "testsrc=size=160x120:rate=15",
		"-c:v", "libx264", "-preset", "ultrafast", "-pix_fmt", "yuv420p", silent)
	loud := filepath.Join(dir, "sting.mp4")
	mustFFmpeg(t, "-f", "lavfi", "-t", "2", "-i", "testsrc=size=160x120:rate=15",
		"-f", "lavfi", "-t", "2", "-i", "sine=frequency=440:sample_rate=48000",
		"-c:v", "libx264", "-preset", "ultrafast", "-pix_fmt", "yuv420p", "-c:a", "aac", loud)

	ed := &cutEditor{auds: []tlAudio{{base: "mic", path: "/x/mic.wav", start: 0, dur: 60}}}
	for _, c := range []struct {
		why  string
		path string
		at   float64
		m    insMode
		want bool
	}{
		{"a silent file over seconds with something to hear", silent, 10, insMode{}, true},
		{"a file with sound of its own, laid over footage: its own, or the session's",
			loud, 10, insMode{}, true},
		{"a silent card the footage is cut open for -- nothing under it, nothing of its own",
			silent, 10, insMode{splice: true}, false},
		{"a sting spliced in: silent, or its own sound", loud, 10, insMode{splice: true}, true},
		{"the lane it stands in for was named by the drag", silent, 10, insMode{lane: "mic"}, false},
		{"a sound insert settles it by being one", filepath.Join(dir, "x.wav"), 10, insMode{}, false},
		{"a copied stretch of the session is footage, and footage has sound",
			copyScheme + "12.000", 10, insMode{}, true},
		{"nothing to hear in those seconds, and nothing of its own either",
			silent, 900, insMode{}, false},
	} {
		if got := ed.soundOpen(c.path, c.at, 2, c.m); got != c.want {
			t.Errorf("%s: the dialog asks=%v, want %v", c.why, got, c.want)
		}
	}
	// and the lanes are what "something to hear" reads: the capture's own track
	// is one of them (masterLanes), so a session with sound is never blind here
	if !ed.soundAt(59.5, 2) || ed.soundAt(60, 2) {
		t.Error("soundAt does not stop at the end of the recording")
	}
}

// The answer, wherever it came from, has to reach the segment and stay there:
// the scope's when the scope could settle it, the dialog's when it could not.
func TestTheAnswerReachesTheCardAndStays(t *testing.T) {
	_, ed := selEd(t)

	// a sound laid down while ▼ named the mic remembers which one it stood in for
	if n := ed.addSound("redo.wav", 10, 2, 0, "mic"); n != 1 {
		t.Fatalf("the sound went over %d pieces of footage, want 1", n)
	}
	card := func() cutSeg {
		t.Helper()
		for _, s := range ed.segs {
			if s.audioIns() {
				return s
			}
		}
		t.Fatal("the sound is not in the cut at all")
		return cutSeg{}
	}
	if got := card().Lane; got != "mic" {
		t.Errorf("the card was laid over %q, want mic", got)
	}

	// a span that straddles a hole in the footage lands as two cards, and both
	// stand in for the same recording -- the scope was named once for the whole
	// span, and a hole is no reason to change its mind halfway
	_, ed = selEd(t)
	ed.segs = []cutSeg{{S: 0, E: 20}, {S: 40, E: 60}}
	if n := ed.addSound("redo.wav", 15, 30, 0, "mic"); n != 2 {
		t.Fatalf("a sound across a hole went over %d pieces of footage, want 2", n)
	}
	for _, s := range ed.segs {
		if s.audioIns() && s.Lane != "mic" {
			t.Errorf("the piece at %.0f s stands in for %q, want mic", s.S, s.Lane)
		}
	}

	// and one laid down at picture-and-sound names nothing, which is the render's
	// way of hearing "stand in for everything audible"
	_, ed = selEd(t)
	ed.addSound("redo.wav", 10, 2, 0, "")
	if got := card().Lane; got != "" {
		t.Errorf("a card laid over everything named %q, want nothing", got)
	}

	// the picture-alone flag survives a trip through the dialog in both
	// directions -- it used to be preserved but never settable, so ticking the
	// box in the dialog changed nothing at all
	_, ed = selEd(t)
	ed.layOver(cutSeg{S: 10, E: 12, Ins: "card.mp4", Mute: true})
	i := -1
	for k, s := range ed.segs {
		if s.isInsert() {
			i = k
		}
	}
	if i < 0 {
		t.Fatal("the card is not in the cut at all")
	}
	for _, c := range []struct {
		what string
		mute bool
	}{{"unticking it lets the seconds go quiet", false}, {"ticking it again keeps them", true}} {
		ed.applyInsert(i, "card.mp4", insMode{dur: 2, mute: c.mute, askMute: true})
		if got := ed.segs[i].Mute; got != c.mute {
			t.Errorf("%s: the card came back Mute=%v, want %v", c.what, got, c.mute)
		}
	}
}

// The seams: the scope reaching the segment, the segment reaching the clip, and
// the clip reaching ffmpeg.
func TestTheLaneKeepingIsWired(t *testing.T) {
	pins := map[string][]string{
		"cut.go": {
			"Lane string `json:\"lane,omitempty\"`",
			"return ed.layOverSound(cutSeg{S: t, E: t + dur, Ins: path, Ss: ss, Lane: lane})",
			"n := a.ed.addSound(rel, at, m.dur, 0, m.lane)",
			"m.askMute = ed.soundOpen(path, at, m.dur, m)",
			"Ins: s.Ins, Ss: s.Ss + t0 - s.S, Lane: s.Lane})",
			"out.mute = keep.Active()",
			"card.Mute = m.mute",
		},
		"cut_audio.go": {"func (ed *cutEditor) soundOpen(path string, at, dur float64, m insMode) bool {"},
		"produce.go": {
			"c = sndClip(i, s, path, v, recs)",
			"noLanes: !s.keepsSoundUnder()",
			"c = copyClip(i, s, from, v)",
			"case c.snd != \"\" && c.dropLane == \"\":",
			"if au.base == c.dropLane {",
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
