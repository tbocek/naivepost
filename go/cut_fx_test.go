package main

// The effects model: the camera function the preview and the render share, the
// clock arithmetic that turns speed effects into segments, and the wiring that
// puts all of it under the same fingers as the rest of the cut page.

import (
	"encoding/json"
	"math"
	"os"
	"strings"
	"testing"
)

func rectNear(a, b fxRect) bool {
	return math.Abs(a.cx-b.cx) < 1e-9 && math.Abs(a.cy-b.cy) < 1e-9 && math.Abs(a.hf-b.hf) < 1e-9
}

// The first region nobody has to place: the output frame carved out of the
// middle of the source at the footage's own scale. For a short cut from
// widescreen that is the full height with both sides cropped (hf = 1) --
// NOT the letterboxed whole frame, which shrank the picture to a stamp and
// made every 9:16 cut open on a mismatched shape. The sides are the user's
// to bring back: a view picks WHICH slice, the default only picks the middle.
func TestTheCameraStartsOnTheMiddleSlice(t *testing.T) {
	srcA := 16.0 / 9
	if got := fullFill(srcA, srcA); !rectNear(got, fxRect{0.5, 0.5, 1}) {
		t.Errorf("same aspect in and out gives %+v, want the exact frame", got)
	}
	// widescreen into a short: full height, sides cropped, picture unshrunk
	if got := fullFill(srcA, 9.0/16); !rectNear(got, fxRect{0.5, 0.5, 1}) {
		t.Errorf("widescreen into 9:16 gives %+v, want the centred full-height slice", got)
	}
	// ...and the other way round: full width, top and bottom cropped
	got := fullFill(9.0/16, srcA)
	if want := (9.0 / 16) / srcA; math.Abs(got.hf-want) > 1e-9 || got.cx != 0.5 || got.cy != 0.5 {
		t.Errorf("portrait into 16:9 gives %+v, want centred hf=%g", got, want)
	}
	// and with no effects at all, that region is the camera, forever
	if got := fxRectAt(nil, 123, srcA, 9.0/16); !rectNear(got, fullFill(srcA, 9.0/16)) {
		t.Errorf("an effect-free camera wanders: %+v", got)
	}
}

// Camera moves chain through their fades: one placed while the camera is still
// travelling starts from where the camera actually is, mid-flight, not from
// where the previous one was going to end up. Anything else makes the picture
// jump at the moment the second one begins.
func TestZoomsChainFromMidGlide(t *testing.T) {
	srcA, outA := 16.0/9, 16.0/9 // same shape, so the base is the plain frame
	// z leads so the opening rule frames the start on z's rect -- which IS the
	// plain frame, keeping every departure below exactly fullFill
	z := cutFx{Kind: "zoom", T: 1, Stay: true, Cx: 0.5, Cy: 0.5, Hf: 1}
	a := cutFx{Kind: "zoom", T: 10, Trans: 2, Dur: 2, Stay: true, Cx: 0.3, Cy: 0.3, Hf: 0.5}
	b := cutFx{Kind: "zoom", T: 11, Trans: 1, Dur: 1, Stay: true, Cx: 0.7, Cy: 0.6, Hf: 0.4}
	fx := []cutFx{z, a, b}

	// a quarter into A's fade in, B not yet begun
	got := fxRectAt(fx, 10.5, srcA, outA)
	want := lerpRect(fullFill(srcA, outA), fxRect{a.Cx, a.Cy, a.Hf}, 0.25)
	if !rectNear(got, want) {
		t.Errorf("mid-glide the camera is %+v, want %+v", got, want)
	}
	// B begins at 11, when A's fade in is only half done: B must depart from
	// that half-way point
	depart := lerpRect(fullFill(srcA, outA), fxRect{a.Cx, a.Cy, a.Hf}, 0.5)
	got = fxRectAt(fx, 11.5, srcA, outA)
	want = lerpRect(depart, fxRect{b.Cx, b.Cy, b.Hf}, 0.5)
	if !rectNear(got, want) {
		t.Errorf("the second zoom departs from %+v, want the mid-flight camera %+v", got, want)
	}
	// and settles on its own rectangle
	if got := fxRectAt(fx, 30, srcA, outA); !rectNear(got, fxRect{b.Cx, b.Cy, b.Hf}) {
		t.Errorf("settled camera is %+v, want the last zoom", got)
	}
}

