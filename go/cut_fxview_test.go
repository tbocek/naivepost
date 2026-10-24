package main

// The overlay's slide-and-snap: the maths that pulls a moved camera rectangle
// onto the frame's edges, and the wiring that makes a press inside the held
// rectangle a slide rather than a fresh drawing.

import (
	"math"
	"os"
	"strings"
	"testing"
)

// A moved region snaps to the frame's top, left, right and bottom -- each
// axis on its own, so a corner is just both at once -- and stays put
// everywhere else. All in fractions of the source frame, exactly as the
// overlay calls it: a 9:16 crop of a 16:9 frame is wf ~0.316 wide at full
// height, so its centre pins to wf/2 on the left and 1-wf/2 on the right.
func TestAMovedRegionSnapsToTheFrameEdges(t *testing.T) {
	wf, hf := 0.316, 1.0/2 // half-height 9:16-ish window
	tx, ty := 0.02, 0.02
	for _, c := range []struct {
		name         string
		cx, cy       float64
		wantX, wantY float64
	}{
		{"left edge", wf/2 + 0.01, 0.5, wf / 2, 0.5},
		{"right edge", 1 - wf/2 - 0.01, 0.5, 1 - wf/2, 0.5},
		{"top edge", 0.5, hf/2 + 0.01, 0.5, hf / 2},
		{"bottom edge", 0.5, 1 - hf/2 - 0.01, 0.5, 1 - hf/2},
		{"corner", wf/2 + 0.01, hf/2 + 0.01, wf / 2, hf / 2},
		{"free in the middle", 0.4, 0.37, 0.4, 0.37},
		{"just out of reach", wf/2 + 0.03, 0.5, wf/2 + 0.03, 0.5},
	} {
		gx, gy := snapRectPos(c.cx, c.cy, wf, hf, tx, ty)
		if math.Abs(gx-c.wantX) > 1e-9 || math.Abs(gy-c.wantY) > 1e-9 {
			t.Errorf("%s: (%.4f, %.4f) snapped to (%.4f, %.4f), want (%.4f, %.4f)",
				c.name, c.cx, c.cy, gx, gy, c.wantX, c.wantY)
		}
	}
}

// The gesture itself, pinned in the source: a press on the rectangle's border
// resizes it, one inside it slides it (one undo for the whole of either,
// snapped through snapRectPos, persisted at the end), a press that grabbed
// nothing is still the play/pause click, and the opening-framing rule lives in
// camRectAt where preview and render both read it.
func TestTheSlideGestureIsWired(t *testing.T) {
	b, err := os.ReadFile("cut_fxview.go")
	if err != nil {
		t.Fatal(err)
	}
	src := string(b)
	for _, want := range []string{
		// one hit test answers every press, and the rectangle it offers when
		// nothing is held is the zoom actually framing the picture
		"gr := grabAt(x, y)",
		"if f := ed.fxDragTarget(); f != nil {",
		// border, then inside: resize, then move
		"g.horiz, g.vert, g.left, g.top, g.inside = fxEdges(x, y, rx, ry, rw, rh)",
		`drag.kind = "size"`,
		`drag.kind = "move"`,
		// the width follows the cut's ratio, always
		"cx, cy, h := resizeRect(px, py, drag.ax, drag.ay,",
		"f.Hf = h / dh",
		// the live zoom layer follows the hand
		"ed.syncPreviewZoom()",
		// a press that never moved is a click, and a click is play/pause --
		// wherever on the picture it landed
		"if !drag.moved {",
		"ed.toggle()",
		// framing is entered from the lane, not from the picture: the picture's
		// click is play/pause and stays that way
		"if ed.fxRectHeld() != nil {",
		// one snapshot before anything shifts, so one Undo undoes the drag
		"ed.pushUndo() // once, before anything shifts: one Undo per drag",
		// every movement runs through the edge snap
		"f.Cx, f.Cy = snapRectPos(cx, cy, wf, f.Hf, 10/dw, 10/dh)",
		// a slide that went somewhere is saved when the button lifts
		"ed.persist()",
		// and can never leave a rectangle nobody can find again
		"fxClampRect(f)",
	} {
		if !strings.Contains(src, want) {
			t.Errorf("the overlay no longer contains %q", want)
		}
	}
	b, err = os.ReadFile("cut_fx.go")
	if err != nil {
		t.Fatal(err)
	}
	if strings.Contains(string(b), "settled = fxRect{z.Cx, z.Cy, z.Hf}\n\t\t\tbreak") {
		t.Error("camRectAt opens the camera on a framing that has not happened yet")
	}
	// the lane says which effect a press would take before it is pressed
	if !strings.Contains(string(b), "ed.fxHovOn, ed.fxHov = i >= 0, i") {
		t.Error("hoverFx no longer remembers which effect the pointer is over")
	}
	if !strings.Contains(string(b), "case ed.fxHovOn && i == ed.fxHov:") {
		t.Error("drawFxLane no longer rings the effect under the pointer")
	}
	// and picking one up puts the line on it. Without this the overlay draws
	// the held zoom's rectangle over a frame from somewhere else entirely,
	// which reads as the other framings never being shown at all.
	if !strings.Contains(string(b), "ed.setPlayhead(ed.fx[i].T)") {
		t.Error("holdFx no longer moves the playhead onto the effect it picks up")
	}
	// the zoom dialog asks all three of its times, named as every other
	// effect names them
	for _, want := range []string{
		`fxNumRow("Fade in (s)"`,
		`fxNumRow("Fade out (s)"`,
		`fxNumRow("Length (s)"`,
	} {
		if !strings.Contains(string(b), want) {
			t.Errorf("the zoom dialog no longer contains %s", want)
		}
	}
}

