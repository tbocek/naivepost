package main

// Inserts on the cut timeline.
//
// The cut is a sorted, non-overlapping list of spans of session time, and every
// operation on it was written on the assumption that a span means "show the
// footage that was recorded here". An insert breaks that assumption in exactly
// one place -- it brings its own picture -- and the whole of the feature is
// making sure the other places do not quietly act on the old one.
//
// So these are the four ways an insert can be silently destroyed: merged into
// the clip beside it, trimmed to a fraction of a file, dropped for having no
// footage under it, or thrown away by a re-suggest that was never told it
// existed. Each one loses a file the user placed by hand and leaves the seconds
// behind, which reads as "the card vanished" long after the edit that did it.

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func insertEd(t *testing.T) *cutEditor {
	t.Helper()
	ed := newTestEd(t)
	ed.a.ed = ed // keepFilmed reaches back through the App
	return ed
}

// Merging is for two selections of the same footage that turned out to touch.
// An insert is a file: swallowing one into the clip beside it keeps the seconds
// and loses the card, and growing one over the footage after it plays the card
// where the footage should be.
func TestCoalesceNeverMergesAnInsert(t *testing.T) {
	ed := insertEd(t)
	ed.segs = []cutSeg{
		{S: 0, E: 10},
		{S: 10, E: 20},                        // touches the one before: footage, so it merges
		{S: 20, E: 24, Ins: "/a/later.mp4"},   // touches both sides, and must not
		{S: 24, E: 28, Ins: "/a/ranking.svg"}, // two cards back to back stay two cards
		{S: 28, E: 40},
	}
	ed.coalesce()
	want := []cutSeg{
		{S: 0, E: 20},
		{S: 20, E: 24, Ins: "/a/later.mp4"},
		{S: 24, E: 28, Ins: "/a/ranking.svg"},
		{S: 28, E: 40},
	}
	if !sameCut(ed.segs, want) {
		t.Fatalf("coalesce gave %v,\nwant %v", ed.segs, want)
	}
}

// Sorting still applies: an insert placed at a time before the segments already
// in the list has to land in its place, or the render walks the cut out of
// order and the concat list is shuffled.
func TestAnInsertSortsIntoPlaceLikeAnythingElse(t *testing.T) {
	ed := insertEd(t)
	ed.segs = []cutSeg{{S: 100, E: 110}, {S: 5, E: 9, Ins: "/a/intro.svg"}, {S: 40, E: 50}}
	ed.coalesce()
	for i := 1; i < len(ed.segs); i++ {
		if ed.segs[i].S < ed.segs[i-1].S {
			t.Fatalf("the cut came out of order: %v", ed.segs)
		}
	}
	if !ed.segs[0].isInsert() {
		t.Errorf("the insert at 5 s is not first: %v", ed.segs)
	}
}

// Footage is a span and can be cut down to part of itself. An insert is a file
// and cannot: half of a title card is a clip nobody asked for, showing the top
// of a graphic for two seconds. So an overlapped insert goes whole.
func TestAnInsertIsDroppedWholeOrNotAtAll(t *testing.T) {
	ed := insertEd(t)
	ed.segs = []cutSeg{
		{S: 0, E: 30},                      // trimmed on the right
		{S: 40, E: 48, Ins: "/a/card.png"}, // clipped at its head -- goes entirely
		{S: 60, E: 90},                     // trimmed on the left
		{S: 95, E: 99, Ins: "/a/keep.svg"}, // untouched
	}
	ed.removeSpan(25, 65)
	want := []cutSeg{{S: 0, E: 25}, {S: 65, E: 90}, {S: 95, E: 99, Ins: "/a/keep.svg"}}
	if !sameCut(ed.segs, want) {
		t.Fatalf("removeSpan gave %v,\nwant %v", ed.segs, want)
	}
	// and the removal that only grazes an insert still takes all of it, because
	// there is no such thing as most of a file
	ed.segs = []cutSeg{{S: 40, E: 48, Ins: "/a/card.png"}}
	ed.removeSpan(47.9, 60)
	if len(ed.segs) != 0 {
		t.Errorf("a grazed insert survived as %v -- it would render as a fraction of the card", ed.segs)
	}
}

