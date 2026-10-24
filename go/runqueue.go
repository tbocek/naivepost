package main

// The run queue: what one press of ▶ turned into, in order. A job fills it as
// work is found, the runner picks tasks off the front, and the bar says one
// short line: which job, which of the run's jobs, what the head task is doing,
// how far down the queue. Filenames belong in the log. The bar's FRACTION is
// not the queue's length -- the queue grows as it is opened, and a total that
// grows under a fraction drags the bar backwards -- but the weighted tracks
// (prog).

import (
	"context"
	"fmt"
	"math"
	"strings"

	"github.com/diamondburned/gotk4/pkg/glib/v2"
)

// The two tracks are the two halves of the bar. They are concurrent in Prepare
// (speech recognition on the GPU, frame extraction on the CPU) and sequential
// on Describe + Transcript, but the arithmetic is the same either way: each
// track reports its own absolute contribution and the bar shows the sum.
// Letting either write a raw fraction would make the bar bounce between them.
const (
	trackSTT    = 0
	trackFrames = 1
	// the same two tracks under the names the Describe + Transcript step uses
	// them for: its two jobs run one after the other, but each still owns half
	// the bar and its own line
	trackDescribe = trackSTT
	trackFix      = trackFrames
)

// qTrack is one half of the bar: the job on it, the work it has left, and what
// the task at the head of the queue is doing this second.
type qTrack struct {
	job        string // "describe", "speech", "narrate" — what this half is
	phase, of  int    // ...and which of the run's jobs it is, when there are two
	queued     int    // tasks known about, including the ones already taken
	taken      int    // how many have been picked up: the head's position
	what       string // what the head is doing, in two or three words
	kind       string // what a task of this queue is, when nothing else is said
	doneJob    bool   // the job finished; the other track may still be going
	everFilled bool   // a queue that was filled and emptied is not an unfilled one
}

// line is a track's whole contribution to the bar:
//
//	describe 1/2: chunk 4/12
//	transcript 2/2: fixing block 3/7
//	speech: recognising 2/3
//
// Every piece is dropped when it has nothing to say, so a job with one task and
// no phases is simply "thumbnail: drawing".
func (t qTrack) line() string {
	if t.doneJob {
		if t.job == "" {
			return ""
		}
		return t.job + " done"
	}
	head := t.job
	if t.of > 1 {
		head += fmt.Sprintf(" %d/%d", t.phase, t.of)
	}
	what := t.what
	if what == "" {
		what = t.kind
	}
	if t.everFilled && t.taken > 0 {
		pos := fmt.Sprintf("%d/%d", t.taken, max(t.queued, t.taken))
		if what == "" {
			what = pos
		} else {
			what += " " + pos
		}
	}
	switch {
	case head == "":
		return what
	case what == "":
		return head
	}
	return head + ": " + what
}

// ---- what a run tells the queue ---------------------------------------------

// qReset empties both halves. A run starts here: the tracks are summed, so last
// run's leftovers would be added to every reading this one takes. It also gives
// the bar back to whatever runs next, whole (see qPhase).
func (a *App) qReset() {
	// the exchange page belongs to the run, and this is the one call every run
	// makes at its start: closing the old page here means the next LLM call
	// opens a new one, named for whichever step makes it (llmlog.go)
	a.llmMu.Lock()
	a.runName, a.runSecs, a.runN = "", nil, 0
	a.llmMu.Unlock()
	a.progMu.Lock()
	a.progParts = [2]float64{}
	a.progQ = [2]qTrack{}
	a.progBase, a.progShare = 0, 0
	a.progMu.Unlock()
	a.showProg()
}

// qPhase says where in the bar the jobs that come next are drawn (base, share)
// and clears the queue for them. A single-step press never calls it; Prepare's
// ▶ is two steps back to back, each reporting its own whole bar, and the
// scaling happens here rather than in every place that moves the needle. A
// phase that has reported nothing stands where the one before it finished.
func (a *App) qPhase(base, share float64) {
	a.progMu.Lock()
	a.progBase, a.progShare = base, share
	a.progQ = [2]qTrack{}
	a.progParts = [2]float64{base / 2, base / 2}
	a.progMu.Unlock()
	a.showProg()
}

// scaled maps a track's own fraction into the phase's slice of the bar. Share 0
// means no phase was set, which is the whole bar -- that is every step but
// Prepare, and it is also the headless App the tests build. Called with
// progMu held.
func (a *App) scaled(f float64) float64 {
	if a.progShare == 0 {
		return f
	}
	return a.progBase/2 + f*a.progShare
}

// qJob says which job now owns a track, and which of the run's jobs it is --
// "describe 1/2", then "transcript 2/2". Pass of = 0 for a run that is one job,
// or whose jobs are concurrent and named rather than numbered.
func (a *App) qJob(track int, job string, phase, of int) {
	a.progMu.Lock()
	a.progQ[track] = qTrack{job: job, phase: phase, of: of}
	a.progMu.Unlock()
	a.showProg()
}

// qPush queues n more tasks of one kind. Called again whenever more work is
// found: a run that could count all of its work in advance would not need a
// queue.
func (a *App) qPush(track, n int, kind string) {
	if n <= 0 {
		return
	}
	a.progMu.Lock()
	t := &a.progQ[track]
	t.queued += n
	t.everFilled = true
	if kind != "" {
		t.kind = kind
	}
	a.progMu.Unlock()
	a.showProg()
}

