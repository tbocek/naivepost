package main

// The rule that decides a chat call has died, and the running commentary that
// says what it was doing while it lived.

import (
	"context"
	"fmt"
	"strings"
	"testing"
	"time"
)

// The old rule was a deadline on the whole call, which cannot tell a hung
// server from a model writing steadily for eleven minutes. The rule is silence
// instead: a call that is producing anything is left alone however long it
// takes, and one that has produced nothing at all for llmStall is given up on.
func TestACallDiesOfSilenceAndNotOfLength(t *testing.T) {
	a := &App{root: t.TempDir()}
	w := a.watchChat("audit", true)
	t0 := w.t0

	// an hour of work, with something arriving throughout: still alive
	for at := time.Second; at < time.Hour; at += llmTick {
		w.wrote("thinking about it", true)
		w.last = t0.Add(at)
		if _, dead := w.check(t0.Add(at)); dead {
			t.Fatalf("gave up %s in on a call that was still producing", durOf(at))
		}
	}
	// ...and then it stops saying anything
	last := w.last
	if _, dead := w.check(last.Add(llmStall - time.Second)); dead {
		t.Error("gave up before the silence was long enough")
	}
	if _, dead := w.check(last.Add(llmStall)); !dead {
		t.Errorf("still waiting after %s of nothing at all", durOf(llmStall))
	}
}

// Only a streamed call has a silence to measure. An unstreamed one says nothing
// until it says everything, so its quiet proves nothing and it keeps a ceiling
// on the whole call instead.
func TestOnlyAStreamedCallCanBeJudgedBySilence(t *testing.T) {
	a := &App{root: t.TempDir()}
	w := a.watchChat("describe", false)
	if _, dead := w.check(w.t0.Add(3 * llmStall)); dead {
		t.Error("gave up on an unstreamed call for being quiet, which is all it can be")
	}
	src := funcBody(t, "llm.go", `func \(a \*App\) llmChatPost\(`)
	for _, pin := range []string{
		"a.watchChat(step, onText != nil)",
		"if onText == nil {\n\t\tclient.Timeout = llmWhole",
	} {
		if !strings.Contains(src, pin) {
			t.Errorf("the wire call no longer has %q", pin)
		}
	}
	if strings.Contains(src, "Timeout: 10 * time.Minute") {
		t.Error("the whole-call deadline is back on every call, streamed ones included")
	}
	if llmWhole <= llmStall {
		t.Errorf("the unstreamed ceiling %s is no longer than the silence window %s",
			durOf(llmWhole), durOf(llmStall))
	}
}

// A call that is alive says so, and says enough of what it is writing that
// garbage can be recognised while stopping it is still cheap. Reasoning is
// sized but not quoted whole -- it is the model talking to itself.
func TestARunningCallSaysWhatIsArriving(t *testing.T) {
	a := &App{root: t.TempDir()}
	w := a.watchChat("suggest", true)

	line, _ := w.check(w.t0.Add(llmTick))
	if !strings.Contains(line, "nothing yet") || !strings.Contains(line, "suggest") {
		t.Errorf("a call with no answer yet reported %q", line)
	}
	if again, _ := w.check(w.t0.Add(llmTick + time.Second)); again != "" {
		t.Errorf("reported twice inside one tick: %q", again)
	}

	w.wrote(strings.Repeat("x", 4096), true)
	w.wrote(`{"segments":[{"start":1,"end":2}]}`, false)
	line, _ = w.check(w.t0.Add(2 * llmTick))
	// quoted, so the JSON arrives escaped -- what matters is that the last of
	// it is there to be read
	for _, want := range []string{"4.0 kB thinking", "34 B reply", `\"end\":2}]}`} {
		if !strings.Contains(line, want) {
			t.Errorf("the line %q does not say %q", line, want)
		}
	}
	if strings.Contains(line, "xxxxxxxx") {
		t.Error("the reasoning is being quoted into the log, not counted")
	}
}

// The tail is taken from the end and put on one line: a stuck model repeats
// itself at the end of what it has written, and a log line is a line.
func TestTheTailIsOneLineFromTheEndOfIt(t *testing.T) {
	if got := logTail("one\ntwo   three\n"); got != "one two three" {
		t.Errorf("logTail collapsed to %q", got)
	}
	if got := logTail("   \n "); got != "" {
		t.Errorf("whitespace made a tail of %q", got)
	}
	long := logTail(strings.Repeat("ab", 400) + "END")
	if !strings.HasSuffix(long, "END") {
		t.Errorf("a long tail kept the wrong end: %q", long)
	}
	if len([]rune(long)) > llmTailLen+1 {
		t.Errorf("the tail is %d runes, want at most %d and the ellipsis", len([]rune(long)), llmTailLen)
	}
}

