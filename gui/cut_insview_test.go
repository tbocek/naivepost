package main

// The card has to be visible on the page you put it on.
//
// An insert used to be a violet band on the timeline and nothing else: the
// preview went on showing the footage the card replaces, so whether the card
// said the right thing, or was even the right file, was a question only Produce
// could answer. What is pinned here is the answer to it -- a picture of the card
// at the moment the playhead is inside it -- rendered by the tool that renders
// the finished video, so a preview that agrees with itself also agrees with the
// render.

import (
	"bytes"
	"image"
	"image/color"
	_ "image/png"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"
)

// ---- what the renderer draws -------------------------------------------------

// An animated card is drawn at the moment asked for, not at its first frame.
//
// The card is a red bar that slides in from off the left over one second and
// then stays. At 0 s the place it ends up is background; at 2 s it is the bar.
// Reading the same pixel at both moments is the whole test: the alternative --
// librsvg's own reading of the document, which is the first frame and nothing
// after it -- gets the same picture twice.
func TestACardIsDrawnAtTheMomentThePlayheadIsIn(t *testing.T) {
	a := insertApp(t)
	a.root = t.TempDir()
	card := filepath.Join(a.root, "ranking.svg")
	if err := os.WriteFile(card, []byte(`<?xml version="1.0"?>
<svg xmlns="http://www.w3.org/2000/svg" width="400" height="400" viewBox="0 0 400 400">
  <rect width="400" height="400" fill="#202030"/>
  <rect y="40" height="60" width="120" x="-120" fill="#cc3344">
    <animate attributeName="x" from="-120" to="40" dur="1s" fill="freeze"/>
  </rect>
</svg>`), 0o644); err != nil {
		t.Fatal(err)
	}

	// relative, the way an insert is stored in a project: the renderer has to
	// put it back together with the project root, or it renders nothing
	at := func(when float64) image.Image {
		t.Helper()
		png, file, err := a.insPNG("ranking.svg", when)
		if err != nil {
			t.Fatalf("the card would not render at %g s: %v", when, err)
		}
		if file != "" {
			t.Fatalf("an svg was handed straight to the toolkit as %q — nothing would evaluate its animation", file)
		}
		img, kind, err := image.Decode(bytes.NewReader(png))
		if err != nil {
			t.Fatalf("what came back at %g s is not a picture: %v", when, err)
		}
		if kind != "png" {
			t.Errorf("the preview frame is a %s, want a png", kind)
		}
		return img
	}
	start, end := at(0), at(2)
	b := start.Bounds()
	if b.Dx() != 400 || b.Dy() != 400 {
		// scaled down but never up: a 400 px card is drawn at 400
		t.Errorf("the card came out %dx%d, want 400x400", b.Dx(), b.Dy())
	}
	// where the bar comes to rest, in the middle of it
	x, y := b.Min.X+b.Dx()*100/400, b.Min.Y+b.Dy()*70/400
	if red(start.At(x, y)) {
		t.Error("the bar is already at its resting place at 0 s — the animation is not being evaluated")
	}
	if !red(end.At(x, y)) {
		t.Errorf("the bar never arrives: at 2 s the card is %v where it should have come to rest", end.At(x, y))
	}
}

// red is "this is the card's bar and not its background", loosely enough to
// survive a rescale and a png round trip.
func red(c color.Color) bool {
	r, g, b, _ := c.RGBA()
	return r>>8 > 150 && g>>8 < 120 && b>>8 < 120
}

// A video insert is previewed at its own time too -- the same question one
// mechanism further along, since a clip dropped into the cut is as easy to get
// wrong as a card and its first frame says even less about it.
func TestAVideoInsertIsDrawnAtItsOwnTime(t *testing.T) {
	a := insertApp(t)
	a.root = t.TempDir()
	clip := filepath.Join(a.root, "later.mp4")
	// two seconds of green, then two of red
	mustFFmpeg(t, "-f", "lavfi", "-t", "2", "-i", "color=c=green:size=320x240:rate=15",
		"-f", "lavfi", "-t", "2", "-i", "color=c=red:size=320x240:rate=15",
		"-filter_complex", "[0:v][1:v]concat=n=2:v=1:a=0[v]", "-map", "[v]",
		"-c:v", "libx264", "-preset", "ultrafast", "-pix_fmt", "yuv420p", clip)

	pick := func(when float64) color.Color {
		t.Helper()
		png, _, err := a.insPNG("later.mp4", when)
		if err != nil {
			t.Fatalf("no frame at %g s: %v", when, err)
		}
		img, _, err := image.Decode(bytes.NewReader(png))
		if err != nil {
			t.Fatalf("what came back at %g s is not a picture: %v", when, err)
		}
		b := img.Bounds()
		return img.At(b.Min.X+b.Dx()/2, b.Min.Y+b.Dy()/2)
	}
	if red(pick(0.5)) {
		t.Error("half a second in, the preview is showing the second half of the clip")
	}
	if !red(pick(3)) {
		t.Errorf("three seconds in, the preview is still on the first half: %v", pick(3))
	}
}

