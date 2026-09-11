package main

// Whether the finished video is already the finished video. Everything that
// reaches the ffmpeg command line -- settings, cut, narration, recordings --
// goes into one stamp written beside the video after a successful encode; if
// it matches and the file exists, ▶ leaves it alone. ↻ Transcode encodes
// anyway. NOT in the stamp: title, description, thumbnail, upload record.

import (
	"crypto/sha1"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strings"
)

// renderStampFile is where the stamp of the video beside it lives. Beside it
// on purpose: deleting the video, or the folder, throws the answer away with
// the thing it was about.
func (a *App) renderStampFile() string {
	out := a.prod.outFile
	return strings.TrimSuffix(out, filepath.Ext(out)) + ".stamp"
}

// renderStamp is what the video is made of, as one line.
//
// The narration is in it by its WAVS, not by its words: a line re-spoken in
// another voice is the same text and a different video, and the wav's size and
// time say so without reading it. The sources are in it by path, size and
// time for the same reason -- a recording re-encoded in place is not the
// footage the last render used.
func (a *App) renderStamp(segs []cutSeg, entries []narrEntry, st prodSettings, vids, auds []string) string {
	type line struct {
		S, E float64
		Text string
		Wav  string
	}
	in := struct {
		Set     prodSettings
		Segs    []cutSeg
		Lines   []line
		Sources []string
		Aspect  string
		Voice   string
		NarrOff bool
	}{Set: st, Segs: segs, Aspect: a.produceCut().Aspect,
		Voice: a.voiceID(), NarrOff: a.narrOff}
	in.Set.OutFile = "" // where it goes is not what it is
	for _, e := range entries {
		in.Lines = append(in.Lines, line{S: e.S, E: e.E, Text: e.Text, Wav: fileMark(a.ttsWav(e))})
	}
	for _, p := range append(append([]string(nil), vids...), auds...) {
		in.Sources = append(in.Sources, p+" "+fileMark(p))
	}
	b, err := json.Marshal(in)
	if err != nil {
		return "" // an empty stamp never matches, which is the safe way to fail
	}
	sum := sha1.Sum(b)
	return hex.EncodeToString(sum[:])
}

// fileMark is a file's size and modification time, which is what "the same
// file" means here: hashing every recording in the session would cost more
// than the encode this is trying to avoid.
func fileMark(path string) string {
	fi, err := os.Stat(path)
	if err != nil {
		return "-"
	}
	return fmt.Sprintf("%d@%d", fi.Size(), fi.ModTime().UnixNano())
}

// renderStale is whether ▶ has anything to encode: no file, or a file made
// from something other than what is on the page now.
func (a *App) renderStale(segs []cutSeg, entries []narrEntry, st prodSettings, vids, auds []string) bool {
	if a.prod == nil || !exists(st.OutFile) {
		return true
	}
	b, err := os.ReadFile(a.renderStampFile())
	if err != nil {
		return true
	}
	want := a.renderStamp(segs, entries, st, vids, auds)
	return want == "" || strings.TrimSpace(string(b)) != want
}

// markRendered writes the stamp for the video that has just been written.
func (a *App) markRendered(segs []cutSeg, entries []narrEntry, st prodSettings, vids, auds []string) {
	s := a.renderStamp(segs, entries, st, vids, auds)
	if s == "" {
		return
	}
	if err := os.WriteFile(a.renderStampFile(), []byte(s+"\n"), 0o644); err != nil {
		a.logfIdle("    produce: could not write the render stamp (%v) — the next ▶ will encode again", err)
	}
}
