package main

// Two things about the settings page: which ffmpeg the pipeline runs, and how
// much of the page is prose.
//
// ffmpeg used to be read-only -- taken off PATH, shown but not settable --
// which is right until a machine has two of them, or the GUI's PATH is not the
// shell's. It is a box now, and the box has to reach the runners: a setting
// that is saved and then ignored is worse than no setting.

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// The box, from typing to shelling out.
func TestTheFFmpegBoxIsWhatTheStepsRun(t *testing.T) {
	old := ffSet.Load()
	t.Cleanup(func() { ffSet.Store(old) })

	// empty is the ordinary answer: the bare name, which exec resolves off
	// PATH the way it always did
	e := ""
	ffSet.Store(&e)
	if got := ffTool("ffmpeg"); got != "ffmpeg" {
		t.Errorf("with an empty box the steps run %q, want the bare name", got)
	}
	if got := ffTool("ffprobe"); got != "ffprobe" {
		t.Errorf("with an empty box the steps probe with %q, want the bare name", got)
	}

	// a path is used as given, and ffprobe comes from BESIDE it -- a
	// hand-built ffmpeg paired with the distro's ffprobe is the mismatch the
	// box exists to fix, so probe must not fall back to PATH
	p := "/opt/ff7/bin/ffmpeg"
	ffSet.Store(&p)
	if got := ffTool("ffmpeg"); got != p {
		t.Errorf("ffTool = %q, want the path that was typed", got)
	}
	if got, want := ffTool("ffprobe"), "/opt/ff7/bin/ffprobe"; got != want {
		t.Errorf("ffprobe = %q, want %q -- beside the ffmpeg that was chosen", got, want)
	}
}

// Saved, re-read, and in force -- the runners read no config of their own, so
// readConf is where ffTool learns what was typed.
func TestTheFFmpegBoxSurvivesASaveAndReachesTheRunners(t *testing.T) {
	ownConfig(t)
	old := ffSet.Load()
	t.Cleanup(func() { ffSet.Store(old) })

	a := &App{root: t.TempDir()}
	c := a.readConf()
	if c.FFmpeg != "" {
		t.Errorf("a fresh config names ffmpeg %q, want empty -- PATH is the default", c.FFmpeg)
	}
	if got := ffTool("ffmpeg"); got != "ffmpeg" {
		t.Errorf("a fresh config left the steps running %q", got)
	}

	c.FFmpeg = "/opt/ff7/bin/ffmpeg"
	if err := a.writeConf(c); err != nil {
		t.Fatal(err)
	}
	// blank it, to prove the value comes back off disk and not out of memory
	blank := ""
	ffSet.Store(&blank)

	if got := a.readConf(); got.FFmpeg != c.FFmpeg {
		t.Errorf("after a save the config names %q, want %q", got.FFmpeg, c.FFmpeg)
	}
	if got := ffTool("ffmpeg"); got != c.FFmpeg {
		t.Errorf("reading the config left the steps running %q, want %q", got, c.FFmpeg)
	}

	// and it is a credential file: the key sits in it, so it stays 0600
	st, err := os.Stat(confPath())
	if err != nil {
		t.Fatal(err)
	}
	if st.Mode().Perm() != 0o600 {
		t.Errorf("llm.conf is mode %v, want 0600 -- it holds an API key", st.Mode().Perm())
	}
}

// Nothing shells out to a bare "ffmpeg" or "ffprobe" any more: one that does
// ignores the box on the machine the box was added for.
func TestNoStepGoesRoundTheFFmpegBox(t *testing.T) {
	files, err := filepath.Glob("*.go")
	if err != nil {
		t.Fatal(err)
	}
	for _, f := range files {
		if strings.HasSuffix(f, "_test.go") || f == "setup.go" {
			continue // setup.go is where the box and the lookup live
		}
		b, err := os.ReadFile(f)
		if err != nil {
			t.Fatal(err)
		}
		for _, bad := range []string{`exec.Command("ffmpeg"`, `exec.Command("ffprobe"`,
			`runCmd("ffmpeg"`, `runCmd("ffprobe"`} {
			if strings.Contains(string(b), bad) {
				t.Errorf("%s still calls %s… — it must go through ffTool, "+
					"or the settings box does nothing there", f, bad)
			}
		}
	}
}

