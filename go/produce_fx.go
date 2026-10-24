package main

// The render's half of the effects (cut_fx.go): per-clip ffmpeg filter chains
// for aspect, views and zooms, plus the audio arithmetic a clip off its own
// speed needs. Every clip must come out at the same frame size (outBox)
// because the join is a stream copy:
//
//	pad  -> room around the frame, when a rectangle reaches past its edges
//	crop -> the fixed box the whole camera journey happens inside
//	zoompan -> the moving window, piecewise-linear expressions of in_time --
//	           or a plain crop and scale when the camera never moves
//
// The camera path comes from fxRectAt, the same function the preview draws,
// sampled at every moment anything starts or stops moving.

import (
	"fmt"
	"math"
	"strings"
)

// camPath is the camera over one clip, in the clip's own output time. ts is
// ascending, starts at 0 and ends at the clip's length; r[i] is where the
// camera is at ts[i], and between breakpoints it travels straight -- which is
// exact, because everything fxRectAt does between its own breakpoints is a
// straight lerp too.
type camPath struct {
	ts []float64
	r  []fxRect
	// the pixel plan, fixed for the whole clip (see buildCam)
	sw, sh     int // source frame
	outW, outH int // the frame every clip comes out at
	padL, padT int // room added around the source
	padW, padH int
	boxX, boxY int // the fixed crop inside the padded frame
	boxW, boxH int
	fps        float64 // the grid zoompan runs on
}

// static is whether the camera never moves in this clip.
func (p *camPath) static() bool {
	for _, r := range p.r[1:] {
		if r != p.r[0] {
			return false
		}
	}
	return true
}

// buildCam samples the camera over one clip. sessS is the session time the
// clip starts at, span the session seconds it covers (0 for a freeze), rate
// its playback rate (1 = normal), length its output seconds, out the frame
// size, fps the grid an animated camera runs on.
func buildCam(fx []cutFx, aspect string, sw, sh int, sessS, span, rate, length float64,
	outW, outH int, fps float64) *camPath {
	if sw <= 0 || sh <= 0 || outW <= 0 || outH <= 0 || length <= 0 {
		return nil
	}
	if rate <= 0 {
		rate = 1
	}
	srcA, outA := float64(sw)/float64(sh), float64(outW)/float64(outH)
	// every session moment the camera starts or stops moving inside this clip
	sessTs := []float64{sessS, sessS + span}
	for _, f := range fx {
		if f.Kind != "zoom" {
			continue
		}
		// every moment the camera changes what it is DOING: arriving, parked,
		// letting go, gone. The path travels straight between samples, so a
		// hold and a fade out that are not sampled are not rendered -- the
		// camera would leave its rectangle the instant it reached it and
		// drift across the rest of the clip instead of standing still for its
		// seconds and then letting go. A staying zoom needs no end samples:
		// it is parked on its rectangle from the moment it arrives.
		tin, tout := f.zoomGlides()
		sessTs = append(sessTs, f.T, f.T+tin)
		if !f.Stay {
			sessTs = append(sessTs, f.T+math.Max(f.Dur, 0)-tout, f.T+math.Max(f.Dur, 0))
		}
	}
	p := &camPath{sw: sw, sh: sh, outW: outW, outH: outH, fps: fps}
	seen := map[int64]bool{}
	for _, st := range sessTs {
		if st < sessS-1e-6 || st > sessS+span+1e-6 {
			continue
		}
		t := 0.0
		if span > 0 {
			t = (st - sessS) / rate
		}
		t = math.Min(math.Max(0, t), length)
		key := int64(math.Round(t * 1000))
		if seen[key] {
			continue
		}
		seen[key] = true
		p.ts = append(p.ts, t)
		p.r = append(p.r, fxRectAt(fx, st, srcA, outA))
	}
	// a freeze (span 0) collapses to one sample; give the path its two ends
	if len(p.ts) == 1 {
		p.ts = append(p.ts, length)
		p.r = append(p.r, p.r[0])
	}
	sortCam(p)
	p.plan(outA)
	return p
}

