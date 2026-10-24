package main

import (
	"os"
	"path/filepath"
	"regexp"
	"strings"
	"testing"
)

// The tab row and the gates are two lists that have to agree. When they drift,
// a finished step is barred or an unfinished one opens onto an empty page, and
// both read as a bug in the step rather than in the table -- which is where the
// renumbering that comes with every merged page actually breaks things.
//
// Since Publish folded into Produce each flag gates exactly one tab again,
// and produceLocked waits on the cut and nothing else -- not on the rendered
// video, which its own ▶ is what makes.
func TestEachGateLocksExactlyItsOwnTab(t *testing.T) {
	for _, c := range []struct {
		pages []string
		set   func(*App)
	}{
		{[]string{"cut"}, func(a *App) { a.cutLocked = true }},
		{[]string{"narrate"}, func(a *App) { a.narrateLocked = true }},
		{[]string{"produce"}, func(a *App) { a.produceLocked = true }},
	} {
		a := &App{}
		c.set(a)
		gated := map[string]bool{}
		for _, p := range c.pages {
			gated[p] = true
		}
		for i, s := range steps {
			if got, want := a.stepLocked(i), gated[s.name]; got != want {
				t.Errorf("with only %v gated, tab %d (%s) locked = %v, want %v",
					c.pages, i, s.name, got, want)
			}
		}
	}

	// the first tab is where a locked tab bounces to, so it must never lock:
	// a locked landing page is a window with nowhere to go
	all := &App{cutLocked: true, narrateLocked: true, produceLocked: true}
	if all.stepLocked(0) {
		t.Error("the first tab locked -- it is where a locked tab bounces to")
	}
	if all.stepLocked(stepIndex("no such page")) {
		t.Error("an unknown page counts as locked; refusing to show a page is the worse mistake")
	}
}

// Every tab says something on hover, and a locked one has to say what is
// missing -- the tooltip is the only place it can, since the tab stays
// clickable precisely so that it can be hovered. help is the same obligation
// one level down: the ⓘ popover is now the only place a step is explained at
// length, so a step added without one is a step nothing describes anywhere.
func TestEveryTabExplainsItself(t *testing.T) {
	for i, s := range steps {
		if stepIndex(s.name) != i {
			t.Errorf("stepIndex(%q) = %d, want %d", s.name, stepIndex(s.name), i)
		}
		if s.label == "" || s.tip == "" {
			t.Errorf("tab %d (%s): label %q, tip %q -- both are shown", i, s.name, s.label, s.tip)
		}
		if (s.wait == "") != (i == 0) {
			t.Errorf("tab %d (%s): wait hint %q, want one on every tab but the first",
				i, s.name, s.wait)
		}
		// long enough to be an explanation rather than the tooltip again: the
		// popover exists because the tooltip had no room, and one that only
		// repeats it is a button that answers nothing
		if len(s.help) < len(s.tip)+80 {
			t.Errorf("tab %d (%s): help is %d chars against a %d-char tip -- "+
				"the ⓘ popover is where the paragraph went, not a second tooltip",
				i, s.name, len(s.help), len(s.tip))
		}
	}
}

