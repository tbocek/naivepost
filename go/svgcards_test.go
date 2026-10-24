package main

// The cards, and the parameters on the end of an insert path.
//
// Two things can go wrong here and neither one announces itself. The first is
// the path: everything that opens, probes or names an insert has to take the
// "?" off first, and the one that forgets opens a file that is not there --
// which the render logs as "not there any more" and skips, so the card is
// simply missing from the video. The second is the picture: a card is generated
// text, and text that does not parse as XML renders as nothing at all.
//
// So these check both ends: that a parameterised path is still a path, and that
// what comes out of the generators is a document that parses, animates, bakes,
// and says the words it was given.

import (
	"bytes"
	"encoding/base64"
	"encoding/xml"
	"fmt"
	"io"
	"math"
	"os"
	"path/filepath"
	"regexp"
	"strconv"
	"strings"
	"testing"
)

// ---- the path ----------------------------------------------------------------

// The file is everything before the "?" -- and a path with no "?" in it must
// come back untouched, because that is every insert that is not a card.
func TestAPathKeepsItsFileAndItsParametersApart(t *testing.T) {
	for _, c := range []struct {
		in   string
		file string
		args string
	}{
		{"assets/later.mp4", "assets/later.mp4", ""},
		{"assets/tier.svg?S=Dust II&A=Nuke", "assets/tier.svg", "S=Dust II|A=Nuke"},
		{"a/b.svg?", "a/b.svg", ""},
		{"a/b.svg?title=", "a/b.svg", "title="},
		// a value with the separators in it survives, because it was escaped on
		// the way out: a map called "A&B" is not two tiers
		{"c.svg?S=A%26B%3DC", "c.svg", "S=A&B=C"},
	} {
		file, q := insSplit(c.in)
		if file != c.file {
			t.Errorf("%q: file is %q, want %q", c.in, file, c.file)
		}
		var got []string
		for _, p := range q {
			got = append(got, p.Key+"="+p.Val)
		}
		if strings.Join(got, "|") != c.args {
			t.Errorf("%q: parameters are %q, want %q", c.in, strings.Join(got, "|"), c.args)
		}
	}
}

// The path is written into cut.json and read back out of it, so what suffix
// writes has to be what insSplit reads -- including the characters that would
// otherwise be read as more parameters.
func TestParametersSurviveTheRoundTrip(t *testing.T) {
	q := svgQuery{{"S", "Dust II, Mirage"}, {"title", "Best & worst = ?"}, {"A", ""}}
	file, back := insSplit("assets/tier.svg" + q.suffix())
	if file != "assets/tier.svg" {
		t.Fatalf("the file came back as %q", file)
	}
	if len(back) != len(q) {
		t.Fatalf("%d parameters went out and %d came back: %v", len(q), len(back), back)
	}
	for i := range q {
		if back[i] != q[i] {
			t.Errorf("parameter %d came back as %v, want %v", i, back[i], q[i])
		}
	}
	// and no parameters means no "?" at all: a bare one is a file that does not
	// exist, which the render skips silently
	if s := (svgQuery{}).suffix(); s != "" {
		t.Errorf("an empty query writes %q", s)
	}
}

// insKind decides how ffmpeg is asked for a file, and it decides by extension.
// With the parameters left on, ".svg?S=Dust" is not ".svg" and a tier board is
// planned as a still image -- one frame of the animation, held.
func TestTheKindOfAnInsertIgnoresItsParameters(t *testing.T) {
	for _, c := range []struct{ path, want string }{
		{"a/tier.svg?S=Dust II", "svg"},
		{"a/later.mp4?x=1", "video"},
		{"a/card.png?x=1", "still"},
	} {
		if got := insKind(c.path); got != c.want {
			t.Errorf("insKind(%q) = %q, want %q", c.path, got, c.want)
		}
	}
	// the same for the folder a bake is written into: the parameters are not
	// part of the name, and a folder named after them is a folder per wording
	if got := safeStem("a/tier.svg?S=Dust II&A=Nuke"); got != "tier" {
		t.Errorf("safeStem gave %q, want %q", got, "tier")
	}
	if got := insBase("a/tier.svg?S=Dust II"); got != "tier.svg" {
		t.Errorf("insBase gave %q, want %q", got, "tier.svg")
	}
}

// ---- the cards ---------------------------------------------------------------

func parses(t *testing.T, src []byte) {
	t.Helper()
	dec := xml.NewDecoder(bytes.NewReader(src))
	for {
		_, err := dec.Token()
		if err == io.EOF {
			return
		}
		if err != nil {
			t.Fatalf("the card is not well-formed XML: %v\n%s", err, src)
		}
	}
}

// A card is generated text and the text goes straight to librsvg, which says
// nothing useful about a document it cannot parse -- ffmpeg reports it as a
// stream it could not decode. An ampersand in a title is all it takes.
func TestACardIsWellFormedWhateverIsWrittenOnIt(t *testing.T) {
	awkward := "Best & worst <maps> \"ever\" — 100% of 'em"
	tier := tierSVG(svgQuery{{"title", awkward}, {"S", awkward}, {"A", ""}}, "", nil)
	parses(t, tier)
	if !strings.Contains(string(tier), "&amp;") {
		t.Error("the ampersand was not escaped anywhere, so it was dropped instead")
	}
	parses(t, badgeSVG(svgQuery{{"label", "<S>"}, {"item", awkward}}, "", nil))
	// and the stamp holds the parameters, which are joined by "&"
	for _, s := range svgSeeds {
		parses(t, svgCardKinds[s.card].draw(s.def, "", nil))
	}
	parses(t, tierSVG(parseSVGQuery("S=A%26B&title=x"), "", nil))
}

