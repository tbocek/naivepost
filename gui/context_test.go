package main

// The session context, without a display. Two things can go wrong here and
// neither one announces itself: the box stops reaching the requests, or a
// project stops storing it. Both leave a tool that works, produces slightly
// worse output, and gives no reason.

import (
	"encoding/json"
	"os"
	"regexp"
	"strings"
	"testing"
)

func TestCtxBlockIsEmptyWhenTheBoxIs(t *testing.T) {
	a := &App{}
	if got := a.ctxBlock(); got != "" {
		t.Errorf("an untouched context produced %q, want nothing at all", got)
	}
	a.setSessionCtx("  \n\t ") // whitespace is not an instruction
	if got := a.ctxBlock(); got != "" {
		t.Errorf("a whitespace context produced %q, want nothing at all", got)
	}
	a.setSessionCtx("  Beans was told not to open the chest.\n")
	got := a.ctxBlock()
	if !strings.Contains(got, "Beans was told not to open the chest.") {
		t.Errorf("the context did not reach the block: %q", got)
	}
	if !strings.HasPrefix(got, "USER CONTEXT") {
		t.Errorf("block does not open with the heading the prompts name: %q", short(got))
	}
	if !strings.HasSuffix(got, "\n\n") {
		t.Errorf("block does not end in a blank line, so it runs into the material: %q", short(got))
	}
}

// TestEveryPromptNamesTheContextHeading: the block under this heading is
// announced once, by the system context that goes in front of every job.
// Change the heading in ctxBlock and that prompt starts describing something
// that is not there -- source-level, because nothing at run time compares the
// two. The second half is the reason it is announced only once: a job that
// wants something done with the user context says so, but none of them re-introduces
// the block, or the model is told twice what it is looking at.
func TestEveryPromptNamesTheContextHeading(t *testing.T) {
	const head = "USER CONTEXT"
	a := &App{}
	a.setSessionCtx("x")
	if !strings.HasPrefix(a.ctxBlock(), head) {
		t.Fatalf("the block no longer opens with %q: %q", head, short(a.ctxBlock()))
	}
	// the system prompt does NOT introduce it any more: the block announces
	// itself in the request, and a session whose notes box is empty sends no
	// notes and should be told nothing about them
	if strings.Contains(promptDefFor("system").def, head) {
		t.Errorf("the system context describes %q to every job, including the ones that get none", head)
	}
	if b := (&App{}).ctxBlock(); b != "" {
		t.Errorf("an empty context box still sends something: %q", b)
	}
	for _, d := range promptDefs {
		if d.key == "system" {
			continue
		}
		if strings.Contains(d.def, "block headed "+head) {
			t.Errorf("the %s wording introduces the %q block again, which the "+
				"system context already did", d.key, head)
		}
	}
}

// TestContextSurvivesTheProjectFile: it is the user's own writing and the only
// copy of it, so a save that drops it loses work outright. The two ends are
// checked in source, since building a real project needs the widgets that hold
// the interval and the frame scale.
func TestContextSurvivesTheProjectFile(t *testing.T) {
	const ctx = "Two of us, first time in the temple. Beans = the one in purple."
	b, err := json.Marshal(Project{Context: ctx})
	if err != nil {
		t.Fatal(err)
	}
	var back Project
	if err := json.Unmarshal(b, &back); err != nil {
		t.Fatal(err)
	}
	if back.Context != ctx {
		t.Errorf("context came back as %q, want %q", back.Context, ctx)
	}
	// an empty one leaves no key behind, the same as an untouched prompt
	if b, _ := json.Marshal(Project{}); strings.Contains(string(b), `"context"`) {
		t.Errorf("an empty context still wrote a key: %s", b)
	}
	// ...and the two ends are actually wired to the box
	src, err := os.ReadFile("project.go")
	if err != nil {
		t.Fatal(err)
	}
	for _, re := range []string{`Context:\s+a\.sessionCtx\(\)`, `a\.applySessionCtx\(p\.Context\)`} {
		if !regexp.MustCompile(re).Match(src) {
			t.Errorf("project.go does not match %s -- saving or loading has lost the context", re)
		}
	}

	// loading a project that has none clears the box rather than leaving the
	// previous project's context in it, which would be the worst kind of bug:
	// invisible, and it changes what every step is told
	a := &App{}
	a.applySessionCtx("the last project's context")
	a.applySessionCtx(Project{}.Context)
	if got := a.sessionCtx(); got != "" {
		t.Errorf("loading a context-less project left %q behind", got)
	}
}

