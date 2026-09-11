package main

import (
	"regexp"
	"strings"
	"testing"
)

// What every job is told about the material, and that no job assembles a system
// message any other way. The point of the prompt is not that it exists -- it is
// that there is one of it: a wording added tomorrow is told how a stamp reads
// without its author having to remember to say so.
func TestEveryJobIsToldTheFormatsOnce(t *testing.T) {
	ownConfig(t)
	a := &App{}

	// it is a prompt like the others, and it is the first of them: the bench
	// lists them in the order they are sent, and this one is sent in front of
	// all of them
	if d := promptDefFor("system"); strings.TrimSpace(d.def) != strings.TrimSpace(sysSystem) {
		t.Fatal(`prompt "system" is not the registered system context`)
	}
	if promptDefs[0].key != "system" {
		t.Errorf("the system context is registered %q, not first -- the registry order "+
			"is the order the bench shows and the order the calls go out in", promptDefs[0].key)
	}

	first := strings.SplitN(strings.TrimSpace(sysSystem), "\n\n", 2)[0]
	for _, key := range []string{"describe", "fix", "cut", "captions", "effects", "narrate", "youtube"} {
		got := a.sysPrompt(key)
		if !strings.HasPrefix(got, first) {
			t.Errorf("the %s job is not told the formats first:\n%s", key, short(got))
		}
		// the parts every job reads, whichever job it is
		for _, want := range []string{"\nTHE ANSWER\n", "\nTHE MATERIAL\n", "\nTHE CLOCKS\n", "\nNEVER INVENT\n"} {
			if !strings.Contains(got, want) {
				t.Errorf("the %s job is not sent the %q section", key, strings.TrimSpace(want))
			}
		}
		if !strings.HasSuffix(got, a.prompt(key)) {
			t.Errorf("the %s job's own prompt is not what follows the context", key)
		}
	}

	// and editing the box reaches every one of them, which is the only reason
	// it is a box and not a const
	a.setPrompt("system", "stamps are in minutes")
	for _, key := range []string{"cut", "narrate"} {
		if !strings.HasPrefix(a.sysPrompt(key), "stamps are in minutes") {
			t.Errorf("the edited system context is not what the %s job is sent", key)
		}
	}
	// emptied, it is the built-in again -- the same answer every other prompt
	// gives to an empty box, and the same one the store gives to an empty file
	// (promptstore.go). It used to depend on whether the box had been edited
	// before it was cleared: a blank typed into an untouched box read as the
	// built-in and a blank typed over an edit read as "send nothing", and the
	// second of those did not survive a restart. Reset is how the built-in
	// comes back on purpose.
	a.setPrompt("system", "   ")
	if got := a.prompt("system"); got != strings.TrimSpace(sysSystem) {
		t.Errorf("an emptied system context reads as %q, want the built-in", short(got))
	}
}

// Every system message in the app is built through the one seam. A job that
// reaches for its prompt directly is a job that is never told the formats, and
// it would look exactly like the others until its answers came back in the
// wrong clock.
func TestNoJobAssemblesItsOwnSystemMessage(t *testing.T) {
	for file, key := range map[string]string{
		"describe.go":    "describe",
		"transcript.go":  "fix",
		"cut_suggest.go": "cut",
		"narrate.go":     "narrate",
		"publish.go":     "youtube",
	} {
		src := readSrc(t, file)
		if !strings.Contains(src, `a.sysPrompt("`+key+`")`) {
			t.Errorf(`%s no longer sends a.sysPrompt(%q)`, file, key)
		}
		for _, raw := range []string{`msg("system", a.prompt("` + key, `system := a.prompt("` + key} {
			if strings.Contains(src, raw) {
				t.Errorf(`%s builds a system message out of a.prompt(%q), so that job is `+
					"the one that is never told how a stamp reads", file, key)
			}
		}
	}
}