// gainCues maps the cut's volume effects onto one clip on textCues' terms --
// same five numbers -- so a title and a gain at the same second come and go
// together, and a gain under a speed effect stretches with its sound. A clip
// that is one held moment (span 0) gets none.
func gainCues(fx []cutFx, sessS, span, rate, length float64) []textCue {
	if length <= 0 || span <= 0 {
		return nil
	}
	var out []textCue
	for i, f := range fx {
		if f.Kind != "volume" {
			continue
		}
		// no guard against a band of no length: cueClip turns down anything
		// shorter than a frame, which is the same answer for a better reason
		fin, fout := f.textFades()
		s, e, cin, cout, ok := cueClip(f.T, f.Dur, fin, fout, sessS, rate, length)
		if !ok {
			continue
		}
		if cin+cout > e-s {
			cin = math.Min(cin, e-s)
			cout = math.Max(0, e-s-cin)
		}
		out = append(out, textCue{fx: f, s: s, e: e, fin: cin, fout: cout, idx: i})
	}
	return out
}

// gainExpr is one volume cue as an ffmpeg volume expression in the clip's own
// seconds: the gain across the band, 1 elsewhere, a straight ramp over each
// fade. One expression, since volume's eval=frame re-reads it anyway. The
// multiplier is written with an explicit sign (1-0.5000*... not 1+-0.5000*...).
func gainExpr(c textCue) string {
	g := clampGain(c.fx.Gain)
	var ramps []string
	if c.fin > 0 {
		ramps = append(ramps, fmt.Sprintf("(t-%.3f)/%.3f", c.s, c.fin))
	}
	if c.fout > 0 {
		ramps = append(ramps, fmt.Sprintf("(%.3f-t)/%.3f", c.e, c.fout))
	}
	val := fmt.Sprintf("%.4f", g)
	switch len(ramps) {
	case 1:
		val = fmt.Sprintf("1%+.4f*min(1,%s)", g-1, ramps[0])
	case 2:
		val = fmt.Sprintf("1%+.4f*min(1,min(%s,%s))", g-1, ramps[0], ramps[1])
	}
	return fmt.Sprintf("if(between(t,%.3f,%.3f),%s,1)", c.s, c.e, val)
}

// gainChain is every volume cue of one clip, applied in turn to the label the
// sound arrives on; it returns the filter text and the label it leaves on. In
// turn, so two overlapping effects multiply -- which is exactly what fxGainAt
// tells the preview they do.
func gainChain(cues []textCue, in string) (string, string) {
	fc := ""
	for k, c := range cues {
		out := fmt.Sprintf("gv%d", k)
		fc += fmt.Sprintf("[%s]volume=volume='%s':eval=frame[%s];", in, gainExpr(c), out)
		in = out
	}
	return fc, in
}

func sortCam(p *camPath) {
	for i := 1; i < len(p.ts); i++ {
		for j := i; j > 0 && p.ts[j] < p.ts[j-1]; j-- {
			p.ts[j], p.ts[j-1] = p.ts[j-1], p.ts[j]
			p.r[j], p.r[j-1] = p.r[j-1], p.r[j]
		}
	}
}

// rectPx is a camera rectangle in source pixels: hf of the source height
// tall, exactly the output aspect wide.
func (p *camPath) rectPx(r fxRect, outA float64) (x, y, w, h float64) {
	h = r.hf * float64(p.sh)
	w = h * outA
	return r.cx*float64(p.sw) - w/2, r.cy*float64(p.sh) - h/2, w, h
}

