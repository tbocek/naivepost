package main

import (
	"math"
	"os"
	"strings"
	"testing"
)

// foldEd is the band's cut with nothing selected: one recording 0-300, kept
// 20-60 and 100-140, at 4 px a second. Its dropped stretches are therefore
// 0-20 (the head), 60-100 (the middle) and 140-300 (the tail).
func foldEd(t *testing.T) *cutEditor {
	t.Helper()
	ed := bandEd(t)
	ed.sel.active = false
	return ed
}

// Every stretch the cut drops is a gap that can be folded -- the head before
// the first clip and the tail after the last one included, because a session
// that starts twenty minutes before the first word is exactly the case this is
// for.
func TestEveryDroppedStretchIsAGap(t *testing.T) {
	ed := foldEd(t)
	want := [][2]float64{{0, 20}, {60, 100}, {140, 300}}
	gaps := ed.foldGaps()
	if len(gaps) != len(want) {
		t.Fatalf("%d gaps, want %d: %+v", len(gaps), len(want), gaps)
	}
	for i, g := range gaps {
		if g.t0 != want[i][0] || g.t1 != want[i][1] || g.on {
			t.Errorf("gap %d is %g-%g folded=%v, want %g-%g open",
				i, g.t0, g.t1, g.on, want[i][0], want[i][1])
		}
	}
}

// A folded gap is drawn as NOTHING -- the clips either side of it meet, border
// against border -- and everything after it moves left by the whole of it,
// which is the point, and is why xOf and tAt walk the cells rather than
// multiplying by pps.
func TestFoldingShrinksTheTimelineAndMovesWhatFollows(t *testing.T) {
	ed := foldEd(t)
	wide := ed.totalW
	x100 := ed.xOf(100)

	ed.folds = [][2]float64{{60, 100}}
	ed.layoutPx()

	if got, want := ed.totalW, wide-40*ed.pps; math.Abs(got-want) > 0.01 {
		t.Errorf("folding 40 s left the page %g px wide, want %g", got, want)
	}
	if got, want := ed.xOf(100), x100-40*ed.pps; math.Abs(got-want) > 0.01 {
		t.Errorf("the clip after the fold is at %g, want %g", got, want)
	}
	// the footage before it has not moved: a fold pushes nothing left of itself
	if got, want := ed.xOf(20), gutterPx+20*ed.pps; math.Abs(got-want) > 0.01 {
		t.Errorf("the clip before the fold moved to %g, want %g", got, want)
	}
	// the two clips MEET: no strip, no gap, nothing standing for the footage
	// that is not on the page
	if got := ed.xOf(100) - ed.xOf(60); got != 0 {
		t.Errorf("the clips either side of the seam are %g px apart, want them touching", got)
	}
	// x and t still agree across it, in both directions -- away from the seam
	// itself, which is one x for forty seconds and cannot answer for all of them
	for _, tt := range []float64{20, 59.9, 100.1, 139.9} {
		if got := ed.tAt(ed.xOf(tt)); math.Abs(got-tt) > 0.01 {
			t.Errorf("x/t round trip at %g s came back %g", tt, got)
		}
	}
	// and on the seam it answers with the second the cut stops keeping, which
	// is the one a press there is aiming at: the end of the clip before it
	if got := ed.tAt(ed.xOf(60)); got != 60 {
		t.Errorf("a press on the seam means %g s, want the 60 s the clip ends at", got)
	}
}

// The − sits in the middle of an open gap, where nothing else on the row is;
// the + sits ON the seam, which is where the two green bars now meet, and is
// the only mark saying the footage between them is still there. A gap with no
// room for the badge itself gets neither.
func TestTheBadgeSitsInTheGapAndThenOnTheSeam(t *testing.T) {
	ed := foldEd(t)
	b := ed.foldBadges()
	if len(b) != 3 {
		t.Fatalf("%d badges for three gaps", len(b))
	}
	if got, want := b[1].cx, (ed.xOf(60)+ed.xOf(100))/2; math.Abs(got-want) > 0.01 {
		t.Errorf("the − is at %g, want the middle of the gap at %g", got, want)
	}
	if got, want := b[1].cy, ed.selBandTop()+selBandH/2; got != want {
		t.Errorf("the badge is at y %g, want the middle of the band at %g", got, want)
	}
	// pressed on: the + is on the seam, and both bars' ends are clear of it
	ed.folds = [][2]float64{{60, 100}}
	ed.layoutPx()
	b = ed.foldBadges()
	if !b[1].gap.on {
		t.Fatal("the folded gap does not say it is folded")
	}
	if got, want := b[1].cx, ed.xOf(60); got != want {
		t.Errorf("the + is at %g, want the seam at %g", got, want)
	}
	// it shares that x with two clip borders, and it is asked FIRST -- both in
	// the press and in the cursor -- or the one control that opens the seam
	// would be unreachable
	src := readSrc(t, "cut.go")
	if !strings.Contains(src, "if i := ed.foldBadgeAt(x+ed.viewX, y); i >= 0 {") {
		t.Error("the press does not ask the fold badge before the bars")
	}
	if !strings.Contains(readSrc(t, "cut_selband.go"), "if ed.foldBadgeAt(x+ed.viewX, y) >= 0 {") {
		t.Error("the cursor does not ask the fold badge before the bars")
	}
	// a gap with no room for the badge is not offered one
	ed.folds = nil
	ed.segs[1].S = 60 + (foldMin-4)/ed.pps // a gap of foldMin-4 px
	ed.layoutPx()
	for _, g := range ed.foldBadges() {
		if g.gap.t0 == 60 {
			t.Error("a gap narrower than the strip that would replace it still offers a −")
		}
	}
}

