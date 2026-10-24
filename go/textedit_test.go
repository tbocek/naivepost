package main

import (
	"math"
	"strings"
	"testing"
)

func said(text string, at float64, src string) []srcWord {
	var out []srcWord
	for i, w := range strings.Fields(text) {
		s := at + float64(i)*0.4
		out = append(out, srcWord{s: s, e: s + 0.3, w: w, src: src})
	}
	return out
}

// The match walks from the end, so where the same words were said twice the
// answer keeps the LATER saying. Walked forwards it would keep the abandoned
// attempt every time -- the rule "prefer the second" is the shape of the
// match, not a request to the model.
func TestTheTextEditKeepsTheLaterOfTwoSayings(t *testing.T) {
	words := append(said("the whole run took roughly thirteen", 100, "t3"),
		said("and the whole run took roughly thirteen point five gpu years", 110, "t4")...)
	kept, extra := keepMask(words, textTokens("and the whole run took roughly thirteen point five gpu years"))
	if extra != 0 {
		t.Errorf("%d words were called never said", extra)
	}
	for i := 0; i < 6; i++ {
		if kept[i] {
			t.Errorf("word %d %q of the abandoned attempt was kept", i, words[i].w)
		}
	}
	for i := 6; i < len(words); i++ {
		if !kept[i] {
			t.Errorf("word %d %q of the retake was dropped", i, words[i].w)
		}
	}
	// a word the answer made up is not matched to anything, and does not
	// throw the words around it off
	kept, extra = keepMask(words, textTokens("and the whole run took roughly INVENTED thirteen point five gpu years"))
	if extra != 1 || !kept[6] || !kept[11] {
		t.Errorf("an invented word cost the words around it: extra=%d kept=%v", extra, kept)
	}
}

// The marks: each dropped run, first word to last, with the word that
// resumes named -- and then the edges placed as a retake's are: without an
// envelope a hair after the last word that stays and a hair before the first
// that resumes (wordPad), so the words fence the cut whatever the sound says.
func TestTheDroppedRunsBecomeMarksOnTheWordEdges(t *testing.T) {
	words := append(said("state of the art", 100, "t3"),
		said("and the whole run took roughly thirteen", 101.6, "t3")...)
	words = append(words, said("and the whole run took roughly thirteen point five", 110, "t4")...)
	kept := make([]bool, len(words))
	for i := range kept {
		kept[i] = i < 4 || i >= 11 // "state of the art" and the retake
	}
	raw := marksFrom(words, kept)
	if len(raw) != 1 || raw[0].S != words[4].s || raw[0].E != words[10].e || raw[0].Again != words[11].s {
		t.Fatalf("one dropped run came out as %+v", raw)
	}
	if !strings.HasPrefix(raw[0].Text, "and the whole run") {
		t.Errorf("the mark's text is %q", raw[0].Text)
	}
	marks, _ := (&App{}).placeEdges(raw, nil, nil, words)
	m := marks[0]
	if math.Abs(m.S-(words[3].e+wordPad)) > 1e-9 {
		t.Errorf("the cut ends at %.3f, want a hair after %q (%.3f)", m.S, words[3].w, words[3].e+wordPad)
	}
	if math.Abs(m.To-(words[11].s-wordPad)) > 1e-9 {
		t.Errorf("the cut resumes at %.3f, want a hair before %q (%.3f)", m.To, words[11].w, words[11].s-wordPad)
	}
	// a run at the very end has nothing to resume at
	kept[len(kept)-1] = false
	if last := marksFrom(words, kept)[1]; last.Again != 0 || last.To != last.E {
		t.Errorf("a dropped run at the end claims a retake: %+v", last)
	}
}

