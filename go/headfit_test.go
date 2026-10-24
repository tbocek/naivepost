package main

import (
	"strings"
	"testing"
)

// The ladder, priced in pixels. What it has to get right is the order: the tab
// words outrank the project's leading directories, because five icons are five
// guesses until you have learned them while a path is one hover away whatever
// the bar shows. So a bar with room for the path but not for the path AND the
// words spends it on the words.
func TestTheBarSpendsItsWidthOnWhatCannotBeGuessed(t *testing.T) {
	const words, name, path = 200, 90, 250
	for _, c := range []struct {
		avail       int
		words, path bool
		why         string
	}{
		{800, true, true, "room for everything"},
		{words + path, true, true, "exactly enough is enough"},
		{words + path - 1, true, false, "one pixel short of the path keeps the words"},
		{words + name, true, false, "exactly enough for the words and the name"},
		{words + name - 1, false, false, "short of both, the words go and the name stays"},
		{path, false, false, "room for the path alone still buys the words nothing, " +
			"and a path over icon-only tabs is the wrong trade"},
		{0, false, false, "a bar with nothing left shows the least it can"},
	} {
		gotW, gotP := headFit(c.avail, words, name, path)
		if gotW != c.words || gotP != c.path {
			t.Errorf("at %d px the bar shows words=%v path=%v, want words=%v path=%v -- %s",
				c.avail, gotW, gotP, c.words, c.path, c.why)
		}
	}
}

// Every step is reachable when the bar has run out of words, which means every
// step needs a symbol of its own. Symbolic names on purpose: they recolour with
// the theme, and the tab row is drawn over the title bar, which is not the same
// colour on every desktop.
func TestEveryStepTabHasAnIconOfItsOwn(t *testing.T) {
	seen := map[string]string{}
	for _, s := range steps {
		if s.icon == "" {
			t.Errorf("the %s tab has no icon -- on a narrow window it would be a blank button", s.name)
			continue
		}
		if !strings.HasSuffix(s.icon, "-symbolic") {
			t.Errorf("the %s tab uses %q, want a -symbolic icon that follows the theme's colour", s.name, s.icon)
		}
		if other, dup := seen[s.icon]; dup {
			t.Errorf("%s and %s both draw %q -- icon-only tabs must not be two of the same button",
				other, s.name, s.icon)
		}
		seen[s.icon] = s.name
	}
}

// The wiring under the ladder. A tab is an icon and a word in a box, the word
// is registered so it can be hidden as a group, and the gap between the two is
// the same constant fitHeader charges for the word -- GTK only spaces visible
// children, so a word that is priced without its gap is priced short by five
// gaps across the row.
func TestATabIsAnIconAndAWordThatCanGoAway(t *testing.T) {
	main := readSrc(t, "main.go")
	for _, want := range []string{
		"gtk.NewBox(gtk.OrientationHorizontal, tabGap)",
		"row.Append(gtk.NewImageFromIconName(st.icon))",
		"a.tabWords = append(a.tabWords, word)",
		"a.watchHeadWidth()",
	} {
		if !strings.Contains(main, want) {
			t.Errorf("the tab row no longer does %s", want)
		}
	}
	fit := funcBody(t, "headfit.go", `func \(a \*App\) fitHeader\(\) \{`)
	if !strings.Contains(fit, "textWidth(w, steps[i].label) + tabGap") {
		t.Error("the words are priced without the gap that disappears with them")
	}
	if !strings.Contains(fit, "w.SetVisible(showWords)") {
		t.Error("fitHeader decides about the words without hiding or showing any")
	}
	// both states priced from whichever one the row is in
	if !strings.Contains(fit, "if a.tabWordsOn {\n\t\ttabs -= words\n\t}") {
		t.Error("fitHeader measures the tab row without allowing for the words already in it -- " +
			"once the words are hidden it would price them a second time and never bring them back")
	}
}

