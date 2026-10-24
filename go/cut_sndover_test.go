package main

// A sound laid over the footage, and the picture it must not move.
//
// A sound insert is a segment like every other segment, which is what makes it
// dangerous. Segments are how the cut says which seconds are IN the video, so
// putting one down where the cut had thrown the footage away puts those
// seconds back -- picture and all -- and the hand that did it was pointing at
// a waveform. The same trespass has two quieter forms: removeSpan drops a
// leftover of clip shorter than a scene, so a sound laid half a second after a
// clip starts used to cost that half second of picture; and it drops an insert
// whole rather than trimming it, so a sound laid across a title card used to
// delete the card.
//
// layOverSound is the answer: the span is cut to the footage the cut already
// keeps, split exactly, and one sound goes over each surviving piece with its
// own offset into the file. What these tests hold is the promise that follows
// from it -- lay a sound anywhere, and the cut is the same length, of the same
// footage, afterwards.

import (
	"math"
	"os"
	"strings"
	"testing"
)

// segsEqual reports the first segment that moved, so a failure says which one
// rather than dumping two lists and leaving the reading to the eye.
func segsEqual(t *testing.T, got, want []cutSeg) {
	t.Helper()
	if len(got) != len(want) {
		t.Fatalf("the cut has %d segments, want %d:\n got %+v\nwant %+v",
			len(got), len(want), got, want)
	}
	for i := range got {
		if math.Abs(got[i].S-want[i].S) > 1e-9 || math.Abs(got[i].E-want[i].E) > 1e-9 ||
			got[i].Ins != want[i].Ins || math.Abs(got[i].Ss-want[i].Ss) > 1e-9 {
			t.Errorf("segment %d is %+v, want %+v", i, got[i], want[i])
		}
	}
}

// The whole point, in one test: footage the cut drops stays dropped. A sound
// is laid OVER a picture, so where there is no picture there is nothing to lay
// it over, and the honest answer is to place nothing and say so.
func TestASoundLaidWhereTheCutKeepsNothingChangesNothing(t *testing.T) {
	_, ed := selEd(t)
	ed.segs = []cutSeg{{S: 0, E: 20}, {S: 40, E: 60}}
	was, wasLen := append([]cutSeg(nil), ed.segs...), ed.cutLen()

	if n := ed.addSound("mic.wav", 25, 10, 0, ""); n != 0 { // 25 – 35, inside the hole at 20 – 40
		t.Errorf("a sound dropped into a cut-out stretch went over %d pieces of footage, want none", n)
	}
	segsEqual(t, ed.segs, was)
	if got := ed.cutLen(); math.Abs(got-wasLen) > 1e-9 {
		t.Errorf("the cut is %.1f s and was %.1f s — sound brought the picture back with it",
			got, wasLen)
	}
}

// A selection drawn across a hole is one sound, not two, so both halves have to
// know how far into the file they are: the second piece starts where the first
// would have got to had the cut kept the seconds between. Playing the file's
// opening seconds again on the far side would be the same sound twice.
func TestASoundDrawnAcrossAHoleGoesOnBothSidesOfIt(t *testing.T) {
	_, ed := selEd(t)
	ed.segs = []cutSeg{{S: 0, E: 20}, {S: 40, E: 60}}
	wasLen := ed.cutLen()

	if n := ed.addSound("mic.wav", 15, 30, 0, ""); n != 2 { // 15 – 45, over the hole at 20 – 40
		t.Fatalf("a sound across a hole went over %d pieces of footage, want 2", n)
	}
	segsEqual(t, ed.segs, []cutSeg{
		{S: 0, E: 15},
		{S: 15, E: 20, Ins: "mic.wav"},
		{S: 40, E: 45, Ins: "mic.wav", Ss: 25}, // the file ran on under the hole
		{S: 45, E: 60},
	})
	if got := ed.cutLen(); math.Abs(got-wasLen) > 1e-9 {
		t.Errorf("the cut is %.1f s, was %.1f s", got, wasLen)
	}
}

// removeSpan refuses to leave a clip shorter than a scene, which is right when
// the seconds are being thrown away and wrong here: the sliver is not a scene
// anybody chose, it is the picture carrying on, and losing it would shorten
// the video by half a second nobody asked about.
func TestASoundLaidJustAfterAClipStartsKeepsTheSliverOfPicture(t *testing.T) {
	_, ed := selEd(t)
	ed.segs = []cutSeg{{S: 0, E: 20}}

	if n := ed.addSound("mic.wav", 0.5, 10, 0, ""); n != 1 {
		t.Fatalf("went over %d pieces of footage, want 1", n)
	}
	segsEqual(t, ed.segs, []cutSeg{
		{S: 0, E: 0.5}, // shorter than minSegLn, and kept anyway
		{S: 0.5, E: 10.5, Ins: "mic.wav"},
		{S: 10.5, E: 20},
	})
	if got := ed.cutLen(); math.Abs(got-20) > 1e-9 {
		t.Errorf("the cut is %.2f s, want 20 — half a second of picture went missing", got)
	}
}