// Every page that reads something says so the same way, and in the same place
// as every page that WRITES something: the two groups at the right of the
// shared bottom bar, Inputs left of Outputs, in the order the step happens.
//
// It used to be a row at the top of each page, built line for line four times
// over -- and before that it was four rows that differed by an indent and a
// colour, which is enough to make a reader stop and check they are on the page
// they think they are. Now there is one heading, one constructor, and a page
// owns only the line under it. Source-level: nothing at run time can tell that
// four rows are meant to be one row.
func TestEveryStepSaysWhatItReadsTheSameWay(t *testing.T) {
	// one constructor for the line itself, so no page can grow its own
	// ellipsizing, its own width cap or its own alignment
	mk := funcBody(t, "prep.go", `func inputsLabel\(\) \*gtk.Label \{`)
	for _, want := range []string{
		"l.SetXAlign(0)",
		"l.SetEllipsize(pango.EllipsizeEnd)", // never a floor under the window
		"l.SetMaxWidthChars(60)",             // nor a bar with its buttons pushed off
	} {
		if !strings.Contains(mk, want) {
			t.Errorf("inputsLabel no longer does %q", want)
		}
	}
	// the shared bar carries the one heading and the one stack behind it
	main := readSrc(t, "main.go")
	for _, want := range []string{
		`inLbl := gtk.NewLabel("Inputs:")`,
		`inLbl.AddCSSClass("heading")`,
		"ctlRow.Append(inLbl)",
		"ctlRow.Append(a.inStack)",
		"a.inStack.SetVisibleChildName(name)",
	} {
		if !strings.Contains(main, want) {
			t.Errorf("the shared bar no longer carries the Inputs group: %q", want)
		}
	}
	// ...and Inputs comes before Outputs on it, the order the step happens in
	if i, j := strings.Index(main, "ctlRow.Append(a.inStack)"), strings.Index(main, "ctlRow.Append(a.outStack)"); i < 0 || j < 0 || i > j {
		t.Errorf("what a step reads is not left of what it wrote (%d, %d)", i, j)
	}
	// each page builds its line the one way and hands it over under its own
	// step name -- and none of them keeps a row of its own at the top
	same := []string{
		`= inputsLabel()`,
	}
	// publish.go is absent: since the merge it builds panes inside Produce's
	// page, and Produce's Inputs row is the one above them
	for _, f := range []string{"prep.go", "cut.go", "narrate.go", "produce.go"} {
		b, err := os.ReadFile(f)
		if err != nil {
			t.Fatal(err)
		}
		src := string(b)
		for _, want := range same {
			if !strings.Contains(src, want) {
				t.Errorf("%s's Inputs line is missing %s", f, want)
			}
		}
		name := strings.TrimSuffix(f, ".go")
		if !strings.Contains(src, `a.inStack.AddNamed(`) {
			t.Errorf("%s does not hand its Inputs line to the shared bar", f)
		}
		if !strings.Contains(src, `"`+name+`")`) {
			t.Errorf("%s registers nothing under its own step name", f)
		}
		if strings.Contains(src, `gtk.NewLabel("Inputs:")`) {
			t.Errorf("%s grew its own Inputs heading back beside the global one", f)
		}
		// and it is the page's own text that is dimmed nowhere: the heading
		// carries the weight, the reading itself is plain, as on Inputs
		if strings.Contains(src, `inputs.AddCSSClass("dim-label")`) {
			t.Errorf("%s dims its Inputs line where the other pages do not", f)
		}
		// The other half of the question -- what this step WROTE -- is not a
		// page row at all any more. Every step writes files, so the count and
		// the way into the folder ride the shared bottom bar and follow the
		// visible tab (outStack in main.go). A page owns only its group,
		// registered under its step name; a heading of its own would put a
		// second "Outputs:" on screen beside the global one.
		if !strings.Contains(src, `a.outStack.AddNamed(outRow, "`+name+`")`) {
			t.Errorf("%s does not hand its Outputs group to the shared bar", f)
		}
		if strings.Contains(src, `gtk.NewLabel("Outputs:")`) {
			t.Errorf("%s grew its own Outputs heading back beside the global one", f)
		}
	}
	// ...and the bar itself: one heading, the pages' groups behind it, switched
	// with the tabs so ▶, the progress text and the outputs are always about
	// the same step. Non-homogeneous, or prep's three folders would reserve
	// their width under every tab.
	b, err := os.ReadFile("main.go")
	if err != nil {
		t.Fatal(err)
	}
	mainSrc := string(b)
	for _, want := range []string{
		`outLbl := gtk.NewLabel("Outputs:")`,
		`outLbl.AddCSSClass("heading")`,
		`ctlRow.Append(outLbl)`,
		`ctlRow.Append(a.outStack)`,
		`a.outStack.SetVisibleChildName(name)`,
		`a.outStack.SetHhomogeneous(false)`,
	} {
		if !strings.Contains(mainSrc, want) {
			t.Errorf("the shared Outputs group is missing %s", want)
		}
	}
}

