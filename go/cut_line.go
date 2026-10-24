package main

// The red line, remembered.
//
// Where the line stood is the one thing about a session the page forgot
// between openings: the cut came back, the effects came back, and the line
// stood at the top of the timeline, so the first thing every reopening
// started with was finding the place again. Now the second the line is on
// is written to a file of its own beside the cut and read back on the next
// reload, and the line is put there -- cued, so the picture is the frame it
// was, and scrolled into view.
//
// Its own file, not cut.json: cut.json coming into existence is what wakes
// Narrate and Produce up (persist), and a line moved over a project with no
// cut yet is not a cut. And not the project file, which is written by a
// comparing autosave and would find itself different ten times a second.
//
// Written once a second at most while the line moves, since playback moves
// it ten times a second and a file write per tick is a file write for
// nothing; the second's last position is what lands. Flushed when the window
// closes, so the position you closed on is the one you open on.

import (
	"encoding/json"
	"os"
	"path/filepath"

	"github.com/diamondburned/gotk4/pkg/glib/v2"
)

// lineSaveMs is the least time between two writes of the line.
const lineSaveMs = 1000

type lineFile struct {
	T float64 `json:"t"` // session second
}

func (a *App) linePath() string { return filepath.Join(a.cutDir(), "line.json") }

// noteLine is every hand on the line -- a click, a step, the tick -- saying
// it moved. The first says arms a write a second out; the ones that follow
// inside that second say nothing, and the write takes whatever the line is
// on when it fires.
func (ed *cutEditor) noteLine() {
	if ed.a == nil || ed.a.outDir == "" || !ed.hasPlay || ed.lineArmed {
		return
	}
	ed.lineArmed = true
	arm := ed.lineArm
	if arm == nil {
		arm = func(ms uint, fn func() bool) { glib.TimeoutAdd(ms, fn) }
	}
	arm(lineSaveMs, func() bool {
		ed.flushLine()
		return false
	})
}

// flushLine writes the line now, if a write is owed. On the window closing,
// and from the timer noteLine armed.
func (ed *cutEditor) flushLine() {
	if !ed.lineArmed {
		return
	}
	ed.lineArmed = false
	ed.saveLine()
}

func (ed *cutEditor) saveLine() {
	if ed.a == nil || ed.a.outDir == "" || !ed.hasPlay {
		return
	}
	b, _ := json.Marshal(lineFile{T: ed.playhead})
	os.MkdirAll(ed.a.cutDir(), 0o755)
	if err := os.WriteFile(ed.a.linePath(), append(b, '\n'), 0o644); err != nil {
		ed.a.logf("save line: %v", err)
	}
}

// restoreLine puts the line where the last session left it, once per
// project: reload runs again whenever a run rewrites what the page shows,
// and a line you have since moved is not to be moved back. A second no
// recording covers -- the sources changed -- is left alone, and so is a
// project that never had a line.
func (ed *cutEditor) restoreLine() {
	if ed.a == nil || ed.a.outDir == "" || ed.lineOf == ed.a.outDir {
		return
	}
	ed.lineOf = ed.a.outDir
	b, err := os.ReadFile(ed.a.linePath())
	if err != nil {
		return
	}
	var l lineFile
	if json.Unmarshal(b, &l) != nil || ed.videoAt(l.T) == nil {
		return
	}
	ed.setPlayhead(l.T)
	ed.revealPlayhead()
}
