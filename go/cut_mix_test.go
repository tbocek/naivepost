package main

// Hearing and seeing the same second.
//
// The cut page used to show one blue lane per separate recording and play the
// footage's file. So the thing you were cutting against -- the voices, which
// are on the other recorder -- was neither audible nor comparable to anything:
// a lane on its own is a smear, and whether it sits where it belongs is not a
// question one waveform can answer. What is pinned here is the pair that makes
// it answerable: the footage's own sound is drawn too -- under its own row of
// pictures now -- from the same clock, and everything else in the session is
// played under it.

import (
	"path/filepath"
	"strings"
	"testing"

	"github.com/diamondburned/gotk4/pkg/cairo"
)

// ---- the picture ------------------------------------------------------------

// The same instant, recorded twice, has to be drawn in the same column.
//
// Two files are made that share one moment and nothing else: the footage
// carries a tone from 10:00:06 to 10:00:08, and a recording that started four
// seconds earlier carries a tone over exactly the same two seconds of the
// afternoon. Both go through the ordinary path -- sessionTracks places them,
// masterLanes makes the footage's track a lane, the real decoder builds both
// envelopes -- and then the lanes are painted and asked where their loud part
// is. Off-by-one-recording's-start is the whole failure mode of this page, and
// it is invisible in every unit that does not draw.
func TestTheSameInstantIsDrawnInTheSameColumn(t *testing.T) {
	a := insertApp(t)
	dir := t.TempDir()

	// footage from 10:00:04, eight seconds, tone over its own 2..4 s
	footage := filepath.Join(dir, "game_2026-01-01_10-00-04.mp4")
	mustFFmpeg(t, "-f", "lavfi", "-t", "8", "-i", "testsrc=size=160x120:rate=15",
		"-f", "lavfi", "-t", "2", "-i", "anullsrc=r=48000:cl=mono",
		"-f", "lavfi", "-t", "2", "-i", "sine=frequency=300:sample_rate=48000",
		"-f", "lavfi", "-t", "4", "-i", "anullsrc=r=48000:cl=mono",
		"-filter_complex", "[1:a][2:a][3:a]concat=n=3:v=0:a=1[a]",
		"-map", "0:v", "-map", "[a]", "-c:v", "libx264", "-preset", "ultrafast",
		"-pix_fmt", "yuv420p", "-c:a", "aac", footage)

	// the recording from 10:00:00, twelve seconds, tone over its own 6..8 s --
	// the same two seconds of the afternoon, told in the other recorder's clock
	mic := filepath.Join(dir, "mic_2026-01-01_10-00-00.wav")
	mustFFmpeg(t, "-f", "lavfi", "-t", "6", "-i", "anullsrc=r=48000:cl=mono",
		"-f", "lavfi", "-t", "2", "-i", "sine=frequency=900:sample_rate=48000",
		"-f", "lavfi", "-t", "4", "-i", "anullsrc=r=48000:cl=mono",
		"-filter_complex", "[0:a][1:a][2:a]concat=n=3:v=0:a=1[a]", "-map", "[a]",
		"-c:a", "pcm_s16le", mic)

	vids, recs, err := a.sessionTracks([]string{footage}, []string{mic})
	if err != nil {
		t.Fatal(err)
	}
	for i := range recs {
		recs[i].chans = ffprobeChannels(recs[i].path)
	}
	ed := &cutEditor{a: a, vids: vids, pps: 40, waves: map[string]*waveform{}}
	ed.auds = append(srcLanes(vids, nil), recs...)
	sortLanes(ed.auds)
	if len(ed.auds) != 2 || !ed.auds[0].master || ed.auds[1].master {
		t.Fatalf("the lanes came out as %+v, want the footage's own first", ed.auds)
	}
	ed.relayout()
	for i, au := range ed.auds {
		wf, err := loadWave(a.waveCache(), au)
		if err != nil {
			t.Fatalf("no envelope for lane %d (%s): %v", i, au.base, err)
		}
		ed.waves[au.base] = wf
	}

	// the two waves are no longer rows of one band: the footage's own sound
	// is the paired strip under its pictures, in the dim voice, and only the
	// recorder is in the band below. Same clock, two painters -- so each is
	// rendered by its own, over the same 500 px, and asked the same question.
	pairAt := renderInk(t, 500, int(waveLaneH), func(cr *cairo.Context) {
		ed.drawPairStrip(cr, ed.vids[0], ed.auds[0], 0, 0, 500)
	})
	bandAt := renderAudio(t, ed, 500, ed.audioHeight())
	// the loud part of each lane, in px. Read across the row just above the
	// lane's floor, which a column of any height at all covers -- the fill
	// stands up from there -- so what separates sound from silence on that row
	// is the ink and not the height. The quiet stretches have the baseline
	// under them, and the baseline is a fraction of the blue over dark ground,
	// well short of what either isBlue will take.
	span := func(at func(x, y int) (r, g, b uint8), y int, blue func(r, g, b uint8) bool) (int, int) {
		t.Helper()
		lo, hi := -1, -1
		for x := 0; x < 500; x++ {
			if r, g, b := at(x, y); blue(r, g, b) {
				if lo < 0 {
					lo = x
				}
				hi = x
			}
		}
		return lo, hi
	}
	m0, m1 := span(pairAt, int(waveLaneH)-2, isDimBlue)      // the footage's own sound
	r0, r1 := span(bandAt, int(wavePad+waveLaneH)-2, isBlue) // the separate recording
	t.Logf("footage tone at px %d..%d, recording tone at px %d..%d", m0, m1, r0, r1)

	// 40 px/s, and the footage's own tone starts 2 s into it
	if m0 < 0 || m1 < 0 {
		t.Fatal("the footage's own sound was not drawn at all — there is no lane to compare against")
	}
	if r0 < 0 || r1 < 0 {
		t.Fatal("the recording's lane is empty where it was recording a tone")
	}
	if m0 < gutterPx+74 || m0 > gutterPx+86 {
		t.Errorf("the footage's tone starts at px %d, want ~%g (2 s at 40 px/s, "+
			"past the gutter)", m0, gutterPx+80)
	}
	// the point of the whole file: the two clocks put it in the same column
	if d := r0 - m0; d < -4 || d > 4 {
		t.Errorf("the recording's tone starts %d px from the footage's (%d vs %d) — "+
			"%.2f s out; the lanes are not on the footage's clock", d, r0, m0, float64(d)/ed.pps)
	}
	if d := r1 - m1; d < -4 || d > 4 {
		t.Errorf("the recording's tone ends %d px from the footage's (%d vs %d)", d, r1, m1)
	}
}

