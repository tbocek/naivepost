package main

// The narrator recording's waveform under the video on Narrate, and how a take
// is chosen on it (narrate_take.go says what a take IS). Deliberately the Cut
// page's audio lane idiom -- envelope, meter, blue, wheel zoom, green for kept.
// NOT a timeline: one recording on its OWN clock, first second to last. Only
// for a narrator slot; a voices-folder wav is used whole and "no audio" has
// nothing to pick from.

import (
	"fmt"
	"math"

	"github.com/diamondburned/gotk4/pkg/cairo"
	"github.com/diamondburned/gotk4/pkg/gdk/v4"
	"github.com/diamondburned/gotk4/pkg/glib/v2"
	"github.com/diamondburned/gotk4/pkg/gtk/v4"
)

const (
	takeRulerH = 12.0 // the seconds along the top
	// ...and how tall one channel of it is drawn. Taller than a Cut lane's
	// (waveLaneH): that band is one of a dozen rows sharing a page, and this
	// is the only picture in its own half of the column -- the seconds worth
	// cloning are picked by looking at it, and a 30 px strip is a strip you
	// squint at.
	takeLaneH  = 56.0
	takeMaxPps = 200.0
	// a press that travels less than this is a click, not a drag: it clears
	// the selection instead of making a nought-length one
	takeClickPx = 3.0
)

// what ▶ says when it is a ▶. Named because the button wears two faces and the
// other one has to be able to put this back (syncPlayBtn).
const takePlayTip = "Play from the red bar: the takes, one after another — " +
	"exactly what the model is cloned from"

type takeBand struct {
	vp    *voicePicker
	area  *gtk.DrawingArea
	frame *gtk.Frame
	// ＋, －, ▶ takes. Kept because they are greyed as a set: with no
	// recording behind the band there is nothing any of them could do.
	addBtn, delBtn, playBtn *gtk.Button

	// the way back to the rest of the recording once the wheel has zoomed past
	// it. Ours rather than a scrolled window's, for the reason the Cut page's is
	// (cut.go): a scrolled window would want a child as wide as the whole file.
	bar *gtk.Scrollbar
	adj *gtk.Adjustment

	src  string // the recording drawn, "" when this voice has none
	base string
	dur  float64
	wf   *waveform

	pps   float64 // px per second; fitPps when the whole file is shown
	viewX float64 // px of it scrolled off the left
	lastX float64 // where the pointer is, so the wheel zooms around it
	w     float64 // the area's width, learned at the last resize

	// where ▶ starts, in the file's own seconds; < 0 for "at the first take".
	// A click puts it there, and until the click there is nowhere in a
	// recording nobody has listened to that is a better guess than its takes.
	at float64
	// and the same bar under the pointer, drawn faint: a click is about to put
	// it there, and a band that says so before the click is one you can aim
	hoverX float64
	hover  bool

	takes []voiceTake
	// the live selection in the file's own seconds; sel1 <= sel0 is none.
	// It is not a take: ＋ is what makes it one, so a drag can be re-aimed
	// as many times as it takes without touching what the model gets.
	sel0, sel1 float64

	// ▶: the takes queued back to back, and which one is sounding. The
	// player's own end-of-segment is what advances it (chainOn).
	queue []voiceTake
	qAt   int
}

func (b *takeBand) sel() (float64, float64, bool) {
	if b.sel1 <= b.sel0 {
		return 0, 0, false
	}
	return b.sel0, b.sel1, true
}

// ---- geometry ---------------------------------------------------------------

// fitPps is the zoom at which the whole recording is on screen, and the zoom
// this will not go below: there is nothing to the left of a file's first second
// or to the right of its last, so scrolling past either is scrolling into
// nothing.
func (b *takeBand) fitPps() float64 {
	if b.dur <= 0 || b.w <= 0 {
		return 1
	}
	return b.w / b.dur
}

func (b *takeBand) xOf(t float64) float64 { return t*b.pps - b.viewX }
func (b *takeBand) tAt(x float64) float64 { return (x + b.viewX) / b.pps }

