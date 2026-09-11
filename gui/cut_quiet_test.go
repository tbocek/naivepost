package main

// Which lanes a scene hears.
//
// The choice used to be one answer for the whole cut -- ed.snd, walked with the
// arrow keys -- so a session with a mic on the table and the game's own sound
// could have one of them, everywhere, and the scene where somebody actually
// says something got the same answer as the scene where nobody does. What is
// pinned here is the per-scene version of that question: that a scene nobody
// has touched still hears everything, that silencing a separate recording takes
// it out of the mix and nothing else with it, that silencing a camera takes its
// own track off the same way, and that the sum of what is left cannot arrive at
// the encoder above 0 dBFS.

import (
	"encoding/json"
	"os/exec"
	"path/filepath"
	"reflect"
	"regexp"
	"strconv"
	"strings"
	"testing"
)

// ---- the scene ---------------------------------------------------------------

// The empty list has to mean "everything plays", because that is what every cut
// written before this says and what every scene nobody has touched says. The
// list holds the SILENT lanes for exactly that reason -- the other way round,
// an untouched scene would be a scene with no sound at all.
func TestASceneNobodyHasTouchedHearsEveryLane(t *testing.T) {
	var s cutSeg
	for _, lane := range []string{"mic", "room", "game", ""} {
		if !s.hears(lane) {
			t.Errorf("an untouched scene does not hear %q", lane)
		}
	}

	// and it writes nothing, so an ordinary cut.json is unchanged by any of this
	b, err := json.Marshal(cutSeg{S: 10, E: 20})
	if err != nil {
		t.Fatal(err)
	}
	if strings.Contains(string(b), "quiet") {
		t.Errorf("a scene with no silenced lane still wrote a key: %s", b)
	}

	// the reading back is the other half: an old file has no key at all
	var back cutSeg
	if err := json.Unmarshal([]byte(`{"s":10,"e":20}`), &back); err != nil {
		t.Fatal(err)
	}
	if !back.hears("mic") || len(back.Quiet) != 0 {
		t.Errorf("a cut written before this came back with %v silenced", back.Quiet)
	}

	// named, it is heard as named -- and only it
	held := cutSeg{S: 10, E: 20, Quiet: []string{"mic"}}
	if held.hears("mic") || !held.hears("room") {
		t.Errorf("silencing mic gave hears(mic)=%v hears(room)=%v",
			held.hears("mic"), held.hears("room"))
	}
}

// sameSeg replaced == when the scene grew a slice, and a hand-written field
// list is a thing that gets forgotten. This walks the type instead: every field
// gets a value it did not have, and every one of them has to read as a change.
// A field added later and left out of sameSeg fails here rather than quietly
// making Revert stop noticing it.
func TestEverySegmentFieldCountsAsAChange(t *testing.T) {
	base := cutSeg{}
	rt := reflect.TypeOf(base)
	for i := 0; i < rt.NumField(); i++ {
		f := rt.Field(i)
		other := base
		v := reflect.ValueOf(&other).Elem().Field(i)
		switch f.Type.Kind() {
		case reflect.Float64:
			v.SetFloat(7)
		case reflect.Int:
			v.SetInt(7)
		case reflect.String:
			v.SetString("x")
		case reflect.Bool:
			v.SetBool(true)
		case reflect.Slice:
			v.Set(reflect.ValueOf([]string{"x"}))
		default:
			t.Fatalf("%s is a %s and this test does not know how to change one",
				f.Name, f.Type.Kind())
		}
		if f.Tag.Get("json") == "-" {
			// a field the cut does not store is not part of what the cut IS:
			// Scene is stamped by the render's own planning, on a copy, and
			// two scenes that differ only by it are the same scene
			continue
		}
		if sameSeg(base, other) {
			t.Errorf("changing %s left the two scenes reading as the same one — "+
				"sameSeg does not look at it, so Revert will not notice it", f.Name)
		}
	}
}

