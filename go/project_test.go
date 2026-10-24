package main

import (
	"encoding/json"
	"os"
	"path/filepath"
	"reflect"
	"regexp"
	"strings"
	"testing"
	"time"
)

// A project stores its input and output folders relative to root so that moving
// the naivepost directory moves them too. The round trip has to be exact: a
// folder that comes back wrong points the whole pipeline at the wrong files,
// and nothing about that failure says "the project file lied".
func TestFolderRoundTripsThroughRoot(t *testing.T) {
	root := t.TempDir()
	a := &App{root: root}

	for _, dir := range []string{
		root,
		filepath.Join(root, "sessions", "gorilla"),
		filepath.Join(filepath.Dir(root), "elsewhere"),
		"/mnt/recordings",
	} {
		stored := a.relToRoot(dir)
		if got := a.loadPath(stored); got != dir {
			t.Errorf("%s stored as %q came back as %s", dir, stored, got)
		}
	}

	inside := filepath.Join(root, "sessions")
	if got := a.relToRoot(inside); filepath.IsAbs(got) {
		t.Errorf("a folder under root should be stored relative, got %s", got)
	}
	outside := "/mnt/recordings"
	if got := a.relToRoot(outside); got != outside {
		t.Errorf("a folder outside root has nothing to be relative to, got %s", got)
	}
}

// An absent in_dir/out_dir means the root -- that is what every project written
// before the folders were settable looks like.
func TestEmptyFolderMeansRoot(t *testing.T) {
	a := &App{root: "/home/x/naivepost"}
	if got := a.loadPath(""); got != a.root {
		t.Fatalf("empty folder resolved to %s, want the root", got)
	}
}

// Prepare's outputs are frames in per-recording subfolders, so the count has to
// be recursive: a top-level count would report "2 files" about a few thousand.
func TestOutputSummaryCountsEveryFrame(t *testing.T) {
	dir := t.TempDir()
	if got := summarizeOutputs(dir); got != "nothing yet" {
		t.Fatalf("empty folder summarized as %q", got)
	}

	os.MkdirAll(filepath.Join(dir, "frames", "session1"), 0o755)
	os.WriteFile(filepath.Join(dir, "meta.env"), []byte("x=1\n"), 0o644)
	for _, n := range []string{"f0001.jpg", "f0002.jpg", "f0003.jpg"} {
		os.WriteFile(filepath.Join(dir, "frames", "session1", n), []byte("x"), 0o644)
	}
	got := summarizeOutputs(dir)
	if !strings.HasPrefix(got, "4 files") {
		t.Errorf("summary = %q, want it to start with 4 files", got)
	}
	// ...and how big they are, which with the count is the whole of the line
	if !strings.Contains(got, " B") {
		t.Errorf("summary = %q, want the size beside the count", got)
	}
	if strings.Contains(got, "ago") || strings.Contains(got, "just now") {
		t.Errorf("summary = %q — the age is the tooltip's", got)
	}
}

// "Add folder" refuses a folder with nothing playable in it, rather than
// reporting that it added nothing: picking the wrong folder is the likely
// reason, and a silent no-op does not say which folder was looked at. This is
// the chooser's guard; listMedia is what it asks.
func TestOnlyAFolderWithMediaIsWorthSwitchingTo(t *testing.T) {
	full := t.TempDir()
	for _, n := range []string{"b.mkv", "a.flac", "notes.txt", "cover.png"} {
		os.WriteFile(filepath.Join(full, n), []byte("x"), 0o644)
	}
	os.MkdirAll(filepath.Join(full, "sub"), 0o755) // a folder is not a source

	if got := listMedia(full); len(got) != 2 || got[0] != "a.flac" || got[1] != "b.mkv" {
		t.Errorf("listMedia = %v, want the two media files sorted by name", got)
	}
	// both ways a pick can be worthless, and both must read the same to the
	// chooser: nothing to switch to
	empty := t.TempDir()
	os.WriteFile(filepath.Join(empty, "readme.md"), []byte("x"), 0o644)
	for _, c := range []struct{ name, dir string }{
		{"a folder with no media in it", empty},
		{"a folder that does not exist", filepath.Join(empty, "nope")},
	} {
		if got := listMedia(c.dir); len(got) != 0 {
			t.Errorf("%s: listMedia = %v, want none", c.name, got)
		}
	}
}

// The two folders are only where the choosers open now, but they are still what
// a pre-merge project's half-relative source names are resolved against -- so
// they have to come out of an old project pointing where they used to. The one
// parent an older project named had to hold input_video/ and input_audio/.
func TestOldProjectsKeepTheirSourceFolders(t *testing.T) {
	for _, c := range []struct {
		name         string
		p            Project
		wantV, wantA string
	}{
		{"a project written since the split",
			Project{VidDir: "footage", AudDir: "/mnt/rec"}, "footage", "/mnt/rec"},
		{"one written before it",
			Project{InDir: "/mnt/session"}, "/mnt/session/input_video", "/mnt/session/input_audio"},
		{"...whose input folder was the root itself",
			Project{}, "input_video", "input_audio"},
		{"one half-written by a version in between",
			Project{InDir: "/mnt/session", AudDir: "/mnt/rec"}, "/mnt/session/input_video", "/mnt/rec"},
	} {
		if v, a := srcDirs(c.p); v != c.wantV || a != c.wantA {
			t.Errorf("%s: srcDirs = (%q, %q), want (%q, %q)", c.name, v, a, c.wantV, c.wantA)
		}
	}

	// and an empty in_dir must resolve to the root's two subfolders, not to the
	// root twice -- that is the case that would silently list nothing
	a := &App{root: "/home/x/naivepost"}
	v, _ := srcDirs(Project{})
	if got, want := a.loadPath(v), "/home/x/naivepost/input_video"; got != want {
		t.Fatalf("a project with no folders at all opens on %s, want %s", got, want)
	}
}