// With an envelope, the sound chooses WHERE inside the fence: the word's own
// tail is followed past the aligner's edge (a trailing s it was early on),
// and then the cut goes to the quietest moment of the gap -- not into the
// breath after it, and never as far as the next word.
func TestTheSoundPlacesTheCutInsideTheWordFence(t *testing.T) {
	// 100 Hz buckets, 4 s: a word 1.00-1.40 at level 200 whose sound really
	// runs to 1.48; quiet 1.48-1.70 (20); a breath 1.70-1.90 (90); quiet to
	// the next word at 2.20
	wf := &waveform{hz: 100, chans: [][]uint8{make([]uint8, 400)}}
	env := wf.chans[0]
	for i := range env {
		env[i] = 20
	}
	for i := 100; i < 148; i++ {
		env[i] = 200
	}
	for i := 170; i < 190; i++ {
		env[i] = 90
	}
	for i := 220; i < 260; i++ {
		env[i] = 200
	}
	e := &edges{wf: wf, off: 0}
	stays := srcWord{s: 1.0, e: 1.4, w: "art"}
	next := srcWord{s: 2.2, e: 2.6, w: "and"}
	got := e.endAfter(stays, next.s)
	if got < 1.48 || got >= 1.70 {
		t.Errorf("the cut ends at %.2f, want after the word's real tail (1.48) and before the breath (1.70)", got)
	}
	// the other way: the word that resumes starts at 2.20; its sound is
	// followed back no further than its own onset, and the resume point is
	// the quiet before it, not the breath
	if got := e.startBefore(next, stays.e); got < 1.90 || got > 2.2 {
		t.Errorf("the cut resumes at %.2f, want in the quiet between the breath (1.90) and the word (2.20)", got)
	}
	// the fence: however the sound reads, the cut never reaches the other word
	if got := e.endAfter(stays, 1.45); got > 1.45 {
		t.Errorf("the cut ran to %.2f, past the fence at 1.45", got)
	}
	// words that abut leave nothing to choose
	if got := e.endAfter(stays, 1.4); got != 1.4 {
		t.Errorf("abutting words: %.2f, want the word's own end", got)
	}
}

// Where the joins are, and what a hand-edited file's words read back as.
//
// textBrief was here -- the whole session as one prompt, seams and pauses
// marked -- and went with the pass that sent it: the model is asked about one
// join at a time now (askSeam), so there is no session-long brief to write.
func TestTheSeamsAreWhereTheRecordingChanges(t *testing.T) {
	words := append(said("stay safe", 100, "t3"), said("stay unsafe forever", 105, "t4")...)
	words = append(words, srcWord{s: 106.9, e: 107.2, w: "bitbox", src: "t4"})
	got := seamsOf(words)
	if len(got) != 1 || got[0] != 2 {
		t.Errorf("the seams are %v, want one at the first word of the second take", got)
	}
	if len(seamsOf(said("one take only", 0, "t1"))) != 0 {
		t.Error("one recording has a join in it")
	}
	// the answer's words, whatever it wrapped them in: this is the hand-edit
	// path's reader (marksFromText), and it still has to bare what it is given
	if got := textTokens("Stay unsafe forever.\nSEAM\n(pause 1.9s)\nBitBox, stated"); strings.Join(got, " ") != "stay unsafe forever bitbox stated" {
		t.Errorf("the answer's words read as %q", strings.Join(got, " "))
	}
}

// The style is a fact about the session, stored with it, and it decides which
// pipeline runs: a read to camera is never described, its marks are the text
// edit's, and its cut is every filmed second minus them -- no model.
func TestAReadToCameraRunsThroughTheTextEdit(t *testing.T) {
	for _, c := range []struct{ file, want string }{
		{"transcript.go", `marks, err := a.findMarks(tl)`},
		{"transcript.go", `return a.findTextEdit(tl)`},
		{"cut_suggest.go", `a.ed.applyTextCut(a.textCut(marks, a.ed.segs, a.ed.runs(), a.ed.talk))`},
		{"project.go", "Style string `json:\"style,omitempty\"`"},
		{"project.go", `Style:      a.videoStyleName(),`},
		{"project.go", `a.applyStyle(p.Style)`},
		{"prompts.go", `{key: "textedit", def: strings.TrimSpace(textSystem)},`},
		{"syscontext.go", `"textedit":  sysSet(`},
		{"syscontext.go", `  textedit: ONE join`},
		// the working subtitle file no longer sits beside the video with its
		// name, where a player loads it as a second track of its own accord
		{"produce.go", `srtPath := filepath.Join(clipDir, "final.srt")`},
	} {
		if !strings.Contains(readSrc(t, c.file), c.want) {
			t.Errorf("%s does not contain %q", c.file, c.want)
		}
	}
	// EVERY style is described, Lecture included: its cut does not need the
	// picture, but Publish picks the thumbnail by what is on a frame, and a
	// frame nobody described is a frame nothing can find
	body := funcBody(t, "prep.go", `func \(a \*App\) understand\(`)
	if strings.Contains(body, "styleRead") {
		t.Error("a lecture is not described, so nothing can find its title slide")
	}
	if !strings.Contains(body, `a.qJob(trackDescribe, "describe", 1, 2)`) {
		t.Error("the describing half is gone")
	}
	// the page's two names, in the order asked for, mapped onto the stored
	// words either way round
	if styleLabels[0] != "Lecture" || styleLabels[1] != "Gaming" {
		t.Errorf("the styles are named %v, want Lecture then Gaming", styleLabels)
	}
	if styleOf(0) != styleRead || styleOf(1) != styleMoments || styleIndex(styleRead) != 0 || styleIndex(styleMoments) != 1 {
		t.Error("Lecture is not the read to camera, or Gaming not the session")
	}
	// and the default -- no style -- is what every older project is
	a := &App{}
	if a.videoStyleName() != styleMoments {
		t.Error("a project with no style is not a session")
	}
	a.videoStyle = styleRead
	if a.videoStyleName() != styleRead {
		t.Error("the project's word for the style is not read before the page exists")
	}
}

