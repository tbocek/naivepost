package main

// Video preview: GStreamer playbin3 into gtk4paintablesink, shown by a
// GtkPicture (Arch ships no GTK media backend). go-gst's objects and gotk4's
// are different Go wrappers around the same C GObjects, so the paintable
// crosses as a raw pointer: taken out of go-gst, ref'd into gotk4's world.

import (
	"fmt"
	"math"
	"strings"
	"time"

	coreglib "github.com/diamondburned/gotk4/pkg/core/glib"
	"github.com/diamondburned/gotk4/pkg/gdk/v4"
	"github.com/diamondburned/gotk4/pkg/gtk/v4"
	gobject "github.com/go-gst/go-glib/pkg/gobject/v2"
	"github.com/go-gst/go-gst/pkg/gst"
)

type Player struct {
	pb      gst.Element
	Picture *gtk.Picture

	// OnState is called whenever playback starts or stops, including the stream
	// ending on its own -- the run bar draws ▶ or ⏸ from this, and the end of a
	// clip is a change it has no other way to hear about. Called on the GTK
	// thread: every state change here happens either on it or on the bus watch,
	// which dispatches there.
	OnState func()

	// OnError receives GStreamer's own words when the pipeline gives up on a
	// file. These used to go to stdout, which for a GUI started from a launcher
	// is nowhere at all: a wav the decoder refused looked exactly like a wav
	// playing silently, and the transport button sat there claiming ⏸. Also on
	// the GTK thread -- the bus watch dispatches there.
	OnError func(string)
	// OnLog is the app's log, for what the mix does: which lanes it built under
	// which file, and a lane whose pipeline failed. Optional, like OnError.
	OnLog func(string)
	// our volume element inside pb's audio path (audioFilter): the app's gain
	// and mute live here, never on the server's stream volume
	gain gst.Element
	// until is when the scene's hush answer expires: the master-file second
	// the scene under the line ends at (Hush); 0 for never
	until float64
	// whether the last applyVol wrote a gain of nought, so it is said once
	volZero bool
	// when the last flushing seek-for-a-rate went out (SetRateNow)
	rateSeekAt time.Time
	// the sink's own paintable, kept so that a picture put over the video (an
	// insert; see ShowStill) can be taken away again
	video   gdk.Paintabler
	still   bool
	loaded  string // file currently cued, so a caller can seek instead of reload
	playing bool
	// the separate recordings heard under this file, each its own pipeline
	// (SetMix). Empty for every player but the cut editor's.
	mix []*auxAudio
	// the session's sound is off: the picture is not the session's. Set while a
	// card is on the preview, where hearing the footage carry on underneath is
	// how an insert ends up sounding exactly like the overwrite it is not.
	muted bool
	// the lanes the scene under the playhead does not hear (Hush): hushOwn for
	// the footage's own sound, and a name for each recording mixed under it.
	// Separate from muted because they answer different questions -- muted is
	// "none of this is the session", hush is "this much of the session" -- and
	// a scene can silence the camera while still hearing the microphone.
	hushOwn bool
	hush    map[string]bool
	// what was last written to each pipeline's mute property, so the ten calls
	// a second this is asked from settle into nothing when nothing changed
	ownMute bool
	// an inserted video's own sound, its own pipeline for the same reason the
	// recordings have theirs. Empty unless a video insert is on screen.
	card     *auxAudio
	cardFile string
	// the stream ran to its end and is sitting on its last frame. Resuming
	// there plays nothing, so ▶ starts the same segment over instead.
	ended               bool
	lastStart, lastStop float64
	// pending segment, applied once the new uri has prerolled
	pendStart, pendStop int64 // nanoseconds; pendStart < 0 = nothing pending
	pendPlay            bool  // false = preroll only, show the first frame paused
	// a second pipeline prerolled at wherever the line is about to be put
	// next, so a cut is a swap and not a reload (Preload, player_spare.go).
	// Built on the first Preload, so a player that never jumps -- Narrate's,
	// the voice sample's -- never carries one.
	spare *spare
	// the clock the stream runs on: 1 is the footage's own, and a cut with
	// speed effects in it moves this as the line crosses them (SetRate). Every
	// seek in this file carries it, the separate recordings' seeks included,
	// or the sound would walk away from the picture at the first slow stretch.
	rate float64
	// the rate the master pipeline is ACTUALLY running at, which is whatever
	// the last seek carried. A rate set while paused does not reach the
	// pipeline until something seeks, and ▶ on its own does not seek -- so
	// resuming has to notice the two have drifted apart and put that right.
	seekRate float64
	// the volume effects' say over this pipeline: the gain the cut asks for at
	// the second under the playhead (fxGainAt), 1 where no volume effect
	// covers it. Kept per player because it is a property of what that player
	// is showing, unlike previewVol below, which is a property of the room.
	// The two are multiplied together at every write (applyVol).
	fxGain float64
}

func NewPlayer() (*Player, error) {
	gst.Init()

	pb, gain, paintable, err := videoPipe("main")
	if err != nil {
		return nil, err
	}

	pic := gtk.NewPicture()
	pic.SetPaintable(paintable)
	pic.SetContentFit(gtk.ContentFitContain)

	p := &Player{pb: pb, gain: gain, Picture: pic, video: paintable, pendStart: -1, pendStop: -1,
		rate: 1, seekRate: 1, fxGain: 1}
	// born at the volume the sliders say, like every pipeline in the app --
	// and on the roll it visits when one of them moves
	p.applyVol()
	allPlayers = append(allPlayers, p)
	p.watch(pb)
	return p, nil
}

