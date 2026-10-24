package main

// The scene-change scores are computed on a worker goroutine while the window
// stays alive, so they must not touch GTK: a pixbuf is a GObject, and gotk4
// finishes a GObject's reference bookkeeping on the main loop, which is a race
// the heap does not survive. These tests hold the Go-only decoding to the same
// job the pixbuf one did -- shrink a frame to a postage stamp, diff consecutive
// stamps -- and pin the threading rule in the source, because nothing at run
// time will complain until it corrupts memory.

import (
	"image"
	"image/jpeg"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// writeFrame paints one flat-coloured half-and-half frame: the left fraction
// `split` of the width black, the rest at `level`. Two such frames differ by
// exactly as much as their levels and splits differ, which makes the scores
// something a test can reason about rather than eyeball.
func writeFrame(t *testing.T, path string, split float64, level uint8) {
	t.Helper()
	img := image.NewYCbCr(image.Rect(0, 0, 320, 180), image.YCbCrSubsampleRatio420)
	for y := 0; y < 180; y++ {
		for x := 0; x < 320; x++ {
			v := uint8(0)
			if float64(x) >= split*320 {
				v = level
			}
			img.Y[img.YOffset(x, y)] = v
		}
	}
	f, err := os.Create(path)
	if err != nil {
		t.Fatal(err)
	}
	defer f.Close()
	if err := jpeg.Encode(f, img, &jpeg.Options{Quality: 95}); err != nil {
		t.Fatal(err)
	}
}

func TestFramePostageAveragesWholeBoxes(t *testing.T) {
	dir := t.TempDir()
	p := filepath.Join(dir, "half.jpg")
	writeFrame(t, p, 0.5, 255)

	px := framePostage(p)
	if len(px) != postW*postH {
		t.Fatalf("postage is %d cells, want %d", len(px), postW*postH)
	}
	// left half dark, right half bright, on every row: a stamp that lost the
	// picture's shape would fail this even with the right average brightness
	for row := 0; row < postH; row++ {
		if got := px[row*postW]; got > 20 {
			t.Fatalf("row %d starts at %d, want near black", row, got)
		}
		if got := px[row*postW+postW-1]; got < 235 {
			t.Fatalf("row %d ends at %d, want near white", row, got)
		}
	}
}

func TestFramePostageIsQuietAboutWhatItCannotRead(t *testing.T) {
	dir := t.TempDir()
	junk := filepath.Join(dir, "torn.jpg")
	if err := os.WriteFile(junk, []byte("\xff\xd8 half a frame"), 0o644); err != nil {
		t.Fatal(err)
	}
	if px := framePostage(junk); px != nil {
		t.Fatalf("a torn frame decoded to %d cells, want nil", len(px))
	}
	if px := framePostage(filepath.Join(dir, "not-there.jpg")); px != nil {
		t.Fatalf("a missing frame decoded to %d cells, want nil", len(px))
	}
}

func TestFrameChangeScoresPeakWhereThePictureChanges(t *testing.T) {
	dir := t.TempDir()
	var frames []string
	// four frames of the same shot, then the shot changes and holds
	for i, s := range []struct {
		split float64
		level uint8
	}{{0.5, 255}, {0.5, 255}, {0.5, 255}, {0.1, 255}, {0.1, 255}} {
		p := filepath.Join(dir, string(rune('a'+i))+".jpg")
		writeFrame(t, p, s.split, s.level)
		frames = append(frames, p)
	}

	sc := frameChangeScores(frames)
	if len(sc) != len(frames) {
		t.Fatalf("got %d scores for %d frames", len(sc), len(frames))
	}
	if sc[0] != 0 {
		t.Fatalf("the first frame has nothing to be different from, got %g", sc[0])
	}
	for _, i := range []int{1, 2, 4} {
		if sc[i] > 1 {
			t.Fatalf("frame %d is the same picture as the one before, scored %g", i, sc[i])
		}
	}
	// the cut is at 3, and it has to stand out rather than merely be the
	// largest of a set of near-identical numbers
	if sc[3] < 50 {
		t.Fatalf("the scene change scored %g, want it well clear of the noise", sc[3])
	}
}

// The point of the whole exercise: no GObject is made off the main thread. If
// someone reaches for the pixbuf loader again here -- it is one line shorter --
// the app goes back to aborting inside an unrelated finalizer minutes later,
// which is a bug nobody would trace back to this function.
func TestTheScoresDecodeWithoutGdkPixbuf(t *testing.T) {
	b, err := os.ReadFile("cut.go")
	if err != nil {
		t.Fatal(err)
	}
	src := string(b)
	fn := src[strings.Index(src, "func frameChangeScores("):]
	fn = fn[:strings.Index(fn, "\n// ---- geometry")]
	if strings.Contains(fn, "gdkpixbuf.") {
		t.Fatal("frameChangeScores/framePostage make GdkPixbufs, and they run on a worker goroutine")
	}
	if !strings.Contains(fn, "image.Decode(f)") {
		t.Fatal("the frames are no longer decoded by the Go stdlib")
	}
	if !strings.Contains(src, "sc := frameChangeScores(v.frames)") {
		t.Fatal("the scores are no longer computed off the main thread; this pin needs rethinking")
	}
}
