package main

// Splitting the voice off a source: the wish on the row, the two files it
// becomes, and what the server has to have before either can happen.
//
// Nothing here reaches a separation model. What one sends back is an envelope
// of base64 wavs, and a fake server can put tones in that envelope as easily as
// a GPU can put stems in it -- which leaves the parts that would otherwise
// break quietly checkable on this machine: which stem is taken for the voice,
// what the row becomes afterwards, and whether the picture survives being
// given a new soundtrack.

import (
	"encoding/base64"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"net/http/httptest"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"strings"
	"testing"
)

// ---- the wish ---------------------------------------------------------------

// The control is a flag and nothing else. A click must not start minutes of
// somebody else's GPU from this page, and a second click must take the wish
// back -- which is the whole reason it is a toggle and not a verb.
func TestTheSplitButtonRecordsAWishAndTakesItBack(t *testing.T) {
	dir, paths := mkSources(t, "cap.mkv", "mic.flac")
	s := &sourceList{}
	s.add(paths...)
	before, _ := os.ReadDir(dir)

	if got := s.sepVoiceWanted(); len(got) != 0 {
		t.Errorf("a fresh list already wants %v split", got)
	}
	s.setSepVoice(0, true)
	if got := s.sepVoiceWanted(); len(got) != 1 || got[0] != paths[0] {
		t.Errorf("after one click the list wants %v, want [%s]", got, paths[0])
	}
	// the wish costs nothing until ▶: nothing was read, written or made
	if after, _ := os.ReadDir(dir); len(after) != len(before) {
		t.Errorf("flagging a row left %d files in the folder, was %d", len(after), len(before))
	}
	s.setSepVoice(0, false)
	if got := s.sepVoiceWanted(); len(got) != 0 {
		t.Errorf("a second click left %v flagged", got)
	}
	// a row can be removed while its handler is in flight, so an index that no
	// longer names one has to be a no-op rather than a panic
	s.setSepVoice(2, true)
	s.setSepVoice(-1, true)
	if got := s.sepVoiceWanted(); len(got) != 0 {
		t.Errorf("an out-of-range click flagged %v", got)
	}
	// a plain recording is as splittable as a screen capture -- someone talking
	// over a game is one file either way, and only one of the two has a picture
	s.setSepVoice(1, true)
	if got := s.sepVoiceWanted(); len(got) != 1 || got[0] != paths[1] {
		t.Errorf("flagging the recording gave %v, want [%s]", got, paths[1])
	}
}

// The toggle sits where the row was asked for it -- after the name, before the
// trash -- and its tooltip carries the key to the whole row. An icon-only
// button in a row of icon-only buttons is unreadable without one, and the
// legend that used to say so was a permanent line of the page a hand's width
// away from the buttons it was about (srcRowKey).
func TestTheSplitToggleSitsBeforeTheTrashAndSaysWhatTheRowIs(t *testing.T) {
	body := funcBody(t, "sources.go", `func \(s \*sourceList\) row\(i int\) \*gtk\.Box \{`)
	sep, del := strings.Index(body, "row.Append(sep)"), strings.Index(body, "row.Append(del)")
	if sep < 0 || del < 0 {
		t.Fatal("the row does not append both the split toggle and the trash")
	}
	if sep > del {
		t.Error("the split toggle is appended after the trash, so it is drawn right of it")
	}
	// a toggle, not a button: a row waiting to be split has to say so without
	// being hovered, because the work happens on a press somewhere else
	if !strings.Contains(body, "gtk.NewToggleButton()") ||
		!strings.Contains(body, "sep.SetActive(it.sepVoice)") {
		t.Error("the split control does not show the row's flag as its own state")
	}
	if !strings.Contains(body, "s.setSepVoice(i, sep.Active())") {
		t.Error("toggling the split control does not reach setSepVoice")
	}
	if !strings.Contains(body, "sep.SetTooltipText(") || !strings.Contains(body, "+ srcRowKey)") {
		t.Error("the scissors say neither what they do nor what the row's other symbols are")
	}
	// ...and the legend it replaced is off the page: four icons drawn once,
	// permanently, to answer a question that is asked at the button
	if p := readSrc(t, "prep.go"); strings.Contains(p, `{"edit-cut-symbolic", "split voice off"}`) ||
		strings.Contains(p, "legend.Append(") {
		t.Error("the legend is back above the source list")
	}
	// every symbol on the row carries the key, not just this one: whichever
	// one the hand is over is the one that has to answer
	if n := len(regexp.MustCompile(`\+ ?srcRowKey[,)]`).FindAllString(readSrc(t, "sources.go"), -1)); n != 4 {
		t.Errorf("%d of the row's four symbols name the others, want all of them", n)
	}
	// ...and the key says what each one DOES, which is what the legend it
	// replaced said: four bare names is a list of words to guess at
	// ...each opening with the symbol it is about, so a line pairs with a
	// button at a glance instead of by counting positions
	for _, want := range []string{"🎥 footage — ", "🎤 narrator — ",
		"✂ split the voice off — ", "🗑 remove — "} {
		if !strings.Contains(srcRowKey, want) {
			t.Errorf("the row key never explains %q", want)
		}
	}
}

