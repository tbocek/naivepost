package main

// The streaming half of the chat client. It exists for one thing: a narration
// is a single request that thinks for a minute and then writes every clip, and
// the run bar had nothing to say about it but "…". Streamed, the clips can be
// counted as they close. The server here is a local httptest one -- these tests
// never reach the configured LLM.

import (
	"encoding/json"
	"fmt"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
)

// sseServer answers one chat completion as server-sent events, one event per
// part, the way llama-server does.
func sseServer(t *testing.T, parts ...string) (*App, *httptest.Server, *bool) {
	t.Helper()
	streamed := false
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		var body map[string]any
		if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
			t.Errorf("request body: %v", err)
		}
		streamed, _ = body["stream"].(bool)
		w.Header().Set("Content-Type", "text/event-stream")
		for _, p := range parts {
			b, _ := json.Marshal(map[string]any{
				"choices": []any{map[string]any{"delta": map[string]any{"content": p}}},
			})
			fmt.Fprintf(w, "data: %s\n\n", b)
			w.(http.Flusher).Flush()
		}
		fmt.Fprint(w, "data: [DONE]\n\n")
	}))
	t.Cleanup(srv.Close)
	a := &App{root: t.TempDir()}
	if err := a.writeConf(appConf{Server: srv.URL, Model: "test"}); err != nil {
		t.Fatal(err)
	}
	return a, srv, &streamed
}

func TestAStreamedReplyIsReadableWhileItIsWritten(t *testing.T) {
	parts := []string{
		"<think>Nine clips. The first one is the fall.</think>\n",
		`{"entries":[{"start":1,"end":5,"text":"So this is where it starts.",`,
		`"emotion":"dry"},`,
		`{"start":20,"end":26,"text":"And that is where it ends.","emotion":"warm"}]}`,
	}
	a, _, streamed := sseServer(t, parts...)

	var seen []string
	reply, err := a.llmChatOn("narrate", []map[string]any{msg("user", "write it")}, true,
		func(s string) { seen = append(seen, s) })
	if err != nil {
		t.Fatal(err)
	}
	if !*streamed {
		t.Error("the request did not ask for a stream, so nothing arrives until the end")
	}
	if want := strings.Join(parts, ""); reply != want {
		t.Errorf("the assembled reply is\n%s\nwant\n%s", reply, want)
	}
	if len(seen) != len(parts) {
		t.Fatalf("the caller heard %d updates for %d events", len(seen), len(parts))
	}
	// each update is everything so far, in order -- what a counter needs
	for i, s := range seen {
		if !strings.HasPrefix(reply, s) {
			t.Errorf("update %d is not a prefix of the reply: %q", i, s)
		}
		if i > 0 && len(s) <= len(seen[i-1]) {
			t.Errorf("update %d did not grow: %q", i, s)
		}
	}
	// and the point of all of it: the count moves while the reply is arriving
	if got := narrEntriesDone(seen[1]); got != 0 {
		t.Errorf("a clip still being written counted as %d done", got)
	}
	if got := narrEntriesDone(seen[2]); got != 1 {
		t.Errorf("the first finished clip counted as %d", got)
	}
	if got := narrEntriesDone(seen[3]); got != 2 {
		t.Errorf("the finished narration counted as %d clips", got)
	}
}

// A server that ignores the stream flag, or answers an error, sends plain JSON.
// That must still be the reply and not an empty string: the fallback is what
// keeps the narration working against anything OpenAI-compatible, streaming or
// not.
func TestAServerThatWillNotStreamStillAnswers(t *testing.T) {
	ownConfig(t)
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		fmt.Fprint(w, `{"choices":[{"message":{"content":"{\"entries\":[]}"}}]}`)
	}))
	defer srv.Close()
	a := &App{root: t.TempDir()}
	if err := a.writeConf(appConf{Server: srv.URL, Model: "test"}); err != nil {
		t.Fatal(err)
	}
	called := 0
	reply, err := a.llmChatOn("narrate", []map[string]any{msg("user", "x")}, true,
		func(string) { called++ })
	if err != nil {
		t.Fatal(err)
	}
	if reply != `{"entries":[]}` {
		t.Errorf("reply = %q", reply)
	}
	if called != 0 {
		t.Errorf("the progress callback ran %d times on a reply that arrived whole", called)
	}
}

