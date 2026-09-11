package main

// The red line's time, printed. The line answers "where am I" to the nearest
// pixel, and at the zoom the page opens on a pixel is several seconds -- so the
// number you actually want after clicking (to mark it, to compare it with a
// narration time, to say where something is) was nowhere on the page. What these
// pin is that the readout says the same thing the rest of the app does, and that
// every path which moves the playhead also moves the readout: a clock that is
// right after a click and stale after ‹f is worse than no clock.

import (
	"os"
	"regexp"
	"strings"
	"testing"
)

func TestTheRedLineSaysWhatTimeItIs(t *testing.T) {
	// mm:ss.d, the spelling the edge readouts and the Narrate page already use
	for _, c := range []struct {
		t    float64
		want string
	}{
		{0, "00:00.0"},
		{5, "00:05.0"},
		{83.45, "01:23.5"},
		{3599.94, "59:59.9"},
	} {
		if got := playheadClock(c.t, true); got != c.want {
			t.Errorf("playheadClock(%g) = %q, want %q", c.t, got, c.want)
		}
	}

	// the placeholder is dashes rather than an empty label, and exactly as wide
	// as a time: an empty one is a hole in the toolbar, and a narrower one makes
	// the first click shove every button to its right sideways
	blank := playheadClock(0, false)
	if blank == "" || strings.ContainsAny(blank, "0123456789") {
		t.Errorf("with no playhead the clock reads %q -- it must not look like a time", blank)
	}
	if len(blank) != len(playheadClock(83.45, true)) {
		t.Errorf("blank clock %q is %d chars against %d for a real time -- the bar will twitch",
			blank, len(blank), len(playheadClock(83.45, true)))
	}
}

func TestTheClocksTooltipPlacesTheLineInARecording(t *testing.T) {
	ed := newTestEd(t)
	ed.vids = []tlVideo{
		{base: "a", path: "/tmp/take-a.mkv", start: 0, dur: 100, fps: 30},
		{base: "b", path: "/tmp/take-b.mkv", start: 100, dur: 100, fps: 25},
	}
	ed.segs = []cutSeg{{S: 10, E: 20}}

	if tip := ed.playheadTip(); !strings.Contains(tip, "left-click") {
		t.Errorf("with no playhead the hover says %q -- it should say how to get one", tip)
	}

	ed.hasPlay = true
	ed.playhead = 12
	tip := ed.playheadTip()
	// session time, the recording, the time INSIDE that recording (which is what
	// the player and ffmpeg count in, and is not the number on the label), the
	// frame, and whether the cut is currently keeping it
	for _, want := range []string{"12.00 s", "take-a.mkv", "00:12.0", "frame 360", "kept"} {
		if !strings.Contains(tip, want) {
			t.Errorf("hover at 12 s = %q, missing %q", tip, want)
		}
	}

	// the second recording starts at 100 s of session time: 130 s is 30 s into
	// it, frame 750 at 25 fps -- and no segment covers it
	ed.playhead = 130
	tip = ed.playheadTip()
	for _, want := range []string{"take-b.mkv", "00:30.0", "frame 750", "cut away"} {
		if !strings.Contains(tip, want) {
			t.Errorf("hover at 130 s = %q, missing %q", tip, want)
		}
	}

	// past the last recording there is no file to name, and no arithmetic to do
	// against a nil video either
	ed.playhead = 500
	if tip = ed.playheadTip(); !strings.Contains(tip, "gap") {
		t.Errorf("hover past the end = %q, want it to admit there is no recording there", tip)
	}
}

