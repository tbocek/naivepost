package main

import (
	"github.com/diamondburned/gotk4/pkg/glib/v2"
	"github.com/diamondburned/gotk4/pkg/gtk/v4"
)

// The header bar re-fits itself on width: path and tab words, then file name
// and words, then name and icons. The tabs keep their words longest (the most
// pressed control); the project label gives up its directories first (the
// whole path is one hover away).

// tabGap is the space between a tab's icon and its word. Here rather than at
// the gtk.NewBox call because fitHeader has to price the word AND the gap that
// disappears with it, and two constants that must agree is one too many.
const tabGap = 6

// headSlack is what the bar spends on what we do not measure: its own padding,
// the gaps around the centered title widget, and the window controls the
// desktop draws at the right end -- one button under GNOME, up to three
// elsewhere, and not ours to ask about. Deliberately generous: guessing high
// costs the bar a word it could have shown, guessing low clips one it showed.
const headSlack = 90

// headFit is what a bar with avail pixels left over can afford, given what the
// tab words, the project's file name and its whole path would each take. Split
// out from the measuring so the ladder above can be read -- and tested --
// without a display attached.
func headFit(avail, words, name, path int) (showWords, showPath bool) {
	switch {
	case avail >= words+path:
		return true, true
	case avail >= words+name:
		return true, false
	}
	return false, false
}

// textWidth is how wide s would draw in w's font. Asking Pango rather than
// measuring the widget: the label we are pricing is ellipsized and capped, so
// its own measurement answers "how wide are you allowed to be", which is the
// question we are trying to decide.
func textWidth(w gtk.Widgetter, s string) int {
	px, _ := gtk.BaseWidget(w).CreatePangoLayout(s).PixelSize()
	return px
}

// fitHeader prices the bar as it stands and applies headFit's answer.
func (a *App) fitHeader() {
	if a.head == nil || a.projLabel == nil || a.tabRow == nil {
		return // headless (tests), or called while the bar is still being built
	}
	avail := a.head.Width()
	if avail <= 0 {
		return // not laid out yet; the first resize brings us straight back
	}
	for _, b := range a.headBtns {
		_, nat, _, _ := gtk.BaseWidget(b).Measure(gtk.OrientationHorizontal, -1)
		avail -= nat
	}
	// What the words cost, and what the tab row costs without them. Measuring
	// the row as it stands and subtracting the words we know are in it prices
	// both states from either one, and needs no guess at a button's padding.
	// The gap goes with the word: GTK only spaces visible children.
	words := 0
	for i, w := range a.tabWords {
		words += textWidth(w, steps[i].label) + tabGap
	}
	_, tabs, _, _ := a.tabRow.Measure(gtk.OrientationHorizontal, -1)
	if a.tabWordsOn {
		tabs -= words
	}
	name, path, _ := projLabelText(a.projPath)
	showWords, showPath := headFit(avail-tabs-headSlack, words,
		textWidth(a.projLabel, name), textWidth(a.projLabel, path))

	if showWords != a.tabWordsOn {
		a.tabWordsOn = showWords
		for _, w := range a.tabWords {
			w.SetVisible(showWords)
		}
	}
	if showPath {
		// measured to fit before it was set, so there is nothing to cap
		a.projLabel.SetMaxWidthChars(-1)
		a.projLabel.SetText(path)
		return
	}
	// the cap is the backstop for the rung below the last one: a file name too
	// long even for a bar showing icons ends in "…" rather than pushing the
	// tabs off center
	a.projLabel.SetMaxWidthChars(projNameChars)
	a.projLabel.SetText(name)
}

// fitHeaderIdle re-fits once the resize that asked for it has finished.
// Straight from the notify handler would mean measuring widgets and changing
// their visibility from inside GTK's own allocation pass, which is how a
// layout loop starts.
func (a *App) fitHeaderIdle() {
	if a.headFitQ {
		return // one queued fit answers any number of width changes
	}
	a.headFitQ = true
	glib.IdleAdd(func() {
		a.headFitQ = false
		a.fitHeader()
	})
}

// watchHeadWidth re-fits the bar whenever the window's width changes.
//
// The window's own default-width property is not the one to watch: it holds
// the size the window would have if it were not maximized, so maximizing --
// the one resize that hands the bar a lot of room at once -- would not report.
// The surface under the window carries the width actually being drawn, and it
// exists only once the window is realized, which is why this waits.
func (a *App) watchHeadWidth() {
	a.win.ConnectRealize(func() {
		if a.headWatch {
			return // a window can be realized again; the handler must not stack
		}
		n := gtk.BaseWidget(a.win).Native()
		if n == nil {
			return
		}
		s := n.Surface()
		if s == nil {
			return
		}
		a.headWatch = true
		s.NotifyProperty("width", a.fitHeaderIdle)
	})
}
