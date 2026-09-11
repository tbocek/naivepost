package main

import (
	"bytes"
	"encoding/json"
	"fmt"
	"image"
	"image/color"
	"image/jpeg"
	"image/png"
	"os"
	"path/filepath"
	"reflect"
	"regexp"
	"strings"
	"testing"
)

// The title does not travel in the instruction any more -- it is printed
// onto the picture after the draw (drawPubTexts), with the marked texts. What
// the instruction has to carry instead is the no-lettering sentence: an image
// model asked for a thumbnail letters something unless told not to, and its
// words would sit underneath ours. History: the title used to be appended
// here as its own "Write the exact text" sentence, and an instruction that
// already named the title got it lettered twice.
func TestTheInstructionAsksForNoLetteringInsteadOfTheTitle(t *testing.T) {
	edit := "brighten the character and blur the background"

	got := editInstruction(pubSettings{Prompt: edit, Title: "Ghost Ship Disaster"})
	if !strings.Contains(got, edit) {
		t.Error("the edit instruction was lost")
	}
	if strings.Contains(got, "Ghost Ship Disaster") {
		t.Errorf("the title is still sent to the image model, so it is lettered twice:\n%s", got)
	}
	if !strings.Contains(got, "Do not write any words, letters, titles, logos or captions") {
		t.Error("the instruction no longer forbids lettering -- the model's words end up under the printed title")
	}
	if !strings.Contains(got, "\n\n") {
		t.Error("the no-lettering sentence runs on from the edit instead of standing on its own")
	}
	// the sentence has to hold the title's band open too, or the print lands
	// on a busy edge -- and it has to name the band the title is ACTUALLY in,
	// since that band can be dragged (pubSettings.TitleBox)
	if !strings.Contains(got, "Keep the upper part of the picture calm") {
		t.Errorf("the default band is not asked for by name:\n%s", got)
	}
	moved := editInstruction(pubSettings{Prompt: edit,
		TitleBox: &pubText{Cx: 0.5, Cy: 0.85, Wf: 0.9, Hf: 0.2}})
	if !strings.Contains(moved, "Keep the lower part of the picture calm") {
		t.Errorf("a title dragged to the bottom still asks for a calm top:\n%s", moved)
	}

	// an empty instruction is empty, not the boilerplate on its own: a model
	// with nothing to do but "don't letter" draws boilerplate, and the caller
	// treats "" as the stop-and-say-so case
	if got := editInstruction(pubSettings{Title: "Ghost Ship Disaster"}); got != "" {
		t.Errorf("a title with no edit conjured an instruction: %q", got)
	}
}

// The four candidates have to come from the video the viewer will see. A
// thumbnail painted over a moment the cut removed advertises a video that does
// not exist -- and it is exactly the moments a cut removes (the loading, the
// walking, the dead air) that a naive "every nth frame" pick lands on, because
// there are more of them.
func TestTheCandidateFramesComeFromTheCut(t *testing.T) {
	var shots []pubShot
	for i := 0; i < 100; i++ {
		shots = append(shots, pubShot{path: string(rune('a'+i%26)) + ".jpg", t: float64(i)})
	}
	// the cut keeps two windows, well away from each other
	segs := []cutSeg{{S: 10, E: 20}, {S: 70, E: 80}}
	got := pickShots(shots, segs, 4)
	if len(got) != 4 {
		t.Fatalf("asked for 4 frames, got %d", len(got))
	}
	kept := map[string]bool{}
	for _, s := range shots {
		for _, c := range segs {
			if s.t >= c.S && s.t <= c.E {
				kept[s.path] = true
			}
		}
	}
	for _, f := range got {
		if !kept[f] {
			t.Errorf("picked %s, which the cut removed", f)
		}
	}
	// and they are four different parts of the video, not four seconds of one
	seen := map[string]bool{}
	for _, f := range got {
		if seen[f] {
			t.Errorf("the same frame twice: %v", got)
		}
		seen[f] = true
	}

	// A cut too tight to hold four frames falls back to the whole session
	// rather than handing back one frame repeated: something to choose between
	// beats something faithful to a cut that has nothing in it.
	tight := pickShots(shots, []cutSeg{{S: 3, E: 4}}, 4)
	if len(tight) != 4 {
		t.Fatalf("a tight cut gave %d frames, want 4", len(tight))
	}
	if tight[0] == tight[3] {
		t.Errorf("a tight cut collapsed the candidates to one frame: %v", tight)
	}

	// nothing extracted is not a crash, it is a run that says so
	if got := pickShots(nil, segs, 4); got != nil {
		t.Errorf("pickShots with no frames = %v, want nil", got)
	}
}

// One model call writes the title, the thumbnail instruction and the
// description, and the only thing separating them in the reply is a line the
// model was asked to prefix. It will forget a line, wrap the whole answer in a
// fence, or put the title in quotes -- none of which is worth a failed run.
// What must not happen is a label surviving into the description box, or a
// forgotten line costing the description written underneath it.
func TestTheLabelledLinesArePeeledOffTheDescription(t *testing.T) {
	const body = "We went in for the chest and came out with nothing.\n\n#tarkov #raid"
	const head = "TITLE: The Door Was A Lie\nTHUMBNAIL: a raider holding an empty case\n\n"
	for _, c := range []struct{ name, reply, title, instr, desc string }{
		{"as asked", head + body, "The Door Was A Lie", "a raider holding an empty case", body},
		{"lowercase keys", strings.ToLower(head[:len(head)-2]) + "\n\n" + body,
			"the door was a lie", "a raider holding an empty case", body},
		{"quoted title", "TITLE: \"The Door Was A Lie\"\n\n" + body, "The Door Was A Lie", "", body},
		{"fenced", "```\n" + head + body + "\n```", "The Door Was A Lie", "a raider holding an empty case", body},
		// order is what the prompt asks for, not what it gets: a model that
		// answers the two labels the other way round is still answering
		{"swapped", "THUMBNAIL: a raider holding an empty case\nTITLE: The Door Was A Lie\n\n" + body,
			"The Door Was A Lie", "a raider holding an empty case", body},
		// no lines at all: the description is still the product, so it is kept
		// whole and the other two boxes are simply left as they were
		{"no labels", body, "", "", body},
		// and one label without the other is half an answer, not a failed one
		{"title only", "TITLE: The Door Was A Lie\n\n" + body, "The Door Was A Lie", "", body},
	} {
		title, instr, desc := splitUpload(c.reply)
		if title != c.title || instr != c.instr || desc != c.desc {
			t.Errorf("%s: splitUpload = (%q, %q, %q), want (%q, %q, %q)",
				c.name, title, instr, desc, c.title, c.instr, c.desc)
		}
	}
}

// Nothing the model writes is allowed to empty a box the user filled. A reply
// that forgets a label loses that line and nothing else, which is why the two
// suggestions land conditionally and the description -- always present, since
// whatever is left over IS the description -- lands flat.
func TestAForgottenLabelKeepsWhatWasAlreadyThere(t *testing.T) {
	body := funcBody(t, "publish.go", `func \(a \*App\) publishStage\(`)
	for _, want := range []string{
		`if title != "" {`,
		`if instr != "" {`,
	} {
		if !strings.Contains(body, want) {
			t.Errorf("publishStage is missing %s -- a reply that forgot one label "+
				"would blank a box the user had filled", want)
		}
	}
}

// The description is prose, and prose asked of a chat model comes back with a
// lead-in it was not asked for. What is stripped is only the wrapping: the text
// itself is the product, so nothing inside it is touched.
func TestTheDescriptionIsUnwrappedButNotRewritten(t *testing.T) {
	body := "We went in for the chest and came out with nothing.\n\nIt was a long night.\n\n#tarkov #raid"
	for _, reply := range []string{
		body,
		"```\n" + body + "\n```",
		"```markdown\n" + body + "\n```",
		"Description:\n" + body,
	} {
		if got := cleanDescription(reply); got != body {
			t.Errorf("cleanDescription(%q)\n = %q\nwant %q", reply, got, body)
		}
	}
	// a first line that is genuinely part of the description is not a lead-in,
	// even with a colon on the end -- length is what tells them apart
	keep := "This is the raid where everything that could go wrong did, in order:\n\nand here is the list."
	if got := cleanDescription(keep); got != keep {
		t.Errorf("cleanDescription ate a real first line: %q", got)
	}
}

