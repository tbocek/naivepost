package main

// Retakes: the stretches you said, stopped, and said again.
//
// A session recorded off a script is full of them -- a sentence trails off, the
// camera is stopped, the next take starts the sentence again, sometimes a
// sentence or two further back. Both attempts are in the transcript and both
// are in the footage, so unless something says otherwise the finished video
// says everything twice, with the breath between.
//
// The cut pass was the wrong place to ask. It reads the timeline to choose
// MOMENTS -- what is worth keeping -- and it read a doubled sentence as
// content: one run's reasoning says "slight repetition, acceptable" in as many
// words. It is not a judgment about the video at all; it is a fact about what
// was said, true whoever cuts it and true before anyone does.
//
// So it is settled here, at the end of Prepare, and only here. That is the
// earliest point where it can be RIGHT, which is two conditions:
//
//   - the whole session is one stream. An attempt and its replacement are
//     usually in two different recordings -- stop, start again -- and anything
//     working per source (the ASR, the fixer's blocks) sees one half of the
//     pattern it is looking for.
//   - the text is already fixed. Two attempts at one sentence have to LOOK
//     alike to be read as one sentence twice, and raw ASR gives two different
//     manglings of it.
//
// What comes out is a marking, never a deletion: these seconds were abandoned,
// and this is where they were said again. The transcript keeps every word. The
// cut brief folds a marked stretch into one line so the model never chooses it,
// the toolbar can skip it, and a mark you disagree with costs you a glance.

import (
	"encoding/json"
	"fmt"
	"math"
	"os"
	"path/filepath"
	"sort"
	"strconv"
	"strings"
)

// retakeSystem is the pass's wording. One narrow job, said as a procedure:
// what a take is, which half of a doubled sentence goes, and the four things
// that look like a retake and are not.
//
// One paragraph or bullet per line, unwrapped: see describeSystem.
const retakeSystem = `You find the stretches a speaker said, broke off, and said again.

You are given every spoken line of one session, numbered, each with its own seconds, the pause in front of it, and the seams where one recording stops and the next begins. A session recorded off a script is made of takes: a sentence trails off or comes out wrong, the recording stops, and the next take says it again -- sometimes starting a sentence or two further back than where it went wrong.

Mark the ABANDONED attempt and never the one that was kept. The abandoned one is earlier, the kept one later. When a take starts further back than the mistake, everything it says again is abandoned: mark from the first line the later take repeats, not from the line that went wrong.

Mark the line even when only its LAST FEW WORDS are said again. A take often breaks off part-way through a line: ten seconds of new material and then two words that the next take starts over with. That line is abandoned from those two words on, and marking it is right -- the editor trims the mark down to exactly the words that are said again, and keeps everything before them. Two takes that end and begin on the same word or two are the commonest retake there is, and the easiest to read past.

These are NOT retakes, and marking one is worse than missing a real one:
- a sentence said twice on purpose, with no pause and no seam before it
- a phrase repeated inside one flowing sentence, with no pause and no seam in the middle of it
- anything said once
- anything not in the script. Going off script is the person talking, not a mistake, and it stays

The user context may carry the script. Where it does it says what was MEANT to be said, and it is a hint rather than the truth: it writes 42 where the speaker says "forty two", and a line matching nothing in it is improvisation and stays. Where there is no script, the pauses, the seams and the words themselves are the whole of the evidence.

A few words broken off and never picked up again is abandoned with nothing to point at: give "again": 0.

Answer with line NUMBERS, never with times.`

// retake is one abandoned stretch of the session: the seconds it covers, and
// where the good attempt starts (0 when it was simply broken off and never
// picked up again).
type retake struct {
	S, E  float64 // the abandoned words, first to last
	Again float64 // where the attempt that was kept begins, 0 for none
	// where what the cut REMOVES ends. E when the words are all that goes;
	// the retake's own onset when everything up to it goes with them -- the
	// camera stop, the breath, the run-up (placeEdges).
	To   float64
	Text string
}

const (
	// how many times the pass is asked, its answers pooled (findRetakes)
	retakeRuns = 3
	// how much room a cut leaves a word: the aligner's edges are within a
	// fiftieth of a second, and a word's last moment is quieter than its
	// middle, so a cut ON the edge clips it. Small, because what is on the
	// other side of it is the breath this is here to take out.
	wordPad = 0.08
	// the longest a line can be and still be a FRAGMENT: something broken off
	// rather than said. The broken-off tails of a scripted read run under two
	// seconds -- "the project.", "Browser for", "keep" -- and the one that ran
	// five was "so they used an already existing project, and". A full line
	// of the script is ten or twelve.
	retakeFrag = 6.0
	// how far apart an attempt and its replacement may be. A retake follows the
	// thing it replaces -- stop the camera, start it again, say the sentence --
	// and two similar sentences ten minutes apart are two sentences.
	retakeReach = 180.0
	// the most of a session's speech that may be called abandoned. A pass that
	// wants to drop half of what was said has not found retakes, it has lost
	// the plot, and the whole answer is refused rather than argued with.
	retakeCeil = 0.4
	// under this a mark is not an attempt at anything: a full stop of its own,
	// a breath the ASR gave a stamp to. Cutting it would open a hole nobody
	// asked for in the middle of a sentence.
	retakeMin = 0.3
	// a pause worth drawing in the brief. Measured, not guessed: on a session
	// of somebody reading to camera the gaps between lines cluster at 0.7 to
	// 1.1 s -- that is the ASR splitting a sentence and the speaker breathing,
	// 139 of 141 gaps -- and only six ran past 1.4. Drawn at 0.6 every line
	// wears one and the shape is lost in them; at this, a pause in the brief
	// means something happened.
	retakePause = 1.5
)

