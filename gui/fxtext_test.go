package main

// The text effect's layout and timing: the numbers the preview draws from and
// the render writes into an SVG. Both sides call these same functions, so
// everything pinned here is pinned for both at once -- which is the point of
// fxtext.go existing at all.

import (
	"math"
	"strings"
	"testing"
)

// Words are broken to the width and the font is shrunk until they fit the
// height -- never the other way round, and never past the floor.
func TestTextIsFittedToItsBox(t *testing.T) {
	// a short title in a generous box takes the whole height in one line
	size, lines := fitText("Hello", 800, 100)
	if len(lines) != 1 || lines[0] != "Hello" {
		t.Fatalf("a short title broke into %q", lines)
	}
	if want := 100 / textLine; math.Abs(size-want) > 1e-6 {
		t.Errorf("one line in a 100 px box is %.2f px, want the box's %.2f", size, want)
	}
	// the same box with a paragraph in it: more lines, smaller type, and the
	// block still inside the box
	size2, lines2 := fitText(strings.Repeat("some words here ", 12), 800, 100)
	if len(lines2) < 2 {
		t.Fatalf("a paragraph fitted onto %d line(s)", len(lines2))
	}
	if size2 >= size {
		t.Errorf("a paragraph got size %.2f, no smaller than the one-liner's %.2f", size2, size)
	}
	if h := float64(len(lines2)) * size2 * textLine; h > 100+1e-6 {
		t.Errorf("%d lines at %.2f px stand %.2f px tall in a 100 px box", len(lines2), size2, h)
	}
	// nothing to say, or nowhere to say it, is no text at all rather than a
	// zero-sized draw the preview would have to guard against
	for _, c := range []struct {
		text string
		w, h float64
	}{
		{"", 800, 100}, {"   \n ", 800, 100}, {"hi", 0, 100}, {"hi", 800, 0},
	} {
		if s, ls := fitText(c.text, c.w, c.h); s != 0 || ls != nil {
			t.Errorf("fitText(%q, %g, %g) = %.2f, %q; want nothing", c.text, c.w, c.h, s, ls)
		}
	}
	// a box far too small for the words stops shrinking rather than vanishing
	if s, _ := fitText(strings.Repeat("x ", 200), 20, 8); s < textMinPt-1e-9 {
		t.Errorf("a hopeless box shrank to %.2f, past the %.2f floor", s, textMinPt)
	}
}

// The line breaking itself: typed newlines always break, blank lines survive,
// and one word too long for the box is split rather than left to run out of it.
func TestTextLinesBreakWhereTheyMust(t *testing.T) {
	// size 10 with the 0.58 advance is 5.8 px a character: a 58 px box is 10
	got := textLines("one two three four", 58, 10)
	for _, ln := range got {
		if len([]rune(ln)) > 10 {
			t.Errorf("line %q is %d characters, past the 10 that fit", ln, len([]rune(ln)))
		}
	}
	if strings.Join(got, " ") != "one two three four" {
		t.Errorf("the words came back as %q", got)
	}
	// a typed newline breaks even when there is room, and an empty one is kept
	if got := textLines("a\n\nb", 580, 10); len(got) != 3 || got[0] != "a" || got[1] != "" || got[2] != "b" {
		t.Errorf("typed newlines gave %q, want three lines with a blank in the middle", got)
	}
	// an unbreakable word is cut at the edge, not allowed to overflow
	for _, ln := range textLines("https://example.com/a/very/long/path", 58, 10) {
		if len([]rune(ln)) > 10 {
			t.Errorf("the long word left %q on a line", ln)
		}
	}
	if joined := strings.Join(textLines("supercalifragilistic", 58, 10), ""); joined != "supercalifragilistic" {
		t.Errorf("splitting a long word lost characters: %q", joined)
	}
}

