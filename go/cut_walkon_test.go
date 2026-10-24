package main

// ▶ plays the SESSION, and a session is more than one file.
//
// A row is several recordings in a line -- a camera stopped and started again,
// a recorder splitting by size, an afternoon in three takes -- and the timeline
// is one clock over all of them. Playback used to stop dead at the end of
// whichever file it was in: the stream ran out, the picture froze on its last
// frame, and the line sat in the hatched strip between two recordings with the
// rest of the session still to the right of it. Pressing ▶ again replayed the
// file that had just ended.

import (
	"strings"
	"testing"
)

func walkEd(t *testing.T, vids ...tlVideo) *cutEditor {
	t.Helper()
	ed := newTestEd(t)
	ed.vids = vids
	ed.relayout()
	return ed
}

// The next recording is where the line goes, whether there is a hole between
// the two or nothing at all.
func TestPlaybackWalksOnToTheNextRecording(t *testing.T) {
	// a gap nobody filmed: the capture card was off for a quarter of an hour
	ed := walkEd(t,
		tlVideo{base: "a", path: "/f/a.mkv", start: 0, dur: 30},
		tlVideo{base: "b", path: "/f/b.mkv", start: 45, dur: 30})
	ed.playVideo, ed.playhead = &ed.vids[0], 30
	got, v, ok := ed.nextPlay()
	if !ok || got != 45 || v != &ed.vids[1] {
		t.Errorf("at the end of a the line goes to %g (%v, %v), want the start of b at 45", got, v, ok)
	}

	// two files written back to back meet at ONE second, and that second reads
	// as both the end of the first and the start of the second. The one that
	// just played out is not the one to play next -- read the clock alone and
	// playback stops forever at the seam, which is what an OBS recording split
	// by size is made of.
	ed = walkEd(t,
		tlVideo{base: "a", path: "/f/a.mkv", start: 0, dur: 30},
		tlVideo{base: "b", path: "/f/b.mkv", start: 30, dur: 30})
	ed.playVideo, ed.playhead = &ed.vids[0], 29.98 // a frame short, as EOS lands
	got, v, ok = ed.nextPlay()
	if !ok || got != 30 || v != &ed.vids[1] {
		t.Errorf("at a seam the line goes to %g (%v, %v), want b's first frame at 30", got, v, ok)
	}

	// a second camera rolling through the end of the first: nothing is skipped
	// at all, the same second carries on from the other recording
	ed = walkEd(t,
		tlVideo{base: "a", path: "/f/a.mkv", start: 0, dur: 30, lane: 0},
		tlVideo{base: "b", path: "/f/b.mkv", start: 10, dur: 50, lane: 1})
	ed.playVideo, ed.playhead = &ed.vids[0], 30
	got, v, ok = ed.nextPlay()
	if !ok || got != 30 || v != &ed.vids[1] {
		t.Errorf("with another camera still rolling the line goes to %g (%v, %v), want 30 on b", got, v, ok)
	}

	// and the end of the last recording IS the end: nothing to walk on to, so
	// the picture stands on its final frame like any player
	ed = walkEd(t, tlVideo{base: "a", path: "/f/a.mkv", start: 0, dur: 30})
	ed.playVideo, ed.playhead = &ed.vids[0], 30
	if got, v, ok := ed.nextPlay(); ok {
		t.Errorf("past the last recording the line goes to %g (%v), want nowhere", got, v)
	}
}

// The tick asks before it gives up. followPlayback returns early on a player
// that is not running, which is exactly the state a stream that has ended is
// in -- so the walk has to be asked ahead of that, and only for an END: a
// pause is a decision and stays one.
func TestTheWalkIsAskedBeforeTheTickGivesUp(t *testing.T) {
	body := funcBody(t, "cut.go", `func \(ed \*cutEditor\) followPlayback\(\) bool \{`)
	walk, stop := strings.Index(body, "ed.walkOn()"), strings.Index(body, "!ed.player.playing")
	if walk < 0 || stop < 0 || walk > stop {
		t.Errorf("the tick gives up on a stopped player before it asks whether the file merely ended:\n%s", body)
	}
	on := funcBody(t, "cut.go", `func \(ed \*cutEditor\) walkOn\(\) bool \{`)
	if !strings.Contains(on, "!ed.player.ended") {
		t.Errorf("the walk does not ask whether the stream ENDED -- a pause would walk on too:\n%s", on)
	}
	// it plays: setPlayhead cues the player in the state it finds it, and it
	// finds it stopped
	if !strings.Contains(on, "ed.player.Toggle()") {
		t.Errorf("the walk cues the next recording and leaves it paused:\n%s", on)
	}
	// ...and in the cut preview it carries on at the next KEPT clip, rather
	// than playing the dropped frames the next file happens to open on
	if !strings.Contains(on, "ed.cutOnlySnap()") {
		t.Errorf("▶✂ walks on into footage the cut throws away:\n%s", on)
	}
}

// A player with no stream behind it never walks: nextPlay is not even asked.
func TestNothingWalksWithoutAPlayer(t *testing.T) {
	ed := walkEd(t, tlVideo{base: "a", path: "/f/a.mkv", start: 0, dur: 30})
	ed.playhead = 30
	if ed.walkOn() {
		t.Error("a page with no player walked the line somewhere")
	}
}
