package main

import (
	"net/url"
	"strings"
	"testing"
)

// The bug: a voice sample is named after the whole voice key, and once takes
// are hand-picked that key carries a '#'. "file://"+path made the rest of the
// name a uri fragment, so the file the app had just written arrived at filesrc
// with its tail cut off and playback failed with "No such file".
func TestAFileNamedWithPunctuationStillReachesThePlayer(t *testing.T) {
	// the exact shape that failed: voice, pitch, hand-picked takes, hash
	const sample = "/mnt/rec/tom.json.naivepost.data/narrate/samples/own@-0.5#3a1f2b_9c2b1e4d5f60.wav"
	uri := fileURI(sample)
	if !strings.HasPrefix(uri, "file:///mnt/") {
		t.Fatalf("uri does not start at the root: %q", uri)
	}
	if strings.ContainsAny(strings.TrimPrefix(uri, "file://"), "#?%") != strings.Contains(uri, "%23") {
		t.Fatalf("punctuation left raw: %q", uri)
	}
	if strings.Contains(uri, "#") {
		t.Fatalf("the '#' is still a fragment marker, so the name is still cut: %q", uri)
	}

	// and what the other side makes of it is the file we asked for, byte for
	// byte -- escaping that does not round-trip is a different bug wearing the
	// same face
	for _, path := range []string{
		sample,
		"/mnt/rec/a b/clip #2.mp4",        // spaces and a hash in footage
		"/mnt/rec/100% done/take?1.wav",   // a percent and a query marker
		"/mnt/rec/Ümläut/vidéo.mp4",       // bytes above ASCII
		"/mnt/rec/plain_file-1.0~b/x.wav", // every character that stays raw
	} {
		got, err := url.PathUnescape(strings.TrimPrefix(fileURI(path), "file://"))
		if err != nil {
			t.Fatalf("%s: uri does not decode: %v", path, err)
		}
		if got != path {
			t.Errorf("%s: came back as %s", path, got)
		}
	}

	// the separators have to survive as separators, or the path is one long
	// file name in the root
	if got := fileURI("/a/b/c.wav"); got != "file:///a/b/c.wav" {
		t.Errorf("a plain path should be left alone, got %q", got)
	}
}

// Both places that hand a uri to a playbin: the preview's own, and the one each
// extra audio track gets. A path concatenated at either is the bug back again.
func TestEveryPlaybinIsGivenAnEscapedURI(t *testing.T) {
	// every line that sets the property, not just the two known ones: a third
	// playbin added later is the same bug waiting
	for _, line := range strings.Split(readSrc(t, "player.go"), "\n") {
		if strings.Contains(line, `SetObjectProperty("uri"`) && !strings.Contains(line, "fileURI(") {
			t.Errorf("a uri built without escaping: %s", strings.TrimSpace(line))
		}
	}
	for _, head := range []string{
		`func \(p \*Player\) PlaySegment\(`,
		`func newAux\(`,
	} {
		if body := funcBody(t, "player.go", head); !strings.Contains(body, "fileURI(") {
			t.Errorf("%s does not escape the path it plays", head)
		}
	}
}
