package main

import (
	"fmt"
	"math"
	"path/filepath"
	"sort"
)

// ---- moving a recording, and moving the cut on it ---------------------------
//
// srcClock places files by name and guesses for the rest, so two waveforms of
// one shout can sit a column apart. The right button drags: on the pictures a
// whole row, on a lane one recording, until they line up. The offset is kept
// per source in cut.json and re-applied on load; the render derives the same
// placement. Over a selection the same button slides the KEPT spans across the
// footage instead -- moving the cut rather than the camera.

// shiftOf is one source's correction, and zero for a source that has none.
func (ed *cutEditor) shiftOf(base string) float64 { return ed.shift[base] }

// applyShift puts the saved corrections onto a freshly loaded timeline. reload
// builds every start from srcClock, which knows nothing about them, so this
// is the one place they go on -- before the gaps are worked out and before
// relayout measures anything.
func (ed *cutEditor) applyShift() {
	for b, d := range ed.shift {
		ed.slideSrc(b, d)
	}
}

// slideSrc moves everything that belongs to one source along the timeline:
// footage, the sound from the same file, the speech-gap points worked out
// against it. It does NOT touch ed.shift (applyShift and the drag do). Gap
// hints another camera derived from this one's speech stay put -- re-reading
// TSVs per frame is not a drag.
func (ed *cutEditor) slideSrc(base string, d float64) {
	if d == 0 {
		return
	}
	for i := range ed.vids {
		if ed.vids[i].base == base {
			ed.vids[i].start += d
		}
	}
	for i := range ed.auds {
		// by name, and also every further track of the video with that name: a
		// multi-track capture's second track is glued to the pictures it was
		// recorded with, so dragging the row to correct its clock has to take
		// the track with it or the correction is a desync nobody asked for. Its
		// own name still works as a name, so it can be nudged apart afterwards.
		if ed.auds[i].base == base || ed.auds[i].base == trackName(base, ed.auds[i].track) {
			ed.auds[i].start += d
		}
	}
	for i := range ed.gaps[base] {
		ed.gaps[base][i] += d
	}
}

// laneSrcs is every source drawn on one row of the picture band. A camera
// stopped and started again is several files on one row and one clock, and
// correcting it means correcting all of them by the same seconds -- shifting
// only the file under the pointer would move that camera's own seam.
func (ed *cutEditor) laneSrcs(lane int) []string {
	var out []string
	for _, v := range ed.vids {
		if v.lane == lane && !contains(out, v.base) {
			out = append(out, v.base)
		}
	}
	return out
}

// setShift puts one source's correction at exactly want seconds and moves the
// source to match. Absolute, so a hundred drag updates accumulate nothing and
// letting go where you started leaves the timeline as found. Rows are frozen
// first: they are worked out FROM the overlaps a shift changes.
func (ed *cutEditor) setShift(base string, want float64) {
	want = ed.clampShift(want)
	cur := ed.shift[base]
	if want == cur {
		return
	}
	ed.freezeRows()
	if ed.shift == nil {
		ed.shift = map[string]float64{}
	}
	ed.slideSrc(base, want-cur)
	if want == 0 {
		delete(ed.shift, base) // back where it started: nothing to save
	} else {
		ed.shift[base] = want
	}
}

// shiftTo moves a set of sources to d seconds off the corrections they had at
// the start of the drag (from), and puts the page back together around them.
func (ed *cutEditor) shiftTo(srcs []string, from map[string]float64, d float64) {
	rows := map[int]bool{}
	for _, v := range ed.vids {
		if contains(srcs, v.base) {
			rows[v.lane] = true
		}
	}
	// how far they actually got, which is not d when the clamp bit. Read off
	// any one of them: every source in a drag moves by the same seconds.
	var was, now float64
	if len(srcs) > 0 {
		was = ed.shift[srcs[0]]
	}
	for _, b := range srcs {
		ed.setShift(b, from[b]+d)
	}
	if len(srcs) > 0 {
		now = ed.shift[srcs[0]]
	}
	ed.followCopies(rows, srcs, now-was)
	sort.SliceStable(ed.vids, func(i, j int) bool { return ed.vids[i].start < ed.vids[j].start })
	sortLanes(ed.auds)
	ed.relayout()
}

