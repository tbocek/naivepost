package main

// The green that says "this is in the video" covers the sound too.
//
// A scene the cut keeps takes the sound filmed with it: that is what the render
// does (the clip's own track comes off the recording it is cut from) and what
// the recorders' band below has always drawn -- every separate lane wearing the
// same green over the seconds the cut keeps. The one band that did not say it
// was the wave strip under the pictures, which is the camera's OWN sound. So a
// session with a mic showed green over the mic's waveform and nothing over the
// capture's, for two recordings that are equally in the video.

import (
	"strings"
	"testing"
)

// keptEd is one camera with its own sound and one kept scene, drawn wide
// enough that the strip under the pictures is several px deep.
func keptEd(t *testing.T) *cutEditor {
	t.Helper()
	ed := newTestEd(t)
	ed.vids, ed.auds = heardTracks()
	ed.relayout()
	ed.segs = []cutSeg{{S: 20, E: 40}}
	return ed
}

func TestTheKeptGreenCoversTheSoundFilmedWithThePictures(t *testing.T) {
	ed := keptEd(t)
	row := ed.segRow(ed.segs[0])
	if ed.pairH(row) <= 0 {
		t.Fatal("the fixture's camera filmed no sound, so there is no strip to tint")
	}
	at := renderTrack(t, ed, 900, 400)
	x := int(ed.xOf(30)) // inside the kept scene
	// the pictures, and the strip under them: both greener than the ground,
	// and the strip fainter -- a waveform IS the reading, so a wash heavy
	// enough to colour it takes it with it (drawAudio says the same)
	_, picG, _ := at(x, int(ed.laneTop(row)+ed.laneH()/2))
	sr, sg, sb := at(x, int(ed.laneTop(row)+ed.laneH()+ed.pairH(row)/2))
	if int(sg) <= int(sr)+3 || int(sg) <= int(sb)+3 {
		t.Errorf("the strip under a kept scene reads rgb(%d,%d,%d) — no green on it", sr, sg, sb)
	}
	if sg >= picG {
		t.Errorf("the strip's wash (g=%d) is as heavy as the thumbnails' (g=%d)", sg, picG)
	}
	// ...and a stretch the cut drops has neither
	x = int(ed.xOf(60))
	dr, dg, db := at(x, int(ed.laneTop(row)+ed.laneH()+ed.pairH(row)/2))
	if int(dg) > int(dr)+3 && int(dg) > int(db)+3 {
		t.Errorf("a dropped stretch's strip reads rgb(%d,%d,%d) — tinted as kept", dr, dg, db)
	}
}

// ...and what the scene does with that sound is drawn ON the green, not under
// it: "kept" and "silenced" are two answers about the same seconds and the
// second is the one that has to be visible.
func TestTheSoundsOwnAnswerIsDrawnOverTheKeptGreen(t *testing.T) {
	src := readSrc(t, "cut.go")
	green := strings.Index(src, "// the state overlay: everything the cut keeps, tinted green")
	washes := strings.Index(src, "ed.drawSilences(cr, ed.pairSilences(), vx0, vx1)")
	if green < 0 || washes < 0 {
		t.Fatal("the picture band no longer draws both")
	}
	if washes < green {
		t.Error("the per-scene sound washes are drawn under the kept green, so a silenced " +
			"scene comes out grey with green over it")
	}
	// the borders run through the strip as well: the cut's edges are the cut's
	// edges on every band that shows those seconds
	if !strings.Contains(src, "cr.LineTo(x, st+lh+ph)") {
		t.Error("the green borders stop at the pictures and do not cross the strip")
	}
}

// Space is ▶/⏸ on the page, and it is the button's own verb rather than a
// second way of doing nearly the same thing.
func TestSpaceIsPlayPause(t *testing.T) {
	src := readSrc(t, "cut.go")
	for _, want := range []string{
		"case keyval == gdk.KEY_space && state&(gdk.ControlMask|gdk.AltMask) == 0:",
		"\t\t\ted.toggle()",
	} {
		if !strings.Contains(src, want) {
			t.Errorf("cut.go no longer contains %q", want)
		}
	}
	// bubble phase, so a text box and a focused button keep the space bar they
	// have always had
	body := funcBody(t, "cut.go", `keys := gtk\.NewEventControllerKey\(\)`)
	if strings.Contains(body, "keys.SetPropagationPhase(gtk.PhaseCapture)") {
		t.Error("the page takes the space bar before the text boxes do")
	}
}
