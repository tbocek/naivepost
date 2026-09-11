package main

// The settings dialog's two Test buttons, without the dialog. Both talk to real
// servers, so both are opt-in -- but they are the same calls the buttons make,
// which makes this the way to answer "is my config broken or is the pipeline?"
// from a terminal.
//
//	NAIVEPOST_LIVE=1 go test -run TestEndpoints -v

import (
	"bytes"
	"encoding/base64"
	"image/png"
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

// TestTestTTS covers what the button is for: not "does something answer" but
// "will narration work against this". A server on the right port serving the
// wrong models is the failure that otherwise only shows up in Narrate.
func TestTestTTS(t *testing.T) {
	srv := func(models string) *httptest.Server {
		return httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			if r.URL.Path == "/health" {
				w.Write([]byte(`{"status":"ok"}`))
				return
			}
			w.Write([]byte(`{"object":"list","data":[` + models + `]}`))
		}))
	}

	good := srv(`{"id":"index-tts2","family":"index_tts2","task":"clon"}`)
	defer good.Close()
	got, err := testTTS(good.URL, "")
	if err != nil {
		t.Fatalf("a server that can clone was rejected: %v", err)
	}
	if !strings.Contains(got, "index-tts2") {
		t.Errorf("the report does not name the model it would narrate with: %q", got)
	}
	// a trailing slash is what a pasted URL usually has
	if _, err := testTTS(good.URL+"/", ""); err != nil {
		t.Errorf("a trailing slash broke the check: %v", err)
	}

	asrOnly := srv(`{"id":"parakeet","family":"parakeet","task":"asr"}`)
	defer asrOnly.Close()
	if _, err := testTTS(asrOnly.URL, ""); err == nil {
		t.Error("a server with no voice-cloning model passed the test")
	} else if !strings.Contains(err.Error(), "clone") {
		t.Errorf("unhelpful message for an ASR-only server: %v", err)
	}

	empty := srv("")
	defer empty.Close()
	if _, err := testTTS(empty.URL, ""); err == nil {
		t.Error("a server with no models at all passed the test")
	}

	dead := srv("")
	dead.Close() // nothing listening on that port any more
	if _, err := testTTS(dead.URL, ""); err == nil {
		t.Error("a dead endpoint passed the test")
	}
}

// TestAudioKeyRidesEveryCall: the API key box beside each server is only real
// if the requests carry it. A blank key sends NO header at all -- the local
// stack has no auth, and a strict proxy handed "Bearer " with nothing after it
// answers 401 to what used to work.
func TestAudioKeyRidesEveryCall(t *testing.T) {
	var auths []string
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		auths = append(auths, r.Header.Get("Authorization"))
		if r.URL.Path == "/health" {
			w.Write([]byte(`{"status":"ok"}`))
			return
		}
		w.Write([]byte(`{"object":"list","data":[{"id":"index-tts2","family":"index_tts2","task":"clon"}]}`))
	}))
	defer srv.Close()

	if _, err := testTTS(srv.URL, "sesame"); err != nil {
		t.Fatalf("a keyed test failed: %v", err)
	}
	if len(auths) < 2 { // /health and /v1/models both
		t.Fatalf("the test made %d request(s), want the probe and the catalog", len(auths))
	}
	for i, got := range auths {
		if got != "Bearer sesame" {
			t.Errorf("request %d carried Authorization %q, want the key", i, got)
		}
	}

	auths = nil
	if _, err := testTTS(srv.URL, " "); err != nil {
		t.Fatalf("a keyless test failed: %v", err)
	}
	for i, got := range auths {
		if got != "" {
			t.Errorf("with a blank key request %d still carried Authorization %q", i, got)
		}
	}
}

