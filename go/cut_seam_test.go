package main

// The seams a reply does not mean to open, and the pictures that must not beat
// a pause.
//
// Both are about the same fault from opposite ends: the timeline is stamped in
// whole seconds, so the finest cut a reply can express is one second, and our
// own snapping then decides what that second means. One run asked for 21 holes,
// eleven of them exactly one second, and eight of those eleven had a word in
// the middle of them.

import (
	"math"
	"strings"
	"testing"
)

func seamRows() []tsvRow {
	return []tsvRow{
		{s: 80, e: 86.4, spk: "SPEAKER_00", text: "The team did not invent"},
		{s: 86.4, e: 87.2, spk: "SPEAKER_00", text: "new algorithms."},
		{s: 87.2, e: 95, spk: "SPEAKER_00", text: "They used the general number field sieve."},
		{s: 96, e: 104, spk: "EVENT", text: "Calm; the article stays open."},
		{s: 120, e: 128, spk: "SPEAKER_00", text: "and it finished in around three weeks."},
	}
}

// A one-second hole with a word in it is the model saying "this carries on".
// Closing it is a repair: without it the word is simply missing from the video,
// mid-sentence, with the cut keeping both sides of it.
func TestASeamWithAWordInItIsClosed(t *testing.T) {
	rows := seamRows()
	got := joinSeams([]cutSeg{{S: 80, E: 86}, {S: 87, E: 95}}, rows)
	if len(got) != 1 || got[0].S != 80 || got[0].E != 95 {
		t.Fatalf("the seam over %q was left open: %+v", "new algorithms.", got)
	}

	// ...and one with nothing but silence in it may well be a trim: those
	// seconds are seconds nobody spoke in, and the reply is allowed to mean it
	quiet := joinSeams([]cutSeg{{S: 80, E: 95}, {S: 96, E: 104}}, rows)
	if len(quiet) != 2 {
		t.Errorf("a silent seam was closed as if it were a rounding slip: %+v", quiet)
	}

	// a real hole stays a real hole, however much is said inside it
	big := joinSeams([]cutSeg{{S: 80, E: 95}, {S: 120, E: 128}}, rows)
	if len(big) != 2 {
		t.Errorf("a %g s hole was closed: %+v", 120-95.0, big)
	}

	// and a cut of one clip comes back as it went in
	if one := joinSeams([]cutSeg{{S: 80, E: 95}}, rows); len(one) != 1 {
		t.Errorf("a single clip did not survive: %+v", one)
	}
	// the suggestion goes through it before its edges are placed, so the
	// repair is deliberate rather than a side effect of two edges happening to
	// snap to the same pause
	src := readSrc(t, "cut_suggest.go")
	i, j := strings.Index(src, "segs = joinSeams(segs, rows)"), strings.Index(src, "a.ed.snapEdge(s.S, true)")
	if i < 0 || j < 0 || i > j {
		t.Error("the seams are repaired after the edges are snapped, or not at all")
	}
}

// A frame where the picture changed is a fine place to cut, unless somebody is
// speaking there. Frames sit on the extraction interval -- one second on this
// footage -- so a visual peak at a whole second is exactly where a reply's
// rounded boundary already is, and it wins against the pause beside it.
func TestAPictureDoesNotBeatAPause(t *testing.T) {
	ed := newTestEd(t)
	ed.talk = [][2]float64{{342.2, 350.36}, {351.16, 353.88}}

	if !ed.talking(350.0) {
		t.Error("a second in the middle of a sentence was called silent")
	}
	if !ed.talking(350.5) {
		t.Errorf("a second %g s after the last word was called silent -- the ear does not agree", 350.5-350.36)
	}
	if ed.talking(350.9) {
		t.Error("the pause between two sentences was called speech")
	}
	if ed.talking(400) {
		t.Error("a second with no speech anywhere near it was called speech")
	}
	// the visual candidates are offered only where the talking is not
	body := funcBody(t, "cut.go", `func \(ed \*cutEditor\) snapEdge\(`)
	if !strings.Contains(body, "ed.scores[v.base]; sc != nil && !ed.talking(t)") {
		t.Errorf("a frame can still outscore a pause in the middle of a word:\n%s", body)
	}
	// ...and the speech that answers it is the same reading the silences come
	// from, so the two halves cannot disagree
	if !strings.Contains(readSrc(t, "cut.go"), "ed.talk = speech") {
		t.Error("the speech spans are built from some other list than the gaps")
	}
}