// The wordings stop repeating what the context now says. Leaving both in is not
// wrong to read, but it is what the change was for: the sentences drifted apart
// precisely because they were written several times over.
func TestTheCutWordingDoesNotRepeatTheSystemContext(t *testing.T) {
	for _, gone := range []string{"[12:04] EVENT", "counting past 59", "4350 seconds"} {
		if strings.Contains(cutSystem, gone) {
			t.Errorf("the cut wording still explains %q itself, which the system "+
				"context says to every job already", gone)
		}
	}
	if !strings.Contains(sysSystem, `{"segments":`) {
		t.Error("the system context lost the cut's reply shape, which the wording no longer spells")
	}
}

// shippedPrompts is every wording the binary ships, the system context aside:
// what a job is sent behind that context, and therefore what must not say
// again what the context already said.
func shippedPrompts() []struct{ name, text string } {
	return []struct{ name, text string }{
		{"describe", describeSystem},
		{"fix", fixSystem},
		{"cut", cutSystem},
		{"speed", speedSystem},
		{"captions", captionSystem},
		{"effects", fxRules},
		{"cut (shared reply)", cutReply},
		{"narrate", narrSystem},
		{"youtube", youtubeSystem},
	}
}

// No wording says a shared rule again. The formats were the easy half and are
// already guarded above; these are the three that had spread furthest, and they
// are the ones that rot quietest -- a format written twice announces itself the
// moment a model answers on the wrong clock, while "never invent" written seven
// times is invisible until the seven have drifted apart, which they had: "never
// invent a moment", "never invent a part, a name or a price", "invent nothing
// that is not in it", "Invent no names, places or outcomes". One rule the model
// met six times, and six places to edit to tighten it once.
func TestNoWordingRepeatsASharedRule(t *testing.T) {
	// how the answer is read: the context's first paragraph
	machineRead := []string{"strict JSON, nothing else", "no markdown", "no code fence", "no code fences"}
	// what may be made up: the context's last
	invented := regexp.MustCompile(`(?i)never invent|invent no|invent nothing`)

	for _, p := range shippedPrompts() {
		for _, gone := range machineRead {
			if strings.Contains(p.text, gone) {
				t.Errorf("the %s wording says %q itself -- the system context tells every "+
					"job the answer is machine-read already", p.name, gone)
			}
		}
		if m := invented.FindString(p.text); m != "" {
			t.Errorf("the %s wording says %q -- not inventing is the system context's "+
				"last paragraph, and it is the rule that must have exactly one home", p.name, m)
		}
	}
	// ...and the context does still say all three, or the above passes because
	// nothing anywhere says them
	for _, want := range []string{"no markdown", "no code fence", "strict JSON", "Never invent"} {
		if !strings.Contains(sysSystem, want) {
			t.Errorf("the system context never says %q, and the wordings were emptied of it "+
				"on the promise that it does", want)
		}
	}
	// the notes outranking what a job would infer is said by the block that
	// carries them, in the request itself -- so the context does not say it a
	// second time, and this is the one place that has to still be true
	if !strings.Contains(readSrc(t, "prompts.go"), "outranks anything you infer from the material") {
		t.Error("nothing tells a job that the session notes outrank what it would infer")
	}
}

// The context says what the four steps ARE, not just what this one is. A job
// that knows it is second of four writes for the third: the describing step is
// writing the only record of the footage the cut will ever see, and the cut is
// choosing seconds the narration will have to talk over. A job that thinks it
// is alone writes for nobody.
func TestTheSystemContextNamesTheWholePipeline(t *testing.T) {
	for _, s := range steps {
		if !strings.Contains(sysSystem, s.label) {
			t.Errorf("the system context never names the %s step, so a job in it is not "+
				"told what becomes of its answer", s.label)
		}
	}
	// the mechanism every step shares, said here so no wording has to: the two
	// clocks a finished video has
	if !strings.Contains(sysSystem, "a time in the video is not a time in the session") {
		t.Error("the system context no longer distinguishes the cut's clock from the session's")
	}
	// ...and what each KIND of effect does is not shared: it is said in the
	// pass that writes it, because that is the only pass that can act on it.
	// The context used to define all five to everybody, so the speed pass --
	// which answers one number per clip -- was told what a zoom does and what
	// a caption is before being asked for a rate.
	for _, c := range []struct{ kind, in string }{
		{"zoom punches in", fxRules},
		{"stop holds the picture still", fxRules},
		{"volume sets how loud", fxRules},
	} {
		if !strings.Contains(c.in, c.kind) {
			t.Errorf("the pass that writes it no longer says %q", c.kind)
		}
		if strings.Contains(sysSystem, c.kind) {
			t.Errorf("the system context still defines %q for every job", c.kind)
		}
	}
	// the rate is the one thing the context does say about a kind, because it
	// is the SCHEMA -- what the number means, where the number is returned
	if !strings.Contains(speedSystem, "Below 1 is slow motion") {
		t.Error("the speed pass no longer says what a rate under 1 does")
	}
}

