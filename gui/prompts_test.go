package main

// The prompt registry without a display. What matters here is not the wording
// but the bookkeeping around it: which prompt a step runner gets, what a
// project stores, and what loading a project puts back.

import (
	"encoding/json"
	"os"
	"path/filepath"
	"regexp"
	"sort"
	"strings"
	"testing"
)

// TestPromptFallsBackToBuiltIn: every runner asks by key, and a key that was
// never edited -- or was blanked -- must still produce a real prompt. An empty
// system message would not fail loudly; it would quietly produce worse output.
func TestPromptFallsBackToBuiltIn(t *testing.T) {
	ownConfig(t)
	a := &App{}
	for _, d := range promptDefs {
		if got := a.prompt(d.key); got != d.def {
			t.Errorf("prompt(%q) with nothing set = %q, want the built-in", d.key, short(got))
		}
		if d.def == "" {
			t.Errorf("prompt %q has no built-in text", d.key)
		}
	}
	a.setPrompt("cut", "   \n ") // an emptied box is not an instruction
	if got := a.prompt("cut"); got != promptDefFor("cut").def {
		t.Errorf("a blanked prompt returned %q, want the built-in", short(got))
	}
	a.setPrompt("cut", "Pick five clips, all of them boring.")
	if got := a.prompt("cut"); !strings.HasPrefix(got, "Pick five") {
		t.Errorf("an edited prompt did not reach the runner: %q", short(got))
	}
	// Keys are the project-file names: renaming one drops what a user wrote.
	// "align" is deliberately not here -- the content-based aligner it belonged
	// to was never wired to anything and was deleted, so an old project's
	// prompts.align is ignored on load and dropped on the next save.
	for _, want := range []string{"describe", "fix", "cut", "narrate"} {
		if promptDefFor(want).def == "" {
			t.Errorf("prompt key %q is gone -- projects that edited it lose their text", want)
		}
	}
}

// TestEveryPromptIsExposed guards the point of the registry: a prompt the tool
// sends but does not show is one the user cannot fix, and the way that happens
// is someone adding a `const fooSystem` and stopping there. Source-level on
// purpose -- there is nothing at run time that knows about a prompt nobody
// registered.
func TestEveryPromptIsExposed(t *testing.T) {
	files, err := filepath.Glob("*.go")
	if err != nil {
		t.Fatal(err)
	}
	declared := map[string]bool{}
	re := regexp.MustCompile(`(?m)^const (\w+)System = `)
	for _, f := range files {
		if strings.HasSuffix(f, "_test.go") {
			continue
		}
		b, err := os.ReadFile(f)
		if err != nil {
			t.Fatal(err)
		}
		for _, m := range re.FindAllStringSubmatch(string(b), -1) {
			declared[m[1]] = true
		}
	}
	if len(declared) == 0 {
		t.Fatal("found no system prompts at all -- the pattern went stale, not the code")
	}
	// Nothing is exempt any more: improveSystem was, and it went out with the
	// Improve button. Every prompt the binary declares is a step of an edit and
	// is on the bench.
	//
	// one prompt per job, so the two counts are the same count: a const that
	// nothing registers is a prompt the tool sends and nobody can fix
	if len(declared) != len(promptDefs) {
		t.Errorf("%d system prompts declared %v, but %d jobs are on the bench -- "+
			"a new one needs a promptDef and a row in prepRows",
			len(declared), keysOf(declared), len(promptDefs))
	}
}

