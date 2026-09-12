package main

// Prepare's half of the audio.cpp server, against a fake one. Nothing here
// reaches a real server or a GPU: what is being pinned is the contract between
// the two, which is where moving ASR and diarization off the CLI could go
// wrong quietly -- a request the server does not understand, or an answer read
// as if the numbers were seconds when they are sample counts.

import (
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"sync/atomic"
	"testing"
)

// fakeAudio is an audio.cpp server that always answers /health and /v1/models,
// and hands /v1/tasks/run to the test. Every request body it saw is recorded:
// the shape of what we send is as much of the contract as what we read back.
func fakeAudio(t *testing.T, models string, run http.HandlerFunc) (*App, *[]map[string]any) {
	t.Helper()
	var seen []map[string]any
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		switch r.URL.Path {
		case "/health":
			w.Write([]byte(`{"status":"ok"}`))
		case "/v1/models":
			w.Write([]byte(`{"object":"list","data":[` + models + `]}`))
		case "/v1/ui/upload":
			// the real server writes the body under its own temp folder and
			// answers with the path it used; the name it echoes is what makes
			// "the request named what the upload returned" checkable
			name := r.Header.Get("X-Audiocpp-Filename")
			n, _ := io.Copy(io.Discard, r.Body)
			fmt.Fprintf(w, `{"path":%q,"bytes":%d}`, "/srv/uploads/"+name, n)
		case "/v1/tasks/run", "/v1/audio/speech":
			// one fake for one server: listening goes through tasks/run and
			// speaking through audio/speech, but both take a JSON body naming
			// files, which is the thing these tests are about
			var body map[string]any
			if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
				t.Errorf("unreadable request body: %v", err)
			}
			seen = append(seen, body)
			run(w, r)
		default:
			t.Errorf("unexpected request to %s", r.URL.Path)
			w.WriteHeader(404)
		}
	}))
	t.Cleanup(srv.Close)

	dir := t.TempDir()
	a := &App{root: dir, outDir: dir}
	if err := a.writeConf(appConf{TTS: srv.URL}); err != nil {
		t.Fatal(err)
	}
	return a, &seen
}

// the two models Prepare asks for, as a server that has them lists them
const ttsModels = `{"id":"index-tts2","family":"index_tts2","task":"clon"}`

const asrModels = `{"id":"nemotron-asr","family":"nemotron_asr","task":"asr"},` +
	`{"id":"sortformer-diar","family":"sortformer_diar","task":"diar"}`

func answer(body string) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) { w.Write([]byte(body)) }
}

// one second in, one word, one speaker -- in the sample counts the server deals in
const asrAnswer = `{"text":"hello there","words":[
	{"word":"hello","start_sample":16000,"end_sample":24000,"confidence":0.9},
	{"word":"there","start_sample":24000,"end_sample":32000,"confidence":0.9}],
	"timing":{"wall_ms":12}}`

const diarAnswer = `{"speaker_turns":[
	{"start_sample":16000,"end_sample":32000,"speaker_id":"speaker_0","confidence":0.8}]}`

// TestASRPostsOneJobAndKeepsTheAnswerWhole pins the request the CLI's flags
// turned into, and that the answer is not filtered on the way to words.json.
func TestASRPostsOneJobAndKeepsTheAnswerWhole(t *testing.T) {
	a, seen := fakeAudio(t, asrModels, answer(asrAnswer))

	body, text, err := a.asrJSON(srcWav(t, a.outDir, "voice16k.wav"))
	if err != nil {
		t.Fatalf("asr: %v", err)
	}
	if len(*seen) != 1 {
		t.Fatalf("posted %d jobs, want 1", len(*seen))
	}
	got := (*seen)[0]
	if got["model"] != defASRModel {
		t.Errorf("asked model %v, want %q", got["model"], defASRModel)
	}
	req, ok := got["request"].(map[string]any)
	if !ok {
		t.Fatalf("no request object in %v", got)
	}
	// the server opens this itself, from a working directory that is not ours
	if p, _ := req["audio"].(string); !filepath.IsAbs(p) {
		t.Errorf("audio path %q is not absolute", p)
	}
	// the CLI could only carry the language on an empty --text; over HTTP it is
	// a field of its own, and it has to actually be sent
	if req["language"] != defLanguage {
		t.Errorf("language went as %v, want %q", req["language"], defLanguage)
	}
	if req["text"] != nil {
		t.Errorf("the empty-text workaround outlived the CLI: %v", req["text"])
	}
	if text != "hello there" {
		t.Errorf("transcript text = %q", text)
	}
	// verbatim: readers of words.json walk whatever shape they find, and the
	// parts we do not read today (timing, confidences) are why a run is
	// debuggable tomorrow
	if !strings.Contains(string(body), `"timing"`) || !strings.Contains(string(body), `"confidence"`) {
		t.Errorf("the answer was rewritten on the way to words.json: %s", body)
	}
}