// videoPipe builds one video pipeline: playbin3 into a gtk4paintablesink,
// with the app's own volume element in its audio path. Its paintable is what
// a GtkPicture shows. Built twice per player that jumps -- the one playing
// and the spare prerolling the next cut -- and once for every other.
func videoPipe(name string) (pb, gain gst.Element, paintable gdk.Paintabler, err error) {
	pb = gst.ElementFactoryMake("playbin3", name)
	if pb == nil {
		return nil, nil, nil, fmt.Errorf("playbin3 not available (gst-plugins-base)")
	}
	sink := gst.ElementFactoryMake("gtk4paintablesink", name+"sink")
	if sink == nil {
		return nil, nil, nil, fmt.Errorf("gtk4paintablesink not available (pacman -S gst-plugin-gtk4)")
	}
	pb.SetObjectProperty("video-sink", sink)

	pv := sink.ObjectProperty("paintable")
	gobj, ok := pv.(gobject.Object)
	if !ok {
		return nil, nil, nil, fmt.Errorf("paintable property is %T, expected a gobject.Object", pv)
	}
	// ToGlibNone hands out the pointer without a transfer; Take refs it into
	// gotk4's world, so both wrappers legitimately co-own the C object.
	paintable = &gdk.Paintable{Object: coreglib.Take(gobject.UnsafeObjectToGlibNone(gobj))}

	filter, gain := audioFilter(name)
	pb.SetObjectProperty("audio-filter", filter)
	resetStreamVolume(pb)
	return pb, gain, paintable, nil
}

// watch puts the bus watch on one of this player's video pipelines. The
// signal watch dispatches on the default main context, i.e. the GTK loop.
// One handler for both pipelines, because which of them is the one playing
// changes at every swap (PlaySegment): the message is read for whichever
// role its pipeline has NOW.
func (p *Player) watch(pb gst.Element) {
	bus := pb.GetBus()
	bus.AddSignalWatch()
	bus.ConnectMessage(func(_ gst.Bus, msg *gst.Message) {
		if p.pb == pb {
			p.mainMsg(msg)
		} else if s := p.spare; s != nil && s.pb == pb {
			s.msg(msg)
		}
	})
}

// mainMsg is the playing pipeline's bus.
func (p *Player) mainMsg(msg *gst.Message) {
	switch msg.Type() {
	case gst.MessageAsyncDone:
		if p.pendStart >= 0 {
			start, stop := p.pendStart, p.pendStop
			p.pendStart, p.pendStop = -1, -1
			p.seekRate = p.rate
			if stop > 0 {
				p.pb.Seek(p.rate, gst.FormatTime,
					gst.SeekFlagFlush|gst.SeekFlagAccurate,
					gst.SeekTypeSet, start, gst.SeekTypeSet, stop)
			} else {
				p.pb.Seek(p.rate, gst.FormatTime,
					gst.SeekFlagFlush|gst.SeekFlagAccurate,
					gst.SeekTypeSet, start, gst.SeekTypeNone, 0)
			}
			if p.pendPlay {
				p.pb.SetState(gst.StatePlaying)
				p.setPlaying(true)
			}
		}
	case gst.MessageEOS:
		// freeze on the last frame instead of tearing the stream down
		p.pb.SetState(gst.StatePaused)
		p.ended = true
		p.setPlaying(false)
	case gst.MessageError:
		errMsg, _ := msg.ParseError()
		// the pipeline is done for; say so, and stop drawing ⏸ over a
		// stream that has stopped
		p.setPlaying(false)
		if p.OnError != nil {
			p.OnError(fmt.Sprint(errMsg))
			return
		}
		fmt.Println("gst error:", errMsg)
	}
}

// audioFilter is every pipeline's audio path: scaletempo (a rate other than 1
// without a pitch change, as atempo does in the render) then a volume element
// of our own. Gain and mute go on that element, not on playbin's properties:
// those are the sound server's per-application stream volume, which it
// REMEMBERS -- one 0 written there muted every later stream across restarts.
// A missing plugin degrades the preview rather than killing it.
func audioFilter(name string) (filter, gain gst.Element) {
	gain = gst.ElementFactoryMake("volume", name+"gain")
	tempo := gst.ElementFactoryMake("scaletempo", name+"tempo")
	bin, ok := gst.NewBin(name + "afilter").(gst.Bin)
	if !ok || gain == nil {
		return tempo, nil
	}
	var first, last gst.Element
	if tempo != nil {
		bin.Add(tempo)
		bin.Add(gain)
		tempo.Link(gain)
		first, last = tempo, gain
	} else {
		bin.Add(gain)
		first, last = gain, gain
	}
	sink := gst.NewGhostPad("sink", first.GetStaticPad("sink"))
	src := gst.NewGhostPad("src", last.GetStaticPad("src"))
	sink.SetActive(true)
	src.SetActive(true)
	bin.AddPad(sink)
	bin.AddPad(src)
	return bin, gain
}

// resetStreamVolume is the one write to playbin's own volume and mute, made
// once when the pipeline is built: full and unmuted, which the sound server
// stores in place of whatever it had remembered for this app (audioFilter).
func resetStreamVolume(pb gst.Element) {
	pb.SetObjectProperty("volume", 1.0)
	pb.SetObjectProperty("mute", false)
}

