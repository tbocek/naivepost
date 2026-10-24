package main

// The thumbnail half of Produce: a picture and the words under it.
//
// The picture is an EDIT of real frames through sd.cpp's native API (sdcpp.go):
// ref_images keep it recognizably this video, and an edit model changes only
// what the instruction names (img2img renoised everything). The row of images
// is the user's; the first is the base, the rest are references named by
// position; an empty row draws from the instruction alone. No words come from
// the model: the title and marked texts are printed locally (publish_text.go),
// and the instruction asks for no lettering. One model job writes the title,
// the edit instruction and the description in one reply (prompt on Prepare).
//
// publish/thumbnail.png        the upload: the picture with the words printed on
// publish/thumbnail-plain.png  the picture as drawn, no words -- what re-prints start from
// publish/description.txt      the YouTube description
// publish/publish.json         all of it as data

import (
	"bytes"
	"crypto/sha1"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"image/jpeg"
	"math"
	"os"
	"path/filepath"
	"regexp"
	"strconv"
	"strings"

	"github.com/diamondburned/gotk4/pkg/gdk/v4"
	"github.com/diamondburned/gotk4/pkg/glib/v2"
	"github.com/diamondburned/gotk4/pkg/gtk/v4"
	"github.com/diamondburned/gotk4/pkg/pango"
)

const (
	// How many frames the FIRST run takes from the cut. Only a starting point:
	// the row is a list you add to and remove from, so a session can end up
	// with none, one, or several. Three, because the instruction composes ONE
	// picture out of them -- the topic, the game, the moment -- and few
	// enough that each is shown at a size where you can tell whether its
	// subject survives being shrunk.
	defPubFrames = 3

	// And how many the row will hold. Not a technical limit -- sd.cpp takes as
	// many references as you send it -- but every image here is also attached
	// to the vision call that picks the base, where each one costs context and
	// makes the choice woollier. Eight is past the point where more helps.
	maxPubFrames = 8
)

// ---- the prompts --------------------------------------------------------------
//
// One paragraph or bullet per line, unwrapped: see describeSystem.

const youtubeSystem = `You write the upload text for a finished video on YouTube: its title, and the description that sits under it.

You are given what the video is made of -- its clips, what was seen and said in each, and the narration that was written over it. That is the video.

What the user context singles out is what the description should lead with. Answer in the upload text's shape.

The title.

- Four to seven words. It is the YouTube title, and it is also printed across the upper part of the thumbnail afterwards, read at the size of a phone's sidebar -- every extra word costs one that mattered.
- Say the specific thing that happens in THIS video: the moment, the mistake, the win, the thing nobody expected. A title that would fit any session like it is a wasted title.
- Plain words people say out loud. No colons splitting a subtitle off, no clickbait punctuation, no ALL CAPS -- it is drawn in large letters already.
- Never promise something the clips do not contain: a title is a claim about the video, and this one is the claim most people will only ever read.

The thumbnail.

There are two ways to answer for it, and the user context decides which.

- Where the context asks for a picture the video ALREADY CONTAINS -- a slide, a title card, a particular moment, anything phrased as "use the frame that shows ...", "pick one", "do not generate" -- the thumbnail line is a frame line: answer it as "THUMBNAIL: frame: clip <n> +<seconds>", naming the clip whose block that picture is described in and the offset stamped on the EVENT line that describes it -- both copied off the brief, neither worked out -- and write no instruction anywhere. The editor takes that frame as it is and prints the title onto it. This is the better answer whenever the context offers it: a frame out of the video cannot promise something the video does not contain.
- Otherwise the thumbnail line is the instruction below, and no second is named.

The thumbnail instruction.

- One or two sentences telling the image model how to compose ONE picture out of the frames: what this video is about, and the moment it shows.
- It is an instruction, not a description. Anything you do not mention is left alone, so describing the whole scene gets a picture of something else instead of the moment that was filmed. Say what to combine, brighten, push forward, or clear out of the way.
- Name only things the clips contain. "Add the dragon" to a video with no dragon in it is a thumbnail that lies about the video.

The description.

- Open with one or two sentences that say what happens in this video, in plain language, and make someone want to watch it. This first line is the only part shown before "...more", so it has to work alone.
- Then a short paragraph, three or four sentences, on what the session actually was: where it is set, who is in it, what went right and wrong.
- Then a chapter list if the video has distinct beats -- one line per beat, "0:00 What this is", at the time the clip list gives for that clip ("at 1:23 in the video"), never a session time. Only if the beats are real; a made-up timestamp is worse than no chapter list.
- Finish with a line of five to eight hashtags, lower case, naming what this is, where it is set and the kind of moment. No hashtag salad.

Voice.

- The voice of someone who was there and is telling a friend about it, not a press release. Contractions are fine.
- No emoji walls, no "smash that like button", no promises about upload schedules, no links to things you were not told exist.`

// ---- what the project keeps -----------------------------------------------------

// pubSettings is the Publish page as the project stores it: decisions the user
// made or suggestions they let stand, nothing derived. Frames are stored the
// way every path in a project is (storePath) -- the candidates are written
// inside the project, so they travel with it.
type pubSettings struct {
	// The images, in the order the image model is given them. The FIRST is the
	// base -- the picture being edited -- and the rest are there to be referred
	// to ("the ship from the second image"). Order is the whole answer, which
	// is why there is no index beside it any more: a list and a pointer into it
	// are two places for the same fact, and they drift.
	Frames []string `json:"frames,omitempty"`

	// Which is what this is, and why it is only ever read. Projects written
	// before the row became a list carry the base as an index; migrate() turns
	// one into the other, and nothing writes it again.
	Base int `json:"base,omitempty"`

	// Where the thumbnail's frame sits on the base image: the centre of the
	// crop box, in fractions of that image (publish_crop.go). A pointer, and
	// nil is the middle -- the box that has never been dragged -- because 0,0
	// is a legitimate place to drag one TO and a project written before the
	// box existed must not read as "pushed into the top-left corner".
	Crop *pubPoint `json:"crop,omitempty"`

	// Own says the thumbnail is a picture chosen from the row, not one the model
	// drew (pubSlot.useAsThumbnail); while set, ▶ prints the words and leaves the
	// picture alone. Cleared only by ↻ over the thumbnail.
	Own bool `json:"own,omitempty"`

	// Where the title is printed, when it is not in the default band
	// (pubTitleBox). A box like a marked text's -- drawn, dragged and resized
	// exactly as one -- but the WORDS are the title entry's, so this one's
	// Text is never read and never written.
	TitleBox *pubText `json:"title_box,omitempty"`

	// ...and the WORDS in it, the picture's own. They start as the video's title
	// (seedThumbTitle, once, when the first thumbnail exists) and part ways the
	// moment either is edited: a thumbnail's line and a YouTube title are read at
	// different sizes.
	ThumbTitle string `json:"thumb_title,omitempty"`
	// ...and whether that has happened, which is not the same as the words
	// being empty: a line taken off the picture on purpose must not come back
	// the next time a thumbnail is drawn.
	TitleSeeded bool `json:"title_seeded,omitempty"`
	// the old spelling of "not printed", read once by migrate: it meant the
	// entry's words were the picture's and were being withheld.
	TitleOff bool `json:"title_off,omitempty"`

	// The words printed onto the picture after the draw (publish_text.go):
	// each a box in fractions of the finished thumbnail and the text fitted
	// into it. The title is not one of them -- it has its own field below and
	// its own band, which starts at the top and can be dragged (pubTitleBox).
	Texts []pubText `json:"texts,omitempty"`

	Title    string `json:"title,omitempty"`
	Prompt   string `json:"prompt,omitempty"`
	Negative string `json:"negative,omitempty"`
	Desc     string `json:"description,omitempty"`
}

// pubPoint is a place on a picture, as fractions of it.
type pubPoint struct {
	X float64 `json:"x"`
	Y float64 `json:"y"`
}

// cropRect is the crop box this project asks for on a base image of aspect
// srcA, for a thumbnail of aspect outA. Everything about its size comes from
// the two aspects; only the centre is the user's.
func (st pubSettings) cropRect(srcA, outA float64) fxRect {
	cx, cy := 0.5, 0.5
	if st.Crop != nil {
		cx, cy = st.Crop.X, st.Crop.Y
	}
	return pubCropAt(srcA, outA, cx, cy)
}

// basePath is the picture being edited, or "" when the row is empty -- which is
// allowed: with no images at all the instruction is drawn from nothing, which
// is a thumbnail some sessions actually want.
func (st pubSettings) basePath() string {
	if len(st.Frames) == 0 {
		return ""
	}
	return st.Frames[0]
}