// A still is not decoded here. GTK opens a png, a jpeg or a webp itself, and
// running it through a renderer to hand back the same picture is a decode, a
// scale and an encode to say what the file already said.
func TestAStillIsLeftToTheToolkit(t *testing.T) {
	a := insertApp(t)
	a.root = t.TempDir()
	still := filepath.Join(a.root, "title.png")
	mustFFmpeg(t, "-f", "lavfi", "-i", "color=c=red:size=320x240", "-frames:v", "1", still)

	png, file, err := a.insPNG("title.png", 0)
	if err != nil {
		t.Fatal(err)
	}
	if len(png) > 0 {
		t.Error("a still was re-rendered rather than handed over as it is")
	}
	if file != still {
		t.Errorf("the still resolves to %q, want %q — a path relative to the project is not a path", file, still)
	}
}

// ---- what the page does with it ----------------------------------------------

// Which insert the playhead is in, and how it moves. A still and a card that
// holds still are one picture for the whole slot; everything else is a frame per
// moment, and asking for a frame of a picture that never changes would be the
// same render eighty times over.
func TestTheCardUnderThePlayhead(t *testing.T) {
	a := insertApp(t)
	a.root = t.TempDir()
	moving := filepath.Join(a.root, "moving.svg")
	stillCard := filepath.Join(a.root, "static.svg")
	for path, body := range map[string]string{
		moving: `<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100">
  <rect width="20" height="20" x="0"><animate attributeName="x" from="0" to="80" dur="1s"/></rect></svg>`,
		stillCard: `<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100">
  <rect width="20" height="20" x="40"/></svg>`,
	} {
		if err := os.WriteFile(path, []byte(body), 0o644); err != nil {
			t.Fatal(err)
		}
	}

	ed := &cutEditor{a: a, segs: []cutSeg{
		{S: 10, E: 20},                    // footage
		{S: 20, E: 26, Ins: "moving.svg"}, // a card that animates
		{S: 26, E: 30},                    // footage again
		{S: 40, E: 44, Ins: "title.png"},  // a still, in a gap
	}}
	if s, _ := ed.insertAt(15); s != nil {
		t.Errorf("the playhead in the middle of a clip found the insert %q", s.Ins)
	}
	if s, into := ed.insertAt(20); s == nil || s.Ins != "moving.svg" {
		t.Error("the first frame of an insert is not inside it — the card would come up a frame late")
	} else if into != 0 {
		t.Errorf("the card's first frame reads as %g s into it", into)
	}
	if s, _ := ed.insertAt(26); s != nil {
		t.Error("the insert's end is inside it — the card would still be up over the footage after it")
	}
	if s, into := ed.insertAt(41); s == nil || s.Ins != "title.png" {
		t.Error("an insert dropped in a gap between recordings is not found")
	} else if into != 1 {
		t.Errorf("a second into the card reads as %g", into)
	}

	if f := a.newFilm("moving.svg"); f.fps <= 0 {
		t.Error("a card that animates would be drawn once and held")
	}
	for _, still := range []string{"static.svg", "title.png"} {
		if f := a.newFilm(still); f.fps != 0 {
			t.Errorf("%s asks for %g frames a second, and it never changes", still, f.fps)
		}
	}
	if f := a.newFilm("later.mp4"); f.fps <= 0 {
		t.Error("a video insert would be previewed as a single held frame")
	}
}

// ---- a card the footage is cut open for --------------------------------------

