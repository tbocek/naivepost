package main

// One written line into a spoken wav: the page's only path off this machine.
// The wav is named after everything that went into it (ttsKey), so the same
// line comes back the same and a take on disk is the answer; rerolling changes
// the seed. The emotion tag (emoTerm..emoOpts) maps a word, or two with
// weights, onto the server's eight-axis vector, forgiving: an unknown tag is
// spoken plainly rather than refused.

import (
	"bytes"
	"crypto/sha1"
	"encoding/binary"
	"encoding/json"
	"fmt"
	"io"
	"math"
	"net/http"
	"os"
	"path/filepath"
	"strconv"
	"strings"

	"github.com/diamondburned/gotk4/pkg/glib/v2"
)

//
// The server itself lives in audiocpp.go: it does the listening for Prepare too,
// and the parts below are only what speaking adds to it.

// ttsModelID is the model the narration is spoken through: the id set in
// Settings (default index-tts2), verified once per session against the
// server's catalog -- a guessed or stale id would otherwise come back as an
// error that reads like a broken model.
func (a *App) ttsModelID() (string, error) {
	if a.ttsModel != "" {
		return a.ttsModel, nil
	}
	c := a.readConf()
	cat, err := audioCatalog(a.audioURL(), c.TTSKey)
	if err != nil {
		return "", err
	}
	want := c.TTSModel
	m, ok := cat[want]
	if !ok {
		return "", fmt.Errorf("the audio.cpp server at %s serves %s, but not %q -- "+
			"the TTS model set in Settings", a.audioURL(), catalogIDs(cat), want)
	}
	// only a cloning model: that server also serves the ASR and diarization
	// models, and either would take the job and fail at it
	if m.Task != "" && m.Task != "clon" && m.Family != "index_tts2" {
		return "", fmt.Errorf("%q on %s is declared task %q -- it cannot clone a voice",
			want, a.audioURL(), m.Task)
	}
	a.ttsModel = want
	return want, nil
}

// ttsKey names one take: everything that decides how this line comes back.
// The voice, pitch included, is part of it -- changing either must not serve
// the old speaker from cache, and must not throw the old speaker's lines away
// either; switch back and they are all still there. So is the emotion blend
// (emoAlpha): a stronger blend is a different performance of the same words,
// and serving the tamer one would make the constant a lie. So is Roll, which
// is the user saying "same line, different draw".
func (a *App) ttsKey(e narrEntry) string {
	// A weighted tag is keyed by the mix it resolves to, not its spelling: the
	// mapping moves ("excited" changed), and "angry=1" and "furious=1" are one
	// take. Unweighted tags keep their old key -- they go to the judge as words.
	emo := e.Emotion
	if vec, ok := emoVector(emo); ok {
		emo = vec
	}
	// the default voice keeps the key it had before the voice picker existed, so a project
	// narrated back then does not re-speak every line the first time it opens
	key := e.Text + "|" + emo
	if v := a.voiceKey(); v != ownVoice {
		key = v + "|" + key
	}
	if e.Roll > 0 { // a line never re-rolled keeps the key it already has
		key = strconv.Itoa(e.Roll) + "#" + key
	}
	// "25e" says which performer and what reached it: 2.5 weights, with the
	// emotion actually delivered. Everything before this prefix is either the
	// 2.0 voice or a flat read from the era when the request carried the
	// emotion in a field the server ignored -- neither may be served as the
	// current take.
	return "25e" + emoAlpha + "|" + key
}

// ttsWav is where that take lives.
func (a *App) ttsWav(e narrEntry) string {
	h := sha1.Sum([]byte(a.ttsKey(e)))
	return filepath.Join(a.narrateDir(), "tts", fmt.Sprintf("%x.wav", h[:8]))
}

// ttsSeed is the take's random seed, cut from the same digest as its filename:
// one filename, one seed, and only what moves the filename (words, emotion,
// voice, re-roll) moves it. Left alone, the engine draws a fresh seed per
// request.
func ttsSeed(key string) uint32 {
	h := sha1.Sum([]byte(key))
	return binary.BigEndian.Uint32(h[8:12])
}

