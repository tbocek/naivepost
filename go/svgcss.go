package main

// CSS animation, baked the way SMIL is: @keyframes rules become svgAnims for
// svganim.go, which interpolates, freezes, repeats and writes the frame.
//
// Subset:
//
//	properties   opacity, transform, fill
//	transform    translate(X/Y), scale(X/Y), rotate, none -- px and deg
//	selectors    one compound selector: #id, .class, tag, *, combinations
//	             ("rect.row"); no combinators, attributes or pseudo-classes
//	timing       animation and its longhands: name, duration, delay,
//	             timing-function, iteration-count, fill-mode
//	easing       linear, ease, ease-in, ease-out, ease-in-out, cubic-bezier();
//	             steps() read as linear
//
// Anything outside it stays in the document and renders statically. Widen via
// cssAnimProps and cssTransform.
//
// Baked frames drop the @keyframes blocks, the animation declarations and the
// declarations of the animated properties: a stylesheet outranks a
// presentation attribute, so "opacity: 0" left in a rule would hide the baked
// value. Cost: a rule that animates one element and statically styles another
// loses that property for both.

import (
	"math"
	"strconv"
	"strings"
)

// cssAnimProps is the allowlist, and each base is the property's value on an
// element that never set it -- the underlying value a keyframe list that starts
// at 50%, or ends at 80%, interpolates from and back to. A slice rather than a
// map so two runs of the baker over one document write the same file.
var cssAnimProps = []struct{ prop, base string }{
	{"opacity", "1"},
	{"transform", ""}, // no useful initial: an identity shaped like the other end
	{"fill", "#000000"},
}

func cssBaseOf(prop string) (string, bool) {
	for _, p := range cssAnimProps {
		if p.prop == prop {
			return p.base, true
		}
	}
	return "", false
}

// svgCSSAnimated is the cheap textual test for a document worth parsing:
// keyframes to read AND something that runs them. Either alone animates nothing.
func svgCSSAnimated(b []byte) bool {
	s := string(b)
	return strings.Contains(s, "@keyframes") && strings.Contains(s, "animation")
}

// ---- the little bit of CSS this reads ----------------------------------------

type cssDecl struct{ prop, val string }

// cssRule is one plain rule. used and baked are filled while the elements are
// walked: an animation is not a property of a rule but of an element, since the
// name can come from one rule and the delay from another -- which is how a tier
// list staggers its rows (.row for the animation, .r2 .r3 for the delays).
type cssRule struct {
	sels  []string
	decls []cssDecl
	used  bool            // contributed to an animation that was baked
	baked map[string]bool // ...and the properties that were baked from it
}

// cssItem keeps the stylesheet's order for writing it back: a plain rule, a
// @keyframes block (kept until it is known to have been consumed), or anything
// else (@font-face, @media, a stray line) held verbatim.
type cssItem struct {
	raw  string
	kf   string // the name, for a @keyframes block
	rule *cssRule
}

type cssKeyframe struct {
	at    float64
	props []cssDecl
}

// cssAnimSpec is one property of one animation, resolved as far as it can be
// without an element: the stops in document order, and the timing around them.
// Only the endpoints depend on the element, so toAnim finishes the job.
type cssAnimSpec struct {
	name     string // the @keyframes it came from, so a consumed one can be dropped
	prop     string
	ats      []float64
	vals     []string
	begin    float64
	dur      float64
	repeat   float64
	freeze   bool
	backfill bool
	ease     *cssEase
}

type cssSheet struct {
	items  []cssItem
	frames map[string][]cssKeyframe
}

// cssStrip removes comments. The baked frames are a rewrite of the document, so
// a comment that survives into them is not worth the parser it would take.
func cssStrip(s string) string {
	for {
		i := strings.Index(s, "/*")
		if i < 0 {
			return s
		}
		j := strings.Index(s[i+2:], "*/")
		if j < 0 {
			return s[:i]
		}
		s = s[:i] + " " + s[i+2+j+2:]
	}
}

