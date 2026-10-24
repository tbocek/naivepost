package main

// The recorder behind every LLM call: each exchange becomes one HTML page
// under llm/ and a preview in the log. The server here is a local httptest
// one -- these tests never reach the configured LLM.

import (
	"encoding/json"
	"fmt"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// chatFake stands up a local chat server answering one canned body, and an
// App whose output folder is a fresh temp dir, so the recorder has somewhere
// to write.
func chatFake(t *testing.T, status int, body string) *App {
	t.Helper()
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(status)
		fmt.Fprint(w, body)
	}))
	t.Cleanup(srv.Close)
	a := &App{root: t.TempDir()}
	a.outDir = a.root
	if err := a.writeConf(appConf{Server: srv.URL, Model: "test"}); err != nil {
		t.Fatal(err)
	}
	return a
}

func TestAnExchangeIsKeptWholeWithItsImages(t *testing.T) {
	a := chatFake(t, 200,
		`{"choices":[{"message":{"content":"<think>the hidden plan</think>THE ANSWER"}}]}`)
	img := filepath.Join(a.root, "frame.jpg")
	if err := os.WriteFile(img, []byte("jpegbytes"), 0o644); err != nil {
		t.Fatal(err)
	}
	part, err := imgPart(img)
	if err != nil {
		t.Fatal(err)
	}
	reply, err := a.llmChat("probe", []map[string]any{
		msg("system", "THE SYSTEM PROMPT"),
		msg("user", []any{txtPart("THE USER TEXT"), part}),
	}, true)
	if err != nil {
		t.Fatal(err)
	}
	if reply != "<think>the hidden plan</think>THE ANSWER" {
		t.Fatalf("reply = %q", reply)
	}
	pages, err := filepath.Glob(filepath.Join(a.llmDir(), "*.html"))
	if err != nil || len(pages) != 1 {
		t.Fatalf("the exchange left %d pages (%v), want 1", len(pages), err)
	}
	if !strings.HasSuffix(pages[0], "-probe.html") {
		t.Errorf("the page %q is not filed under its step", pages[0])
	}
	b, err := os.ReadFile(pages[0])
	if err != nil {
		t.Fatal(err)
	}
	page := string(b)
	for _, want := range []string{
		"THE SYSTEM PROMPT", // what was sent...
		"THE USER TEXT",
		"data:image/jpeg;base64,",              // ...including the picture, inline
		"THE ANSWER",                           // what came back
		"<details><summary>thinking</summary>", // with the self-talk folded away
	} {
		if !strings.Contains(page, want) {
			t.Errorf("the page does not hold %q", want)
		}
	}
}

// End to end: the server answers with reasoning and no content. The caller gets
// the empty reply it always got -- the answer is what it parses -- but the page
// holds the reasoning, and says which kind of nothing this was.
func TestACallThatOnlyThoughtIsRecordedAsSuch(t *testing.T) {
	a := chatFake(t, 200,
		`{"choices":[{"message":{"content":"","reasoning_content":"I could start at 12s... or 14s..."}}]}`)
	reply, err := a.llmChat("probe", []map[string]any{msg("user", "x")}, true)
	if err != nil {
		t.Fatal(err)
	}
	if reply != "" {
		t.Errorf("the reasoning leaked into the reply: %q", reply)
	}
	pages, _ := filepath.Glob(filepath.Join(a.llmDir(), "*.html"))
	if len(pages) != 1 {
		t.Fatalf("the call left %d pages, want 1", len(pages))
	}
	b, err := os.ReadFile(pages[0])
	if err != nil {
		t.Fatal(err)
	}
	page := string(b)
	for _, want := range []string{
		"<details><summary>thinking</summary>", // the working is on the page...
		"I could start at 12s",
		"(no answer", // ...and the page says the answer was missing, not the record
	} {
		if !strings.Contains(page, want) {
			t.Errorf("the page does not hold %q:\n%s", want, page)
		}
	}
}

