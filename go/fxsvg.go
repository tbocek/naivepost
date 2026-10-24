package main

// A drawing over the picture: a user's SVG laid over the running video for a
// few seconds. It is a text effect with ink instead of words (overFx,
// fxtext.go) -- same bar, fades and OUTPUT-fraction box -- fitted by scaling,
// centred on the axis the fit left short. Rasterized at the box's size:
// librsvg renders at the document's declared size unless told (-width/-height
// on the input), so both preview and render pass it.

import (
	"bytes"
	"fmt"
	"image"
	"image/png"
	"math"
	"os/exec"
	"path/filepath"
	"strings"

	"github.com/diamondburned/gotk4/pkg/cairo"
	"github.com/diamondburned/gotk4/pkg/glib/v2"
	"github.com/diamondburned/gotk4/pkg/gtk/v4"
	"github.com/diamondburned/gotk4/pkg/pango"
)

// fxSvgDefault is the box a drawing gets when it is placed without one being
// drawn: the middle of the frame, a bit over half of it. The middle rather
// than the lower third a caption gets -- a drawing is as often the subject as
// it is a decoration, and the middle is the one place that is not a guess
// about which.
var fxSvgDefault = fxBox{cx: 0.5, cy: 0.5, wf: 0.6, hf: 0.6}

// svgPreviewPx is how big the preview's copy is rendered. Bigger than any box
// on a preview widget, so the drawing is never blown up on screen, and small
// enough that the raster costs nothing to keep.
const svgPreviewPx = 512

// svgBase is a drawing's file as something short enough to put on a label.
func svgBase(path string) string {
	if strings.TrimSpace(path) == "" {
		return "(no file)"
	}
	return filepath.Base(path)
}

// svgName is svgBase for an effect.
func svgName(f cutFx) string { return svgBase(f.Src) }

// svgFitPx is the box a drawing goes in on a w×h finished frame, in whole
// pixels. The one place either side works the geometry out, so the preview and
// the render put the drawing in the same place -- the bargain fitText strikes
// for the words. ok is false when the frame's size is not known, which is the
// only case there is nothing to fit against.
func svgFitPx(f cutFx, w, h int) (x, y, bw, bh int, ok bool) {
	if w <= 0 || h <= 0 {
		return 0, 0, 0, 0, false
	}
	fx, fy, fw, fh := f.textBox().px(float64(w), float64(h))
	bw, bh = int(math.Round(fw)), int(math.Round(fh))
	if bw < 1 || bh < 1 {
		return 0, 0, 0, 0, false
	}
	return int(math.Round(fx)), int(math.Round(fy)), bw, bh, true
}

// svgRaster is a drawing as PNG bytes, rendered w×h if a size is asked for.
// Its own ffmpeg call rather than ffmpegPNG's, for the two things a drawing
// needs and a frame of footage does not: the librsvg options that set the size
// the vector is rendered at, and an output kept in rgba so a transparent
// background stays transparent instead of coming out black.
func svgRaster(path string, w, h int) ([]byte, error) {
	args := []string{"-v", "error", "-nostdin"}
	if w > 0 && h > 0 {
		args = append(args, "-width", fmt.Sprint(w), "-height", fmt.Sprint(h), "-keep_ar", "1")
	}
	args = append(args, "-i", path, "-frames:v", "1",
		"-pix_fmt", "rgba", "-f", "image2", "-c:v", "png", "-")
	cmd := exec.Command(ffTool("ffmpeg"), args...)
	var out, errb bytes.Buffer
	cmd.Stdout, cmd.Stderr = &out, &errb
	if err := cmd.Run(); err != nil {
		tail := strings.TrimSpace(errb.String())
		if len(tail) > 200 {
			tail = tail[len(tail)-200:]
		}
		return nil, fmt.Errorf("ffmpeg: %w: %s", err, tail)
	}
	if out.Len() == 0 {
		return nil, fmt.Errorf("ffmpeg drew nothing")
	}
	return out.Bytes(), nil
}

// fxSVG is one drawing rasterized for the preview. One per file rather than
// one per effect: the same logo placed six times is one raster, and a drawing
// that cannot be read says so once and is then left alone.
type fxSVG struct {
	surf   *cairo.Surface
	busy   bool
	failed bool
}

