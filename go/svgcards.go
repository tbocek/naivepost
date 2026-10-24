package main

// Cards: the pictures the app draws itself. An insert path may carry
// parameters after a "?", as a URL does:
//
//	assets/tier.svg?S=Dust II,Mirage|logos/mirage.png&A=Nuke&title=Best maps
//	assets/s.svg?label=S&item=Dust II
//
// The part before "?" must exist. A document with {{name}} placeholders has
// them filled in; one stamped data-naivepost="tier" is drawn again by that card
// from the parameters, the file saying where every place is. The stamp also
// carries the parameters it was drawn with (data-naivepost-args), overridden key
// by key by the "?". Animation is SMIL, baked per frame by svganim.go.

import (
	"bytes"
	"encoding/base64"
	"encoding/xml"
	"fmt"
	"math"
	"os"
	"path/filepath"
	"regexp"
	"strconv"
	"strings"
)

// ---- an insert's parameters -------------------------------------------------

// svgParam is one key=value from an insert path's query. A slice of them rather
// than a map because the order is content here: the tier rows come out in the
// order they were written, and a map would rank the maps alphabetically.
type svgParam struct{ Key, Val string }

type svgQuery []svgParam

// insSplit takes an insert path apart into the file and its parameters. Every
// place that opens, probes or names an insert goes through this: a path with a
// "?" left on it is a file that does not exist, and the symptom is a card that
// silently disappears from the render.
func insSplit(p string) (string, svgQuery) {
	i := strings.IndexByte(p, '?')
	if i < 0 {
		return p, nil
	}
	return p[:i], parseSVGQuery(p[i+1:])
}

// insBase is what an insert is called on screen and in the log: the file's name
// without the parameters, which are usually longer than the rest of the row.
func insBase(p string) string {
	file, _ := insSplit(p)
	return filepath.Base(file)
}

func parseSVGQuery(s string) svgQuery {
	var q svgQuery
	for _, part := range strings.Split(s, "&") {
		if strings.TrimSpace(part) == "" {
			continue
		}
		k, v := part, ""
		if j := strings.IndexByte(part, '='); j >= 0 {
			k, v = part[:j], part[j+1:]
		}
		if k = strings.TrimSpace(svgUnescape(k)); k == "" {
			continue
		}
		q = append(q, svgParam{k, svgUnescape(v)})
	}
	return q
}

// args writes the parameters back the way they are read, so a path survives a
// round trip through the project file -- and so a card can be stamped with the
// ones it was drawn from.
func (q svgQuery) args() string {
	var b strings.Builder
	for i, p := range q {
		if i > 0 {
			b.WriteByte('&')
		}
		b.WriteString(svgEscape(p.Key))
		b.WriteByte('=')
		b.WriteString(svgEscape(p.Val))
	}
	return b.String()
}

// suffix is args with the "?" that joins them to a path, and nothing at all
// when there are no parameters: a bare "?" on a path is a file that is not there.
func (q svgQuery) suffix() string {
	if len(q) == 0 {
		return ""
	}
	return "?" + q.args()
}

func (q svgQuery) get(key string) string {
	for _, p := range q {
		if strings.EqualFold(p.Key, key) {
			return p.Val
		}
	}
	return ""
}

func (q svgQuery) has(key string) bool {
	for _, p := range q {
		if strings.EqualFold(p.Key, key) {
			return true
		}
	}
	return false
}

// merge is the file's own parameters with the path's laid over them, key by
// key. Order comes from whichever mentioned the key first, so overriding one
// tier's items does not move that tier to the bottom of the board.
func (q svgQuery) merge(over svgQuery) svgQuery {
	out := make(svgQuery, 0, len(q)+len(over))
	for _, p := range q {
		if over.has(p.Key) {
			p.Val = over.get(p.Key)
		}
		out = append(out, p)
	}
	for _, p := range over {
		if !q.has(p.Key) {
			out = append(out, p)
		}
	}
	return out
}

// Only the four characters that would be read as structure are escaped, and
// spaces are left alone: these strings are typed into a path by hand as often
// as they are written by the dialog, and "?S=Dust II" has to mean what it says.
var svgEsc = strings.NewReplacer("%", "%25", "&", "%26", "=", "%3D", "?", "%3F")

func svgEscape(s string) string { return svgEsc.Replace(s) }

func svgUnescape(s string) string {
	if !strings.ContainsRune(s, '%') {
		return s
	}
	var b strings.Builder
	for i := 0; i < len(s); i++ {
		if s[i] == '%' && i+2 < len(s) {
			if h, ok := hexByte(s[i+1], s[i+2]); ok {
				b.WriteByte(h)
				i += 2
				continue
			}
		}
		b.WriteByte(s[i])
	}
	return b.String()
}

func hexByte(a, b byte) (byte, bool) {
	hi, ok1 := hexNib(a)
	lo, ok2 := hexNib(b)
	return hi<<4 | lo, ok1 && ok2
}

func hexNib(c byte) (byte, bool) {
	switch {
	case c >= '0' && c <= '9':
		return c - '0', true
	case c >= 'a' && c <= 'f':
		return c - 'a' + 10, true
	case c >= 'A' && c <= 'F':
		return c - 'A' + 10, true
	}
	return 0, false
}

// ---- filling a document in ---------------------------------------------------

