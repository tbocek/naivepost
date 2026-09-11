package main

// What the describe model is told was said around a chunk of frames. The
// election has two caps that can each bind first, a boundary rule that decides
// whether a line is evidence or merely context, and a rendering that has to
// survive an empty transcript without looking like a broken pipeline.

import (
	"strings"
	"testing"
)

// rows named for what they are, so a failure message says which line moved
func ctxRows() []tsvRow {
	return []tsvRow{
		{s: 0.5, e: 1.5, text: "way before"},     // 8.5 s before the chunk
		{s: 2.0, e: 3.0, text: "far before"},     // 7.0 s before
		{s: 6.0, e: 7.0, text: "near before"},    // 3.0 s before
		{s: 9.5, e: 10.5, text: "straddles in"},  // starts before the chunk, ends inside
		{s: 11.4, e: 12.0, text: "during one"},   //
		{s: 14.8, e: 15.6, text: "during two"},   //
		{s: 17.1, e: 18.0, text: "near after"},   // 1.1 s after the last frame
		{s: 21.0, e: 22.0, text: "far after"},    // 5.0 s after
		{s: 25.5, e: 26.0, text: "way after"},    // 9.5 s after
		{s: 26.5, e: 27.0, text: "just too far"}, // 10.5 s after -- outside the window
	}
}

func texts(rows []tsvRow) string {
	var out []string
	for _, r := range rows {
		out = append(out, r.text)
	}
	return strings.Join(out, "|")
}

func TestElectSpeechPicksTheNearestFewEitherSide(t *testing.T) {
	before, during, after := electSpeech(ctxRows(), 10, 16)
	// two a side, nearest first while choosing but chronological when emitted
	if got := texts(before); got != "far before|near before" {
		t.Errorf("before = %q, want the two nearest in reading order", got)
	}
	if got := texts(after); got != "near after|far after" {
		t.Errorf("after = %q, want the two nearest in reading order", got)
	}
	// the straddling line is evidence, whole and in the middle section
	if got := texts(during); got != "straddles in|during one|during two" {
		t.Errorf("during = %q, want the straddling line and both inside ones", got)
	}
}

// Both caps apply on their own, and either can be the one that binds.
func TestElectSpeechAppliesBothCapsIndependently(t *testing.T) {
	// count binds: five lines within the window, two survive
	dense := []tsvRow{
		{s: 5.0, e: 5.5, text: "a"}, {s: 6.0, e: 6.5, text: "b"}, {s: 7.0, e: 7.5, text: "c"},
		{s: 8.0, e: 8.5, text: "d"}, {s: 9.0, e: 9.5, text: "e"},
	}
	before, _, _ := electSpeech(dense, 10, 16)
	if got := texts(before); got != "d|e" {
		t.Errorf("count cap: before = %q, want the two nearest", got)
	}
	// window binds: only one line is close enough, so only one comes along
	sparse := []tsvRow{{s: 10.0, e: 11.0, text: "ancient"}, {s: 80.0, e: 81.0, text: "old"}, {s: 95.0, e: 97.0, text: "recent"}}
	before, _, _ = electSpeech(sparse, 100, 106)
	if got := texts(before); got != "recent" {
		t.Errorf("window cap: before = %q, want only the one inside 10 s", got)
	}
	// and the window is inclusive at exactly 10 s, exclusive past it
	edge := []tsvRow{{s: 89.0, e: 90.0, text: "exactly ten"}, {s: 88.0, e: 89.5, text: "ten and a half"}}
	before, _, _ = electSpeech(edge, 100, 106)
	if got := texts(before); got != "exactly ten" {
		t.Errorf("edge: before = %q, want 10.0 s kept and 10.5 s dropped", got)
	}
}

