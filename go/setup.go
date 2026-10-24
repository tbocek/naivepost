package main

import (
	"bytes"
	"context"
	"encoding/base64"
	"encoding/json"
	"fmt"
	"image"
	"image/color"
	"image/draw"
	"image/png"
	"net/http"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"sync/atomic"
	"time"

	"github.com/diamondburned/gotk4/pkg/glib/v2"
	"github.com/diamondburned/gotk4/pkg/gtk/v4"
)

// Settings: llm.conf, kept bash-sourceable. The two HTTP endpoints (the
// writing model, the audio.cpp server) and the model ids the second needs,
// plus ffmpeg and firefox. The language is the project's, not the machine's.
// Each server can be queried from here, because the failures this catches --
// a misspelled model id, a server serving the wrong family, an ffmpeg missing
// a filter -- are silent otherwise.

type appConf struct {
	Server, Model, Key string // the LLM that writes
	TTS                string // the audio.cpp server; blank = the compose default
	TTSKey             string // its API key; blank = none, which the local stack is

	// the sd.cpp server that draws the thumbnail; blank for the compose default,
	// with a key like every server here (any can sit behind a proxy wanting a
	// token). No model box: sd-server loads one model at start and takes none per
	// request; the Test button reports what is loaded.
	SD    string
	SDKey string

	// What to ask the audio server for. The voices folder is the ONE folder
	// naivepost still reads -- the wavs the voice picker lists, and where "Add
	// file…" converts new ones into -- and it has no box in the GUI at all: it
	// is AUDIOCPP_VOICES in llm.conf, defaulting to defVoices, which is the
	// same path the compose file mounts into the audio server. It had a row on
	// the Narrate step, next to a scrolling list of voices; the list is a
	// dropdown now and the row went with it, because pointing at a folder is
	// not how a voice gets used here -- "Add file…" copies one in. Model weights are
	// not read from anywhere here; their paths are config-audiocpp.json's
	// business, on the server's side. The model ids are the server's own --
	// what that json calls them -- and they are here rather than compiled in
	// because the models get replaced faster than the code does.
	Voices              string
	ASRModel, DiarModel string
	TTSModel            string
	SepModel            string
	// which model places cut points on the word. Empty is not a missing
	// setting: it means "whichever one on that server does align", which is
	// the ordinary answer when a server has one aligner. It is a box rather
	// than a rule because a server can have two, and because one of them can
	// be registered and unserviceable -- and then a rule leaves nothing to do
	// about it.
	AlignModel string

	// Which ffmpeg to shell out to. Blank -- the ordinary answer -- means the
	// name alone, resolved off PATH like any other tool. A path is for the
	// machine with a hand-built ffmpeg beside the distro's, or one where the
	// GUI's PATH is not the shell's; see ffTool for where ffprobe comes from
	// then. No default: inventing one would break every machine but this one,
	// which is the fault the whole config file grew out of.
	FFmpeg string

	// The firefox the model searches the web through (websearch.go). Blank
	// means the one on PATH; "off" means the model is offered no search and
	// writes only what the material says.
	Firefox string
}

// ffSet is the settings box, shared with the runners. The pipeline shells out
// from goroutines, so this is stored rather than passed: readConf refreshes it
// before each step, the same read that gives that step its endpoints.
var ffSet atomic.Pointer[string]

// ffTool is the command to run for one of the two ffmpeg binaries. With no
// path set it is the bare name, which exec resolves off PATH. With one set,
// ffprobe is taken from the SAME folder: a hand-built ffmpeg paired with the
// distro's ffprobe is precisely the mismatch the box exists to fix, and
// letting probe fall back to PATH would quietly recreate it.
func ffTool(name string) string {
	p := ffSet.Load()
	if p == nil || *p == "" {
		return name
	}
	if name == "ffmpeg" {
		return *p
	}
	return filepath.Join(filepath.Dir(*p), name)
}

// The dev box this grew up on. A config line that is missing or blank means
// "whatever the build shipped with" -- an empty model id would otherwise fail
// with a message about nothing.
const (
	defVoices    = "/mnt/models/audiocpp/voices"
	defASRModel  = "nemotron-asr"
	defDiarModel = "sortformer-diar"
	defTTSModel  = "index-tts2"
	defSepModel  = "bs-roformer"
	// ...and the forced aligner, which is a PREFERENCE rather than a demand:
	// it is the one tried first when the box is empty and the server has it,
	// and a server with some other aligner still aligns (alignModels). The
	// stack registers two -- mms-aligner and this -- and the id is the whole
	// difference in how well a cut point lands, so leaving it to sort order,
	// where "mms" comes first, was picking the weaker one by alphabet.
	defAlignModel = "qwen3-aligner"
)

func or(v, def string) string {
	if s := strings.TrimSpace(v); s != "" {
		return s
	}
	return def
}

// withDefaults fills the blanks. The TTS endpoint is pointedly not in here:
// empty there is a real answer, meaning "the compose service on loopback".
// The two SD fields are out for the same reason and for one more: an empty
// model name means "do not check", and inventing a default would turn every
// Test on a differently-stocked server into a failure about a name nobody
// typed.
func (c appConf) withDefaults() appConf {
	// inside a Flatpak the shared folder is not visible unless the user
	// granted it, so the default is a folder of the sandbox's own, which "Add
	// file…" can write to; the wav is sent to the server with each request,
	// so the server needs no view of it. Pointing AUDIOCPP_VOICES at the
	// shared folder still works once the folder is granted (flatpak override).
	if inFlatpak() {
		c.Voices = or(c.Voices, filepath.Join(dataHome(), "naivepost", "voices"))
	} else {
		c.Voices = or(c.Voices, defVoices)
	}
	c.ASRModel = or(c.ASRModel, defASRModel)
	c.DiarModel = or(c.DiarModel, defDiarModel)
	c.TTSModel = or(c.TTSModel, defTTSModel)
	c.SepModel = or(c.SepModel, defSepModel)
	// AlignModel is not filled in here on purpose, though it has a default
	// (defAlignModel): no aligner at all is a working setup, so writing a name
	// into the file would put a red badge on a row that is allowed to be
	// empty, and would hold a server that has a different one to a name it
	// cannot answer to. Empty means "prefer that one if you have it", and
	// alignModels is where that is done.
	return c
}

// confPath is the one config file for the machine: ~/.config/naivepost/llm.conf,
// or "" when there is nowhere to put it -- no HOME, no XDG_CONFIG_HOME. Not
// per naivepost root, which is where it used to be and was wrong twice over: the
// endpoints are the same for every session this computer ever cuts, and a
// session folder gets copied and zipped and handed around, so the key went
// with it.
func confPath() string {
	d := configDir()
	if d == "" {
		return ""
	}
	return filepath.Join(d, "llm.conf")
}

// legacyConfPath is where the file used to be, beside the videos. Read when
// the new one is not there yet, so a machine that has been cutting for months
// keeps its endpoints without being asked to retype them; migrateConf copies
// it across on the next launch.
func (a *App) legacyConfPath() string {
	// no session folder means nothing to look beside. Never a relative path,
	// which is what filepath.Join would hand back: the config a program reads
	// must not depend on the directory it happened to be started from.
	if a.root == "" {
		return ""
	}
	return filepath.Join(a.root, "llm.conf")
}

// globalConf is the whole file: this machine's endpoints, and the handful of
// things naivepost remembers between launches. One struct because it is one
// file, and because writeGlobal writes the whole of it -- a caller that
// changed one half has to hand the other half back unharmed, which is a rule
// that is much harder to forget when both halves are in its hand.
type globalConf struct {
	appConf

	// The named project last opened or saved, keyed by the naivepost root: every
	// path inside a project file is relative to its root (relToRoot). Nothing
	// prunes this map; entries are one short string.
	Projects map[string]string

	// PROMPT_* lines were here: which of a job's several wordings this
	// machine had picked, back when a Style dropdown chose between them.
	// There is one wording per job now (prompts.go), so there is nothing to
	// pick; a settings file written then keeps the lines and nothing reads
	// them.
}

