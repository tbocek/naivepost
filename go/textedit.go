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
	"slices"
	"strings"
	"unicode"
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

// textSystem is the pass's wording: one seam at a time.
//
// It used to be one call over the whole session, answering with the text of
// the finished video. That asks a model to reproduce five thousand words while
// making twenty surgical deletions, and the copy is the likeliest continuation
// at every step: measured on a 63-minute lecture, 4743 words in and 4738 out,
// with the retake at the first seam kept word for word. A seam at a time is a
// small, local question with a small answer, and no copying to drift into.
const textSystem = `You are repairing the join where a recording stopped and the next one started.

The speaker stumbled, stopped recording, and said it again -- sometimes starting a sentence or two further back. So the end of BEFORE and the start of AFTER can be the same thing said twice, and what the video plays has to run on as one.

You are given the last words of BEFORE and the first words of AFTER, as they were said. Answer with the two of them RUN ON AS ONE, strict JSON and nothing else:

  {"joined":"<the words of BEFORE and AFTER, in order, with the stumble left out>"}

THE TEST IS THE READING. Read your answer aloud. It has to be one piece of speech that makes sense: the sentence has to finish, and the paragraph has to say what the speaker was saying.

DELETE ONLY. Every word of your answer must be a word you were given, spelled as you were given it and in the order you were given it. Do not reword anything, do not add a word, do not move a word, do not repair the grammar of anything you keep. The ONLY thing you may do is leave words out.

LEAVE OUT ONE STRETCH, AT THE JOIN. The words you leave out have to be next to each other and they have to be the ones either side of the join -- the end of BEFORE, the start of AFTER, or some of each. Never a stretch out of the middle of BEFORE, and never one further into AFTER.

BEFORE gives way first: leave out the abandoned attempt off the end of BEFORE, going as far back as the repetition goes, and keep AFTER whole. Only where the join still does not read may you leave out words from the start of AFTER too, and then as few as possible -- what was said last is what the speaker meant to keep.

WHAT WAS SAID, NOT THE SCRIPT. The USER CONTEXT may hold the script the speaker read from. It is what they meant to say, not what they said: a sentence said differently from the script, or said in place of it, is speech like any other and stays. Use the script to see where a sentence was going; never leave something out because the script words it otherwise.

PUNCTUATION IS A HINT. The words carry the transcript's punctuation where it has any, and a sentence end is not always marked -- "it should And the next" is two sentences. Read for the sense, not the commas.

Leaving nothing out is a whole answer, and the ordinary one. A speaker who stopped to change a slide did not stumble: give back everything you were given, word for word.`

// There is no second pass over the middles of takes, and there was one.
//
// It asked, take by take, which stretches inside it were fumbles. On a
// 42-minute lecture it answered with 23 removals, against 22 the joins found,
// and three of the 23 were real. The rest took content out of grammatical
// sentences -- typically the object of a verb, leaving a sentence that parses
// and means nothing -- and several technical terms stopped appearing in the
// script at all.
//
// It is not a wording that can be fixed. A pass asked "which stretches here are
// fumbles" answers with stretches, and saying {"remove":[]} is the ordinary
// answer did not stop it -- that sentence was in the prompt for all 23. The
// question presupposes its answer.
//
// And it has no job. A read to camera is recorded by stopping when you stumble
// and saying it again, so the mistakes are at the joins; that is what Lecture
// MEANS, and the join pass already looks exactly there. What it cost was every
// sentence it touched. What removing it costs is two or three real stumbles a
// session, which are a word each in final.txt.

