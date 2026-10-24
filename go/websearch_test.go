package main

// The web tools without a Firefox: the ladder over a table of answers, the
// tool loop over a fake server, and the shape the model is offered. What a
// real search returns is the web's business; what these pin is that three
// queries become the narrowest one with results, that a round of calls is
// answered and asked again, and that a job with no firefox is offered nothing.

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
)

// The ladder climbs from broad to narrow and keeps the narrowest query the
// web still answered: a name too specific for the web is answered by the step
// below it, and a name so broad it means another game is passed on the way
// up. Nothing anywhere is nothing.
func TestTheLadderKeepsTheNarrowestQueryWithResults(t *testing.T) {
	table := func(answers map[string]int) searcher {
		return func(_ context.Context, q string) ([]webHit, error) {
			var hits []webHit
			for i := 0; i < answers[q]; i++ {
				hits = append(hits, webHit{Title: fmt.Sprintf("%s %d", q, i), URL: "https://x/" + q})
			}
			return hits, nil
		}
	}
	for _, c := range []struct {
		name    string
		answers map[string]int
		want    string
		n       int
	}{
		{"every step answers: the narrowest wins", map[string]int{"game": 9, "game thing": 4, "kenos tower": 2}, "kenos tower", 2},
		{"the web does not know the exact name: the step below", map[string]int{"game": 9, "game thing": 4, "kenos tower": 0}, "game thing", 4},
		{"only the broad one answers", map[string]int{"game": 3}, "game", 3},
		{"nothing answers", map[string]int{}, "", 0},
		// the climb stops where the web goes dry: a middle step with nothing
		// is the web saying the name is already too specific, and the narrow
		// step is not tried -- three searches for a name the web does not
		// know would be paid for on every caption
		{"the middle step is empty", map[string]int{"game": 5, "kenos tower": 1}, "game", 5},
	} {
		q, hits, err := searchLadder(context.Background(), table(c.answers), "game", "game thing", "kenos tower")
		if err != nil {
			t.Errorf("%s: %v", c.name, err)
		}
		if q != c.want || len(hits) != c.n {
			t.Errorf("%s: used %q with %d hits, want %q with %d", c.name, q, len(hits), c.want, c.n)
		}
	}
	// a search that fails is the web's fault, not silence: with nothing found
	// at any step the error is reported, and with something found it is not
	broken := func(_ context.Context, q string) ([]webHit, error) {
		if q == "game thing" {
			return nil, errors.New("firefox fell over")
		}
		return table(map[string]int{"game": 2})(context.Background(), q)
	}
	if q, hits, err := searchLadder(context.Background(), broken, "game", "game thing", "kenos tower"); err != nil || q != "game" || len(hits) != 2 {
		t.Errorf("one failed step lost the ladder: %q %d %v", q, len(hits), err)
	}
	if _, _, err := searchLadder(context.Background(), func(context.Context, string) ([]webHit, error) {
		return nil, errors.New("no firefox")
	}, "a", "b", "c"); err == nil {
		t.Error("a search that never worked was reported as no results")
	}
	// and what the model reads back names the query the results are for, so a
	// broader answer is read as a broader answer
	if s := formatHits("game thing", []webHit{{Title: "T", URL: "https://x", Snippet: "s"}}); !strings.HasPrefix(s, `Results for "game thing":`) || !strings.Contains(s, "https://x") {
		t.Errorf("the results read:\n%s", s)
	}
}

// The tool loop: a round that answers with calls is answered with one tool
// message per call and asked again, with the assistant turn echoed as it
// came; the round that answers with words is the reply. Over the wire, in
// both shapes the server speaks.
func TestACallForAToolIsAnsweredAndAskedAgain(t *testing.T) {
	for _, streamed := range []bool{false, true} {
		var got []map[string]any // the messages of the last request
		round := 0
		srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			var body struct {
				Messages []map[string]any `json:"messages"`
				Tools    []map[string]any `json:"tools"`
				Stream   bool             `json:"stream"`
			}
			json.NewDecoder(r.Body).Decode(&body)
			got = body.Messages
			if len(body.Tools) != 2 {
				t.Errorf("the request carried %d tools, want the two", len(body.Tools))
			}
			round++
			if round == 1 {
				// the model asks for a search, in two argument pieces when streamed
				if body.Stream {
					w.Header().Set("Content-Type", "text/event-stream")
					fmt.Fprint(w, `data: {"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_1","type":"function","function":{"name":"web_search","arguments":"{\"broad\":\"game\","}}]}}]}`+"\n\n")
					fmt.Fprint(w, `data: {"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"\"medium\":\"m\",\"narrow\":\"n\"}"}}]}}]}`+"\n\n")
					fmt.Fprint(w, "data: [DONE]\n\n")
					return
				}
				fmt.Fprint(w, `{"choices":[{"message":{"content":"","tool_calls":[{"id":"call_1","type":"function","function":{"name":"web_search","arguments":"{\"broad\":\"game\",\"medium\":\"m\",\"narrow\":\"n\"}"}}]}}]}`)
				return
			}
			if body.Stream {
				w.Header().Set("Content-Type", "text/event-stream")
				fmt.Fprint(w, `data: {"choices":[{"delta":{"content":"the tower costs 300 slaps"}}]}`+"\n\n")
				fmt.Fprint(w, "data: [DONE]\n\n")
				return
			}
			fmt.Fprint(w, `{"choices":[{"message":{"content":"the tower costs 300 slaps"}}]}`)
		}))
		ownConfig(t)
		a := &App{outDir: t.TempDir()}
		if err := a.writeConf(appConf{Server: srv.URL, Model: "test"}); err != nil {
			t.Fatal(err)
		}
		var asked string
		run := func(name string, args json.RawMessage) string {
			asked = name + " " + string(args)
			return "1. Kenos Tower wiki\n   https://w/kenos\n   costs 300 slaps"
		}
		var onText func(string)
		if streamed {
			onText = func(string) {}
		}
		reply, err := a.llmChatTools("suggest", []map[string]any{msg("user", "cut it")}, true, webTools(), run, onText)
		srv.Close()
		if err != nil {
			t.Fatalf("streamed=%v: %v", streamed, err)
		}
		if reply != "the tower costs 300 slaps" {
			t.Errorf("streamed=%v: the reply is %q, want the second round's words", streamed, reply)
		}
		if asked != `web_search {"broad":"game","medium":"m","narrow":"n"}` {
			t.Errorf("streamed=%v: the tool was asked %q -- the call's arguments did not survive the wire", streamed, asked)
		}
		if round != 2 {
			t.Errorf("streamed=%v: %d rounds, want 2", streamed, round)
		}
		// the second request carried the whole exchange: the question, the
		// assistant's call as it came, and the tool's answer under the call's id
		if len(got) != 3 {
			t.Fatalf("streamed=%v: the second request carried %d messages, want 3:\n%v", streamed, len(got), got)
		}
		if got[1]["role"] != "assistant" || got[1]["tool_calls"] == nil {
			t.Errorf("streamed=%v: the assistant turn was not echoed with its calls: %v", streamed, got[1])
		}
		if got[2]["role"] != "tool" || got[2]["tool_call_id"] != "call_1" || !strings.Contains(fmt.Sprint(got[2]["content"]), "300 slaps") {
			t.Errorf("streamed=%v: the tool message is %v", streamed, got[2])
		}
	}
}

