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

// What the model reads: a breath is a line, a recording change is a SEAM, and
// a long pause says how long. The markers come back out of the answer.
func TestTheTextBriefMarksSeamsAndPauses(t *testing.T) {
	words := append(said("stay safe", 100, "t3"), said("stay unsafe forever", 105, "t4")...)
	words = append(words, srcWord{s: 106.9, e: 107.2, w: "bitbox", src: "t4"}) // a breath after "forever" (106.1)
	words = append(words, srcWord{s: 109.1, e: 109.4, w: "stated", src: "t4"}) // 1.9 s after "bitbox"
	b := textBrief(words)
	for _, want := range []string{"stay safe\nSEAM\nstay unsafe forever", "forever\nbitbox", "bitbox\n(pause 1.9s)\nstated"} {
		if !strings.Contains(b, want) {
			t.Errorf("the brief does not read %q:\n%s", want, b)
		}
	}
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
		{"cut_suggest.go", `a.cutByText(marks)`},
		{"project.go", "Style string `json:\"style,omitempty\"`"},
		{"project.go", `Style:      a.videoStyleName(),`},
		{"project.go", `a.applyStyle(p.Style)`},
		{"prompts.go", `{key: "textedit", def: strings.TrimSpace(textSystem)},`},
		{"syscontext.go", `"textedit":  sysSet(`},
		{"syscontext.go", `  textedit: every word spoken in the session`},
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
	if !strings.Contains(funcBody(t, "textedit.go", `func \(a \*App\) cutByText\(`), "trimRunToWords(") {
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
	if i, j := strings.Index(pass, "os.WriteFile(a.finalText()"), strings.Index(pass, "a.writeRetakes(marks)"); i < 0 || j < 0 || i > j {
		t.Error("the marks are written before the text, so the text always looks edited")
	}
}
