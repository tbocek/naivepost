package main

import (
	"os"
	"path/filepath"
	"strings"

	"github.com/diamondburned/gotk4/pkg/gtk/v4"
)

// The system prompts: one registry, editable in the app. An edited prompt is
// stored in ~/.config/naivepost/prompts (this machine's, not the project's --
// taste does not change between sessions); an untouched one is not, so it
// picks up a new build's wording. The box IS the prompt: nothing is glued on.

// promptStyle is one named wording for a job. Most jobs have one; the cut
// ships several because what makes a good segment is a property of the
// footage, and a project can add its own.
// promptRow is the widgets above one prompt box: the edited/shipped label and
// the reset button.
type promptRow struct {
	mark *gtk.Label
	drop *gtk.Button
}

// promptDef is one editable prompt: the job and the wording this build ships.
// One wording per job, written to be true of any session; what KIND of video
// this is goes in the user context, which outranks the wording (ctxRule).
type promptDef struct{ key, def string }

// Keys name the prompt in project.json and are therefore permanent -- renaming
// one silently drops what a user wrote under the old name.
var promptDefs = []promptDef{
	// first, because it goes in front of every one of the others: the formats
	// and the house rules they all work to (syscontext.go).
	{key: "system", def: strings.TrimSpace(sysSystem)},
	{key: "describe", def: strings.TrimSpace(describeSystem)},
	{key: "fix", def: strings.TrimSpace(fixSystem)},
	// last in Prepare, on the merged timeline: the stretches that were said
	// twice (retake.go). Before the cut and not inside it -- a retake is a
	// fact about what was said, not a judgment about the video.
	{key: "retake", def: strings.TrimSpace(retakeSystem)},
	// ...or, for a read to camera, the edit itself as text (textedit.go)
	{key: "textedit", def: strings.TrimSpace(textSystem)},
	// the cut, then the three passes that follow it clip by clip: captions,
	// speed, decorations. "audit" was here -- a second long call that moved
	// borders by seconds; removed, not renamed, so a project's edited copy is a
	// dead key.
	{key: "cut", def: strings.TrimSpace(cutSystem)},
	{key: "captions", def: strings.TrimSpace(captionSystem)},
	{key: "speed", def: strings.TrimSpace(speedSystem)},
	{key: "effects", def: strings.TrimSpace(effectsSystem)},
	{key: "narrate", def: strings.TrimSpace(narrSystem)},
	// "thumbnail" was here: a second Publish prompt that picked which frame to
	// edit and wrote the instruction for it. Removed, not renamed -- the key is
	// gone from the registry, so a project that saved an edited copy of it just
	// keeps a dead key nobody reads.
	// the subtitle track in another language (translate.go): the same cues,
	// the same times, one line out for every line in
	{key: "translate", def: strings.TrimSpace(translateSystem)},
	{key: "youtube", def: strings.TrimSpace(youtubeSystem)},
	// "improve" was here, and before that it had already come off the bench:
	// the Improve button asked the model why a step decided what it did and
	// offered edits to the prompts below. Button, prompt and cards are all
	// gone, and a project that saved an edited copy of the key just keeps a
	// dead key nobody reads.
}

func promptDefFor(key string) promptDef {
	for _, d := range promptDefs {
		if d.key == key {
			return d
		}
	}
	return promptDef{key: key}
}

// prompt returns the system prompt for key: what this machine has of its own,
// or the wording the build ships. Callable from a step runner's goroutine --
// it reads a cached string, never the GtkTextBuffer, which belongs to the GUI
// thread.
func (a *App) prompt(key string) string {
	a.promptMu.Lock()
	s := strings.TrimSpace(a.promptTxt[key])
	a.promptMu.Unlock()
	if s != "" {
		return s
	}
	return promptDefFor(key).def
}

// setPrompt records what is in the box. Kept per machine and written to disk
// on the autosave tick (flushPrompts): how you like to be edited for is the
// same in January's raid as in March's.
func (a *App) setPrompt(key, text string) {
	a.promptMu.Lock()
	if a.promptTxt == nil {
		a.promptTxt = map[string]string{}
	}
	a.promptTxt[key] = text
	a.promptMu.Unlock()
	a.markPromptRow(key)
}

// promptOwned is whether what the model reads is not what shipped. It is the
// one thing worth a permanent mark wherever a prompt is named (the ✎ in the
// bench's menu), and it is exact rather than a flag: typing an edit and typing
// it back is not an edit.
func (a *App) promptOwned(key string) bool {
	a.promptMu.Lock()
	s := strings.TrimSpace(a.promptTxt[key])
	a.promptMu.Unlock()
	return s != "" && s != promptDefFor(key).def
}

