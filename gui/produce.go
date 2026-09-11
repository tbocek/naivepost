package main

// Produce: every clip cut from its own recording, narration over ducked game
// audio, joined by stream copy, loudness-normalized. A clip grows (maxExtend)
// or the narration speeds up (maxTempo) when a line does not fit; both are
// logged. Encoding happens once, per clip -- burned subtitles go into that
// encode.
//
// produce/clips/c000.<ext>   per-clip encodes
// produce/clips/final.srt    subtitles on the produced timeline (scratch)
// produce/final.<container>  the upload

import (
	"encoding/json"
	"errors"
	"fmt"
	"math"
	"os"
	"os/exec"
	"path/filepath"
	"sort"
	"strconv"
	"strings"
	"sync"

	"github.com/diamondburned/gotk4/pkg/glib/v2"
	"github.com/diamondburned/gotk4/pkg/gtk/v4"
)

const (
	narrLead  = 0.3  // the earliest a line may start into its clip (writer's "at" 0 lands here)
	narrGap   = 0.3  // ...and the breath left between two lines on the same clip
	narrTail  = 0.2  // ...and after the last one, before the clip is over
	maxExtend = 4.0  // seconds a clip may grow to fit its line
	maxTempo  = 1.25 // ... and how much the line may be sped up after that
	loudFlt   = "loudnorm=I=-14:TP=-1.5:LRA=11"
	// clipCeil limits every clip's audio before its AAC encode: lanes are mixed
	// at their recorded levels (normalize=0), so two lanes are louder than one,
	// and the loudnorm pass over the joined file comes too late to undo a
	// clipped encode. level=disabled keeps alimiter from scaling back up to 0
	// dBFS. On every clip, not only mixed ones: the join is a copy, so a filter
	// some clips went through is a seam.
	clipCeil = "alimiter=limit=0.891:level=disabled" // -1 dBFS
)

// audFmt is the shape every clip's audio is forced into. Every clip the same,
// or the concat copy produces a file whose second half is silent on half the
// players -- which is also why the stereo/mono choice has to reach in here
// rather than sit on the encoder at the end: the clips are joined by copying,
// so a layout decided per clip IS the layout of the video.
func audFmt(st prodSettings) string {
	return "aformat=sample_fmts=fltp:sample_rates=48000:channel_layouts=" + audLayout(st)
}

// audLayout is that choice by its ffmpeg name. Mono halves the audio bitrate's
// work for a screen recording whose two sides were the same signal all along --
// which is most of them, and is exactly what the timeline says when it draws
// such a recording on one lane (see sameLanes).
func audLayout(st prodSettings) string {
	if st.Mono {
		return "mono"
	}
	return "stereo"
}

type prodSettings struct {
	Container string  `json:"container"`
	Codec     string  `json:"codec"`
	CRF       int     `json:"crf"`
	Preset    string  `json:"preset"`
	Height    int     `json:"height"` // 0 = keep source
	FPS       float64 `json:"fps"`    // 0 = keep source
	VFR       bool    `json:"vfr,omitempty"`
	AudioKbps int     `json:"audio_kbps"`
	Mono      bool    `json:"mono,omitempty"` // one channel out, whatever went in
	GameVol   float64 `json:"game_vol"`
	// Bare leaves the parts of the frame the picture does not reach black,
	// instead of filling them with a blurred blow-up of the picture itself.
	// Stored the wrong way round on purpose: the blurred backdrop is the
	// default, and a project written before this setting existed has to keep
	// getting it.
	Bare bool   `json:"bare,omitempty"`
	Subs string `json:"subs"` // burn | mux | sidecar | none
	// the languages the subtitles are also written in (translate.go). The
	// session's own language is always there and is not in this list, so an
	// empty list -- every project written before this -- is one track and no
	// translation.
	SubLangs []string `json:"sub_langs,omitempty"`
	OutFile  string   `json:"out_file"`
	// There was a second subtitle setting here: what the track CARRIES -- the
	// narration, or the transcript of what the people in the recording said.
	// It is gone, and the track is the narration.
	//
	// The transcript half was a second answer to a question the app already
	// answers twice: the Cut page's captions put chosen lines on screen as
	// text effects, and the user context is where "put what I say on screen"
	// is asked for. A dropdown offering a third path -- every spoken line,
	// automatically, in a subtitle track -- is a setting whose result nothing
	// on the pages before it can show you. Old projects that stored subs_from
	// simply lose it: unknown keys are ignored, and their subtitles become the
	// narration's.
}

// captionLines is what the subtitle track carries for one clip: the narration
// lines the writer put on it.
// captionLines is what the subtitle track carries for a clip: the narration's
// lines where there is a narration, and otherwise what was SAID in the clip,
// off the transcript (transcriptSubs). The track used to be the voice-over's
// alone, so a session with nobody narrating -- a read to camera, where the
// speech IS the video -- asked for a track in the file and got a file with no
// track in it, and nothing said why.
func captionLines(c prodClip) []prodLine {
	if len(c.lines) > 0 {
		return c.lines
	}
	return c.subs
}

// transcriptSubs puts each footage clip's own speech on it as caption-only
// lines, in the clip's output seconds. Rows are the session timeline's
// (sessionRows): a row overlapping the clip is clipped to it, and a row on the
// narrator's own microphone is not a subtitle, since the video does not play
// that microphone (tlLabel).
func transcriptSubs(clips []prodClip, words []srcWord, narr string) {
	for i := range clips {
		c := &clips[i]
		if c.video == nil || c.ins != "" || c.freeze {
			continue
		}
		s0, s1 := c.sessS, c.sessS+c.length*c.speed()
		var mine []srcWord
		for _, w := range words {
			if w.s >= s0-0.01 && w.e <= s1+0.01 && (narr == "" || w.src != narr) {
				mine = append(mine, w)
			}
		}
		c.subs = wordCues(mine, s0, c.speed(), c.length)
	}
}

// wordCues groups one clip's words into caption lines, in the clip's own
// output seconds.
//
// Per WORD and not per transcript line, because a transcript line is not what
// the video says. The lines come off Prepare, before the cut, and the cut
// removes stretches inside them: a line clipped to the clip kept its whole
// text, so the join at 1:45 read "…public state of the art, and the whole run
// took roughly 13" over footage that says "…public state of the art" -- words
// that are not in the video, and are read AGAIN when the retake plays. Eleven
// of that session's eighty-one lines were cut into. Built from the words that
// survive, the subtitles say what the video says, and there is nothing left
// for them to be wrong about.
//
// A cue ends at a breath (subBreak), or when it has as much on it as two rows
// hold, or after subCueMax seconds -- a player wraps what it is given at its
// own font size, so a cue longer than two rows is one it will break into four.
func wordCues(words []srcWord, s0, speed, length float64) []prodLine {
	var out []prodLine
	var cur []srcWord
	flush := func() {
		if len(cur) == 0 {
			return
		}
		var txt []string
		for _, w := range cur {
			txt = append(txt, w.raw)
		}
		at := math.Max(0, (cur[0].s-s0)/speed)
		end := math.Min(length, (cur[len(cur)-1].e-s0)/speed)
		if end > at {
			out = append(out, prodLine{text: strings.Join(txt, " "), at: at, delay: at, dur: end - at})
		}
		cur = nil
	}
	n := 0
	for i, w := range words {
		if len(cur) > 0 {
			long := n+1+len(w.raw) > 2*subRowChars
			if w.s-cur[len(cur)-1].e >= subBreak || long || (w.e-cur[0].s)/speed >= subCueMax {
				flush()
				n = 0
			}
		}
		cur = append(cur, w)
		n += len(w.raw) + 1
		_ = i
	}
	flush()
	return out
}

const (
	// a gap between two words this long ends a caption: past it they are two
	// thoughts, and a caption that spans the pause is one the eye has already
	// finished reading
	subBreak = 0.6
	// how many characters a row of a caption holds (wrapSub wraps at this),
	// and how long one may stay up before the next takes over
	subRowChars = 42
	subCueMax   = 6.0
)

var (
	prodContainers = []string{"mp4", "mkv", "webm"}
	prodCodecs     = []string{"h264", "h265", "vp9"}
	prodPresets    = []string{"ultrafast", "veryfast", "fast", "medium", "slow", "veryslow"}
	prodHeights    = []string{"720p", "1080p", "original"}
	prodFPS        = []string{"source", "60", "30", "24"}
	prodABR        = []string{"128", "192", "256", "320"}
	prodSubsLbl    = []string{"burned in", "track in file", "sidecar .srt", "none"}
	prodSubsKey    = []string{"burn", "mux", "sidecar", "none"}
)

type producer struct {
	a *App

	container, codec, preset, height, fps, abr, subs *gtk.DropDown
	// which languages the subtitles are also written in: a menu of ticks, and
	// the button that says which are on (syncLangs)
	langs     *gtk.MenuButton
	langsLbl  *gtk.Label
	langBox   *gtk.Popover
	langTicks map[string]*gtk.CheckButton
	// the two controls that exist to carry a narration, with their labels: how
	// loud the game sits UNDER the voice, and what becomes of the voice's
	// subtitles. With no narration they are about nothing, and they go
	// (syncNarrOff).
	subsLbl, gvolLbl *gtk.Label
	vfr, mono, blur  *gtk.CheckButton
	crf, gvol        *gtk.Scale
	again            *gtk.Button // ↻ on the Transcode heading: encode again
	save             *gtk.Button // ...and ⤓ beside it: the video, somewhere else
	outFile          string      // always produce/final.<container> (setOut)
	inputs, out      *gtk.Label  // the two rows every step has
	guard            bool        // suppresses feedback while applying a project
}

// ---- settings ---------------------------------------------------------------

func pickText(d *gtk.DropDown, list []string) string {
	i := int(d.Selected())
	if i < 0 || i >= len(list) {
		i = 0
	}
	return list[i]
}

func setPick(d *gtk.DropDown, list []string, want string) {
	for i, s := range list {
		if s == want {
			d.SetSelected(uint(i))
			return
		}
	}
}

func atoiOr(s string, def int) int {
	if n, err := strconv.Atoi(s); err == nil {
		return n
	}
	return def
}

// syncNarrOff shows or hides what only a narration needs. Called when the tick
// on the Narrate page changes and when a project's answer arrives, and safe
// before the page exists -- a project can load before Produce is built.
func (a *App) syncNarrOff() {
	p := a.prod
	if p == nil || p.subs == nil {
		return
	}
	// the game-volume slider only: the subtitles are no longer the
	// narration's alone (captionLines), so their choice stays whatever the
	// narration does
	on := !a.narrOff
	for _, w := range []interface{ SetVisible(bool) }{
		p.gvol, p.gvolLbl,
	} {
		if w != nil {
			w.SetVisible(on)
		}
	}
}

// defaultProdSettings is the render a project gets before anyone touches the
// Produce page: h264 in mp4 at the quality this pipeline was tuned on. Named
// because two places need it -- the page before it is built, and a new project,
// which has to come back to these rather than to zeroes (a 0 CRF is lossless
// and a 0 fps is not a video at all).
func defaultProdSettings() prodSettings {
	return prodSettings{Container: "mp4", Codec: "h264", CRF: 24, Preset: "veryslow",
		Height: 1080, FPS: 30, AudioKbps: 128, GameVol: 0.22, Subs: "sidecar"}
}

// UnmarshalJSON seeds the defaults before decoding so an ABSENT game_vol (an
// older project: keep 0.22) and a stored 0 (a deliberate pick: silence the
// game) stop meaning the same thing. Only GameVol: CRF's slider starts at 14,
// so its 0 guard cannot be wrong.
func (s *prodSettings) UnmarshalJSON(b []byte) error {
	type raw prodSettings // a defined type carries no methods, so no recursion
	v := raw{GameVol: defaultProdSettings().GameVol}
	if err := json.Unmarshal(b, &v); err != nil {
		return err
	}
	*s = prodSettings(v)
	return nil
}

// gameVol is the level the render plays the game at under a narration line.
// Asked on its own because the Narrate preview has to duck to it on the
// playback tick, and reading a page of widgets ten times a second to get one
// number is not what that tick is for.
func (a *App) gameVol() float64 {
	if a == nil || a.prod == nil || a.prod.gvol == nil {
		return defaultProdSettings().GameVol
	}
	return a.prod.gvol.Value()
}

func (a *App) prodSettings() prodSettings {
	p := a.prod
	if p == nil {
		return defaultProdSettings()
	}
	st := prodSettings{
		Container: pickText(p.container, prodContainers),
		Codec:     pickText(p.codec, prodCodecs),
		CRF:       int(math.Round(p.crf.Value())),
		Preset:    pickText(p.preset, prodPresets),
		Height:    atoiOr(strings.TrimSuffix(pickText(p.height, prodHeights), "p"), 0),
		FPS:       float64(atoiOr(pickText(p.fps, prodFPS), 0)),
		VFR:       p.vfr.Active(),
		AudioKbps: atoiOr(pickText(p.abr, prodABR), 128),
		Mono:      p.mono.Active(),
		Bare:      !p.blur.Active(),
		GameVol:   p.gvol.Value(),
		Subs:      prodSubsKey[int(p.subs.Selected())],
		SubLangs:  p.pickedLangs(),
		OutFile:   p.outFile,
	}
	// webm carries neither h264 nor aac; silently producing an unplayable
	// file would be worse than overriding the pick and saying so
	if st.Container == "webm" && st.Codec != "vp9" {
		st.Codec = "vp9"
	}
	return st
}

func (a *App) applyProdSettings(st *prodSettings) {
	p := a.prod
	if p == nil || st == nil {
		return
	}
	p.guard = true
	setPick(p.container, prodContainers, st.Container)
	setPick(p.codec, prodCodecs, st.Codec)
	setPick(p.preset, prodPresets, st.Preset)
	setPick(p.height, prodHeights, tierLabel(st.Height))
	setPick(p.fps, prodFPS, fmtOpt(st.FPS))
	p.vfr.SetActive(st.VFR)
	setPick(p.abr, prodABR, strconv.Itoa(st.AudioKbps))
	p.mono.SetActive(st.Mono)
	p.blur.SetActive(!st.Bare)
	for i, k := range prodSubsKey {
		if k == st.Subs {
			p.subs.SetSelected(uint(i))
		}
	}
	on := map[string]bool{}
	for _, c := range st.SubLangs {
		on[c] = true
	}
	for code, t := range p.langTicks {
		t.SetActive(on[code])
	}
	p.syncLangs()
	if st.CRF > 0 {
		p.crf.SetValue(float64(st.CRF))
	}
	// no "> 0" guard here, unlike CRF above: 0 is a real level -- game fully
	// silent under the narration -- and UnmarshalJSON has already filled in the
	// default for a project that stored none.
	p.gvol.SetValue(st.GameVol)
	p.guard = false
	// the destination is not restored from the project: it is produce/final,
	// and where produce/ is follows the project itself (followOutDir)
	p.setOut(filepath.Join(a.produceDir(), "final"+filepath.Ext(p.outFile)))
	p.syncExt()
}

// followOutDir retargets the produced file when the project moves: the video
// is always produce/final beside the rest of the step's work, and produce/ is
// under the project's own folder.
func (a *App) followOutDir() {
	p := a.prod
	if p == nil {
		return
	}
	p.setOut(filepath.Join(a.produceDir(), filepath.Base(p.outFile)))
}

// fmtOpt renders a numeric dropdown value, with 0 meaning "source".
// tierLabel is the dropdown's word for a stored height: 0 is "original" and
// anything else its p-name. A height an old project saved that is no longer
// offered ("2160") matches nothing, and setPick then leaves the default 1080p
// standing -- the same trade the shorter list itself makes.
func tierLabel(h int) string {
	if h <= 0 {
		return "original"
	}
	return fmt.Sprintf("%dp", h)
}

func fmtOpt(v float64) string {
	if v <= 0 {
		return "source"
	}
	return strconv.Itoa(int(v))
}

func (p *producer) setOut(path string) {
	p.outFile = path
}

// syncExt keeps the output filename's extension on the chosen container.
func (p *producer) syncExt() {
	if p.guard || p.outFile == "" {
		return
	}
	want := "." + pickText(p.container, prodContainers)
	if filepath.Ext(p.outFile) != want {
		p.setOut(strings.TrimSuffix(p.outFile, filepath.Ext(p.outFile)) + want)
	}
}

