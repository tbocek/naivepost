package main

// The third ▶: review the cuts.
//
// ▶ plays the recording and ▶✂ plays the finished video, and neither is the
// way to check a cut. What you want to know about a cut is whether the JOIN
// works -- does the sentence run on, is the splice in a silence, did the model
// take one word too many -- and that is a few seconds either side of each
// seam, not the minutes of kept material between them. So ▶✂✂ plays exactly
// that: for every join, reviewPad seconds of the cut before it and reviewPad
// after, one join after the other, and stops when the last one has been heard.
//
// It is the ▶✂ preview underneath -- the removed stretch is jumped by the same
// skipGap, the clock reads the cut's own time -- with one more hand on the
// line: reviewTick, which notices when the seconds after a join have played
// out and moves the line to the run-up of the next. A join whose run-up is
// already under the line (two cuts close together) is simply played into,
// so a stretch of short clips is heard once, in order, and not rewound for
// every seam in it.
//
// Nothing about it is saved; pressing anything else that moves the line ends
// the review, and pausing does too.

import (
	"fmt"
	"math"

	"github.com/diamondburned/gotk4/pkg/gtk/v4"
)

// reviewPad is how much of the cut is heard on each side of a join, in
// seconds of the finished video.
const reviewPad = 10.0

// reviewMove is what the review does on a tick.
type reviewMove int

const (
	reviewStay reviewMove = iota // the join under review is still being heard
	reviewSeek                   // the next join's run-up is ahead: seek to it
	reviewDone                   // the last join has been heard
	reviewLost                   // the line is nowhere near the join: somebody moved it
)

// reviewWindow is seam i's stretch of the cut: the join is where segs[i]
// ends and segs[i+1] begins. from is reviewPad before it, but never before
// the earlier clip's own start -- what lies before that is a removed stretch,
// not the cut -- and to is reviewPad after it, but never past the later clip's
// end, for the same reason the other way round.
func reviewWindow(segs []cutSeg, i int) (from, to float64) {
	from = math.Max(segs[i].S, segs[i].E-reviewPad)
	to = math.Min(segs[i+1].E, segs[i+1].S+reviewPad)
	return from, to
}

// reviewStep is the review's whole decision, pure so it can be checked
// without a player: seam i is being heard and the line is at session time t.
// It answers which seam is under review after this tick and what the line
// has to do about it. A seam whose run-up begins at or before t is not sought
// but played on into (see the top of the file).
//
// armed says the line has already been seen inside this seam's window. A
// line found BEFORE the window is then a hand's doing and the review is
// over; unarmed it is the review's own seek not yet landed -- the player
// answers the old position for a tick or two after a flushing seek, and 0
// for the first tick on a freshly cued file -- and the answer is to wait.
func reviewStep(segs []cutSeg, i int, t float64, armed bool) (int, reviewMove) {
	last := len(segs) - 2 // the last seam: there are len-1 of them
	if i > last {
		return i, reviewDone
	}
	from, to := reviewWindow(segs, i)
	if t < from {
		if armed {
			return i, reviewLost
		}
		return i, reviewStay
	}
	if t < to {
		return i, reviewStay
	}
	for i < last {
		i++
		from, to = reviewWindow(segs, i)
		if from > t {
			return i, reviewSeek
		}
		if t < to {
			return i, reviewStay
		}
	}
	return i, reviewDone
}

// reviewSeamAt is where a review pressed with the line at t begins: the seam
// whose window the line is already in, played on from the line itself, or
// else the next seam ahead, sought to its run-up. Past the last window it
// wraps to the first join -- there is nothing ahead to hear, and a review
// that refuses to start is worse than one that starts over.
func reviewSeamAt(segs []cutSeg, t float64) (i int, seek bool) {
	for i := 0; i < len(segs)-1; i++ {
		from, to := reviewWindow(segs, i)
		if t < from {
			return i, true
		}
		if t < to {
			return i, false
		}
	}
	return 0, true
}

// reviewSeams is how many joins the cut has to review: one fewer than its
// clips. A cut of one clip has nothing to hear.
func (ed *cutEditor) reviewSeams() int { return max(len(ed.segs)-1, 0) }

// playReview is the ▶✂✂ button. Pressed while it is the one running, it
// pauses, and the review is over -- a resume would be ▶✂ from wherever the
// line stands, which is what that button is for. Otherwise it starts from the
// first join, whatever was playing and whichever preview it was.
//
// The three ▶s are one rule: each wears ⏸ only while ITS thing runs, pressing
// that one pauses it, and pressing another switches the preview over to that
// button's thing without stopping. ▶ during a review plays the recording on
// from the line; ▶✂ during a review plays the cut on from the line; either
// ends the review (setCutOnly, playAs).
func (ed *cutEditor) playReview() {
	if ed.reviewOn && ed.playing() {
		ed.toggle() // pauses, and the pause ends the review (see toggle)
		return
	}
	if ed.reviewSeams() == 0 {
		ed.a.setStatus("nothing to review — a cut needs two clips to have a join between them")
		return
	}
	ed.setCutOnly(true) // the review IS the cut preview, with one more hand on the line
	// from the red line, not from the top: a review paused to look at a join
	// resumes at that join, and a line parked between two joins goes to the
	// next one. The first join only when there is none ahead.
	i, seek := reviewSeamAt(ed.segs, ed.playhead)
	ed.reviewOn, ed.review, ed.reviewArmed = true, i, !seek
	ed.jumped = -1
	if seek {
		from, _ := reviewWindow(ed.segs, i)
		ed.setPlayhead(ed.playable(from))
	} else {
		ed.setPlayhead(ed.playhead) // where it stands: cues the player there
	}
	if !ed.playing() {
		ed.toggle()
	}
	ed.sayReview()
	ed.syncFaces()
}

