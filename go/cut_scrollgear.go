package main

// The scrollbar's thumb, geared down.
//
// The bar under the tracks is a stock GtkScrollbar over the page's own
// adjustment, and a stock scrollbar maps the thumb onto the whole range: one
// pixel of thumb is totalW/troughW pixels of timeline. Zoomed right in on a
// session of any length that is a couple of hundred timeline pixels -- a good
// part of what is on screen -- per pixel of drag, so the pictures whip past
// faster than they can be read, and the thumb is useless for the one thing it
// was picked up for, which was to look along the session.
//
// So the drag is geared: once a pixel of thumb would move the view by more
// than a fortieth of itself, it moves it by exactly that instead. The thumb
// then no longer stays under the pointer -- it cannot, the range is what it
// is -- but the view scrolls at a speed the eye can follow, and a screenful
// is always the same forty pixels of drag whatever the zoom. Below that ratio
// the bar is left entirely to GTK, thumb under pointer, as it always was: the
// gearing exists for the zoomed-in case and is not felt anywhere else. A
// click in the trough is GTK's too.

import (
	"github.com/diamondburned/gotk4/pkg/gtk/v4"
)

// gearPx is the drag, in pixels, that scrolls one screenful once the thumb
// is geared.
const gearPx = 40.0

// scrollGear is how many timeline pixels one pixel of thumb drag moves:
// linear is what the stock bar would do, and the answer is that, held to a
// fortieth of the view.
func scrollGear(linear, viewW float64) float64 {
	if lim := viewW / gearPx; linear > lim {
		return lim
	}
	return linear
}

// gearScrollbar puts the geared drag on the bar. A capture-phase gesture, so
// it sees the press before the scrollbar's own does; it claims the sequence
// only when the drag is geared, and denies it -- hands it to GTK untouched --
// for every press that is not on the thumb, or is on it at a zoom the stock
// bar serves fine.
func (ed *cutEditor) gearScrollbar() {
	if ed.hbar == nil {
		return
	}
	drag := gtk.NewGestureDrag()
	drag.SetPropagationPhase(gtk.PhaseCapture)
	var v0, gear float64
	drag.ConnectDragBegin(func(x, _ float64) {
		gear = 0
		slider := cssChild(ed.hbar, "slider")
		if slider == nil {
			drag.SetState(gtk.EventSequenceDenied)
			return
		}
		sb, ok := slider.ComputeBounds(ed.hbar)
		if !ok || x < float64(sb.X()) || x > float64(sb.X()+sb.Width()) {
			drag.SetState(gtk.EventSequenceDenied) // the trough: GTK pages
			return
		}
		trough := slider.Parent()
		if trough == nil {
			drag.SetState(gtk.EventSequenceDenied)
			return
		}
		tb, ok := gtk.BaseWidget(trough).ComputeBounds(ed.hbar)
		if !ok || tb.Width()-sb.Width() <= 0 {
			drag.SetState(gtk.EventSequenceDenied)
			return
		}
		linear := (ed.hadj.Upper() - ed.hadj.PageSize()) / float64(tb.Width()-sb.Width())
		g := scrollGear(linear, ed.viewW)
		if g >= linear {
			drag.SetState(gtk.EventSequenceDenied) // not geared: the stock drag
			return
		}
		drag.SetState(gtk.EventSequenceClaimed)
		v0, gear = ed.hadj.Value(), g
	})
	drag.ConnectDragUpdate(func(dx, _ float64) {
		if gear > 0 {
			ed.setOff(v0 + dx*gear)
		}
	})
	drag.ConnectDragEnd(func(_, _ float64) { gear = 0 })
	ed.hbar.AddController(drag)
}

// cssChild is the first descendant of w with that CSS node name -- how a
// stock widget's parts are found, since a scrollbar hands out no handle to
// its slider.
func cssChild(w gtk.Widgetter, name string) *gtk.Widget {
	for c := gtk.BaseWidget(w).FirstChild(); c != nil; c = gtk.BaseWidget(c).NextSibling() {
		cw := gtk.BaseWidget(c)
		if cw.CSSName() == name {
			return cw
		}
		if d := cssChild(cw, name); d != nil {
			return d
		}
	}
	return nil
}
