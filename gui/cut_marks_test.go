package main

// The marks on the timeline.
//
// These were glyphs -- ⊕ ▭ ⏸ ❝ ♪ ⧉ -- and on the screenshot that came back
// every one of them was a hollow box. Everything on the timeline is painted
// with cairo's toy text API, which selects a single font face and does no
// fontconfig fallback whatever, so a character that face happens not to carry
// is drawn as tofu rather than looked for elsewhere. The toolbar was fine in
// the same screenshot because GTK buttons go through Pango, which does fall
// back; the lane, which is the one place the kind of an effect is meant to be
// readable at a glance, said nothing at all.
//
// So the marks are drawn now. These tests hold the two halves of that: the
// paths put ink where they are asked to and no two kinds look alike, and the
// words beside them stay inside what a font face is certain to have.

import (
	"os"
	"runtime"
	"strings"
	"testing"

	"github.com/diamondburned/gotk4/pkg/cairo"
)

// markInk paints one mark into a surface twice the size of its square and
// hands back the alpha at each pixel. The mark is asked for at (10, 10), so
// there is a clear margin on every side to catch a path that wanders.
const markAt = 10

func markInk(t *testing.T, kind string) func(x, y int) uint8 {
	t.Helper()
	const w, h = 30, 30
	surf := cairo.CreateImageSurface(cairo.FormatARGB32, w, h)
	cr := cairo.Create(surf)
	drawMark(cr, kind, markAt, markAt)
	surf.Flush()
	data, stride := surf.Data(), surf.Stride()
	pix := make([]byte, len(data))
	copy(pix, data) // off the C heap before the surface is collected
	runtime.KeepAlive(surf)
	return func(x, y int) uint8 { return pix[y*stride+x*4+3] }
}

// everyMark is every kind drawMark answers to. A kind the lane can ask for and
// the drawing does not know is a blank space where the answer should be, which
// is exactly the failure this whole file is about.
var everyMark = []string{"zoom", "stay", "stop", "hush", "speed", "text", "svg", "card", "sound"}

func TestEveryMarkPutsInkOnThePicture(t *testing.T) {
	for _, kind := range everyMark {
		at := markInk(t, kind)
		n := 0
		for y := 0; y < 30; y++ {
			for x := 0; x < 30; x++ {
				if at(x, y) > 64 {
					n++
				}
			}
		}
		// a mark nine pixels on a side that lands under a dozen pixels of ink
		// is not a mark, it is a smudge
		if n < 12 {
			t.Errorf("the %q mark draws %d pixels of ink, want a recognisable mark", kind, n)
		}
	}
	// and a kind nobody has drawn draws nothing rather than a placeholder
	at := markInk(t, "no such kind")
	for y := 0; y < 30; y++ {
		for x := 0; x < 30; x++ {
			if at(x, y) > 0 {
				t.Fatalf("an unknown kind put ink at %d,%d", x, y)
			}
		}
	}
}

// The mark is drawn in the square it is given, because the plate behind it and
// the words after it are both sized from that square. A path that overruns is
// a mark drawn over its own label.
func TestEveryMarkStaysInItsSquare(t *testing.T) {
	// half a line width, plus the half pixel a stroke is centred over, plus
	// the strike on the hush mark that deliberately reaches the corners
	const slack = 2.0
	lo, hi := markAt-slack, markAt+fxMarkW+slack
	for _, kind := range everyMark {
		at := markInk(t, kind)
		for y := 0; y < 30; y++ {
			for x := 0; x < 30; x++ {
				if at(x, y) == 0 {
					continue
				}
				if float64(x)+1 <= lo || float64(x) >= hi || float64(y)+1 <= lo || float64(y) >= hi {
					t.Errorf("the %q mark put ink at %d,%d, outside its %g px square at %d,%d",
						kind, x, y, fxMarkW, markAt, markAt)
				}
			}
		}
	}
}