// The stretch before the first clip and the one after the last are not between
// anything: the middle of one is an arbitrary point in the void -- these are
// the longest gaps on the page -- and its far end is the edge of the page,
// where a badge floats in black with nothing to say which timeline it is on.
// So their badges sit just INSIDE the clip they run up against, on the green.
func TestTheHeadAndTailFoldSitInsideTheClipTheyMeet(t *testing.T) {
	ed := foldEd(t) // clips 20-60 and 100-140; gaps 0-20, 60-100, 140-300
	b := ed.foldBadges()
	if len(b) != 3 {
		t.Fatalf("%d badges for three gaps", len(b))
	}
	if got, want := b[0].cx, ed.xOf(20)+killIn; got != want {
		t.Errorf("the head's − is at %g, want it inside the first clip at %g", got, want)
	}
	if got, want := b[2].cx, ed.xOf(140)-killIn; got != want {
		t.Errorf("the tail's − is at %g, want it inside the last clip at %g", got, want)
	}
	// the gap between two clips keeps its middle: its ends are those clips' grips
	if got, want := b[1].cx, (ed.xOf(60)+ed.xOf(100))/2; got != want {
		t.Errorf("the middle gap's − is at %g, want %g", got, want)
	}
	// never past that clip's middle, which is its ✕: on a clip too short to
	// hold both, the fold's badge gives way
	ed.segs[0].E = 24 // a 4 s clip, narrower than twice killIn
	if got, want := ed.foldBadges()[0].cx, (ed.xOf(20)+ed.xOf(24))/2; got != want {
		t.Errorf("on a short first clip the − is at %g, want its middle at %g", got, want)
	}
	ed.segs[0].E = 60
	// and never half off the page: folded, the head's seam is the head of the tape
	ed.segs[0].S = 20
	ed.folds = [][2]float64{{0, 20}}
	ed.layoutPx()
	if got := ed.foldBadges()[0].cx; got < segKillR+segKillPad {
		t.Errorf("the folded head's + is at %g, with half its plate off the page", got)
	}
}

// A press on the badge folds and unfolds, and the second under the pointer
// stays under the pointer: everything right of a fold moves when it opens, so
// without the anchor the press throws the page sideways by however many
// minutes it hides.
func TestPressingTheBadgeFoldsAndHoldsTheView(t *testing.T) {
	ed := foldEd(t)
	ed.viewX = 200 // scrolled along: the page has room to move under the press
	i := 1
	px := ed.foldBadges()[i].cx
	if got := ed.foldBadgeAt(px, ed.selBandTop()+selBandH/2); got != i {
		t.Fatalf("a press on the − answers gap %d, want %d", got, i)
	}
	sx := px - ed.viewX // where the hand is on screen

	ed.toggleFold(i, px)
	if !ed.foldedGap(60, 100) {
		t.Fatal("the press did not fold the gap")
	}
	// the gap the − was in is gone, so what the anchor can hold under the
	// pointer is the seam that replaced it -- and it does
	if got := ed.xOf(60) - ed.viewX; math.Abs(got-sx) > 0.5 {
		t.Errorf("the seam landed %g px from the − that made it, at %g", got-sx, got)
	}
	// no undo step: the cut is the same cut, and what was looked at is not
	// something to take back
	if len(ed.undo) != 0 {
		t.Errorf("folding pushed %d undo steps", len(ed.undo))
	}
	// ...and it goes on disk, so it survives the project being closed
	if !strings.Contains(readSrc(t, "cut.go"), "Folds: ed.folds") {
		t.Error("the folds are not written with the cut")
	}
	// pressing the + puts it back, and holds the view the same way: the seam
	// is the second the clip before it ends at, and that second stays put
	// while the gap opens to the right of it
	px = ed.foldBadges()[1].cx
	sx = px - ed.viewX
	ed.toggleFold(1, px)
	if ed.foldedGap(60, 100) {
		t.Fatal("the + left the gap folded")
	}
	if got := ed.xOf(60) - ed.viewX; math.Abs(got-sx) > 0.5 {
		t.Errorf("unfolding moved the clip's end %g px, to %g", got-sx, got)
	}
}

