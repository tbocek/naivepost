package main

import (
	"os"
	"path/filepath"
	"regexp"
	"strings"
	"testing"
)

// TestMain keeps the suite off the developer's own settings. The config used
// to be a file beside the videos, so a test that built an App under a TempDir
// was isolated by construction; it is ~/.config/naivepost now, and without this
// a test that writes a conf would overwrite the endpoints of the machine it is
// running on -- and one that reads one would point a "local" test at whatever
// server this machine talks to.
//
// A test that needs its own folder still says so with t.Setenv. This is the
// floor, not the isolation.
func TestMain(m *testing.M) {
	dir, err := os.MkdirTemp("", "naivepost-test-config")
	if err != nil {
		panic(err)
	}
	os.Setenv("XDG_CONFIG_HOME", dir)
	code := m.Run()
	os.RemoveAll(dir)
	os.Exit(code)
}

// The whole point of the file, in one round trip: what was open when the window
// closed is what the next launch opens. The bug it replaces is the one the user
// hit -- save the session as jan-video.json, quit, come back to project.json --
// and it was invisible, because the header bar said the right name for as long
// as the session lasted.
func TestTheOpenProjectSurvivesARestart(t *testing.T) {
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())
	root := t.TempDir()
	a := &App{root: root}

	if got := a.lastProject(); got != "" {
		t.Errorf("a first launch remembered %q", got)
	}

	named := filepath.Join(root, "jan-video.json")
	if err := os.WriteFile(named, []byte("{}\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	a.rememberProject(named)

	// a fresh App is the restart: nothing is carried in memory
	if got := (&App{root: root}).lastProject(); got != named {
		t.Errorf("after a restart the project is %q, want %q", got, named)
	}
	if p := confPath(); !exists(p) {
		t.Errorf("nothing was written to %s", p)
	} else if !strings.HasSuffix(p, filepath.Join("naivepost", "llm.conf")) {
		t.Errorf("the settings live at %s, which is not the path the user was told", p)
	}

	// and going back to the unnamed session is a decision, not an absence
	work := filepath.Join(root, "project.json")
	if err := os.WriteFile(work, []byte("{}\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	a.rememberProject(work)
	if got := (&App{root: root}).lastProject(); got != work {
		t.Errorf("after saving back to the working copy the project is %q, want %q", got, work)
	}
}

// Every path inside a project file is relative to its root, so a project
// remembered against one naivepost folder must never be opened from another: its
// sources would resolve against the wrong directory and be dropped one warning
// at a time. Two sessions on one machine is the normal case, not the exotic one.
func TestEachNaivepostFolderRemembersItsOwnProject(t *testing.T) {
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())
	one, two := t.TempDir(), t.TempDir()
	pOne := filepath.Join(one, "jan-video.json")
	pTwo := filepath.Join(two, "the-other-raid.json")
	for _, p := range []string{pOne, pTwo} {
		if err := os.WriteFile(p, []byte("{}\n"), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	(&App{root: one}).rememberProject(pOne)
	(&App{root: two}).rememberProject(pTwo)

	if got := (&App{root: one}).lastProject(); got != pOne {
		t.Errorf("the first folder opens %q, want %q", got, pOne)
	}
	if got := (&App{root: two}).lastProject(); got != pTwo {
		t.Errorf("the second folder opens %q, want %q -- the second save overwrote the first", got, pTwo)
	}
	if got := (&App{root: t.TempDir()}).lastProject(); got != "" {
		t.Errorf("a folder that has never been used opens %q", got)
	}
}

// Nothing here is worth a failed launch. The file is a convenience: half-written
// by a kill -9, hand-edited into invalid JSON, pointing at a project on a drive
// that is not mounted this morning, or unwritable because there is no HOME at
// all -- every one of those has to end with naivepost opening the working copy.
func TestABrokenSettingsFileIsNotAFailedLaunch(t *testing.T) {
	cfg := t.TempDir()
	t.Setenv("XDG_CONFIG_HOME", cfg)
	root := t.TempDir()

	if err := os.MkdirAll(filepath.Join(cfg, "naivepost"), 0o755); err != nil {
		t.Fatal(err)
	}
	// a line-based file cannot really be "invalid", so this is the shape a
	// half-written one takes: keys nobody knows, and a line with no = in it
	if err := os.WriteFile(confPath(), []byte("{not a conf at al\nWHAT=\"who\"\n"), 0o600); err != nil {
		t.Fatal(err)
	}
	if got := (&App{root: root}).lastProject(); got != "" {
		t.Errorf("a corrupt settings file yielded %q", got)
	}
	// and it is overwritten rather than being a permanent state
	named := filepath.Join(root, "jan-video.json")
	if err := os.WriteFile(named, []byte("{}\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	(&App{root: root}).rememberProject(named)
	if got := (&App{root: root}).lastProject(); got != named {
		t.Errorf("after a corrupt file was replaced the project is %q, want %q", got, named)
	}

	// a project that has since been renamed, moved or deleted
	if err := os.Remove(named); err != nil {
		t.Fatal(err)
	}
	if got := (&App{root: root}).lastProject(); got != "" {
		t.Errorf("a project that is not there any more was still opened: %q", got)
	}
	// ...but the name is kept, because an unmounted drive looks the same as a
	// deletion and forgetting it would make the difference permanent
	if (&App{root: root}).readGlobal().Projects[root] != named {
		t.Error("the remembered name was pruned the first time the file was missing")
	}

	// nowhere to put the file at all
	t.Setenv("XDG_CONFIG_HOME", "")
	t.Setenv("HOME", "")
	if p := confPath(); p != "" {
		t.Errorf("with no HOME the settings path is %q, want it disabled", p)
	}
	b := &App{root: t.TempDir()} // and nothing left beside the videos either
	b.rememberProject(named)     // must not panic, must not fail
	if got := b.lastProject(); got != "" {
		t.Errorf("with no HOME the last project is %q", got)
	}
}

// The wiring, which is the half that rots: remembering has to happen in both
// places that assign projPath, the launch has to prefer what was remembered
// over the working copy, and running Prepare must not rename the open project
// back to project.json -- that last one is how a Save could be undone by
// pressing ▶, which would defeat all of the above.
func TestWhatIsRememberedIsWhatTheNextLaunchOpens(t *testing.T) {
	ownConfig(t)
	p := readSrc(t, "project.go")
	for _, fn := range []string{"saveProjectTo", "loadProjectFrom"} {
		body := regexp.MustCompile(`(?s)func \(a \*App\) ` + fn + `\(path string\) \{.*?\n}\n`).FindString(p)
		if body == "" {
			t.Fatalf("%s is gone", fn)
		}
		if !strings.Contains(body, "a.rememberProject(") {
			t.Errorf("%s changes the open project without recording it for the next launch", fn)
		}
	}
	m := readSrc(t, "main.go")
	if !strings.Contains(m, `case a.lastProject() != "":`) {
		t.Error("the launch no longer prefers the remembered project over the working copy")
	}
	if strings.Contains(m, `a.saveProjectTo(filepath.Join(a.root, "project.json"))`) {
		t.Error("running a step still renames the open project to the working copy, " +
			"which un-names a project the user saved")
	}
}

// ownConfig gives one test its own settings folder, made and empty. TestMain is
// the floor -- it keeps the suite off the developer's real config -- but the
// endpoints being global means every test that writes one writes the same file,
// and then an SD_SERVER saved by one test is what the next one reads as "with
// nothing set". The folder is created because a test that writes the file by
// hand, rather than through writeConf, has nowhere to put it otherwise.
func ownConfig(t *testing.T) {
	t.Helper()
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())
	if err := os.MkdirAll(filepath.Dir(confPath()), 0o700); err != nil {
		t.Fatal(err)
	}
}

// One file holds two kinds of answer: the endpoints, which the settings dialog
// writes, and what was remembered without anyone being asked -- which project
// was open. They are written by different code on different occasions, so the
// dialog has to read before it writes: saving an ffmpeg path must not be how
// you lose the project the next launch would have opened.
func TestSavingTheSettingsKeepsWhatWasRemembered(t *testing.T) {
	ownConfig(t)
	root := t.TempDir()
	a := &App{root: root}
	named := filepath.Join(root, "jan-video.json")
	if err := os.WriteFile(named, []byte("{}\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	a.rememberProject(named)

	if err := a.writeConf(appConf{Server: "https://x"}); err != nil {
		t.Fatal(err)
	}

	b := &App{root: root}
	if got := b.lastProject(); got != named {
		t.Errorf("saving the settings left the last project as %q, want %q", got, named)
	}
	if got := b.readConf().Server; got != "https://x" {
		t.Errorf("the endpoint that was saved reads back as %q", got)
	}
}
