package main

// Selecting sound, and the verbs that follow.
//
// The selection has always been one object shared by the picture band and the
// lanes -- it is a stretch of session time, and both bands show the same
// clock. What was missing is that it MEANT the same thing wherever it was
// drawn: ⧉ Copy took footage whether the hand was on a thumbnail or on a
// waveform, ⧉ Insert offered tier cards to a selection made in a lane, and the
// lanes showed neither the cut nor the selection, so choosing where a sound
// went meant reading the answer off a different row than the one being worked
// in.
//
// So a selection remembers the band that drew it. Drawn on the pictures it is
// footage and the verbs are the ones they always were; drawn in a lane it is
// THAT recording's sound, ⧉ Copy takes those seconds of it, ⧉ Paste lays them
// over the footage without costing the video a frame, and ⧉ Insert offers
// sound files. And the lanes wear the cut's own green, so what is in the video
// is readable from the row the sound is chosen in.
//
// These tests hold the geography (which lane a point is in), the two verbs,
// what a laid-over sound does to the cut's length, and the colours.

import (
	"math"
	"os"
	"strings"
	"testing"
)

// selEd is a session with three recordings under the footage: a stereo mic
// (two lanes) and two mono tracks. Three rather than two because the ground
// between recordings only becomes load-bearing at the third -- with two, a
// walk that forgot the gap still lands every point on the right lane.
func selEd(t *testing.T) (*App, *cutEditor) {
	t.Helper()
	a := &App{root: t.TempDir()}
	ed := moveEd(t)
	ed.a, a.ed = a, ed
	ed.auds = []tlAudio{
		{base: "mic", path: "mic.wav", start: 0, dur: 300, chans: 2},
		{base: "cam", path: "cam.m4a", start: 0, dur: 300, chans: 1, master: true},
		{base: "room", path: "room.ogg", start: 0, dur: 300, chans: 1},
	}
	ed.segs = []cutSeg{{S: 0, E: 300}}
	return a, ed
}

// TestEveryPointInTheLanesNamesARecording: audAtY walks the layout drawAudio
// draws, so the hand lands on the lane the eye is pointing at -- and it is
// total, because a press in the pad or in the hair of ground between two
// recordings that quietly meant "the pictures" would take footage while the
// hand was on a waveform.
func TestEveryPointInTheLanesNamesARecording(t *testing.T) {
	_, ed := selEd(t)
	// lanes: mic L 3-33, mic R 33-63, gap, room 67-97 -- and no cam anywhere:
	// the footage's own sound is paired under its pictures now, not down here
	for _, c := range []struct {
		y    float64
		want string
	}{
		{0, "mic"}, {10, "mic"}, {40, "mic"}, {62, "mic"},
		{64, "room"}, {70, "room"}, {96, "room"},
		{100, "room"}, {120, "room"}, {400, "room"},
	} {
		if got := ed.audAtY(c.y); got != c.want {
			t.Errorf("audAtY(%.0f) = %q, want %q", c.y, got, c.want)
		}
	}
	if got := ed.audAtY(10); got != "mic" {
		t.Errorf("audAtY over a stereo recording's second lane = %q", got)
	}
	// no lanes at all: nothing to name, and "" is what the pictures mean
	ed.auds = nil
	if got := ed.audAtY(10); got != "" {
		t.Errorf("audAtY with no recordings = %q, want the pictures", got)
	}
}

