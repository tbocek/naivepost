package main

import (
	"strings"
	"testing"
	"time"
)

// The chain is a LIST that endRun advances: the step that finished hands it
// on, and nothing about a step has to know it is in one.
func TestTheChainIsAdvancedByTheStepThatFinished(t *testing.T) {
	if !strings.Contains(funcBody(t, "runqueue.go", `func \(a \*App\) endRun\(`), "defer a.chainDone()") {
		t.Error("a finished step does not hand the chain on")
	}
	// ...and a stopped run abandons the rest: a chain saves presses, it does
	// not carry on past the answer going wrong
	done := funcBody(t, "runchain.go", `func \(a \*App\) chainDone\(`)
	end := funcBody(t, "runchain.go", `func \(a \*App\) chainEnd\(`)
	if !strings.Contains(done, "a.stopFlag.Load()") || !strings.Contains(end, "a.chain, a.chainStep = nil, \"\"") {
		t.Error("⏹ does not end the chain")
	}
	// the steps are the pipeline's order, not the tick order
	var pages []string
	for _, s := range chainSteps {
		pages = append(pages, s.page)
	}
	if strings.Join(pages, ",") != "prep,cut,narrate,produce" {
		t.Errorf("the chain runs %v", pages)
	}
	// every step names a page the dispatch actually knows
	run := funcBody(t, "pipeline.go", `func \(a \*App\) runPageNow\(`)
	for _, s := range chainSteps {
		if !strings.Contains(run, `case "`+s.page+`":`) {
			t.Errorf("the chain cannot run %q", s.page)
		}
	}
}

// Two steps skip themselves rather than halting the chain: a video with no
// narration has nothing to narrate, and a cut with hand edits IS the answer --
// Cut refuses over them, and in a chain that refusal would be a halt nobody
// asked for.
func TestTheChainSkipsWhatHasNothingToDo(t *testing.T) {
	body := funcBody(t, "runchain.go", `func \(a \*App\) chainNext\(`)
	for _, want := range []string{
		`page == "narrate" && a.narrOff`,
		"Narrate skipped",
		`page == "cut" && a.ed != nil`,
		"!sameCut(a.ed.segs, a.ed.base.segs)",
		"Cut skipped",
	} {
		if !strings.Contains(body, want) {
			t.Errorf("the chain does not skip cleanly: want %q", want)
		}
	}
	// a skip continues the loop rather than returning: the rest of the chain
	// still has work in it
	if strings.Count(body, "continue") < 3 {
		t.Error("a skipped step ends the chain instead of passing it on")
	}
	// ...and a step that declined to start carries the chain itself, since no
	// endRun is coming for it
	if !strings.Contains(body, "if !a.running {") {
		t.Error("a step that never started leaves the chain hanging")
	}
}

// Prepare alone to begin with: it is the one step a fresh project can run, and
// the only one whose inputs are on disk before anything else has happened. The
// rest are a tick away. A project written before this has no answer and reads
// the same way.
func TestTheChainDefaultsToPrepareAlone(t *testing.T) {
	a := &App{}
	a.applyChain(nil) // safe before the ticks exist: a project loads first
	if len(a.chainSteps) != 0 {
		t.Errorf("an absent answer was stored as %v", a.chainSteps)
	}
	src := readSrc(t, "runchain.go")
	if !strings.Contains(src, "c.SetActive(s.page == chainSteps[0].page)") {
		t.Error("a fresh project does not start with the first step ticked, and only that")
	}
	if !strings.Contains(src, "t.SetActive(page == chainSteps[0].page)") {
		t.Error("a project with no answer does not get the first step")
	}
	for _, want := range []string{`RunSteps []string `, "RunSteps:   a.chainPicked(),", "a.applyChain(p.RunSteps)"} {
		if !strings.Contains(readSrc(t, "project.go"), want) {
			t.Errorf("project.go does not contain %q", want)
		}
	}
}

