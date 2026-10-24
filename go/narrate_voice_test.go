package main

// The voice picker has failure modes a mouse would not catch quickly: a voice
// that does not survive being written to the project folder, a pitch slider
// that moves nothing or moves the wrong file, and a synthesis cache that
// ignores either -- which would serve the previous speaker after a switch, or
// throw away every line already spoken. All are checked here; the GUI wiring
// around them is not testable without a display.

import (
	"bytes"
	"encoding/binary"
	"math"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"sort"
	"strconv"
	"strings"
	"testing"
)

// sineWav writes a pure tone, which toneHz can read back exactly -- the point
// of the pitch tests is the ratio, not how speech survives rubberband.
func sineWav(t *testing.T, hz int) string {
	t.Helper()
	p := filepath.Join(t.TempDir(), "cv-xx-female-30s.wav")
	if err := exec.Command("ffmpeg", "-v", "error", "-y", "-f", "lavfi",
		"-i", "sine=frequency="+strconv.Itoa(hz)+":duration=2", p).Run(); err != nil {
		t.Skipf("no usable ffmpeg: %v", err)
	}
	return p
}

// toneHz counts zero crossings over the middle second, away from the edges
// where a phase vocoder has nothing to work with yet.
func toneHz(t *testing.T, wav string) float64 {
	t.Helper()
	const sr = 16000
	raw, err := exec.Command("ffmpeg", "-v", "error", "-ss", "0.5", "-t", "1", "-i", wav,
		"-f", "s16le", "-ac", "1", "-ar", "16000", "-").Output()
	if err != nil {
		t.Fatalf("decode %s: %v", wav, err)
	}
	n, cross, prev := len(raw)/2, 0, int16(0)
	if n < sr/2 {
		t.Fatalf("%s decoded to %d samples -- too short to measure", wav, n)
	}
	for i := 0; i < n; i++ {
		s := int16(uint16(raw[2*i]) | uint16(raw[2*i+1])<<8)
		if (s >= 0) != (prev >= 0) && i > 0 {
			cross++
		}
		prev = s
	}
	return float64(cross) / 2 * sr / float64(n)
}

func TestVoiceRoundTrip(t *testing.T) {
	a := &App{outDir: t.TempDir(), curCmds: map[*exec.Cmd]bool{}}
	if got := a.voiceID(); got != ownVoice {
		t.Fatalf("a fresh project speaks in %q, want %q", got, ownVoice)
	}

	// a real wav through the real conversion path; ffmpeg is a hard dependency
	// of the pipeline anyway
	src := sineWav(t, 220)
	v := voiceOpt{id: "cv-xx-female-30s", name: "cv-xx-female-30s.wav", path: src}
	if err := a.setVoice(v); err != nil {
		t.Fatalf("setVoice: %v", err)
	}
	if !exists(a.refBase()) {
		t.Fatal("choosing a voice installed no base recording to clone")
	}
	if err := a.ensureVoiceRef(); err != nil {
		t.Fatalf("ensureVoiceRef: %v", err)
	}
	if !exists(a.refPath()) {
		t.Fatal("no voice_ref.wav for the server to clone")
	}

	// a later session reads the choice back out of the folder
	if got := (&App{outDir: a.outDir}).voiceID(); got != v.id {
		t.Fatalf("reopened project speaks in %q, want %q", got, v.id)
	}

	// back to own: both stale files must go, or ensureVoiceRef short-circuits
	// on them and keeps cloning the CC0 speaker forever
	if err := a.setVoice(voiceOpt{id: ownVoice}); err != nil {
		t.Fatalf("setVoice(own): %v", err)
	}
	if exists(a.refPath()) || exists(a.refBase()) {
		t.Fatal("switching back to your own voice left the previous reference in place")
	}
	if got := (&App{outDir: a.outDir}).voiceID(); got != ownVoice {
		t.Fatalf("reopened project speaks in %q, want %q", got, ownVoice)
	}
}

