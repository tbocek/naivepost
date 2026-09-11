package main

// The audio lanes. The video track is the master and the lanes are drawn
// against it, so what has to be pinned here is the arithmetic that keeps a
// second recording honest: that a lane reads its own clock (the two machines
// started at different moments), that only the part overlapping the footage is
// painted, that the two channels stay apart, and that a cached envelope is
// thrown away when the file under it changed. The drawing is checked by
// rendering it to an image surface and looking at the pixels, because "only the
// relevant part, in blue" is a claim about pixels and nothing short of pixels
// witnesses it.

import (
	"encoding/binary"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strings"
	"testing"

	"github.com/diamondburned/gotk4/pkg/cairo"
)

// ---- the envelope -----------------------------------------------------------

// A drawn column is many buckets wide when zoomed out and a fraction of one when
// zoomed in. Both have to answer, and the wide one has to answer with the
// loudest bucket: an envelope that averaged would quietly fade out as you zoomed
// out, which is the one thing a waveform must not do.
func TestTheLoudestThingInTheWindowWins(t *testing.T) {
	wf := &waveform{hz: 100, chans: [][]uint8{
		// 0.00-0.01 s ... 0.05-0.06 s
		{0, 10, 200, 10, 0, 255},
		{1, 1, 1, 1, 1, 1},
	}}
	for _, c := range []struct {
		name     string
		ch       int
		from, to float64
		want     float64
	}{
		{"a window over many buckets takes the loudest", 0, 0, 0.04, 200.0 / 255},
		{"a window inside one bucket takes that one", 0, 0.021, 0.0219, 200.0 / 255},
		{"a window of no width still reads a bucket", 0, 0.05, 0.05, 1},
		{"a quiet stretch is quiet", 0, 0.03, 0.05, 10.0 / 255},
		{"the other channel is its own signal", 1, 0, 0.06, 1.0 / 255},
		{"before the recording", 0, -5, -1, 0},
		{"past the end of it", 0, 9, 10, 0},
		{"a channel the recording does not have", 5, 0, 0.06, 0},
	} {
		if got := wf.peak(c.ch, c.from, c.to); got != c.want {
			t.Errorf("%s: peak(%d, %g, %g) = %g, want %g", c.name, c.ch, c.from, c.to, got, c.want)
		}
	}
	var none *waveform
	if got := none.peak(0, 0, 1); got != 0 {
		t.Errorf("an envelope that has not arrived yet reads %g, want 0", got)
	}
}

func TestAnEnvelopeSurvivesTheRoundTrip(t *testing.T) {
	file := filepath.Join(t.TempDir(), "a.wave")
	wf := &waveform{hz: waveHz, chans: [][]uint8{{0, 7, 255, 9}, {3, 3, 0, 128}}}
	if err := writeWave(file, wf, 1234, 5678); err != nil {
		t.Fatal(err)
	}
	got, ok := readWave(file, 1234, 5678)
	if !ok {
		t.Fatal("the envelope just written back was not read")
	}
	if got.hz != wf.hz || len(got.chans) != 2 {
		t.Fatalf("read back hz %g with %d channels, want %g with 2", got.hz, len(got.chans), wf.hz)
	}
	for c := range wf.chans {
		if string(got.chans[c]) != string(wf.chans[c]) {
			t.Errorf("channel %d read back as %v, want %v", c, got.chans[c], wf.chans[c])
		}
	}
}