// What a granted wish does to the list: the row that asked keeps its place and
// its picture role, the voice follows it carrying the narrator tag, and the
// wish is gone -- a wish that survived being granted would be granted again on
// the next ▶, splitting a file that no longer has a voice in it.
func TestASplitRowBecomesTwoRowsThatKeepItsRoles(t *testing.T) {
	items := []sourceItem{
		{path: "/s/cam2.mkv", footage: true},
		{path: "/s/cap.mkv", footage: true, narrator: 2, sepVoice: true},
		{path: "/s/mic.flac", narrator: 1},
	}
	got := sepApply(items, []sepResult{
		{src: "/s/cap.mkv", rest: "/o/cap.split-novoice.mkv", voice: "/o/cap.split-voice.wav"}})
	want := []sourceItem{
		{path: "/s/cam2.mkv", footage: true},
		{path: "/o/cap.split-novoice.mkv", footage: true},
		{path: "/o/cap.split-voice.wav", narrator: 2},
		{path: "/s/mic.flac", narrator: 1},
	}
	if len(got) != len(want) {
		t.Fatalf("%d rows became %d, want %d: %+v", len(items), len(got), len(want), got)
	}
	for i := range want {
		if !sameSource(got[i], want[i]) {
			t.Errorf("row %d is %+v, want %+v", i, got[i], want[i])
		}
	}
	for _, it := range got {
		if it.sepVoice {
			t.Errorf("%s still asks to be split after having been", filepath.Base(it.path))
		}
	}
	// the tag has to move, not be copied: two rows claiming narrator 2 is the
	// state loading rejects, and Narrate would clone the voice from the half
	// that no longer has one
	if got[1].narrator != 0 {
		t.Errorf("the voiceless half kept the narrator tag: %+v", got[1])
	}
	// nothing asked for is nothing changed
	same := sepApply(items, nil)
	if len(same) != len(items) {
		t.Fatalf("with nothing split the list went from %d rows to %d", len(items), len(same))
	}
	for i := range items {
		if !sameSource(same[i], items[i]) {
			t.Errorf("row %d changed with nothing split: %+v, was %+v", i, same[i], items[i])
		}
	}
}

// The halves keep the original's name, timestamp and all. That stamp is how
// every source is placed on the session clock -- a split recording has to land
// exactly where the whole one did, or its two halves drift apart from each
// other and from every other camera.
func TestTheTwoHalvesKeepTheNameTheSessionClockReads(t *testing.T) {
	const src = "/rec/Kooha-2026-08-30-17-10-36.mp4"
	rest, voice := sepNames("/out/stems", src)
	if filepath.Ext(rest) != ".mkv" {
		t.Errorf("the voiceless half of a video is %s -- it has a picture to carry", rest)
	}
	if filepath.Ext(voice) != ".wav" {
		t.Errorf("the voice came out as %s, want a wav", voice)
	}
	for _, p := range []string{rest, voice} {
		if filepath.Dir(p) != "/out/stems" {
			t.Errorf("%s was not written beside the other stems", p)
		}
		if _, ok := nameStamp(filepath.Base(p)); !ok {
			t.Errorf("%s has lost the timestamp the sources line up on", filepath.Base(p))
		}
	}
	// a recording has no picture to keep, so its rest is a plain wav
	r2, v2 := sepNames("/out/stems", "/rec/mic-2026-08-30-17-10-36.flac")
	if filepath.Ext(r2) != ".wav" {
		t.Errorf("the rest of a recording came out as %s, want a wav", r2)
	}
	// two sources in one folder must not write over each other
	for _, pair := range [][2]string{{rest, voice}, {rest, r2}, {voice, v2}} {
		if pair[0] == pair[1] {
			t.Errorf("two halves share the name %s", pair[0])
		}
	}
}

// ---- reading the answer -----------------------------------------------------

// Which stem is the voice, and which of them add back up to the recording
// without it. Two families answer differently and both are right, so this is
// the model's business to say and ours to read -- not the caller's to know
// which model it happened to ask.
func TestTheVoiceIsFoundWhicheverFamilyAnswered(t *testing.T) {
	for _, c := range []struct {
		what  string
		ids   []string
		voice string
		rest  []string
	}{
		{"a roformer separates into the pair", []string{"vocals", "instrumental"},
			"vocals", []string{"instrumental"}},
		{"the order it lists them in is not ours to assume", []string{"instrumental", "Vocals"},
			"Vocals", []string{"instrumental"}},
		{"htdemucs separates into four instruments", []string{"drums", "bass", "other", "vocals"},
			"vocals", []string{"drums", "bass", "other"}},
		{"a model that calls it the voice means the same thing", []string{"voice", "background"},
			"voice", []string{"background"}},
		// a model offering both the sum and the parts it is the sum of: adding
		// all four back together would count the drums and the bass twice
		{"the sum wins over the parts it is made of", []string{"vocals", "drums", "instrumental", "bass"},
			"vocals", []string{"instrumental"}},
	} {
		v, rest, err := sepPick(c.ids)
		if err != nil {
			t.Errorf("%s: %v", c.what, err)
			continue
		}
		if v != c.voice {
			t.Errorf("%s: took %q for the voice, want %q", c.what, v, c.voice)
		}
		if strings.Join(rest, ",") != strings.Join(c.rest, ",") {
			t.Errorf("%s: the rest is %v, want %v", c.what, rest, c.rest)
		}
	}
	// a model that gave no voice is not doing this step's work, and saying so
	// here beats writing a silent narration track and finding out at the render
	_, _, err := sepPick([]string{"drums", "bass"})
	if err == nil {
		t.Fatal("stems with no voice among them passed")
	}
	for _, want := range []string{"drums", "bass"} {
		if !strings.Contains(err.Error(), want) {
			t.Errorf("the error does not say what did come back (%q): %v", want, err)
		}
	}
	if _, _, err := sepPick([]string{"vocals"}); err == nil {
		t.Error("a lone voice passed -- the recording without it is the other half")
	}
}