// migrate brings a project written against the old fixed pair -- two slots and
// a radio saying which was the base -- up to the list. The radio's answer is
// applied by moving that frame to the front, where the answer now lives.
func (st pubSettings) migrate() pubSettings {
	st.Frames = moveToFront(append([]string(nil), st.Frames...), st.Base)
	st.Base = 0
	// a project from when the picture printed the entry's words: it was
	// printing them unless title_off said otherwise, and either way that
	// question has been answered once already
	if !st.TitleSeeded && (st.TitleOff || st.Title != "") {
		st.TitleSeeded = true
		if !st.TitleOff {
			st.ThumbTitle = st.Title
		}
	}
	st.TitleOff = false
	// "frame: 10" in the instruction box was never an instruction. It is a
	// frame line the reader had nowhere to put, from before the thumbnail line
	// could carry one (splitUploadAt), and left there it goes to the image
	// model as the description of a picture -- which is how a run came back
	// generation_failed with no thumbnail at all. One line only: a real
	// instruction does not fit on one and start with the label.
	if p := strings.TrimSpace(st.Prompt); !strings.Contains(p, "\n") {
		if _, _, ok := peelLabel(p, "frame:"); ok {
			st.Prompt = ""
		}
	}
	return st
}

// moveToFront makes fs[i] the base. Out-of-range is a no-op rather than an
// error: i comes from a project file and from a model's reply, and neither is
// trustworthy enough to panic on.
func moveToFront(fs []string, i int) []string {
	if i <= 0 || i >= len(fs) {
		return fs
	}
	out := append([]string{fs[i]}, fs[:i]...)
	return append(out, fs[i+1:]...)
}

// ---- the page ------------------------------------------------------------------

type publisher struct {
	a *App

	// The images and the row of widgets showing them. frames is the state --
	// the widgets are rebuilt from it whenever it changes, rather than being
	// mutated in place. With a row whose length changes, rebuilding is both
	// shorter and the only version that cannot leave a stale slot behind.
	frames    []string
	framesBox *gtk.Box
	addFrame  *gtk.Button

	// where the thumbnail's frame sits on the base image, and the shape that
	// frame is. crop is nil until the box has been dragged, which is the same
	// "never chosen" the project file stores; aspect is the cut's, cached by
	// reread because the crop box is redrawn on every pointer move and reading
	// cut.json per frame is not a thing to do in a draw handler.
	crop *pubPoint
	// the thumbnail is a picture chosen from the row, not a drawn one
	// (pubSettings.Own)
	own bool
	// where the picture's line is printed, nil for the default band
	// (pubSettings.TitleBox)
	titleBox *pubText
	// the words printed there, and whether they have ever been put on a
	// picture (pubSettings.ThumbTitle, TitleSeeded)
	thumbTitle  string
	titleSeeded bool
	aspect      string

	// the marked words and the layer that shows them (publish_text.go).
	// texts is the state, like frames; shotPath and shotA are what showShot
	// last put in the picture, cached because the overlay's draw handler and
	// gestures must not open the file per frame; quiet stops apply's SetText
	// from re-printing what a run just printed.
	texts    []pubText
	shotOver *gtk.DrawingArea
	shotPath string
	shotA    float64
	quiet    bool

	title *gtk.Entry
	// The four editable boxes, split by column: prompt and neg are the drawing
	// side and are typed by hand, title and desc are the writing side and are
	// what the model suggests. Text views rather than entries for the long
	// ones -- an edit instruction is a paragraph and a description is several.
	prompt, neg, desc *gtk.TextView
	shot              *gtk.Picture // what was drawn last
	out               *gtk.Label
	suggest           *gtk.Button
	redraw            *gtk.Button
	export            *gtk.Button
}

// pubSlot is one image in the row: which position it is in, what is in it, and
// the three things you can do to it -- promote it to base, swap it, drop it.
// Built fresh on every rebuild, so it holds no widgets worth keeping.
type pubSlot struct {
	p    *publisher
	i    int
	path string
}

// publishDir is under produce/ with the video and the per-clip encodes: what
// this step makes is one upload, and an upload is one folder to open and copy
// away. A project written before that keeps its own publish/ -- the thumbnail
// and the description are work, and a folder move is not a reason to redo it.
func (a *App) publishDir() string {
	if old := filepath.Join(a.outDir, "publish"); exists(old) {
		return old
	}
	return filepath.Join(a.produceDir(), "publish")
}

// publishRecorded reports whether the model has written this session's text:
// publish.json, laid down before anything is drawn so a failed draw keeps the
// thinking. A file, not a project flag: removing the folder is the gesture
// that means start this step over.
func (a *App) publishRecorded() bool {
	return exists(filepath.Join(a.publishDir(), "publish.json"))
}

// buildPublishPanes is the thumbnail-and-words half of the Produce page, in
// parts: the drawing column, the written column (title, description) and the
// on-disk row. It was a page of its own; Produce owns the page now and one ▶
// makes everything the upload needs.
func (a *App) buildPublishPanes() (draw, said gtk.Widgetter) {
	p := &publisher{a: a}
	a.pub = p

	// The images, side by side and in order. A row rather than a column because
	// the first question asked of them is which one is the base, and that is a
	// question you answer by looking at them together.
	p.framesBox = gtk.NewBox(gtk.OrientationHorizontal, 8)
	p.framesBox.SetHomogeneous(true)
	p.rebuildFrames()

	p.addFrame = gtk.NewButtonWithLabel("Add image…")
	p.addFrame.AddCSSClass("flat")
	p.addFrame.SetTooltipText("Add another image the instruction can refer to — a frame from the " +
		"session, a logo, anything on disk. The first in the row is the one being edited; " +
		"the rest are only there to be named (\"the ship from the second image\")")
	p.addFrame.ConnectClicked(func() { p.addImage() })

	framesHead := p.a.heading("Images", fmt.Sprintf("What the image model is given, in order. The FIRST is "+
		"the base — the picture being edited — and the others are references the instruction can name. "+
		"%d are taken from the cut the first time this page runs; after that the row is yours: add, "+
		"remove, swap, or make another one the base. An empty row is allowed, and draws the thumbnail "+
		"from the instruction alone.", defPubFrames), p.addFrame)

	// The two long boxes on the drawing side. Both are text views rather than
	// entries: an instruction is a paragraph, and a negative prompt is a list
	// that outgrows one line the moment a picture goes wrong in a new way.
	var promptBox, negBox, descBox *gtk.ScrolledWindow
	p.prompt, promptBox = p.textBox(4, "What the image model is told to make out of the images. "+
		"Written by the same call that writes the title, and yours to rewrite. "+
		"Plain sentences: \"blur the background\", \"add the ship from the second image behind them\". "+
		"Anything you do not mention is left alone, so describing the whole scene gets you a different one. "+
		"No words: the title and the marked texts are printed on afterwards, and the model is told to letter nothing.")
	p.neg, negBox = p.textBox(2, "What must stay out of the picture — watermarks, logos, "+
		"lettering, extra limbs.")

	// The result, at the size it will be judged at, under the boxes that make
	// it -- so pressing ▶ after rewording the instruction shows the change in
	// the same place you asked for it.
	p.shot = gtk.NewPicture()
	p.shot.SetCanShrink(true)
	// no fixed height: this column scrolls, so a request was the whole answer and
	// the picture never resized. Left alone a GtkPicture asks for the height its
	// width implies. The floor is for the empty state, where there is no picture
	// to measure.
	p.shot.SetSizeRequest(-1, 120)
	shotFrame := videoFrame(p.textOverlay(p.shot))
	shotFrame.SetMarginTop(4)

	// ↻ over the picture, and the same ↻ the narration re-rolls a line with:
	// one mark for "make me another one of these", wherever the app offers it.
	//
	// It draws and does nothing else -- no model call, no render -- because
	// that is the loop this page is for: reword the instruction, look, reword
	// it again. ▶ was the only way to redraw and it also rendered the video,
	// so trying a second thumbnail cost an encode.
	p.redraw = gtk.NewButtonFromIconName("view-refresh-symbolic")
	p.redraw.AddCSSClass("flat")
	p.redraw.SetTooltipText("Draw the thumbnail again from the images and the instruction as they " +
		"stand — a fresh draw, nothing rewritten and nothing rendered. The title is printed onto " +
		"it afterwards, as always.")
	p.redraw.ConnectClicked(func() { a.publishRedraw() })

	// ...and out of the app. The thumbnail is kept as a PNG because it is
	// re-printed from the plain copy every time a word changes, and a
	// generation of JPEG per keystroke would be a picture that got worse the
	// more it was worked on. What YouTube takes is a JPEG under 2 MB, so the
	// one conversion happens here, on the way out, once.
	p.export = gtk.NewButtonFromIconName("document-save-symbolic")
	p.export.AddCSSClass("flat")
	p.export.SetTooltipText("Export the thumbnail as a JPEG — what YouTube takes, " +
		"under its 2 MB limit. The words are on it; the copy here stays a PNG.")
	p.export.ConnectClicked(func() { p.exportThumb() })

	// LEFT: everything that makes the picture, in the order it happens --
	// choose the images, say what to change, say what to keep out, look at what
	// came back. Nothing on this side calls the language model: the instruction
	// arrives from the one call the words' side makes, and is then this side's.
	col := gtk.NewBox(gtk.OrientationVertical, 6)
	col.SetMarginTop(8)
	col.SetMarginBottom(8)
	col.SetMarginStart(12) // the window's edge
	col.SetMarginEnd(6)    // ...and the handle's
	col.Append(framesHead)
	col.Append(p.framesBox)
	col.Append(p.a.heading("Edit instruction", "What to change about the first image, sent to sd.cpp with the whole row — ▶ writes one, and it is yours to rewrite"))
	col.Append(promptBox)
	col.Append(p.a.heading("Negative prompt", "What must not appear"))
	col.Append(negBox)
	col.Append(p.a.heading("Thumbnail", "What sd.cpp drew from the images and the instruction above",
		p.export, p.redraw))
	col.Append(shotFrame)

	drawScroll := gtk.NewScrolledWindow()
	drawScroll.SetChild(col)
	drawScroll.SetPolicy(gtk.PolicyNever, gtk.PolicyAutomatic)
	drawScroll.SetVExpand(true)

	// RIGHT: everything the language model writes. The title lives here rather
	// than beside the drawing: this is what writes it and where it is read -- it
	// is the YouTube title first and the words on the thumbnail second.
	p.title = gtk.NewEntry()
	p.title.SetHExpand(true)
	p.title.SetPlaceholderText("the video's title, also printed on the thumbnail — ▶ suggests one")
	p.title.SetTooltipText("The YouTube title. The first thumbnail to exist takes it as the " +
		"line printed across it; after that the two are separate — reword this and the picture " +
		"keeps its line, reword the picture's (its ✎) and the upload keeps this. " +
		"Four to seven words: a thumbnail is read at the size of a phone's sidebar.")
	// ...and it does NOT re-print the picture. The thumbnail carries its own
	// line, taken from this one the first time a thumbnail exists and its own
	// words from then on (pubSettings.ThumbTitle): rewording the upload's
	// title is not an instruction to redraw the words on a picture that may
	// have been cropped, moved and reworded around them.

	p.desc, descBox = p.textBox(8, "The text under the video on the YouTube page. Written by the "+
		"prompt above, and yours to rewrite.")

	// ↻ sits on the Title heading because the title is the first thing it
	// rewrites -- it is one call and it writes all three, the instruction on the
	// other side of the page included -- and it is the only thing that rewrites
	// them: ▶ writes them once and then never touches them again
	p.suggest = gtk.NewButtonFromIconName("view-refresh-symbolic")
	p.suggest.AddCSSClass("flat")
	p.suggest.SetTooltipText("Ask the model for a fresh title, thumbnail instruction and " +
		"description — the only thing that does. ▶ never rewrites text that has already been " +
		"written, and nothing here redraws the picture; ▶ does that")
	p.suggest.ConnectClicked(func() { a.publishSuggest() })

	wrote := gtk.NewBox(gtk.OrientationVertical, 6)
	// no margins of its own: the words and the encoder settings under them are
	// two rows of one column, and the column carries the margins for both
	// (buildProduce). Its own pair put the words 12 further in than the
	// settings, so the two halves of one column had two right edges.
	wrote.Append(p.a.heading("Title", "The YouTube title, printed across the top of the thumbnail",
		p.suggest))
	wrote.Append(p.title)
	wrote.Append(p.a.heading("YouTube description", "The text under the video on the upload page"))
	wrote.Append(descBox)
	descBox.SetVExpand(true)
	wrote.SetVExpand(true)

	// no Outputs row of its own: the thumbnail and the words are written
	// under produce/ with the video, and the page has one Outputs group
	// (producer.updateOut)
	p.refresh()
	return drawScroll, wrote
}