// cssSplit splits on a separator that is not inside brackets -- selector lists,
// animation lists, and the arguments of cubic-bezier() all need it.
func cssSplit(s string, sep byte) []string {
	var out []string
	depth, start := 0, 0
	for i := 0; i < len(s); i++ {
		switch s[i] {
		case '(', '[':
			depth++
		case ')', ']':
			depth--
		case sep:
			if depth == 0 {
				out = append(out, strings.TrimSpace(s[start:i]))
				start = i + 1
			}
		}
	}
	out = append(out, strings.TrimSpace(s[start:]))
	return out
}

// cssFields splits on whitespace outside brackets, so cubic-bezier(.1, 0, .2, 1)
// stays one word of the animation shorthand.
func cssFields(s string) []string {
	var out []string
	depth, start := 0, -1
	for i := 0; i < len(s); i++ {
		switch c := s[i]; {
		case c == '(' || c == '[':
			depth++
		case c == ')' || c == ']':
			depth--
		case (c == ' ' || c == '\t' || c == '\n' || c == '\r') && depth == 0:
			if start >= 0 {
				out = append(out, s[start:i])
				start = -1
			}
			continue
		}
		if start < 0 {
			start = i
		}
	}
	if start >= 0 {
		out = append(out, s[start:])
	}
	return out
}

// cssDecls reads a declaration block. Unknown properties come through untouched;
// this is a rewriter, and a rule it does not understand still has to come out.
func cssDecls(body string) []cssDecl {
	var out []cssDecl
	for _, d := range cssSplit(body, ';') {
		if d == "" {
			continue
		}
		p, v, ok := strings.Cut(d, ":")
		if !ok {
			continue
		}
		out = append(out, cssDecl{strings.ToLower(strings.TrimSpace(p)), strings.TrimSpace(v)})
	}
	return out
}

// parseCSS reads a stylesheet into rules and @keyframes, keeping the order of
// everything it did not take apart.
func parseCSS(src string) *cssSheet {
	sh := &cssSheet{frames: map[string][]cssKeyframe{}}
	s := cssStrip(src)
	for i := 0; i < len(s); {
		open := strings.IndexByte(s[i:], '{')
		if open < 0 {
			if rest := strings.TrimSpace(s[i:]); rest != "" {
				sh.items = append(sh.items, cssItem{raw: rest})
			}
			break
		}
		open += i
		shut := cssBlockEnd(s, open)
		prelude := strings.TrimSpace(s[i:open])
		body := s[open+1 : shut]
		i = shut + 1
		if i > len(s) {
			i = len(s)
		}
		low := strings.ToLower(prelude)
		switch {
		case strings.HasPrefix(low, "@keyframes") || strings.HasPrefix(low, "@-webkit-keyframes"):
			name := strings.TrimSpace(prelude[strings.IndexByte(prelude, ' ')+1:])
			name = strings.Trim(name, `"'`)
			if fr := parseKeyframes(body); len(fr) > 0 && name != "" {
				sh.frames[name] = fr
				sh.items = append(sh.items, cssItem{raw: prelude + " {" + body + "}", kf: name})
			}
		case strings.HasPrefix(prelude, "@"):
			sh.items = append(sh.items, cssItem{raw: prelude + " {" + body + "}"})
		default:
			r := &cssRule{sels: cssSplit(prelude, ','), decls: cssDecls(body), baked: map[string]bool{}}
			sh.items = append(sh.items, cssItem{rule: r})
		}
	}
	return sh
}

// cssBlockEnd is the index of the brace closing the one at open, or the end of
// the text for a block nobody closed.
func cssBlockEnd(s string, open int) int {
	depth := 0
	for i := open; i < len(s); i++ {
		switch s[i] {
		case '{':
			depth++
		case '}':
			if depth--; depth == 0 {
				return i
			}
		}
	}
	return len(s)
}

// parseKeyframes reads the blocks inside one @keyframes. "from" is 0, "to" is 1,
// and a block can carry several selectors ("0%, 100% { opacity: 0 }").
func parseKeyframes(body string) []cssKeyframe {
	var out []cssKeyframe
	for i := 0; i < len(body); {
		open := strings.IndexByte(body[i:], '{')
		if open < 0 {
			break
		}
		open += i
		end := cssBlockEnd(body, open)
		sel := strings.TrimSpace(body[i:open])
		decls := cssDecls(body[open+1 : end])
		i = end + 1
		for _, s := range cssSplit(sel, ',') {
			at, ok := cssStopAt(s)
			if !ok {
				continue
			}
			out = append(out, cssKeyframe{at: at, props: decls})
		}
	}
	return out
}