// Which lane the hand silenced FIRST is not part of what the cut sounds like.
// Compared as lists, pressing two toggles in the other order would light Revert
// up over a change that is not one.
func TestTheSilencedLanesAreASetAndNotAnOrder(t *testing.T) {
	a := cutSeg{S: 1, E: 2, Quiet: []string{"mic", "room"}}
	b := cutSeg{S: 1, E: 2, Quiet: []string{"room", "mic"}}
	if !sameSeg(a, b) {
		t.Errorf("the same two lanes in the other order read as a different cut")
	}
	if sameSeg(a, cutSeg{S: 1, E: 2, Quiet: []string{"mic"}}) {
		t.Errorf("one silenced lane reads the same as two")
	}
	// and two of them is not two of any other two: same count, other lanes
	if sameSeg(a, cutSeg{S: 1, E: 2, Quiet: []string{"mic", "head"}}) {
		t.Errorf("silencing mic+room reads the same as silencing mic+head")
	}
	// and sameCut is the caller that matters -- Revert asks through it
	if sameCut([]cutSeg{a}, []cutSeg{{S: 1, E: 2}}) {
		t.Errorf("a scene with two lanes silenced reads as an untouched one")
	}
}

// ---- the render --------------------------------------------------------------

// A silenced separate recording is one input fewer, and nothing else moves: the
// others keep the stretch and the placement they had, because taking the mic
// out of a scene is not meant to change where the room sound sits under it.
func TestASilencedLaneIsTheOnlyOneMissingFromTheMix(t *testing.T) {
	_, recs := laneEd()
	recs = append(recs, tlAudio{base: "head", path: "/x/head.wav", start: 0, dur: 100})
	v := tlVideo{base: "game", path: "/x/game.mp4", start: 0, dur: 100}
	clip := func(quiet ...string) prodClip {
		return prodClip{video: &v, local: 10, sessS: 10, length: 5, tempo: 1, rate: 1, quiet: quiet}
	}

	all := clipMixes(clip(), recs)
	if len(all) != 3 {
		t.Fatalf("an untouched scene got %d recording(s), want all 3: %+v", len(all), all)
	}
	got := clipMixes(clip("room"), recs)
	if len(got) != 2 || got[0].base != "mic" || got[1].base != "head" {
		t.Fatalf("silencing room left %+v, want mic and head", got)
	}
	// the survivors are untouched, down to the second they play from
	was := map[string]prodMix{}
	for _, m := range all {
		was[m.base] = m
	}
	for _, m := range got {
		if m != was[m.base] {
			t.Errorf("%s came out as %+v, want %+v — silencing a lane moved another one",
				m.base, m, was[m.base])
		}
	}
	// and all of them named is a scene with no separate recording under it
	if m := clipMixes(clip("mic", "room", "head"), recs); len(m) != 0 {
		t.Errorf("every lane silenced still played %+v", m)
	}
	// a name nobody has takes nothing out, the way dropLane already behaves
	if m := clipMixes(clip("gone"), recs); len(m) != 3 {
		t.Errorf("naming a lane the session has not got dropped one: %d left", len(m))
	}
}

// The camera's own track is the other kind of lane and the same question, so it
// is silenced by the same list -- and by the name the page draws it under,
// which masterLanes takes straight off the video. The render already had the
// path for it: a clip with no sound of its own gets anullsrc, the way a muted
// paste and a held frame do.
func TestSilencingACameraTakesItsOwnTrackOff(t *testing.T) {
	if _, err := exec.LookPath("ffmpeg"); err != nil {
		t.Skip("no ffmpeg on this machine")
	}
	dir := t.TempDir()
	loud := filepath.Join(dir, "game.mp4")
	mustFFmpeg(t, "-f", "lavfi", "-t", "3", "-i", "testsrc=size=64x64:rate=15",
		"-f", "lavfi", "-t", "3", "-i", "sine=frequency=300:sample_rate=48000",
		"-shortest", loud)

	a := &App{}
	st := defaultProdSettings()
	v := tlVideo{base: "game", path: loud, start: 0, dur: 3}
	clip := func(quiet ...string) prodClip {
		return prodClip{video: &v, local: 0, length: 2, tempo: 1, rate: 1, quiet: quiet}
	}

	if _, snd, err := a.clipInput(clip(), st); err != nil || !snd {
		t.Fatalf("an untouched camera came out silent (sound=%v, err=%v)", snd, err)
	}
	if _, snd, err := a.clipInput(clip("game"), st); err != nil || snd {
		t.Errorf("silencing the camera left its own track on (sound=%v, err=%v)", snd, err)
	}
	// its lane and a recording's are one namespace, so silencing a recording
	// must not reach the picture's sound as well
	if _, snd, err := a.clipInput(clip("mic"), st); err != nil || !snd {
		t.Errorf("silencing mic took game's own track off too (sound=%v, err=%v)", snd, err)
	}
}

