package main

// A test that reaches the LLM or the TTS server has to say so and be off by
// default. This one enforces that, because the failure it prevents is silent:
// TestCutPipeline drove the real fixer for months, so every plain `go test
// ./...` posted the whole transcript to whatever server llm.conf named, and
// nothing in the output said a request had left the machine.

import (
	"os"
	"path/filepath"
	"regexp"
	"strings"
	"testing"
)

// The calls that leave the machine. Everything else in the tool is ffmpeg,
// files and arithmetic, and none of that needs a guard.
var remoteCalls = []string{
	"a.ingest(", "a.describeAll(", "a.fixTranscripts(", // whole steps: STT, describe, fix
	"a.understand(",                 // the two of them back to back
	"a.llmChat(", "a.llmChatRetry(", // the chat client itself
	"a.speak(", // TTS
	"testLLM(", "testVision(", "testTTS(", "testSD(",
}

var liveGuards = []string{"NAIVEPOST_LIVE", "NAIVEPOST_TTS_LIVE"}

// the declared name of a function or a method, from the text after "func "
var funcName = regexp.MustCompile(`^(?:\([^)]*\)\s*)?(\w+)\(`)

func TestRemoteTestsAreOptIn(t *testing.T) {
	files, err := filepath.Glob("*_test.go")
	if err != nil || len(files) == 0 {
		t.Fatalf("no test files found: %v", err)
	}
	// splits a file into whole top-level functions, so a guard in one test
	// cannot be credited to the next one down
	split := regexp.MustCompile(`(?m)^func `)
	for _, f := range files {
		if f == "live_guard_test.go" {
			continue // the call names above are not calls
		}
		src, err := os.ReadFile(f)
		if err != nil {
			t.Fatal(err)
		}
		funcs := split.Split(string(src), -1)
		// Helpers in this file that stand a fake server up. A test that calls one
		// is as local as a test that inlines httptest.NewServer -- and once a
		// file has several such tests, factoring the fake out is the obvious
		// thing to do, so a guard that only recognised the inline form would
		// punish the tidier version of the same test.
		var fakes []string
		for _, fn := range funcs {
			if !strings.Contains(fn, "httptest.NewServer") {
				continue
			}
			// the name, whether it is a plain function or a method on the fake
			if m := funcName.FindStringSubmatch(fn); m != nil && !strings.HasPrefix(m[1], "Test") {
				fakes = append(fakes, m[1]+"(")
			}
		}
		for _, fn := range funcs {
			name := fn
			if i := strings.IndexAny(name, "(\n"); i >= 0 {
				name = name[:i]
			}
			// a test that stands up its own httptest server is calling
			// 127.0.0.1 with a URL it made itself -- local by construction
			if strings.Contains(fn, "httptest.NewServer") {
				continue
			}
			local := false
			for _, f := range fakes {
				local = local || strings.Contains(fn, f)
			}
			if local {
				continue
			}
			for _, call := range remoteCalls {
				if !strings.Contains(fn, call) {
					continue
				}
				guarded := false
				for _, g := range liveGuards {
					guarded = guarded || strings.Contains(fn, g)
				}
				if !guarded {
					t.Errorf("%s: %s calls %s with no %s skip -- "+
						"a plain `go test ./...` would hit the server",
						f, name, call, strings.Join(liveGuards, "/"))
				}
			}
		}
	}
}
