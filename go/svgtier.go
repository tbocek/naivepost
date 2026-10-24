package main

// The tier board: all geometry is in tierTemplate (written into a project's
// assets, an ordinary SVG) -- six tiers S A B C D F, six places each, all
// coordinates written out. This file supplies what a document cannot: what is
// in a place, how big its name must be to fit, which place just arrived, and
// when everything moves.

import (
	"fmt"
	"math"
	"strings"
)

// The board as tier.svg draws it. These numbers are in two places on purpose --
// there and here -- because two things need them that a document cannot do:
// fitting a name to the width of a place, and knowing how far off the right edge
// a chip flies in from. Move something in the file and move it here.
const (
	tierSlots  = 6     // places in a tier, and the whole of what a tier holds
	tierTop    = 180.0 // the first row's top edge
	tierRowH   = 140.0 // and one row to the next
	tierBoxH   = 124.0 // how tall the row itself is, the gap taken off
	tierChipX  = 338.0 // the first place in a row
	tierChipDX = 244.0 // and one place to the next
	tierChipW  = 230.0
	tierChipH  = 108.0
	tierPad    = 14.0 // inside a place, around the picture and under the name
)

// The six tiers, in the order they are written down the side of the board, with
// the colour each letter has always had. E is missing because a tier list is
// read as S to F and nobody has ever put an E in the middle of one.
var (
	tierLetters = []string{"S", "A", "B", "C", "D", "F"}
	tierColors  = []string{"#ff7f7f", "#ffbf7f", "#ffdf7f", "#ffff7f", "#bfff7f", "#7fffff"}
)

func tierRowY(i int) float64      { return tierTop + float64(i)*tierRowH }
func tierPlaceX(j int) float64    { return tierChipX + float64(j)*tierChipDX }
func tierPlaceY(i int) float64    { return tierRowY(i) + 8 }
func tierNameY(i int) float64     { return tierPlaceY(i) + tierChipH - tierPad }
func tierPrefix(i int) string     { return strings.ToLower(tierLetters[i]) }
func tierSlotKey(i, j int) string { return fmt.Sprintf("%s%d", tierPrefix(i), j+1) }

// tierSVG draws the board: rows fly in from the right one after another, their
// contents follow, all frozen at the end. A named new item is held back and
// flies into a board that has stopped moving. src is the file drawn from, so a
// restyled board stays restyled; a file with none of the card's holes left is
// a picture, and the built-in template is used instead.
func tierSVG(q svgQuery, dir string, src []byte) []byte {
	tmpl := []byte(tierTemplate)
	if tierIsTmpl(src) {
		tmpl = src
	}
	out := svgFillScope(tmpl, tierScope(q, dir))
	return stampArgs(svgFill(out, q), q)
}

func tierIsTmpl(src []byte) bool {
	for _, h := range svgHoles(src) {
		if cardOwnHole(h.Key) {
			return true
		}
	}
	return false
}

// tierScope is the whole of the card's arithmetic, as the holes the document
// asks for: one set for the board, one per tier, one per place in a tier. A
// place nobody filled in is left out of this entirely and falls back to what the
// file says it is when it is empty, which is off.
func tierScope(q svgQuery, dir string) svgScope {
	title := strings.TrimSpace(q.get("title"))
	rows, arrive := tierBoard(q)

	// When the board names a new item, the board that was already there is not
	// news: it arrives the same way, at a third of the pace, and then the new
	// item flies in by itself into the quiet after it. Everything the viewer has
	// seen before is a recap, and a recap that takes as long as the first time is
	// where a tier list gets boring. With nothing new, fast is 1 and this is the
	// card as it always was -- one pace for every row, nothing singled out.
	fast := 1.0
	if len(arrive) > 0 {
		fast = tierCatchUp
	}
	rowFly, chipRise := 0.55*fast, 0.3*fast

	// two passes: the second needs the card's whole length, and the length is the
	// last thing to land plus the time it is then read for
	rowAt := func(i int) float64 { return (0.15 + float64(i)*0.28) * fast }
	chipAt := func(i, j int) float64 { return rowAt(i) + (0.30+float64(j)*0.07)*fast }
	caught := rowAt(len(rows)-1) + rowFly
	for i, r := range rows {
		for j := range r.items {
			if _, ok := arrive[[2]int{i, j}]; !ok {
				caught = math.Max(caught, chipAt(i, j)+chipRise)
			}
		}
	}
	// the new ones go last, one after another, each waiting for the board to be
	// standing still first -- unless the list said when, in which case that is
	// when, and several things can be sent in one after another on a beat of
	// somebody else's choosing.
	newAt := map[[2]int]float64{}
	auto := 0
	for i, r := range rows {
		for j := range r.items {
			at, ok := arrive[[2]int{i, j}]
			if !ok {
				continue
			}
			if at < 0 {
				at = caught + tierNewBeat + float64(auto)*tierNewStep
				auto++
			}
			newAt[[2]int{i, j}] = at
		}
	}
	total := caught
	for _, at := range newAt {
		total = math.Max(total, at+tierNewFly)
	}
	total += cardHold

	sc := svgScope{"total": secStr(total)}
	if title != "" {
		vals, times := animPair(total, animStop{0, "0"}, animStop{0.4, "1"})
		sc["title"] = title
		sc["title.size"] = trimNum(fitFont(title, cardW-320, 72))
		sc["title.fade.values"], sc["title.fade.keytimes"] = vals, times
	}
	for i, r := range rows {
		p := tierPrefix(i)
		fade, fadeT, fly, flyT := flyPairs(total, rowAt(i), rowFly, cardW)
		sc[p+".at"] = trimNum(rowAt(i))
		sc[p+".fade.values"], sc[p+".fade.keytimes"] = fade, fadeT
		sc[p+".fly.values"], sc[p+".fly.keytimes"] = fly, flyT
		for j, it := range r.items {
			k := tierSlotKey(i, j)
			// a logo that cannot be read is not a chip that says nothing: the name
			// stands, and an item that was only ever a logo falls back to what the
			// file is called, so the board still ranks something
			href, hasLogo := cardLogo(dir, it.logo)
			name := it.text
			if !hasLogo && name == "" {
				name = baseName(it.logo)
			}

			at := chipAt(i, j)
			var fade, fadeT, move, moveT string
			if a, ok := newAt[[2]int{i, j}]; ok {
				// the one just added does not rise with the rest of its row: it
				// comes in from off the right edge of the card, the long way, on
				// a board that has already stopped moving. Same movement the rows
				// make, so it belongs to the card; alone, so it is the point of it.
				at = a
				fade, fadeT, move, moveT = flyPairs(total, at, tierNewFly, cardW-tierPlaceX(j))
			} else {
				// the chips rise into their row rather than flying across it: two
				// movements in the same direction read as one thing arriving
				fade, fadeT = animPair(total, animStop{at, "0"}, animStop{at + chipRise, "1"})
				move, moveT = animPair(total,
					animStop{at, fmt.Sprintf("0 %s", trimNum(tierBoxH*0.4))},
					animStop{at + chipRise, "0 0"})
			}
			sc[k+".show"] = "inline"
			sc[k+".at"] = trimNum(at)
			sc[k+".fade.values"], sc[k+".fade.keytimes"] = fade, fadeT
			sc[k+".move.values"], sc[k+".move.keytimes"] = move, moveT

			// a picture on its own fills the place; a picture with a name under it
			// gives the name the bottom line and keeps the rest. The name alone
			// sits in the middle, which is dy from the line it is written on.
			if hasLogo {
				sc[k+".pic"], sc[k+".logo"] = "inline", href
				sc[k+".pic.h"] = trimNum(tierChipH - 2*tierPad)
			}
			if name == "" {
				continue
			}
			fs := fitFont(name, tierChipW-24, math.Min(tierBoxH*0.30, 44))
			dy := tierRowY(i) + tierBoxH/2 + fs*0.35 - tierNameY(i)
			if hasLogo {
				fs = fitFont(name, tierChipW-24, math.Min(tierBoxH*0.22, 34))
				dy = 0
				sc[k+".pic.h"] = trimNum(math.Max(8, tierChipH-2*tierPad-fs*1.25))
			}
			sc[k+".name"] = name
			sc[k+".name.size"] = trimNum(fs)
			sc[k+".name.dy"] = trimNum(dy)
		}
	}
	return sc
}

