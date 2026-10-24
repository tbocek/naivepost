package main

// The finished picture, shared by Cut and Narrate: one type both pages hold, so
// a narration line is judged against the frame the render will make. It owns
// the three layers over the player -- the camera (a second picture on a
// GtkFixed, transformed so the camera window fills the output box), a stop's
// frozen frame on its own Fixed with the same transform, and a drawing area for
// the black around the frame and the titles. What differs per page comes
// through fxPage. cut_fxview.go wraps this with the editor's gestures and grabs.

import (
	"fmt"
	"math"
	"time"

	"github.com/diamondburned/gotk4/pkg/cairo"
	"github.com/diamondburned/gotk4/pkg/gdk/v4"
	"github.com/diamondburned/gotk4/pkg/glib/v2"
	"github.com/diamondburned/gotk4/pkg/gtk/v4"
)

// fxPage is the page under an fxScreen: the four questions the layers cannot
// answer for themselves.
type fxPage interface {
	// fxCut is the Cut editor, which owns the one cut and the one set of
	// effects however many pages are showing them. A preview that read its own
	// copy would be previewing a different video.
	fxCut() *cutEditor
	fxPlayer() *Player // the player the layers sit over
	// fxAt is where this page's playhead is, in session time, as last read
	// from that player. The smoothing on top of it is the screen's own
	// (livePlayhead), and is the same smoothing on both pages.
	fxAt() float64
	// fxSrcSize is the recording's pixel size when the paintable will not say
	// -- before the first frame arrives, mostly. Never zero.
	fxSrcSize() (float64, float64)
	// fxCamOK is whether the camera layer may be up at all, over and above
	// there being a camera to show. An editor says no while something is being
	// aimed by hand: the outline then has to be drawn against everything the
	// camera COULD see, not against what it has been cropped to. A page that
	// only watches says yes whenever it has footage.
	fxCamOK() bool
}

// fxScreen is those three layers and the clock they are drawn on. Embedded in
// cutEditor, held by the narrator.
type fxScreen struct {
	page fxPage
	// the live-zoom layer: the same video again, transformed so the camera's
	// window fills the output box
	fxZoom    *gtk.Fixed
	fxZoomPic *gtk.Picture
	// the stop effect's still, on its own Fixed so the camera can move over
	// it, and the frame it is showing (cut_fxstill.go)
	fxStillBox *gtk.Fixed
	fxStillPic *gtk.Picture
	fstill     *fxStill
	fxArea     *gtk.DrawingArea // the mask and the titles, over both

	// the last playhead read off the player, and the wall clock when it was
	// read. The camera runs on these rather than on the playhead itself; see
	// livePlayhead.
	posT  float64
	posAt time.Time
	// the highest time livePlayhead has answered since the clock was last
	// re-based: while the stream plays, the live clock never runs backward
	liveMax float64
}

// fx is the effects on the session, which live on the Cut editor whichever
// page is showing them: there is one cut and one set of effects, and a preview
// that read its own copy would be previewing a different video.
func (s *fxScreen) fx() []cutFx {
	if ed := s.ed(); ed != nil {
		return ed.fx
	}
	return nil
}

// ed is the Cut editor, which owns the cut, the effects and the SVG cache.
func (s *fxScreen) ed() *cutEditor { return s.page.fxCut() }

// ---- geometry ---------------------------------------------------------------

// srcSize is the pixel size of the recording under the preview, for turning
// normalized camera rectangles into places on the screen.
func (s *fxScreen) srcSize() (float64, float64) { return s.page.fxSrcSize() }

// liveSize is the size the zoom layer's picture is actually drawn at: the
// paintable's own pixels when it knows them, the probe's otherwise.
func (s *fxScreen) liveSize() (float64, float64) {
	if p := s.page.fxPlayer(); p != nil {
		if pw, ph := p.video.IntrinsicWidth(), p.video.IntrinsicHeight(); pw > 0 && ph > 0 {
			return float64(pw), float64(ph)
		}
	}
	return s.srcSize()
}

