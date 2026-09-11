package main

// Prepare: everything before there is anything to cut, one tab and one ▶ --
// transcripts and frames, then describing and fixing. Sources on the left; on
// the right one box switched by a menu: the session context first, then every
// system prompt in pipeline order (prepedit.go). Jobs stay separate on disk
// (prepare/inputs, describe, transcript) because the describer resumes per
// chunk and the fixer does not. Runners: pipeline.go, describe.go, cut.go.

import (
	"errors"
	"fmt"
	"path/filepath"
	"strings"

	"github.com/diamondburned/gotk4/pkg/glib/v2"
	"github.com/diamondburned/gotk4/pkg/gtk/v4"
	"github.com/diamondburned/gotk4/pkg/pango"
)

type preproc struct {
	a *App

	inputs  *gtk.Label // one line: what the sources are, and what they become
	prepOut *gtk.Label // how much is already in prepare/, the step's own folder
}

// ---- page ------------------------------------------------------------------

func (a *App) buildPrep() gtk.Widgetter {
	p := &preproc{a: a}
	a.prep = p

	// One line, not a listing, and it goes on the shared bottom bar beside the
	// Outputs group (inputsLabel): what the step reads, left of what it has
	// written. The names and the per-source arithmetic go to the log when the
	// run starts -- that is scrollable and this is not.
	p.inputs = inputsLabel()
	a.inStack.AddNamed(p.inputs, "prep")

	// Files left, context and prompts right (prepedit.go), a handle between
	// opening at the middle; both sides resize with the window and stand six
	// off the handle like every other divider in the app.
	bench := a.prepEditor()
	gtk.BaseWidget(bench).SetMarginStart(6)
	sources := a.buildSources()
	sources.SetMarginEnd(6)

	outer := gtk.NewPaned(gtk.OrientationHorizontal)
	outer.SetStartChild(sources)
	outer.SetEndChild(bench)
	outer.SetResizeStartChild(true)
	outer.SetResizeEndChild(true)
	openAtHalf(outer)
	// Shrink off on both ends: shrink means a child may be allocated less than
	// it needs, and what these two need includes a heading row and a button
	// row. With it off, GtkPaned clamps the handle to what both children need,
	// so a shorter window moves the handle instead of hiding half a column.
	outer.SetShrinkStartChild(false)
	outer.SetShrinkEndChild(false)
	outer.SetVExpand(true)
	// 12 from the window's edges, like the columns on Cut and Narrate, so the
	// boxes line up with the Inputs row above them and with the same boxes one
	// tab over.
	outer.SetMarginStart(12)
	outer.SetMarginEnd(12)
	// and 8 off the tabs above and the shared bar below, so the Freq row and
	// the editor frame do not sit on the transport buttons -- the breathing
	// room every other edge of every page has
	outer.SetMarginTop(8)
	outer.SetMarginBottom(8)

	// One open-folder button for prepare/ with how much is in it; the three
	// subfolders are the folder's business. Rides the shared bottom bar
	// (outStack in main.go).
	outRow := gtk.NewBox(gtk.OrientationHorizontal, 8)
	outBtn := gtk.NewButtonFromIconName("folder-open-symbolic")
	outBtn.SetTooltipText("prepare/ — this step's three folders:\n" +
		"inputs/ — the frames, the per-source transcripts and who spoke when\n" +
		"describe/ — the event logs, one per video\n" +
		"transcript/ — the fixed transcripts, the subtitles and the session timeline")
	outBtn.ConnectClicked(func() { a.openFolder(a.prepareDir()) })
	p.prepOut = gtk.NewLabel("")
	outRow.Append(outBtn)
	outRow.Append(gtk.NewLabel("Prepare:"))
	outRow.Append(p.prepOut)
	a.outStack.AddNamed(outRow, "prep")

	// The work, and nothing above it -- no prompt row at the bottom any more,
	// because this page's prompts live in the right-hand box now, behind its
	// menu, and no Inputs row at the top: what this step reads and what it has
	// written are the two groups on the shared bar below all of it.
	page := gtk.NewBox(gtk.OrientationVertical, 4)
	page.Append(outer)

	p.refresh()
	return page
}