// ---- page -------------------------------------------------------------------

func (a *App) buildProduce() gtk.Widgetter {
	p := &producer{a: a}
	a.prod = p
	defer a.syncNarrOff()  // a project loaded before this page was built has already said
	defer a.syncSubLangs() // ...and said which language it is spoken in

	// no paragraph at the top: what this step does is in the ⓘ in the header bar
	// (steps[].help), which the settings below it can now have the space of
	grid := gtk.NewGrid()
	grid.SetColumnSpacing(10)
	grid.SetRowSpacing(6)
	grid.SetColumnHomogeneous(false)
	// each column as wide as what is in it, and the whole grid as wide as its
	// columns. Filling the page, the grid hands the slack to whichever column
	// has a child that will take it -- so "Quality (CRF)" and "Frame timing"
	// ended up a hand's width apart with nothing between them, on a page that
	// is mostly empty to the right of both.
	grid.SetHAlign(gtk.AlignStart)
	grid.SetHExpand(false)
	// The second grid: the same rows, two across. The six menus are one kind of
	// thing and fit three across; a dropdown with a slider, a slider with a tick
	// and two ticks are wider, so three rows of two. One grid for both put the
	// ticks against the far edge with a hand's width of nothing before them.
	low := gtk.NewGrid()
	// ...and a third for the subtitles, ALONE. A grid column is as wide as
	// the widest thing in it, so the subtitle row -- a dropdown, a word and a
	// menu -- set the width of the column the CRF slider and the mono tick sit
	// in, and pushed "Frame timing" and "Frame edges" a hand's width to the
	// right of them. A row whose answer is that much wider than the rest
	// belongs in a grid of its own.
	subs := gtk.NewGrid()
	subs.SetColumnSpacing(10)
	subs.SetRowSpacing(6)
	subs.SetColumnHomogeneous(false)
	subs.SetHAlign(gtk.AlignStart)
	subs.SetHExpand(false)
	low.SetColumnSpacing(10)
	low.SetRowSpacing(6)
	low.SetColumnHomogeneous(false)
	low.SetHAlign(gtk.AlignStart)
	low.SetHExpand(false)
	// One width per label column, shared by both grids, every label flush left:
	// right-aligned across two grids of different widths, the labels came out on
	// four different left edges.
	var lblCol [3]*gtk.SizeGroup
	// a label and the thing it names, in whichever grid the caller is filling
	lbl := func(g *gtk.Grid, col, row int, label string, w gtk.Widgetter) *gtk.Label {
		l := gtk.NewLabel(label)
		l.SetXAlign(0)
		l.AddCSSClass("dim-label")
		if col < len(lblCol) {
			if lblCol[col] == nil {
				lblCol[col] = gtk.NewSizeGroup(gtk.SizeGroupHorizontal)
			}
			lblCol[col].AddWidget(l)
		}
		// centred in the row rather than filling it: a row holding a slider is
		// as tall as the slider -- which draws its number above itself -- and
		// everything else in that row was being stretched to match. A dropdown
		// three times its own height reads as a text field somebody typed into.
		l.SetVAlign(gtk.AlignCenter)
		l.SetHExpand(false)
		// ...and the answer takes its own width too: a child that expands
		// makes its column swallow the row (see the grid's own halign)
		if e, ok := w.(interface{ SetHExpand(bool) }); ok {
			e.SetHExpand(false)
		}
		g.Attach(l, col*2, row, 1, 1)
		g.Attach(w, col*2+1, row, 1, 1)
		return l
	}
	at := func(col, row int, label string, w gtk.Widgetter) *gtk.Label {
		return lbl(grid, col, row, label, w)
	}
	// a tick is a row like any other: the subject in the dim label column, the
	// answer on the control. The tick's own words are NOT dimmed -- dim is what
	// this app draws a dead control in.
	check := func(col, row int, name string, w *gtk.CheckButton) {
		lbl(low, col, row, name, w)
	}
	dd := func(list []string, sel int, tip string) *gtk.DropDown {
		d := gtk.NewDropDownFromStrings(list)
		d.SetSelected(uint(sel))
		d.SetTooltipText(tip)
		// its own width, not the column's: the grid column is as wide as the
		// CRF slider, and "mp4" stretched to slider width reads as a text
		// field, not a menu
		d.SetHAlign(gtk.AlignStart)
		d.SetVAlign(gtk.AlignCenter) // see at(): a row is only as tall as its tallest thing
		return d
	}

	p.container = dd(prodContainers, 0, "mp4 plays everywhere; mkv keeps subtitle tracks best; webm forces VP9 + Opus")
	p.container.Connect("notify::selected", func() { p.syncExt() })
	p.codec = dd(prodCodecs, 0, "h264 is the safe upload; h265 is smaller but slower; vp9 is for webm")
	p.preset = dd(prodPresets, 5, "how long the encoder may think — slower means smaller at the same quality")
	p.height = dd(prodHeights, 1, "the short side of the frame — the cut page's aspect sets its shape; original keeps the footage's own size")
	p.fps = dd(prodFPS, 2, "output frame rate — a ceiling rather than a target with VFR on")
	p.abr = dd(prodABR, 0, "audio bitrate in kbit/s")
	p.subs = dd(prodSubsLbl, 2, "what to do with the subtitles: burned "+
		"into the picture, a separate track inside the file, an .srt beside it, or nothing")

	// ...and which languages they are also written in. A menu of ticks rather
	// than a dropdown: more than one at a time is the whole point, and the
	// button says which are on so the answer is readable with the menu shut.
	p.langBox = gtk.NewPopover()
	langs := gtk.NewBox(gtk.OrientationVertical, 4)
	langs.SetMarginTop(6)
	langs.SetMarginBottom(6)
	langs.SetMarginStart(10)
	langs.SetMarginEnd(10)
	p.langTicks = map[string]*gtk.CheckButton{}
	for _, l := range subLangs {
		l := l
		c := gtk.NewCheckButtonWithLabel(l.name)
		c.ConnectToggled(func() {
			p.syncLangs()
			if !p.guard {
				a.saveProjectNow()
			}
		})
		p.langTicks[l.code] = c
		langs.Append(c)
	}
	p.langBox.SetChild(langs)
	p.langs = gtk.NewMenuButton()
	p.langs.SetPopover(p.langBox)
	p.langs.SetHAlign(gtk.AlignStart)
	p.langs.SetVAlign(gtk.AlignCenter)
	p.langs.SetTooltipText("Also write the subtitles in these languages, translated. " +
		"The language the session is spoken in is always written and is not offered here. " +
		"Each becomes a track of its own in the file, or an .srt of its own beside it.")
	p.syncLangs()

	// VFR makes the rate above a ceiling. Capture from a headset is variable by
	// nature -- it renders what it can, and the rate above is the peak it
	// reaches, not the rate it holds. Forced up to a constant rate that becomes
	// duplicated frames, which cost bitrate and buy nothing.
	p.vfr = gtk.NewCheckButtonWithLabel("peak rate (VFR)")
	p.vfr.SetTooltipText("Treat the frame rate above as a ceiling: footage faster than it is " +
		"dropped down to it, footage slower keeps its own rate instead of having frames " +
		"duplicated. Off, every clip is resampled to exactly that rate.")

	// Stereo out of habit rather than out of the footage: a screen capture is
	// usually one signal written to two channels (the timeline says so -- it
	// draws such a recording on one lane), and half the bitrate then goes on
	// carrying that signal a second time. Off by default all the same, because
	// a game that really is in stereo is a game whose stereo you would miss,
	// and this must not quietly flatten it.
	p.mono = gtk.NewCheckButtonWithLabel("mono")
	p.mono.SetTooltipText("Mix the finished audio down to a single channel. Worth it when " +
		"the capture's two sides carry the same signal — the same bitrate then goes on " +
		"one channel instead of two. Leave it off for anything with a real stereo image.")

	// What fills the frame where the picture does not reach (a camera pulled back
	// past the edge, a portrait cut of widescreen footage). On by default: black
	// bars read as a fault. A toggle here rather than on the cut because the
	// preview paints those edges black.
	p.blur = gtk.NewCheckButtonWithLabel("blurred")
	p.blur.SetActive(true)
	p.blur.SetTooltipText("Fill the empty edges of the frame with a blown-up, blurred " +
		"copy of the picture itself, instead of black. Off gives plain black bars — " +
		"which is also what the Cut preview draws, so turn it off if you want the " +
		"finished video to look exactly like the preview did.")

	// The two sliders in the app's one shape (formSlider): own width, value
	// beside the trough, one line tall -- a grid would stretch each over two
	// columns. The CRF mark is unlabelled; what it says is the tooltip's.
	p.crf = gtk.NewScaleWithRange(gtk.OrientationHorizontal, 14, 34, 1)
	p.crf.SetValue(24)
	formSlider(p.crf, "quality: lower is better and bigger (18–24 is the usual range; 24 is the default, marked)")
	p.crf.AddMark(24, gtk.PosBottom, "")

	p.gvol = gtk.NewScaleWithRange(gtk.OrientationHorizontal, 0, 1, 0.02)
	p.gvol.SetValue(0.22)
	formSlider(p.gvol, "how loud the original game audio sits under the narration")

	// Two grids: the six menus three across, then the rest two across (each row
	// one kind of control), with one shared width per label column.
	at(0, 0, "Container:", p.container)
	at(1, 0, "Codec:", p.codec)
	at(2, 0, "Preset:", p.preset)

	at(0, 1, "Resolution:", p.height)
	at(1, 1, "Frame rate:", p.fps)
	at(2, 1, "Audio:", p.abr)

	// ...and the second block, two across. The narration's pair is its first
	// row and goes with the narration: with the tick off it collapses
	// (syncNarrOff) rather than leaving labelled holes in the middle of the
	// form. Then the quality, with the frame timing that qualifies the rate
	// two rows above it, and last the two ticks about the finished file.
	// the two subtitle answers on one row -- what becomes of them, and which
	// languages they are also written in. One is about the other, and a row
	// apart they read as two unrelated settings.
	subRow := gtk.NewBox(gtk.OrientationHorizontal, 8)
	subRow.Append(p.subs)
	p.langsLbl = gtk.NewLabel("Translate:")
	p.langsLbl.AddCSSClass("dim-label")
	p.langsLbl.SetVAlign(gtk.AlignCenter)
	subRow.Append(p.langsLbl)
	subRow.Append(p.langs)
	p.subsLbl = lbl(subs, 0, 0, "Subtitles:", subRow)
	p.gvolLbl = lbl(subs, 1, 0, "Game audio:", p.gvol)

	lbl(low, 0, 0, "Quality (CRF):", p.crf)
	check(1, 0, "Frame timing:", p.vfr)
	check(0, 1, "Channels:", p.mono)
	check(1, 1, "Frame edges:", p.blur)

	// Where the video is written is not a question: produce/final with the
	// container's extension (syncExt). The Outputs group already opens the folder.
	p.setOut(filepath.Join(a.produceDir(), "final.mp4"))

	// No buttons of its own down here. Rendering is what this page does, so it
	// is what ▶ in the run bar means, and a finished run cues its result into
	// the picture below by itself -- a "Preview result" button beside a second
	// ▶ was two more ways to press the one thing the run bar already does, and
	// the pair of ▶s did not even mean the same thing.

	// The two rows every other step has, in the two places every other step has
	// them: what this one is working from, at the top, and what it has put on
	// disk, at the bottom right. Cut and Narrate answer the same question in the
	// same words, and this page used to answer neither -- what it had instead
	// was one dim paragraph in the middle that mixed its inputs in with its
	// encoder settings.
	p.inputs = inputsLabel()
	a.inStack.AddNamed(p.inputs, "produce") // the shared bar's Inputs line; see inStack in main.go

	openOut := gtk.NewButtonFromIconName("folder-open-symbolic")
	openOut.SetTooltipText("produce/ — the finished video, the per-clip encodes, the thumbnail and the upload text")
	openOut.ConnectClicked(func() { a.openFolder(a.produceDir()) })
	p.out = gtk.NewLabel("")
	outRow := gtk.NewBox(gtk.OrientationHorizontal, 8)
	outRow.Append(openOut)
	outRow.Append(p.out)
	a.outStack.AddNamed(outRow, "produce") // the shared bar's Outputs group; see outStack in main.go

	// no margins of its own: this box is one of the right column's rows and
	// the column carries them (below). 6 between the heading and its grids,
	// and between the two grids -- the same step the rest of the column uses
	// between a heading and its box.
	box := gtk.NewBox(gtk.OrientationVertical, 6)
	// the heading this half of the column is under, with the ↻ that runs it
	// again beside it -- the same mark, in the same corner, as the ↻ over the
	// thumbnail on the other half of the page
	p.again = gtk.NewButtonFromIconName("view-refresh-symbolic")
	p.again.AddCSSClass("flat")
	p.again.SetTooltipText("Encode the video again from the cut and these settings — " +
		"no model call, and the thumbnail and the upload text are left alone")
	p.again.ConnectClicked(func() { a.transcodeClicked() })
	// ...and the way out of the project, beside it: the finished video lives
	// in produce/ with the work it was made from, which is right for a project
	// and wrong for the thing you actually upload. The same ⤓ the thumbnail
	// wears, doing the same job one file over.
	p.save = gtk.NewButtonFromIconName("document-save-symbolic")
	p.save.AddCSSClass("flat")
	p.save.SetTooltipText("Save the finished video somewhere else — a copy; " +
		"produce/final stays where it is")
	p.save.ConnectClicked(func() { a.exportVideo() })
	box.Append(a.heading("Transcode", "How the finished video is encoded, and where it goes: "+
		"produce/final, beside everything else this step writes", p.save, p.again))
	box.Append(grid)
	box.Append(subs)
	box.Append(low)

	a.updateProduceInfo() // the rows say something before anything is clicked

	// The thumbnail half of the step -- what was the Publish page. The drawing
	// side fills the left of this page; the words the model wrote sit top
	// right, where they are read back and reworded; and the encoder settings
	// sit under them -- the knobs set once, below the text reread every run.
	// One page because one ▶ runs it all (produceClicked), and what that ▶
	// makes is one thing: the upload.
	drawSide, said := a.buildPublishPanes()

	// only the settings scroll: the words above stay put, and a settings grid
	// taller than its half slides rather than pushing the title off the page
	scroll := gtk.NewScrolledWindow()
	scroll.SetChild(box)
	scroll.SetPropagateNaturalHeight(true)

	// the column's own margins, for everything in it: 6 off the handle, 12 off
	// the window, 8 top and bottom -- the same four numbers every page keeps
	// (steps_test.go). They were on the rows instead, one row at a time, so
	// the words ended 24 from the window and the settings 12.
	right := gtk.NewBox(gtk.OrientationVertical, 6)
	right.SetSizeRequest(360, -1)
	margins(right, 8, 8, 6, 12)
	right.Append(said)
	right.Append(gtk.NewSeparator(gtk.OrientationHorizontal))
	right.Append(scroll)

	outer := gtk.NewPaned(gtk.OrientationHorizontal)
	outer.SetStartChild(drawSide)
	outer.SetEndChild(right)
	outer.SetResizeStartChild(true)
	outer.SetResizeEndChild(true)
	outer.SetShrinkStartChild(false)
	outer.SetShrinkEndChild(false)
	outer.SetVExpand(true)
	// half each, like Prepare: the picture being made and the words that go up
	// with it are both the work of this page, and left to itself the pane gave
	// the thumbnail whatever it asked for and the title, the description and
	// every encoder setting what was left
	openAtHalf(outer)

	page := gtk.NewBox(gtk.OrientationVertical, 4)
	page.Append(outer)
	return page
}