// A staying zoom holds the camera for the rest of the video and for none of it
// before. It used to reach backwards -- the earliest one framed the video from
// second zero, on the theory that a region chosen a minute in was a statement
// about the whole thing -- and every time it was read as the zoom being stuck:
// scrub to a second BEFORE the effect and the picture was already inside it.
// The framing starts where its band starts, like every other effect on the
// lane, and the seconds ahead of it wear the plain centred slice.
func TestAStayingZoomFramesNothingBeforeItself(t *testing.T) {
	srcA, outA := 16.0/9, 9.0/16
	full := fullFill(srcA, outA)
	v := cutFx{Kind: "zoom", T: 30, Dur: 2, Stay: true, Cx: 0.3, Cy: 0.5, Hf: 0.6}
	vr := fxRect{v.Cx, v.Cy, v.Hf}
	for _, tt := range []float64{0, 29.9} {
		if got := fxRectAt([]cutFx{v}, tt, srcA, outA); !rectNear(got, full) {
			t.Errorf("at %.1f s, before the framing, the camera is %+v, want fullFill %+v",
				tt, got, full)
		}
	}
	for _, tt := range []float64{30, 31, 100} {
		if got := fxRectAt([]cutFx{v}, tt, srcA, outA); !rectNear(got, vr) {
			t.Errorf("at %.1f s the camera is %+v, want the framing %+v", tt, got, vr)
		}
	}
	// and the fade in of that first framing is a real journey now, from the
	// centred slice to the region, not a number with nowhere to travel
	g := cutFx{Kind: "zoom", T: 30, Trans: 2, Dur: 4, Stay: true, Cx: 0.3, Cy: 0.5, Hf: 0.6}
	if got, want := fxRectAt([]cutFx{g}, 31, srcA, outA),
		lerpRect(full, vr, 0.5); !rectNear(got, want) {
		t.Errorf("halfway into the first framing's glide the camera is %+v, want %+v", got, want)
	}
	if !camMoving([]cutFx{g}, 31) {
		t.Error("the first framing's glide does not count as the camera moving")
	}
	// a second framing still glides away from the first, exactly as before
	w := cutFx{Kind: "zoom", T: 50, Trans: 2, Dur: 2, Stay: true, Cx: 0.7, Cy: 0.5, Hf: 0.4}
	got := fxRectAt([]cutFx{v, w}, 51, srcA, outA)
	want := lerpRect(vr, fxRect{w.Cx, w.Cy, w.Hf}, 0.5)
	if !rectNear(got, want) {
		t.Errorf("mid-glide to the second framing the camera is %+v, want %+v", got, want)
	}
	// a zoom that comes back out leaves the opening alone too
	pull := cutFx{Kind: "zoom", T: 30, Trans: 2, Dur: 6, Tout: 2, Cx: 0.3, Cy: 0.5, Hf: 0.6}
	if got := fxRectAt([]cutFx{pull}, 0, srcA, outA); !rectNear(got, full) {
		t.Errorf("a pull-back zoom framed the opening: %+v, want fullFill", got)
	}
	// and no camera effects at all still means the letterboxed full frame
	if got := fxRectAt(nil, 0, srcA, outA); !rectNear(got, full) {
		t.Errorf("with no zooms the camera is %+v, want fullFill", got)
	}
}

// hasStay is whether the cut has been given a lasting framing at all, which is
// what the first box drawn on a reshaped cut opens as.
func TestALastingFramingIsKnownFromAPassingOne(t *testing.T) {
	ed := &cutEditor{fx: []cutFx{
		{Kind: "zoom", T: 20, Stay: true, Cx: 0.5, Cy: 0.5, Hf: 0.5},
		{Kind: "zoom", T: 5, Dur: 2, Cx: 0.5, Cy: 0.5, Hf: 0.3},
	}}
	if !ed.hasStay() {
		t.Error("a cut with a staying zoom says it has no lasting framing")
	}
	if (&cutEditor{fx: []cutFx{{Kind: "zoom", T: 5, Dur: 2}}}).hasStay() {
		t.Error("a cut whose only zoom pulls back claims a lasting framing")
	}
}

// An old cut.json speaks the two-kind language: a "view" was a region the
// camera kept, with its hold stored WITHOUT the glides around it. It has to
// come back as the same finished video.
func TestAnOldViewLoadsAsAStayingZoom(t *testing.T) {
	got := migrateFx([]cutFx{
		{Kind: "view", T: 30, Trans: 2, Dur: 5, Cx: 0.3, Cy: 0.5, Hf: 0.6},
		{Kind: "view", T: 60, Trans: 1, Dur: 4, Tout: 2, Cx: 0.7, Cy: 0.5, Hf: 0.5},
		{Kind: "zoom", T: 90, Trans: 1, Dur: 3, Tout: 1},
		{Kind: "text", T: 5, Dur: 2, Text: "hi"},
	})
	// the ordinary view: it stays, and its band is everything it owned
	if got[0].Kind != "zoom" || !got[0].Stay || got[0].Dur != 7 || got[0].Trans != 2 {
		t.Errorf("a plain view loaded as %+v, want a 7 s staying zoom", got[0])
	}
	// one that glided back out was already a zoom that pulls back
	if got[1].Kind != "zoom" || got[1].Stay || got[1].Dur != 7 || got[1].Tout != 2 {
		t.Errorf("a view with a glide out loaded as %+v, want a 7 s pull-back", got[1])
	}
	// and the kinds that never changed are left exactly alone
	if got[2].Dur != 3 || got[2].Stay || got[3].Kind != "text" || got[3].Dur != 2 {
		t.Errorf("migration touched what it had no business touching: %+v %+v", got[2], got[3])
	}
}