// setGain writes a gain, or a mute, onto our own volume element -- or, on a
// machine with no volume plugin, onto playbin's, which is the old behaviour
// and the old fault, kept over silence.
func setGain(gain, pb gst.Element, vol float64, mute bool) {
	el := gain
	if el == nil {
		el = pb
	}
	el.SetObjectProperty("volume", vol)
	el.SetObjectProperty("mute", mute)
}

// fileURI is how a path is handed to playbin, and it is not "file://"+path --
// which is what this was, and what a file could be lost behind. A path is
// bytes; a uri is text with punctuation, and the two disagree about '#'.
// Everything after a '#' in a uri is a fragment, dropped before the file is
// ever opened, so a voice sample named for hand-picked takes
// (own@-0.5#3a1f2b_9c2b1e4d.wav) reached filesrc as ".../own@-0.5" and failed
// with "No such file" -- a name the app itself had written. '?' would cut the
// same way and a '%' in a name would be read as an escape of whatever came
// after it.
//
// So every byte outside the unreserved set is percent-escaped, which GStreamer
// undoes on the way back to a filename. '/' is the exception: it is the one
// piece of punctuation that means the same thing on both sides.
func fileURI(path string) string {
	var b strings.Builder
	b.WriteString("file://")
	for i := 0; i < len(path); i++ {
		switch c := path[i]; {
		case c >= 'a' && c <= 'z', c >= 'A' && c <= 'Z', c >= '0' && c <= '9',
			c == '-', c == '_', c == '.', c == '~', c == '/':
			b.WriteByte(c)
		default:
			fmt.Fprintf(&b, "%%%02X", c)
		}
	}
	return b.String()
}

// PlaySegment cues [start, stop) seconds of file; stop < 0 means to the end.
// play=false prerolls only: the first frame shows, paused, until Toggle.
func (p *Player) PlaySegment(file string, start, stop float64, play bool) {
	if p.swapIn(file, start, stop, play) {
		return // the spare had it prerolled: no reload, and no wait
	}
	p.pb.SetState(gst.StateReady)
	p.pb.SetObjectProperty("uri", fileURI(file))
	p.loaded = file
	p.pendStart = int64(start * 1e9)
	p.pendStop = -1
	if stop > 0 {
		p.pendStop = int64(stop * 1e9)
	}
	p.pendPlay = play
	p.ended = false
	p.lastStart, p.lastStop = start, stop
	// preroll paused; the bus watch seeks (and maybe plays) on AsyncDone
	p.pb.SetState(gst.StatePaused)
	p.setPlaying(false)
	for _, a := range p.mix {
		a.cue(start, play, p.rate, p.stopFor(a))
	}
}

// ---- a picture in front of the video ----------------------------------------

// ShowStill puts a picture where the video was -- the card an insert plays --
// without touching the stream underneath, which the timeline scrolls against
// and the clock is read from. The swap is the paintable, not a widget over the
// picture.
func (p *Player) ShowStill(tex gdk.Paintabler) {
	if tex == nil {
		return
	}
	p.Picture.SetPaintable(tex)
	p.still = true
}

// ShowVideo takes that picture away again. Cheap to call on every tick: it is
// the playhead leaving an insert that has to reach it, and the playhead does not
// know when that was.
func (p *Player) ShowVideo() {
	if !p.still {
		return
	}
	p.Picture.SetPaintable(p.video)
	p.still = false
}

// ---- the separate recordings ------------------------------------------------

// What the cut plays is the session at that moment, not the file with the
// pictures: each separate recording is its own audio-only pipeline, seeked to
// its own second of the same instant and driven by the same transport. A
// monitor mix; the render does its own arithmetic (clipMixes).
type auxAudio struct {
	pb   gst.Element
	gain gst.Element // our volume element inside pb's audio path (audioFilter)
	// the lane's name, which is how a scene names the lanes it does not hear
	// (cutSeg.Quiet). Empty for a card's own sound, which no scene silences.
	base string
	// the file's seconds this lane covers: a recording is its whole file, a
	// cut lane is a window into one (tlAudio.off), and a further track of the
	// capture is the capture's own span. A seek outside [lo, hi] is silence,
	// not the wrong minute played quietly under the picture.
	lo, hi float64
	// the scene under the playhead does not hear this lane. Not merely
	// turned down: a hushed lane is never cued, never seeked and never set
	// to PLAYING (audible), so there is no sample of it to be heard early.
	// See applyMute.
	mute bool
	// what to add to a time in the master's file to get the same instant in
	// this one: (master's session start) - (this recording's session start).
	delta float64
	pend  int64 // nanoseconds; < 0 = nothing pending
	play  bool
	// live is a pipeline that has prerolled and not been sent below PAUSED
	// since: one that can be seeked in place. A lane in READY or NULL has to
	// preroll first and seek when that lands (cue, landed).
	live bool
	// stopAt is the second of this file the running seek stops at -- the end
	// of the scene under the line, in this lane's clock (Player.until) -- or
	// 0 for no stop. The stop is GStreamer's, sample-accurate, which is what
	// a timer-driven hush could never be: the lane fell silent up to a tick
	// after the boundary, and a tick is a tenth of a second of the next
	// scene's sound.
	stopAt float64
	// the master's rate, copied here at every seek: this pipeline's deferred
	// seek fires from a bus callback that has no player to ask
	rate float64
}

// mixTrack is one recording to be heard under the footage, as the cut editor
// knows it: where the file is, and how the two clocks differ.
type mixTrack struct {
	base  string
	path  string
	delta float64
	// the window of the file this lane is: the seconds of it between lo and
	// hi are what session time maps onto (tlAudio.off, tlAudio.dur)
	lo, hi float64
	// which audio stream of the file, a:N. Above nought this lane is a further
	// track of a multi-track capture, sharing its path with the footage that
	// is already playing -- and is the one kind of lane the preview used to
	// leave out entirely, which is what a badge lit green over silence was.
	track int
}