// Which image is the base is now its position, and a base index arrives from
// two untrusted places -- a project file somebody edited, and a model's reply.
// Applying one must never index off the end of the list, and must never drop or
// duplicate an image: the row is what gets sent to sd.cpp, so a promotion that
// loses a reference loses it silently.
func TestPromotingAnImageIsWhatMakesItTheBase(t *testing.T) {
	fs := []string{"a.jpg", "b.jpg", "c.jpg"}
	for _, c := range []struct {
		i    int
		want string
	}{{0, "a b c"}, {1, "b a c"}, {2, "c a b"}, {-1, "a b c"}, {3, "a b c"}, {99, "a b c"}} {
		got := moveToFront(append([]string(nil), fs...), c.i)
		// compared on stems, so the table above stays readable
		if stems := strings.ReplaceAll(strings.Join(got, " "), ".jpg", ""); stems != c.want {
			t.Errorf("moveToFront(%d) = %q, want %q", c.i, stems, c.want)
		}
		if len(got) != len(fs) {
			t.Errorf("moveToFront(%d) changed the row from %d images to %d", c.i, len(fs), len(got))
		}
	}
	if (pubSettings{}).basePath() != "" {
		t.Error("an empty row reported a base image")
	}
	if got := (pubSettings{Frames: []string{"a.jpg"}}).basePath(); got != "a.jpg" {
		t.Errorf("the first image is the base, got %q", got)
	}

	// and a project written before the row became a list: its base lived in an
	// index beside the frames, and loading it has to apply that index rather
	// than quietly editing whichever image happens to be first
	old := pubSettings{Frames: []string{"a.jpg", "b.jpg"}, Base: 1}
	got := old.migrate()
	if got.basePath() != "b.jpg" {
		t.Errorf("an old project's base was lost: %+v", got)
	}
	if got.Base != 0 {
		t.Error("the retired index survived the migration and can now disagree with the order")
	}
	if len(got.Frames) != 2 {
		t.Errorf("migrating dropped an image: %+v", got.Frames)
	}
}

// What reaches sd.cpp is the row, in the row's order. Both halves matter: the
// prompt tells the model it may borrow "from the second image", which is a lie
// unless every image is actually sent, and "the first image is the one being
// edited" is a lie unless the order survives the trip.
//
// The two missing-file cases are deliberately different. A base that has been
// deleted since it was chosen is fatal -- drawing from the second image instead
// would be a thumbnail of the wrong moment with nothing on the page saying so
// -- while a missing reference is only ever named in passing, and losing the
// mention beats losing the run.
func TestTheWholeRowIsSentInOrder(t *testing.T) {
	body := funcBody(t, "publish.go", `func \(a \*App\) drawThumbnail\(`)
	if !strings.Contains(body, "range st.Frames") {
		t.Error("drawThumbnail no longer walks the row, so references never reach the model")
	}
	if !strings.Contains(body, "RefImages:") {
		t.Error("the images are not sent as ref_images")
	}
	if strings.Contains(body, "InitImage") || strings.Contains(body, "Strength") {
		t.Error("drawThumbnail is still building an img2img request")
	}
	iBase := strings.Index(body, "the base image is gone")
	iSkip := strings.Index(body, "drawing without it")
	if iBase < 0 {
		t.Error("a deleted base image no longer stops the run; the thumbnail would be of another moment")
	}
	if iSkip < 0 {
		t.Error("a deleted reference is no longer skipped, so one stale path kills the whole draw")
	}

	// an empty row is a state the page allows -- Remove works down to zero --
	// so the drawing side must treat it as text-to-image, not as an error
	if strings.Contains(body, `no base frame chosen`) {
		t.Error("an empty row is rejected, but the page lets you empty it")
	}
}

// One printer. The words go on through drawPubTexts and nothing else: not
// ffmpeg's drawtext (gone since the model lettered, and still gone now that
// cairo prints), and not the image model (editInstruction forbids it). Half a
// removal is the worst outcome here -- two printers is the title twice, which
// is the screenshot that started the original removal. The Settings probe is
// checked too: it used to demand the drawtext filter, and an ffmpeg build
// without one nothing uses is not a broken build.
func TestOnlyDrawPubTextsPrintsTheWords(t *testing.T) {
	src := readSrc(t, "publish.go")
	for _, gone := range []string{"drawtext", "title.txt", "Write the exact text"} {
		if strings.Contains(src, gone) {
			t.Errorf("publish.go still refers to %q — a second printer for the words", gone)
		}
	}
	if strings.Contains(readSrc(t, "setup.go"), `"drawtext"`) {
		t.Error("ffFilters still requires drawtext, failing builds over a filter nothing uses")
	}
	// the one printer runs inside the draw, from the plain copy
	body := funcBody(t, "publish.go", `func \(a \*App\) drawThumbnail\(st pubSettings, aspect string\) error \{`)
	if !strings.Contains(body, "drawPubTexts(") {
		t.Error("drawThumbnail never prints the words, so ▶ makes a thumbnail without its title")
	}
}

// Adding and removing is the page's own state, and every path that changes the
// row goes through setFrames -- which is what keeps the widgets, the Inputs
// line and the project from disagreeing about what will be sent.
func TestTheImageRowIsAddableAndRemovable(t *testing.T) {
	src := readSrc(t, "publish.go")
	for _, want := range []string{
		"func (p *publisher) addImage()",      // the + button
		"func (p *publisher) setFrames(",      // the one way the row changes
		"list-remove-symbolic",                // the − on each image
		`gtk.NewButtonWithLabel("Make base")`, // promotion, in place of the old radio
	} {
		if !strings.Contains(src, want) {
			t.Errorf("the image row is missing %s", want)
		}
	}
	if strings.Contains(src, "gtk.NewCheckButtonWithLabel(\"Base\")") {
		t.Error("the base radio is still there; position is what says which image is the base now")
	}
	// the row is capped, and the cap has to be enforced where the row is set
	// rather than only where the button is pressed -- a project file can carry
	// any number of frames
	p := &publisher{a: &App{}} // widget-free: rebuildFrames and the info line both stand down
	long := make([]string, maxPubFrames+7)
	for i := range long {
		long[i] = fmt.Sprintf("f%d.jpg", i)
	}
	p.setFrames(long)
	if len(p.frames) != maxPubFrames {
		t.Errorf("setFrames kept %d images, want the row capped at %d -- a hand-edited "+
			"project would send them all to the vision call", len(p.frames), maxPubFrames)
	}
	// and loading a project has to go through the same cap, not around it
	if !strings.Contains(funcBody(t, "publish.go", `func \(p \*publisher\) apply\(st pubSettings\) \{`),
		"p.setFrames(") {
		t.Error("apply sets the row directly, so a project file skips the cap")
	}
}

// The prompt is the point of the step being editable at all: the user asked for
// a generic prompt they can change, and the registry is what makes an edited one
// stick to the project while an untouched one keeps tracking the built-in.
//
// There used to be two. The second one picked which frame to edit and wrote the
// instruction for it, and it did not work: the picking was guesswork and the
// instruction it wrote was worse than the one the user would have typed. The
// key is gone from the registry rather than renamed, so a project that saved an
// edited copy just keeps a dead key nobody reads.
func TestThePublishPromptIsEditable(t *testing.T) {
	ownConfig(t)
	d := promptDefFor("youtube")
	if d.key != "youtube" || strings.TrimSpace(d.def) == "" {
		t.Fatal(`prompt "youtube" is not in promptDefs`)
	}
	if promptDefFor("thumbnail").def != "" {
		t.Error(`the "thumbnail" prompt is still in the registry -- ` +
			"the job it belonged to is gone, so it is a prompt nobody sends")
	}
	src := readSrc(t, "publish.go")
	// the page sends it; the box that edits it is on Prepare (prepedit.go)
	if !strings.Contains(src, `a.sysPrompt("youtube")`) {
		t.Error(`step6 no longer sends a.sysPrompt("youtube") -- an editable prompt that is not sent is a lie`)
	}
	if strings.Contains(src, "a.promptBar(") || strings.Contains(src, "promptSlot{") {
		t.Error("step6 builds a prompt editor again -- every prompt lives in the one box on Prepare")
	}
	if strings.Contains(src, `"thumbnail"`) {
		t.Error("step6 still names the thumbnail prompt, which no longer exists")
	}
}

