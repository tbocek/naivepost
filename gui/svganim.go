package main

// Animated SVG, without a browser. Nothing in the render path plays SMIL
// (librsvg, Inkscape and resvg all render t=0), so it is EVALUATED: read the
// animation elements, compute every animated attribute at t, write a static
// document with those values baked in, once per frame; ffmpeg reads the
// sequence as stills.
//
// Supported: <animate>, <set>, <animateTransform> on numbers, number lists,
// lengths with units, hex/rgb() colours; from/to/by, values with keyTimes,
// dur, begin, repeatCount (number or indefinite), fill="freeze"; calcMode
// discrete steps, paced and spline read as linear. CSS @keyframes is folded in
// by svgcss.go. Not read: begin chained off another animation or an event, and
// elements targeting something other than their parent -- both render static.

import (
	"encoding/xml"
	"fmt"
	"io"
	"math"
	"os"
	"path/filepath"
	"strconv"
	"strings"
)

// svgFPS is the rate an insert's baked sequence is rendered at when the render
// settings do not name one. Animation is the only reason a still becomes a
// sequence at all, so this only has to be smooth, not to match a camera.
const svgFPS = 25

// svgNode is the document as read, with the animation elements pulled out of
// the children and hung on the element they animate. Attrs keeps the original
// order and spelling: what comes out of here has to be the same document with
// different numbers in it, not a normalized rewrite of it.
type svgNode struct {
	name  string
	attrs []xml.Attr
	kids  []*svgNode
	text  string // character data; only leaves have any that matters
	anims []*svgAnim
	// self-closing in the source. Preserved because an empty <rect/> written as
	// <rect></rect> is the same document, but a reader diffing two baked frames
	// against the original should see only the numbers move.
	empty bool
}

// svgAnim is one animation element, already parsed into the four things
// evaluating it needs: what it animates, what the values are, when it runs, and
// whether it holds its last value afterwards.
type svgAnim struct {
	attr      string   // attributeName
	transform string   // animateTransform's type; empty for a plain animate
	values    []string // at least two once from/to/by has been folded in
	keyTimes  []float64
	begin     float64
	dur       float64
	repeat    float64 // how many times; math.Inf(1) for indefinite
	freeze    bool
	discrete  bool
	additive  bool // additive="sum": the value adds to the static attribute
	// the two CSS brought with it (svgcss.go). SMIL leaves both zero: an
	// animation with no easing is linear, and one that has not begun yet leaves
	// the static value alone.
	backfill bool     // fill-mode backwards: hold the first value before it begins
	ease     *cssEase // nil is linear, which is every SMIL animation
}

// svgAnimated reports whether a document has anything to bake. Cheap and
// deliberately textual: the caller uses it to decide whether an insert is a
// still or a sequence, and a document with no SMIL in it should not be parsed
// twice to find that out.
func svgAnimated(b []byte) bool {
	s := string(b)
	for _, tag := range []string{"<animate", "<set", "<animateTransform", "<animateMotion"} {
		if strings.Contains(s, tag) {
			return true
		}
	}
	return svgCSSAnimated(b)
}

// svgHasCSSAnimation spots a document that means to animate with CSS. Its use is
// the leftover case: something that asks for an animation whose @keyframes are
// not in the file, which svgAnimated therefore refuses to bake. Worth naming
// rather than silently rendering frame 1 nine hundred times -- a card that does
// not move is a bug report, and this turns it into a line in the log.
func svgHasCSSAnimation(b []byte) bool {
	s := string(b)
	return strings.Contains(s, "@keyframes") || strings.Contains(s, "animation:") ||
		strings.Contains(s, "animation-name")
}