// SetMix replaces the recordings heard under whatever this player shows. The
// pipelines are rebuilt rather than reused: the set changes when the session's
// sources do, which is rare, and a stale uri playing under the wrong footage is
// the one failure that would be hard to notice.
func (p *Player) SetMix(tracks []mixTrack) {
	for _, a := range p.mix {
		a.pb.SetState(gst.StateNull)
	}
	p.mix = nil
	var names []string
	for i, t := range tracks {
		a := newAux(fmt.Sprintf("mix%d", i), t, p.vol(), p.laneErr(t.base))
		if a == nil {
			// one lane GStreamer would not build is one lane, not the rest of
			// the mix and not the hush below it
			p.say("!!! preview: could not build a pipeline for " + t.base)
			continue
		}
		p.mix = append(p.mix, a)
		names = append(names, t.base)
		p.say(fmt.Sprintf(">>> preview: lane %s is file seconds %.1f-%.1f, master second + %.1f, track %d",
			t.base, t.lo, t.hi, t.delta, t.track))
	}
	// said once per file, because a lane that is missing from this line is
	// the answer to "why can I not hear it" and there is no other way to see
	// which recordings the preview is mixing
	if len(names) > 0 {
		p.say(">>> preview: mixing " + strings.Join(names, ", ") + " under the footage")
	}
	// after the loop, so a lane the scene already silences is born silent
	// rather than saying its first ten milliseconds out loud
	p.applyMute()
}

// say puts a line in the app's log, when there is one to put it in. The
// player is built before the window is, so the hook is optional.
func (p *Player) say(s string) {
	if p.OnLog != nil {
		p.OnLog(s)
	}
}

// laneErr is what a mix lane's pipeline does with an error: name the lane and
// say it. A lane whose file will not decode, or whose sink cannot open the
// device a second time, used to fail without a word -- indistinguishable, on
// the page, from a lane that was playing.
func (p *Player) laneErr(base string) func(string) {
	return func(m string) { p.say("!!! preview: " + base + " will not play — " + m) }
}

// newAux builds one audio-only pipeline for a file and the bus watch that does
// its seeking. nil when GStreamer will not give us a playbin, which is the one
// failure a caller can do nothing about.
func newAux(name string, t mixTrack, vol float64, onErr func(string)) *auxAudio {
	// playbin, not playbin3: a lane may be the second track of the capture,
	// and playbin's current-audio is the one property that picks a track by
	// number. playbin3 selects streams by id, which is a query and a callback
	// for what is one integer here. With no video sink of its own it would put
	// up a window; a fake one is how "audio only" is spelled without flags.
	pb := gst.ElementFactoryMake("playbin", name)
	if pb == nil {
		return nil
	}
	if fake := gst.ElementFactoryMake("fakesink", name+"novideo"); fake != nil {
		pb.SetObjectProperty("video-sink", fake)
	}
	pb.SetObjectProperty("uri", fileURI(t.path))
	if t.track > 0 {
		pb.SetObjectProperty("current-audio", t.track)
	}
	filter, gain := audioFilter(name)
	pb.SetObjectProperty("audio-filter", filter)
	resetStreamVolume(pb)
	a := &auxAudio{pb: pb, gain: gain, base: t.base, delta: t.delta, lo: t.lo, hi: t.hi, pend: -1, rate: 1}
	// born at the loudness its player is already running at, which is the
	// slider and any volume effect under the playhead together (Player.vol)
	setGain(a.gain, pb, vol, false)
	bus := pb.GetBus()
	bus.AddSignalWatch()
	bus.ConnectMessage(func(_ gst.Bus, msg *gst.Message) {
		switch msg.Type() {
		case gst.MessageError:
			e, _ := msg.ParseError()
			a.pend, a.live = -1, false
			if onErr != nil {
				onErr(fmt.Sprint(e))
			}
		case gst.MessageAsyncDone:
			a.landed()
		}
	})
	return a
}

// landed is the deferred seek: the preroll a cue asked for has finished, so
// the lane can be put at its second and, if the transport is running and the
// scene hears it, let go. From the bus after an AsyncDone, and from cue itself
// when the state change was not asynchronous at all -- a pipeline already in
// PAUSED answers a request for PAUSED at once and posts no AsyncDone, and a
// cue that waited for one there waited forever, with the lane standing silent
// at its old second under a picture that had moved on.
func (a *auxAudio) landed() {
	if a.pend < 0 {
		return
	}
	at := a.pend
	a.pend, a.live = -1, true
	a.seekTo(float64(at) / 1e9)
	// the hush may have landed while this preroll was in flight, and
	// this callback is the one thing that would start the lane after it
	if a.play && !a.mute {
		a.pb.SetState(gst.StatePlaying)
	}
}

// The preview's loudness, one number for the whole app. Package state rather
// than a Player field because it is how loud the ROOM is, and the room does
// not change when a tab does -- there is a slider on the run bar and one
// beside each transport that can play video (volumeCtl), and all of them are
// hands on this one number. 1 is the footage as recorded; playbin treats it as
// a plain linear gain.
var previewVol = 1.0

// every player ever built, so a volume set on any of the sliders reaches
// pipelines that already exist. Players are made once per page at startup and
// never torn down, so the roll only ever holds those few.
var allPlayers []*Player

