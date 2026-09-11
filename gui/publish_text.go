package main

// Words printed on the thumbnail, locally, AFTER the draw -- not lettered by
// the image model. Same layout as a Text effect (fitText, drawFxText). Two
// kinds, the same object: the TITLE in a draggable band (pubTitleBox; the
// model is told to keep that part calm, pubTitleWhere) whose words are the
// YouTube title, and any number of MARKED texts drawn on the result. Every box
// is outlined, dragged by its border, moved by its middle and wears a ✎.
// thumbnail-plain.png is the picture without words; thumbnail.png with.
// Rewording re-prints from the plain copy (recomposite) -- no model, no run.

import (
	"math"
	"os"
	"strings"

	"github.com/diamondburned/gotk4/pkg/cairo"
	"github.com/diamondburned/gotk4/pkg/gdk/v4"
	"github.com/diamondburned/gotk4/pkg/gtk/v4"
)

// pubText is one block of marked words: a box in fractions of the finished
// thumbnail, and the text fitted into it. The same four numbers a text
// effect's box is (fxBox), stored with names because this goes in the project
// file.
type pubText struct {
	Cx   float64 `json:"cx"`
	Cy   float64 `json:"cy"`
	Wf   float64 `json:"wf"`
	Hf   float64 `json:"hf"`
	Text string  `json:"text"`
}

// box is the pubText as the drawing code speaks it, kept on the frame.
func (t pubText) box() fxBox { return fxBox{cx: t.Cx, cy: t.Cy, wf: t.Wf, hf: t.Hf}.clamp() }

// pubTitleBox is where the title goes before anybody moves it: the UPPER HALF
// of the picture, which is what the image model is asked to leave calm
// (pubNoLettering). It moves; the instruction says where it is
// (pubTitleWhere).
//
// Half and not the band it was: a thumbnail title is four to seven words read
// at the size of a phone's sidebar, and a band a sixth of the picture tall
// sets those words small enough to be a caption. Given the half, fitText uses
// as much of it as the words need and leaves the rest -- the box is a ceiling,
// not a fill.
var pubTitleBox = fxBox{cx: 0.5, cy: 0.25, wf: 0.94, hf: 0.5}

// printedTitle is what the picture carries: its own line (ThumbTitle), which
// starts as the video's title and is its own from then on. st.Title is what
// goes to YouTube, and the two stop being the same sentence the moment either
// is edited.
func (st pubSettings) printedTitle() string { return st.ThumbTitle }

// titleBox is where THIS project's title goes: the band it was dragged to, or
// the default one.
func (st pubSettings) titleBox() fxBox {
	if st.TitleBox == nil {
		return pubTitleBox
	}
	return st.TitleBox.box()
}

// pubTitleWhere is the third of the picture the title band sits in, for the
// sentence the image model is asked to keep clear (pubNoLettering). Named
// rather than measured, because "keep the upper part calm" is an instruction a
// model follows and "keep 0.14 to 0.23 calm" is not.
func pubTitleWhere(b fxBox) string {
	switch {
	case b.cy < 1.0/3:
		return "upper"
	case b.cy > 2.0/3:
		return "lower"
	}
	return "middle"
}

// drawPubTexts prints the words onto the plain picture and writes the result:
// every marked text in its box, then the title across the top, drawn exactly
// as the video's Text effects are (fitText + drawFxText, so the preview, the
// render and the thumbnail cannot disagree about how words look). With
// nothing to print the plain bytes are copied through untouched -- a decode
// and re-encode of a picture nothing was drawn on is a lossless way to make
// the two files differ.
func drawPubTexts(plain, out string, texts []pubText, title string, tb fxBox) error {
	title = strings.TrimSpace(title)
	var on []pubText
	for _, t := range texts {
		if strings.TrimSpace(t.Text) != "" {
			on = append(on, t)
		}
	}
	if len(on) == 0 && title == "" {
		b, err := os.ReadFile(plain)
		if err != nil {
			return err
		}
		return os.WriteFile(out, b, 0o644)
	}
	surf, err := cairo.CreatePNGSurfaceFromPNG(plain)
	if err != nil {
		return err
	}
	cr := cairo.Create(surf)
	w, h := float64(surf.Width()), float64(surf.Height())
	for _, t := range on {
		x, y, bw, bh := t.box().px(w, h)
		drawFxText(cr, cutFx{Text: t.Text}, 1, x, y, bw, bh)
	}
	if title != "" {
		x, y, bw, bh := tb.px(w, h)
		drawFxText(cr, cutFx{Text: title}, 1, x, y, bw, bh)
	}
	return surf.WriteToPNG(out)
}