func cssStopAt(s string) (float64, bool) {
	switch s = strings.ToLower(strings.TrimSpace(s)); s {
	case "from":
		return 0, true
	case "to":
		return 1, true
	}
	if !strings.HasSuffix(s, "%") {
		return 0, false
	}
	f, err := strconv.ParseFloat(strings.TrimSuffix(s, "%"), 64)
	if err != nil || f < 0 || f > 100 {
		return 0, false
	}
	return f / 100, true
}

// ---- the animation shorthand --------------------------------------------------

// cssAnim is one entry of an animation list, before its keyframes are looked up.
type cssAnim struct {
	name     string
	dur      float64
	delay    float64
	iter     float64
	ease     *cssEase
	freeze   bool // fill-mode forwards or both
	backfill bool // fill-mode backwards or both
	bad      bool // something in it is outside the subset: leave the rule alone
}

// cssAnims reads a rule's animation declarations -- the shorthand and the
// longhands, in the order they were written, last one winning as CSS does.
func cssAnims(decls []cssDecl) []cssAnim {
	var list []cssAnim
	at := func(i int) *cssAnim {
		for len(list) <= i {
			list = append(list, cssAnim{iter: 1})
		}
		return &list[i]
	}
	for _, d := range decls {
		if !strings.HasPrefix(d.prop, "animation") {
			continue
		}
		for i, part := range cssSplit(d.val, ',') {
			a := at(i)
			switch d.prop {
			case "animation":
				parseAnimShorthand(a, part)
			case "animation-name":
				a.name = part
			case "animation-duration":
				a.dur, _ = cssTime(part)
			case "animation-delay":
				a.delay, _ = cssTime(part)
			case "animation-timing-function":
				a.ease = cssEaseOf(part)
			case "animation-iteration-count":
				a.iter = cssIter(part)
			case "animation-fill-mode":
				a.freeze, a.backfill = cssFill(part)
			case "animation-direction":
				if strings.ToLower(part) != "normal" {
					a.bad = true // reverse and alternate are not read; see the header
				}
			}
		}
	}
	return list
}

// parseAnimShorthand reads "in 1.2s ease-out .3s both" -- the values are in any
// order, so each word is placed by what it looks like. The first time is the
// duration and the second the delay, which is the one piece of order CSS keeps.
func parseAnimShorthand(a *cssAnim, s string) {
	*a = cssAnim{iter: 1} // the shorthand resets whatever it does not mention
	times := 0
	for _, w := range cssFields(s) {
		low := strings.ToLower(w)
		if t, ok := cssTime(low); ok {
			if times == 0 {
				a.dur = t
			} else if times == 1 {
				a.delay = t
			}
			times++
			continue
		}
		if cssIsTiming(low) {
			a.ease = cssEaseOf(low)
			continue
		}
		switch low {
		case "infinite":
			a.iter = math.Inf(1)
			continue
		case "none":
			continue
		case "normal":
			continue
		case "reverse", "alternate", "alternate-reverse":
			a.bad = true
			continue
		case "forwards", "backwards", "both":
			a.freeze, a.backfill = cssFill(low)
			continue
		case "running", "paused":
			continue
		}
		if f, err := strconv.ParseFloat(low, 64); err == nil {
			a.iter = f
			continue
		}
		a.name = w // whatever is left is the name of the keyframes
	}
}

// cssIsTiming says whether a word of the shorthand is the timing function --
// including the ones read as linear, which still must not be mistaken for the
// name of the keyframes.
func cssIsTiming(s string) bool {
	switch s {
	case "linear", "ease", "ease-in", "ease-out", "ease-in-out",
		"step-start", "step-end":
		return true
	}
	return strings.HasPrefix(s, "cubic-bezier(") || strings.HasPrefix(s, "steps(")
}

func cssFill(s string) (freeze, backfill bool) {
	switch strings.ToLower(strings.TrimSpace(s)) {
	case "forwards":
		return true, false
	case "backwards":
		return false, true
	case "both":
		return true, true
	}
	return false, false
}

func cssIter(s string) float64 {
	if strings.EqualFold(strings.TrimSpace(s), "infinite") {
		return math.Inf(1)
	}
	if f, err := strconv.ParseFloat(strings.TrimSpace(s), 64); err == nil && f > 0 {
		return f
	}
	return 1
}