// The live zoom maps the camera onto the output box: whatever rectangle
// fxRectAt answers at the playhead, the transformed picture puts it exactly
// over the output-shaped box centred in the widget -- its height filling the
// box, its centre on the box's centre. The default full-height slice fills
// the box's whole height with the frame's middle, sides cropped off-box.
func TestTheLiveZoomMapsTheCameraOntoTheOutputBox(t *testing.T) {
	W, H, sw, sh, outA := 800.0, 450.0, 1920.0, 1080.0, 9.0/16
	bx, by, bw, bh := fxDisp(W, H, outA)
	for _, r := range []fxRect{
		fullFill(sw/sh, outA),
		{0.4, 0.6, 0.5},
		{0.25, 0.3, 0.2},
	} {
		s, tx, ty := liveZoom(W, H, sw, sh, outA, r)
		if got := r.hf * sh * s; math.Abs(got-bh) > 1e-6 {
			t.Errorf("rect %+v: camera height maps to %.3f px, want the box's %.3f", r, got, bh)
		}
		if gx, gy := r.cx*sw*s+tx, r.cy*sh*s+ty; math.Abs(gx-(bx+bw/2)) > 1e-6 || math.Abs(gy-(by+bh/2)) > 1e-6 {
			t.Errorf("rect %+v: centre maps to (%.2f, %.2f), want the box centre (%.2f, %.2f)",
				r, gx, gy, bx+bw/2, by+bh/2)
		}
	}
	// the default slice keeps the footage at the box's own scale: the frame's
	// FULL height maps to the box's height (hf = 1, nothing shrunk), and what
	// hangs past the box's sides is the crop, centred
	s, tx, _ := liveZoom(W, H, sw, sh, outA, fullFill(sw/sh, outA))
	if math.Abs(sh*s-bh) > 1e-6 {
		t.Errorf("the default slice draws the frame %.2f px tall, want the box's %.2f", sh*s, bh)
	}
	if want := bx + bw/2 - sw*s/2; math.Abs(tx-want) > 1e-6 {
		t.Errorf("the default slice starts at x=%.2f, want %.2f — the crop must be centred", tx, want)
	}
}

// The zoom's freedom and the playing preview, pinned in the source: a zoom
// drag is the free rectangle (the camera then takes the smallest
// output-shaped window that holds it), and while the stream runs a second
// picture of the same paintable is transformed to the camera and synced from
// every place the clock moves.
func TestTheFreeZoomAndTheLivePreviewAreWired(t *testing.T) {
	b, err := os.ReadFile("cut_fxview.go")
	if err != nil {
		t.Fatal(err)
	}
	src := string(b)
	for _, want := range []string{
		"if zoomFree {",
		"hf = math.Max(rh, rw/pxAspect()) / dh",
	} {
		if !strings.Contains(src, want) {
			t.Errorf("the overlay no longer contains %q", want)
		}
	}
	// the layer itself is on the shared screen both previews are built from
	src = readSrc(t, "cut_fxscreen.go")
	for _, want := range []string{
		"over.AddOverlay(s.fxZoom)",
		"s.fxZoom.SetChildTransform(s.fxZoomPic, zoomTransform(sc, tx, ty))",
	} {
		if !strings.Contains(src, want) {
			t.Errorf("the overlay no longer contains %q", want)
		}
	}
	b, err = os.ReadFile("cut.go")
	if err != nil {
		t.Fatal(err)
	}
	if n := strings.Count(string(b), "ed.syncPreviewZoom()"); n < 2 {
		t.Errorf("the live zoom is synced from %d places in cut.go, want the redraw and the pause", n)
	}
}