// clampView keeps the zoom and the offset in range together, since neither is
// valid on its own: zooming out past the fit and then scrolling is how a band
// ends up drawing a file that starts three hundred pixels in.
func (b *takeBand) clampView() {
	b.pps = math.Max(b.fitPps(), math.Min(takeMaxPps, b.pps))
	b.viewX = math.Max(0, math.Min(math.Max(0, b.dur*b.pps-b.w), b.viewX))
}

// zoomAt multiplies the zoom and leaves the second under the cursor where it
// was, which is the only zoom that does not lose your place.
func (b *takeBand) zoomAt(x, factor float64) {
	at := b.tAt(x)
	b.pps *= factor
	b.clampView()
	b.viewX = at*b.pps - x
	b.clampView()
	b.syncScroll()
	b.redraw()
}

// syncScroll points the scrollbar at the recording as it is now shown, and is
// also where the bar disappears. The adjustment holds the offset in the same
// pixels viewX is in, and its clamping is the one that counts: setting the
// value against the new upper is what pulls a view back onto the file after a
// zoom out, so viewX is read back from it rather than trusted.
func (b *takeBand) syncScroll() {
	if b.adj == nil {
		return
	}
	b.adj.SetUpper(b.dur * b.pps)
	b.adj.SetPageSize(b.w)
	b.adj.SetStepIncrement(b.w / 8)
	b.adj.SetPageIncrement(b.w * 0.9)
	b.adj.SetValue(b.viewX)
	b.viewX = b.adj.Value()
	b.bar.SetVisible(b.dur*b.pps > b.w+0.5)
}

func (b *takeBand) redraw() {
	if b.area != nil {
		b.area.QueueDraw()
	}
}

// laneCount is how many lanes the envelope draws, before it has arrived as well
// as after: one, until the decode says two.
func (b *takeBand) laneCount() int {
	if b.wf != nil && len(b.wf.chans) > 0 {
		return len(b.wf.chans)
	}
	return 1
}

func (b *takeBand) height() int {
	return int(takeRulerH + float64(b.laneCount())*takeLaneH + 2*wavePad)
}

// ---- building ---------------------------------------------------------------