// The envelope itself. An answer that is not one has to be an error: the next
// thing that happens to it is base64-decoding into a file, and an empty file
// is a silent lane nobody notices until the render.
func TestAnAnswerWithNoStemsIsAnErrorAndNotASilentFile(t *testing.T) {
	stems, err := sepStems([]byte(`{"named_audio_outputs":[
		{"id":"vocals","audio":"AAA=","sample_rate":44100,"channels":2},
		{"id":"instrumental","audio":"BBB=","sample_rate":44100,"channels":2}]}`))
	if err != nil {
		t.Fatalf("a good answer was rejected: %v", err)
	}
	if len(stems) != 2 || stems[0].ID != "vocals" || stems[1].Audio != "BBB=" {
		t.Errorf("the answer was read as %+v", stems)
	}
	if _, err := sepStems([]byte(`{"named_audio_outputs":[]}`)); err == nil {
		t.Error("an answer with no stems in it passed")
	}
	if _, err := sepStems([]byte(`{"error":"no such model"}`)); err == nil {
		t.Error("an answer that is not a separation at all passed")
	} else if !strings.Contains(err.Error(), "no such model") {
		t.Errorf("the error hides what the server actually said: %v", err)
	}
	if _, err := sepStems([]byte("<html>502</html>")); err == nil {
		t.Error("an answer that is not JSON passed")
	}
}

// ---- what the run reads -----------------------------------------------------

// The runner asks the snapshot, never the widget: it is on a goroutine, and the
// list under it can be edited while it works. Granting the wishes rewrites that
// same snapshot, so the phases after the split see the two files and not the
// one that no longer describes the session.
func TestTheRunReadsTheWishesOffItsOwnSnapshot(t *testing.T) {
	a := &App{root: t.TempDir()}
	a.snapItems([]sourceItem{
		{path: "/s/cap.mkv", footage: true, narrator: 2, sepVoice: true},
		{path: "/s/mic.flac", narrator: 1},
	})
	if got := a.sepWanted(); len(got) != 1 || got[0] != "/s/cap.mkv" {
		t.Fatalf("the run sees %v to split, want [/s/cap.mkv]", got)
	}
	a.snapItems(sepApply(a.snappedItems(), []sepResult{
		{src: "/s/cap.mkv", rest: "/o/cap.split-novoice.mkv", voice: "/o/cap.split-voice.wav"}}))

	if got := a.sepWanted(); len(got) != 0 {
		t.Errorf("the wish survived being granted: %v", got)
	}
	vids, auds := a.snappedSources()
	if len(vids) != 1 || vids[0] != "/o/cap.split-novoice.mkv" {
		t.Errorf("the frames would be taken from %v, want the voiceless half", vids)
	}
	if len(auds) != 2 || auds[0] != "/o/cap.split-voice.wav" {
		t.Errorf("the transcript would be of %v, want the voice among them", auds)
	}
	// the voice is who that recording was, so Narrate's slot 2 has to follow it
	if got := a.narratorPath(2); got != "/o/cap.split-voice.wav" {
		t.Errorf("narrator 2 is now %q, want the voice that was split off", got)
	}
}

// The wish belongs to the project: it is set on one day and spent on another,
// and a session reopened in between has to still be asking for it.
func TestTheWishIsSavedWithTheProject(t *testing.T) {
	a := &App{root: "/home/x/naivepost"}
	stored := []ProjectSource{
		{Path: "input_video/cap.mkv", Footage: true, Narrator: 2, SepVoice: true},
		{Path: "rec/mic.flac", Narrator: 1},
	}
	raw, err := json.Marshal(Project{Sources: stored})
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(raw), `"sepvoice":true`) {
		t.Errorf("the wish is not in the written project: %s", raw)
	}
	// omitempty: a row that never asked must not grow a key in every project
	if strings.Count(string(raw), "sepvoice") != 1 {
		t.Errorf("a row with no wish still wrote one: %s", raw)
	}
	var back Project
	if err := json.Unmarshal(raw, &back); err != nil {
		t.Fatal(err)
	}
	items := a.projectSources(back)
	if len(items) != 2 {
		t.Fatalf("%d sources came back, want 2: %+v", len(items), items)
	}
	if !items[0].sepVoice {
		t.Errorf("the reopened session has forgotten the wish: %+v", items[0])
	}
	if items[1].sepVoice {
		t.Errorf("a row that never asked came back asking: %+v", items[1])
	}
	// and the writer puts it there -- the round trip above is only half of it
	// if what the app saves is a different struct than what the test built
	if !strings.Contains(readSrc(t, "project.go"), "SepVoice: it.sepVoice") {
		t.Error("saving a project does not write the row's split flag")
	}
}

