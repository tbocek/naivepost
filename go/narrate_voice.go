package main

import (
	"crypto/sha1"
	"encoding/binary"
	"fmt"
	"io"
	"math"
	"os"
	"path/filepath"
	"sort"
	"strconv"
	"strings"
	"time"

	"github.com/diamondburned/gotk4/pkg/glib/v2"
	"github.com/diamondburned/gotk4/pkg/gtk/v4"
	"github.com/diamondburned/gotk4/pkg/pango"
)

// The voice every line is spoken in. IndexTTS2 takes timbre from a reference
// wav: one of the session's speakers (cut from the recording tagged with that
// narrator slot) or a file in the voices folder, installed as
// narrate/voice_ref.wav for the TTS server. The pitch slider shifts the
// reference before cloning (voice_ref_base.wav is unshifted); the synthesis
// cache is keyed by voice and pitch.

// ownVoice is the id of "clone me", the default the pipeline had before this
// step existed. It is narrator 1's id and stays spelled "own": it is what every
// existing project's voice.txt says and what every synthesis cache key was
// built from, and renaming it would silently re-speak finished work.
const ownVoice = "own"

// narratorPrefix names the other three. They exist because a session is often a
// group: the Inputs step tags up to four voices, and any of them can be the one
// the narration is spoken in.
const narratorPrefix = "narrator"

// captionsVoice is the "no audio" choice: the narration is written and timed
// exactly as ever, but nothing is synthesized and nothing is mixed in -- the
// lines exist as captions alone, for the Produce step to burn in or ship as
// subtitles. Everything that would speak asks captionsOnly first.
const captionsVoice = "captions"

// captionsOnly is whether this project's narration is written but never
// spoken.
func (a *App) captionsOnly() bool { return a.voiceID() == captionsVoice }

// narratorVoiceID is the voice id for a narrator slot.
func narratorVoiceID(n int) string {
	if n <= 1 {
		return ownVoice
	}
	return narratorPrefix + strconv.Itoa(n)
}

// narratorSlot is the reverse: the slot an id names, or 0 for a wav out of the
// voices folder.
func narratorSlot(id string) int {
	if id == ownVoice {
		return 1
	}
	if !strings.HasPrefix(id, narratorPrefix) {
		return 0
	}
	n, err := strconv.Atoi(strings.TrimPrefix(id, narratorPrefix))
	if err != nil || n < 2 || n > narratorSlots {
		return 0
	}
	return n
}

const sampleDefault = "This is the voice the narration will be spoken in. " +
	"It should stay clear and easy to follow for a couple of minutes."

// pitchRange caps the reference shift at half an octave either way. Past that
// the clone stops sounding like a person rather than like a different one.
const pitchRange = 6.0

type voiceOpt struct {
	id   string // a narrator slot ("own", "narrator2"..) or the wav's base name;
	name string // also part of the cache key. name is what the row reads
	path string // source wav; empty for a narrator, who is cut from the recording
}

type voicePicker struct {
	a *App
	// the choice, as a closed dropdown: its row text says which voice and which
	// recording it is cut from, which is the whole of what the label under it
	// used to say back
	pick  *gtk.DropDown
	names *gtk.StringList // the same rows as voices, in the same order
	// the picture of the recording this voice is cut from, and the seconds
	// picked out of it by hand (narrate_takeband.go). Nil in a test that
	// never built the page.
	band    *takeBand
	sample  *gtk.Entry
	pitch   *gtk.Scale // semitones on the reference: a different speaker
	voices  []voiceOpt
	player  *Player
	playBtn *gtk.Button // ▶/⏸ for the sample; drawn by syncPlayIcons
	stopBtn *gtk.Button // and its ⏹, lit only while there is a sample to end
	rollBtn *gtk.Button // ⟳: the same words again, drawn differently
	roll    int         // how many times that has been asked for
	spoken  string      // sampleKey of the wav in the player, or "" for none
	busy    bool        // one synthesis at a time; the button says so
	syncing bool        // showing the stored voice, which is not a fresh choice
	drag    int         // bumped per slider move; only the last one rebuilds the reference
}

// voicesDir is where the reference wavs live -- the same folder the WebUI
// lists in its "Built-in voice" dropdown, mounted into the server by the
// compose file.
func (a *App) voicesDir() string { return a.readConf().Voices }

func (a *App) listVoices() []voiceOpt {
	// "no audio" first -- it is the one entry that is not a voice at all --
	// then the people in the session, then the CC0 samples. Slot 1 is always
	// offered -- it is the default, and an untagged session still falls back to
	// a recording -- while 2..4 appear only once somebody is tagged as them.
	out := []voiceOpt{{id: captionsVoice, name: "No audio — captions only"}}
	for n := 1; n <= narratorSlots; n++ {
		if n > 1 && a.narratorPath(n) == "" {
			continue
		}
		out = append(out, voiceOpt{id: narratorVoiceID(n), name: a.narratorVoiceName(n)})
	}
	dir := a.voicesDir()
	ents, _ := os.ReadDir(dir)
	var names []string
	for _, e := range ents {
		if !e.IsDir() && strings.EqualFold(filepath.Ext(e.Name()), ".wav") {
			names = append(names, e.Name())
		}
	}
	sort.Strings(names)
	for _, n := range names {
		// the file name, not a prettified version of it: these are files on
		// disk that can be opened, replaced and added to, and a row that reads
		// "gb · female · 30s" cannot be looked up in a folder
		out = append(out, voiceOpt{
			id:   strings.TrimSuffix(n, filepath.Ext(n)),
			name: n,
			path: filepath.Join(dir, n),
		})
	}
	return out
}

// narratorSource is the recording slot n is actually cut from: whoever carries
// the tag on the Inputs step, and for slot 1 the session's fallback as well --
// a project from before the tags existed still has to speak.
func (a *App) narratorSource(n int) string {
	if p := a.narratorPath(n); p != "" {
		return p
	}
	if n == 1 {
		return a.voiceSource()
	}
	return ""
}

// narratorFile is the same by name. Which file it is decides who speaks the
// narration, so the row says it rather than leaving it to be inferred.
func (a *App) narratorFile(n int) string {
	if p := a.narratorSource(n); p != "" {
		return filepath.Base(p)
	}
	return ""
}

