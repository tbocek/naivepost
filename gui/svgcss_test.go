package main

// CSS animation, and the one thing it has to do: come out of the baker looking
// exactly like the same card written in SMIL.
//
// The failure mode here is quiet. A card that animates in a browser and stands
// still in the render is a bug nobody can see the shape of -- the file is
// obviously right, so the render must be. So these pin the whole path: that a
// stylesheet's animation reaches the element, that it lands where the keyframes
// say at the times they say, that what is left of the stylesheet cannot undo it,
// and that everything outside the subset is left alone rather than baked wrong.

import (
	"math"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// flyInCSS is the shape a drawing tool exports: a class with an animation on it,
// a delay, an easing, and a fill mode. Written by hand here so a change in the
// generators cannot quietly change what these tests are about.
const flyInCSS = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 50">
<style>
  /* the row slides in from the right */
  @keyframes in { from { opacity: 0; transform: translateX(60px) } to { opacity: 1 } }
  .row { fill: #c33; opacity: 0; animation: in 0.8s ease-out 0.2s both }
</style>
<rect class="row" width="40" height="10"/>
</svg>`

func frameAt(t *testing.T, doc string, at float64) string {
	t.Helper()
	root, err := parseSVG([]byte(doc))
	if err != nil {
		t.Fatal(err)
	}
	var b strings.Builder
	root.renderAt(&b, at)
	return b.String()
}

// The whole point: a CSS card is a sequence, not a still.
func TestACSSAnimationIsBakedLikeASMILOne(t *testing.T) {
	if !svgAnimated([]byte(flyInCSS)) {
		t.Fatal("a @keyframes card is not seen as animated, so it renders as one held frame")
	}
	root, err := parseSVG([]byte(flyInCSS))
	if err != nil {
		t.Fatal(err)
	}
	// 0.2s of delay and 0.8s of animation: the card is done at a second, and
	// that is the length an insert offers when it is dropped in
	if d := svgDuration(root); math.Abs(d-1) > 1e-9 {
		t.Errorf("the animation runs for %v, not the delay plus the duration", d)
	}

	start := frameAt(t, flyInCSS, 0)
	if !strings.Contains(start, `opacity="0"`) || !strings.Contains(start, `transform="translate(60 0)"`) {
		t.Errorf("the first frame is not where the keyframes start:\n%s", start)
	}
	// fill-mode both, so the delay is spent off screen rather than standing
	// where it lands until its turn comes
	if delayed := frameAt(t, flyInCSS, 0.19); !strings.Contains(delayed, `transform="translate(60 0)"`) {
		t.Errorf("the card moved during its delay:\n%s", delayed)
	}
	end := frameAt(t, flyInCSS, 1)
	if !strings.Contains(end, `opacity="1"`) || !strings.Contains(end, `transform="translate(0 0)"`) {
		t.Errorf("the last frame is not where the keyframes end:\n%s", end)
	}
	// ...and it stays there: an insert is usually longer than its animation
	if held := frameAt(t, flyInCSS, 8); !strings.Contains(held, `transform="translate(0 0)"`) {
		t.Errorf("the card slides back after it landed:\n%s", held)
	}
	mid := frameAt(t, flyInCSS, 0.6)
	if strings.Contains(mid, `translate(60 0)`) || strings.Contains(mid, `translate(0 0)`) {
		t.Errorf("halfway through, the card is at one end or the other:\n%s", mid)
	}
}

// A "to" that only mentions opacity says nothing about the transform, and the
// transform's other end is then the element's own value -- an identity shaped
// like the end that IS written, or there is nothing to interpolate between.
func TestAnUnwrittenEndOfTheRunIsTheElementsOwnValue(t *testing.T) {
	for _, c := range []struct{ from, want string }{
		{"translateX(60px)", "translate(0 0)"},
		{"translate(10px, 20px)", "translate(0 0)"},
		{"scale(0.5)", "scale(1 1)"},
		{"rotate(45deg)", "rotate(0)"},
	} {
		doc := `<svg><style>@keyframes in { from { transform: ` + c.from + ` } }
			rect { animation: in 1s forwards }</style><rect/></svg>`
		if got := frameAt(t, doc, 1); !strings.Contains(got, `transform="`+c.want+`"`) {
			t.Errorf("from %s ends at something other than %s:\n%s", c.from, c.want, got)
		}
	}
	// and when the element HAS a transform, that is what the run returns to
	doc := `<svg><style>@keyframes in { from { transform: translateX(60px) } }
		rect { animation: in 1s forwards }</style><rect transform="translate(5 5)"/></svg>`
	if got := frameAt(t, doc, 1); !strings.Contains(got, `transform="translate(5 5)"`) {
		t.Errorf("the run does not return to the element's own transform:\n%s", got)
	}
}

// The stylesheet that survives into a baked frame must not be able to undo the
// baking. A rule outranks a presentation attribute, so an "opacity: 0" left
// behind is a card that never appears -- and a @keyframes left behind is a card
// that animates twice in anything that plays CSS.
func TestTheBakedFrameCarriesNoAnimationToPlayAgain(t *testing.T) {
	frame := frameAt(t, flyInCSS, 0.5)
	for _, gone := range []string{"@keyframes", "animation:", "opacity: 0"} {
		if strings.Contains(frame, gone) {
			t.Errorf("the baked frame still carries %q:\n%s", gone, frame)
		}
	}
	// what was NOT animated is still styled: this is a rewrite of the document,
	// not a replacement for it
	if !strings.Contains(frame, "fill: #c33") {
		t.Errorf("the rule lost the declarations that had nothing to do with the animation:\n%s", frame)
	}
}

// One compound selector, and no guessing past it: a selector this cannot read
// has to match nothing, because matching the wrong element animates a part of
// the card that was meant to hold still.
func TestOnlyTheSelectorsItCanReadMatch(t *testing.T) {
	for _, c := range []struct {
		sel  string
		want bool
	}{
		{"rect", true},
		{".row", true},
		{"#one", true},
		{"rect.row", true},
		{"*", true},
		{"#one.row", true},
		{"circle", false},
		{".other", false},
		{"#two", false},
		{"g .row", false},     // a combinator: not read
		{"rect:hover", false}, // a pseudo-class: not read
		{"[data-x]", false},   // an attribute selector: not read
	} {
		doc := `<svg><style>@keyframes in { from { opacity: 0 } }
			` + c.sel + ` { animation: in 1s }</style><rect id="one" class="row"/></svg>`
		got := strings.Contains(frameAt(t, doc, 0), `opacity="0"`)
		if got != c.want {
			t.Errorf("%s: matched = %v, want %v", c.sel, got, c.want)
		}
	}
}

// A baked frame is a static SVG, so a CSS transform has to come out as one an
// SVG transform attribute means. Anything that needs the box measured -- % of
// the element, em of a font -- cannot be worked out here and is refused whole.
func TestCSSTransformsBecomeSVGTransforms(t *testing.T) {
	for _, c := range []struct {
		in, want string
		ok       bool
	}{
		{"translate(10px, 20px)", "translate(10 20)", true},
		{"translate(10px)", "translate(10 0)", true},
		{"translateX(-5px)", "translate(-5 0)", true},
		{"translateY(8)", "translate(0 8)", true},
		{"scale(2)", "scale(2 2)", true},
		{"scale(2, 3)", "scale(2 3)", true},
		{"rotate(45deg)", "rotate(45)", true},
		{"rotate(0.5turn)", "rotate(180)", true},
		{"translateX(20px) scale(2)", "translate(20 0) scale(2 2)", true},
		{"none", "", true},
		{"translate(50%, 0)", "", false},
		{"translate(2em)", "", false},
		{"matrix(1,0,0,1,10,10)", "", false},
		{"skewX(10deg)", "", false},
	} {
		got, ok := cssTransform(c.in)
		if ok != c.ok {
			t.Errorf("%s: readable = %v, want %v", c.in, ok, c.ok)
			continue
		}
		if ok && got != c.want {
			t.Errorf("%s: got %q, want %q", c.in, got, c.want)
		}
	}
}

// The easing is the difference between a card that lands and a card that
// arrives: ease-out is most of the way there in the first half, ease-in is
// barely started.
func TestTheEasingIsTheOneTheStylesheetAsked(t *testing.T) {
	var linear *cssEase
	if got := linear.at(0.5); got != 0.5 {
		t.Errorf("no easing is not linear: %v", got)
	}
	out, in := cssEaseOf("ease-out"), cssEaseOf("ease-in")
	if out.at(0.5) <= 0.6 {
		t.Errorf("ease-out is at %v halfway, which is not eased out", out.at(0.5))
	}
	if in.at(0.5) >= 0.4 {
		t.Errorf("ease-in is at %v halfway, which is not eased in", in.at(0.5))
	}
	// whatever the curve, both ends are pinned, or the card starts or finishes
	// somewhere the keyframes never mentioned
	for _, e := range []*cssEase{out, in, cssEaseOf("ease"), cssEaseOf("ease-in-out"),
		cssEaseOf("cubic-bezier(0.68, -0.55, 0.27, 1.55)")} {
		if e.at(0) != 0 || e.at(1) != 1 {
			t.Errorf("a curve does not start at 0 and end at 1: %v %v", e.at(0), e.at(1))
		}
	}
	// steps() is read as linear on purpose -- a wrong easing rather than a
	// wrong picture, which is the call the SMIL side makes for spline too
	if e := cssEaseOf("steps(4, end)"); e != nil {
		t.Error("steps() is being interpreted, and it is not implemented")
	}
}

// The shorthand's values come in any order except the two times, and the name is
// whatever is left. Getting this wrong reads a duration as an iteration count,
// which is a card that flashes once and vanishes.
func TestTheAnimationShorthandReadsInAnyOrder(t *testing.T) {
	for _, c := range []struct {
		in    string
		name  string
		dur   float64
		delay float64
		iter  float64
		eased bool
	}{
		{"in 0.8s", "in", 0.8, 0, 1, false},
		{"in 0.8s ease-out 0.2s both", "in", 0.8, 0.2, 1, true},
		{"0.5s linear 2 slide", "slide", 0.5, 0, 2, false},
		{"spin 1s infinite", "spin", 1, 0, math.Inf(1), false},
		{"up 1s cubic-bezier(0.1, 0, 0.2, 1) 250ms", "up", 1, 0.25, 1, true},
	} {
		var a cssAnim
		parseAnimShorthand(&a, c.in)
		if a.name != c.name || a.dur != c.dur || a.delay != c.delay || a.iter != c.iter {
			t.Errorf("%q read as name=%q dur=%v delay=%v iter=%v", c.in, a.name, a.dur, a.delay, a.iter)
		}
		if (a.ease != nil) != c.eased {
			t.Errorf("%q: eased = %v, want %v", c.in, a.ease != nil, c.eased)
		}
	}
}

// A keyframe that does not mention a property is not a stop for that property.
// Reading it as one drags the value to whatever the element happened to have and
// back, which shows up as a jerk in the middle of an otherwise smooth slide.
func TestAKeyframeIsOnlyAStopForWhatItMentions(t *testing.T) {
	doc := `<svg><style>
		@keyframes in { 0% { opacity: 0; transform: translateX(100px) }
		                50% { opacity: 1 }
		                100% { transform: translateX(0) } }
		rect { animation: in 1s }</style><rect/></svg>`
	// halfway, the slide is halfway -- not back at the start because the 50%
	// keyframe said nothing about it
	if got := frameAt(t, doc, 0.5); !strings.Contains(got, `transform="translate(50 0)"`) {
		t.Errorf("the 50%% keyframe moved a property it never mentioned:\n%s", got)
	}
	if got := frameAt(t, doc, 0.5); !strings.Contains(got, `opacity="1"`) {
		t.Errorf("the 50%% keyframe did not reach the opacity it named:\n%s", got)
	}
}

// Outside the subset, the document has to come out as it went in: the rule keeps
// its declarations and renders statically, which is what happened before any of
// this existed.
func TestWhatItCannotBakeItLeavesAlone(t *testing.T) {
	for _, c := range []struct{ name, doc string }{
		{"keyframes that are not in the file",
			`<svg><style>rect { animation: gone 1s }</style><rect/></svg>`},
		{"a direction it cannot play",
			`<svg><style>@keyframes in { from { opacity: 0 } }
				rect { opacity: 0.5; animation: in 1s alternate }</style><rect/></svg>`},
		{"a transform it cannot measure",
			`<svg><style>@keyframes in { from { transform: translate(50%, 0) } }
				rect { animation: in 1s }</style><rect/></svg>`},
	} {
		got := frameAt(t, c.doc, 0.5)
		if strings.Contains(got, `opacity="0`) || strings.Contains(got, `transform="translate`) {
			t.Errorf("%s: baked something it does not understand:\n%s", c.name, got)
		}
		if !strings.Contains(got, "animation:") {
			t.Errorf("%s: the rule was taken apart anyway, so the card lost its styling:\n%s", c.name, got)
		}
	}
	// the first of those is the one worth a line in the log rather than silence
	miss := []byte(`<svg><style>rect { animation: gone 1s }</style><rect/></svg>`)
	if svgAnimated(miss) {
		t.Error("an animation with no keyframes claims to be bakeable, which produces identical frames")
	}
	if !svgHasCSSAnimation(miss) {
		t.Error("nothing spots it, so the card renders as a still with no explanation")
	}
}

// An animation that repeats forever has no last moment, so it repeats for as
// long as the insert is on screen -- the same rule SMIL's indefinite gets.
func TestAnEndlessAnimationTakesTheSlotsLength(t *testing.T) {
	doc := []byte(`<svg><style>@keyframes pulse { from { opacity: 0.2 } to { opacity: 1 } }
		rect { animation: pulse 0.5s infinite }</style><rect/></svg>`)
	root, err := parseSVG(doc)
	if err != nil {
		t.Fatal(err)
	}
	if d := svgDuration(root); d != 0 {
		t.Errorf("an endless animation reports a length of %v instead of leaving it to the slot", d)
	}
	dir := t.TempDir()
	pat, n, err := bakeSVG(doc, dir, 10, 2) // a two-second slot at 10 fps
	if err != nil {
		t.Fatal(err)
	}
	if n != 20 {
		t.Fatalf("baked %d frames for two seconds at 10 fps", n)
	}
	if !strings.HasSuffix(pat, "f%05d.svg") {
		t.Errorf("the pattern ffmpeg reads back is %q", pat)
	}
	// it is still pulsing in the second second, which is the whole claim
	first, err := os.ReadFile(filepath.Join(dir, "f00000.svg"))
	if err != nil {
		t.Fatal(err)
	}
	late, err := os.ReadFile(filepath.Join(dir, "f00012.svg"))
	if err != nil {
		t.Fatal(err)
	}
	if string(first) == string(late) {
		t.Error("the animation stopped after its own duration instead of repeating")
	}
}

// The stagger. A tier list puts the animation on one class and the delay on
// another, because that is the only way to write "the same fly-in, a beat
// apart" -- so the declarations have to cascade onto the ELEMENT rather than be
// read off any one rule. Read per rule, the delay rule has no animation name in
// it, is skipped, and every row arrives at once.
func TestADelayFromAnotherRuleStaggersTheRows(t *testing.T) {
	doc := `<svg><style>
		@keyframes fly { from { transform: translateX(400px) } to { transform: translateX(0) } }
		.row { animation: fly 0.5s both }
		.r2 { animation-delay: 0.5s }
		</style>
		<g class="row"><rect/></g>
		<g class="row r2"><rect/></g></svg>`
	// half a second in, the first row has landed and the second has not left
	got := frameAt(t, doc, 0.5)
	if !strings.Contains(got, `transform="translate(0 0)"`) {
		t.Errorf("the first row has not landed after its half second:\n%s", got)
	}
	if !strings.Contains(got, `transform="translate(400 0)"`) {
		t.Errorf("the second row ignored the delay its own class gave it:\n%s", got)
	}
	// and by the end both are home
	if end := frameAt(t, doc, 1); strings.Contains(end, `translate(400 0)`) {
		t.Errorf("the delayed row never arrived:\n%s", end)
	}
}

// A document can hold one animation this bakes and one it does not. The one it
// does not has to come out untouched -- its rule, and the @keyframes it names,
// since a still that renders is better than a card with a hole in it.
func TestOnlyWhatWasBakedIsTakenOutOfTheStylesheet(t *testing.T) {
	doc := `<svg><style>
		@keyframes fade { from { opacity: 0 } }
		@keyframes wobble { from { transform: translate(10%, 0) } }
		#a { animation: fade 1s both }
		#b { animation: wobble 1s both }
		</style><rect id="a"/><rect id="b"/></svg>`
	got := frameAt(t, doc, 0)
	if strings.Contains(got, "@keyframes fade") || strings.Contains(got, "#a {") {
		t.Errorf("the baked animation is still in the stylesheet, so a CSS renderer plays it twice:\n%s", got)
	}
	if !strings.Contains(got, "@keyframes wobble") || !strings.Contains(got, "animation: wobble") {
		t.Errorf("the animation that could not be baked was taken out anyway, so it does nothing at all:\n%s", got)
	}
	if !strings.Contains(got, `id="a" opacity="0"`) {
		t.Errorf("the bakeable half did not bake:\n%s", got)
	}
}
