package main

// The sd.cpp server (leejet/stable-diffusion.cpp sd-server, the compose file's
// "sd" service), talked to over HTTP like audio.cpp. Of its three API families
// this uses the native /sdcpp/v1: the only one that takes an init_image, and
// the thumbnail starts from a frame of the video. It is asynchronous -- POST
// img_gen answers 202 with a job id, GET jobs/{id} carries the picture when
// "completed" -- because a diffusion job outlives a proxy's read timeout. ONE
// model, chosen at server start: the Settings name is a check, not a choice.

import (
	"bytes"
	"context"
	"encoding/base64"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"os"
	"strings"
	"time"
)

// sdPort is where the compose "sd" service listens. Same convention as
// ttsPort: a blank endpoint in Settings means this, on the loopback.
const sdPort = 1234

// sdURL is the server the thumbnail is drawn by. The settings file wins over
// the environment, for the same reason it does for audio.cpp: it is the one of
// the two the user can see and clear.
func (a *App) sdURL() string {
	if u := serverURL(a.readConf().SD); u != "" {
		return u
	}
	if u := serverURL(os.Getenv("SD_SERVER")); u != "" {
		return u
	}
	return fmt.Sprintf("http://127.0.0.1:%d", sdPort)
}

// sdCaps is what /sdcpp/v1/capabilities says about the running server. Only
// the parts naivepost can act on are decoded: which weights are loaded, and
// whether this build can generate an image at all -- a server started on a
// video model answers happily and then fails every img_gen.
type sdCaps struct {
	Model struct {
		Name string `json:"name"`
		Stem string `json:"stem"`
		Path string `json:"path"`
	} `json:"model"`
	Modes []string `json:"supported_modes"`
}

// modelName is the loaded model as a human would name it: the file's stem when
// the server reports one, since that is what the Settings box is filled with.
func (c sdCaps) modelName() string {
	if s := strings.TrimSpace(c.Model.Stem); s != "" {
		return s
	}
	return strings.TrimSpace(c.Model.Name)
}

func (c sdCaps) canImage() bool {
	for _, m := range c.Modes {
		if m == "img_gen" {
			return true
		}
	}
	// an older build that does not report modes is not a broken one: assume it
	// can draw, and let the job say otherwise
	return len(c.Modes) == 0
}

func sdCapabilities(ctx context.Context, url, key string) (sdCaps, error) {
	var caps sdCaps
	req, err := http.NewRequestWithContext(ctx, "GET",
		strings.TrimRight(url, "/")+"/sdcpp/v1/capabilities", nil)
	if err != nil {
		return caps, err
	}
	bearer(req, key)
	resp, err := (&http.Client{Timeout: 15 * time.Second}).Do(req)
	if err != nil {
		return caps, fmt.Errorf("nothing answering: %w", err)
	}
	defer resp.Body.Close()
	if resp.StatusCode != 200 {
		return caps, fmt.Errorf("%s does not look like an sd.cpp server: it answered %s",
			url, resp.Status)
	}
	if err := json.NewDecoder(resp.Body).Decode(&caps); err != nil {
		return caps, fmt.Errorf("%s answered something that is not sd.cpp capabilities: %w", url, err)
	}
	return caps, nil
}

// sdRequest is one image job. Deliberately small: what is not here is left out
// of the JSON entirely, so the server applies the defaults it was STARTED with
// -- and those matter, because steps, cfg-scale, sampler and flow-shift belong
// to the checkpoint and are passed to sd-server on its command line (see
// SD_ARGS in cpp/run.sh). Sending a textbook cfg of 7 here would quietly
// override the value the loaded model actually wants.
type sdRequest struct {
	Prompt   string `json:"prompt"`
	Negative string `json:"negative_prompt,omitempty"`
	Width    int    `json:"width,omitempty"`
	Height   int    `json:"height,omitempty"`
	Seed     int64  `json:"seed"`
	// The pictures the instruction talks about, as data URLs, in order: "the
	// first image" is RefImages[0]. Not init_image: an edit model is conditioned
	// on references and rewrites the canvas from the instruction; img2img
	// renoises everything, and no strength survives it.
	RefImages []string `json:"ref_images,omitempty"`
	// Let the server fit references to the output size. The frames are 16:9 and
	// so is the thumbnail, so this normally does nothing -- but a hand-picked
	// frame from some other source should not be able to fail the job.
	AutoResizeRef bool   `json:"auto_resize_ref_image"`
	Format        string `json:"output_format,omitempty"`
}

