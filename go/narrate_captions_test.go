package main

// "No audio — captions only": a project whose narration is written and timed
// but never spoken. The lines become subtitles for Produce to burn in or ship,
// and each carries a placement -- top, center, or the bottom that subtitles
// default to. What is tested here is the seam: the choice is a voice like any
// other (round-trips, keys the caches), a placement word is routed to Pos and
// never sent to the TTS as an emotion, and nothing on the way to a render
// tries to speak.

import (
	"os/exec"
	"strings"
	"testing"
)

func TestNoAudioIsAVoiceYouCanChoose(t *testing.T) {
	a := &App{outDir: t.TempDir(), curCmds: map[*exec.Cmd]bool{}}

	// first in the picker: it is the one entry that is not a voice at all
	vs := a.listVoices()
	if len(vs) == 0 || vs[0].id != captionsVoice {
		t.Fatalf("listVoices()[0] = %+v, want the %q entry first", vs[:min(1, len(vs))], captionsVoice)
	}

	if a.captionsOnly() {
		t.Fatal("a fresh project thinks it is captions-only")
	}
	// choosing it runs no ffmpeg and installs no reference -- nothing will
	// ever be synthesized from it
	if err := a.setVoice(voiceOpt{id: captionsVoice}); err != nil {
		t.Fatalf("setVoice(captions): %v", err)
	}
	if !a.captionsOnly() {
		t.Fatal("the captions voice was chosen and captionsOnly() still says no")
	}
	if exists(a.refBase()) || exists(a.refPath()) {
		t.Fatal("choosing no-audio left a voice reference on disk")
	}
	// a later session reads the choice back out of the folder
	if got := (&App{outDir: a.outDir}); !got.captionsOnly() {
		t.Fatalf("reopened project speaks in %q, want captions-only", got.voiceID())
	}

	// with no voice a written line is a finished line: ▶ must not see work
	n := &narrator{a: a, entries: []narrEntry{{S: 0, E: 10, Text: "hello"}}}
	if got := n.unspoken(); got != 0 {
		t.Fatalf("unspoken() = %d in captions mode, want 0 — ▶ would try to speak", got)
	}
	// and a re-roll has no take to draw again
	a.narr = n
	a.rerollEntry(0)
	if n.entries[0].Roll != 0 {
		t.Fatal("rerollEntry moved the take of a line that is never spoken")
	}
}

func TestAPlacementWordIsAPlacementNotAnEmotion(t *testing.T) {
	// the three placements, their kin, and the spelled-out default
	for word, want := range map[string]string{
		"top": "top", "TOP": "top", " Center ": "center", "centre": "center",
		"middle": "center", "bottom": "", "Bottom": "",
	} {
		p, ok := posTag(word)
		if !ok || p != want {
			t.Errorf("posTag(%q) = %q, %v — want %q, true", word, p, ok, want)
		}
	}
	// an emotion is not one
	if _, ok := posTag("calm"); ok {
		t.Fatal(`posTag("calm") claimed a placement — every emotion would stop reaching the TTS`)
	}

	// the box renders the placement as the tag when there is no delivery...
	if got := lineText(narrEntry{Text: "look up here", Pos: "top"}); got != "[top] look up here" {
		t.Fatalf("lineText = %q, want the placement as the tag", got)
	}
	// ...and the delivery wins when a spoken line also carries one
	if got := lineText(narrEntry{Text: "hi", Emotion: "calm", Pos: "top"}); got != "[calm] hi" {
		t.Fatalf("lineText = %q, want the emotion shown for a spoken line", got)
	}
	// what the box shows parses back as the same tag word
	if emo, _, _, text := lineParts("[top] look up here"); emo != "top" || text != "look up here" {
		t.Fatalf("lineParts = %q, %q", emo, text)
	}

	// the model's "pos" survives binding, normalized to the three words
	// Produce can burn -- anything it invents is the bottom
	segs := []cutSeg{{S: 0, E: 10}, {S: 20, E: 30}, {S: 40, E: 50}}
	entries, problem := bindEntries(segs, []rawEntry{
		{Start: 0, End: 10, Text: "a", Pos: "TOP"},
		{Start: 20, End: 30, Text: "b", Pos: "middle"},
		{Start: 40, End: 50, Text: "c", Pos: "sideways"},
	})
	if problem != "" {
		t.Fatalf("bindEntries: %s", problem)
	}
	if entries[0].Pos != "top" || entries[1].Pos != "center" || entries[2].Pos != "" {
		t.Fatalf("bound placements = %q, %q, %q", entries[0].Pos, entries[1].Pos, entries[2].Pos)
	}
}