// heading is the one-line label above each field, with anything the caller
// wants on its right. It joins the same size group every prompt box's heading
// row is in, so the fields on this side of the divider line up with the prompts
// on the other.
func (a *App) heading(title, tip string, extra ...gtk.Widgetter) *gtk.Box {
	l := gtk.NewLabel(title)
	l.SetXAlign(0)
	l.SetHExpand(true)
	l.SetEllipsize(pango.EllipsizeEnd)
	l.AddCSSClass("heading")
	l.SetTooltipText(tip)
	row := gtk.NewBox(gtk.OrientationHorizontal, 8)
	row.Append(l)
	for _, w := range extra {
		row.Append(w)
	}
	if a.headGroup == nil {
		a.headGroup = gtk.NewSizeGroup(gtk.SizeGroupVertical)
	}
	a.headGroup.AddWidget(row)
	return row
}

// textBox is one of the editable result fields, floored at lines of text.
// Monospace, like every editable box in the app, and the floor is measured in
// monospace lines (a few px taller than the proportional font).
func (p *publisher) textBox(lines int, tip string) (*gtk.TextView, *gtk.ScrolledWindow) {
	tv := gtk.NewTextView()
	tv.SetMonospace(true)
	tv.SetWrapMode(gtk.WrapWord)
	tv.SetTopMargin(4)
	tv.SetBottomMargin(4)
	tv.SetLeftMargin(6)
	tv.SetRightMargin(6)
	tv.SetTooltipText(tip)
	sc := gtk.NewScrolledWindow()
	sc.SetChild(tv)
	sc.SetPolicy(gtk.PolicyNever, gtk.PolicyAutomatic)
	sc.SetSizeRequest(-1, lines*monoLineH+8)
	sc.AddCSSClass("frame")
	return tv, sc
}

// monoLineH is a monospace line's height at the app's font size, near enough
// for a size REQUEST: the box scrolls, so being a pixel out costs nothing, and
// measuring it properly means building a widget to ask (narrate.go does, for a
// number that has to be exact).
const monoLineH = 20

// setFrames replaces the row. Everything that changes the list goes through
// here -- a user gesture and apply alike -- so one place keeps the widgets,
// the cap and the Inputs line in step. Nothing flags the edit beyond that:
// the project is bytes-compared by the autosave. The cap lives here rather
// than on the Add button so that it also holds for a hand-edited project
// file: the row is what gets attached to the vision call, and twenty images
// is a call that costs a fortune and answers worse.
func (p *publisher) setFrames(fs []string) {
	p.frames = append([]string(nil), fs...)
	if len(p.frames) > maxPubFrames {
		p.frames = p.frames[:maxPubFrames]
	}
	p.rebuildFrames()
	p.a.updateProduceInfo() // the page's Inputs row counts these images
}

// rebuildFrames throws the row away and builds it again from p.frames. Cheaper
// to reason about than editing it in place: a row whose length changes has to
// renumber every slot after the one that moved anyway, and "Ref 3" left over
// beside the second image is the kind of wrong that is never noticed.
func (p *publisher) rebuildFrames() {
	if p.framesBox == nil {
		return
	}
	for c := p.framesBox.FirstChild(); c != nil; c = p.framesBox.FirstChild() {
		p.framesBox.Remove(c)
	}
	if len(p.frames) == 0 {
		// an empty row is a legitimate state, not an error, so it says what it
		// will do rather than looking like something failed to load
		empty := gtk.NewLabel("No image — the thumbnail will be drawn from the instruction alone.\n" +
			"Add image… to edit one of your own frames instead.")
		empty.AddCSSClass("dim-label")
		empty.SetJustify(gtk.JustifyCenter)
		empty.SetVExpand(true)
		p.framesBox.Append(empty)
	}
	for i, f := range p.frames {
		s := &pubSlot{p: p, i: i, path: f}
		p.framesBox.Append(s.build())
	}
	if p.addFrame != nil {
		p.addFrame.SetSensitive(len(p.frames) < maxPubFrames)
	}
}

// addImage appends one. New images go to the END, never the front: appending
// cannot silently change which picture is being edited, and promoting is one
// click away for when that is what was meant.
func (p *publisher) addImage() {
	if len(p.frames) >= maxPubFrames {
		return
	}
	start := ""
	if n := len(p.frames); n > 0 {
		start = filepath.Dir(p.frames[n-1])
	}
	p.pickImage(fmt.Sprintf("Image %d", len(p.frames)+1), start, func(path string) {
		p.setFrames(append(p.frames, path))
	})
}

// useAsThumbnail makes this picture the thumbnail without drawing anything:
// it is written as the plain copy and the words are printed onto it, which is
// the same last step a drawn one gets (recomposite).
//
// It exists because a session often already contains the picture. The frame
// where the tower is lit and centred is a better thumbnail than anything a
// model will invent from it, and until now the only way to get it there was
// to make it the base and ask sd.cpp to change as little as possible -- a GPU
// run to approximate a file that was already on disk.
//
// Cropped like the base is (pubCropRect): a thumbnail is the video's shape,
// and a widescreen frame dropped whole into a vertical thumbnail would be
// letterboxed by whatever showed it.
func (s *pubSlot) useAsThumbnail() {
	p := s.p
	if p.a.busy() {
		return
	}
	dir := p.a.publishDir()
	if err := os.MkdirAll(dir, 0o755); err != nil {
		p.a.logf("thumbnail: %v", err)
		return
	}
	w, h := pubBox(p.a.produceCut().Aspect)
	srcA, outA := imageAspect(s.path), float64(w)/float64(h)
	if err := pubWriteCropped(s.path, p.snapshot().cropRect(srcA, outA), srcA, outA, w, h,
		p.a.thumbPlain()); err != nil {
		p.a.logf("thumbnail: %v", err)
		p.a.setStatus("could not use that image — see log")
		return
	}
	// the choice, remembered: ▶ prints the words onto it and draws nothing,
	// until ↻ over the thumbnail says otherwise
	st := p.snapshot()
	st.Own = true
	p.apply(st)
	p.seedThumbTitle() // the first picture to exist takes the title as its line
	p.a.logf(">>> publish: %s is the thumbnail, as it is — no model, no GPU, and ▶ will not redraw it",
		filepath.Base(s.path))
	p.recomposite() // the title and the marked words go on, as on a drawn one
	p.a.updateGates()
	p.a.setStatus("thumbnail taken from " + filepath.Base(s.path) +
		" — the words are printed on it; ↻ draws over it")
}

