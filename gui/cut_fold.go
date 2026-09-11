package main

import (
	"fmt"
	"math"

	"github.com/diamondburned/gotk4/pkg/cairo"
)

// Folding dropped stretches, IDE-style: a folded gap is drawn with no width
// (foldPx), so its clips meet border to border; everything follows because
// xOf/tAt walk the cells (layoutPx). A fold is a view, not an edit -- no undo
// step -- but is kept with the cut (cutFile.Folds).
//
// Drags open it: a press that can MOVE something opens the folds around it
// while the button is down (foldOpen/foldShut), so the drag runs on the
// ordinary timeline and the merge at its end is the usual one. The view is
// anchored on the pressed second across both, as the wheel zoom anchors on the
// cursor.

const (
	// foldPx: a folded stretch has NO width; the two clips meet border to
	// border. A press on the seam opens it (foldOpen); which border it means is
	// the side it lands on (bandClipPartAt).
	foldPx = 0.0
	// a gap with no room to draw the − in is not offered one: the badge would
	// be wider than the thing it folds, and would sit on both its neighbours.
	foldMin = 2 * segKillHit
)

// foldGap is one dropped stretch and whether it is folded: what the − and +
// are drawn on, and what a press on one toggles.
type foldGap struct {
	t0, t1 float64
	on     bool
}

// foldGaps is every stretch the cut drops, in order, each saying whether it is
// folded. The head of a run before its first clip and the tail after its last
// are dropped stretches like any other and fold like any other.
func (ed *cutEditor) foldGaps() []foldGap {
	var out []foldGap
	for _, g := range ed.droppedSpans() {
		out = append(out, foldGap{g[0], g[1], ed.foldedGap(g[0], g[1])})
	}
	return out
}

// foldedGap is whether a stretch is folded: a stored fold that OVERLAPS it,
// rather than one that matches its ends.
//
// Trimming a clip moves the edge of the gap beside it, splitting one moves a
// gap's whole middle, and a fold matched by its ends would be lost to both. It
// is the gap between these two clips that is folded, and overlap is how that
// survives the cut being worked on.
func (ed *cutEditor) foldedGap(t0, t1 float64) bool {
	for _, f := range ed.folds {
		if f[1] > t0 && f[0] < t1 {
			return true
		}
	}
	return false
}

// syncFolds rewrites the stored list to the gaps as they now are: one entry
// per folded gap, at that gap's own bounds, and nothing for a gap that no
// longer exists. Called wherever the cut changes shape, so what is stored
// cannot drift away from what is drawn.
func (ed *cutEditor) syncFolds() {
	if len(ed.folds) == 0 {
		return
	}
	var out [][2]float64
	for _, g := range ed.foldGaps() {
		if g.on && !ed.wholeRun(g.t0, g.t1) {
			out = append(out, [2]float64{g.t0, g.t1})
		}
	}
	ed.folds = out
}

// wholeRun is whether a gap has swallowed an entire recording: nothing kept
// before or after it. A fold is remembered by overlap, which would otherwise
// carry it through the cut being emptied and collapse the whole page to one
// strip. A fold is a gap BETWEEN scenes; with none either side it is not one.
func (ed *cutEditor) wholeRun(t0, t1 float64) bool {
	for _, r := range ed.runs() {
		if t0 <= r.t0+0.01 && t1 >= r.t1-0.01 {
			return true
		}
	}
	return false
}

// cells is the filmed runs cut into the pieces the page lays out: a run with a
// folded gap in it becomes the stretch before, the fold, and the stretch
// after. Only folds are cut on -- an unfolded gap is drawn to scale like the
// footage around it, and cutting there would be a boundary nothing needs.
func (ed *cutEditor) cells() []tlSpan {
	var out []tlSpan
	for _, run := range ed.runs() {
		at := run.t0
		for _, g := range ed.foldGaps() {
			if !g.on || g.t1 <= at || g.t0 >= run.t1 {
				continue
			}
			t0, t1 := math.Max(g.t0, at), math.Min(g.t1, run.t1)
			if t0 > at {
				out = append(out, tlSpan{t0: at, t1: t0})
			}
			out = append(out, tlSpan{t0: t0, t1: t1, fold: true})
			at = t1
		}
		// at == run.t0 is "nothing folded here": the run is one cell, and it is
		// emitted even if it has no length at all
		if at < run.t1 || at == run.t0 {
			out = append(out, tlSpan{t0: at, t1: run.t1})
		}
	}
	return out
}

