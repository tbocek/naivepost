package main

// The thumbnail is the video's own shape (parseAspect, as produce), and the
// crop box says which part of the base frame survives. The box is the biggest
// rectangle of the output's shape that fits (fullFill) and cannot be resized;
// where it sits is the whole question, answered by dragging. Only the base
// wears one -- references are named in a sentence, not composed.

import (
	"bytes"
	"encoding/base64"
	"fmt"
	"image"
	"image/color"
	"image/png"
	"math"
	"os"

	"github.com/diamondburned/gotk4/pkg/cairo"
	"github.com/diamondburned/gotk4/pkg/gtk/v4"
)

// pubLongSide is the thumbnail's longer edge in pixels. 1280 because that is
// what YouTube asks for, and because a 16:9 cut then comes out at exactly the
// 1280x720 this page always drew.
const pubLongSide = 1280

// pubBox is the frame the thumbnail is drawn at for a cut of this aspect.
// "source" or "" -- no aspect chosen -- keeps the widescreen default: the
// footage's own shape is not known here, and 16:9 is what a recording is.
// Both sides are even, because half the image models quietly round and the
// half that do not refuse.
func pubBox(aspect string) (int, int) {
	a := parseAspect(aspect)
	if a <= 0 {
		a = 16.0 / 9
	}
	w, h := float64(pubLongSide), float64(pubLongSide)
	if a >= 1 {
		h = w / a
	} else {
		w = h * a
	}
	return int(math.Round(w/2)) * 2, int(math.Round(h/2)) * 2
}

// pubCropAt is the crop box on a source frame of aspect srcA for a thumbnail
// of aspect outA, centred on cx,cy: fullFill's size, the centre pulled back
// inside the frame. hf and the width it implies are fractions of the SOURCE
// frame, like a camera rectangle (cutFx).
func pubCropAt(srcA, outA, cx, cy float64) fxRect {
	r := fullFill(srcA, outA)
	wf := pubCropW(r.hf, srcA, outA)
	// An axis with no slack needs no separate case: fullFill never returns
	// more than the whole frame, so when the box fills one the clamp's two
	// bounds meet at the middle and the drag on that axis simply does nothing.
	r.cx = math.Min(math.Max(cx, wf/2), 1-wf/2)
	r.cy = math.Min(math.Max(cy, r.hf/2), 1-r.hf/2)
	return r
}

// pubCropW is the crop box's width as a fraction of the source frame: hf frame
// heights tall and outA of those wide, said in widths.
func pubCropW(hf, srcA, outA float64) float64 {
	if srcA <= 0 || outA <= 0 {
		return 1
	}
	return math.Min(1, hf*outA/srcA)
}

// pubWholeFrame is whether a crop takes the whole picture -- the case where
// the recording and the thumbnail are already the same shape. Worth knowing:
// re-encoding a frame that is not being cropped would cost a JPEG its quality
// for nothing.
func pubWholeFrame(r fxRect, srcA, outA float64) bool {
	return r.hf >= 0.999 && pubCropW(r.hf, srcA, outA) >= 0.999
}

// imageAspect is an image file's width over height, or 0 if it cannot be read.
// DecodeConfig rather than Decode: the header carries the size and a thumbnail
// row of eight 4K frames is not worth eight full decodes.
func imageAspect(path string) float64 {
	f, err := os.Open(path)
	if err != nil {
		return 0
	}
	defer f.Close()
	c, _, err := image.DecodeConfig(f)
	if err != nil || c.Width <= 0 || c.Height <= 0 {
		return 0
	}
	return float64(c.Width) / float64(c.Height)
}