func (a *App) buildSources() *gtk.Box {
	// every edit to the list changes what a run would see and who the narration
	// would be spoken by, so the snapshot and the voice picker are refreshed
	// from here rather than from each of the six things that can edit a row
	a.srcList = newSourceList(func() {
		a.snapSources()
		if a.voicePick != nil {
			a.voicePick.refreshNarrators()
		}
		a.prep.refresh()
		a.refreshCut() // the tracks ARE this list: a row added or unmarked changes them
	})

	// the frame controls: how often, then how big. A stepper over the same
	// discrete stops the slider had, each..5s, typable by hand.
	a.interval = newFreqPick()

	labels := make([]string, len(scalePresets))
	for i, p := range scalePresets {
		labels[i] = p.Label
	}
	a.scalePick = gtk.NewDropDownFromStrings(labels)
	a.scalePick.SetTooltipText("Frame size — Original keeps the video's own size")
	a.setFrameScale("original")
	a.scalePick.SetVAlign(gtk.AlignCenter)

	// The language, on the page whose run listens -- in Settings it was a property
	// of the machine, three tabs from the sources it was wrong about. Free text:
	// the code is the server's to interpret.
	a.langEntry = gtk.NewEntry()
	a.langEntry.SetWidthChars(4)
	a.langEntry.SetMaxWidthChars(6)
	a.langEntry.SetPlaceholderText(defLanguage)
	a.langEntry.SetTooltipText("Language of this session's speech, as the ASR model spells it (en, de, …) — " +
		"the wrong one transcribes into gibberish. Empty means " + defLanguage)
	a.langEntry.SetVAlign(gtk.AlignCenter)
	// the cache follows every keystroke rather than a commit: unlike the frame
	// stepper there is nothing to parse, and a value that only counts once you
	// tab away is a value the next run silently disagrees with
	a.langEntry.ConnectChanged(func() { a.setLanguage(a.langEntry.Text()) })

	// One list, the width of the page; each row says what its file is for. The
	// buttons say "source" so the list needs no heading.
	addBtn := gtk.NewButtonWithLabel("Add source files…")
	addBtn.SetTooltipText("Add recordings or footage — several at once")
	addBtn.ConnectClicked(a.addFilesDialog)
	addDirBtn := gtk.NewButtonWithLabel("Add source folder…")
	addDirBtn.SetTooltipText("Add everything playable in a folder")
	addDirBtn.ConnectClicked(a.addFolderDialog)
	// The legend was here: the four row icons with their words, once, along
	// the top of the list. It cost a line of the page permanently to answer a
	// question asked twice -- and it answered it a hand's width away from the
	// buttons it was about, which is the one place the eye is not while it is
	// deciding what a symbol does. The words are on the buttons themselves
	// now: every one of them ends its tooltip with the whole row (srcRowKey),
	// so hovering any symbol says what all four are.

	// Where an added file goes, decided once and remembered with the project
	// rather than asked on every Add: into the project folder, or left where
	// it is. Copy is the default -- a project is one folder that can be moved
	// or zipped, and a session whose footage is on the card it was recorded on
	// is not one thing -- and the tick comes off for the machine that recorded
	// the footage, where 20 GB of capture does not want a twin.
	a.copyTick = gtk.NewCheckButtonWithLabel("copy into project")
	a.copyTick.SetActive(true)
	a.copyTick.SetTooltipText("Ticked, an added file is copied into the project's sources/ folder, " +
		"so the project holds everything it needs. Unticked, the file is referenced where it is: " +
		"nothing is duplicated, and the session breaks if it moves.")
	a.copyTick.ConnectToggled(func() {
		if a.refQuiet {
			return
		}
		a.refSources = !a.copyTick.Active()
		a.saveProjectNow()
	})

	addRow := gtk.NewBox(gtk.OrientationHorizontal, 8)
	addRow.Append(addBtn)
	addRow.Append(addDirBtn)
	addRow.Append(a.copyTick)

	listScroll := gtk.NewScrolledWindow()
	listScroll.SetChild(a.srcList.box)
	listScroll.SetVExpand(true)
	// what the heading used to say, kept on the thing it was about
	listScroll.SetTooltipText("Every file here is transcribed, and placed on the session clock by the timestamp in its name")
	// this row is this side's heading, and it joins the size group every
	// editor's heading is in (editorBody): the two sides of the divider are
	// read against each other, so a button row a few px taller than the label
	// row opposite it starts the list a few px above the box it is beside.
	// The same group is why the Publish page's fields line up with the prompts
	// (publisher.heading).
	if a.headGroup == nil {
		a.headGroup = gtk.NewSizeGroup(gtk.SizeGroupVertical)
	}
	a.headGroup.AddWidget(addRow)

	sources := gtk.NewBox(gtk.OrientationVertical, 4)
	sources.SetVExpand(true)
	// the same 4 px the editor's own box stands off the top with, so the two
	// frames begin on one line (editorFrame)
	sources.SetMarginTop(4)
	sources.Append(addRow)
	sources.Append(listScroll)

	// The bottom line, one line: how the frames are taken, and what language
	// they are spoken in. Where it all lands used to be chosen here too; it is
	// the project file's own folder now (project.go), so there is nothing to
	// choose, and what has been written into it is counted along the bottom of
	// the page where the other two folders this step writes are.
	bottom := gtk.NewBox(gtk.OrientationHorizontal, 8)
	bottom.Append(gtk.NewLabel("Freq:"))
	bottom.Append(a.interval.box)
	bottom.Append(a.scalePick)
	bottom.Append(gtk.NewLabel("Language:"))
	bottom.Append(a.langEntry)
	// What kind of video this is, mechanically: a read to camera is edited as
	// text and its picture is never described; a session is described and
	// its moments picked. Not a wording -- the old Style dropdown below was
	// one, and went to the context box -- but which PIPELINE runs, and that
	// is this page's to say, since it is this page's ▶ that runs it.
	a.stylePick = gtk.NewDropDownFromStrings(styleLabels)
	a.stylePick.SetTooltipText("Lecture: a read to camera -- the speech is the video, cut by removing " +
		"the words said twice and the false starts. Gaming: a session -- the cut picks the " +
		"moments worth keeping. Both describe the picture; only what the cut is chosen from differs.")
	a.stylePick.SetVAlign(gtk.AlignCenter)
	a.stylePick.NotifyProperty("selected", func() {
		if !a.styleQuiet {
			a.saveProjectNow()
		}
	})
	bottom.Append(gtk.NewLabel("Style:"))
	bottom.Append(a.stylePick)
	// The Style dropdown was here, after Language: which kind of video ▶
	// Suggest built -- highlights, a rating, a Short -- turning every prompt
	// at once to a wording of that name. It is gone. What kind of video this
	// is, is a fact about the session like everything else on this page, and
	// the box beside these controls is where the session's facts go: said
	// there it reaches every step, it outranks the wordings (ctxRule), and
	// there is one place to say it instead of two that can disagree.

	// No margins of its own: it is one side of the page's divider now, and the
	// page keeps its columns 12 from the window's edges as Cut and Narrate do.
	box := gtk.NewBox(gtk.OrientationVertical, 12)
	box.SetVExpand(true)
	box.Append(sources)
	box.Append(bottom) // how the frames are taken, under the list they come from
	return box
}