// TestASRWithoutWordsIsSilence: a recording with no speech in it is a real
// input -- a screen capture with no mic behind it -- so an answer with no words
// is an empty transcript, not a failed step. It used to stop the whole run here, and
// what that stopped in practice was a project whose one source never had a
// voice on it at all.
func TestASRWithoutWordsIsSilence(t *testing.T) {
	a, _ := fakeAudio(t, asrModels, answer(`{"text":"","words":[]}`))
	body, text, err := a.asrJSON(srcWav(t, a.outDir, "silence.wav"))
	if err != nil {
		t.Fatalf("a silent recording failed to transcribe: %v", err)
	}
	if text != "" {
		t.Errorf("silence transcribed as %q", text)
	}
	// the answer is still written whole: words.json is the resume marker, and
	// silence recognised once must not be recognised again on every resume
	if !strings.Contains(string(body), `"words"`) {
		t.Errorf("the silent answer lost its shape: %s", body)
	}
}

// TestSilenceMergesToAnEmptyTranscript is the same case one stage on. No words
// at all means empty transcript files -- written, because later steps look for
// them -- while words whose times cannot be read are the ASR answer changing
// shape, which has to stop the step rather than quietly emptying every
// transcript after it.
func TestSilenceMergesToAnEmptyTranscript(t *testing.T) {
	a, _ := fakeAudio(t, asrModels, answer(""))
	out := filepath.Join(a.inputsDir(), "clip")
	if err := os.MkdirAll(out, 0o755); err != nil {
		t.Fatal(err)
	}
	write := func(name, body string) {
		if err := os.WriteFile(filepath.Join(out, name), []byte(body), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	write("words.json", `{"text":"","words":[]}`)
	write("turns.json", `{"speaker_turns":[]}`)
	if err := a.mergeSegments(out); err != nil {
		t.Fatalf("merging silence: %v", err)
	}
	for _, name := range []string{"transcript.tsv", "transcript.srt"} {
		b, err := os.ReadFile(filepath.Join(out, name))
		if err != nil {
			t.Fatalf("%s was not written for a silent clip: %v", name, err)
		}
		if len(b) != 0 {
			t.Errorf("%s of a silent clip holds %q", name, b)
		}
	}

	// words present but under different keys: a format drift, not silence
	write("words.json", `{"text":"hi","words":[{"word":"hi","start_ms":100,"end_ms":200}]}`)
	if err := a.mergeSegments(out); err == nil || !strings.Contains(err.Error(), "changed shape") {
		t.Errorf("unreadable word times merged without complaint: %v", err)
	}
}

// TestDiarSpansAreSecondsAndSilenceIsNotAnError covers the unit change (the
// server counts samples, the pipeline works in seconds) and the case the CLI
// expressed by writing no file at all.
func TestDiarSpansAreSecondsAndSilenceIsNotAnError(t *testing.T) {
	a, seen := fakeAudio(t, asrModels, answer(diarAnswer))
	spans, err := a.diarSpans(srcWav(t, a.outDir, "win.wav"))
	if err != nil {
		t.Fatalf("diar: %v", err)
	}
	if len(spans) != 1 || spans[0].s != 1 || spans[0].e != 2 || spans[0].slot != "speaker_0" {
		t.Fatalf("spans = %+v, want one second-long turn from 1 s to 2 s", spans)
	}
	if (*seen)[0]["model"] != defDiarModel {
		t.Errorf("asked model %v, want %q", (*seen)[0]["model"], defDiarModel)
	}

	quiet, _ := fakeAudio(t, asrModels, answer(`{"speaker_turns":[]}`))
	spans, err = quiet.diarSpans(srcWav(t, quiet.outDir, "win.wav"))
	if err != nil {
		t.Fatalf("a window with no speech failed: %v", err)
	}
	if len(spans) != 0 {
		t.Errorf("silence produced %+v", spans)
	}
}

// TestTheServersAnswersAreTheFilesPrepareKeeps is the point of the port: the two
// answers, written to disk exactly as they arrive, are read by the code that
// used to read the CLI's output files -- same names, same numbers, same
// segments out the other end.
func TestTheServersAnswersAreTheFilesPrepareKeeps(t *testing.T) {
	a, _ := fakeAudio(t, asrModels, func(w http.ResponseWriter, r *http.Request) {
		w.Write([]byte(asrAnswer)) // this one only ever gets the ASR job
	})
	out := filepath.Join(a.inputsDir(), "clip")
	if err := os.MkdirAll(out, 0o755); err != nil {
		t.Fatal(err)
	}
	body, text, err := a.asrJSON(srcWav(t, out, "voice16k.wav"))
	if err != nil {
		t.Fatal(err)
	}
	write := func(name string, b []byte) {
		if err := os.WriteFile(filepath.Join(out, name), b, 0o644); err != nil {
			t.Fatal(err)
		}
	}
	write("words.json", body)
	write("transcript.txt", []byte(text+"\n"))
	// turns.json is naivepost's own resolved list rather than one answer, but it
	// is written in the server's units and read by the same walker
	write("turns.json", []byte(diarAnswer))

	if err := a.mergeSegments(out); err != nil {
		t.Fatalf("merging the server's answers: %v", err)
	}
	tsv, err := os.ReadFile(filepath.Join(out, "transcript.tsv"))
	if err != nil {
		t.Fatal(err)
	}
	want := "1.00\t2.00\tspeaker_0\thello there\n"
	if string(tsv) != want {
		t.Errorf("transcript.tsv =\n%q\nwant\n%q", tsv, want)
	}
}

// TestAudioRunPassesTheServersRefusalBack: one server, one GPU, and a busy one
// answers 503. Whatever it says has to reach the log -- "the request failed" is
// not something a user can act on.
func TestAudioRunPassesTheServersRefusalBack(t *testing.T) {
	a, _ := fakeAudio(t, asrModels, func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(503)
		w.Write([]byte(`{"error":{"message":"model busy: timed out after 30000 ms","type":"server_busy"}}`))
	})
	_, _, err := a.asrJSON(srcWav(t, a.outDir, "voice16k.wav"))
	if err == nil {
		t.Fatal("a 503 passed as a transcript")
	}
	for _, want := range []string{"503", "server_busy", defASRModel} {
		if !strings.Contains(err.Error(), want) {
			t.Errorf("the error drops %q: %v", want, err)
		}
	}
}

// TestAudioRunNeedsAServer: the message for "nothing is listening" is the one
// most likely to be read by someone who has not started the stack.
func TestAudioRunNeedsAServer(t *testing.T) {
	ownConfig(t)
	a, _ := fakeAudio(t, asrModels, answer(asrAnswer))
	if err := a.writeConf(appConf{TTS: "http://127.0.0.1:1"}); err != nil {
		t.Fatal(err)
	}
	_, _, err := a.asrJSON(srcWav(t, a.outDir, "voice16k.wav"))
	if err == nil {
		t.Fatal("a dead endpoint passed")
	}
	if !strings.Contains(err.Error(), "docker compose up -d audio") {
		t.Errorf("the error does not say how to start it: %v", err)
	}
}

// TestEnsureAudioModelsNamesWhatIsMissing guards the preflight. It exists
// because the alternative is minutes of frame extraction followed by "unknown
// model", and because a server CAN answer perfectly while serving only the
// voice.
func TestEnsureAudioModelsNamesWhatIsMissing(t *testing.T) {
	fail := func(t *testing.T, models string, wants ...string) {
		t.Helper()
		a, _ := fakeAudio(t, models, answer(""))
		err := a.ensureAudioModels(false)
		if err == nil {
			t.Fatalf("%s passed the preflight", models)
		}
		for _, w := range wants {
			if !strings.Contains(err.Error(), w) {
				t.Errorf("the error drops %q: %v", w, err)
			}
		}
	}

	a, _ := fakeAudio(t, asrModels, answer(""))
	if err := a.ensureAudioModels(false); err != nil {
		t.Fatalf("a server with both models was rejected: %v", err)
	}

	// the narration server before anyone added the step-1 entries
	fail(t, `{"id":"index-tts2","family":"index_tts2","task":"clon"}`,
		defASRModel, "index-tts2", "config-audiocpp.json")
	// the id is there but points at the wrong thing -- a copied entry with the
	// task left as it was
	fail(t, `{"id":"nemotron-asr","family":"nemotron_asr","task":"asr"},`+
		`{"id":"sortformer-diar","family":"sortformer_diar","task":"asr"}`,
		defDiarModel, "diar")
}

// TestTheSeparationModelIsOnlyRequiredWhenSomethingAskedToBeSplit: every stack
// this runs against has the ASR and the diarizer; hardly any has a separation
// model until somebody installs one for this. Demanding it of a session that
// flagged nothing would be refusing to start over a model nothing would call.
func TestTheSeparationModelIsOnlyRequiredWhenSomethingAskedToBeSplit(t *testing.T) {
	a, _ := fakeAudio(t, asrModels, answer(""))
	if err := a.ensureAudioModels(false); err != nil {
		t.Fatalf("a session that flagged nothing was refused: %v", err)
	}
	err := a.ensureAudioModels(true)
	if err == nil {
		t.Fatal("a session that asked to be split started without a model to split with")
	}
	for _, w := range []string{defSepModel, "bs_roformer_q8_0", "config-audiocpp.json"} {
		if !strings.Contains(err.Error(), w) {
			t.Errorf("the error drops %q: %v", w, err)
		}
	}

	// present, but declared for something else: the entry was copied from the
	// diarizer's and the task went with it
	b, _ := fakeAudio(t, asrModels+`,{"id":"bs-roformer","family":"bs_roformer","task":"diar"}`,
		answer(""))
	err = b.ensureAudioModels(true)
	if err == nil {
		t.Fatal("a model declared for diarization was accepted for separation")
	}
	if !strings.Contains(err.Error(), `"sep"`) {
		t.Errorf("the error does not say what the task should be: %v", err)
	}

	c, _ := fakeAudio(t, asrModels+`,{"id":"bs-roformer","family":"bs_roformer","task":"sep"}`,
		answer(""))
	if err := c.ensureAudioModels(true); err != nil {
		t.Fatalf("a server with all three models was rejected: %v", err)
	}
}

// TestCatalogIDsIsStable: these names go into error messages, and a message
// that reorders itself between two runs of the same broken config reads like
// something changed.
func TestCatalogIDsIsStable(t *testing.T) {
	cat := map[string]audioModel{}
	for _, id := range []string{"sortformer-diar", "index-tts2", "nemotron-asr"} {
		cat[id] = audioModel{ID: id}
	}
	for i := 0; i < 5; i++ {
		if got, want := catalogIDs(cat), "index-tts2, nemotron-asr, sortformer-diar"; got != want {
			t.Fatalf("catalogIDs = %q, want %q", got, want)
		}
	}
	if got := catalogIDs(nil); got != "nothing" {
		t.Errorf("an empty catalog reads %q", got)
	}
}

// A capture is twelve minutes and an ASR encoder is not: its position table
// runs out somewhere in the middle, and Nemotron says so with a 500 rather
// than transcribing what it can. So a long recording is cut up, and what the
// cutting must not do is hand any piece back over the ceiling -- two cuts can
// slide away from each other towards their nearest silences, and a piece that
// grows past max is the very error the cutting exists to avoid.
func TestALongRecordingIsCutIntoPiecesTheEncoderCanHold(t *testing.T) {
	const max, seek = 300.0, 45.0
	// silences all over, including two right where a cut would like to be
	quiet := []span{{s: 100, e: 101}, {s: 180, e: 184}, {s: 300, e: 302},
		{s: 370, e: 371}, {s: 500, e: 507}, {s: 600, e: 601}}
	for _, dur := range []float64{301, 400, 751.8, 900, 3600, 7200} {
		cuts := asrCuts(dur, quiet, max, seek)
		at := 0.0
		for _, c := range cuts {
			if c <= at {
				t.Errorf("%.1f s: cuts %v do not climb", dur, cuts)
				break
			}
			if c-at > max {
				t.Errorf("%.1f s: a piece of %.1f s is past the %.0f s ceiling (cuts %v)",
					dur, c-at, max, cuts)
			}
			at = c
		}
		if dur-at > max {
			t.Errorf("%.1f s: the last piece is %.1f s, past the %.0f s ceiling", dur, dur-at, max)
		}
	}
	// a recording that fits is not cut at all -- it goes to the server whole
	// and its answer is written through as it came
	if got := asrCuts(300, quiet, max, seek); got != nil {
		t.Errorf("a %s-second recording was cut at %v", "300", got)
	}
}

// The point of looking for silence is that a cut lands where nobody is
// talking. Within reach, the cut moves to the middle of the quiet; out of
// reach, it stays on the clock rather than dragging the piece somewhere worse.
func TestACutGoesToTheNearestSilenceOrStaysOnTheClock(t *testing.T) {
	// 700 s at a 300 s ceiling with 20 s of reach: pieces are sized so the
	// sliding cannot overflow, which puts two cuts near 233 and 467
	cuts := asrCuts(700, []span{{s: 220, e: 230}}, 300, 20)
	if len(cuts) != 2 {
		t.Fatalf("700 s came back as %v, want two cuts", cuts)
	}
	if cuts[0] != 225 {
		t.Errorf("the first cut is at %.1f s, want 225 -- the middle of the silence beside it", cuts[0])
	}
	// nothing quiet within reach of the second, so it stays where the clock put it
	if cuts[1] < 450 || cuts[1] > 480 {
		t.Errorf("the second cut ran to %.1f s with no silence to go to", cuts[1])
	}
	// and a silence nowhere near a cut moves nothing
	far := asrCuts(700, []span{{s: 10, e: 20}}, 300, 20)
	if far[0] != cuts[1]/2 {
		t.Errorf("with the only silence at 15 s the cuts are %v, want them left on the clock", far)
	}
}

// ffmpeg reports silence on stderr, in two lines that have to be paired up.
func TestTheSilenceReportIsRead(t *testing.T) {
	report := `[silencedetect @ 0x55] silence_start: 12.345
[silencedetect @ 0x55] silence_end: 15.678 | silence_duration: 3.333
size=N/A time=00:12:31.80 bitrate=N/A speed=1e+03x
[silencedetect @ 0x55] silence_start: 700.5
`
	got := parseSilence(report)
	if len(got) != 1 {
		t.Fatalf("read %v, want the one closed silence -- the one still open at the end of the file is not a place to cut", got)
	}
	if got[0].s != 12.345 || got[0].e != 15.678 {
		t.Errorf("read %.3f-%.3f, want 12.345-15.678", got[0].s, got[0].e)
	}
}

// Every chunk answers in its own time, starting from zero, and the stitched
// answer has to read as one recording. Anything else a word carries rides
// along -- and a word whose times cannot be read rides along too, because the
// merge is what has to notice an answer shape nobody here understands, and it
// cannot notice what this dropped.
func TestAChunksWordsComeBackWhereTheChunkWas(t *testing.T) {
	body := []byte(`{"text":"hi there","words":[
		{"word":"hi","start_sample":8000,"end_sample":16000,"confidence":0.9},
		{"word":"there","start_sample":16000,"end_sample":24000},
		{"word":"???"}]}`)
	got := shiftWords(body, 300) // the chunk that started five minutes in
	if len(got) != 3 {
		t.Fatalf("got %d words, want all three", len(got))
	}
	first := got[0].(map[string]any)
	if first["start_sample"] != float64(300*sampleRate+8000) {
		t.Errorf("the first word starts at sample %v, want it moved by the chunk's own start", first["start_sample"])
	}
	if first["confidence"] != 0.9 {
		t.Errorf("confidence came back as %v -- what a word carries travels with it", first["confidence"])
	}
	if last := got[2].(map[string]any); last["word"] != "???" {
		t.Errorf("the word with no times was dropped: %v", got[2])
	}
}

// A blank API key is "this server wants none", not "the key is empty". The
// local stack has no auth at all, and an Authorization header with nothing
// after the word Bearer is a header that means nothing to a server that ignores
// it and is a 401 from a proxy that does not. Every client goes through the one
// helper, because the rule is easy to forget one client at a time -- the audio
// server had it right while the LLM chat and both Settings probes sent the
// empty header for as long as they had existed.
func TestABlankKeyIsNoHeaderAtAll(t *testing.T) {
	req, err := http.NewRequest("GET", "http://example.invalid/v1/models", nil)
	if err != nil {
		t.Fatal(err)
	}
	for _, blank := range []string{"", "   ", "\t\n"} {
		bearer(req, blank)
		if got := req.Header.Get("Authorization"); got != "" {
			t.Errorf("a key of %q sent %q", blank, got)
		}
	}
	// ...and a real one arrives trimmed: a key pasted out of a web page brings
	// the newline with it, and a header value with a newline in it is refused
	// by the transport rather than by the server
	bearer(req, "  sk-abc\n")
	if got := req.Header.Get("Authorization"); got != "Bearer sk-abc" {
		t.Errorf("Authorization = %q, want %q", got, "Bearer sk-abc")
	}

	// one place builds it, so there is one rule rather than one per client
	files, err := filepath.Glob("*.go")
	if err != nil {
		t.Fatal(err)
	}
	for _, f := range files {
		if f == "audiocpp.go" || strings.HasSuffix(f, "_test.go") {
			continue
		}
		if strings.Contains(readSrc(t, f), `"Bearer "+`) {
			t.Errorf("%s builds the Authorization header itself instead of calling bearer()", f)
		}
	}
}

// The chunks a long recording is cut into are scratch, and they used to be
// swept up on the way out of asrLong whatever had happened. That is right when
// the answer was stitched and exactly wrong when it was not: the one thing
// worth having after "the server could not open c00.wav" is c00.wav.
func TestAFailedASRKeepsTheChunkItFailedOn(t *testing.T) {
	// asrLongAt is the pass; asrLong above it only chooses how much audio a
	// request may carry and walks that down when the server has no room
	body := funcBody(t, "pipeline.go", `func \(a \*App\) asrLongAt\(`)
	if !strings.Contains(body, "done := false") || !strings.Contains(body, "if done {") {
		t.Fatalf("asrLong no longer decides whether to sweep the chunks up:\n%s", body)
	}
	// set on the way out and nowhere else: an early return is a failure, and
	// every one of them is above this line
	i, j := strings.Index(body, "done = true"), strings.Index(body, "return append(b, '\\n'), text, nil")
	if i < 0 || j < 0 {
		t.Fatal("asrLong no longer marks the run finished before it returns the answer")
	}
	if i > j {
		t.Error("the chunks are swept up after the answer is built rather than with it")
	}
	if n := strings.Count(body, "os.RemoveAll(dir)"); n != 2 {
		t.Errorf("asrLong removes the chunk folder %d times, want the stale one and the swept one", n)
	}
}

// The audio server is in a container and can only open what its compose entry
// mounts. Recording anywhere else used to end in a 500 naming a file that is
// plainly on disk here, which reads like naivepost naming a file it never wrote --
// the one thing it is not.
//
// So naivepost no longer names a file it holds. It posts the bytes to
// /v1/ui/upload, which writes them under the server's own temp folder and
// answers with that path, and the job names what came back. Which folders are
// mounted stops mattering.
func TestAFileGoesToTheServerBeforeItIsNamedToIt(t *testing.T) {
	ownConfig(t)
	for _, tc := range []struct {
		name, field string
		call        func(*App, string)
	}{
		{"asr", "audio", func(a *App, w string) { a.asrJSON(w) }},
		{"diar", "audio", func(a *App, w string) { a.diarSpans(w) }},
	} {
		t.Run(tc.name, func(t *testing.T) {
			a, seen := fakeAudio(t, asrModels, answer(`{"segments":[]}`))
			if err := a.writeConf(appConf{TTS: a.configuredAudio(),
				ASRModel: "nemotron-asr", DiarModel: "sortformer-diar"}); err != nil {
				t.Fatal(err)
			}
			tc.call(a, srcWav(t, a.outDir, "voice16k.wav"))
			if len(*seen) != 1 {
				t.Fatalf("%d jobs posted, want 1", len(*seen))
			}
			req, _ := (*seen)[0]["request"].(map[string]any)
			// the server's path for our bytes, not our path for our bytes
			if got, _ := req[tc.field].(string); got != "/srv/uploads/voice16k.wav" {
				t.Errorf("the job named %q, want the path the upload answered with", got)
			}
		})
	}

	// and the voice to clone, which does not go through audioRun at all
	t.Run("tts", func(t *testing.T) {
		a, seen := fakeAudio(t, ttsModels, func(w http.ResponseWriter, r *http.Request) {
			w.Write(make([]byte, 2000))
		})
		if err := a.writeConf(appConf{TTS: a.configuredAudio(), TTSModel: "index-tts2"}); err != nil {
			t.Fatal(err)
		}
		writeRef(t, a)
		if err := a.speak("hello", "", 1, filepath.Join(t.TempDir(), "out.wav")); err != nil {
			t.Fatal(err)
		}
		if len(*seen) != 1 {
			t.Fatalf("%d speech requests, want 1", len(*seen))
		}
		if got, _ := (*seen)[0]["voice_ref"].(string); got != "/srv/uploads/voice_ref.wav" {
			t.Errorf("the voice was asked for as %q, want the path the upload answered with", got)
		}
	})
}

// A server built without the UI refuses uploads, and then it really can only
// read what it already has. That is the one case where the old advice is still
// the advice, so the refusal has to carry both ways out rather than 403.
func TestAServerThatRefusesUploadsSaysWhatToDoInstead(t *testing.T) {
	ownConfig(t)
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		switch r.URL.Path {
		case "/health":
			w.Write([]byte(`{"status":"ok"}`))
		case "/v1/ui/upload":
			w.WriteHeader(403)
			w.Write([]byte(`{"error":{"message":"UI uploads are disabled","type":"forbidden"}}`))
		default:
			t.Errorf("the job got as far as %s with nowhere to put the file", r.URL.Path)
		}
	}))
	t.Cleanup(srv.Close)
	dir := t.TempDir()
	a := &App{root: dir, outDir: dir}
	if err := a.writeConf(appConf{TTS: srv.URL, ASRModel: "nemotron-asr"}); err != nil {
		t.Fatal(err)
	}
	wav := srcWav(t, filepath.Join(a.inputsDir(), "clip"), "voice16k.wav")
	_, _, err := a.asrJSON(wav)
	if err == nil {
		t.Fatal("a refused upload passed as a transcript")
	}
	for _, want := range []string{
		"--ui-management",    // turn uploads on...
		"docker-compose.yml", // ...here
		filepath.Dir(wav),    // or make this folder visible to it
	} {
		if !strings.Contains(err.Error(), want) {
			t.Errorf("the error drops %q: %v", want, err)
		}
	}
}