// pubCropRefImage is sdRefImage with the box applied: the base frame cut down
// to what the crop keeps, handed over as a PNG data URL. A crop that keeps the
// whole picture goes through sdRefImage untouched.
func pubCropRefImage(path string, r fxRect, srcA, outA float64) (string, error) {
	if srcA <= 0 || outA <= 0 || pubWholeFrame(r, srcA, outA) {
		return sdRefImage(path)
	}
	sub, err := pubCropImage(path, r, srcA, outA)
	if err != nil {
		return "", err
	}
	if sub == nil {
		return sdRefImage(path) // a decoder with no SubImage: the whole frame
	}
	var buf bytes.Buffer
	if err := png.Encode(&buf, sub); err != nil {
		return "", err
	}
	return "data:image/png;base64," + base64.StdEncoding.EncodeToString(buf.Bytes()), nil
}

// pubWriteCropped writes the same crop as a PNG for the picture used as the
// thumbnail as it stands (pubSlot.useAsThumbnail). Always ENCODED, never
// copied through: the frames are JPEGs, and a JPEG named .png is what cairo
// refuses to read.
func pubWriteCropped(path string, r fxRect, srcA, outA float64, w, h int, out string) error {
	img, err := pubCropImage(path, r, srcA, outA)
	if err != nil {
		return err
	}
	if img == nil {
		if img, err = pubDecode(path); err != nil {
			return err
		}
	}
	img = pubFit(img, w, h)
	f, err := os.Create(out)
	if err != nil {
		return err
	}
	defer f.Close()
	return png.Encode(f, img)
}

// pubFit scales an image down to exactly w by h; the crop has already made the
// shape right. A chosen frame and a drawn one must be the same size -- a 3840
// wide capture frame is a 14 MB PNG YouTube refuses. Area average (always a
// downscale, often 3-4×); smaller images are returned as they are.
func pubFit(src image.Image, w, h int) image.Image {
	b := src.Bounds()
	if w <= 0 || h <= 0 || b.Dx() <= w || b.Dy() <= h {
		return src
	}
	dst := image.NewRGBA(image.Rect(0, 0, w, h))
	for y := 0; y < h; y++ {
		y0 := b.Min.Y + y*b.Dy()/h
		y1 := b.Min.Y + (y+1)*b.Dy()/h
		for x := 0; x < w; x++ {
			x0 := b.Min.X + x*b.Dx()/w
			x1 := b.Min.X + (x+1)*b.Dx()/w
			var rs, gs, bs, as, n uint64
			for sy := y0; sy < y1; sy++ {
				for sx := x0; sx < x1; sx++ {
					cr, cg, cb, ca := src.At(sx, sy).RGBA()
					rs, gs, bs, as, n = rs+uint64(cr), gs+uint64(cg), bs+uint64(cb), as+uint64(ca), n+1
				}
			}
			if n == 0 {
				continue
			}
			dst.Set(x, y, color.RGBA64{uint16(rs / n), uint16(gs / n), uint16(bs / n), uint16(as / n)})
		}
	}
	return dst
}

// pubDecode is one image file, whatever it is encoded as.
func pubDecode(path string) (image.Image, error) {
	f, err := os.Open(path)
	if err != nil {
		return nil, err
	}
	defer f.Close()
	img, _, err := image.Decode(f)
	return img, err
}

// pubCropImage is the crop itself: the part of the file the box keeps, or nil
// where the whole frame is kept and the caller should use the file as it is.
func pubCropImage(path string, r fxRect, srcA, outA float64) (image.Image, error) {
	if srcA <= 0 || outA <= 0 || pubWholeFrame(r, srcA, outA) {
		return nil, nil
	}
	f, err := os.Open(path)
	if err != nil {
		return nil, err
	}
	defer f.Close()
	src, _, err := image.Decode(f)
	if err != nil {
		return nil, err
	}
	b := src.Bounds()
	wf := pubCropW(r.hf, srcA, outA)
	x0 := b.Min.X + int(math.Round((r.cx-wf/2)*float64(b.Dx())))
	y0 := b.Min.Y + int(math.Round((r.cy-r.hf/2)*float64(b.Dy())))
	x1 := x0 + int(math.Round(wf*float64(b.Dx())))
	y1 := y0 + int(math.Round(r.hf*float64(b.Dy())))
	cut := image.Rect(x0, y0, x1, y1).Intersect(b)
	if cut.Empty() {
		return nil, fmt.Errorf("the crop box keeps none of %s", path)
	}
	// SubImage is what every std image type offers and none of them promise;
	// the type assertion is the check, and a decoder that does not have it
	// falls back to the whole frame rather than to an error nobody can act on.
	sub, ok := src.(interface {
		SubImage(image.Rectangle) image.Image
	})
	if !ok {
		return nil, nil
	}
	return sub.SubImage(cut), nil
}