// A server that streams a token per event fed the heartbeat one word at a
// time, and the log quoted the word: >>> narrate: 59.2 kB thinking, 0 B reply,
// 5m45s in — "At". Five of those in a row say nothing about whether the model
// is converging or going round in circles, which is the whole reason the quote
// is in the line.
func TestTheQuoteIsASentenceEvenWhenTheServerSendsAWordAtATime(t *testing.T) {
	a := &App{root: t.TempDir()}
	w := a.watchChat("narrate", true)
	for _, tok := range strings.Fields("So the cut runs long and the last beat is weak . At") {
		w.wrote(tok+" ", true)
	}
	line, _ := w.check(w.t0.Add(llmTick))
	if !strings.Contains(line, "the last beat is weak") {
		t.Errorf("the heartbeat quoted %q, want a readable run of what arrived", line)
	}
}

// ...and only the recent end of it: a call that thinks for six minutes writes
// far more than a log line can hold, and it is the newest words that say what
// it is doing now.
func TestTheQuoteFollowsTheModelRatherThanKeepingItsFirstWords(t *testing.T) {
	a := &App{root: t.TempDir()}
	w := a.watchChat("narrate", true)
	w.wrote(strings.Repeat("opening ", 200), true)
	w.wrote("and now the ending", true)
	line, _ := w.check(w.t0.Add(llmTick))
	if !strings.Contains(line, "and now the ending") {
		t.Errorf("the heartbeat quoted %q, want the newest words", line)
	}
	if strings.Count(line, "opening") > 12 {
		t.Errorf("the heartbeat grew with the reasoning: %q", line)
	}
}

// The window is fixed: a call that thinks for ten minutes sends more text than
// anything wants to hold, and appending all of it to one string to show ninety
// characters of it is a megabyte of copying for a log line. Fixed, but not
// tight -- collapsing the whitespace inside the window has to leave enough to
// fill the line, or the quote gets shorter the more the model indents.
func TestTheWindowIsBoundedAndStillFillsTheLine(t *testing.T) {
	a := &App{root: t.TempDir()}
	w := a.watchChat("narrate", true)
	for i := 0; i < 500; i++ { // ~2.5 kB, about a quarter-minute of a real call
		w.wrote("word\n", true)
	}
	if len(w.thought) > tailKeep {
		t.Errorf("the watch is holding %d bytes; a ten-minute call would hold all of it", len(w.thought))
	}
	if n := len([]rune(logTail(w.thought))); n < llmTailLen {
		t.Errorf("the quote came out %d characters, want %d -- the window is too tight to fill a line", n, llmTailLen)
	}
}

// The cut into a tail counts bytes, and a model writing dashes and quotes puts
// multi-byte runes across it.
func TestALongTailIsNotCutThroughACharacter(t *testing.T) {
	// é is two bytes and is laid across the cut: the last llmTailLen bytes
	// begin one byte into it. The whole rune goes, and nothing after it does
	tail := strings.Repeat("y", llmTailLen-1)
	got := logTail(strings.Repeat("x", 10) + "é" + tail)
	if want := "…" + tail; got != want {
		t.Errorf("a rune across the cut gave %q, want %q", got, want)
	}
}

// Once there is an answer it is the answer that gets quoted: the reasoning is
// the model talking to itself, and the answer is the thing being judged.
func TestTheAnswerIsQuotedInPreferenceToTheReasoning(t *testing.T) {
	a := &App{root: t.TempDir()}
	w := a.watchChat("suggest", true)
	w.wrote("so the second half is the weaker one and I will drop it", true)
	w.wrote(`{"segments":[`, false)
	line, _ := w.check(w.t0.Add(llmTick))
	if !strings.Contains(line, "segments") {
		t.Errorf("the line %q quotes something other than the answer", line)
	}
	if strings.Contains(line, "weaker one") {
		t.Error("the reasoning is still being quoted after the answer started")
	}
}

