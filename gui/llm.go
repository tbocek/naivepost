package main

// Chat client for the llama-server in llm.conf. Two parameter sets: "thinking"
// for judgment calls, "execute" with thinking off for mechanical ones (32 s vs
// 2 s per frame description). A call may offer tools (llmChatTools): OpenAI
// shape, one tool message per call, asked again until the model answers with
// words; every round is its own recorded exchange (websearch.go).

import (
	"bufio"
	"bytes"
	"context"
	"encoding/base64"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"os"
	"strings"
	"time"
)

func txtPart(s string) map[string]any {
	return map[string]any{"type": "text", "text": s}
}

func imgPart(path string) (map[string]any, error) {
	b, err := os.ReadFile(path)
	if err != nil {
		return nil, err
	}
	return map[string]any{
		"type": "image_url",
		"image_url": map[string]any{
			"url": "data:image/jpeg;base64," + base64.StdEncoding.EncodeToString(b),
		},
	}, nil
}

func msg(role string, content any) map[string]any {
	return map[string]any{"role": role, "content": content}
}

// toolCall is one call the model made, in the wire's own shape, so it can be
// echoed back in the assistant turn exactly as it arrived.
type toolCall struct {
	ID       string `json:"id"`
	Type     string `json:"type"`
	Function struct {
		Name      string `json:"name"`
		Arguments string `json:"arguments"`
	} `json:"function"`
}

// How much the server may write in one answer, reasoning included. The
// thinking ceiling is the one that gets hit: a cut over a 28-minute session
// once stopped mid-sum at 68 kB of reasoning with no answer.
const (
	thinkTokens = 65536
	plainTokens = 8192
)

// chatReply is what one round returns: words, or calls, or both.
type chatReply struct {
	Content string
	// what the model told itself on the way to the answer, when the server
	// keeps it out of the content instead of wrapping it in <think> tags.
	// It is NOT part of the reply -- no caller parses it, no retry echoes it
	// back -- and it exists for one reason: the recorded exchange (chatRec).
	// A call that reasons for three minutes and answers nothing was written
	// down as a blank page, which is the one case the pages are read for.
	Think string
	Calls []toolCall
	// why the server stopped writing, as it says it: "stop" for an answer that
	// ended, "length" for one that ran into the token ceiling and was cut off
	// mid-word. The difference is invisible in the text -- a truncated JSON
	// reply is just a parse error, and "unexpected end of JSON input" sends
	// the reader looking for a bug in an answer that was never finished.
	Stop string
}

// callSummary is a round's calls as one short phrase for the log: the tool and
// what it was pointed at, so two rounds asking the same thing read the same and
// two asking different things do not.
func callSummary(calls []toolCall) string {
	var out []string
	for _, c := range calls {
		arg := strings.TrimSpace(c.Function.Arguments)
		if len(arg) > 60 {
			arg = arg[:60] + "…"
		}
		out = append(out, c.Function.Name+arg)
	}
	return strings.Join(out, ", ")
}

// toolRunner answers one call by name; what it returns is what the model
// reads as the tool's result.
type toolRunner func(name string, args json.RawMessage) string

// the most rounds of tool calls one question may take. A model that has not
// found its words after this many has found a loop instead.
const toolRounds = 8

// llmChat posts a chat completion; thinking selects the parameter set. step
// names the caller -- "suggest", "describe" -- and is what the recorded
// exchange is filed and logged under (recordChatStart in llmlog.go).
func (a *App) llmChat(step string, msgs []map[string]any, thinking bool) (string, error) {
	return a.llmChatOn(step, msgs, thinking, nil)
}

// llmChatOn is llmChat with the reply readable as it arrives: onText gets
// everything received so far on every chunk, on the calling goroutine; nil
// means the old single-response call. Streaming is what lets the bar count
// narration clips as they close (narrEntriesDone). Every call is recorded as
// an HTML page under llm/ (request first, reply folded in) with a preview in
// the log.
func (a *App) llmChatOn(step string, msgs []map[string]any, thinking bool, onText func(string)) (string, error) {
	rep, err := a.chatRound(step, msgs, thinking, nil, onText)
	return rep.Content, err
}

