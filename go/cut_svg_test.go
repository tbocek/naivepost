package main

// A drawing over the picture.
//
// "Also add to the effect to add svg -> similar to text, where it ovrlays the
// video. This is different that the insert, as the insert, cuts the video."
//
// So the whole of this file is one distinction held in two halves. A drawing
// is LIKE a title: the same bar, the same fades, the same box on the finished
// frame, the same compositing after the camera -- which is why it goes through
// the same cues and the same chain, and why the tests below mostly check that
// it is not treated as a special case. And a drawing is NOT an insert: it does
// nothing whatever to the segments, the footage under it runs on, and the cut
// is exactly as long with it as without.

import (
	"math"
	"strings"
	"testing"
)

// The bar and the label: a drawing owns T to T+Dur like everything else, and
// says which file it is, because "svg" alone in a status line is no answer
// when three of them are on the same second.
func TestADrawingOwnsItsBarAndNamesItsFile(t *testing.T) {
	f := cutFx{Kind: "svg", Src: "/home/me/art/arrow.svg", T: 12, Dur: 4, Trans: 0.5, Tout: 0.5}
	if s, e := f.fxSpan(); s != 12 || e != 16 {
		t.Errorf("a drawing's bar is %v..%v, want 12..16 -- fades live inside Dur", s, e)
	}
	if l := f.fxLabel(); !strings.Contains(l, "arrow.svg") || !strings.Contains(l, "4.0s") {
		t.Errorf("a drawing introduces itself as %q, want its file and its seconds", l)
	}
	if got := svgBase(""); got != "(no file)" {
		t.Errorf("a drawing with no file is named %q, want something rather than a gap", got)
	}
	// on the lane it wears the picture mark and its own name
	mark, label := laneLabel(f, 40)
	if mark != "svg" || label != "arrow.svg" {
		t.Errorf("the lane labels it %q/%q, want svg/arrow.svg", mark, label)
	}
}

// The other half of the sentence: an insert cuts the video, a drawing does
// not. Nothing about the segments may change because a drawing is over them.
func TestADrawingDoesNotCutTheVideo(t *testing.T) {
	segs := []cutSeg{{S: 0, E: 30}}
	fx := []cutFx{{Kind: "svg", Src: "logo.svg", T: 5, Dur: 10}}
	got := applyFx(segs, fx)
	if len(got) != 1 || got[0].S != 0 || got[0].E != 30 || got[0].Rate != 0 {
		t.Errorf("a drawing changed the cut to %+v, want the one 0..30 segment untouched", got)
	}
	if n := len(freezeCues(fx, 0, 30, 1, 30)); n != 0 {
		t.Errorf("a drawing produced %d stop stills, want none -- it holds nothing", n)
	}
}

// One predicate for both kinds of overlay, and it is the emptiness test as
// much as the kind test: a title with no words and a drawing with no file are
// inputs ffmpeg cannot open, so they must never reach a cue.
func TestOnlyOverlaysWithSomethingToShowCount(t *testing.T) {
	for _, c := range []struct {
		f    cutFx
		want bool
		why  string
	}{
		{cutFx{Kind: "text", Text: "hi"}, true, "a title with words"},
		{cutFx{Kind: "text", Text: "  \n "}, false, "a title of whitespace"},
		{cutFx{Kind: "svg", Src: "a.svg"}, true, "a drawing with a file"},
		{cutFx{Kind: "svg"}, false, "a drawing with no file"},
		{cutFx{Kind: "svg", Text: "a.svg"}, false, "words are not a file"},
		{cutFx{Kind: "zoom", Hf: 0.5}, false, "a camera move"},
		{cutFx{Kind: "speed", Rate: 2}, false, "a speed"},
	} {
		if got := overFx(c.f); got != c.want {
			t.Errorf("%s: overFx is %v, want %v", c.why, got, c.want)
		}
	}

	// and the three places that ask: what is on the cut, what is on screen at
	// a second, and what one clip has to composite
	fx := []cutFx{
		{Kind: "text", Text: "words", T: 0, Dur: 4},
		{Kind: "svg", Src: "logo.svg", T: 2, Dur: 4},
		{Kind: "svg", T: 2, Dur: 4},         // no file
		{Kind: "zoom", T: 2, Dur: 4, Hf: 1}, // not an overlay at all
	}
	if got := textsOf(fx); len(got) != 2 {
		t.Errorf("the cut has %d overlays, want the title and the drawing", len(got))
	}
	if got := textsAt(fx, 3); len(got) != 2 || got[0] != 0 || got[1] != 1 {
		t.Errorf("at 3s the picture carries %v, want the title and the drawing", got)
	}
	if got := textsAt(fx, 5); len(got) != 1 || got[0] != 1 {
		t.Errorf("at 5s the picture carries %v, want the drawing alone", got)
	}
	if got := textCues(fx, 0, 10, 1, 10); len(got) != 2 {
		t.Errorf("the clip has %d overlay cues, want 2", len(got))
	}
}