// spanW is how wide a cell is drawn: its seconds at the current zoom, or the
// one fixed width when it is folded.
func (ed *cutEditor) spanW(s tlSpan) float64 {
	if s.fold {
		return foldPx
	}
	return s.dur() * ed.pps
}

// foldAt is the folded cell covering a session second, or nil.
func (ed *cutEditor) foldAt(t float64) *tlSpan {
	for i := range ed.spans {
		if s := &ed.spans[i]; s.fold && t >= s.t0 && t < s.t1 {
			return s
		}
	}
	return nil
}

// cellsOf is the cells a stretch of session time is drawn across: one for a
// stretch with no fold in it, and the pieces either side of every fold that
// cuts through it. What is drawn IN each is the caller's own business -- the
// pictures skip the folded ones, the waveforms skip them, and both walk the
// rest at that cell's own origin, because px is only linear in time inside a
// cell (xOf).
func (ed *cutEditor) cellsOf(t0, t1 float64) []tlSpan {
	var out []tlSpan
	for _, s := range ed.spans {
		if s.fold || s.t1 <= t0 || s.t0 >= t1 {
			continue
		}
		out = append(out, s)
	}
	return out
}

// ---- the − and the + ---------------------------------------------------------

// foldBadge is one gap's control: where it is drawn, and which way it goes.
// The + of a folded gap sits on the seam itself -- the two bars either side
// meet at one x, and that x is the only mark on the page saying the footage
// between them is still there; the − of an unfolded one sits in the middle of
// the gap, where there is nothing else to press.
type foldBadge struct {
	gap    foldGap
	cx, cy float64 // timeline x, and the middle of the selection band
}

// foldBadges is one per gap worth offering: a gap with no room for the badge
// itself gets none (foldMin).
func (ed *cutEditor) foldBadges() []foldBadge {
	var out []foldBadge
	cy := ed.selBandTop() + selBandH/2
	for _, g := range ed.foldGaps() {
		x0, x1 := ed.xOf(g.t0), ed.xOf(g.t1)
		if !g.on && x1-x0 < foldMin {
			continue
		}
		out = append(out, foldBadge{g, ed.foldBadgeX(g, x0, x1), cy})
	}
	return out
}

// foldBadgeX: the badge sits in the middle of a gap between two clips (the ends
// are grips). The head and tail gaps are not between anything, so their badge
// goes just INSIDE the first/last clip, killIn from the border -- never past
// that clip's middle, which is its ✕.
func (ed *cutEditor) foldBadgeX(g foldGap, x0, x1 float64) float64 {
	mid := (x0 + x1) / 2
	head, tail := ed.headTail(g)
	switch {
	case head:
		mid = ed.insideBar(g.t1, true)
	case tail:
		mid = ed.insideBar(g.t0, false)
	}
	const plate = segKillR + segKillPad
	return math.Max(plate, math.Min(mid, ed.totalW-plate))
}

// insideBar is a point killIn inside the kept clip that starts (or ends) at
// second t, and no further in than that clip's own middle. With no clip there
// -- an empty cut is one gap and no bars -- it is the border itself.
func (ed *cutEditor) insideBar(t float64, start bool) float64 {
	for _, s := range ed.segs {
		if s.isInsert() {
			continue
		}
		if start && math.Abs(s.S-t) < 0.01 {
			x0, x1 := ed.xOf(s.S), ed.xOf(s.E)
			return math.Min(x0+killIn, (x0+x1)/2)
		}
		if !start && math.Abs(s.E-t) < 0.01 {
			x0, x1 := ed.xOf(s.S), ed.xOf(s.E)
			return math.Max(x1-killIn, (x0+x1)/2)
		}
	}
	return ed.xOf(t)
}