// The three sets partition the transcript. That is what keeps a line from
// being shown twice -- as evidence and again as context -- without any
// bookkeeping, and what keeps a line from being dropped in the gap between
// them. Both failures are silent in a prompt, so they are checked here.
func TestElectSpeechIsDisjointAndTotal(t *testing.T) {
	// every chunk placement against the same rows, including ones that start
	// or end mid-segment and ones the transcript does not reach
	for _, c := range []struct{ s, e float64 }{
		{0, 6}, {10, 16}, {10.5, 10.5}, {9.9, 10.1}, {15, 30}, {60, 66}, {-5, 0},
	} {
		rows := ctxRows()
		before, during, after := electSpeech(rows, c.s, c.e)
		seen := map[string]int{}
		for _, set := range [][]tsvRow{before, during, after} {
			for _, r := range set {
				seen[r.text]++
			}
		}
		for text, n := range seen {
			if n > 1 {
				t.Errorf("chunk %g-%g: %q appears %d times", c.s, c.e, text, n)
			}
		}
		// nothing may fall through a crack: a row that is in none of the three
		// sets must have been dropped by the window or the count, never by an
		// interval test that misses it
		for _, r := range rows {
			if seen[r.text] > 0 {
				continue
			}
			overlaps := r.e > c.s && r.s < c.e
			if overlaps {
				t.Errorf("chunk %g-%g: %q overlaps but was not emitted", c.s, c.e, r.text)
			}
		}
	}
}

// one source, named the way describeVideo names the footage's own audio
func ownAudio(rows []tsvRow) []speechSrc {
	return []speechSrc{{label: "clip.mkv", rows: rows}}
}

// The rendered block, exactly. The headings are load-bearing -- the prompt
// tells the model to trust one section and not describe the others -- so the
// wording is pinned rather than described. So is the label on every line: it is
// the same label the cut and the narration steps use for the same line
// (tlLabel), because four prompts describing their input four different ways is
// three formats too many for a small model to learn.
func TestSpeechBlockRendersAllThreeSections(t *testing.T) {
	rows := []tsvRow{
		{s: 3.8, e: 5.0, spk: "SPEAKER_00", text: "so the trick with these panels is"},
		{s: 11.4, e: 12.0, spk: "SPEAKER_00", text: "you pull the left one first"},
		{s: 14.8, e: 15.6, spk: "SPEAKER_01", text: "and the latch releases"},
		{s: 17.1, e: 18.0, spk: "SPEAKER_00", text: "now the same on the other side"},
	}
	want := `--- context before (do not describe) ---
[-6.2s] SPEAKER_00: so the trick with these panels is
--- spoken during these frames ---
[+1.4s] SPEAKER_00: you pull the left one first
[+4.8s] SPEAKER_01: and the latch releases
--- context after (do not describe) ---
[+7.1s] SPEAKER_00: now the same on the other side`
	if got := speechBlock(ownAudio(rows), "", 10, 16); got != want {
		t.Errorf("block is\n%s\nwant\n%s", got, want)
	}
}