// resetPrompt puts the built-in back: the box, the mark, and -- on the next
// flush -- the file on disk, which is what makes Reset a delete.
func (a *App) resetPrompt(key string) {
	a.promptMu.Lock()
	delete(a.promptTxt, key)
	a.promptMu.Unlock()
	a.showPrompt(key)
}

// showPrompt fills the box for a job and settles the row above it. Safe before
// the widgets exist, so a project load does not have to care whether a page
// has been built yet.
//
// promptQuiet is what keeps it from being a loop: filling the box fires
// "changed", whose handler would otherwise write the wording straight back
// out. GUI thread only, which is why a plain bool is enough.
func (a *App) showPrompt(key string) {
	if tv, ok := a.promptViews[key]; ok {
		a.promptQuiet = true
		tv.Buffer().SetText(a.prompt(key))
		a.promptQuiet = false
	}
	a.markPromptRow(key)
}

// adoptProjectPrompts takes in what a project written before the prompts were
// the machine's has to say about them. It ADOPTS rather than replaces: a
// wording lands only where this machine has nothing of its own for the job, so
// an old project opened for five minutes cannot overwrite tuned wordings.
func (a *App) adoptProjectPrompts(legacy map[string]string) {
	for _, d := range promptDefs {
		if a.promptOwned(d.key) {
			continue // this machine's own wording; a project does not overwrite it
		}
		if s := strings.TrimSpace(legacy[d.key]); s != "" {
			a.setPrompt(d.key, s)
		}
	}
	for _, d := range promptDefs {
		a.showPrompt(d.key)
	}
}

// markPromptRow says whether this machine is holding an edit, and whether the
// button that lets go of it has anything to do.
func (a *App) markPromptRow(key string) {
	// the ✎ in the bench's menu first, and unconditionally: it says the same
	// thing this row does -- "what the model reads is not what shipped" -- and
	// it is the only place that says it once the boxes are behind a button
	// (prepedit.go), so it cannot depend on a box being open.
	a.syncPromptMarks()

	row, ok := a.promptRows[key]
	if !ok {
		return
	}
	if a.promptOwned(key) {
		row.mark.SetText("edited — kept in your settings")
		row.drop.SetSensitive(true)
		row.drop.SetTooltipText("Put the built-in wording back")
		return
	}
	row.mark.SetText("")
	// nothing to undo: a live button here reads as "there is something
	// stored", which is exactly what the empty mark is denying
	row.drop.SetSensitive(false)
	row.drop.SetTooltipText("This is the built-in wording, unchanged")
}

// syncPromptMarks redraws the bench's menu, where the ✎ on a row is the only
// thing that says a prompt has been reworded once the boxes are behind a
// button (prepedit.go).
func (a *App) syncPromptMarks() {
	if a.prepSync != nil {
		a.prepSync()
	}
}

// sameStrings is whether a menu already holds exactly these rows. Splicing a
// model that did not change is a menu that flickers and a selection that
// bounces through every index on the way.
func sameStrings(model *gtk.StringList, want []string) bool {
	if int(model.NItems()) != len(want) {
		return false
	}
	for i, s := range want {
		if model.String(uint(i)) != s {
			return false
		}
	}
	return true
}

// askName is a modal one-line prompt. Same hand-rolled shape as confirm, and
// the same reasons; Enter is OK here rather than Cancel, because the only thing
// this asks for is a name and typing one is the whole interaction.
func (a *App) askName(question, detail string, ok func(string)) {
	var win *gtk.Window
	entry := gtk.NewEntry()
	done := func() {
		if name := strings.TrimSpace(entry.Text()); name != "" {
			win.Close()
			ok(name)
		}
	}
	entry.ConnectActivate(done)
	save := gtk.NewButtonWithLabel("Save")
	save.AddCSSClass("suggested-action")
	save.ConnectClicked(done)
	cancel := gtk.NewButtonWithLabel("Cancel")
	win = a.modal(question, detail, 380, entry, cancel, save)
	cancel.ConnectClicked(func() { win.Close() })
	entry.GrabFocus()
	win.SetVisible(true)
}

// editorBody is the one shape a text box on a step page has: a heading row,
// then a framed, scrolling box floored at four lines. Every heading row on
// every page joins one size group, so a row with a button and a row with a
// bare label are the same height and the boxes under them start at the same
// y. The four-line floor is the one number that can push things off a short
// window; 240 once did.
func (a *App) editorBody(head *gtk.Box, tv *gtk.TextView) *gtk.Box {
	if a.headGroup == nil {
		a.headGroup = gtk.NewSizeGroup(gtk.SizeGroupVertical)
	}
	a.headGroup.AddWidget(head)
	return editorFrame(head, tv)
}

