package main

// Running the whole pipeline in one press.
//
// Each step is its own goroutine that ends on the GUI thread through endRun,
// so a chain is a LIST that endRun advances: the step that finished says what
// became of it, and the next is started or the chain is abandoned. Nothing
// about a step has to know it is in one.
//
// The steps are the pages, in the order the pipeline runs them, because that
// is the only order that makes sense: the cut reads what Prepare wrote, the
// narration is written over the cut, and the render needs both.

import (
	"fmt"
	"strings"
	"time"

	"github.com/diamondburned/gotk4/pkg/gtk/v4"
)

// the steps a chain can hold, in pipeline order: the page each runs on, and
// what to call it in the menu.
var chainSteps = []struct{ page, name string }{
	{"prep", "Prepare"},
	{"cut", "Cut"},
	{"narrate", "Narrate"},
	{"produce", "Produce"},
}

// chainRun starts a chain: the ticked steps, in order, from the first one on.
func (a *App) chainRun() {
	if a.busy() {
		return
	}
	want := a.chainPicked()
	if len(want) == 0 {
		a.setStatus("nothing ticked beside ▶ — tick the steps to run")
		return
	}
	a.chain = want
	a.chainFrom, a.chainAt = time.Now(), time.Now()
	a.chainRan = nil
	a.logf(">>> run: %s", strings.Join(a.chainNames(want), " → "))
	a.chainNext()
}

// chainTime is a stretch of a run said the way a person would: under a minute
// in seconds, over it in minutes and seconds. Not mmss -- "3:04" beside a step
// name reads as a timestamp in the session rather than as how long the step
// took, and this is the one number on the page that is a duration.
func chainTime(d time.Duration) string {
	s := d.Seconds()
	if s < 60 {
		return fmt.Sprintf("%.0fs", s)
	}
	return fmt.Sprintf("%dm %02ds", int(s)/60, int(s)%60)
}

// chainNext takes the next step off the chain and runs it, on the page it
// belongs to: a step's own ▶ works on the widgets of its page, and a page that
// was never shown has none.
func (a *App) chainNext() {
	for len(a.chain) > 0 {
		page := a.chain[0]
		a.chain = a.chain[1:]
		if page == "narrate" && a.narrOff {
			a.logf(">>> run: Narrate skipped — this video has no narration")
			continue
		}
		// Cut refuses over hand edits, and in a chain that refusal would be a
		// halt nobody asked for: the edits are the answer, so the step has
		// nothing to do and the rest of the chain still does.
		if page == "cut" && a.ed != nil && len(a.ed.segs) > 0 && !sameCut(a.ed.segs, a.ed.base.segs) {
			a.logf(">>> run: Cut skipped — the cut has hand edits, which are kept")
			continue
		}
		// showStep, not the stack: moving a page is more than showing it. It
		// swaps the tab row and the two bars, and -- the half that matters
		// here -- it REBUILDS the cut's tracks when a run has moved what they
		// are drawn from, synchronously, before anything presses that page's
		// button. Setting the stack's child alone left the cut asking about a
		// session it had not loaded: Prepare finished, the chain reached Cut
		// with an editor that still had no recordings in it, and the step
		// came to nothing.
		a.showStep(page)
		a.logf(">>> run: %s", a.pageName(page))
		a.chainStep, a.chainAt = page, time.Now()
		a.runPageNow(page)
		if !a.running {
			// the step declined -- nothing to do, or it refused -- so the
			// chain has to carry itself rather than wait for an endRun that
			// is not coming. Its clock goes with it: a step that never ran
			// has no duration, and leaving the marker set would put the next
			// press on a page of its own into this chain's tally.
			a.chainStep = ""
			continue
		}
		return
	}
	a.chainEnd("done")
}

// chainDone is endRun's half: the step that just finished either hands the
// chain on or ends it. Stopped or failed, the rest is abandoned -- a chain
// exists to save presses, not to carry on past the answer going wrong.
func (a *App) chainDone() {
	if a.chainStep == "" {
		return // no chain under way: a step pressed on its own page
	}
	// how long that step took, kept for the line at the end. Timed here rather
	// than by each step, because what a chain is about is the WAIT -- the
	// whole press, not the part of it any one page is responsible for.
	a.chainRan = append(a.chainRan, fmt.Sprintf("%s %s",
		a.pageName(a.chainStep), chainTime(time.Since(a.chainAt))))
	a.chainStep = ""
	if a.stopFlag.Load() {
		a.chainEnd("stopped")
		return
	}
	if len(a.chain) == 0 {
		a.chainEnd("done")
		return
	}
	a.chainNext()
}

