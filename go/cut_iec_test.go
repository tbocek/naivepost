package main

// How loud is drawn. The envelope on disk is linear amplitude and the lane is a
// meter, and iecScale is the whole of the difference. What has to be pinned
// here is that the curve is the standard's curve rather than something that
// merely looks steep -- the knots of IEC 60268-18, to the digit -- that it is
// stretched onto the range the envelope can actually hold rather than left with
// a dead band at the bottom, that it never doubles back, and that the drawing
// really goes through it. The last one is checked in pixels: "quiet sound is
// visible now" is a claim about the picture, and a claim about the picture is
// witnessed by looking at it.

import (
	"math"
	"os"
	"strings"
	"testing"
)

// ---- the standard's curve ---------------------------------------------------

// The knots are the whole specification. IEC 60268-18 is a piece of paper that
// says where the needle stands at eight loudnesses and straight lines between
// them, so if these eight are right the curve is right, and if one of them is
// off by a hundredth this is not a broadcast meter scale, it is a shape.
func TestTheMeterCurveHasTheStandardsKnots(t *testing.T) {
	for _, c := range []struct {
		db, want float64
	}{
		{-80, 0}, // below the bottom there is nothing to show
		{-70, 0}, // the bottom itself
		{-60, 0.025},
		{-50, 0.075},
		{-40, 0.15},
		{-30, 0.3},
		{-20, 0.5}, // half the scale for a tenth of the amplitude
		{0, 1},     // full scale
	} {
		got := iecRaw(math.Pow(10, c.db/20))
		if math.Abs(got-c.want) > 1e-9 {
			t.Errorf("%g dBFS: meter reads %.4f, the standard says %.4f", c.db, got, c.want)
		}
	}
}

// Above full scale there is no more lane. A sample cannot exceed full scale, so
// this is not a case the envelope produces -- but a curve that kept climbing
// would draw outside its lane and into the one below the first time anything
// else fed it, and the cheap guard is at the curve.
func TestTheMeterStopsAtTheTopAndTheBottom(t *testing.T) {
	if got := iecRaw(2); got != 1 {
		t.Errorf("twice full scale reads %.4f, want it pinned at 1", got)
	}
	if got := iecRaw(0); got != 0 {
		t.Errorf("silence reads %.4f, want 0", got)
	}
	if got := iecScale(4); got != 1 {
		t.Errorf("four times full scale draws %.4f of the lane, want 1", got)
	}
	if got := iecScale(0); got != 0 {
		t.Errorf("silence draws %.4f of the lane, want 0", got)
	}
}

// ---- the curve as this page uses it -----------------------------------------

// The standard's bottom is -70 dBFS and the envelope's bottom is one byte in
// 255, which is -48. Used raw, the 22 dB between them would be a band of lane
// no signal could land in: the quietest sound there is would jump straight to a
// twelfth of the lane, and a noise floor sitting on the byte's own floor would
// draw as a picket fence -- nothing, a notch, nothing -- instead of a low hum.
// So the curve is stretched onto the range the envelope can hold: the faintest
// bucket sits on the floor and full scale reaches the top.
func TestTheCurveIsStretchedOntoWhatTheEnvelopeCanHold(t *testing.T) {
	if got := iecScale(1.0 / 255); math.Abs(got) > 1e-12 {
		t.Errorf("the faintest bucket draws %.4f of the lane, want it on the floor", got)
	}
	if got := iecScale(1); math.Abs(got-1) > 1e-12 {
		t.Errorf("full scale draws %.4f of the lane, want all of it", got)
	}
	// and the thing the stretch is for: one step above the floor is one small
	// step up the lane, not a leap.
	if got := iecScale(2.0 / 255); got > 0.1 {
		t.Errorf("two bytes draws %.4f of the lane -- the dead band is still there", got)
	}
}

// A meter that doubled back would be worse than a linear one: two different
// loudnesses drawing the same height is a picture that cannot be read at all.
func TestLouderIsNeverShorter(t *testing.T) {
	prev := -1.0
	for b := 0; b <= 255; b++ {
		got := iecScale(float64(b) / 255)
		if got < prev {
			t.Fatalf("byte %d draws %.4f, less than the byte below it (%.4f)", b, got, prev)
		}
		if got < 0 || got > 1 {
			t.Fatalf("byte %d draws %.4f of the lane, which is not on the lane", b, got)
		}
		prev = got
	}
}

