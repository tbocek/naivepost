package main

import (
	"math"
	"runtime"
	"strings"
	"testing"
	"time"

	"github.com/diamondburned/gotk4/pkg/cairo"
)

// renderMask paints over a fresh surface and hands back the alpha at a point.
// The mask's whole job is which pixels it covers, and it covers them with
// opaque black on nothing -- so the colour says nothing and the alpha says
// everything. Cairo's ARGB32 is premultiplied little-endian: B, G, R, A.
func renderMask(t *testing.T, w, h int, paint func(cr *cairo.Context)) func(x, y int) uint8 {
	t.Helper()
	surf := cairo.CreateImageSurface(cairo.FormatARGB32, w, h)
	cr := cairo.Create(surf)
	paint(cr)
	surf.Flush()
	data, stride := surf.Data(), surf.Stride()
	pix := make([]byte, len(data))
	copy(pix, data)
	runtime.KeepAlive(surf)
	return func(x, y int) uint8 { return pix[y*stride+x*4+3] }
}

// The mask is the preview's claim about what the finished video contains. It
// lights the output frame and blacks out the widget around it -- and it blacks
// out the part of that frame the blown-up picture does not reach, which is what
// a camera pulled back past the edge leaves and what the render fills with its
// blurred backdrop rather than with footage.
func TestTheMaskLightsOnlyTheOutputFrameTheFootageReaches(t *testing.T) {
	// a square output in a 2:1 widget: the frame is the middle 100 px
	const W, H = 200, 100
	for _, c := range []struct {
		name          string
		sw, sh, s, tx float64
		lit, dark     [2]int
		alsoDark      [2]int
	}{
		{"the footage fills the frame", 100, 100, 1, 50,
			[2]int{100, 50}, [2]int{10, 50}, [2]int{190, 50}},
		// the same frame with the picture reaching only halfway across it: the
		// bare half is as black as the bars, because the render has no footage
		// for it either
		{"the footage reaches half of it", 50, 100, 1, 50,
			[2]int{75, 50}, [2]int{125, 50}, [2]int{10, 50}},
	} {
		at := renderMask(t, W, H, func(cr *cairo.Context) {
			fxMaskLive(cr, W, H, c.sw, c.sh, 1, c.s, c.tx, 0)
		})
		if a := at(c.lit[0], c.lit[1]); a != 0 {
			t.Errorf("%s: (%d,%d) is painted over (alpha %d), and it is footage the video keeps",
				c.name, c.lit[0], c.lit[1], a)
		}
		for _, p := range [][2]int{c.dark, c.alsoDark} {
			if a := at(p[0], p[1]); a != 255 {
				t.Errorf("%s: (%d,%d) shows through (alpha %d), and the finished video has nothing there",
					c.name, p[0], p[1], a)
			}
		}
	}
}

// An overlay is a fraction of the OUTPUT frame, never of the picture. That is
// what lets a title stay put while the camera moves under it -- and it is why
// both previews are handed the frame's rectangle rather than working it out.
func TestAnOverlaySitsOnTheOutputFrameNotOnThePicture(t *testing.T) {
	// half the frame wide, a quarter high, centred across and 60% down
	f := cutFx{Kind: "text", Cx: 0.5, Cy: 0.6, Wf: 0.5, Hf: 0.25}
	x, y, w, h := fxOverPx(f, 100, 20, 400, 200)
	if x != 200 || y != 115 || w != 200 || h != 50 {
		t.Fatalf("the box landed at %g,%g %gx%g, want 200,115 200x50", x, y, w, h)
	}
	// and it is the FRAME it is a fraction of: the same effect on a frame put
	// somewhere else moves with the frame and keeps its size
	if x2, y2, w2, h2 := fxOverPx(f, 0, 0, 400, 200); x2 != x-100 || y2 != y-20 || w2 != w || h2 != h {
		t.Errorf("moving the frame gave %g,%g %gx%g rather than the same box moved with it", x2, y2, w2, h2)
	}
	// the same effect on the same frame, asked twice with different cameras in
	// force: nothing about the camera reaches this
	if x2, y2, w2, h2 := fxOverPx(f, 100, 20, 400, 200); x2 != x || y2 != y || w2 != w || h2 != h {
		t.Error("the same box on the same frame moved between two asks")
	}
}