func (s *pubSlot) build() gtk.Widgetter {
	pic := gtk.NewPicture()
	pic.SetCanShrink(true)
	// Big enough to judge by. At thumbnail size every frame of a session looks
	// like every other one, which is exactly the mistake the choice is
	// supposed to avoid.
	pic.SetSizeRequest(-1, 200)
	pic.SetFilename(s.path)
	// the base wears the thumbnail's own frame, draggable (publish_crop.go)
	pf := videoFrame(s.cropOverlay(pic))

	// Which file it is, on the row of buttons under the picture rather than on a
	// heading over it; the role word ("Base", "Ref 2") is the tooltip's -- the
	// row already shows it (the base has no "Make base" button).
	name := gtk.NewLabel(strings.TrimSuffix(filepath.Base(s.path), filepath.Ext(s.path)))
	name.SetXAlign(0)
	name.SetHExpand(true)
	name.SetEllipsize(pango.EllipsizeMiddle)
	name.AddCSSClass("dim-label")
	name.SetTooltipText(s.path + "\n\nThe picture being edited — the instruction changes this one")
	if s.i > 0 {
		name.SetTooltipText(fmt.Sprintf("%s\n\nA reference: unchanged, and there to be named. "+
			"The instruction calls this one \"the %s image\"", s.path, ordinal(s.i+1)))
	}

	row := gtk.NewBox(gtk.OrientationHorizontal, 4)
	row.Append(name)
	if s.i > 0 {
		mk := gtk.NewButtonWithLabel("Make base")
		mk.AddCSSClass("flat")
		mk.SetTooltipText("Edit this one instead, and demote the current base to a reference")
		mk.ConnectClicked(func() { s.p.setFrames(moveToFront(s.p.frames, s.i)) })
		row.Append(mk)
	}
	use := gtk.NewButtonWithLabel("Set Thumbnail")
	use.AddCSSClass("flat")
	use.SetTooltipText("Put this picture on the thumbnail as it is -- cropped to the video's " +
		"shape, no model and no GPU. The title and any marked words are printed onto it, and " +
		"↻ over the thumbnail draws over it again whenever you want the model back.")
	use.ConnectClicked(func() { s.useAsThumbnail() })
	row.Append(use)

	change := gtk.NewButtonWithLabel("Change…")
	change.AddCSSClass("flat")
	change.SetTooltipText("Swap this image for another, keeping its place in the row")
	change.ConnectClicked(func() { s.choose() })
	row.Append(change)

	drop := gtk.NewButtonFromIconName("list-remove-symbolic")
	drop.AddCSSClass("flat")
	drop.SetTooltipText("Remove this image from the row")
	drop.SetHAlign(gtk.AlignEnd)
	drop.ConnectClicked(func() {
		s.p.setFrames(append(append([]string(nil), s.p.frames[:s.i]...), s.p.frames[s.i+1:]...))
	})
	row.Append(drop)

	box := gtk.NewBox(gtk.OrientationVertical, 2)
	box.Append(pf)
	box.Append(row)
	return box
}

// ordinal is for the tooltip that tells the user what to call an image in the
// instruction. Only ever asked for small numbers -- the row holds maxPubFrames.
func ordinal(n int) string {
	names := []string{"", "first", "second", "third", "fourth", "fifth", "sixth", "seventh", "eighth"}
	if n < len(names) {
		return names[n]
	}
	return fmt.Sprintf("%dth", n)
}

// choose swaps this slot's image, keeping its position -- so changing the base
// leaves it the base, and changing a reference does not renumber the others.
func (s *pubSlot) choose() {
	s.p.pickImage(fmt.Sprintf("Image %d", s.i+1), filepath.Dir(s.path), func(path string) {
		fs := append([]string(nil), s.p.frames...)
		if s.i < len(fs) {
			fs[s.i] = path
		}
		s.p.setFrames(fs)
	})
}

// pickImage opens the file chooser. An empty start folder means the one Prepare
// extracted frames into, which is the only place worth opening on by default.
func (p *publisher) pickImage(title, start string, done func(string)) {
	a := p.a
	if start == "" || !exists(start) {
		start = filepath.Join(a.inputsDir(), "frames")
		if vids, _ := a.snappedSources(); len(vids) > 0 {
			if d := filepath.Join(start, baseName(vids[0])); exists(d) {
				start = d
			}
		}
	}
	a.pickFile(title, start, extFilter("Images", "jpg", "jpeg", "png", "webp"), done)
}

// ---- reading and writing the page ------------------------------------------------

// snapshot is the page as a runner will see it, taken on the GUI thread. Same
// rule as snapSources: a goroutine never touches a widget, and a value copied
// out before the run is a value the run cannot see change under it.
func (p *publisher) snapshot() pubSettings {
	st := pubSettings{Frames: append([]string(nil), p.frames...), Crop: p.crop,
		Own: p.own, TitleBox: p.titleBox, ThumbTitle: p.thumbTitle,
		TitleSeeded: p.titleSeeded, Texts: append([]pubText(nil), p.texts...)}
	st.Title = strings.TrimSpace(p.title.Text())
	st.Prompt = strings.TrimSpace(viewText(p.prompt))
	st.Negative = strings.TrimSpace(viewText(p.neg))
	st.Desc = strings.TrimSpace(viewText(p.desc))
	return st
}

// apply is snapshot's inverse: what a run wrote, or what a project holds, put
// back on the page.
func (p *publisher) apply(st pubSettings) {
	p.crop, p.own, p.titleBox = st.Crop, st.Own, st.TitleBox
	p.thumbTitle, p.titleSeeded = st.ThumbTitle, st.TitleSeeded
	p.setFrames(st.Frames)
	// quiet: the title entry re-prints the words when TYPED in, and this is
	// not typing -- a run or a project load is putting back words that are
	// already on the picture (or about to be printed by the run itself)
	p.quiet = true
	p.title.SetText(st.Title)
	p.quiet = false
	p.texts = append([]pubText(nil), st.Texts...)
	if p.shotOver != nil {
		p.shotOver.QueueDraw()
	}
	setViewText(p.prompt, st.Prompt)
	setViewText(p.neg, st.Negative)
	setViewText(p.desc, st.Desc)
}

func viewText(tv *gtk.TextView) string {
	b := tv.Buffer()
	return b.Text(b.StartIter(), b.EndIter(), false)
}

func setViewText(tv *gtk.TextView, s string) { tv.Buffer().SetText(s) }

// currentPublish is what the project stores, with the frames made relative to
// root so a moved naivepost folder keeps its thumbnail.
func (a *App) currentPublish() *pubSettings {
	if a.pub == nil {
		return nil
	}
	st := a.pub.snapshot()
	for i, f := range st.Frames {
		st.Frames[i] = a.storePath(f)
	}
	// nothing chosen and nothing written is not worth a key in the file
	if len(st.Frames) == 0 && st.Crop == nil && len(st.Texts) == 0 && !st.Own &&
		st.TitleBox == nil && st.Title == "" && st.Prompt == "" && st.Desc == "" {
		return nil
	}
	return &st
}

func (a *App) applyPublish(st *pubSettings) {
	if a.pub == nil {
		return
	}
	if st == nil {
		a.pub.apply(pubSettings{})
		a.pub.showShot()
		return
	}
	// migrate first, then resolve: an old project's base index points into the
	// order it was saved in, so rotating has to happen before anything else
	// touches the list
	c := st.migrate()
	for i, f := range c.Frames {
		c.Frames[i] = a.loadPath(f)
	}
	a.pub.apply(c)
	a.pub.showShot()
}

// refresh redraws both rows and the picture -- called when the page is entered,
// because everything it reads (the cut, the narration, the output folder) is
// made somewhere else.
func (p *publisher) refresh() {
	if p == nil {
		return
	}
	p.reread()
	// the row is rebuilt, not just re-read: the base wears a crop box of the
	// CUT's shape, and the cut is the thing most likely to have been reshaped
	// since this page was last looked at
	p.rebuildFrames()
	p.updateOut()
	p.showShot()
}