// findTextEdit is the pass: one call per seam, the removals, the marks.
//
// The marks come straight out of the answers -- each is a run of words with
// times of its own -- so nothing here has to match a text back against the
// words. The finished text is WRITTEN from what survives (finalText), and it
// is that file the hand-edit path reads back through the match (marksFromText).
func (a *App) findTextEdit(rows []tsvRow) ([]retake, error) {
	vids, auds := a.snapSources()
	paths := append(vids, auds...)
	words := a.spokenWords(paths)
	if len(words) < 4 {
		return nil, a.writeRetakes(nil)
	}
	for _, w := range words {
		if w.stray {
			a.logfIdle(">>> words: %q was placed on almost no sound, well after the word before it -- timed at the end of that word instead (%s)",
				w.raw, mmss(w.s))
		}
	}
	seams := seamsOf(words)
	if len(seams) == 0 {
		// one recording, so no join to repair. Everything said stands, which
		// is what a take nobody interrupted means.
		a.logfIdle(">>> text edit: one recording, no seam to repair -- every word stands")
		if err := a.writeFinalText(words, nil); err != nil {
			return nil, err
		}
		return nil, a.writeRetakes(nil)
	}
	system := a.sysPrompt("textedit")
	drop := make([]bool, len(words))
	asked, cached := 0, 0
	for k, at := range seams {
		if err := a.checkpoint(); err != nil {
			return nil, err
		}
		a.prog(trackFix, 0, "repairing join %d/%d", k+1, len(seams))
		gone, hit, err := a.askSeam(system, words, drop, at, k, len(seams))
		if err != nil {
			return nil, err
		}
		asked++
		if hit {
			cached++
		}
		for _, i := range gone {
			drop[i] = true
		}
	}
	a.logfIdle(">>> text edit: %d join(s) repaired, %d from the cache", asked, cached)

	kept := make([]bool, len(words))
	for i := range kept {
		kept[i] = !drop[i]
	}
	// ...and then "keep the later" once more, at every join this leaves behind.
	//
	// A join can be repaired correctly and still leave a word or two standing
	// on BOTH sides of the cut, and then the video says them twice. This is the
	// mechanical backstop for that: it was already here for the hand-edited
	// path (marksOfText) and the per-seam pass was written without it, which is
	// how one stumble reached one video three times running.
	for _, n := range dedupeJoins(words, kept) {
		a.logfIdle(">>> text edit: %s", n)
	}
	// the text and the marks are one answer, so whatever the dedupe took goes
	// out of both (writeFinalText reads drop, marksFrom reads kept)
	for i := range drop {
		drop[i] = !kept[i]
	}
	marks, notes := a.placeEdges(marksFrom(words, kept), paths, rows, words)
	for _, n := range notes {
		a.logfIdle(">>> text edit: %s", n)
	}
	marks = mergeMarks(marks)
	marks = a.flagWholeTakes(marks, words, kept)
	gone := 0
	for _, d := range drop {
		if d {
			gone++
		}
	}
	for _, m := range marks {
		a.logfIdle(">>> text edit: %s-%s goes (%q)", mmss(m.S), mmss(m.To), m.Text)
	}
	a.logfIdle(">>> text edit: %d of %d words removed in %d stretch(es)", gone, len(words), len(marks))
	if err := a.writeFinalText(words, drop); err != nil {
		return nil, err
	}
	return marks, a.writeRetakes(marks)
}

// seamCut is one join's answer, as the rest of the pass needs it: how many
// words come off each side. It is worked out from the joined text the model
// writes (seamCutOf), never read off a number the model was asked to count.
type seamCut struct {
	Before int
	After  int
}

// seamsOf is where one recording's words end and the next one's begin: the
// index of the first word of each later take.
func seamsOf(words []srcWord) []int {
	var out []int
	for i := 1; i < len(words); i++ {
		if words[i].src != words[i-1].src {
			out = append(out, i)
		}
	}
	return out
}

// seamReach is how many words either side of a join the model is shown.
//
// It was seventy: a retake starts "a sentence or two further back", and
// seventy words is four or five sentences. Right for a stumble and wrong for a
// restarted paragraph. On one lecture the speaker abandoned 63 words at one
// join and 71 at the next and began the thought again; the model was handed
// back its input unchanged at both, and it could not have done otherwise. At
// the second the attempt was longer than everything it was shown, and at the
// first it filled all but four words of it, so there was nothing before the
// attempt to see that a new thought had started. Twice that is still a small
// request, and the window has to hold the whole of what went plus enough in
// front of it to tell.
const seamReach = 140

