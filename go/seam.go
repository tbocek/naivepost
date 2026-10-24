package main

import "context"

// The seam between the pipeline and the pages.
//
// Twenty-odd files in this package run the pipeline -- transcribe, align,
// describe, fix, translate, the LLM client, the image server -- and none of
// them touches a widget. They are still methods on *App, in the same package
// as the buttons, so nothing said what they were allowed to take from it.
// This is that list.
//
// runner is everything a pipeline file may ask the application for. *App
// satisfies it (the assertion below is checked by the compiler), and the fence
// test (seam_test.go) reads the pipeline files and refuses any use of *App
// that is not here, not a field named beside it, and not another pipeline
// file's own method. Adding to the seam is allowed; it is meant to be a
// deliberate line in a diff rather than something a helper call grew by
// accident.
//
// The shape is the interface a separate package would take, if the split is
// ever made: what is here is what would have to cross it.

type runner interface {
	// the log, from the runner's goroutine and from the GUI thread
	logf(format string, args ...any)
	logfIdle(format string, args ...any)

	// the progress bar and its work queue (runqueue.go)
	prog(track int, f float64, format string, args ...any)
	qPush(track, n int, kind string)
	qTake(track int)
	qDone(track int, f float64)

	// pause and stop, between subprocesses; the wait that stop can end
	checkpoint() error

	// the settings file, re-read rather than cached so a change takes effect
	// on the next step (setup.go)
	readConf() appConf

	// where things are
	inputsDir() string
	transcriptDir() string
	describeDir() string
	framesDir(base string) string
	transcriptPath(base string) string
	thumbFile() string

	// the session as the runner may read it: values snapped on the GUI thread
	// or cached under a mutex, never a widget (main.go, prompts.go, prep.go)
	snapSources() (vids, auds []string)
	snappedSources() (vids, auds []string)
	sessionRows() []tsvRow
	sessionCtx() string
	asrLanguage() string
	videoStyleName() string
	narratorMic() string
	voiceID() string
	produceCut() cutFile
	ttsWav(e narrEntry) string

	// the wordings (prompts.go, syscontext.go)
	prompt(key string) string
	ctxBlockFor(key string) string

	// subprocesses and their sound
	runCmd(name string, args ...string) error
	quietSpots(wav string) []span

	// the record of a chat, written for the log page (llmlog.go)
	recordChatStart(step string, thinking bool, msgs []map[string]any) *chatRec
	// one chat request on the wire at a time, across every step (llmgate.go)
	takeLLM(ctx context.Context, step string) error
	giveLLM()
}

var _ runner = (*App)(nil)

// runnerFields is the state a pipeline file reads straight off *App rather
// than through a method -- an interface cannot carry fields, so these are the
// seam's other half, named for the fence test. Each is either written once
// before a run starts (outDir, runCtx, stopFlag) or is a runner's own scratch
// that no page reads (alignPick, audioNoted).
var runnerFields = []string{
	"outDir",     // where the project writes
	"runCtx",     // cancelled by ⏹ (runqueue.go)
	"stopFlag",   // ...and the flag that says it was
	"ctlMu",      // the subprocess registry ⏹ kills through
	"curCmds",    //
	"alignPick",  // which aligner answered last (align.go)
	"audioNoted", // the audio server already reported (audiocpp.go)
	"narrOff",    // this video has no narration
	"ttsModel",   // the model id the audio server serves for speech
}

// pipelineFiles is the pipeline: the files that run it and touch no widget.
// A file is on this list to be fenced, and a new one goes here when it is
// written as logic -- which the fence then holds it to.
var pipelineFiles = []string{
	"align.go", "audiocpp.go", "describe.go", "llm.go", "llmstall.go", "llmcache.go",
	"retake.go", "retake_edge.go", "textedit.go", "transcript.go", "translate.go",
	"subwords.go", "sdcpp.go", "websearch.go", "produce_embed.go", "produce_stamp.go",
	"syscontext.go",
}