// tierNote is what a path asked for that the board cannot show, for the log.
// The board is the six tiers with six places in each, so a seventh name in a
// tier and an arrival into a tier that is not on the board are both things
// somebody typed and will otherwise never see.
func tierNote(q svgQuery) string {
	var over []string
	for _, l := range tierLetters {
		if n := len(splitItems(q.get(l))); n > tierSlots {
			over = append(over, fmt.Sprintf("%s has %d", l, n))
		}
	}
	var missing []string
	for _, n := range parseNew(q.get("new")) {
		if n.tier != "" && !hasLetter(n.tier) && !contains(missing, n.tier) {
			missing = append(missing, n.tier)
		}
	}
	var out []string
	if len(over) > 0 {
		out = append(out, "a tier has six places and "+strings.Join(over, ", ")+
			" in it, so the rest are not drawn")
	}
	if len(missing) > 0 {
		out = append(out, "the board is S A B C D F and there is no "+
			strings.Join(missing, " or ")+" row to fly anything into")
	}
	return strings.Join(out, "; ")
}

func hasLetter(l string) bool {
	for _, have := range tierLetters {
		if strings.EqualFold(have, l) {
			return true
		}
	}
	return false
}

// flyPairs is an arrival as four strings: in from one side, overshooting a little
// and settling back, while it fades up. dx is where it comes from. The card holds
// the element and this holds the timing, which is the whole trade -- an arrival
// written [1.2s] in the Just added list comes out as a keyTimes in the document,
// where it can be read and moved.
func flyPairs(total, at, dur, dx float64) (fade, fadeT, fly, flyT string) {
	fade, fadeT = animPair(total, animStop{at, "0"}, animStop{at + math.Min(0.25, dur/2), "1"})
	over := -dx * 0.06 // the overshoot, always a little way past where it lands
	fly, flyT = animPair(total,
		animStop{at, fmt.Sprintf("%s 0", trimNum(dx))},
		animStop{at + dur*0.66, fmt.Sprintf("%s 0", trimNum(over))},
		animStop{at + dur, "0 0"})
	return fade, fadeT, fly, flyT
}

