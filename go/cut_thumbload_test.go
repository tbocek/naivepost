package main

// Nothing is decoded inside a draw.
//
// The frames on the video rows are files -- at the default frame scale, one
// full-resolution JPEG per extracted frame -- and they used to be read where
// they were painted: a cache miss called gdk_pixbuf_new_from_file_at_scale from
// inside drawTrack, on the GTK thread. Still, that costs nothing worth
// measuring; a file is read once and the cache is never emptied.
//
// Zooming is what made it hurt. The step between drawn frames is a thumbnail's
// width (thumbStep), so a zoom changes WHICH files are needed: at 4 px/s with
// frames every 5 s every fifth frame is drawn, at 20 px/s every one of them. A
// single notch of the wheel therefore replaces four out of five thumbnails on
// screen with files nobody has read, and the draw after it stopped for a
// screenful of decodes -- with the next notch queued behind it.

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/diamondburned/gotk4/pkg/cairo"
	"github.com/diamondburned/gotk4/pkg/gdkpixbuf/v2"
)

// A draw asks for what it has not got and paints nothing there; it does not
// read a single byte, however real the file is.
func TestADrawAsksForAFrameAndNeverReadsOne(t *testing.T) {
	dir := t.TempDir()
	png := filepath.Join(dir, "frame.png")
	surf := cairo.CreateImageSurface(cairo.FormatARGB32, 32, 18)
	if err := surf.WriteToPNG(png); err != nil {
		t.Skipf("cairo cannot write a png here: %v", err)
	}
	if _, err := os.Stat(png); err != nil {
		t.Fatal(err)
	}

	ed := newTestEd(t)
	if got := ed.thumb(png); got != nil {
		t.Errorf("a frame nobody has read yet came back as %+v", got)
	}
	if len(ed.thumbs) != 0 {
		t.Errorf("the draw filled the cache itself: %v", ed.thumbs)
	}
	if !ed.thumbWant[png] {
		t.Error("the frame was not asked for, so nothing will ever read it")
	}
	if !ed.thumbBusy {
		t.Error("no loader was armed")
	}
	// asking again does not arm a second one, nor re-add the path
	ed.thumbBusy = false
	if ed.thumb(png); ed.thumbBusy {
		t.Error("a frame already on the list armed another loader")
	}

	// what the loader hands back is a surface, converted once: every paint
	// after this is a copy rather than a pixbuf-to-surface conversion
	ed.thumbBusy = true
	ed.runThumbs()
	if len(ed.thumbWant) != 0 {
		t.Error("the loader left the list it took")
	}
}

// A batch that comes back for a row height nobody is drawing any more is
// dropped: ＋/－ on the thumbnails empties the cache and asks again at the new
// size, and a picture decoded for the old one would be drawn at the wrong scale.
func TestAPictureForTheOldRowHeightIsDropped(t *testing.T) {
	ed := newTestEd(t)
	ed.thumbs = map[string]*thumbPic{}
	gen := ed.thumbGen
	ed.setThumbH(96)
	if ed.thumbGen == gen {
		t.Fatal("changing the row height is not a new generation of pictures")
	}
	ed.tookThumbs(map[string]*gdkpixbuf.Pixbuf{"/f/a.png": nil}, gen)
	if len(ed.thumbs) != 0 {
		t.Errorf("a stale batch landed: %v", ed.thumbs)
	}
	// an unreadable file is remembered as one, so it is not read again on
	// every frame for the rest of the session
	if p := thumbSurface(nil); p == nil || p.surf != nil {
		t.Errorf("a file that could not be read came back as %+v", p)
	}
}

// The wiring: the draw paints surfaces, the reading happens off it, and the
// zoom's own coalescing is still in place.
func TestTheThumbnailsAreLoadedOffTheDraw(t *testing.T) {
	src := readSrc(t, "cut.go")
	if strings.Contains(src, "NewPixbufFromFileAtScale") {
		t.Error("the cut page decodes a frame again where it draws it")
	}
	for _, want := range []string{
		"pic := ed.thumb(v.frames[i])",
		"cr.SetSourceSurface(pic.surf, x, lt+2)",
	} {
		if !strings.Contains(src, want) {
			t.Errorf("cut.go no longer contains %q", want)
		}
	}
	if strings.Contains(src, "gdk.CairoSetSourcePixbuf") {
		t.Error("the draw still converts a pixbuf into a surface on every paint")
	}
	load := readSrc(t, "cut_draw.go")
	for _, want := range []string{
		"go func() {", // off the GTK thread
		"pb, err := gdkpixbuf.NewPixbufFromFileAtScale(p, -1, h, true)",
		"glib.IdleAdd(func() { ed.tookThumbs(batch, gen) })", // ...and back onto it
		"ed.queueTracks()",                       // one draw per batch
		"gdk.CairoSetSourcePixbuf(cr, pb, 0, 0)", // converted once, on arrival
	} {
		if !strings.Contains(load, want) {
			t.Errorf("the loader no longer contains %q", want)
		}
	}
	// and the wheel still folds a burst of notches into one zoom
	body := funcBody(t, "cut.go", `func \(ed \*cutEditor\) zoomWheel\(`)
	if !strings.Contains(body, "ed.zoomPend += dy") || !strings.Contains(body, "if ed.zoomBook {") {
		t.Errorf("the wheel no longer coalesces:\n%s", body)
	}
}