// A video with no sound gets no lane: the lane would be a strip of ground with
// nothing in it and a decode that can only fail, and the file is in the session
// for its pictures.
func TestASilentVideoHasNoLane(t *testing.T) {
	insertApp(t) // for ffmpeg; the App itself is not needed here
	dir := t.TempDir()
	silent := filepath.Join(dir, "silent_2026-01-01_10-00-00.mp4")
	mustFFmpeg(t, "-f", "lavfi", "-t", "1", "-i", "testsrc=size=160x120:rate=15",
		"-c:v", "libx264", "-preset", "ultrafast", "-pix_fmt", "yuv420p", silent)
	if got := ffprobeChannels(silent); got != 0 {
		t.Errorf("a video with no audio stream probes as %d channel(s), want 0", got)
	}
	if lanes := srcLanes([]tlVideo{{base: "silent", path: silent, dur: 1}}, nil); len(lanes) != 0 {
		t.Errorf("a silent video was given %d lane(s): %+v", len(lanes), lanes)
	}
}

// ---- the sound --------------------------------------------------------------

// What the preview plays under a piece of footage: every recording that was
// running while it ran, each with the offset that turns a time in the footage
// into a time in that recording. Not the footage's own track -- that is what
// the picture is already playing, and a second copy of it half a frame late is
// a broken speaker, not a mix.
func TestThePreviewPlaysTheRecordingsUnderTheFootage(t *testing.T) {
	v := tlVideo{base: "game", path: "/x/game.mp4", start: 100, dur: 60}
	ed := &cutEditor{
		vids: []tlVideo{v},
		auds: []tlAudio{
			// the footage's own sound: a lane, never a mix input
			{base: "game", path: "/x/game.mp4", start: 100, dur: 60, master: true},
			// running from long before: the footage's 0 is its own 90 s
			{base: "mic", path: "/x/mic.wav", start: 10, dur: 300},
			// started after the footage did, and still running at its end
			{base: "late", path: "/x/late.wav", start: 130, dur: 100},
			// a recording of another part of the day entirely
			{base: "other", path: "/x/other.wav", start: 4000, dur: 600},
		},
	}
	got := ed.mixUnder(&v)
	want := []mixTrack{
		// named, because the scene that silences a lane names it (cut_hush_test.go)
		{base: "mic", path: "/x/mic.wav", delta: 90, hi: 300},
		{base: "late", path: "/x/late.wav", delta: -30, hi: 100},
	}
	if len(got) != len(want) {
		t.Fatalf("%d recording(s) go under the footage, want %d: %+v", len(got), len(want), got)
	}
	for i := range want {
		if got[i] != want[i] {
			t.Errorf("%s comes out as %+v, want %+v", want[i].path, got[i], want[i])
		}
	}

	// and the offsets mean what they say: at the head of the footage the mic is
	// 90 s into itself, and the one that started 30 s later is not playing yet
	mic := &auxAudio{delta: 90, hi: 300}
	late := &auxAudio{delta: -30, hi: 100}
	if !mic.running(0) || !mic.running(60) {
		t.Error("the mic was recording throughout and the preview would not play it")
	}
	if late.running(0) {
		t.Error("a recording that started 30 s into the footage would be played at its head — " +
			"which is 30 s of the wrong sound, in sync with nothing")
	}
	if !late.running(30.5) {
		t.Error("the late recording is not played once it has started")
	}
	// past its own end it is silence, not its last second held or its first
	// second again
	if late.running(140) {
		t.Error("a recording that had already stopped would still be played")
	}
}

