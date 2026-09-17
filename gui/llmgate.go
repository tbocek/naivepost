package main

// One request at a time.
//
// Produce runs its two halves side by side -- the upload text on one
// goroutine, the render with its subtitle translation on another -- and on
// a session of any length both halves put a very large prompt to the same
// server at the same moment. A llama.cpp server with one slot does not run
// them together: it hands the slot back and forth, saving and restoring a
// hundred-thousand-token KV state each time, and on a 64-minute lecture that
// juggling is where the GPU was lost. Two prompts of that size in turn take
// the sum of their times; interleaved they took longer than that and then
// took the server down.
//
// So a chat request waits for the one before it to finish. Per app, not per
// step: it is the server that is single, and every step's request lands on
// it. Held only across the HTTP call, so a step that thinks between calls
// (the tool rounds) holds nothing while it thinks. A wait can be given up:
// ⏹ cancels the run's context, and a request queued behind another is
// still a request the run can end.
//
// The wait is said in the log once, with what it waits for, and it is not
// the stall watch's business: the watch starts once the request is on the
// wire (llmChatPost), so a minute in this queue is never "nothing yet".

import (
	"context"
	"sync"
)

// llmGate is the one-request-at-a-time lock and who holds it. A channel of
// one, not a mutex, because a wait on it has to be cancellable.
type llmGate struct {
	slot chan struct{}
	mu   sync.Mutex
	held string // the step whose request is on the wire, "" for none
}

func newLLMGate() *llmGate { return &llmGate{slot: make(chan struct{}, 1)} }

// take waits for the slot. It returns the step that was holding it when the
// wait began ("" for none, i.e. no wait), or the context's error if the run
// ended first.
func (g *llmGate) take(ctx context.Context, step string) (string, error) {
	g.mu.Lock()
	holder := g.held
	g.mu.Unlock()
	select {
	case g.slot <- struct{}{}:
	case <-ctx.Done():
		return holder, ctx.Err()
	}
	g.mu.Lock()
	g.held = step
	g.mu.Unlock()
	return holder, nil
}

func (g *llmGate) give() {
	g.mu.Lock()
	g.held = ""
	g.mu.Unlock()
	<-g.slot
}

// takeLLM is the gate as the app uses it: the wait, said in the log when
// there was one. The app's gate is made on first use so a headless test's
// App works without one.
func (a *App) takeLLM(ctx context.Context, step string) error {
	a.gateMu.Lock()
	if a.gate == nil {
		a.gate = newLLMGate()
	}
	g := a.gate
	a.gateMu.Unlock()
	holder, err := g.take(ctx, step)
	if err != nil {
		return err
	}
	if holder != "" && holder != step {
		a.logfIdle(">>> %s: waited for the LLM -- it was busy with %s; one request at a time", step, holder)
	}
	return nil
}

func (a *App) giveLLM() {
	a.gateMu.Lock()
	g := a.gate
	a.gateMu.Unlock()
	if g != nil {
		g.give()
	}
}
