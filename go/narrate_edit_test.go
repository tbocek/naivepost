package main

// The Narrate page's manual editing: lines are added at the playhead, removed
// per row, and warned about when their words outrun their slot. The ✎ notes
// are gone -- the lines themselves are the editing surface -- so what these
// pin is the arithmetic that editing rests on: where a new line may go (only
// inside the cut), what a removed line leaves behind (a clip that still says
// "leave me alone"), and how long a line may speak.

import (
	"math"
	"os"
	"regexp"
	"strings"
	"testing"
)

func editNarr(t *testing.T) *narrator {
	t.Helper()
	root := t.TempDir()
	a := &App{root: root, outDir: root}
	a.ed = &cutEditor{segs: []cutSeg{{S: 100, E: 130}, {S: 200, E: 260}}}
	return &narrator{a: a, entries: []narrEntry{
		{S: 100, E: 130, At: 5, Text: "first clip line"},
		{S: 200, E: 260, Text: ""}, // deliberately silent
	}}
}

func TestAddLineAtRespectsTheCut(t *testing.T) {
	n := editNarr(t)

	// between clips there is nothing to narrate: the cut is Cut's
	if i := n.addLineAt(150); i != -1 {
		t.Fatalf("a line was added in the gap between clips (row %d)", i)
	}
	if len(n.entries) != 2 {
		t.Fatalf("the refused line still changed the entries: %+v", n.entries)
	}

	// inside clip 1, after the existing line: inserted in placement order
	i := n.addLineAt(120)
	if i != 1 {
		t.Fatalf("the new line landed at row %d, want 1 (after the clip's first line)", i)
	}
	e := n.entries[1]
	if e.S != 100 || e.E != 130 || e.At != 20 || e.Text != "" {
		t.Fatalf("the new line is %+v, want the clip's bounds with at=20 and no words yet", e)
	}

	// ...and before it: placement order, not click order
	i = n.addLineAt(102)
	if i != 0 || n.entries[0].At != 2 || n.entries[1].At != 5 {
		t.Fatalf("a line placed earlier did not sort before the later one: %+v", n.entries)
	}

	// the silent clip's marker becomes the line instead of gaining a sibling
	before := len(n.entries)
	i = n.addLineAt(230)
	if len(n.entries) != before {
		t.Fatalf("adding to a silent clip grew the entries: %+v", n.entries)
	}
	if got := n.entries[i]; got.S != 200 || got.At != 30 || got.Text != "" {
		t.Fatalf("the silent marker did not become the line: %+v", got)
	}

	// the final second is not a place a line can start (the render could
	// never finish it there)
	i = n.addLineAt(259.8)
	if got := n.entries[i]; got.At > 59 {
		t.Fatalf("a line may start %0.1f s into a 60 s clip", got.At)
	}
}

// A second can only hold one line: + where one already starts jumps to it,
// and + where one is still speaking refuses -- the duplicate rows the button
// used to leave behind were exactly these two presses.
func TestAddLineAtRefusesAnOccupiedSecond(t *testing.T) {
	n := editNarr(t)
	n.entries = []narrEntry{
		{S: 100, E: 130, At: 5, Text: "this line has enough words in it to keep the narrator talking five seconds"}, // ~5 s: speaks 105-110
	}

	// where the line starts: the press means "this line", not "another one"
	if i := n.addLineAt(105.4); i != 0 || len(n.entries) != 1 {
		t.Fatalf("+ on a line's own start made row %d of %d entries", i, len(n.entries))
	}
	// while its audio is speaking: no room, refuse
	if i := n.addLineAt(108); i != -1 || len(n.entries) != 1 {
		t.Fatalf("+ inside a speaking line made row %d of %d entries", i, len(n.entries))
	}
	// the second after it ends is free again
	if i := n.addLineAt(112); i != 1 || n.entries[1].At != 12 {
		t.Fatalf("+ after the line's audio: row %d, entries %+v", i, n.entries)
	}
	// pressing + again right there is the jump, not a duplicate -- this is the
	// double-press that used to stack empty rows
	if i := n.addLineAt(112.3); i != 1 || len(n.entries) != 2 {
		t.Fatalf("the second press duplicated: row %d of %d entries", i, len(n.entries))
	}
}

// The row's own +: a line starting where this one's audio ends plus a beat.
func TestAddLineAfterLandsPastTheAudio(t *testing.T) {
	n := editNarr(t)
	n.entries = []narrEntry{
		{S: 100, E: 130, At: 5, Text: "this line has enough words in it to keep the narrator talking five seconds"}, // ends ~110
		{S: 200, E: 260, At: 58, Text: "too late for a successor"},
	}
	want := 5 + n.speechDur(n.entries[0]) + 0.5 // the audio, and half a second of air
	i := n.addLineAfter(0)
	if i != 1 || math.Abs(n.entries[1].At-want) > 0.01 {
		t.Fatalf("the new line landed at row %d %+v, want at %.2f", i, n.entries, want)
	}
	// no room before the clip ends: refuse rather than clamp into the wall
	if i := n.addLineAfter(2); i != -1 { // row 2 is the late line after the insert above
		t.Fatalf("a line was added where the clip ends (row %d)", i)
	}
	if len(n.entries) != 3 {
		t.Fatalf("the refusal still changed the entries: %+v", n.entries)
	}
}

