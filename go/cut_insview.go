package main

// Showing an insert in the preview while the playhead is inside it, with the
// insert's sound and the session hushed -- the cut the render makes. Over the
// footage: the stream keeps running underneath. Between the footage: the stream
// is HELD at the split point, the card plays at its own speed, the footage goes
// on from the same frame. A card in a gap stands still. Frames come from
// ffmpeg, the same renderer Produce uses.

import (
	"bytes"
	"fmt"
	"math"
	"os"
	"os/exec"
	"strings"
	"time"

	"github.com/diamondburned/gotk4/pkg/gdk/v4"
	"github.com/diamondburned/gotk4/pkg/gdkpixbuf/v2"
	"github.com/diamondburned/gotk4/pkg/glib/v2"
)

const (
	// frames per second of a card's own time. Not the render's rate: this is a
	// preview of a four-second card on a widget the size of a postcard, and
	// eight is enough to see a wipe happen while being little enough work that
	// the picture keeps up with the playhead.
	insPreviewFPS = 8.0
	// how wide a preview frame is rendered. The pane is never bigger, and a
	// 1920-wide card decoded ten times a second is memory spent on pixels that
	// are thrown away in the scale to the widget.
	insPreviewW = 960
	// frames kept as textures before the lot is dropped. A minute-long insert at
	// eight a second would otherwise be half a gigabyte of pictures nobody is
	// looking at any more; dropping all of them costs the next pass through the
	// card a re-render, which is what the first pass cost anyway.
	insFilmMax = 48
)

// insFilm is one insert's pictures, rendered as the playhead asks for them.
//
// Rendering happens off the GTK thread and the result comes back through an
// idle, so a card that takes 80 ms to draw slows the card down and nothing else.
// One frame is in flight at a time: the playhead moves ten times a second and
// the renderer is slower than that, and a queue of frames for moments already
// gone by is how a preview ends up lagging further behind the longer it runs.
type insFilm struct {
	ins    string // the insert path these belong to, parameters and all
	fps    float64
	tex    map[int]*gdk.Texture
	busy   bool
	shown  int  // frame currently on the picture, -1 for none
	failed bool // it cannot be drawn; said once, then left alone
}

// insertAt is the insert session time t falls inside, and how far into the
// card's own time that is; nil on footage. A spliced card owns no session time,
// so its marker (spliceSpan) is what the playhead sweeps -- the same picture
// the eye reads, 1:1 at a zoom where the marker is the card's own length.
func (ed *cutEditor) insertAt(t float64) (*cutSeg, float64) {
	for i := range ed.segs {
		s := &ed.segs[i]
		switch {
		case !s.isInsert():
		case s.spliced():
			x0, x1 := ed.spliceSpan(*s)
			w := (x1 - x0) / math.Max(ed.pps, 0.001)
			if t >= s.S && t < s.S+w {
				return s, (t - s.S) / w * s.Dur
			}
		case t >= s.S && t < s.E:
			return s, t - s.S
		}
	}
	return nil, 0
}

// insHold is the footage stopped while a spliced card plays through.
//
// A spliced card owns no session time -- it is a point where the footage is cut
// open -- so there is no stretch of the stream for it to play "during". What
// there is instead is this: the player paused where the cut is, a card running
// on the wall clock for as long as it was given, and then the footage let go
// again at the same frame. Watched end to end that is what the render makes.
type insHold struct {
	on    bool
	seg   cutSeg
	start time.Time
	// this card has just played through and the line has not left it yet. The
	// marker is still under the playhead and insertAt still answers with the
	// card; without this the hold would start again the moment it ended, which
	// is a preview that never gets past the first insert.
	done bool
}

// splicedCrossed is the spliced card playback has just run into: the first
// whose split point lies in (from, to]. The split point, not the marker (which
// widens with zoom), is the trigger; and only the distance one tick covers
// counts, so a seek past a card does not play it.
func (ed *cutEditor) splicedCrossed(from, to float64) *cutSeg {
	if to <= from || to-from > 1.0 {
		return nil
	}
	var best *cutSeg
	for i := range ed.segs {
		s := &ed.segs[i]
		if !s.spliced() || s.S <= from || s.S > to {
			continue
		}
		if best == nil || s.S < best.S {
			best = s
		}
	}
	return best
}