// The block is centred in its box: the same distance above the first line as
// below the last, and one line spacing between each pair.
func TestTextBaselinesCentreTheBlock(t *testing.T) {
	const y, boxH, size, n = 100.0, 200.0, 20.0, 3
	b := textBaselines(y, boxH, size, n)
	if len(b) != n {
		t.Fatalf("%d baselines for %d lines", len(b), n)
	}
	for i := 1; i < n; i++ {
		if gap := b[i] - b[i-1]; math.Abs(gap-size*textLine) > 1e-9 {
			t.Errorf("baselines %d and %d are %.3f apart, want %.3f", i-1, i, gap, size*textLine)
		}
	}
	block := float64(n) * size * textLine
	top := b[0] - size*textAscent
	if above, below := top-y, (y+boxH)-(top+block); math.Abs(above-below) > 1e-9 {
		t.Errorf("the block sits %.2f from the top and %.2f from the bottom", above, below)
	}
	if textBaselines(y, boxH, size, 0) != nil {
		t.Error("no lines should be no baselines")
	}
}

// A box is a fraction of the OUTPUT frame, is filled in when it was never set,
// and can never be dragged off the frame or collapsed out of reach.
func TestATextBoxStaysOnTheFrame(t *testing.T) {
	if got := (cutFx{Kind: "text", Text: "hi"}).textBox(); got != fxTextDefault {
		t.Errorf("a text with no box got %+v, want the default %+v", got, fxTextDefault)
	}
	for _, c := range []fxBox{
		{cx: -5, cy: -5, wf: 0.4, hf: 0.2},
		{cx: 9, cy: 9, wf: 0.4, hf: 0.2},
		{cx: 0.5, cy: 0.5, wf: 0, hf: 0},
		{cx: 0.5, cy: 0.5, wf: 4, hf: 4},
	} {
		b := c.clamp()
		if b.wf < 0.04-1e-9 || b.wf > 1+1e-9 || b.hf < 0.03-1e-9 || b.hf > 1+1e-9 {
			t.Errorf("%+v clamped to a %.3f x %.3f box", c, b.wf, b.hf)
		}
		if b.cx-b.wf/2 < -1e-9 || b.cx+b.wf/2 > 1+1e-9 ||
			b.cy-b.hf/2 < -1e-9 || b.cy+b.hf/2 > 1+1e-9 {
			t.Errorf("%+v clamped to %+v, which is off the frame", c, b)
		}
	}
	// px is the same rectangle in a frame's own pixels
	x, y, w, h := fxBox{cx: 0.5, cy: 0.5, wf: 0.5, hf: 0.25}.px(1920, 1080)
	if x != 480 || y != 405 || w != 960 || h != 270 {
		t.Errorf("px gave %.0f,%.0f %.0fx%.0f, want 480,405 960x270", x, y, w, h)
	}
}

// The fades share the time on screen the way a zoom's glides do: never
// negative, never together longer than the text is up.
func TestTextFadesShareTheTimeOnScreen(t *testing.T) {
	for _, c := range []struct {
		f               cutFx
		wantIn, wantOut float64
	}{
		{cutFx{Dur: 4, Trans: 0.5, Tout: 0.5}, 0.5, 0.5},
		{cutFx{Dur: 4, Trans: -1, Tout: -1}, 0, 0},
		{cutFx{Dur: 2, Trans: 3, Tout: 3}, 2, 0}, // the way in wins
		{cutFx{Dur: 2, Trans: 1.5, Tout: 3}, 1.5, 0.5},
	} {
		in, out := c.f.textFades()
		if math.Abs(in-c.wantIn) > 1e-9 || math.Abs(out-c.wantOut) > 1e-9 {
			t.Errorf("%+v faded %.2f/%.2f, want %.2f/%.2f", c.f, in, out, c.wantIn, c.wantOut)
		}
	}
	// and the alpha the preview draws with follows them exactly
	f := cutFx{Kind: "text", Text: "hi", T: 10, Dur: 4, Trans: 1, Tout: 1}
	for _, c := range []struct{ t, want float64 }{
		{9.9, 0}, {10, 0}, {10.5, 0.5}, {11, 1}, {12, 1}, {13.5, 0.5}, {14, 0}, {20, 0},
	} {
		if got := textAlpha(f, c.t); math.Abs(got-c.want) > 1e-9 {
			t.Errorf("at %.1fs the text is %.2f opaque, want %.2f", c.t, got, c.want)
		}
	}
}

