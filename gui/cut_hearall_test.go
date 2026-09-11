package main

// The whole-lane switch, seen on the cut rather than on its own plate.
//
// Switching a recording off for the whole cut is one press that writes the
// same answer onto every scene (toggleLaneAll) -- there is no lane-level state
// anywhere, and nothing that could disagree with the scenes. The page did not
// show it that way: the per-scene answer was drawn for the scene in hand and
// for no other, so the press darkened one plate beside a name and left every
// green selection looking exactly as it had. A control whose effect you cannot
// see is a control with a state of its own, whatever the code says.
//
// So every scene draws its own answer, in both bands: grey over the lanes it
// silences, on the recorders' band and on the strip under the pictures.

import (
	"math"
	"strings"
	"testing"
)

// silenceEd is heardTracks with two scenes on the first camera and nothing in
// hand -- the state the switch is pressed in, where nothing on the page was
// saying anything about which lanes were heard.
func silenceEd(t *testing.T) *cutEditor {
	t.Helper()
	ed := newTestEd(t)
	ed.vids, ed.auds = heardTracks()
	ed.relayout()
	ed.segs = []cutSeg{{S: 20, E: 40}, {S: 60, E: 80}}
	return ed
}

func TestEverySceneShowsTheLanesItSilences(t *testing.T) {
	ed := silenceEd(t)
	if got := ed.laneSilences(); len(got) != 0 {
		t.Errorf("a cut that silences nothing is greyed in %d place(s)", len(got))
	}
	// the switch's press, in the state it leaves behind: every scene told
	ed.segs[0].Quiet = []string{"mic"}
	ed.segs[1].Quiet = []string{"mic"}
	got := ed.laneSilences()
	if len(got) != 2 {
		t.Fatalf("a lane switched off for the whole cut greys %d scene(s), want 2", len(got))
	}
	y0, y1, ok := ed.audLaneSpan("mic")
	if !ok {
		t.Fatal("the mic has no lanes in the band")
	}
	for i, w := range got {
		if w.y0 != y0 || w.y1 != y1 {
			t.Errorf("scene %d is greyed at y %g–%g, want the mic's own %g–%g", i, w.y0, w.y1, y0, y1)
		}
	}
	if math.Abs(got[0].x0-ed.xOf(20)) > 1e-9 || math.Abs(got[0].x1-ed.xOf(40)) > 1e-9 {
		t.Errorf("the first scene is greyed from x %g to %g, want its own seconds", got[0].x0, got[0].x1)
	}
	// the scene the badges are about is left to drawHearBadges, which paints
	// it in the same grey: painted here as well it would be the darkest scene
	// on the page for a reason nobody could read off it
	ed.segOn, ed.segSel = true, 0
	if got := ed.laneSilences(); len(got) != 1 || math.Abs(got[0].x0-ed.xOf(60)) > 1e-9 {
		t.Errorf("with the first scene in hand the band greys %+v, want the other scene alone", got)
	}
	ed.dropSeg()
	// an insert brings its own sound and no scene silences it
	ed.segs = append(ed.segs, cutSeg{S: 90, E: 95, Ins: "card.png", Quiet: []string{"mic"}})
	if got := ed.laneSilences(); len(got) != 2 {
		t.Errorf("a card was greyed as a scene that silences a lane: %+v", got)
	}
}

// The same on the strip under the pictures, for the sound filmed with the
// camera the scene is SHOWN from -- the only strip with an answer to give.
func TestASceneThatDropsItsOwnCameraSoundSaysSoOnTheStrip(t *testing.T) {
	ed := silenceEd(t)
	if got := ed.pairSilences(); len(got) != 0 {
		t.Errorf("a cut that hears its cameras greys %d strip(s)", len(got))
	}
	// the first scene runs at 20 s, which is a1's footage on row 0
	ed.segs[0].Quiet = []string{"a1"}
	got := ed.pairSilences()
	if len(got) != 1 {
		t.Fatalf("the strip is greyed in %d place(s), want 1: %+v", len(got), got)
	}
	top := ed.laneTop(0) + ed.laneH()
	if got[0].y0 != top || got[0].y1 != top+waveLaneH {
		t.Errorf("the strip is greyed at y %g–%g, want %g–%g", got[0].y0, got[0].y1, top, top+waveLaneH)
	}
	// a scene that drops the recorder instead says nothing here: the strip is
	// about the camera's own sound
	ed.segs[0].Quiet = []string{"mic"}
	if got := ed.pairSilences(); len(got) != 0 {
		t.Errorf("silencing the mic greyed a camera's strip: %+v", got)
	}
}

// Both bands paint it, inside their own translation -- a wash in timeline
// coordinates drawn after the view is put back would land off the side of the
// widget -- and the switch that writes the answers is still one press over
// every scene, which is what these washes are the picture of.
func TestBothBandsDrawTheSilences(t *testing.T) {
	for _, c := range []struct{ file, head, call string }{
		{"cut_audio.go", `func \(ed \*cutEditor\) drawAudio\(cr \*cairo\.Context, w, h int\) \{`,
			"ed.drawSilences(cr, ed.laneSilences(), vx0, vx1)"},
		{"cut.go", `func \(ed \*cutEditor\) drawTrack\(cr \*cairo\.Context, w, h int\) \{`,
			"ed.drawSilences(cr, ed.pairSilences(), vx0, vx1)"},
	} {
		body := funcBody(t, c.file, c.head)
		i := strings.Index(body, c.call)
		if i < 0 {
			t.Errorf("%s no longer draws what each scene silences", c.file)
			continue
		}
		// ...before the view is put back: a band that restores by hand
		// (drawAudio) rather than on the way out (drawTrack's defer)
		if j := strings.Index(body, "\n\tcr.Restore()"); j >= 0 && j < i {
			t.Errorf("%s draws the silences after the view is put back, so they land off the widget", c.file)
		}
	}
	// and the switch is still the scenes themselves and nothing else
	body := funcBody(t, "cut_hear.go", `func \(ed \*cutEditor\) toggleLanesAll\(bases \[\]string, name string\) \{`)
	if !strings.Contains(body, "for i := range ed.segs {") || !strings.Contains(body, "s.Quiet = next") {
		t.Error("the whole-lane switch no longer writes its answer onto every scene")
	}
}