func TestAFailedCallIsKeptWithItsError(t *testing.T) {
	a := chatFake(t, 500, `{"error":"boom"}`)
	_, err := a.llmChat("probe", []map[string]any{msg("user", "x")}, false)
	if err == nil {
		t.Fatal("a 500 with no choices did not error")
	}
	pages, _ := filepath.Glob(filepath.Join(a.llmDir(), "*.html"))
	if len(pages) != 1 {
		t.Fatalf("the failed exchange left %d pages, want 1 -- failures are when the record matters most", len(pages))
	}
	b, _ := os.ReadFile(pages[0])
	if !strings.Contains(string(b), "the call failed") {
		t.Error("the page does not say the call failed")
	}
}

func TestHeadlessCallsLeaveNoFiles(t *testing.T) {
	a := chatFake(t, 200, `{"choices":[{"message":{"content":"ok"}}]}`)
	a.outDir = "" // an App built by a test that never chose an output folder
	if _, err := a.llmChat("probe", []map[string]any{msg("user", "x")}, false); err != nil {
		t.Fatal(err)
	}
	if pages, _ := filepath.Glob(filepath.Join("llm", "*.html")); len(pages) != 0 {
		t.Errorf("a recorder with nowhere to write wrote %d pages into the working dir", len(pages))
	}
}

func TestThePreviewQuotesTheAnswerNotTheThinking(t *testing.T) {
	long := strings.Repeat("word ", 60) // 120 words later, well past the cut
	for _, c := range []struct{ reply, want string }{
		{"<think>hidden  plan</think>\n  {\"segments\": []}", `{"segments": []}`},
		{"no tags, just the answer", "no tags, just the answer"},
		{"", ""},
		{"<think>only thinking</think>", ""},
		{long, strings.Repeat("word ", 22) + "…"}, // the cut lands exactly after word 22
	} {
		if got := chatPreview(c.reply); got != c.want {
			t.Errorf("chatPreview(%.30q...) = %q, want %q", c.reply, got, c.want)
		}
	}
}

func TestWhatWentOutIsMeasuredNotGuessed(t *testing.T) {
	imgish := map[string]any{"type": "image_url", "image_url": map[string]any{"url": "data:image/jpeg;base64,xxxx"}}
	text, imgs := chatSent([]map[string]any{
		msg("system", "abcd"),
		msg("user", []any{txtPart("ee"), imgish, imgish}),
	})
	if text != 6 || imgs != 2 {
		t.Errorf("chatSent = %d text, %d images; want 6 and 2 -- image bytes must not count as text", text, imgs)
	}
	if got := sizeOf(500); got != "500 B" {
		t.Errorf("sizeOf(500) = %q", got)
	}
	if got := sizeOf(4096); got != "4.0 kB" {
		t.Errorf("sizeOf(4096) = %q", got)
	}
}

// TestEveryCallGoesThroughTheRecorder pins the wiring: the one seam in llm.go
// records, every step names itself there, and the logged path is a real link
// (tagged text with a click gesture), not just words.
func TestEveryCallGoesThroughTheRecorder(t *testing.T) {
	for file, wants := range map[string][]string{
		"llm.go": {
			"rec := a.recordChatStart(step, thinking, msgs)",
			"rec.done(rep.recorded(), rep.Stop, time.Since(t0), err)",
		},
		"describe.go":   {"a.llmChat" + `Retry("describe", `},
		"transcript.go": {"a.llmChat" + `Retry("transcript", `},
		"cut_suggest.go": {`a.llmChatRetryTools("suggest", `, `a.llmChatRetryOn("captions", `,
			`a.llmChatRetryOn("effects", `},
		"narrate.go": {`a.llmChatRetryTools("narrate", `},
		"publish.go": {"a.llmChat" + `RetryTools("publish", `}, // split: the guard must not read pins as calls
		"llmlog.go": {
			"buf.ApplyTag(a.linkTag,",    // the path is tagged...
			"a.log.AddController(click)", // ...and the tag is what the click resolves
		},
	} {
		src := readSrc(t, file)
		for _, want := range wants {
			if !strings.Contains(src, want) {
				t.Errorf("%s no longer contains %q", file, want)
			}
		}
	}
}