// {{name}}, or {{name|what it says when nobody said}}. Deliberately the dumbest
// syntax that works: it survives being pasted into an SVG editor, and a file
// full of them still opens as a picture.
var svgHole = regexp.MustCompile(`\{\{\s*([A-Za-z0-9_.\-]+)\s*(?:\|([^{}]*))?\}\}`)

// svgFill puts the parameters into a document's placeholders. A placeholder
// nobody gave a value for falls back to its own default, and then to nothing --
// never to the literal {{name}}, which would be baked into the video.
func svgFill(src []byte, q svgQuery) []byte {
	return outsideComments(src, func(src []byte) []byte {
		return svgHole.ReplaceAllFunc(src, fillHole(q))
	})
}

// fillHole is one placeholder answered, split out so svgFill can hand it the
// parts of a document that are not comments.
func fillHole(q svgQuery) func([]byte) []byte {
	return func(m []byte) []byte {
		g := svgHole.FindSubmatch(m)
		v := ""
		if q.has(string(g[1])) {
			v = q.get(string(g[1]))
		} else {
			v = string(g[2])
		}
		return []byte(escapeText(v))
	}
}

// svgHoles is what a document asks for, in the order it asks. Used to build the
// dialog for an SVG nothing here drew.
func svgHoles(src []byte) []svgField {
	var out []svgField
	seen := map[string]bool{}
	for _, g := range svgHole.FindAllSubmatch(reComment.ReplaceAll(src, nil), -1) {
		k := string(g[1])
		if seen[strings.ToLower(k)] {
			continue
		}
		seen[strings.ToLower(k)] = true
		out = append(out, svgField{Key: k, Label: k, Val: string(g[2])})
	}
	return out
}

// ---- what a document asks for, in the document's own words -------------------
//
// A card declares its parameters in comments, one per parameter:
//
//	<!-- Input: title | Title | over the board, empty for none -->
//	<!-- Input: S[keep logo] | Tier S | what is in it, comma separated -->
//
// Key, dialog label, hint (everything after the second bar, bars included).
// Brackets are the flags: keep, logo (svgField). The dialog comes out of the
// file, so a hand-written SVG gets a form; assets/CARDS.md documents this for
// whoever writes one.
var svgInput = regexp.MustCompile(`(?s)<!--\s*Input:\s*(.*?)-->`)

func svgInputs(src []byte) []svgField {
	var out []svgField
	for _, m := range svgInput.FindAllSubmatch(src, -1) {
		parts := strings.SplitN(string(m[1]), "|", 3)
		var f svgField
		key := strings.TrimSpace(parts[0])
		if i := strings.IndexByte(key, '['); i >= 0 {
			for _, fl := range strings.Fields(strings.ReplaceAll(strings.Trim(key[i:], "[]"), ",", " ")) {
				switch strings.ToLower(fl) {
				case "keep":
					f.Keep = true
				case "logo":
					f.Logo = true
				}
			}
			key = strings.TrimSpace(key[:i])
		}
		if key == "" {
			continue
		}
		f.Key, f.Label = key, key
		if len(parts) > 1 && strings.TrimSpace(parts[1]) != "" {
			f.Label = strings.TrimSpace(parts[1])
		}
		if len(parts) > 2 {
			f.Hint = strings.TrimSpace(parts[2])
		}
		out = append(out, f)
	}
	return out
}

// declaredFields is the form of a document nothing here drew: what it declares,
// filled in with what is set now, and then any {{hole}} it did not declare. A
// document is allowed to say nothing about itself, some of it, or all of it --
// what it leaves out is still asked for, just under its own bare name.
func declaredFields(src []byte, q svgQuery) []svgField {
	dec := svgInputs(src)
	if len(dec) == 0 {
		return nil
	}
	holes := svgHoles(src)
	out := make([]svgField, 0, len(dec)+len(holes))
	for _, f := range dec {
		f.Val = fieldVal(f.Key, q, holes)
		out = append(out, f)
	}
	for _, h := range holes {
		if fieldOf(out, h.Key) < 0 {
			h.Val = fieldVal(h.Key, q, holes)
			out = append(out, h)
		}
	}
	return out
}

// fieldVal is what the box starts with: what the path says, and failing that
// what the placeholder itself falls back to, so a field is never emptier than
// the picture already is.
func fieldVal(key string, q svgQuery, holes []svgField) string {
	if q.has(key) {
		return q.get(key)
	}
	if i := fieldOf(holes, key); i >= 0 {
		return holes[i].Val
	}
	return ""
}

func fieldOf(f []svgField, key string) int {
	for i, x := range f {
		if strings.EqualFold(x.Key, key) {
			return i
		}
	}
	return -1
}

// cardInputs is a card's declaration as its own file carries it, and the
// built-in one when the file has none: a card written into assets before the
// comments existed still has to have a form, and so does a card being drawn
// rather than read.
func cardInputs(src []byte, built []svgField) []svgField {
	if f := svgInputs(src); len(f) > 0 {
		return f
	}
	return built
}

// inputDecl writes a field back the way svgInputs reads it, which is how a card
// comes to carry its own form.
func inputDecl(f svgField) string {
	key := f.Key
	var flags []string
	for _, fl := range []struct {
		on bool
		s  string
	}{{f.Keep, "keep"}, {f.Logo, "logo"}} {
		if fl.on {
			flags = append(flags, fl.s)
		}
	}
	if len(flags) > 0 {
		key += "[" + strings.Join(flags, " ") + "]"
	}
	return fmt.Sprintf("<!-- Input: %s | %s | %s -->", cmtEsc(key), cmtEsc(f.Label), cmtEsc(f.Hint))
}