// TestTheClipCountIsOnlyEverWhatIsFinished pins the counter itself, including
// the two things that would make a bar lie: a brace inside a line of narration,
// and the model's own thinking about the JSON it is about to write.
func TestTheClipCountIsOnlyEverWhatIsFinished(t *testing.T) {
	for _, c := range []struct {
		name string
		in   string
		want int
	}{
		{"nothing yet", `<think>I will write "entries" with nine objects {like this}</think>`, 0},
		{"the key without its array", `{"entries" : "nine of them {x}"`, 0},
		{"array opened", `{"entries":[`, 0},
		{"half a clip", `{"entries":[{"start":1,"text":"so`, 0},
		{"one closed", `{"entries":[{"start":1,"text":"so"},`, 1},
		{"brace in the line", `{"entries":[{"text":"we hit {this} wall"},{"text":"x`, 1},
		{"escaped quote in the line", `{"entries":[{"text":"he said \"go\" and went"},`, 1},
		{"whole answer", `{"entries":[{"text":"a"},{"text":"b"},{"text":"c"}]}`, 3},
		{"after the thinking", "<think>{\"entries\":[{}]}? no</think>\n" +
			`{"entries":[{"text":"a"},{"text":"b"}]}`, 2},
	} {
		if got := narrEntriesDone(c.in); got != c.want {
			t.Errorf("%s: counted %d finished clips, want %d", c.name, got, c.want)
		}
	}
}

// An empty Server box is an answer, the way it is for the audio and image
// servers: the writing model runs on this machine too, on halogen-flash-
// server's own port. It used to be a box with no default and a placeholder
// reading "https://ai.example.com", so a machine running the stack as it comes
// was told "no LLM configured" until three URLs had been typed.
func TestAnEmptyWritingServerIsTheLocalOne(t *testing.T) {
	if got, want := llmServer(appConf{}), "http://127.0.0.1:8731"; got != want {
		t.Errorf("with nothing typed, llmServer = %q, want %q", got, want)
	}
	if got := llmServer(appConf{Server: "  https://ai.jos.li/  "}); got != "https://ai.jos.li" {
		t.Errorf("a typed server gave %q -- its spaces and trailing slash have to go, "+
			"because a path is joined straight onto it", got)
	}
	// and the model still has none: the id has to be one the server lists, and
	// a made-up default would fail every call with a name nobody typed
	if (appConf{}).withDefaults().Model != "" {
		t.Error("the model id was given a default; Fetch models is how that box is filled")
	}
}

// Thinking is switched off in BOTH spellings, because the two servers this
// talks to read different ones: llama.cpp renders the chat template itself and
// takes chat_template_kwargs, halogen-flash-server takes the same two names at
// the top level (/health lists them under "supported") and ignores what it does
// not know.
//
// The nested spelling alone is what shipped, and halogen never saw it: the
// Settings Test asks for one word inside a 16-token budget, the model spent all
// 16 on reasoning, and the round trip came back with empty content and
// finish_reason "length" -- reported to the user as a fault of the model. With
// the switch at the top level the same request answers in two tokens.
func TestTheThinkingSwitchGoesOutInBothSpellings(t *testing.T) {
	for _, on := range []bool{true, false} {
		body := map[string]any{}
		thinkSwitch(body, on)
		if body["enable_thinking"] != on {
			t.Errorf("top-level enable_thinking is %v, want %v", body["enable_thinking"], on)
		}
		kw, ok := body["chat_template_kwargs"].(map[string]any)
		if !ok {
			t.Fatalf("no chat_template_kwargs: %v", body)
		}
		if kw["enable_thinking"] != on {
			t.Errorf("templated enable_thinking is %v, want %v", kw["enable_thinking"], on)
		}
		// the reasoning is kept either way: the recorded page is the only
		// place a run's thinking can be read afterwards
		if body["preserve_thinking"] != true || kw["preserve_thinking"] != true {
			t.Error("preserve_thinking is not sent in both spellings")
		}
	}
	// and both callers go through it -- the pipeline's own call and the Test
	// button, which has to fail and pass for the same reasons the run does
	if !strings.Contains(readSrc(t, "llm.go"), "thinkSwitch(body, thinking)") {
		t.Error("llmChatPost sets the thinking switch some other way")
	}
	if !strings.Contains(readSrc(t, "setup.go"), "thinkSwitch(req, false)") {
		t.Error("the Settings Test does not send the switch the pipeline sends, so it can " +
			"pass or fail for reasons a run never meets")
	}
}