// TestALaneKnowsWhereItIs: audLaneSpan is audAtY's other half, and the two
// have to agree or the brighter wash lands on the wrong recording.
func TestALaneKnowsWhereItIs(t *testing.T) {
	_, ed := selEd(t)
	for _, base := range []string{"mic", "room"} {
		y0, y1, ok := ed.audLaneSpan(base)
		if !ok {
			t.Fatalf("audLaneSpan(%q) found no lanes", base)
		}
		for _, y := range []float64{y0, (y0 + y1) / 2, y1 - 1} {
			if got := ed.audAtY(y); got != base {
				t.Errorf("audLaneSpan(%q) says %.0f-%.0f, but audAtY(%.0f) = %q",
					base, y0, y1, y, got)
			}
		}
	}
	if y0, y1, ok := ed.audLaneSpan("gone"); ok {
		t.Errorf("audLaneSpan of a recording that is not here = %.0f-%.0f, want no span", y0, y1)
	}
	// the master is in the session but not in the band: its wave lives under
	// its own row of pictures, and a wash aimed at it must land there instead
	if y0, y1, ok := ed.audLaneSpan("cam"); ok {
		t.Errorf("audLaneSpan of the footage's own sound = %.0f-%.0f in the band, want none", y0, y1)
	}
	// two lanes are twice a lane: a stereo recording's wash covers both
	y0, y1, _ := ed.audLaneSpan("mic")
	if y1-y0 != 2*waveLaneH {
		t.Errorf("the stereo recording spans %.0f px, want %.0f", y1-y0, 2*waveLaneH)
	}
	// and the recordings are laid out the way drawAudio lays them out: a pad
	// above the first, the gap between each pair, and nothing left over at the
	// bottom. The bright wash is drawn from these numbers, so a layout that
	// drifts from the drawing's puts it over the wrong waveform.
	r0, r1, _ := ed.audLaneSpan("room")
	if y0 != wavePad || r0-y1 != waveGap || r1+wavePad != float64(ed.audioHeight()) {
		t.Errorf("the lanes start at %.0f, leave %.0f between recordings and end at %.0f in "+
			"an area %d high — want a %.0f pad and a %.0f gap",
			y0, r0-y1, r1, ed.audioHeight(), wavePad, waveGap)
	}
}

// TestCopyingInALaneTakesThatSound: the selection says which band drew it, and
// ⧉ Copy takes the hand to match -- footage from the pictures, one recording's
// sound from a lane. Copying is still reading: the cut is untouched either way.
func TestCopyingInALaneTakesThatSound(t *testing.T) {
	a, ed := selEd(t)

	ed.sel.t0, ed.sel.t1, ed.sel.active, ed.sel.aud = 20, 35, true, "mic"
	a.copyClicked()
	if !ed.copyOn || ed.copyAud != "mic" || ed.copyFrom != 20 || ed.copyLen != 15 {
		t.Errorf("copy of a lane selection = on=%v of=%q from=%v len=%v, want mic 20 for 15 s",
			ed.copyOn, ed.copyAud, ed.copyFrom, ed.copyLen)
	}
	if len(ed.undo) != 0 || len(ed.segs) != 1 {
		t.Error("taking a copy of sound edited the cut — copying is reading")
	}

	// the same drag on the pictures is footage, and the hand must not keep the
	// lane the last one was taken from
	ed.sel.aud = ""
	a.copyClicked()
	if ed.copyAud != "" {
		t.Errorf("a selection on the pictures copied sound from %q", ed.copyAud)
	}
}

// TestPastedSoundIsLaidOverTheFootage: the sound half of ⧉ Paste. The picture
// is left alone -- the segment is an audio insert over running footage, the
// video does not get one frame longer -- and the file is started where it was
// copied from, which is the whole difference between a copy and a file.
func TestPastedSoundIsLaidOverTheFootage(t *testing.T) {
	a, ed := selEd(t)
	ed.auds[0].start = 5 // the recording came in five seconds into the session
	ed.copyFrom, ed.copyLen, ed.copyOn, ed.copyAud = 20, 15, true, "mic"
	ed.playhead, ed.hasPlay = 100, true
	before := ed.cutLen()

	a.pasteCopy()
	var got *cutSeg
	for i := range ed.segs {
		if ed.segs[i].isInsert() {
			got = &ed.segs[i]
		}
	}
	if got == nil {
		t.Fatal("no insert in the cut after pasting sound")
	}
	if !got.audioIns() || got.spliced() {
		t.Errorf("pasted %+v — want sound over running footage, not a splice", *got)
	}
	if got.S != 100 || got.E != 115 {
		t.Errorf("pasted sound covers %.1f-%.1f s, want the copied 15 s at the red line", got.S, got.E)
	}
	if math.Abs(got.Ss-15) > 1e-9 {
		t.Errorf("the sound starts %.3f s into its file, want 15 — the copy was 20 s into a "+
			"session the recording joined at 5", got.Ss)
	}
	if got.Ins != "mic.wav" {
		t.Errorf("pasted sound names %q, want the recording's file relative to the project", got.Ins)
	}
	if grew := ed.cutLen() - before; math.Abs(grew) > 1e-9 {
		t.Errorf("the video grew by %.1f s — sound laid over footage costs it nothing", grew)
	}
	if ed.copyOn {
		t.Error("the copy is still in hand after pasting — Insert would stay Paste forever")
	}
	if len(ed.undo) != 1 {
		t.Errorf("pasting sound pushed %d undo state(s), want exactly 1", len(ed.undo))
	}
}

