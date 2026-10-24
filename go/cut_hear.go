package main

import (
	"fmt"
	"math"
	"slices"
	"strings"

	"github.com/diamondburned/gotk4/pkg/cairo"
)

// Which lanes a scene is heard on: per scene, on the lanes. Take a scene in
// hand and every audio lane shows green/grey with a badge that toggles it; two
// lanes on are summed at their recorded levels with a limiter on the clip
// (clipCeil, produce.go). The badge sits at the LEFT of the held scene only --
// the rest of the scene is the press that moves the line or takes it in hand.
// Both the camera's own strip and the recorders' band wear one.

const (
	hearR   = 4.5  // the speaker cone, from its centre
	hearPad = 3.5  // the plate's edge, beyond it
	hearHit = 10.0 // and the target, bigger than either
	// its centre, in from the held scene's LEFT border -- far enough that the
	// badge's TARGET begins where the border's grab ends (edgeGrab), the same
	// clearance the bar's ✕ keeps from the grip at the other end. Closer in,
	// the two shared pixels: a press meant for the border switched a lane,
	// and the badge looked like part of the handle it was sitting on.
	hearIn = edgeGrab + hearHit
	// under this a scene has no room to wear one, by killMin's rule: the
	// target reaches hearIn+hearHit in from the left border, and a scene
	// narrower than twice that has its MIDDLE inside the badge -- so a press
	// meant to put the line on the clip would switch a lane instead.
	hearMin = 2*(hearIn+hearHit) + 6
)

// hearBadge is one lane's answer for the held scene, and where it is drawn.
//
// h is the whole RECORDING's depth and not one channel's: a stereo microphone
// is two waveforms and one answer, and a wash that covered only the first would
// say the second is a lane of its own that nobody has decided about yet.
type hearBadge struct {
	base   string
	cx, cy float64 // timeline x, and the middle of the lane in area y
	h      float64 // how deep the lane is, all its channels together
	on     bool    // this scene hears this lane
}

// hearScene is the scene the badges are about: the one in hand, else the one
// under the line -- which is the scene the preview's hush follows (syncHush),
// so the badge shown is the thing you hear.
func (ed *cutEditor) hearScene() *cutSeg {
	if s := ed.heldSeg(); s != nil {
		return s
	}
	if i := ed.segAt(ed.playhead); i >= 0 {
		return &ed.segs[i]
	}
	return nil
}

// hearX is where the badges for the scene go, and whether it is wide enough
// to wear any. Timeline px, like everything else drawn in a translated area.
func (ed *cutEditor) hearX() (float64, bool) {
	s := ed.hearScene()
	if s == nil || s.isInsert() {
		// an insert brings its own sound or replaces the lane it was laid in
		// (dropLane), and neither is a question about which lanes are heard
		return 0, false
	}
	x0, x1 := ed.xOf(s.S), ed.xOf(s.E)
	if x1-x0 < hearMin {
		return 0, false
	}
	return x0 + hearIn, true
}

// hearBadgesAud is the badges in the recorders' band, walking exactly the
// layout drawAudio draws so the hand lands where the eye is pointing. One per
// RECORDING and not per channel: a stereo lane is one microphone.
func (ed *cutEditor) hearBadgesAud() []hearBadge {
	cx, ok := ed.hearX()
	if !ok {
		return nil
	}
	s := ed.hearScene()
	var out []hearBadge
	y := wavePad
	for _, au := range ed.sepAuds() {
		h := float64(ed.lanes(au)) * waveLaneH
		out = append(out, hearBadge{au.base, cx, y + h/2, h, s.hears(au.base)})
		y += h + waveGap
	}
	return out
}

// ---- the whole lane -----------------------------------------------------
//
// Each recording wears a lane switch on its name plate at the left; the
// footage's own sound has one on its strip. Off silences the lane in every
// scene, on silences it in none. It stores nothing new: it IS the badges,
// written to every scene at once. Per ROW, not per file -- a camera stopped
// and restarted is several recordings on one row.