// A zoom borrows the camera and gives it back: in over Trans, hold, out over
// Tout -- each glide its own number -- and after Dur the camera is exactly
// where the views say it belongs; the zoom leaves no trace.
func TestAZoomBorrowsTheCameraAndGivesItBack(t *testing.T) {
	srcA, outA := 16.0/9, 16.0/9
	base := fullFill(srcA, outA)
	z := cutFx{Kind: "zoom", T: 20, Trans: 1, Tout: 1, Dur: 4, Cx: 0.6, Cy: 0.4, Hf: 0.3}
	fx := []cutFx{z}
	zr := fxRect{z.Cx, z.Cy, z.Hf}

	for _, c := range []struct {
		t    float64
		want fxRect
	}{
		{19.9, base},                   // not yet
		{20, base},                     // the very first instant is the departure
		{20.5, lerpRect(base, zr, .5)}, // half-way in
		{22, zr},                       // holding
		{23.5, lerpRect(base, zr, .5)}, // half-way back out
		{24, base},                     // given back, to the second
	} {
		if got := fxRectAt(fx, c.t, srcA, outA); !rectNear(got, c.want) {
			t.Errorf("at %.1f s the camera is %+v, want %+v", c.t, got, c.want)
		}
	}

	// the glides are independent: in over 1 s, out over 2, and a Tout of 0 is
	// a hard cut back -- still zoomed at the last covered instant, base after
	asym := cutFx{Kind: "zoom", T: 40, Trans: 1, Tout: 2, Dur: 6, Cx: 0.6, Cy: 0.4, Hf: 0.3}
	hard := cutFx{Kind: "zoom", T: 60, Trans: 1, Dur: 3, Cx: 0.6, Cy: 0.4, Hf: 0.3}
	fx = []cutFx{asym, hard}
	for _, c := range []struct {
		t    float64
		want fxRect
	}{
		{40.5, lerpRect(base, zr, .5)}, // half of the 1 s way in
		{43, zr},                       // holding: the out has not begun at 44
		{45, lerpRect(base, zr, .5)},   // half of the 2 s way out
		{46, base},                     // over
		{62.9, zr},                     // no out-glide: zoomed to the end...
		{63, base},                     // ...and cut straight back
	} {
		if got := fxRectAt(fx, c.t, srcA, outA); !rectNear(got, c.want) {
			t.Errorf("at %.1f s the camera is %+v, want %+v", c.t, got, c.want)
		}
	}
}

// Slow motion splits the footage exactly at its own boundaries: the stretch
// inside [T, T+Dur) carries the rate, the rest of the clip plays as itself,
// and the lengths say what the finished video will actually run.
func TestSlowMotionSplitsAtItsOwnBoundaries(t *testing.T) {
	segs := []cutSeg{{S: 0, E: 60}}
	fx := []cutFx{{Kind: "speed", T: 10, Dur: 5, Rate: 0.5}}
	got := applyFx(segs, fx)
	want := []cutSeg{{S: 0, E: 10}, {S: 10, E: 15, Rate: 0.5}, {S: 15, E: 60}}
	if len(got) != len(want) {
		t.Fatalf("applyFx gave %v, want %v", got, want)
	}
	for i := range want {
		if !sameSeg(got[i], want[i]) {
			t.Errorf("seg %d is %+v, want %+v", i, got[i], want[i])
		}
	}
	// 5 s of footage at half speed runs 10 s on screen
	if l := got[1].length(); math.Abs(l-10) > 1e-9 {
		t.Errorf("the slowed stretch runs %.1f s, want 10", l)
	}
	// an effect whose scene was cut away does nothing, silently -- the segs
	// come through untouched and the effect waits for Undo to matter again
	elsewhere := applyFx([]cutSeg{{S: 30, E: 40}}, fx)
	if len(elsewhere) != 1 || !sameSeg(elsewhere[0], cutSeg{S: 30, E: 40}) {
		t.Errorf("an orphaned slow rewrote the cut: %v", elsewhere)
	}
}

// The same split, the other way round the clock. A capture is mostly dead air,
// and 100x is what makes twenty minutes of it watchable: nothing on this side
// of the render changes except the rate the segment carries, and the length it
// gives the finished video. What needs guarding is the far end -- fast enough
// over few enough seconds and the clip comes out shorter than the frames it is
// made of, which the render drops, which is no effect at all.
func TestFastMotionRunsTheSameSplitTheOtherWay(t *testing.T) {
	segs := []cutSeg{{S: 0, E: 600}}
	got := applyFx(segs, []cutFx{{Kind: "speed", T: 100, Dur: 300, Rate: 100}})
	want := []cutSeg{{S: 0, E: 100}, {S: 100, E: 400, Rate: 100}, {S: 400, E: 600}}
	if len(got) != len(want) {
		t.Fatalf("applyFx gave %v, want %v", got, want)
	}
	for i := range want {
		if !sameSeg(got[i], want[i]) {
			t.Errorf("seg %d is %+v, want %+v", i, got[i], want[i])
		}
	}
	// five minutes of footage at a hundred times: three seconds of video
	if l := got[1].length(); math.Abs(l-3) > 1e-9 {
		t.Errorf("the fast stretch runs %.2f s, want 3", l)
	}

	// what the dialog does with what was typed into it: the ceiling, the
	// floor, and the case where the two ends fight -- there it is the RATE
	// that gives way, because the seconds are the ones the user marked and
	// can see on the timeline
	for _, c := range []struct{ rate, dur, wantR, wantD float64 }{
		{0.5, 5, 0.5, 5},          // untouched
		{1000, 60, fxMaxRate, 60}, // past the ceiling
		{0, 5, fxMinRate, 5},      // an empty entry must not become a freeze
		{100, 10, 20, 10},         // 10 s at ×100 is 0.1 s on screen: ×20 instead
		{100, 0, 1, minClipLn},    // no length at all: the shortest band, at its own speed
	} {
		r, d := clampSpeed(c.rate, c.dur)
		if math.Abs(r-c.wantR) > 1e-9 || math.Abs(d-c.wantD) > 1e-9 {
			t.Errorf("clampSpeed(%g, %g) = %g over %gs, want %g over %gs",
				c.rate, c.dur, r, d, c.wantR, c.wantD)
		}
		if d/r < fxMinPlay-1e-9 {
			t.Errorf("clampSpeed(%g, %g) leaves %.3f s of video, under the %g floor",
				c.rate, c.dur, d/r, fxMinPlay)
		}
	}

	// the lane plate is two characters wide and the status line is read by a
	// human: %g spells a hundred "1e+02", and the one decimal every other
	// field uses spells a quarter speed "0.2", which is a different effect
	for _, c := range []struct {
		v    float64
		want string
	}{{100, "100"}, {0.25, "0.25"}, {1.5, "1.5"}, {fxMinRate, "0.05"}, {1, "1"}} {
		if got := fxNum(c.v); got != c.want {
			t.Errorf("fxNum(%g) = %q, want %q", c.v, got, c.want)
		}
	}
	fast := cutFx{Kind: "speed", T: 90, Dur: 300, Rate: 100}.fxLabel()
	if !strings.Contains(fast, "sped up ×100") {
		t.Errorf("a 100x effect reads %q", fast)
	}
	slow := cutFx{Kind: "speed", T: 90, Dur: 5, Rate: 0.25}.fxLabel()
	if !strings.Contains(slow, "slowed ×0.25") {
		t.Errorf("a quarter-speed effect reads %q", slow)
	}
}