// The cache is keyed by the recording, not by its name. Recording a new session
// over the old filename is the normal way to work, and drawing yesterday's
// envelope under today's audio is a picture that lies rather than one that is
// merely stale.
func TestACachedEnvelopeIsRejectedWhenTheRecordingChanged(t *testing.T) {
	dir := t.TempDir()
	file := filepath.Join(dir, "a.wave")
	if err := writeWave(file, &waveform{hz: waveHz, chans: [][]uint8{{1, 2, 3}}}, 1000, 2000); err != nil {
		t.Fatal(err)
	}
	for _, c := range []struct {
		name         string
		size, mtime  int64
		wantAccepted bool
	}{
		{"the same file", 1000, 2000, true},
		{"re-recorded to a different length", 1001, 2000, false},
		{"re-recorded at the same length", 1000, 2001, false},
	} {
		if _, ok := readWave(file, c.size, c.mtime); ok != c.wantAccepted {
			t.Errorf("%s: accepted=%v, want %v", c.name, ok, c.wantAccepted)
		}
	}
	// a cache written by an older version of this format is not read as if it
	// were this one
	b, err := os.ReadFile(file)
	if err != nil {
		t.Fatal(err)
	}
	old := filepath.Join(dir, "old.wave")
	if err := os.WriteFile(old, append([]byte("AWV1"), b[len(waveMagic):]...), 0o644); err != nil {
		t.Fatal(err)
	}
	if _, ok := readWave(old, 1000, 2000); ok {
		t.Error("a cache with a foreign magic was read as this format")
	}
	// ...and neither is a truncated one, which is what a cache written by a run
	// that was killed halfway looks like
	short := filepath.Join(dir, "short.wave")
	if err := os.WriteFile(short, b[:len(b)-2], 0o644); err != nil {
		t.Fatal(err)
	}
	if _, ok := readWave(short, 1000, 2000); ok {
		t.Error("a half-written cache was read as a whole one")
	}
	if _, ok := readWave(filepath.Join(dir, "nothing.wave"), 1000, 2000); ok {
		t.Error("a cache that does not exist was read")
	}
	// the header is what the reader relies on to know how much to read, so its
	// size is part of the format
	if n := binary.Size(waveHead{}); n != 1+2+4+8+8 {
		t.Errorf("the header is %d bytes; the format changed without the magic changing", n)
	}
}

// ---- the decode -------------------------------------------------------------

// Only ffmpeg can witness this one. buildWave reads interleaved little-endian
// s16 out of a pipe and takes a peak per bucket per channel, and every part of
// that is quietly easy to get wrong in a way no unit test on a hand-built
// envelope would notice: the byte order, the interleaving, the bucket size. So
// the test makes a stereo file whose two channels differ and whose loud part is
// somewhere known, and asks the envelope where the noise is.
func TestARealRecordingIsDecodedIntoTheEnvelopeItSoundsLike(t *testing.T) {
	dir := t.TempDir()
	wav := filepath.Join(dir, "mic.wav")
	// left: a second of near full-scale tone, then two of silence. right: silent
	// throughout. Written as one stereo expression rather than two sources
	// joined, because joining a mono source into a stereo layout goes through a
	// channel-layout conversion that quietly attenuates it, and a test whose
	// signal is 18 dB down is not testing the amplitude it says it is.
	if err := exec.Command("ffmpeg", "-v", "error", "-y", "-f", "lavfi",
		"-i", "aevalsrc='if(lt(t,1),0.9*sin(2*PI*440*t),0)|0':d=3:s=44100",
		"-c:a", "pcm_s16le", wav).Run(); err != nil {
		t.Skipf("no usable ffmpeg: %v", err)
	}
	if got := ffprobeChannels(wav); got != 2 {
		t.Fatalf("a stereo file was probed as %d channel(s)", got)
	}
	wf, err := buildWave(wav, 0, 2)
	if err != nil {
		t.Fatal(err)
	}
	if wf.hz != waveHz || len(wf.chans) != 2 {
		t.Fatalf("decoded at %g Hz into %d channels, want %g into 2", wf.hz, len(wf.chans), waveHz)
	}
	// three seconds at a bucket per 10 ms, give or take the last partial one
	if n := len(wf.chans[0]); n < 295 || n > 305 {
		t.Errorf("three seconds came out as %d buckets, want about 300", n)
	}
	if p := wf.peak(0, 0.2, 0.8); p < 0.5 {
		t.Errorf("the tone reads as %g, want most of the way up", p)
	}
	if p := wf.peak(0, 1.5, 2.5); p > 0.02 {
		t.Errorf("the silence after the tone reads as %g — the buckets are off", p)
	}
	if p := wf.peak(1, 0, 3); p > 0.02 {
		t.Errorf("the silent channel reads as %g — the channels are interleaved the wrong way", p)
	}

	// ...and the whole of that happens once: loadWave leaves an envelope on
	// disk, and the second look reads it rather than starting ffmpeg again
	cache := filepath.Join(dir, "cache")
	first, err := loadWave(cache, tlAudio{base: baseName(wav), path: wav, chans: 2})
	if err != nil {
		t.Fatal(err)
	}
	cf := filepath.Join(cache, baseName(wav)+".wave")
	if !exists(cf) {
		t.Fatalf("no envelope was cached at %s", cf)
	}
	fi, err := os.Stat(wav)
	if err != nil {
		t.Fatal(err)
	}
	cached, ok := readWave(cf, fi.Size(), fi.ModTime().Unix())
	if !ok {
		t.Fatal("the envelope loadWave just cached does not read back for its own recording")
	}
	if string(cached.chans[0]) != string(first.chans[0]) {
		t.Error("what was cached is not what was decoded")
	}
	// a file that is not audio at all is an error, not an empty lane drawn as
	// silence
	notAudio := filepath.Join(dir, "notes.txt")
	if err := os.WriteFile(notAudio, []byte("this is not a recording"), 0o644); err != nil {
		t.Fatal(err)
	}
	if _, err := buildWave(notAudio, 0, 1); err == nil {
		t.Error("a text file decoded into a waveform")
	}
}