// What the one remaining prompt must and must not ask for. It writes three
// things in one reply, so the separators have to be spelled out exactly --
// without the labelled lines splitUpload has nothing to peel and the title
// arrives as the first paragraph of the description. And asking for JSON would
// throw away good prose over one unescaped quote.
func TestThePublishPromptMatchesHowTheAnswerIsUsed(t *testing.T) {
	for _, want := range []string{
		"TITLE: ",     // the separators splitUpload looks for, verbatim
		"THUMBNAIL: ", // ...both of them
		"Four to sev", // a title short enough to be lettered across a thumbnail
		"No JSON",     // which models volunteer even when asked for prose
	} {
		if !strings.Contains(strings.TrimSpace(sysSystem)+"\n\n"+youtubeSystem, want) {
			t.Errorf("the upload-text prompt no longer says %q", want)
		}
	}
	if strings.Contains(youtubeSystem, "strict JSON") {
		t.Error("the upload-text prompt asks for JSON; the answer is prose and is read as prose")
	}
	// The thumbnail line must not ask for lettering. The title is printed onto
	// the picture after the draw, so a model that asks for it in the picture
	// as well puts a second copy underneath the print.
	low := strings.ToLower(strings.TrimSpace(sysSystem) + "\n\n" + youtubeSystem) // the thumbnail's mechanics are the context's
	if !strings.Contains(low, "no text") && !strings.Contains(low, "no lettering") {
		t.Error("the upload-text prompt does not tell the model to keep words out of the " +
			"thumbnail instruction -- the title is added to it afterwards")
	}
	// and it is an instruction for an edit model, not a caption of the frame
	if !strings.Contains(low, "instruction") {
		t.Error("the upload-text prompt does not say the thumbnail line is an instruction")
	}
	// what belonged to the deleted frame-picking job stays deleted: this job
	// writes words, it does not choose an image or set sampler knobs
	for _, gone := range []string{"base_frame", "negative_prompt"} {
		if strings.Contains(youtubeSystem, gone) {
			t.Errorf("the upload-text prompt still asks for %q, which belonged to the deleted job", gone)
		}
	}
}

// One call, three answers, against a local fake server. The split is tested
// above on strings; what this adds is that the call actually returns all three
// -- a reader wired to two of them and dropping the third is the kind of thing
// that only shows up as an empty box.
func TestOneCallAnswersWithAllThreeParts(t *testing.T) {
	a := chatFake(t, 200, `{"choices":[{"message":{"content":`+
		`"TITLE: The Door Was A Lie\nTHUMBNAIL: the raider holding an empty case\n\nWe went in for the chest.\n\n#tarkov"}}]}`)
	title, instr, desc, err := a.writeUpload("THE CLIPS")
	if err != nil {
		t.Fatal(err)
	}
	if title != "The Door Was A Lie" {
		t.Errorf("title = %q", title)
	}
	if instr != "the raider holding an empty case" {
		t.Errorf("instruction = %q", instr)
	}
	if desc != "We went in for the chest.\n\n#tarkov" {
		t.Errorf("description = %q", desc)
	}

	// and a reply with nothing in it is an error rather than three empty boxes:
	// the description is the part with no fallback, so it is what is checked
	a = chatFake(t, 200, `{"choices":[{"message":{"content":"TITLE: A Title\n"}}]}`)
	if _, _, _, err := a.writeUpload("THE CLIPS"); err == nil {
		t.Error("a reply with no description was accepted -- the run would land an empty box " +
			"on the page and call it done")
	}
}

// The run's shape, which is what makes it survivable. The text is landed on the
// page before anything is drawn, so an image server that is down costs the
// picture and not the thinking call already paid for; and what is already
// written is never overwritten by ▶, which is what makes ▶ the button you press
// after rewording the instruction.
func TestPublishWritesTheTextBeforeItDraws(t *testing.T) {
	body := funcBody(t, "publish.go", `func \(a \*App\) publishStage\(`)
	iDesc := strings.Index(body, "a.writeUploadAt(brief, segs)")
	iDraw := strings.Index(body, "drawThumbnail")
	if iDesc < 0 || iDraw < 0 {
		t.Fatalf("publishStage no longer does both: %d %d", iDesc, iDraw)
	}
	if iDesc > iDraw {
		t.Error("the thumbnail is drawn before the text is written -- a failed draw would then " +
			"throw away an LLM call that had already succeeded")
	}
	if !strings.Contains(body, "landPublish") {
		t.Error("nothing lands the written text on the page, so a failed draw would lose it")
	}
	// The model writes this page's text once per project. What makes ▶ safe to
	// press over and over -- with the instruction reworded, with a different base
	// frame -- is that the gate is the record on disk and not the state of the
	// boxes: a title you emptied is a deletion you made, not a gap to refill.
	// The gate is read where ▶ lands, on the GTK thread, and handed in.
	clicked := funcBody(t, "produce.go", `func \(a \*App\) produceRun\(words bool\) \{`)
	if !strings.Contains(clicked, "written := a.publishRecorded()") ||
		!strings.Contains(clicked, "!written, written, false") {
		t.Error("▶ no longer gates the model call on the step6 record; " +
			"it would go back to the LLM on every press")
	}
	// and Suggest again is the opposite: it always rewrites and never draws
	sug := funcBody(t, "publish.go", `func \(a \*App\) publishSuggest\(\) \{`)
	if !strings.Contains(sug, "true, written, true, true)") {
		t.Error("Suggest again no longer forces a rewrite (needText, textOnly)")
	}
	if !strings.Contains(body, "if textOnly {\n\t\treturn nil\n\t}") {
		t.Error("Suggest again no longer stops before drawing")
	}
	// ...with one way out of a dead end. publish.json is written BEFORE the
	// thumbnail is attempted, so a run whose draw failed closes the text gate
	// and leaves nothing to draw from: no picture, no instruction, no frames.
	// Every ▶ after that died on "nothing to tell the image model" without
	// ever asking the job that names the frame -- on a session whose context
	// said which frame to use.
	if !strings.Contains(body, `len(st.Frames) == 0 && !exists(a.thumbFile())`) ||
		!strings.Contains(body, "needText = true") {
		t.Error("a page with nothing to draw from and nothing drawn never asks again")
	}
}

// The drawing and the words are two panes worked on at different times: you
// reword the instruction and press ▶ half a dozen times without touching the
// description, then rewrite the description without redrawing. Since the merge
// they are panes of Produce -- the drawing the whole left, the words above the
// encoder settings on the right -- and the split itself lives in buildProduce.
//
// This is a fact about the source because a GTK page cannot be built without a
// display. What it pins is the assignment: which widget went into which side.
func TestThePageIsSplitBetweenTheDrawingAndTheWords(t *testing.T) {
	body := funcBody(t, "publish.go", `func \(a \*App\) buildPublishPanes\(\)`)
	// left: the images, what to change, what to keep out, the result
	for _, want := range []string{
		"col.Append(p.framesBox)",
		"col.Append(promptBox)",
		"col.Append(negBox)",
		"col.Append(shotFrame)",
	} {
		if !strings.Contains(body, want) {
			t.Errorf("the drawing column is missing %s", want)
		}
	}
	// right top: the words themselves, and only those -- the prompt that
	// writes them is on Prepare
	for _, want := range []string{
		"wrote.Append(p.title)",
		"wrote.Append(descBox)",
	} {
		if !strings.Contains(body, want) {
			t.Errorf("the text column is missing %s", want)
		}
	}
	// and buildProduce is where the panes meet
	prod := funcBody(t, "produce.go", `func \(a \*App\) buildProduce\(\)`)
	if !strings.Contains(prod, "outer := gtk.NewPaned(gtk.OrientationHorizontal)") {
		t.Fatal("the page is no longer two columns")
	}
	iSaid := strings.Index(prod, "right.Append(said)")
	iSet := strings.Index(prod, "right.Append(scroll)")
	if iSaid < 0 || iSet < 0 || iSaid > iSet {
		t.Errorf("the words are no longer above the encoder settings: %d %d", iSaid, iSet)
	}
	// the page is the split and nothing else: the Inputs line belongs to
	// neither pane and rides the shared bottom bar with the Outputs group
	if strings.Contains(prod, "page.Append(inRow)") {
		t.Error("the Inputs row is back on the page, above the split")
	}
	if !strings.Contains(prod, `a.inStack.AddNamed(p.inputs, "produce")`) {
		t.Error("Produce hands no Inputs line to the shared bar")
	}
	// one Outputs group for the whole page: the thumbnail and the upload text
	// are written under produce/ with the video (publishDir), so there is one
	// folder to open and one count to read
	if strings.Contains(prod, "pubOuts") {
		t.Error("the thumbnail half has an Outputs group of its own again")
	}
	if !strings.Contains(readSrc(t, "publish.go"), "filepath.Join(a.produceDir(), \"publish\")") {
		t.Error("publish/ is outside produce/, so the page's one count misses it")
	}
}