// plainWavBytes is the smallest wav the server would take: a 44-byte plain PCM
// header at refRate and a little silence. Tests that only need "a reference is
// on disk" write this, because on disk is no longer enough -- the header is
// read, and four bytes of "RIFF" is a file that would be cut again.
func plainWavBytes(n int) []byte {
	b := &bytes.Buffer{}
	b.WriteString("RIFF")
	binary.Write(b, binary.LittleEndian, uint32(36+n))
	b.WriteString("WAVEfmt ")
	binary.Write(b, binary.LittleEndian, uint32(16))
	binary.Write(b, binary.LittleEndian, uint16(1))     // plain PCM, the tag that matters
	binary.Write(b, binary.LittleEndian, uint16(1))     // mono
	binary.Write(b, binary.LittleEndian, uint32(48000)) // refRate
	binary.Write(b, binary.LittleEndian, uint32(96000)) // bytes a second
	binary.Write(b, binary.LittleEndian, uint16(2))     // block align
	binary.Write(b, binary.LittleEndian, uint16(16))
	b.WriteString("data")
	binary.Write(b, binary.LittleEndian, uint32(n))
	b.Write(make([]byte, n))
	return b.Bytes()
}

// wavHdr is the fmt chunk, which is what a reader switches on: the tag says
// what the samples are, and the chunk's own length says which of the two wav
// headers this is.
type wavHdr struct{ tag, chans, bits, rate, fmtLen int }

func wavFmt(t *testing.T, p string) wavHdr {
	t.Helper()
	b, err := os.ReadFile(p)
	if err != nil {
		t.Fatal(err)
	}
	if len(b) < 12 || string(b[:4]) != "RIFF" || string(b[8:12]) != "WAVE" {
		t.Fatalf("%s is not a wav at all", p)
	}
	for i := 12; i+8 <= len(b); {
		n := int(binary.LittleEndian.Uint32(b[i+4 : i+8]))
		if string(b[i:i+4]) == "fmt " && n >= 16 && i+8+n <= len(b) {
			c := b[i+8:]
			return wavHdr{
				tag:    int(binary.LittleEndian.Uint16(c[0:2])),
				chans:  int(binary.LittleEndian.Uint16(c[2:4])),
				rate:   int(binary.LittleEndian.Uint32(c[4:8])),
				bits:   int(binary.LittleEndian.Uint16(c[14:16])),
				fmtLen: n,
			}
		}
		i += 8 + n + n%2
	}
	t.Fatalf("%s has no fmt chunk", p)
	return wavHdr{}
}

// A reference the server will not read is a narrator who never speaks, and the
// header is where that goes wrong: loudnorm hands back 192 kHz, and above
// 48 kHz ffmpeg writes WAVE_FORMAT_EXTENSIBLE, whose format tag the server does
// not recognise. Both files are checked -- the base, and the shifted copy that
// is the one actually uploaded -- and at both ends of the slider, because only
// one of those two branches runs ffmpeg.
func TestTheReferenceIsAWavTheServerWillRead(t *testing.T) {
	a := &App{outDir: t.TempDir(), curCmds: map[*exec.Cmd]bool{}}
	if err := a.setVoice(voiceOpt{id: "cv-xx-female-30s", path: sineWav(t, 220)}); err != nil {
		t.Fatalf("setVoice: %v", err)
	}
	if err := a.ensureVoiceRef(); err != nil {
		t.Fatalf("ensureVoiceRef: %v", err)
	}
	for _, st := range []float64{0, 5} {
		if err := a.setPitchST(st); err != nil {
			t.Fatalf("setPitchST(%v): %v", st, err)
		}
		for _, p := range []string{a.refBase(), a.refPath()} {
			h, name := wavFmt(t, p), filepath.Base(p)
			if h.tag != 1 || h.fmtLen != 16 {
				t.Errorf("%+.0f st: %s has format tag 0x%04X in a %d-byte fmt chunk; "+
					"the server only reads plain PCM (tag 1, 16 bytes)", st, name, h.tag, h.fmtLen)
			}
			if h.rate > 48000 {
				t.Errorf("%+.0f st: %s is %d Hz -- past 48000 the header stops being plain",
					st, name, h.rate)
			}
			if h.chans != 1 || h.bits != 16 {
				t.Errorf("%+.0f st: %s is %d channel / %d bit, want mono 16", st, name, h.chans, h.bits)
			}
		}
	}
}