// ---- the room the lanes take ------------------------------------------------

func TestTheLanesTakeRoomOnlyWhenThereIsAudio(t *testing.T) {
	ed := newTestEd(t)
	if ed.audioLanes() != 0 || ed.audioHeight() != 0 {
		t.Errorf("a session with no separate recording asks for %d lanes / %d px, want none",
			ed.audioLanes(), ed.audioHeight())
	}
	ed.auds = []tlAudio{{base: "mic", chans: 2}, {base: "room", chans: 1}}
	if got := ed.audioLanes(); got != 3 {
		t.Fatalf("a stereo and a mono recording make %d lanes, want 3", got)
	}
	want := int(3*waveLaneH + waveGap + 2*wavePad)
	if got := ed.audioHeight(); got != want {
		t.Errorf("three lanes are %d px tall, want %d", got, want)
	}
	// a recording whose channel count never arrived still gets a lane rather
	// than none: something was recorded, and a missing probe is not a reason to
	// pretend otherwise
	ed.auds = []tlAudio{{base: "mic", chans: 0}}
	if got := ed.audioLanes(); got != 1 {
		t.Errorf("a recording with an unknown channel count got %d lanes, want 1", got)
	}
}

func TestALaneSaysWhichChannelItIs(t *testing.T) {
	// a mono recording says "mono" rather than calling its one channel L, which
	// would imply an R that was never recorded -- and a stereo recording drawn on
	// one lane says why it is on one lane
	for _, c := range []struct {
		ch, lanes, chans int
		want             string
	}{
		{0, 1, 1, "mono"}, {0, 1, 0, "mono"},
		{0, 2, 2, "L"}, {1, 2, 2, "R"},
		{0, 1, 2, "L=R"},
	} {
		if got := laneName(c.ch, c.lanes, c.chans); got != c.want {
			t.Errorf("channel %d of %d on %d lane(s) is called %q, want %q",
				c.ch, c.chans, c.lanes, got, c.want)
		}
	}
}

// The two clocks differ by exactly where each machine started, which is the
// whole of the alignment: a px on the timeline is a session time through the
// video it belongs to, and the recording's own time is that minus its start.
func TestALaneReadsItsOwnClock(t *testing.T) {
	v := tlVideo{start: 200, dur: 600, pxOrigin: 1000}
	au := tlAudio{start: 150, dur: 900} // started 50 s before the capture card
	// px 1000 is session 200, which the recorder was 50 s into
	if got := au.timeAt(v, 4, 1000); got != 50 {
		t.Errorf("the start of the footage is %g s into the recording, want 50", got)
	}
	// forty px later is ten session seconds later, in both clocks
	if got := au.timeAt(v, 4, 1040); got != 60 {
		t.Errorf("ten seconds on is %g s into the recording, want 60", got)
	}
	// a recording that started after the footage reads negative before it began
	late := tlAudio{start: 260, dur: 100}
	if got := late.timeAt(v, 4, 1000); got != -60 {
		t.Errorf("a recording that started a minute late reads %g at the head of the footage, want -60", got)
	}
}

// ---- the picture ------------------------------------------------------------

