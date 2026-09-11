package main

// Moving a whole clip, and the two ways a card can sit in the cut.
//
// These are one feature seen twice. The timeline learned to hold a CLIP rather
// than a border -- a second click to take hold, a drag to slide it, snapping to
// whatever is beside it -- and the thing most worth holding turned out to be an
// insert, because a card is the one item on the page you place by hand and get
// wrong by a second. Holding one is also how you get at it: the Insert button
// becomes Edit, and the dialog it opens is where a card says what it says, how
// long it runs, and which of the two modes it is in.
//
// The modes are the part with teeth. A card OVER the footage costs the seconds
// it runs, exactly as Remove does; a card BETWEEN it costs nothing and makes the
// video longer. Switching between them has to move the footage to match, or the
// cut quietly loses the seconds the other mode gave back.

import (
	"math"
	"os"
	"runtime"
	"strings"
	"testing"

	"github.com/diamondburned/gotk4/pkg/cairo"
)

// moveEd is a page with one recording long enough to slide a clip about in.
func moveEd(t *testing.T) *cutEditor {
	t.Helper()
	ed := newTestEd(t) // pps 4: a session second is four px
	ed.vids = []tlVideo{{base: "v", path: "v.mkv", start: 0, dur: 300, interval: 5, fps: 30}}
	ed.relayout()
	return ed
}

// A press already meant "pick up the border under the cursor". Away from any
// border, on the second click of a double click, it means "pick up the whole
// clip", and the two readings must not overlap: the border is the smaller
// target and it lives inside the larger one.
func TestTheSecondClickPicksUpAWholeClip(t *testing.T) {
	ed := moveEd(t)
	ed.segs = []cutSeg{{S: 10, E: 20}, {S: 30, E: 40}}

	if i := ed.segAtPx(ed.xOf(15)); i != 0 {
		t.Errorf("the middle of the first clip found clip %d, want 0", i)
	}
	if i := ed.segAtPx(ed.xOf(25)); i != -1 {
		t.Errorf("footage the cut drops found clip %d, want nothing", i)
	}
	if !ed.grabSeg(ed.xOf(35)) {
		t.Fatal("the second clip cannot be picked up")
	}
	if s := ed.heldSeg(); s == nil || s.S != 30 {
		t.Fatalf("the held clip is %v, want the one starting at 30", s)
	}
	// a press on the held clip is the drag that moves it; anywhere else it is
	// the selection it has always been
	if !ed.onHeldSeg(ed.xOf(38)) {
		t.Error("a press on the held clip does not take hold of it")
	}
	if ed.onHeldSeg(ed.xOf(15)) {
		t.Error("a press on another clip moves the held one")
	}
	ed.dropSeg()
	if ed.heldSeg() != nil {
		t.Error("the clip is still held after being put down")
	}
}

// Del takes the held clip out, which is the only way to get rid of a spliced
// card: it has no span to select, and it is under the playhead for one instant.
func TestDeleteRemovesTheHeldClip(t *testing.T) {
	a := &App{outDir: t.TempDir()}
	ed := moveEd(t)
	ed.a, a.ed = a, ed
	ed.segs = []cutSeg{{S: 0, E: 60}, {S: 20, E: 20, Ins: "card.svg", Dur: 4}}
	if !ed.grabSeg(ed.xOf(20)) {
		t.Fatal("the card cannot be picked up")
	}
	a.removeSelClicked()
	if len(ed.segs) != 1 || ed.segs[0].isInsert() {
		t.Errorf("the cut is %v, want the footage on its own", ed.segs)
	}
	if ed.heldSeg() != nil {
		t.Error("something is still held after it was removed")
	}
	ed.undoLast()
	if len(ed.segs) != 2 {
		t.Errorf("undo left %d clips, want the card back", len(ed.segs))
	}
}

