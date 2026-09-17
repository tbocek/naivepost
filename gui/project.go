package main

// Project state: which sources are in, in what order, and the step settings.
// Saved as plain JSON with root-relative paths, so a project file survives
// moving the naivepost directory. There is always an open file -- root/session.naivepost
// until Save names another one -- and it is the only file a save writes; Save/Load
// dialogs are for keeping named variants, and whichever file is open when the
// window closes is the one the next launch reopens (settings.go).

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io/fs"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"time"

	"github.com/diamondburned/gotk4/pkg/gio/v2"
	"github.com/diamondburned/gotk4/pkg/glib/v2"
	"github.com/diamondburned/gotk4/pkg/gtk/v4"
)

type Project struct {
	// the session's files in order, each with what it is for; paths relative to
	// root where they can be
	Sources []ProjectSource `json:"sources,omitempty"`
	// videos/audios are read, never written: membership used to be a folder and
	// a checkbox, one list per folder. A project written back then loads as
	// sources -- its videos as footage, its first recording as narrator 1 --
	// and the keys go away with the next save. See projectSources.
	Videos     []string `json:"videos,omitempty"`
	Audios     []string `json:"audios,omitempty"`
	Interval   float64  `json:"interval"` // seconds between frames; 0 = every frame
	FrameScale string   `json:"frame_scale,omitempty"`
	// which pipeline the session runs through (textedit.go): "read" for a
	// read to camera, edited as text with the picture never described; blank
	// for a session, described and its moments picked -- so every project
	// written before this reads as the latter.
	Style string `json:"style,omitempty"`
	// which steps the ▶▶ beside the play button runs, in pipeline order
	// (runchain.go). Absent -- every project written before this -- means all
	// of them, which is what "run it all" is for.
	RunSteps []string `json:"run_steps,omitempty"`
	// what the ASR model is told this session is spoken in. It was a setting --
	// one language for the machine, however many languages its sessions were in
	// -- and it is a property of the footage, so it belongs to the project that
	// names that footage. Absent means defLanguage, the same deal a prompt gets.
	Language string `json:"language,omitempty"`
	// this video has no narration at all: no lines written, none spoken, and
	// nothing in Produce that exists to carry one. Off is the ordinary answer
	// and every project written before this reads as off, so an old project
	// still narrates. See App.narrOff.
	NoNarration bool `json:"no_narration,omitempty"`
	// the sources are referenced where they are rather than copied into the
	// project when added. Off -- copy -- is the ordinary answer, so a project
	// is one folder that holds what it needs; on is for the machine that
	// recorded the footage, where 20 GB of capture does not want a twin.
	// Stored the wrong way round so every project written before this copies.
	RefSources bool   `json:"reference_sources,omitempty"`
	VidDir     string `json:"vid_dir,omitempty"` // where the choosers open, each
	AudDir     string `json:"aud_dir,omitempty"` // stored the way every path is (storePath)
	// in_dir is read, never written: there was one input folder, which had to
	// hold input_video/ and input_audio/. A project written back then names it
	// here, and those two subfolders are where the two folders above start.
	InDir string `json:"in_dir,omitempty"`
	// out_dir is read, never written: the output folder used to be chosen and
	// stored, and is now derived from the project file's own name (see projExt).
	// It is read only to say, once, where an older project's work was left.
	OutDir string `json:"out_dir,omitempty"`
	// these four are read, never written: the pages they belonged to had a notes
	// box beside the system prompt, which the runner glued onto the prompt just
	// before sending. One box per step now, so a project written by an older
	// build has its notes folded into the prompt on load (migrateHints) and the
	// keys go away with the next save.
	DescribeHints   string `json:"describe_hints,omitempty"`
	TranscriptHints string `json:"transcript_hints,omitempty"`
	CutHints        string `json:"cut_hints,omitempty"`
	NarrateHints    string `json:"narrate_hints,omitempty"`
	// "pitch" used to live here too -- the output-pitch slider. It is gone; a
	// project written back then still loads, the key is simply ignored.

	// what the editor typed about this session on the Describe page. Stored in
	// full, unlike a prompt: there is no built-in wording for it to differ from
	// (see context.go).
	Context string `json:"context,omitempty"`

	// Read, never written: the prompts live on the machine now (promptstore in
	// prompts.go). A project still carrying them is adopted once for whichever
	// jobs this machine has nothing of its own for (applyPromptStyles); the
	// file itself is left as it was.
	Prompts map[string]string `json:"prompts,omitempty"`

	Produce *prodSettings `json:"produce,omitempty"`
	// the thumbnail and the upload text. Absent until the thumbnail half of
	// Produce has something on it, so an older project -- or a session that stops at the
	// rendered video -- stays as short a file as it was.
	Publish *pubSettings `json:"publish,omitempty"`
}

// ProjectSource is one source as a project stores it. The roles are omitted
// when unset so the common row -- a recording nobody narrates as -- is one
// line, and so a project file stays something you can read and fix by hand.
type ProjectSource struct {
	Path     string `json:"path"`
	Footage  bool   `json:"footage,omitempty"`
	Narrator int    `json:"narrator,omitempty"`
	// the wish, not the result: it is saved because a project closed before ▶
	// has to open with the same rows still flagged, and it is cleared by the
	// run that grants it, so a finished project stores nothing here
	SepVoice bool `json:"sepvoice,omitempty"`
	// which audio tracks of a multi-track file this session uses (a:N indices).
	// Omitted for the ordinary answer -- the first track alone -- so a project
	// written before multi-track files were reachable already says the right
	// thing by saying nothing, and no migration was needed (wantTracks).
	Tracks []int `json:"tracks,omitempty"`
}