// ---- the box on the picture -------------------------------------------------

// cropOverlay is the base image with its crop box laid over it, draggable.
// A reference, or a base whose shape already matches, comes back plain -- a
// control that cannot change anything invites the question of what it does.
// The box is drawn by dimming everything OUTSIDE it.
func (s *pubSlot) cropOverlay(pic *gtk.Picture) gtk.Widgetter {
	p := s.p
	outA := 0.0
	if w, h := pubBox(p.aspect); h > 0 {
		outA = float64(w) / float64(h)
	}
	// read once per rebuild, not once per frame: this opens the file
	srcA := imageAspect(s.path)
	if s.i != 0 || srcA <= 0 || outA <= 0 || pubWholeFrame(pubCropAt(srcA, outA, 0.5, 0.5), srcA, outA) {
		return pic
	}
	// through pubSettings so the "never dragged means the middle" default is
	// written down once, in the place the project file reads it from
	rect := func() fxRect { return pubSettings{Crop: p.crop}.cropRect(srcA, outA) }

	area := gtk.NewDrawingArea()
	area.SetTooltipText("The part of this frame the thumbnail keeps — the video's own shape. " +
		"Drag it over what matters; everything dimmed is cut away before the image model " +
		"is given the picture.")
	area.SetDrawFunc(func(_ *gtk.DrawingArea, cr *cairo.Context, w, h int) {
		ox, oy, dw, dh := fxDisp(float64(w), float64(h), srcA)
		if dw <= 0 || dh <= 0 {
			return
		}
		r := rect()
		wf := pubCropW(r.hf, srcA, outA)
		bx, by := ox+(r.cx-wf/2)*dw, oy+(r.cy-r.hf/2)*dh
		bw, bh := wf*dw, r.hf*dh
		// the surround, as one path with the box cut out of it: filled even-odd
		// so the hole is a hole and not a second dark rectangle over the first
		cr.SetSourceRGBA(0, 0, 0, 0.55)
		cr.SetFillRule(cairo.FillRuleEvenOdd)
		cr.Rectangle(ox, oy, dw, dh)
		cr.Rectangle(bx, by, bw, bh)
		cr.Fill()
		cr.SetFillRule(cairo.FillRuleWinding)
		// and a hairline on the box itself, because a dimmed surround alone
		// disappears over footage that is already dark
		cr.SetSourceRGBA(1, 1, 1, 0.9)
		cr.SetLineWidth(1.5)
		cr.Rectangle(bx+0.75, by+0.75, bw-1.5, bh-1.5)
		cr.Stroke()
	})

	// the drag. The centre is taken at the press and moved by the pointer's
	// travel, not set to where the pointer is: grabbing the box near its edge
	// and having it jump so its middle is under the finger is the one thing
	// that makes a box like this feel broken.
	g := gtk.NewGestureDrag()
	var cx0, cy0 float64
	g.ConnectDragBegin(func(float64, float64) { r := rect(); cx0, cy0 = r.cx, r.cy })
	g.ConnectDragUpdate(func(dx, dy float64) {
		_, _, dw, dh := fxDisp(float64(area.AllocatedWidth()), float64(area.AllocatedHeight()), srcA)
		if dw <= 0 || dh <= 0 {
			return
		}
		r := pubCropAt(srcA, outA, cx0+dx/dw, cy0+dy/dh)
		p.crop = &pubPoint{X: r.cx, Y: r.cy}
		area.QueueDraw()
	})
	area.AddController(g)

	ov := gtk.NewOverlay()
	ov.SetChild(pic)
	ov.AddOverlay(area)
	return ov
}
