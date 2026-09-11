package main

// Where a retake's edges actually are.
//
// The ASR stamps a word to 80 ms, and on the first word after a silence the
// stamp runs late -- half a second late, measured, on a take's opening "And".
// A boundary placed on the stamp therefore lands INSIDE the word it was meant
// to precede, and whatever came out before it -- the breath, the false start
// the ASR folded away -- is kept.
//
// The waveform envelope already exists for the timeline, at 10 ms (waveHz),
// and for placing a cut it is the right instrument: what matters at a join is
// where sound starts and stops, not what the sound was. So the two edges of a
// mark are moved onto it. The cut ends where the last kept sound ends, and
// resumes where the retake's sound begins -- everything between goes, whatever
// it was, which is what "said again" means.
//
// The envelopes are built once per source and kept under cache/edges, apart
// from the timeline's own cache: that one is built to the page's channel count
// and this one is mono, and a file written to one reading must not be read by
// the other.

import (
	"math"
	"path/filepath"
	"sort"
)

const (
	// how far an edge may be moved off its stamp. A stamp half a second late
	// is what was measured; a sound a whole second away is a different sound.
	edgeReach = 0.8
	// what is left of a sound at a cut: a few buckets, so the last consonant
	// is not shaved and the first is not clipped
	edgePad = 0.05
	// how far behind its sound a stamp may run and still be that sound's:
	// half a second was measured on a take's opening word
	lateStamp = 0.6

	// With ALIGNED words the envelope's job changes: the words fence the cut
	// and the sound only chooses where inside the fence it falls.
	//
	// how far below a word's own level its tail still counts as the word.
	// On the meter scale the envelope is drawn in (-70..0 dBFS over 0..255),
	// a breath sits 20 dB or more under the word it follows, and a trailing
	// consonant a few dB under; twelve tells them apart.
	edgeTailDB = 12.0
	// how far past the aligner's edge a word's own sound is followed. The
	// aligner is within a fiftieth on an onset and can be a touch early on
	// a trailing s or k; a quarter second is that, and not the breath after.
	edgeTailMax = 0.25
	// how far into the gap the quietest moment is looked for. Past the
	// word's own tail the gap is breath and room; the cut goes where that
	// is lowest, and a trough further off than this is a different gap.
	troughReach = 0.4
)

// endAfter is where the cut ends after a word that stays, given the word and
// how far the cut may go (the next word's start, which it never reaches):
// the word's own sound followed to where it drops (edgeTailMax), then the
// quietest moment after that (troughReach). The words fence it; the sound
// chooses inside the fence. Without an envelope, a hair after the word.
func (e *edges) endAfter(w srcWord, limit float64) float64 {
	if limit <= w.e {
		return w.e
	}
	i0, i1 := e.at(w.s), e.at(w.e)
	lim := e.at(limit)
	if e == nil || i0 < 0 || i1 < 0 {
		return math.Min(w.e+wordPad, limit)
	}
	if lim < 0 {
		lim = len(e.wf.chans[0]) - 1
	}
	thr := e.tailLevel(i0, i1)
	end := i1
	for stop := min(lim, i1+int(edgeTailMax*e.wf.hz)); end < stop && e.sound(end, thr); end++ {
	}
	best := end
	for k := end; k <= min(lim, end+int(troughReach*e.wf.hz)); k++ {
		if e.wf.chans[0][k] < e.wf.chans[0][best] {
			best = k
		}
	}
	return math.Min(limit, math.Max(w.e, e.off+float64(best)/e.wf.hz))
}

// startBefore is the same the other way: where the cut resumes before a word
// that starts again, given how far back it may go (the last dropped word's
// end, which it never reaches).
func (e *edges) startBefore(w srcWord, limit float64) float64 {
	if limit >= w.s {
		return w.s
	}
	i0, i1 := e.at(w.s), e.at(w.e)
	lim := e.at(limit)
	if e == nil || i0 < 0 || i1 < 0 {
		return math.Max(w.s-wordPad, limit)
	}
	if lim < 0 {
		lim = 0
	}
	thr := e.tailLevel(i0, i1)
	start := i0
	for stop := max(lim, i0-int(edgeTailMax*e.wf.hz)); start > stop && e.sound(start-1, thr); start-- {
	}
	best := start
	for k := start; k >= max(lim, start-int(troughReach*e.wf.hz)); k-- {
		if e.wf.chans[0][k] < e.wf.chans[0][best] {
			best = k
		}
	}
	return math.Max(limit, math.Min(w.s, e.off+float64(best)/e.wf.hz))
}