// The point of the whole exercise, in numbers. These are the loudnesses a
// recorded voice actually lives at, and next to each is the fraction of the
// lane a linear envelope gave it -- which is why the old picture was a row of
// spikes over a flat line.
func TestTheQuietPartOfTheRecordingIsWhereTheLaneGrew(t *testing.T) {
	for _, c := range []struct {
		db, linear, want float64
		what             string
	}{
		{-40, 0.010, 0.067, "room tone"},
		{-30, 0.032, 0.232, "a distant voice"},
		{-20, 0.100, 0.451, "conversational speech"},
		{-6, 0.501, 0.835, "a peak"},
	} {
		amp := math.Pow(10, c.db/20)
		got := iecScale(amp)
		if math.Abs(got-c.want) > 0.002 {
			t.Errorf("%s (%g dBFS) draws %.3f of the lane, want %.3f", c.what, c.db, got, c.want)
		}
		if got <= c.linear {
			t.Errorf("%s (%g dBFS) draws %.3f, no better than the linear %.3f it replaced",
				c.what, c.db, got, c.linear)
		}
	}
}

// ---- and the drawing goes through it ----------------------------------------

// In pixels, because that is what the change was for. A lane of conversational
// speech -- byte 26, about -20 dBFS -- used to be a hairline: 0.102 of the
// lane is three px of blue in a lane thirty px tall. On the meter curve the
// same sound stands nearly half way up, about thirteen. Anything over eight px
// of blue in the column is a height a linear envelope could not have drawn
// there, so eight is the line this test holds.
func TestAQuietLaneIsDrawnAsSomethingYouCanSee(t *testing.T) {
	ed := waveEd(t)
	quiet := make([]uint8, 30000)
	for i := 10000; i < 15000; i++ {
		quiet[i] = 26 // conversational speech, not a shout
	}
	ed.waves = map[string]*waveform{"mic": {hz: 100, chans: [][]uint8{quiet, make([]uint8, 30000)}}}

	h := ed.audioHeight()
	at := renderAudio(t, ed, 900, h)
	blue := 0
	for y := 0; y < h; y++ {
		if isBlue(at(50, y)) { // px 50 is inside the recording's loud stretch
			blue++
		}
	}
	if blue < 8 {
		t.Errorf("a -20 dBFS lane drew %d px of blue; linear would have drawn about 3, "+
			"so the drawing is not going through the meter curve", blue)
	}
}

// The wiring, because the curve is only worth anything where it is applied and
// there is exactly one place that draws a column.
func TestTheLanesAreDrawnOnTheMeterCurve(t *testing.T) {
	src, err := os.ReadFile("cut_audio.go")
	if err != nil {
		t.Fatal(err)
	}
	if want := "p := iecScale(wf.peak(ch, at, at+spp))"; !strings.Contains(string(src), want) {
		t.Errorf("the column height is not taken off the meter curve; want %q", want)
	}
}

// ---- and it stands up from the floor ----------------------------------------

// Not mirrored. A bucket holds the loudest absolute sample in ten milliseconds
// -- a level, with a floor and a top and no negative side -- so a shape drawn
// both ways from a midline was drawing the same number twice and paying half
// the lane's height for the copy. A half-loud signal has to fill the bottom
// half of its lane and leave the top half alone; a mirrored one would put a
// quarter at the top, a quarter at the bottom, and nothing on either edge.
func TestALaneIsFilledFromTheFloorUp(t *testing.T) {
	ed := waveEd(t)
	// a signal that draws about half the lane: the byte whose meter reading is
	// nearest 0.5, found rather than asserted so the test is about the shape
	// and not about the curve.
	half := uint8(1)
	for b := 1; b <= 255; b++ {
		if math.Abs(iecScale(float64(b)/255)-0.5) < math.Abs(iecScale(float64(half)/255)-0.5) {
			half = uint8(b)
		}
	}
	sig := make([]uint8, 30000)
	for i := 10000; i < 15000; i++ {
		sig[i] = half
	}
	ed.waves = map[string]*waveform{"mic": {hz: 100, chans: [][]uint8{sig, make([]uint8, 30000)}}}
	at := renderAudio(t, ed, 900, ed.audioHeight())

	y0 := int(wavePad) // the top of the first lane
	blue := func(dy int) bool { return isBlue(at(50, y0+dy)) }
	if !blue(int(waveLaneH) - 3) {
		t.Error("the bottom of the lane is empty — the fill is not standing on the floor")
	}
	if blue(2) {
		t.Error("a half-loud signal reached the top of its lane")
	}
	if blue(int(waveLaneH/2) - 4) {
		t.Error("there is ink above the half-way mark of a half-loud lane")
	}
	// and the giveaway of a mirror: a gap in the middle with ink on both sides
	// of it. Filled from the floor there is one run of blue and it ends at the
	// bottom edge.
	runs, was := 0, false
	for dy := 0; dy < int(waveLaneH); dy++ {
		if is := blue(dy); is && !was {
			runs++
		} else if !is && was && dy < int(waveLaneH)-1 {
			t.Errorf("the blue stops at %d px into the lane and the lane is %d tall — "+
				"that is a mirrored shape, not a fill", dy, int(waveLaneH))
		}
		was = blue(dy)
	}
	if runs != 1 {
		t.Errorf("the lane holds %d separate bands of blue, want one solid fill", runs)
	}
}
