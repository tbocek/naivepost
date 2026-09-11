package main

// The sample row is three buttons over a synthesis cache, and everything that
// can go wrong with it is a question of identity: what would ▶ speak now, is
// that what the player is already holding, and does the file on disk belong to
// this take or the one before it. The picker's widgets are not testable without
// a display, so the judgements are all here, out of them.
//
// The reference the model clones from is checked here too: it is written by
// three different paths and a loudness rule that only two of them followed
// would be a voice that changed level when the project moved.

import (
	"math"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
)

// ---- what ▶ does ------------------------------------------------------------

func TestASampleNobodyChangedIsResumedRatherThanSpokenAgain(t *testing.T) {
	if sampleNeedsSpeaking(true, "cv-en-a|hello", "cv-en-a|hello") {
		t.Error("pressing ▶ on a paused sample re-synthesized it instead of resuming")
	}
}

func TestAChangedPitchIsSpokenAgainRatherThanReplayed(t *testing.T) {
	if !sampleNeedsSpeaking(true, "cv-en-a|hello", "cv-en-a@+4.0|hello") {
		t.Error("after moving the pitch slider ▶ replayed the old setting")
	}
}

func TestAChangedVoiceIsSpokenAgainRatherThanReplayed(t *testing.T) {
	if !sampleNeedsSpeaking(true, "cv-en-a|hello", "cv-en-b|hello") {
		t.Error("after picking another voice ▶ replayed the previous one")
	}
}

func TestWithNothingLoadedThereIsNothingToResume(t *testing.T) {
	if !sampleNeedsSpeaking(false, "cv-en-a|hello", "cv-en-a|hello") {
		t.Error("▶ resumed a player that holds nothing")
	}
}

// ---- what a sample is -------------------------------------------------------

func samplePicker(t *testing.T) *voicePicker {
	t.Helper()
	dir := t.TempDir()
	a := &App{root: dir, outDir: dir}
	if err := os.MkdirAll(a.narrateDir(), 0o755); err != nil {
		t.Fatalf("narrate dir: %v", err)
	}
	return &voicePicker{a: a}
}

func TestThePitchIsPartOfWhatASampleIs(t *testing.T) {
	vp := samplePicker(t)
	flat := vp.sampleKey("hello")
	if err := vp.a.setPitchST(4); err != nil {
		t.Fatalf("setPitchST: %v", err)
	}
	if up := vp.sampleKey("hello"); up == flat {
		t.Errorf("the same key %q at two pitches -- the slider would move nothing", up)
	}
}

func TestTheWordsArePartOfWhatASampleIs(t *testing.T) {
	vp := samplePicker(t)
	if vp.sampleKey("hello") == vp.sampleKey("goodbye") {
		t.Error("two sample texts share a key")
	}
}

func TestARerollIsAnotherTakeOfTheSameWords(t *testing.T) {
	vp := samplePicker(t)
	first := vp.sampleKey("hello")
	vp.roll++
	if again := vp.sampleKey("hello"); again == first {
		t.Errorf("⟳ left the key at %q, so the cache would serve the same take back", again)
	}
}

func TestASampleNobodyRerolledKeepsTheKeyItAlreadyHad(t *testing.T) {
	vp := samplePicker(t)
	if k := vp.sampleKey("hello"); !strings.HasPrefix(k, vp.a.voiceKey()+"|") {
		t.Errorf("key %q is not the plain voice-and-words key -- samples spoken before ⟳ existed would be orphaned", k)
	}
}

func TestSpaceAroundTheSampleTextIsNotAnotherSample(t *testing.T) {
	vp := samplePicker(t)
	if vp.sampleKey("hello") != vp.sampleKey("  hello \n") {
		t.Error("a trailing space asked the server for the same words twice")
	}
}

func TestTheSampleFileFollowsTheWholeKeyNotJustTheWords(t *testing.T) {
	vp := samplePicker(t)
	a, id := vp.a, vp.a.voiceID()
	flat := a.sampleWav(id, vp.sampleKey("hello"))
	vp.roll++
	if rolled := a.sampleWav(id, vp.sampleKey("hello")); rolled == flat {
		t.Errorf("both takes are cached as %s, so the second would never be synthesized", filepath.Base(flat))
	}
}