// TestTestAudioModel: one Test per id, aimed at the failure the run otherwise
// reports minutes too late. The GUI is standalone -- it learns everything over
// HTTP and reads no file of the server's -- so the miss message has to teach
// how a catalog grows: live on the server's own model page, or in the config it
// reads at startup and then a recreate.
func TestTestAudioModel(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path == "/health" {
			w.Write([]byte(`{"status":"ok"}`))
			return
		}
		w.Write([]byte(`{"object":"list","data":[
			{"id":"index-tts2","family":"index_tts2","task":"clon"},
			{"id":"parakeet-tdt","family":"parakeet_tdt","task":"asr"}]}`))
	}))
	defer srv.Close()

	got, err := testAudioModel(srv.URL, "", "parakeet-tdt", "asr", "transcribe")
	if err != nil {
		t.Fatalf("a served model was rejected: %v", err)
	}
	if !strings.Contains(got, "parakeet-tdt") {
		t.Errorf("the report does not name the model it proved: %q", got)
	}

	_, err = testAudioModel(srv.URL, "", "sortformer-diar", "diar", "tell speakers apart")
	if err == nil {
		t.Fatal("a missing model passed its test")
	}
	for _, want := range []string{"sortformer-diar", "index-tts2",
		"config-audiocpp.json", "force-recreate"} {
		if !strings.Contains(err.Error(), want) {
			t.Errorf("the error drops %q: %v", want, err)
		}
	}

	// the id exists but is the wrong kind of model -- a copied json entry
	if _, err := testAudioModel(srv.URL, "", "index-tts2", "asr", "transcribe"); err == nil {
		t.Error("a TTS model passed as an ASR model")
	} else if !strings.Contains(err.Error(), "clon") {
		t.Errorf("the error does not say what the model actually is: %v", err)
	}
}

// TestTestLLM checks that the two answers a broken config actually produces --
// a rejection with a reason, and a reasoning model that never gets to the
// content -- come back as errors a user can act on.
func TestTestLLM(t *testing.T) {
	reply := func(status int, body string) *httptest.Server {
		return httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			w.WriteHeader(status)
			w.Write([]byte(body))
		}))
	}

	ok := reply(200, `{"choices":[{"message":{"content":" ok "}}]}`)
	defer ok.Close()
	got, err := testLLM(appConf{Server: ok.URL, Model: "m"})
	if err != nil {
		t.Fatalf("a working server was rejected: %v", err)
	}
	if !strings.Contains(got, `"ok"`) {
		t.Errorf("the report does not quote the answer: %q", got)
	}

	denied := reply(401, `{"error":{"message":"invalid api key"}}`)
	defer denied.Close()
	if _, err := testLLM(appConf{Server: denied.URL, Model: "m"}); err == nil {
		t.Error("a rejected key passed the test")
	} else if !strings.Contains(err.Error(), "invalid api key") {
		t.Errorf("the server's own reason was dropped: %v", err)
	}

	thinking := reply(200, `{"choices":[{"message":{"content":""}}]}`)
	defer thinking.Close()
	if _, err := testLLM(appConf{Server: thinking.URL, Model: "m"}); err == nil {
		t.Error("an empty completion passed as a working model")
	} else if !strings.Contains(err.Error(), "reasoning") {
		t.Errorf("no hint about where the budget went: %v", err)
	}
}

func TestEndpointsLive(t *testing.T) {
	ownConfig(t)
	if os.Getenv("NAIVEPOST_LIVE") == "" {
		t.Skip("set NAIVEPOST_LIVE=1 to reach the configured servers")
	}
	a := &App{root: "/home/draft/git/aitools/naivepost"}
	c := a.readConf()

	t.Run("llm", func(t *testing.T) {
		if c.Server == "" {
			t.Skip("no LLM server in " + confPath())
		}
		got, err := testLLM(c)
		if err != nil {
			t.Fatalf("%s: %v", c.Server, err)
		}
		t.Log(got)
	})

	t.Run("tts", func(t *testing.T) {
		got, err := testTTS(a.audioURL(), c.TTSKey)
		if err != nil {
			t.Fatalf("%s: %v", a.audioURL(), err)
		}
		t.Logf("%s: %s", a.audioURL(), got)
	})
}