// chainEnd says what the whole press cost, and what it did not get to. Only
// worth a line for a chain of more than one: a single step already says what
// it did, and "Prepare 9m 12s — total 9m 12s" is the same number twice.
func (a *App) chainEnd(how string) {
	left := len(a.chain)
	a.chain, a.chainStep = nil, ""
	if len(a.chainRan) == 0 {
		return
	}
	total := chainTime(time.Since(a.chainFrom))
	if len(a.chainRan) == 1 && left == 0 {
		a.chainRan = nil
		return
	}
	switch {
	case how == "stopped":
		a.logf(">>> run: stopped after %s — %s — %d step(s) left undone",
			total, strings.Join(a.chainRan, ", "), left)
		a.setStatus("stopped after " + total)
	case left > 0:
		a.logf(">>> run: %s — %s — %d step(s) left undone",
			total, strings.Join(a.chainRan, ", "), left)
		a.setStatus(fmt.Sprintf("ran %s, %d left undone", total, left))
	default:
		a.logf(">>> run: all done in %s — %s", total, strings.Join(a.chainRan, ", "))
		a.setStatus("all done in " + total)
	}
	a.chainRan = nil
}

func (a *App) pageName(page string) string {
	for _, s := range chainSteps {
		if s.page == page {
			return s.name
		}
	}
	return page
}

func (a *App) chainNames(pages []string) []string {
	var out []string
	for _, p := range pages {
		out = append(out, a.pageName(p))
	}
	return out
}

// chainPicked is the steps ticked, in pipeline order.
func (a *App) chainPicked() []string {
	var out []string
	for _, s := range chainSteps {
		if t := a.chainTicks[s.page]; t != nil && t.Active() {
			out = append(out, s.page)
		}
	}
	return out
}

// buildChainMenu is the ticks beside ▶ and the button that runs them. A menu
// rather than a second ▶: which steps is a standing answer, not a choice made
// on every press, and the button says which are on with the menu shut.
func (a *App) buildChainMenu() *gtk.Box {
	pop := gtk.NewPopover()
	list := gtk.NewBox(gtk.OrientationVertical, 4)
	list.SetMarginTop(6)
	list.SetMarginBottom(6)
	list.SetMarginStart(10)
	list.SetMarginEnd(10)
	a.chainTicks = map[string]*gtk.CheckButton{}
	for _, s := range chainSteps {
		s := s
		c := gtk.NewCheckButtonWithLabel(s.name)
		// Prepare alone to begin with: it is the one step a fresh project can
		// run, and the only one whose inputs are on disk before anything else
		// has happened. The rest are a tick away, and the answer is kept with
		// the project.
		c.SetActive(s.page == chainSteps[0].page)
		c.ConnectToggled(func() {
			a.syncChain()
			if !a.chainQuiet {
				a.saveProjectNow()
			}
		})
		a.chainTicks[s.page] = c
		list.Append(c)
	}
	pop.SetChild(list)

	a.chainPick = gtk.NewMenuButton()
	a.chainPick.SetPopover(pop)
	a.chainPick.SetTooltipText("Which steps ▶ runs, in this order. Narrate skips itself " +
		"when the video has no narration, and Cut skips itself when the cut has hand edits.")
	// one control in two halves, the split button's shape: press ▶ for the
	// thing, press the arrow for which thing. GTK4 has no AdwSplitButton
	// without libadwaita, and does not need one -- a linked box of a button
	// and a menu button IS that control, and is what the tab row and the
	// stepper on this window are already made of (widgets.go).
	//
	// ▶ itself is the left half: there is one thing to press on this bar, and
	// two play buttons beside each other -- one for this page, one for
	// several -- was one too many.
	box := linked(a.playBtn, a.chainPick)
	a.syncChain()
	return box
}

// syncChain puts the answer on the button.
func (a *App) syncChain() {
	if a.chainPick == nil {
		return
	}
	n := len(a.chainPicked())
	switch n {
	case 0:
		a.chainPick.SetLabel("none")
	case len(chainSteps):
		a.chainPick.SetLabel("all")
	default:
		a.chainPick.SetLabel(fmt.Sprintf("%d steps", n))
	}
}

// applyChain is a project's answer, put on the ticks without saving it back. A
// project written before this has none, and gets the first step alone -- the
// same as a new one.
func (a *App) applyChain(pages []string) {
	if a.chainTicks == nil {
		a.chainSteps = pages
		return
	}
	a.chainQuiet = true
	if pages == nil {
		for page, t := range a.chainTicks {
			t.SetActive(page == chainSteps[0].page)
		}
	} else {
		on := map[string]bool{}
		for _, p := range pages {
			on[p] = true
		}
		for page, t := range a.chainTicks {
			t.SetActive(on[page])
		}
	}
	a.chainQuiet = false
	a.syncChain()
}