// narratorVoiceName is what the picker lists a slot as. Slot 1 is "Narrator 1"
// like the other three: it is a tag on the Inputs step, and whoever is running
// the app need not be the one wearing it. Its id stays "own" -- that spelling
// is in every synthesis cache key written before the tags existed.
func (a *App) narratorVoiceName(n int) string {
	who := fmt.Sprintf("Narrator %d", n)
	if f := a.narratorFile(n); f != "" {
		return who + " — " + f
	}
	return who + " — cut from the recording"
}

// ---- the chosen voice -------------------------------------------------------

// voiceID is the voice this project speaks in, remembered in the output folder
// so it survives a restart, and mixed into the synthesis cache key so switching
// voices does not throw away work.
func (a *App) voiceID() string {
	a.voiceMu.Lock()
	defer a.voiceMu.Unlock()
	if a.voiceSel != "" {
		return a.voiceSel
	}
	a.voiceSel = ownVoice
	if b, err := os.ReadFile(filepath.Join(a.narrateDir(), "voice.txt")); err == nil {
		if s := strings.TrimSpace(string(b)); s != "" {
			a.voiceSel = s
		}
	}
	return a.voiceSel
}

// refLoud levels the reference before the model hears it: a clone is as loud
// as its source, and a quiet narrator under loudnorm'd game audio is somebody
// murmuring. Single-pass (dynamic) loudnorm, since the takes come from
// different minutes; LRA tight because it is one person talking.
const refLoud = "loudnorm=I=-16:TP=-1.5:LRA=7"

// refRate is what a reference is written at. loudnorm resamples to 192 kHz,
// and above 48 kHz ffmpeg writes WAVE_FORMAT_EXTENSIBLE (tag 0xFFFE), which
// the audio server refuses ("unsupported WAV encoding"). 48 kHz keeps the
// header plain and is at or above anything we clone from.
const refRate = "48000"

// wavPlain asks the question the server asks: is this a wav whose samples it
// knows how to read. The format tag is the whole answer -- 1 for integer PCM,
// 3 for float, and 0xFFFE for the WAVE_FORMAT_EXTENSIBLE header ffmpeg writes
// above refRate, which is the one it refuses. The chunk's length is not the
// test: a plain float wav carries 18 bytes rather than 16. Only the head of the
// file is read -- fmt comes before the samples, and the samples are megabytes.
func wavPlain(p string) bool {
	f, err := os.Open(p)
	if err != nil {
		return false
	}
	defer f.Close()
	b := make([]byte, 512)
	n, _ := io.ReadFull(f, b)
	if n < 12 || string(b[:4]) != "RIFF" || string(b[8:12]) != "WAVE" {
		return false
	}
	// walk only as far as a whole fmt chunk head plus its tag can have been
	// read. A file that ends inside its own fmt chunk never reaches the tag,
	// and unread is not readable: it is refused with everything else.
	for i := 12; i+10 <= n; {
		size := int(binary.LittleEndian.Uint32(b[i+4 : i+8]))
		if string(b[i:i+4]) == "fmt " {
			if size < 16 {
				return false
			}
			tag := binary.LittleEndian.Uint16(b[i+8 : i+10])
			return tag == 1 || tag == 3
		}
		i += 8 + size + size%2
	}
	return false
}

// levelRef runs one ffmpeg pass into a reference file: whatever the input is,
// out as levelled mono PCM. Three things write a reference -- a picked wav
// chosen, the same wav healed on another machine, and a narrator's own takes
// concatenated -- and a level rule that only two of them followed would be a
// voice that changed loudness when the project moved.
func (a *App) levelRef(dst string, in ...string) error {
	args := append([]string{"-v", "error", "-y"}, in...)
	args = append(args, "-af", refLoud, "-ac", "1", "-ar", refRate, "-c:a", "pcm_s16le", dst)
	return a.runCmd(ffTool("ffmpeg"), args...)
}

// refBase is the chosen recording as it was picked, refPath the shifted copy
// the server is handed. Keeping them apart is what makes the slider cheap: the
// base is cut or converted once, the shift is redone from it.
// The base is also where the rate is decided (refRate): every writer of it goes
// through levelRef, and shifting does not resample, so pinning it once covers
// the file the server actually reads.
func (a *App) refBase() string { return filepath.Join(a.narrateDir(), "voice_ref_base.wav") }
func (a *App) refPath() string { return filepath.Join(a.narrateDir(), "voice_ref.wav") }

// setVoice installs the reference the model clones from. A narrator slot just
// removes the files, so ensureVoiceRef cuts a fresh one from that recording the
// next time anything speaks -- which is also how a project built under an older
// rule for choosing those seconds gets a reference under the current one.
// Anything else is converted to the mono PCM the model wants.
// Safe off the GUI thread -- it only runs ffmpeg and writes files.
func (a *App) setVoice(v voiceOpt) error {
	dir := a.narrateDir()
	if err := os.MkdirAll(dir, 0o755); err != nil {
		return err
	}
	// the shifted copy belongs to the old voice; drop it either way and let
	// ensureVoiceRef rebuild it from whatever base we end up with. "No audio"
	// needs no reference at all -- nothing will ever be synthesized from it.
	os.Remove(a.refPath())
	if v.id == captionsVoice || narratorSlot(v.id) > 0 {
		os.Remove(a.refBase())
	} else if err := a.levelRef(a.refBase(), "-i", v.path); err != nil {
		return err
	}
	if err := os.WriteFile(filepath.Join(dir, "voice.txt"), []byte(v.id), 0o644); err != nil {
		return err
	}
	a.voiceMu.Lock()
	a.voiceSel = v.id
	a.voiceMu.Unlock()
	return nil
}