// "...existing project and | and the more it got away": the model dropped one
// word between them and left the same word on both sides of the cut. Keep the
// later, at the join too -- the match cannot see this one, since both were
// kept.
func TestADoubledWordAtAJoinKeepsTheLater(t *testing.T) {
	words := append(said("so they used an already existing project and as", 194, "t4"),
		said("and the more it got away from that", 204.8, "t5")...)
	kept := make([]bool, len(words))
	for i := range kept {
		kept[i] = words[i].w != "as"
	}
	notes := dedupeJoins(words, kept)
	if kept[7] {
		t.Errorf("the earlier %q stayed on its side of the cut (%v)", words[7].w, notes)
	}
	if !kept[6] || !kept[9] {
		t.Errorf("more than the doubled word went: %v", kept)
	}
	// two words doubled go as two; different words are left alone
	words = append(said("the whole run took", 100, "t3"), said("run took roughly thirteen", 110, "t4")...)
	kept = []bool{true, true, true, false, true, true, true, true}
	dedupeJoins(words, kept)
	if kept[2] || !kept[1] {
		t.Errorf("a two-word repeat across the join was not handled: %v", kept)
	}
	words = append(said("state of the art", 100, "t3"), said("and the whole run", 110, "t4")...)
	kept = []bool{true, true, true, false, true, true, true, true}
	dedupeJoins(words, kept)
	if !kept[2] {
		t.Error("different words either side of a cut were called a repeat")
	}
}

// A recording starts rolling before the first word and stops after the last,
// and neither stretch is the video: 0.85 s of nothing at the head of a clip,
// which is what "too much silence around the cut" was.
func TestAFilmedRunIsTrimmedToItsWords(t *testing.T) {
	words := said("so they used an already existing project", 193.85, "t4")
	none := func(float64) *edges { return nil }
	got := trimRunToWords(cutSeg{S: 193, E: 199.53}, words, none)
	if math.Abs(got.S-(193.85-wordPad)) > 1e-9 {
		t.Errorf("the clip starts at %.2f, want a hair before %q", got.S, "so")
	}
	if last := words[len(words)-1]; math.Abs(got.E-(last.e+wordPad)) > 1e-9 {
		t.Errorf("the clip ends at %.2f, want a hair after %q", got.E, last.w)
	}
	// never outside the run, and a run with nothing said in it stands
	if got := trimRunToWords(cutSeg{S: 193.9, E: 194.1}, words, none); got.S < 193.9 || got.E > 194.1 {
		t.Errorf("the trim left the run: %+v", got)
	}
	if got := trimRunToWords(cutSeg{S: 300, E: 310}, words, none); got.S != 300 || got.E != 310 {
		t.Errorf("a run with no words was trimmed: %+v", got)
	}
	if !strings.Contains(funcBody(t, "textedit.go", `func \(a \*App\) textCut\(`), "trimRunToWords(") {
		t.Error("the mechanical cut keeps the recordings' silent heads and tails")
	}
}