// llmChatTools is llmChatOn with tools on the table. The loop is here and
// nowhere else: a round that comes back with calls is answered -- the
// assistant turn echoed with its calls, then one tool message per call, in
// order -- and the question is put again with all of it in the history, until
// a round comes back with words alone. Those words are the reply; the rounds
// before them are in the recorded exchanges.
func (a *App) llmChatTools(step string, msgs []map[string]any, thinking bool,
	tools []map[string]any, run toolRunner, onText func(string)) (string, error) {
	if len(tools) == 0 || run == nil {
		return a.llmChatOn(step, msgs, thinking, onText)
	}
	msgs = append([]map[string]any(nil), msgs...)
	last := "" // what the round before asked for, to catch a model going in circles
	for round := 0; round < toolRounds; round++ {
		rep, err := a.chatRound(step, msgs, thinking, tools, onText)
		if err != nil && round == 0 && !errors.Is(err, errStopped) {
			// a server that will not take a tools field answers the first
			// round with an error and nothing else; the job is worth more
			// than the search, so it is asked again plainly
			a.logfIdle(">>> %s: the server refused the request with tools (%v) -- asked again without them", step, err)
			return a.llmChatOn(step, msgs, thinking, onText)
		}
		if err != nil {
			return "", err
		}
		if len(rep.Calls) == 0 {
			return rep.Content, nil
		}
		// the round itself, in the log. The tools log what they DID -- what was
		// searched, what was read -- and that was the whole of it: a step six
		// minutes into a tool loop showed a handful of search lines with
		// nothing tying them to the call they belong to, no count of how many
		// rounds were left, and no sign at all when the loop ran out. The
		// round is the fact; the searches under it are the detail.
		asked := callSummary(rep.Calls)
		if asked == last {
			// the tell for a model going in circles: the identical call again,
			// which is what eight rounds of one wiki page looked like from the
			// outside -- nothing, until the step failed
			a.logfIdle(">>> %s: round %d of %d asks for the same thing again — %s",
				step, round+1, toolRounds, asked)
		} else {
			a.logfIdle(">>> %s: round %d of %d — the model asked for %s",
				step, round+1, toolRounds, asked)
		}
		last = asked
		msgs = append(msgs, map[string]any{"role": "assistant", "content": rep.Content, "tool_calls": rep.Calls})
		for _, c := range rep.Calls {
			msgs = append(msgs, map[string]any{"role": "tool", "tool_call_id": c.ID,
				"content": run(c.Function.Name, json.RawMessage(c.Function.Arguments))})
		}
	}
	a.logfIdle("!!! %s: still calling tools after %d rounds — the step gets no answer from this call",
		step, toolRounds)
	return "", fmt.Errorf("the model was still calling tools after %d rounds", toolRounds)
}

// chatRound is one request and its recorded exchange.
func (a *App) chatRound(step string, msgs []map[string]any, thinking bool,
	tools []map[string]any, onText func(string)) (chatReply, error) {
	rec := a.recordChatStart(step, thinking, msgs)
	// tee an existing stream through the live page; a caller with no callback
	// stays unstreamed (wrapping nil would flip the wire request to streaming)
	if onText != nil {
		user := onText
		onText = func(s string) { rec.stream(s); user(s) }
	}
	t0 := time.Now()
	rep, err := a.llmChatPost(step, msgs, thinking, tools, onText)
	rec.done(rep.recorded(), rep.Stop, time.Since(t0), err)
	return rep, err
}

// recorded is the reply as the exchange log shows it: thinking, words, then
// the calls. Reasoning is wrapped in <think> tags -- how an inlining model
// arrives anyway, and what chatHTML folds away -- so the page has one spelling.
func (r chatReply) recorded() string {
	body := r.Content
	if strings.TrimSpace(r.Think) != "" && !strings.Contains(body, "</think>") {
		body = "<think>" + r.Think + "</think>" + body
	}
	if len(r.Calls) == 0 {
		return body
	}
	var b strings.Builder
	b.WriteString(body)
	for _, c := range r.Calls {
		fmt.Fprintf(&b, "\n[tool call %s(%s)]", c.Function.Name, c.Function.Arguments)
	}
	return b.String()
}