// importVoice adds a recording to the voices folder as the mono PCM wav the
// model wants -- so any audio file works, not just the ones already in the
// right format. It lands beside the built-in voices rather than in the project
// because the id the caches and voice.txt use is the file's base name, which
// only keeps resolving while the file stays somewhere the list looks; a voice
// added here is therefore available to every project, and to the WebUI.
// Safe off the GUI thread -- ffmpeg and files only.
func (a *App) importVoice(src string) (voiceOpt, error) {
	dir := a.voicesDir()
	if err := os.MkdirAll(dir, 0o755); err != nil {
		return voiceOpt{}, fmt.Errorf("%s is not writable: %w", dir, err)
	}
	base := sanitizeVoiceID(strings.TrimSuffix(filepath.Base(src), filepath.Ext(src)))
	id, dst := base, filepath.Join(dir, base+".wav")
	// never overwrite: a name already taken belongs to another voice, and every
	// project pointing at it would start speaking in this one instead
	for i := 2; exists(dst); i++ {
		id = fmt.Sprintf("%s-%d", base, i)
		dst = filepath.Join(dir, id+".wav")
	}
	if err := a.runCmd(ffTool("ffmpeg"), "-v", "error", "-y", "-i", src,
		"-ac", "1", "-c:a", "pcm_s16le", dst); err != nil {
		return voiceOpt{}, err
	}
	return voiceOpt{id: id, name: filepath.Base(dst), path: dst}, nil
}

// A folder import used to live here: pick a folder, convert every recording
// directly in it, skip what is there already. It went with the row that drove
// it. Voices arrive one at a time -- you hear a sample, you want THAT speaker
// -- and a button that installs thirty at once was answering a question about
// stocking a library, which is a file manager's job and not this page's.

// sanitizeVoiceID keeps the id usable as a file name: it is pasted into the
// sample cache's names and read back out of voice.txt, so a slash in it would
// write somewhere else entirely.
func sanitizeVoiceID(s string) string {
	s = strings.Map(func(r rune) rune {
		switch {
		case r >= 'a' && r <= 'z', r >= 'A' && r <= 'Z', r >= '0' && r <= '9':
			return r
		case r == '-', r == '_', r == '.':
			return r
		}
		return '-'
	}, s)
	if s = strings.Trim(s, "-._"); s == "" {
		return "voice"
	}
	return s
}

// ---- the reference pitch ----------------------------------------------------

// pitchST is how far the reference is moved, in semitones, remembered in the
// output folder next to the voice it belongs to.
func (a *App) pitchST() float64 {
	// under voiceMu with the voice id and the takes, and for the same reason:
	// voiceKey asks all three, and it is asked by whichever worker is about to
	// speak a line as well as by the GUI thread that moved the slider
	a.voiceMu.Lock()
	defer a.voiceMu.Unlock()
	if a.pitchRead {
		return a.pitchSel
	}
	a.pitchRead, a.pitchSel = true, 0
	if b, err := os.ReadFile(filepath.Join(a.narrateDir(), "pitch.txt")); err == nil {
		if f, err := strconv.ParseFloat(strings.TrimSpace(string(b)), 64); err == nil {
			a.pitchSel = clampPitch(f)
		}
	}
	return a.pitchSel
}

func clampPitch(st float64) float64 {
	return math.Max(-pitchRange, math.Min(pitchRange, st))
}

// setPitchST remembers the shift and drops the shifted reference so it is built
// again. When there is no base yet -- "own voice" before Prepare has run -- that
// is all there is to do; the next line spoken cuts one and shifts it then.
// Safe off the GUI thread.
func (a *App) setPitchST(st float64) error {
	st = clampPitch(st)
	dir := a.narrateDir()
	if err := os.MkdirAll(dir, 0o755); err != nil {
		return err
	}
	if err := os.WriteFile(filepath.Join(dir, "pitch.txt"),
		[]byte(strconv.FormatFloat(st, 'f', 1, 64)), 0o644); err != nil {
		return err
	}
	a.voiceMu.Lock()
	a.pitchRead, a.pitchSel = true, st
	a.voiceMu.Unlock() // before shiftRef: it asks pitchST back
	os.Remove(a.refPath())
	if !exists(a.refBase()) {
		return nil
	}
	return a.shiftRef()
}

// shiftRef writes the file the server actually clones: the base moved by the
// slider. formant=preserved moves the pitch and leaves the vowels where a
// human's are, so the reference still sounds like someone worth cloning
// instead of like a sped-up tape -- the model reproduces artifacts as
// faithfully as it reproduces the voice.
func (a *App) shiftRef() error {
	if st := a.pitchST(); st != 0 {
		return a.runCmd(ffTool("ffmpeg"), "-v", "error", "-y", "-i", a.refBase(),
			"-af", fmt.Sprintf("rubberband=pitch=%.4f:formant=preserved:pitchq=quality",
				math.Pow(2, st/12)),
			"-ac", "1", "-c:a", "pcm_s16le", a.refPath())
	}
	// unshifted: the server still reads one fixed path, so hand it a copy
	// rather than teaching every caller which of the two files to use
	b, err := os.ReadFile(a.refBase())
	if err != nil {
		return err
	}
	return os.WriteFile(a.refPath(), b, 0o644)
}

// ensureVoiceRef makes sure the file the server clones from is on disk and
// matches the voice above: a base recording, moved by the pitch slider.
func (a *App) ensureVoiceRef() error {
	if err := a.ensureVoiceBase(); err != nil {
		return err
	}
	// on disk is not enough: it has to be one the server will read. shiftRef
	// overwrites, so a copy that fails goes back through it.
	if exists(a.refPath()) && wavPlain(a.refPath()) {
		return nil
	}
	return a.shiftRef()
}

