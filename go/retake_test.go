package main

// The retakes: the stretches said, broken off and said again.
//
// The pass itself is one small call; everything a test can hold is around it --
// what the model is shown, what is done with an answer, and what the cut is
// handed afterwards. The answer is line NUMBERS, which is the point: every one
// of them is checked back against the timeline here, so a wrong answer is a
// refused line rather than a hole somewhere in the video.

import (
	"math"
	"os"
	"strings"
	"testing"
)

// a session in two takes: a sentence trailing off, the recording stopping, and
// the same sentence again from further back
func takeRows() []tsvRow {
	return []tsvRow{
		{s: 90.0, e: 99.0, src: "take3", spk: "SPEAKER_00", text: "The team did not invent a new algorithm."},
		{s: 99.1, e: 108.0, src: "take3", spk: "SPEAKER_00", text: "their GPU implementation is about 10 times cheaper, and the whole run took roughly"},
		{s: 108.9, e: 109.2, src: "take3", spk: "SPEAKER_00", text: "13.5"},
		{s: 111.3, e: 111.4, src: "take3", spk: "SPEAKER_00", text: "GPU-years."},
		{s: 114.6, e: 118.2, src: "take4", spk: "SPEAKER_00", text: "The whole run took roughly 13.5 GPU-years,"},
		{s: 119.0, e: 123.0, src: "take4", spk: "SPEAKER_00", text: "which is around 400,000 US dollars market price,"},
	}
}

// What the pass reads. The pauses and the seam are the whole of the signal, and
// they cost nothing: the lines' own times say nobody spoke for two seconds, so
// there is nothing to hear and no audio to transform.
func TestTheRetakeBriefShowsPausesSeamsAndNumbers(t *testing.T) {
	b := retakeBrief(takeRows(), "")
	for _, want := range []string{
		"   1  [01:30.0-01:39.0]",                       // numbered, with its own seconds
		"   5  [01:54.6-01:58.2]",                       // ...all the way down
		"(2.1s pause)",                                  // the gap before the last fragment
		"the recording stops here; the next one begins", // and the seam it sits at
	} {
		if !strings.Contains(b, want) {
			t.Errorf("the brief does not show %q:\n%s", want, b)
		}
	}
	// a pause too short to mean anything is not drawn: every line would
	// otherwise carry one and the shape would be lost in it
	if strings.Contains(b, "(0.1s pause)") {
		t.Errorf("the brief marks the gap between two words of one sentence:\n%s", b)
	}
}

// An answer is line numbers, and every one is checked. What cannot be true is
// dropped with a note rather than applied: the model never writes a time, so a
// mistake is always a line this timeline can be asked about.
func TestAnImpossibleRetakeIsRefusedNotApplied(t *testing.T) {
	rows := takeRows()
	type ans = struct{ From, To, Again int }

	// the real one: lines 2-4 abandoned, said again at line 5
	got, notes := keepRetakes(rows, []ans{{2, 4, 5}})
	if len(got) != 1 || got[0].S != 99.1 || got[0].E != 111.4 || got[0].Again != 114.6 {
		t.Fatalf("the abandoned stretch came out as %+v (%v)", got, notes)
	}

	for _, c := range []struct {
		what string
		in   ans
	}{
		{"a line this timeline does not have", ans{2, 99, 0}},
		{"a stretch that ends before it starts", ans{4, 2, 0}},
		{"a replacement that comes before the attempt", ans{4, 5, 2}},
	} {
		if got, _ := keepRetakes(rows, []ans{c.in}); len(got) != 0 {
			// a replacement pointing backwards is not a replacement; the
			// stretch may still be a broken-off attempt, so what must not
			// survive is the claim about where it was said again
			if c.what == "a replacement that comes before the attempt" && got[0].Again == 0 {
				continue
			}
			t.Errorf("%s was applied: %+v", c.what, got)
		}
	}

	// two answers over the same lines both come through -- three pooled runs
	// name one stretch three ways -- and are folded into one AFTER each has
	// been verified on its own (mergeMarks), so nothing is dropped twice.
	// Refusing the second here used to empty the pool: one run's marks
	// pushed the cursor to the session's last line, and every other run's
	// then read as overlap.
	got, _ = keepRetakes(rows, []ans{{2, 4, 5}, {3, 4, 5}})
	if len(got) != 2 {
		t.Fatalf("the second answer over the same lines was refused up front: %+v", got)
	}
	if merged := mergeMarks(got); len(merged) != 1 || merged[0].S != 99.1 || merged[0].E != 111.4 {
		t.Errorf("two answers over one stretch did not fold into one mark: %+v", merged)
	}
	// ...and a mark whose retake lies INSIDE it is not a mark at all: one run
	// answers every stretch that way, "again" set to its own last line
	if got, notes := keepRetakes(rows, []ans{{2, 4, 4}}); len(got) != 0 {
		t.Errorf("a retake inside the stretch it replaces was applied: %+v (%v)", got, notes)
	}

	// a sentence said again three minutes later is a callback, not a retake
	far := append(takeRows(), tsvRow{s: 400, e: 404, src: "take9", text: "The whole run took roughly 13.5 GPU-years,"})
	if got, _ := keepRetakes(far, []ans{{2, 4, 7}}); len(got) != 0 {
		t.Errorf("a repetition %g s later was called a retake: %+v", 400-111.4, got)
	}
}