// plan fixes the pixel geometry: how much black to pad around the source so
// no rectangle falls off it, and the one crop box -- output-shaped, covering
// the whole journey -- the moving window works inside.
func (p *camPath) plan(outA float64) {
	minX, minY := math.Inf(1), math.Inf(1)
	maxX, maxY := math.Inf(-1), math.Inf(-1)
	for _, r := range p.r {
		x, y, w, h := p.rectPx(r, outA)
		minX, minY = math.Min(minX, x), math.Min(minY, y)
		maxX, maxY = math.Max(maxX, x+w), math.Max(maxY, y+h)
	}
	p.padL = int(math.Ceil(math.Max(0, -minX)))
	p.padT = int(math.Ceil(math.Max(0, -minY)))
	padR := int(math.Ceil(math.Max(0, maxX-float64(p.sw))))
	padB := int(math.Ceil(math.Max(0, maxY-float64(p.sh))))
	even := func(n int) int { return n + n%2 }
	p.padW = even(p.sw + p.padL + padR)
	p.padH = even(p.sh + p.padT + padB)
	// the box: the union of every rectangle, grown to the output's aspect so
	// the moving window inside it never distorts
	bx, by := minX+float64(p.padL), minY+float64(p.padT)
	bw, bh := maxX-minX, maxY-minY
	if bw/bh < outA {
		grow := bh*outA - bw
		bx -= grow / 2
		bw += grow
	} else if bw/outA > bh {
		grow := bw/outA - bh
		by -= grow / 2
		bh += grow
	}
	p.boxW, p.boxH = even(int(math.Ceil(bw))), even(int(math.Ceil(bh)))
	p.boxX = int(math.Round(bx))
	p.boxY = int(math.Round(by))
	// the grown box may poke out of the padded frame; slide it back in, and
	// pad more if it is simply bigger
	if p.boxW > p.padW {
		p.padW = even(p.boxW)
	}
	if p.boxH > p.padH {
		p.padH = even(p.boxH)
	}
	p.boxX = int(math.Max(0, math.Min(float64(p.boxX), float64(p.padW-p.boxW))))
	p.boxY = int(math.Max(0, math.Min(float64(p.boxY), float64(p.padH-p.boxH))))
}

// pieceExpr is a piecewise-linear function of in_time as an ffmpeg
// expression: nested ifs, one segment per interval, clamped flat past either
// end. vs[i] is the value at ts[i].
func pieceExpr(ts, vs []float64) string {
	if len(ts) == 0 {
		return "0"
	}
	expr := fmt.Sprintf("%.4f", vs[len(vs)-1]) // past the end: the last value
	for i := len(ts) - 2; i >= 0; i-- {
		t0, t1, v0, v1 := ts[i], ts[i+1], vs[i], vs[i+1]
		var seg string
		if t1-t0 < 1e-4 || v0 == v1 {
			seg = fmt.Sprintf("%.4f", v1)
		} else {
			seg = fmt.Sprintf("%.4f+%.4f*clip((it-%.4f)/%.4f\\,0\\,1)",
				v0, v1-v0, t0, t1-t0)
		}
		expr = fmt.Sprintf("if(lt(it\\,%.4f)\\,%s\\,%s)", t1, seg, expr)
	}
	return expr
}

// padBox is the border the camera needs around the recording: the padded
// frame's size and where the recording sits in it. encodeClip fills it
// (bdrop); every crop and zoompan expression below is measured on the padded
// frame. ok is false when the camera never looks past the edge.
func (p *camPath) padBox() (w, h, l, t int, ok bool) {
	if p.padW == p.sw && p.padH == p.sh {
		return 0, 0, 0, 0, false
	}
	return p.padW, p.padH, p.padL, p.padT, true
}

// chain is the filter steps that realize the camera: pad and crop (skipped
// when no-ops), then a plain scale for a still camera or zoompan with the
// journey in its expressions. padded says the caller already put the recording
// on a padded frame (encodeClip, for the blurred border).
func (p *camPath) chain() []string { return p.chainOn(false) }

