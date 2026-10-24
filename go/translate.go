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
	"strconv"
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
	system := a.sysPrompt("translate")
	// the same cues, the same language, the same wording: the same answer
	// (llmcache.go). This one is not a button anybody presses for a second
	// opinion -- it is a side effect of rendering, and it ran again in full on
	// every ▶ that muxed subtitles: three minutes of a ten-minute video's
	// render spent translating lines that had not changed since the last one.
	ask := askKey(system, user)
	reply, hit := a.cachedReply("translate", ask)
	if hit {
		a.logfIdle(">>> subtitles: %s came from the cache -- the same lines were translated before", name)
	} else {
		var err error
		reply, err = a.llmChatRetry("translate",
			[]map[string]any{msg("system", system), msg("user", user)}, false)
		if err != nil {
			a.logfIdle("!!! subtitles: %s: %v -- that language is left out", name, err)
			return nil
		}
	}
	out, miss := numberedLines(reply, len(cues))
	// a line with nothing in it is not a line that went missing: the model is
	// right to answer nothing for it, and asking again gets nothing again.
	// (Two calls were spent on exactly that -- an empty cue 31, which
	// wordCues no longer makes.)
	miss -= blankCues(cues, out)
	// A gap is not a reason to throw the language away. One line in 123 came
	// back missing and the whole German track was dropped -- 122 good lines
	// for one -- so the gaps are asked about again on their own, and whatever
	// is still missing after that stays in the language it was spoken in. A
	// viewer can read one line of English in a German track; they cannot read
	// a track that is not there.
	if miss > 0 {
		a.logfIdle(">>> subtitles: %s came back missing %s -- asking again for %s",
			name, gapList(out), plural(miss, "line"))
		out, miss = a.fillGaps(out, cues, name, system)
	}
	if miss > 0 {
		left := gapList(out) // named before they are filled, or there is nothing left to name
		for i := range out {
			if strings.TrimSpace(out[i]) == "" {
				out[i] = strings.ReplaceAll(cues[i].text, "\n", " / ")
			}
		}
		a.logfIdle("!!! subtitles: %s: %s left in the original (%s) -- the rest of the track is good",
			name, plural(miss, "line"), left)
	}
	// kept only once every line is there IN THE LANGUAGE ASKED FOR: a track
	// carrying lines of the original is one the next render should try again
	// rather than inherit
	if !hit && miss == 0 {
		a.keepReply("translate", ask, numberedText(out))
	}
	done := make([]subCue, len(cues))
	for i, c := range cues {
		done[i] = subCue{s: c.s, e: c.e, text: wrapSub(strings.ReplaceAll(out[i], " / ", "\n"))}
	}
	return done
}

// fillGaps asks again for the lines that did not come back, and only those.
// Their own numbers go with them, so the answer lands where it belongs however
// few of them there are; the wording says so, because a model given lines 7
// and 92 will otherwise answer 1 and 2.
//
// Never cached: this call exists BECAUSE the answer before it was short, and
// serving that from a file would repeat the gap for ever (the same rule the
// transcript pass keeps for its second attempt).
func (a *App) fillGaps(out []string, cues []subCue, name, system string) ([]string, int) {
	var want []int
	for i, s := range out {
		// a cue with no words in it is not asked about: there is nothing to
		// translate, and the answer to an empty line is an empty line
		if strings.TrimSpace(s) == "" && strings.TrimSpace(cues[i].text) != "" {
			want = append(want, i)
		}
	}
	if len(want) == 0 {
		return out, 0
	}
	var b strings.Builder
	for _, i := range want {
		fmt.Fprintf(&b, "%d\t%s\n", i+1, strings.ReplaceAll(cues[i].text, "\n", " / "))
	}
	user := a.ctxBlockFor("translate") + fmt.Sprintf(
		"TRANSLATE THESE %d LINES INTO %s. They are lines of a longer track, so their numbers "+
			"do not start at 1: answer with %d lines, each beginning with the number printed "+
			"in front of it here and a tab.\n\n%s", len(want), name, len(want), b.String())
	reply, err := a.llmChatRetry("translate",
		[]map[string]any{msg("system", system), msg("user", user)}, false)
	if err != nil {
		a.logfIdle("!!! subtitles: %s: %v", name, err)
		return out, len(want)
	}
	got, _ := numberedLines(reply, len(out))
	miss := 0
	for _, i := range want {
		if strings.TrimSpace(got[i]) != "" {
			out[i] = got[i]
			continue
		}
		miss++
	}
	return out, miss
}

