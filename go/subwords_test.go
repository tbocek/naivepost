package main

// What the subtitles spell, which is not what the recogniser heard.

import (
	"strings"
	"testing"
)

// mkWords is a line as the aligner hands it back: bare lowercase, one second
// each, already dressed from the raw transcript.
func mkWords(src string, raw ...string) []srcWord {
	var out []srcWord
	for i, r := range raw {
		out = append(out, srcWord{s: float64(i), e: float64(i) + 0.9, w: bareWord(r), raw: r, src: src})
	}
	return out
}

func spelt(ws []srcWord) string {
	var out []string
	for _, w := range ws {
		if w.raw == "" {
			continue
		}
		out = append(out, w.raw)
	}
	return strings.Join(out, " ")
}

// The session context reaches the subtitles through the transcript pass and
// not by a second opinion: "use the script's spelling for numbers I say aloud"
// is answered once, when the transcript is fixed, and the subtitles inherit
// it. Before this they were dressed from the RAW transcript, so the same
// seconds of the same video read "rsa two hundred and sixty" or "RSA-260"
// depending on which speech model happened to be loaded, and the sentence in
// the user context asking for one of them changed nothing.
func TestTheSubtitlesTakeTheTranscriptsSpelling(t *testing.T) {
	ws := mkWords("cam", "and", "they", "have", "factored", "rsa", "two", "hundred", "and", "sixty,")
	rows := []tsvRow{{s: 0, e: 9, src: "cam", spk: "SPEAKER_00",
		text: "and they have factored RSA-260,"}}
	fixWords(ws, rows)
	if got := spelt(ws); got != "and they have factored RSA-260," {
		t.Errorf("the line reads %q", got)
	}
	// the fold is printed on the FIRST word of the stretch and the rest say
	// nothing: every word keeps its own seconds, so a cut through the middle
	// of a rewritten stretch loses words rather than showing words the video
	// does not play -- the one thing word-built subtitles exist to prevent
	if ws[4].raw != "RSA-260," {
		t.Errorf("the rewritten stretch is not on its first word: %q", ws[4].raw)
	}
	for i := 5; i < len(ws); i++ {
		if ws[i].raw != "" {
			t.Errorf("word %d still says %q as well", i, ws[i].raw)
		}
		if ws[i].s != float64(i) {
			t.Errorf("word %d lost its seconds", i)
		}
	}
	// ...and the word every other pass matches on is untouched: the cut, the
	// retakes and the text edit all read .w, and only the subtitles read .raw
	if ws[5].w != "two" {
		t.Errorf("the matching word was rewritten too (%q) -- the cut reads that one", ws[5].w)
	}
}

// The other ways the two spellings differ, none of which may invent a time.
func TestTheRedressingNeverInventsATime(t *testing.T) {
	for _, c := range []struct {
		what  string
		raw   []string
		fixed string
		want  string
	}{
		{"case and punctuation, word for word",
			[]string{"welcome", "back", "this", "is"}, "Welcome back. This is", "Welcome back. This is"},
		{"a word the transcript adds rides on the next one that has a time",
			[]string{"moved", "it", "to", "gpus"}, "moved it from CPUs to GPUs", "moved it from CPUs to GPUs"},
		{"words left over when the line has run out are already printed",
			[]string{"was", "rsa", "two", "hundred", "and", "fifty"}, "was RSA-250", "was RSA-250"},
		{"a line with nothing in common is left as it was heard",
			[]string{"kept", "on", "track"}, "völlig anderer Text hier", "kept on track"},
	} {
		ws := mkWords("cam", c.raw...)
		fixWords(ws, []tsvRow{{s: 0, e: 99, src: "cam", spk: "SPEAKER_00", text: c.fixed}})
		if got := spelt(ws); got != c.want {
			t.Errorf("%s: %q, want %q", c.what, got, c.want)
		}
		for i := range ws {
			if ws[i].s != float64(i) || ws[i].e != float64(i)+0.9 {
				t.Errorf("%s: word %d was re-timed", c.what, i)
			}
		}
	}
}

// A row is only ever matched against its own recording's words, and an EVENT
// line describes the picture rather than something said.
func TestOnlyTheRightWordsAreRedressed(t *testing.T) {
	ws := mkWords("cam", "hello", "there")
	fixWords(ws, []tsvRow{
		{s: 0, e: 9, src: "mic", spk: "SPEAKER_00", text: "Something else entirely"},
		{s: 0, e: 9, src: "cam", spk: "EVENT", text: "Calm; a slide holds"},
	})
	if got := spelt(ws); got != "hello there" {
		t.Errorf("words were dressed from another recording or from an EVENT line: %q", got)
	}
	// and with no timeline at all -- Prepare has not merged yet -- nothing
	// happens rather than everything being emptied
	fixWords(ws, nil)
	if got := spelt(ws); got != "hello there" {
		t.Errorf("with no session timeline the words were changed: %q", got)
	}
}

// Where it is wired in, and what reads the result.
func TestTheSpellingIsFixedWhereTheWordsAreRead(t *testing.T) {
	body := funcBody(t, "retake.go", `func \(a \*App\) sessionWords\(`)
	dress := strings.Index(body, "dressWords(mine,")
	fix := strings.Index(body, "fixWords(out,")
	if dress < 0 || fix < 0 || dress > fix {
		t.Errorf("the transcript's spelling is not applied after the raw one (%d, %d)", dress, fix)
	}
	// the raw transcript is still what the ALIGNER was handed, and the only
	// text its bare words can be walked against
	if !strings.Contains(body, `"transcript.txt"`) {
		t.Error("dressWords no longer walks the transcript the aligner was given")
	}
	// a word whose spelling was folded away says nothing, rather than putting
	// a double space in the middle of a caption
	cues := funcBody(t, "produce.go", `func wordCues\(`)
	if !strings.Contains(cues, `if w.raw == "" {`) {
		t.Error("a folded word still contributes to the caption text")
	}
}
