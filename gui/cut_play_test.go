package main

// ▶ is the Cut page's Suggest.
//
// Every other step is run by the run bar's ▶, and Cut used to be the exception:
// it had a "Suggest cut" button of its own, so ▶ had to mean something else
// most of the time. It meant three things by turns -- add the pending selection,
// or suggest but only into an empty cut, or print a sentence about which of the
// two you had meant -- and the last of those was the common case, which is a
// button that mostly declines to do anything.
//
// With the button gone the only way to run the step is ▶, so what these pin is
// that ▶ actually runs it: unconditionally, not "if the cut is empty", and that
// nothing else in the page still tries to be the button that started it.

import (
	"strings"
	"testing"
)

// The toolbar has no Suggest button any more. If one comes back the page has two
// ways to start the same long run again, and they disagree about hand edits the
// moment either one grows a condition.
func TestTheCutPageHasNoSuggestButtonOfItsOwn(t *testing.T) {
	src := readSrc(t, "cut.go")
	if strings.Contains(src, `NewButtonWithLabel("Suggest`) {
		t.Error("the Cut toolbar builds a Suggest button again -- ▶ in the run bar is the step's run")
	}
	// the length it runs to is not here either: it is a sentence in the user
	// context ("about 12 min"), read at the run (suggestClicked)
	if strings.Contains(src, "ed.target = gtk.NewEntry()") {
		t.Error("the target box is back on the toolbar, a second place to say the length")
	}

}

// The Cut branch is the whole of the step's run control -- ▶ reaches it
// through the steps ticked beside it now (runPageNow), rather than through
// whichever page is showing. It must call suggestClicked plainly: a condition
// here is a ▶ that silently does nothing on a cut that has segments in it,
// which is most cuts.
func TestPlayRunsSuggestOnTheCutPage(t *testing.T) {
	body := funcBody(t, "pipeline.go", `func \(a \*App\) runPageNow\(`)
	i := strings.Index(body, `case "cut":`)
	if i < 0 {
		t.Fatal("playClicked has no Cut branch")
	}
	// up to the next case label in the same switch
	rest := body[i+len(`case "cut":`):]
	if j := strings.Index(rest, `case "narrate":`); j >= 0 {
		rest = rest[:j]
	}
	if !strings.Contains(rest, "a.suggestClicked()") {
		t.Errorf("▶ on Cut does not suggest:\n%s", rest)
	}
	if strings.Contains(rest, "a.addSelClicked()") {
		t.Error("▶ on Cut still adds the pending selection -- that is ＋ Add, and it makes ▶ " +
			"unable to suggest while a region is selected")
	}
	if strings.Contains(rest, "len(a.ed.segs) == 0") {
		t.Errorf("▶ on Cut only suggests into an empty cut; with no button beside it that "+
			"leaves no way to re-suggest at all:\n%s", rest)
	}
}

// Refusing over hand edits belongs to the cut, not to the button: it is a rule
// about losing work, so it has to hold however the run was started. It lives in
// suggestClicked for that reason, and the message has to name a way out.
func TestSuggestStillRefusesToEatHandEdits(t *testing.T) {
	fn := funcBody(t, "cut_suggest.go", `func \(a \*App\) suggestClicked\(\) \{`)
	if !strings.Contains(fn, "!sameCut(a.ed.segs, a.ed.base.segs)") {
		t.Error("suggestClicked no longer compares the cut against the last suggestion, " +
			"so ▶ now silently discards hand edits")
	}
	if !strings.Contains(fn, "Revert") {
		t.Error("the refusal does not name Revert, which is the only way past it")
	}
	// Revert is an icon button now, so a message pointing at a glyph it does not
	// carry sends people looking for a control that is not there
	if strings.Contains(fn, "↺") {
		t.Error("the refusal still names the ↺ glyph the Revert button no longer shows")
	}
}