const (
	// the switch's centre, and where a name starts beside it. Same plate and
	// speaker as a scene's badge (hearPlate). The switch stands in the gutter
	// (cut_fold.go), never over the waveform; the name is pinned and starts where
	// the strip ends.
	laneSwX   = gutterMid
	laneNameX = gutterPx + 4
)

// laneSwitch is one recording's whole-lane switch: where it is drawn in the
// recorders' band, in TIMELINE coordinates -- it stands in the gutter, which
// is the head of the tape and scrolls with it -- and whether the cut hears the
// lane anywhere.
type laneSwitch struct {
	base   string
	cx, cy float64
	on     bool
}

// laneHeard is whether the cut hears a lane in any scene at all. That is what
// the switch shows: a lane no scene keeps reads off, and one kept in a single
// scene reads on, because that scene is what pressing it would take away.
//
// An empty cut has no scene to hear anything, and the switch reads off --
// which is honest, and is why pressing it then says so rather than doing
// nothing quietly.
func (ed *cutEditor) laneHeard(base string) bool {
	for i := range ed.segs {
		if !ed.segs[i].isInsert() && ed.segs[i].hears(base) {
			return true
		}
	}
	return false
}

// laneSwitches is one per recording in the band, walking the same layout
// drawAudio draws so the hand lands where the eye is pointing. Centred on the
// recording's whole depth, exactly as its badge is: a stereo lane is one
// recording and gets one switch, not one per channel.
func (ed *cutEditor) laneSwitches() []laneSwitch {
	var out []laneSwitch
	y := wavePad
	for _, au := range ed.sepAuds() {
		h := float64(ed.lanes(au)) * waveLaneH
		out = append(out, laneSwitch{au.base, laneSwX, y + h/2, ed.laneHeard(au.base)})
		y += h + waveGap
	}
	return out
}

// laneSwitchAt is the switch under a press, or "". px is a TIMELINE x: the
// switch stands in the gutter and scrolls with the tape.
func (ed *cutEditor) laneSwitchAt(px, y float64) string {
	for _, s := range ed.laneSwitches() {
		if math.Abs(px-s.cx) <= hearHit && math.Abs(y-s.cy) <= hearHit {
			return s.base
		}
	}
	return ""
}

// ---- the same switch on the footage's own strip -----------------------------

// pairSwitch is one camera ROW's whole-cut switch for the sound filmed with
// its pictures: every master lane drawn on that row, one plate over the lot.
//
// cx is a TIMELINE x, like the band's switches: it stands in the gutter at the
// head of the tape (cut_gutter.go) rather than over the strip it is about. cy
// is an area y, which in this widget is the same thing -- only x is
// translated (drawTrack).
type pairSwitch struct {
	bases  []string
	cx, cy float64
	on     bool
}

// pairSwitches is one per row that has a paired strip, walking the layout
// drawTrack draws so the hand lands where the eye is pointing. A row whose
// footage filmed no sound has no strip and gets none.
func (ed *cutEditor) pairSwitches() []pairSwitch {
	var out []pairSwitch
	for r := 0; r < ed.laneN; r++ {
		h := ed.pairH(r)
		if h == 0 {
			continue
		}
		var bases []string
		on := false
		for i := range ed.vids {
			if ed.vids[i].lane != r {
				continue
			}
			if au := ed.pairAud(ed.vids[i].base); au != nil {
				bases = append(bases, au.base)
				on = on || ed.laneHeard(au.base)
			}
		}
		if len(bases) == 0 {
			continue
		}
		out = append(out, pairSwitch{bases, laneSwX, ed.laneTop(r) + ed.laneH() + h/2, on})
	}
	return out
}

// pairSwitchAt is the recordings whose switch is under a press, or nil. px is
// a TIMELINE x: the switch stands in the gutter and scrolls with the tape.
func (ed *cutEditor) pairSwitchAt(px, y float64) []string {
	for _, s := range ed.pairSwitches() {
		if math.Abs(px-s.cx) <= hearHit && math.Abs(y-s.cy) <= hearHit {
			return s.bases
		}
	}
	return nil
}