// buildTakeBand makes the band and the three buttons that work on it. The
// buttons are returned separately because they belong on the dropdown's row --
// the choice of voice and the choice of seconds within it are one decision, and
// splitting them over two rows put a slider between them.
func (vp *voicePicker) buildTakeBand() (*gtk.Frame, *gtk.Box) {
	b := &takeBand{vp: vp, pps: 1, at: -1}
	vp.band = b

	b.area = gtk.NewDrawingArea()
	b.area.SetDrawFunc(b.draw)
	b.area.SetContentHeight(b.height())
	b.area.SetHExpand(true)
	b.area.SetTooltipText("The recording this voice is cloned from. Drag to select, " +
		"＋ to make that a take, click to put the red bar where ▶ starts, wheel to " +
		"zoom, Shift+wheel to pan. With no takes the seconds are chosen for you.")

	// the width is the view, and it is learned here rather than at the draw:
	// the scrollbar has to be told about it, and showing or hiding a widget
	// from inside a draw is how you get a resize loop (fitSrc says the same)
	b.area.ConnectResize(func(w, _ int) {
		b.w = float64(w)
		b.clampView()
		b.syncScroll()
		b.redraw()
	})

	motion := gtk.NewEventControllerMotion()
	motion.ConnectMotion(func(x, _ float64) {
		b.lastX, b.hoverX, b.hover = x, x, true
		b.redraw()
	})
	motion.ConnectLeave(func() {
		b.hover = false
		b.redraw()
	})
	b.area.AddController(motion)

	// the Cut page's wheel, to the letter (cut.go): plain wheel zooms around
	// the cursor, Shift -- and a trackpad's sideways swipe -- pans
	scroll := gtk.NewEventControllerScroll(gtk.EventControllerScrollBothAxes)
	scroll.ConnectScroll(func(dx, dy float64) bool {
		if scroll.CurrentEventState()&gdk.ShiftMask != 0 {
			dx, dy = dy, 0
		}
		if dx != 0 {
			b.viewX += dx * b.w / 8
			b.clampView()
			b.syncScroll()
			b.redraw()
		}
		if dy != 0 {
			b.zoomAt(b.lastX, math.Pow(1.25, -dy))
		}
		return true
	})
	b.area.AddController(scroll)

	drag := gtk.NewGestureDrag()
	var fromX float64
	drag.ConnectDragBegin(func(x, _ float64) { fromX = x })
	drag.ConnectDragUpdate(func(dx, _ float64) { b.dragTo(fromX, fromX+dx) })
	drag.ConnectDragEnd(func(dx, _ float64) {
		if math.Abs(dx) < takeClickPx {
			// a click is "never mind, start here": it puts the selection away
			// rather than leaving a hairline one that ＋ would then refuse, and
			// it moves the bar ▶ picks up from
			b.sel0, b.sel1 = 0, 0
			if b.dur > 0 {
				b.at = math.Max(0, math.Min(b.dur, b.tAt(fromX)))
			}
			b.redraw()
			return
		}
		b.dragTo(fromX, fromX+dx)
	})
	b.area.AddController(drag)

	// the playhead is only worth a redraw while something is running through
	// the takes; the rest of the time this band is a still picture
	b.area.AddTickCallback(func(_ gtk.Widgetter, _ gdk.FrameClocker) bool {
		if len(b.queue) > 0 && vp.playing() {
			b.redraw()
		}
		return true
	})

	// Hidden while the whole file is on screen: a bar at the zoom floor says
	// there is more recording off to the right when there is not.
	b.adj = gtk.NewAdjustment(0, 0, 0, 1, 1, 0)
	b.adj.ConnectValueChanged(func() {
		b.viewX = b.adj.Value()
		b.redraw()
	})
	b.bar = gtk.NewScrollbar(gtk.OrientationHorizontal, b.adj)
	b.bar.SetVisible(false)

	inner := gtk.NewBox(gtk.OrientationVertical, 0)
	inner.Append(b.area)
	inner.Append(b.bar)
	b.frame = gtk.NewFrame("")
	b.frame.SetLabelWidget(nil)
	b.frame.SetChild(inner)

	b.addBtn = gtk.NewButtonFromIconName("list-add-symbolic")
	b.addBtn.SetTooltipText("Use the selected seconds as a voice-clone take")
	b.addBtn.ConnectClicked(b.addClicked)
	b.delBtn = gtk.NewButtonFromIconName("list-remove-symbolic")
	b.delBtn.SetTooltipText("Take the selected seconds back out of the takes")
	b.delBtn.ConnectClicked(b.delClicked)
	b.playBtn = gtk.NewButtonFromIconName("media-playback-start-symbolic")
	b.playBtn.SetTooltipText(takePlayTip)
	b.playBtn.ConnectClicked(b.playClicked)

	btns := gtk.NewBox(gtk.OrientationHorizontal, 4)
	btns.SetVAlign(gtk.AlignCenter)
	btns.Append(b.addBtn)
	btns.Append(b.delBtn)
	btns.Append(b.playBtn)
	return b.frame, btns
}

// dragTo sets the selection from two x's, either way round, clipped to the
// file: dragging off the left end means "from the start", not a negative
// second the reference would then be cut at.
func (b *takeBand) dragTo(x0, x1 float64) {
	t0, t1 := b.tAt(math.Min(x0, x1)), b.tAt(math.Max(x0, x1))
	b.sel0 = math.Max(0, t0)
	b.sel1 = math.Min(b.dur, t1)
	b.redraw()
}

// ---- what it is showing -----------------------------------------------------