// Nothing is listening is the message most likely to be read by someone who has
// not started the stack, and the upload is now the first request of every job --
// so it, not audioRun, is where a dead endpoint gets noticed.
func TestADeadServerIsStillNamedBeforeAnyFileMoves(t *testing.T) {
	ownConfig(t)
	a, _ := fakeAudio(t, asrModels, answer(asrAnswer))
	if err := a.writeConf(appConf{TTS: "http://127.0.0.1:1"}); err != nil {
		t.Fatal(err)
	}
	_, _, err := a.asrJSON(srcWav(t, a.outDir, "voice16k.wav"))
	if err == nil {
		t.Fatal("a dead endpoint passed")
	}
	if !strings.Contains(err.Error(), "docker compose up -d audio") {
		t.Errorf("the error does not say how to start it: %v", err)
	}
}

// writeRef puts a voice reference where speak expects one, so a test can reach
// the server without ffmpeg cutting a real sample first.
func writeRef(t *testing.T, a *App) {
	t.Helper()
	if err := os.MkdirAll(a.narrateDir(), 0o755); err != nil {
		t.Fatal(err)
	}
	for _, f := range []string{a.refBase(), a.refPath()} {
		// a real header: ensureVoiceRef reads it, and a stub would send it off
		// to be cut again from a recording this test does not have
		if err := os.WriteFile(f, plainWavBytes(64), 0o644); err != nil {
			t.Fatal(err)
		}
	}
}