// The row's time field: session clock out, several spellings back in, and the
// list re-sorted by (clip, placement) once an edit lands.
func TestTheTimeFieldRoundTripsAndSorts(t *testing.T) {
	for _, c := range []struct {
		in   string
		want float64
		ok   bool
	}{
		{"06:42.5", 402.5, true}, {"06:42", 402, true}, {"402.5", 402.5, true},
		{"1:06:40", 4000, true}, {" 06:42 ", 402, true},
		{"", 0, false}, {"six", 0, false}, {"-5", 0, false}, {"1:2:3:4", 0, false},
	} {
		got, ok := parseClock(c.in)
		if ok != c.ok || (ok && got != c.want) {
			t.Errorf("parseClock(%q) = %g,%v want %g,%v", c.in, got, ok, c.want, c.ok)
		}
	}
	for _, tv := range []float64{0, 402.5, 402, 3599.9} {
		got, ok := parseClock(fmtClock(tv))
		if !ok || got != tv {
			t.Errorf("fmtClock(%g) = %q, parses back to %g,%v", tv, fmtClock(tv), got, ok)
		}
	}
	// the field corrects itself as it is typed: everything a clock cannot
	// contain is dropped, and what remains is judged by parseClock alone
	for _, c := range []struct{ in, want string }{
		{"06:4a2.5", "06:42.5"}, {"6m42s", "642"}, {"⏱ 07:00", "07:00"},
		{"06:42.5", "06:42.5"}, {"", ""},
	} {
		if got := cleanClock(c.in); got != c.want {
			t.Errorf("cleanClock(%q) = %q, want %q", c.in, got, c.want)
		}
	}
	n := &narrator{entries: []narrEntry{
		{S: 100, E: 130, At: 20, Text: "b"},
		{S: 200, E: 260, At: 1, Text: "c"},
		{S: 100, E: 130, At: 2, Text: "a"},
	}}
	n.sortEntries()
	if n.entries[0].Text != "a" || n.entries[1].Text != "b" || n.entries[2].Text != "c" {
		t.Fatalf("sorted order is %+v", n.entries)
	}
}

// The trash removes the row it is pointed at. All of it, including the last
// line on a clip -- that one used to turn into a blank "this clip plays its own
// audio" row, so the button cleared the words, left a text box's worth of page
// behind, and did nothing at all the second time it was pressed.
//
// What the blank row was protecting is staleFor, which reads coverage: a clip
// with no line looks exactly like a clip the cut moved under, and ▶ would
// rewrite the whole narration to fill it back in. The decision is still kept,
// as a line in the file rather than a row on the page.
func TestTheTrashRemovesTheRowAndRemembersTheClipIsQuiet(t *testing.T) {
	n := editNarr(t)
	n.entries = []narrEntry{
		{S: 100, E: 130, At: 5, Text: "one"},
		{S: 100, E: 130, At: 20, Text: "two"},
		{S: 200, E: 260, At: 10, Text: "three"},
	}

	// a clip with two lines just loses one, and is not marked quiet
	n.deleteLine(0)
	if len(n.entries) != 2 || n.entries[0].Text != "two" {
		t.Fatalf("after deleting one of two lines: %+v", n.entries)
	}
	if len(n.silent) != 0 {
		t.Errorf("a clip that still has a line was recorded as quiet: %+v", n.silent)
	}

	// the clip's last line goes too -- row and all
	n.deleteLine(1)
	if len(n.entries) != 1 || n.entries[0].S != 100 {
		t.Fatalf("deleting a clip's only line left something behind: %+v", n.entries)
	}
	if !silentFor(n.silent, cutSeg{S: 200, E: 260}) || len(n.silent) != 1 {
		t.Fatalf("the emptied clip was not recorded as quiet exactly once: %+v", n.silent)
	}
	// and saying it twice does not say it twice
	n.silent = markSilent(n.silent, cutSeg{S: 200, E: 260})
	if len(n.silent) != 1 {
		t.Errorf("the same clip was recorded quiet twice: %+v", n.silent)
	}
	// which is the whole point: ▶ must not decide the narration is stale and
	// rewrite the line you just deleted
	if why := n.staleFor(n.a.ed.segs); why != "" {
		t.Fatalf("hand-deleting a clip's last line made the narration stale: %s", why)
	}
	// and it survives the app being closed
	n.save()
	n.load()
	if !silentFor(n.silent, cutSeg{S: 200, E: 260}) {
		t.Errorf("the quiet clip was forgotten on reload: %+v", n.silent)
	}
	// putting a line back on it drops the note
	if i := n.addLineAt(210); i < 0 {
		t.Fatal("a line could not be added back to the emptied clip")
	}
	n.save()
	if silentFor(n.silent, cutSeg{S: 200, E: 260}) {
		t.Errorf("a clip narrated again is still carrying a note saying it is quiet: %+v", n.silent)
	}
}

// The ⚠ arithmetic: a line may speak until the same clip's next line, or to
// the clip's end plus the growth the render allows (maxExtend). Until the wav
// exists the length is estimated from the text, and the estimate has to be in
// the right neighbourhood or the ⚠ fires on lines that fit.
func TestALineIsWarnedWhenItsWordsOutrunItsSlot(t *testing.T) {
	root := t.TempDir()
	a := &App{root: root, outDir: root}
	long := "and this one runs much longer than the fourteen seconds this clip " +
		"has for it, because it keeps going and going and adds another clause " +
		"every time it should have stopped, well past anything the render could " +
		"stretch the picture to cover, and then a little further still"
	n := &narrator{a: a, entries: []narrEntry{
		{S: 100, E: 130, At: 5, Text: "these are exactly five words"},
		{S: 100, E: 130, At: 20, Text: long},
		{S: 200, E: 260, At: 55, Text: "short"},
	}}
	if got := n.lineWindow(0); got != 15 {
		t.Errorf("line 0 may speak for %g s, want 15 (until the next line)", got)
	}
	if got := n.lineWindow(1); got != 10+maxExtend {
		t.Errorf("line 1 may speak for %g s, want %g (to the clip's end plus its growth)", got, 10+maxExtend)
	}
	if got := n.lineWindow(2); got != 5+maxExtend {
		t.Errorf("line 2 may speak for %g s, want %g", got, 5+maxExtend)
	}
	// nothing spoken yet: 28 characters at the default rate
	if got := n.speechDur(n.entries[0]); math.Abs(got-28/speechRate) > 0.01 {
		t.Errorf("five words estimate as %g s, want %g", got, 28/speechRate)
	}
	if got := n.speechDur(narrEntry{Text: "   "}); got != 0 {
		t.Errorf("a silent line estimates as %g s", got)
	}
	// the pair the warning actually compares
	if n.speechDur(n.entries[1]) <= n.lineWindow(1) {
		t.Error("the deliberately overlong line does not read as overlong")
	}
	if n.speechDur(n.entries[0]) > n.lineWindow(0) {
		t.Error("a five-word line in a fifteen-second slot reads as overlong")
	}
}