// startHold stops the footage and starts the card. Whatever the transport button
// says for the next few seconds, the page is not idle: the card is playing, and
// endHold gives the footage back.
func (ed *cutEditor) startHold(s *cutSeg) {
	if ed.player == nil {
		return
	}
	ed.player.Pause()
	ed.hold = insHold{on: true, seg: *s, start: time.Now()}
	ed.showInsert()
	if ed.a != nil {
		ed.a.setStatus(fmt.Sprintf("%s — the footage is held while it plays", insBase(s.Ins)))
	}
}

// tickHold advances a card that is holding the footage, and ends the hold when
// the card has run its length. Called from the same timer that follows playback,
// because while the hold is on there is no playback to follow.
func (ed *cutEditor) tickHold() {
	switch {
	case !ed.hold.on:
	case ed.player == nil || ed.player.playing:
		// somebody pressed ▶ while the card was up; the footage has taken itself
		// back and the card is over
		ed.endHold(false)
	case ed.holdInto(time.Now()) >= ed.hold.seg.Dur:
		ed.endHold(true)
	default:
		ed.showInsert()
	}
}

// holdInto is how far into the held card we are, in the card's own seconds,
// never past its end -- the last frame is the one it finishes on.
func (ed *cutEditor) holdInto(now time.Time) float64 {
	return math.Min(now.Sub(ed.hold.start).Seconds(), ed.hold.seg.Dur)
}

// endHold takes the card away and, if it played through, sets the footage going
// again from the frame it was stopped at. done marks the card so that the line
// standing on its marker does not start it over.
func (ed *cutEditor) endHold(done bool) {
	if !ed.hold.on {
		return
	}
	ed.hold = insHold{seg: ed.hold.seg, done: done}
	ed.showInsert() // the footage, its sound, and no card
	// ...and on again from the frame it stopped at. Not a stream that had
	// already run out under the card: Toggle would take that for "play it
	// again" and start the whole clip over.
	if done && ed.player != nil && !ed.player.playing && !ed.player.ended {
		ed.player.Toggle()
	}
}

// cancelHold drops a hold without giving the footage back, for the paths that
// have already decided where the preview should be: a click on the timeline, a
// frame step, an edit. showInsert settles the picture and the sound afterwards.
func (ed *cutEditor) cancelHold() {
	ed.hold = insHold{}
}

// cardNow is the card the preview owes the screen and how far into it. Not the
// same question as insertAt: while a card holds the footage the answer comes
// from the wall clock, and a card that has just played through is not shown
// again merely because the line is still standing on its marker.
func (ed *cutEditor) cardNow() (*cutSeg, float64) {
	if ed.hold.on {
		s := ed.hold.seg
		return &s, ed.holdInto(time.Now())
	}
	s, into := ed.insertAt(ed.playhead)
	if s != nil && s.spliced() && ed.hold.done &&
		s.S == ed.hold.seg.S && s.Ins == ed.hold.seg.Ins {
		return nil, 0
	}
	if s == nil || !s.spliced() {
		ed.hold.done = false // the line is off the card it played
	}
	return s, into
}

// cardSound: what the preview plays while a card is up. The session goes quiet;
// a video insert's own sound is cued once when it comes up and left on its own
// clock (seeking it 10×/s against an 8 fps preview would stutter).
// cardHush: whether the session's sound is silenced while s is up. Only an
// insert covering the picture alone leaves the recording playing (soundUnder),
// as the render does.
func cardHush(s *cutSeg) bool { return s != nil && !s.keepsSoundUnder() }

func (ed *cutEditor) cardSound(s *cutSeg, into float64) {
	ed.player.SetMuted(cardHush(s) || fxHush(ed.fx, ed.playhead))
	want := ed.cardVoice(s)
	if want == ed.cardSnd {
		return
	}
	ed.cardSnd = want
	ed.player.CardSound(want, ed.cardInto(s, into), want != "")
}