// qTake picks the next task up. It is called once per task whatever becomes of
// it -- work already on disk from an earlier run is a task this run is done
// with, and a queue that skipped those would stall at the position where the
// resume started.
func (a *App) qTake(track int) {
	a.progMu.Lock()
	t := &a.progQ[track]
	t.taken++
	t.what = ""
	a.progMu.Unlock()
	a.showProg()
}

// qDone ends a track's job: it keeps the half of the bar it earned and stops
// saying anything but its name, so the line belongs to whatever is still
// running.
func (a *App) qDone(track int, f float64) {
	a.progMu.Lock()
	a.progParts[track] = a.scaled(f)
	a.progQ[track].doneJob = true
	a.progMu.Unlock()
	a.showProg()
}

// busy says whether a run is already active, and tells the status line so.
func (a *App) busy() bool {
	if a.running {
		a.setStatus("a run is already active — stop it first (⏹)")
	}
	return a.running
}

// startRun flips the app into a run: the flags, a fresh context, an empty
// queue, the controls, the log open. Every ▶ and ↻ goes through it.
func (a *App) startRun() {
	a.running = true
	a.stopFlag.Store(false)
	a.pauseFlag.Store(false)
	a.runCtx, a.runCancel = context.WithCancel(context.Background())
	a.qReset()
	a.updateRunControls()
	a.logExp.SetExpanded(true)
}

// endRun is the GUI-thread half of a run finishing.
func (a *App) endRun() {
	a.running = false
	a.updateRunControls()
	// ...and the next step of a chain, if one is under way (runchain.go).
	// Before the model unload below, which is housekeeping: the next step
	// usually wants the same models back.
	defer a.chainDone()
	// ...and the machine's half: whatever the run left loaded on the audio
	// server is memory nothing is waiting on any more (freeAudioModels). Off
	// the GUI thread, because it is a request over the network and the window
	// has to come back to life now, not when the server answers.
	go a.freeAudioModels()
}

// prog is how far this track has got and what its task is doing: the fraction
// is the track's absolute contribution; the text is two or three words, no
// filename, no count (the queue counts, the log names). An empty format falls
// back to the task's kind ("chunk 4/12").
func (a *App) prog(track int, f float64, format string, args ...any) {
	txt := fmt.Sprintf(format, args...)
	a.progMu.Lock()
	a.progParts[track] = a.scaled(f)
	a.progQ[track].what = txt
	a.progMu.Unlock()
	a.showProg()
}

// pulseUntilCounted keeps the bar moving while a model thinks. The LLM calls
// have nothing countable in them, so the bar pulses until something with real
// news -- the thumbnail's first drawing fraction -- takes the needle. What
// stops it is that fraction rather than a flag set from the goroutine: Pulse
// and SetFraction drive the same needle, so the one that lasts has to be the
// one with news -- and reading progParts under its mutex is also the only way
// to ask this question from the GUI thread without racing the runner.
func (a *App) pulseUntilCounted() {
	glib.TimeoutAdd(150, func() bool {
		if !a.running {
			return false
		}
		a.progMu.Lock()
		counted := a.progParts[trackSTT] > 0
		a.progMu.Unlock()
		if counted {
			return false
		}
		a.progress.Pulse()
		return true
	})
}

// showProg puts the two halves on the bar: the fractions summed, the lines
// joined. Callers are worker goroutines, so the widget is touched on the GUI
// thread and nowhere else.
func (a *App) showProg() {
	a.progMu.Lock()
	total := math.Max(0, math.Min(1, a.progParts[0]+a.progParts[1]))
	text, tip := progLine(a.progQ)
	a.progMu.Unlock()
	if a.progress == nil {
		return // a headless App under the tests
	}
	glib.IdleAdd(func() {
		a.progress.SetFraction(total)
		a.setStatus(text) // the bar is a fraction; the words are the status line's
		if tip != "" {
			// a run with nothing queued yet leaves the standing tooltip -- the
			// one that says how to read the bar -- rather than blanking it
			a.progress.SetTooltipText(tip)
		}
	})
}

// progLine is the bar's text and its tooltip. The text is the short one: at
// most two jobs, joined, each of them a few words. The tooltip is where the
// counting is spelled out, for the one moment anyone wants it.
func progLine(q [2]qTrack) (text, tip string) {
	var lines, tips []string
	for _, t := range q {
		if s := t.line(); s != "" {
			lines = append(lines, s)
		}
		if t.job == "" || !t.everFilled {
			continue
		}
		switch left := max(t.queued, t.taken) - t.taken; {
		case t.doneJob:
			tips = append(tips, fmt.Sprintf("%s: %d task(s), all done", t.job, t.taken))
		case left == 0:
			tips = append(tips, fmt.Sprintf("%s: task %d of %d, none waiting",
				t.job, t.taken, max(t.queued, t.taken)))
		default:
			tips = append(tips, fmt.Sprintf("%s: task %d of %d, %d waiting",
				t.job, t.taken, t.queued, left))
		}
	}
	return strings.Join(lines, "  ·  "), strings.Join(tips, "\n")
}