// parseSVG reads a document into svgNodes, hanging every animation element on
// its parent rather than keeping it as a child. Unknown elements, comments and
// processing instructions come through as they were: this is a rewriter, not a
// validator, and an SVG it does not understand must still come out the far side.
func parseSVG(b []byte) (*svgNode, error) {
	dec := xml.NewDecoder(strings.NewReader(string(b)))
	dec.Strict = false
	var root *svgNode
	var stack []*svgNode
	for {
		tok, err := dec.Token()
		if err == io.EOF {
			break
		}
		if err != nil {
			return nil, err
		}
		switch t := tok.(type) {
		case xml.StartElement:
			n := &svgNode{name: xmlName(t.Name), attrs: t.Attr}
			if len(stack) > 0 {
				stack[len(stack)-1].kids = append(stack[len(stack)-1].kids, n)
			} else if root == nil {
				root = n
			}
			stack = append(stack, n)
		case xml.EndElement:
			if len(stack) == 0 {
				continue
			}
			n := stack[len(stack)-1]
			stack = stack[:len(stack)-1]
			n.empty = len(n.kids) == 0 && strings.TrimSpace(n.text) == ""
			// an animation element belongs to whoever it is inside, and stops
			// being a child of it: the baked document must not carry the SMIL
			// that produced it, or a renderer that DOES understand SMIL would
			// animate an already-animated frame
			if an := asAnim(n); an != nil && len(stack) > 0 {
				p := stack[len(stack)-1]
				p.kids = p.kids[:len(p.kids)-1]
				p.anims = append(p.anims, an)
			}
		case xml.CharData:
			if len(stack) > 0 {
				stack[len(stack)-1].text += string(t)
			}
		}
	}
	if root == nil {
		return nil, fmt.Errorf("no element in the document")
	}
	// the stylesheet's animations become svgAnims here, so that everything past
	// this point -- the duration, the frames, the still-or-sequence decision --
	// sees one kind of animation and not two
	applyCSSAnimations(root)
	return root, nil
}

// xmlName puts a namespaced name back the way it was written. encoding/xml
// resolves prefixes to URIs, which is right for reading and useless for writing
// the same file back -- an href that came in as xlink:href has to go out as
// xlink:href or every renderer stops resolving it.
func xmlName(n xml.Name) string {
	switch n.Space {
	case "", "http://www.w3.org/2000/svg":
		return n.Local
	case "http://www.w3.org/1999/xlink":
		return "xlink:" + n.Local
	case "http://www.w3.org/XML/1998/namespace":
		return "xml:" + n.Local
	case "xmlns":
		return "xmlns:" + n.Local
	default:
		return n.Local
	}
}

// asAnim recognizes an animation element and reads it, or returns nil for an
// ordinary one. An animation this cannot evaluate -- no attributeName, no
// values, a begin that waits on an event -- is nil too, so it is dropped from
// the output rather than baked wrong.
func asAnim(n *svgNode) *svgAnim {
	kind := n.name
	if i := strings.IndexByte(kind, ':'); i >= 0 {
		kind = kind[i+1:]
	}
	switch kind {
	case "animate", "set", "animateTransform":
	default:
		return nil
	}
	at := map[string]string{}
	for _, a := range n.attrs {
		at[xmlName(a.Name)] = a.Value
	}
	an := &svgAnim{
		attr:      at["attributeName"],
		transform: at["type"],
		freeze:    at["fill"] == "freeze" || kind == "set",
		discrete:  at["calcMode"] == "discrete" || kind == "set",
		additive:  at["additive"] == "sum",
		repeat:    1,
	}
	if an.attr == "" {
		return nil
	}
	if kind != "animateTransform" {
		an.transform = ""
	}
	// a begin this cannot resolve -- "click", "other.end+1s" -- is not a
	// timing bug to guess at; the element is dropped and the document renders
	// as if it were not there
	var ok bool
	if an.begin, ok = clockValue(at["begin"], 0); !ok {
		return nil
	}
	if an.dur, ok = clockValue(at["dur"], 0); !ok {
		return nil
	}
	switch r := strings.TrimSpace(at["repeatCount"]); {
	case r == "":
	case r == "indefinite":
		an.repeat = math.Inf(1)
	default:
		f, err := strconv.ParseFloat(r, 64)
		if err != nil || f <= 0 {
			return nil
		}
		an.repeat = f
	}

	switch {
	case at["values"] != "":
		for _, v := range strings.Split(at["values"], ";") {
			an.values = append(an.values, strings.TrimSpace(v))
		}
	case at["to"] != "" && at["from"] != "":
		an.values = []string{at["from"], at["to"]}
	case at["to"] != "":
		// no from: SMIL says the underlying value, which is the static
		// attribute. A sentinel rather than a value, because the element it
		// belongs to is not in hand here.
		an.values = []string{"", at["to"]}
	case at["by"] != "":
		an.values = []string{"", "+" + at["by"]}
	default:
		return nil
	}
	if len(an.values) == 1 { // a one-value list is a set, whatever it was written as
		an.values = append(an.values, an.values[0])
		an.discrete = true
	}
	if kt := at["keyTimes"]; kt != "" {
		for _, s := range strings.Split(kt, ";") {
			f, err := strconv.ParseFloat(strings.TrimSpace(s), 64)
			if err != nil {
				an.keyTimes = nil
				break
			}
			an.keyTimes = append(an.keyTimes, f)
		}
		if len(an.keyTimes) != len(an.values) {
			an.keyTimes = nil // mismatched: even spacing is better than a wrong map
		}
	}
	// a zero dur with more than one value has no way to get from the first to
	// the last; treat it as a set to the last, which is what fill="freeze"
	// would have produced a moment later anyway
	if an.dur <= 0 {
		an.dur = 0
		an.discrete = true
	}
	return an
}