// An insert is painted over the footage, so it is what you are pointing at.
// Picking up the clip behind it would mean a card could never be got hold of at
// all -- every card is inside or beside footage.
func TestACardIsHeldBeforeTheClipUnderIt(t *testing.T) {
	ed := moveEd(t)
	ed.segs = []cutSeg{{S: 10, E: 40}, {S: 15, E: 18, Ins: "card.svg"}}
	if i := ed.segAtPx(ed.xOf(16)); i != 1 {
		t.Errorf("the card was pointed at and clip %d came back", i)
	}
	// ...and a spliced card, which has no width at all, is found by the marker
	// drawn for it rather than by its span
	ed.segs = []cutSeg{{S: 10, E: 40}, {S: 20, E: 20, Ins: "card.svg", Dur: 4}}
	if i := ed.segAtPx(ed.xOf(20)); i != 1 {
		t.Errorf("the spliced card's marker found clip %d, want the card", i)
	}
	_, mx1 := ed.spliceSpan(ed.segs[1])
	if i := ed.segAtPx(mx1 + 2); i != 0 {
		t.Errorf("clear of the marker found clip %d, want the footage behind it", i)
	}

	// and the whole double click agrees: a border is asked about before a clip,
	// and a spliced card's two borders are both at the centre of the marker you
	// press to hold it. Answering with one would mean a card could be picked up
	// only by aiming at the edge of its own marker, and the thing in hand after
	// a press on the middle of it would be a border of a clip with no length.
	ed.pickAt(ed.xOf(20), true)
	if ed.edgeOn {
		t.Errorf("pressing the card gave a border (clip %d) instead of the card", ed.edgeSeg)
	}
	if s := ed.heldSeg(); s == nil || !s.isInsert() {
		t.Errorf("pressing the card held %v, want the card", s)
	}
	// the footage it is spliced into still has borders, and they are still what
	// a press near one means
	ed.pickAt(ed.xOf(40)-edgeGrab/2, true)
	if !ed.edgeOn || ed.edgeSeg != 0 || !ed.edgeEnd {
		t.Errorf("the footage's end border did not come up: on=%v seg=%d end=%v",
			ed.edgeOn, ed.edgeSeg, ed.edgeEnd)
	}
}

// What the drag is FOR: the clip keeps its length. Two edge drags that have to
// agree to the frame was the old way to say this, and when they disagreed the
// clip changed length instead of moving.
func TestMovingAClipKeepsItsLength(t *testing.T) {
	ed := moveEd(t)
	ed.segs = []cutSeg{{S: 100, E: 130}}
	ed.grabSeg(ed.xOf(115))

	// a drag is many motion events and ONE undo step: fifty of them would be
	// the entire history
	for _, to := range []float64{101, 103, 107, 110} {
		ed.moveSegTo(to, true)
	}
	if got := len(ed.undo); got != 1 {
		t.Errorf("a drag left %d undo entries, want 1", got)
	}
	if s := ed.segs[0]; s.S != 110 || s.E-s.S != 30 {
		t.Errorf("the clip ended at %v, want 30 s starting at 110", s)
	}

	// and it cannot leave the recording it was cut from: those are that file's
	// frames, and sliding it into the next recording would show footage nobody
	// selected
	ed.moveSegTo(-20, false)
	if ed.segs[0].S != 0 {
		t.Errorf("dragged off the front, the clip starts at %g, want 0", ed.segs[0].S)
	}
	ed.moveSegTo(1000, false)
	if s := ed.segs[0]; s.E != 300 || s.S != 270 {
		t.Errorf("dragged off the end, the clip is %v, want 270–300", s)
	}
}

// Snapping, which is the difference between "put these two together" as a
// gesture and as an arithmetic exercise. It is a distance in PIXELS -- a snap of
// so many seconds would be unreachable zoomed in and unavoidable zoomed out.
func TestAClipSnapsToWhatIsBesideIt(t *testing.T) {
	segs := []cutSeg{{S: 0, E: 10}, {S: 20, E: 30}, {S: 50, E: 60}}
	const snap = 1 // one second, whatever the zoom makes that

	for _, c := range []struct {
		to   float64
		want float64
		why  string
	}{
		{to: 10.6, want: 10, why: "close behind: it goes flush against the clip in front of it"},
		{to: 39.5, want: 40, why: "close ahead: its END goes flush against the next clip's start"},
		{to: 15, want: 15, why: "clear of both: it goes where it was put"},
		{to: 45, want: 40, why: "past the next clip: it stops against it"},
		{to: 5, want: 10, why: "past the clip behind: it stops against that"},
	} {
		if got := clampSeg(segs, 1, c.to, math.Inf(-1), math.Inf(1), snap); got != c.want {
			t.Errorf("dragged to %g the clip starts at %g, want %g — %s", c.to, got, c.want, c.why)
		}
	}
	// the recording's own ends bound it as well, and they win over an empty
	// stretch of session that has no clip in it to snap to
	if got := clampSeg(segs, 0, -5, 2, 300, snap); got != 2 {
		t.Errorf("clamped to the recording the clip starts at %g, want 2", got)
	}
}