// exportVideo copies the finished video out of the project. A copy, not a
// move: produce/final is what the stamp is about (produce_stamp.go). On a
// goroutine, through the same copyInto every import uses, so an interrupted
// copy leaves a .part.
func (a *App) exportVideo() {
	p := a.prod
	if p == nil {
		return
	}
	if !exists(p.outFile) {
		a.setStatus("nothing to save yet — ▶ renders the video first")
		return
	}
	// named for the project rather than "final": the folder it came out of
	// says which project it is, and the copy is leaving that folder
	name := strings.TrimSuffix(filepath.Base(a.projPath), filepath.Ext(a.projPath)) + filepath.Ext(p.outFile)
	a.saveAs("Save the video", filepath.Dir(a.projPath), name, nil, func(out string) {
		src := p.outFile
		a.setStatus("saving " + filepath.Base(out) + "…")
		go func() {
			err := copyFile(src, out)
			glib.IdleAdd(func() {
				if err != nil {
					a.logf("!!! save video: %v", err)
					a.setStatus("could not save the video — see log")
					return
				}
				fi, _ := os.Stat(out)
				a.logf(">>> saved %s", out)
				a.setStatus(fmt.Sprintf("saved %s — %s", filepath.Base(out), humanSize(fi.Size())))
			})
		}()
	})
}

// updateProduceInfo redraws both rows: what the render reads and what it has
// written. Everything that changes either comes through here -- a cut edited
// next door, a finished run, a project load.
func (a *App) updateProduceInfo() {
	p := a.prod
	if p == nil {
		return
	}
	p.updateInputs()
	p.updateOut()
}

// updateInputs is the row Cut and Narrate open with, asking the same question
// of this page: what is actually going into the render? Three things, and not
// one of them is made here -- the clips come from Cut, the lines and their
// recordings from Narrate, and a line whose wav is missing is spoken before any
// video is encoded, which is minutes this row is the only warning of.
func (p *producer) updateInputs() {
	if p == nil || p.inputs == nil {
		return
	}
	a := p.a
	segs := a.produceSegs()
	entries := a.produceEntries()
	total := 0.0
	for _, s := range segs {
		total += s.length()
	}
	line := fmt.Sprintf("%s · %s", plural(len(segs), "clip"), mmss(total))
	detail := fmt.Sprintf("cut/cut.json — %d clips, %s of video (the produced file grows a little where the narration needs room)",
		len(segs), mmss(total))
	if len(segs) == 0 {
		line, detail = "no cut yet — build one on the Cut step", ""
	}
	spoken := 0
	for _, e := range entries {
		if exists(a.ttsWav(e)) {
			spoken++
		}
	}
	// What the run will DO, and nothing it will merely use. How many lines
	// there are, which voice speaks them, how many recordings are mixed in and
	// how many candidate thumbnails are on the page are all either said by
	// another page or shown by this one; what is worth a row here is the work
	// still owed -- lines with no wav yet, and an upload text nobody has
	// written -- because that is what the next ▶ spends its minutes on.
	switch {
	case len(entries) == 0:
		line += " · no narration"
	case spoken < len(entries):
		line += fmt.Sprintf(" · %d to speak", len(entries)-spoken)
		detail += fmt.Sprintf("\n\nnarrate/narration.json — %d lines, %d already in narrate/tts; the other %d are spoken first, before any video is encoded",
			len(entries), spoken, len(entries)-spoken)
	default:
		detail += fmt.Sprintf("\n\nnarrate/narration.json — %d lines, all of them already in narrate/tts", len(entries))
	}
	if _, auds := a.snappedSources(); len(auds) > 0 {
		detail += "\n\nMixed into each clip's own audio, for the stretch of it that was running while that clip was:"
		for _, p := range auds {
			detail += "\n" + baseName(p)
		}
	}
	if vp := a.voicePick; vp != nil && len(entries) > 0 {
		if v, ok := vp.current(); ok {
			detail += "\n\nSpoken by " + v.name + " (narrate/voice_ref.wav)"
		}
	}
	if a.pub != nil {
		if a.publishRecorded() {
			detail += "\n\npublish/publish.json — the upload text is written; ▶ redraws and re-renders without asking the model again (deleting publish/ starts the text over)"
		} else {
			line += " · no upload text"
			detail += "\n\nNo publish/publish.json yet — the first ▶ writes the title, the thumbnail instruction and the description before drawing anything"
		}
	}
	p.inputs.SetText(line)
	p.inputs.SetTooltipText(strings.TrimSpace(detail))
}

// updateOut is the line every step ends on: one folder, how many files, how
// big. Everything this step writes is under produce/ (produceDir, publishDir).
func (p *producer) updateOut() {
	if p == nil || p.out == nil {
		return
	}
	p.out.SetText(summarizeOutputs(p.a.produceDir()))
	tip := p.a.produceDir()
	if fi, err := os.Stat(p.outFile); err == nil {
		tip = fmt.Sprintf("%s — %s, %s\n\n%s", p.outFile,
			humanSize(fi.Size()), humanAgo(fi.ModTime()), p.a.produceDir())
	}
	p.out.SetTooltipText(tip)
}

// subsIndex is the label for a stored subtitle mode, defaulting to the first
// rather than panicking on a project written by a later version.
func subsIndex(key string) int {
	for i, k := range prodSubsKey {
		if k == key {
			return i
		}
	}
	return 0
}

// ---- inputs -----------------------------------------------------------------

// produceSegs prefers the live editor, so an unsaved tweak still renders.
//
// It is the cut as a SEQUENCE, which is not quite the cut as the timeline holds
// it: a spliced insert cuts the clip it sits in, and here it comes out as two
// clips with the card between them (splitSpliced). Every step after this one
// reads the cut through here, so they all see the same running order -- the one
// the narration is written against and the one that gets rendered.
func (a *App) produceSegs() []cutSeg {
	c := a.produceCut()
	// every piece remembers the scene it was cut from, before anything cuts
	// it: splitSpliced opens scenes for cards and applyFx cuts them again at
	// every rate boundary, and both copy the segment whole, so the stamp
	// travels with the pieces (cutSeg.Scene).
	segs := splitSpliced(c.Segs)
	for i := range segs {
		segs[i].Scene = i
	}
	return applyFx(segs, c.Fx)
}

// produceCut is the whole cut file the render works from: the live editor when
// it has one, what is saved otherwise; everything above the segment level reads
// it here so it cannot disagree with produceSegs. The editor counts as having a
// cut when it has segments, a timeline correction or a row of its own.
func (a *App) produceCut() cutFile {
	if ed := a.ed; ed != nil && (len(ed.segs) > 0 || len(ed.shift) > 0 || len(ed.cutLanes) > 0) {
		return cutFile{Segs: ed.segs, Aspect: ed.aspect, Fx: ed.fx, Shift: ed.shift,
			Rows: ed.rows, Lanes: ed.cutLanes, NRows: ed.nRows}
	}
	b, err := os.ReadFile(a.cutPath())
	if err != nil {
		return cutFile{}
	}
	var c cutFile
	if json.Unmarshal(b, &c) != nil {
		return cutFile{}
	}
	c.Fx = migrateFx(c.Fx)
	return c
}

func (a *App) produceEntries() []narrEntry {
	// no narration at all: whatever is written stays on the Narrate page, and
	// the render behaves as if it were not there -- nothing spoken, nothing
	// laid over a clip, no subtitle track (narrate.go's own tick). Read here
	// rather than at each of the four places that use these entries, because
	// this is the one seam every one of them comes through.
	if a.narrOff {
		return nil
	}
	if a.narr != nil {
		a.narr.pullRows()
		if len(a.narr.entries) > 0 {
			return append([]narrEntry(nil), a.narr.entries...)
		}
	}
	a.narr.flushSave() // a line typed a moment ago belongs in this render
	b, err := os.ReadFile(a.narrPath())
	if err != nil {
		return nil
	}
	var f struct{ Entries []narrEntry }
	if json.Unmarshal(b, &f) != nil {
		return nil
	}
	return f.Entries
}

// sessionTracks is the same placement for both kinds at once: the footage on
// the timeline, and the separate recordings beside it on the same clock. The
// render needs the second list for the same reason the cut page draws lanes
// from it -- a recording that was running while a clip was running is part of
// that clip's sound, and the only thing that says which stretch of it that is
// is where both of them sit on this one clock.
func (a *App) sessionTracks(vids, auds []string) ([]tlVideo, []tlAudio, error) {
	if len(vids) == 0 {
		return nil, nil, fmt.Errorf("no videos selected")
	}
	type st struct {
		path  string
		start float64
	}
	// same zero convention as session.tsv: the earliest moment any source
	// names, and 0:00 when none of them names one (srcClock)
	var all []st
	paths := append(append([]string{}, vids...), auds...)
	at, zero := srcClock(paths)
	for _, p := range paths {
		all = append(all, st{p, at[p]})
	}
	var out []tlVideo
	for _, s := range all[:len(vids)] { // the videos are the leading entries
		dur, err := ffprobeDur(s.path)
		if err != nil {
			return nil, nil, err
		}
		out = append(out, tlVideo{base: baseName(s.path), path: s.path,
			start: s.start - zero, wall: s.start, dur: dur, fps: ffprobeFPS(s.path)})
	}

	var rec []tlAudio
	for _, s := range all[len(vids):] {
		dur, err := ffprobeDur(s.path)
		if err != nil || dur <= 0 {
			continue // not something with sound in it; the lanes skip it too
		}
		rec = append(rec, tlAudio{base: baseName(s.path), path: s.path,
			start: s.start - zero, dur: dur})
	}
	// the corrections the cut page made to these clocks, put on before anything
	// is sorted or coloured. The render places its own tracks and would
	// otherwise place them off the raw timestamps -- the ones the hand was
	// dragging to correct in the first place.
	c := a.produceCut()
	for i := range out {
		out[i].start += c.Shift[out[i].base]
	}
	for i := range rec {
		rec[i].start += c.Shift[rec[i].base]
	}
	// the further tracks of a multi-track capture, which are recordings in every
	// way that matters here: a lane each, on the file's own clock, mixed under
	// the clips they were running through. They are built from the videos AFTER
	// those were corrected, so a row dragged to fix its clock takes its own
	// second track with it, and their own name is still a name a further drag
	// can be stored under (slideSrc, cut_shift.go).
	for _, au := range srcLanes(out, a.snappedTracks()) {
		if au.master {
			continue // that one is the footage's own sound, taken off its input
		}
		au.start += c.Shift[au.base]
		rec = append(rec, au)
	}
	// and the rows the cut put on the band itself, which are in cut.json and in
	// no source list -- the render has to be told about them or every scene cut
	// to one would resolve to whatever recording was rolling at that second
	// (cut_lane.go). They are placed by laneVideos and corrected here, exactly
	// as the editor does it.
	lanes := a.laneVideos(c.Lanes, out)
	for i := range lanes {
		lanes[i].start += c.Shift[lanes[i].base]
	}
	out = append(out, lanes...)
	sort.Slice(out, func(i, j int) bool { return out[i].start < out[j].start })
	sort.Slice(rec, func(i, j int) bool { return rec[i].start < rec[j].start })
	// the rows, worked out the same way the cut page works them out, because a
	// scene says which ROW its picture comes from (cutSeg.Cam) and the number
	// has to mean the same thing on both sides. Without this every scene would
	// resolve against row nought and a two-camera cut would render as one.
	assignLanes(out, c.Rows)
	return out, rec, nil
}

func hasAudioStream(path string) bool {
	out, err := exec.Command(ffTool("ffprobe"), "-v", "error", "-select_streams", "a:0",
		"-show_entries", "stream=codec_type", "-of", "csv=p=0", path).Output()
	return err == nil && strings.Contains(string(out), "audio")
}

// ---- render -----------------------------------------------------------------

// prodLine is one narration line placed inside a clip. at is where the writer
// put it, seconds from the clip's start; delay is where the mix actually
// starts it -- at, unless the line had to be pulled earlier to fit. delay is
// what adelay and the subtitles use; at is only its input.
type prodLine struct {
	wav   string // synthesized wav, "" = not spoken (captioned only)
	dur   float64
	text  string
	at    float64
	delay float64
	// pos is where the caption sits on the picture: "top", "center", or ""
	// for the bottom (narrEntry.Pos, already normalized). Only the subtitles
	// read it; the mix does not care where words are drawn.
	pos string
}

type prodClip struct {
	idx int
	// exactly one of these two is set. video is a stretch of a recording; ins is
	// a file that plays instead of one -- a card, a still, an animated ranking
	// (see cutSeg.Ins). An insert has no local time and no recording to run past
	// the end of, so the planning that fits narration into a slot is the same for
	// both and the geometry is not.
	video *tlVideo
	ins   string
	local float64 // start inside that recording
	// where this clip starts on the SESSION clock -- the clock the effects are
	// placed on. The camera reads it off the recording (video.start + local);
	// an insert has no recording, so it is kept here for both.
	sessS float64
	// the frame an insert is fitted into, so every clip in the concat list has
	// the same dimensions (clipBox). Unset for footage, which sets its own.
	boxW, boxH int

	// an audio-only insert: the picture is the session's own (video is set,
	// exactly as for footage), and this file is the clip's sound in the
	// capture's place. Spliced, the picture is one held frame (freeze below);
	// over a selection, it keeps running.
	snd string
	// where in that file the sound starts: nought for a file from disk, the
	// copied second for a stretch copied out of the session. snd says "the sound
	// comes from HERE, not the picture's input"; an insert covering the picture
	// alone (cutSeg.Mute) puts the recording underneath in this slot.
	sndAt float64
	// this clip brings no sound of its own -- an insert placed by a selection
	// scoped to the picture alone. Spliced, that means silence, and this is
	// what makes clipInput say the input has no audio to take. Overwriting, the
	// session's own sound is put in snd above instead and this is only a record
	// of why.
	mute bool
	// no separately-recorded lane is heard under this clip. The zero value is
	// the ordinary answer -- every recording that was running is mixed in -- and
	// this is set for the two ways that stops being true: the clip is time put
	// INTO the session rather than a stretch of it (anything spliced in), or
	// what it brought replaced everything that was audible (an ordinary insert
	// over a selection, a picture-alone paste). See clipMixes.
	noLanes bool
	// the one lane the clip's own sound stands in for: a sound laid over a
	// selection drawn in a single recording's lane replaces THAT recording and
	// leaves the others playing, so it is dropped from the mix by name.
	dropLane string
	// the lanes this clip's scene does not hear (cutSeg.Quiet), carried through
	// unread: clipMixes drops the separate recordings named here, and clipInput
	// treats the capture's own track as having no sound when the camera's own
	// lane is one of them.
	quiet []string

	length float64 // slot length after growing for the narration
	tempo  float64
	lines  []prodLine // empty = original audio only
	// the clip's own speech as caption-only lines, for the subtitle track when
	// there is no narration (transcriptSubs). Never in the mix: lines above is
	// what says a clip carries a voice-over, and this must not.
	subs []prodLine
	mix  []prodMix // separate recordings running under this clip

	// the effects (cut_fx.go), already resolved into per-clip terms by the
	// planning: rate is the playback rate (1 = normal, 0.5 = half speed --
	// length is output seconds either way), freeze holds the clip's first
	// frame for the whole length, and cam is the camera when an aspect or a
	// view/zoom is in play (nil = the frame as shot).
	rate   float64
	freeze bool
	cam    *camPath
	// the text effects that fall inside this clip, in its own output seconds
	// (textCues). Resolved in the planning beside the camera, because both
	// need the frame the clip comes out at.
	texts []textCue
	// the stop effects that fall inside this clip (freezeCues): each a frame
	// of a recording standing over the running footage, faded on and off.
	stills []stillCue
	// the volume effects that fall inside this clip (gainCues), in its own
	// output seconds. The only effect that reaches the sound and nothing else,
	// so it is the only one resolved here that the picture never sees.
	gains []textCue
	// the seconds of this clip nothing is heard over: a speed effect whose
	// sound is Silent (hushCues). A window rather than a flag on the clip,
	// because a stop and a ×1 do not cut the segment list at all -- their
	// bands lie inside a clip that is longer than they are.
	hushes []textCue

	// ---- the sound off the picture's clock (cut_fxsound.go) ----------------
	//
	// audOwn is the two answers that let the sound keep its own speed while
	// the picture runs off without it. The sound is then a second input of
	// the same recording, read from audAt at 1x, and the separate recordings
	// under it are re-based onto audSess the same way (laneOverlap) -- they
	// are the same moment and they travel together.
	audOwn  bool
	audPath string  // the recording the sound is read from
	audAt   float64 // ...and where in it, in that file's own seconds
	audSess float64 // the session second the sound starts at
	audRun  int     // which run of shifted clips this belongs to; 0 is none
	// audPitch is the other kind of answer: the sound goes with the picture,
	// but by tape speed rather than by time-stretching, so the pitch rides
	// the rate.
	audPitch bool
	// the splice where a shifted run closes is dipped rather than cut: the
	// tail of one clip fades down and the head of the next fades up. Nothing
	// can cross the join between two clips -- they are separate encodes,
	// concatenated with a stream copy -- so a crossfade is not available and
	// two half-fades are.
	audIn  bool
	audOut bool
	// which SCENE of the cut this clip was cut from (cutSeg.Scene): the
	// question "is the sound still in the scene it came adrift in".
	scene int
}

