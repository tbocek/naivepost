package main

// Two tools, web_search and web_read, offered to the jobs that write words the
// viewer reads (captions, narration, upload text) through the tool-calling
// loop in llm.go -- so a detail can be looked up rather than inferred.
//
// The search is headless Firefox over WebDriver BiDi (WebSocket on
// --remote-debugging-port: navigate, evaluate, close), reading DuckDuckGo's
// result anchors off the DOM; the HTML endpoint captchas within a dozen
// queries. A search is THREE queries, broad to narrow, answered with the
// narrowest that finds anything (searchLadder). Firefox comes from
// appConf.Firefox: empty = PATH, "off" = no tools. Registered in curCmds, so ⏹
// kills it.

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net"
	"net/url"
	"os"
	"os/exec"
	"strings"
	"sync"
	"sync/atomic"
	"time"

	"github.com/coder/websocket"
)

const (
	searchHits      = 8                // results handed back per query
	searchPageChars = 6000             // of a page's text; enough for a fact, not an article
	searchRender    = 15 * time.Second // how long a result page may take to draw its anchors
	searchTimeout   = 45 * time.Second // the whole of one tool call
	searchBoot      = 15 * time.Second // firefox's port coming up
)

// firefoxOff is what the settings box says when the tools are not wanted.
const firefoxOff = "off"

// firefoxBin resolves the settings box to a binary: the path it names, or the
// firefox on PATH, or the usual places. "" with an error means there is none,
// and the tools are simply not offered -- a model that cannot search writes
// only what the material shows, which is the rule anyway.
func firefoxBin(conf string) (string, error) {
	conf = strings.TrimSpace(conf)
	if strings.EqualFold(conf, firefoxOff) {
		return "", errors.New("web search is off")
	}
	if conf != "" {
		if _, err := os.Stat(conf); err != nil {
			return "", fmt.Errorf("firefox: %w", err)
		}
		return conf, nil
	}
	for _, n := range []string{"firefox", "firefox-esr", "firefox-bin"} {
		if p, err := exec.LookPath(n); err == nil {
			return p, nil
		}
	}
	for _, p := range []string{"/usr/bin/firefox", "/usr/bin/firefox-esr", "/snap/bin/firefox",
		"/Applications/Firefox.app/Contents/MacOS/firefox"} {
		if _, err := os.Stat(p); err == nil {
			return p, nil
		}
	}
	return "", errors.New("no firefox found -- name one in the settings, or set it to off")
}

// webHit is one search result as the model reads it.
type webHit struct {
	Title, URL, Snippet string
}

// ---- the browser ------------------------------------------------------------

// browser is one headless Firefox and the WebSocket it is driven over. One per
// tool call, closed when the call is answered: a browser kept warm between
// calls is a process to leak, and a search is seconds anyway.
type browser struct {
	cmd     *exec.Cmd
	conn    *websocket.Conn
	profile string
	tab     string // the one browsing context, from browsingContext.getTree

	mu      sync.Mutex
	pending map[int64]chan json.RawMessage
	next    atomic.Int64
	done    chan struct{}
}

// ports are handed out in turn so two calls in flight -- the narration and a
// retry of the cut, say -- do not fight over one debugging port.
var browserPort atomic.Int64

func init() { browserPort.Store(9222) }

