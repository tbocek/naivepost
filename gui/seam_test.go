package main

// The line between the pages and the pipeline, where it was found crossed.
//
// The pipeline's files are written as logic -- a translate pass touches no
// widget -- but nothing enforced it, and three places had drifted over the
// line. Each is pinned here with what it cost, so the next drift is a red
// test rather than a race that happens to work.

import (
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