// pairSwitchName is what the status calls a row: the row, when its pictures
// are several recordings in a line, and the recording itself when it is one.
func pairSwitchName(bases []string) string {
	if len(bases) == 1 {
		return bases[0]
	}
	return fmt.Sprintf("the sound filmed with %s and %d more", bases[0], len(bases)-1)
}

// drawPairSwitches paints them, from inside drawTrack's translation and in the
// tape's own coordinates: the switch is in the gutter, which is part of the
// tape (cut_gutter.go). It used to be a widget x put back on the tape's scale
// by the view's left edge, so that it stayed pinned as the footage went past
// underneath -- and underneath is the word: it was drawn on the wave strip it
// is about.
func (ed *cutEditor) drawPairSwitches(cr *cairo.Context) {
	for _, s := range ed.pairSwitches() {
		hearPlate(cr, s.cx, s.cy, s.on, ed.badgeHot(badgeID{kind: badgePair, base: s.bases[0]}))
	}
}

// toggleLaneAll rewrites every scene's answer for one lane: off if any scene
// still hears it, on only from a lane silent everywhere. A FRESH list per
// scene (undo snapshots share the string slices, see toggleHear), and one
// pushUndo for the lot.
func (ed *cutEditor) toggleLaneAll(base string) {
	if base == "" {
		return
	}
	ed.toggleLanesAll([]string{base}, base)
}

// toggleLanesAll is that press for a switch standing for several recordings --
// a camera row (pairSwitches). name is what the status calls them. Off if ANY
// of them is still heard anywhere, so a half-silenced row finishes the job.
func (ed *cutEditor) toggleLanesAll(bases []string, name string) {
	if len(bases) == 0 {
		return
	}
	if len(ed.segs) == 0 {
		ed.a.setStatus(name + " is in no scene yet — cut something first")
		return
	}
	on := true
	for _, b := range bases {
		if ed.laneHeard(b) {
			on = false
		}
	}
	ed.pushUndo()
	n := 0
	for i := range ed.segs {
		s := &ed.segs[i]
		if s.isInsert() {
			continue // an insert brings its own sound; no scene silences it
		}
		var next []string
		for _, q := range s.Quiet {
			if !slices.Contains(bases, q) {
				next = append(next, q)
			}
		}
		if !on {
			next = append(next, bases...)
		}
		if len(next) != len(s.Quiet) {
			n++
		}
		s.Quiet = next
	}
	ed.persist()
	ed.redrawTracks()
	if on {
		ed.a.setStatus(fmt.Sprintf("%s is on for the whole cut — every scene hears it (%d changed)", name, n))
		return
	}
	ed.a.setStatus(fmt.Sprintf("%s off for the whole cut — %d scene(s) changed", name, n))
}

// ---- what every scene hears, said on every scene -----------------------------

// A lane a scene silences is washed grey where that scene is, on every scene
// -- otherwise the whole-lane switch would read as a state kept somewhere else.
// A wash, not badges: forty plates would be forty controls that are not there.

// laneWash is a stretch of one lane to be greyed: timeline x, area y.
type laneWash struct{ x0, x1, y0, y1 float64 }

// laneSilences is one per lane a scene does not hear, in the recorders' band.
//
// The scene the badges are about is left out: drawHearBadges paints that one
// itself, in the same grey, and painting it twice would make the scene under
// the hand darker than every other silent scene for no reason anybody could
// read off the page.
func (ed *cutEditor) laneSilences() []laneWash {
	auds := ed.sepAuds()
	if len(auds) == 0 {
		return nil
	}
	shown := ed.hearScene()
	var out []laneWash
	for i := range ed.segs {
		s := &ed.segs[i]
		if s.isInsert() || s == shown {
			continue // an insert brings its own sound; no scene silences it
		}
		for _, au := range auds {
			if s.hears(au.base) {
				continue
			}
			if y0, y1, ok := ed.audLaneSpan(au.base); ok {
				out = append(out, laneWash{ed.xOf(s.S), ed.xOf(s.E), y0, y1})
			}
		}
	}
	return out
}