// relToRoot stores a folder the way a project file wants it: relative when it
// lives inside root, so moving the naivepost directory moves it too; absolute
// when it does not, because then there is nothing to be relative to.
func (a *App) relToRoot(dir string) string {
	if rel, err := filepath.Rel(a.root, dir); err == nil && !strings.HasPrefix(rel, "..") {
		return rel
	}
	return dir
}

// projPrefix marks a path stored relative to the PROJECT rather than to the
// root. Sources can live inside the project folder -- the voice-split step
// writes its stems there, and Add can copy footage in (project_import.go) --
// and a project is a folder you are meant to be able to rename, move or zip.
// Stored absolute, every one of those breaks the session silently: the file is
// not where the project says, the row is pruned, and the autosave then writes
// the pruned list back.
const projPrefix = "project:"

// projRel is a path as the project folder sees it, and whether it is in there
// at all. Slash-separated, because a stored path is read on whatever machine
// the folder is opened on.
func (a *App) projRel(p string) (string, bool) {
	if a.outDir == "" {
		return p, false
	}
	rel, err := filepath.Rel(a.outDir, p)
	if err != nil || rel == ".." || strings.HasPrefix(rel, ".."+string(filepath.Separator)) {
		return p, false
	}
	return filepath.ToSlash(rel), true
}

// storePath is how ANY file path goes into a project's files -- naivepost.json,
// cut.json, the publish record: relative to the project when it is inside it,
// relative to the root when it is under that, and absolute otherwise. Every
// writer goes through here, because a path stored the other way is a file the
// project loses the moment the folder is renamed or moved.
func (a *App) storePath(p string) string {
	if rel, ok := a.projRel(p); ok {
		return projPrefix + rel
	}
	return a.relToRoot(p)
}

// loadPath is its inverse, and the one reader: a project written before this
// stores those same files relative to the root, which is what the last two
// cases are. An empty value means the root itself.
func (a *App) loadPath(p string) string {
	switch {
	case p == "":
		return a.root
	case strings.HasPrefix(p, projPrefix):
		return filepath.Join(a.outDir, filepath.FromSlash(strings.TrimPrefix(p, projPrefix)))
	case filepath.IsAbs(p):
		return p
	default:
		return filepath.Join(a.root, p)
	}
}

// A project is a FOLDER: /mnt/rec/tom.naivepost, with naivepost.json in it and
// every step's work beside it. One thing to copy, back up, hand over or zip;
// nothing stores where the work goes, so nothing can disagree about it. The
// unsaved session is root/session.naivepost, and Save renames the folder.
// Projects from earlier builds (a file beside a .data folder) are adopted on
// open (adoptLegacy).
const projExt = ".naivepost"

// projMain is the project's own file inside its folder. Named rather than
// spelled out because the folder IS the project everywhere else in the app,
// and this is the one place that cares which file in it carries the JSON.
//
// naivepost.json rather than main.json: opened on its own -- in an editor, in a
// diff, out of a zip somebody sent -- "main.json" is a file that could belong
// to anything.
const projMain = "naivepost.json"

// projFile is where a project folder keeps its JSON.
func projFile(dir string) string { return filepath.Join(dir, projMain) }

// workName is the working copy inside the naivepost root: what the first launch
// starts as, and what New Project goes back to.
const workName = "session" + projExt

// dataDir is a project's output folder, which is the project (see projExt).
// Kept as a name because every step, every page and the Save that moves the
// folder go through it, and "the project" and "where it writes" are worth
// being able to tell apart in the reading even when they are one path.
func dataDir(proj string) string { return proj }

// withProjExt is what Save actually writes, whatever was typed in the name box.
// The extension is not decoration: it is what the desktop matches to open a
// file with Naivepost (icon.go), and it is what .data hangs off, so "tom" and
// "tom.naivepost" would otherwise be two projects sharing neither name nor work.
func withProjExt(p string) string {
	if strings.EqualFold(filepath.Ext(p), projExt) {
		return p
	}
	return p + projExt
}

func (a *App) currentProject() Project {
	scaleName, _ := a.frameScale()
	var prod *prodSettings
	if a.prod != nil {
		st := a.prodSettings()
		// the destination is not stored: it is produce/final with the
		// container's extension, worked out from where the project is
		// (setOut, followOutDir), so a project that moves takes its video's
		// folder with it and a project file cannot disagree with the page
		st.OutFile = ""
		prod = &st
	}
	var srcs []ProjectSource
	for _, it := range a.srcList.items {
		srcs = append(srcs, ProjectSource{
			Path: a.storePath(it.path), Footage: it.footage, Narrator: it.narrator,
			SepVoice: it.sepVoice, Tracks: it.tracks,
		})
	}
	return Project{
		Sources:    srcs,
		Interval:   a.frameInterval(),
		FrameScale: scaleName,
		Style:      a.videoStyleName(),
		RunSteps:   a.chainPicked(),
		Language:   a.projectLanguage(),
		// the cut's wording IS the style: the dropdown lists that job's
		// wordings and every other job followed it. Written even
		// when it is the shipped default, so that "this project is General"
		// and "this project predates the field" stay different answers -- the
		// first has to be able to switch a machine back off Showcase.
		VidDir:      a.storePath(a.vidDir),
		AudDir:      a.storePath(a.audDir),
		Context:     a.sessionCtx(),
		NoNarration: a.narrOff,
		RefSources:  a.refSources,
		Produce:     prod,
		Publish:     a.currentPublish(),
	}
}

