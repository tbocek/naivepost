package main

// The words printed on the thumbnail (publish_text.go). What these tests hold
// is the contract the feature IS: the words land inside the box they were
// given and nowhere else, the title lands in its band across the upper part,
// a picture with nothing to print survives byte-identical, and the marked
// texts ride the project and the run like every other setting.
//
// The drawing tests run cairo against real PNG files -- no widgets, no
// display -- because the printing is the product here, and a source pin
// cannot say whether a word actually landed where the box was.

import (
	"bytes"
	"image"
	"image/png"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// whitePNG writes a w×h all-white picture and returns its path.
func whitePNG(t *testing.T, dir string, w, h int) string {
	t.Helper()
	img := image.NewRGBA(image.Rect(0, 0, w, h))
	for i := range img.Pix {
		img.Pix[i] = 0xff
	}
	var buf bytes.Buffer
	if err := png.Encode(&buf, img); err != nil {
		t.Fatal(err)
	}
	p := filepath.Join(dir, "plain.png")
	if err := os.WriteFile(p, buf.Bytes(), 0o644); err != nil {
		t.Fatal(err)
	}
	return p
}

// inked reports whether any pixel in the rectangle (fractions of the picture)
// is visibly not white -- the outline is dark and the fill is white, so any
// printing shows up as darkness.
func inked(img image.Image, x0, y0, x1, y1 float64) bool {
	b := img.Bounds()
	for y := b.Min.Y + int(y0*float64(b.Dy())); y < b.Min.Y+int(y1*float64(b.Dy())); y++ {
		for x := b.Min.X + int(x0*float64(b.Dx())); x < b.Min.X+int(x1*float64(b.Dx())); x++ {
			r, g, bl, _ := img.At(x, y).RGBA()
			if r < 0xe000 || g < 0xe000 || bl < 0xe000 {
				return true
			}
		}
	}
	return false
}

func decode(t *testing.T, path string) image.Image {
	t.Helper()
	f, err := os.Open(path)
	if err != nil {
		t.Fatal(err)
	}
	defer f.Close()
	img, err := png.Decode(f)
	if err != nil {
		t.Fatal(err)
	}
	return img
}

// A marked text is printed inside the box it was given and nowhere else. The
// margin around the box allows the dark edge its couple of pixels; past that,
// ink outside the box is a word that escaped it.
func TestTheWordsLandInsideTheirBox(t *testing.T) {
	dir := t.TempDir()
	plain := whitePNG(t, dir, 320, 180)
	out := filepath.Join(dir, "out.png")
	tx := pubText{Cx: 0.5, Cy: 0.75, Wf: 0.5, Hf: 0.2, Text: "GG WP"}
	if err := drawPubTexts(plain, out, []pubText{tx}, "", pubTitleBox); err != nil {
		t.Fatal(err)
	}
	img := decode(t, out)
	if !inked(img, 0.25, 0.65, 0.75, 0.85) {
		t.Error("nothing was printed inside the box")
	}
	// the whole upper half stays clean: no title was given, and the marked
	// box is in the lower quarter
	if inked(img, 0, 0, 1, 0.5) {
		t.Error("ink in the upper half, far outside the marked box")
	}
	if inked(img, 0, 0.95, 1, 1) {
		t.Error("ink along the bottom edge, below the marked box")
	}
}

// The title goes across the upper part of the picture -- that is the whole
// arrangement with the image model, which is told to keep that part calm --
// and an empty title prints nothing there.
func TestTheTitleIsPrintedAcrossTheUpperPart(t *testing.T) {
	dir := t.TempDir()
	plain := whitePNG(t, dir, 320, 180)
	out := filepath.Join(dir, "out.png")
	if err := drawPubTexts(plain, out, nil, "BIG WIN", pubTitleBox); err != nil {
		t.Fatal(err)
	}
	img := decode(t, out)
	if !inked(img, 0, 0, 1, 0.3) {
		t.Error("the title did not land in the upper part of the picture")
	}
	// the box is the upper HALF, and never more: the lower half of a
	// thumbnail is the picture, and a title printed into it covers the thing
	// the title is about
	if inked(img, 0, 0.55, 1, 1) {
		t.Error("the title reached into the lower half -- its box has moved")
	}
	if bottom := pubTitleBox.cy + pubTitleBox.hf/2; bottom > 0.5+1e-9 {
		t.Errorf("pubTitleBox reaches %.2f down the frame, past the half it is given", bottom)
	}
	if top := pubTitleBox.cy - pubTitleBox.hf/2; top > 1e-9 {
		t.Errorf("pubTitleBox starts %.2f down the frame; the half it is given begins at the top", top)
	}
	// ...and it is a CEILING rather than a fill: four to seven words in a box
	// half the picture tall are set as large as they fit, and the rest of the
	// half is left alone (fitText)
	if !inked(img, 0, 0, 1, 0.3) || inked(img, 0, 0.45, 1, 0.5) {
		t.Error("the title fills the whole half instead of taking what it needs")
	}
}

// With no words there is no printing: the plain bytes are copied through
// untouched, because a decode-and-re-encode of a picture nothing was drawn on
// only makes the two files differ for no reason. A text of pure whitespace is
// no words -- overFx makes the same call for the video's overlays.
func TestNothingToPrintCopiesThePlainBytesThrough(t *testing.T) {
	dir := t.TempDir()
	plain := whitePNG(t, dir, 64, 36)
	out := filepath.Join(dir, "out.png")
	if err := drawPubTexts(plain, out, []pubText{{Cx: 0.5, Cy: 0.5, Wf: 0.5, Hf: 0.2, Text: "  \n "}}, "  ", pubTitleBox); err != nil {
		t.Fatal(err)
	}
	a, _ := os.ReadFile(plain)
	b, err := os.ReadFile(out)
	if err != nil {
		t.Fatal(err)
	}
	if !bytes.Equal(a, b) {
		t.Error("a picture with nothing to print came out different from the plain copy")
	}
}

// A box from a project file -- or a wild drag -- is pulled back onto the
// frame, exactly as a text effect's box is: the same clamp answers for both.
func TestAMarkedBoxIsKeptOnTheFrame(t *testing.T) {
	b := pubText{Cx: -3, Cy: 9, Wf: 4, Hf: -1, Text: "x"}.box()
	if b.wf <= 0 || b.hf <= 0 || b.wf > 1 || b.hf > 1 {
		t.Errorf("box size off the frame: %+v", b)
	}
	if b.cx-b.wf/2 < -1e-9 || b.cx+b.wf/2 > 1+1e-9 || b.cy-b.hf/2 < -1e-9 || b.cy+b.hf/2 > 1+1e-9 {
		t.Errorf("box dragged off the picture survives as %+v", b)
	}
}

// The marked texts are page state like the image row: they ride the snapshot
// into the run, land back through apply, count toward "worth a key in the
// project file", and every change goes through the one gate that re-prints
// (setTexts). Source pins, because the seams are one-line and behavioural
// coverage of the printing itself is above.
func TestTheMarkedTextsRideTheProjectAndTheRun(t *testing.T) {
	src := readSrc(t, "publish.go")
	for _, want := range []string{
		"Texts: append([]pubText(nil), p.texts...)",     // snapshot
		"p.texts = append([]pubText(nil), st.Texts...)", // apply
		"len(st.Texts) == 0",                            // currentPublish's nothing-worth-saving check
		"videoFrame(p.textOverlay(p.shot))",             // the marking layer wraps the result
		"Texts []pubText `json:\"texts,omitempty\"`",    // and the project file carries them
	} {
		if !strings.Contains(src, want) {
			t.Errorf("publish.go does not contain %q — the marked texts no longer ride the project", want)
		}
	}
	// retyping the video's title does NOT re-print the picture: the thumbnail
	// carries its own line, seeded from the title the first time a thumbnail
	// exists and its own from then on (setThumbTitle). Rewording the picture's
	// line re-prints, and never while a run owns the files.
	if strings.Contains(src, "p.letter.call(p.recomposite)") {
		t.Error("retyping the video's title reprints the thumbnail again")
	}
	if !strings.Contains(funcBody(t, "publish_text.go", `func \(p \*publisher\) setThumbTitle\(s string\) \{`),
		"p.recomposite()") {
		t.Error("rewording the picture's line does not re-print it")
	}
	over := readSrc(t, "publish_text.go")
	if !strings.Contains(funcBody(t, "publish_text.go", `func \(p \*publisher\) recomposite\(\) \{`),
		"if p.a.running {") {
		t.Error("recomposite runs during a run, racing the runner for thumbnail.png")
	}
	// every path that changes the texts re-prints through the one gate
	for _, want := range []string{"p.setTexts(append(append([]pubText(nil), p.texts[:i]...)", "p.recomposite()"} {
		if !strings.Contains(over, want) {
			t.Errorf("publish_text.go does not contain %q", want)
		}
	}
}

// What the prompt now asks for: one picture composed out of the frames, and a
// calm upper part because the title is printed there afterwards. The old
// wording asked for an edit of ONE frame with the title lettered in, and a
// prompt that drifts back re-introduces the double title.
func TestThePromptComposesAndLeavesRoomForTheTitle(t *testing.T) {
	for _, want := range []string{
		"compose ONE picture out of the frames",
		// onto the picture, and not lettered by the model -- but no longer
		// "the upper part": the band can be dragged (pubSettings.TitleBox),
		// and the instruction names where it actually is (pubTitleWhere)
		"the title is printed onto the finished picture afterwards",
	} {
		if !strings.Contains(strings.TrimSpace(sysSystem)+"\n\n"+youtubeSystem, want) {
			t.Errorf("the upload text is never told %q", want)
		}
	}
	if strings.Contains(youtubeSystem, "appended to your instruction automatically") {
		t.Error("youtubeSystem still describes the appended-title arrangement, which is gone")
	}
}
