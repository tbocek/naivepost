package main

// The prompt bench: one box on Prepare holding everything the models are told,
// in the order the pipeline sends it, behind one menu. The context is the
// first row and is not a prompt: what the editor knows about THIS session,
// carried by every request (context.go). One editor per key:
// promptViews/promptRows hold exactly the row on screen.

import (
	"fmt"

	"github.com/diamondburned/gotk4/pkg/gtk/v4"
	"github.com/diamondburned/gotk4/pkg/pango"
)

// prepRow is one row of the menu: what it is called, which store it reads and
// writes (key "" is the session context), and the detail that would otherwise
// make the name a sentence.
type prepRow struct{ menu, key, tip string }

// title is what the heading says while the row is shown. The menu names the
// job; the heading says which of the two kinds of text this is, because "Cut"
// on its own over a box of rules reads like the cut itself.
func (r prepRow) title() string {
	if r.key == "" {
		return r.menu
	}
	return r.menu + " prompt"
}

// prepRows is the menu, in the order the pipeline sends them: this page's two
// jobs, then the cut and its audit, then the narration, then the upload text.
//
// Written as a function rather than a var because the tips quote sizes that are
// constants elsewhere, and a var initialised from those reads as if the numbers
// were free to change at runtime.
func prepRows() []prepRow {
	return []prepRow{
		{"User Context", "", "What kind of video this is, who is in it, what they were " +
			"doing, how names are spelled, how long it should run, what has to end up " +
			"in it.\n\nSent with every request this project makes — the frame " +
			"describer, the transcript fixer, the cut and its three passes, the " +
			"narration, the upload text — and it outranks the prompts below, which are " +
			"written to be true of any session. Left empty, nothing is sent."},
		{"System context", "system", "The formats every job works to: the three kinds " +
			"of line, which clock a request stamps them on, and that the answer is read " +
			"by a machine.\n\nSent in front of every prompt below, so a fact about this " +
			"tool is written once instead of in each of them."},
		{"Describe", "describe", fmt.Sprintf(
			"%d frames per request, plus the last %d descriptions and up to %d spoken "+
				"lines either side as context. No frame is ever sent twice: those "+
				"descriptions are the model's only memory of what it already saw.",
			framesPerReq, recentEvents, ctxSegs)},
		{"Transcript", "fix", fmt.Sprintf(
			"The fixer: %d transcript lines per request, each block given what every "+
				"other source showed or said at the same moment.", fixBlock)},
		{"Retakes", "retake", "Last in Prepare, on the merged timeline: which " +
			"stretches were said, broken off and said again. Marked, never deleted — " +
			"the transcript keeps every word, and the cut is handed one line saying " +
			"those seconds were an attempt. The user context's script, where there is " +
			"one, says what was meant to be said."},
		{"Text edit", "textedit", "The edit of a read to camera, as text: every word that was said, " +
			"answered with the words the finished video says -- the retakes and the false starts " +
			"removed, nothing added. The session's style decides whether this or Retakes runs."},
		{"Cut", "cut", "How ▶ Suggest chooses the moments: read what the session is, " +
			"place what the context names, fill the rest, shape the whole. It assumes " +
			"nothing about the kind of video — that is what the context above is for."},
		{"Captions", "captions", "The second pass, clip by clip: what was said over each " +
			"kept clip, on screen as text effects. Cleaned as a subtitler would; the " +
			"user context says fewer, or none."},
		{"Speed", "speed", "The third pass: how fast each kept clip plays, and the " +
			"arithmetic that lands the cut on the target length. A clip with captions " +
			"on it runs at 1."},
		{"Effects", "effects", "The last pass, over the kept clips: the zooms, stops and " +
			"volume that make a moment land. Speed and captions are the passes before."},
		{"Narration", "narrate", "The craft the narration is written to: what a line is " +
			"about, how it is placed, how a pause is made. Who the voice IS comes from " +
			"the context above."},
		{"Translate", "translate", "The subtitle track in another language: the finished " +
			"video's own lines, numbered, answered one for one with the times untouched. " +
			"Which languages is chosen on the Produce page."},
		{"Upload text", "youtube", "Gets the cut and the narration — no images — and " +
			"answers with the YouTube title, the thumbnail instruction and the description."},
	}
}

// prepEditNames is the menu's rows. A row wore the wording the Style had
// turned it to -- "Cut (Highlights)" -- and there are no wordings to name any
// more: one prompt per job, and what kind of video this is goes in the
// context. The ✎ stays: it marks a prompt this machine has reworded
// (promptOwned), which is the one thing about a row worth saying when the row
// is not open. The context wears neither -- it is this session's own text,
// with nothing built-in to differ from.
func (a *App) prepEditNames() []string {
	rows := prepRows()
	out := make([]string, len(rows))
	for i, r := range rows {
		out[i] = r.menu
		if r.key == "" {
			continue
		}
		if a.promptOwned(r.key) {
			out[i] += " ✎"
		}
	}
	return out
}