// projectSources is a stored project as a list of sources, migrating one
// written before the two lists became one.
//
// The migration has to preserve what the old project meant, which was: these
// videos are the footage, these recordings are the voices, and the first
// recording is the one Narrate clones. That last part was a convention nothing
// stated -- ownVoiceFile took audios[0] -- so it becomes the narrator 1 tag,
// which is the same choice said out loud.
//
// Legacy paths are resolved through the folder, not through root: a project
// from before the folders were settable stores input_video/x.mkv, which is
// relative to a root that may no longer be this one.
func (a *App) projectSources(p Project) []sourceItem {
	if len(p.Sources) > 0 {
		var out []sourceItem
		for _, s := range p.Sources {
			out = append(out, sourceItem{
				path: a.loadPath(s.Path), footage: s.Footage, narrator: s.Narrator,
				sepVoice: s.SepVoice, tracks: s.Tracks,
			})
		}
		return out
	}
	var out []sourceItem
	for _, l := range []struct {
		dir     string
		files   []string
		footage bool
	}{{a.vidDir, p.Videos, true}, {a.audDir, p.Audios, false}} {
		for i, f := range l.files {
			path := a.loadPath(f)
			if inDir := filepath.Join(l.dir, filepath.Base(f)); !exists(path) && exists(inDir) {
				path = inDir
			}
			it := sourceItem{path: path, footage: l.footage}
			if !l.footage && i == 0 {
				it.narrator = 1 // what "my own voice" used to mean
			}
			out = append(out, it)
		}
	}
	return out
}

// ---- saving, and then saving itself -----------------------------------------

// projectJSON is the project as it would be written right now. It is also how
// the autosave decides there is anything to write: the bytes are the state, so
// comparing them catches every field of every page without a single page
// having to remember to say it changed. Nothing else could -- the settings are
// spread over five pages, two text buffers, a list and a dozen widgets, and a
// notify-me hook on each of them is a hook somebody will forget on the next
// one, silently, with the symptom appearing a session later as lost work.
func (a *App) projectJSON() []byte {
	b, err := json.MarshalIndent(a.currentProject(), "", "  ")
	if err != nil {
		return nil // cannot happen with these types; not worth a wrong write
	}
	return append(b, '\n')
}

// writeProject writes the JSON into the project folder, making it if this is
// the first write -- a project that has never been saved is a folder that does
// not exist yet, and the autosave is what brings it into being.
func (a *App) writeProject(path string, b []byte) error {
	if err := os.MkdirAll(path, 0o755); err != nil {
		return err
	}
	return os.WriteFile(projFile(path), b, 0o644)
}

// projNameChars is how much of the project's name the header bar shows before
// it ellipsizes. Wide enough for the names people actually type ("before the
// recut.json"), narrow enough that it cannot walk the centered tabs sideways
// on a small window.
const projNameChars = 28

// projLabelText is what the header bar can say about a project path: the file's
// name, the whole path, and the tooltip behind whichever of the two is on
// screen. Which one is shown is a question of width and belongs to fitHeader;
// this is only the wording, including the one for a session that has never been
// saved -- a blank space where a file name goes reads as a bug rather than as
// "nothing has been written yet". That case has no path to offer, so it gives
// the same words twice and the bar is free to pick either.
func projLabelText(p string) (name, full, tip string) {
	if strings.TrimSpace(p) == "" {
		none := "no project file"
		return none, none, "this session has not been saved to a project file yet"
	}
	return filepath.Base(p), p, p
}

// showProject puts the open project in the header bar. Called from both places
// that assign projPath, so the bar can never name a file the autosave has
// stopped following. The name first and then the fit, because the fit is what
// decides between the name and the path and it declines to answer at all
// before the bar has been laid out -- at which point the name is the right
// thing to be showing.
func (a *App) showProject() {
	if a.projLabel == nil {
		return // headless (tests)
	}
	name, _, tip := projLabelText(a.projPath)
	a.projLabel.SetText(name)
	a.projLabel.SetTooltipText(tip)
	a.fitHeader()
}

// setProject points the session at a project file: the file the autosave
// follows, and the folder beside it that every step writes into. The two are
// one decision (see projExt), so they are made in one place, and everything on
// screen that was read out of the old folder is re-read here.
func (a *App) setProject(path string) {
	a.projPath = path
	a.outDir = dataDir(path)
	a.migrateFolders()
	a.showProject()
	a.followOutDir()
	// the voice belongs to the project, not to the session: drop the cached id,
	// pitch and hand-picked takes so all three are re-read from the folder we
	// just moved to
	a.voiceMu.Lock()
	a.voiceSel = ""
	a.takesRead, a.takesMap = false, nil
	a.pitchRead = false
	a.voiceMu.Unlock()
	if a.voicePick != nil {
		a.voicePick.syncSelection()
	}
	// The narration lives in the output folder, and this is the only thing that
	// re-reads it: the page is built once at startup, against the empty
	// session's folder, so without this a saved narration would never load for
	// any project at all -- it looked like narration was never saved.
	if a.narr != nil {
		a.narr.load()
		a.narr.rebuildRows()
	}
	// every page shows state derived from the output folder -- refresh all.
	// The Cut page included: its tracks were just drawn from the folder of the
	// project we are leaving.
	a.prep.refresh()
	a.updateProduceInfo()
	a.pub.refresh()
	a.refreshCut()
	a.updateGates()
}

