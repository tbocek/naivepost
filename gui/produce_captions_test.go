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
	// a project that wrote captions and turned subtitles off is told so
	if !strings.Contains(s, "the lines appear nowhere") {
		t.Fatal("produce no longer warns when captions have no way into the video")
	}
}
