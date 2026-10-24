package main

import (
	"math"
	"os"
	"strings"
	"testing"
)

// The queue is what one press of ▶ turned into, and the bar is the one place a
// user sees it. These pin the sentence it writes, and the two things the
// arithmetic must survive: a queue that grows while it is being worked through,
// and a run that starts after another one finished.

func TestTheBarSaysTheJobTheTaskAndHowFarDownTheQueue(t *testing.T) {
	for _, c := range []struct {
		name string
		tr   qTrack
		want string
	}{
		{"nothing at all", qTrack{}, ""},
		{"a job that has not said what it is doing", qTrack{job: "publish"}, "publish"},
		{"a job with words but no queue", qTrack{job: "publish", what: "thinking"}, "publish: thinking"},
		{"one of the run's two jobs", qTrack{job: "describe", phase: 1, of: 2, what: "watching"},
			"describe 1/2: watching"},
		{"a queue speaking for itself", qTrack{job: "transcript", phase: 2, of: 2,
			kind: "block", queued: 7, taken: 3, everFilled: true}, "transcript 2/2: block 3/7"},
		{"words win over the kind", qTrack{job: "speech", kind: "recording", what: "recognising",
			queued: 3, taken: 2, everFilled: true}, "speech: recognising 2/3"},
		{"a filled queue nobody has taken from yet", qTrack{job: "frames", kind: "video",
			queued: 4, everFilled: true}, "frames: video"},
		{"a finished job keeps only its name", qTrack{job: "frames", what: "extracting",
			queued: 4, taken: 4, everFilled: true, doneJob: true}, "frames done"},
		{"a phase of one is not a phase", qTrack{job: "render", phase: 1, of: 1, what: "joining"},
			"render: joining"},
	} {
		if got := c.tr.line(); got != c.want {
			t.Errorf("%s: got %q, want %q", c.name, got, c.want)
		}
	}
}

// The filename is the whole reason this exists: the log says which file, the bar
// says how far along. If one creeps back in, the line stops being readable at a
// glance and starts being the log line again.
func TestTheBarHasNoRoomForAFilename(t *testing.T) {
	tr := qTrack{job: "describe", phase: 1, of: 2, kind: "chunk", queued: 12, taken: 4, everFilled: true}
	if got, want := tr.line(), "describe 1/2: chunk 4/12"; got != want {
		t.Fatalf("got %q, want %q", got, want)
	}
	if len(tr.line()) > 40 {
		t.Errorf("the line is %d characters -- that is a log line, not a bar", len(tr.line()))
	}
}

// Most of the work cannot be counted before it is opened, so the queue is filled
// as it is found. The head must never pass the end of a queue it is still
// filling -- "5/4" is the reading that tells a user the bar is lying.
func TestTheQueueGrowsWhileItIsBeingWorkedThrough(t *testing.T) {
	a := &App{}
	a.qJob(trackSTT, "speech", 0, 0)
	a.qPush(trackSTT, 2, "recording")
	a.qTake(trackSTT)
	a.qTake(trackSTT)
	if got, want := a.progQ[trackSTT].line(), "speech: recording 2/2"; got != want {
		t.Fatalf("got %q, want %q", got, want)
	}
	a.qPush(trackSTT, 3, "recording") // three more turned up mid-run
	if got, want := a.progQ[trackSTT].line(), "speech: recording 2/5"; got != want {
		t.Fatalf("after the queue grew: got %q, want %q", got, want)
	}
	// and if a run somehow takes more than it queued, the total follows the head
	// rather than reading 6/5
	for i := 0; i < 4; i++ {
		a.qTake(trackSTT)
	}
	if got, want := a.progQ[trackSTT].line(), "speech: recording 6/6"; got != want {
		t.Fatalf("past the end of the queue: got %q, want %q", got, want)
	}
}

// A task's own words are the head's words. Picking the next task up has to drop
// them, or the bar describes the task before this one.
func TestTakingTheNextTaskDropsTheLastOnesWords(t *testing.T) {
	a := &App{}
	a.qJob(trackFix, "transcript", 2, 2)
	a.qPush(trackFix, 3, "block")
	a.qTake(trackFix)
	a.prog(trackFix, 0.1, "fixing")
	if got, want := a.progQ[trackFix].line(), "transcript 2/2: fixing 1/3"; got != want {
		t.Fatalf("got %q, want %q", got, want)
	}
	a.qTake(trackFix)
	if got, want := a.progQ[trackFix].line(), "transcript 2/2: block 2/3"; got != want {
		t.Fatalf("got %q, want %q", got, want)
	}
}

// Prepare runs both halves at once, and the bar has to hold both without either
// of them claiming all of it.
func TestBothHalvesShareTheOneBar(t *testing.T) {
	a := &App{}
	a.qJob(trackSTT, "speech", 0, 0)
	a.qPush(trackSTT, 3, "recording")
	a.qTake(trackSTT)
	a.prog(trackSTT, 0.1, "recognising")
	a.qJob(trackFrames, "frames", 0, 0)
	a.qPush(trackFrames, 2, "video")
	a.qTake(trackFrames)
	a.prog(trackFrames, 0.2, "extracting")

	text, tip := progLine(a.progQ)
	if want := "speech: recognising 1/3  ·  frames: extracting 1/2"; text != want {
		t.Errorf("bar: got %q, want %q", text, want)
	}
	// the counting lives in the tooltip, where it is out of the way until it is
	// wanted -- this is the "how many tasks are still in the queue" answer
	for _, want := range []string{
		"speech: task 1 of 3, 2 waiting",
		"frames: task 1 of 2, 1 waiting",
	} {
		if !strings.Contains(tip, want) {
			t.Errorf("tooltip %q does not say %q", tip, want)
		}
	}
	if got := a.progParts[trackSTT] + a.progParts[trackFrames]; math.Abs(got-0.3) > 1e-9 {
		t.Errorf("the halves sum to %v, not 0.3", got)
	}
}

