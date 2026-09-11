package main

// A call that thought and answered nothing.
//
// The reasoning arrives in a field of its own on this server -- never inside
// content -- and it was read only to be counted and then dropped. So a model
// that spent its whole budget reasoning and wrote no answer produced: an empty
// reply, a validation failure blaming JSON for an empty string, an empty
// assistant turn echoed back into the retry, and a recorded exchange page with
// nothing on it at all. Four things saying nothing about the one call anybody
// would want to read.
//
// The reply is still the answer alone -- no caller parses the reasoning and no
// retry echoes it back -- but the record keeps it, and everything that reports
// on an empty answer now says it was empty.

import (
	"encoding/json"
	"errors"
	"strings"
	"testing"
)

// The stream keeps the reasoning for the record while keeping it out of the
// reply and out of the progress bar, which is what it was already doing.
func TestTheReasoningIsKeptForTheRecordAndNotForTheReply(t *testing.T) {
	a := &App{root: t.TempDir()}
	w := a.watchChat("suggest", true)
	sse := strings.Join([]string{
		`data: {"choices":[{"delta":{"reasoning_content":"let me think"}}]}`,
		`data: {"choices":[{"delta":{"reasoning_content":" some more"}}]}`,
		`data: {"choices":[{"delta":{"content":"{\"segments\":[]}"}}]}`,
		"data: [DONE]",
	}, "\n\n") + "\n\n"

	rep, err := a.readChatStream(strings.NewReader(sse), func(string) {}, w)
	if err != nil {
		t.Fatalf("stream: %v", err)
	}
	if rep.Content != `{"segments":[]}` {
		t.Errorf("the reply came out %q -- the thinking is in it", rep.Content)
	}
	if rep.Think != "let me think some more" {
		t.Errorf("the reasoning was kept as %q, so the record cannot show it", rep.Think)
	}
	// a stream cut off before [DONE] said what it said, thinking included
	rep, err = a.readChatStream(strings.NewReader(
		`data: {"choices":[{"delta":{"reasoning_content":"half a thought"}}]}`+"\n\n"),
		func(string) {}, a.watchChat("suggest", true))
	if err != nil {
		t.Fatalf("truncated stream: %v", err)
	}
	if rep.Think != "half a thought" || rep.Content != "" {
		t.Errorf("a stream that stopped mid-thought came out think=%q content=%q", rep.Think, rep.Content)
	}
}

// On the page the two spellings of thinking read the same: a model that wraps
// its own in <think> tags and a server that sends it in a field of its own end
// up with one fold, because recorded() puts the second into the first's marks.
func TestTheRecordedReplyWearsTheThinkingTheOneWay(t *testing.T) {
	got := chatReply{Content: "THE ANSWER", Think: "the working"}.recorded()
	if got != "<think>the working</think>THE ANSWER" {
		t.Errorf("the recorded reply is %q", got)
	}
	// a reply that already carries its own tags is left alone: wrapping it
	// again would fold the answer away with the thinking
	inline := chatReply{Content: "<think>inline</think>THE ANSWER"}.recorded()
	if inline != "<think>inline</think>THE ANSWER" {
		t.Errorf("an inline-thinking reply was rewritten to %q", inline)
	}
	// and the tool calls still come after both
	var tc toolCall
	tc.Function.Name, tc.Function.Arguments = "web_search", `{"q":"x"}`
	calls := chatReply{Content: "", Think: "hm", Calls: []toolCall{tc}}.recorded()
	if !strings.Contains(calls, "<think>hm</think>") || !strings.Contains(calls, "[tool call web_search(") {
		t.Errorf("a round that reasoned and called a tool records as %q", calls)
	}
	// splitThink is the one reading of the marker, used by the page and the log
	think, answer := splitThink("<think>a</think>b")
	if think != "<think>a</think>" || answer != "b" {
		t.Errorf("splitThink gave %q / %q", think, answer)
	}
	if think, answer := splitThink("plain"); think != "" || answer != "plain" {
		t.Errorf("splitThink invented a fold: %q / %q", think, answer)
	}
}

