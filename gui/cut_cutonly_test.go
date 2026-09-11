package main

// The ✂ Cut only preview: the button that makes ▶ play the FINISHED video
// rather than the recording. Two halves have to hold. The transport half --
// the tick jumps every stretch the cut throws away, and the clock switches to
// the cut's own time, because "3:20 into the session" answers nothing once the
// removed stretches stop playing. And the drawing half -- the track dims what
// is about to be skipped, so the jumps are visible before they happen rather
// than felt as stutters.

import (
	"math"
	"os"
	"strings"
	"testing"
)

// ---- the cut-only preview ---------------------------------------------------

// What the ✂ Cut only scrim covers: everything the finished video will never
// contain. The holes between kept clips, and whatever hangs off either end of
// a recording -- and nothing that is kept, which is the half that matters,
// because dimming a clip that plays would say the cut drops it.
func TestTheDroppedStretchesAreTheOnesNotKept(t *testing.T) {
	ed := &cutEditor{
		vids: []tlVideo{{start: 0, dur: 60}, {start: 60, dur: 40}},
		segs: []cutSeg{
			{S: 5, E: 20},
			{S: 30, E: 55},
			{S: 70, E: 90},
		},
	}
	want := [][2]float64{
		{0, 5},   // the run-up to the first clip
		{20, 30}, // a hole
		// one stretch and not two, though the seam between the recordings is
		// in the middle of it: the axis is session time, the second camera
		// starts where the first stopped, and there is no hole at 60 to break
		// the scrim over. It used to be split there, back when the timeline
		// was files laid end to end rather than a clock
		{55, 70},
		{90, 100}, // and the tail of the last recording
	}
	got := ed.droppedSpans()
	if len(got) != len(want) {
		t.Fatalf("dropped %v, want %v", got, want)
	}
	for i := range want {
		if got[i] != want[i] {
			t.Errorf("dropped span %d is %v, want %v", i, got[i], want[i])
		}
	}
	// nothing that is kept is in there, said as the property rather than as
	// the list: a clip covered by a dropped span is a clip the scrim hides
	for _, s := range ed.segs {
		mid := (s.S + s.E) / 2
		for _, g := range got {
			if mid >= g[0] && mid < g[1] {
				t.Errorf("the middle of the kept clip %v–%v falls inside dropped span %v", s.S, s.E, g)
			}
		}
	}
	// a cut that keeps everything drops nothing
	whole := &cutEditor{vids: []tlVideo{{start: 0, dur: 60}}, segs: []cutSeg{{S: 0, E: 60}}}
	if g := whole.droppedSpans(); len(g) != 0 {
		t.Errorf("a cut that keeps the whole recording dropped %v", g)
	}
	// two cameras on the same minute are ONE minute of dropped time. Counted
	// per recording it would be dropped twice, and the scrim would be painted
	// twice over the same pixels -- harmless there, but the same count is what
	// the zoom-to-fit is measured against, and that would fit the tracks into
	// half the window
	both := &cutEditor{vids: []tlVideo{{start: 0, dur: 60}, {start: 10, dur: 40}}}
	if g := both.droppedSpans(); len(g) != 1 || g[0] != [2]float64{0, 60} {
		t.Errorf("two overlapping recordings with nothing kept drop %v, want one span 0-60", g)
	}
}

// The clock reads the cut's own time under ✂ Cut only: how far into the
// FINISHED video the line is, which is the question that mode is asked. Both
// readings are the same width, or switching modes would shove the whole bar
// sideways -- the reason playheadClock pads its dashes in the first place.
func TestTheCutClockIsTheFinishedVideosOwn(t *testing.T) {
	segs := []cutSeg{{S: 5, E: 20}, {S: 30, E: 55}}
	// 12s into the session is 7s into the cut; 40s in is 15+10 = 25s in
	for _, c := range []struct{ sess, cut float64 }{
		{5, 0}, {12, 7}, {20, 15}, {30, 15}, {40, 25}, {55, 40}, {600, 40},
	} {
		if got := cutPos(segs, c.sess); math.Abs(got-c.cut) > 1e-9 {
			t.Errorf("%.0fs of session reads as %.2fs of cut, want %.0f", c.sess, got, c.cut)
		}
	}
	if a, b := len(playheadClock(12, true)), len(playheadClock(cutPos(segs, 12), true)); a != b {
		t.Errorf("the two clocks are %d and %d characters wide — switching modes will shove the bar", a, b)
	}
}