// readGlobal parses the file. Every blank is filled by the built-in, so a
// caller never has to ask whether a missing key means "default" or "empty".
//
// Callable from a runner's goroutine: it re-reads a small file rather than
// sharing mutable state, which is also why a settings change takes effect on
// the next step without a restart.
func (a *App) readGlobal() globalConf {
	g := globalConf{Projects: map[string]string{}}
	b, err := os.ReadFile(confPath())
	if err != nil {
		b, err = os.ReadFile(a.legacyConfPath())
	}
	if err != nil {
		g.appConf = g.appConf.withDefaults()
		g.Projects = loadSettings().Projects
		storeFF(g.FFmpeg)
		return g
	}
	c := &g.appConf
	legacyRoot := ""     // AUDIOCPP_MODELS named the parent; voices/ was implied
	sawProjects := false // ...so a file written before the merge takes settings.json's
	roots, files := map[string]string{}, map[string]string{}
	for _, line := range strings.Split(string(b), "\n") {
		k, v, ok := strings.Cut(strings.TrimSpace(line), "=")
		if !ok || strings.HasPrefix(k, "#") {
			continue
		}
		v = strings.Trim(v, `"`)
		switch {
		case strings.HasPrefix(k, "PROJECT_") && strings.HasSuffix(k, "_ROOT"):
			sawProjects = true
			roots[strings.TrimSuffix(strings.TrimPrefix(k, "PROJECT_"), "_ROOT")] = v
			continue
		case strings.HasPrefix(k, "PROJECT_") && strings.HasSuffix(k, "_FILE"):
			sawProjects = true
			files[strings.TrimSuffix(strings.TrimPrefix(k, "PROJECT_"), "_FILE")] = v
			continue
		case strings.HasPrefix(k, "PROMPT_"):
			continue // which wording a job was picked to; there is one now
		}
		switch k {
		case "LLM_SERVER":
			c.Server = v
		case "LLM_MODEL":
			c.Model = v
		case "LLM_API_KEY":
			c.Key = v
		case "AUDIOCPP_SERVER":
			c.TTS = strings.TrimRight(strings.TrimSpace(v), "/")
		case "AUDIOCPP_API_KEY":
			c.TTSKey = v
		case "AUDIOCPP_VOICES":
			c.Voices = v
		case "AUDIOCPP_MODELS":
			legacyRoot = v
		case "AUDIOCPP_ASR_MODEL":
			c.ASRModel = v
		case "AUDIOCPP_DIAR_MODEL":
			c.DiarModel = v
		case "AUDIOCPP_SEP_MODEL":
			c.SepModel = v
		case "AUDIOCPP_ALIGN_MODEL":
			c.AlignModel = v
		case "FFMPEG":
			c.FFmpeg = v
		case "FIREFOX":
			c.Firefox = v
		case "AUDIOCPP_TTS_MODEL":
			c.TTSModel = v
			// AUDIOCPP_LANGUAGE was here, and is now the project's (Project.Language).
			// An old conf file still carrying the key falls through unread, the same
			// way SD_MODEL below does.
		case "SD_SERVER":
			c.SD = strings.TrimRight(strings.TrimSpace(v), "/")
			// SD_MODEL was here. It named the weights the Test button held the
			// server to; the server reports them itself, so an old conf file
			// still carrying the key just falls through unread.
		case "SD_API_KEY":
			c.SDKey = v
		}
	}
	if c.Voices == "" && legacyRoot != "" {
		c.Voices = filepath.Join(legacyRoot, "voices")
	}
	for n, root := range roots {
		if f := files[n]; root != "" && f != "" {
			g.Projects[root] = f
		}
	}
	// a conf written before the two files became one: what it does not say
	// about, settings.json still does
	if !sawProjects {
		for root, f := range loadSettings().Projects {
			g.Projects[root] = f
		}
	}
	*c = c.withDefaults()
	storeFF(c.FFmpeg)
	return g
}

// storeFF is where ffTool learns what the settings box says, on the same read
// that feeds the step -- the runners read no config of their own.
func storeFF(path string) {
	ff := strings.TrimSpace(path)
	ffSet.Store(&ff)
}

// readConf is the machine's endpoints, which is all most callers want.
func (a *App) readConf() appConf { return a.readGlobal().appConf }

// migrateConf moves a pre-merge llm.conf into the config folder, once, on the
// launch that finds it beside the videos. Copied rather than moved: the old
// file is one line of documentation about a machine that has been working for
// months, and deleting somebody's config to tidy up is not this program's
// call. It stops being READ the moment the new one exists, which is what the
// log line says.
func (a *App) migrateConf() {
	if confPath() == "" || exists(confPath()) || !exists(a.legacyConfPath()) {
		return
	}
	g := a.readGlobal() // the old file, plus whatever settings.json remembered
	if err := a.writeGlobal(g); err != nil {
		a.logf("settings: %v", err)
		return
	}
	a.logf(">>> settings moved to %s -- %s is no longer read", confPath(), a.legacyConfPath())
}

// writeConf saves what the gear dialog holds, and nothing else. The rest of
// the file is read back first and handed on untouched: this writes the whole
// file, so a half not carried through is a half erased.
func (a *App) writeConf(c appConf) error {
	g := a.readGlobal()
	g.appConf = c
	return a.writeGlobal(g)
}

// writeGlobal puts the file down whole. Still bash-sourceable -- quoted
// values, no syntax a shell would choke on -- because the endpoints are as
// useful to a script on this machine as they are to the GUI, and still 0600,
// because the LLM key is in it.
func (a *App) writeGlobal(g globalConf) error {
	p := confPath()
	if p == "" {
		return fmt.Errorf("nowhere to write the settings -- neither XDG_CONFIG_HOME nor HOME is set")
	}
	if err := os.MkdirAll(filepath.Dir(p), 0o700); err != nil {
		return err
	}
	c := g.appConf.withDefaults() // a cleared box means the default, never an empty flag
	body := fmt.Sprintf(`# Endpoints and local tools used by the pipeline (written by the GUI's
# settings dialog). Bash-sourceable -- keep this file chmod 600, the key is a
# credential.
# The model that writes -- an OpenAI-compatible chat API; empty means
# 127.0.0.1:%d, which is halogen-flash-server as it comes. The model id has no
# default: it has to be one this server lists (Fetch models in the dialog).
LLM_SERVER=%q
LLM_MODEL=%q
LLM_API_KEY=%q
# audio.cpp server -- it speaks the narration and listens for Prepare; empty
# means 127.0.0.1:%d. Naivepost only talks to it over HTTP, never starts it --
# starting it is the job of whoever runs the stack.
AUDIOCPP_SERVER=%q
AUDIOCPP_API_KEY=%q

# The reference-voice wavs the voice picker lists; "Add sample…" converts new
# ones into here, and the folder is chosen on the Narrate step, beside the list
# it fills. The compose file mounts this same folder into the server as its
# voice library. Nothing else is read from disk: model weights and their paths
# are config-audiocpp.json's business, on the server's side.
AUDIOCPP_VOICES=%q
# The ids the server lists for its four jobs -- transcribe, tell speakers
# apart, speak the narration, and split a voice off a recording. Only the last
# is optional: nothing asks for it unless a source is flagged to be split.
AUDIOCPP_ASR_MODEL=%q
AUDIOCPP_DIAR_MODEL=%q
AUDIOCPP_TTS_MODEL=%q
AUDIOCPP_SEP_MODEL=%q

# ...and the one that places a cut point on the word rather than on a silence.
# Empty means %s where that server declares it, and otherwise whichever
# model it declares for "align" -- the answer whenever it has exactly one. Name
# it when there are two others, or when the one it picks is registered but the
# engine will not serve it.
AUDIOCPP_ALIGN_MODEL=%q

# Which ffmpeg every step shells out to; empty means whichever one is on PATH,
# which is what it should be unless this machine has more than one. ffprobe is
# taken from the same folder as whatever is named here.
FFMPEG=%q

# The firefox the model looks facts up through, headless, when a caption or a
# line needs a detail the footage does not show (web_search); empty means the
# one on PATH, "off" means no search is offered at all.
FIREFOX=%q

# stable-diffusion.cpp's sd-server -- it draws the thumbnail on the Produce
# step; empty means 127.0.0.1:%d. There is no model key: the server serves the
# one model it was started with (SD_ARGS in cpp/run.sh), and nothing naivepost
# sends can change it.
SD_SERVER=%q
SD_API_KEY=%q
`, llmPort, c.Server, c.Model, c.Key, ttsPort, c.TTS, c.TTSKey,
		c.Voices, c.ASRModel, c.DiarModel, c.TTSModel, c.SepModel, defAlignModel, c.AlignModel, c.FFmpeg,
		c.Firefox, sdPort, c.SD, c.SDKey)
	body += rememberedBody(g)
	return os.WriteFile(p, []byte(body), 0o600)
}

