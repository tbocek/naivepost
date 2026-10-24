package main

// The narrate page's logic that survives without a display: where preview
// playback jumps, and where the narration is read from.
//
// Preview playback follows the CUT, not the source file -- the picture has to
// skip what the edit removed. The GTK and GStreamer halves need a display, but
// the arithmetic that decides where to jump does not, and that is where an
// off-by-one costs the user a wrong preview.

import (
	"math"
	"os"
	"path/filepath"
	"regexp"
	"strings"
	"testing"
)

// realCut is the shape a session actually has: long holes between clips, the
// first clip starting minutes in, fractional bounds.
var realCut = []cutSeg{
	{S: 392.85, E: 419}, {S: 464, E: 491}, {S: 543, E: 575},
	{S: 639, E: 670}, {S: 2080, E: 2114.36},
}

func TestGapAt(t *testing.T) {
	for _, tc := range []struct {
		name      string
		t         float64
		cur, next int
	}{
		{"before the cut starts", 0, -1, 0},
		{"first frame of a clip", 392.85, 0, -1},
		{"inside a clip", 400, 0, -1},
		// the end is exclusive: 419 is the first instant the edit removed, so
		// it must read as a gap or the jump would fire a tick late
		{"the instant a clip ends", 419, -1, 1},
		{"deep in a gap", 450, -1, 1},
		{"the long gap before the last clip", 1000, -1, 4},
		{"inside the last clip", 2100, 4, -1},
		{"past the whole cut", 2114.36, -1, -1},
	} {
		t.Run(tc.name, func(t *testing.T) {
			cur, next := gapAt(realCut, tc.t)
			if cur != tc.cur || next != tc.next {
				t.Errorf("gapAt(%.2f) = (%d, %d), want (%d, %d)",
					tc.t, cur, next, tc.cur, tc.next)
			}
		})
	}

	// no cut at all must not read as "past the end", which would pause the
	// preview the moment it started
	if cur, next := gapAt(nil, 100); cur != -1 || next != -1 {
		t.Errorf("gapAt(nil) = (%d, %d), want (-1, -1)", cur, next)
	}
}

// TestClipsPrefersTheCut pins where the preview gets its clip list. Narration is
// written one entry per clip, but it is written LATER: following the entries
// alone means a project that has been cut but not yet narrated previews the
// uncut source, which is exactly not what Narrate asked for.
func TestClipsPrefersTheCut(t *testing.T) {
	n := &narrator{a: &App{}, entries: []narrEntry{{S: 10, E: 20}}}

	if got := n.clips(); len(got) != 1 || got[0].S != 10 {
		t.Fatalf("with no editor, clips() = %v, want the narration entries", got)
	}

	n.a.ed = &cutEditor{segs: realCut}
	got := n.clips()
	if len(got) != len(realCut) || got[0].S != realCut[0].S {
		t.Fatalf("with an editor, clips() = %v, want the cut", got)
	}

	// an emptied cut is not a cut: fall back rather than preview nothing
	n.a.ed = &cutEditor{}
	if got := n.clips(); len(got) != 1 || got[0].E != 20 {
		t.Fatalf("with an empty editor, clips() = %v, want the narration entries", got)
	}
}

// TestEntryAtIsByTime guards the voice lookup. Clip index and entry index drift
// apart the moment someone edits the cut after narrating; speaking by index
// would then play the wrong line over the wrong picture, confidently.
func TestEntryAtIsByTime(t *testing.T) {
	n := &narrator{entries: []narrEntry{
		{S: 392.85, E: 419, Text: "one"},
		{S: 2080, E: 2114.36, Text: "two"},
	}}
	for _, tc := range []struct {
		t    float64
		want int
	}{{392.85, 0}, {400, 0}, {419, -1}, {1000, -1}, {2100, 1}, {3000, -1}} {
		if got := n.entryAt(tc.t); got != tc.want {
			t.Errorf("entryAt(%.2f) = %d, want %d", tc.t, got, tc.want)
		}
	}
}