// cmtEsc keeps a hint inside an XML comment: two hyphens end one, and a hint
// that ends the comment early would take the rest of the card with it.
func cmtEsc(s string) string {
	for strings.Contains(s, "--") {
		s = strings.ReplaceAll(s, "--", "-")
	}
	return strings.TrimSpace(s)
}

// ---- the cards themselves ----------------------------------------------------

// svgField is one thing to ask for: a parameter, what to call it, and what it
// says at the moment.
type svgField struct {
	Key   string
	Label string
	Hint  string
	Val   string
	// written out even when it is left empty. A tier row is: an empty D tier is
	// a board with a D on it and nothing in it, which is a thing people mean.
	Keep bool
	// a list that can hold image files, so the dialog offers to go and find them
	// rather than leaving a path to be typed by hand.
	Logo bool
}

// insFields is what an insert can be asked for before it is placed, and whether
// there is anything to ask at all. A card knows its own parameters; any other
// SVG answers with what it declares, and failing that with what its
// {{placeholders}} name; everything else -- a video, a still, an SVG that is
// just a picture -- has nothing to fill in.
func insFields(path string) ([]svgField, bool) {
	if insKind(path) != "svg" {
		return nil, false
	}
	file, q := insSplit(path)
	if f, ok := insForm(path, q); ok {
		return f, true
	}
	src, err := os.ReadFile(file)
	if err != nil {
		return nil, false
	}
	if h := svgHoles(src); len(h) > 0 {
		return h, true
	}
	return nil, false
}

// insForm is the same question insFields asks, of a card as the dialog has been
// editing it: the card is asked what it wants asked, so what the boxes are is
// the card's business and not the dialog's. A document that is not a card
// answers with what it declares, which is the same list every time -- but the
// values in it are the ones on screen.
func insForm(path string, q svgQuery) ([]svgField, bool) {
	file, _ := insSplit(path)
	src, err := os.ReadFile(file)
	if err != nil {
		return nil, false
	}
	if card, def := svgStamp(src); card != nil {
		return card.form(def.merge(q), src), true
	}
	if f := declaredFields(src, q); len(f) > 0 {
		return f, true
	}
	return nil, false
}

// svgCard is a picture the app draws. draw gets the file's parameters with the
// path's laid over them, the card's folder (what a logo is relative to) and
// the document itself, so a restyled board stays restyled. tmpl is the
// document as shipped. form is what to ask for, handed the document because
// labels and hints are written in it.
type svgCard struct {
	name string
	what string
	tmpl string
	draw func(q svgQuery, dir string, src []byte) []byte
	form func(q svgQuery, src []byte) []svgField
	// note is what a path asked for that the card cannot show, for the log. A
	// card has a fixed amount of room in it, and something typed into a board
	// that has no room left for it is otherwise a change nobody sees.
	note func(q svgQuery) string
}

var svgCardKinds = map[string]*svgCard{
	"tier": {
		name: "tier",
		what: "a tier board: one coloured row per tier, the items in it beside the letter",
		tmpl: tierTemplate,
		draw: tierSVG,
		form: tierForm,
		note: tierNote,
	},
	"badge": {
		name: "badge",
		what: "one big tier letter that flies in, with a caption under it",
		draw: badgeSVG,
		form: badgeForm,
	},
}

// svgSeeds are the cards written into a project's assets folder, so the insert
// chooser has something to pick and a plain SVG viewer has something to show.
// The parameters here are the file's defaults: they are stamped into it, and
// what a path's "?" overrides.
// The badges come first and the board last, and that order is load-bearing: the
// board's own defaults put two of them on it, and a logo is embedded from the
// file, so a board drawn before a.svg exists is a board with the letter missing.
var svgSeeds = []struct {
	file string
	card string
	def  svgQuery
}{
	{"s.svg", "badge", svgQuery{{"label", "S"}}},
	{"a.svg", "badge", svgQuery{{"label", "A"}}},
	{"b.svg", "badge", svgQuery{{"label", "B"}}},
	{"c.svg", "badge", svgQuery{{"label", "C"}}},
	{"d.svg", "badge", svgQuery{{"label", "D"}}},
	{"f.svg", "badge", svgQuery{{"label", "F"}}},
	// the board arrives empty except for two chips that are not there at the
	// start and fly into it a second in -- which is the one thing about these
	// cards that a still cannot show, and is what the Just added line does
	{"tier.svg", "tier", svgQuery{{"new", "A[1.2s]: a.svg, B[1.8s]: b.svg"}}},
}