// ---- what will be sent -------------------------------------------------------

// refresh re-reads everything on the page that is derived from the sources or
// from the output folder: what a run would send, and how much of it is already
// there. Safe on a page that has not been built -- it is called from the
// project loader and from the rescan, both of which can run first.
func (p *preproc) refresh() {
	if p == nil || p.inputs == nil {
		return
	}
	line, detail := p.a.inputsSummary()
	p.inputs.SetText(line)
	p.inputs.SetTooltipText(detail) // the per-file arithmetic, on hover
	setOutCount(p.prepOut, p.a.prepareDir())

}

// setOutCount fills one of the three output readings: how many files on the
// row, when the newest was written on hover. Three of the one-line form side by
// side is a paragraph across the bottom of the page, and the question the row
// is asked is how much is there.
func setOutCount(l *gtk.Label, dir string) {
	if l == nil {
		return
	}
	n, newest, size := countOutputs(dir)
	if n == 0 {
		l.SetText("nothing yet")
		l.SetTooltipText("")
		return
	}
	l.SetText(fmt.Sprintf("%d files, %s", n, humanSize(size)))
	l.SetTooltipText("newest " + humanAgo(newest))
}

// inputsSummary walks the session's sources once: line is what the page shows
// (files in, requests they become), detail the same per file for the tooltip
// and the log. One walk, because it reads every video's frame directory on
// every edit. framesPerReq and fixBlock are visible only here.
func (a *App) inputsSummary() (line, detail string) {
	if a.srcList == nil {
		return "", ""
	}
	vids, auds := a.srcList.split()
	if len(vids)+len(auds) == 0 {
		return "no input files — add some above", "(no sources)"
	}
	var b strings.Builder
	frames, vision, lines, fixes := 0, 0, 0, 0
	descDir := a.describeDir()
	count := func(base string) {
		n := len(loadSeg4(a.transcriptPath(base)))
		lines += n
		fixes += (n + fixBlock - 1) / fixBlock
	}
	for _, v := range vids {
		base := baseName(v)
		fmt.Fprintf(&b, "%s\n", base)
		if p, err := a.planVideo(v, descDir); err != nil {
			fmt.Fprintf(&b, "  frames: %v\n", err)
		} else {
			scale := p.scale
			if scale == "" {
				scale = "original"
			}
			frames += len(p.frames)
			vision += p.chunks
			fmt.Fprintf(&b, "  %d frames @ %gs (%s)  ->  %d vision requests\n",
				len(p.frames), p.interval, scale, p.chunks)
		}
		count(base)
		b.WriteString(fixerLine(a.transcriptPath(base)))
	}
	for _, aud := range auds {
		base := baseName(aud)
		fmt.Fprintf(&b, "%s\n", base)
		count(base)
		b.WriteString(fixerLine(a.transcriptPath(base)))
	}
	// Names and counts, nothing else: the per-file arithmetic is the detail
	// beside this line -- the tooltip ON it (refresh), and the log when ▶
	// starts (prepRun) -- and a row that spells out what each number is FOR is
	// a paragraph wearing a row's clothes. Nobody reads it twice.
	// no count of the files: the list of them is directly under this row, with
	// what each one is for on its own button (srcRowKey). A row that counts
	// what the page below it shows is a row saying nothing.
	line = fmt.Sprintf("%d frames → %d vision · %d lines → %d fixer", frames, vision, lines, fixes)
	return line, strings.TrimRight(b.String(), "\n")
}

