package main

// Which stretches of a recording become the voice reference: the diarization
// says who had the floor, the transcript says who was using it.

import (
	"strings"
	"testing"
)

// say builds a run of transcript rows at a steady pace, so a test can state
// "twenty seconds of talking" without counting words by hand.
func words(n int) string { return strings.TrimSpace(strings.Repeat("word ", n)) }

func say(spk string, from, to float64, perRow int) []seg4 {
	var out []seg4
	for t := from; t+1 <= to; t += 2 {
		out = append(out, seg4{s: t, e: t + 1.8, spk: spk, text: words(perRow)})
	}
	return out
}

// The old rule was length alone, and length is where somebody HELD the floor,
// not where they spoke: a long turn can be a laugh and a think. The take with
// the most words in it wins even when a quieter one runs longer.
func TestTheTakeWithTheMostSaidInItWins(t *testing.T) {
	turns := []span{
		{0, 30, "SPEAKER_00"},  // long, and nearly silent
		{40, 52, "SPEAKER_00"}, // shorter, and talked all the way through
	}
	var rows []seg4
	rows = append(rows, seg4{s: 5, e: 8, spk: "SPEAKER_00", text: "yeah"})
	rows = append(rows, say("SPEAKER_00", 40, 52, 5)...)

	got := refCuts(turns, rows)
	if len(got) == 0 {
		t.Fatal("nothing was picked at all")
	}
	if got[0].s < 40 {
		t.Errorf("picked %+v first — the 30 s of near-silence beat the 12 s of talking", got[0])
	}
	if len(got) != 1 {
		t.Errorf("kept %d takes, want only the one somebody was speaking in: %+v", len(got), got)
	}
}

// A diarization turn opens and closes on a voice detector's edges; the words
// inside start later and stop earlier. The reference is the words, not the
// silence around them.
func TestATakeIsTrimmedToTheWordsInsideIt(t *testing.T) {
	turns := []span{{0, 30, "SPEAKER_00"}}
	rows := say("SPEAKER_00", 8, 22, 6)

	got := refCuts(turns, rows)
	if len(got) != 1 {
		t.Fatalf("picked %d takes, want 1: %+v", len(got), got)
	}
	if got[0].s < 8 || got[0].e > 22 {
		t.Errorf("the take is %+v, want it inside the 8–22 s that was spoken", got[0])
	}
	if got[0].dur() < refMinLen {
		t.Errorf("trimming left %.1f s, shorter than a take is allowed to be", got[0].dur())
	}
	if got[0].words == 0 {
		t.Error("the take counted no words, so nothing weighed it")
	}
}

// Ranked by what was said, not by how long it ran: a dense take beats a longer
// one that both clear the bar, because the budget below only fits a few and
// the ones it drops should be the worst.
func TestTakesAreOrderedByHowMuchWasSaidInThem(t *testing.T) {
	turns := []span{
		{0, 20, "SPEAKER_00"},  // 20 s, sparse but talking
		{40, 52, "SPEAKER_00"}, // 12 s, dense
	}
	rows := append(say("SPEAKER_00", 0, 20, 4), say("SPEAKER_00", 40, 52, 12)...)

	got := refCuts(turns, rows)
	if len(got) != 2 {
		t.Fatalf("picked %d takes, want both: %+v", len(got), got)
	}
	if got[0].s < 40 {
		t.Errorf("the order is %+v, want the 12 s with %d words ahead of the 20 s with %d",
			got, got[1].words, got[0].words)
	}
	if got[0].dur() >= got[1].dur() {
		t.Fatal("the test's own takes rank the same by length, so it proves nothing")
	}
}

// Trimming can leave less than a take is allowed to be: a long turn holding
// three seconds of words is three seconds of words.
func TestATurnTrimmedTooShortIsNotATake(t *testing.T) {
	turns := []span{
		{0, 30, "SPEAKER_00"},
		{40, 52, "SPEAKER_00"},
	}
	rows := append(say("SPEAKER_00", 10, 13, 6), say("SPEAKER_00", 40, 52, 6)...)

	got := refCuts(turns, rows)
	if len(got) != 1 || got[0].s < 40 {
		t.Errorf("picked %+v, want only the take with %.0f s of speech in it", got, refMinLen)
	}
}