// One half finishing does not end the run: it keeps the fraction it earned and
// gets out of the way of the half still going.
func TestAFinishedHalfKeepsItsFractionAndStopsTalking(t *testing.T) {
	a := &App{}
	a.qJob(trackFrames, "frames", 0, 0)
	a.qPush(trackFrames, 2, "video")
	a.qTake(trackFrames)
	a.qTake(trackFrames)
	a.qDone(trackFrames, 0.5)
	a.qJob(trackSTT, "speech", 0, 0)
	a.prog(trackSTT, 0.2, "recognising")

	text, tip := progLine(a.progQ)
	if want := "speech: recognising  ·  frames done"; text != want {
		t.Errorf("bar: got %q, want %q", text, want)
	}
	if !strings.Contains(tip, "frames: 2 task(s), all done") {
		t.Errorf("tooltip %q does not say the frames queue emptied", tip)
	}
	if a.progParts[trackFrames] != 0.5 {
		t.Errorf("the finished half gave back its %v of the bar", a.progParts[trackFrames])
	}
}

// The tracks are summed, so a run that did not clear them would start wherever
// the last one finished -- and would show the last run's jobs while it did.
func TestAResetEmptiesBothHalves(t *testing.T) {
	a := &App{}
	a.qJob(trackSTT, "describe", 1, 2)
	a.qPush(trackSTT, 5, "chunk")
	a.qTake(trackSTT)
	a.qDone(trackFrames, 0.5)

	a.qReset()
	if a.progParts != [2]float64{} {
		t.Errorf("the fractions survived the reset: %v", a.progParts)
	}
	if text, tip := progLine(a.progQ); text != "" || tip != "" {
		t.Errorf("the last run is still on the bar: %q / %q", text, tip)
	}
}

// A run whose work was all done by an earlier one must still walk its queue:
// skipping the cached tasks silently would leave the head stuck where the resume
// started, which reads as a hung run.
func TestTheStepsQueueTheWorkTheyDo(t *testing.T) {
	for _, c := range []struct {
		file, want, why string
	}{
		{"pipeline.go", `a.qPush(trackSTT, len(inputs), "recording")`,
			"Prepare queues the recordings it has to listen to"},
		{"pipeline.go", `a.qPush(trackFrames, len(videos), "video")`,
			"Prepare queues the videos it has to open"},
		{"prep.go", `a.qJob(trackDescribe, "describe", 1, 2)`,
			"describe is not named as the first of the step's two jobs"},
		{"prep.go", `a.qJob(trackFix, "transcript", 2, 2)`,
			"transcript is not named as the second of the step's two jobs"},
		{"describe.go", `a.qPush(trackDescribe, total, "chunk")`,
			"the chunks to describe are not queued"},
		{"transcript.go", `a.qPush(trackFix, total, "block")`,
			"the blocks to fix are not queued"},
		{"narrate.go", `a.qPush(trackSTT, len(speak), "line")`,
			"the lines to speak are not queued"},
		{"produce.go", `a.qPush(trackSTT, len(clips), "clip")`,
			"the clips to encode are not queued"},
	} {
		b, err := os.ReadFile(c.file)
		if err != nil {
			t.Fatal(err)
		}
		if !strings.Contains(string(b), c.want) {
			t.Errorf("%s: %s (looked for %s)", c.file, c.why, c.want)
		}
	}

	// the resume case: describe takes its task BEFORE it decides the chunk is
	// already on disk
	b, err := os.ReadFile("describe.go")
	if err != nil {
		t.Fatal(err)
	}
	take := strings.Index(string(b), "a.qTake(trackDescribe)")
	skip := strings.Index(string(b), "if done[key] {")
	if take < 0 || skip < 0 {
		t.Fatal("the describe loop no longer reads the way this test assumes")
	}
	if take > skip {
		t.Error("a chunk that was already described never reaches the queue, so the bar stalls on a resumed session")
	}
}

// Nothing may talk to the bar in the old vocabulary. The per-track text the
// steps used to keep is gone, and a step that kept its own would be wiped by the
// next queue call anyway -- showProg sets the text from an idle, so a direct
// write racing it loses. Ending a run by hand ("done", "failed -- see log") is
// still fine: nothing is queued behind it.
func TestNoStepKeepsItsOwnBarText(t *testing.T) {
	files, err := os.ReadDir(".")
	if err != nil {
		t.Fatal(err)
	}
	for _, f := range files {
		n := f.Name()
		if !strings.HasSuffix(n, ".go") || strings.HasSuffix(n, "_test.go") {
			continue
		}
		b, err := os.ReadFile(n)
		if err != nil {
			t.Fatal(err)
		}
		if strings.Contains(string(b), "progTexts") {
			t.Errorf("%s still keeps its own bar text -- the queue writes the line now", n)
		}
	}
}