// How much of a join one answer may take out. A stumble is a phrase or a
// sentence said twice; past that the model is rewriting the paragraph, which is
// the one thing this pass must never be allowed to do.
//
// Two numbers because the window is not always seamReach either side: the first
// join of a session may have twenty words in front of it, where forty is the
// whole of it and a share is the only guard that means anything.
const (
	seamMaxWords = 40
	seamCeil     = 0.6
	// ...and how far short of the join a stretch may stop and still be taken
	// as being at it (seamCutOf). A word or two is a model stopping short of
	// the last syllables of a fumble; farther is a different edit.
	seamSnap = 3
	// ...and the longest stretch away from the join that is taken as the model
	// having respelled a word rather than removed it (seamCutOf).
	seamNoise = 2
	// ...and how many times a join whose answer was refused is asked again.
	// A refusal keeps the stumble, which is the safe way to fail, but on one
	// lecture four of 57 joins were refused and every one of them was a
	// stumble left for the hand; a second answer costs one call where the
	// first went wrong and nothing where it did not.
	seamRetries = 1
)

// askSeam asks about one join and answers how much comes off each side.
//
// The model is asked for the JOINED TEXT, not for two numbers, and the numbers
// are worked out here from what it left out.
//
// Counting was the whole trouble. Asked "how many words come off the end of
// BEFORE", a model that has read the join correctly still has to count
// backwards through a list it cannot index, and it came back short every time:
// measured on one lecture, an abandoned attempt six words long was given up as
// three, and a phrase said on both sides of the stop was left standing on both.
// Writing the join out is the job it is good at, and it is the same job the
// prompt already judges it by: READ IT.
//
// The answer can only take words away. It is matched back against the words it
// was shown (keepMask, which is how a hand-edited final.txt is read), so a word
// the model reworded, added or moved matches nothing and changes nothing, and
// the worst a wandering answer can do is leave the join exactly as it was.
func (a *App) askSeam(system string, words []srcWord, drop []bool, at, k, n int) ([]int, bool, error) {
	idx, atW := seamWindow(words, drop, at)
	if atW == 0 || atW == len(idx) {
		return nil, false, nil // one side of this join is already gone whole
	}
	win := make([]srcWord, len(idx))
	for i, x := range idx {
		win[i] = words[x]
	}
	before, after := seamWords(win[:atW]), seamWords(win[atW:])
	user := a.ctxBlockFor("textedit") + fmt.Sprintf(
		"JOIN %d of %d.\n\nBEFORE (the end of the take that was interrupted):\n%s\n\n"+
			"AFTER (the beginning of the take that follows):\n%s\n\n"+
			"Answer {\"joined\":\"...\"} and nothing else: these %d words and then these %d, "+
			"in that order, with the stretch at the join left out.",
		k+1, n, before, after, len(strings.Fields(before)), len(strings.Fields(after)))

	first := askKey(system, user)
	cached := false
	for try := 0; try <= seamRetries; try++ {
		ask := first
		if try > 0 {
			// its own cache slot: the first answer was refused and never kept,
			// and asking again must not be answered by the same words
			ask = fmt.Sprintf("%s|try %d", first, try+1)
		}
		reply, hit := a.cachedReply("textedit", ask)
		cached = cached || hit
		if !hit {
			var err error
			// thinking, which is the one thing measured to help this pass.
			//
			// Scored against a 42-minute lecture cut by hand, 29 joins: without it
			// the model hands back its input unchanged at ten to twelve of them and
			// gets fifteen exactly right; with it there is not one join it passes
			// over, and twenty are exactly right. It finds 128 of the 180 words the
			// hand cut removed, against 56 to 67 without.
			//
			// It is not free. It costs about two minutes a join where the plain
			// call costs seconds, and it removed 14 words the hand cut kept, 13 of
			// them at one join, where the plain call removed none. Both were worth
			// it here: a stumble left in is a stumble you can still take out by
			// hand in final.txt, and a join nobody looked at is not.
			//
			// Two things that sounded better and measured worse: telling the model
			// which kind of stumble the join is (13 right, and it over-cut), and
			// having it choose between readings the machine builds from the
			// punctuation (5 right, and it removed 289 words the hand cut kept).
			reply, err = a.llmChatRetry("textedit",
				[]map[string]any{msg("system", system), msg("user", user)}, true)
			if err != nil {
				return nil, cached, err
			}
		}
		then := "nothing removed there"
		if try < seamRetries {
			then = "asking once more"
		}
		var got struct {
			Joined string `json:"joined"`
		}
		if problem := jsonReply(reply, &got); problem != "" {
			// a join nobody could answer for is a join that stays: the words on
			// both sides of it were said, and keeping something said twice is a
			// smaller fault than cutting something said once
			a.logfIdle("!!! text edit: join %d: %s -- %s", k+1, problem, then)
			continue
		}
		cut, why := seamCutOf(win, atW, got.Joined)
		if why != "" {
			a.logfIdle("!!! text edit: join %d: %s -- %s", k+1, why, then)
			continue
		}
		if !hit {
			a.keepReply("textedit", ask, reply)
		}
		// back to the session's own words: the window skipped the ones an
		// earlier join had already taken, so its neighbours are not always
		// neighbours in words
		return idx[atW-cut.Before : atW+cut.After], cached, nil
	}
	return nil, cached, nil
}

