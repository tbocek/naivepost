package main

// Undo has one way to go wrong that a mouse would not catch quickly: a snapshot
// that shares its backing array with the live cut, so editing after an Add
// silently rewrites history. This exercises exactly that.

import (
	"math"
	"path/filepath"
	"testing"
)

func newTestEd(t *testing.T) *cutEditor {
	t.Helper()
	// the hover indices start where buildCut starts them: -1 is "the pointer is
	// on no badge", and a zero there would light the first one red on a page
	// nobody has touched yet
	return &cutEditor{a: &App{outDir: t.TempDir()}, pps: 4, thumbHt: 64,
		rowHov: -1, fxKillHov: -1, bandKillHov: -1, foldHov: -1}
}

func TestCutUndoRestores(t *testing.T) {
	ed := newTestEd(t)
	ed.segs = []cutSeg{{S: 10, E: 20}, {S: 30, E: 40}, {S: 50, E: 60}}

	ed.pushUndo()
	ed.removeRange(28, 42) // drops the middle scene
	if len(ed.segs) != 2 {
		t.Fatalf("remove left %d segments, want 2: %v", len(ed.segs), ed.segs)
	}

	// a further edit must not reach back into the snapshot
	ed.pushUndo()
	ed.segs = append(ed.segs[:0], ed.segs[1:]...)
	ed.persist()

	ed.segs = ed.undo[len(ed.undo)-1].segs
	ed.undo = ed.undo[:len(ed.undo)-1]
	if len(ed.segs) != 2 || !sameSeg(ed.segs[0], cutSeg{S: 10, E: 20}) || !sameSeg(ed.segs[1], cutSeg{S: 50, E: 60}) {
		t.Fatalf("first undo gave %v, want [{10 20} {50 60}]", ed.segs)
	}
	ed.segs = ed.undo[len(ed.undo)-1].segs
	if len(ed.segs) != 3 || !sameSeg(ed.segs[1], cutSeg{S: 30, E: 40}) {
		t.Fatalf("second undo gave %v, want the original three", ed.segs)
	}
}

// Revert must take back the hand-made delta and leave the checkpoint standing.
// The failure that matters is the opposite of undo aliasing: a baseline that
// tracks the live cut, so Revert becomes a no-op or wipes the suggestion too.
func TestCutRevertKeepsBaseline(t *testing.T) {
	ed := newTestEd(t)
	ed.segs = []cutSeg{{S: 100, E: 120}, {S: 200, E: 220}} // as if suggested
	ed.setBase()
	if !sameCut(ed.segs, ed.base.segs) {
		t.Fatal("baseline does not match the cut it was taken from")
	}

	ed.segs = append(ed.segs, cutSeg{S: 300, E: 310}) // ten Adds, abridged
	ed.segs = append(ed.segs, cutSeg{S: 400, E: 410})
	ed.coalesce()
	ed.persist()
	ed.removeRange(195, 225) // and a hand Remove of a suggested scene
	if sameCut(ed.segs, ed.base.segs) {
		t.Fatal("baseline followed the edits")
	}

	ed.pushUndo()
	ed.segs = append([]cutSeg(nil), ed.base.segs...)
	ed.persist()
	if !sameCut(ed.segs, []cutSeg{{S: 100, E: 120}, {S: 200, E: 220}}) {
		t.Fatalf("revert gave %v, want the suggestion back", ed.segs)
	}

	// and the revert itself is undoable
	ed.segs = ed.undo[len(ed.undo)-1].segs
	if len(ed.segs) != 3 {
		t.Fatalf("undo after revert gave %v, want the 3 edited segments", ed.segs)
	}
}

func TestCutSegAt(t *testing.T) {
	ed := newTestEd(t)
	ed.segs = []cutSeg{{S: 10, E: 20}, {S: 30, E: 40}}
	for _, c := range []struct {
		t    float64
		want int
	}{{10, 0}, {19.9, 0}, {20, -1}, {25, -1}, {35, 1}, {99, -1}} {
		if got := ed.segAt(c.t); got != c.want {
			t.Errorf("segAt(%g) = %d, want %d", c.t, got, c.want)
		}
	}
}

func TestCutPersistRoundTrip(t *testing.T) {
	ed := newTestEd(t)
	ed.segs = []cutSeg{{S: 1, E: 5}, {S: 9, E: 12}}
	ed.persist()
	if !exists(filepath.Join(ed.a.cutDir(), "cut.json")) {
		t.Fatal("persist wrote no cut.json")
	}
}