// A stopped run, a dropped connection and the guard giving up all reach the
// caller as "context canceled", which says nothing about which happened. Where
// the guard was the one who cancelled, the error says so.
func TestAGivenUpCallSaysSoRatherThanSayingContextCanceled(t *testing.T) {
	a := &App{root: t.TempDir()}
	w := a.watchChat("audit", true)
	plain := fmt.Errorf("context canceled")
	if got := w.blame(plain); got != plain {
		t.Errorf("an error nobody caused was rewritten to %v", got)
	}
	if w.blame(nil) != nil {
		t.Error("blame invented an error out of a call that worked")
	}
	w.check(w.t0.Add(llmStall)) // the guard gives up
	got := w.blame(plain).Error()
	if strings.Contains(got, "context canceled") || !strings.Contains(got, "nothing") {
		t.Errorf("the given-up call reported %q", got)
	}

	// a call that answered for a while and then stopped is a different story
	// from one that never started, and the error tells them apart
	w2 := a.watchChat("audit", true)
	w2.last = w2.t0.Add(90 * time.Second)
	w2.check(w2.last.Add(llmStall))
	if got := w2.blame(plain).Error(); !strings.Contains(got, "1m30s") {
		t.Errorf("a call that died mid-answer reported %q, want how far it got", got)
	}
}

// Liveness is any byte off the wire, not just the ones that become an answer:
// the parser above never sees a keep-alive, and a server sending them is a
// server that is still there.
func TestAnyByteOffTheWireCountsAsLife(t *testing.T) {
	a := &App{root: t.TempDir()}
	w := a.watchChat("narrate", true)
	w.last = w.t0.Add(-llmStall) // long since quiet
	buf := make([]byte, 8)
	if _, err := w.wrap(strings.NewReader(": keep-alive\n\n")).Read(buf); err != nil {
		t.Fatalf("read: %v", err)
	}
	if _, dead := w.check(time.Now()); dead {
		t.Error("a comment line kept the connection open and the watch called it dead")
	}
	src := funcBody(t, "llm.go", `func \(a \*App\) llmChatPost\(`)
	if !strings.Contains(src, "a.readChatStream(w.wrap(resp.Body)") {
		t.Error("the body is read unwrapped, so only parsed deltas count as life")
	}
}

// The reasoning is the model's own working. This server keeps it out of
// content entirely, so a call thinking hard sent nothing the parser could see
// and looked exactly like one that had hung -- it is read to be counted, and
// it is not the reply and not what the progress bar counts.
func TestTheReasoningIsCountedButIsNotTheReply(t *testing.T) {
	a := &App{root: t.TempDir()}
	w := a.watchChat("suggest", true)
	sse := strings.Join([]string{
		`data: {"choices":[{"delta":{"reasoning_content":"let me think"}}]}`,
		`data: {"choices":[{"delta":{"reasoning_content":" some more"}}]}`,
		`data: {"choices":[{"delta":{"content":"{\"segments\":"}}]}`,
		`data: {"choices":[{"delta":{"content":"[]}"}}]}`,
		"data: [DONE]",
	}, "\n\n") + "\n\n"

	var seen []string
	rep, err := a.readChatStream(strings.NewReader(sse), func(s string) { seen = append(seen, s) }, w)
	if err != nil {
		t.Fatalf("stream: %v", err)
	}
	reply := rep.Content
	if reply != `{"segments":[]}` {
		t.Errorf("the reply came out %q -- the thinking is in it", reply)
	}
	if len(seen) != 2 || seen[len(seen)-1] != reply {
		t.Errorf("the bar was fed %q, want only the two pieces of answer", seen)
	}
	if w.think != len("let me think some more") || w.reply != len(reply) {
		t.Errorf("counted %d thinking and %d reply bytes", w.think, w.reply)
	}
	if _, dead := w.check(w.t0.Add(llmStall)); dead {
		t.Error("a call that sent nothing but reasoning was called dead")
	}
}

// The guard cancels the request itself: the watch cannot end a call, it can
// only say that one should end.
func TestTheGuardCancelsTheCallItGaveUpOn(t *testing.T) {
	a := &App{root: t.TempDir()}
	w := a.watchChat("audit", true)
	w.t0 = w.t0.Add(-2 * llmStall)
	w.last, w.said, w.poll = w.t0, w.t0, time.Millisecond
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	stop := w.guard(cancel)
	defer stop()
	select {
	case <-ctx.Done():
	case <-time.After(5 * time.Second):
		t.Fatal("the guard let a call that had been silent for twice the window run on")
	}
	if !w.blameIsStall(t) {
		t.Error("the guard cancelled without recording that it was the one who did")
	}
}

func (w *chatWatch) blameIsStall(t *testing.T) bool {
	t.Helper()
	return strings.Contains(w.blame(fmt.Errorf("context canceled")).Error(), "nothing")
}