// The order inside ▶. Splitting changes what the rest of Prepare is OF -- the
// frames come out of the voiceless half and the transcript is of the voice --
// so it has to run before the frames and before the transcripts, and the
// server's models have to be checked before any of it.
func TestPrepareSplitsBeforeItLooksAtTheFiles(t *testing.T) {
	body := funcBody(t, "prep.go",
		`func \(a \*App\) prepare\(videos, audios \[\]string, interval float64, scaleName, scaleVF string\) \(bool, error\) \{`)
	// named without the receiver, so this pin is not itself mistaken for a test
	// that reaches a server -- which is what those two names mean anywhere else
	var at []int
	for _, step := range []string{"ensureAudioModels(", "separateVoices(", "ingest(", "understand("} {
		i := strings.Index(body, step)
		if i < 0 {
			t.Fatalf("prepare never calls %s", step)
		}
		at = append(at, i)
	}
	for i := 1; i < len(at); i++ {
		if at[i] < at[i-1] {
			t.Errorf("prepare runs its steps out of order: %v", at)
		}
	}
	// the models are checked knowing whether a split was asked for, because the
	// separation model is the one an ordinary stack is least likely to have
	if !strings.Contains(body, "sep := len(a.sepWanted()) > 0") ||
		!strings.Contains(body, "ensureAudioModels(sep)") {
		t.Error("the preflight is not told whether anything asked to be split")
	}
	// the separation reassigns what ingest and understand are given: a split
	// that produced two files the rest of the run never sees is a wasted GPU
	if !strings.Contains(body, "videos, audios, err = a.separateVoices(videos, audios)") {
		t.Error("the phases after the split still work on the unsplit lists")
	}
	// and ▶ is what runs it: the flag on the row starts nothing by itself
	if !strings.Contains(readSrc(t, "prep.go"), "a.prepare(") {
		t.Error("nothing in Prepare's run reaches prepare()")
	}
	if strings.Contains(readSrc(t, "sources.go"), "separateVoices") {
		t.Error("the source row can start a separation on its own -- ▶ is what spends minutes")
	}
}

// ---- settings ---------------------------------------------------------------

// The Test button beside the box, aimed at the failure this feature is most
// likely to hit: no ordinary audio.cpp stack ships a separation model, so the
// first thing anyone flagging a row will run into is a server that does not
// have one -- and finding that out here beats finding it out after ▶.
func TestSettingsCanCheckTheSplitModelIsInstalled(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path == "/health" {
			w.Write([]byte(`{"status":"ok"}`))
			return
		}
		w.Write([]byte(`{"object":"list","data":[
			{"id":"nemotron-asr","family":"nemotron_asr","task":"asr"},
			{"id":"bs-roformer","family":"bs_roformer","task":"sep"}]}`))
	}))
	defer srv.Close()

	got, err := testAudioModel(srv.URL, "", defSepModel, "sep", "split a voice off a recording")
	if err != nil {
		t.Fatalf("an installed separation model was rejected: %v", err)
	}
	if !strings.Contains(got, defSepModel) {
		t.Errorf("the report does not name the model it proved: %q", got)
	}
	// a stack that has never installed one, which is every stack until it does
	if _, err := testAudioModel(srv.URL, "", "mel-band-roformer", "sep",
		"split a voice off a recording"); err == nil {
		t.Error("a model the server does not serve passed its test")
	} else if !strings.Contains(err.Error(), "bs-roformer") {
		t.Errorf("the miss does not list what the server does have: %v", err)
	}
	// the id is there but is a transcriber -- a copied json entry
	if _, err := testAudioModel(srv.URL, "", "nemotron-asr", "sep",
		"split a voice off a recording"); err == nil {
		t.Error("an ASR model passed as a separation model")
	}

	// and the dialog's button is wired to exactly that call, on the id in its
	// own box rather than on the one saved before it was edited
	src := readSrc(t, "setup.go")
	for _, want := range []string{
		`hook(testSepBtn, sepBadge, "separation"`,
		`or(sepModel.Text(), defSepModel)`,
		`testAudioModel(url, k, id, "sep", "split a voice off a recording")`,
		`{"Voice split model:", sepModel, testSepBtn, sepBadge},`,
		`strings.TrimSpace(sepModel.Text())`,
	} {
		if !strings.Contains(src, want) {
			t.Errorf("setup.go does not contain %q", want)
		}
	}
}

// ---- the split itself -------------------------------------------------------

// sepFake is an audio.cpp server whose separations are wavs the test made. The
// envelope is the real one -- named_audio_outputs, base64 -- and the model
// behind it is a tone generator, which is enough to tell a stem that went to
// the right file from one that did not.
func sepFake(t *testing.T, stems func() []sepStem) (*App, *sepCalls) {
	t.Helper()
	var c sepCalls
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		switch r.URL.Path {
		case "/health":
			w.Write([]byte(`{"status":"ok"}`))
		case "/v1/ui/upload":
			name := r.Header.Get("X-Audiocpp-Filename")
			n, peak := wavScan(t, r.Body)
			c.up, c.names, c.peaks = append(c.up, n), append(c.names, name), append(c.peaks, peak)
			fmt.Fprintf(w, `{"path":%q,"bytes":%d}`, "/srv/up/"+name, n)
		case "/v1/tasks/run":
			var body struct {
				Model   string         `json:"model"`
				Request map[string]any `json:"request"`
			}
			if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
				t.Errorf("unreadable request: %v", err)
			}
			// the separation models declare no options at all, so the request
			// is the uploaded path and nothing else -- a field the server does
			// not know is a rejection, not something it ignores
			if _, ok := body.Request["audio"].(string); !ok {
				t.Errorf("the request does not name an uploaded file: %v", body.Request)
			}
			if len(body.Request) != 1 {
				t.Errorf("the request carries more than the audio: %v", body.Request)
			}
			c.runs++
			out, err := json.Marshal(map[string]any{"named_audio_outputs": stems()})
			if err != nil {
				t.Error(err)
			}
			w.Write(out)
		default:
			t.Errorf("unexpected request to %s", r.URL.Path)
			w.WriteHeader(404)
		}
	}))
	t.Cleanup(srv.Close)

	dir := t.TempDir()
	a := &App{root: dir, outDir: dir, curCmds: map[*exec.Cmd]bool{}}
	if err := a.writeConf(appConf{TTS: srv.URL}); err != nil {
		t.Fatal(err)
	}
	return a, &c
}