// retakeFile is where the marks are kept: beside the transcript they are about,
// so they travel with the project and are re-read rather than re-asked.
func (a *App) retakeFile() string { return filepath.Join(a.transcriptDir(), "retakes.tsv") }

// findRetakes is the pass. It runs on the merged timeline, writes the marks and
// returns them; a session with nothing to mark writes an empty file, which is
// the answer "asked, and there are none" rather than "never asked".
func (a *App) findRetakes(rows []tsvRow) ([]retake, error) {
	spoken := speechOnly(rows)
	if len(spoken) < 4 {
		return nil, a.writeRetakes(nil)
	}
	brief := retakeBrief(spoken, a.narratorMic())
	user := a.ctxBlockFor("retake") + fmt.Sprintf("WHAT WAS SAID, line by line:\n%s", brief)
	msgs := []map[string]any{msg("system", a.sysPrompt("retake")), msg("user", user)}
	// Asked MORE THAN ONCE, and every answer pooled. The call is sampled, and
	// one run's answer drifts against the next -- twelve marks, then ten, one
	// of them inverted -- so a retake missed is a retake missed by luck. It is
	// a short call, and every mark it names is verified below before it
	// counts (trimToRepeat), so pooling the runs costs seconds and risks
	// nothing: a wrong mark is refused whichever run it came from.
	var found []struct{ From, To, Again int }
	seen := map[[3]int]bool{}
	for run := 0; run < retakeRuns; run++ {
		var out struct {
			Abandoned []struct {
				From, To, Again int
			} `json:"abandoned"`
		}
		reply, err := a.llmChatRetry("retake", msgs, false)
		if err != nil {
			if run == 0 {
				return nil, err
			}
			a.logfIdle("!!! retakes: run %d: %v -- going on with %d", run+1, err, run)
			break
		}
		if p := jsonReply(reply, &out); p != "" {
			a.logfIdle("!!! retakes: run %d: %s -- its answer is set aside", run+1, p)
			continue
		}
		for _, x := range out.Abandoned {
			if k := [3]int{x.From, x.To, x.Again}; !seen[k] {
				seen[k] = true
				found = append(found, x)
			}
		}
	}
	marks, notes := keepRetakes(spoken, found)
	// ...and then the half the model cannot be taken at its word on: a retake
	// is usually the tail of a line, and dropping the whole line takes content
	// with it (trimToRepeat).
	vids, auds := a.snapSources()
	paths := append(vids, auds...)
	words := a.sessionWords(paths)
	trimmed, refused, more := trimToRepeat(marks, words, spoken)
	marks, notes = trimmed, append(notes, more...)
	// a second hearing for the ones the words refused: the transcript was
	// taken off a whole recording, and a short clip of the two attempts alone
	// often comes back as the same words twice (reHear)
	if len(refused) > 0 {
		heard, hn := a.reHear(refused, paths, spoken)
		marks, notes = append(marks, heard...), append(notes, hn...)
		sort.Slice(marks, func(i, j int) bool { return marks[i].S < marks[j].S })
	}
	// and then the gap between the attempt and the retake, which is not worth
	// a frame: the camera stop, the breath, the run-up. There are no words in
	// it to place a boundary by, so the sound places it (retake_edge.go) --
	// between the words on either side of it, and never across one.
	marks, more = a.placeEdges(marks, paths, spoken, words)
	notes = append(notes, more...)
	// three runs name the same stretch three ways -- one line, two lines, the
	// line and a half before it -- and after trimming they are one mark, or
	// two that overlap. One mark per stretch (mergeMarks).
	marks = mergeMarks(marks)
	for _, n := range notes {
		a.logfIdle(">>> retakes: %s", n)
	}
	total := retakeSecs(marks)
	if tooMuch(marks, spoken) {
		a.logfIdle("!!! retakes: %s of %s of speech called abandoned -- refused, nothing is marked",
			mmss(total), mmss(speechSecs(spoken)))
		return nil, a.writeRetakes(nil)
	}
	if len(marks) > 0 {
		a.logfIdle(">>> retakes: %d abandoned stretch(es), %s of speech, kept out of the cut",
			len(marks), mmss(total))
	} else {
		a.logfIdle(">>> retakes: none")
	}
	return marks, a.writeRetakes(marks)
}

