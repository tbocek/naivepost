package main

// A lane the scene does not hear is not started at all.
//
// Silencing it used to be the mute property alone: the pipeline was cued,
// seeked and set to PLAYING like any other, and the property was supposed to
// keep it inaudible. It did not, quite -- ▶✂ on a cut whose scene drops a lane
// played that lane for about a second first. A pipeline that is started has a
// moment between its first buffer and the property reaching the sink, and the
// deferred seek that fires when a preroll finishes could set it PLAYING again
// after the hush had already landed.
//
// So the answer is the one the report asked for: do not play it. A silenced
// lane and a lane with nothing recorded at this second are now the same thing
// to the preview (audible) -- a pipeline left standing in PAUSED -- and the
// scene's answer is settled before anything is told to play, not a tick after.
//
// The pipelines are GStreamer and stay out of a unit test, so what runs here is
// the decision, and the source pins carry it onto them.

import (
	"strings"
	"testing"
)

func TestASilencedLaneIsNotStartedAtAll(t *testing.T) {
	// a recording that covers the whole session, half a minute behind the
	// footage's clock
	a := &auxAudio{base: "mic", delta: -30, hi: 600}
	if !a.audible(60) {
		t.Fatal("a lane the scene hears, with material at this second, is not played")
	}
	a.mute = true
	if a.audible(60) {
		t.Error("a lane the scene silences is still started")
	}
	// it is the hush that stops it, not the clock: the material is still there,
	// which is what makes this a different answer from the one below
	if !a.running(60) {
		t.Error("the silenced lane lost the material it has at this second")
	}
	// and the older answer still holds on its own -- before it began, and past
	// its end, there is nothing to play whatever the scene says
	a.mute = false
	if a.audible(10) {
		t.Error("a recording that had not started yet is played")
	}
	if a.audible(700) {
		t.Error("a recording that had already stopped is played")
	}
}

// place is the only place a mix pipeline is started, so it is the only place
// that has to ask. Every path that moves the sound goes through it: the
// transport, a seek, and the hush lifting off a lane that is heard again.
func TestOnlyOnePathStartsARecording(t *testing.T) {
	for _, tc := range []struct{ head, want string }{
		{`func \(p \*Player\) place\(a \*auxAudio, t float64, play bool\) \{`, "if !a.audible(t) {"},
		{`func \(p \*Player\) syncMix\(play bool\) \{`, "p.place(a, pos, play)"},
		{`func \(p \*Player\) SeekTo\(t float64\) \{`, "p.place(a, t, p.playing)"},
		{`func \(a \*auxAudio\) cue\(t float64, play bool, rate float64, stop float64\) \{`, "if !a.audible(t) {"},
	} {
		if b := funcBody(t, "player.go", tc.head); !strings.Contains(b, tc.want) {
			t.Errorf("%s no longer contains %q — a silenced lane can be started from it",
				tc.head, tc.want)
		}
	}
	// the preroll's deferred seek is the one starter that runs later than the
	// hush that stopped the lane, so it asks again before it lets go
	if !strings.Contains(readSrc(t, "player.go"), "if a.play && !a.mute {") {
		t.Error("the deferred seek starts a lane the hush silenced while it prerolled")
	}
	// and the hush landing on a running lane stops it rather than only turning
	// it down, which is the whole of the fix
	// -- and stops it all the way to READY, so a lane nobody hears holds no
	// stream at the sound server and sits in no system mixer
	if b := funcBody(t, "player.go", `func \(p \*Player\) applyMute\(\) \{`); !strings.Contains(b,
		"a.pb.SetState(gst.StateReady)") {
		t.Error("a lane the scene has just silenced is left running under the mute")
	}
}

// The answer has to be in before the transport moves. showInsert settles it at
// the bottom of setPlayhead, which is where every other path reaches it -- but
// by then the pipelines below have already been cued and told to play.
func TestTheSceneIsAskedBeforeAnythingIsToldToPlay(t *testing.T) {
	body := funcBody(t, "cut.go", `func \(ed \*cutEditor\) setPlayhead\(t float64\) \{`)
	hush := strings.Index(body, "ed.syncHush()")
	if hush < 0 {
		t.Fatal("setPlayhead no longer asks what the scene under the line hears")
	}
	for _, later := range []string{"ed.player.PlaySegment(", "ed.player.SeekTo("} {
		if i := strings.Index(body, later); i < 0 || i < hush {
			t.Errorf("%s runs before the hush, so the lane plays first and is silenced after", later)
		}
	}
	// the mix has to be built first, though: the lanes the answer is about are
	// the ones under THIS piece of footage
	if i := strings.Index(body, "ed.player.SetMix("); i < 0 || i > hush {
		t.Error("the hush is applied before the mix under this footage exists")
	}
	// and ▶ itself: it starts the recordings (syncMix), so it asks first too
	tog := funcBody(t, "cut.go", `func \(ed \*cutEditor\) toggle\(\) \{`)
	h, play := strings.Index(tog, "ed.syncHush()"), strings.Index(tog, "ed.player.Toggle()")
	if h < 0 || play < 0 || h > play {
		t.Error("▶ moves the transport before it knows which lanes the scene hears")
	}
}