// stillCue is one stop effect inside one clip: a textCue's window and fades --
// s to e in the clip's own output seconds -- plus where the frame itself comes
// from, the recording and the second in it. Composited before the camera
// (encodeClip), so a zoom or a view crops the still exactly as it crops the
// footage running under it.
type stillCue struct {
	s, e      float64
	fin, fout float64
	path      string
	at        float64
	// the stop asked for its seconds to be silent (cutFx.Mute) rather than
	// letting the footage's sound run on under the held frame.
	mute bool
	// the size the held frame has to come out at, when that is not the size it
	// already is. A stop's bar can hang across a cut into a clip cut from
	// ANOTHER recording, and the frame it holds is still the one the stop
	// started on -- so a 1280x720 webcam frame can end up laid over 3840x2160
	// gameplay. The overlay has no size of its own and no scaling in it, so
	// that frame would sit in the top-left corner at a third of the size. Left
	// at nought when the two agree, which is every single-camera session.
	w, h int
}

// bdrop fills the parts of the frame the picture does not reach with a
// blown-up, blurred copy of itself (or black when bare). fit scales the
// picture into the frame (inserts); otherwise it goes on at its own size at
// x,y.
type bdrop struct {
	w, h int
	x, y int
	fit  bool
	bare bool
}

func (b bdrop) on() bool { return b.w > 0 && b.h > 0 }

// minClipLn is the shortest clip the render will make. Under it there are not
// enough frames to encode and splice, and the piece is dropped.
//
// Everything upstream that decides how long a clip will COME OUT measures
// itself against this number rather than one of its own -- the speed clamp and
// the ramp stairs both do -- because a floor here and a different floor there
// is how footage goes missing between the timeline and the video.
const minClipLn = 0.5

// blurSigma is how far the backdrop is smeared, as a fraction of the finished
// frame's height. Enough that no detail survives to be read as a second
// picture, not so much that a bright scene turns into one flat colour.
const blurSigma = 0.02

// chain is the sub-graph that lays the picture on a blurred blow-up of itself:
// one split, the copy scaled to COVER the frame and blurred, the picture laid
// back over it. in is the label it reads, k a number that keeps its own labels
// apart from every other one in the graph; it returns the filter_complex text
// and the label it wrote.
func (b bdrop) chain(in string, k int) (string, string) {
	out := fmt.Sprintf("bd%d", k)
	if b.bare {
		// black, and one filter to say it: pad puts the picture on a bigger
		// frame at the corner it belongs at, and the fitted case scales it
		// down first because an insert brings its own shape
		if b.fit {
			return fmt.Sprintf("[%s]scale=%d:%d:force_original_aspect_ratio=decrease,"+
				"pad=%d:%d:(ow-iw)/2:(oh-ih)/2:color=black[%s];",
				in, b.w, b.h, b.w, b.h, out), out
		}
		return fmt.Sprintf("[%s]pad=%d:%d:%d:%d:color=black[%s];",
			in, b.w, b.h, b.x, b.y, out), out
	}
	sig := math.Max(4, float64(b.h)*blurSigma)
	fg := fmt.Sprintf("bdf%d", k)
	bg := fmt.Sprintf("bdb%d", k)
	fc := fmt.Sprintf("[%s]split=2[%s][%s];", in, bg, fg)
	// increase then crop is "cover": the copy is blown up until it is at
	// least the frame in both directions, and the excess is cut off centred
	fc += fmt.Sprintf("[%s]scale=%d:%d:force_original_aspect_ratio=increase,"+
		"crop=%d:%d,gblur=sigma=%.2f,setsar=1[%s];", bg, b.w, b.h, b.w, b.h, sig, bg)
	if b.fit {
		fc += fmt.Sprintf("[%s]scale=%d:%d:force_original_aspect_ratio=decrease[%s];",
			fg, b.w, b.h, fg)
		fc += fmt.Sprintf("[%s][%s]overlay=(W-w)/2:(H-h)/2[%s];", bg, fg, out)
		return fc, out
	}
	fc += fmt.Sprintf("[%s][%s]overlay=%d:%d[%s];", bg, fg, b.x, b.y, out)
	return fc, out
}

// clipSize is what one encoded clip actually came out at.
type clipSize struct {
	name string
	w, h int
}

// joinMismatch is what to say about clips that will break the join: the concat
// demuxer stream-copies and does not refuse a clip of another size -- the
// decoder comes apart on it and the video smears from there. Every branch of
// encodeClip pins its size, so a mismatch means one stopped. Measured against
// the FIRST clip, reported by name.
func joinMismatch(made []clipSize) []string {
	var out []string
	for i, c := range made {
		if i == 0 || (c.w == made[0].w && c.h == made[0].h) {
			continue
		}
		out = append(out, fmt.Sprintf(
			"%s came out %d×%d where %s is %d×%d — the join is a stream copy and cannot mix sizes, so the video breaks at this clip",
			c.name, c.w, c.h, made[0].name, made[0].w, made[0].h))
	}
	return out
}

// fitsFrame is whether this recording, scaled down until it fits the finished
// frame, then covers it -- so that the edges need nothing filling in. Asked of
// the source size the Prepare page probed; a recording nobody measured answers
// no, because a wrong yes here is footage pulled out of shape.
func fitsFrame(v *tlVideo, w, h int) bool {
	if v == nil || v.w <= 0 || v.h <= 0 || w <= 0 || h <= 0 {
		return false
	}
	// what force_original_aspect_ratio=decrease would come out at: the smaller
	// of the two scales, so the picture never overshoots the frame and the
	// only question left is what it falls SHORT of. Within a pixel on both
	// axes is covered -- 1366x768 into 1920x1080 is not 16:9 to the last
	// decimal, and a one-pixel seam of blur is not worth a filter chain
	k := math.Min(float64(w)/float64(v.w), float64(h)/float64(v.h))
	return float64(w)-float64(v.w)*k < 2 && float64(h)-float64(v.h)*k < 2
}

// stillSize is the size a held frame has to be brought to before it is laid
// over a clip, or nought when it needs no bringing. Two recordings and the
// question is only ever asked of a pair that a stop's bar reaches across; a
// session shot on one camera answers nought here every time, and so does one
// whose sizes were never probed -- a made-up size would be worse than none.
func stillSize(from, over *tlVideo) (int, int) {
	if from == nil || over == nil {
		return 0, 0
	}
	if from.w <= 0 || from.h <= 0 || over.w <= 0 || over.h <= 0 {
		return 0, 0
	}
	if from.w == over.w && from.h == over.h {
		return 0, 0
	}
	return over.w, over.h
}

// stillMute is the ffmpeg enable expression for the seconds this clip's stops
// asked to have taken out of its sound: one between() per still, added
// together the way ffmpeg spells "or". Empty when every stop keeps its sound,
// which is the ordinary case and leaves the graph exactly as it was.
func stillMute(stills []stillCue) string {
	var parts []string
	for _, sc := range stills {
		if sc.mute && sc.e > sc.s {
			parts = append(parts, fmt.Sprintf("between(t,%.3f,%.3f)", sc.s, sc.e))
		}
	}
	return strings.Join(parts, "+")
}

// prodMix is a separate recording playing under one clip: a headset recorder, a
// mic on the table, OBS's second track. The picture's own sound is the game and
// whatever the capture card happened to hear; this is the rest of what was said
// while it was recorded, and it is mixed into the same bed rather than over the
// narration -- so "Game audio under voice" ducks both together and the AI voice
// still sits on top of everything that was actually there.
//
// The three numbers are the whole placement: at is where in the clip it comes
// in (0 unless the recorder started later than the clip did), ss is where in
// the recording the clip starts, and dur is how much of it is used. They come
// out of the one session clock the lanes are drawn on, which is why a recording
// that ran for the second half of a clip is heard for the second half of it.
type prodMix struct {
	base string
	path string
	at   float64
	ss   float64
	dur  float64
	// which audio stream of that file, a:N. Nought for a recording and for a
	// sound laid in by hand; above nought for a further track of a multi-track
	// capture, where the same file is in the mix on more than one lane and this
	// is the only thing that says which (cut_tracks.go).
	track int
}

// speed is the clip's playback rate with the zero value meaning normal: the
// planning always writes one, but a prodClip is also built by hand in tests
// and the arithmetic below divides by this.
func (c prodClip) speed() float64 {
	if c.rate > 0 {
		return c.rate
	}
	return 1
}

// name is what a clip is called in the log and in the clips folder.
func (c prodClip) name() string {
	if c.ins != "" {
		return insBase(c.ins)
	}
	return c.video.base
}

// produceClicked is ▶: the video, the upload text and the thumbnail.
// transcodeClicked is the ↻ on the Transcode heading: the video alone. Both
// ask before overwriting a file (minutes of encoding, no undo); ▶ asks nothing
// when the video is already what the page describes (renderStale).
func (a *App) produceClicked() {
	if !a.renderWanted() {
		a.produceRun(true)
		return
	}
	a.askOverwrite(func() { a.produceRun(true) })
}

func (a *App) transcodeClicked() { a.askOverwrite(func() { a.produceRun(false) }) }

// renderWanted is renderStale with the page's own inputs read for it, for the
// two callers that have not gathered them yet.
func (a *App) renderWanted() bool {
	if a.prod == nil {
		return false
	}
	vids, auds := a.snapSources()
	return a.renderStale(a.produceSegs(), a.produceEntries(), a.prodSettings(), vids, auds)
}

func (a *App) askOverwrite(run func()) {
	p := a.prod
	if p == nil || !exists(p.outFile) {
		run()
		return
	}
	fi, err := os.Stat(p.outFile)
	detail := p.outFile
	if err == nil {
		detail = fmt.Sprintf("%s — %s, %s", p.outFile, humanSize(fi.Size()), humanAgo(fi.ModTime()))
	}
	a.confirm("Overwrite "+filepath.Base(p.outFile)+"?",
		detail+"\n\nThe encode takes minutes and there is no undo for it.", "Overwrite", run)
}

func (a *App) produceRun(words bool) {
	if a.busy() {
		return
	}
	if a.pub == nil {
		return
	}
	segs := a.produceSegs()
	if len(segs) == 0 {
		a.setStatus("no cut yet — build one on the Cut step first")
		return
	}
	entries := a.produceEntries()
	st := a.prodSettings()
	vids, auds := a.snapSources()
	// the drawing half's inputs, read on this thread like everything above:
	// the goroutine below must not go reading widgets or the editor
	pst := a.pub.snapshot()
	aspect := a.produceCut().Aspect
	written := a.publishRecorded()
	a.saveProjectNow() // the run is a moment worth a file, whatever the ticker is doing

	a.startRun()
	// ▶ leaves an up-to-date video alone; ↻ Transcode is the press that means
	// "encode it anyway", so it never asks this question.
	encode := !words || a.renderStale(segs, entries, st, vids, auds)
	if !encode {
		a.logf(">>> the video is already what this page describes — not encoding it again " +
			"(↻ beside Transcode encodes anyway)")
	}
	switch {
	case !encode:
	case !words:
		a.logf(">>> transcoding %s: %d clips at %s/%s crf %d — the thumbnail and the upload text are left as they are",
			filepath.Base(st.OutFile), len(segs), st.Container, st.Codec, st.CRF)
	case written:
		a.logf(">>> producing %s: %d clips at %s/%s crf %d, and the thumbnail redrawn beside them",
			filepath.Base(st.OutFile), len(segs), st.Container, st.Codec, st.CRF)
	default:
		a.logf(">>> producing %s: %d clips at %s/%s crf %d, and the upload text and thumbnail written beside them",
			filepath.Base(st.OutFile), len(segs), st.Container, st.Codec, st.CRF)
	}
	// two halves, side by side. The render owns the fraction -- it is minutes
	// where the other is seconds, so the needle IS the render's progress --
	// and the words-and-picture half owns the second line of the bar, which
	// is what it has to say for itself.
	a.qJob(trackSTT, "render", 0, 0)
	a.prog(trackSTT, 0, "preparing")
	if words {
		a.qJob(trackFrames, "publish", 0, 0)
		a.prog(trackFrames, 0, "thinking")
	}
	a.pulseUntilCounted()

	go func() {
		// The two halves do not touch. The upload text and the thumbnail are
		// written from the cut and the narration and read their frames out of
		// inputs/; the render reads the same cut and narration and writes
		// produce/ and the video. Neither reads a file the other writes, and
		// the render never reads the publish record -- so there was nothing
		// for the render to wait for, and it waited anyway: a minute of an
		// idle encoder while a model thought about a title, and then the
		// thumbnail drawing on the GPU while the CPU had nothing to do.
		//
		// A failure in the words does not throw the render away any more.
		// They are separate deliverables, the expensive one is the video, and
		// an sd.cpp that is down is not a reason to spend the encode again.
		var wg sync.WaitGroup
		var pubErr error
		if words {
			wg.Add(1)
			go func() {
				defer wg.Done()
				pubErr = a.publishStage(trackFrames, pst, aspect, segs, entries, !written, written, false, false)
				a.qDone(trackFrames, 0) // its own line goes quiet; the needle was never its
				glib.IdleAdd(func() {
					if p := a.pub; p != nil {
						p.refresh() // whatever landed, up as soon as it is written
					}
				})
				if pubErr != nil && !errors.Is(pubErr, errStopped) {
					a.logfIdle("!!! the upload text and thumbnail failed: %v -- the render carries on", pubErr)
				}
			}()
		}
		var err error
		if encode {
			if err = a.produce(segs, entries, st, vids, auds); err == nil {
				a.markRendered(segs, entries, st, vids, auds)
			}
		}
		wg.Wait()
		// the render's word is the run's: it is what the press was for, and a
		// title that did not get written is a line in the log, not a failure
		// to show for a video that rendered
		if err == nil && pubErr != nil {
			err = pubErr
		}
		glib.IdleAdd(func() {
			a.endRun()
			a.updateGates()
			if err != nil {
				if !errors.Is(err, errStopped) {
					a.logf("produce FAILED: %v", err)
				}
				if errors.Is(err, errStopped) {
					a.setStatus("production stopped")
				} else {
					a.setStatus("production failed — see log")
				}
				return
			}
			a.progress.SetFraction(1)
			a.setStatus("done")
			// the bar carries the outcome; filled in below, once the file has
			// been measured
			dur, _ := ffprobeDur(st.OutFile)
			fi, _ := os.Stat(st.OutFile)
			size := int64(0)
			if fi != nil {
				size = fi.Size()
			}
			a.logf(">>> %s  (%.1f s, %s)", st.OutFile, dur, humanSize(size))
			a.progress.SetText(fmt.Sprintf("produced %s — %.1f s, %s",
				filepath.Base(st.OutFile), dur, humanSize(size)))
			a.updateProduceInfo()
		})
	}()
}