// showShot puts the last thumbnail back in the picture. The file on disk is the
// state, not something remembered in the struct: it survives a restart, and a
// project whose folder was reopened shows what was drawn for it last time.
func (p *publisher) showShot() {
	if p == nil || p.shot == nil {
		return
	}
	// the finished one first, then the plain copy: the plain one is what a
	// run interrupted between the two writes leaves behind, and a picture
	// without its words is still the thumbnail
	for _, f := range []string{p.a.thumbFile(), p.a.thumbPlain()} {
		if exists(f) {
			// the TEXTURE, not the filename. GtkPicture compares the GFile it
			// is given against the one it holds and returns early when they
			// are equal -- so re-printing the words writes thumbnail.png and
			// then shows the copy already decoded, and the box you had just
			// typed into came up empty. Every path here rewrites the same two
			// names, so the name is exactly what cannot be used to notice a
			// change.
			if tex, err := gdk.NewTextureFromFilename(f); err == nil {
				p.shot.SetPaintable(tex)
			} else {
				p.a.logf("thumbnail: %v", err)
				p.shot.SetFilename(f)
			}
			p.shot.SetTooltipText("publish/" + filepath.Base(f))
			// cached for the marking layer: its draw handler and its drag
			// both need to know where the picture is, per pointer move
			p.shotPath = f
			p.shotA = imageAspect(f)
			if p.shotOver != nil {
				p.shotOver.QueueDraw()
			}
			return
		}
	}
	p.shotPath, p.shotA = "", 0
	p.shot.SetPaintable(nil)
	p.shot.SetTooltipText("nothing drawn yet — ▶ below draws it")
	if p.shotOver != nil {
		p.shotOver.QueueDraw()
	}
}

// reread reloads the cut's aspect off disk -- the one thing the drawing side
// still reads from another step's file. Only on arrival and after a run, never
// per frame: the crop box is redrawn on every pointer move.
func (p *publisher) reread() {
	p.aspect = p.a.produceCut().Aspect
}

// updateOut passes the news to the page's one Outputs group: what this half
// writes lands in the same folder the video does.
func (p *publisher) updateOut() {
	if p == nil {
		return
	}
	p.a.prod.updateOut()
}

// ---- choosing the candidate frames -------------------------------------------

// pubShot is one extracted frame and where it falls on the session clock.
type pubShot struct {
	path string
	t    float64
}

// publishShots is every frame Prepare extracted, on the session's clock. The
// times come from the frame's own filename when it has a stamp -- which is
// what they are named for -- and from the video's start plus the interval when
// it does not, which is how folders extracted before the renaming look.
//
// Runner-side: it reads a frame folder per source, so it is not something to
// call from a page refresh.
func (a *App) publishShots() []pubShot {
	vids, _ := a.snappedSources()
	if len(vids) == 0 {
		return nil
	}
	zero := a.sessionZero()
	var out []pubShot
	for _, v := range vids {
		plan, err := a.planVideo(v, a.describeDir())
		if err != nil {
			a.logfIdle("    publish: no frames for %s (%v)", baseName(v), err)
			continue
		}
		start := a.sourceStart(v) // its own stamp, or the session's start
		for i, f := range plan.frames {
			t := start - zero + float64(i)*plan.interval
			if s, _, ok := readStamp(f); ok {
				t = s - zero
			}
			out = append(out, pubShot{path: f, t: t})
		}
	}
	return out
}

// pickShots chooses n candidate frames: frames the cut kept first (a thumbnail
// of an edited-out moment promises a video that does not exist), spread evenly
// as the middles of n equal bands -- the first frame is usually a loading
// screen, the last a fade.
func pickShots(shots []pubShot, segs []cutSeg, n int) []string {
	if n <= 0 || len(shots) == 0 {
		return nil
	}
	pool := shots
	if len(segs) > 0 {
		var kept []pubShot
		for _, s := range shots {
			for _, c := range segs {
				// an insert covers session seconds without showing them, so a
				// frame under one is not in the video at all -- picking a
				// thumbnail from it would offer a shot no viewer ever sees
				if !c.isInsert() && s.t >= c.S && s.t <= c.E {
					kept = append(kept, s)
					break
				}
			}
		}
		// only if the cut actually leaves enough to choose between: a cut
		// whose clips fall between two frames would otherwise reduce the
		// candidates to the same one repeated
		if len(kept) >= n {
			pool = kept
		}
	}
	var out []string
	for i := 0; i < n; i++ {
		j := int(float64(len(pool)) * (float64(i) + 0.5) / float64(n))
		if j >= len(pool) {
			j = len(pool) - 1
		}
		out = append(out, pool[j].path)
	}
	return out
}

// ---- what the model is told about the video -------------------------------------

// publishBrief is the video as a paragraph of facts: how long it is, what its
// clips contain, and what the narration says over them. It is the same clip
// briefing the narration was written from (clipBriefs), plus the narration
// itself -- which is the tightest description of the finished video that
// exists, because it was written to be spoken over exactly these clips.
func (a *App) publishBrief(segs []cutSeg, entries []narrEntry) string {
	rows := a.sessionRows()
	total := 0.0
	for _, s := range segs {
		total += s.length()
	}
	var b strings.Builder
	fmt.Fprintf(&b, "THE FINISHED VIDEO: %d clips, %d:%02d long.\n",
		len(segs), int(total)/60, int(total)%60)
	// each clip headed by where it starts in the FINISHED video: the chapter
	// list is written on that clock, and it is arithmetic the model would
	// otherwise have to do from the clip lengths -- which a small one does not,
	// and writes session times instead
	b.WriteString("\nWHAT IS IN EACH CLIP:\n")
	pos := 0.0
	b.WriteString(clipBriefsWith(segs, rows, nil, a.narratorMic(), func(i int, s cutSeg) string {
		h := fmt.Sprintf("CLIP %d (at %d:%02d in the video, %.0f s): session %.1f–%.1f",
			i+1, int(pos)/60, int(pos)%60, s.length(), s.S, s.E)
		pos += s.length()
		return h
	}))
	said := 0
	var n strings.Builder
	// running time, not session time: the description's chapter marks are
	// counted from the start of the finished video, and this is the only place
	// that arithmetic is available
	at := 0.0
	for i, s := range segs {
		for _, e := range entries {
			if e.S != s.S || e.E != s.E || strings.TrimSpace(e.Text) == "" {
				continue
			}
			fmt.Fprintf(&n, "  [%d:%02d] (clip %d) %s\n",
				int(at+e.At)/60, int(at+e.At)%60, i+1, e.Text)
			said++
		}
		at += s.length()
	}
	if said > 0 {
		b.WriteString("\nTHE NARRATION SPOKEN OVER IT, at its time in the finished video:\n")
		b.WriteString(n.String())
	} else {
		b.WriteString("\n(no narration has been written for this video)\n")
	}
	return b.String()
}

// ---- the model job ----------------------------------------------------------

// writeUpload asks for the title, the edit instruction and the description in
// one reply: labelled lines in front of the prose, no JSON, so an unescaped
// quote cannot throw a good reply away. The instruction is a suggestion in an
// editable box; picking the base frame stays the user's.
func (a *App) writeUpload(brief string) (title, instr, desc string, err error) {
	title, instr, _, desc, err = a.writeUploadAt(brief, nil)
	return title, instr, desc, err
}

// writeUploadAt is writeUpload with the thumbnail's own second: where the user
// context asks for a picture the video already contains, the answer names the
// moment instead of describing one to draw (youtubeSystem). -1 for no moment.
func (a *App) writeUploadAt(brief string, segs []cutSeg) (title, instr string, at float64, desc string, err error) {
	msgs := []map[string]any{
		msg("system", a.sysPrompt("youtube")),
		msg("user", a.ctxBlockFor("youtube")+brief),
	}
	if err := a.checkpoint(); err != nil {
		return "", "", -1, "", err
	}
	tools, ffx := a.webToolsFor("publish") // the description may name what the game is
	reply, err := a.llmChatRetryTools("publish", msgs, true, tools, a.webRunner("publish", ffx), nil)
	if err != nil {
		return "", "", -1, "", err
	}
	title, instr, at, desc = splitUploadAt(reply, segs)
	if desc == "" {
		return "", "", -1, "", fmt.Errorf("the model answered with nothing")
	}
	return title, instr, at, desc, nil
}

// splitUpload peels the labelled lines off the front of the reply: the title
// and the picture instruction, in either order. Missing either is not an error
// -- an empty box is easier to notice than a wrong line. The rest goes through
// cleanDescription.
func splitUpload(reply string) (title, instr, desc string) {
	title, instr, _, desc = splitUploadAt(reply, nil)
	return title, instr, desc
}

// splitUploadAt is splitUpload with the thumbnail's own second: the moment the
// picture is to be taken FROM, where the answer names one rather than
// describing a picture to draw (youtubeSystem). -1 when it does not.
//
// segs are the clips the brief was written from, and they are here because a
// frame is named the way the brief stamps everything -- a clip and an offset
// inside it -- and turning that into a session second is arithmetic the editor
// does rather than the model (frameSecs).
func splitUploadAt(reply string, segs []cutSeg) (title, instr string, at float64, desc string) {
	// A fenced reply puts the fence before the labelled lines, so it has to come
	// off here rather than in cleanDescription: by the time they are peeled the
	// text no longer *starts* with a fence, and the closing one would be left
	// sitting at the bottom of the description.
	s := unfence(strings.TrimSpace(reply))
	at = -1
	for i := 0; i < 3; i++ {
		if v, rest, ok := peelLabel(s, "title:"); ok {
			title, s = v, rest
			continue
		}
		if v, rest, ok := peelLabel(s, "thumbnail:"); ok {
			instr, s = v, rest
			continue
		}
		if v, rest, ok := peelLabel(s, "frame:"); ok {
			s = rest
			// "frame: clip 3 +12", and a line that names no second at all is
			// an answer that meant to leave it out
			at = frameSecs(v, segs)
			continue
		}
		break
	}
	// A frame FOLDED INTO the thumbnail line -- "THUMBNAIL: frame: 10" -- is a
	// frame. The shape the job is given has one thumbnail line, so a model that
	// decides to name a second writes it there rather than inventing a fourth
	// line, and that is the answer this app most wants: it happened on a
	// session whose context said "pick the title-slide frame, do not generate",
	// and "frame: 10" went to sd.cpp as an edit instruction, which came back
	// generation_failed with no thumbnail at all.
	// The colon is what tells the two apart: an instruction that merely opens
	// with the word -- "Frame the lecturer against the slide" -- is an
	// instruction and stays one.
	if instr != "" {
		if v, _, ok := peelLabel(instr, "frame:"); ok {
			// whether or not a second can be read off it: "frame: clip 9 +4"
			// names a clip the cut does not have, and the one thing it
			// certainly is not is an edit instruction. Sending it as one is
			// what drew nothing at all.
			instr = ""
			if at < 0 {
				at = frameSecs(v, segs)
			}
		}
	}
	return title, instr, at, cleanDescription(s)
}