// The estimate used to be a second a word, which read this line -- ten words
// the TTS says in about three seconds -- as ten seconds of speech and put a ⚠
// on a line with room to spare. Characters, not words: the emotion tag is not
// in the text, so it is not in the count either.
func TestAnUnspokenLineIsEstimatedAtSpeakingPace(t *testing.T) {
	root := t.TempDir()
	a := &App{root: root, outDir: root}
	n := &narrator{a: a, entries: []narrEntry{{S: 393, E: 403, At: 0,
		Emotion: "excited", Text: "Hey everyone, welcome to the big treasure hunt live event."}}}
	got := n.speechDur(n.entries[0])
	if got < 2.5 || got > 5 {
		t.Errorf("a ten-word line estimates as %.1f s, want somewhere near the "+
			"three or four seconds it takes to say", got)
	}
	if got > n.lineWindow(0) {
		t.Errorf("a ten-word line in a ten-second clip warns as overlong (%.1f s)", got)
	}
}

// And once takes exist the guess stops guessing: every voice and language
// speaks at its own pace, and the wavs already on disk measure it. The cache
// is keyed by wav path, so a prefilled entry stands in for a spoken take.
func TestTheEstimateLearnsFromTheTakesAlreadySpoken(t *testing.T) {
	root := t.TempDir()
	a := &App{root: root, outDir: root}
	spoken := []narrEntry{
		{S: 0, E: 30, Text: "one hundred characters of text spoken in exactly ten seconds flat, which is ten a second, no more"},
		{S: 30, E: 60, At: 0, Text: "another line of one hundred characters, also ten seconds long, to make the measurement worth using"},
	}
	n := &narrator{a: a, durCache: map[string]float64{}, entries: append(
		append([]narrEntry{}, spoken...),
		narrEntry{S: 60, E: 90, Text: "this one has not been spoken at all"})}
	if got := n.spokenRate(); got != speechRate {
		t.Errorf("with nothing spoken the rate is %g, want the default %g", got, speechRate)
	}
	for _, e := range spoken {
		n.durCache[a.ttsWav(e)] = float64(len(e.Text)) / 10
	}
	if got := n.spokenRate(); math.Abs(got-10) > 0.1 {
		t.Errorf("two takes at ten characters a second measure as %g", got)
	}
	unspoken := n.entries[2]
	want := float64(len(unspoken.Text)) / 10
	if got := n.speechDur(unspoken); math.Abs(got-want) > 0.05 {
		t.Errorf("the unspoken line estimates as %g s, want %g at the measured rate", got, want)
	}
	// a take that came out chopped cannot teach a rate nobody speaks at
	for _, e := range spoken {
		n.durCache[a.ttsWav(e)] = 2
	}
	if got := n.spokenRate(); got != rateMax {
		t.Errorf("a truncated wav moved the rate to %g, want it clamped to %g", got, rateMax)
	}
}

// A typed time is a placement, and the row has to end up agreeing with itself
// about it. Typing a time that belongs to another clip used to clamp the line
// into the last second of its own -- silently, while the box kept showing what
// was typed -- so a row could read "12:42 – 11:07.5", an end a minute and a half
// before its start, with nothing to say which number was the line's real place.
func TestATypedTimeMovesTheLineToTheClipItNames(t *testing.T) {
	n := editNarr(t) // clips 100–130 and 200–260
	n.entries = []narrEntry{
		{S: 100, E: 130, At: 5, Text: "first"},
		{S: 200, E: 260, At: 10, Text: "second"},
	}

	// inside its own clip: a nudge, and nothing moves house
	if moved := n.moveLine(0, 120); moved {
		t.Error("a nudge inside the clip reported a move between clips")
	}
	if e := n.entries[0]; e.S != 100 || e.E != 130 || math.Abs(e.At-20) > 0.01 {
		t.Errorf("nudged to 120 s, got %+v", e)
	}

	// a time in the OTHER clip re-homes the line, bounds and all: that is what
	// was asked for, and the spoken wav is keyed by words and delivery, so it
	// travels with the line instead of having to be spoken again
	if moved := n.moveLine(0, 230); !moved {
		t.Error("a time in the next clip did not report a move")
	}
	if e := n.entries[0]; e.S != 200 || e.E != 260 || math.Abs(e.At-30) > 0.01 {
		t.Errorf("moved to 230 s, got %+v -- want the clip 200–260 at +30", e)
	}

	// the last second of a clip is not a place to START speaking, wherever the
	// line came from: the whole page's arithmetic (lineWindow, the ⚠, the
	// render's spill room) is written against that rule
	n.moveLine(0, 259.8)
	if e := n.entries[0]; math.Abs(e.At-59) > 0.01 {
		t.Errorf("placed at the very end of the clip, got At %.2f, want the clip's last second (59)", e.At)
	}

	// a time in the gap between clips, or past the cut entirely, has no clip to
	// land in: the line stays where it is rather than moving somewhere it
	// cannot be, and the status line says so (the field is written back from
	// the entry, so the box stops disagreeing with the label)
	n.entries[0] = narrEntry{S: 100, E: 130, At: 5, Text: "first"}
	for _, tv := range []float64{150, 1000} {
		if moved := n.moveLine(0, tv); moved {
			t.Errorf("%.0f s is not in any clip, but the line reported a move", tv)
		}
		if e := n.entries[0]; e.S != 100 || e.E != 130 {
			t.Errorf("%.0f s is not in any clip, but the line left its own: %+v", tv, e)
		}
		if e := n.entries[0]; e.S+e.At > e.E {
			t.Errorf("%.0f s put the line past the end of its clip: %+v", tv, e)
		}
	}

	// and it is never out of range, whatever is typed
	for _, tv := range []float64{-100, 0, 129.99, 260, 1e6} {
		n.moveLine(0, tv)
		e := n.entries[0]
		if e.At < 0 || e.At > e.E-e.S-1+0.001 {
			t.Errorf("time %g left the line at %+v -- outside its own clip", tv, e)
		}
	}
}