// ---- the two modes -----------------------------------------------------------

// A spliced card is stored as a POINT with its length beside it, and the render
// list is where that becomes a sequence: the clip it sits in is cut in two and
// the card comes out between the halves.
func TestASplicedCardCutsTheClipItSitsIn(t *testing.T) {
	card := cutSeg{S: 20, E: 20, Ins: "card.svg", Dur: 4}
	got := splitSpliced([]cutSeg{{S: 10, E: 40}, card})
	want := []cutSeg{{S: 10, E: 20}, card, {S: 20, E: 40}}
	if len(got) != len(want) {
		t.Fatalf("the cut came out as %v, want three clips", got)
	}
	for i := range want {
		if !sameSeg(got[i], want[i]) {
			t.Errorf("clip %d is %v, want %v", i, got[i], want[i])
		}
	}
	// the footage is untouched by it -- that is the whole point of this mode --
	// and the video is longer by exactly the card
	if f := cutLen(filmedOf(got)); f != 30 {
		t.Errorf("%g s of footage survived the splice, want 30", f)
	}
	if tot := cutLen(got); tot != 34 {
		t.Errorf("the cut runs %g s, want 34: the footage plus the card", tot)
	}

	// a card that lands ON a border cuts nothing: there is nothing to split, and
	// splitting there would leave a clip of no length
	head := cutSeg{S: 10, E: 10, Ins: "card.svg", Dur: 4}
	if got := splitSpliced([]cutSeg{{S: 10, E: 40}, head}); len(got) != 2 || !sameSeg(got[0], head) {
		t.Errorf("a card at the head of a clip came out as %v, want the card then the clip", got)
	}
	// and a cut with nothing spliced into it comes back exactly as it went in
	plain := []cutSeg{{S: 0, E: 5}, {S: 10, E: 20, Ins: "over.svg"}}
	if got := splitSpliced(plain); len(got) != 2 || !sameSeg(got[0], plain[0]) || !sameSeg(got[1], plain[1]) {
		t.Errorf("an ordinary cut was rearranged into %v", got)
	}
}

// Switching modes moves the FOOTAGE, which is the part that is not a field
// assignment. Overwrite took seconds out of the recording; spliced has to give
// them back, or the card costs its length twice -- once in footage and once in
// running time.
func TestSwitchingAcardBetweenTheModesMovesTheFootage(t *testing.T) {
	ed := moveEd(t)
	ed.segs = []cutSeg{{S: 0, E: 60}}
	ed.addInsert("card.svg", 20, 4, false)
	if got := cutLen(filmedOf(ed.segs)); got != 56 {
		t.Fatalf("placing a card over the footage left %g s of it, want 56", got)
	}

	i := ed.indexOfSeg(cutSeg{S: 20, Ins: "card.svg"})
	if i < 0 {
		t.Fatal("the card is not in the cut")
	}
	ed.setSpliced(i, true)

	if got := cutLen(filmedOf(ed.segs)); got != 60 {
		t.Errorf("%g s of footage came back, want all 60: a spliced card costs none", got)
	}
	if n := len(filmedOf(ed.segs)); n != 1 {
		t.Errorf("the footage is in %d clips, want 1 — the halves did not meet again", n)
	}
	i = ed.indexOfSeg(cutSeg{S: 20, Ins: "card.svg"})
	if i < 0 {
		t.Fatal("the card was lost switching modes")
	}
	if s := ed.segs[i]; !s.spliced() || s.Dur != 4 || s.S != s.E {
		t.Errorf("the spliced card is %v, want a point at 20 running 4 s", s)
	}
	// the whole edit is one undo step, and it goes back to what was there
	ed.undoLast()
	if got := cutLen(filmedOf(ed.segs)); got != 56 {
		t.Errorf("after undo the footage runs %g s, want the 56 it did before", got)
	}

	// ...and back the other way the footage gives way again, exactly as it does
	// when a card is placed there in the first place
	ed.setSpliced(ed.indexOfSeg(cutSeg{S: 20, Ins: "card.svg"}), true)
	ed.setSpliced(ed.indexOfSeg(cutSeg{S: 20, Ins: "card.svg"}), false)
	if got := cutLen(filmedOf(ed.segs)); got != 56 {
		t.Errorf("back over the footage it runs %g s, want 56 again", got)
	}
	if s := ed.segs[ed.indexOfSeg(cutSeg{S: 20, Ins: "card.svg"})]; s.spliced() || s.E != 24 {
		t.Errorf("the card is %v, want 20–24 over the footage", s)
	}
}