// synthesize speaks one entry through the resident server into the cache.
func (a *App) synthesize(e narrEntry) error {
	k := a.ttsKey(e)
	return a.speak(e.Text, e.Emotion, ttsSeed(k), a.ttsWav(e))
}

// The eight bases the engine mixes, in the order it wants them. Everything a
// line can ask for is a point in this space; the names below are only the doors
// into it. The kin lists are deliberately generous -- someone writing "furious"
// or "deadpan" means one of these eight, and refusing them would send the line
// down the slower path for a spelling.
var emoBases = [8][]string{
	{"happy", "happiness", "joy", "joyful", "cheerful", "glad", "delighted", "upbeat", "pleased"},
	{"angry", "anger", "mad", "furious", "fury", "rage", "annoyed", "irritated", "indignant"},
	{"sad", "sadness", "unhappy", "sorrow", "sorrowful", "hurt", "crying", "tearful"},
	{"afraid", "fear", "fearful", "scared", "frightened", "terrified", "nervous", "anxious", "panicked"},
	{"disgusted", "disgust", "revolted", "repulsed", "grossed"},
	{"melancholic", "melancholy", "low", "gloomy", "down", "depressed", "depression", "wistful", "weary"},
	{"surprised", "surprise", "shocked", "amazed", "astonished", "startled", "stunned"},
	{"natural", "neutral", "calm", "deadpan", "flat", "plain", "even", "relaxed", "matter-of-fact"},
}

// emoTerm resolves one word to its base, -1 if it is none of them.
func emoTerm(w string) int {
	w = strings.ToLower(strings.TrimSpace(w))
	for i, kin := range emoBases {
		for _, k := range kin {
			if w == k {
				return i
			}
		}
	}
	return -1
}

// The blends: mixes over the eight bases that no single base reaches --
// "excited" is happy plus surprise, not a kin of happy. Recipes, not synonyms;
// every recipe peaks at 1 so a weight means the same as on a base. Nothing
// outside the eight axes can be invented (smug, sarcastic, whispered stay on
// the judge's side).
var emoBlends = []struct {
	Kin []string
	V   [8]float64 // happy, angry, sad, afraid, disgusted, melancholic, surprised, calm
}{
	{[]string{"excited", "thrilled", "exhilarated", "eager", "enthusiastic"}, [8]float64{0: 1, 6: 0.55}},
	{[]string{"ecstatic", "overjoyed", "elated", "jubilant"}, [8]float64{0: 1, 6: 0.8}},
	{[]string{"playful", "amused", "teasing", "cheeky"}, [8]float64{0: 1, 6: 0.3, 7: 0.35}},
	{[]string{"proud", "triumphant", "victorious"}, [8]float64{0: 1, 7: 0.4}},
	{[]string{"relieved", "reassured"}, [8]float64{0: 0.6, 7: 1}},
	{[]string{"hopeful", "encouraging", "optimistic"}, [8]float64{0: 0.7, 7: 1}},
	{[]string{"tender", "warm", "affectionate", "gentle", "fond"}, [8]float64{0: 0.5, 7: 1}},
	{[]string{"nostalgic", "bittersweet", "reminiscent"}, [8]float64{0: 0.4, 5: 1}},
	{[]string{"solemn", "grave", "reverent", "serious"}, [8]float64{1: 0.3, 5: 0.45, 7: 1}},
	{[]string{"awed", "awestruck", "breathless", "wonder"}, [8]float64{0: 0.5, 3: 0.25, 6: 1}},
	{[]string{"alarmed", "urgent", "frantic"}, [8]float64{3: 0.85, 6: 1}},
	{[]string{"horrified", "appalled", "aghast"}, [8]float64{3: 1, 4: 0.7, 6: 0.5}},
	{[]string{"desperate", "pleading", "begging"}, [8]float64{2: 0.7, 3: 1}},
	{[]string{"confused", "puzzled", "baffled", "bewildered", "perplexed"}, [8]float64{3: 0.3, 6: 1, 7: 0.4}},
	{[]string{"frustrated", "exasperated"}, [8]float64{1: 1, 4: 0.35, 5: 0.5}},
	{[]string{"bitter", "resentful", "sullen"}, [8]float64{1: 0.7, 4: 0.4, 5: 1}},
	{[]string{"contemptuous", "scornful", "sneering", "dismissive"}, [8]float64{1: 0.6, 4: 1}},
	{[]string{"dismayed", "crestfallen", "disappointed"}, [8]float64{2: 1, 6: 0.5}},
	{[]string{"heartbroken", "devastated", "grief", "anguished"}, [8]float64{2: 1, 5: 0.8}},
	{[]string{"ominous", "foreboding", "menacing"}, [8]float64{1: 0.3, 3: 0.5, 7: 1}},
	{[]string{"tense", "suspenseful", "wary"}, [8]float64{3: 0.7, 6: 0.35, 7: 1}},
}

