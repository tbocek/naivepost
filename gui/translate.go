package main

// Translating the subtitles.
//
// The cues are already the words of the finished video, on the video's own
// clock (wordCues, tidyCues). A translation is those same cues with their text
// in another language and their times untouched: one line in, one line out, in
// order, so nothing has to be re-timed and a cue cannot go missing.
//
// One call per language, not one per line: a subtitle is read in the company of
// the ones around it, and a model given a whole track keeps a term spelled the
// same way in cue 3 and cue 90.

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"
)

// the languages the subtitles can be translated into. Two for now; the list is
// the only thing that has to grow.
var subLangs = []struct {
	code, tag, name string // ISO 639-1, the 3-letter tag a container wants, and the label
}{
	{"en", "eng", "English"},
	{"de", "deu", "German"},
	{"fr", "fra", "French"},
}

func subLangOf(code string) (string, string, bool) {
	for _, l := range subLangs {
		if l.code == code {
			return l.tag, l.name, true
		}
	}
	return "", "", false
}

// translateSystem is the pass's wording. The shape is the transcript fixer's,
// and for the same reason: N lines in, exactly N lines out, because the times
// belong to the line numbers and nothing else carries them.
const translateSystem = `You translate subtitles.

You are given the numbered lines of one video's subtitle track, in order. Answer with exactly those lines, in the same order, translated -- one line out for every line in, each still beginning with its own number and a tab.

A subtitle is read in a second and a half, so translate for the ear and the eye rather than word for word: say what the line says, as briefly as it can be said, in the language asked for.

The lines are one continuous talk cut into pieces, so a line often begins mid sentence and ends mid sentence. Translate it as the part of the sentence it is -- do not complete it, do not merge it with its neighbours, do not move words from one line into another. Read the lines around it to know what it means.

Names, products, companies and technical terms keep their own spelling. A term already in the target language stays as it is. Where a term is translated, translate it the same way every time it appears.

Keep the line breaks inside a line where you can, and keep the punctuation that ends it. Never add a line, never drop one, never leave one empty, and write nothing but the lines.`

// translateCues is one language's track: the same cues with their text
// translated. Empty when the call fails -- a subtitle track that could not be
// translated is one the render goes on without, not a render that stops.
func (a *App) translateCues(cues []subCue, code string) []subCue {
	_, name, ok := subLangOf(code)
	if !ok || len(cues) == 0 {
		return nil
	}
	var b strings.Builder
	for i, c := range cues {
		// the breaks inside a cue are the wrapping, and a tab would end the
		// line: both are put back the way they came
		fmt.Fprintf(&b, "%d\t%s\n", i+1, strings.ReplaceAll(c.text, "\n", " / "))
	}
	user := a.ctxBlockFor("translate") + fmt.Sprintf(
		"TRANSLATE THESE %d LINES INTO %s. Answer with %d lines, numbered as they are here:\n\n%s",
		len(cues), name, len(cues), b.String())
	msgs := []map[string]any{msg("system", a.sysPrompt("translate")), msg("user", user)}
	reply, err := a.llmChatRetry("translate", msgs, false)
	if err != nil {
		a.logfIdle("!!! subtitles: %s: %v -- that language is left out", name, err)
		return nil
	}
	out, miss := numberedLines(reply, len(cues))
	if miss > 0 {
		a.logfIdle("!!! subtitles: %s came back missing %d of %d lines -- that language is left out",
			name, miss, len(cues))
		return nil
	}
	done := make([]subCue, len(cues))
	for i, c := range cues {
		done[i] = subCue{s: c.s, e: c.e, text: wrapSub(strings.ReplaceAll(out[i], " / ", "\n"))}
	}
	return done
}

// numberedLines reads the answer back onto the line numbers it was given, and
// says how many never came. By NUMBER and not by position: a model that drops a
// blank line or wraps one in two would otherwise shift every line after it onto
// the wrong seconds, which is the one mistake here that cannot be seen.
func numberedLines(reply string, n int) ([]string, int) {
	out := make([]string, n)
	for _, ln := range strings.Split(reply, "\n") {
		num, text, ok := strings.Cut(strings.TrimSpace(ln), "\t")
		if !ok {
			// a model that answered "3. text" or "3 text" instead
			num, text, ok = strings.Cut(strings.TrimSpace(ln), " ")
			if !ok {
				continue
			}
		}
		i := 0
		if _, err := fmt.Sscanf(strings.TrimRight(num, ".:"), "%d", &i); err != nil {
			continue
		}
		if i >= 1 && i <= n && out[i-1] == "" {
			out[i-1] = strings.TrimSpace(text)
		}
	}
	miss := 0
	for _, s := range out {
		if s == "" {
			miss++
		}
	}
	return out, miss
}

// subTrack is one finished subtitle file: the language it is in, and where it
// was written.
type subTrack struct {
	code, tag, name string
	path            string
}

// subTracks writes the track and its translations, and says what it wrote. The
// first is always the session's own language, whatever the list says: the words
// the video actually speaks are not a translation of anything.
func (a *App) subTracks(cues []subCue, dir string, want []string) []subTrack {
	if len(cues) == 0 {
		return nil
	}
	// the language the video is SPOKEN in, default filled in (asrLanguage):
	// the first track is not a translation of anything and carries it
	own := a.asrLanguage()
	tag, name, ok := subLangOf(own)
	if !ok {
		tag, name = "und", strings.ToUpper(own)
	}
	out := []subTrack{{code: own, tag: tag, name: name, path: filepath.Join(dir, "final.srt")}}
	for _, code := range want {
		if code == own {
			continue // already the track above, and not a translation of it
		}
		t, n, ok := subLangOf(code)
		if !ok {
			continue
		}
		a.prog(trackSTT, 0.94, "translating the subtitles")
		a.logfIdle(">>> subtitles: translating %d lines into %s", len(cues), n)
		done := a.translateCues(cues, code)
		if len(done) == 0 {
			continue
		}
		out = append(out, subTrack{code: code, tag: t, name: n,
			path: filepath.Join(dir, "final."+code+".srt")})
		if err := os.WriteFile(out[len(out)-1].path, []byte(srtText(done)), 0o644); err != nil {
			a.logfIdle("!!! subtitles: %s: %v", n, err)
			out = out[:len(out)-1]
		}
	}
	return out
}

// srtText is the cues as an .srt.
func srtText(cues []subCue) string {
	var b strings.Builder
	for i, c := range cues {
		fmt.Fprintf(&b, "%d\n%s --> %s\n%s\n\n", i+1, srtTime(c.s), srtTime(c.e), c.text)
	}
	return b.String()
}