// headTail says whether a gap opens a filmed run or closes one: the stretch
// before that recording's first kept clip, and the one after its last.
func (ed *cutEditor) headTail(g foldGap) (head, tail bool) {
	for _, r := range ed.runs() {
		if math.Abs(g.t0-r.t0) < 0.01 {
			head = true
		}
		if math.Abs(g.t1-r.t1) < 0.01 {
			tail = true
		}
	}
	return head, tail
}

// foldBadgeAt is the gap whose badge is under a press at timeline-x px, or -1
// into foldBadges.
func (ed *cutEditor) foldBadgeAt(px, y float64) int {
	for i, b := range ed.foldBadges() {
		if math.Abs(px-b.cx) <= segKillHit && math.Abs(y-b.cy) <= segKillHit {
			return i
		}
	}
	return -1
}

// toggleFold folds a gap or opens it again, keeping the second under the
// pointer under the pointer: everything right of the gap moves, so anchor
// (foldAnchor) is the timeline x the press landed on. No undo step.
func (ed *cutEditor) toggleFold(i int, anchor float64) {
	badges := ed.foldBadges()
	if i < 0 || i >= len(badges) {
		return
	}
	g := badges[i].gap
	t := ed.tAt(anchor)
	if g.on {
		ed.unfold(g.t0, g.t1)
	} else {
		ed.folds = append(ed.folds, [2]float64{g.t0, g.t1})
	}
	ed.foldLayout(t, anchor)
	ed.persist()
	if g.on {
		ed.a.setStatus(fmt.Sprintf("unfolded %s – %s (%s)", mmss(g.t0), mmss(g.t1), ed.spanSecs(g.t0, g.t1)))
		return
	}
	ed.a.setStatus(fmt.Sprintf("folded %s – %s (%s)", mmss(g.t0), mmss(g.t1), ed.spanSecs(g.t0, g.t1)))
}

// unfold drops every stored fold overlapping a stretch.
func (ed *cutEditor) unfold(t0, t1 float64) {
	var out [][2]float64
	for _, f := range ed.folds {
		if f[1] > t0 && f[0] < t1 {
			continue
		}
		out = append(out, f)
	}
	ed.folds = out
}

// foldLayout re-lays the timeline out and puts session second t back at
// timeline x, which is the same trick the wheel zooms about the cursor with
// (zoomAt). Everything right of a fold moves when it opens or closes; this is
// what keeps the thing being pressed from moving with it.
func (ed *cutEditor) foldLayout(t, x float64) {
	ed.layoutPx()
	ed.syncScroll()
	ed.setOff(ed.xOf(t) - (x - ed.viewX))
	ed.redrawTracks()
}

// drawFoldBadges paints the − and the + on the selection band, over the bars.
func (ed *cutEditor) drawFoldBadges(cr *cairo.Context, vx0, vx1 float64) {
	for i, b := range ed.foldBadges() {
		if b.cx < vx0-segKillHit || b.cx > vx1+segKillHit {
			continue
		}
		mark := "−"
		if b.gap.on {
			mark = "+"
		}
		foldPlate(cr, b.cx, b.cy, mark, ed.foldHov == i)
	}
}

// foldPlate is the badge itself: the plate every control on this page wears
// (drawKillBadge), with a bar or a cross on it rather than the ✕. Lit under
// the pointer like the rest of them, and not red -- folding takes nothing
// away.
func foldPlate(cr *cairo.Context, cx, cy float64, mark string, hot bool) {
	if hot {
		hotPlate(cr, cx, cy, segKillR+segKillPad)
	} else {
		plate(cr, cx, cy, segKillR+segKillPad, 0.06, 0.06, 0.07, 0.55)
	}
	cr.SetSourceRGBA(1, 1, 1, 0.92)
	cr.SetLineWidth(1.6)
	cr.MoveTo(cx-segKillR, cy)
	cr.LineTo(cx+segKillR, cy)
	if mark == "+" {
		cr.MoveTo(cx, cy-segKillR)
		cr.LineTo(cx, cy+segKillR)
	}
	cr.Stroke()
}