// A transcript row runs up to mergeMaxLen and a diarization edge is
// approximate, so a sentence lands across the boundary often. It counts for
// whichever side holds most of it, or a take is credited with words that were
// said outside it and ranked above a take where they were not.
func TestASentenceAcrossTheEdgeCountsForOneSideOnly(t *testing.T) {
	turns := []span{
		{20, 32, "SPEAKER_00"},
		{50, 62, "SPEAKER_00"},
	}
	// nine of its twelve seconds are before the turn, so it is not this turn's
	rows := []seg4{{s: 11, e: 23, spk: "SPEAKER_00", text: words(60)}}
	rows = append(rows, say("SPEAKER_00", 24, 32, 4)...)
	rows = append(rows, say("SPEAKER_00", 50, 62, 8)...)

	got := refCuts(turns, rows)
	if len(got) != 2 {
		t.Fatalf("picked %d takes, want both: %+v", len(got), got)
	}
	if got[0].s < 50 {
		t.Errorf("the order is %+v, want the take those 60 words were not said in second", got)
	}
	if got[1].s < 24 {
		t.Errorf("the second take is %+v, want it to start where its own words do", got[1])
	}
}

// A take never reaches outside the turn it came from. A row counted by its
// middle can still begin before the turn or run past its end, and the seconds
// out there are the clearance the next speaker is kept behind.
func TestATakeReachesNoFurtherThanTheTurn(t *testing.T) {
	turns := []span{{20, 40, "SPEAKER_00"}}
	rows := []seg4{{s: 17, e: 27, spk: "SPEAKER_00", text: words(30)}} // begins before it
	rows = append(rows, say("SPEAKER_00", 28, 34, 5)...)
	rows = append(rows, seg4{s: 35, e: 44, spk: "SPEAKER_00", text: words(20)}) // runs past it

	got := refCuts(turns, rows)
	if len(got) != 1 {
		t.Fatalf("picked %d takes, want 1: %+v", len(got), got)
	}
	if got[0].s < 20 || got[0].e > 40 {
		t.Errorf("the take is %+v, want it inside the 20–40 s turn it came from", got[0])
	}
}

// A row ASR put no words in stands for silence it could not read, and a take
// that ended before it does not reach out to it.
func TestAWordlessRowDoesNotStretchATake(t *testing.T) {
	turns := []span{{20, 40, "SPEAKER_00"}}
	rows := append(say("SPEAKER_00", 22, 30, 5), seg4{s: 36, e: 39.5, spk: "SPEAKER_00"})

	got := refCuts(turns, rows)
	if len(got) != 1 {
		t.Fatalf("picked %d takes, want 1: %+v", len(got), got)
	}
	if got[0].e > 31 {
		t.Errorf("the take ends at %.1f s, out over the seconds nobody said anything in", got[0].e)
	}
}

// A project with no transcript still needs a voice. Every take reads zero
// words, nothing clears the speech bar, and the ranking falls back to the
// longest first — which is exactly what this did before the transcript was
// consulted at all.
func TestWithNoTranscriptTheLongestStretchIsStillTaken(t *testing.T) {
	turns := []span{
		{0, 8, "SPEAKER_00"},
		{20, 40, "SPEAKER_00"},
		{50, 56, "SPEAKER_00"},
	}
	got := refCuts(turns, nil)
	if len(got) != 3 {
		t.Fatalf("picked %d takes, want all 3: %+v", len(got), got)
	}
	if got[0].dur() != 20 || got[1].dur() != 8 {
		t.Errorf("the order is %+v, want longest first", got)
	}
}

