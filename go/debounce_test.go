package main

import (
	"os"
	"strings"
	"testing"
)

// clock stands in for glib's timer: it collects what was armed instead of
// waiting, and tick() is the beat going off. Nothing here needs a main loop,
// which is the point of the arm hook.
type clock struct {
	fired []func() bool
	waits []uint
}

func (c *clock) arm(ms uint, f func() bool) {
	c.waits = append(c.waits, ms)
	c.fired = append(c.fired, f)
}

// tick runs every timer armed so far, oldest first, as glib would -- and, as
// glib does, keeps the ones that ask to run again by returning true.
func (c *clock) tick() {
	pending := c.fired
	c.fired = nil
	for _, f := range pending {
		if f() {
			c.fired = append(c.fired, f)
		}
	}
}

// Typing a sentence is one run of the work, at the end -- not one per key.
func TestABurstOfEditsCostsOneRunOfTheWork(t *testing.T) {
	c := &clock{}
	d := &debounce{ms: 5, arm: c.arm}
	runs := 0
	work := func() { runs++ }

	for range 20 { // "a sentence"
		d.call(work)
	}
	if runs != 0 {
		t.Errorf("the work ran %d times while the typing was still going", runs)
	}
	if !d.pending() {
		t.Error("twenty edits owe the disk nothing")
	}
	c.tick() // all twenty beats go off; nineteen find themselves overtaken
	if runs != 1 {
		t.Errorf("a twenty-key burst ran the work %d times, want 1", runs)
	}
	if d.pending() {
		t.Error("the work has run and something is still owed")
	}

	// ...and a beat that goes off with nothing behind it runs nothing again.
	// This also settles that the beat is one shot: twenty timers that asked to
	// keep going would be twenty more runs on the next tick, forever.
	c.tick()
	c.tick()
	if runs != 1 {
		t.Errorf("the work ran %d times, want the 1 from before", runs)
	}
	if len(c.fired) != 0 {
		t.Errorf("%d beats are still armed after they have gone off", len(c.fired))
	}

	// the next burst is its own run
	d.call(work)
	d.call(work)
	c.tick()
	if runs != 2 {
		t.Errorf("a second burst brings the count to %d, want 2", runs)
	}
}

// ...and the moments where waiting is not allowed do not wait: leaving the
// page, closing the window, starting the run that reads the file.
func TestFlushRunsWhatIsOwedAndLeavesNothingArmed(t *testing.T) {
	c := &clock{}
	d := &debounce{ms: 5, arm: c.arm}
	runs := 0
	d.call(func() { runs++ })
	d.flush()
	if runs != 1 {
		t.Errorf("flushing an owed edit ran the work %d times, want 1", runs)
	}
	c.tick() // the beat armed by the call still goes off, and finds itself stale
	if runs != 1 {
		t.Errorf("the flushed work ran again on the beat: %d runs, want 1", runs)
	}
	// nothing owed is a no-op, so a flush can sit wherever it belongs rather
	// than only where something is known to be pending
	d.flush()
	if runs != 1 || d.pending() {
		t.Errorf("flushing an idle debounce ran the work (%d runs) or left it owing", runs)
	}
}

// A field with nothing set still waits: the type is used as a plain member and
// a zero wait would be no debounce at all.
func TestADebounceThatWasNeverSetUpStillWaits(t *testing.T) {
	c := &clock{}
	d := &debounce{arm: c.arm}
	d.call(func() {})
	if len(c.waits) != 1 || c.waits[0] != editWait {
		t.Errorf("an unconfigured debounce waited %v, want the %d ms default", c.waits, editWait)
	}
}

// ...and the page that owes it behaves that way: the words are in memory as
// they are typed, and the file hears about them when the sentence is over.
func TestANarrationLineReachesTheDiskWhenTheTypingStops(t *testing.T) {
	dir := t.TempDir()
	n := &narrator{a: &App{root: dir, outDir: dir},
		entries: []narrEntry{{S: 0, E: 5, Text: "a line"}}}
	c := &clock{}
	n.saveQ.arm = c.arm

	n.saveSoon()
	if exists(n.a.narrPath()) {
		t.Error("a keystroke wrote the narration file")
	}
	n.entries[0].Text = "a line, still being typed"
	n.saveSoon()
	c.tick()
	b, err := os.ReadFile(n.a.narrPath())
	if err != nil {
		t.Fatalf("the typing stopped and the file was never written: %v", err)
	}
	if !strings.Contains(string(b), "still being typed") {
		t.Errorf("the file holds an earlier keystroke, not the last:\n%s", b)
	}
}

// Typing a narration line used to write the whole narration file AND walk the
// output folder counting files, per keystroke. The words go into memory as they
// are typed -- what is on screen is what is stored -- and the disk hears about
// it once the sentence is over.
func TestTypingANarrationLineDoesNotWriteTheFilePerKeystroke(t *testing.T) {
	build := funcBody(t, "narrate.go", `func \(n \*narrator\) rebuildRows\(\)`)
	if strings.Contains(build, "n.pullRows()\n\t\t\t\tn.save()") {
		t.Error("a keystroke in a line box still writes the narration file")
	}
	for _, want := range []string{
		"n.pullRows()", // the model keeps up with the keys...
		"n.saveSoon()", // ...and the disk does not
		"n.restamp(i)", // and so does what the row says about itself
	} {
		if !strings.Contains(build, want) {
			t.Errorf("typing a line no longer does %s", want)
		}
	}
	// the clock field beside it is the same edit and gets the same treatment
	if strings.Contains(build, "n.save()\n\t\t\tn.restamp(i)") {
		t.Error("typing a line's start time still writes the file per keystroke")
	}

	// ...and every way of leaving the words behind settles what they owe first
	for file, want := range map[string]string{
		"main.go":    "a.narr.flushSave()", // switching pages
		"project.go": "a.narr.flushSave()", // closing the window
		"produce.go": "a.narr.flushSave()", // rendering, which reads the file
	} {
		b, err := os.ReadFile(file)
		if err != nil {
			t.Fatal(err)
		}
		if !strings.Contains(string(b), want) {
			t.Errorf("%s can leave a typed line unwritten", file)
		}
	}
	// and it is nil-safe, because two of those three run before the page exists
	if !strings.Contains(readSrc(t, "narrate.go"), "func (n *narrator) flushSave() {\n\tif n != nil {") {
		t.Error("flushSave crashes on a page that was never opened")
	}
}
