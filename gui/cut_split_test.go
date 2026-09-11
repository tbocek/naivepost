package main

// | Split, and the drag that puts a split back together.
//
// What is pinned here is the pair: a border either side of the selection that
// costs the video nothing, and a border that survives the next edit somewhere
// else -- because two touching clips of one camera are the same clip to
// coalesce, and a split that the next press swallowed would be a button that
// works until you use the page.

import (
	"math"
	"strings"
	"testing"
)

// splitEd is a session with one long recording and one kept minute, like the
// remove button's -- the two verbs act on the same thing.
func splitEd(t *testing.T) (*App, *cutEditor) {
	t.Helper()
	ed := newTestEd(t)
	a := ed.a
	a.ed = ed
	ed.vids = []tlVideo{{base: "a", path: "a.mkv", start: 0, dur: 120, interval: 5, fps: 30}}
	ed.relayout()
	ed.segs = []cutSeg{{S: 0, E: 60}}
	return a, ed
}

func keptOf(segs []cutSeg) float64 {
	kept := 0.0
	for _, s := range segs {
		if !s.isInsert() {
			kept += s.E - s.S
		}
	}
	return kept
}

// The button's reason for existing: ten seconds marked in the middle of a kept
// minute become a scene, and the video is unchanged. Removing them and adding
// them back was the only way to draw those borders, which is two presses that
// cancel each other and a history that says a removal happened.
func TestSplittingASelectionMakesASceneWithoutChangingTheCut(t *testing.T) {
	a, ed := splitEd(t)
	ed.sel.t0, ed.sel.t1, ed.sel.active = 20, 30, true
	a.splitSelRange()

	segsEqual(t, ed.segs, []cutSeg{{S: 0, E: 20}, {S: 20, E: 30}, {S: 30, E: 60}})
	if got := keptOf(ed.segs); math.Abs(got-60) > 1e-9 {
		t.Errorf("the cut keeps %g s after a split, want the 60 it kept before", got)
	}
	// the middle one is the selection, and it is the one wearing the flag on
	// its start; the last one wears it on the border at 30
	if !ed.segs[1].Split || !ed.segs[2].Split {
		t.Errorf("the new borders are not marked as deliberate: %+v", ed.segs)
	}
	if ed.segs[0].Split {
		t.Error("the border the cut already had was marked as one this press drew")
	}
	if len(ed.undo) != 1 {
		t.Errorf("the split left %d undo step(s), want 1", len(ed.undo))
	}
	// the band stays up: it is exactly the scene just made, and the reason for
	// making one is the press after this one
	if !ed.sel.active {
		t.Error("the selection came down, so the scene it just made is not in hand")
	}
	ed.undoLast()
	segsEqual(t, ed.segs, []cutSeg{{S: 0, E: 60}})
}

// The border has to outlive the next press. Two clips of one camera that touch
// are one clip to coalesce, which runs on nearly every edit -- so without the
// flag a split was undone by an Add at the other end of the timeline.
func TestASplitBorderSurvivesTheNextEditElsewhere(t *testing.T) {
	a, ed := splitEd(t)
	ed.sel.t0, ed.sel.t1, ed.sel.active = 20, 30, true
	a.splitSelRange()

	ed.coalesce() // what an Add, a Remove or a suggestion all end with
	if len(ed.segs) != 3 {
		t.Fatalf("the split was swallowed by the next edit: %+v", ed.segs)
	}
	// ...and two clips that merely happen to touch are still merged, which is
	// what the flag has to leave alone
	ed.segs = []cutSeg{{S: 0, E: 20}, {S: 20, E: 40}}
	ed.coalesce()
	segsEqual(t, ed.segs, []cutSeg{{S: 0, E: 40}})
}

// A press that draws nothing says so, and leaves no undo step: a history entry
// that undoes nothing is worse than a refusal.
func TestSplittingWhereNoBorderCanGoRefuses(t *testing.T) {
	a, ed := splitEd(t)
	for _, c := range []struct {
		what   string
		t0, t1 float64
		aud    string
		on     bool
	}{
		{"with nothing selected", 20, 30, "", false},
		{"a selection pointed at a waveform", 20, 30, "mic", true},
		{"past everything the cut keeps", 80, 90, "", true},
		{"a sliver of a second at the scene's own edge", 0, 0.5, "", true},
	} {
		ed.undo = nil
		ed.segs = []cutSeg{{S: 0, E: 60}}
		ed.sel.t0, ed.sel.t1, ed.sel.active, ed.sel.aud = c.t0, c.t1, c.on, c.aud
		a.splitSelRange()

		segsEqual(t, ed.segs, []cutSeg{{S: 0, E: 60}})
		if len(ed.undo) != 0 {
			t.Errorf("%s: left %d undo step(s) over a cut that did not move",
				c.what, len(ed.undo))
		}
	}
	// one end already on a border draws the other one, and only that one
	ed.sel.aud, ed.sel.active = "", true
	ed.sel.t0, ed.sel.t1 = 0, 30
	a.splitSelRange()
	segsEqual(t, ed.segs, []cutSeg{{S: 0, E: 30}, {S: 30, E: 60}})
}