// cardInto is how far into its own FILE a card is when the playhead is `into`
// seconds into the card. The two are the same number for a file chosen from
// disk, which plays from its top, and not for anything copied out of the
// session: copied footage starts at the second it was copied from, and copied
// sound at the second Ss names in its recording. The render reads the same two
// offsets (produceSegs for the footage, -ss for the sound), so what is heard
// scrubbing through one is what the finished video has.
func (ed *cutEditor) cardInto(s *cutSeg, into float64) float64 {
	if s == nil {
		return into
	}
	if from, ok := copySrc(s.Ins); ok {
		if v := ed.videoAt(from); v != nil {
			into += v.at(from)
		}
	}
	return into + s.Ss
}

// running is whether the preview is playing anything at all: the footage, or a
// card holding it still. Sound follows this and nothing else -- a parked preview
// is silent, and a card sitting under a parked playhead is a picture.
func (ed *cutEditor) running() bool {
	return ed.hold.on || (ed.player != nil && ed.player.Playing())
}

// cardVoice is the file whose own sound belongs on the preview while s is up:
// an inserted video's audio, an inserted sound file itself, and nothing at all
// for a still or a drawn card, which have none. The session's sound is not a candidate -- that is the thing
// being cut.
func (ed *cutEditor) cardVoice(s *cutSeg) string {
	if s == nil || !ed.running() {
		return ""
	}
	// an insert that brings no sound of its own has no voice either way it is
	// read: spliced it is silent, and over the picture alone what is heard is
	// the session's, which cardSound has just left unmuted. Asked before the
	// copy below and not after it, because a stretch of footage pasted from a
	// picture-alone selection IS a copy, and the copy's own answer would hand
	// back the very recording the paste was taken without.
	if s.Mute {
		return ""
	}
	// a copy's sound is its recording's own, from the copied seconds on
	if from, ok := copySrc(s.Ins); ok {
		if v := ed.videoAt(from); v != nil {
			return v.path
		}
		return ""
	}
	if k := insKind(s.Ins); k != "video" && k != "audio" {
		return ""
	}
	file, _ := insSplit(s.Ins)
	return ed.a.loadPath(file)
}

// showInsert puts the card under the playhead on the preview, or takes it away
// again. Cheap enough to call from every path that moves the playhead, which is
// how it is called: a click, a frame step, playback following its own clock, and
// dropping the insert in the first place.
func (ed *cutEditor) showInsert() {
	if ed.player == nil || ed.a == nil {
		return
	}
	// the stop effect's still rides its own overlay layer over whatever this
	// settles on, so every path that moves the playhead settles both at once
	defer ed.syncFxStill()
	// which lanes this scene is heard on, likewise: the card's answer about the
	// session's sound is all-or-nothing (cardSound below), the scene's is lane
	// by lane, and both are true at once
	ed.syncHush()
	s, into := ed.cardNow()
	ed.cardSound(s, into)
	if s == nil || s.audioIns() {
		// no card, or a sound-only one: the picture stays the session's, and cardSound
		// has routed the file's sound. Unless standing still on a row with nothing
		// under the line: the pipeline holds some OTHER row's frame (videoAt falls
		// back), and black is what is true. Only at a standstill -- in playback the
		// running pipeline is the clock.
		if !ed.player.playing && ed.videoShown(ed.playhead) == nil {
			ed.player.ShowStill(blackStill())
		} else {
			ed.player.ShowVideo()
		}
		if ed.film != nil {
			ed.film.shown = -1 // whatever it was showing is off the picture now
		}
		return
	}
	if ed.film == nil || ed.film.ins != s.Ins {
		ed.film = ed.a.newFilm(s.Ins)
	}
	f := ed.film
	if f.failed {
		return
	}
	i := 0
	if f.fps > 0 {
		i = max(0, int(into*f.fps))
	}
	if tex := f.tex[i]; tex != nil {
		if f.shown != i || !ed.player.still {
			ed.player.ShowStill(tex)
			f.shown = i
		}
		return
	}
	// The frame for this instant is not drawn yet. Showing nothing here is not
	// neutral: it leaves the paused FOOTAGE on screen, and an animated card
	// whose rendering runs slower than its own clock -- which is the usual case,
	// a frame takes longer to draw than 1/8th of a second -- then never catches
	// up with the playhead and never appears at all. So the nearest frame that
	// IS drawn goes up while this one is asked for: a card a little behind its
	// clock, rather than no card.
	if j, tex := f.nearest(i); tex != nil {
		if f.shown != j || !ed.player.still {
			ed.player.ShowStill(tex)
			f.shown = j
		}
	}
	ed.renderFrame(f, i)
}