func (a *App) produce(segs []cutSeg, entries []narrEntry, st prodSettings, srcVid, srcAud []string) error {
	vids, recs, err := a.sessionTracks(srcVid, srcAud)
	if err != nil {
		return err
	}
	dir := a.produceDir()
	clipDir := filepath.Join(dir, "clips")
	if err := os.RemoveAll(clipDir); err != nil {
		return err
	}
	if err := os.MkdirAll(clipDir, 0o755); err != nil {
		return err
	}

	// 1. speak whatever is still missing, so a render never silently drops a line
	// -- unless nothing is ever spoken: captions only renders every line as a
	// caption with no wav, which the planning below already knows how to lay out
	var todo []narrEntry
	if !a.captionsOnly() {
		for _, e := range entries {
			if strings.TrimSpace(e.Text) != "" && !exists(a.ttsWav(e)) {
				todo = append(todo, e)
			}
		}
	} else if len(entries) > 0 && st.Subs == "none" {
		a.logfIdle("!!! captions only and subtitles set to none — the lines appear nowhere")
	}
	if len(todo) > 0 {
		// a render that has to speak first is two jobs, and the bar says so;
		// when everything is already spoken there is only ever the one
		a.qJob(trackSTT, "speaking", 1, 2)
		a.qPush(trackSTT, len(todo), "line")
	}
	for i, e := range todo {
		if err := a.checkpoint(); err != nil {
			return err
		}
		a.qTake(trackSTT)
		a.prog(trackSTT, 0.2*float64(i)/float64(len(todo)), "")
		if err := a.synthesize(e); err != nil {
			return fmt.Errorf("synthesis for %.0fs: %w", e.S, err)
		}
	}
	base := 0.0
	if len(todo) > 0 {
		base = 0.2
		a.qJob(trackSTT, "render", 2, 2)
		a.prog(trackSTT, base, "planning the clips")
	}

	// 2. plan every clip: which recording, how long, which voice file, and
	// which lanes the scene was told not to hear (cutSeg.Quiet, cut_hear.go)
	var clips []prodClip
	for i, s := range segs {
		var c prodClip
		switch {
		case s.isCopy():
			// a copied stretch: not a file to stretch into a slot but footage
			// to cut from its recording, exactly as the segment it was copied
			// from would be cut
			from, _ := copySrc(s.Ins)
			v := pickVideoOn(vids, s.Cam, from)
			if v == nil {
				a.logfIdle("clip %d copies footage at %.0f s that falls in no recording — skipped", i+1, from)
				continue
			}
			c = copyClip(i, s, from, v)
			if end := v.start + v.dur; from+c.length > end {
				c.length = end - from
				a.logfIdle("clip %d copies past the end of %s — shortened to %.1f s", i+1, v.base, c.length)
			}
		case s.audioIns():
			// sound alone: the picture is the session's own -- held on one
			// frame when the file is spliced in, kept running when it covers a
			// selection -- and the file replaces the session's sound for the
			// slot (encodeClip routes snd where the capture's sound was)
			file, _ := insSplit(s.Ins)
			path := a.loadPath(file)
			if !exists(path) {
				a.logfIdle("clip %d: %s is not there any more — skipped", i+1, file)
				continue
			}
			v := pickVideoOn(vids, s.Cam, s.S)
			if v == nil {
				a.logfIdle("clip %d at %.0f s falls in no recording — its sound has no picture, skipped", i+1, s.S)
				continue
			}
			c = sndClip(i, s, path, v, recs)
			if end := v.start + v.dur; !c.freeze && s.E > end {
				c.length = end - s.S
				a.logfIdle("clip %d runs past the end of %s — shortened to %.1f s", i+1, v.base, c.length)
			}
		case s.isInsert():
			// an insert is its own picture, so there is nothing to look up and
			// nothing to run past the end of: the slot is exactly as long as the
			// cut says, and assetClip stretches or trims the file to fill it
			// the file is resolved and the card's parameters ride along with it:
			// what is on disk is a picture the parameters are applied TO, and
			// only the part before the "?" is a path at all
			file, q := insSplit(s.Ins)
			path := a.loadPath(file)
			if !exists(path) {
				a.logfIdle("clip %d: %s is not there any more — skipped", i+1, file)
				continue
			}
			var note string
			c, note = insClip(i, s, path+q.suffix(), vids)
			if note != "" {
				a.logfIdle("clip %d %s", i+1, note)
			}
		default:
			v := pickVideoOn(vids, s.Cam, s.S)
			if v == nil {
				a.logfIdle("clip %d at %.0f s falls in no recording — skipped", i+1, s.S)
				continue
			}
			c = prodClip{idx: i, video: v, local: v.at(s.S), tempo: 1, rate: 1,
				length: s.length()}
			if s.Rate > 0 {
				c.rate = s.Rate
			}
			// the clamp is in session seconds -- what the recording has left --
			// and the slot in output seconds, which under slow motion is longer
			if end := v.start + v.dur; s.E > end {
				c.length = (end - s.S) / c.speed()
				a.logfIdle("clip %d runs past the end of %s — shortened to %.1f s", i+1, v.base, c.length)
			}
		}
		if c.length < minClipLn {
			// under half a second there is nothing to encode, but a piece
			// vanishing out of the middle of the cut in silence reads as a
			// render bug. It is nearly always an effect boundary landing just
			// short of a cut, which is a thing the hand can move.
			a.logfIdle("clip %d at %s is %.2f s — too short to render, dropped%s",
				i+1, mmss(s.S), c.length, spokenHere(entries, s))
			continue
		}
		c.sessS = s.S
		c.scene = s.Scene // which scene of the cut this piece is part of
		c.quiet = s.Quiet // every clip kind, so one scene answers one way
		if from, ok := copySrc(s.Ins); ok {
			// its text cues are the copied footage's, not the paste point's:
			// a name plate over the original belongs over the copy too
			c.sessS = from
		}
		matched := matchEntries(entries, s)
		for _, e := range matched {
			text := strings.TrimSpace(e.Text)
			if text == "" {
				continue
			}
			// at is an offset into the clip the line was WRITTEN against, and
			// matchEntries deliberately accepts a clip that has moved since --
			// so it has to be re-based onto the clip actually being rendered. A
			// border dragged 20 s to the left and the line kept its old offset,
			// which is 20 s early: near the head of the clip, over whatever the
			// line before it was saying. Identical arithmetic to Narrate's
			// refit, and a no-op for a clip that has not moved.
			at := (e.S + e.At - s.S) / c.speed() // under slow motion the moment lands later in the clip
			ln := prodLine{text: text, pos: e.Pos,
				at: math.Min(math.Max(0, at), math.Max(0, c.length-1))}
			ln.delay = math.Max(narrLead, ln.at)
			wav := a.ttsWav(*e)
			if !exists(wav) {
				if !a.captionsOnly() { // with no voice chosen, this is every line
					a.logfIdle("clip %d: no synthesis for a line — it is captioned only", i+1)
				}
			} else {
				ln.wav = wav
				ln.dur, _ = ffprobeDur(wav)
			}
			c.lines = append(c.lines, ln)
		}
		if len(c.lines) == 0 && len(entries) > 0 && len(matched) == 0 && s.E > s.S {
			a.logfIdle("clip %d at %.0f s has no narration entry — it keeps its own audio", i+1, s.S)
		}
		// make the lines fit where the writer put them: each starts no earlier
		// than the one before it ended, the slot grows first (the placement
		// survives whole -- a sign-off at the end stays at the end), then the
		// schedule slides earlier as one piece, and only as the last resort
		// are the lines distorted
		if len(c.lines) > 0 {
			pack := func() float64 { return narrRun(c.lines, c.tempo) }
			need := pack()
			if need > c.length {
				c.length += math.Min(need-c.length, maxExtend)
				// footage runs out; an insert does not -- a still or a loop is
				// as long as the slot asks for
				if v := c.video; v != nil {
					if end := v.start + v.dur; s.S+c.length*c.speed() > end {
						c.length = (end - s.S) / c.speed()
					}
				}
			}
			if over := need - c.length; over > 0 {
				shift := math.Min(over, c.lines[0].delay-narrLead)
				for k := range c.lines {
					c.lines[k].delay -= shift
				}
				need -= shift
				a.logfIdle("clip %d: the narration does not fit where it was placed — moved %.1f s earlier",
					i+1, shift)
			}
			if need > c.length {
				var talk float64
				for _, ln := range c.lines {
					talk += ln.dur
				}
				avail := c.length - narrLead - 0.2 - 0.3*float64(len(c.lines)-1)
				c.tempo = math.Min(talk/math.Max(0.1, avail), maxTempo)
				// repack with the sped-up lines, then slide once more: the
				// placements are re-derived from at, so the schedule is the
				// writer's shape at the new speed
				need = pack()
				if over := need - c.length; over > 0 {
					shift := math.Min(over, c.lines[0].delay-narrLead)
					for k := range c.lines {
						c.lines[k].delay -= shift
					}
				}
				a.logfIdle("clip %d: narration %.1f s does not fit %.1f s — sped up %.2fx",
					i+1, talk, c.length, c.tempo)
			}
		}
		clips = append(clips, c)
	}
	if len(clips) == 0 {
		return fmt.Errorf("no clip could be placed on a recording")
	}
	// what the sound does, before the recordings are placed: a clip whose
	// sound has come away from the picture hears the lanes from the same
	// second the capture is read from, and clipMixes below asks it
	// (laneOverlap). After the lengths are settled, because the sound
	// advances by however long the clip is on SCREEN -- a slot grown for its
	// narration is more seconds of sound too.
	audioPlan(clips, a.produceCut().Fx)
	// the separate recordings, now that every clip's length is settled: growing
	// a slot for its narration lengthens what was heard under it too
	for i := range clips {
		clips[i].mix = clipMixes(clips[i], recs)
	}
	if len(recs) > 0 {
		for _, line := range laneReport(clips, recs) {
			a.logfIdle("%s", line)
		}
	}
	// every insert is fitted into the size the footage comes out at, which can
	// only be known once it is settled which recordings are in (clipBox) --
	// and when the cut names an aspect, that size is the aspect's own frame
	// instead, for footage and inserts alike (outBox)
	cf := a.produceCut()
	fx, aspect := cf.Fx, cf.Aspect
	boxW, boxH := outBox(clips, st, aspect)
	if boxW > 0 {
		// on every clip, not only the inserts that are fitted into it: a text
		// overlay is drawn at the finished frame's size too, and it has to be
		// the same size for footage as for a card or the words land elsewhere
		for i := range clips {
			clips[i].boxW, clips[i].boxH = boxW, boxH
		}
	}
	// the camera: with an aspect, a view or a zoom in play, every footage clip
	// is taken through fxRectAt -- the same function the preview overlay draws
	// -- sampled at every moment anything starts or stops moving (buildCam)
	if fxHasCamera(aspect, fx) && boxW > 0 {
		camFps := st.FPS
		if camFps <= 0 {
			camFps = 30
		}
		for i := range clips {
			c := &clips[i]
			if c.video == nil {
				continue
			}
			sw, sh, err := ffprobeSize(c.video.path)
			if err != nil {
				return fmt.Errorf("%s: the camera needs the source frame size: %w", c.video.base, err)
			}
			span := c.length * c.speed()
			if c.freeze {
				span = 0 // a freeze is one session moment, not a stretch of it
			}
			c.cam = buildCam(fx, aspect, sw, sh, c.video.sessionAt(c.local), span,
				c.speed(), c.length, boxW, boxH, camFps)
			if c.cam != nil && c.cam.maxZoom() > 10 {
				a.logfIdle("clip %d: the zoom goes deeper than ffmpeg's 10× — it is rendered at 10×", i+1)
			}
		}
	}

	// the words: the same session-to-clip mapping the camera just used, so a
	// title and a camera move placed at the same second happen at the same
	// second in the render. An insert or a freeze is one session moment held
	// for a while (span 0), exactly as buildCam treats it.
	if boxW > 0 {
		for i := range clips {
			c := &clips[i]
			span := c.length * c.speed()
			if c.freeze || c.ins != "" {
				span = 0
			}
			c.texts = textCues(fx, c.sessS, span, c.speed(), c.length)
		}
	}

	// the volume effects: the same mapping a third time, and outside the boxW
	// guard above because a gain needs no frame size -- a clip with no picture
	// to put a title on still has sound to turn up.
	for i := range clips {
		c := &clips[i]
		span := c.length * c.speed()
		if c.freeze || c.ins != "" {
			span = 0
		}
		c.gains = gainCues(fx, c.sessS, span, c.speed(), c.length)
	}

	// the seconds nothing is heard over: a speed effect whose sound is Silent,
	// at any rate. Mapped exactly as the volume effects above are, and for the
	// same reason -- a stop and a ×1 leave the segment list alone, so their
	// bands lie inside a clip rather than being one.
	for i := range clips {
		c := &clips[i]
		span := c.length * c.speed()
		if c.freeze || c.ins != "" {
			span = 0
		}
		c.hushes = hushCues(fx, c.sessS, span, c.speed(), c.length)
	}

	// the stop effects: the same mapping once more, but the overlay is a frame
	// of a recording rather than a drawn card, resolved here where the
	// recordings are known -- a stop's bar can hang into a clip cut from a
	// different recording than the one its frame is in
	for i := range clips {
		c := &clips[i]
		if c.video == nil || c.ins != "" || c.freeze {
			continue // no footage runs under a card or a held frame
		}
		for _, cue := range freezeCues(fx, c.sessS, c.length*c.speed(), c.speed(), c.length) {
			// the scene's own camera (cutVideoOn), not whichever recording
			// happened to be rolling: on a session shot on two cameras,
			// pickVideo froze camera 1's frame over a scene showing camera 2 --
			// a still of something the clip around it never shows
			v := cutVideoOn(segs, vids, cue.fx.T)
			if v == nil {
				a.logfIdle("a stop at %.0f s falls in no recording — its still is skipped", cue.fx.T)
				continue
			}
			sc := stillCue{s: cue.s, e: cue.e, fin: cue.fin,
				fout: cue.fout, path: v.path, at: v.at(cue.fx.T),
				mute: cue.fx.sound() == sndMute}
			sc.w, sc.h = stillSize(v, c.video)
			c.stills = append(c.stills, sc)
		}
	}

	// 3. subtitles, on the produced timeline: the narration's lines where
	// there is a narration, and what was said in the clips where there is
	// not (captionLines)
	cum := 0.0
	var cues []subCue
	if st.Subs != "none" {
		vids, auds := a.snapSources()
		transcriptSubs(clips, a.sessionWords(append(vids, auds...)), a.narratorMic())
	}
	for _, c := range clips {
		caps := captionLines(c)
		for k, ln := range caps {
			end := cum + ln.delay + ln.dur/c.tempo
			if ln.dur == 0 { // unspoken: hold the caption until the next line, or the clip's end
				end = cum + c.length
				if k+1 < len(caps) {
					end = cum + caps[k+1].delay
				}
			}
			cues = append(cues, subCue{s: cum + ln.delay, e: math.Min(end, cum+c.length), text: subText(ln)})
		}
		cum += c.length
	}
	cues = tidyCues(cues)
	srt := srtText(cues)
	cue := len(cues)
	// under clips/, with the scratch: beside the video and named for it, a
	// player picks it up as a second subtitle track of its own accord --
	// VLC did, next to the one muxed in -- and the sidecar choice below is
	// where a file beside the video is asked for
	srtPath := filepath.Join(clipDir, "final.srt")
	// ...and the one an older build left beside the video is taken away, or
	// it stays a second track forever: the sidecar choice writes its own
	// below, when it is chosen
	os.Remove(strings.TrimSuffix(st.OutFile, filepath.Ext(st.OutFile)) + ".srt")
	if err := os.WriteFile(srtPath, []byte(srt), 0o644); err != nil {
		return err
	}
	// ...and the same track in the other languages asked for (translate.go).
	// Only where there is somewhere to put them: burned into the picture there
	// is one picture, and none where there are no subtitles at all.
	tracks := []subTrack{{code: a.asrLanguage(), tag: "und", path: srtPath}}
	if cue > 0 && (st.Subs == "mux" || st.Subs == "sidecar") {
		tracks = a.subTracks(cues, clipDir, st.SubLangs)
	}

	// 4. encode each clip -- the only video encode in the whole pipeline
	ext := "." + st.Container
	var list strings.Builder
	var made []clipSize
	// the queue: one task per clip to encode, which is where nearly all of the
	// time goes and the only part of a render anyone counts. It is filled here
	// rather than at the top because what the clips ARE is worked out above.
	a.qPush(trackSTT, len(clips), "clip")
	for i, c := range clips {
		if err := a.checkpoint(); err != nil {
			return err
		}
		a.qTake(trackSTT)
		a.prog(trackSTT, base+(0.9-base)*float64(i)/float64(len(clips)), "")
		// numbered by their place in the cut, which is what the concat list and
		// the finished video are in, and then by the second they were shot: a
		// clip folder says where in the session every piece came from. An insert
		// was never shot, so it is named after the file instead.
		stem := fmt.Sprintf("c%03d_%s", i, safeStem(c.ins))
		if c.video != nil {
			stem = fmt.Sprintf("c%03d_%s", i, stampName(c.video.wall+c.local))
		}
		name := stem + ext
		var cueFile string
		caps := captionLines(c)
		if st.Subs == "burn" && len(caps) > 0 {
			cueFile = filepath.Join(clipDir, stem+".srt")
			one := ""
			for k, ln := range caps {
				end := ln.delay + ln.dur/c.tempo
				if ln.dur == 0 {
					end = c.length
					if k+1 < len(caps) {
						end = caps[k+1].delay
					}
				}
				one += fmt.Sprintf("%d\n%s --> %s\n%s\n\n", k+1, srtTime(ln.delay),
					srtTime(math.Min(end, c.length)), subText(ln))
			}
			if err := os.WriteFile(cueFile, []byte(one), 0o644); err != nil {
				return err
			}
		}
		if err := a.encodeClip(c, filepath.Join(clipDir, name), cueFile, st); err != nil {
			return fmt.Errorf("clip %d: %w", i+1, err)
		}
		if w, h, err := ffprobeSize(filepath.Join(clipDir, name)); err == nil {
			made = append(made, clipSize{name: name, w: w, h: h})
		}
		// bare name: concat-list entries resolve against the LIST's directory
		list.WriteString(concatLine(name))
	}
	for _, line := range joinMismatch(made) {
		a.logfIdle("%s", line)
	}
	lf := filepath.Join(clipDir, "concat.txt")
	if err := os.WriteFile(lf, []byte(list.String()), 0o644); err != nil {
		return err
	}

	// 5. join (stream copy) and normalize loudness over the whole thing, so
	//    clip boundaries do not pump
	if err := a.checkpoint(); err != nil {
		return err
	}
	a.prog(trackSTT, 0.92, "joining")
	joined := filepath.Join(clipDir, "joined"+ext)
	if err := a.runCmd(ffTool("ffmpeg"), "-v", "error", "-y", "-f", "concat", "-safe", "0",
		"-i", lf, "-c", "copy", joined); err != nil {
		return err
	}

	if err := a.checkpoint(); err != nil {
		return err
	}
	a.prog(trackSTT, 0.96, "loudness + mux")
	if err := os.MkdirAll(filepath.Dir(st.OutFile), 0o755); err != nil {
		return err
	}
	args := []string{"-v", "error", "-y", "-i", joined}
	muxSubs := st.Subs == "mux" && st.Container != "webm" && cue > 0
	if muxSubs {
		for _, t := range tracks {
			args = append(args, "-i", t.path)
		}
	}
	args = append(args, "-map", "0:v", "-map", "0:a", "-c:v", "copy",
		"-af", loudFlt, "-ar", "48000")
	args = append(args, audioArgs(st)...)
	if muxSubs {
		codec := "srt"
		if st.Container == "mp4" {
			codec = "mov_text"
		}
		for i, t := range tracks {
			// each its own input, each tagged with its own language, so a
			// player's Sub Track menu names them rather than listing two
			// "Track 1 - [English]"
			args = append(args, "-map", fmt.Sprintf("%d:0", i+1), "-c:s", codec,
				fmt.Sprintf("-metadata:s:s:%d", i), "language="+t.tag)
			if t.name != "" {
				args = append(args, fmt.Sprintf("-metadata:s:s:%d", i), "title="+t.name)
			}
		}
	}
	args = append(args, st.OutFile)
	if err := a.runCmd(ffTool("ffmpeg"), args...); err != nil {
		return err
	}
	if st.Subs != "none" && cue == 0 {
		a.logfIdle("!!! subtitles were asked for (%s) and there is nothing to write: no narration, and no speech in the clips", st.Subs)
	}

	switch {
	case st.Subs == "mux" && st.Container == "webm":
		a.logfIdle("webm cannot carry an srt track — subtitles written next to the video instead")
		fallthrough
	case st.Subs == "sidecar":
		stem := strings.TrimSuffix(st.OutFile, filepath.Ext(st.OutFile))
		for i, t := range tracks {
			// the first beside the video under its own name, the rest with
			// their language in it -- which is how every player finds them
			side := stem + ".srt"
			if i > 0 {
				side = stem + "." + t.code + ".srt"
			}
			b, err := os.ReadFile(t.path)
			if err != nil {
				return err
			}
			if err := os.WriteFile(side, b, 0o644); err != nil {
				return err
			}
			a.logfIdle(">>> subtitles: %s", side)
		}
	}
	return nil
}

