package main

import (
	"context"
	"strings"
	"testing"
	"time"
)

// One chat request on the wire at a time: a second caller waits for the
// first to give the slot back, learns who it waited for, and a wait can be
// ended by the run's context.
func TestOneChatRequestAtATime(t *testing.T) {
	g := newLLMGate()
	if holder, err := g.take(context.Background(), "publish"); err != nil || holder != "" {
		t.Fatalf("the first take waited for %q (%v), want nothing", holder, err)
	}
	got := make(chan string, 1)
	go func() {
		holder, _ := g.take(context.Background(), "translate")
		got <- holder
	}()
	select {
	case h := <-got:
		t.Fatalf("the second request went out while the first was on the wire (waited for %q)", h)
	case <-time.After(50 * time.Millisecond):
	}
	g.give()
	select {
	case h := <-got:
		if h != "publish" {
			t.Errorf("the second request waited for %q, want publish", h)
		}
	case <-time.After(time.Second):
		t.Fatal("the second request never got the slot after the first gave it back")
	}
	// ⏹ ends a wait
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	if _, err := g.take(ctx, "describe"); err == nil {
		t.Error("a cancelled run still waited for the slot")
	}
	// and the wiring: the HTTP call takes it before the stall watch starts,
	// and the render translates after the clips are encoded, not before
	post := funcBody(t, "llm.go", `func \(a \*App\) llmChatPost\(`)
	i, j := strings.Index(post, "a.takeLLM(ctx, step)"), strings.Index(post, "a.watchChat(step")
	if i < 0 || j < 0 || i > j {
		t.Error("llmChatPost does not take the gate before the stall watch starts")
	}
	prod := funcBody(t, "produce.go", `func \(a \*App\) produce\(`)
	enc, tr := strings.Index(prod, "a.encodeClip("), strings.Index(prod, "a.subTracks(")
	if enc < 0 || tr < 0 || tr < enc {
		t.Error("the render translates the subtitles before the clips are encoded, so the encoder idles behind the gate")
	}
}
