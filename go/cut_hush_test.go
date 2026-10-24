package main

// A lane the scene does not hear must be silent in the PREVIEW too.
//
// The badge already reached the render: cut.json carried the scene's Quiet list
// and clipMixes left those lanes out of the filtergraph. The preview built its
// sidecar pipelines from the file alone, so the lane went grey, the wash went
// grey, and pressing play played it anyway -- the one place the choice could be
// checked by ear was the one place it did not apply.
//
// The pipelines themselves are GStreamer and stay out of a unit test, so what
// is tested here is the answer -- which lanes are silenced, for which scene --
// and the wiring that carries it is pinned in the source.

import (
	"strings"
	"testing"
)

func TestASceneSaysWhichLanesThePreviewDoesNotHear(t *testing.T) {
	held := &cutSeg{S: 10, E: 20, Quiet: []string{"mic", "cam"}}
	for _, tc := range []struct {
		name  string
		seg   *cutSeg
		base  string // the recording the preview is playing the picture from
		own   bool
		quiet []string
	}{
		// between kept scenes, and off the ends of the cut: nothing there holds
		// an opinion, and a scrub through a cut-out stretch is worth hearing
		{"no scene under the line", nil, "cam", false, nil},
		// the ordinary shape: the picture's own sound stays, one lane goes
		{"a scene that drops one lane", &cutSeg{Quiet: []string{"mic"}}, "cam",
			false, []string{"mic"}},
		// shown from the camera, heard from the microphone -- a legal scene,
		// and the one the report was about
		{"a scene that drops the lane it is shown from", held, "cam",
			true, []string{"mic", "cam"}},
		// nothing loaded yet: there is no own sound to have an answer about
		{"no recording on the preview", held, "", false, []string{"mic", "cam"}},
		{"a scene that hears everything", &cutSeg{}, "cam", false, nil},
	} {
		own, quiet := hushOf(tc.seg, tc.base)
		if own != tc.own {
			t.Errorf("%s: the footage's own sound is hushed=%v, want %v", tc.name, own, tc.own)
		}
		if strings.Join(quiet, ",") != strings.Join(tc.quiet, ",") {
			t.Errorf("%s: the silenced lanes are %v, want %v", tc.name, quiet, tc.quiet)
		}
	}
}

// hushed is a player told what the scene at the line hears, which is Hush
// without the writing of it -- the pipelines are GStreamer, so what runs here
// is the answer and the source pins below carry it onto them.
func hushed(s *cutSeg, base string) *Player {
	own, quiet := hushOf(s, base)
	return &Player{hushOwn: own, hush: hushSet(quiet)}
}

func TestASilencedLaneIsSilentAndTheRestAreNot(t *testing.T) {
	p := hushed(&cutSeg{Quiet: []string{"mic"}}, "cam")
	if p.hushes("mic", false) != true {
		t.Error("the lane the scene silences is still audible in the preview")
	}
	if p.hushes("desk", false) != false {
		t.Error("silencing one lane silenced another the scene does hear")
	}
	if p.hushes("", true) != false {
		t.Error("silencing a recording under the footage silenced the footage")
	}

	// the scene shown from the camera and heard from the microphone
	p = hushed(&cutSeg{Quiet: []string{"cam"}}, "cam")
	if p.hushes("", true) != true {
		t.Error("a scene that silences the lane it is shown from still plays it")
	}
	if p.hushes("mic", false) != false {
		t.Error("silencing the picture's own sound silenced the microphone with it")
	}

	// and a scene that hears everything hands the lanes back, rather than
	// leaving the last scene's answer standing over the rest of the cut
	p = hushed(&cutSeg{}, "cam")
	if p.hush != nil {
		t.Errorf("a scene that hears everything left %v behind", p.hush)
	}
	if p.hushes("mic", false) || p.hushes("", true) {
		t.Error("a scene that hears everything is not hearing all of it")
	}
}

// SetMuted is a different question from Hush -- "none of this is the session"
// against "this much of the session" -- and they are both true at once while a
// card is on the preview over a scene that drops a lane. Neither may overwrite
// the other's answer, because either of them ending has to hand back only what
// it took.
func TestACardSilencesEverythingWithoutForgettingTheScene(t *testing.T) {
	p := hushed(&cutSeg{Quiet: []string{"mic"}}, "cam")
	p.muted = true
	for _, c := range []struct {
		what string
		got  bool
	}{
		{"the footage", p.hushes("", true)},
		{"the silenced lane", p.hushes("mic", false)},
		{"a lane the scene hears", p.hushes("desk", false)},
	} {
		if !c.got {
			t.Errorf("under a card %s is still heard; the render puts no session "+
				"sound under an insert", c.what)
		}
	}
	// the card comes off and the scene's own answer is still the answer
	p.muted = false
	if !p.hushes("mic", false) {
		t.Error("taking the card off gave back a lane the scene never asked for")
	}
	if p.hushes("desk", false) || p.hushes("", true) {
		t.Error("taking the card off left the session silent")
	}
}

// The player is told which lanes it is playing by name, because the scene names
// the lanes it does not hear by name. A mix that arrived unnamed could only be
// silenced all at once.
func TestEveryLaneUnderTheFootageArrivesWithItsName(t *testing.T) {
	ed := &cutEditor{
		auds: []tlAudio{
			{base: "cam", path: "/f/cam.mkv", start: 0, dur: 60, master: true},
			{base: "mic", path: "/f/mic.flac", start: 0, dur: 60},
			{base: "desk", path: "/f/desk.flac", start: 30, dur: 60},
			{base: "later", path: "/f/later.flac", start: 500, dur: 60},
		},
	}
	v := &tlVideo{base: "cam", path: "/f/cam.mkv", start: 0, dur: 60}
	var got []string
	for _, m := range ed.mixUnder(v) {
		if m.base == "" {
			t.Errorf("the lane at %s came to the preview unnamed", m.path)
		}
		got = append(got, m.base)
	}
	if want := "mic,desk"; strings.Join(got, ",") != want {
		t.Fatalf("the preview mixes %v under the footage, want %s", got, want)
	}
}