// The header check itself: what the server accepts, and what ffmpeg writes
// above 48 kHz instead.
func TestOnlyAPlainWavHeaderCountsAsReadable(t *testing.T) {
	dir := t.TempDir()
	mk := func(name string, args ...string) string {
		p := filepath.Join(dir, name)
		a := append([]string{"-v", "error", "-y", "-f", "lavfi", "-i", "sine=frequency=220:duration=1"}, args...)
		if err := exec.Command("ffmpeg", append(a, p)...).Run(); err != nil {
			t.Skipf("no usable ffmpeg: %v", err)
		}
		return p
	}
	if !wavPlain(mk("plain.wav", "-ar", "48000", "-c:a", "pcm_s16le")) {
		t.Error("a 48 kHz PCM16 wav was called unreadable")
	}
	if !wavPlain(mk("float.wav", "-ar", "48000", "-c:a", "pcm_f32le")) {
		t.Error("a float32 wav was called unreadable -- the server takes those")
	}
	// what loudnorm handed over before refRate was pinned
	if wavPlain(mk("ext.wav", "-ar", "192000", "-c:a", "pcm_s16le")) {
		t.Error("a 192 kHz wav passed; that header is WAVE_FORMAT_EXTENSIBLE")
	}
	if p := filepath.Join(dir, "notawav"); os.WriteFile(p, []byte("hello"), 0o644) == nil && wavPlain(p) {
		t.Error("five bytes of text passed as a wav")
	}
	if wavPlain(filepath.Join(dir, "missing.wav")) {
		t.Error("a file that is not there passed")
	}
	short := plainWavBytes(0)
	binary.LittleEndian.PutUint32(short[16:20], 8) // a fmt chunk too short to be one
	p := filepath.Join(dir, "short.wav")
	if os.WriteFile(p, short, 0o644) == nil && wavPlain(p) {
		t.Error("an 8-byte fmt chunk passed as a header")
	}
	// a download or a write that died mid-header: the chunk announces itself
	// and the file ends before the format tag it announced. Reading the tag
	// there is reading past the end of what was loaded, so the bound has to be
	// checked before the read, not after.
	cut := filepath.Join(dir, "cut.wav")
	if os.WriteFile(cut, plainWavBytes(0)[:20], 0o644) == nil && wavPlain(cut) {
		t.Error("a wav that ends inside its fmt chunk passed as a header")
	}
}