// startBrowser launches firefox on a fresh profile, waits for its debugging
// port, opens the BiDi session and finds the tab. The first page is navigated
// to afterwards rather than passed on the command line, because a URL on the
// command line gives no signal for when it has loaded.
func (a *App) startBrowser(ctx context.Context, bin string) (*browser, error) {
	profile, err := os.MkdirTemp("", "naivepost-firefox-*")
	if err != nil {
		return nil, err
	}
	port := browserPort.Add(1)
	cmd := exec.Command(bin, "-headless", "--private-window", "--no-remote", "--new-instance",
		"--profile", profile, fmt.Sprintf("--remote-debugging-port=%d", port), "about:blank")
	cmd.Env = append(os.Environ(), "MOZ_HEADLESS=1")
	cmd.Stdout, cmd.Stderr = io.Discard, io.Discard
	b := &browser{cmd: cmd, profile: profile, pending: map[int64]chan json.RawMessage{}, done: make(chan struct{})}
	// on the roll ⏹ kills, like ffmpeg (runCmd)
	a.ctlMu.Lock()
	a.curCmds[cmd] = true
	a.ctlMu.Unlock()
	if err := cmd.Start(); err != nil {
		b.close(a)
		return nil, fmt.Errorf("start firefox: %w", err)
	}
	addr := fmt.Sprintf("127.0.0.1:%d", port)
	up := false
	for t0 := time.Now(); time.Since(t0) < searchBoot; {
		if c, err := net.DialTimeout("tcp", addr, 500*time.Millisecond); err == nil {
			c.Close()
			up = true
			break
		}
		select {
		case <-ctx.Done():
			b.close(a)
			return nil, ctx.Err()
		case <-time.After(300 * time.Millisecond):
		}
	}
	if !up {
		b.close(a)
		return nil, fmt.Errorf("firefox did not open port %d within %s", port, searchBoot)
	}
	conn, _, err := websocket.Dial(ctx, "ws://"+addr+"/session", nil)
	if err != nil {
		b.close(a)
		return nil, fmt.Errorf("firefox bidi: %w", err)
	}
	conn.SetReadLimit(16 << 20)
	b.conn = conn
	go b.readLoop()
	if _, err := b.send(ctx, "session.new", map[string]any{"capabilities": map[string]any{}}); err != nil {
		b.close(a)
		return nil, err
	}
	tree, err := b.send(ctx, "browsingContext.getTree", map[string]any{})
	if err != nil {
		b.close(a)
		return nil, err
	}
	var got struct {
		Contexts []struct {
			Context string `json:"context"`
		} `json:"contexts"`
	}
	if json.Unmarshal(tree, &got) != nil || len(got.Contexts) == 0 {
		b.close(a)
		return nil, errors.New("firefox bidi: no browsing context")
	}
	b.tab = got.Contexts[0].Context
	return b, nil
}

// readLoop hands every answer to the call waiting for it. Events -- messages
// with no id -- are nobody's and are dropped.
func (b *browser) readLoop() {
	defer close(b.done)
	for {
		_, data, err := b.conn.Read(context.Background())
		if err != nil {
			b.mu.Lock()
			for id, ch := range b.pending {
				close(ch)
				delete(b.pending, id)
			}
			b.mu.Unlock()
			return
		}
		var r struct {
			ID     int64           `json:"id"`
			Type   string          `json:"type"`
			Result json.RawMessage `json:"result"`
			Error  string          `json:"error"`
			Msg    string          `json:"message"`
		}
		if json.Unmarshal(data, &r) != nil || r.ID == 0 {
			continue
		}
		b.mu.Lock()
		ch := b.pending[r.ID]
		delete(b.pending, r.ID)
		b.mu.Unlock()
		if ch == nil {
			continue
		}
		if r.Type == "error" {
			e, _ := json.Marshal(map[string]string{"bidiError": r.Error + ": " + r.Msg})
			ch <- e
		} else {
			ch <- r.Result
		}
	}
}

// send is one BiDi command and its answer.
func (b *browser) send(ctx context.Context, method string, params any) (json.RawMessage, error) {
	id := b.next.Add(1)
	ch := make(chan json.RawMessage, 1)
	b.mu.Lock()
	b.pending[id] = ch
	b.mu.Unlock()
	req, _ := json.Marshal(map[string]any{"id": id, "method": method, "params": params})
	if err := b.conn.Write(ctx, websocket.MessageText, req); err != nil {
		return nil, err
	}
	select {
	case <-ctx.Done():
		return nil, ctx.Err()
	case res, ok := <-ch:
		if !ok {
			return nil, errors.New("firefox closed the connection")
		}
		var e struct {
			Err string `json:"bidiError"`
		}
		if json.Unmarshal(res, &e) == nil && e.Err != "" {
			return nil, fmt.Errorf("%s: %s", method, e.Err)
		}
		return res, nil
	}
}