func (p *camPath) chainOn(padded bool) []string {
	outA := float64(p.outW) / float64(p.outH)
	var vf []string
	if _, _, _, _, ok := p.padBox(); ok && !padded {
		vf = append(vf, fmt.Sprintf("pad=%d:%d:%d:%d:color=black",
			p.padW, p.padH, p.padL, p.padT))
	}
	if p.static() {
		// one rectangle for the whole clip: crop it and scale it, exactly
		x, y, w, h := p.rectPx(p.r[0], outA)
		even := func(v float64) int { n := int(math.Round(v)); return n - n%2 }
		cw, ch := even(w), even(h)
		cx := int(math.Round(x)) + p.padL
		cy := int(math.Round(y)) + p.padT
		cx = int(math.Max(0, math.Min(float64(cx), float64(p.padW-cw))))
		cy = int(math.Max(0, math.Min(float64(cy), float64(p.padH-ch))))
		vf = append(vf, fmt.Sprintf("crop=%d:%d:%d:%d", cw, ch, cx, cy),
			fmt.Sprintf("scale=%d:%d", p.outW, p.outH))
		return vf
	}
	if p.boxW != p.padW || p.boxH != p.padH {
		vf = append(vf, fmt.Sprintf("crop=%d:%d:%d:%d", p.boxW, p.boxH, p.boxX, p.boxY))
	}
	// the moving window: zoom is how many times the box's height the window
	// is, position is the rectangle's corner. All three are driven by the
	// same piecewise-linear samples, so what the window does between two
	// breakpoints is precisely what the preview overlay drew.
	n := len(p.ts)
	hs, xs, ys := make([]float64, n), make([]float64, n), make([]float64, n)
	for i, r := range p.r {
		x, y, w, h := p.rectPx(r, outA)
		_ = w
		hs[i] = h
		xs[i] = x + float64(p.padL) - float64(p.boxX)
		ys[i] = y + float64(p.padT) - float64(p.boxY)
	}
	fps := p.fps
	if fps <= 0 {
		fps = 30
	}
	vf = append(vf, fmt.Sprintf(
		"zoompan=z='%d/(%s)':x='%s':y='%s':d=1:s=%dx%d:fps=%g",
		p.boxH, pieceExpr(p.ts, hs), pieceExpr(p.ts, xs), pieceExpr(p.ts, ys),
		p.outW, p.outH, fps))
	return vf
}

// maxZoom is the deepest the moving window goes, box heights per window
// height. zoompan clamps at 10; past that the render comes out shallower
// than the preview promised, which is worth a line in the log.
func (p *camPath) maxZoom() float64 {
	z := 1.0
	for _, r := range p.r {
		if h := r.hf * float64(p.sh); h > 0 {
			z = math.Max(z, float64(p.boxH)/h)
		}
	}
	return z
}

// outBox is the frame size every clip comes out at: the footage's own shape
// (clipBox) until an aspect is chosen, and then the settings' tier -- or the
// footage's own height -- inside that aspect (tierBox).
func outBox(clips []prodClip, st prodSettings, aspect string) (int, int) {
	a := parseAspect(aspect)
	if a <= 0 {
		return clipBox(clips, st)
	}
	h0 := 0
	if st.Height <= 0 {
		for _, c := range clips {
			if c.video == nil {
				continue
			}
			if _, fh, err := ffprobeSize(c.video.path); err == nil {
				h0 = fh
				break
			}
		}
	}
	return tierBox(a, h0, st.Height)
}

// tierBox is the frame a chosen aspect comes out at. The tier names the SHORT
// side: "1080p" on a 9:16 cut is 1080×1920. With no tier the footage's height
// names the frame's. Both sides rounded even for yuv420. Pure, so the Produce
// info line can say the frame without opening a file.
func tierBox(a float64, h0, height int) (int, int) {
	if a < 1 && height > 0 {
		w := height - height%2
		return w, int(math.Round(float64(w)/a/2)) * 2
	}
	h := height
	if h <= 0 {
		h = h0
	}
	if h <= 0 {
		h = 1080
	}
	h -= h % 2
	return int(math.Round(float64(h)*a/2)) * 2, h
}

// atempoChain is the audio counterpart of footage off its own clock: atempo
// holds the pitch, and the rates one instance of it will not take are reached
// by chaining. It refuses anything below 0.5, and old builds refuse above 2 --
// so both ends are walked in halves and doublings, which every build takes.
func atempoChain(rate float64) string {
	if rate <= 0 || rate == 1 {
		return ""
	}
	var parts []string
	for rate < 0.5 {
		parts = append(parts, "atempo=0.5")
		rate *= 2
	}
	for rate > 2 {
		parts = append(parts, "atempo=2")
		rate /= 2
	}
	if rate != 1 {
		parts = append(parts, fmt.Sprintf("atempo=%g", rate))
	}
	return "," + strings.Join(parts, ",")
}