// saveProjectNow writes every target and remembers what it wrote. Quiet: it is
// what the ticker and the step runners call, and a status line per autosave
// would push the message the user is actually waiting on off the bar.
func (a *App) saveProjectNow() {
	b := a.projectJSON()
	if b == nil {
		return
	}
	// before the writes, not after: a target that cannot be written must not be
	// retried every tick for the rest of the session, and the log line below
	// says it once per change either way.
	a.projSaved = b
	// One file: the open one. It used to be two -- the named file and a working
	// copy the next launch opened -- and the working copy is now simply the
	// project that is open when nobody has named one, so there is nothing left
	// to keep in step with anything.
	if err := a.writeProject(a.projPath, b); err != nil {
		a.logf("save project: %v", err)
	}
}

// adoptLegacy turns an older build's project -- the file tom.naivepost beside
// tom.naivepost.data -- into the folder this build opens, via a staging name so
// the file is never in the way of its own new name: rename .data aside, write
// naivepost.json into it, remove the file, rename into place. Interrupted, it
// leaves the file plus <name>.adopting and refuses next time rather than
// guessing. A folder is returned untouched.
func (a *App) adoptLegacy(path string) (string, error) {
	if fi, err := os.Stat(path); err == nil && fi.IsDir() {
		return path, nil
	}
	// the naivepost.json a file manager or an old recents list can point at
	if strings.EqualFold(filepath.Base(path), projMain) {
		return filepath.Dir(path), nil
	}
	b, err := os.ReadFile(path)
	if err != nil {
		return "", fmt.Errorf("open %s: %w", path, err)
	}
	dir := withProjExt(path)
	// root/project.json was never a name anybody chose -- it was the working
	// copy, before a project was something you could double-click -- and the
	// working copy has a name of its own now
	if path == filepath.Join(a.root, "project.json") {
		dir = filepath.Join(a.root, workName)
	}
	if dir != path && exists(dir) {
		return "", fmt.Errorf("%s was written by an older build and %s already exists: "+
			"move one of them aside", filepath.Base(path), filepath.Base(dir))
	}
	tmp := dir + ".adopting"
	if exists(tmp) {
		return "", fmt.Errorf("%s is left over from an interrupted open: move it aside, "+
			"or rename it to %s", filepath.Base(tmp), filepath.Base(dir))
	}
	if old := path + ".data"; exists(old) {
		if err := os.Rename(old, tmp); err != nil {
			return "", fmt.Errorf("moving %s aside: %w", filepath.Base(old), err)
		}
	} else if err := os.MkdirAll(tmp, 0o755); err != nil {
		return "", fmt.Errorf("making %s: %w", tmp, err)
	}
	// ...and the paths inside it that pointed into the folder being renamed.
	// The stems the voice split writes live in there, and so does anything Add
	// copied in: left absolute they would name a folder that no longer exists,
	// the rows would be pruned on the next open, and the autosave would write
	// the pruned list back. Stored project-relative they cannot come apart
	// again (storePath).
	b = []byte(strings.ReplaceAll(string(b), `"`+path+".data"+string(filepath.Separator), `"`+projPrefix))
	if err := os.WriteFile(projFile(tmp), b, 0o644); err != nil {
		return "", fmt.Errorf("writing %s: %w", projFile(tmp), err)
	}
	if err := os.Remove(path); err != nil {
		return "", fmt.Errorf("removing %s, whose contents are now in %s: %w",
			filepath.Base(path), filepath.Base(tmp), err)
	}
	if err := os.Rename(tmp, dir); err != nil {
		return "", fmt.Errorf("naming %s: %w", filepath.Base(dir), err)
	}
	a.logf(">>> %s is a folder now, with the project inside it as %s — everything this "+
		"session writes is in there", filepath.Base(dir), projMain)
	return dir, nil
}

// migrateFolders moves a project's data through the two renames there have
// been: step1/..step6/ into folders named for their steps, then inputs/ and
// understand/{describe,transcript} under prepare/. Once, on the open that
// finds them; a folder already under its new name is left alone. Logged.
func (a *App) migrateFolders() {
	if a.outDir == "" {
		return
	}
	for _, m := range [][2]string{
		{"step1", "inputs"}, {"step2", "understand"}, {"step3", "cut"},
		{"step4", "narrate"}, {"step5", "produce"}, {"step6", "publish"},
	} {
		from, to := filepath.Join(a.outDir, m[0]), filepath.Join(a.outDir, m[1])
		if !exists(from) || exists(to) {
			continue
		}
		if err := os.Rename(from, to); err != nil {
			a.logf("!!! could not move %s/ to %s/: %v -- the files are still under the old name", m[0], m[1], err)
			continue
		}
		a.logf(">>> moved %s/ to %s/ -- the folders are named for their steps now", m[0], m[1])
	}
	// ...and the three folders the FIRST step fills into one folder of its
	// own. inputs/ sat beside understand/, and understand/ held the other
	// two, so one step was two places on disk -- and its page carried three
	// output buttons where every other step has one.
	for _, m := range [][2]string{
		{"inputs", filepath.Join("prepare", "inputs")},
		{filepath.Join("understand", "describe"), filepath.Join("prepare", "describe")},
		{filepath.Join("understand", "transcript"), filepath.Join("prepare", "transcript")},
	} {
		from, to := filepath.Join(a.outDir, m[0]), filepath.Join(a.outDir, m[1])
		if !exists(from) || exists(to) {
			continue
		}
		if err := os.MkdirAll(filepath.Dir(to), 0o755); err != nil {
			a.logf("!!! could not make %s/: %v -- %s/ is still where it was", filepath.Dir(m[1]), err, m[0])
			continue
		}
		if err := os.Rename(from, to); err != nil {
			a.logf("!!! could not move %s/ to %s/: %v -- the files are still under the old name", m[0], m[1], err)
			continue
		}
		a.logf(">>> moved %s/ to %s/ -- Prepare's work is under its own name now", m[0], m[1])
	}
	// what is left of understand/ once its two are out: nothing, and an empty
	// folder beside the named ones is a step somebody will go looking for.
	// Remove, not RemoveAll -- anything still in there is not ours to throw
	// away, and the call simply fails.
	os.Remove(filepath.Join(a.outDir, "understand"))
}

