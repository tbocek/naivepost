package main

// The selection band: the row between the ruler and the thumbnails where the
// selection is an object -- press to pick up, drag the middle to slide, drag an
// end to move it, ✕ to throw away; ends snap to cut borders, seams and the
// playhead. The tint over the pictures still says WHICH FOOTAGE; the band says
// where it begins and ends and offers the handles.

import (
	"fmt"
	"math"

	"github.com/diamondburned/gotk4/pkg/cairo"
	"github.com/diamondburned/gotk4/pkg/gtk/v4"
)

// what a press on the band lands on.
const (
	selNone  = iota // clear of it: a press here draws a new one
	selWhole        // the middle: the whole band travels
	selStart        // the left end
	selEnd          // the right end
	selKill         // the ✕
)

const (
	selGripPx = 6.0 // px either side of an end that grabs that end
	// ...and on the pictures, where the band's grips are not drawn and the
	// eye aims at a line: wider, since there is nothing to see the six px by
	selGripPicsPx = 10.0
	selKillW      = 12.0 // the blue band's ✕ target, narrower than a plated one's
	// ...and centred at killIn from the right end like every other ✕. Two flat
	// strokes, no plate: it throws away a SELECTION, the plated ones remove
	// footage. Under selMinBand the band is all grips and the ✕ is not drawn.
	selMinBand = 2*(killIn+selKillW/2) + 6
	// the GREEN bar's ✕ is the plated badge the lanes and the effects wear
	// (drawKillBadge), because it is the same verb they answer -- and it sits
	// in the MIDDLE of its bar rather than against an end. Both ends of a
	// green bar are grips, and the seam where two of them meet is the fold's
	// + (cut_fold.go); the middle is what is left, and it is the same place on
	// an effect's band (fxKillCentre). The blue's stays at its end: it has one
	// grip per end and no seam, and its middle is the drag that moves it.
)

// bandGround is the shade the two bands that speak for the whole cut are drawn
// on: this row, and the effects lane under it. One ground for the pair, one
// step up from the page, so the group reads as a group -- above it the clock,
// below it a hairline and then the recordings (fxLaneTop).
const bandGround = 0.16

// selBandTop is the band's y inside the source-track area: directly under the
// ruler's clock.
func (ed *cutEditor) selBandTop() float64 { return float64(rulerH) }

// hitSelBand is whether a press in the source-track area lands in the band.
func (ed *cutEditor) hitSelBand(y float64) bool {
	return y >= ed.selBandTop() && y < ed.fxLaneTop()
}

// selSpan is the selection in session time, low end first. The stored pair is
// in the order it was dragged in -- t1 is where the hand is -- and everything
// outside the dragging wants it sorted.
func (ed *cutEditor) selSpan() (float64, float64) {
	a, b := ed.sel.t0, ed.sel.t1
	if b < a {
		a, b = b, a
	}
	return a, b
}

// selSpanPx is the band as drawn and grabbed.
func (ed *cutEditor) selSpanPx() (float64, float64) {
	a, b := ed.selSpan()
	return ed.xOf(a), ed.xOf(b)
}

// selPartAt is what a press at timeline-x px takes hold of.
//
// The ends are asked about before the middle, so a band narrow enough that its
// grips meet is two ends and no middle rather than a middle you cannot get out
// of. The ✕ sits inboard of the right grip rather than under it -- they would
// otherwise be the same twelve pixels, and "throw it away" is not something a
// hand aiming at "make it a bit shorter" should be able to hit by accident.
func (ed *cutEditor) selPartAt(px float64) int { return ed.selPartNear(px, selGripPx) }

// selPartNear is selPartAt with the ends' reach given: the band's own grips
// are selGripPx; on the pictures the reach is wider (selGripPicsPx).
func (ed *cutEditor) selPartNear(px, grip float64) int {
	if !ed.sel.active {
		return selNone
	}
	x0, x1 := ed.selSpanPx()
	switch {
	case math.Abs(px-x0) <= grip:
		return selStart
	case math.Abs(px-x1) <= grip:
		return selEnd
	case x1-x0 >= selMinBand && math.Abs(px-(x1-killIn)) <= selKillW/2:
		return selKill
	case px > x0 && px < x1:
		return selWhole
	}
	return selNone
}