// Three separate paths move the playhead -- a left click, the frame buttons, and
// playback following the player's own clock -- and each keeps its own copy of
// the assignment because each does something different around it. A readout
// pushed from only one of them is right after a click and silently wrong after
// every ‹f, which is the worse failure: a stale number is trusted, a missing one
// is not. Source-level, because nothing at run time can tell that the label and
// the field are supposed to be the same fact.
func TestEveryPathThatMovesTheLineSaysSo(t *testing.T) {
	b, err := os.ReadFile("cut.go")
	if err != nil {
		t.Fatal(err)
	}
	lines := strings.Split(string(b), "\n")
	set := regexp.MustCompile(`^\s*ed\.playhead = `)
	n := 0
	for i, ln := range lines {
		if !set.MatchString(ln) {
			continue
		}
		n++
		told := false
		for j := i; j < len(lines) && j < i+8; j++ {
			if strings.Contains(lines[j], "ed.showTime()") {
				told = true
				break
			}
		}
		if !told {
			t.Errorf("cut.go:%d moves the playhead without calling showTime() nearby:\n\t%s",
				i+1, strings.TrimSpace(ln))
		}
	}
	if n < 3 {
		t.Errorf("found %d places that move the playhead, expected the click, the frame "+
			"buttons and playback -- if one was removed, drop it from this count", n)
	}
	// and the reading is on the page, named: it was small print under the
	// transport, which is a caption on a control; it is a line in the quiet
	// column now, with the selection and the totals (cut_form.go)
	if !strings.Contains(string(b), `ed.formIdle.Append(idleRow("Playhead", ed.clock))`) {
		t.Error("the clock is built but never shown")
	}
}

// The selection's readout, under the buttons that consume a selection. Its own
// ⟦ in / out ⟧ / ✕ buttons are gone -- the band made them a second way of doing
// what a drag already does, and marks set by button could exist with no
// selection under them, which left Add refusing while the readout showed a
// range. Same contract as the clock: the mm:ss.d spelling everything else
// uses, dashes exactly as wide as a time while nothing is selected, and pushed
// from every path in this file that changes a mark (the band's own moves push
// from syncSelMarks in cut_selband.go).
func TestTheMarksReadTheSelectionUnderTheCutButtons(t *testing.T) {
	if got, want := marksClock(0, 0, false, false), "--:--.- – --:--.-"; got != want {
		t.Errorf("no marks read %q, want %q", got, want)
	}
	if got, want := marksClock(83.45, 0, true, false), "01:23.5 – --:--.-"; got != want {
		t.Errorf("in only reads %q, want %q", got, want)
	}
	if got, want := marksClock(83.45, 3599.94, true, true), "01:23.5 – 59:59.9"; got != want {
		t.Errorf("both marks read %q, want %q", got, want)
	}
	// every state is the same width; the bar must never twitch as marks come and go
	blank := len(marksClock(0, 0, false, false))
	if l := len(marksClock(83.45, 3599.94, true, true)); l != blank {
		t.Errorf("full readout is %d bytes against %d empty -- the bar will twitch", l, blank)
	}

	b, err := os.ReadFile("cut.go")
	if err != nil {
		t.Fatal(err)
	}
	src := string(b)
	// both paths that change a mark push the label; a readout only one of them
	// updates is right after an Undo and silently stale after the click that
	// dismisses a selection
	lines := strings.Split(src, "\n")
	set := regexp.MustCompile(`^\s*ed\.(markIn, ed\.hasIn|markOut, ed\.hasOut|hasIn, ed\.hasOut) = `)
	n := 0
	for i, ln := range lines {
		if !set.MatchString(ln) {
			continue
		}
		n++
		told := false
		for j := i; j < len(lines) && j < i+10; j++ {
			if strings.Contains(lines[j], "ed.showMarks()") {
				told = true
				break
			}
		}
		if !told {
			t.Errorf("cut.go:%d changes a mark without calling showMarks() nearby:\n\t%s",
				i+1, strings.TrimSpace(ln))
		}
	}
	if n < 2 {
		t.Errorf("found %d places that change a mark, expected clearMarks and the click that "+
			"dismisses a selection -- if one was removed, drop it from this count", n)
	}
	// and the reading is a named line in the quiet column, not small print
	// under the buttons that consume a selection
	for _, want := range []string{
		`ed.formIdle.Append(idleRow("Selection", ed.marks))`,
		`ed.marks.SetText(marksClock(ed.markIn, ed.markOut, ed.hasIn, ed.hasOut))`,
	} {
		if !strings.Contains(src, want) {
			t.Errorf("cut.go no longer contains %q -- the marks readout came unwired", want)
		}
	}
}