// ---- the rectangle under the hand -------------------------------------

// Which zoom a drag on the picture edits. The rule has to match what the
// overlay draws, or the hand grabs one rectangle and moves another: a zoom
// whose band covers the playhead owns the picture for those seconds, otherwise
// it is the staying zoom in force -- and before the first of those, none, the
// camera being parked on the plain centred slice that belongs to no effect.
func TestTheZoomInForceIsTheOneOnScreen(t *testing.T) {
	ed := &cutEditor{fx: []cutFx{
		{Kind: "zoom", T: 55, Stay: true, Cx: 0.7, Cy: 0.5, Hf: 0.5},
		{Kind: "zoom", T: 10, Dur: 4, Cx: 0.9, Cy: 0.5, Hf: 0.4},
		{Kind: "zoom", T: 20, Stay: true, Cx: 0.3, Cy: 0.5, Hf: 0.8},
	}}
	for _, c := range []struct {
		t    float64
		want float64 // the zoom's Cx
	}{
		{12, 0.9}, // inside the close-up's band: the close-up owns the picture
		{20, 0.3}, // its own moment counts as in force
		{40, 0.3},
		{55, 0.7},
		{999, 0.7},
	} {
		f := ed.camInForce(c.t)
		if f == nil {
			t.Errorf("at %gs no zoom is in force, wanted Cx %g", c.t, c.want)
			continue
		}
		if f.Cx != c.want {
			t.Errorf("at %gs the zoom in force has Cx %g, wanted %g", c.t, f.Cx, c.want)
		}
	}
	// and ahead of every framing there is nothing in force: those seconds are
	// the centred slice, which no effect owns and no drag can reframe
	for _, tt := range []float64{0, 9.9, 19.9} {
		if f := ed.camInForce(tt); f != nil {
			t.Errorf("at %gs, before any framing, %+v was offered to drag", tt, *f)
		}
	}
	// clear of every band, with nothing that stays, the camera is parked on
	// the whole frame -- a framing that belongs to no effect and cannot be
	// dragged into one
	if f := (&cutEditor{fx: []cutFx{{Kind: "zoom", T: 0, Dur: 3}}}).camInForce(5); f != nil {
		t.Errorf("a cut with no lasting framing still offered %+v to drag", *f)
	}
}

// A camera mid-move owns no rectangle a hand can take: it is somewhere between
// two framings, and dragging it would have to snap to one end of the journey
// the moment it was touched. Holding still -- before the move, on the plateau,
// after it -- is what makes the box grabbable.
func TestACameraMidMoveIsNotSomethingToGrab(t *testing.T) {
	fx := []cutFx{{Kind: "zoom", T: 10, Dur: 4, Trans: 1, Tout: 1}}
	for _, c := range []struct {
		t    float64
		want bool
	}{{9.9, false}, {10, true}, {10.9, true}, {11, false}, {12.9, false},
		{13, true}, {13.9, true}, {14, false}} {
		if got := camMoving(fx, c.t); got != c.want {
			t.Errorf("at %gs the camera reads moving=%v, wanted %v", c.t, got, c.want)
		}
	}
	// a zoom that cuts straight in and straight out never moves at all
	if camMoving([]cutFx{{Kind: "zoom", T: 10, Dur: 4}}, 11) {
		t.Error("a zoom with no fades claims to be travelling")
	}
	// the cut's earliest framing glides like any other: it travels from the
	// centred slice the video opened on, which is a real journey
	if !camMoving([]cutFx{{Kind: "zoom", T: 10, Dur: 4, Trans: 2, Stay: true}}, 11) {
		t.Error("the first framing's glide reads as the camera standing still")
	}
	// and the whole rule: held beats everything, otherwise the framing on
	// screen, and nothing at all while the camera travels
	ed := &cutEditor{hasPlay: true, playhead: 10.5, fx: []cutFx{
		{Kind: "zoom", T: 0, Stay: true, Cx: 0.25},
		{Kind: "zoom", T: 10, Dur: 4, Trans: 1, Tout: 1, Cx: 0.9},
	}}
	if f := ed.fxDragTarget(); f != nil {
		t.Errorf("the camera mid-move still offered %+v to drag", *f)
	}
	ed.playhead = 12 // on the close-up's plateau: the close-up itself
	if f := ed.fxDragTarget(); f == nil || f.Cx != 0.9 {
		t.Errorf("on the plateau the drag target is %v, wanted the zoom itself", f)
	}
	ed.playhead = 20
	if f := ed.fxDragTarget(); f == nil || f.Cx != 0.25 {
		t.Errorf("past the zoom the drag target is %v, wanted the framing at 0", f)
	}
	ed.hasPlay = false
	if f := ed.fxDragTarget(); f != nil {
		t.Error("with no video loaded the picture still offered something to drag")
	}
}

