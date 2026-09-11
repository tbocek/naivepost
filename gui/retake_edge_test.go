package main

// The edges of a retake, read off the sound rather than the stamp.

import (
	"strings"
	"testing"
)

// env builds a mono envelope at waveHz from a picture of it: one rune per
// 100 ms, "." for room tone and "#" for speech, starting at session second off.
func env(off float64, pic string) *edges {
	var b []uint8
	for _, r := range pic {
		v := uint8(4)
		if r == '#' {
			v = 120
		}
		for i := 0; i < 10; i++ { // ten buckets to the tenth
			b = append(b, v)
		}
	}
	return &edges{wf: &waveform{hz: waveHz, chans: [][]uint8{b}}, off: off}
}

// The cut ends where the last kept sound stops, not on the stamp of the first
// abandoned word -- which runs late, and lands inside that word.
func TestTheCutEndsWhereTheSoundBeforeItStops(t *testing.T) {
	//        0    0.5  1.0  1.5  2.0  2.5
	//        "art"      gap  "and the whole run"
	e := env(100, "#####.....##########")
	// the stamp for "and" sits 0.3 s into its sound, as a late stamp does
	got := e.endBefore(101.3)
	if want := 100.5 + edgePad; got < want-0.011 || got > want+0.011 {
		t.Errorf("the cut ends at %.2f, want just after the sound stops at %.2f", got, want)
	}
	// a stamp that already sits in the gap gives the same answer
	if got := e.endBefore(100.8); got < 100.5 || got > 100.6 {
		t.Errorf("from inside the gap the cut ends at %.2f, want ~100.55", got)
	}
	// speech with no gap in it is cut where the stamp says
	solid := env(100, "####################")
	if got := solid.endBefore(101.3); got != 101.3 {
		t.Errorf("with nowhere quieter to go the stamp moved to %.2f", got)
	}
	// and nothing within reach leaves it alone too
	far := env(100, "#..................."+"#")
	if got := far.endBefore(101.9); got != 101.9 {
		t.Errorf("an edge %.1f s away was used: %.2f", 1.8, got)
	}
}

// The cut resumes where the retake's sound begins. A stamp runs late and never
// early, so the resume may move back to the onset and may never move forward:
// forward is into the word the stamp names.
func TestTheCutResumesWhereTheRetakeStartsToSound(t *testing.T) {
	//        0    0.5  1.0  1.5  2.0
	//        quiet     "And the whole run"
	e := env(200, "..........##########")
	// the stamp half a second into the word: back to the onset
	got := e.startAt(201.5)
	if want := 201.0 - edgePad; got < want-0.011 || got > want+0.011 {
		t.Errorf("the cut resumes at %.2f, want the onset at %.2f", got, want)
	}
	// the stamp early, in a long quiet: it stands -- the sound has not come
	// yet, and the next one is not this word
	if got := e.startAt(200.4); got != 200.4 {
		t.Errorf("from a long quiet the cut resumes at %.2f, want the stamp itself", got)
	}
	// the stamp just AFTER a short sound: that sound is the word, stamped
	// late, and the cut resumes at its onset
	//              "And"   .3s   "they"
	late := env(300, "....####...#########")
	if got := late.startAt(300.9); got < 300.3 || got > 300.4 {
		t.Errorf("a late stamp resumed at %.2f, want the onset of the sound before it ~300.35", got)
	}
	// nothing within reach: unchanged
	silent := env(200, "....................")
	if got := silent.startAt(200.4); got != 200.4 {
		t.Errorf("with no sound in reach the stamp moved to %.2f", got)
	}
}

// The floor is the room, measured, not a number: the same rule has to find
// speech over a hiss and over silence.
func TestTheFloorIsTheRoomAndNotANumber(t *testing.T) {
	quietRoom := env(0, "....................")
	if thr := quietRoom.floor(1); thr < 8 || thr > 20 {
		t.Errorf("over silence the floor is %d, want a small margin over 4", thr)
	}
	loud := &edges{wf: &waveform{hz: waveHz, chans: [][]uint8{make([]uint8, 400)}}}
	for i := range loud.wf.chans[0] {
		loud.wf.chans[0][i] = 30 // a hiss
		if i%40 < 20 {
			loud.wf.chans[0][i] = 200 // and speech half the time
		}
	}
	if thr := loud.floor(2); thr <= 30 || thr >= 200 {
		t.Errorf("over a hiss with speech the floor is %d, want between the two", thr)
	}
}

// The server's tokens are pieces of words with a lone space for the boundary.
func TestTokensAreGluedBackIntoWords(t *testing.T) {
	toks := []asrToken{
		{Word: " wa", Start: 0, End: 1280}, {Word: "lle", Start: 1280, End: 2560}, {Word: "t", Start: 2560, End: 3840},
		{Word: " ", Start: 4000, End: 4000}, {Word: "ma", Start: 4000, End: 5280}, {Word: "kers", Start: 5280, End: 6560},
		{Word: ",", Start: 7000, End: 7080}, {Word: " at", Start: 8000, End: 9280},
	}
	got := glueWords(toks, 10)
	var words []string
	for _, w := range got {
		words = append(words, w.w)
	}
	if strings.Join(words, " ") != "wallet makers at" {
		t.Errorf("glued to %q, want %q", strings.Join(words, " "), "wallet makers at")
	}
	if got[0].s != 10 || got[0].e != 10+3840.0/sampleRate {
		t.Errorf("the first word spans %.3f-%.3f, want 10.000-10.240", got[0].s, got[0].e)
	}
	// the comma carries no sound and does not stretch the word before it
	if got[1].e != 10+6560.0/sampleRate {
		t.Errorf("punctuation stretched a word to %.3f", got[1].e)
	}
}

// Two token conventions, and no way to tell them apart one token at a time.
// Nemotron answers in pieces with a leading space starting each word; Qwen and
// the aligner answer in whole words with no leading space anywhere. Read a
// whole-word stream by the piece rule and a take becomes one word — which is
// exactly what happened, twice, before this was noticed.
func TestBothTokenConventionsBecomeWords(t *testing.T) {
	// nemotron: pieces, a leading space on the first of each word
	pieces := []asrToken{
		{Word: "And", Start: 0, End: 1280}, {Word: " they", Start: 1600, End: 2880},
		{Word: " fac", Start: 3200, End: 4480}, {Word: "to", Start: 4480, End: 5760},
		{Word: "red", Start: 5760, End: 7040},
	}
	got := glueWords(pieces, 0)
	if len(got) != 3 || got[2].w != "factored" {
		t.Errorf("pieces glued to %+v, want and/they/factored", spelled(got))
	}

	// qwen and the aligner: whole words, contiguous samples, no spaces
	whole := []asrToken{
		{Word: "And", Start: 26880, End: 30720}, {Word: "the", Start: 32000, End: 35840},
		{Word: "art", Start: 35840, End: 40960},
	}
	got = glueWords(whole, 0)
	if len(got) != 3 || got[1].w != "the" || got[2].w != "art" {
		t.Errorf("whole words glued to %+v, want three words", spelled(got))
	}
	// the giveaway: the old rule made this one word, because the tokens touch
	if len(got) == 1 {
		t.Error("a whole-word stream was welded into one word again")
	}
}

func spelled(ws []srcWord) []string {
	var out []string
	for _, w := range ws {
		out = append(out, w.w)
	}
	return out
}