// The frame buttons with an effect held. Two things went wrong at once here:
// the nudge moved the effect and nothing on screen moved with it -- one frame
// is a fraction of a pixel on the lane and the label reads the same to the
// second, so ‹‹f looked like a dead button -- and a hold left over an Undo
// swallowed the press instead of giving it back to the playhead.
func TestTheFrameButtonsWithAnEffectHeldMoveTheLineWithIt(t *testing.T) {
	ed := newTestEd(t)
	ed.vids = []tlVideo{{base: "v", path: "v.mkv", start: 0, dur: 600, interval: 5, fps: 30}}
	ed.relayout()
	ed.segs = []cutSeg{{S: 0, E: 600}}
	ed.addFx(cutFx{Kind: "speed", T: 100, Dur: 20, Rate: 8})
	if ed.heldFx() == nil {
		t.Fatal("a placed effect is not the one in hand")
	}

	ed.frameStep(-15) // 30 fps: half a second back
	if got := ed.fx[0].T; math.Abs(got-99.5) > 1e-9 {
		t.Errorf("‹‹f moved the effect to %g, want 99.5", got)
	}
	// and the red line came with it, which is the only visible answer the
	// press has: the effect itself moved half a pixel
	if !ed.hasPlay || math.Abs(ed.playhead-99.5) > 1e-9 {
		t.Errorf("the playhead is at %g (set=%v), want it on the effect at 99.5",
			ed.playhead, ed.hasPlay)
	}
	// two nudges of one held effect are one entry in the history, the same
	// bargain the edges and clips make
	ed.frameStep(+15)
	if len(ed.undo) != 2 { // the place, then the move
		t.Errorf("placing and nudging left %d undo entries, want 2", len(ed.undo))
	}

	// an Undo (or a re-cut, or another project) can leave the hold pointing at
	// an effect that is no longer there. The nudge has to SAY it moved nothing,
	// which is what lets frameStep give the press back to the playhead instead
	// of dropping it -- and it has to let the stale hold go.
	ed.fx = nil
	if ed.nudgeFx(+1) {
		t.Error("nudging a vanished effect claimed to have moved it")
	}
	if ed.fxOn {
		t.Error("a hold on a vanished effect survived the press")
	}
}

// A stop leaves the cut alone: it is an overlay over running footage
// (freezeCues), not a cut, so the segments come through applyFx untouched and
// the cut is the same length with or without it. A spliced card still stands
// in the clip it cut open as a zero-span, Dur-long segment, and the seek bar
// knows both shapes.
func TestAStopLeavesTheSegmentsAlone(t *testing.T) {
	got := applyFx([]cutSeg{{S: 0, E: 60}}, []cutFx{{Kind: "speed", T: 20, Dur: 2}})
	if len(got) != 1 || !sameSeg(got[0], cutSeg{S: 0, E: 60}) {
		t.Fatalf("a stop cut the segments open: %v", got)
	}
	// slow motion still cuts its stretch out and rates it, and a stop laid
	// ACROSS it is averaged in as the ×0 it is (cut_speedmix.go): ×0.5 and
	// ×0 over the same two seconds come out ×0.25, and the picture runs --
	// diluted, the stop no longer freezes anything
	got = applyFx([]cutSeg{{S: 0, E: 60}}, []cutFx{
		{Kind: "speed", T: 10, Dur: 10, Rate: 0.5},
		{Kind: "speed", T: 15, Dur: 2},
	})
	want := []cutSeg{{S: 0, E: 10}, {S: 10, E: 15, Rate: 0.5},
		{S: 15, E: 17, Rate: 0.25}, {S: 17, E: 20, Rate: 0.5}, {S: 20, E: 60}}
	if len(got) != len(want) {
		t.Fatalf("slow motion crossed by a stop gave %v, want %v", got, want)
	}
	for i := range want {
		if !sameSeg(got[i], want[i]) {
			t.Errorf("seg %d is %+v, want %+v", i, got[i], want[i])
		}
	}

	// the seek bar: a spliced card is a point of the session however far into
	// its bar-time the handle lands, and slowed footage maps back through its
	// rate -- x bar seconds cover x*rate session seconds
	segs := []cutSeg{{S: 0, E: 10}, {S: 10, E: 15, Rate: 0.5},
		{S: 15, E: 15, Dur: 2, Ins: "card.svg"}, {S: 15, E: 20, Rate: 0.5}, {S: 20, E: 60}}
	if at := cutAt(segs, 10+3); math.Abs(at-(10+3*0.5)) > 1e-9 {
		t.Errorf("3 s into the slowed stretch is session %.2f, want %.2f", at, 10+1.5)
	}
	if at := cutAt(segs, 20+1); at != 15 { // inside the card
		t.Errorf("inside the card the session time is %.2f, want the spliced moment 15", at)
	}
	// session 17: 10 s kept + 10 s of slowed footage + the 2 s card + 2 s
	// more at half speed = 26 on the bar
	if pos := cutPos(segs, 17); math.Abs(pos-26) > 1e-9 {
		t.Errorf("session 17 is %.2f on the bar, want 26", pos)
	}
}

