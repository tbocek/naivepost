package main

import (
	"math"
	"strings"

	"github.com/diamondburned/gotk4/pkg/cairo"
	"github.com/diamondburned/gotk4/pkg/gdk/v4"
)

// The ✕ on an effect's band: every band wide enough (killMin) carries one and
// pressing it drops THAT effect. It is asked before the band, so the stretch
// under it neither picks the effect up nor takes its end. Narrow bands keep
// in-hand + ⌦.

// fxKillCentre is where effect i's ✕ sits -- timeline x, area y -- and
// whether it has one at all. The width floor is the scene's (killMin), for
// the scene's reason: below it the band's middle is inside the target, and a
// press meant to slide the effect would remove it.
func (ed *cutEditor) fxKillCentre(i int) (float64, float64, bool) {
	if i < 0 || i >= len(ed.fx) {
		return 0, 0, false
	}
	x0, x1 := ed.fxSpanPx(ed.fx[i])
	if x1-x0 < killMin {
		return 0, 0, false
	}
	rows, _ := fxRows(ed.fx)
	// the middle, the green bar's own place for it (cut_selband.go): a band's
	// two ends are its grips and the ✕ is not one of them
	return (x0 + x1) / 2, ed.fxLaneTop() + (float64(rows[i])+0.5)*fxLaneH, true
}

// fxKillAt is the effect whose ✕ is under a press at timeline-x px and
// area-y y, or -1.
func (ed *cutEditor) fxKillAt(px, y float64) int {
	for i := range ed.fx {
		cx, cy, ok := ed.fxKillCentre(i)
		if ok && math.Abs(px-cx) <= segKillHit && math.Abs(y-cy) <= segKillHit {
			return i
		}
	}
	return -1
}

// killFx drops effect i. It is what ⌦ has always done, with the hold taken
// out: the press names the effect, so nothing has to be picked up first --
// and removeHeldFx now comes through here too, so there is one copy of the
// index surgery for both doors.
func (ed *cutEditor) killFx(i int) {
	if i < 0 || i >= len(ed.fx) {
		return
	}
	was := ed.fx[i].fxLabel()
	ed.pushUndo()
	ed.fx = append(ed.fx[:i], ed.fx[i+1:]...)
	// the hold and the hover were indices, and the indices have moved
	switch {
	case ed.fxOn && ed.fxSel == i:
		ed.dropFx()
	case ed.fxOn && ed.fxSel > i:
		ed.fxSel--
	}
	ed.fxHovOn, ed.fxKillHov = false, -1
	ed.persist()
	ed.a.setStatus("removed " + was + " — ↶ Undo takes it back")
	ed.redrawTracks()
}

// hoverFxKill lights the ✕ under the pointer. x below zero means the pointer
// has left the band.
func (ed *cutEditor) hoverFxKill(x, y float64) {
	i := -1
	if x >= 0 && ed.fxHitLane(y) {
		i = ed.fxKillAt(x+ed.viewX, y)
	}
	if i != ed.fxKillHov {
		ed.fxKillHov = i
		if ed.srcArea != nil {
			ed.srcArea.QueueDraw()
		}
	}
}

// drawFxKill paints the badges, the same plate-and-arms every other remove on
// the page wears (drawKillBadge), on top of the bands drawFxLane has already
// drawn. Called from drawTrack inside
// its translation, so x here is timeline px like everything drawn around it.
func (ed *cutEditor) drawFxKill(cr *cairo.Context, vx0, vx1 float64) {
	for i := range ed.fx {
		cx, cy, ok := ed.fxKillCentre(i)
		if !ok || cx < vx0-segKillHit || cx > vx1+segKillHit {
			continue
		}
		drawKillBadge(cr, cx, cy, ed.fxKillHov == i)
	}
}

// fxLabelRoom is how many characters of a band's label fit before its ✕.
//
// The badge is in the middle of the band now, not against its right end, so
// the words have the left half of it less the plate rather than the whole of
// it less a corner. A band too narrow to wear one (killMin) keeps the whole
// width, minus the margin the plate would have wanted.
func fxLabelRoom(x0, x1 float64) int {
	w := x1 - x0 - 16
	if x1-x0 >= killMin {
		w = (x1-x0)/2 - (segKillR + segKillPad) - 6
	}
	return int(w / 5)
}

