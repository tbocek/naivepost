package main

// Text edit: the cut of a read to camera, made as a text edit.
//
// A session recorded off a script is the speech and nothing else -- every word
// goes into the video except the mistakes in the saying: an attempt said
// again, a false start, a fragment left hanging. So the finished video is a
// SUBSEQUENCE of what was spoken, and choosing it is choosing words, not
// seconds. The model is handed the words and answers with the words; it never
// sees a clock. The cut follows from the aligner's times for the words that
// survive, and every boundary is a word edge by construction.
//
// That replaces the retake pass for this kind of session (styleRead), and the
// picture is never described: nothing about which words survive is in the
// picture, and describing it was the longest part of Prepare.

import (
	"fmt"
	"math"
	"os"
	"path/filepath"
	"regexp"
	"strings"
)

// the video styles, as the project stores them. Blank is the ordinary answer
// and every project written before this reads as it.
const (
	styleMoments = ""     // a session: the cut picks the moments, with the picture described
	styleRead    = "read" // a read to camera: the words are the video
)

// the two, as the page names them: Lecture first, since it is the one this
// editor is mostly for. The stored words stay what they were (styleRead,
// styleMoments), so a project written before the rename still reads.
var styleLabels = []string{"Lecture", "Gaming"}

// styleOf is the stored style for a dropdown index, and styleIndex the other
// way: Lecture is the read to camera, Gaming the session whose moments are
// picked.
func styleOf(i uint) string {
	if i == 0 {
		return styleRead
	}
	return styleMoments
}

func styleIndex(name string) uint {
	if name == styleRead {
		return 0
	}
	return 1
}

// textSystem is the pass's wording.
const textSystem = `You edit what was said down to what the finished video says.

You are given every word spoken in one session, in order, lowercase and without punctuation, as the speech recognizer heard it. A line break is a breath. "SEAM" is where one recording stops and the next begins. "(pause 4.6s)" is that much silence. The user context may carry the script: what was MEANT to be said. It is a hint and not the truth -- it writes 42 where the speaker says forty two, and a sentence not in it is the person talking, which stays.

Answer with the text of the finished video, made from the words you were given by REMOVING WORDS ONLY. Never add a word, never change one, never reorder them; keep the spelling you were given. Leave the markers out.

Remove:
- an attempt that was said again. The earlier one goes and the later one stays, always. When the later take starts further back than the mistake, everything it says again goes with it.
- a false start: a few words broken off and never finished.
- a fragment left hanging before a seam or a long pause, that the words after it do not carry on.

Keep everything else, off-script included: a word said once is in the video. If nothing was said twice and nothing broke off, answer with every word you were given.`

// findTextEdit is the pass: the words in, the words out, the marks between.
func (a *App) findTextEdit(rows []tsvRow) ([]retake, error) {
	vids, auds := a.snapSources()
	words := a.spokenWords(append(vids, auds...))
	if len(words) < 4 {
		return nil, a.writeRetakes(nil)
	}
	user := a.ctxBlockFor("textedit") + "WHAT WAS SAID:\n" + textBrief(words)
	msgs := []map[string]any{msg("system", a.sysPrompt("textedit")), msg("user", user)}
	reply, err := a.llmChatRetry("textedit", msgs, false)
	if err != nil {
		return nil, err
	}
	// the text itself, beside the marks: it is the edit, and a person can
	// read it -- or change it and cut again (marksFromText). Written first,
	// so the marks are always the newer file of the two.
	if err := os.WriteFile(a.finalText(), []byte(strings.TrimSpace(reply)+"\n"), 0o644); err != nil {
		return nil, err
	}
	marks, ok := a.marksOfText(reply, words, rows, append(vids, auds...))
	if !ok {
		return nil, a.writeRetakes(nil)
	}
	return marks, a.writeRetakes(marks)
}

// finalText is where the edit lives: the words of the finished video.
func (a *App) finalText() string { return filepath.Join(a.transcriptDir(), "final.txt") }