// The system context is the formats, and the formats are the formats: it is
// what every job is told before its own wording, and nothing about a session
// changes how a stamp reads. It is one prompt like every other now -- the
// wording machinery it was kept out of is gone (prompts.go) -- so what is left
// to hold is that it is on the bench, editable, and named without ceremony.
func TestTheSystemContextIsOnTheBenchLikeTheRest(t *testing.T) {
	ownConfig(t)
	a := &App{}
	if got := a.prompt("system"); got != strings.TrimSpace(sysSystem) {
		t.Errorf("the system context is not what a job is sent: %s", short(got))
	}
	found := false
	for _, name := range a.prepEditNames() {
		if name == "System context" {
			found = true
		}
		if strings.Contains(name, "(") {
			t.Errorf("a bench row reads %q, naming a wording nothing can pick", name)
		}
	}
	if !found {
		t.Errorf("the system context is not on the bench: %v", a.prepEditNames())
	}
}

// The context is sectioned. It is the longest thing the model reads and it is
// read by a 27B one: nine blocks of prose with nothing between them is nine
// blocks it weighs equally, where a heading says which of them a question
// belongs to. They are the app's own vocabulary, so a section that is renamed
// here is a section somebody has to find again in every wording.
func TestTheSystemContextIsUnderHeadings(t *testing.T) {
	for _, h := range []string{
		"THE ANSWER",     // what comes back, and nothing around it
		"THE MATERIAL",   // the three labels
		"THE CLOCKS",     // the three stamps
		"THE FOUR STEPS", // where this job sits
		"THE CUT",        // segments and effects
		"WHAT EACH JOB IS GIVEN",
		"TOOLS",
		"NEVER INVENT",
	} {
		if !strings.Contains(sysSystem, "\n"+h) {
			t.Errorf("the system context has no %q section", h)
		}
	}
	// each heading is on its own line, or it is a phrase in a paragraph and
	// not a heading at all
	for _, line := range strings.Split(sysSystem, "\n") {
		if strings.HasPrefix(line, "THE ") || strings.HasPrefix(line, "NEVER ") {
			if strings.HasSuffix(line, ".") {
				t.Errorf("%q is a sentence, not a heading", line)
			}
		}
	}
	// the notes have no section here at all: everything about them travels
	// with them, in the request (ctxBlock), so a session with an empty notes
	// box sends none of it
	if strings.Contains(sysSystem, "ABOUT THIS SESSION") {
		t.Error("the system context describes the notes block to jobs that may get no notes")
	}
}

