package main

// Filling a card in: the drawing lives in the file, the numbers come from
// here. Holes in the document:
//
//	{{name}}                     filled in, or nothing
//	{{name|what it says instead}} filled in, or that
//
// The holes are a place's contents, the fitted name size, and every
// animation's values and keyTimes. Everything else is in the file only.

import (
	"bytes"
	"regexp"
	"strings"
)

// svgScope is what the card has to say about a document: hole name (lower case)
// to value. The value is escaped on the way in, so a title with an ampersand in
// it is a title and not a broken document.
type svgScope map[string]string

// svgFillScope answers the holes the card has a value for and leaves the rest as
// they are, defaults and all -- those are the parameters' turn, and after them
// their own defaults. A place the card said nothing about is a place the file
// itself calls empty.
func svgFillScope(src []byte, vals svgScope) []byte {
	if len(vals) == 0 {
		return src
	}
	return outsideComments(src, func(b []byte) []byte {
		return svgHole.ReplaceAllFunc(b, func(m []byte) []byte {
			g := svgHole.FindSubmatch(m)
			v, ok := vals[strings.ToLower(string(g[1]))]
			if !ok {
				return m
			}
			return []byte(escapeText(v))
		})
	})
}

// A comment is not part of the picture, and neither is a hole written in one:
// the file explains itself in comments, and the example it uses to explain what
// a {{hole}} is must not turn into a box in the dialog, be quietly answered by a
// path, or be filled in by the card and stop explaining anything. So everything
// that fills a document in steps over its comments.
var reComment = regexp.MustCompile(`(?s)<!--.*?-->`)

func outsideComments(src []byte, f func([]byte) []byte) []byte {
	var out bytes.Buffer
	pos := 0
	for _, c := range reComment.FindAllIndex(src, -1) {
		out.Write(f(src[pos:c[0]]))
		out.Write(src[c[0]:c[1]])
		pos = c[1]
	}
	out.Write(f(src[pos:]))
	return out.Bytes()
}

// stampArgs writes the parameters a card was drawn with back into its root
// element, so the finished document is the same complete statement of itself the
// drawn cards always were: what drew it, and what with.
func stampArgs(src []byte, q svgQuery) []byte {
	return reStampArgs.ReplaceAll(src, []byte(`data-naivepost-args="`+attrEsc(q.args())+`"`))
}

var reStampArgs = regexp.MustCompile(`data-naivepost-args="[^"]*"`)