// The board is S A B C D F, always, in that order: six rows in the file, six
// lines in the dialog, and a parameter that is not one of them is not a tier. A
// board whose shape is whatever somebody typed is a board where a typo is a row.
func TestTheBoardIsAlwaysTheSixTiers(t *testing.T) {
	rows := tierRowsOf(parseSVGQuery("title=x&C=Anubis&S=Dust II, Mirage&Meh=Office"))
	if got := formLabels(rows); got != "S,A,B,C,D,F" {
		t.Fatalf("the board is %s, want the six tiers in their own order", got)
	}
	if len(rows[0].items) != 2 || rows[0].items[0].text != "Dust II" {
		t.Errorf("the items did not split on commas: %v", rows[0].items)
	}
	if len(rows[3].items) != 1 || rows[3].items[0].text != "Anubis" {
		t.Errorf("the C row holds %v, want what the C parameter says", rows[3].items)
	}
	for _, r := range rows {
		if r.label == "Meh" {
			t.Error("a parameter that is not a tier was drawn as one")
		}
	}
	// no parameters at all is the empty board, which is what the file in assets
	// shows and what an insert with nothing filled in renders
	if got := formLabels(tierRowsOf(nil)); got != "S,A,B,C,D,F" {
		t.Errorf("the default board is %s", got)
	}
	// a tier holds six things, because there are six places in it. A seventh is
	// not drawn -- and because that is somebody's typing quietly going nowhere,
	// the card says so rather than swallowing it
	seven := parseSVGQuery("S=a, b, c, d, e, f, g")
	if got := len(tierRowsOf(seven)[0].items); got != tierSlots {
		t.Errorf("S holds %d of the seven it was given, want %d", got, tierSlots)
	}
	if note := tierNote(seven); !strings.Contains(note, "S has 7") {
		t.Errorf("nothing is said about the item that fell off the board: %q", note)
	}
	if note := tierNote(parseSVGQuery("new=E[1s]: Nuke")); !strings.Contains(note, "no E") {
		t.Errorf("flying something into a tier that is not there says %q", note)
	}
	if note := tierNote(parseSVGQuery("S=a, b&new=S: b")); note != "" {
		t.Errorf("an ordinary board is being complained about: %q", note)
	}
	// the colours are in the file now, one written into each row, and they are
	// the colours the letters have always had here
	for i, l := range tierLetters {
		if tierColors[i] != tierColor(l, i) {
			t.Errorf("tier %s is %s in the file and %s everywhere else", l, tierColors[i], tierColor(l, i))
		}
		if !strings.Contains(tierTemplate, tierColors[i]) {
			t.Errorf("the board in the file has no %s row in %s", l, tierColors[i])
		}
	}
}

// formLabels is a board as a line: which tiers it has, in order.
func formLabels(rows []tierRow) string {
	var out []string
	for _, r := range rows {
		out = append(out, r.label)
	}
	return strings.Join(out, ",")
}

// The form is what the dialog puts on screen, so this is where the shape has to
// come out right: the title, a line per tier, and last the thing that has just
// landed on one of them. The same eight lines every time.
func TestTheFormAsksForTheSixTiersAndTheArrivalsLast(t *testing.T) {
	f := tierForm(parseSVGQuery("S=Dust II|logos/dust2.png"), nil)
	if want := "title,S,A,B,C,D,F,new"; formKeys(f) != want {
		t.Fatalf("the dialog asks for %s, want %s", formKeys(f), want)
	}
	// a tier's line comes back written the way it was typed, logo and all, or the
	// dialog loses what it was not looking at
	if f[1].Val != "Dust II|logos/dust2.png" {
		t.Errorf("the S line came back as %q", f[1].Val)
	}
	if !f[1].Keep || !f[1].Logo {
		t.Errorf("a tier line is not kept when empty (%v) or offers no logo (%v)", f[1].Keep, f[1].Logo)
	}
	// an empty board asks the same eight things: the dialog is the board, whether
	// or not there is anything on it yet
	if got := formKeys(tierForm(nil, nil)); got != "title,S,A,B,C,D,F,new" {
		t.Errorf("the empty board asks for %s", got)
	}
	// a hole somebody has added to their own copy of the card is a parameter like
	// any other and gets a box; the card's own arithmetic never does
	mine := []byte(strings.Replace(tierTemplate, "{{title}}", "{{title}} {{subtitle}}", 1))
	if got := formKeys(tierForm(nil, mine)); got != "title,S,A,B,C,D,F,new,subtitle" {
		t.Errorf("the edited card asks for %s", got)
	}
}

// The dialog asks the card what to put on screen, with the board as it has been
// edited rather than as it was opened: a box the user cleared has to stay
// cleared, or the file's own demo arrivals come back every time it is opened.
func TestTheCardIsAskedWithTheBoardOnScreen(t *testing.T) {
	dir := t.TempDir()
	if _, err := writeSVGCards(dir); err != nil {
		t.Fatal(err)
	}
	path := filepath.Join(dir, "tier.svg")
	// with the demo the file ships with cleared, which is what making a board of
	// one's own looks like
	f, ok := insForm(path, parseSVGQuery("S=Dust II&new="))
	if !ok {
		t.Fatal("the card would not draw its own form again")
	}
	if want := "title,S,A,B,C,D,F,new"; formKeys(f) != want {
		t.Errorf("the dialog asks for %s, want %s", formKeys(f), want)
	}
	if f[1].Val != "Dust II" {
		t.Errorf("the S line came back as %q rather than what is on screen", f[1].Val)
	}
	if got := f[fieldOf(f, "new")].Val; got != "" {
		t.Errorf("the Just added line came back as %q after being cleared", got)
	}
	// left alone, the file's own parameters are what the dialog opens with
	if g, _ := insForm(path, nil); !strings.Contains(g[fieldOf(g, "new")].Val, "a.svg") {
		t.Errorf("the file's own arrivals are not offered: %q", g[fieldOf(g, "new")].Val)
	}
	// an SVG the app did not draw has the same holes whatever is typed into them,
	// and must not be rebuilt under the user's cursor
	plain := filepath.Join(dir, "mine.svg")
	os.WriteFile(plain, []byte(`<svg xmlns="http://www.w3.org/2000/svg"><text>{{who}}</text></svg>`), 0o644)
	if _, ok := insForm(plain, parseSVGQuery("who=x")); ok {
		t.Error("a document with placeholders claims to draw its own form")
	}
}

// The dialog itself is a display, so this is source-level. What it must not
// have is a free-form "key=value&key=value" line: the boxes are the card's, the
// card is the file, and a line that takes a raw query would be a second way to
// say the same thing that nothing else in the app understands.
func TestTheDialogIsBuiltFromTheCardAndNotFromARawQuery(t *testing.T) {
	b, err := os.ReadFile("cut.go")
	if err != nil {
		t.Fatal(err)
	}
	src := string(b)
	if strings.Contains(src, "parseSVGQuery(extra.Text())") {
		t.Error("the insert dialog still has its free-form More line")
	}
	// and it does not rebuild itself while it is open either: with the board
	// fixed, no answer changes what the other boxes are
	if strings.Contains(src, ".Shape") {
		t.Error("a box still claims to change what the other boxes are")
	}
	for _, c := range []struct{ want, why string }{
		{"insFields(path", "the boxes do not come from the file at all"},
		{"a.pickLogos(&a.win.Window, e)", "a logo has to be typed as a path by hand"},
	} {
		if !strings.Contains(src, c.want) {
			t.Errorf("%s (looked for %s)", c.why, c.want)
		}
	}
}