// sdRefImage reads a frame into the field above. Any image field takes a raw
// base64 string or a data URL; the data URL is used because it carries the
// type, and the frames are jpeg while the output is png.
func sdRefImage(path string) (string, error) {
	b, err := os.ReadFile(path)
	if err != nil {
		return "", err
	}
	kind := "jpeg"
	if strings.HasSuffix(strings.ToLower(path), ".png") {
		kind = "png"
	}
	return "data:image/" + kind + ";base64," + base64.StdEncoding.EncodeToString(b), nil
}

// sdJob is a job as the server reports it, at submission and at every poll.
type sdJob struct {
	ID     string `json:"id"`
	Status string `json:"status"`
	Queue  int    `json:"queue_position"`
	Result *struct {
		Images []struct {
			B64 string `json:"b64_json"`
		} `json:"images"`
	} `json:"result"`
	Error *struct {
		Code    string `json:"code"`
		Message string `json:"message"`
	} `json:"error"`
}

// sdGenerate submits one image job and waits for the picture. onWait (may be
// nil) gets a human sentence per poll. The context is the run's, so ⏹ ends
// the wait, and the job is then cancelled on the server -- it would otherwise
// hold the card ahead of the next ▶.
func (a *App) sdGenerate(ctx context.Context, req sdRequest, onWait func(string)) ([]byte, error) {
	url := a.sdURL()
	key := a.readConf().SDKey // read once; the same key rides every request of the job
	body, err := json.Marshal(req)
	if err != nil {
		return nil, err
	}
	post, err := http.NewRequestWithContext(ctx, "POST",
		url+"/sdcpp/v1/img_gen", bytes.NewReader(body))
	if err != nil {
		return nil, err
	}
	post.Header.Set("Content-Type", "application/json")
	bearer(post, key)
	// Generous, and it is only the submission: the drawing happens after the
	// 202, in the polling loop below.
	resp, err := (&http.Client{Timeout: 60 * time.Second}).Do(post)
	if err != nil {
		return nil, fmt.Errorf("sd.cpp at %s: %w", url, err)
	}
	raw, _ := io.ReadAll(resp.Body)
	resp.Body.Close()
	var job sdJob
	if err := json.Unmarshal(raw, &job); err != nil || job.ID == "" {
		return nil, fmt.Errorf("sd.cpp answered %s: %s", resp.Status, sdSnippet(raw))
	}
	if resp.StatusCode != 200 && resp.StatusCode != 202 {
		return nil, fmt.Errorf("sd.cpp refused the job (%s): %s", resp.Status, sdJobError(job, raw))
	}

	client := &http.Client{Timeout: 30 * time.Second}
	for {
		select {
		case <-ctx.Done():
			a.sdCancel(url, job.ID)
			return nil, errStopped
		case <-time.After(sdPollEvery):
		}
		get, err := http.NewRequestWithContext(ctx, "GET",
			url+"/sdcpp/v1/jobs/"+job.ID, nil)
		if err != nil {
			return nil, err
		}
		bearer(get, key)
		r, err := client.Do(get)
		if err != nil {
			// a poll that fails is not a job that failed: the server may be
			// busy with the card. Only a context that is done ends the wait,
			// and that is the case handled above.
			if ctx.Err() != nil {
				a.sdCancel(url, job.ID)
				return nil, errStopped
			}
			return nil, fmt.Errorf("sd.cpp job %s: %w", job.ID, err)
		}
		raw, _ := io.ReadAll(r.Body)
		r.Body.Close()
		if r.StatusCode == 404 || r.StatusCode == 410 {
			return nil, fmt.Errorf("sd.cpp forgot job %s (%s) -- it was probably restarted mid-draw",
				job.ID, r.Status)
		}
		var st sdJob
		if err := json.Unmarshal(raw, &st); err != nil {
			return nil, fmt.Errorf("sd.cpp job %s answered %s: %s", job.ID, r.Status, sdSnippet(raw))
		}
		switch st.Status {
		case "completed":
			if st.Result == nil || len(st.Result.Images) == 0 {
				return nil, fmt.Errorf("sd.cpp job %s finished with no image", job.ID)
			}
			img, err := base64.StdEncoding.DecodeString(st.Result.Images[0].B64)
			if err != nil {
				return nil, fmt.Errorf("sd.cpp job %s: undecodable image: %w", job.ID, err)
			}
			return img, nil
		case "failed", "cancelled":
			return nil, fmt.Errorf("sd.cpp job %s %s: %s", job.ID, st.Status, sdJobError(st, raw))
		default:
			if onWait != nil {
				where := st.Status
				if st.Queue > 0 {
					where = fmt.Sprintf("%s, %d ahead in the queue", st.Status, st.Queue)
				}
				onWait(where)
			}
		}
	}
}