// The text is the edit. Delete a word from final.txt by hand, save, press Cut:
// the marks are remade from the file and the word is out of the video -- no
// model asked. Only when the file is NEWER than the marks, so a Cut after a
// Prepare that wrote both does nothing twice.
func TestAHandEditedFinalTextRemakesTheMarks(t *testing.T) {
	words := append(said("they forked the project", 160, "t3"), said("and the more it got away", 204.8, "t4")...)
	a := &App{}
	// the model's text, then the same with "the project" taken out by hand
	marks, ok := a.marksOfText("they forked the project\nSEAM\nand the more it got away", words, nil, nil)
	if !ok || len(marks) != 0 {
		t.Fatalf("the unedited text made marks: %+v", marks)
	}
	marks, ok = a.marksOfText("they forked\nand the more it got away", words, nil, nil)
	if !ok || len(marks) != 1 || marks[0].Text != "the project" {
		t.Fatalf("the hand edit did not become a mark: %+v", marks)
	}
	if math.Abs(marks[0].S-(words[1].e+wordPad)) > 1e-9 || math.Abs(marks[0].To-(words[4].s-wordPad)) > 1e-9 {
		t.Errorf("the hand edit's mark is not on the word edges: %+v", marks[0])
	}
	// the staleness rule, and that Cut asks it
	body := funcBody(t, "textedit.go", `func \(a \*App\) marksFromText\(`)
	if !strings.Contains(body, "!ti.ModTime().After(mi.ModTime())") {
		t.Error("a final.txt older than the marks is re-read every Cut")
	}
	if !strings.Contains(readSrc(t, "cut_suggest.go"), "if m, ok := a.marksFromText(); ok {") {
		t.Error("Cut never reads a hand-edited final.txt")
	}
	// and the pass writes the text BEFORE the marks, so a fresh Prepare's
	// marks are the newer file and nothing is remade behind it
	pass := funcBody(t, "textedit.go", `func \(a \*App\) findTextEdit\(`)
	if i, j := strings.Index(pass, "a.writeFinalText(words, drop)"), strings.Index(pass, "a.writeRetakes(marks)"); i < 0 || j < 0 || i > j {
		t.Error("the marks are written before the text, so the text always looks edited")
	}
}

// The pass asks about ONE JOIN at a time, and the test it applies is the
// reading.
//
// It was one call over the whole session answering with the text of the
// finished video, and a model asked to reproduce five thousand words while
// making twenty deletions copies them instead: measured, 4743 words in and
// 4738 out, with a textbook retake at the first seam -- "Startups. Das heisst
// ich war frueher bei einem Tech Venture. | Ein Interessensgebiet, das sind
// Startups" -- kept word for word. A join at a time is a small question with a
// small answer, and no five thousand words to drift through.
//
// It then asked for that small answer as two NUMBERS, and numbers were the next
// trouble: a model that has read the join correctly still has to count
// backwards through a list it cannot index. The same Tech Venture join came
// back as three words where six were said twice. So it writes the join out
// instead -- the thing the prompt was already asking it to judge by -- and the
// numbers are worked out from what it left out (seamCutOf).
func TestTheTextEditIsJudgedByWhetherTheJoinReads(t *testing.T) {
	for _, want := range []string{
		`{"joined":`,                         // the two takes, run on as one
		"THE TEST IS THE READING",            // the standard it is judged by
		"Read your answer aloud",             //
		"DELETE ONLY",                        // and nothing else: no rewording, no repair
		"LEAVE OUT ONE STRETCH, AT THE JOIN", // not a hole in the middle of a sentence
		"BEFORE gives way first",             // the earlier saying goes
		"Leaving nothing out is a whole answer",
		// the script in the User Context is what was meant, not what was
		// said: a restart said off-script was once taken for the stumble
		"WHAT WAS SAID, NOT THE SCRIPT",
		// and a sentence end the transcript did not mark is still one
		"PUNCTUATION IS A HINT",
	} {
		if !strings.Contains(textSystem, want) {
			t.Errorf("textSystem no longer says %q", want)
		}
	}
	// it must not ask for a COUNT again: that is what it answers badly
	for _, gone := range []string{"how many words", `{"before":<n>`} {
		if strings.Contains(textSystem, gone) {
			t.Errorf("the join is asked to count again: %q", gone)
		}
	}
	// the wording asks about a join, not about the video: "the text of the
	// finished video" invited the model to decide what BELONGS in it, which is
	// a different job and one this pass must not do
	if strings.Contains(textSystem, "the text of the finished video") {
		t.Error("the wording still asks for an editorial judgement rather than a repair")
	}
	// the model is shown the words AS WRITTEN, or it cannot judge a sentence:
	// a lower-case stream with no stops in it has no sentences to read
	if !strings.Contains(funcBody(t, "textedit.go", `func seamWord\(`), "w.raw") {
		t.Error("the join is shown as bare words, so the reading test has nothing to read")
	}
	// ...and the answer is matched against those same printed forms, never
	// against the bare ones the recogniser heard. The two do not agree word for
	// word, and matching the wrong one refused every join of a whole session.
	shown := funcBody(t, "textedit.go", `func shownTokens\(`)
	if !strings.Contains(shown, "seamWord(w)") || !strings.Contains(shown, "bareWord(f)") {
		t.Error("the answer is matched against something other than what was printed")
	}
	if !strings.Contains(funcBody(t, "textedit.go", `func seamCutOf\(`), "shownTokens(win)") {
		t.Error("seamCutOf does not match against the words the model was shown")
	}
	// ...and the answer is checked against the window it was asked about
	ask := funcBody(t, "textedit.go", `func \(a \*App\) askSeam\(`)
	if !strings.Contains(ask, "seamCutOf(win, atW, got.Joined)") {
		t.Error("a join's answer is trusted rather than checked against what it was asked about")
	}
	// a join nobody could answer for keeps its words: something said twice is
	// a smaller fault than something said once and cut
	if !strings.Contains(ask, "nothing removed there") {
		t.Error("an unreadable answer removes something anyway")
	}
	// ...but it is asked once more first: on one lecture every refused join
	// was a stumble left for the hand
	if !strings.Contains(ask, "try <= seamRetries") || !strings.Contains(ask, "asking once more") || seamRetries != 1 {
		t.Error("a refused join is not asked a second time")
	}
}