// pairSilences is the same over the strips under the pictures: the sound
// filmed with the camera each scene is SHOWN from, which is the only strip
// with an answer to give (hearBadgesSrc's rule -- the render takes a clip's
// own sound off the recording it is cut from).
func (ed *cutEditor) pairSilences() []laneWash {
	shown := ed.hearScene()
	var out []laneWash
	for i := range ed.segs {
		s := &ed.segs[i]
		if s.isInsert() || s == shown {
			continue
		}
		v := pickVideoOn(ed.vids, s.Cam, s.S)
		if v == nil || s.hears(v.base) {
			continue
		}
		au := ed.pairAud(v.base)
		if au == nil {
			continue // a camera that filmed no sound has no strip to grey
		}
		y := ed.laneTop(v.lane) + ed.laneH()
		out = append(out, laneWash{ed.xOf(s.S), ed.xOf(s.E),
			y, y + float64(ed.lanes(*au))*waveLaneH})
	}
	return out
}

// drawSilences paints them, from inside each band's translation, in the grey
// drawHearBadges gives a lane the held scene does not hear -- one colour for
// one answer, wherever the page says it.
func (ed *cutEditor) drawSilences(cr *cairo.Context, washes []laneWash, vx0, vx1 float64) {
	cr.SetSourceRGBA(0.55, 0.55, 0.6, 0.3)
	n := 0
	for _, w := range washes {
		if w.x1 < vx0 || w.x0 > vx1 {
			continue
		}
		cr.Rectangle(w.x0, w.y0, w.x1-w.x0, w.y1-w.y0)
		n++
	}
	if n > 0 {
		cr.Fill()
	}
}

// hearBadgesSrc is the badges on the paired strips, in the picture band. Only
// the camera the scene is SHOWN from wears one: the render takes a clip's own
// sound off the recording it is cut from (pickVideoOn), so silencing another
// row's camera would be a control that does nothing to this scene.
func (ed *cutEditor) hearBadgesSrc() []hearBadge {
	cx, ok := ed.hearX()
	if !ok {
		return nil
	}
	s := ed.hearScene()
	v := pickVideoOn(ed.vids, s.Cam, s.S)
	if v == nil {
		return nil // no footage on that row at this second
	}
	au := ed.pairAud(v.base)
	if au == nil {
		return nil // a camera that filmed no sound
	}
	h := float64(ed.lanes(*au)) * waveLaneH
	return []hearBadge{{v.base, cx, ed.laneTop(v.lane) + ed.laneH() + h/2, h, s.hears(v.base)}}
}

// hearAt is the lane whose badge is under a press, or "". src says which area
// the press was in, because the two draw their lanes in different y.
func (ed *cutEditor) hearAt(px, y float64, src bool) string {
	badges := ed.hearBadgesAud()
	if src {
		badges = ed.hearBadgesSrc()
	}
	for _, b := range badges {
		if math.Abs(px-b.cx) <= hearHit && math.Abs(y-b.cy) <= hearHit {
			return b.base
		}
	}
	return ""
}

// toggleHear turns one lane off or on for the scene the badges are about
// (hearScene): the held one, or the one under the line.
//
// A FRESH list every time, never an append into the one that is there: the undo
// snapshot copies the segment slice but not the strings inside it, so growing
// the held scene's list in place would silently rewrite every snapshot holding
// the same one -- and Undo would put back the state it was already in.
func (ed *cutEditor) toggleHear(base string) {
	s := ed.hearScene()
	if s == nil || base == "" {
		return
	}
	ed.pushUndo()
	var next []string
	for _, q := range s.Quiet {
		if q != base {
			next = append(next, q)
		}
	}
	on := len(next) != len(s.Quiet) // it was silent, and dropping it turns it on
	if !on {
		next = append(next, base)
	}
	s.Quiet = next
	ed.persist()
	ed.redrawTracks()
	word := "silent in"
	if on {
		word = "heard in"
	}
	ed.a.setStatus(fmt.Sprintf("%s is %s the scene at %s", base, word, mmss(s.S)))
}