// And the whole answer is refused when it stops being a list of retakes: one
// wrong line number early can carry every later one with it, and half a session
// marked abandoned is not a result to apply line by line.
func TestAnAnswerThatWantsHalfTheSessionIsRefused(t *testing.T) {
	rows := takeRows()
	// ...against a session with a few minutes of kept speech in it, which is
	// what the ceiling is a fraction of
	for t0 := 130.0; t0 < 400; t0 += 10 {
		rows = append(rows, tsvRow{s: t0, e: t0 + 8, src: "take4", spk: "SPEAKER_00", text: "more of the session"})
	}
	said := speechSecs(rows)
	if tooMuch([]retake{{S: 99.1, E: 111.4}}, rows) {
		t.Errorf("a real retake (%.1fs of %.1fs) was refused as too much", 111.4-99.1, said)
	}
	if !tooMuch([]retake{{S: 0, E: 400}}, rows) {
		t.Error("an answer covering the whole session was accepted")
	}
}

// What the cut is handed: one line where the attempt was, and everything inside
// it -- what was said and what was on screen -- left out. Not hidden: the line
// says the seconds are there and were said again, which is what stops a model
// choosing a moment inside them.
func TestAnAbandonedStretchIsOneLineInTheCutBrief(t *testing.T) {
	rows := append(takeRows(), tsvRow{s: 100, e: 104, src: "take3", spk: "EVENT",
		text: "Calm; the presenter talks over a static article."})
	marks := []retake{{S: 99.1, E: 111.4, Again: 114.6}}

	out := sessionText(rows, "", marks)
	if !strings.Contains(out, "[99s | 01:39] (abandoned attempt to [111s | 01:51], said again at [114s | 01:54] -- already removed, read straight past it)") {
		t.Errorf("the abandoned stretch is not one line saying so:\n%s", out)
	}
	// the whole rendered line, since the sentence that REPLACED it quotes the
	// same words -- that is what a retake is
	for _, gone := range []string{": 13.5\n", ": GPU-years.\n", "Calm; the presenter"} {
		if strings.Contains(out, gone) {
			t.Errorf("%q is still in the brief inside an abandoned stretch:\n%s", gone, out)
		}
	}
	// what was kept is untouched, both sides of it
	for _, want := range []string{"did not invent a new algorithm", "The whole run took roughly 13.5 GPU-years,"} {
		if !strings.Contains(out, want) {
			t.Errorf("%q went with the stretch it is not in:\n%s", want, out)
		}
	}
	// one line for the stretch, not one per line inside it
	if n := strings.Count(out, "abandoned attempt"); n != 1 {
		t.Errorf("the stretch is folded into %d lines, want 1:\n%s", n, out)
	}
	// and with no marks at all the brief is exactly what it always was
	if plain := sessionText(rows, "", nil); strings.Contains(plain, "abandoned") ||
		!strings.Contains(plain, "GPU-years.") {
		t.Errorf("an unmarked timeline is rendered differently:\n%s", plain)
	}
}