// srcWav writes a stub recording and returns its path. Every file naivepost names
// to the audio server is now read here first and sent up, so a test that hands
// over a path has to hand over a file.
func srcWav(t *testing.T, dir, name string) string {
	t.Helper()
	p := filepath.Join(dir, name)
	if err := os.MkdirAll(filepath.Dir(p), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(p, []byte("RIFF....WAVEfmt "), 0o644); err != nil {
		t.Fatal(err)
	}
	return p
}

// The server loads a model on first use and keeps it. After a Prepare that
// transcribed, aligned and diarized, three of them sat resident for the rest of
// the day on a machine whose GPU memory is shared with everything else -- which
// is what the user saw: the bar stays high after the work is done. So a run
// ending hands the memory back.
func TestAFinishedRunHandsBackTheModels(t *testing.T) {
	var hits int32
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/v1/tasks/unload_all_models" || r.Method != "POST" {
			t.Errorf("unexpected %s %s", r.Method, r.URL.Path)
		}
		if got := r.Header.Get("Authorization"); got != "Bearer k" {
			t.Errorf("the unload went unauthenticated: %q", got)
		}
		atomic.AddInt32(&hits, 1)
		w.Write([]byte(`{"unloaded":["qwen3-asr"]}`))
	}))
	defer srv.Close()

	dir := t.TempDir()
	a := &App{root: dir, outDir: dir}
	if err := a.writeConf(appConf{TTS: srv.URL, TTSKey: "k"}); err != nil {
		t.Fatal(err)
	}
	a.freeAudioModels()
	if atomic.LoadInt32(&hits) != 1 {
		t.Error("a finished run leaves the models loaded")
	}

	// ...and a run that never touched audio ends the same way, so this cannot
	// be left to the callers to remember. Nothing is listening there, and that
	// has to be silence rather than an error on the way out of a good run
	b := &App{root: t.TempDir(), outDir: t.TempDir()}
	if err := b.writeConf(appConf{TTS: "http://127.0.0.1:1"}); err != nil {
		t.Fatal(err)
	}
	b.freeAudioModels()

	// the one place it is asked from: every ▶ and ↻ ends through endRun, and
	// six call sites each remembering to do it themselves is six chances not to
	if !strings.Contains(funcBody(t, "runqueue.go", `func \(a \*App\) endRun\(`), "go a.freeAudioModels()") {
		t.Error("endRun does not free the models -- some runs will leave them loaded")
	}
}