// The page keeps its titles and hides its paragraphs. Every section says which
// API it expects, because that is the one thing a box cannot show.
func TestEverySettingsSectionSaysWhichAPIItExpects(t *testing.T) {
	b, err := os.ReadFile("setup.go")
	if err != nil {
		t.Fatal(err)
	}
	s := string(b)

	// the heading is a title and an ⓘ, and the words are the ⓘ's tooltip --
	// the same way every other explanation in the app is read. It was a button
	// with a popover: a thing to click, that stayed up, with selectable text
	// in it, so the one mark on this page that looks like every other ⓘ
	// behaved like none of them
	for _, want := range []string{
		"head := func(title, why string, opt bool) *gtk.Box {",
		`info := gtk.NewImageFromIconName("help-about-symbolic")`,
		"info.SetTooltipText(why)",
	} {
		if !strings.Contains(s, want) {
			t.Errorf("the section heading no longer builds %q", want)
		}
	}
	for _, gone := range []string{"gtk.NewPopover()", "SetSelectable(true)"} {
		if strings.Contains(s, gone) {
			t.Errorf("the settings page still explains itself with %q", gone)
		}
	}
	// a heading shares its line with the section's first row: it is a column
	// of the grid, not a row spanning it -- and the sections are told apart by
	// a rule between them rather than by a box around each, which would lay
	// every section's columns out separately
	for _, want := range []string{
		"grid.Attach(head(title, why, opt), 0, row, 1, 1)", // ...at column 0, on the section's first row
		"rule := gtk.NewSeparator(gtk.OrientationHorizontal)",
		"grid.Attach(rule, 0, row, 5, 1)", // ...across the whole width
		`grid.Attach(lbl("Server:"), 1, row, 1, 1)`,
		`grid.Attach(server, 2, row, 1, 1)`,
	} {
		if !strings.Contains(s, want) {
			t.Errorf("the settings grid no longer puts a heading beside its first row: %q", want)
		}
	}
	// and every box is one cell in one column, keys included -- those used to
	// run wide across the columns the Tests are in, so they were the only rows
	// whose right edge was somewhere else
	for _, want := range []string{"grid.Attach(key, 2, row+1, 1, 1)",
		"grid.Attach(ttsKey, 2, row+1, 1, 1)", "grid.Attach(sdKey, 2, row+1, 1, 1)"} {
		if !strings.Contains(s, want) {
			t.Errorf("a key box is not in the value column: %q", want)
		}
	}
	// four sections, four titles, and nothing longer than a word or two on
	// the page itself -- the em-dash subtitles are what moved behind the ⓘ.
	// Speaking and Listening were two of them, with two Server boxes and two
	// API key boxes for the same audio.cpp; they are one section now, Audio.
	for _, title := range []string{
		`sec("Writing", `, `sec("Cutting", `, `sec("Audio", `, `sec("Drawing", `,
	} {
		if strings.Count(s, title) != 1 {
			t.Errorf("the settings page has %d sections titled %s, want one",
				strings.Count(s, title), title)
		}
	}
	for _, gone := range []string{`sec("Speaking", `, `sec("Listening", `} {
		if strings.Contains(s, gone) {
			t.Errorf("the audio server is asked for twice again: %s", gone)
		}
	}
	// the two a session cannot run without come first, and the rest say they
	// are optional -- one small word, where a page of identical headings said
	// nothing about which was which
	order := []string{`sec("Writing", `, `sec("Cutting", `, `sec("Audio", `, `sec("Drawing", `}
	for i := 1; i < len(order); i++ {
		if strings.Index(s, order[i-1]) > strings.Index(s, order[i]) {
			t.Errorf("%s comes before %s", order[i], order[i-1])
		}
	}
	for _, c := range []struct{ tail, want string }{
		{`can see.", false)`, "Writing is required"},
		{`into a render.", false)`, "Cutting is required"},
		{`same path.", true)`, "Audio is optional"},
		{`holding it to a name.", true)`, "Drawing is optional"},
	} {
		if !strings.Contains(s, c.tail) {
			t.Errorf("the settings page no longer says %s", c.want)
		}
	}
	if !strings.Contains(s, `o.SetMarkup("<small>optional</small>")`) {
		t.Error("an optional section is not marked as one")
	}
	// each one names the API its boxes are expected to speak
	for _, api := range []string{
		"/v1/chat/completions", // Writing
		"/v1/audio/speech",     // Audio: speaking
		"/v1/tasks/run",        // Audio: listening
		"/sdcpp/v1/img_gen",    // Drawing
		"/sdcpp/v1/capabilities",
		"/v1/models",
	} {
		if !strings.Contains(s, api) {
			t.Errorf("no settings section tells the user about %s", api)
		}
	}
	// and Cutting says what ffmpeg is: not a server, and empty means PATH
	if !strings.Contains(s, "Not servers: local binaries") {
		t.Error("the ffmpeg section no longer says it is a local binary, not an endpoint")
	}
	// the standing paragraph above the model rows is gone
	if strings.Contains(s, `foot.AddCSSClass("dim-label")`) {
		t.Error("the settings page still prints a paragraph above the model rows")
	}
}

