package main

// Rendering an insert.
//
// The join at the end of Produce is a stream copy: the concat demuxer takes the
// clips as they are and refuses one whose frame size differs from the ones
// before it, and there is no re-encode afterwards to paper over it. So an
// insert -- a card, a still, an animated ranking -- has to come out of ffmpeg at
// exactly the size the footage clips do, whatever shape the file was. That is
// the one thing here that cannot be reasoned about from the arguments: it needs
// ffmpeg to say what it made. The rest is offline.

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
)

// insKind picks the whole input shape -- a stretch of a stream, a held frame, a
// baked sequence -- so a file read as the wrong one does not render badly, it
// fails to render.
func TestInsertKindComesFromTheName(t *testing.T) {
	for _, c := range []struct{ path, want string }{
		{"/a/a few moments later.mp4", "video"},
		{"/a/clip.MKV", "video"}, // the extension is not case
		{"/a/clip.webm", "video"},
		{"/a/ranking.svg", "svg"},
		{"/a/ranking.SVGZ", "svg"},
		{"/a/card.png", "still"},
		{"/a/card.jpg", "still"},
		{"/a/card", "still"}, // no extension at all is a still, not a crash
	} {
		if got := insKind(c.path); got != c.want {
			t.Errorf("insKind(%q) = %q, want %q", c.path, got, c.want)
		}
	}
}

// The stem names a folder of baked frames, so anything a path can hold and a
// folder name cannot has to go.
func TestSafeStemIsSomethingAFolderCanBeCalled(t *testing.T) {
	for _, c := range []struct{ path, want string }{
		{"/a/a few moments later.mp4", "a-few-moments-later"},
		{"/a/ranking.svg", "ranking"},
		{"/a/../weird name (2).png", "weird-name--2"},
		{"/a/ünïcode.svg", "n-code"}, // and no leading dash, which hides a folder
		{"/a/.gitignore", "insert"},  // Go reads a dotfile as all extension
		{"/a/---.png", "insert"},     // nothing usable left: still needs a name
	} {
		if got := safeStem(c.path); got != c.want {
			t.Errorf("safeStem(%q) = %q, want %q", c.path, got, c.want)
		}
	}
	// and never long enough to trip a filesystem
	long := safeStem("/a/" + strings.Repeat("x", 300) + ".svg")
	if len(long) > 40 {
		t.Errorf("a 300-character name became a %d-character folder", len(long))
	}
	// two different files must not collide into one frames folder... which they
	// can, so the clip index is what actually separates them. Pinned here so that
	// stays a deliberate choice: the caller prefixes cNNN_.
	if safeStem("/a/card.png") != safeStem("/b/card.svg") {
		t.Error("this test's premise changed -- update the naming note in clipInput")
	}
}

// A cut of nothing but cards has no footage to take its geometry from, and
// still has to produce clips that concat together.
func TestClipBoxFallsBackToTheOutputShape(t *testing.T) {
	cards := []prodClip{{ins: "/a/one.svg"}, {ins: "/a/two.png"}}
	for _, c := range []struct {
		h     int
		w     int
		wantH int
	}{
		{1080, 1920, 1080},
		{720, 1280, 720},
		{0, 1920, 1080},  // no height chosen either: a video is 1080p unless told
		{721, 1282, 720}, // an odd height (hand-edited settings) lands even
	} {
		w, h := clipBox(cards, prodSettings{Height: c.h})
		if w != c.w || h != c.wantH {
			t.Errorf("height %d: box is %dx%d, want %dx%d", c.h, w, h, c.w, c.wantH)
		}
		// odd is what concat refuses and what yuv420p cannot even represent
		if w%2 != 0 || h%2 != 0 {
			t.Errorf("height %d: box %dx%d has an odd side", c.h, w, h)
		}
	}
}

// testApp is enough App to run ffmpeg through: an output folder and the command
// registry runCmd bookkeeps into.
func insertApp(t *testing.T) *App {
	t.Helper()
	if _, err := exec.LookPath("ffmpeg"); err != nil {
		t.Skip("no ffmpeg on this machine")
	}
	return &App{outDir: t.TempDir(), curCmds: map[*exec.Cmd]bool{}}
}