// TestFFmpegCheck runs the settings dialog's local-tools check against the
// ffmpeg this machine has. Nothing remote is contacted. A failure here is not a
// false alarm: the pipeline uses rubberband, libass and libx264 unconditionally,
// so a build without them cannot render, and finding that out from a test beats
// finding it out ten minutes into a render.
func TestFFmpegCheck(t *testing.T) {
	got, err := testFFmpeg("") // empty box = whatever PATH gives
	if err != nil {
		t.Fatalf("ffmpeg check: %v", err)
	}
	t.Log(got)
	// and the same build named outright, which is what the box holds
	ff, err := exec.LookPath("ffmpeg")
	if err != nil {
		t.Skip("no ffmpeg on PATH")
	}
	if _, err := testFFmpeg(ff); err != nil {
		t.Errorf("the build at %s passes off PATH but fails when named: %v", ff, err)
	}
	// a name that is not there fails saying so, rather than falling back to
	// PATH and reporting a build nobody asked about
	bogus := filepath.Join(t.TempDir(), "ffmpeg")
	if _, err := testFFmpeg(bogus); err == nil {
		t.Errorf("a path with no ffmpeg at it passed the check")
	} else if !strings.Contains(err.Error(), bogus) {
		t.Errorf("the failure never names the path that was asked for: %v", err)
	}
}

// TestFFMissing pins the parser both ways: a check that never reports anything
// missing would pass on every build, including the ones this is meant to catch.
func TestFFMissing(t *testing.T) {
	ff, err := exec.LookPath("ffmpeg")
	if err != nil {
		t.Skip("no ffmpeg on PATH")
	}
	m := ffMissing(ff, "-filters", []string{"amix", "no_such_filter_exists"})
	if len(m) != 1 || m[0] != "no_such_filter_exists" {
		t.Errorf("ffMissing = %v, want only the made-up name", m)
	}
	// a binary that will not run tells us nothing, so it must report everything
	// missing rather than pass by silence
	if m := ffMissing(filepath.Join(t.TempDir(), "ffmpeg"), "-filters",
		[]string{"amix", "loudnorm"}); len(m) != 2 {
		t.Errorf("unrunnable binary reported %v missing, want both", m)
	}
}

// TestConfDefaults pins that a machine with no llm.conf, or one written before
// the audio.cpp settings existed, still runs: every one of those settings used
// to be a compiled-in constant, and a blank line must mean that constant rather
// than an empty model id posted to the server.
func TestConfDefaults(t *testing.T) {
	ownConfig(t)
	a := &App{root: t.TempDir()} // no llm.conf at all
	c := a.readConf()
	if c.Voices != defVoices || c.ASRModel != defASRModel ||
		c.DiarModel != defDiarModel || c.TTSModel != defTTSModel {
		t.Errorf("a missing config did not fall back to the built-ins: %+v", c)
	}
	// a config written when the setting was the models ROOT, voices/ implied:
	// the same folder has to come out of the new field
	if err := os.WriteFile(confPath(), []byte("AUDIOCPP_MODELS=\"/srv/models\"\n"), 0o600); err != nil {
		t.Fatal(err)
	}
	if c := a.readConf(); c.Voices != "/srv/models/voices" {
		t.Errorf("the legacy models root migrated to %q, want /srv/models/voices", c.Voices)
	}
	// an old config: the LLM keys only
	if err := os.WriteFile(confPath(), []byte("LLM_SERVER=\"https://x\"\n"), 0o600); err != nil {
		t.Fatal(err)
	}
	if c := a.readConf(); c.Server != "https://x" || c.ASRModel != defASRModel {
		t.Errorf("a pre-existing config lost its defaults: %+v", c)
	}
	// a config from before the transcripts moved onto the server, and from before the
	// language became the project's: those keys are gone, and being handed one
	// must not cost the reader the rest of the file
	old := "AUDIOCPP_IMAGE=\"audio:latest\"\nAUDIOCPP_CLI=\"/opt/audiocpp_cli\"\n" +
		"AUDIOCPP_BACKEND=\"cuda\"\nAUDIOCPP_LANGUAGE=\"de\"\nAUDIOCPP_ASR_MODEL=\"whisper-large\"\n"
	if err := os.WriteFile(confPath(), []byte(old), 0o600); err != nil {
		t.Fatal(err)
	}
	if c := a.readConf(); c.ASRModel != "whisper-large" || c.TTSModel != defTTSModel {
		t.Errorf("a config with retired keys did not read the rest: %+v", c)
	}
	// ...and a blank value is the same as no value, not an empty model id
	if err := os.WriteFile(confPath(), []byte("AUDIOCPP_ASR_MODEL=\"\"\n"), 0o600); err != nil {
		t.Fatal(err)
	}
	if c := a.readConf(); c.ASRModel != defASRModel {
		t.Errorf("a cleared ASR model came back as %q, want the default", c.ASRModel)
	}
}