// The mode is wired: the tick skips, ▶ snaps before the picture starts, the
// button drives the flag, and the track dims what is about to be jumped.
// Pinned as source, like every other gesture on this page.
func TestTheCutOnlyPreviewIsWired(t *testing.T) {
	b, err := os.ReadFile("cut.go")
	if err != nil {
		t.Fatal(err)
	}
	src := string(b)
	for _, want := range []string{
		// the tick jumps the gap, and stops dead past the last clip
		"if ed.skipGap() {",
		"cur, next := gapAt(ed.segs, ed.playhead)",
		"case next != ed.jumped:",
		"ed.setPlayhead(ed.segs[next].S)",
		// ▶ never opens on a frame the cut throws away
		"ed.cutOnlySnap()",
		// the second play button, and the flag that follows it
		"ed.cutPlayIcon = gtk.NewImageFromIconName(\"media-playback-start-symbolic\")",
		"func (ed *cutEditor) playAs(cut bool) {",
		"ed.cutOnly = cut",
		// the clock changes meaning with it, and reads the effects: the
		// finished video's clock is the one the speed effects are in
		"t = ed.cutPos(ed.playhead)",
		"func (ed *cutEditor) cutPos(t float64) float64 { return cutPos(ed.fxSegs(), t) }",
		// and the track says which seconds are about to be skipped
		"for _, g := range ed.droppedSpans() {",
	} {
		if !strings.Contains(src, want) {
			t.Errorf("the cut page no longer contains %q", want)
		}
	}
	// the re-entry guard is reset wherever the cut it counted against is
	// replaced, or the first gap of a freshly loaded project is not jumped
	if n := strings.Count(src, "ed.jumped = -1"); n < 4 {
		t.Errorf("the gap-skip guard is reset in %d places, want the reload, the toggle and both arms of the skip", n)
	}
}

// Where ▶✂ starts is the red line. Inside a clip that is a second the cut
// keeps, so the finished video runs from there; outside one there is nothing to
// run, so the line moves to the next clip and starts from that. Nothing else
// gets a say -- and something else used to: a held clip or edge moves the line
// under plain ▶, which is right there (it is the thing you just trimmed and the
// reason you pressed play), and was also happening under ▶✂, where the question
// is not "show me what I am editing" but "how does the video run from here".
// Asking that wound you back to a boundary you had not asked about.
func TestTheCutPreviewStartsFromTheRedLine(t *testing.T) {
	segs := []cutSeg{{S: 5, E: 20}, {S: 30, E: 55}}
	for _, c := range []struct {
		what     string
		at, want float64
	}{
		{"inside the first clip", 12, 12},
		{"inside the second", 40, 40},
		{"on a clip's own first frame", 30, 30},
		{"in the run-up to the cut", 2, 5},
		{"in a hole between two clips", 25, 30},
		// nothing ahead to move to: the line stays, and the tick stops the
		// preview there rather than playing the tail (skipGap)
		{"past the last clip", 80, 80},
	} {
		ed := &cutEditor{a: &App{}, cutOnly: true, segs: segs, playhead: c.at, hasPlay: true}
		ed.cutOnlySnap()
		if ed.playhead != c.want {
			t.Errorf("%s: ▶✂ starts at %.0fs, want %.0fs", c.what, ed.playhead, c.want)
		}
	}
	// the recording's own preview is not snapped at all: under ▶ a dropped
	// stretch is footage like any other, and moving the line out of it would
	// take away the one way to watch what the cut removed
	ed := &cutEditor{a: &App{}, segs: segs, playhead: 25, hasPlay: true}
	ed.cutOnlySnap()
	if ed.playhead != 25 {
		t.Errorf("▶ moved the line off a removed stretch to %.0fs", ed.playhead)
	}

	// and nothing else moves the line at all any more. ▶ and ▶✂ start where
	// the red line is; the snap above is the one exception, and it is not a
	// hold -- a line in a dropped stretch is a line where the finished video
	// has nothing to play (see toggle).
	body := funcBody(t, "cut.go", `func \(ed \*cutEditor\) toggle\(\) \{`)
	if !strings.Contains(body, "ed.cutOnlySnap()") {
		t.Errorf("▶✂ no longer snaps out of a gap:\n%s", body)
	}
	for _, gone := range []string{"case ed.edgeOn:", "case s != nil:"} {
		if strings.Contains(body, gone) {
			t.Errorf("a held thing chooses where ▶ starts again: %q", gone)
		}
	}
}

