package main

import (
	"math"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"strings"
	"sync"
	"testing"
	"time"
)

// The concat demuxer reads its file arguments as single-quoted, so an
// apostrophe anywhere in the path -- and the project's name is the whole data
// folder's name -- used to end the quote early and take the rest of the list
// with it.
func TestAConcatListSurvivesAnApostropheInThePath(t *testing.T) {
	if got, want := concatLine("/mnt/rec/plain/a.wav"), "file '/mnt/rec/plain/a.wav'\n"; got != want {
		t.Errorf("an ordinary path should be written as it is: %q", got)
	}
	got := concatLine("/mnt/rec/tom's cut.naivepost.data/narrate/.ref0.wav")
	if want := `file '/mnt/rec/tom'\'')s cut`; strings.Contains(got, want) {
		t.Fatalf("nonsense guard tripped: %q", got)
	}
	// close, an escaped quote, open again -- what ffmpeg's own parser undoes
	if want := "file '/mnt/rec/tom'\\''s cut.naivepost.data/narrate/.ref0.wav'\n"; got != want {
		t.Errorf("escaped as\n %q\nwant\n %q", got, want)
	}
	if strings.Count(got, "'") != 5 { // two around the path, three in the escape
		t.Errorf("quote count is wrong for %q", got)
	}

	// and nowhere in the package still writes one by hand: the bug is one
	// Fprintf away from coming back in whichever list is added next
	files, _ := filepath.Glob("*.go")
	raw := regexp.MustCompile(`"file '%s'`)
	for _, f := range files {
		if strings.HasSuffix(f, "_test.go") {
			continue
		}
		if raw.MatchString(readSrc(t, f)) {
			t.Errorf("%s builds a concat line by hand; concatLine is what quotes it", f)
		}
	}
}

// The takes are part of the cache key, so they are read by whichever worker is
// about to speak a line while a ＋ on the GUI thread writes them. Handing the
// map itself back to be indexed outside the lock is not a stale answer in Go --
// it stops the program.
func TestTheHandPickedTakesAreOnlyTouchedUnderTheLock(t *testing.T) {
	a := &App{outDir: t.TempDir()}
	var wg sync.WaitGroup
	for i := 0; i < 8; i++ {
		wg.Add(2)
		go func(i int) {
			defer wg.Done()
			for n := 0; n < 200; n++ {
				a.setTakesFor("tom", []voiceTake{{S: float64(i), E: float64(i) + 3}})
			}
		}(i)
		go func() {
			defer wg.Done()
			for n := 0; n < 200; n++ {
				_ = a.takesFor("tom")
			}
		}()
	}
	wg.Wait()

	// the map never leaves voiceMu: every mention of it is inside a function
	// that holds the lock
	src := readSrc(t, "narrate_take.go")
	for _, head := range []string{
		`func \(a \*App\) takesFor\(`,
		`func \(a \*App\) setTakesFor\(`,
	} {
		if body := funcBody(t, "narrate_take.go", head); !strings.Contains(body, "a.voiceMu.Lock()") {
			t.Errorf("%s touches the takes without taking the lock", head)
		}
	}
	if strings.Contains(src, "return a.takesMap\n") {
		t.Error("narrate_take.go hands the map back to be read outside the lock")
	}
	// takesFor's whole answer is a map index, and it is the last thing it does:
	// the unlock has to be the deferred kind or that index is outside it again
	if body := funcBody(t, "narrate_take.go", `func \(a \*App\) takesFor\(`); !strings.Contains(
		body, "defer a.voiceMu.Unlock()") {
		t.Error("takesFor lets go of voiceMu before it reads the map")
	}
}

// The pitch sits in the same guarded block as the voice id and the takes, is
// asked by the same voiceKey, and was the one of the three left unguarded.
func TestThePitchIsReadUnderTheSameLockAsTheVoice(t *testing.T) {
	a := &App{outDir: t.TempDir()}
	var wg sync.WaitGroup
	for i := 0; i < 4; i++ {
		wg.Add(2)
		go func() {
			defer wg.Done()
			for n := 0; n < 200; n++ {
				_ = a.voiceKey()
			}
		}()
		go func(i int) {
			defer wg.Done()
			for n := 0; n < 100; n++ {
				a.setPitchST(float64(i%3) - 1)
			}
		}(i)
	}
	wg.Wait()

	if body := funcBody(t, "narrate_voice.go", `func \(a \*App\) pitchST\(`); !strings.Contains(body, "a.voiceMu.Lock()") {
		t.Error("pitchST reads the cached pitch without the lock voiceID takes")
	}
	// and the writer takes it too -- but must let go before shiftRef, which
	// asks pitchST straight back
	body := funcBody(t, "narrate_voice.go", `func \(a \*App\) setPitchST\(`)
	lock, unlock := strings.Index(body, "a.voiceMu.Lock()"), strings.Index(body, "a.voiceMu.Unlock()")
	shift := strings.Index(body, "a.shiftRef()")
	if lock < 0 || unlock < lock {
		t.Fatal("setPitchST writes the cached pitch without the lock")
	}
	if shift > 0 && shift < unlock || strings.Contains(body, "defer a.voiceMu.Unlock()") {
		t.Error("setPitchST still holds voiceMu when it calls shiftRef -- pitchST inside it would deadlock")
	}
	// and the same thing asked of the running code, because the deadlock only
	// shows up once a reference exists: before that setPitchST returns early
	// and never reaches shiftRef. The file's contents do not matter -- an
	// unshifted pitch copies it.
	os.MkdirAll(a.narrateDir(), 0o755)
	os.WriteFile(a.refBase(), []byte("stands in for a reference"), 0o644)
	done := make(chan struct{})
	go func() { defer close(done); a.setPitchST(0) }()
	select {
	case <-done:
	case <-time.After(5 * time.Second):
		t.Fatal("setPitchST never came back: it holds voiceMu across shiftRef, which asks pitchST for the lock again")
	}
	// the project switch clears all three, so it clears them together
	if body := funcBody(t, "project.go", `func \(a \*App\) setProject\(`); !regexp.MustCompile(
		`(?s)a\.voiceMu\.Lock\(\).*a\.pitchRead = false.*a\.voiceMu\.Unlock\(\)`).MatchString(body) {
		t.Error("setProject clears the cached pitch outside the lock it takes for the rest")
	}
}

