package main

// The framing overlay over the preview: a transparent drawing area showing the
// camera at the playhead (the cut's aspect bright, the rest dimmed) and, with
// an effect armed or held, taking a drag to draw or slide the rectangle,
// snapped to the frame's edges. It only draws and hands normalized numbers to
// cutFx; the render (produce_fx.go) reads the same numbers.

import (
	"math"
	"strings"

	"github.com/diamondburned/gotk4/pkg/cairo"
	"github.com/diamondburned/gotk4/pkg/gdk/v4"
	"github.com/diamondburned/gotk4/pkg/gsk/v4"
	"github.com/diamondburned/gotk4/pkg/gtk/v4"
)

// ---- what the Cut page answers for its screen (fxPage) -----------------------

func (ed *cutEditor) fxCut() *cutEditor { return ed }
func (ed *cutEditor) fxPlayer() *Player { return ed.player }
func (ed *cutEditor) fxAt() float64     { return ed.playhead }

// screen is this page's fxScreen with the page wired in, done here rather than
// at construction because a cutEditor is a struct literal in a hundred tests.
// These forwarders shadow the promoted ones so every ed.livePlayhead() and
// ed.syncPreviewZoom() runs the code the Narrate preview runs.
func (ed *cutEditor) screen() *fxScreen {
	if ed.fxScreen.page == nil {
		ed.fxScreen.page = ed
	}
	return &ed.fxScreen
}

func (ed *cutEditor) livePlayhead() float64        { return ed.screen().livePlayhead() }
func (ed *cutEditor) reLive(t float64)             { ed.screen().reLive(t) }
func (ed *cutEditor) livePreview() bool            { return ed.screen().livePreview() }
func (ed *cutEditor) srcSize() (float64, float64)  { return ed.screen().srcSize() }
func (ed *cutEditor) liveSize() (float64, float64) { return ed.screen().liveSize() }
func (ed *cutEditor) outAspect() float64           { return ed.screen().outAspect() }
func (ed *cutEditor) syncPreviewZoom()             { ed.screen().syncPreviewZoom() }
func (ed *cutEditor) syncCamLayer()                { ed.screen().syncCamLayer() }
func (ed *cutEditor) fitStill()                    { ed.screen().fitStill() }
func (ed *cutEditor) syncFxStill()                 { ed.screen().syncFxStill() }
func (ed *cutEditor) paintLive(cr *cairo.Context, w, h int) bool {
	return ed.screen().paintLive(cr, w, h)
}

// fxSrcSize falls back to the first recording, then to plain HD, so the
// overlay never divides by zero.
func (ed *cutEditor) fxSrcSize() (float64, float64) {
	v := ed.playVideo
	if v == nil && len(ed.vids) > 0 {
		v = &ed.vids[0]
	}
	if v != nil && v.w > 0 && v.h > 0 {
		return float64(v.w), float64(v.h)
	}
	return 1920, 1080
}

// fxCamOK takes the camera layer down while a zoom is aimed by hand (you must
// see everything the camera could see) or a text's bar is held (its box is
// only editable on the layer with a grab). An armed TEXT keeps the layer up:
// its box lives on the OUTPUT frame.
func (ed *cutEditor) fxCamOK() bool {
	return ed.hasPlay && (ed.fxArm == "" || ed.fxOverArm()) &&
		ed.fxRectHeld() == nil && ed.fxHeldBox() == nil
}

// fxRectHeld is the held effect if it is one with a camera rectangle.
func (ed *cutEditor) fxRectHeld() *cutFx {
	if f := ed.heldFx(); f != nil && f.Kind == "zoom" {
		return f
	}
	return nil
}

// camInForce is the zoom whose OWN rectangle the picture is framed by at
// session time t. nil when the framing at t belongs to no single effect.
func (ed *cutEditor) camInForce(t float64) *cutFx {
	if i := ed.camIndexInForce(t); i >= 0 {
		return &ed.fx[i]
	}
	return nil
}

// camIndexInForce is camInForce as a position in ed.fx (holdFx takes an index;
// a pointer into a slice undo replaces is not one). A zoom whose band covers t
// wins, latest first; otherwise the staying zoom in force; -1 when neither --
// the camera parked on the whole frame, which no effect can be edited by.
func (ed *cutEditor) camIndexInForce(t float64) int {
	band, stay := -1, -1
	for i := range ed.fx {
		f := &ed.fx[i]
		if f.Kind != "zoom" {
			continue
		}
		if f.T <= t+1e-9 && t < f.T+math.Max(f.Dur, 0) && (band < 0 || f.T >= ed.fx[band].T) {
			band = i
		}
		if f.Stay && f.T <= t+1e-9 && (stay < 0 || f.T >= ed.fx[stay].T) {
			stay = i
		}
	}
	if band >= 0 {
		return band
	}
	return stay
}

// camMoving is whether the camera is MID-MOVE at t -- inside some zoom's fade
// in or fade out, on its way between two framings.
//
// Every fade in counts, the cut's first included: it travels from the plain
// centred slice the video opens on, which is a journey like any other now that
// no zoom reaches back before its own T (camRectAt).
func camMoving(fx []cutFx, t float64) bool {
	for _, z := range zoomsOf(fx) {
		tin, tout := z.zoomGlides()
		if tin > 0 && t >= z.T && t < z.T+tin {
			return true
		}
		if !z.Stay && tout > 0 && t >= z.T+math.Max(z.Dur, 0)-tout && t < z.T+math.Max(z.Dur, 0) {
			return true
		}
	}
	return false
}