// The scene carries the answer and the clip has to be handed it, for every kind
// of clip there is -- a copy, an insert and plain footage all sit under the
// same lanes. This is the one line that does it; the alternative is four.
func TestTheSilencedLanesRideFromTheSceneToTheClip(t *testing.T) {
	src := readSrc(t, "produce.go")
	if !strings.Contains(src, "\t\tc.quiet = s.Quiet") {
		t.Errorf("the clip is never told which lanes its scene silenced")
	}
	// and after the switch that builds it, not inside one branch of it
	if i, j := strings.Index(src, "c.quiet = s.Quiet"), strings.Index(src, "if from, ok := copySrc(s.Ins)"); i < 0 || i > j {
		t.Errorf("the hand-off sits at %d and the copy branch at %d — it must come first, "+
			"where every clip kind still passes through", i, j)
	}
}

// ---- the headroom --------------------------------------------------------------

var maxVolRe = regexp.MustCompile(`max_volume:\s*(-?[\d.]+) dB`)

// peakVol is the loudest sample in a file, in dB. Unlike highBand's mean this
// is the number clipping is a claim about, and volumedetect reports nothing for
// a stretch that decoded to silence -- which comes back as a floor.
func peakVol(t *testing.T, path string) float64 {
	t.Helper()
	out, err := exec.Command("ffmpeg", "-v", "info", "-i", path,
		"-af", "volumedetect", "-f", "null", "-").CombinedOutput()
	if err != nil {
		t.Fatalf("measuring %s: %v\n%s", filepath.Base(path), err, out)
	}
	m := maxVolRe.FindStringSubmatch(string(out))
	if m == nil {
		return -91
	}
	v, err := strconv.ParseFloat(m[1], 64)
	if err != nil {
		t.Fatalf("volumedetect said %q", m[1])
	}
	return v
}

