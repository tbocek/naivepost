package main

// The board stopped being a picture this package prints and became a picture
// this package fills in. These are the four things that has to mean, or the
// change bought nothing: the file decides what a board looks like, the file
// carries the schedule in numbers a person can read, a place with nothing in it
// is switched off rather than drawn empty, and a file that explains itself in
// comments is not taken literally.

import (
	"os"
	"path/filepath"
	"regexp"
	"strings"
	"testing"
)

// The point of the whole exercise: restyle the file, and the boards drawn from
// it come out restyled. Before this, a card in assets was an output -- you could
// edit it all you liked and the next render threw it away.
func TestABoardIsDrawnFromTheFileAndNotFromTheProgram(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "tier.svg")
	// two edits an actual person would make: a different strip behind the places,
	// and rounder corners on the plate each letter sits on
	edited := strings.NewReplacer(
		`fill="#20242b"`, `fill="#101820"`,
		`width="190" height="124" rx="12"`, `width="190" height="124" rx="40"`,
	).Replace(tierTemplate)
	if err := os.WriteFile(path, []byte(edited), 0o644); err != nil {
		t.Fatal(err)
	}
	out, note, err := insSVG(path + "?S=Dust II&A=Nuke")
	if err != nil {
		t.Fatalf("the edited board does not draw (%q): %v", note, err)
	}
	got := string(out)
	// six tiers, so six strips and six plates, all of them the file's own
	if n := strings.Count(got, `fill="#101820"`); n != len(tierLetters) {
		t.Errorf("the strip is the colour the file says in %d rows of %d:\n%s",
			n, len(tierLetters), got)
	}
	if strings.Contains(got, `fill="#20242b"`) {
		t.Error("the built-in colour came back, so the drawing is still in the program")
	}
	if n := strings.Count(got, `rx="40"`); n != len(tierLetters) {
		t.Errorf("%d plates have the corners the file gives them, want %d", n, len(tierLetters))
	}
	// and it is still a board: the parameters went into the file that was there
	if !strings.Contains(got, ">Dust II<") || !strings.Contains(got, ">Nuke<") {
		t.Errorf("the edited file drew something that is not the board asked for:\n%s", got)
	}
}

// A schedule that only exists as fractions of the card's length is a schedule
// nobody can read. What somebody typed as D[1.1s] is in the document as 1.1,
// beside the animation it belongs to -- and changing the file changes when the
// thing arrives, because the timing is filled into the file rather than printed
// around it.
func TestTheMomentSomethingArrivesIsWrittenInTheFile(t *testing.T) {
	src := tierSVG(parseSVGQuery("S=Dust II&new=D[1.1s]: Nuke"), "", nil)
	got := string(src)
	if !strings.Contains(got, `data-at="1.1"`) {
		t.Errorf("the arrival's own moment is nowhere in the board:\n%s", got)
	}
	// the fraction beside it is that moment over the card's whole length, which
	// is what SMIL wants and what a reader cannot do in their head
	root := mustParse(t, src)
	chip := chipOf(t, root, "Nuke")
	if visible(chip, 1.05) {
		t.Error("the chip is on the board before the moment written on it")
	}
	if got := landing(t, root, chip); got < 1.1 {
		t.Errorf("the chip lands at %gs, before it was told to start", got)
	}
}

// The board is thirty six places whether or not anybody filled them in, so the
// question is not whether an empty one is drawn but whether it is switched off.
// display is the answer, and it is the file's answer: the fallback in the file
// is off and the card switches on the places it has something for.
func TestAPlaceNobodyFilledInIsSwitchedOff(t *testing.T) {
	// the file explains its own holes in comments, and a comment is not the
	// picture: what is drawn is everything outside them
	plain := string(reComment.ReplaceAll(
		tierSVG(parseSVGQuery("S=Dust II, Mirage&A=Nuke"), "", nil), nil))
	if n := strings.Count(plain, `display="inline"`); n != 3 {
		t.Errorf("%d things on the board are switched on, and three were asked for:\n%s", n, plain)
	}
	if strings.Contains(plain, "{{") {
		t.Errorf("a hole was left showing in the drawn board:\n%s", plain)
	}
	// the pictures are in the file too, one per place, and a board of names has
	// no picture on it
	if strings.Contains(plain, `<image display="inline"`) {
		t.Error("a board with no logos in it is showing a picture element")
	}
	// the heading is written out the same way: it is there, and it says nothing
	head := regexp.MustCompile(`y="120"[^>]*>([^<]*)</text>`).FindStringSubmatch(plain)
	if head == nil {
		t.Errorf("the heading has gone from the board entirely:\n%s", plain)
	} else if head[1] != "" {
		t.Errorf("a board with no title is headed %q", head[1])
	}
	titled := string(tierSVG(parseSVGQuery("title=Maps&S=Dust II"), "", nil))
	if !strings.Contains(titled, ">Maps<") {
		t.Error("a board with a title lost its heading")
	}
}

// A card explains itself in comments, and the built-in board explains what a
// {{hole}} is by writing one. If that counted, the dialog would offer a box for
// the example and a path could fill in the documentation.
func TestAHoleInACommentIsNotAParameter(t *testing.T) {
	doc := []byte(`<svg xmlns="http://www.w3.org/2000/svg">` +
		"\n  <!-- put {{example}} where the words go -->" +
		"\n  <!-- Input: line | Line | the words -->" +
		"\n  <text>{{line}}</text>\n</svg>\n")
	f := declaredFields(doc, nil)
	if len(f) != 1 || f[0].Key != "line" {
		t.Fatalf("the dialog asks for %v, want the one real box", formKeys(f))
	}
	out := string(svgFill(doc, parseSVGQuery("example=gone&line=Hello")))
	if !strings.Contains(out, "{{example}}") {
		t.Error("the comment's example was answered from the path")
	}
	if !strings.Contains(out, ">Hello<") {
		t.Errorf("the real placeholder was not filled in:\n%s", out)
	}
	// the built-in board is the reason this matters: its own explanation of
	// {{holes}} must not turn into boxes
	for _, f := range tierForm(nil, []byte(tierTemplate)) {
		if strings.EqualFold(f.Key, "name") || strings.Contains(f.Key, ".") {
			t.Errorf("the board asks for %q, which is something it says about itself", f.Key)
		}
	}
}

// A stamped file with no holes left in it is a board this app drew earlier, or
// one somebody flattened in an editor. There is nothing to fill in there, so it
// is a picture -- and the board asked for is drawn from the built-in file rather
// than handed back as whatever that document happened to be.
func TestAFlattenedBoardIsDrawnFromTheBuiltInOne(t *testing.T) {
	flat := []byte(`<svg data-naivepost="tier" data-naivepost-args=""><rect/></svg>`)
	if tierIsTmpl(flat) {
		t.Error("a document with no holes in it was taken for a template")
	}
	if !tierIsTmpl([]byte(tierTemplate)) {
		t.Error("the built-in board is not recognised as the thing boards are drawn from")
	}
	out := string(reComment.ReplaceAll(tierSVG(parseSVGQuery("S=Dust II"), "", flat), nil))
	if !strings.Contains(out, ">Dust II<") || strings.Contains(out, "{{") {
		t.Errorf("a board with nothing left to fill in drew:\n%s", out)
	}
}
