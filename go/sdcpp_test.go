package main

import (
	"context"
	"encoding/base64"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"testing"
	"time"
)

// sdFake is sd-server's native API, small enough to reason about: it answers
// capabilities, takes a job, reports it queued for a poll or two, and then
// hands back the picture. Everything the real one does that matters here.
type sdFake struct {
	mu       sync.Mutex
	caps     sdCaps
	polls    int  // how many times the job has been asked about
	after    int  // ...before it completes
	fail     bool // finish "failed" instead
	got      sdRequest
	cancels  int
	forgotAt int // answer 404 from this poll on; 0 never
}

func (f *sdFake) serve(t *testing.T) *httptest.Server {
	t.Helper()
	// the real interval is a second, which is right for a job that takes thirty
	// and wrong for a suite that has to stay under a couple of seconds
	old := sdPollEvery
	sdPollEvery = time.Millisecond
	t.Cleanup(func() { sdPollEvery = old })
	mux := http.NewServeMux()
	mux.HandleFunc("/sdcpp/v1/capabilities", func(w http.ResponseWriter, r *http.Request) {
		f.mu.Lock()
		defer f.mu.Unlock()
		json.NewEncoder(w).Encode(f.caps)
	})
	mux.HandleFunc("/sdcpp/v1/img_gen", func(w http.ResponseWriter, r *http.Request) {
		f.mu.Lock()
		defer f.mu.Unlock()
		json.NewDecoder(r.Body).Decode(&f.got)
		f.polls = 0
		w.WriteHeader(202)
		json.NewEncoder(w).Encode(map[string]any{"id": "job-1", "status": "queued"})
	})
	mux.HandleFunc("/sdcpp/v1/jobs/job-1/cancel", func(w http.ResponseWriter, r *http.Request) {
		f.mu.Lock()
		f.cancels++
		f.mu.Unlock()
		w.WriteHeader(200)
	})
	mux.HandleFunc("/sdcpp/v1/jobs/job-1", func(w http.ResponseWriter, r *http.Request) {
		f.mu.Lock()
		defer f.mu.Unlock()
		f.polls++
		switch {
		case f.forgotAt > 0 && f.polls >= f.forgotAt:
			w.WriteHeader(404)
			w.Write([]byte(`{"error":{"code":"not_found","message":"no such job"}}`))
		case f.polls < f.after:
			json.NewEncoder(w).Encode(map[string]any{
				"id": "job-1", "status": "queued", "queue_position": f.after - f.polls})
		case f.fail:
			json.NewEncoder(w).Encode(map[string]any{"id": "job-1", "status": "failed",
				"error": map[string]string{"code": "oom", "message": "out of memory"}})
		default:
			json.NewEncoder(w).Encode(map[string]any{"id": "job-1", "status": "completed",
				"result": map[string]any{"images": []map[string]string{
					{"b64_json": base64.StdEncoding.EncodeToString([]byte("PNGDATA"))}}}})
		}
	})
	srv := httptest.NewServer(mux)
	t.Cleanup(srv.Close)
	return srv
}

// sdApp points an App at a fake server through the settings file, which is the
// path the real one takes: sdURL reads llm.conf before it reads the
// environment, precisely so that what Settings says wins.
func sdApp(t *testing.T, url string) *App {
	t.Helper()
	a := &App{root: t.TempDir()}
	if err := a.writeConf(appConf{SD: url}); err != nil {
		t.Fatal(err)
	}
	return a
}

// The picture arrives through a poll loop, not through the POST, because a
// diffusion job takes tens of seconds and a request held open that long dies in
// a proxy. What comes back has to be the decoded bytes, and the queue position
// has to reach the page while it waits -- a frozen page during the one slow
// step of the run is what the sentence is for.
func TestSDGenerateWaitsForTheJobAndReportsTheQueue(t *testing.T) {
	f := &sdFake{after: 3}
	a := sdApp(t, f.serve(t).URL)

	var said []string
	img, err := a.sdGenerate(context.Background(), sdRequest{Prompt: "a door", Seed: -1},
		func(s string) { said = append(said, s) })
	if err != nil {
		t.Fatalf("sdGenerate: %v", err)
	}
	if string(img) != "PNGDATA" {
		t.Errorf("got %q, want the decoded image bytes", img)
	}
	if len(said) == 0 {
		t.Fatal("nothing was reported while the job was queued; the page would just freeze")
	}
	if !strings.Contains(said[0], "ahead in the queue") {
		t.Errorf("the wait said %q, which does not say where in the queue it is", said[0])
	}
}