// hoverFold lights the badge under the pointer, the way every other badge on
// the page lights.
func (ed *cutEditor) hoverFold(x, y float64) {
	i := -1
	if x >= 0 && ed.hitSelBand(y) {
		i = ed.foldBadgeAt(x+ed.viewX, y)
	}
	if i != ed.foldHov {
		ed.foldHov = i
		if ed.srcArea != nil {
			ed.srcArea.QueueDraw()
		}
	}
}

// ---- what opens a fold besides its own + -------------------------------------

// walkFold opens the fold the line has walked into: ▶ plays the footage,
// dropped stretches included, and a seam shown while the line is inside it
// lies about where the line is. ▶✂ never enters one. It stays open afterwards
// -- refolding under a running line would pull the page sideways.
func (ed *cutEditor) walkFold() {
	s := ed.foldAt(ed.playhead)
	if s == nil {
		return
	}
	t0, t1 := s.t0, s.t1
	x := ed.xOf(ed.playhead)
	ed.unfold(t0, t1)
	ed.foldLayout(ed.playhead, x)
	ed.persist()
	ed.a.setStatus(fmt.Sprintf("unfolded %s — ▶ ran into it", mmss(t0)))
}

// foldOpen opens the folds a press could drag something into or out of -- the
// one under it and either side of what it grabbed -- for as long as the button
// is down, and returns what foldShut needs to put them back.
func (ed *cutEditor) foldOpen(a, b, px float64) []foldGap {
	reach := foldReach / math.Max(ed.pps, 0.001)
	var shut []foldGap
	for _, g := range ed.foldGaps() {
		if g.on && g.t1 >= a-reach && g.t0 <= b+reach {
			shut = append(shut, g)
		}
	}
	if len(shut) == 0 {
		return nil
	}
	t := ed.tAt(px)
	for _, g := range shut {
		ed.unfold(g.t0, g.t1)
	}
	ed.foldLayout(t, px)
	return shut
}

// foldReach is how far past what is held a fold still counts as reachable, in
// px: the grab itself, so the seam AT the end of a clip being dragged opens
// with it and the next one along does not.
const foldReach = edgeGrab

// foldShut folds back what foldOpen opened, once the button is up, anchored on
// the second under the pointer so the page does not jump out from under the
// hand that just let go.
//
// A gap the drag CLOSED -- a clip grown across it, two clips merged -- is gone,
// and folding it back would fold whatever gap now overlaps where it was. So
// each one is only put back if it is still a dropped stretch of its own.
func (ed *cutEditor) foldShut(shut []foldGap, px float64) {
	if len(shut) == 0 {
		return
	}
	t := ed.tAt(px)
	gaps := ed.foldGaps()
	for _, g := range shut {
		for _, now := range gaps {
			if now.t0 < g.t1 && now.t1 > g.t0 && now.t1-now.t0 > 0.05 {
				ed.folds = append(ed.folds, [2]float64{now.t0, now.t1})
				break
			}
		}
	}
	ed.syncFolds()
	ed.foldLayout(t, px)
	ed.persist()
}

// The gutter: the timeline starts gutterPx in, and the black strip before it
// holds the permanent controls (whole-lane sound switches, row ✕, fold-all) so
// they never sit on footage. It is content, not a pinned column: the first
// gutterPx of timeline coordinate space (layoutPx), so controls are placed and
// pressed in timeline px and scroll away with the tape.

const (
	// gutterPx is how much black stands in front of second zero, and
	// gutterMid is where a control in it is centred. Wide enough for the
	// plates the switches wear (hearPlate, foldPlate) with a px or two either
	// side, and no wider: it is space taken off the width the footage has.
	gutterPx  = 30.0
	gutterMid = gutterPx / 2
)