// An empty cut under ✂ Cut only is an empty video, so ▶ has nothing it could
// honestly play -- with no clips there are no gaps to skip, and the preview
// would run the whole recording against the mode's one promise. The button
// greys out, and every other way into playing refuses for the same reason.
// The sensitivity runs on live widgets, so the wiring is pinned to the source;
// the refusal itself is exercised.
func TestAnEmptyCutHasNothingToPlay(t *testing.T) {
	// the refusal comes before the player is even asked: a click on the picture
	// and the run bar land in toggle too, and none of them may start playback
	ed := &cutEditor{a: &App{}, cutOnly: true}
	ed.toggle() // must return without touching the (absent) player
	if ed.started {
		t.Error("toggling an empty cut-only preview counted as started")
	}

	b, err := os.ReadFile("cut.go")
	if err != nil {
		t.Fatal(err)
	}
	src := string(b)
	for _, want := range []string{
		// the one rule, in the one place that draws it -- and it greys ▶✂,
		// never plain ▶: the recording is always there to play
		"ed.cutPlayBtn.SetSensitive(len(ed.segs) > 0)",
		// ...and toggle refuses on the same rule, saying why
		"the cut is empty — add a clip to play it",
		// the toolbar clock stays on the SESSION's time too: an empty cut's own
		// clock reads 0:00.0 wherever the red line stands, and "where is the
		// line" is the one thing the clock is for
		"if ed.cutOnly && len(ed.segs) > 0 {",
	} {
		if !strings.Contains(src, want) {
			t.Errorf("the cut page no longer contains %q", want)
		}
	}
	// synced from the mode's toggle AND from syncButtons, which every edit
	// passes through -- or the button would stay grey after the first clip is
	// added, or stay live after the last is removed
	if n := strings.Count(src, "ed.syncPlayBtn()"); n < 2 {
		t.Errorf("syncPlayBtn is called from %d places, want the ✂ toggle and syncButtons both", n)
	}
}

// Two play buttons instead of a mode toggle: ▶ plays the recording, ▶✂ plays
// the cut, and the flag the old toggle set now simply follows whichever was
// pressed. The presses run headless (no player, so nothing actually rolls);
// what must hold is the state each press leaves behind.
func TestEachPlayButtonBringsItsOwnPreview(t *testing.T) {
	ed := &cutEditor{a: &App{}, hasPlay: true}
	ed.segs = []cutSeg{{S: 10, E: 20}}

	// ▶✂ makes the preview the cut -- and delivers kept material immediately:
	// a playhead parked in a dropped stretch moves to the first kept frame
	ed.playAs(true)
	if !ed.cutOnly {
		t.Error("▶✂ did not make the preview the cut")
	}
	if ed.playhead != 10 {
		t.Errorf("playhead = %v after ▶✂ from a dropped stretch, want snapped to 10", ed.playhead)
	}

	// plain ▶ takes the preview back to the recording
	ed.playAs(false)
	if ed.cutOnly {
		t.Error("▶ did not take the preview back to the recording")
	}

	// pressing the button whose preview is already current is a plain
	// play/pause, not a re-switch: the flag stays put
	ed.playAs(false)
	if ed.cutOnly {
		t.Error("a second ▶ flipped the preview to the cut")
	}
}