// saveProjectTo is the explicit save behind the button: it names the file the
// autosave follows from here on, and says so.
//
// The work follows the name. Transcripts, frames and renders live in a folder
// derived from the file name (see projExt), so a Save As without the move would
// leave a project whose every output is filed under the name it used to have,
// and which reads, on every page, as one that has never been run.
func (a *App) saveProjectTo(path string) {
	path = withProjExt(path)
	if path != a.projPath {
		a.moveOutputs(a.outDir, dataDir(path))
		a.setProject(path)
	}
	a.rememberProject(path)
	a.saveProjectNow()
	a.setStatus("project saved")
}

// moveOutputs takes one project's output folder to another's name. A rename is
// instant on one filesystem and cannot half-happen, which is why it is a rename
// and not a copy: nobody presses Save expecting gigabytes of frames to be
// duplicated. It is also the whole check -- a rename onto a folder that already
// has work in it fails, and onto one that is empty succeeds, which is exactly
// the answer wanted in both cases. When it will not go through, across
// filesystems or into a name in use, the files stay where they are and the log
// says where, because a Save that silently moved work would be worse than one
// that did not.
//
// Silent when there is nothing to move, which is the ordinary case: a project
// saved before it has been run has an empty folder or none at all.
func (a *App) moveOutputs(from, to string) {
	n, _, _ := countOutputs(from)
	if n == 0 {
		return
	}
	if err := os.Rename(from, to); err != nil {
		a.logf("!!! could not move the output folder to %s: %v -- the %d file(s) are still in %s",
			to, err, n, from)
		return
	}
	a.logf(">>> moved the output folder to %s", to)
}

// autosaveTick is how often the project is compared with what is on disk.
// Long enough that typing a prompt is not a write per keystroke, short enough
// that closing the window after a change loses nothing anyone would notice.
const autosaveTick = 2000

// startAutosave keeps the file up to date without a button. Save Project stays
// -- it is how a variant gets a name -- but it stopped being the thing that
// decides whether tonight's work exists in the morning.
//
// A ticker rather than a debounce on every setter, for the reason projectJSON
// gives. The tick costs one marshal of a few kB, and closing the window
// flushes whatever the last tick has not seen yet.
func (a *App) startAutosave() {
	a.projSaved = a.projectJSON() // what the startup load already put in memory
	glib.TimeoutAdd(autosaveTick, func() bool {
		a.flushProject()
		a.flushPrompts() // the prompts are the machine's, not the project's, and
		return true      // are written on the same tick for the same reason
	})
	// the tick leaves a gap of up to autosaveTick between a change and the
	// file, and the change most likely to land in it is the LAST one -- you
	// click it, you see it happen, you close the window. That gap is what made
	// a narrator tag come back after it was taken off: the tick that caught
	// the tagging ran, the one that would have caught the untagging never did.
	if a.win != nil {
		a.win.ConnectCloseRequest(func() bool {
			a.narr.flushSave() // ...and the same for the last line typed
			if a.ed != nil {
				a.ed.flushLine() // the red line, as it stood when the window closed (cut_line.go)
			}
			a.flushProject()
			a.flushPrompts()
			return false // false lets the window close; this only writes
		})
	}
}

// flushProject writes the project if it differs from what is on disk. The
// compare is what makes a tick free in practice: a session spends most of its
// time not changing anything, and an unchanged project writes nothing at all.
func (a *App) flushProject() {
	if b := a.projectJSON(); b != nil && !bytes.Equal(b, a.projSaved) {
		a.saveProjectNow()
	}
}

func (a *App) loadProjectFrom(path string) {
	path, err := a.adoptLegacy(path)
	if err != nil {
		a.logf("!!! %v", err)
		a.setStatus("could not open that project — see log")
		return
	}
	b, err := os.ReadFile(projFile(path))
	if err != nil {
		a.logf("load project: %v", err)
		return
	}
	var p Project
	if err := json.Unmarshal(b, &p); err != nil {
		a.logf("load project: %v", err)
		return
	}
	// The folder BEFORE the pages: a source that lives inside the project is
	// stored relative to it (storePath), and applyProject is what resolves
	// those. Read with the last project's folder still set, they would resolve
	// into the last project's folder, not be there, and be pruned -- which is
	// how a session loses the sources it was cut from.
	a.projPath, a.outDir = path, dataDir(path)
	a.applyProject(p)
	// ...and then the rest of what opening a project means: the pages that
	// read the folder, the voice, the autosave's target.
	a.setProject(path)
	if p.OutDir != "" && a.loadPath(p.OutDir) != a.outDir {
		a.logf("!!! this project used to write into %s and now writes into %s -- "+
			"the old folder is untouched", a.loadPath(p.OutDir), a.outDir)
	}
	a.rememberProject(a.projPath)
	a.projSaved = a.projectJSON()
}