// A job that fails has to come back with the server's own words. "the thumbnail
// failed" is not a message anyone can act on; "out of memory" is.
func TestSDGenerateReportsTheServersOwnError(t *testing.T) {
	f := &sdFake{after: 1, fail: true}
	a := sdApp(t, f.serve(t).URL)
	_, err := a.sdGenerate(context.Background(), sdRequest{Prompt: "x"}, nil)
	if err == nil {
		t.Fatal("a failed job came back as a success")
	}
	if !strings.Contains(err.Error(), "out of memory") || !strings.Contains(err.Error(), "oom") {
		t.Errorf("error was %q, want the server's code and message", err)
	}

	// a server restarted mid-draw forgets the job, and that is a different
	// sentence: nothing is wrong with the request
	f2 := &sdFake{after: 99, forgotAt: 1}
	a2 := sdApp(t, f2.serve(t).URL)
	_, err = a2.sdGenerate(context.Background(), sdRequest{Prompt: "x"}, nil)
	if err == nil || !strings.Contains(err.Error(), "forgot job") {
		t.Errorf("a vanished job said %v, want something about the server forgetting it", err)
	}
}

// ⏹ has to give the card back. A diffusion job nobody is waiting for still
// holds the GPU, and the next press of ▶ would queue behind the abandoned one --
// which looks, from the page, exactly like a server that has hung.
func TestStoppingCancelsTheJobOnTheServer(t *testing.T) {
	f := &sdFake{after: 1 << 30} // never finishes on its own
	a := sdApp(t, f.serve(t).URL)
	ctx, cancel := context.WithCancel(context.Background())
	go func() { time.Sleep(20 * time.Millisecond); cancel() }()
	_, err := a.sdGenerate(ctx, sdRequest{Prompt: "x"}, nil)
	if err != errStopped {
		t.Fatalf("a stopped draw returned %v, want errStopped", err)
	}
	// the cancel is best effort and fired from the same goroutine, so it has
	// already happened by the time sdGenerate returns
	f.mu.Lock()
	defer f.mu.Unlock()
	if f.cancels == 0 {
		t.Error("the job was abandoned without telling the server, so it keeps the card")
	}
}

// What is NOT in the request body is the point. sd-server is started with the
// steps, cfg, sampler and flow-shift that suit the weights it loaded (run.sh);
// sending a textbook cfg from here would silently override them, and the two
// checkpoints this has run against want values three apart. So the JSON must
// carry what the page decided and nothing else.
func TestSDRequestLeavesTheServersOwnDefaultsAlone(t *testing.T) {
	f := &sdFake{after: 1}
	a := sdApp(t, f.serve(t).URL)
	if _, err := a.sdGenerate(context.Background(), sdRequest{
		Prompt: "a door", Negative: "watermark", Width: 1280, Height: 720,
		RefImages: []string{"data:image/png;base64,eA=="}, AutoResizeRef: true,
		Seed: -1, Format: "png"}, nil); err != nil {
		t.Fatal(err)
	}
	b, err := json.Marshal(sdRequest{Prompt: "a door", Seed: -1})
	if err != nil {
		t.Fatal(err)
	}
	for _, forbidden := range []string{"steps", "cfg_scale", "sample_params", "sampling_method", "model"} {
		if strings.Contains(string(b), forbidden) {
			t.Errorf("the request body carries %q; the server's startup settings are the ones that "+
				"suit its weights, and this would override them", forbidden)
		}
	}
	f.mu.Lock()
	defer f.mu.Unlock()
	if f.got.Prompt != "a door" || len(f.got.RefImages) != 1 || f.got.Width != 1280 {
		t.Errorf("the server received %+v, not what the page asked for", f.got)
	}
}

// The frames are what make the thumbnail this video's rather than the genre's,
// and each travels as a data URL so the type rides with it -- the frames are
// jpeg, the output is png.
func TestTheBaseFrameTravelsAsADataURL(t *testing.T) {
	dir := t.TempDir()
	jpg := filepath.Join(dir, "f.jpg")
	if err := os.WriteFile(jpg, []byte("JPEGBYTES"), 0o644); err != nil {
		t.Fatal(err)
	}
	got, err := sdRefImage(jpg)
	if err != nil {
		t.Fatal(err)
	}
	if !strings.HasPrefix(got, "data:image/jpeg;base64,") {
		t.Errorf("a .jpg became %q", short(got))
	}
	if dec, err := base64.StdEncoding.DecodeString(strings.TrimPrefix(got,
		"data:image/jpeg;base64,")); err != nil || string(dec) != "JPEGBYTES" {
		t.Errorf("the payload did not survive: %q %v", dec, err)
	}
	png := filepath.Join(dir, "f.PNG")
	if err := os.WriteFile(png, []byte("x"), 0o644); err != nil {
		t.Fatal(err)
	}
	if got, _ := sdRefImage(png); !strings.HasPrefix(got, "data:image/png;base64,") {
		t.Errorf("a .PNG became %q -- the extension test is case sensitive", short(got))
	}
	if _, err := sdRefImage(filepath.Join(dir, "gone.jpg")); err == nil {
		t.Error("a missing frame came back as an image")
	}
}

