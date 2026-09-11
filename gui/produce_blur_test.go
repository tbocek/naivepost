package main

// The bare parts of a frame. Three things leave one: a camera pulled back past
// the edge of the recording, a recording that is not the shape of the finished
// video, and an insert or a card that is not either. All three used to come
// out black, which in a finished video reads as a fault rather than a choice.
// They are filled with a blown-up, blurred copy of the picture itself instead.
//
// The sub-graph is exercised here; the ffmpeg run is pinned to the source,
// because a filter graph is only true when ffmpeg accepts it and this suite
// does not run one.

import (
	"encoding/json"
	"fmt"
	"os"
	"strings"
	"testing"
)

// The picture at its own size on a bigger frame -- what pad did, with the
// border filled in. The copy COVERS the frame (increase then crop), so no part
// of the border is left over for black to show through.
func TestABackdropFillsTheBorderWithTheBlurredPicture(t *testing.T) {
	b := bdrop{w: 1920, h: 1080, x: 200, y: 60}
	fc, lab := b.chain("vpre", 0)
	if lab != "bd0" {
		t.Errorf("the backdrop wrote %q, want its own label", lab)
	}
	for _, want := range []string{
		"[vpre]split=2[bdb0][bdf0];",
		"scale=1920:1080:force_original_aspect_ratio=increase",
		"crop=1920:1080",
		"gblur=sigma=",
		"[bdb0][bdf0]overlay=200:60[bd0];",
	} {
		if !strings.Contains(fc, want) {
			t.Errorf("the backdrop graph has no %q:\n%s", want, fc)
		}
	}
	// the picture goes on UNSCALED: it is already the right size, and
	// resampling it to sit on its own backdrop would soften the one part of
	// the frame that is meant to be sharp
	if strings.Contains(fc, "[bdf0]scale") {
		t.Errorf("the picture is being rescaled onto its own backdrop:\n%s", fc)
	}
	// every label it writes is one it reads, and no label leaks
	countLabels(t, fc, "vpre", "bd0")
}

// An insert or a card: its own size is nobody's business, so it is scaled down
// until it fits and centred -- and what would have been the black bars beside
// it is the same blurred blow-up.
func TestAnInsertIsCentredOnItsOwnBlurredBlowUp(t *testing.T) {
	b := bdrop{w: 1080, h: 1920, fit: true}
	fc, lab := b.chain("vpre", 3)
	if lab != "bd3" {
		t.Errorf("the backdrop wrote %q, want bd3", lab)
	}
	for _, want := range []string{
		"[vpre]split=2[bdb3][bdf3];",
		"[bdf3]scale=1080:1920:force_original_aspect_ratio=decrease[bdf3];",
		"[bdb3][bdf3]overlay=(W-w)/2:(H-h)/2[bd3];",
	} {
		if !strings.Contains(fc, want) {
			t.Errorf("the fitted backdrop has no %q:\n%s", want, fc)
		}
	}
	countLabels(t, fc, "vpre", "bd3")
}