// TestConfRoundTrip guards the config format: the GUI must not write something
// bash cannot read back, and adding a setting must not disturb the LLM keys.
func TestConfRoundTrip(t *testing.T) {
	ownConfig(t)
	a := &App{root: t.TempDir()}
	want := appConf{
		Server: "https://ai.example.com",
		Model:  "Qwen3.6 (27B; 128k ctx; Q4_K_XL; visual)", // spaces, parens, semicolons
		Key:    "sk-not-a-real-key",
		TTS:    "http://127.0.0.1:8765",
		TTSKey: "tts-secret",
		// the other machine this is all for: other paths, other ids
		Voices:    "/srv/models/voices",
		ASRModel:  "whisper-large",
		DiarModel: "sortformer-8spk",
		TTSModel:  "kokoro-82m",
		SepModel:  "mel-band-roformer",
		SD:        "http://127.0.0.1:1234",
		SDKey:     "sd-secret",
	}
	if err := a.writeConf(want); err != nil {
		t.Fatal(err)
	}
	if got := a.readConf(); got != want {
		t.Fatalf("round trip changed the config:\n got %+v\nwant %+v", got, want)
	}
	fi, err := os.Stat(confPath())
	if err != nil {
		t.Fatal(err)
	}
	if fi.Mode().Perm() != 0o600 { // it holds an API key
		t.Errorf("%s is mode %v, want 600", confPath(), fi.Mode().Perm())
	}
	if filepath.Base(confPath()) != "llm.conf" {
		t.Errorf("the config is llm.conf, the GUI writes %s", confPath())
	}

	// the file stays sourceable so a shell can read the endpoints too: the model
	// id is full of parens and semicolons, and anything that quotes it wrongly
	// leaves the GUI working while a shell chokes on it
	out, err := exec.Command("bash", "-c",
		"source "+confPath()+`; printf '%s|%s|%s|%s' "$LLM_MODEL" "$LLM_SERVER" "$AUDIOCPP_SERVER" "$AUDIOCPP_ASR_MODEL"`).CombinedOutput()
	if err != nil {
		t.Fatalf("bash could not source the config: %v\n%s", err, out)
	}
	if got, want := string(out), want.Model+"|"+want.Server+"|"+want.TTS+"|"+want.ASRModel; got != want {
		t.Errorf("bash reads back %q, want %q", got, want)
	}

	// an empty endpoint is how the user asks for the compose default, and it has
	// to survive the round trip as empty rather than as a URL.
	want.TTS, want.SD = "", ""
	if err := a.writeConf(want); err != nil {
		t.Fatal(err)
	}
	got := a.readConf()
	if got.TTS != "" || got.SD != "" {
		t.Errorf("cleared endpoints came back as %q / %q", got.TTS, got.SD)
	}

	// SD_MODEL used to live in this file, naming the weights the Test button
	// held the server to. It is gone -- sd-server serves what it was started
	// with and nothing naivepost sent could change it -- and an old conf file
	// still carrying the key has to be read, not rejected.
	if err := os.WriteFile(confPath(),
		[]byte("SD_SERVER=\"http://127.0.0.1:1234\"\nSD_MODEL=\"Krea-2-Turbo-Q8_0\"\n"), 0o600); err != nil {
		t.Fatal(err)
	}
	if got := a.readConf(); got.SD != "http://127.0.0.1:1234" {
		t.Errorf("a conf file with the retired SD_MODEL key read back as %+v", got)
	}
}