// Removing the step6 folder is the one gesture that lets the model write this
// page again. The record therefore has to be a file in that folder and nothing
// else -- a flag in the project would survive the deletion and leave the user
// with no way back short of editing JSON by hand.
func TestDeletingTheFolderIsWhatUnlocksAFreshSuggestion(t *testing.T) {
	a := &App{outDir: t.TempDir()}
	if a.publishRecorded() {
		t.Fatal("an empty output folder already counts as written")
	}
	if err := a.writePublishFiles(pubSettings{Title: "x", Prompt: "y", Desc: "z"}); err != nil {
		t.Fatal(err)
	}
	if !a.publishRecorded() {
		t.Error("the text was written but the run would ask the model for it again")
	}
	// what the user does to start over
	if err := os.RemoveAll(a.publishDir()); err != nil {
		t.Fatal(err)
	}
	if a.publishRecorded() {
		t.Error("the folder is gone but the page still refuses to suggest again")
	}
}

// Everything on the page belongs to the project: the frames chosen, the words
// written, and how far the drawing may travel from the base. A page whose state
// only lives in widgets is a page you have to redo after every restart -- and
// the frames are stored root-relative like every other path, so moving the
// naivepost folder moves the thumbnail with it.
func TestThePublishPageIsSavedWithTheProject(t *testing.T) {
	src := readSrc(t, "project.go")
	for _, want := range []string{
		"Publish *pubSettings",
		"a.currentPublish()",
		"a.applyPublish(p.Publish)",
	} {
		if !strings.Contains(src, want) {
			t.Errorf("project.go is missing %s", want)
		}
	}
	body := funcBody(t, "publish.go", `func \(a \*App\) currentPublish\(\) \*pubSettings \{`)
	if !strings.Contains(body, "a.storePath(f)") {
		t.Error("the frames are stored absolute; moving the project would break them")
	}
	if !strings.Contains(funcBody(t, "publish.go", `func \(a \*App\) applyPublish\(st \*pubSettings\) \{`),
		"a.loadPath(f)") {
		t.Error("stored frames are not resolved back on load")
	}

	// and the words themselves survive the trip, which is what makes ▶ cheap:
	// a reopened project redraws from the instruction it already paid for
	// rather than asking the language model for a new one
	want := pubSettings{Title: "Ghost Ship Disaster", Prompt: "blur the background",
		Negative: "watermark, logo", Frames: []string{"a.jpg", "b.jpg"}}
	b, err := json.Marshal(Project{Publish: &want})
	if err != nil {
		t.Fatal(err)
	}
	var back Project
	if err := json.Unmarshal(b, &back); err != nil {
		t.Fatal(err)
	}
	if back.Publish == nil || !reflect.DeepEqual(*back.Publish, want) {
		t.Errorf("the Publish page did not survive the project file: %+v", back.Publish)
	}
}

// Publish is no longer a tab: its panes live on Produce and its work runs off
// Produce's ▶. Each absence below is a way the old page could quietly come
// back -- as a step, as a stack page, or as a ▶ case -- and the order pin at
// the end is the merge's whole point: one press writes the words, draws the
// picture, and only then starts the long render.
func TestPublishIsFoldedIntoProduce(t *testing.T) {
	if i := stepIndex("publish"); i != -1 {
		t.Errorf("publish is still a tab (index %d) -- it was merged into Produce", i)
	}
	if stepIndex("produce") != len(steps)-1 {
		t.Errorf("produce is at %d of %d; the merged page is the last step",
			stepIndex("produce"), len(steps))
	}
	if strings.Contains(readSrc(t, "main.go"), `"publish"`) {
		t.Error(`main.go still routes something to a "publish" page`)
	}
	play := funcBody(t, "pipeline.go", `func \(a \*App\) playClicked\(\) \{`)
	if strings.Contains(play, `case "publish":`) {
		t.Error("▶ still dispatches to a Publish page that is not there")
	}
	// arriving on Produce reads the thumbnail folder, since the picture on
	// the page is the file on disk rather than something remembered
	show := funcBody(t, "main.go", `func \(a \*App\) showStep\(`)
	if !strings.Contains(show, "a.pub.refresh()") {
		t.Error("arriving on Produce no longer rereads the thumbnail from disk")
	}
	// ...and the page's Inputs row speaks for both halves: the images going
	// to the image model, and whether the first ▶ still owes the text
	ins := funcBody(t, "produce.go", `func \(p \*producer\) updateInputs\(`)
	// ...which on the row itself is the one thing still owed: an upload text
	// nobody has written yet. How many candidate images are on the page is
	// something the page is showing you.
	if !strings.Contains(ins, "if a.pub != nil {") ||
		!strings.Contains(ins, "a.publishRecorded()") ||
		!strings.Contains(ins, `line += " · no upload text"`) {
		t.Error("the merged Inputs row says nothing about the thumbnail half")
	}
	// an edit to the image row is an edit to that line, wherever it came from
	if !strings.Contains(funcBody(t, "publish.go", `func \(p \*publisher\) setFrames\(`),
		"p.a.updateProduceInfo()") {
		t.Error("changing the image row leaves a stale count on the Inputs line")
	}
	// the words and the picture run BESIDE the render, not before it: neither
	// reads a file the other writes, and the render is minutes where they are
	// seconds
	body := funcBody(t, "produce.go", `func \(a \*App\) produceRun\(words bool\) \{`)
	// the model call has no byte count to report, so the bar pulses until the
	// first real part is counted -- without this ▶ looks hung while it thinks
	if !strings.Contains(body, "a.pulseUntilCounted()") {
		t.Error("nothing moves the bar while the model thinks")
	}
	for _, want := range []string{
		"var wg sync.WaitGroup",       // the publish half on its own goroutine...
		"wg.Wait()",                   // ...and both finish before the run says so
		"a.publishStage(trackFrames,", // on the other half of the bar, or the two fight over the needle
		"a.produce(segs, entries, st, vids, auds)",
	} {
		if !strings.Contains(body, want) {
			t.Errorf("▶ no longer runs the publish stage beside the render -- missing %q", want)
		}
	}
	// the render owns the fraction and the publish half owns its own line: the
	// needle is the thing that takes minutes
	if !strings.Contains(body, `a.qJob(trackSTT, "render", 0, 0)`) ||
		!strings.Contains(body, `a.qJob(trackFrames, "publish", 0, 0)`) {
		t.Error("the two halves do not each own a track of the bar")
	}
	if !strings.Contains(body, "a.qDone(trackFrames, 0)") {
		t.Error("the publish half claims part of the needle, which belongs to the render")
	}
	// a failed title does not throw away a finished encode: they are separate
	// deliverables and the expensive one is the video
	if !strings.Contains(body, "the render carries on") {
		t.Error("a failure in the words still costs the render")
	}
	if !strings.Contains(body, "if err == nil && pubErr != nil {") {
		t.Error("a failure in the words is not reported at all once the render succeeds")
	}
}

// One mark for "make me another one of these", wherever the app offers it: the
// narration re-rolls a line with it, the voice picker re-draws a sample with
// it, and the thumbnail is drawn again with it.
func TestRegeneratingWearsTheOneMark(t *testing.T) {
	const mark = `gtk.NewButtonFromIconName("view-refresh-symbolic")`
	for file, what := range map[string]string{
		"narrate.go":       "the line re-roll",
		"narrate_voice.go": "the voice sample re-roll",
	} {
		if !strings.Contains(readSrc(t, file), mark) {
			t.Errorf("%s (%s) no longer wears the regenerate mark", file, what)
		}
	}
	pub := readSrc(t, "publish.go")
	for _, want := range []string{"p.redraw = " + mark, "p.suggest = " + mark} {
		if !strings.Contains(pub, want) {
			t.Errorf("publish.go no longer contains %q", want)
		}
	}
	if strings.Contains(pub, `NewButtonWithLabel("Suggest again")`) {
		t.Error("the suggest button is a label again, beside three icons that mean the same thing")
	}
	if !strings.Contains(pub, `p.a.heading("Thumbnail", "What sd.cpp drew from the images and the instruction above",`) ||
		!strings.Contains(pub, "p.export, p.redraw))") {
		t.Error("the redraw button is not on the thumbnail's heading")
	}
}

