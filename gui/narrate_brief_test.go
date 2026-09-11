package main

// What the narration writer is actually told about a clip.
//
// This is the regression behind narration that reads well and describes
// something else: loadTSVRows took column 4 as the line, which is the line in a
// single recording's transcript and the SPEAKER_00/EVENT label in the merged
// session timeline. Narrate reads the merged one, so every narration request was
// built from a column of the words "SPEAKER_00" and "EVENT" -- the model knew a
// clip's length and nothing whatever about what happened in it, and wrote what
// a session like that usually contains.

import (
	"math"
	"os"
	"path/filepath"
	"regexp"
	"strings"
	"testing"
)

// Both shapes this app writes, read by the one function. Four columns is a
// single recording (inputs/<base>/transcript.tsv), five is the merged session
// timeline with the recording's name in between (understand/transcript/session.tsv).
func TestLoadTSVRowsReadsBothTimelineShapes(t *testing.T) {
	dir := t.TempDir()
	four := filepath.Join(dir, "transcript.tsv")
	if err := os.WriteFile(four, []byte(
		"4.40\t10.80\tSPEAKER_00\tOh my god. I'll be down.\n"+
			"17.60\t28.48\tSPEAKER_01\tCome back the way you came.\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	rows := loadTSVRows(four)
	if len(rows) != 2 {
		t.Fatalf("read %d rows from the four-column file, want 2", len(rows))
	}
	if rows[0].text != "Oh my god. I'll be down." || rows[0].spk != "SPEAKER_00" || rows[0].s != 4.40 {
		t.Errorf("row 0 = %+v", rows[0])
	}

	five := filepath.Join(dir, "session.tsv")
	if err := os.WriteFile(five, []byte(
		"742.00\t746.00\tgorillatag-0\tEVENT\tThe player leaps from a high vantage point.\n"+
			"748.00\t760.00\tgorillatag-0\tSPEAKER_00\tFind my three objects, those be mine.\n"+
			"765.60\t772.00\tmic-1\tSPEAKER_02\tIncrease your volume\tthen I'm recording.\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	rows = loadTSVRows(five)
	if len(rows) != 3 {
		t.Fatalf("read %d rows from the session timeline, want 3", len(rows))
	}
	if rows[0].spk != "EVENT" {
		t.Errorf("the screen description came back as speaker %q", rows[0].spk)
	}
	if rows[0].text != "The player leaps from a high vantage point." {
		t.Errorf("row 0's line is %q -- the narration is being written from the label column", rows[0].text)
	}
	if rows[1].text != "Find my three objects, those be mine." {
		t.Errorf("row 1's line is %q", rows[1].text)
	}
	// a tab inside a line keeps the whole line, not its tail
	if rows[2].text != "Increase your volume\tthen I'm recording." {
		t.Errorf("a line containing a tab came back as %q", rows[2].text)
	}
}

// The brief has to carry three things the model cannot guess: the words, which
// of them are the picture rather than the talk, and WHEN inside the clip each
// one falls. The last is what stops a line about the pickaxe that comes out in
// the final two seconds from being written over the forty before it.
func TestClipBriefsCarryTheWordsTheKindAndTheTiming(t *testing.T) {
	rows := []tsvRow{
		{s: 700, e: 704, spk: "EVENT", text: "The camera swings across a beach."},
		{s: 754, e: 758, spk: "EVENT", text: "A glowing green figure floats near a treasure chest."},
		{s: 758, e: 762, spk: "SPEAKER_00", text: "there's not much time"},
		{s: 798, e: 802, spk: "EVENT", text: "The player swings a pickaxe at a green wall."},
		{s: 900, e: 904, spk: "EVENT", text: "Everyone regroups on the dock."},
	}
	segs := []cutSeg{{S: 751, E: 800}, {S: 880, E: 890}}
	got := clipBriefs(segs, rows, nil, "")

	for _, want := range []string{
		"CLIP 1: 751.0–800.0 (49 s, at most 30 words -- fewer is better, none is fine)",
		"[+3s] EVENT: A glowing green figure floats near a treasure chest.",
		// no narrator named, so every line is assumed audible: the assumption
		// that cannot embarrass the narration
		"[+7s] SPEAKER_00: there's not much time",
		"[+47s] EVENT: The player swings a pickaxe at a green wall.",
	} {
		if !strings.Contains(got, want) {
			t.Errorf("the brief is missing %q:\n%s", want, got)
		}
	}
	// far outside the clip is not this clip's material
	if strings.Contains(got, "swings across a beach") {
		t.Errorf("a line 47 s before the clip was handed to it:\n%s", got)
	}
	// ...and a clip with nothing over it says so, rather than leaving the model
	// to fill an unexplained silence out of the story so far. Neutrally: four
	// passes read this brief now, and "invent nothing" is an instruction to
	// one of them -- the narration's own wording carries it (narrCraft rule 2),
	// while to the speed pass the same clip is simply a dull one.
	clip2 := got[strings.Index(got, "CLIP 2"):]
	if !strings.Contains(clip2, "nothing said, nothing described") {
		t.Errorf("a clip with no material says nothing about it:\n%s", clip2)
	}
	if !strings.Contains(narrCraft, "Add nothing nobody said and the pictures do not show") {
		t.Error("the narration wording no longer tells it to invent nothing")
	}
	// the offsets are what make the order legible, so they have to be there for
	// every line, not only the ones inside the clip
	if n := strings.Count(got, "[+"); n < 3 {
		t.Errorf("only %d stamped lines in:\n%s", n, got)
	}
}

// TestTheNarrationDoesNotTalkOverItself pins the rule that follows from how
// Produce mixes -- a clip whose own voices are kept plays them under the
// voice-over, so a narration that says one back is heard twice -- and, since
// the prompt was redone, the fact that it is a rule about SOME sessions.
//
// The everyday session here has the voices split off and the voice lane
// silenced: nothing anybody said is in the video, and there "say what was
// said, better" is the whole job. So the no-repeat rule lives in the note
// (narrNoMicNote) and rides along exactly when the scenes keep the speech,
// which speechHeard is the one reading of.
func TestTheNarrationDoesNotTalkOverItself(t *testing.T) {
	if strings.Contains(narrSystem, "VERBATIM") {
		t.Error("the prompt asks for the clip's own lines back as they stand")
	}
	for _, want := range []string{"never say one back", "heard by the viewer"} {
		if !strings.Contains(narrNoMicNote, want) {
			t.Errorf("the note never says %q, so nothing stops the narration repeating a clip that plays its voices", want)
		}
	}
	// ...and the base prompt is written for the other case, plainly enough
	// that a model has no doubt whose voice is in the video
	for _, want := range []string{"the only voice in the video", "nothing anybody said is played", "Say what was said, better"} {
		if !strings.Contains(narrSystem, want) {
			t.Errorf("the prompt no longer says %q", want)
		}
	}
	// and it does not invent to fill the silence it is handed
	if !strings.Contains(narrSystem, "Add nothing nobody said and the pictures do not show") {
		t.Error("the prompt no longer forbids inventing what it was not given")
	}
	// ...and the render is what makes it true: the original is ducked, not
	// dropped. If this ever becomes a replace, the rule above is wrong.
	b, err := os.ReadFile("produce.go")
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(b), "amix=inputs=%d:duration=first") ||
		!strings.Contains(string(b), `fmt.Sprintf("[bg]%samix`) {
		t.Error("the narration no longer mixes over the clip's own audio -- the prompt's " +
			"no-quoting rule was written for a mix and needs rereading")
	}
}

// TestTheNarrationIsCommentaryAndNotMemoir pins the difference between the two
// voices this prompt has had. Told it had been there and that this had happened
// to it, the model wrote a man remembering his own body: "I'm already
// spinning", "my hands are not listening", six entries out of nine opening with
// I'm, every clip a summary of what it had cost him. What the video is actually
// up against talks over the picture in the present with a verdict on every
// second of it, and the old prompt forbade exactly that in one line.
//
// The wording is pinned because there is nothing else to catch this: a prompt
// that has drifted back to memoir still parses, still fills every clip, and is
// only noticeable by watching the video.
func TestTheNarrationIsCommentaryAndNotMemoir(t *testing.T) {
	for _, gone := range []string{"You were there", "this happened to you",
		"Do not describe the picture"} {
		if strings.Contains(narrSystem, gone) {
			t.Errorf("the prompt says %q again, which is the voice that wrote "+
				"nine clips of what it had felt like", gone)
		}
	}
	for _, want := range []string{
		"Never report your own body", // the memoir, in one line
		// a spoken line is material to say properly, never a transcript to
		// read out: the runs of speech-to-text were what that looked like
		"never to quote as it stands",
		"broken speech-to-text",
	} {
		if !strings.Contains(narrSystem, want) {
			t.Errorf("the prompt no longer says %q", want)
		}
	}
	// the worked example is the load-bearing part of a prompt this size, so it
	// has to survive an edit to the rules above it
	if !strings.Contains(narrSystem, "  -> \"") {
		t.Error("the example of a clip block and the line it should get is gone")
	}
}

// TestTheNarrationLeavesRoom is the other half of it, and the one the second
// rewrite got wrong: told to have an opinion about every second, the model had
// an opinion about every second. Nine clips came back filled to a budget that
// was a speaking rate, so the voice-over ran from the first clip to the last
// with the same pause in the same place every time -- and a voice that talks
// over everything is not commenting on anything.
//
// Nothing downstream measures this. A wall-to-wall narration renders, plays and
// sounds professional in the first ten seconds, which is why the ask is pinned
// here in the words that carry it: the ceiling, the silence, and the empty line
// that makes a clip nobody talks over possible at all.
func TestTheNarrationLeavesRoom(t *testing.T) {
	for _, want := range []string{
		"Less is more", // the instruction, in the words the model reads first
		"ceiling across all its entries, not a target", // ...what the number under each clip means
		`gets text ""`,          // a clip may get no line at all
		`  -> ""`,               // and the example shows one, which is what gets copied
		"one or two clips",      // ...but as the exception: shown two examples of which
		"Most clips get a line", // one was "", the model went half-silent (5 of 9)
		"the offset it happens", // a line lands on a moment, not on a clip
		"it's a bit crowded",    // describing the picture is allowed when it is funny
	} {
		if !strings.Contains(narrSystem, want) {
			t.Errorf("the prompt no longer says %q -- the narration goes back to "+
				"talking through every second of every clip", want)
		}
	}

	// and the budget has to agree with it: the prompt can ask for restraint all
	// it likes while the brief hands out a word for every 0.4 s of video
	for _, c := range []struct {
		dur  float64
		want int
	}{{3, 8}, {12, 9}, {20, 15}, {49, 30}, {120, 30}, {600, 30}} {
		if got := narrBudget(c.dur); got != c.want {
			t.Errorf("a %.0f s clip is allowed %d words, want %d", c.dur, got, c.want)
		}
	}
	// the shape that matters more than any single number: what a clip is allowed
	// is a fraction of what fits in it. Speech runs at about 2.5 words a second.
	if narrBudget(40) > int(40*2.5)/3 {
		t.Error("a clip's word count is within reach of filling it end to end")
	}
}

// The row is one box, "[excited] Weee, ziplining!", but the wire is two
// fields: the TTS server takes the emotion as its own request parameter, and
// whatever stays in the text gets pronounced. lineParts/lineText are the seam,
// and a slip there either speaks the word "excited" out loud or loses the
// delivery -- both only audible, never visible.
func TestOneBoxCarriesEmotionAndWords(t *testing.T) {
	for _, c := range []struct {
		box, emo string
		at       float64
		hasAt    bool
		text     string
	}{
		{"[excited] Weee, ziplining!", "excited", 0, false, "Weee, ziplining!"},
		{"[excited @13] Weee, ziplining!", "excited", 13, true, "Weee, ziplining!"},
		{"[panicking, laughing @6.5] and down we go", "panicking, laughing", 6.5, true, "and down we go"},
		{"[@62] no emotion, placed", "", 62, true, "no emotion, placed"},
		{"no tag at all", "", 0, false, "no tag at all"},
		{"  [deadpan]   spaced out  ", "deadpan", 0, false, "spaced out"},
		{"[half open so it is words", "", 0, false, "[half open so it is words"},
		{"say it @9 sharp", "", 0, false, "say it @9 sharp"}, // an @ in the words is words
		{"", "", 0, false, ""},
	} {
		emo, at, hasAt, text := lineParts(c.box)
		if emo != c.emo || at != c.at || hasAt != c.hasAt || text != c.text {
			t.Errorf("lineParts(%q) = %q @%g(%v) + %q, want %q @%g(%v) + %q",
				c.box, emo, at, hasAt, text, c.emo, c.at, c.hasAt, c.text)
		}
	}
	// and the box shows what the parts will be read back from. The placement is
	// deliberately NOT rendered: the row's time field owns it, and a second
	// editable spelling of the same number is how the two drift apart.
	for _, c := range []struct {
		e    narrEntry
		want string
	}{
		{narrEntry{Text: "Weee, ziplining!", Emotion: "excited"}, "[excited] Weee, ziplining!"},
		{narrEntry{Text: "Weee, ziplining!", Emotion: "excited", At: 13}, "[excited] Weee, ziplining!"},
		{narrEntry{Text: "plain", At: 62}, "plain"},
		{narrEntry{Text: "", Emotion: "excited", At: 5}, ""}, // a silent clip shows nothing to speak
	} {
		if got := lineText(c.e); got != c.want {
			t.Errorf("lineText(%+v) = %q, want %q", c.e, got, c.want)
		}
	}
	// the round trip is what the save path actually does: words and delivery
	// survive the box, and the box never volunteers an @ to go stale
	for _, e := range []narrEntry{
		{Text: "Weee, ziplining!", Emotion: "excited", At: 13},
		{Text: "plain"},
	} {
		emo, _, hasAt, text := lineParts(lineText(e))
		if emo != e.Emotion || text != e.Text || hasAt {
			t.Errorf("round trip of %+v came back %q (hasAt %v) + %q", e, emo, hasAt, text)
		}
	}
}

// The eight base emotions are the TTS's actual vocabulary (IndexTTS2 maps the
// emotion text onto them): the prompt has to teach them to the writer, and the
// ⓘ has to teach them to the user, because "loud, angry, fast" reads as one
// third emotion and two thirds dilution. And the blend strength is part of the
// synthesis cache key -- a changed emoAlpha is a different performance, and
// serving the tamer take from cache would make the constant a lie.
func TestTheEmotionBasisIsTaught(t *testing.T) {
	for _, base := range []string{"happy", "angry", "sad", "afraid",
		"disgusted", "melancholic", "surprised", "calm"} {
		// taught in the system context, under narrate's answer, as the TTS's
		// own vocabulary rather than the wording's taste
		if !strings.Contains(strings.TrimSpace(sysSystem)+"\n\n"+narrSystem, base) {
			t.Errorf("the model is never told the base emotion %q", base)
		}
		found := false
		for _, s := range steps {
			found = found || strings.Contains(s.help, base)
		}
		if !found {
			t.Errorf("the ⓘ no longer names the base emotion %q", base)
		}
	}
	b, err := os.ReadFile("narrate_tts.go")
	if err != nil {
		t.Fatal(err)
	}
	a := &App{root: t.TempDir()}
	if !strings.Contains(a.ttsKey(narrEntry{Text: "hi"}), emoAlpha) {
		t.Error("emoAlpha is no longer part of the synthesis cache key — changing the " +
			"blend serves the old intensity from cache")
	}
	// ...and the request has to carry the emotion where the server reads it.
	// The endpoint parses input/language/options and DROPS unknown top-level
	// fields, and the engine's emotion path only runs behind use_emotion_text:
	// this client spent its whole life sending a top-level "emotion" that was
	// ignored, which is why "[excited] Yeah!" sounded like every other line.
	src := string(b)
	if !strings.Contains(src, `opts := emoOpts(emotion)`) {
		t.Error("the request no longer builds its options from emoOpts — the emotion is " +
			"back to being carried somewhere the server does not read")
	}
	if strings.Contains(src, `"emotion":   emotion`) {
		t.Error("speak() sends a top-level emotion field again, which the server ignores")
	}
	// a plain word is scored by the judge, and only reaches it behind its switch
	o := emoOpts("angry")
	if o["emotion_text"] != "angry" || o["use_emotion_text"] != "true" {
		t.Errorf(`emoOpts("angry") = %v, want the judge asked for by name`, o)
	}
	if o["emotion_alpha"] != emoAlpha {
		t.Errorf("a word-read line blends at %v, want emoAlpha", o["emotion_alpha"])
	}
	if o, ok := emoOpts("")["emotion_text"]; ok {
		t.Errorf("a line with no tag still asks for an emotion (%v)", o)
	}
}

// The weighted spelling: "[angry=1]" and "[happy=0.8, surprised=0.4]" go to the
// engine as the eight floats it mixes, skipping the judge that reads plain
// words. What this pins is the shape the server insists on -- exactly eight
// finite numbers, in ITS order -- and which tags take which path, because a tag
// that quietly fell back to the judge would look identical from here and sound
// like the thing the weights were written to stop.
func TestAWeightedEmotionIsSentAsAVector(t *testing.T) {
	for _, c := range []struct {
		tag  string
		want string
	}{
		{"angry=1", "0,1,0,0,0,0,0,0"},
		{"happy=0.8, surprised=0.4", "0.8,0,0,0,0,0,0.4,0"},
		{"calm=1", "0,0,0,0,0,0,0,1"},           // the eighth axis is "natural"
		{"melancholy=0.5", "0,0,0,0,0,0.5,0,0"}, // ...and kin resolve to their base
		{"furious=1", "0,1,0,0,0,0,0,0"},
		{"deadpan=0.6", "0,0,0,0,0,0,0,0.6"},
		{"angry=3", "0,1,0,0,0,0,0,0"},                // weights are a 0..1 blend, not a gain
		{"angry=1, angry=0.2", "0,1,0,0,0,0,0,0"},     // named twice: the louder ask wins
		{"angry, surprised=0.5", "0,1,0,0,0,0,0.5,0"}, // an unweighted name in a weighted tag is full
	} {
		got, ok := emoVector(c.tag)
		if !ok || got != c.want {
			t.Errorf("emoVector(%q) = %q,%v want %q,true", c.tag, got, ok, c.want)
		}
		if n := len(strings.Split(got, ",")); ok && n != 8 {
			t.Errorf("emoVector(%q) returned %d values, the server takes exactly 8", c.tag, n)
		}
	}
	// Everything else stays on the text path: the judge is forgiving where this
	// mapping is not, and a wrong axis is worse than a slower one.
	for _, tag := range []string{"angry", "surprised, happy", "", "smug=1",
		"angry=loud", "angry=0", "loud, angry, fast"} {
		if got, ok := emoVector(tag); ok {
			t.Errorf("emoVector(%q) = %q, want the judge to read it instead", tag, got)
		}
	}
	// ...and what the judge is handed has the weights taken off, so an axis this
	// client does not know still arrives as a word it can score.
	for _, c := range []struct{ in, want string }{
		{"angry", "angry"}, {"smug=1", "smug"}, {"wistful=1, smug=0.3", "wistful, smug"},
		{"surprised, happy", "surprised, happy"},
	} {
		if got := emoText(c.in); got != c.want {
			t.Errorf("emoText(%q) = %q, want %q", c.in, got, c.want)
		}
	}
	// The two ways in are exclusive, and the order matters: the engine tests
	// use_emotion_text BEFORE emotion_vector, so a request carrying both is a
	// request for the judge -- the weights would be read, sent, and thrown away.
	o := emoOpts("angry=1")
	if o["emotion_vector"] != "0,1,0,0,0,0,0,0" {
		t.Errorf(`emoOpts("angry=1") sent vector %v`, o["emotion_vector"])
	}
	if _, ok := o["use_emotion_text"]; ok {
		t.Error("a weighted line also asks for the judge, which overrides the weights it just sent")
	}
	if _, ok := o["emotion_text"]; ok {
		t.Error("a weighted line still carries emotion_text")
	}
	// ...and alpha multiplies those weights before use, so anything below 1
	// quietly rewrites "=1" as "=alpha" and fills the rest with the sample's own
	// reading. The weight is the intensity; the dial has to stand aside.
	if o["emotion_alpha"] != "1" {
		t.Errorf("a weighted line blends at %v — a written =1 arrives as =%v", o["emotion_alpha"], o["emotion_alpha"])
	}
}

// The engine has eight axes and no more, so "more emotions" can only mean more
// points inside them. A blend is a recipe over the eight: "excited" was listed
// as kin of happy, which is the nearest base and not the thing -- what makes it
// excited is the surprise in it, and read as plain happiness it came back
// merely pleased. What this pins is that the recipes stay reachable, stay
// mixtures rather than synonyms, and keep a weight meaning what it means
// everywhere else.
func TestABlendIsMoreThanItsNearestBase(t *testing.T) {
	for _, c := range []struct{ tag, want string }{
		// happy at full force, with over half as much surprise: the difference
		// between "a chest of coins" and "a nice day"
		{"excited=1", "1,0,0,0,0,0,0.55,0"},
		{"excited=0.5", "0.5,0,0,0,0,0,0.275,0"}, // the same recipe, read half as hard
		{"awed=1", "0.5,0,0,0.25,0,0,1,0"},
		{"frustrated=1", "0,1,0,0,0.35,0.5,0,0"},
		// a blend mixes with a base in one tag, and the louder ask still wins
		{"excited=1, angry=0.3", "1,0.3,0,0,0,0,0.55,0"},
		{"excited=1, surprised=1", "1,0,0,0,0,0,1,0"},
	} {
		got, ok := emoVector(c.tag)
		if !ok || got != c.want {
			t.Errorf("emoVector(%q) = %q,%v want %q,true", c.tag, got, ok, c.want)
		}
	}
	// no long tails: this string is read by the engine and is part of the take's
	// name, and 0.16499999999999998 is neither
	for _, tag := range []string{"excited=0.3", "awed=0.7", "tense=0.15", "ominous=0.42"} {
		got, _ := emoVector(tag)
		for _, f := range strings.Split(got, ",") {
			if len(f) > 5 {
				t.Errorf("emoVector(%q) = %q — %q is a floating-point tail, not a weight", tag, got, f)
			}
		}
	}
	// Every recipe peaks at 1 so that "=1" is the mix at full force wherever it
	// is written, spends itself on the eight the engine has, and is a mixture --
	// a one-axis "blend" is a kin word and belongs in emoBases.
	seen := map[string]string{}
	for _, b := range emoBlends {
		peak, axes := 0.0, 0
		for _, f := range b.V {
			if f < 0 || f > 1 {
				t.Errorf("blend %q has a weight of %g outside 0..1", b.Kin[0], f)
			}
			if f > 0 {
				axes++
			}
			peak = math.Max(peak, f)
		}
		if peak != 1 {
			t.Errorf("blend %q peaks at %g — \"=1\" would not be full force", b.Kin[0], peak)
		}
		if axes < 2 {
			t.Errorf("blend %q touches %d axis — that is a kin word, not a mix", b.Kin[0], axes)
		}
		// ...and no name may be claimed twice, in either table: the first match
		// would win silently and the second spelling would be dead.
		for _, k := range b.Kin {
			if emoTerm(k) >= 0 {
				t.Errorf("%q is both a base kin word and a blend", k)
			}
			if was, dup := seen[k]; dup {
				t.Errorf("%q is claimed by both the %q and %q blends", k, was, b.Kin[0])
			}
			seen[k] = b.Kin[0]
		}
	}
	// A take is named by the mix it asked for, not by the spelling: the same
	// request written two ways is one performance, and a recipe that changes
	// re-speaks the lines that used it instead of serving the old reading.
	a := &App{root: t.TempDir()}
	if a.ttsKey(narrEntry{Text: "hi", Emotion: "angry=1"}) != a.ttsKey(narrEntry{Text: "hi", Emotion: "furious=1"}) {
		t.Error("two spellings of one weighted request are two takes")
	}
	if !strings.Contains(a.ttsKey(narrEntry{Text: "hi", Emotion: "excited=1"}), "0.55") {
		t.Error("a weighted take is still named by its spelling — a changed recipe would serve the old performance")
	}
	// ...but a plain word is still keyed as written: those go to the judge, and
	// re-keying them would re-speak every line of every project for nothing.
	if !strings.Contains(a.ttsKey(narrEntry{Text: "hi", Emotion: "excited"}), "|excited") {
		t.Error("an unweighted tag no longer keeps its old key — every narrated project re-speaks")
	}
}

// The worked example must not be made of the session it will be run against.
// It was, once: the example quoted "Open up, FBI." -- a line off this repo's
// own test session -- and the model's handling of that real line became
// unreadable: quoted while the prompt showed it, avoided the moment the rules
// around it changed, with the replacement paraphrasing the example's own words.
// The fixture line below is the same one the vocabulary tests feed through the
// brief, which is exactly why the prompt may not contain it.
// What stays is the style line in the voice paragraph ("so many gorillas") --
// the user supplied it as the tone to hit, and tone is not material: the
// hazard is a quotable line or a scene the block can also contain.
func TestTheExampleIsInventedAndNotTheSessions(t *testing.T) {
	for _, gone := range []string{"Open up, FBI", "chest"} {
		if strings.Contains(narrSystem, gone) {
			t.Errorf("the prompt's example says %q, which is the test session's own "+
				"material -- the model cannot be read against footage its prompt "+
				"already used", gone)
		}
	}
}

// TestTheLineLandsWhereTheWriterPutIt: an entry carries "at", the second inside
// its clip where the line starts. Without it every line played at a fixed 0.3 s
// into its clip, which put "Open up, FBI" a minute before the chest was on
// screen and a sign-off 30 s before the video ended -- and made every pause the
// same length, because the line's position never depended on the clip. The
// placement has to hold in three places at once: the prompt asks for it, the
// preview starts the voice there, and the render's adelay puts it there in the
// mix. Each is edited separately and none notices the others drift.
func TestTheLineLandsWhereTheWriterPutIt(t *testing.T) {
	for _, want := range []string{
		`"at" is seconds from that clip's start`, // the field, defined where the model reads it
		"react to the vault after we have seen the vault",
		`"at":<sec>`, // ...and in the JSON it returns
		"a sign-off if the user context wants one", // the sign-off, on request
		"near that clip's end",                     // ...and sits at the end, not the head, of the last clip
	} {
		if !strings.Contains(strings.TrimSpace(sysSystem)+"\n\n"+narrSystem, want) {
			t.Errorf("the prompt no longer says %q", want)
		}
	}

	// the preview: a line placed at +20 into a 60 s clip is silence until then
	n := &narrator{entries: []narrEntry{{S: 100, E: 160, At: 20, Text: "there it is"}}}
	for _, c := range []struct {
		t    float64
		want int
	}{{100, -1}, {119.9, -1}, {120, 0}, {159, 0}, {160, -1}} {
		if got := n.entryAt(c.t); got != c.want {
			t.Errorf("at %.1f s the preview speaks entry %d, want %d", c.t, got, c.want)
		}
	}

	// the render: the mix delays the voice by the clip's own delay, not by the
	// constant, and the subtitles ride the same number
	b, err := os.ReadFile("produce.go")
	if err != nil {
		t.Fatal(err)
	}
	src := string(b)
	// (delayMS is that number in adelay's own units -- see
	// TestNarrationLandsWhereItWasPlaced, which pins the units against ffmpeg)
	if !strings.Contains(src, "adelay=%d:all=1") || !strings.Contains(src, "delayMS(ln.delay)") {
		t.Error("encodeClip no longer delays each voice to its line's placement")
	}
	// the whole-video track builds its cue at cum+ln.delay before tidyCues
	// holds the lines against each other; the burned-in one, per clip, at
	// ln.delay. Both are the placement the mix delays the voice by.
	if !strings.Contains(src, "cues = append(cues, subCue{s: cum + ln.delay,") || !strings.Contains(src, "srtTime(ln.delay)") {
		t.Error("the subtitles no longer start where the voice does")
	}
	if !strings.Contains(src, "ln.delay = math.Max(narrLead, ln.at)") {
		t.Error("the writer's placement never reaches the mix")
	}

	// ...and the row's ▶ cues just ahead of the line, not the head of the clip:
	// with a placement a minute in, an audition from the top is a minute of
	// silence that reads as a line that was never spoken
	tts, err := os.ReadFile("narrate_tts.go")
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(tts), "n.cue(n.leadIn(i), true)") {
		t.Error("the audition no longer cues to the line's placement")
	}
}

// An entry with no words is a supported answer, not a broken one, and three
// places have to agree about that or the feature is worse than not having it:
// the writer must not reject the model for using it, the preview must not stop
// the picture and send the empty string to the TTS server, and the render must
// leave that clip on its own audio. The first two are checked here; the third is
// produce.go's `strings.TrimSpace(e.Text) != ""` before it takes a wav.
func TestAClipWithNoLineIsLeftAlone(t *testing.T) {
	n := &narrator{entries: []narrEntry{
		{S: 0, E: 10, Text: "look at this idiot"},
		{S: 10, E: 20, Text: ""},
		{S: 20, E: 30, Text: "   "},
		{S: 30, E: 40, Text: "and he does it again"},
	}}
	for _, c := range []struct {
		t    float64
		want int
	}{{1, 0}, {9.9, 0}, {10, -1}, {19, -1}, {25, -1}, {35, 3}} {
		if got := n.entryAt(c.t); got != c.want {
			t.Errorf("at %.1f s the preview would speak entry %d, want %d", c.t, got, c.want)
		}
	}
	if !strings.Contains(narrSystem, `still an entry`) {
		t.Error("the prompt no longer says an empty line is still an entry, and the " +
			"writer rejects any answer whose entry count does not match the cut")
	}
	// the writer's own validation: an empty line passes, an answer that is
	// nothing but empty lines does not
	b, err := os.ReadFile("narrate.go")
	if err != nil {
		t.Fatal(err)
	}
	src := string(b)
	if strings.Contains(src, `problem = "empty text"`) {
		t.Error("writeNarration still rejects a clip the model chose to leave silent")
	}
	if !strings.Contains(src, `said == 0`) {
		t.Error("nothing catches a narration that came back empty from end to end")
	}
}

// The prompt describes the format the brief is written in. They are edited in
// different places -- one is a const, the other is a box in the project the
// user can rewrite -- so this pins the words that have to mean the same thing
// in both.
func TestTheNarratePromptDescribesTheBriefItGets(t *testing.T) {
	// the labels and the offset stamp are described once, in the system
	// context this prompt is sent behind, so the promise is read off both.
	told := strings.TrimSpace(sysSystem) + "\n\n" + narrSystem
	for _, want := range []string{"EVENT", "SPEAKER_01", "NARRATOR", "[+2.0s]", "offsets from that clip's start"} {
		if !strings.Contains(told, want) {
			t.Errorf("the narration prompt never mentions %q, so the model is left to "+
				"work out the shape of its own input", want)
		}
	}
	brief := clipBriefs([]cutSeg{{S: 0, E: 10}}, []tsvRow{
		{s: 1, e: 2, spk: "EVENT", text: "x"},
		{s: 3, e: 4, spk: "SPEAKER_01", text: "y", src: "capture"},
		{s: 5, e: 6, spk: "SPEAKER_00", text: "z", src: "his-own-mic"},
	}, nil, "his-own-mic")
	for _, want := range []string{"EVENT:", "SPEAKER_01:", "NARRATOR:"} {
		if !strings.Contains(brief, want) {
			t.Errorf("the brief no longer marks %q, which the prompt says it will:\n%s", want, brief)
		}
	}
}

// TestOnlyWhatTheVideoCarriesIsOffLimits: the no-quoting rule is about a
// collision in the mix, not about ownership of the words. The render takes each
// clip's sound from the footage it was cut from; the narrator's own microphone
// -- which is where a session's best lines usually are, because that is the
// good mic on the person talking, and because he is the one player the capture
// never plays back -- is transcribed, aligned, and then never heard again.
// Forbidding the narration to say those lines forbids it the only voice that
// will ever carry them.
//
// And the label is the name of the person, not a note about the mix: the model
// is told NARRATOR, because that is the word it already knows what to do with.
func TestOnlyWhatTheVideoCarriesIsOffLimits(t *testing.T) {
	// the merged timeline says which recording each line came off, and that has
	// to survive the read: it is the whole basis of the distinction
	dir := t.TempDir()
	f := filepath.Join(dir, "session.tsv")
	if err := os.WriteFile(f, []byte(
		"646.08\t648.96\this-own-mic\tSPEAKER_00\tOpen up, FBI.\n"+
			"651.20\t653.20\tcapture-0\tSPEAKER_01\topen this chest.\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	rows := loadTSVRows(f)
	if len(rows) != 2 || rows[0].src != "his-own-mic" || rows[1].src != "capture-0" {
		t.Fatalf("the recording each line came off was dropped: %+v", rows)
	}

	// whoever wears the narrator tag is the exempt one, and it is the tag that
	// says so -- the same tag the voice picker clones from
	a := &App{root: dir, outDir: dir,
		selVid: []string{"/in/capture-0.mp4"}, selAud: []string{"/in/his-own-mic.flac"}}
	a.selNarr[0] = "/in/his-own-mic.flac"
	if got := a.narratorMic(); got != "his-own-mic" {
		t.Fatalf("narratorMic = %q -- the narration is quoting the wrong microphone", got)
	}
	// a session that is one capture and nothing else has no exempt recording:
	// then the narrator is on the footage like everybody else
	if got := (&App{root: dir, outDir: dir, selVid: []string{"/in/capture-0.mp4"}}).narratorMic(); got != "" {
		t.Errorf("a footage-only session exempts %q, which the video plays out loud", got)
	}

	brief := clipBriefs([]cutSeg{{S: 640, E: 660}}, rows, nil, a.narratorMic())
	if !strings.Contains(brief, "NARRATOR: Open up, FBI.") {
		t.Errorf("the joke is not marked quotable:\n%s", brief)
	}
	if !strings.Contains(brief, "SPEAKER_01: open this chest.") {
		t.Errorf("a line the video carries is not marked audible:\n%s", brief)
	}

	// ...and the prompt has to ask for it, or the mark means nothing. It asks
	// for all of them now rather than one per clip: a line nobody will ever
	// hear is material, and rationing it was rationing the session's own words
	for _, want := range []string{"NARRATOR", "material nobody will ever hear unless you use it"} {
		if !strings.Contains(narrSystem, want) {
			t.Errorf("the prompt never says %q, so the narration stays silent about "+
				"lines nobody else will ever say", want)
		}
	}
	// and where the video DOES carry them, the note says so instead
	if !strings.Contains(narrNoMicNote, "The NARRATOR lines are still yours") {
		t.Error("the note takes the narrator's own lines away along with the speakers'")
	}

	// the fact the whole distinction rests on: a clip's audio is its video's,
	// plus the narration. If the mic recordings are ever mixed in, every line
	// above becomes a collision again and these rules need rereading.
	b, err := os.ReadFile("produce.go")
	if err != nil {
		t.Fatal(err)
	}
	enc := string(regexp.MustCompile(`(?s)func \(a \*App\) encodeClip\(.*?\n}\n`).Find(b))
	if enc == "" {
		t.Fatal("encodeClip is gone")
	}
	if !strings.Contains(enc, `game := "0:a"`) || strings.Contains(enc, "selAud") {
		t.Error("the render no longer takes a clip's audio from its own video alone")
	}
}

// A pause is an entry, not punctuation. The voice runs straight through a
// comma, a dash and a full stop -- IndexTTS2 reads a line as one breath -- so
// a beat written into the words is a beat that never happens, and the setup
// and the punchline come out in one flat run. The only pause the render can
// make is the silence between two entries, which it already has: a clip may
// carry several, each with its own "at" and its own emotion (bindEntries).
//
// So the prompt has to say that in the words the writer reads, and the
// example has to show it -- an example of one line with dots in it is what
// gets copied.
func TestAPauseIsAnEntryAndNotPunctuation(t *testing.T) {
	for _, want := range []string{
		"A pause is an entry, not punctuation",
		"runs straight through a comma",        // why it cannot be written in
		"ending the line and starting another", // ...and what to do instead
		"two and a half words a second",        // the arithmetic that places the second one
		"its own \"at\" and its own emotion",
	} {
		if !strings.Contains(narrSystem, want) {
			t.Errorf("the prompt no longer says %q, so a pause goes back to being a comma", want)
		}
	}
	// the worked example is the load-bearing half: one thought per entry,
	// seconds apart, and the prompt says that is what it is showing
	if !strings.Contains(narrSystem, "The first clip is what a pause looks like") {
		t.Error("nothing names the example as an example of a pause")
	}
	if n := strings.Count(narrSystem, "  -> at "); n < 5 {
		t.Errorf("the examples show %d placed lines, want enough to show a clip carrying several", n)
	}
	// ...and the emotion moves across them, which is the other half of what
	// makes a pause land
	for _, want := range []string{"[calm]", "[happy]", "the setup calm, the reaction surprised"} {
		if !strings.Contains(narrSystem, want) {
			t.Errorf("the prompt no longer says %q", want)
		}
	}
	// the render can actually do it: several entries on one clip, each placed
	// by its own "at", is what bindEntries accepts
	segs := []cutSeg{{S: 100, E: 160}}
	got, problem := bindEntries(segs, []rawEntry{
		{Start: 100, End: 160, At: 10, Text: "Nobody has the key.", Emotion: "calm"},
		{Start: 100, End: 160, At: 13, Text: "Housekeeping!", Emotion: "happy"},
	})
	if problem != "" {
		t.Fatalf("two entries on one clip were refused: %s", problem)
	}
	if len(got) != 2 || got[0].At != 10 || got[1].At != 13 {
		t.Errorf("the pause did not survive binding: %+v", got)
	}
	if got[0].Emotion != "calm" || got[1].Emotion != "happy" {
		t.Errorf("each entry did not keep its own emotion: %+v", got)
	}

	// and a two-word line is allowed when it is the funny one -- the prompt
	// used to forbid every one of them
	if strings.Contains(narrSystem, "never a two-word caption") {
		t.Error("the prompt still bans a two-word line outright")
	}
	if !strings.Contains(narrSystem, "two words are a whole line when they are the right two") {
		t.Error("the prompt no longer allows the short funny line")
	}
}
