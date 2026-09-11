package main

// The preview's volume sliders: one gain for the whole preview, shown wherever
// a video can be played. The players themselves are GStreamer pipelines and
// stay out of a unit test, so the knob's arithmetic is tested straight and the
// wiring -- who is told, and when -- is pinned in the source.

import (
	"math"
	"strings"
	"testing"
)

func TestTheVolumeIsOneNumberAndStaysOnTheDial(t *testing.T) {
	was := previewVol
	defer SetPreviewVolume(was)

	// the slider hands over 0..1; anything outside is a bug upstream, and the
	// gain must not amplify (playbin would happily go to 10)
	SetPreviewVolume(0.35)
	if previewVol != 0.35 {
		t.Fatalf("previewVol = %v, want 0.35", previewVol)
	}
	SetPreviewVolume(1.7)
	if previewVol != 1 {
		t.Fatalf("previewVol = %v, want clamped to 1", previewVol)
	}
	SetPreviewVolume(-0.2)
	if previewVol != 0 {
		t.Fatalf("previewVol = %v, want clamped to 0", previewVol)
	}
}

func TestTheVolumeReachesEveryPipeline(t *testing.T) {
	src := readSrc(t, "player.go")
	for _, pin := range []string{
		// every pipeline is born at the loudness its player already runs at:
		// the master in NewPlayer, and every aux (a recording under the
		// footage, an insert's own sound) at newAux's vol argument
		"setGain(a.gain, pb, vol, false)",
		"newAux(fmt.Sprintf(\"mix%d\", i), t, p.vol(), p.laneErr(t.base))",
		"allPlayers = append(allPlayers, p)",
		// ...and a turn of the dial visits the live ones, mixes and card
		// included -- a slider that only reached the footage would rebalance
		// the preview instead of turning it down
		"for _, a := range p.mix {",
		"if p.card != nil {",
	} {
		if !strings.Contains(src, pin) {
			t.Errorf("player.go lost its pin %q", pin)
		}
	}
	if n := strings.Count(src, "p.applyVol()"); n < 3 {
		t.Errorf("applyVol is called %d times; the birth, the slider and the effect all need it", n)
	}

	// one builder, so the two sliders cannot drift into two behaviours
	for file, pin := range map[string]string{
		"cut.go":     "bar.Append(volumeCtl())",
		"narrate.go": "transport.Append(volumeCtl())",
	} {
		if !strings.Contains(readSrc(t, file), pin) {
			t.Errorf("%s no longer shows a volume control (%q)", file, pin)
		}
	}
}

// A volume slider belongs beside the ▶ that uses it, and nowhere else. The run
// bar spans every page, so its slider was only ever justified by one of them:
// Produce, which watched its own result and had no transport of its own. Produce
// does not play anything now, which leaves the bar's slider a control over
// silence on all five pages -- and a slider that does nothing on the page you
// are standing on is worse than none, because it invites the turn that will not
// help. The two pages that do play keep theirs, on the page, next to their own ▶.
func TestTheVolumeSliderIsOnlyWhereSomethingPlays(t *testing.T) {
	if src := readSrc(t, "main.go"); strings.Contains(src, "volumeCtl()") ||
		strings.Contains(src, "volBox") {
		t.Error("the run bar has a volume slider again, on four pages that play nothing")
	}
	if strings.Contains(readSrc(t, "player.go"), "func barVolume(") {
		t.Error("barVolume is back: the run bar is deciding which page its slider is for")
	}
	// ...and the pages that do play still have one, which is the other half of
	// the same claim (the pin map above names where)
	for _, file := range []string{"cut.go", "narrate.go"} {
		if !strings.Contains(readSrc(t, file), "volumeCtl()") {
			t.Errorf("%s plays a preview with no volume control", file)
		}
	}
}

// Produce renders; it does not play. It used to cue the finished file into a
// picture at the bottom of the page and take over the run bar's ▶ ⏹ to drive
// it -- a second video player, in the app that has just written a video file,
// with the settings that made it scrolled off the top of the page.
func TestProduceRendersAndDoesNotPlay(t *testing.T) {
	src := readSrc(t, "produce.go")
	for _, pin := range []string{"p.player", "videoFrame(", "PlaySegment("} {
		if strings.Contains(src, pin) {
			t.Errorf("the Produce page is playing video again (%q)", pin)
		}
	}
	// the run bar's ▶ is the render on this page, always -- there is no
	// playback left for it to be handed to
	if strings.Contains(funcBody(t, "runbar.go", `func \(a \*App\) pageTransport\(`), `"produce"`) {
		t.Error("the run bar still hands ▶ to a Produce preview that no longer exists")
	}
	// and the one shared player it borrowed goes with it
	if strings.Contains(readSrc(t, "main.go"), "a.player") {
		t.Error("main still builds the player Produce was the only user of")
	}
}

