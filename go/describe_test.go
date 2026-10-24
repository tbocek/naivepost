package main

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// A describe reply is two labelled lines, and one run of six chunks in one
// recording wrote the words without the first label. The description was right;
// only "EVENT:" was missing, and the whole reply -- state and all -- was then
// filed as the event and read by the cut as a sixty-word line about nothing.
//
// The wording shows both labels in a worked example now. This is the floor
// under it: an example makes the model right more often, never always.
func TestAReplyThatForgetsTheEventLabelIsStillRead(t *testing.T) {
	for _, c := range []struct{ what, reply, event, state string }{
		{"both labels, as asked",
			"EVENT: Calm; the article stays open.\nSTATE: Screen share of an article.",
			"Calm; the article stays open.", "Screen share of an article."},
		{"the label dropped, the words right",
			"Calm; Thomas keeps talking to camera in front of the Tor blog page.\nSTATE: Thomas is reading the Tor VPN section.",
			"Calm; Thomas keeps talking to camera in front of the Tor blog page.",
			"Thomas is reading the Tor VPN section."},
		{"...on one line, as it came back",
			"Calm; same view.  STATE: Reading the article.",
			"Calm; same view.", "Reading the article."},
	} {
		ev, st := eventState(c.reply)
		if ev != c.event {
			t.Errorf("%s: event %q, want %q", c.what, ev, c.event)
		}
		if st != c.state {
			t.Errorf("%s: state %q, want %q", c.what, st, c.state)
		}
		if strings.Contains(ev, "STATE:") {
			t.Errorf("%s: the state was filed as part of the event: %q", c.what, ev)
		}
	}
	// nothing recognisable is still said out loud rather than passed off as a
	// description: a chunk with no event at all is a fact about the run
	if ev, _ := eventState("I cannot help with that."); !strings.HasPrefix(ev, "(no event line") {
		t.Errorf("an unusable reply was filed as a description: %q", ev)
	}
	// and the wording shows what it asks for, including the short case, which
	// is most of the frames
	sys := describeSystem
	for _, want := range []string{
		"An EVENT line for every frame and one STATE line, every time",
		"EVENT [+0s]: same",
		"Both examples are invented",
	} {
		if !strings.Contains(sys, want) {
			t.Errorf("the describe wording no longer shows %q", want)
		}
	}
}

// One EVENT line per frame, on the frame's own stamp.
//
// One line per batch, stamped at the batch's first second, described four
// seconds and dated all of it to the first: "cuts to the title slide" was
// stamped three seconds before the slide arrived, and the thumbnail taken at
// that stamp was the moment before. The moment now sits on the frame it
// happened on, and a frame that changed nothing says only "same".
func TestEachFrameGetsItsOwnStampedLine(t *testing.T) {
	reply := "EVENT [+0s]: same\nEVENT [+1s]: same\n" +
		"EVENT [+2s]: Calm; the view cuts to the course page.\n" +
		"EVENT [+3s]: Calm; the \"Repetition DSy\" title slide fills the screen.\n" +
		"STATE: Title slide up, the lecturer in the corner."
	ev, st := eventLines(reply)
	if len(ev) != 4 {
		t.Fatalf("%d lines from four frames: %+v", len(ev), ev)
	}
	for i, want := range []float64{0, 1, 2, 3} {
		if ev[i].off != want {
			t.Errorf("line %d is stamped %+g, want %+g", i, ev[i].off, want)
		}
	}
	if !isSame(ev[0].text) || !isSame(ev[1].text) || isSame(ev[2].text) {
		t.Errorf("same/changed read wrongly: %+v", ev)
	}
	if !strings.Contains(ev[3].text, "title slide") || st == "" {
		t.Errorf("the change and the state did not come through: %+v / %q", ev, st)
	}
	// the stamp survives the ways a model writes it
	for _, form := range []string{"EVENT [+2.0s]: x", "EVENT [2s]: x", "EVENT[ +2 s ]: x"} {
		if ev, _ := eventLines(form); len(ev) != 1 || ev[0].off != 2 {
			t.Errorf("%q read as %+v", form, ev)
		}
	}
	// a reply in the old shape still reads: the first frame's line, the rest same
	ev, st = eventLines("EVENT: Calm; the driver keeps talking.\nSTATE: Lap 4 of 5.")
	if len(ev) != 1 || ev[0].off != 0 || isSame(ev[0].text) || st != "Lap 4 of 5." {
		t.Errorf("an unstamped reply read as %+v / %q", ev, st)
	}
	// and the wording asks for exactly this
	for _, want := range []string{"EVENT [+n s]: one line per frame", `the one word "same"`, "An EVENT line for every frame"} {
		if !strings.Contains(describeSystem, want) {
			t.Errorf("describeSystem no longer says %q", want)
		}
	}
}

// The file keeps a row per frame; a reader sees one line per thing that
// happened, running until the next. That is what keeps a brief the same size
// it was while the stamp on a change becomes the second of the change.
func TestSameFramesFoldIntoTheLineBefore(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "events.tsv")
	os.WriteFile(path, []byte(
		"8.00\t9.00\tCalm; the lecturer faces the camera.\n"+
			"9.00\t10.00\tsame\n"+
			"10.00\t11.00\tsame\n"+
			"11.00\t12.00\tCalm; the title slide fills the screen.\n"+
			"12.00\t13.00\tsame\n"), 0o644)
	rows := loadEvents(path)
	if len(rows) != 2 {
		t.Fatalf("%d rows from two events: %+v", len(rows), rows)
	}
	if rows[0].s != 8 || rows[0].e != 11 {
		t.Errorf("the first line runs %g-%g, want 8-11", rows[0].s, rows[0].e)
	}
	if rows[1].s != 11 || rows[1].e != 13 || !strings.Contains(rows[1].text, "title slide") {
		t.Errorf("the change is at %g-%g %q, want 11-13 and the slide", rows[1].s, rows[1].e, rows[1].text)
	}
}