// TestTheRequestIsOnDiskBeforeTheReplyArrives: a suggest call thinks for
// minutes, and those minutes are when "what did we just send?" gets asked --
// so the page, with everything that was sent, must exist while the model is
// still working, and the finished page must be the same file rewritten. The
// server here IS the model mid-thought: its handler runs after the request
// went out and before any reply exists, and reads what the recorder has filed.
func TestTheRequestIsOnDiskBeforeTheReplyArrives(t *testing.T) {
	ownConfig(t)
	a := &App{root: t.TempDir()}
	a.outDir = a.root
	var midCall []byte
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if pages, _ := filepath.Glob(filepath.Join(a.llmDir(), "*.html")); len(pages) == 1 {
			midCall, _ = os.ReadFile(pages[0])
		}
		fmt.Fprint(w, `{"choices":[{"message":{"content":"THE ANSWER"}}]}`)
	}))
	t.Cleanup(srv.Close)
	if err := a.writeConf(appConf{Server: srv.URL, Model: "test"}); err != nil {
		t.Fatal(err)
	}
	if _, err := a.llmChat("probe", []map[string]any{msg("user", "THE USER TEXT")}, false); err != nil {
		t.Fatal(err)
	}
	page := string(midCall)
	if page == "" {
		t.Fatal("no page was on disk while the model was still thinking")
	}
	if !strings.Contains(page, "THE USER TEXT") {
		t.Error("the mid-call page does not show what was sent")
	}
	if !strings.Contains(page, "reply pending") {
		t.Error("the mid-call page does not say it is waiting")
	}
	if strings.Contains(page, "THE ANSWER") {
		t.Error("the mid-call page holds a reply that did not exist yet")
	}
	// afterwards: one file, the same one, now whole
	pages, _ := filepath.Glob(filepath.Join(a.llmDir(), "*.html"))
	if len(pages) != 1 {
		t.Fatalf("the exchange left %d pages, want the pending one rewritten in place", len(pages))
	}
	b, _ := os.ReadFile(pages[0])
	if !strings.Contains(string(b), "THE ANSWER") {
		t.Error("the finished page lacks the reply")
	}
	if strings.Contains(string(b), "has not arrived yet") {
		t.Error("the finished page still claims to be waiting")
	}
}

// TestAStreamedReplyGrowsThePageAsItArrives: a thinking call streams, and the
// page is where you watch it -- refresh mid-call and read as far as the model
// has got. The recorder is handed everything-so-far on each token, so it must
// append only what is new rather than repeat the lot.
func TestAStreamedReplyGrowsThePageAsItArrives(t *testing.T) {
	ownConfig(t)
	a := &App{root: t.TempDir()}
	a.outDir = a.root
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "text/event-stream")
		for _, tok := range []string{"ALPHA ", "BETA ", "GAMMA"} {
			fmt.Fprintf(w, "data: {\"choices\":[{\"delta\":{\"content\":%q}}]}\n\n", tok)
			w.(http.Flusher).Flush()
		}
		fmt.Fprint(w, "data: [DONE]\n\n")
	}))
	t.Cleanup(srv.Close)
	if err := a.writeConf(appConf{Server: srv.URL, Model: "test"}); err != nil {
		t.Fatal(err)
	}
	// read the live page from inside the caller's own callback: the recorder
	// tees ahead of it, so by the time this runs the page must already hold
	// everything the model has said so far
	var seen, onPage []string
	reply, err := a.llmChatOn("probe", []map[string]any{msg("user", "x")}, false,
		func(s string) {
			seen = append(seen, s)
			if pages, _ := filepath.Glob(filepath.Join(a.llmDir(), "*.html")); len(pages) == 1 {
				b, _ := os.ReadFile(pages[0])
				onPage = append(onPage, string(b))
			}
		})
	if err != nil {
		t.Fatal(err)
	}
	if reply != "ALPHA BETA GAMMA" {
		t.Fatalf("reply = %q", reply)
	}
	// the caller's own callback still gets every token: the recorder tees, not steals
	if len(seen) != 3 || seen[2] != "ALPHA BETA GAMMA" {
		t.Errorf("the caller saw %v -- the recorder swallowed the stream", seen)
	}
	if len(onPage) != 3 {
		t.Fatalf("the live page was readable at %d of the 3 tokens", len(onPage))
	}
	// growing, token by token, and each token written once
	for i, want := range []string{"ALPHA", "ALPHA BETA", "ALPHA BETA GAMMA"} {
		if !strings.Contains(onPage[i], want) {
			t.Errorf("after token %d the live page does not hold %q -- the reply is not being appended as it arrives", i+1, want)
		}
	}
	if got := strings.Count(onPage[2], "ALPHA"); got != 1 {
		t.Errorf("ALPHA appears %d times on the live page -- everything-so-far was appended each time, not the new part", got)
	}
	pages, _ := filepath.Glob(filepath.Join(a.llmDir(), "*.html"))
	if len(pages) != 1 {
		t.Fatalf("the exchange left %d pages, want the live one rewritten in place", len(pages))
	}
	b, _ := os.ReadFile(pages[0])
	if got := strings.Count(string(b), "ALPHA"); got != 1 {
		t.Errorf("ALPHA appears %d times on the finished page", got)
	}
}