// frameSecs is the session second a frame line names. Two spellings, and the
// first is the one the job asks for:
//
//	clip 3 +12   the clip the brief numbered, and the offset stamped on the
//	             EVENT line inside it -- both COPIED off the block the answer
//	             was read from. The addition is done here.
//	12 or 0:12   a plain session second, for an answer that gives one anyway.
//
// The clip form exists because the upload brief stamps every line as an offset
// from its clip ([+10s]) and the session second the editor takes a frame by
// appears nowhere in it: asked for a session second, a model answered
// "frame: 10" off a line stamped [+10s] in a clip that starts at session 1.9,
// and the thumbnail came out of the wrong moment. A clip further in would have
// been wrong by minutes.
//
// -1 for a line with no number in it, a clip the cut does not have, or a
// second before the video starts -- each of which means "no frame was named"
// rather than "take the first one".
func frameSecs(v string, segs []cutSeg) float64 {
	v = strings.TrimSpace(strings.Trim(strings.TrimSpace(v), `"“”`))
	m := frameClipRe.FindStringSubmatch(v)
	if m == nil {
		return labelSecs(v)
	}
	n, err := strconv.Atoi(m[1])
	if err != nil || n < 1 || n > len(segs) {
		return -1
	}
	// no offset is the clip's own start: "the title slide is in clip 3" is an
	// answer, and that clip's first frame is the picture it means
	off := 0.0
	if rest := strings.TrimSpace(m[2]); rest != "" {
		if _, err := fmt.Sscanf(rest, "%f", &off); err != nil {
			return -1
		}
	}
	// a line can be stamped just OUTSIDE its clip ([-2s] is something said into
	// the clip before it), so what is checked is the sum and not the offset
	if t := segs[n-1].S + off; t >= 0 {
		return t
	}
	return -1
}

// frameClipRe is "clip 3 +12", with whatever a model puts between the two
// numbers -- a comma, "at", "+", nothing -- and an "s" after the second.
var frameClipRe = regexp.MustCompile(`^(?i:clip)\s*#?\s*([0-9]+)\s*(?:at\b)?[,:]?\s*([-+]?[0-9]*\.?[0-9]*)\s*s?$`)

// labelSecs reads a second off a labelled line: a plain number, or mm:ss as
// the timeline writes one. -1 when there is no number in it.
func labelSecs(v string) float64 {
	v = strings.TrimSpace(strings.Trim(strings.TrimSpace(v), "\"'"))
	if v == "" {
		return -1
	}
	if m, rest, ok := strings.Cut(v, ":"); ok {
		var mm, ss float64
		if _, err := fmt.Sscanf(strings.TrimSpace(m), "%f", &mm); err != nil {
			return -1
		}
		if _, err := fmt.Sscanf(strings.TrimSpace(rest), "%f", &ss); err != nil {
			return -1
		}
		return mm*60 + ss
	}
	var f float64
	if _, err := fmt.Sscanf(v, "%f", &f); err != nil {
		return -1
	}
	if f < 0 {
		return -1
	}
	return f
}

// peelLabel takes "NAME: value" off the front when the first line carries it,
// and says whether it did. The quotes come off the value: a model asked for a
// title on a labelled line very often gives it to you in quotation marks, and
// those would be printed onto the picture.
func peelLabel(s, name string) (val, rest string, ok bool) {
	line := s
	if i := strings.IndexByte(s, '\n'); i >= 0 {
		line = s[:i]
	}
	t := strings.TrimSpace(line)
	if len(t) <= len(name) || !strings.EqualFold(t[:len(name)], name) {
		return "", s, false
	}
	val = strings.Trim(strings.TrimSpace(t[len(name):]), `"“”`)
	return val, strings.TrimSpace(strings.TrimPrefix(s, line)), true
}

// unfence takes a ```-fenced block down to what is inside it. A chat model
// wraps prose it was asked for bare often enough that both readers of a reply
// need this, and a fence left in the box is a fence in the YouTube description.
func unfence(s string) string {
	if !strings.HasPrefix(s, "```") {
		return s
	}
	if i := strings.IndexByte(s, '\n'); i >= 0 {
		s = s[i+1:]
	}
	if i := strings.LastIndex(s, "```"); i >= 0 {
		s = s[:i]
	}
	return strings.TrimSpace(s)
}

// cleanDescription strips the wrapping a chat model puts around prose it was
// asked for bare: a fenced block, and a "Description:" style lead-in on the
// first line. What is left is what goes in the box, verbatim -- the text is
// the product here, so nothing else is touched.
func cleanDescription(reply string) string {
	s := unfence(strings.TrimSpace(reply))
	if line, rest, ok := strings.Cut(s, "\n"); ok {
		l := strings.TrimSpace(line)
		if len(l) < 40 && strings.HasSuffix(l, ":") {
			s = strings.TrimSpace(rest)
		}
	}
	return strings.TrimSpace(s)
}

// ---- drawing the picture ----------------------------------------------------------

// ---- the run --------------------------------------------------------------------

// publishRedraw draws the thumbnail again and nothing else: the picture half
// of publishStage with needText false, the same path ▶ takes.
func (a *App) publishRedraw() {
	if a.busy() {
		return
	}
	p := a.pub
	if p == nil {
		return
	}
	segs := a.produceSegs()
	if len(segs) == 0 {
		a.setStatus("no cut yet — the thumbnail is drawn from the cut's own frames")
		return
	}
	// read on this thread, like every other run: the goroutine must not go
	// looking at widgets or at the editor
	// ↻ is the one thing that means "draw over this", so it is the one thing
	// that takes the picture back off a chosen frame
	st := p.snapshot()
	st.Own = false
	p.apply(st)
	entries := a.produceEntries()
	aspect := a.produceCut().Aspect
	written := a.publishRecorded()
	a.saveProjectNow()

	a.startRun()
	a.logf(">>> publish: drawing the thumbnail again — one sd.cpp call, nothing rewritten")
	a.qJob(trackSTT, "publish", 0, 0)
	a.prog(trackSTT, 0, "drawing")
	a.pulseUntilCounted()

	go func() {
		var failed error
		defer func() { a.publishDone("thumbnail drawn", failed) }()
		failed = a.publishStage(trackSTT, st, aspect, segs, entries, false, written, false, true)
	}()
}

// publishSuggest is "Suggest again": one LLM call rewrites the title, the
// thumbnail instruction and the description; nothing drawn or rendered. It is
// the only thing that rewrites them.
func (a *App) publishSuggest() {
	if a.busy() {
		return
	}
	p := a.pub
	if p == nil {
		return
	}
	segs := a.produceSegs()
	if len(segs) == 0 {
		a.setStatus("no cut yet — build one on the Cut step first")
		return
	}
	st := p.snapshot()
	entries := a.produceEntries()
	// the cut's shape, read once on this thread: the goroutine below must not
	// go reading the editor for it
	aspect := a.produceCut().Aspect
	written := a.publishRecorded()
	a.saveProjectNow() // the run is a moment worth a file, whatever the ticker is doing

	a.startRun()
	a.logf(">>> publish: rewriting the title, instruction and description — one LLM call")
	a.qJob(trackSTT, "publish", 0, 0)
	a.prog(trackSTT, 0, "thinking")
	a.pulseUntilCounted()

	go func() {
		var failed error
		defer func() { a.publishDone("title, instruction and description rewritten", failed) }()
		failed = a.publishStage(trackSTT, st, aspect, segs, entries, true, written, true, true)
	}()
}

// drawStamp is what a drawn thumbnail is made of: images, instruction,
// negative, crop and shape. The printed words are not in it.
func (a *App) drawStamp(st pubSettings, aspect string) string {
	var b strings.Builder
	for _, f := range st.Frames {
		fmt.Fprintf(&b, "%s %s\n", f, fileMark(f))
	}
	cx, cy := 0.5, 0.5
	if st.Crop != nil {
		cx, cy = st.Crop.X, st.Crop.Y
	}
	fmt.Fprintf(&b, "%s\n%s\n%g %g %s %v", st.Prompt, st.Negative, cx, cy, aspect, st.Own)
	sum := sha1.Sum([]byte(b.String()))
	return hex.EncodeToString(sum[:])
}