// rememberedBody is the half of the file that is not a setting: what naivepost
// noticed. Numbered pairs and one key per prompt, so it reads and sources like
// the rest of the file; sorted, so an unchanged save is byte-identical.
func rememberedBody(g globalConf) string {
	var b strings.Builder
	if len(g.Projects) > 0 {
		b.WriteString(`
# The project file last open in each session folder, and the folder it belongs
# to. Keyed by folder because every path inside a project is relative to it:
# opening last night's project from a different folder would resolve its
# sources against the wrong directory. Delete a pair to forget one.
`)
		for i, root := range sortedKeys(g.Projects) {
			fmt.Fprintf(&b, "PROJECT_%d_ROOT=%q\nPROJECT_%d_FILE=%q\n", i+1, root, i+1, g.Projects[root])
		}
	}
	return b.String()
}

// fetchModels asks the server for its model list.
func fetchModels(server, key string) ([]string, error) {
	req, err := http.NewRequest("GET", strings.TrimRight(server, "/")+"/v1/models", nil)
	if err != nil {
		return nil, err
	}
	bearer(req, key)
	client := &http.Client{Timeout: 15 * time.Second}
	resp, err := client.Do(req)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()
	if resp.StatusCode != 200 {
		return nil, fmt.Errorf("server answered %s", resp.Status)
	}
	var body struct {
		Data []struct {
			ID string `json:"id"`
		} `json:"data"`
	}
	if err := json.NewDecoder(resp.Body).Decode(&body); err != nil {
		return nil, err
	}
	var ids []string
	for _, m := range body.Data {
		ids = append(ids, m.ID)
	}
	if len(ids) == 0 {
		return nil, fmt.Errorf("server lists no models")
	}
	return ids, nil
}

// llmRoundTrip is one completion against the configured server, shaped like
// the pipeline's own execute mode, thinking off and all: a reasoning model
// left to think spends the whole budget on it and answers with empty content,
// which is a pass that proves nothing. The content is either a plain string or
// a parts array (txtPart/imgPart), which is the whole difference between the
// two Test buttons that call it.
func llmRoundTrip(c appConf, content any, timeout time.Duration) (string, error) {
	req := map[string]any{
		"model":       c.Model,
		"messages":    []map[string]any{msg("user", content)},
		"temperature": 0.6,
		"max_tokens":  16, // one word and a stop; a rambling model is cut off here
	}
	thinkSwitch(req, false) // the same switch the pipeline's execute mode sends
	body, _ := json.Marshal(req)
	post, err := http.NewRequest("POST", llmServer(c)+"/v1/chat/completions",
		strings.NewReader(string(body)))
	if err != nil {
		return "", err
	}
	bearer(post, c.Key)
	post.Header.Set("Content-Type", "application/json")
	resp, err := (&http.Client{Timeout: timeout}).Do(post)
	if err != nil {
		return "", err
	}
	defer resp.Body.Close()
	var out struct {
		Choices []struct {
			Message struct{ Content string } `json:"message"`
		} `json:"choices"`
		Error struct{ Message string } `json:"error"`
	}
	if err := json.NewDecoder(resp.Body).Decode(&out); err != nil {
		return "", fmt.Errorf("%s: unreadable answer", resp.Status)
	}
	if resp.StatusCode != 200 {
		if out.Error.Message != "" {
			return "", fmt.Errorf("%s: %s", resp.Status, out.Error.Message)
		}
		return "", fmt.Errorf("server answered %s", resp.Status)
	}
	if len(out.Choices) == 0 {
		return "", fmt.Errorf("no answer -- is %q the id the server lists?", c.Model)
	}
	reply := strings.TrimSpace(out.Choices[0].Message.Content)
	if reply == "" {
		return "", fmt.Errorf("%q answered with empty content -- it is probably spending "+
			"the token budget on reasoning", c.Model)
	}
	return reply, nil
}

// testLLM does one real round trip -- model id and key are only proven by a
// completion. The smallest completion that proves them; a slow answer is the
// server loading the model, hence the generous timeout and the elapsed time.
func testLLM(c appConf) (string, error) {
	start := time.Now()
	reply, err := llmRoundTrip(c, "Reply with the single word: ok", 60*time.Second)
	if err != nil {
		return "", err
	}
	if r := []rune(reply); len(r) > 40 { // by rune: the answer can be anything
		reply = string(r[:40]) + "…"
	}
	return fmt.Sprintf("%s answered in %.1f s: %q", c.Model, time.Since(start).Seconds(), reply), nil
}

// visionProbe is the sample image the vision test shows the model: a plain red
// square, generated here rather than shipped, as the data URL the request
// carries. Small on purpose -- 48 px is enough pixels for any vision encoder,
// and few enough tokens to keep the test as quick as the text one.
func visionProbe() string {
	img := image.NewRGBA(image.Rect(0, 0, 48, 48))
	draw.Draw(img, img.Bounds(), image.NewUniform(color.RGBA{R: 220, A: 255}), image.Point{}, draw.Src)
	var buf bytes.Buffer
	png.Encode(&buf, img)
	return "data:image/png;base64," + base64.StdEncoding.EncodeToString(buf.Bytes())
}

// testVision asks the same model about a picture. A model that describes
// frames is what the describer is built on, and "can it see at all" is exactly what a
// completion of words cannot prove: a text-only model, or a vision model
// served without its mmproj file, passes the text test and then fails minutes
// into a run -- or worse, answers every frame from the file name alone. One
// red square settles it: the answer is either the colour or a confession.
func testVision(c appConf) (string, error) {
	content := []map[string]any{
		txtPart("In one word: what colour is this square?"),
		{"type": "image_url", "image_url": map[string]any{"url": visionProbe()}},
	}
	start := time.Now()
	// more generous than the text test: an image prompt makes a cold server
	// load the vision encoder too
	reply, err := llmRoundTrip(c, content, 120*time.Second)
	if err != nil {
		return "", err
	}
	if r := []rune(reply); len(r) > 40 {
		reply = string(r[:40]) + "…"
	}
	if !strings.Contains(strings.ToLower(reply), "red") {
		return "", fmt.Errorf("shown a plain red square, %q answered %q -- it is not seeing the "+
			"image. Prepare sends video frames to this model: it needs a vision model, served with "+
			"its mmproj/vision file", c.Model, reply)
	}
	return fmt.Sprintf("%s saw the red square in %.1f s: %q", c.Model, time.Since(start).Seconds(), reply), nil
}

// audioProbe is the half both audio.cpp tests share: something answers on that
// port, and it answers like audio.cpp.
func audioProbe(url, key string) (map[string]audioModel, time.Duration, error) {
	url = strings.TrimRight(url, "/")
	start := time.Now()
	req, err := http.NewRequest("GET", url+"/health", nil)
	if err != nil {
		return nil, 0, err
	}
	bearer(req, key)
	if _, err := (&http.Client{Timeout: 15 * time.Second}).Do(req); err != nil {
		return nil, 0, fmt.Errorf("nothing answering: %w", err)
	}
	cat, err := audioCatalog(url, key)
	if err != nil {
		return nil, 0, fmt.Errorf("%s does not look like an audio.cpp server: %w", url, err)
	}
	if len(cat) == 0 {
		return nil, 0, fmt.Errorf("healthy, but serving no models")
	}
	return cat, time.Since(start), nil
}

// testTTS checks the thing that actually matters about the endpoint for
// speaking: that what it serves can clone a voice. A server on the right port
// with only the step-1 models loaded is the failure this catches.
func testTTS(url, key string) (string, error) {
	cat, took, err := audioProbe(url, key)
	if err != nil {
		return "", err
	}
	clone := ""
	for _, id := range sortedKeys(cat) {
		if m := cat[id]; m.Family == "index_tts2" || m.Task == "clon" {
			clone = id
			break
		}
	}
	if clone == "" {
		return "", fmt.Errorf("serves %s -- none of them can clone a voice", catalogIDs(cat))
	}
	return fmt.Sprintf("healthy in %.0f ms, will narrate with %q (of %d model(s))",
		float64(took.Milliseconds()), clone, len(cat)), nil
}