// There is no second pass over the middles of takes, and removing it was the
// single biggest improvement this file has had.
//
// It asked, take by take, which stretches inside it were fumbles. On a
// 42-minute lecture it answered with 23 removals against the joins' 22, and
// three of the 23 were real. The rest took content out of grammatical
// sentences: "Es muss am Ende einen X402-End-to-End-Flow geben" came back as
// "Es muss am Ende einen geben", and "Markdown", "Polygon" and "Testnet"
// stopped appearing in the script at all.
//
// It is not a wording that can be fixed. "Which stretches here are fumbles" is
// a question that presupposes its answer, and `{"remove":[]} is the ordinary
// answer` was in the prompt for all 23 of them. A read to camera is recorded by
// stopping when you stumble and saying it again, so the mistakes are at the
// joins -- that is what Lecture means, and the join pass looks exactly there.
func TestNothingLooksInsideATakeForFumbles(t *testing.T) {
	src := readSrc(t, "textedit.go")
	for _, gone := range []string{"stumbleSystem", "askTakes", "takesOf", "takeCeil"} {
		if strings.Contains(src, "func "+gone) || strings.Contains(src, gone+" =") {
			t.Errorf("the mid-take fumble pass is back: %q", gone)
		}
	}
	// and its prompt is a dead key rather than a renamed one, so a project that
	// edited it does not silently get someone else's wording
	if strings.Contains(readSrc(t, "prompts.go"), `def: strings.TrimSpace(stumbleSystem)`) {
		t.Error("the stumble prompt is still registered")
	}
	// the pass that remains looks only where one recording meets the next
	body := funcBody(t, "textedit.go", `func \(a \*App\) findTextEdit\(`)
	if !strings.Contains(body, "seamsOf(words)") {
		t.Error("findTextEdit no longer works join by join")
	}
	if strings.Contains(body, "takesOf") {
		t.Error("findTextEdit still reads whole takes")
	}
}