// The cut file: effects and aspect go to disk with the cut and come back
// through the same door the render reads (produceCut) -- and a cut with no
// effects writes byte-for-byte the file it always wrote, so nothing that
// diffs, hashes or hand-reads cut.json sees this feature until it is used.
func TestEffectsGoToDiskAndAPlainCutDoesNotChange(t *testing.T) {
	ed := newTestEd(t)
	ed.segs = []cutSeg{{S: 5, E: 25}}
	ed.persist()
	b, err := os.ReadFile(ed.a.cutPath())
	if err != nil {
		t.Fatal(err)
	}
	plain, _ := json.MarshalIndent(struct {
		Segs []cutSeg `json:"segs"`
	}{ed.segs}, "", "  ")
	if string(b) != string(plain)+"\n" {
		t.Errorf("an effect-free cut writes:\n%s\nwant the format it always had:\n%s", b, plain)
	}

	ed.aspect = "9:16"
	ed.fx = []cutFx{
		{Kind: "zoom", T: 8, Trans: 1, Dur: 1, Stay: true, Cx: 0.4, Cy: 0.5, Hf: 0.6},
		{Kind: "speed", T: 12, Dur: 3, Rate: 0.5},
	}
	ed.persist()
	c := ed.a.produceCut() // a.ed is nil: this is the file speaking
	segs, fx, aspect := c.Segs, c.Fx, c.Aspect
	if len(segs) != 1 || !sameSeg(segs[0], ed.segs[0]) {
		t.Errorf("the cut came back as %v", segs)
	}
	if aspect != "9:16" || len(fx) != 2 || fx[0] != ed.fx[0] || fx[1] != ed.fx[1] {
		t.Errorf("the effects came back as aspect=%q fx=%v", aspect, fx)
	}
}

// The new nouns answer to the old verbs: the lane picks through the same left
// press as clips and edges, a held effect is moved by the drag and nudged by
// ‹f f›, Remove removes it, the Insert button turns into ✎ Edit for it, and the
// overlay hangs over the player's own picture.
// Pinned as source, like the edge tool before it, because a regression here is
// a gesture that silently stops working.
func TestTheEffectsLaneIsWired(t *testing.T) {
	b, err := os.ReadFile("cut.go")
	if err != nil {
		t.Fatal(err)
	}
	src := string(b)
	for _, want := range []string{
		// the press that trims is for clips and edges only: in the effects lane
		// it declines, because down there a press is the effect's
		"if area == ed.srcArea && ed.fxHitLane(y) {",
		"hover.ConnectMotion(func(x, y float64) { ed.hoverTracks(x, y) })",
		"hover.ConnectLeave(func() { ed.hoverTracks(-1, -1) })",
		// the left drag picks the effect under it up and slides it, snapped
		// by BOTH ends to the cuts and the other effects
		"if !ed.fxOn || ed.fxSel != i {",
		"ed.moveFxTo(ed.snapFxSpan(ed.tAtView(dragStartX+ox)-grabAt, t1-t0), true)",
		// ‹f f› nudge it, Del removes it, Esc puts it down
		"ed.nudgeFx(n)",
		"case ed.heldFx() != nil:",
		"|| ed.selOn || ed.copyOn || ed.fxArm != \"\") && keyval == gdk.KEY_Escape:",
		// the one button that edits whatever is held edits effects too
		"a.editFx()",
		// the lane is drawn inside the source track, the overlay over the picture
		"ed.drawFxLane(cr, vx0, vx1)",
		"vframe := videoFrame(ed.buildFxOverlay())",
		// and the aspect is a dropdown on the toolbar, not a dialog
		"ed.aspectDD = gtk.NewDropDownFromStrings(fxAspects)",
		// the effects are one dropdown of verbs -- and only five, because a
		// stop is a speed of x0 rather than an entry of its own
		"fxDD := gtk.NewDropDownFromStrings(fxKinds)",
		`fxKinds := []string{"✚ Effect", "⊕ Zoom", "❝ Text", "▨ SVG", "⏩ Speed", "🔊 Volume", "🏷 Label"}`,
	} {
		if !strings.Contains(src, want) {
			t.Errorf("the cut page no longer contains %q", want)
		}
	}
	// undo is one stack for everything on the page: segments, effects and
	// aspect travel together or Undo tears them apart
	for _, f := range []string{"cut.go"} {
		b, _ := os.ReadFile(f)
		if !strings.Contains(string(b), "type cutState struct") {
			t.Errorf("%s no longer snapshots segs, fx and aspect as one undo state", f)
		}
	}
}

