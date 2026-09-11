package main

// The playhead's own layer (cut_playline.go): the red line moved off the bands
// onto a transparent overlay so the 100ms playback tick repaints a near-empty
// widget instead of every thumbnail and waveform on the page.

import (
	"runtime"
	"strings"
	"testing"

	"github.com/diamondburned/gotk4/pkg/cairo"
)

// lineInk paints the layer headless and answers with per-pixel alpha.
func lineInk(t *testing.T, ed *cutEditor, w, h int) func(x, y int) uint8 {
	t.Helper()
	surf := cairo.CreateImageSurface(cairo.FormatARGB32, w, h)
	cr := cairo.Create(surf)
	ed.drawPlayline(cr, h)
	surf.Flush()
	data, stride := surf.Data(), surf.Stride()
	pix := make([]byte, len(data))
	copy(pix, data) // off the C heap before the surface is collected
	runtime.KeepAlive(surf)
	return func(x, y int) uint8 { return pix[y*stride+x*4+3] }
}

func TestTheLineLayerDrawsTheLineAndNothingElse(t *testing.T) {
	// one filmed run, 10px a second, scrolled 40px along: the playhead at 12s
	// stands at 120 on the timeline and 80 on the glass
	ed := &cutEditor{
		hasPlay:  true,
		playhead: 12,
		viewX:    40,
		pps:      10,
		spans:    []tlSpan{{t0: 0, t1: 60, px: 0}},
	}
	at := lineInk(t, ed, 200, 50)
	for _, y := range []int{0, 25, 49} {
		if at(80, y) == 0 {
			t.Errorf("no ink at (80,%d) — the line is not where the playhead is", y)
		}
	}
	// top to bottom: the layer spans both bands and the gap between them,
	// which the per-band lines never crossed
	for _, x := range []int{60, 100, 160} {
		if at(x, 25) != 0 {
			t.Errorf("ink at (%d,25) — the layer draws more than the line", x)
		}
	}

	// before the first click there is no playhead, and the layer says nothing
	ed.hasPlay = false
	at = lineInk(t, ed, 200, 50)
	for x := 0; x < 200; x += 5 {
		if at(x, 25) != 0 {
			t.Fatalf("ink at (%d,25) with no playhead set", x)
		}
	}
}

// The point of the layer: the tick stops repainting the bands. The tick and
// the widgets cannot run headless, so the wiring is the fact.
func TestThePlaybackTickRepaintsTheLineNotTheBands(t *testing.T) {
	src := readSrc(t, "cut.go")
	for _, want := range []string{
		// the tick's choice: full bands only when the green bar walked on
		"if ed.bandClipIdx() != ed.lineIdx {",
		"ed.redrawLine()",
		// a full repaint remembers which scene the bar was painted on, and
		// takes the line's layer with it -- scroll and zoom move the line too
		"ed.lineIdx = ed.bandClipIdx()",
		// the layer rides over the two bands and swallows no events
		"tracks.Append(ed.lineOver(band))",
	} {
		if !strings.Contains(src, want) {
			t.Errorf("cut.go no longer contains %q", want)
		}
	}
	for _, want := range []string{
		"ed.lineArea.SetCanTarget(false)", // hovers and drags belong to the bands
		"over.AddOverlay(ed.lineArea)",
		"x := ed.xOf(ed.playhead) - ed.viewX", // the layer does not scroll; the bands do
	} {
		if !strings.Contains(readSrc(t, "cut_draw.go"), want) {
			t.Errorf("cut_playline.go no longer contains %q", want)
		}
	}
	// a full repaint moves the line's layer too: the scrollbar under a paused
	// player repaints the bands at the new offset, and a line left where it
	// was would stand on the wrong second until the next tick. The draws live
	// in queueTracks -- the half a pan and a zoom use on their own, without the
	// preview sync -- and redrawTracks is that plus the syncs.
	body := funcBody(t, "cut.go", `func \(ed \*cutEditor\) queueTracks\(\) \{`)
	if !strings.Contains(body, "ed.lineArea.QueueDraw()") {
		t.Error("queueTracks no longer repaints the line's layer")
	}
	if body := funcBody(t, "cut.go", `func \(ed \*cutEditor\) redrawTracks\(\) \{`); !strings.Contains(body, "ed.queueTracks()") {
		t.Error("redrawTracks no longer queues the tracks, so an edit repaints nothing")
	}
	// and the framing overlay still follows the clock on the cheap path
	body = funcBody(t, "cut_draw.go", `func \(ed \*cutEditor\) redrawLine\(\) \{`)
	for _, want := range []string{"ed.fxArea.QueueDraw()", "ed.syncFxCursor()", "ed.syncPreviewZoom()"} {
		if !strings.Contains(body, want) {
			t.Errorf("redrawLine no longer settles %q with the line", want)
		}
	}

	// the bands themselves are out of the red-line business: the only red
	// vertical left in their draw funcs is the layer's
	for _, f := range []struct{ file, head string }{
		{"cut.go", `func \(ed \*cutEditor\) drawTrack\(cr \*cairo\.Context, w, h int\) \{`},
		{"cut_audio.go", `func \(ed \*cutEditor\) drawAudio\(cr \*cairo\.Context, w, h int\) \{`},
	} {
		if strings.Contains(funcBody(t, f.file, f.head), "0.9, 0.15, 0.15") {
			t.Errorf("%s still paints the playhead line — the tick repaints it all again", f.file)
		}
	}
}

// ▶ starts where the red line is, and nothing else gets a say.
//
// It used to have starts of its own, all of them one idea: whatever is in hand
// is what you want to watch. A held clip edge played from the edge, a held
// clip from the clip's own start, and ⏸ then ▶ had to be told apart from both
// -- a remembered second, and a test that the line had not moved since -- to
// carry on where it stopped rather than jump.
//
// Clicking the green is how a scene is taken in hand, which is the commonest
// press on this page, so the exception fired on nearly every ▶: back to the
// scene's first frame, the seconds you had just watched again. ▶✂ beside it
// refused the same holds on the grounds that holding a clip is how you edit
// it, not how you choose where the video starts. That is the rule now, and it
// is one rule for both buttons.
func TestPlayStartsWhereTheLineIs(t *testing.T) {
	body := funcBody(t, "cut.go", `func \(ed \*cutEditor\) toggle\(\) \{`)
	for _, gone := range []string{
		"case ed.edgeOn:", // ...played from the held edge
		"case s != nil:",  // ...and from the held clip's start
		"ed.setPlayhead(", // neither, nor anything else: ▶ does not move the line
		"resumingHere",    // so there is nothing for a resume to be an exception to
	} {
		if strings.Contains(body, gone) {
			t.Errorf("▶ chooses where to start again: toggle still has %q", gone)
		}
	}
	// and the machinery that told a resume from a start is gone with it,
	// rather than left standing as a state nothing reads
	src := readSrc(t, "cut.go")
	for _, gone := range []string{"resumeOn", "resumeT", "markPause"} {
		if strings.Contains(src, gone) {
			t.Errorf("the cut page still carries %q, which nothing can act on now", gone)
		}
	}
	// the one move that is left is not a hold: under ✂ Cut only a line in a
	// dropped stretch is a line where the finished video has nothing
	if !strings.Contains(body, "if !ed.playing() && ed.cutOnly {\n\t\ted.cutOnlySnap()\n\t}") {
		t.Errorf("▶✂ no longer moves a line standing in a gap:\n%s", body)
	}
}