// drawHearBadges paints them, and the held scene's own stretch of each lane
// behind them: green where it is heard, grey where it is not. The wash is the
// part that works at any zoom -- a scene too narrow for a badge still says what
// it does -- and the badge is the part you can press.
//
// Called from inside each area's translation, so x is timeline px.
func (ed *cutEditor) drawHearBadges(cr *cairo.Context, badges []hearBadge, vx0, vx1 float64) {
	s := ed.hearScene()
	if s == nil {
		return
	}
	x0, x1 := ed.xOf(s.S), ed.xOf(s.E)
	for _, b := range badges {
		if x1 < vx0 || x0 > vx1 {
			continue
		}
		if b.on {
			cr.SetSourceRGBA(0.2, 0.85, 0.35, 0.22)
		} else {
			cr.SetSourceRGBA(0.55, 0.55, 0.6, 0.3)
		}
		cr.Rectangle(x0, b.cy-b.h/2, x1-x0, b.h)
		cr.Fill()
	}
	for _, b := range badges {
		if b.cx < vx0-hearHit || b.cx > vx1+hearHit {
			continue
		}
		hearPlate(cr, b.cx, b.cy, b.on, ed.badgeHot(badgeID{kind: badgeHear, base: b.base}))
	}
}

// drawLaneSwitches paints the whole-lane switches in the gutter. Called from
// inside drawAudio's translated pass: the strip they stand in is the head of
// the tape, so they scroll with it (cut_gutter.go).
func (ed *cutEditor) drawLaneSwitches(cr *cairo.Context) {
	for _, s := range ed.laneSwitches() {
		hearPlate(cr, s.cx, s.cy, s.on, ed.badgeHot(badgeID{kind: badgeSwitch, base: s.base}))
	}
}

// hearPlate is the round plate a speaker is drawn on: lit where the sound is
// heard, dark where it is not. Both controls draw it, so the mark means one
// thing in both places -- a scene's badge and a lane's switch are the same
// question asked at two sizes.
func hearPlate(cr *cairo.Context, cx, cy float64, on, hot bool) {
	litPlate(cr, cx, cy, on, hot)
	drawSpeaker(cr, cx, cy, on)
}

// litPlate is the round plate a lane's or a camera's badge sits on: green when
// the thing is in use, dark when it is not. plate is the bare disc every badge
// on the page is drawn on (drawKillBadge, foldPlate use it too).
func litPlate(cr *cairo.Context, cx, cy float64, on, hot bool) {
	switch {
	case hot:
		hotPlate(cr, cx, cy, hearR+hearPad)
	case on:
		plate(cr, cx, cy, hearR+hearPad, 0.15, 0.65, 0.3, 0.95)
	default:
		plate(cr, cx, cy, hearR+hearPad, 0.06, 0.06, 0.07, 0.62)
	}
}

// hotPlate is the blue a plated control wears under the pointer -- one colour
// for all of them, so a badge that lights means the same thing wherever it is.
// The ✕ badges are the exception and light red (drawKillBadge): "this press
// removes it" is not the promise these make.
func hotPlate(cr *cairo.Context, cx, cy, r float64) {
	plate(cr, cx, cy, r, 0.25, 0.55, 0.85, 0.95)
}

func plate(cr *cairo.Context, cx, cy, r, R, G, B, A float64) {
	cr.SetSourceRGBA(R, G, B, A)
	cr.Arc(cx, cy, r, 0, 2*math.Pi)
	cr.Fill()
}

