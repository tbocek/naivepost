package main

// The fence around the pipeline files: what they may take from *App.

import (
	"os"
	"path/filepath"
	"reflect"
	"regexp"
	"sort"
	"strings"
	"testing"
)

var (
	appMethodRe = regexp.MustCompile(`(?m)^func \(a \*App\) ([A-Za-z_][A-Za-z0-9_]*)\(`)
	appUseRe    = regexp.MustCompile(`\ba\.([A-Za-z_][A-Za-z0-9_]*)`)
	pageObjects = []string{"ed", "prod", "pub", "narr", "prep", "srcList", "voicePick",
		"stylePick", "stack", "win", "playBtn", "stopBtn", "progress", "status", "ctxView", "langEntry"}
)

// Every use of *App in a pipeline file is one of three things: a method of
// runner, a field named in runnerFields, or a method another pipeline file
// defines. Anything else is the pipeline reaching past the seam -- which is
// how cutByText came to drive the editor's undo stack, and how videoStyleName
// came to read a dropdown from a goroutine. Both were found by looking; this
// finds the next one.
func TestThePipelineTakesOnlyTheSeamFromTheApp(t *testing.T) {
	// what *App has at all, so a stray variable named a in some other function
	// is not mistaken for the application
	src := map[string]string{}
	appMethods := map[string]string{} // name -> file that defines it
	files, _ := filepath.Glob("*.go")
	for _, f := range files {
		if strings.HasSuffix(f, "_test.go") {
			continue
		}
		b, err := os.ReadFile(f)
		if err != nil {
			t.Fatal(err)
		}
		src[f] = string(b)
		for _, m := range appMethodRe.FindAllStringSubmatch(src[f], -1) {
			appMethods[m[1]] = f
		}
	}
	appFields := map[string]bool{}
	body := readSrc(t, "main.go")
	if i := strings.Index(body, "type App struct {"); i >= 0 {
		for _, line := range strings.Split(body[i:strings.Index(body[i:], "\n}")+i], "\n") {
			line = strings.TrimSpace(line)
			if line == "" || strings.HasPrefix(line, "//") {
				continue
			}
			for _, name := range strings.Split(strings.Fields(line)[0], ",") {
				appFields[strings.TrimSpace(name)] = true
			}
		}
	}

	seam := map[string]bool{}
	rt := reflect.TypeOf((*runner)(nil)).Elem()
	for i := 0; i < rt.NumMethod(); i++ {
		seam[rt.Method(i).Name] = true
	}
	fields := map[string]bool{}
	for _, f := range runnerFields {
		fields[f] = true
	}
	inside := map[string]bool{}
	for _, f := range pipelineFiles {
		inside[f] = true
	}

	for _, f := range pipelineFiles {
		s, ok := src[f]
		if !ok {
			t.Errorf("%s is on the pipeline list and does not exist", f)
			continue
		}
		if strings.Contains(s, "github.com/diamondburned/gotk4") {
			t.Errorf("%s imports GTK", f)
		}
		var over []string
		seen := map[string]bool{}
		for _, m := range appUseRe.FindAllStringSubmatch(s, -1) {
			name := m[1]
			if seen[name] {
				continue
			}
			seen[name] = true
			def, isMethod := appMethods[name]
			isField := appFields[name]
			if !isMethod && !isField {
				continue // not the application's: another a
			}
			switch {
			case seam[name], fields[name]:
			case isMethod && inside[def]:
				// the pipeline's own
			default:
				where := "field"
				if isMethod {
					where = "method in " + def
				}
				over = append(over, name+" ("+where+")")
			}
		}
		sort.Strings(over)
		if len(over) > 0 {
			t.Errorf("%s reaches past the seam for: %s\n\tadd it to runner in seam.go if the pipeline really needs it",
				f, strings.Join(over, ", "))
		}
		// and never a page. A page object is the GUI thread's, and reading one
		// from a runner is the race the seam exists to prevent.
		for _, p := range pageObjects {
			if regexp.MustCompile(`\ba\.` + p + `\b`).MatchString(s) {
				t.Errorf("%s touches a page object: a.%s", f, p)
			}
		}
	}
}

// The seam is not decoration: every method on it is used by some pipeline
// file, and every field named is read by one. A member nobody uses is a door
// left open.
func TestTheSeamIsExactlyWhatIsUsed(t *testing.T) {
	var all strings.Builder
	for _, f := range pipelineFiles {
		all.WriteString(readSrc(t, f))
	}
	s := all.String()
	rt := reflect.TypeOf((*runner)(nil)).Elem()
	for i := 0; i < rt.NumMethod(); i++ {
		if !strings.Contains(s, "a."+rt.Method(i).Name+"(") {
			t.Errorf("runner.%s is on the seam and no pipeline file calls it", rt.Method(i).Name)
		}
	}
	for _, f := range runnerFields {
		if !regexp.MustCompile(`\ba\.` + f + `\b`).MatchString(s) {
			t.Errorf("runnerFields names %s and no pipeline file reads it", f)
		}
	}
}