// The inverse: drag a clip up against the one beside it and they are one clip
// again. It is the only thing that clears a deliberate border, and it clears
// it on the pair it joins -- one drag must not re-merge every split in the cut.
func TestDraggingAClipOntoItsNeighbourJoinsThem(t *testing.T) {
	a, ed := splitEd(t)
	ed.sel.t0, ed.sel.t1, ed.sel.active = 20, 30, true
	a.splitSelRange() // 0-20, 20-30, 30-60, the last two deliberate

	// the middle one taken in hand and dragged back against its left
	// neighbour, which is where it already sits: the drop is the join
	ed.segOn, ed.segSel = true, 1
	if !ed.mergeDropped() {
		t.Fatal("a clip dropped against its neighbour did not join it")
	}
	segsEqual(t, ed.segs, []cutSeg{{S: 0, E: 30}, {S: 30, E: 60}})
	// the OTHER split border is untouched: it was not part of this drag
	if !ed.segs[1].Split {
		t.Error("one drag cleared a border at the other end of the cut")
	}
	// coalesce dropped the hold, as it does whenever the list is rebuilt
	if ed.segOn {
		t.Error("a clip is still held after the list under it was rebuilt")
	}

	// a clip that touches nothing is not joined to anything
	ed.segs = []cutSeg{{S: 0, E: 20}, {S: 30, E: 60}}
	ed.segOn, ed.segSel = true, 0
	if ed.mergeDropped() {
		t.Errorf("a clip with a gap beside it was joined anyway: %+v", ed.segs)
	}
	// nor is one dropped against another camera: that seam IS the cut from one
	// to the other, and merging it would keep the seconds and lose the switch
	ed.segs = []cutSeg{{S: 0, E: 20}, {S: 20, E: 60, Cam: 1}}
	ed.segOn, ed.segSel = true, 0
	if ed.mergeDropped() {
		t.Errorf("two cameras were merged into one scene: %+v", ed.segs)
	}
}

// The seams: the button is on the bar between the pair it belongs to, it calls
// the selection's verb and no other, it is greyed by ＋ Add's rule, and the
// drop asks for the merge before it writes the cut to disk.
func TestTheSplitButtonIsWired(t *testing.T) {
	src := readSrc(t, "cut.go")
	for _, want := range []string{
		`ed.splitBtn = gtk.NewButtonFromIconName("edit-cut-symbolic")`,
		"ed.splitBtn.ConnectClicked(func() { a.splitSelRange() })",
		"linked(add, ed.splitBtn, ed.remBtn, ed.copyBtn, ed.pasteBtn, ins, ed.laneBtn)",
		"ed.splitBtn.SetSensitive(!snd && (on || ed.hasPlay))",
		// the drop: the join is decided before the write, so what lands on
		// disk is the merged cut
		"merged := ed.segDirty && ed.mergeDropped()",
	} {
		if !strings.Contains(src, want) {
			t.Errorf("cut.go has lost %q", want)
		}
	}
	i := strings.Index(src, "merged := ed.segDirty && ed.mergeDropped()")
	j := strings.Index(src[i:], "ed.persist()")
	if i < 0 || j < 0 {
		t.Error("the drop no longer writes the cut after the merge")
	}
	// and the flag is a field of the cut, so a split survives being saved and
	// opened again
	if !strings.Contains(src, "Split bool `json:\"split,omitempty\"`") {
		t.Error("a deliberate border is not stored, so it lasts until the project is reopened")
	}
}