// Nothing owed, nothing built: a camera that stays inside the recording and a
// clip with no insert leave the graph exactly as it was.
func TestNothingBareMeansNoBackdrop(t *testing.T) {
	if (bdrop{}).on() {
		t.Error("an empty backdrop claims to be owed")
	}
	if (bdrop{w: 1920}).on() || (bdrop{h: 1080}).on() {
		t.Error("half a frame counts as a backdrop")
	}
	// and the camera says so itself. A portrait cut of widescreen footage,
	// parked on a slice of it: every pixel it wants exists, so no border.
	p := cam(fxRect{0.5, 0.5, 1})
	if _, _, _, _, ok := p.padBox(); ok {
		t.Errorf("a camera inside the recording asked for a %d×%d border", p.padW, p.padH)
	}
	// pulled back past the top and bottom of the recording: now there is
	// something to fill, and it is as tall as the camera reached
	p = cam(fxRect{0.5, 0.5, 1.4})
	w, h, l, top, ok := p.padBox()
	if !ok {
		t.Fatal("a camera reaching past the recording asked for no border")
	}
	if w != p.padW || h != p.padH || l != p.padL || top != p.padT {
		t.Errorf("padBox = %d×%d at %d,%d, want the plan's %d×%d at %d,%d",
			w, h, l, top, p.padW, p.padH, p.padL, p.padT)
	}
	if h <= 1080 || top <= 0 {
		t.Errorf("the border is %d tall starting at %d, want room above and below 1080", h, top)
	}
	// asked to skip the pad, the chain does -- everything after it is
	// measured on the padded frame either way, so the two must not both run
	on := strings.Join(p.chainOn(true), ",")
	off := strings.Join(p.chainOn(false), ",")
	if strings.Contains(on, "pad=") {
		t.Errorf("the camera padded a frame the backdrop had already made: %s", on)
	}
	want := fmt.Sprintf("pad=%d:%d:%d:%d:color=black", p.padW, p.padH, p.padL, p.padT)
	if !strings.HasPrefix(off, want+",") {
		t.Errorf("without a backdrop the camera lost its own pad: %s", off)
	}
	if strings.TrimPrefix(off, want+",") != on {
		t.Errorf("the two chains differ by more than the pad:\n%s\n%s", off, on)
	}
}

// cam is one still camera on a portrait cut of widescreen footage, planned the
// way buildCam plans one.
func cam(r fxRect) *camPath {
	p := &camPath{ts: []float64{0, 2}, r: []fxRect{r, r},
		sw: 1920, sh: 1080, outW: 1080, outH: 1920, fps: 30}
	p.plan(float64(p.outW) / float64(p.outH))
	return p
}

// The wiring: both sites hand their bare frame to the backdrop instead of
// filling it with black, and it is spliced into the graph where the pad it
// replaces stood -- after the stop stills, before the camera crops.
func TestTheBackdropWiringIsInPlace(t *testing.T) {
	b, err := os.ReadFile("produce.go")
	if err != nil {
		t.Fatal(err)
	}
	s := string(b)
	for _, want := range []string{
		"bd = bdrop{w: c.boxW, h: c.boxH, fit: true}",
		"bd = bdrop{w: pw, h: ph, x: pl, y: pt}",
		"vf = append(vf, c.cam.chainOn(bd.on())...)",
		"sub, lab := bd.chain(strings.Trim(head, \"[]\"), 0)",
		"if len(c.stills) > 0 || bd.on() {",
	} {
		if !strings.Contains(s, want) {
			t.Errorf("produce.go no longer contains %q", want)
		}
	}
	if !strings.Contains(s, "bd.bare = st.Bare") {
		t.Error("the Produce toggle no longer reaches the backdrop")
	}
	// black is still spelled, but in exactly one place: the branch the toggle
	// turns on. Anywhere else it is a clip padded with black by accident.
	i := strings.Index(s, "func (b bdrop) chain(")
	if i < 0 {
		t.Fatal("bdrop.chain is gone")
	}
	j := strings.Index(s[i+1:], "\nfunc ")
	if j < 0 {
		t.Fatal("bdrop.chain has no end")
	}
	rest := s[:i] + s[i+1+j:]
	if strings.Contains(rest, "color=black") {
		t.Error("a clip is padded with black outside the backdrop's own off switch")
	}
}

// Turned off, the backdrop still covers exactly the region it covered: the
// same frame, the same corner, filled with black. It is not a second code path
// that works out the border again -- getting a different answer there is how
// the concat demuxer ends up refusing a clip for being the wrong size.
func TestTurningTheBackdropOffGivesBlackOverTheSameRegion(t *testing.T) {
	for _, c := range []struct {
		what string
		b    bdrop
		want string
	}{
		{"the picture at its own size on a bigger frame",
			bdrop{w: 1920, h: 1080, x: 200, y: 60, bare: true},
			"[vpre]pad=1920:1080:200:60:color=black[bd0];"},
		{"an insert fitted and centred",
			bdrop{w: 1080, h: 1920, fit: true, bare: true},
			"[vpre]scale=1080:1920:force_original_aspect_ratio=decrease," +
				"pad=1080:1920:(ow-iw)/2:(oh-ih)/2:color=black[bd0];"},
	} {
		fc, lab := c.b.chain("vpre", 0)
		if lab != "bd0" {
			t.Errorf("%s: wrote %q, want bd0", c.what, lab)
		}
		if fc != c.want {
			t.Errorf("%s:\n got %s\nwant %s", c.what, fc, c.want)
		}
		if strings.Contains(fc, "gblur") {
			t.Errorf("%s: blurred anyway with the toggle off:\n%s", c.what, fc)
		}
		countLabels(t, fc, "vpre", "bd0")
		// the same frame either way, or the stream-copy join breaks
		on := c.b
		on.bare = false
		onFc, _ := on.chain("vpre", 0)
		for _, size := range []string{fmt.Sprintf("%d:%d", c.b.w, c.b.h)} {
			if !strings.Contains(fc, size) || !strings.Contains(onFc, size) {
				t.Errorf("%s: %s is not the frame both ways", c.what, size)
			}
		}
	}
}

