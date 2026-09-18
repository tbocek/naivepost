package main

import (
	"strings"
	"testing"

	"github.com/diamondburned/gotk4/pkg/cairo"
)

// A join is shown without the words an earlier join already took out.
//
// The take before it restarted inside itself ("the release is new and the
// release is new and it"), and the join before that one had already taken the
// first saying. Shown both, the model kept one of them -- and matched back, its
// answer read as two stretches, the earlier join's and its own, and was refused.
// Without them it is one stretch at the join.
func TestAWordAnEarlierJoinTookIsNotShownAgain(t *testing.T) {
	words := append(seamSaid("so here we are today. the release is new and", "t1"),
		append(seamSaid("the release is new and it shipped in may and the core of it is", "t2"),
			seamSaid("and it shipped in may and cut the fees for everyone.", "t3")...)...)
	drop := make([]bool, len(words))
	for i := 5; i < 10; i++ {
		drop[i] = true // "the release is new and" off the end of t1: the first join's cut
	}
	at := 10 + 15 // the second join: t2 -> t3
	idx, atW := seamWindow(words, drop, at)
	for _, i := range idx {
		if drop[i] {
			t.Fatalf("the window shows %q, which an earlier join took out", words[i].w)
		}
	}
	win := make([]srcWord, len(idx))
	for i, x := range idx {
		win[i] = words[x]
	}
	// the model keeps one saying of the release and drops the abandoned tail
	joined := "so here we are today. the release is new and it shipped in may and cut the fees for everyone."
	cut, why := seamCutOf(win, atW, joined)
	if why != "" {
		t.Fatalf("refused: %s", why)
	}
	var gone []string
	for _, i := range idx[atW-cut.Before : atW+cut.After] {
		gone = append(gone, words[i].w)
	}
	// the abandoned end of t2, whole; t3 keeps its (later) saying
	if got := strings.Join(gone, " "); got != "and it shipped in may and the core of it is" {
		t.Errorf("took %q", got)
	}
	// with the old window -- every word, taken or not -- the same answer is
	// two stretches, which is the refusal this replaces
	all := make([]srcWord, 0, len(words))
	all = append(all, words...)
	if _, why := seamCutOf(all, at, joined); !strings.Contains(why, "separate stretches") {
		t.Errorf("the old window was not the problem after all: %q", why)
	}
}

// Two stretches that both reach the join are two stretches.
//
// The answer left out "... look at what it made" and "then." with one word it
// kept between them. The later one used to take the earlier one's place
// silently, so the join was cut as "then." alone.
func TestTwoStretchesAtTheJoinAreTwo(t *testing.T) {
	win, at := seamFixture("so I built it, and now I can look at what it built then.", "And now I see here the result")
	_, why := seamCutOf(win, at, "so I built it, and now I can look at what it built And now I see here the result")
	if why != "" {
		t.Fatalf("one stretch at the join was refused: %s", why)
	}
	// keep "built" between two stretches, both within reach of the join
	_, why = seamCutOf(win, at, "so I built it, built And now I see here the result")
	if !strings.Contains(why, "separate stretches") {
		t.Errorf("two stretches at the join were taken as one: %q", why)
	}
}

// A compound the transcript wrote over several heard words is shown once, and
// those words go together.
func TestAFoldedWordGoesWithItsCompound(t *testing.T) {
	win := []srcWord{
		{w: "that", raw: "That", src: "t1"},
		{w: "was", raw: "was", src: "t1"},
		{w: "the", raw: "the", src: "t1"},
		{w: "proof", raw: "Proof-of-Concept-Series,", src: "t1"},
		{w: "of", raw: "", src: "t1"},
		{w: "concept", raw: "", src: "t1"},
		{w: "series", raw: "", src: "t1"},
		{w: "not", raw: "not", src: "t1"},
		{w: "yet", raw: "yet.", src: "t1"},
		{w: "not", raw: "Not", src: "t2"},
		{w: "yet", raw: "yet,", src: "t2"},
		{w: "but", raw: "but", src: "t2"},
		{w: "soon", raw: "soon.", src: "t2"},
	}
	at := 9
	if got := seamWords(win[:at]); got != "That was the Proof-of-Concept-Series, not yet." {
		t.Fatalf("shown as %q", got)
	}
	// keeping the compound keeps all four of its words
	cut, why := seamCutOf(win, at, "That was the Proof-of-Concept-Series, Not yet, but soon.")
	if why != "" {
		t.Fatalf("refused: %s", why)
	}
	if cut.Before != 2 || cut.After != 0 {
		t.Errorf("took %d and %d, want the two words of the abandoned 'not yet.' and nothing after", cut.Before, cut.After)
	}
	// a word the transcript pass took OUT is not part of the word before it:
	// it is still shown, bare, as it always was -- hiding it would change the
	// text the model reads, not just how a compound is printed
	filler := append(append([]srcWord{}, win[:3]...), srcWord{w: "well", raw: "", src: "t1"})
	if got := seamWords(filler); got != "That was the well" {
		t.Errorf("a removed filler is hidden: %q", got)
	}
	// and leaving it out takes all four, not the first alone
	cut, why = seamCutOf(win, at, "That was the Not yet, but soon.")
	if why != "" {
		t.Fatalf("refused: %s", why)
	}
	if cut.Before != 6 {
		t.Errorf("took %d off BEFORE, want the compound's four words and 'not yet.'", cut.Before)
	}
}