// The whole reason inserts are fitted rather than scaled, checked against the
// only witness that can settle it. A 4:3 card and a square still go into a 16:9
// video: all three clips must come out the same size, and the concat demuxer
// must then take them with -c copy.
//
// Without the pad, ffmpeg happily encodes a 480x360 card next to a 640x360 clip
// and the failure lands at the join, three minutes later, as "Non-monotonous
// DTS" or a silent stop after the first clip.
func TestEveryInsertComesOutTheSizeTheFootageDoes(t *testing.T) {
	a := insertApp(t)
	dir := t.TempDir()

	// 1280x720 footage with sound, which is what the cut is made of
	footage := filepath.Join(dir, "footage.mp4")
	mustFFmpeg(t, "-f", "lavfi", "-t", "6", "-i", "testsrc=size=1280x720:rate=30",
		"-f", "lavfi", "-t", "6", "-i", "sine=frequency=300:sample_rate=48000",
		"-c:v", "libx264", "-preset", "ultrafast", "-pix_fmt", "yuv420p", "-c:a", "aac", footage)

	// a 4:3 still and an animated SVG that is square: neither is the video's shape
	still := filepath.Join(dir, "a card.png")
	mustFFmpeg(t, "-f", "lavfi", "-i", "color=c=red:size=800x600", "-frames:v", "1", still)
	card := filepath.Join(dir, "ranking.svg")
	if err := os.WriteFile(card, []byte(`<?xml version="1.0"?>
<svg xmlns="http://www.w3.org/2000/svg" width="400" height="400" viewBox="0 0 400 400">
  <rect width="400" height="400" fill="#202030"/>
  <rect y="40" height="60" width="120" x="-120" fill="#cc3344">
    <animate attributeName="x" from="-120" to="40" dur="1s" fill="freeze"/>
  </rect>
</svg>`), 0o644); err != nil {
		t.Fatal(err)
	}

	st := prodSettings{
		Container: "mp4", Codec: "h264", CRF: 30, Preset: "ultrafast",
		Height: 360, FPS: 24, AudioKbps: 96, GameVol: 0.22, Subs: "none",
	}
	clips := []prodClip{
		{idx: 0, video: &tlVideo{base: "footage", path: footage}, local: 0, length: 2},
		{idx: 1, ins: still, length: 1.5},
		{idx: 2, ins: card, length: 2},
	}
	// what produce does after planning: the box comes from the footage, not the
	// settings, because the settings only name a height
	boxW, boxH := clipBox(clips, st)
	if boxW != 640 || boxH != 360 {
		t.Fatalf("box is %dx%d, want 640x360 (1280x720 footage scaled to 360)", boxW, boxH)
	}
	for i := range clips {
		if clips[i].ins != "" {
			clips[i].boxW, clips[i].boxH = boxW, boxH
		}
	}

	var list strings.Builder
	for _, c := range clips {
		name := filepath.Join(dir, fmt.Sprintf("clip%d.mp4", c.idx))
		if err := a.encodeClip(c, name, "", st); err != nil {
			t.Fatalf("%s: %v", c.name(), err)
		}
		w, h, err := ffprobeSize(name)
		if err != nil {
			t.Fatalf("%s produced nothing readable: %v", c.name(), err)
		}
		if w != boxW || h != boxH {
			t.Errorf("%s came out %dx%d, want %dx%d — concat refuses this clip",
				c.name(), w, h, boxW, boxH)
		}
		// a still and a one-second animation both have to fill their whole slot:
		// tpad holds the last frame, and the output -t stops a loop running on
		dur, err := ffprobeDur(name)
		if err != nil {
			t.Fatalf("%s: %v", c.name(), err)
		}
		if dur < c.length-0.2 || dur > c.length+0.2 {
			t.Errorf("%s runs %.2fs in a %.2fs slot", c.name(), dur, c.length)
		}
		list.WriteString("file '" + filepath.Base(name) + "'\n")
	}

	// and now the join that all of the above is for
	lf := filepath.Join(dir, "concat.txt")
	if err := os.WriteFile(lf, []byte(list.String()), 0o644); err != nil {
		t.Fatal(err)
	}
	joined := filepath.Join(dir, "joined.mp4")
	if err := a.runCmd("ffmpeg", "-v", "error", "-y", "-f", "concat", "-safe", "0",
		"-i", lf, "-c", "copy", joined); err != nil {
		t.Fatalf("the clips do not concat: %v", err)
	}
	dur, err := ffprobeDur(joined)
	if err != nil {
		t.Fatal(err)
	}
	if want := 5.5; dur < want-0.5 || dur > want+0.5 {
		t.Errorf("the joined video is %.2fs, want about %.2fs — a clip was dropped at the join", dur, want)
	}
}