// keepRetakes turns the reply's line numbers into marks, and drops the ones
// that cannot be true. The model names lines and never times, so a wrong answer
// is a wrong line rather than a second somewhere in the session -- and every
// line it names is checked back against the transcript here.
func keepRetakes(spoken []tsvRow, in []struct{ From, To, Again int }) ([]retake, []string) {
	var out []retake
	var notes []string
	// Overlaps are allowed through here. The answers of several runs are
	// pooled (findRetakes), and three runs name one stretch three ways; each
	// is verified on its own below and the survivors are folded into one
	// afterwards (mergeMarks). Refusing the later ones used to leave the
	// pool with nothing: one run's marks set a cursor at the last line of
	// the session, and every mark of every other run then read as overlap.
	for _, r := range in {
		f, t := r.From-1, r.To-1 // the brief numbers from 1
		if f < 0 || t < f || t >= len(spoken) {
			notes = append(notes, fmt.Sprintf("lines %d-%d are not lines of this timeline", r.From, r.To))
			continue
		}
		m := retake{S: spoken[f].s, E: spoken[t].e, To: spoken[t].e, Text: spoken[f].text}
		if g := r.Again - 1; g >= 0 {
			if g <= t {
				// the retake INSIDE the stretch it replaces. One run answers
				// every mark this way -- "again" set to the mark's own last
				// line -- and read as "never picked up" that is a claim the
				// model did not make, which then fails for the wrong reason
				notes = append(notes, fmt.Sprintf("lines %d-%d say they are said again at line %d, which is inside them -- not a mark",
					r.From, r.To, r.Again))
				continue
			}
			if g >= len(spoken) {
				notes = append(notes, fmt.Sprintf("lines %d-%d are said again at line %d, which is not a line", r.From, r.To, r.Again))
				continue
			}
			// the replacement has to follow the thing it replaces, and follow
			// it soon: a sentence said again three minutes later is a callback
			if spoken[g].s-m.E > retakeReach {
				notes = append(notes, fmt.Sprintf("lines %d-%d are said again %s later, too far to be a retake",
					r.From, r.To, mmss(spoken[g].s-m.E)))
				continue
			}
			m.Again = spoken[g].s
		}
		out = append(out, m)
	}
	return out, notes
}

// retakeBrief is what the pass reads: every spoken line numbered, with its own
// seconds, the pause in front of it, and the seam where one recording ends and
// the next begins.
//
// Those three are the whole of the signal. An attempt that trails off, a gap,
// a new take, the same sentence again -- said in the text, so the model reads
// a shape rather than inferring one from prose. The pauses come from the lines'
// own times and cost nothing: there is no need to look at the audio to know
// that nobody spoke for two seconds.
func retakeBrief(spoken []tsvRow, narr string) string {
	var b strings.Builder
	for i, r := range spoken {
		if i > 0 {
			if gap := r.s - spoken[i-1].e; gap >= retakePause {
				fmt.Fprintf(&b, "        (%.1fs pause)\n", gap)
			}
			if r.src != spoken[i-1].src {
				fmt.Fprintf(&b, "        --- the recording stops here; the next one begins ---\n")
			}
		}
		fmt.Fprintf(&b, "%4d  [%s-%s] %s: %s\n", i+1, mmssT(r.s), mmssT(r.e), tlLabel(r, narr), r.text)
	}
	return b.String()
}

// mmssT is a stamp with a tenth on it. The cut works in whole seconds because
// it chooses moments; this pass points at lines, and the tenth is there for the
// person reading the brief back against the waveform.
func mmssT(t float64) string {
	return fmt.Sprintf("%02d:%04.1f", int(t)/60, math.Mod(t, 60))
}

func speechOnly(rows []tsvRow) []tsvRow {
	var out []tsvRow
	for _, r := range rows {
		if r.spk != "EVENT" && strings.TrimSpace(r.text) != "" {
			out = append(out, r)
		}
	}
	return out
}

// tooMuch is whether an answer has stopped being a list of retakes and become
// a proposal to halve the session. A pass that wants most of the speech gone
// has not found attempts, it has lost its place in the numbering -- and one
// wrong line number early can carry every later one with it, so the whole
// answer is refused rather than argued with line by line.
func tooMuch(marks []retake, spoken []tsvRow) bool {
	said := speechSecs(spoken)
	return said > 0 && retakeSecs(marks) > said*retakeCeil
}

func retakeSecs(marks []retake) float64 {
	t := 0.0
	for _, m := range marks {
		t += m.E - m.S
	}
	return t
}

func speechSecs(rows []tsvRow) float64 {
	t := 0.0
	for _, r := range rows {
		t += r.e - r.s
	}
	return t
}

// writeRetakes stores the marks, and an empty file when there are none: the
// difference between "asked, none found" and "never asked" is what stops a
// reload from silently reading a stale answer from a run that predates this.
func (a *App) writeRetakes(marks []retake) error {
	var b strings.Builder
	for _, m := range marks {
		fmt.Fprintf(&b, "%.2f\t%.2f\t%.2f\t%.2f\t%s\n", m.S, m.E, m.Again, m.To, m.Text)
	}
	if err := os.MkdirAll(a.transcriptDir(), 0o755); err != nil {
		return err
	}
	return os.WriteFile(a.retakeFile(), []byte(b.String()), 0o644)
}

// loadRetakes reads them back. Missing is not an error: a project prepared
// before this existed has no file, and no marks is what it means.
func (a *App) loadRetakes() []retake {
	b, err := os.ReadFile(a.retakeFile())
	if err != nil {
		return nil
	}
	var out []retake
	for _, ln := range strings.Split(string(b), "\n") {
		f := strings.Split(ln, "\t")
		if len(f) < 3 {
			continue
		}
		s, e1 := strconv.ParseFloat(f[0], 64)
		e, e2 := strconv.ParseFloat(f[1], 64)
		ag, e3 := strconv.ParseFloat(f[2], 64)
		if e1 != nil || e2 != nil || e3 != nil {
			continue
		}
		m := retake{S: s, E: e, Again: ag, To: e}
		// a fifth column is where the removal ends; a file with four was
		// written before the edges were placed, and removes the words alone
		if len(f) > 4 {
			if to, err := strconv.ParseFloat(f[3], 64); err == nil && to > 0 {
				m.To = to
			}
			m.Text = f[4]
		} else if len(f) > 3 {
			m.Text = f[3]
		}
		out = append(out, m)
	}
	return out
}

