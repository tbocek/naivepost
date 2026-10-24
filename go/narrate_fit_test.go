package main

import (
	"math"
	"strings"
	"testing"
)

// The render used to report this after the encode, seven clips at a time:
// "clip 3: the narration does not fit where it was placed — moved 1.3 s
// earlier". The page where the words are written said nothing, because its own
// ⚠ asks whether ONE line has room before the next one arrives -- and an overrun
// is cumulative. A first line three seconds long pushes every line under it
// three seconds later, each with all the room in the world of its own, and it
// is the last one that falls off the end of the clip.
func TestTheNarratePageSaysWhatTheRenderWouldHaveToMove(t *testing.T) {
	// one clip, 20 s, three lines placed 0/6/12 s in, each 5 s of speech.
	// Packed: 0.3, 5.6, 10.9 → ends 15.9, +0.2 tail = 16.1. Fits.
	n := &narrator{durCache: map[string]float64{}, entries: []narrEntry{
		{S: 100, E: 120, At: 0, Text: "one"},
		{S: 100, E: 120, At: 6, Text: "two"},
		{S: 100, E: 120, At: 12, Text: "three"},
	}}
	n.a = &App{root: t.TempDir(), outDir: t.TempDir()}
	fake := map[int]float64{0: 5, 1: 5, 2: 5}
	for i, d := range fake {
		n.durCache[n.a.ttsWav(n.entries[i])] = d
	}
	if !n.clipTop(0) || n.clipTop(1) || n.clipTop(2) {
		t.Fatal("the three rows are one clip and only the first is its top")
	}
	// ...and the row after them, on a different stretch, starts a new one --
	// with its own schedule, so a clip is not "everything below the first row"
	n.entries = append(n.entries, narrEntry{S: 200, E: 210, At: 1, Text: "four"})
	n.durCache[n.a.ttsWav(n.entries[3])] = 3
	if !n.clipTop(3) {
		t.Error("a row on the next stretch of footage is not the top of its own clip")
	}
	if got := len(n.clipLines(0)); got != 3 {
		t.Errorf("the first clip packs %d lines, want its own 3", got)
	}
	if got := len(n.clipLines(3)); got != 1 {
		t.Errorf("the second clip packs %d lines, want its own 1", got)
	}
	// a row with nothing written on it is not a line: there is nothing to
	// speak, so there is nothing to fit, and counting it would warn about
	// clips that are fine
	n.entries = append(n.entries, narrEntry{S: 200, E: 210, At: 4})
	if got := len(n.clipLines(3)); got != 1 {
		t.Errorf("a wordless row is packed as a line: %d lines, want 1", got)
	}
	// the second clip's line is written 1 s in, so the render may slide the
	// whole schedule that far back before it has to speed the words up --
	// down to the lead-in and no further
	if want, got := 1-narrLead, n.clipSlack(3); math.Abs(got-want) > 0.001 {
		t.Errorf("the render has %.2f s to slide this clip's lines, want %.2f", got, want)
	}
	if over := n.clipOverrun(0); over != 0 {
		t.Errorf("a clip whose lines fit reports %.2f s of overrun", over)
	}

	// now make the FIRST line long: 14 s. Packed: 0.3, 14.6, 19.9 → ends
	// 24.9, +0.2 = 25.1 against 20 s of clip. The render grows it by 4
	// (maxExtend) and still has 1.1 s to slide out of the way.
	n.durCache[n.a.ttsWav(n.entries[0])] = 14
	over := n.clipOverrun(0)
	if over < 1.0 || over > 1.2 {
		t.Errorf("the clip runs %.2f s past its end, want about 1.1", over)
	}
	// and not one of the three rows' own ⚠ would have said so: line 2 and
	// line 3 each have their placed room, and the last has the clip's end
	if n.speechDur(n.entries[1]) > n.lineWindow(1) || n.speechDur(n.entries[2]) > n.lineWindow(2) {
		t.Error("this case is supposed to be one the per-line ⚠ cannot see")
	}

	// the slide has a floor -- the first line cannot start before the lead-in
	// -- and past it the render speeds the words up instead
	if slack := n.clipSlack(0); slack != 0 {
		t.Errorf("the first line is already at the lead-in, so there is %.2f s to slide, want 0", slack)
	}
	if over <= n.clipSlack(0) {
		t.Error("this overrun is supposed to be past what sliding can absorb")
	}

	// a clip with nothing written on it is not an overrun
	n2 := &narrator{durCache: map[string]float64{}, a: n.a,
		entries: []narrEntry{{S: 0, E: 5}}}
	if over := n2.clipOverrun(0); over != 0 {
		t.Errorf("a clip with no line reports %.2f s of overrun", over)
	}

	// the row says it, on the row the whole clip hangs off
	body := funcBody(t, "narrate.go", `func \(n \*narrator\) rebuildRows\(\)`)
	if !strings.Contains(body, "n.clipTop(i)") || !strings.Contains(body, "n.clipOverrun(i)") {
		t.Error("the page still says nothing about a clip whose lines do not fit it")
	}
}