// Two kinds that look alike are worse than no mark at all: the lane would be
// saying something and saying the wrong thing. A stop and a stop that takes
// its sound with it are the close pair, and they differ by the strike.
func TestNoTwoKindsWearTheSameMark(t *testing.T) {
	seen := map[string]string{}
	for _, kind := range everyMark {
		at := markInk(t, kind)
		var b strings.Builder
		for y := 0; y < 30; y++ {
			for x := 0; x < 30; x++ {
				if at(x, y) > 64 {
					b.WriteByte('#')
				} else {
					b.WriteByte('.')
				}
			}
		}
		if was, ok := seen[b.String()]; ok {
			t.Errorf("%q and %q draw the same mark", was, kind)
		}
		seen[b.String()] = kind
	}
}

// The rule the marks exist to keep. Every label the app composes itself has to
// live inside what a font face is certain to carry, because there is no second
// face to fall back to. Anything a mark can say, a mark says.
func TestALaneLabelOnlyUsesLettersEveryFontHas(t *testing.T) {
	drawn := map[string]bool{}
	for _, m := range everyMark {
		drawn[m] = true
	}
	for _, f := range []cutFx{
		{Kind: "zoom", Dur: 4, Cx: 0.5, Cy: 0.5, Hf: 0.5},
		{Kind: "zoom", Dur: 4, Stay: true},
		{Kind: "speed", Dur: 3.2, Rate: 8},
		{Kind: "speed", Dur: 3.2, Rate: 0.5},
		{Kind: "speed", Dur: 3.2, Rate: 0},             // a stop
		{Kind: "speed", Dur: 3.2, Rate: 0, Mute: true}, // and a silent one
	} {
		mark, label := laneLabel(f, 40)
		if !drawn[mark] {
			t.Errorf("%s ×%g asks for the %q mark, which nothing draws", f.Kind, f.Rate, mark)
		}
		for _, r := range label {
			if r > 0xff {
				t.Errorf("%s ×%g is labelled %q, which needs the glyph %q -- cairo's toy "+
					"text API has one face and no fallback, so that is a hollow box",
					f.Kind, f.Rate, label, string(r))
			}
		}
	}
	// a title is the one label the app does not compose: it is the user's own
	// words, passed through with nothing added that a font could fail to draw
	mark, label := laneLabel(cutFx{Kind: "text", Dur: 4, Text: "Finally 500! Yeah"}, 40)
	if mark != "text" {
		t.Errorf("a title wears the %q mark, want \"text\"", mark)
	}
	if label != "Finally 500! Yeah" {
		t.Errorf("a title is labelled %q, want its own words back", label)
	}
	// and a kind with no lane band of its own asks for nothing
	if m, l := laneLabel(cutFx{Kind: "nothing"}, 40); m != "" || l != "" {
		t.Errorf("an unknown kind labelled itself %q/%q", m, l)
	}
}

// The plate is what makes a label legible over a band's own fill, so it has to
// cover the mark as well as the words.
func TestTheDarkPlateCoversTheMarkAndTheWords(t *testing.T) {
	const w, h = 200, 30
	surf := cairo.CreateImageSurface(cairo.FormatARGB32, w, h)
	cr := cairo.Create(surf)
	cr.SetFontSize(10)
	markPlate(cr, 20, 20, "stop", "3.2s")
	surf.Flush()
	data, stride := surf.Data(), surf.Stride()
	pix := make([]byte, len(data))
	copy(pix, data)
	runtime.KeepAlive(surf)
	ink := func(x, y int) uint8 { return pix[y*stride+x*4+3] }

	// the plate runs from three px left of the mark to past the last word
	right := 0
	for x := 0; x < w; x++ {
		for y := 0; y < h; y++ {
			if ink(x, y) > 0 && x > right {
				right = x
			}
		}
	}
	if right < 20+int(fxMarkW)+4+10 {
		t.Errorf("the plate and its label end at x=%d, want room for a mark, a gap and 3.2s", right)
	}
	for x := 20 - 3; x <= right; x++ {
		lit := false
		for y := 0; y < h; y++ {
			if ink(x, y) > 0 {
				lit = true
			}
		}
		if !lit {
			t.Errorf("a gap in the plate at x=%d: the label is not covered end to end", x)
		}
	}
}