// ensureVoiceBase builds the unshifted reference. For a picked voice that is a
// copy of the wav it names -- rebuilt here so a project folder carried to
// another machine heals itself. For one of the session's own voices it is cut
// from that recording's best stretches -- solo, clear of everyone else, and
// with the most actually said in them (refCuts, narrate_ref.go) -- taken from
// the ORIGINAL file for fidelity.
func (a *App) ensureVoiceBase() error {
	ref := a.refBase()
	// before the pitch slider there was one file and it was never shifted, so
	// an existing voice_ref.wav is exactly the base this now keeps apart
	if !exists(ref) && exists(a.refPath()) {
		if err := os.Rename(a.refPath(), ref); err != nil {
			return err
		}
	}
	if exists(ref) && wavPlain(ref) {
		return nil
	}
	// A reference is built once and then kept, which means a bad one is kept
	// too: every project that had a reference before refRate was pinned holds a
	// 192 kHz file the server refuses, and nothing would ever cut it again --
	// the voice would simply never speak. So what is on disk is judged by the
	// header rather than by existing, and one this build would not have written
	// is cut afresh over the top of it.
	if exists(ref) {
		a.logf("voice reference %s is not a wav the server reads -- cutting it again", filepath.Base(ref))
	}
	id := a.voiceID()
	slot := narratorSlot(id)
	if slot == 0 {
		src := filepath.Join(a.voicesDir(), id+".wav")
		if !exists(src) {
			return fmt.Errorf("voice %q is no longer in %s -- pick another at the top of this step", id, a.voicesDir())
		}
		os.MkdirAll(filepath.Dir(ref), 0o755)
		return a.levelRef(ref, "-i", src)
	}
	aud := a.narratorSource(slot)
	if aud == "" {
		return fmt.Errorf("nothing is tagged as narrator %d on the Prepare step", slot)
	}
	base := baseName(aud)
	// Hand-picked takes are the whole answer when there are any (narrate_take.go):
	// they were chosen by ear, on this recording, usually against a clone that
	// was wrong -- so nothing here re-ranks them, and the caps that keep a guess
	// honest do not apply. They also need no diarization, which is what lets a
	// recording nobody has run Prepare over still be cloned from.
	hand := a.takesFor(base)
	picks := handPicks(hand)
	if len(picks) == 0 {
		turns, err := loadSpans(filepath.Join(a.inputsDir(), base, "turns.json"))
		if err != nil {
			return fmt.Errorf("no diarization for %s -- run Prepare, or pick the seconds by hand under the video", base)
		}
		// the transcript of the same recording, on the same clock: what refCuts
		// weighs the stretches by. Missing is allowed and means unweighed.
		rows := loadSeg4(filepath.Join(a.inputsDir(), base, "transcript.tsv"))
		if picks = refCuts(turns, rows); len(picks) == 0 {
			return fmt.Errorf("no clean solo stretch found for the voice reference")
		}
	}
	dir := a.narrateDir()
	os.MkdirAll(dir, 0o755)
	// the pieces and the list that names them are scaffolding for one concat.
	// They were left in narrate/ afterwards, where a later build with fewer takes
	// leaves the extra ones lying beside the reference looking like part of it.
	var tmp []string
	defer func() {
		for _, f := range tmp {
			os.Remove(f)
		}
	}()
	var list strings.Builder
	total, words := 0.0, 0
	for i, t := range picks {
		if len(hand) == 0 && (total >= refWant || i >= refTakeMax) {
			break
		}
		d := t.dur()
		if len(hand) == 0 {
			d = math.Min(d, refWant-total)
		}
		f := filepath.Join(dir, fmt.Sprintf(".ref%d.wav", i))
		tmp = append(tmp, f)
		if err := a.runCmd(ffTool("ffmpeg"), "-v", "error", "-y",
			"-ss", fmt.Sprint(t.s), "-t", fmt.Sprint(d),
			"-i", aud, "-ac", "1", f); err != nil {
			return err
		}
		list.WriteString(concatLine(f))
		total, words = total+d, words+t.words
	}
	lf := filepath.Join(dir, ".ref.list")
	tmp = append(tmp, lf)
	if err := os.WriteFile(lf, []byte(list.String()), 0o644); err != nil {
		return err
	}
	if err := a.levelRef(ref, "-f", "concat", "-safe", "0", "-i", lf); err != nil {
		return err
	}
	// which of the two it was, because they fail differently: an automatic
	// reference that sounds wrong is a ranking to overrule by hand, and a
	// hand-picked one that sounds wrong is seconds to re-pick.
	how := fmt.Sprintf("%d words", words)
	if len(hand) > 0 {
		how = fmt.Sprintf("%d hand-picked take(s)", len(hand))
	}
	a.logfIdle(">>> voice reference built: %.1f s, %s from %s", total, how, base)
	return nil
}

// voiceKey is the voice as the caches see it. Pitch 0 and no takes keep the
// key older projects had, so nothing already spoken is orphaned. Both parts
// change who is speaking -- a reference cut from different seconds is a
// different clone (sampleNeedsSpeaking).
func (a *App) voiceKey() string {
	k := a.voiceID()
	if st := a.pitchST(); st != 0 {
		k = fmt.Sprintf("%s@%+.1f", k, st)
	}
	if h := takesKey(a.voiceTakes()); h != "" {
		k += "#" + h
	}
	return k
}

// sampleKey is what ▶ would speak if it were pressed now: which voice, shifted
// by how much, saying which words, on which draw. Everything the sample can be
// is in here, which is what lets the button tell "play that again" from "that
// is not what I asked for any more".
func (vp *voicePicker) sampleKey(text string) string {
	k := vp.a.voiceKey() + "|" + strings.TrimSpace(text)
	if vp.roll > 0 { // a sample nobody re-rolled keeps the key it already has
		k = strconv.Itoa(vp.roll) + "#" + k
	}
	return k
}

// sampleNeedsSpeaking is what ▶ does: resume the sample in the player, or make
// a new one. The player holds a file, not a question, and the file stops being
// the answer when the voice, pitch or words change -- resuming then played the
// old setting.
func sampleNeedsSpeaking(sounding bool, loaded, now string) bool {
	return !sounding || loaded != now
}

// sampleWav caches one voice's reading of one sample text, so hearing a voice
// again is instant and comparing two is a click each. id is the readable half
// of the name; key is what decides the file.
func (a *App) sampleWav(id, key string) string {
	h := sha1.Sum([]byte(key))
	return filepath.Join(a.narrateDir(), "samples", fmt.Sprintf("%s_%x.wav", id, h[:6]))
}

// ---- the page ---------------------------------------------------------------