// ---- dragging a border ------------------------------------------------

// The border drag. The anchor -- the far edge, or the far corner -- stays
// exactly where it was, the height follows the hand, and the width is always
// the height times the cut's aspect, whichever border was taken hold of.
func TestResizeRectKeepsTheAnchorAndTheRatio(t *testing.T) {
	const pxA = 16.0 / 9.0
	for _, c := range []struct {
		name                   string
		x, y, ax, ay           float64
		horiz, vert, left, top bool
		wantCx, wantCy, wantH  float64
	}{
		// right border pulled out to x=500, left edge anchored at 100
		{"right border", 500, 0, 100, 250, true, false, false, false, 300, 250, 400 / pxA},
		// left border pulled to x=100 with the right edge anchored at 500
		{"left border", 100, 0, 500, 250, true, false, true, false, 300, 250, 400 / pxA},
		// bottom border: the height is the hand, the width follows
		{"bottom border", 0, 400, 300, 100, false, true, false, false, 300, 250, 300},
		{"top border", 0, 100, 300, 400, false, true, false, true, 300, 250, 300},
		// a corner takes the longer axis: 400 px across is 225 tall, less
		// than the 300 the hand moved down, so the height wins
		{"corner, height wins", 500, 400, 100, 100, true, true, false, false,
			100 + 300*pxA/2, 250, 300},
		// and the other way round: 800 across is 450 tall, more than 300
		{"corner, width wins", 900, 400, 100, 100, true, true, false, false,
			500, 100 + 800/pxA/2, 800 / pxA},
	} {
		cx, cy, h := resizeRect(c.x, c.y, c.ax, c.ay, pxA, c.horiz, c.vert, c.left, c.top)
		if math.Abs(cx-c.wantCx) > 1e-6 || math.Abs(cy-c.wantCy) > 1e-6 || math.Abs(h-c.wantH) > 1e-6 {
			t.Errorf("%s: got centre (%g, %g) height %g, wanted (%g, %g) %g",
				c.name, cx, cy, h, c.wantCx, c.wantCy, c.wantH)
			continue
		}
		// the anchor really did not move
		w := h * pxA
		l, r, top0, bot := cx-w/2, cx+w/2, cy-h/2, cy+h/2
		if c.horiz {
			held := l // the right border was taken, so the left one is anchored
			if c.left {
				held = r
			}
			if math.Abs(held-c.ax) > 1e-6 {
				t.Errorf("%s: the anchored edge moved from %g to %g", c.name, c.ax, held)
			}
		}
		if c.vert {
			held := top0
			if c.top {
				held = bot
			}
			if math.Abs(held-c.ay) > 1e-6 {
				t.Errorf("%s: the anchored edge moved from %g to %g", c.name, c.ay, held)
			}
		}
	}
	// a rectangle dragged to nothing stops at a size a hand can still find
	if _, _, h := resizeRect(100, 100, 100, 100, pxA, true, true, false, false); h != 12 {
		t.Errorf("a drag back onto the anchor left a %g px rectangle", h)
	}
}

// ---- snapping to the cuts ---------------------------------------------

// Dragging an effect along the lane snaps it to the clip borders -- the green
// edges -- because that is where a view almost always belongs: on the cut, not
// four frames after it. Nudging with ‹f f› is deliberately left unsnapped.
func TestSnapFxTimeTakesTheNearestCutBorder(t *testing.T) {
	segs := []cutSeg{{S: 10, E: 20}, {S: 55, E: 90}}
	for _, c := range []struct{ in, want float64 }{
		{9.8, 10},  // just before a border
		{20.4, 20}, // just after one
		{54.6, 55},
		{30, 30}, // nowhere near one: left alone
		{20.9, 20.9},
	} {
		if got := snapFxTime(segs, nil, c.in, 0.5); got != c.want {
			t.Errorf("%gs snapped to %gs, wanted %gs", c.in, got, c.want)
		}
	}
	// between two borders inside the snap, the nearer one wins
	if got := snapFxTime([]cutSeg{{S: 10, E: 11}}, nil, 10.6, 2); got != 11 {
		t.Errorf("between two borders it snapped to %gs, wanted 11s", got)
	}
	// and the pixel snap is a pixel distance turned into seconds by the zoom
	ed := &cutEditor{pps: 40, segs: segs}
	if got := ed.snapFx(20 + 7/40.0); got != 20 {
		t.Errorf("7 px from a border it snapped to %gs, wanted 20s", got)
	}
	if got := ed.snapFx(20 + 9/40.0); got == 20 {
		t.Error("9 px from a border it still snapped, which is further than snapPx")
	}
}