// retakeAt is which marked stretch a line lies in, or -1.
func retakeAt(marks []retake, r tsvRow) int {
	mid := (r.s + r.e) / 2
	for i, m := range marks {
		if mid >= m.S && mid <= m.E {
			return i
		}
	}
	return -1
}

// retakeJSON is what a marked answer looks like, for the log and the tests.
func retakeJSON(marks []retake) string {
	b, _ := json.Marshal(marks)
	return string(b)
}

// ---- what was actually said again -------------------------------------------
//
// The pass answers in whole LINES, and a retake is rarely a whole line. You
// stop in the middle of a sentence and pick it up from the last few words, so
// the line the model marks holds both the words that were said again and the
// words that were not -- and dropping all of it takes real content out of the
// video, silently, in the middle of a sentence the cut keeps both sides of.
//
// One run marked eleven stretches. Five were clean, five held content that was
// never repeated, and one was not a retake at all: the sentence ran across a
// seam and only the word "which" was said twice.
//
// So the claim is checked against the words. What survives is the SUFFIX of the
// marked stretch that the later take says again -- the stutter and nothing else
// -- and a stretch with no repeated tail is not marked at all. The words are the
// ASR's own (words.json), which is where the times come from too, so this can
// be exact where the lines cannot: it cuts inside a line, at a word boundary.

// srcWord is one word of the session, on the session clock.
type srcWord struct {
	s, e float64
	w    string
	// the word as it is WRITTEN, case and punctuation kept: w is bared and
	// lowercased for matching, and matching is not reading. The subtitles are
	// built from these (wordCues), so they have to say "Eastern University"
	// and not "eastern university".
	raw string
	src string // the recording it was said in (baseName), for the seams
}

// againReach is how much of the later take is read for the repeat: a retake
// says the sentence again at its start, not a minute into it.
const againReach = 25.0

// trimToRepeat shrinks every mark to the words the later take repeats, and
// drops the ones it does not repeat at all. Marks with no replacement to check
// against (the model said it was broken off and never picked up) are left as
// they are: there is nothing to compare them with.
func trimToRepeat(marks []retake, words []srcWord, spoken []tsvRow) (out, refused []retake, notes []string) {
	for _, m := range marks {
		if goesWith(m, spoken) < retakeMin {
			notes = append(notes, fmt.Sprintf("%s-%s is too short to be an attempt at anything", mmss(m.S), mmss(m.E)))
			continue
		}
		if len(words) == 0 {
			out = append(out, m) // nothing to check it against; the answer stands
			continue
		}
		if m.Again <= 0 {
			// the pass says it was broken off and never picked up, and there is
			// nothing to verify that against. The shape has to carry it alone,
			// and the only shape that does is a take that holds nothing else
			// (wholeTake): a stretch with the rest of its take around it, or one
			// that runs across a seam, is part of something that was said once.
			if wholeTake(m, spoken) {
				out = append(out, m)
				continue
			}
			notes = append(notes, fmt.Sprintf("%s-%s is said to be broken off, but its take holds more than it -- left in",
				mmss(m.S), mmss(m.E)))
			continue
		}
		said := wordsIn(words, m.S, m.E)
		again := wordsIn(words, m.Again, m.Again+againReach)
		if len(said) == 0 || len(again) == 0 {
			out = append(out, m)
			continue
		}
		k := repeatFrom(said, again)
		if k < 0 {
			// not said again -- so the pass was wrong about where, and the
			// question becomes whether it was right about the stretch at all.
			//
			// A take that consists of NOTHING BUT this stretch is a take that
			// failed: you started, it came out wrong, you stopped. That is a
			// false start whatever the next take goes on to say, and it is the
			// one shape of abandonment there is nothing to compare against.
			//
			// A stretch with the rest of its take still around it is a
			// different thing: usually a sentence carried across the seam,
			// where dropping it takes a sentence out of the video that was
			// only ever said once.
			if wholeTake(m, spoken) {
				m.Again = 0
				out = append(out, m)
				notes = append(notes, fmt.Sprintf("%s-%s is the whole of its take and is not said again -- a false start (%q)",
					mmss(m.S), mmss(m.E), shortWords(said)))
				continue
			}
			// ...or a REPHRASE: the retake says the same thing in other
			// words, so nothing repeats and there is no word to trim to. What
			// there is instead is the shape of the stop: the mark ends in
			// fragments -- a line or two too short to be anything but broken
			// off -- and the take ends right after them. Those go; the full
			// lines the model swept up in front of them stay (tailFragments).
			// "...they forked, the project." and then thirty seconds of
			// silence is this, and it went into the video three runs running.
			if s := tailFragments(m, spoken); s >= 0 {
				if s > m.S {
					notes = append(notes, fmt.Sprintf("%s-%s trimmed to %s: not said again, but that is where it breaks off",
						mmss(m.S), mmss(m.E), mmss(s)))
					m.S = s
				}
				if goesWith(m, spoken) < retakeMin {
					continue
				}
				out = append(out, m)
				continue
			}
			notes = append(notes, fmt.Sprintf("%s-%s is not said again at %s -- left in (%q)",
				mmss(m.S), mmss(m.E), mmss(m.Again), shortWords(said)))
			refused = append(refused, m)
			continue
		}
		if k > 0 {
			notes = append(notes, fmt.Sprintf("%s-%s trimmed to %s: only %q is said again",
				mmss(m.S), mmss(m.E), mmss(said[k].s), shortWords(said[k:])))
			m.S = said[k].s
		}
		// ...and the floor again, on what the trim left. A stutter of one
		// clipped word is a tenth of a second, and taking a tenth of a second
		// out of the middle of a sentence is a click rather than an edit: the
		// doubled word is less audible than the splice that would remove it.
		if goesWith(m, spoken) < retakeMin {
			notes = append(notes, fmt.Sprintf("%s-%s is all that is said again -- too short to cut cleanly, left in",
				mmss(m.S), mmss(m.E)))
			continue
		}
		out = append(out, m)
	}
	return out, refused, notes
}

