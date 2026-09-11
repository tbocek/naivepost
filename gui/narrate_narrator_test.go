package main

// The narrator tags reaching Produce. What a slot means is "whose voice this is",
// so it is the tag -- not the order the files happen to sit in -- that decides
// who the narration is spoken by, and the picker has to offer exactly the people
// the session named.

import (
	"os"
	"path/filepath"
	"regexp"
	"strings"
	"testing"
)

// voiceApp is an App whose voices folder is empty and whose sources are the
// given snapshot: enough for the picker without a window, a server or the CC0
// samples this machine may or may not have installed.
func voiceApp(t *testing.T, narr ...string) *App {
	t.Helper()
	root := t.TempDir()
	a := &App{root: root, outDir: root}
	models := filepath.Join(root, "models")
	if err := os.MkdirAll(filepath.Join(models, "voices"), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := a.writeConf(appConf{Voices: filepath.Join(models, "voices")}); err != nil {
		t.Fatal(err)
	}
	for i, p := range narr {
		if i < narratorSlots {
			a.selNarr[i] = p
		}
		if p != "" {
			a.selAud = append(a.selAud, p)
		}
	}
	return a
}

// The ids are what voice.txt and every synthesis cache key hold, so they have to
// survive a round trip -- and slot 1 has to stay spelled "own", or every project
// written before the other three existed starts speaking in something else and
// re-synthesizes narration that was already paid for.
func TestNarratorVoiceIDsRoundTripAndSlotOneIsStillOwn(t *testing.T) {
	if got := narratorVoiceID(1); got != ownVoice {
		t.Errorf("narrator 1's id is %q, want %q", got, ownVoice)
	}
	for n := 1; n <= narratorSlots; n++ {
		if got := narratorSlot(narratorVoiceID(n)); got != n {
			t.Errorf("slot %d survived the round trip as %d", n, got)
		}
	}
	// a wav out of the voices folder is not a narrator, whatever it is called
	for _, id := range []string{"cv-gb-female-30s", "narrator", "narrator0",
		"narrator1", "narrator5", "narratorX", "2"} {
		if got := narratorSlot(id); got != 0 {
			t.Errorf("voice %q was read as narrator %d", id, got)
		}
	}
}

// The picker offers the people the session tagged: slot 1 always, since somebody
// speaks the narration either way, and 2..4 only once a recording carries them.
func TestThePickerOffersExactlyTheTaggedNarrators(t *testing.T) {
	a := voiceApp(t, "/rec/me.flac", "", "/rec/mate.flac")
	var ids []string
	for _, v := range a.listVoices() {
		ids = append(ids, v.id)
	}
	// "no audio" leads: not a narrator, but always on offer (see its own tests)
	want := []string{captionsVoice, ownVoice, "narrator3"}
	if len(ids) != len(want) {
		t.Fatalf("the picker offers %v, want %v", ids, want)
	}
	for i := range want {
		if ids[i] != want[i] {
			t.Errorf("row %d is %q, want %q", i, ids[i], want[i])
		}
	}
	// and each row names the file it clones, because that is the thing being
	// chosen -- a slot number alone says nothing about who that is. Slot 1 is a
	// tag like the rest: it is whoever the Inputs step says, not "me".
	if got := a.narratorVoiceName(1); got != "Narrator 1 — me.flac" {
		t.Errorf("slot 1 reads %q", got)
	}
	if got := a.narratorVoiceName(3); got != "Narrator 3 — mate.flac" {
		t.Errorf("slot 3 reads %q", got)
	}
}

// Where each slot is cut from. Slot 1 falls back to the session's first
// recording -- that is what "my own voice" meant before the tags existed, so a
// project from back then keeps speaking in the same voice -- while an untagged
// slot 2..4 is nobody and must not quietly borrow somebody else's recording.
func TestNarratorSourceFallsBackOnlyForSlotOne(t *testing.T) {
	tagged := voiceApp(t, "/rec/me.flac")
	if got := tagged.narratorSource(1); got != "/rec/me.flac" {
		t.Errorf("slot 1 is cut from %q", got)
	}
	if got := tagged.narratorSource(2); got != "" {
		t.Errorf("an untagged slot 2 is cut from %q, want nothing", got)
	}

	untagged := &App{selAud: []string{"/rec/first.flac", "/rec/second.flac"}}
	if got := untagged.narratorSource(1); got != "/rec/first.flac" {
		t.Errorf("a project with no tags speaks as %q, want the first recording", got)
	}
	// a session that is one screen capture and nothing else: the capture is
	// still where a voice can be cut from
	video := &App{selVid: []string{"/rec/screen.mkv"}}
	if got := video.narratorSource(1); got != "/rec/screen.mkv" {
		t.Errorf("a footage-only session speaks as %q, want the footage", got)
	}
}

// ---- what ▶ owes you --------------------------------------------------------

// TestStaleForOnlyRewritesWhenItMust guards the rule that makes one button
// safe: ▶ asks the model again when the narration cannot be right, and never
// merely because it could be different. Get this wrong in the permissive
// direction and every press quietly throws away hand-written lines; wrong the
// other way and a cut you have re-edited keeps narration written for clips that
// are no longer there.
func TestStaleForOnlyRewritesWhenItMust(t *testing.T) {
	segs := []cutSeg{{S: 1, E: 5}, {S: 20, E: 26}}
	n := &narrator{}
	if n.staleFor(segs) == "" {
		t.Error("no narration at all counted as current")
	}
	n.entries = []narrEntry{{S: 1, E: 5, Text: "one"}, {S: 20, E: 26, Text: "two"}}
	if why := n.staleFor(segs); why != "" {
		t.Errorf("a narration matching its cut wants a rewrite: %s", why)
	}
	// the case that has to stay quiet: your own words, kept
	n.entries[0].Text = "the line I wrote myself"
	n.entries[1].Emotion = "dry"
	if why := n.staleFor(segs); why != "" {
		t.Errorf("editing a line asked for a rewrite that would delete it: %s", why)
	}
	if n.staleFor([]cutSeg{{S: 1, E: 5}, {S: 21, E: 26}}) == "" {
		t.Error("a clip moved a second and the narration still counted as current")
	}
	if n.staleFor(segs[:1]) == "" {
		t.Error("a clip removed and the narration still counted as current")
	}
	if n.staleFor(append(segs, cutSeg{S: 40, E: 44})) == "" {
		t.Error("a clip added and the narration still counted as current")
	}
	// a clip may carry several lines: still current, and still the user's
	n.entries = []narrEntry{{S: 1, E: 5, Text: "one"}, {S: 1, E: 5, At: 2, Text: "one more"},
		{S: 20, E: 26, Text: "two"}}
	if why := n.staleFor(segs); why != "" {
		t.Errorf("a clip with two hand-placed lines wants a rewrite: %s", why)
	}
}

// TestHandEditsSurviveTheAppBeingClosed: since the ✎ notes went away, the
// lines themselves are the editing surface -- placement, emotion and words all
// live in the entries, and the file is the only place they survive an evening.
func TestHandEditsSurviveTheAppBeingClosed(t *testing.T) {
	root := t.TempDir()
	a := &App{root: root, outDir: root}
	n := &narrator{a: a, entries: []narrEntry{
		{S: 0, E: 30, At: 4, Text: "x", Emotion: "deadpan"},
		{S: 0, E: 30, At: 21, Text: "y", Emotion: "screaming"},
	}}
	n.save()
	back := &narrator{a: a}
	back.load()
	if len(back.entries) != 2 || back.entries[1].At != 21 ||
		back.entries[1].Emotion != "screaming" || back.entries[0].Text != "x" {
		t.Errorf("entries came back as %+v", back.entries)
	}
}

// TestNarrateIsOneButton: the two halves of this step were two buttons in two
// places, and the split was the tool's, not the user's. Source-level, because
// what it guards is a page's wiring -- nothing at run time notices a second
// button growing back or ▶ losing half its job.
func TestNarrateIsOneButton(t *testing.T) {
	page, err := os.ReadFile("narrate.go")
	if err != nil {
		t.Fatal(err)
	}
	if strings.Contains(string(page), `NewButtonWithLabel("Generate narration")`) {
		t.Error("the page grew its own generate button back; ▶ is the way in")
	}
	run := regexp.MustCompile(`(?s)func \(a \*App\) narrateRun\(\).*?\n}\n`).Find(page)
	if run == nil {
		t.Fatal("narrateRun is gone")
	}
	for _, half := range []string{"a.writeNarration(", "a.synthesize("} {
		if !strings.Contains(string(run), half) {
			t.Errorf("narrateRun does not reach %s — ▶ only does half the step", half)
		}
	}
	bar, err := os.ReadFile("pipeline.go")
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(bar), "a.narrateRun()") {
		t.Error("the run bar's ▶ no longer starts the narration step")
	}
}