// A speed effect that ramps. The render holds one rate per clip, so a rate
// that changes has to arrive as a staircase; these are the stairs.
//
// The two things worth pinning are the shape and the arithmetic. The shape:
// the stairs tile the effect's span exactly, with no gap and no overlap, or
// footage goes missing between them. The arithmetic: the rates step
// geometrically, so the middle of a ×1 → ×4 ramp is ×2 and not ×2.5.
func TestASpeedRampArrivesAsStairs(t *testing.T) {
	f := cutFx{Kind: "speed", T: 10, Dur: 12, Rate: 4, Trans: 3, Tout: 3}
	steps := speedSteps(f)
	if len(steps) < 3 {
		t.Fatalf("a ramped speed came back as %d steps, want a way up, a middle and a way down", len(steps))
	}
	// they tile [T, T+Dur) end to end
	if math.Abs(steps[0].t0-10) > 1e-9 {
		t.Errorf("the stairs start at %.3f, want 10", steps[0].t0)
	}
	if last := steps[len(steps)-1]; math.Abs(last.t1-22) > 1e-9 {
		t.Errorf("the stairs end at %.3f, want 22", last.t1)
	}
	for i := 1; i < len(steps); i++ {
		if math.Abs(steps[i].t0-steps[i-1].t1) > 1e-9 {
			t.Errorf("a gap between stairs %d and %d: %.3f to %.3f", i-1, i, steps[i-1].t1, steps[i].t0)
		}
		// measured where the floor is: on screen, after the rate has had the
		// footage. A stair of a ×4 ramp covers four times what it lasts.
		if out := (steps[i].t1 - steps[i].t0) / steps[i].rate; out < minClipLn-1e-9 {
			t.Errorf("stair %d covers %.3fs at ×%.2f — %.3fs on screen, under the "+
				"render's floor of %.1fs", i, steps[i].t1-steps[i].t0, steps[i].rate,
				out, minClipLn)
		}
	}
	// the way up climbs towards the rate without reaching it, the middle is
	// the rate, and the way down comes back
	if steps[0].rate <= 1 || steps[0].rate >= 4 {
		t.Errorf("the first stair runs at ×%.3f, want something between 1 and 4", steps[0].rate)
	}
	var flat bool
	for _, s := range steps {
		if math.Abs(s.rate-4) < 1e-9 {
			flat = true
		}
	}
	if !flat {
		t.Error("no stair runs at the rate that was asked for")
	}
	if last := steps[len(steps)-1]; last.rate <= 1 || last.rate >= 4 {
		t.Errorf("the last stair runs at ×%.3f, want something between 1 and 4", last.rate)
	}
	// geometric, not linear: a ×1 → ×4 ramp in one stair sits at its middle,
	// which is ×2 -- ×2.5 would be the linear answer and the wrong one. One
	// stair of a ramp to ×4 costs 1.2 s of footage (rampStep×√4), so this is
	// two ramps of exactly one stair each and no flat middle between them.
	one := speedSteps(cutFx{Kind: "speed", T: 0, Dur: 2.4, Rate: 4, Trans: 1.2, Tout: 1.2})
	if len(one) != 2 {
		t.Fatalf("a two-stair ramp came back as %d stairs", len(one))
	}
	if math.Abs(one[0].rate-2) > 1e-9 {
		t.Errorf("the stair from ×1 to ×4 runs at ×%.4f, want the geometric middle ×2", one[0].rate)
	}

	// a ramp too short for one stair is dropped rather than shrunk: half a
	// stair of footage is footage the render would throw away entirely
	short := speedSteps(cutFx{Kind: "speed", T: 0, Dur: 4, Rate: 2, Trans: 0.1, Tout: 0.1})
	if len(short) != 1 || math.Abs(short[0].rate-2) > 1e-9 {
		t.Errorf("a sub-stair ramp became %+v, want one flat stretch at the rate", short)
	}

	// and no ramp at all is the single span it always was
	plain := speedSteps(cutFx{Kind: "speed", T: 5, Dur: 4, Rate: 0.5})
	if len(plain) != 1 || plain[0].t0 != 5 || plain[0].t1 != 9 || plain[0].rate != 0.5 {
		t.Errorf("an unramped speed became %+v, want one span of 5–9 at ×0.5", plain)
	}
}

