package main

// The render's half of the effects: the output frame, the camera path and its
// filter chain, and the slow/freeze clip shapes -- each checked against the
// only witness whose opinion matters, ffmpeg itself, because a filter
// expression that LOOKS right and does not parse fails three minutes into a
// render.

import (
	"math"
	"os"
	"os/exec"
	"path/filepath"
	"strconv"
	"strings"
	"testing"
)

// The frame every clip comes out at. With no aspect chosen it is clipBox --
// the footage's own shape -- which is every cut made before this feature
// existed. With one chosen, the settings' tier names the SHORT side of the
// frame -- so "1080p" on a 9:16 cut is 1080×1920, the size a Short is
// uploaded at, and not a 608-wide strip -- rounded even because yuv420 says
// so.
func TestOutBoxFollowsTheAspect(t *testing.T) {
	for _, c := range []struct {
		aspect string
		h      int
		w, wh  int
	}{
		{"9:16", 1080, 1080, 1920}, // the tier is the width of a tall frame
		{"9:16", 720, 720, 1280},
		{"1:1", 1080, 1080, 1080},
		{"16:9", 720, 1280, 720}, // ...and the height of a wide one, as ever
		{"4:5", 1080, 1080, 1350},
		{"9:16", 0, 608, 1080}, // original, nothing probed: 1080 tall stands in
	} {
		w, h := outBox(nil, prodSettings{Height: c.h}, c.aspect)
		if w != c.w || h != c.wh {
			t.Errorf("aspect %s at tier %d gives %dx%d, want %dx%d", c.aspect, c.h, w, h, c.w, c.wh)
		}
	}
	// "source" and "" are the absence of a choice: clipBox's answer, untouched
	if w, h := outBox(nil, prodSettings{Height: 360}, "source"); w != 640 || h != 360 {
		t.Errorf(`aspect "source" gives %dx%d, want clipBox's 640x360`, w, h)
	}
}

// atempo refuses rates below 0.5, and older builds refuse above 2, so both
// ends are reached by chaining halves and doublings. The chain is audio's
// whole share of a clip off its own clock; get it wrong and the voices come
// out at the wrong pitch or the wrong length -- and at 100x, the rate the
// Speed effect now goes to, a single atempo would simply be rejected.
func TestAtempoChainReachesDeepRates(t *testing.T) {
	for _, c := range []struct {
		rate float64
		want string
	}{
		{1, ""},
		{0.5, ",atempo=0.5"},
		{0.75, ",atempo=0.75"},
		{0.25, ",atempo=0.5,atempo=0.5"},
		{2, ",atempo=2"},
		{4, ",atempo=2,atempo=2"},
		{10, ",atempo=2,atempo=2,atempo=2,atempo=1.25"},
	} {
		if got := atempoChain(c.rate); got != c.want {
			t.Errorf("atempoChain(%g) = %q, want %q", c.rate, got, c.want)
		}
	}
	// the ceiling the dialog allows, taken apart and put back together: what
	// matters is not the exact spelling but that every link is one atempo can
	// take and that they multiply back to the rate asked for
	chain := atempoChain(fxMaxRate)
	got := 1.0
	for _, part := range strings.Split(strings.TrimPrefix(chain, ","), ",") {
		v, err := strconv.ParseFloat(strings.TrimPrefix(part, "atempo="), 64)
		if err != nil {
			t.Fatalf("atempoChain(%g) = %q: %q is not an atempo", fxMaxRate, chain, part)
		}
		if v < 0.5 || v > 2 {
			t.Errorf("atempoChain(%g) = %q: %g is outside atempo's range", fxMaxRate, chain, v)
		}
		got *= v
	}
	if math.Abs(got-fxMaxRate) > 1e-9 {
		t.Errorf("atempoChain(%g) = %q, which multiplies out to %g", fxMaxRate, chain, got)
	}
}

