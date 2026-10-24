package main

// A video with no narration.
//
// Some videos want none: the speakers carry them, or the words go on screen
// instead. The nearest thing to saying so was the captions voice, which still
// writes lines and still asks Produce to carry them -- so the page kept a
// game-volume slider for a voice that was not there, and a subtitle question
// about a track with nothing in it.

import (
	"encoding/json"
	"strings"
	"testing"
)

func TestNoNarrationTakesAwayWhatOnlyANarrationNeeds(t *testing.T) {
	// it is the project's answer, and an older project reads as narrated
	b, err := json.Marshal(Project{NoNarration: true})
	if err != nil {
		t.Fatal(err)
	}
	var back Project
	if err := json.Unmarshal(b, &back); err != nil || !back.NoNarration {
		t.Errorf("the flag came back as %v (%v)", back.NoNarration, err)
	}
	var older Project
	if err := json.Unmarshal([]byte(`{"context":"x"}`), &older); err != nil || older.NoNarration {
		t.Error("a project written before the flag reads as un-narrated")
	}
	// the tick writes it, and both pages follow
	a := &App{}
	a.applyNarrOff(true)
	if !a.narrOff {
		t.Error("a project's answer did not reach the app")
	}
	src := readSrc(t, "narrate.go")
	for _, want := range []string{
		`n.onBox = gtk.NewCheckButtonWithLabel("Narration")`,
		"n.onBox.ConnectToggled(func() { a.setNarrOff(!n.onBox.Active()) })",
		// top left of the page, over the picture: a page's first question
		"head.SetHAlign(gtk.AlignStart)",
		"head.Append(n.onBox)",
		"top.Append(head)",
		// ...and the run refuses rather than writing lines nobody asked for
		`a.setStatus("this video has no narration — tick Narration at the top of this page to write one")`,
	} {
		if !strings.Contains(src, want) {
			t.Errorf("narrate.go no longer contains %q", want)
		}
	}
	// the refusal comes before anything is written or saved
	i := strings.Index(src, "if a.narrOff {")
	j := strings.Index(src, "n.pullRows() // an edit still sitting in a text box is part of this run")
	if i < 0 || j < 0 || i > j {
		t.Error("the narration run does not check the tick before it starts")
	}
	// Produce hides the one control that is about the voice-over alone -- how
	// loud the game sits under it. The subtitle choice STAYS: with no
	// narration the track carries what was said in the clips instead
	// (captionLines), so it is about something either way.
	prod := readSrc(t, "produce.go")
	for _, want := range []string{
		"func (a *App) syncNarrOff() {",
		"p.gvol, p.gvolLbl,",
		"defer a.syncNarrOff()", // a project that loaded before the page was built
	} {
		if !strings.Contains(prod, want) {
			t.Errorf("produce.go no longer contains %q", want)
		}
	}
	for _, gone := range []string{"p.subs, p.subsLbl, p.gvol, p.gvolLbl,", `st.Subs = "none" // no lines, no track`} {
		if strings.Contains(prod, gone) {
			t.Errorf("no narration still takes the subtitles away: %q", gone)
		}
	}
	// and it is safe before either page exists: a project loads first
	(&App{}).syncNarrOff()
}

// Off means the render behaves as if there were no narration: nothing spoken,
// nothing laid over a clip, no subtitle track. A produce run with the tick
// clear still spoke nineteen lines, because every step read the entries off
// the page rather than asking whether this video has a narration at all.
func TestNoNarrationIsReadAtTheOneSeamEveryStepUses(t *testing.T) {
	a := &App{narrOff: true}
	if got := a.produceEntries(); got != nil {
		t.Errorf("with no narration the render still reads %d entries", len(got))
	}
	// ...and that seam is where the four callers get theirs, so none of them
	// can speak a line behind the tick's back
	for _, f := range []string{"produce.go", "publish.go"} {
		src := readSrc(t, f)
		if strings.Contains(src, "a.narr.entries") && f != "produce.go" {
			t.Errorf("%s reads the page's entries directly, around produceEntries", f)
		}
	}
	body := funcBody(t, "produce.go", `func \(a \*App\) produceEntries\(\) \[\]narrEntry \{`)
	if !strings.Contains(body, "if a.narrOff {\n\t\treturn nil\n\t}") {
		t.Error("produceEntries does not answer the tick, so the speaking and the render still see lines")
	}
	// the speaking loop is fed from it, so an empty list is an empty loop
	prod := readSrc(t, "produce.go")
	i := strings.Index(prod, "entries := a.produceEntries()")
	j := strings.Index(prod, "if strings.TrimSpace(e.Text) != \"\" && !exists(a.ttsWav(e)) {")
	if i < 0 || j < 0 || i > j {
		t.Error("the render speaks from something other than produceEntries")
	}
}

// ...and the page itself goes quiet: with no narration there is nothing on it
// to do, and a page of live controls over a video that has none invites the
// work the tick just said not to do. The tick stays alive -- it is how the
// page comes back.
func TestNoNarrationGreysThePage(t *testing.T) {
	src := readSrc(t, "narrate.go")
	for _, want := range []string{
		"n.body = []gtk.Widgetter{left, preview, voice}", // the lines, the preview, the voices
		"func (a *App) syncNarrPage() {",
		"s.SetSensitive(!a.narrOff)",
	} {
		if !strings.Contains(src, want) {
			t.Errorf("narrate.go no longer contains %q", want)
		}
	}
	// the tick is not in what it greys, or it could not be pressed again
	if strings.Contains(src, "n.body = []gtk.Widgetter{left, preview, voice, n.onBox") ||
		strings.Contains(src, "n.body = []gtk.Widgetter{n.onBox") {
		t.Error("the Narration tick greys itself out, so nothing can turn it back on")
	}
	// both ways in settle the page: the tick, and a project's answer
	for _, fn := range []string{`func \(a \*App\) setNarrOff\(`, `func \(a \*App\) applyNarrOff\(`} {
		if b := funcBody(t, "narrate.go", fn); !strings.Contains(b, "a.syncNarrPage()") {
			t.Errorf("%s does not settle the page", fn)
		}
	}
	// and it is safe before the page exists: a project loads first
	(&App{narrOff: true}).syncNarrPage()
}