// The Edit dialog's answer, applied: the wording, the mode and the length arrive
// together and are one edit. A length typed for an overwrite card takes the
// footage it now covers, the same surgery placing one does.
func TestEditingACardAppliesTheWholeAnswerAtOnce(t *testing.T) {
	ed := moveEd(t)
	ed.segs = []cutSeg{{S: 0, E: 60}}
	ed.addInsert("card.svg", 20, 4, false)
	i := ed.indexOfSeg(cutSeg{S: 20, Ins: "card.svg"})

	ed.applyInsert(i, "card.svg?Title=Hi", insMode{dur: 10})
	i = ed.indexOfSeg(cutSeg{S: 20, Ins: "card.svg?Title=Hi"})
	if i < 0 {
		t.Fatal("the re-worded card is not in the cut")
	}
	if s := ed.segs[i]; s.E != 30 {
		t.Errorf("the card runs to %g, want 30", s.E)
	}
	if got := cutLen(filmedOf(ed.segs)); got != 50 {
		t.Errorf("a card grown to 10 s left %g s of footage, want 50", got)
	}
	// the card stays held, so the next edit does not need it picked up again
	if s := ed.heldSeg(); s == nil || s.Ins != "card.svg?Title=Hi" {
		t.Error("the card being edited was let go of")
	}

	// a spliced card's length is the only thing that can be said about it -- it
	// has no edges on the timeline to drag
	ed.applyInsert(i, "card.svg?Title=Hi", insMode{splice: true, dur: 6})
	i = ed.indexOfSeg(cutSeg{S: 20, Ins: "card.svg?Title=Hi"})
	if s := ed.segs[i]; !s.spliced() || s.length() != 6 {
		t.Errorf("the spliced card is %v, want a 6 s point", s)
	}
	if got := cutLen(filmedOf(ed.segs)); got != 60 {
		t.Errorf("%g s of footage, want all 60 back", got)
	}
}

// Zooming in widens every second of the timeline, and the marker for a spliced
// card was the one thing on it that did not move: 22 px at every zoom, so a card
// beside a clip of its own length looked shorter the further in you went, and a
// card is not getting shorter because you leaned closer. It is drawn at its own
// length now -- the seconds it plays for, taken at the zoom you are looking at
// -- with the old fixed width as a floor, so it is still findable at the zoom
// where a whole session fits on the screen and four seconds is 16 px.
func TestTheSplicedMarkerIsTheCardsLengthAtThisZoom(t *testing.T) {
	ed := moveEd(t)
	ed.segs = []cutSeg{{S: 0, E: 60}, {S: 20, E: 20, Ins: "card.svg", Dur: 4}}
	card := ed.segs[1]

	// zoomed out: the floor, or the card is a sliver of violet nobody can hit
	ed.pps = 4
	ed.relayout()
	if x0, x1 := ed.spliceSpan(card); math.Abs((x1-x0)-splicePx) > 0.01 {
		t.Errorf("at 4 px/s the marker is %g px wide, want the %g px floor", x1-x0, splicePx)
	}

	// zoomed in: the card's own length, which is exactly as wide as the same
	// number of seconds of footage beside it
	ed.pps = 40
	ed.relayout()
	x0, x1 := ed.spliceSpan(card)
	if got, want := x1-x0, ed.xOf(24)-ed.xOf(20); math.Abs(got-want) > 0.01 {
		t.Errorf("the 4 s card is %g px wide and 4 s of footage is %g px", got, want)
	}
	if math.Abs(x0-ed.xOf(20)) > 0.01 {
		t.Errorf("the marker starts at %g and the footage is cut open at %g -- the card "+
			"begins where the red line stood when it was placed, and the footage that "+
			"follows resumes after it", x0, ed.xOf(20))
	}

	// and what can be seen can be pressed: the marker IS the card's target
	if i := ed.segAtPx(ed.xOf(20) + 70); i != 1 {
		t.Errorf("a press well inside the marker found clip %d, want the card", i)
	}
	if i := ed.segAtPx(x1 + 2); i != 0 {
		t.Errorf("a press clear of it found clip %d, want the footage", i)
	}
	// including the sweep that previews it: the wider marker is the same card,
	// so its far edge is still its last frame
	if s, into := ed.insertAt(ed.tAt(x1) - 0.01); s == nil || math.Abs(into-card.Dur) > 0.05 {
		t.Errorf("the end of the marker reads as %.2f s into the card, want %.1f", into, card.Dur)
	}
}