// The same fallback catches a speaker slow enough that nothing clears the bar:
// a thin reference is worth more than an error.
func TestASpeakerTooSlowToClearTheBarIsStillCloned(t *testing.T) {
	turns := []span{{0, 20, "SPEAKER_00"}}
	rows := []seg4{{s: 2, e: 18, spk: "SPEAKER_00", text: "well ... I suppose so"}}
	got := refCuts(turns, rows)
	if len(got) != 1 {
		t.Fatalf("picked %d takes, want the one there is: %+v", len(got), got)
	}
	if float64(got[0].words) >= refMinRate*got[0].dur() {
		t.Fatal("the test's own stretch clears the bar, so it proves nothing")
	}
}

// The credits ASR invents over silence — "Untertitel von ...", "thanks for
// watching" — are a handful of words spread across a long quiet, which is what
// the speech rate is there to turn away.
func TestAStretchOfInventedCreditsIsNotAVoice(t *testing.T) {
	turns := []span{
		{0, 25, "SPEAKER_00"},
		{40, 48, "SPEAKER_00"},
	}
	rows := []seg4{{s: 1, e: 24, spk: "SPEAKER_00", text: "Untertitel von Amara.org"}}
	rows = append(rows, say("SPEAKER_00", 40, 48, 5)...)

	got := refCuts(turns, rows)
	if len(got) != 1 || got[0].s < 40 {
		t.Errorf("picked %+v, want only the 8 s somebody really spoke in", got)
	}
}

// The reference is one person. A stretch with anyone else near it is dropped,
// pad included, because diarization edges are approximate and a syllable of
// the wrong voice is a syllable the clone learns.
func TestAStretchWithAnyoneElseNearItIsNotTaken(t *testing.T) {
	turns := []span{
		{0, 20, "SPEAKER_00"},
		{21, 23, "SPEAKER_01"}, // inside the pad after it
		{40, 60, "SPEAKER_00"},
	}
	rows := append(say("SPEAKER_00", 0, 20, 5), say("SPEAKER_00", 40, 60, 5)...)
	got := refCuts(turns, rows)
	if len(got) != 1 || got[0].s < 40 {
		t.Errorf("picked %+v, want only the stretch nobody else is near", got)
	}
	if soloTurn(turns, span{0, 20, "SPEAKER_00"}, "SPEAKER_00") {
		t.Errorf("a turn with another speaker %.0f s away counts as solo, and the pad is %.0f s",
			1.0, refPad)
	}
}

// Whoever talked most is who the recording is of; everyone else is what the
// room leaked into it.
func TestTheReferenceIsTheSpeakerWhoTalkedMost(t *testing.T) {
	turns := []span{
		{0, 4, "SPEAKER_01"},
		{10, 40, "SPEAKER_00"},
		{50, 54, "SPEAKER_01"},
	}
	if got := domSpeaker(turns); got != "SPEAKER_00" {
		t.Errorf("the recording is of %q", got)
	}
	if domSpeaker(nil) != "" {
		t.Error("an empty diarization named a speaker")
	}
	if got := refCuts(nil, nil); got != nil {
		t.Errorf("an empty diarization was cut into %+v", got)
	}
}

// The reference is assembled under a budget: a few pieces, a handful of
// seconds. Past that more material stops helping, and the takes are ranked so
// that what is dropped is the worst of them.
func TestTheReferenceIsBuiltFromTheBestFewTakesOnly(t *testing.T) {
	src := funcBody(t, "narrate_voice.go", `func \(a \*App\) ensureVoiceBase\(`)
	for _, pin := range []string{
		`loadSeg4(filepath.Join(a.inputsDir(), base, "transcript.tsv"))`,
		"refCuts(turns, rows)",
		"total >= refWant || i >= refTakeMax",
		"math.Min(d, refWant-total)",
		// ...and the budget is the AUTOMATIC pick's alone: a set chosen by
		// hand is taken whole (narrate_take.go)
		"len(hand) == 0",
	} {
		if !strings.Contains(src, pin) {
			t.Errorf("the reference build no longer has %q", pin)
		}
	}
	if refWant > 30 || refTakeMax > 4 {
		t.Errorf("the budget grew to %v s over %d takes", refWant, refTakeMax)
	}
}