// outAspect is the shape of the finished video: the chosen aspect, or the
// footage's own when none is chosen.
func (s *fxScreen) outAspect() float64 {
	if ed := s.ed(); ed != nil {
		if a := parseAspect(ed.aspect); a > 0 {
			return a
		}
	}
	sw, sh := s.srcSize()
	return sw / sh
}

// livePreview is whether the zoomed live layer is the thing on screen. While
// it is, Cut's overlay draws only the black around the output box and offers
// nothing to grab: framing is done on a paused picture, where the outline
// shows everything the camera COULD see.
func (s *fxScreen) livePreview() bool {
	return s.fxZoom != nil && s.fxZoom.Visible()
}

// area is the widget's size, which every layer is measured against.
func (s *fxScreen) size() (W, H float64) {
	if s.fxArea == nil {
		return 0, 0
	}
	return float64(s.fxArea.AllocatedWidth()), float64(s.fxArea.AllocatedHeight())
}

// ---- the clock --------------------------------------------------------------

// livePlayhead adds the time since the last playTick read: a camera glide
// sampled at 10 Hz is ten jumps, while the render (zoompan per frame) is
// smooth. Arithmetic and clamps: liveClock.
func (s *fxScreen) livePlayhead() float64 {
	rate, playing := 1.0, false
	if p := s.page.fxPlayer(); p != nil && p.playing {
		rate, playing = p.Rate(), true
	}
	now, mark := liveClock(s.page.fxAt(), s.posT, s.posAt, s.liveMax, rate, playing)
	s.liveMax = mark
	return now
}

// reLive re-arms the live clock on t: the base position, the wall clock it was
// read at, and the high-water mark move together. A seek inside the open file
// does not stop playback, so the next overlay read would otherwise extrapolate
// from the pre-jump position, exceed the freshly lowered mark, and pin the
// clock in the future.
func (s *fxScreen) reLive(t float64) {
	s.liveMax, s.posT, s.posAt = t, t, time.Now()
}

// ---- the layers -------------------------------------------------------------

// buildLayers puts the three layers over pic and hands back the overlay to
// hang on the page. Both pages get the same stack in the same order, because
// the order is what makes the picture right: the camera under the still, both
// under the mask that blacks out what the render will not have.
func (s *fxScreen) buildLayers(page fxPage, pic gtk.Widgetter, video gdk.Paintabler) *gtk.Overlay {
	s.page = page
	over := gtk.NewOverlay()
	over.SetChild(pic)

	s.fxZoom = gtk.NewFixed()
	s.fxZoom.SetOverflow(gtk.OverflowHidden)
	s.fxZoom.SetCanTarget(false)
	s.fxZoomPic = gtk.NewPicture()
	s.fxZoomPic.SetPaintable(video)
	s.fxZoom.Put(s.fxZoomPic, 0, 0)
	s.fxZoom.SetVisible(false)
	over.AddOverlay(s.fxZoom)

	// A stop is a still the CAMERA STILL MOVES OVER. The render has always done
	// it that way -- encodeClip splices the stills into the chain before the
	// camera filters, so a zoom crops the frozen frame exactly as it crops the
	// footage running under it -- but hung straight on the overlay, as this
	// once was, the still was the one thing on the picture the camera could
	// not reach. A zoom during a stop then did nothing you could see.
	s.fxStillBox = gtk.NewFixed()
	s.fxStillBox.SetOverflow(gtk.OverflowHidden)
	s.fxStillBox.SetCanTarget(false)
	s.fxStillPic = gtk.NewPicture()
	s.fxStillPic.SetCanTarget(false)
	s.fxStillBox.Put(s.fxStillPic, 0, 0)
	s.fxStillBox.SetVisible(false)
	over.AddOverlay(s.fxStillBox)

	s.fxArea = gtk.NewDrawingArea()
	s.fxArea.SetCanTarget(false) // passive until a page arms something on it
	over.AddOverlay(s.fxArea)
	return over
}

