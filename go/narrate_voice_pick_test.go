package main

import (
	"strings"
	"testing"
)

// The voice picker was a scrolling list, a filter box over it, two import
// buttons, an editable folder path and three explanatory labels stacked in a
// divider. It is a dropdown and a sample row now. These tests are what that
// collapse is allowed to have cost: the dropdown has to say everything the
// removed label said, and picking from it has to still mean choosing.

// The "Voice: narrator 1 — the dominant speaker in X.wav" label was removed
// because the dropdown's own row already reads that. That is only true while
// the rows carry the recording, so it is the thing to pin: a row reading bare
// "Narrator 1" would leave the page with nowhere that says who speaks.
func TestEveryRowSaysEnoughThatNoLabelHasToRepeatIt(t *testing.T) {
	vs := []voiceOpt{
		{id: captionsVoice, name: "No audio — captions only"},
		{id: ownVoice, name: "Narrator 1 — Kooha-2026-08-30.split-voice.wav"},
		{id: "gb_male", name: "gb_male.wav", path: "/voices/gb_male.wav"},
	}
	got := voiceNames(vs)
	if len(got) != len(vs) {
		t.Fatalf("%d rows for %d voices — the dropdown's index is read straight back as vp.voices[i]", len(got), len(vs))
	}
	for i, v := range vs {
		if got[i] != v.name {
			t.Errorf("row %d reads %q, want %q", i, got[i], v.name)
		}
	}
	// order is the contract, not a nicety: current() and choose() both turn the
	// dropdown's selected INDEX into a voice by indexing vp.voices
	if got[0] != vs[0].name || got[2] != vs[2].name {
		t.Error("voiceNames reordered the rows — the index the dropdown reports would name a different voice")
	}
	if n := len(voiceNames(nil)); n != 0 {
		t.Errorf("voiceNames(nil) = %d rows, want none — a project with no voices must not offer one", n)
	}
}

// The page is two rows: who speaks, and how it sounds. Every widget the collapse
// removed is one the user asked to be gone, and each would come back silently as
// a leftover in a later edit -- there is no run-time check that a page is simple.
func TestThePickerIsTheChoiceAndTheWayToHearIt(t *testing.T) {
	src := readSrc(t, "narrate_voice.go")
	for _, c := range []struct{ gone, why string }{
		{"gtk.NewSearchEntry", "the filter box is back — a dropdown is picked from and closed, so there is nothing to filter"},
		{"gtk.NewListBox", "the voice list is a ListBox again; it is a dropdown"},
		{"gtk.NewScrolledWindow", "something on this page scrolls again — two rows do not"},
		{"gtk.NewPaned", "the divider is back; it was there to share height between a list and the knobs"},
		{"Add folder…", "the folder import is back"},
		{`gtk.NewLabel("Folder:")`, "the voices folder is an editable row again — it is llm.conf's, with a default"},
	} {
		if strings.Contains(src, c.gone) {
			t.Error(c.why)
		}
	}
	// the removed paragraph's words survive, but as the dropdown's tooltip --
	// read at the moment the choice is being made rather than on every look at
	// the page. Where it sits is the point, so pin where and not just whether.
	if n := strings.Count(src, "Every line is spoken by cloning"); n != 1 {
		t.Errorf("%d copies of what choosing a voice does, want the one on the tooltip", n)
	} else if i := strings.Index(src, "Every line is spoken by cloning"); !strings.Contains(src[max(0, i-200):i], "vp.pick.SetTooltipText(") {
		t.Error("the explanation is a widget of its own again — it belongs on the dropdown's tooltip")
	}

	// ...and the two rows themselves, in the order the eye reads them
	body := funcBody(t, "narrate_voice.go", `func \(a \*App\) buildVoicePicker\(`)
	for _, want := range []string{
		"who.Append(vp.pick)", "who.Append(add)", // the choice
		"hear.Append(vp.sample)", "knob.Append(vp.pitch)", // and how it sounds
		// the sentence takes what the pitch does not: the slider is 150 px and
		// aligned left (formSlider), so half each spent a quarter of the row
		// on nothing and cut the sample off mid-word
		"hear.SetHExpand(true)",
		"tune.Append(hear)", "tune.Append(knob)",
		"box.Append(who)", "box.Append(tune)",
	} {
		if !strings.Contains(body, want) {
			t.Errorf("buildVoicePicker has no %q", want)
		}
	}
	if i, j := strings.Index(body, "box.Append(who)"), strings.Index(body, "box.Append(tune)"); i > j {
		t.Error("the sample row is above the voice it would be spoken in")
	}
}