// llmChatPost is the wire call itself: build the body, post it, read the answer.
// step names the caller so the watch can say whose call is running (llmstall.go).
func (a *App) llmChatPost(step string, msgs []map[string]any, thinking bool,
	tools []map[string]any, onText func(string)) (chatReply, error) {
	c := a.readConf()
	if c.Server == "" || c.Model == "" {
		return chatReply{}, fmt.Errorf("no LLM configured -- use the gear button")
	}
	body := map[string]any{
		"model":            c.Model,
		"messages":         msgs,
		"top_p":            0.95,
		"top_k":            20,
		"min_p":            0.0,
		"presence_penalty": 0.0,
	}
	if thinking {
		body["temperature"] = 1.0
		body["max_tokens"] = thinkTokens
		body["chat_template_kwargs"] = map[string]any{"preserve_thinking": true}
	} else {
		body["temperature"] = 0.6
		body["max_tokens"] = plainTokens
		body["chat_template_kwargs"] = map[string]any{
			"preserve_thinking": true, "enable_thinking": false,
		}
	}
	if len(tools) > 0 {
		body["tools"] = tools
	}
	if onText != nil {
		body["stream"] = true
	}
	buf, err := json.Marshal(body)
	if err != nil {
		return chatReply{}, err
	}
	ctx := a.runCtx
	if ctx == nil {
		ctx = context.Background()
	}
	// the watch holds the call to a silence rule and says every minute what is
	// arriving; a call nobody streams has no silence to measure, so it keeps a
	// whole-call ceiling on the client instead
	w := a.watchChat(step, onText != nil)
	ctx, cancel := context.WithCancel(ctx)
	defer cancel()
	stop := w.guard(cancel)
	defer stop()
	req, err := http.NewRequestWithContext(ctx, "POST",
		strings.TrimRight(c.Server, "/")+"/v1/chat/completions", bytes.NewReader(buf))
	if err != nil {
		return chatReply{}, err
	}
	bearer(req, c.Key)
	req.Header.Set("Content-Type", "application/json")
	client := &http.Client{}
	if onText == nil {
		client.Timeout = llmWhole
	}
	resp, err := client.Do(req)
	if err != nil {
		if a.stopFlag.Load() {
			return chatReply{}, errStopped
		}
		return chatReply{}, w.blame(err)
	}
	defer resp.Body.Close()
	// only if the server actually streamed: one that ignores the flag, or that
	// answers an error as plain JSON, falls through to the decode below
	if onText != nil && strings.Contains(resp.Header.Get("Content-Type"), "text/event-stream") {
		rep, err := a.readChatStream(w.wrap(resp.Body), onText, w)
		return rep, w.blame(err)
	}
	var out struct {
		Choices []struct {
			Message struct {
				Content string `json:"content"`
				// the same out-of-band thinking readChatStream reads, for a
				// call nobody streamed: kept for the record and nothing else
				Reasoning string     `json:"reasoning_content"`
				ToolCalls []toolCall `json:"tool_calls"`
			} `json:"message"`
			FinishReason string `json:"finish_reason"`
		} `json:"choices"`
		Error any `json:"error"`
	}
	if err := json.NewDecoder(resp.Body).Decode(&out); err != nil {
		return chatReply{}, fmt.Errorf("bad response (%s): %w", resp.Status, err)
	}
	if len(out.Choices) == 0 {
		return chatReply{}, fmt.Errorf("no choices (%s): %v", resp.Status, out.Error)
	}
	return chatReply{Content: out.Choices[0].Message.Content,
		Think: out.Choices[0].Message.Reasoning, Calls: out.Choices[0].Message.ToolCalls,
		Stop: out.Choices[0].FinishReason}, nil
}