// TestTheBarPulsesOnlyWhileNothingCanBeCounted: this step is one unmeasurable
// call followed by n measurable ones. The pulse was stopped by the end of the
// RUN rather than the end of the writing, so the whole synthesis -- the half
// that knows exactly how far along it is, and says so in the text on the bar --
// was shown as a block sliding back and forth. A bar that reads "speaking 4/9"
// while animating like it has no idea is worse than either alone.
func TestTheBarPulsesOnlyWhileNothingCanBeCounted(t *testing.T) {
	page, err := os.ReadFile("narrate.go")
	if err != nil {
		t.Fatal(err)
	}
	run := string(regexp.MustCompile(`(?s)func \(a \*App\) narrateRun\(\).*?\n}\n`).Find(page))
	if run == "" {
		t.Fatal("narrateRun is gone")
	}
	if !strings.Contains(run, "if !a.running || !writing {") {
		t.Error("the pulse outlives the writing stage, so it animates over the counted one")
	}
	if !strings.Contains(run, "writing = false") {
		t.Error("nothing ever ends the pulse: the bar pulses to the end of the run")
	}
	// and the count covers every line, cached or not -- otherwise the bar
	// stands still through a narration that was already spoken
	prog := strings.Index(run, `a.prog(trackSTT`)
	cache := strings.Index(run, "exists(a.ttsWav(e))")
	if prog < 0 || cache < 0 {
		t.Fatal("the speaking loop no longer reads the way this test assumes")
	}
	if prog > cache {
		t.Error("a cached line moves neither the bar nor its count")
	}
	// the two tracks are summed, so last step's leftovers would be added to
	// every reading this one takes: startRun empties the queue
	if !strings.Contains(run, "a.startRun()") {
		t.Error("the bar is not reset at the start of the run")
	}
	// ...and once the writing IS being counted, the pulse has to stop on its
	// own: Pulse and SetFraction drive the same needle, so a pulse still
	// running would wipe out every reading the stream produces
	if !strings.Contains(run, "counted := a.progParts[trackSTT] > 0") {
		t.Error("the pulse ignores the streamed clip count and animates over it")
	}
	// the two halves share the bar rather than each owning all of it: the
	// speaking used to start again from zero after the writing filled it
	if !strings.Contains(run, "speakBase+speakSpan*") {
		t.Error("the speaking does not continue where the writing stopped")
	}
	// and the writing is only measurable because the reply is streamed
	if !strings.Contains(string(page), `a.llmChatRetryTools("narrate", msgs, think, tools, a.webRunner("narrate", ffx), onText)`) {
		t.Error("the narration request is not streamed, so there is nothing to count until it is over")
	}
}