// inputsLabel is the line every step wears on the shared bottom bar: what the
// step reads, whole on hover. Ellipsized and capped so it cannot push the run
// bar's controls off the window.
func inputsLabel() *gtk.Label {
	l := gtk.NewLabel("")
	l.SetXAlign(0)
	l.SetEllipsize(pango.EllipsizeEnd)
	l.SetMaxWidthChars(60)
	return l
}

// plural is "1 clip" and "2 clips": the Inputs rows are read at a glance and
// "1 clip(s)" is a word nobody says out loud. Only the plural-by-s cases are
// on those rows, so this is the whole of the grammar needed.
func plural(n int, one string) string {
	if n == 1 {
		return fmt.Sprintf("%d %s", n, one)
	}
	return fmt.Sprintf("%d %ss", n, one)
}

func (a *App) transcriptPath(base string) string {
	return filepath.Join(a.inputsDir(), base, "transcript.tsv")
}

func fixerLine(path string) string {
	if !exists(path) {
		return "  transcript.tsv: missing -- not transcribed yet\n"
	}
	n := len(loadSeg4(path))
	return fmt.Sprintf("  transcript.tsv, %d lines  ->  %d fixer requests\n",
		n, (n+fixBlock-1)/fixBlock)
}

// ---- run --------------------------------------------------------------------