func (a *App) drawStampFile() string { return filepath.Join(a.publishDir(), "thumbnail.stamp") }

// drawStale is whether ▶ has a thumbnail to draw: none on disk, or one drawn
// from something other than what the page holds now. ↻ over the picture never
// asks -- an image model asked twice answers differently, which is the whole
// reason that button exists.
func (a *App) drawStale(st pubSettings, aspect string) bool {
	if !exists(a.thumbFile()) || !exists(a.thumbPlain()) {
		return true
	}
	b, err := os.ReadFile(a.drawStampFile())
	return err != nil || strings.TrimSpace(string(b)) != a.drawStamp(st, aspect)
}

// publishStage is the writing-and-drawing half of a run: fill the image row the
// first time, write the text (once per project -- the gate is the record on
// disk, not the boxes) and land it before anything is drawn, then draw. textOnly
// stops before the drawing (Suggest again); force draws even when unchanged.
func (a *App) publishStage(track int, st pubSettings, aspect string, segs []cutSeg,
	entries []narrEntry, needText, written, textOnly, force bool) error {
	// ...and ask anyway when there is NOTHING to make a thumbnail from: no
	// picture drawn or chosen, no instruction, no frames in the row.
	//
	// The gate the caller passes is about the text -- what is written once is
	// not rewritten, because a title you emptied is a deletion you made. But
	// it is publish.json that records "written", and that file is laid down
	// before the thumbnail is attempted: a run whose draw failed leaves the
	// page with the gate closed and nothing to draw, and every ▶ after it dies
	// on "nothing to tell the image model" without ever asking the one job
	// that would name a frame or describe a picture. That is where this
	// session ended up, with a context that says which frame to use.
	if !needText && !textOnly && !st.Own && strings.TrimSpace(st.Prompt) == "" &&
		len(st.Frames) == 0 && !exists(a.thumbFile()) {
		a.logfIdle("    publish: no picture, no images and no instruction — asking for the upload text again")
		needText = true
	}
	if needText {
		brief := a.publishBrief(segs, entries)
		a.logCtx("publish")
		a.prog(track, 0, "writing the title, the instruction and the description")
		title, instr, at, desc, err := a.writeUploadAt(brief, segs)
		if err != nil {
			return err
		}
		// ...and where the answer named a MOMENT rather than a picture to
		// draw, that frame becomes the thumbnail as it is: cropped to the
		// shape, with the title printed on it and nothing generated (Own).
		// The context is what asks for this -- "use the frame that shows the
		// title slide" -- and a frame out of the video cannot promise
		// something the video does not contain.
		if at >= 0 {
			if err := a.takeFrameAt(&st, at, aspect); err != nil {
				a.logfIdle("    publish: %v -- the thumbnail is drawn instead", err)
			}
		}
		// a reply that forgot one of its labelled lines still has a good
		// description in it, and an empty box is easier to notice than a
		// wrong line -- so a missing part leaves what was there rather than
		// clearing it
		if title != "" {
			st.Title = title
		}
		if instr != "" {
			st.Prompt = instr
		}
		st.Desc = desc
		a.logfIdle("    publish: title %q, instruction %d characters, description %d characters",
			st.Title, len(st.Prompt), len(desc))
		a.landPublish(st)
	}
	// A starting image on the very first run so the row is not empty; the first
	// is simply the base. Not on a redraw: a row the user emptied is a decision.
	//
	// AFTER the text, not before it: an answer that named a frame has already
	// put the one picture it means in the row (takeFrameAt), and candidates
	// chosen for an image model that is not going to run are three pictures
	// the page would have to explain.
	if len(st.Frames) == 0 && !written && !st.Own {
		a.logfIdle("    publish: no images chosen — taking %d from the cut", defPubFrames)
		if st.Frames = pickShots(a.publishShots(), segs, defPubFrames); len(st.Frames) > 0 {
			a.landPublish(st)
		} else {
			a.logfIdle("    publish: no frames extracted either — drawing from the instruction alone")
		}
	}
	if err := a.writePublishFiles(st); err != nil {
		a.logfIdle("    publish: %v", err) // the text is on the page either way
	}
	// The first thumbnail to exist takes the video's title as the line printed
	// on it, and nothing seeds it twice (seedThumbTitle says the same thing
	// for the pictures chosen on the page). After this the two are separate:
	// a title reworded later leaves the picture alone, and the picture's own
	// ✎ leaves the upload alone.
	if !st.TitleSeeded && strings.TrimSpace(st.Title) != "" {
		st.ThumbTitle, st.TitleSeeded = strings.TrimSpace(st.Title), true
		a.landPublish(st)
	}
	if st.Own {
		// the thumbnail is a picture that was chosen, and choosing it was the
		// answer. The words still go on it: they are printed locally from the
		// plain copy, which is what that picture now is.
		//
		// Before the textOnly gate, and deliberately: Suggest again is how an
		// answer that names a frame is asked for, and printing the title onto
		// the frame it just chose is not drawing -- it is a PNG encode. Behind
		// the gate, that button returned before the line above had given the
		// picture its words, so a frame it had just chosen came up with no
		// title printed on it (publishDone re-prints, from a ThumbTitle that
		// was never seeded).
		a.logfIdle("    publish: the thumbnail is a chosen picture — not redrawing it (↻ over it draws)")
		return a.printPubWords(st)
	}
	if textOnly {
		return nil
	}
	// ...and a picture that is already this picture is not drawn again. ▶ is
	// pressed to make the upload, not to spend a GPU on a thumbnail nothing
	// has changed about; ↻ over it is the button that means "another one".
	if !force && !a.drawStale(st, aspect) {
		a.logfIdle("    publish: the thumbnail is already drawn from these images and this instruction — printing the words onto it")
		return a.printPubWords(st)
	}
	a.prog(track, 0.5, "drawing the thumbnail")
	if err := a.drawThumbnail(st, aspect); err != nil {
		return err
	}
	if s := a.drawStamp(st, aspect); s != "" {
		if err := os.WriteFile(a.drawStampFile(), []byte(s+"\n"), 0o644); err != nil {
			a.logfIdle("    publish: could not write the thumbnail stamp (%v)", err)
		}
	}
	return nil
}

// takeFrameAt makes the extracted frame nearest session second t the
// thumbnail, as it is: cropped to the aspect, never drawn over. The same thing
// pressing "use as thumbnail" on the page does (useAsThumbnail), from the
// runner rather than the hand.
func (a *App) takeFrameAt(st *pubSettings, t float64, aspect string) error {
	shots := a.publishShots()
	if len(shots) == 0 {
		return fmt.Errorf("no frames were extracted, so there is none to take")
	}
	best := shots[0]
	for _, s := range shots {
		if math.Abs(s.t-t) < math.Abs(best.t-t) {
			best = s
		}
	}
	if err := os.MkdirAll(a.publishDir(), 0o755); err != nil {
		return err
	}
	w, h := pubBox(aspect)
	srcA, outA := imageAspect(best.path), float64(w)/float64(h)
	if err := pubWriteCropped(best.path, st.cropRect(srcA, outA), srcA, outA, w, h, a.thumbPlain()); err != nil {
		return err
	}
	// the row becomes that frame and NOTHING else. It exists to be handed to
	// the image model, and here nothing is drawn: references beside a picture
	// that was chosen are pictures with no job, and a page showing four when
	// one was asked for reads as four candidates rather than as the answer.
	// It is also what ↻ over the thumbnail draws FROM, which is the one thing
	// the row is still for.
	st.Frames = []string{a.storePath(best.path)}
	st.Own = true
	st.Prompt = "" // nothing is drawn, so there is no instruction to be stale
	a.logfIdle("    publish: the thumbnail is the frame at %s, as it is — no model, no GPU", mmss(best.t))
	return nil
}

// landPublish puts a stage's result on the page from the runner's goroutine and
// waits for it to be there. Waiting is what makes the next stage's snapshot
// consistent with the screen -- and, more to the point, what guarantees the
// user sees a written title even if the drawing then fails.
func (a *App) landPublish(st pubSettings) {
	done := make(chan struct{})
	glib.IdleAdd(func() {
		if a.pub != nil {
			a.pub.apply(st)
		}
		close(done)
	})
	<-done
}

// writePublishFiles puts the text in the output folder as well as in the
// project. The project file is the state; these are what you copy out of when
// the upload page is open in the other window.
func (a *App) writePublishFiles(st pubSettings) error {
	dir := a.publishDir()
	if err := os.MkdirAll(dir, 0o755); err != nil {
		return err
	}
	b, err := json.MarshalIndent(st, "", "  ")
	if err != nil {
		return err
	}
	if err := os.WriteFile(filepath.Join(dir, "publish.json"), append(b, '\n'), 0o644); err != nil {
		return err
	}
	if st.Desc != "" {
		if err := os.WriteFile(filepath.Join(dir, "description.txt"),
			[]byte(st.Desc+"\n"), 0o644); err != nil {
			return err
		}
	}
	return nil
}