// Dropping a card into the middle of a clip is the honest reading of the
// gesture: the footage under it gives way, exactly as Remove would, and the
// insert takes the seconds between.
func TestAddInsertClearsTheFootageUnderIt(t *testing.T) {
	ed := insertEd(t)
	ed.segs = []cutSeg{{S: 0, E: 60}}
	ed.addInsert("/a/later.mp4", 20, 4, false)
	want := []cutSeg{{S: 0, E: 20}, {S: 20, E: 24, Ins: "/a/later.mp4"}, {S: 24, E: 60}}
	if !sameCut(ed.segs, want) {
		t.Fatalf("addInsert gave %v,\nwant %v", ed.segs, want)
	}
	// undoable like every other edit
	if len(ed.undo) != 1 || !sameCut(ed.undo[0].segs, []cutSeg{{S: 0, E: 60}}) {
		t.Errorf("addInsert pushed %v for undo, want the cut as it was", ed.undo)
	}
	// the split halves must not then merge back over the insert
	ed.coalesce()
	if len(ed.segs) != 3 {
		t.Errorf("after a further coalesce the cut is %v -- the card was swallowed", ed.segs)
	}
}

// The space between two recordings is session time nobody filmed, so a card
// there takes nothing away.
func TestAddInsertInAGapCostsNothing(t *testing.T) {
	ed := insertEd(t)
	ed.segs = []cutSeg{{S: 0, E: 30}, {S: 100, E: 130}}
	ed.addInsert("/a/ranking.svg", 50, 6, false)
	want := []cutSeg{{S: 0, E: 30}, {S: 50, E: 56, Ins: "/a/ranking.svg"}, {S: 100, E: 130}}
	if !sameCut(ed.segs, want) {
		t.Fatalf("addInsert into a gap gave %v,\nwant %v", ed.segs, want)
	}
}

// A length nothing could supply -- an image has none, and a file that would not
// probe reports zero -- becomes the default rather than a zero-length clip that
// ffmpeg either refuses or renders as a single frame.
func TestAnInsertWithNoLengthGetsTheDefault(t *testing.T) {
	ed := insertEd(t)
	ed.addInsert("/a/card.png", 10, 0, false)
	if len(ed.segs) != 1 {
		t.Fatalf("got %v, want one insert", ed.segs)
	}
	if got := ed.segs[0].E - ed.segs[0].S; got != insDefault {
		t.Errorf("a lengthless insert runs %gs, want the %gs default", got, insDefault)
	}
}

// Suggest and Audit both replace the footage wholesale, and no model was ever
// told the inserts exist -- a run that came back without them has not decided
// against them, it never saw them. So the two are partitioned rather than
// merged, and every segment belongs to exactly one side.
func TestTheFootageAndTheInsertsPartitionTheCut(t *testing.T) {
	segs := []cutSeg{
		{S: 0, E: 10},
		{S: 10, E: 14, Ins: "/a/later.mp4"},
		{S: 20, E: 30},
		{S: 30, E: 36, Ins: "/a/ranking.svg"},
	}
	ins, filmed := insertsOf(segs), filmedOf(segs)
	if len(ins) != 2 || len(filmed) != 2 {
		t.Fatalf("split %d inserts and %d filmed out of %d segments", len(ins), len(filmed), len(segs))
	}
	if len(ins)+len(filmed) != len(segs) {
		t.Error("a segment is on both sides or on neither")
	}
	for _, s := range ins {
		if !s.isInsert() {
			t.Errorf("insertsOf returned footage: %v", s)
		}
	}
	for _, s := range filmed {
		if s.isInsert() {
			t.Errorf("filmedOf returned an insert: %v", s)
		}
	}
	// what a re-suggest does: the model's footage, the user's cards
	fresh := append(insertsOf(segs), cutSeg{S: 50, E: 70})
	if len(insertsOf(fresh)) != 2 {
		t.Errorf("a suggestion dropped the hand-placed cards: %v", fresh)
	}
}