// ---- the green bar ----------------------------------------------------------

// bandClipIdx is the clip the green bar stands for, or -1 when there is none.
// The clip in hand comes first -- held whole or by an edge, from this bar or
// from the picture band, the bar stays on it, so trimming a clip's end does not
// make the bar vanish the moment the playhead lands on the border being
// dragged -- and otherwise it is the kept clip the playhead sits in, which is
// what the bar has always shown.
func (ed *cutEditor) bandClipIdx() int {
	if ed.segOn && ed.segSel < len(ed.segs) && !ed.segs[ed.segSel].isInsert() {
		return ed.segSel
	}
	if ed.edgeOn && ed.edgeSeg < len(ed.segs) {
		return ed.edgeSeg
	}
	if !ed.hasPlay {
		return -1
	}
	for i, s := range ed.segs {
		if !s.isInsert() && ed.playhead >= s.S && ed.playhead < s.E {
			return i
		}
	}
	return -1
}

// bandBars is every scene the band draws a green bar for: all of the kept ones,
// always, in the cut's own order. Inserts are not among them for the same reason
// they are not bandClipIdx's answer -- a card is not a stretch of the recording,
// and the bar is about where the recording is kept.
func (ed *cutEditor) bandBars() []int {
	var out []int
	for i := range ed.segs {
		if !ed.segs[i].isInsert() {
			out = append(out, i)
		}
	}
	return out
}

// bandKillAt is the kept clip whose ✕ is under timeline-x px, or -1. EVERY
// bar's ✕ is live, not just the held bar's -- like the effects lane. Trimming
// and moving stay the tall bar's alone, since a drag needs an anchor; removing
// does not. Target: segKillHit either side of the plate, on bars with room
// (killMin).
func (ed *cutEditor) bandKillAt(px float64) int {
	for _, i := range ed.bandBars() {
		s := ed.segs[i]
		x0, x1 := ed.xOf(s.S), ed.xOf(s.E)
		if x1-x0 >= killMin && math.Abs(px-(x0+x1)/2) <= segKillHit {
			return i
		}
	}
	return -1
}

// bandClipPartAt is what a press at timeline-x px takes hold of on the green
// bar: which clip and which part. Same order as the blue bar -- ends first
// (over ALL bars, the bar in reach first, since touching scenes share a border
// to the pixel), then the ✕, then the middles -- so the outer px stay the grip
// that trims and "a bit shorter" cannot find "gone". EVERY bar answers, not
// just the one under the line. The blue bar is asked before this everywhere.
func (ed *cutEditor) bandClipPartAt(px float64) (int, int) {
	bars := ed.bandBars()
	if cur := ed.bandClipIdx(); cur >= 0 {
		bars = append([]int{cur}, bars...)
	}
	// the SIDE first, then the order. Folded, one bar's end and the next
	// one's start are the same x (cut_fold.go), and the only thing left to
	// tell them apart is which side of the seam the press landed on -- the
	// press is in one bar or the other, and that bar's border is the one it
	// means. Everywhere else there is one border in reach and it answers from
	// either side of itself, which is what the second pass is.
	for _, inside := range []bool{true, false} {
		for _, i := range bars {
			x0, x1 := ed.xOf(ed.segs[i].S), ed.xOf(ed.segs[i].E)
			switch {
			case math.Abs(px-x0) <= selGripPx && (!inside || px >= x0):
				return i, selStart
			case math.Abs(px-x1) <= selGripPx && (!inside || px <= x1):
				return i, selEnd
			}
		}
	}
	if k := ed.bandKillAt(px); k >= 0 {
		return k, selKill
	}
	for _, i := range bars {
		if x0, x1 := ed.xOf(ed.segs[i].S), ed.xOf(ed.segs[i].E); px > x0 && px < x1 {
			return i, selWhole
		}
	}
	return -1, selNone
}