// The marks survive the run that found them: written beside the transcript,
// read back by the cut. An empty file is an answer -- "asked, none found" --
// and a missing one is a project prepared before this existed.
func TestTheMarksAreWrittenAndReadBack(t *testing.T) {
	a := &App{outDir: t.TempDir()}
	marks := []retake{{S: 99.1, E: 111.4, Again: 114.6, To: 114.4, Text: "and the whole run took roughly"}}
	if err := a.writeRetakes(marks); err != nil {
		t.Fatal(err)
	}
	got := a.loadRetakes()
	if len(got) != 1 || got[0].S != 99.1 || got[0].E != 111.4 || got[0].Again != 114.6 || got[0].To != 114.4 {
		t.Fatalf("the marks came back as %+v", got)
	}
	// a file from before the edges were placed has four columns, and removes
	// the words alone
	os.WriteFile(a.retakeFile(), []byte("99.10\t111.40\t114.60\tsome words\n"), 0o644)
	if old := a.loadRetakes(); len(old) != 1 || old[0].To != 111.4 || old[0].Text != "some words" {
		t.Errorf("a four-column file read back as %+v", old)
	}
	if err := a.writeRetakes(nil); err != nil {
		t.Fatal(err)
	}
	if got := a.loadRetakes(); len(got) != 0 {
		t.Errorf("an empty answer read back as %+v", got)
	}
	if (&App{outDir: t.TempDir()}).loadRetakes() != nil {
		t.Error("a project with no file at all read back marks")
	}
}

// Where the pass sits: the last thing Prepare does, on the merged timeline. It
// needs the whole session in one stream -- an attempt and its replacement are
// usually two different recordings -- and the text already fixed, and this is
// the first moment both are true.
func TestTheRetakePassRunsOnTheMergedTimeline(t *testing.T) {
	src := readSrc(t, "transcript.go")
	merge := strings.Index(src, `"session.tsv"`)
	call := strings.Index(src, "a.findMarks(tl)")
	if merge < 0 || call < 0 || call < merge {
		t.Error("the retakes are looked for before the timeline they are about is merged")
	}
	// it is a marking, not the step: a pass that could not run leaves the
	// timeline unmarked and Prepare finishes
	if !strings.Contains(src, "the timeline stands unmarked") {
		t.Error("a failed retake pass takes the whole transcript step down with it")
	}
	// ...unless the user stopped the run, which is not a failure to survive
	if !strings.Contains(src, "errors.Is(err, errStopped)") {
		t.Error("⏹ during the retake pass is swallowed as a pass failure")
	}
	// and the cut reads the marks rather than asking again
	if !strings.Contains(readSrc(t, "cut_suggest.go"), "marks := a.loadRetakes()") {
		t.Error("the cut does not read the marks Prepare left")
	}
}

// ---- and what the words say about the answer ---------------------------------

func heard(text string, t0 float64) []srcWord {
	var out []srcWord
	for i, w := range strings.Fields(text) {
		out = append(out, srcWord{s: t0 + float64(i)*0.4, e: t0 + float64(i)*0.4 + 0.35, w: w})
	}
	return out
}