// The point of the whole feature: the same file, two sessions, two boards.
func TestTheSameCardSaysWhatThePathTellsIt(t *testing.T) {
	dir := t.TempDir()
	if _, err := writeSVGCards(dir); err != nil {
		t.Fatal(err)
	}
	got, note, err := insSVG(filepath.Join(dir, "tier.svg") + "?S=Dust II&A=Nuke&B=&title=Maps&new=")
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(note, "tier") {
		t.Errorf("the log line for a redrawn card is %q", note)
	}
	parses(t, got)
	// every tier is on the board whatever the path says, because the board is the
	// six tiers -- what the path decides is what is in them
	for _, want := range []string{">Dust II<", ">Nuke<", ">Maps<", ">S<", ">A<", ">B<", ">C<", ">D<", ">F<"} {
		if !strings.Contains(string(got), want) {
			t.Errorf("the board does not say %s:\n%s", want, got)
		}
	}
	if strings.Contains(string(got), ">E<") {
		t.Error("the board grew an E tier, which no tier list has ever had")
	}
	// and the tiers nobody filled in are empty rather than absent: thirty six
	// places, two of them switched on
	if n := strings.Count(string(got), `<g display="inline"`); n != 2 {
		t.Errorf("%d places on the board are switched on, and two were filled in", n)
	}
}

// A file the app never drew is nobody's business but its author's. If they left
// {{holes}} in it those are filled; if they did not, the file is rendered as it
// is and the log says the parameters went nowhere -- rather than the app
// deciding it knows better and drawing something else entirely.
func TestAnSVGTheAppDidNotDrawIsFilledInOrLeftAlone(t *testing.T) {
	dir := t.TempDir()
	holes := filepath.Join(dir, "mine.svg")
	os.WriteFile(holes, []byte(`<svg xmlns="http://www.w3.org/2000/svg">`+
		`<text>{{who|nobody}}</text><text>{{what}}</text></svg>`), 0o644)

	// via suffix(), because a bare "&" in a path is where the next parameter
	// starts -- and the value has to come back out escaped for XML as well
	got, _, err := insSVG(holes + svgQuery{{"who", "Tom & Jerry"}}.suffix())
	if err != nil {
		t.Fatal(err)
	}
	parses(t, got)
	if !strings.Contains(string(got), "Tom &amp; Jerry") {
		t.Errorf("the placeholder was not filled or not escaped:\n%s", got)
	}
	// {{what}} was not given and has no default of its own, so it falls back to
	// nothing -- never to the literal {{what}}, which would be rendered into the
	// video as those six characters
	if strings.Contains(string(got), "{{") {
		t.Errorf("a placeholder survived into the render:\n%s", got)
	}
	if !strings.Contains(string(got), "<text></text>") {
		t.Errorf("the unfilled placeholder left something behind:\n%s", got)
	}
	// and the default is what stands when nobody said
	def, _, err := insSVG(holes + "?what=x")
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(def), ">nobody<") {
		t.Errorf("the {{who|nobody}} default did not apply:\n%s", def)
	}

	plain := filepath.Join(dir, "plain.svg")
	os.WriteFile(plain, []byte(`<svg xmlns="http://www.w3.org/2000/svg"><rect/></svg>`), 0o644)
	out, note, err := insSVG(plain + "?S=Dust")
	if err != nil {
		t.Fatal(err)
	}
	if strings.Contains(string(out), "Dust") {
		t.Error("a plain SVG was rewritten from parameters it never declared")
	}
	if !strings.Contains(note, "ignored") {
		t.Errorf("nothing said the parameters went nowhere; the note was %q", note)
	}
	// and with no parameters at all, a file is its own bytes and nothing looks
	// at it twice
	same, note, err := insSVG(plain)
	if err != nil || note != "" || !strings.Contains(string(same), "<rect/>") {
		t.Errorf("a plain insert came back as (%q, %v):\n%s", note, err, same)
	}
}

// The stamp is what makes a card on disk a complete statement of itself: which
// card drew it, and what with. s.svg and a.svg are one card and one letter.
func TestACardOnDiskRemembersWhatDrewIt(t *testing.T) {
	dir := t.TempDir()
	wrote, err := writeSVGCards(dir)
	if err != nil {
		t.Fatal(err)
	}
	if len(wrote) != len(svgSeeds)+1 { // the cards, and the note on writing one
		t.Fatalf("wrote %v, want all %d cards and %s", wrote, len(svgSeeds), cardGuideFile)
	}
	src, err := os.ReadFile(filepath.Join(dir, "a.svg"))
	if err != nil {
		t.Fatal(err)
	}
	card, def := svgStamp(src)
	if card == nil || card.name != "badge" {
		t.Fatalf("a.svg does not say what drew it: %v", card)
	}
	if def.get("label") != "A" {
		t.Errorf("a.svg does not carry its own letter: %v", def)
	}
	// the letter is the file's, the caption is the path's, and the path wins
	// where they overlap
	out := card.draw(def.merge(parseSVGQuery("item=Nuke&label=A+")), "", nil)
	if !strings.Contains(string(out), ">Nuke<") || !strings.Contains(string(out), ">A+<") {
		t.Errorf("the badge did not take the path's word for it:\n%s", out)
	}
	// a second seeding never touches a card the user has since edited
	os.WriteFile(filepath.Join(dir, "a.svg"), []byte("mine"), 0o644)
	if wrote, _ := writeSVGCards(dir); len(wrote) != 0 {
		t.Errorf("seeding overwrote %v", wrote)
	}
	if b, _ := os.ReadFile(filepath.Join(dir, "a.svg")); string(b) != "mine" {
		t.Error("an edited card was replaced by the built-in one")
	}
}

// ---- what the cards do -------------------------------------------------------