// A stop effect in the preview: the frame at its marker, layered over the
// running video (buildFxOverlay) with the widget's opacity as the fades -- the
// same overlay the render makes (freezeCues). Triggered by POSITION, not a
// crossing, so seeks are not missed. The frame comes from ffmpeg (ffmpegPNG),
// the same cut the render's overlay input makes.

// freezeNow is the stop effect standing over session time t -- the still the
// preview owes the screen -- or nil when the playhead is on running footage.
//
// A stop's bar is not quite the answer on its own. A stop is the ×0 in the
// mean every overlap is settled by (cut_speedmix.go), so a ×2 laid across
// part of it makes the mean ×1 there and the picture runs for those seconds:
// the still is owed exactly where the mean is nought.
func freezeNow(fx []cutFx, t float64) *cutFx {
	if fxMeanRate(fx, t) > 0 {
		return nil
	}
	for i := range fx {
		f := &fx[i]
		if f.frozenFx() && f.Dur > 0 && t >= f.T && t < f.T+f.Dur {
			return f
		}
	}
	return nil
}

// fxHush is whether the preview's sound is owed silence at session time t: a
// covering speed effect whose sound is Silent (cut_fxsound.go). The two 1×
// answers cannot be previewed -- the player has one pipeline, glued to the
// picture -- so those stretches preview at the picture's speed; the lane draws
// the difference (drawSndTail).
func fxHush(fx []cutFx, t float64) bool {
	for _, f := range fx {
		if f.Kind != "speed" || f.sound() != sndMute || f.Dur <= 0 {
			continue
		}
		if t >= f.T && t < f.T+f.Dur {
			return true
		}
	}
	return false
}

// fxStill is one stop effect's frame, rendered once and kept while the
// playhead is anywhere in its bar. One at a time: the playhead is in at most
// one bar, and a re-entered bar costs one re-render, which is what the first
// entry cost anyway.
type fxStill struct {
	t      float64 // the session moment the frame is cut at (the effect's T)
	tex    *gdk.Texture
	shown  bool
	busy   bool
	failed bool // it cannot be drawn; said once, then left alone
}

// Painting the finished frame, shared by Cut's editing overlay (cut_fxview.go)
// and Narrate's plain preview: everything here is a claim about what the RENDER
// makes -- where the output frame sits, how the camera maps into it, what is
// painted over, where an overlay's box lands. Editor-only things (outlines,
// labels, grabs) stay in cut_fxview.go.

// fxLiveFit is the mapping the finished frame is on at session time t: the
// camera rectangle there, put through liveZoom. ok is false for the cases a
// gsk transform cannot be built from -- a widget with no size yet, an unknown
// source size, a scale that came back NaN -- so the callers stop asking each
// in their own way and stop disagreeing about which ones matter.
func fxLiveFit(W, H, sw, sh, outA float64, fx []cutFx, t float64) (s, tx, ty float64, ok bool) {
	if W <= 0 || H <= 0 || sw <= 0 || sh <= 0 {
		return 0, 0, 0, false
	}
	s, tx, ty = liveZoom(W, H, sw, sh, outA, fxRectAt(fx, t, sw/sh, outA))
	if s <= 0 || math.IsNaN(s) || math.IsInf(s, 0) {
		return 0, 0, 0, false
	}
	return s, tx, ty, true
}

// fxMaskLive paints black over everything the finished video will not have:
// outside the output box, and any part of it the transformed frame (s,tx,ty
// from fxLiveFit) does not reach -- what the render fills with its backdrop.
func fxMaskLive(cr *cairo.Context, w, h, sw, sh, outA, s, tx, ty float64) {
	bx, by, bw, bh := fxDisp(w, h, outA)
	x0, y0 := math.Max(bx, tx), math.Max(by, ty)
	x1, y1 := math.Min(bx+bw, tx+sw*s), math.Min(by+bh, ty+sh*s)
	cr.SetSourceRGB(0, 0, 0)
	cr.Rectangle(0, 0, w, h)
	if x1 > x0 && y1 > y0 {
		cr.NewSubPath()
		cr.Rectangle(x0, y0, x1-x0, y1-y0)
	}
	cr.SetFillRule(cairo.FillRuleEvenOdd)
	cr.Fill()
	cr.SetFillRule(cairo.FillRuleWinding)
}