// TestEveryTextBoxIsTheSameBox guards the shape, not the wording. The prompts
// and the session context are the same object to a reader -- a heading and a
// box of text, side by side on the Describe page -- and they were built by two
// functions that drifted: one box had a frame, a floor and a natural height,
// the other had a frame and nothing else, and one heading row carried a button
// while the other carried a label, so the two boxes started a button's height
// apart. Source-level, because nothing at run time notices a page looking
// wrong; the only thing that does is the person using it.
func TestEveryTextBoxIsTheSameBox(t *testing.T) {
	src := map[string]string{}
	for _, f := range []string{"prompts.go", "prepedit.go"} {
		b, err := os.ReadFile(f)
		if err != nil {
			t.Fatal(err)
		}
		src[f] = string(b)
	}

	// editorFrame is the box itself and editorBody is that box plus the size
	// group. They are two functions because the prompt editor is opened from a
	// dropdown now (promptpick.go) and a window that outlives the page must not
	// be joined to a size group the page owns -- so the shared shape has to be
	// reachable without it.
	frame := regexp.MustCompile(`(?s)func editorFrame\(.*?\n}\n`).FindString(src["prompts.go"])
	if frame == "" {
		t.Fatal("editorFrame is gone -- the two boxes are being built separately again")
	}
	for what, must := range map[string]string{
		"a border":                     `AddCSSClass("frame")`,
		"the whole text asked for":     "SetPropagateNaturalHeight(true)",
		"a four-line floor":            "SetSizeRequest(-1, 72)",
		"a taller window a taller box": "SetVExpand(true)",
	} {
		if !strings.Contains(frame, must) {
			t.Errorf("the shared box no longer gives %s (%s):\n%s", what, must, frame)
		}
	}
	body := regexp.MustCompile(`(?s)func \(a \*App\) editorBody\(.*?\n}\n`).FindString(src["prompts.go"])
	if !strings.Contains(body, "headGroup") || !strings.Contains(body, "editorFrame(head, tv)") {
		t.Errorf("editorBody no longer lines the heading rows up over the shared box:\n%s", body)
	}

	// ...and the one box that shows every prompt still comes out of it, rather
	// than growing its own scroller alongside it. It takes the size-grouped
	// form, because it is a box on a step page beside other boxes on that page
	// and their heading rows have to line up.
	f := regexp.MustCompile(`(?s)func \(a \*App\) prepEditor\(.*?\n}\n`).FindString(src["prepedit.go"])
	if f == "" {
		t.Fatal("prepEditor is gone")
	}
	if !strings.Contains(f, "a.editorBody(head, tv)") {
		t.Error("prepEditor builds its own box instead of the shared one")
	}
	if strings.Contains(f, "NewScrolledWindow()") {
		t.Error("prepEditor grew a second scroller: whichever of the two is edited next, " +
			"the pair stops matching")
	}
}

// TestThePromptColumnsKeepTheirSliderOffTheBoxes: the Cut page puts its column
// in a viewport of its own, so the column has a scrollbar that belongs to no
// box. Left as an overlay it is drawn on top of the framed box under it -- a
// slider with no border of its own, sitting on somebody else's. Narrate was the
// other one; its prompt is behind the dropdown now and its column went with it.
// The Cut column outlived the prompts that named it: it is where every form on
// the page opens (cut_form.go), and a form is framed boxes the same way.
func TestThePromptColumnsKeepTheirSliderOffTheBoxes(t *testing.T) {
	for _, c := range []struct{ file, viewport string }{
		{"cut_form.go", "pane"},
	} {
		b, err := os.ReadFile(c.file)
		if err != nil {
			t.Fatal(err)
		}
		if !strings.Contains(string(b), c.viewport+".SetOverlayScrolling(false)") {
			t.Errorf("%s's form column (%s) scrolls over its own boxes", c.file, c.viewport)
		}
	}
}

func keysOf(m map[string]bool) []string {
	out := make([]string, 0, len(m))
	for k := range m {
		out = append(out, k)
	}
	sort.Strings(out)
	return out
}

// held is what the store is holding of its own -- the wordings that will be
// written to ~/.config/naivepost/prompts on the next flush, and nothing else.
// Nil when there is nothing, which is the state an untouched machine is in.
func held(a *App) map[string]string {
	out := map[string]string{}
	for _, d := range promptDefs {
		if a.promptOwned(d.key) {
			out[d.key] = a.prompt(d.key)
		}
	}
	if len(out) == 0 {
		return nil
	}
	return out
}

// TestOnlyEditsAreStored is the reason a machine can pick up a better shipped
// prompt: storing them all verbatim would freeze today's wording forever.
func TestOnlyEditsAreStored(t *testing.T) {
	ownConfig(t)
	a := &App{}
	if got := held(a); got != nil {
		t.Errorf("an untouched app wants to store %v, want nothing", got)
	}
	// what the editor does on build: fill the box with the default
	for _, d := range promptDefs {
		a.setPrompt(d.key, d.def)
	}
	if got := held(a); got != nil {
		t.Errorf("boxes holding the defaults want to store %v, want nothing", got)
	}

	a.setPrompt("narrate", "Say nothing at all.")
	got := held(a)
	if len(got) != 1 || got["narrate"] != "Say nothing at all." {
		t.Fatalf("the store holds %v, want only the edited one", got)
	}
	// and the project file is out of it entirely: the prompts are the
	// machine's, and a project that carried a copy would put it back on every
	// other machine the folder is opened on
	if body := funcBody(t, "project.go", `func \(a \*App\) currentProject\(\)`); strings.Contains(body, "Prompt") {
		t.Error("a saved project still carries the prompts, which are the machine's now")
	}
	if b, _ := json.Marshal(Project{}); strings.Contains(string(b), "prompt") {
		t.Errorf("an untouched project mentions prompts: %s", b)
	}
	// typing the built-in wording back is how an edit is undone -- storing a
	// copy of it instead would freeze it exactly as storing them all would
	a.setPrompt("narrate", promptDefFor("narrate").def)
	if got := held(a); got != nil {
		t.Errorf("an edit typed back to the built-in still wants to store %v", got)
	}
}