// insKind says what an insert file is, which is the only thing that decides how
// ffmpeg has to be asked for it. By extension, deliberately: the alternative is
// probing every asset on every render, and an .svg that ffprobe calls a png
// stream is not more true than the name the user gave it.
func insKind(path string) string {
	file, _ := insSplit(path)
	switch strings.ToLower(filepath.Ext(file)) {
	case ".mp4", ".mkv", ".mov", ".webm", ".avi", ".m4v", ".mpg", ".mpeg", ".ts":
		return "video"
	case ".svg", ".svgz":
		return "svg"
	case ".mp3", ".wav", ".ogg", ".oga", ".flac", ".m4a", ".aac", ".opus":
		return "audio"
	default:
		return "still"
	}
}

// clipInput builds the ffmpeg input arguments for a clip's picture, and says
// whether that input carries sound. Three shapes come out of here: a stretch of
// a recording, a held still, and a baked SVG sequence.
func (a *App) clipInput(c prodClip, st prodSettings) ([]string, bool, error) {
	if c.ins == "" {
		if c.freeze {
			// one frame is all that is kept (trim=end_frame=1 in encodeClip),
			// so half a second of input is plenty -- and a held frame has no
			// sound of its own, so the anullsrc path supplies the silence
			return []string{
				"-ss", fmt.Sprintf("%.3f", math.Max(0, c.local)),
				"-t", "0.5", "-i", c.video.path,
			}, false, nil
		}
		// input seconds: a slowed clip reads rate·length of footage. Its own sound is
		// that input's FIRST track only ([0:a], encodeClip); a chosen second track
		// reaches the clip as a lane in the mix (ownTrack).
		return []string{
			"-ss", fmt.Sprintf("%.3f", math.Max(0, c.local)),
			"-t", fmt.Sprintf("%.3f", c.length*c.speed()), "-i", c.video.path,
		}, hasAudioStream(c.video.path) && !c.mute && !laneQuiet(c.quiet, c.video.base) &&
			ownTrack(a.snappedTracks(), c.video.path), nil
	}
	rate := st.FPS
	if rate <= 0 {
		rate = svgFPS
	}
	// everything from here on wants a file to open, and an insert may name one
	// with a card's parameters after it (see svgcards.go)
	file, params := insSplit(c.ins)
	switch insKind(c.ins) {
	case "video":
		// -t on the input, so a long file is trimmed before it is decoded; the
		// short case is tpad's and the output -t's, over in encodeClip
		return []string{"-t", fmt.Sprintf("%.3f", c.length), "-i", file},
			hasAudioStream(file) && !c.mute, nil

	case "svg":
		src, note, err := insSVG(c.ins)
		if err != nil {
			return nil, false, err
		}
		if note != "" {
			a.logfIdle("%s: %s", insBase(c.ins), note)
		}
		if !svgAnimated(src) {
			if svgHasCSSAnimation(src) {
				a.logfIdle("%s: a CSS animation with no @keyframes in the file — drawn as a still",
					insBase(c.ins))
			}
			// a static card with parameters is no longer the file on disk, so it
			// is written out for ffmpeg to open; without them the asset is opened
			// where it lies. Either way it is a still like any other from here.
			if len(params) > 0 {
				file = filepath.Join(a.produceDir(), "clips",
					fmt.Sprintf("c%03d_%s.svg", c.idx, safeStem(c.ins)))
				if err := os.MkdirAll(filepath.Dir(file), 0o755); err != nil {
					return nil, false, err
				}
				if err := os.WriteFile(file, src, 0o644); err != nil {
					return nil, false, err
				}
			}
			break
		}
		// baked beside the clips rather than next to the asset: this is derived
		// from the cut's slot length, so it belongs to the render and goes when
		// the render does
		dir := filepath.Join(a.produceDir(), "clips",
			fmt.Sprintf("c%03d_%s.frames", c.idx, safeStem(c.ins)))
		pat, n, err := bakeSVG(src, dir, rate, c.length)
		if err != nil {
			return nil, false, fmt.Errorf("animating %s: %w", insBase(c.ins), err)
		}
		a.logfIdle("%s: %d frames baked at %g fps", insBase(c.ins), n, rate)
		return []string{"-framerate", fmt.Sprintf("%g", rate), "-i", pat}, false, nil
	}
	// a still, and a static SVG that fell through to here: one frame held for
	// the whole slot. -framerate on the input rather than -r, so the held frames
	// are generated at the output's rate instead of one frame being stretched
	// into a stream nothing downstream can cut at.
	return []string{"-loop", "1", "-framerate", fmt.Sprintf("%g", rate),
		"-t", fmt.Sprintf("%.3f", c.length), "-i", file}, false, nil
}

// safeStem is a file's name reduced to what a clip folder can be named after:
// no separators, no spaces, no surprises for a shell that never sees it anyway.
func safeStem(path string) string {
	file, _ := insSplit(path) // a card's parameters are not part of its name
	base := strings.TrimSuffix(filepath.Base(file), filepath.Ext(file))
	var b strings.Builder
	for _, r := range base {
		switch {
		case r >= 'a' && r <= 'z', r >= 'A' && r <= 'Z', r >= '0' && r <= '9', r == '-', r == '_':
			b.WriteRune(r)
		default:
			b.WriteByte('-')
		}
	}
	s := strings.Trim(b.String(), "-")
	if s == "" {
		return "insert"
	}
	if len(s) > 40 {
		s = s[:40]
	}
	return s
}

// clipBox is the frame every insert has to fill: the size the footage clips
// come out at (the join is a stream copy), from the first recording used
// rather than the settings, which only name a height. A cut of nothing but
// inserts falls back to the output height at 16:9.
func clipBox(clips []prodClip, st prodSettings) (int, int) {
	for _, c := range clips {
		if c.video == nil {
			continue
		}
		w0, h0, err := ffprobeSize(c.video.path)
		if err != nil {
			continue
		}
		return outSize(w0, h0, st.Height)
	}
	return outSize(0, 0, st.Height)
}

// outSize is the frame the render comes out at, given the footage's own size
// and the tier that was asked for. The tier names the SHORT side of the frame:
// the height of a wide video -- which is all "1080p" has ever meant there --
// and the width of a tall one, where 1080p is 1080×1920, the size a Short is
// uploaded at, not a 608-wide strip. "original" (0) keeps the footage's own
// frame. Both sides rounded even -- an insert one pixel off is a clip a
// concat will not take.
//
// Its own function, over sizes that are already known, so the Produce page can
// say the frame in pixels without opening a file. "original" on a 4K screen
// capture reads like a setting and lands as a 700 MB upload, and the only
// moment that is worth knowing is before the encode, not after it.
func outSize(w0, h0, height int) (int, int) {
	if w0 <= 0 || h0 <= 0 {
		h := height
		if h <= 0 {
			h = 1080 // nothing probed and nothing chosen
		}
		return int(math.Round(float64(h)*16/9/2)) * 2, h - h%2
	}
	k := 1.0 // original: the footage's own frame
	if height > 0 {
		k = float64(height) / float64(min(w0, h0))
	}
	return int(math.Round(float64(w0)*k/2)) * 2, int(math.Round(float64(h0)*k/2)) * 2
}

// clipMixes is the stretch of every separate recording running while this
// clip was, in the clip's own time; a recording not running at all is absent.
// Inserts and freezes get none (added time, nothing was recorded under them).
// On a slowed clip the session span is the length through the rate, the
// placement is divided by it, and dur stays in file seconds (the stretch
// happens in the graph, atempo).
func clipMixes(c prodClip, recs []tlAudio) []prodMix {
	var out []prodMix
	if c.dropLane != "" && c.snd != "" {
		// the inserted sound goes in the mix where the recording it replaced
		// would have been, rather than in the capture's slot: that is the whole
		// difference between "these seconds sound like the file" and "this one
		// recording sounds like the file". at is 0 because a sound laid over
		// footage starts where the footage does, by construction.
		out = append(out, prodMix{base: baseName(c.snd), path: c.snd,
			at: 0, ss: c.sndAt, dur: c.length})
	}
	if len(recs) == 0 {
		return out
	}
	for _, au := range recs {
		if au.base == c.dropLane {
			continue // this one is what the inserted sound was put in place of
		}
		if laneQuiet(c.quiet, au.base) {
			continue // and this one the scene was told not to hear
		}
		t0, t1 := laneOverlap(c, au)
		if t1-t0 < laneMinMix { // nothing worth an input, and a 0 s one ffmpeg refuses
			continue
		}
		at := (t0 - c.sessS) / c.speed()
		if c.audOwn {
			at = t0 - c.audSess // 1x, from the second the sound starts at
		}
		out = append(out, prodMix{base: au.base, path: au.path, track: au.track,
			at: at, ss: t0 - au.start, dur: t1 - t0})
	}
	return out
}

// laneMinMix is the shortest overlap worth an ffmpeg input -- and a zero-length
// one it refuses outright.
const laneMinMix = 0.1

// laneReport says per recording whether it reached the render and, if not,
// which of the two reasons: not running while any clip was (a placement to
// look at), or silenced by the scenes (the cut doing what it was told). It
// reads clipMixes' decision rather than re-deciding.
func laneReport(clips []prodClip, recs []tlAudio) []string {
	var out []string
	for _, au := range recs {
		under, past := 0, 0
		for i := range clips {
			mixed := false
			for _, m := range clips[i].mix {
				mixed = mixed || m.base == au.base
			}
			// three ways for a clip to stand to a recording, and every clip
			// is exactly one of them. The overlap is read off the same
			// function the mix is built from, so the count and the mix
			// cannot disagree about which clips a track was running under.
			if t0, t1 := laneOverlap(clips[i], au); mixed {
				under++
			} else if t1-t0 >= laneMinMix {
				past++
			}
		}
		switch {
		case under > 0:
			out = append(out, fmt.Sprintf("%s is mixed into %d of the %d clips",
				au.base, under, len(clips)))
		case past > 0:
			out = append(out, fmt.Sprintf("%s runs under %d clip(s) and every one of them leaves it out — it is not in the render",
				au.base, past))
		default:
			out = append(out, fmt.Sprintf("%s was not running while any clip was — it is not in the render",
				au.base))
		}
	}
	return out
}

// laneOverlap is the session stretch a clip and a recording were both running
// in, empty when they were not; shared by the mix and the run's report so the
// two cannot disagree. A freeze and a card overlap nothing: they are time
// ADDED to the session.
func laneOverlap(c prodClip, au tlAudio) (float64, float64) {
	if c.freeze || c.noLanes {
		return 0, 0
	}
	s0 := c.sessS
	s1 := s0 + c.length*c.speed()
	if c.audOwn {
		// the sound is off the picture's clock: the lanes are the same moment
		// as the capture's own track and are read from the same second, at 1x
		// for exactly as long as the clip is on screen (cut_fxsound.go)
		s0 = c.audSess
		s1 = s0 + c.length
	}
	return math.Max(s0, au.start), math.Min(s1, au.start+au.dur)
}

// narrRun is the slot a clip's narration asks for: where the last line stops,
// plus the moment of air the render leaves after it. It is the number every
// decision about a clip's length is made against.
func narrRun(lines []prodLine, tempo float64) float64 {
	return packLines(lines, tempo) + narrTail
}

