package main

// Hand-picked seconds of a narrator's recording for the voice reference,
// overruling narrate_ref.go's automatic pick (which cannot hear clipping, fans
// or a reading voice). Drag on the wave, ＋, and those seconds are the
// reference, taken whole -- not re-ranked, trimmed to refWant or capped at
// refTakeMax. Kept per RECORDING, not per narrator slot, so re-tagging does
// not hand them to someone else. Part of voiceKey: change a take and every
// line is respoken.

import (
	"crypto/sha1"
	"encoding/json"
	"fmt"
	"math"
	"os"
	"path/filepath"
	"sort"
)

// voiceTake is one stretch of a recording, in that file's own seconds.
type voiceTake struct {
	S float64 `json:"s"`
	E float64 `json:"e"`
}

func (t voiceTake) dur() float64 { return t.E - t.S }

// takeMin is the shortest stretch ＋ will accept. Not refMinLen (five seconds,
// what an automatic pick has to clear): somebody adding a third piece to two
// good ones knows what they are doing, and a floor there would be this file
// arguing with the ear it exists to defer to. It is here at all because a
// click that moved two pixels is a click, and a 30 ms take in the list is a
// mistake nothing else would report.
const takeMin = 0.4

// takesFile is where they live -- beside voice.txt and pitch.txt, the project's
// other answers about who speaks.
func (a *App) takesFile() string { return filepath.Join(a.narrateDir(), "takes.json") }

// takesRead1 reads the file once into the map. The caller holds voiceMu: the
// map itself never leaves this lock, which is the point. It used to be handed
// back to be indexed by the caller, and the two threads that ask -- the GUI
// after a ＋, and whatever worker is about to speak a line (voiceKey) -- would
// then read it while a ＋ on the other one was writing it. A Go map does not
// merely give a stale answer to that; it stops the program.
func (a *App) takesRead1() {
	if a.takesRead {
		return
	}
	a.takesRead, a.takesMap = true, map[string][]voiceTake{}
	if b, err := os.ReadFile(a.takesFile()); err == nil {
		var m map[string][]voiceTake
		if json.Unmarshal(b, &m) == nil {
			for k, v := range m {
				if ts := cleanTakes(v); len(ts) > 0 {
					a.takesMap[k] = ts
				}
			}
		}
	}
}

// takesFor is one recording's hand-picked takes, in order. Asked for on every
// cache key, which is once per narration line, so the file is read once and the
// answer is a lookup after that.
func (a *App) takesFor(base string) []voiceTake {
	a.voiceMu.Lock()
	defer a.voiceMu.Unlock()
	a.takesRead1()
	return a.takesMap[base]
}

// setTakesFor writes them and drops the reference built from the old ones, so
// the next thing spoken cuts a fresh one -- the same move setVoice makes, for
// the same reason: the file on disk is an answer to a question that has
// changed. Safe off the GUI thread.
func (a *App) setTakesFor(base string, ts []voiceTake) error {
	if base == "" {
		return fmt.Errorf("no recording to pick takes from")
	}
	a.voiceMu.Lock()
	a.takesRead1() // the file first, or writing one recording's takes drops the rest
	if ts = cleanTakes(ts); len(ts) == 0 {
		delete(a.takesMap, base)
	} else {
		a.takesMap[base] = ts
	}
	b, err := json.MarshalIndent(a.takesMap, "", "  ")
	a.voiceMu.Unlock()
	if err != nil {
		return err
	}
	if err := os.MkdirAll(a.narrateDir(), 0o755); err != nil {
		return err
	}
	if err := os.WriteFile(a.takesFile(), b, 0o644); err != nil {
		return err
	}
	os.Remove(a.refBase())
	os.Remove(a.refPath())
	return nil
}

// takeSource is the recording the chosen voice is cut from, or "" when there
// is nothing to pick from: a wav out of the voices folder is used whole, and
// "no audio" is not a voice.
func (a *App) takeSource() string {
	// narratorSource says "" for slot 0, which is what a voice that is not a
	// narrator has: a wav from the voices folder, or "no audio"
	return a.narratorSource(narratorSlot(a.voiceID()))
}