// ...and it is the RENDER's arithmetic, in one place, so the page and the
// encode cannot disagree about whether a clip fits.
func TestThePageAndTheRenderPackTheLinesTheSameWay(t *testing.T) {
	lines := []prodLine{{at: 0, dur: 5}, {at: 1, dur: 5}}
	// the second is placed 1 s in but the first is still talking, so it waits
	// for it and a breath: 0.3 + 5 + 0.3 = 5.6
	end := packLines(lines, 1)
	if lines[0].delay != narrLead {
		t.Errorf("the first line starts at %.2f, want the lead-in %.2f", lines[0].delay, narrLead)
	}
	if want := narrLead + 5 + narrGap; lines[1].delay != want {
		t.Errorf("the pushed line starts at %.2f, want %.2f", lines[1].delay, want)
	}
	if want := narrLead + 5 + narrGap + 5; end != want {
		t.Errorf("the last line ends at %.2f, want %.2f", end, want)
	}
	if run := narrRun(lines, 1); run != end+narrTail {
		t.Errorf("the slot asked for is %.2f, want the last end plus the tail %.2f", run, end+narrTail)
	}
	// sped up, the whole schedule closes up
	if fast := narrRun(lines, maxTempo); fast >= narrRun(lines, 1) {
		t.Error("speeding the lines up did not shorten what the clip has to hold")
	}
	// a clip built by hand has no tempo, and every line here divides by it
	if narrRun([]prodLine{{at: 0, dur: 4}}, 0) != narrLead+4+narrTail {
		t.Error("a zero tempo is not read as normal speed, so the arithmetic divides by zero")
	}

	// the render asks through the same door
	body := funcBody(t, "produce.go", `func \(a \*App\) produce\(`)
	if !strings.Contains(body, "narrRun(c.lines, c.tempo)") {
		t.Error("the render packs the lines its own way again, so the page's warning can be wrong")
	}
}

// A recording that reached no clip is out of the render for one of two reasons,
// and they are not the same news. One was running at another time of day --
// that is a placement to go and look at. One was silenced by every scene it
// runs under, which is the cut doing exactly what it was told, and is what the
// split-off narrator track always is. Both used to print "was not running while
// any clip was", which sent you hunting a timeline problem that was not there.
func TestASilencedRecordingIsNotReportedAsOneThatWasNeverRunning(t *testing.T) {
	c := prodClip{sessS: 100, length: 20, rate: 1, quiet: []string{"voice"}}
	voice := tlAudio{base: "voice", path: "/rec/voice.wav", start: 0, dur: 600}
	elsewhere := tlAudio{base: "later", path: "/rec/later.wav", start: 5000, dur: 60}

	if t0, t1 := laneOverlap(c, voice); t1-t0 != 20 {
		t.Errorf("the clip and the recording overlap by %.2f s, want the clip's 20", t1-t0)
	}
	if t0, t1 := laneOverlap(c, elsewhere); t1-t0 > 0 {
		t.Errorf("a recording made an hour later overlaps the clip by %.2f s", t1-t0)
	}
	// a clip played fast covers MORE session than it lasts -- 20 s of render
	// at 4x is 80 s of the day -- and it is the session stretch the recording
	// has to be cut against
	fast := c
	fast.rate = 4
	if t0, t1 := laneOverlap(fast, voice); t1-t0 != 80 {
		t.Errorf("a 20 s clip at 4x overlaps the recording by %.2f s, want the 80 s of session it covers", t1-t0)
	}
	// a freeze and a card are time ADDED to the session, so nothing was
	// recorded under them however the clock lines up
	froz := c
	froz.freeze = true
	if t0, t1 := laneOverlap(froz, voice); t1-t0 > 0 {
		t.Errorf("a held frame is reported as %.2f s of recorded session", t1-t0)
	}
	// the silenced lane really is out of the mix -- that part was never wrong
	if mix := clipMixes(c, []tlAudio{voice}); len(mix) != 0 {
		t.Errorf("a lane the scene silences is mixed in anyway: %+v", mix)
	}

	// so the run says which of the two it is
	heard := prodClip{sessS: 100, length: 20, rate: 1}
	heard.mix = clipMixes(heard, []tlAudio{voice})
	clips := []prodClip{c, heard}
	said := strings.Join(laneReport(clips, []tlAudio{voice, elsewhere}), "\n")
	if !strings.Contains(said, "voice is mixed into 1 of the 2 clips") {
		t.Errorf("a recording that reached a clip is not reported as in the render:\n%s", said)
	}
	if !strings.Contains(said, "later was not running while any clip was") {
		t.Errorf("a recording made at another time of day is not reported as misplaced:\n%s", said)
	}
	// and the one every scene it runs under leaves out is not the same news.
	// Two scenes here leave it out two different ways -- one silences the
	// lane, one drops a card's sound over it -- and neither is a misplaced
	// recording
	dropped := prodClip{sessS: 200, length: 20, rate: 1, dropLane: "voice"}
	dropped.mix = clipMixes(dropped, []tlAudio{voice})
	said = strings.Join(laneReport([]prodClip{c, dropped}, []tlAudio{voice}), "\n")
	if !strings.Contains(said, "every one of them leaves it out") {
		t.Errorf("a recording every scene leaves out is reported as one that was never running:\n%s", said)
	}
	if strings.Contains(said, "was not running") {
		t.Errorf("a recording that WAS running is reported as one that was not:\n%s", said)
	}
	if !strings.Contains(said, "runs under 2 clip(s)") {
		t.Errorf("the run does not say how many scenes it ran under:\n%s", said)
	}
}