// readChatStream assembles a server-sent-event reply, handing the caller the
// text so far as each piece lands. bufio.Reader, not Scanner: one event can be
// a line of any length. A tool call arrives in pieces stamped with its index
// and is reassembled by it.
func (a *App) readChatStream(r io.Reader, onText func(string), w *chatWatch) (chatReply, error) {
	br := bufio.NewReader(r)
	var b, think strings.Builder
	var calls []toolCall
	stop := "" // the reason the LAST chunk gives for stopping; see chatReply.Stop
	for {
		line, err := br.ReadString('\n')
		if s := strings.TrimSpace(line); strings.HasPrefix(s, "data:") {
			payload := strings.TrimSpace(strings.TrimPrefix(s, "data:"))
			if payload == "[DONE]" {
				return chatReply{Content: b.String(), Think: think.String(), Calls: calls, Stop: stop}, nil
			}
			var ch struct {
				Choices []struct {
					Delta struct {
						Content string `json:"content"`
						// the model's reasoning. This server keeps it out of content, so a call
						// thinking hard used to look hung; it is counted and shown (onText feeds the
						// bar and the recorded page) and goes nowhere near the reply.
						Reasoning string `json:"reasoning_content"`
						ToolCalls []struct {
							Index    int    `json:"index"`
							ID       string `json:"id"`
							Type     string `json:"type"`
							Function struct {
								Name      string `json:"name"`
								Arguments string `json:"arguments"`
							} `json:"function"`
						} `json:"tool_calls"`
					} `json:"delta"`
					FinishReason string `json:"finish_reason"`
				} `json:"choices"`
			}
			// a chunk that will not parse is a chunk, not the reply: keep reading
			if json.Unmarshal([]byte(payload), &ch) == nil && len(ch.Choices) > 0 {
				if r := ch.Choices[0].FinishReason; r != "" {
					stop = r
				}
				d := ch.Choices[0].Delta
				if d.Reasoning != "" {
					w.wrote(d.Reasoning, true)
					think.WriteString(d.Reasoning) // for the record, not for the reply
				}
				if d.Content != "" {
					w.wrote(d.Content, false)
					b.WriteString(d.Content)
					onText(b.String())
				}
				for _, tc := range d.ToolCalls {
					for len(calls) <= tc.Index {
						calls = append(calls, toolCall{})
					}
					c := &calls[tc.Index]
					if tc.ID != "" {
						c.ID = tc.ID
					}
					if tc.Type != "" {
						c.Type = tc.Type
					}
					if tc.Function.Name != "" {
						c.Function.Name += tc.Function.Name
					}
					c.Function.Arguments += tc.Function.Arguments
					w.wrote(tc.Function.Arguments, true)
				}
			}
		}
		if err != nil {
			if errors.Is(err, io.EOF) {
				// a stream that ends without [DONE] still said what it said
				return chatReply{Content: b.String(), Think: think.String(), Calls: calls, Stop: stop}, nil
			}
			if a.stopFlag.Load() {
				return chatReply{}, errStopped
			}
			return chatReply{}, err // a half-written JSON reply is worth less than the error
		}
	}
}

// jsonItemsDone reads a reply still arriving: how many objects have closed
// inside the LAST "<key>": [ (everything before is thinking, which may mention
// the key), and the "end" of the last closed one for shapes that carry it.
// Only complete items count. The end time is the bar's denominator-free
// progress: the model walks the session from the front.
func jsonItemsDone(s, key string) (done int, through float64) {
	q := `"` + key + `"`
	i := -1
	for at := 0; ; {
		j := strings.Index(s[at:], q)
		if j < 0 {
			break
		}
		j += at
		at = j + 1
		rest := strings.TrimLeft(s[j+len(q):], " \t\r\n")
		if !strings.HasPrefix(rest, ":") {
			continue
		}
		if strings.HasPrefix(strings.TrimLeft(rest[1:], " \t\r\n"), "[") {
			i = j
		}
	}
	if i < 0 {
		return 0, 0
	}
	tail := s[i:]
	depth, from := 0, 0
	inStr, esc := false, false
	for k, r := range tail {
		switch {
		case esc:
			esc = false
		case inStr && r == '\\':
			esc = true
		case r == '"':
			inStr = !inStr
		case inStr: // braces inside a line of narration are just text
		case r == '{':
			if depth == 0 {
				from = k
			}
			depth++
		case r == '}':
			if depth--; depth == 0 {
				done++
				if e, ok := jsonEnd(tail[from : k+1]); ok && e > through {
					through = e
				}
			}
		}
	}
	return done, through
}

// jsonEnd reads the "end" out of one finished object. A shape without one -- a
// narration clip, say -- simply has no answer here, and its caller falls back
// to counting.
func jsonEnd(obj string) (float64, bool) {
	var o struct {
		End *float64 `json:"end"`
	}
	if json.Unmarshal([]byte(obj), &o) != nil || o.End == nil {
		return 0, false
	}
	return *o.End, true
}