// editorFrame is editorBody without the shared heading height: the same box, for
// somewhere there is nothing beside it to line up with. That is where every
// prompt is shown now -- one at a time, in the picker's window or in the Cut
// page's form column (promptpick.go) -- and a size group is worse than useless
// there, since it goes on measuring rows belonging to windows that have closed.
func editorFrame(head *gtk.Box, tv *gtk.TextView) *gtk.Box {
	// vexpand is what makes a taller window a taller box. Without it the box
	// stops at the height of its text and the window's extra height piles up as
	// blank page below it -- growing the window then bought nothing on the one
	// page whose whole content is text. It propagates up through what this
	// returns, so a page only has to put the box somewhere that can grow.
	scroll := gtk.NewScrolledWindow()
	scroll.SetChild(tv)
	scroll.SetPropagateNaturalHeight(true)
	scroll.SetPolicy(gtk.PolicyNever, gtk.PolicyAutomatic) // wrapped text has no width to scroll
	scroll.SetSizeRequest(-1, 72)
	scroll.SetVExpand(true)
	scroll.AddCSSClass("frame")

	body := gtk.NewBox(gtk.OrientationVertical, 4)
	body.SetMarginTop(4)
	body.Append(head)
	body.Append(scroll)
	return body
}

// Edited prompts live in ~/.config/naivepost/prompts/<job>.txt, beside the
// settings that are also this machine's. Files, not project JSON: a prompt is
// how you like to be edited for, the same across sessions, and prose that
// gets diffed and grepped. Only what differs is written, so Reset is a delete
// and a newer build's wording reaches an untouched machine.

// promptsDir is the folder, or "" when there is nowhere to put it. Same answer
// as configDir, and for the same reason: nowhere to write is not an error, it
// is a launch where the prompts are whatever the binary ships.
func promptsDir() string {
	d := configDir()
	if d == "" {
		return ""
	}
	return filepath.Join(d, "prompts")
}

const promptExt = ".txt"

// promptPath is where a job's wording is kept, or "" with nowhere to put it:
// prompts/cut.txt, one file per job (it was a folder per job with a file per
// wording, back when styles existed).
func promptPath(key string) string {
	d := promptsDir()
	if d == "" || key == "" {
		return ""
	}
	return filepath.Join(d, key+promptExt)
}

// oldPromptNames are the file names the folder-per-job era wrote a job's
// DEFAULT wording under. Anything else in that folder was a wording for a
// style, and the styles no longer exist -- the file is left where it is, and
// what is worth keeping out of it goes in the box or the user context by hand.
var oldPromptNames = []string{"General" + promptExt, "Default" + promptExt}

// loadGlobalPrompts is the startup read: whatever this machine has of its own
// for each job. Called once, before the first project is opened, so that what
// the boxes show is this machine's answer rather than the last project's.
func (a *App) loadGlobalPrompts() {
	txt, disk := map[string]string{}, map[string]string{}
	for _, d := range promptDefs {
		text, from := a.readPrompt(d.key)
		if text == "" {
			continue
		}
		txt[d.key] = text
		if from == promptPath(d.key) {
			// adopted from the old folder instead: leaving promptDisk empty
			// for it is what makes the next flush write it where it belongs
			disk[d.key] = text
		}
	}
	a.promptMu.Lock()
	a.promptTxt = txt
	a.promptMu.Unlock()
	a.promptDisk = disk
	for _, d := range promptDefs {
		a.showPrompt(d.key)
	}
}

// readPrompt is one job's stored wording and the file it came from: its own
// file, or -- for a machine that last ran a build with styles -- the default
// wording out of the old folder.
//
// An empty file is a wording that says nothing, which would send the model no
// system prompt at all: treated as absent, here as before.
func (a *App) readPrompt(key string) (text, from string) {
	try := []string{promptPath(key)}
	if d := promptsDir(); d != "" {
		for _, n := range oldPromptNames {
			try = append(try, filepath.Join(d, key, n))
		}
	}
	for _, p := range try {
		if p == "" {
			continue
		}
		b, err := os.ReadFile(p)
		if err != nil {
			if !os.IsNotExist(err) {
				a.logf("!!! could not read the %s prompt: %v", key, err)
			}
			continue
		}
		if s := strings.TrimSpace(string(b)); s != "" {
			return s, p
		}
	}
	return "", ""
}

