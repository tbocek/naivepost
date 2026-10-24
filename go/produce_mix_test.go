package main

// The separate recordings in the render.
//
// The cut page has drawn them as lanes for a while -- the headset recorder, the
// mic on the table, OBS's second track -- and the render did not have them at
// all: what came out carried the capture card's own sound and the narration
// over it, so everyone who was actually talking was in the picture and not in
// the video. What is pinned here is the join between the two: that the stretch
// of a recording a clip falls on is the stretch that ends up under it, that a
// card gets none of it, and -- with ffmpeg as the only witness that can settle
// it -- that the sound is really in the file and really where it was recorded.

import (
	"fmt"
	"os/exec"
	"path/filepath"
	"regexp"
	"strconv"
	"testing"
)

// ---- the arithmetic ---------------------------------------------------------

// A clip is a stretch of the session clock and a recording is another one, and
// what belongs under the clip is the overlap -- read from the recording's own
// middle when it started first, waited for when it started later. Every case
// here is one somebody's session actually has in it.
func TestOnlyTheStretchThatWasRunningEndsUpUnderTheClip(t *testing.T) {
	// the footage sits at 100..160 s of the session; the clip is 120..140
	v := tlVideo{base: "game", path: "/x/game.mp4", start: 100, dur: 60}
	clip := prodClip{video: &v, local: 20, sessS: 120, length: 20, tempo: 1}

	recs := []tlAudio{
		// running the whole time, from long before: heard from its own 110 s
		{base: "mic", path: "/x/mic.wav", start: 10, dur: 300},
		// started after the clip did: comes in 5 s late and plays to the end
		{base: "late", path: "/x/late.wav", start: 125, dur: 100},
		// stopped in the middle of it: 5 s of audio, then the clip carries on
		{base: "short", path: "/x/short.wav", start: 115, dur: 10},
		// a recording of something else entirely, hours away
		{base: "other", path: "/x/other.wav", start: 4000, dur: 600},
		// touching the clip's edge and no more: nothing worth an input
		{base: "edge", path: "/x/edge.wav", start: 119.95, dur: 0.05},
	}
	got := clipMixes(clip, recs)
	want := []prodMix{
		{base: "mic", path: "/x/mic.wav", at: 0, ss: 110, dur: 20},
		{base: "late", path: "/x/late.wav", at: 5, ss: 0, dur: 15},
		{base: "short", path: "/x/short.wav", at: 0, ss: 5, dur: 5},
	}
	if len(got) != len(want) {
		t.Fatalf("%d recording(s) went under the clip, want %d: %+v", len(got), len(want), got)
	}
	for i := range want {
		if got[i] != want[i] {
			t.Errorf("%s came out as %+v, want %+v", want[i].base, got[i], want[i])
		}
	}

	// A card is time added to the cut, not a moment of the session: there is no
	// stretch of any recording that belongs under it, and dropping the room
	// audio in there would play a sentence that was said somewhere else.
	card := prodClip{ins: "/x/tier.svg", length: 5, tempo: 1}
	if m := clipMixes(card, recs); m != nil {
		t.Errorf("a card was given %d recording(s) to play under it: %+v", len(m), m)
	}
}

// ---- the render -------------------------------------------------------------