// sepCalls is what the fake was asked for: one entry per file that went up, and
// a count of the separations run on them.
type sepCalls struct {
	runs  int
	up    []int64  // bytes of each upload, in the order they arrived
	names []string // and the names they went up under
	peaks []int    // and how loud each of them was
}

// wavSecs is how long an uploaded pcm_s16le wav is, from its size alone -- the
// chunker's boundaries are measurable without decoding anything.
func wavSecs(n int64) float64 { return float64(n-44) / (44100 * 2 * 2) }

// wavScan measures an upload as it arrives: how many bytes, and the loudest
// sample among them. Which stretch of a recording a chunk actually holds is
// not a question its size can answer -- two chunks of the same length can be
// the same audio twice -- and its loudness can.
func wavScan(t *testing.T, r io.Reader) (int64, int) {
	t.Helper()
	var total int64
	peak, buf := 0, make([]byte, 64<<10)
	for {
		n, err := r.Read(buf)
		for i := 0; i+1 < n; i += 2 {
			// past any header the file may carry, so RIFF itself is not the peak
			if total+int64(i) < 4096 {
				continue
			}
			v := int(int16(uint16(buf[i]) | uint16(buf[i+1])<<8))
			if v < 0 {
				v = -v
			}
			if v > peak {
				peak = v
			}
		}
		total += int64(n)
		if err != nil {
			if err != io.EOF {
				t.Errorf("reading an upload: %v", err)
			}
			return total, peak
		}
	}
}

// sepTone writes a stereo wav of one frequency, at what the separation models
// work in -- which is also the shape a stem comes back in.
func sepTone(t *testing.T, path string, hz int, dur float64) string {
	t.Helper()
	in := fmt.Sprintf("sine=frequency=%d:sample_rate=%s", hz, sepRate)
	if hz == 0 {
		in = "anullsrc=r=" + sepRate + ":cl=stereo"
	}
	mustFFmpeg(t, "-f", "lavfi", "-t", fmt.Sprint(dur), "-i", in,
		"-ac", sepChannels, "-c:a", "pcm_s16le", path)
	return path
}

// sepStemOf is one file as the server would send it.
func sepStemOf(t *testing.T, id, path string) sepStem {
	t.Helper()
	b, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	return sepStem{ID: id, Audio: base64.StdEncoding.EncodeToString(b)}
}

// vidSize is what a container says its picture is, or "" when it has none.
func vidSize(t *testing.T, path string) string {
	t.Helper()
	out, err := exec.Command("ffprobe", "-v", "error", "-select_streams", "v:0",
		"-show_entries", "stream=width,height", "-of", "csv=p=0", path).Output()
	if err != nil {
		t.Fatalf("probing %s: %v", filepath.Base(path), err)
	}
	return strings.TrimSpace(string(out))
}

// vidHash is the picture itself, as the bytes of the encoded stream. Two files
// whose pictures hash the same hold the same frames encoded the same way --
// which is what "copied, not re-encoded" means and the only way to see it.
func vidHash(t *testing.T, path string) string {
	t.Helper()
	out, err := exec.Command("ffmpeg", "-v", "error", "-i", path,
		"-map", "0:v:0", "-c", "copy", "-f", "md5", "-").Output()
	if err != nil {
		t.Fatalf("hashing the picture of %s: %v", filepath.Base(path), err)
	}
	return strings.TrimSpace(string(out))
}

// One recording, end to end: the two halves land on disk, the right stem is in
// each of them, and the picture comes through untouched. The last part is what
// -c:v copy is for -- this file exists only to carry a different soundtrack,
// and re-compressing the frames to give it one would cost a generation of
// quality for nothing.
func TestSplittingARecordingWritesBothHalvesAndLeavesThePictureAlone(t *testing.T) {
	if _, err := exec.LookPath("ffmpeg"); err != nil {
		t.Skip("no ffmpeg on this machine")
	}
	stems := t.TempDir()
	// what the model "hears" apart: a low voice and a high everything-else, so
	// which one ended up where is a measurement rather than a guess
	voiceStem := sepTone(t, filepath.Join(stems, "v.wav"), 300, 3)
	restStem := sepTone(t, filepath.Join(stems, "r.wav"), 6000, 3)
	a, calls := sepFake(t, func() []sepStem {
		return []sepStem{sepStemOf(t, "vocals", voiceStem), sepStemOf(t, "instrumental", restStem)}
	})

	src := filepath.Join(a.root, "Kooha-2026-08-30-17-10-36.mkv")
	mustFFmpeg(t, "-f", "lavfi", "-t", "3", "-i", "testsrc=size=160x120:rate=15",
		"-f", "lavfi", "-t", "3", "-i", "sine=frequency=1000:sample_rate=44100",
		"-c:v", "libx264", "-pix_fmt", "yuv420p", "-c:a", "pcm_s16le", "-shortest", src)

	res, err := a.separateOne(src, 0, 1)
	if err != nil {
		t.Fatalf("splitting: %v", err)
	}
	if calls.runs != 1 {
		t.Errorf("a 3 s recording took %d requests, want one", calls.runs)
	}
	wantRest, wantVoice := sepNames(a.sepDir(), src)
	if res.rest != wantRest || res.voice != wantVoice || res.src != src {
		t.Fatalf("the split reports %+v, want rest %s voice %s", res, wantRest, wantVoice)
	}
	for _, p := range []string{res.rest, res.voice} {
		if !exists(p) {
			t.Fatalf("%s was not written", p)
		}
		if d, err := ffprobeDur(p); err != nil || d < 2.5 || d > 3.5 {
			t.Errorf("%s is %.2f s long (%v), want the recording's 3", filepath.Base(p), d, err)
		}
	}
	// the halves are not swapped: the voice is the low tone and the rest is the
	// high one, and the original's own 1 kHz is in neither
	if v := highBand(t, res.voice, 0.5, 2); v > -60 {
		t.Errorf("the voice half is %.1f dB above 2 kHz -- it has the wrong stem in it", v)
	}
	if r := highBand(t, res.rest, 0.5, 2); r < -30 {
		t.Errorf("the voiceless half is %.1f dB above 2 kHz -- it has the wrong stem in it", r)
	}
	// the picture came through, and came through untouched: this file exists
	// only to carry a different soundtrack, and paying a generation of quality
	// for that would be paying it for nothing
	if got := vidSize(t, res.rest); got != "160,120" {
		t.Errorf("the voiceless half's picture is %q, want the recording's 160,120", got)
	}
	if got, want := vidHash(t, res.rest), vidHash(t, src); got != want {
		t.Errorf("the picture was re-encoded: %s, was %s", got, want)
	}
	// the scratch is swept once both halves exist, and only then
	if work := filepath.Join(a.sepDir(), baseName(src)+".work"); exists(work) {
		t.Errorf("%s was left behind", filepath.Base(work))
	}

	// a re-run after a stop picks up where it stopped: both halves on disk is
	// the work already done, and asking the GPU again for it would be minutes
	// spent producing the file that is already there
	again, err := a.separateOne(src, 0, 1)
	if err != nil {
		t.Fatalf("re-running: %v", err)
	}
	if again != res {
		t.Errorf("the resume reports %+v, want the same as %+v", again, res)
	}
	if calls.runs != 1 {
		t.Errorf("the resume asked the server %d times in all, want the first only", calls.runs)
	}
}