// Two lanes at their own level ARE louder than one -- that is the whole point
// of keeping them at their own level, and the reason a hand on a desk does it
// that way: switching a second mic on must not duck the first. What it must
// also not do is arrive at the encoder above full scale, because every clip is
// encoded on its own and the loudnorm pass that sets the finished video's level
// runs over the joined file, long after any clipping is baked in.
//
// So: a loud capture and an equally loud recording under it, which sum to well
// past 0 dBFS, and the clip they produce is asked how loud its loudest sample
// is. This is a thing only ffmpeg can settle.
func TestTwoLanesAtOnceCannotReachTheEncoderClipping(t *testing.T) {
	a := insertApp(t)
	dir := t.TempDir()

	// the capture: a tone at very nearly full scale, which on its own already
	// leaves no room. lavfi's sine comes out around -18 dBFS, so it is lifted
	// here rather than assumed loud -- the whole test is about what happens
	// when there is nothing left to add.
	footage := filepath.Join(dir, "game.mp4")
	mustFFmpeg(t, "-f", "lavfi", "-t", "4", "-i", "testsrc=size=160x120:rate=30",
		"-f", "lavfi", "-t", "4", "-i", "sine=frequency=300:sample_rate=48000",
		"-af", "volume=7.9", "-c:v", "libx264", "-preset", "ultrafast", "-pix_fmt", "yuv420p",
		"-c:a", "pcm_s16le", "-shortest", footage)
	// and a recording of the same moment, just as loud, at another pitch so the
	// two do not simply cancel or double a single frequency
	mic := filepath.Join(dir, "mic.wav")
	mustFFmpeg(t, "-f", "lavfi", "-t", "4", "-i", "sine=frequency=700:sample_rate=48000",
		"-af", "volume=7.9", "-c:a", "pcm_s16le", mic)

	v := tlVideo{base: "game", path: footage, start: 0, dur: 4}
	recs := []tlAudio{{base: "mic", path: mic, start: 0, dur: 4}}
	st := defaultProdSettings()
	// 320 kbps is not what anyone ships; it is what makes the answer readable.
	// At the default rate AAC's own error is half a dB, which is the whole gap
	// between a limited peak and a clipped one -- so the encoder would be the
	// thing under test. At 320 the two read as -1.0 and 0.0, exactly.
	st.Preset, st.CRF, st.FPS, st.AudioKbps = "ultrafast", 32, 30, 320

	c := prodClip{video: &v, local: 0, sessS: 0, length: 3, tempo: 1, rate: 1}
	c.mix = clipMixes(c, recs)
	if len(c.mix) != 1 {
		t.Fatalf("the recording did not go under the clip: %+v", c.mix)
	}
	out := filepath.Join(dir, "both.mp4")
	if err := a.encodeClip(c, out, "", st); err != nil {
		t.Fatal(err)
	}
	both := peakVol(t, out)
	t.Logf("two lanes at once peak at %.1f dB", both)
	if both > -0.5 {
		t.Errorf("two lanes summed to %.1f dB — that is at or over full scale, and the "+
			"clip is encoded here, before the loudnorm pass could undo it", both)
	}

	// The graph has two endings and the narration one is the other. It ducks
	// the bed under the voice (GameVol), which normally leaves room -- so this
	// asks with the duck wide open, which is a setting the page allows and a
	// scene with a quiet game wants. Voice plus bed is still a sum, and it is
	// encoded on this same pass.
	open := st
	open.GameVol = 1
	nar := prodClip{video: &v, local: 0, sessS: 0, length: 3, tempo: 1, rate: 1,
		lines: []prodLine{{wav: mic, dur: 3, text: "hello", at: 0, delay: 0}}}
	narOut := filepath.Join(dir, "narrated.mp4")
	if err := a.encodeClip(nar, narOut, "", open); err != nil {
		t.Fatal(err)
	}
	spoken := peakVol(t, narOut)
	t.Logf("a voice over an unducked game peaks at %.1f dB", spoken)
	if spoken > -0.5 {
		t.Errorf("the narration ending of the graph summed to %.1f dB — the two endings "+
			"must both be caught, and only one of them is", spoken)
	}

	// The other half: below the ceiling the limiter has to be a pass-through.
	// A per-clip leveller in this slot -- dynaudnorm, loudnorm -- would also
	// stop the clipping, and would also make every scene as loud as every other
	// one, which hands the loudnorm pass at the end a file whose moments no
	// longer agree with each other. That fault is silent, so it is pinned here.
	quiet := filepath.Join(dir, "quiet.wav")
	mustFFmpeg(t, "-f", "lavfi", "-t", "4", "-i", "sine=frequency=700:sample_rate=48000",
		"-af", "volume=7.9,volume=-24dB", "-c:a", "pcm_s16le", quiet)
	softV := tlVideo{base: "soft", path: footage, start: 0, dur: 4}
	soft := prodClip{video: &softV, local: 0, sessS: 0, length: 3, tempo: 1, rate: 1,
		quiet: []string{"soft"}} // the capture's loud tone off, the quiet lane alone
	soft.mix = clipMixes(soft, []tlAudio{{base: "q", path: quiet, start: 0, dur: 4}})
	softOut := filepath.Join(dir, "soft.mp4")
	if err := a.encodeClip(soft, softOut, "", st); err != nil {
		t.Fatal(err)
	}
	got := peakVol(t, softOut)
	t.Logf("a quiet lane alone peaks at %.1f dB", got)
	if got > -18 {
		t.Errorf("a -24 dB lane came out peaking at %.1f dB — the limiter is levelling, "+
			"not just catching peaks, so every scene will arrive equally loud", got)
	}
}