// renderAudio paints the lanes into an image surface and hands back a reader
// for its pixels.
func renderAudio(t *testing.T, ed *cutEditor, w, h int) func(x, y int) (r, g, b uint8) {
	t.Helper()
	return renderInk(t, w, h, func(cr *cairo.Context) { ed.drawAudio(cr, w, h) })
}

// renderInk runs one painter over a fresh surface and hands back a reader for
// its pixels. Cairo's ARGB32 is premultiplied and little-endian, so a byte
// quad is B, G, R, A.
func renderInk(t *testing.T, w, h int, paint func(cr *cairo.Context)) func(x, y int) (r, g, b uint8) {
	t.Helper()
	surf := cairo.CreateImageSurface(cairo.FormatARGB32, w, h)
	cr := cairo.Create(surf)
	paint(cr)
	surf.Flush()
	data, stride := surf.Data(), surf.Stride()
	pix := make([]byte, len(data))
	copy(pix, data) // off the C heap before the surface is collected
	runtime.KeepAlive(surf)
	return func(x, y int) (uint8, uint8, uint8) {
		i := y*stride + x*4
		return pix[i+2], pix[i+1], pix[i]
	}
}

// isBlue is the waveform's ink and nothing else on the page: the ground under a
// lane is a dark slate and the centre line is that ground with a third of the
// blue over it, both far short of this.
func isBlue(r, _, b uint8) bool { return b > 200 && r < 150 }

// isDimBlue is the same ink in the paired strip's voice: at just over half
// strength the columns land near b=153, still twice the strip's own baseline
// (~70) and far over its ground (~28).
func isDimBlue(r, _, b uint8) bool { return b > 110 && r < 90 }

func waveEd(t *testing.T) *cutEditor {
	t.Helper()
	ed := newTestEd(t) // pps 4: a session second is four px
	ed.vids = []tlVideo{{base: "v", path: "v.mkv", start: 0, dur: 600, interval: 5, fps: 30}}
	ed.relayout()
	// a recording that started 100 s before the capture card and ran 300 s, so
	// it covers session -100..200 and overlaps the footage over 0..200 (px
	// 0..800). Its left channel is loud over its own 100..150 s -- session
	// 0..50, px 0..200 -- and silent everywhere else; its right channel is
	// silent throughout.
	ed.auds = []tlAudio{{base: "mic", path: "mic.wav", start: -100, dur: 300, chans: 2}}
	left := make([]uint8, 30000)
	for i := 10000; i < 15000; i++ {
		left[i] = 255
	}
	ed.waves = map[string]*waveform{"mic": {hz: 100, chans: [][]uint8{left, make([]uint8, 30000)}}}
	return ed
}

// Only the part of the recording that overlaps the footage is drawn, and it is
// drawn where the wall clock says it happened rather than from the left edge.
func TestOnlyTheOverlappingPartOfARecordingIsDrawn(t *testing.T) {
	ed := waveEd(t)
	at := renderAudio(t, ed, 900, ed.audioHeight())

	blueIn := func(x int) bool {
		for y := int(wavePad); y < int(wavePad+waveLaneH); y++ {
			if isBlue(at(x, y)) {
				return true
			}
		}
		return false
	}
	for _, c := range []struct {
		name string
		x    int
		want bool
	}{
		// px, and the tape starts a strip in from the widget's edge now
		// (cut_gutter.go): every x below is that offset plus the second it means
		{"the head of the footage, where the recording was already loud", gutterPx + 4, true},
		{"still loud a few seconds in", gutterPx + 100, true},
		{"the recorder went quiet (session 50 on)", gutterPx + 260, false},
		{"past the end of the recording (session 200 on)", gutterPx + 850, false},
	} {
		if got := blueIn(c.x); got != c.want {
			t.Errorf("%s: blue at px %d = %v, want %v", c.name, c.x, got, c.want)
		}
	}
	// ...and the lane's own ground stops with the recording, so a silent
	// stretch of it and the emptiness after it are not the same picture
	ground := func(x int) bool {
		r, g, b := at(x, int(wavePad)+2)
		return r > 30 && r < 60 && g > 30 && g < 60 && b > 40 && b < 70
	}
	if !ground(260) {
		t.Error("a silent stretch of the recording has no ground under it — it reads as nothing recorded")
	}
	if ground(850) {
		t.Error("the lane is drawn past the end of the recording")
	}
}

