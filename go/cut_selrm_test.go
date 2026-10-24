package main

import (
	"os"
	"strings"
	"testing"
)

// rmSelEd is a session with one long recording, one kept minute, and a hand
// ready to draw on it.
func rmSelEd(t *testing.T) (*App, *cutEditor) {
	t.Helper()
	ed := newTestEd(t)
	a := ed.a
	a.ed = ed
	ed.vids = []tlVideo{{base: "a", path: "a.mkv", start: 0, dur: 120, interval: 5, fps: 30}}
	ed.relayout()
	ed.segs = []cutSeg{{S: 0, E: 60}}
	return a, ed
}

// The button's reason for existing: ten seconds marked in the middle of a kept
// minute come out, and what is left is two kept stretches either side. The ✕ on
// a scene cannot do this -- there is no scene to press for a hole, and the two
// scenes it makes did not exist until the span was drawn.
func TestRemovingASelectionMakesTwoScenesOutOfOne(t *testing.T) {
	a, ed := rmSelEd(t)
	ed.sel.t0, ed.sel.t1, ed.sel.active = 20, 30, true
	a.removeSelRange()

	segsEqual(t, ed.segs, []cutSeg{{S: 0, E: 20}, {S: 30, E: 60}})
	if len(ed.undo) != 1 {
		t.Errorf("the remove left %d undo step(s), want 1", len(ed.undo))
	}
	if ed.sel.active {
		t.Error("the band stayed up over footage that is no longer there")
	}
}

// A press that changes nothing must say so. Saying "removed" over a cut that
// did not move -- and leaving an undo step that undoes nothing -- is the one
// status line that cannot be trusted afterwards.
func TestRemovingWhereTheCutKeepsNothingRefusesInsteadOfPretending(t *testing.T) {
	a, ed := rmSelEd(t)
	for _, c := range []struct {
		what   string
		t0, t1 float64
	}{
		{"past the end of the kept minute", 80, 90},
		{"exactly on the scene's far border", 60, 70},
	} {
		ed.undo = nil
		ed.segs = []cutSeg{{S: 0, E: 60}}
		ed.sel.t0, ed.sel.t1, ed.sel.active = c.t0, c.t1, true
		a.removeSelRange()

		segsEqual(t, ed.segs, []cutSeg{{S: 0, E: 60}})
		if len(ed.undo) != 0 {
			t.Errorf("%s: left %d undo step(s) over a cut that did not move",
				c.what, len(ed.undo))
		}
		if !ed.sel.active {
			t.Errorf("%s: cleared the band, so there is nothing left to move", c.what)
		}
	}
}

// The old toolbar remove guessed -- selection, then held clip, then whatever
// the playhead sat on -- and that is why it was taken off the bar. This one is
// the selection's verb and nothing else's, so with no selection it removes
// nothing, however plainly the red line is sitting on a scene.
func TestRemoveWithNoSelectionRemovesNothingRatherThanGuessing(t *testing.T) {
	a, ed := rmSelEd(t)
	ed.playhead, ed.hasPlay = 30, true
	// and the band is down but still remembers where it was, which is what
	// every cleared selection looks like from in here
	ed.sel.t0, ed.sel.t1, ed.sel.active = 20, 30, false
	a.removeSelRange()
	segsEqual(t, ed.segs, []cutSeg{{S: 0, E: 60}})
	if len(ed.undo) != 0 {
		t.Errorf("a press with nothing selected edited the cut: %d undo step(s)", len(ed.undo))
	}
}

