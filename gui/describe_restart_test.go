package main

// ⏸ and ⏹ are two buttons and they have to mean two things. Describe is the
// run that takes hours, and it is the only one in the app that can resume at
// all -- it keeps an event log per source and skips the chunks already in it.
// That resume is unconditional, so ⏹ then ▶ picked up mid-session exactly like
// ⏸ then ▶ did, and there was no way to say "no, again, from the top" short of
// deleting understand/describe by hand.
//
// What these pin is the split: paused is parked, stopped is abandoned, and the
// ▶ after a stop is what actually throws the half-run away -- not the ⏹, so
// stopping at the end of the day still leaves the work on disk in the morning.

import (
	"errors"
	"os"
	"path/filepath"
	"testing"
	"time"
)

// The other half of the split, and the half that has to keep working: a paused
// run is a goroutine sitting in checkpoint, still holding its place in the
// chunk loop, and ▶ hands it back. Both jobs on the page go through here --
// describeVideo per chunk, fixRows per block -- so this is the whole of pause.
func TestPauseParksTheRunAndPlayHandsItBack(t *testing.T) {
	a := &App{}
	a.pauseFlag.Store(true)
	parked := make(chan error, 1)
	go func() { parked <- a.checkpoint() }()

	select {
	case err := <-parked:
		t.Fatalf("checkpoint let a paused run straight through (%v) -- ⏸ does nothing", err)
	case <-time.After(300 * time.Millisecond): // longer than checkpoint's own poll
	}

	a.pauseFlag.Store(false) // ▶
	select {
	case err := <-parked:
		if err != nil {
			t.Errorf("resuming a paused run reported %v -- it must carry on, not unwind", err)
		}
	case <-time.After(3 * time.Second):
		t.Error("checkpoint never returned after ▶ -- the run is parked for good")
	}
}

// ⏹ while parked has to end the run rather than wait for a ▶ that is never
// coming: stop is checked before pause, so the order the two flags are set in
// cannot deadlock the runner.
func TestStopEndsAParkedRun(t *testing.T) {
	a := &App{}
	a.pauseFlag.Store(true)
	a.stopFlag.Store(true)
	done := make(chan error, 1)
	go func() { done <- a.checkpoint() }()
	select {
	case err := <-done:
		if !errors.Is(err, errStopped) {
			t.Errorf("⏹ on a paused run reported %v, want errStopped", err)
		}
	case <-time.After(3 * time.Second):
		t.Error("⏹ on a paused run hung -- pause outranked stop")
	}
}

// halfDescribed is a project with two sources part-way through Describe: an
// event log, a rolling state, and the scaled frames the describer cached on
// the way there.
func halfDescribed(t *testing.T) *App {
	t.Helper()
	a := &App{outDir: t.TempDir()}
	for _, base := range []string{"gameplay-1", "gameplay-2"} {
		dir := filepath.Join(a.describeDir(), base)
		if err := os.MkdirAll(filepath.Join(dir, ".llmframes"), 0o755); err != nil {
			t.Fatal(err)
		}
		write := func(name, body string) {
			t.Helper()
			if err := os.WriteFile(filepath.Join(dir, name), []byte(body), 0o644); err != nil {
				t.Fatal(err)
			}
		}
		write("events.tsv", "0.00\t4.00\tsomething happens\n4.00\t8.00\tit goes on\n")
		write("state.txt", "In the lobby.\n")
		write(filepath.Join(".llmframes", "2026-08-16_08-54-55.jpg"), "\xff\xd8pretend jpeg")
	}
	return a
}

func describeLeftovers(t *testing.T, a *App, base string) (events, state, frame bool) {
	t.Helper()
	dir := filepath.Join(a.describeDir(), base)
	return exists(filepath.Join(dir, "events.tsv")),
		exists(filepath.Join(dir, "state.txt")),
		exists(filepath.Join(dir, ".llmframes", "2026-08-16_08-54-55.jpg"))
}

// A ▶ that follows a ⏸ -- or a ▶ on a project described last week -- resumes.
// Nothing is armed, so nothing is cleared: this is the case where the hours
// already spent are the whole point of the event log.
func TestAPlainPlayResumesTheDescriber(t *testing.T) {
	a := halfDescribed(t)
	if a.undRestart {
		t.Fatal("a fresh App is armed to start over -- only a stop may do that")
	}
	if err := a.undFreshStart(); err != nil {
		t.Fatal(err)
	}
	for _, base := range []string{"gameplay-1", "gameplay-2"} {
		if ev, st, _ := describeLeftovers(t, a, base); !ev || !st {
			t.Errorf("%s: a plain ▶ cleared the resume state (events=%v state=%v) -- "+
				"the run will re-describe footage it already paid for", base, ev, st)
		}
	}
}

// A ▶ that follows a ⏹ starts the step over: every source's event log and
// rolling state go, so describeVideo has nothing to skip and begins at t=0.
func TestPlayAfterStopDescribesFromTheStart(t *testing.T) {
	a := halfDescribed(t)
	a.undRestart = true // what the stopped run sets on its way out
	if err := a.undFreshStart(); err != nil {
		t.Fatal(err)
	}
	for _, base := range []string{"gameplay-1", "gameplay-2"} {
		ev, st, frame := describeLeftovers(t, a, base)
		if ev || st {
			t.Errorf("%s: ▶ after ⏹ left the resume state behind (events=%v state=%v) -- "+
				"the run carries on mid-session instead of starting over", base, ev, st)
		}
		// the one thing starting over must NOT cost: scaled frames are pixels,
		// not results, and re-scaling an hour of footage is minutes of ffmpeg
		// for a file that would come out byte-identical
		if !frame {
			t.Errorf("%s: starting over threw the scaled frames away too", base)
		}
	}
	// and it disarms: a second ▶ (or one after the restarted run itself gets
	// somewhere) must resume like any other
	if a.undRestart {
		t.Error("still armed after starting over -- the next ▶ would wipe the run that just began")
	}
}

// The clearing is deliberately not the stop's own doing. ⏹ at the end of the
// day has to leave the session on disk: what decides the work was unwanted is
// the press that starts it again.
func TestStoppingKeepsTheWorkUntilTheNextPlay(t *testing.T) {
	a := halfDescribed(t)
	a.undRestart = true // stopped, and not restarted yet
	for _, base := range []string{"gameplay-1", "gameplay-2"} {
		if ev, st, _ := describeLeftovers(t, a, base); !ev || !st {
			t.Errorf("%s: the stop itself deleted the run (events=%v state=%v)", base, ev, st)
		}
	}
}

// Every folder under understand/describe/ is cleared, not just the sources selected
// now -- a log left by a recording since deselected is exactly the stale
// half-run this is for. And a project that has never been described is already
// at the start, so there is nothing to fail on.
func TestStartingOverIsSafeOnAnUndescribedProject(t *testing.T) {
	a := &App{outDir: t.TempDir()}
	a.undRestart = true
	if err := a.undFreshStart(); err != nil {
		t.Errorf("starting over on a project with no understand/ at all failed: %v", err)
	}

	// ...and on one where the folder exists but holds nothing yet
	b := &App{outDir: t.TempDir()}
	if err := os.MkdirAll(b.describeDir(), 0o755); err != nil {
		t.Fatal(err)
	}
	b.undRestart = true
	if err := b.undFreshStart(); err != nil {
		t.Errorf("starting over on an empty understand/describe failed: %v", err)
	}
}