// Where a drawing goes on the finished frame. Both sides work it out here and
// nowhere else, so the preview and the render cannot drift apart.
func TestTheBoxIsMeasuredInOnePlace(t *testing.T) {
	f := cutFx{Kind: "svg", Src: "a.svg", Cx: 0.5, Cy: 0.25, Wf: 0.5, Hf: 0.2}
	x, y, w, h, ok := svgFitPx(f, 1000, 2000)
	if !ok || x != 250 || y != 300 || w != 500 || h != 400 {
		t.Errorf("the box is %d,%d %dx%d (ok=%v), want 250,300 500x400", x, y, w, h, ok)
	}
	// no frame size is the one case there is nothing to fit against
	if _, _, _, _, ok := svgFitPx(f, 0, 0); ok {
		t.Error("a box was measured against a frame of unknown size")
	}
	// a drawing placed without a box gets the middle; a title gets the lower
	// third. They are put on the picture for different reasons.
	sb := cutFx{Kind: "svg", Src: "a.svg"}.textBox()
	tb := cutFx{Kind: "text", Text: "hi"}.textBox()
	if sb != fxSvgDefault || sb.cy != 0.5 {
		t.Errorf("a drawing's default box is %+v, want the middle of the frame", sb)
	}
	if tb != fxTextDefault || tb.cy <= sb.cy {
		t.Errorf("a title's default box is %+v, want it lower than a drawing's", tb)
	}
}

// The filter graph. A title's file is already the whole frame, so it goes on at
// the origin; a drawing is the user's own file at whatever size it happens to
// be, so it is scaled to sit inside its box and centred on the axis the fit
// leaves short -- with w and h read at run time, so the centring is done with
// the size ffmpeg ends up with rather than the one we predicted.
func TestTheChainFitsADrawingIntoItsBox(t *testing.T) {
	cues := textCues([]cutFx{
		{Kind: "text", Text: "one", T: 0, Dur: 2},
		{Kind: "svg", Src: "logo.svg", T: 0, Dur: 2, Cx: 0.5, Cy: 0.5, Wf: 0.4, Hf: 0.2},
	}, 0, 10, 1, 10)
	if len(cues) != 2 {
		t.Fatalf("%d cues, want 2", len(cues))
	}
	chain, last := textChain(cues, "vcam", 4, 1000, 2000)
	if last != "vtx1" {
		t.Errorf("the picture leaves on %q, want the last overlay's label", last)
	}
	for _, want := range []string{
		// the title, unchanged: no scale, and straight over the frame
		"[4:v]format=rgba[tx0];",
		"[vcam][tx0]overlay=x=0:y=0:eof_action=pass:",
		// the drawing: fitted into 400x400 at 300,800, keeping its own shape
		"[5:v]format=rgba,scale=400:400:force_original_aspect_ratio=decrease[tx1];",
		"[vtx0][tx1]overlay=x=300+(400-w)/2:y=800+(400-h)/2:eof_action=pass:",
	} {
		if !strings.Contains(chain, want) {
			t.Errorf("the chain is missing %q\ngot: %s", want, chain)
		}
	}
	if strings.Count(chain, "force_original_aspect_ratio") != 1 {
		t.Error("a title was scaled to a box; its own file is already the frame's size")
	}
	// the fades are still the fades: a drawing comes up and goes exactly the
	// way a title does
	f := cutFx{Kind: "svg", Src: "a.svg", T: 10, Dur: 4, Trans: 1, Tout: 1}
	if a := textAlpha(f, 10.5); math.Abs(a-0.5) > 1e-9 {
		t.Errorf("half a second into a 1s fade in the drawing is %.3f opaque, want 0.5", a)
	}
	if a := textAlpha(f, 13.5); math.Abs(a-0.5) > 1e-9 {
		t.Errorf("half a second before the end the drawing is %.3f opaque, want 0.5", a)
	}
	if a := textAlpha(f, 14.5); a != 0 {
		t.Errorf("past its bar the drawing is %.3f opaque, want gone", a)
	}
}