// seamWindow is one join as the model is shown it: up to seamReach words
// either side of it, as indexes into words, and where between them the join
// falls.
//
// Words an earlier join already took out are not in it. They used to be: the
// model read a phrase said twice where the video now says it once, and an
// answer that kept one of the two was matched back as two stretches -- the
// earlier join's and its own -- and refused. Measured on one lecture that was a
// join whose answer was exactly the cut made by hand, thrown away.
func seamWindow(words []srcWord, drop []bool, at int) ([]int, int) {
	var before, after []int
	for i := at - 1; i >= 0 && len(before) < seamReach; i-- {
		if drop == nil || !drop[i] {
			before = append(before, i)
		}
	}
	slices.Reverse(before)
	for i := at; i < len(words) && len(after) < seamReach; i++ {
		if drop == nil || !drop[i] {
			after = append(after, i)
		}
	}
	return append(before, after...), len(before)
}

// seamCutOf turns the joined text back into how much comes off each side, or
// says why it cannot be used. at is the index, within win, of the first word of
// AFTER.
//
// The match is against WHAT WAS PRINTED, and that is the whole of the lesson
// here. The model reads seamWords: the spelling the transcript pass settled on.
// The words themselves carry a second spelling, the bare one the aligner heard,
// and for a while the answer was matched against that instead. They are not the
// same: the transcript folds a heard "das über datum" into a written "das
// Über-Datum.", and one German lecture had sixty-four words folded that way.
// Every fold looked to the match like a word the model had deliberately
// dropped, so every answer read as several separate stretches and was refused
// whole -- twenty-nine joins of a session, nought marks, while the very same
// answers matched against the printed forms give ten clean cuts with not one
// invented word between them.
//
// So the answer can only take words away, and what it may take words away from
// is exactly the text it was shown.
func seamCutOf(win []srcWord, at int, joined string) (seamCut, string) {
	toks, owner := shownTokens(win)
	ans := textTokens(joined)
	if len(ans) == 0 {
		return seamCut{}, "the answer has no words in it"
	}
	// keepMask walks the answer backwards against the words, which is what
	// prefers the LATER of two sayings -- the rule the whole pass runs on
	shown := make([]srcWord, len(toks))
	for i, t := range toks {
		shown[i].w = t
	}
	keptTok, _ := keepMask(shown, ans)
	// a word survives if any of what was printed for it did: the transcript
	// spells some single words as two, and half a spelling is not a deletion
	kept := make([]bool, len(win))
	for i, ok := range keptTok {
		if ok {
			kept[owner[i]] = true
		}
	}
	// a word shown as part of the one before it goes where that one goes
	for i, r := range seamRoots(win) {
		if r != i {
			kept[i] = kept[r]
		}
	}
	// what it left out, stretch by stretch. There should be exactly one and it
	// should be at the join; a second one somewhere else is the model editing
	// prose, and the whole answer is refused over it.
	//
	// Except where that second one is a word or two, which is usually not a
	// deletion at all. The model respells as it writes -- "Ein interessantes
	// Gebiet" for a transcript that reads "Ein Interessensgebiet" -- and a word
	// it spelled its own way matches nothing and reads here exactly as if it
	// had been removed on purpose. Measured over two runs of a 42-minute
	// lecture: taking those as respellings rather than removals turns four
	// refusals into two and five into two, and gets one more join exactly right
	// each time. It was the only change of several that held up across both.
	first, last, other := -1, -1, 0
	for i := 0; i < len(win); {
		if kept[i] {
			i++
			continue
		}
		j := i
		for j < len(win) && !kept[j] {
			j++
		}
		switch {
		case i <= at+seamSnap && j-1 >= at-1-seamSnap && first < 0:
			first, last = i, j-1 // the one at the join, which is the repair
		case i <= at+seamSnap && j-1 >= at-1-seamSnap:
			// a second stretch that also reaches the join. It used to take
			// the first one's place, silently: an answer that left out
			// "... anschauen, was er" and "hat." with one word kept between
			// them was applied as "hat." alone. Two stretches are two.
			other++
		case j-i > seamNoise:
			other++
		}
		i = j
	}
	switch {
	case first < 0 && other == 0:
		return seamCut{}, "" // nothing left out, which is a whole answer
	case first < 0:
		return seamCut{}, fmt.Sprintf("the %s left out is not at the join",
			plural(other, "stretch"))
	case other > 0:
		return seamCut{}, fmt.Sprintf("%d separate stretches left out, not one at the join", other+1)
	}
	// ...and the stretch has to be AT the join, which is what tells a repair
	// apart from the model editing prose: a hole in the middle of BEFORE is the
	// removal that takes the object out of a sentence, leaving something that
	// parses and means nothing.
	//
	// Coming within a word or two of the join counts as being at it, and the
	// rest of the way is closed here. A model that names "Das heißt, hier habe
	// ich jeweils das Über-Datum." and leaves "datum und" standing against the
	// stop has found the abandoned attempt and stopped short of its last
	// syllables; the words it left are the end of that same attempt, and they
	// are on the side the pass spends first. Refusing the whole answer over
	// them is how the join everybody could see kept coming back.
	if d := at - 1 - last; d > 0 {
		if d > seamSnap {
			return seamCut{}, fmt.Sprintf("the stretch left out (%q) stops %d words short of the join",
				seamWords(win[first:last+1]), d)
		}
		last = at - 1
	}
	if d := first - at; d > 0 {
		if d > seamSnap {
			return seamCut{}, fmt.Sprintf("the stretch left out (%q) starts %d words past the join",
				seamWords(win[first:last+1]), d)
		}
		first = at
	}
	if n := last - first + 1; n > seamMaxWords || float64(n) > seamCeil*float64(len(win)) {
		return seamCut{}, fmt.Sprintf("%d of the %d words left out -- that is not a repair",
			n, len(win))
	}
	return seamCut{Before: at - first, After: last + 1 - at}, ""
}

