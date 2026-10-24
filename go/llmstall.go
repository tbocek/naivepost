package main

// Liveness of a chat call is judged by SILENCE, not length: a streamed call may
// run as long as it takes and is given up when nothing arrives for llmStall.
// An unstreamed call says nothing until it says everything, so it keeps a
// whole-call ceiling (llmWhole). A live call logs sizes and the tail of what
// arrived every llmTick, so "still going" and "producing garbage" can be told
// apart.

import (
	"context"
	"fmt"
	"io"
	"strings"
	"sync"
	"time"
	"unicode/utf8"
)

const (
	// llmStall is how long a streamed call may say NOTHING before it is given
	// up on. Generous because the silence before the first token is real work:
	// a prompt with a half-hour session in it is tens of thousands of tokens
	// the server has to read before it can answer at all.
	llmStall = 5 * time.Minute
	// llmWhole is the ceiling on a call nobody is streaming. It cannot be a
	// silence rule -- there is no silence to measure -- so it is the old
	// deadline, kept for the short mechanical calls that are the only ones
	// made this way.
	llmWhole = 10 * time.Minute
	// llmTick is how often a running call reports. A minute: often enough to
	// see the answer is nonsense while it is still cheap to stop, rare enough
	// that a ten-minute call is ten lines and not a scroll.
	llmTick = time.Minute
	// llmTailLen is how much of what just arrived goes in that line -- enough
	// to recognise prose, JSON, or a stuck loop repeating itself.
	llmTailLen = 90
)

// chatWatch is one call's liveness and its running commentary. Written by the
// reader goroutine and by the guard's own ticker, so everything behind the mutex.
type chatWatch struct {
	a    *App
	step string

	mu      sync.Mutex
	t0      time.Time // when the call went out
	last    time.Time // when a byte last arrived
	said    time.Time // when the log was last told
	reply   int       // bytes of answer so far...
	think   int       // ...and of reasoning, which is not the answer
	thought string    // the last of the reasoning...
	answer  string    // ...and of the answer, which is quoted once there is one
	quiet   bool      // gave up: nothing arrived for llmStall
	stream  bool      // whether silence is a rule this call can be held to
	// how often the guard looks. A quarter of a tick, so a stall is noticed
	// within a quarter of the window rather than a whole one -- and a field
	// rather than a constant so a test can watch it happen in a moment
	// instead of in a minute.
	poll time.Duration
}

func (a *App) watchChat(step string, streamed bool) *chatWatch {
	now := time.Now()
	return &chatWatch{a: a, step: step, t0: now, last: now, said: now, stream: streamed,
		poll: llmTick / 4}
}

// alive marks that something arrived. Every byte off the wire counts, not just
// the ones that turn into an answer: a keep-alive and a reasoning delta are
// both proof the server is still there, which is the only thing being asked.
func (w *chatWatch) alive() {
	w.mu.Lock()
	w.last = time.Now()
	w.mu.Unlock()
}

// wrote records what those bytes turned out to be. think is the reasoning the
// server sends alongside the answer -- counted and shown, never returned.
func (w *chatWatch) wrote(s string, think bool) {
	w.mu.Lock()
	defer w.mu.Unlock()
	w.last = time.Now()
	// a rolling window over the text, not the chunk that just landed. A server
	// that streams a token per event was leaving a one-word quote in the
	// heartbeat -- "At", "," -- which says nothing about whether the model is
	// converging or going round in circles, the one thing the quote is for.
	if think {
		w.think += len(s)
		w.thought = keepTail(w.thought + s)
	} else {
		w.reply += len(s)
		w.answer = keepTail(w.answer + s)
	}
}

// keepTail drops all but the end of a window. Fixed, because a call that
// thinks for ten minutes writes more than anything wants to hold to show
// ninety characters of it.
func keepTail(s string) string {
	if n := len(s); n > tailKeep {
		return s[n-tailKeep:]
	}
	return s
}

// wrap ties the body to the watch, so ANY byte read from it counts as life --
// including the ones between the events, which the SSE parser above never sees.
func (w *chatWatch) wrap(r io.Reader) io.Reader { return &aliveReader{r: r, w: w} }