// TestTestAllIsEveryTestOnce pins the one-button sweep to the buttons it
// presses. Test All is not a ninth test: it is the eight presses made at
// once, which is why there is nothing of its own to exercise here -- each press
// runs the hook the row's own button runs, busy rows are skipped by the same
// sensitivity that greys them, and every verdict lands on the row that owns it.
// What CAN go quietly wrong is a new Test button that never joins runAll, so
// the pins hold the shape: every hook registers itself, and the sweep walks
// the register.
func TestTestAllIsEveryTestOnce(t *testing.T) {
	b, err := os.ReadFile("setup.go")
	if err != nil {
		t.Fatal(err)
	}
	src := string(b)
	for _, want := range []string{
		// the register, and the guard that keeps a sweep from doubling a run
		"var runAll []func()",
		"runAll = append(runAll, run)",
		"if !btn.Sensitive() {",
		// the button itself, walking the register
		`testAll := gtk.NewButtonWithLabel("Test All")`,
		"for _, run := range runAll {",
	} {
		if !strings.Contains(src, want) {
			t.Errorf("setup.go does not contain %q", want)
		}
	}
	// one registration per Test button: eleven rows have one, and Fetch models
	// is not a test and must not be swept into a run against a typed-half URL
	if got := strings.Count(src, "hook(test"); got != 11 {
		t.Errorf("setup.go hooks %d test buttons, want 11 — if a row was added, "+
			"check its Test joined runAll (it does if it went through hook)", got)
	}
	// at the right, after the spring: the dialog has no Save and no Cancel to
	// sit beside any more, and a lone button belongs where a dialog keeps its
	// buttons
	i, j := strings.Index(src, "btns.Append(spring)"), strings.Index(src, "btns.Append(testAll)")
	if i < 0 || j < 0 || i > j {
		t.Error("Test All is not at the right of the button row")
	}
	// ...and there is nothing to press to keep the settings: they save
	// themselves a beat after the typing stops
	for _, gone := range []string{`gtk.NewButtonWithLabel("Save")`, `gtk.NewButtonWithLabel("Cancel")`} {
		if strings.Contains(src, gone) {
			t.Errorf("the settings dialog has a %s again -- a window you can leave "+
				"without saving is one you leave without saving", gone)
		}
	}
	for _, want := range []string{
		"saveTimer = glib.TimeoutAdd(confSaveWait, func() bool {", // the beat
		"e.ConnectChanged(touched)",                               // every box pushes it back
		"win.ConnectCloseRequest(func() bool {",                   // and closing flushes it
	} {
		if !strings.Contains(src, want) {
			t.Errorf("setup.go no longer contains %q", want)
		}
	}
}

// TestTestVision: the model the describer sends video frames to has to be able to
// SEE, and no completion of words proves that -- a text-only model, or a vision
// model served without its mmproj file, passes the text test and then fails
// minutes into a run. The probe is a generated red square; the test is whether
// the model names the colour.
func TestTestVision(t *testing.T) {
	// the probe itself is a real red square, decoded rather than trusted: a
	// probe that encoded the wrong pixels would fail every honest model
	url := visionProbe()
	const prefix = "data:image/png;base64,"
	if !strings.HasPrefix(url, prefix) {
		t.Fatalf("the probe is not a png data URL: %.40q", url)
	}
	raw, err := base64.StdEncoding.DecodeString(strings.TrimPrefix(url, prefix))
	if err != nil {
		t.Fatalf("the probe's base64 does not decode: %v", err)
	}
	img, err := png.Decode(bytes.NewReader(raw))
	if err != nil {
		t.Fatalf("the probe's png does not decode: %v", err)
	}
	r, g, b, _ := img.At(img.Bounds().Dx()/2, img.Bounds().Dy()/2).RGBA()
	if r < 0xc000 || g > 0x2000 || b > 0x2000 {
		t.Errorf("the probe's centre pixel is r=%d g=%d b=%d -- not red", r>>8, g>>8, b>>8)
	}

	// the request that carries it is shaped like the describer's own: a parts array
	// with the image as an image_url data URL
	answer := `{"choices":[{"message":{"content":" Red. "}}]}`
	var body string
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		raw, _ := io.ReadAll(r.Body)
		body = string(raw)
		w.Write([]byte(answer))
	}))
	defer srv.Close()

	got, err := testVision(appConf{Server: srv.URL, Model: "m"})
	if err != nil {
		t.Fatalf("a model that saw red was rejected: %v", err)
	}
	if !strings.Contains(got, `"Red."`) {
		t.Errorf("the report does not quote the answer: %q", got)
	}
	for _, want := range []string{`"image_url"`, "data:image/png;base64,", "colour"} {
		if !strings.Contains(body, want) {
			t.Errorf("the vision request does not carry %q", want)
		}
	}

	// a model that answers from the void: the error names the answer, so the
	// user can see whether it hallucinated or confessed -- and names the fix
	answer = `{"choices":[{"message":{"content":"I cannot see images."}}]}`
	_, err = testVision(appConf{Server: srv.URL, Model: "m"})
	if err == nil {
		t.Fatal("a blind model passed the vision test")
	}
	for _, want := range []string{"I cannot see images.", "mmproj", "frames"} {
		if !strings.Contains(err.Error(), want) {
			t.Errorf("the blind-model error drops %q: %v", want, err)
		}
	}
}