// writeSVGCards puts the built-in cards in dir, and never over one that is
// already there: these are starting points, and a card the user has edited is
// theirs. Returns the ones it actually wrote, for the log.
func writeSVGCards(dir string) ([]string, error) {
	if err := os.MkdirAll(dir, 0o755); err != nil {
		return nil, err
	}
	var wrote []string
	for _, s := range svgSeeds {
		path := filepath.Join(dir, s.file)
		if exists(path) {
			continue
		}
		card := svgCardKinds[s.card]
		if card == nil {
			continue
		}
		// a card with a template is written as its template, holes and all: that
		// is the file people edit, and it opens as a board either way, since the
		// holes in it are the contents and not the picture. The parameters go
		// into the stamp, so the file still says what it is a picture of. A card
		// without a template is written drawn, as the finished picture it is.
		doc := stampArgs([]byte(card.tmpl), s.def)
		if card.tmpl == "" {
			doc = card.draw(s.def, dir, nil)
		}
		if err := os.WriteFile(path, doc, 0o644); err != nil {
			return wrote, err
		}
		wrote = append(wrote, s.file)
	}
	// and the note on how to write one of these by hand, for whoever -- or
	// whatever -- writes the next card in this folder
	if guide := filepath.Join(dir, cardGuideFile); !exists(guide) {
		if err := os.WriteFile(guide, []byte(cardGuide), 0o644); err != nil {
			return wrote, err
		}
		wrote = append(wrote, cardGuideFile)
	}
	return wrote, nil
}

// svgStamp reads what drew a document and what with, from the root element. A
// file nothing here drew has no stamp, which is the same answer as "not a card".
func svgStamp(src []byte) (*svgCard, svgQuery) {
	dec := xml.NewDecoder(strings.NewReader(string(src)))
	dec.Strict = false
	for {
		tok, err := dec.Token()
		if err != nil {
			return nil, nil
		}
		t, ok := tok.(xml.StartElement)
		if !ok {
			continue
		}
		var card *svgCard
		var def svgQuery
		for _, a := range t.Attr {
			switch xmlName(a.Name) {
			case "data-naivepost":
				card = svgCardKinds[a.Value]
			case "data-naivepost-args":
				def = parseSVGQuery(a.Value)
			}
		}
		return card, def
	}
}

// insSVG is the picture an insert path names: the file, with its parameters
// applied. The note is how it was arrived at, for the log -- a path that asks
// for something the file cannot do has to say so rather than render the file
// unchanged and let the user find out in the video.
func insSVG(path string) (out []byte, note string, err error) {
	file, q := insSplit(path)
	src, err := os.ReadFile(file)
	if err != nil {
		return nil, "", err
	}
	// a card first, parameters or none: the file is a template, and what makes it
	// a picture is being drawn. Handing back the file itself would put {{row.label}}
	// in the video.
	if card, def := svgStamp(src); card != nil {
		q = def.merge(q)
		note := "drawn by the built-in " + card.name + " card"
		if card.note != nil {
			if n := card.note(q); n != "" {
				note += ", except that " + n
			}
		}
		return card.draw(q, filepath.Dir(file), src), note, nil
	}
	if len(q) == 0 {
		return src, "", nil
	}
	if len(svgHoles(src)) > 0 {
		return svgFill(src, q), "filled in", nil
	}
	return src, "has no {{placeholders}} and was not drawn by a card, so its parameters " +
		"were ignored", nil
}

// ---- drawing -----------------------------------------------------------------

// A card is drawn at 1920x1080 whatever the video is: it is scaled to fit the
// footage's frame at render time (clipBox), and an SVG scales without loss.
const (
	cardW    = 1920.0
	cardH    = 1080.0
	cardBG   = "#14171c"
	cardFG   = "#e6e9ef"
	cardInk  = "#16181d" // text on a tier colour, which is always a light one
	cardFont = "DejaVu Sans, Liberation Sans, Helvetica, Arial, sans-serif"
	cardHold = 2.2 // seconds the finished card stands still before the slot ends
)

// How a tier board that has something new on it spends its time: the board so
// far is replayed at tierCatchUp of its own pace, and then the new item gets
// more time on its own than any of it had. A third is about as fast as the
// recap can go and still be followed; below that the rows land on top of each
// other and it reads as a flash rather than as a board being rebuilt.
const (
	tierCatchUp = 0.35 // of its usual pace, for what was already on the board
	tierNewBeat = 0.25 // the board stands still this long before the new item
	tierNewStep = 0.45 // between two new items, on the rare board that has two
	tierNewFly  = 0.6  // the new item's own flight, slower than any of the recap
)

// The classic tier-list palette, hot to cold. A row keeps the colour its letter
// has always had -- S is red, D is green -- and a row named something else takes
// the next colour along, so a board of five reads as a board of five.
var tierPalette = []string{
	"#ff7f7f", "#ffbf7f", "#ffdf7f", "#ffff7f", "#bfff7f", "#7fff7f", "#7fffff", "#bf7fff", "#ff7fbf",
}

func tierColor(label string, i int) string {
	if len([]rune(label)) == 1 {
		if j := strings.Index("SABCDEF", strings.ToUpper(label)); j >= 0 {
			return tierPalette[j]
		}
	}
	return tierPalette[i%len(tierPalette)]
}

type tierRow struct {
	label string
	items []tierItem
}

// tierItem is one thing in a tier: a name, a logo, or both.
//
//	Dust II              a name
//	logos/dust2.png      a logo, recognised by being an image file
//	Dust II|dust2.png    both, the name under the logo
//
// The bar is the only character free to mean "and" here: the comma already
// separates the items, and "=" and "&" are the query's own structure.
type tierItem struct {
	text string
	logo string
}

// written is the item the way it was typed, which is how it goes back into the
// dialog and into the path.
func (it tierItem) written() string {
	switch {
	case it.logo == "":
		return it.text
	case it.text == "":
		return it.logo
	}
	return it.text + "|" + it.logo
}