// TestZoomedOutTimelineFitsItsWindow: at the zoom floor the whole session is on
// screen, so the scrollbar has nothing to move and (being Automatic) is not
// there at all. It used to be possible to zoom out and still be too wide, when
// the timeline carried a fixed 26 px of hatching between every two recordings
// and that width did not shrink with the zoom -- five recordings stayed 104 px
// past the window however far you went. Unfilmed time is laid out at no width
// now, so the fit is the duration and nothing else, and this holds it there.
func TestZoomedOutTimelineFitsItsWindow(t *testing.T) {
	for _, c := range []struct{ view, dur float64 }{
		{1200, 3600},  // an hour
		{640, 90},     // narrow window
		{1200, 0.5},   // shorter than the window is wide
		{300, 7200},   // two hours through a keyhole
		{1200, 0.001}, // and a duration that rounds to nothing
	} {
		pps := fitPps(c.view, c.dur)
		w := int(c.dur*pps) + 1 // exactly what relayout lays out with that pps
		if float64(w) > c.view {
			t.Errorf("view %g, %g s: zoomed out to %g px/s the timeline is %d px wide "+
				"-- the scrollbar still moves", c.view, c.dur, pps, w)
		}
		if pps < 0 {
			t.Errorf("view %g, %g s: negative zoom %g", c.view, c.dur, pps)
		}
	}
	// before the first allocation there is no width to fit into, and nothing
	// loaded has no duration to fit: both must be a floor of zero rather than a
	// division by it
	if got := fitPps(0, 3600); got != 0 {
		t.Errorf("with no allocation yet, floor = %g, want 0", got)
	}
	if got := fitPps(1200, 0); got != 0 {
		t.Errorf("with nothing loaded, floor = %g, want 0", got)
	}
}

// The tracks draw only what is on screen -- an hour at the top zoom is 432,000
// px of timeline and no redraw may cost that. What the clipping must not do is
// change WHICH frames are drawn: only every step'th one is, so a range that
// began at the edge of the view instead of on the stride would pick a different
// set at every scroll position and the thumbnails would crawl and reshuffle as
// the timeline moved. Sliding a window across a recording must therefore visit
// exactly the frames one unclipped pass would have.
func TestVisibleFramesAreTheSameFramesEitherWay(t *testing.T) {
	const view = 1400.0 // a timeline window of about that many px
	for _, step := range []int{1, 2, 7, 115} {
		v := &tlVideo{interval: 3, pxOrigin: 40, frames: make([]string, 400)}
		const pps = 4.0
		want := map[int]bool{}
		for i := 0; i < len(v.frames); i += step {
			want[i] = true
		}
		got := map[int]bool{}
		width := v.pxOrigin + float64(len(v.frames))*v.interval*pps
		for x := -view; x < width+view; x += view / 3 { // scrolled past both ends
			first, last := v.frameRange(pps, x, x+view, step)
			if first%step != 0 {
				t.Fatalf("step %d at x %g: range starts at %d, off the stride", step, x, first)
			}
			if first < 0 || last > len(v.frames) || last < first {
				t.Fatalf("step %d at x %g: range [%d,%d) is not inside 0..%d",
					step, x, first, last, len(v.frames))
			}
			for i := first; i < last; i += step {
				got[i] = true
			}
		}
		if len(got) != len(want) {
			t.Errorf("step %d: sliding the view over the whole recording drew %d frames, "+
				"one pass over all of it draws %d", step, len(got), len(want))
		}
		for i := range want {
			if !got[i] {
				t.Errorf("step %d: frame %d is never drawn by any view position", step, i)
				break
			}
		}
	}

	// and nothing is drawn for a recording that is off screen entirely
	v := &tlVideo{interval: 1, pxOrigin: 10_000, frames: make([]string, 100)}
	if first, last := v.frameRange(8, 0, 1400, 1); last > first {
		t.Errorf("a recording 10,000 px to the right still draws frames [%d,%d)", first, last)
	}

	// the ruler's visible span, same idea: at the top zoom a tick a second over
	// an hour is 3600 labels, of which a window holds about twenty
	ticks := 0
	for _, pps := range []float64{0.33, 4, 20, 120} {
		stepS := tickStep(pps)
		from, to := 1000.0/pps, (1000.0+view)/pps // a window somewhere in the middle
		for t := math.Ceil(from/stepS) * stepS; t < to; t += stepS {
			ticks++
		}
	}
	if ticks > 4*40 {
		t.Errorf("%d ruler ticks across four zooms -- a window fits about 20 at any of them", ticks)
	}
}