// A join that takes a whole recording is flagged, not refused: in the marks,
// in retakes.tsv, and in final.txt.
func TestARemovedWholeTakeIsFlagged(t *testing.T) {
	words := append(seamSaid("one two three", "t1"),
		append(seamSaid("four five", "t2"), seamSaid("six seven", "t3")...)...)
	drop := make([]bool, len(words))
	drop[3], drop[4] = true, true // all of t2
	kept := make([]bool, len(words))
	for i := range kept {
		kept[i] = !drop[i]
	}
	for i := range words {
		words[i].s, words[i].e = float64(i), float64(i)+0.5
	}
	a := &App{outDir: t.TempDir()}
	if got := a.finishedText(words, drop); got != "one two three |cut 2|whole take| six seven" {
		t.Errorf("final.txt reads %q", got)
	}
	// ...and the mark reads back as no words at all
	if toks := textTokens("one |cut 2|whole take| six"); strings.Join(toks, " ") != "one six" {
		t.Errorf("the mark is read as words: %q", toks)
	}
	marks := a.flagWholeTakes(marksFrom(words, kept), words, kept)
	if len(marks) != 1 || marks[0].Whole != "t2" {
		t.Fatalf("marks %+v, want one naming t2", marks)
	}
	if err := a.writeRetakes(marks); err != nil {
		t.Fatal(err)
	}
	if back := a.loadRetakes(); len(back) != 1 || back[0].Whole != "t2" {
		t.Errorf("retakes.tsv lost the flag: %+v", back)
	}
	// a part of a take is not a whole one
	drop[4] = false
	kept[4] = true
	if m := a.flagWholeTakes(marksFrom(words, kept), words, kept); len(m) != 1 || m[0].Whole != "" {
		t.Errorf("a part of a take was flagged: %+v", m)
	}
	// ...and the Cut page reads the flag and paints it
	if !strings.Contains(readSrc(t, "cut.go"), "wholeTakeMark(cr, v.pxOrigin, ed.xOf(v.start+v.dur), lt, ed.laneH())") {
		t.Error("the Cut page does not tint a whole take")
	}
}

// The tint is yellow and lets the picture through.
func TestAWholeTakeIsTintedYellow(t *testing.T) {
	surf := cairo.CreateImageSurface(cairo.FormatARGB32, 40, 20)
	cr := cairo.Create(surf)
	wholeTakeMark(cr, 0, 40, 0, 20)
	surf.Flush()
	data, stride := surf.Data(), surf.Stride()
	px := func(x, y int) (b, g, r, al byte) {
		o := y*stride + x*4
		return data[o], data[o+1], data[o+2], data[o+3]
	}
	// the middle: washed, not covered
	if b, g, r, al := px(20, 10); al == 0 || al > 200 || r < g/2 || b > g {
		t.Errorf("the wash is not a see-through yellow: bgra %d %d %d %d", b, g, r, al)
	}
	// the frame: solid yellow
	if b, g, r, al := px(1, 10); al < 250 || r < 200 || g < 150 || b > 80 {
		t.Errorf("the frame is not solid yellow: bgra %d %d %d %d", b, g, r, al)
	}
}

// A word the aligner put on almost no sound, well after the word before, is
// moved back onto that word's end -- and only such a word.
func TestAWordOnNoSoundIsMovedBack(t *testing.T) {
	const hz = 100.0
	lvl := make([]uint8, 800) // 8 s
	for i := 0; i < 300; i++ {
		lvl[i] = 60 // 0-3 s: speech
	}
	lvl[650], lvl[651] = 3, 3 // 6.5 s: a breath
	e := &edges{wf: &waveform{hz: hz, chans: [][]uint8{lvl}}, off: 0}
	ws := []srcWord{
		{s: 0.2, e: 0.8, w: "so"}, {s: 1.0, e: 1.5, w: "very"}, {s: 1.6, e: 2.2, w: "much"},
		{s: 2.3, e: 2.9, w: "said"},
		{s: 6.4, e: 6.6, w: "money"}, // on the breath, 3.5 s late
	}
	retimeStrays(ws, e)
	if !ws[4].stray || ws[4].s != 2.9 || ws[4].e != 2.9 {
		t.Errorf("the stray word stands at %.1f-%.1f (stray %v), want on 2.9", ws[4].s, ws[4].e, ws[4].stray)
	}
	for _, w := range ws[:4] {
		if w.stray {
			t.Errorf("%q was moved", w.w)
		}
	}
	// a quiet word right after another is ordinary speech
	ws = []srcWord{{s: 0.2, e: 0.8, w: "a"}, {s: 1.0, e: 1.5, w: "b"}, {s: 2.3, e: 2.9, w: "c"}, {s: 3.1, e: 3.3, w: "soft"}}
	lvl[310] = 3
	retimeStrays(ws, e)
	if ws[3].stray {
		t.Error("a soft word straight after another was taken for a stray")
	}
}
