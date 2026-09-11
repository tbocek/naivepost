package main

// The audio.cpp server: one endpoint for listening (ASR, diarization) and
// speaking (TTS). Paths in requests are the SERVER's -- it runs in a container
// -- so files go up via serverFile first. A model id names family, task,
// weights and session options (config-audiocpp.json), never per-request flags.

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"time"
)

// audioURL is where an audio.cpp server is expected. AUDIOCPP_SERVER is
// audio.cpp's own variable -- the WebUI reads it to talk to an already-running
// server instead of managing one -- so setting it once points both frontends at
// the same server. NAIVEPOST_TTS_URL overrides it when only naivepost should move.
// The settings dialog wins over the environment: it is the one of the two a
// user can see and clear, and a stale export in the launching shell should not
// quietly outrank what the dialog says.
func (a *App) audioURL() string {
	if u := a.configuredAudio(); u != "" {
		return u
	}
	return fmt.Sprintf("http://127.0.0.1:%d", ttsPort)
}

// configuredAudio is the server the user pointed us at, from the settings file
// or the environment; "" means the compose default on the loopback port.
func (a *App) configuredAudio() string {
	if u := a.readConf().TTS; u != "" {
		return u
	}
	for _, k := range []string{"NAIVEPOST_TTS_URL", "AUDIOCPP_SERVER"} {
		if u := strings.TrimRight(strings.TrimSpace(os.Getenv(k)), "/"); u != "" {
			return u
		}
	}
	return ""
}

// bearer puts an API key on a request, and a blank key nowhere. It is the only
// place this header is built -- the audio server, the LLM and both Settings
// probes all come through here -- because the rule is easy to forget one client
// at a time: the local stack has no auth, and sending "Bearer " with nothing
// after it is a header that means nothing at best and a 401 from a strict proxy
// at worst. An empty key is "this server wants none", not "the key is empty".
func bearer(req *http.Request, key string) {
	if k := strings.TrimSpace(key); k != "" {
		req.Header.Set("Authorization", "Bearer "+k)
	}
}

// serverFile uploads a local file (POST /v1/ui/upload) and returns the
// server-side path every request must name: the server only sees its own
// container. Uploaded every time -- a remembered path dies with a restart.
func (a *App) serverFile(path string) (string, error) {
	// before the file, not after: this is now the first request of any job, so
	// "nothing is listening" has to be answered here or it arrives as a bare
	// dial error from a POST nobody was expecting to be the first one
	if err := a.ensureAudioServer(); err != nil {
		return "", err
	}
	f, err := os.Open(path)
	if err != nil {
		return "", err
	}
	defer f.Close()
	fi, err := f.Stat()
	if err != nil {
		return "", err
	}
	ctx := a.runCtx
	if ctx == nil {
		ctx = context.Background()
	}
	req, err := http.NewRequestWithContext(ctx, "POST", a.audioURL()+"/v1/ui/upload", f)
	if err != nil {
		return "", err
	}
	// the body is the file itself, not a form. Setting the length keeps Go from
	// chunking it, which the server's own HTTP parser does not read.
	req.ContentLength = fi.Size()
	req.Header.Set("Content-Type", "application/octet-stream")
	req.Header.Set("X-Audiocpp-Filename", filepath.Base(path))
	bearer(req, a.readConf().TTSKey)
	resp, err := (&http.Client{}).Do(req)
	if err != nil {
		if a.stopFlag.Load() {
			return "", errStopped
		}
		return "", err
	}
	defer resp.Body.Close()
	out, _ := io.ReadAll(resp.Body)
	if resp.StatusCode == 403 {
		return "", fmt.Errorf("%s will not take uploads, so it can only read files it "+
			"already has: start it with --ui-management (cpp/docker-compose.yml, the "+
			"audio service's command), or mount %s into the container at that same path",
			a.audioURL(), filepath.Dir(path))
	}
	if resp.StatusCode != 200 {
		return "", fmt.Errorf("uploading %s to %s answered %s: %.400s",
			filepath.Base(path), a.audioURL(), resp.Status, strings.TrimSpace(string(out)))
	}
	var got struct {
		Path string `json:"path"`
	}
	if err := json.Unmarshal(out, &got); err != nil || got.Path == "" {
		return "", fmt.Errorf("%s took %s but did not say where it put it: %.200s",
			a.audioURL(), filepath.Base(path), strings.TrimSpace(string(out)))
	}
	return got.Path, nil
}