// buildVoicePicker is the top of the Narrate step: who speaks. It was a step of
// its own, which put the choice of voice and the lines it would speak on
// different screens -- and made the run bar's ▶ mean "play a sample" here and
// "run this step" everywhere else. Sharing the page, the sample gets its own ▶
// and the run bar keeps one meaning.
//
// Two rows. This was a scrolling list, a filter box over it, two import
// buttons, an editable folder path and three explanatory labels, stacked in a
// divider -- a third of a column for a choice made once per project. Every one
// of those parts was answering a question the dropdown's own row text answers:
// the list is picked from and then closed, so it is a dropdown; the filter went
// with the list; and the "Voice: …" label read back what the closed dropdown is
// already showing. What is left is the choice, and the way to hear it.
func (a *App) buildVoicePicker() gtk.Widgetter {
	vp := &voicePicker{a: a}
	a.voicePick = vp
	if p, err := NewPlayer(); err == nil {
		vp.player = p
		// one player, two things that use it: the sample, and ▶ walking the
		// takes. A segment reaching its end arrives here, which is what moves
		// the walk on to the next take (chainOn).
		p.OnState = func() {
			vp.band.chainOn()
			a.updateRunControls()
		}
		p.OnError = a.playerErr("the voice sample")
	}
	vp.voices = a.listVoices()

	vp.names = gtk.NewStringList(voiceNames(vp.voices))
	vp.pick = gtk.NewDropDown(vp.names, nil)
	vp.pick.SetHExpand(true)
	// what the paragraph under this used to say, moved to where it is read at
	// the moment the choice is being made rather than on every look at the page
	vp.pick.SetTooltipText("Who speaks the narration. Every line is spoken by cloning the " +
		"selected recording; switching voices keeps what you already synthesized")
	// picking IS choosing: this is the step whose whole job is that choice, and
	// switching costs nothing now that the cache knows about voices.
	vp.pick.NotifyProperty("selected", func() {
		if vp.syncing {
			return
		}
		vp.choose(int(vp.pick.Selected()))
	})

	// one recording at a time. The folder import that stood beside this is gone
	// with the folder row: you hear a sample and want THAT speaker, and the
	// voices folder is set in llm.conf for the audio server anyway.
	add := flexButton("Add file…", "Copy one recording into the voices folder and use it")
	add.ConnectClicked(vp.addVoiceDialog)

	// ＋ － ▶ for the takes, on this row and not under the band: choosing the
	// voice and choosing which seconds of it to clone are one decision made in
	// one place, and a row of their own would put the pitch slider between
	// the buttons and the thing they act on.
	bandFrame, bandBtns := vp.buildTakeBand()

	who := gtk.NewBox(gtk.OrientationHorizontal, 6)
	who.Append(vp.pick)
	who.Append(bandBtns)
	who.Append(add)

	vp.sample = gtk.NewEntry()
	vp.sample.SetText(sampleDefault)
	vp.sample.SetHExpand(true)
	vp.sample.SetTooltipText("Enter — or ▶ beside it — speaks this in the selected voice")
	vp.sample.ConnectActivate(func() { vp.playSample() })

	// its own transport, both halves of it, because the run bar belongs to the
	// narration preview under it: one page, two things that can play, and a
	// shared button could only ever mean one of them. ▶ is the ⏸ for the sample
	// while it sounds, like every other play button in the app (syncPlayIcons),
	// and ⏹ is here rather than borrowed from the run bar for the same reason.
	vp.playBtn = gtk.NewButtonFromIconName("media-playback-start-symbolic")
	vp.playBtn.SetTooltipText("Speak the sample in the selected voice")
	vp.playBtn.ConnectClicked(vp.playClicked)
	vp.stopBtn = gtk.NewButtonFromIconName("media-playback-stop-symbolic")
	vp.stopBtn.SetTooltipText("Stop the sample")
	vp.stopBtn.SetSensitive(false)
	vp.stopBtn.ConnectClicked(vp.stopClicked)
	// ⟳ is the same ⟳ the narration's lines have (narrate.go), and means the
	// same thing: these words again, drawn again. A voice is judged on a
	// reading, and one bad reading of a good sentence is not the voice.
	vp.rollBtn = gtk.NewButtonFromIconName("view-refresh-symbolic")
	vp.rollBtn.SetTooltipText("Speak the sample again as a different take — same words, same voice, new draw")
	vp.rollBtn.ConnectClicked(vp.rollClicked)
	hear := gtk.NewBox(gtk.OrientationHorizontal, 4)
	hear.Append(vp.sample)
	hear.Append(vp.playBtn)
	hear.Append(vp.stopBtn)
	hear.Append(vp.rollBtn)

	// the reference is shifted, not the narration: this changes who is speaking
	// in the finished video, so it belongs next to the choice of voice rather
	// than next to the render
	vp.pitch = gtk.NewScaleWithRange(gtk.OrientationHorizontal, -pitchRange, pitchRange, 0.5)
	formSlider(vp.pitch, "Shift the reference recording before it is cloned — "+
		"a different speaker, not the same one transposed")
	// One unlabelled tick, at the recording's own pitch. It had three labels
	// under it -- deeper, as recorded, higher -- and a row of words under a
	// slider is a whole line of height for a legend nobody reads twice: which
	// way is deeper is what a slider means. The tick keeps the one position
	// worth finding again, the semitones over the handle say where it is, and
	// the tooltip says what it does.
	vp.pitch.AddMark(0, gtk.PosBottom, "")
	// dragging crosses two dozen stops; only the one it is let go on is worth
	// an ffmpeg pass, so each move cancels the pass the previous one queued
	vp.pitch.ConnectValueChanged(func() {
		if vp.syncing {
			return
		}
		vp.drag++
		gen, st := vp.drag, vp.pitch.Value()
		glib.TimeoutAdd(400, func() bool {
			if gen == vp.drag {
				vp.applyPitch(st)
			}
			return false
		})
	})
	pitchLbl := gtk.NewLabel("pitch:")
	pitchLbl.SetXAlign(0)

	// there used to be a second slider here that moved the finished narration
	// instead of the reference. It was free where this one costs a re-speak, but
	// it post-processed generated speech without preserving formants and stacked
	// with the tempo fit in produce -- two passes over exactly the lines already
	// struggling to fit their slot. One knob, applied where the quality is.
	knob := gtk.NewBox(gtk.OrientationHorizontal, 8)
	knob.Append(pitchLbl)
	knob.Append(vp.pitch)

	// The sentence takes what the pitch does not. It was half each --
	// homogeneous, so that neither side could be squeezed by the other's
	// content -- and that was written when the slider stretched to fill its
	// half. It does not any more (formSlider: 150 px, aligned left), so
	// "half each" spent a quarter of the row on nothing while the entry, which
	// is a line of text to be read, was cut off at "This is the voice the".
	tune := gtk.NewBox(gtk.OrientationHorizontal, 12)
	tune.Append(hear)
	tune.Append(knob)
	hear.SetHExpand(true)
	// homogeneous is about the WIDTH. Left alone the entry and its three
	// transport buttons also grow to the height of the pitch slider -- the
	// semitones are drawn over the handle -- and a play button two lines tall
	// reads as a mistake. Centered, they keep the size a button has everywhere
	// else, and the row is as tall as the slider and no taller.
	hear.SetVAlign(gtk.AlignCenter)
	knob.SetVAlign(gtk.AlignCenter)

	// no side margins of its own: the column already has them (buildNarrate),
	// and a second 12 px here indented this half of it past the transport
	// above -- two blocks in one column, starting at two different x.
	box := gtk.NewBox(gtk.OrientationVertical, 6)
	box.SetMarginTop(8) // clear of the handle above it (buildNarrate)
	box.SetMarginBottom(4)
	// Which voice, then the recording it is cut from, then the sentence it
	// will be speaking: the ＋ － ▶ sit on the dropdown's row, an arm's length
	// over the band they act on.
	box.Append(who)
	box.Append(bandFrame)
	box.Append(tune)

	vp.syncSelection()
	return box
}