// marksOfText is the mechanical half of the pass: the text matched back to
// the words, checked, and the dropped runs made into marks with their edges
// placed. false when the text is refused as an edit.
func (a *App) marksOfText(text string, words []srcWord, rows []tsvRow, paths []string) ([]retake, bool) {
	kept, extra := keepMask(words, textTokens(text))
	// ...and "keep the later" once more, at every join: a word left dangling
	// on one side of a cut that the other side begins with is the same word
	// twice in the video -- "...existing project and | and the more..." --
	// whatever the model meant by leaving it
	for _, n := range dedupeJoins(words, kept) {
		a.logfIdle(">>> text edit: %s", n)
	}
	if extra > 0 {
		a.logfIdle("!!! text edit: %d word(s) in the text were never said -- left out, there is no sound for them", extra)
	}
	dropped := 0
	for _, k := range kept {
		if !k {
			dropped++
		}
	}
	if float64(dropped) > retakeCeil*float64(len(words)) {
		a.logfIdle("!!! text edit: %d of %d words removed -- refused, that is not an edit, nothing is marked", dropped, len(words))
		return nil, false
	}
	// the runs that went, then their edges: fenced by the words either side,
	// placed by the sound between them (placeEdges)
	marks, notes := a.placeEdges(marksFrom(words, kept), paths, rows, words)
	for _, n := range notes {
		a.logfIdle(">>> text edit: %s", n)
	}
	marks = mergeMarks(marks)
	for _, m := range marks {
		a.logfIdle(">>> text edit: %s-%s goes (%q)", mmss(m.S), mmss(m.To), m.Text)
	}
	a.logfIdle(">>> text edit: %d of %d words removed in %d stretch(es)", dropped, len(words), len(marks))
	return marks, true
}

// marksFromText rebuilds the marks from a final.txt edited BY HAND: when the
// text is newer than the marks, the words in it are the edit and the marks
// are remade from them, no model asked. Delete a word in the file, save, press
// Cut, and it is out of the video -- which is the whole point of the text
// being a file. false when there is nothing newer to read.
func (a *App) marksFromText() ([]retake, bool) {
	ti, err := os.Stat(a.finalText())
	if err != nil {
		return nil, false
	}
	if mi, err := os.Stat(a.retakeFile()); err == nil && !ti.ModTime().After(mi.ModTime()) {
		return nil, false
	}
	text, err := os.ReadFile(a.finalText())
	if err != nil {
		return nil, false
	}
	vids, auds := a.snapSources()
	paths := append(vids, auds...)
	words := a.spokenWords(paths)
	if len(words) == 0 {
		return nil, false
	}
	a.logf(">>> final.txt was edited after Prepare -- the marks are remade from it, no model asked")
	marks, ok := a.marksOfText(string(text), words, a.sessionRows(), paths)
	if !ok {
		return nil, false
	}
	if err := a.writeRetakes(marks); err != nil {
		a.logf("!!! could not write the marks: %v", err)
	}
	return marks, true
}

// spokenWords is the session's words that the video plays: everything but the
// narrator's own microphone, which the video never plays (tlLabel).
func (a *App) spokenWords(paths []string) []srcWord {
	narr := a.narratorMic()
	var out []srcWord
	for _, w := range a.sessionWords(paths) {
		if narr == "" || w.src != narr {
			out = append(out, w)
		}
	}
	return out
}

// textBrief is the words as the model reads them: one line per breath, a
// SEAM where the recording changes, and the long pauses said in seconds.
func textBrief(words []srcWord) string {
	var b strings.Builder
	for i, w := range words {
		if i > 0 {
			p := words[i-1]
			switch gap := w.s - p.e; {
			case w.src != p.src:
				b.WriteString("\nSEAM\n")
			case gap >= retakePause:
				fmt.Fprintf(&b, "\n(pause %.1fs)\n", gap)
			case gap >= mergeGap:
				b.WriteString("\n")
			default:
				b.WriteString(" ")
			}
		}
		b.WriteString(w.w)
	}
	return b.String()
}

// textTokens is the answer as words: whatever it was wrapped in, bare and
// lowercase like the words it was made from, with the markers left out.
func textTokens(reply string) []string {
	var out []string
	reply = pauseMark.ReplaceAllString(reply, " ")
	for _, f := range strings.Fields(reply) {
		if f == "SEAM" {
			continue
		}
		if w := bareWord(f); w != "" {
			out = append(out, w)
		}
	}
	return out
}

// pauseMark is the pause marker as the brief writes it, echoed or not.
var pauseMark = regexp.MustCompile(`\(pause [0-9.]+s\)`)

// keepMask is which of the words the answer kept, and how many of the
// answer's words were never said.
//
// Matched from the END backwards, on purpose. The same words occur twice
// wherever there was a retake, and an answer that keeps them once could be
// matched to either; walked forwards it would take the earlier -- the
// abandoned attempt -- every time. Walked backwards it takes the later, which
// is the one that stays, and the rule "prefer the second" is not a request
// to the model but the shape of the match.
func keepMask(words []srcWord, out []string) ([]bool, int) {
	kept := make([]bool, len(words))
	extra := 0
	i := len(words) - 1
	for j := len(out) - 1; j >= 0; j-- {
		k := i
		for ; k >= 0 && i-k < keepReach && words[k].w != out[j]; k-- {
		}
		if k < 0 || i-k >= keepReach {
			extra++ // nothing said matches this: it was not said
			continue
		}
		kept[k] = true
		i = k - 1
	}
	return kept, extra
}

// keepReach is how many words back the match may look for the answer's next
// word: past it, the word was not said and the answer moves on. A dropped
// stretch is a sentence or two, and two hundred words is a minute of speech.
const keepReach = 200