// cssTime reads a CSS <time>. Unlike SMIL's clock values the unit is required,
// which is what keeps "2" in the shorthand an iteration count rather than a
// two-second duration.
func cssTime(s string) (float64, bool) {
	s = strings.TrimSpace(s)
	switch {
	case strings.HasSuffix(s, "ms"):
		f, err := strconv.ParseFloat(strings.TrimSuffix(s, "ms"), 64)
		return f / 1000, err == nil
	case strings.HasSuffix(s, "s"):
		f, err := strconv.ParseFloat(strings.TrimSuffix(s, "s"), 64)
		return f, err == nil
	}
	return 0, false
}

// ---- easing -------------------------------------------------------------------

// cssEase is a cubic bezier easing, which is what every named CSS easing is.
// nil means linear, so the SMIL side keeps its straight lines by doing nothing.
type cssEase struct{ x1, y1, x2, y2 float64 }

func cssEaseOf(s string) *cssEase {
	s = strings.ToLower(strings.TrimSpace(s))
	switch s {
	case "linear":
		return nil
	case "ease":
		return &cssEase{0.25, 0.1, 0.25, 1}
	case "ease-in":
		return &cssEase{0.42, 0, 1, 1}
	case "ease-out":
		return &cssEase{0, 0, 0.58, 1}
	case "ease-in-out":
		return &cssEase{0.42, 0, 0.58, 1}
	}
	if strings.HasPrefix(s, "cubic-bezier(") && strings.HasSuffix(s, ")") {
		parts := cssSplit(s[len("cubic-bezier("):len(s)-1], ',')
		if len(parts) != 4 {
			return nil
		}
		var v [4]float64
		for i, p := range parts {
			f, err := strconv.ParseFloat(strings.TrimSpace(p), 64)
			if err != nil {
				return nil
			}
			v[i] = f
		}
		return &cssEase{v[0], v[1], v[2], v[3]}
	}
	return nil // steps() and anything unknown: linear, and the picture is right
}

// at is the eased fraction: solve x(t) = f for t, then read y(t). Newton first
// because it lands in three or four rounds, bisection behind it for the curves
// where the derivative goes flat.
func (e *cssEase) at(f float64) float64 {
	if e == nil || f <= 0 || f >= 1 {
		return math.Min(1, math.Max(0, f))
	}
	bez := func(a, b, t float64) float64 {
		u := 1 - t
		return 3*u*u*t*a + 3*u*t*t*b + t*t*t
	}
	slope := func(a, b, t float64) float64 {
		u := 1 - t
		return 3*u*u*a + 6*u*t*(b-a) + 3*t*t*(1-b)
	}
	t := f
	for i := 0; i < 8; i++ {
		x := bez(e.x1, e.x2, t) - f
		if math.Abs(x) < 1e-6 {
			return bez(e.y1, e.y2, t)
		}
		d := slope(e.x1, e.x2, t)
		if math.Abs(d) < 1e-6 {
			break
		}
		t -= x / d
	}
	lo, hi := 0.0, 1.0
	t = f
	for i := 0; i < 40; i++ {
		x := bez(e.x1, e.x2, t)
		if math.Abs(x-f) < 1e-6 {
			break
		}
		if x < f {
			lo = t
		} else {
			hi = t
		}
		t = (lo + hi) / 2
	}
	return bez(e.y1, e.y2, t)
}

// ---- transforms ---------------------------------------------------------------