// testAligner reports which model on that server does forced alignment, and
// says what it means when none does.
//
// No box goes with it, and that is the point. Every other model here is asked
// for by id because the request carries one; the aligner is asked for by TASK
// (align.go), so the catalog is the whole of the answer and a box beside it
// could only ever disagree with the server. The same deal the drawing server
// gets: name what is loaded rather than hold it to a name.
//
// Not having one is not a failure. It is a smaller tool -- the joins fall back
// to the waveform, which places a cut wherever there is a silence to place it
// in, and cannot cut between two words of one breath.
func testAligner(url, key, want string) (string, error) {
	cat, took, err := audioProbe(url, key)
	if err != nil {
		return "", err
	}
	if want = strings.TrimSpace(want); want != "" {
		m, ok := cat[want]
		if !ok {
			return "", fmt.Errorf("no model %q on %s -- it serves %s", want, url, catalogIDs(cat))
		}
		if m.Task != "" && m.Task != "align" {
			return "", fmt.Errorf("%q is declared task %q, and cannot align", want, m.Task)
		}
		return fmt.Sprintf("%s (%s) places the cut points, on the word (%s)", want, m.Family, took), nil
	}
	var ids []string
	for id, m := range cat {
		if m.Task == "align" {
			ids = append(ids, id)
		}
	}
	// the order the run will try them in, so a server with two is not told
	// "the first is used" about a different first (alignOrder)
	alignOrder(ids)
	named := make([]string, 0, len(ids))
	for _, id := range ids {
		named = append(named, fmt.Sprintf("%s (%s)", id, cat[id].Family))
	}
	switch len(ids) {
	case 0:
		return fmt.Sprintf("no model on %s does forced alignment, so cut points come off the "+
			"waveform: clean wherever there is a silence to cut in, and unable to cut between "+
			"two words of one breath. Register one with task \"align\" in config-audiocpp.json "+
			"to place them on the words instead (%s)", url, took), nil
	case 1:
		return fmt.Sprintf("%s places the cut points, on the word (%s)", named[0], took), nil
	default:
		return fmt.Sprintf("%s answer for alignment on %s; the first is used -- %s, because "+
			"%s is preferred where a server has it -- and a name in this box overrides that (%s)",
			strings.Join(named, " and "), url, ids[0], defAlignModel, took), nil
	}
}

// testAudioModel asks the server whether one id is in the catalog and declared
// for the task we will ask of it -- one id per button, so the verdict names
// which. Learned over HTTP only, so the error can name the two ways a catalog
// grows but cannot do either.
func testAudioModel(url, key, id, task, what string) (string, error) {
	cat, took, err := audioProbe(url, key)
	if err != nil {
		return "", err
	}
	m, ok := cat[id]
	if !ok {
		return "", fmt.Errorf("no model %q to %s -- the server serves %s. Add it either in "+
			"the server's own browser UI at %s (its model page installs and loads one live, "+
			"if the server was started with --ui-management), or in the config-audiocpp.json "+
			"it reads at startup, followed by docker compose up -d --force-recreate audio "+
			"-- recreate, because a restarted container can keep a stale copy of the edited file",
			id, what, catalogIDs(cat), url)
	}
	if m.Task != "" && m.Task != task {
		return "", fmt.Errorf("%q is declared task %q, and cannot %s", id, m.Task, what)
	}
	return fmt.Sprintf("%q is served, answered in %.0f ms", id, float64(took.Milliseconds())), nil
}

// What the pipeline asks ffmpeg for by name. Each of these is a real build
// option, not a given: rubberband and libx264 need --enable-gpl, and the
// subtitles filter needs libass. A build missing one works perfectly until the
// step that uses it, which is minutes into a render.
var (
	// drawtext used to be here for the Publish step's thumbnail title. The
	// title is now lettered by the image model itself (publish.go), so requiring
	// drawtext would fail a build over a filter nothing asks for any more.
	ffFilters  = []string{"rubberband", "subtitles", "loudnorm", "atempo", "amix", "adelay", "alimiter"}
	ffEncoders = []string{"libx264", "libx265", "aac", "libopus"}
)

// ffMissing lists which of want this build does not have. listFlag is -filters
// or -encoders; both print one component per line with the name in the second
// field, after a flags column.
func ffMissing(bin, listFlag string, want []string) []string {
	out, err := exec.Command(bin, "-hide_banner", listFlag).Output()
	if err != nil {
		return want
	}
	have := map[string]bool{}
	for _, line := range strings.Split(string(out), "\n") {
		if f := strings.Fields(line); len(f) >= 2 {
			have[f[1]] = true
		}
	}
	var missing []string
	for _, w := range want {
		if !have[w] {
			missing = append(missing, w)
		}
	}
	return missing
}

// firefoxVersion is the first line of --version from a firefox, or why it
// will not run. Split from testFirefox so it can be checked against a fake.
func firefoxVersion(bin string) (string, error) {
	out, err := exec.Command(bin, "--version").Output()
	if err != nil {
		return "", fmt.Errorf("%s will not run: %w", bin, err)
	}
	ver := strings.TrimSpace(strings.SplitN(string(out), "\n", 2)[0])
	if ver == "" {
		return "", fmt.Errorf("%s said nothing to --version", bin)
	}
	return ver, nil
}

// testFirefox is the firefox row's Test, the shape of testFFmpeg's: which
// binary the box resolves to, that it runs, and then the thing the pipeline
// actually does with it -- a headless start, the debugging port, one search
// -- because a firefox that prints its version and then cannot be driven is
// the failure the version check cannot see. "off" is not a failure: it is
// the box saying the model gets no search, and the badge says so back.
func (a *App) testFirefox(box string) (string, error) {
	box = strings.TrimSpace(box)
	if strings.EqualFold(box, firefoxOff) {
		return "off -- the model is offered no web search, and writes only what the material says", nil
	}
	bin, err := firefoxBin(box)
	if err != nil {
		if box != "" {
			return "", fmt.Errorf("%w -- leave the box empty to use PATH, or off for no search", err)
		}
		return "", fmt.Errorf("%w (Arch: pacman -S firefox)", err)
	}
	ver, err := firefoxVersion(bin)
	if err != nil {
		return "", err
	}
	where := bin
	if box == "" {
		where += " (off PATH)" // which one PATH gave is the thing worth seeing
	}
	hits, err := a.webSearch(context.Background(), bin, "duckduckgo")
	if err != nil {
		return "", fmt.Errorf("%s at %s runs, but could not be driven headless: %w", ver, where, err)
	}
	return fmt.Sprintf("%s\nat %s, drove a headless search: %d result(s)", ver, where, len(hits)), nil
}

// testFFmpeg checks the build named by the settings box -- or, with the box
// empty, whichever ffmpeg is on PATH. Failures report the path too: "which
// ffmpeg is it finding" is half of every ffmpeg problem, and PATH here is the
// GUI's, not the shell's.
func testFFmpeg(bin string) (string, error) {
	bin = strings.TrimSpace(bin)
	ff, err := exec.LookPath(or(bin, "ffmpeg"))
	if err != nil {
		if bin != "" {
			return "", fmt.Errorf("%s will not run: %w -- leave the box empty to use PATH", bin, err)
		}
		return "", fmt.Errorf("not on PATH -- every step shells out to it (Arch: pacman -S ffmpeg)")
	}
	// beside it, never off PATH: the pair has to match, and ffTool pairs them
	probe := filepath.Join(filepath.Dir(ff), "ffprobe")
	if _, err := exec.LookPath(probe); err != nil {
		return "", fmt.Errorf("found ffmpeg at %s, but no ffprobe beside it -- both are used", ff)
	}
	out, err := exec.Command(ff, "-hide_banner", "-version").Output()
	if err != nil {
		return "", fmt.Errorf("%s will not run: %w", ff, err)
	}
	ver := strings.TrimSpace(strings.SplitN(string(out), "\n", 2)[0])
	where := ff
	if bin == "" {
		where += " (off PATH)" // which one PATH gave is the thing worth seeing
	}
	missing := append(ffMissing(ff, "-filters", ffFilters),
		ffMissing(ff, "-encoders", ffEncoders)...)
	if len(missing) > 0 {
		return "", fmt.Errorf("%s at %s is built without %s -- the steps that need "+
			"those will fail mid-run", ver, where, strings.Join(missing, ", "))
	}
	return fmt.Sprintf("%s\nat %s, with every filter and encoder the pipeline uses", ver, where), nil
}

// testBadge is a Test button's verdict, at the row that asked: a spinner while
// the test runs, then a green check or a red cross, with the reason on hover.
// The log below says the same in words; the badge is what can be read at a
// glance, and it stays put while other rows are tested.
type testBadge struct {
	stack *gtk.Stack
	spin  *gtk.Spinner
}