// packLines lays a clip's lines out in its slot and answers where the last one
// stops. Each starts where the writer placed it, but never before the line
// above has finished and a breath has passed -- so an overlong line pushes
// everything under it later, and it is the LAST line that falls off the end of
// the clip. The delays are written back into the slice, because that schedule
// is what the render feeds adelay.
//
// A function rather than the closure it used to be, because the Narrate page
// has to be able to ask the same question. Its own per-line ⚠ answers a
// different one -- "has THIS line room before the next" -- and cannot see the
// pushing, which is exactly what the render's "does not fit where it was
// placed" is complaining about (clipOverrun, narrate.go).
func packLines(lines []prodLine, tempo float64) float64 {
	if tempo <= 0 {
		tempo = 1 // a clip built by hand, and every arithmetic here divides
	}
	prev := 0.0
	for k := range lines {
		d := math.Max(narrLead, lines[k].at)
		if d < prev {
			d = prev
		}
		lines[k].delay = d
		prev = d + lines[k].dur/tempo + narrGap
	}
	return prev - narrGap
}

// delayMS is a line's start in the units adelay actually reads: whole
// milliseconds, never below the lead-in. Rounded rather than truncated, so a
// placement is out by at most half a millisecond either way.
func delayMS(delay float64) int {
	return int(math.Round(math.Max(narrLead, delay) * 1000))
}

// encodeClip cuts one slot out of its recording -- or renders an insert into
// one -- and mixes the narration in.
func (a *App) encodeClip(c prodClip, out, cueFile string, st prodSettings) error {
	src, srcSound, err := a.clipInput(c, st)
	if err != nil {
		return err
	}
	args := append([]string{"-v", "error", "-y"}, src...)
	game := "0:a"
	switch {
	case c.snd != "" && c.dropLane == "":
		// the inserted sound goes exactly where the silence would: input 1,
		// which keeps every index downstream where it was. The recording's own
		// sound is simply never mapped -- overwriting is replacing, not mixing.
		//
		// A copied stretch of a lane starts inside its file rather than at the
		// top of it, and -ss before -i is how that is asked: the trim is the
		// input's, so -t below still measures the slot and not the file.
		if c.sndAt > 0 {
			args = append(args, "-ss", fmt.Sprintf("%.3f", c.sndAt))
		}
		// the trim is in SOURCE seconds and the slot is in output ones, so a
		// clip played fast eats more of the file than it is long: without the
		// speed the sound would run out partway and apad would finish the slot
		// in silence. Slower than 1 takes less and the extra is simply
		// decoded and dropped, which costs nothing worth a branch.
		args = append(args, "-t", fmt.Sprintf("%.3f", c.length*math.Max(1, c.speed())),
			"-i", c.snd)
		game = "1:a"
	case c.audOwn && srcSound:
		// the sound has come away from the picture (cut_fxsound.go): it is a
		// second read of the same recording, from the second the plan says
		// and at 1×, for exactly as long as this clip is on screen. -ss
		// before -i, like every other seek here: the trim is the input's, so
		// this decodes the seconds it uses rather than the file.
		args = append(args, "-ss", fmt.Sprintf("%.3f", math.Max(0, c.audAt)),
			"-t", fmt.Sprintf("%.3f", c.length), "-i", c.audPath)
		game = "1:a"
	case !srcSound:
		args = append(args, "-f", "lavfi", "-t", fmt.Sprintf("%.3f", c.length),
			"-i", "anullsrc=channel_layout="+audLayout(st)+":sample_rate=48000")
		game = "1:a"
	}
	// every spoken line is its own input, mixed over the ducked game together
	var spoken []prodLine
	for _, ln := range c.lines {
		if ln.wav != "" {
			spoken = append(spoken, ln)
			args = append(args, "-i", ln.wav)
		}
	}
	voiceBase := 1
	if game == "1:a" {
		voiceBase = 2
	}
	// the separate recordings last, so adding one cannot move a voice input out
	// from under the index the narration filters were written against. Seeked
	// and trimmed on the input rather than in the graph: this is one stretch of
	// a file that may be an hour long, and decoding all of it to throw away the
	// rest is the difference between a render and an afternoon.
	mixBase := voiceBase + len(spoken)
	for _, m := range c.mix {
		args = append(args, "-ss", fmt.Sprintf("%.3f", math.Max(0, m.ss)),
			"-t", fmt.Sprintf("%.3f", m.dur), "-i", m.path)
	}
	// the overlays, last, for the same reason the recordings are last: a new
	// one must not move an index an earlier filter was written against. A
	// title is a still SVG the size of the finished frame, written here
	// (fxtext.go); a drawing is the user's own file (fxsvg.go). Either way an
	// SVG looped for the clip's length -- ffmpeg reads SVG already, it is how
	// the cards get in.
	txtBase := mixBase + len(c.mix)
	svgFps := st.FPS
	if svgFps <= 0 {
		svgFps = 30
	}
	for _, cue := range c.texts {
		file := cue.fx.Src
		if cue.fx.Kind != "svg" {
			file = fmt.Sprintf("%s_t%02d.svg", strings.TrimSuffix(out, filepath.Ext(out)), cue.idx)
			if err := os.WriteFile(file, textSVG(cue.fx, c.boxW, c.boxH), 0o644); err != nil {
				return err
			}
		}
		// a hair longer than the slot, so the last frame of the clip still has
		// something to composite; the overlay's eof_action=pass covers the rest
		args = append(args, "-loop", "1", "-framerate", fmt.Sprintf("%g", svgFps))
		if cue.fx.Kind == "svg" {
			// a vector rendered at the size it is used at rather than at the
			// size its document happens to declare (fxsvg.go); a title's file
			// is already written the frame's size and wants none of this
			if _, _, bw, bh, ok := svgFitPx(cue.fx, c.boxW, c.boxH); ok {
				args = append(args, "-width", strconv.Itoa(bw), "-height", strconv.Itoa(bh),
					"-keep_ar", "1")
			}
		}
		args = append(args, "-t", fmt.Sprintf("%.3f", c.length+0.2), "-i", file)
	}
	// the stop stills, after everything else for the same index-stability
	// reason: each is one decoded frame of its recording, cut at the stop's own
	// second. The half-second input window is all trim=end_frame=1 below needs,
	// and it spares decoding the hour of recording behind it.
	stillBase := txtBase + len(c.texts)
	for _, sc := range c.stills {
		args = append(args, "-ss", fmt.Sprintf("%.3f", sc.at), "-t", "0.5", "-i", sc.path)
	}

	var vf []string
	// the clock first: setpts stretches or squeezes the timestamps, and
	// everything after it -- the fps grid, the camera -- works in that time,
	// which is the clip's own output time
	if c.ins == "" && !c.freeze && c.speed() != 1 {
		vf = append(vf, fmt.Sprintf("setpts=PTS/%g", c.speed()))
	}
	if c.freeze {
		// one decoded frame, held: everything after the first frame is cut and
		// the frame is cloned out a hair past the slot (the output -t below is
		// what makes the length exact)
		vf = append(vf, "trim=end_frame=1,setpts=PTS-STARTPTS",
			fmt.Sprintf("tpad=stop_mode=clone:stop_duration=%.3f", c.length+0.2))
	}
	// the fps FILTER is what pins a stream to a rate: it duplicates and drops
	// frames until they land on that grid, whether the source was faster or
	// slower. Under VFR the rate is a ceiling instead, which is the encoder's
	// job and not a filter's (see fpsArgs).
	if st.FPS > 0 && !st.VFR {
		vf = append(vf, fmt.Sprintf("fps=%g", st.FPS))
	} else if c.cam != nil && !c.cam.static() {
		// zoompan places its window per frame, so a moving camera needs a grid
		// even when the output is VFR; this clip gets one at the camera's rate
		vf = append(vf, fmt.Sprintf("fps=%g", c.cam.fps))
		a.logfIdle("%s: the moving camera needs a fixed frame rate — this clip is %g fps",
			c.name(), c.cam.fps)
	}
	// the stop stills go on between here and the camera: in the clip's own
	// time but in the SOURCE frame, so a zoom or a view crops the still
	// exactly as it crops the footage running under it (the chain is spliced
	// in where fc is assembled below, at this point of the filter list)
	clock := len(vf)
	// what the bare parts of the frame get instead of black. Emitted below,
	// at the same point in the graph the pad it replaces sat at: after the
	// stop stills (a still is the recording's own frame and is padded with
	// it) and before the crop the camera does.
	var bd bdrop
	switch {
	case c.ins != "":
		// An insert has to come out at exactly the frame size the footage clips
		// do, because the join is a stream copy: the concat demuxer refuses a
		// clip whose dimensions differ from the ones before it, and there is no
		// re-encode later to paper over it. So it is fitted rather than scaled --
		// scaled down until it fits, then centred on a black frame, which keeps a
		// square diagram square and a 4:3 card 4:3 inside a 16:9 video.
		if c.boxW > 0 && c.boxH > 0 {
			bd = bdrop{w: c.boxW, h: c.boxH, fit: true}
		}
		// a video insert shorter than its slot would end the clip early and take
		// the narration written over it with it; the last frame is held instead
		vf = append(vf, fmt.Sprintf("tpad=stop_mode=clone:stop_duration=%.3f", c.length))
	case c.cam != nil:
		// the camera looking past the edge of the recording: the same padded
		// frame it always measured itself on, with the border filled in
		if pw, ph, pl, pt, ok := c.cam.padBox(); ok {
			bd = bdrop{w: pw, h: ph, x: pl, y: pt}
		}
		vf = append(vf, c.cam.chainOn(bd.on())...)
	case c.boxW > 0 && c.boxH > 0:
		// Plain footage comes out at the finished frame: the join is a stream
		// copy, so every clip must match. Fitted onto a backdrop, not stretched
		// (a 4:3 webcam keeps its shape); footage already the frame's shape takes
		// the plain scale, since the backdrop would be fully covered.
		if fitsFrame(c.video, c.boxW, c.boxH) {
			vf = append(vf, fmt.Sprintf("scale=%d:%d", c.boxW, c.boxH))
		} else {
			bd = bdrop{w: c.boxW, h: c.boxH, fit: true}
		}
	case st.Height > 0:
		// no box was worked out for this clip -- which the render always does
		// (clipBox), so this is a clip built by hand -- and the height that was
		// asked for is then the best answer available
		vf = append(vf, fmt.Sprintf("scale=-2:%d", st.Height))
	}
	bd.bare = st.Bare // black edges or a blurred blow-up: the region is the same
	vf = append(vf, "setsar=1")
	if cueFile != "" {
		vf = append(vf, "subtitles="+ffEscape(cueFile))
	}
	vlab := "v"
	head := "[0:v]"
	fc := ""
	if len(c.stills) > 0 || bd.on() {
		// the clock and the fps grid run on their own, so that the stills and
		// the backdrop can be spliced in between them and the camera
		pre := strings.Join(vf[:clock], ",")
		if pre == "" {
			pre = "null"
		}
		fc = head + pre + "[vpre];"
		head = "[vpre]"
		vf = vf[clock:]
	}
	if len(c.stills) > 0 {
		// each still is the same one-frame hold an audio insert's picture uses,
		// on an overlay input: cut to its first frame, cloned out past the
		// slot, faded on its alpha exactly as a title is (textChain), and laid
		// over the running footage only while its bar says so
		cur := strings.Trim(head, "[]")
		for k, sc := range c.stills {
			fc += fmt.Sprintf("[%d:v]trim=end_frame=1,setpts=PTS-STARTPTS,"+
				"tpad=stop_mode=clone:stop_duration=%.3f,format=rgba",
				stillBase+k, c.length+0.2)
			if sc.w > 0 && sc.h > 0 {
				// fitted and centred rather than stretched, and padded with
				// transparency (#00000000, not a black the toggle would owe an
				// answer for) so the running footage shows around a held frame
				// of another shape instead of a black border on a moving picture
				fc += fmt.Sprintf(",scale=%d:%d:force_original_aspect_ratio=decrease,"+
					"pad=%d:%d:(ow-iw)/2:(oh-ih)/2:color=#00000000", sc.w, sc.h, sc.w, sc.h)
			}
			if sc.fin > 0 {
				fc += fmt.Sprintf(",fade=t=in:st=%.3f:d=%.3f:alpha=1", sc.s, sc.fin)
			}
			if sc.fout > 0 {
				fc += fmt.Sprintf(",fade=t=out:st=%.3f:d=%.3f:alpha=1", sc.e-sc.fout, sc.fout)
			}
			next := fmt.Sprintf("vfz%d", k)
			fc += fmt.Sprintf("[fz%d];[%s][fz%d]overlay=x=0:y=0:eof_action=pass:enable=between(t\\,%.3f\\,%.3f)[%s];",
				k, cur, k, sc.s, sc.e, next)
			cur = next
		}
		head = "[" + cur + "]"
	}
	if bd.on() {
		sub, lab := bd.chain(strings.Trim(head, "[]"), 0)
		fc += sub
		head = "[" + lab + "]"
	}
	fc += head + strings.Join(vf, ",")
	if len(c.texts) > 0 {
		// the words go on AFTER the camera and the subtitles: a title is put on
		// the finished frame, and it holds still while the camera moves under it
		fc += "[vcam];"
		chain, last := textChain(c.texts, "vcam", txtBase, c.boxW, c.boxH)
		fc += chain
		vlab = last
	} else {
		fc += "[v];"
	}
	// footage off its own clock: everything recorded with the picture goes with
	// it, pitch held (atempoChain) -- the capture's own sound here, each
	// separate recording where it is prepared below
	slow := ""
	if c.ins == "" && !c.freeze && c.speed() != 1 && !c.audOwn {
		// ...or by tape speed, pitch and all, when that is the answer. A clip
		// whose sound is on its own clock takes neither: it is already at 1×.
		slow = atempoChain(c.speed())
		if c.audPitch {
			slow = asetrateChain(c.speed())
		}
		if srcSound {
			fc += fmt.Sprintf("[%s]%s%s[slowgm];", game, audFmt(st), slow)
			game = "slowgm"
		}
	}
	// a stop told to take the sound with it. The window is in the clip's own
	// output seconds, which is what the still overlay above is enabled on and
	// what the sound reads as too once the atempo above has run, so the
	// silence lands exactly under the held frame.
	if mute := hushExpr(c.hushes, c.stills); mute != "" {
		fc += fmt.Sprintf("[%s]%svolume=0:enable='%s'[hush];", game, audFmt(st), mute)
		game = "hush"
	}
	if c.snd != "" || c.audOwn {
		// a file shorter than its slot must not end the clip's sound early -- the
		// join is a stream copy, and a short track puts every later clip out of step.
		// Padded with silence past the slot; the output -t makes the length exact.
		// Sound off the picture's clock needs it too: under slow motion it reads
		// AHEAD and may ask for seconds the file has not got (cut_fxsound.go).
		fc += fmt.Sprintf("[%s]%s,apad[snd];", game, audFmt(st))
		game = "snd"
	}
	// The bed is everything that was there: the picture's own sound plus every
	// recording running under it, ducked together under the narration.
	// duration=first pins the mix to the picture's sound so a recording stopping
	// mid-clip does not end it early; normalize=0 keeps the game at its recorded
	// level.
	if len(c.mix) > 0 {
		seps := ""
		for k, m := range c.mix {
			fc += fmt.Sprintf("[%s]%s%s", trackOf(mixBase+k, m.track), audFmt(st), slow)
			// whole milliseconds, and only when there is a wait to honour: this
			// is the same integer adelay the narration learned to hand over
			if ms := int(math.Round(m.at * 1000)); ms > 0 {
				fc += fmt.Sprintf(",adelay=%d:all=1", ms)
			}
			fc += fmt.Sprintf("[sep%d];", k)
			seps += fmt.Sprintf("[sep%d]", k)
		}
		fc += fmt.Sprintf("[%s]%s[gm];", game, audFmt(st))
		fc += fmt.Sprintf("[gm]%samix=inputs=%d:duration=first:normalize=0[bed];",
			seps, 1+len(c.mix))
		game = "bed" // everything downstream ducks and mixes the bed, not the capture
	}
	// the volume effects, on everything that was actually there and nothing
	// that was added afterwards: after the bed, so a boost lifts the capture
	// and the separate recordings together the way one hand on one fader
	// would, and before the narration, which is written on this page and has
	// its own level (GameVol) rather than a cut effect's say over it.
	if fx, lab := gainChain(c.gains, game); fx != "" {
		fc += fx
		game = lab
	}
	// the seam where the sound goes back in sync with the picture: half a dip
	// here, half at the head of the clip that follows (audDipChain). On the
	// bed, so the lanes dip with the capture -- they came adrift together --
	// and before the narration, which is written on this page and has nothing
	// to do with where the footage's own sound is up to.
	if fx, lab := audDipChain(c, game); fx != "" {
		fc += fx
		game = lab
	}
	if len(spoken) > 0 {
		nrs := ""
		for k, ln := range spoken {
			// MILLISECONDS, as a whole number: adelay takes an integer per channel, and a
			// fractional seconds suffix ("13.09s") is silently dropped -- every hand-placed
			// line then started at zero, together, which was the overlapping narration.
			fc += fmt.Sprintf("[%d:a]atempo=%.3f,aresample=48000,%s,adelay=%d:all=1[nr%d];",
				voiceBase+k, math.Max(0.5, c.tempo), voicePan(st), delayMS(ln.delay), k)
			nrs += fmt.Sprintf("[nr%d]", k)
		}
		fc += fmt.Sprintf("[%s]volume=%.3f,aresample=48000[bg];", game, st.GameVol)
		fc += fmt.Sprintf("[bg]%samix=inputs=%d:duration=first:normalize=0,", nrs, 1+len(spoken)) +
			clipCeil + "," + audFmt(st) + "[a]"
	} else {
		fc += fmt.Sprintf("[%s]%s,%s[a]", game, clipCeil, audFmt(st))
	}
	args = append(args, "-filter_complex", fc, "-map", "["+vlab+"]", "-map", "[a]")
	if c.ins != "" || c.snd != "" || c.freeze || c.speed() != 1 {
		// the slot is what decides these clips' length, not the input: a still
		// has no length of its own, a looped animation has too much, tpad gave
		// a short video (and every freeze) an endless tail, and a rated clip's
		// stretch only lands on the slot to rounding. This is what stops them.
		args = append(args, "-t", fmt.Sprintf("%.3f", c.length))
	}
	args = append(args, fpsArgs(st)...)
	args = append(args, codecArgs(st)...)
	args = append(args, audioArgs(st)...)
	args = append(args, out)
	return a.runCmd(ffTool("ffmpeg"), args...)
}