// navigate loads a page and waits for it to finish -- or for the deadline,
// after which whatever has arrived is what gets read.
func (b *browser) navigate(ctx context.Context, u string) error {
	_, err := b.send(ctx, "browsingContext.navigate",
		map[string]any{"context": b.tab, "url": u, "wait": "complete"})
	return err
}

// eval runs a script in the page and returns its string result.
func (b *browser) eval(ctx context.Context, js string) (string, error) {
	res, err := b.send(ctx, "script.evaluate", map[string]any{
		"expression": js, "target": map[string]any{"context": b.tab}, "awaitPromise": false})
	if err != nil {
		return "", err
	}
	var out struct {
		Result struct {
			Value string `json:"value"`
		} `json:"result"`
	}
	if err := json.Unmarshal(res, &out); err != nil {
		return "", err
	}
	return out.Result.Value, nil
}

// close ends the browser and forgets the profile. Kill rather than a polite
// quit: it is a private window on a throwaway profile, and nothing in it is
// worth the seconds a shutdown takes.
func (b *browser) close(a *App) {
	if b.conn != nil {
		b.conn.CloseNow()
	}
	if b.cmd.Process != nil {
		b.cmd.Process.Kill()
		b.cmd.Wait()
	}
	a.ctlMu.Lock()
	delete(a.curCmds, b.cmd)
	a.ctlMu.Unlock()
	os.RemoveAll(b.profile)
}

// ---- the two things it does -------------------------------------------------

// ddgExtract reads the result anchors off a rendered DuckDuckGo page. The
// page repeats an anchor in its carousels, so the reader dedupes by URL.
const ddgExtract = `JSON.stringify(Array.from(document.querySelectorAll('a[data-testid="result-title-a"]')).map(a => {
  const root = a.closest('article') || a.closest('[data-testid="result"]') || a.parentElement;
  const snip = root && (root.querySelector('[data-result="snippet"]') || root.querySelector('span[data-testid="result-snippet"]') || root.querySelector('.result__snippet'));
  return {title: (a.innerText || "").trim(), url: a.href, snippet: snip ? (snip.innerText || "").trim() : ""};
}).filter(r => r.url.startsWith('http')))`

// webSearch is one query on a browser of its own: the settings dialog's
// Test. A job's searches share one (runWebTool), since a ladder is up to
// three and a browser takes seconds to come up.
func (a *App) webSearch(ctx context.Context, bin, query string) ([]webHit, error) {
	ctx, cancel := context.WithTimeout(ctx, searchTimeout)
	defer cancel()
	b, err := a.startBrowser(ctx, bin)
	if err != nil {
		return nil, err
	}
	defer b.close(a)
	return b.search(ctx, query)
}

// search is one query on this browser: the result page, the anchors off it.
func (b *browser) search(ctx context.Context, query string) ([]webHit, error) {
	nav, cancelNav := context.WithTimeout(ctx, 10*time.Second)
	b.navigate(nav, "https://duckduckgo.com/?q="+url.QueryEscape(query)) // a slow page is read as it stands
	cancelNav()
	var hits []webHit
	for t0 := time.Now(); time.Since(t0) < searchRender; {
		raw, err := b.eval(ctx, ddgExtract)
		if err != nil {
			return nil, err
		}
		var got []struct{ Title, URL, Snippet string }
		if json.Unmarshal([]byte(raw), &got) == nil && len(got) > 0 {
			seen := map[string]bool{}
			for _, g := range got {
				if seen[g.URL] {
					continue
				}
				seen[g.URL] = true
				hits = append(hits, webHit{Title: g.Title, URL: g.URL, Snippet: g.Snippet})
				if len(hits) == searchHits {
					break
				}
			}
			break
		}
		select {
		case <-ctx.Done():
			return nil, ctx.Err()
		case <-time.After(500 * time.Millisecond):
		}
	}
	return hits, nil
}

