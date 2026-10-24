package main

// A caption's place on the picture, on its way through Produce. The narration
// entry says top/center/bottom; the .srt is the one thing that can carry that
// to both destinations -- the burn (encodeClip's subtitles filter, drawn by
// libass) and a sidecar or muxed track -- so the placement rides as an ASS
// override tag at the head of the cue text. And a captions-only project must
// render without a TTS server anywhere near it: the synthesis pass is the one
// remote call Produce could make, and it is skipped whole.

import (
	"os"
	"strings"
	"testing"
)

func TestACaptionCarriesItsPlaceOnThePicture(t *testing.T) {
	if got := subText(prodLine{text: "look up here", pos: "top"}); !strings.HasPrefix(got, `{\an8}`) {
		t.Fatalf("a top caption came out as %q — libass would draw it at the bottom", got)
	}
	if got := subText(prodLine{text: "TITLE", pos: "center"}); !strings.HasPrefix(got, `{\an5}`) {
		t.Fatalf("a centered caption came out as %q", got)
	}
	// the bottom is where subtitles already live: no tag, nothing for a player
	// that knows no ASS to mispronounce
	if got := subText(prodLine{text: "plain line", pos: ""}); strings.Contains(got, `{\`) {
		t.Fatalf("a bottom caption grew a tag: %q", got)
	}
	// the tag prefixes the WRAPPED text -- wrapping after tagging would count
	// the tag's characters as words on the first row
	long := strings.Repeat("word ", 12) + "end"
	if got := subText(prodLine{text: long, pos: "top"}); !strings.Contains(got, "\n") {
		t.Fatalf("a long caption stopped wrapping once it had a place: %q", got)
	}
}

func TestProduceNeverSpeaksACaptionsOnlyProject(t *testing.T) {
	src, err := os.ReadFile("produce.go")
	if err != nil {
		t.Fatal(err)
	}
	s := string(src)
	// the missing-lines synthesis pass is fenced off whole: with no voice
	// chosen, every line is missing and every one is meant to be
	if !strings.Contains(s, "if !a.captionsOnly() {") {
		t.Fatal("produce lost the captions-only fence around its synthesis pass")
	}
	// the placement crosses from the narration entry into the render's line...
	if !strings.Contains(s, "pos: e.Pos") {
		t.Fatal("the planning no longer copies a caption's placement")
	}
	// ...and both .srt writers -- the per-clip burn and the final timeline --
	// go through the tagging, not around it
	if strings.Count(s, "subText(ln)") < 2 {
		t.Fatal("an .srt writer bypasses subText — its captions all sit at the bottom")
	}
	// a project that wrote captions and put nothing in the video is told where
	// they went instead -- they are not lost, they are in the .srt beside it,
	// which is written whatever the dropdown says
	if !strings.Contains(s, "the lines are in the .srt beside it") {
		t.Fatal("produce no longer says where a captions-only project's lines went")
	}
}

// A cue with no words in it is not written at all.
//
// fixWords prints a rewritten stretch on the FIRST word of it and empties the
// rest ("RSA-1024" over "one thousand twenty four"), and when such a stretch
// crosses a cue boundary the second cue has no words left of its own. Written
// out it was a blank caption in the .srt, and a blank line in the request to
// the translator -- which answered nothing for it, twice, and had the line
// reported as one it had lost. The seconds belong to the phrase already on
// screen, so they extend it.
func TestACueWithNoWordsExtendsTheOneBeforeIt(t *testing.T) {
	// four words, the last two folded into the second, split by a long gap so
	// they land in two cues
	ws := []srcWord{
		{s: 0, e: 1, w: "the", raw: "The"},
		{s: 1, e: 2, w: "rsa", raw: "RSA-1024"},
		{s: 3.0, e: 4, w: "one", raw: ""},
		{s: 4, e: 5, w: "thousand", raw: ""},
	}
	got := wordCues(ws, 0, 1, 10)
	if len(got) != 1 {
		t.Fatalf("%d cues, want the one with words in it: %+v", len(got), got)
	}
	if got[0].text != "The RSA-1024" {
		t.Errorf("the cue reads %q", got[0].text)
	}
	// ...and it stays up while the words it stands for are still being said
	if end := got[0].at + got[0].dur; end < 4.9 {
		t.Errorf("the caption leaves at %.1fs, before the phrase is finished", end)
	}
	// a cue that is long already is not stretched past what anyone can read
	if got[0].dur > subCueMax+0.01 {
		t.Errorf("the caption is held %.1fs, past the ceiling", got[0].dur)
	}
}
