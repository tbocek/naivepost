package main

// The Mono toggle on the produce page. The choice has to hold in three places
// at once -- the filter graph that builds each clip, the encoder flags on the
// final pass, and the narration's pan -- because the clips are joined by a
// concat stream COPY: a layout decided per clip is the video's layout, and two
// clips that decided differently are one broken concat list. These tests pin
// each of the three to the one Mono flag, and the widget that sets it to the
// settings that carry it.

import (
	"encoding/json"
	"os"
	"strings"
	"testing"
)

// TestTheLayoutFollowsTheOneToggle: every string the filter graph builds from
// the settings says the same layout, and it is the toggle that picks it.
func TestTheLayoutFollowsTheOneToggle(t *testing.T) {
	stereo, mono := prodSettings{}, prodSettings{Mono: true}

	if got := audLayout(stereo); got != "stereo" {
		t.Errorf("audLayout off = %q, want stereo", got)
	}
	if got := audLayout(mono); got != "mono" {
		t.Errorf("audLayout on = %q, want mono", got)
	}

	// aformat is the per-clip contract: every audio path in encodeClip runs
	// through it, so this string IS the clip's layout
	if got := audFmt(stereo); !strings.Contains(got, "channel_layouts=stereo") {
		t.Errorf("audFmt off = %q, want a stereo layout in it", got)
	}
	if got := audFmt(mono); !strings.Contains(got, "channel_layouts=mono") {
		t.Errorf("audFmt on = %q, want a mono layout in it", got)
	}

	// the narration arrives mono from the synthesizer either way; the pan is
	// what spreads it, and a mono video has nothing to spread it across
	if got := voicePan(stereo); got != "pan=stereo|c0=c0|c1=c0" {
		t.Errorf("voicePan off = %q", got)
	}
	if got := voicePan(mono); got != "pan=mono|c0=c0" {
		t.Errorf("voicePan on = %q", got)
	}
}

// TestTheEncoderAgreesWithTheGraph: -ac on the encode pass says the same number
// the graph's aformat said, for both containers, so a path that skips the graph
// cannot smuggle the other layout into the concat list.
func TestTheEncoderAgreesWithTheGraph(t *testing.T) {
	for _, c := range []struct {
		st   prodSettings
		want string
	}{
		{prodSettings{Container: "mp4", AudioKbps: 160}, "-ac 2"},
		{prodSettings{Container: "mp4", AudioKbps: 160, Mono: true}, "-ac 1"},
		{prodSettings{Container: "webm", AudioKbps: 160}, "-ac 2"},
		{prodSettings{Container: "webm", AudioKbps: 160, Mono: true}, "-ac 1"},
	} {
		got := strings.Join(audioArgs(c.st), " ")
		if !strings.Contains(got, c.want) {
			t.Errorf("audioArgs(%+v) = %q, want %q in it", c.st, got, c.want)
		}
	}
}

// TestMonoRidesInTheProject: the choice is part of the produce settings, so it
// comes back with the project -- and an untouched project's json is untouched,
// because off is the default and the tag says omitempty.
func TestMonoRidesInTheProject(t *testing.T) {
	b, err := json.Marshal(prodSettings{Mono: true})
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(b), `"mono":true`) {
		t.Errorf("marshalled settings %s, want a mono field", b)
	}
	var st prodSettings
	if err := json.Unmarshal(b, &st); err != nil || !st.Mono {
		t.Errorf("mono did not survive the round trip: %v %+v", err, st)
	}
	if b, _ = json.Marshal(prodSettings{}); strings.Contains(string(b), "mono") {
		t.Errorf("default settings wrote %s — mono off should write nothing", b)
	}
}

// TestTheMonoToggleIsWired pins the widget to the settings: the check button
// exists on the grid, prodSettings() reads it, applyProdSettings writes it, and
// the summary line names the layout so the choice can be read back off the page.
func TestTheMonoToggleIsWired(t *testing.T) {
	b, err := os.ReadFile("produce.go")
	if err != nil {
		t.Fatal(err)
	}
	src := string(b)
	for _, want := range []string{
		`p.mono = gtk.NewCheckButtonWithLabel("mono")`,
		`check(0, 1, "Channels:", p.mono)`, // named in the label column, like every other row
		`Mono:      p.mono.Active(),`,
		`p.mono.SetActive(st.Mono)`,
		`audLayout(st)`,
	} {
		if !strings.Contains(src, want) {
			t.Errorf("produce.go does not contain %q", want)
		}
	}
}