// fxOverPx is where an overlay's box lands, given the finished frame's own
// rectangle in the widget. An overlay is a fraction of the OUTPUT frame and not
// of the picture, which is what lets a title stay put while the camera moves
// under it.
func fxOverPx(f cutFx, ox, oy, ow, oh float64) (x, y, w, h float64) {
	bx, by, bw, bh := f.textBox().px(ow, oh)
	return ox + bx, oy + by, bw, bh
}

// The dark edge round a title, shared by render and preview: fxEdgeR is its
// reach as a fraction of the font size, fxEdgeA its darkness. The render
// strokes with them (textSVG, at twice the radius); the preview and thumbnail
// dilate. fxEdgeSteps samples sixteen directions -- eight was faceted and
// fatter on diagonals.
const (
	fxEdgeR     = 0.08
	fxEdgeA     = 0.85
	fxEdgeSteps = 16
)

// drawFxText paints one text effect as the render will: fitText's size and
// line breaks, each line centred (SVG text-anchor). The dark edge is the glyph
// drawn again around itself INTO A GROUP, composited once at one alpha -- the
// copies compound where they overlap otherwise and read as banded edges.
// Outline pass over all lines, then fill, so a descender is never drawn over
// the next line's outline (see textSVG).
func drawFxText(cr *cairo.Context, f cutFx, alpha, tx, ty, tw, th float64) {
	size, lines := fitText(f.Text, tw, th)
	if size <= 0 || alpha <= 0.01 {
		return
	}
	cr.SelectFontFace("sans-serif", cairo.FontSlantNormal, cairo.FontWeightBold)
	cr.SetFontSize(size)
	base := textBaselines(ty, th, size, len(lines))
	mid := tx + tw/2
	d := math.Max(1, size*fxEdgeR)
	left := func(ln string) float64 { return mid - cr.TextExtents(ln).XAdvance/2 }

	cr.PushGroup()
	cr.SetSourceRGB(0, 0, 0)
	for i, ln := range lines {
		if strings.TrimSpace(ln) == "" {
			continue
		}
		x := left(ln)
		for k := 0; k < fxEdgeSteps; k++ {
			a := 2 * math.Pi * float64(k) / fxEdgeSteps
			cr.MoveTo(x+d*math.Cos(a), base[i]+d*math.Sin(a))
			cr.ShowText(ln)
		}
	}
	cr.PopGroupToSource()
	cr.PaintWithAlpha(fxEdgeA * alpha)

	cr.SetSourceRGBA(1, 1, 1, alpha)
	for i, ln := range lines {
		if strings.TrimSpace(ln) == "" {
			continue
		}
		cr.MoveTo(left(ln), base[i])
		cr.ShowText(ln)
	}
}

// drawFxOver is either kind of overlay put on the finished frame the way the
// render will put it: the words drawn, or the drawing painted into its box. On
// the editor because the SVG cache is (svgSurface) -- one parse per drawing,
// shared by both previews.
func (ed *cutEditor) drawFxOver(cr *cairo.Context, f cutFx, alpha, ox, oy, ow, oh float64) {
	x, y, w, h := fxOverPx(f, ox, oy, ow, oh)
	if f.Kind == "svg" {
		ed.drawSVG(cr, f, x, y, w, h, alpha)
		return
	}
	drawFxText(cr, f, alpha, x, y, w, h)
}

// drawFxOverlaysAt is every overlay owed to session time t, faded exactly as
// the render fades it. The titles are the half of "show me the effects" that
// has nothing to do with the camera, and a preview without them is a preview of
// a video that has not been finished.
func (ed *cutEditor) drawFxOverlaysAt(cr *cairo.Context, fx []cutFx, t, ox, oy, ow, oh float64) {
	for _, i := range textsAt(fx, t) {
		ed.drawFxOver(cr, fx[i], textAlpha(fx[i], t), ox, oy, ow, oh)
	}
}