// A recording is a LANE before it is a transcript. Everything the lane is made
// of -- where it starts, how long it runs, what shape it is, what it sounds
// like -- is in the file, so the page opens on the footage alone: laying the
// session out, seeing where the takes fall against each other, shifting one by
// hand, hearing it, is work that comes before describing anything. The frames
// are the pictures drawn ON the lane, and the session timeline is text printed
// on it; without either the page is a lane with no pictures and no words, not
// a page that cannot be opened.
//
// What still waits for Prepare is the SUGGESTION, and it says so itself.
func TestCutOpensOnTheFootageAlone(t *testing.T) {
	root := t.TempDir()
	vid := filepath.Join(root, "capture.mp4")
	if err := os.WriteFile(vid, []byte("x"), 0o644); err != nil {
		t.Fatal(err)
	}
	a := &App{root: root, outDir: root}
	if a.canCut() {
		t.Error("the page opened with no footage at all")
	}
	a.selVid = []string{vid} // the snapshot a runner works from; no widgets here
	if !a.canCut() {
		t.Error("footage on the Prepare step did not open the page")
	}
	// no frames, no timeline, and it still opens: neither is what a lane is
	if exists(a.framesDir("capture")) || exists(filepath.Join(a.transcriptDir(), "session.tsv")) {
		t.Fatal("this session has been prepared after all -- the test proves nothing")
	}
	// ...and the suggestion is where the timeline is actually needed
	if !strings.Contains(funcBody(t, "cut_suggest.go", `func \(a \*App\) suggestClicked\(`), "run Describe first") {
		t.Error("nothing says where a session timeline is needed")
	}
	// the tab's own words say what it waits for, which is footage
	for _, s := range steps {
		if s.name == "cut" && !strings.Contains(s.wait, "Add footage") {
			t.Errorf("the Cut tab still says %q", s.wait)
		}
	}
	// a lane with no frames draws no pictures rather than dividing by their
	// spacing (thumbStep, frameRange)
	v := &tlVideo{base: "capture", path: vid, dur: 60}
	if step := v.thumbStep(48, 10); step < 1 {
		t.Errorf("a frameless lane steps %d frames at a time", step)
	}
	if first, last := v.frameRange(10, 0, 500, v.thumbStep(48, 10)); first != 0 || last != 0 {
		t.Errorf("a frameless lane painted frames %d..%d", first, last)
	}
}

// The pages are named after what they do; the folders are named after what they
// were. Both halves matter, and they pull in opposite directions.
//
// The files and the identifiers were numbered -- a numbering nobody but the tab
// row knew, which had to be looked up every time and which lied twice already,
// once when Narrate became the fourth step and again when Inputs and Describe
// merged into one page. So the code is called Cut, Narrate, Produce and Publish
// now, wherever it can be.
//
// The folders on disk are too, since migrateFolders: a project written under
// step1/ to step6/ is moved to the named folders on the open that finds it,
// which is what makes the rename safe for somebody's finished work.
func TestThePagesAreNamedForWhatTheyDoAndTheFoldersForWhatTheyWere(t *testing.T) {
	files, err := filepath.Glob("*.go")
	if err != nil {
		t.Fatal(err)
	}
	for _, f := range files {
		if regexp.MustCompile(`^step[0-9]`).MatchString(f) {
			t.Errorf("%s still carries a step number in its name", f)
		}
	}

	// one page file per tab, called what the tab is called -- publish.go
	// stays on disk as the thumbnail pane's source, but it is Produce's now
	for _, f := range []string{"prep.go", "cut.go", "narrate.go", "produce.go"} {
		if _, err := os.Stat(f); err != nil {
			t.Errorf("%s is missing -- one file per page, named for the page", f)
		}
	}
	if len(steps) != 4 {
		t.Errorf("%d tabs against 4 page files", len(steps))
	}

	// ...and no identifier is numbered either. A page whose builder is called
	// after its tab number is a page whose name has to be looked up in this
	// very table.
	for _, f := range files {
		b, err := os.ReadFile(f)
		if err != nil {
			t.Fatal(err)
		}
		// an identifier, not the word: stepN is still the name of a folder and
		// still the honest way to say "the fourth one" in a sentence
		for _, m := range regexp.MustCompile(
			`[a-z][A-Za-z0-9_]*Step[0-9][A-Za-z0-9_]*|\bstep[0-9][A-Za-z_][A-Za-z0-9_]*`).
			FindAllString(string(b), -1) {
			t.Errorf("%s still declares or names %s", f, m)
		}
	}

	// the folders, named for their steps, each reached through its one helper
	src := readSrc(t, "main.go")
	for _, want := range []string{
		`func (a *App) prepareDir() string    { return filepath.Join(a.outDir, "prepare") }`,
		`func (a *App) inputsDir() string     { return filepath.Join(a.prepareDir(), "inputs") }`,
		`func (a *App) describeDir() string   { return filepath.Join(a.prepareDir(), "describe") }`,
		`func (a *App) transcriptDir() string { return filepath.Join(a.prepareDir(), "transcript") }`,
		`func (a *App) narrateDir() string    { return filepath.Join(a.outDir, "narrate") }`,
		`func (a *App) produceDir() string    { return filepath.Join(a.outDir, "produce") }`,
	} {
		if !strings.Contains(src, want) {
			t.Errorf("a folder helper changed -- missing %s", want)
		}
	}
	if !strings.Contains(readSrc(t, "cut.go"), `filepath.Join(a.outDir, "cut")`) {
		t.Error("the cut folder is no longer cut/")
	}
	if !strings.Contains(readSrc(t, "publish.go"), `filepath.Join(a.outDir, "publish")`) {
		t.Error("the publish folder is no longer publish/")
	}
	// ...and nothing spells a folder out beside its helper: a literal is a path
	// the next rename misses
	for _, f := range files {
		if strings.HasSuffix(f, "_test.go") || f == "main.go" || f == "cut.go" || f == "publish.go" || f == "project.go" {
			continue
		}
		for _, lit := range []string{`"prepare"`, `"narrate"`, `"produce"`, `"publish"`} {
			if strings.Contains(readSrc(t, f), "filepath.Join(a.outDir, "+lit) {
				t.Errorf("%s joins a.outDir with %s itself instead of through the helper", f, lit)
			}
		}
	}
}