// SetPreviewVolume turns the whole preview up or down: every player, and inside
// each one the footage, the separate recordings mixed under it and an insert's
// own sound alike. One gain for all of them, because the slider's promise is
// "quieter", not "rebalanced".
func SetPreviewVolume(v float64) {
	previewVol = math.Max(0, math.Min(1, v))
	for _, p := range allPlayers {
		p.applyVol()
	}
}

// every volume slider on screen. There is one wherever a video can be played
// -- the run bar, the Cut transport, the Narrate transport -- and they are
// hands on the one number, so a slider pulled down on Cut has to be found
// pulled down on Narrate. Otherwise the second page would show 100% while
// playing at 40, which is a control lying about the state it is in.
var volScales []*gtk.Scale

// volSyncing is up while one slider is writing the others, so their own
// value-changed handlers know not to write back. GtkAdjustment stays quiet
// when the value does not actually change, so the loop would end anyway; the
// flag is here because "would end anyway" is a thing to have to work out, and
// the rounding through 0..100 and back is exactly where it would stop being
// true.
var volSyncing bool

// formSlider is what a slider with a number on it looks like everywhere: its
// own width, its value beside the trough (above, GTK's default, makes the row
// a line and a half tall), centred in its row.
func formSlider(sc *gtk.Scale, tip string) {
	sc.SetDrawValue(true)
	sc.SetValuePos(gtk.PosRight)
	sc.SetSizeRequest(150, -1)
	sc.SetHAlign(gtk.AlignStart)
	sc.SetVAlign(gtk.AlignCenter)
	sc.SetTooltipText(tip)
}

// volumeCtl is the preview volume control: a speaker and a slider, built fresh
// for each place a video can be played from. Built rather than shared because
// a GTK widget has one parent, and the alternative to one per transport is
// re-parenting it on every tab switch.
func volumeCtl() *gtk.Box {
	icon := gtk.NewImageFromIconName("audio-volume-high-symbolic")
	sc := gtk.NewScaleWithRange(gtk.OrientationHorizontal, 0, 100, 1)
	sc.SetValue(previewVol * 100)
	sc.SetSizeRequest(120, -1)
	tip := "preview volume — the players only; nothing that is rendered, and the " +
		"same setting wherever it is shown"
	icon.SetTooltipText(tip)
	sc.SetTooltipText(tip)
	sc.ConnectValueChanged(func() {
		if volSyncing {
			return
		}
		SetPreviewVolume(sc.Value() / 100)
		volSyncing = true
		for _, o := range volScales {
			if o != sc {
				o.SetValue(previewVol * 100)
			}
		}
		volSyncing = false
	})
	volScales = append(volScales, sc)
	box := gtk.NewBox(gtk.OrientationHorizontal, 4)
	box.SetVAlign(gtk.AlignCenter)
	box.Append(icon)
	box.Append(sc)
	return box
}

// SetFxGain is the volume effect under the playhead, which the preview obeys
// as it obeys a speed effect's rate. Multiplied with the slider, so turning
// the preview down still turns a boosted stretch down. Nothing is written when
// the gain has not moved -- this runs ten times a second.
func (p *Player) SetFxGain(g float64) {
	g = math.Max(0, math.Min(fxMaxGain, g))
	if math.Abs(p.fxGain-g) < 1e-6 {
		return
	}
	p.fxGain = g
	p.applyVol()
}

// vol is the one loudness every pipeline this player owns runs at: the room's
// setting (previewVol) and the cut's own say over the seconds under the
// playhead (fxGain), multiplied and held to what the property will take. One
// function, because a pipeline built later must start at the same number the
// ones built earlier are already at.
func (p *Player) vol() float64 {
	return math.Max(0, math.Min(fxMaxGain, previewVol*p.fxGain))
}

// applyVol writes the two gains, multiplied, onto every pipeline this player
// owns -- the footage, the separate recordings heard under it, and an insert's
// own sound. playbin's volume runs to 10, which is exactly as far as a volume
// effect goes (fxMaxGain), so a boosted stretch played at the slider's full
// travel still lands inside what the property will take.
func (p *Player) applyVol() {
	v := p.vol()
	if (v == 0) != p.volZero {
		p.volZero = v == 0
		if p.volZero {
			p.say(fmt.Sprintf(">>> preview: silent -- the slider is at %.2f and the volume effect under the line at %.2f", previewVol, p.fxGain))
		} else {
			p.say(">>> preview: sound is back")
		}
	}
	setGain(p.gain, p.pb, v, p.ownMute)
	for _, a := range p.mix {
		setGain(a.gain, a.pb, v, a.mute)
	}
	if p.card != nil {
		setGain(p.card.gain, p.card.pb, v, false)
	}
}

// SetMuted cuts the session's sound -- footage and every recording under it --
// without stopping anything; the clock runs on. The preview's half of an
// insert: the render never puts session audio under a card (clipMixes).
func (p *Player) SetMuted(v bool) {
	p.muted = v
	p.applyMute()
}

// Hush silences what the scene under the playhead does not hear (cutSeg.Quiet):
// only mute properties move, never the pipelines, since the answer changes
// every tick. until is the master-file second the answer expires at (0 =
// never); a lane started under it is seeked with a stop there (stopFor) so it
// falls silent on the boundary, and the tick re-places it (applyMute).
func (p *Player) Hush(own bool, quiet []string, until float64) {
	p.hushOwn, p.hush, p.until = own, hushSet(quiet), until
	p.applyMute()
}

