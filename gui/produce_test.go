package main

import (
	"math"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// The subtitle track used to be the narration's lines and nothing else, so a
// read to camera -- nobody narrating, the speech IS the video -- asked for a
// track in the file and got a file with no track in it, and a final.srt of
// zero bytes. With no narration the track carries what was said in each clip,
// built from the ALIGNED WORDS that survive the cut.
func TestSubtitlesFallBackToWhatWasSaid(t *testing.T) {
	v := &tlVideo{base: "cam", path: "/f/cam.mkv", start: 100, dur: 60}
	clips := []prodClip{
		{idx: 0, video: v, local: 10, sessS: 110, length: 20, tempo: 1, rate: 1},              // 110..130
		{idx: 1, video: v, local: 40, sessS: 140, length: 10, tempo: 1, rate: 0.5},            // 140..145 at half speed
		{idx: 2, ins: "/f/card.png", sessS: 145, length: 3, tempo: 1, rate: 1},                // a card: no speech
		{idx: 3, video: v, local: 50, sessS: 150, length: 5, tempo: 1, rate: 1, freeze: true}, // a held frame
	}
	words := []srcWord{
		{s: 112, e: 112.4, w: "inside", raw: "Inside", src: "cam"},
		{s: 112.4, e: 112.8, w: "it", raw: "it.", src: "cam"},
		{s: 114, e: 114.4, w: "the", raw: "the", src: "mic"}, // the narrator's own microphone
		{s: 141, e: 141.4, w: "slow", raw: "slow", src: "cam"},
		{s: 151, e: 151.4, w: "under", raw: "under", src: "cam"},
	}
	transcriptSubs(clips, words, "mic")
	got := captionLines(clips[0])
	if len(got) != 1 {
		t.Fatalf("clip 0 carries %d captions, want 1: %+v", len(got), got)
	}
	// the words as they are WRITTEN, not as they are matched
	if got[0].text != "Inside it." {
		t.Errorf("the caption reads %q, want %q -- case and punctuation kept", got[0].text, "Inside it.")
	}
	if math.Abs(got[0].at-2) > 1e-9 || math.Abs(got[0].dur-0.8) > 1e-9 {
		t.Errorf("the caption is at %.2f for %.2f, want 2 for 0.8", got[0].at, got[0].dur)
	}
	if got[0].wav != "" {
		t.Error("a transcript caption claims a wav: it would be mixed as narration")
	}
	// half speed: the moment lands twice as late and the line runs twice as long
	if s := captionLines(clips[1]); len(s) != 1 || math.Abs(s[0].at-2) > 1e-9 || math.Abs(s[0].dur-0.8) > 1e-9 {
		t.Errorf("under slow motion the caption is %+v, want at 2 for 0.8", s)
	}
	// a card and a held frame carry nothing
	for _, i := range []int{2, 3} {
		if n := len(captionLines(clips[i])); n != 0 {
			t.Errorf("clip %d (no running speech) carries %d captions", i, n)
		}
	}
	// ...and where there IS a narration, its lines are the track, not these
	clips[0].lines = []prodLine{{text: "the voice-over", wav: "/x.wav", dur: 3}}
	if got := captionLines(clips[0]); len(got) != 1 || got[0].text != "the voice-over" {
		t.Errorf("a narrated clip's track is not the narration: %+v", got)
	}
	// the render no longer turns the track off just because narration is off
	src := readSrc(t, "produce.go")
	if strings.Contains(src, `st.Subs = "none" // no lines, no track`) {
		t.Error("no narration still means no subtitles")
	}
	if !strings.Contains(src, "transcriptSubs(clips, a.sessionWords(") {
		t.Error("the render does not build the captions from the aligned words")
	}
}

// A transcript line is not what the video says. The lines come off Prepare,
// before the cut, and the cut removes stretches inside them: clipped to a clip
// the line kept its whole TEXT, so the join at 1:45 read "...public state of
// the art, and the whole run took roughly 13" over footage that says only
// "...public state of the art" -- words not in the video, read again when the
// retake plays. Eleven of that session's eighty-one lines were cut into.
func TestACaptionSaysOnlyTheWordsTheClipKeeps(t *testing.T) {
	// the line, and the clip that keeps its first four words
	words := []srcWord{
		{s: 103.5, e: 104.0, w: "public", raw: "public", src: "cam"},
		{s: 104.0, e: 104.7, w: "state", raw: "state", src: "cam"},
		{s: 104.7, e: 104.9, w: "of", raw: "of", src: "cam"},
		{s: 105.0, e: 105.5, w: "the", raw: "the", src: "cam"},
		{s: 105.5, e: 105.9, w: "art", raw: "art,", src: "cam"},
		{s: 106.0, e: 106.4, w: "and", raw: "and", src: "cam"}, // dropped by the cut
		{s: 106.4, e: 109.2, w: "the", raw: "the whole run", src: "cam"},
	}
	v := &tlVideo{base: "cam", path: "/f/cam.mkv", start: 100, dur: 60}
	clips := []prodClip{{idx: 0, video: v, sessS: 103.5, length: 2.4, tempo: 1, rate: 1}} // 103.5..105.9
	transcriptSubs(clips, words, "")
	got := captionLines(clips[0])
	if len(got) != 1 {
		t.Fatalf("%d cues, want 1: %+v", len(got), got)
	}
	if got[0].text != "public state of the art," {
		t.Errorf("the caption reads %q -- it says words the clip does not play", got[0].text)
	}
}

// A cue ends at a breath, or when it holds as much as two rows do: a player
// wraps what it is given at its own font size, so one long cue becomes four
// lines across the picture.
func TestACaptionBreaksAtABreathAndAtTwoRows(t *testing.T) {
	var words []srcWord
	at := 0.0
	for i := 0; i < 30; i++ {
		words = append(words, srcWord{s: at, e: at + 0.3, w: "wordword", raw: "wordword", src: "cam"})
		at += 0.35
	}
	// ...with a breath in the middle of it
	words[10].s += subBreak
	words[10].e += subBreak
	for i := 11; i < len(words); i++ {
		words[i].s += subBreak
		words[i].e += subBreak
	}
	got := wordCues(words, 0, 1, 100)
	if len(got) < 3 {
		t.Fatalf("thirty long words came out as %d cue(s): %+v", len(got), got)
	}
	for i, ln := range got {
		if len(ln.text) > 2*subRowChars {
			t.Errorf("cue %d is %d characters, more than two rows hold", i+1, len(ln.text))
		}
		if ln.dur > subCueMax+0.5 {
			t.Errorf("cue %d stays up %.1fs", i+1, ln.dur)
		}
	}
	// the breath is a break: no cue spans it
	for _, ln := range got {
		if ln.at < words[9].e && ln.at+ln.dur > words[10].s {
			t.Errorf("a cue spans the breath: %+v", ln)
		}
	}
}

// The cues come off the transcript one line each, and the gap between two
// lines of one sentence is the breath between them -- five, twenty, eighty
// milliseconds. At thirty frames a second each is the text blanking for a
// frame and coming back, and the track flickers all the way through. The line
// already up is held across such a gap.
func TestTheSubtitleTrackDoesNotFlicker(t *testing.T) {
	got := tidyCues([]subCue{
		{s: 0, e: 11.84, text: "first"},
		{s: 11.86, e: 14.8, text: "second"}, // 20 ms later: a breath
		{s: 15.04, e: 18.0, text: "third"},  // 240 ms: still a breath
		{s: 21.0, e: 24.0, text: "fourth"},  // 3 s: a real pause, left alone
		{s: 23.5, e: 26.0, text: "fifth"},   // overlapping the one before it
	})
	if len(got) != 5 {
		t.Fatalf("%d cues, want 5: %+v", len(got), got)
	}
	for i, want := range []float64{11.86, 15.04, 18.0, 23.5, 26.0} {
		if math.Abs(got[i].e-want) > 1e-9 {
			t.Errorf("cue %d ends at %.3f, want %.3f", i+1, got[i].e, want)
		}
	}
	if got[3].s != 21.0 {
		t.Errorf("a real pause was closed too: cue 4 starts at %.2f", got[3].s)
	}
	// nothing on screen for less than a reader needs: a flash of a word is
	// folded into the line that follows it
	got = tidyCues([]subCue{{s: 0, e: 0.1, text: "so"}, {s: 5, e: 9, text: "that is it"}})
	if len(got) != 1 || got[0].text != "so\nthat is it" || got[0].s != 0 {
		t.Errorf("a flash was not folded into the next line: %+v", got)
	}
	// ...and a last line with nothing after it is given the time to read it
	got = tidyCues([]subCue{{s: 0, e: 4, text: "first"}, {s: 10, e: 10.2, text: "last"}})
	if len(got) != 2 || math.Abs(got[1].e-(10+subMin)) > 1e-9 {
		t.Errorf("the last line keeps the time it was spoken in: %+v", got)
	}
	// the render builds the track through it
	if !strings.Contains(readSrc(t, "produce.go"), "cues = tidyCues(cues)") {
		t.Error("the render writes the cues untidied")
	}
}

// The aligner answers in bare lowercase -- it hands back what it matched, not
// what it was given -- so the times are right and the writing is gone. The
// transcript is what it was handed, word for word, so the spelling comes from
// there: a subtitle is read, and "eastern university of applied sciences" is
// not what the video says.
func TestTheWordsGetTheirSpellingBack(t *testing.T) {
	words := []srcWord{{w: "welcome"}, {w: "back"}, {w: "this"}, {w: "is"},
		{w: "the"}, {w: "eastern"}, {w: "university"}}
	dressWords(words, "Welcome back. This is the Eastern University of Applied Sciences")
	var got []string
	for _, w := range words {
		got = append(got, w.raw)
	}
	if strings.Join(got, " ") != "Welcome back. This is the Eastern University" {
		t.Errorf("dressed as %q", strings.Join(got, " "))
	}
	// a word the aligner dropped does not throw the walk off
	words = []srcWord{{w: "welcome"}, {w: "this"}, {w: "is"}}
	dressWords(words, "Welcome back. This is")
	if words[1].raw != "This" || words[2].raw != "is" {
		t.Errorf("a dropped word lost the walk its place: %+v", words)
	}
	// ...and a word that is nowhere in the transcript keeps its bare form
	words = []srcWord{{w: "welcome"}, {w: "zzz", raw: "zzz"}, {w: "back"}}
	dressWords(words, "Welcome back.")
	if words[1].raw != "zzz" || words[2].raw != "back." {
		t.Errorf("an unplaceable word broke the rest: %+v", words)
	}
	if !strings.Contains(readSrc(t, "retake.go"), `dressWords(mine, readFileString(filepath.Join(dir, "transcript.txt")))`) {
		t.Error("the session's words are never dressed")
	}
}

// A translation is the same cues with their text in another language and their
// times untouched, read back BY NUMBER: a model that drops a line or wraps one
// in two would otherwise shift every line after it onto the wrong seconds,
// which is the one mistake here nobody can see.
func TestATranslationKeepsEveryLineOnItsOwnSeconds(t *testing.T) {
	got, miss := numberedLines("1\tWillkommen zurück.\n2\tMein Name ist Thomas.\n3\tHeute geht es um drei Themen.", 3)
	if miss != 0 || got[0] != "Willkommen zurück." || got[2] != "Heute geht es um drei Themen." {
		t.Errorf("a clean answer read back as %+v (%d missing)", got, miss)
	}
	// out of order, and wrapped in prose the model added: each line still
	// lands on its own number
	got, miss = numberedLines("Here you go:\n\n3\tdrei\n1\teins\n2\tzwei\n", 3)
	if miss != 0 || got[0] != "eins" || got[1] != "zwei" || got[2] != "drei" {
		t.Errorf("an out-of-order answer read back as %+v (%d missing)", got, miss)
	}
	// "3. text" and "3 text", which a model answers when it forgets the tab
	if got, miss := numberedLines("1. eins\n2 zwei", 2); miss != 0 || got[0] != "eins" || got[1] != "zwei" {
		t.Errorf("a dotted answer read back as %+v (%d missing)", got, miss)
	}
	// a missing line is COUNTED, not papered over: the track is left out
	// rather than shown a line late all the way down
	if _, miss := numberedLines("1\teins\n3\tdrei", 3); miss != 1 {
		t.Errorf("a dropped line was not noticed: %d missing, want 1", miss)
	}
	// a number outside the track cannot overwrite one inside it
	if got, miss := numberedLines("1\teins\n99\tnope", 2); miss != 1 || got[0] != "eins" {
		t.Errorf("a line number off the end landed somewhere: %+v (%d missing)", got, miss)
	}
	// the render only translates where there is somewhere to put it -- not
	// burned into the one picture, not when there are no subtitles at all
	src := readSrc(t, "produce.go")
	if !strings.Contains(src, `if cue > 0 && (st.Subs == "mux" || st.Subs == "sidecar") {`) {
		t.Error("the render translates for a track it is not going to write")
	}
	// every track gets its own language tag, or a player lists two "English"
	if !strings.Contains(src, `fmt.Sprintf("-metadata:s:s:%d", i), "language="+t.tag`) {
		t.Error("the muxed tracks are not tagged with their own languages")
	}
}

// The session's own language is always written and is never a translation of
// itself: ticking it in the menu must not produce two of the same track.
func TestTheSessionsOwnLanguageIsNotTranslated(t *testing.T) {
	a := &App{}
	dir := t.TempDir()
	if err := os.WriteFile(filepath.Join(dir, "final.srt"), []byte("x"), 0o644); err != nil {
		t.Fatal(err)
	}
	cues := []subCue{{s: 0, e: 1, text: "hello"}}
	// The language box is EMPTY by default and shows "en" as a placeholder,
	// so the project's own answer is "" until somebody types in it -- and ""
	// matches no language, which is why English stayed in the menu of an
	// English session. asrLanguage is that answer with the default filled in.
	if a.projectLanguage() != "" || a.asrLanguage() != defLanguage {
		t.Fatalf("an untouched language box reads %q / %q", a.projectLanguage(), a.asrLanguage())
	}
	// defLanguage with defLanguage ticked: one track, no call made (a call
	// would fail in a test with no server, and the count proves none was)
	got := a.subTracks(cues, dir, []string{a.asrLanguage()})
	if len(got) != 1 || got[0].code != a.asrLanguage() {
		t.Errorf("the session's own language came back as %d track(s): %+v", len(got), got)
	}
	// ...and a language nobody offers is not attempted either
	if got := a.subTracks(cues, dir, []string{"klingon"}); len(got) != 1 {
		t.Errorf("an unknown language was attempted: %+v", got)
	}
	// no cues, no tracks
	if got := a.subTracks(nil, dir, []string{"de"}); got != nil {
		t.Errorf("a track was written for a video with no subtitles: %+v", got)
	}
}

// The picker is a menu of ticks, and the button says which are on so the
// answer is readable with the menu shut.
func TestTheTranslationPickerSaysWhatIsOn(t *testing.T) {
	var codes []string
	for _, l := range subLangs {
		codes = append(codes, l.code)
	}
	if strings.Join(codes, ",") != "en,de,fr" {
		t.Errorf("the languages offered are %v, want en, de, fr", codes)
	}
	for _, l := range subLangs {
		tag, name, ok := subLangOf(l.code)
		if !ok || len(tag) != 3 || name == "" {
			t.Errorf("%q has no three-letter tag a container can carry: %q %q", l.code, tag, name)
		}
	}
	if _, _, ok := subLangOf("klingon"); ok {
		t.Error("a language nobody offers was resolved")
	}
	src := readSrc(t, "produce.go")
	for _, want := range []string{
		`SubLangs []string `, // stored with the project
		"SubLangs:  p.pickedLangs(),",
		"func (p *producer) syncLangs()",
		// the language the session is spoken in is not offered: its track is
		// written anyway, and a tick for it would buy a call that answers
		// with what it was given
		"t.SetVisible(l.code != own)",
		"if l.code == own {",
		"own := p.a.asrLanguage()", // not projectLanguage: that is "" until typed in

		// ...and the menu follows the box on the other page that says which
		// language that is
		"a.syncSubLangs()",
		// the two subtitle answers share a row: one is about the other
		"subRow.Append(p.subs)",
		`p.subsLbl = lbl(subs, 0, 0, "Subtitles:", subRow)`,
	} {
		if !strings.Contains(src, want) {
			t.Errorf("produce.go does not contain %q", want)
		}
	}
	if !strings.Contains(readSrc(t, "main.go"), "a.syncSubLangs()") {
		t.Error("changing the session's language does not change what can be translated into")
	}
	// safe before the page exists: a project loads first
	(&App{}).syncSubLangs()
}