// Opening a project must not rewrite the prompts. They are the machine's now,
// and the wording somebody tuned over four videos has to survive a five-minute
// look at an old session -- which is a project file still carrying the copy it
// saved back when the prompts were the project's.
//
// It is adopted where there is nothing to lose, and only there: that is the
// migration, and it is what makes the first launch after the move keep what
// the last project had.
func TestAProjectDoesNotOverwriteThisMachinesWording(t *testing.T) {
	ownConfig(t)
	a := &App{}
	a.setPrompt("cut", "what this machine was tuned to")

	a.adoptProjectPrompts(map[string]string{
		"cut":     "an old project's idea of a cut",
		"narrate": "and its idea of a voice"})

	if got := a.prompt("cut"); got != "what this machine was tuned to" {
		t.Errorf("opening a project replaced the machine's wording with %q", short(got))
	}
	// nothing of ours to lose on narrate, so the project's is taken in --
	// which is how a pre-merge project's prompts reach the new store at all
	if got := a.prompt("narrate"); got != "and its idea of a voice" {
		t.Errorf("narrate = %q, want the old project's, adopted", short(got))
	}
	// and having adopted it, it is ours: the next project does not get it back
	a.adoptProjectPrompts(map[string]string{"narrate": "a second project's voice"})
	if got := a.prompt("narrate"); got != "and its idea of a voice" {
		t.Errorf("a second project overwrote the adopted wording with %q", short(got))
	}
	// a project that says nothing changes nothing, which is every project
	// saved from this build on
	a.adoptProjectPrompts(nil)
	if got := a.prompt("cut"); got != "what this machine was tuned to" {
		t.Errorf("a prompt-less project cleared the machine's wording: %q", short(got))
	}
}

// A project written before the prompts were the machine's stored one string
// per key. That is exactly the shape they have again, and dropping it on load
// would silently hand the model the built-in instead of what the user wrote.
func TestALegacyProjectPromptIsAdopted(t *testing.T) {
	ownConfig(t)
	a := &App{}
	a.adoptProjectPrompts(map[string]string{"cut": "the old project's idea of a cut"})

	if got := a.prompt("cut"); got != "the old project's idea of a cut" {
		t.Errorf("cut = %q, want what the old project stored", short(got))
	}
	// and it is this machine's from then on, which is what makes it survive
	// the next project and reach the disk (flushPrompts)
	if got := held(a); len(got) != 1 || got["cut"] == "" {
		t.Errorf("the adopted prompt is not held as this machine's: %v", got)
	}
}