// reviewTick is the review's hand on the line, from followPlayback: once the
// seconds after the join under review have played, the line goes to the next
// join's run-up, or stops after the last one. true when it moved or stopped
// the line, in which case the tick is done (setPlayhead did the rest).
func (ed *cutEditor) reviewTick() bool {
	if !ed.reviewOn {
		return false
	}
	if !ed.cutOnly || ed.reviewSeams() == 0 {
		ed.reviewOff() // the preview was switched under it, or the cut was edited away
		return false
	}
	i, move := reviewStep(ed.segs, ed.review, ed.playhead, ed.reviewArmed)
	if i != ed.review {
		ed.reviewArmed = false // a new seam: its window has not been seen yet
	}
	ed.review = i
	if from, _ := reviewWindow(ed.segs, i); ed.playhead >= from {
		ed.reviewArmed = true
	}
	switch move {
	case reviewSeek:
		ed.jumped = -1
		ed.reviewArmed = false
		from, _ := reviewWindow(ed.segs, i)
		ed.setPlayhead(ed.playable(from))
		ed.sayReview()
		return true
	case reviewDone:
		n := ed.reviewSeams()
		ed.reviewOff()
		if ed.player != nil {
			ed.player.Pause()
		}
		ed.a.setStatus(fmt.Sprintf("reviewed all %d cuts", n))
		ed.a.updateRunControls()
		return true
	case reviewLost:
		ed.reviewOff()
		ed.a.setStatus("the line was moved — the cut review is over; ▶✂✂ starts it again")
	}
	return false
}

// reviewOff ends the review without touching the transport: whatever is
// playing plays on as the plain ▶✂ preview.
func (ed *cutEditor) reviewOff() {
	if !ed.reviewOn {
		return
	}
	ed.reviewOn = false
	ed.syncFaces()
}

// syncFaces redraws all three ▶s. The review starting or ending changes what
// ▶✂ says as much as what ▶✂✂ says -- the ⏸ and the lamp move from one to
// the other -- and when the cut was already playing there is no transport
// change to redraw them for: a review started over a running cut left ▶✂
// wearing ⏸ and lit beside a ▶✂✂ wearing the same, which is the one state
// the three-button rule exists to rule out.
func (ed *cutEditor) syncFaces() {
	if ed.a != nil {
		ed.a.syncPlayIcons()
		return
	}
	ed.syncReviewPlay()
}

func (ed *cutEditor) sayReview() {
	ed.a.setStatus(fmt.Sprintf("reviewing cut %d of %d — %.0f s before and after the join at %s",
		ed.review+1, ed.reviewSeams(), reviewPad, mmss(ed.segs[ed.review].E)))
}

// syncReviewPlay draws the ▶✂✂ button: a pause face while the review is what
// is running, lit while a review is on at all, and grey when the cut has no
// join to review. The face is built the way ▶✂'s is -- a stock icon and a
// label beside it -- for the reason given at syncCutPlay.
func (ed *cutEditor) syncReviewPlay() {
	if ed.reviewBtn == nil || ed.reviewIcon == nil {
		return
	}
	ed.reviewBtn.SetSensitive(ed.reviewSeams() > 0)
	if ed.reviewOn && ed.playing() {
		ed.reviewIcon.SetFromIconName("media-playback-pause-symbolic")
		ed.reviewBtn.SetTooltipText("pause the cut review")
	} else {
		ed.reviewIcon.SetFromIconName("media-playback-start-symbolic")
		ed.reviewBtn.SetTooltipText(fmt.Sprintf("review every cut in one go: plays %.0f s of the "+
			"finished video before each join and %.0f s after it, one join after the other, "+
			"and stops after the last. The removed stretches are skipped as under ▶✂. "+
			"Changes nothing that is saved.", reviewPad, reviewPad))
	}
	lamp(ed.reviewBtn, ed.reviewOn) // one of the three is lit: see syncPlayIcons
}

// lamp lights a button, or puts it out. The three ▶s on the Cut page share
// one lamp between them: the lit face is the one whose preview the page is
// in right now -- the recording, the cut, or the review of the cut -- so the
// page always says which of the three a ⏸, the clock and the dimming belong
// to. Never two at once and never none.
func lamp(b *gtk.Button, on bool) {
	if b == nil {
		return
	}
	if on {
		b.AddCSSClass("suggested-action")
	} else {
		b.RemoveCSSClass("suggested-action")
	}
}

// buildReviewBtn is the button itself, for the transport group in buildCut.
func (ed *cutEditor) buildReviewBtn() *gtk.Button {
	ed.reviewIcon = gtk.NewImageFromIconName("media-playback-start-symbolic")
	face := gtk.NewBox(gtk.OrientationHorizontal, 2)
	face.Append(ed.reviewIcon)
	face.Append(gtk.NewLabel("✂✂"))
	ed.reviewBtn = gtk.NewButton()
	ed.reviewBtn.SetChild(face)
	ed.reviewBtn.ConnectClicked(ed.playReview)
	ed.syncReviewPlay()
	return ed.reviewBtn
}