// recomposite re-prints the words onto the last drawn picture and shows the
// result. GTK thread; cheap enough to run on a saved edit (one PNG decode and
// encode). Not during a run -- the runner is writing these very files, and it
// prints the words itself from its own snapshot when it gets there.
func (p *publisher) recomposite() {
	if p.a.running {
		return
	}
	// the one printing, shared with the run (printPubWords): the words a
	// keystroke re-prints and the words a run prints have to be the same
	// words in the same boxes, and two copies of that arithmetic is two
	// places for a box to be read differently
	if err := p.a.printPubWords(p.snapshot()); err != nil {
		p.a.logf("thumbnail words: %v", err)
		return
	}
	p.showShot()
}

// setTexts replaces the marked texts and re-prints. Every path that changes
// them lands here, the way setFrames owns the image row: the overlay's boxes,
// the picture and the project must not disagree about what is printed.
func (p *publisher) setTexts(ts []pubText) {
	p.texts = ts
	if p.shotOver != nil {
		p.shotOver.QueueDraw()
	}
	p.recomposite()
}

// pubIconPx is the ✎ chip's edge in widget pixels: big enough to press,
// small enough not to cover the words it edits.
const pubIconPx = 22.0

// textOverlay is the result picture with the marking layer on it: the boxes
// the words were given, a ✎ on each, and the drag that marks a new one.
// The words themselves are not drawn here -- they are IN thumbnail.png
// (drawPubTexts), and painting them twice would double the edge.
func (p *publisher) textOverlay(pic *gtk.Picture) gtk.Widgetter {
	area := gtk.NewDrawingArea()
	p.shotOver = area
	area.SetTooltipText("Drag a box over the picture to put words on it — they are printed " +
		"to fill the box, like a Text effect in Cut. Drag a box's border to resize it or its " +
		"middle to move it, and press its ✎ to reword or remove it. The title is printed " +
		"across the top on its own.")

	// the rubber band, while a NEW box is being dragged out: widget pixels,
	// shared by the drag and the draw
	var band *[4]float64
	// and the box being moved or resized: which one, what was grabbed, where
	// it started, and where it is now. Live in widget pixels and committed to
	// fractions once the hand lets go -- a box that only moved when the drag
	// ended would be a box you place blind.
	var grab struct {
		on                             bool
		i                              int
		horiz, vert, left, top, inside bool
		px, py, ax, ay                 float64
		x0, y0, w0, h0                 float64
		cur                            [4]float64
	}
	cursor := ""

	// disp is where the shown picture sits in the widget, or ok=false when
	// there is nothing to mark on.
	disp := func(w, h float64) (ox, oy, dw, dh float64, ok bool) {
		if p.shotPath == "" || p.shotA <= 0 || w <= 0 || h <= 0 {
			return 0, 0, 0, 0, false
		}
		ox, oy, dw, dh = fxDisp(w, h, p.shotA)
		return ox, oy, dw, dh, dw > 0 && dh > 0
	}
	// The boxes are the marked texts, then the TITLE at index len(p.texts) --
	// a box like any other, but only while it HAS words and is being printed.
	// The words are the PICTURE's (pubSettings.ThumbTitle), not the entry's, so
	// Remove does not clear the video's title and retyping the title does not
	// put the box back.
	hasTitle := func() bool { return strings.TrimSpace(p.thumbTitle) != "" }
	nbox := func() int {
		if hasTitle() {
			return len(p.texts) + 1
		}
		return len(p.texts)
	}
	isTitle := func(i int) bool { return hasTitle() && i == len(p.texts) }
	boxOf := func(i int) fxBox {
		if isTitle(i) {
			return p.snapshot().titleBox()
		}
		return p.texts[i].box()
	}
	// rectPx is box i in widget pixels -- the live one while it is being
	// dragged, so everything drawn and hit-tested agrees with the hand.
	rectPx := func(i int, ox, oy, dw, dh float64) (x, y, w, h float64) {
		if grab.on && grab.i == i {
			return grab.cur[0], grab.cur[1], grab.cur[2], grab.cur[3]
		}
		bx, by, bw, bh := boxOf(i).px(dw, dh)
		return ox + bx, oy + by, bw, bh
	}
	// iconPx is text i's ✎ chip: the box's top-left corner, held inside it.
	iconPx := func(i int, ox, oy, dw, dh float64) (x, y float64) {
		bx, by, _, _ := rectPx(i, ox, oy, dw, dh)
		return bx + 2, by + 2
	}
	// snapLines is what a box lands on: the picture's edges and middle, the
	// title band, and every OTHER box's edges and middle. Skip is the box being
	// dragged.
	snapLines := func(skip int, ox, oy, dw, dh float64) (xs, ys []float64) {
		xs = []float64{ox, ox + dw/2, ox + dw}
		ys = []float64{oy, oy + dh/2, oy + dh}
		for i := 0; i < nbox(); i++ {
			if i == skip {
				continue
			}
			bx, by, bw, bh := rectPx(i, ox, oy, dw, dh)
			xs = append(xs, bx, bx+bw/2, bx+bw)
			ys = append(ys, by, by+bh/2, by+bh)
		}
		return xs, ys
	}
	// grabAt is what a press at (x, y) has hold of: the topmost box whose
	// border or middle is under it, or -1. Last first, because that is the one
	// drawn on top and so the one the eye is pointing at.
	grabAt := func(x, y, ox, oy, dw, dh float64) (i int, horiz, vert, left, top, inside bool) {
		for i := nbox() - 1; i >= 0; i-- {
			bx, by, bw, bh := rectPx(i, ox, oy, dw, dh)
			h, v, l, t, in := fxEdges(x, y, bx, by, bw, bh)
			if h || v || in {
				return i, h, v, l, t, in
			}
		}
		return -1, false, false, false, false, false
	}

	area.SetDrawFunc(func(_ *gtk.DrawingArea, cr *cairo.Context, w, h int) {
		ox, oy, dw, dh, ok := disp(float64(w), float64(h))
		if !ok {
			return
		}
		for i := 0; i < nbox(); i++ {
			bx, by, bw, bh := rectPx(i, ox, oy, dw, dh)
			// dashed and violet, the same frame the Cut page draws around the
			// text effect being worked on (cut_fxview.go): the two are the
			// same object -- a box words are fitted into -- and a solid
			// hairline here read as part of the picture
			pubBoxOutline(cr, bx, by, bw, bh)
			ix, iy := iconPx(i, ox, oy, dw, dh)
			cr.SetSourceRGBA(0, 0, 0, 0.7)
			cr.Rectangle(ix, iy, pubIconPx, pubIconPx)
			cr.Fill()
			drawPencil(cr, ix+pubIconPx/2, iy+pubIconPx/2, pubIconPx*0.62)
		}
		if band != nil {
			pubBoxOutline(cr, band[0], band[1], band[2], band[3])
		}
	})

	// the arrows on a border and the hand inside one, which is the whole of
	// how anyone finds out a border resizes
	motion := gtk.NewEventControllerMotion()
	motion.ConnectMotion(func(x, y float64) {
		name := "default"
		if ox, oy, dw, dh, ok := disp(float64(area.AllocatedWidth()), float64(area.AllocatedHeight())); ok {
			if i, h, v, l, t, in := grabAt(x, y, ox, oy, dw, dh); i >= 0 {
				name = fxCursorName(h, v, l, t, in)
			}
		}
		if name != cursor {
			cursor = name
			area.SetCursor(gdk.NewCursorFromName(name, nil))
		}
	})
	area.AddController(motion)

	g := gtk.NewGestureDrag()
	var x0, y0 float64
	g.ConnectDragBegin(func(x, y float64) {
		x0, y0 = x, y
		grab.on = false
		ox, oy, dw, dh, ok := disp(float64(area.AllocatedWidth()), float64(area.AllocatedHeight()))
		if !ok {
			return
		}
		i, h, v, l, t, in := grabAt(x, y, ox, oy, dw, dh)
		if i < 0 {
			return // clear of every box: this drag marks a new one
		}
		bx, by, bw, bh := rectPx(i, ox, oy, dw, dh)
		grab.on, grab.i = true, i
		grab.horiz, grab.vert, grab.left, grab.top, grab.inside = h, v, l, t, in
		grab.px, grab.py = x, y
		grab.x0, grab.y0, grab.w0, grab.h0 = bx, by, bw, bh
		grab.cur = [4]float64{bx, by, bw, bh}
		// the edge kept still is the far one from whichever was grabbed
		grab.ax, grab.ay = bx+bw, by+bh
		if !l {
			grab.ax = bx
		}
		if !t {
			grab.ay = by
		}
	})
	g.ConnectDragUpdate(func(dx, dy float64) {
		ox, oy, dw, dh, ok := disp(float64(area.AllocatedWidth()), float64(area.AllocatedHeight()))
		if grab.on {
			xs, ys := []float64(nil), []float64(nil)
			if ok {
				xs, ys = snapLines(grab.i, ox, oy, dw, dh)
			}
			x, y := grab.px+dx, grab.py+dy
			if grab.horiz || grab.vert {
				// the edge under the hand IS what moves, so putting the
				// pointer on a line puts the edge on it (snapPointPx)
				nx, ny, nw, nh := resizeFree(snapPointPx(x, fxSnapPx, xs...),
					snapPointPx(y, fxSnapPx, ys...), grab.ax, grab.ay,
					grab.x0, grab.y0, grab.w0, grab.h0, pubBoxMin,
					grab.horiz, grab.vert, grab.left, grab.top)
				grab.cur = [4]float64{nx, ny, nw, nh}
			} else {
				// the whole box slides, so any of its three lines on an axis
				// may be the one that lands (snapEdgePx)
				nx, ny := grab.x0+dx, grab.y0+dy
				grab.cur = [4]float64{snapEdgePx(nx, grab.w0, fxSnapPx, xs...),
					snapEdgePx(ny, grab.h0, fxSnapPx, ys...), grab.w0, grab.h0}
			}
			area.QueueDraw()
			return
		}
		// a NEW box: the corner under the hand snaps to the same lines
		bx1, by1 := x0+dx, y0+dy
		if ok {
			xs, ys := snapLines(-1, ox, oy, dw, dh)
			bx1, by1 = snapPointPx(bx1, fxSnapPx, xs...), snapPointPx(by1, fxSnapPx, ys...)
		}
		band = &[4]float64{math.Min(x0, bx1), math.Min(y0, by1),
			math.Abs(bx1 - x0), math.Abs(by1 - y0)}
		area.QueueDraw()
	})
	g.ConnectDragEnd(func(dx, dy float64) {
		band = nil
		area.QueueDraw()
		ox, oy, dw, dh, ok := disp(float64(area.AllocatedWidth()), float64(area.AllocatedHeight()))
		if !ok {
			grab.on = false
			return
		}
		// a box that was moved or resized: its new corners, back into
		// fractions of the picture
		if grab.on {
			i, r := grab.i, grab.cur
			grab.on = false
			if math.Abs(dx) < 2 && math.Abs(dy) < 2 {
				// a press that never travelled, on a box: the ✎ if it landed
				// on one, and otherwise nothing -- pressing a box is not an
				// edit of it
				if ix, iy := iconPx(i, ox, oy, dw, dh); x0 >= ix && x0 <= ix+pubIconPx && y0 >= iy && y0 <= iy+pubIconPx {
					if isTitle(i) {
						p.editTitle()
					} else {
						p.editText(i)
					}
				}
				area.QueueDraw()
				return
			}
			b := fxBox{
				cx: (r[0] + r[2]/2 - ox) / dw,
				cy: (r[1] + r[3]/2 - oy) / dh,
				wf: r[2] / dw,
				hf: r[3] / dh,
			}.clamp()
			if isTitle(i) {
				p.setTitleBox(&pubText{Cx: b.cx, Cy: b.cy, Wf: b.wf, Hf: b.hf})
				return
			}
			ts := append([]pubText(nil), p.texts...)
			ts[i].Cx, ts[i].Cy, ts[i].Wf, ts[i].Hf = b.cx, b.cy, b.wf, b.hf
			p.setTexts(ts)
			return
		}
		// a press that never travelled, clear of every box, is a press on
		// nothing: the ✎s were asked about above
		if math.Abs(dx) < 8 && math.Abs(dy) < 8 {
			return
		}
		// the band as it was last drawn, snapped -- not the raw drag, or the
		// box would jump off the line it was shown sitting on
		bx1, by1 := x0+dx, y0+dy
		xs, ys := snapLines(-1, ox, oy, dw, dh)
		bx1, by1 = snapPointPx(bx1, fxSnapPx, xs...), snapPointPx(by1, fxSnapPx, ys...)
		b := fxBox{
			cx: ((x0+bx1)/2 - ox) / dw,
			cy: ((y0+by1)/2 - oy) / dh,
			wf: math.Abs(bx1-x0) / dw,
			hf: math.Abs(by1-y0) / dh,
		}.clamp()
		p.a.askPubText("", func(s string) {
			if strings.TrimSpace(s) == "" {
				return // a box with no words would print nothing and still wear a ✎
			}
			p.setTexts(append(append([]pubText(nil), p.texts...),
				pubText{Cx: b.cx, Cy: b.cy, Wf: b.wf, Hf: b.hf, Text: s}))
		}, nil)
	})
	area.AddController(g)

	ov := gtk.NewOverlay()
	ov.SetChild(pic)
	ov.AddOverlay(area)
	return ov
}