// Retyping a line's time into another clip is a departure like deleting it: the
// clip it left is empty, and staleFor reads an empty clip as one the cut moved
// under. Either way the emptiness has to be recorded, or the next ▶ rewrites
// the whole narration to fill a hole you made on purpose.
func TestALineLeavingAClipLeavesItRecordedAsQuiet(t *testing.T) {
	n := editNarr(t) // clips 100–130 and 200–260
	n.entries = []narrEntry{
		{S: 100, E: 130, At: 5, Text: "the only line on the first clip"},
		{S: 200, E: 260, At: 10, Text: "second"},
	}

	if !n.moveLine(0, 230) {
		t.Fatal("a time in the next clip did not report a move")
	}
	// through the file, because that is where the page's memory lives
	n.save()
	n.load()
	if len(n.entries) != 2 {
		t.Fatalf("the move changed how many lines there are: %+v", n.entries)
	}
	if !silentFor(n.silent, cutSeg{S: 100, E: 130}) {
		t.Fatalf("the abandoned clip was not recorded as quiet: %+v", n.silent)
	}
	// ...and the page still agrees with the cut, which is what ▶ reads before
	// deciding to rewrite every line
	if why := n.staleFor(n.a.ed.segs); why != "" {
		t.Errorf("after the move the narration reads as stale: %s", why)
	}

	// a clip with another line left on it is not abandoned, so no note
	if !n.moveLine(1, 110) { // the later of the two lines on 200–260 goes back
		t.Fatal("the line did not report a move")
	}
	n.save()
	if silentFor(n.silent, cutSeg{S: 200, E: 260}) {
		t.Errorf("a clip that still has a line was recorded as quiet: %+v", n.silent)
	}
}

// The transport is wiring, so it is pinned at the source level like the other
// page wiring: the tick feeds the slider and the playhead, the slider seeks
// debounced, and the two editing verbs exist and reach their actions.
func TestTheTransportIsWired(t *testing.T) {
	b, err := os.ReadFile("narrate.go")
	if err != nil {
		t.Fatal(err)
	}
	src := string(b)
	for _, want := range []string{
		"n.setPlayhead(t)",                                    // the tick records where the preview is
		"n.slider.SetValue(cutPos(segs, t))",                  // ...and shows it, on the cut's clock
		"n.seekWant = cutAt(n.clips(), n.slider.Value())",     // ...which a drag reads back the other way
		"n.seekTo(n.seekWant)",                                // a drag seeks, debounced
		"a.addLineClicked()",                                  // ＋ adds a line
		"transport.Append(addBtn)",                            // ...from the transport row, not a row of its own
		"n.holdForSynth(",                                     // an edited line is re-spoken the first time it plays
		"n.deleteLine(i)",                                     // 🗑 per row
		"n.focusLine(n.addLineAfter(i))",                      // the row's ＋ adds below it
		"head.Append(add)",                                    // ...and is on the row, not merely built
		"i := n.addLineAt(n.pos)",                             // the button adds at the playhead
		"exists(a.ttsWav(e))",                                 // ▶'s speak pass skips what the cache already has
		"n.rows[i].text.GrabFocus()",                          // a new line is ready to type into
		"n.seekTo(n.leadIn(i))",                               // clicking a row cues its LINE, not its clip
		"snapToCut(n.clips(), n.pos, n.pos-3, cutEdge)",       // ⏪ and ⏩ flank the play button
		"snapToCut(n.clips(), n.pos, n.pos+3, cutEdge)",       // ...and land on the cut, not in a gap
		"n.slider.SetRange(0, math.Max(0.001, cutLen(segs)))", // the bar IS the cut: removed footage is not on it
		"if n.livePlayRow() != n.liveRow {",                   // the ⏸ is redrawn when it should move
		"tl.AddCSSClass(\"error\")",                           // the ⚠ is visible, not only textual
		"cleanClock(when.Text())",                             // the time field rejects letters as typed
		`when.AddCSSClass("error")`,                           // ...and shows a half-typed time as such
		"moved := n.moveLine(i, t)",                           // a finished edit is placed for real, other clips included
		"when.SetText(fmtClock(e.S + e.At))",                  // ...and written back, so the box cannot lie about where the line is
		`"– " + fmtClock(e.S+e.At+dur)`,                       // the dash-time is when the AUDIO ends
		"n.a.rerollEntry(i)",                                  // ↻ per row
		"n.frameStep(int(math.Round(dy)))",                    // the wheel over the slider steps frames
		"n.player.Playing() {",                                // ...and only while the picture is stopped
		"when.SetMaxWidthChars(8)",                            // a clock field asks for a clock's width, not the row's
		"n.selectRow(n.nearestEntry(t))",                      // the blue row rides the playhead
		"tScroll.SetMinContentHeight(oneLine)",
		"tScroll.SetMaxContentHeight(threeLines)",
		"tScroll.SetPropagateNaturalHeight(true)", // the three above only clamp; this one sizes
	} {
		if !strings.Contains(src, want) {
			t.Errorf("the transport wiring lost %s", want)
		}
	}
	// the box grows a line at a time, so its height has to be a whole number of
	// lines plus fixed chrome -- anything else and the third line is half cut off
	// exactly when the scrollbar appears
	for _, c := range []struct{ lineH, lines, want int }{
		{20, 1, 28}, {20, 2, 48}, {20, 3, 68}, {17, 3, 59},
	} {
		if got := textBoxHeight(c.lineH, c.lines); got != c.want {
			t.Errorf("%d lines at %d px each is %d px of box, want %d", c.lines, c.lineH, got, c.want)
		}
	}
	if textBoxHeight(20, 3)-textBoxHeight(20, 1) != 2*20 {
		t.Error("growing from one line to three does not add exactly two lines")
	}

	// there is deliberately NO speak-changed button: an edited line's cache key
	// changes, the tick finds no wav the next time playback reaches it, and
	// holdForSynth speaks it then -- the pin above is that path. What must
	// still exist is the run bar's ▶ speaking everything missing in one pass
	// (narrateRun), which is the batch version of the same rule.
	if strings.Contains(src, "speakMissing") {
		t.Error("a speak-changed button is back — first play is the re-speak")
	}

	// and deliberately NO second ＋ on the row. It existed to reach above a
	// clip's first line, which is a real place to need -- but the transport's ＋
	// already goes there, and better: it adds at the playhead, so the line is
	// placed against the picture it has to land on rather than against a row.
	// Two arrows on every row bought the same reach at the price of a sixth
	// button on each of them.
	if strings.Contains(src, "addLineBefore") {
		t.Error("the row grew its second ＋ back — the transport's ＋ is the way above a line")
	}
}

