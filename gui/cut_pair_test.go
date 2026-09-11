package main

// The pair: a row of pictures and the sound filmed with it, in one piece.
//
// The footage's own track used to be a lane in the band below, which put the
// one sound that belongs to a row of pictures two bands away from it and left
// the band mostly full of things nobody had to move. Now each row's own sound
// is a wave strip drawn directly under its pictures -- dimmer, plateless,
// sharing an edge with the thumbnails, because it is the row's shadow and not
// a row of its own -- and the band below holds only the separate recorders,
// vanishing entirely in a cameras-only session.
//
// These tests hold the geometry (a row is as deep as its own sound), whose
// sound a press on a strip lands on, the drawn strip itself, and the seams.

import (
	"os"
	"strings"
	"testing"
)

// pairEd is two cameras on their own rows: the first filmed in stereo, the
// second silently, and a separate recorder that must pair with nobody.
func pairEd(t *testing.T) *cutEditor {
	t.Helper()
	ed := axisEd(t,
		tlVideo{base: "a", path: "/f/a.mp4", start: 0, dur: 100},
		tlVideo{base: "b", path: "/f/b.mp4", start: 10, dur: 80})
	ed.auds = []tlAudio{
		{base: "a", path: "/f/a.mp4", start: 0, dur: 100, chans: 2, master: true},
		{base: "mic", path: "/f/mic.wav", start: 0, dur: 100, chans: 1},
	}
	return ed
}

// ---- the geometry -----------------------------------------------------------

func TestARowIsAsDeepAsItsOwnSound(t *testing.T) {
	ed := pairEd(t)
	if got, want := ed.pairH(0), 2*waveLaneH; got != want {
		t.Errorf("the stereo row's strip is %g px deep, want %g", got, want)
	}
	// the silent camera gets no strip at all: a row of pictures, not a row
	// over an empty band pretending it recorded something
	if got := ed.pairH(1); got != 0 {
		t.Errorf("the silent row grew a %g px strip", got)
	}
	// the separate recorder deepens nobody's row -- its lane is in the band
	if au := ed.pairAud("mic"); au != nil {
		t.Errorf("the separate recorder paired with a row: %+v", au)
	}
	// and the rows below move down by exactly the strip: the pair shares an
	// edge with its pictures and the next camera starts after both
	if got, want := ed.laneTop(1), ed.picTop()+ed.laneH()+2*waveLaneH+laneGap; got != want {
		t.Errorf("the second row starts at %g, want %g — the strip's room went missing", got, want)
	}
	if got, want := ed.picBottom(), ed.laneTop(1)+ed.laneH(); got != want {
		t.Errorf("the stack ends at %g, want %g", got, want)
	}
	// the decode is believed over the probe, same as the band: a stereo file
	// that came back one signal gives the row its lane back
	ed.waves = map[string]*waveform{"a": {hz: waveHz, chans: [][]uint8{{1}}}}
	if got := ed.pairH(0); got != waveLaneH {
		t.Errorf("the collapsed stereo row still asks for %g px, want %g", got, waveLaneH)
	}
}

// ---- what a press lands on --------------------------------------------------

