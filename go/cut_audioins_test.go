package main

// An insert that is sound alone: an audio file placed on the cut, over a
// selection or spliced in at the playhead, exactly the two ways a video insert
// goes in. What is different about it is WHERE it lives on the page and in the
// render -- the picture stays the session's own, so the marker is drawn in the
// audio lanes and not the picture band, and the renderer keeps the footage's
// frames (or holds one, spliced) while the file replaces the session's sound.
// These tests hold the kind, the geography, the preview, and the render's
// routing.

import (
	"fmt"
	"math"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"runtime"
	"strconv"
	"strings"
	"testing"

	"github.com/diamondburned/gotk4/pkg/cairo"
)

// renderLanes rasterizes the audio lanes the way renderTrack does the picture
// band, so a test can ask what colour the lanes actually are at a point.
func renderLanes(t *testing.T, ed *cutEditor, w, h int) func(x, y int) (r, g, b uint8) {
	t.Helper()
	surf := cairo.CreateImageSurface(cairo.FormatARGB32, w, h)
	cr := cairo.Create(surf)
	ed.drawAudio(cr, w, h)
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

// TestASoundFileIsItsOwnKind: the extensions the insert dialog offers come out
// of insKind as "audio", and audioIns answers yes to exactly the segments that
// carry one -- not to cards, not to copies, not to footage.
func TestASoundFileIsItsOwnKind(t *testing.T) {
	for file, want := range map[string]string{
		"sting.mp3": "audio", "voice.wav": "audio", "bed.flac": "audio",
		"note.opus": "audio", "aac.m4a": "audio",
		"clip.mp4": "video", "card.svg": "svg", "logo.png": "still",
	} {
		if got := insKind(file); got != want {
			t.Errorf("insKind(%q) = %q, want %q", file, got, want)
		}
	}
	for _, c := range []struct {
		s    cutSeg
		want bool
	}{
		{cutSeg{S: 20, E: 20, Ins: "sting.mp3", Dur: 5}, true},   // spliced
		{cutSeg{S: 30, E: 40, Ins: "sting.wav"}, true},           // over a selection
		{cutSeg{S: 20, E: 20, Ins: "card.svg", Dur: 5}, false},   // a card
		{cutSeg{S: 20, E: 20, Ins: "copy:5.000", Dur: 4}, false}, // a copy
		{cutSeg{S: 0, E: 60}, false},                             // footage
	} {
		if got := c.s.audioIns(); got != c.want {
			t.Errorf("audioIns(%+v) = %v, want %v", c.s, got, c.want)
		}
	}
}

// TestTheSoundMarksTheLanesAndNotThePicture: the user's sentence, as pixels.
// The picture band shows no violet for either shape of audio insert -- an
// overwrite keeps its green, because its picture IS kept -- while the lanes
// wear the violet, and the hatching when the file is spliced in.
func TestTheSoundMarksTheLanesAndNotThePicture(t *testing.T) {
	ed := moveEd(t)
	ed.auds = []tlAudio{{base: "cam", path: "c.wav", start: 0, dur: 300, chans: 1}}
	ed.segs = []cutSeg{
		{S: 0, E: 60},
		{S: 20, E: 20, Ins: "sting.mp3", Dur: 5}, // spliced at 20
		{S: 30, E: 40, Ins: "bed.wav"},           // over a selection
	}
	const w, h = 400, 200
	// blue past red AND green: the kept-footage tint is bluish enough at its
	// edges to pass a red-only test
	violet := func(r, g, b uint8) bool { return int(b) > int(r)+20 && b > g && b > 60 }

	at := renderTrack(t, ed, w, h)
	stray, green := 0, 0
	for y := int(ed.picTop()) + 2; y < int(ed.picTop())+ed.thumbHt; y++ {
		for x := int(ed.xOf(18)); x < int(ed.xOf(42)); x++ {
			r, g, b := at(x, y)
			if violet(r, g, b) {
				stray++
			}
			if x > int(ed.xOf(31)) && x < int(ed.xOf(39)) && g > r+30 && g > b+30 {
				green++
			}
		}
	}
	if stray > 0 {
		t.Errorf("the picture band shows %d violet px over the audio inserts — sound belongs in the lanes", stray)
	}
	if green == 0 {
		t.Error("no green under the overwriting sound — its picture is kept footage and the tint has to say so")
	}

	lanes := renderLanes(t, ed, w, h)
	splice, hatch, over := 0, 0, 0
	for y := 2; y < h-2; y++ {
		for x := int(ed.xOf(20)) - int(splicePx/2) + 2; x < int(ed.xOf(20))+int(splicePx/2)-2; x++ {
			r, g, b := lanes(x, y)
			if violet(r, g, b) {
				splice++
			}
			if r > 90 && g > 70 && b < 90 {
				hatch++ // a yellow hatch stroke: the footage stops here
			}
		}
		for x := int(ed.xOf(32)); x < int(ed.xOf(38)); x++ {
			if violet(lanes(x, y)) {
				over++
			}
		}
	}
	if splice == 0 || over == 0 {
		t.Errorf("the lanes show violet at %d/%d px for splice/overwrite — the marker lives here", splice, over)
	}
	if hatch == 0 {
		t.Error("the spliced sound has no hatching in the lanes — nothing says the footage stops for it")
	}
}

// TestThePreviewPlaysTheFileAndKeepsThePicture: cardVoice hands the preview the
// sound file itself, resolved from the project root the way every insert path
// is, and a still keeps returning nothing.
func TestThePreviewPlaysTheFileAndKeepsThePicture(t *testing.T) {
	a := &App{root: t.TempDir()}
	ed := &cutEditor{a: a}
	a.ed = ed
	ed.hold.on = true // something is playing; sound follows the transport

	s := cutSeg{S: 40, E: 40, Ins: "sting.mp3", Dur: 5}
	if got := ed.cardVoice(&s); got != a.loadPath("sting.mp3") {
		t.Errorf("an audio insert is heard from %q, want the file itself", got)
	}
	still := cutSeg{S: 40, E: 40, Ins: "logo.png", Dur: 5}
	if got := ed.cardVoice(&still); got != "" {
		t.Errorf("a still is heard from %q, want silence — it has no sound to play", got)
	}
}

// TestTheRenderReplacesTheSessionSound: a clip whose sound is an inserted file
// gets none of the separate recordings mixed under it -- the file replaces
// everything that was there, exactly as a video insert's own sound does.
func TestTheRenderReplacesTheSessionSound(t *testing.T) {
	v := tlVideo{base: "v", path: "v.mkv", start: 0, dur: 100}
	recs := []tlAudio{{base: "mic", path: "mic.wav", start: 0, dur: 100}}
	c := prodClip{video: &v, local: 10, sessS: 10, length: 5, rate: 1, tempo: 1}
	if len(clipMixes(c, recs)) != 1 {
		t.Fatal("plain footage under a running recording gets no mix — the fixture is wrong")
	}
	c.snd, c.noLanes = "sting.mp3", true
	if got := clipMixes(c, recs); got != nil {
		t.Errorf("an audio-insert clip still mixes %v under the file — overwriting is replacing", got)
	}
}

// TestTheAudioInsertIsWired pins the seams across the four files: the kind, the
// dialog that offers the files, the two drawing sites that move the marker into
// the lanes, the preview branch that keeps the picture, and the render's snd
// routing -- the file as input 1 where the silence would go, padded to the
// slot, with the output -t pinning the length.
func TestTheAudioInsertIsWired(t *testing.T) {
	pins := map[string][]string{
		"cut.go": {
			"func (s cutSeg) audioIns() bool",
			`var audExts = []string{"mp3", "wav", "ogg", "oga", "flac", "m4a", "aac", "opus"}`,
			`case "video", "audio":`,                               // a sound file knows its own length
			"if s.isInsert() && !(s.audioIns() && !s.spliced()) {", // the overwrite keeps its green
			"if !s.isInsert() || s.audioIns() {",                   // no violet in the picture band
			"if s := ed.heldSeg(); s != nil && !s.audioIns() {",    // the held outline follows the marker
		},
		"cut_audio.go": {
			"if !s.audioIns() {", // the lanes draw exactly these
			// one painter for the band and the rows' paired strips
			"func (ed *cutEditor) sndInsMark(",
			"hatchStrokes(cr, x0, x1-x0, y, h)",
			`markPlate(cr, tx, y+h-6, "sound", fmt.Sprintf("%s  %.1fs", insName(s), s.Dur))`,
		},
		"cut_insview.go": {
			"if s == nil || s.audioIns() {", // the preview keeps the session's picture
			`k != "video" && k != "audio"`,  // and plays the file
		},
		"produce.go": {
			`case ".mp3", ".wav", ".ogg", ".oga", ".flac", ".m4a", ".aac", ".opus":`,
			"case s.audioIns():",
			"length: s.length(), freeze: s.spliced(), snd: path, sndAt: s.Ss,", // spliced holds the frame
			`case c.snd != "" && c.dropLane == "":`,                            // the file where the silence would go
			"apad[snd];",                                                       // a short file padded out to the slot
			"if c.freeze || c.noLanes {",                                       // no session mixes under it (laneOverlap)
			`if c.ins != "" || c.snd != "" || c.freeze || c.speed() != 1 {`,    // -t makes the length exact
		},
	}
	for file, wants := range pins {
		b, err := os.ReadFile(file)
		if err != nil {
			t.Fatal(err)
		}
		for _, want := range wants {
			if !strings.Contains(string(b), want) {
				t.Errorf("%s does not contain %q", file, want)
			}
		}
	}
}

// TestTheRenderPlaysTheFileInsteadOfTheRecording, end to end through encodeClip
// and settled by the only witness that can: the footage carries a 200 Hz tone
// and the inserted file a 2000 Hz one, and the finished clip has to sound like
// the file with the recording's tone gone. Both shapes are rendered -- an
// overwrite whose file is LONGER than the slot (trimmed by the input -t and the
// output -t) and a spliced one whose file is SHORTER (apad carries the track to
// the slot, or every clip after this one comes out of step with its picture).
func TestTheRenderPlaysTheFileInsteadOfTheRecording(t *testing.T) {
	a := insertApp(t)
	dir := t.TempDir()
	footage := filepath.Join(dir, "footage.mp4")
	mustFFmpeg(t, "-f", "lavfi", "-t", "4", "-i", "testsrc=size=1280x720:rate=30",
		"-f", "lavfi", "-t", "4", "-i", "sine=frequency=200:sample_rate=48000",
		"-c:v", "libx264", "-preset", "ultrafast", "-pix_fmt", "yuv420p", "-c:a", "aac", footage)
	long := filepath.Join(dir, "long.wav")
	mustFFmpeg(t, "-f", "lavfi", "-t", "5", "-i", "sine=frequency=2000:sample_rate=48000", long)
	short := filepath.Join(dir, "short.wav")
	mustFFmpeg(t, "-f", "lavfi", "-t", "1", "-i", "sine=frequency=2000:sample_rate=48000", short)

	// how loud one tone is in a file: two band passes back to back, because one
	// biquad lets enough of a tone a decade away through to blur the verdict
	band := func(file string, hz int) float64 {
		filter := fmt.Sprintf("bandpass=f=%d:width_type=h:w=200,bandpass=f=%d:width_type=h:w=200,volumedetect", hz, hz)
		out, err := exec.Command("ffmpeg", "-v", "info", "-i", file,
			"-af", filter, "-f", "null", "-").CombinedOutput()
		if err != nil {
			t.Fatalf("volumedetect: %v\n%s", err, out)
		}
		m := regexp.MustCompile(`mean_volume: (-?[0-9.]+) dB`).FindStringSubmatch(string(out))
		if m == nil {
			t.Fatalf("no mean_volume in\n%s", out)
		}
		v, _ := strconv.ParseFloat(m[1], 64)
		return v
	}

	st := prodSettings{
		Container: "mp4", Codec: "h264", CRF: 30, Preset: "ultrafast",
		Height: 360, FPS: 24, AudioKbps: 96, GameVol: 0.22, Subs: "none",
	}
	v := &tlVideo{base: "footage", path: footage}
	for _, c := range []struct {
		name string
		clip prodClip
		// how far under the file's own level the clip may sit: the sine is
		// nowhere near full scale, so the yardstick is the file, not 0 dB --
		// and a 1 s file in a 2 s slot averages 6 dB down over the padding
		drop float64
	}{
		{"over", prodClip{idx: 0, video: v, local: 0, length: 2, rate: 1, tempo: 1, snd: long}, 3},
		{"spliced", prodClip{idx: 1, video: v, local: 1, length: 2, rate: 1, tempo: 1, freeze: true, snd: short}, 9},
	} {
		out := filepath.Join(dir, c.name+".mp4")
		if err := a.encodeClip(c.clip, out, "", st); err != nil {
			t.Fatalf("%s: %v", c.name, err)
		}
		d, err := ffprobeDur(out)
		if err != nil {
			t.Fatalf("%s produced nothing readable: %v", c.name, err)
		}
		if math.Abs(d-2) > 0.15 {
			t.Errorf("%s runs %.2f s, want 2 — the file's own length leaked into the slot", c.name, d)
		}
		ref := band(c.clip.snd, 2000)
		ins, rec := band(out, 2000), band(out, 200)
		if ins < ref-c.drop {
			t.Errorf("%s: the inserted sound reads %.1f dB against the file's own %.1f — the file is not in the clip",
				c.name, ins, ref)
		}
		if rec > ins-20 {
			t.Errorf("%s: the recording's tone reads %.1f dB against the file's %.1f — the session sound leaked through",
				c.name, rec, ins)
		}
	}
}