// The wiring of the pair: each button's face answers only for its own preview,
// and the ▶✂ face stays lit while the preview is the cut at all. Buttons and
// CSS are live widgets, so the seam is pinned in the source.
func TestTheTwoPlayButtonsAreWired(t *testing.T) {
	src := readSrc(t, "pipeline.go")
	for _, want := range []string{
		// plain ▶ wears ⏸ only while the RECORDING runs...
		"setPlayIcon(a.ed.playBtn, a.ed.playing() && !a.ed.cutOnly,",
		// ...and ▶✂ is redrawn beside it on every state change
		"a.ed.syncCutPlay()",
	} {
		if !strings.Contains(src, want) {
			t.Errorf("pipeline.go no longer contains %q", want)
		}
	}
	src = readSrc(t, "cut.go")
	for _, want := range []string{
		// the pause face only while the cut itself is running
		"if ed.playing() && ed.cutOnly {",
		"ed.cutPlayIcon.SetFromIconName(\"media-playback-pause-symbolic\")",
		// the lamp: a lit face for as long as the preview is the cut
		"ed.cutPlayBtn.AddCSSClass(\"suggested-action\")",
		// each button hands its own idea of the preview to the shared press
		"ed.playBtn.ConnectClicked(func() { ed.playAs(false) })",
		"ed.cutPlayBtn.ConnectClicked(func() { ed.playAs(true) })",
	} {
		if !strings.Contains(src, want) {
			t.Errorf("cut.go no longer contains %q", want)
		}
	}
}

// Pressing play must not move the page.
//
// The cut's play button wore a label of two glyphs -- "▶✂" and "⏸✂" -- and
// those two glyphs do not come from the same font. On an ordinary Linux
// desktop U+25B6 (▶) is in the text face and U+23F8 (⏸) is not, so Pango falls
// back to the emoji face for it, which is taller. The button grew a few px the
// moment the preview started, the toolbar row grew with it, and the whole
// timeline under it stepped down -- and back up again on pause. The area was
// the same; everything in it had moved.
//
// The play/pause half is an icon now, which is one size in both states, and
// the ✂ that made it a label in the first place is a label of its own that
// never changes.
func TestPressingPlayDoesNotResizeTheButton(t *testing.T) {
	src := readSrc(t, "cut.go")
	for _, want := range []string{
		`ed.cutPlayIcon = gtk.NewImageFromIconName("media-playback-start-symbolic")`,
		`face.Append(gtk.NewLabel("✂"))`,
		"ed.cutPlayBtn.SetChild(face)",
		`ed.cutPlayIcon.SetFromIconName("media-playback-pause-symbolic")`,
		`ed.cutPlayIcon.SetFromIconName("media-playback-start-symbolic")`,
	} {
		if !strings.Contains(src, want) {
			t.Errorf("the cut's play button no longer keeps one size: %q is gone", want)
		}
	}
	// the two-glyph label is gone from the button, not merely unused: a
	// SetLabel anywhere on it puts the resize back
	if strings.Contains(src, "ed.cutPlayBtn.SetLabel(") {
		t.Error("the button's face is a label again, so its size follows the font the glyph falls to")
	}
	// the other play button was always an icon and stays one
	if !strings.Contains(src, `ed.playBtn = gtk.NewButtonFromIconName("media-playback-start-symbolic")`) {
		t.Error("the recording's play button is no longer an icon button")
	}
}