// followCopies drags copied stretches along with the footage they were copied
// from: a copy is stored as the session second its footage starts at
// (copyScheme), which only means anything relative to the recording. Only when
// the WHOLE row moved, and by the delta actually applied.
func (ed *cutEditor) followCopies(rows map[int]bool, srcs []string, d float64) {
	if d == 0 {
		return
	}
	whole := map[int]bool{}
	for lane := range rows {
		whole[lane] = true
		for _, b := range ed.laneSrcs(lane) {
			if !contains(srcs, b) {
				whole[lane] = false
			}
		}
	}
	for i, s := range ed.segs {
		if at, ok := copySrc(s.Ins); ok && whole[s.Cam] {
			ed.segs[i].Ins = fmt.Sprintf("%s%.3f", copyScheme, math.Max(0, at+d))
		}
	}
}

// clampShift bounds a correction to the length of all the material there is, in
// each direction. A drag is in pixels and a session is minutes, so the hand
// cannot run away with this in one go -- but drags add up, and past that bound
// the recording overlaps nothing: a lane with nothing beside it is a lane there
// is no longer anything to read it against, which is the one thing this whole
// row is for.
func (ed *cutEditor) clampShift(want float64) float64 {
	span := 60.0
	for _, v := range ed.vids {
		span += v.dur
	}
	for _, au := range ed.auds {
		if !au.master {
			span += au.dur
		}
	}
	return math.Max(-span, math.Min(span, want))
}

// freezeRows writes the rows down as they are now, once per project. Until the
// first drag they stay derived, so adding a camera on Prepare still puts
// it where the arithmetic says; from the first drag on they are the project's,
// because cutSeg.Cam is a row number and it has to keep meaning the same camera.
func (ed *cutEditor) freezeRows() {
	if ed.rows != nil {
		return
	}
	ed.rows = map[string]int{}
	for _, v := range ed.vids {
		ed.rows[v.base] = v.lane
	}
}

// slideGreen moves the kept scenes wholly inside the selection by d seconds
// over footage that stays put. from is the cut at drag start, so a hundred
// updates do not clamp cumulatively. They move as a GROUP keeping lengths: the
// tightest recording bound wins for all. Cards inside travel with them; a
// half-covered scene is left alone rather than split.
func (ed *cutEditor) slideGreen(from []cutSeg, d float64) bool {
	a, b := ed.selSpan()
	in := func(s cutSeg) bool { return s.S >= a-1e-9 && s.E <= b+1e-9 }
	lo, hi, any := math.Inf(-1), math.Inf(1), false
	for _, s := range from {
		if !in(s) {
			continue
		}
		any = true
		// asked of where it came FROM, not of where it lands: on a row where
		// the camera was stopped and started again there are seconds nothing
		// covers, and a scene over one of those would be clamped by nothing.
		// A card has no footage of its own to run off the end of, and answers
		// nil here.
		if s.isInsert() {
			continue
		}
		if v := videoOn(ed.vids, s.Cam, s.S); v != nil {
			lo = math.Max(lo, v.start-s.S)
			hi = math.Min(hi, v.start+v.dur-s.E)
		}
	}
	if d = math.Max(lo, math.Min(hi, d)); !any || math.Abs(d) < 1e-9 {
		return false
	}
	out := make([]cutSeg, 0, len(from))
	for _, s := range from {
		if in(s) {
			s.S, s.E = s.S+d, s.E+d
		}
		out = append(out, s)
	}
	ed.segs = out
	ed.coalesce()
	return true
}

// ---- moving a part onto another row -----------------------------------------