// A moved effect also snaps to the OTHER effects' band edges -- a title that
// comes up the moment the zoom lands is aimed at the zoom, not at a cut --
// and a band slid along whole offers both its ends, so it can be put flush
// against a neighbour on either side.
func TestAMovedEffectSnapsToTheOtherEffects(t *testing.T) {
	ed := &cutEditor{pps: 40, segs: []cutSeg{{S: 0, E: 200}}}
	ed.fx = []cutFx{
		{Kind: "zoom", T: 100, Dur: 5},                        // band [100, 105]
		{Kind: "zoom", T: 40, Trans: 1, Dur: 5, Tout: 1},      // band [40, 45]
		{Kind: "text", T: 60, Dur: 4, Text: "hi", Trans: 0.5}, // the one being moved
	}
	ed.fxOn, ed.fxSel = true, 2

	// a single dragged moment lands on another band's edge within reach
	if got := ed.snapFx(105.1); got != 105 {
		t.Errorf("near the zoom's end it snapped to %gs, wanted 105s", got)
	}
	if got := ed.snapFx(39.9); got != 40 {
		t.Errorf("near the zoom's start it snapped to %gs, wanted 40s", got)
	}
	// the held effect's own edges are not marks: standing just off its own
	// start must not snag it back where it already is
	if got := ed.snapFx(60.1); got != 60.1 {
		t.Errorf("the held text snagged on its own edge: %gs", got)
	}

	// slid whole, the TRAILING end snaps too: the 4 s text dragged so its end
	// sits just short of the zoom's start lands flush against it
	if got := ed.snapFxSpan(95.9, 4); math.Abs(got-96) > 1e-9 {
		t.Errorf("the band's end near the zoom's start left T at %gs, wanted 96s", got)
	}
	// and the leading end still wins when it is the closer fit
	if got := ed.snapFxSpan(104.95, 4); math.Abs(got-105) > 1e-9 {
		t.Errorf("the band's start near the zoom's end left T at %gs, wanted 105s", got)
	}
	// out of reach of everything, the hand keeps what it has
	if got := ed.snapFxSpan(80, 4); got != 80 {
		t.Errorf("clear of every landmark it moved to %gs, wanted 80s", got)
	}

	// the selection band shares the landmarks: its snapMarks now carry every
	// effect's edges, so "cut away exactly where the zoom ends" is a snap too
	marks := ed.snapMarks()
	var has105 bool
	for _, m := range marks {
		if m == 105 {
			has105 = true
		}
	}
	if !has105 {
		t.Error("the selection's snapMarks do not carry the zoom band's end")
	}
}

// What a point on a rectangle has hold of: the borders within reach, and
// whether it is inside at all. The reach is the same on both sides of the
// line, because a 1.5 px border is not a mouse target.
func TestTheBorderUnderThePointerIsKnown(t *testing.T) {
	const rx, ry, rw, rh = 100.0, 50.0, 200.0, 100.0
	for _, c := range []struct {
		name                           string
		x, y                           float64
		horiz, vert, left, top, inside bool
	}{
		{"the middle", 200, 100, false, false, false, false, true},
		{"the left border", 100, 100, true, false, true, false, true},
		{"just outside the left border", 95, 100, true, false, true, false, false},
		{"the right border", 300, 100, true, false, false, false, true},
		{"the top border", 200, 50, false, true, false, true, true},
		{"the bottom border", 200, 150, false, true, false, false, true},
		{"the top-left corner", 100, 50, true, true, true, true, true},
		{"the bottom-right corner", 300, 150, true, true, false, false, true},
		{"nowhere near it", 500, 400, false, false, false, false, false},
	} {
		h, v, l, tp, in := fxEdges(c.x, c.y, rx, ry, rw, rh)
		if h != c.horiz || v != c.vert || in != c.inside ||
			(h && l != c.left) || (v && tp != c.top) {
			t.Errorf("%s: horiz=%v vert=%v left=%v top=%v inside=%v, want %v %v %v %v %v",
				c.name, h, v, l, tp, in, c.horiz, c.vert, c.left, c.top, c.inside)
		}
	}
	// and the pointer says so before anything is pressed
	for _, c := range []struct {
		horiz, vert, left, top, inside bool
		want                           string
	}{
		{true, true, true, true, true, "nwse-resize"},   // top-left
		{true, true, false, false, true, "nwse-resize"}, // bottom-right
		{true, true, false, true, true, "nesw-resize"},  // top-right
		{true, true, true, false, true, "nesw-resize"},  // bottom-left
		{true, false, true, false, true, "ew-resize"},
		{false, true, false, true, true, "ns-resize"},
		{false, false, false, false, true, "move"},
		{false, false, false, false, false, "default"},
	} {
		if got := fxCursorName(c.horiz, c.vert, c.left, c.top, c.inside); got != c.want {
			t.Errorf("%+v gave the %q cursor, want %q", c, got, c.want)
		}
	}
}