// A four-instrument model hands back everything separately, and everything that
// is not the voice IS the recording without it. Summed at their own level: they
// were one recording a moment ago, and putting them back has to give that
// recording again, not a third of it.
func TestAFourStemAnswerIsPutBackTogetherAtItsOwnLoudness(t *testing.T) {
	if _, err := exec.LookPath("ffmpeg"); err != nil {
		t.Skip("no ffmpeg on this machine")
	}
	stems := t.TempDir()
	loud := sepTone(t, filepath.Join(stems, "drums.wav"), 6000, 2)
	a, _ := sepFake(t, func() []sepStem {
		return []sepStem{
			sepStemOf(t, "drums", loud),
			sepStemOf(t, "bass", sepTone(t, filepath.Join(stems, "bass.wav"), 0, 2)),
			sepStemOf(t, "other", sepTone(t, filepath.Join(stems, "other.wav"), 0, 2)),
			sepStemOf(t, "vocals", sepTone(t, filepath.Join(stems, "vocals.wav"), 300, 2)),
		}
	})
	work := t.TempDir()
	part := sepTone(t, filepath.Join(work, "mix.wav"), 440, 2)

	voice, rest, err := a.sepChunk(part, work, 0)
	if err != nil {
		t.Fatalf("separating a chunk: %v", err)
	}
	if v := highBand(t, voice, 0.3, 1.4); v > -60 {
		t.Errorf("the voice is %.1f dB above 2 kHz -- it is not the vocals stem", v)
	}
	// the three that are not the voice, at the level they arrived: dropping to
	// a third would be ffmpeg's amix averaging, which is what normalize=0 is for
	one, sum := highBand(t, loud, 0.3, 1.4), highBand(t, rest, 0.3, 1.4)
	if diff := sum - one; diff < -1 || diff > 1 {
		t.Errorf("the three stems summed to %.1f dB, but the loud one alone is %.1f dB "+
			"-- putting them back changed the recording's level", sum, one)
	}
}

// The chunks go back together in the order they were cut, and one chunk is the
// whole thing and is simply moved -- a rename beats a decode.
func TestTheChunksAreJoinedBackInOrder(t *testing.T) {
	if _, err := exec.LookPath("ffmpeg"); err != nil {
		t.Skip("no ffmpeg on this machine")
	}
	dir := t.TempDir()
	a := &App{root: dir, outDir: dir, curCmds: map[*exec.Cmd]bool{}}
	lo := sepTone(t, filepath.Join(dir, "c00.wav"), 300, 1)
	hi := sepTone(t, filepath.Join(dir, "c01.wav"), 6000, 1)

	joined := filepath.Join(dir, "both.wav")
	if err := a.sepJoin([]string{lo, hi}, joined); err != nil {
		t.Fatalf("joining: %v", err)
	}
	if d, err := ffprobeDur(joined); err != nil || d < 1.8 || d > 2.2 {
		t.Fatalf("two one-second chunks joined to %.2f s (%v)", d, err)
	}
	if first := highBand(t, joined, 0.1, 0.7); first > -60 {
		t.Errorf("the joined file starts %.1f dB above 2 kHz -- the chunks are out of order", first)
	}
	if second := highBand(t, joined, 1.2, 0.7); second < -30 {
		t.Errorf("the joined file's second half is %.1f dB above 2 kHz -- the chunks are out of order", second)
	}

	only := sepTone(t, filepath.Join(dir, "solo.wav"), 300, 1)
	one := filepath.Join(dir, "one.wav")
	if err := a.sepJoin([]string{only}, one); err != nil {
		t.Fatalf("joining one chunk: %v", err)
	}
	if !exists(one) || exists(only) {
		t.Error("a single chunk was not simply moved into place")
	}
}

