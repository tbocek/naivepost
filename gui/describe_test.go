package main

import (
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
		"Both labels, every time",
		"EVENT: Calm; same view, the driver keeps talking.",
		"Both examples are invented",
	} {
		if !strings.Contains(sys, want) {
			t.Errorf("the describe wording no longer shows %q", want)
		}
	}
}