// drawPencil is the ✎ as a path: cr.ShowText("✎") came out as tofu on a
// machine whose sans-serif has no U+270E (every other mark on these pages is
// drawn for the same reason). Tip at the lower left, barrel up to the right,
// a band where the lead meets the wood.
func drawPencil(cr *cairo.Context, cx, cy, size float64) {
	cr.Save()
	cr.Translate(cx, cy)
	cr.Rotate(-math.Pi / 4)
	l, hh, tip := size*0.9, size*0.19, size*0.26
	cr.SetSourceRGBA(1, 1, 1, 0.95)
	cr.MoveTo(-l/2, 0)
	cr.LineTo(-l/2+tip, -hh)
	cr.LineTo(l/2, -hh)
	cr.LineTo(l/2, hh)
	cr.LineTo(-l/2+tip, hh)
	cr.ClosePath()
	cr.Fill()
	cr.SetSourceRGBA(0, 0, 0, 0.6)
	cr.SetLineWidth(math.Max(1, size*0.09))
	cr.MoveTo(-l/2+tip, -hh)
	cr.LineTo(-l/2+tip, hh)
	cr.Stroke()
	cr.Restore()
}

// pubBoxMin is the smallest a marked box may be dragged to, in widget pixels:
// small enough for a word in a corner, big enough that its ✎ still fits and
// the hand can find its border again.
const pubBoxMin = 28.0