// The redraw draws and does nothing else: no model call, no render. ▶ was the
// only way to redraw and it also renders, so trying a second thumbnail cost a
// whole encode.
func TestTheRedrawDrawsAndNothingElse(t *testing.T) {
	body := funcBody(t, "publish.go", `func \(a \*App\) publishRedraw\(\) \{`)
	if !strings.Contains(body, "a.publishStage(trackSTT, st, aspect, segs, entries, false, written, false, true)") {
		t.Errorf("publishRedraw does not run the picture half of publishStage:\n%s", body)
	}
	if strings.Contains(body, "writeUpload") || strings.Contains(body, "a.produce(") {
		t.Error("the redraw writes text or renders the video")
	}
	for _, want := range []string{"st := p.snapshot()", "aspect := a.produceCut().Aspect", "a.produceEntries()"} {
		if !strings.Contains(body, want) {
			t.Errorf("publishRedraw no longer reads %q before its goroutine", want)
		}
	}
	if !strings.Contains(body, "if a.busy() {") {
		t.Error("the redraw can start on top of a run")
	}
	if !strings.Contains(body, `a.publishDone("thumbnail drawn", failed)`) {
		t.Error("the redraw reports somebody else's outcome")
	}
	if !strings.Contains(funcBody(t, "publish.go", `func \(a \*App\) publishSuggest\(\) \{`),
		`a.publishDone("title, instruction and description rewritten", failed)`) {
		t.Error("the suggest run no longer says what it did")
	}
}

// The picture on screen is re-read when the file under it changes. GtkPicture
// compares the GFile it is handed against the one it holds and returns early
// when they are equal -- and every path here rewrites the same two names, so
// the name is exactly what cannot be used to notice a change.
func TestTheShownThumbnailIsReReadWhenItChanges(t *testing.T) {
	body := funcBody(t, "publish.go", `func \(p \*publisher\) showShot\(\) \{`)
	if !strings.Contains(body, "gdk.NewTextureFromFilename(f)") || !strings.Contains(body, "p.shot.SetPaintable(tex)") {
		t.Errorf("showShot hands GtkPicture a filename it may already hold:\n%s", body)
	}
	for _, c := range []struct{ file, fn string }{
		{"publish_text.go", `func \(p \*publisher\) recomposite\(\) \{`},
		{"publish.go", `func \(p \*publisher\) refresh\(\) \{`},
	} {
		if !strings.Contains(funcBody(t, c.file, c.fn), "howShot()") {
			t.Errorf("%s does not go through showShot", c.fn)
		}
	}
}

// A marked box is the Cut page's text box: dashed violet, dragged by its
// border to resize and by its middle to move, with the pointer saying which
// before anything is pressed.
func TestAMarkedBoxIsDraggedLikeTheCutsTextBox(t *testing.T) {
	src := readSrc(t, "publish_text.go")
	out := funcBody(t, "publish_text.go", `func pubBoxOutline\(`)
	for _, want := range []string{"cr.SetSourceRGBA(0.6, 0.55, 0.95, 0.9)", "cr.SetDash([]float64{4, 3}, 0)"} {
		if !strings.Contains(out, want) {
			t.Errorf("the marked box is not drawn like the cut's: missing %q", want)
		}
	}
	if !strings.Contains(readSrc(t, "cut_fxview.go"), "cr.SetDash([]float64{4, 3}, 0)") {
		t.Error("the cut's own box is no longer dashed, so this page is copying nothing")
	}
	for _, want := range []string{"fxEdges(x, y, bx, by, bw, bh)", "resizeFree(snapPointPx(x, fxSnapPx, xs...),", "fxCursorName(h, v, l, t, in)"} {
		if !strings.Contains(src, want) {
			t.Errorf("publish_text.go no longer contains %q", want)
		}
	}
	if !strings.Contains(src, "snapEdgePx(nx, grab.w0, fxSnapPx, xs...)") {
		t.Error("the box does not follow the hand while it is moved")
	}
	if !strings.Contains(src, "ts[i].Cx, ts[i].Cy, ts[i].Wf, ts[i].Hf = b.cx, b.cy, b.wf, b.hf") {
		t.Error("a moved or resized box is never written back")
	}
	if pubBoxMin < pubIconPx {
		t.Errorf("a box may be dragged to %g px, smaller than the ✎ it wears (%g)", pubBoxMin, pubIconPx)
	}
}

// A marked box snaps, as the cut's does: to the picture's own edges and
// middle, to the band the title is printed in, and to every other box.
func TestAMarkedBoxSnapsToTheFrameAndToTheOthers(t *testing.T) {
	src := readSrc(t, "publish_text.go")
	for _, want := range []string{
		"xs = []float64{ox, ox + dw/2, ox + dw}",
		"for i := 0; i < nbox(); i++ {",
		"xs = append(xs, bx, bx+bw/2, bx+bw)",
		"if i == skip {",
		"snapPointPx(x, fxSnapPx, xs...)",
		"snapEdgePx(nx, grab.w0, fxSnapPx, xs...)",
	} {
		if !strings.Contains(src, want) {
			t.Errorf("publish_text.go no longer contains %q", want)
		}
	}
	if !strings.Contains(readSrc(t, "cut_fxview.go"), "const fxSnapPx = 10.0") {
		t.Error("the cut's snap reach moved; this page is sharing a constant that is gone")
	}
	if !strings.Contains(src, "wf: math.Abs(bx1-x0) / dw,") {
		t.Error("a new box commits the raw drag rather than the snapped band")
	}
}

// The ✎ is drawn, not typed: a glyph is the font's idea of a pencil at 13 px,
// and on a machine whose sans-serif has no U+270E it is an empty box -- tofu
// on the only control that rewords a caption.
func TestTheEditMarkIsAPathAndNotAGlyph(t *testing.T) {
	src := readSrc(t, "publish_text.go")
	if strings.Contains(funcBody(t, "publish_text.go", `func \(p \*publisher\) textOverlay\(`), "ShowText") {
		t.Error("the edit mark is a glyph again, which is tofu wherever the font has no pencil")
	}
	if !strings.Contains(src, "drawPencil(cr, ix+pubIconPx/2, iy+pubIconPx/2, pubIconPx*0.62)") {
		t.Error("the chip no longer draws the pencil")
	}
	body := funcBody(t, "publish_text.go", `func drawPencil\(`)
	for _, want := range []string{"cr.Rotate(-math.Pi / 4)", "cr.ClosePath()", "cr.Fill()"} {
		if !strings.Contains(body, want) {
			t.Errorf("drawPencil no longer contains %q", want)
		}
	}
	if !strings.Contains(readSrc(t, "cut_hear.go"), "func drawSpeaker(") {
		t.Error("drawSpeaker is gone; the rule this follows went with it")
	}
}

// A frame can BE the thumbnail: the session usually already contains the
// picture, and the only way to get it there was to make it the base and ask
// sd.cpp to change as little as possible.
func TestAnImageCanBeUsedAsTheThumbnailAsItIs(t *testing.T) {
	src := readSrc(t, "publish.go")
	if !strings.Contains(src, `use := gtk.NewButtonWithLabel("Set Thumbnail")`) ||
		!strings.Contains(src, "s.useAsThumbnail()") {
		t.Error("an image in the row has no way to become the thumbnail as it is")
	}
	body := funcBody(t, "publish.go", `func \(s \*pubSlot\) useAsThumbnail\(\) \{`)
	for _, want := range []string{
		"pubWriteCropped(",
		"p.a.thumbPlain()", // written as the plain copy, by the one name for it
		"p.recomposite()",
		"if p.a.busy() {",
		"st.Own = true",
	} {
		if !strings.Contains(body, want) {
			t.Errorf("useAsThumbnail does not %q:\n%s", want, body)
		}
	}
	for _, gone := range []string{"drawThumbnail", "publishStage", "sdDraw"} {
		if strings.Contains(body, gone) {
			t.Errorf("useAsThumbnail calls %q -- it is meant to draw nothing", gone)
		}
	}
	crop := readSrc(t, "publish_crop.go")
	if strings.Count(crop, "func pubCropImage(") != 1 {
		t.Error("the crop is not in one place")
	}
	for _, fn := range []string{"pubCropRefImage", "pubWriteCropped"} {
		if !strings.Contains(funcBody(t, "publish_crop.go", `func `+fn+`\(`), "pubCropImage(path, r, srcA, outA)") {
			t.Errorf("%s does its own crop instead of the shared one", fn)
		}
	}
}

func readSrc(t *testing.T, file string) string {
	t.Helper()
	b, err := os.ReadFile(file)
	if err != nil {
		t.Fatal(err)
	}
	return string(b)
}

// funcBody is the source of one function, for the wiring that cannot be called
// headless: a GTK page needs a display, but what it is wired to is a fact about
// the text.
func funcBody(t *testing.T, file, head string) string {
	t.Helper()
	re := regexp.MustCompile(`(?s)` + head + `.*?\n}\n`)
	m := re.FindString(readSrc(t, file))
	if m == "" {
		t.Fatalf("%s: no function matching %s", file, head)
	}
	return m
}

