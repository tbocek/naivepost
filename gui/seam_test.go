package main

// The line between the pages and the pipeline, where it was found crossed.
//
// The pipeline's files are written as logic -- a translate pass touches no
// widget -- but nothing enforced it, and three places had drifted over the
// line. Each is pinned here with what it cost, so the next drift is a red
// test rather than a race that happens to work.

import (
	"errors"
	"os"
	"strings"
	"testing"
)

// videoStyleName is read by findMarks, in Prepare's goroutine. It used to
// answer with stylePick.Selected() -- a GTK call off the main thread, which
// worked the way such calls do: until it does not. It is a cached string now,
// under the same mutex as the language and the context, written by the
// dropdown and by the project.
func TestTheStyleIsReadFromACacheAndNotTheWidget(t *testing.T) {
	body := funcBody(t, "prep.go", `func \(a \*App\) videoStyleName\(`)
	if strings.Contains(body, "stylePick") || strings.Contains(body, "Selected()") {
		t.Error("videoStyleName reads the dropdown, and it is called from a runner's goroutine")
	}
	if !strings.Contains(body, "a.promptMu.Lock()") {
		t.Error("videoStyleName is not guarded like the other cached strings (sessionCtx, asrLanguage)")
	}
	// both writers go through the setter: the hand on the dropdown, and the
	// project on load. A widget change that skipped the cache would leave the
	// runner cutting the previous style.
	src := readSrc(t, "prep.go")
	for _, want := range []string{
		"a.setVideoStyle(styleOf(a.stylePick.Selected()))", // the dropdown's notify
		"a.setVideoStyle(name)",                            // applyStyle
	} {
		if !strings.Contains(src, want) {
			t.Errorf("prep.go no longer writes the style cache with %q", want)
		}
	}
	// and the cache is written BEFORE the save that the same change triggers,
	// or the project file carries the previous style
	hook := closure(t, "prep.go", `a.stylePick.NotifyProperty("selected", func() {`)
	i, j := strings.Index(hook, "a.setVideoStyle("), strings.Index(hook, "a.saveProjectNow()")
	if i < 0 || j < 0 || i > j {
		t.Error("the style is saved before the cache that the save reads is written")
	}
}

// textCut is arithmetic over words and returns segments; the page applies
// them. It used to do both -- undo stack, persist, status line -- which made
// a file about words the one logic file that drove a widget.
func TestTheTextCutIsArithmeticAndThePageAppliesIt(t *testing.T) {
	body := funcBody(t, "textedit.go", `func \(a \*App\) textCut\(`)
	for _, gone := range []string{"a.ed.", "a.setStatus(", "pushUndo", "persist()", "a.logf("} {
		if strings.Contains(body, gone) {
			t.Errorf("textCut still reaches the page: %q", gone)
		}
	}
	if !strings.Contains(body, "return segs, marked, dead") {
		t.Error("textCut does not hand its answer back as values")
	}
	// the other half lives with the page, and is what ▶ calls
	apply := funcBody(t, "cut_suggest.go", `func \(ed \*cutEditor\) applyTextCut\(`)
	for _, want := range []string{"ed.pushUndo()", "ed.persist()", "ed.setBase()", "ed.a.setStatus("} {
		if !strings.Contains(apply, want) {
			t.Errorf("applyTextCut no longer does %q -- the page half of the old cutByText", want)
		}
	}
	if strings.Contains(readSrc(t, "textedit.go"), "gotk4") {
		t.Error("textedit.go imports GTK")
	}
}

// The render's stamp is named from the settings the run was handed, not read
// off the Produce page: renderStale and markRendered run in the render's
// goroutine, and the page is the GUI thread's.
func TestTheRenderStampIsNamedFromTheSettingsNotThePage(t *testing.T) {
	src := readSrc(t, "produce_stamp.go")
	if strings.Contains(src, "a.prod.") || strings.Contains(src, "a.prod ") {
		t.Error("produce_stamp.go reads the Produce page from the render's goroutine")
	}
	if !strings.Contains(src, "func renderStampFile(out string) string") {
		t.Error("renderStampFile no longer takes the output path it names")
	}
	for _, want := range []string{"renderStampFile(st.OutFile)"} {
		if strings.Count(src, want) < 2 {
			t.Errorf("renderStale and markRendered do not both name the stamp from st.OutFile")
		}
	}
}

// pipeline.go is the steps and their subprocesses, and needs no GTK to
// compile. The run bar -- ▶, ⏹, which page's playback they drive -- is
// runbar.go, where the one widget helper that used to sit in pipeline.go
// went with the rest of its family.
func TestThePipelineFileImportsNoGTK(t *testing.T) {
	for _, f := range []string{"pipeline.go", "textedit.go", "translate.go", "transcript.go",
		"retake.go", "describe.go", "align.go", "llm.go", "audiocpp.go", "sdcpp.go",
		"produce_stamp.go", "produce_embed.go", "subwords.go"} {
		b, err := os.ReadFile(f)
		if err != nil {
			t.Fatal(err)
		}
		if strings.Contains(string(b), "github.com/diamondburned/gotk4") {
			t.Errorf("%s imports GTK; it is a pipeline file", f)
		}
	}
	bar := readSrc(t, "runbar.go")
	for _, want := range []string{"func setPlayIcon(", "func (a *App) syncPlayIcons()",
		"func (a *App) playClicked()", "type transport interface"} {
		if !strings.Contains(bar, want) {
			t.Errorf("runbar.go does not hold %q", want)
		}
	}
}