// A model that never stops calling is stopped; and with no tools on the
// table the call is the plain one, tool-shaped answers and all.
func TestTheToolLoopEnds(t *testing.T) {
	rounds := 0
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		rounds++
		fmt.Fprint(w, `{"choices":[{"message":{"content":"","tool_calls":[{"id":"c","type":"function","function":{"name":"web_search","arguments":"{}"}}]}}]}`)
	}))
	defer srv.Close()
	ownConfig(t)
	a := &App{outDir: t.TempDir()}
	if err := a.writeConf(appConf{Server: srv.URL, Model: "test"}); err != nil {
		t.Fatal(err)
	}
	run := func(string, json.RawMessage) string { return "nothing" }
	if _, err := a.llmChatTools("x", []map[string]any{msg("user", "q")}, false, webTools(), run, nil); err == nil {
		t.Error("a model calling tools forever was not stopped")
	}
	if rounds != toolRounds {
		t.Errorf("%d rounds, want %d", rounds, toolRounds)
	}
	rounds = 0
	if reply, err := a.llmChatTools("x", []map[string]any{msg("user", "q")}, false, nil, nil, nil); err != nil || reply != "" || rounds != 1 {
		t.Errorf("with no tools: reply %q, err %v, %d rounds -- want the one plain call", reply, err, rounds)
	}
}

// What a job is offered: the two tools, in the shape the API takes, each with
// the instruction in its description -- and nothing at all when the settings
// say off, which is the one case that must never reach the wire.
func TestTheToolsAreOfferedOnlyWhereThereIsAFirefox(t *testing.T) {
	tools := webTools()
	names := map[string]bool{}
	for _, tl := range tools {
		fn, _ := tl["function"].(map[string]any)
		names[fmt.Sprint(fn["name"])] = true
		if !strings.Contains(fmt.Sprint(fn["description"]), "guess") && fn["name"] == "web_search" {
			t.Error("the search tool's description no longer says what it is for: a fact you would otherwise guess")
		}
	}
	if !names["web_search"] || !names["web_read"] {
		t.Errorf("the tools are %v", names)
	}
	if _, err := firefoxBin(firefoxOff); err == nil {
		t.Error("off found a firefox")
	}
	if _, err := firefoxBin("/no/such/firefox"); err == nil {
		t.Error("a path that is not there was accepted")
	}
	ownConfig(t)
	a := &App{}
	if err := a.writeConf(appConf{Server: "http://x", Model: "m", Firefox: firefoxOff}); err != nil {
		t.Fatal(err)
	}
	if tools, _ := a.webToolsFor("x"); tools != nil {
		t.Error("with firefox off the job was still offered the tools")
	}
	// the setting survives the file
	if got := a.readConf().Firefox; got != firefoxOff {
		t.Errorf("FIREFOX came back as %q", got)
	}
	// and every job that writes words the viewer reads asks for them: the cut
	// (captions), the narration and the upload text -- through the one loop
	for file, step := range map[string]string{"cut_suggest.go": "suggest", "narrate.go": "narrate", "publish.go": "publish"} {
		src := readSrc(t, file)
		if !strings.Contains(src, `a.webToolsFor("`+step+`")`) || !strings.Contains(src, `a.llmChatRetryTools("`+step+`"`) {
			t.Errorf("%s does not offer the web tools to the %s job", file, step)
		}
	}
	// the page is read as a wall, not as a page
	for _, wall := range []string{"Just a moment...", "Verify you are human", ""} {
		if pageIssue(wall) == "" {
			t.Errorf("%q was read as a page", wall)
		}
	}
	if pageIssue(strings.Repeat("Kenos Tower costs 300 slaps and shoots twice a second. ", 10)) != "" {
		t.Error("a real page was read as a wall")
	}
}