// keepFilmed exists to stop a hole in the video: a span nobody recorded is a
// span with nothing to show. An insert's normal state is having no recording
// under it -- that is the entire point of one -- so the check must not apply.
func TestKeepFilmedKeepsAnInsertOverNothing(t *testing.T) {
	ed := insertEd(t)
	ed.vids = []tlVideo{{start: 0, dur: 100}}
	got := ed.a.keepFilmed([]cutSeg{
		{S: 10, E: 20},                       // filmed
		{S: 500, E: 520},                     // nothing there: dropped
		{S: 500, E: 506, Ins: "/a/card.svg"}, // nothing there, and kept
		{S: 95, E: 140, Ins: "/a/later.mp4"}, // straddling the end, also kept
	})
	want := []cutSeg{
		{S: 10, E: 20},
		{S: 500, E: 506, Ins: "/a/card.svg"},
		{S: 95, E: 140, Ins: "/a/later.mp4"},
	}
	if !sameCut(got, want) {
		t.Fatalf("keepFilmed gave %v,\nwant %v", got, want)
	}
}

// cut.json is the handoff to Produce, so a path that does not survive the write
// is a card that renders as black footage of the seconds it covered.
func TestAnInsertSurvivesCutJson(t *testing.T) {
	ed := insertEd(t)
	ed.segs = []cutSeg{{S: 0, E: 10}, {S: 10, E: 14, Ins: "assets/ranking.svg"}}
	ed.persist()

	// read back the way Produce reads it: a different App, no editor in memory
	fresh := &App{outDir: ed.a.outDir}
	got := fresh.produceSegs()
	if !sameCut(got, ed.segs) {
		t.Fatalf("cut.json round-tripped to %v, want %v", got, ed.segs)
	}
	if !got[1].isInsert() || got[1].Ins != "assets/ranking.svg" {
		t.Errorf("the asset path did not survive: %+v", got[1])
	}
	// ...and an ordinary cut is unchanged on disk, so a cut.json written by an
	// older build still loads and one written by this build still opens there
	ed.segs = []cutSeg{{S: 1, E: 5}}
	ed.persist()
	b, err := os.ReadFile(filepath.Join(ed.a.cutDir(), "cut.json"))
	if err != nil {
		t.Fatal(err)
	}
	if strings.Contains(string(b), `"ins"`) {
		t.Errorf("a cut with no inserts writes an ins field:\n%s", b)
	}
}

// ---- the two modes ----------------------------------------------------------
//
// A card can be put in a cut in two ways and they do opposite things to the
// footage: over it, which costs the seconds it runs, or between it, which costs
// nothing and makes the video longer by exactly the card. Both are wanted. What
// is not wanted is the button called Insert quietly doing the first one, which
// is a card that appears to work and a session eight seconds shorter than it
// was -- and, since the sound goes with the footage, audio that stops under the
// card and picks up eight seconds further on instead of where it left off.

// The gesture decides the mode: a card dropped at the playhead is inserted, a
// card put over a marked selection covers what was marked. Pinned on the source
// because the decision is made in a file dialog's callback, which no test can
// open.
func TestACardAtThePlayheadIsInsertedAndOneOverASelectionCovers(t *testing.T) {
	b, err := os.ReadFile("cut.go")
	if err != nil {
		t.Fatal(err)
	}
	src := string(b)
	for _, want := range []string{
		// the length and the mode are one answer: the selection gave it both
		"m := insMode{dur: want, splice: want < minSegLn, lane: lane}",
		// and the dialog says which it is doing, as a pair of radio buttons
		"between := gtk.NewCheckButtonWithLabel(",
		"over.SetGroup(between)",
		// ...three of them now: over the footage, between it, or beside it on a
		// row of its own (cut_lane.go)
		"own.SetGroup(between)",
		"out := insMode{splice: between.Active(), asLane: own.Active(), dur: m.dur,\n\t\t\tmute: m.mute, lane: m.lane, askMute: m.askMute}",
	} {
		if !strings.Contains(src, want) {
			t.Errorf("the insert dialog no longer contains %q", want)
		}
	}
}