// Sources are stored relative to root where they can be, so a project folder
// that moves keeps working; anything outside it is stored as it is. The roles
// travel with them: a project that came back with the footage flags or the
// narrator tags lost is a session that renders the wrong frames in the wrong
// voice, and nothing about that says "the project file lied".
func TestSourcesRoundTripWithTheirRoles(t *testing.T) {
	a := &App{root: "/home/x/naivepost"}
	items := []sourceItem{
		{path: "/home/x/naivepost/input_video/clip.mkv", footage: true},
		{path: "/mnt/recordings/voice.flac", narrator: 1},
		{path: "/mnt/recordings/mate.flac", narrator: 3},
		{path: "/home/x/naivepost/input_video/cam2.mp4", footage: true, narrator: 2},
	}
	var stored []ProjectSource
	for _, it := range items {
		stored = append(stored, ProjectSource{
			Path: a.relToRoot(it.path), Footage: it.footage, Narrator: it.narrator})
	}
	if got, want := stored[0].Path, "input_video/clip.mkv"; got != want {
		t.Errorf("a source under root is stored as %q, want %q", got, want)
	}
	if got, want := stored[1].Path, "/mnt/recordings/voice.flac"; got != want {
		t.Errorf("a source outside root has nothing to be relative to, stored as %q", got)
	}
	back := a.projectSources(Project{Sources: stored})
	if len(back) != len(items) {
		t.Fatalf("%d sources went in, %d came back", len(items), len(back))
	}
	for i, it := range items {
		if !sameSource(back[i], it) {
			t.Errorf("source %d came back as %+v, want %+v", i, back[i], it)
		}
	}
}