// tierIsNew answers whether an item is one of the ones the board is being shown
// for. The "new" parameter names them, and it names them loosely on purpose:
// the item on the board is written "Dust II|dust2.png" and what gets typed into
// the New field is "Dust II", because that is what the item is called. So the
// name matches, the logo matches, the two of them together match, and the name a
// bare logo falls back to on the chip matches too -- anything that identifies
// the item to a person identifies it here.
func tierIsNew(names []string, it tierItem) bool {
	for _, n := range names {
		switch {
		case strings.EqualFold(n, it.written()),
			it.text != "" && strings.EqualFold(n, it.text),
			it.logo != "" && strings.EqualFold(n, it.logo),
			it.logo != "" && strings.EqualFold(n, baseName(it.logo)):
			return true
		}
	}
	return false
}

// ---- what the board is being shown for ----------------------------------------
//
// The "new" parameter lists the arrivals the shot is about -- which row, when,
// what:
//
//	Mirage                        an item already on the board, in turn
//	D[1.1s]: logos/bla.svg, Test  into row D, 1.1 s in, the logo with Test under it
//	S[2s]: Nuke, D: Vertigo       two arrivals, each in its own row
//
// Commas separate as in a row's items; a prefix starts a new entry. A logo and
// a name beside each other are one chip. A row named but not on the board is
// added.
type tierNew struct {
	tier string  // "" is an item already on the board somewhere, found by name
	at   float64 // when it starts, in seconds; below zero is "after the recap"
	item tierItem
}

// the prefix that starts an arrival: a tier, a time in brackets, or both, and
// then a colon. Either part may be left out, so "[1.1s]: Mirage" times an item
// that is already up there and "D: Nuke" leaves the timing to the card.
var reNewEntry = regexp.MustCompile(`^\s*([^\[\]:]*?)\s*(?:\[\s*([^\]]*?)\s*\])?\s*:\s*(.*)$`)

func parseNew(s string) []tierNew {
	var out []tierNew
	tier, at, open := "", -1.0, false
	for _, p := range strings.Split(s, ",") {
		if p = strings.TrimSpace(p); p == "" {
			continue
		}
		fresh := false
		if m := reNewEntry.FindStringSubmatch(p); m != nil && (m[1] != "" || m[2] != "") {
			tier, at, open, fresh = m[1], -1, true, true
			if d, ok := parseDelay(m[2]); ok {
				at = d
			}
			if p = strings.TrimSpace(m[3]); p == "" {
				continue // "D[1.1s]:" on its own; what lands follows after the comma
			}
		} else if !open {
			tier, at = "", -1
		}
		its := splitItems(p)
		if len(its) == 0 {
			continue
		}
		it := its[0]
		// beside the one before it and filling the half it left empty: one chip.
		// A part that would overwrite something is the next chip instead.
		if n := len(out); !fresh && open && n > 0 &&
			out[n-1].tier == tier && out[n-1].at == at && mergeItem(&out[n-1].item, it) {
			continue
		}
		out = append(out, tierNew{tier: tier, at: at, item: it})
	}
	return out
}

// mergeItem puts an item into the one before it when the two are halves of the
// same chip -- a logo and a name -- and says so. Anything else is a chip of its
// own and this leaves both alone.
func mergeItem(into *tierItem, it tierItem) bool {
	switch {
	case into.text == "" && it.text != "" && it.logo == "":
		into.text = it.text
	case into.logo == "" && it.logo != "" && it.text == "":
		into.logo = it.logo
	default:
		return false
	}
	return true
}

// parseDelay reads the moment in the brackets. Milliseconds are spelled out
// because that is how a video editor counts small waits; a bare number is
// seconds, which is how everything else on a card is written.
func parseDelay(s string) (float64, bool) {
	s = strings.ToLower(strings.TrimSpace(s))
	mul := 1.0
	switch {
	case strings.HasSuffix(s, "ms"):
		s, mul = strings.TrimSpace(strings.TrimSuffix(s, "ms")), 0.001
	case strings.HasSuffix(s, "s"):
		s = strings.TrimSpace(strings.TrimSuffix(s, "s"))
	}
	v, err := strconv.ParseFloat(s, 64)
	if err != nil || v < 0 {
		return 0, false
	}
	return v * mul, true
}

// tierBoard is the board as it will be drawn: the six tiers with their
// contents, and which place is arriving now with its moment (below zero for
// "in turn"). An arrival names a tier and takes its first free place; naming
// something already up there singles it out. A tier not on the board or with
// no room is said in tierNote.
func tierBoard(q svgQuery) ([]tierRow, map[[2]int]float64) {
	rows := tierRowsOf(q)
	arrive := map[[2]int]float64{}
	for _, n := range parseNew(q.get("new")) {
		if n.tier == "" {
			for i, r := range rows {
				for j, it := range r.items {
					if tierIsNew([]string{n.item.written()}, it) {
						arrive[[2]int{i, j}] = n.at
					}
				}
			}
			continue
		}
		i := rowOf(rows, n.tier)
		if i < 0 {
			continue // the board is S A B C D F; there is no row to add
		}
		j := itemOf(rows[i].items, n.item)
		if j < 0 {
			if len(rows[i].items) >= tierSlots {
				continue // the tier is full, and a seventh chip is one nobody sees
			}
			rows[i].items = append(rows[i].items, n.item)
			j = len(rows[i].items) - 1
		}
		arrive[[2]int{i, j}] = n.at
	}
	return rows, arrive
}

func rowOf(rows []tierRow, label string) int {
	for i, r := range rows {
		if strings.EqualFold(r.label, label) {
			return i
		}
	}
	return -1
}

