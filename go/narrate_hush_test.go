package main

import (
	"strings"
	"testing"
)

// Two scenes that touch, on different cameras, is what stealing a scene for
// the other lane makes -- and there is no gap between them for the preview to
// jump. Only a gap ever re-cued, so the picture ran on into the second scene
// still playing the FIRST camera's file, and with the file came its sound: at
// the wrong scene's levels, past a lane the second scene silences. The page
// that exists to fit a line between the game's own noises was judging it
// against noises the finished video does not have.
func TestNarrateFollowsTheCutWhenTheCameraChangesWithNoGapToJump(t *testing.T) {
	vids := []tlVideo{
		{base: "cam0", path: "/rec/cam0.mkv", start: 0, dur: 600, lane: 0},
		{base: "cam1", path: "/rec/cam1.mkv", start: 0, dur: 600, lane: 1},
	}
	// 10-20 on camera 0, and 20-30 on camera 1 with no second between them
	segs := []cutSeg{{S: 10, E: 20, Cam: 0}, {S: 20, E: 30, Cam: 1}}

	if needsReload(segs, vids, "/rec/cam0.mkv", 15) {
		t.Error("inside its own scene the preview is already on the right file")
	}
	if !needsReload(segs, vids, "/rec/cam0.mkv", 25) {
		t.Error("the cut hands the picture to camera 1 at 20s and the preview must follow it")
	}
	if needsReload(segs, vids, "/rec/cam1.mkv", 25) {
		t.Error("once it has followed, it stays")
	}
	// off the ends of the cut there is no finished video, so nothing to load:
	// the tick holds the file it has while a line finishes over a clip's end
	if needsReload(segs, vids, "/rec/cam0.mkv", 45) {
		t.Error("past the last clip the cut names no file, so nothing is reloaded")
	}
	// and the tick has to act on it
	body := funcBody(t, "narrate.go", `func \(n \*narrator\) followPlayback\(`)
	if !strings.Contains(body, "needsReload(") {
		t.Error("followPlayback never asks whether the cut has changed camera under it")
	}
}

// A card laid over the footage takes those seconds' sound with the picture --
// that is what the render does with them. The preview cannot load a card, so
// it leaves the footage running underneath, and the footage went on being
// HEARD: audio the cut has replaced, under a line written to fit around it.
func TestACardOverTheFootageSilencesTheFootageInNarrateToo(t *testing.T) {
	over := cutSeg{S: 10, E: 15, Ins: "sting.mp4"}
	overKeep := cutSeg{S: 10, E: 15, Ins: "logo.png", Mute: true} // picture only
	spliced := cutSeg{S: 10, E: 10, Ins: "card.mp4", Dur: 3}
	segs := []cutSeg{{S: 0, E: 10}, over, overKeep, spliced}

	if got := overInsert(segs, 5); got != nil {
		t.Error("plain footage is not a card")
	}
	if got := overInsert(segs, 12); got == nil || got.Ins != "sting.mp4" {
		t.Fatalf("the card covering 0:12 is the sting: %v", got)
	}
	if !cardHush(overInsert(segs, 12)) {
		t.Error("a card with its own sound takes those seconds -- the footage under it is silent")
	}
	// a spliced card owns no session time at all, so the playhead is never
	// inside one and the footage it is cut into keeps its sound
	if got := overInsert([]cutSeg{spliced}, 10); got != nil {
		t.Errorf("a spliced card owns no session time, but 0:10 landed in one: %v", got)
	}
	// and the one card that does NOT silence it: picture alone, the render
	// leaves the recording underneath playing
	if cardHush(&overKeep) {
		t.Error("an insert put there for the picture alone keeps the sound under it")
	}
	// the page has to ask, and ask it of the whole silence
	body := funcBody(t, "narrate.go", `func \(n \*narrator\) syncFxSound\(`)
	if !strings.Contains(body, "cardHush(overInsert(") {
		t.Error("syncFxSound never asks whether a card has taken these seconds' sound")
	}
	if !strings.Contains(body, "fxHush(") {
		t.Error("syncFxSound stopped honouring a stop's silence")
	}
}