// A spliced card is previewed by sweeping the playhead across its marker: there
// is no session time inside it, so the marker's width stands in for the card's
// whole length.
func TestASplicedCardIsScrubbedAcrossItsMarker(t *testing.T) {
	ed := moveEd(t)
	ed.segs = []cutSeg{{S: 0, E: 60}, {S: 20, E: 20, Ins: "card.svg", Dur: 4}}
	mx0, mx1 := ed.spliceSpan(ed.segs[1])
	w := (mx1 - mx0) / ed.pps // the whole marker, in seconds of session time

	for _, c := range []struct {
		at   float64
		want float64 // seconds into the card, -1 for "not on the card at all"
	}{
		{at: 20 - 0.1, want: -1}, // before the cut is the footage ahead of it
		{at: 20 + 0.001, want: 0},
		{at: 20 + w/2, want: 2}, // the middle of the marker is the middle of the card
		{at: 20 + w - 0.001, want: 4},
		{at: 20 + w + 0.1, want: -1},
	} {
		s, into := ed.insertAt(c.at)
		if c.want < 0 {
			if s != nil {
				t.Errorf("%.2f s is %.2f s into the card, and should be on the footage", c.at, into)
			}
			continue
		}
		if s == nil {
			t.Errorf("%.2f s is not on the card", c.at)
			continue
		}
		if math.Abs(into-c.want) > 0.02 {
			t.Errorf("%.2f s reads as %.2f s into the card, want %.2f", c.at, into, c.want)
		}
	}
}

// ---- the picture --------------------------------------------------------------

func renderTrack(t *testing.T, ed *cutEditor, w, h int) func(x, y int) (r, g, b uint8) {
	t.Helper()
	surf := cairo.CreateImageSurface(cairo.FormatARGB32, w, h)
	cr := cairo.Create(surf)
	ed.drawTrack(cr, w, h)
	surf.Flush()
	data, stride := surf.Data(), surf.Stride()
	pix := make([]byte, len(data))
	copy(pix, data) // off the C heap before the surface is collected
	runtime.KeepAlive(surf)
	return func(x, y int) (uint8, uint8, uint8) {
		i := y*stride + x*4
		return pix[i+2], pix[i+1], pix[i]
	}
}

// The hatching is the page's word for "the footage stops here", and a spliced
// card is exactly that, so it wears the same marks -- broken now, at a fifth of
// their old length, because full-height strokes every six px read as a texture
// laid over the track rather than as marks on it.
func TestASplicedCardIsMarkedLikeAHoleInTheFootage(t *testing.T) {
	ed := moveEd(t)
	ed.segs = []cutSeg{{S: 0, E: 60}, {S: 20, E: 20, Ins: "card.svg", Dur: 4}}
	const w, h = 400, 200
	at := renderTrack(t, ed, w, h)

	// the marker is centred on the card's point, and at this zoom -- four px a
	// second, where four seconds of card is under the floor -- it is splicePx wide
	x := int(ed.xOf(20))
	band, ground := 0, 0
	for y := int(ed.picTop()) + 2; y < int(ed.picTop())+ed.thumbHt; y++ {
		for dx := -int(splicePx/2) + 2; dx <= int(splicePx/2)-2; dx++ {
			r, g, b := at(x+dx, y)
			if r > 90 && g > 70 && b < 90 {
				band++ // a yellow hatch stroke
			}
			if int(b) > int(r)+20 && b > 60 {
				ground++ // the violet the card is drawn in
			}
		}
	}
	if band == 0 {
		t.Error("the spliced card has no hatching on it — nothing says the footage is cut here")
	}
	if ground == 0 {
		t.Error("the spliced card is not violet — nothing says a card goes here")
	}

	// dashed, not solid: a column through the middle of a hatch stroke has gaps
	// in it. Counted over the hole between two recordings, which is the same
	// picture drawn by the same helper.
	solid := 0
	for y := int(ed.picTop()) + 1; y < int(ed.picTop())+ed.thumbHt+4; y++ {
		r, g, b := at(x, y)
		if r > 90 && g > 70 && b < 90 {
			solid++
		}
	}
	if tall := ed.thumbHt + 3; solid >= tall {
		t.Errorf("a column of the marker is hatched %d px of %d — the dashes are still whole strokes",
			solid, tall)
	}
}