// holdBandClip takes the green bar in hand. The bar IS its clip, so holding it
// is holding that: an end press picks up that clip border exactly as grabEdge
// does, a middle press the whole clip exactly as grabSeg does, and every verb
// downstream -- the drag's clamps and snaps, the throttled preview, one undo
// per drag, ‹f f› nudging afterwards -- is the picture band's own machinery
// rather than a second copy of it.
func (ed *cutEditor) holdBandClip(i, part int) {
	ed.dropSel() // one thing is held at a time, and this is now it
	ed.dropFx()
	if part == selWhole {
		ed.edgeOn = false
		ed.segOn, ed.segSel, ed.segDirty = true, i, false
	} else {
		ed.segOn = false
		ed.edgeOn, ed.edgeSeg, ed.edgeEnd, ed.edgeDirty = true, i, part == selEnd, false
	}
	ed.syncInsertBtn()
	ed.redrawTracks()
}

// ---- moving it --------------------------------------------------------------

// snapMarks are the landmarks a dragged selection lands on: every border of
// the cut (what a selection is nearly always aimed at), every seam between
// recordings (first and last frames nobody finds by hand), the ends of every
// effect's band, and the playhead.
func (ed *cutEditor) snapMarks() []float64 {
	out := make([]float64, 0, 2*len(ed.segs)+2*len(ed.vids)+2*len(ed.fx)+1)
	for _, s := range ed.segs {
		out = append(out, s.S, s.E)
	}
	for _, v := range ed.vids {
		out = append(out, v.start, v.start+v.dur)
	}
	for _, f := range ed.fx {
		t0, t1 := f.fxSpan()
		out = append(out, t0)
		if t1 > t0 {
			out = append(out, t1)
		}
	}
	if ed.hasPlay {
		out = append(out, ed.playhead)
	}
	return out
}

// snapSel pulls a single dragged end onto the nearest landmark within a few
// pixels' reach. Pixels rather than seconds, so the pull is the same distance
// for the hand at every zoom.
func (ed *cutEditor) snapSel(t float64) float64 {
	tol := snapPx / math.Max(ed.pps, 0.001)
	best, bd := t, tol
	for _, m := range ed.snapMarks() {
		if d := math.Abs(t - m); d < bd {
			best, bd = m, d
		}
	}
	return best
}

// snapSelSpan is snapSel for a band being slid along whole: BOTH ends are
// offered to the landmarks and the closer fit wins.
//
// Snapping only the leading end would mean a selection can be put flush against
// the cut on its left and never on its right, which is half a feature -- and
// the half you want is usually the other one, because a selection is dragged
// leftward as often as rightward.
func (ed *cutEditor) snapSelSpan(t0, ln float64) float64 {
	tol := snapPx / math.Max(ed.pps, 0.001)
	best, bd := t0, tol
	for _, m := range ed.snapMarks() {
		if d := math.Abs(t0 - m); d < bd {
			best, bd = m, d
		}
		if d := math.Abs(t0 + ln - m); d < bd {
			best, bd = m-ln, d
		}
	}
	return best
}

// selMinLen is the shortest a band may be dragged down to. Not zero: a
// selection with no length is invisible, cannot be grabbed again, and every
// action taken on it does nothing, so a hand that overshoots would silently
// destroy the thing it was adjusting.
const selMinLen = 0.04

// moveSelTo slides the whole band so it starts at t.
func (ed *cutEditor) moveSelTo(t float64) {
	a, b := ed.selSpan()
	ln := b - a
	t = math.Max(0, ed.snapSelSpan(t, ln))
	ed.sel.t0, ed.sel.t1 = t, t+ln
	ed.syncSelMarks()
}

