package main

import (
	"strings"
	"testing"
)

// The Narrate preview is a preview OF THE FINISHED VIDEO. You stand a line
// against it and judge whether the line fits the picture -- so if the picture
// is not the one the render will make, the judgement is about nothing.
//
// It was not. The page asked ed.videoAt, which starts from the row the Cut
// page was last clicked to watch; a glance at the second camera's thumbnails
// while editing left Narrate playing that camera for the entire session,
// whatever the cut said. cutVideoAt is the question the render asks: the scene
// covering t, on the camera THAT SCENE names, and nothing in a gap.
func TestNarrateShowsTheSceneSCameraNotTheWatchedRow(t *testing.T) {
	ed := &cutEditor{
		vids: []tlVideo{
			{path: "/a.mp4", base: "a", lane: 0, start: 0, dur: 100},
			{path: "/b.mp4", base: "b", lane: 1, start: 0, dur: 100},
		},
		segs:   []cutSeg{{S: 10, E: 20, Cam: 1}, {S: 40, E: 50, Cam: 0}},
		laneN:  2,
		monRow: 1, // a click on row 0's thumbnails: "watch camera 1"
	}
	for _, c := range []struct {
		name string
		t    float64
		want string // "" means nil
	}{
		{"the scene names camera 2, so that is what plays", 15, "/b.mp4"},
		{"the next scene names camera 1", 45, "/a.mp4"},
		{"a gap between scenes is not part of the finished video", 30, ""},
		{"and neither is anything past the last scene", 80, ""},
	} {
		got := ""
		if v := ed.cutVideoAt(c.t); v != nil {
			got = v.path
		}
		if got != c.want {
			t.Errorf("%s: at %gs the preview cues %q, want %q", c.name, c.t, got, c.want)
		}
	}
	// and the editor's own question still answers differently, which is the
	// whole reason the two had to be told apart: it hands back the watched row
	// inside the first scene, and footage in the gap
	if v := ed.videoAt(15); v == nil || v.path != "/a.mp4" {
		t.Errorf("the Cut page's own lookup stopped following the watched row: %v", v)
	}
	if v := ed.videoAt(30); v == nil {
		t.Error("the Cut page's own lookup stopped showing footage in a gap")
	}
}

// The other half of "respect the cut" is what you HEAR. A narration is written
// against the sound bed it will sit over, so the preview owed all five of the
// things the render does to the audio and was doing none of them: the other
// lanes' microphones mixed in under the picture, a lane the scene silences
// staying silent, volume effects, a stop muting, and a speed ramp.
//
// Pinned as source: every one of these is a call into a live GStreamer
// pipeline, and the claim is that the call is made at all.
func TestTheNarratePreviewIsMixedLikeTheRender(t *testing.T) {
	sound := funcBody(t, "narrate.go", `func \(n \*narrator\) syncFxSound\(\)`)
	for _, want := range []string{
		"p.SetFxGain(n.gameGain(n.pos))",  // a volume effect, and the render's duck
		"p.SetMuted(fxHush(ed.fx, n.pos)", // a stop that silences
		"ed.cutVideoAt(n.pos)",            // asked about the scene's camera, not the watched row
		"s := n.heardScene(n.pos)",        // ...and about the scene whose sound is sounding
		"p.Hush(own, quiet, until)",       // a lane that scene silences
		"cardHush(overInsert(",            // and a card that has taken these seconds' sound
	} {
		if !strings.Contains(sound, want) {
			t.Errorf("the Narrate preview's sound no longer does %q", want)
		}
	}
	src := readSrc(t, "narrate.go")
	for _, want := range []string{
		"v := ed.cutVideoAt(t)",           // cue plays the finished video's camera
		"n.player.SetMix(ed.mixUnder(v))", // under the other lanes the render mixes in
		"n.syncFxSound()",                 // and settled again wherever the line moves
		"n.syncPlayRate()",                // ...including across a speed boundary while running
		"n.syncFx(t)",                     // and the picture with it
	} {
		if !strings.Contains(src, want) {
			t.Errorf("narrate.go no longer contains %q", want)
		}
	}
	// at the speed the render will use, on BOTH of cue's paths -- a rate only
	// takes hold at a seek, and cue has two of them (a seek inside the file
	// already loaded, and a fresh PlaySegment)
	if n := strings.Count(src, "SetRate(fxPreviewRateAt(ed.fx, t))"); n < 2 {
		t.Errorf("only %d of cue's two seek paths sets the speed", n)
	}
	// the old answer is gone: videoAt here is the Cut page leaking into this one
	if strings.Contains(src, "ed.videoAt(") {
		t.Error("narrate.go is back on ed.videoAt — the preview will follow the watched row again")
	}
}
