package main

// What the app says about itself, and where it says it. Three places report,
// and each owns one kind of news: the progress bar is the run, the status line
// is the answer to a click, the log is the record. They sit within an inch of
// each other on the bottom row, so anything said in two of them reads as two
// things happening rather than one -- and a log that explains itself is a log
// whose real news scrolls past unread.
//
// These are guards against drift rather than tests of behaviour: the rules are
// about wording, and wording is what a later change quietly widens.

import (
	"go/ast"
	"go/parser"
	"go/printer"
	"go/token"
	"path/filepath"
	"strconv"
	"strings"
	"testing"
)

// srcFiles is the app's own source: every .go file that is not a test.
func srcFiles(t *testing.T) []string {
	t.Helper()
	all, err := filepath.Glob("*.go")
	if err != nil {
		t.Fatal(err)
	}
	var out []string
	for _, f := range all {
		if !strings.HasSuffix(f, "_test.go") {
			out = append(out, f)
		}
	}
	if len(out) < 20 {
		t.Fatalf("%d source files found — the glob is wrong, not the app", len(out))
	}
	return out
}

// callArgs renders the first argument of every call to any of the named
// methods, by file and line. Rendered rather than matched as text because
// "the same words in two places" is the thing being looked for, and a string
// that was wrapped across two lines in one place and not the other is still
// the same string.
func callArgs(t *testing.T, file string, names ...string) map[string]int {
	t.Helper()
	fset := token.NewFileSet()
	f, err := parser.ParseFile(fset, file, nil, 0)
	if err != nil {
		t.Fatalf("%s: %v", file, err)
	}
	want := map[string]bool{}
	for _, n := range names {
		want[n] = true
	}
	out := map[string]int{}
	ast.Inspect(f, func(n ast.Node) bool {
		c, ok := n.(*ast.CallExpr)
		if !ok || len(c.Args) == 0 {
			return true
		}
		sel, ok := c.Fun.(*ast.SelectorExpr)
		if !ok || !want[sel.Sel.Name] {
			return true
		}
		var b strings.Builder
		if printer.Fprint(&b, fset, c.Args[0]) == nil {
			out[b.String()] = fset.Position(c.Args[0].Pos()).Line
		}
		return true
	})
	return out
}

// logFormats is the literal half of every log line: the format string, with the
// %-verbs left in and the arguments left out. A format built from a variable
// has no literal to weigh and is skipped.
func logFormats(t *testing.T, file string) map[string]int {
	t.Helper()
	fset := token.NewFileSet()
	f, err := parser.ParseFile(fset, file, nil, 0)
	if err != nil {
		t.Fatalf("%s: %v", file, err)
	}
	var lit func(ast.Expr) (string, bool)
	lit = func(e ast.Expr) (string, bool) {
		switch v := e.(type) {
		case *ast.BasicLit:
			if v.Kind != token.STRING {
				return "", false
			}
			s, err := strconv.Unquote(v.Value)
			return s, err == nil
		case *ast.BinaryExpr:
			if v.Op != token.ADD {
				return "", false
			}
			l, ok1 := lit(v.X)
			r, ok2 := lit(v.Y)
			return l + r, ok1 && ok2
		}
		return "", false
	}
	out := map[string]int{}
	ast.Inspect(f, func(n ast.Node) bool {
		c, ok := n.(*ast.CallExpr)
		if !ok || len(c.Args) == 0 {
			return true
		}
		sel, ok := c.Fun.(*ast.SelectorExpr)
		if !ok || (sel.Sel.Name != "logf" && sel.Sel.Name != "logfIdle") {
			return true
		}
		if s, ok := lit(c.Args[0]); ok {
			out[s] = fset.Position(c.Pos()).Line
		}
		return true
	})
	return out
}

// logLineMax is how long a log line's own words may be. A line reports -- a
// name, a count, a size, a failure -- and reporting fits in one clause and a
// number. Past this it has started explaining what a thing is or what to do
// about it, and neither is news: the page, the tooltip and the source are
// where those live. The longest real line in the app is a little over a
// hundred characters, so this is a ceiling with room in it rather than a
// target to write up to.
const logLineMax = 120