// The settings grids: three columns of menus, then two columns of everything
// else, and rows of one kind of control in each.
//
// It was two columns and seven rows -- the sound settings stacked under the
// picture settings, the page's whole right-hand half empty -- and every tick
// carried a leading label that said the same thing the tick did ("Frame
// timing: [x] Peak frame rate (VFR)"). Then it was three columns of one
// subject each, which put a slider in with the dropdowns of every column and
// left the six menus in three tall rows: a form is read across, and a column
// whose rows are not the same height is not a column anybody reads down.
//
// So: six dropdowns in two rows of three, the narration's three in the row
// under them, then the sliders and the ticks.
func TestTheProduceSettingsAreThreeColumns(t *testing.T) {
	body := funcBody(t, "produce.go", `func \(a \*App\) buildProduce\(\)`)
	for _, want := range []string{
		// two rows of three menus, in the order a render is described in
		`at(0, 0, "Container:", p.container)`,
		`at(2, 0, "Preset:", p.preset)`,
		`at(0, 1, "Resolution:", p.height)`,
		`at(2, 1, "Audio:", p.abr)`,
		// the second block is a grid of its own: the same rows two across,
		// because a dropdown-and-slider row is wider per item than a menu and
		// one set of columns cannot serve both widths
		"low := gtk.NewGrid()",
		"box.Append(low)",
		// the narration's own row, which goes when the narration does
		`p.subsLbl = lbl(subs, 0, 0, "Subtitles:", subRow)`,
		`p.gvolLbl = lbl(subs, 1, 0, "Game audio:", p.gvol)`,
		// ...and the ticks, which are rows like any other: the subject in the
		// label column, dim, and the answer on the tick
		"check := func(col, row int, name string, w *gtk.CheckButton) {",
		`check(1, 0, "Frame timing:", p.vfr)`, // on the CRF row, beside it
	} {
		if !strings.Contains(body, want) {
			t.Errorf("the settings grid no longer contains %q", want)
		}
	}
	// a tick's own label is the ANSWER, one word where one will do: the
	// subject is in the label column beside it, and "Frame timing: [x] Peak
	// frame rate (VFR)" was the same sentence twice
	for _, gone := range []string{`"Peak frame rate (VFR)"`, `"Mono (one channel)"`, `"Blurred backdrop"`} {
		if strings.Contains(body, gone) {
			t.Errorf("a tick says its subject twice: %s", gone)
		}
	}
	// VFR shares the row with the CRF slider it was two rows under
	crf := strings.Index(body, `lbl(low, 0, 0, "Quality (CRF):", p.crf)`)
	vfr := strings.Index(body, `check(1, 0, "Frame timing:", p.vfr)`)
	if crf < 0 || vfr < 0 {
		t.Fatal("the CRF slider and the VFR tick are not both placed")
	}
	// ...and the slider is its own width rather than the column's: a grid
	// stretches what it holds, and a scale filling two columns is half the
	// form spent on one number between 14 and 34
	if !strings.Contains(body, "formSlider(p.crf, ") || !strings.Contains(body, "formSlider(p.gvol, ") {
		t.Error("a slider is built by hand rather than in the app's one shape (formSlider)")
	}
	// ...and a tick's own words are not dimmed: dim is how this app draws a
	// control that does not work, so a dimmed label on a live checkbox says
	// the two disagree
	if strings.Contains(body, `w.AddCSSClass("dim-label")`) {
		t.Error("the ticks are dimmed, which is what a dead control looks like")
	}
	// nothing in a row is stretched to the height of the slider in it
	for _, want := range []string{"l.SetVAlign(gtk.AlignCenter)", "d.SetVAlign(gtk.AlignCenter)"} {
		if !strings.Contains(body, want) {
			t.Errorf("the grid no longer centres its rows: %q", want)
		}
	}
	// a slider's own reading is drawn in the foreground colour: dimmed, it is
	// the app's own way of saying a control is dead
	if !strings.Contains(readSrc(t, "main.go"), "scale value, scale marks label { color: @theme_fg_color; }") {
		t.Error("a slider's value and marks are back in the dimmed colour")
	}
}

// Each column as wide as what is in it. Filling the page, a grid hands the
// slack to whichever column has a child that will take it -- so "Quality
// (CRF)" and "Frame timing" sat a hand's width apart with nothing between
// them, on a page that is mostly empty to the right of both.
func TestTheSettingsColumnsTakeTheirOwnWidth(t *testing.T) {
	src := readSrc(t, "produce.go")
	for _, want := range []string{
		"grid.SetHAlign(gtk.AlignStart)",
		"grid.SetHExpand(false)",
		"low.SetHAlign(gtk.AlignStart)",
		"low.SetHExpand(false)",
		"low.SetColumnHomogeneous(false)",
		// the subtitle row is in a grid of ITS OWN: it is much wider than the
		// rows under it, and sharing a grid it set their column width and
		// pushed the right-hand pair a hand's width away
		"subs.SetHAlign(gtk.AlignStart)",
		"box.Append(subs)",
		// and nothing inside a cell claims the slack either
		"l.SetHExpand(false)",
		"e.SetHExpand(false)",
	} {
		if !strings.Contains(src, want) {
			t.Errorf("produce.go does not contain %q -- the columns spread again", want)
		}
	}
}