// dedupeJoins drops the kept words before a cut that the kept words after it
// begin with, up to joinReach of them, and says which. The later saying stays,
// exactly as the match prefers it (keepMask) -- the match cannot see this one
// because the model kept both.
func dedupeJoins(words []srcWord, kept []bool) []string {
	var notes []string
	for j := 1; j < len(words); j++ {
		if kept[j] || !kept[j-1] {
			continue // not the first dropped word of a run
		}
		k := j
		for k < len(words) && !kept[k] {
			k++
		}
		if k >= len(words) {
			break
		}
		// the kept words before the run, and after it
		var before, after []int
		for i := j - 1; i >= 0 && kept[i] && len(before) < joinReach; i-- {
			before = append([]int{i}, before...)
		}
		for i := k; i < len(words) && kept[i] && len(after) < joinReach; i++ {
			after = append(after, i)
		}
		for n := min(len(before), len(after)); n > 0; n-- {
			same := true
			for x := 0; x < n && same; x++ {
				same = sameWord(words[before[len(before)-n+x]].w, words[after[x]].w)
			}
			if same {
				for _, i := range before[len(before)-n:] {
					kept[i] = false
				}
				notes = append(notes, fmt.Sprintf("%s: %q said again straight after the cut -- the earlier one goes",
					mmss(words[before[len(before)-n]].s), shortWords(words[before[len(before)-n]:before[len(before)-1]+1])))
				break
			}
		}
	}
	return notes
}

// joinReach is how many words either side of a join are compared: a doubled
// word or two is a join's own stumble; a whole sentence twice is a retake, and
// the model's to find.
const joinReach = 3

// marksFrom is the dropped stretches as marks: each a run of words that went,
// from the first to the last, with the word that resumes named. The edges
// themselves are placed afterwards, exactly as a retake's are (placeEdges).
func marksFrom(words []srcWord, kept []bool) []retake {
	var out []retake
	for i := 0; i < len(words); {
		if kept[i] {
			i++
			continue
		}
		j := i
		for j < len(words) && !kept[j] {
			j++
		}
		m := retake{S: words[i].s, E: words[j-1].e, To: words[j-1].e, Text: shortWords(words[i:j])}
		if j < len(words) {
			m.Again = words[j].s
		}
		out = append(out, m)
		i = j
	}
	return out
}

// cutByText is the cut of a read to camera: every filmed second, minus the
// marks, minus the dead air. No model -- the words decided, in Prepare.
func (a *App) cutByText(marks []retake) {
	a.ed.pushUndo()
	segs := insertsOf(a.ed.segs)
	// each filmed run trimmed to its words: a recording starts rolling
	// before the first word and stops after the last, and neither stretch
	// is the video. The same fenced placement a cut gets (endAfter,
	// startBefore), with the run's own edge as the fence.
	vids, auds := a.snapSources()
	paths := append(vids, auds...)
	words := a.spokenWords(paths)
	edgeOf := a.edgeLookup(paths, a.sessionRows())
	for _, r := range a.ed.runs() {
		segs = append(segs, trimRunToWords(cutSeg{S: r.t0, E: r.t1}, words, edgeOf))
	}
	n := dropMarked(&segs, marks)
	gone := dropDeadAir(&segs, a.ed.talk)
	a.ed.segs = segs
	a.ed.coalesce()
	a.ed.persist()
	a.ed.setBase()
	a.setStatus(fmt.Sprintf("cut by the words: %d segments", len(a.ed.segs)))
	total := a.ed.cutLen()
	a.logf(">>> cut by the words: %d stretch(es) taken out, %s of silence, %d segments, %d:%02d total",
		n, mmss(gone), len(a.ed.segs), int(total)/60, int(total)%60)
}

// trimRunToWords is a filmed run cut down to its speech: from just before its
// first word to just after its last, placed by the sound within troughReach
// of each and never outside the run. A run with no words in it stands as it
// is -- footage with nothing said over it is still footage.
func trimRunToWords(seg cutSeg, words []srcWord, edgeOf func(float64) *edges) cutSeg {
	var first, last *srcWord
	for i := range words {
		w := &words[i]
		if w.s < seg.S-0.01 || w.e > seg.E+0.01 {
			continue
		}
		if first == nil {
			first = w
		}
		last = w
	}
	if first == nil {
		return seg
	}
	s := math.Max(seg.S, first.s-wordPad)
	if e := edgeOf(first.s); e != nil {
		s = e.startBefore(*first, math.Max(seg.S, first.s-troughReach))
	}
	t := math.Min(seg.E, last.e+wordPad)
	if e := edgeOf(last.e); e != nil {
		t = e.endAfter(*last, math.Min(seg.E, last.e+troughReach))
	}
	return cutSeg{S: s, E: t}
}