// The toggle is a setting like the others: it survives a round trip, and it is
// stored the way round that leaves a project written before it existed with
// the blurred backdrop it was rendered with.
func TestTheBackdropToggleIsOnUnlessAProjectSaysOtherwise(t *testing.T) {
	if defaultProdSettings().Bare {
		t.Error("a new project starts with black frame edges")
	}
	var old prodSettings
	if err := json.Unmarshal([]byte(`{"container":"mp4","crf":24}`), &old); err != nil {
		t.Fatal(err)
	}
	if old.Bare {
		t.Error("a project saved before the toggle existed loads with black edges — " +
			"its finished video would stop matching the one it already made")
	}
	var round prodSettings
	b, err := json.Marshal(prodSettings{Bare: true})
	if err != nil {
		t.Fatal(err)
	}
	if err := json.Unmarshal(b, &round); err != nil {
		t.Fatal(err)
	}
	if !round.Bare {
		t.Errorf("the toggle did not survive being saved: %s", b)
	}
}

// countLabels checks the sub-graph is closed: every label it writes it reads
// again, except the one it hands on, and the only one it reads without writing
// is the one it was given.
func countLabels(t *testing.T, fc, in, out string) {
	t.Helper()
	wrote, read := map[string]int{}, map[string]int{}
	for _, chain := range strings.Split(strings.TrimSuffix(fc, ";"), ";") {
		// [a][b]filters[c][d] -- leading labels are read, trailing written
		lead := true
		for _, part := range strings.Split(chain, "[") {
			name, rest, found := strings.Cut(part, "]")
			if !found {
				continue
			}
			if lead {
				read[name]++
			} else {
				wrote[name]++
			}
			if strings.TrimSpace(rest) != "" {
				lead = false
			}
		}
	}
	for l, n := range wrote {
		if l == out {
			continue
		}
		if read[l] != n {
			t.Errorf("label %q is written %d times and read %d:\n%s", l, n, read[l], fc)
		}
	}
	if read[in] != 1 {
		t.Errorf("the incoming label %q is read %d times, want once:\n%s", in, read[in], fc)
	}
	if wrote[out] != 1 {
		t.Errorf("the outgoing label %q is written %d times, want once:\n%s", out, wrote[out], fc)
	}
	if _, ok := wrote[in]; ok {
		t.Errorf("the sub-graph overwrote the label it was handed:\n%s", fc)
	}
}

// The toggle exists on the Produce grid, is read into the settings and is
// written back out of them -- the same three points every other Produce
// setting is pinned at.
func TestTheBackdropToggleIsWired(t *testing.T) {
	b, err := os.ReadFile("produce.go")
	if err != nil {
		t.Fatal(err)
	}
	src := string(b)
	for _, want := range []string{
		`p.blur = gtk.NewCheckButtonWithLabel("blurred")`,
		`p.blur.SetActive(true)`,
		`check(1, 1, "Frame edges:", p.blur)`, // named in the label column, like every other row
		`Bare:      !p.blur.Active(),`,
		`p.blur.SetActive(!st.Bare)`,
	} {
		if !strings.Contains(src, want) {
			t.Errorf("produce.go does not contain %q", want)
		}
	}
}