// The three ways a lane came to be silent while its badge was green, pinned
// as arithmetic: a further track of the capture is in the mix and knows its
// track; a cut lane's delta carries its window's offset; and running is asked
// against the window, not against second nought of the file.
func TestEveryLaneTheBadgeShowsIsInThePreviewMix(t *testing.T) {
	v := &tlVideo{base: "cap", path: "/r/cap.mkv", start: 100, dur: 600}
	ed := &cutEditor{auds: []tlAudio{
		{base: "cap", path: "/r/cap.mkv", start: 100, dur: 600, master: true},
		{base: "cap #2", path: "/r/cap.mkv", start: 100, dur: 600, track: 1},
		{base: "mic", path: "/r/mic.wav", start: 90, dur: 700},
		// a cut lane: a window opening 30 s into a file, put at session 400
		{base: "lane", path: "/r/other.mkv", start: 400, off: 30, dur: 60},
	}}
	got := ed.mixUnder(v)
	want := map[string]mixTrack{
		"cap #2": {base: "cap #2", path: "/r/cap.mkv", delta: 0, lo: 0, hi: 600, track: 1},
		"mic":    {base: "mic", path: "/r/mic.wav", delta: 10, lo: 0, hi: 700},
		"lane":   {base: "lane", path: "/r/other.mkv", delta: 100 - 400 + 30, lo: 30, hi: 90},
	}
	if len(got) != len(want) {
		t.Fatalf("the mix is %+v, want the second track, the mic and the cut lane", got)
	}
	for _, m := range got {
		if w, ok := want[m.base]; !ok || m != w {
			t.Errorf("lane %s came out %+v, want %+v", m.base, m, w)
		}
	}
	// the master's file second 0 is session 100; the cut lane's window runs
	// session 400-460, which is its file's 30-90
	lane := &auxAudio{delta: want["lane"].delta, lo: 30, hi: 90}
	for _, c := range []struct {
		masterSec float64
		want      bool
	}{{299, false}, {300, true}, {330, true}, {360, true}, {361, false}} {
		if got := lane.running(c.masterSec); got != c.want {
			t.Errorf("at the master's second %g the cut lane running = %v, want %v", c.masterSec, got, c.want)
		}
	}
	for _, m := range got {
		if m.path == v.path && m.track == 0 {
			t.Errorf("the master track is in the mix as %+v", m)
		}
	}
}

// A lane standing at its stop is placed again only once the scene under the
// line has moved on -- the new stop is ahead of the old one -- and never while
// the line still reads as the old scene: a seek whose stop is already behind
// it is a seek with no stop, and the lane would run on past the boundary.
func TestALaneAtItsStopWaitsForTheNextScene(t *testing.T) {
	body := funcBody(t, "player.go", `func \(p \*Player\) applyMute\(\) \{`)
	if !strings.Contains(body, "p.stopFor(a) > a.stopAt") {
		t.Errorf("applyMute re-places a stopped lane under the same stop:\n%s", body)
	}
	// and the stop itself is GStreamer's: a seek with a stop position
	seek := funcBody(t, "player.go", `func \(a \*auxAudio\) seekTo\(`)
	if !strings.Contains(seek, "gst.SeekTypeSet, int64(a.stopAt*1e9)") {
		t.Errorf("seekTo no longer hands the stop to the seek:\n%s", seek)
	}
}