// Which scene the line is standing in, which is the whole of what changes ten
// times a second while the preview runs.
func TestTheLineFollowsTheAnswerFromSceneToScene(t *testing.T) {
	// no recording on the preview, so the footage's own answer stays false and
	// the mix is empty: nothing here reaches a GStreamer pipeline
	ed := &cutEditor{
		player: &Player{},
		segs: []cutSeg{
			{S: 0, E: 10, Quiet: []string{"mic"}},
			{S: 20, E: 30},
		},
	}
	ed.playhead = 5
	ed.syncHush()
	if !ed.player.hushes("mic", false) {
		t.Fatal("standing in a scene that silences the mic, the preview still plays it")
	}
	// the gap between two kept scenes is not a scene and has no answer: a
	// scrub through a cut-out stretch is worth hearing
	ed.playhead = 15
	ed.syncHush()
	if ed.player.hushes("mic", false) {
		t.Error("a cut-out stretch is silent because the scene before it was")
	}
	ed.playhead = 25
	ed.syncHush()
	if ed.player.hushes("mic", false) {
		t.Error("the next scene is carrying the last one's silenced lanes")
	}
	// and the far end of a scene belongs to the next one, not to it
	ed.segs[1].Quiet = []string{"mic"}
	ed.playhead = 29.9
	ed.syncHush()
	if !ed.player.hushes("mic", false) {
		t.Error("the last moment of a scene is not in it")
	}
	ed.playhead = 30
	ed.syncHush()
	if ed.player.hushes("mic", false) {
		t.Error("the scene's answer runs past its own end")
	}

	// and the recording the picture comes from is a lane like the rest: the
	// scene shown from the camera and heard from the microphone silences it.
	// Born already silent (ownMute), which is a state the preview reaches on
	// its own and is the one where nothing is written to a pipeline.
	ed.player = &Player{ownMute: true}
	ed.playVideo = &tlVideo{base: "cam"}
	ed.segs[0].Quiet = []string{"cam"}
	ed.playhead = 5
	ed.syncHush()
	if !ed.player.hushes("", true) {
		t.Error("a scene silencing the lane it is shown from still plays that lane")
	}
	if ed.player.hushes("mic", false) {
		t.Error("silencing the picture's own sound silenced the rest of the session")
	}
}

func TestTheLineTellsThePreviewWhatTheSceneHears(t *testing.T) {
	// showInsert is every path that moves the line -- a click, a frame step,
	// playback following its own clock, and persist, which is every edit --
	// so the badge is audible on the press and playback picks up the new
	// scene's lanes at the boundary instead of carrying the old ones on
	if !strings.Contains(funcBody(t, "cut_insview.go", `func \(ed \*cutEditor\) showInsert\(\) \{`),
		"ed.syncHush()") {
		t.Error("showInsert no longer says what the scene under the line hears")
	}
	if !strings.Contains(funcBody(t, "cut_hear.go", `func \(ed \*cutEditor\) syncHush\(\) \{`),
		"ed.player.Hush(own, quiet, until)") {
		t.Error("syncHush no longer hands the scene's answer to the preview")
	}
	// what the test's own hushed() stands in for: a player told the answer
	// holds exactly these two fields, and both are written every time
	if !strings.Contains(funcBody(t, "player.go", `func \(p \*Player\) Hush\(own bool, quiet \[\]string, until float64\) \{`),
		"p.hushOwn, p.hush, p.until = own, hushSet(quiet), until") {
		t.Error("Hush no longer stores all three parts of the scene's answer")
	}
	// SetMuted must not go near them: it answers a different question, and a
	// card that came and went would hand back lanes the scene never asked for
	if b := funcBody(t, "player.go", `func \(p \*Player\) SetMuted\(v bool\) \{`); strings.Contains(b, "hush") {
		t.Error("SetMuted writes the scene's answer instead of its own")
	}

	src := readSrc(t, "player.go")
	for _, pin := range []string{
		// the answer is written onto the pipelines from all three places it can
		// change: the mix being rebuilt, the card going on or off, and the line
		// crossing into another scene. Each pipeline is asked about ITSELF --
		// the footage as its own lane, every recording by name -- because one
		// answer for all of them is the bug this fixed.
		"if m := p.hushes(\"\", true); m != p.ownMute {",
		"setGain(p.gain, p.pb, p.vol(), m)",
		"m := p.hushes(a.base, false)",
		"setGain(a.gain, a.pb, p.vol(), m)",
		"newAux(fmt.Sprintf(\"mix%d\", i), t, p.vol(), p.laneErr(t.base))",
	} {
		if !strings.Contains(src, pin) {
			t.Errorf("player.go lost its pin %q", pin)
		}
	}
	if n := strings.Count(src, "p.applyMute()"); n != 3 {
		t.Errorf("applyMute is called %d times, want 3: SetMix, so a lane the scene "+
			"already silences is born silent; SetMuted; and Hush", n)
	}
	// mixUnder runs only when the FILE changes, so the scene's answer cannot
	// live there: it would arrive a clip late and be a gap in the sound at
	// every boundary. Only the mute property moves.
	if strings.Contains(funcBody(t, "cut.go", `func \(ed \*cutEditor\) mixUnder\(v \*tlVideo\) \[\]mixTrack \{`),
		"Quiet") {
		t.Error("mixUnder builds the mix from the scene, which rebuilds the pipelines " +
			"at every scene boundary")
	}
}
