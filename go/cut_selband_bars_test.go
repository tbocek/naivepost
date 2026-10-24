package main

import (
	"strings"
	"testing"
)

// The green that says "kept" is drawn over the thumbnails, so on a dark capture
// it is green on black -- and the band only ever carried a bar for ONE scene,
// the one the playhead happened to be inside. Reading what the cut keeps meant
// hunting the tint's edges over the footage, or scrubbing the playhead through
// the timeline one scene at a time. Every kept scene gets a bar now, always.
func TestEveryKeptSceneHasABarInTheBand(t *testing.T) {
	ed := &cutEditor{segs: []cutSeg{
		{S: 0, E: 10},
		{S: 20, E: 30, Cam: 1},
		{S: 30, E: 33, Ins: "card.png"}, // a card is not a stretch of recording
		{S: 40, E: 50},
	}}
	got := ed.bandBars()
	want := []int{0, 1, 3}
	if len(got) != len(want) {
		t.Fatalf("the band draws %d bars for %d kept scenes: %v", len(got), len(want), got)
	}
	for i := range want {
		if got[i] != want[i] {
			t.Errorf("bar %d stands for scene %d, want %d", i, got[i], want[i])
		}
	}
	// no playhead anywhere, nothing in hand: the bars are there regardless,
	// which is the whole of "always"
	if ed.bandClipIdx() != -1 {
		t.Error("this cut is supposed to have no scene in hand and no playhead in one")
	}
	if len(ed.bandBars()) == 0 {
		t.Error("with no playhead the band went blank again")
	}
	// an empty cut draws nothing rather than something odd
	if bars := (&cutEditor{}).bandBars(); len(bars) != 0 {
		t.Errorf("an empty cut drew %d bars", len(bars))
	}

	// and the drawing uses them, rather than the single scene it used to ask for
	body := funcBody(t, "cut_selband.go", `func \(ed \*cutEditor\) drawSelBand\(`)
	if !strings.Contains(body, "ed.bandBars()") {
		t.Error("drawSelBand still draws a bar for one scene only")
	}
	// the scene the hand CAN reach is still drawn differently from the ones it
	// cannot: only that one carries handles and an ✕ (bandClipPartAt)
	if !strings.Contains(body, "cur := ed.bandClipIdx()") || !strings.Contains(body, "i == cur") {
		t.Error("the scene in hand is drawn like every other bar, so the row promises handles it does not have")
	}
}