// The blue row answers "where am I" for every second of the preview, not only
// the seconds a line happens to be sounding in. entryAt is -1 through the
// lead-in before a line, through the tail after it, and through every clip the
// narration left alone -- which is most of the running time, and exactly when
// you are watching for something to fix.
func TestTheSelectedRowFollowsThePicture(t *testing.T) {
	n := &narrator{entries: []narrEntry{
		{S: 100, E: 130, At: 5, Text: "first"},   // speaks from 105
		{S: 100, E: 130, At: 20, Text: "second"}, // ...to 120, then this one to 130
		{S: 200, E: 260, Text: ""},               // a clip with no line is still a row
	}}
	for _, c := range []struct {
		t    float64
		want int
		why  string
	}{
		{100, 0, "the clip's head, before its first line arrives"},
		{107, 0, "inside the first line"},
		{119.9, 0, "the last moment before the second line"},
		{120, 1, "the second line starts"},
		{129, 1, "still the second line, its words long finished"},
		{160, 1, "the gap between clips: nearer the line just heard"},
		{199, 2, "...and nearer the next clip once past the middle"},
		{230, 2, "a silent clip is where you are, and it has a row"},
		{9999, 2, "past the end of the cut"},
	} {
		if got := n.nearestEntry(c.t); got != c.want {
			t.Errorf("at %gs the blue row is %d, want %d (%s)", c.t, got, c.want, c.why)
		}
	}
	// the narrower question -- whose wav should be sounding -- is unchanged, and
	// is still -1 for most of the same times
	if got := n.entryAt(100); got != -1 {
		t.Errorf("entryAt(100) = %d: a line is being voiced before it starts", got)
	}
	if got := n.entryAt(230); got != -1 {
		t.Errorf("entryAt(230) = %d: a clip with no line is being voiced", got)
	}
	if n.nearestEntry(0) != 0 {
		t.Error("with a playhead before everything the list points nowhere")
	}
	if (&narrator{}).nearestEntry(5) != -1 {
		t.Error("an empty narration selected a row")
	}
}

// The wheel over the slider steps frames. ⏪ and ⏩ jump three seconds to find a
// moment; this lands on it, so the arithmetic has to respect the two things
// that make it a frame step rather than a nudge: the source's own rate, and the
// cut -- a step must never come to rest in material the edit removed.
func TestTheWheelStepsWholeFrames(t *testing.T) {
	segs := []cutSeg{{S: 100, E: 130}, {S: 200, E: 260}}
	const fps = 30
	one := 1.0 / fps

	if got := frameTarget(segs, 110, fps, 1); math.Abs(got-(110+one)) > 1e-9 {
		t.Errorf("one frame forward from 110 landed at %g", got)
	}
	if got := frameTarget(segs, 110, fps, -1); math.Abs(got-(110-one)) > 1e-9 {
		t.Errorf("one frame back from 110 landed at %g", got)
	}
	// the rate is the source's: the same notch is a shorter step on faster video
	if a, b := frameTarget(segs, 110, 60, 1), frameTarget(segs, 110, 30, 1); a-110 >= b-110 {
		t.Errorf("a frame at 60 fps (%g) is not shorter than one at 30 (%g)", a-110, b-110)
	}
	// off the end of a clip: the gap is removed material, so the step lands on
	// the near edge of the neighbour rather than inside it
	if got := frameTarget(segs, 129.99, fps, 1); got != 200 {
		t.Errorf("stepping off the end of clip 1 landed at %g, want the head of clip 2", got)
	}
	if got := frameTarget(segs, 200, fps, -1); math.Abs(got-(130-one)) > 1e-9 {
		t.Errorf("stepping back off the head of clip 2 landed at %g, want the last frame of clip 1", got)
	}
	// ...and neither end of the cut can be stepped out of
	if got := frameTarget(segs, 100, fps, -1); got != 100 {
		t.Errorf("stepping back from the first frame left the cut at %g", got)
	}
	if got := frameTarget(segs, 259.99, fps, 1); math.Abs(got-(260-one)) > 1e-9 {
		t.Errorf("stepping past the last frame left the cut at %g", got)
	}
}

// The slider is the cut's own clock: its length is the kept seconds, so what
// the edit removed takes up no room on the bar and there is no way to drop the
// handle on footage the finished video will not contain. It used to span the
// session instead -- half an hour of source for five minutes of cut meant five
// sixths of the bar stood for material nobody will ever see, and every drag
// across it fought a snap pulling the handle back to a clip edge.
func TestTheSliderIsTheCutsOwnClock(t *testing.T) {
	segs := []cutSeg{{S: 100, E: 130}, {S: 200, E: 260}, {S: 1000, E: 1010}}
	const total = 30.0 + 60 + 10
	if got := cutLen(segs); math.Abs(got-total) > 1e-9 {
		t.Errorf("cutLen = %g, want %g -- the bar is as long as the video it previews", got, total)
	}
	if cutLen(nil) != 0 {
		t.Error("no cut, but the bar has a length")
	}

	// session time -> the cut's clock
	for _, c := range []struct {
		what string
		t    float64
		want float64
	}{
		{"the first frame of the cut", 100, 0},
		{"ten seconds into clip 1", 110, 10},
		{"the head of clip 2", 200, 30},
		{"deep in clip 2", 230, 60},
		{"the head of the last clip", 1000, 90},
		{"the end of the cut", 1010, 100},
		// a gap is not on the bar at all: the handle waits at the join the
		// picture is about to reach rather than running ahead of it
		{"in the gap between 1 and 2", 160, 30},
		{"in the long gap", 500, 90},
		{"before the cut starts", 0, 0},
		{"past the end of the cut", 5000, 100},
	} {
		if got := cutPos(segs, c.t); math.Abs(got-c.want) > 1e-9 {
			t.Errorf("%s: cutPos(%g) = %g, want %g", c.what, c.t, got, c.want)
		}
	}

	// ...and back, which is what a drag does. Every point on the bar is inside
	// a clip -- that is the whole property being bought here.
	for x := 0.0; x <= total; x += 0.5 {
		got := cutAt(segs, x)
		if cur, _ := gapAt(segs, got); cur < 0 {
			t.Fatalf("the handle at %g of %g reads as session time %g, which the cut removed", x, total, got)
		}
	}
	for _, c := range []struct{ x, want float64 }{
		{0, 100}, {10, 110}, {30, 200}, {60, 230}, {90, 1000},
		{-5, 100},             // below the bar: its first frame
		{100, 1010 - cutEdge}, // the far end is the last FRAME, not the gap after it
		{1e6, 1010 - cutEdge}, // and nothing past the bar exists
	} {
		if got := cutAt(segs, c.x); math.Abs(got-c.want) > 1e-9 {
			t.Errorf("cutAt(%g) = %g, want %g", c.x, got, c.want)
		}
	}

	// a round trip through both has to be the identity inside a clip, or the
	// handle creeps every time the tick writes it and a drag reads it back
	for _, tv := range []float64{100, 110, 129.99, 200, 259.5, 1000, 1009.9} {
		if got := cutAt(segs, cutPos(segs, tv)); math.Abs(got-tv) > 1e-9 {
			t.Errorf("%g went round the two clocks and came back as %g", tv, got)
		}
	}

	// with no cut there is nothing to map onto, and a slider that swallows
	// seeks would be worse than one that passes them through
	if got := cutAt(nil, 42); got != 42 {
		t.Errorf("with no cut, the handle at 42 reads as %g", got)
	}
}