// A text box's resize: the two axes independent (it owes nothing to the cut's
// aspect), the edge opposite the one grabbed standing still, an edge that was
// not grabbed not moving at all, and never collapsing below the minimum.
func TestResizeFreeMovesOnlyTheEdgesGrabbed(t *testing.T) {
	const x0, y0, w0, h0, min = 100.0, 50.0, 200.0, 100.0, 16.0
	// the right border pulled out: the left edge stays, the height is untouched
	nx, ny, nw, nh := resizeFree(400, 999, x0, y0, x0, y0, w0, h0, min, true, false, false, false)
	if nx != x0 || ny != y0 || nh != h0 || nw != 300 {
		t.Errorf("the right border gave %.0f,%.0f %.0fx%.0f, want 100,50 300x100", nx, ny, nw, nh)
	}
	// the left border pulled out: the right edge (x0+w0 = 300) stays put
	nx, _, nw, _ = resizeFree(40, 0, x0+w0, y0+h0, x0, y0, w0, h0, min, true, false, true, false)
	if nx != 40 || nx+nw != 300 {
		t.Errorf("the left border gave x=%.0f w=%.0f; the right edge moved to %.0f, want 300",
			nx, nw, nx+nw)
	}
	// the top-left corner: both axes move, the far corner stands still
	nx, ny, nw, nh = resizeFree(40, 20, x0+w0, y0+h0, x0, y0, w0, h0, min, true, true, true, true)
	if nx+nw != 300 || ny+nh != 150 {
		t.Errorf("the corner drag moved the far corner to %.0f,%.0f, want 300,150", nx+nw, ny+nh)
	}
	// dragged inside out: it stops at the minimum instead of inverting
	_, _, nw, nh = resizeFree(x0+500, y0+500, x0, y0, x0, y0, w0, h0, min, true, true, true, true)
	if nw < min || nh < min {
		t.Errorf("an inverted drag left a %.0fx%.0f box, want nothing below %.0f", nw, nh, min)
	}
	_, _, nw, nh = resizeFree(x0-500, y0-500, x0, y0, x0, y0, w0, h0, min, true, true, false, false)
	if nw < min || nh < min {
		t.Errorf("the other inversion left a %.0fx%.0f box, want nothing below %.0f", nw, nh, min)
	}
}

// A camera rectangle is left somewhere a person can find it again: a pull-back
// well past the frame and a tight crop both survive, a sliver and a rectangle
// dragged into the void do not.
func TestACameraRectangleCannotBeLost(t *testing.T) {
	for _, c := range []cutFx{
		{Kind: "zoom", Hf: 0, Cx: 0.5, Cy: 0.5},
		{Kind: "zoom", Hf: -3, Cx: -99, Cy: 99},
		{Kind: "zoom", Hf: 1e6, Cx: 1e6, Cy: -1e6},
	} {
		f := c
		fxClampRect(&f)
		if f.Hf < 0.02 || f.Hf > 12 || f.Cx < -2 || f.Cx > 3 || f.Cy < -2 || f.Cy > 3 {
			t.Errorf("%+v clamped to %+v, which is still off the map", c, f)
		}
	}
	// the ordinary ones are left exactly alone
	for _, c := range []cutFx{
		{Kind: "zoom", Hf: 1, Cx: 0.5, Cy: 0.5},
		{Kind: "zoom", Hf: 0.2, Cx: 0.1, Cy: 0.9},
		{Kind: "zoom", Hf: 2.5, Cx: 0.5, Cy: 0.5}, // a pull back past the frame
	} {
		f := c
		fxClampRect(&f)
		if f != c {
			t.Errorf("%+v was changed to %+v", c, f)
		}
	}
	fxClampRect(nil) // and nothing is not a crash
}