// pubBoxOutline is the frame around a marked box, and it is the Cut page's
// frame around the text effect being worked on: dashed violet, same weight,
// because they are the same object -- a box words are fitted into -- and a
// solid hairline over a photograph reads as part of the photograph.
func pubBoxOutline(cr *cairo.Context, x, y, w, h float64) {
	cr.SetSourceRGBA(0.6, 0.55, 0.95, 0.9)
	cr.SetLineWidth(1.5)
	cr.SetDash([]float64{4, 3}, 0)
	cr.Rectangle(x, y, w, h)
	cr.Stroke()
	cr.SetDash(nil, 0)
}

// setTitleBox moves the title's band and re-prints. nil puts it back to the
// default (pubTitleBox), which is what Remove means for a box whose words are
// not its own to delete.
func (p *publisher) setTitleBox(b *pubText) {
	p.titleBox = b
	if p.shotOver != nil {
		p.shotOver.QueueDraw()
	}
	p.recomposite()
}

// editTitle is the title band's ✎: the words ON THE PICTURE. Rewording them
// leaves the video's title alone and vice versa. Remove takes the line off and
// it stays off (nothing seeds twice, seedThumbTitle).
func (p *publisher) editTitle() {
	p.a.askPubText(p.thumbTitle, func(s string) {
		p.setThumbTitle(strings.TrimSpace(s))
	}, func() {
		p.setThumbTitle("")
	})
}