// ⏪, ⏩ and the wheel still move in session time -- they are frame and second
// steps, not places on the bar -- so they keep snapping over what was removed.
func TestAScrubLandsOnTheCut(t *testing.T) {
	segs := []cutSeg{{S: 100, E: 130}, {S: 200, E: 260}}
	const e = cutEdge
	for _, c := range []struct {
		what     string
		from, to float64
		want     float64
	}{
		{"inside a clip, nothing to snap", 110, 120, 120},
		{"forward into the gap", 120, 160, 200},
		{"forward into the gap, from just off the end", 129, 130, 200},
		{"back into the gap", 210, 160, 130 - e},
		{"back to exactly a clip's end", 210, 130, 130 - e},
		{"forward past the whole cut", 250, 900, 260 - e},
		{"back before the whole cut", 110, 3, 100},
		{"forward from before the cut", 3, 40, 100},
		{"back from past the end", 900, 400, 260 - e},
	} {
		if got := snapToCut(segs, c.from, c.to, e); math.Abs(got-c.want) > 1e-9 {
			t.Errorf("%s: %g -> %g landed at %g, want %g", c.what, c.from, c.to, got, c.want)
		}
	}
	// a drag through a gap must not bounce: wherever the first event of it puts
	// the playhead, the next event of the SAME drag goes on the way it was going
	back := snapToCut(segs, 210, 165, e)
	if next := snapToCut(segs, 165, 160, e); next != back {
		t.Errorf("dragging on through the gap turned around: %g then %g", back, next)
	}
	if got := snapToCut(nil, 10, 40, e); got != 40 {
		t.Errorf("with no cut to snap to, the seek moved to %g", got)
	}
}

// The ⏸ belongs on the row being played, which is the blue one. It used to sit
// on whichever row's wav was last handed to the voice player and stay there:
// the line ends, the player reports stopped, and nothing redraws the icon until
// the next line starts -- so a row that finished talking still offered to pause
// itself while the picture was three lines further on.
func TestThePauseIconRidesThePlayhead(t *testing.T) {
	n := &narrator{liveRow: -1, speaking: -1, solo: -1, entries: []narrEntry{
		{S: 100, E: 130, At: 0, Text: "the first line"},
		{S: 200, E: 260, At: 5, Text: "the second line"},
		{S: 300, E: 330, At: 0}, // a clip the narration left alone
	}}

	// stopped: no row is playing anything
	if got := n.livePlayRow(); got != -1 {
		t.Errorf("with nothing playing, row %d shows the ⏸", got)
	}

	n.player = &Player{playing: true}
	n.pos = 105
	if got := n.livePlayRow(); got != 0 {
		t.Errorf("inside the first line, the ⏸ is on row %d", got)
	}
	// between the two lines -- the first has stopped talking and the second has
	// not started -- the ⏸ is where the picture is heading, like the selection
	n.pos = 180
	if got := n.livePlayRow(); got != 1 {
		t.Errorf("between the lines, the ⏸ is on row %d, want the row the playhead is nearest", got)
	}
	if got, sel := n.livePlayRow(), n.nearestEntry(n.pos); got != sel {
		t.Errorf("the ⏸ (row %d) and the blue selection (row %d) came apart", got, sel)
	}
	// a clip with no line is playing too -- its ▶ plays the clip on its own
	// audio, so the row it is on offers to pause it like any other
	n.pos = 310
	if got := n.livePlayRow(); got != 2 {
		t.Errorf("the picture is inside the silent clip and the ⏸ is on row %d, want row 2", got)
	}
	// spoken over a still frame: no picture to follow, so the voice decides
	n.player = &Player{}
	n.voice = &Player{playing: true}
	n.speaking = 1
	if got := n.livePlayRow(); got != 1 {
		t.Errorf("a line playing over a still frame put the ⏸ on row %d", got)
	}
	n.voice = &Player{}
	if got := n.livePlayRow(); got != -1 {
		t.Errorf("the voice stopped and row %d kept the ⏸", got)
	}
}

// The take is the thing you keep. Left alone the engine draws a fresh seed per
// request, so the same words came back as a different performance every time
// they were re-spoken -- after an unrelated edit, after a cleared cache -- and a
// delivery you liked could not be held on to. The seed comes off the same
// digest as the filename, so one wav means one performance, and the re-roll is
// the single deliberate way to move both.
func TestATakeIsStableUntilItIsRerolled(t *testing.T) {
	a := &App{root: t.TempDir(), outDir: t.TempDir()}
	e := narrEntry{S: 100, E: 130, At: 5, Text: "Open up, FBI!", Emotion: "angry=1"}

	if a.ttsKey(e) != a.ttsKey(e) || a.ttsWav(e) != a.ttsWav(e) {
		t.Fatal("the same line keys differently on two reads")
	}
	if ttsSeed(a.ttsKey(e)) != ttsSeed(a.ttsKey(e)) {
		t.Error("the same line draws a different seed each time it is spoken")
	}
	// a line nobody has re-rolled keeps the key it had before the field existed
	if strings.Contains(a.ttsKey(e), "#") {
		t.Errorf("an un-rolled line changed key (%s) — every cached take would re-speak", a.ttsKey(e))
	}
	// ...and a re-roll moves the wav AND the seed, or the button would hand back
	// the very take it was pressed to replace
	rolled := e
	rolled.Roll = 1
	if a.ttsWav(rolled) == a.ttsWav(e) {
		t.Error("a re-rolled line is served from the old take's file")
	}
	if ttsSeed(a.ttsKey(rolled)) == ttsSeed(a.ttsKey(e)) {
		t.Error("a re-rolled line is spoken with the old take's seed")
	}
	// two rolls are two takes, and rolling back returns the first one
	two := e
	two.Roll = 2
	if a.ttsWav(two) == a.ttsWav(rolled) {
		t.Error("the second re-roll repeats the first")
	}
	back := e
	back.Roll = 1
	if a.ttsWav(back) != a.ttsWav(rolled) {
		t.Error("the same roll of the same line is not the same take")
	}
	// the seed reaches the request as a plain integer option: the server parses
	// it as u32 and falls back to a random draw if it cannot
	b, err := os.ReadFile("narrate_tts.go")
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(b), `opts["seed"] = strconv.FormatUint(uint64(seed), 10)`) {
		t.Error("the request no longer carries a seed — every synthesis is a fresh draw again")
	}
}