// The clip being held is outlined, because a gesture that moves something has to
// say what it is about to move.
func TestTheHeldClipIsDrawn(t *testing.T) {
	ed := moveEd(t)
	ed.segs = []cutSeg{{S: 10, E: 30}}
	const w, h = 400, 200
	white := func() int {
		at := renderTrack(t, ed, w, h)
		n := 0
		for y := int(ed.picTop()); y < int(ed.picTop())+ed.thumbHt+6; y++ {
			for x := int(ed.xOf(10)) - 2; x <= int(ed.xOf(30))+2; x++ {
				if r, g, b := at(x, y); r > 200 && g > 200 && b > 200 {
					n++
				}
			}
		}
		return n
	}
	before := white()
	ed.grabSeg(ed.xOf(20))
	if after := white(); after <= before {
		t.Errorf("holding the clip drew %d white px, and not holding it drew %d — "+
			"nothing on the track says which clip a drag would move", after, before)
	}
}

// A press picks something up, and picking something up does not move
// the picture. That is the whole rule, and it is here because both halves of it
// have been got wrong.
//
// The border is asked about before the clip, and has to be: a clip is the whole
// green area, its borders are a few px inside the ends of that area, and the
// clips of a cut sit edge to edge -- so a border asked about second is a border
// with no press anywhere on the timeline that can reach it. Trimming would be
// gone. What the border winning must NOT do is move the red line: two presses on
// the same area a few px apart mean different things (the clip, then its end),
// which is fine and visible on the track, but if the second one also cues the
// preview then the picture jumps to the end of the area for no reason the hand
// can see. The line follows what MOVES -- drag or ‹f/f›, on an edge or a clip --
// and choosing a thing is not moving it.
func TestPickingSomethingUpNeverMovesTheRedLine(t *testing.T) {
	ed := moveEd(t)
	ed.segs = []cutSeg{{S: 10, E: 20}, {S: 30, E: 40}}
	ed.playhead, ed.hasPlay = 5, true

	ed.pickAt(ed.xOf(35), true) // the middle of the second clip
	if s := ed.heldSeg(); s == nil || s.S != 30 {
		t.Fatalf("the held clip is %v, want the one starting at 30", s)
	}
	if ed.playhead != 5 {
		t.Errorf("taking hold of a clip moved the red line to %.2f", ed.playhead)
	}

	// and its border is still reachable with the clip in hand -- this is the
	// press that trims, and there is nowhere else to make it from
	ed.pickAt(ed.xOf(40)-edgeGrab/2, true)
	if !ed.edgeOn || !ed.edgeEnd || ed.edgeSeg != 1 {
		t.Fatalf("the border did not come up: on=%v end=%v seg=%d", ed.edgeOn, ed.edgeEnd, ed.edgeSeg)
	}
	if ed.heldSeg() != nil {
		t.Error("the clip is still held as well; one thing is held at a time, or ‹f/f› has two meanings")
	}
	if ed.playhead != 5 {
		t.Errorf("picking the border up moved the red line to %.2f, want it left at 5", ed.playhead)
	}

	// the same holds with nothing in hand: an edge picked up is an edge picked
	// up, not a request to look at it
	ed.dropEdge()
	ed.pickAt(ed.xOf(30)+edgeGrab/2, true)
	if !ed.edgeOn || ed.edgeEnd || ed.edgeSeg != 1 {
		t.Errorf("the start border did not come up: on=%v end=%v seg=%d", ed.edgeOn, ed.edgeEnd, ed.edgeSeg)
	}
	if ed.playhead != 5 {
		t.Errorf("the red line moved to %.2f on a bare pick-up", ed.playhead)
	}

	// but a border that MOVES takes the picture with it, which is the whole
	// point of trimming against the preview
	ed.nudgeEdge(1)
	if ed.playhead == 5 {
		t.Error("the border was nudged and the preview stayed put -- an edge is judged by the frame it cuts on")
	}

	// and a press clear of everything puts down whatever is held
	ed.pickAt(ed.xOf(25), true)
	if ed.edgeOn || ed.heldSeg() != nil {
		t.Error("a press on footage the cut drops kept hold of something")
	}
}