// applyProject puts a project on screen -- every page of it. Split out of
// loadProjectFrom because New Project needs exactly this and nothing else:
// handed a blank project it walks the same list and each page comes back to its
// default, which is a reset that cannot forget a page. A newProject() that
// cleared what it could remember to clear would leave the last session's
// narration settings, or its thumbnail, in a project claiming to be new -- the
// invisible kind of bug applyPromptStyles's comment is about.
func (a *App) applyProject(p Project) {
	// the folders first: they are where the file choosers open, and where a
	// pre-merge project's half-relative source names are resolved from
	vid, aud := srcDirs(p)
	a.vidDir, a.audDir = a.loadPath(vid), a.loadPath(aud)
	items := a.projectSources(p)
	// a project entry whose file vanished (renamed, moved) must be LOUD: a
	// silently-dropped source once cost half a debugging session
	for _, it := range items {
		if !exists(it.path) {
			a.logf("!!! %s is not there any more -- dropped from the session", it.path)
		}
	}
	a.srcList.load(items)
	a.srcList.prune()
	a.setFrameInterval(p.Interval)
	if p.FrameScale != "" {
		a.setFrameScale(p.FrameScale)
	}
	a.applyStyle(p.Style)
	a.applyChain(p.RunSteps)
	a.applyLanguage(p.Language)
	a.adoptProjectPrompts(p.Prompts)
	a.applySessionCtx(p.Context)
	a.applyNarrOff(p.NoNarration)
	a.applyRefSources(p.RefSources)
	a.migrateHints(p)
	a.applyProdSettings(p.Produce)
	a.applyPublish(p.Publish)
	// Every page except the ones drawn from the output folder -- the Cut page,
	// the counts, the narration. The stored out_dir is deliberately not read:
	// the folder is derived from the file's own name (see projExt), and the
	// caller sets both with setProject, which is what redraws those pages.
}

// blankProject is what New Project starts from. Interval and Produce are
// stated because their zero values are real and wrong (0 seconds is EVERY
// frame; a zeroed produce block is a 0-CRF, 0-fps render). The output folder
// is not a setting: it follows the project file.
func blankProject() Project {
	prod := defaultProdSettings()
	return Project{
		Interval:   frameStops[defFrameStop],
		FrameScale: scalePresets[0].Name,
		Produce:    &prod,
	}
}

// newProject empties the session into a project of its own at path. The
// project that was open is not deleted and not written over: it stays on disk
// exactly as it was, with everything it has written, and going back to it is
// Load, not undo.
func (a *App) newProject(path string) {
	a.applyProject(blankProject())
	// where the project was put is where its footage almost certainly is, so
	// that is where Add opens -- not input_video/ under the folder naivepost was
	// started from, which for a session on a card or a scratch disk is a folder
	// with nothing in it. The working copy keeps those defaults: nobody chose
	// where it went.
	if dir := filepath.Dir(path); dir != a.root {
		a.vidDir, a.audDir = dir, dir
	}
	a.setProject(path)
	// the new project is now the open one, and that is a decision, not an
	// absence: without this the next launch reopens the project this was meant
	// to get away from (see rememberProject)
	a.rememberProject(a.projPath)
	a.saveProjectNow()
	a.setStatus("new project — " + filepath.Base(path))
	a.logf(">>> new project %s -- the session is empty; outputs on disk are untouched", path)
}

// newProjectAt is the answer to that dialog: the folder is made and the empty
// session moves into it.
//
// A name that is already a project is refused rather than started over. The
// blank session would be written into it within the second (saveProjectNow),
// on top of a project whose frames, cut and narration are all still in that
// folder and would then belong to a session that knows nothing about them.
func (a *App) newProjectAt(path string) {
	path = withProjExt(path)
	if exists(projFile(path)) {
		a.setStatus(filepath.Base(path) + " is a project already — open it, or pick another name")
		a.logf("!!! new project: %s is a project already -- nothing was changed", path)
		return
	}
	a.newProject(path)
}

// askNewProject asks what to call it and where to put it.
//
// New used to make session.naivepost in the folder naivepost was started from,
// which is the one folder a session's own footage is never in: every project
// began in the checkout and had to be moved by a Save As afterwards. A project
// is a folder you name and place (projExt), and this is where that happens.
//
// It opens beside the open project, which is where saveProjectDialog opens and
// for the same reason -- the last project is nearly always beside the footage
// this one is about.
func (a *App) askNewProject() {
	dir := filepath.Dir(a.projPath)
	a.saveAs("New project", dir, freeProjName(dir), nil, a.newProjectAt)
}

// freeProjName is what the name box starts with: today's date, which is how the
// recorders name their own files, and a number after it when a project of that
// name is already there -- a second session in one day is not a mistake worth
// a refusal.
func freeProjName(dir string) string {
	day := time.Now().Format("2006-01-02")
	name := day + projExt
	for i := 2; exists(filepath.Join(dir, name)); i++ {
		name = fmt.Sprintf("%s-%d%s", day, i, projExt)
	}
	return name
}