// flushPrompts writes what changed since the last flush, on the autosave tick
// (startAutosave). promptDisk is what is believed on disk; a failed write is
// logged and treated as done (the next edit retries). A job back on its
// shipped wording has its file REMOVED. GUI thread only, like flushProject.
func (a *App) flushPrompts() {
	if promptsDir() == "" {
		return
	}
	cur := map[string]string{}
	for _, d := range promptDefs {
		if a.promptOwned(d.key) {
			cur[d.key] = a.prompt(d.key)
		}
	}
	for key, text := range cur {
		if a.promptDisk[key] == text {
			continue
		}
		if err := os.MkdirAll(promptsDir(), 0o700); err != nil {
			a.logf("!!! could not keep the %s prompt: %v", key, err)
			continue
		}
		if err := os.WriteFile(promptPath(key), []byte(text+"\n"), 0o600); err != nil {
			a.logf("!!! could not keep the %s prompt: %v", key, err)
		}
	}
	for key := range a.promptDisk {
		if _, ok := cur[key]; ok {
			continue
		}
		if err := os.Remove(promptPath(key)); err != nil && !os.IsNotExist(err) {
			a.logf("!!! could not drop the %s prompt: %v", key, err)
		}
	}
	a.promptDisk = cur
}

// The user context: what the editor knows that the material does not say --
// who is in the session, what they were doing, what to call things. One box,
// the first row of the bench on Prepare, carried by every request. Prompts say
// HOW to work; this says WHAT this session was, which is why it is stored in
// full and a prompt only when it differs from the built-in.

// sessionCtx is the box's text, callable from a runner's goroutine.
func (a *App) sessionCtx() string {
	a.promptMu.Lock()
	defer a.promptMu.Unlock()
	return strings.TrimSpace(a.ctxTxt)
}

func (a *App) setSessionCtx(s string) {
	a.promptMu.Lock()
	a.ctxTxt = s
	a.promptMu.Unlock()
}

// applySessionCtx loads a project's context into the box and the cache. GUI
// thread only (a GtkTextBuffer). No box is the ordinary case: the bench shows
// one row at a time, and the box fills from the cache when switched back to.
func (a *App) applySessionCtx(s string) {
	a.setSessionCtx(s)
	if a.ctxView != nil {
		a.ctxView.Buffer().SetText(s)
	}
}

// ctxBlock is the context as a request carries it, or nothing when the box is
// empty (an empty heading invites invention). Headed USER CONTEXT, which is
// what downstream prompts call it. It goes in the USER message ahead of the
// material, never the system prompt: the prompt boxes stay what the user
// wrote, and a job's rules stay separable from a session's facts.
func (a *App) ctxBlock() string { return a.ctxBlockFor("cut") }

// ctxBlockFor is the block as one job carries it. The speech rule under it is
// about what to DO with spoken lines -- keep them, caption them, cut on them
// -- and only the jobs that decide that get it: the cut and the narration. The frame describer, the transcript fixer and the upload text
// are told the context and nothing about a decision they never make.
func (a *App) ctxBlockFor(key string) string {
	s := a.sessionCtx()
	if s == "" {
		return ""
	}
	b := "USER CONTEXT -- written by the person who made this recording and " +
		"is editing it. It outranks anything you infer from the material, and it " +
		"outranks the rules of the job you were given wherever the two disagree; " +
		"only the mechanics of the answer -- its shape, its clock, what may be " +
		"invented -- are not its to change:\n" + s + "\n\n"
	switch key {
	case "cut", "narrate":
		b += ctxSpeech + "\n\n"
	}
	return b
}

// ctxSpeech rides under the user context, and only under it: how the context
// bears on the spoken lines. Sent exactly when there is a context to be about.
// Read the wrong way it is the worst answer this app gives -- an aside to the
// editor captioned into the video, or the video thrown away as asides.
const ctxSpeech = `The speech is content unless the user context above says otherwise: the speakers are in the video, and what they say is why a moment is worth keeping. Where the user context calls it directions ("this part is boring", "speed this up"), do what a direction asks at the second it asks and keep its words out of the video -- never caption them, and never keep a stretch just because it was spoken over. An instruction about a kind of stretch -- speed the dull parts up and show them instead of cutting them, caption each thing as it is named -- holds wherever such a stretch occurs. It decides segments too: a stretch to be shown fast has to be in the cut, with a speed effect over it, or there is nothing left to speed up.`

// logCtx says, in the log, that this step's requests carried the context. A
// second input that changes the result and appears nowhere in the run is how
// an editor ends up baffled by their own note from three sessions ago; naming
// the step matters, because the log is read long after the run.
func (a *App) logCtx(step string) {
	if c := a.sessionCtx(); c != "" {
		a.logfIdle(">>> %s: sending the session context from Prepare (%d characters)", step, len(c))
	}
}