// llmChatRetry absorbs one transport hiccup, which a multi-hour pass will hit
// -- but never retries a user stop.
// ---- what a rejected answer does to the conversation -------------------------

// noAnswer names the failure a JSON parser cannot: a reply with no answer in
// it at all -- the thinking budget spent and not a word written (reasoning is
// kept out of the reply, readChatStream), which json would report as
// "unexpected end of JSON input".
func noAnswer(reply string) string {
	if _, answer := splitThink(reply); strings.TrimSpace(answer) != "" {
		return ""
	}
	return "you returned no answer at all -- the whole reply was reasoning. " +
		"Think briefly, then write the complete JSON: every segment and every effect, not a sample"
}

// thinkAgain: whether the NEXT attempt may still think. An EMPTY answer means
// the thinking budget was spent with no words -- the same question asked the
// same way answers the same -- so the retry sends enable_thinking false with
// the shorter ceiling (llmChatPost). Bad JSON or the wrong shape keeps
// thinking: that model is writing and getting it wrong.
func thinkAgain(think bool, reply string) bool { return think && noAnswer(reply) == "" }

// cutOff names the other failure a JSON parser cannot: an answer that stopped
// at the token ceiling. json says "unexpected end of JSON input" either way,
// and the two want opposite fixes -- a malformed answer wants care, a
// truncated one wants a SHORTER answer.
func cutOff(reply string, err error) string {
	if strings.TrimSpace(reply) == "" || err == nil {
		return ""
	}
	if !errors.Is(err, io.ErrUnexpectedEOF) &&
		!strings.Contains(err.Error(), "unexpected end of JSON input") {
		return ""
	}
	return "your answer stopped in the middle -- it was too long to finish. " +
		"Answer again with far fewer items"
}

// retryTurn puts a rejected answer and its correction into the history.
//
// An empty answer is not echoed back. There is nothing to show the model, and
// an empty assistant turn is one the next round has to make sense of -- some
// servers refuse a conversation containing one outright, and the rest are
// being told "you said:" followed by nothing. The correction carries the whole
// message in that case, which is what noAnswer is for.
func retryTurn(msgs []map[string]any, reply, problem string) []map[string]any {
	if strings.TrimSpace(reply) != "" {
		msgs = append(msgs, msg("assistant", reply))
	}
	return append(msgs, msg("user",
		"Your answer failed validation: "+problem+". Return corrected strict JSON only."))
}

// jsonReply reads a model's answer into out: the JSON object after any
// preamble, code fences stripped. The problem string is the correction to send
// back, "" when it parsed.
func jsonReply(reply string, out any) string {
	problem := noAnswer(reply)
	if problem != "" {
		return problem
	}
	clean := strings.TrimSpace(reply)
	if i := strings.Index(clean, "{"); i >= 0 {
		clean = clean[i:]
	}
	clean = strings.TrimSuffix(strings.TrimSpace(clean), "```")
	if err := json.Unmarshal([]byte(clean), out); err != nil {
		if problem = cutOff(reply, err); problem == "" {
			problem = "not valid JSON: " + err.Error()
		}
	}
	return problem
}

func (a *App) llmChatRetry(step string, msgs []map[string]any, thinking bool) (string, error) {
	return a.llmChatRetryOn(step, msgs, thinking, nil)
}

func (a *App) llmChatRetryOn(step string, msgs []map[string]any, thinking bool, onText func(string)) (string, error) {
	return a.llmChatRetryTools(step, msgs, thinking, nil, nil, onText)
}

// llmChatRetryTools is the retrying call with tools on the table; with none
// it is llmChatRetryOn exactly.
func (a *App) llmChatRetryTools(step string, msgs []map[string]any, thinking bool,
	tools []map[string]any, run toolRunner, onText func(string)) (string, error) {
	reply, err := a.llmChatTools(step, msgs, thinking, tools, run, onText)
	if err == nil || errors.Is(err, errStopped) {
		return reply, err
	}
	time.Sleep(2 * time.Second)
	return a.llmChatTools(step, msgs, thinking, tools, run, onText)
}