// The fit is refused rather than fudged when there is nothing to fit: a widget
// with no size yet, a source size nobody knows, an aspect that came back as no
// number at all. Fudged, each of those reaches gsk as a NaN matrix, which is a
// preview drawn somewhere the screen is not.
func TestTheCameraFitIsRefusedWhenThereIsNothingToFitIt(t *testing.T) {
	for _, c := range []struct {
		name               string
		W, H, sw, sh, outA float64
	}{
		{"the widget has no width yet", 0, 100, 1920, 1080, 1},
		{"the widget has no height yet", 200, 0, 1920, 1080, 1},
		{"the recording's width is unknown", 200, 100, 0, 1080, 1},
		{"the recording's height is unknown", 200, 100, 1920, 0, 1},
		{"the output shape is not a number", 200, 100, 1920, 1080, math.NaN()},
	} {
		if _, _, _, ok := fxLiveFit(c.W, c.H, c.sw, c.sh, c.outA, nil, 0); ok {
			t.Errorf("%s: the fit was handed out anyway", c.name)
		}
	}
	s, tx, ty, ok := fxLiveFit(200, 100, 1920, 1080, 1, nil, 0)
	if !ok {
		t.Fatal("a widget with a size and a recording with a size got no fit")
	}
	ws, wtx, wty := liveZoom(200, 100, 1920, 1080, 1, fxRectAt(nil, 0, 1920.0/1080, 1))
	if s != ws || tx != wtx || ty != wty {
		t.Errorf("the fit is %g/%g/%g and the camera is %g/%g/%g -- they are the same mapping or the still and the footage part company",
			s, tx, ty, ws, wtx, wty)
	}
}

// The clock the overlay draws on: the position is read ten times a second and
// the picture sixty, so what is between the reads is extrapolated -- but never
// by more than the one tick that is missing, or a jump backwards would be
// hidden along with the jitter.
func TestTheLiveClockFillsInOneTickAndNoMore(t *testing.T) {
	const span = float64(playTick) / 1000
	now := time.Now()
	for _, c := range []struct {
		name           string
		playhead, posT float64
		posAt          time.Time
		liveMax, rate  float64
		playing        bool
		want           float64
	}{
		{"stopped, the line itself is the answer", 7, 3, now, 5, 1, false, 7},
		{"never played, likewise", 7, 3, time.Time{}, 5, 1, true, 7},
		{"just read, the position stands", 0, 10, now, 0, 1, true, 10},
		{"a whole tick late, one tick is filled in", 0, 10, now.Add(-5 * time.Second), 0, 1, true, 10 + span},
		{"and at double speed, twice as much of it", 0, 10, now.Add(-5 * time.Second), 0, 2, true, 10 + 2*span},
		{"a mark just ahead of the read is not walked back", 0, 10, now, 10 + span/2, 1, true, 10 + span/2},
		{"a mark from before a seek is", 0, 10, now, 99, 1, true, 10 + span},
	} {
		got, mark := liveClock(c.playhead, c.posT, c.posAt, c.liveMax, c.rate, c.playing)
		// a millisecond of slack: now was read before the calls, and the point
		// of the test is the tenth of a second either side of it
		if math.Abs(got-c.want) > 1e-3 {
			t.Errorf("%s: the clock says %g, want %g", c.name, got, c.want)
		}
		if mark != got {
			t.Errorf("%s: the clock handed back %g and told the caller to keep %g", c.name, got, mark)
		}
	}
}

// Two pages show the finished frame, and there is now one piece of code that
// makes it: fxScreen. The Cut page had all of it first and Narrate had a bare
// GtkPicture -- no mask, no camera, no titles -- so what you narrated over was
// not what you would get. A second implementation would have drifted from this
// one within a week, so neither page has one: both build the same layers and
// both paint through the same painters.
func TestBothPreviewsPaintThroughTheOnePainter(t *testing.T) {
	for file, wants := range map[string][]string{
		// the one painter, run once, over the page's own answers
		"cut_fxscreen.go": {
			"sc, tx, ty, ok := fxLiveFit(W, H, lw, lh, outA, fx, now)",
			"fxMaskLive(cr, W, H, lw, lh, outA, sc, tx, ty)",
			"s.ed().drawFxOverlaysAt(cr, fx, now, ox, oy, ow, oh)",
		},
		// and each page hands its drawing area straight to it
		"cut_fxview.go": {
			"if !ed.paintLive(cr, w, h) {",
			"ed.drawFxOver(cr, f, alpha, ox, oy, ow, oh)",
			"return fxOverPx(f, ox, oy, ow, oh)",
		},
		"narrate.go": {
			"n.fx.paintLive(cr, w, h)",
			"n.fx.buildLayers(n, n.player.Picture, n.player.video)",
		},
	} {
		src := readSrc(t, file)
		for _, want := range wants {
			if !strings.Contains(src, want) {
				t.Errorf("%s no longer contains %q", file, want)
			}
		}
	}
	// and neither page grew a second opinion about the same picture
	for _, file := range []string{"cut_fxview.go", "narrate.go"} {
		for _, no := range []string{"fxMaskLive(", "fxLiveFit("} {
			if strings.Contains(readSrc(t, file), no) {
				t.Errorf("%s calls %s itself instead of going through fxScreen", file, no)
			}
		}
	}
	// and the arithmetic itself is in neither of them: the fit, the mask and
	// the overlay box live in cut_fxpaint.go, where a test can reach them
	// without a window
	for _, want := range []string{
		"func fxLiveFit(", "func fxMaskLive(", "func fxOverPx(", "func drawFxText(",
	} {
		if !strings.Contains(readSrc(t, "cut_fxdraw.go"), want) {
			t.Errorf("cut_fxpaint.go no longer defines %q", want)
		}
	}
}