// drawSpeaker is the mark on the plate: a cone, with two arcs coming off it
// when the lane is heard and a stroke through it when it is not. A path, like
// every other mark on this page -- a glyph would be the font's idea of a
// speaker at 9 px, which on some machines is a box.
func drawSpeaker(cr *cairo.Context, cx, cy float64, on bool) {
	cr.SetSourceRGBA(1, 1, 1, 0.92)
	cr.SetLineWidth(1.4)
	// the cone: a small square with a horn opening to the right
	cr.MoveTo(cx-hearR, cy-hearR/2.5)
	cr.LineTo(cx-hearR/2.5, cy-hearR/2.5)
	cr.LineTo(cx+hearR/4, cy-hearR)
	cr.LineTo(cx+hearR/4, cy+hearR)
	cr.LineTo(cx-hearR/2.5, cy+hearR/2.5)
	cr.LineTo(cx-hearR, cy+hearR/2.5)
	cr.ClosePath()
	cr.Fill()
	if on {
		for _, r := range []float64{hearR * 0.65, hearR * 1.05} {
			cr.Arc(cx+hearR/4, cy, r, -0.9, 0.9)
			cr.Stroke()
		}
		return
	}
	cr.MoveTo(cx+hearR*0.55, cy-hearR*0.55)
	cr.LineTo(cx+hearR*1.25, cy+hearR*0.55)
	cr.MoveTo(cx+hearR*1.25, cy-hearR*0.55)
	cr.LineTo(cx+hearR*0.55, cy+hearR*0.55)
	cr.Stroke()
}

// migrateSound reads a cut from when sound was one lane for the whole project
// (cutFile.Sound) and writes the same thing per scene: every scene silences
// every lane but that one. Runs once -- nothing writes Sound any more; scenes
// that already name silenced lanes are left alone. The old choice could put
// camera A's sound under camera B's picture; those scenes keep their OWN camera
// (audible and correctable beats silent), and the note counts them.
func migrateSound(segs []cutSeg, snd string, vids []tlVideo, auds []tlAudio) ([]cutSeg, string) {
	if strings.TrimSpace(snd) == "" || len(segs) == 0 {
		return segs, ""
	}
	// the chosen name read as a camera ROW and not as one file: a camera
	// stopped and started again is several recordings in a line, and the old
	// choice carried across them exactly as the picture does. Below zero means
	// it named a separately recorded lane instead, which is one file and needs
	// none of this.
	row := -1
	for _, v := range vids {
		if v.base == snd {
			row = v.lane
		}
	}
	out := append([]cutSeg(nil), segs...)
	crossed := 0
	for i := range out {
		if len(out[i].Quiet) > 0 {
			continue
		}
		keep := snd
		if row >= 0 {
			switch v := pickVideoOn(vids, out[i].Cam, out[i].S); {
			case v == nil:
				keep = ""
			case v.lane != row:
				keep, crossed = v.base, crossed+1
			default:
				keep = v.base
			}
		}
		var quiet []string
		for _, au := range auds {
			if au.base != keep {
				quiet = append(quiet, au.base)
			}
		}
		out[i].Quiet = quiet
	}
	note := fmt.Sprintf("this cut was heard on %s from end to end; that is now said scene "+
		"by scene, and every scene has been set to silence the other lanes", snd)
	if crossed > 0 {
		were := "scenes are"
		if crossed == 1 {
			were = "scene is"
		}
		note += fmt.Sprintf(" — except %d, which %s shown from another camera and left "+
			"hearing their own sound: one camera's sound under another's picture is no "+
			"longer something the render can do", crossed, were)
	}
	return out, note
}

// syncHush tells the preview what the scene under the playhead hears, from
// showInsert -- every path that moves the line -- so a badge press is audible
// at once and playback picks the new answer up at the boundary. Off the ends
// of the cut and in gaps everything is heard.
func (ed *cutEditor) syncHush() {
	if ed.player == nil {
		return
	}
	var s *cutSeg
	if i := ed.segAt(ed.playhead); i >= 0 {
		s = &ed.segs[i]
	}
	var base string
	if ed.playVideo != nil {
		base = ed.playVideo.base
	}
	// and how long the answer holds: to the end of this scene, or, in a gap,
	// to the start of the next -- in the master's own seconds, which is the
	// clock the lanes are placed on. A lane started now stops there by
	// itself (auxAudio.stopAt), which is what keeps the first sound of the
	// next scene from being a tick's worth of the lane it silences.
	until := 0.0
	if ed.playVideo != nil {
		if s != nil {
			until = ed.playVideo.at(s.E)
		} else if _, next := gapAt(ed.segs, ed.playhead); next >= 0 {
			until = ed.playVideo.at(ed.segs[next].S)
		}
	}
	own, quiet := hushOf(s, base)
	ed.player.Hush(own, quiet, until)
}