// shownTokens is the window as the model reads it: one entry per word of what
// seamWords printed, each saying which of the window's words it came from.
//
// Usually one token per word. Not always: where the transcript has words the
// recogniser never heard, they ride on the next word that does have a time
// (redress), so one word can print as two or three. Those tokens all point back
// at the same word, and the word goes only if the whole of it went.
func shownTokens(win []srcWord) (toks []string, owner []int) {
	roots := seamRoots(win)
	for i, w := range win {
		if roots[i] != i {
			continue // printed as part of the word it was folded into
		}
		n := 0
		for _, f := range strings.Fields(seamWord(w)) {
			if b := bareWord(f); b != "" {
				toks = append(toks, b)
				owner = append(owner, i)
				n++
			}
		}
		if n == 0 {
			// nothing printable at all: a token of its own so the walk keeps
			// its place, and one the answer will never match
			toks = append(toks, "")
			owner = append(owner, i)
		}
	}
	return toks, owner
}

// seamRoots is, for every word of a window, the word it is shown as: itself,
// or -- for a word the transcript pass wrote INTO the word before it
// ("Proof-of-Concept-Serie," over proof, of, concept, serie) -- that word.
//
// Those words used to be shown a second time, bare: "Proof-of-Concept-Serie,
// of concept serie". A model that tidied the doubling away left out the bare
// ones, and with them went the sound of the compound it had meant to keep --
// "die letzte Proof-" and then the next take. Shown once, the compound is one
// thing to keep or leave out, and its words go together.
//
// Only words that ARE part of that spelling. The transcript pass also leaves
// no spelling on words it took out -- a filler, a word said twice inside a
// take -- and those are still shown, bare, as they always were: hiding them
// changes the text the model reads, not just how a compound is printed.
func seamRoots(win []srcWord) []int {
	roots := make([]int, len(win))
	for i, w := range win {
		roots[i] = i
		if w.raw != "" || i == 0 || win[i-1].src != w.src {
			continue
		}
		r := roots[i-1]
		if part := letters(w.w); part != "" && strings.Contains(letters(win[r].raw), part) {
			roots[i] = r
		}
	}
	return roots
}