// While the band walks the takes the shared player holds the RECORDING, and
// the sample's own button was drawing a ⏸ over it: a face that offered to pause
// a sample that was not there, and synthesized a new one when pressed.
func TestTheSamplesPlayButtonDoesNotClaimTheTakesWalk(t *testing.T) {
	body := funcBody(t, "runbar.go", `func \(a \*App\) syncPlayIcons\(`)
	i := strings.Index(body, "setPlayIcon(vp.playBtn")
	if i < 0 {
		t.Fatal("syncPlayIcons no longer draws the sample's play button")
	}
	call := body[i:]
	if j := strings.Index(call, "\n"); j > 0 {
		call = call[:j]
	}
	if !strings.Contains(call, "vp.spoken") {
		t.Errorf("the sample's ▶ is drawn from the player alone: %s", strings.TrimSpace(call))
	}
}

// The reference is concatenated out of one wav per take plus a list naming
// them. Those are scaffolding, and a later build with fewer takes leaves the
// extra ones sitting in step4 looking like part of the reference.
func TestBuildingTheReferenceLeavesNoScaffoldingBehind(t *testing.T) {
	dir, src := takeFixture(t, 40)
	a := &App{outDir: filepath.Join(dir, "out"), curCmds: map[*exec.Cmd]bool{}}
	a.selNarr[0] = src
	a.voiceSel = ownVoice
	if err := a.setTakesFor("tom", []voiceTake{{S: 2, E: 9}, {S: 14, E: 21}}); err != nil {
		t.Fatal(err)
	}
	if err := a.ensureVoiceBase(); err != nil {
		t.Fatalf("no reference from hand-picked takes: %v", err)
	}
	ents, err := os.ReadDir(a.narrateDir())
	if err != nil {
		t.Fatal(err)
	}
	for _, e := range ents {
		if strings.HasPrefix(e.Name(), ".ref") {
			t.Errorf("%s was left in step4 after the concat", e.Name())
		}
	}
	if !exists(a.refBase()) {
		t.Error("the reference itself is gone -- the cleanup took too much")
	}
}

// entryAt decides whose wav sounds at a moment; lineEnd draws the same
// boundary on the row. entryAt worked it out for itself with an exact float
// compare on the two starts, which is not the twentieth of a second of slack
// the rest of the page calls "the same clip" -- so on lines whose starts had
// drifted within that slack, the row printed one end and playback used another.
func TestTheRowAndThePlaybackAgreeOnWhereALineEnds(t *testing.T) {
	n := &narrator{a: &App{}, entries: []narrEntry{
		{S: 10, E: 40, At: 0, Text: "one"},
		{S: 10.02, E: 40, At: 5, Text: "two"}, // the same clip, a hair apart
	}}
	if got, want := n.lineEnd(0), 15.0; math.Abs(got-want) > 1e-9 {
		t.Fatalf("line 1 runs until line 2 arrives: %v", got)
	}
	if got := n.entryAt(16); got != 1 {
		t.Errorf("at 0:16 line 2 is speaking, but playback picked entry %d", got+1)
	}
	// and the plain case still divides where it always did
	if got := n.entryAt(12); got != 0 {
		t.Errorf("at 0:12 line 1 is speaking, but playback picked entry %d", got+1)
	}
	// one line alone owns its clip to the end
	solo := &narrator{a: &App{}, entries: []narrEntry{{S: 10, E: 40, At: 0, Text: "one"}}}
	if got, want := solo.lineEnd(0), 40.0; got != want {
		t.Errorf("the only line on a clip runs to the clip's end: %v", got)
	}
	// a next line with no words is not a next line: the seconds go back to the
	// clip rather than to a row that says nothing
	blank := &narrator{a: &App{}, entries: []narrEntry{
		{S: 10, E: 40, At: 0, Text: "one"}, {S: 10, E: 40, At: 5}}}
	if got, want := blank.lineEnd(0), 40.0; got != want {
		t.Errorf("an empty line below does not cut the one above short: %v", got)
	}
}

// A line's slot runs to where the next line on its clip starts, so retyping one
// row's time changes the row ABOVE it as well -- its printed end and its ⚠.
// Only the edited row was redrawn, and the other went on showing the old slot.
func TestMovingALineRedrawsTheRowAboveItToo(t *testing.T) {
	if body := funcBody(t, "narrate.go", `func \(n \*narrator\) restamp\(`); !strings.Contains(body, "i - 1") {
		t.Error("restamp redraws only the row it was given; the row above keeps a stale end time")
	}
	body := funcBody(t, "narrate.go", `func \(n \*narrator\) rebuildRows\(`)
	if !strings.Contains(body, "stamp: stamp") {
		t.Error("the row does not keep its own stamp, so nothing else can ever redraw it")
	}
	// the first draw is the one direct call; every handler goes through restamp
	if c := strings.Count(body, "stamp()"); c != 1 {
		t.Errorf("%d rows redraw themselves alone in rebuildRows -- they should call restamp", c-1)
	}
	if !strings.Contains(body, "n.restamp(i)") {
		t.Error("nothing in rebuildRows redraws the neighbouring row")
	}
}