// Two microphones on one chunk. Each is elected on its own -- so the cap is per
// speaker and a talkative track cannot crowd the other out of its own context
// -- and the survivors are merged into one run in time order, because the block
// is a record of a conversation and reading it out of order misreads who
// answered whom.
func TestSpeechBlockMergesEverySourceInTimeOrder(t *testing.T) {
	srcs := []speechSrc{
		{label: "clip.mkv", rows: []tsvRow{
			{s: 11.0, e: 11.4, spk: "SPEAKER_01", text: "game one"},
			{s: 14.0, e: 14.4, spk: "SPEAKER_01", text: "game two"},
			{s: 4.0, e: 4.5, spk: "SPEAKER_01", text: "game before A"}, // both inside the window: the two nearest
			{s: 6.0, e: 6.5, spk: "SPEAKER_01", text: "game before B"}, // survive, and they are these two
			{s: 1.0, e: 1.5, spk: "SPEAKER_01", text: "game before C"}, // furthest, so dropped by the count cap
		}},
		// ...and this one is the narrator's, which the finished video never
		// plays: the label says so here exactly as it does in the cut and in the
		// narration brief
		{label: "mic.flac", rows: []tsvRow{
			{s: 12.0, e: 12.4, spk: "SPEAKER_00", text: "mic one"},
			{s: 15.0, e: 15.4, spk: "SPEAKER_00", text: "mic two"},
			{s: 5.0, e: 5.5, spk: "SPEAKER_00", text: "mic before"}, // its own budget, untouched by clip.mkv's
		}},
	}
	got := speechBlock(srcs, "mic.flac", 10, 16)
	want := `--- context before (do not describe) ---
[-6.0s] SPEAKER_01: game before A
[-5.0s] NARRATOR: mic before
[-4.0s] SPEAKER_01: game before B
--- spoken during these frames ---
[+1.0s] SPEAKER_01: game one
[+2.0s] NARRATOR: mic one
[+4.0s] SPEAKER_01: game two
[+5.0s] NARRATOR: mic two
--- context after (do not describe) ---
(none)`
	if got != want {
		t.Errorf("merged block is\n%s\nwant\n%s", got, want)
	}
	if strings.Contains(got, "game before C") {
		t.Error("the count cap did not bind per source")
	}
}

// A silent stretch still gets all three headings. An omitted section is
// indistinguishable from a pipeline failure, and the model cannot tell "nobody
// spoke" from "the speech was lost on the way here".
func TestSpeechBlockSaysNothingRatherThanSayingNothing(t *testing.T) {
	want := `--- context before (do not describe) ---
(none)
--- spoken during these frames ---
(no speech during these frames)
--- context after (do not describe) ---
(none)`
	if got := speechBlock(nil, "", 10, 16); got != want {
		t.Errorf("empty block is\n%s\nwant\n%s", got, want)
	}
	// and a transcript that exists but is nowhere near this chunk reads the same
	far := []tsvRow{{s: 0, e: 1, text: "long gone"}, {s: 600, e: 601, text: "much later"}}
	if got := speechBlock(ownAudio(far), "", 300, 306); got != want {
		t.Errorf("out-of-range block is\n%s\nwant\n%s", got, want)
	}
	// so does a session whose second microphone recorded nothing at all
	quiet := []speechSrc{{label: "clip.mkv"}, {label: "mic.flac"}}
	if got := speechBlock(quiet, "", 10, 16); got != want {
		t.Errorf("silent-sources block is\n%s\nwant\n%s", got, want)
	}
}

// Whatever the ASR produced goes through untouched: the wording is the
// evidence, and a line that reads oddly is a line the model should see reading
// oddly. Segments stay one per line for the same reason -- the boundaries are
// where the pauses were.
func TestSpeechBlockPassesTextThroughUnchanged(t *testing.T) {
	rows := []tsvRow{
		{s: 11.0, e: 11.5, spk: "SPEAKER_00", text: "  no wait  "},
		{s: 11.5, e: 12.0, spk: "SPEAKER_00", text: `he said "left" i think`},
		{s: 12.0, e: 12.5, spk: "SPEAKER_00", text: "uh"},
	}
	got := speechBlock(ownAudio(rows), "", 10, 16)
	// a line is no longer wrapped in quotes of its own: a transcript full of
	// speech marks put quotes inside quotes, and the label already says where
	// the line ends and the words begin
	for _, want := range []string{
		`[+1.0s] SPEAKER_00:   no wait  `,
		`[+1.5s] SPEAKER_00: he said "left" i think`,
		`[+2.0s] SPEAKER_00: uh`,
	} {
		if !strings.Contains(got, want) {
			t.Errorf("block is missing the line %s:\n%s", want, got)
		}
	}
	if n := strings.Count(got, "[+"); n != 3 {
		t.Errorf("%d lines emitted for 3 segments -- adjacent ones were merged:\n%s", n, got)
	}
}