// A recording longer than one request is cut into pieces, and the pieces have
// to be the recording: cut at the right offsets, sent all of them, and joined
// back in order. The sizes are the proof -- an uploaded pcm wav is as long as
// it is big, so where the chunker put its boundary is measurable without
// decoding anything.
func TestALongRecordingGoesUpInPiecesThatAddBackUp(t *testing.T) {
	if _, err := exec.LookPath("ffmpeg"); err != nil {
		t.Skip("no ffmpeg on this machine")
	}
	stems := t.TempDir()
	a, calls := sepFake(t, func() []sepStem {
		return []sepStem{
			sepStemOf(t, "vocals", sepTone(t, filepath.Join(stems, "v.wav"), 300, 1)),
			sepStemOf(t, "instrumental", sepTone(t, filepath.Join(stems, "r.wav"), 6000, 1)),
		}
	})
	// longer than one request holds, by twenty seconds, and loud in its first
	// half only -- so which stretch a chunk actually holds can be measured
	src := filepath.Join(a.root, "long-2026-08-30-17-10-36.wav")
	half := (sepChunkMax + 20) / 2
	mustFFmpeg(t, "-f", "lavfi", "-t", fmt.Sprint(half),
		"-i", "sine=frequency=440:sample_rate="+sepRate,
		"-f", "lavfi", "-t", fmt.Sprint(half), "-i", "anullsrc=r="+sepRate+":cl=mono",
		"-filter_complex", "[0][1]concat=n=2:v=0:a=1",
		"-ac", sepChannels, "-c:a", "pcm_s16le", src)

	res, err := a.separateOne(src, 0, 1)
	if err != nil {
		t.Fatalf("splitting: %v", err)
	}
	if calls.runs != 2 {
		t.Fatalf("%.0f s went up in %d requests, want two", sepChunkMax+20, calls.runs)
	}
	// the pieces are the recording, not a re-reading of its first minutes: the
	// second one starts where the first ended, which is what seeking the input
	// ahead of opening it buys
	if len(calls.up) != 2 {
		t.Fatalf("%d files went up for two chunks: %v", len(calls.up), calls.names)
	}
	first, second := wavSecs(calls.up[0]), wavSecs(calls.up[1])
	for i, secs := range []float64{first, second} {
		// under the ceiling, which is the reason the chunking exists at all,
		// and not a runt either -- even pieces beat a full one and a remainder
		if secs > sepChunkMax || secs < sepChunkMax/3 {
			t.Errorf("chunk %d is %.1f s, want an even share under the %.0f s ceiling",
				i, secs, sepChunkMax)
		}
	}
	if total := first + second; total < sepChunkMax+19 || total > sepChunkMax+21 {
		t.Errorf("the chunks add up to %.1f s, but the recording is %.0f s -- "+
			"a piece of it was never separated", total, sepChunkMax+20)
	}
	// the second chunk is the second half of the recording, not the first half
	// sent twice: two chunks of the right length can still be the same audio,
	// and seeking the input ahead of opening it is what makes them different
	if len(calls.peaks) != 2 || calls.peaks[0] < 1000 {
		t.Fatalf("the chunks came up peaking at %v, want the first one loud", calls.peaks)
	}
	if calls.peaks[1] > 100 {
		t.Errorf("the second chunk peaks at %d, but the recording is silent after "+
			"%.0f s -- it was cut from the wrong place", calls.peaks[1], half)
	}
	// and both answers came back into the two halves, in order
	for _, p := range []string{res.rest, res.voice} {
		if d, err := ffprobeDur(p); err != nil || d < 1.8 || d > 2.2 {
			t.Errorf("%s is %.2f s (%v), want both one-second answers joined",
				filepath.Base(p), d, err)
		}
	}
	// a recording has no picture, so the rest of it is a plain wav
	if filepath.Ext(res.rest) != ".wav" {
		t.Errorf("the rest of a recording came out as %s", filepath.Base(res.rest))
	}
}

// The phase as the run sees it: the wishes are granted, and what comes back is
// the session the rest of Prepare works on -- the frames out of the voiceless
// half, the transcript of the voice, and the narrator still pointing at whoever
// was talking. Nothing here touches a widget; the run is on a goroutine and the
// snapshot is what it has.
func TestGrantingTheWishesRewritesWhatTheRestOfTheRunSees(t *testing.T) {
	if _, err := exec.LookPath("ffmpeg"); err != nil {
		t.Skip("no ffmpeg on this machine")
	}
	stems := t.TempDir()
	a, calls := sepFake(t, func() []sepStem {
		return []sepStem{
			sepStemOf(t, "vocals", sepTone(t, filepath.Join(stems, "v.wav"), 300, 2)),
			sepStemOf(t, "instrumental", sepTone(t, filepath.Join(stems, "r.wav"), 6000, 2)),
		}
	})
	cap1 := filepath.Join(a.root, "cap-2026-08-30-17-10-36.mkv")
	mustFFmpeg(t, "-f", "lavfi", "-t", "2", "-i", "testsrc=size=160x120:rate=15",
		"-f", "lavfi", "-t", "2", "-i", "sine=frequency=1000:sample_rate=44100",
		"-c:v", "libx264", "-pix_fmt", "yuv420p", "-c:a", "pcm_s16le", "-shortest", cap1)
	cam2 := filepath.Join(a.root, "cam2-2026-08-30-17-10-36.mkv")
	mustFFmpeg(t, "-f", "lavfi", "-t", "2", "-i", "testsrc2=size=160x120:rate=15",
		"-c:v", "libx264", "-pix_fmt", "yuv420p", cam2)

	a.snapItems([]sourceItem{
		{path: cap1, footage: true, narrator: 1, sepVoice: true},
		{path: cam2, footage: true},
	})
	vids, auds := a.snappedSources()
	vids, auds, err := a.separateVoices(vids, auds)
	if err != nil {
		t.Fatalf("granting the wishes: %v", err)
	}
	if calls.runs != 1 {
		t.Errorf("one flagged row took %d separations", calls.runs)
	}
	rest, voice := sepNames(a.sepDir(), cap1)
	if len(vids) != 2 || vids[0] != rest || vids[1] != cam2 {
		t.Errorf("the frames would be taken from %v, want [%s %s]", vids, rest, cam2)
	}
	if len(auds) != 1 || auds[0] != voice {
		t.Errorf("the transcript would be of %v, want [%s]", auds, voice)
	}
	if got := a.narratorPath(1); got != voice {
		t.Errorf("narrator 1 is %q, want the voice that was split off", got)
	}
	if got := a.sepWanted(); len(got) != 0 {
		t.Errorf("the run would split %v again on the next press", got)
	}
	// nothing was asked of the server for the row that never asked
	if len(calls.names) != 1 || !strings.HasPrefix(calls.names[0], "mix") {
		t.Errorf("the files that went up were %v, want the one flagged mixdown", calls.names)
	}
}

