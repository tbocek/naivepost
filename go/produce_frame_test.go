package main

import (
	"encoding/json"
	"fmt"
	"image"
	_ "image/png"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// The join is a stream copy, so every clip has to come out at exactly the same
// frame -- and the one clip nothing was pinning was the plain footage. With
// "keep source" set, a second recording at another size went into the concat at
// its own, which is not a video: the demuxer either refuses it or the picture
// falls apart at the join.
//
// The inserts have always been fitted for this reason (the comment in encodeClip
// says so). A second camera is the same problem arriving from the other side.
func TestFootageFromASecondCameraComesOutTheSameSizeAsTheFirst(t *testing.T) {
	a := insertApp(t)
	dir := t.TempDir()

	wide := filepath.Join(dir, "wide.mp4") // the screen capture: 16:9
	mustFFmpeg(t, "-f", "lavfi", "-t", "4", "-i", "testsrc=size=1280x720:rate=30",
		"-f", "lavfi", "-t", "4", "-i", "sine=frequency=300:sample_rate=48000",
		"-c:v", "libx264", "-preset", "ultrafast", "-pix_fmt", "yuv420p", "-c:a", "aac", wide)
	cam := filepath.Join(dir, "cam.mp4") // the webcam: smaller, and 4:3
	mustFFmpeg(t, "-f", "lavfi", "-t", "4",
		"-i", "color=c=red:size=320x480:rate=30,pad=640:480:0:0:color=blue",
		"-f", "lavfi", "-t", "4", "-i", "sine=frequency=800:sample_rate=48000",
		"-c:v", "libx264", "-preset", "ultrafast", "-pix_fmt", "yuv420p", "-c:a", "aac", cam)

	// "keep source", which is what every project written before today holds
	st := prodSettings{Container: "mp4", Codec: "h264", CRF: 30, Preset: "ultrafast",
		Height: 0, FPS: 24, AudioKbps: 96, Subs: "none"}
	clips := []prodClip{
		{idx: 0, video: &tlVideo{base: "wide", path: wide}, local: 0, length: 1.5},
		{idx: 1, video: &tlVideo{base: "cam", path: cam}, local: 0, length: 1.5},
	}
	boxW, boxH := clipBox(clips, st)
	if boxW != 1280 || boxH != 720 {
		t.Fatalf("the box is %dx%d, want the first footage's own 1280x720", boxW, boxH)
	}
	for i := range clips {
		clips[i].boxW, clips[i].boxH = boxW, boxH
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
		fmt.Fprintf(&list, "file '%s'\n", name)
	}
	// ...and the webcam is FITTED into the frame, not pulled wide to it. Its
	// left half is red and its right half blue, so where the boundary lands
	// says which happened: centred at 960 wide it is at x=640, and a picture
	// dropped in the corner at its own 640 puts it at x=320.
	shot := filepath.Join(dir, "cam.png")
	mustFFmpeg(t, "-i", filepath.Join(dir, "clip1.mp4"), "-frames:v", "1", shot)
	if r, g, b := pixelAt(t, shot, 500, 360); b > r || g > 100 {
		t.Errorf("at x=500 the webcam frame is rgb(%d,%d,%d) — blue there means it was "+
			"stretched to the frame instead of fitted into it", r, g, b)
	}

	// ...and the join itself, which is the thing that actually broke
	lst := filepath.Join(dir, "list.txt")
	if err := os.WriteFile(lst, []byte(list.String()), 0o644); err != nil {
		t.Fatal(err)
	}
	out := filepath.Join(dir, "joined.mp4")
	mustFFmpeg(t, "-f", "concat", "-safe", "0", "-i", lst, "-c", "copy", out)
	if d, err := ffprobeDur(out); err != nil || d < 2.5 {
		t.Errorf("the two clips joined to %.2fs (%v), want about 3", d, err)
	}
}

// A 4K screen capture rendered at "source resolution" is a 4K upload: five
// minutes of it came out at 667 MB. The dropdown has always started on 1080,
// and the settings the page loads over it said 0 -- keep source -- so what the
// control showed and what the render did were never the same thing.
func TestAFreshProjectRendersAt1080AndTheDropdownAgrees(t *testing.T) {
	if h := defaultProdSettings().Height; h != 1080 {
		t.Errorf("a new project renders at height %d, want 1080", h)
	}
	// the dropdown's own starting pick is the same number, from the same list
	src := readSrc(t, "produce.go")
	if !strings.Contains(src, `p.height = dd(prodHeights, 1,`) {
		t.Error("the height dropdown no longer starts where it did")
	}
	if prodHeights[1] != "1080p" {
		t.Errorf("the dropdown starts on %q while a new project renders at 1080",
			prodHeights[1])
	}
	// ...and a project that already says "keep source" still gets it
	var st prodSettings
	if err := json.Unmarshal([]byte(`{"height":0}`), &st); err != nil {
		t.Fatal(err)
	}
	if st.Height != 0 {
		t.Errorf("a project asking for source resolution was quietly moved to %d", st.Height)
	}
}

// ...and the page says the frame in pixels, which is the only form of "this is
// a 4K render" anyone reads before waiting out the encode.
func TestTheProducePageSaysTheFrameItWillComeOutAt(t *testing.T) {
	// keep source: the footage's own frame, whatever that is
	if w, h := outSize(3840, 2160, 0); w != 3840 || h != 2160 {
		t.Errorf("source resolution over 4K footage is %dx%d, want 3840x2160", w, h)
	}
	// a tier asked for: the footage's shape with its SHORT side on the tier
	if w, h := outSize(3840, 2160, 1080); w != 1920 || h != 1080 {
		t.Errorf("1080p over 4K footage is %dx%d, want 1920x1080", w, h)
	}
	if w, h := outSize(1440, 1080, 720); w != 960 || h != 720 {
		t.Errorf("720p over 4:3 footage is %dx%d, want 960x720", w, h)
	}
	// ...which for tall footage is the width: a phone clip at 1080p is a real
	// 1080×1920, not shrunk to 608 wide because the tier was read as a height
	if w, h := outSize(1080, 1920, 1080); w != 1080 || h != 1920 {
		t.Errorf("1080p over tall footage is %dx%d, want 1080x1920", w, h)
	}
	if w, h := outSize(2160, 3840, 720); w != 720 || h != 1280 {
		t.Errorf("720p over tall 4K footage is %dx%d, want 720x1280", w, h)
	}
	if w, _ := outSize(1080, 1080, 719); w%2 != 0 {
		t.Errorf("an odd height gives an odd width (%d) — scale=-2 will not", w)
	}
	// nothing probed: 16:9 at the height, and 1080 when there is no height either
	if w, h := outSize(0, 0, 0); w != 1920 || h != 1080 {
		t.Errorf("with nothing known at all the frame is %dx%d, want 1920x1080", w, h)
	}
	// the render works this out off the file it is about to encode -- the one
	// place the frame is decided, now that the page's summary line is gone
	body := funcBody(t, "produce.go", `func clipBox\(`)
	if !strings.Contains(body, "outSize(w0, h0, st.Height)") {
		t.Error("the render no longer sizes the frame through outSize")
	}
}

// pixelAt is one pixel of an image file, 0-255 per channel. The only witness
// that can settle where a fitted picture actually landed in its frame.
func pixelAt(t *testing.T, file string, x, y int) (int, int, int) {
	t.Helper()
	f, err := os.Open(file)
	if err != nil {
		t.Fatal(err)
	}
	defer f.Close()
	im, _, err := image.Decode(f)
	if err != nil {
		t.Fatal(err)
	}
	r, g, b, _ := im.At(x, y).RGBA()
	return int(r >> 8), int(g >> 8), int(b >> 8)
}

// The frame fitting from the item above put every plain footage clip on a
// blurred blow-up of itself. For a 4:3 webcam among 16:9 gameplay that is the
// point -- but for footage already of the finished frame's shape, which is
// every ordinary session, the backdrop is covered to the last pixel by the
// picture laid over it, and a split, a cover-scale and a gaussian blur per
// frame render something nothing can ever see.
func TestFootageOfTheFramesOwnShapeIsScaled_NotLaidOnABlurredCopy(t *testing.T) {
	for _, c := range []struct {
		what string
		v    *tlVideo
		w, h int
		want bool
	}{
		{"a 4K capture into 1080p", &tlVideo{w: 3840, h: 2160}, 1920, 1080, true},
		{"footage already the frame", &tlVideo{w: 1920, h: 1080}, 1920, 1080, true},
		{"1080p blown up to 4K", &tlVideo{w: 1920, h: 1080}, 3840, 2160, true},
		// not 16:9 to the last decimal, and a one-pixel seam of blur is not
		// worth a filter chain
		{"a 1366x768 laptop capture", &tlVideo{w: 1366, h: 768}, 1920, 1080, true},
		{"a 4:3 webcam", &tlVideo{w: 640, h: 480}, 1920, 1080, false},
		// wider than the frame instead of narrower: the width lands and the
		// height is what falls short, so one axis is not enough to ask
		{"a 21:9 ultrawide capture", &tlVideo{w: 2560, h: 1080}, 1920, 1080, false},
		{"a portrait phone clip", &tlVideo{w: 1080, h: 1920}, 1920, 1080, false},
		{"a recording nobody probed", &tlVideo{}, 1920, 1080, false},
		{"no recording at all", nil, 1920, 1080, false},
		{"no frame worked out", &tlVideo{w: 1920, h: 1080}, 0, 0, false},
	} {
		if got := fitsFrame(c.v, c.w, c.h); got != c.want {
			t.Errorf("%s: covers the frame = %v, want %v", c.what, got, c.want)
		}
	}
	// and the encode acts on the answer: a scale on one path, the backdrop on
	// the other, and the same finished size out of both
	body := funcBody(t, "produce.go", `func \(a \*App\) encodeClip\(`)
	if !strings.Contains(body, "if fitsFrame(c.video, c.boxW, c.boxH) {") {
		t.Error("the encode no longer asks whether the footage covers the frame")
	}
	if !strings.Contains(body, `vf = append(vf, fmt.Sprintf("scale=%d:%d", c.boxW, c.boxH))`) {
		t.Error("footage of the frame's own shape is no longer simply scaled to it")
	}
}

// The join is `ffmpeg -f concat -c copy`: it does not scale, and it does not
// refuse. A clip of another size goes into the finished video and the decoder
// comes apart on it -- blocks and smears from that clip on, with nothing said
// anywhere. Every branch of encodeClip pins the size, so this is a guard on
// those branches rather than on anything a user does.
func TestAClipOfTheWrongSizeIsCalledOutBeforeTheJoinSwallowsIt(t *testing.T) {
	if got := joinMismatch(nil); got != nil {
		t.Errorf("a render with no clips complained anyway: %v", got)
	}
	same := []clipSize{{"c000.mp4", 1920, 1080}, {"c001.mp4", 1920, 1080}, {"c002.mp4", 1920, 1080}}
	if got := joinMismatch(same); got != nil {
		t.Errorf("clips that all agree were reported: %v", got)
	}
	// the odd one out, and the one it is odd against: named, so the file in
	// the clips folder can be looked at
	mixed := []clipSize{{"c000.mp4", 1920, 1080}, {"c001.mp4", 640, 480}, {"c002.mp4", 1920, 1080}}
	got := joinMismatch(mixed)
	if len(got) != 1 {
		t.Fatalf("one clip is the wrong size and %d lines were said: %v", len(got), got)
	}
	// each name next to its OWN size: a message that swaps them sends whoever
	// reads it to the wrong file
	for _, want := range []string{"c001.mp4 came out 640×480", "c000.mp4 is 1920×1080"} {
		if !strings.Contains(got[0], want) {
			t.Errorf("the warning does not say %q: %q", want, got[0])
		}
	}
	// and either dimension on its own is enough to break the join: a clip of
	// the right width and the wrong height, or the other way about
	if got := joinMismatch([]clipSize{{"a", 1920, 1080}, {"b", 1920, 1440}}); len(got) != 1 {
		t.Errorf("a clip of the right width and the wrong height passed: %v", got)
	}
	if got := joinMismatch([]clipSize{{"a", 1920, 1080}, {"b", 1440, 1080}}); len(got) != 1 {
		t.Errorf("a clip of the right height and the wrong width passed: %v", got)
	}
	// measured against the first clip, because that is the size the decoder is
	// set up on: two odd ones are two lines, not one
	if got := joinMismatch([]clipSize{{"a", 1920, 1080}, {"b", 640, 480}, {"c", 1280, 720}}); len(got) != 2 {
		t.Errorf("two wrong-sized clips gave %d lines: %v", len(got), got)
	}
	// ...and it is the first that sets the standard, not the majority: two
	// clips agreeing with each other do not make the first one the odd one
	if got := joinMismatch([]clipSize{{"a", 640, 480}, {"b", 1920, 1080}, {"c", 1920, 1080}}); len(got) != 2 {
		t.Errorf("the majority overruled the first clip: %v", got)
	}
	// and the render measures every clip it writes
	src := readSrc(t, "produce.go")
	for _, want := range []string{
		"made = append(made, clipSize{name: name, w: w, h: h})",
		"for _, line := range joinMismatch(made) {",
	} {
		if !strings.Contains(src, want) {
			t.Errorf("the render no longer does %q", want)
		}
	}
}

// The resolution dropdown offers three sizes worth choosing between -- 720p,
// 1080p, original -- and the p-number names the short side of the frame, so a
// 9:16 cut at 1080p renders 1080×1920 (the frame outBox makes) rather than a
// 608-wide strip. The words on the control and the number in the settings
// round-trip through tierLabel and the "p"-trim, and a height an old project
// saved that is no longer offered falls back to the default pick instead of
// matching nothing forever.
func TestTheResolutionTiersNameTheShortSide(t *testing.T) {
	want := []string{"720p", "1080p", "original"}
	if len(prodHeights) != len(want) {
		t.Fatalf("the dropdown offers %v, want %v", prodHeights, want)
	}
	for i, s := range want {
		if prodHeights[i] != s {
			t.Errorf("dropdown entry %d is %q, want %q", i, prodHeights[i], s)
		}
	}
	// the stored number and the word on the control are the same fact
	for _, c := range []struct {
		label string
		h     int
	}{{"720p", 720}, {"1080p", 1080}, {"original", 0}} {
		if got := tierLabel(c.h); got != c.label {
			t.Errorf("tierLabel(%d) = %q, want %q", c.h, got, c.label)
		}
		if got := atoiOr(strings.TrimSuffix(c.label, "p"), 0); got != c.h {
			t.Errorf("%q reads back as height %d, want %d", c.label, got, c.h)
		}
	}
	// a saved 2160 has no row to land on; setPick leaves the default standing,
	// which tierLabel makes explicit by naming a word not in the list
	if got := tierLabel(2160); got != "2160p" {
		t.Errorf("tierLabel(2160) = %q, want the honest %q", got, "2160p")
	}
	// the tall-cut frame itself, as the render builds it
	if w, h := tierBox(9.0/16, 0, 1080); w != 1080 || h != 1920 {
		t.Errorf("a 9:16 cut at 1080p renders %dx%d, want 1080x1920", w, h)
	}
	// original + aspect: the footage's height stays the frame's height -- the
	// crop a 9:16 aspect takes from 2160-tall footage is 1216×2160 of real
	// pixels, and "original" means keeping them
	if w, h := tierBox(9.0/16, 2160, 0); w != 1216 || h != 2160 {
		t.Errorf("a 9:16 cut at original over 4K renders %dx%d, want 1216x2160", w, h)
	}

	// ...and the round trip above is only real if the page actually takes it:
	// the settings read the number through the p-trim, and the restore writes
	// the word through tierLabel -- not the old bare-number fmtOpt, which
	// matches no row and silently re-picks the default on every load
	read := funcBody(t, "produce.go", `func \(a \*App\) prodSettings\(\)`)
	if !strings.Contains(read, `atoiOr(strings.TrimSuffix(pickText(p.height, prodHeights), "p"), 0)`) {
		t.Error(`the settings no longer trim the "p", so every tier reads back as 0 -- original`)
	}
	wrote := funcBody(t, "produce.go", `func \(a \*App\) applyProdSettings\(`)
	if !strings.Contains(wrote, "setPick(p.height, prodHeights, tierLabel(st.Height))") {
		t.Error("the restore no longer speaks the dropdown's language, so a saved height matches no row")
	}

	// when the cut names an aspect the render must reshape the frame the same
	// way (tierBox), not keep the footage's shape as if none had been picked
	if !strings.Contains(readSrc(t, "produce_fx.go"), "return tierBox(a, h0, st.Height)") {
		t.Error("the render ignores the cut's aspect exactly when the shape was changed on purpose")
	}
}