// What the retry does with it. "unexpected end of JSON input" is a true
// sentence about an empty string and a useless one to be told, and an empty
// assistant turn is a turn the next round has to make sense of.
func TestAnEmptyAnswerIsNamedAndNotEchoedBack(t *testing.T) {
	if p := noAnswer("<think>all of it</think>   "); p == "" {
		t.Error("a reply that is nothing but reasoning passed as an answer")
	}
	if p := noAnswer(""); p == "" {
		t.Error("an empty reply passed as an answer")
	}
	if p := noAnswer("<think>some</think>{}"); p != "" {
		t.Errorf("a real answer was called empty: %q", p)
	}

	base := []map[string]any{msg("user", "the question")}
	// nothing to echo: the correction carries the whole message
	got := retryTurn(base, "  ", "you returned no answer at all")
	if len(got) != 2 || got[1]["role"] != "user" {
		t.Errorf("an empty answer was echoed back into the history: %+v", got)
	}
	// a real answer is echoed, so the model can see what it is correcting
	got = retryTurn(base, "{bad json", "not valid JSON")
	if len(got) != 3 || got[1]["role"] != "assistant" || got[1]["content"] != "{bad json" {
		t.Errorf("the rejected answer is not in the history: %+v", got)
	}

	// and both loops that retry use it, so neither can drift back to echoing
	// an empty turn
	for _, f := range []string{"narrate.go", "cut_suggest.go"} {
		src := readSrc(t, f)
		if !strings.Contains(src, "msgs = retryTurn(msgs, reply, problem)") {
			t.Errorf("%s builds its own retry turn again", f)
		}
		if !strings.Contains(src, "jsonReply(reply, &out)") {
			t.Errorf("%s parses a reply on its own again", f)
		}
	}
	if src := readSrc(t, "llm.go"); !strings.Contains(src, "problem := noAnswer(reply)") {
		t.Error("jsonReply reports an empty answer as a JSON problem again")
	}
}

// The same question asked the same way gets the same silence. A model that
// answered nothing was cut off inside its own reasoning, so the attempt after
// an empty one is asked with thinking off -- the shorter ceiling and no
// reasoning is what makes the words arrive. Only after an empty one: bad JSON
// is a model that is writing and getting it wrong, and taking its reasoning
// away would not help.
func TestAnEmptyAnswerTurnsTheThinkingOffForTheNextTry(t *testing.T) {
	for _, c := range []struct {
		what  string
		think bool
		reply string
		want  bool
	}{
		{"an answer that was only reasoning", true, "<think>on and on</think>", false},
		{"an answer that never started", true, "", false},
		{"an answer that came out as bad JSON", true, "{oops", true},
		{"an answer that was fine", true, `{"segments":[]}`, true},
		{"a call that already had thinking off", false, "", false},
	} {
		if got := thinkAgain(c.think, c.reply); got != c.want {
			t.Errorf("after %s the next try thinks=%v, want %v", c.what, got, c.want)
		}
	}
	// both loops carry the flag into the call rather than hard-coding true,
	// or the escape hatch is one nothing opens
	for _, f := range []struct{ file, step string }{
		{"cut_suggest.go", "suggest"}, {"narrate.go", "narrate"},
	} {
		src := readSrc(t, f.file)
		for _, want := range []string{
			`reply, err := a.llmChatRetryTools("` + f.step + `", msgs, think, tools,`,
			"if next := thinkAgain(think, reply); next != think {",
		} {
			if !strings.Contains(src, want) {
				t.Errorf("%s no longer contains %q", f.file, want)
			}
		}
	}
}

// An answer the token ceiling chopped in half.
//
// The server says so -- finish_reason "length" -- and nothing downstream could
// see it: the text just ends mid-word, json says "unexpected end of JSON
// input", and the reader goes looking for a fault in an answer that was never
// finished. One run answered with four hundred segments marching past the end
// of a 28-minute session and was chopped mid-number, three times over.
func TestATruncatedReplyIsNamedAsOne(t *testing.T) {
	a := &App{root: t.TempDir()}
	sse := strings.Join([]string{
		`data: {"choices":[{"delta":{"content":"{\"segments\":[{\"start\":10,\"en"}}]}`,
		`data: {"choices":[{"delta":{},"finish_reason":"length"}]}`,
		"data: [DONE]",
	}, "\n\n") + "\n\n"
	rep, err := a.readChatStream(strings.NewReader(sse), func(string) {}, a.watchChat("suggest", true))
	if err != nil {
		t.Fatalf("stream: %v", err)
	}
	if rep.Stop != "length" {
		t.Errorf("the stream kept the stop reason as %q, so no one downstream can say why", rep.Stop)
	}

	// and the correction the model gets asks for a SHORTER answer: telling it
	// to return corrected JSON invites the same too-long reply again
	var out struct {
		Segments []struct{ Start, End float64 }
	}
	jsonErr := json.Unmarshal([]byte(rep.Content), &out)
	if jsonErr == nil {
		t.Fatal("the truncated reply parsed, so this test proves nothing")
	}
	p := cutOff(rep.Content, jsonErr)
	if !strings.Contains(p, "stopped in the middle") || !strings.Contains(p, "fewer") {
		t.Errorf("a truncated answer is reported as %q", p)
	}
	// a malformed answer is not a truncated one: it wants care, not brevity
	if p := cutOff("{\"segments\": nonsense}", errors.New("invalid character 'o'")); p != "" {
		t.Errorf("a malformed answer was called truncated: %q", p)
	}
	if p := cutOff("", errors.New("unexpected end of JSON input")); p != "" {
		t.Error("an empty answer was called truncated -- noAnswer is the one that names that")
	}
	// the one reader every loop goes through asks before falling back to the
	// parser's own words
	if src := readSrc(t, "llm.go"); !strings.Contains(src, "if problem = cutOff(reply, err); problem == \"\" {") {
		t.Error("jsonReply reports a truncated answer as an ordinary parse error")
	}
}