// The Cut page is a page you come back to. A clip dragged wider there has to
// move the line's slot here -- the row's end time, the ⚠ that measures the
// words against it, and the preview -- without the words themselves being
// rewritten by a model.
func TestALineFollowsItsClipWhenTheCutMoves(t *testing.T) {
	n := editNarr(t)
	n.entries = []narrEntry{
		{S: 100, E: 130, At: 5, Text: "spoken at 105"},
		{S: 200, E: 260, At: 10, Text: "spoken at 210"},
	}
	// clip 1 grew at the back, clip 2 was trimmed at the front
	segs := []cutSeg{{S: 100, E: 150}, {S: 205, E: 260}}
	moved, orphan := refitEntries(segs, n.entries)
	if moved != 2 || orphan != 0 {
		t.Fatalf("refit moved %d line(s) with %d orphan(s), want 2 and 0", moved, orphan)
	}
	if got := n.entries[0]; got.S != 100 || got.E != 150 || got.At != 5 {
		t.Errorf("the grown clip's line is %+v, want 100-150 with the line still at 105", got)
	}
	// the front moved five seconds later, so the offset shrinks by five and the
	// line stays where it was placed against the video
	if got := n.entries[1]; got.S != 205 || got.E != 260 || got.S+got.At != 210 {
		t.Errorf("the trimmed clip's line is %+v, want 205-260 with the line still at 210", got)
	}
	// a cut that has not moved is not a change: nothing is rewritten, nothing
	// is re-saved, and the page is not rebuilt under the user
	if moved, _ := refitEntries(segs, n.entries); moved != 0 {
		t.Errorf("refitting the same cut twice moved %d line(s)", moved)
	}
	// the words are the expensive half and refit never touches them
	if n.entries[0].Text != "spoken at 105" || n.entries[1].Text != "spoken at 210" {
		t.Errorf("refit rewrote the lines: %+v", n.entries)
	}
}

// A clip the cut dropped altogether is not something a tab change may delete
// words over: the line stays, is counted, and staleFor says so -- which is ▶'s
// business, not refit's.
func TestALineWhoseClipIsGoneIsKept(t *testing.T) {
	n := editNarr(t)
	n.entries = []narrEntry{
		{S: 100, E: 130, At: 5, Text: "clip that survived"},
		{S: 200, E: 260, At: 10, Text: "clip that was cut"},
	}
	segs := []cutSeg{{S: 100, E: 130}}
	moved, orphan := refitEntries(segs, n.entries)
	if moved != 0 || orphan != 1 {
		t.Fatalf("refit moved %d line(s) with %d orphan(s), want 0 and 1", moved, orphan)
	}
	if got := n.entries[1]; got.S != 200 || got.Text != "clip that was cut" {
		t.Errorf("the orphaned line was changed: %+v", got)
	}
	if n.staleFor(segs) == "" {
		t.Error("a narration with a line for a clip the cut dropped reads as up to date")
	}
	// two clips merged into one: both lines land on it and both keep their
	// place against the video
	n.entries = []narrEntry{
		{S: 100, E: 130, At: 5, Text: "first"},
		{S: 200, E: 260, At: 10, Text: "second"},
	}
	if moved, orphan := refitEntries([]cutSeg{{S: 100, E: 260}}, n.entries); moved != 2 || orphan != 0 {
		t.Fatalf("a merge moved %d line(s) with %d orphan(s), want 2 and 0", moved, orphan)
	}
	if a, b := n.entries[0], n.entries[1]; a.S+a.At != 105 || b.S+b.At != 210 || a.E != 260 || b.E != 260 {
		t.Errorf("the merged clip's lines are %+v, want both on 100-260 at 105 and 210", n.entries)
	}
}

// ...and the page has to do it on arrival, which is the only moment the cut
// can have changed under it.
func TestNarrateRefitsOnArrival(t *testing.T) {
	src, err := os.ReadFile("main.go")
	if err != nil {
		t.Fatal(err)
	}
	show := regexp.MustCompile(`(?s)func \(a \*App\) showStep\(name string\) \{.*?\n}\n`).Find(src)
	if show == nil {
		t.Fatal("showStep is gone")
	}
	if !strings.Contains(string(show), "a.narr.refit()") {
		t.Error("opening Narrate does not refit the lines onto the cut")
	}
	b, err := os.ReadFile("narrate.go")
	if err != nil {
		t.Fatal(err)
	}
	in := regexp.MustCompile(`(?s)func \(n \*narrator\) updateInputs\(\) \{.*?\n}\n`).Find(b)
	if in == nil {
		t.Fatal("updateInputs is gone")
	}
	if !strings.Contains(string(in), "n.staleFor(segs)") {
		t.Error("the Inputs row does not say when the narration no longer answers the cut")
	}
}