// ---- the wiring ---------------------------------------------------------------

// The gestures and the button, pinned in the source: none of this can be pressed
// from a test, and all of it is the difference between a feature and a set of
// methods nobody calls.
func TestTheClipToolAndTheEditButtonAreWired(t *testing.T) {
	b, err := os.ReadFile("cut.go")
	if err != nil {
		t.Fatal(err)
	}
	src := string(b)
	for _, want := range []string{
		"ed.pickAt(x+ed.viewX, true)",                          // the second click, and all of what it means
		"case ed.grabSeg(px):",                                 // ...away from a border, which is a whole clip
		"if !ed.onHeldSeg(px) {",                               // right press on the scene under the pointer
		"ed.moveSegTo(ed.tAtView(slideX0+ox)-slideGrab, true)", // ...drags it, without writing the file per motion
		"ed.showSeg(true)",
		"if ed.segDirty {\n\t\t\t\t\ted.persist()", // release: this is the cut that goes on disk
		"if ed.segOn && ed.nudgeSeg(n) {",          // ‹f and f› are the clip's while one is held
		"ed.dropSeg()",
		// the button is one button with two jobs, and which one it is doing
		// follows what is held
		"ed.insBtn.SetIconName(\"document-edit-symbolic\")",
		"a.editInsert()",
		"em := insMode{splice: was.spliced(), dur: was.length(), mute: was.Mute, lane: was.Lane}",
		"between := gtk.NewCheckButtonWithLabel(",
		"over.SetGroup(between)", // the two modes are a choice, not a tick nobody reads
		"ed.applyInsert(i, file+q.suffix(), m)",
		// ...and placing one asks the same two questions of every file. A video
		// sting used to be dropped straight onto the timeline without them,
		// which left "between the footage" reachable only by placing it as an
		// overwrite first and editing it afterwards
		"fields, _ := insFields(path)\n\t\ta.askInsertParams(\"Insert\", path, fields, m, func(q svgQuery, m insMode) {",
	} {
		if !strings.Contains(src, want) {
			t.Errorf("the cut page no longer contains %q", want)
		}
	}
}

// A click is not a drag, and a held clip is not a hole in the page.
//
// Nobody double-clicks without moving the mouse a pixel or two. The second
// press landed on a clip the first had taken in hand, the wiggle was read as a
// drag, the clip slid, and the picture jumped to its start to show where it
// had gone -- and from then on every press inside that clip was another move,
// so the red line could not be put anywhere in the one stretch you were
// working on.
func TestAClickOnAHeldClipMovesTheLineAndNotTheClip(t *testing.T) {
	src := readSrc(t, "cut.go")
	// the threshold: until the pointer has travelled, a press on something
	// held is still a click
	for _, want := range []string{
		"dragSlop = 4.0",
		"if (trimming && !ed.edgeDirty) || (moving && !ed.segDirty) {",
		"if math.Abs(ox) < dragSlop && math.Abs(oy) < dragSlop {",
	} {
		if !strings.Contains(src, want) {
			t.Errorf("cut.go no longer contains %q", want)
		}
	}
	// ...and the release of such a press says what is in hand rather than
	// dragging it. The line is not moved: sliding a clip is the right button's
	// now, and where the red line goes is the left's alone.
	i := strings.Index(src, "if !ed.segDirty && math.Abs(ox) < dragSlop && math.Abs(oy) < dragSlop {")
	if i < 0 {
		t.Fatal("a right click on a held clip is read as a drag of it")
	}
	if j := strings.Index(src[i:], "ed.segStatus() // a right click on a scene is \"this one\""); j < 0 || j > 200 {
		t.Error("the click on a held clip no longer says what it is holding")
	}
	// and the second click of a double one leaves the green alone: the single
	// click has already taken that scene in hand
	if !strings.Contains(src, "if ed.segOnGreen(x+ed.viewX, y) >= 0 {\n\t\t\t\treturn // the single click has it") {
		t.Error("a double click on the green picks the clip up a second time")
	}
	// the threshold is smaller than the border grab, or a press meant for a
	// border would be swallowed before it could trim
	if dragSlop > edgeGrab {
		t.Errorf("the drag threshold (%g) is wider than the border's grab (%g)", dragSlop, edgeGrab)
	}
}