// A fold is remembered by OVERLAP, not by its ends: trimming the clip beside a
// gap moves that gap's edge, and a fold matched by its ends would be lost to
// every trim.
func TestAFoldSurvivesTheGapChangingShape(t *testing.T) {
	ed := foldEd(t)
	ed.folds = [][2]float64{{60, 100}}
	ed.segs[0].E = 50 // the gap is 50-100 now
	if !ed.foldedGap(50, 100) {
		t.Error("trimming the clip before the gap lost its fold")
	}
	// and syncFolds writes the gap's new bounds back
	ed.syncFolds()
	if len(ed.folds) != 1 || ed.folds[0] != [2]float64{50, 100} {
		t.Errorf("after the trim the stored folds are %v, want [[50 100]]", ed.folds)
	}
	// a gap that stopped existing takes its fold with it
	ed.segs = []cutSeg{{S: 20, E: 300}}
	ed.syncFolds()
	if len(ed.folds) != 0 {
		t.Errorf("the gap is gone and its fold stayed: %v", ed.folds)
	}
	// ...and so does the cut itself. Emptied -- cleared, or undone back to
	// before there was one -- the gap beside a fold grows to the whole
	// session, and folded that is the page collapsed to one strip by a press
	// nobody made. A fold is a gap BETWEEN scenes.
	ed.folds = [][2]float64{{60, 100}}
	ed.segs = nil
	ed.syncFolds()
	if len(ed.folds) != 0 {
		t.Errorf("clearing the cut folded the whole recording away: %v", ed.folds)
	}
}

// ▶ plays the dropped stretches too, so a seam the line walks into comes open
// -- a page showing a seam while the line is somewhere inside it is lying about
// where the line is. ▶✂ plays the cut, which never enters one.
func TestPlayingIntoASeamOpensIt(t *testing.T) {
	ed := foldEd(t)
	ed.folds = [][2]float64{{60, 100}}
	ed.layoutPx()

	ed.playhead = 80
	ed.walkFold()
	if ed.foldedGap(60, 100) {
		t.Error("the line walked into the seam and it stayed shut")
	}
	// it stays open afterwards: refolding under a running line would pull the
	// page out from under it
	ed.playhead = 120
	ed.walkFold()
	if ed.foldedGap(60, 100) {
		t.Error("the seam folded itself back while the line was still running")
	}
	// and the cut's own play never asks: it is called under !ed.cutOnly
	if !strings.Contains(readSrc(t, "cut.go"), "if !ed.cutOnly {\n\t\t\ted.walkFold()\n\t\t}") {
		t.Error("▶✂ is not held clear of the unfolding")
	}
}

// A press that can MOVE something opens the seams around it for as long as the
// button is down: folded, two clips are drawn foldPx apart with minutes
// between them, and a drag across that would have to invent what a pixel is
// worth inside a fold. Only the seams the press can reach -- opening all of
// them would throw the whole page about for a drag that cannot touch them.
func TestAHoldOpensTheSeamsAroundItAndTheReleaseShutsThem(t *testing.T) {
	ed := foldEd(t)
	ed.folds = [][2]float64{{0, 20}, {60, 100}, {140, 300}}
	ed.layoutPx()

	s := ed.segs[0] // the clip 20-60: the head seam and the middle one touch it
	px := ed.xOf(30)
	shut := ed.foldOpen(s.S, s.E, px)
	if len(shut) != 2 {
		t.Fatalf("the hold opened %d seams, want the two beside the clip", len(shut))
	}
	if ed.foldedGap(0, 20) || ed.foldedGap(60, 100) {
		t.Error("a seam beside the held clip is still folded")
	}
	if !ed.foldedGap(140, 300) {
		t.Error("the tail seam, which this drag cannot reach, was opened too")
	}
	ed.foldShut(shut, px)
	if !ed.foldedGap(0, 20) || !ed.foldedGap(60, 100) {
		t.Error("the release did not put the opened seams back")
	}

	// a gap the drag CLOSED is gone, and folding it back would fold whatever
	// gap now overlaps where it was
	shut = ed.foldOpen(ed.segs[0].S, ed.segs[0].E, px)
	ed.segs[0].E = 100 // the middle gap is closed: the clips now touch
	ed.foldShut(shut, px)
	if ed.foldedGap(60, 100) {
		t.Errorf("a gap that no longer exists was folded back: %v", ed.folds)
	}
	if !ed.foldedGap(0, 20) {
		t.Error("the head seam, untouched by the drag, was not put back")
	}
}