// The render drops the game under a narration line to the Produce page's
// "game volume" -- 0.22 by default, and per CLIP: a clip with anything
// written on it has its whole bed ducked, start to end (encodeClip). The
// Narrate preview played the same footage at full level, so the one thing
// this page is for -- listening to a gap and deciding whether a sentence
// fits in it -- was being decided against a mix four times louder than the
// finished video's. Every gap sounded narrower than it was going to be.
func TestTheNarratePreviewDucksTheGameLikeTheRenderWill(t *testing.T) {
	n := &narrator{entries: []narrEntry{
		{S: 0, E: 10, At: 1, Text: "over this clip"},
		{S: 10, E: 20, At: 1}, // written on later, or never: nothing to duck for
	}}
	n.a = &App{ed: &cutEditor{}}
	def := defaultProdSettings().GameVol
	if def <= 0 || def >= 1 {
		t.Fatalf("a game volume of %.2f is not a duck", def)
	}
	// no Produce page has been built, so the level is the one it would start
	// on -- the page is not a prerequisite for hearing what it will do
	if got := n.a.gameVol(); got != def {
		t.Errorf("with no Produce page the duck is %.2f, want the default %.2f", got, def)
	}
	if got := n.gameGain(5); math.Abs(got-def) > 1e-9 {
		t.Errorf("under a spoken clip the preview plays the game at %.2f, want %.2f", got, def)
	}
	if got := n.gameGain(15); math.Abs(got-1) > 1e-9 {
		t.Errorf("a clip with nothing written on it is ducked to %.2f, want full", got)
	}
	// ...and the duck is the whole clip, not the seconds the words cover: the
	// render holds it flat across the clip, so the level must not step here
	if got := n.gameGain(9.5); math.Abs(got-def) > 1e-9 {
		t.Errorf("the far end of a spoken clip is at %.2f, want the same %.2f duck", got, def)
	}
	if !n.clipSpeaks(0) || n.clipSpeaks(10) {
		t.Error("a clip speaks over exactly its own seconds, start included and end not")
	}
	// and a volume effect still has its say, on top of the duck rather than
	// instead of it -- both reach the same one gain the player has
	n.a.ed.fx = []cutFx{{Kind: "volume", T: 0, Dur: 10, Gain: 2}}
	if got := n.gameGain(5); math.Abs(got-2*def) > 1e-9 {
		t.Errorf("a doubling effect over a ducked clip gives %.2f, want %.2f", got, 2*def)
	}
	if got := n.gameGain(15); math.Abs(got-1) > 1e-9 {
		t.Errorf("the effect leaked past its own band: %.2f", got)
	}
}

// A clip under half a second is dropped -- there are not enough frames to
// encode and splice -- and the drop happens before the narration is attached
// to it. So a sentence written over that half-second went out of the finished
// video with nothing said about it: the log mentioned the footage, which is
// the part nobody misses, and not the words, which is the part they do.
func TestDroppingATinyClipSaysWhatWasWrittenOnIt(t *testing.T) {
	seg := cutSeg{S: 100, E: 100.4}
	lines := []narrEntry{
		{S: 100, E: 100.4, At: 0, Text: "the one good joke"},
		{S: 100, E: 100.4, At: 0.2},   // nothing written: nothing lost
		{S: 200, E: 210, Text: "far"}, // another clip's line
	}
	got := spokenHere(lines, seg)
	if !strings.Contains(got, "the narration line written on it is dropped") {
		t.Errorf("dropping a clip with a line on it says %q", got)
	}
	lines = append(lines, narrEntry{S: 100, E: 100.4, At: 0.3, Text: "and the sign-off"})
	if got := spokenHere(lines, seg); !strings.Contains(got, "2 narration lines") {
		t.Errorf("two lines on a dropped clip: %q", got)
	}
	// a clip nobody wrote on adds nothing to the message -- the ordinary case,
	// and the message already says what it needs to
	if got := spokenHere(lines, cutSeg{S: 300, E: 300.4}); got != "" {
		t.Errorf("a clip with no lines on it said %q", got)
	}
	if got := spokenHere(nil, seg); got != "" {
		t.Errorf("a session with no narration at all said %q", got)
	}
	// and the drop asks
	if !strings.Contains(readSrc(t, "produce.go"), "c.length, spokenHere(entries, s))") {
		t.Error("the too-short message stopped saying what was written on the clip")
	}
}