// blankCues is how many of the lines that came back empty were empty going
// out. They are not missing translations, and counting them as missing spends
// a call on them and then reports them as lines the model lost.
func blankCues(cues []subCue, out []string) int {
	n := 0
	for i, c := range cues {
		if strings.TrimSpace(c.text) == "" && strings.TrimSpace(out[i]) == "" {
			n++
		}
	}
	return n
}

// gapList names the lines that did not come back, for the log: the first few
// by number, because "missing 3 of 123" is a number and "7, 64, 92" is
// something to go and look at.
func gapList(out []string) string {
	var at []string
	for i, s := range out {
		if strings.TrimSpace(s) != "" {
			continue
		}
		if len(at) == 5 {
			at = append(at, "…")
			break
		}
		at = append(at, strconv.Itoa(i+1))
	}
	return "line " + strings.Join(at, ", ")
}

// numberedText is the lines back in the shape they came in, which is what the
// cache holds: the answer to that request, whether it arrived in one piece or
// two (fillGaps).
func numberedText(lines []string) string {
	var b strings.Builder
	for i, s := range lines {
		fmt.Fprintf(&b, "%d\t%s\n", i+1, s)
	}
	return b.String()
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
	// the cues themselves, because the file is not the only thing written
	// from them: the same track goes out as WebVTT beside the video (vttText),
	// and re-reading the .srt to get them back would be a parser nobody needs.
	cues []subCue
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
	out := []subTrack{{code: own, tag: tag, name: name,
		path: filepath.Join(dir, "final.srt"), cues: cues}}
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
			path: filepath.Join(dir, "final."+code+".srt"), cues: done})
		if err := os.WriteFile(out[len(out)-1].path, []byte(srtText(done)), 0o644); err != nil {
			a.logfIdle("!!! subtitles: %s: %v", n, err)
			out = out[:len(out)-1]
		}
	}
	return out
}

// subSideFiles is every path this video's subtitles can occupy beside it: the
// spoken language under the video's own name, and one pair per language the
// app can translate into.
//
// Named exactly, never globbed. "final*.srt" beside a final.mp4 also matches
// the final2.srt of the render before it -- a different video, in the folder
// people actually keep these in -- and this list is used to DELETE the stale
// ones as well as to find them.
func subSideFiles(out string) []string {
	stem := strings.TrimSuffix(out, filepath.Ext(out))
	files := []string{stem + ".srt", stem + ".vtt"}
	for _, l := range subLangs {
		files = append(files, stem+"."+l.code+".srt", stem+"."+l.code+".vtt")
	}
	return files
}

// srtText is the cues as an .srt.
func srtText(cues []subCue) string {
	var b strings.Builder
	for i, c := range cues {
		fmt.Fprintf(&b, "%d\n%s --> %s\n%s\n\n", i+1, srtTime(c.s), srtTime(c.e), c.text)
	}
	return b.String()
}

// vttText is the same cues as WebVTT, which is the only subtitle format a
// browser reads.
//
// A <video> tag needs this and not the .srt: <track> parses WebVTT alone --
// point it at an .srt and Firefox fetches the file, fails to parse it and
// shows nothing -- and the tracks muxed INTO the mp4 are not offered by any
// browser at all, whatever VLC does with them. Two files for the same cues,
// because the player on the desk and the player on the page do not read the
// same one.
//
// The difference is a header, a full stop instead of a comma, and the three
// characters WebVTT treats as markup.
func vttText(cues []subCue) string {
	var b strings.Builder
	b.WriteString("WEBVTT\n\n")
	for i, c := range cues {
		fmt.Fprintf(&b, "%d\n%s --> %s\n%s\n\n", i+1, vttTime(c.s), vttTime(c.e), vttEscape(c.text))
	}
	return b.String()
}

// vttTime is an .srt stamp with the decimal point WebVTT wants.
func vttTime(t float64) string { return strings.Replace(srtTime(t), ",", ".", 1) }

// vttEscape keeps a cue's text from being read as markup: WebVTT allows
// <b>, <i> and &amp; inside a cue, so a spoken "R&D" or a "<" out of a
// transcript would be swallowed or break the cue.
func vttEscape(s string) string {
	s = strings.ReplaceAll(s, "&", "&amp;")
	s = strings.ReplaceAll(s, "<", "&lt;")
	return strings.ReplaceAll(s, ">", "&gt;")
}