// prepEditor is the right half of Prepare: one box, its heading holding the
// menu for what it shows and the prompt controls (save as new wording, reset).
// Every keystroke writes through, so there is nothing to save or close on a
// switch. editorBody like every text box on a step page (prompts.go).
func (a *App) prepEditor() gtk.Widgetter {
	rows := prepRows()

	tv := gtk.NewTextView()
	tv.SetWrapMode(gtk.WrapWord)
	tv.SetMonospace(true)
	tv.SetTopMargin(4)
	tv.SetBottomMargin(4)
	tv.SetLeftMargin(6)
	tv.SetRightMargin(6)

	lbl := gtk.NewLabel(rows[0].title())
	lbl.SetXAlign(0)
	lbl.SetHExpand(true)
	lbl.SetEllipsize(pango.EllipsizeEnd) // one line, like every other heading row
	lbl.AddCSSClass("heading")

	// Editing a prompt is what stops the project from tracking the shipped
	// wording -- a real consequence with nothing to show for it on screen, and
	// the whole reason there used to be a second "notes" box beside it. Say it
	// instead, right next to the button that undoes it. markPromptRow writes
	// this; the context row has no built-in to differ from, so it stays empty.
	mark := gtk.NewLabel("")
	mark.AddCSSClass("dim-label")
	mark.SetTooltipText("Your wording is kept in ~/.config/naivepost/prompts, so a newer " +
		"built-in prompt will not replace it. Reset puts it back.")

	add := gtk.NewButtonWithLabel("＋")
	add.AddCSSClass("flat")
	add.SetTooltipText("Save what is in the box as a new wording. The Style " +
		"dropdown finds wordings by name: saved for the cut, a new name is a " +
		"new style; saved for another job under a style's name, it is what " +
		"that style sends for the job.")
	drop := gtk.NewButtonWithLabel("Reset")
	drop.AddCSSClass("flat")

	cur := 0
	quiet := false

	// showCtx is the heading with the prompt controls taken off it. The context
	// has one wording by definition -- the one you wrote -- so a ＋ and a Reset
	// would each be a control with nothing to do, and a greyed row of them says
	// "this row is broken" rather than "this row is different".
	showCtx := func(r prepRow) {
		prompt := r.key != ""
		mark.SetVisible(prompt)
		drop.SetVisible(prompt)
	}

	tv.Buffer().ConnectChanged(func() {
		if quiet || a.promptQuiet {
			return // show below, or showPrompt, is filling the box
		}
		b := tv.Buffer()
		s := b.Text(b.StartIter(), b.EndIter(), false)
		if r := rows[cur]; r.key == "" {
			a.setSessionCtx(s)
		} else {
			a.setPrompt(r.key, s)
			a.markPromptRow(r.key)
		}
	})

	// show puts row i in the box. Registration follows the selection: the box
	// stands in promptViews/promptRows only while it shows that prompt, and is
	// a.ctxView only while it shows the context, so a project load fills
	// whichever store changed without clobbering what is actually on screen --
	// the box rereads its store on the next switch anyway.
	show := func(i int) {
		if i < 0 || i >= len(rows) {
			i = 0
		}
		if old := rows[cur]; old.key != "" {
			delete(a.promptViews, old.key)
			delete(a.promptRows, old.key)
		}
		cur = i
		r := rows[i]
		lbl.SetText(r.title())
		lbl.SetTooltipText(r.tip)
		showCtx(r)
		quiet = true
		if r.key == "" {
			a.ctxView = tv
			tv.Buffer().SetText(a.sessionCtx())
			quiet = false
			return
		}
		a.ctxView = nil
		if a.promptViews == nil {
			a.promptViews = map[string]*gtk.TextView{}
		}
		if a.promptRows == nil {
			a.promptRows = map[string]promptRow{}
		}
		a.promptViews[r.key] = tv
		a.promptRows[r.key] = promptRow{mark: mark, drop: drop}
		quiet = false
		// fills the box and the mark from the store -- which is why switching
		// away and back is lossless however much was typed in between
		a.showPrompt(r.key)
	}

	// Reset puts the built-in back, which is also what throws this machine's
	// copy away: there is one wording per job now, so an edit is the only
	// thing Reset could mean. The ＋ beside it was "Name this wording" -- a new
	// wording for the job, and for the cut a new style -- and there are no
	// styles to name any more (prompts.go).
	drop.ConnectClicked(func() {
		if r := rows[cur]; r.key != "" {
			a.resetPrompt(r.key)
		}
	})

	menu := gtk.NewStringList(nil)
	pick := gtk.NewDropDown(menu, nil)
	pick.SetTooltipText("What the box shows. The context is this session's facts, " +
		"sent with every request; the rest are the prompts, in the order the " +
		"pipeline sends them, in the order the pipeline sends them. " +
		"✎ marks one edited on this machine.")
	pick.NotifyProperty("selected", func() {
		if a.promptQuiet {
			return // prepSync is redrawing the rows; the pick did not move
		}
		if i := pick.Selected(); i < menu.NItems() && int(i) != cur {
			show(int(i))
		}
	})
	// the ✎ redraw, reached through syncPromptMarks. Quiet around the splice:
	// Splice resets the selection, and the notify above must not read that
	// reset as the user switching rows.
	a.prepSync = func() {
		fresh := a.prepEditNames()
		if sameStrings(menu, fresh) {
			return
		}
		sel := pick.Selected()
		a.promptQuiet = true
		menu.Splice(0, menu.NItems(), fresh)
		if int(sel) < len(fresh) {
			pick.SetSelected(sel) // the picked row did not move, only its label
		}
		a.promptQuiet = false
	}
	a.prepSync()
	show(0) // the context first: the row that is this session's own

	head := gtk.NewBox(gtk.OrientationHorizontal, 8)
	head.Append(lbl)
	head.Append(mark)
	head.Append(pick)
	head.Append(drop)
	return a.editorBody(head, tv)
}
