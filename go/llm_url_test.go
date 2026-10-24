package main

import "testing"

// A Server box is a URL however it was typed: a bare host gets https://, a
// scheme that was typed is kept, a trailing slash comes off, and an empty
// box stays empty so the caller's loopback default applies.
func TestABareHostInTheServerBoxIsHTTPS(t *testing.T) {
	for in, want := range map[string]string{
		"ai.jos.li":                "https://ai.jos.li",
		"  ai.jos.li/ ":            "https://ai.jos.li",
		"ai.jos.li:8731":           "https://ai.jos.li:8731",
		"http://192.168.1.5:8731/": "http://192.168.1.5:8731",
		"https://ai.jos.li/v1":     "https://ai.jos.li/v1",
		"":                         "",
		"   ":                      "",
	} {
		if got := serverURL(in); got != want {
			t.Errorf("serverURL(%q) = %q, want %q", in, got, want)
		}
	}
	// and the four servers all go through it: the box, and the environment
	// behind it
	if got := llmServer(appConf{Server: "ai.jos.li"}); got != "https://ai.jos.li" {
		t.Errorf("llmServer with a bare host = %q", got)
	}
}