// emoBlend resolves one word to its recipe, ok false if it names none.
func emoBlend(w string) ([8]float64, bool) {
	w = strings.ToLower(strings.TrimSpace(w))
	for _, b := range emoBlends {
		for _, k := range b.Kin {
			if w == k {
				return b.V, true
			}
		}
	}
	return [8]float64{}, false
}

// emoWeights splits a tag into its name=weight pairs. weighted says whether any
// weight was actually written -- "[angry]" and "[angry=1]" name the same point,
// but only the second one asked for it, and only the second one skips the judge.
func emoWeights(tag string) (parts []struct {
	Name string
	W    float64
}, weighted bool) {
	for _, f := range strings.Split(tag, ",") {
		f = strings.TrimSpace(f)
		if f == "" {
			continue
		}
		name, w := f, 1.0
		if i := strings.IndexByte(f, '='); i >= 0 {
			v, err := strconv.ParseFloat(strings.TrimSpace(f[i+1:]), 64)
			if err != nil || math.IsNaN(v) || math.IsInf(v, 0) {
				return nil, false // a weight that is not a number is not a weight
			}
			name, w, weighted = strings.TrimSpace(f[:i]), math.Max(0, math.Min(1, v)), true
		}
		parts = append(parts, struct {
			Name string
			W    float64
		}{name, w})
	}
	return parts, weighted
}

// emoVector turns a weighted tag into the eight floats the engine takes
// directly. emotion_text goes through a small judge model (a guess, diluted by
// every non-emotion word); a vector is exact and repeatable. Weights are the
// opt-in: a bare "[angry]" still goes through the judge. Bases, kin and blends
// reach a vector; anything else falls back to the text path.
func emoVector(tag string) (string, bool) {
	parts, weighted := emoWeights(tag)
	if !weighted || len(parts) == 0 {
		return "", false
	}
	var v [8]float64
	sum := 0.0
	for _, p := range parts {
		if i := emoTerm(p.Name); i >= 0 {
			v[i] = math.Max(v[i], p.W) // named twice: the louder ask wins
			sum += p.W
			continue
		}
		// not one of the eight, but possibly a mix of them: a blend spends its
		// weight across several axes at once, scaled so "=0.5" is the same
		// recipe read half as hard
		mix, ok := emoBlend(p.Name)
		if !ok {
			return "", false
		}
		for i, f := range mix {
			v[i] = math.Max(v[i], f*p.W)
			sum += f * p.W
		}
	}
	if sum <= 0 {
		return "", false // "[angry=0]" asks for nothing; let the judge read the line
	}
	out := make([]string, 8)
	for i, f := range v {
		// three decimals: a blend scaled by a weight lands on numbers like
		// 0.16499999999999998, and this string is both what the engine reads
		// and part of the take's name (ttsKey) -- neither wants the tail
		out[i] = strconv.FormatFloat(math.Round(f*1000)/1000, 'g', -1, 64)
	}
	return strings.Join(out, ","), true
}

// emoText is what the judge sees: the same tag with the weights taken off, so a
// tag that named an axis this client does not know ("[wistful=1, smug=0.3]")
// still arrives as words it can score.
func emoText(tag string) string {
	parts, weighted := emoWeights(tag)
	if !weighted {
		return tag
	}
	names := make([]string, 0, len(parts))
	for _, p := range parts {
		names = append(names, p.Name)
	}
	return strings.Join(names, ", ")
}