// The writer is told when nobody will speak, and when nothing in the session
// was ever off-mic. Both ride the narrate prompt as addenda rather than
// shipping as styles, so a reworded prompt keeps working; the wiring is pinned
// to the source because writeNarration itself is an LLM call.
func TestThePromptKnowsWhenNobodySpeaks(t *testing.T) {
	if !strings.Contains(narrCaptionsAddendum, `"pos"`) ||
		!strings.Contains(narrCaptionsAddendum, "bottom|top|center") {
		t.Fatal("the captions addendum never asks for a placement")
	}
	if !strings.Contains(narrCaptionsAddendum, "READS") {
		t.Fatal("the captions addendum never says the lines are read, not heard")
	}
	if !strings.Contains(narrNoMicNote, "PLAYS WHAT PEOPLE SAID OUT LOUD") {
		t.Fatal("the note lost its point")
	}
	for file, pins := range map[string][]string{
		"narrate.go": {
			// writeNarration appends the right addendum to whatever wording the
			// user has in the box
			"system += \"\\n\\n\" + narrNoMicNote",
			"system += \"\\n\\n\" + narrCaptionsAddendum",
			// the run writes and then stops: no synthesis loop in captions mode
			"captions only — %d line(s) written, none spoken",
			// the ticking preview neither plays wavs nor holds to synthesize one
			"|| n.a.captionsOnly() {",
		},
		// a row's ▶ is a seek to the moment, not an audition
		"narrate_tts.go": {"read, never spoken; playing its moment"},
	} {
		s := readSrc(t, file)
		for _, pin := range pins {
			if !strings.Contains(s, pin) {
				t.Errorf("%s lost the captions seam pinned by %q", file, pin)
			}
		}
	}
}

// Whose voice is in the video is not a setting: it is what the scenes hear,
// and the narration's whole premise turns on it. The prompt is written for a
// session with the voices split off and silenced -- the narration is the only
// voice, so what was said is material to say better -- and the note rides
// along exactly where that is untrue.
//
// The old condition was "the session has no separate narrator recording",
// which is a different question and was right only by accident: a session
// with a narrator mic AND the footage's own voices kept was told the voices
// were nobody's to repeat.
func TestWhetherTheVideoPlaysWhatPeopleSaidIsWhatTheScenesHear(t *testing.T) {
	a := &App{}
	rows := []tsvRow{
		{s: 10, e: 12, spk: "EVENT", text: "a door", src: "cap"},
		{s: 13, e: 14, spk: "SPEAKER_01", text: "open it", src: "cap"},
		{s: 30, e: 31, spk: "SPEAKER_00", text: "mine", src: "voice"},
	}
	seg := func(s, e float64, quiet ...string) cutSeg { return cutSeg{S: s, E: e, Quiet: quiet} }

	// the footage's own voices, kept: the viewer hears them
	if !a.speechHeard([]cutSeg{seg(10, 20)}, rows) {
		t.Error("a scene keeping the footage's own track was read as silent")
	}
	// the same scene with that lane silenced: nothing said is in the video
	if a.speechHeard([]cutSeg{seg(10, 20, "cap")}, rows) {
		t.Error("a scene that silences the lane was read as playing it")
	}
	// a scene that covers no speech at all
	if a.speechHeard([]cutSeg{seg(50, 60)}, rows) {
		t.Error("a scene with no spoken line under it was read as playing one")
	}
	// an insert brings its own sound and carries none of the session's
	if a.speechHeard([]cutSeg{{S: 10, E: 20, Ins: "card.svg"}}, rows) {
		t.Error("an insert was read as playing the session's speech")
	}
	// the narrator's own microphone is never played, whatever the scene says
	a.selVid, a.selAud = []string{"/in/cap.mp4"}, []string{"/in/voice.wav"}
	a.selNarr[0] = "/in/voice.wav"
	if got := a.narratorMic(); got != "voice" {
		t.Fatalf("narratorMic = %q", got)
	}
	if a.speechHeard([]cutSeg{seg(28, 35)}, rows) {
		t.Error("the narrator's own microphone was read as played out loud")
	}
	// ...while the footage's voices under the same rule still are
	if !a.speechHeard([]cutSeg{seg(10, 20)}, rows) {
		t.Error("a narrator mic in the session silenced the footage's own voices too")
	}
	// and that is what decides the note
	if !strings.Contains(readSrc(t, "narrate.go"), "if a.speechHeard(segs, rows) {") {
		t.Error("the note no longer rides on what the scenes hear")
	}
}