// TestASilencedLaneStaysSilentPastTheClipsEnd: the preview does not stop dead
// at a clip's last frame. It holds past it while a line finishes speaking, and
// it takes a seek and a preroll to skip the removed stretch after it. Those
// seconds belonged to no scene, so the question "which lanes does this scene
// hear" had no answer and every lane came back -- the recording's own voice,
// silenced in the cut and replaced by the narration, spoke again underneath it.
func TestASilencedLaneStaysSilentPastTheClipsEnd(t *testing.T) {
	ed := &cutEditor{segs: []cutSeg{
		{S: 10, E: 20, Quiet: []string{"mic"}},
		{S: 30, E: 40},
	}}
	n := &narrator{a: &App{ed: ed}}
	for _, c := range []struct {
		what string
		at   float64
		want float64 // the S of the scene whose sound answer should be used
	}{
		{"inside the first clip", 15, 10},
		{"just past its end, where a line is still talking", 20.4, 10},
		{"in the removed stretch a seek is skipping", 25, 10},
		{"inside the second clip", 35, 30},
		{"past the last clip", 60, 30},
		{"before the cut starts", 2, 10},
	} {
		s := n.heardScene(c.at)
		if s == nil {
			t.Errorf("%s: no scene, so every lane is heard", c.what)
			continue
		}
		if s.S != c.want {
			t.Errorf("%s: took the scene at %.0f, wanted the one at %.0f", c.what, s.S, c.want)
		}
	}
	// and that answer is the one the preview's sound is set from
	if own, quiet := hushOf(n.heardScene(20.4), "cam"); own || !laneQuiet(quiet, "mic") {
		t.Errorf("the tail past a clip does not silence its lanes: own=%v quiet=%v", own, quiet)
	}
	body := funcBody(t, "narrate.go", `func \(n \*narrator\) syncFxSound\(\)`)
	if !strings.Contains(body, "n.heardScene(n.pos)") || !strings.Contains(body, "hushOf(s, base)") {
		t.Error("the preview's sound is back on the playhead's own scene, so a gap hears everything")
	}
}

// The narration column wraps when the window narrows; it does not scroll
// sideways. Left on GtkScrolledWindow's default the rows kept the width their
// longest line wanted and grew a horizontal scrollbar under them, so making
// the window smaller hid the words instead of re-wrapping them -- and the
// text boxes, which are the thing being read, never changed width at all.
//
// The two halves have to hold together: the policy is what hands the rows a
// narrower width, and the ellipsize is what lets them take it. A label with
// neither wrap nor ellipsize has a minimum width of its whole text, and this
// page's stamp line holds whole sentences -- so with the policy set and the
// label unbounded, the column would simply refuse to shrink instead.
func TestTheNarrationColumnWrapsRatherThanScrollingSideways(t *testing.T) {
	src := readSrc(t, "narrate.go")
	for _, want := range []string{
		"left.SetPolicy(gtk.PolicyNever, gtk.PolicyAutomatic)",    // the column
		"tScroll.SetPolicy(gtk.PolicyNever, gtk.PolicyAutomatic)", // and each box in it
		"tl.SetEllipsize(pango.EllipsizeEnd)",                     // the stamp line is not a floor
		"n.inputs = inputsLabel()",                                // the Inputs line ellipsizes too
		"text.SetWrapMode(gtk.WrapWord)",                          // and the words themselves wrap
	} {
		if !strings.Contains(src, want) {
			t.Errorf("narrate.go no longer contains %q -- the column stops shrinking with the window", want)
		}
	}
	// the floor is the size request and nothing else: a column that cannot go
	// under 360 px is a decision, one that cannot go under its longest
	// sentence is an accident
	if !strings.Contains(src, "left.SetSizeRequest(360, -1)") {
		t.Error("the narration column lost its deliberate minimum width")
	}
	// every other scrolling column in the app already says it, which is what
	// made this one the exception rather than the rule
	for _, f := range []string{"cut_form.go", "publish.go"} {
		if !strings.Contains(readSrc(t, f), "SetPolicy(gtk.PolicyNever, gtk.PolicyAutomatic)") {
			t.Errorf("%s no longer wraps its column, so narrate.go is not following a rule any more", f)
		}
	}
}

// A rebuild keeps the reader's place. Every row is thrown away and built
// again -- a time edit commits, a re-roll changes an end time, a take lands --
// and a fresh GtkListBox is scrolled to the top, so editing line twelve put
// line one on screen and line twelve below the fold. That is what "it jumps
// back to the start" was, and it happened on the ordinary path: type a line,
// leave the box.
//
// The offset is put back and not the focus, because a rebuild is usually
// raised BY the focus leaving (queueRebuild) and taking it back would pull it
// off whatever it was moving to. The one caller that does want the focus asks
// for it by name afterwards.
func TestARebuildKeepsTheReadersPlace(t *testing.T) {
	src := readSrc(t, "narrate.go")
	for _, want := range []string{
		"at, sel := n.listOffset(), -1", // taken before the rows go
		"n.restoreOffset(at, sel)",      // and put back after they are built
		"n.listScroll = left",           // the scroller the offset is read from
		"adj.Upper()-adj.PageSize()",    // clamped to what the new list can show
	} {
		if !strings.Contains(src, want) {
			t.Errorf("narrate.go no longer contains %q -- the list goes back to the top on every rebuild", want)
		}
	}
	// on the idle: the rows have just been appended and have no height yet, so
	// the adjustment has no range and setting the value now clamps it to nought
	body := funcBody(t, "narrate.go", `func \(n \*narrator\) restoreOffset\(`)
	if !strings.Contains(body, "glib.IdleAdd(func() {") {
		t.Errorf("restoreOffset sets the offset before the rows have a height:\n%s", body)
	}
	// the selection goes back through selectRow, which holds n.building across
	// it -- picking a row seeks the preview, and putting back a row that was
	// already picked is not a hand asking for anything
	if !strings.Contains(body, "n.selectRow(sel)") {
		t.Error("restoreOffset re-selects the row some other way than selectRow")
	}
	if !strings.Contains(funcBody(t, "narrate.go", `func \(n \*narrator\) selectRow\(`), "n.building = true") {
		t.Error("selectRow no longer guards the seek, so restoring a selection would jump the preview")
	}
	// and rebuildRows still takes the offset before it empties the list, or it
	// would read nought off a list it had already torn down
	rb := funcBody(t, "narrate.go", `func \(n \*narrator\) rebuildRows\(`)
	i, j := strings.Index(rb, "n.listOffset()"), strings.Index(rb, "n.list.Remove(row)")
	if i < 0 || j < 0 || i > j {
		t.Errorf("rebuildRows reads the offset after emptying the list (offset %d, remove %d)", i, j)
	}
	// headless, where there is no scroller at all: no panic, no work
	n := &narrator{}
	if got := n.listOffset(); got != 0 {
		t.Errorf("a page with no scroller has offset %g", got)
	}
	n.restoreOffset(120, 3) // must not reach for a nil scroller
}