func TestTheStripKnowsWhoseSoundItIs(t *testing.T) {
	ed := pairEd(t)
	strip := ed.picTop() + ed.laneH() // row 0's strip starts under its pictures
	for _, c := range []struct {
		y    float64
		want int
		what string
	}{
		{ed.picTop() + 1, -1, "on the pictures"},
		{strip + 1, 0, "the top of the strip"},
		{strip + 2*waveLaneH - 1, 0, "the bottom of the strip"},
		{strip + 2*waveLaneH + 1, -1, "in the gap below"},
		{ed.laneTop(1) + 1, -1, "on the silent row's pictures"},
	} {
		if got := ed.pairAt(c.y); got != c.want {
			t.Errorf("pairAt(%g) (%s) = %d, want %d", c.y, c.what, got, c.want)
		}
	}
	// two sources sharing a row each bring the stretch under their own
	// pictures, so whose sound the press is about is answered by x -- and a
	// press on the stretch of the row neither of them covers is the nearest,
	// audAtY's rule. The third camera is what opens that stretch: it overlaps
	// both, so it takes a row of its own and its fifty seconds are laid out --
	// leaving row 0 with a hole in it that IS drawn, unlike unfilmed time.
	ed = axisEd(t,
		tlVideo{base: "one", path: "/f/one.mp4", start: 0, dur: 50},
		tlVideo{base: "mid", path: "/f/mid.mp4", start: 40, dur: 70},
		tlVideo{base: "two", path: "/f/two.mp4", start: 100, dur: 50})
	ed.auds = []tlAudio{
		{base: "one", path: "/f/one.mp4", start: 0, dur: 50, chans: 1, master: true},
		{base: "two", path: "/f/two.mp4", start: 100, dur: 50, chans: 1, master: true},
	}
	if ed.laneN != 2 {
		t.Fatalf("three recordings, one overlapping both, landed on %d rows, want 2", ed.laneN)
	}
	y := ed.picTop() + ed.laneH() + 1
	for _, c := range []struct {
		px   float64
		want string
	}{
		{40, "one"},
		{ed.xOf(140), "two"},
		{ed.xOf(52), "one"}, // the row's own hole, near one
		{ed.xOf(98), "two"}, // ...and near two
	} {
		if got := ed.pairAudAt(c.px, y); got != c.want {
			t.Errorf("pairAudAt(%g) = %q, want %q", c.px, got, c.want)
		}
	}
}

// ---- the picture ------------------------------------------------------------

// The strip is on the page, under its own pictures, in the dim voice -- and
// the silent row has nothing under it. In pixels, because "the wave is the
// row's shadow" is a claim about ink.
func TestThePairedWaveIsDrawnUnderItsPictures(t *testing.T) {
	ed := pairEd(t)
	ed.auds[0].chans = 1
	loud := make([]uint8, 100*int(waveHz))
	for i := range loud {
		loud[i] = 255
	}
	ed.waves = map[string]*waveform{"a": {hz: waveHz, chans: [][]uint8{loud}}}
	at := renderTrack(t, ed, 400, int(ed.picBottom())+80)

	x := int(ed.xOf(20))
	y := int(ed.picTop()+ed.laneH()) + int(waveLaneH) - 2 // just over the strip's floor
	r, g, b := at(x, y)
	if !isDimBlue(r, g, b) {
		t.Errorf("row 0's strip painted rgb(%d,%d,%d) where its wave should be", r, g, b)
	}
	// dim MEANS dim: the full-strength ink is the band's voice, and a strip
	// that shouts like the band reads as a row of its own between two cameras
	if isBlue(r, g, b) {
		t.Errorf("the paired wave is at full strength: rgb(%d,%d,%d)", r, g, b)
	}
}

// ---- the seams --------------------------------------------------------------

func TestThePairingIsWired(t *testing.T) {
	pins := map[string][]string{
		"cut.go": {
			// drawTrack lowers the strip onto the shared edge under each row
			"ed.drawPairStrip(cr, v, *au, lt+ed.laneH(), vx0, vx1)",
			// a drag on a strip selects THAT recording's sound
			"ed.sel.aud = ed.pairAudAt(x+ed.viewX, y)",
			// a right-drag on a strip moves the one recording under the hand
			"slideSrcs = []string{ed.pairAudAt(x+ed.viewX, y)}",
			// and the badge saying whether the held scene hears that row's
			// own sound is drawn on the strip, where the sound is
			"ed.drawHearBadges(cr, ed.hearBadgesSrc(), vx0, vx1)",
		},
		"cut_audio.go": {
			"func (ed *cutEditor) drawPairStrip(",
			// a master decoding to fewer channels than probed shrinks its ROW
			"ed.fitSrc()", "a master's collapse is a shallower ROW",
			"alpha = 0.55", // the dim voice, in one place
			// the band asks the recorders and only the recorders for room
			"func (ed *cutEditor) sepAuds() []tlAudio {",
		},
	}
	for file, want := range pins {
		src, err := os.ReadFile(file)
		if err != nil {
			t.Fatal(err)
		}
		for _, w := range want {
			if !strings.Contains(string(src), w) {
				t.Errorf("%s no longer contains %q", file, w)
			}
		}
	}
}