// TestNarrationFollowsOutDir is the regression behind "my narration is gone
// after a restart". It was always written; it was READ once, while building the
// page at startup, when outDir was still the root -- so a project whose output
// folder is out/<name> loaded an empty list and looked like it had never saved.
// Anything that moves outDir has to re-read it.
func TestNarrationFollowsOutDir(t *testing.T) {
	root := t.TempDir()
	out := filepath.Join(root, "out", "test")

	// a project narrated with its output folder set
	saver := &narrator{a: &App{root: root, outDir: out}}
	saver.entries = []narrEntry{{S: 392.85, E: 419, Text: "one", Emotion: "calm"}}
	saver.save()

	// the restart: the page is built before the project moves outDir
	n := &narrator{a: &App{root: root, outDir: root}}
	n.entries = []narrEntry{{Text: "stale"}} // load must clear, not merge
	n.load()
	if len(n.entries) != 0 {
		t.Fatalf("loaded %d entries from the root, want none there", len(n.entries))
	}

	// ...and then the project points it at the real output folder
	n.a.outDir = out
	n.load()
	if len(n.entries) != 1 || n.entries[0].Text != "one" || n.entries[0].S != 392.85 {
		t.Fatalf("after outDir moved, entries = %+v, want the saved narration", n.entries)
	}
}

// TestALinePlayedAloneOutlivesTheTick guards the rule that made a line's own ▶
// speak one word and stop. The preview and the per-line ▶ share one audio
// player, and the 100 ms tick's job is to hush that player whenever the picture
// is not running -- which is exactly the state a line auditioned over a still
// frame plays in. n.solo is what tells the two apart, and nothing at run time
// notices when a rewrite of this branch drops it.
func TestALinePlayedAloneOutlivesTheTick(t *testing.T) {
	src, err := os.ReadFile("narrate.go")
	if err != nil {
		t.Fatal(err)
	}
	hush := regexp.MustCompile(`(?s)if n\.player == nil \|\| !n\.player\.playing \{.*?\n\t\}`).Find(src)
	if hush == nil {
		t.Fatal("followPlayback's picture-stopped branch is gone")
	}
	if !strings.Contains(string(hush), "n.solo") {
		t.Error("the tick pauses the voice without asking who owns it — a line played " +
			"from its own ▶ will be cut off within 100 ms")
	}
	// and the other half: starting the preview has to take that player back,
	// or the line and the picture speak over each other
	tog := regexp.MustCompile(`(?s)func \(n \*narrator\) toggle\(\) \{.*?\n}\n`).Find(src)
	if !strings.Contains(string(tog), "claimVoice()") {
		t.Error("the preview starts without claiming the shared audio player")
	}
	claim := regexp.MustCompile(`(?s)func \(n \*narrator\) claimVoice\(\) \{.*?\n}\n`).Find(src)
	if claim == nil || !strings.Contains(string(claim), "n.solo = -1") {
		t.Error("claimVoice does not hand the players back to the preview")
	}
}

// TestAnEditedLineResumesCleanly is the fix for "a syllable of the old voice,
// then the video plays with no voice". Editing a line changes its cache key;
// the tick holds the picture and synthesizes -- but the voice player still
// held the OLD wav (paused, or ended, where Toggle replays it), and the resume
// continued from wherever the picture froze, starting the NEW wav at a stale
// mid-line offset: past its end if the new line is shorter, which is instant
// EOS and silence. So the hold must drop the stale wav and forget the voiced
// line, and the resume must go back to the line's start.
func TestAnEditedLineResumesCleanly(t *testing.T) {
	src, err := os.ReadFile("narrate.go")
	if err != nil {
		t.Fatal(err)
	}
	hold := regexp.MustCompile(`(?s)func \(n \*narrator\) holdForSynth\(i int\) \{.*?\n}\n`).Find(src)
	if hold == nil {
		t.Fatal("holdForSynth is gone")
	}
	for _, want := range []string{"n.voice.Stop()", "n.playSeg = -1"} {
		if !strings.Contains(string(hold), want) {
			t.Errorf("the synthesis hold no longer says %s — a stale wav survives it", want)
		}
	}
	if strings.Contains(string(hold), "n.voice.Pause()") {
		t.Error("the hold merely pauses the stale wav, which a later resume can replay")
	}
	wait := regexp.MustCompile(`(?s)func \(n \*narrator\) synthWait\(.*?\n}\n`).Find(src)
	if wait == nil {
		t.Fatal("synthWait is gone")
	}
	if !strings.Contains(string(wait), "n.cue(math.Max(e.S, e.S+e.At), true)") {
		t.Error("the resume after synthesis no longer returns to the line's start — " +
			"a mid-line hold starts the new wav at a stale offset")
	}
	// and the transport's resume drops whatever wav is cued: the tick reloads
	// the right one at the right offset, edited or not
	tog := regexp.MustCompile(`(?s)func \(n \*narrator\) toggle\(\) \{.*?\n}\n`).Find(src)
	if tog == nil || !strings.Contains(string(tog), "n.voice.Stop()") {
		t.Error("the transport resume leaves a stale wav loaded in the voice player")
	}
}