// forget aims the band at a new recording, which means putting away every
// number that was a second in the old one: the picked stretch, the red bar ▶
// starts from, the zoom and how far along it we had scrolled, and the queue a
// ▶ already pressed was walking. None of them mean anything in the next file,
// and one left behind aims at a place that is not on screen any more.
func (b *takeBand) forget(src string) {
	b.src, b.base = src, baseName(src)
	b.wf, b.dur = nil, 0
	b.sel0, b.sel1 = 0, 0
	b.viewX, b.pps = 0, 1
	b.at = -1
	b.hover = false
	b.queue, b.qAt = nil, 0
}

// sync points the band at whatever the chosen voice is cut from, and is the
// one way in: it is called when the page is built, when the voice changes, and
// when the Prepare page re-tags who is who. Cheap when nothing moved, because
// the answer is usually the same recording it was already drawing.
func (b *takeBand) sync() {
	if b == nil {
		return
	}
	a := b.vp.a
	src := a.takeSource()
	show := src != ""
	b.frame.SetVisible(show)
	for _, btn := range []*gtk.Button{b.addBtn, b.delBtn, b.playBtn} {
		btn.SetSensitive(show)
	}
	defer b.syncPlayBtn() // forget() may end a walk; the ⏹ goes back to ▶
	if src == b.src {
		b.takes = a.voiceTakes() // the file may have been edited under us
		b.redraw()
		return
	}
	b.forget(src)
	if !show {
		b.redraw()
		return
	}
	b.takes = a.voiceTakes()
	b.load()
}

// load decodes the envelope and probes the length off the GUI thread -- both
// shell out, and a recording nobody has drawn yet is an ffmpeg pass over the
// whole file. The band draws its ground and says so in the meantime.
func (b *takeBand) load() {
	a, src, base := b.vp.a, b.src, b.base
	go func() {
		dur, _ := ffprobeDur(src)
		au := tlAudio{base: base, path: src, dur: dur, chans: ffprobeChannels(src)}
		wf, err := loadWave(a.waveCache(), au)
		glib.IdleAdd(func() {
			if b.src != src {
				return // the voice changed while this was decoding
			}
			if err != nil {
				a.logf("no waveform for %s: %v", base, err)
			}
			b.wf, b.dur = wf, dur
			b.pps = 0 // below fit, so clampView lands exactly on the whole file
			b.clampView()
			b.syncScroll()
			b.area.SetContentHeight(b.height())
			b.redraw()
		})
	}()
}

// ---- the three buttons -------------------------------------------------------

func (b *takeBand) addClicked() {
	s, e, ok := b.sel()
	if !ok {
		b.vp.a.setStatus("drag across the wave first — ＋ makes the selection a take")
		return
	}
	if e-s < takeMin {
		b.vp.a.setStatus(fmt.Sprintf("that is %.2f s — a take has to be at least %.1f s", e-s, takeMin))
		return
	}
	b.commit(addTake(b.takes, s, e), fmt.Sprintf("take added: %s–%s", mmss(s), mmss(e)))
}

func (b *takeBand) delClicked() {
	s, e, ok := b.sel()
	if !ok {
		b.vp.a.setStatus("drag across the takes you want gone — － removes those seconds")
		return
	}
	next := dropTakes(b.takes, s, e)
	if sameTakes(next, b.takes) {
		b.vp.a.setStatus("nothing is picked in those seconds")
		return
	}
	b.commit(next, "takes removed")
}

// commit stores the new set and says what it means. The write and the ffmpeg
// the write triggers go off the GUI thread; the band is redrawn straight away,
// because the set is what the band draws and it has already changed.
func (b *takeBand) commit(ts []voiceTake, what string) {
	a, base := b.vp.a, b.base
	b.takes = ts
	b.sel0, b.sel1 = 0, 0
	b.redraw()
	total := takesTotal(ts)
	go func() {
		if err := a.setTakesFor(base, ts); err != nil {
			a.logfIdle("takes: %v", err)
			glib.IdleAdd(func() { a.setStatus("could not save the takes — see log") })
			return
		}
		glib.IdleAdd(func() {
			if len(ts) == 0 {
				a.setStatus("no takes left — the seconds are chosen for you again on the next line spoken")
				return
			}
			// the total, because it is the number that decides whether this is
			// a voice: refWant is what the automatic pick aims for and is a
			// fair mark to be judged against
			a.setStatus(fmt.Sprintf("%s — %d take(s), %.1f s of reference (%.0f s is plenty). "+
				"Re-cut on the next line spoken.", what, len(ts), total, refWant))
		})
	}()
}