// ensureAudioServer checks that something is listening, and says so when
// nothing is. Naivepost never starts a server: one GPU, started by whoever owns
// the stack. The compose "audio" service runs audiocpp_server and points the
// WebUI at it, so bringing that up is what makes narration -- and now the
// transcripts -- work.
func (a *App) ensureAudioServer() error {
	url := a.audioURL()
	req, err := http.NewRequest("GET", url+"/health", nil)
	if err != nil {
		return err
	}
	bearer(req, a.readConf().TTSKey)
	r, err := http.DefaultClient.Do(req)
	if err != nil {
		return fmt.Errorf("no audio.cpp server answering at %s -- start it "+
			"(in the stack's compose folder: docker compose up -d audio), or point "+
			"Settings at where one runs", url)
	}
	r.Body.Close()
	if a.audioNoted != url {
		a.audioNoted, a.ttsModel = url, "" // a different server, a different catalog
		a.logfIdle(">>> using the audio.cpp server on %s", url)
	}
	return nil
}

// audioModel is one entry of the server's catalog: the id we ask for, and what
// it turns out to be.
type audioModel struct {
	ID     string `json:"id"`
	Family string `json:"family"`
	Task   string `json:"task"`
}

// audioCatalog is what the server serves right now, by id.
func audioCatalog(url, key string) (map[string]audioModel, error) {
	req, err := http.NewRequest("GET", strings.TrimRight(url, "/")+"/v1/models", nil)
	if err != nil {
		return nil, err
	}
	bearer(req, key)
	r, err := http.DefaultClient.Do(req)
	if err != nil {
		return nil, err
	}
	defer r.Body.Close()
	var out struct {
		Data []audioModel `json:"data"`
	}
	if err := json.NewDecoder(r.Body).Decode(&out); err != nil {
		return nil, fmt.Errorf("model list from %s: %w", url, err)
	}
	got := map[string]audioModel{}
	for _, m := range out.Data {
		got[m.ID] = m
	}
	return got, nil
}

// sortedKeys is a map read in an order that does not change between runs --
// for an error message that must not reshuffle itself, and for a config file
// that must not (rememberedBody).
func sortedKeys[V any](m map[string]V) []string {
	ids := make([]string, 0, len(m))
	for id := range m {
		ids = append(ids, id)
	}
	sort.Strings(ids)
	return ids
}

// catalogIDs names the catalog for an error message: what a server offers is
// half of why the model you asked for is not there.
func catalogIDs(cat map[string]audioModel) string {
	if len(cat) == 0 {
		return "nothing"
	}
	return strings.Join(sortedKeys(cat), ", ")
}

// freeAudioModels asks the audio server to let go of everything it is holding.
//
// It loads a model on first use and then keeps it, for as long as it runs. So a
// Prepare that separates, transcribes, aligns and diarizes leaves four of them
// resident afterwards -- gigabytes of a shared GPU held for work that finished
// -- and the next thing that needs room competes with models nobody is going
// to ask another question of until the user presses something. That press can
// afford the reload; the machine cannot afford the wait.
//
// Quiet on purpose, and it never starts a server to say this. A run that used
// no audio at all still ends here, and a server that is not running is already
// holding nothing: both are the same silence.
func (a *App) freeAudioModels() {
	req, err := http.NewRequest("POST", a.audioURL()+"/v1/tasks/unload_all_models", nil)
	if err != nil {
		return
	}
	bearer(req, a.readConf().TTSKey)
	// a deadline, unlike every other call here: this one is housekeeping after
	// the answer the user wanted is already on disk, and nothing waits on it
	r, err := (&http.Client{Timeout: 20 * time.Second}).Do(req)
	if err != nil {
		return
	}
	r.Body.Close()
}