// The plain copy is a PNG whatever the frame was, and the size a drawn one is.
//
// Two ways the button did nothing. It copied the source bytes through when no
// crop was needed -- which is most frames -- so a JPEG was written to a file
// called thumbnail-plain.png, cairo refused to read it ("undefined"), the
// words were never printed and thumbnail.png was never rewritten: the picture
// on screen simply did not change. And at source size a capture frame is a
// 14 MB PNG, seven times what YouTube takes for a thumbnail, so the one that
// did land was a file the upload would refuse.
func TestAChosenThumbnailIsAPNGTheSizeADrawnOneIs(t *testing.T) {
	dir := t.TempDir()
	// a JPEG, like every extracted frame, already the video's shape -- the
	// case that used to copy the bytes through
	src := filepath.Join(dir, "frame.jpg")
	img := image.NewRGBA(image.Rect(0, 0, 1920, 1080))
	for y := 0; y < 1080; y++ {
		for x := 0; x < 1920; x++ {
			img.Set(x, y, color.RGBA{uint8(x % 251), uint8(y % 241), 90, 255})
		}
	}
	f, err := os.Create(src)
	if err != nil {
		t.Fatal(err)
	}
	if err := jpeg.Encode(f, img, nil); err != nil {
		t.Fatal(err)
	}
	f.Close()

	w, h := pubBox("16:9")
	srcA, outA := imageAspect(src), float64(w)/float64(h)
	if !pubWholeFrame(pubSettings{}.cropRect(srcA, outA), srcA, outA) {
		t.Fatal("the fixture is not the case that used to copy bytes through")
	}
	out := filepath.Join(dir, "thumbnail-plain.png")
	if err := pubWriteCropped(src, pubSettings{}.cropRect(srcA, outA), srcA, outA, w, h, out); err != nil {
		t.Fatalf("pubWriteCropped: %v", err)
	}
	b, err := os.ReadFile(out)
	if err != nil {
		t.Fatal(err)
	}
	if len(b) < 8 || string(b[1:4]) != "PNG" {
		t.Fatalf("the plain copy is not a PNG: % x", b[:8])
	}
	got, err := png.Decode(bytes.NewReader(b))
	if err != nil {
		t.Fatalf("the plain copy will not decode as a PNG: %v", err)
	}
	if d := got.Bounds(); d.Dx() != w || d.Dy() != h {
		t.Errorf("the plain copy is %dx%d, want the %dx%d a drawn one is", d.Dx(), d.Dy(), w, h)
	}
	// ...and the words print onto it, which never happened while it was a JPEG
	if err := drawPubTexts(out, filepath.Join(dir, "thumbnail.png"), nil, "A Title", pubTitleBox); err != nil {
		t.Errorf("the words will not print onto a chosen thumbnail: %v", err)
	}
	// a picture already inside the box is left alone rather than blown up
	small := image.NewRGBA(image.Rect(0, 0, 40, 30))
	if pubFit(small, w, h) != image.Image(small) {
		t.Error("a picture smaller than the box was resized anyway")
	}
}

// Choosing a picture is the answer to "what should the thumbnail be", so ▶
// stops drawing over it. It used to redraw on the next run, because the run
// draws whenever the record does not say otherwise -- so the choice survived
// exactly until the next press of ▶.
func TestAChosenThumbnailIsNotRedrawnByARun(t *testing.T) {
	blob, err := json.Marshal(pubSettings{Own: true})
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(blob), `"own":true`) {
		t.Errorf("the choice is not written to the project: %s", blob)
	}
	if blob, _ := json.Marshal(pubSettings{}); strings.Contains(string(blob), "own") {
		t.Errorf("a drawn thumbnail writes the key anyway: %s", blob)
	}
	// the run asks before it draws, and prints the words instead
	stage := funcBody(t, "publish.go", `func \(a \*App\) publishStage\(`)
	i, j := strings.Index(stage, "if st.Own {"), strings.Index(stage, "a.drawThumbnail(st, aspect)")
	if i < 0 || j < 0 || i > j {
		t.Errorf("publishStage draws before it asks whether the picture was chosen (%d, %d)", i, j)
	}
	if !strings.Contains(stage, "return a.printPubWords(st)") {
		t.Error("a chosen thumbnail gets no words printed on it")
	}
	// ↻ is the one thing that takes it back
	if !strings.Contains(funcBody(t, "publish.go", `func \(a \*App\) publishRedraw\(\) \{`), "st.Own = false") {
		t.Error("the redraw does not clear the choice, so ↻ would draw and the next run would not")
	}
	// nothing else touches it: rewording a title reprints onto whatever is
	// there, drawn or chosen
	for _, c := range []struct{ file, fn string }{
		{"publish.go", `func \(a \*App\) publishSuggest\(\) \{`},
		{"publish_text.go", `func \(p \*publisher\) recomposite\(\) \{`},
	} {
		if strings.Contains(funcBody(t, c.file, c.fn), ".Own") {
			t.Errorf("%s changes whether the thumbnail was chosen", c.fn)
		}
	}
	// and it round-trips through the page
	if !strings.Contains(funcBody(t, "publish.go", `func \(p \*publisher\) snapshot\(\) pubSettings \{`), "Own: p.own") {
		t.Error("the snapshot drops the choice")
	}
	if !strings.Contains(funcBody(t, "publish.go", `func \(p \*publisher\) apply\(`), "p.crop, p.own, p.titleBox = st.Crop, st.Own, st.TitleBox") {
		t.Error("a project load drops the choice")
	}
}

// The title is a box on the picture like any other: outlined, dragged by its
// border, moved by its middle, and wearing a ✎ that opens its words.
//
// It was printed with none of that -- no outline, no handle, no chip -- so a
// thumbnail showed two blocks of words and the page could reach only one of
// them. The way to move a title was to type it a second time as a marked text
// and leave the band empty, which is what the picture that raised this had in
// it: the same seven words printed twice.
func TestTheTitleIsAReachableBoxLikeAnyOther(t *testing.T) {
	src := readSrc(t, "publish_text.go")
	for _, want := range []string{
		"return len(p.texts) + 1", // the title is the last box...
		"isTitle := func(i int) bool { return hasTitle() && i == len(p.texts) }", // ...while it has words
		"for i := 0; i < nbox(); i++ {",                                          // drawn, snapped against and hit-tested with the rest
		"p.setTitleBox(&pubText{",                                                // a drag on it writes its band
		"p.editTitle()",                                                          // and its ✎ opens the title's words
	} {
		if !strings.Contains(src, want) {
			t.Errorf("publish_text.go no longer contains %q -- the title is unreachable again", want)
		}
	}
	// its words are the PICTURE's: the first thumbnail to exist takes the
	// video's title as its line and the two are separate from then on, so the
	// ✎ here rewords what is printed and leaves the upload's title alone
	body := funcBody(t, "publish_text.go", `func \(p \*publisher\) editTitle\(\) \{`)
	if !strings.Contains(body, "p.setThumbTitle(strings.TrimSpace(s))") {
		t.Errorf("the title's ✎ does not edit the picture's own line:\n%s", body)
	}
	if strings.Contains(body, "p.title.SetText") {
		t.Error("rewording the picture's line still rewrites the video's title")
	}
	// Remove takes the words off the PICTURE and leaves the video its name.
	// It used to clear the entry, which is one press meaning two things -- the
	// thumbnail loses the words and the upload loses its title -- and typing
	// the title again, which you must, printed it again.
	if !strings.Contains(body, `p.setThumbTitle("")`) {
		t.Errorf("Remove on the title leaves the words on the picture:\n%s", body)
	}
	// ...and it stays removed: setting the line counts as answering the
	// question, so nothing seeds it again
	seed := funcBody(t, "publish_text.go", `func \(p \*publisher\) seedThumbTitle\(\) \{`)
	if !strings.Contains(seed, "if p == nil || p.titleSeeded {") {
		t.Error("a removed line comes back the next time a thumbnail is drawn")
	}
	if !strings.Contains(funcBody(t, "publish_text.go", `func \(p \*publisher\) setThumbTitle\(s string\) \{`),
		"p.thumbTitle, p.titleSeeded = s, true") {
		t.Error("setting the picture's line does not count as answering the question")
	}
	// and there is no tick beside the heading offering to put it back: words
	// wanted on the picture after the line is removed are dragged out as a box
	// like any other (textOverlay), which is the same gesture as every other
	// word on that picture
	if strings.Contains(readSrc(t, "publish.go"), "titleOn") {
		t.Error("the Title heading grew a print-on-thumbnail tick again")
	}
	if !strings.Contains(funcBody(t, "publish_text.go", `func \(st pubSettings\) titleBox\(\)`), "return pubTitleBox") {
		t.Error("a project that never moved the title has no default band")
	}

	// the band round-trips through the project
	blob, err := json.Marshal(pubSettings{TitleBox: &pubText{Cx: 0.5, Cy: 0.8, Wf: 0.9, Hf: 0.2}})
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(blob), `"title_box"`) {
		t.Errorf("the title's band is not written to the project: %s", blob)
	}
	if blob, _ := json.Marshal(pubSettings{}); strings.Contains(string(blob), "title_box") {
		t.Errorf("a title in the default band writes the key anyway: %s", blob)
	}
	plain, err := json.Marshal(pubSettings{})
	if err != nil {
		t.Fatal(err)
	}
	var back pubSettings
	if err := json.Unmarshal(plain, &back); err != nil {
		t.Fatal(err)
	}
	if got := back.titleBox(); got != pubTitleBox {
		t.Errorf("a project with no band came back with %+v, want the default", got)
	}
	moved := pubSettings{TitleBox: &pubText{Cx: 0.5, Cy: 0.8, Wf: 0.9, Hf: 0.2}}
	if got := moved.titleBox(); got.cy != 0.8 {
		t.Errorf("a moved band came back at %v", got.cy)
	}

	// ...and the words are printed in the band the project asks for, which is
	// the whole point of moving it
	if !strings.Contains(funcBody(t, "publish_text.go", `func drawPubTexts\(`), "x, y, bw, bh := tb.px(w, h)") {
		t.Error("the title is printed in a fixed band whatever the project says")
	}
	// one function prints the words, and it reads the project's band: the
	// words a keystroke re-prints and the words a run prints have to be the
	// same words in the same boxes
	if !strings.Contains(funcBody(t, "publish.go", `func \(a \*App\) printPubWords\(`), "st.titleBox()") {
		t.Error("the printing uses the default band whatever the project says")
	}
	if !strings.Contains(funcBody(t, "publish_text.go", `func \(p \*publisher\) recomposite\(\) \{`),
		"p.a.printPubWords(p.snapshot())") {
		t.Error("the page prints the words its own way instead of through the one printing")
	}
	// the run's own print, after a draw, reads it too
	if !strings.Contains(funcBody(t, "publish.go", `func \(a \*App\) drawThumbnail\(`), "st.titleBox()") {
		t.Error("a drawn thumbnail prints the title in the default band")
	}
	// and a chosen one goes through the same printing rather than its own
	if !strings.Contains(funcBody(t, "publish.go", `func \(a \*App\) publishStage\(`), "return a.printPubWords(st)") {
		t.Error("a chosen thumbnail prints its words some other way")
	}

	// a box is where words are. A marked one goes when its words go, and the
	// title's goes the same way -- Remove used to take the words off the
	// picture and leave the dashed rectangle and its ✎ sitting over nothing.
	if !strings.Contains(src, `hasTitle := func() bool { return strings.TrimSpace(p.thumbTitle) != "" }`) {
		t.Error("the title's box is drawn whether or not there is a title in it")
	}

	// where a band sits, in the words a model follows
	for _, c := range []struct {
		cy   float64
		want string
	}{{0.14, "upper"}, {0.5, "middle"}, {0.86, "lower"}} {
		if got := pubTitleWhere(fxBox{cx: 0.5, cy: c.cy, wf: 0.9, hf: 0.18}); got != c.want {
			t.Errorf("a band at %.2f is called %q, want %q", c.cy, got, c.want)
		}
	}
}