// TestEachJobIsSentOnlyTheSectionsItCanUse: the box is one text and is sent
// whole to nobody. The describing step used to be handed the cut's reply
// shape and the narration's emotions; the transcript fixer was told what a
// zoom does -- seven kilobytes in front of every call, of which a describe
// call could use two. The sections go by heading, and the jobs list keeps only
// the job's own line.
func TestEachJobIsSentOnlyTheSectionsItCanUse(t *testing.T) {
	ownConfig(t)
	a := &App{}
	for _, c := range []struct {
		key     string
		own     string   // its own line in the jobs list
		others  []string // lines that belong to other jobs
		without []string // sections it has no use for
		with    []string // ...and the ones it must still get
	}{
		{"describe", "\n  describe:", []string{"\n  captions:", "\n  narrate:", "\n  cut:", "\n  upload text:"},
			[]string{"\nTHE CUT\n", "\nTHE FOUR STEPS\n", "\nTOOLS\n"}, nil},
		{"fix", "\n  transcript:", []string{"\n  describe:", "\n  cut:", "\n  narrate:"},
			[]string{"\nTHE CUT\n", "\nTHE FOUR STEPS\n", "\nTOOLS\n"}, nil},
		{"cut", "\n  cut:", []string{"\n  captions:", "\n  narrate:", "\n  describe:"},
			nil, []string{"\nTHE CUT\n", "\nTHE FOUR STEPS\n", "\nTOOLS\n"}},
		{"narrate", "\n  narrate:", []string{"\n  cut:", "\n  captions:"},
			[]string{"\nTHE CUT\n"}, []string{"\nTHE FOUR STEPS\n", "\nTOOLS\n", "a time in the video is not a time in the session"}},
		{"youtube", "\n  upload text:", []string{"\n  cut:", "\n  narrate:"},
			[]string{"\nTHE CUT\n"}, []string{"\nTHE FOUR STEPS\n", "\nTOOLS\n", "a time in the video is not a time in the session"}},
	} {
		got := a.sysPrompt(c.key)
		if !strings.Contains(got, c.own) {
			t.Errorf("%s is not told its own answer shape", c.key)
		}
		for _, o := range c.others {
			if strings.Contains(got, o) {
				t.Errorf("%s is told another job's answer shape: %q", c.key, strings.TrimSpace(o))
			}
		}
		for _, w := range c.without {
			if strings.Contains(got, w) {
				t.Errorf("%s is sent the %q section, which it has no use for", c.key, strings.TrimSpace(w))
			}
		}
		for _, w := range c.with {
			if !strings.Contains(got, w) {
				t.Errorf("%s is not sent %q, which it needs", c.key, strings.TrimSpace(w))
			}
		}
		// and it is smaller than the whole box, or the filter did nothing
		if len(got) >= len(sysSystem)+len(a.prompt(c.key)) {
			t.Errorf("%s is sent the whole box: %d bytes", c.key, len(got))
		}
	}
	// the system context itself is never cut down: the bench shows the box
	if got := a.sysPrompt("system"); !strings.Contains(got, "\n  captions:") || !strings.Contains(got, "\nTHE CUT\n") {
		t.Error("the system context's own assembly is filtered, so the bench would show a box missing its sections")
	}
	// a section somebody adds to the box under a heading this does not know
	// goes to every job: the safe reading of an unknown section is that it
	// matters. And an edit with no headings at all goes whole.
	a.setPrompt("system", strings.TrimSpace(sysSystem)+"\n\nHOUSE RULE\nName the game every time.")
	for _, key := range []string{"describe", "fix", "cut", "narrate"} {
		if !strings.Contains(a.sysPrompt(key), "HOUSE RULE\nName the game every time.") {
			t.Errorf("%s did not get a section the user added to the box", key)
		}
	}
	a.setPrompt("system", "stamps are in minutes")
	if !strings.HasPrefix(a.sysPrompt("describe"), "stamps are in minutes") {
		t.Error("a box with no headings was not sent whole")
	}
	// the speech rule rides under the context only where speech is decided
	a.setSessionCtx("Beans = purple")
	for _, key := range []string{"cut", "narrate"} {
		if !strings.Contains(a.ctxBlockFor(key), "The speech is content") {
			t.Errorf("%s decides what to do with speech and is not told the rule", key)
		}
	}
	for _, key := range []string{"describe", "fix", "youtube", "captions", "effects"} {
		if strings.Contains(a.ctxBlockFor(key), "The speech is content") {
			t.Errorf("%s never decides what to do with speech and is told the rule anyway", key)
		}
	}
	for file, key := range map[string]string{"describe.go": "describe", "transcript.go": "fix",
		"publish.go": "youtube", "narrate.go": "narrate"} {
		if !strings.Contains(readSrc(t, file), `a.ctxBlockFor("`+key+`")`) {
			t.Errorf("%s does not ask for its own job's block", file)
		}
	}
}