// clockValue reads SMIL's clock syntax -- "2s", "500ms", "1.5", "min:sec" --
// and says whether it could. An empty value is the default rather than a
// failure; anything else unreadable is a failure, because guessing at a begin
// puts the animation somewhere the author did not ask for.
func clockValue(s string, def float64) (float64, bool) {
	s = strings.TrimSpace(s)
	if s == "" {
		return def, true
	}
	if s == "indefinite" {
		return math.Inf(1), true
	}
	if strings.Contains(s, ":") { // [hh:]mm:ss
		var total float64
		for _, p := range strings.Split(s, ":") {
			f, err := strconv.ParseFloat(strings.TrimSpace(p), 64)
			if err != nil {
				return 0, false
			}
			total = total*60 + f
		}
		return total, true
	}
	mul := 1.0
	switch {
	case strings.HasSuffix(s, "ms"):
		s, mul = strings.TrimSuffix(s, "ms"), 0.001
	case strings.HasSuffix(s, "s"):
		s = strings.TrimSuffix(s, "s")
	case strings.HasSuffix(s, "min"):
		s, mul = strings.TrimSuffix(s, "min"), 60
	case strings.HasSuffix(s, "h"):
		s, mul = strings.TrimSuffix(s, "h"), 3600
	}
	f, err := strconv.ParseFloat(strings.TrimSpace(s), 64)
	if err != nil {
		return 0, false
	}
	return f * mul, true
}

// valueAt is the animation's value at time t, and whether it has one at all --
// before it begins, or after it ends without freezing, it does not, and the
// static attribute stands.
func (an *svgAnim) valueAt(t float64, base string) (string, bool) {
	local := t - an.begin
	if local < 0 {
		// CSS fill-mode backwards: the first value is worn during the delay,
		// which is what makes a staggered fly-in start off screen rather than
		// standing where it lands until its turn comes
		if !an.backfill {
			return "", false
		}
		local = 0
	}
	span := an.dur * an.repeat // +Inf for an indefinite repeat, which is the point
	if an.dur > 0 && local >= span {
		if !an.freeze {
			return "", false
		}
		local = an.dur // the last value, held
	} else if an.dur > 0 {
		local = math.Mod(local, an.dur)
		if local == 0 && t > an.begin {
			local = 0 // a repeat boundary is the start of the next run
		}
	} else {
		local = 0
	}

	f := 0.0
	if an.dur > 0 {
		f = math.Min(1, local/an.dur)
	} else if t >= an.begin {
		f = 1
	}

	vals := make([]string, len(an.values))
	copy(vals, an.values)
	// "" is the underlying value: a to-animation with no from, or a by. Folding
	// it in here rather than at parse time is what lets one <animate> be read
	// once and applied to whatever it is attached to.
	for i, v := range vals {
		switch {
		case v == "":
			vals[i] = base
		case strings.HasPrefix(v, "+"):
			vals[i] = addValues(base, strings.TrimPrefix(v, "+"))
		}
	}

	times := an.keyTimes
	if times == nil {
		times = make([]float64, len(vals))
		for i := range vals {
			times[i] = float64(i) / float64(len(vals)-1)
		}
	}
	i := len(vals) - 1
	for k := 0; k+1 < len(times); k++ {
		if f < times[k+1] || times[k+1] == times[k] {
			i = k
			break
		}
	}
	if i >= len(vals)-1 {
		return vals[len(vals)-1], true
	}
	if an.discrete {
		return vals[i], true
	}
	span2 := times[i+1] - times[i]
	local2 := 0.0
	if span2 > 0 {
		local2 = (f - times[i]) / span2
	}
	// the easing is applied inside the segment, which is where CSS applies it:
	// a timing function runs between one keyframe and the next, not once across
	// the whole animation
	return lerpValue(vals[i], vals[i+1], an.ease.at(math.Min(1, math.Max(0, local2)))), true
}

// ---- interpolating an attribute value ---------------------------------------
//
// Every attribute shape -- a bare number, a number with a unit, a list, a
// colour -- interpolates the same way once the numbers are found, so the
// parser is deliberately dumb: pull the numbers out, keep the separators, put
// new numbers back. Anything with no numbers steps rather than slides.