// The whole of what "between" means, in the two places it is read: the timeline
// keeps the footage whole with a point in it, and the render is the two halves
// with the card in between.
func TestASplicedCardKeepsTheFootageAndLengthensTheCut(t *testing.T) {
	ed := insertEd(t)
	ed.segs = []cutSeg{{S: 0, E: 60}}
	ed.addSplice("assets/tier.svg", 20, 3, false, 0)

	// on the timeline: one clip of footage, still 60 s of it, and a card at a
	// point inside it that takes none of it
	want := []cutSeg{{S: 0, E: 60}, {S: 20, E: 20, Ins: "assets/tier.svg", Dur: 3}}
	if !sameCut(ed.segs, want) {
		t.Fatalf("splicing gave %v,\nwant %v", ed.segs, want)
	}
	filmed := 0.0
	for _, s := range filmedOf(ed.segs) {
		filmed += s.length()
	}
	if filmed != 60 {
		t.Errorf("the footage is %.1f s after a splice, want all 60 of it", filmed)
	}

	// as a sequence: footage up to the split, the card, the rest of the footage
	got := ed.a.produceSegs()
	seq := []cutSeg{
		{S: 0, E: 20},
		{S: 20, E: 20, Ins: "assets/tier.svg", Dur: 3},
		{S: 20, E: 60},
	}
	if !sameCut(got, seq) {
		t.Fatalf("the render sequence is %v,\nwant %v", got, seq)
	}
	total := 0.0
	for _, s := range got {
		total += s.length()
	}
	if total != 63 {
		t.Errorf("the cut runs %.1f s, want 63 -- 60 of footage and the 3 s card", total)
	}
	// the second half starts at the frame the first half stopped on, which is
	// the sentence about the sound: it resumes where it stopped
	if got[0].E != got[2].S {
		t.Errorf("the footage resumes at %.1f s after stopping at %.1f",
			got[2].S, got[0].E)
	}
}

// The other mode is unchanged, and is the one that costs footage: the seconds
// under the card are gone from the cut, exactly as Remove would take them.
func TestAnOverwritingCardStillCostsTheSecondsItRuns(t *testing.T) {
	ed := insertEd(t)
	ed.segs = []cutSeg{{S: 0, E: 60}}
	ed.addInsert("assets/tier.svg", 20, 3, false)
	total := 0.0
	for _, s := range ed.a.produceSegs() {
		total += s.length()
	}
	if total != 60 {
		t.Errorf("an overwriting card gave a cut of %.1f s, want the same 60", total)
	}
}

// The number that answers "did it get longer": the cut's own length, which is
// what the finished video will run. The timeline cannot answer it -- that is the
// session's clock, and it is as long as the recording whatever is done here --
// so this is the figure under the tracks and in the status line after the edit.
func TestASplicedCardIsThreeSecondsOfVideoAndAnOverwritingOneIsNone(t *testing.T) {
	ed := insertEd(t)
	ed.segs = []cutSeg{{S: 0, E: 60}}
	was := ed.cutLen()

	ed.addSplice("assets/tier.svg", 20, 3, false, 0)
	if got := ed.cutLen(); got != was+3 {
		t.Errorf("a 3 s spliced card made the cut %.1f s, was %.1f — it has to be %.1f",
			got, was, was+3)
	}
	ed.segs = []cutSeg{{S: 0, E: 60}}
	ed.addInsert("assets/tier.svg", 20, 3, false)
	if got := ed.cutLen(); got != was {
		t.Errorf("a 3 s overwriting card made the cut %.1f s, want the same %.1f", got, was)
	}
}