// The reference is built once and then kept, so the rate fix alone would never
// reach a project that already HAD one: it would go on uploading the 192 kHz
// file the server refuses, forever, and the voice would simply never speak.
// A reference is therefore judged by its header, not by existing.
func TestAReferenceTheServerCannotReadIsCutAgain(t *testing.T) {
	ownConfig(t)
	a := &App{outDir: t.TempDir(), curCmds: map[*exec.Cmd]bool{}}
	src := sineWav(t, 220)
	// the re-cut reads the voice back out of the voices folder, as it would on
	// a machine the project was carried to
	if err := a.writeConf(appConf{Voices: filepath.Dir(src)}); err != nil {
		t.Fatal(err)
	}
	if err := a.setVoice(voiceOpt{id: "cv-xx-female-30s", path: src}); err != nil {
		t.Fatalf("setVoice: %v", err)
	}
	// exactly what the old levelRef wrote: loudnorm with no rate asked for
	os.MkdirAll(a.narrateDir(), 0o755)
	for _, dst := range []string{a.refBase(), a.refPath()} {
		if err := exec.Command("ffmpeg", "-v", "error", "-y", "-i", src,
			"-af", refLoud, "-ac", "1", "-c:a", "pcm_s16le", dst).Run(); err != nil {
			t.Fatalf("could not write the old-style reference: %v", err)
		}
		if wavPlain(dst) {
			t.Skip("this ffmpeg writes a plain header at loudnorm's rate; nothing to heal")
		}
	}
	if err := a.ensureVoiceRef(); err != nil {
		t.Fatalf("ensureVoiceRef over a stale reference: %v", err)
	}
	for _, p := range []string{a.refBase(), a.refPath()} {
		if h := wavFmt(t, p); h.tag != 1 || h.fmtLen != 16 || h.rate > 48000 {
			t.Errorf("%s is still tag 0x%04X, %d-byte fmt, %d Hz -- the old file was kept",
				filepath.Base(p), h.tag, h.fmtLen, h.rate)
		}
	}

	// and the shifted copy is judged on its OWN header: a base that is fine
	// does not vouch for the file the server is actually handed
	good, err := os.ReadFile(a.refBase())
	if err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(a.refPath(), []byte("RIFF....WAVEfmt "), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := a.ensureVoiceRef(); err != nil {
		t.Fatalf("ensureVoiceRef over a stale shifted copy: %v", err)
	}
	if !wavPlain(a.refPath()) {
		t.Error("the unreadable shifted copy was kept because the base was fine")
	}
	if now, _ := os.ReadFile(a.refBase()); !bytes.Equal(now, good) {
		t.Error("a bad shifted copy sent the base back through ffmpeg too")
	}
}

// TestRefPitchShift is the whole point of the slider: the file the server is
// handed really is moved, by the amount asked for, and the base it came from is
// left alone so the next move is not a shift of a shift.
func TestRefPitchShift(t *testing.T) {
	a := &App{outDir: t.TempDir(), curCmds: map[*exec.Cmd]bool{}}
	src := sineWav(t, 220)
	if err := a.setVoice(voiceOpt{id: "cv-xx-female-30s", path: src}); err != nil {
		t.Fatalf("setVoice: %v", err)
	}
	if err := a.ensureVoiceRef(); err != nil {
		t.Fatalf("ensureVoiceRef: %v", err)
	}
	if got := toneHz(t, a.refPath()); math.Abs(got-220) > 22 {
		t.Fatalf("unshifted reference is %.0f Hz, want ~220", got)
	}

	for _, st := range []float64{6, -6, 0} {
		if err := a.setPitchST(st); err != nil {
			t.Fatalf("setPitchST(%v): %v", st, err)
		}
		want := 220 * math.Pow(2, st/12)
		if got := toneHz(t, a.refPath()); math.Abs(got-want) > want*0.1 {
			t.Errorf("%+.0f semitones: reference is %.0f Hz, want ~%.0f", st, got, want)
		}
		if got := toneHz(t, a.refBase()); math.Abs(got-220) > 22 {
			t.Fatalf("%+.0f semitones moved the base too (%.0f Hz) -- shifts would compound", st, got)
		}
		if got := (&App{outDir: a.outDir}).pitchST(); got != st {
			t.Errorf("reopened project is at %+.1f semitones, want %+.1f", got, st)
		}
	}

	// past the cap the clone stops sounding human; the slider cannot ask for it
	// but a hand-edited pitch.txt can
	if err := a.setPitchST(40); err != nil {
		t.Fatalf("setPitchST(40): %v", err)
	}
	if got := a.pitchST(); got != pitchRange {
		t.Errorf("40 semitones was stored as %+.1f, want the %+.1f cap", got, pitchRange)
	}
}

// TestVoiceRefMigration covers projects narrated before this step gained a
// slider: they have the single unshifted voice_ref.wav and nothing else.
func TestVoiceRefMigration(t *testing.T) {
	a := &App{outDir: t.TempDir(), curCmds: map[*exec.Cmd]bool{}}
	if err := os.MkdirAll(a.narrateDir(), 0o755); err != nil {
		t.Fatal(err)
	}
	// a readable one: a reference the server refuses is re-cut rather than
	// adopted (TestAReferenceTheServerCannotReadIsCutAgain), which is a
	// different claim from this one
	old := plainWavBytes(128)
	if err := os.WriteFile(a.refPath(), old, 0o644); err != nil {
		t.Fatal(err)
	}
	if err := a.ensureVoiceRef(); err != nil {
		t.Fatalf("ensureVoiceRef: %v", err)
	}
	got, err := os.ReadFile(a.refBase())
	if err != nil {
		t.Fatalf("the old reference was not kept as the base: %v", err)
	}
	if !bytes.Equal(got, old) {
		t.Fatal("the old reference was replaced rather than adopted as the base")
	}
	if got, _ := os.ReadFile(a.refPath()); !bytes.Equal(got, old) {
		t.Fatal("an unshifted project lost the reference the server reads")
	}
}

// TestVoicePitchLive is the end-to-end claim: shifting the reference shifts the
// clone. The same sentence, the same speaker, three settings, and a voice that
// actually moves with the slider. Needs the TTS server and writes under the
// project folder the server has mounted at its own path, so it is opt-in.
//
//	NAIVEPOST_TTS_LIVE=1 go test -run TestVoicePitchLive -v -timeout 30m
func TestVoicePitchLive(t *testing.T) {
	if os.Getenv("NAIVEPOST_TTS_LIVE") == "" {
		t.Skip("set NAIVEPOST_TTS_LIVE=1 to speak against the real server")
	}
	root := "/home/draft/git/aitools/naivepost"
	a := &App{
		root:    root,
		outDir:  filepath.Join(root, "out", "test", "pitchlive"),
		curCmds: map[*exec.Cmd]bool{},
	}
	src := filepath.Join(a.voicesDir(), "audio-sample-tbocek.wav")
	if !exists(src) {
		t.Skipf("no reference wav at %s", src)
	}
	if err := a.setVoice(voiceOpt{id: "audio-sample-tbocek", path: src}); err != nil {
		t.Fatalf("setVoice: %v", err)
	}
	const line = "This is the voice the narration will be spoken in."
	f0 := map[float64]float64{}
	for _, st := range []float64{-4, 0, 4} {
		if err := a.setPitchST(st); err != nil {
			t.Fatalf("setPitchST(%v): %v", st, err)
		}
		out := a.sampleWav(a.voiceKey(), line)
		if !exists(out) {
			if err := a.speak(line, "", ttsSeed(out), out); err != nil {
				t.Fatalf("speak at %+.0f: %v", st, err)
			}
		}
		f0[st] = speechHz(t, out)
		t.Logf("%+.0f semitones: reference %.0f Hz -> clone %.0f Hz  (%s)",
			st, speechHz(t, a.refPath()), f0[st], filepath.Base(out))
	}
	if !(f0[-4] < f0[0] && f0[0] < f0[4]) {
		t.Errorf("the clone did not follow the slider: %.0f / %.0f / %.0f Hz for -4 / 0 / +4",
			f0[-4], f0[0], f0[4])
	}
}

// speechHz is a median f0 over the voiced frames -- autocorrelation, which
// unlike counting zero crossings survives speech.
func speechHz(t *testing.T, wav string) float64 {
	t.Helper()
	const sr, win = 16000, 640 // 40 ms
	raw, err := exec.Command("ffmpeg", "-v", "error", "-i", wav,
		"-f", "s16le", "-ac", "1", "-ar", "16000", "-").Output()
	if err != nil {
		t.Fatalf("decode %s: %v", wav, err)
	}
	x := make([]float64, len(raw)/2)
	for i := range x {
		x[i] = float64(int16(uint16(raw[2*i]) | uint16(raw[2*i+1])<<8))
	}
	lo, hi := sr/400, sr/70 // the range a human voice lives in
	var f0s []float64
	for i := 0; i+win <= len(x); i += win / 2 {
		f := append([]float64(nil), x[i:i+win]...)
		var mean, energy float64
		for _, v := range f {
			mean += v
		}
		mean /= win
		for j := range f {
			f[j] -= mean
			energy += f[j] * f[j]
		}
		if math.Sqrt(energy/win) < 300 { // silence between words
			continue
		}
		bestK, best := 0, 0.0
		for k := lo; k < hi; k++ {
			var c float64
			for j := 0; j+k < win; j++ {
				c += f[j] * f[j+k]
			}
			if c > best {
				bestK, best = k, c
			}
		}
		if bestK > 0 && best > 0.3*energy {
			f0s = append(f0s, float64(sr)/float64(bestK))
		}
	}
	if len(f0s) == 0 {
		t.Fatalf("no voiced frames in %s", wav)
	}
	sort.Float64s(f0s)
	return f0s[len(f0s)/2]
}

func TestTTSWavKeyedOnVoice(t *testing.T) {
	a := &App{outDir: t.TempDir()}
	e := narrEntry{Text: "the same line", Emotion: "calm"}

	a.voiceSel, a.pitchSel, a.pitchRead = ownVoice, 0, true
	own := a.ttsWav(e)
	a.voiceSel = "cv-gb-female-30s"
	other := a.ttsWav(e)
	if own == other {
		t.Fatal("two voices share one cache file — a switch would serve the old speaker")
	}
	// pitch is part of the voice: shifting it must not serve the unshifted take
	a.pitchSel = -3
	shifted := a.ttsWav(e)
	if shifted == other {
		t.Fatal("shifting the reference kept the cache file — the slider would seem to do nothing")
	}
	a.pitchSel = 0
	if again := a.ttsWav(e); again != other {
		t.Fatalf("returning to the unshifted voice missed its cache: %s vs %s", again, other)
	}
	a.voiceSel = ownVoice
	if again := a.ttsWav(e); again != own {
		t.Fatalf("switching back missed the earlier cache: %s vs %s", again, own)
	}
}

// The imported id becomes a file name in the sample cache and the contents of
// voice.txt, so anything that could steer a write out of the folder has to be
// gone before it gets there.
func TestSanitizeVoiceID(t *testing.T) {
	for in, want := range map[string]string{
		"cv-gb-female-30s":  "cv-gb-female-30s",
		"my recording":      "my-recording",
		"../../etc/passwd":  "etc-passwd",
		"Tom's take #2":     "Tom-s-take--2",
		"jan2_2026-08-08":   "jan2_2026-08-08",
		"...":               "voice",
		"":                  "voice",
		"/absolute/path/wv": "absolute-path-wv",
	} {
		got := sanitizeVoiceID(in)
		if got != want {
			t.Errorf("sanitizeVoiceID(%q) = %q, want %q", in, got, want)
		}
		if strings.ContainsAny(got, `/\`) || got == "." || got == ".." {
			t.Errorf("sanitizeVoiceID(%q) = %q, which is not a plain file name", in, got)
		}
	}
}

// An import must never land on a name already in use: the id is what every
// project's voice.txt points at, so overwriting one would silently change the
// speaker in a project that was never opened.
func TestImportVoiceKeepsNamesDistinct(t *testing.T) {
	ownConfig(t)
	root, models := t.TempDir(), t.TempDir()
	a := &App{root: root, outDir: t.TempDir(), curCmds: map[*exec.Cmd]bool{}}
	if err := os.WriteFile(confPath(),
		[]byte("AUDIOCPP_MODELS="+strconv.Quote(models)+"\n"), 0o600); err != nil {
		t.Fatal(err)
	}

	src := sineWav(t, 330)
	first, err := a.importVoice(src)
	if err != nil {
		t.Fatalf("importVoice: %v", err)
	}
	second, err := a.importVoice(src)
	if err != nil {
		t.Fatalf("importVoice (again): %v", err)
	}
	if first.id == second.id {
		t.Fatalf("both imports claimed the id %q", first.id)
	}
	if !exists(first.path) || !exists(second.path) {
		t.Fatal("an imported voice is missing from the voices folder")
	}
	names := map[string]bool{}
	for _, v := range a.listVoices() {
		names[v.id] = true
	}
	if !names[first.id] || !names[second.id] {
		t.Fatalf("imported voices are not listed: %v", names)
	}
}

// toneFile writes a one-second tone under a name of the caller's choosing --
// what a folder import cares about is which files it picks up, not how they
// sound.
func toneFile(t *testing.T, dir, name string) string {
	t.Helper()
	p := filepath.Join(dir, name)
	if err := exec.Command("ffmpeg", "-v", "error", "-y", "-f", "lavfi",
		"-i", "sine=frequency=330:duration=1", p).Run(); err != nil {
		t.Skipf("no usable ffmpeg: %v", err)
	}
	return p
}

// TestEveryPlayerSaysWhenItFails: GStreamer reports a file it cannot decode on
// the bus and nowhere else, so a player without OnError swallows it -- the
// window goes on showing ⏸ over silence and the log never mentions it. Nothing
// at run time notices a missing hookup, hence source level, and hence counting
// rather than spot-checking: the next player added is the one that would be
// forgotten.
func TestEveryPlayerSaysWhenItFails(t *testing.T) {
	made := regexp.MustCompile(`NewPlayer\(\)`)
	wired := regexp.MustCompile(`(?m)^\s*\S+\.OnError = `)
	for _, f := range []string{"main.go", "cut.go", "narrate.go", "narrate_voice.go"} {
		src, err := os.ReadFile(f)
		if err != nil {
			t.Fatal(err)
		}
		n, w := len(made.FindAll(src, -1)), len(wired.FindAll(src, -1))
		if n != w {
			t.Errorf("%s builds %d players but wires OnError on %d — one of them fails silently", f, n, w)
		}
	}
	// and the handler has to reach the log, not stdout, which is where these
	// went before and where nobody running the app from a launcher looks
	src, err := os.ReadFile("player.go")
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(src), "func (a *App) playerErr(") {
		t.Fatal("playerErr is gone; the OnError hookups above point at nothing")
	}
	if !regexp.MustCompile(`(?s)func \(a \*App\) playerErr\(.*?a\.logf\(`).Match(src) {
		t.Error("playerErr does not write to the log")
	}
	if !regexp.MustCompile(`(?s)func \(a \*App\) playerErr\(.*?a\.setStatus\(`).Match(src) {
		t.Error("playerErr does not touch the status line")
	}
}

// TestSampleAccountsForItself: the sample is the one thing on this page that
// leaves no artifact to inspect -- no clip, no row, no file the user goes
// looking for -- so if the run says nothing, the run is gone. Each branch of
// playSample must reach either the log or the status line.
func TestSampleAccountsForItself(t *testing.T) {
	src, err := os.ReadFile("narrate_voice.go")
	if err != nil {
		t.Fatal(err)
	}
	body := regexp.MustCompile(`(?s)func \(vp \*voicePicker\) playSample\(\).*?\n}\n`).Find(src)
	if body == nil {
		t.Fatal("playSample not found")
	}
	for _, want := range []string{
		`a.logf(">>> sample%s:`, // what was asked for, and which take, before it can fail
		`a.logf("sample FAILED`, // the server said no
		`a.logf("!!! sample:`,   // the server said yes and sent nothing
		`a.logf("    sample:`,   // it worked: which file, how big, how long
	} {
		if !strings.Contains(string(body), want) {
			t.Errorf("playSample has no %s line — that path runs silently", want)
		}
	}
	// the already-playing branch of the button, which changed the icon and said
	// nothing at all
	tog := regexp.MustCompile(`(?s)func \(vp \*voicePicker\) playClicked\(\).*?\n}\n`).Find(src)
	if !strings.Contains(string(tog), "setStatus") {
		t.Error("playClicked's pause/resume branch reports nothing")
	}
}

// TestTheVoiceRowKeepsButtonHeight: the pitch slider shares a row with an
// entry and three transport buttons. Left to fill, those buttons stretched to
// the slider's height and the row looked broken. The height is capped from
// both ends: the slider wears the one shape every slider in the app wears --
// value beside the trough, not stacked over it (formSlider) -- and the
// controls beside it sit centered at their own size rather than filling.
func TestTheVoiceRowKeepsButtonHeight(t *testing.T) {
	body := funcBody(t, "narrate_voice.go", `func \(a \*App\) buildVoicePicker\(\)`)
	for _, want := range []string{
		"hear.SetVAlign(gtk.AlignCenter)",
		"knob.SetVAlign(gtk.AlignCenter)",
		"formSlider(vp.pitch, ",
	} {
		if !strings.Contains(body, want) {
			t.Errorf("the voice row lost %s — its buttons grow to the slider's height", want)
		}
	}
	// one recipe, so no page can grow a slider of its own shape
	mk := funcBody(t, "player.go", `func formSlider\(sc \*gtk.Scale, tip string\) \{`)
	for _, want := range []string{"sc.SetValuePos(gtk.PosRight)", "sc.SetVAlign(gtk.AlignCenter)", "sc.SetHAlign(gtk.AlignStart)"} {
		if !strings.Contains(mk, want) {
			t.Errorf("formSlider no longer does %q", want)
		}
	}
	for _, f := range []string{"produce.go", "narrate_voice.go"} {
		if strings.Contains(readSrc(t, f), "SetDrawValue(true)") {
			t.Errorf("%s builds a slider by hand instead of through formSlider", f)
		}
	}
}