// The one ffmpeg can settle. A separate recording that was running while the
// footage ran has to be audible in the produced clip, at the moment it was
// recorded at -- which is a claim about a filter graph with input indices,
// -ss on an input, adelay in milliseconds and an amix in it, every one of
// which is quietly easy to get wrong in a way that still produces a file.
//
// So the recording is silent apart from a two-second tone at a frequency
// nothing else in the clip has, and the finished clip is asked, second by
// second, where that tone is. Narration is in the mix too: the voice inputs and
// the recording inputs share one numbering, and a test without both would not
// notice them trading places.
func TestASeparateRecordingIsHeardWhereItWasRecorded(t *testing.T) {
	a := insertApp(t)
	dir := t.TempDir()

	// The footage: eight seconds from 10:00:04, sounding a low tone throughout.
	footage := filepath.Join(dir, "game_2026-01-01_10-00-04.mp4")
	mustFFmpeg(t, "-f", "lavfi", "-t", "8", "-i", "testsrc=size=320x240:rate=30",
		"-f", "lavfi", "-t", "8", "-i", "sine=frequency=300:sample_rate=48000",
		"-c:v", "libx264", "-preset", "ultrafast", "-pix_fmt", "yuv420p", "-c:a", "aac", footage)

	// The recording: twelve seconds from 10:00:00 -- four seconds before the
	// capture card, which is the normal way round -- silent but for a high tone
	// from 6 s to 8 s into it. On the session clock that is 6..8, and the clip
	// starts at 4, so it has to land 2 s into the finished clip.
	mic := filepath.Join(dir, "mic_2026-01-01_10-00-00.wav")
	mustFFmpeg(t, "-f", "lavfi", "-t", "6", "-i", "anullsrc=r=48000:cl=mono",
		"-f", "lavfi", "-t", "2", "-i", "sine=frequency=4000:sample_rate=48000",
		"-f", "lavfi", "-t", "4", "-i", "anullsrc=r=48000:cl=mono",
		"-filter_complex", "[0:a][1:a][2:a]concat=n=3:v=0:a=1[a]", "-map", "[a]",
		"-c:a", "pcm_s16le", mic)

	// a line of narration, so the voice inputs and the recording inputs are
	// both in the graph and cannot swap places unnoticed
	voice := filepath.Join(dir, "line.wav")
	mustFFmpeg(t, "-f", "lavfi", "-t", "1", "-i", "sine=frequency=800:sample_rate=48000",
		"-c:a", "pcm_s16le", voice)

	vids, recs, err := a.sessionTracks([]string{footage}, []string{mic})
	if err != nil {
		t.Fatal(err)
	}
	if len(vids) != 1 || len(recs) != 1 {
		t.Fatalf("the session came out as %d video(s) and %d recording(s)", len(vids), len(recs))
	}
	// the names carry the clock, so this is arithmetic and not a guess
	if vids[0].start != 4 || recs[0].start != 0 {
		t.Fatalf("the session was placed with the video at %g and the recording at %g, want 4 and 0",
			vids[0].start, recs[0].start)
	}

	c := prodClip{video: &vids[0], local: 0, sessS: 4, length: 8, tempo: 1,
		lines: []prodLine{{wav: voice, dur: 1, text: "hello", at: 0.5, delay: 0.5}}}
	c.mix = clipMixes(c, recs)
	if len(c.mix) != 1 || c.mix[0].ss != 4 || c.mix[0].at != 0 {
		t.Fatalf("the recording was placed under the clip as %+v, want it from 4 s of itself", c.mix)
	}

	st := defaultProdSettings()
	st.Preset, st.CRF, st.FPS, st.GameVol = "ultrafast", 32, 30, 0.5
	out := filepath.Join(dir, "clip.mp4")
	if err := a.encodeClip(c, out, "", st); err != nil {
		t.Fatal(err)
	}

	// the mix must not decide how long the clip is: the picture does
	if d, err := ffprobeDur(out); err != nil || d < 7.5 || d > 8.6 {
		t.Errorf("the clip came out %.2f s long (err %v), want its own 8", d, err)
	}

	// where the tone is, in the clip's own seconds. Only the high band is
	// listened to, so the game's own tone and the narration are not in the
	// answer -- what is left is the separate recording or nothing.
	quietBefore := highBand(t, out, 0, 2)
	tone := highBand(t, out, 2.1, 1.8)
	quietAfter := highBand(t, out, 4.1, 3.8)
	t.Logf("high band: %.1f dB before, %.1f dB during, %.1f dB after", quietBefore, tone, quietAfter)
	if tone < -40 {
		t.Errorf("nothing came through where the recording was talking (%.1f dB) — "+
			"the separate audio is not in the mix", tone)
	}
	if tone-quietBefore < 20 || tone-quietAfter < 20 {
		t.Errorf("the tone is not where it was recorded: %.1f dB before, %.1f dB during, "+
			"%.1f dB after — the mix is out of place", quietBefore, tone, quietAfter)
	}
}

var meanVolRe = regexp.MustCompile(`mean_volume:\s*(-?[\d.]+) dB`)

// highBand is how loud a stretch of a file is above 2 kHz, in dB. volumedetect
// reports nothing at all for a stretch that decoded to silence, which is the
// answer this test wants for "the tone is not here" -- so that comes back as a
// floor rather than as an error.
func highBand(t *testing.T, path string, ss, dur float64) float64 {
	t.Helper()
	out, err := exec.Command("ffmpeg", "-v", "info", "-ss", fmt.Sprintf("%.3f", ss),
		"-t", fmt.Sprintf("%.3f", dur), "-i", path,
		"-af", "highpass=f=2000,highpass=f=2000,volumedetect", "-f", "null", "-").CombinedOutput()
	if err != nil {
		t.Fatalf("measuring %s at %gs: %v\n%s", filepath.Base(path), ss, err, out)
	}
	m := meanVolRe.FindStringSubmatch(string(out))
	if m == nil {
		return -91 // digital silence: volumedetect had nothing to report
	}
	v, err := strconv.ParseFloat(m[1], 64)
	if err != nil {
		t.Fatalf("volumedetect said %q", m[1])
	}
	return v
}