// hushOf is what a scene does not hear, in the two pieces the preview keeps
// its sound in: whether the recording the picture comes from is silenced, and
// the scene's own list of silenced lanes. No scene, no answer: gaps and the
// ends of the cut are heard whole.
func hushOf(s *cutSeg, base string) (bool, []string) {
	if s == nil {
		return false, nil
	}
	return base != "" && laneQuiet(s.Quiet, base), s.Quiet
}

// Which camera a scene is shown from (cutSeg.Cam), said on the rows: a held
// scene wears a lens badge on every row that was rolling, lit on the row shown,
// at the same x as the speaker badges. A lens is a radio (one picture, never
// none) where a speaker is a checkbox. Rows filming nothing at those seconds
// get none: the render would fall back anyway (pickVideoOn).

// camBadge is one row's answer for the scene the badges are about: where it is
// drawn, in timeline x and area y, and whether the scene is shown from there.
type camBadge struct {
	row    int
	cx, cy float64
	on     bool
}

// camBadges is one per row that could show this scene: the rows that have
// footage under it. Empty when there is no scene in hand, when the scene is too
// narrow to wear a mark, or when one row is all there is -- a badge that is the
// only badge answers a question nobody can ask.
//
// The x is hearX's, so the picture's marks and the sound's stand in one column
// at the scene's left edge and read as one question asked of every row.
func (ed *cutEditor) camBadges() []camBadge {
	cx, ok := ed.hearX()
	if !ok || ed.laneN < 2 {
		return nil
	}
	s := ed.hearScene()
	var out []camBadge
	for r := 0; r < ed.laneN; r++ {
		if videoOn(ed.vids, r, s.S) == nil {
			continue // that camera was not rolling here
		}
		out = append(out, camBadge{r, cx, ed.laneTop(r) + ed.laneH()/2, r == s.Cam})
	}
	if len(out) < 2 {
		return nil // one answer is not a choice
	}
	return out
}

// camBadgeAt is the row whose badge is under a press, or -1.
func (ed *cutEditor) camBadgeAt(px, y float64) int {
	for _, b := range ed.camBadges() {
		if math.Abs(px-b.cx) <= hearHit && math.Abs(y-b.cy) <= hearHit {
			return b.row
		}
	}
	return -1
}

// setSegCam shows the scene from row r; seconds, silenced lanes and effects
// stay. It does not coalesce -- rearranging the list under a hand still on a
// badge is how a press becomes "what happened to my clip"; the next edit joins
// neighbours that now agree.
func (ed *cutEditor) setSegCam(r int) {
	s := ed.hearScene()
	if s == nil || r < 0 || r == s.Cam {
		return
	}
	ed.pushUndo()
	s.Cam = r
	ed.persist()
	ed.showInsert() // the preview is standing on a frame that came from the old row
	ed.redrawTracks()
	ed.a.setStatus(fmt.Sprintf("the scene at %s is shown from %s now", mmss(s.S), ed.camName(r)))
}