// An insert is dropped whole or not at all, so a sound that overlapped a card
// used to take the card away with it. It steps over one instead: the card
// keeps its seconds and the sound keeps its clock across them.
func TestASoundLaidAcrossACardLeavesTheCardAlone(t *testing.T) {
	_, ed := selEd(t)
	ed.segs = []cutSeg{{S: 0, E: 60}}
	ed.addInsert("card.svg", 20, 4, false)
	wasLen := ed.cutLen()

	if n := ed.addSound("mic.wav", 18, 10, 0, ""); n != 2 { // 18 – 28, the card holds 20 – 24
		t.Fatalf("went over %d pieces of footage, want 2", n)
	}
	segsEqual(t, ed.segs, []cutSeg{
		{S: 0, E: 18},
		{S: 18, E: 20, Ins: "mic.wav"},
		{S: 20, E: 24, Ins: "card.svg"}, // untouched
		{S: 24, E: 28, Ins: "mic.wav", Ss: 6},
		{S: 28, E: 60},
	})
	if got := ed.cutLen(); math.Abs(got-wasLen) > 1e-9 {
		t.Errorf("the cut is %.1f s, was %.1f s — the card lost or gained seconds", got, wasLen)
	}
}

// The general promise, over the awkward placements: whatever a sound is laid
// over, the video is exactly as long afterwards as before.
func TestLayingSoundOverTheFootageNeverChangesTheCutsLength(t *testing.T) {
	for _, c := range []struct{ at, dur float64 }{
		{0, 5}, {0.2, 3}, {17, 6}, {19.5, 1.5}, {0, 20}, {5, 100}, {58, 40},
	} {
		_, ed := selEd(t)
		ed.segs = []cutSeg{{S: 0, E: 20}, {S: 40, E: 60}}
		was := ed.cutLen()
		ed.addSound("mic.wav", c.at, c.dur, 0, "")
		if got := ed.cutLen(); math.Abs(got-was) > 1e-9 {
			t.Errorf("a sound at %.1f s for %.1f s made the cut %.2f s, was %.2f s",
				c.at, c.dur, got, was)
		}
	}
}

// A paste that lands nowhere is a miss, not a failure: the seconds are still in
// hand, so moving the red line and pressing again is the whole of the recovery.
// Dropping the copy would mean going back to the lane and taking it a second
// time to correct an aim.
func TestAPasteWithNoPictureUnderItKeepsTheCopyInHand(t *testing.T) {
	a, ed := selEd(t)
	ed.segs = []cutSeg{{S: 0, E: 20}, {S: 40, E: 60}}
	ed.copyFrom, ed.copyLen, ed.copyOn, ed.copyAud = 5, 10, true, "mic"
	ed.playhead, ed.hasPlay = 25, true
	was, wasLen := append([]cutSeg(nil), ed.segs...), ed.cutLen()

	a.pasteCopy()
	if !ed.copyOn {
		t.Error("the copy was taken out of hand by a paste that placed nothing")
	}
	segsEqual(t, ed.segs, was)
	if got := ed.cutLen(); math.Abs(got-wasLen) > 1e-9 {
		t.Errorf("the cut is %.1f s, was %.1f s", got, wasLen)
	}
	// and the same paste onto kept footage does land, so the refusal above is
	// about where the red line was and not about the copy being unusable
	ed.playhead = 10
	a.pasteCopy()
	if ed.copyOn {
		t.Error("a paste onto kept footage left the copy in hand")
	}
	if got := ed.cutLen(); math.Abs(got-wasLen) > 1e-9 {
		t.Errorf("the paste made the cut %.1f s, was %.1f s", got, wasLen)
	}
}

// The seams: the one place a sound meets the cut, the exact splits, and the two
// doors that now lead to it -- ⧉ Paste and a sound file chosen from disk. A
// file insert that still went through addInsert would displace footage again
// with nothing on the page saying it had.
func TestTheSoundOverlayIsWired(t *testing.T) {
	pins := map[string][]string{
		"cut.go": {
			"func (ed *cutEditor) layOverSound(s cutSeg) int {",
			"if f.isInsert() || t1-t0 < sndMinLn {", // cards and blinks are stepped over
			"out = append(out, cutSeg{S: t0, E: t1, Ins: s.Ins, Ss: s.Ss + t0 - s.S, Lane: s.Lane})",
			"n := ed.addSound(a.storePath(au.path), at, ed.copyLen, ss, ed.copyAud)",
			`case insKind(file) == "audio":`, // the chooser's sound goes the same way
			"n := a.ed.addSound(rel, at, m.dur, 0, m.lane)",
		},
	}
	for file, want := range pins {
		src, err := os.ReadFile(file)
		if err != nil {
			t.Fatal(err)
		}
		for _, w := range want {
			if !strings.Contains(string(src), w) {
				t.Errorf("%s no longer contains %q", file, w)
			}
		}
	}
}