// fxCamSettled is whether the camera is STANDING STILL at t -- parked on one
// zoom's rectangle, no fade half done -- which is the condition for offering
// the rectangle to the hand. Mid-move it belongs to no single effect.
func fxCamSettled(fx []cutFx, t float64) bool {
	return len(zoomsOf(fx)) > 0 && !camMoving(fx, t)
}

// fxDragTarget is the effect a drag on the picture edits when nothing is held:
// the zoom the picture is framed by right now. Deliberately narrow: nothing
// while playing (the camera layer uses a different mapping, syncPreviewZoom),
// nothing while the camera moves, nothing before the first zoom.
func (ed *cutEditor) fxDragTarget() *cutFx {
	if f := ed.fxRectHeld(); f != nil {
		return f
	}
	if !ed.hasPlay || ed.livePreview() || !fxCamSettled(ed.fx, ed.playhead) {
		return nil
	}
	return ed.camInForce(ed.playhead)
}

// fxHeldBox is the held effect if it is one of the two laid over the picture
// -- a text or a drawing (overFx) -- which are the other kind of rectangle the
// picture offers, and the only one not bound to the cut's aspect.
func (ed *cutEditor) fxHeldBox() *cutFx {
	if f := ed.heldFx(); f != nil && overFx(*f) {
		return f
	}
	return nil
}

// fxOverArm is whether the next drag on the picture draws an overlay's box
// rather than a camera window. The two kinds are armed apart -- a text needs
// words typed, a drawing needs a file chosen -- and treated alike from the
// drag on, because a box is a box.
func (ed *cutEditor) fxOverArm() bool {
	return ed.fxArm == "text" || ed.fxArm == "svg"
}

// fxEdges says what a point at (x, y) has hold of on the rectangle r: which
// borders it is within reach of, and whether it is inside at all. Both edges
// at once is a corner, which is why these come back as two independent axes.
func fxEdges(x, y, rx, ry, rw, rh float64) (horiz, vert, left, top, inside bool) {
	inX := x >= rx-fxGrab && x <= rx+rw+fxGrab
	inY := y >= ry-fxGrab && y <= ry+rh+fxGrab
	nearL, nearR := math.Abs(x-rx) <= fxGrab, math.Abs(x-(rx+rw)) <= fxGrab
	nearT, nearB := math.Abs(y-ry) <= fxGrab, math.Abs(y-(ry+rh)) <= fxGrab
	horiz, vert = (nearL || nearR) && inY, (nearT || nearB) && inX
	inside = x >= rx && x <= rx+rw && y >= ry && y <= ry+rh
	return horiz, vert, nearL, nearT, inside
}

// fxCursorName is the pointer over a rectangle: the resize arrows on the
// borders and the corners, the move hand inside. Naming what a border does
// before it is pressed is the whole of how anyone finds out it resizes.
func fxCursorName(horiz, vert, left, top, inside bool) string {
	switch {
	case horiz && vert:
		if left == top {
			return "nwse-resize"
		}
		return "nesw-resize"
	case horiz:
		return "ew-resize"
	case vert:
		return "ns-resize"
	case inside:
		return "move"
	}
	return "default"
}

// resizeRect is a border drag in widget pixels: the hand at (x, y), the fixed
// far edge/corner at (ax, ay), pxA the cut's aspect in those pixels;
// horiz/vert say which borders were grabbed, left/top which side. ONE number
// is dragged -- the height -- and the width follows the aspect; a corner takes
// the axis the hand moved further. Returns the new centre and height.
func resizeRect(x, y, ax, ay, pxA float64, horiz, vert, left, top bool) (cx, cy, h float64) {
	switch {
	case horiz && vert:
		h = math.Max(math.Abs(x-ax)/pxA, math.Abs(y-ay))
	case horiz:
		h = math.Abs(x-ax) / pxA
	default:
		h = math.Abs(y - ay)
	}
	h = math.Max(h, 12) // a rectangle smaller than this is a slip, not an edit
	w := h * pxA
	cx, cy = ax, ay
	if horiz {
		cx = ax + w/2
		if left {
			cx = ax - w/2
		}
	}
	if vert {
		cy = ay + h/2
		if top {
			cy = ay - h/2
		}
	}
	return cx, cy, h
}

// resizeFree is a border drag on a rectangle that owes nothing to the cut's
// aspect -- a text box, which is whatever shape the words want. The two axes
// are independent: a side moves one edge, a corner moves two, and an edge that
// was not grabbed does not move at all. x0,y0,w0,h0 is the rectangle as it was;
// ax,ay the edges being kept still. min is the smallest it may become, so a
// box cannot be collapsed into something the hand can no longer find.
func resizeFree(x, y, ax, ay, x0, y0, w0, h0, min float64,
	horiz, vert, left, top bool) (nx, ny, nw, nh float64) {
	nx, ny, nw, nh = x0, y0, w0, h0
	if horiz {
		if left {
			nx = math.Min(x, ax-min)
			nw = ax - nx
		} else {
			nw = math.Max(x-ax, min)
			nx = ax
		}
	}
	if vert {
		if top {
			ny = math.Min(y, ay-min)
			nh = ay - ny
		} else {
			nh = math.Max(y-ay, min)
			ny = ay
		}
	}
	return nx, ny, nw, nh
}