// fpsArgs decides the output's frame timing, explicitly in every direction.
// Left to itself ffmpeg decides per container and per whether a rate was named,
// so the same checkbox would have meant different things depending on a
// dropdown three rows up.
//
// A ceiling and pure passthrough cannot be asked for together: -fpsmax next to
// an explicit -fps_mode vfr is refused as contradictory ("One of -r/-fpsmax was
// specified together a non-CFR -fps_mode"). So a rate under VFR goes in as
// -fpsmax, which is a ceiling rather than a target -- footage slower than it
// keeps its own rate instead of being duplicated up onto the grid, and footage
// faster is dropped down to it. That is what a headset capture wants: 72 is the
// peak it reaches, not the rate it holds. With no rate named there is nothing
// to clamp, and every source timestamp goes through as it came.
func fpsArgs(st prodSettings) []string {
	switch {
	case !st.VFR:
		return []string{"-fps_mode", "cfr"} // the fps filter already set the grid
	case st.FPS > 0:
		return []string{"-fpsmax", fmt.Sprintf("%g", st.FPS)}
	default:
		return []string{"-fps_mode", "vfr"}
	}
}

func codecArgs(st prodSettings) []string {
	switch st.Codec {
	case "h265":
		// hvc1 instead of hev1: QuickTime and Safari refuse the other tag
		return []string{"-c:v", "libx265", "-preset", st.Preset,
			"-crf", strconv.Itoa(st.CRF), "-tag:v", "hvc1", "-pix_fmt", "yuv420p"}
	case "vp9":
		return []string{"-c:v", "libvpx-vp9", "-crf", strconv.Itoa(st.CRF), "-b:v", "0",
			"-row-mt", "1", "-cpu-used", vp9Speed(st.Preset), "-pix_fmt", "yuv420p"}
	default:
		// -refs 4: x264's slow presets ask for 16 reference frames, which at 4K
		// forces Level 6.0, and no consumer hardware decoder implements 6.0 --
		// the file plays in software and tears on every GPU. Four keeps the
		// buffer inside 5.1 with margin; the level itself is left to x264.
		return []string{"-c:v", "libx264", "-preset", st.Preset,
			"-crf", strconv.Itoa(st.CRF), "-pix_fmt", "yuv420p", "-refs", "4"}
	}
}

// vp9Speed maps the x264 preset names onto libvpx's -cpu-used scale.
func vp9Speed(preset string) string {
	switch preset {
	case "ultrafast":
		return "8"
	case "veryfast":
		return "5"
	case "fast":
		return "4"
	case "medium":
		return "2"
	case "slow":
		return "1"
	default:
		return "0"
	}
}

// voicePan spreads the narration -- which is one channel however it was
// recorded -- over the layout the video is being made in. Said out loud rather
// than left to the encoder, because a mono source dropped into a stereo mix
// without it lands in the left speaker only.
func voicePan(st prodSettings) string {
	if st.Mono {
		return "pan=mono|c0=c0"
	}
	return "pan=stereo|c0=c0|c1=c0"
}

func audioArgs(st prodSettings) []string {
	br := strconv.Itoa(st.AudioKbps) + "k"
	// -ac as well as the aformat above: the filter graph decides the layout for
	// every clip that goes through it, and this decides it for anything that
	// does not, so the two can never disagree inside one concat list.
	ac := []string{"-ac", "2"}
	if st.Mono {
		ac[1] = "1"
	}
	if st.Container == "webm" {
		return append([]string{"-c:a", "libopus", "-b:a", br}, ac...)
	}
	return append([]string{"-c:a", "aac", "-b:a", br}, ac...)
}

// copyClip is what a pasted stretch of footage becomes. Mute here is a copy at
// the picture-alone scope: two silences, mute stops the recording's own track
// reaching the mixer and noLanes stops every separate recording being mixed
// under it.
func copyClip(i int, s cutSeg, from float64, v *tlVideo) prodClip {
	return prodClip{idx: i, video: v, local: v.at(from), tempo: 1, rate: 1,
		length: s.length(), mute: s.Mute, noLanes: s.Mute}
}

// sndClip is a sound laid over the session: the picture is the session's own
// (held under a splice, running over a selection) and the file replaces what
// the scope settled (cutSeg.Lane). Unnamed, it stands in for everything
// audible; named, for that one recording, and laneRecorded tells the capture's
// own track from a separate recording.
func sndClip(i int, s cutSeg, path string, v *tlVideo, recs []tlAudio) prodClip {
	c := prodClip{idx: i, video: v, local: v.at(s.S), tempo: 1, rate: 1,
		length: s.length(), freeze: s.spliced(), snd: path, sndAt: s.Ss,
		noLanes: s.spliced() || s.Lane == ""}
	if !s.spliced() && laneRecorded(recs, s.Lane) {
		c.dropLane = s.Lane
	}
	return c
}

// insClip is what an insert becomes: its own picture stretched to its slot,
// and -- covering the picture alone -- the sound of the recording it was laid
// over. The note is soundUnder's, logged by the caller.
func insClip(i int, s cutSeg, file string, vids []tlVideo) (prodClip, string) {
	c := prodClip{idx: i, ins: file, tempo: 1, rate: 1, length: s.length(), mute: s.Mute,
		noLanes: !s.keepsSoundUnder()}
	var note string
	c.snd, c.sndAt, note = soundUnder(s, vids)
	return c, note
}

// laneRecorded says whether a named lane is one of the separately-recorded
// files. The other two answers a lane name can have are the capture's own
// track -- a lane on the cut page, drawn from the video and not a recording of
// its own (masterLanes) -- and a recording that has since left the session. Both
// come out false here, and both mean the same thing to the render: there is no
// mix input to take out, so an inserted sound stands in for the capture's slot.
func laneRecorded(recs []tlAudio, base string) bool {
	for _, au := range recs {
		if au.base == base {
			return true
		}
	}
	return false
}

// soundUnder is where an insert covering the picture alone gets its sound:
// the recording it is drawn over, at the second it covers, in the sound
// input's slot (encodeClip). Ordinary inserts bring their own; a spliced muted
// one is meant to be silent. The note is for the two ways this comes out
// silent by accident: seconds in no recording, or a recording with no sound.
func soundUnder(s cutSeg, vids []tlVideo) (path string, at float64, note string) {
	if !s.keepsSoundUnder() {
		return "", 0, ""
	}
	v := pickVideoOn(vids, s.Cam, s.S)
	switch {
	case v == nil:
		return "", 0, fmt.Sprintf("covers the picture at %.0f s and keeps what is heard, "+
			"but those seconds fall in no recording — it plays silent", s.S)
	case !hasAudioStream(v.path):
		return "", 0, fmt.Sprintf("covers the picture at %.0f s and keeps what is heard, "+
			"but %s has no sound — it plays silent", s.S, v.base)
	}
	return v.path, v.at(s.S), ""
}

// spokenHere is what to add to a dropped-clip message when narration lines
// were written on it; empty for a clip nobody wrote on.
func spokenHere(entries []narrEntry, s cutSeg) string {
	n := 0
	for _, e := range matchEntries(entries, s) {
		if strings.TrimSpace(e.Text) != "" {
			n++
		}
	}
	switch n {
	case 0:
		return ""
	case 1:
		return " — the narration line written on it is dropped with it"
	default:
		return fmt.Sprintf(" — the %d narration lines written on it are dropped with it", n)
	}
}

// matchEntries finds the narration written for a segment, every line of it in
// placement order. Real overlap is enough, merely touching is not: a cut
// edited after narrating can shift, and a line silently dropped is the worst
// failure here.
func matchEntries(entries []narrEntry, s cutSeg) []*narrEntry {
	var out []*narrEntry
	for i := range entries {
		e := &entries[i]
		if s.E <= s.S {
			// a card or a freeze occupies no span of the session, so overlap
			// cannot find its lines: they are the entries written exactly on it
			if math.Abs(e.S-s.S) <= 0.05 && math.Abs(e.E-s.E) <= 0.05 {
				out = append(out, e)
			}
			continue
		}
		ov := math.Min(e.E, s.E) - math.Max(e.S, s.S)
		shorter := math.Min(e.E-e.S, s.E-s.S)
		if ov > 0 && (shorter <= 0 || ov >= shorter/2) {
			out = append(out, e)
		}
	}
	sort.SliceStable(out, func(a, b int) bool { return out[a].At < out[b].At })
	return out
}

func srtTime(t float64) string {
	if t < 0 {
		t = 0
	}
	ms := int(math.Round(t * 1000))
	return fmt.Sprintf("%02d:%02d:%02d,%03d", ms/3600000, ms/60000%60, ms/1000%60, ms%1000)
}

// wrapSub breaks a line into at most two subtitle rows of readable width.
func wrapSub(s string) string {
	words := strings.Fields(s)
	var rows []string
	cur := ""
	for _, w := range words {
		if cur == "" {
			cur = w
		} else if len(cur)+1+len(w) <= 42 {
			cur += " " + w
		} else {
			rows = append(rows, cur)
			cur = w
		}
	}
	if cur != "" {
		rows = append(rows, cur)
	}
	if len(rows) > 2 { // rebalance rather than drop text off the screen
		joined := strings.Join(rows, " ")
		half := len(joined) / 2
		cut := strings.LastIndex(joined[:half], " ")
		if cut < 0 {
			cut = half
		}
		rows = []string{joined[:cut], joined[cut+1:]}
	}
	return strings.Join(rows, "\n")
}

// subText is a caption as the .srt carries it: wrapped, and prefixed with its
// placement when that is not the bottom. {\an8} (top-center) and {\an5} (dead
// center) are ASS override tags in an srt file -- nonstandard but the one
// spelling everything honors: libass reads them when the burn happens
// (encodeClip's subtitles filter), and so do the usual players for a sidecar
// or muxed track. A bottom line carries no tag; the bottom is where subtitles
// already live.
func subText(ln prodLine) string {
	switch ln.pos {
	case "top":
		return `{\an8}` + wrapSub(ln.text)
	case "center":
		return `{\an5}` + wrapSub(ln.text)
	}
	return wrapSub(ln.text)
}

// ffEscape quotes a path for use inside a filtergraph argument, where ':' and
// ',' separate options and '\' escapes.
func ffEscape(p string) string {
	return strings.NewReplacer(
		`\`, `\\`, `:`, `\:`, `'`, `\'`, `[`, `\[`, `]`, `\]`, `,`, `\,`,
	).Replace(p)
}

// subCue is one line of the subtitle track on the produced timeline.
type subCue struct {
	s, e float64
	text string
}

// tidyCues makes the track watchable: it holds each line until the next one
// begins, drops the ones too short to read, and never lets two overlap.
//
// The cues come off the transcript one line each, and the gap between two
// lines of the same sentence is the breath between them -- five, twenty,
// eighty milliseconds. At thirty frames a second every one of those is the
// text blanking for a frame and coming back, and a sentence of six lines
// flickers six times. Nobody means those gaps to be seen: a subtitle that
// disappears between two words is a fault of the clock, not a choice. So a
// gap under subHold is closed by holding the line that is already up.
//
// A line's own length is left alone where the gap is a real pause -- a held
// caption over silence is what an editor would do anyway -- except at the very
// end, where nothing follows to hold it against.
func tidyCues(in []subCue) []subCue {
	var out []subCue
	for _, c := range in {
		if c.e < c.s {
			c.e = c.s
		}
		if n := len(out); n > 0 {
			p := &out[n-1]
			if c.s < p.e {
				p.e = c.s // no two on screen at once, whatever the clocks said
			}
			if c.s-p.e < subHold {
				p.e = c.s // the breath between two lines is not a blank screen
			}
			if p.e-p.s < subMin {
				// too short to read even after the hold: fold its words into
				// the line that follows rather than flashing them
				c.s, c.text = p.s, p.text+"\n"+c.text
				out = out[:n-1]
			}
		}
		out = append(out, c)
	}
	// ...and the last line has nothing to hold against, so it is given the
	// time a reader needs rather than the time it was spoken in
	if n := len(out); n > 0 && out[n-1].e-out[n-1].s < subMin {
		out[n-1].e = out[n-1].s + subMin
	}
	return out
}

const (
	// a gap between two lines shorter than this is a breath, not a blank
	// screen: the line already up is held across it
	subHold = 1.2
	// ...and no line is on screen for less than this, however briefly it was
	// said: under it a reader sees a flash rather than a word
	subMin = 0.8
)

// pickedLangs is the languages ticked, in the order the menu lists them. The
// language the session is SPOKEN in is never among them, whatever a tick says:
// its track is written anyway, and translating a language into itself is a
// call that costs a minute and answers with what it was given.
func (p *producer) pickedLangs() []string {
	// asrLanguage and not projectLanguage: the box on the Prepare page is
	// EMPTY by default and shows "en" as a placeholder, so the project's own
	// answer is "" until somebody types in it -- and "" matches no language,
	// which is why English stayed in the menu of an English session.
	own := p.a.asrLanguage()
	var out []string
	for _, l := range subLangs {
		if l.code == own {
			continue
		}
		if t := p.langTicks[l.code]; t != nil && t.Active() {
			out = append(out, l.code)
		}
	}
	return out
}

// syncLangs takes the session's own language out of the menu and puts the
// answer on the button, so the menu can stay shut. Called when the language
// changes as well as when a tick does: the box that says which language this
// is, is on another page, and this menu has to follow it.
func (p *producer) syncLangs() {
	if p.langs == nil {
		return
	}
	own := p.a.asrLanguage()
	for _, l := range subLangs {
		if t := p.langTicks[l.code]; t != nil {
			t.SetVisible(l.code != own)
		}
	}
	var names []string
	for _, c := range p.pickedLangs() {
		if _, n, ok := subLangOf(c); ok {
			names = append(names, n)
		}
	}
	if len(names) == 0 {
		p.langs.SetLabel("none")
		return
	}
	p.langs.SetLabel(strings.Join(names, ", "))
}

// syncSubLangs is that from anywhere, and safe before the page exists: a
// project loads before Produce is built, and the language box is on Prepare.
func (a *App) syncSubLangs() {
	if a.prod != nil {
		a.prod.syncLangs()
	}
}