// drawCamBadges paints them over the pictures, inside drawTrack's translation.
// The wash under a badge says which row a scene is shown from at any zoom;
// only the row NOT showing it gets one -- the showing row has the green.
func (ed *cutEditor) drawCamBadges(cr *cairo.Context, vx0, vx1 float64) {
	badges := ed.camBadges()
	if len(badges) == 0 {
		return
	}
	s := ed.hearScene()
	x0, x1 := ed.xOf(s.S), ed.xOf(s.E)
	for _, b := range badges {
		if b.on || x1 < vx0 || x0 > vx1 {
			continue
		}
		cr.SetSourceRGBA(0.55, 0.55, 0.6, 0.22)
		cr.Rectangle(x0, ed.laneTop(b.row), x1-x0, ed.laneH())
		cr.Fill()
	}
	for _, b := range badges {
		if b.cx < vx0-hearHit || b.cx > vx1+hearHit {
			continue
		}
		camPlate(cr, b.cx, b.cy, b.on, ed.badgeHot(badgeID{kind: badgeCam, row: b.row}))
	}
}

// camPlate is the round plate a lens is drawn on: lit on the row the scene is
// shown from, dark on the rows it could be shown from instead. The plate is the
// speaker's, because both marks answer "is this row in this scene" and the page
// should say that once -- and what is ON the plate is what says which half of
// the question this is, and that a press here is a choice rather than a toggle.
func camPlate(cr *cairo.Context, cx, cy float64, on, hot bool) {
	litPlate(cr, cx, cy, on, hot)
	drawLens(cr, cx, cy, on)
}

// drawLens is the mark: a ring, filled on the row in use and hollow on the
// rest -- a radio button wearing a camera's face. A path rather than a glyph,
// like every other mark on this page.
func drawLens(cr *cairo.Context, cx, cy float64, on bool) {
	cr.SetSourceRGBA(1, 1, 1, 0.92)
	cr.SetLineWidth(1.4)
	cr.Arc(cx, cy, hearR*0.82, 0, 2*math.Pi)
	cr.Stroke()
	if !on {
		return
	}
	cr.Arc(cx, cy, hearR*0.4, 0, 2*math.Pi)
	cr.Fill()
}

// ---- the badge under the pointer ---------------------------------------------
//
// Every speaker and every lens lights under the pointer, like the rest of the
// plated controls on this page. One id rather than a field per control, and one
// hit test asking the press's own questions in the press's own order (cut.go):
// a hover that answered differently would light a badge the press does not act
// on.
type badgeID struct {
	kind int
	base string // the recording a speaker is about
	row  int    // the row a lens is about
}

const (
	badgeNone   = iota
	badgeSwitch // a recording's whole-lane switch, in the band's gutter
	badgePair   // a camera row's, in the picture band's
	badgeHear   // one lane's badge on the scene the badges are about
	badgeCam    // the lens that says which camera a scene is shown from
)

// badgeAt is the badge under a point in timeline px, or none. src says which
// band: the two carry different controls at the same x.
func (ed *cutEditor) badgeAt(px, y float64, src bool) badgeID {
	if src {
		if bases := ed.pairSwitchAt(px, y); len(bases) > 0 {
			return badgeID{kind: badgePair, base: bases[0]}
		}
	} else if base := ed.laneSwitchAt(px, y); base != "" {
		return badgeID{kind: badgeSwitch, base: base}
	}
	if base := ed.hearAt(px, y, src); base != "" {
		return badgeID{kind: badgeHear, base: base}
	}
	if src {
		if r := ed.camBadgeAt(px, y); r >= 0 {
			return badgeID{kind: badgeCam, row: r}
		}
	}
	return badgeID{}
}

// badgeHot is whether b is the badge under the pointer.
func (ed *cutEditor) badgeHot(b badgeID) bool { return ed.badgeHov == b }

// hoverBadges lights the one under the pointer. x below zero means the pointer
// has left the band, which puts them all out: the two bands share the field
// because only one of them has the pointer.
func (ed *cutEditor) hoverBadges(x, y float64, src bool) {
	var b badgeID
	if x >= 0 {
		b = ed.badgeAt(x+ed.viewX, y, src)
	}
	if b == ed.badgeHov {
		return
	}
	ed.badgeHov = b
	if ed.srcArea != nil {
		ed.srcArea.QueueDraw()
	}
	if ed.audArea != nil {
		ed.audArea.QueueDraw()
	}
}