// TestTheDialogReadsTheSameWayEverywhere pins the consistency pass: every
// server row is called "Server:" (Endpoint was the same thing under a second
// name), every server has an API key row beside it, the voices folder is not a
// row at all -- it is one path in llm.conf with a working default -- and the log
// is the dialog's LAST row -- below the button row, so a failure being read grows the
// dialog downward instead of shoving Save off the bottom.
func TestTheDialogReadsTheSameWayEverywhere(t *testing.T) {
	b, err := os.ReadFile("setup.go")
	if err != nil {
		t.Fatal(err)
	}
	src := string(b)
	if strings.Contains(src, `"Endpoint:"`) {
		t.Error("setup.go still labels a server \"Endpoint:\" -- one name for one thing")
	}
	if got := strings.Count(src, `lbl("Server:")`); got != 3 {
		t.Errorf("the dialog has %d Server: rows, want 3 (LLM, audio.cpp, sd.cpp)", got)
	}
	if got := strings.Count(src, `lbl("API key:")`); got != 3 {
		t.Errorf("the dialog has %d API key rows, want 3 -- every server takes one", got)
	}
	// The voices folder has no box anywhere: it is AUDIOCPP_VOICES in llm.conf,
	// with a default the compose file agrees with. Save still has to carry it,
	// which is the trap -- writeConf writes the WHOLE file from the struct, so a
	// field left at its zero value here is not "unchanged", it is erased, and
	// the next start would list the default folder's voices instead of yours.
	if strings.Contains(src, `"Voices folder:"`) {
		t.Error("the voices folder is a row in Settings again -- it is one path with a default, set in llm.conf")
	}
	if !strings.Contains(src, "c.Voices,") {
		t.Error("Save does not carry the voices folder through, so saving anything else wipes it")
	}
	// ...and the Narrate step, which is the only page that touches the folder,
	// only ever adds one file to it
	vb, err := os.ReadFile("narrate_voice.go")
	if err != nil {
		t.Fatal(err)
	}
	for _, gone := range []string{"applyDir", "dirRow", "addVoiceDirDialog"} {
		if strings.Contains(string(vb), gone) {
			t.Errorf("narrate_voice.go still has %q -- the folder row and the folder import are gone", gone)
		}
	}
	if !strings.Contains(string(vb), "add.ConnectClicked(vp.addVoiceDialog)") {
		t.Error("nothing on the Narrate step puts a voice in the folder any more")
	}
	// the log is the last row: attached after the buttons, one grid row below
	i, j := strings.Index(src, "grid.Attach(btns"), strings.Index(src, "grid.Attach(logExp")
	if i < 0 || j < 0 || j < i {
		t.Error("the Log expander is not the dialog's lowest entry")
	}
}

// The config left the session folder, and a machine that has been cutting for
// months still has it there. It has to arrive on its own on the next launch:
// the file holds the API key and the endpoints, and "settings are gone, retype
// them" is the worst possible way to introduce a good change. Once only, and
// never over a newer file -- the old one is left where it is, so a downgrade
// still finds it.
func TestTheOldConfigBesideTheVideosArrivesOnItsOwn(t *testing.T) {
	ownConfig(t)
	root := t.TempDir()
	a := &App{root: root}
	old := filepath.Join(root, "llm.conf")
	if err := os.WriteFile(old, []byte("LLM_SERVER=\"https://old\"\nLLM_API_KEY=\"sk-secret\"\n"), 0o600); err != nil {
		t.Fatal(err)
	}

	a.migrateConf()

	if !exists(confPath()) {
		t.Fatalf("nothing arrived at %s", confPath())
	}
	if c := (&App{}).readConf(); c.Server != "https://old" || c.Key != "sk-secret" {
		t.Errorf("the endpoints came across as %+v, want the old server and key", c)
	}
	if fi, err := os.Stat(confPath()); err != nil {
		t.Fatal(err)
	} else if fi.Mode().Perm() != 0o600 {
		t.Errorf("the copy is mode %v -- it holds the API key", fi.Mode().Perm())
	}
	if !exists(old) {
		t.Error("the old file was removed; an older build would find nothing")
	}

	// and a second launch does not undo what has been changed since
	if err := (&App{root: root}).writeConf(appConf{Server: "https://new"}); err != nil {
		t.Fatal(err)
	}
	fi, err := os.Stat(confPath())
	if err != nil {
		t.Fatal(err)
	}
	a.migrateConf()
	if got := (&App{}).readConf().Server; got != "https://new" {
		t.Errorf("a later launch put the old server back: %q", got)
	}
	// untouched, not rewritten with the same bytes: the move is announced in
	// the log, and announcing it on every launch for the rest of the machine's
	// life is its own small bug
	if now, err := os.Stat(confPath()); err != nil {
		t.Fatal(err)
	} else if !now.ModTime().Equal(fi.ModTime()) {
		t.Error("a later launch rewrote the settings it had already migrated")
	}
}