// cssTransform rewrites a CSS transform list as an SVG transform attribute:
// px and deg off, translateX and friends spelled out. A unit that is not a
// static length -- %, em, vw -- cannot be baked without knowing the box, so the
// whole property is refused and the element keeps whatever it had.
func cssTransform(v string) (string, bool) {
	v = strings.TrimSpace(v)
	if v == "" || strings.EqualFold(v, "none") {
		return "", true // an identity, shaped later like the other end of the run
	}
	var out []string
	for i := 0; i < len(v); {
		open := strings.IndexByte(v[i:], '(')
		if open < 0 {
			break
		}
		open += i
		shut := strings.IndexByte(v[open:], ')')
		if shut < 0 {
			return "", false
		}
		shut += open
		fn := strings.ToLower(strings.TrimSpace(v[i:open]))
		args := cssSplit(v[open+1:shut], ',')
		if len(args) == 1 {
			args = strings.Fields(args[0])
		}
		i = shut + 1
		nums := make([]float64, 0, len(args))
		for _, a := range args {
			f, ok := cssNum(a, fn == "rotate" || fn == "skewx" || fn == "skewy")
			if !ok {
				return "", false
			}
			nums = append(nums, f)
		}
		switch fn {
		case "translate":
			if len(nums) == 1 {
				nums = append(nums, 0)
			}
			if len(nums) != 2 {
				return "", false
			}
			out = append(out, "translate("+fmtNum(nums[0])+" "+fmtNum(nums[1])+")")
		case "translatex":
			if len(nums) != 1 {
				return "", false
			}
			out = append(out, "translate("+fmtNum(nums[0])+" 0)")
		case "translatey":
			if len(nums) != 1 {
				return "", false
			}
			out = append(out, "translate(0 "+fmtNum(nums[0])+")")
		case "scale":
			if len(nums) == 1 {
				nums = append(nums, nums[0])
			}
			if len(nums) != 2 {
				return "", false
			}
			out = append(out, "scale("+fmtNum(nums[0])+" "+fmtNum(nums[1])+")")
		case "scalex":
			if len(nums) != 1 {
				return "", false
			}
			out = append(out, "scale("+fmtNum(nums[0])+" 1)")
		case "scaley":
			if len(nums) != 1 {
				return "", false
			}
			out = append(out, "scale(1 "+fmtNum(nums[0])+")")
		case "rotate":
			if len(nums) != 1 {
				return "", false
			}
			out = append(out, "rotate("+fmtNum(nums[0])+")")
		default:
			return "", false
		}
	}
	if len(out) == 0 {
		return "", false
	}
	return strings.Join(out, " "), true
}

// cssNum reads one transform argument. Lengths are px or nothing; angles are deg
// or nothing, since an SVG rotate is in degrees anyway. turn and rad are read
// too -- they cost two lines and a card that spins is a card that spins.
func cssNum(s string, angle bool) (float64, bool) {
	s = strings.ToLower(strings.TrimSpace(s))
	mul := 1.0
	switch {
	case strings.HasSuffix(s, "px"):
		s = strings.TrimSuffix(s, "px")
	case angle && strings.HasSuffix(s, "deg"):
		s = strings.TrimSuffix(s, "deg")
	case angle && strings.HasSuffix(s, "rad"):
		s, mul = strings.TrimSuffix(s, "rad"), 180/math.Pi
	case angle && strings.HasSuffix(s, "turn"):
		s, mul = strings.TrimSuffix(s, "turn"), 360
	case strings.HasSuffix(s, "%"), strings.HasSuffix(s, "em"),
		strings.HasSuffix(s, "vw"), strings.HasSuffix(s, "vh"):
		return 0, false
	}
	f, err := strconv.ParseFloat(strings.TrimSpace(s), 64)
	if err != nil {
		return 0, false
	}
	return f * mul, true
}

// cssIdentityLike is the do-nothing transform shaped like the one given, for the
// end of a run that has no keyframe of its own and no static transform to fall
// back on: a fly-in written as "from { transform: translate(40px,0) }" ends at
// translate(0 0), and both ends have to have the same functions in them or there
// is nothing to interpolate.
func cssIdentityLike(v string) string {
	nums, seps := numsOf(v)
	if len(nums) == 0 {
		return v
	}
	out := make([]float64, len(nums))
	fn := ""
	for i := range out {
		// seps[i] is the text in front of nums[i], so the last function name
		// seen is the one this number belongs to
		if low := strings.ToLower(seps[i]); strings.Contains(low, "(") {
			fn = low
		}
		if strings.Contains(fn, "scale") {
			out[i] = 1
		}
	}
	return rebuild(out, seps)
}

// ---- putting it on the document -----------------------------------------------