// One wording per job, and the cut's assumes nothing about the footage.
//
// It shipped five for a while -- a generic one and four shapes, picked by a
// Style dropdown on Prepare. A shape cut better when the footage was that
// shape and worse when it was not, and which shape a session is, is a fact
// about the session: it belongs in the user context with the rest of them,
// where every step reads it and it outranks the wording (ctxRule).
func TestOneWordingPerJobAndTheCutGuessesNothing(t *testing.T) {
	for _, d := range promptDefs {
		if strings.TrimSpace(d.def) == "" {
			t.Errorf("the %s job ships no wording", d.key)
		}
	}
	// the shapes are gone from the binary, not merely unlisted: a wording
	// nothing can pick is a wording nobody can fix
	src := readSrc(t, "cut.go") + readSrc(t, "narrate.go")
	for _, gone := range []string{
		"suggestSystem", "ratingSystem", "showcaseSystem", "shortsSystem",
		"narrShowcaseSystem", "shortsStyleName",
	} {
		if strings.Contains(src, gone) {
			t.Errorf("%s is still in the binary -- the styles are gone", gone)
		}
	}
	def := strings.ToLower(strings.TrimSpace(promptDefFor("cut").def))
	for _, guess := range []string{"gaming", "game session", "tier list", "youtube short", "showcase"} {
		if strings.Contains(def, guess) {
			t.Errorf("the cut wording says %q -- it is a shape, and a shape is "+
				"something the user context says, not something the prompt assumes", guess)
		}
	}
	// it asks what the session is instead, and the user context is where the
	// answer comes from when there is one
	for _, want := range []string{"work out what this session is", "user context", "the first minutes of the timeline"} {
		if !strings.Contains(def, want) {
			t.Errorf("the cut wording never says %q -- with no genre to fall back "+
				"on, reading the session first is the whole method", want)
		}
	}
	// ...and it says so as a thing to work out rather than a thing to answer
	// with: the reply is JSON and nothing else, and a step that reads like an
	// instruction to write a sentence is a sentence in front of the JSON
	if !strings.Contains(def, "this is for you, not for the answer") {
		t.Error("the cut wording asks for a line of prose in a JSON-only reply")
	}
	// the contract the parser reads, half of it here and half said once in the
	// system context for every job (syscontext.go)
	for _, want := range []string{
		`{"segments":[{"start":<sec>,"end":<sec>}]}`, // the schema, named once (syscontext.go)
		"answer with segments",
		"target length",
	} {
		if !strings.Contains(strings.TrimSpace(sysSystem)+"\n\n"+def, want) {
			t.Errorf("the cut wording never says %q -- the reply is read by the same "+
				"code whatever asked for it", want)
		}
	}
	for _, want := range []string{"session seconds", "EVENT lines"} {
		if !strings.Contains(sysSystem, want) {
			t.Errorf("the system context never says %q -- it was taken out of the "+
				"wordings on the promise that every job is told it once", want)
		}
	}
}

// The bench is one box per job now: no wording to name on a row, no ＋ to save
// a second one under a name, and Reset is the only thing beside the menu.
func TestTheBenchHasNoWordingsToChooseBetween(t *testing.T) {
	src := readSrc(t, "prepedit.go")
	for _, gone := range []string{"askName", "applyStyle", "promptPickName", "add.SetVisible"} {
		if strings.Contains(src, gone) {
			t.Errorf("the bench still offers wordings: %q", gone)
		}
	}
	if !strings.Contains(src, "a.resetPrompt(r.key)") {
		t.Error("Reset no longer puts the built-in back")
	}
	// and the Style dropdown is off Prepare's bottom row
	if p := readSrc(t, "prep.go"); strings.Contains(p, "styleBar(") {
		t.Error("the Style dropdown is back on Prepare")
	}
	// the ✎ stays: what the model reads is not what shipped is worth a mark
	if !strings.Contains(src, "a.promptOwned(r.key)") {
		t.Error("the bench no longer marks a prompt this machine edited")
	}
}

// TestMigrateHintsFoldsNotesIntoThePrompt: describe, fix and narrate used to
// have a notes box that the runner glued onto the system prompt at request
// time. The boxes are gone, so a project written before the merge has to have
// its notes moved into the prompt -- dropping them would change what the model
// is told without changing anything visible.
func TestMigrateHintsFoldsNotesIntoThePrompt(t *testing.T) {
	ownConfig(t)
	a := &App{}
	a.adoptProjectPrompts(nil)
	p := Project{
		DescribeHints:   "the HUD number top left is ammo",
		TranscriptHints: "SPEAKER_00 is Jan",
		CutHints:        "this was the tournament final, keep the last round whole",
		NarrateHints:    "open with an intro over the first clip",
	}
	a.migrateHints(p)

	for _, c := range []struct{ key, note string }{
		{"describe", "the HUD number top left is ammo"},
		{"fix", "SPEAKER_00 is Jan"},
		{"cut", "this was the tournament final, keep the last round whole"},
		{"narrate", "open with an intro over the first clip"},
	} {
		got := a.prompt(c.key)
		if !strings.Contains(got, c.note) {
			t.Errorf("%s prompt lost the project's notes: %q", c.key, short(got))
		}
		if !strings.HasPrefix(got, promptDefFor(c.key).def) {
			t.Errorf("%s prompt no longer starts with the built-in: %q", c.key, short(got))
		}
	}
	// the notes are prompt text now, so the project stores them as an edit --
	// and reloading the same file must not append them a second time
	before := a.prompt("describe")
	a.migrateHints(p)
	if got := a.prompt("describe"); got != before {
		t.Errorf("a second load appended the notes again:\n%q", short(got))
	}
	got := held(a)
	for _, key := range []string{"describe", "fix", "cut", "narrate"} {
		if got[key] == "" {
			t.Errorf("%s's folded-in notes are not held as this machine's edit", key)
		}
	}
	// nothing to fold is the common case and must leave the built-ins alone
	b := &App{}
	b.adoptProjectPrompts(nil)
	b.migrateHints(Project{})
	if got := held(b); got != nil {
		t.Errorf("a project without notes wants to store %v, want nothing", got)
	}
}