// One number, three sliders. A volume control is built once per place a video
// can be played from -- the run bar, the Cut transport, the Narrate transport
// -- and moving any of them has to move the others, or the second page a
// preview is played on shows 100% while playing at 40.
func TestEveryVolumeSliderShowsTheSameNumber(t *testing.T) {
	src := readSrc(t, "player.go")
	for _, pin := range []string{
		// every slider built goes on the roll...
		"volScales = append(volScales, sc)",
		// ...the one being dragged writes the shared number...
		"SetPreviewVolume(sc.Value() / 100)",
		// ...and then the others, without their own handlers writing back
		"volSyncing = true",
		"o.SetValue(previewVol * 100)",
		"volSyncing = false",
	} {
		if !strings.Contains(src, pin) {
			t.Errorf("player.go lost its pin %q", pin)
		}
	}
	if !strings.Contains(funcBody(t, "player.go", `func volumeCtl\(\) \*gtk.Box \{`),
		"sc.SetValue(previewVol * 100)") {
		t.Error("a slider built later starts at 100% instead of at what the others already say")
	}
}

// The volume EFFECT reaches the preview, and it reaches it multiplied by the
// slider rather than instead of it: turning the preview down still turns a
// boosted stretch down.
func TestTheVolumeEffectIsHeardInThePreview(t *testing.T) {
	wasVol := previewVol
	defer SetPreviewVolume(wasVol)

	p := &Player{fxGain: 1}
	SetPreviewVolume(0.5)
	if got := p.vol(); math.Abs(got-0.5) > 1e-9 {
		t.Fatalf("with no effect the player runs at %v, want the slider's 0.5", got)
	}
	p.fxGain = 4
	if got := p.vol(); math.Abs(got-2) > 1e-9 {
		t.Fatalf("a x4 effect at half volume runs at %v, want 2", got)
	}
	// and neither end can push the property past what playbin takes
	SetPreviewVolume(1)
	p.fxGain = fxMaxGain
	if got := p.vol(); got != fxMaxGain {
		t.Fatalf("full slider under the loudest effect runs at %v, want %v", got, fxMaxGain)
	}

	// the cut's half: the tick and every seek tell the player where the line
	// is, so a scrub across a boosted stretch sounds like one
	cut := readSrc(t, "cut.go")
	for _, pin := range []string{
		"ed.player.SetFxGain(fxGainAt(ed.fx, ed.playhead))",
		"ed.syncPlayGain()",
	} {
		if !strings.Contains(cut, pin) {
			t.Errorf("cut.go lost its pin %q", pin)
		}
	}
	if n := strings.Count(cut, "ed.syncPlayGain()"); n != 2 {
		t.Errorf("syncPlayGain is called %d times, want 2: the tick, which follows a "+
			"playing line across a band, and setPlayhead, which is every seek and "+
			"every scrub", n)
	}
}

// The app's gain and mute are a volume element inside the pipeline, never
// playbin's own properties. Those are the sound server's per-stream volume
// with a pulse or pipewire sink, and the server remembers a stream volume per
// application: the first 0 the app ever wrote -- a stop, a nought gain, a
// hush -- was restored to every stream it opened afterwards, across restarts,
// and both naivepost streams sat at 0 in the system mixer with nothing in the
// app at 0 any more. So playbin's volume and mute are written exactly once
// each, full and unmuted, when the pipeline is built (resetStreamVolume) --
// which overwrites what the server remembered -- and in setGain's fallback
// for a machine with no volume plugin. Nowhere else.
func TestTheGainNeverTouchesTheServersStreamVolume(t *testing.T) {
	src := readSrc(t, "player.go")
	for _, prop := range []string{`SetObjectProperty("volume"`, `SetObjectProperty("mute"`} {
		if n := strings.Count(src, prop); n != 2 {
			t.Errorf("player.go writes %s %d times, want 2: once in resetStreamVolume and once in setGain", prop, n)
		}
	}
	reset := funcBody(t, "player.go", `func resetStreamVolume\(`)
	if !strings.Contains(reset, `"volume", 1.0`) || !strings.Contains(reset, `"mute", false`) {
		t.Errorf("resetStreamVolume does not put the stream back to full and unmuted:\n%s", reset)
	}
	// every pipeline goes through it as it is built
	for _, fn := range []string{`func NewPlayer\(`, `func newAux\(`} {
		b := funcBody(t, "player.go", fn)
		if !strings.Contains(b, "audioFilter(") || !strings.Contains(b, "resetStreamVolume(pb)") {
			t.Errorf("%s does not build the shared audio path and reset the stream volume:\n%s", fn, b)
		}
	}
	// and the filter is scaletempo then our volume, ghosted into one bin
	af := funcBody(t, "player.go", `func audioFilter\(`)
	for _, want := range []string{`ElementFactoryMake("volume"`, "tempo.Link(gain)", `NewGhostPad("sink"`, `NewGhostPad("src"`} {
		if !strings.Contains(af, want) {
			t.Errorf("audioFilter no longer contains %q", want)
		}
	}
}