// itemOf finds a chip already in the row, so that saying where something goes
// twice -- once in the row, once in the list of arrivals -- puts it there once.
func itemOf(items []tierItem, it tierItem) int {
	for j, have := range items {
		switch {
		case strings.EqualFold(have.written(), it.written()),
			have.text != "" && strings.EqualFold(have.text, it.text),
			have.logo != "" && strings.EqualFold(have.logo, it.logo):
			return j
		}
	}
	return -1
}

// tierRowsOf reads the board out of the parameters. The board is fixed -- S, A,
// B, C, D and F, with six places in each -- because that is what tier.svg has
// written out in it, so a parameter is a tier only if it is one of those, and a
// seventh thing in a tier is a thing there is nowhere to put. An empty query is
// the empty board, which is what the file in assets shows.
func tierRowsOf(q svgQuery) []tierRow {
	rows := make([]tierRow, 0, len(tierLetters))
	for _, l := range tierLetters {
		items := splitItems(q.get(l))
		if len(items) > tierSlots {
			items = items[:tierSlots]
		}
		rows = append(rows, tierRow{label: l, items: items})
	}
	return rows
}

func splitItems(s string) []tierItem {
	var out []tierItem
	for _, p := range splitLabels(s) {
		it := tierItem{text: p}
		if i := strings.IndexByte(p, '|'); i >= 0 {
			it = tierItem{text: strings.TrimSpace(p[:i]), logo: strings.TrimSpace(p[i+1:])}
		} else if isImageFile(p) {
			it = tierItem{logo: p}
		}
		if it.text == "" && it.logo == "" {
			continue
		}
		out = append(out, it)
	}
	return out
}

// splitLabels is the one list syntax on a card: comma separated, spaces around
// the commas ignored, nothing for the empties. What is in a tier is written this
// way, and so is what has just arrived on the board.
func splitLabels(s string) []string {
	var out []string
	for _, p := range strings.Split(s, ",") {
		if p = strings.TrimSpace(p); p != "" {
			out = append(out, p)
		}
	}
	return out
}

func joinItems(items []tierItem) string {
	var out []string
	for _, it := range items {
		out = append(out, it.written())
	}
	return strings.Join(out, ", ")
}

// tierInputs is the board's form as this package knows it, for a file that does
// not say: a board drawn before any of this existed, or one whose declarations
// have been taken out. The template in svgtier.go carries the same list, one
// line per tier written out beside the tier it belongs to, and that is the copy
// that is actually read.
var tierInputs = tierBuiltIn()

func tierBuiltIn() []svgField {
	f := []svgField{{Key: "title", Label: "Title", Hint: "over the board — leave it empty for none"}}
	for _, l := range tierLetters {
		f = append(f, svgField{
			Key: l, Label: "Tier " + l, Keep: true, Logo: true,
			Hint: "up to six, comma separated: a name, a logo file, or Name|logo.png",
		})
	}
	// The board is usually shown because one more thing has just been ranked,
	// and that is the thing the shot is about: the rest of the board is caught up
	// on quickly and the arrivals fly in on their own. Empty is the board as it
	// always was, every row at one pace.
	return append(f, svgField{
		Key: "new", Label: "Just added",
		Hint: "what this board is being shown for, comma separated: " +
			"D[1.1s]: logo.svg, Name — into row D, 1.1 s in. A bare name is an item " +
			"already above, which flies in after the recap",
	})
}

// tierForm asks the file what the board's boxes are called: the file declares
// one line per tier, in the order the tiers are on it, and this fills each one
// in with what is in that tier at the moment.
func tierForm(q svgQuery, src []byte) []svgField {
	var f []svgField
	for _, in := range cardInputs(src, tierInputs) {
		in.Val = q.get(in.Key)
		if hasLetter(in.Key) {
			// tidied on the way through: what was typed into the tier, not what
			// the arrivals put in it -- those are written in the Just added line
			// and belong to it until they are moved
			in.Val = joinItems(splitItems(in.Val))
		}
		f = append(f, in)
	}
	// a hole somebody added to their own copy of the card is a parameter like any
	// other: they wrote {{subtitle}} into the file, so the dialog asks for a
	// subtitle. The card's own holes are not offered -- those are filled from the
	// arithmetic, and a box that overrides {{s1.name.size}} is a name that no
	// longer fits the place it is in.
	for _, h := range svgHoles(src) {
		if !cardOwnHole(h.Key) && fieldOf(f, h.Key) < 0 {
			h.Val = fieldVal(h.Key, q, nil)
			f = append(f, h)
		}
	}
	return f
}

// cardOwnHole is a hole the card fills in itself. They are the ones with a dot
// in the name -- s1.name.size, d3.fade.values -- plus the card's own length,
// which every animation in it is measured against.
func cardOwnHole(key string) bool {
	return strings.Contains(key, ".") || strings.EqualFold(key, "total")
}

// ---- the animation a card is written in --------------------------------------
//
// Every animation begins at 0 and runs the card's whole length, the waiting in
// keyTimes rather than begin: before an animation begins the static attribute
// stands, so the static document is the FINISHED card and a viewer that
// ignores SMIL (thumbnailer, chooser preview, the fallback) shows the
// completed board.

// animStop is one value and the moment the card reaches it.
type animStop struct {
	at  float64
	val string
}

