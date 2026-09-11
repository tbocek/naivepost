package main

import (
	"fmt"
	"math"
	"strings"

	"github.com/diamondburned/gotk4/pkg/cairo"
)

// Clear empties the cut -- kept stretches and effects, including proposed ones
// -- and leaves the session alone: sources, rows, clock corrections, cut lanes.
// One undo step for the lot.

// clearCut empties the cut. Nothing at all to clear is not an edit and says so
// rather than leaving an undo step that undoes nothing.
func (ed *cutEditor) clearCut() {
	if len(ed.segs) == 0 && len(ed.fx) == 0 {
		ed.a.setStatus("nothing to clear — the timeline holds no cut yet")
		return
	}
	segs, fx := len(ed.segs), len(ed.fx)
	ed.pushUndo()
	ed.segs, ed.fx = nil, nil
	// every hold is an index into a list that is now empty, and the selection
	// is a span over footage the cut no longer keeps
	ed.dropSeg()
	ed.dropEdge()
	ed.dropFx()
	ed.dropSel()
	ed.sel.active = false
	ed.clearMarks()
	ed.persist()
	ed.syncSelBtns()
	ed.redrawTracks()
	ed.a.setStatus(fmt.Sprintf("cleared %s and %s", plural(segs, "scene"), plural(fx, "effect")))
}

// － Remove is the SELECTION's verb and nothing else's: it cuts a hole in a kept
// scene, which the ✕ on a bar cannot (there is no scene to press for a span that
// makes two). No selection, a sound-scoped selection, or nothing kept under it:
// it says so instead of guessing. Greyed under the same rule as ＋ Add.

// spanTouches reports whether removing t0..t1 would change the cut at all.
//
// It is the same test removeSpan makes on each segment, asked ahead of time:
// anything with a positive overlap is trimmed, split or (an insert) dropped
// whole, and anything else is copied across untouched. So a false here means
// the press would be a no-op, which is the one thing the status line must not
// call a removal.
func (ed *cutEditor) spanTouches(t0, t1 float64) bool {
	if t1 < t0 {
		t0, t1 = t1, t0
	}
	for _, s := range ed.segs {
		if s.E > t0 && s.S < t1 {
			return true
		}
	}
	return false
}

// removeSelRange is － Remove: the selected stretch comes out of the cut.
func (a *App) removeSelRange() {
	ed := a.ed
	// the band, not its numbers: sel.t0/t1 keep whatever the last drag left in
	// them after the band comes down, so a press with nothing selected would
	// otherwise cut a hole where a selection USED to be. (＋ Add asks for
	// footage as well as a band; this one does not need to -- it edits the cut,
	// and a cut with scenes in it is one there is something to remove from.)
	if !ed.sel.active {
		a.setStatus("drag a region on a track first")
		return
	}
	// the mirror of ＋ Add's guard, in the same words: both buttons act on
	// footage, and a selection pointed at a waveform is not about footage. The
	// button is greyed for this; the guard is for every other way in.
	if ed.sel.aud != "" {
		a.setStatus(fmt.Sprintf("－ Remove drops footage — the selection is %s's sound", ed.sel.aud))
		return
	}
	if !ed.spanTouches(ed.sel.t0, ed.sel.t1) {
		a.setStatus(fmt.Sprintf("nothing to remove: the cut keeps nothing between %s and %s",
			mmss(math.Min(ed.sel.t0, ed.sel.t1)), mmss(math.Max(ed.sel.t0, ed.sel.t1))))
		return
	}
	// measured on the finished video's clock, not on the session's: a stretch
	// under a ×2 is half as many seconds of video as it is of footage, and
	// "removed 20 s" over a video that got 10 s shorter is the number nobody
	// can check against the total under the tracks (cutLen).
	before, was := len(ed.segs), ed.cutLen()
	ed.pushUndo()
	ed.removeRange(ed.sel.t0, ed.sel.t1)
	ed.sel.active = false
	ed.clearMarks()
	a.setStatus(removedMsg(was-ed.cutLen(), before, len(ed.segs)))
}

// removedMsg is what the line says afterwards.
//
// The scene count going UP is the whole point of this button and reads as a
// mistake unless it is named -- "2 scenes, was 1" over a press that REMOVED
// something is a sentence that has to be worked out. So the split says what
// happened in words, and everything else says the count.
func removedMsg(gone float64, before, after int) string {
	if after > before {
		return fmt.Sprintf("removed %.1f s — the scene it went through is two now "+
			"(↶ Undo takes it back)", gone)
	}
	return fmt.Sprintf("removed %.1f s — %d scene(s), was %d (↶ Undo takes it back)",
		gone, after, before)
}

// Dropping a scene: the ✕ on the green bar in the selection row (bandKillAt),
// which stands for the clip in hand or under the red line. ⌦ still removes
// whatever is in hand. drawKillBadge is the one recipe every ✕ on the page
// wears -- bar, cut lane, emptied row, effect.