// setThumbTitle is the one place the picture's line changes: state, overlay
// boxes and printed file agree. Setting it counts as seeding either way, so a
// line taken off stays off; words wanted afterwards are a box like any other
// (textOverlay).
func (p *publisher) setThumbTitle(s string) {
	p.thumbTitle, p.titleSeeded = s, true
	if p.shotOver != nil {
		p.shotOver.QueueDraw()
	}
	p.recomposite()
}

// seedThumbTitle gives a picture the video's title as its line, once. Called
// where a thumbnail comes into existence -- drawn, or chosen from the row --
// and never again: after that the picture's words are its own.
func (p *publisher) seedThumbTitle() {
	if p == nil || p.titleSeeded {
		return
	}
	if t := strings.TrimSpace(p.title.Text()); t != "" {
		p.setThumbTitle(t)
		return
	}
	p.titleSeeded = false // nothing to seed with yet; the next thumbnail tries again
}

// editText reopens text i's words. Saving empty removes it -- the words are
// the whole effect, so no words and Remove mean the same thing.
func (p *publisher) editText(i int) {
	if i < 0 || i >= len(p.texts) {
		return
	}
	rm := func() {
		p.setTexts(append(append([]pubText(nil), p.texts[:i]...), p.texts[i+1:]...))
	}
	p.a.askPubText(p.texts[i].Text, func(s string) {
		if strings.TrimSpace(s) == "" {
			rm()
			return
		}
		ts := append([]pubText(nil), p.texts...)
		ts[i].Text = s
		p.setTexts(ts)
	}, rm)
}