// A project written before the two lists became one still has to open on the
// same files, doing the same things. Its videos were the footage; its first
// recording was what "my own voice" cloned -- a convention nothing stated --
// and that is now the narrator 1 tag, which is the same choice said out loud.
func TestOldProjectsBecomeSourcesWithTheirRoles(t *testing.T) {
	root := t.TempDir()
	vid, aud := filepath.Join(root, "input_video"), filepath.Join(root, "rec")
	for _, d := range []string{vid, aud} {
		if err := os.MkdirAll(d, 0o755); err != nil {
			t.Fatal(err)
		}
	}
	// the legacy names, and the file each one has to resolve to
	for _, n := range []string{filepath.Join(vid, "clip.mkv"),
		filepath.Join(aud, "me.flac"), filepath.Join(aud, "mate.flac")} {
		if err := os.WriteFile(n, []byte("x"), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	a := &App{root: root, vidDir: vid, audDir: aud}
	got := a.projectSources(Project{
		Videos: []string{"input_video/clip.mkv"},
		// stored when the recordings folder was called something else, so it
		// resolves through the folder rather than through root
		Audios: []string{"input_audio/me.flac", "input_audio/mate.flac"},
	})
	want := []sourceItem{
		{path: filepath.Join(vid, "clip.mkv"), footage: true},
		{path: filepath.Join(aud, "me.flac"), narrator: 1},
		{path: filepath.Join(aud, "mate.flac")},
	}
	if len(got) != len(want) {
		t.Fatalf("migrated to %+v, want %d sources", got, len(want))
	}
	for i := range want {
		if !sameSource(got[i], want[i]) {
			t.Errorf("source %d migrated to %+v, want %+v", i, got[i], want[i])
		}
	}
}

// ---- autosave ---------------------------------------------------------------

// Where a save lands, and where the work lands. One file -- the open one -- and
// one folder, derived from that file's own name, so a project and everything it
// wrote can only be moved together. Two failures behind this: edits after a
// Save As going only into a working copy, leaving the named file a session
// behind, and a session writing into a folder that belonged to a project nobody
// had open any more.
func TestAProjectOwnsTheFolderBesideIt(t *testing.T) {
	const proj = "/mnt/rec/tom.json.naivepost"
	// the project IS the folder now: one thing to copy, back up or zip
	if got := dataDir(proj); got != proj {
		t.Errorf("%s writes into %s, want the project's own folder", proj, got)
	}
	if got, want := projFile(proj), proj+"/naivepost.json"; got != want {
		t.Errorf("the project's file is %s, want %s", got, want)
	}
	// Save takes whatever was typed into the name box. The extension is what
	// the desktop opens with Naivepost and what .data hangs off, so it is added
	// rather than hoped for -- and capitals the user typed are theirs to keep.
	for _, c := range []struct{ in, want string }{
		{"/mnt/rec/tom", "/mnt/rec/tom" + projExt},
		{"/mnt/rec/tom.json", "/mnt/rec/tom.json" + projExt},
		{"/mnt/rec/tom" + projExt, "/mnt/rec/tom" + projExt},
		{"/mnt/rec/tom.NAIVEPOST", "/mnt/rec/tom.NAIVEPOST"},
	} {
		if got := withProjExt(c.in); got != c.want {
			t.Errorf("saving %q writes %q, want %q", c.in, got, c.want)
		}
	}
	// the session nobody has named is a file like any other: same rule, no
	// unsaved special case, and the desktop can open it too
	if !strings.HasSuffix(workName, projExt) {
		t.Errorf("the working copy is %q, which is not a file the desktop would open", workName)
	}
	// and the launch hands the session that file before anything can be saved
	// into it, which is what makes "not saved yet" not a case anywhere else
	main := funcBody(t, "main.go", `func main\(\)`)
	for _, want := range []string{"a.projPath = filepath.Join(wd, workName)", "a.outDir = dataDir(a.projPath)"} {
		if !strings.Contains(main, want) {
			t.Errorf("a session starts without %s, so it starts with no file of its own", want)
		}
	}
	// and a save goes to that one file. It used to go to two -- the named file
	// and a working copy -- which is how edits after a Save As reached only the
	// copy.
	body := funcBody(t, "project.go", `func \(a \*App\) saveProjectNow\(`)
	if !strings.Contains(body, "a.writeProject(a.projPath, b)") {
		t.Errorf("a save no longer writes the open project file:\n%s", body)
	}
}

// Save As moves the work. The folder is derived from the name, so renaming the
// project without renaming the folder leaves the transcripts, frames and
// renders filed under the name it used to have -- and every page of the project
// that is open reads as one that has never been run.
//
// The move is a rename and nothing else: it is instant, it cannot half-happen,
// and when it will not go through the files stay exactly where they are. Nobody
// presses Save expecting gigabytes of frames to be copied, and nobody presses
// it expecting them to be lost either.
func TestSavingUnderANewNameTakesTheWorkWithIt(t *testing.T) {
	root := t.TempDir()
	a := &App{root: root}
	from, to := filepath.Join(root, "a"+projExt+".data"), filepath.Join(root, "b"+projExt+".data")

	// nothing written yet: nothing to move, and no empty folder invented
	a.moveOutputs(from, to)
	if exists(to) {
		t.Errorf("%s was created for a project that had written nothing", to)
	}
	// an empty folder is not work either -- it is what a project that has been
	// opened and not run leaves behind, and moving it says something ran
	if err := os.MkdirAll(from, 0o755); err != nil {
		t.Fatal(err)
	}
	a.moveOutputs(from, to)
	if exists(to) || !exists(from) {
		t.Errorf("an empty %s was moved to %s", from, to)
	}

	if err := os.MkdirAll(filepath.Join(from, "inputs"), 0o755); err != nil {
		t.Fatal(err)
	}
	frame := filepath.Join(from, "inputs", "0001.jpg")
	if err := os.WriteFile(frame, []byte("x"), 0o644); err != nil {
		t.Fatal(err)
	}
	a.moveOutputs(from, to)
	if !exists(filepath.Join(to, "inputs", "0001.jpg")) {
		t.Errorf("the work did not follow the project to %s", to)
	}
	if exists(from) {
		t.Errorf("%s is still there -- the work was copied, not moved", from)
	}

	// the new name already has work of its own: the rename fails on it, so
	// nothing is written over and nothing is half-merged in
	if err := os.MkdirAll(filepath.Dir(frame), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(frame, []byte("x"), 0o644); err != nil {
		t.Fatal(err)
	}
	a.moveOutputs(from, to)
	if !exists(frame) {
		t.Errorf("%s was moved into a folder that was already in use", frame)
	}

	// and the move happens before the project is pointed at the new folder --
	// the other way round moves a folder the pages have already been drawn from
	body := funcBody(t, "project.go", `func \(a \*App\) saveProjectTo\(`)
	move, set := strings.Index(body, "a.moveOutputs("), strings.Index(body, "a.setProject(")
	if move < 0 || set < 0 || move > set {
		t.Errorf("Save As no longer moves the outputs before renaming the project:\n%s", body)
	}
	// the extension is forced here, or a project saved as "tom" and one saved
	// as "tom.naivepost" are two projects with neither name nor folder in common
	if !strings.Contains(body, "withProjExt(path)") {
		t.Errorf("Save takes the typed name as it is:\n%s", body)
	}
	// and a plain Save over the open file is not a move: it would warn about a
	// folder already being in use -- its own
	if !strings.Contains(body, "if path != a.projPath {") {
		t.Errorf("saving the open project under its own name still moves its folder:\n%s", body)
	}
}

// setProject is where the two halves of a project meet: the file the autosave
// follows, and the folder every step writes into. Everything on screen that was
// read out of the LAST project's folder has to be re-read here, or the pages go
// on showing another project's work -- the narration that "was never saved" was
// exactly this, a page built once at startup against the empty session.
func TestPointingAtAProjectRedrawsWhatItsFolderHolds(t *testing.T) {
	body := funcBody(t, "project.go", `func \(a \*App\) setProject\(`)
	if !strings.Contains(body, "a.outDir = dataDir(path)") {
		t.Errorf("the folder is no longer derived from the file:\n%s", body)
	}
	for _, want := range []string{
		"a.showProject()",  // the header bar names the file
		"a.followOutDir()", // and the render target follows it
		`a.voiceSel = ""`,  // the voice is the project's, not the session's
		// ...and so are the seconds it is cloned from, which are read once and
		// held: without this the next project shows this one's picks under the
		// same recording name (narrate_take.go)
		"a.takesRead, a.takesMap = false, nil",
		"a.narr.load()",    // the narration lives in the folder
		"a.prep.refresh()", // and so do the counts on every page
		"a.updateProduceInfo()",
		"a.pub.refresh()",
		"a.refreshCut()", // the Cut page's tracks most of all
		"a.updateGates()",
	} {
		if !strings.Contains(body, want) {
			t.Errorf("setProject no longer does %s, so that part keeps the last project's folder", want)
		}
	}
}

// The autosave writes bytes to a path and notices when they stop matching what
// it last wrote. Those two are all the ticker is; currentProject needs a built
// window, so this covers the half that can be tested without one.
func TestTheAutosaveWritesAndThenKnowsItIsUpToDate(t *testing.T) {
	root := t.TempDir()
	a := &App{root: root}
	p := filepath.Join(root, "tom"+projExt)
	if err := a.writeProject(p, []byte("{}\n")); err != nil {
		t.Fatal(err)
	}
	// the folder is made by the write: a project nobody has saved is a folder
	// that does not exist yet, and the autosave is what brings it into being
	b, err := os.ReadFile(projFile(p))
	if err != nil || string(b) != "{}\n" {
		t.Fatalf("read back %q, %v", b, err)
	}
	// a write into a place that cannot hold a folder fails rather than
	// panicking: the ticker calls this every couple of seconds, and a project
	// on a removed drive must cost one log line, not the session
	if err := a.writeProject(filepath.Join(projFile(p), "under-a-file"), b); err == nil {
		t.Error("writing into a path that is a file reported success")
	}
}

// The ticker is a poll, so there is always a window of up to autosaveTick in
// which a change exists only in the widgets -- and the change most likely to
// sit in that window is the last one made, because making it is what tells the
// user they are done. Closing the window has to write, or the file keeps a
// state the user already moved past: a narrator tag taken off, seen taken off,
// and back the next morning because the tick that would have written it never
// came. currentProject needs a built window, so the flush itself cannot run
// here; what this holds is that both the tick and the close go through it.
func TestClosingTheWindowWritesWhatTheTickHasNotSeen(t *testing.T) {
	src, err := os.ReadFile("project.go")
	if err != nil {
		t.Fatal(err)
	}
	// ...and everything the close writes is inside the one handler
	close := funcBody(t, "project.go", `func \(a \*App\) startAutosave\(\)`)
	close = close[strings.Index(close, "ConnectCloseRequest"):]
	for _, want := range []string{"a.narr.flushSave()", "a.flushProject()", "a.flushPrompts()"} {
		if !strings.Contains(close, want) {
			t.Errorf("closing the window no longer runs %s", want)
		}
	}
	for _, want := range []string{
		"glib.TimeoutAdd(autosaveTick, func() bool {\n\t\ta.flushProject()",
		"a.win.ConnectCloseRequest(func() bool {",
		"a.narr.flushSave() // ...and the same for the last line typed",
		"return false // false lets the window close; this only writes",
	} {
		if !strings.Contains(string(src), want) {
			t.Errorf("project.go no longer holds:\n%s", want)
		}
	}
}

// Narrate writes narrate/ and nothing else, and no test writes into the live
// session. Both halves of "there is a produce/ folder and I never opened
// Produce": the renumbering that made Narrate the fourth step left the old name in
// places that read either way, and the render smoke test rendered a smoke.mp4
// straight into the user's own out/test/step5 on every `go test ./...`,
// clearing its clips on the way past.
//
// The folder is produce/ now, and Produce is the ordinary name of a great many
// things, so what is banned is the folder's helper and the literal path.
func TestOnlyProduceWritesTheProduceFolder(t *testing.T) {
	files, err := filepath.Glob("*.go")
	if err != nil {
		t.Fatal(err)
	}
	for _, f := range files {
		// main.go defines it; produce.go is the step that owns it, and
		// publish.go is the other half of that step -- the thumbnail and the
		// upload text are written under produce/ with the video, so the page
		// has one folder to open and one to count (publishDir)
		if f == "produce.go" || f == "main.go" || f == "publish.go" || strings.HasSuffix(f, "_test.go") {
			continue
		}
		b, err := os.ReadFile(f)
		if err != nil {
			t.Fatal(err)
		}
		if strings.Contains(string(b), "produceDir()") {
			t.Errorf("%s names produceDir() -- a produce/ folder now appears for a user who never opened Produce", f)
		}
		if strings.Contains(string(b), `filepath.Join(a.outDir, "produce")`) {
			t.Errorf("%s names the produce folder literally", f)
		}
	}
	// ...the narration's own files are all under step4...
	b, err := os.ReadFile("narrate.go")
	if err != nil {
		t.Fatal(err)
	}
	if strings.Contains(string(b), `filepath.Join(a.outDir, "produce")`) {
		t.Error("narrate.go still writes into produce/ -- the narration used to go there")
	}
	// ...and the one test that renders for real reads the session but writes
	// into its own folder. A test that leaves files in someone's project is a
	// bug report from a user who did nothing wrong.
	b, err = os.ReadFile("produce_render_test.go")
	if err != nil {
		t.Fatal(err)
	}
	if n := strings.Count(string(b), "\tredirectOutput(t, a"); n < 2 {
		t.Errorf("%d of the render tests redirect their output -- the rest write into the live out/test", n)
	}
}

// ---- the project name in the header bar --------------------------------------

// What the title bar can say about the open project: the short form, the long
// one, and the hover behind either. The short form is the file name, because
// variants of a session live beside each other in one folder and the leading
// directories are the identical half. The unnamed case has to say something
// too -- a blank space where a file name goes reads as a bug, not as "nothing
// has been saved yet" -- and it has no path to offer, so both forms are the
// same words and the bar may show whichever it has room for.
func TestTheHeaderBarNamesTheOpenProject(t *testing.T) {
	name, full, tip := projLabelText("/home/tom/cuts/before-the-recut.json")
	if name != "before-the-recut.json" {
		t.Errorf("the bar shows %q, want the file name alone", name)
	}
	if full != "/home/tom/cuts/before-the-recut.json" {
		t.Errorf("the long form is %q, want the whole path", full)
	}
	if tip != "/home/tom/cuts/before-the-recut.json" {
		t.Errorf("the tooltip shows %q, want the whole path -- it answers whichever form is on the bar", tip)
	}
	name, full, tip = projLabelText("  ")
	if !strings.Contains(name, "no project") {
		t.Errorf("with nothing saved the bar shows %q, want it to say so in words", name)
	}
	if full != name {
		t.Errorf("with nothing saved the long form is %q and the short one %q -- a bar with "+
			"room to spare would go blank on the difference", full, name)
	}
	if tip == "" {
		t.Error("with nothing saved the tooltip is empty -- hovering must still answer")
	}
}

// The wiring: the label is ellipsized (a project buried in a long name must not
// walk the centered tabs sideways) and both places that assign projPath refresh
// it. The second half is the one that rots quietly -- a Save As that renames
// the file the autosave follows while the bar keeps showing the old name is
// worse than showing no name at all. The cap is fitHeader's now, since it is
// the backstop for one of that ladder's rungs and not for the other.
func TestTheProjectNameFollowsSaveAndLoad(t *testing.T) {
	src, err := os.ReadFile("main.go")
	if err != nil {
		t.Fatal(err)
	}
	for _, want := range []string{
		"a.projLabel.SetEllipsize(pango.EllipsizeEnd)",
		"head.PackStart(a.projLabel)",
	} {
		if !strings.Contains(string(src), want) {
			t.Errorf("the header bar no longer does %s", want)
		}
	}
	p, err := os.ReadFile("project.go")
	if err != nil {
		t.Fatal(err)
	}
	// Both of the two that rename a project go through setProject, and that is
	// what tells the bar -- one place, so the name on screen cannot be the file
	// the autosave stopped following.
	for _, fn := range []string{"saveProjectTo", "loadProjectFrom"} {
		body := regexp.MustCompile(`(?s)func \(a \*App\) ` + fn + `\(path string\) \{.*?\n}\n`).Find(p)
		if body == nil {
			t.Fatalf("%s is gone", fn)
		}
		if !strings.Contains(string(body), "a.setProject(") {
			t.Errorf("%s renames the project without going through setProject", fn)
		}
	}
	if !strings.Contains(string(regexp.MustCompile(`(?s)func \(a \*App\) setProject\(path string\) \{.*?\n}\n`).Find(p)),
		"a.showProject()") {
		t.Error("setProject renames the project without telling the header bar")
	}
	// showProject is also where a rename gets re-priced: a Save As from
	// short.json to a path three folders deep may no longer fit as a path.
	body := regexp.MustCompile(`(?s)func \(a \*App\) showProject\(\) \{.*?\n}\n`).Find(p)
	if body == nil {
		t.Fatal("showProject is gone")
	}
	if !strings.Contains(string(body), "a.fitHeader()") {
		t.Error("showProject names the project without re-fitting the bar to it")
	}
}

// ---- the language, and a project that starts empty --------------------------

// The language is the project's, not the machine's. It used to be a line in
// llm.conf, which meant one language for every session that machine ever cut:
// the German session after the English one came back as gibberish, from a box
// three tabs away that nobody had reason to open. What this pins is the whole
// path -- what a runner reads, what the file stores, and that the setting is
// really gone from the config rather than quietly written in both places.
func TestTheLanguageBelongsToTheProject(t *testing.T) {
	ownConfig(t)
	a := &App{root: t.TempDir()}

	// nothing typed is the default, not an empty language field posted to a
	// server that would then transcribe into whatever it guesses
	if got := a.asrLanguage(); got != defLanguage {
		t.Errorf("an untouched session asks for %q, want %q", got, defLanguage)
	}
	if got := a.projectLanguage(); got != "" {
		t.Errorf("an untouched session STORES %q -- an unset box must not freeze today's default into the file", got)
	}

	// what the box says is what the request carries, whitespace and all removed
	a.setLanguage("  de \n")
	if got := a.asrLanguage(); got != "de" {
		t.Errorf("the run would ask for %q, want de", got)
	}
	if got := a.projectLanguage(); got != "de" {
		t.Errorf("the project would store %q, want de", got)
	}
	// ...and clearing it comes back to the default rather than to ""
	a.setLanguage("")
	if got := a.asrLanguage(); got != defLanguage {
		t.Errorf("a cleared box asks for %q, want the default", got)
	}
	// applyLanguage runs at load time, before any window exists in the tests
	a.applyLanguage("fr")
	if got := a.asrLanguage(); got != "fr" {
		t.Errorf("a loaded project's language came out as %q", got)
	}

	// through the file: stored when set, absent when not
	b, err := json.Marshal(Project{Language: "de"})
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(b), `"language":"de"`) {
		t.Errorf("a project with a language wrote %s", b)
	}
	var back Project
	if err := json.Unmarshal(b, &back); err != nil || back.Language != "de" {
		t.Errorf("the language came back as %q (%v)", back.Language, err)
	}
	if b, err = json.Marshal(Project{}); err != nil || strings.Contains(string(b), "language") {
		t.Errorf("a project with no language wrote %s (%v)", b, err)
	}

	// and it is out of the config: a value left in llm.conf would be a second
	// place to set it, disagreeing with the page silently
	if err := a.writeConf(appConf{Server: "https://x"}); err != nil {
		t.Fatal(err)
	}
	conf, err := os.ReadFile(confPath())
	if err != nil {
		t.Fatal(err)
	}
	if strings.Contains(string(conf), "AUDIOCPP_LANGUAGE") {
		t.Errorf("llm.conf still carries the language:\n%s", conf)
	}
}

// New Project hands applyProject a blank one, so "blank" has to mean the
// program's defaults and not Go's zero values. Two of them are the whole reason
// this function exists: interval 0 is not "unset", it is EVERY frame, which is
// gigabytes of jpegs nobody asked for; and a zeroed produce block renders at
// CRF 0 and 0 fps.
func TestANewProjectStartsFromTheDefaultsNotFromZero(t *testing.T) {
	p := blankProject()
	if p.Interval != frameStops[defFrameStop] || p.Interval == 0 {
		t.Errorf("a new project extracts a frame every %gs, want %gs", p.Interval, frameStops[defFrameStop])
	}
	if p.FrameScale != scalePresets[0].Name {
		t.Errorf("a new project's frame size is %q, want %q", p.FrameScale, scalePresets[0].Name)
	}
	if p.Produce == nil {
		t.Fatal("a new project has no produce settings -- the page would load zeroes")
	}
	// reflect, not ==: the settings carry a list now (SubLangs), and a struct
	// holding one cannot be compared. A new project's list is empty -- one
	// subtitle track, in the session's own language, translated into nothing.
	if !reflect.DeepEqual(*p.Produce, defaultProdSettings()) {
		t.Errorf("a new project renders with %+v, want %+v", *p.Produce, defaultProdSettings())
	}
	// everything else is legitimately empty: a new project is an empty session
	if len(p.Sources) > 0 || p.Context != "" || len(p.Prompts) > 0 ||
		p.Publish != nil || p.OutDir != "" || p.Language != "" {
		t.Errorf("a new project came out carrying something: %+v", p)
	}
}

// The reset has to go through applyProject rather than clearing what somebody
// remembered to clear: a page left out is last session's thumbnail, or its
// narration settings, sitting in a project that says it is new -- and nothing
// on screen says so. Pinned by reading the source, because applyProject touches
// widgets and there is no window here.
func TestNewProjectResetsEveryPageThroughApplyProject(t *testing.T) {
	ownConfig(t)
	src, err := os.ReadFile("project.go")
	if err != nil {
		t.Fatal(err)
	}
	body := regexp.MustCompile(`(?s)func \(a \*App\) newProject\(path string\) \{.*?\n}\n`).Find(src)
	if body == nil {
		t.Fatal("newProject is gone")
	}
	if !strings.Contains(string(body), "a.applyProject(blankProject())") {
		t.Errorf("newProject resets by hand instead of through applyProject:\n%s", body)
	}
	// the one thing it must NOT do is delete or overwrite the named project the
	// session was in: New is not a destructive file operation, it is a session
	// that stops being that file
	for _, no := range []string{"os.Remove", "a.writeProject("} {
		if strings.Contains(string(body), no) {
			t.Errorf("newProject calls %s -- it must leave the named project file alone:\n%s", no, body)
		}
	}
	// every page's applier belongs to the load path, so a new one added later is
	// in the reset by construction
	load := regexp.MustCompile(`(?s)func \(a \*App\) applyProject\(p Project\) \{.*?\n}\n`).Find(src)
	if load == nil {
		t.Fatal("applyProject is gone")
	}
	for _, want := range []string{"a.srcList.load(", "a.setFrameInterval(", "a.applyLanguage(",
		"a.adoptProjectPrompts(", "a.applySessionCtx(", "a.applyProdSettings(", "a.applyPublish("} {
		if !strings.Contains(string(load), want) {
			t.Errorf("applyProject no longer calls %s -- New Project would leave that page as it was", want)
		}
	}
	// The output folder is the one thing it must NOT take from the file: it is
	// derived from the project's own name, and a stored folder is a second
	// answer that can disagree with the first -- which is how a session ends up
	// writing where nobody is looking.
	if strings.Contains(string(load), "p.OutDir") {
		t.Errorf("applyProject reads the stored output folder instead of deriving it:\n%s", load)
	}
	// and both callers point the session at its file, which is what sets the
	// folder and redraws the pages read out of it
	for _, fn := range []string{"newProject", "loadProjectFrom"} {
		body := funcBody(t, "project.go", `func \(a \*App\) `+fn+`\(`)
		if !strings.Contains(body, "a.setProject(") {
			t.Errorf("%s does not point the session at a file, so the pages keep the last project's folder:\n%s", fn, body)
		}
	}
}

// New Project asks what to call it and where to put it.
//
// It used to make session.naivepost in the folder naivepost was started from --
// which is the one folder a session's own footage is never in. Every project
// therefore began in the checkout and had to be moved afterwards by a Save As,
// and by then the move is a folder with an hour of frames in it.
func TestNewProjectIsNamedAndPlaced(t *testing.T) {
	dir := t.TempDir()

	// the name box starts on today's date -- which is how the recorders name
	// their own files -- and steps past a project already called that rather
	// than proposing a name that would be refused
	day := time.Now().Format("2006-01-02")
	if got, want := freeProjName(dir), day+projExt; got != want {
		t.Errorf("the name box starts at %q, want %q", got, want)
	}
	if err := os.MkdirAll(filepath.Join(dir, day+projExt), 0o755); err != nil {
		t.Fatal(err)
	}
	if got, want := freeProjName(dir), day+"-2"+projExt; got != want {
		t.Errorf("with today's name taken the box starts at %q, want %q", got, want)
	}

	// a name that is a project already is refused, and that project is left
	// exactly as it was: a blank session lands on disk within the second
	// (saveProjectNow), and it would land on top of frames, a cut and a
	// narration it knows nothing about
	proj := filepath.Join(dir, "raid"+projExt)
	if err := os.MkdirAll(proj, 0o755); err != nil {
		t.Fatal(err)
	}
	was := []byte(`{"context":"the project that was there"}`)
	if err := os.WriteFile(projFile(proj), was, 0o644); err != nil {
		t.Fatal(err)
	}
	a := &App{root: dir, projPath: filepath.Join(dir, workName)}
	a.outDir = dataDir(a.projPath)
	a.newProjectAt(proj)
	if got, err := os.ReadFile(projFile(proj)); err != nil || string(got) != string(was) {
		t.Errorf("New on an existing project rewrote it: %q (%v)", got, err)
	}
	if a.projPath != filepath.Join(dir, workName) {
		t.Errorf("the session moved into %s regardless", a.projPath)
	}
	// ...including when the extension was not typed, since the name box is
	// where a folder gets its name and "raid" and "raid.naivepost" are one
	// project (withProjExt)
	a.newProjectAt(filepath.Join(dir, "raid"))
	if got, _ := os.ReadFile(projFile(proj)); string(got) != string(was) {
		t.Errorf("a name typed without the extension started over an existing project: %q", got)
	}

	// both ways in lead to the box, and the box is a NAME box: a folder
	// chooser can only pick a folder that already exists, and this one is
	// being created
	body := funcBody(t, "project.go", `func \(a \*App\) newProjectDialog\(\) \{`)
	if strings.Count(body, "a.askNewProject") != 2 {
		t.Errorf("New does not ask where it goes on both paths:\n%s", body)
	}
	if strings.Contains(body, "a.newProject(") {
		t.Errorf("New still starts a project without asking where:\n%s", body)
	}
	ask := funcBody(t, "project.go", `func \(a \*App\) askNewProject\(\) \{`)
	if !strings.Contains(ask, "a.saveAs(") || strings.Contains(ask, "a.pickFolder(") {
		t.Errorf("the new project is not named in a name box:\n%s", ask)
	}
	// and Add opens where the project was put, not in the checkout's own
	// input_video, which for a session on a card is a folder with nothing in it
	if !strings.Contains(funcBody(t, "project.go", `func \(a \*App\) newProject\(path string\) \{`),
		"a.vidDir, a.audDir = dir, dir") {
		t.Error("a new project's choosers open in the folder naivepost was started from")
	}
}

// A project written by an older build is adopted on open: the .data folder
// becomes the project, and the file that named it becomes the naivepost.json in
// it. One thing on disk where there were two, which is what the whole change
// is for -- and the old pair could be separated by a move, which broke a
// project silently.
//
// Nothing is written over. A name already taken by a folder stops the adoption
// and says so, because quietly merging somebody's two projects is worse than
// refusing to open one.
func TestAProjectFromAnOlderBuildBecomesAFolder(t *testing.T) {
	ownConfig(t)
	root := t.TempDir()
	a := &App{root: root}

	// the pair an older build wrote: the file, and the folder hanging off its name
	file := filepath.Join(root, "tom.json"+projExt)
	data := file + ".data"
	if err := os.WriteFile(file, []byte(`{"out_dir":"x"}`), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.MkdirAll(filepath.Join(data, "cut"), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(data, "cut", "cut.json"), []byte("{}"), 0o644); err != nil {
		t.Fatal(err)
	}
	got, err := a.adoptLegacy(file)
	if err != nil {
		t.Fatalf("adopting the old pair: %v", err)
	}
	if got != file {
		t.Errorf("the project is now %s, want the folder %s", got, file)
	}
	if b, err := os.ReadFile(projFile(got)); err != nil || string(b) != `{"out_dir":"x"}` {
		t.Errorf("the project's own file did not come with it: %q %v", b, err)
	}
	// the work that was in .data is in the project, and .data is gone
	if !exists(filepath.Join(got, "cut", "cut.json")) {
		t.Error("the cut was left behind in the old .data folder")
	}
	if exists(data) {
		t.Error("the old .data folder is still there, so the work is in two places")
	}
	// opening it again is a no-op: it is already a folder
	if again, err := a.adoptLegacy(got); err != nil || again != got {
		t.Errorf("a project that is already a folder came back as %q, %v", again, err)
	}
	// ...and so is being handed the naivepost.json inside it, which is what a
	// file manager or an old recents entry can still be pointing at
	if in, err := a.adoptLegacy(projFile(got)); err != nil || in != got {
		t.Errorf("naivepost.json picked out of its own folder came back as %q, %v", in, err)
	}

	// a .json from before projects had an extension becomes name.json.naivepost
	old := filepath.Join(root, "jan.json")
	if err := os.WriteFile(old, []byte("{}"), 0o644); err != nil {
		t.Fatal(err)
	}
	if got, err := a.adoptLegacy(old); err != nil || got != old+projExt {
		t.Errorf("%s became %q (%v), want %s", old, got, err, old+projExt)
	}
	// the very old working copy comes back as the working copy, not as
	// project.json.naivepost: it was never a name anybody chose
	work := filepath.Join(root, "project.json")
	if err := os.WriteFile(work, []byte("{}"), 0o644); err != nil {
		t.Fatal(err)
	}
	if got, err := a.adoptLegacy(work); err != nil || got != filepath.Join(root, workName) {
		t.Errorf("the old working copy came back as %q (%v), want %s", got, err, filepath.Join(root, workName))
	}

	// a name already taken by a folder: the adoption stops rather than merging
	taken := filepath.Join(root, "kim.json")
	if err := os.WriteFile(taken, []byte("{}"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.MkdirAll(taken+projExt, 0o755); err != nil {
		t.Fatal(err)
	}
	if _, err := a.adoptLegacy(taken); err == nil {
		t.Error("a project was adopted into a folder that already exists")
	}

	// and the load path is what does it -- Save is not the only door in
	body := funcBody(t, "project.go", `func \(a \*App\) loadProjectFrom\(`)
	if !strings.Contains(body, "path, err := a.adoptLegacy(path)") {
		t.Errorf("a project read from an older build's file is not adopted:\n%s", body)
	}
	// what it is remembered as has to be the folder it is kept as, or the next
	// launch reopens the old file and the adoption happens again every time
	if !strings.Contains(body, "a.rememberProject(a.projPath)") {
		t.Errorf("the next launch is pointed at the file this one stopped using:\n%s", body)
	}
	// what an older session already wrote is NOT moved: the working copy wrote
	// into the root, which also holds the checkout, and no rename could tell
	// one from the other
	launch := funcBody(t, "main.go", `func \(a \*App\) build\(`)
	for _, no := range []string{"os.Rename", "a.moveOutputs("} {
		if strings.Contains(launch, no) {
			t.Errorf("the launch calls %s to move an old session's outputs, which is a guess", no)
		}
	}
	if !strings.Contains(launch, `case exists(filepath.Join(a.root, "project.json")):`) {
		t.Error("a session saved by an older build no longer opens at all")
	}
}

// Nobody chooses the output folder. It was a button on the Prepare page and a
// line in every project file, which meant two answers to "where does this
// session write" that could disagree -- and when they did, the work went
// somewhere nobody was looking. One answer now: the open project's own name.
func TestNothingChoosesTheOutputFolder(t *testing.T) {
	files, err := filepath.Glob("*.go")
	if err != nil {
		t.Fatal(err)
	}
	sets := 0
	for _, f := range files {
		if strings.HasSuffix(f, "_test.go") {
			continue
		}
		src := readSrc(t, f)
		for _, gone := range []string{"outDirRow", "chooseOutDirDialog", "outLabel", "setOutDir("} {
			if strings.Contains(src, gone) {
				t.Errorf("%s still has %s -- the output folder is derived, not picked", f, gone)
			}
		}
		sets += strings.Count(src, "a.outDir = ")
	}
	// three: the empty session main() starts with, setProject, and the load
	// that has to know the folder before it resolves the sources stored inside
	// it (loadProjectFrom -- setProject then says it again, harmlessly). A
	// fourth is a folder that moved without the project it belongs to.
	if sets != 3 {
		t.Errorf("the output folder is assigned in %d places, want 3", sets)
	}
	// ...and that one is before the pages are filled, or a source stored
	// inside the project resolves into the project that was open before it
	load := funcBody(t, "project.go", `func \(a \*App\) loadProjectFrom\(`)
	if i, j := strings.Index(load, "a.projPath, a.outDir = path, dataDir(path)"), strings.Index(load, "a.applyProject(p)"); i < 0 || j < 0 || i > j {
		t.Error("the project's folder is set after its sources are resolved, so they resolve into the last one's")
	}
}

// A project no longer carries a WORDING style: the Style dropdown that turned
// every prompt to a wording of one name is gone, and so are the keys behind
// it. What kind of video this session is, as a matter of wording, goes in the
// context (prompts.go). What it carries instead is which PIPELINE the session
// runs through (textedit.go) -- a read to camera is edited as text and never
// described -- and that is a fact the app acts on, not a wording it sends.
func TestAProjectNoLongerCarriesAStyle(t *testing.T) {
	src := readSrc(t, "project.go")
	for _, gone := range []string{"prompt_styles", "prompt_pick"} {
		if strings.Contains(src, gone) {
			t.Errorf("a project still carries %q", gone)
		}
	}
	if !strings.Contains(src, "Style string `json:\"style,omitempty\"`") {
		t.Error("a project does not say which pipeline it runs through")
	}
	// the one prompt key it still reads is the oldest: one string per job,
	// which is the shape the prompts have again, adopted where this machine
	// has nothing of its own (adoptProjectPrompts)
	if !strings.Contains(src, "Prompts map[string]string `json:\"prompts,omitempty\"`") {
		t.Error("an old project's edited prompts are no longer read at all")
	}
	if b, _ := json.Marshal(Project{}); strings.Contains(string(b), "style") {
		t.Errorf("an untouched project mentions a style: %s", b)
	}
}

// Adding a source asks where it should live: in place, or in the project.
//
// A project is a folder so that it can be copied, backed up or zipped as one
// thing -- and a session whose footage is on the card it was recorded on is
// not one thing, with nothing saying so until the card is gone and the row
// goes red. Both answers are right for different sessions, so it is asked once
// and never again for that file.
func TestAddingASourceAsksWhetherToCopyItIn(t *testing.T) {
	root := t.TempDir()
	a := &App{root: root, outDir: filepath.Join(root, "tom"+projExt)}
	if err := os.MkdirAll(a.sourcesDir(), 0o755); err != nil {
		t.Fatal(err)
	}
	// where the copies go: inside the project, which is the whole reason to
	// press it
	if got, want := a.sourcesDir(), filepath.Join(a.outDir, "sources"); got != want {
		t.Errorf("copies land in %s, want %s", got, want)
	}
	// a file already inside the project is not a candidate for copying
	inside := filepath.Join(a.sourcesDir(), "a.mp4")
	if err := os.WriteFile(inside, []byte("video"), 0o644); err != nil {
		t.Fatal(err)
	}
	if !a.inProject(inside) {
		t.Error("a file in the project's own sources folder reads as outside it")
	}
	if a.inProject(filepath.Join(root, "elsewhere.mp4")) {
		t.Error("a file beside the project reads as inside it")
	}
	// the copy itself: written to a .part and renamed, so an interrupted copy
	// cannot be mistaken for a source
	src := filepath.Join(root, "card.mp4")
	if err := os.WriteFile(src, []byte("0123456789"), 0o644); err != nil {
		t.Fatal(err)
	}
	to, err := copyInto(a.sourcesDir(), src, nil)
	if err != nil {
		t.Fatal(err)
	}
	if b, err := os.ReadFile(to); err != nil || string(b) != "0123456789" {
		t.Errorf("the copy reads %q, %v", b, err)
	}
	if exists(to + ".part") {
		t.Error("the half-written copy is still on disk beside the finished one")
	}
	// the same file again costs nothing: same name, same size, already here
	stat, _ := os.Stat(to)
	if again, err := copyInto(a.sourcesDir(), src, nil); err != nil || again != to {
		t.Errorf("adding the same card twice came back %q, %v", again, err)
	}
	if now, _ := os.Stat(to); now.ModTime() != stat.ModTime() {
		t.Error("the second add copied the file again")
	}
	// Add goes through the one decision... (there were two Add buttons, files
	// and folder; the folder one went, because recordings are picked by name
	// and that is the one shape a portal file chooser has)
	src2 := readSrc(t, "project.go")
	if n := strings.Count(src2, "a.askImport"); n != 1 {
		t.Errorf("%d Add paths decide where the file should live, want the one", n)
	}
	if strings.Contains(readSrc(t, "prep.go"), `NewButtonWithLabel("Add source folder`) || strings.Contains(src2, "func (a *App) addFolderDialog") {
		t.Error("the folder button is back")
	}
	// ...which is a setting of the project, not a question per file: copy by
	// default, and the tick beside the buttons turns it off
	imp := readSrc(t, "project_import.go")
	if !strings.Contains(funcBody(t, "project_import.go", `func \(a \*App\) askImport\(paths \[\]string\) \{`), "if a.refSources {") {
		t.Error("Add does not read the project's copy/reference answer")
	}
	if strings.Contains(imp, "askCopy(") {
		t.Error("Add asks the question on every file again")
	}
	prep := readSrc(t, "prep.go")
	for _, want := range []string{
		`a.copyTick = gtk.NewCheckButtonWithLabel("copy into project")`,
		"a.copyTick.SetActive(true)", // copy is the default
		"a.refSources = !a.copyTick.Active()",
	} {
		if !strings.Contains(prep, want) {
			t.Errorf("prep.go no longer has %q", want)
		}
	}
	if !strings.Contains(src2, "RefSources:  a.refSources,") || !strings.Contains(src2, "a.applyRefSources(p.RefSources)") {
		t.Error("the answer does not ride the project")
	}
	// the copy reports its bytes, not its files: one 18 GB capture is one file
	m := &copyMeter{total: 1000}
	var got []float64
	m.tick = func(f float64) { got = append(got, f) }
	for i := 0; i < 100; i++ {
		m.Write(make([]byte, 10))
	}
	if len(got) < 50 || got[len(got)-1] < 0.99 {
		t.Errorf("a 1000-byte copy in 10-byte writes reported %d fractions ending at %v", len(got), got)
	}
}

// A source inside the project is stored relative to the project.
//
// This is what a folder-shaped project is for: rename it, move it, zip it and
// hand it over. Absolute paths break every one of those silently -- the file
// is not where the project says, the row is pruned on open, and the autosave
// writes the pruned list back, so the session loses the sources it was cut
// from. The voice split writes its stems inside the project, and Add can copy
// footage in, so this is the ordinary case and not a corner.
func TestASourceInsideTheProjectMovesWithIt(t *testing.T) {
	root := t.TempDir()
	a := &App{root: root, outDir: filepath.Join(root, "tom"+projExt)}

	in := filepath.Join(a.outDir, "stems", "a.novoice.mkv")
	if got, want := a.storePath(in), projPrefix+"stems/a.novoice.mkv"; got != want {
		t.Errorf("a stem is stored as %q, want %q", got, want)
	}
	if got := a.loadPath(a.storePath(in)); got != in {
		t.Errorf("it reads back as %q, want %q", got, in)
	}
	// ...and the same project under another name finds it again, which is the
	// whole point
	moved := &App{root: root, outDir: filepath.Join(root, "jan"+projExt)}
	if got, want := moved.loadPath(a.storePath(in)), filepath.Join(moved.outDir, "stems", "a.novoice.mkv"); got != want {
		t.Errorf("after a rename the stem reads as %q, want %q", got, want)
	}
	// a source outside the project is stored as it always was
	out := filepath.Join(root, "Nick2", "card.mp4")
	if got := a.storePath(out); strings.HasPrefix(got, projPrefix) {
		t.Errorf("a file beside the project is stored as project-relative: %q", got)
	}
	// the two ends of the project file use the pair
	src := readSrc(t, "project.go")
	for _, want := range []string{"Path: a.storePath(it.path)", "path: a.loadPath(s.Path)"} {
		if !strings.Contains(src, want) {
			t.Errorf("the project file does not go through the pair: %q", want)
		}
	}

	// and a project adopted from an older build has its old .data paths
	// rewritten to it, or the first open after the upgrade prunes them
	if !strings.Contains(funcBody(t, "project.go", `func \(a \*App\) adoptLegacy\(`), `"`+"`"+`+path+".data"`) {
		t.Error("adopting an older project leaves paths pointing into the folder it renamed")
	}
}

// ...and so does everything else the project writes down. A source was the
// first path to move inside the project and it is not the only one: the cut
// names the lanes and the cards it was given, the publish record names the
// frames it drew from, and all three of those live in the project folder now.
// Any one of them stored root-relative is a file the project loses the moment
// the folder is renamed -- silently, because the reader resolves it against a
// root that still exists.
func TestEveryPathAProjectWritesIsRelativeToIt(t *testing.T) {
	root := t.TempDir()
	a := &App{root: root, outDir: filepath.Join(root, "tom"+projExt)}
	moved := &App{root: root, outDir: filepath.Join(root, "jan"+projExt)}

	for _, in := range []string{
		filepath.Join(a.outDir, "sources", "cam.mkv"),   // copied in by Add
		filepath.Join(a.outDir, "assets", "tier.svg"),   // a card the project ships
		filepath.Join(a.outDir, "produce", "shot4.jpg"), // a thumbnail candidate
	} {
		stored := a.storePath(in)
		if !strings.HasPrefix(stored, projPrefix) {
			t.Errorf("%s is stored as %q, which does not travel with the project", in, stored)
		}
		want := filepath.Join(moved.outDir, filepath.Base(filepath.Dir(in)), filepath.Base(in))
		if got := moved.loadPath(stored); got != want {
			t.Errorf("after a rename %q reads as %q, want %q", stored, got, want)
		}
	}

	// every writer goes through the one function, so a path cannot be written
	// the old way by a page that has not heard about the project folder
	for _, w := range []struct{ file, call string }{
		{"cut_lane.go", "Src: ed.a.storePath(src)"},          // a lane cut from a file
		{"cut.go", "rel := a.storePath(file) + q.suffix()"},  // an insert, card parameters and all
		{"cut.go", "ed.addSound(a.storePath(au.path)"},       // a pasted stretch of sound
		{"publish.go", "st.Frames[i] = a.storePath(f)"},      // the frames the thumbnail was drawn from
		{"project.go", "VidDir:      a.storePath(a.vidDir)"}, // and where the choosers open
	} {
		if !strings.Contains(readSrc(t, w.file), w.call) {
			t.Errorf("%s writes a path without storePath: want %q", w.file, w.call)
		}
	}
	// relToRoot is storePath's own fallback and nothing else's: called
	// directly, it is exactly the bug above
	files, err := filepath.Glob("*.go")
	if err != nil {
		t.Fatal(err)
	}
	for _, f := range files {
		if f == "project.go" || strings.HasSuffix(f, "_test.go") {
			continue
		}
		if strings.Contains(readSrc(t, f), "relToRoot(") {
			t.Errorf("%s stores a path with relToRoot instead of storePath", f)
		}
	}
	if n := strings.Count(readSrc(t, "project.go"), "a.relToRoot("); n != 1 {
		t.Errorf("relToRoot is called %d times in project.go, want the one in storePath", n)
	}
}