// cardAnim writes one animation element: stops in seconds, turned into values
// and keyTimes over the card's whole length. Held before the first stop and
// after the last, so a row waits its turn and then stays where it landed.
func cardAnim(b *strings.Builder, pad, attr, kind string, total float64, stops ...animStop) {
	vals, times := animPair(total, stops...)
	if vals == "" {
		return
	}
	el, extra := "animate", ""
	if kind != "" {
		el, extra = "animateTransform", fmt.Sprintf(" type=%q", kind)
	}
	fmt.Fprintf(b, "\n%s<%s attributeName=%q%s values=%q keyTimes=%q dur=%q fill=\"freeze\"/>",
		pad, el, attr, extra, vals, times, secStr(total))
}

// animPair is that same schedule as the two strings an <animate> is made of.
// A card written as a template holds the element and gets these handed to it,
// which is what puts a moment somebody typed -- D[1.1s] -- into the document as
// a number instead of leaving it in a printf in here.
func animPair(total float64, stops ...animStop) (values, keyTimes string) {
	if total <= 0 || len(stops) == 0 {
		return "", ""
	}
	full := stops
	if full[0].at > 0 {
		full = append([]animStop{{0, full[0].val}}, full...)
	}
	if last := full[len(full)-1]; last.at < total {
		full = append(append([]animStop{}, full...), animStop{total, last.val})
	}
	vs := make([]string, len(full))
	ts := make([]string, len(full))
	for i, st := range full {
		vs[i] = st.val
		ts[i] = trimNum(math.Min(1, math.Max(0, st.at/total)))
	}
	return strings.Join(vs, ";"), strings.Join(ts, ";")
}

// flyIn is the movement every card is made of: in from one side, overshooting a
// little and settling back, while it fades up. dx is where it comes from.
func flyIn(b *strings.Builder, pad string, total, at, dur, dx float64) {
	cardAnim(b, pad, "opacity", "", total,
		animStop{at, "0"}, animStop{at + math.Min(0.25, dur/2), "1"})
	over := -dx * 0.06 // the overshoot, always a little way past where it lands
	cardAnim(b, pad, "transform", "translate", total,
		animStop{at, fmt.Sprintf("%s 0", trimNum(dx))},
		animStop{at + dur*0.66, fmt.Sprintf("%s 0", trimNum(over))},
		animStop{at + dur, "0 0"})
}

// ---- logos -------------------------------------------------------------------

// The extensions an item is recognised as a logo by when it is written on its
// own. With a name in front of it -- "Dust II|whatever" -- the bar has already
// said which half is which and this is not consulted.
var logoExt = map[string]string{
	".png":  "image/png",
	".jpg":  "image/jpeg",
	".jpeg": "image/jpeg",
	".webp": "image/webp",
	".gif":  "image/gif",
	".bmp":  "image/bmp",
	".svg":  "image/svg+xml",
}

func isImageFile(p string) bool {
	_, ok := logoExt[strings.ToLower(filepath.Ext(p))]
	return ok
}

// cardLogo embeds a logo's bytes base64 in the href: baked frames are written
// elsewhere, so a relative href resolves against nothing and an absolute one
// is refused. Looked for beside the card, then in the folder above (the
// project).
func cardLogo(dir, p string) (string, bool) {
	if strings.TrimSpace(p) == "" {
		return "", false
	}
	tries := []string{p}
	if !filepath.IsAbs(p) && dir != "" {
		tries = []string{filepath.Join(dir, p), filepath.Join(dir, "..", p), p}
	}
	for _, t := range tries {
		b, err := os.ReadFile(t)
		if err != nil || len(b) == 0 {
			continue
		}
		mime := logoMime(b, t)
		if mime == "" {
			return "", false // readable, but not a picture: better a name than a hole
		}
		return "data:" + mime + ";base64," + base64.StdEncoding.EncodeToString(b), true
	}
	return "", false
}

// logoMime asks the bytes first and the name second: a logo dragged out of a
// download is as likely to be called "logo" as "logo.png", and a file named
// .png that is not one would be a chip drawn as nothing.
func logoMime(b []byte, path string) string {
	switch {
	case bytes.HasPrefix(b, []byte("\x89PNG\r\n\x1a\n")):
		return "image/png"
	case bytes.HasPrefix(b, []byte{0xff, 0xd8, 0xff}):
		return "image/jpeg"
	case bytes.HasPrefix(b, []byte("GIF8")):
		return "image/gif"
	case bytes.HasPrefix(b, []byte("BM")):
		return "image/bmp"
	case bytes.HasPrefix(b, []byte("RIFF")) && len(b) > 12 && bytes.Equal(b[8:12], []byte("WEBP")):
		return "image/webp"
	case bytes.Contains(b[:min(len(b), 1024)], []byte("<svg")):
		return "image/svg+xml"
	}
	return logoExt[strings.ToLower(filepath.Ext(path))]
}

// ---- drawing -----------------------------------------------------------------

// badgeInputs is the badge's form, and what a badge file says about itself: one
// letter and one caption, however it is filled in.
var badgeInputs = []svgField{
	{Key: "label", Label: "Letter", Hint: "the big letter on the card"},
	{Key: "item", Label: "Caption", Hint: "what earned it — under the badge"},
	{Key: "color", Label: "Colour", Hint: "#rrggbb; empty follows the letter"},
}