// emoOpts is the request's options block. The emotion belongs HERE: the speech
// endpoint drops unknown top-level fields (a top-level "emotion" was silently
// ignored for a long time). emotion_text needs use_emotion_text; emotion_vector
// needs no switch; the engine tests use_emotion_text FIRST, so exactly one
// branch is sent.
func emoOpts(emotion string) map[string]any {
	opts := map[string]any{"emotion_alpha": emoAlpha}
	tag := strings.TrimSpace(emotion)
	if tag == "" {
		return opts
	}
	if vec, ok := emoVector(tag); ok {
		// alpha is a multiplier over the weights, not a separate dial: the
		// engine scales every value by it and fills the remainder with the
		// speaker's own reading. At 0.85 a written "angry=1" would arrive as
		// 0.85 anger and 0.15 of whatever the sample sounds like, which is the
		// one thing an exact weight is written to prevent. So the weighted path
		// sends 1 and lets the number mean itself; "=0.85" is then the same
		// intensity a plain word gets.
		opts["emotion_vector"] = vec
		opts["emotion_alpha"] = "1"
		return opts
	}
	opts["use_emotion_text"] = "true"
	opts["emotion_text"] = emoText(tag)
	return opts
}

// speak is the one call that reaches the model: text in, wav at out. The voice
// sample at the top of this page and the narration lines below it go through it
// identically, so a sample is a true preview -- same server, same reference,
// same settings.
func (a *App) speak(text, emotion string, seed uint32, out string) error {
	if err := a.ensureVoiceRef(); err != nil {
		return err
	}
	if err := a.ensureAudioServer(); err != nil {
		return err
	}
	model, err := a.ttsModelID()
	if err != nil {
		return err
	}
	opts := emoOpts(emotion)
	opts["seed"] = strconv.FormatUint(uint64(seed), 10)
	// the reference goes up like any other file the server has to open -- it is
	// the same wav on every line, but see serverFile on why it is not remembered
	ref, err := a.serverFile(a.refPath())
	if err != nil {
		return err
	}
	body, _ := json.Marshal(map[string]any{
		"model":     model,
		"input":     text,
		"voice_ref": ref,
		"language":  "en",
		"options":   opts,
	})
	req, err := http.NewRequest("POST", a.audioURL()+"/v1/audio/speech", bytes.NewReader(body))
	if err != nil {
		return err
	}
	req.Header.Set("Content-Type", "application/json")
	bearer(req, a.readConf().TTSKey)
	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		return err
	}
	defer resp.Body.Close()
	data, _ := io.ReadAll(resp.Body)
	if resp.StatusCode != 200 || len(data) < 1000 {
		return fmt.Errorf("tts (%s): %.300s", resp.Status, string(data))
	}
	os.MkdirAll(filepath.Dir(out), 0o755)
	return os.WriteFile(out, data, 0o644)
}

// syncSpeakIcons draws the per-line buttons: the running row shows ⏸, every
// other ▶. Running is three states: the voice sounding on that row; the row
// auditioning with its picture rolling before any sound (the audio pipeline
// reports playing only after preroll); or the audition waiting for a line
// never spoken.
func (n *narrator) syncSpeakIcons() {
	live := n.livePlayRow()
	n.liveRow = live
	rolling := n.soloPic && n.player != nil && n.player.Playing()
	for i, r := range n.rows {
		// a clip with no line plays too, on its own audio -- so its ▶ says that
		// rather than promising a line that is not there
		tip := "play this clip with its line spoken over it"
		if i < len(n.entries) && strings.TrimSpace(n.entries[i].Text) == "" {
			tip = "play this clip — it has no line, so you hear the game"
		}
		setPlayIcon(r.speak, i == live || (i == n.solo && (rolling || n.synthing)), tip, "pause this clip")
	}
}