// wholeTake is whether a marked stretch is all the speech its recording holds.
//
// Nothing was said in that take but this, so the take is the attempt: it began,
// it went wrong, it stopped. The seam either side is what makes it readable as
// one -- a stretch with more of its own take around it is part of something,
// and what it is part of was said once.
// mergeMarks folds marks that overlap into one: the earliest start, the latest
// end, and the retake the later of them points at. Sorted on the way.
func mergeMarks(marks []retake) []retake {
	sort.Slice(marks, func(i, j int) bool { return marks[i].S < marks[j].S })
	var out []retake
	for _, m := range marks {
		if n := len(out); n > 0 && m.S <= math.Max(out[n-1].E, out[n-1].To)+0.01 {
			p := &out[n-1]
			p.E = math.Max(p.E, m.E)
			p.To = math.Max(p.To, m.To)
			if m.Again > p.Again {
				p.Again = m.Again
			}
			if p.Text == "" {
				p.Text = m.Text
			}
			continue
		}
		out = append(out, m)
	}
	return out
}

// tailFragments is where a mark's broken-off tail begins, or -1 when it has
// none: the mark has to END its take -- that is the stop -- and the lines at
// the end of it have to be fragments (retakeFrag). Walked back from the end,
// so a full line the model marked in front of the fragments is not one.
func tailFragments(m retake, spoken []tsvRow) float64 {
	var in []tsvRow
	for _, r := range spoken {
		if mid := (r.s + r.e) / 2; mid >= m.S && mid <= m.E {
			in = append(in, r)
		}
	}
	if len(in) == 0 || !endsTake(in[len(in)-1], spoken) {
		return -1
	}
	i := len(in)
	for i > 0 && in[i-1].e-in[i-1].s < retakeFrag {
		i--
	}
	if i == len(in) {
		return -1 // the last thing said is a whole line, not a fragment
	}
	return in[i].s
}

// endsTake is whether r is the last thing said in its recording.
func endsTake(r tsvRow, spoken []tsvRow) bool {
	for _, o := range spoken {
		if o.src == r.src && o.s > r.s {
			return false
		}
	}
	return true
}

func wholeTake(m retake, spoken []tsvRow) bool {
	src, n, in := "", 0, 0
	for _, r := range spoken {
		mid := (r.s + r.e) / 2
		if mid < m.S || mid > m.E {
			continue
		}
		if src == "" {
			src = r.src
		} else if r.src != src {
			return false // it spans two recordings: not one take's failure
		}
		in++
	}
	if src == "" {
		return false
	}
	for _, r := range spoken {
		if r.src == src {
			n++
		}
	}
	return in == n
}

// repeatFrom is where the repeated tail of said begins: the earliest word from
// which what follows is said again in again. -1 when none of it is.
//
// Earliest, not longest: the whole stutter goes, not the part of it that
// matches best.
//
// Anchored, then tolerant. The tail is looked for anywhere in the later take --
// a retake does not always resume at its first word -- and once a run is going,
// a word that does not match is stepped over rather than ending it. Both are
// needed and neither alone is enough: without the anchor a repeat that starts
// three words in is never found, and without the tolerance one word ends the
// match.
//
// It has to be tolerant because the two attempts are never the same text. These
// are raw ASR words, and the same phrase came back as "and the tor project keep
// hearing" one time and "and the tour project kept hearing" the next -- two
// words, one edit each, in a phrase of six. sameWord is what forgives that.
func repeatFrom(said, again []srcWord) int {
	for k := range said {
		left := said[k:]
		for a := range again {
			if sameWord(left[0].w, again[a].w) && runFrom(left, again, a) >= int(math.Ceil(0.7*float64(len(left)))) {
				return k
			}
		}
	}
	return -1
}

// runFrom counts how much of left turns up in again, in order, starting at a.
// A word that is not there is stepped over: the next one may still be, and one
// word heard differently must not end the match.
func runFrom(left, again []srcWord, a int) int {
	j, hit := a, 0
	for _, w := range left {
		for d := 0; d <= repeatSkip; d++ {
			if j+d < len(again) && sameWord(w.w, again[j+d].w) {
				hit, j = hit+1, j+d+1
				break
			}
		}
	}
	return hit
}