// The join the model got wrong three times running, and the backstop that
// catches it however wrong the answer is.
//
// Said: "...schauen wir uns an, wie man eben das Gelernte praktisch" -- stop,
// restart -- "wie man die Theorie auch praktisch anwenden kann". The model is
// asked how many words come off each side and answered four and none, which is
// a word or two short: "wie man" was left standing on BOTH sides of the cut and
// the video said it twice. It is the counting that is unreliable, not this
// particular count, so the repair is mechanical and runs on whatever the model
// answers -- for the per-seam pass (findTextEdit) exactly as for a final.txt
// edited by hand (marksOfText).
func TestAWordLeftOnBothSidesOfAJoinGoesOnce(t *testing.T) {
	words := joinWords("schauen wir uns an wie man eben das gelernte praktisch",
		"wie man die theorie auch praktisch anwenden kann")
	kept := make([]bool, len(words))
	for i := range kept {
		kept[i] = true
	}
	for i := 6; i < 10; i++ {
		kept[i] = false // the answer: {"before":4,"after":0}
	}
	notes := dedupeJoins(words, kept)
	got := keptWords(words, kept)
	if strings.Contains(got, "wie man wie man") {
		t.Errorf("the video still says it twice: %q", got)
	}
	if want := "schauen wir uns an wie man die theorie auch praktisch anwenden kann"; got != want {
		t.Errorf("the join reads %q, want %q", got, want)
	}
	// and it says which words it took, because a repair nobody can see is one
	// nobody can argue with
	if len(notes) != 1 || !strings.Contains(notes[0], "wie man") {
		t.Errorf("the repair was silent or vague: %v", notes)
	}
	// the LATER saying is the one that stays: what was said last is what was
	// meant, which is the same rule the whole pass runs on
	if words[4].src == words[len(words)-1].src {
		t.Fatal("the fixture no longer spans two takes")
	}
}

// and the pass itself calls it -- the regression that let the above reach the
// video was simply that findTextEdit did not, though marksOfText always had
func TestBothTextPassesRepairTheirJoins(t *testing.T) {
	for _, fn := range []string{`func \(a \*App\) findTextEdit\(`, `func \(a \*App\) marksOfText\(`} {
		if !strings.Contains(funcBody(t, "textedit.go", fn), "dedupeJoins(words, kept)") {
			t.Errorf("%s does not repair its joins", fn)
		}
	}
	// and the text it writes is the cut it makes: the dedupe has to reach both
	body := funcBody(t, "textedit.go", `func \(a \*App\) findTextEdit\(`)
	dd := strings.Index(body, "dedupeJoins(words, kept)")
	for _, after := range []string{"marksFrom(words, kept)", "a.writeFinalText(words, drop)"} {
		if i := strings.Index(body, after); i < 0 || i < dd {
			t.Errorf("%s is built before the joins are repaired", after)
		}
	}
	if !strings.Contains(body, "drop[i] = !kept[i]") {
		t.Error("the dedupe reaches the marks but not the text, so final.txt and the video disagree")
	}
}

// joinWords is two takes of speech, one after the other with a gap between.
func joinWords(before, after string) []srcWord {
	var out []srcWord
	at := 0.0
	for i, part := range []string{before, after} {
		for _, w := range strings.Fields(part) {
			out = append(out, srcWord{s: at, e: at + 0.3, w: w, raw: w,
				src: []string{"take1", "take2"}[i]})
			at += 0.4
		}
		at += 4 // the recording stopped and started again
	}
	return out
}

// keptWords is what the video says, in order.
func keptWords(words []srcWord, kept []bool) string {
	var out []string
	for i, k := range kept {
		if k {
			out = append(out, words[i].w)
		}
	}
	return strings.Join(out, " ")
}

// The join answer: the two takes run on as one, matched back against the words
// the model was shown. The shapes, not one session's sentences.
func TestAJoinIsReadBackOffTheWordsItWasShown(t *testing.T) {
	for _, c := range []struct {
		name, before, after, joined string
		want                        string // what the video says, or "" when refused
		why                         string // a fragment of the refusal
	}{
		{name: "the abandoned attempt comes off the end of BEFORE",
			before: "and so the point here is that we were",
			after:  "The point here is that we were early.",
			joined: "and so The point here is that we were early.",
			want:   "and so The point here is that we were early."},
		{name: "a phrase said either side of the stop is said once",
			before: "now let us look at how you would",
			after:  "how you would run this yourself",
			joined: "now let us look at how you would run this yourself",
			want:   "now let us look at how you would run this yourself"},
		{name: "a recording that simply carries on loses nothing",
			before: "and that is the plan for today.",
			after:  "First we look at the slides.",
			joined: "and that is the plan for today. First we look at the slides.",
			want:   "and that is the plan for today. First we look at the slides."},
		{name: "both sides, where taking BEFORE alone does not read",
			before: "we will look at how that works",
			after:  "how that works, we see shortly",
			joined: "we will look at we see shortly",
			want:   "we will look at we see shortly"},
		{name: "a stretch out of the middle of BEFORE is refused",
			before: "there has to be a full end to end run for you to pass",
			after:  "The task opened last year.",
			joined: "there has to be a full run for you to pass The task opened last year.",
			why:    "not at the join"},
		{name: "a stretch a word or two short of the join is taken as being at it",
			before: "and here I have the over date date and",
			after:  "Here I have the date of the lecture.",
			joined: "and here date and Here I have the date of the lecture.",
			want:   "and here Here I have the date of the lecture."},
		{name: "a rewrite is refused",
			before: "I used to work at a small company",
			after:  "One area of interest is startups.",
			joined: "I am interested in startups.",
			why:    "that is not a repair"},
		{name: "words it was never given change nothing",
			before: "I used to work there",
			after:  "One area of interest is startups.",
			joined: "I used to work there and besides One area of interest is startups.",
			want:   "I used to work there One area of interest is startups."},
	} {
		win, at := seamFixture(c.before, c.after)
		cut, why := seamCutOf(win, at, c.joined)
		if c.why != "" {
			if !strings.Contains(why, c.why) {
				t.Errorf("%s: refused with %q, want something about %q", c.name, why, c.why)
			}
			continue
		}
		if why != "" {
			t.Errorf("%s: refused with %q", c.name, why)
			continue
		}
		var keep []srcWord
		for i, w := range win {
			if i >= at-cut.Before && i < at+cut.After {
				continue
			}
			keep = append(keep, w)
		}
		if got := seamWords(keep); got != c.want {
			t.Errorf("%s:\n got  %q\n want %q", c.name, got, c.want)
		}
	}
}

