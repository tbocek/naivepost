package main

// One signal gets one lane.
//
// A microphone plugged into the left input of an interface and written out as a
// stereo file is the ordinary case, not the exotic one: the second channel is
// the first channel again, or the first channel and the encoder's noise. Drawing
// it twice costs a row of the page to say the same thing twice, and worse, it
// invites the reading that there are two sources here when there is one.
//
// So the decode decides how many lanes a recording draws, not the probe -- the
// probe can only count channels in a header, and whether the two are the same
// signal is a question only the samples answer.

import (
	"os/exec"
	"path/filepath"
	"testing"
)

// The four cases, each built as a real file and put through the real decode,
// because the whole question is about what comes out of ffmpeg and no hand-built
// buffer can be asked it.
func TestTwoChannelsCarryingOneSignalAreOneLane(t *testing.T) {
	dir := t.TempDir()
	// left and right identical: the same expression twice over
	same := "0.5*sin(2*PI*440*t)|0.5*sin(2*PI*440*t)"

	for _, c := range []struct {
		name  string
		build func(path string)
		chans int // what the file says it holds
		want  int // what it should be drawn as
		why   string
	}{
		{
			name:  "dual-mono.wav",
			build: func(p string) { mustLavfi(t, same, p) },
			chans: 2, want: 1,
			why: "two identical channels are one signal",
		},
		{
			name: "dual-mono.m4a",
			build: func(p string) {
				// the same thing after a lossy coder has been at it: the sides
				// come back close but not equal, which is why this is not a
				// sample-for-sample comparison
				mustLavfi(t, same, p)
			},
			chans: 2, want: 1,
			why: "a mono recording that went through an encoder is still one signal",
		},
		{
			name: "stereo.wav",
			build: func(p string) {
				// a tenth quieter on the right. Nothing dramatic -- a pair of
				// mics that are not quite matched -- and it has to survive
				mustLavfi(t, "0.5*sin(2*PI*440*t)|0.45*sin(2*PI*440*t)", p)
			},
			chans: 2, want: 2,
			why: "the sides differ and both have to be drawn",
		},
		{
			name: "phase.wav",
			build: func(p string) {
				// the same tone on both sides, one of them a third of a radian
				// late. The SAMPLES are a long way apart -- a tenth of full
				// scale, which is nothing like 40 dB down -- and the two
				// envelopes are the same number in every bucket, because the
				// loudest sample of a shifted sine is the loudest sample of the
				// sine. You could hear the difference and you could not see it,
				// and a lane is a picture.
				mustLavfi(t, "0.5*sin(2*PI*440*t)|0.5*sin(2*PI*440*t+0.3)", p)
			},
			chans: 2, want: 1,
			why: "the sides draw the same lane, whatever they do to a listener",
		},
		{
			name: "mono.wav",
			build: func(p string) {
				mustFFmpeg(t, "-f", "lavfi", "-i", "aevalsrc='0.5*sin(2*PI*440*t)':d=2:s=44100",
					"-ac", "1", "-c:a", "pcm_s16le", p)
			},
			chans: 1, want: 1,
			why: "a mono file has one channel to begin with",
		},
	} {
		path := filepath.Join(dir, c.name)
		c.build(path)
		if got := ffprobeChannels(path); got != c.chans {
			t.Errorf("%s probes as %d channel(s), want %d", c.name, got, c.chans)
			continue
		}
		wf, err := buildWave(path, 0, c.chans)
		if err != nil {
			t.Errorf("%s: %v", c.name, err)
			continue
		}
		if len(wf.chans) != c.want {
			t.Errorf("%s came out on %d lane(s), want %d — %s", c.name, len(wf.chans), c.want, c.why)
		}
		// whichever way it went, the lane that is kept is still the recording:
		// a collapse that dropped the wrong half, or emptied it, would pass a
		// count check and draw a flat line
		if p := wf.peak(0, 0.5, 1.5); p < 0.3 {
			t.Errorf("%s: the tone reads as %g on the first lane, want most of the way up", c.name, p)
		}
	}
}

// A silent stereo file collapses as well -- nothing is nothing twice over -- and
// it is worth pinning because it is the one case where the rule fires without
// any signal to compare, and the answer still has to be one lane rather than a
// division by a peak that is zero.
func TestASilentStereoRecordingIsOneLaneToo(t *testing.T) {
	path := filepath.Join(t.TempDir(), "quiet.wav")
	mustLavfi(t, "0|0", path)
	wf, err := buildWave(path, 0, 2)
	if err != nil {
		t.Fatal(err)
	}
	if len(wf.chans) != 1 {
		t.Errorf("silence was drawn on %d lanes, want 1", len(wf.chans))
	}
}