func newTestBadge() *testBadge {
	b := &testBadge{stack: gtk.NewStack(), spin: gtk.NewSpinner()}
	ok := gtk.NewImageFromIconName("object-select-symbolic")
	ok.AddCSSClass("test-ok")
	bad := gtk.NewImageFromIconName("window-close-symbolic")
	bad.AddCSSClass("test-bad")
	b.stack.AddNamed(gtk.NewLabel(" "), "idle")
	b.stack.AddNamed(b.spin, "busy")
	b.stack.AddNamed(ok, "ok")
	b.stack.AddNamed(bad, "bad")
	return b
}

func (b *testBadge) busy() {
	b.stack.SetTooltipText("")
	b.spin.Start()
	b.stack.SetVisibleChildName("busy")
}

func (b *testBadge) done(ok bool, why string) {
	b.spin.Stop()
	b.stack.SetTooltipText(why) // the words, where the symbol is
	name := "bad"
	if ok {
		name = "ok"
	}
	b.stack.SetVisibleChildName(name)
}

// confSaveWait is how long the settings dialog waits after the last keystroke
// before it writes the file. Long enough that a typed URL is one write and not
// thirty; short enough that closing the window a moment later finds the work
// already done.
const confSaveWait = 600

func (a *App) setupDialog() {
	c := a.readConf()

	win := gtk.NewWindow()
	win.SetTransientFor(&a.win.Window)
	win.SetModal(true)
	win.SetTitle("Settings")
	win.SetDefaultSize(680, -1)

	server := gtk.NewEntry()
	server.SetText(c.Server)
	server.SetPlaceholderText(fmt.Sprintf("empty = http://127.0.0.1:%d", llmPort))
	server.SetHExpand(true)

	key := gtk.NewPasswordEntry()
	key.SetShowPeekIcon(true)
	key.SetText(c.Key)
	key.SetHExpand(true)

	model := gtk.NewEntry()
	model.SetText(c.Model)
	model.SetPlaceholderText("model id exactly as the server lists it")
	model.SetHExpand(true)

	var ids []string
	pick := gtk.NewDropDownFromStrings([]string{"(fetch models first)"})
	pick.SetHExpand(true)
	use := gtk.NewButtonWithLabel("Use")
	use.SetSensitive(false)
	use.ConnectClicked(func() {
		if i := int(pick.Selected()); i >= 0 && i < len(ids) {
			model.SetText(ids[i])
		}
	})

	// the audio.cpp server that speaks; empty means the compose service on the
	// loopback port. Naivepost only ever talks to it -- starting it is the stack's
	// job, not a side effect of opening this dialog.
	tts := gtk.NewEntry()
	tts.SetText(c.TTS)
	tts.SetPlaceholderText(fmt.Sprintf("empty = http://127.0.0.1:%d", ttsPort))
	tts.SetHExpand(true)

	// every server gets a key box, shaped like the LLM's: the local stack wants
	// none -- blank sends none -- but any of these can sit behind a proxy that
	// does, and the one that can not be authenticated is the one that ends up
	// exposed on the LAN instead
	ttsKey := gtk.NewPasswordEntry()
	ttsKey.SetShowPeekIcon(true)
	ttsKey.SetText(c.TTSKey)
	ttsKey.SetHExpand(true)

	// the sd.cpp server Produce draws its thumbnail on -- the compose
	// "sd" service by default, like the one above
	sd := gtk.NewEntry()
	sd.SetText(c.SD)
	sd.SetPlaceholderText(fmt.Sprintf("empty = http://127.0.0.1:%d", sdPort))
	sd.SetHExpand(true)

	sdKey := gtk.NewPasswordEntry()
	sdKey.SetShowPeekIcon(true)
	sdKey.SetText(c.SDKey)
	sdKey.SetHExpand(true)

	// the log: what each test asked and what came back, in order -- the badge
	// on the row says pass or fail, this says why, and it keeps saying it after
	// the next click. Mirrored into the main log so it outlives the dialog.
	logView, logScroll := newLogPane(110)
	// collapsed like the main window's log: the badges answer the usual
	// question, and the words are one click away -- except on a failure, which
	// opens it, because then the words ARE the answer
	logExp := gtk.NewExpander("Log")
	logExp.SetChild(logScroll)
	// open, the log is the dialog's one stretchy row; closed, it hands the height
	// back. Following the property keeps the failure auto-open in step. Both
	// widgets: vexpand on the expander alone wins the row height the child does
	// not fill.
	logGrow := func() {
		on := logExp.Expanded()
		logScroll.SetVExpand(on)
		logExp.SetVExpand(on)
	}
	logExp.NotifyProperty("expanded", logGrow)
	logGrow()
	slog := func(format string, args ...any) {
		s := fmt.Sprintf(format, args...)
		buf := logView.Buffer()
		end := buf.EndIter()
		buf.Insert(end, s+"\n")
		mark := buf.CreateMark("", buf.EndIter(), false)
		logView.ScrollToMark(mark, 0, false, 0, 1)
		buf.DeleteMark(mark)
		a.logf("settings: %s", s)
	}

	fetch := gtk.NewButtonWithLabel("Fetch models")
	fetch.ConnectClicked(func() {
		// llmServer, not the box: an empty box is the local default, and a
		// fetch that asked "" for its models would fail on a setup that works
		srv, k := llmServer(appConf{Server: server.Text()}), key.Text()
		slog("LLM: querying %s for its model list …", srv)
		go func() {
			got, err := fetchModels(srv, k)
			glib.IdleAdd(func() {
				if err != nil {
					logExp.SetExpanded(true)
					slog("LLM: fetch FAILED: %v", err)
					return
				}
				ids = got
				pick.SetModel(gtk.NewStringList(ids))
				use.SetSensitive(true)
				slog("LLM: %d model(s) -- pick one and press Use", len(ids))
			})
		}()
	})

	// every Test behaves the same way: read the boxes on the GUI thread, work in
	// the background, land the verdict as a badge on the row and words in the
	// log. They test what is typed, not what is saved. Each also lands in runAll
	// (Test All); a run in flight has its button greyed, and the guard reads that.
	var runAll []func()
	hook := func(btn *gtk.Button, badge *testBadge, name string, prep func() (string, func() (string, error))) {
		run := func() {
			if !btn.Sensitive() {
				return // already running; its verdict is on the way
			}
			doing, job := prep()
			slog("%s: %s", name, doing)
			btn.SetSensitive(false)
			badge.busy()
			go func() {
				got, err := job()
				glib.IdleAdd(func() {
					btn.SetSensitive(true)
					if err != nil {
						badge.done(false, err.Error())
						logExp.SetExpanded(true)
						slog("%s FAILED: %v", name, err)
						return
					}
					badge.done(true, got)
					slog("%s ok — %s", name, got)
				})
			}()
		}
		btn.ConnectClicked(run)
		runAll = append(runAll, run)
	}
	// the audio.cpp tests all aim at the same server; empty means the compose
	// service on the loopback port
	audioTarget := func() string {
		if u := serverURL(tts.Text()); u != "" {
			return u
		}
		return fmt.Sprintf("http://127.0.0.1:%d", ttsPort)
	}

	testLLMBtn := gtk.NewButtonWithLabel("Test")
	testLLMBtn.SetTooltipText("Ask this server and model for one short completion")
	llmBadge := newTestBadge()
	hook(testLLMBtn, llmBadge, "LLM", func() (string, func() (string, error)) {
		cc := appConf{Server: server.Text(), Model: model.Text(), Key: key.Text()}
		return "asking " + llmServer(cc) + " for one completion …",
			func() (string, error) { return testLLM(cc) }
	})

	// the same model, shown a picture: the describer works frames through it, and
	// whether it can see at all is the one thing the text round trip cannot say
	testVisBtn := gtk.NewButtonWithLabel("Test")
	testVisBtn.SetTooltipText("Show this model a small sample image and check it names what it sees")
	visBadge := newTestBadge()
	hook(testVisBtn, visBadge, "LLM vision", func() (string, func() (string, error)) {
		cc := appConf{Server: server.Text(), Model: model.Text(), Key: key.Text()}
		return "showing " + or(cc.Model, "the model") + " a red square …",
			func() (string, error) { return testVision(cc) }
	})

	testTTSBtn := gtk.NewButtonWithLabel("Test")
	testTTSBtn.SetTooltipText("Check that an audio.cpp server answers and serves a voice-cloning model")
	ttsBadge := newTestBadge()
	hook(testTTSBtn, ttsBadge, "TTS", func() (string, func() (string, error)) {
		url, k := audioTarget(), ttsKey.Text()
		return "asking " + url + " for a voice-cloning model …",
			func() (string, error) { return testTTS(url, k) }
	})

	// a box, because on a machine with two ffmpegs the right one is not always
	// the one PATH finds first. The placeholder is what PATH gives right now --
	// PATH here is the GUI's, not the shell's, and seeing which one that is
	// answers half the ffmpeg questions before anything is typed.
	ff := gtk.NewEntry()
	ff.SetText(c.FFmpeg)
	ff.SetHExpand(true)
	if p, err := exec.LookPath("ffmpeg"); err == nil {
		ff.SetPlaceholderText("empty = " + p)
	} else {
		ff.SetPlaceholderText("empty = off PATH, where none was found")
	}

	testFFBtn := gtk.NewButtonWithLabel("Test")
	testFFBtn.SetTooltipText("Check ffmpeg and ffprobe, and that this build has the filters and encoders the pipeline uses")
	ffBadge := newTestBadge()
	hook(testFFBtn, ffBadge, "ffmpeg", func() (string, func() (string, error)) {
		bin := strings.TrimSpace(ff.Text())
		where := "the build on PATH"
		if bin != "" {
			where = bin
		}
		return "checking " + where + " …",
			func() (string, error) { return testFFmpeg(bin) }
	})

	// Prepare's side of the same server: which of its models does which job.
	// Model ids, not paths -- what the weights are and how they run is settled
	// in config-audiocpp.json, where the server can act on it.
	entry := func(text, placeholder, tip string) *gtk.Entry {
		e := gtk.NewEntry()
		e.SetText(text)
		e.SetPlaceholderText(placeholder)
		e.SetTooltipText(tip)
		e.SetHExpand(true)
		return e
	}
	// the voices folder is deliberately not a row here either: it is one path
	// with a working default that the audio server has to agree with anyway
	// (the compose file mounts it), set once in llm.conf as AUDIOCPP_VOICES.
	// Voices are added to it with "Add file…" on the Narrate step, which is the
	// only thing the GUI does with the folder.
	ttsm := entry(c.TTSModel, defTTSModel,
		"Id of the voice-cloning model the narration is spoken through, exactly as the server lists it")
	asrModel := entry(c.ASRModel, defASRModel, "Id of the speech-to-text model, exactly as the server lists it")
	diarModel := entry(c.DiarModel, defDiarModel, "Id of the diarization model — the one that tells speakers apart")
	sepModel := entry(c.SepModel, defSepModel,
		"Id of the separation model — the one that lifts the voice off a recording")
	alignModel := entry(c.AlignModel, defAlignModel+" if the server has it",
		"Id of the forced aligner, which places a cut point on the word rather than on the "+
			"nearest silence. Empty means "+defAlignModel+" where the server declares it, and "+
			"otherwise whichever model it declares for \"align\"; name one when it serves two "+
			"others, or when the one it picks will not load")

	// the other local binary: the browser the model looks facts up through.
	// Empty is the one on PATH, "off" is no search at all -- and the
	// placeholder says which of the two an empty box means on this machine
	fxPh := "empty = off PATH, where none was found; off = no web search"
	if p, err := firefoxBin(""); err == nil {
		fxPh = "empty = " + p + "; off = no web search"
	}
	fx := entry(c.Firefox, fxPh, "Path of the firefox the model searches the web through, headless, "+
		"when a caption or a line needs a fact the footage does not show. \"off\" offers the model "+
		"no search: it then writes only what the material says.")
	testFxBtn := gtk.NewButtonWithLabel("Test")
	testFxBtn.SetTooltipText("Start that firefox headless and run one search through it")
	fxBadge := newTestBadge()
	hook(testFxBtn, fxBadge, "firefox", func() (string, func() (string, error)) {
		box := strings.TrimSpace(fx.Text())
		where := "the firefox on PATH"
		if box != "" {
			where = box
		}
		return "checking " + where + ", then searching through it headless …",
			func() (string, error) { return a.testFirefox(box) }
	})
	testTTSMBtn := gtk.NewButtonWithLabel("Test")
	testTTSMBtn.SetTooltipText("Check that the audio.cpp server serves this voice-cloning model")
	ttsmBadge := newTestBadge()
	hook(testTTSMBtn, ttsmBadge, "TTS model", func() (string, func() (string, error)) {
		url, k, id := audioTarget(), ttsKey.Text(), or(ttsm.Text(), defTTSModel)
		return fmt.Sprintf("asking %s for %q …", url, id),
			func() (string, error) { return testAudioModel(url, k, id, "clon", "clone a voice") }
	})

	testASRBtn := gtk.NewButtonWithLabel("Test")
	testASRBtn.SetTooltipText("Check that the audio.cpp server really serves this model, declared for speech-to-text")
	asrBadge := newTestBadge()
	hook(testASRBtn, asrBadge, "ASR", func() (string, func() (string, error)) {
		url, k, id := audioTarget(), ttsKey.Text(), or(asrModel.Text(), defASRModel)
		return fmt.Sprintf("asking %s for %q …", url, id),
			func() (string, error) { return testAudioModel(url, k, id, "asr", "transcribe") }
	})

	testDiarBtn := gtk.NewButtonWithLabel("Test")
	testDiarBtn.SetTooltipText("Check that the audio.cpp server really serves this model, declared for diarization")
	diarBadge := newTestBadge()
	hook(testDiarBtn, diarBadge, "diarization", func() (string, func() (string, error)) {
		url, k, id := audioTarget(), ttsKey.Text(), or(diarModel.Text(), defDiarModel)
		return fmt.Sprintf("asking %s for %q …", url, id),
			func() (string, error) {
				return testAudioModel(url, k, id, "diar", "tell speakers apart")
			}
	})

	testAlignBtn := gtk.NewButtonWithLabel("Test")
	testAlignBtn.SetTooltipText("Ask the server which model places cut points on the word, if any")
	alignBadge := newTestBadge()
	hook(testAlignBtn, alignBadge, "aligner", func() (string, func() (string, error)) {
		url, k, id := audioTarget(), ttsKey.Text(), strings.TrimSpace(alignModel.Text())
		what := "what aligns"
		if id != "" {
			what = fmt.Sprintf("%q", id)
		}
		return fmt.Sprintf("asking %s for %s …", url, what),
			func() (string, error) { return testAligner(url, k, id) }
	})

	testSepBtn := gtk.NewButtonWithLabel("Test")
	testSepBtn.SetTooltipText("Check that the audio.cpp server really serves this model, declared for separation")
	sepBadge := newTestBadge()
	hook(testSepBtn, sepBadge, "separation", func() (string, func() (string, error)) {
		url, k, id := audioTarget(), ttsKey.Text(), or(sepModel.Text(), defSepModel)
		return fmt.Sprintf("asking %s for %q …", url, id),
			func() (string, error) {
				return testAudioModel(url, k, id, "sep", "split a voice off a recording")
			}
	})

	// one Test for both boxes: the endpoint and the model are one question here
	// -- the server has exactly one model, so "does it answer" and "is it the
	// right one" are answered by the same capabilities call
	sdTarget := func() string {
		if u := serverURL(sd.Text()); u != "" {
			return u
		}
		return fmt.Sprintf("http://127.0.0.1:%d", sdPort)
	}
	testSDBtn := gtk.NewButtonWithLabel("Test")
	testSDBtn.SetTooltipText("Check that an sd.cpp server answers, can draw an image, and say which weights it has loaded")
	sdBadge := newTestBadge()
	hook(testSDBtn, sdBadge, "sd.cpp", func() (string, func() (string, error)) {
		url, k := sdTarget(), sdKey.Text()
		return fmt.Sprintf("asking %s what it has loaded …", url),
			func() (string, error) { return testSD(url, k) }
	})

	// No Save and no Cancel: a settings window you can leave without saving is
	// one you leave without saving, and nothing here is dangerous enough to
	// confirm. A change to any box writes the file a beat after typing stops --
	// a save per keystroke is thirty rewrites across one URL.
	var saveTimer glib.SourceHandle
	writeConf := func() {
		cc := appConf{Server: server.Text(), Model: model.Text(), Key: key.Text(),
			TTS:    strings.TrimRight(strings.TrimSpace(tts.Text()), "/"),
			TTSKey: ttsKey.Text(),
			// carried through, not read off a widget: there is no box for the
			// voices folder anywhere in the GUI, and Save writes the whole file
			// -- so anything not carried is silently erased on the next Save
			Voices:     c.Voices,
			ASRModel:   asrModel.Text(),
			DiarModel:  diarModel.Text(),
			SepModel:   strings.TrimSpace(sepModel.Text()),
			AlignModel: strings.TrimSpace(alignModel.Text()),
			TTSModel:   strings.TrimSpace(ttsm.Text()),
			SD:         strings.TrimRight(strings.TrimSpace(sd.Text()), "/"),
			SDKey:      sdKey.Text(),
			FFmpeg:     strings.TrimSpace(ff.Text()),
			Firefox:    strings.TrimSpace(fx.Text()),
		}
		if err := a.writeConf(cc); err != nil {
			logExp.SetExpanded(true)
			slog("save FAILED: %v", err)
			return
		}
		// a different server has a different catalog; the cached model id and
		// the "already listening on" note both belong to the old one
		a.ttsModel, a.audioNoted = "", ""
		a.setStatus("settings saved to " + confPath())
	}
	// touched is what every box calls: the pending write is pushed back, so
	// the file is written once the typing stops rather than once per letter.
	touched := func() {
		if saveTimer != 0 {
			glib.SourceRemove(saveTimer)
		}
		saveTimer = glib.TimeoutAdd(confSaveWait, func() bool {
			saveTimer = 0
			writeConf()
			return false
		})
	}
	// ...and closing the window is the other end of it: a beat that has not
	// elapsed must not lose the last thing typed
	win.ConnectCloseRequest(func() bool {
		if saveTimer != 0 {
			glib.SourceRemove(saveTimer)
			saveTimer = 0
			writeConf()
		}
		return false
	})
	for _, e := range []interface {
		ConnectChanged(func()) glib.SignalHandle
	}{server, model, tts, asrModel, diarModel, sepModel, alignModel, ttsm, sd, ff, fx} {
		e.ConnectChanged(touched)
	}
	key.ConnectChanged(touched)
	ttsKey.ConnectChanged(touched)
	sdKey.ConnectChanged(touched)

	grid := gtk.NewGrid()
	grid.SetRowSpacing(10)
	grid.SetColumnSpacing(10)
	margins(grid, 16, 16, 16, 16)
	lbl := func(s string) *gtk.Label { l := gtk.NewLabel(s); l.SetXAlign(1); return l }
	// A section is its title and an ⓘ. The explanations are worth having --
	// which API a box is expected to speak is not guessable from the box --
	// but they are read once, when something is wrong, and a page of prose
	// above every row is in the way on all the other openings. So the title
	// stays on the page and the words are on the mark beside it.
	//
	// A TOOLTIP, like every other explanation in this app. It was a button
	// with a popover: a thing to click, that stayed up until it was dismissed,
	// with selectable text in it -- so the one mark on this page that looks
	// like every other ⓘ behaved like none of them, and reading it left a blue
	// smear across the paragraph. The cost is that the endpoints in it can no
	// longer be dragged out with the mouse; they are in this file and in the
	// log, and everything else about it is better.
	head := func(title, why string, opt bool) *gtk.Box {
		l := gtk.NewLabel(title)
		l.SetXAlign(0)
		l.AddCSSClass("heading")
		info := gtk.NewImageFromIconName("help-about-symbolic")
		info.SetTooltipText(why)
		box := gtk.NewBox(gtk.OrientationHorizontal, 6)
		box.SetVAlign(gtk.AlignCenter) // it shares a line with the first row now
		box.Append(l)
		box.Append(info)
		// ...and, on the sections a session can do without, one small word
		// saying so. Two of these five have to be filled in before anything
		// runs; the other three are the steps you may simply not use, and a
		// page of identical headings says nothing about which is which.
		if opt {
			o := gtk.NewLabel("")
			o.SetMarkup("<small>optional</small>")
			o.AddCSSClass("dim-label")
			box.Append(o)
		}
		return box
	}
	// Sections are told apart by a rule across the page, not by a box around
	// each one.
	//
	// The modern form of this is the boxed list -- one rounded card per group,
	// its title above it -- and it was the wrong trade here twice over. A card
	// is a container, so each section would lay its own columns out and the
	// boxes would stop lining up down the page unless three size groups were
	// kept in step by hand; and a card's border, padding and title line cost
	// about twenty px a section on a dialog that already reaches the bottom of
	// a laptop screen with the log shut. A rule costs one px and groups just
	// as well: what it separates is the same thing the heading beside the
	// first row already says.
	row := 0
	sec := func(title, why string, opt bool) {
		if row > 0 {
			rule := gtk.NewSeparator(gtk.OrientationHorizontal)
			rule.SetMarginTop(2)
			rule.SetMarginBottom(2)
			grid.Attach(rule, 0, row, 5, 1)
			row++
		}
		grid.Attach(head(title, why, opt), 0, row, 1, 1)
	}
	// Five columns: the section, what it is, the value, the verdict, its Test.
	// The check lands beside the box it judges, before the button that made
	// it, and every server section reads the same way -- Server, API key, then
	// whatever that server is asked for, the same words for the same things.
	//
	// The section is a COLUMN, so a heading shares the line with the first row
	// under it. It used to be a row of its own spanning the whole grid: five
	// lines of the dialog spent on five short words, each with the rest of its
	// line empty, on a dialog already taller than some of the screens it opens
	// on. And every box then lines up with every other box by construction --
	// they are one column of the same grid, including the key rows, which used
	// to run wide across the columns the Test buttons are in.
	sec("Writing", "The model that describes the footage, proposes the cut and "+
		"writes the narration.\n\nExpects an OpenAI-compatible chat API: POST /v1/chat/completions, "+
		fmt.Sprintf("an empty Server meaning 127.0.0.1:%d, ", llmPort)+
		"and GET /v1/models for the Fetch models button. The key is sent as "+
		"Authorization: Bearer …; leave it empty for a server that wants none. "+
		"Test asks for one short completion; the Test beside Model shows it a small "+
		"picture, because describing footage needs a model that can see.", false)
	grid.Attach(lbl("Server:"), 1, row, 1, 1)
	grid.Attach(server, 2, row, 1, 1)
	grid.Attach(llmBadge.stack, 3, row, 1, 1)
	grid.Attach(testLLMBtn, 4, row, 1, 1)
	grid.Attach(lbl("API key:"), 1, row+1, 1, 1)
	grid.Attach(key, 2, row+1, 1, 1)
	// the model's own Test is the vision one: the server round trip is the row
	// above, and what is left to prove about the MODEL is that it can see
	grid.Attach(lbl("Model:"), 1, row+2, 1, 1)
	grid.Attach(model, 2, row+2, 1, 1)
	grid.Attach(visBadge.stack, 3, row+2, 1, 1)
	grid.Attach(testVisBtn, 4, row+2, 1, 1)
	// the list the server offers, in the columns the box and its button are
	// in: Fetch where a label goes, the list where the value goes, Use where
	// the Tests are
	grid.Attach(fetch, 1, row+3, 1, 1)
	grid.Attach(pick, 2, row+3, 1, 1)
	grid.Attach(use, 4, row+3, 1, 1)
	row += 4

	// the one local tool here: no API, a binary. Which ffmpeg answers, and what
	// it was built with, decides whether the render works at all
	sec("Cutting", "ffmpeg, which every step shells out to, and the firefox the "+
		"model searches the web through. Not servers: local binaries.\n\nLeave the ffmpeg box empty and it comes off PATH like any other tool, "+
		"which is what almost every machine wants. Give a path -- /usr/bin/ffmpeg, or a "+
		"build of your own -- and that one is used instead, with ffprobe taken from the "+
		"same folder; both are needed, and a mismatched pair is its own kind of bug.\n\n"+
		"Test runs it and checks this build has the filters and encoders the pipeline "+
		"uses: rubberband, subtitles, loudnorm, atempo, amix, adelay, alimiter, libx264, libx265, "+
		"aac, libopus. A build missing one works perfectly until the step that needs it, "+
		"which is minutes into a render.", false)
	grid.Attach(lbl("ffmpeg:"), 1, row, 1, 1)
	grid.Attach(ff, 2, row, 1, 1)
	grid.Attach(ffBadge.stack, 3, row, 1, 1)
	grid.Attach(testFFBtn, 4, row, 1, 1)
	grid.Attach(lbl("firefox:"), 1, row+1, 1, 1)
	grid.Attach(fx, 2, row+1, 1, 1)
	grid.Attach(fxBadge.stack, 3, row+1, 1, 1)
	grid.Attach(testFxBtn, 4, row+1, 1, 1)
	row += 2

	// One server, one section. Speaking and Listening were two of these, with
	// two Server boxes and two API key boxes for the same audio.cpp -- so the
	// dialog asked the same address twice and let the answers differ, which is
	// a way to spend an afternoon on a "server down" that was a typo in the
	// half nobody looked at.
	sec("Audio", "The audio.cpp server: it speaks the narration, and it does the "+
		"speech-to-text, the diarization -- who said what, and who is who -- and the "+
		"splitting of a voice off a recording. One address for all four.\n\n"+
		"Expects an OpenAI-compatible speech API (POST /v1/audio/speech), the task API "+
		"the rest go through (POST /v1/tasks/run), and GET /v1/models to check the ids. "+
		"Empty means the compose service on loopback. Naivepost only ever talks to it over "+
		"HTTP -- starting it is the job of whoever runs the stack.\n\n"+
		"The four model boxes are ids as the server lists them, not files: which weights "+
		"they are, and on which backend, is set in config-audiocpp.json. Blank means the "+
		"built-in default. The server opens the project folder itself, so it has to see "+
		"it at this same path.", true)
	grid.Attach(lbl("Server:"), 1, row, 1, 1)
	grid.Attach(tts, 2, row, 1, 1)
	grid.Attach(ttsBadge.stack, 3, row, 1, 1)
	grid.Attach(testTTSBtn, 4, row, 1, 1)
	grid.Attach(lbl("API key:"), 1, row+1, 1, 1)
	grid.Attach(ttsKey, 2, row+1, 1, 1)
	row += 2
	for i, r := range []struct {
		name  string
		w     *gtk.Entry
		btn   *gtk.Button
		badge *testBadge
	}{
		{"TTS model:", ttsm, testTTSMBtn, ttsmBadge},
		{"ASR model:", asrModel, testASRBtn, asrBadge},
		{"Diarization model:", diarModel, testDiarBtn, diarBadge},
		{"Voice split model:", sepModel, testSepBtn, sepBadge},
	} {
		grid.Attach(lbl(r.name), 1, row+i, 1, 1)
		grid.Attach(r.w, 2, row+i, 1, 1)
		grid.Attach(r.badge.stack, 3, row+i, 1, 1)
		grid.Attach(r.btn, 4, row+i, 1, 1)
	}
	row += 4

	// Empty is the ordinary answer here, unlike every box above it: the catalog
	// says which model aligns, and on a server with one that is the whole
	// answer. The box is for the two cases a rule cannot handle -- a server
	// with two aligners, where picking by name is picking blind, and one whose
	// aligner is registered but which the engine will not serve, where a rule
	// leaves nothing to do about it. Test names whichever one will be used.
	grid.Attach(lbl("Forced aligner:"), 1, row, 1, 1)
	grid.Attach(alignModel, 2, row, 1, 1)
	grid.Attach(alignBadge.stack, 3, row, 1, 1)
	grid.Attach(testAlignBtn, 4, row, 1, 1)
	row++

	// the last step's server. No model row: unlike audio.cpp above, there is
	// no model id to send per request, so there is nothing here to choose. Test
	// reports which weights it found rather than checking them against a box.
	sec("Drawing", "The stable-diffusion.cpp server that paints the thumbnail on "+
		"the Produce step. Empty means the compose service on loopback.\n\nExpects sd.cpp's own "+
		"asynchronous API, not an OpenAI-shaped one: GET /sdcpp/v1/capabilities, POST "+
		"/sdcpp/v1/img_gen for a job id, then GET /sdcpp/v1/jobs/{id} until the picture "+
		"arrives.\n\nThere is no model box: sd-server loads one model when it starts and "+
		"nothing Naivepost sends can switch it, so Test reports which weights it found "+
		"instead of holding it to a name.", true)
	grid.Attach(lbl("Server:"), 1, row, 1, 1)
	grid.Attach(sd, 2, row, 1, 1)
	grid.Attach(sdBadge.stack, 3, row, 1, 1)
	grid.Attach(testSDBtn, 4, row, 1, 1)
	grid.Attach(lbl("API key:"), 1, row+1, 1, 1)
	grid.Attach(sdKey, 2, row+1, 1, 1)
	row += 2

	// the dialog's one verb, at the right where a dialog keeps its buttons --
	// Save and Cancel used to be there and the settings save themselves now,
	// so this is what is left. Every verdict still lands on the row that owns
	// it; this button only saves eight trips up the page.
	testAll := gtk.NewButtonWithLabel("Test All")
	testAll.SetTooltipText("Run every Test on this page at once — each verdict lands beside its own row")
	testAll.ConnectClicked(func() {
		for _, run := range runAll {
			run()
		}
	})
	btns := gtk.NewBox(gtk.OrientationHorizontal, 8)
	spring := gtk.NewBox(gtk.OrientationHorizontal, 0)
	spring.SetHExpand(true)
	btns.Append(spring)
	btns.Append(testAll)
	grid.Attach(btns, 0, row, 5, 1)

	// the log is the LAST row, below even the verbs: expanded it grows downward
	// into space the dialog adds, instead of shoving the buttons off the bottom
	// of the screen while a failure is being read
	grid.Attach(logExp, 0, row+1, 5, 1)

	win.SetChild(grid)
	win.SetVisible(true)
}