// A word the model respelled is not a word it removed.
//
// It writes as it reads, and it respells while it writes: "Ein interessantes
// Gebiet" for a transcript that says "Ein Interessensgebiet". That word matches
// nothing, so the match reads it as deliberately dropped, and the answer came
// back as two separate stretches and was refused whole -- the repair at the
// join thrown away over a synonym a hundred words from it.
//
// Measured over two runs of one lecture's 29 joins: taking a stray word or two
// as a respelling rather than a removal turns four refusals into two and five
// into two, and gets one more join exactly right in both. A longer stretch away
// from the join is still a refusal, because that is the model editing prose.
func TestARespelledWordIsNotARemovedOne(t *testing.T) {
	before := "and so the point here is that we were"
	after := "The point here is that we were early."
	win, at := seamFixture(before, after)
	// the repair at the join, plus one word respelled far from it
	joined := "and so THE POINT here is that we were early."
	cut, why := seamCutOf(win, at, joined)
	if why != "" {
		t.Fatalf("a respelling refused the join: %s", why)
	}
	if cut.Before == 0 {
		t.Error("the repair at the join was lost")
	}
	// ...but a real stretch away from the join is still refused: that is the
	// removal that takes the object out of a sentence and leaves it parsing
	if _, why := seamCutOf(win, at, "and so is that we were The point here is that we were early."); why == "" {
		t.Error("a stretch out of the middle of BEFORE was accepted")
	}
	// and the threshold is a word or two, not a phrase
	if seamNoise > 2 {
		t.Errorf("stretches of up to %d words away from the join are ignored", seamNoise)
	}
}

// The answer is matched against WHAT WAS SHOWN, not against what was heard.
//
// The two are not the same word for word. The transcript pass respells what the
// recogniser heard and sometimes folds two heard words into one written one,
// leaving the other with no written form at all; one German lecture had 64
// words folded that way. Matched against the heard spellings, every fold looked
// like a word the model had deliberately dropped, so every answer read as
// several separate stretches and was refused: 29 joins of a session produced 0
// marks, where the same answers matched against the printed forms give 10 clean
// cuts.
func TestAFoldedWordDoesNotBreakTheJoin(t *testing.T) {
	// printed:  Das heißt, hier habe ich jeweils das Über-Datum. datum und
	// heard  :  das heißt  hier habe ich jeweils das über        datum und
	heard := []string{"das", "heißt", "hier", "habe", "ich", "jeweils", "das", "über", "datum", "und"}
	written := []string{"Das", "heißt,", "hier", "habe", "ich", "jeweils", "das", "Über-Datum.", "", "und"}
	var win []srcWord
	at := 0.0
	for i := range heard {
		win = append(win, srcWord{s: at, e: at + 0.3, w: heard[i], raw: written[i], src: "t1"})
		at += 0.4
	}
	at += 4
	join := len(win)
	for _, x := range strings.Fields("Das heißt, hier habe ich entsprechend das Datum der Vorlesung.") {
		win = append(win, srcWord{s: at, e: at + 0.3, w: bareWord(x), raw: x, src: "t2"})
		at += 0.4
	}
	// a word the transcript wrote into the one before it is shown once, as
	// part of that word: shown again bare, it was the thing a model tidied
	// away, and its sound went with it
	if !strings.Contains(seamWords(win[:join]), "Über-Datum. und") {
		t.Fatalf("the window is not printed as the model reads it: %q", seamWords(win[:join]))
	}
	joined := "Das heißt, hier habe ich entsprechend das Datum der Vorlesung."
	cut, why := seamCutOf(win, join, joined)
	if why != "" {
		t.Fatalf("a fold refused the join: %s", why)
	}
	if cut.Before != join || cut.After != 0 {
		t.Fatalf("took %d off BEFORE and %d off AFTER, want %d and 0", cut.Before, cut.After, join)
	}
	var keep []srcWord
	for i, w := range win {
		if i >= join-cut.Before && i < join+cut.After {
			continue
		}
		keep = append(keep, w)
	}
	if got := seamWords(keep); got != joined {
		t.Errorf("\n got  %q\n want %q", got, joined)
	}
}