// applyCSSAnimations turns every animation the subset covers into svgAnims on
// the elements it applies to, and rewrites the stylesheet without the parts that
// were baked. Called once, from parseSVG, so everything downstream -- duration,
// frame writing, the still/sequence decision -- sees CSS and SMIL as one thing.
func applyCSSAnimations(root *svgNode) {
	var styles []*svgNode
	var walk func(*svgNode)
	walk = func(n *svgNode) {
		if strings.EqualFold(n.name, "style") {
			styles = append(styles, n)
		}
		for _, k := range n.kids {
			walk(k)
		}
	}
	walk(root)
	if len(styles) == 0 {
		return
	}

	sheets := make([]*cssSheet, len(styles))
	frames := map[string][]cssKeyframe{}
	var rules []*cssRule
	for i, st := range styles {
		sheets[i] = parseCSS(st.text)
		for k, v := range sheets[i].frames {
			frames[k] = v
		}
		for _, it := range sheets[i].items {
			if it.rule != nil {
				rules = append(rules, it.rule)
			}
		}
	}
	if len(frames) == 0 || len(rules) == 0 {
		return
	}

	used := map[string]bool{} // the @keyframes that were baked, by name
	// every rule that matches contributes its declarations, in document order,
	// and only then is the animation read: the shorthand and the longhands
	// cascade onto the element, not onto any one rule. That is what lets a tier
	// list stagger -- .row carries the animation, .r2 and .r3 only the delay.
	animate := func(n *svgNode) {
		var matched []*cssRule
		var decls []cssDecl
		for _, r := range rules {
			if !cssMatches(r.sels, n) {
				continue
			}
			matched = append(matched, r)
			decls = append(decls, r.decls...)
		}
		specs := specsFor(decls, frames)
		if len(specs) == 0 {
			return
		}
		for _, sp := range specs {
			if an := sp.toAnim(n); an != nil {
				n.anims = append(n.anims, an)
				used[sp.name] = true
			}
		}
		for _, r := range matched {
			r.used = true
			for _, sp := range specs {
				r.baked[sp.prop] = true
			}
		}
	}
	var apply func(*svgNode)
	apply = func(n *svgNode) {
		if !strings.EqualFold(n.name, "style") {
			animate(n)
		}
		for _, k := range n.kids {
			apply(k)
		}
	}
	apply(root)
	if len(used) == 0 {
		return // nothing was bakeable: leave the document exactly as it was
	}

	for i, st := range styles {
		st.text = sheets[i].clean(used)
	}
}

// specsFor resolves a rule's animation declarations against the keyframes they
// name: one spec per animated property, with the stops it actually has.
func specsFor(decls []cssDecl, frames map[string][]cssKeyframe) []cssAnimSpec {
	var out []cssAnimSpec
	for _, a := range cssAnims(decls) {
		if a.bad || a.name == "" || a.dur <= 0 {
			continue
		}
		fr := frames[a.name]
		if len(fr) == 0 {
			continue
		}
		// one pass per property, since a keyframe that does not mention a
		// property is not a stop for it: "50% { opacity: 1 }" says nothing about
		// the transform, and interpolating through it would jerk
		for _, ap := range cssAnimProps {
			prop := ap.prop
			var ats []float64
			var vals []string
			ok := true
			for _, f := range fr {
				for _, d := range f.props {
					if d.prop != prop {
						continue
					}
					v := d.val
					if prop == "transform" {
						t, good := cssTransform(v)
						if !good {
							ok = false
							break
						}
						v = t
					}
					ats, vals = insertStop(ats, vals, f.at, v)
				}
				if !ok {
					break
				}
			}
			if !ok || len(vals) == 0 {
				continue
			}
			out = append(out, cssAnimSpec{
				name: a.name, prop: prop, ats: ats, vals: vals,
				begin: a.delay, dur: a.dur, repeat: a.iter,
				freeze: a.freeze, backfill: a.backfill, ease: a.ease,
			})
		}
	}
	return out
}

// insertStop keeps the stops sorted by time. Two keyframes at the same fraction
// are the same stop, and the later one wins, as it does in a stylesheet.
func insertStop(ats []float64, vals []string, at float64, v string) ([]float64, []string) {
	for i, a := range ats {
		switch {
		case a == at:
			vals[i] = v
			return ats, vals
		case a > at:
			ats = append(ats[:i], append([]float64{at}, ats[i:]...)...)
			vals = append(vals[:i], append([]string{v}, vals[i:]...)...)
			return ats, vals
		}
	}
	return append(ats, at), append(vals, v)
}