// buildCam is fxRectAt -- the preview's own function -- sampled at every
// moment anything starts or stops moving, in the clip's OUTPUT time. The two
// halves of the feature cannot drift apart as long as this holds: what the
// orange outline promised on the Cut page is what zoompan is told to do.
func TestTheCameraPathSamplesThePreview(t *testing.T) {
	// the opening framing (long before this clip) is what the move departs
	// from; without it the opening rule would pin the whole timeline on the
	// T=105 rect and there would be nothing to animate
	fx := []cutFx{
		{Kind: "zoom", T: 0, Stay: true, Cx: 0.3, Cy: 0.5, Hf: 0.8},
		{Kind: "zoom", T: 105, Trans: 2, Dur: 2, Stay: true, Cx: 0.5, Cy: 0.5, Hf: 0.5},
	}
	sw, sh, ow, oh := 1920, 1080, 608, 1080
	srcA, outA := float64(sw)/float64(sh), float64(ow)/float64(oh)

	p := buildCam(fx, "9:16", sw, sh, 100, 10, 1, 10, ow, oh, 30)
	if p == nil || p.static() {
		t.Fatal("a gliding reframing built no moving camera")
	}
	wantTs := []float64{0, 5, 7, 10} // clip edges, fade start, fade end
	if len(p.ts) != len(wantTs) {
		t.Fatalf("breakpoints at %v, want %v", p.ts, wantTs)
	}
	for i, ts := range wantTs {
		if math.Abs(p.ts[i]-ts) > 1e-6 {
			t.Errorf("breakpoint %d at %.3f, want %.3f", i, p.ts[i], ts)
		}
		// the rect at each breakpoint is fxRectAt's answer for the session
		// moment it came from, verbatim
		if want := fxRectAt(fx, 100+ts, srcA, outA); !rectNear(p.r[i], want) {
			t.Errorf("at clip %.1f s the path holds %+v, want the preview's %+v", ts, p.r[i], want)
		}
	}

	// under slow motion the same session moments land later in the clip:
	// dividing by the rate is exactly the stretch the footage gets
	p = buildCam(fx, "9:16", sw, sh, 100, 10, 0.5, 20, ow, oh, 30)
	for i, ts := range []float64{0, 10, 14, 20} {
		if math.Abs(p.ts[i]-ts) > 1e-6 {
			t.Errorf("slowed breakpoint %d at %.3f, want %.3f", i, p.ts[i], ts)
		}
	}

	// a clip the effects never touch still gets a camera when an aspect is
	// chosen -- the letterboxed full frame, static, which the chain renders as
	// a plain crop-and-scale
	p = buildCam(nil, "9:16", sw, sh, 0, 10, 1, 10, ow, oh, 30)
	if p == nil || !p.static() {
		t.Fatal("aspect alone should give a static full-fit camera")
	}
	if !rectNear(p.r[0], fullFill(srcA, outA)) {
		t.Errorf("the untouched camera is %+v, want fullFill", p.r[0])
	}
}

// A zoom's two glides put their own breakpoints on the camera path: the way
// in ends after Trans, the way out begins Tout before the end -- and every
// sampled rectangle is still fxRectAt's answer, verbatim.
func TestAZoomsTwoGlidesReachTheRender(t *testing.T) {
	fx := []cutFx{{Kind: "zoom", T: 102, Trans: 1, Tout: 2, Dur: 6, Cx: 0.3, Cy: 0.3, Hf: 0.25}}
	sw, sh, ow, oh := 1920, 1080, 608, 1080
	srcA, outA := float64(sw)/float64(sh), float64(ow)/float64(oh)
	p := buildCam(fx, "9:16", sw, sh, 100, 10, 1, 10, ow, oh, 30)
	if p == nil || p.static() {
		t.Fatal("an asymmetric zoom built no moving camera")
	}
	wantTs := []float64{0, 2, 3, 6, 8, 10} // edges; in 102-103, hold, out 106-108
	if len(p.ts) != len(wantTs) {
		t.Fatalf("breakpoints at %v, want %v", p.ts, wantTs)
	}
	for i, ts := range wantTs {
		if math.Abs(p.ts[i]-ts) > 1e-6 {
			t.Errorf("breakpoint %d at %.3f, want %.3f", i, p.ts[i], ts)
		}
		if want := fxRectAt(fx, 100+ts, srcA, outA); !rectNear(p.r[i], want) {
			t.Errorf("at clip %.1f s the path holds %+v, want the preview's %+v", ts, p.r[i], want)
		}
	}
}