// The Settings Test button. sd-server loads ONE model when it starts and its
// requests have no model field, so there is nothing for naivepost to choose and
// nothing to hold the server to. What the button owes the user is the name of
// what is actually loaded -- that is what tells them whether the thumbnail is
// about to be drawn by the weights they think it is.
func TestTheSettingsProbeChecksTheLoadedModel(t *testing.T) {
	f := &sdFake{}
	f.caps.Model.Stem = "qwen-image-edit-2511-Q8_0"
	f.caps.Model.Path = "/mnt/models/sd/unet/qwen-image-edit-2511-Q8_0.gguf"
	f.caps.Modes = []string{"img_gen", "img_edit"}
	url := f.serve(t).URL

	msg, err := testSD(url, "")
	if err != nil {
		t.Fatalf("a healthy server was rejected: %v", err)
	}
	if !strings.Contains(msg, "qwen-image-edit-2511-Q8_0") {
		t.Errorf("the success said %q without naming the loaded model", msg)
	}
	if !strings.Contains(msg, "/mnt/models/sd/unet/") {
		t.Errorf("the success said %q without the path -- two builds of the same stem "+
			"are told apart by where they came from", msg)
	}

	// a server with no weights, and one that cannot draw at all: both answer
	// happily and then fail every job, which is exactly what a probe is for
	var blank sdFake
	if _, err := testSD(blank.serve(t).URL, ""); err == nil ||
		!strings.Contains(err.Error(), "no loaded model") {
		t.Errorf("a server with no model passed the probe: %v", err)
	}
	var vid sdFake
	vid.caps.Model.Stem = "wan2.2"
	vid.caps.Modes = []string{"vid_gen"}
	if _, err := testSD(vid.serve(t).URL, ""); err == nil ||
		!strings.Contains(err.Error(), "img_gen") {
		t.Errorf("a video-only server passed the image probe: %v", err)
	}
	// nothing there at all is the most common case of the four
	if _, err := testSD("http://127.0.0.1:1", ""); err == nil {
		t.Error("a dead endpoint passed the probe")
	}
}

// The usual wrong tenant of port 1234: an OpenAI-shaped server (LM Studio's
// default port is 1234 too). It has no /sdcpp/v1/capabilities, but it does
// answer /v1/models -- so the probe can name what is actually there, and the
// verdict says squarely that it is not sd-server rather than reading as a
// mysterious 404.
func TestTheProbeNamesAnOpenAIShapedSquatter(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path == "/v1/models" {
			w.Write([]byte(`{"object":"list","data":[{"id":"qwen2.5-coder-32b"}]}`))
			return
		}
		http.NotFound(w, r)
	}))
	defer srv.Close()
	_, err := testSD(srv.URL, "")
	if err == nil {
		t.Fatal("an OpenAI-shaped server passed the sd.cpp probe")
	}
	for _, want := range []string{"qwen2.5-coder-32b", "not sd.cpp's sd-server"} {
		if !strings.Contains(err.Error(), want) {
			t.Errorf("the verdict drops %q: %v", want, err)
		}
	}
}

// Where the endpoint comes from, in order. The settings file wins over the
// environment for the same reason it does for audio.cpp: it is the one of the
// two the user can see and clear from inside the program.
func TestTheImageServerIsSettingsThenEnvThenCompose(t *testing.T) {
	ownConfig(t)
	a := &App{root: t.TempDir()}
	t.Setenv("SD_SERVER", "")
	if got, want := a.sdURL(), "http://127.0.0.1:1234"; got != want {
		t.Errorf("with nothing set, sdURL = %q, want the compose default %q", got, want)
	}
	t.Setenv("SD_SERVER", "http://env:1234/")
	if got := a.sdURL(); got != "http://env:1234" {
		t.Errorf("the environment gave %q (and its trailing slash has to go)", got)
	}
	if err := a.writeConf(appConf{SD: "http://settings:1234"}); err != nil {
		t.Fatal(err)
	}
	if got := a.sdURL(); got != "http://settings:1234" {
		t.Errorf("Settings did not win over the environment: %q", got)
	}
}