// flexButton is a button whose label may be ellipsized, so a row of them keeps
// this column's minimum width small: an unellipsized label is a floor under the
// whole pane, and this pane is one of the parts meant to give way first.
func flexButton(text, tip string) *gtk.Button {
	l := gtk.NewLabel(text)
	l.SetEllipsize(pango.EllipsizeEnd)
	b := gtk.NewButton()
	b.SetChild(l)
	b.SetTooltipText(tip)
	return b
}

// voiceNames is what the dropdown reads, in the order listVoices offers. The
// file name and not a prettified version of it, for the reason listVoices gives:
// these are files that can be opened and replaced, and the row is now the only
// place the app says which one this project speaks in.
func voiceNames(vs []voiceOpt) []string {
	out := make([]string, len(vs))
	for i, v := range vs {
		out[i] = v.name
	}
	return out
}

// reload rebuilds the list from the folder. sel names the voice to leave
// selected; choose says whether landing on it counts as picking it (importing
// a file does; anything else must not -- re-picking a narrator slot deletes
// the reference cut). Never from inside notify::selected: splicing a
// dropdown's model while its popup closes leaves the view drawing a gone list
// (see showPromptStyle).
func (vp *voicePicker) reload(sel string, choose bool) {
	vp.syncing = true
	defer func() { vp.syncing = false }()
	defer vp.band.sync() // ...including on the paths that do not fall through to syncSelection
	vp.voices = vp.a.listVoices()
	vp.names.Splice(0, vp.names.NItems(), voiceNames(vp.voices))
	for i, v := range vp.voices {
		if v.id != sel {
			continue
		}
		vp.pick.SetSelected(uint(i))
		if choose {
			vp.syncing = false // ...so landing on it counts as picking it
			vp.choose(i)
		}
		return
	}
	vp.syncing = false
	vp.syncSelection()
}

// refreshNarrators rebuilds the list when the Inputs step changes -- the
// narrator rows ARE that step's tags, so tagging somebody as narrator 2 adds a
// row and untagging them takes it away, and the file each row clones is part of
// what it reads. Rebuilding rather than renaming in place, because the set of
// rows moves and not just their text; false, because none of this is a choice
// the user just made and re-choosing "own" would throw the reference away.
func (vp *voicePicker) refreshNarrators() {
	if vp.pick == nil {
		return
	}
	vp.reload(vp.a.voiceID(), false)
}

// addVoiceDialog picks a recording anywhere on disk and installs it. The
// conversion runs off the GUI thread, since it is ffmpeg.
func (vp *voicePicker) addVoiceDialog() {
	a := vp.a
	exts := make([]string, 0, len(mediaExt))
	for e := range mediaExt {
		exts = append(exts, e)
	}
	a.pickFile("Choose a voice sample", a.audDir, extFilter("Audio and video", exts...), func(src string) {
		a.setStatus("adding " + filepath.Base(src) + "…")
		go func() {
			v, err := a.importVoice(src)
			glib.IdleAdd(func() {
				if err != nil {
					a.logf("add voice: %v", err)
					a.setStatus("could not add that sample — see log")
					return
				}
				vp.reload(v.id, true)
			})
		}()
	})
}

// syncSelection points the dropdown at whatever the output folder says the
// voice is. The guard matters: without it, showing the stored voice would count
// as choosing it, and choosing a narrator slot re-cuts the reference from the
// recording -- so every start, and every switch of output folder, would quietly
// throw the existing one away.
func (vp *voicePicker) syncSelection() {
	vp.syncing = true
	defer func() { vp.syncing = false }()
	// which recording the band draws is decided by the same stored voice this
	// is reading back, so it settles here and not at each of the five callers
	defer vp.band.sync()
	if vp.pitch != nil {
		vp.pitch.SetValue(vp.a.pitchST())
	}
	want := vp.a.voiceID()
	for i, v := range vp.voices {
		if v.id == want {
			vp.pick.SetSelected(uint(i))
			return
		}
	}
	// the voice this project speaks in is not on offer. Leave the dropdown
	// showing nothing rather than landing on whatever is first -- a picker
	// silently pointing at a voice nobody chose is how the wrong speaker gets
	// into a finished video -- and say which of the two ways it happened, since
	// they are fixed in different places.
	vp.pick.SetSelected(gtk.InvalidListPosition)
	if n := narratorSlot(want); n > 0 {
		vp.a.setStatus(fmt.Sprintf(
			"narrator %d is not tagged on the Prepare step — tag a recording, or pick another voice", n))
		return
	}
	vp.a.setStatus(fmt.Sprintf("voice %q is no longer in %s — pick another", want, vp.a.voicesDir()))
}