// fxGrab is how near the rectangle's edge a press counts as taking hold of
// that edge rather than of the whole rectangle. A 1.5 px line is not a mouse
// target; this is, on either side of it.
const fxGrab = 9.0

// syncFxCursor decides whether the overlay owns the pointer. Only while a
// drag would mean something: armed, or holding a view/zoom to re-frame. The
// rest of the time it must not take the click that toggles playback.
func (ed *cutEditor) syncFxCursor() {
	ed.syncFxArm() // the same state, said in the column where it can be read
	if ed.fxArea == nil {
		return
	}
	armed := ed.fxArm != ""
	held := ed.fxRectHeld() != nil || ed.fxHeldBox() != nil
	// the overlay also takes the pointer when there is simply something on the
	// picture to grab -- a view framing it, a text showing on it -- so that a
	// box can be moved and resized where it is seen. That swallows the click
	// that toggles playback, so every press that turns out not to be a drag is
	// handed back to it (see the drag's end).
	ed.fxArea.SetCanTarget(armed || held || ed.fxDragTarget() != nil ||
		len(textsAt(ed.fx, ed.playhead)) > 0)
	if armed {
		// nothing is under the pointer yet; the motion controller takes over
		// as soon as it moves. Its cache is set here too, or the first move
		// after arming would decide the crosshair is already up and leave the
		// resize arrow the last hover put there.
		ed.fxCursor = "crosshair"
		ed.fxArea.SetCursor(gdk.NewCursorFromName("crosshair", nil))
	} else if ed.fxCursor == "crosshair" {
		ed.fxCursor = "default"
		ed.fxArea.SetCursor(gdk.NewCursorFromName("default", nil))
	}
}

// fxDisp is where the video's picture actually is inside the overlay widget:
// GtkPicture keeps the frame's aspect (contain), so there are bars either
// side or above, and the maths here has to match or the rectangle is drawn on
// the bars.
func fxDisp(w, h, srcA float64) (ox, oy, dw, dh float64) {
	if w <= 0 || h <= 0 || srcA <= 0 {
		return 0, 0, w, h
	}
	dw = w
	dh = w / srcA
	if dh > h {
		dh = h
		dw = h * srcA
	}
	return (w - dw) / 2, (h - dh) / 2, dw, dh
}

// snapRectPos pulls a sliding camera rectangle onto the frame's edges when it
// comes close: cx,cy is the centre, wf,hf the rectangle's size, all fractions
// of the source frame; tx,ty how close counts, in the same units. The axes
// snap independently, so corners come for free.
func snapRectPos(cx, cy, wf, hf, tx, ty float64) (float64, float64) {
	switch {
	case math.Abs(cx-wf/2) < tx:
		cx = wf / 2 // left edge on the frame's left
	case math.Abs(cx-(1-wf/2)) < tx:
		cx = 1 - wf/2 // right edge on the frame's right
	}
	switch {
	case math.Abs(cy-hf/2) < ty:
		cy = hf / 2 // top on top
	case math.Abs(cy-(1-hf/2)) < ty:
		cy = 1 - hf/2 // bottom on bottom
	}
	return cx, cy
}

// fxSnapPx is how near a line has to come, in widget pixels, before a
// rectangle on the picture is put exactly on it. The same reach the timeline's
// own snapping uses, measured on the screen rather than in seconds.
const fxSnapPx = 10.0

// snapPointPx pulls a dragged edge onto one of the frame's own lines when the
// hand brings it within tol pixels -- the nearest one wins, and a hand nowhere
// near any of them is left exactly where it is. This is the resize half of the
// snapping: what moves under the hand IS the edge, so putting the pointer on
// the line puts the edge on it.
func snapPointPx(v, tol float64, lines ...float64) float64 {
	best, d := v, tol
	for _, l := range lines {
		if e := math.Abs(v - l); e < d {
			best, d = l, e
		}
	}
	return best
}

// snapEdgePx is the move half: a whole rectangle is sliding, so any of its
// three lines on this axis -- near edge, middle, far edge -- may be the one
// that lands. v is the near edge and ln the length; whichever of the three
// comes closest to one of the frame's lines is put on it and the rectangle
// carried with it. Nothing within reach leaves v alone.
func snapEdgePx(v, ln, tol float64, lines ...float64) float64 {
	best, d := v, tol
	for _, mine := range []float64{v, v + ln/2, v + ln} {
		for _, l := range lines {
			if e := math.Abs(mine - l); e < d {
				best, d = v+(l-mine), e
			}
		}
	}
	return best
}

// fxZoomDrag reports that the drag in flight belongs to a zoom -- armed, or
// re-framing a held one. A zoom's rectangle is not bound by the cut's aspect.
func (ed *cutEditor) fxZoomDrag() bool {
	if ed.fxArm != "" {
		return ed.fxArm == "zoom"
	}
	f := ed.fxRectHeld()
	return f != nil && f.Kind == "zoom"
}

// fxFreeDrag reports that the rectangle being drawn is a free shape rather
// than one locked to the cut's aspect: a zoom's window (which the render fits
// the output box into) and a text's box (which is a shape on the finished
// frame, not a frame of its own).
func (ed *cutEditor) fxFreeDrag() bool {
	return ed.fxOverArm() || ed.fxZoomDrag()
}

