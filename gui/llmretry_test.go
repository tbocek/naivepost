package main

// Which failures are worth asking again about, and how patiently.

import (
	"errors"
	"fmt"
	"io"
	"net/url"
	"strings"
	"syscall"
	"testing"
	"time"
)

// A call that died IN TRANSIT is not an answer: the server went away mid-reply
// and is coming back. On this stack that is halogen's own watchdog taking the
// container down after 180 s of engine silence for the restart policy to pick
// up -- and the restart is minutes of reading weights, so a two-second retry
// lands on a connection refused and the whole step fails for a server that is
// fine by the time anyone looks.
//
// A 400, a refusal or a reply that will not parse is the server ANSWERING, and
// answering the same way in four minutes. Those keep the one short retry.
func TestOnlyACallThatDiedInTransitIsWaitedFor(t *testing.T) {
	for _, c := range []struct {
		err  error
		gone bool
	}{
		{&url.Error{Op: "Post", URL: "http://127.0.0.1:8731/v1/chat/completions", Err: io.EOF}, true},
		{&url.Error{Op: "Post", URL: "x", Err: syscall.ECONNRESET}, true},
		{&url.Error{Op: "Post", URL: "x", Err: syscall.ECONNREFUSED}, true}, // the restart, still loading
		{io.ErrUnexpectedEOF, true}, // a stream that stopped short
		{errors.New("read tcp 127.0.0.1:60400->127.0.0.1:8731: read: connection reset by peer"), true},
		{errors.New("server answered 400 Bad Request"), false},
		{errors.New("no LLM model configured -- use the gear button"), false},
		{errors.New("stopped answering after 5m -- nothing more for 5m"), false}, // our own stall guard
		{errStopped, false},
		{nil, false},
		// a model's own words in an error must not read as a transport failure
		{errors.New(`the reply begins: "Beowulf and the theology of..."`), false},
	} {
		if got := llmGone(c.err); got != c.gone {
			t.Errorf("llmGone(%v) = %v, want %v", c.err, got, c.gone)
		}
	}
}

// How long the patience lasts, and that it ends.
func TestThePatienceClimbsPastARestartAndThenStops(t *testing.T) {
	gone := &url.Error{Op: "Post", Err: io.EOF}
	total := time.Duration(0)
	last := time.Duration(0)
	for try := 0; ; try++ {
		w, again := llmWait(gone, try)
		if !again {
			if try != len(llmBackoff) {
				t.Errorf("gave up after %d tries, want %d", try, len(llmBackoff))
			}
			break
		}
		if w < last {
			t.Errorf("try %d waits %s, less than the %s before it -- the waits climb", try, w, last)
		}
		last, total = w, total+w
		if try > 20 {
			t.Fatal("the retries never end")
		}
	}
	// enough to sit out a container restart and a cold load, and not so much
	// that a server that is really gone holds a run for an afternoon
	if total < 5*time.Minute || total > 15*time.Minute {
		t.Errorf("the whole patience is %s, want something a restart fits inside", total)
	}
	// anything else: one retry, and only after the first failure
	other := errors.New("server answered 500")
	if w, again := llmWait(other, 0); !again || w != 2*time.Second {
		t.Errorf("the first retry of an answered call is %v/%v, want 2s", w, again)
	}
	if _, again := llmWait(other, 1); again {
		t.Error("an answered call is asked a third time; it will answer the same way")
	}
}

// The wait is interruptible, or ⏹ is a button that does nothing for eight
// minutes.
func TestTheWaitEndsWhenTheRunIsStopped(t *testing.T) {
	a := &App{}
	a.stopFlag.Store(true)
	start := time.Now()
	if a.nap(50 * time.Millisecond) {
		t.Error("a stopped run went on waiting")
	}
	a.stopFlag.Store(false)
	if !a.nap(time.Millisecond) {
		t.Error("an ordinary wait reported itself as stopped")
	}
	if time.Since(start) > time.Second {
		t.Error("nap slept through the stop")
	}
	// and the retry loop itself waits through the one and not the other
	body := funcBody(t, "llm.go", `func \(a \*App\) llmChatRetryTools\(`)
	if !strings.Contains(body, "if !a.nap(wait) {") || !strings.Contains(body, "return \"\", errStopped") {
		t.Error("the retry loop sleeps without asking whether the run is still on")
	}
	if strings.Contains(body, "time.Sleep") {
		t.Error("the retry loop sleeps uninterruptibly")
	}
	if !strings.Contains(body, "the server went away mid-call") {
		t.Error(fmt.Sprint("nothing in the log says why the run is sitting still"))
	}
}