// rowAt is the row of the picture band under y, counting a row's wave strip as
// the row: for moving things between rows, grabbing a part by its sound is
// grabbing the part.
func (ed *cutEditor) rowAt(y float64) int {
	if l := ed.laneAt(y); l >= 0 {
		return l
	}
	return ed.pairAt(y)
}

// rowFits says whether every dragged source could sit on row `to` without
// lying over something already there. Overlap in TIME is the one thing a row
// cannot draw -- there is no x where both could be shown -- which is exactly
// why assignLanes stacked them apart in the first place; everything else about
// a row move is bookkeeping.
func (ed *cutEditor) rowFits(srcs []string, to int) bool {
	if len(srcs) == 0 || to < 0 || to >= ed.laneN {
		return false
	}
	for i := range ed.vids {
		m := &ed.vids[i]
		if !contains(srcs, m.base) {
			continue
		}
		if m.lane == to {
			return false // already there: nothing to do, nothing to undo
		}
		for j := range ed.vids {
			v := &ed.vids[j]
			if v.lane != to || contains(srcs, v.base) {
				continue
			}
			if m.start < v.start+v.dur-1e-9 && v.start < m.start+m.dur-1e-9 {
				return false
			}
		}
	}
	return true
}

// moveRow puts the dragged sources onto row `to` when there is room; the row is
// pinned (freezeRows) since cutSeg.Cam is a row number. Scenes wholly inside a
// moved source's stretch that named its old row are repointed -- unlike a time
// drag, a row drag changes no seconds, so a scene left behind would show a
// hole. An emptied row stays (killLane renumbers on purpose).
func (ed *cutEditor) moveRow(srcs []string, to int) bool {
	if !ed.rowFits(srcs, to) {
		return false
	}
	from := -1
	for i := range ed.vids {
		if contains(srcs, ed.vids[i].base) {
			from = ed.vids[i].lane
			break
		}
	}
	ed.freezeRows()
	// the shape as it stands becomes the floor, so the vacated row survives
	// the relayout below even when it is the bottom one: emptied on purpose,
	// it stays until its ✕ says otherwise (killRow) -- the same wait an
	// emptied middle row already gets from the pins holding the gap open
	ed.nRows = max(ed.nRows, ed.laneN)
	for _, b := range srcs {
		ed.rows[b] = to
	}
	for i := range ed.segs {
		s := &ed.segs[i]
		if s.Cam != from || s.isInsert() {
			continue
		}
		for j := range ed.vids {
			m := &ed.vids[j]
			if contains(srcs, m.base) && s.S >= m.start-1e-9 && s.E <= m.start+m.dur+1e-9 {
				s.Cam = to
				break
			}
		}
	}
	ed.relayout()
	return true
}

// ---- snapping the drag ------------------------------------------------------

// slideSnapSet is worked out once, at the press: the session seconds this drag
// MOVES (the dragged recordings' edges, or the travelling scenes' borders) and
// the still ones to land on (the selection's edges, the playhead, effect bands,
// every staying recording and scene). Frozen, since the drag recomputes from
// the press each update.
func (ed *cutEditor) slideSnapSet(srcs []string, green bool) (edges, targets []float64) {
	// the session's own start, always: everything else here is only a target
	// while something still sits on it, and the one recording that anchors 0
	// stops offering it the moment it is the thing being dragged
	targets = append(targets, 0)
	a0, a1 := ed.selSpan()
	if ed.sel.active {
		targets = append(targets, a0, a1)
	}
	if ed.hasPlay {
		targets = append(targets, ed.playhead)
	}
	for _, f := range ed.fx {
		t0, t1 := f.fxSpan()
		targets = append(targets, t0)
		if t1 > t0 {
			targets = append(targets, t1)
		}
	}
	for _, v := range ed.vids {
		if contains(srcs, v.base) {
			edges = append(edges, v.start, v.start+v.dur)
		} else {
			targets = append(targets, v.start, v.start+v.dur)
		}
	}
	for _, au := range ed.auds {
		if au.master {
			continue // it moves with its footage, and the footage was counted
		}
		if contains(srcs, au.base) {
			edges = append(edges, au.start, au.start+au.dur)
		} else {
			targets = append(targets, au.start, au.start+au.dur)
		}
	}
	for _, s := range ed.segs {
		// the same wholly-inside rule slideGreen moves by: a scene that will
		// travel offers its borders to the still ones, never to itself
		if green && ed.sel.active && s.S >= a0-1e-9 && s.E <= a1+1e-9 {
			edges = append(edges, s.S, s.E)
		} else {
			targets = append(targets, s.S, s.E)
		}
	}
	return edges, targets
}

