package main

// What the sound does over a speed effect: the five answers, and the two of
// them that let it come away from the picture.
//
// The arithmetic these tests hold is the whole idea. Two read-heads on one
// recording, the picture's and the sound's; a speed effect moves one of them
// and not the other; the gap that opens is closed where the effect says. What
// the gap costs is the rate's doing and not a fault: a fast stretch skips the
// sound it never reached, a slow one plays seconds it has already played, and
// both are what "keep talking while the picture runs on" means.

import (
	"math"
	"strings"
	"testing"
)

func TestTheOldSilentStopIsTheSilentAnswer(t *testing.T) {
	// a cut written before the answer was a word: the tick under the stop
	old := cutFx{Kind: "speed", T: 10, Dur: 2, Rate: 0, Mute: true}
	if got := old.sound(); got != sndMute {
		t.Errorf("an older cut's silent stop reads as %q, want %q", got, sndMute)
	}
	// ...and it is rewritten once, on load, so the key leaves the file
	got := migrateFx([]cutFx{old})
	if got[0].Mute || got[0].Snd != sndMute {
		t.Errorf("the migration left %+v", got[0])
	}
	// a stop that kept its sound is the plain answer, then and now
	if k := (cutFx{Kind: "speed", T: 10, Dur: 2, Rate: 0}).sound(); k != sndWith {
		t.Errorf("a stop that kept its sound reads as %q", k)
	}
	// a word this version does not know is the plain answer rather than
	// nothing at all: a cut written by a later build still renders
	if k := (cutFx{Kind: "speed", Snd: "whistling"}).sound(); k != sndWith {
		t.Errorf("an unknown answer reads as %q", k)
	}
	// the dropdown's two lists are one list read twice
	if len(sndKinds) != len(sndNames) {
		t.Fatalf("%d answers and %d names", len(sndKinds), len(sndNames))
	}
	for i, k := range sndKinds {
		if sndKindOf(sndIndex(k)) != k {
			t.Errorf("%q does not survive the trip through the dropdown (row %d)", k, i)
		}
	}
}

// The gap, in seconds of the recording: what the picture eats minus what the
// sound has time to.
func TestTheGapIsWhatTheRateCosts(t *testing.T) {
	for _, c := range []struct {
		what string
		f    cutFx
		want float64
	}{
		{"×4 over 20 s: 5 s on screen", cutFx{Kind: "speed", T: 0, Dur: 20, Rate: 4}, 15},
		{"×0.5 over 20 s: 40 s on screen", cutFx{Kind: "speed", T: 0, Dur: 20, Rate: 0.5}, -20},
		{"×1 has nothing to come apart", cutFx{Kind: "speed", T: 0, Dur: 20, Rate: 1}, 0},
		// a stop's footage runs at 1× underneath the held frame (applied), so
		// the sound is where it always was
		{"a stop", cutFx{Kind: "speed", T: 0, Dur: 20, Rate: 0}, 0},
		{"not a speed at all", cutFx{Kind: "zoom", T: 0, Dur: 20}, 0},
	} {
		if got := sndDebt(c.f); math.Abs(got-c.want) > 0.01 {
			t.Errorf("%s: the sound ends %g s out, want %g", c.what, got, c.want)
		}
	}
	// the line under the dropdown says it in words, and only for the answers
	// it is true of
	if n := sndNote(sndScene, 4, 20); !strings.Contains(n, "15 s behind") || !strings.Contains(n, "skips") {
		t.Errorf("a ×4 with the sound on its own clock reads %q", n)
	}
	if n := sndNote(sndFx, 0.5, 20); !strings.Contains(n, "20 s ahead") || !strings.Contains(n, "again") {
		t.Errorf("a ×0.5 with the sound on its own clock reads %q", n)
	}
	if n := sndNote(sndWith, 4, 20); n != "" {
		t.Errorf("the sound travelling with the picture has a note: %q", n)
	}
	if n := sndNote(sndWith, 0, 3); !strings.Contains(n, "1×") {
		t.Errorf("a stop does not say that its answers are all the same: %q", n)
	}
}