// playClicked plays what takeQueue says, from the red bar. Not the reference
// file, which does not exist until something has been spoken and is levelled
// and pitch-shifted besides: this is the seconds themselves, in the recording
// they came from, which is the thing being judged.
func (b *takeBand) playClicked() {
	if len(b.queue) > 0 {
		b.stopWalk()
		return
	}
	q, takes := takeQueue(b.takes, b.at, b.dur)
	if len(q) == 0 {
		b.vp.a.setStatus("no takes yet — drag across the wave and press ＋")
		return
	}
	if b.vp.player == nil {
		b.vp.a.setStatus("no player available — see log")
		return
	}
	b.queue, b.qAt = q, 0
	b.syncPlayBtn() // ⏹ now, not when the first segment prerolls
	// the player is the sample's as well, and it is holding the recording now:
	// leaving the old sample's key behind would have ▶ beside the sample
	// "resume" a wav that is no longer in it
	b.vp.spoken = ""
	b.cue()
	if !takes {
		b.vp.a.setStatus(fmt.Sprintf("playing the recording from %s — nothing is picked after it", mmss(q[0].S)))
		return
	}
	b.vp.a.setStatus(fmt.Sprintf("playing %d take(s), %.1f s from %s",
		len(q), takesTotal(q), mmss(q[0].S)))
}

// stopWalk ends the walk on purpose, which is the other thing that button can
// mean. The queue is emptied FIRST: stopping the player lands straight back
// here through OnState, and chainOn finding nothing queued is what makes that
// re-entry a no-op rather than a second "takes played".
func (b *takeBand) stopWalk() {
	b.queue, b.qAt = nil, 0
	if b.vp.player != nil {
		b.vp.player.Stop()
	}
	b.syncPlayBtn()
	b.vp.a.setStatus("stopped playing the takes")
	b.redraw()
}

// syncPlayBtn draws the button's two faces: ⏹, not ⏸, since a walk cues each
// take from its own start and a second press can only mean "enough". It reads
// the QUEUE, not the player, which pauses at every join.
func (b *takeBand) syncPlayBtn() {
	if b == nil || b.playBtn == nil {
		return
	}
	if len(b.queue) > 0 {
		b.playBtn.SetIconName("media-playback-stop-symbolic")
		b.playBtn.SetTooltipText("Stop playing the takes")
		return
	}
	b.playBtn.SetIconName("media-playback-start-symbolic")
	b.playBtn.SetTooltipText(takePlayTip)
}

// takeQueue is what ▶ plays: the takes from the red bar onward, the one the bar
// lands inside trimmed to begin there; all of them with no bar. The bool says
// whether those ARE takes -- a bar past the last, or no takes at all, means
// play the recording from there, which is how takes get found.
func takeQueue(ts []voiceTake, at, dur float64) ([]voiceTake, bool) {
	var out []voiceTake
	for _, t := range ts {
		if at >= 0 {
			if t.E <= at {
				continue
			}
			t.S = math.Max(t.S, at)
		}
		out = append(out, t)
	}
	if len(out) > 0 {
		return out, true
	}
	if at < 0 || at >= dur {
		return nil, false
	}
	return []voiceTake{{S: at, E: dur}}, false
}

func (b *takeBand) cue() {
	t := b.queue[b.qAt]
	b.vp.player.PlaySegment(b.src, t.S, t.E, true)
}

