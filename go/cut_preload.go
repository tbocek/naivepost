package main

// Where the line is going next, said to the player ahead of time.
//
// The transport moves the line without being asked in three places: the gap
// skip (a clip ends, the next begins), the review (a join's seconds are
// heard, the next join's run-up is sought) and the walk-on (a recording ends,
// the next begins). Each of those is a jump the tick will make, and each is
// known seconds before it is made. This tells the player's spare pipeline
// (player_spare.go) so the jump is a swap and not a reload.

import "math"

// preloadLead is how far ahead of a jump the spare is asked for it. A
// preroll is a few hundred milliseconds on a local file; three seconds is
// that with room for a slow disk, and short enough that a hand moving the
// line does not have a stale preroll waiting for long.
const preloadLead = 3.0

// jumpAhead is the cut's own jumps, pure: the session second the line will
// be put at next (to) and when (at), if that is within lead seconds of t.
// Under ▶✂ a clip's end is a jump to the next clip; under a review a seam's
// window closing is a jump to the next seam's run-up, unless that run-up is
// already behind the line and is simply played into. The earlier of the two
// is the answer, since it comes first.
func jumpAhead(segs []cutSeg, t float64, cutOnly, review bool, seam int, lead float64) (at, to float64, ok bool) {
	at = math.Inf(1)
	if cutOnly {
		if cur, _ := gapAt(segs, t); cur >= 0 && cur+1 < len(segs) && segs[cur].E-t < lead {
			at, to, ok = segs[cur].E, segs[cur+1].S, true
		}
	}
	if review && seam >= 0 && seam+1 < len(segs)-1 {
		_, end := reviewWindow(segs, seam)
		from, _ := reviewWindow(segs, seam+1)
		if from > end && end-t < lead && end < at {
			at, to, ok = end, from, true
		}
	}
	return at, to, ok
}

// preloadAhead is the tick's call: whichever jump comes first -- the cut's
// own (jumpAhead) or the recording under the line running out -- the spare
// is pointed at where it lands. Nothing to do while nothing is coming.
func (ed *cutEditor) preloadAhead() {
	v := ed.playVideo
	if ed.player == nil || v == nil {
		return
	}
	t := ed.playhead
	at, to, ok := jumpAhead(ed.segs, t, ed.cutOnly, ed.reviewOn, ed.review, preloadLead)
	// the file's end is a jump in every mode (walkOn): to the next recording
	if end := v.start + v.dur; end-t < preloadLead && end < at {
		if nt, ok2 := ed.recordingAfter(end); ok2 {
			at, to, ok = end, nt, true
		}
	}
	if !ok {
		return
	}
	to = ed.playable(to)
	if nv := ed.videoAt(to); nv != nil {
		ed.player.Preload(nv.path, nv.at(to))
	}
}

// recordingAfter is the first second of the first recording that starts at
// or after t, other than the one playing -- where walkOn will go.
func (ed *cutEditor) recordingAfter(t float64) (float64, bool) {
	best := math.Inf(1)
	for i := range ed.vids {
		if v := &ed.vids[i]; v != ed.playVideo && v.start >= t-0.01 && v.start < best {
			best = v.start
		}
	}
	if math.IsInf(best, 1) {
		return 0, false
	}
	return best, true
}
