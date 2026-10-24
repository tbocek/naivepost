package main

// Drives the step-3 pipeline directly, without the GUI, against the real
// project data. Doubles as diagnosis (a crash here explains a dead button)
// and as the actual run (its outputs are the real understand/transcript/ artifacts).
//
// It sends every ASR block to the configured LLM, so it is off by default --
// a test suite that quietly bills a remote server is not a test suite:
//
//	NAIVEPOST_LIVE=1 go test -run TestCutPipelineLive -v -timeout 60m

import (
	"os"
	"os/exec"
	"path/filepath"
	"testing"
)

func TestCutPipelineLive(t *testing.T) {
	if os.Getenv("NAIVEPOST_LIVE") == "" {
		t.Skip("set NAIVEPOST_LIVE=1 to run the fixer against the configured LLM")
	}
	root := "/home/draft/git/aitools/naivepost"
	a := &App{
		root:    root,
		vidDir:  filepath.Join(root, "input_video"),
		audDir:  filepath.Join(root, "input_audio"),
		outDir:  filepath.Join(root, "out", "test"),
		curCmds: map[*exec.Cmd]bool{},
	}
	videos := []string{
		filepath.Join(root, "input_video", "com.AnotherAxiom.GorillaTag-20260808-195900-0.mp4"),
		filepath.Join(root, "input_video", "com.AnotherAxiom.GorillaTag-20260808-200145-0.mp4"),
	}
	audios := []string{filepath.Join(root, "input_audio", "jan2-2026-08-08_19-55-15.flac")}
	if err := a.fixTranscripts(videos, audios, 1); err != nil { // 1 = the whole progress bar
		t.Fatalf("step3: %v", err)
	}
}