// chainOn hangs on the player's OnState, where a segment's end arrives (the
// bus watch pauses on EOS). It drops the chain whenever the player holds
// anything but this recording, so playing a sample cancels it.
func (b *takeBand) chainOn() {
	if b == nil || len(b.queue) == 0 {
		return
	}
	p := b.vp.player
	if p == nil {
		return
	}
	next := takeNext(p.loaded, b.src, p.ended, b.qAt, len(b.queue))
	switch {
	case next == b.qAt: // a start, a pause, anything that is not an end
	case next < 0:
		// the walk is over, either because the last take finished or because
		// something else took the player. Only the first is worth saying.
		done := p.loaded == b.src
		b.queue = nil
		if done {
			b.vp.a.setStatus("takes played")
		}
	default:
		b.qAt = next
		b.cue()
	}
}

// takeNext is the whole of that decision, split out from the pipeline it is
// made about: the player is GStreamer and stays out of a unit test, exactly as
// Player.hushes is split from applyMute. It answers with the take to cue next,
// the one already sounding for "nothing has happened", or -1 for "stop".
func takeNext(loaded, src string, ended bool, at, n int) int {
	if loaded != src {
		return -1 // the player is holding something else: the sample took it
	}
	if !ended {
		return at
	}
	if at+1 >= n {
		return -1
	}
	return at + 1
}

// ---- drawing ----------------------------------------------------------------

func (b *takeBand) draw(_ *gtk.DrawingArea, cr *cairo.Context, w, h int) {
	if b.pps <= 0 || b.pps < b.fitPps() {
		b.clampView() // the first draw, and every one after a window resize
	}
	cr.SetSourceRGB(0.11, 0.11, 0.12)
	cr.Rectangle(0, 0, float64(w), float64(h))
	cr.Fill()
	if b.src == "" {
		return
	}
	top := takeRulerH + wavePad
	lanes := b.laneCount()
	laneH := float64(lanes) * takeLaneH
	x0 := math.Max(0, b.xOf(0))
	x1 := math.Min(float64(w), b.xOf(b.dur))
	if b.dur <= 0 {
		x0, x1 = 0, float64(w) // length not probed yet; the ground is the whole band
	}

	// the ground says the recording is here, exactly as a Cut lane's does, and
	// is what a file still being decoded shows instead of nothing
	cr.SetSourceRGB(0.16, 0.17, 0.2)
	cr.Rectangle(x0, top, x1-x0, laneH)
	cr.Fill()
	for ch := 0; ch < lanes; ch++ {
		b.drawLane(cr, ch, top+float64(ch)*takeLaneH, x0, x1)
	}
	if b.wf == nil {
		cr.SetSourceRGBA(1, 1, 1, 0.5)
		cr.SetFontSize(10)
		cr.MoveTo(x0+6, top+14)
		cr.ShowText("reading the recording…")
	}

	// the takes, in the green that means kept everywhere else in this app
	for _, t := range b.takes {
		tx0, tx1 := b.xOf(t.S), b.xOf(t.E)
		if tx1 < 0 || tx0 > float64(w) {
			continue
		}
		cr.SetSourceRGBA(0.2, 0.8, 0.3, 0.28)
		cr.Rectangle(tx0, top, tx1-tx0, laneH)
		cr.Fill()
		cr.SetSourceRGB(0.5, 0.92, 0.58)
		cr.SetLineWidth(2)
		for _, x := range []float64{tx0, tx1} {
			cr.MoveTo(x, top)
			cr.LineTo(x, top+laneH)
			cr.Stroke()
		}
		if tx1-tx0 > 34 {
			cr.SetFontSize(9)
			cr.MoveTo(tx0+4, top+laneH-4)
			cr.ShowText(fmt.Sprintf("%.1fs", t.dur()))
		}
	}

	// the live selection over the top of them: it is about to become one, or
	// about to take one away, and either way it is the thing being aimed
	if s, e, ok := b.sel(); ok {
		sx0, sx1 := b.xOf(s), b.xOf(e)
		cr.SetSourceRGBA(0.3, 0.55, 0.9, 0.45)
		cr.Rectangle(sx0, top, sx1-sx0, laneH)
		cr.Fill()
		cr.SetSourceRGB(0.62, 0.82, 1)
		cr.SetLineWidth(2)
		for _, x := range []float64{sx0, sx1} {
			cr.MoveTo(x, top)
			cr.LineTo(x, top+laneH)
			cr.Stroke()
		}
	}

	// the bar a click put down, and where ▶ picks up. Through the ruler as
	// well as the lanes, because what it is saying is a TIME.
	if b.at >= 0 {
		x := math.Round(b.xOf(b.at)) + 0.5
		cr.SetSourceRGB(0.95, 0.25, 0.25)
		cr.SetLineWidth(1.5)
		cr.MoveTo(x, 0)
		cr.LineTo(x, top+laneH)
		cr.Stroke()
	}
	// the same bar under the pointer, faint: where a click would put it
	if b.hover {
		x := math.Round(b.hoverX) + 0.5
		cr.SetSourceRGBA(0.95, 0.25, 0.25, 0.4)
		cr.SetLineWidth(1)
		cr.MoveTo(x, 0)
		cr.LineTo(x, top+laneH)
		cr.Stroke()
	}

	// where ▶ has got to, in the playhead's own red
	if len(b.queue) > 0 && b.vp.player != nil {
		if at, ok := b.vp.player.Position(); ok {
			x := b.xOf(at)
			cr.SetSourceRGB(0.95, 0.25, 0.25)
			cr.SetLineWidth(1.5)
			cr.MoveTo(x, top)
			cr.LineTo(x, top+laneH)
			cr.Stroke()
		}
	}

	b.drawRuler(cr, w)
}