// livePlayRow is the row the ⏸ belongs on: the row under the playhead while
// the picture rolls, not the row whose wav is on the voice player -- the two
// differ between lines, where the ⏸ used to stick. A wordless row counts,
// since it plays like any other (speakEntry).
func (n *narrator) livePlayRow() int {
	i := -1
	switch {
	case n.player != nil && n.player.Playing():
		i = n.nearestEntry(n.pos)
	case n.voice != nil && n.voice.Playing():
		i = n.speaking // a line spoken over a still frame: no picture to follow
	}
	if !n.has(i) {
		return -1
	}
	return i
}

// rerollEntry asks for another take of a line whose words are right: it moves
// the seed (and so the key) and lets the audition speak it through the normal
// path. The old take stays on disk -- a re-roll can come back worse or fail.
func (a *App) rerollEntry(i int) {
	n := a.narr
	n.pullRows()
	if !n.has(i) {
		return
	}
	if strings.TrimSpace(n.entries[i].Text) == "" {
		a.setStatus(fmt.Sprintf("clip %d has no line to re-roll", i+1))
		return
	}
	if a.captionsOnly() {
		a.setStatus("no audio is chosen — a caption has no take to re-roll")
		return
	}
	n.entries[i].Roll++
	n.save()
	n.rebuildRows() // the end time is an estimate again until the new take exists
	a.setStatus(fmt.Sprintf("line %d: new take, speaking it", i+1))
	a.speakEntry(i)
}

// pausePress is whether pressing row i's button means "pause it" -- the same
// question syncSpeakIcons draws the face from, asked in one place. Only the
// picture, not the voice: a line over a still frame is voice.Toggle's.
func (n *narrator) pausePress(i int) bool {
	return n.player != nil && n.player.Playing() && n.livePlayRow() == i
}