// TestARowsPlayNeverStartsInsideTheLineAbove is "I press ▶ on line 5 and it
// plays line 4".
//
// The run-in is a few seconds, and a clip carries as many lines as the writer
// put on it -- three of them on one clip is ordinary. A few seconds ahead of the
// second line on a clip is the FIRST one, still mid-sentence: entryAt gives
// those seconds to it, because it owns the clip until the next line starts, so
// the tick found it under the playhead and resumed its wav from wherever the
// seek had landed inside it. The button said line 5 and the page spoke line 4.
//
// The run-in is kept where the seconds belong to nobody, which is most rows.
func TestARowsPlayNeverStartsInsideTheLineAbove(t *testing.T) {
	n := &narrator{a: &App{}, entries: []narrEntry{
		{S: 100, E: 160, At: 4, Text: "first line on this clip"},
		{S: 100, E: 160, At: 20, Text: "second line, same clip"},
		{S: 200, E: 260, At: 30, Text: "a line on the next clip"},
		{S: 300, E: 360, At: 1, Text: "and one written near a clip's head"},
	}}
	for _, c := range []struct {
		i    int
		want float64
	}{
		{0, 101}, // three seconds ahead of a line at +4: seconds that are nobody's
		{1, 120}, // ...but ahead of the second line is the first one: start on the line
		{2, 227}, // its own clip, its own run-in
		{3, 300}, // clamped to the clip's head: there is no finished video before it
	} {
		if got := n.leadIn(c.i); math.Abs(got-c.want) > 1e-9 {
			t.Errorf("▶ on line %d starts the preview at %.2f, want %.2f", c.i+1, got, c.want)
		}
	}
	// which is the whole point of it: whoever the tick finds under the playhead
	// when the picture starts rolling is the row whose button was pressed, or
	// nobody at all -- never another row
	for i := range n.entries {
		if j := n.entryAt(n.leadIn(i)); j >= 0 && j != i {
			t.Errorf("▶ on line %d starts the preview inside line %d", i+1, j+1)
		}
	}

	// ...and every ▶ on the page goes through it. Both of speakEntry's playing
	// paths -- the spoken line and the caption, which has no voice to wake but
	// the same wrong row to land on -- had the run-in written out in place, so
	// either could keep the arithmetic while the other used the rule. Counted
	// rather than merely found, because "contains" cannot tell two call sites
	// apart and one of them regressing is exactly the bug this is about.
	tts, err := os.ReadFile("narrate_tts.go")
	if err != nil {
		t.Fatal(err)
	}
	if got := strings.Count(string(tts), "n.cue(n.leadIn(i), true)"); got != 2 {
		t.Errorf("%d of speakEntry's 2 play paths cue through leadIn", got)
	}
	if strings.Contains(string(tts), "e.S+e.At-") {
		t.Error("a ▶ works the run-in out for itself again — leadIn is the one place that decides")
	}
}