// The chains themselves, run. Two shapes: the static camera (pad, crop,
// scale -- costs nothing) and the moving one (zoompan with piecewise-linear
// expressions). Both must come out at exactly the output box, because the
// concat join is a stream copy and a clip one pixel off is refused.
func TestTheCameraChainIsAcceptedByFfmpeg(t *testing.T) {
	if _, err := exec.LookPath("ffmpeg"); err != nil {
		t.Skip("no ffmpeg on this machine")
	}
	dir := t.TempDir()
	src := filepath.Join(dir, "src.mp4")
	mustFFmpeg(t, "-f", "lavfi", "-t", "2", "-i", "testsrc=size=1920x1080:rate=30",
		"-c:v", "libx264", "-preset", "ultrafast", "-pix_fmt", "yuv420p", src)

	run := func(name string, p *camPath) {
		out := filepath.Join(dir, name+".mp4")
		vf := strings.Join(append([]string{"fps=30"}, p.chain()...), ",")
		if b, err := exec.Command("ffmpeg", "-v", "error", "-y", "-i", src,
			"-vf", vf, "-c:v", "libx264", "-preset", "ultrafast", "-pix_fmt", "yuv420p",
			out).CombinedOutput(); err != nil {
			t.Fatalf("%s: the chain %q does not run: %v\n%s", name, vf, err, b)
		}
		w, h, err := ffprobeSize(out)
		if err != nil {
			t.Fatalf("%s produced nothing readable: %v", name, err)
		}
		if w != 608 || h != 1080 {
			t.Errorf("%s came out %dx%d, want 608x1080 — concat refuses this clip", name, w, h)
		}
		if d, _ := ffprobeDur(out); math.Abs(d-2) > 0.15 {
			t.Errorf("%s runs %.2f s, want 2 — the camera changed the clock", name, d)
		}
	}

	// aspect only: the letterboxed frame, static
	run("static", buildCam(nil, "9:16", 1920, 1080, 0, 2, 1, 2, 608, 1080, 30))
	// a reframing gliding to a half-height crop, then a zoom on top: the
	// moving window, breakpoints inside the clip
	run("moving", buildCam([]cutFx{
		{Kind: "zoom", T: 0.2, Trans: 0.6, Dur: 0.6, Stay: true, Cx: 0.5, Cy: 0.5, Hf: 0.5},
		{Kind: "zoom", T: 1.0, Trans: 0.3, Dur: 0.8, Cx: 0.3, Cy: 0.3, Hf: 0.25},
	}, "9:16", 1920, 1080, 0, 2, 1, 2, 608, 1080, 30))
}

// The two speed shapes, end to end through encodeClip: a half-speed clip must
// come out twice as long as the footage it reads (sound stretched with it),
// and a freeze must come out at its own Dur having read almost nothing.
func TestSlowAndFreezeComeOutTheRightLength(t *testing.T) {
	a := insertApp(t)
	dir := t.TempDir()
	footage := filepath.Join(dir, "footage.mp4")
	mustFFmpeg(t, "-f", "lavfi", "-t", "4", "-i", "testsrc=size=1280x720:rate=30",
		"-f", "lavfi", "-t", "4", "-i", "sine=frequency=300:sample_rate=48000",
		"-c:v", "libx264", "-preset", "ultrafast", "-pix_fmt", "yuv420p", "-c:a", "aac", footage)
	st := prodSettings{
		Container: "mp4", Codec: "h264", CRF: 30, Preset: "ultrafast",
		Height: 360, FPS: 24, AudioKbps: 96, GameVol: 0.22, Subs: "none",
	}
	v := &tlVideo{base: "footage", path: footage}

	for _, c := range []struct {
		name string
		clip prodClip
		want float64
	}{
		// 2 s of footage at half speed plays for 4
		{"slow", prodClip{idx: 0, video: v, local: 0, length: 4, rate: 0.5, tempo: 1}, 4},
		// the frame at 1 s, held for 2
		{"freeze", prodClip{idx: 1, video: v, local: 1, length: 2, rate: 1, freeze: true, tempo: 1}, 2},
	} {
		out := filepath.Join(dir, c.name+".mp4")
		if err := a.encodeClip(c.clip, out, "", st); err != nil {
			t.Fatalf("%s: %v", c.name, err)
		}
		d, err := ffprobeDur(out)
		if err != nil {
			t.Fatalf("%s produced nothing readable: %v", c.name, err)
		}
		if math.Abs(d-c.want) > 0.15 {
			t.Errorf("%s runs %.2f s, want %.1f", c.name, d, c.want)
		}
		// the same 640x360 every other clip comes out at: speed changes the
		// clock, never the frame
		if w, h, _ := ffprobeSize(out); w != 640 || h != 360 {
			t.Errorf("%s came out %dx%d, want 640x360", c.name, w, h)
		}
	}
	// what the slow clip actually read: its input -t is rate*length, not length
	args, _, err := a.clipInput(prodClip{video: v, local: 0, length: 4, rate: 0.5}, st)
	if err != nil {
		t.Fatal(err)
	}
	if got := strings.Join(args, " "); !strings.Contains(got, "-t 2.000") {
		t.Errorf("the half-speed clip reads %q, want 2 s of footage for 4 s of video", got)
	}
}