// The delay lives in keyTimes, not in begin, and this is the reason: before an
// animation begins, SMIL says the static attribute stands. A card that hid its
// rows with a static opacity="0" would be a blank rectangle in every viewer that
// does not animate -- the file chooser, a file manager's thumbnail, an editor,
// and this app's own still fallback. So the static document is the FINISHED card.
func TestACardThatIsNotAnimatedIsTheFinishedCard(t *testing.T) {
	src := tierSVG(parseSVGQuery("S=Dust II&A=Nuke"), "", nil)
	root, err := parseSVG(src)
	if err != nil {
		t.Fatal(err)
	}
	var walk func(*svgNode)
	walk = func(n *svgNode) {
		for _, a := range n.attrs {
			if xmlName(a.Name) == "opacity" && strings.TrimSpace(a.Value) == "0" {
				t.Errorf("<%s> is statically invisible, so the card is blank in anything "+
					"that does not animate it", n.name)
			}
			if xmlName(a.Name) == "transform" {
				t.Errorf("<%s> is statically moved (%s), so the card is out of place in "+
					"anything that does not animate it", n.name, a.Value)
			}
		}
		for _, an := range n.anims {
			if an.begin != 0 {
				t.Errorf("an animation on <%s> begins at %gs; the wait belongs in keyTimes, "+
					"or the static document is not the finished card", n.name, an.begin)
			}
		}
		for _, k := range n.kids {
			walk(k)
		}
	}
	walk(root)
}

// ...and it does still animate: at t=0 the rows are away and invisible, at the
// end they have landed, and the card lasts long enough to be read after the
// last one does.
func TestTheRowsFlyInAndStay(t *testing.T) {
	src := tierSVG(parseSVGQuery("S=Dust II&A=Nuke&B=Vertigo"), "", nil)
	if !svgAnimated(src) {
		t.Fatal("the board does not animate at all")
	}
	root, err := parseSVG(src)
	if err != nil {
		t.Fatal(err)
	}
	at := func(t0 float64) string {
		var b strings.Builder
		root.renderAt(&b, t0)
		return b.String()
	}
	if !strings.Contains(at(0), `opacity="0"`) {
		t.Error("nothing is hidden at t=0, so the board is simply there from the first frame")
	}
	dur := svgDuration(root)
	if dur < 1.5 {
		t.Fatalf("the board is over in %gs", dur)
	}
	last := at(dur)
	if strings.Contains(last, `opacity="0"`) {
		t.Errorf("something never arrives:\n%s", last)
	}
	for _, m := range regexp.MustCompile(`translate\([^)]*\)`).FindAllString(last, -1) {
		if m != "translate(0 0)" {
			t.Errorf("something is still %s at the end of the board", m)
		}
	}
	// the card outlasts its own animation: the last row lands, and then there is
	// time to read the board before the slot ends
	lastRow := 0.15 + float64(len(tierLetters)-1)*0.28 + 0.55
	if dur < lastRow+cardHold-0.01 {
		t.Errorf("the board runs %gs; the last row lands at %gs and needs %gs to be read",
			dur, lastRow, cardHold)
	}
	// the rows are always the six, so what makes a board longer is what is in
	// them: the last place in the bottom tier is the last thing to arrive
	if full := docDur(t, tierSVG(parseSVGQuery("F=a, b, c, d, e, f"), "", nil)); full <= dur {
		t.Errorf("a board with a full bottom tier runs %gs and a board of three chips %gs", full, dur)
	}
}

// A tier board turns up in a video because one more thing has just been ranked,
// and the board itself is a recap of where the ranking already stood. Playing
// that recap at the pace of a board being built for the first time is the dull
// stretch of every tier list, so: the part the viewer has seen is put back up
// quickly, and the item the shot is actually about arrives on its own, after
// the board has stopped moving, flying in the way the rows do.
func TestTheItemJustAddedArrivesAfterTheRestHasCaughtUp(t *testing.T) {
	const board = "S=Dust II, Mirage&A=Nuke"
	// naming the new one must not put a second chip on the board: it points at an
	// item that is already in a tier
	rows, arrive := tierBoard(parseSVGQuery(board + "&new=Mirage"))
	if got := len(rows[0].items); got != 2 {
		t.Errorf("S holds %d chips, so the item named as new was added beside itself", got)
	}
	if _, ok := arrive[[2]int{0, 1}]; !ok || len(arrive) != 1 {
		t.Errorf("the arrivals are %v, want the one chip that was named", arrive)
	}

	plain := mustParse(t, tierSVG(parseSVGQuery(board), "", nil))
	fresh := mustParse(t, tierSVG(parseSVGQuery(board+"&new=Mirage"), "", nil))
	newChip := chipOf(t, fresh, "Mirage")
	oldChip := chipOf(t, fresh, "Nuke")

	newLands, oldLands := landing(t, fresh, newChip), landing(t, fresh, oldChip)
	if newLands < oldLands+tierNewBeat {
		t.Errorf("the new chip lands at %gs and the rest of the board at %gs -- it is "+
			"arriving in the middle of the recap instead of after it", newLands, oldLands)
	}
	// and the board really is standing still in between: at the moment the recap
	// is over, the one that is news has not started
	if visible(newChip, oldLands+0.01) {
		t.Error("the new chip is already on screen when the recap ends, so there is no " +
			"moment where the board is the board as it was")
	}
	// the recap is the same arrivals, faster -- not the same card with something
	// stuck on the end of it
	if was := landing(t, plain, chipOf(t, plain, "Nuke")); oldLands >= was {
		t.Errorf("the board caught up in %gs and takes %gs when nothing is new: the recap "+
			"is not any quicker", oldLands, was)
	}

	// it comes in from off the right, like a row, rather than rising out of its
	// own row like the chips around it
	var b strings.Builder
	newChip.renderAt(&b, newLands-tierNewFly+0.05)
	m := regexp.MustCompile(`translate\(([-0-9.]+)`).FindStringSubmatch(b.String())
	if m == nil {
		t.Fatalf("the new chip is not moving as it arrives:\n%s", b.String())
	}
	if x, _ := strconv.ParseFloat(m[1], 64); x < cardW/4 {
		t.Errorf("the new chip starts %s from home; it is meant to come in from off the "+
			"right edge of the card", m[1])
	}

	// and the card is still a card: the new one has landed with time left to read
	// the board it landed on
	dur := svgDuration(fresh)
	if dur < newLands+cardHold-0.01 {
		t.Errorf("the board runs %gs and the new item lands at %gs, leaving %gs to read it",
			dur, newLands, dur-newLands)
	}
	var end strings.Builder
	fresh.renderAt(&end, dur)
	if strings.Contains(end.String(), `opacity="0"`) {
		t.Errorf("something never arrives:\n%s", end.String())
	}
}