// drawGutter fills the strip. Black rather than the band's own dark grey: the
// bands say "footage" and this is the one part of the page that is not, and at
// the zoom floor -- where the whole session is on screen and the strip is the
// only thing to the left of it -- a shade of the same grey would read as more
// timeline with nothing on it.
func (ed *cutEditor) drawGutter(cr *cairo.Context, top, h float64) {
	cr.SetSourceRGB(0, 0, 0)
	cr.Rectangle(0, top, gutterPx, h)
	cr.Fill()
}

// ---- fold the lot -----------------------------------------------------------

// foldAllY is where the fold-all control sits: the selection band's own line,
// under the clock, because folding is about the gaps between the green bars
// that band draws.
func (ed *cutEditor) foldAllY() float64 { return ed.selBandTop() + selBandH/2 }

// foldAllOn is what pressing it would do: with anything folded it opens
// everything, and only with nothing folded does it fold. One press to get the
// page back is worth more than one press to fold the last gap -- and a control
// that reads "+" while half the page is folded and half is not would be lying
// about the half it is not.
func (ed *cutEditor) foldAllOn() bool { return len(ed.folds) > 0 }

// foldAllAt is whether a press in the gutter is on it.
func (ed *cutEditor) foldAllAt(px, y float64) bool {
	return len(ed.foldGaps()) > 0 &&
		math.Abs(px-gutterMid) <= segKillHit && math.Abs(y-ed.foldAllY()) <= segKillHit
}

// toggleFoldAll folds every dropped stretch away, or brings them all back.
//
// The view is anchored the way one gap's own badge anchors it (toggleFold):
// the second at the left of the page stays at the left of the page, so folding
// the lot pulls the timeline in around what you were looking at rather than
// throwing it somewhere else entirely.
func (ed *cutEditor) toggleFoldAll() {
	gaps := ed.foldGaps()
	if len(gaps) == 0 {
		return
	}
	t := ed.tAt(ed.viewX + gutterPx)
	on := ed.foldAllOn()
	n := 0
	if on {
		ed.folds = nil
		n = len(gaps)
	} else {
		for _, g := range gaps {
			ed.folds = append(ed.folds, [2]float64{g.t0, g.t1})
			n++
		}
		ed.syncFolds() // a gap that is a whole recording is not one to fold (wholeRun)
		n = len(ed.folds)
	}
	ed.foldLayout(t, ed.xOf(t))
	ed.persist()
	if on {
		ed.a.setStatus("unfolded " + plural(n, "seam"))
		return
	}
	ed.a.setStatus("folded " + plural(n, "seam"))
}

// drawFoldAll paints it: the same − and + a single gap's badge wears, because
// it is the same verb over all of them.
func (ed *cutEditor) drawFoldAll(cr *cairo.Context) {
	if len(ed.foldGaps()) == 0 {
		return
	}
	mark := "−"
	if ed.foldAllOn() {
		mark = "+"
	}
	foldPlate(cr, gutterMid, ed.foldAllY(), mark, ed.foldAllHov)
}

// gutterCtl is whether a press landed on a control in the strip: the fold-all
// badge, a lane's sound switch, a row strip's switch, an emptied row's ✕.
// Every second in the gutter is second zero, so a press on a control must not
// also cue the line there; only the black between them is a place for the line.
func (ed *cutEditor) gutterCtl(px, y float64) bool {
	if px > gutterPx {
		return false
	}
	return ed.foldAllAt(px, y) || ed.laneSwitchAt(px, y) != "" ||
		ed.pairSwitchAt(px, y) != nil || ed.rowKillAt(px, y) >= 0
}

// hoverFoldAll lights it under the pointer, like every other badge here.
func (ed *cutEditor) hoverFoldAll(x, y float64) {
	on := x >= 0 && ed.foldAllAt(x+ed.viewX, y)
	if on != ed.foldAllHov {
		ed.foldAllHov = on
		if ed.srcArea != nil {
			ed.srcArea.QueueDraw()
		}
	}
}