// audioRun posts one job and hands back the answer verbatim. Verbatim matters:
// what comes back is what gets written to words.json and turns.json, and every
// reader of those walks whatever shape it finds rather than a fixed one.
//
// No client timeout on purpose. An hour of audio is an hour of work, and the
// only honest deadline is the user's: the request rides a.runCtx, so stop
// aborts it mid-flight the way it used to kill the container.
func (a *App) audioRun(model string, req map[string]any) ([]byte, error) {
	if strings.TrimSpace(model) == "" {
		return nil, fmt.Errorf("no model id configured -- set one in Settings")
	}
	if err := a.ensureAudioServer(); err != nil {
		return nil, err
	}
	body, err := json.Marshal(map[string]any{"model": model, "request": req})
	if err != nil {
		return nil, err
	}
	ctx := a.runCtx
	if ctx == nil {
		ctx = context.Background()
	}
	hreq, err := http.NewRequestWithContext(ctx, "POST",
		a.audioURL()+"/v1/tasks/run", bytes.NewReader(body))
	if err != nil {
		return nil, err
	}
	hreq.Header.Set("Content-Type", "application/json")
	bearer(hreq, a.readConf().TTSKey)
	resp, err := (&http.Client{}).Do(hreq)
	if err != nil {
		if a.stopFlag.Load() {
			return nil, errStopped
		}
		return nil, err
	}
	defer resp.Body.Close()
	out, err := io.ReadAll(resp.Body)
	if err != nil {
		if a.stopFlag.Load() {
			return nil, errStopped
		}
		return nil, err
	}
	if resp.StatusCode != 200 {
		// the server answers errors as JSON, but a proxy in between might not:
		// print what came back, trimmed, rather than a status alone
		return nil, fmt.Errorf("%s on %s answered %s: %.400s",
			model, a.audioURL(), resp.Status, strings.TrimSpace(string(out)))
	}
	return out, nil
}

// asrJSON transcribes one wav: the whole answer (words.json) and its plain text.
// Language comes off the project, not llm.conf. An answer with no words is a
// silent recording, not an error; misconfiguration is caught earlier
// (ensureAudioModels).
func (a *App) asrJSON(wav string) ([]byte, string, error) {
	c := a.readConf()
	up, err := a.serverFile(wav)
	if err != nil {
		return nil, "", err
	}
	body, err := a.audioRun(c.ASRModel, map[string]any{
		"audio": up, "language": a.asrLanguage()})
	if err != nil {
		return nil, "", err
	}
	var out struct {
		Text string `json:"text"`
	}
	if err := json.Unmarshal(body, &out); err != nil {
		return nil, "", fmt.Errorf("unreadable answer from %s: %w", c.ASRModel, err)
	}
	return body, out.Text, nil
}

// diarSpans runs one clip through diarization. A clip with no speech is not a
// failure: it comes back with no turns, and no turns is silence.
func (a *App) diarSpans(wav string) ([]span, error) {
	c := a.readConf()
	up, err := a.serverFile(wav)
	if err != nil {
		return nil, err
	}
	body, err := a.audioRun(c.DiarModel, map[string]any{"audio": up})
	if err != nil {
		return nil, err
	}
	return spansFrom(body)
}

// ensureAudioModels is Prepare's preflight: "no such model" is worth hearing
// before minutes of ffmpeg. The separation model is only checked when a row
// asked to be split -- most servers do not have it.
func (a *App) ensureAudioModels(sep bool) error {
	if err := a.ensureAudioServer(); err != nil {
		return err
	}
	c := a.readConf()
	cat, err := audioCatalog(a.audioURL(), c.TTSKey)
	if err != nil {
		return err
	}
	wants := []struct{ id, def, task, pkg string }{
		{c.ASRModel, defASRModel, "asr", "nemotron_asr_q8_0"},
		{c.DiarModel, defDiarModel, "diar", "sortformer_diar_4spk_v1_q8_0"},
	}
	if sep {
		wants = append(wants, struct{ id, def, task, pkg string }{
			c.SepModel, defSepModel, "sep", "bs_roformer_q8_0"})
	}
	for _, want := range wants {
		m, ok := cat[want.id]
		if !ok {
			// the package name only fits the model this ships expecting; for a
			// hand-picked id, saying which weights to fetch would be a guess
			how := ""
			if want.id == want.def {
				how = fmt.Sprintf(" (the weights install with: docker compose exec audio "+
					"python3 tools/model_manager_v2.py install %s --models-root models)", want.pkg)
			}
			return fmt.Errorf("the audio.cpp server at %s serves %s, but not %q -- add it on "+
				"that server's own model page in the browser, or in the config-audiocpp.json "+
				"it reads at startup followed by docker compose up -d --force-recreate audio%s",
				a.audioURL(), catalogIDs(cat), want.id, how)
		}
		if m.Task != "" && m.Task != want.task {
			return fmt.Errorf("%q on %s is declared task %q, but Prepare needs %q there",
				want.id, a.audioURL(), m.Task, want.task)
		}
	}
	return nil
}