// Which item on the board is the new one is answered by whatever a person would
// call it: the item is written "Dust II|logos/dust2.png" and what gets typed in
// the New field is the name, or the file, or what the chip says when it is only
// a logo.
func TestTheNewItemIsRecognisedByAnyOfItsNames(t *testing.T) {
	it := splitItems("Dust II|logos/dust2.png")[0]
	for _, name := range []string{
		"Dust II", "dust ii", "logos/dust2.png", "Dust II|logos/dust2.png", "dust2",
	} {
		if !tierIsNew([]string{name}, it) {
			t.Errorf("%q does not name the item it plainly names", name)
		}
	}
	for _, name := range []string{"Nuke", "dust", "logos"} {
		if tierIsNew([]string{name}, it) {
			t.Errorf("%q was taken for the item, and some other chip gets the arrival", name)
		}
	}
	// nothing named is the card as it always was: no chip singled out
	if tierIsNew(splitLabels(""), it) {
		t.Error("an empty New field made everything new")
	}
}

// The list of arrivals says three things about each of them, and it has to be
// able to say them in one line typed into one field: which row it lands in, when
// it starts, and what lands.
//
// The commas separate the list the way they separate a row's items, and the
// prefix is what starts an entry -- everything up to the next prefix belongs to
// the one before it. Which leaves one question the syntax has to answer by
// itself: "bla.svg, Test" is a logo and a name, and it means the chip with the
// name under the logo, while "Dust II, Mirage" is two names and means two chips.
// A part beside another that fills the half it left empty is the same chip; a
// part that would overwrite something is the next one.
func TestTheArrivalsSayWhereAndWhenAndWhat(t *testing.T) {
	for _, c := range []struct {
		in   string
		want []tierNew
	}{
		// the case this was written for, in both spellings of the wait
		{"D[1100ms]: assets/bla.svg, Test",
			[]tierNew{{"D", 1.1, tierItem{text: "Test", logo: "assets/bla.svg"}}}},
		{"D[1.1s]: assets/bla.svg, Test",
			[]tierNew{{"D", 1.1, tierItem{text: "Test", logo: "assets/bla.svg"}}}},
		{"D[1.1]: Test|assets/bla.svg",
			[]tierNew{{"D", 1.1, tierItem{text: "Test", logo: "assets/bla.svg"}}}},
		// several things flying in, each on its own beat
		{"D[1.1s]: a.svg, S[2s]: b.svg, C: Nuke", []tierNew{
			{"D", 1.1, tierItem{logo: "a.svg"}},
			{"S", 2, tierItem{logo: "b.svg"}},
			{"C", -1, tierItem{text: "Nuke"}},
		}},
		// a row's own list, which is the syntax this borrows: two names are two
		// chips, both into S, both timed by the card
		{"S: Dust II, Mirage", []tierNew{
			{"S", -1, tierItem{text: "Dust II"}},
			{"S", -1, tierItem{text: "Mirage"}},
		}},
		// no row named: the item is already on the board and this points at it,
		// which is what the field has always meant
		{"Mirage", []tierNew{{"", -1, tierItem{text: "Mirage"}}}},
		{"Dust II, Mirage", []tierNew{
			{"", -1, tierItem{text: "Dust II"}},
			{"", -1, tierItem{text: "Mirage"}},
		}},
		// ...and it can still be told when to arrive
		{"[2s]: Mirage", []tierNew{{"", 2, tierItem{text: "Mirage"}}}},
		{"", nil},
	} {
		got := parseNew(c.in)
		if len(got) != len(c.want) {
			t.Errorf("%q read as %d arrivals, want %d: %v", c.in, len(got), len(c.want), got)
			continue
		}
		for i, w := range c.want {
			if got[i] != w {
				t.Errorf("%q arrival %d is %+v, want %+v", c.in, i, got[i], w)
			}
		}
	}
}

// The board is the six tiers, so flying something into D puts it in the D that
// is already there. Flying something into E is a typo, and a typo that quietly
// draws nothing is the field not working -- so it is said instead.
func TestAnArrivalLandsInTheTierItNames(t *testing.T) {
	q := parseSVGQuery("S=Dust II&new=D[1.1s]: assets/bla.svg, Test")
	rows, arrive := tierBoard(q)
	if formLabels(rows) != "S,A,B,C,D,F" {
		t.Fatalf("the board is %s", formLabels(rows))
	}
	if got := len(rows[4].items); got != 1 {
		t.Fatalf("row D holds %d chips, want the one that was sent to it", got)
	}
	if it := rows[4].items[0]; it.text != "Test" || it.logo != "assets/bla.svg" {
		t.Errorf("row D got (%q, %q), want the logo with Test under it", it.text, it.logo)
	}
	if at, ok := arrive[[2]int{4, 0}]; !ok || at != 1.1 {
		t.Errorf("the chip in D arrives at %g (found: %v), want 1.1", at, ok)
	}
	// the dialog is the same eight lines whatever arrives, because the tiers are
	// the tiers
	if got := formKeys(tierForm(q, nil)); got != "title,S,A,B,C,D,F,new" {
		t.Errorf("the dialog asks for %s", got)
	}

	// a tier that is not on the board is not made: nothing is drawn for it, and
	// the card says where it went
	e := parseSVGQuery("S=Dust II&new=E[1.1s]: Nuke")
	rows, arrive = tierBoard(e)
	if formLabels(rows) != "S,A,B,C,D,F" {
		t.Errorf("the board grew an E row: %s", formLabels(rows))
	}
	if len(arrive) != 0 {
		t.Errorf("something arrives on a board that has nowhere to put it: %v", arrive)
	}
	for _, r := range rows {
		for _, it := range r.items {
			if it.text == "Nuke" {
				t.Errorf("the E arrival landed in %s instead", r.label)
			}
		}
	}
	if !strings.Contains(tierNote(e), "no E") {
		t.Errorf("the arrival went nowhere and nothing was said: %q", tierNote(e))
	}
	// a tier that is already full is the same answer: six is what there is room
	// for, and a seventh does not push one out
	full := parseSVGQuery("S=a, b, c, d, e, f&new=S: Nuke")
	rows, arrive = tierBoard(full)
	if got := len(rows[0].items); got != tierSlots || len(arrive) != 0 {
		t.Errorf("a full tier took another chip: %d items, arrivals %v", got, arrive)
	}

	// saying where something goes twice puts it there once
	twice := parseSVGQuery("S=Dust II|d.png&new=S[0.5s]: Dust II")
	rows, arrive = tierBoard(twice)
	if got := len(rows[0].items); got != 1 {
		t.Errorf("row S holds %d chips; the arrival was added beside the item it names", got)
	}
	if at := arrive[[2]int{0, 0}]; at != 0.5 {
		t.Errorf("the item already in the row arrives at %g, want the 0.5 it was given", at)
	}
}

