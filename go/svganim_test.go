package main

// Baking SMIL down to still frames, which is what makes an animated SVG usable
// as an insert without a browser in the render path (see svganim.go).
//
// Everything here is offline and pixel-free on purpose: what is being checked is
// the arithmetic of "what is this attribute at t", and the document that comes
// out of it. Whether librsvg then draws that document correctly is librsvg's
// business, and the one thing that would make it not draw it -- output that is
// no longer well-formed XML -- is checked by parsing every frame back.

import (
	"encoding/xml"
	"fmt"
	"math"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// svgAttrAt pulls one attribute out of a rendered frame, which is what almost
// every test here is really asking about.
func svgAttrAt(t *testing.T, doc, element, attr string) string {
	t.Helper()
	dec := xml.NewDecoder(strings.NewReader(doc))
	for {
		tok, err := dec.Token()
		if err != nil {
			t.Fatalf("no <%s> in the frame:\n%s", element, doc)
		}
		se, ok := tok.(xml.StartElement)
		if !ok || se.Name.Local != element {
			continue
		}
		for _, a := range se.Attr {
			if xmlName(a.Name) == attr {
				return a.Value
			}
		}
		return ""
	}
}

func renderSVGAt(t *testing.T, src string, at float64) string {
	t.Helper()
	root, err := parseSVG([]byte(src))
	if err != nil {
		t.Fatal(err)
	}
	var b strings.Builder
	root.renderAt(&b, at)
	return b.String()
}

// The plain case, and the one a tier list is made of: something slides from one
// place to another over a known time. Half way through it is half way there.
func TestAnAnimatedAttributeIsBakedAtTheRightValue(t *testing.T) {
	const doc = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">
  <rect id="row" x="-100" y="10" width="80" height="20" fill="#cc3333">
    <animate attributeName="x" from="-100" to="10" dur="2s" fill="freeze"/>
  </rect>
</svg>`
	for _, c := range []struct {
		at   float64
		want string
	}{
		{0, "-100"},
		{1, "-45"}, // half of the way from -100 to 10
		{2, "10"},  // arrived
		{9, "10"},  // ...and frozen there, which is what fill="freeze" buys
	} {
		if got := svgAttrAt(t, renderSVGAt(t, doc, c.at), "rect", "x"); got != c.want {
			t.Errorf("at %gs x=%q, want %q", c.at, got, c.want)
		}
	}
}

// Without fill="freeze" an animation stops applying when it ends and the static
// attribute comes back. Getting this backwards makes every card end on its last
// animated frame instead of its authored state -- invisible until the one
// document that relied on it looks wrong.
func TestAnUnfrozenAnimationLetsGoWhenItEnds(t *testing.T) {
	const doc = `<svg xmlns="http://www.w3.org/2000/svg">
  <circle cx="50" cy="50" r="5"><animate attributeName="r" from="10" to="40" dur="1s"/></circle>
</svg>`
	if got := svgAttrAt(t, renderSVGAt(t, doc, 0.5), "circle", "r"); got != "25" {
		t.Errorf("mid-animation r=%q, want 25", got)
	}
	if got := svgAttrAt(t, renderSVGAt(t, doc, 2), "circle", "r"); got != "5" {
		t.Errorf("after it ended r=%q, want the static 5 back", got)
	}
}

// values + keyTimes is how a real tier-list card is written: appear, hold,
// fade. The hold is the part a from/to animation cannot express.
func TestValuesAndKeyTimesFollowTheirOwnSchedule(t *testing.T) {
	const doc = `<svg xmlns="http://www.w3.org/2000/svg">
  <g opacity="0"><animate attributeName="opacity" values="0;1;1;0"
      keyTimes="0;0.25;0.75;1" dur="4s" fill="freeze"/></g>
</svg>`
	for _, c := range []struct {
		at   float64
		want string
	}{
		{0, "0"},
		{0.5, "0.5000"}, // half way into the first quarter
		{1, "1"},
		{2, "1"}, // the hold
		{3, "1"},
		{4, "0"},
	} {
		if got := svgAttrAt(t, renderSVGAt(t, doc, c.at), "g", "opacity"); got != c.want {
			t.Errorf("at %gs opacity=%q, want %q", c.at, got, c.want)
		}
	}
}

// A begin is a delay, and before it the element is exactly as authored. Rows
// that stagger in are nothing but a list of begins, so this is the whole trick
// of an animated ranking.
func TestBeginDelaysAnAnimation(t *testing.T) {
	const doc = `<svg xmlns="http://www.w3.org/2000/svg">
  <rect x="0"><animate attributeName="x" from="0" to="100" begin="2s" dur="1s" fill="freeze"/></rect>
</svg>`
	for _, c := range []struct {
		at   float64
		want string
	}{{0, "0"}, {1.9, "0"}, {2.5, "50"}, {3, "100"}, {5, "100"}} {
		if got := svgAttrAt(t, renderSVGAt(t, doc, c.at), "rect", "x"); got != c.want {
			t.Errorf("at %gs x=%q, want %q", c.at, got, c.want)
		}
	}
}

// animateTransform does not write its attribute, it composes a transform -- and
// it has to compose WITH whatever static transform the element already had, or
// an element that was positioned by a translate jumps to the origin the moment
// it starts rotating.
func TestAnimateTransformComposesWithTheStaticOne(t *testing.T) {
	const doc = `<svg xmlns="http://www.w3.org/2000/svg">
  <g transform="translate(50 50)"><animateTransform attributeName="transform" type="rotate"
      from="0" to="90" dur="1s" fill="freeze"/></g>
</svg>`
	got := svgAttrAt(t, renderSVGAt(t, doc, 0.5), "g", "transform")
	if !strings.Contains(got, "translate(50 50)") {
		t.Errorf("transform=%q lost the element's own placement", got)
	}
	if !strings.Contains(got, "rotate(45)") {
		t.Errorf("transform=%q, want the half-way rotation composed in", got)
	}
}

// Colours are the other thing a tier list animates, and they have no numbers in
// them that a number parser would find.
func TestHexColoursInterpolate(t *testing.T) {
	const doc = `<svg xmlns="http://www.w3.org/2000/svg">
  <rect fill="#000000"><animate attributeName="fill" from="#000000" to="#ffffff" dur="1s" fill="freeze"/></rect>
</svg>`
	if got := svgAttrAt(t, renderSVGAt(t, doc, 0.5), "rect", "fill"); got != "#808080" {
		t.Errorf("half way from black to white is %q, want #808080", got)
	}
	if got := svgAttrAt(t, renderSVGAt(t, doc, 1), "rect", "fill"); got != "#ffffff" {
		t.Errorf("at the end fill=%q, want #ffffff", got)
	}
}

// <set> is a step, not a slide: it holds its value from its begin onwards and
// never interpolates towards it.
func TestSetSteps(t *testing.T) {
	const doc = `<svg xmlns="http://www.w3.org/2000/svg">
  <text display="none"><set attributeName="display" to="inline" begin="1s"/>WINNER</text>
</svg>`
	if got := svgAttrAt(t, renderSVGAt(t, doc, 0.5), "text", "display"); got != "none" {
		t.Errorf("before its begin display=%q, want the static none", got)
	}
	if got := svgAttrAt(t, renderSVGAt(t, doc, 2), "text", "display"); got != "inline" {
		t.Errorf("after its begin display=%q, want inline", got)
	}
}

// An animation this cannot evaluate must not become a half-baked one. A begin
// that waits on a click has no time at all, so it is never applied and the
// document renders as authored -- which is what would have happened in any
// renderer that does not run SMIL.
func TestAnUnevaluableAnimationIsDroppedNotGuessed(t *testing.T) {
	const doc = `<svg xmlns="http://www.w3.org/2000/svg">
  <rect x="5"><animate attributeName="x" from="5" to="90" begin="click" dur="1s" fill="freeze"/></rect>
</svg>`
	for _, at := range []float64{0, 1, 5} {
		if got := svgAttrAt(t, renderSVGAt(t, doc, at), "rect", "x"); got != "5" {
			t.Errorf("at %gs x=%q, want the static 5 -- a begin nobody can resolve was acted on", at, got)
		}
	}
}

// The SMIL must not survive into the baked frame. Anything that DOES understand
// SMIL -- a browser previewing the frames, a future renderer -- would otherwise
// animate an already-animated document and play the whole thing again inside
// every single frame.
func TestTheBakedFrameCarriesNoAnimation(t *testing.T) {
	const doc = `<svg xmlns="http://www.w3.org/2000/svg">
  <rect x="0"><animate attributeName="x" from="0" to="9" dur="1s" fill="freeze"/></rect>
</svg>`
	out := renderSVGAt(t, doc, 0.5)
	if strings.Contains(out, "<animate") {
		t.Errorf("the baked frame still contains the animation element:\n%s", out)
	}
	if svgAnimated([]byte(out)) {
		t.Errorf("a baked frame reports itself as animated:\n%s", out)
	}
}

// Everything not being animated has to come out untouched -- namespaces,
// gradients, text, self-closing tags. This is a rewriter, and the failure mode
// of a rewriter is a document that renders differently for reasons nobody asked
// for.
func TestTheRestOfTheDocumentSurvives(t *testing.T) {
	const doc = `<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" viewBox="0 0 320 180">
  <defs><linearGradient id="g"><stop offset="0" stop-color="#123456"/></linearGradient></defs>
  <rect width="320" height="180" fill="url(#g)"/>
  <text x="10" y="40" font-family="sans-serif" font-size="24">S tier &amp; friends</text>
  <use xlink:href="#g" x="1"/>
  <g opacity="0"><animate attributeName="opacity" to="1" dur="1s" fill="freeze"/></g>
</svg>`
	out := renderSVGAt(t, doc, 1)
	for _, want := range []string{
		`viewBox="0 0 320 180"`,
		`<linearGradient id="g">`,
		`stop-color="#123456"`,
		`fill="url(#g)"`,
		`font-family="sans-serif"`,
		"S tier &amp; friends",
		`xlink:href="#g"`,
		`xmlns:xlink="http://www.w3.org/1999/xlink"`,
	} {
		if !strings.Contains(out, want) {
			t.Errorf("the rewrite lost %s:\n%s", want, out)
		}
	}
	// and a to-animation with no from starts from the element's own value
	if got := svgAttrAt(t, out, "g", "opacity"); got != "1" {
		t.Errorf("opacity=%q at the end of a to-animation, want 1", got)
	}
}

// The whole job, end to end: a sequence on disk that ffmpeg can be pointed at,
// every frame of it well-formed, and the movement actually different from frame
// to frame.
func TestBakeWritesAWellFormedSequence(t *testing.T) {
	const doc = `<svg xmlns="http://www.w3.org/2000/svg" width="320" height="180">
  <rect y="10" width="40" height="40" x="0">
    <animate attributeName="x" from="0" to="280" dur="2s" fill="freeze"/>
  </rect>
</svg>`
	dir := t.TempDir()
	pat, n, err := bakeSVG([]byte(doc), dir, 10, 3)
	if err != nil {
		t.Fatal(err)
	}
	if n != 30 {
		t.Errorf("3 s at 10 fps baked %d frames, want 30", n)
	}
	if want := filepath.Join(dir, "f%05d.svg"); pat != want {
		t.Errorf("pattern %q, want %q -- ffmpeg reads the sequence by this", pat, want)
	}
	// the pattern has to name files that are really there, starting at index 0:
	// ffmpeg's image2 demuxer begins at the first frame it finds and a gap ends
	// the read early
	var xs []string
	for i := 0; i < n; i++ {
		b, err := os.ReadFile(filepath.Join(dir, fmt.Sprintf("f%05d.svg", i)))
		if err != nil {
			t.Fatalf("frame %d: %v", i, err)
		}
		// well-formed, or librsvg draws nothing and ffmpeg reports a decode
		// error for a file this wrote itself
		if err := xml.Unmarshal(b, new(struct{ XMLName xml.Name })); err != nil {
			t.Fatalf("frame %d is not well-formed XML (%v):\n%s", i, err, b)
		}
		xs = append(xs, svgAttrAt(t, string(b), "rect", "x"))
	}
	if xs[0] != "0" {
		t.Errorf("the first frame is at x=%q, want the animation's start", xs[0])
	}
	if xs[0] == xs[5] || xs[5] == xs[10] {
		t.Errorf("nothing moved between frames: %v", xs[:11])
	}
	// past the end of a 2 s animation in a 3 s slot, it is frozen
	if xs[20] != xs[29] {
		t.Errorf("the frozen tail still moves: %q then %q", xs[20], xs[29])
	}
}

// svgAnimated decides whether an insert is a still or a sequence, so a false
// positive costs a pile of baked frames and a false negative silently renders
// frame 1 for the whole slot.
func TestSvgAnimatedSpotsSMILAndOnlySMIL(t *testing.T) {
	for _, c := range []struct {
		name string
		doc  string
		want bool
	}{
		{"a plain drawing", `<svg><rect x="1"/></svg>`, false},
		{"an animate", `<svg><rect><animate attributeName="x" to="2" dur="1s"/></rect></svg>`, true},
		{"a set", `<svg><rect><set attributeName="x" to="2" begin="1s"/></rect></svg>`, true},
		{"an animateTransform", `<svg><g><animateTransform attributeName="transform" type="scale" to="2" dur="1s"/></g></svg>`, true},
		{"the word animation in a title", `<svg><title>animation of a map</title></svg>`, false},
	} {
		if got := svgAnimated([]byte(c.doc)); got != c.want {
			t.Errorf("%s: svgAnimated = %v, want %v", c.name, got, c.want)
		}
	}
}

// CSS animation is the other way to write an animated SVG, and svgcss.go bakes
// it. This is the textual gate in front of that: both halves have to be in the
// file, since keyframes nobody runs animate nothing and an animation whose
// keyframes are missing has nothing to run. The second case is the one worth a
// line in the log -- a card that does not move is a bug report, and a card that
// says why it does not move is a five-second fix.
func TestACSSDocumentIsBakedAndAMissingKeyframesIsWarnedAbout(t *testing.T) {
	css := []byte(`<svg><style>@keyframes in { from { opacity: 0 } }
  rect { animation: in 1s }</style><rect/></svg>`)
	if !svgAnimated(css) {
		t.Error("a @keyframes card is drawn as a still, though the baker reads CSS now")
	}
	missing := []byte(`<svg><style>rect { animation: in 1s }</style><rect/></svg>`)
	if svgAnimated(missing) {
		t.Error("an animation with no keyframes claims to be bakeable, which would produce identical frames")
	}
	if !svgHasCSSAnimation(missing) {
		t.Error("nothing spots it, so it renders as a still with no explanation")
	}
	if svgHasCSSAnimation([]byte(`<svg><rect fill="red"/></svg>`)) {
		t.Error("an ordinary drawing is being warned about")
	}
}

// svgDuration is where an insert's default length comes from, so "as long as
// the animation" is a length the file itself decides.
func TestSvgDurationIsTheLastMomentAnythingMoves(t *testing.T) {
	root, err := parseSVG([]byte(`<svg>
  <rect><animate attributeName="x" to="1" dur="1s"/></rect>
  <rect><animate attributeName="y" to="1" begin="2s" dur="1.5s"/></rect>
</svg>`))
	if err != nil {
		t.Fatal(err)
	}
	if got := svgDuration(root); math.Abs(got-3.5) > 1e-9 {
		t.Errorf("svgDuration = %g, want 3.5 -- the later animation ends at 2+1.5", got)
	}
	// an indefinite repeat has no end, and reporting one would cut a loop off
	// mid-cycle instead of letting it fill the slot
	loop, err := parseSVG([]byte(`<svg><rect><animate attributeName="x" to="1" dur="1s" repeatCount="indefinite"/></rect></svg>`))
	if err != nil {
		t.Fatal(err)
	}
	if got := svgDuration(loop); got != 0 {
		t.Errorf("an indefinite loop reports a duration of %g, want 0 (the slot decides)", got)
	}
}

// A repeat runs its cycle again from the top rather than freezing or stopping.
func TestRepeatCountCyclesTheAnimation(t *testing.T) {
	const doc = `<svg><rect x="0"><animate attributeName="x" from="0" to="10" dur="1s" repeatCount="indefinite"/></rect></svg>`
	if got := svgAttrAt(t, renderSVGAt(t, doc, 0.5), "rect", "x"); got != "5" {
		t.Errorf("half way through the first cycle x=%q, want 5", got)
	}
	if got := svgAttrAt(t, renderSVGAt(t, doc, 3.5), "rect", "x"); got != "5" {
		t.Errorf("half way through the fourth cycle x=%q, want 5 again", got)
	}
}

func TestClockValuesReadTheSMILSpellings(t *testing.T) {
	for _, c := range []struct {
		in   string
		want float64
		ok   bool
	}{
		{"", 0, true},
		{"2s", 2, true},
		{"2", 2, true},
		{"500ms", 0.5, true},
		{"1.5s", 1.5, true},
		{"2min", 120, true},
		{"01:30", 90, true},
		{"click", 0, false},
		{"other.end+1s", 0, false},
	} {
		got, ok := clockValue(c.in, 0)
		if ok != c.ok || (ok && math.Abs(got-c.want) > 1e-9) {
			t.Errorf("clockValue(%q) = %g,%v want %g,%v", c.in, got, ok, c.want, c.ok)
		}
	}
}