// webRead is one page's text, as a reader would see it, cut to what a fact
// needs. A wall -- a captcha, a "verify you are human" -- is reported as one
// rather than handed to the model as the page.
func (a *App) webRead(ctx context.Context, bin, u string) (string, error) {
	ctx, cancel := context.WithTimeout(ctx, searchTimeout)
	defer cancel()
	b, err := a.startBrowser(ctx, bin)
	if err != nil {
		return "", err
	}
	defer b.close(a)
	nav, cancelNav := context.WithTimeout(ctx, 15*time.Second)
	b.navigate(nav, u)
	cancelNav()
	text, err := b.eval(ctx, "document.body ? document.body.innerText : ''")
	if err != nil {
		return "", err
	}
	if issue := pageIssue(text); issue != "" {
		return "", errors.New(issue)
	}
	return clipRunes(strings.TrimSpace(text), searchPageChars), nil
}

// botWalls is what a page says instead of its content when it would rather
// not be read by a program.
var botWalls = []string{"just a moment", "checking your browser", "verify you are human",
	"verifying you are human", "are you human", "press and hold", "please enable javascript and cookies",
	"attention required", "access denied", "403 forbidden", "too many requests"}

func pageIssue(text string) string {
	t := strings.ToLower(strings.TrimSpace(text))
	if t == "" {
		return "the page had no text"
	}
	for _, w := range botWalls {
		if strings.Contains(t, w) {
			return "the page is behind a bot wall (" + w + ")"
		}
	}
	if len(t) < 200 {
		return "the page had almost no text (blocked, or not loaded)"
	}
	return ""
}

func clipRunes(s string, n int) string {
	if len(s) <= n {
		return s
	}
	c := s[:n]
	for len(c) > 0 && !isRuneStart(c[len(c)-1]) {
		c = c[:len(c)-1]
	}
	return c + " …"
}

func isRuneStart(b byte) bool { return b&0xC0 != 0x80 }

// ---- the ladder -------------------------------------------------------------

// searcher is webSearch with the browser chosen, so the ladder can be tested
// against a table rather than a Firefox.
type searcher func(ctx context.Context, query string) ([]webHit, error)

// searchLadder runs the three queries from the broadest and keeps climbing
// while the web still answers: the narrowest query with results is the one
// whose results are about the thing asked for. A narrower query that finds
// nothing is a name the web does not know in that spelling, and the step
// below it is the answer. Nothing at any step is reported as nothing.
func searchLadder(ctx context.Context, search searcher, broad, medium, narrow string) (string, []webHit, error) {
	var bestQ string
	var best []webHit
	var lastErr error
	for _, q := range []string{broad, medium, narrow} {
		q = strings.TrimSpace(q)
		if q == "" {
			continue
		}
		hits, err := search(ctx, q)
		if err != nil {
			lastErr = err
			if ctx.Err() != nil {
				return bestQ, best, err
			}
			continue
		}
		if len(hits) == 0 {
			if best != nil {
				break // the web stopped answering: the step below is the answer
			}
			continue
		}
		bestQ, best = q, hits
	}
	if best == nil && lastErr != nil {
		return "", nil, lastErr
	}
	return bestQ, best, nil
}

// formatHits is what the model reads back: numbered, the URL on its own line
// so a web_read can quote it exactly.
func formatHits(query string, hits []webHit) string {
	if len(hits) == 0 {
		return "No results for any of the three queries."
	}
	var b strings.Builder
	fmt.Fprintf(&b, "Results for %q:\n", query)
	for i, h := range hits {
		fmt.Fprintf(&b, "%d. %s\n   %s\n", i+1, h.Title, h.URL)
		if h.Snippet != "" {
			fmt.Fprintf(&b, "   %s\n", h.Snippet)
		}
	}
	return b.String()
}

// ---- as the model sees it ---------------------------------------------------