// numsOf splits a value into its numbers and the text between them, so
// rebuilding is exact where nothing changed.
func numsOf(s string) (nums []float64, seps []string) {
	i := 0
	for i < len(s) {
		j := i
		for j < len(s) && !startsNumber(s, j) {
			j++
		}
		seps = append(seps, s[i:j])
		if j >= len(s) {
			return nums, seps
		}
		k := j
		if s[k] == '+' || s[k] == '-' {
			k++
		}
		for k < len(s) && (s[k] >= '0' && s[k] <= '9' || s[k] == '.') {
			k++
		}
		if k < len(s) && (s[k] == 'e' || s[k] == 'E') {
			e := k + 1
			if e < len(s) && (s[e] == '+' || s[e] == '-') {
				e++
			}
			if e < len(s) && s[e] >= '0' && s[e] <= '9' {
				for e < len(s) && s[e] >= '0' && s[e] <= '9' {
					e++
				}
				k = e
			}
		}
		f, err := strconv.ParseFloat(s[j:k], 64)
		if err != nil {
			seps[len(seps)-1] += s[j:k]
			i = k
			continue
		}
		nums = append(nums, f)
		i = k
	}
	return nums, seps
}

func startsNumber(s string, i int) bool {
	c := s[i]
	if c >= '0' && c <= '9' {
		return true
	}
	if c != '.' && c != '-' && c != '+' {
		return false
	}
	// a sign or a dot only starts a number if a digit follows, so "e-" in a
	// colour name or "-" in a class does not
	if i+1 < len(s) && s[i+1] >= '0' && s[i+1] <= '9' {
		return true
	}
	return c != '.' && i+1 < len(s) && s[i+1] == '.' && i+2 < len(s) && s[i+2] >= '0' && s[i+2] <= '9'
}

func rebuild(nums []float64, seps []string) string {
	var b strings.Builder
	for i, sep := range seps {
		b.WriteString(sep)
		if i < len(nums) {
			b.WriteString(fmtNum(nums[i]))
		}
	}
	return b.String()
}

// fmtNum writes a number the way a hand-written SVG does: short, and without an
// exponent, which some renderers accept and none need here.
func fmtNum(f float64) string {
	if f == math.Trunc(f) && math.Abs(f) < 1e15 {
		return strconv.FormatFloat(f, 'f', -1, 64)
	}
	return strconv.FormatFloat(f, 'f', 4, 64)
}

// lerpValue interpolates between two attribute values. Hex colours first, since
// "#ff0000" has no numbers in it a reader would find; then the general shape.
// Two values whose numbers do not line up cannot be blended and step instead --
// wrong in a way that shows as a jump rather than as a mangled attribute.
func lerpValue(from, to string, f float64) string {
	if a, ok := parseHexColor(from); ok {
		if b, ok := parseHexColor(to); ok {
			return fmt.Sprintf("#%02x%02x%02x",
				lerpByte(a[0], b[0], f), lerpByte(a[1], b[1], f), lerpByte(a[2], b[2], f))
		}
	}
	an, seps := numsOf(from)
	bn, _ := numsOf(to)
	if len(an) == 0 || len(an) != len(bn) {
		if f >= 1 {
			return to
		}
		return from
	}
	out := make([]float64, len(an))
	for i := range an {
		out[i] = an[i] + (bn[i]-an[i])*f
	}
	return rebuild(out, seps)
}

// addValues is what by-animation and additive="sum" need: the numbers added
// position by position, everything else kept from the base.
func addValues(base, delta string) string {
	bn, seps := numsOf(base)
	dn, dseps := numsOf(delta)
	if len(bn) == 0 {
		return delta
	}
	if len(bn) != len(dn) {
		if len(dn) == 0 {
			return base
		}
		return rebuild(dn, dseps)
	}
	out := make([]float64, len(bn))
	for i := range bn {
		out[i] = bn[i] + dn[i]
	}
	return rebuild(out, seps)
}

func lerpByte(a, b uint8, f float64) uint8 {
	return uint8(math.Round(float64(a) + (float64(b)-float64(a))*f))
}

func parseHexColor(s string) ([3]uint8, bool) {
	s = strings.TrimSpace(s)
	if !strings.HasPrefix(s, "#") {
		return [3]uint8{}, false
	}
	h := s[1:]
	if len(h) == 3 {
		h = string([]byte{h[0], h[0], h[1], h[1], h[2], h[2]})
	}
	if len(h) != 6 {
		return [3]uint8{}, false
	}
	var out [3]uint8
	for i := 0; i < 3; i++ {
		v, err := strconv.ParseUint(h[i*2:i*2+2], 16, 8)
		if err != nil {
			return [3]uint8{}, false
		}
		out[i] = uint8(v)
	}
	return out, true
}