// A retake is rarely a whole line: you stop mid-sentence and pick it up from
// the last few words. Marking the whole line takes the words that were NOT said
// again out of the video, in the middle of a sentence the cut keeps both sides
// of -- so the claim is checked against the words, and only the part that comes
// back is dropped.
func TestOnlyTheWordsSaidAgainAreDropped(t *testing.T) {
	// the real one: everything up to "the art" is said once and stays; the
	// clause after it is said twice and goes
	said := heard("their gpu implementation is about 10 times cheaper than the previous public state of the art and the whole run took roughly thirteen", 99)
	again := heard("and the whole run took roughly thirteen gpu years which is around four hundred thousand", 114)
	k := repeatFrom(said, again)
	if k != 16 || said[k].w != "and" {
		t.Fatalf("the repeat starts at word %d (%+v), want the second \"and\" at 16", k, said[max(k, 0)])
	}

	// a sentence carried across a seam, with one word doubled: the sentence
	// stays and the doubled word goes
	said = heard("came mostly from the global south and it is the opposite of the tor browser for android which", 681)
	again = heard("which seems to have more users in the global north and a lot of work", 707)
	if k := repeatFrom(said, again); k != len(said)-1 {
		t.Errorf("a sentence continued across a seam was read as a retake from word %d", k)
	}

	// and one that was never said again is not a retake at all
	said = heard("so they used on already existing project and as", 194)
	again = heard("and the more it got away from that the more confused the agents got", 205)
	if k := repeatFrom(said, again); k >= 0 {
		t.Errorf("a stretch that is not repeated was called a retake from word %d", k)
	}
}

// The two attempts are never the same text. One microphone, one model, twice:
// "tor" and "tour", "keep" and "kept" -- two words of six, one edit each, in a
// phrase that is plainly the same phrase.
func TestTheSameWordsHeardTwiceStillMatch(t *testing.T) {
	said := heard("and the tor project keep hearing", 587)
	again := heard("and the tour project kept hearing the same request", 594)
	if k := repeatFrom(said, again); k != 0 {
		t.Errorf("the phrase matched from word %d, want the whole of it", k)
	}
	for _, c := range []struct {
		a, b string
		want bool
	}{
		{"tor", "tour", true},     // one heard it with a u
		{"keep", "kept", false},   // two edits apart: the run's threshold carries this one, not the matcher
		{"sub", "subject", true},  // broken off mid-word, then finished
		{"safe", "unsafe", false}, // not the same word, whatever it sounds like
		{"as", "was", false},      // too short to guess at
		{"the", "the", true},
	} {
		if got := sameWord(c.a, c.b); got != c.want {
			t.Errorf("sameWord(%q, %q) = %v, want %v", c.a, c.b, got, c.want)
		}
	}
}