// fxClampRect keeps a camera rectangle somewhere a person can find it again.
// The limits are deliberately loose -- a pull-back well past the frame's edge
// is a real shot, and so is a tight crop -- they only rule out the rectangles
// that are no longer a picture: a hair-thin sliver, something a thousand
// frames wide, a centre dragged into the void off the side of the recording.
func fxClampRect(f *cutFx) {
	if f == nil {
		return
	}
	f.Hf = math.Min(math.Max(f.Hf, 0.02), 12)
	f.Cx = math.Min(math.Max(f.Cx, -2), 3)
	f.Cy = math.Min(math.Max(f.Cy, -2), 3)
}

// liveZoom is the transform that puts the camera on the playing preview: the
// scale s and offset tx,ty that map the picture, drawn 1:1 at sw×sh, so the
// camera rect r fills the output-shaped box centred in a W×H widget.
func liveZoom(W, H, sw, sh, outA float64, r fxRect) (s, tx, ty float64) {
	bx, by, bw, bh := fxDisp(W, H, outA)
	s = bh / (r.hf * sh)
	tx = bx + bw/2 - r.cx*sw*s
	ty = by + bh/2 - r.cy*sh*s
	return
}

// stillFit is that mapping, source pixels to widget pixels, as one scale and
// one offset. Live, it is the camera's own -- the SAME numbers the footage
// under the still is on, which is the point: the two layers move together or
// the frozen frame is a picture the camera cannot reach. Not live, it is the
// plain contain fit GtkPicture would do, which is what the raw frame beneath
// is on.
func stillFit(W, H, sw, sh, outA float64, live bool, r fxRect) (s, tx, ty float64) {
	if live {
		return liveZoom(W, H, sw, sh, outA, r)
	}
	s = math.Min(W/sw, H/sh)
	return s, (W - sw*s) / 2, (H - sh*s) / 2
}

// zoomTransform builds translate(tx,ty)·scale(s) as ONE matrix from a nil
// transform. Chaining gsk_transform_* calls consumes each transform while the
// Go value keeps a finalizer that unrefs it again -- a double free on every
// tick. The result goes to SetChildTransform (transfer-none), so its one
// finalizer balances.
func zoomTransform(s, tx, ty float64) *gsk.Transform {
	var identity *gsk.Transform
	return identity.Matrix2D(float32(s), 0, 0, float32(s), float32(tx), float32(ty))
}