// slideSnap pulls a drag of d seconds onto the nearest exact meeting of a
// moving edge with a still one, when the hand brings any pair within tol
// seconds -- and otherwise changes nothing. Every moving edge is offered to
// every target and the closest pair wins, the same bargain the selection band
// and the effect bands make (snapSelSpan, snapFxSpan): flush on the left is
// worth as much as flush on the right.
func slideSnap(d float64, edges, targets []float64, tol float64) float64 {
	best, bd := d, tol
	for _, e := range edges {
		for _, t := range targets {
			if diff := math.Abs(t - (e + d)); diff < bd {
				best, bd = t-e, diff
			}
		}
	}
	return best
}

// shiftLabel is what the drag leaves in the status line. Signed seconds,
// because lining two waveforms up by eye is arithmetic nobody wants to do twice
// -- the number is what makes it repeatable on the next project shot with the
// same two devices.
func shiftLabel(what string, d float64) string {
	if d == 0 {
		return what + " is back where it started"
	}
	return fmt.Sprintf("%s moved %+.2f s", what, d)
}

// The two maps are copied in and out of every undo snapshot rather than shared,
// for the reason every other list in cutState is: a snapshot that pointed at the
// live map would be rewritten by the next drag and would then put the timeline
// back exactly where it already was.
func copyShift(m map[string]float64) map[string]float64 {
	if m == nil {
		return nil
	}
	out := make(map[string]float64, len(m))
	for k, v := range m {
		out[k] = v
	}
	return out
}

func copyRows(m map[string]int) map[string]int {
	if m == nil {
		return nil
	}
	out := make(map[string]int, len(m))
	for k, v := range m {
		out[k] = v
	}
	return out
}

func sameShift(a, b map[string]float64) bool {
	if len(a) != len(b) {
		return false
	}
	for k, v := range a {
		if w, ok := b[k]; !ok || w != v {
			return false
		}
	}
	return true
}

// The rows are compared as well as the seconds, because a drag out and back
// leaves no seconds behind and still leaves the rows frozen -- and an undo that
// did not unfreeze them would leave the project nailed to a shape by an edit
// that is no longer in it.
func sameRows(a, b map[string]int) bool {
	if len(a) != len(b) {
		return false
	}
	for k, v := range a {
		if w, ok := b[k]; !ok || w != v {
			return false
		}
	}
	return true
}

// sessionRows is the session timeline put onto the timeline the cut is made
// against: session.tsv is written off the raw file clocks, and a clock
// corrected by hand afterwards would leave Suggest, Narrate and the upload
// text reading the wrong seconds. tsvRow.src says which recording each line
// came off, so the correction is one addition per line.
func (a *App) sessionRows() []tsvRow {
	rows := loadTSVRows(filepath.Join(a.transcriptDir(), "session.tsv"))
	shift := a.produceCut().Shift
	if len(shift) == 0 {
		return rows
	}
	for i := range rows {
		d := shift[rows[i].src]
		rows[i].s, rows[i].e = rows[i].s+d, rows[i].e+d
	}
	sort.SliceStable(rows, func(i, j int) bool { return rows[i].s < rows[j].s })
	return rows
}