// syncPreviewZoom puts the real camera on the preview whenever the cut has one
// and nobody is framing by hand: the same video again, transformed so the
// camera's window fills the output box. Independent of whether the stream is
// running -- paused must show the framing too.
func (s *fxScreen) syncPreviewZoom() {
	s.syncCamLayer()
	s.fitStill() // whatever the camera just did, the still does too
}

// syncCamLayer is syncPreviewZoom's own half: the live-zoom layer itself.
func (s *fxScreen) syncCamLayer() {
	if s.fxZoom == nil {
		return
	}
	p := s.page.fxPlayer()
	ed := s.ed()
	on := p != nil && !p.still && ed != nil &&
		fxHasCamera(ed.aspect, ed.fx) && s.page.fxCamOK()
	if !on {
		s.fxZoom.SetVisible(false)
		return
	}
	W, H := s.size()
	sw, sh := s.liveSize()
	// the picture is pinned to its own pixel size, so the transform below is
	// the whole mapping (GtkFixed hands children their natural size)
	if rw, rh := s.fxZoomPic.SizeRequest(); rw != int(sw) || rh != int(sh) {
		s.fxZoomPic.SetSizeRequest(int(sw), int(sh))
	}
	// livePlayhead, not the page's own: the transform is asked for once per
	// display frame (the tick callback) and the playhead only moves ten times
	// a second, so a glide sampled at the playhead would step
	sc, tx, ty, ok := fxLiveFit(W, H, sw, sh, s.outAspect(), s.fx(), s.livePlayhead())
	if !ok {
		s.fxZoom.SetVisible(false)
		return
	}
	s.fxZoom.SetChildTransform(s.fxZoomPic, zoomTransform(sc, tx, ty))
	s.fxZoom.SetVisible(true)
}

// fitStill puts a stop's frozen frame on the same mapping as the footage it is
// standing over: the camera's, when the live-zoom layer is up, and the plain
// aspect fit GtkPicture does when it is down -- an armed or held zoom, or no
// camera on this session at all. Either way the still and the picture beneath
// it are the same size in the same place, which is the whole of "the pause
// stops the video and nothing else".
func (s *fxScreen) fitStill() {
	pic, box := s.fxStillPic, s.fxStillBox
	if pic == nil || box == nil || s.fxArea == nil {
		return
	}
	W, H := s.size()
	sw, sh := s.liveSize()
	if W <= 0 || H <= 0 || sw <= 0 || sh <= 0 {
		return
	}
	// pinned to its own pixel size, so the transform below is the whole
	// mapping -- a Fixed hands its children their natural size
	if rw, rh := pic.SizeRequest(); rw != int(sw) || rh != int(sh) {
		pic.SetSizeRequest(int(sw), int(sh))
	}
	outA := s.outAspect()
	sc, tx, ty := stillFit(W, H, sw, sh, outA, s.livePreview(),
		fxRectAt(s.fx(), s.livePlayhead(), sw/sh, outA))
	if sc <= 0 || math.IsNaN(sc) || math.IsInf(sc, 0) {
		return
	}
	box.SetChildTransform(pic, zoomTransform(sc, tx, ty))
}