func short(s string) string {
	if len(s) > 60 {
		return s[:60] + "…"
	}
	return s
}

// The bench's own menu is replaced in one emission rather than an item at a
// time, and only when the rows actually changed.
//
// A Remove loop empties the model an item at a time, and an empty model is a
// selection GTK moves for you at every step -- which is read as the user
// switching rows. The wordings dropdown that taught us this is gone; the rule
// is the bench's now, where syncPromptMarks redraws the rows on every project
// load and every edit.
func TestTheBenchMenuIsSplicedAndOnlyWhenItMoved(t *testing.T) {
	ed := readSrc(t, "prepedit.go")
	if !strings.Contains(ed, "menu.Splice(") {
		t.Error("the row menu is no longer replaced in a single items-changed")
	}
	if !strings.Contains(ed, "if sameStrings(menu, fresh) {") {
		t.Error("the menu is rebuilt even when the rows did not move")
	}
	if !strings.Contains(ed, "a.promptQuiet = true") {
		t.Error("the splice is not quiet, so the selection it resets reads as a row switch")
	}
}

// TestTheEditorRegistriesInitIndependently: promptViews and promptRows used to
// be created together, guarded by promptViews alone, and a box that filled one
// without the other then panicked on the first write into a nil map. There is
// one box now, so both are filled from the same place -- but they are still
// separate maps written on separate lines, and the guard has to match: a map
// created because the OTHER one was nil is the same bug with the names
// swapped.
func TestTheEditorRegistriesInitIndependently(t *testing.T) {
	ed := funcBody(t, "prepedit.go", `func \(a \*App\) prepEditor\(`)
	for _, want := range []string{
		"if a.promptViews == nil {",
		"if a.promptRows == nil {",
	} {
		if !strings.Contains(ed, want) {
			t.Errorf("prepEditor lacks %q -- one registry existing must not stand in for the other", want)
		}
	}
	if strings.Contains(ed, "a.promptViews = map[string]*gtk.TextView{}\n\t\ta.promptRows") {
		t.Error("the registries are created together again, guarded by one of them")
	}
}

// What the styles used to say, said once, in the one place that is about this
// session: the user context.
//
// The wordings are written to be true of any session, so nothing in them may
// name a shape of video -- and the context has to be where the shape is asked
// for, or the information the styles carried has nowhere to go.
func TestTheShapeOfTheVideoIsTheContextsToSay(t *testing.T) {
	shapes := []string{"gaming", "tier list", "youtube short", "showcase", "highlight reel"}
	for _, d := range promptDefs {
		low := strings.ToLower(d.def)
		for _, shape := range shapes {
			if strings.Contains(low, shape) {
				t.Errorf("the %s wording says %q -- a shape is what the user context "+
					"says, not what a prompt assumes", d.key, shape)
			}
		}
	}
	// the upload text was the last one that assumed: it wrote "for a finished
	// gaming video" and asked for hashtags naming the game
	if strings.Contains(strings.ToLower(youtubeSystem), "game") {
		t.Error("the upload text still assumes the video is of a game")
	}
	// and the box that has to carry it says so, on the row that is that box
	ctx := prepRows()[0]
	if ctx.key != "" {
		t.Fatal("the first bench row is no longer the user context")
	}
	for _, want := range []string{"What kind of video this is", "outranks the prompts"} {
		if !strings.Contains(ctx.tip, want) {
			t.Errorf("the context row never says %q, so nothing tells you where the "+
				"shape of the video goes now", want)
		}
	}
	// the rule that makes it outrank them is sent with every request that has
	// a context at all (sysPrompt), so this is not only a promise in a tooltip
	if !strings.Contains(ctxRule, "user context") {
		t.Error("the precedence rule no longer names the user context")
	}
}