// stopFor is where a lane's running seek stops, in the lane's own clock, or
// 0 for no stop.
func (p *Player) stopFor(a *auxAudio) float64 {
	if p.until <= 0 {
		return 0
	}
	return p.until + a.delta
}

// hushSet turns the scene's list into what the pipelines are checked against.
// nil for a scene that hears everything, which is most of them: the absence of
// the map IS that answer, so the common case costs nothing and a scene that
// stops silencing a lane cannot leave the old set standing behind it.
func hushSet(quiet []string) map[string]bool {
	if len(quiet) == 0 {
		return nil
	}
	m := make(map[string]bool, len(quiet))
	for _, q := range quiet {
		m[q] = true
	}
	return m
}

// hushes is the whole decision about one pipeline's sound, split out from the
// writing of it so it can be asked directly -- the pipelines are GStreamer and
// stay out of a unit test. own marks the footage's own sound, which is a lane
// the scene can silence like any other and is not one of p.mix.
func (p *Player) hushes(base string, own bool) bool {
	if p.muted {
		return true // not the session's picture at all, so none of its sound
	}
	if own {
		return p.hushOwn
	}
	return p.hush[base]
}

// applyMute writes that answer onto every pipeline, and only where it changed:
// this runs from the playback tick, and a property written ten times a second
// with the value it already holds is ten notifications a second for nothing.
//
// The footage's own sound is the video's pipeline and cannot be stopped -- the
// picture comes out of it -- so for that one the answer is the mute property.
// A separate recording is its own pipeline with nothing but sound in it, and
// there the answer is to stop it: muting it still starts it, and a pipeline
// that is started can be heard for the moment between the first buffer and the
// mute reaching the sink, which is exactly the "I can hear the lane I switched
// off, briefly" this exists to prevent. The property is written too, so the
// sound stops at once rather than when the state change lands.
func (p *Player) applyMute() {
	if m := p.hushes("", true); m != p.ownMute {
		p.ownMute = m
		setGain(p.gain, p.pb, p.vol(), m)
		switch {
		case !m:
			p.say(">>> preview: the footage's own sound is heard again")
		case p.muted:
			p.say(">>> preview: the footage's own sound is muted -- a card or a stop stands over the picture")
		default:
			p.say(">>> preview: the footage's own sound is muted -- the scene under the line does not hear it")
		}
	}
	// the master's second, asked once and only when a lane needs it -- a
	// player with no pipeline (a test's) is never asked
	pos, havePos, asked := 0.0, false, false
	where := func() (float64, bool) {
		if !asked {
			asked = true
			if p.pb != nil {
				pos, havePos = p.Position()
			}
		}
		return pos, havePos
	}
	for _, a := range p.mix {
		m := p.hushes(a.base, false)
		if m == a.mute {
			// a lane whose running seek has reached its stop stands silent at the
			// boundary; once the scene under the line has moved on and still hears it,
			// it is placed again with the next stop. Not before: a seek whose stop is
			// already behind it has no stop, and the lane would bleed past the boundary.
			if !m && a.live && a.stopAt > 0 && p.stopFor(a) > a.stopAt {
				if pos, ok := where(); ok && pos+a.delta >= a.stopAt-0.05 {
					p.place(a, pos, p.playing)
				}
			}
			continue
		}
		a.mute = m
		setGain(a.gain, a.pb, p.vol(), m)
		if m {
			p.say(">>> preview: " + a.base + " silent -- the scene under the line does not hear it")
		} else {
			p.say(">>> preview: " + a.base + " heard again")
		}
		if m {
			// READY, not PAUSED: a paused lane still holds a stream at the
			// sound server, corked, and sits in the system mixer as a second
			// "naivepost-gui" at whatever level -- which is a thing to wonder
			// about. A lane nobody hears has no stream.
			a.pend, a.live = -1, false
			a.pb.SetState(gst.StateReady)
			continue
		}
		// heard again: it has been sitting still wherever it was stopped, so
		// it is put back on the master's clock before it is let go
		if pos, ok := where(); ok {
			p.place(a, pos, p.playing)
		}
	}
}

// CardSound plays an inserted video's own audio, at seconds into it; an empty
// file takes it away. Its own pipeline, unmuted by SetMuted: it is the
// insert's sound, not the session's. A card with no audio track plays nothing.
func (p *Player) CardSound(file string, at float64, play bool) {
	if file != p.cardFile {
		p.dropCard()
		if file == "" {
			return
		}
		a := newAux("cardsound", mixTrack{path: file}, p.vol(), p.laneErr("the card's sound"))
		if a == nil {
			return
		}
		p.card, p.cardFile = a, file
	}
	if p.card == nil {
		return
	}
	p.card.cue(at, play, p.rate, 0)
}

// dropCard tears the insert's audio pipeline down. Not merely paused: the next
// card is a different file, and a uri cannot be changed under a live pipeline.
func (p *Player) dropCard() {
	if p.card != nil {
		p.card.pb.SetState(gst.StateNull)
	}
	p.card, p.cardFile = nil, ""
}

