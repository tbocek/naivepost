package main

// The run bar: one ▶ and one ⏹ for every page, meaning the same thing on each
// -- start, pause and resume what the page does; end it. Moved out of
// pipeline.go, which is the steps and their subprocesses, because this is the
// one thing in that file that touched a widget, and a file that runs ffmpeg
// should not need GTK to compile.

import (
	"github.com/diamondburned/gotk4/pkg/gtk/v4"
)

// The tracks the two halves of the bar are reported on, and prog itself, live
// in runqueue.go with the work queue they feed.

// transport is what a page offers the run bar when it plays media rather than
// running a pipeline. With it, ▶ and ⏹ mean the same thing on every step: ▶
// starts what the page does and then pauses and resumes it, ⏹ ends it. Without
// it a page's playback was reachable only through the page's own buttons --
// which is why ⏸ and ⏹ sat there doing nothing while a voice sample played.
type transport interface {
	playing() bool // running right now
	cued() bool    // loaded but paused, so ▶ means resume rather than start over
	toggle()
	stop()
}

// pageTransport is the visible page's playback, if that page has any.
func (a *App) pageTransport() transport {
	switch a.stack.VisibleChildName() {
	case "cut":
		// ▶ here suggests a cut, the page's long job, and it stays that way
		// until the preview is actually running. The player is loaded the
		// moment you click the timeline -- that cues a frame to look at, it is
		// not playback, and it used to mean ▶ played the video on a page whose
		// cut was still empty. Once the preview HAS been started, the run bar
		// is its transport until ⏹.
		if a.ed != nil && (a.ed.playing() || a.ed.started) {
			return a.ed
		}
	case "narrate":
		// ▶ here is the step itself -- write the narration, then speak it -- and
		// it stays that way until the preview is actually running: the voice
		// sample has its own two buttons beside it, and a clip merely cued by
		// clicking a line to look at it is not playback. Once the preview HAS
		// been started, the run bar is its transport until ⏹ -- otherwise ⏸
		// would have nothing to pause and ⏹ nothing to end.
		if a.narr != nil && (a.narr.playing() || a.narr.started) {
			return a.narr
		}
	}
	return nil
}

// playClicked runs whatever step is on screen, pauses it, or resumes it.
func (a *App) playClicked() {
	if a.running {
		// while a run is under way ▶ is the pause button, and says so
		if a.pauseFlag.Load() {
			a.pauseFlag.Store(false)
			a.setStatus("resumed")
		} else {
			a.pauseFlag.Store(true)
			a.setStatus("pausing after the current stage…")
		}
		a.updateRunControls()
		return
	}
	// playback beats the page's action: once something is playing, the button
	// belongs to it until it is stopped or reaches the end
	if t := a.pageTransport(); t != nil && (t.playing() || t.cued()) {
		t.toggle()
		a.updateRunControls()
		return
	}
	// ▶ runs the steps ticked beside it (runchain.go), which is one step on a
	// fresh project and as many as you like after that. It used to run
	// whichever page was showing, which made the button mean a different
	// thing on each of four pages and gave the run bar two ▶s -- one for this
	// page, one for several -- where there is only ever one thing to press.
	a.snapSources()
	a.chainRun()
}

// runPageNow is one step, the same call that page's ▶ used to make on its own.
func (a *App) runPageNow(page string) {
	switch page {
	case "prep":
		// one press for the whole step: the transcripts and the frames, then
		// the describing and the fixing. They were two tabs and two buttons,
		// and the second could not be pressed before the first had finished.
		a.prepRun()
	case "cut":
		// ▶ is this step's job, suggesting, as on every other page; adding a
		// selection is ＋ Add. Suggesting over hand edits still refuses in
		// suggestClicked and says to Revert first.
		if a.ed != nil {
			a.suggestClicked()
		}
	case "narrate":
		// the whole step in one press: write the narration if the cut has no
		// narration or has moved under the one it has, then speak every line
		// that is not already in the cache (narrateRun). It used to be only the
		// speaking half, with the writing on a button beside the video.
		a.narrateRun()
	case "produce":
		// the whole end of the pipeline in one press: the upload text once,
		// the thumbnail, then the render -- whatever of it is still missing is
		// made, and what is already written is never rewritten (produceClicked)
		a.produceClicked()
	}
}