// The diarization window is a preference, not a constant.
//
// Sortformer allocates one contiguous buffer per request and it grows with the
// window. Measured on a real box with an LLM's weights pinned beside it: at
// 90 s the server answered "Failed to allocate Sortformer diar backend
// tensors", at 45 s it answered in 400 ms, at 5 s likewise. The window the
// model permits and the window the machine can hold are two different numbers,
// so the pass walks down until one fits.
func TestDiarizationStepsDownItsWindowRatherThanFailing(t *testing.T) {
	if len(diarWins) < 2 || diarWins[0] != diarWin {
		t.Errorf("the window ladder is %v, want the longest first", diarWins)
	}
	for i := 1; i < len(diarWins); i++ {
		if diarWins[i] >= diarWins[i-1] {
			t.Errorf("the ladder does not descend: %v", diarWins)
		}
	}
	// only an allocation failure steps down; anything else is the answer
	for _, c := range []struct {
		err  string
		down bool
	}{
		{"sortformer-diar on http://127.0.0.1:8765 answered 500 Internal Server Error: " +
			`{"error":{"message":"Failed to allocate Sortformer diar backend tensors"}}`, true},
		{"cudaMalloc failed: out of memory", true},
		{"alloc_tensor_range: failed to allocate ROCm0 buffer of size 3074294656", true},
		{"no model \"sortformer-diar\" on that server", false},
		{"anchor too long (48.0 s of 45 s window)", false},
		{"stopped by user", false},
	} {
		if got := noRoom(errors.New(c.err)); got != c.down {
			t.Errorf("noRoom(%.40q) = %v, want %v", c.err, got, c.down)
		}
	}
	if noRoom(nil) {
		t.Error("no error reads as no room")
	}
	body := funcBody(t, "pipeline.go", `func \(a \*App\) diarize\(`)
	if !strings.Contains(body, "for i, win := range diarWins {") ||
		!strings.Contains(body, "!noRoom(err)") {
		t.Error("diarize no longer walks the ladder, or walks it for errors that are the answer")
	}
	// the anchor is a share of the window: four voices at a fixed 12 s each is
	// 48 s, which leaves a 45 s window no room for any new audio at all
	at := funcBody(t, "pipeline.go", `func \(a \*App\) diarizeAt\(`)
	if !strings.Contains(at, "per := math.Min(anchorPer, win/4)") {
		t.Error("the anchor does not shrink with the window, so a short window has no room for audio")
	}
	if strings.Contains(at, "diarWin") {
		t.Error("diarizeAt still reads the constant instead of the window it was given")
	}
}

// The ASR walks the same ladder, for the same failure and with one more thing
// at stake: a chunk edge is where the decoder loses its context, so the text at
// a join is the worst text in the file. That is why the chunk starts at what
// the MODEL allows -- 300 s, or 60 s for a qwen3, which allocates about 1.5 GB
// per 20 s -- and is halved only when the server says it has no room, rather
// than being set small to be safe.
func TestTheASRHalvesItsChunkRatherThanFailing(t *testing.T) {
	body := funcBody(t, "pipeline.go", `func \(a \*App\) asrLong\(`)
	for _, want := range []string{
		"limit := a.asrChunk()",                // the model's answer first
		"!noRoom(err) || limit <= asrChunkMin", // and only memory steps down
		"math.Max(asrChunkMin, math.Floor(limit/2))",
	} {
		if !strings.Contains(body, want) {
			t.Errorf("asrLong no longer contains %q", want)
		}
	}
	// there is a floor: under it the edges outnumber the sentences, and a
	// transcript in ten-second pieces is a worse answer than the failure
	if asrChunkMin < 15 || asrChunkMin >= asrChunkQwen {
		t.Errorf("the floor is %.0f s, which is not between a sentence and the smallest model limit", asrChunkMin)
	}
	// the retry re-cuts from scratch: the edges move when the limit does, so a
	// chunk list from the longer attempt cannot be reused
	at := funcBody(t, "pipeline.go", `func \(a \*App\) asrLongAt\(`)
	if !strings.Contains(at, "asrCuts(dur, a.quietSpots(wav), limit, seek)") {
		t.Error("the chunk edges are not cut from the limit this attempt is using")
	}
	if strings.Contains(at, "a.asrChunk()") {
		t.Error("asrLongAt asks the model again instead of using the limit it was given")
	}
}

// The user context is one box for the whole session, so it carries
// instructions for jobs other than the one reading it -- a title, a
// description, a thumbnail. It shipped that way and the text edit answered
// with them: a session whose context said a title and a one-paragraph
// description got back exactly those, 668 bytes where 32'000 characters of
// speech went in. Nothing matched, nothing was marked, and Cut kept every
// filmed second including the stumbles the pass exists to remove.
func TestAContextInstructionForAnotherJobIsNotAnInstruction(t *testing.T) {
	// said once, to every job, where the context's own rule is stated
	for _, want := range []string{
		"every job is given the same copy",
		"An instruction that names a step or a thing another job makes is that job's",
		"answer your own question, in your own shape, and leave it alone",
	} {
		if !strings.Contains(ctxRule, want) {
			t.Errorf("ctxRule no longer says %q", want)
		}
	}
	// ...and the text edit's answer may only DELETE from the words it was
	// shown, so a title or a paragraph arriving there matches nothing and
	// changes nothing (seamCutOf)
	if !strings.Contains(textSystem, `{"joined":`) || !strings.Contains(textSystem, "DELETE ONLY") {
		t.Error("the text edit accepts an answer that is not made of the words it gave")
	}
}