// TestPlayAlwaysRewritesTheNarration: ▶ used to write only when staleFor found
// something wrong, which meant a narration you wanted redone -- every clip
// covered, so nothing "wrong" -- had no button at all. Asking for it again is
// the ordinary reason to press ▶, so the check now names the reason for the log
// and never withholds the run.
func TestPlayAlwaysRewritesTheNarration(t *testing.T) {
	run := string(regexp.MustCompile(`(?s)func \(a \*App\) narrateRun\(\).*?\n}\n`).Find([]byte(readSrc(t, "narrate.go"))))
	if run == "" {
		t.Fatal("narrateRun is gone")
	}
	if !strings.Contains(run, "writing := true") {
		t.Error("the writing half is conditional again — ▶ can decline to rewrite")
	}
	for _, gate := range []string{`if why != ""`, "the narration matches the cut"} {
		if strings.Contains(run, gate) {
			t.Errorf("narrateRun still gates on %q — a run that matches the cut does nothing", gate)
		}
	}
	if !strings.Contains(run, `why = "rewriting every line"`) {
		t.Error("a run that found nothing stale logs a blank reason")
	}
	if !strings.Contains(run, "keepPrevNarration(a.narrPath())") {
		t.Error("the old narration is overwritten with no copy kept — a hand-edited line is one press from gone")
	}
}

// TestTheOverwrittenNarrationIsKeptAside: the price of ▶ always rewriting is
// that hand-written lines are one press from a fresh draft. One copy, taken
// before the model's answer lands, is what makes that press undoable.
func TestTheOverwrittenNarrationIsKeptAside(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "narration.json")
	if _, err := keepPrevNarration(path); err == nil {
		t.Error("keeping a narration that was never written reported success")
	}
	if err := os.WriteFile(path, []byte(`{"entries":[{"text":"by hand"}]}`), 0o644); err != nil {
		t.Fatal(err)
	}
	prev, err := keepPrevNarration(path)
	if err != nil {
		t.Fatal(err)
	}
	if prev == path {
		t.Fatal("the copy is the original — the run would overwrite it too")
	}
	b, err := os.ReadFile(prev)
	if err != nil {
		t.Fatalf("keepPrevNarration reported %s but nothing is there: %v", prev, err)
	}
	if !strings.Contains(string(b), "by hand") {
		t.Errorf("the copy does not hold the lines it was meant to save: %s", b)
	}
}