// ---- writing a frame back ----------------------------------------------------

// renderAt writes the document as it stands at time t. Every animated attribute
// is replaced by its value then, and animateTransform is composed into one
// transform attribute alongside whatever static transform the element had --
// SMIL's own rule, and the reason a row can slide and fade at once.
func (n *svgNode) renderAt(b *strings.Builder, t float64) {
	over := map[string]string{}
	var transforms []string
	for _, an := range n.anims {
		base := ""
		for _, a := range n.attrs {
			if xmlName(a.Name) == an.attr {
				base = a.Value
			}
		}
		if v, ok := over[an.attr]; ok {
			base = v
		}
		v, ok := an.valueAt(t, base)
		if !ok {
			continue
		}
		if an.transform != "" {
			transforms = append(transforms, an.transform+"("+v+")")
			continue
		}
		if an.additive {
			v = addValues(base, v)
		}
		over[an.attr] = v
	}

	b.WriteByte('<')
	b.WriteString(n.name)
	wrote := map[string]bool{}
	for _, a := range n.attrs {
		name := xmlName(a.Name)
		val := a.Value
		if v, ok := over[name]; ok {
			val = v
		}
		if name == "transform" && len(transforms) > 0 {
			val = strings.TrimSpace(val + " " + strings.Join(transforms, " "))
			transforms = nil
		}
		wrote[name] = true
		// attrEsc, not %q: Go's quoting is Go's, and an attribute holding an "&"
		// -- a title, a stamp, a query in a data- attribute -- comes back out as
		// a bare ampersand that no XML parser will take. The value in hand was
		// decoded on the way in, so it is escaped again on the way out.
		fmt.Fprintf(b, ` %s="%s"`, name, attrEsc(val))
	}
	// an animated attribute the element never carried statically -- animating
	// opacity on a shape that had none -- still has to be written
	for _, an := range n.anims {
		if v, ok := over[an.attr]; ok && !wrote[an.attr] {
			wrote[an.attr] = true
			fmt.Fprintf(b, ` %s="%s"`, an.attr, attrEsc(v))
		}
	}
	if len(transforms) > 0 {
		fmt.Fprintf(b, ` transform="%s"`, attrEsc(strings.Join(transforms, " ")))
	}

	if n.empty && len(n.kids) == 0 && strings.TrimSpace(n.text) == "" {
		b.WriteString("/>")
		return
	}
	b.WriteByte('>')
	b.WriteString(escapeText(n.text))
	for _, k := range n.kids {
		k.renderAt(b, t)
	}
	fmt.Fprintf(b, "</%s>", n.name)
}

func escapeText(s string) string {
	r := strings.NewReplacer("&", "&amp;", "<", "&lt;", ">", "&gt;")
	return r.Replace(s)
}

// svgDuration is how long the document's animation runs: the last moment
// anything is still moving. An indefinite repeat has no such moment, so it
// reports 0 and the caller uses the slot length -- an animation meant to loop
// forever loops for as long as it is on screen.
func svgDuration(root *svgNode) float64 {
	end := 0.0
	var walk func(*svgNode)
	walk = func(n *svgNode) {
		for _, an := range n.anims {
			if math.IsInf(an.repeat, 1) {
				continue
			}
			if e := an.begin + an.dur*an.repeat; e > end {
				end = e
			}
		}
		for _, k := range n.kids {
			walk(k)
		}
	}
	walk(root)
	return end
}

// bakeSVG writes one static SVG per frame into dir and returns ffmpeg's printf
// pattern and the count. The animation plays at its own speed and is then held;
// a document repeating indefinitely repeats for the whole slot.
func bakeSVG(src []byte, dir string, fps, dur float64) (string, int, error) {
	root, err := parseSVG(src)
	if err != nil {
		return "", 0, err
	}
	if fps <= 0 {
		fps = svgFPS
	}
	if dur <= 0 {
		dur = math.Max(1, svgDuration(root))
	}
	if err := os.MkdirAll(dir, 0o755); err != nil {
		return "", 0, err
	}
	n := int(math.Ceil(dur * fps))
	if n < 1 {
		n = 1
	}
	for i := 0; i < n; i++ {
		var b strings.Builder
		b.WriteString(`<?xml version="1.0" encoding="UTF-8"?>` + "\n")
		root.renderAt(&b, float64(i)/fps)
		name := filepath.Join(dir, fmt.Sprintf("f%05d.svg", i))
		if err := os.WriteFile(name, []byte(b.String()), 0o644); err != nil {
			return "", 0, err
		}
	}
	return filepath.Join(dir, "f%05d.svg"), n, nil
}