func badgeForm(q svgQuery, src []byte) []svgField {
	var f []svgField
	for _, in := range cardInputs(src, badgeInputs) {
		in.Val = q.get(in.Key)
		f = append(f, in)
	}
	return f
}

var hexColor = regexp.MustCompile(`^#[0-9a-fA-F]{3,8}$`)

// badgeSVG is the single-letter card: one tier's badge flying in from the left,
// overshooting and settling, with the caption arriving after it has landed. It
// is the card for calling a verdict out one at a time -- s.svg, a.svg and the
// rest are this one card with the letter stamped into them.
func badgeSVG(q svgQuery, _ string, _ []byte) []byte {
	label := strings.TrimSpace(q.get("label"))
	item := strings.TrimSpace(q.get("item"))
	col := strings.TrimSpace(q.get("color"))
	if !hexColor.MatchString(col) {
		col = tierColor(label, 0)
	}

	boxW, boxH := 440.0, 440.0
	cx, cy := cardW/2, cardH/2
	if item != "" {
		cy -= 90 // room under it for the caption
	}
	total := 0.75
	if item != "" {
		total = 0.97
	}
	total += cardHold

	var b strings.Builder
	b.WriteString("\n  <g>")
	flyIn(&b, "    ", total, 0.1, 0.65, -(cardW/2 + boxW))
	fmt.Fprintf(&b, "\n    <rect x=\"%s\" y=\"%s\" width=\"%s\" height=\"%s\" rx=\"52\" fill=%q/>",
		trimNum(cx-boxW/2), trimNum(cy-boxH/2), trimNum(boxW), trimNum(boxH), col)
	fs := fitFont(label, boxW-80, 300)
	fmt.Fprintf(&b, "\n    <text x=\"%s\" y=\"%s\" font-family=%q font-size=\"%s\" font-weight=\"700\""+
		" fill=%q text-anchor=\"middle\">%s</text>\n  </g>",
		trimNum(cx), trimNum(cy+fs*0.35), cardFont, trimNum(fs), cardInk, escapeText(label))

	if item != "" {
		cfs := fitFont(item, cardW-240, 96)
		b.WriteString("\n  <g>")
		cardAnim(&b, "    ", "opacity", "", total, animStop{0.62, "0"}, animStop{0.97, "1"})
		cardAnim(&b, "    ", "transform", "translate", total,
			animStop{0.62, "0 48"}, animStop{0.97, "0 0"})
		fmt.Fprintf(&b, "\n    <text x=\"%s\" y=\"%s\" font-family=%q font-size=\"%s\" font-weight=\"600\""+
			" fill=%q text-anchor=\"middle\">%s</text>\n  </g>",
			trimNum(cx), trimNum(cy+boxH/2+140), cardFont, trimNum(cfs), cardFG, escapeText(item))
	}
	return cardDoc("badge", q, badgeInputs, b.String())
}

// cardDoc wraps a card's body in the document every card has: frame,
// background, the stamp saying what drew it and with what (insSVG reads it
// back), and the card's form as Input comments, which the dialog is built
// from. Comments, so no renderer sees them and the baker drops them.
func cardDoc(name string, args svgQuery, inputs []svgField, body string) []byte {
	var b strings.Builder
	b.WriteString(`<?xml version="1.0" encoding="UTF-8"?>` + "\n")
	fmt.Fprintf(&b, `<svg xmlns="http://www.w3.org/2000/svg" width="%s" height="%s" viewBox="0 0 %s %s"`+
		" data-naivepost=%q data-naivepost-args=\"%s\">",
		trimNum(cardW), trimNum(cardH), trimNum(cardW), trimNum(cardH), name, attrEsc(args.args()))
	for _, in := range inputs {
		b.WriteString("\n  " + inputDecl(in))
	}
	fmt.Fprintf(&b, "\n  <rect width=\"%s\" height=\"%s\" fill=%q/>", trimNum(cardW), trimNum(cardH), cardBG)
	b.WriteString(body)
	b.WriteString("\n</svg>\n")
	return []byte(b.String())
}

// trimNum writes a coordinate the short way: SVG takes "120" and "120.4" and
// not "1.204e+02", and a card is several hundred numbers.
func trimNum(f float64) string {
	s := strconv.FormatFloat(f, 'f', 2, 64)
	s = strings.TrimSuffix(strings.TrimRight(s, "0"), ".")
	if s == "" || s == "-" {
		return "0"
	}
	return s
}

// fitFont is a font size that keeps s inside w. SVG cannot measure text without
// a renderer, so this uses the one number that holds across sans-serif faces --
// an average advance of a bit over half an em -- and errs small: a card whose
// text is a size too modest still reads, and one whose text runs off the chip
// does not.
func fitFont(s string, w, max float64) float64 {
	n := float64(len([]rune(strings.TrimSpace(s))))
	if n == 0 {
		return max
	}
	if f := w / (0.58 * n); f < max {
		return math.Max(f, 12)
	}
	return max
}

// secStr writes a SMIL clock value without an exponent or a trailing run of
// zeroes -- "0.15s", not "1.5e-01s", which clockValue would refuse.
func secStr(f float64) string {
	return strings.TrimSuffix(strings.TrimRight(fmt.Sprintf("%.3f", f), "0"), ".") + "s"
}

var attrEscaper = strings.NewReplacer("&", "&amp;", "<", "&lt;", ">", "&gt;", `"`, "&quot;")

func attrEsc(s string) string { return attrEscaper.Replace(s) }