// TestSoundCopiedFromBeforeARecordingStartsAtItsBeginning: a selection that
// began in silence before the recording joined has nothing earlier to play, so
// the file starts at its own top rather than at a negative second ffmpeg would
// refuse.
func TestSoundCopiedFromBeforeARecordingStartsAtItsBeginning(t *testing.T) {
	a, ed := selEd(t)
	ed.auds[0].start = 30
	ed.copyFrom, ed.copyLen, ed.copyOn, ed.copyAud = 10, 15, true, "mic"
	ed.playhead, ed.hasPlay = 100, true
	a.pasteCopy()
	for _, s := range ed.segs {
		if s.audioIns() && s.Ss != 0 {
			t.Errorf("the sound starts %.3f s into its file, want its beginning", s.Ss)
		}
	}
}

// TestPastingSoundFromARecordingThatIsGoneChangesNothing: a copy outlives the
// selection it was taken from, so it can outlive a reload too. Refusing beats
// pasting silence from a path nobody has.
func TestPastingSoundFromARecordingThatIsGoneChangesNothing(t *testing.T) {
	a, ed := selEd(t)
	ed.copyFrom, ed.copyLen, ed.copyOn, ed.copyAud = 20, 15, true, "vanished"
	ed.playhead, ed.hasPlay = 100, true
	a.pasteCopy()
	if len(ed.segs) != 1 || len(ed.undo) != 0 {
		t.Errorf("pasting sound from a missing recording left %v", ed.segs)
	}
}

// TestTheLanesShowWhatTheCutKeeps: the user's sentence, as pixels. Green over
// the seconds the video keeps and none over the seconds it drops, in the row
// where sound is now chosen.
func TestTheLanesShowWhatTheCutKeeps(t *testing.T) {
	_, ed := selEd(t)
	ed.segs = []cutSeg{{S: 30, E: 60}} // x 120-240 at four px a second
	at := renderLanes(t, ed, 400, ed.audioHeight())

	// +gutterPx on every x: the tape starts a strip in from the widget's edge
	// (cut_gutter.go)
	_, keptG, _ := at(gutterPx+180, 20)
	_, dropG, _ := at(gutterPx+300, 20)
	if keptG <= dropG {
		t.Errorf("the lanes are %d green over kept footage and %d over dropped — "+
			"what is in the cut has to be readable here", keptG, dropG)
	}
	// and the hard border, which is where the eye lands the sound
	r, g, b := at(gutterPx+120, 20)
	if g < 150 || g <= r || g <= b {
		t.Errorf("the border of the kept stretch is rgb(%d,%d,%d), want a green line", r, g, b)
	}
}

// TestTheLanesShowWhichSoundIsSelected: a selection covers every lane, because
// it is a span of session time -- and the recording it is OF is washed again
// on top, which is the only thing on the page that says which sound ⧉ Copy
// will take.
func TestTheLanesShowWhichSoundIsSelected(t *testing.T) {
	_, ed := selEd(t)
	ed.segs = nil // no green in the way of the blue
	ed.sel.t0, ed.sel.t1, ed.sel.active, ed.sel.aud = 70, 90, true, "mic"
	at := renderLanes(t, ed, 400, ed.audioHeight())

	_, _, micB := at(320, 20) // inside the selection, on the mic's lanes
	_, _, camB := at(320, 80) // inside it, on the camera's
	_, _, outB := at(200, 20) // clear of it
	if micB <= camB {
		t.Errorf("the selected recording is %d blue and the other %d — the lane the "+
			"selection is of has to stand out", micB, camB)
	}
	if camB <= outB {
		t.Errorf("the lanes are %d blue inside the selection and %d outside — the span "+
			"crosses every lane", camB, outB)
	}
	// drawn on the pictures, no lane is the one it is of: they are all tinted
	// alike, and nothing claims a recording that was never chosen
	ed.sel.aud = ""
	at = renderLanes(t, ed, 400, ed.audioHeight())
	_, _, micB = at(320, 20)
	_, _, camB = at(320, 80)
	if micB != camB {
		t.Errorf("a selection made on the pictures washes the mic %d and the camera %d — "+
			"it is of neither", micB, camB)
	}
}