// Picking is choosing, and showing is not. Everything that lands a selection
// without the user having asked for one has to raise vp.syncing first: choose()
// re-cuts a narrator's reference from the recording, so a selection set at
// startup, on a project switch or after an import would throw away the
// reference that is already there.
func TestOnlyAHandOnTheDropdownCountsAsChoosingAVoice(t *testing.T) {
	src := readSrc(t, "narrate_voice.go")
	if !strings.Contains(src, `vp.pick.NotifyProperty("selected", func() {`) {
		t.Fatal("nothing watches the dropdown, so picking a voice would do nothing")
	}
	// the handler's own guard: reload() sets a selection from inside a splice
	if !strings.Contains(src, "if vp.syncing {\n\t\t\treturn\n\t\t}\n\t\tvp.choose(int(vp.pick.Selected()))") {
		t.Error("the dropdown's handler chooses without checking vp.syncing")
	}
	for _, c := range []struct{ fn, want string }{
		{`func \(vp \*voicePicker\) syncSelection\(`, "vp.syncing = true"},
		{`func \(vp \*voicePicker\) reload\(`, "vp.syncing = true"},
	} {
		if b := funcBody(t, "narrate_voice.go", c.fn); !strings.Contains(b, c.want) {
			t.Errorf("%s lands a selection without %q — showing the stored voice would re-cut its reference", c.fn, c.want)
		}
	}
	// reload's one exception, which is the whole reason it takes the flag:
	// importing a file IS asking to speak in it
	rel := funcBody(t, "narrate_voice.go", `func \(vp \*voicePicker\) reload\(`)
	if !strings.Contains(rel, "vp.syncing = false // ...so landing on it counts as picking it") ||
		!strings.Contains(rel, "vp.choose(i)") {
		t.Error("reload(sel, true) no longer picks the voice it landed on, so an added file would not be used")
	}
	// splicing a dropdown's model from inside notify::selected hangs the view
	// (see showPromptStyle); reload is the only splice, and choose is what the
	// handler calls, so choose must not reach it
	ch := funcBody(t, "narrate_voice.go", `func \(vp \*voicePicker\) choose\(`)
	if strings.Contains(ch, "vp.reload(") || strings.Contains(ch, "vp.names.Splice") {
		t.Error("choose() rebuilds the dropdown's model — it runs inside notify::selected, where that freezes the window")
	}
}

// A project whose stored voice is not on offer must not silently point at
// whatever is first: that is how a finished video ends up in the wrong speaker.
// Nothing is selected, and the status line says which of the two ways it
// happened, since they are fixed in different places.
func TestAVoiceThatIsGoneLeavesTheDropdownShowingNothing(t *testing.T) {
	b := funcBody(t, "narrate_voice.go", `func \(vp \*voicePicker\) syncSelection\(`)
	if !strings.Contains(b, "vp.pick.SetSelected(gtk.InvalidListPosition)") {
		t.Error("a missing voice leaves the dropdown pointing at some other voice")
	}
	for _, want := range []string{
		"is not tagged on the Prepare step", // the slot has no recording
		"is no longer in %s — pick another", // the wav was deleted or the folder changed
	} {
		if !strings.Contains(b, want) {
			t.Errorf("syncSelection does not say %q, so the empty dropdown has no explanation", want)
		}
	}
	// current() reads an unsigned index, so InvalidListPosition arrives as a
	// huge positive number and only the upper bound catches it
	c := funcBody(t, "narrate_voice.go", `func \(vp \*voicePicker\) current\(`)
	if !strings.Contains(c, "i >= len(vp.voices)") {
		t.Error("current() does not bound-check the selection — with nothing picked it would index past the voices")
	}
}