func (vp *voicePicker) current() (voiceOpt, bool) {
	// Selected is unsigned and reads InvalidListPosition when nothing is
	// picked, which is the state syncSelection leaves behind when the stored
	// voice is missing -- so the upper bound is the real check here.
	i := int(vp.pick.Selected())
	if i < 0 || i >= len(vp.voices) {
		return voiceOpt{}, false
	}
	return vp.voices[i], true
}

// choose installs the voice off the GUI thread -- it shells out to ffmpeg -- and
// reports either way, since a voice that silently failed to install would only
// show up as the wrong speaker in the finished video.
func (vp *voicePicker) choose(i int) {
	if i < 0 || i >= len(vp.voices) {
		return
	}
	v := vp.voices[i]
	a := vp.a
	go func() {
		if err := a.setVoice(v); err != nil {
			a.logfIdle("voice: %v", err)
			glib.IdleAdd(func() { a.setStatus("could not install that voice — see log") })
			return
		}
		glib.IdleAdd(func() {
			// a different voice is a different recording under the band, or
			// none at all
			vp.band.sync()
			if v.id == captionsVoice {
				a.setStatus("no audio — the narration is written and timed, never spoken")
				return
			}
			if n := narratorSlot(v.id); n > 0 {
				who := fmt.Sprintf("narrator %d's", n)
				a.setStatus("voice: " + who + " — it is re-cut from the recording on the next line spoken")
				return
			}
			a.setStatus(fmt.Sprintf("voice: %s — ▶ beside the sample plays it", v.name))
		})
	}()
}

// applyPitch commits the slider off the GUI thread -- it reruns ffmpeg over the
// reference -- and says what it means, since the change is only audible after
// the next synthesis.
func (vp *voicePicker) applyPitch(st float64) {
	a := vp.a
	go func() {
		if err := a.setPitchST(st); err != nil {
			a.logfIdle("pitch: %v", err)
			glib.IdleAdd(func() { a.setStatus("could not shift the reference — see log") })
			return
		}
		glib.IdleAdd(func() {
			if st == 0 {
				a.setStatus("voice as recorded — ▶ beside the sample plays it")
				return
			}
			a.setStatus(fmt.Sprintf("voice shifted %+.1f semitones — ▶ beside the sample plays it", st))
		})
	}()
}

// The sample's own two buttons drive it through these. The run bar does not:
// auditioning a voice must not take ▶ away from the step's own run, which is what
// the page is for (pageTransport in pipeline.go).
func (vp *voicePicker) playing() bool { return vp.player != nil && vp.player.Playing() }
func (vp *voicePicker) cued() bool    { return vp.player != nil && vp.player.Cued() }

func (vp *voicePicker) toggle() {
	if vp.player != nil {
		vp.player.Toggle()
	}
}

func (vp *voicePicker) stop() {
	if vp.player != nil {
		vp.player.Stop()
	}
}

// playClicked is the button beside the sample: speak it, then pause and resume
// that same sample. A second click on a ▶ that is already sounding meant
// "synthesize and start over" before, which on a cold server is a ten second
// wait for a sentence that was already playing.
func (vp *voicePicker) playClicked() {
	if !sampleNeedsSpeaking(vp.playing() || vp.cued(), vp.spoken, vp.sampleKey(vp.sample.Text())) {
		vp.toggle()
		// this branch used to be the one silent button on the page: a click on
		// a sounding ⏸ changed the icon and said nothing, which is
		// indistinguishable from a click that did not register
		if vp.playing() {
			vp.a.setStatus("sample playing")
		} else {
			vp.a.setStatus("sample paused — ▶ resumes, ⏹ starts over")
		}
		vp.a.updateRunControls()
		return
	}
	vp.playSample()
}

// stopClicked ends the sample: the position and the file both, so the next ▶
// starts over rather than resuming. It no longer has to be the way out of a
// sample you have since edited -- ▶ notices that by itself (sampleNeedsSpeaking).
func (vp *voicePicker) stopClicked() {
	vp.stop()
	vp.a.setStatus("sample stopped")
	vp.a.updateRunControls()
}

// rollClicked asks for another reading of the same words. The seed is fixed to
// the key (ttsSeed), which is what stops a sample changing under you between
// two clicks -- and what leaves you stuck with a take that got the words right
// and the delivery wrong. Bumping the roll moves the key, so the old wav is
// not in the way and the play below is a first play.
func (vp *voicePicker) rollClicked() {
	if vp.busy {
		vp.a.setStatus("still synthesizing the last sample…")
		return
	}
	vp.roll++
	vp.stop() // the old take is in the player; it is not the one being asked for
	vp.playSample()
}