// resizeSelTo moves one end of the band to t and leaves the other where it is.
func (ed *cutEditor) resizeSelTo(end bool, t float64) {
	a, b := ed.selSpan()
	t = math.Max(0, ed.snapSel(t))
	if end {
		b = math.Max(t, a+selMinLen)
	} else {
		a = math.Min(t, b-selMinLen)
	}
	ed.sel.t0, ed.sel.t1 = a, b
	ed.syncSelMarks()
}

// syncSelMarks keeps the selection readout telling the truth.
//
// The band and the marks are one object seen twice: the marks are how the rest
// of the page reads the band (the readout under Add, the flags on the picture
// band), so a band that could be dragged away from them would leave the clock
// describing a selection that is no longer anywhere. Every path that changes
// the band comes through here -- dragging a new one included.
func (ed *cutEditor) syncSelMarks() {
	ed.markIn, ed.markOut = ed.selSpan()
	ed.hasIn, ed.hasOut = true, true
	ed.showMarks()
	ed.redrawTracks()
}

// dropSel puts the band down without changing it.
func (ed *cutEditor) dropSel() {
	if !ed.selOn {
		return
	}
	ed.selOn = false
	ed.redrawTracks()
}

// killSel throws the selection away. The footage is untouched -- this is the ✕
// on the band, not ⌦, and the difference is worth keeping straight: ✕ removes
// the SELECTION, ⌦ removes what the selection is over.
func (ed *cutEditor) killSel() {
	ed.sel.active, ed.selOn = false, false
	ed.clearMarks()
	ed.a.setStatus("selection cleared — drag across a track for a new one")
	ed.redrawTracks()
}

// holdSel takes the band in hand and says what can be done to it.
func (ed *cutEditor) holdSel(part int) {
	ed.dropEdge() // one thing is held at a time, and this is now it
	ed.dropSeg()
	ed.dropFx()
	ed.selOn = true
	a, b := ed.selSpan()
	ed.a.setStatus(fmt.Sprintf("selection %s – %s (%s)", mmss(a), mmss(b), ed.spanSecs(a, b)))
	ed.redrawTracks()
}

// hoverTracks answers the pointer for a whole band -- which effect a press
// would take, which border it would trim, whether it is over the selection
// row, and the cursor -- in one handler, since two would fight over the cursor.
// x below zero means the pointer has left.
func (ed *cutEditor) hoverTracks(x, y float64) {
	ed.hoverFx(x, y)
	ed.hoverFxKill(x, y)
	on := x >= 0 && ed.hitSelBand(y) && ed.selPartAt(x+ed.viewX) != selNone
	// ...and over either END of the selection on the pictures, where the
	// press takes that end too: the band lights the same way, so the hand
	// sees the grip before it reaches
	if x >= 0 && !ed.hitSelBand(y) {
		if p := ed.selPartNear(x+ed.viewX, selGripPicsPx); p == selStart || p == selEnd {
			on = true
		}
	}
	gOn, gKill := false, -1
	if x >= 0 && !on && ed.hitSelBand(y) {
		// the green bar highlights only where the blue does not answer first:
		// same precedence as the press and the cursor
		var part int
		_, part = ed.bandClipPartAt(x + ed.viewX)
		gOn = part != selNone
		// and the ✕ under the pointer lights on its own, the way every other ✕
		// on the page does: the bar's ring says "this clip answers the hand",
		// the red plate says "this press removes it", and they are not the
		// same promise -- so the plate is lit by index, on whichever bar it is
		gKill = ed.bandKillAt(x + ed.viewX)
	}
	if on != ed.selHov || gOn != ed.bandHov || gKill != ed.bandKillHov {
		ed.selHov, ed.bandHov, ed.bandKillHov = on, gOn, gKill
		if ed.srcArea != nil {
			ed.srcArea.QueueDraw()
		}
	}
	ed.hoverFold(x, y)
	ed.hoverFoldAll(x, y)
	ed.hoverLaneKill(x, y)
	ed.hoverBadges(x, y, true)
	ed.hoverEdge(x, x >= 0 && ed.hitPics(y))
	ed.setCursor(ed.srcArea, ed.wantCursor(x, y))
}