// Trimming applies the answer: the mark keeps its end and gives up its front,
// and a mark with nothing repeated in it is dropped rather than applied.
func TestAMarkIsTrimmedToWhatComesBack(t *testing.T) {
	ws := append(heard("and the tor project keep hearing", 587), heard("and the tour project kept hearing the same request", 594)...)
	got, _, notes := trimToRepeat([]retake{{S: 587, E: 589.4, Again: 594}}, ws, nil)
	if len(got) != 1 || got[0].S != 587 {
		t.Fatalf("a stretch repeated in full was moved: %+v (%v)", got, notes)
	}

	ws = append(heard("came mostly from the global south which", 681), heard("which seems to have more users", 707)...)
	got, _, _ = trimToRepeat([]retake{{S: 681, E: 684, Again: 707}}, ws, nil)
	if len(got) != 1 || got[0].S <= 681 {
		t.Fatalf("only the doubled word should go, mark came back as %+v", got)
	}

	ws = append(heard("so they used on already existing project", 194), heard("and the more it got away from that", 205)...)
	// ...not said again but REPHRASED, and broken off: a short tail that ends
	// its take, with a full line of the take in front of it. The tail goes and
	// the full line stays (tailFragments). This used to be refused outright
	// as "part of something said once", and "...they forked, the project."
	// followed by thirty seconds of silence went into the video three runs
	// running on the strength of it.
	mid := []tsvRow{
		{s: 180, e: 193, src: "take4", text: "the sentence before it"},
		{s: 194, e: 196.4, src: "take4", text: "so they used on already existing project"},
		{s: 205, e: 208, src: "take5", text: "and the more it got away from that"},
	}
	got, refused, notes := trimToRepeat([]retake{{S: 194, E: 196.4, Again: 205}}, ws, mid)
	if len(got) != 1 || got[0].S != 194 || got[0].Again != 205 {
		t.Errorf("the broken-off tail was not marked: %+v (%v)", got, notes)
	}
	if len(refused) != 0 {
		t.Errorf("a mark that verified was handed back for a second hearing: %+v", refused)
	}
	// the model sweeping the full line in front of it into the mark does not
	// take the full line with it. (With another line of the take in front of
	// both: a mark that IS the whole take is a take that was redone, and that
	// goes whole -- wholeTake, and the test below.)
	more := append([]tsvRow{{s: 170, e: 179, src: "take4", text: "the line before that"}}, mid...)
	got, _, notes = trimToRepeat([]retake{{S: 180, E: 196.4, Again: 205}}, ws, more)
	if len(got) != 1 || got[0].S != 194 {
		t.Errorf("the full line before the fragment went with it: %+v (%v)", got, notes)
	}
	// and a tail that is a whole line -- twelve seconds of script -- is not a
	// fragment, so with nothing repeated it is refused and handed back
	long := []tsvRow{
		{s: 180, e: 193, src: "take4", text: "the sentence before it"},
		{s: 194, e: 206, src: "take4", text: "twelve seconds of a sentence that was only ever said once"},
		{s: 207, e: 210, src: "take5", text: "and the more it got away from that"},
	}
	lws := append(heard("twelve seconds of a sentence that was only ever said once", 194),
		heard("and the more it got away from that", 207)...)
	got, refused, notes = trimToRepeat([]retake{{S: 194, E: 206, Again: 207}}, lws, long)
	if len(got) != 0 || len(refused) != 1 {
		t.Errorf("a whole line not said again was cut on the model's word alone: %+v (%v)", got, notes)
	}
	if len(notes) == 0 || !strings.Contains(notes[0], "not said again") {
		t.Errorf("the refusal was not said out loud: %v", notes)
	}

	// with no words at all -- a project prepared before words.json was kept --
	// the answer stands as the model gave it rather than being thrown away
	got, _, _ = trimToRepeat([]retake{{S: 194, E: 196.4, Again: 205}}, nil, mid)
	if len(got) != 1 {
		t.Error("with no word timings to check against, the marks were dropped")
	}
}

// A take that holds nothing but the stretch is a take that failed: you started,
// it came out wrong, you stopped. There is nothing to compare it against and
// nothing to compare it with -- the next take says something else -- so the
// only evidence is the shape, and the shape is the whole of a recording being
// one broken-off attempt.
func TestATakeThatIsNothingButAFalseStartGoes(t *testing.T) {
	ws := append(heard("so they used on already existing project and as", 194),
		heard("and the more it got away from that the more confused", 205)...)
	// the marked stretch IS take4, end to end
	only := []tsvRow{
		{s: 100, e: 150, src: "take3", text: "everything before it"},
		{s: 194, e: 199.5, src: "take4", text: "so they used on already existing project and as"},
		{s: 205, e: 212, src: "take5", text: "and the more it got away from that"},
	}
	got, _, notes := trimToRepeat([]retake{{S: 194, E: 199.5, Again: 205}}, ws, only)
	if len(got) != 1 || got[0].S != 194 || got[0].Again != 0 {
		t.Fatalf("a take that is one false start was left in: %+v (%v)", got, notes)
	}
	if len(notes) == 0 || !strings.Contains(notes[0], "false start") {
		t.Errorf("the reason was not said out loud: %v", notes)
	}

	// ...and the same words with the rest of their take in front of them are
	// still a broken-off tail -- 5.5 s, ending the take, rephrased at 205 --
	// so they go too, and the retake they point at stays on the mark
	with := append([]tsvRow{{s: 180, e: 193, src: "take4", text: "the sentence this belongs to"}}, only...)
	with[1].src = "take4"
	if got, _, _ := trimToRepeat([]retake{{S: 194, E: 199.5, Again: 205}}, ws, with); len(got) != 1 || got[0].Again != 205 {
		t.Errorf("a broken-off tail with its take around it was left in: %+v", got)
	}

	// two fragments across two recordings -- a stop, a second try, a second
	// stop -- are two false starts in a row, and both go
	two := []tsvRow{
		{s: 194, e: 196, src: "take4", text: "so they used on"},
		{s: 197, e: 199.5, src: "take5", text: "already existing project and as"},
	}
	if got, _, _ := trimToRepeat([]retake{{S: 194, E: 199.5, Again: 205}}, ws, two); len(got) != 1 || got[0].S != 194 {
		t.Errorf("two false starts in a row were left in: %+v", got)
	}
}