// The flag says "insert between the footage", and until now the preview played
// it exactly like the mode it is not: the stream ran on underneath, the card sat
// over it for as many seconds as the marker was wide, and the two modes were
// indistinguishable to watch. What makes them different is that the footage
// STOPS -- so what is pinned here is when it stops, which is the split point and
// not the marker.
//
// The marker is a mouse target and it grows with the zoom (spliceSpan). Trigger
// the hold from it and the same cut would play the card earlier the further you
// zoomed in, which is a preview lying about the video it is previewing.
func TestPlaybackRunsIntoASplicedCardAtTheSplitAndNotAtItsMarker(t *testing.T) {
	ed := &cutEditor{pps: 40, segs: []cutSeg{
		{S: 0, E: 30},                              // footage
		{S: 12, E: 12, Ins: "sting.mp4", Dur: 4},   // cut open here, play this
		{S: 20, E: 24, Ins: "title.svg"},           // over the footage: no hold
		{S: 26, E: 26, Ins: "outro.svg", Dur: 2.5}, // another split, later
	}}
	// the marker at this zoom is 160 px wide, i.e. four seconds of timeline,
	// and none of that is a moment the card starts at
	if x0, x1 := ed.spliceSpan(ed.segs[1]); x1-x0 < 100 {
		t.Fatalf("the marker is only %g px wide at pps 40 — this test is not testing what it says", x1-x0)
	}
	for _, c := range []struct {
		from, to float64
		want     string
	}{
		{11.90, 12.00, "sting.mp4"}, // the tick that reaches the split
		{11.00, 11.90, ""},          // near it, and inside its marker, is not it
		{12.00, 12.10, ""},          // already played: a split is crossed once
		{13.00, 12.90, ""},          // backwards is not a crossing at all
		{5.00, 13.00, ""},           // a seek over it is not playing through it
		{19.90, 20.10, ""},          // a card over the footage holds nothing
		{25.90, 26.00, "outro.svg"}, // the next one still fires
	} {
		got := ""
		if s := ed.splicedCrossed(c.from, c.to); s != nil {
			got = s.Ins
		}
		if got != c.want {
			t.Errorf("playing from %.2f to %.2f held for %q, expected %q",
				c.from, c.to, got, c.want)
		}
	}
}

// A held card plays for as long as it was given, then hands the footage back --
// and does not start again because the line is standing on its marker.
//
// That last part is the whole reason the hold remembers anything. Playback
// resumes at the split point, which is the middle of the marker, so the card is
// under the playhead for as long as it takes to play out of it. Without "done"
// the card would start over there, forever, and the preview would never reach
// the second half of the video.
func TestAHeldCardPlaysOnceAndGivesTheFootageBack(t *testing.T) {
	card := cutSeg{S: 12, E: 12, Ins: "sting.mp4", Dur: 4}
	ed := &cutEditor{pps: 40, playhead: 12, segs: []cutSeg{{S: 0, E: 30}, card}}

	now := time.Now()
	ed.hold = insHold{on: true, seg: card, start: now.Add(-2 * time.Second)}
	if s, into := ed.cardNow(); s == nil || s.Ins != "sting.mp4" {
		t.Fatal("the card holding the footage is not what the preview is showing")
	} else if into < 1.9 || into > 2.2 {
		t.Errorf("two seconds into a held card reads as %.2f s", into)
	}
	// its own clock, not the timeline's: the playhead has not moved and will
	// not until the card is over
	if got := ed.holdInto(now.Add(9 * time.Second)); got != card.Dur {
		t.Errorf("a card left running past its end reads as %g s of %g", got, card.Dur)
	}

	ed.endHold(true)
	if ed.hold.on {
		t.Error("the card is still holding the footage after it finished")
	}
	if s, _ := ed.cardNow(); s != nil {
		t.Errorf("the card came back up at the split it just played at (%q)", s.Ins)
	}
	// ...and it is only that one moment it is kept off the screen for. Dragging
	// the line away and back is a request to see the card, and gets it
	ed.playhead = 18
	if s, _ := ed.cardNow(); s != nil {
		t.Errorf("footage at 18 s shows the card %q", s.Ins)
	}
	ed.playhead = 12
	if s, _ := ed.cardNow(); s == nil {
		t.Error("scrubbing back onto the marker no longer shows the card")
	}
}