// hoverLanes is the same answer for the waveform area, which has one question
// in it: the lanes are this timeline seen as sound, the cut points run through
// them, and there is nothing else down there to take hold of.
func (ed *cutEditor) hoverLanes(x, y float64) {
	ed.hoverBadges(x, y, false)
	ed.hoverEdge(x, x >= 0)
	ed.setCursor(ed.audArea, ed.wantLaneCursor(x, y))
}

// wantLaneCursor is what a press in the recorders' band would do, said in the
// pointer: a button over the switch standing in the gutter, which is the one
// part of this band that is not the tape (cut_fold.go), and the resize arrow
// wherever a border can be trimmed.
func (ed *cutEditor) wantLaneCursor(x, y float64) string {
	if x < 0 {
		return ""
	}
	if ed.laneSwitchAt(x+ed.viewX, y) != "" {
		return "pointer"
	}
	if _, _, ok := ed.edgeAt(x + ed.viewX); ok {
		return "ew-resize"
	}
	return ""
}

// hoverEdge highlights the clip border a press would take hold of. This is the
// half of the gesture that makes the other half safe: trimming and selecting
// are the same press over the same pixels, told apart only by landing within
// edgeGrab of a border, and a hand cannot aim at a tolerance it cannot see. The
// highlight is the tolerance, drawn.
func (ed *cutEditor) hoverEdge(x float64, cut bool) {
	on, t := false, 0.0
	if cut {
		if seg, end, ok := ed.edgeAt(x + ed.viewX); ok {
			on = true
			if t = ed.segs[seg].S; end {
				t = ed.segs[seg].E
			}
		}
	}
	if on == ed.edgeHovOn && t == ed.edgeHovT {
		return // nothing the bands draw has changed
	}
	ed.edgeHovOn, ed.edgeHovT = on, t
	// the two bands that draw the marker, and not redrawTracks: a hover happens
	// on the way past and must not drag the overlay's pointer-grabbing and the
	// track's height along behind it
	if ed.srcArea != nil {
		ed.srcArea.QueueDraw()
	}
	if ed.audArea != nil {
		ed.audArea.QueueDraw()
	}
}

// wantCursor is what a press at this point would do, said in the pointer: the
// three things the selection row does live in the same sixteen pixels, and a
// resize cursor over an end and an open hand over a middle tell them apart
// before anything is committed. The effects lane answers the same way.
func (ed *cutEditor) wantCursor(x, y float64) string {
	if x < 0 {
		return ""
	}
	// the gutter's controls stand in front of second zero, where nothing on
	// the tape reaches (cut_gutter.go)
	if ed.foldAllAt(x+ed.viewX, y) || ed.pairSwitchAt(x+ed.viewX, y) != nil ||
		ed.rowKillAt(x+ed.viewX, y) >= 0 {
		return "pointer"
	}
	switch {
	case ed.hitSelBand(y):
		// the fold's own badge first, and for the reason every badge is asked
		// first: it sits between two bars, where a press would otherwise be
		// read as one of their ends (cut_fold.go)
		if ed.foldBadgeAt(x+ed.viewX, y) >= 0 {
			return "pointer"
		}
		switch ed.selPartAt(x + ed.viewX) {
		case selStart, selEnd:
			return "ew-resize"
		case selWhole:
			return "grab"
		case selKill:
			return "pointer"
		}
		// clear of the blue, the green bar answers with the same cursors: its
		// ends trim the clip's borders, its middle moves the clip, its ✕
		// removes it
		switch _, part := ed.bandClipPartAt(x + ed.viewX); part {
		case selStart, selEnd:
			return "ew-resize"
		case selWhole:
			return "grab"
		case selKill:
			return "pointer"
		}
	case ed.fxHitLane(y):
		// the band's own ✕ first, exactly as the press asks it first
		// (cut_fxkill.go): it sits ON the band, so without this the pointer
		// over a button that removes the effect was the open hand that means
		// "this whole thing moves" -- the one promise it must not make
		if ed.fxKillAt(x+ed.viewX, y) >= 0 {
			return "pointer"
		}
		i := ed.fxIndexAt(x+ed.viewX, y)
		if i < 0 {
			return ""
		}
		if p := ed.fxPartAt(i, x+ed.viewX); p == fxStart || p == fxEnd {
			return "ew-resize"
		}
		return "grab"
	case ed.hitPics(y):
		// a lane's ✕ first, for the reason the press asks it first: it can
		// overlap a clip border, and a resize arrow over a button is a lie
		if ed.laneKillAt(x+ed.viewX, y) != "" {
			return "pointer"
		}
		// ...and an emptied row's, which the press asks in the same breath
		if ed.rowKillAt(x+ed.viewX, y) >= 0 {
			return "pointer"
		}
		if _, _, ok := ed.edgeAt(x + ed.viewX); ok {
			return "ew-resize"
		}
		// ...and the blue selection's own ends, which the press takes from
		// here exactly as the selection row does; the hand has to be able
		// to see that before it reaches
		if p := ed.selPartNear(x+ed.viewX, selGripPicsPx); p == selStart || p == selEnd {
			return "ew-resize"
		}
	}
	return ""
}