// TestEveryLLMStepSendsTheContext is the one that catches the real regression:
// a new step, or a rewritten request, that quietly builds its user message
// without the block. Source-level, since calling the runners would mean calling
// the server.
func TestEveryLLMStepSendsTheContext(t *testing.T) {
	// every place a user message is built, by the file it lives in and the
	// string that starts it
	want := []struct{ file, sig string }{
		{"describe.go", `text := a.ctxBlockFor("describe")`},   // the frame describer
		{"transcript.go", `user := a.ctxBlockFor("fix")`},      // the transcript fixer
		{"cut_suggest.go", `user := a.ctxBlockFor("cut")`},     // suggest...
		{"narrate.go", `user := a.ctxBlockFor("narrate")`},     // the narration
		{"publish.go", `msg("user", a.ctxBlockFor("youtube")`}, // the upload text
	}
	for _, w := range want {
		src, err := os.ReadFile(w.file)
		if err != nil {
			t.Fatal(err)
		}
		if !strings.Contains(string(src), w.sig) {
			t.Errorf("%s builds its request without the session context (%q)", w.file, w.sig)
		}
	}
	// Suggest builds three of them; all three have to carry it
	src, err := os.ReadFile("cut_suggest.go")
	if err != nil {
		t.Fatal(err)
	}
	users := regexp.MustCompile(`(?m)^\tuser := `).FindAllString(string(src), -1)
	withCtx := regexp.MustCompile(`(?m)^\tuser := a\.ctxBlockFor\("\w+"\) \+ `).FindAllString(string(src), -1)
	if len(users) != len(withCtx) {
		t.Errorf("cut_suggest.go builds %d user messages but only %d carry the context",
			len(users), len(withCtx))
	}
}

// TestTheUserContextOutranksEveryWordingButTheMechanics: a wording is a set of
// defaults for a session nobody described, and the USER CONTEXT was written by
// the person whose video it is. Where they disagree the context wins, and every
// job's wording says so at its end -- assembled on, so an edited copy carries
// it and a wording added later cannot forget it. The system context is the one
// place it does not reach: that is how the answer is read, not how the video
// is made, and a context that could change it would break the reader.
func TestTheUserContextOutranksEveryWordingButTheMechanics(t *testing.T) {
	a := &App{}
	a.setSessionCtx("Beans = the one in purple. Caption every tower as it is named.")
	for _, d := range promptDefs {
		got := a.sysPrompt(d.key)
		has := strings.Contains(got, ctxRule)
		if d.key == "system" {
			if has {
				t.Error("the system context carries the precedence rule, so the user context can rewrite the mechanics")
			}
			continue
		}
		if !has {
			t.Errorf("%q is assembled without the precedence rule, so its defaults beat the user context", d.key)
		}
		// at the END, after the wording's own rules, where the model has them
		// in its hands -- not buried before them
		if i, j := strings.Index(got, strings.TrimSpace(a.prompt(d.key))), strings.Index(got, ctxRule); i < 0 || j < i {
			t.Errorf("%q carries the rule before its own wording", d.key)
		}
	}
	// the rule names both halves: what gives way, and what does not
	for _, want := range []string{
		"the user context wins and the rule above gives way",
		"What it does not change is the mechanics",
		"the shape of the answer, the clock, what may be invented",
	} {
		if !strings.Contains(ctxRule, want) {
			t.Errorf("the precedence rule no longer says %q", want)
		}
	}
	// and the block says the same from its side, so the two messages agree
	if b := a.ctxBlock(); !strings.Contains(b, "outranks the rules of the job") ||
		!strings.Contains(b, "are not its to change") {
		t.Errorf("the block does not claim the precedence the rule grants it: %q", short(b))
	}
	// a session with no context is told nothing about one
	none := &App{}
	for _, d := range promptDefs {
		if strings.Contains(none.sysPrompt(d.key), ctxRule) {
			t.Errorf("%q describes a user context to a session that has none", d.key)
		}
	}
	// the mechanics no longer state a tolerance the gate does not use: the
	// range is the request's, and a tenth was what sent a model into eleven
	// minutes of arithmetic
	if strings.Contains(promptDefFor("system").def, "within a tenth") {
		t.Error("the system context still promises a tenth of the target, which is tighter than the gate")
	}
	if strings.Contains(promptDefFor("system").def, "target length within a tenth") {
		t.Error("the system context promises a tolerance of its own again; the range is the request's")
	}
}