// A mark with no replacement to point at is the one the code cannot check, so
// the shape has to carry it alone: a take that holds nothing else. Anything
// looser and a 13-second stretch spanning a seam gets dropped on the model's
// word, which is how a sentence leaves a video without anybody noticing.
func TestAnUncheckableMarkNeedsTheShapeToCarryIt(t *testing.T) {
	ws := heard("and put the app", 730)
	// spanning two takes: not one take's failure, so there is nothing to go on
	across := []tsvRow{
		{s: 705, e: 729, src: "take12", text: "the rest of that take"},
		{s: 730, e: 732, src: "take12", text: "and put the app"},
		{s: 734, e: 743, src: "take13", text: "and put the app on the phone"},
	}
	if got, _, notes := trimToRepeat([]retake{{S: 730, E: 743, Again: 0}}, ws, across); len(got) != 0 {
		t.Errorf("a 13 s drop across a seam was applied on the model's word: %+v (%v)", got, notes)
	}
	// a take that is nothing but the stretch still goes
	own := []tsvRow{
		{s: 705, e: 729, src: "take12", text: "the take before"},
		{s: 730, e: 732, src: "take13", text: "and put the app"},
		{s: 734, e: 743, src: "take14", text: "and put the app on the phone"},
	}
	if got, _, _ := trimToRepeat([]retake{{S: 730, E: 732, Again: 0}}, ws, own); len(got) != 1 {
		t.Error("a take that holds nothing but a false start was left in")
	}
	// and a mark on a full stop is not an attempt at anything
	if got, _, notes := trimToRepeat([]retake{{S: 762, E: 762.08, Again: 0}}, ws, own); len(got) != 0 {
		t.Errorf("a 0.08 s mark was applied: %+v (%v)", got, notes)
	}
}

// The floor applies to what the trim LEAVES, not only to what the model sent.
// A stutter of one clipped word is a tenth of a second; cutting a tenth of a
// second out of the middle of a sentence is a click, and the doubled word is
// the quieter of the two.
func TestATrimTooSmallToCutIsLeftIn(t *testing.T) {
	ws := append(heard("came mostly from the global south which", 681),
		heard("which seems to have more users", 707)...)
	// "which" alone is 0.35 s here, which is over the floor
	got, _, _ := trimToRepeat([]retake{{S: 681, E: 684, Again: 707}}, ws, nil)
	if len(got) != 1 {
		t.Fatalf("a trim worth making was refused: %+v", got)
	}
	// ...and the same word as the ASR often stamps it -- 80 ms -- is not, when
	// the retake follows it straight away: cutting a tenth of a second out of
	// the middle of a sentence is a click, and the doubled word is the quieter
	// of the two
	tiny := []srcWord{{s: 681, e: 683.6, w: "came"}, {s: 688.28, e: 688.36, w: "which"}}
	tiny = append(tiny, heard("which seems to have more users", 688.5)...)
	spoken := []tsvRow{{s: 681, e: 688.4, text: "came ... which"}, {s: 688.5, e: 691, text: "which seems to have more users"}}
	if got, _, notes := trimToRepeat([]retake{{S: 681, E: 688.4, Again: 688.5}}, tiny, spoken); len(got) != 0 {
		t.Errorf("an 80 ms splice was applied: %+v (%v)", got, notes)
	}
	// But the same 80 ms with the camera stopped after it is not a splice at
	// all: what goes is the eighteen seconds of nothing between the false
	// start and the retake, and the tenth of a second is the least of it
	// (goesWith). This is the shape that survived three runs of a real
	// session -- "...for Android, which" and then a man sitting in silence.
	far := append([]srcWord{{s: 681, e: 683.6, w: "came"}, {s: 688.28, e: 688.36, w: "which"}},
		heard("which seems to have more users", 707)...)
	gap := []tsvRow{{s: 681, e: 688.4, text: "came ... which"}, {s: 707, e: 710, text: "which seems to have more users"}}
	got, _, notes := trimToRepeat([]retake{{S: 681, E: 688.4, Again: 707}}, far, gap)
	if len(got) != 1 {
		t.Fatalf("eighteen seconds of silence were left in because the words in front of them are short: %v", notes)
	}
	if math.Abs(got[0].S-688.28) > 1e-9 {
		t.Errorf("the mark starts at %.2f, want 688.28 -- the false start, not the sentence before it", got[0].S)
	}
}