// cue puts this recording at the master's time t and either holds it there or
// lets it run. A time this recording was not running at is silence, and silence
// is a pipeline taken to READY -- no stream at all -- rather than one seeked to
// its own edge, which would play the wrong minute quietly under the picture.
//
// stop is the second of this file to stop at, or 0 (Player.stopFor).
//
// A lane that is live -- prerolled, in PAUSED or PLAYING -- is seeked in
// place and set going: no state change on the way, because a PLAYING lane
// asked for PAUSED answers asynchronously and needs a fresh buffer to
// preroll on before it says so, and the seek that waited for that came late
// or not at all -- scene 3 played silent until a badge was pressed twice. A
// lane below PAUSED has no choice: it prerolls, and seeks when that lands.
func (a *auxAudio) cue(t float64, play bool, rate float64, stop float64) {
	at := t + a.delta
	a.rate = rate // the deferred seek fires with no player in reach
	if !a.audible(t) {
		a.pend, a.live = -1, false
		a.pb.SetState(gst.StateReady)
		return
	}
	a.play, a.stopAt = play, stop
	if a.live {
		a.pend = -1
		a.seekTo(at)
		if play {
			a.pb.SetState(gst.StatePlaying)
		} else {
			a.pb.SetState(gst.StatePaused)
		}
		return
	}
	a.pend = int64(at * 1e9)
	// a synchronous answer is a pipeline that was already there: no bus
	// message is coming, so the seek is made now (see landed)
	if a.pb.SetState(gst.StatePaused) == gst.StateChangeSuccess {
		a.landed()
	}
}

// seekTo is the lane's flushing seek to its own second, with the stop the
// scene asked for when there is one ahead of it.
func (a *auxAudio) seekTo(at float64) {
	if a.stopAt > at {
		a.pb.Seek(a.rate, gst.FormatTime, gst.SeekFlagFlush|gst.SeekFlagAccurate,
			gst.SeekTypeSet, int64(at*1e9), gst.SeekTypeSet, int64(a.stopAt*1e9))
		return
	}
	a.pb.Seek(a.rate, gst.FormatTime, gst.SeekFlagFlush|gst.SeekFlagAccurate,
		gst.SeekTypeSet, int64(at*1e9), gst.SeekTypeNone, 0)
}

// running says whether this one has something to play at the master's time t,
// which is what keeps a resume from starting a recording that had already
// stopped when this second happened.
func (a *auxAudio) running(t float64) bool {
	at := t + a.delta
	return at >= a.lo && (a.hi <= a.lo || at <= a.hi)
}

// audible is whether this lane is to be running at all: it has something at
// this second AND the scene under the playhead hears it. Every path that
// starts a recording asks this one question, because "silenced" has to mean
// the same thing as "nothing recorded here" -- a pipeline that is not started
// cannot be heard for the moment before the mute lands.
func (a *auxAudio) audible(t float64) bool {
	return !a.mute && a.running(t)
}

// setPlaying records the state and tells whoever is drawing a transport button
// that it changed. Nothing else may write p.playing.
func (p *Player) setPlaying(v bool) {
	if p.playing == v {
		return
	}
	p.playing = v
	if p.OnState != nil {
		p.OnState()
	}
}

// Playing reports whether the stream is running right now; Cued reports that
// something is loaded and paused, i.e. that ▶ should resume it rather than
// start whatever the page does when idle.
func (p *Player) Playing() bool { return p.playing }
func (p *Player) Cued() bool    { return p.loaded != "" && !p.playing }

// Loaded is whether this player has been pointed at a file at all, playing or
// not. The question an overlay asks before it draws anything on the picture:
// there is no framing to show over a player that has never been given footage.
func (p *Player) Loaded() bool { return p.loaded != "" }

// Toggle is what every play button in the app does: pause what is running,
// resume what is paused -- and, on a stream that has run to its end, play it
// again from the top. Resuming at the last frame is silence and a still
// picture, i.e. a play button that looks broken.
func (p *Player) Toggle() {
	switch {
	case p.playing:
		p.pb.SetState(gst.StatePaused)
		p.syncMix(false)
		p.cardState(gst.StatePaused)
		p.setPlaying(false)
	case p.ended && p.loaded != "":
		p.PlaySegment(p.loaded, p.lastStart, p.lastStop, true)
	default:
		// a rate chosen while paused has not reached the pipeline yet, and
		// syncMix below is about to seek the recordings at it: without this
		// the picture would run at the old clock and the sound at the new one
		if math.Abs(p.rate-p.seekRate) > 1e-6 {
			if pos, ok := p.Position(); ok {
				p.SeekTo(pos)
			}
		}
		p.pb.SetState(gst.StatePlaying)
		p.syncMix(true)
		// an insert's sound stops and starts with the transport like everything
		// else on the page; where it is in the card is where it was left
		p.cardState(gst.StatePlaying)
		p.setPlaying(true)
	}
}

// syncMix puts every recording back on the master's clock. It is called on
// every transport change rather than only on the first one: two pipelines
// started a minute apart agree to the millisecond and then drift as slowly as
// their clocks differ, and a resync that costs a seek nobody hears is cheaper
// than reasoning about how long that takes to be audible.
func (p *Player) syncMix(play bool) {
	if len(p.mix) == 0 {
		return
	}
	pos, ok := p.Position()
	if !ok {
		return
	}
	for _, a := range p.mix {
		p.place(a, pos, play)
	}
}

// place puts one recording at the master's time t and lets it run or holds it.
// The one place a mix pipeline is started, so it knows that a lane with nothing
// to play and a lane the scene does not hear are the same thing (audible):
// READY. Through cue, not a bare seek: a hushed lane is in READY with no
// stream (applyMute), and a seek on that is a start from the file's first
// second.
func (p *Player) place(a *auxAudio, t float64, play bool) {
	if !a.audible(t) {
		a.pend, a.live = -1, false
		a.pb.SetState(gst.StateReady)
		if !a.mute {
			p.say(fmt.Sprintf(">>> preview: %s has nothing at the master's second %.1f (its file second %.1f is outside %.1f-%.1f)",
				a.base, t, t+a.delta, a.lo, a.hi))
		}
		return
	}
	if play {
		p.say(fmt.Sprintf(">>> preview: %s playing from its second %.1f", a.base, t+a.delta))
	}
	a.cue(t, play, p.rate, p.stopFor(a))
}