// A wait that was asked for is the wait: the point of writing one is to send
// several things in on a beat, and a card that decides for itself when they go
// cannot be cut to anything.
func TestAnArrivalWaitsAsLongAsItWasTold(t *testing.T) {
	root := mustParse(t, tierSVG(parseSVGQuery(
		"tiers=S, A&S=Dust II&new=A[1.1s]: Test, S[2.5s]: Nuke"), "", nil))
	first, second := chipOf(t, root, "Test"), chipOf(t, root, "Nuke")
	for _, c := range []struct {
		who  *svgNode
		name string
		at   float64
	}{{first, "Test", 1.1}, {second, "Nuke", 2.5}} {
		if visible(c.who, c.at-0.05) {
			t.Errorf("%s is already arriving before the %gs it was given", c.name, c.at)
		}
		if got := landing(t, root, c.who); math.Abs(got-(c.at+tierNewFly)) > 0.05 {
			t.Errorf("%s lands at %gs; told to start at %gs it should be down by %gs",
				c.name, got, c.at, c.at+tierNewFly)
		}
	}
	// the card lasts long enough for the last one to be read, however late it
	// was sent
	if dur := svgDuration(root); dur < 2.5+tierNewFly+cardHold-0.01 {
		t.Errorf("the board runs %gs and the last arrival is not down until %gs",
			dur, 2.5+tierNewFly)
	}
}

// formKeys is the dialog as a line, for the tests that care about which
// questions are asked and in what order rather than about any of the answers.
func formKeys(f []svgField) string {
	var out []string
	for _, x := range f {
		out = append(out, x.Key)
	}
	return strings.Join(out, ",")
}

func mustParse(t *testing.T, src []byte) *svgNode {
	t.Helper()
	root, err := parseSVG(src)
	if err != nil {
		t.Fatal(err)
	}
	return root
}

// chipOf finds the group drawn for one item of the board, which is the innermost
// one holding the item's name.
func chipOf(t *testing.T, root *svgNode, name string) *svgNode {
	t.Helper()
	var found *svgNode
	var walk func(*svgNode, *svgNode)
	walk = func(n, g *svgNode) {
		if n.name == "g" {
			g = n
		}
		if n.name == "text" && strings.TrimSpace(n.text) == name {
			found = g
		}
		for _, k := range n.kids {
			walk(k, g)
		}
	}
	walk(root, nil)
	if found == nil {
		t.Fatalf("no chip on the board says %q", name)
	}
	return found
}

// chipState is what one group looks like at a moment of the card: how far in it
// has faded, and whether it is still somewhere other than where it belongs.
func chipState(n *svgNode, at float64) (opacity float64, placed bool) {
	var b strings.Builder
	n.renderAt(&b, at)
	open := strings.SplitN(b.String(), ">", 2)[0]
	opacity = 1
	if m := reOpacity.FindStringSubmatch(open); m != nil {
		opacity, _ = strconv.ParseFloat(m[1], 64)
	}
	m := reTranslate.FindStringSubmatch(open)
	return opacity, m == nil || strings.TrimSpace(m[1]) == "0 0"
}

var (
	reOpacity   = regexp.MustCompile(`opacity="([-0-9.]+)"`)
	reTranslate = regexp.MustCompile(`translate\(([^)]*)\)`)
)

// visible is whether a group has begun to arrive. What has not started yet is
// held at opacity 0, which is how the card keeps it off the screen.
func visible(n *svgNode, at float64) bool {
	op, _ := chipState(n, at)
	return op > 0
}

// landing is the first moment a group is fully there and where it belongs,
// asked of the document rather than worked out from the schedule: the schedule
// is the thing under test.
func landing(t *testing.T, root, n *svgNode) float64 {
	t.Helper()
	dur := svgDuration(root)
	for at := 0.0; at <= dur; at += 0.01 {
		if op, placed := chipState(n, at); op > 0.999 && placed {
			return at
		}
	}
	t.Fatalf("<%s> never lands in the %gs the card runs", n.name, dur)
	return 0
}

// A thing in a tier is a name, a logo, or both, and whatever it is has to come
// back out of the dialog written the way it went in.
func TestAnItemIsANameALogoOrBoth(t *testing.T) {
	for _, c := range []struct {
		in         string
		text, logo string
	}{
		{"Dust II", "Dust II", ""},
		{"logos/dust2.png", "", "logos/dust2.png"},
		{"Dust II|logos/dust2.png", "Dust II", "logos/dust2.png"},
		{" Dust II | logos/dust2.png ", "Dust II", "logos/dust2.png"},
		{"Dust II|", "Dust II", ""},
		{"|logos/dust2.png", "", "logos/dust2.png"},
		// a name that is not an image file is a name, whatever it has in it
		{"half-life 2.exe", "half-life 2.exe", ""},
	} {
		got := splitItems(c.in)
		if len(got) != 1 {
			t.Fatalf("%q split into %d items", c.in, len(got))
		}
		if got[0].text != c.text || got[0].logo != c.logo {
			t.Errorf("%q read as (%q, %q), want (%q, %q)", c.in, got[0].text, got[0].logo, c.text, c.logo)
		}
	}
	items := splitItems("Dust II|d.png, Nuke, , mirage.png")
	if len(items) != 3 {
		t.Fatalf("the commas separated %d items, want 3: %v", len(items), items)
	}
	if got, want := joinItems(items), "Dust II|d.png, Nuke, mirage.png"; got != want {
		t.Errorf("the items went back to the dialog as %q, want %q", got, want)
	}
}