// repeatSkip is how many words of the later take may be stepped over to find
// the next one. Enough for a word or two said differently; not enough to walk
// the rest of the take looking for a common word.
const repeatSkip = 3

// sameWord is whether two heard words are the same word.
//
// The same, one a prefix of the other -- which is what an attempt broken off
// mid-word looks like beside the one that finishes it, "sub" against "subject"
// -- or one edit apart, which is what one microphone and one model make of the
// same sound twice: "tor" and "tour", "keep" and "kept". Short words are
// compared exactly: at two letters every rule above matches half the language.
// bareWord is a word with the punctuation around it taken off, lowercased, so
// that "again," and "Again" are the same word.
func bareWord(s string) string {
	return strings.ToLower(strings.TrimFunc(s, func(r rune) bool {
		return !('a' <= r && r <= 'z' || 'A' <= r && r <= 'Z' || '0' <= r && r <= '9' || r == '\'')
	}))
}

func sameWord(a, b string) bool {
	if a == b {
		return true
	}
	if len(a) < 3 || len(b) < 3 {
		return false
	}
	if strings.HasPrefix(a, b) || strings.HasPrefix(b, a) {
		return true
	}
	return oneEdit(a, b)
}

// oneEdit is whether a and b are at most one insertion, deletion or
// substitution apart.
func oneEdit(a, b string) bool {
	if len(a) < len(b) {
		a, b = b, a
	}
	if len(a)-len(b) > 1 {
		return false
	}
	i, j, seen := 0, 0, false
	for i < len(a) && j < len(b) {
		if a[i] == b[j] {
			i, j = i+1, j+1
			continue
		}
		if seen {
			return false
		}
		seen = true
		if len(a) > len(b) {
			i++
		} else {
			i, j = i+1, j+1
		}
	}
	return !(seen && (len(a)-i > 0 || len(b)-j > 0))
}

func wordsIn(words []srcWord, t0, t1 float64) []srcWord {
	var out []srcWord
	for _, w := range words {
		if w.s >= t0-0.01 && w.e <= t1+0.01 {
			out = append(out, w)
		}
	}
	return out
}

func shortWords(ws []srcWord) string {
	var b []string
	for _, w := range ws {
		b = append(b, w.w)
	}
	s := strings.Join(b, " ")
	if len(s) > 60 {
		s = s[:57] + "..."
	}
	return s
}

// ---- the words themselves ----------------------------------------------------

// sessionWords is every word the ASR heard, on the session clock.
//
// words.json is the server's own answer, kept as it came (pipeline.go): one
// entry per token with its sample range, at sampleRate. Tokens are pieces of
// words -- "wa", "lle", "t" -- and a leading space is what starts a new one, so
// they are glued back together here. Punctuation-only tokens carry no sound of
// their own and are attached to the word before them without moving its end:
// left in, a comma's stamp lands in the silence after the sentence and makes
// every last word of a sentence look a second longer than it is.
func (a *App) sessionWords(paths []string) []srcWord {
	// on the SESSION clock, like everything the marks are measured in.
	// sourceStart is a wall clock -- seconds since the epoch -- and the one
	// place that difference shows up is here, where a word at 1.789e9 and a
	// mark at 99.1 simply never meet: nothing matched, nothing was verified,
	// and every mark came through exactly as the model wrote it.
	at, zero := srcClock(paths)
	var out []srcWord
	for _, p := range paths {
		base := baseName(p)
		dir := filepath.Join(a.inputsDir(), base)
		mine := glueWords(wordTimes(dir), at[p]-zero)
		dressWords(mine, readFileString(filepath.Join(dir, "transcript.txt")))
		for _, w := range mine {
			w.src = base
			if w.raw == "" {
				w.raw = w.w
			}
			out = append(out, w)
		}
	}
	sort.Slice(out, func(i, j int) bool { return out[i].s < out[j].s })
	return out
}

// dressWords gives each word its spelling back: case and punctuation, off the
// transcript the aligner was handed.
//
// The aligner answers in bare lowercase -- it is matching sound to text and
// hands back what it matched rather than what it was given -- so the times are
// right and the writing is gone: "welcome back this is a weekly summary" for a
// transcript that reads "Welcome back. This is a weekly summary". The
// subtitles are built from these words (wordCues), and a subtitle is read.
//
// The transcript IS what the aligner was given, word for word and in order, so
// the two are walked together. A word the walk cannot place keeps the bare
// form: a caption missing a capital is a smaller fault than one missing a word.
func dressWords(words []srcWord, text string) {
	fields := strings.Fields(text)
	j := 0
	for i := range words {
		for k := j; k < len(fields) && k-j < dressReach; k++ {
			if bareWord(fields[k]) == words[i].w {
				words[i].raw = fields[k]
				j = k + 1
				break
			}
		}
	}
}

// dressReach is how far ahead in the transcript a word is looked for: the
// aligner drops one here and there, and past a few the walk has lost its place
// and every word after it would be dressed in the wrong one.
const dressReach = 8

// asrToken is one entry of the server's words list.
type asrToken struct {
	Word  string `json:"word"`
	Start int64  `json:"start_sample"`
	End   int64  `json:"end_sample"`
}