// The two effects that reach past their own bar say so on the lane: the tail
// runs from the effect's end to the second the sound goes back in sync.
func TestOnlyTheSceneAnswerDrawsATail(t *testing.T) {
	ed := newTestEd(t)
	ed.vids = []tlVideo{{base: "a", path: "a.mkv", start: 0, dur: 300, interval: 5, fps: 30}}
	ed.relayout()
	ed.segs = []cutSeg{{S: 0, E: 120}}

	f := cutFx{Kind: "speed", T: 10, Dur: 20, Rate: 4, Snd: sndScene}
	t0, t1, debt, ok := ed.sndTail(f)
	if !ok || t0 != 30 || t1 != 120 || math.Abs(debt-15) > 0.01 {
		t.Errorf("the tail runs %g–%g s at %g s out (ok=%v), want 30–120 at 15", t0, t1, debt, ok)
	}
	// the answer that closes the gap on its own last frame has nothing to draw
	f.Snd = sndFx
	if _, _, _, ok := ed.sndTail(f); ok {
		t.Error("the effect that goes back in sync at its own end drew a tail past it")
	}
	// nor has one whose scene ends with it
	f.Snd, f.Dur = sndScene, 110
	if _, _, _, ok := ed.sndTail(f); ok {
		t.Error("a tail was drawn into a scene that is already over")
	}
	// ...nor a ×1, which never comes apart
	f.Dur, f.Rate = 20, 1
	if _, _, _, ok := ed.sndTail(f); ok {
		t.Error("a ×1 drew a tail")
	}
}

// The plan the render walks: which clips read their sound from somewhere else,
// where the run stops, and where the splice that ends it falls.
func TestTheSoundRunsOnUntilTheAnswerSaysStop(t *testing.T) {
	v := &tlVideo{base: "a", path: "/f/a.mp4", start: 0, dur: 600}
	// four clips of one scene: 10 s of ×4 (2.5 s on screen), then three plain
	// 10 s stretches, the last of them a scene of its own
	clips := []prodClip{
		{video: v, sessS: 0, length: 2.5, rate: 4, scene: 0},
		{video: v, sessS: 10, length: 10, rate: 1, scene: 0},
		{video: v, sessS: 20, length: 10, rate: 1, scene: 0},
		{video: v, sessS: 40, length: 10, rate: 1, scene: 1},
	}
	fx := []cutFx{{Kind: "speed", T: 0, Dur: 10, Rate: 4, Snd: sndScene}}
	got := append([]prodClip(nil), clips...)
	audioPlan(got, fx)
	for i, want := range []float64{0, 2.5, 12.5} {
		if !got[i].audOwn || math.Abs(got[i].audSess-want) > 0.01 {
			t.Errorf("clip %d reads its sound from %g s (own=%v), want %g",
				i, got[i].audSess, got[i].audOwn, want)
		}
	}
	if got[3].audOwn {
		t.Error("the sound ran on into the next scene")
	}
	// the seam: the last shifted clip dips out, the one after it dips in
	if !got[2].audOut || !got[3].audIn {
		t.Errorf("the splice is not dipped: out=%v in=%v", got[2].audOut, got[3].audIn)
	}
	if got[0].audOut || got[1].audIn {
		t.Error("a clip in the middle of the run is dipped")
	}

	// the other answer stops with the effect's own clips
	got = append([]prodClip(nil), clips...)
	fx[0].Snd = sndFx
	audioPlan(got, fx)
	if !got[0].audOwn || got[1].audOwn {
		t.Errorf("the run reached %d clips, want the effect's own", 1+btoi(got[1].audOwn))
	}
	if !got[0].audOut || !got[1].audIn {
		t.Error("the effect's own seam is not dipped")
	}
	// and the answers that keep the sound with the picture plan nothing
	for _, k := range []string{sndWith, sndPitch, sndMute} {
		got = append([]prodClip(nil), clips...)
		fx[0].Snd = k
		audioPlan(got, fx)
		for i := range got {
			if got[i].audOwn {
				t.Errorf("%q left clip %d off the picture's clock", k, i)
			}
		}
		if k == sndPitch && !got[0].audPitch {
			t.Error("the pitched answer did not reach the clip it is on")
		}
	}
}

func btoi(b bool) int {
	if b {
		return 1
	}
	return 0
}