// sdPollEvery is how often a running job is asked about. A thumbnail on this
// stack takes ten to thirty seconds, so a second is fine-grained enough to make
// the page look alive and coarse enough to be free.
//
// A var rather than a const only so the tests can shrink it: they drive a fake
// server through the same loop, and at a second a poll the offline suite spends
// most of its runtime asleep.
var sdPollEvery = 1 * time.Second

// sdCancel gives the card back. Best effort by design -- it runs when the user
// has already pressed ⏹, and there is nothing useful to tell them if the
// server has forgotten the job by the time we ask.
func (a *App) sdCancel(url, id string) {
	req, err := http.NewRequest("POST", url+"/sdcpp/v1/jobs/"+id+"/cancel", nil)
	if err != nil {
		return
	}
	bearer(req, a.readConf().SDKey)
	if resp, err := (&http.Client{Timeout: 10 * time.Second}).Do(req); err == nil {
		resp.Body.Close()
	}
}

// sdJobError is the server's own words about a failure, falling back to the
// raw body: an error naivepost cannot name is still an error worth reading.
func sdJobError(job sdJob, raw []byte) string {
	if job.Error != nil && strings.TrimSpace(job.Error.Message) != "" {
		if job.Error.Code != "" {
			return job.Error.Code + ": " + job.Error.Message
		}
		return job.Error.Message
	}
	return sdSnippet(raw)
}

// sdSnippet trims a response body down to something that fits in the log. An
// HTML error page from a proxy in front of the server is the usual case, and
// the first line of it is the part that identifies the proxy.
func sdSnippet(raw []byte) string {
	s := strings.TrimSpace(string(raw))
	if s == "" {
		return "(empty body)"
	}
	if i := strings.IndexByte(s, '\n'); i >= 0 {
		s = s[:i]
	}
	if r := []rune(s); len(r) > 160 {
		s = string(r[:160]) + "…"
	}
	return s
}

// testSD is the Settings button: something answers, it answers like sd.cpp, it
// can draw, and these are the weights loaded (chosen in SD_ARGS, cpp/run.sh,
// not here). When capabilities fails, /v1/models is tried so the usual tenant
// of port 1234 (an OpenAI-shaped server) can at least be named in the verdict.
func testSD(url, key string) (string, error) {
	caps, err := sdCapabilities(context.Background(), url, key)
	if err != nil {
		if ids, e2 := fetchModels(url, key); e2 == nil {
			return "", fmt.Errorf("%s answers /v1/models with %s, but not /sdcpp/v1/capabilities -- "+
				"that is an OpenAI-shaped server, not sd.cpp's sd-server, and the thumbnail needs "+
				"sd-server's native API", url, strings.Join(ids, ", "))
		}
		return "", err
	}
	got := caps.modelName()
	if got == "" {
		return "", fmt.Errorf("answering, but reporting no loaded model -- was it started without weights?")
	}
	if !caps.canImage() {
		return "", fmt.Errorf("%q is loaded, but this server does not offer img_gen (modes: %s)",
			got, strings.Join(caps.Modes, ", "))
	}
	return fmt.Sprintf("%q is loaded and can draw (%s)", got, caps.Model.Path), nil
}