// letters is a word with everything but its letters and digits taken out,
// lower case: "Proof-of-Concept-Serie," is "proofofconceptserie".
func letters(s string) string {
	var b strings.Builder
	for _, r := range strings.ToLower(s) {
		if unicode.IsLetter(r) || unicode.IsDigit(r) {
			b.WriteRune(r)
		}
	}
	return b.String()
}

// seamWords is one side of a join as the model reads it: the words as they are
// WRITTEN -- the case and punctuation the transcript pass settled on -- because
// the question is whether the join reads as a sentence, and a lower-case stream
// with no stops in it has no sentences to read. The match that follows is on
// the bare forms either way (textTokens), so what is shown here changes nothing
// about how an answer is understood.
func seamWords(ws []srcWord) string {
	var b []string
	for i, r := range seamRoots(ws) {
		if r == i {
			b = append(b, seamWord(ws[i]))
		}
	}
	return strings.Join(b, " ")
}

// seamWord is one word as it is shown and as it is written out: the spelling
// the transcript pass settled on, or the bare one it was heard as where the
// fold left it none. Never empty. A word folded into the one before it is not
// asked for at all (seamRoots); this is what the rest print, and what a word
// with nothing to be folded into prints.
func seamWord(w srcWord) string {
	if w.raw != "" {
		return w.raw
	}
	return w.w
}

// writeFinalText writes the finished text: the words that survive, as they are
// written. It is the edit, and a person can read it -- or change it and cut
// again (marksFromText), which is why it is punctuated rather than the bare
// stream the match works in.
//
// Every join is marked, because a join is the only place in the file where
// anything can be wrong. |cut 10| is a join where ten words went; |cut| is one
// where the recording stopped and nothing went, which is worth seeing too --
// a stumble the pass walked past looks exactly like ordinary prose otherwise.
// Reading 29 marks and the few words either side of each is the check; reading
// five thousand words of continuous text is not.
//
// The marks come back out again on the way in (textTokens), so this file can be
// edited by hand and cut from with them left in or taken out.
func (a *App) writeFinalText(words []srcWord, drop []bool) error {
	return os.WriteFile(a.finalText(), []byte(a.finishedText(words, drop)+"\n"), 0o644)
}