// What naivepost remembers across sessions, and this machine's settings, in one
// file: anything whose answer is "this machine" -- servers, ffmpeg, and which
// project was open.
//
//	~/.config/naivepost/llm.conf        the settings, and what is remembered
//	~/.config/naivepost/prompts/        the prompts edited here (prompts.go)
//
// XDG_CONFIG_HOME when set (the tests use it). Bash-sourceable, chmod 600
// (writeGlobal). Reads and writes of the remembered half are best-effort.

// configDir is ~/.config/naivepost, or "" when there is nowhere to put it -- no
// HOME, no XDG_CONFIG_HOME. That is not an error worth reporting: it means the
// feature is off for this launch, and every caller falls back.
func configDir() string {
	dir, err := os.UserConfigDir()
	if err != nil {
		return ""
	}
	return filepath.Join(dir, "naivepost")
}

// uiSettings is the old settings.json, which held exactly this. Read, never
// written: readGlobal takes the map from here when the conf file has nothing
// to say about projects, which is once, on the launch after the merge.
type uiSettings struct {
	Projects map[string]string `json:"projects,omitempty"`
}

func settingsPath() string {
	dir := configDir()
	if dir == "" {
		return ""
	}
	return filepath.Join(dir, "settings.json")
}

// loadSettings reads the old file, or hands back an empty one. A settings file
// that has been corrupted (edited by hand, half-written by a kill -9) is
// treated the same as a missing one: the whole content is a convenience, so
// refusing to start over it would be the wrong trade.
func loadSettings() uiSettings {
	var s uiSettings
	p := settingsPath()
	if p == "" {
		return s
	}
	b, err := os.ReadFile(p)
	if err != nil {
		return s
	}
	if err := json.Unmarshal(b, &s); err != nil {
		return uiSettings{}
	}
	return s
}

// lastProject is the project file to open on startup, or "" for none. A
// remembered file that has since been renamed or deleted is not an error and
// not a dialog -- it is simply not there, and the working copy is what opens
// instead. The entry is left alone rather than pruned: an external drive that
// is not mounted this morning is the same shape as a deletion, and forgetting
// the name would make the difference permanent.
func (a *App) lastProject() string {
	p := a.readGlobal().Projects[a.root]
	if p == "" || !exists(p) {
		return ""
	}
	return p
}

// rememberProject records what the header bar now names. Called from the two
// places that assign projPath, so what the next launch opens is what this one
// last had open -- including the working copy itself, which is a decision
// ("go back to the unnamed session") and not an absence.
func (a *App) rememberProject(path string) {
	g := a.readGlobal()
	if g.Projects[a.root] == path {
		return // the startup load re-remembering what it just read
	}
	if g.Projects == nil {
		g.Projects = map[string]string{}
	}
	g.Projects[a.root] = path
	if err := a.writeGlobal(g); err != nil {
		// worth a line, not worth interrupting: the session is unaffected, the
		// only casualty is which file the NEXT launch opens
		a.logf("settings: %v", err)
	}
}
