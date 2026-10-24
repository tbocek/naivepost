package main

// What the preview shows after the line is moved by hand.
//
// The picture on screen is drawn on a second clock -- livePlayhead, which adds
// the time since the last position read back on so a camera glide is smooth at
// sixty frames a second instead of stepped at ten. That clock carries every
// effect with it: which titles are up and how faded, where the camera is,
// which stretch of footage is running fast.
//
// So the clock has to be where the red line is. It was not, after a jump: the
// line went back and the clock stayed in the future, and the preview went on
// drawing a title whose band the line had not reached yet.

import (
	"math"
	"strings"
	"testing"
	"time"
)

// jumpEd is a cut with one title and one zoom late in the session, and a
// preview playing through them.
func jumpEd() *cutEditor {
	ed := &cutEditor{playhead: 724, hasPlay: true}
	ed.fx = []cutFx{
		{Kind: "text", T: 724, Dur: 3.2, Trans: 0.4, Tout: 0.4, Text: "Finally 500! Yeah"},
		{Kind: "zoom", T: 724, Dur: 3.2, Trans: 0.4, Tout: 0.4, Cx: 0.3, Cy: 0.3, Hf: 0.5},
	}
	ed.player = &Player{playing: true}
	ed.posT, ed.posAt = 724, time.Now().Add(-50*time.Millisecond)
	return ed
}

// The case this came from: playing through a title at 12:04, jumping back to
// 11:59, and finding the words still on the picture -- at full strength, over
// footage five seconds before the effect starts.
func TestJumpingBackTakesTheEffectsWithIt(t *testing.T) {
	ed := jumpEd()
	if len(textsAt(ed.fx, ed.livePlayhead())) != 1 {
		t.Fatal("the title is not on screen while the line is standing in its band")
	}

	// the hand on the line, the way every path that moves it does the moving
	ed.playhead = 719
	ed.reLive(719)

	now := ed.livePlayhead()
	if now > 719+float64(playTick)/1000 {
		t.Fatalf("the line went back to 719 and the live clock reads %.3f", now)
	}
	if n := len(textsAt(ed.fx, now)); n != 0 {
		t.Errorf("%d title(s) on the picture at 719, five seconds before the band starts", n)
	}
	if a := textAlpha(ed.fx[0], now); a != 0 {
		t.Errorf("the title is %.0f%% opaque at 719, before it begins", a*100)
	}
	// and the camera with it: a zoom that has not come round yet does not get
	// to frame the picture either, so the framing at 719 is still the one
	// nobody placed
	srcA, outA := 16.0/9, 9.0/16
	if r, want := fxRectAt(ed.fx, now, srcA, outA), fullFill(srcA, outA); r != want {
		t.Errorf("the camera at 719 is %.2f of the frame at (%.2f, %.2f) -- "+
			"the zoom at 724 is already holding it", r.hf, r.cx, r.cy)
	}
}

// The clock may run ahead of the last position read, because that is the whole
// point of it, but only by the one tick it is filling in. Anything more is not
// smoothing, it is the clock standing somewhere the stream is not.
//
// This is the same bug as above seen from the other side, and it is what makes
// the answer self-righting: a path that moves the line and forgets to re-arm
// (reLive) is wrong for one tick rather than wrong until playback climbs all
// the way back to where it had been. The high-water mark used to be monotonic
// for the life of the page, so "until it climbs back" meant "for good" on any
// jump the stream never repeated.
func TestTheLiveClockCannotOutrunTheLastPositionRead(t *testing.T) {
	ed := jumpEd()
	high := ed.livePlayhead()
	if high < 724 {
		t.Fatalf("the live clock reads %.3f while the stream plays 724", high)
	}
	// the line goes back, the mark is lowered, and the stale position drags
	// the mark straight back up to where the line came from
	ed.playhead = 719
	ed.liveMax = 719
	if got := ed.livePlayhead(); got < 724 {
		t.Fatalf("this test no longer reproduces the stale base: %.3f", got)
	}
	// ...and then the next tick reads the real position, at the line
	ed.posT, ed.posAt = 719, time.Now()
	got := ed.livePlayhead()
	if got > 719+float64(playTick)/1000+1e-9 {
		t.Errorf("the stream is at 719 and the live clock still reads %.3f -- "+
			"a tick past it is %.3f", got, 719+float64(playTick)/1000)
	}
	if got < 719 {
		t.Errorf("the live clock fell behind the stream: %.3f at 719", got)
	}
	// the jitter it exists to hide is still hidden: a read landing a few
	// milliseconds behind the extrapolation does not pull the picture back
	ed.posT, ed.posAt = 719, time.Now().Add(-40*time.Millisecond)
	ahead := ed.livePlayhead()
	ed.posT, ed.posAt = 719.005, time.Now()
	if back := ed.livePlayhead(); back < ahead-1e-9 {
		t.Errorf("a lagging read pulled the clock back from %.4f to %.4f", ahead, back)
	}
}

// The three parts of the live clock move together. Re-basing the mark alone
// was the bug: a seek into the file already open does not stop playback, so
// the next read extrapolated from the position the line had BEFORE the jump.
func TestReArmingTheLiveClockMovesAllOfIt(t *testing.T) {
	ed := jumpEd()
	ed.livePlayhead()
	before := time.Now()
	ed.reLive(300)
	if ed.liveMax != 300 || ed.posT != 300 {
		t.Errorf("reLive(300) left the clock at mark %.3f, base %.3f", ed.liveMax, ed.posT)
	}
	if ed.posAt.Before(before) {
		t.Error("reLive kept the wall clock of the reading it replaced")
	}
	if got := ed.livePlayhead(); math.Abs(got-300) > float64(playTick)/1000 {
		t.Errorf("straight after reLive(300) the clock reads %.3f", got)
	}
}

// Every hand on the line re-arms it, and a frame step is a hand on the line.
// Pinned to the source because both paths seek a real pipeline.
func TestEveryHandOnTheLineReArmsTheLiveClock(t *testing.T) {
	src := readSrc(t, "cut.go")
	for _, want := range []string{
		// the line placed outright
		"ed.reLive(t) // the live clock is re-based with the line; see livePlayhead",
		// and nudged a frame at a time
		"ed.reLive(ed.playhead) // a hand on the line: the live clock comes with it",
		// playback's own advance keeps writing the base, and only the base
		"ed.posT, ed.posAt = ed.playhead, time.Now()",
	} {
		if !strings.Contains(src, want) {
			t.Errorf("cut.go no longer contains %q", want)
		}
	}
	// a frame step lands in a stretch with its own clock: the rate goes on
	// before the seek, because a rate only takes hold at one
	i, j := strings.Index(src, "ed.reLive(ed.playhead)"), strings.Index(src, "ed.player.SeekTo(local)")
	k := strings.Index(src, "ed.player.SetRate(fxPreviewRateAt(ed.fx, ed.playhead))")
	if i < 0 || j < 0 || k < 0 || !(i < k && k < j) {
		t.Errorf("frameStep does not re-arm the clock and set the rate before its seek "+
			"(reLive at %d, SetRate at %d, SeekTo at %d)", i, k, j)
	}
}