// An edge may never cross a word that stays.
//
// The sound used to place it alone, and it cannot tell a breath from a word it
// was never told about: where an attempt breaks off mid-sentence there is no
// camera stop and no real pause in front of it, so endBefore steps back over
// the quiet and lands inside the word before -- or past it. That is what turned
// "the previous public state of the art" into "state of" and "through apps, not
// a browser" into "through apps, not".
func TestARetakeEdgeNeverCrossesAWordThatStays(t *testing.T) {
	// "...state of the art | and the whole run took roughly thirteen" -- the
	// abandoned attempt starts at "and", and "art" ends a third of a second
	// before it, which is not a pause the envelope will find
	words := []srcWord{
		{s: 104.03, e: 104.67, w: "state"}, {s: 104.75, e: 104.91, w: "of"},
		{s: 104.99, e: 105.15, w: "the"}, {s: 105.15, e: 105.47, w: "art"},
		{s: 105.79, e: 105.95, w: "and"}, {s: 105.95, e: 106.03, w: "the"},
	}
	if got := lastWordEnd(words, 105.79); math.Abs(got-105.47) > 1e-9 {
		t.Errorf("the floor under the edge is %.2f, want 105.47 -- the end of %q", got, "art")
	}
	// the edge is allowed forward of the floor, never behind it
	for _, c := range []struct{ at, want float64 }{
		{105.79, 105.47}, // the abandoned word: back to the end of "art"
		{104.91, 104.91}, // already on a word edge: left alone
		{0, 0},           // before every word: nothing to stand on
	} {
		if got := lastWordEnd(words, c.at); math.Abs(got-c.want) > 1e-9 {
			t.Errorf("lastWordEnd(%.2f) = %.2f, want %.2f", c.at, got, c.want)
		}
	}
	// and the placement itself, with words to place by: the sound is not
	// consulted at all. A breath is sound, and the envelope keeps it on both
	// sides of every cut -- "after the sound before the abandoned word stops"
	// is after the breath, "where the retake starts to sound" is at the
	// breath. That was every deep breath heard at a join.
	a := &App{}
	spoken := []tsvRow{{s: 103, e: 109.2, src: "t3", text: "state of the art and the whole run"},
		{s: 114.08, e: 120, src: "t4", text: "and the whole run took roughly"}}
	words = append(words, srcWord{s: 114.08, e: 114.16, w: "and"})
	got, notes := a.placeEdges([]retake{{S: 105.79, E: 109.2, Again: 114.08, To: 109.2}}, nil, spoken, words)
	if math.Abs(got[0].S-(105.47+wordPad)) > 1e-9 {
		t.Errorf("the cut ends at %.3f, want %.3f -- a hair after %q (%v)", got[0].S, 105.47+wordPad, "art", notes)
	}
	if math.Abs(got[0].To-(114.08-wordPad)) > 1e-9 {
		t.Errorf("the cut resumes at %.3f, want %.3f -- a hair before the retake's %q (%v)", got[0].To, 114.08-wordPad, "and", notes)
	}
	body := funcBody(t, "retake.go", `func \(a \*App\) placeEdges\(`)
	if !strings.Contains(body, "if len(words) > 0 {") {
		t.Error("the envelope still has a say when there are words to place by")
	}
}