type aliveReader struct {
	r io.Reader
	w *chatWatch
}

func (a *aliveReader) Read(p []byte) (int, error) {
	n, err := a.r.Read(p)
	if n > 0 {
		a.w.alive()
	}
	return n, err
}

// guard is the call's timekeeper: it says what is arriving, and it cancels the
// request when nothing is. It runs until the call returns and the caller stops
// it -- there is no other exit, because a call that ends normally ends by the
// reader returning, not by anything this can see.
func (w *chatWatch) guard(cancel context.CancelFunc) (stop func()) {
	done := make(chan struct{})
	go func() {
		t := time.NewTicker(w.poll)
		defer t.Stop()
		for {
			select {
			case <-done:
				return
			case <-t.C:
				line, dead := w.check(time.Now())
				if line != "" {
					w.a.logfIdle("%s", line)
				}
				if dead {
					cancel()
					return
				}
			}
		}
	}()
	return func() { close(done) }
}

// check is one tick, taking now rather than reading the clock so that what it
// decides can be asked without waiting for it: the line to log if it is time
// to say something, and whether this call has been quiet too long to wait for.
func (w *chatWatch) check(now time.Time) (string, bool) {
	w.mu.Lock()
	defer w.mu.Unlock()
	quiet, since := now.Sub(w.last), now.Sub(w.t0)
	if w.stream && quiet >= llmStall {
		w.quiet = true
		return fmt.Sprintf(">>> %s: nothing for %s — giving up", w.step, durOf(quiet)), true
	}
	if now.Sub(w.said) < llmTick {
		return "", false
	}
	w.said = now
	// What it is doing, and enough of what it is writing to see that it has
	// gone wrong. The answer is what gets quoted, because the answer is what
	// is being judged and the reasoning is the model talking to itself -- but
	// until there is an answer the reasoning is quoted instead, because a call
	// six minutes in with nothing to show is exactly when what it is thinking
	// is the only thing worth knowing.
	tail := logTail(w.answer)
	if tail == "" {
		tail = logTail(w.thought)
	}
	switch {
	case w.reply == 0 && w.think == 0:
		return fmt.Sprintf(">>> %s: nothing yet, %s in", w.step, durOf(since)), false
	case tail == "":
		return fmt.Sprintf(">>> %s: %s thinking, %s reply, %s in",
			w.step, sizeOf(w.think), sizeOf(w.reply), durOf(since)), false
	}
	return fmt.Sprintf(">>> %s: %s thinking, %s reply, %s in — %q",
		w.step, sizeOf(w.think), sizeOf(w.reply), durOf(since), tail), false
}

// blame names the cancellation. A guard that gave up, a stopped run and a
// server that dropped the connection all reach the caller as "context
// canceled", which tells whoever reads the log nothing at all; where the guard
// was the one who cancelled, the error says so and says for how long.
func (w *chatWatch) blame(err error) error {
	if err == nil {
		return nil
	}
	w.mu.Lock()
	quiet := w.quiet
	silence := w.last.Sub(w.t0)
	w.mu.Unlock()
	if !quiet {
		return err
	}
	if silence <= 0 {
		return fmt.Errorf("nothing arrived in %s", durOf(llmStall))
	}
	return fmt.Errorf("stopped answering after %s -- nothing more for %s",
		durOf(silence), durOf(llmStall))
}

// tailKeep is how much text a watch holds on to. Comfortably more than a tail
// so that collapsing the whitespace inside it still leaves llmTailLen to show.
const tailKeep = 4 * llmTailLen

// logTail is the end of what arrived, on one line. Newlines and runs of spaces
// collapse because a log line is a line -- and the tail is taken from the END
// because that is where a stuck model repeats itself.
func logTail(s string) string {
	s = strings.Join(strings.Fields(s), " ")
	if s == "" {
		return ""
	}
	if len(s) > llmTailLen {
		s = s[len(s)-llmTailLen:]
		// the cut counted bytes, and a model writing dashes and quotes puts
		// multi-byte runes across it: step forward to the start of one, or the
		// quote opens on a replacement character
		for len(s) > 0 && !utf8.RuneStart(s[0]) {
			s = s[1:]
		}
		s = "…" + s
	}
	return s
}