// tierTemplate is the board as a document: every row, every place, and the
// comments that say what they are. Written into a project's assets folder as-is,
// so what the app draws and what the user can edit are the same file, and it
// opens as a board rather than as a page of braces.
//
// Read it as the card's real documentation -- it is the only place where what a
// board looks like is actually written down.
const tierTemplate = `<?xml version="1.0" encoding="UTF-8"?>
<svg xmlns="http://www.w3.org/2000/svg" width="1920" height="1080" viewBox="0 0 1920 1080"
     font-family="DejaVu Sans, Liberation Sans, Helvetica, Arial, sans-serif"
     text-anchor="middle" fill="#e6e9ef"
     data-naivepost="tier" data-naivepost-args="">

  <!-- A TIER BOARD. This file is the picture AND the template naivepost draws
       from, and it is an ordinary SVG either way: open it, move things about,
       recolour it, and every board drawn from it comes out changed.

       The board is fixed. Six tiers, S A B C D F, and six places in each: all
       thirty six of them are written out below with their own coordinates, so
       what a board is, is what you can see here. Nothing is generated and
       nothing repeats; to move a row, move it.

       The only thing in here that is not ordinary SVG is a hole, written
       {{name}} or {{name|what it says when nobody says otherwise}}. The app
       fills those in and there are few of them left: what is in a place and
       what it is called, and every animation's own values and keyTimes, so the
       moment something arrives is a number in this file rather than a number
       inside the program. A place nobody has put anything in is switched off by
       its display hole and stays empty.

       Every animation begins at 0 and does its waiting inside keyTimes, so a
       viewer that ignores animation shows the FINISHED board rather than an
       empty one. That is also why this file opens as a board and not as a page
       of braces: the numbers are all here, only the contents are missing. -->

  <!-- Input: title | Title | over the board — leave it empty for none -->

  <rect width="1920" height="1080" fill="#14171c"/>

  <!-- the heading over the board, empty unless the Title box says otherwise.
       It fades up on its own, before anything else arrives. -->
  <g>
    <animate attributeName="opacity" values="{{title.fade.values|1;1}}"
             keyTimes="{{title.fade.keyTimes|0;1}}" dur="{{total|8s}}" fill="freeze"/>
    <text x="960" y="120" font-size="{{title.size|72}}" font-weight="700">{{title}}</text>
  </g>

  <!-- ==== tier S =========================================================== -->
  <!-- Input: S[keep logo] | Tier S | up to six, comma separated: a name, a logo file, or Name|logo.png -->
  <!-- ONE TIER: the plate with the letter on it, the strip beside it, and the
       six places on the strip. The row flies in from the right and stays where
       it lands; data-at is the second it starts, written out because the
       keyTimes beside it are fractions of the whole card and nobody can read a
       schedule in those. The five tiers below this one are the same thing
       again, 140 further down each time. -->
  <g data-at="{{s.at|0}}">
    <animate attributeName="opacity" values="{{s.fade.values|1;1}}"
             keyTimes="{{s.fade.keyTimes|0;1}}" dur="{{total|8s}}" fill="freeze"/>
    <animateTransform attributeName="transform" type="translate"
                      values="{{s.fly.values|0 0;0 0}}" keyTimes="{{s.fly.keyTimes|0;1}}"
                      dur="{{total|8s}}" fill="freeze"/>
    <rect x="120" y="180" width="190" height="124" rx="12" fill="#ff7f7f"/>
    <text x="215" y="266.5" font-size="70" font-weight="700" fill="#16181d">S</text>
    <rect x="324" y="180" width="1478" height="124" rx="12" fill="#20242b"/>

    <!-- ONE PLACE in this tier. It holds a picture, a name, or a picture with
         the name under it, and display below is what an empty place is: off.
         A place that was just added flies in from off the right edge instead
         of rising into its row, and the values here are the whole difference
         between the two; there is no second shape for it. data-at is when it
         gets there, so what somebody wrote as S[1.1s] in the Just added line
         is 1.1 in this file. -->
    <g display="{{s1.show|none}}" data-at="{{s1.at|0}}">
      <animate attributeName="opacity" values="{{s1.fade.values|1;1}}"
               keyTimes="{{s1.fade.keyTimes|0;1}}" dur="{{total|8s}}" fill="freeze"/>
      <animateTransform attributeName="transform" type="translate"
                        values="{{s1.move.values|0 0;0 0}}" keyTimes="{{s1.move.keyTimes|0;1}}"
                        dur="{{total|8s}}" fill="freeze"/>
      <rect x="338" y="188" width="230" height="108" rx="10" fill="#2b313b" stroke="#ff7f7f" stroke-width="3"/>
      <image display="{{s1.pic|none}}" x="352" y="202" width="202" height="{{s1.pic.h|80}}"
             preserveAspectRatio="xMidYMid meet" href="{{s1.logo}}"/>
      <text x="453" y="282" dy="{{s1.name.dy|0}}" font-size="{{s1.name.size|27}}"
            font-weight="600">{{s1.name}}</text>
    </g>

    <g display="{{s2.show|none}}" data-at="{{s2.at|0}}">
      <animate attributeName="opacity" values="{{s2.fade.values|1;1}}"
               keyTimes="{{s2.fade.keyTimes|0;1}}" dur="{{total|8s}}" fill="freeze"/>
      <animateTransform attributeName="transform" type="translate"
                        values="{{s2.move.values|0 0;0 0}}" keyTimes="{{s2.move.keyTimes|0;1}}"
                        dur="{{total|8s}}" fill="freeze"/>
      <rect x="582" y="188" width="230" height="108" rx="10" fill="#2b313b" stroke="#ff7f7f" stroke-width="3"/>
      <image display="{{s2.pic|none}}" x="596" y="202" width="202" height="{{s2.pic.h|80}}"
             preserveAspectRatio="xMidYMid meet" href="{{s2.logo}}"/>
      <text x="697" y="282" dy="{{s2.name.dy|0}}" font-size="{{s2.name.size|27}}"
            font-weight="600">{{s2.name}}</text>
    </g>

    <g display="{{s3.show|none}}" data-at="{{s3.at|0}}">
      <animate attributeName="opacity" values="{{s3.fade.values|1;1}}"
               keyTimes="{{s3.fade.keyTimes|0;1}}" dur="{{total|8s}}" fill="freeze"/>
      <animateTransform attributeName="transform" type="translate"
                        values="{{s3.move.values|0 0;0 0}}" keyTimes="{{s3.move.keyTimes|0;1}}"
                        dur="{{total|8s}}" fill="freeze"/>
      <rect x="826" y="188" width="230" height="108" rx="10" fill="#2b313b" stroke="#ff7f7f" stroke-width="3"/>
      <image display="{{s3.pic|none}}" x="840" y="202" width="202" height="{{s3.pic.h|80}}"
             preserveAspectRatio="xMidYMid meet" href="{{s3.logo}}"/>
      <text x="941" y="282" dy="{{s3.name.dy|0}}" font-size="{{s3.name.size|27}}"
            font-weight="600">{{s3.name}}</text>
    </g>

    <g display="{{s4.show|none}}" data-at="{{s4.at|0}}">
      <animate attributeName="opacity" values="{{s4.fade.values|1;1}}"
               keyTimes="{{s4.fade.keyTimes|0;1}}" dur="{{total|8s}}" fill="freeze"/>
      <animateTransform attributeName="transform" type="translate"
                        values="{{s4.move.values|0 0;0 0}}" keyTimes="{{s4.move.keyTimes|0;1}}"
                        dur="{{total|8s}}" fill="freeze"/>
      <rect x="1070" y="188" width="230" height="108" rx="10" fill="#2b313b" stroke="#ff7f7f" stroke-width="3"/>
      <image display="{{s4.pic|none}}" x="1084" y="202" width="202" height="{{s4.pic.h|80}}"
             preserveAspectRatio="xMidYMid meet" href="{{s4.logo}}"/>
      <text x="1185" y="282" dy="{{s4.name.dy|0}}" font-size="{{s4.name.size|27}}"
            font-weight="600">{{s4.name}}</text>
    </g>

    <g display="{{s5.show|none}}" data-at="{{s5.at|0}}">
      <animate attributeName="opacity" values="{{s5.fade.values|1;1}}"
               keyTimes="{{s5.fade.keyTimes|0;1}}" dur="{{total|8s}}" fill="freeze"/>
      <animateTransform attributeName="transform" type="translate"
                        values="{{s5.move.values|0 0;0 0}}" keyTimes="{{s5.move.keyTimes|0;1}}"
                        dur="{{total|8s}}" fill="freeze"/>
      <rect x="1314" y="188" width="230" height="108" rx="10" fill="#2b313b" stroke="#ff7f7f" stroke-width="3"/>
      <image display="{{s5.pic|none}}" x="1328" y="202" width="202" height="{{s5.pic.h|80}}"
             preserveAspectRatio="xMidYMid meet" href="{{s5.logo}}"/>
      <text x="1429" y="282" dy="{{s5.name.dy|0}}" font-size="{{s5.name.size|27}}"
            font-weight="600">{{s5.name}}</text>
    </g>

    <g display="{{s6.show|none}}" data-at="{{s6.at|0}}">
      <animate attributeName="opacity" values="{{s6.fade.values|1;1}}"
               keyTimes="{{s6.fade.keyTimes|0;1}}" dur="{{total|8s}}" fill="freeze"/>
      <animateTransform attributeName="transform" type="translate"
                        values="{{s6.move.values|0 0;0 0}}" keyTimes="{{s6.move.keyTimes|0;1}}"
                        dur="{{total|8s}}" fill="freeze"/>
      <rect x="1558" y="188" width="230" height="108" rx="10" fill="#2b313b" stroke="#ff7f7f" stroke-width="3"/>
      <image display="{{s6.pic|none}}" x="1572" y="202" width="202" height="{{s6.pic.h|80}}"
             preserveAspectRatio="xMidYMid meet" href="{{s6.logo}}"/>
      <text x="1673" y="282" dy="{{s6.name.dy|0}}" font-size="{{s6.name.size|27}}"
            font-weight="600">{{s6.name}}</text>
    </g>
  </g>

  <!-- ==== tier A =========================================================== -->
  <!-- Input: A[keep logo] | Tier A | up to six, comma separated: a name, a logo file, or Name|logo.png -->
  <g data-at="{{a.at|0}}">
    <animate attributeName="opacity" values="{{a.fade.values|1;1}}"
             keyTimes="{{a.fade.keyTimes|0;1}}" dur="{{total|8s}}" fill="freeze"/>
    <animateTransform attributeName="transform" type="translate"
                      values="{{a.fly.values|0 0;0 0}}" keyTimes="{{a.fly.keyTimes|0;1}}"
                      dur="{{total|8s}}" fill="freeze"/>
    <rect x="120" y="320" width="190" height="124" rx="12" fill="#ffbf7f"/>
    <text x="215" y="406.5" font-size="70" font-weight="700" fill="#16181d">A</text>
    <rect x="324" y="320" width="1478" height="124" rx="12" fill="#20242b"/>

    <g display="{{a1.show|none}}" data-at="{{a1.at|0}}">
      <animate attributeName="opacity" values="{{a1.fade.values|1;1}}"
               keyTimes="{{a1.fade.keyTimes|0;1}}" dur="{{total|8s}}" fill="freeze"/>
      <animateTransform attributeName="transform" type="translate"
                        values="{{a1.move.values|0 0;0 0}}" keyTimes="{{a1.move.keyTimes|0;1}}"
                        dur="{{total|8s}}" fill="freeze"/>
      <rect x="338" y="328" width="230" height="108" rx="10" fill="#2b313b" stroke="#ffbf7f" stroke-width="3"/>
      <image display="{{a1.pic|none}}" x="352" y="342" width="202" height="{{a1.pic.h|80}}"
             preserveAspectRatio="xMidYMid meet" href="{{a1.logo}}"/>
      <text x="453" y="422" dy="{{a1.name.dy|0}}" font-size="{{a1.name.size|27}}"
            font-weight="600">{{a1.name}}</text>
    </g>

    <g display="{{a2.show|none}}" data-at="{{a2.at|0}}">
      <animate attributeName="opacity" values="{{a2.fade.values|1;1}}"
               keyTimes="{{a2.fade.keyTimes|0;1}}" dur="{{total|8s}}" fill="freeze"/>
      <animateTransform attributeName="transform" type="translate"
                        values="{{a2.move.values|0 0;0 0}}" keyTimes="{{a2.move.keyTimes|0;1}}"
                        dur="{{total|8s}}" fill="freeze"/>
      <rect x="582" y="328" width="230" height="108" rx="10" fill="#2b313b" stroke="#ffbf7f" stroke-width="3"/>
      <image display="{{a2.pic|none}}" x="596" y="342" width="202" height="{{a2.pic.h|80}}"
             preserveAspectRatio="xMidYMid meet" href="{{a2.logo}}"/>
      <text x="697" y="422" dy="{{a2.name.dy|0}}" font-size="{{a2.name.size|27}}"
            font-weight="600">{{a2.name}}</text>
    </g>

    <g display="{{a3.show|none}}" data-at="{{a3.at|0}}">
      <animate attributeName="opacity" values="{{a3.fade.values|1;1}}"
               keyTimes="{{a3.fade.keyTimes|0;1}}" dur="{{total|8s}}" fill="freeze"/>
      <animateTransform attributeName="transform" type="translate"
                        values="{{a3.move.values|0 0;0 0}}" keyTimes="{{a3.move.keyTimes|0;1}}"
                        dur="{{total|8s}}" fill="freeze"/>
      <rect x="826" y="328" width="230" height="108" rx="10" fill="#2b313b" stroke="#ffbf7f" stroke-width="3"/>
      <image display="{{a3.pic|none}}" x="840" y="342" width="202" height="{{a3.pic.h|80}}"
             preserveAspectRatio="xMidYMid meet" href="{{a3.logo}}"/>
      <text x="941" y="422" dy="{{a3.name.dy|0}}" font-size="{{a3.name.size|27}}"
            font-weight="600">{{a3.name}}</text>
    </g>

    <g display="{{a4.show|none}}" data-at="{{a4.at|0}}">
      <animate attributeName="opacity" values="{{a4.fade.values|1;1}}"
               keyTimes="{{a4.fade.keyTimes|0;1}}" dur="{{total|8s}}" fill="freeze"/>
      <animateTransform attributeName="transform" type="translate"
                        values="{{a4.move.values|0 0;0 0}}" keyTimes="{{a4.move.keyTimes|0;1}}"
                        dur="{{total|8s}}" fill="freeze"/>
      <rect x="1070" y="328" width="230" height="108" rx="10" fill="#2b313b" stroke="#ffbf7f" stroke-width="3"/>
      <image display="{{a4.pic|none}}" x="1084" y="342" width="202" height="{{a4.pic.h|80}}"
             preserveAspectRatio="xMidYMid meet" href="{{a4.logo}}"/>
      <text x="1185" y="422" dy="{{a4.name.dy|0}}" font-size="{{a4.name.size|27}}"
            font-weight="600">{{a4.name}}</text>
    </g>

    <g display="{{a5.show|none}}" data-at="{{a5.at|0}}">
      <animate attributeName="opacity" values="{{a5.fade.values|1;1}}"
               keyTimes="{{a5.fade.keyTimes|0;1}}" dur="{{total|8s}}" fill="freeze"/>
      <animateTransform attributeName="transform" type="translate"
                        values="{{a5.move.values|0 0;0 0}}" keyTimes="{{a5.move.keyTimes|0;1}}"
                        dur="{{total|8s}}" fill="freeze"/>
      <rect x="1314" y="328" width="230" height="108" rx="10" fill="#2b313b" stroke="#ffbf7f" stroke-width="3"/>
      <image display="{{a5.pic|none}}" x="1328" y="342" width="202" height="{{a5.pic.h|80}}"
             preserveAspectRatio="xMidYMid meet" href="{{a5.logo}}"/>
      <text x="1429" y="422" dy="{{a5.name.dy|0}}" font-size="{{a5.name.size|27}}"
            font-weight="600">{{a5.name}}</text>
    </g>

    <g display="{{a6.show|none}}" data-at="{{a6.at|0}}">
      <animate attributeName="opacity" values="{{a6.fade.values|1;1}}"
               keyTimes="{{a6.fade.keyTimes|0;1}}" dur="{{total|8s}}" fill="freeze"/>
      <animateTransform attributeName="transform" type="translate"
                        values="{{a6.move.values|0 0;0 0}}" keyTimes="{{a6.move.keyTimes|0;1}}"
                        dur="{{total|8s}}" fill="freeze"/>
      <rect x="1558" y="328" width="230" height="108" rx="10" fill="#2b313b" stroke="#ffbf7f" stroke-width="3"/>
      <image display="{{a6.pic|none}}" x="1572" y="342" width="202" height="{{a6.pic.h|80}}"
             preserveAspectRatio="xMidYMid meet" href="{{a6.logo}}"/>
      <text x="1673" y="422" dy="{{a6.name.dy|0}}" font-size="{{a6.name.size|27}}"
            font-weight="600">{{a6.name}}</text>
    </g>
  </g>

  <!-- ==== tier B =========================================================== -->
  <!-- Input: B[keep logo] | Tier B | up to six, comma separated: a name, a logo file, or Name|logo.png -->
  <g data-at="{{b.at|0}}">
    <animate attributeName="opacity" values="{{b.fade.values|1;1}}"
             keyTimes="{{b.fade.keyTimes|0;1}}" dur="{{total|8s}}" fill="freeze"/>
    <animateTransform attributeName="transform" type="translate"
                      values="{{b.fly.values|0 0;0 0}}" keyTimes="{{b.fly.keyTimes|0;1}}"
                      dur="{{total|8s}}" fill="freeze"/>
    <rect x="120" y="460" width="190" height="124" rx="12" fill="#ffdf7f"/>
    <text x="215" y="546.5" font-size="70" font-weight="700" fill="#16181d">B</text>
    <rect x="324" y="460" width="1478" height="124" rx="12" fill="#20242b"/>

    <g display="{{b1.show|none}}" data-at="{{b1.at|0}}">
      <animate attributeName="opacity" values="{{b1.fade.values|1;1}}"
               keyTimes="{{b1.fade.keyTimes|0;1}}" dur="{{total|8s}}" fill="freeze"/>
      <animateTransform attributeName="transform" type="translate"
                        values="{{b1.move.values|0 0;0 0}}" keyTimes="{{b1.move.keyTimes|0;1}}"
                        dur="{{total|8s}}" fill="freeze"/>
      <rect x="338" y="468" width="230" height="108" rx="10" fill="#2b313b" stroke="#ffdf7f" stroke-width="3"/>
      <image display="{{b1.pic|none}}" x="352" y="482" width="202" height="{{b1.pic.h|80}}"
             preserveAspectRatio="xMidYMid meet" href="{{b1.logo}}"/>
      <text x="453" y="562" dy="{{b1.name.dy|0}}" font-size="{{b1.name.size|27}}"
            font-weight="600">{{b1.name}}</text>
    </g>

    <g display="{{b2.show|none}}" data-at="{{b2.at|0}}">
      <animate attributeName="opacity" values="{{b2.fade.values|1;1}}"
               keyTimes="{{b2.fade.keyTimes|0;1}}" dur="{{total|8s}}" fill="freeze"/>
      <animateTransform attributeName="transform" type="translate"
                        values="{{b2.move.values|0 0;0 0}}" keyTimes="{{b2.move.keyTimes|0;1}}"
                        dur="{{total|8s}}" fill="freeze"/>
      <rect x="582" y="468" width="230" height="108" rx="10" fill="#2b313b" stroke="#ffdf7f" stroke-width="3"/>
      <image display="{{b2.pic|none}}" x="596" y="482" width="202" height="{{b2.pic.h|80}}"
             preserveAspectRatio="xMidYMid meet" href="{{b2.logo}}"/>
      <text x="697" y="562" dy="{{b2.name.dy|0}}" font-size="{{b2.name.size|27}}"
            font-weight="600">{{b2.name}}</text>
    </g>

    <g display="{{b3.show|none}}" data-at="{{b3.at|0}}">
      <animate attributeName="opacity" values="{{b3.fade.values|1;1}}"
               keyTimes="{{b3.fade.keyTimes|0;1}}" dur="{{total|8s}}" fill="freeze"/>
      <animateTransform attributeName="transform" type="translate"
                        values="{{b3.move.values|0 0;0 0}}" keyTimes="{{b3.move.keyTimes|0;1}}"
                        dur="{{total|8s}}" fill="freeze"/>
      <rect x="826" y="468" width="230" height="108" rx="10" fill="#2b313b" stroke="#ffdf7f" stroke-width="3"/>
      <image display="{{b3.pic|none}}" x="840" y="482" width="202" height="{{b3.pic.h|80}}"
             preserveAspectRatio="xMidYMid meet" href="{{b3.logo}}"/>
      <text x="941" y="562" dy="{{b3.name.dy|0}}" font-size="{{b3.name.size|27}}"
            font-weight="600">{{b3.name}}</text>
    </g>

    <g display="{{b4.show|none}}" data-at="{{b4.at|0}}">
      <animate attributeName="opacity" values="{{b4.fade.values|1;1}}"
               keyTimes="{{b4.fade.keyTimes|0;1}}" dur="{{total|8s}}" fill="freeze"/>
      <animateTransform attributeName="transform" type="translate"
                        values="{{b4.move.values|0 0;0 0}}" keyTimes="{{b4.move.keyTimes|0;1}}"
                        dur="{{total|8s}}" fill="freeze"/>
      <rect x="1070" y="468" width="230" height="108" rx="10" fill="#2b313b" stroke="#ffdf7f" stroke-width="3"/>
      <image display="{{b4.pic|none}}" x="1084" y="482" width="202" height="{{b4.pic.h|80}}"
             preserveAspectRatio="xMidYMid meet" href="{{b4.logo}}"/>
      <text x="1185" y="562" dy="{{b4.name.dy|0}}" font-size="{{b4.name.size|27}}"
            font-weight="600">{{b4.name}}</text>
    </g>

    <g display="{{b5.show|none}}" data-at="{{b5.at|0}}">
      <animate attributeName="opacity" values="{{b5.fade.values|1;1}}"
               keyTimes="{{b5.fade.keyTimes|0;1}}" dur="{{total|8s}}" fill="freeze"/>
      <animateTransform attributeName="transform" type="translate"
                        values="{{b5.move.values|0 0;0 0}}" keyTimes="{{b5.move.keyTimes|0;1}}"
                        dur="{{total|8s}}" fill="freeze"/>
      <rect x="1314" y="468" width="230" height="108" rx="10" fill="#2b313b" stroke="#ffdf7f" stroke-width="3"/>
      <image display="{{b5.pic|none}}" x="1328" y="482" width="202" height="{{b5.pic.h|80}}"
             preserveAspectRatio="xMidYMid meet" href="{{b5.logo}}"/>
      <text x="1429" y="562" dy="{{b5.name.dy|0}}" font-size="{{b5.name.size|27}}"
            font-weight="600">{{b5.name}}</text>
    </g>

    <g display="{{b6.show|none}}" data-at="{{b6.at|0}}">
      <animate attributeName="opacity" values="{{b6.fade.values|1;1}}"
               keyTimes="{{b6.fade.keyTimes|0;1}}" dur="{{total|8s}}" fill="freeze"/>
      <animateTransform attributeName="transform" type="translate"
                        values="{{b6.move.values|0 0;0 0}}" keyTimes="{{b6.move.keyTimes|0;1}}"
                        dur="{{total|8s}}" fill="freeze"/>
      <rect x="1558" y="468" width="230" height="108" rx="10" fill="#2b313b" stroke="#ffdf7f" stroke-width="3"/>
      <image display="{{b6.pic|none}}" x="1572" y="482" width="202" height="{{b6.pic.h|80}}"
             preserveAspectRatio="xMidYMid meet" href="{{b6.logo}}"/>
      <text x="1673" y="562" dy="{{b6.name.dy|0}}" font-size="{{b6.name.size|27}}"
            font-weight="600">{{b6.name}}</text>
    </g>
  </g>

  <!-- ==== tier C =========================================================== -->
  <!-- Input: C[keep logo] | Tier C | up to six, comma separated: a name, a logo file, or Name|logo.png -->
  <g data-at="{{c.at|0}}">
    <animate attributeName="opacity" values="{{c.fade.values|1;1}}"
             keyTimes="{{c.fade.keyTimes|0;1}}" dur="{{total|8s}}" fill="freeze"/>
    <animateTransform attributeName="transform" type="translate"
                      values="{{c.fly.values|0 0;0 0}}" keyTimes="{{c.fly.keyTimes|0;1}}"
                      dur="{{total|8s}}" fill="freeze"/>
    <rect x="120" y="600" width="190" height="124" rx="12" fill="#ffff7f"/>
    <text x="215" y="686.5" font-size="70" font-weight="700" fill="#16181d">C</text>
    <rect x="324" y="600" width="1478" height="124" rx="12" fill="#20242b"/>

    <g display="{{c1.show|none}}" data-at="{{c1.at|0}}">
      <animate attributeName="opacity" values="{{c1.fade.values|1;1}}"
               keyTimes="{{c1.fade.keyTimes|0;1}}" dur="{{total|8s}}" fill="freeze"/>
      <animateTransform attributeName="transform" type="translate"
                        values="{{c1.move.values|0 0;0 0}}" keyTimes="{{c1.move.keyTimes|0;1}}"
                        dur="{{total|8s}}" fill="freeze"/>
      <rect x="338" y="608" width="230" height="108" rx="10" fill="#2b313b" stroke="#ffff7f" stroke-width="3"/>
      <image display="{{c1.pic|none}}" x="352" y="622" width="202" height="{{c1.pic.h|80}}"
             preserveAspectRatio="xMidYMid meet" href="{{c1.logo}}"/>
      <text x="453" y="702" dy="{{c1.name.dy|0}}" font-size="{{c1.name.size|27}}"
            font-weight="600">{{c1.name}}</text>
    </g>

    <g display="{{c2.show|none}}" data-at="{{c2.at|0}}">
      <animate attributeName="opacity" values="{{c2.fade.values|1;1}}"
               keyTimes="{{c2.fade.keyTimes|0;1}}" dur="{{total|8s}}" fill="freeze"/>
      <animateTransform attributeName="transform" type="translate"
                        values="{{c2.move.values|0 0;0 0}}" keyTimes="{{c2.move.keyTimes|0;1}}"
                        dur="{{total|8s}}" fill="freeze"/>
      <rect x="582" y="608" width="230" height="108" rx="10" fill="#2b313b" stroke="#ffff7f" stroke-width="3"/>
      <image display="{{c2.pic|none}}" x="596" y="622" width="202" height="{{c2.pic.h|80}}"
             preserveAspectRatio="xMidYMid meet" href="{{c2.logo}}"/>
      <text x="697" y="702" dy="{{c2.name.dy|0}}" font-size="{{c2.name.size|27}}"
            font-weight="600">{{c2.name}}</text>
    </g>

    <g display="{{c3.show|none}}" data-at="{{c3.at|0}}">
      <animate attributeName="opacity" values="{{c3.fade.values|1;1}}"
               keyTimes="{{c3.fade.keyTimes|0;1}}" dur="{{total|8s}}" fill="freeze"/>
      <animateTransform attributeName="transform" type="translate"
                        values="{{c3.move.values|0 0;0 0}}" keyTimes="{{c3.move.keyTimes|0;1}}"
                        dur="{{total|8s}}" fill="freeze"/>
      <rect x="826" y="608" width="230" height="108" rx="10" fill="#2b313b" stroke="#ffff7f" stroke-width="3"/>
      <image display="{{c3.pic|none}}" x="840" y="622" width="202" height="{{c3.pic.h|80}}"
             preserveAspectRatio="xMidYMid meet" href="{{c3.logo}}"/>
      <text x="941" y="702" dy="{{c3.name.dy|0}}" font-size="{{c3.name.size|27}}"
            font-weight="600">{{c3.name}}</text>
    </g>

    <g display="{{c4.show|none}}" data-at="{{c4.at|0}}">
      <animate attributeName="opacity" values="{{c4.fade.values|1;1}}"
               keyTimes="{{c4.fade.keyTimes|0;1}}" dur="{{total|8s}}" fill="freeze"/>
      <animateTransform attributeName="transform" type="translate"
                        values="{{c4.move.values|0 0;0 0}}" keyTimes="{{c4.move.keyTimes|0;1}}"
                        dur="{{total|8s}}" fill="freeze"/>
      <rect x="1070" y="608" width="230" height="108" rx="10" fill="#2b313b" stroke="#ffff7f" stroke-width="3"/>
      <image display="{{c4.pic|none}}" x="1084" y="622" width="202" height="{{c4.pic.h|80}}"
             preserveAspectRatio="xMidYMid meet" href="{{c4.logo}}"/>
      <text x="1185" y="702" dy="{{c4.name.dy|0}}" font-size="{{c4.name.size|27}}"
            font-weight="600">{{c4.name}}</text>
    </g>

    <g display="{{c5.show|none}}" data-at="{{c5.at|0}}">
      <animate attributeName="opacity" values="{{c5.fade.values|1;1}}"
               keyTimes="{{c5.fade.keyTimes|0;1}}" dur="{{total|8s}}" fill="freeze"/>
      <animateTransform attributeName="transform" type="translate"
                        values="{{c5.move.values|0 0;0 0}}" keyTimes="{{c5.move.keyTimes|0;1}}"
                        dur="{{total|8s}}" fill="freeze"/>
      <rect x="1314" y="608" width="230" height="108" rx="10" fill="#2b313b" stroke="#ffff7f" stroke-width="3"/>
      <image display="{{c5.pic|none}}" x="1328" y="622" width="202" height="{{c5.pic.h|80}}"
             preserveAspectRatio="xMidYMid meet" href="{{c5.logo}}"/>
      <text x="1429" y="702" dy="{{c5.name.dy|0}}" font-size="{{c5.name.size|27}}"
            font-weight="600">{{c5.name}}</text>
    </g>

    <g display="{{c6.show|none}}" data-at="{{c6.at|0}}">
      <animate attributeName="opacity" values="{{c6.fade.values|1;1}}"
               keyTimes="{{c6.fade.keyTimes|0;1}}" dur="{{total|8s}}" fill="freeze"/>
      <animateTransform attributeName="transform" type="translate"
                        values="{{c6.move.values|0 0;0 0}}" keyTimes="{{c6.move.keyTimes|0;1}}"
                        dur="{{total|8s}}" fill="freeze"/>
      <rect x="1558" y="608" width="230" height="108" rx="10" fill="#2b313b" stroke="#ffff7f" stroke-width="3"/>
      <image display="{{c6.pic|none}}" x="1572" y="622" width="202" height="{{c6.pic.h|80}}"
             preserveAspectRatio="xMidYMid meet" href="{{c6.logo}}"/>
      <text x="1673" y="702" dy="{{c6.name.dy|0}}" font-size="{{c6.name.size|27}}"
            font-weight="600">{{c6.name}}</text>
    </g>
  </g>

  <!-- ==== tier D =========================================================== -->
  <!-- Input: D[keep logo] | Tier D | up to six, comma separated: a name, a logo file, or Name|logo.png -->
  <g data-at="{{d.at|0}}">
    <animate attributeName="opacity" values="{{d.fade.values|1;1}}"
             keyTimes="{{d.fade.keyTimes|0;1}}" dur="{{total|8s}}" fill="freeze"/>
    <animateTransform attributeName="transform" type="translate"
                      values="{{d.fly.values|0 0;0 0}}" keyTimes="{{d.fly.keyTimes|0;1}}"
                      dur="{{total|8s}}" fill="freeze"/>
    <rect x="120" y="740" width="190" height="124" rx="12" fill="#bfff7f"/>
    <text x="215" y="826.5" font-size="70" font-weight="700" fill="#16181d">D</text>
    <rect x="324" y="740" width="1478" height="124" rx="12" fill="#20242b"/>

    <g display="{{d1.show|none}}" data-at="{{d1.at|0}}">
      <animate attributeName="opacity" values="{{d1.fade.values|1;1}}"
               keyTimes="{{d1.fade.keyTimes|0;1}}" dur="{{total|8s}}" fill="freeze"/>
      <animateTransform attributeName="transform" type="translate"
                        values="{{d1.move.values|0 0;0 0}}" keyTimes="{{d1.move.keyTimes|0;1}}"
                        dur="{{total|8s}}" fill="freeze"/>
      <rect x="338" y="748" width="230" height="108" rx="10" fill="#2b313b" stroke="#bfff7f" stroke-width="3"/>
      <image display="{{d1.pic|none}}" x="352" y="762" width="202" height="{{d1.pic.h|80}}"
             preserveAspectRatio="xMidYMid meet" href="{{d1.logo}}"/>
      <text x="453" y="842" dy="{{d1.name.dy|0}}" font-size="{{d1.name.size|27}}"
            font-weight="600">{{d1.name}}</text>
    </g>

    <g display="{{d2.show|none}}" data-at="{{d2.at|0}}">
      <animate attributeName="opacity" values="{{d2.fade.values|1;1}}"
               keyTimes="{{d2.fade.keyTimes|0;1}}" dur="{{total|8s}}" fill="freeze"/>
      <animateTransform attributeName="transform" type="translate"
                        values="{{d2.move.values|0 0;0 0}}" keyTimes="{{d2.move.keyTimes|0;1}}"
                        dur="{{total|8s}}" fill="freeze"/>
      <rect x="582" y="748" width="230" height="108" rx="10" fill="#2b313b" stroke="#bfff7f" stroke-width="3"/>
      <image display="{{d2.pic|none}}" x="596" y="762" width="202" height="{{d2.pic.h|80}}"
             preserveAspectRatio="xMidYMid meet" href="{{d2.logo}}"/>
      <text x="697" y="842" dy="{{d2.name.dy|0}}" font-size="{{d2.name.size|27}}"
            font-weight="600">{{d2.name}}</text>
    </g>

    <g display="{{d3.show|none}}" data-at="{{d3.at|0}}">
      <animate attributeName="opacity" values="{{d3.fade.values|1;1}}"
               keyTimes="{{d3.fade.keyTimes|0;1}}" dur="{{total|8s}}" fill="freeze"/>
      <animateTransform attributeName="transform" type="translate"
                        values="{{d3.move.values|0 0;0 0}}" keyTimes="{{d3.move.keyTimes|0;1}}"
                        dur="{{total|8s}}" fill="freeze"/>
      <rect x="826" y="748" width="230" height="108" rx="10" fill="#2b313b" stroke="#bfff7f" stroke-width="3"/>
      <image display="{{d3.pic|none}}" x="840" y="762" width="202" height="{{d3.pic.h|80}}"
             preserveAspectRatio="xMidYMid meet" href="{{d3.logo}}"/>
      <text x="941" y="842" dy="{{d3.name.dy|0}}" font-size="{{d3.name.size|27}}"
            font-weight="600">{{d3.name}}</text>
    </g>

    <g display="{{d4.show|none}}" data-at="{{d4.at|0}}">
      <animate attributeName="opacity" values="{{d4.fade.values|1;1}}"
               keyTimes="{{d4.fade.keyTimes|0;1}}" dur="{{total|8s}}" fill="freeze"/>
      <animateTransform attributeName="transform" type="translate"
                        values="{{d4.move.values|0 0;0 0}}" keyTimes="{{d4.move.keyTimes|0;1}}"
                        dur="{{total|8s}}" fill="freeze"/>
      <rect x="1070" y="748" width="230" height="108" rx="10" fill="#2b313b" stroke="#bfff7f" stroke-width="3"/>
      <image display="{{d4.pic|none}}" x="1084" y="762" width="202" height="{{d4.pic.h|80}}"
             preserveAspectRatio="xMidYMid meet" href="{{d4.logo}}"/>
      <text x="1185" y="842" dy="{{d4.name.dy|0}}" font-size="{{d4.name.size|27}}"
            font-weight="600">{{d4.name}}</text>
    </g>

    <g display="{{d5.show|none}}" data-at="{{d5.at|0}}">
      <animate attributeName="opacity" values="{{d5.fade.values|1;1}}"
               keyTimes="{{d5.fade.keyTimes|0;1}}" dur="{{total|8s}}" fill="freeze"/>
      <animateTransform attributeName="transform" type="translate"
                        values="{{d5.move.values|0 0;0 0}}" keyTimes="{{d5.move.keyTimes|0;1}}"
                        dur="{{total|8s}}" fill="freeze"/>
      <rect x="1314" y="748" width="230" height="108" rx="10" fill="#2b313b" stroke="#bfff7f" stroke-width="3"/>
      <image display="{{d5.pic|none}}" x="1328" y="762" width="202" height="{{d5.pic.h|80}}"
             preserveAspectRatio="xMidYMid meet" href="{{d5.logo}}"/>
      <text x="1429" y="842" dy="{{d5.name.dy|0}}" font-size="{{d5.name.size|27}}"
            font-weight="600">{{d5.name}}</text>
    </g>

    <g display="{{d6.show|none}}" data-at="{{d6.at|0}}">
      <animate attributeName="opacity" values="{{d6.fade.values|1;1}}"
               keyTimes="{{d6.fade.keyTimes|0;1}}" dur="{{total|8s}}" fill="freeze"/>
      <animateTransform attributeName="transform" type="translate"
                        values="{{d6.move.values|0 0;0 0}}" keyTimes="{{d6.move.keyTimes|0;1}}"
                        dur="{{total|8s}}" fill="freeze"/>
      <rect x="1558" y="748" width="230" height="108" rx="10" fill="#2b313b" stroke="#bfff7f" stroke-width="3"/>
      <image display="{{d6.pic|none}}" x="1572" y="762" width="202" height="{{d6.pic.h|80}}"
             preserveAspectRatio="xMidYMid meet" href="{{d6.logo}}"/>
      <text x="1673" y="842" dy="{{d6.name.dy|0}}" font-size="{{d6.name.size|27}}"
            font-weight="600">{{d6.name}}</text>
    </g>
  </g>

  <!-- ==== tier F =========================================================== -->
  <!-- Input: F[keep logo] | Tier F | up to six, comma separated: a name, a logo file, or Name|logo.png -->
  <g data-at="{{f.at|0}}">
    <animate attributeName="opacity" values="{{f.fade.values|1;1}}"
             keyTimes="{{f.fade.keyTimes|0;1}}" dur="{{total|8s}}" fill="freeze"/>
    <animateTransform attributeName="transform" type="translate"
                      values="{{f.fly.values|0 0;0 0}}" keyTimes="{{f.fly.keyTimes|0;1}}"
                      dur="{{total|8s}}" fill="freeze"/>
    <rect x="120" y="880" width="190" height="124" rx="12" fill="#7fffff"/>
    <text x="215" y="966.5" font-size="70" font-weight="700" fill="#16181d">F</text>
    <rect x="324" y="880" width="1478" height="124" rx="12" fill="#20242b"/>

    <g display="{{f1.show|none}}" data-at="{{f1.at|0}}">
      <animate attributeName="opacity" values="{{f1.fade.values|1;1}}"
               keyTimes="{{f1.fade.keyTimes|0;1}}" dur="{{total|8s}}" fill="freeze"/>
      <animateTransform attributeName="transform" type="translate"
                        values="{{f1.move.values|0 0;0 0}}" keyTimes="{{f1.move.keyTimes|0;1}}"
                        dur="{{total|8s}}" fill="freeze"/>
      <rect x="338" y="888" width="230" height="108" rx="10" fill="#2b313b" stroke="#7fffff" stroke-width="3"/>
      <image display="{{f1.pic|none}}" x="352" y="902" width="202" height="{{f1.pic.h|80}}"
             preserveAspectRatio="xMidYMid meet" href="{{f1.logo}}"/>
      <text x="453" y="982" dy="{{f1.name.dy|0}}" font-size="{{f1.name.size|27}}"
            font-weight="600">{{f1.name}}</text>
    </g>

    <g display="{{f2.show|none}}" data-at="{{f2.at|0}}">
      <animate attributeName="opacity" values="{{f2.fade.values|1;1}}"
               keyTimes="{{f2.fade.keyTimes|0;1}}" dur="{{total|8s}}" fill="freeze"/>
      <animateTransform attributeName="transform" type="translate"
                        values="{{f2.move.values|0 0;0 0}}" keyTimes="{{f2.move.keyTimes|0;1}}"
                        dur="{{total|8s}}" fill="freeze"/>
      <rect x="582" y="888" width="230" height="108" rx="10" fill="#2b313b" stroke="#7fffff" stroke-width="3"/>
      <image display="{{f2.pic|none}}" x="596" y="902" width="202" height="{{f2.pic.h|80}}"
             preserveAspectRatio="xMidYMid meet" href="{{f2.logo}}"/>
      <text x="697" y="982" dy="{{f2.name.dy|0}}" font-size="{{f2.name.size|27}}"
            font-weight="600">{{f2.name}}</text>
    </g>

    <g display="{{f3.show|none}}" data-at="{{f3.at|0}}">
      <animate attributeName="opacity" values="{{f3.fade.values|1;1}}"
               keyTimes="{{f3.fade.keyTimes|0;1}}" dur="{{total|8s}}" fill="freeze"/>
      <animateTransform attributeName="transform" type="translate"
                        values="{{f3.move.values|0 0;0 0}}" keyTimes="{{f3.move.keyTimes|0;1}}"
                        dur="{{total|8s}}" fill="freeze"/>
      <rect x="826" y="888" width="230" height="108" rx="10" fill="#2b313b" stroke="#7fffff" stroke-width="3"/>
      <image display="{{f3.pic|none}}" x="840" y="902" width="202" height="{{f3.pic.h|80}}"
             preserveAspectRatio="xMidYMid meet" href="{{f3.logo}}"/>
      <text x="941" y="982" dy="{{f3.name.dy|0}}" font-size="{{f3.name.size|27}}"
            font-weight="600">{{f3.name}}</text>
    </g>

    <g display="{{f4.show|none}}" data-at="{{f4.at|0}}">
      <animate attributeName="opacity" values="{{f4.fade.values|1;1}}"
               keyTimes="{{f4.fade.keyTimes|0;1}}" dur="{{total|8s}}" fill="freeze"/>
      <animateTransform attributeName="transform" type="translate"
                        values="{{f4.move.values|0 0;0 0}}" keyTimes="{{f4.move.keyTimes|0;1}}"
                        dur="{{total|8s}}" fill="freeze"/>
      <rect x="1070" y="888" width="230" height="108" rx="10" fill="#2b313b" stroke="#7fffff" stroke-width="3"/>
      <image display="{{f4.pic|none}}" x="1084" y="902" width="202" height="{{f4.pic.h|80}}"
             preserveAspectRatio="xMidYMid meet" href="{{f4.logo}}"/>
      <text x="1185" y="982" dy="{{f4.name.dy|0}}" font-size="{{f4.name.size|27}}"
            font-weight="600">{{f4.name}}</text>
    </g>

    <g display="{{f5.show|none}}" data-at="{{f5.at|0}}">
      <animate attributeName="opacity" values="{{f5.fade.values|1;1}}"
               keyTimes="{{f5.fade.keyTimes|0;1}}" dur="{{total|8s}}" fill="freeze"/>
      <animateTransform attributeName="transform" type="translate"
                        values="{{f5.move.values|0 0;0 0}}" keyTimes="{{f5.move.keyTimes|0;1}}"
                        dur="{{total|8s}}" fill="freeze"/>
      <rect x="1314" y="888" width="230" height="108" rx="10" fill="#2b313b" stroke="#7fffff" stroke-width="3"/>
      <image display="{{f5.pic|none}}" x="1328" y="902" width="202" height="{{f5.pic.h|80}}"
             preserveAspectRatio="xMidYMid meet" href="{{f5.logo}}"/>
      <text x="1429" y="982" dy="{{f5.name.dy|0}}" font-size="{{f5.name.size|27}}"
            font-weight="600">{{f5.name}}</text>
    </g>

    <g display="{{f6.show|none}}" data-at="{{f6.at|0}}">
      <animate attributeName="opacity" values="{{f6.fade.values|1;1}}"
               keyTimes="{{f6.fade.keyTimes|0;1}}" dur="{{total|8s}}" fill="freeze"/>
      <animateTransform attributeName="transform" type="translate"
                        values="{{f6.move.values|0 0;0 0}}" keyTimes="{{f6.move.keyTimes|0;1}}"
                        dur="{{total|8s}}" fill="freeze"/>
      <rect x="1558" y="888" width="230" height="108" rx="10" fill="#2b313b" stroke="#7fffff" stroke-width="3"/>
      <image display="{{f6.pic|none}}" x="1572" y="902" width="202" height="{{f6.pic.h|80}}"
             preserveAspectRatio="xMidYMid meet" href="{{f6.logo}}"/>
      <text x="1673" y="982" dy="{{f6.name.dy|0}}" font-size="{{f6.name.size|27}}"
            font-weight="600">{{f6.name}}</text>
    </g>
  </g>

  <!-- what the board is being shown for. It comes last because that is the
       order the boxes come up in: the tiers first, then what has just
       landed on one of them. -->
  <!-- Input: new | Just added | what this board is being shown for, comma separated: D[1.1s]: logo.svg, Name — into row D, 1.1 s in. A bare name is an item already above, which flies in after the recap -->
</svg>
`