// TestAnUnstreamedCallStaysUnstreamed: wrapping a nil callback would turn every
// one-shot request into a streaming one on the wire, which changes what the
// server sends back. The recorder only tees a stream that already exists.
func TestAnUnstreamedCallStaysUnstreamed(t *testing.T) {
	ownConfig(t)
	seam := readSrc(t, "llm.go")
	if !strings.Contains(seam, "if onText != nil {") {
		t.Error("llm.go tees the stream unconditionally -- a callerless call would ask the server to stream")
	}
	a := &App{root: t.TempDir()}
	a.outDir = a.root
	streamed := false
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		var body map[string]any
		json.NewDecoder(r.Body).Decode(&body)
		_, streamed = body["stream"]
		fmt.Fprint(w, `{"choices":[{"message":{"content":"ok"}}]}`)
	}))
	t.Cleanup(srv.Close)
	if err := a.writeConf(appConf{Server: srv.URL, Model: "test"}); err != nil {
		t.Fatal(err)
	}
	if _, err := a.llmChat("probe", []map[string]any{msg("user", "x")}, false); err != nil {
		t.Fatal(err)
	}
	if streamed {
		t.Error("a call with no callback asked the server to stream")
	}
}

// One page per run, not one per call. A run is the cut, then its captions
// batch by batch, then its effects -- and reading what happened meant opening
// nine files in the order their names implied. The question is never "what did
// call four say", it is "what did this run do".
func TestARunIsOnePage(t *testing.T) {
	a := chatFake(t, 200, `{"choices":[{"message":{"content":"THE ANSWER"}}]}`)
	a.qReset() // every step's runner starts here, and so does the page
	for i := 0; i < 3; i++ {
		if _, err := a.llmChat("suggest", []map[string]any{msg("user", fmt.Sprintf("call %d", i+1))}, false); err != nil {
			t.Fatal(err)
		}
	}
	pages, _ := filepath.Glob(filepath.Join(a.llmDir(), "*.html"))
	if len(pages) != 1 {
		t.Fatalf("three calls left %d pages, want one for the run: %v", len(pages), pages)
	}
	b, err := os.ReadFile(pages[0])
	if err != nil {
		t.Fatal(err)
	}
	page := string(b)
	for i, want := range []string{"call 1", "call 2", "call 3"} {
		if !strings.Contains(page, want) {
			t.Errorf("the run page is missing what call %d sent", i+1)
		}
	}
	// numbered in the order they went out, and each with its own reply
	for _, want := range []string{"<h1>1. suggest</h1>", "<h1>2. suggest</h1>", "<h1>3. suggest</h1>"} {
		if !strings.Contains(page, want) {
			t.Errorf("the run page has no heading %q", want)
		}
	}
	if n := strings.Count(page, "THE ANSWER"); n != 3 {
		t.Errorf("%d replies on the page, want 3", n)
	}
	// one document, not three concatenated
	if n := strings.Count(page, "<!doctype html>"); n != 1 {
		t.Errorf("the page has %d doctypes", n)
	}
	// the next run gets its own page
	a.qReset()
	if _, err := a.llmChat("narrate", []map[string]any{msg("user", "another run")}, false); err != nil {
		t.Fatal(err)
	}
	pages, _ = filepath.Glob(filepath.Join(a.llmDir(), "*.html"))
	if len(pages) != 2 {
		t.Errorf("the second run left %d pages in all, want 2", len(pages))
	}
}