// The firefox row's Test is testFFmpeg's shape: the box resolves to a binary,
// the binary runs and says its version, and "off" is an answer rather than a
// fault. The headless search itself is the web's business and is not run
// here; what is pinned is that the Test goes through it, since a firefox
// that prints a version and cannot be driven is the one failure the version
// cannot show.
func TestTheFirefoxTestIsTheFFmpegTestsShape(t *testing.T) {
	a := &App{}
	if got, err := a.testFirefox(" OFF "); err != nil || !strings.Contains(got, "no web search") {
		t.Errorf("off answered %q, %v -- want a plain answer, not a fault", got, err)
	}
	if _, err := a.testFirefox("/no/such/firefox"); err == nil || !strings.Contains(err.Error(), "leave the box empty") {
		t.Errorf("a path that is not there answered %v, want the hint to use PATH", err)
	}
	// a fake that speaks --version
	dir := t.TempDir()
	fake := filepath.Join(dir, "firefox")
	if err := os.WriteFile(fake, []byte("#!/bin/sh\necho 'Mozilla Firefox 999.0'\n"), 0o755); err != nil {
		t.Fatal(err)
	}
	if ver, err := firefoxVersion(fake); err != nil || ver != "Mozilla Firefox 999.0" {
		t.Errorf("version = %q, %v", ver, err)
	}
	if _, err := firefoxVersion(filepath.Join(dir, "none")); err == nil {
		t.Error("a binary that is not there had a version")
	}
	// and the row goes through it, after the version, before the search
	src := readSrc(t, "setup.go")
	if !strings.Contains(src, `hook(testFxBtn, fxBadge, "firefox"`) || !strings.Contains(src, "return a.testFirefox(box)") {
		t.Error("the firefox row's Test does not run testFirefox")
	}
	body := funcBody(t, "setup.go", `func \(a \*App\) testFirefox\(`)
	i, j := strings.Index(body, "firefoxVersion(bin)"), strings.Index(body, `a.webSearch(context.Background(), bin, "duckduckgo")`)
	if i < 0 || j < 0 || i > j {
		t.Errorf("testFirefox does not check the version and then drive a search:\n%s", body)
	}
}