// svgSurface is the drawing at path ready to paint, or nil while it is being
// made -- and nil for good if it cannot be. The render is asked for in the
// background and the picture is redrawn when it lands, the same bargain the
// stop effect's still makes (renderStill).
func (ed *cutEditor) svgSurface(path string) *cairo.Surface {
	if strings.TrimSpace(path) == "" {
		return nil
	}
	if ed.svgs == nil {
		ed.svgs = map[string]*fxSVG{}
	}
	c := ed.svgs[path]
	if c == nil {
		c = &fxSVG{}
		ed.svgs[path] = c
	}
	if c.surf != nil || c.busy || c.failed {
		return c.surf
	}
	c.busy = true
	a := ed.a
	go func() {
		raw, err := svgRaster(path, svgPreviewPx, svgPreviewPx)
		var img image.Image
		if err == nil {
			img, err = png.Decode(bytes.NewReader(raw))
		}
		glib.IdleAdd(func() {
			c.busy = false
			if err != nil {
				c.failed = true
				if a != nil {
					a.logf(">>> the drawing %s cannot be shown: %v", svgBase(path), err)
				}
				return
			}
			c.surf = cairo.CreateSurfaceFromImage(img)
			if ed.fxArea != nil {
				ed.fxArea.QueueDraw()
			}
		})
	}()
	return nil
}

// drawSVG paints one drawing into the box (x, y, w, h) on the preview, fitted
// exactly as textChain fits it in the render: scaled to sit inside, its own
// shape kept, centred on the axis the fit left short.
func (ed *cutEditor) drawSVG(cr *cairo.Context, f cutFx, x, y, w, h, alpha float64) {
	if alpha <= 0.01 || w <= 0 || h <= 0 {
		return
	}
	surf := ed.svgSurface(f.Src)
	if surf == nil {
		return
	}
	sw, sh := float64(surf.Width()), float64(surf.Height())
	if sw <= 0 || sh <= 0 {
		return
	}
	s := math.Min(w/sw, h/sh)
	cr.Save()
	cr.Translate(x+(w-sw*s)/2, y+(h-sh*s)/2)
	cr.Scale(s, s)
	cr.SetSourceSurface(surf, 0, 0)
	cr.PaintWithAlpha(alpha)
	cr.Restore()
}

// ---- placing one ------------------------------------------------------------

// chooseSVG asks for the file. Its own little function because a drawing is
// chosen twice: once when it is placed, and again whenever the dialog's
// Choose… swaps one drawing for another.
func (a *App) chooseSVG(ok func(string)) {
	a.pickFile("Choose a drawing to lay over the video", a.insertDir(), extFilter("SVG drawing", "svg"), ok)
}

// svgClicked is the toolbar's SVG entry. The file is asked for FIRST and the
// drag armed after it: a drawing cannot be placed without one, and the box
// drawn on the picture means nothing until you know what is going in it.
func (a *App) svgClicked() {
	ed := a.ed
	if ed.player == nil || ed.playVideo == nil || !ed.hasPlay {
		a.setStatus("click a track first — the drawing needs a moment to appear at")
		return
	}
	a.chooseSVG(func(path string) {
		ed.fxSrc = path
		ed.fxArm = "" // never a toggle-off: a file was just chosen on purpose
		ed.armFx("svg")
	})
}

// askSvgParams asks a drawing's four things: which file, how long it is on
// screen, and the two fades. The same dialog as a title's, one row apart --
// the words are a file instead.
func (a *App) askSvgParams(f cutFx, isNew bool, ok func(cutFx)) {
	// the form applies as it is typed, and the first answer it gives places
	// the drawing or opens the undo step (fxWin, fxLiveOk)
	ok = a.fxLiveOk(ok)
	live := &fxLive{}
	if f.Dur <= 0 {
		f.Dur = 3
	}
	name := gtk.NewLabel(svgName(f))
	name.SetXAlign(0)
	name.SetHExpand(true)
	name.SetEllipsize(pango.EllipsizeMiddle)
	name.SetTooltipText(f.Src)
	pick := gtk.NewButtonWithLabel("Choose…")
	pick.ConnectClicked(func() {
		a.chooseSVG(func(path string) {
			f.Src = path
			name.SetText(svgBase(path))
			name.SetTooltipText(path)
			live.touch() // the file is an answer like any other
		})
	})
	fRow := gtk.NewBox(gtk.OrientationHorizontal, 8)
	fRow.Append(name)
	fRow.Append(pick)
	dRow, d := fxNumRow("Length (s)",
		"how long the drawing stays up altogether, fades included — the same "+
			"seconds its bar covers on the lane", f.Dur, live)
	iRow, i := fxNumRow("Fade in (s)",
		"how long it takes to appear: 0 cuts it straight on", f.Trans, live)
	oRow, o := fxNumRow("Fade out (s)",
		"how long it takes to go again: 0 cuts it straight off", f.Tout, live)
	eRow, ec := fxEaseRow(f, live)
	a.fxWin(fmt.Sprintf("SVG at %s", mmss(f.T)), isNew, live,
		[]gtk.Widgetter{fRow, fxLine(dRow), fxLine(iRow, oRow, eRow)}, func() {
			f.Dur = math.Max(0.3, fxNumOf(d, f.Dur))
			f.Trans = fxNumOf(i, f.Trans)
			f.Tout = fxNumOf(o, f.Tout)
			f.Ease = fxEaseOf(ec.Selected())
			clampFades(&f)
			ok(f)
		})
}