// The channels are separate lanes because a stereo recording of a group is
// often not a stereo picture at all -- one player per side, or a mic on one
// channel and the game on the other -- and one merged lane hides exactly that.
func TestTheChannelsAreDrawnApart(t *testing.T) {
	ed := waveEd(t)
	at := renderAudio(t, ed, 900, ed.audioHeight())
	blueInLane := func(lane int) bool {
		y0 := int(wavePad) + lane*int(waveLaneH)
		for y := y0; y < y0+int(waveLaneH); y++ {
			for x := 0; x < 900; x++ {
				if isBlue(at(x, y)) {
					return true
				}
			}
		}
		return false
	}
	if !blueInLane(0) {
		t.Error("the loud channel drew nothing")
	}
	if blueInLane(1) {
		t.Error("the silent channel drew the other one's signal — the lanes are not separate")
	}
}

// A recording whose envelope is still being decoded is still placed: the page
// comes up while ffmpeg is reading, and a lane that appears from nowhere a few
// seconds later reads as a bug.
func TestARecordingWithNoEnvelopeYetStillHasItsGround(t *testing.T) {
	ed := waveEd(t)
	ed.waves = map[string]*waveform{}
	at := renderAudio(t, ed, 900, ed.audioHeight())
	r, g, b := at(100, int(wavePad)+2)
	if r < 30 || r > 60 || g < 30 || g > 60 || b < 40 || b > 70 {
		t.Errorf("a recording still being decoded painted rgb(%d,%d,%d) where its ground should be", r, g, b)
	}
}

// ---- the wiring -------------------------------------------------------------

func TestTheLanesAreWiredUnderTheCut(t *testing.T) {
	b, err := os.ReadFile("cut.go")
	if err != nil {
		t.Fatal(err)
	}
	src := string(b)
	for _, want := range []string{
		"band.Append(ed.audArea) // the recorders' band: the sound nobody filmed",
		"ed.audArea.SetDrawFunc(",
		// every sound in the session gets a lane: the footage's own first, then
		// the sources the Inputs step did NOT mark as footage, placed by the
		// same zero as everything else and sorted back onto the one clock
		"ed.auds = srcLanes(ed.vids, a.snappedTracks())",
		"for _, s := range all[len(vids):] {",
		"start: s.start - zero,",
		"chans: max(1, ffprobeChannels(s.path)),",
		"sortLanes(ed.auds)",
		// and the preview plays them: which recordings are under a piece of
		// footage is settled when the file it is playing changes
		"ed.player.SetMix(ed.mixUnder(v))",
		// zoom, scroll, the playhead and the edge all repaint the lanes too
		"if ed.audArea != nil {\n\t\ted.audArea.QueueDraw()",
		"for _, area := range []*gtk.DrawingArea{ed.srcArea, ed.audArea} {",
	} {
		if !strings.Contains(src, want) {
			t.Errorf("the cut page no longer contains %q", want)
		}
	}
	// the envelopes arrive on their own time and land on the GUI thread, from
	// wherever the lanes were last changed -- a reload, or a lane added by hand
	for _, want := range []string{
		"go func() {",
		"wf, err := loadWave(a.waveCache(), au)",
		"ed.waves[au.base] = wf",
	} {
		if !strings.Contains(readSrc(t, "cut_audio.go"), want) {
			t.Errorf("cut_audio.go no longer contains %q", want)
		}
	}
	for _, want := range []string{"ed.loadWaves()"} {
		if !strings.Contains(src, want) {
			t.Errorf("the cut page no longer asks for the envelopes: %q", want)
		}
		if !strings.Contains(readSrc(t, "cut_lane.go"), want) {
			t.Errorf("a lane added by hand no longer asks for its envelope: %q", want)
		}
	}
	// the lanes are drawn against the video layout, not laid out themselves: a
	// late envelope must cost a redraw and nothing more
	if strings.Contains(src, "for i := range ed.auds {\n\t\tif i > 0 {") {
		t.Error("the audio is being given its own geometry — the video track is the master")
	}
}