// TestTheSampleRowIsWiredToTheseJudgements pins the widget half, which no test
// can click: the rules above are only worth anything if the buttons ask them.
func TestTheSampleRowIsWiredToTheseJudgements(t *testing.T) {
	if b := funcBody(t, "narrate_voice.go", `func \(vp \*voicePicker\) playClicked\(`); !strings.Contains(b, "sampleNeedsSpeaking(vp.playing() || vp.cued(), vp.spoken, vp.sampleKey(vp.sample.Text()))") {
		t.Error("▶ decides whether to resume without asking sampleNeedsSpeaking")
	}
	play := funcBody(t, "narrate_voice.go", `func \(vp \*voicePicker\) playSample\(`)
	for _, want := range []string{
		"key := vp.sampleKey(text)",      // what is being asked for
		"a.sampleWav(a.voiceKey(), key)", // named after all of it, not just the words
		"vp.spoken = key",                // remembered, so the next ▶ can judge
	} {
		if !strings.Contains(play, want) {
			t.Errorf("playSample has no %q -- the cache or the button would answer for the wrong take", want)
		}
	}
	roll := funcBody(t, "narrate_voice.go", `func \(vp \*voicePicker\) rollClicked\(`)
	// vp.busy as well: a ⟳ during a synthesis would start a second one, and
	// the two would race to be the file the player then loads
	for _, want := range []string{"vp.busy", "vp.roll++", "vp.stop()", "vp.playSample()"} {
		if !strings.Contains(roll, want) {
			t.Errorf("⟳ has no %q, so it would not produce a new take", want)
		}
	}
	src := readSrc(t, "narrate_voice.go")
	if !strings.Contains(src, "hear.Append(vp.rollBtn)") {
		t.Error("the ⟳ button is built but never put in the sample row")
	}
	if !strings.Contains(src, "vp.rollBtn.ConnectClicked(vp.rollClicked)") {
		t.Error("the ⟳ button is in the row but does nothing")
	}
}

// ---- the reference the model clones from ------------------------------------

// rmsDB is the level of a wav, in dB below full scale.
func rmsDB(t *testing.T, wav string) float64 {
	t.Helper()
	raw, err := exec.Command("ffmpeg", "-v", "error", "-i", wav,
		"-f", "s16le", "-ac", "1", "-ar", "16000", "-").Output()
	if err != nil {
		t.Fatalf("decode %s: %v", wav, err)
	}
	n := len(raw) / 2
	if n == 0 {
		t.Fatalf("%s decoded to nothing", wav)
	}
	sum := 0.0
	for i := 0; i < n; i++ {
		s := float64(int16(uint16(raw[2*i]) | uint16(raw[2*i+1])<<8))
		sum += s * s
	}
	return 20 * math.Log10(math.Sqrt(sum/float64(n))/32768)
}

func TestAQuietRecordingIsLevelledBeforeTheModelClonesIt(t *testing.T) {
	dir := t.TempDir()
	quiet := filepath.Join(dir, "quiet.wav")
	if err := exec.Command("ffmpeg", "-v", "error", "-y", "-f", "lavfi",
		"-i", "sine=frequency=220:duration=8", "-af", "volume=0.02",
		"-ac", "1", "-c:a", "pcm_s16le", quiet).Run(); err != nil {
		t.Skipf("no usable ffmpeg: %v", err)
	}
	a := &App{root: dir, outDir: dir, curCmds: map[*exec.Cmd]bool{}}
	ref := filepath.Join(dir, "ref.wav")
	if err := a.levelRef(ref, "-i", quiet); err != nil {
		t.Fatalf("levelRef: %v", err)
	}
	// mono as well as levelled: the server clones one voice, and a reference
	// that arrived in stereo is one it has to make a choice about
	if ch, err := exec.Command(ffTool("ffprobe"), "-v", "error", "-select_streams", "a:0",
		"-show_entries", "stream=channels", "-of", "csv=p=0", ref).Output(); err != nil {
		t.Fatalf("ffprobe: %v", err)
	} else if strings.TrimSpace(string(ch)) != "1" {
		t.Errorf("the reference came out %s-channel", strings.TrimSpace(string(ch)))
	}
	in, out := rmsDB(t, quiet), rmsDB(t, ref)
	t.Logf("reference levelled from %.1f dB to %.1f dB", in, out)
	if out < in+20 {
		t.Errorf("a %.1f dB recording came out at %.1f dB -- the clone would murmur under the game audio", in, out)
	}
}

func TestEveryWayOfMakingAReferenceLevelsIt(t *testing.T) {
	// a reference has three makers: the voice you pick, the same voice healed
	// on a machine where the file went missing, and the takes cut out of a
	// narrator's own recording. One of them levelling and the others not is a
	// voice that changes loudness when the project moves.
	if b := funcBody(t, "narrate_voice.go", `func \(a \*App\) setVoice\(`); !strings.Contains(b, "a.levelRef(a.refBase()") {
		t.Error("the picked voice is written to the reference without being levelled")
	}
	if n := strings.Count(funcBody(t, "narrate_voice.go", `func \(a \*App\) ensureVoiceBase\(`), "a.levelRef("); n != 2 {
		t.Errorf("ensureVoiceBase levels %d of its two references (the healed wav, the narrator's takes)", n)
	}
	if !strings.Contains(funcBody(t, "narrate_voice.go", `func \(a \*App\) levelRef\(`), `"-af", refLoud`) {
		t.Error("levelRef does not apply refLoud")
	}
}

func TestTheShiftedCopyIsNotLevelledASecondTime(t *testing.T) {
	b := funcBody(t, "narrate_voice.go", `func \(a \*App\) shiftRef\(`)
	// by any spelling: the helper, the rule's name, or the filter written out
	if strings.Contains(b, "levelRef") ||
		strings.Contains(b, "refLoud") || strings.Contains(b, "loudnorm") {
		t.Error("the pitch-shifted copy is levelled again, so the same voice would sit at two levels depending on the slider")
	}
}