// Which texts are on screen at a session moment, in the order they were
// placed: the last one draws on top, so a press has to be offered it first.
func TestTextsAtIsWhatIsOnScreen(t *testing.T) {
	fx := []cutFx{
		{Kind: "zoom", T: 0, Stay: true},
		{Kind: "text", Text: "first", T: 5, Dur: 3},
		{Kind: "text", Text: "  ", T: 5, Dur: 3}, // nothing to show
		{Kind: "text", Text: "no time", T: 5},    // and no time to show it in
		{Kind: "text", Text: "second", T: 6, Dur: 3},
	}
	if got := textsAt(fx, 6.5); len(got) != 2 || got[0] != 1 || got[1] != 4 {
		t.Errorf("at 6.5s the picture shows %v, want effects 1 and 4", got)
	}
	if got := textsAt(fx, 8.5); len(got) != 1 || got[0] != 4 {
		t.Errorf("at 8.5s the picture shows %v, want only effect 4", got)
	}
	if got := textsAt(fx, 0); got != nil {
		t.Errorf("at 0s the picture shows %v, want nothing", got)
	}
	if got := textsOf(fx); len(got) != 2 {
		t.Errorf("the cut has %d showable texts, want 2", len(got))
	}
}

// The session clock mapped onto one clip's own: the same five numbers buildCam
// takes, mapped the same way, so a title and a camera move placed at the same
// second happen at the same second in the render.
func TestTextCuesLandOnTheClipsOwnClock(t *testing.T) {
	fx := []cutFx{{Kind: "text", Text: "hi", T: 100, Dur: 4, Trans: 1, Tout: 1}}

	// a clip starting at 98 s: the text comes up two seconds in
	if c := textCues(fx, 98, 10, 1, 10); len(c) != 1 {
		t.Fatalf("the text fell outside a clip that covers it: %v", c)
	} else if math.Abs(c[0].s-2) > 1e-9 || math.Abs(c[0].e-6) > 1e-9 {
		t.Errorf("the cue runs %.2f..%.2f, want 2..6", c[0].s, c[0].e)
	}
	// half speed: every session second is two output ones, fades included
	c := textCues(fx, 98, 10, 0.5, 20)
	if len(c) != 1 {
		t.Fatalf("the text was lost under slow motion")
	}
	if math.Abs(c[0].s-4) > 1e-9 || math.Abs(c[0].e-12) > 1e-9 ||
		math.Abs(c[0].fin-2) > 1e-9 || math.Abs(c[0].fout-2) > 1e-9 {
		t.Errorf("at half speed the cue is %+v, want 4..12 fading 2/2", c[0])
	}
	// a clip that starts in the middle of the text: it is already up, so the
	// fade in is only the part still owing
	c = textCues(fx, 100.5, 10, 1, 10)
	if len(c) != 1 || math.Abs(c[0].s) > 1e-9 || math.Abs(c[0].fin-0.5) > 1e-9 {
		t.Errorf("a text already up came out %+v, want 0.. with 0.5 s of fade left", c)
	}
	// and one that runs past the clip's end keeps only the fade that fits
	c = textCues(fx, 98, 5, 1, 5)
	if len(c) != 1 || math.Abs(c[0].e-5) > 1e-9 || c[0].fout != 0 {
		t.Errorf("a text cut off by the clip came out %+v, want it clipped at 5 with no fade out", c)
	}
	// nowhere near this clip
	if got := textCues(fx, 500, 10, 1, 10); got != nil {
		t.Errorf("a text 400 s away produced %v", got)
	}
	// a freeze or an insert is one session moment held: either the text covers
	// that moment and is up for the whole clip, or it is not there at all
	if got := textCues(fx, 101, 0, 1, 6); len(got) != 1 ||
		got[0].s != 0 || got[0].e != 6 || got[0].fin != 0 || got[0].fout != 0 {
		t.Errorf("over a held frame the cue is %v, want the whole clip unfaded", got)
	}
	if got := textCues(fx, 90, 0, 1, 6); got != nil {
		t.Errorf("a held frame outside the text produced %v", got)
	}
	// and a clip with no length has nothing to put anything on
	if got := textCues(fx, 98, 10, 1, 0); got != nil {
		t.Errorf("a zero-length clip produced %v", got)
	}
}

