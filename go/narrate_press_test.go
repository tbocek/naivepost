package main

import (
	"strings"
	"testing"
)

// A row's button showed ⏸ while the picture was running on it -- syncSpeakIcons
// draws that from livePlayRow -- but only the branch for a clip with NO line
// ever asked whether a press meant "pause". A press on a line that was playing
// fell all the way through to the audition at the bottom of speakEntry, which
// re-cued the preview at the line's lead-in and started it again. The button
// showed ⏸, and pressing it played. It took a second press -- by which time
// solo had been set, so the solo branch matched -- to actually pause.
func TestPressingThePauseOnASpokenLinePausesItOnTheFirstPress(t *testing.T) {
	n := &narrator{liveRow: -1, speaking: -1, solo: -1, entries: []narrEntry{
		{S: 100, E: 130, At: 0, Text: "the first line"},
		{S: 200, E: 260, At: 5, Text: "the second line"},
		{S: 300, E: 330, At: 0}, // a clip the narration left alone
	}}

	// nothing is running, so no row's press is a pause -- every one of them
	// means "play this row"
	for i := range n.entries {
		if n.pausePress(i) {
			t.Errorf("with the preview stopped, row %d's press was read as a pause", i)
		}
	}

	n.player = &Player{playing: true}
	n.pos = 105 // inside the first line, which is the row wearing the ⏸
	if !n.pausePress(0) {
		t.Error("the picture is running on the first line and its ⏸ does not pause")
	}
	if n.pausePress(1) || n.pausePress(2) {
		t.Error("a row the picture is not on read its press as a pause")
	}
	// the wordless clip was already right, and stays right
	n.pos = 310
	if !n.pausePress(2) {
		t.Error("the clip with no line lost the pause it already had")
	}

	// the face and the press come from one question, so they cannot disagree
	for _, pos := range []float64{105, 180, 205, 310} {
		n.pos = pos
		for i := range n.entries {
			if want := n.livePlayRow() == i; n.pausePress(i) != want {
				t.Errorf("at %gs row %d shows ⏸=%v but its press pauses=%v",
					pos, i, want, n.pausePress(i))
			}
		}
	}

	// a line spoken over a still frame is not this: that is a voice-only
	// toggle further down speakEntry, which also knows how to play it again
	n.player = &Player{}
	n.voice = &Player{playing: true}
	n.speaking = 1
	if n.pausePress(1) {
		t.Error("a line over a still frame was taken by the picture's pause, which would never resume it")
	}
}

// ...and it is asked ahead of every branch, because every kind of row can be the
// one under the playhead: a line, a caption, a clip with nothing written on it.
func TestEveryKindOfRowAnswersItsPauseBeforeAnythingElse(t *testing.T) {
	body := funcBody(t, "narrate_tts.go", `func \(a \*App\) speakEntry\(`)
	press := strings.Index(body, "n.pausePress(i)")
	if press < 0 {
		t.Fatal("speakEntry never asks whether the press was a pause")
	}
	for _, later := range []string{
		`strings.TrimSpace(e.Text) == ""`, // the clip with no line
		"a.captionsOnly()",                // the caption, never spoken
		"n.solo == i && n.synthing",       // the line being spoken for the first time
		"n.claimVoice()",                  // and the audition itself
	} {
		if at := strings.Index(body, later); at >= 0 && at < press {
			t.Errorf("%q is decided before the pause is, so that kind of row still takes two presses", later)
		}
	}
	if strings.Contains(body, "n.nearestEntry(n.pos) == i") {
		t.Error("one branch still works out the live row for itself instead of asking pausePress")
	}
}