// The marks are not a hint to the model, they are a fact about the material:
// those seconds were said again, and no arrangement of segments may keep them.
//
// Hiding them from the brief was not enough. The model answers in RANGES, and a
// range spanning a folded stretch keeps every second of it -- one run came back
// with 39.28-108.44 and 113.28-164.00 around a mark at 106.16-111.36, having
// aimed at the seam and missed it by two seconds. Two seconds is the whole of a
// doubled sentence.
func TestAMarkedStretchCannotSurviveInTheCut(t *testing.T) {
	segs := []cutSeg{{S: 39.28, E: 108.44}, {S: 113.28, E: 164}}
	marks := []retake{{S: 106.16, E: 111.36, Again: 114.56}}
	if n := dropMarked(&segs, marks); n != 1 {
		t.Fatalf("the mark acted on %d scenes, want 1", n)
	}
	if len(segs) != 2 || segs[0].E != 106.16 || segs[1].S != 113.28 {
		t.Fatalf("the marked seconds are still in the cut: %+v", segs)
	}

	// a scene that spans one comes apart into the two halves either side
	segs = []cutSeg{{S: 90, E: 130}}
	dropMarked(&segs, marks)
	if len(segs) != 2 || segs[0].E != 106.16 || segs[1].S != 111.36 {
		t.Fatalf("a scene across the mark did not come apart: %+v", segs)
	}

	// what is left too short to be a scene goes with it: a sliver either side
	// of a stumble is not a shot
	segs = []cutSeg{{S: 106, E: 112}}
	if dropMarked(&segs, marks); len(segs) != 0 {
		t.Errorf("slivers of a stumble survived as scenes: %+v", segs)
	}

	// an insert is a file, not seconds of the session, and is never cut
	segs = []cutSeg{{S: 107, E: 107, Ins: "card.svg", Dur: 4}}
	if dropMarked(&segs, marks); len(segs) != 1 {
		t.Errorf("a card placed by hand was removed by a mark: %+v", segs)
	}

	// and it runs AFTER the snapping: snapEdge moves an end outward by up to
	// five seconds to find a silence, and outward from a mark's border is into
	// the mark
	src := readSrc(t, "cut_suggest.go")
	snap := strings.Index(src, "a.ed.snapEdge(s.S, true)")
	drop := strings.Index(src, "dropMarked(&a.ed.segs, marks)")
	if snap < 0 || drop < 0 || drop < snap {
		t.Error("the marks are applied before the snapping, which can put them back")
	}
}

// A cut inside a phrase has one place it can go: the gap between two words.
// No silence marks it — there is none between "art" and "and", only a closure —
// so a silence midpoint cannot find it and a frame boundary lands wherever the
// extraction interval happened to fall. The aligner's word edges can.
func TestAnEdgeSnapsToAWordBoundary(t *testing.T) {
	ed := newTestEd(t)
	ed.words = []srcWord{
		{s: 105.52, e: 105.70, w: "art"},
		{s: 105.86, e: 106.04, w: "and"},
		{s: 106.10, e: 106.30, w: "the"},
	}
	got := ed.wordEdges(105.9)
	if len(got) != 6 {
		t.Fatalf("the words near the point came back as %v", got)
	}
	// far away, nothing: a boundary five seconds off is not this boundary
	if far := ed.wordEdges(300); len(far) != 0 {
		t.Errorf("words %v were offered to a point %g s away", far, 300-106.3)
	}
	// and they are offered to the snap above a silence midpoint's score,
	// because a midpoint is a guess at exactly this
	body := funcBody(t, "cut.go", `func \(ed \*cutEditor\) snapEdge\(`)
	iw, ig := strings.Index(body, "ed.wordEdges(t)"), strings.Index(body, "ed.gaps[v.base]")
	if iw < 0 || ig < 0 {
		t.Fatalf("snapEdge no longer offers both candidates:\n%s", body)
	}
	if !strings.Contains(body, "try(w, 0.9)") {
		t.Error("a word boundary does not outrank a silence midpoint (0.8)")
	}
	// the words are the aligner's, read through the one door
	if !strings.Contains(readSrc(t, "cut.go"), "ed.words = a.sessionWords(paths)") {
		t.Error("the page reads word times from somewhere other than sessionWords")
	}
}

// A boundary on a word edge in the middle of a sentence is exact and still
// wrong. The model ended a clip at 740 inside "Performance is still behind the
// old C implementation, but they're catching up" (733.7-744.4) -- it had to
// GUESS where the line ended, because the brief never said, and a picture
// line claiming he "finishes the point and pauses" at 737 won the guess. So a
// line carries its end now, the cut is told what the two numbers mean, and a
// boundary a breath from a line's end snaps to the line's end over the word.
func TestALineCarriesItsEndAndABoundaryPrefersIt(t *testing.T) {
	if got := stampSpan(733.71, 744.43); got != "[733s-745s | 12:13]" {
		t.Errorf("a line's stamp reads %q, want [733s-745s | 12:13] -- start floored, end ceiled", got)
	}
	for _, want := range []string{
		"the seconds it STARTS and the seconds it ENDS",
		"never between the two numbers of one line",
		"the SPEAKER line is right",
	} {
		if !strings.Contains(cutSystem, want) {
			t.Errorf("the cut is not told what a line's two stamps mean: want %q", want)
		}
	}
	// the snap: an end asked for 0.6 s short of a line's end lands on the
	// line's end, not on the word edge nearer to it
	ed := axisEd(t, tlVideo{base: "a", path: "/f/a.mp4", start: 700, dur: 60})
	ed.talk = [][2]float64{{733.71, 744.43}, {746.51, 748.19}}
	ed.words = []srcWord{{s: 743.08, e: 743.48, w: "but"}, {s: 743.56, e: 743.72, w: "they're"},
		{s: 743.88, e: 744.2, w: "catching"}, {s: 744.28, e: 744.43, w: "up"}}
	if got := ed.snapEdge(743.8, false); math.Abs(got-744.43) > 1e-9 {
		t.Errorf("an end 0.6 s inside the sentence snapped to %.2f, want 744.43 -- the end of the line", got)
	}
	// ...but four seconds inside it, the line's end is too far to claim it,
	// and the boundary stays where the words put it (the brief's job now)
	if got := ed.snapEdge(740.0, false); math.Abs(got-744.43) < 1e-9 {
		t.Error("a boundary four seconds inside a line was dragged to its end -- that is not a snap")
	}
}