// The cut is told the stretch is gone, not that it is bad. Told only "not
// kept", a model helpfully ends its segment in front of the marker -- and its
// own boundary is a second coarser than the mark, so every marker cost a word
// off the sentence before it. dropMarked removes the stretch to the word
// whatever the cut answers, so the cut's job is to read past it.
func TestTheCutIsToldTheAbandonedStretchIsAlreadyGone(t *testing.T) {
	out := sessionText([]tsvRow{{s: 105, e: 108, spk: "SPEAKER_00", text: "the art"}},
		"", []retake{{S: 105, E: 109, Again: 114}})
	if !strings.Contains(out, "already removed, read straight past it") {
		t.Errorf("the marker still reads as a warning rather than a fact:\n%s", out)
	}
	for _, want := range []string{
		"has ALREADY been taken out of the video for you",
		"you never have to aim a boundary at one",
	} {
		if !strings.Contains(cutSystem, want) {
			t.Errorf("the cut is not told to read past a marker: want %q", want)
		}
	}
	// ...and it is told in the same breath that this is not permission to stop
	// cutting. The first wording said only "do not end a segment at one", and
	// the answer to the very next run was five segments end to end across the
	// whole session -- every silence and every dead end still in it.
	for _, want := range []string{
		"This does not change your job",
		"that run end to end over the whole session is not a cut",
	} {
		if !strings.Contains(cutSystem, want) {
			t.Errorf("the marker rule reads as leave to stop choosing: want %q", want)
		}
	}
}

// A take that breaks off part-way through a line is the commonest retake there
// is -- ten seconds of new material and then two words the next take starts
// over with -- and it was being read past every time, because marking a line
// that is 90%% unique looks wrong until you know the editor trims the mark down
// to the words that are actually said again (trimToRepeat).
func TestTheRetakePassIsToldToMarkATailRepeat(t *testing.T) {
	for _, want := range []string{
		"only its LAST FEW WORDS are said again",
		"the editor trims the mark down to exactly the words that are said again",
	} {
		if !strings.Contains(retakeSystem, want) {
			t.Errorf("the retake pass is not told about a tail repeat: want %q", want)
		}
	}
	// ...and the rule it used to be read past by now says what makes it one
	if !strings.Contains(retakeSystem, "with no pause and no seam in the middle of it") {
		t.Error(`"a phrase repeated inside one flowing sentence" still excuses a repeat with a pause in it`)
	}
}

// The pass is sampled, so one run's answer drifts against the next -- twelve
// marks, then ten, one inverted. Asked three times and pooled, a retake is
// found if any run found it; every mark is verified before it counts, so a
// wrong one is refused whichever run it came from.
func TestTheRetakePassIsAskedMoreThanOnceAndPooled(t *testing.T) {
	if retakeRuns < 3 {
		t.Errorf("the pass is asked %d time(s); one answer is one sample", retakeRuns)
	}
	body := funcBody(t, "retake.go", `func \(a \*App\) findRetakes\(`)
	for _, want := range []string{"for run := 0; run < retakeRuns; run++", "keepRetakes(spoken, found)", "mergeMarks(marks)"} {
		if !strings.Contains(body, want) {
			t.Errorf("the runs are not pooled: want %q", want)
		}
	}
	// three runs naming one stretch three ways come out as one mark
	got := mergeMarks([]retake{
		{S: 105.5, E: 109.2, Again: 114.1, To: 114.0},
		{S: 103.5, E: 109.2, Again: 114.1, To: 114.0}, // the line before it swept in
		{S: 106.0, E: 108.0, Again: 114.1, To: 114.0}, // a piece of it
		{S: 585.7, E: 591.1, Again: 594.2, To: 594.2}, // somewhere else entirely
	})
	if len(got) != 2 {
		t.Fatalf("three answers for one stretch came out as %d marks: %+v", len(got), got)
	}
	if got[0].S != 103.5 || got[0].E != 109.2 || got[0].To != 114.0 || got[1].S != 585.7 {
		t.Errorf("merged wrong: %+v", got)
	}
}