// TestThePreviewPlaysCopiedSoundFromWhereItWasCopied: the preview and the
// render have to stand on the same second, or scrubbing through a pasted
// sound is not what the finished video has. A file chosen from disk plays
// from its top; a copied stretch of a lane plays from the second it was
// copied at, which is the offset the render hands to -ss.
func TestThePreviewPlaysCopiedSoundFromWhereItWasCopied(t *testing.T) {
	_, ed := selEd(t)
	if got := ed.cardInto(&cutSeg{S: 100, E: 115, Ins: "sting.wav"}, 4); got != 4 {
		t.Errorf("a sound file from disk is %v s into itself 4 s in, want its own 4", got)
	}
	if got := ed.cardInto(&cutSeg{S: 100, E: 115, Ins: "mic.wav", Ss: 15}, 4); math.Abs(got-19) > 1e-9 {
		t.Errorf("copied sound is %v s into its recording 4 s in, want 19", got)
	}
	// copied footage keeps the offset it always had
	if got := ed.cardInto(&cutSeg{S: 40, E: 40, Ins: "copy:12.000", Dur: 5}, 2); math.Abs(got-14) > 1e-9 {
		t.Errorf("copied footage is %v s into its recording 2 s in, want 14", got)
	}
	if got := ed.cardInto(nil, 3); got != 3 {
		t.Errorf("no card is %v s into nothing, want the playhead's own 3", got)
	}
}

// TestTheSoundVerbsAreWired pins the seams: the press that says which band
// drew the selection, the one place an insert is laid over the footage, the
// chooser that follows the band, and the render's -ss for a sound that starts
// inside its file.
func TestTheSoundVerbsAreWired(t *testing.T) {
	pins := map[string][]string{
		"cut.go": {
			"Ss float64 `json:\"ss,omitempty\"`",
			"ed.sel.aud = ed.audAtY(y)", // the press names the lane
			"ed.copyAud, ed.copyCam = ed.sel.aud, ed.sel.lane",
			"n := ed.addSound(a.storePath(au.path), at, ed.copyLen, ss, ed.copyAud)", // paste lays it over
			"ss := au.at(math.Max(ed.copyFrom, au.start))",                           // never before the lane's top
			"func (ed *cutEditor) layOver(s cutSeg) {",                               // one place, two ways in
			"return ed.layOverSound(cutSeg{S: t, E: t + dur, Ins: path, Ss: ss, Lane: lane})",
			`title, name, exts = "Insert a sound over the selected seconds", "Audio", audExts`,
		},
		"cut_audio.go": {
			"func (ed *cutEditor) audAtY(y float64) string {",
			"func (ed *cutEditor) audLaneSpan(base string) (float64, float64, bool) {",
			"if s.isInsert() && !(s.audioIns() && !s.spliced()) {", // the picture band's own rule
			"cr.SetSourceRGBA(0.2, 0.8, 0.3, 0.16)",                // green, kept
			"if y0, y1, ok := ed.audLaneSpan(ed.sel.aud); ok {",    // which sound is in hand
		},
		"cut_insview.go": {
			"ed.player.CardSound(want, ed.cardInto(s, into), want != \"\")",
			"return into + s.Ss", // the preview starts where -ss starts
		},
		"produce.go": {
			"sndAt float64",
			"snd: path, sndAt: s.Ss,",
			"if c.sndAt > 0 {",
			`args = append(args, "-ss", fmt.Sprintf("%.3f", c.sndAt))`,
		},
	}
	for file, wants := range pins {
		b, err := os.ReadFile(file)
		if err != nil {
			t.Fatal(err)
		}
		for _, want := range wants {
			if !strings.Contains(string(b), want) {
				t.Errorf("%s does not contain %q", file, want)
			}
		}
	}
}