// glueWords turns a model's tokens into words at off seconds.
//
// Two conventions are in play and they cannot be told apart token by token.
// Nemotron answers in PIECES with a leading space on the first of each word --
// " wa", "lle", "t" -- and a token of nothing but space where a word begins;
// drop that space without remembering it and the word glues onto the one
// before. Qwen and the aligner answer in whole words with no leading space
// anywhere, and the same rule welds a whole take into a single word: the first
// two measurements of this taken by hand came out as "1 word", which is what
// sent me looking.
//
// So the stream says which it is. A leading space anywhere means pieces; none
// means every token is a word.
func glueWords(toks []asrToken, off float64) []srcWord {
	pieces := false
	for _, w := range toks {
		if strings.HasPrefix(w.Word, " ") {
			pieces = true
			break
		}
	}
	var out []srcWord
	brk := true
	for _, w := range toks {
		bare := bareWord(w.Word)
		raw := strings.TrimSpace(w.Word)
		t0, t1 := off+float64(w.Start)/sampleRate, off+float64(w.End)/sampleRate
		if bare == "" {
			// a comma or a full stop is text and not sound, but it is still
			// text: it belongs to the word in front of it, which is what the
			// subtitles read back. A space is the boundary before the next.
			if raw != "" && len(out) > 0 && !brk {
				out[len(out)-1].raw += raw
			}
			brk = brk || strings.TrimSpace(w.Word) == ""
			continue
		}
		if !pieces || strings.HasPrefix(w.Word, " ") || brk || len(out) == 0 {
			out = append(out, srcWord{s: t0, e: t1, w: bare, raw: raw})
			brk = false
			continue
		}
		out[len(out)-1].e = t1
		out[len(out)-1].w += bare
		out[len(out)-1].raw += raw
	}
	return out
}

// edgeLookup is the envelope under a session second, loaded once per source:
// which recording is under t is the timeline's to say (srcOf), and the
// recording's envelope is read or built on the first ask (loadEdges).
func (a *App) edgeLookup(paths []string, spoken []tsvRow) func(t float64) *edges {
	at, zero := srcClock(paths)
	byBase := map[string]string{}
	for _, p := range paths {
		byBase[baseName(p)] = p
	}
	env := map[string]*edges{}
	return func(t float64) *edges {
		base := srcOf(spoken, t)
		p, ok := byBase[base]
		if !ok {
			return nil
		}
		if e, ok := env[p]; ok {
			return e
		}
		env[p] = a.loadEdges(p, at[p]-zero)
		return env[p]
	}
}

// goesWith is how much of the session this mark actually takes out, which is
// not the same as how long the abandoned words are.
//
// A false start is often ONE word -- "...for Android, which" -- and then the
// camera stops and eighteen seconds of nothing follow before the retake says
// "which seems to have". Measured on the words that is a tenth of a second and
// refused as too small to cut cleanly; measured on what goes it is eighteen
// seconds of a man sitting in silence, and the tenth of a second is the least
// of what the cut is for. So the floor is applied to the removal, which runs
// to the retake's onset wherever there is nothing said in between.
func goesWith(m retake, spoken []tsvRow) float64 {
	if m.Again > m.E && !wordsBetween(spoken, m.E, m.Again) {
		return m.Again - m.S
	}
	return m.E - m.S
}

// lastWord is the last word that ends at or before t.
func lastWord(words []srcWord, t float64) (srcWord, bool) {
	var out srcWord
	ok := false
	for _, w := range words {
		if w.e <= t+0.01 && (!ok || w.e > out.e) {
			out, ok = w, true
		}
	}
	return out, ok
}

// wordAt is the word that starts at t, if one does.
func wordAt(words []srcWord, t float64) (srcWord, bool) {
	for _, w := range words {
		if math.Abs(w.s-t) < 0.01 {
			return w, true
		}
	}
	return srcWord{}, false
}

// lastWordEnd is where the last word that ends at or before t ends, or 0 when
// there is none. The floor under an edge: what is before it was said once and
// stays, and an edge that crosses it takes a word out of the video with it.
func lastWordEnd(words []srcWord, t float64) float64 {
	out := 0.0
	for _, w := range words {
		if w.e <= t+0.01 && w.e > out {
			out = w.e
		}
	}
	return out
}

// srcOf is which recording a session second falls in, by the lines around it,
// and where that recording starts. "" when no line is near.
func srcOf(spoken []tsvRow, t float64) string {
	best, d := "", 1e9
	for _, r := range spoken {
		if r.s <= t && t <= r.e {
			return r.src
		}
		if x := math.Min(math.Abs(r.s-t), math.Abs(r.e-t)); x < d {
			best, d = r.src, x
		}
	}
	if d > 30 {
		return ""
	}
	return best
}