// syncFxStill settles the still layer for wherever the playhead is now.
// Called from every path that moves the playhead -- a tick, a click, a frame
// step, an edit dropping the effect.
func (s *fxScreen) syncFxStill() {
	pic, box := s.fxStillPic, s.fxStillBox
	p := s.page.fxPlayer()
	if pic == nil || box == nil || p == nil {
		return
	}
	at := s.page.fxAt()
	f := freezeNow(s.fx(), at)
	// a card owns the whole picture while it is up; the still yields to it
	if f == nil || p.still {
		if s.fstill != nil {
			s.fstill.shown = false
		}
		box.SetVisible(false)
		return
	}
	st := s.fstill
	if st == nil || st.t != f.T {
		st = &fxStill{t: f.T}
		s.fstill = st
	}
	if st.tex == nil {
		s.renderStill(st)
		box.SetVisible(false) // nothing to put up yet; the render will call back
		return
	}
	if !st.shown {
		pic.SetPaintable(st.tex)
		st.shown = true
	}
	// the fades, evaluated the way the render's fade filters will (textAlpha
	// reads the same Trans/Tout bargain for a freeze as for a title)
	box.SetOpacity(textAlpha(*f, at))
	s.fitStill() // under whatever camera is over the footage right now
	box.SetVisible(true)
}

// renderStill draws the stop's frame in the background and puts it up when it
// arrives -- by asking syncFxStill again rather than by showing it directly,
// because by then the playhead may have left the bar.
func (s *fxScreen) renderStill(st *fxStill) {
	ed := s.ed()
	if st.busy || st.failed || ed == nil {
		return
	}
	// cutVideoAt, not videoAt: this frame is the one the RENDER freezes, so it
	// comes off the camera the scene names. Asking videoAt meant a click that
	// pointed the preview at another row also changed which camera the frozen
	// frame was cut from -- the still on screen then was a frame the finished
	// video never contains.
	v := ed.cutVideoAt(st.t)
	if v == nil {
		st.failed = true // a stop in a gap has no frame to stand on
		return
	}
	st.busy = true
	a, local, path := ed.a, v.at(st.t), v.path
	go func() {
		png, err := ffmpegPNG("-ss", fmt.Sprintf("%.3f", local), "-i", path)
		glib.IdleAdd(func() {
			st.busy = false
			if s.fstill != st {
				return // a different stop owns the layer now
			}
			var tex *gdk.Texture
			if err == nil {
				tex, err = gdk.NewTextureFromBytes(glib.NewBytes(png))
			}
			if err != nil {
				st.failed = true
				if a != nil {
					a.logf(">>> the stop frame at %s cannot be shown in the preview: %v", mmss(st.t), err)
				}
				return
			}
			st.tex = tex
			s.syncFxStill()
		})
	}()
}

// ---- what the drawing area paints -------------------------------------------

// paintLive is the finished frame over a running camera: the black around
// everything the render will not have, and the titles on the frame that is
// left. Reports whether it painted -- a fit that cannot be made is a frame
// nobody can draw on either.
func (s *fxScreen) paintLive(cr *cairo.Context, w, h int) bool {
	lw, lh := s.liveSize()
	outA := s.outAspect()
	W, H := float64(w), float64(h)
	now := s.livePlayhead() // the same clock the layer under this runs on
	fx := s.fx()
	sc, tx, ty, ok := fxLiveFit(W, H, lw, lh, outA, fx, now)
	if !ok {
		return false
	}
	fxMaskLive(cr, W, H, lw, lh, outA, sc, tx, ty)
	// the words go on while it plays, faded exactly as they will be: a title
	// is a thing you watch, not a thing you inspect paused
	ox, oy, ow, oh := fxDisp(W, H, outA)
	s.ed().drawFxOverlaysAt(cr, fx, now, ox, oy, ow, oh)
	return true
}

// paintFlat is the same titles with no camera under them: the picture below is
// already the finished frame, letterboxed into the widget by GtkPicture, so
// the words go where GtkPicture put it and nothing is masked. Masking here
// would be painting the widget's own background onto itself at best, and
// hiding footage the render keeps at worst.
func (s *fxScreen) paintFlat(cr *cairo.Context, w, h int) {
	sw, sh := s.liveSize()
	if sw <= 0 || sh <= 0 {
		return
	}
	ox, oy, ow, oh := fxDisp(float64(w), float64(h), sw/sh)
	s.ed().drawFxOverlaysAt(cr, s.fx(), s.livePlayhead(), ox, oy, ow, oh)
}