// The ramps reach the cut, not just the model: applyFx runs every stair
// through rateSpan, so the footage under a ramped effect comes out as clips
// carrying a spread of rates rather than one.
func TestARampedSpeedSplitsTheFootage(t *testing.T) {
	// nine seconds of ramp at ×4 buys three stairs either side of the flat
	// middle: a stair there costs 2.4 s of footage, and the spread is the
	// point of the test
	segs := []cutSeg{{S: 0, E: 60}}
	out := applyFx(segs, []cutFx{{Kind: "speed", T: 5, Dur: 30, Rate: 4, Trans: 9, Tout: 9}})
	rates := map[float64]bool{}
	var covered float64
	for _, s := range out {
		if s.Rate > 0 && s.S >= 5 && s.E <= 35 {
			rates[s.Rate] = true
			covered += s.E - s.S
		}
	}
	if len(rates) < 3 {
		t.Errorf("the ramped stretch came out at %d distinct rates, want a spread", len(rates))
	}
	if math.Abs(covered-30) > 1e-9 {
		t.Errorf("the rated clips cover %.3fs of the 30s the effect asked for", covered)
	}
	// the footage on either side is untouched and still whole
	var head, tail float64
	for _, s := range out {
		if s.E <= 5 {
			head += s.E - s.S
		}
		if s.S >= 35 {
			tail += s.E - s.S
		}
	}
	if math.Abs(head-5) > 1e-9 || math.Abs(tail-25) > 1e-9 {
		t.Errorf("the footage around the ramp is %.3fs before and %.3fs after, want 5 and 25", head, tail)
	}
}

// The clock the preview runs on. It has to be the same walk applyFx makes over
// the same stairs, or the editor plays a stretch at one speed and Produce
// renders it at another -- which is the kind of disagreement you only find
// after the render.
func TestTheRateUnderTheLineIsTheRenderedOne(t *testing.T) {
	fx := []cutFx{
		{Kind: "speed", T: 10, Dur: 12, Rate: 4, Trans: 3, Tout: 3},
		{Kind: "speed", T: 40, Dur: 4, Rate: 0.5},
		{Kind: "speed", T: 60, Dur: 2}, // a stop frame: not a rate at all
	}
	for _, c := range []struct {
		t    float64
		want float64
	}{
		{0, 1},    // clear of everything
		{9.99, 1}, // right up to the edge
		{22, 1},   // and the moment it ends
		{16, 4},   // the flat middle of the ramped one
		{40, 0.5}, // its own moment counts as inside
		{43.9, 0.5},
		{44, 1},  // the moment it ends
		{60, 1},  // a stop frame leaves the clock alone
		{600, 1}, // past the lot
	} {
		if got := fxRateAt(fx, c.t); math.Abs(got-c.want) > 1e-9 {
			t.Errorf("at %.2fs the preview runs at ×%.4f, want ×%g", c.t, got, c.want)
		}
	}
	// inside a ramp it is neither end but somewhere between them, and it
	// matches the stair applyFx would have put there
	up := fxRateAt(fx, 11)
	if up <= 1 || up >= 4 {
		t.Errorf("partway up the ramp the preview runs at ×%.4f, want between 1 and 4", up)
	}
	var found bool
	for _, st := range speedSteps(fx[0]) {
		if 11 >= st.t0 && 11 < st.t1 && math.Abs(st.rate-up) < 1e-12 {
			found = true
		}
	}
	if !found {
		t.Error("the preview's rate is not one of the stairs the render will make")
	}
}

// The preview is put on that clock, and the pieces that make it work are all
// there. Pinned as source: a rate that quietly stops being carried is a
// preview that quietly stops previewing.
func TestThePreviewClockIsWired(t *testing.T) {
	b, err := os.ReadFile("player.go")
	if err != nil {
		t.Fatal(err)
	}
	src := string(b)
	if n := strings.Count(src, "Seek(1.0, gst.FormatTime"); n > 0 {
		t.Errorf("%d seeks are still pinned to rate 1 — they will not carry a speed effect", n)
	}
	for _, want := range []string{
		// the master, the deferred segment and every separate recording
		"p.pb.Seek(p.rate, gst.FormatTime",
		"a.pb.Seek(a.rate, gst.FormatTime",
		// pitch held, the same trade atempo makes in the render -- on every
		// pipeline, through the one audio path they share (audioFilter)
		`gst.ElementFactoryMake("scaletempo", name+"tempo")`,
		`pb.SetObjectProperty("audio-filter", filter)`,
		// and ▶ notices a rate chosen while paused
		"if math.Abs(p.rate-p.seekRate) > 1e-6 {",
	} {
		if !strings.Contains(src, want) {
			t.Errorf("the player no longer contains %q", want)
		}
	}
	b, err = os.ReadFile("cut.go")
	if err != nil {
		t.Fatal(err)
	}
	src = string(b)
	for _, want := range []string{
		"ed.player.SetRate(fxPreviewRateAt(ed.fx, t))",              // set before the seek that carries it
		"ed.syncPlayRate()",                                         // and again as the line runs
		"ed.player.SetRateNow(fxPreviewRateAt(ed.fx, ed.playhead))", // ...without a flush per boundary
	} {
		if !strings.Contains(src, want) {
			t.Errorf("the cut page no longer contains %q", want)
		}
	}
}

