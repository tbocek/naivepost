package main

// Output files that name a moment name it the same way the recorders do, to
// the second (stampName), and the ones sharing a second are told apart by a
// suffix (stampSeq). Both halves have a way to go wrong that is invisible in a
// listing of the happy case: a second holding two frames, which is any interval
// under a second, and a folder written by a version that numbered its frames.

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"sort"
	"strings"
	"testing"
	"time"
)

func TestStampNamesReadBackAsTheirOwnTime(t *testing.T) {
	base := time.Date(2026, 8, 8, 19, 59, 0, 0, time.Local)
	for _, off := range []float64{0, 0.4, 0.999, 1, 59, 60, 3600, 86399} {
		u := float64(base.Unix()) + off
		name := stampName(u)
		sec, sub, ok := readStamp(name + ".jpg")
		if !ok {
			t.Fatalf("%q does not read back as a stamp", name)
		}
		// floored, not rounded: a frame belongs to the second it was shot in
		if want := float64(int64(u)); sec != want {
			t.Errorf("%g -> %q -> %g, want %g", u, name, sec, want)
		}
		if sub != 0 {
			t.Errorf("%q read back as frame %d of its second, want the first", name, sub)
		}
	}
	// anything without a stamp says so rather than guessing: the numbered names
	// from before this are exactly that case, and they sort by name instead
	for _, n := range []string{"f000001.jpg", "concat.txt", "2026-08-08.jpg", "cut.json"} {
		if _, _, ok := readStamp(n); ok {
			t.Errorf("%q was read as a timestamp", n)
		}
	}
}

// The suffix rule and the sort have to agree, and the obvious sort disagrees:
// '-' is 0x2D and '.' is 0x2E, so "19-59-00-1.jpg" sorts BEFORE "19-59-00.jpg"
// and every extra frame of a second lands in front of the frame it follows.
// Describe sends frames to the model in listing order and the timeline draws
// them in it, so this is not cosmetic.
func TestFramesSharingASecondStayInOrder(t *testing.T) {
	start := float64(time.Date(2026, 8, 8, 19, 59, 0, 0, time.Local).Unix())
	var seq stampSeq
	var names []string
	for i := 0; i < 8; i++ { // 4 fps: four frames to a second
		names = append(names, seq.name(start+float64(i)*0.25, ".jpg"))
	}
	want := []string{
		"2026-08-08_19-59-00.jpg", "2026-08-08_19-59-00-1.jpg",
		"2026-08-08_19-59-00-2.jpg", "2026-08-08_19-59-00-3.jpg",
		"2026-08-08_19-59-01.jpg", "2026-08-08_19-59-01-1.jpg",
		"2026-08-08_19-59-01-2.jpg", "2026-08-08_19-59-01-3.jpg",
	}
	for i := range want {
		if names[i] != want[i] {
			t.Fatalf("frame %d is %q, want %q", i, names[i], want[i])
		}
	}

	shuffled := []string{want[5], want[0], want[7], want[4], want[2], want[1], want[6], want[3]}
	sortStamped(shuffled)
	for i := range want {
		if shuffled[i] != want[i] {
			t.Errorf("sorted position %d is %q, want %q", i, shuffled[i], want[i])
		}
	}
	byString := append([]string(nil), want...)
	sort.Strings(byString)
	if byString[0] == want[0] {
		t.Error("plain string sort now agrees -- this test is guarding nothing")
	}
}