// The dark edge round a title is ONE shape at one alpha.
//
// It is the glyph drawn again around itself -- cairo's Go binding has no text
// path to stroke -- and those copies used to be painted straight onto the
// picture at fxEdgeA each. Overlapping, they compound: the edge came out in
// bands of one, two and three copies' worth of black, and on a letter whose
// diagonals meet a stem, a K, the bands separate and read as three edges round
// one letter. A group composites the union once, which is what a stroke is and
// what the render already did.
func TestTheTitlesEdgeIsOneShapeAndNotStackedCopies(t *testing.T) {
	body := funcBody(t, "cut_fxdraw.go", `func drawFxText\(`)
	for _, want := range []string{
		"cr.PushGroup()",
		"cr.PopGroupToSource()",
		"cr.PaintWithAlpha(fxEdgeA * alpha)",
	} {
		if !strings.Contains(body, want) {
			t.Errorf("drawFxText no longer contains %q -- the edge stacks again", want)
		}
	}
	// the copies themselves must be opaque: an alpha on each is the banding,
	// grouped or not
	if strings.Contains(body, "0.85*alpha)") || strings.Contains(body, "SetSourceRGBA(0, 0, 0,") {
		t.Error("the edge copies carry an alpha of their own, which is what compounded")
	}
	// sixteen directions on a circle, not eight on a compass: the compass put
	// its diagonals at d·√2 and left gaps between them
	if fxEdgeSteps < 16 {
		t.Errorf("the edge samples %d directions, too few to be smooth on a diagonal", fxEdgeSteps)
	}
	if !strings.Contains(body, "d*math.Cos(a)") || !strings.Contains(body, "d*math.Sin(a)") {
		t.Error("the edge is back on a compass rather than a circle")
	}
	// both passes over all the lines, outline then fill, for the reason
	// textSVG gives: a descender must not land on the next line's outline
	i, j := strings.Index(body, "cr.PaintWithAlpha"), strings.Index(body, "cr.SetSourceRGBA(1, 1, 1, alpha)")
	if i < 0 || j < 0 || i > j {
		t.Errorf("the fill is not drawn after the whole outline (%d, %d)", i, j)
	}
}

// ...and the render wears the same edge, from the same number. A stroke
// straddles the outline, so its width is twice the radius the preview dilates
// by -- one constant for both, or the thumbnail and the video differ in a way
// only a screenshot shows.
func TestThePreviewAndTheRenderWearTheSameEdge(t *testing.T) {
	f := cutFx{Kind: "text", Text: "The Kenos Tower", Cx: 0.5, Cy: 0.5, Wf: 0.9, Hf: 0.3}
	const w, h = 1920, 1080
	_, _, bw, bh := f.textBox().px(w, h)
	size, lines := fitText(f.Text, bw, bh)
	if size <= 0 || len(lines) == 0 {
		t.Fatalf("the fixture does not fit: size %g, %d line(s)", size, len(lines))
	}
	svg := string(textSVG(f, w, h))
	if want := `stroke-width="` + trimNum(size*fxEdgeR*2) + `"`; !strings.Contains(svg, want) {
		t.Errorf("the render's stroke is not twice the preview's radius -- want %s", want)
	}
	if !strings.Contains(svg, `opacity="`+trimNum(fxEdgeA)+`"`) {
		t.Error("the render's edge is not fxEdgeA dark")
	}
	if !strings.Contains(svg, `stroke-linejoin="round"`) {
		t.Error("the render's edge lost its round join")
	}
}