// The other way to close a gap, and the commoner one: drag a clip's BORDER out
// until it meets the next clip. Sliding a whole clip against its neighbour
// moves the footage with it; extending the border keeps what is between, which
// is what "join these two" means when the cut has dropped the seconds in the
// middle.
func TestTrimmingABorderOntoTheNextClipJoinsThem(t *testing.T) {
	a, ed := splitEd(t)
	_ = a
	ed.segs = []cutSeg{{S: 0, E: 20}, {S: 30, E: 60}}

	// the first clip's end, taken in hand and dragged out to the second's start
	ed.edgeOn, ed.edgeSeg, ed.edgeEnd = true, 0, true
	ed.moveEdgeTo(30, false)
	if ed.segs[0].E != 30 {
		t.Fatalf("the border stopped at %g, so it never met the next clip", ed.segs[0].E)
	}
	if !ed.mergeTouching(0) {
		t.Fatal("a border trimmed onto the next clip did not join them")
	}
	segsEqual(t, ed.segs, []cutSeg{{S: 0, E: 60}})

	// a border that stops short of the next clip joins nothing: the gap is
	// still a gap, and closing it is what the gesture was for
	ed.segs = []cutSeg{{S: 0, E: 20}, {S: 30, E: 60}}
	ed.edgeOn, ed.edgeSeg, ed.edgeEnd = true, 0, true
	ed.moveEdgeTo(28, false)
	if ed.mergeTouching(0) {
		t.Errorf("a border two seconds short joined anyway: %+v", ed.segs)
	}
	// and the drop asks for it, before the cut is written
	src := readSrc(t, "cut.go")
	if !strings.Contains(src, "merged := ed.edgeDirty && ed.mergeTouching(ed.edgeSeg)") {
		t.Error("a trimmed border no longer asks whether it closed a gap")
	}
}

// | Split with nothing selected is a razor at the red line.
//
// A region has two ends and gets two borders; a line names one place and gets
// one. Refusing the second gesture because no region was drawn is the page
// insisting on a drag for a cut that needs one number -- and "Split" on a
// toolbar reads as a razor to every hand that has used an editor before.
func TestSplitWithNothingSelectedCutsAtTheLine(t *testing.T) {
	a, ed := splitEd(t)
	ed.sel.active = false
	ed.playhead, ed.hasPlay = 25, true
	a.splitSelRange()

	segsEqual(t, ed.segs, []cutSeg{{S: 0, E: 25}, {S: 25, E: 60}})
	if !ed.segs[1].Split {
		t.Error("the border it drew is not marked deliberate, so the next edit will swallow it")
	}
	if len(ed.undo) != 1 {
		t.Errorf("the split left %d undo step(s), want 1", len(ed.undo))
	}
	// the new right-hand clip is in hand: two touching scenes are one stretch
	// of green with one more line on it, and the outline is what says a press
	// did something
	if !ed.segOn || ed.segSel != 1 {
		t.Errorf("after the cut the held clip is on=%v i=%d, want clip 2", ed.segOn, ed.segSel)
	}
	ed.undoLast()
	segsEqual(t, ed.segs, []cutSeg{{S: 0, E: 60}})

	// with no line either, it says what it needs rather than doing nothing
	ed.undo, ed.hasPlay = nil, false
	a.splitSelRange()
	segsEqual(t, ed.segs, []cutSeg{{S: 0, E: 60}})
	if len(ed.undo) != 0 {
		t.Error("a press with nothing to cut at left an undo step")
	}
	// nor in a stretch the cut does not keep
	ed.segs = []cutSeg{{S: 0, E: 20}}
	ed.playhead, ed.hasPlay = 40, true
	a.splitSelRange()
	segsEqual(t, ed.segs, []cutSeg{{S: 0, E: 20}})
	if len(ed.undo) != 0 {
		t.Error("a press where the cut keeps nothing left an undo step")
	}
}

// A click that takes a clip in hand does not move the picture. The second
// press of a double click lands on a clip that is already held and ends a drag
// that went nowhere: putting the line on the clip's start there yanks the
// picture away from the frame the hand just clicked.
func TestPickingUpAClipLeavesTheLineWhereItIs(t *testing.T) {
	src := readSrc(t, "cut.go")
	i := strings.Index(src, "merged := ed.segDirty && ed.mergeDropped()")
	if i < 0 {
		t.Fatal("the clip drop no longer asks about the join")
	}
	tail := src[i:min(len(src), i+900)]
	j, k := strings.Index(tail, "ed.segDirty = false"), strings.Index(tail, "ed.showSeg(false)")
	if j < 0 || k < 0 || k < j {
		t.Fatal("the drop no longer cues the picture after a move")
	}
	// the cue is INSIDE the "it actually moved" branch, the same rule the held
	// edge above keeps
	if end := strings.Index(tail, "if merged {"); end < 0 || k > end {
		t.Error("the picture is cued after every drop, moved or not — a press that " +
			"only picked the clip up moves the red line")
	}
}