// prepRun validates the sources and starts the whole step: transcripts and
// frames, then describing and fixing, in one press. No describe-only or
// fix-only run: describe resumes per chunk, and half of what the fixer is for
// is the events the describer just wrote.
func (a *App) prepRun() {
	if a.running {
		return
	}
	// one source is enough: a session can be a single screen recording that is
	// both the footage and every voice on it, and it can equally be a recording
	// with no footage at all
	vids, auds := a.snapSources()
	if len(vids)+len(auds) == 0 {
		a.setStatus("add at least one source")
		return
	}
	// two sources of the same name would write into one folder under inputs/,
	// and the second would quietly overwrite the first's transcript
	if x, y := a.srcList.clash(); x != "" {
		a.logf("!!! %s and %s are both inputs/%s -- rename one", x, y, baseName(x))
		a.setStatus(fmt.Sprintf("%s and %s have the same name — rename one",
			filepath.Base(x), filepath.Base(y)))
		return
	}
	// The working copy always reflects what actually ran -- and so does the
	// named project, since saveProjectNow writes every target. It used to be
	// saveProjectTo(project.json), which also RENAMED the open project to the
	// working copy: running the step quietly stopped the autosave following the
	// file you had opened, and the header bar changed under you to say so.
	a.saveProjectNow()
	// ⏹ then ▶ is "do it again", not "carry on" -- and this is the moment to
	// act on that rather than the stop itself: pressing ⏹ has to be safe to do
	// at the end of the day, with the half-run still on disk in the morning.
	// It is the press that starts the work over that throws the work away.
	if err := a.undFreshStart(); err != nil {
		// nothing here is worth refusing to run over: say what could not be
		// cleared, and let the run pick up from what is still on disk
		a.logf(">>> could not clear the last run (%v) -- resuming it", err)
	}
	scaleName, scaleVF := a.frameScale()
	a.startPrep(vids, auds, a.frameInterval(), scaleName, scaleVF)
}

// undFreshStart is the difference between ⏸ and ⏹: paused resumes, stopped
// restarts the describing from the beginning, dropping the event logs it
// resumes from. Transcripts and frames are per-file on disk and untouched; the
// fixer never resumed anyway. Only armed by a ⏹ that had reached the
// describing.
func (a *App) undFreshStart() error {
	if !a.undRestart {
		return nil
	}
	a.undRestart = false
	return a.resetDescribe()
}

func (a *App) startPrep(videos, audios []string, interval float64, scaleName, scaleVF string) {
	a.startRun()
	a.prog(trackSTT, 0, "preparing")
	a.logExp.SetExpanded(true)
	// what went in, by name -- the page has room for a count and nothing more
	a.logf(">>> prepare: %d input files", len(videos)+len(audios))
	for _, f := range append(append([]string{}, videos...), audios...) {
		a.logf("    %s", f)
	}
	if _, detail := a.inputsSummary(); detail != "" {
		a.logf("%s", detail)
	}
	a.logCtx("prepare")
	go func() {
		described, err := a.prepare(videos, audios, interval, scaleName, scaleVF)
		glib.IdleAdd(func() {
			a.endRun()
			switch {
			case errors.Is(err, errStopped):
				// the work stays on disk until the next ▶, which is the press
				// that decides it was not wanted (undFreshStart) -- and only
				// the describing is thrown away, so a stop during the
				// transcribing arms nothing
				a.undRestart = described
				a.setStatus("stopped — finished work is kept")
			case err != nil:
				a.logf("prepare FAILED: %v", err)
				a.setStatus("prepare failed — see log")
			default:
				a.progress.SetFraction(1)
				a.logf(">>> prepare wrote:")
				n := a.logOutputs("inputs", a.inputsDir()) +
					a.logOutputs("describe", a.describeDir()) +
					a.logOutputs("transcript", a.transcriptDir())
				a.setStatus(fmt.Sprintf("prepared — %d files", n))
			}
			a.prep.refresh()
			a.updateGates()
			// the frames and the session timeline the Cut page draws are what
			// this run just wrote -- including the first time, where the tab it
			// unlocks would otherwise open on nothing
			a.refreshCut()
		})
	}()
}