// The framed preview does not depend on the stream running. It used to, and
// the symptom was the one thing a preview must never do: change size when you
// pause it. Pausing dropped the transformed layer, so the picture fell back to
// the whole source frame letterboxed into the widget -- smaller, uncropped,
// and framed by nothing at all.
//
// Pinned as source because the condition is the whole behaviour: what may
// still switch the layer off is a card standing in for the footage, no camera
// in the cut, or a hand framing by eye. Not the transport.
func TestThePreviewIsFramedPausedAndPlaying(t *testing.T) {
	// syncCamLayer is syncPreviewZoom's own half; the other half puts a stop's
	// frozen frame on the same transform (cut_stillcam_test.go)
	src := funcBody(t, "cut_fxscreen.go", `func \(s \*fxScreen\) syncCamLayer\(\)`)
	if strings.Contains(src, "p.playing") {
		t.Error("the camera layer is gated on playback again: the preview will resize on pause")
	}
	for _, want := range []string{
		"!p.still",                      // a card is showing instead of the footage
		"fxHasCamera(ed.aspect, ed.fx)", // nothing to frame with
		"s.page.fxCamOK()",              // and whatever the page itself says
	} {
		if !strings.Contains(src, want) {
			t.Errorf("the camera layer no longer tests %q", want)
		}
	}
	// the Cut page's own half of that answer: a hand framing by eye
	cam := funcBody(t, "cut_fxview.go", `func \(ed \*cutEditor\) fxCamOK\(\)`)
	for _, want := range []string{
		`ed.fxArm == ""`,         // an armed button is about to draw a box
		"ed.fxRectHeld() == nil", // and a held box is framed against the full frame
	} {
		if !strings.Contains(cam, want) {
			t.Errorf("the Cut page's camera answer no longer tests %q", want)
		}
	}
	// and the still follows it rather than being gated on its own terms
	if fit := funcBody(t, "cut_fxscreen.go", `func \(s \*fxScreen\) fitStill\(\)`); strings.Contains(fit, "p.playing") {
		t.Error("the still layer is gated on playback: the frozen frame will jump on pause")
	}
}

// A zoom that is held for a while and then comes back off its region. The fade
// out is the fade in run backwards: back to the framing it departed from, over
// the seconds asked for, starting only once the arrival and the hold are both
// done with.
func TestAZoomComesBackOffItsRegion(t *testing.T) {
	// 16:9 source, 16:9 out, so the whole frame is the full rectangle and the
	// numbers below are readable on their own
	zooms := []cutFx{{Kind: "zoom", T: 10, Cx: 0.25, Cy: 0.5, Hf: 0.5, Trans: 2, Dur: 8, Tout: 2}}
	at := func(t float64) fxRect { return camRectAt(zooms, t, 16.0/9, 16.0/9) }

	// before it: the whole frame. A zoom that gives the camera back is not a
	// choice of framing, so it leaves the opening seconds alone
	if got := at(0); got.hf != 1 || got.cx != 0.5 {
		t.Errorf("before the zoom: %+v, want the whole frame", got)
	}
	// halfway in: halfway between the whole frame and the region
	if got := at(11); math.Abs(got.hf-0.75) > 1e-9 || math.Abs(got.cx-0.375) > 1e-9 {
		t.Errorf("halfway in: %+v, want hf 0.75 cx 0.375", got)
	}
	// held: still exactly the rectangle, right up to the last moment
	if got := at(15.9); got.hf != 0.5 || got.cx != 0.25 {
		t.Errorf("during the hold: %+v, want the zoom's own rectangle", got)
	}
	// halfway out: halfway between the rectangle and the whole frame
	mid := at(17)
	if math.Abs(mid.hf-0.75) > 1e-9 || math.Abs(mid.cx-0.375) > 1e-9 {
		t.Errorf("halfway out: %+v, want hf 0.75 cx 0.375", mid)
	}
	// and afterwards, the whole frame, for good
	for _, tt := range []float64{18, 30, 600} {
		if got := at(tt); math.Abs(got.hf-1) > 1e-9 || math.Abs(got.cx-0.5) > 1e-9 {
			t.Errorf("at %.0fs: %+v, want the whole frame", tt, got)
		}
	}
	// one told to stay never lets go: it holds until the next one says
	// otherwise, however long that is
	plain := []cutFx{{Kind: "zoom", T: 10, Cx: 0.25, Cy: 0.5, Hf: 0.5, Stay: true}}
	if got := camRectAt(plain, 600, 16.0/9, 16.0/9); got.hf != 0.5 {
		t.Errorf("a staying zoom let go anyway: %+v", got)
	}
}