// askPubText is the words dialog: a multi-line box, because newlines are
// content here exactly as they are in a Text effect (they break a line, and
// nothing else does). rm is nil for a new box -- there is nothing to remove
// until the words exist.
func (a *App) askPubText(initial string, ok func(string), rm func()) {
	tv := gtk.NewTextView()
	tv.SetMonospace(true)
	tv.SetWrapMode(gtk.WrapWordChar)
	tv.SetAcceptsTab(false)
	tv.Buffer().SetText(initial)
	sc := gtk.NewScrolledWindow()
	sc.SetChild(tv)
	sc.SetSizeRequest(-1, 90)
	sc.SetPolicy(gtk.PolicyNever, gtk.PolicyAutomatic)
	sc.SetHasFrame(true)

	var win *gtk.Window
	var btns []gtk.Widgetter
	if rm != nil {
		del := gtk.NewButtonWithLabel("Remove")
		del.AddCSSClass("destructive-action")
		del.ConnectClicked(func() { win.Close(); rm() })
		btns = append(btns, del)
	}
	cancel := gtk.NewButtonWithLabel("Cancel")
	save := gtk.NewButtonWithLabel("Save")
	save.AddCSSClass("suggested-action")
	save.ConnectClicked(func() {
		b := tv.Buffer()
		s := b.Text(b.StartIter(), b.EndIter(), false)
		win.Close()
		ok(s)
	})
	win = a.modal("Words on the thumbnail", "Printed to fill the box you marked — a longer line comes out "+
		"smaller, and Enter starts a new line.", 380, sc, append(btns, cancel, save)...)
	cancel.ConnectClicked(func() { win.Close() })
	tv.GrabFocus()
	win.SetVisible(true)
}
