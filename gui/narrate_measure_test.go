package main

// measured (narrate.go) used to spawn an ffprobe on the GTK thread. It is
// asked on every rebuild and on every playback tick, so a batch of fresh takes
// froze the page for a probe apiece. Now the probe goes out on a goroutine,
// the estimate stands in, and the answer lands back on the idle loop.

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestATakesLengthIsMeasuredOffThePageThread(t *testing.T) {
	n := &narrator{durCache: map[string]float64{}, durProbe: map[string]bool{}}
	n.a = &App{root: t.TempDir(), outDir: t.TempDir()}
	e := narrEntry{S: 0, E: 10, Text: "hello there"}
	wav := n.a.ttsWav(e)

	// already measured: answered from the cache, nobody sent out
	n.durCache[wav] = 3.25
	if got := n.measured(e); got != 3.25 {
		t.Errorf("a measured take answers %v, want the cached 3.25", got)
	}
	if len(n.durProbe) != 0 {
		t.Error("a cached take was sent out to be measured again")
	}

	// not yet spoken: nothing to measure, and nothing sent looking for it
	delete(n.durCache, wav)
	if got := n.measured(e); got != 0 {
		t.Errorf("an unspoken take answers %v, want 0", got)
	}
	if n.durProbe[wav] {
		t.Error("a probe was sent out for a wav that does not exist")
	}

	// spoken but unmeasured: the answer is 0 RIGHT NOW -- speechDur estimates
	// off the text until the real length lands -- and the probe goes out
	// instead of being waited for
	if err := os.MkdirAll(filepath.Dir(wav), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(wav, []byte("not really a wav"), 0o644); err != nil {
		t.Fatal(err)
	}
	if got := n.measured(e); got != 0 {
		t.Errorf("an unmeasured take answered %v at once — the page waited on the probe", got)
	}
	if !n.durProbe[wav] {
		t.Error("no probe was sent out for the fresh take")
	}
	// asked again while the probe is out (headless: the answer never lands):
	// still the estimate, and no second probe behind the first
	if got := n.measured(e); got != 0 {
		t.Errorf("asked again mid-probe the take answers %v, want 0", got)
	}

	// a narrator built bare (tests) has no idle loop for an answer to land
	// on, and says so instead of leaking a goroutine into it
	bare := &narrator{a: n.a}
	if got := bare.measured(e); got != 0 {
		t.Errorf("a bare narrator answers %v, want 0", got)
	}
}

// The wiring the headless test cannot watch: where the probe runs, and where
// its answer lands.
func TestTheMeasureProbesAnswerLandsOnTheIdleLoop(t *testing.T) {
	body := funcBody(t, "narrate.go", `func \(n \*narrator\) measured\(e narrEntry\) float64 \{`)
	for _, want := range []string{
		// one guard, all four reasons not to send a probe
		"if n.durCache == nil || n.durProbe == nil || n.durProbe[wav] || !exists(wav) {",
		"go func() {",           // the ffprobe is a spawned process: off the GTK thread
		"glib.IdleAdd(func() {", // and its answer lands back on it
		"delete(n.durProbe, wav)",
		"n.durCache[wav] = d",
		"n.queueRebuild() // the (~) rows re-measure against the real length",
		// a failed probe is not remembered as an answer -- the wav may still
		// have been being written -- so the next ask simply tries again
		"if err != nil {\n\t\t\t\treturn\n\t\t\t}",
	} {
		if !strings.Contains(body, want) {
			t.Errorf("measured no longer contains %q", want)
		}
	}
	if strings.Contains(body, "durCache[wav] = 0") {
		t.Error("a failed probe is written into the cache — a half-written take would stick at 0")
	}
}