// A logo is carried in the card rather than pointed at, and this is the reason:
// the baked frames are written to a folder somewhere else entirely, so a path in
// an href resolves against nothing by the time librsvg reads it.
func TestALogoIsCarriedInTheCard(t *testing.T) {
	dir := t.TempDir()
	png := []byte("\x89PNG\r\n\x1a\n" + strings.Repeat("x", 40))
	os.MkdirAll(filepath.Join(dir, "logos"), 0o755)
	os.WriteFile(filepath.Join(dir, "logos", "dust2.png"), png, 0o644)

	// the card is in assets and the logo is written the way the project writes a
	// path, so it is found in the folder above the card as well as beside it
	assets := filepath.Join(dir, "assets")
	os.MkdirAll(assets, 0o755)
	src := tierSVG(parseSVGQuery("S=Dust II|logos/dust2.png"), assets, nil)
	parses(t, src)
	// the board carries a picture element for every place in it; what says
	// whether one is a logo is whether it has been switched on
	if got := picsOn(string(src)); got != 1 {
		t.Fatalf("%d pictures are showing, want the one logo:\n%s", got, src)
	}
	if !strings.Contains(string(src), "href=\"data:image/png;base64,") {
		t.Errorf("the logo is a path rather than the picture itself:\n%s", src)
	}
	if !strings.Contains(string(src), base64.StdEncoding.EncodeToString(png)) {
		t.Error("the href does not hold the file's own bytes")
	}
	// both were asked for, so both are on the chip
	if !strings.Contains(string(src), ">Dust II<") {
		t.Error("the name went missing once the item had a logo")
	}

	// a logo alone is a chip with no text on it at all
	only := tierSVG(parseSVGQuery("S=logos/dust2.png"), assets, nil)
	if strings.Contains(cardText(string(only)), "dust2") {
		t.Errorf("a logo-only chip was captioned anyway:\n%s", only)
	}
	if picsOn(string(only)) != 1 {
		t.Error("the bare path was not read as a logo")
	}

	// and a logo that is not there is not a hole in the board: the file's name
	// stands in, so the chip still ranks something
	missing := tierSVG(parseSVGQuery("S=logos/nope.png"), assets, nil)
	parses(t, missing)
	if picsOn(string(missing)) != 0 {
		t.Error("a logo that could not be read was drawn as an empty image")
	}
	if !strings.Contains(string(missing), ">nope<") {
		t.Errorf("the missing logo left the chip blank:\n%s", missing)
	}
}

// picsOn is how many of the board's picture elements are switched on: the file
// holds one per place, and a place with no logo in it leaves its own at
// display="none".
func picsOn(src string) int {
	return strings.Count(src, `<image display="inline"`)
}

// cardText is what a card says, without the attributes -- a data URI is a
// thousand letters and would answer for any of them.
func cardText(s string) string {
	var b strings.Builder
	tag := false
	for _, r := range s {
		switch {
		case r == '<':
			tag = true
		case r == '>':
			tag = false
		case !tag:
			b.WriteRune(r)
		}
	}
	return b.String()
}

func docDur(t *testing.T, src []byte) float64 {
	t.Helper()
	root, err := parseSVG(src)
	if err != nil {
		t.Fatal(err)
	}
	return svgDuration(root)
}

// The badge flies in from off the left. Anything that is on screen at t=0 is
// not flying in, it is sitting there.
func TestTheBadgeComesInFromOffTheSide(t *testing.T) {
	root, err := parseSVG(badgeSVG(parseSVGQuery("label=S&item=Dust II"), "", nil))
	if err != nil {
		t.Fatal(err)
	}
	var b strings.Builder
	root.renderAt(&b, 0)
	first := b.String()
	if !strings.Contains(first, "translate(-") {
		t.Errorf("the badge does not start off to the left:\n%s", first)
	}
	b.Reset()
	root.renderAt(&b, svgDuration(root))
	if !strings.Contains(b.String(), ">Dust II<") {
		t.Error("the caption never arrives")
	}
}

// Every frame of a baked card goes to librsvg on its own. One that does not
// parse is a decode error and a clip of nothing, so this bakes a card the whole
// way and reads every frame back.
func TestABakedCardIsAWellFormedFrameEveryTime(t *testing.T) {
	dir := t.TempDir()
	src := tierSVG(parseSVGQuery("title=Best & worst&S=Dust II, Mirage&A=Nuke"), "", nil)
	pat, n, err := bakeSVG(src, dir, 10, 2)
	if err != nil {
		t.Fatal(err)
	}
	if n != 20 {
		t.Errorf("baked %d frames for 2s at 10fps", n)
	}
	if !strings.Contains(pat, "%05d") {
		t.Errorf("the pattern ffmpeg reads is %q", pat)
	}
	for i := 0; i < n; i++ {
		b, err := os.ReadFile(filepath.Join(dir, fmt.Sprintf("f%05d.svg", i)))
		if err != nil {
			t.Fatal(err)
		}
		parses(t, b)
		if strings.Contains(string(b), "<animate") {
			t.Fatalf("frame %d still carries SMIL, so librsvg has to animate it and does not", i)
		}
	}
}

// ---- a card's own words ------------------------------------------------------

// The dialog used to be a Go function per card, which meant a card somebody else
// wrote could only ever be asked "what goes in {{this}}?" -- no label, no
// explanation, no file picker. So a card says what its boxes are called, in the
// file, and this is that round trip: what the card draws is what the reader
// reads back.
func TestACardCarriesTheFormItIsAskedWith(t *testing.T) {
	// the file declares every box there is, one Input line per tier beside the
	// tier it belongs to: the board is fixed, so the file can say all of it
	if got := formKeys(svgInputs([]byte(tierTemplate))); got != "title,S,A,B,C,D,F,new" {
		t.Fatalf("the template declares %s, want a line per tier and the card's own boxes", got)
	}
	// and a board drawn from it says the same thing, because the comments are
	// carried through the drawing untouched
	src := tierSVG(nil, "", nil)
	parses(t, src) // a comment that ended early would take the rest of the card with it
	in := svgInputs(src)
	if got := formKeys(in); got != "title,S,A,B,C,D,F,new" {
		t.Fatalf("the drawn board declares %s", got)
	}
	row := in[1]
	if row.Label != "Tier S" {
		t.Errorf("the first tier line is called %q", row.Label)
	}
	if !row.Keep || !row.Logo {
		t.Errorf("a tier line came back keep=%v logo=%v", row.Keep, row.Logo)
	}
	// the hint has a bar in it -- "Name|logo.png" -- and the bar is also what
	// separates the parts, so this is the one thing the format has to get right
	if !strings.Contains(row.Hint, "Name|logo.png") {
		t.Errorf("the hint was cut off at the bar it contains: %q", row.Hint)
	}
	// and the file has the last word: rename a box there and the dialog says that
	dir := t.TempDir()
	path := filepath.Join(dir, "tier.svg")
	edited := strings.Replace(tierTemplate, "| Just added |", "| Newly ranked |", 1)
	if err := os.WriteFile(path, []byte(edited), 0o644); err != nil {
		t.Fatal(err)
	}
	f, ok := insForm(path, nil)
	if !ok {
		t.Fatal("an edited card stopped answering for its own form")
	}
	// which boxes there are is still the six tiers and the card's own two: what
	// the file has the last word on is what they are called
	if got := formKeys(f); got != "title,S,A,B,C,D,F,new" {
		t.Fatalf("the edited board asks for %s", got)
	}
	if f[fieldOf(f, "new")].Label != "Newly ranked" {
		t.Errorf("the dialog calls it %q, not what the file calls it", f[fieldOf(f, "new")].Label)
	}
	if f[fieldOf(f, "S")].Label != "Tier S" {
		t.Errorf("a row is labelled %q, want what the file calls that tier", f[fieldOf(f, "S")].Label)
	}
}