// ▶ and its menu are one control in two halves -- the split button's shape:
// press ▶ for the thing, press the arrow for which thing. There is one thing
// to press on this bar, and two play buttons beside each other -- one for this
// page, one for several -- was one too many. GTK4 has no
// AdwSplitButton without libadwaita and does not need one: a linked box of a
// button and a menu button is that control, and it is what the tab row and the
// frame stepper on this window are already made of.
func TestPlayAndItsMenuAreOneSplitControl(t *testing.T) {
	body := funcBody(t, "runchain.go", `func \(a \*App\) buildChainMenu\(`)
	if !strings.Contains(body, "linked(a.playBtn, a.chainPick)") {
		t.Error("▶ and its menu are two loose buttons, not one control")
	}
	// ...and there is ONE of them: ▶ runs what is ticked, wherever you are
	if strings.Contains(readSrc(t, "main.go"), "ctlRow.Append(a.playBtn)") {
		t.Error("the run bar appends ▶ separately, so there are two play buttons")
	}
	if !strings.Contains(funcBody(t, "pipeline.go", `func \(a \*App\) playClicked\(`), "a.chainRun()") {
		t.Error("▶ does not run the ticked steps")
	}
	// the house helper, not a hand-rolled box: three places already draw a
	// segmented control and they must not drift apart
	if !strings.Contains(readSrc(t, "widgets.go"), `b.AddCSSClass("linked")`) {
		t.Error("the linked helper no longer makes a segmented control")
	}
}

// A chain is pressed for the WAIT: four steps of it is most of an hour, and
// the one thing to say at the end is how long that was, and where it went.
func TestTheChainSaysHowLongItTook(t *testing.T) {
	for _, c := range []struct {
		d    time.Duration
		want string
	}{
		{9 * time.Second, "9s"},
		{59500 * time.Millisecond, "60s"}, // still seconds until a minute has passed
		{time.Minute + 4*time.Second, "1m 04s"},
		{72 * time.Minute, "72m 00s"},
	} {
		if got := chainTime(c.d); got != c.want {
			t.Errorf("chainTime(%s) = %q, want %q", c.d, got, c.want)
		}
	}
	// each step is timed where the chain hands on, not by the step: what a
	// chain is about is the whole press
	done := funcBody(t, "runchain.go", `func \(a \*App\) chainDone\(`)
	if !strings.Contains(done, "chainTime(time.Since(a.chainAt))") || !strings.Contains(done, "a.chainRan = append") {
		t.Error("a finished step is not timed")
	}
	next := funcBody(t, "runchain.go", `func \(a \*App\) chainNext\(`)
	if !strings.Contains(next, "a.chainStep, a.chainAt = page, time.Now()") {
		t.Error("a step that starts does not start a clock")
	}
	// ...and a step that never ran does not leave its marker behind, or the
	// next press on a page of its own is counted into this chain
	if !strings.Contains(next, "a.chainStep = \"\"") {
		t.Error("a declined step leaves the chain's clock running on it")
	}
	// the end says the total, what each step cost, and what it did not reach
	end := funcBody(t, "runchain.go", `func \(a \*App\) chainEnd\(`)
	for _, want := range []string{
		"all done in %s — %s",
		"stopped after %s",
		"step(s) left undone",
		"time.Since(a.chainFrom)",
		"a.setStatus(",
	} {
		if !strings.Contains(end, want) {
			t.Errorf("the end of a chain does not say %q", want)
		}
	}
	// one step is not a chain: it already said what it did, and "Prepare
	// 9m 12s — total 9m 12s" is the same number twice
	if !strings.Contains(end, "len(a.chainRan) == 1 && left == 0") {
		t.Error("a chain of one still prints a summary")
	}
	// every way out of the chain goes through it, including the one where
	// every step skipped itself
	if !strings.Contains(next, `a.chainEnd("done")`) {
		t.Error("a chain emptied by skips alone never reports")
	}
	if !strings.Contains(done, `a.chainEnd("stopped")`) {
		t.Error("⏹ ends the chain without saying how long it ran")
	}
}

// Moving to a page is more than showing it: showStep swaps the tab row and the
// two bars, and rebuilds the cut's tracks when a run has moved what they are
// drawn from -- synchronously, before anything presses that page's button.
// Setting the stack's child alone left the cut asking about a session it had
// not loaded, so a chain that finished Prepare reached Cut with an editor that
// still had no recordings in it and the step came to nothing.
func TestTheChainMovesPagesTheWayTheTabsDo(t *testing.T) {
	body := funcBody(t, "runchain.go", `func \(a \*App\) chainNext\(`)
	if !strings.Contains(body, "a.showStep(page)") {
		t.Error("the chain does not move the page the way a tab click does")
	}
	if strings.Contains(body, "a.stack.SetVisibleChildName(") {
		t.Error("the chain sets the stack's child directly, which skips the page's own catching up")
	}
	// ...and showStep is what rebuilds the tracks, synchronously
	show := funcBody(t, "main.go", `func \(a \*App\) showStep\(`)
	if !strings.Contains(show, "a.updateCutInfo()") {
		t.Error("showStep no longer brings the cut's tracks up to date")
	}
	if !strings.Contains(show, `if name == "cut" && a.ed != nil {`) {
		t.Error("showStep no longer knows which page needs rebuilding")
	}
}