// An animated SVG insert has to reach ffmpeg as a sequence, and a static one as
// a single held frame. Same file type, two completely different input shapes,
// and getting it wrong renders a tier list that never moves.
func TestAnAnimatedInsertIsBakedAndAStaticOneIsHeld(t *testing.T) {
	a := &App{outDir: t.TempDir()}
	dir := t.TempDir()
	st := prodSettings{FPS: 10, Height: 360}

	moving := filepath.Join(dir, "moving.svg")
	if err := os.WriteFile(moving, []byte(`<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100">
  <rect x="0" width="10" height="10"><animate attributeName="x" from="0" to="90" dur="1s" fill="freeze"/></rect>
</svg>`), 0o644); err != nil {
		t.Fatal(err)
	}
	still := filepath.Join(dir, "still.svg")
	if err := os.WriteFile(still, []byte(
		`<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100"><rect width="100" height="100"/></svg>`), 0o644); err != nil {
		t.Fatal(err)
	}

	args, sound, err := a.clipInput(prodClip{idx: 7, ins: moving, length: 2}, st)
	if err != nil {
		t.Fatal(err)
	}
	if sound {
		t.Error("a graphic reports having audio, and the mix would then take it as the game track")
	}
	joined := strings.Join(args, " ")
	if !strings.Contains(joined, "-framerate 10") || !strings.Contains(joined, "f%05d.svg") {
		t.Fatalf("an animated SVG is not read as a sequence: %v", args)
	}
	if strings.Contains(joined, "-loop") {
		t.Errorf("a baked sequence is also being looped, which would play it twice: %v", args)
	}
	// baked beside the clips, named by the clip index: it belongs to this render
	// and goes when the render does
	frames := filepath.Join(a.produceDir(), "clips", "c007_moving.frames")
	ents, err := os.ReadDir(frames)
	if err != nil {
		t.Fatalf("no frames were baked: %v", err)
	}
	if len(ents) != 20 { // 2 s at 10 fps
		t.Errorf("%d frames baked for a 2 s slot at 10 fps, want 20", len(ents))
	}

	args, _, err = a.clipInput(prodClip{idx: 8, ins: still, length: 2}, st)
	if err != nil {
		t.Fatal(err)
	}
	joined = strings.Join(args, " ")
	if !strings.Contains(joined, "-loop 1") || !strings.Contains(joined, still) {
		t.Errorf("a static SVG is not held as a still: %v", args)
	}
	if strings.Contains(joined, ".frames") {
		t.Errorf("a static SVG was baked frame by frame anyway: %v", args)
	}
}

// Footage still goes in the way it always did -- the seek is on the input side,
// where it is fast, and the source's own audio comes with it.
func TestFootageInputIsUnchangedByTheInsertPath(t *testing.T) {
	a := &App{outDir: t.TempDir()}
	c := prodClip{video: &tlVideo{path: "/nowhere/rec.mp4"}, local: 12.5, length: 8}
	args, _, err := a.clipInput(c, prodSettings{FPS: 30})
	if err != nil {
		t.Fatal(err)
	}
	if got := strings.Join(args, " "); got != "-ss 12.500 -t 8.000 -i /nowhere/rec.mp4" {
		t.Errorf("footage input is %q", got)
	}
	// a segment dragged before the start of its recording seeks to zero rather
	// than asking ffmpeg for a negative time
	args, _, _ = a.clipInput(prodClip{video: c.video, local: -3, length: 8}, prodSettings{})
	if !strings.Contains(strings.Join(args, " "), "-ss 0.000") {
		t.Errorf("a negative offset went through to ffmpeg: %v", args)
	}
}

func mustFFmpeg(t *testing.T, args ...string) {
	t.Helper()
	full := append([]string{"-v", "error", "-y"}, args...)
	if out, err := exec.Command("ffmpeg", full...).CombinedOutput(); err != nil {
		t.Fatalf("ffmpeg %v: %v\n%s", args, err, out)
	}
}