// tailLevel is what still counts as the word between buckets i0 and i1: its
// own peak less edgeTailDB, and never under the room (floor).
func (e *edges) tailLevel(i0, i1 int) uint8 {
	peak := uint8(0)
	for k := i0; k <= i1 && k < len(e.wf.chans[0]); k++ {
		peak = max(peak, e.wf.chans[0][k])
	}
	thr := int(peak) - int(math.Round(edgeTailDB*255/70))
	return uint8(max(thr, int(e.floor(e.off+float64(i0)/e.wf.hz))))
}

// edges is the envelope of one source, mono, with the source's place in the
// session so a session second can be looked up in it.
type edges struct {
	wf  *waveform
	off float64 // session second the file's first sample lands on
}

// loadEdges reads or builds the envelope for a source.
func (a *App) loadEdges(path string, off float64) *edges {
	wf, err := loadWave(filepath.Join(a.outDir, "cache", "edges"),
		tlAudio{base: baseName(path), path: path, chans: 1})
	if err != nil || wf == nil || len(wf.chans) == 0 {
		return nil
	}
	return &edges{wf: wf, off: off}
}

// at is the bucket a session second falls in, or -1 outside the file.
func (e *edges) at(t float64) int {
	i := int((t - e.off) * e.wf.hz)
	if i < 0 || i >= len(e.wf.chans[0]) {
		return -1
	}
	return i
}

// floor is what counts as sound around a point: the quiet of that stretch of
// the recording, with room over it. Measured on the buckets around t rather
// than fixed, because a room, a microphone and a gain setting each put the
// silence somewhere else -- and taken from the quieter end of the window, so
// a window that is mostly talking still finds its floor.
func (e *edges) floor(t float64) uint8 {
	i := e.at(t)
	if i < 0 {
		return 255
	}
	lo, hi := max(0, i-400), min(len(e.wf.chans[0]), i+400) // ±4 s
	win := append([]uint8(nil), e.wf.chans[0][lo:hi]...)
	sort.Slice(win, func(a, b int) bool { return win[a] < win[b] })
	quiet := int(win[len(win)/5]) // the 20th percentile is the room
	return uint8(min(255, max(quiet*3, quiet+8)))
}

// sound is whether bucket i is above the floor.
func (e *edges) sound(i int, thr uint8) bool {
	return i >= 0 && i < len(e.wf.chans[0]) && e.wf.chans[0][i] >= thr
}

// endBefore is where the last sound before session second t ends: the cut
// ends there. t is the stamp of the first abandoned word, which may fall inside
// that word or just before it; either way the sound run it belongs to is
// stepped out of first, then the quiet before it, and the sound before THAT is
// what the cut keeps. Unchanged when no such edge is within reach -- speech
// with no gap in it is cut where the stamp says.
func (e *edges) endBefore(t float64) float64 {
	i := e.at(t)
	if i < 0 {
		return t
	}
	thr := e.floor(t)
	limit := int(edgeReach * e.wf.hz)
	j := i
	for ; j >= 0 && i-j <= limit && e.sound(j, thr); j-- {
	}
	for ; j >= 0 && i-j <= limit && !e.sound(j, thr); j-- {
	}
	if j < 0 || i-j > limit {
		return t
	}
	return e.off + float64(j+1)/e.wf.hz + edgePad
}

// startAt is where the retake's sound begins: the cut resumes there. t is the
// stamp of its first word, and a stamp runs LATE -- never early -- so the word
// is the sound run the stamp falls in, or the one that ended just before it.
// Its onset is where the cut resumes. A stamp with quiet for a while before it
// is a stamp the sound has not reached yet, and it stands as it is: moving
// forward to the next sound would clip the very word it names.
func (e *edges) startAt(t float64) float64 {
	i := e.at(t)
	if i < 0 {
		return t
	}
	thr := e.floor(t)
	limit := int(edgeReach * e.wf.hz)
	j := i
	if !e.sound(i, thr) {
		// back over the quiet to the run before it, if the quiet is short
		// enough to be the tail of a late stamp
		for ; j > 0 && i-j <= int(lateStamp*e.wf.hz) && !e.sound(j-1, thr); j-- {
		}
		if j == 0 || !e.sound(j-1, thr) {
			return t
		}
	}
	for ; j > 0 && i-j <= limit && e.sound(j-1, thr); j-- {
	}
	if i-j > limit {
		return t
	}
	return max(e.off, e.off+float64(j)/e.wf.hz-edgePad)
}