const (
	segKillR   = 4.0  // the arms of the ✕, from its centre
	segKillPad = 3.0  // the plate's edge, beyond the arms
	segKillHit = 10.0 // and the target, which is bigger than either
	segKillTop = 11.0 // its centre, down from the top of the picture band
	// ...and in from the edge it sits against, ONE number for every ✕. Every such
	// edge can be grabbed within edgeGrab px, so the badge's target begins exactly
	// where the grab stops -- otherwise "a bit shorter" finds "gone". Same rule as
	// the speaker badges (hearIn).
	killIn = edgeGrab + segKillHit
	// ...for a badge drawn against an edge. Plated ✕ sit in the MIDDLE of what
	// they remove (drawSelBand, fxKillCentre) since the ends are grips; killMin is
	// the floor under which there is no middle to spare and no ✕ is drawn.
	killMin = 2 * killIn
)

// drawKillBadge paints one ✕ centred on cx,cy: a plate, then the arms.
//
// A plate under the mark, always: two white strokes laid straight onto a
// thumbnail are legible over a dark frame and invisible over a bright one, and
// a control that disappears on half the footage is not a control. Red only
// under the pointer -- a row of red buttons standing open over the cut reads
// as damage already done.
func drawKillBadge(cr *cairo.Context, cx, cy float64, hot bool) {
	if hot {
		plate(cr, cx, cy, segKillR+segKillPad, 0.85, 0.24, 0.28, 0.95)
	} else {
		plate(cr, cx, cy, segKillR+segKillPad, 0.06, 0.06, 0.07, 0.55)
	}
	cr.SetSourceRGBA(1, 1, 1, 0.9)
	cr.SetLineWidth(1.6)
	for _, d := range [][2]float64{{-1, -1}, {-1, 1}} {
		cr.MoveTo(cx+segKillR*d[0], cy+segKillR*d[1])
		cr.LineTo(cx-segKillR*d[0], cy-segKillR*d[1])
	}
	cr.Stroke()
}

// killSeg drops scene i. It is removeSelClicked's own arithmetic with the
// guessing taken out: the scene is named by the press, so there is no held
// clip and no playhead to consult.
func (ed *cutEditor) killSeg(i int) {
	if i < 0 || i >= len(ed.segs) {
		return
	}
	s := ed.segs[i]
	ed.pushUndo()
	ed.segs = append(ed.segs[:i], ed.segs[i+1:]...)
	// whatever was in hand was holding an index, and the indices have moved
	ed.dropSeg()
	ed.dropEdge()
	ed.bandKillHov = -1 // the indices moved; whatever was lit is not that clip now
	ed.persist()
	ed.a.setStatus(fmt.Sprintf("removed the scene at %s (%s) — ↶ Undo takes it back",
		mmss(s.S), ed.spanSecs(s.S, s.E)))
	ed.redrawTracks()
}

// | Split cuts the span free of the scene it lies in: nothing is removed,
// nothing moves, the render shows the same frames. The border is DELIBERATE and
// stored (cutSeg.Split), because coalesce would otherwise merge two touching
// clips of one camera on the next edit. Only mergeDropped clears it.

// splitSelRange is the button: a border at each end of the selection. Same
// guards as － Remove, plus its own no-op case: a selection in a gap, or whose
// ends fall on existing borders, says why nothing changed.
func (a *App) splitSelRange() {
	ed := a.ed
	if !ed.sel.active {
		// no band drawn: the red line is the border. A clip in hand and a
		// press on Split is a razor, which is what the button looks like it
		// is -- and refusing because no REGION was drawn is the page insisting
		// on a gesture for a cut that needs one number, not two.
		a.splitAtLine()
		return
	}
	if ed.sel.aud != "" {
		a.setStatus(fmt.Sprintf("| Split cuts footage — the selection is %s's sound", ed.sel.aud))
		return
	}
	t0, t1 := math.Min(ed.sel.t0, ed.sel.t1), math.Max(ed.sel.t0, ed.sel.t1)
	if !ed.spanTouches(t0, t1) {
		a.setStatus(fmt.Sprintf("nothing to split: the cut keeps nothing between %s and %s",
			mmss(t0), mmss(t1)))
		return
	}
	// asked before the undo entry is pushed: a press that draws no border must
	// not leave a step in the history that undoes nothing
	if ed.splitIdx(t0) < 0 && ed.splitIdx(t1) < 0 {
		a.setStatus(fmt.Sprintf("nothing to split at %s – %s", mmss(t0), mmss(t1)))
		return
	}
	before := len(ed.segs)
	ed.pushUndo()
	// the LATER border first: splitting at t0 renumbers everything after it,
	// and a search for t1 that runs before any of that happens is one fewer
	// thing to be right about.
	//
	// The borders that were actually drawn are what the status names, and a
	// selection with one end already on a border draws one: "split at 1:20 and
	// 1:45" over a single cut is a sentence the page can be caught out on.
	var at []string
	if ed.splitBorder(t1) {
		at = append(at, mmss(t1))
	}
	if ed.splitBorder(t0) {
		at = append([]string{mmss(t0)}, at...)
	}
	ed.persist()
	ed.redrawTracks()
	// the selection stays up. It is exactly the scene that was just made, and
	// the reason for making one is nearly always the next press -- ⧉ Copy, a
	// camera, a lane switched off -- which wants that span still in hand.
	a.setStatus(fmt.Sprintf("split at %s — %d scenes, was %d", strings.Join(at, " and "), len(ed.segs), before))
	ed.syncSelBtns()
}