// setCursor points a band's pointer at name, or back to the page's own for "".
// One slot per band, remembered, because SetCursorFromName on every motion
// event is a call into the display server for a cursor it already has.
func (ed *cutEditor) setCursor(area *gtk.DrawingArea, name string) {
	if area == nil {
		return
	}
	last := &ed.selCur
	if area == ed.audArea {
		last = &ed.audCur
	}
	if *last == name {
		return
	}
	*last = name
	if name == "" {
		area.SetCursor(nil)
		return
	}
	area.SetCursorFromName(name)
}

// ---- drawing ----------------------------------------------------------------

// drawSelBand paints the band. Called from drawTrack inside its translation, so
// x here is timeline px like everything drawn around it.
func (ed *cutEditor) drawSelBand(cr *cairo.Context, vx0, vx1 float64) {
	y := ed.selBandTop()
	// the row itself, as wide as the recordings, so that an empty band is
	// visibly a place a selection could go rather than a gap in the page
	cr.SetSourceRGB(bandGround, bandGround, bandGround+0.01)
	for _, v := range ed.vids {
		cr.Rectangle(v.pxOrigin, y, v.dur*ed.pps, selBandH)
	}
	cr.Fill()
	// Every kept scene as a bar, always: the green tint over dark footage is
	// invisible, the band's flat ground answers "what does the cut keep" at a
	// glance. The clip under the red line is drawn brighter with end handles (it
	// is what the arrows and toolbar act on); the rest dimmer, a pixel short of
	// their right edge so touching scenes read as two. Drawn first so a selection
	// lands on top.
	cur := ed.bandClipIdx()
	// A pixel short of its own right edge so touching clips read as two, deep
	// enough to hold a badge. Every bar wears END HANDLES, because every bar
	// answers the hand (bandClipPartAt) -- as the picture band draws every kept
	// stretch's borders.
	for _, i := range ed.bandBars() {
		s := ed.segs[i]
		gx0, gx1 := ed.xOf(s.S), ed.xOf(s.E)
		if gx1 < vx0 || gx0 > vx1 {
			continue
		}
		// the bar in reach is a shade stronger and a touch deeper: it is the
		// one the arrow keys and the toolbar are about, and the row still says
		// which that is
		if i == cur {
			cr.SetSourceRGBA(0.2, 0.8, 0.3, 0.5)
			cr.Rectangle(gx0, y+2, gx1-gx0, selBandH-4)
		} else {
			cr.SetSourceRGBA(0.2, 0.8, 0.3, 0.4)
			cr.Rectangle(gx0, y+4, math.Max(1, gx1-gx0-1), selBandH-8)
		}
		cr.Fill()
		cr.SetSourceRGB(0.5, 0.92, 0.58)
		for _, hx := range []float64{gx0, gx1} {
			cr.Rectangle(hx-1.5, y, 3, selBandH)
		}
		cr.Fill()
		// the ✕, the same badge on every bar and red under the pointer,
		// because this is the page's one "remove that" and it says so the
		// same way everywhere (drawKillBadge). Not the blue's flat mark --
		// the blue's ✕ throws away a selection and leaves the footage alone.
		if gx1-gx0 >= killMin {
			drawKillBadge(cr, (gx0+gx1)/2, y+selBandH/2, ed.bandKillHov == i)
		}
		if i != cur {
			continue
		}
		switch {
		case (ed.segOn && ed.segSel == i) || (ed.edgeOn && ed.edgeSeg == i):
			cr.SetSourceRGBA(1, 1, 1, 0.9)
			cr.SetLineWidth(2)
			cr.Rectangle(gx0-1, y+1, gx1-gx0+2, selBandH-2)
			cr.Stroke()
		case ed.bandHov:
			cr.SetSourceRGBA(1, 1, 1, 0.4)
			cr.SetLineWidth(1.5)
			cr.Rectangle(gx0-1, y+1.25, gx1-gx0+2, selBandH-2.5)
			cr.Stroke()
		}
	}
	if !ed.sel.active {
		return
	}
	x0, x1 := ed.selSpanPx()
	if x1 < vx0 || x0 > vx1 {
		return
	}
	a, b := ed.selSpan()

	cr.SetSourceRGBA(0.3, 0.55, 0.9, 0.75)
	cr.Rectangle(x0, y+2, x1-x0, selBandH-4)
	cr.Fill()
	// the ends, drawn as the handles they are: a bar the full height of the
	// row, brighter than the fill, on both sides of the border rather than
	// inside it, so that what is grabbable looks grabbable
	cr.SetSourceRGB(0.62, 0.82, 1)
	for _, x := range []float64{x0, x1} {
		cr.Rectangle(x-1.5, y, 3, selBandH)
	}
	cr.Fill()

	cr.SetFontSize(9)
	if x1-x0 >= selMinBand {
		// the ✕, inboard of the right handle and in the same column as every
		// other one on the page
		kx := x1 - killIn
		cr.SetSourceRGBA(1, 1, 1, 0.85)
		cr.SetLineWidth(1.6)
		for _, d := range [][2]float64{{-1, -1}, {-1, 1}} {
			cr.MoveTo(kx+3*d[0], y+selBandH/2+3*d[1])
			cr.LineTo(kx-3*d[0], y+selBandH/2-3*d[1])
		}
		cr.Stroke()
	}
	// how long it is, which is the number a selection is usually being adjusted
	// towards. Plain white on the blue rather than the plated text the rest of
	// the page uses: a plate is for words that land on thumbnails and have to
	// survive whatever is under them, and putting one here would black out most
	// of the band it is labelling. Drawn only when it fits clear of the ✕ --
	// truncated to "0:20 – 0:5" it would say nothing and cost the band its
	// colour.
	lbl := fmt.Sprintf("%s – %s  %.1fs", mmss(a), mmss(b), b-a)
	if e := cr.TextExtents(lbl); x0+6+e.Width < x1-killIn-selKillW/2-4 {
		cr.SetSourceRGB(1, 1, 1)
		cr.MoveTo(x0+6, y+selBandH-5)
		cr.ShowText(lbl)
	}
	// held is a solid white ring, the same two weights the effects lane uses
	switch {
	case ed.selOn:
		cr.SetSourceRGBA(1, 1, 1, 0.9)
		cr.SetLineWidth(2)
		cr.Rectangle(x0-1, y+1, x1-x0+2, selBandH-2)
		cr.Stroke()
	case ed.selHov:
		cr.SetSourceRGBA(1, 1, 1, 0.4)
		cr.SetLineWidth(1.5)
		cr.Rectangle(x0-1, y+1.25, x1-x0+2, selBandH-2.5)
		cr.Stroke()
	}
}