// speakEntry auditions one line: the clip it was written for, played with the
// line spoken over it, and stopping at the end of that clip. A line read over a
// frozen frame is not what it was written against -- half of judging a line is
// whether it lands on what is happening -- and this button used to give you
// exactly that. Clicked again it pauses, and again resumes, the picture and the
// voice together. Another line's button interrupts it, which is what a list of
// lines to audition wants: one click, not stop-then-play.
func (a *App) speakEntry(i int) {
	n := a.narr
	n.pullRows()
	if !n.has(i) {
		return
	}
	// The ⏸ a row is showing means that row, whatever is written on it. Only
	// the wordless branch below ever asked, so pressing ⏸ on a LINE that was
	// playing fell through to the audition at the bottom and re-cued it from
	// its lead-in -- which starts the picture again -- and it took a second
	// press, with solo now set, to actually pause. Ahead of every branch
	// because every kind of row can be the one under the playhead: a line, a
	// caption, a clip with nothing written on it.
	if n.pausePress(i) {
		n.player.Pause()
		if n.voice != nil {
			n.voice.Pause()
		}
		n.syncSpeakIcons()
		a.updateRunControls()
		return
	}
	e := n.entries[i]
	if strings.TrimSpace(e.Text) == "" {
		// A clip the narration left alone is still a clip of the video, and this
		// button is "play from here" first: the picture rolls from the top of the
		// clip and the preview carries on down the cut. No synthesis (saying nothing
		// costs a call) and no solo -- this is a seek into the cut that plays.
		if ed := a.ed; ed != nil && n.player != nil && ed.videoAt(e.S) != nil {
			n.claimVoice()
			n.playSeg, n.jumped = -1, -1
			n.cue(e.S, true)
			n.selectRow(i)
			n.syncSpeakIcons()
			a.updateRunControls()
			a.setStatus(fmt.Sprintf("clip %d has no line — playing it on its own audio", i+1))
			return
		}
		a.setStatus(fmt.Sprintf("clip %d has no line, and no recording covers it", i+1))
		return
	}
	if a.captionsOnly() {
		// no voice to audition: the ▶ is a seek to the line's moment, and the
		// caption itself is judged where it is burned in -- Produce
		if ed := a.ed; ed != nil && n.player != nil && ed.videoAt(e.S) != nil {
			n.claimVoice()
			n.playSeg, n.jumped = -1, -1
			n.cue(n.leadIn(i), true)
			n.selectRow(i)
			n.syncSpeakIcons()
			a.updateRunControls()
			a.setStatus(fmt.Sprintf("line %d is a caption — read, never spoken; playing its moment", i+1))
			return
		}
		a.setStatus(fmt.Sprintf("line %d is a caption, and no recording covers its clip", i+1))
		return
	}
	// a failed line stays mute in the ticking preview so one bad request does
	// not stall every pass -- but this ▶ is the user asking for THIS line, and
	// that is an explicit retry
	delete(n.synthFail, a.ttsWav(e))
	// this line has never been spoken and the server is on it: with the picture
	// the video is stopped waiting, without it there is nothing to play yet.
	// Either way a second click can only start the same synthesis twice.
	if n.solo == i && n.synthing {
		a.setStatus(fmt.Sprintf("still speaking line %d for the first time — ⏹ gives up on it", i+1))
		return
	}
	if n.solo == i && n.soloPic && n.player != nil {
		if n.player.Playing() {
			n.player.Pause()
			if n.voice != nil {
				n.voice.Pause()
			}
		} else {
			// playSeg back to -1, or the tick would see a line it has already
			// played and run the rest of the clip mute
			n.playSeg = -1
			n.player.Toggle()
		}
		n.syncSpeakIcons()
		a.updateRunControls()
		return
	}
	// spoken over a still frame, and clicked again: pause, resume, or -- if it
	// ran to its end -- play it again, all of which Toggle knows how to do
	if n.voice != nil && n.solo == i && !n.soloPic && (n.voice.Playing() || n.voice.Cued()) {
		n.voice.Toggle()
		n.syncSpeakIcons()
		return
	}
	n.claimVoice() // whatever else was sounding, this button takes over from it
	// A seek into the preview, landing just ahead of this line. The tick does the
	// speaking from here: it is the one thing that knows where the picture has
	// got to, and it already knows how to wait for a line that has never been
	// spoken (holdForSynth). What it no longer does is stop or hop when the line
	// is over -- past its own line this is simply the preview, playing the cut.
	if ed := a.ed; ed != nil && n.player != nil && ed.videoAt(e.S) != nil {
		n.playSeg, n.jumped = -1, -1
		n.solo, n.soloPic = i, true
		// a few seconds ahead of the line, not the head of the clip: the line
		// may sit a minute in ("at"), and a ▶ that answers with silence for a
		// minute reads as a line that is not there. The run-in is what the
		// audition is FOR -- seeing the moment the line was placed on arrive --
		// and leadIn is where it may start without waking the line above
		n.cue(n.leadIn(i), true)
		n.selectRow(i) // the ▶ is inside the row but does not select it on its own
		a.updateRunControls()
		return
	}
	a.speakAlone(i, e)
}

// speakAlone is the audition for a clip no recording covers -- a source not on
// this machine, or one taken off the Inputs step since the cut was made. The
// line is spoken over whatever the picture is showing, because that is all
// there is, and this path does its own synthesis: with the picture rolling that
// is the tick's job, and it stops the video to wait rather than speaking late.
func (a *App) speakAlone(i int, e narrEntry) {
	n := a.narr
	// claimed before the request goes out, not after it comes back: the row is
	// this button's from the click, and the wait for a first synthesis is the
	// part of it the user is most likely to press again
	n.speaking, n.solo, n.soloPic, n.synthing = i, i, false, true
	n.selectRow(i)
	n.syncSpeakIcons()
	a.snapSources()
	go func() {
		wav := a.ttsWav(e)
		if !exists(wav) {
			glib.IdleAdd(func() { a.setStatus("synthesizing… (first line after a cold start also loads the model)") })
			if err := a.synthesize(e); err != nil {
				a.logfIdle("synthesis failed: %v", err)
				glib.IdleAdd(func() {
					n.synthing, n.solo = false, -1
					n.syncSpeakIcons()
					a.setStatus("synthesis failed — see log")
				})
				return
			}
		}
		glib.IdleAdd(func() {
			n.synthing = false
			a.setStatus(fmt.Sprintf("entry %d — no recording covers this clip, so the line plays on its own", i+1))
			if n.voice != nil {
				n.voice.PlaySegment(wav, 0, -1, true)
			}
			n.syncSpeakIcons()
		})
	}()
}