// splitAtLine is | Split with nothing selected: one border, where the red line
// is. The selection's version draws two because a region has two ends; this
// draws the one the line names, which is how a razor works everywhere else and
// what a hand that has just put the line somewhere is asking for.
func (a *App) splitAtLine() {
	ed := a.ed
	if !ed.hasPlay {
		a.setStatus("| Split cuts at the red line — click a track to put it somewhere")
		return
	}
	t := ed.playhead
	if ed.splitIdx(t) < 0 {
		a.setStatus(fmt.Sprintf("nothing to split at %s", mmss(t)))
		return
	}
	before := len(ed.segs)
	ed.pushUndo()
	ed.splitBorder(t)
	ed.persist()
	ed.redrawTracks()
	// the right-hand half in hand, outlined: two scenes that touch are one
	// stretch of green with one more line drawn on it, and the white outline
	// is what says a press just made a scene rather than doing nothing
	if i := ed.segAt(t); i >= 0 {
		ed.segOn, ed.segSel, ed.segDirty = true, i, false
		ed.edgeOn, ed.fxOn = false, false
	}
	a.setStatus(fmt.Sprintf("split at %s — %d scenes, was %d", mmss(t), len(ed.segs), before))
	ed.syncSelBtns()
}

// splitBorder cuts the footage at t in two and says whether it did. Only
// inside a clip and only where both halves clear minSegLn; inserts are passed
// over. The right-hand half carries the flag: it is the clip whose START is
// the new border (coalesce).
func (ed *cutEditor) splitBorder(t float64) bool {
	i := ed.splitIdx(t)
	if i < 0 {
		return false
	}
	right := ed.segs[i] // every other field goes to both halves: same camera,
	right.S = t         // same silenced lanes -- it is the same footage
	right.Split = true
	ed.segs[i].E = t
	ed.segs = append(ed.segs, cutSeg{})
	copy(ed.segs[i+2:], ed.segs[i+1:]) // memmove: the tail shifts right by one
	ed.segs[i+1] = right
	// the halves are new items in the list, and whatever was held is an index
	// into the old one
	ed.dropSeg()
	ed.dropEdge()
	return true
}

// splitIdx is the clip a border at t would cut, or -1: the question splitBorder
// acts on and splitSelRange asks first, so the refusal and the edit cannot
// drift apart.
func (ed *cutEditor) splitIdx(t float64) int {
	for i := range ed.segs {
		s := ed.segs[i]
		if !s.isInsert() && t > s.S+minSegLn && t < s.E-minSegLn {
			return i
		}
	}
	return -1
}

// mergeDropped joins the dropped clip to the neighbour it was dragged against
// and reports whether it did. It is | Split's inverse and the only thing that
// clears a deliberate border -- on this pair only. Same-camera, touching
// neighbours only (coalesce's rule).
func (ed *cutEditor) mergeDropped() bool {
	if !ed.segOn {
		return false
	}
	return ed.mergeTouching(ed.segSel)
}

// mergeTouching is the same join asked about a clip by index: dragging a
// clip's BORDER out to meet the next clip is the commoner way to close a gap,
// and both drops end here.
func (ed *cutEditor) mergeTouching(held int) bool {
	if held < 0 || held >= len(ed.segs) {
		return false
	}
	s := &ed.segs[held]
	if s.isInsert() {
		return false
	}
	for i := range ed.segs {
		n := &ed.segs[i]
		if i == held || n.isInsert() || n.Cam != s.Cam {
			continue
		}
		switch {
		case math.Abs(s.S-n.E) <= mergeTol:
			s.Split = false // this drag is the join, whatever put the border there
		case math.Abs(n.S-s.E) <= mergeTol:
			n.Split = false
		default:
			continue
		}
		a, b := math.Min(s.S, n.S), math.Max(s.E, n.E)
		ed.coalesce() // which is where two touching clips of one camera become one
		ed.a.setStatus(fmt.Sprintf("joined into one scene, %s – %s (%s) — "+
			"↶ Undo puts the border back", mmss(a), mmss(b), ed.spanSecs(a, b)))
		ed.redrawTracks()
		return true
	}
	return false
}

// mergeTol is how close two ends have to be to count as touching. coalesce's
// own number, named here because a drag has to ask the question before the
// merge rather than discover the answer afterwards: clampSeg snaps a dragged
// clip against its neighbour, so the everyday case is exactly 0 apart, and the
// tolerance is for the frame or two a hand-placed one lands out by.
const mergeTol = 0.25