// And the page follows the decode: the lane count, the room the area asks for,
// and what the lane calls itself all come from the envelope once it has landed,
// and from the probe only until then.
func TestThePageGivesBackTheLaneTheDecodeDropped(t *testing.T) {
	ed := newTestEd(t)
	ed.auds = []tlAudio{{base: "mic", chans: 2}, {base: "room", chans: 2}}

	// before any envelope arrives, the probe is all there is: two stereo
	// recordings, four lanes
	if got := ed.audioLanes(); got != 4 {
		t.Fatalf("two stereo recordings ask for %d lanes before decoding, want 4", got)
	}
	tall := ed.audioHeight()

	// mic comes back as one signal, room as two
	ed.waves = map[string]*waveform{
		"mic":  {hz: waveHz, chans: [][]uint8{{1, 2, 3}}},
		"room": {hz: waveHz, chans: [][]uint8{{1}, {2}}},
	}
	if got := ed.lanes(ed.auds[0]); got != 1 {
		t.Errorf("the collapsed recording still draws %d lanes", got)
	}
	if got := ed.lanes(ed.auds[1]); got != 2 {
		t.Errorf("the stereo recording draws %d lanes, want 2", got)
	}
	if got := ed.audioLanes(); got != 3 {
		t.Fatalf("after decoding the page asks for %d lanes, want 3", got)
	}
	if got := ed.audioHeight(); got != tall-int(waveLaneH) {
		t.Errorf("the area is %d px tall, want %d — the dropped lane's room was not given back",
			got, tall-int(waveLaneH))
	}
	// and the label says which of the two it is, since one lane out of a stereo
	// file and one lane out of a mono file are not the same thing
	if got := laneName(0, ed.lanes(ed.auds[0]), ed.auds[0].chans); got != "L=R" {
		t.Errorf("the collapsed lane is called %q, want L=R", got)
	}
}

// mustLavfi writes expr -- one aevalsrc expression per channel -- as two seconds
// of audio, in whatever format the extension asks for.
func mustLavfi(t *testing.T, expr, path string) {
	t.Helper()
	args := []string{"-v", "error", "-y", "-f", "lavfi",
		"-i", "aevalsrc='" + expr + "':d=2:s=44100", path}
	if out, err := exec.Command("ffmpeg", args...).CombinedOutput(); err != nil {
		t.Fatalf("ffmpeg %v: %v\n%s", args, err, out)
	}
}

// ---- the picture test on its own ---------------------------------------------

// sameLanes decides the loosened half of the rule, and it is worth asking
// directly: a mean under a byte collapses, a mean over it does not, and one
// bucket far enough out on its own keeps both lanes however quiet the rest of
// the recording is about it.
func TestTwoEnvelopesThatDrawTheSameLaneAreOne(t *testing.T) {
	// a hundred buckets of something, and the same hundred back
	base := make([]uint8, 100)
	for i := range base {
		base[i] = uint8(40 + i)
	}
	same := append([]uint8(nil), base...)
	if !sameLanes(base, same) {
		t.Error("two copies of one envelope were called different")
	}

	// a byte here and there, in both directions: a lossy coder's leavings
	noisy := append([]uint8(nil), base...)
	for i := range noisy {
		if i%3 == 0 {
			noisy[i]++
		} else if i%7 == 0 {
			noisy[i]--
		}
	}
	if !sameLanes(base, noisy) {
		t.Error("a byte of noise scattered through the envelope was called a stereo image — " +
			"a bucket IS a byte, and two envelopes that agree to one are the same picture")
	}

	// a level difference: a pair of mics that are not quite matched
	quieter := append([]uint8(nil), base...)
	for i := range quieter {
		quieter[i] -= 13
	}
	if sameLanes(base, quieter) {
		t.Error("a thirteen-byte level difference was collapsed into one lane")
	}

	// quiet about it everywhere except once, which is a pan and is exactly the
	// thing a mean over an hour would swallow
	panned := append([]uint8(nil), base...)
	panned[50] += laneSameMax + 1
	if sameLanes(base, panned) {
		t.Errorf("one bucket %d bytes out was averaged away — that is something "+
			"panning, and it is the whole reason there are two lanes", laneSameMax+1)
	}

	// and nothing to compare is not a match
	if sameLanes(nil, nil) || sameLanes(base, base[:10]) {
		t.Error("an empty or ragged pair was called the same")
	}
}