// The point of putting it in the file: an SVG nothing here drew gets a real
// form, with words on it, without this package knowing what the picture is.
func TestADocumentThatSaysWhatItWantsIsAskedThatWay(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "mine.svg")
	os.WriteFile(path, []byte(`<?xml version="1.0" encoding="UTF-8"?>
<svg xmlns="http://www.w3.org/2000/svg" width="1920" height="1080">
  <!-- Input: line | Headline | the one thing this card says -->
  <!-- Input: logos[keep logo] | Pictures | files, comma separated -->
  <text>{{line}}</text><text>{{sub|later}}</text><text>{{logos}}</text>
</svg>`), 0o644)

	f, ok := insFields(path + "?line=Hello")
	if !ok {
		t.Fatal("the document was not asked about at all")
	}
	// what it declared, in the order it declared it, and then the hole it forgot
	// to mention -- saying something about one box does not hide the others
	if got := formKeys(f); got != "line,logos,sub" {
		t.Fatalf("the dialog asks for %s", got)
	}
	if f[0].Label != "Headline" || !strings.Contains(f[0].Hint, "the one thing") {
		t.Errorf("the first box is %q / %q, not what the file says it is", f[0].Label, f[0].Hint)
	}
	if !f[1].Keep || !f[1].Logo {
		t.Errorf("the Pictures box takes no files: keep=%v logo=%v", f[1].Keep, f[1].Logo)
	}
	if f[0].Val != "Hello" {
		t.Errorf("the box starts at %q rather than at what the path already says", f[0].Val)
	}
	// a box nobody has filled in starts at what the placeholder itself falls back
	// to, so the dialog is never emptier than the picture
	if f[2].Val != "later" {
		t.Errorf("the undeclared box starts at %q, want the placeholder's own default", f[2].Val)
	}
	// and the picture is still filled in the ordinary way: the comments say what
	// to ask, they do not take over the drawing
	out, note, err := insSVG(path + "?line=Hello")
	if err != nil || !strings.Contains(string(out), ">Hello<") {
		t.Errorf("the document was not filled in (%q, %v):\n%s", note, err, out)
	}
}

// The board in assets is the app's own worked example, and the one thing a still
// cannot show is the thing these cards are for: something arriving. So it ships
// with two of the letter cards beside it flying onto it -- in the file, not on
// screen, until the moment they were given.
//
// The file itself is the template now, so this is also the round trip that
// matters: what is written into a project is what the app draws boards from.
func TestTheBoardInAssetsComesWithTwoBadgesFlyingOntoIt(t *testing.T) {
	dir := t.TempDir()
	if _, err := writeSVGCards(dir); err != nil {
		t.Fatal(err)
	}
	path := filepath.Join(dir, "tier.svg")
	file, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	// the whole board is written out in there, holes and all -- a board drawn
	// flat into the folder would be a board nobody can restyle
	for _, want := range []string{"{{s1.name}}", "{{f6.name}}", `display="{{`, "{{total|8s}}"} {
		if !strings.Contains(string(file), want) {
			t.Fatalf("the board in assets no longer carries %s", want)
		}
	}
	for _, l := range tierLetters {
		if want := "<!-- Input: " + l + "[keep logo] |"; !strings.Contains(string(file), want) {
			t.Errorf("the board in assets does not say what its %s line is", l)
		}
	}
	if n := strings.Count(string(file), `<g display="{{`); n != len(tierLetters)*tierSlots {
		t.Errorf("the file holds %d places, want %d", n, len(tierLetters)*tierSlots)
	}
	src, note, err := insSVG(path)
	if err != nil {
		t.Fatalf("the board in assets does not draw (%q): %v", note, err)
	}
	// carried in the document, base64 and all: a href to a file beside it would
	// resolve against nothing once the frames are baked somewhere else
	if n := strings.Count(string(src), "data:image/svg+xml;base64,"); n != 2 {
		t.Fatalf("the board has %d letters in it, want a.svg and b.svg", n)
	}
	root := mustParse(t, src)
	chips := logoChips(root)
	if len(chips) != 2 {
		t.Fatalf("the board draws %d chips with a picture in them", len(chips))
	}
	for i, at := range []float64{1.2, 1.8} {
		if visible(chips[i], at-0.05) {
			t.Errorf("chip %d is on the board before the %gs it was given", i, at)
		}
		if got := landing(t, root, chips[i]); math.Abs(got-(at+tierNewFly)) > 0.05 {
			t.Errorf("chip %d lands at %gs, want %gs", i, got, at+tierNewFly)
		}
	}
	// and the note on how to write one of these is in the folder with them
	guide, err := os.ReadFile(filepath.Join(dir, cardGuideFile))
	if err != nil {
		t.Fatal(err)
	}
	for _, want := range []string{"1920x1080", "<!-- Input:", "keyTimes", `display="{{`} {
		if !strings.Contains(string(guide), want) {
			t.Errorf("%s does not mention %s", cardGuideFile, want)
		}
	}
}

// logoChips are the groups drawn for an item that is a picture, in the order the
// board draws them.
func logoChips(root *svgNode) []*svgNode {
	var out []*svgNode
	var walk func(*svgNode, *svgNode)
	walk = func(n, g *svgNode) {
		if n.name == "g" {
			g = n
		}
		// every place in the board holds a picture element and most of them are
		// switched off; the ones that are showing are the logos
		if n.name == "image" && g != nil && attrOf(n, "display") == "inline" {
			out = append(out, g)
		}
		for _, k := range n.kids {
			walk(k, g)
		}
	}
	walk(root, nil)
	return out
}

// attrOf is one attribute of a node, or "" -- the tests ask about display, which
// is how the board switches a place it has nothing for off.
func attrOf(n *svgNode, name string) string {
	for _, a := range n.attrs {
		if xmlName(a.Name) == name {
			return strings.TrimSpace(a.Value)
		}
	}
	return ""
}