// The label shows the path only after the path has been measured against the
// room there is, and it drops the character cap when it does: the cap is what
// keeps an over-long file NAME from walking the centered tabs sideways, and a
// path that has been measured to fit needs no such backstop -- but left on, it
// would ellipsize the path away to 28 characters of leading directories, which
// is the one thing worse than not showing it.
func TestThePathIsOnlyShownAfterItHasBeenMeasured(t *testing.T) {
	fit := funcBody(t, "headfit.go", `func \(a \*App\) fitHeader\(\) \{`)
	path := strings.Index(fit, "a.projLabel.SetText(path)")
	name := strings.Index(fit, "a.projLabel.SetText(name)")
	decide := strings.Index(fit, "headFit(")
	if path < 0 || name < 0 || decide < 0 {
		t.Fatal("fitHeader no longer decides between the name and the path")
	}
	if path < decide || name < decide {
		t.Error("the label is written before the fit has been worked out")
	}
	if !strings.Contains(fit, "a.projLabel.SetMaxWidthChars(-1)") {
		t.Error("the path is shown under the name's character cap, which would ellipsize it " +
			"down to the directories it starts with")
	}
	if !strings.Contains(fit, "a.projLabel.SetMaxWidthChars(projNameChars)") {
		t.Error("the name is shown with no cap -- a long one would push the tabs off center")
	}
	if !strings.Contains(fit, "avail <= 0") {
		t.Error("fitHeader prices a bar that has not been laid out yet, where every width is 0 " +
			"and the answer would be the narrowest rung")
	}
}

// What triggers a re-fit. The window's own default-width is the trap: it holds
// the size the window would have if it were not maximized, so watching it means
// the one resize that hands the bar the most room at once never reports. The
// surface carries the width actually drawn. And the answer is worked out on the
// idle, not inside GTK's allocation pass, which is where layout loops come from.
func TestTheBarRefitsWhenTheWindowChangesWidth(t *testing.T) {
	watch := funcBody(t, "headfit.go", `func \(a \*App\) watchHeadWidth\(\) \{`)
	if !strings.Contains(watch, `s.NotifyProperty("width", a.fitHeaderIdle)`) {
		t.Error("nothing re-fits the bar when the window is resized")
	}
	if strings.Contains(watch, "default-width") {
		t.Error("the bar follows the window's default width, which does not move when it is maximized")
	}
	if !strings.Contains(watch, "a.headWatch") {
		t.Error("realize can happen more than once and the handler would stack up")
	}
	idle := funcBody(t, "headfit.go", `func \(a \*App\) fitHeaderIdle\(\) \{`)
	if !strings.Contains(idle, "glib.IdleAdd(") {
		t.Error("the re-fit runs inside the resize that asked for it")
	}
	if !strings.Contains(idle, "if a.headFitQ {") || !strings.Contains(idle, "a.headFitQ = true") {
		t.Error("nothing turns the queued fit away -- a drag across the screen queues one per pixel")
	}
}

// The ⓘ in the header bar explains the open step the way every other ⓘ in this
// app explains its own thing: you hover it. It was a menu button with a
// popover -- a thing to click, that stayed up over the page it was explaining,
// while its own tooltip gave a second, shorter answer to the same question --
// so the one mark up there that looks like the settings page's ⓘ behaved like
// none of them.
func TestTheStepHelpIsATooltipLikeEveryOtherExplanation(t *testing.T) {
	src := readSrc(t, "main.go")
	for _, want := range []string{
		`info := gtk.NewImageFromIconName("help-about-symbolic")`,
		"a.helpInfo = info",
		`a.helpInfo.SetTooltipText(steps[i].label + "\n\n" + steps[i].help)`,
	} {
		if !strings.Contains(src, want) {
			t.Errorf("the header's ⓘ no longer explains itself by tooltip: %q", want)
		}
	}
	for _, gone := range []string{"helpPopover", `SetTooltipText("What this step does")`} {
		if strings.Contains(src, gone) {
			t.Errorf("the step help is still a popover: %q", gone)
		}
	}
	// and it follows the tabs: the page you are on is the paragraph you get
	if !strings.Contains(funcBody(t, "main.go", `func \(a \*App\) showStep\(name string\) \{`), "a.syncHelp()") {
		t.Error("changing step leaves the ⓘ on the last page's help")
	}
}