// A vector is drawn at the size it is used at. librsvg renders a document at
// whatever size the document declares unless it is told otherwise, so without
// this a 24 px icon in a 400 px box is a 24 px icon blown up sixteen times.
func TestTheDrawingIsRasterizedAtTheSizeItIsUsed(t *testing.T) {
	src := readSrc(t, "produce.go")
	for _, want := range []string{
		// the user's own file is the input; only a title's is written out
		"file := cue.fx.Src",
		`if cue.fx.Kind != "svg" {`,
		"textSVG(cue.fx, c.boxW, c.boxH)",
		// and the box it will be used at is what librsvg is asked for
		`if _, _, bw, bh, ok := svgFitPx(cue.fx, c.boxW, c.boxH); ok {`,
		`args = append(args, "-width", strconv.Itoa(bw), "-height", strconv.Itoa(bh),`,
		`"-keep_ar", "1")`,
		// the chain is told the frame's size, or it has nothing to fit against
		`chain, last := textChain(c.texts, "vcam", txtBase, c.boxW, c.boxH)`,
	} {
		if !strings.Contains(src, want) {
			t.Errorf("produce.go no longer contains %q", want)
		}
	}
	// the preview asks for the same thing, with alpha kept: a drawing with a
	// transparent background must not come back on black
	raster := funcBody(t, "fxsvg.go", `func svgRaster\(`)
	for _, want := range []string{`"-width", fmt.Sprint(w), "-height", fmt.Sprint(h), "-keep_ar", "1"`,
		`"-pix_fmt", "rgba"`} {
		if !strings.Contains(raster, want) {
			t.Errorf("svgRaster no longer contains %q", want)
		}
	}
}

// The seams. A drawing is placed, drawn, picked up, edited and trimmed by the
// same machinery a title is; these are the places that had to learn there are
// two kinds of overlay rather than one.
func TestTheDrawingIsWiredLikeATitle(t *testing.T) {
	for file, wants := range map[string][]string{
		"cut.go": {
			// its own entry on the effects dropdown, and the file first
			`fxKinds := []string{"✚ Effect", "⊕ Zoom", "❝ Text", "▨ SVG", "⏩ Speed", "🔊 Volume", "🏷 Label"}`,
			"a.svgClicked()",
		},
		"cut_suggest.go": {
			// and it survives a cut moving under it, like a title
			`case "zoom", "text", "svg", "volume":`,
		},
		"cut_fx.go": {
			// the drawing's file rides on the effect, apart from the words
			"Src string `json:\"src,omitempty\"`",
			`case "zoom", "speed", "text", "svg", "volume", "label":`,
			"return fmt.Sprintf(\"svg %s at %s for %.1fs\", svgName(f), mmss(f.T), f.Dur)",
			// the same dialog reopens it
			"a.askSvgParams(was, false, func(nf cutFx) { ed.updateFx(was, nf) })",
			// and the lane draws it as a bracket beside the title's
			`case "text", "svg":`,
		},
		"cut_fxview.go": {
			// one arm, two kinds; one held box, two kinds; one drawing call
			"func (ed *cutEditor) fxOverArm() bool {",
			`return ed.fxArm == "text" || ed.fxArm == "svg"`,
			"func (ed *cutEditor) fxHeldBox() *cutFx {",
			"if f := ed.heldFx(); f != nil && overFx(*f) {",
			"drawOver := func(cr *cairo.Context, f cutFx, alpha float64) {",
			// placing one: the box is drawn on the picture, then the numbers
			"if ed.fxOverArm() {",
			"f.Src, b = ed.fxSrc, fxSvgDefault",
			"ed.a.askSvgParams(f, true, func(nf cutFx) {",
		},
		"cut_fxdraw.go": {
			// the drawing call itself is in the shared painter, so Narrate's
			// preview shows the same drawing in the same box
			`if f.Kind == "svg" {`,
			"ed.drawSVG(cr, f, x, y, w, h, alpha)",
		},
		"fxsvg.go": {
			// the file is chosen before the box, not after
			"a.chooseSVG(func(path string) {",
			"ed.fxSrc = path",
			`ed.armFx("svg")`,
			// and one raster per file, kept
			"ed.svgs[path] = c",
		},
	} {
		src := readSrc(t, file)
		for _, want := range wants {
			if !strings.Contains(src, want) {
				t.Errorf("%s no longer contains %q", file, want)
			}
		}
	}
}