// newProjectDialog asks first, and then asks where (askNewProject). The
// session is not a file until Save names one, so New on a session nobody saved
// throws away work that exists nowhere else -- and it is one click from Load
// and Save in the header bar, which are the two buttons a hand reaching for it
// is aiming between.
//
// Nothing to lose, no question: an empty session being emptied is not a
// decision worth interrupting anyone for. The name dialog still comes.
func (a *App) newProjectDialog() {
	if a.running {
		a.setStatus("stop the run first — a new project would pull its inputs out from under it")
		return
	}
	if len(a.srcList.items) == 0 && a.sessionCtx() == "" {
		a.askNewProject()
		return
	}
	detail := "The sources, the session context and every prompt edit go back to empty. " +
		"Files already written to the output folder are left alone."
	if filepath.Base(a.projPath) != workName {
		detail += "\n\n" + filepath.Base(a.projPath) + " stays on disk as it is, " +
			"with everything it has written -- this session simply stops being it."
	} else {
		detail += "\n\nThis session has never been saved under a name of its own, " +
			"so there is nothing to come back to."
	}
	// the ellipsis is the promise that the press is not the last word: what
	// follows is the box that names the new project and puts it somewhere
	a.confirm("Start a new project?", detail, "Start new…", a.askNewProject)
}

// confirm is a modal yes/no with the red button: what the left button does
// cannot be undone. Cancel has the focus, so a blind Enter does nothing.
func (a *App) confirm(question, detail, okLabel string, ok func()) {
	var win *gtk.Window
	go1 := gtk.NewButtonWithLabel(okLabel)
	go1.AddCSSClass("destructive-action")
	go1.ConnectClicked(func() { win.Close(); ok() })
	cancel := gtk.NewButtonWithLabel("Cancel")
	win = a.modal(question, detail, 420, nil, cancel, go1)
	cancel.ConnectClicked(func() { win.Close() })
	cancel.GrabFocus()
	win.SetVisible(true)
}

// migrateHints folds a pre-merge project's notes into the prompts they used to
// be appended to. The runners did that concatenation at request time, using
// exactly these lead-ins, so folding them in preserves what the model saw;
// dropping them would silently change what a reloaded project sends. Must run
// after applyPromptStyles, which is what puts the prompt in the box in the first
// place.
func (a *App) migrateHints(p Project) {
	fold := func(key, notes, lead string) {
		notes = strings.TrimSpace(notes)
		if notes == "" {
			return
		}
		cur := a.prompt(key)
		if strings.Contains(cur, notes) {
			return // already folded in, by this load or an earlier one
		}
		merged := cur + "\n" + lead + "\n" + notes
		a.setPrompt(key, merged)
		if tv := a.promptViews[key]; tv != nil {
			tv.Buffer().SetText(merged)
		}
		if a.log != nil { // not during tests, which have no window
			a.logf(">>> moved this project's notes into the %q prompt", key)
		}
	}
	fold("describe", p.DescribeHints, "Editor's notes about this footage -- trust them:")
	fold("fix", p.TranscriptHints, "Editor's notes -- trust them:")
	fold("cut", p.CutHints, "Editor's notes about this session -- trust them and let them guide what matters:")
	fold("narrate", p.NarrateHints, "Editor's goals and context -- honor them:")
}

// srcDirs says where a project's two source folders are, still relative to root
// where it stored them that way. A project written before the split names one
// folder that had to hold input_video/ and input_audio/, and those two
// subfolders are exactly the folders it meant -- so an old project keeps
// pointing at the same files instead of opening on two empty lists.
func srcDirs(p Project) (vid, aud string) {
	vid, aud = p.VidDir, p.AudDir
	if vid == "" {
		vid = filepath.Join(p.InDir, "input_video")
	}
	if aud == "" {
		aud = filepath.Join(p.InDir, "input_audio")
	}
	return vid, aud
}

// addFilesDialog adds files to the session, several at a time -- a card holds
// one take per angle and picking them one dialog at a time is not a workflow.
func (a *App) addFilesDialog() {
	exts := make([]string, 0, len(mediaExt))
	for e := range mediaExt {
		exts = append(exts, e)
	}
	a.pickFiles("Add sources", a.vidDir, extFilter("Audio and video", exts...), a.askImport)
}

// addSources puts files in the list and remembers where they came from, so the
// next chooser opens where the last one did. The two folders are no longer what
// the list is made of -- they are only where it starts looking.
func (a *App) addSources(paths ...string) {
	n := a.srcList.add(paths...)
	for _, p := range paths {
		if isVideo(p) {
			a.vidDir = filepath.Dir(p)
		} else {
			a.audDir = filepath.Dir(p)
		}
	}
	switch {
	case n == 0:
		a.setStatus("already in the session — nothing added")
	case n == len(paths):
		a.setStatus(fmt.Sprintf("added %d source(s)", n))
	default:
		a.setStatus(fmt.Sprintf("added %d of %d — the rest were already in", n, len(paths)))
	}
}