// webTools is the two tools in the shape the chat API takes. The descriptions
// are the instructions: when to reach for one, and that the three queries are
// one call.
func webTools() []map[string]any {
	return []map[string]any{
		{"type": "function", "function": map[string]any{
			"name":        "web_search",
			"description": "Look up a fact you are about to write into the video and would otherwise guess: what a named thing is, does or costs, a name's spelling, a number. Only for something the material does not contain, and only when the user context asks for a detail you do not have -- not to understand the session, not to check what you have already been told. Give three queries at once, from broad to narrow -- the game, the game and the thing, the thing's exact name -- and you get the narrowest one that found anything.",
			"parameters": map[string]any{"type": "object", "properties": map[string]any{
				"broad":  map[string]any{"type": "string", "description": "the general subject, e.g. the game"},
				"medium": map[string]any{"type": "string", "description": "the subject and the thing"},
				"narrow": map[string]any{"type": "string", "description": "the thing's exact name, as the user context spells it"},
			}, "required": []string{"broad", "medium", "narrow"}}}},
		{"type": "function", "function": map[string]any{
			"name":        "web_read",
			"description": "Read one page from a web_search result, by its URL, when the snippet did not say enough. Returns the page's text, shortened.",
			"parameters": map[string]any{"type": "object", "properties": map[string]any{
				"url": map[string]any{"type": "string"}}, "required": []string{"url"}}}},
	}
}

// webToolsFor is what a job is offered: the two tools when a firefox is at
// hand and the settings do not say off, nothing otherwise -- and the reason,
// once, in the log, so a run with no searches in it says why.
func (a *App) webToolsFor(step string) ([]map[string]any, string) {
	// a sandbox has no host browser to launch; the model writes only what the
	// material shows, which is the rule anyway
	if inFlatpak() {
		a.logfIdle(">>> %s: no web search inside Flatpak", step)
		return nil, ""
	}
	bin, err := firefoxBin(a.readConf().Firefox)
	if err != nil {
		a.logfIdle(">>> %s: no web search (%v)", step, err)
		return nil, ""
	}
	return webTools(), bin
}

// webRunner is the tool runner a job hands the chat loop: one browser per
// call, on the run's own context so ⏹ ends a search with the run.
func (a *App) webRunner(step, bin string) toolRunner {
	return func(name string, args json.RawMessage) string {
		ctx := a.runCtx
		if ctx == nil {
			ctx = context.Background()
		}
		return a.runWebTool(ctx, bin, step, name, args)
	}
}

// runWebTool answers one call. Errors go back to the model as text: a tool
// that failed is something it can work around by writing less, and an error
// that ended the whole job would be a caption's worth of fact costing a cut.
func (a *App) runWebTool(ctx context.Context, bin, step, name string, args json.RawMessage) string {
	switch name {
	case "web_search":
		var q struct{ Broad, Medium, Narrow string }
		if err := json.Unmarshal(args, &q); err != nil {
			return "web_search: could not read the arguments: " + err.Error()
		}
		ctx, cancel := context.WithTimeout(ctx, searchTimeout)
		defer cancel()
		b, err := a.startBrowser(ctx, bin)
		if err != nil {
			a.logfIdle("!!! %s: web search failed: %v", step, err)
			return "web_search failed: " + err.Error()
		}
		defer b.close(a)
		used, hits, err := searchLadder(ctx, b.search, q.Broad, q.Medium, q.Narrow)
		if err != nil {
			a.logfIdle("!!! %s: web search failed: %v", step, err)
			return "web_search failed: " + err.Error()
		}
		a.logfIdle(">>> %s: searched %q / %q / %q -- %d result(s) for %q", step, q.Broad, q.Medium, q.Narrow, len(hits), used)
		return formatHits(used, hits)
	case "web_read":
		var p struct{ URL string }
		if err := json.Unmarshal(args, &p); err != nil || strings.TrimSpace(p.URL) == "" {
			return "web_read: no url given"
		}
		text, err := a.webRead(ctx, bin, p.URL)
		if err != nil {
			a.logfIdle("!!! %s: could not read %s: %v", step, p.URL, err)
			return "web_read failed: " + err.Error()
		}
		a.logfIdle(">>> %s: read %s (%d characters)", step, p.URL, len(text))
		return text
	}
	return "unknown tool " + name
}