// A card brings its own sound, so a run cannot carry across one: the sound
// would be a file nothing on screen came from.
func TestARunStopsAtACard(t *testing.T) {
	v := &tlVideo{base: "a", path: "/f/a.mp4", start: 0, dur: 600}
	clips := []prodClip{
		{video: v, sessS: 0, length: 2.5, rate: 4, scene: 0},
		{ins: "card.svg", sessS: 10, length: 3, rate: 1, scene: 0},
		{video: v, sessS: 10, length: 10, rate: 1, scene: 0},
	}
	audioPlan(clips, []cutFx{{Kind: "speed", T: 0, Dur: 10, Rate: 4, Snd: sndScene}})
	if !clips[0].audOwn || clips[1].audOwn || clips[2].audOwn {
		t.Errorf("the run reached %v", []bool{clips[0].audOwn, clips[1].audOwn, clips[2].audOwn})
	}
}

// Silence is a window, not a property of the clip: a stop and a ×1 do not cut
// the segment list, so their bands lie inside a clip that is longer than they
// are.
func TestSilenceIsTheSecondsTheEffectCovers(t *testing.T) {
	fx := []cutFx{{Kind: "speed", T: 15, Dur: 5, Rate: 0, Snd: sndMute}}
	cues := hushCues(fx, 10, 30, 1, 30)
	if len(cues) != 1 || math.Abs(cues[0].s-5) > 0.01 || math.Abs(cues[0].e-10) > 0.01 {
		t.Fatalf("a stop 5 s into a 30 s clip is silent over %+v", cues)
	}
	// the expression the render enables the volume filter on holds the stops
	// and these together: one question, one filter
	expr := hushExpr(cues, []stillCue{{s: 20, e: 22, mute: true}})
	for _, want := range []string{"between(t,5.000,10.000)", "between(t,20.000,22.000)", "+"} {
		if !strings.Contains(expr, want) {
			t.Errorf("the silence expression %q is missing %q", expr, want)
		}
	}
	if hushExpr(nil, nil) != "" {
		t.Error("a clip with nothing silenced still asks for a volume filter")
	}
	// an effect that is not asking for silence is not in it
	fx[0].Snd = sndWith
	if got := hushCues(fx, 10, 30, 1, 30); len(got) != 0 {
		t.Errorf("a stop that keeps its sound was silenced anyway: %+v", got)
	}
}

// The filters, and where each one is used.
func TestTheRenderAsksForTheAnswerItWasGiven(t *testing.T) {
	if got := asetrateChain(4); got != ",asetrate=48000*4,aresample=48000" {
		t.Errorf("the pitched answer is %q", got)
	}
	if asetrateChain(1) != "" || asetrateChain(0) != "" {
		t.Error("a rate that changes nothing still asks for a filter")
	}
	src := readSrc(t, "produce.go")
	for _, want := range []string{
		// the sound off the picture's clock is its own read of the file
		`case c.audOwn && srcSound:`,
		`"-t", fmt.Sprintf("%.3f", c.length), "-i", c.audPath)`,
		// ...and takes neither of the with-the-picture chains
		`c.speed() != 1 && !c.audOwn {`,
		"slow = asetrateChain(c.speed())",
		// the silence, on the same filter the stop's window always used
		`if mute := hushExpr(c.hushes, c.stills); mute != "" {`,
		// the dip, on the bed and before the narration
		"if fx, lab := audDipChain(c, game); fx != \"\" {",
	} {
		if !strings.Contains(src, want) {
			t.Errorf("produce.go no longer contains %q", want)
		}
	}
	i := strings.Index(src, "audDipChain(c, game)")
	if j := strings.Index(src, "volume=%.3f,aresample=48000[bg]"); i < 0 || j < 0 || i > j {
		t.Error("the dip is applied after the narration is mixed in, so the voice dips with the bed")
	}
	// the dip itself: half at each side of a join nothing can cross
	c := prodClip{length: 10, audIn: true, audOut: true}
	fc, lab := audDipChain(c, "bed")
	if !strings.Contains(fc, "afade=t=in:st=0:d=0.150") ||
		!strings.Contains(fc, "afade=t=out:st=9.850:d=0.150") || lab == "bed" {
		t.Errorf("the dip came out %q -> %q", fc, lab)
	}
	if fc, lab := audDipChain(prodClip{length: 10}, "bed"); fc != "" || lab != "bed" {
		t.Errorf("a clip with no seam still filters its sound: %q", fc)
	}
}