// A zoom placed on top of another one that is still moving picks the camera up
// from where it actually is, whether that other one was fading in or coming
// back out. Without this the two fight over the same second and the picture
// jumps.
func TestAZoomInterruptsAnotherComingBack(t *testing.T) {
	zooms := []cutFx{
		{Kind: "zoom", T: 0, Cx: 0.25, Cy: 0.5, Hf: 0.5, Dur: 4, Tout: 4},
		{Kind: "zoom", T: 2, Cx: 0.75, Cy: 0.5, Hf: 0.5, Trans: 2, Dur: 2},
	}
	// at t=2 the first is halfway back out (hf 0.75); the second starts its
	// fade from there, so at t=3 it is halfway between 0.75 and its own 0.5
	got := camRectAt(zooms, 3, 16.0/9, 16.0/9)
	if math.Abs(got.hf-0.625) > 1e-9 {
		t.Errorf("interrupted move: hf %.4f, want 0.625 -- the second zoom did not start from where the camera was", got.hf)
	}
}

// Picking an aspect places the framing it asks for.
//
// A cut with a shape of its own has one question outstanding from the moment
// the shape is picked: which slice of the recording the finished video shows.
// The render has always had an answer -- the centred window that fills the
// frame (fullFill) -- and it was arrived at invisibly, by a rule nothing on the
// page states, so the lane said nothing and the preview's outline was the only
// clue there was a decision at all. Now the answer is an effect: a staying zoom
// at the beginning, centred, exactly that window.
func TestPickingAnAspectPlacesTheFramingItAsksFor(t *testing.T) {
	ed := newTestEd(t)
	ed.vids = []tlVideo{{base: "a", path: "a.mkv", start: 0, dur: 300, w: 1920, h: 1080}}
	ed.segs = []cutSeg{{S: 30, E: 90}}

	ed.aspectChanged("9:16")
	if len(ed.fx) != 1 {
		t.Fatalf("picking an aspect left %d effect(s), want the framing it asks for", len(ed.fx))
	}
	f := ed.fx[0]
	want := fullFill(1920.0/1080.0, parseAspect("9:16"))
	if f.Kind != "zoom" || !f.Stay {
		t.Errorf("the framing is %+v, want a staying zoom", f)
	}
	if f.T != 0 || f.Dur != aspectStayLn {
		t.Errorf("it runs %g–%g s, want a second from the start", f.T, f.T+f.Dur)
	}
	if f.Cx != want.cx || f.Cy != want.cy || f.Hf != want.hf {
		t.Errorf("its window is %g,%g h=%g, want the centred full fill %g,%g h=%g",
			f.Cx, f.Cy, f.Hf, want.cx, want.cy, want.hf)
	}
	if f.Trans != 0 || f.Tout != 0 {
		t.Error("the framing drifts in or out; it is where the video starts, not a move")
	}
	// one press, one step back: the shape and its framing go together
	if len(ed.undo) != 1 {
		t.Errorf("picking an aspect left %d undo step(s), want 1", len(ed.undo))
	}
	ed.undoLast()
	if ed.aspect != "" || len(ed.fx) != 0 {
		t.Errorf("undo left aspect %q and %d effect(s)", ed.aspect, len(ed.fx))
	}

	// a cut that is already framed has answered the question: a second answer
	// at second nought would outrank it for every clip before the first one
	ed.fx = []cutFx{{Kind: "zoom", T: 40, Dur: 3, Cx: 0.3, Cy: 0.5, Hf: 0.5, Stay: true}}
	ed.aspectChanged("1:1")
	if len(ed.fx) != 1 || ed.fx[0].T != 40 {
		t.Errorf("a framed cut was given a second framing: %+v", ed.fx)
	}
	// and going back to the source's own shape places nothing at all
	ed.fx = nil
	ed.aspectChanged("")
	if len(ed.fx) != 0 {
		t.Errorf("the source's own shape placed %+v", ed.fx)
	}
	// the placement is the CHOICE's, not the setter's: setAspect runs on every
	// project load and every undo, and an effect placed there would breed
	set := funcBody(t, "cut_fx.go", `func \(ed \*cutEditor\) setAspect\(`)
	if strings.Contains(set, "Stay: true") {
		t.Error("loading a project places a framing")
	}
}