// ＋ Add and － Remove are one pair over one span, so they refuse the same
// selection for the same reason: a ▼ selection is about a sound, and there is
// no way to drop the sound and keep the picture it was filmed with.
func TestRemoveRefusesASoundSelection(t *testing.T) {
	a, ed := rmSelEd(t)
	ed.auds = []tlAudio{{base: "mic", path: "mic.wav", start: 0, dur: 120}}
	ed.sel.t0, ed.sel.t1, ed.sel.active = 20, 30, true
	ed.sel.aud = "mic"
	a.removeSelRange()
	segsEqual(t, ed.segs, []cutSeg{{S: 0, E: 60}})

	// and pointing it back at the picture hands the verb over: the refusal is
	// about the scope, not about the span being unusable
	ed.sel.aud = ""
	a.removeSelRange()
	segsEqual(t, ed.segs, []cutSeg{{S: 0, E: 20}, {S: 30, E: 60}})
}

// The seams: the button is on the bar in the pair it belongs to, it calls the
// selection's verb and no other, and it is greyed by the same rule as ＋ Add.
func TestTheRemoveButtonIsWired(t *testing.T) {
	src, err := os.ReadFile("cut.go")
	if err != nil {
		t.Fatal(err)
	}
	cut := string(src)
	for _, want := range []string{
		`ed.remBtn = gtk.NewButtonFromIconName("list-remove-symbolic")`,
		"ed.remBtn.ConnectClicked(func() { a.removeSelRange() })",
		"linked(add, ed.splitBtn, ed.remBtn, ed.copyBtn, ed.pasteBtn, ins, ed.laneBtn)",
		"ed.remBtn.SetSensitive(on)",
	} {
		if !strings.Contains(cut, want) {
			t.Errorf("cut.go has lost %q", want)
		}
	}
}

// "2 scenes, was 1" over a press that removed something has to be worked out
// before it reads right, and the split is the one outcome this button exists
// for. It says it in words; the rest report the count.
func TestTheStatusNamesTheSplitAndCountsTheRest(t *testing.T) {
	if got := removedMsg(10, 1, 2); !strings.Contains(got, "is two now") ||
		!strings.Contains(got, "10.0 s") {
		t.Errorf("a hole cut through one scene said %q", got)
	}
	if got := removedMsg(30, 3, 2); !strings.Contains(got, "2 scene(s), was 3") {
		t.Errorf("a whole scene removed said %q", got)
	}
	if got := removedMsg(30, 3, 2); strings.Contains(got, "is two now") {
		t.Errorf("a cut that lost a scene claimed a split: %q", got)
	}
}

// A verb is live when it has something to act on. All four of the bar's verbs
// used to be live with nothing selected at all, and the press then answered
// "drag a region on a track first" -- a click spent to be told it was never
// the button. And ＋ Add was the blue one, which says "press this first" about
// a control that cannot be pressed until seconds are marked.
func TestTheCutVerbsAreLiveOnlyWhenTheyHaveSomethingToActOn(t *testing.T) {
	body := funcBody(t, "cut.go", `func \(ed \*cutEditor\) syncSelBtns\(\) \{`)
	for _, want := range []string{
		// a footage selection, and one long enough to be a scene
		`on := ed.sel.active && ed.sel.aud == ""`,
		"long := on && math.Abs(ed.sel.t1-ed.sel.t0) >= minSegLn",
		"ed.addBtn.SetSensitive(long)",
		"ed.remBtn.SetSensitive(on)",
		// ...except Split, which cuts at the red line when nothing is selected
		"ed.splitBtn.SetSensitive(!snd && (on || ed.hasPlay))",
	} {
		if !strings.Contains(body, want) {
			t.Errorf("the bar's verbs are live with nothing to act on: %q", want)
		}
	}
	// the gate follows the line as well as the selection, or Split would stay
	// grey until something was selected
	if !strings.Contains(funcBody(t, "cut.go", `func \(ed \*cutEditor\) setPlayhead\(t float64\) \{`), "ed.syncSelBtns()") {
		t.Error("putting the red line down does not wake | Split")
	}
	// and Add is not the page's primary verb: ▶ on the run bar is
	src := readSrc(t, "cut.go")
	if strings.Contains(src, `add.AddCSSClass("suggested-action")`) {
		t.Error("＋ Add is blue again")
	}
}