// placeEdges moves every mark's two edges onto the sound (retake_edge.go): the
// removal starts where the last kept sound ends, and -- when there is a retake
// and nothing else was said before it -- ends where the retake's sound begins.
func (a *App) placeEdges(marks []retake, paths []string, spoken []tsvRow, words []srcWord) ([]retake, []string) {
	edgeOf := a.edgeLookup(paths, spoken)
	var notes []string
	for i := range marks {
		m := &marks[i]
		if len(words) > 0 {
			// The WORDS fence both edges, and the sound chooses inside the
			// fence (endAfter, startBefore).
			//
			// The envelope alone used to place them, for ASR stamps that ran
			// half a second late -- and it cannot tell a breath from a word.
			// A breath is sound: asked where the sound before the abandoned
			// word stops it answered "after the breath", and the breath was
			// kept; that was every deep breath heard at a join. Now the
			// aligner's edges say which words the cut may touch -- none --
			// and between the last word that stays and the first that was
			// abandoned the sound only says where the quietest moment is,
			// with the word's own tail followed first so a trailing s is not
			// shaved. Everything between the two edges goes.
			if w, ok := lastWord(words, m.S); ok && w.e < m.S {
				s := math.Min(m.S, w.e+wordPad)
				if e := edgeOf(w.e); e != nil {
					s = e.endAfter(w, m.S)
				}
				notes = append(notes, fmt.Sprintf("%s: the cut ends %.2fs earlier, after the last word that stays", mmss(m.S), m.S-s))
				m.S = s
			}
			if m.Again > m.E && len(wordsIn(words, m.E, m.Again)) == 0 {
				// nothing said between: it all goes, up to the retake's own
				// first word
				to := math.Max(m.E, m.Again-wordPad)
				if w, ok := wordAt(words, m.Again); ok {
					if e := edgeOf(m.Again); e != nil {
						to = e.startBefore(w, m.E)
					}
				}
				if to > m.To {
					notes = append(notes, fmt.Sprintf("%s: the cut resumes at %s, just before the retake's first word", mmss(m.S), mmss(to)))
					m.To = to
				}
			}
			continue
		}
		// no word times: the sound is all there is to place an edge by
		if e := edgeOf(m.S); e != nil {
			if s := e.endBefore(m.S); s != m.S {
				notes = append(notes, fmt.Sprintf("%s: the cut ends %.2fs earlier, where the sound before it stops", mmss(m.S), m.S-s))
				m.S = s
			}
		}
		if m.Again > 0 && !wordsBetween(spoken, m.E, m.Again) {
			to := m.Again
			if e := edgeOf(m.Again); e != nil {
				to = math.Min(math.Max(e.startAt(m.Again), m.E), m.Again)
			}
			if to > m.To {
				notes = append(notes, fmt.Sprintf("%s: the cut resumes at %s, where the retake starts to sound", mmss(m.S), mmss(to)))
				m.To = to
			}
		}
	}
	return marks, notes
}

// reHear asks the audio server about the two attempts alone, and applies the
// marks it can then verify.
//
// The transcript was taken off a whole recording, and one word heard
// differently in the two attempts -- "stay safe" against "stay unsafe" -- is
// enough for the word check to refuse a real retake. A clip of a few seconds
// holding just the one attempt gives the model nothing to segment across, and
// it usually comes back as the same words both times. Only the refused marks
// are asked about, and only when there is a retake to compare against; a few
// short calls, not a second transcription.
func (a *App) reHear(refused []retake, paths []string, spoken []tsvRow) ([]retake, []string) {
	var out []retake
	var notes []string
	if err := a.ensureAudioServer(); err != nil {
		return nil, []string{fmt.Sprintf("no second hearing: %v", err)}
	}
	at, zero := srcClock(paths)
	byBase := map[string]string{}
	for _, p := range paths {
		byBase[baseName(p)] = p
	}
	tmp, err := os.MkdirTemp("", "naivepost-rehear-*")
	if err != nil {
		return nil, []string{fmt.Sprintf("no second hearing: %v", err)}
	}
	defer os.RemoveAll(tmp)
	hear := func(t0, t1 float64, name string) []srcWord {
		p, ok := byBase[srcOf(spoken, t0)]
		if !ok {
			return nil
		}
		off := at[p] - zero
		wav := filepath.Join(tmp, name+".wav")
		if err := a.runCmd(ffTool("ffmpeg"), "-v", "error", "-y", "-ss", fmt.Sprintf("%.2f", t0-off),
			"-t", fmt.Sprintf("%.2f", t1-t0), "-i", p, "-vn", "-ac", "1", "-ar", "16000",
			"-c:a", "pcm_s16le", wav); err != nil {
			return nil
		}
		body, _, err := a.asrJSON(wav)
		if err != nil {
			return nil
		}
		var doc struct {
			Words []asrToken `json:"words"`
		}
		if json.Unmarshal(body, &doc) != nil {
			return nil
		}
		return glueWords(doc.Words, t0)
	}
	for i, m := range refused {
		if m.Again <= 0 {
			continue
		}
		said := hear(m.S-0.3, m.E+0.3, fmt.Sprintf("said%d", i))
		again := hear(m.Again-0.3, m.Again+againReach, fmt.Sprintf("again%d", i))
		if len(said) == 0 || len(again) == 0 {
			notes = append(notes, fmt.Sprintf("%s-%s heard again: nothing came back -- left in", mmss(m.S), mmss(m.E)))
			continue
		}
		k := repeatFrom(said, again)
		if k < 0 {
			notes = append(notes, fmt.Sprintf("%s-%s heard again as %q, still not said again -- left in",
				mmss(m.S), mmss(m.E), shortWords(said)))
			continue
		}
		if said[k].s > m.S {
			m.S = said[k].s
		}
		if m.E-m.S < retakeMin {
			continue
		}
		notes = append(notes, fmt.Sprintf("%s-%s heard again: %q is said again after all -- cut",
			mmss(m.S), mmss(m.E), shortWords(said[k:])))
		out = append(out, m)
	}
	return out, notes
}