// A speed boundary crossed while the preview runs is a flushing seek to where
// the stream already is, and never GStreamer's instant rate change. The
// instant seek was tried first for a while, to spare the stumble -- and on this
// stack (playbin3, scaletempo, gtk4paintablesink) gst_element_seek with the
// INSTANT_RATE_CHANGE flag never returned: the GTK thread sat inside it half a
// second into every ×4 until the shell offered to kill the window, which the
// hang watchdog's dump showed frame by frame. The stumble is cheap; this pins
// that nothing brings the instant path back.
func TestASpeedEdgeCrossedMidPlayNeverUsesTheInstantRateSeek(t *testing.T) {
	src := readSrc(t, "player.go")
	if strings.Contains(src, "SeekFlagInstantRateChange") {
		t.Error("player.go seeks with INSTANT_RATE_CHANGE again, which hangs the GTK thread on this stack")
	}
	body := funcBody(t, "player.go", `func \(p \*Player\) SetRateNow\(`)
	for _, want := range []string{"if !p.playing {", "p.SeekTo(pos)", "time.Since(p.rateSeekAt) < rateSeekGap"} {
		if !strings.Contains(body, want) {
			t.Errorf("SetRateNow no longer contains %q", want)
		}
	}
	// and both pages' boundary crossings go through it
	if !strings.Contains(readSrc(t, "narrate.go"), "n.player.SetRateNow(fxPreviewRateAt(n.a.ed.fx, n.pos))") {
		t.Error("the Narrate preview's speed boundaries no longer go through SetRateNow")
	}
}

// The preview runs a speed effect at its rate from its first second to its
// last: no stairs. The stairs are the render's (fxRateAt), and following them
// in the preview was a flushing seek per tick through every ramp, which is
// how the window stopped answering on the way into a ×4.
func TestThePreviewRunsASpeedFlatAndNotByItsStairs(t *testing.T) {
	fx := []cutFx{{Kind: "speed", T: 100, Dur: 20, Rate: 4, Trans: 2, Tout: 2}}
	for _, c := range []struct{ t, want float64 }{
		{99.9, 1}, {100, 4}, {100.5, 4}, {110, 4}, {119, 4}, {120, 1},
	} {
		if got := fxPreviewRateAt(fx, c.t); got != c.want {
			t.Errorf("preview rate at %g = %g, want %g", c.t, got, c.want)
		}
	}
	// ...which the render's own reading confirms is a different answer: at
	// the foot of a 2 s ramp to ×4 its stair is not ×4
	if fxRateAt(fx, 100.5) == 4 {
		t.Error("the render has no stair at the foot of the ramp, so the test above proves nothing")
	}
	// a stop is footage running on under a still, here as there
	if got := fxPreviewRateAt([]cutFx{{Kind: "speed", T: 5, Dur: 3, Rate: 0}}, 6); got != 1 {
		t.Errorf("under a stop the preview rate is %g, want 1", got)
	}
	// every preview asks this one, and the render never does
	for _, f := range []string{"cut.go", "narrate.go"} {
		if strings.Contains(readSrc(t, f), "fxRateAt(") {
			t.Errorf("%s still follows the render's stairs in the preview", f)
		}
	}
	for _, f := range []string{"produce_fx.go", "produce.go"} {
		if strings.Contains(readSrc(t, f), "fxPreviewRateAt(") {
			t.Errorf("%s renders at the preview's flat rate", f)
		}
	}
	// and the fallback seek is held to a gap, so a rate that changes every
	// tick can never again be a seek every tick
	body := funcBody(t, "player.go", `func \(p \*Player\) SetRateNow\(`)
	if !strings.Contains(body, "time.Since(p.rateSeekAt) < rateSeekGap") || !strings.Contains(body, "p.rate = was") {
		t.Errorf("SetRateNow no longer holds its flushing seeks to rateSeekGap:\n%s", body)
	}
}

// What a pick-up says is what is in hand.
//
// It used to say what could be done with it too: drag it along the lane, nudge
// it with the frame keys, ⌦ to remove it, a click elsewhere to put it down,
// and — for the kinds with a box on the picture — the whole gesture vocabulary
// of that box. Three lines of it, on a status bar that is one line, so
// everything past the first clause was truncated at the window's edge: the
// part meant to teach was the part nobody could read. And it was printed on
// every press, long after there was anything left to learn.
//
// The gestures are learnt from the page instead — the pointer changes shape
// over a border, the box on the video has handles, a bar's ends are drawn as
// ends. The one exception is which BUTTON moves a thing, which nothing on the
// page can show, so the two pick-ups that have one say it in four words.
func TestAPickUpSaysWhatIsInHandAndLittleElse(t *testing.T) {
	body := funcBody(t, "cut_fx.go", `func \(ed \*cutEditor\) fxStatus\(\) \{`)
	if !strings.Contains(body, `ed.a.setStatus(f.fxLabel() + " picked up")`) {
		t.Errorf("an effect's pick-up says more than what it picked up:\n%s", body)
	}
	for _, gone := range []string{"nudge", "puts it down", "re-fitted", "⌦"} {
		if strings.Contains(body, gone) {
			t.Errorf("the pick-up line still explains %q, on a line that cannot hold it", gone)
		}
	}
	// the clip and the border say the one thing the page cannot show them
	src := readSrc(t, "cut.go")
	for _, want := range []string{
		`picked up at %s — right-drag to trim`,
		`" picked up — right-drag to move"`,
	} {
		if !strings.Contains(src, want) {
			t.Errorf("a pick-up no longer names the button that moves it: %q", want)
		}
	}
	if strings.Contains(src, "a click clear of it puts it down") {
		t.Error("the pick-up lines are back to a paragraph each")
	}
}