// playSample speaks the sample text in the selected voice. The first one after
// a cold start also loads the model, which is why the button says what it is
// doing rather than just going quiet for ten seconds.
func (vp *voicePicker) playSample() {
	a := vp.a
	if vp.busy {
		a.setStatus("still synthesizing the last sample…")
		return
	}
	v, ok := vp.current()
	if !ok {
		a.setStatus("pick a voice first")
		return
	}
	if v.id == captionsVoice {
		a.setStatus("no audio is chosen — there is no voice to sample")
		return
	}
	text := strings.TrimSpace(vp.sample.Text())
	if text == "" {
		a.setStatus("type a sample sentence to hear the voice")
		return
	}
	vp.busy = true
	a.setStatus("synthesizing the sample…")
	// The log is the only account of this that survives the next click, and a
	// sample is the one thing on the page with no output file to inspect
	// afterwards -- so it says which voice, at what pitch, in what words,
	// before anything can go wrong.
	take := ""
	if vp.roll > 0 {
		take = fmt.Sprintf(" take %d", vp.roll+1)
	}
	a.logf(">>> sample%s: %s at %+.1f semitones — %q", take, v.name, vp.pitch.Value(), text)
	a.snapSources() // ensureVoiceRef reads the narrator's recording
	key := vp.sampleKey(text)
	go func() {
		// keyed on the pitch and the re-rolls too, or those controls would
		// appear to do nothing: the first sample would answer for every
		// setting after it
		out := a.sampleWav(a.voiceKey(), key)
		cached := exists(out)
		t0 := time.Now()
		var err error
		if !cached {
			// the sample is keyed by voice and words, so seed it the same way:
			// replaying a sample must be the take you just heard, not a new one
			err = a.speak(text, "", ttsSeed(out), out)
		}
		took := time.Since(t0)
		glib.IdleAdd(func() {
			vp.busy = false
			if err != nil {
				a.logf("sample FAILED: %v", err)
				a.setStatus("could not speak the sample — see log")
				return
			}
			// what actually came back. A server that answers with nothing, or
			// with a header and no audio, is the failure this page cannot show
			// you: the player takes the file, reports itself playing, and the
			// room stays quiet.
			var size int64
			if fi, e := os.Stat(out); e == nil {
				size = fi.Size()
			}
			if size < 1024 {
				a.logf("!!! sample: %s is %d bytes — no audio came back", filepath.Base(out), size)
				a.setStatus("the sample came back empty — see log")
				return
			}
			if cached {
				a.logf("    sample: %s (%d kB, spoken earlier)", filepath.Base(out), size>>10)
			} else {
				a.logf("    sample: %s (%d kB, spoken in %s)", filepath.Base(out), size>>10, took.Round(time.Second/10))
			}
			if vp.player == nil {
				// no GStreamer, so nothing will play and nothing will say why
				a.logf("!!! sample: there is no player — %s was written but cannot be heard", out)
				a.setStatus("no player available — see log")
				return
			}
			vp.player.PlaySegment(out, 0, -1, true)
			vp.spoken = key // what ▶ is now holding, for the next press to judge
			a.setStatus(fmt.Sprintf("sample in %s", v.name))
		})
	}()
}

// Which seconds a voice reference is cut from: the takes with the most SPEECH
// in them, by the transcript's words, narrowed to the first and last word.
// Diarization alone finds where somebody held the floor, which can be a laugh
// and a long think. With no transcript the fallback is longest turns first.

const (
	// refMinLen is the shortest take worth having. Under about five seconds
	// there is not enough of a voice in it to be one.
	refMinLen = 5.0
	// refPad is how far another speaker must stay from a take on either side.
	// Diarization edges are approximate, and a syllable of the wrong person
	// bleeding into the reference is a syllable the clone learns.
	refPad = 2.0
	// refWant is how much reference the model is given, and refTakeMax how
	// many pieces it may be assembled from. More than this stops helping;
	// fewer, longer pieces are steadier than many short ones.
	refWant    = 14.0
	refTakeMax = 3
	// refMinRate is how much speech a take has to hold to count as speech:
	// words per second. Ordinary talk runs near 2.5, so this admits somebody
	// speaking slowly or leaving pauses and turns away the stretch that is
	// mostly not talking -- including the credits ASR invents over silence,
	// which are a handful of words spread over a long quiet.
	refMinRate = 1.5
)

// refTake is one stretch worth cloning: the seconds it covers, and how much
// was said in them.
type refTake struct {
	s, e  float64
	words int
}

func (t refTake) dur() float64 { return t.e - t.s }

// domSpeaker is whoever did most of the talking, by total time. A voice
// recording is one person plus whatever the room leaked into it, so this is
// the person, and everyone else is the leak.
func domSpeaker(turns []span) string {
	by := map[string]float64{}
	for _, t := range turns {
		by[t.slot] += t.e - t.s
	}
	dom, best := "", 0.0
	for s, d := range by {
		if d > best {
			dom, best = s, d
		}
	}
	return dom
}

// refCuts ranks the stretches of a recording a reference should be cut from,
// best first. turns is the diarization, rows the transcript of the same
// recording on the same clock; rows may be empty.
func refCuts(turns []span, rows []seg4) []refTake {
	dom := domSpeaker(turns)
	if dom == "" {
		return nil
	}
	var cand []refTake
	for _, t := range turns {
		if t.slot != dom || !soloTurn(turns, t, dom) {
			continue
		}
		// no length test on the turn itself: spokenIn only ever narrows it,
		// so the take being long enough says the turn was
		if c := spokenIn(t, rows); c.dur() >= refMinLen {
			cand = append(cand, c)
		}
	}
	// The takes somebody is speaking through. With no transcript none of them
	// qualifies and the whole set stays, ranked below by length alone -- and
	// the same happens for a speaker so slow that nothing clears the bar,
	// which is a thin reference but a better one than none.
	var spoke []refTake
	for _, c := range cand {
		if float64(c.words) >= refMinRate*c.dur() {
			spoke = append(spoke, c)
		}
	}
	if len(spoke) > 0 {
		cand = spoke
	}
	// most said first, and length breaks the tie the no-transcript case makes
	// of every comparison
	sort.SliceStable(cand, func(i, j int) bool {
		if cand[i].words != cand[j].words {
			return cand[i].words > cand[j].words
		}
		return cand[i].dur() > cand[j].dur()
	})
	return cand
}

// soloTurn is whether a turn has the recording to itself, refPad included.
func soloTurn(turns []span, t span, dom string) bool {
	for _, o := range turns {
		if o.slot != dom && o.e > t.s-refPad && o.s < t.e+refPad {
			return false
		}
	}
	return true
}

// spokenIn narrows a turn to the words inside it and counts them. A row counts
// when its middle is in the turn: diarization edges are approximate, and a row
// across the boundary belongs to the side holding most of it. Speakers are not
// checked -- the turn is already this speaker's alone.
func spokenIn(t span, rows []seg4) refTake {
	out := refTake{s: t.s, e: t.e}
	first, last := 0.0, 0.0
	for _, r := range rows {
		if mid := (r.s + r.e) / 2; mid < t.s || mid >= t.e {
			continue
		}
		// a row with no words in it is not the end of the speech: letting it
		// through would stretch the take out over the silence it stands for
		n := len(strings.Fields(r.text))
		if n == 0 {
			continue
		}
		// clamped, because a row counted by its middle can still begin before
		// the turn or end after it, and the seconds outside are the pad
		if out.words == 0 {
			first = max(r.s, t.s)
		}
		last = min(r.e, t.e)
		out.words += n
	}
	if out.words > 0 {
		out.s, out.e = first, last
	}
	return out
}