// stampFrames is the rename that turns ffmpeg's counting into wall-clock names.
// What it must not do is take the times from the position in the listing: a
// chunk that comes up short leaves a hole in the numbering, and t = (n-1) *
// interval has to survive it.
func TestFrameNamesFollowTheWallClock(t *testing.T) {
	if _, err := exec.LookPath("ffprobe"); err != nil {
		t.Skip("no ffprobe on PATH")
	}
	dir := t.TempDir()
	// the caller reads the start off the name (srcClock); this is that moment
	video := filepath.Join(dir, "cam-20260808-195900-0.mp4")
	start := float64(time.Date(2026, 8, 8, 19, 59, 0, 0, time.Local).Unix())
	if err := exec.Command("ffmpeg", "-v", "error", "-y", "-f", "lavfi",
		"-i", "testsrc=size=64x64:rate=10:duration=4", video).Run(); err != nil {
		t.Skipf("no usable ffmpeg: %v", err)
	}

	for _, c := range []struct {
		name     string
		nums     []int
		interval float64
		want     []string
	}{
		{"one a second", []int{1, 2, 3}, 1, []string{
			"2026-08-08_19-59-00.jpg", "2026-08-08_19-59-01.jpg", "2026-08-08_19-59-02.jpg"}},
		{"two a second", []int{1, 2, 3, 4}, 0.5, []string{
			"2026-08-08_19-59-00.jpg", "2026-08-08_19-59-00-1.jpg",
			"2026-08-08_19-59-01.jpg", "2026-08-08_19-59-01-1.jpg"}},
		{"every three", []int{1, 2, 3}, 3, []string{
			"2026-08-08_19-59-00.jpg", "2026-08-08_19-59-03.jpg", "2026-08-08_19-59-06.jpg"}},
		// a short chunk: frame 2 was never written, and 3 must still be t=2s
		{"a hole in the numbering", []int{1, 3}, 1, []string{
			"2026-08-08_19-59-00.jpg", "2026-08-08_19-59-02.jpg"}},
	} {
		fdir := filepath.Join(dir, strings.ReplaceAll(c.name, " ", "-"))
		if err := os.MkdirAll(fdir, 0o755); err != nil {
			t.Fatal(err)
		}
		for _, n := range c.nums {
			if err := os.WriteFile(filepath.Join(fdir, fmt.Sprintf("f%06d.jpg", n)), []byte("x"), 0o644); err != nil {
				t.Fatal(err)
			}
		}
		// the marker file lives in the same folder and is not a frame
		if err := os.WriteFile(filepath.Join(fdir, ".interval"), []byte("1|original\n"), 0o644); err != nil {
			t.Fatal(err)
		}
		n, err := stampFrames(fdir, video, start, c.interval)
		if err != nil {
			t.Fatalf("%s: %v", c.name, err)
		}
		if n != len(c.nums) {
			t.Errorf("%s: reported %d frames renamed, want %d", c.name, n, len(c.nums))
		}
		ents, err := os.ReadDir(fdir)
		if err != nil {
			t.Fatal(err)
		}
		var got []string
		for _, e := range ents {
			if strings.HasSuffix(e.Name(), ".jpg") {
				got = append(got, e.Name())
			}
		}
		sortStamped(got)
		if strings.Join(got, " ") != strings.Join(c.want, " ") {
			t.Errorf("%s: frames are\n  %v\nwant\n  %v", c.name, got, c.want)
		}
		if _, err := os.Stat(filepath.Join(fdir, ".interval")); err != nil {
			t.Errorf("%s: the marker file was renamed too: %v", c.name, err)
		}
		// Prepare re-runs over a finished folder, which is how an old one gets
		// its names: the second pass must find nothing left to do rather than
		// stamping the stamps again
		if n, err := stampFrames(fdir, video, start, c.interval); err != nil || n != 0 {
			t.Errorf("%s: a second pass renamed %d frames (%v), want none", c.name, n, err)
		}
	}
}

// A project extracted before frames were timestamped keeps its f000001.jpg
// names -- re-running Prepare is minutes of decoding, and the marker file says
// there is nothing to redo. Both namings therefore have to plan and order.
func TestPlanVideoReadsBothFrameNamings(t *testing.T) {
	for _, c := range []struct {
		what  string
		names []string
	}{
		{"numbered", []string{"f000001.jpg", "f000002.jpg", "f000010.jpg"}},
		{"stamped", []string{"2026-08-08_19-59-00.jpg", "2026-08-08_19-59-00-1.jpg",
			"2026-08-08_19-59-01.jpg"}},
	} {
		a := &App{outDir: t.TempDir()}
		video := "/nowhere/cam-20260808-195900-0.mp4"
		fdir := filepath.Join(a.inputsDir(), "frames", baseName(video))
		if err := os.MkdirAll(fdir, 0o755); err != nil {
			t.Fatal(err)
		}
		for _, n := range c.names {
			if err := os.WriteFile(filepath.Join(fdir, n), []byte("x"), 0o644); err != nil {
				t.Fatal(err)
			}
		}
		if err := os.WriteFile(filepath.Join(fdir, ".interval"), []byte("1|original\n"), 0o644); err != nil {
			t.Fatal(err)
		}
		p, err := a.planVideo(video, a.describeDir())
		if err != nil {
			t.Fatalf("%s: %v", c.what, err)
		}
		if len(p.frames) != len(c.names) {
			t.Fatalf("%s: planned %d frames, want %d: %v", c.what, len(p.frames), len(c.names), p.frames)
		}
		for i, n := range c.names {
			if filepath.Base(p.frames[i]) != n {
				t.Errorf("%s: frame %d is %s, want %s", c.what, i, filepath.Base(p.frames[i]), n)
			}
		}
		if p.interval != 1 {
			t.Errorf("%s: interval %g, want 1", c.what, p.interval)
		}
	}
}