// The thumbnail leaves the app as a JPEG, and the working copy stays a PNG.
//
// It is re-printed from the plain picture every time a word changes, so a
// generation of JPEG per keystroke would be a thumbnail that got worse the
// more it was worked on. What YouTube takes is a JPEG under 2 MB, so the one
// conversion happens on the way out.
func TestTheThumbnailExportsAsAJPEGThatFits(t *testing.T) {
	dir := t.TempDir()
	src := filepath.Join(dir, "thumbnail.png")
	// noise, so the encoder cannot cheat its way under any limit
	img := image.NewRGBA(image.Rect(0, 0, 1280, 720))
	for y := 0; y < 720; y++ {
		for x := 0; x < 1280; x++ {
			img.Set(x, y, color.RGBA{uint8(x * y % 253), uint8(x % 251), uint8(y % 241), 255})
		}
	}
	f, err := os.Create(src)
	if err != nil {
		t.Fatal(err)
	}
	if err := png.Encode(f, img); err != nil {
		t.Fatal(err)
	}
	f.Close()

	out := filepath.Join(dir, "t.jpg")
	n, err := writeJPEGUnder(src, out, pubJPEGMax)
	if err != nil {
		t.Fatalf("writeJPEGUnder: %v", err)
	}
	if n > pubJPEGMax {
		t.Errorf("the export is %d bytes, over the %d YouTube takes", n, pubJPEGMax)
	}
	b, err := os.ReadFile(out)
	if err != nil {
		t.Fatal(err)
	}
	if len(b) < 3 || b[0] != 0xFF || b[1] != 0xD8 {
		t.Fatalf("the export is not a JPEG: % x", b[:3])
	}
	got, err := jpeg.Decode(bytes.NewReader(b))
	if err != nil {
		t.Fatalf("the export will not decode: %v", err)
	}
	// the size is kept: 1280x720 is what YouTube wants, so shrinking to save
	// bytes would be losing the thing rather than compressing it
	if d := got.Bounds(); d.Dx() != 1280 || d.Dy() != 720 {
		t.Errorf("the export is %dx%d, want the thumbnail's own size", d.Dx(), d.Dy())
	}
	// a limit nothing can meet still writes the best it managed, rather than
	// failing with a file the user cannot see
	small, err := writeJPEGUnder(src, filepath.Join(dir, "tiny.jpg"), 1)
	if err != nil {
		t.Errorf("an impossible limit failed instead of writing the smallest it could: %v", err)
	}
	if small >= n {
		t.Errorf("the smallest it could manage (%d) is no smaller than its best quality (%d)", small, n)
	}

	// the button is beside the ↻, on the thumbnail's own heading
	src2 := readSrc(t, "publish.go")
	if !strings.Contains(src2, `p.export = gtk.NewButtonFromIconName("document-save-symbolic")`) ||
		!strings.Contains(src2, "p.exportThumb()") {
		t.Error("the thumbnail has no export button")
	}
	if !strings.Contains(src2, "p.export, p.redraw))") {
		t.Error("the export button is not on the thumbnail's heading beside the redraw")
	}
	// it exports what is on screen -- the words printed -- not the plain copy
	body := funcBody(t, "publish.go", `func \(p \*publisher\) exportThumb\(\) \{`)
	if !strings.Contains(body, "a.thumbFile()") {
		t.Error("the export takes the picture without its words")
	}
	if !strings.Contains(body, "nothing to export yet") {
		t.Error("exporting before anything is drawn fails silently")
	}
	// and a name without a suffix gets one: a JPEG called .png is a file
	// every uploader argues about
	if !strings.Contains(body, `out += ".jpg"`) {
		t.Error("the export can be written under a name that does not say what it is")
	}
}

// The picture's line and the video's title are one sentence exactly once.
//
// The first thumbnail to exist -- drawn by a run, redrawn by ↻, or chosen off
// the row -- takes the title as the words printed on it. After that they are
// two facts: a thumbnail's line is read at the size of a phone's sidebar and a
// title is read in a list, so improving one is not an instruction to rewrite
// the other. Nothing seeds it twice, which is also what makes a line removed
// on purpose stay removed.
func TestTheThumbnailsLineIsSeededOnceAndThenItsOwn(t *testing.T) {
	// the run seeds it where a thumbnail comes into existence, and only then
	stage := funcBody(t, "publish.go", `func \(a \*App\) publishStage\(`)
	if !strings.Contains(stage, `if !st.TitleSeeded && strings.TrimSpace(st.Title) != "" {`) ||
		!strings.Contains(stage, "st.ThumbTitle, st.TitleSeeded = strings.TrimSpace(st.Title), true") {
		t.Error("a run never gives the first thumbnail the video's title")
	}
	// ...and so does choosing a picture off the row
	if !strings.Contains(funcBody(t, "publish.go", `func \(s \*pubSlot\) useAsThumbnail\(\) \{`), "p.seedThumbTitle()") {
		t.Error("a chosen picture never takes the title as its line")
	}
	// what is printed is the picture's own line, not the entry's
	if !strings.Contains(readSrc(t, "publish_text.go"), "func (st pubSettings) printedTitle() string { return st.ThumbTitle }") {
		t.Error("the picture is printed with the video's title again")
	}
	// both ride the project, and a project from before this reads as already
	// answered -- its picture was printing the entry's words
	blob, err := json.Marshal(pubSettings{Title: "a name", ThumbTitle: "a line", TitleSeeded: true})
	if err != nil {
		t.Fatal(err)
	}
	var back pubSettings
	if err := json.Unmarshal(blob, &back); err != nil {
		t.Fatal(err)
	}
	if back.ThumbTitle != "a line" || back.Title != "a name" || !back.TitleSeeded {
		t.Errorf("the two lines do not round-trip: %+v", back)
	}
	old := pubSettings{Title: "a name"}.migrate()
	if old.ThumbTitle != "a name" || !old.TitleSeeded {
		t.Errorf("a project from before the split reads as %+v, want its title on the picture", old)
	}
	// and a frame line that ended up in the instruction box before the reader
	// could take one: not an instruction, and the image model must never be
	// asked to draw those words (it answered generation_failed when it was)
	if got := (pubSettings{Prompt: "frame: 10"}).migrate().Prompt; got != "" {
		t.Errorf("a stored frame line survives as an edit instruction: %q", got)
	}
	if got := (pubSettings{Prompt: "Frame the lecturer against the slide."}).migrate().Prompt; got == "" {
		t.Error("a real instruction that opens with the word was thrown away")
	}
	off := pubSettings{Title: "a name", TitleOff: true}.migrate()
	if off.ThumbTitle != "" || !off.TitleSeeded || off.TitleOff {
		t.Errorf("a project that had taken the line off reads as %+v, want it still off", off)
	}
}