// Stop tears the stream down and forgets the file, so the next ▶ is a fresh
// start rather than a resume of something the user already ended.
func (p *Player) Stop() {
	p.pb.SetState(gst.StateReady)
	for _, a := range p.mix {
		a.pend, a.live = -1, false
		a.pb.SetState(gst.StateReady)
	}
	p.loaded = ""
	p.ended = false
	p.dropSpare() // ⏹ is a fresh start for the next ▶, and that includes what was prerolled for it
	p.dropCard()
	p.SetMuted(false) // whatever was covering the sound is over with the stream
	p.setPlaying(false)
}

func (p *Player) Pause() {
	p.pb.SetState(gst.StatePaused)
	for _, a := range p.mix {
		a.pb.SetState(gst.StatePaused)
	}
	p.cardState(gst.StatePaused)
	p.setPlaying(false)
}

// cardState moves the insert's audio pipeline, if there is one, and is silence
// about it when there is not.
func (p *Player) cardState(s gst.State) {
	if p.card != nil {
		p.card.pb.SetState(s)
	}
}

// SeekTo jumps within the currently loaded file; while paused the new frame
// still renders, which is what frame-stepping relies on.
func (p *Player) SeekTo(t float64) {
	p.ended = false // wherever we land, there is stream ahead of it again
	p.seekRate = p.rate
	p.pb.Seek(p.rate, gst.FormatTime,
		gst.SeekFlagFlush|gst.SeekFlagAccurate,
		gst.SeekTypeSet, int64(t*1e9), gst.SeekTypeNone, 0)
	// the recordings land on the same instant, told in their own seconds --
	// asking the master where it is would ask before this seek has taken
	for _, a := range p.mix {
		p.place(a, t, p.playing)
	}
}

// Rate is the clock the picture is actually running on -- the last seek's, not
// one asked for and not yet taken hold.
func (p *Player) Rate() float64 {
	if p.seekRate <= 0 {
		return 1
	}
	return p.seekRate
}

// SetRate stores the clock the stream is to run on and says whether that is a
// change. It does NOT seek: a rate only takes effect at a seek and every caller
// is about to make one. Changing the rate mid-playback is the editor's job
// (syncPlayRate).
func (p *Player) SetRate(r float64) bool {
	if r <= 0 || math.IsNaN(r) || math.IsInf(r, 0) {
		r = 1
	}
	if math.Abs(r-p.rate) < 1e-6 {
		return false
	}
	p.rate = r
	return true
}

// SetRateNow changes the rate under a running preview with a flushing seek to
// the current position -- a small stumble. INSTANT_RATE_CHANGE does not return
// on this stack (playbin3, scaletempo, gtk4paintablesink). Held to one seek
// per rateSeekGap; inside the gap the rate is put back so the next tick asks
// again.
func (p *Player) SetRateNow(r float64) {
	was := p.rate
	if !p.SetRate(r) {
		return
	}
	if !p.playing {
		return // parked: whatever seek starts the stream again carries it
	}
	if time.Since(p.rateSeekAt) < rateSeekGap {
		p.rate = was
		return
	}
	p.rateSeekAt = time.Now()
	if pos, ok := p.Position(); ok {
		p.SeekTo(pos)
	}
}

// rateSeekGap is the least time between two flushing seeks made only to
// change the rate under a running preview.
const rateSeekGap = 250 * time.Millisecond

// Position reports the current playback position in seconds.
func (p *Player) Position() (float64, bool) {
	ns, ok := p.pb.QueryPosition(gst.FormatTime)
	if !ok || ns < 0 {
		return 0, false
	}
	return float64(ns) / 1e9, true
}

// videoFrame is the box a preview sits in, and all three pages that show video
// use it so they cannot drift apart.
//
// gtk.NewFrame("") is a frame WITH a label: the empty string still builds a
// GtkLabel, and an empty label is a whole line of text high. Measured, not
// guessed -- 640x361 frame, child allocated at y=26 with a height of 333 and
// the top 26 pixels gone. Every video in the app was sitting under an invisible
// caption. A frame with no label widget hands its child the whole inside.
//
// The dark fill is the other half of the same complaint. A picture keeps the
// video's aspect, so whatever the frame has spare turns into bars beside it.
// In the window's own background those bars read as an over-wide border; in
// black they read as letterboxing, which is what they are.
func videoFrame(child gtk.Widgetter) *gtk.Frame {
	f := gtk.NewFrame("")
	f.SetLabelWidget(nil)
	f.AddCSSClass("videoframe")
	if child != nil {
		f.SetChild(child)
	}
	return f
}

// playerErr is what every player's OnError is set to. Named, because there are
// five of them and "playback failed" in a log shared by the whole pipeline
// answers none of the questions you have at that point. The status line gets it
// too: a failure the log records and the window does not mention is a failure
// nobody notices until they wonder why the video is silent.
func (a *App) playerErr(who string) func(string) {
	return func(m string) {
		a.logf("!!! %s: playback failed — %s", who, m)
		if a.status == nil {
			return // a player that failed before the window finished; stderr has it
		}
		a.setStatus(who + " would not play — see log")
		a.updateRunControls()
	}
}