// buildFxOverlay wraps the preview picture with the framing overlay and wires
// its gestures. Everything the drag knows lives in the closure; what it emits
// goes through the same addFx/updateFx as every other edit.
func (ed *cutEditor) buildFxOverlay() *gtk.Overlay {
	// the three layers themselves are the shared ones (cut_fxscreen.go), so
	// this page and Narrate's are the same picture; what is added here is
	// everything only an editor has, starting with the gestures
	over := ed.screen().buildLayers(ed, ed.player.Picture, ed.player.video)
	// the wheel over the picture steps frames -- the preview is where you look
	// while hunting for one, so it answers the scrub gesture itself
	over.AddController(ed.wheelFrames())
	area := ed.fxArea

	// The camera's own clock: the 100ms timer is right for a red line and wrong
	// for a glide, and the render (zoompan per frame) has no steps. The frame
	// clock costs nothing while the layer is down; the timeline keeps its 100ms.
	area.AddTickCallback(func(_ gtk.Widgetter, _ gdk.FrameClocker) bool {
		if ed.livePreview() && ed.player != nil && ed.player.playing {
			ed.syncPreviewZoom()
			area.QueueDraw() // the mask and the titles move with the camera
		}
		return true
	})

	// ---- geometry ---------------------------------------------------------
	//
	// Three nested rectangles: dispPx is where the video's picture is in the
	// widget (GtkPicture letterboxes), camPx the camera's window on it -- what
	// the finished video shows -- and a text box is a fraction of THAT (fxtext.go).

	dispPx := func() (ox, oy, dw, dh float64) {
		aw, ah := float64(area.AllocatedWidth()), float64(area.AllocatedHeight())
		sw, sh := ed.srcSize()
		return fxDisp(aw, ah, sw/sh)
	}

	// pxAspect is the cut's aspect measured in widget pixels: dh widget px are
	// sh source px, so a rectangle's width is its height times this.
	pxAspect := func() float64 {
		sw, sh := ed.srcSize()
		_, _, dw, dh := dispPx()
		if dw <= 0 || dh <= 0 || sw <= 0 || sh <= 0 {
			return ed.outAspect()
		}
		return ed.outAspect() * (dw / sw) * (sh / dh)
	}

	// rectPx puts a normalized camera rectangle on the widget.
	rectPx := func(r fxRect) (rx, ry, rw, rh float64) {
		ox, oy, dw, dh := dispPx()
		rh = r.hf * dh
		rw = rh * pxAspect()
		return ox + r.cx*dw - rw/2, oy + r.cy*dh - rh/2, rw, rh
	}

	// camPx is the camera rectangle the overlay is showing: the held view or
	// zoom's own, or -- with nothing held -- where the camera is at the
	// playhead. Not ok when nothing frames the picture at all.
	camPx := func() (rx, ry, rw, rh float64, ok bool) {
		if f := ed.fxRectHeld(); f != nil {
			rx, ry, rw, rh = rectPx(fxRect{f.Cx, f.Cy, f.Hf})
			return rx, ry, rw, rh, true
		}
		if !ed.hasPlay || !fxHasCamera(ed.aspect, ed.fx) {
			return
		}
		sw, sh := ed.srcSize()
		rx, ry, rw, rh = rectPx(fxRectAt(ed.fx, ed.playhead, sw/sh, ed.outAspect()))
		return rx, ry, rw, rh, true
	}

	// outFramePx is the finished frame on screen -- the rectangle a text box
	// is a fraction of. That is the camera's window when there is a camera,
	// the whole picture when there is not, and while the preview is playing
	// the output box the live layer is drawn into.
	outFramePx := func() (x, y, w, h float64) {
		if ed.livePreview() {
			aw, ah := float64(area.AllocatedWidth()), float64(area.AllocatedHeight())
			return fxDisp(aw, ah, ed.outAspect())
		}
		if rx, ry, rw, rh, ok := camPx(); ok {
			return rx, ry, rw, rh
		}
		return dispPx()
	}

	textPx := func(f cutFx) (x, y, w, h float64) {
		ox, oy, ow, oh := outFramePx()
		return fxOverPx(f, ox, oy, ow, oh)
	}

	// setTextPx is the way back: a rectangle on the widget written onto the
	// effect as fractions of the finished frame, clamped so a box can never be
	// dragged off the video and lost.
	setTextPx := func(f *cutFx, x, y, w, h float64) {
		ox, oy, ow, oh := outFramePx()
		if ow <= 0 || oh <= 0 {
			return
		}
		b := fxBox{cx: (x + w/2 - ox) / ow, cy: (y + h/2 - oy) / oh, wf: w / ow, hf: h / oh}.clamp()
		f.Cx, f.Cy, f.Wf, f.Hf = b.cx, b.cy, b.wf, b.hf
	}

	// ---- what a press takes hold of ----------------------------------------

	// fxGrabbed is one rectangle offered to the hand: the effect behind it,
	// where it is on the widget, and which of its borders the pointer has.
	type fxGrabbed struct {
		f                              *cutFx
		text                           bool
		x, y, w, h                     float64
		horiz, vert, left, top, inside bool
	}
	// grabAt is the whole hit test, and its ORDER is the rule the picture
	// follows: whatever is held answers first (you are working on it), then
	// the texts on screen from the top down (they sit inside the camera
	// rectangle, so they have to be asked before it), and last the view that
	// frames the picture. Nothing while an effect is armed -- that drag is
	// drawing a new box -- and nothing while the preview plays.
	grabAt := func(x, y float64) *fxGrabbed {
		try := func(f *cutFx, text bool, rx, ry, rw, rh float64) *fxGrabbed {
			g := &fxGrabbed{f: f, text: text, x: rx, y: ry, w: rw, h: rh}
			g.horiz, g.vert, g.left, g.top, g.inside = fxEdges(x, y, rx, ry, rw, rh)
			if !g.horiz && !g.vert && !g.inside {
				return nil
			}
			return g
		}
		if ed.fxArm != "" || ed.livePreview() {
			return nil
		}
		if f := ed.fxHeldBox(); f != nil {
			tx, ty, tw, th := textPx(*f)
			return try(f, true, tx, ty, tw, th)
		}
		if f := ed.fxRectHeld(); f != nil {
			if rx, ry, rw, rh, ok := camPx(); ok {
				return try(f, false, rx, ry, rw, rh)
			}
			return nil
		}
		vis := textsAt(ed.fx, ed.playhead)
		for i := len(vis) - 1; i >= 0; i-- {
			f := &ed.fx[vis[i]]
			tx, ty, tw, th := textPx(*f)
			if g := try(f, true, tx, ty, tw, th); g != nil {
				return g
			}
		}
		if f := ed.fxDragTarget(); f != nil {
			if rx, ry, rw, rh, ok := camPx(); ok {
				return try(f, false, rx, ry, rw, rh)
			}
		}
		return nil
	}

	// The drag in flight, in widget px. Its kind is settled at the press and
	// never changes under the hand:
	//
	//	""     grabbed nothing -- the press is the preview's own click
	//	draw   a fresh rectangle, dragged corner to corner
	//	move   slides what was grabbed
	//	size   scales it about the border opposite the one taken hold of
	var drag struct {
		on             bool
		kind           string
		x0, y0, x1, y1 float64
		moved          bool
		g              fxGrabbed
		cx0, cy0       float64 // a camera rectangle's centre when the press landed
		ax, ay         float64 // the point a resize keeps still
	}

	// dragRect is the rectangle the current drag means, in widget px:
	// anchored at the press, following the pointer, locked to the cut's aspect
	// and snapped to the frame's full width or height when it comes within
	// reach -- unless it is a free one (a zoom's window, a text's box), which
	// is simply the two corners.
	dragRect := func() (x, y, w, h float64) {
		_, _, dw, dh := dispPx()
		pxA := pxAspect()
		dx, dy := drag.x1-drag.x0, drag.y1-drag.y0
		if ed.fxFreeDrag() {
			return math.Min(drag.x0, drag.x1), math.Min(drag.y0, drag.y1),
				math.Abs(dx), math.Abs(dy)
		}
		h = math.Max(math.Abs(dy), math.Abs(dx)/pxA)
		// snap: close to the whole frame in either direction is the whole frame
		if math.Abs(h*pxA-dw) < dw*0.05 {
			h = dw / pxA
		} else if math.Abs(h-dh) < dh*0.05 {
			h = dh
		}
		w = h * pxA
		x, y = drag.x0, drag.y0
		if dx < 0 {
			x -= w
		}
		if dy < 0 {
			y -= h
		}
		return x, y, w, h
	}

	// drawOver is either kind of overlay put on the picture the way the render
	// will put it, and the way the OTHER preview puts it: Narrate draws the
	// same titles over the same finished frame (cut_fxpaint.go), so the words,
	// the font fitting and the fades are shared and only the box is local.
	drawOver := func(cr *cairo.Context, f cutFx, alpha float64) {
		ox, oy, ow, oh := outFramePx()
		ed.drawFxOver(cr, f, alpha, ox, oy, ow, oh)
	}

	// boxOutline is the dashed violet frame around a text box, drawn only for
	// the one being worked on: a title with a rectangle permanently round it
	// would not look like the title the render makes.
	boxOutline := func(cr *cairo.Context, x, y, w, h float64) {
		cr.SetSourceRGBA(0.6, 0.55, 0.95, 0.9)
		cr.SetLineWidth(1.5)
		cr.SetDash([]float64{4, 3}, 0)
		cr.Rectangle(x, y, w, h)
		cr.Stroke()
		cr.SetDash(nil, 0)
	}

	area.SetDrawFunc(func(_ *gtk.DrawingArea, cr *cairo.Context, w, h int) {
		if ed.player == nil || ed.player.still {
			return // a card is on screen; the camera talks about footage
		}
		if ed.livePreview() {
			// the camera is live on the picture underneath (syncPreviewZoom),
			// so the mask and the titles are the shared painting the Narrate
			// preview also does (fxScreen.paintLive) and nothing else -- the
			// outline waits for the pause
			if !ed.paintLive(cr, w, h) {
				return
			}
			// an overlay's box being drawn by hand: the one arm that keeps
			// this layer up (syncPreviewZoom), because the box is a fraction
			// of the OUTPUT frame and this is the output frame on screen
			if drag.on && drag.kind == "draw" && ed.fxOverArm() {
				x, y, ww, hh := dragRect()
				boxOutline(cr, x, y, ww, hh)
			}
			return
		}
		// 1. the camera rectangle, or the one being drawn in its place
		var rx, ry, rw, rh float64
		haveCam := false
		label := ed.aspect
		drawingCam := drag.on && drag.kind == "draw" && !ed.fxOverArm()
		switch {
		case drawingCam:
			rx, ry, rw, rh = dragRect()
			haveCam = true
		default:
			if x, y, ww, hh, ok := camPx(); ok {
				rx, ry, rw, rh, haveCam = x, y, ww, hh, true
				if f := ed.fxRectHeld(); f != nil {
					label = f.fxLabel()
				}
			}
		}
		if haveCam {
			// dim what the finished video does not show
			cr.SetSourceRGBA(0, 0, 0, 0.45)
			cr.Rectangle(0, 0, float64(w), float64(h))
			cr.NewSubPath()
			cr.Rectangle(rx, ry, rw, rh)
			cr.SetFillRule(cairo.FillRuleEvenOdd)
			cr.Fill()
			cr.SetFillRule(cairo.FillRuleWinding)
			col := [3]float64{1, 1, 1}
			if drawingCam || ed.fxRectHeld() != nil {
				// the lane's own two camera colours, so the box on the
				// picture and the bar under it are plainly the same effect:
				// the close-up that comes back out, and the reframing that
				// stays on the region
				col = [3]float64{0.25, 0.72, 0.82}
				if f := ed.fxRectHeld(); f != nil && f.Stay {
					col = [3]float64{0.95, 0.62, 0.15}
				}
			}
			cr.SetSourceRGB(col[0], col[1], col[2])
			cr.SetLineWidth(1.5)
			cr.Rectangle(rx, ry, rw, rh)
			cr.Stroke()
			if label != "" {
				cr.SetFontSize(10)
				plateText(cr, rx+4, ry+13, label)
			}
		}
		// 2. the words on the finished frame, faded as the render will fade
		// them -- and the held one always, wherever the playhead is, because
		// something being edited has to be visible to be edited
		held := ed.fxHeldBox()
		for _, i := range textsAt(ed.fx, ed.playhead) {
			if f := &ed.fx[i]; f != held {
				drawOver(cr, *f, textAlpha(*f, ed.playhead))
			}
		}
		if held != nil {
			drawOver(cr, *held, 1)
			tx, ty, tw, th := textPx(*held)
			boxOutline(cr, tx, ty, tw, th)
		}
		// 3. a text box being drawn by hand, or one under the pointer that a
		// press would take hold of
		switch {
		case drag.on && drag.kind == "draw" && ed.fxOverArm():
			x, y, ww, hh := dragRect()
			boxOutline(cr, x, y, ww, hh)
		case drag.on && drag.g.text && drag.g.f != nil && drag.g.f != held:
			tx, ty, tw, th := textPx(*drag.g.f)
			boxOutline(cr, tx, ty, tw, th)
		}
	})

	g := gtk.NewGestureDrag()
	g.ConnectDragBegin(func(x, y float64) {
		drag.kind, drag.moved, drag.g = "", false, fxGrabbed{}
		drag.on = true
		drag.x0, drag.y0, drag.x1, drag.y1 = x, y, x, y
		defer area.QueueDraw()
		if ed.fxArm != "" {
			drag.kind = "draw" // an armed button always means a fresh box
			return
		}
		gr := grabAt(x, y)
		if gr == nil {
			// clear of everything: with a camera rectangle held a drag draws a new one
			// for it; otherwise the press is the picture's, which answers a click with
			// play/pause (ConnectDragEnd). Nothing is offered while the framed layer is
			// up: framing starts by taking the effect off the lane.
			if ed.fxRectHeld() != nil {
				drag.kind = "draw"
			}
			return
		}
		drag.g = *gr
		drag.cx0, drag.cy0 = gr.f.Cx, gr.f.Cy
		if gr.horiz || gr.vert {
			// an edge grabbed square-on keeps the middle of the opposite one,
			// so the rectangle grows along that axis alone; a corner keeps the
			// far corner, which is what every resize handle everywhere does
			drag.kind = "size"
			drag.ax, drag.ay = gr.x+gr.w/2, gr.y+gr.h/2
			if gr.horiz {
				drag.ax = gr.x
				if gr.left {
					drag.ax = gr.x + gr.w
				}
			}
			if gr.vert {
				drag.ay = gr.y
				if gr.top {
					drag.ay = gr.y + gr.h
				}
			}
			return
		}
		drag.kind = "move"
	})
	g.ConnectDragUpdate(func(dx, dy float64) {
		if !drag.on {
			return
		}
		drag.x1, drag.y1 = drag.x0+dx, drag.y0+dy
		editing := drag.kind == "move" || drag.kind == "size"
		if !drag.moved && math.Hypot(dx, dy) > 2 {
			drag.moved = true
			if editing {
				ed.pushUndo() // once, before anything shifts: one Undo per drag
			}
		}
		if editing && drag.moved {
			f := drag.g.f
			ox, oy, dw, dh := dispPx()
			if f == nil || dw <= 0 || dh <= 0 {
				drag.on = false
				return
			}
			switch {
			case drag.g.text && drag.kind == "move":
				// the finished frame's own edges and middles: a caption is
				// nearly always against a side or centred, and hitting that
				// by hand at this size is a matter of luck
				fx0, fy0, fw0, fh0 := outFramePx()
				nx, ny := drag.g.x+dx, drag.g.y+dy
				if fw0 > 0 && fh0 > 0 {
					nx = snapEdgePx(nx, drag.g.w, fxSnapPx, fx0, fx0+fw0/2, fx0+fw0)
					ny = snapEdgePx(ny, drag.g.h, fxSnapPx, fy0, fy0+fh0/2, fy0+fh0)
				}
				setTextPx(f, nx, ny, drag.g.w, drag.g.h)
			case drag.g.text:
				// a text box owes nothing to the cut's aspect: the two axes
				// move independently, and an edge not grabbed does not move.
				// The edge under the hand snaps to the same lines the whole
				// box does, so a box widened to the frame lands ON it
				fx0, fy0, fw0, fh0 := outFramePx()
				px, py := drag.x1, drag.y1
				if fw0 > 0 && fh0 > 0 {
					px = snapPointPx(px, fxSnapPx, fx0, fx0+fw0/2, fx0+fw0)
					py = snapPointPx(py, fxSnapPx, fy0, fy0+fh0/2, fy0+fh0)
				}
				nx, ny, nw, nh := resizeFree(px, py, drag.ax, drag.ay,
					drag.g.x, drag.g.y, drag.g.w, drag.g.h, 16,
					drag.g.horiz, drag.g.vert, drag.g.left, drag.g.top)
				setTextPx(f, nx, ny, nw, nh)
			case drag.kind == "move":
				// the rectangle's width as a fraction of the frame's width:
				// hf frame-heights tall, and the cut's aspect wide
				sw, sh := ed.srcSize()
				wf := f.Hf * (sh / sw) * ed.outAspect()
				cx, cy := drag.cx0+dx/dw, drag.cy0+dy/dh
				f.Cx, f.Cy = snapRectPos(cx, cy, wf, f.Hf, 10/dw, 10/dh)
				fxClampRect(f)
			default:
				// the same snapping the slide already had, on the edge being
				// dragged: a rectangle pulled out to the top of the recording
				// lands on the top instead of a pixel under it, which at this
				// size is a black hairline in the finished video
				px := snapPointPx(drag.x1, fxSnapPx, ox, ox+dw)
				py := snapPointPx(drag.y1, fxSnapPx, oy, oy+dh)
				cx, cy, h := resizeRect(px, py, drag.ax, drag.ay,
					pxAspect(), drag.g.horiz, drag.g.vert, drag.g.left, drag.g.top)
				f.Hf = h / dh
				f.Cx, f.Cy = (cx-ox)/dw, (cy-oy)/dh
				fxClampRect(f)
			}
			ed.syncPreviewZoom() // the live layer may be showing this camera
		}
		area.QueueDraw()
	})
	g.ConnectDragEnd(func(dx, dy float64) {
		if !drag.on {
			return
		}
		drag.on = false
		area.QueueDraw()
		zoomFree := ed.fxZoomDrag() // before the arm is cleared below
		switch drag.kind {
		case "", "move", "size":
			// A press that never moved is a CLICK, and a click on the picture
			// plays or pauses it -- wherever it landed. The overlay owns the
			// pointer over most of the frame now, and a picture that stops
			// answering the click that starts it because there happens to be a
			// view under the finger is a worse trade than any gesture is worth.
			if !drag.moved {
				ed.toggle()
				return
			}
			if f := drag.g.f; f != nil && drag.kind != "" {
				ed.persist()
				what := " moved"
				if drag.kind == "size" {
					what = " resized"
				}
				ed.a.setStatus(f.fxLabel() + what + " — ↶ Undo takes it back")
			}
			return
		}
		rx, ry, rw, rh := dragRect()
		ox, oy, dw, dh := dispPx()
		tiny := math.Max(rw, rh) < 12 || dw <= 0 || dh <= 0
		if ed.fxOverArm() {
			// a click is enough for an overlay: the default box -- the lower
			// third for a caption, the middle for a drawing -- is where each
			// usually goes, and having to draw one before being allowed to
			// type or to choose is a toll on the common case
			kind := ed.fxArm
			ed.fxArm = ""
			ed.syncFxCursor()
			// the marked stretch is the seconds it covers, exactly as it is
			// for a speed: marking the words' moment and then drawing their
			// box is two halves of one sentence
			t0, dur := ed.fxSpanNow(3)
			f := cutFx{Kind: kind, T: t0, Dur: dur, Trans: 0.3, Tout: 0.3}
			b := fxTextDefault
			if kind == "svg" {
				f.Src, b = ed.fxSrc, fxSvgDefault
			}
			if !tiny {
				fx0, fy0, fw0, fh0 := outFramePx()
				if fw0 > 0 && fh0 > 0 {
					b = fxBox{cx: (rx + rw/2 - fx0) / fw0, cy: (ry + rh/2 - fy0) / fh0,
						wf: rw / fw0, hf: rh / fh0}.clamp()
				}
			}
			f.Cx, f.Cy, f.Wf, f.Hf = b.cx, b.cy, b.wf, b.hf
			if kind == "svg" {
				ed.a.askSvgParams(f, true, func(nf cutFx) {
					if strings.TrimSpace(nf.Src) == "" {
						// the form is live and this is its first answer, given
						// as it opened: there is nothing to place until a file
						// is chosen, and choosing one is an answer too (fxWin)
						ed.a.setStatus("choose a drawing and it goes on the picture")
						return
					}
					ed.addFx(nf)
					ed.a.setStatus(nf.fxLabel() + " — the drawing is on the picture for " +
						"those seconds; ↶ Undo takes it back")
				})
				return
			}
			ed.a.askTextParams(f, true, func(nf cutFx) {
				if strings.TrimSpace(nf.Text) == "" {
					// as above: an empty caption is not placed, and the words
					// are what place it -- they go on the picture as they are
					// typed
					ed.a.setStatus("type the words and they go on the picture — " +
						"the form applies as you type it")
					return
				}
				ed.addFx(nf)
				ed.a.setStatus(nf.fxLabel() + " — the words are on the picture for those " +
					"seconds; ↶ Undo takes it back")
			})
			return
		}
		if tiny {
			return // a click, not a framing -- the arm stays up for another try
		}
		cx := (rx + rw/2 - ox) / dw
		cy := (ry + rh/2 - oy) / dh
		hf := rh / dh
		if zoomFree {
			// the smallest output-shaped window that holds the free drawing
			hf = math.Max(rh, rw/pxAspect()) / dh
		}
		switch {
		case ed.fxArm == "zoom":
			ed.fxArm = ""
			ed.syncFxCursor()
			// a cut that has been given a shape of its own and has not been
			// framed yet is asking the one question a staying zoom answers --
			// which slice of the recording the finished video shows -- so
			// that is what the first box drawn on it opens as. Everywhere
			// else the ordinary close-up that comes back out is the guess.
			// Either way the dialog shows the choice; neither is hidden.
			stay := ed.aspect != "" && !ed.hasStay()
			t0, dur := ed.fxSpanNow(3)
			f := cutFx{Kind: "zoom", T: t0, Cx: cx, Cy: cy, Hf: hf,
				Trans: 1, Tout: 1, Dur: dur, Stay: stay}
			if stay {
				f.Trans, f.Tout = 0, 0
			}
			fxClampRect(&f)
			ed.a.askZoomParams(f, true, func(nf cutFx) {
				ed.addFx(nf)
				msg := " — ↶ Undo takes it back"
				if nf.Stay {
					msg = " — the video shows this region from here on; ↶ Undo takes it back"
				}
				ed.a.setStatus(nf.fxLabel() + msg)
			})
		default:
			if f := ed.fxRectHeld(); f != nil {
				ed.pushUndo()
				f.Cx, f.Cy, f.Hf = cx, cy, hf
				fxClampRect(f)
				ed.persist()
				ed.a.setStatus(f.fxLabel() + " re-framed — ↶ Undo takes it back")
			}
		}
	})
	area.AddController(g)

	// the pointer says what a press would do before it is pressed: the resize
	// arrows on a border, the move hand inside a box. Without this the border
	// handle is invisible -- there is nothing on a 1.5 px line to suggest that
	// pulling it is different from pulling the middle.
	motion := gtk.NewEventControllerMotion()
	motion.ConnectMotion(func(x, y float64) {
		name := "default"
		if ed.fxArm != "" {
			name = "crosshair"
		} else if gr := grabAt(x, y); gr != nil {
			name = fxCursorName(gr.horiz, gr.vert, gr.left, gr.top, gr.inside)
		}
		if name != ed.fxCursor {
			ed.fxCursor = name
			area.SetCursor(gdk.NewCursorFromName(name, nil))
		}
	})
	area.AddController(motion)

	// There is deliberately no right button here. Everything the picture can
	// be asked is asked with the left one -- a press picks a box up, Esc puts
	// it down, and ✎ Edit opens the numbers of whatever is held.

	return over
}