// A thumbnail can be a picture the video ALREADY CONTAINS. Where the user
// context asks for one -- "use the frame that shows the title slide, do not
// generate" -- the answer names the moment instead of describing a picture to
// draw, and that frame becomes the thumbnail as it is: cropped, with the title
// printed on it locally, nothing generated.
func TestTheThumbnailCanBeAFramePickedOutOfTheVideo(t *testing.T) {
	// The frame is named the way the brief stamps its lines -- a clip and an
	// offset inside it -- and the editor does the addition (frameSecs). It was
	// a session second once, and the model answered "frame: 10" off a line
	// stamped [+10s] in a clip starting at session 1.9: the one number the
	// brief never contains is the one it was asked for.
	segs := []cutSeg{{S: 1.9, E: 32.2}, {S: 39.6, E: 105.5}}
	for _, c := range []struct {
		reply string
		want  float64
	}{
		{"title: A Title\nframe: clip 1 +14\n\nThe description.", 15.9},
		{"title: A Title\nframe: clip 2 +12s\n\nThe description.", 51.6},
		{"title: A Title\nframe: Clip 2, +0.5\n\nThe description.", 40.1},
		{"title: A Title\nframe: clip 2\n\nThe description.", 39.6},   // no offset: the clip's own start
		{"title: A Title\nframe: clip 1 -2s\n\nThe description.", -1}, // a line said into the clip before the first
		{"title: A Title\nframe: clip 9 +4\n\nThe description.", -1},  // a clip the cut does not have
		// a plain session second still reads as one: an answer that gives one
		// anyway is not an answer to throw away
		{"title: A Title\nframe: 42\n\nThe description.", 42},
		{"title: A Title\nframe: 1:23\n\nThe description.", 83},
		{"frame: \"0:07\"\ntitle: A Title\n\nThe description.", 7},
		{"title: A Title\nthumbnail: draw it\n\nThe description.", -1}, // no frame line
		{"title: A Title\nframe: soon\n\nThe description.", -1},        // no number in it
		{"title: A Title\nframe: -3\n\nThe description.", -1},          // nor a second before the video
	} {
		title, _, at, desc := splitUploadAt(c.reply, segs)
		if at != c.want {
			t.Errorf("%q named second %v, want %v", c.reply, at, c.want)
		}
		if title != "A Title" || !strings.Contains(desc, "The description.") {
			t.Errorf("peeling the frame line ate the rest: %q / %q", title, desc)
		}
	}
	// ...and the same second FOLDED INTO the thumbnail line, which is where a
	// model given a three-part shape actually writes it. This one shipped:
	// a session whose context said "pick the title-slide frame, do not
	// generate" answered "THUMBNAIL: frame: 10", the app sent those nine
	// characters to sd.cpp as an edit instruction, and the job came back
	// generation_failed with no thumbnail at all.
	for _, c := range []struct {
		reply string
		at    float64
		instr string
	}{
		{"title: A Title\nthumbnail: frame: clip 2 +12\n\nThe description.", 51.6, ""},
		{"title: A Title\nTHUMBNAIL: frame: 2:05\n\nThe description.", 125, ""},
		// a frame line the cut cannot place is still a frame line: it names
		// no picture to draw, and drawing it is the failure this came from
		{"title: A Title\nthumbnail: frame: clip 9 +4\n\nThe description.", -1, ""},
		// an instruction that merely opens with the word names no second and
		// stays an instruction
		{"title: A Title\nthumbnail: Frame the lecturer against the slide.\n\nThe description.",
			-1, "Frame the lecturer against the slide."},
	} {
		_, instr, at, desc := splitUploadAt(c.reply, segs)
		if at != c.at || instr != c.instr {
			t.Errorf("%q read as second %v instruction %q, want %v / %q", c.reply, at, instr, c.at, c.instr)
		}
		if !strings.Contains(desc, "The description.") {
			t.Errorf("peeling the folded frame line ate the description: %q", desc)
		}
	}
	// naming a frame and describing a picture are alternatives, and the one
	// that names a frame leaves nothing for the image model to be stale about
	body := funcBody(t, "publish.go", `func \(a \*App\) takeFrameAt\(`)
	// the row becomes that ONE picture: it is the thumbnail, not a candidate
	// among the three the cut offered, and nothing is going to be drawn from
	// the others
	for _, want := range []string{"st.Own = true", `st.Prompt = ""`, "pubWriteCropped(", "a.thumbPlain()",
		"st.Frames = []string{a.storePath(best.path)}"} {
		if !strings.Contains(body, want) {
			t.Errorf("takeFrameAt does not %q", want)
		}
	}
	// Own is what already means "chosen, not drawn": the words still go on it,
	// printed locally, and ▶ never redraws it
	stage := funcBody(t, "publish.go", `func \(a \*App\) publishStage\(`)
	// and no candidates are taken from the cut once a frame has been named:
	// the seeding happens AFTER the text for that reason, and skips a chosen
	// picture outright. Three more frames beside it would read as a row of
	// candidates rather than as the answer.
	if i, j := strings.Index(stage, "a.writeUploadAt(brief, segs)"), strings.Index(stage, "pickShots("); i < 0 || j < 0 || i > j {
		t.Error("the candidate row is filled before the text is written, so an answer that " +
			"names a frame lands in a row that already holds three others")
	}
	if !strings.Contains(stage, "if len(st.Frames) == 0 && !written && !st.Own {") {
		t.Error("a chosen thumbnail still gets candidate frames beside it")
	}
	// printing the title onto a CHOSEN picture is not drawing, so it happens
	// on Suggest again too -- that button is how a frame answer is asked for,
	// and returning in front of the seeding left the frame it had just chosen
	// with no title printed on it
	if i, j := strings.Index(stage, "if st.Own {"), strings.Index(stage, "if textOnly {"); i < 0 || j < 0 || i > j {
		t.Error("Suggest again returns before the words are printed onto a picture it just chose")
	}
	if !strings.Contains(stage, "if at >= 0 {") || !strings.Contains(stage, "a.takeFrameAt(&st, at, aspect)") {
		t.Error("a named moment does not become the thumbnail")
	}
	if !strings.Contains(stage, "if st.Own {") || !strings.Contains(stage, "a.printPubWords(st)") {
		t.Error("a chosen thumbnail is redrawn, or loses its words")
	}
	// ...and a video with no frames says so and draws instead of failing
	if !strings.Contains(body, "no frames were extracted") {
		t.Error("a video with no frames takes one anyway")
	}
	// the prompt offers the choice, and says which to prefer
	// on the thumbnail line itself, and spelled there too (syscontext.go): the
	// shape has ONE thumbnail line, and a wording that asks for a fourth line
	// instead is a wording the model reconciles by writing "THUMBNAIL: frame:
	// 10" -- which is how nine characters of instruction reached sd.cpp
	if !strings.Contains(youtubeSystem, `answer it as "THUMBNAIL: frame: clip <n> +<seconds>"`) {
		t.Error("the prompt never mentions naming a frame on the thumbnail line")
	}
	if !strings.Contains(sysSystem, `"frame: clip <n> +<seconds>"`) {
		t.Error("the answer's shape does not allow the thumbnail line to name a second")
	}
	if !strings.Contains(youtubeSystem, "cannot promise something the video does not contain") {
		t.Error("the prompt does not say why a real frame is the better answer")
	}
}