// finishedText is what it writes, apart from the file, so that the marking can
// be read without a project on disk.
func (a *App) finishedText(words []srcWord, drop []bool) string {
	// which recordings keep anything at all: a join whose dropped words hold
	// every word of one is marked as taking a whole take (flagWholeTakes)
	alive := map[string]bool{}
	for i, w := range words {
		if drop == nil || !drop[i] {
			alive[w.src] = true
		}
	}
	var b []string
	last, gone := "", 0
	whole := false
	for i, w := range words {
		if drop != nil && drop[i] {
			gone++
			if !alive[w.src] {
				whole = true
			}
			continue
		}
		if last != "" && w.src != last {
			mark := "|cut|"
			if gone > 0 {
				mark = fmt.Sprintf("|cut %d|", gone)
			}
			if whole {
				// one mark still: every field of it holds a pipe, so it is
				// read back as no words at all (textTokens)
				mark += "whole take|"
			}
			b = append(b, mark)
		}
		last, gone, whole = w.src, 0, false
		// a word whose spelling was folded into the one in front of it
		// (redress): it keeps its seconds and stays in the video, but it has
		// nothing of its own to print. The subtitles skip it for the same
		// reason, and so does the join prompt now (seamRoots). This file used
		// to fall back to the bare heard form instead and printed
		// "Public-Key-Pairs. key pairs", the one spelling twice.
		if w.raw == "" {
			continue
		}
		b = append(b, w.raw)
	}
	return strings.Join(b, " ")
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
	marks = a.flagWholeTakes(marks, words, kept)
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

// textTokens is the answer as words: whatever it was wrapped in, bare and
// lowercase like the words it was made from, with the markers left out.
func textTokens(reply string) []string {
	var out []string
	reply = pauseMark.ReplaceAllString(reply, " ")
	for _, f := range strings.Fields(reply) {
		// the join marks final.txt is written with, and the one the whole-text
		// pass used to write. Never a word anyone said: a pipe does not occur
		// in a transcript, so the test can be the character rather than the
		// exact spelling, and a mark a hand has mangled still goes.
		if f == "SEAM" || strings.Contains(f, "|") {
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

// textCut is the cut of a read to camera: every filmed second, minus the
// marks, minus the dead air. No model -- the words decided, in Prepare.
//
// Only the arithmetic. What it is given is the editor's state as values --
// the inserts it already holds, the filmed runs, where the talking is -- and
// what it hands back is segments and two numbers for the log; putting them on
// the page is the page's job (applyTextCut). It used to reach into the editor
// itself, undo stack and status line included, which made a file about words
// the one logic file that drove a widget.
func (a *App) textCut(marks []retake, inserts []cutSeg, runs []tlSpan, talk [][2]float64) (segs []cutSeg, marked int, dead float64) {
	segs = insertsOf(inserts)
	// each filmed run trimmed to its words: a recording starts rolling
	// before the first word and stops after the last, and neither stretch
	// is the video. The same fenced placement a cut gets (endAfter,
	// startBefore), with the run's own edge as the fence.
	vids, auds := a.snapSources()
	paths := append(vids, auds...)
	words := a.spokenWords(paths)
	edgeOf := a.edgeLookup(paths, a.sessionRows())
	for _, r := range runs {
		segs = append(segs, trimRunToWords(cutSeg{S: r.t0, E: r.t1}, words, edgeOf))
	}
	marked = dropMarked(&segs, marks)
	dead = dropDeadAir(&segs, talk)
	return segs, marked, dead
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

// wholeTakes is every recording none of whose words survive, with its first
// and last word: a join is allowed to take a whole take out -- a short take
// that was nothing but a false start is exactly that -- but it is also what a
// join that misread its window looks like. Measured on one lecture: a join
// took out all fifteen seconds of the take before it, and the first eight of
// its words were wanted.
func wholeTakes(words []srcWord, kept []bool) map[string][2]float64 {
	span := map[string][2]float64{}
	alive := map[string]bool{}
	for i, w := range words {
		if kept[i] {
			alive[w.src] = true
		}
		if sp, ok := span[w.src]; ok {
			span[w.src] = [2]float64{sp[0], w.e}
		} else {
			span[w.src] = [2]float64{w.s, w.e}
		}
	}
	for src := range alive {
		delete(span, src)
	}
	return span
}

// flagWholeTakes names, on each mark, the recordings it takes out entire, and
// says so in the log. Flagged, not refused: the Cut page tints such a
// recording yellow, and final.txt marks its join "|whole take|", so it is
// found by looking rather than by listening to the whole video.
func (a *App) flagWholeTakes(marks []retake, words []srcWord, kept []bool) []retake {
	whole := wholeTakes(words, kept)
	for i := range marks {
		var names []string
		for src, sp := range whole {
			if sp[0] >= marks[i].S-0.01 && sp[1] <= marks[i].E+0.01 {
				names = append(names, src)
			}
		}
		slices.Sort(names)
		marks[i].Whole = strings.Join(names, ",")
		for _, src := range names {
			a.logfIdle(">>> text edit: all of %s goes (%s-%s) -- marked yellow on Cut, worth a look",
				src, mmss(whole[src][0]), mmss(whole[src][1]))
		}
	}
	return marks
}