// A project written under the numbered folders is moved to the named ones on
// open -- once, and never over a folder that already has the new name.
func TestANumberedProjectIsMovedToTheNamedFolders(t *testing.T) {
	a := &App{outDir: t.TempDir()}
	for _, old := range []string{"step1", "step3", "step6"} {
		if err := os.MkdirAll(filepath.Join(a.outDir, old), 0o755); err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(filepath.Join(a.outDir, old, "x"), []byte(old), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	// a folder already under its new name stays what it is
	if err := os.MkdirAll(filepath.Join(a.outDir, "publish"), 0o755); err != nil {
		t.Fatal(err)
	}
	a.migrateFolders()
	for _, c := range []struct{ dir, want string }{
		{filepath.Join("prepare", "inputs"), "step1"}, // ...and on into prepare/, in the same pass
		{"cut", "step3"},
	} {
		b, err := os.ReadFile(filepath.Join(a.outDir, c.dir, "x"))
		if err != nil || string(b) != c.want {
			t.Errorf("%s/x = %q, %v -- want the file moved from %s/", c.dir, b, err, c.want)
		}
	}
	for _, old := range []string{"step1", "step3"} {
		if _, err := os.Stat(filepath.Join(a.outDir, old)); !os.IsNotExist(err) {
			t.Errorf("%s/ is still there after the move", old)
		}
	}
	if _, err := os.Stat(filepath.Join(a.outDir, "step6", "x")); err != nil {
		t.Errorf("step6/ was moved over an existing publish/: %v", err)
	}
	if _, err := os.Stat(filepath.Join(a.outDir, "narrate")); !os.IsNotExist(err) {
		t.Error("a folder that never existed under the old name was created")
	}
	a.migrateFolders() // a second open changes nothing
	if b, _ := os.ReadFile(filepath.Join(a.outDir, "prepare", "inputs", "x")); string(b) != "step1" {
		t.Error("the second migration disturbed the first")
	}
}

// The second rename there has been: Prepare's three folders under one name.
//
// inputs/ sat beside understand/, and understand/ held describe/ and
// transcript/ -- one step in two places on disk, and three output buttons on
// its page where every other step has one. They are prepare/'s three
// subfolders now, moved on the open that finds them.
func TestPreparesFoldersAreMovedUnderItsOwn(t *testing.T) {
	a := &App{outDir: t.TempDir()}
	for _, old := range []string{"inputs", filepath.Join("understand", "describe"),
		filepath.Join("understand", "transcript")} {
		if err := os.MkdirAll(filepath.Join(a.outDir, old), 0o755); err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(filepath.Join(a.outDir, old, "x"), []byte(old), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	a.migrateFolders()

	for _, c := range []struct{ dir, want string }{
		{"inputs", "inputs"},
		{"describe", filepath.Join("understand", "describe")},
		{"transcript", filepath.Join("understand", "transcript")},
	} {
		b, err := os.ReadFile(filepath.Join(a.outDir, "prepare", c.dir, "x"))
		if err != nil || string(b) != c.want {
			t.Errorf("prepare/%s/x = %q, %v -- want the file moved from %s/", c.dir, b, err, c.want)
		}
	}
	// the folder the two came out of goes with them: an empty understand/
	// beside the named ones is a step somebody goes looking for
	if _, err := os.Stat(filepath.Join(a.outDir, "understand")); !os.IsNotExist(err) {
		t.Errorf("understand/ is still there: %v", err)
	}
	// and the paths the app uses are the ones it moved them to
	if got := a.inputsDir(); got != filepath.Join(a.outDir, "prepare", "inputs") {
		t.Errorf("inputsDir is %q", got)
	}
	// twice is once: a second open finds them already there and leaves them
	a.migrateFolders()
	if b, _ := os.ReadFile(filepath.Join(a.outDir, "prepare", "describe", "x")); string(b) == "" {
		t.Error("the second open moved the folders again")
	}
	// ...and a project with nothing to move gets no folders it never had
	fresh := &App{outDir: t.TempDir()}
	fresh.migrateFolders()
	if _, err := os.Stat(filepath.Join(fresh.outDir, "prepare")); !os.IsNotExist(err) {
		t.Error("a project with no work in it was given a prepare/ folder to be empty in")
	}
}

// One set of numbers for the space around a page's work, on all four of them.
//
// They had drifted: 10 px under the tabs on Cut and Produce, 4 on the
// thumbnail's column and none at all on Prepare and Narrate; 12 either side of
// Prepare's handle, 12 and nothing either side of Narrate's, 6 and 6 on
// Produce's. Two columns of the same app started at two different x, which is
// the kind of difference you feel without being able to name.
//
// The rule: 12 at the window's edges, 6 either side of a handle -- so two
// columns stand 12 apart, the same as the edges -- and 8 above and below the
// work.
func TestEveryPageKeepsTheSameMarginsAroundItsWork(t *testing.T) {
	for _, c := range []struct {
		file, fn string
		want     []string
	}{
		{"prep.go", `func \(a \*App\) buildPrep\(`, []string{
			"outer.SetMarginStart(12)", "outer.SetMarginEnd(12)",
			"outer.SetMarginTop(8)", "outer.SetMarginBottom(8)",
			"gtk.BaseWidget(bench).SetMarginStart(6)", "sources.SetMarginEnd(6)",
		}},
		{"cut.go", `func \(a \*App\) buildCut\(`, []string{
			"vframe.SetMarginTop(8)", "vframe.SetMarginStart(12)", "vframe.SetMarginEnd(6)",
		}},
		{"cut_form.go", `func \(ed \*cutEditor\) buildForm\(\) \*gtk.Box \{`, []string{
			"col.SetMarginTop(8)", "col.SetMarginBottom(8)",
			"col.SetMarginStart(6)", "col.SetMarginEnd(12)",
		}},
		{"narrate.go", `func \(a \*App\) buildNarrate\(`, []string{
			"shown.SetMarginStart(12)", "shown.SetMarginEnd(6)",
			"shown.SetMarginTop(8)", "shown.SetMarginBottom(8)",
			"margins(written, 8, 8, 6, 12)",
		}},
		{"publish.go", `func \(a \*App\) buildPublishPanes\(`, []string{
			"col.SetMarginStart(12)", "col.SetMarginEnd(6)", "col.SetMarginTop(8)",
		}},
		// ...and the other half of that page is one column carrying the
		// margins for both its rows: the words and the encoder settings under
		// them had a pair each, so one ended 24 from the window and the other 12
		{"produce.go", `func \(a \*App\) buildProduce\(`, []string{
			"margins(right, 8, 8, 6, 12)",
		}},
	} {
		body := funcBody(t, c.file, c.fn)
		for _, want := range c.want {
			if !strings.Contains(body, want) {
				t.Errorf("%s: %q — the page's margins are its own again", c.file, want)
			}
		}
	}
	// ...and a column inside one of those halves does not add margins of its
	// own on top: Produce's settings grid was indented 12 further than the
	// title and the description directly above it
	prod := funcBody(t, "produce.go", `func \(a \*App\) buildProduce\(`)
	for _, gone := range []string{"box.SetMargin", "outer.SetMargin"} {
		if strings.Contains(prod, gone) {
			t.Errorf("a row inside the column indents itself past its neighbours: %q", gone)
		}
	}
	if strings.Contains(readSrc(t, "publish.go"), "wrote.SetMargin") {
		t.Error("the words carry margins of their own again, so the column has two right edges")
	}
}