// What comes out of the speakers while a card is up is not the session.
//
// The render has always been clear about this: an insert's clip gets no session
// audio (clipMixes), and a video insert keeps its own (clipInput). The preview
// used to play the footage straight through the card, so an insert sounded
// exactly like the footage it replaces -- and a sting, whose entire job is to
// say something out loud, said nothing.
func TestACardIsHeardInsteadOfTheFootage(t *testing.T) {
	a := &App{root: t.TempDir()}
	ed := &cutEditor{a: a}
	ed.hold.on = true // something is playing; sound follows the transport

	video := cutSeg{S: 12, E: 12, Ins: "sting.mp4", Dur: 4}
	if got, want := ed.cardVoice(&video), filepath.Join(a.root, "sting.mp4"); got != want {
		t.Errorf("an inserted video is heard as %q, expected %q", got, want)
	}
	// a card carries its parameters in its path; a file name is what plays
	withParams := cutSeg{S: 5, E: 5, Ins: "sting.mp4?title=Later", Dur: 4}
	if got, want := ed.cardVoice(&withParams), filepath.Join(a.root, "sting.mp4"); got != want {
		t.Errorf("a video insert with parameters is heard as %q, expected %q", got, want)
	}
	for _, quiet := range []string{"title.png", "ranking.svg?tiers=S,A"} {
		s := cutSeg{S: 20, E: 24, Ins: quiet}
		if got := ed.cardVoice(&s); got != "" {
			t.Errorf("%s would play %q — a drawn card has no sound of its own", quiet, got)
		}
	}
	if got := ed.cardVoice(nil); got != "" {
		t.Errorf("footage plays the insert sound %q", got)
	}
	// parked on a card is a picture, not a sting on a loop: no player and no
	// hold means nothing is running
	ed.hold.on = false
	if got := ed.cardVoice(&video); got != "" {
		t.Errorf("a parked preview plays %q under the playhead", got)
	}
}

// ---- the wiring --------------------------------------------------------------

func TestTheInsertPreviewIsWired(t *testing.T) {
	for file, wants := range map[string][]string{
		"cut.go": {
			// every path that moves the red line asks what is under it
			"ed.showInsert()\n\ted.redrawTracks()",                                  // a click / a seek
			"ed.showInsert()     // stepping through a card steps through the card", // ‹f and f›
			"ed.film = nil // another project's card is not this one's",
			// playback's own clock: a card the footage is cut open for stops it
			// here, everything else is drawn as the line passes through it
			"if s := ed.splicedCrossed(was, ed.playhead); s != nil {",
			"ed.startHold(s)",
			// the else arm, ending at its own brace rather than at whatever
			// follows: what this pins is that the card check HAS an else, not
			// what playback goes on to do next
			"} else {\n\t\t\ted.showInsert()\n\t\t}",
			"if ed.hold.on {\n\t\ted.tickHold()",
			// and a hand on the line is the end of any hold
			"ed.cancelHold()",
		},
		"player.go": {
			// the swap itself: one picture, showing either the stream or a frame
			"func (p *Player) ShowStill(tex gdk.Paintabler) {",
			"p.Picture.SetPaintable(tex)",
			"func (p *Player) ShowVideo() {",
			"p.Picture.SetPaintable(p.video)",
			// the sound of it: the session cut, the insert's own played. The
			// card's answer is one of two about the session's sound now -- the
			// scene under the line silences lanes of its own (Player.hush) --
			// so both go through the one place that writes the property
			"func (p *Player) SetMuted(v bool) {",
			"p.muted = v\n\tp.applyMute()",
			`setGain(p.gain, p.pb, p.vol(), m)`,
			"func (p *Player) CardSound(file string, at float64, play bool) {",
		},
		"cut_insview.go": {
			// one frame in flight, and the result goes back through showInsert
			// rather than onto the picture, because the playhead has moved
			"if f.busy {",
			"glib.IdleAdd(func() {",
			"ed.showInsert()",
			// whatever is on the picture decides what comes out of the speakers
			"s, into := ed.cardNow()\n\ted.cardSound(s, into)",
			"ed.player.SetMuted(cardHush(s) || fxHush(ed.fx, ed.playhead))",
		},
	} {
		b, err := os.ReadFile(file)
		if err != nil {
			t.Fatal(err)
		}
		for _, want := range wants {
			if !strings.Contains(string(b), want) {
				t.Errorf("%s no longer contains %q", file, want)
			}
		}
	}
}