// The seams: nothing on the timeline writes a mark any more.
func TestTheTimelineDrawsItsMarksRatherThanWritingThem(t *testing.T) {
	for _, file := range []string{"cut.go", "cut_fx.go", "cut_audio.go", "cut_draw.go"} {
		b, err := os.ReadFile(file)
		if err != nil {
			t.Fatal(err)
		}
		for i, ln := range strings.Split(string(b), "\n") {
			if !strings.Contains(ln, "ShowText(") && !strings.Contains(ln, "plateText(") {
				continue
			}
			for _, r := range ln {
				if r > 0xff {
					t.Errorf("%s:%d draws %q with the toy text API, which has no fallback "+
						"for it -- draw it as a mark instead: %s", file, i+1, string(r), strings.TrimSpace(ln))
				}
			}
		}
	}
	pins := map[string][]string{
		"cut_fx.go": {
			// every kind passes the room its bar has, because any of them can
			// carry a name the user typed (cutFx.Label)
			"mark, label := laneLabel(f, fxLabelRoom(x0, x1))",
			"markPlate(cr, x0+3, y+fxLaneH-4, mark, label)",
		},
		"cut.go":       {`markPlate(cr, x0+4, top+th-2, "card", insName(s))`},
		"cut_audio.go": {`markPlate(cr, x0+4, y+h-6, "sound", insName(s))`},
	}
	for file, wants := range pins {
		b, err := os.ReadFile(file)
		if err != nil {
			t.Fatal(err)
		}
		for _, want := range wants {
			if !strings.Contains(string(b), want) {
				t.Errorf("%s does not contain %q", file, want)
			}
		}
	}
}

// The plates are rounded; the bands are not, and that is not a taste.
//
// A band is a MEASUREMENT: you aim at its ends with a few px of tolerance and
// trim to the frame, so a corner radius blurs the one edge the control is
// about -- and at 4 px per second a kept clip can be three pixels wide, which
// is narrower than any radius worth drawing. A plate is a label: nobody aims
// at one, none of them is ever three pixels wide, and rounded they read as
// chips laid on the footage rather than as holes cut out of it.
func TestThePlatesAreRoundedAndTheBandsAreNot(t *testing.T) {
	// both plate painters lay the same path, so there is one radius in the app
	for _, c := range []struct{ file, head string }{
		{"cut.go", `func plateText\(`},
		{"cut_draw.go", `func markPlate\(`},
	} {
		body := funcBody(t, c.file, c.head)
		if !strings.Contains(body, "platePath(cr, ") {
			t.Errorf("%s draws its own plate instead of the one shape", c.head)
		}
		if strings.Contains(body, "cr.Rectangle(") {
			t.Errorf("%s is back to a square plate", c.head)
		}
	}
	// the radius gives way on a plate too small to hold it, so what is drawn
	// is always a plate and never a lozenge
	if !strings.Contains(funcBody(t, "cut.go", `func platePath\(`),
		"r := math.Min(plateR, math.Min(w, h)/2)") {
		t.Error("a plate narrower than its own corners comes out a lozenge")
	}
	// and the bands stay square: the green over the pictures, the bar in the
	// band, an effect's own bar
	for _, c := range []struct{ file, want string }{
		{"cut.go", "cr.Rectangle(x0, st, x1-x0, lh)"},
		{"cut_selband.go", "cr.Rectangle(gx0, y+2, gx1-gx0, selBandH-4)"},
		{"cut_fx.go", "cr.Rectangle(x0, y+2, x1-x0, fxLaneH-4)"},
	} {
		if !strings.Contains(readSrc(t, c.file), c.want) {
			t.Errorf("%s no longer draws a square band (%s)", c.file, c.want)
		}
	}
}