// pubNoLettering rides at the end of every instruction: the words are printed
// on locally after the draw (drawPubTexts), so a model that letters anything
// -- and asked for a thumbnail, an image model letters something -- puts its
// words underneath ours. The calm-top half is for the title's band: a busy
// top edge is a title nobody can read.
const pubNoLettering = "Do not write any words, letters, titles, logos or " +
	"captions into the picture. Keep the %s part of the picture calm and " +
	"uncluttered: a title will be printed across it afterwards."

// editInstruction is what the image model is actually sent: the edit the
// instruction box describes, plus the no-lettering sentence. The title is NOT
// in it any more -- it is printed onto the picture afterwards, with the
// marked texts (publish_text.go), which is what lets retyping it cost a PNG
// encode instead of a GPU run.
func editInstruction(st pubSettings) string {
	edit := strings.TrimSpace(st.Prompt)
	if edit == "" {
		return ""
	}
	// where the band actually is, not always "upper": the title can be dragged
	// now, and asking the model to keep clear a part of the frame the words no
	// longer land in gets the wrong picture twice over
	return edit + "\n\n" + fmt.Sprintf(pubNoLettering, pubTitleWhere(st.titleBox()))
}

// drawThumbnail hands sd.cpp every frame in the row and the instruction, writes
// the result as thumbnail-plain.png and prints the words onto thumbnail.png
// (drawPubTexts). aspect decides the frame drawn into and the crop handed in
// (publish_crop.go): asking a model to choose the crop goes badly.
func (a *App) drawThumbnail(st pubSettings, aspect string) error {
	// An empty row is allowed: with no references this is plain text-to-image,
	// which is what a session with nothing worth editing actually wants. What
	// is not allowed is a base that has been deleted since it was chosen --
	// silently drawing from the second image instead would be a thumbnail of
	// the wrong moment, and nothing on the page would say so.
	base := st.basePath()
	if base != "" && !exists(base) {
		return fmt.Errorf("the base image is gone: %s", base)
	}
	instr := editInstruction(st)
	if strings.TrimSpace(instr) == "" {
		return fmt.Errorf("nothing to tell the image model — write an edit instruction first (▶ suggests one)")
	}
	dir := a.publishDir()
	if err := os.MkdirAll(dir, 0o755); err != nil {
		return err
	}

	w, h := pubBox(aspect)
	outA := float64(w) / float64(h)

	// In row order, which IS the answer: the first is the picture being edited,
	// the rest are what "the second image" refers to. A missing reference is
	// skipped. The crop applies to the FIRST alone.
	var imgs []string
	cropped := ""
	for i, f := range st.Frames {
		if !exists(f) {
			a.logfIdle("    publish: reference %d is gone, drawing without it: %s", i+1, f)
			continue
		}
		var u string
		var err error
		if i == 0 {
			srcA := imageAspect(f)
			r := st.cropRect(srcA, outA)
			if !pubWholeFrame(r, srcA, outA) {
				cropped = fmt.Sprintf(", base cropped to %.0f%% of its width around %.2f,%.2f",
					100*pubCropW(r.hf, srcA, outA), r.cx, r.cy)
			}
			u, err = pubCropRefImage(f, r, srcA, outA)
		} else {
			u, err = sdRefImage(f)
		}
		if err != nil {
			return fmt.Errorf("image %s: %w", filepath.Base(f), err)
		}
		imgs = append(imgs, u)
	}

	req := sdRequest{
		Prompt:        instr,
		Negative:      st.Negative,
		Width:         w,
		Height:        h,
		Seed:          -1, // a fresh draw every time ▶ is pressed, which is the point
		RefImages:     imgs,
		AutoResizeRef: true,
		Format:        "png",
	}
	if base == "" {
		a.logfIdle("    publish: %dx%d drawn from the instruction alone, no images", w, h)
	} else {
		a.logfIdle("    publish: %dx%d editing %s, %d image(s) sent%s",
			w, h, filepath.Base(base), len(imgs), cropped)
	}
	img, err := a.sdGenerate(a.runCtx, req, func(where string) {
		a.prog(trackSTT, 0.5, "drawing (%s)", where)
	})
	if err != nil {
		return err
	}
	if err := os.WriteFile(a.thumbPlain(), img, 0o644); err != nil {
		return err
	}
	// the words go on last, locally: the title across the top band, each
	// marked text filling its box. A print that fails still has a good
	// picture in hand, and a thumbnail without its words beats no thumbnail.
	if err := drawPubTexts(a.thumbPlain(), a.thumbFile(), st.Texts, st.printedTitle(), st.titleBox()); err != nil {
		a.logfIdle("    publish: printing the words failed (%v) — the plain picture stands", err)
		return os.WriteFile(a.thumbFile(), img, 0o644)
	}
	return nil
}

// printPubWords prints the title and the marked texts onto the plain copy,
// which is what a run does after a draw and what it does INSTEAD of a draw
// when the thumbnail is a picture that was chosen (pubSettings.Own). Nothing
// to print onto is not a failure: the picture is chosen on the page and a run
// that got here before one was is simply early.
func (a *App) printPubWords(st pubSettings) error {
	plain := a.thumbPlain()
	if !exists(plain) {
		// nothing drawn or chosen yet. Not a failure and not worth a line:
		// this runs on every pause in typing a title, and a log that said so
		// each time would be a log about an empty page.
		return nil
	}
	return drawPubTexts(plain, a.thumbFile(), st.Texts, st.printedTitle(), st.titleBox())
}

// The two files the thumbnail is, named once. They were spelled out at nine
// call sites between two files, which is nine chances to write the plain one
// where the finished one was meant -- and the two differ only by the words on
// them, so getting it wrong shows up as a thumbnail that has quietly lost its
// title rather than as anything failing.
func (a *App) thumbPlain() string { return filepath.Join(a.publishDir(), "thumbnail-plain.png") }
func (a *App) thumbFile() string  { return filepath.Join(a.publishDir(), "thumbnail.png") }

// pubJPEGMax is the largest thumbnail YouTube accepts. The export drops
// quality until it fits rather than handing back a file the upload refuses
// with a number nobody sees.
const pubJPEGMax = 2 << 20

// exportThumb writes the thumbnail as a JPEG wherever the user says.
//
// Out of the app rather than into it: publish/thumbnail.png is the working
// copy and stays a PNG, because every reworded title re-prints it from the
// plain picture and a generation of JPEG per edit is a thumbnail that gets
// worse the more it is worked on. This is the one conversion, on the way out.
func (p *publisher) exportThumb() {
	a := p.a
	src := a.thumbFile()
	if !exists(src) {
		a.setStatus("nothing to export yet — draw a thumbnail, or use one of the images")
		return
	}
	name := strings.TrimSuffix(filepath.Base(a.projPath), filepath.Ext(a.projPath)) + "-thumbnail.jpg"
	a.saveAs("Export the thumbnail", filepath.Dir(a.projPath), name, extFilter("JPEG", "jpg", "jpeg"), func(out string) {
		if e := strings.ToLower(filepath.Ext(out)); e != ".jpg" && e != ".jpeg" {
			out += ".jpg" // a JPEG named .png is a file every uploader argues about
		}
		n, err := writeJPEGUnder(src, out, pubJPEGMax)
		if err != nil {
			a.logf("!!! export: %v", err)
			a.setStatus("could not export the thumbnail — see log")
			return
		}
		a.logf(">>> exported %s (%s)", out, humanSize(n))
		a.setStatus(fmt.Sprintf("exported %s — %s", filepath.Base(out), humanSize(n)))
	})
}

// writeJPEGUnder encodes src as a JPEG at out, dropping quality until the file
// is at most max bytes, and answers how big it came out. Quality first, never
// scale: 1280x720 is already the size wanted. Stops at 40.
func writeJPEGUnder(src, out string, max int64) (int64, error) {
	img, err := pubDecode(src)
	if err != nil {
		return 0, err
	}
	var buf bytes.Buffer
	for _, q := range []int{92, 85, 75, 60, 40} {
		buf.Reset()
		if err := jpeg.Encode(&buf, img, &jpeg.Options{Quality: q}); err != nil {
			return 0, err
		}
		if int64(buf.Len()) <= max {
			break
		}
	}
	if err := os.WriteFile(out, buf.Bytes(), 0o644); err != nil {
		return 0, err
	}
	return int64(buf.Len()), nil
}

// publishDone ends either of the two runs this page starts, and says which one
// finished: they land in the same place and leave the page in the same state,
// but "rewritten" over a redraw is a line that lies about what just happened.
func (a *App) publishDone(what string, err error) {
	glib.IdleAdd(func() {
		a.endRun()
		if p := a.pub; p != nil {
			p.refresh()
		}
		a.updateGates()
		if err != nil {
			if !errors.Is(err, errStopped) {
				a.logf("%s FAILED: %v", what, err)
				a.setStatus(what + " failed — see log")
				return
			}
			a.setStatus(what + " stopped")
			return
		}
		// the fresh title is words on the picture too: re-print it onto the
		// last drawn thumbnail, so the suggestion is visible where it will
		// land and not only in the entry. No-op when nothing is drawn yet.
		if p := a.pub; p != nil {
			p.recomposite()
		}
		a.progress.SetFraction(1)
		a.setStatus(what + " — ▶ renders the video")
	})
}
