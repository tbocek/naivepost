package main

import (
	"bytes"
	"encoding/base64"
	"image"
	"image/color"
	"image/png"
	"math"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// The thumbnail comes out the shape the video does. It was 1280x720 whatever
// the cut was, which for a 9:16 short meant a widescreen advertisement for a
// portrait video -- and the widescreen frame handed to the image model on top
// of it, so the model was left to guess the crop and guessed differently every
// time. The long side stays 1280, so a 16:9 cut draws exactly the frame this
// page always drew.
func TestTheThumbnailIsTheShapeTheVideoIs(t *testing.T) {
	for _, c := range []struct {
		aspect string
		w, h   int
	}{
		{"16:9", 1280, 720},
		{"9:16", 720, 1280},
		{"1:1", 1280, 1280},
		{"4:5", 1024, 1280},
		{"source", 1280, 720}, // no aspect chosen: what a recording is
		{"", 1280, 720},
	} {
		w, h := pubBox(c.aspect)
		if w != c.w || h != c.h {
			t.Errorf("a %q cut draws %dx%d, want %dx%d", c.aspect, w, h, c.w, c.h)
		}
		if w%2 != 0 || h%2 != 0 {
			t.Errorf("a %q cut draws an odd frame %dx%d", c.aspect, w, h)
		}
	}
}

// The crop box is not a rectangle anybody draws: it is the biggest one of the
// output's shape that fits in the frame, which is the same rule the camera's
// own default framing uses. Only where it sits is a choice, and it cannot sit
// off the picture.
func TestTheCropBoxIsTheBiggestOneThatFitsAndStaysOnThePicture(t *testing.T) {
	srcA, outA := 16.0/9, 9.0/16
	r := pubCropAt(srcA, outA, 0.5, 0.5)
	if math.Abs(r.hf-1) > 1e-9 {
		t.Errorf("a portrait box in a widescreen frame is %.3f of its height, want all of it", r.hf)
	}
	wf := pubCropW(r.hf, srcA, outA)
	if want := (9.0 / 16) / (16.0 / 9); math.Abs(wf-want) > 1e-9 {
		t.Errorf("the box is %.4f of the width, want %.4f", wf, want)
	}
	// dragged off either end, it stops with its edge on the frame's
	if got := pubCropAt(srcA, outA, -5, 0.5); math.Abs(got.cx-wf/2) > 1e-9 {
		t.Errorf("dragged left the box sits at %.4f, want its edge on the frame at %.4f",
			got.cx, wf/2)
	}
	if got := pubCropAt(srcA, outA, 5, 0.5); math.Abs(got.cx-(1-wf/2)) > 1e-9 {
		t.Errorf("dragged right the box sits at %.4f, want %.4f", got.cx, 1-wf/2)
	}
	// the axis with no slack is not a choice at all, so it is not offered one
	if got := pubCropAt(srcA, outA, 0.5, 0.1); math.Abs(got.cy-0.5) > 1e-9 {
		t.Errorf("a box as tall as the frame was moved vertically to %.4f", got.cy)
	}
	// and a recording already the video's shape has nothing to crop
	if !pubWholeFrame(pubCropAt(srcA, srcA, 0.5, 0.5), srcA, srcA) {
		t.Error("a frame that already matches the cut still reports a crop")
	}
	if pubWholeFrame(r, srcA, outA) {
		t.Error("a widescreen frame cut to a portrait thumbnail claims to keep the whole picture")
	}
	// a project that has never been dragged is centred, and 0,0 is a place the
	// box can legitimately be dragged TO -- which is why nil is the default and
	// not a zero value
	if got := (pubSettings{}).cropRect(srcA, outA); math.Abs(got.cx-0.5) > 1e-9 {
		t.Errorf("an untouched project crops at %.4f, want the middle", got.cx)
	}
	if got := (pubSettings{Crop: &pubPoint{X: 0.9, Y: 0.5}}).cropRect(srcA, outA); got.cx <= 0.6 {
		t.Errorf("a dragged crop was ignored: %.4f", got.cx)
	}
}

// And the box is what actually reaches the image model: the base frame is cut
// down to it before it is sent, so the model is composing inside the shape it
// is drawing rather than being handed three times the width and left to pick.
func TestOnlyWhatTheBoxKeepsIsSentToTheModel(t *testing.T) {
	dir := t.TempDir()
	// a 160x90 frame, red on the left half and blue on the right
	src := image.NewRGBA(image.Rect(0, 0, 160, 90))
	for y := 0; y < 90; y++ {
		for x := 0; x < 160; x++ {
			c := color.RGBA{255, 0, 0, 255}
			if x >= 80 {
				c = color.RGBA{0, 0, 255, 255}
			}
			src.Set(x, y, c)
		}
	}
	path := filepath.Join(dir, "frame.png")
	var buf bytes.Buffer
	if err := png.Encode(&buf, src); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(path, buf.Bytes(), 0o644); err != nil {
		t.Fatal(err)
	}
	if got := imageAspect(path); math.Abs(got-160.0/90) > 1e-9 {
		t.Fatalf("the frame reads as aspect %.4f, want %.4f", got, 160.0/90)
	}

	srcA, outA := 160.0/90, 9.0/16
	decode := func(url string) image.Image {
		const pfx = "data:image/png;base64,"
		if !strings.HasPrefix(url, pfx) {
			t.Fatalf("the cropped base is not a png data url: %.40s", url)
		}
		b, err := base64.StdEncoding.DecodeString(strings.TrimPrefix(url, pfx))
		if err != nil {
			t.Fatal(err)
		}
		im, err := png.Decode(bytes.NewReader(b))
		if err != nil {
			t.Fatal(err)
		}
		return im
	}

	// hard right: the thumbnail is the blue half and none of the red
	u, err := pubCropRefImage(path, pubCropAt(srcA, outA, 1, 0.5), srcA, outA)
	if err != nil {
		t.Fatal(err)
	}
	im := decode(u)
	b := im.Bounds()
	if got := float64(b.Dx()) / float64(b.Dy()); math.Abs(got-outA) > 0.02 {
		t.Errorf("the cropped base is aspect %.3f, want the thumbnail's %.3f", got, outA)
	}
	if b.Dy() != 90 {
		t.Errorf("the crop kept %d rows of a 90-row frame, want all of them", b.Dy())
	}
	for _, pt := range []image.Point{{X: b.Min.X, Y: b.Min.Y}, {X: b.Max.X - 1, Y: b.Max.Y - 1}} {
		r, g2, bl, _ := im.At(pt.X, pt.Y).RGBA()
		if bl < 0x8000 || r > 0x4000 || g2 > 0x4000 {
			t.Errorf("at %v the crop kept a non-blue pixel — it did not move to the right", pt)
		}
	}
	// hard left, and the same box keeps only red
	u, err = pubCropRefImage(path, pubCropAt(srcA, outA, 0, 0.5), srcA, outA)
	if err != nil {
		t.Fatal(err)
	}
	im = decode(u)
	b = im.Bounds()
	r, g2, bl, _ := im.At(b.Max.X-1, b.Min.Y).RGBA()
	if r < 0x8000 || bl > 0x4000 || g2 > 0x4000 {
		t.Error("dragged hard left the crop still kept blue")
	}
	// a frame already the right shape is passed through untouched, not
	// re-encoded: a jpeg round-tripped for nothing comes back worse
	u, err = pubCropRefImage(path, pubCropAt(srcA, srcA, 0.5, 0.5), srcA, srcA)
	if err != nil {
		t.Fatal(err)
	}
	want, err := sdRefImage(path)
	if err != nil {
		t.Fatal(err)
	}
	if u != want {
		t.Error("a frame that needs no crop was re-encoded anyway")
	}
}

// The wiring: the request is the cut's frame, the crop goes on the base alone,
// and the row is rebuilt when the page is entered so the box follows a cut
// that has been reshaped since.
func TestTheCropReachesTheRequestAndOnlyTheBase(t *testing.T) {
	body := funcBody(t, "publish.go", `func \(a \*App\) drawThumbnail\(`)
	for _, want := range []string{
		"w, h := pubBox(aspect)",
		"Width:         w,",
		"Height:        h,",
		"if i == 0 {",
		"u, err = pubCropRefImage(f, r, srcA, outA)",
		"u, err = sdRefImage(f)",
	} {
		if !strings.Contains(body, want) {
			t.Errorf("drawThumbnail no longer contains %q:\n%s", want, body)
		}
	}
	src := readSrc(t, "publish.go")
	if strings.Contains(src, "pubImgW") {
		t.Error("the fixed 1280x720 thumbnail size is back")
	}
	if !strings.Contains(funcBody(t, "publish.go", `func \(p \*publisher\) refresh\(`),
		"p.rebuildFrames()") {
		t.Error("entering the page no longer rebuilds the row, so the crop box keeps a stale shape")
	}
	if !strings.Contains(funcBody(t, "publish.go", `func \(p \*publisher\) reread\(`),
		"p.aspect = p.a.produceCut().Aspect") {
		t.Error("the page no longer reads the cut's aspect")
	}
	// the overlay is the base's alone, and only when there is something to move
	ov := funcBody(t, "publish_crop.go", `func \(s \*pubSlot\) cropOverlay\(`)
	if !strings.Contains(ov, "if s.i != 0 || srcA <= 0 || outA <= 0 || pubWholeFrame(") {
		t.Errorf("the crop box is offered on pictures it cannot help:\n%s", ov)
	}
	if !strings.Contains(ov, "p.crop = &pubPoint{X: r.cx, Y: r.cy}") {
		t.Errorf("dragging the box no longer moves the crop:\n%s", ov)
	}
}