// The drawing: a seam is not a strip of anything. A fold is footage that exists
// and that the cut drops, and the page has nothing to say about it beyond the +
// -- so the clips meet, and there is no band between them. Nor is there one
// where nobody filmed at all: that is drawn as the two takes touching, each
// with its own border.
func TestASeamLeavesNoGap(t *testing.T) {
	src := readSrc(t, "cut_fold.go")
	if !strings.Contains(src, "func (ed *cutEditor) drawFoldBadges(") {
		t.Error("cut_fold.go does not draw the badges")
	}
	for _, no := range []string{"hatchStrokes", "drawFolds"} {
		if strings.Contains(src, no) {
			t.Errorf("the seam is drawn as a strip of its own (%s)", no)
		}
	}
	// on screen: the strip is drawn, and the badge on it is not red -- folding
	// takes nothing away
	ed := foldEd(t)
	ed.folds = [][2]float64{{60, 100}}
	ed.layoutPx()
	const w, h = 1300, 200
	at := renderTrack(t, ed, w, h)
	x, y := int(ed.xOf(60)), int(ed.selBandTop())+selBandH/2
	white := false
	for dx := -3; dx <= 3 && !white; dx++ {
		if r, g, b := at(x+dx, y); r > 200 && g > 200 && b > 200 {
			white = true
		}
	}
	if !white {
		t.Error("no + is drawn on the seam")
	}
	r, g, b := at(x, y-5)
	if r > 150 && int(r) > int(g)+60 && int(r) > int(b)+60 {
		t.Error("the fold's badge is red — folding is not a remove")
	}
}

// A seam is no px wide, and the two borders it puts on the same x are told
// apart by the side the press lands on -- there is no room left to tell them
// apart by, which is the price of the clips meeting, and it is paid once at
// the press instead of by a strip standing on the page for good.
func TestASeamIsNoWideAtAllAndTheSideDecides(t *testing.T) {
	if foldPx != 0 {
		t.Errorf("a folded stretch is drawn %g px wide, want none at all", foldPx)
	}
	if foldMin < 2*segKillHit {
		t.Errorf("a gap of %g px is offered a − wider than the gap itself", foldMin)
	}
	ed := foldEd(t)
	ed.folds = [][2]float64{{60, 100}}
	ed.layoutPx()
	seam := ed.xOf(60)
	// left of it: the first clip's end. Right of it: the second clip's start.
	// Nearest-wins answered with the first one in the list from both sides,
	// which left the right-hand border unreachable while the seam was shut.
	for _, c := range []struct {
		px        float64
		seg, part int
		what      string
	}{
		{seam - 3, 0, selEnd, "just left of the seam"},
		{seam + 3, 1, selStart, "just right of the seam"},
	} {
		if seg, part := ed.bandClipPartAt(c.px); seg != c.seg || part != c.part {
			t.Errorf("a press %s takes clip %d part %d, want %d/%d",
				c.what, seg, part, c.seg, c.part)
		}
	}
	// and the picture band answers the same way, for the same reason
	if seg, end, ok := ed.edgeAt(seam - 3); !ok || seg != 0 || !end {
		t.Errorf("left of the seam the pictures take clip %d end=%v ok=%v, want clip 0's end",
			seg, end, ok)
	}
	if seg, end, ok := ed.edgeAt(seam + 3); !ok || seg != 1 || end {
		t.Errorf("right of the seam the pictures take clip %d end=%v ok=%v, want clip 1's start",
			seg, end, ok)
	}
	// the folds go on disk with the cut, and are cleared with it
	src := readSrc(t, "cut.go")
	for _, want := range []string{"Folds [][2]float64", "ed.folds = "} {
		if !strings.Contains(src, want) {
			t.Errorf("cut.go does not contain %q", want)
		}
	}
	if _, err := os.Stat("cut_fold.go"); err != nil {
		t.Fatal(err)
	}
}