// blackTex is built once and kept: the "nothing here" frame never changes, and
// showInsert puts it up on every playhead move across an empty stretch.
var blackTex *gdk.Texture

// blackStill is the picture for a row with no footage under the line: plain
// black, in the video's shape so the letterboxing does not jump when real
// footage comes back. 16x9 pixels is enough -- every pixel is the same one.
func blackStill() *gdk.Texture {
	if blackTex == nil {
		pb := gdkpixbuf.NewPixbuf(gdkpixbuf.ColorspaceRGB, false, 8, 16, 9)
		pb.Fill(0x000000ff)
		blackTex = gdk.NewTextureForPixbuf(pb)
	}
	return blackTex
}

// nearest is the rendered frame closest to i, preferring the one behind it: a
// card seen slightly earlier in its run reads as the card arriving, one seen
// ahead of its run is a jump backwards when the real frame lands.
func (f *insFilm) nearest(i int) (int, *gdk.Texture) {
	best, d := -1, 0
	for j := range f.tex {
		dj := i - j
		if dj < 0 {
			dj = (j - i) * 2 // ahead of the clock costs double
		}
		if best < 0 || dj < d {
			best, d = j, dj
		}
	}
	if best < 0 {
		return -1, nil
	}
	return best, f.tex[best]
}

// newFilm decides how an insert moves, which is the only thing about it this
// page needs to know in advance: a still and a card that does not animate are
// one picture held for the whole slot, everything else is a frame per moment.
func (a *App) newFilm(ins string) *insFilm {
	f := &insFilm{ins: ins, tex: map[int]*gdk.Texture{}, shown: -1}
	if _, ok := copySrc(ins); ok {
		f.fps = insPreviewFPS // footage moves; drawn at the card rate like a sting
		return f
	}
	switch insKind(ins) {
	case "video":
		f.fps = insPreviewFPS
	case "svg":
		file, q := insSplit(ins)
		if src, _, err := insSVG(a.loadPath(file) + q.suffix()); err == nil && svgAnimated(src) {
			f.fps = insPreviewFPS
		}
	}
	return f
}

// renderFrame draws one frame in the background and puts it up when it arrives
// -- by asking showInsert again rather than by showing it directly, because by
// then the playhead may be somewhere else entirely and the frame that was asked
// for is no longer the frame that belongs on screen.
func (ed *cutEditor) renderFrame(f *insFilm, i int) {
	if f.busy {
		return
	}
	f.busy = true
	a, ins := ed.a, f.ins
	at := 0.0
	if f.fps > 0 {
		at = float64(i) / f.fps
	}
	go func() {
		png, file, err := a.insPNG(ins, at)
		glib.IdleAdd(func() {
			f.busy = false
			if ed.film != f {
				return // a different insert is on the playhead now
			}
			tex, terr := insTexture(png, file)
			if err == nil {
				err = terr
			}
			if err != nil {
				f.failed = true
				a.logf(">>> %s cannot be shown in the preview: %v", insBase(ins), err)
				return
			}
			if len(f.tex) >= insFilmMax {
				f.tex = map[int]*gdk.Texture{}
				f.shown = -1
			}
			f.tex[i] = tex
			ed.showInsert()
		})
	}()
}