func (a *App) stopClicked() {
	if t := a.pageTransport(); t != nil && (t.playing() || t.cued()) {
		t.stop()
		a.setStatus("playback stopped")
		a.updateRunControls()
	}
	if !a.running {
		return
	}
	a.stopFlag.Store(true)
	a.pauseFlag.Store(false)
	if a.runCancel != nil {
		a.runCancel() // aborts LLM requests, which are not subprocesses
	}
	a.ctlMu.Lock()
	for cmd := range a.curCmds {
		if cmd.Process != nil {
			_ = cmd.Process.Kill()
		}
	}
	a.ctlMu.Unlock()
	a.setStatus("stopping…")
}

// updateRunControls draws the two buttons from whatever is actually under way:
// a run, this page's playback, or nothing. ▶ is a toggle rather than a separate
// pair, because a dedicated ⏸ was dead on every page whose ▶ is not a run.
func (a *App) updateRunControls() {
	t := a.pageTransport()
	busy := (a.running && !a.pauseFlag.Load()) || (t != nil && t.playing())
	if busy {
		a.playBtn.SetIconName("media-playback-pause-symbolic")
		a.playBtn.SetTooltipText("Pause")
	} else {
		a.playBtn.SetIconName("media-playback-start-symbolic")
		a.playBtn.SetTooltipText("Run this step — or resume what is paused")
	}
	a.stopBtn.SetSensitive(a.running || busy || (t != nil && t.cued()))
	a.syncPlayIcons()
}

// setPlayIcon draws one transport button from what its player is doing. Every
// play button in the app is this one button in two states -- a ▶ that has
// already started something is a lie, and a separate ⏸ beside it is a button
// that is dead more often than not.
func setPlayIcon(b *gtk.Button, playing bool, playTip, pauseTip string) {
	if b == nil {
		return
	}
	if playing {
		b.SetIconName("media-playback-pause-symbolic")
		b.SetTooltipText(pauseTip)
	} else {
		b.SetIconName("media-playback-start-symbolic")
		b.SetTooltipText(playTip)
	}
}

// syncPlayIcons redraws the pages' own play buttons. They hang off four
// different players, all of which report here through Player.OnState, so this
// runs on every start, pause and end-of-stream -- including the ones nobody
// clicked for, like a clip simply finishing.
func (a *App) syncPlayIcons() {
	if a.ed != nil {
		// two ▶s on that page, each pausing only what IT plays: plain ▶ wears
		// ⏸ only while the RECORDING runs, and ▶✂ (its own two-glyph face,
		// synced beside this) only while the cut does -- one ⏸ across both
		// would pause something its button never claimed to have started
		setPlayIcon(a.ed.playBtn, a.ed.playing() && !a.ed.cutOnly,
			"play the recording from the playhead — every second of it, removed "+
				"stretches included", "pause the preview")
		// and its lamp: lit while the preview is the recording, as ▶✂ is lit
		// while it is the cut and ▶✂✂ while it is the review (lamp)
		lamp(a.ed.playBtn, !a.ed.cutOnly)
		a.ed.syncCutPlay()
		a.ed.syncReviewPlay() // and the third, whose ⏸ is only the review's
	}
	if n := a.narr; n != nil {
		// the preview has no button of its own any more: the picture is its
		// play/pause, and the run bar is the rest of its transport
		n.syncSpeakIcons()
	}
	if vp := a.voicePick; vp != nil {
		// vp.spoken is what the player is holding: while the band walks the
		// takes it is "" and the player is on the RECORDING, so a face drawn
		// from playing() alone offered to pause a sample that is not there --
		// and pressing it synthesized one instead
		setPlayIcon(vp.playBtn, vp.playing() && vp.spoken != "",
			"Speak the sample in the selected voice", "pause the sample")
		if vp.stopBtn != nil {
			// nothing loaded is nothing to stop: the sample's ⏹ is the one
			// button on this page that the run bar's ⏹ no longer covers
			vp.stopBtn.SetSensitive(vp.playing() || vp.cued())
		}
		// and the band's ▶ beside the dropdown, which wears ⏹ rather than ⏸
		// while it walks the takes -- this is where a walk that simply ran out
		// puts it back (narrate_takeband.go)
		vp.band.syncPlayBtn()
	}
	// Produce is not here: it has no play button of its own. A finished run cues
	// its result into the picture and the run bar is that video's transport from
	// there, which is one ▶ for the page instead of the two it used to have.
}