// seamFixture is one join: the end of a take, the start of the next, and where
// between them the recording stopped.
func seamFixture(before, after string) ([]srcWord, int) {
	var w []srcWord
	at := 0.0
	for i, part := range []string{before, after} {
		for _, x := range strings.Fields(part) {
			w = append(w, srcWord{s: at, e: at + 0.3, w: bareWord(x), raw: x,
				src: []string{"t1", "t2"}[i]})
			at += 0.4
		}
		at += 4 // the recording stopped and started again
	}
	return w, len(strings.Fields(before))
}

// final.txt is marked at every join, because a join is the only place in it
// where anything can be wrong. Reading 29 marks and the words either side of
// each is a check a person can do; reading five thousand words of continuous
// prose is not, and the whole of one session was spent finding faults that way.
func TestTheFinishedTextIsMarkedAtEveryJoin(t *testing.T) {
	words := append(seamSaid("one two three", "t1"),
		append(seamSaid("four five", "t2"), seamSaid("six seven", "t3")...)...)
	drop := make([]bool, len(words))
	drop[2] = true // "three" goes, at the first join
	a := &App{}
	got := a.finishedText(words, drop)
	want := "one two |cut 1| four five |cut| six seven"
	if got != want {
		t.Errorf("\n got  %q\n want %q", got, want)
	}
	// a word the transcript spelled as part of the one before it prints
	// nothing: its spelling is already there. The subtitles skip it the same
	// way; this file used to print its bare heard form as a second copy.
	folded := []srcWord{
		{w: "public", raw: "Public-Key-Pairs.", src: "t1"},
		{w: "key", raw: "", src: "t1"},
		{w: "pairs", raw: "", src: "t1"},
		{w: "eine", raw: "Eine", src: "t1"},
	}
	if got := a.finishedText(folded, nil); got != "Public-Key-Pairs. Eine" {
		t.Errorf("a folded spelling is printed twice: %q", got)
	}
	// ...and the prompt shows it once too, the compound owning its words
	// (seamRoots)
	if got := seamWords(folded); got != "Public-Key-Pairs. Eine" {
		t.Errorf("the prompt shows a folded word twice: %q", got)
	}
	// a mark says how many words went, and says so even when none did: a
	// stumble the pass walked past reads as ordinary prose otherwise
	if !strings.Contains(got, "|cut 1|") || !strings.Contains(got, "|cut|") {
		t.Error("the marks do not tell a repaired join from an untouched one")
	}
	// ...and they come back out on the way in, so the file can be edited by
	// hand with them left in (marksFromText -> textTokens)
	if toks := textTokens(got); strings.Join(toks, " ") != "one two four five six seven" {
		t.Errorf("the marks are read back as words: %q", toks)
	}
	// however a hand mangles one, it is still a mark: no word anyone says has
	// a pipe in it
	for _, mangled := range []string{"|cut", "cut|", "||", "|cut 12|"} {
		if toks := textTokens("one " + mangled + " two"); len(toks) != 2 {
			t.Errorf("%q was read as a word: %q", mangled, toks)
		}
	}
}

// seamSaid is one take's words.
func seamSaid(text, src string) []srcWord {
	var out []srcWord
	for _, x := range strings.Fields(text) {
		out = append(out, srcWord{w: bareWord(x), raw: x, src: src})
	}
	return out
}