// A half of a split is told apart by its name -- the only thing a stem carries
// -- and a row that is one offers no scissors: the wish would be to split the
// voice off a voice. The old spellings still count, for a project split before
// the names changed.
func TestAHalfOfASplitOffersNoScissors(t *testing.T) {
	for _, c := range []struct {
		path string
		want bool
	}{
		{"/o/stems/cap.split-novoice.mkv", true},
		{"/o/stems/cap.split-voice.wav", true},
		{"/o/stems/cap.novoice.mkv", true},
		{"/o/stems/cap.voice.wav", true},
		{"/rec/cap.mkv", false},
		{"/rec/my-voice-memo.wav", false},
		{"/rec/voice.wav", false},
	} {
		if got := splitProduct(c.path); got != c.want {
			t.Errorf("splitProduct(%q) = %v, want %v", c.path, got, c.want)
		}
	}
	// dead on such a row, not gone from it: taken out of the layout the
	// scissors take their column with them, and a list where some rows are
	// split products and some are not then has its trash at two different x
	src := readSrc(t, "sources.go")
	if !strings.Contains(src, "sep.SetSensitive(!splitProduct(it.path))") {
		t.Error("the scissors are offered on a row that is already a half of a split")
	}
	if strings.Contains(src, "sep.SetVisible(") {
		t.Error("the scissors leave the row rather than going grey, so the rows stop lining up")
	}
	// the same rule for the narrator slot: the number's room is there whether
	// or not there is a number in it, or a tagged row is wider than an
	// untagged one and no two file names start at the same x
	for _, want := range []string{"slot := gtk.NewLabel(\"\")", "slot.SetWidthChars(1)", "nb.Append(slot)"} {
		if !strings.Contains(src, want) {
			t.Errorf("the narrator button no longer keeps room for its slot: %q", want)
		}
	}
	// and the halves are named so: split-novoice keeps the picture and the
	// room, split-voice is the voice alone
	rest, voice := sepNames("/out/stems", "/rec/Kooha-2026-08-30-17-10-36.mp4")
	if filepath.Base(rest) != "Kooha-2026-08-30-17-10-36.split-novoice.mkv" || filepath.Base(voice) != "Kooha-2026-08-30-17-10-36.split-voice.wav" {
		t.Errorf("the halves are %s and %s", filepath.Base(rest), filepath.Base(voice))
	}
}

// The split says how the sound fell between its halves, because a rest that is
// a residue is otherwise found by ear: a recording the model heard as voice
// throughout leaves the half named for the room with next to nothing, and a
// day later that is "the footage is silent". Measured on real tones through
// ffmpeg, like the render tests.
func TestASplitSaysWhenTheRestIsAResidue(t *testing.T) {
	if _, err := exec.LookPath(ffTool("ffmpeg")); err != nil {
		t.Skip("no ffmpeg")
	}
	dir := t.TempDir()
	tone := func(name string, gain float64) string {
		p := filepath.Join(dir, name)
		if err := exec.Command(ffTool("ffmpeg"), "-v", "error", "-y", "-f", "lavfi", "-i",
			fmt.Sprintf("sine=frequency=440:duration=1,volume=%g", gain), "-c:a", "pcm_s16le", p).Run(); err != nil {
			t.Fatal(err)
		}
		return p
	}
	mix, rest := tone("mix.wav", 1), tone("rest.wav", 0.1) // 20 dB under
	// lavfi's sine is a quiet tone; what matters below is the difference
	if got := sepMeanDB(sepLoudness(mix)); got > -2 || got < -40 {
		t.Errorf("the mix measured %g dB", got)
	}
	note := sepLopsided(mix, rest)
	if !strings.Contains(note, "under the mix") || !strings.Contains(note, "20 dB") {
		t.Errorf("a rest 20 dB under the mix drew the note %q", note)
	}
	if note := sepLopsided(mix, tone("even.wav", 0.7)); note != "" {
		t.Errorf("a rest 3 dB under the mix drew a note: %q", note)
	}
	if sepLoudness(filepath.Join(dir, "none.wav")) != "" {
		t.Error("a file that is not there had a loudness")
	}
}