// insTexture turns what the renderer produced into something GtkPicture can
// draw: the PNG it rendered, or -- for a file that is already a picture -- the
// file itself, which lets GTK's own loaders deal with a webp or a gif rather
// than this decoding one to re-encode it as the other.
func insTexture(png []byte, file string) (*gdk.Texture, error) {
	if len(png) > 0 {
		return gdk.NewTextureFromBytes(glib.NewBytes(png))
	}
	if file == "" {
		return nil, fmt.Errorf("nothing came out of the renderer")
	}
	return gdk.NewTextureFromFilename(file)
}

// insPNG renders an insert at one moment of its own time. It returns either the
// picture, or the path of a file the caller should open directly.
//
// at is seconds into the insert, and it is what makes a preview of an animated
// card worth having: the same document, evaluated at the same moment svganim
// would bake it at, so a wipe that has not finished by the time the slot ends is
// visible here rather than in the finished video.
func (a *App) insPNG(ins string, at float64) (png []byte, file string, err error) {
	// a copy has no file of its own: its frames are its recording's, at the
	// copied seconds -- the same cut the render makes, one frame at a time
	if from, ok := copySrc(ins); ok {
		v := a.ed.videoAt(from + at)
		if v == nil {
			return nil, "", fmt.Errorf("the copied footage falls in no recording")
		}
		out, err := ffmpegPNG("-ss", fmt.Sprintf("%.3f", v.at(from+at)), "-i", v.path)
		return out, "", err
	}
	src, q := insSplit(ins)
	path := a.loadPath(src)
	switch insKind(ins) {
	case "still":
		return nil, path, nil

	case "video":
		out, err := ffmpegPNG("-ss", fmt.Sprintf("%.3f", at), "-i", path)
		return out, "", err
	}

	// an svg: the card with its parameters applied, and, if it moves, at the
	// moment the playhead is inside it. Same two calls Produce makes (insSVG,
	// renderAt), so the preview cannot disagree with the render about what the
	// card says.
	doc, _, err := insSVG(path + q.suffix())
	if err != nil {
		return nil, "", err
	}
	if svgAnimated(doc) {
		root, err := parseSVG(doc)
		if err != nil {
			return nil, "", err
		}
		var b strings.Builder
		b.WriteString(`<?xml version="1.0" encoding="UTF-8"?>` + "\n")
		root.renderAt(&b, at)
		doc = []byte(b.String())
	}
	// through a file rather than a pipe: ffmpeg's svg decoder wants to seek in
	// what it is reading, and a card is a few kilobytes
	tmp, err := os.CreateTemp("", "naivepost-card-*.svg")
	if err != nil {
		return nil, "", err
	}
	defer os.Remove(tmp.Name())
	if _, err := tmp.Write(doc); err != nil {
		tmp.Close()
		return nil, "", err
	}
	if err := tmp.Close(); err != nil {
		return nil, "", err
	}
	out, err := ffmpegPNG("-i", tmp.Name())
	return out, "", err
}

// ffmpegPNG is one frame of whatever the input arguments name, as PNG bytes on
// stdout. Scaled down but never up: a card is drawn at 1920 and a still may be
// anything, and blowing a small picture up here would only make the widget scale
// it back down with fewer pixels to work from.
func ffmpegPNG(in ...string) ([]byte, error) {
	args := append([]string{"-v", "error", "-nostdin"}, in...)
	args = append(args, "-frames:v", "1",
		"-vf", fmt.Sprintf("scale='min(iw,%d)':-2", insPreviewW),
		"-f", "image2", "-c:v", "png", "-")
	cmd := exec.Command(ffTool("ffmpeg"), args...)
	var out, errb bytes.Buffer
	cmd.Stdout, cmd.Stderr = &out, &errb
	if err := cmd.Run(); err != nil {
		tail := strings.TrimSpace(errb.String())
		if len(tail) > 200 {
			tail = tail[len(tail)-200:]
		}
		return nil, fmt.Errorf("ffmpeg: %w: %s", err, tail)
	}
	if out.Len() == 0 {
		return nil, fmt.Errorf("ffmpeg drew nothing")
	}
	return out.Bytes(), nil
}