// TestALineAuditionRollsThePicture: a line's ▶ plays the clip it was written
// for, not the words alone over a frozen frame -- most of what there is to
// judge about a narration line is whether it lands on what is on screen. Source
// level, because the wiring is a GStreamer pipeline and a 100 ms tick: what a
// test can hold is that the button still reaches the picture, and that once the
// line has been spoken the transport goes back to being the cut's.
func TestALineAuditionRollsThePicture(t *testing.T) {
	src, err := os.ReadFile("narrate_tts.go")
	if err != nil {
		t.Fatal(err)
	}
	speak := regexp.MustCompile(`(?s)func \(a \*App\) speakEntry\(i int\) \{.*?\n}\n`).Find(src)
	if speak == nil {
		t.Fatal("speakEntry is gone")
	}
	// the cue lands a moment ahead of the line's placement, not on the head of
	// the clip -- a line an "at" puts a minute in would otherwise audition as
	// a minute of silence. And the ▶ clears the line's failure mark: the tick
	// runs a refused line mute so one bad request cannot stall every pass,
	// which without a retry path reads as "the TTS stopped working".
	for _, want := range []string{"n.cue(n.leadIn(i), true)", "n.solo, n.soloPic = i, true",
		"delete(n.synthFail, a.ttsWav(e))"} {
		if !strings.Contains(string(speak), want) {
			t.Errorf("a line's ▶ no longer rolls the picture with the voice (missing %s)", want)
		}
	}
	// the fallback is not a nicety: a clip whose recording is not on this
	// machine still has to be auditionable, and that is the only case left where
	// the line is spoken over whatever the frame happens to be showing
	if !strings.Contains(string(speak), "a.speakAlone(i, e)") {
		t.Error("a clip no recording covers can no longer be auditioned at all")
	}
	page, err := os.ReadFile("narrate.go") // the tick stayed with the page
	if err != nil {
		t.Fatal(err)
	}
	follow := regexp.MustCompile(`(?s)func \(n \*narrator\) followPlayback\(\) bool \{.*?\n}\n`).Find(page)
	if follow == nil {
		t.Fatal("followPlayback is gone")
	}
	// The cut is the master. The audition is a seek into the preview and lasts
	// exactly as long as its line: when the line has been spoken the tick drops
	// solo and the preview goes on playing the cut in order.
	//
	// It used to own the transport instead -- at the end of the auditioned CLIP
	// it hopped to the clip of the next line, or stopped dead if there was none.
	// A line placed near the end of its clip therefore threw the picture
	// somewhere else a second after it finished speaking, so the thing on screen
	// was never the cut the page exists to preview.
	for _, want := range []string{
		"t >= e.S+e.At+n.speechDur(e)",  // the audition ends with its line...
		"n.solo, n.soloPic = -1, false", // ...and hands the transport back
		"n.selectRow(ei)",               // the blue row still rides the tick
	} {
		if !strings.Contains(string(follow), want) {
			t.Errorf("the audition no longer hands the transport back to the cut (missing %s)", want)
		}
	}
	for _, gone := range []string{"nextSpoken", "n.entries[n.solo].E"} {
		if strings.Contains(string(follow), gone) {
			t.Errorf("the tick still steers by the audition's clip (%s) -- the cut is the master", gone)
		}
	}
	// and the row that is sounding has to draw the ⏸ for it wherever the sound
	// came from, or the button the user just pressed goes on showing ▶
	if !strings.Contains(string(follow), "n.speaking = ei") {
		t.Error("the tick speaks a line without telling its row, so the row keeps showing ▶")
	}
}

// A clip the narration left alone is still part of the cut, and its ▶ is the
// only way to watch it from the list. It used to answer with a status line and
// nothing else -- "clip 3 has no line" -- so the one part of the video the page
// would not play was the part you were deciding whether to write a line for.
func TestAClipWithNoLinePlaysFromItsRow(t *testing.T) {
	src, err := os.ReadFile("narrate_tts.go")
	if err != nil {
		t.Fatal(err)
	}
	speak := regexp.MustCompile(`(?s)func \(a \*App\) speakEntry\(i int\) \{.*?\n}\n`).Find(src)
	if speak == nil {
		t.Fatal("speakEntry is gone")
	}
	empty := regexp.MustCompile(`(?s)if strings\.TrimSpace\(e\.Text\) == "" \{.*?\n\t\}`).Find(speak)
	if empty == nil {
		t.Fatal("speakEntry no longer has a branch for a clip with no line")
	}
	for _, want := range []string{
		"n.cue(e.S, true)", // the clip rolls, from its own start
		"n.selectRow(i)",   // the blue row follows the press
		"n.claimVoice()",   // whatever was sounding gives the players up
	} {
		if !strings.Contains(string(empty), want) {
			t.Errorf("a wordless clip's ▶ does not play it (missing %s)", want)
		}
	}
	// ...and pressing it again pauses, like the ⏸ it draws. That is answered
	// above the branch now, for every kind of row at once (pausePress), which
	// is why it is asked of the whole function and not of this branch
	if !strings.Contains(string(speak), "n.pausePress(i)") {
		t.Error("a wordless clip that is playing no longer pauses on the press that shows ⏸")
	}
	// what it must NOT do is speak: an empty line costs a call and comes back
	// as silence
	for _, gone := range []string{"a.synthesize(", "a.speakAlone(", "n.holdForSynth("} {
		if strings.Contains(string(empty), gone) {
			t.Errorf("a clip with no line is sent to the TTS anyway (%s)", gone)
		}
	}
	// and the ⏸ has to be allowed to land on that row, or the button offers to
	// play something that is already playing
	live := regexp.MustCompile(`(?s)func \(n \*narrator\) livePlayRow\(\) int \{.*?\n}\n`).Find(src)
	if live == nil {
		t.Fatal("livePlayRow is gone")
	}
	if strings.Contains(string(live), `strings.TrimSpace(n.entries[i].Text) == ""`) {
		t.Error("livePlayRow still refuses a wordless row the ⏸, which its ▶ now needs")
	}
}