// The render reads the cut through applyFx, and the segments it turns speed
// effects into carry everything downstream needs. Pinned as source: this is
// the single line that makes effects render at all.
func TestProduceRunsTheCutThroughTheEffects(t *testing.T) {
	fn := funcBody(t, "produce.go", `func \(a \*App\) produceSegs\(\) \[\]cutSeg \{`)
	for _, want := range []string{
		"segs := splitSpliced(c.Segs)",
		"return applyFx(segs, c.Fx)",
		"segs[i].Scene = i", // the stamp the sound's own planning reads (cut_fxsound.go)
	} {
		if !strings.Contains(fn, want) {
			t.Errorf("produceSegs no longer %q — slows and freezes are edited but never rendered", want)
		}
	}
}

// Text, end to end through encodeClip: the overlay is written as an SVG the
// size of the finished frame, handed to ffmpeg as another input, and
// composited without touching the clip's length or its dimensions -- which is
// what the concat join demands of every clip.
func TestATextIsRenderedOntoTheClip(t *testing.T) {
	if _, err := exec.LookPath("ffmpeg"); err != nil {
		t.Skip("no ffmpeg on this machine")
	}
	dir := t.TempDir()
	// ffmpeg without librsvg cannot read an SVG at all, and then neither can
	// the card inserts -- that is a build problem, not this test's business
	probe := filepath.Join(dir, "probe.svg")
	if err := os.WriteFile(probe, textSVG(cutFx{Kind: "text", Text: "x",
		Cx: 0.5, Cy: 0.5, Wf: 0.8, Hf: 0.2}, 320, 180), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := exec.Command("ffmpeg", "-v", "error", "-y", "-i", probe,
		"-frames:v", "1", filepath.Join(dir, "probe.png")).Run(); err != nil {
		t.Skip("this ffmpeg cannot decode SVG (no librsvg) — the cards cannot render either")
	}

	a := insertApp(t)
	footage := filepath.Join(dir, "footage.mp4")
	mustFFmpeg(t, "-f", "lavfi", "-t", "4", "-i", "testsrc=size=1280x720:rate=30",
		"-f", "lavfi", "-t", "4", "-i", "sine=frequency=300:sample_rate=48000",
		"-c:v", "libx264", "-preset", "ultrafast", "-pix_fmt", "yuv420p", "-c:a", "aac", footage)
	st := prodSettings{
		Container: "mp4", Codec: "h264", CRF: 30, Preset: "ultrafast",
		Height: 360, FPS: 24, AudioKbps: 96, GameVol: 0.22, Subs: "none",
	}
	fx := []cutFx{
		{Kind: "text", Text: "Hello world", T: 1, Dur: 2, Trans: 0.3, Tout: 0.3},
		{Kind: "text", Text: "and\nthe second one", T: 2, Dur: 1.5},
	}
	c := prodClip{idx: 0, video: &tlVideo{base: "footage", path: footage},
		local: 0, sessS: 0, length: 4, rate: 1, tempo: 1, boxW: 640, boxH: 360}
	c.texts = textCues(fx, c.sessS, c.length*c.speed(), c.speed(), c.length)
	if len(c.texts) != 2 {
		t.Fatalf("%d cues on a clip that covers both texts", len(c.texts))
	}
	out := filepath.Join(dir, "withtext.mp4")
	if err := a.encodeClip(c, out, "", st); err != nil {
		t.Fatalf("the text overlay does not render: %v", err)
	}
	if w, h, _ := ffprobeSize(out); w != 640 || h != 360 {
		t.Errorf("the clip came out %dx%d, want 640x360 — concat refuses this clip", w, h)
	}
	if d, err := ffprobeDur(out); err != nil || math.Abs(d-4) > 0.15 {
		t.Errorf("the clip runs %.2f s (%v), want 4 — the overlay changed the clock", d, err)
	}
	// one file per cue, named after the effect's place in the cut so a
	// re-render overwrites rather than litters
	for _, want := range []string{"withtext_t00.svg", "withtext_t01.svg"} {
		if !exists(filepath.Join(dir, want)) {
			t.Errorf("%s was never written", want)
		}
	}
	// and a clip with no text is left exactly as it was: no extra input, no
	// overlay, the picture still coming out on [v]
	plain := prodClip{idx: 1, video: c.video, length: 2, rate: 1, tempo: 1, boxW: 640, boxH: 360}
	if err := a.encodeClip(plain, filepath.Join(dir, "plain.mp4"), "", st); err != nil {
		t.Fatalf("a clip without text stopped rendering: %v", err)
	}
}

// The wiring that gets the words from the cut to the encoder, pinned as
// source: the clip has to remember where it starts on the session clock, every
// clip has to know the frame it comes out at (the overlay is drawn at that
// size), and the cues have to be mapped exactly as the camera's are -- a title
// and a camera move placed at the same second must happen at the same second.
func TestTheTextRenderIsWired(t *testing.T) {
	b, err := os.ReadFile("produce.go")
	if err != nil {
		t.Fatal(err)
	}
	src := string(b)
	for _, want := range []string{
		// the session clock, kept for inserts as well as footage
		"c.sessS = s.S",
		// the output frame on every clip, not only the inserts fitted into it
		"clips[i].boxW, clips[i].boxH = boxW, boxH",
		// the same mapping buildCam gets, freeze and insert as one held moment
		"c.texts = textCues(fx, c.sessS, span, c.speed(), c.length)",
		// one looped SVG input per cue, after the mixes so no earlier index moves
		"txtBase := mixBase + len(c.mix)",
		"textSVG(cue.fx, c.boxW, c.boxH)",
		// composited after the camera and the subtitles, on the finished frame
		`chain, last := textChain(c.texts, "vcam", txtBase, c.boxW, c.boxH)`,
		`"-map", "["+vlab+"]"`,
	} {
		if !strings.Contains(src, want) {
			t.Errorf("the render no longer contains %q", want)
		}
	}
}

// camAt walks the rendered path the way zoompan does: straight between
// breakpoints. The whole point of the path is that this equals fxRectAt at
// every moment, not merely at the samples -- so this is what the test below
// compares against.
func camAt(p *camPath, t float64) fxRect {
	if t <= p.ts[0] {
		return p.r[0]
	}
	for i := 1; i < len(p.ts); i++ {
		if t <= p.ts[i] {
			if p.ts[i]-p.ts[i-1] < 1e-9 {
				return p.r[i]
			}
			return lerpRect(p.r[i-1], p.r[i], (t-p.ts[i-1])/(p.ts[i]-p.ts[i-1]))
		}
	}
	return p.r[len(p.r)-1]
}

// A camera move's hold and its fade out are moments the camera changes what it
// is doing, so they have to be breakpoints. They were not: the path sampled
// only the arrival, and between that and the end of the clip it travelled
// straight -- so a zoom told to sit still for two seconds and then come back
// over one instead crept away from its rectangle for the whole rest of the
// clip. The preview was right and the file was wrong, the worst way round.
func TestAZoomsHoldAndFadeOutAreOnThePath(t *testing.T) {
	fx := []cutFx{
		{Kind: "zoom", T: 0, Stay: true, Cx: 0.3, Cy: 0.5, Hf: 0.8},
		// arrives 103, parked until 105, gone by 106
		{Kind: "zoom", T: 102, Trans: 1, Dur: 4, Tout: 1, Cx: 0.7, Cy: 0.5, Hf: 0.5},
	}
	sw, sh, ow, oh := 1920, 1080, 608, 1080
	srcA, outA := float64(sw)/float64(sh), float64(ow)/float64(oh)
	p := buildCam(fx, "9:16", sw, sh, 100, 10, 1, 10, ow, oh, 30)
	if p == nil {
		t.Fatal("no camera path")
	}
	for _, want := range []float64{0, 2, 3, 5, 6, 10} {
		found := false
		for _, ts := range p.ts {
			if math.Abs(ts-want) < 1e-6 {
				found = true
			}
		}
		if !found {
			t.Errorf("clip second %.0f is not a breakpoint; the path is %v", want, p.ts)
		}
	}
	// and the property the breakpoints exist for, checked between them
	for k := 0; k <= 200; k++ {
		ct := float64(k) / 20
		if got, want := camAt(p, ct), fxRectAt(fx, 100+ct, srcA, outA); !rectNear(got, want) {
			t.Fatalf("at clip %.2f s the render is at %+v and the preview at %+v", ct, got, want)
		}
	}

	// a reframing that arrives and stays needs no end samples: it is parked on
	// its rectangle from the moment it gets there
	plain := []cutFx{fx[0], {Kind: "zoom", T: 102, Trans: 1, Dur: 1, Stay: true, Cx: 0.7, Cy: 0.5, Hf: 0.5}}
	if q := buildCam(plain, "9:16", sw, sh, 100, 10, 1, 10, ow, oh, 30); len(q.ts) != 4 {
		t.Errorf("a plain gliding reframing has breakpoints %v, want four", q.ts)
	}
}