// prepInputsShare is how much of the progress bar the first half gets. The two
// halves are weighted by how long they TAKE, as the tracks inside each of them
// are: transcribing an hour of audio and pulling a frame out of it every couple
// of seconds is minutes, and describing those frames is a model call per
// handful of them and is the longest wait in the app. A third to the front is
// a rule of thumb from real sessions, and being roughly right is the whole
// requirement -- what a progress bar owes you is a direction, not an ETA.
const prepInputsShare = 0.3

// prepSepShare is the slice of that first third the voice splitting takes when
// there is any to do. A third of it: the separation is one model pass over the
// audio and the transcripts are two, over the same audio, plus every frame.
const prepSepShare = 0.1

// prepare is the whole press: the transcripts and the frames, then the
// describing and the fixing. It reports whether the second half had begun,
// which is what a ⏹ needs to know -- that is the half a restart throws away.
//
// Neither half knows the other exists: each still divides its own work into two
// tracks that add up to a whole bar. qPhase is what puts each of them in its
// own slice of it, so the needle crosses the middle once and never goes back.
func (a *App) prepare(videos, audios []string, interval float64, scaleName, scaleVF string) (bool, error) {
	// the models are the server's to load, but that it HAS them is worth
	// finding out now rather than after the frame extraction -- and before the
	// separation too, since that is the phase this step now opens with and the
	// one asking for the model a server is least likely to have.
	sep := len(a.sepWanted()) > 0
	if err := a.ensureAudioModels(sep); err != nil {
		return false, err
	}
	// the splitting goes first, because it changes what the rest of this is OF:
	// a recording being split is not the file the frames come out of or the
	// transcript is of -- the two files it becomes are. With nothing flagged
	// this phase costs nothing and the two below are the two there always were.
	sepAt := 0.0
	if sep {
		a.qPhase(0, prepSepShare)
		var err error
		if videos, audios, err = a.separateVoices(videos, audios); err != nil {
			return false, err
		}
		sepAt = prepSepShare
	}
	a.qPhase(sepAt, prepInputsShare-sepAt)
	if err := a.ingest(videos, audios, interval, scaleName, scaleVF); err != nil {
		return false, err
	}
	a.qPhase(prepInputsShare, 1-prepInputsShare)
	return true, a.understand(videos, audios)
}

func (a *App) understand(videos, audios []string) error {
	// Every style is described, Lecture included. Its CUT does not need the
	// picture -- the words are the video (textedit.go) -- and this step used
	// to skip it on that ground, which was half the story: Publish picks the
	// thumbnail by what is ON a frame, and a frame nobody described is a
	// frame nothing can find. A lecture's title slide is the thumbnail.
	//
	// two jobs, one after the other, and the bar says which of the two it is
	// on: this page's ▶ is the longest press in the app, and "1/2" is the
	// difference between halfway through and nearly done.
	a.qJob(trackDescribe, "describe", 1, 2)
	if err := a.describeAll(videos, audios, 0.5); err != nil {
		return err
	}
	a.qJob(trackFix, "transcript", 2, 2)
	return a.fixTranscripts(videos, audios, 0.5)
}

// openAtHalf opens a pane's handle at the middle of the window (unset, a
// GtkPaned gives each child what its widest content wants). It waits for the
// widget to be measured; Map fires on every show, hence the once guard, and
// after the first placement the handle is the user's.
func openAtHalf(p *gtk.Paned) {
	split := false
	p.ConnectMap(func() {
		if split {
			return
		}
		split = true
		glib.IdleAdd(func() { p.SetPosition(p.AllocatedWidth() / 2) })
	})
}

// videoStyleName is the style as the page shows it, or as the project said
// before the page existed.
func (a *App) videoStyleName() string {
	if a.stylePick != nil {
		return styleOf(a.stylePick.Selected())
	}
	return a.videoStyle
}

// applyStyle is the project's answer, put on the page without saving it back.
func (a *App) applyStyle(name string) {
	a.videoStyle = name
	if a.stylePick == nil {
		return
	}
	a.styleQuiet = true
	a.stylePick.SetSelected(styleIndex(name))
	a.styleQuiet = false
}