// drawRuler is the file's own seconds along the top. It is what makes the zoom
// mean anything: without it a wave zoomed in is a wave, and "how long is that
// stretch" is a question the band cannot answer.
func (b *takeBand) drawRuler(cr *cairo.Context, w int) {
	if b.dur <= 0 {
		return
	}
	step := tickStep(b.pps)
	cr.SetFontSize(9)
	cr.SetLineWidth(1)
	for t := math.Ceil(b.tAt(0)/step) * step; t <= b.tAt(float64(w)); t += step {
		x := math.Round(b.xOf(t)) + 0.5
		cr.SetSourceRGB(0.6, 0.6, 0.6)
		cr.MoveTo(x, takeRulerH)
		cr.LineTo(x, takeRulerH-4)
		cr.Stroke()
		cr.MoveTo(x+2, takeRulerH-3)
		cr.ShowText(mmss(t))
	}
}

// drawLane is one channel of the envelope, drawn straight across: this band has
// one recording on one clock, so there are no gaps to walk around and no
// per-video spans to break the sweep into (which is the whole of why the Cut
// page's drawWaveSpan is shaped the way it is).
func (b *takeBand) drawLane(cr *cairo.Context, ch int, y, x0, x1 float64) {
	bot := y + takeLaneH - 1
	full := takeLaneH - 2
	cr.SetSourceRGBA(0.35, 0.6, 1, 0.35)
	cr.SetLineWidth(1)
	cr.MoveTo(x0, math.Round(bot)+0.5)
	cr.LineTo(x1, math.Round(bot)+0.5)
	cr.Stroke()
	if b.wf == nil {
		return
	}
	spp := 1 / b.pps
	hgts := make([]float64, 0, int(x1-math.Floor(x0))+1)
	cr.SetSourceRGB(0.29, 0.62, 1)
	for x := math.Floor(x0); x < x1; x++ {
		at := b.tAt(x)
		h := 0.0
		if p := iecScale(b.wf.peak(ch, at, at+spp)); p > 0 {
			h = math.Max(1, p*full)
			cr.Rectangle(x, bot-h, 1, h)
		}
		hgts = append(hgts, h)
	}
	cr.Fill()
	cr.SetSourceRGB(0.09, 0.27, 0.52)
	for i, h := range hgts {
		if h >= 3 {
			cr.Rectangle(math.Floor(x0)+float64(i), bot-h, 1, 1)
		}
	}
	cr.Fill()
}