// The wiring for both of the above. A migration that is never called is a
// migration that does not exist, and the same goes for the startup read of the
// prompts: without it the boxes show the shipped wording and the first autosave
// tick writes that over what this machine had. Source-level, because both live
// in build() between a GTK window and an icon theme.
func TestTheMachinesSettingsAreReadAtLaunch(t *testing.T) {
	m := readSrc(t, "main.go")
	for _, call := range []string{"a.migrateConf()", "a.loadGlobalPrompts()"} {
		if !strings.Contains(m, call) {
			t.Errorf("build() no longer calls %s", call)
		}
	}
	if strings.Index(m, "a.migrateConf()") > strings.Index(m, "a.loadGlobalPrompts()") {
		t.Error("the prompts are read before the config is moved into place, " +
			"so the first launch after the move reads the picks from a file that is not there yet")
	}
	// and the prompts are written on the same tick as the project, because
	// setPrompt fires per keystroke and a file per keystroke is not a plan
	p := readSrc(t, "project.go")
	for _, part := range strings.Split(p, "a.flushProject()")[1:] {
		if !strings.HasPrefix(strings.TrimSpace(part), "a.flushPrompts()") {
			t.Error("the project is flushed without the prompts, so an edited " +
				"wording is only kept if the session ends the other way")
		}
	}
}

// Every box on the settings page is both saved and watched.
//
// Two ways to get this wrong, and the aligner box arrived with both. Left out
// of writeConf, a box is not merely unsaved: writeConf writes the WHOLE file,
// so the next save of any other box erases what was typed in it. Left out of
// the ConnectChanged list, typing in it never asks for a save at all — the
// value sits on screen looking stored and is gone with the window.
func TestEverySettingsBoxIsSavedAndWatched(t *testing.T) {
	src := readSrc(t, "setup.go")
	// the boxes that ask for a save, read off the list that wires them
	m := regexp.MustCompile(`ConnectChanged\(func\(\)\) glib\.SignalHandle\s*\}\{([^}]*)\}`).
		FindStringSubmatch(src)
	if m == nil {
		t.Fatal("the list that wires the boxes to the autosave is gone")
	}
	watched := strings.Split(m[1], ",")

	body := funcBody(t, "setup.go", `writeConf := func\(\) \{`)
	if body == "" {
		b := strings.Index(src, "writeConf := func() {")
		e := strings.Index(src[b:], "\n\t}\n")
		body = src[b : b+e]
	}
	for _, w := range watched {
		w = strings.TrimSpace(w)
		if w == "" {
			continue
		}
		if !strings.Contains(body, w+".Text()") {
			t.Errorf("%s asks for a save but is not carried into it — the next save of "+
				"any other box erases what was typed in it", w)
		}
	}
	// ...and the other direction, for the boxes the conf holds
	for _, f := range []string{"ASRModel", "DiarModel", "SepModel", "AlignModel", "TTSModel"} {
		if !strings.Contains(body, f+":") {
			t.Errorf("writeConf does not carry %s, so it is erased on the next save", f)
		}
	}
	if !strings.Contains(src, "sepModel, alignModel, ttsm") {
		t.Error("the aligner box does not ask for a save when it is typed in")
	}
}