// saveProjectDialog names the project. It opens beside the open project rather
// than in the root, because Save As on /mnt/rec/tom.json.naivepost is nearly
// always another name in /mnt/rec -- that is where the footage is.
//
// Not during a run: the save moves the output folder the run is writing into.
func (a *App) saveProjectDialog() {
	if a.running {
		a.setStatus("stop the run first — saving under a new name moves the folder it is writing into")
		return
	}
	// Save, not SelectFolder: a name is typed here, and the name is what the
	// folder will be called (withProjExt). A folder chooser can only pick one
	// that exists.
	a.saveAs("Save the project", filepath.Dir(a.projPath), filepath.Base(a.projPath), nil, a.saveProjectTo)
}

// loadProjectDialog picks a project FOLDER -- which is what a project is
// (projExt). A project from an older build is a file and cannot be picked
// here; it is opened by double-clicking it, from the recents list, or from the
// command line, and adopted into a folder on the way in (adoptLegacy).
func (a *App) loadProjectDialog() {
	a.pickFolder("Open a project", filepath.Dir(a.projPath), a.loadProjectFrom)
}

// openFolder shows a directory in the user's file manager -- via the desktop
// portal (works on any DE, unlike bare xdg-open from an app context), falling
// back to xdg-open. The directory is created first: launching a missing path
// silently does nothing, which reads as a dead button.
func (a *App) openFolder(dir string) {
	if err := os.MkdirAll(dir, 0o755); err != nil {
		a.logf("open folder: %v", err)
		return
	}
	l := gtk.NewFileLauncher(gio.NewFileForPath(dir))
	l.Launch(context.Background(), &a.win.Window, func(res gio.AsyncResulter) {
		if err := l.LaunchFinish(res); err != nil {
			a.logf("portal launch failed (%v), trying xdg-open", err)
			if err := exec.Command("xdg-open", dir).Start(); err != nil {
				a.logf("xdg-open: %v", err)
			}
		}
	})
}

// ---- output inspection ------------------------------------------------------

// summarizeOutputs is the one-line answer: how many files are under dir, and
// how long ago the newest was written. Recursive, because Prepare's output is
// mostly frames in subdirectories -- a top-level count would say "3 entries"
// about four thousand files. The age is what tells a finished step from a
// stale one at a glance.
func summarizeOutputs(dir string) string {
	n, _, size := countOutputs(dir)
	if n == 0 {
		return "nothing yet"
	}
	// how many and how big, and nothing else. It used to end on "newest 12 min
	// ago", which is a fact about the last run rather than about what is on
	// disk -- and the run that wrote it finished in front of you.
	return fmt.Sprintf("%s, %s", plural(n, "file"), humanSize(size))
}

// countOutputs is the same walk with the two halves kept apart. Prepare
// shows three of these at once and puts the age on hover: three sentences of
// the form above, side by side, is a paragraph across the bottom of a page.
func countOutputs(dir string) (n int, newest time.Time, size int64) {
	filepath.WalkDir(dir, func(_ string, d fs.DirEntry, err error) error {
		if err != nil || d.IsDir() {
			return nil // an unreadable subtree is not worth a whole error line here
		}
		n++
		if fi, err := d.Info(); err == nil {
			size += fi.Size()
			if fi.ModTime().After(newest) {
				newest = fi.ModTime()
			}
		}
		return nil
	})
	return n, newest, size
}

func humanAgo(t time.Time) string {
	d := time.Since(t)
	switch {
	case d < time.Minute:
		return "just now"
	case d < time.Hour:
		return fmt.Sprintf("%d min ago", int(d.Minutes()))
	case d < 24*time.Hour:
		return fmt.Sprintf("%d h ago", int(d.Hours()))
	case d < 48*time.Hour:
		return "yesterday"
	default:
		return fmt.Sprintf("%d days ago", int(d.Hours()/24))
	}
}

// logListMax is how many files a directory gets to name before it becomes a
// count. Prepare writes one frame per second of footage; a log carrying four
// thousand f000123.jpg lines is a log nobody scrolls, and the run's own
// messages would be buried in it.
const logListMax = 12

// logOutputs names what a step actually wrote, in the log, and returns the file
// count for the status line. The page shows the count; the names live here,
// because that is the place you can scroll back through after a run.
// GUI thread only -- callers are completion handlers.
func (a *App) logOutputs(label, dir string) int {
	n := a.logTree(dir, label) // label leads every path, so the log says which step
	if n == 0 {
		a.logf("    %s/: nothing written", label)
	}
	return n
}

func (a *App) logTree(dir, rel string) int {
	ents, err := os.ReadDir(dir)
	if err != nil {
		return 0
	}
	n := 0
	var files []os.DirEntry
	for _, e := range ents {
		if e.IsDir() {
			n += a.logTree(filepath.Join(dir, e.Name()), filepath.Join(rel, e.Name()))
			continue
		}
		files = append(files, e)
	}
	if len(files) > logListMax {
		a.logf("    %s/ — %d files (%s … %s)", rel, len(files),
			files[0].Name(), files[len(files)-1].Name())
	} else {
		for _, e := range files {
			size := ""
			if fi, err := e.Info(); err == nil {
				size = "  (" + humanSize(fi.Size()) + ")"
			}
			a.logf("    %s%s", filepath.Join(rel, e.Name()), size)
		}
	}
	return n + len(files)
}

func humanSize(n int64) string {
	switch {
	case n > 1<<20:
		return fmt.Sprintf("%.1f MB", float64(n)/(1<<20))
	case n > 1<<10:
		return fmt.Sprintf("%.0f kB", float64(n)/(1<<10))
	default:
		return fmt.Sprintf("%d B", n)
	}
}
