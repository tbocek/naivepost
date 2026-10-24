package main

// Adding a source: copied into <project>/sources/ by default, so the project
// folder stays one thing; a tick beside the Add buttons switches to
// referencing in place (Project.RefSources) for footage that should not exist
// twice.

import (
	"fmt"
	"io"
	"os"
	"path/filepath"
	"strings"

	"github.com/diamondburned/gotk4/pkg/glib/v2"
)

// sourcesDir is where copied sources land.
func (a *App) sourcesDir() string { return filepath.Join(a.outDir, "sources") }

// inProject is whether a file is already inside the project folder -- one
// already there is added as it is, whatever the answer, because copying a file
// onto itself is not a thing to offer.
func (a *App) inProject(path string) bool {
	if a.outDir == "" {
		return false
	}
	rel, err := filepath.Rel(a.outDir, path)
	return err == nil && !strings.HasPrefix(rel, "..")
}

// applyRefSources puts a project's answer on the tick.
func (a *App) applyRefSources(ref bool) {
	a.refSources = ref
	if a.copyTick == nil {
		return
	}
	a.refQuiet = true
	a.copyTick.SetActive(!ref)
	a.refQuiet = false
}

// askImport is what every Add does with the files it was handed: copies them
// into the project, or references them where they are, as the tick beside the
// Add buttons says. It was a question asked on every Add; it is a setting,
// because the answer is a fact about the project and not about the file.
func (a *App) askImport(paths []string) {
	if a.refSources {
		a.addSources(paths...)
		return
	}
	a.copySources(paths)
}

// copySources copies what is outside the project into <project>/sources/ and
// adds the copies, on a goroutine. A file already inside the project is added
// where it is; a name already in sources/ with the same size is the same file
// and is not copied again.
func (a *App) copySources(paths []string) {
	if a.busy() {
		return
	}
	dir := a.sourcesDir()
	if err := os.MkdirAll(dir, 0o755); err != nil {
		a.logf("!!! copy sources: %v", err)
		a.setStatus("could not make the project's sources folder — see log")
		return
	}
	// files already inside the project are added as they are, whatever the
	// tick says: copying a file onto itself is not a thing to offer
	var todo []string
	total := int64(0)
	for _, p := range paths {
		if a.inProject(p) {
			continue
		}
		todo = append(todo, p)
		if fi, err := os.Stat(p); err == nil {
			total += fi.Size()
		}
	}
	if len(todo) == 0 {
		a.addSources(paths...)
		return
	}
	a.startRun()
	a.logf(">>> copying %s (%s) into %s", plural(len(todo), "source"), humanSize(total), dir)
	go func() {
		var out []string
		done := int64(0)
		for _, p := range paths {
			if a.inProject(p) {
				out = append(out, p)
				continue
			}
			// the bar is the bytes, not the files: one 18 GB capture is one
			// file, and a bar that sat at 0 for ten minutes is a bar that
			// says the copy is stuck
			name := filepath.Base(p)
			meter := &copyMeter{total: total, done: done, tick: func(f float64) {
				a.progIdle(f, "copying %s", name)
			}}
			to, err := copyInto(dir, p, meter)
			if err != nil {
				a.logfIdle("!!! copying %s: %v", name, err)
				continue
			}
			if fi, err := os.Stat(to); err == nil {
				done += fi.Size()
			}
			out = append(out, to)
		}
		glib.IdleAdd(func() {
			a.endRun()
			a.progress.SetFraction(0)
			a.addSources(out...)
			a.saveProjectNow()
		})
	}()
}

// progIdle moves the run bar from a worker goroutine.
func (a *App) progIdle(f float64, format string, args ...any) {
	s := fmt.Sprintf(format, args...)
	glib.IdleAdd(func() {
		a.progress.SetFraction(f)
		a.setStatus(s)
	})
}

// copyFile writes src to the exact path out, through the same .part-and-rename
// copyInto uses: a copy interrupted by a pulled drive or a full disk must not
// look like a finished file.
func copyFile(src, out string) error {
	in, err := os.Open(src)
	if err != nil {
		return err
	}
	defer in.Close()
	part := out + ".part"
	f, err := os.Create(part)
	if err != nil {
		return err
	}
	if _, err := io.Copy(f, in); err != nil {
		f.Close()
		os.Remove(part)
		return err
	}
	if err := f.Close(); err != nil {
		os.Remove(part)
		return err
	}
	return os.Rename(part, out)
}

// copyMeter counts the bytes of one copy into a bar over all of them, and tells
// the bar only when the needle would visibly move: a Write per 32 kB block is
// hundreds of thousands of GUI round trips on a large file.
type copyMeter struct {
	total, done int64
	last        float64
	tick        func(float64)
}

func (m *copyMeter) Write(b []byte) (int, error) {
	m.done += int64(len(b))
	if m.total > 0 {
		if f := float64(m.done) / float64(m.total); f-m.last >= 0.005 {
			m.last = f
			m.tick(f)
		}
	}
	return len(b), nil
}

// copyInto copies one file into dir and answers with its new path. The copy is
// written to a .part and renamed, so an interrupted copy cannot be mistaken
// for a source: a half file that plays for ten seconds and stops is the worst
// way to find out a drive was pulled. meter, when given, sees every byte.
func copyInto(dir, src string, meter io.Writer) (string, error) {
	fi, err := os.Stat(src)
	if err != nil {
		return "", err
	}
	dst := filepath.Join(dir, filepath.Base(src))
	if d, err := os.Stat(dst); err == nil && d.Size() == fi.Size() {
		return dst, nil // already here, same size: the same file
	}
	in, err := os.Open(src)
	if err != nil {
		return "", err
	}
	defer in.Close()
	part := dst + ".part"
	out, err := os.Create(part)
	if err != nil {
		return "", err
	}
	var w io.Writer = out
	if meter != nil {
		w = io.MultiWriter(out, meter)
	}
	if _, err := io.Copy(w, in); err != nil {
		out.Close()
		os.Remove(part)
		return "", err
	}
	if err := out.Close(); err != nil {
		os.Remove(part)
		return "", err
	}
	return dst, os.Rename(part, dst)
}
