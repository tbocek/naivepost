package main

import (
	"os"
	"path/filepath"
	"testing"
)

// A recorder opened and closed again leaves a file under a second long. Three
// things used to fail on it, each with an ffmpeg error about something else.

func TestNoVoiceLongEnoughMakesNoAnchorPieces(t *testing.T) {
	rows := []span{{0.1, 0.3, "a"}, {0.4, 0.55, "b"}} // both under anchorCut
	if got := anchorPieces(rows, 12); len(got) != 0 {
		t.Fatalf("pieces from snippets under %.1f s: %+v", anchorCut, got)
	}
}

func TestAnchorPiecesTakeUpToPerSecondsOfEachVoice(t *testing.T) {
	rows := []span{{0, 5, "a"}, {10, 20, "a"}, {30, 31, "b"}}
	got := anchorPieces(rows, 8)
	if len(got) != 3 {
		t.Fatalf("want 3 pieces, got %+v", got)
	}
	if got[0].len != 5 || got[1].len != 3 || got[1].src != 10 {
		t.Errorf("voice a should give 5 s then the 3 s to fill 8: %+v", got[:2])
	}
	if got[2].slot != "b" || got[2].len != 1 {
		t.Errorf("voice b's second is its own piece: %+v", got[2])
	}
}

func TestATakeShorterThanTheIntervalGetsItsOneFrame(t *testing.T) {
	vf, one := frameFilter("fps=0.250000,scale=896:-2", "scale=896:-2", 0.8, 4)
	if !one || vf != "scale=896:-2" {
		t.Errorf("0.8 s at 4 s: want the scale alone and one frame, got %q %v", vf, one)
	}
	vf, one = frameFilter("fps=0.250000,scale=896:-2", "scale=896:-2", 30, 4)
	if one || vf != "fps=0.250000,scale=896:-2" {
		t.Errorf("30 s at 4 s: the graph stands, got %q %v", vf, one)
	}
	if vf, one = frameFilter("scale=896:-2", "scale=896:-2", 0.8, 0); one || vf != "scale=896:-2" {
		t.Errorf("every frame wanted: nothing changes, got %q %v", vf, one)
	}
}

func TestAStartStopIsWrittenUpAsSilence(t *testing.T) {
	out := t.TempDir()
	if err := writeSilence(out); err != nil {
		t.Fatal(err)
	}
	for _, f := range []string{"transcript.txt", "words.json", "turns.json"} {
		if _, err := os.Stat(filepath.Join(out, f)); err != nil {
			t.Errorf("%s: %v", f, err)
		}
	}
	if w := wordTimes(out); len(w) != 0 {
		t.Errorf("words in silence: %+v", w)
	}
	if turns, err := loadSpans(filepath.Join(out, "turns.json")); err != nil || len(turns) != 0 {
		t.Errorf("turns in silence: %+v %v", turns, err)
	}
	// and the segments built from it are the empty-transcript case, not an error
	a := &App{root: t.TempDir()}
	if err := a.mergeSegments(out); err != nil {
		t.Fatalf("segments from silence: %v", err)
	}
}

// The frame count is a rounding of the file's length (what the fps filter
// emits), and no worker is ever sent after a single last frame.

func TestFrameChunksCoverEveryFrameOnceWithoutALoneTail(t *testing.T) {
	for _, c := range []struct{ total, workers, chunks, lastN int }{
		{49, 8, 7, 7}, // 49.25 s at 1 s on 32 cores: 7 x 7, the 8th worker idle
		{50, 8, 7, 8}, // one more frame: a lone tail, folded into the 7th chunk
		{32, 8, 8, 4}, // exact
		{57, 8, 7, 9}, // 7 x 8 would leave 1: folded, the 7th chunk takes 9
	} {
		got := frameChunks(c.total, c.workers, 1)
		if got[len(got)-1].n != c.lastN {
			t.Errorf("%+v: last chunk has %d frames", c, got[len(got)-1].n)
		}
		sum, next := 0, 1
		for _, ch := range got {
			if ch.first != next {
				t.Errorf("%+v: chunk starts at frame %d, want %d", c, ch.first, next)
			}
			if ch.start != float64(ch.first-1) || ch.dur != float64(ch.n) {
				t.Errorf("%+v: chunk %+v is not on the frame clock", c, ch)
			}
			if ch.n == 1 && len(got) > 1 {
				t.Errorf("%+v: a lone-frame chunk %+v", c, ch)
			}
			sum += ch.n
			next += ch.n
		}
		if sum != c.total || len(got) != c.chunks {
			t.Errorf("%+v: %d chunks summing to %d: %+v", c, len(got), sum, got)
		}
	}
}