// toAnim finishes a spec against the element it applies to: the ends of the run
// that no keyframe gave a value to are the element's own value, which is what
// CSS calls the underlying value.
func (s cssAnimSpec) toAnim(n *svgNode) *svgAnim {
	base, _ := cssBaseOf(s.prop)
	for _, a := range n.attrs {
		if xmlName(a.Name) == s.prop {
			base = a.Value
		}
	}
	ats, vals := append([]float64{}, s.ats...), append([]string{}, s.vals...)
	if s.prop == "transform" && strings.TrimSpace(base) == "" {
		base = cssIdentityLike(vals[0])
	}
	if ats[0] > 0 {
		ats, vals = append([]float64{0}, ats...), append([]string{base}, vals...)
	}
	if ats[len(ats)-1] < 1 {
		ats, vals = append(ats, 1), append(vals, base)
	}
	if len(vals) < 2 {
		return nil
	}
	return &svgAnim{
		attr:     s.prop,
		values:   vals,
		keyTimes: ats,
		begin:    s.begin,
		dur:      s.dur,
		repeat:   s.repeat,
		freeze:   s.freeze,
		backfill: s.backfill,
		ease:     s.ease,
	}
}

// cssMatches is the selector engine, and it is one compound selector deep on
// purpose: a card is a handful of elements with ids and classes on them, and a
// selector this cannot read matches nothing rather than matching the wrong
// element -- the rule then stays in the stylesheet and renders as it always did.
func cssMatches(sels []string, n *svgNode) bool {
	for _, sel := range sels {
		if sel == "" || strings.ContainsAny(sel, " >+~[]:,") {
			continue
		}
		if matchCompound(sel, n) {
			return true
		}
	}
	return false
}

func matchCompound(sel string, n *svgNode) bool {
	var id, class []string
	tag := ""
	for i := 0; i < len(sel); {
		j := i + 1
		for j < len(sel) && sel[j] != '.' && sel[j] != '#' {
			j++
		}
		part := sel[i:j]
		switch part[0] {
		case '#':
			id = append(id, part[1:])
		case '.':
			class = append(class, part[1:])
		default:
			tag = part
		}
		i = j
	}
	if tag != "" && tag != "*" && !strings.EqualFold(tag, n.name) {
		return false
	}
	at := func(name string) string {
		for _, a := range n.attrs {
			if xmlName(a.Name) == name {
				return a.Value
			}
		}
		return ""
	}
	for _, want := range id {
		if at("id") != want {
			return false
		}
	}
	if len(class) > 0 {
		have := map[string]bool{}
		for _, c := range strings.Fields(at("class")) {
			have[c] = true
		}
		for _, want := range class {
			if !have[want] {
				return false
			}
		}
	}
	return tag != "" || len(id) > 0 || len(class) > 0
}

// clean writes the stylesheet back without what was baked: the @keyframes are
// gone with the rest of the animation, and so is every declaration of a property
// the rule animates -- a surviving "opacity: 0" outranks the baked attribute and
// the card would never show up.
func (sh *cssSheet) clean(used map[string]bool) string {
	var b strings.Builder
	for _, it := range sh.items {
		switch {
		case it.kf != "":
			if !used[it.kf] {
				b.WriteString(it.raw + "\n") // an animation nobody could bake
			}
			continue
		case it.rule == nil:
			b.WriteString(it.raw + "\n")
			continue
		case !it.rule.used:
			// a rule that had nothing to do with the baking keeps every
			// declaration it was written with, animation included
			if len(it.rule.decls) > 0 {
				b.WriteString(ruleText(it.rule, nil))
			}
			continue
		}
		b.WriteString(ruleText(it.rule, it.rule.baked))
	}
	return b.String()
}

// ruleText writes a rule back without the declarations named in drop, and
// without the animation that has already happened.
func ruleText(r *cssRule, drop map[string]bool) string {
	var keep []string
	for _, d := range r.decls {
		if drop != nil && (strings.HasPrefix(d.prop, "animation") || drop[d.prop]) {
			continue
		}
		keep = append(keep, d.prop+": "+d.val)
	}
	if len(keep) == 0 {
		return ""
	}
	return strings.Join(r.sels, ", ") + " { " + strings.Join(keep, "; ") + " }\n"
}