// The filter graph: one input per cue, faded on its own alpha, composited over
// what came before, and switched on for exactly its seconds -- with the commas
// inside enable= escaped, because the filtergraph parser splits on bare ones.
func TestTextChainOverlaysEveryCue(t *testing.T) {
	cues := textCues([]cutFx{
		{Kind: "text", Text: "one", T: 0, Dur: 2, Trans: 0.5, Tout: 0.5},
		{Kind: "text", Text: "two", T: 3, Dur: 2},
	}, 0, 10, 1, 10)
	if len(cues) != 2 {
		t.Fatalf("%d cues, want 2", len(cues))
	}
	chain, last := textChain(cues, "vcam", 4, 1080, 1920)
	if last != "vtx1" {
		t.Errorf("the picture leaves on %q, want the last overlay's label", last)
	}
	for _, want := range []string{
		"[4:v]format=rgba,fade=t=in:st=0.000:d=0.500:alpha=1",
		"fade=t=out:st=1.500:d=0.500:alpha=1[tx0];",
		"[vcam][tx0]overlay=x=0:y=0:eof_action=pass:enable=between(t\\,0.000\\,2.000)[vtx0];",
		"[5:v]format=rgba[tx1];",
		"[vtx0][tx1]overlay=",
	} {
		if !strings.Contains(chain, want) {
			t.Errorf("the chain is missing %q\ngot: %s", want, chain)
		}
	}
	if strings.Contains(chain, "between(t,") {
		t.Error("a comma inside enable= was left unescaped — the graph would not parse")
	}
	// no cues is no filter and the picture unchanged
	if c, l := textChain(nil, "v", 4, 1080, 1920); c != "" || l != "v" {
		t.Errorf("an empty chain gave %q, %q", c, l)
	}
}

// The SVG the render draws: the size of the finished frame, every line in it
// twice (outline then fill), centred, and with the markup escaped.
func TestTextSVGDrawsTheSameLayout(t *testing.T) {
	f := cutFx{Kind: "text", Text: "A <B> & C", T: 0, Dur: 3,
		Cx: 0.5, Cy: 0.5, Wf: 0.8, Hf: 0.2}
	got := string(textSVG(f, 1920, 1080))
	for _, want := range []string{
		`width="1920" height="1080"`, `viewBox="0 0 1920 1080"`,
		`text-anchor="middle"`, `font-weight="bold"`,
		`stroke="#000"`, `fill="#ffffff"`,
		`A &lt;B&gt; &amp; C`,
	} {
		if !strings.Contains(got, want) {
			t.Errorf("the svg is missing %q", want)
		}
	}
	// exactly two <text> per line: the outline pass and the fill pass
	size, lines := fitText(f.Text, 0.8*1920, 0.2*1080)
	if size <= 0 {
		t.Fatal("the words did not fit a box that size")
	}
	if n := strings.Count(got, "<text "); n != 2*len(lines) {
		t.Errorf("%d <text> elements for %d lines, want %d", n, len(lines), 2*len(lines))
	}
	// and it is the same layout the preview would draw: the size in the file
	// is the one fitText answers, to the digit
	if !strings.Contains(got, `font-size="`+trimNum(size)+`"`) {
		t.Errorf("the svg's font size is not fitText's %s", trimNum(size))
	}
	// a text with nothing in it is a valid empty document, not a crash
	if s := string(textSVG(cutFx{Kind: "text"}, 100, 100)); !strings.Contains(s, "</svg>") ||
		strings.Contains(s, "<text") {
		t.Errorf("an empty text produced %q", s)
	}
}
