package main

// The spare: a second video pipeline, prerolled at the next cut.
//
// A cut in the preview is a jump of the line to somewhere else -- the next
// clip after a removed stretch, the next join in a review, the next recording
// when this one runs out -- and until now every jump onto another file was
// PlaySegment: the pipeline to READY, a new uri, a preroll, a seek, and only
// then PLAYING, some three hundred milliseconds in which the picture froze
// and the sound stopped. A jump within the file was a flushing seek, which is
// quicker but still a stumble. That is what a cut looked like, and it is not
// what the render makes.
//
// So the editor says where the line is going a few seconds before it goes
// there (cut_preload.go), and this pipeline is taken to PAUSED at exactly
// that file and second -- decoded, prerolled, first frame ready, sound
// buffered. When the jump comes, PlaySegment finds it ready and SWAPS: the
// spare becomes the pipeline playing and the old one becomes the spare, the
// picture is pointed at the new paintable, and PLAYING on a prerolled
// pipeline is immediate. The join plays through like the join it will be.
//
// One spare, not one per row: the line goes one place next. Built on the
// first Preload, so only the player that jumps -- the cut editor's -- ever
// has one.

import (
	"math"

	"github.com/diamondburned/gotk4/pkg/gdk/v4"
	"github.com/go-gst/go-gst/pkg/gst"
)

// preloadTol is how close the prerolled second has to be to the one asked
// for. The editor computes both from the same numbers, so this is float
// slack, not a search radius.
const preloadTol = 0.01

type spare struct {
	pb, gain gst.Element
	video    gdk.Paintabler
	loaded   string  // the file it holds, or "" for nothing
	at       float64 // the file second it was asked to preroll at
	pend     int64   // the seek still to make once the uri has prerolled, ns; <0 none
	ready    bool    // prerolled AT at: the seek has landed
	seekRate float64 // the rate that seek carried
}

// Preload asks for the spare to be prerolled at a second of a file. Cheap to
// ask on every tick: already there, or on its way there, is nothing to do.
func (p *Player) Preload(file string, start float64) {
	if p.spare == nil {
		pb, gain, video, err := videoPipe("spare")
		if err != nil {
			return // no spare: every cut is a reload, as it was
		}
		p.spare = &spare{pb: pb, gain: gain, video: video, pend: -1}
		p.watch(pb)
	}
	s := p.spare
	if s.loaded == file && math.Abs(s.at-start) < preloadTol {
		return
	}
	s.pb.SetState(gst.StateReady)
	s.pb.SetObjectProperty("uri", fileURI(file))
	s.loaded, s.at, s.ready = file, start, false
	s.pend = int64(start * 1e9)
	s.seekRate = p.rate
	// silent while it waits: a paused pipeline makes no sound, but the
	// moment it is swapped in it must be at the player's loudness, and that
	// is written on the way in (swapIn)
	s.pb.SetState(gst.StatePaused)
}

// Preloaded is whether a PlaySegment for this file and second would be a
// swap rather than a reload -- what setPlayhead asks before choosing an
// in-place seek, which the swap beats.
func (p *Player) Preloaded(file string, start float64) bool {
	s := p.spare
	return s != nil && s.ready && s.loaded == file && math.Abs(s.at-start) < preloadTol
}

// swapIn is PlaySegment's first question: is the spare sitting at exactly
// this? If so the two pipelines change places and the answer is yes. A
// segment with a stop is never swapped in -- the spare prerolls with none,
// and the cut editor never asks for one.
func (p *Player) swapIn(file string, start, stop float64, play bool) bool {
	s := p.spare
	if stop > 0 || !p.Preloaded(file, start) {
		return false
	}
	p.pb, s.pb = s.pb, p.pb
	p.gain, s.gain = s.gain, p.gain
	p.video, s.video = s.video, p.video
	// the old pipeline is the spare now: parked, holding nothing
	s.loaded, s.ready, s.pend = "", false, -1
	s.pb.SetState(gst.StateReady)
	p.loaded = file
	p.pendStart, p.pendStop, p.pendPlay = -1, -1, false
	p.ended = false
	p.lastStart, p.lastStop = start, stop
	p.seekRate = s.seekRate
	if !p.still {
		p.Picture.SetPaintable(p.video) // a card over the picture stays; ShowVideo finds the new one
	}
	setGain(p.gain, p.pb, p.vol(), p.ownMute)
	if math.Abs(p.rate-p.seekRate) > 1e-6 {
		p.SeekTo(start) // prerolled at the old clock: one seek in place puts it on the new
	}
	if play {
		p.pb.SetState(gst.StatePlaying)
		p.setPlaying(true)
	} else {
		p.setPlaying(false)
	}
	for _, a := range p.mix {
		a.cue(start, play, p.rate, p.stopFor(a))
	}
	return true
}

// dropSpare forgets what the spare holds. The pipeline stays: it is reused by
// the next Preload.
func (p *Player) dropSpare() {
	if s := p.spare; s != nil {
		s.loaded, s.ready, s.pend = "", false, -1
		s.pb.SetState(gst.StateReady)
	}
}

// msg is the spare's bus: the uri has prerolled, so the seek is made; the
// seek has prerolled, so it is ready. An error is a spare that holds nothing
// -- the cut will be a reload, which will then say what is wrong with the
// file in the usual place.
func (s *spare) msg(msg *gst.Message) {
	switch msg.Type() {
	case gst.MessageAsyncDone:
		if s.pend >= 0 {
			at := s.pend
			s.pend = -1
			s.pb.Seek(s.seekRate, gst.FormatTime,
				gst.SeekFlagFlush|gst.SeekFlagAccurate,
				gst.SeekTypeSet, at, gst.SeekTypeNone, 0)
			return
		}
		if s.loaded != "" {
			s.ready = true
		}
	case gst.MessageError:
		s.loaded, s.ready, s.pend = "", false, -1
	}
}
