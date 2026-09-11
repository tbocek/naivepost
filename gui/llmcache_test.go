package main

// The answers kept under the questions.

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// The key is the whole request. That is the only thing that makes a cache like
// this safe: change the wording, the frames, the interval or a word of the user
// context and the question is a different question, so the answer is asked for
// again rather than served from a run that was answering something else.
func TestTheCacheIsKeyedOnEverythingThatDecidesTheAnswer(t *testing.T) {
	sys := "you describe footage"
	body := []any{map[string]any{"type": "text", "text": "STATE so far: nothing yet"},
		map[string]any{"type": "image_url", "image_url": map[string]any{"url": "data:image/jpeg;base64,AAAA"}}}

	base := askKey(sys, body)
	if base == "" {
		t.Fatal("a request that can be sent produced no key")
	}
	if askKey(sys, body) != base {
		t.Error("the same request keyed differently twice")
	}
	for _, c := range []struct {
		what string
		key  string
	}{
		{"a reworded prompt", askKey(sys+" carefully", body)},
		{"a different picture", askKey(sys, []any{body[0],
			map[string]any{"type": "image_url", "image_url": map[string]any{"url": "data:image/jpeg;base64,BBBB"}}})},
		{"a different state", askKey(sys, []any{
			map[string]any{"type": "text", "text": "STATE so far: lap 3"}, body[1]})},
		{"one fewer frame", askKey(sys, []any{body[0]})},
	} {
		if c.key == base {
			t.Errorf("%s keyed the same as the original request", c.what)
		}
	}
}

// A hit is the same words back; a miss is a miss. Neither is allowed to be a
// crash, and a project with nowhere to write is a step that runs at its
// ordinary speed.
func TestAnAnswerComesBackUnderItsOwnKey(t *testing.T) {
	a := &App{outDir: t.TempDir()}
	k := askKey("sys", []any{"frames"})

	if _, ok := a.cachedReply("describe", k); ok {
		t.Error("an empty cache answered")
	}
	a.keepReply("describe", k, "EVENT: calm\nSTATE: reading")
	got, ok := a.cachedReply("describe", k)
	if !ok || got != "EVENT: calm\nSTATE: reading" {
		t.Errorf("the answer came back as %q (%v)", got, ok)
	}
	if _, ok := a.cachedReply("describe", askKey("other", []any{"frames"})); ok {
		t.Error("a different question was served the same answer")
	}
	// it goes under the project's cache/, which is the folder for things that
	// can be deleted and made again
	if _, err := os.Stat(filepath.Join(a.outDir, "cache", "llm", "describe", k)); err != nil {
		t.Errorf("the answer was not filed under cache/llm: %v", err)
	}
	// an empty answer is not worth keeping, and a project with no folder at all
	// simply has no cache
	a.keepReply("describe", k+"x", "")
	if _, ok := a.cachedReply("describe", k+"x"); ok {
		t.Error("an empty answer was filed")
	}
	none := &App{}
	none.keepReply("describe", k, "anything")
	if _, ok := none.cachedReply("describe", k); ok {
		t.Error("a session with no project folder cached something somewhere")
	}
}

// Where it sits in the run: the cache is asked before the model and filled
// after it, and a stop is still a stop.
func TestDescribeAsksTheCacheBeforeTheModel(t *testing.T) {
	body := funcBody(t, "describe.go", `func \(a \*App\) describeVideo\(`)
	if body == "" {
		body = readSrc(t, "describe.go")
	}
	ask := strings.Index(body, `a.cachedReply("describe", ask)`)
	call := strings.Index(body, "a.llmChat"+`Retry("describe"`) // split: the live guard must not read a pin as a call
	keep := strings.Index(body, `a.keepReply("describe", ask, reply)`)
	if ask < 0 || call < 0 || keep < 0 || ask > call || call > keep {
		t.Errorf("the cache is not asked before the model and filled after it (%d, %d, %d)", ask, call, keep)
	}
	// the key is the request, not the frame paths: two runs with the same
	// pictures at different paths are the same question, and the same path
	// with different pixels is not
	if !strings.Contains(body, "askKey(sys, content)") {
		t.Error("the cache is keyed on something other than the request itself")
	}
}

// ...and so do the other passes whose answer cannot legitimately differ: the
// text edit, the retake pool and the subtitle translation.
//
// They are not buttons anybody presses for a second opinion -- the first two
// are the last thing Prepare does, after every stage above them has resumed
// from disk, and the third is a side effect of rendering. Uncached, they were
// what a re-run on unchanged material still paid for in full: one call over
// every word of the session, three more over every spoken line, and three
// minutes of a ten-minute render translating subtitles that had not moved.
//
// (Cut, Narrate and Suggest again are deliberately NOT here: pressing those
// again IS how a different answer is asked for.)
func TestTheMechanicalPassesAskTheCacheBeforeTheModel(t *testing.T) {
	for _, c := range []struct{ file, fn, step string }{
		{"textedit.go", `func \(a \*App\) findTextEdit\(`, "textedit"},
		{"retake.go", `func \(a \*App\) findRetakes\(`, "retake"},
		{"translate.go", `func \(a \*App\) translateCues\(`, "translate"},
	} {
		body := funcBody(t, c.file, c.fn)
		ask := strings.Index(body, `a.cachedReply("`+c.step+`", ask)`)
		call := strings.Index(body, "a.llmChat"+`Retry("`+c.step+`"`)
		// what is FILED is not always the reply as it arrived -- a track
		// completed by a second ask is filed whole (numberedText) -- so this
		// asks only that the answer is filed under the key it was asked with
		keep := strings.Index(body, `a.keepReply("`+c.step+`", ask, `)
		if ask < 0 || call < 0 || keep < 0 || ask > call {
			t.Errorf("%s: the cache is not asked before the model (%d, %d, %d)",
				c.step, ask, call, keep)
		}
	}
	// The retake pool asks the SAME question retakeRuns times on purpose and
	// pools the answers, which drift. The run number is therefore part of its
	// key: one slot per run replays the pool, where a key without it would
	// serve run 1's answer three times and quietly turn a pool into one run.
	rt := funcBody(t, "retake.go", `func \(a \*App\) findRetakes\(`)
	if !strings.Contains(rt, "askKey(system, user, run)") {
		t.Error("the retake pool is keyed without its run number, which collapses it to one answer")
	}
	// and nothing is filed before it has been read back: an answer that was
	// set aside or came back short must not be served from the file for ever
	if i, j := strings.Index(rt, "its answer is set aside"), strings.Index(rt, "a.keepReply("); i < 0 || j < 0 || i > j {
		t.Error("a retake answer is cached before it is known to parse")
	}
	tr := funcBody(t, "translate.go", `func \(a \*App\) translateCues\(`)
	if i, j := strings.Index(tr, "came back missing"), strings.Index(tr, "a.keepReply("); i < 0 || j < 0 || i > j {
		t.Error("a translation is cached before it is known to cover every line")
	}
}