// The crash: editing a row's time and committing it took the whole app down
// with a SIGSEGV under GTK's text-iterator code. rebuildRows destroys every
// widget in the list, and both commit paths ran it from INSIDE one of those
// widgets' own handlers -- Enter on the entry, and the focus leaving it. GTK is
// still walking that widget when the handler returns.
func TestARowRebuildWaitsForTheEventToFinish(t *testing.T) {
	src, err := os.ReadFile("narrate.go")
	if err != nil {
		t.Fatal(err)
	}
	q := regexp.MustCompile(`(?s)func \(n \*narrator\) queueRebuild\(\) \{.*?\n}\n`).Find(src)
	if q == nil {
		t.Fatal("queueRebuild is gone")
	}
	for _, want := range []string{"glib.IdleAdd(", "n.rebuildRows()", "n.rebuildQ"} {
		if !strings.Contains(string(q), want) {
			t.Errorf("queueRebuild no longer defers the rebuild (missing %s)", want)
		}
	}
	// the two paths that a typed time commits through, and the shape of the
	// bug: a rebuild reached directly from either of them
	rows := regexp.MustCompile(`(?s)func \(n \*narrator\) rebuildRows\(\) \{.*?\n}\n`).Find(src)
	if rows == nil {
		t.Fatal("rebuildRows is gone")
	}
	for _, handler := range []string{
		`when.ConnectActivate(func() {`,
		`wf.ConnectLeave(func() {`,
	} {
		i := strings.Index(string(rows), handler)
		if i < 0 {
			t.Errorf("the time field lost its %s handler", handler)
			continue
		}
		body := string(rows)[i : i+400]
		if !strings.Contains(body, "n.queueRebuild()") {
			t.Errorf("%s does not queue its rebuild", handler)
		}
		if strings.Contains(body, "\n\t\t\t\tn.rebuildRows()") {
			t.Errorf("%s still tears the list down inside its own event", handler)
		}
	}
}

// TestARescanReadsTheNarrationBack: delete narrate/ and the page went on showing
// the narration it had in memory, with an Outputs line counting files that were
// no longer there. ⟳ is the button whose whole job is "the folder is the
// answer" -- and this was the one step it skipped.
func TestARescanReadsTheNarrationBack(t *testing.T) {
	root := t.TempDir()
	a := &App{root: root, outDir: root}
	n := &narrator{a: a, entries: []narrEntry{{S: 0, E: 3, Text: "one"}}}
	n.save()

	n.load()
	if len(n.entries) != 1 {
		t.Fatalf("after saving and re-reading, entries = %+v", n.entries)
	}
	if err := os.RemoveAll(a.narrateDir()); err != nil {
		t.Fatal(err)
	}
	n.load()
	if len(n.entries) != 0 {
		t.Errorf("narrate/ is gone and the page still holds %d line(s)", len(n.entries))
	}
	if got := summarizeOutputs(a.narrateDir()); got != "nothing yet" {
		t.Errorf("a deleted narrate/ still summarizes as %q", got)
	}

	// the wiring that gets that re-read to happen at all
	src, err := os.ReadFile("main.go")
	if err != nil {
		t.Fatal(err)
	}
	rescan := regexp.MustCompile(`(?s)func \(a \*App\) rescanAll\(\) \{.*?\n}\n`).Find(src)
	if rescan == nil || !strings.Contains(string(rescan), "a.updateNarrateInfo()") {
		t.Error("a rescan skips the narrate step, so its rows and its Outputs line go stale")
	}
	info := regexp.MustCompile(`(?s)func \(a \*App\) updateNarrateInfo\(\) \{.*?\n}\n`).Find(src)
	if info == nil {
		t.Fatal("updateNarrateInfo is gone")
	}
	for _, want := range []string{"n.load()", "n.rebuildRows()", "n.updateOut()"} {
		if !strings.Contains(string(info), want) {
			t.Errorf("the narrate rescan does not %s", want)
		}
	}
}