// One wording per job, one file per job, and the folder-per-wording layout
// read once so an edit made under the old build is not lost.
func TestAnEditKeptUnderTheOldLayoutIsAdopted(t *testing.T) {
	ownConfig(t)
	dir := promptsDir()
	if err := os.MkdirAll(filepath.Join(dir, "cut"), 0o700); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(dir, "cut", "General.txt"),
		[]byte("what four videos tuned\n"), 0o600); err != nil {
		t.Fatal(err)
	}
	a := &App{}
	a.loadGlobalPrompts()
	if got := a.prompt("cut"); got != "what four videos tuned" {
		t.Errorf("the old layout's edit reads as %q", short(got))
	}
	// and the next flush writes it where prompts live now, without touching
	// the folder it came from -- anything else in there was a wording for a
	// style, and the styles are gone
	a.flushPrompts()
	b, err := os.ReadFile(promptPath("cut"))
	if err != nil || strings.TrimSpace(string(b)) != "what four videos tuned" {
		t.Errorf("the adopted edit was not written to %s: %v", promptPath("cut"), err)
	}
	if _, err := os.Stat(filepath.Join(dir, "cut", "General.txt")); err != nil {
		t.Errorf("the old file was removed: %v", err)
	}
	// Reset drops the file, which is what makes a newer build's wording reach
	// a machine that has stopped editing
	a.resetPrompt("cut")
	a.flushPrompts()
	if _, err := os.Stat(promptPath("cut")); !os.IsNotExist(err) {
		t.Errorf("Reset left the file behind: %v", err)
	}
	if got := a.prompt("cut"); got != promptDefFor("cut").def {
		t.Errorf("after Reset the cut prompt is %q", short(got))
	}
}

// One font in every box of text.
//
// Every editable box in the app is monospace -- the prompts, the user context,
// the narration lines, the words on a card, the log. Three were not: the
// Publish page's title, its thumbnail instruction and its description, built by
// their own textBox rather than by editorBody, in the proportional font. So the
// one page that shows a prompt's ANSWER beside the prompt that asked for it
// showed the two in different typefaces, and the description looked like a
// different kind of thing from every other box you type in.
func TestEveryTextBoxIsInTheSameFont(t *testing.T) {
	files, err := filepath.Glob("*.go")
	if err != nil {
		t.Fatal(err)
	}
	for _, f := range files {
		if strings.HasSuffix(f, "_test.go") {
			continue
		}
		src := readSrc(t, f)
		for i, line := range strings.Split(src, "\n") {
			if !strings.Contains(line, "gtk.NewTextView()") {
				continue
			}
			// the name it was given, and the next few lines it is set up in
			name, _, _ := strings.Cut(strings.TrimSpace(line), " ")
			rest := strings.Split(src, "\n")[i : i+6]
			if strings.Contains(strings.Join(rest, "\n"), name+".SetMonospace(true)") {
				continue
			}
			// a read-only view is a box of text too, and the log is one
			if strings.Contains(strings.Join(rest, "\n"), name+".SetEditable(false)") {
				continue
			}
			t.Errorf("%s:%d builds a text box in another font than every other one", f, i+1)
		}
	}
	// the one-line fields, which cannot be told one at a time: a GtkEntry has
	// no monospace switch, so it is said once in the app's own CSS
	if !strings.Contains(readSrc(t, "main.go"), "entry, entry text { font-family: monospace; }") {
		t.Error("the entries are back in the proportional font, one line above the boxes")
	}
}

// The boxes you type in have the corner the controls beside them have.
//
// Every one of them is a scrolled window with the frame class -- the shared
// editorFrame, and the four that build their own -- and the theme draws that
// frame square. A page of rounded GTK controls with one square thing on it,
// and the square thing is what you spend the most time looking at.
//
// The timeline is deliberately not in this: its bands are cairo and they stay
// square, because a band is a measurement you aim at with a few px of
// tolerance (see platePath in cut.go).
func TestTheTextBoxesAreRoundedLikeEverythingElse(t *testing.T) {
	if css := readSrc(t, "main.go"); !strings.Contains(css, `".frame { border-radius: 6px; }`) {
		t.Error("the framed boxes are square again")
	}
	// and every text box is framed the same way, so one rule reaches all of
	// them rather than four pages each deciding
	n := strings.Count(readSrc(t, "prompts.go"), `AddCSSClass("frame")`)
	for _, f := range []string{"main.go", "publish.go", "narrate.go"} {
		n += strings.Count(readSrc(t, f), `AddCSSClass("frame")`)
	}
	for _, f := range []string{"cut_fx.go", "publish_text.go"} {
		n += strings.Count(readSrc(t, f), "SetHasFrame(true)")
	}
	if n != 6 {
		t.Errorf("%d text boxes are framed, want the six the app has -- one that "+
			"frames itself another way is one this rule does not reach", n)
	}
}