// voiceTakes is the chosen voice's own takes, which is what the cache key and
// the reference are built from.
func (a *App) voiceTakes() []voiceTake {
	src := a.takeSource()
	if src == "" {
		return nil
	}
	return a.takesFor(baseName(src))
}

// ---- the list as a set of seconds -------------------------------------------

// cleanTakes is the shape everything else may assume: sorted, non-empty, and
// with nothing overlapping or touching. Overlaps are merged rather than
// rejected because they are how a set is edited -- widening a take by dragging
// across its neighbour is one gesture, not an error -- and a reference cut from
// overlapping takes would say the same syllable twice.
func cleanTakes(ts []voiceTake) []voiceTake {
	var in []voiceTake
	for _, t := range ts {
		if t.E-t.S >= takeMin && t.S >= 0 {
			in = append(in, t)
		}
	}
	sort.Slice(in, func(i, j int) bool { return in[i].S < in[j].S })
	var out []voiceTake
	for _, t := range in {
		if n := len(out); n > 0 && t.S <= out[n-1].E {
			out[n-1].E = math.Max(out[n-1].E, t.E)
			continue
		}
		out = append(out, t)
	}
	return out
}

// addTake puts one stretch in the set.
func addTake(ts []voiceTake, s, e float64) []voiceTake {
	return cleanTakes(append(append([]voiceTake(nil), ts...), voiceTake{S: s, E: e}))
}

// dropTakes takes the seconds [s, e) back out, which is not the same as
// removing the takes that overlap them: a selection dragged across the middle
// of a long take means "not that bit", and throwing the whole take away would
// lose the two good halves either side of it. Anything left shorter than
// takeMin goes, since that is not a stretch anybody chose.
func dropTakes(ts []voiceTake, s, e float64) []voiceTake {
	var out []voiceTake
	for _, t := range ts {
		if e <= t.S || s >= t.E {
			out = append(out, t)
			continue
		}
		if t.S < s {
			out = append(out, voiceTake{S: t.S, E: s})
		}
		if t.E > e {
			out = append(out, voiceTake{S: e, E: t.E})
		}
	}
	return cleanTakes(out)
}

// takesTotal is how much reference the set adds up to -- what the status line
// says after every ＋ and －, because "is that enough" is the one question the
// picture cannot answer.
func takesTotal(ts []voiceTake) float64 {
	sum := 0.0
	for _, t := range ts {
		sum += t.dur()
	}
	return sum
}

// sameTakes asks whether two sets are the same seconds, which is what a ＋ or
// － needs to know before it says anything happened. Counting them and adding
// them up is not that question -- two sets can agree on both and still be
// different takes -- and the answer to the real one is this cheap.
func sameTakes(a, b []voiceTake) bool {
	if len(a) != len(b) {
		return false
	}
	for i := range a {
		if a[i] != b[i] {
			return false
		}
	}
	return true
}

// takesKey is the set as a cache key: empty for a voice nobody has hand-picked,
// so every project that has never touched this keeps the key it already has and
// nothing already spoken is orphaned by the feature existing. Rounded to the
// millisecond, which is finer than any edge anyone can drag and coarse enough
// that float noise cannot invent a new voice.
func takesKey(ts []voiceTake) string {
	if len(ts) == 0 {
		return ""
	}
	h := sha1.New()
	for _, t := range ts {
		fmt.Fprintf(h, "%.3f-%.3f;", t.S, t.E)
	}
	return fmt.Sprintf("%x", h.Sum(nil)[:4])
}

// handPicks turns the set into what ensureVoiceBase cuts from. The word count
// is nought and stays nought: it is what refCuts RANKS by, and a hand-picked
// set is not ranked.
func handPicks(ts []voiceTake) []refTake {
	out := make([]refTake, 0, len(ts))
	for _, t := range ts {
		out = append(out, refTake{s: t.S, e: t.E})
	}
	return out
}