func TestALogLineReportsRatherThanExplains(t *testing.T) {
	n := 0
	for _, file := range srcFiles(t) {
		for f, line := range logFormats(t, file) {
			n++
			if len(f) > logLineMax {
				t.Errorf("%s:%d is %d characters of log line:\n\t%s",
					file, line, len(f), f)
			}
			// two sentences is two pieces of news, and the second one is
			// almost always the advice rather than the fact
			for i := 1; i+1 < len(f); i++ {
				if strings.ContainsRune(".!?", rune(f[i])) && f[i+1] == ' ' &&
					f[i+2] >= 'A' && f[i+2] <= 'Z' {
					t.Errorf("%s:%d says two sentences where the log gets one:\n\t%s",
						file, line, f)
					break
				}
			}
		}
	}
	if n < 100 {
		t.Fatalf("only %d log lines found — the parser missed them, and this test passed on nothing", n)
	}
}

// A run has the progress bar. Everything a run had to say was being said twice
// -- the identical string on the bar and on the status line at the same instant
// -- which is why the status line read as furniture: the one message it held
// after a run was the one already on the bar beside it.
func TestARunReportsOnTheBarAndNotAlsoOnTheStatusLine(t *testing.T) {
	for _, file := range srcFiles(t) {
		bar := callArgs(t, file, "SetText")
		status := callArgs(t, file, "setStatus")
		for arg, line := range status {
			if barLine, dup := bar[arg]; dup {
				t.Errorf("%s: the status line at %d and the bar at %d say the same thing:\n\t%s",
					file, line, barLine, arg)
			}
		}
	}
}

// Nor does it repeat the header bar. The project it would name is the project
// the header names, with the full path already in that label's tooltip -- and
// because loading is the last thing a launch does, that message was what the
// line held for the whole session unless something else was clicked.
func TestTheStatusLineDoesNotNameTheProjectTheHeaderNames(t *testing.T) {
	for arg, line := range callArgs(t, "project.go", "setStatus") {
		if strings.Contains(arg, "projPath") || strings.Contains(arg, "+ path") {
			t.Errorf("project.go:%d puts the project's path on the status line: %s", line, arg)
		}
	}
	// the header is where it is instead, name shown and path on hover
	src := readSrc(t, "project.go")
	for _, pin := range []string{"a.projLabel.SetText(name)", "a.projLabel.SetTooltipText(tip)"} {
		if !strings.Contains(src, pin) {
			t.Errorf("the header bar lost its pin %q, and nothing names the project now", pin)
		}
	}
}

// The counter belongs to the bar. Transcribing an hour of audio walks sixty
// windows twice over, and a line per window is a hundred and thirty lines of
// nothing but a number -- with the run's actual news buried in the middle of
// them. prog takes a format for exactly this reason.
func TestThePerWindowCounterIsOnTheBarAndNotInTheLog(t *testing.T) {
	src := readSrc(t, "pipeline.go")
	for _, pin := range []string{
		`"recognising speech %d/%d", i+1, n)`,
		`"finding voices %d/%d", i+1, nwin)`,
		`"placing speakers %d/%d", i+1, nwin)`,
	} {
		if !strings.Contains(src, pin) {
			t.Errorf("pipeline.go lost its pin %q — the bar no longer counts the windows", pin)
		}
	}
	for f, line := range logFormats(t, "pipeline.go") {
		if strings.Contains(f, "window %d/%d") || strings.Contains(f, "chunk %d/%d") {
			t.Errorf("pipeline.go:%d logs a line per window again:\n\t%s", line, f)
		}
	}
}

// The one thing that must survive all of this: a failure still says so, in the
// log, with the error in it. Terseness is not the same as silence.
func TestAFailureStillReachesTheLog(t *testing.T) {
	for _, file := range []string{"prep.go", "produce.go", "publish.go", "narrate.go", "cut_suggest.go"} {
		found := false
		for f := range logFormats(t, file) {
			if strings.Contains(f, "FAILED: %v") {
				found = true
			}
		}
		if !found {
			t.Errorf("%s no longer logs its failure with the error", file)
		}
	}
	// and the loud ones are still loud. !!! is the log's only shout, kept for
	// the lines that describe work quietly not happening -- a source the
	// session will now run without, an input silently overwriting another,
	// outputs left behind in the folder a rename could not move. Those read
	// as ordinary progress without it, which is exactly when they are missed.
	shouts := map[string][]string{
		"project.go": {"is not there any more", "could not move the output folder"},
		"prep.go":    {"rename one"},
	}
	for file, wants := range shouts {
		got := logFormats(t, file)
		for _, want := range wants {
			found := false
			for f := range got {
				if strings.Contains(f, want) && strings.HasPrefix(f, "!!! ") {
					found = true
				}
			}
			if !found {
				t.Errorf("%s: the line about %q is no longer marked !!!", file, want)
			}
		}
	}
}
