package main

// The note written into a project's assets folder beside the cards, so whoever
// (or whatever model) writes a second card finds the rules next to the
// examples. Keep it in step with svgcards.go, svganim.go and svgcss.go: the
// same three subsets, described from the document's side.

// cardGuideFile is what the note is called. Uppercase, so it sorts above the
// cards and reads as a note about them rather than as one of them.
const cardGuideFile = "CARDS.md"

// Indented code blocks rather than fenced ones throughout: a Go raw string
// cannot contain a backtick, and four spaces is markdown just as old.
const cardGuide = `# Writing a card

This folder holds *cards*: SVG files that can be dropped into a cut as an
insert. The footage stops, the card is on screen for a few seconds, the footage
carries on.

Naivepost renders SVG itself -- no browser is involved anywhere. A card that
animates is evaluated and written out as one static SVG per frame, and those
frames are handed to ffmpeg as an image sequence. A card that does not animate
is one still held for the length of the insert.

This note is the whole contract. If you are a model being asked to write a new
card for this folder, everything you need is below, and the files beside it are
worked examples of it.

## The canvas

    <svg xmlns="http://www.w3.org/2000/svg" width="1920" height="1080"
         viewBox="0 0 1920 1080">

* Always 1920x1080. The card is scaled to the footage's frame at render time,
  so the numbers below are the only ones you have to think in.
* Draw an opaque background first -- a full-size rect, dark (#14171c is what
  the built-in cards use). A card replaces the picture; it does not sit over it.
* The file must be self-contained. Frames are written to a temporary folder and
  read from there, so a relative href resolves against nothing and an absolute
  one is refused. Put images in the document: href="data:image/png;base64,...".
* Fonts are the ones installed on the machine, named as a family list --
  "DejaVu Sans, Liberation Sans, Helvetica, Arial, sans-serif". No @font-face
  with a URL, no webfont.
* Nothing can measure text before it is drawn. Err small: a heading a size too
  modest still reads, one that runs off the edge does not. About 0.58em of
  advance per character is a safe estimate for these faces.

## Filling one in

A card is worth more than a picture when the same file can say something
different every time it is used. Put placeholders in it:

    <text ...>{{title}}</text>
    <text ...>{{caption|Nobody said}}</text>

* {{name}} is empty when nobody says otherwise; {{name|words}} falls back to
  those words. A placeholder is never left in the picture as literal text.
* They work in attributes as well as in content: fill="{{colour|#ff7f7f}}".
* The insert carries the values on the end of its path, as a URL does:
  card.svg?title=Best maps&caption=Nuke. Inside a value, only % & = ? are
  escaped (%25 %26 %3D %3F); spaces, commas and colons are written as they are.
* Whatever is substituted is XML-escaped, so a title with an & in it is safe.

## Saying what the placeholders are

The insert dialog builds itself out of the file. By default it asks for each
{{name}} under its bare name and says nothing about it, which is enough for a
card with one hole and thin for a card with six. Declare them instead:

    <!-- Input: title | Title | over the card, empty for none -->
    <!-- Input: logo[logo] | Logo | a picture file, from anywhere in the project -->
    <!-- Input: colour | Colour | #rrggbb -->

The key, then what the box is called, then what it says under the box. The key
matches the placeholder. Everything after the second bar is the hint, bars and
all, so a hint may contain a bar.

The square brackets are flags, space separated, all optional:

    keep    the value is written out even when it is left empty
    logo    the box is a comma-separated list that may hold image files, and
            the dialog offers a file picker for it

Declare some and the rest are still asked for -- a declaration adds words to a
box, it does not hide the others. The order of the comments is the order of the
dialog.

A {{name}} written inside a comment is not a parameter: comments are where a
card explains itself, and an example in one is never asked for and never filled
in from a path. The one exception is the Input line itself, which is read.

## Room for more than one thing

A card that ranks things needs somewhere to put them, and a file cannot invent
places to put them in. So write the places out -- all of them, with their own
coordinates -- and switch off the ones nobody filled in:

    <g display="{{s1.show|none}}">
      <rect x="338" y="188" width="230" height="108" rx="10" fill="#2b313b"/>
      <text x="453" y="282" font-size="{{s1.name.size|27}}">{{s1.name}}</text>
    </g>

* display="{{...|none}}" is how an empty place stays empty: the fallback is off,
  and the card switches on the ones it has something for.
* The same trick works for anything optional -- a picture element with no
  picture, a caption nobody wrote.
* It costs nothing to read: a place nobody used is a group with display:none in
  it, and the file still opens as the finished card in any viewer.

tier.svg is written exactly this way: six tiers, six places in each, all thirty
six written out, and one Input line per tier beside the tier it belongs to.

## Animating it

**Every animation begins at 0 and does its waiting inside keyTimes.** Not a
style rule. Before an animation begins, SVG says the static attribute stands, so
an element parked off-screen by a static transform is off-screen for anything
that does not animate -- the file manager's thumbnail, the insert chooser's
preview, an SVG editor, and this app's own fallback when a document turns out
not to bake. With the delay written into the animation, the static document is
the *finished* card, which is what all of those should show.

So, a group that is invisible for the first 1.2 seconds of a 3.4 second card and
fades in over the next 0.4:

    <g>
      <animate attributeName="opacity" dur="3.4s" fill="freeze"
               values="0;0;1;1" keyTimes="0;0.353;0.471;1"/>
      <animate attributeName="transform" type="translate" dur="3.4s" ... />
    </g>

Note the shape of it: one animation per attribute, running the card's whole
length, with the moments as fractions of it, and fill="freeze". Every card in
this folder is written that way.

What is read:

* SMIL -- <animate>, <set>, <animateTransform> -- on numbers, number lists,
  lengths with units, and hex or rgb() colours. from/to/by, values with
  keyTimes, dur, begin, repeatCount, fill="freeze". calcMode discrete steps;
  paced and spline are treated as linear.
* CSS @keyframes in a <style> block, which is what a drawing tool exports:
  opacity, transform and fill; translate, scale and rotate in px and deg; one
  compound selector per rule (#id, .class, tag, and combinations -- no
  descendant selectors, no attribute selectors, no pseudo-classes); animation
  and its longhands; linear, ease, ease-in, ease-out, ease-in-out and
  cubic-bezier() easing.
* An animation element must sit inside the element it animates. A begin that
  chains off another animation or off an event is not read.

Anything outside that renders statically, which is the finished card, which is
never wrong -- only less alive than it could be.

The card's own length is offered as the length of the insert: the last moment
anything is still moving. Leave about two seconds of stillness at the end, so
the card is read after it has arrived rather than while it is arriving.

## The cards this app draws itself

tier.svg and the single letters carry data-naivepost="tier" or "badge" and the
parameters they were drawn with. They are *drawn*, not just filled in: the app
works out which places on the board are used, how big a name has to be to fit,
which one is the thing that just arrived and when everything moves, and puts
those numbers into the file.

tier.svg is the file the board is drawn FROM. The whole picture is in it -- the
six rows, the six places in each, every coordinate -- and the holes are only
what the app has to say: what is in a place, what it is called, and every
animation's own values and keyTimes. Restyle it and the boards drawn from it
come out restyled; that is what it is there for. Rename a box in one of its
Input comments and the dialog calls it that. The single letters are drawn flat:
a badge is one letter and one caption, so editing their shapes does nothing --
they are redrawn from the letter every time.

To write your own card, do not copy the stamp. A document without one, with
{{placeholders}} in it, is filled in exactly as written and is yours.

## A complete example

    <?xml version="1.0" encoding="UTF-8"?>
    <svg xmlns="http://www.w3.org/2000/svg" width="1920" height="1080"
         viewBox="0 0 1920 1080">
      <!-- Input: line | Headline | the one thing this card says -->
      <!-- Input: sub | Under it | smaller, arrives second -->
      <rect width="1920" height="1080" fill="#14171c"/>
      <g>
        <animate attributeName="transform" type="translate" dur="3.2s"
                 fill="freeze" values="1920 0;1920 0;0 0;0 0"
                 keyTimes="0;0.06;0.28;1"/>
        <text x="960" y="520" font-family="DejaVu Sans, Liberation Sans, sans-serif"
              font-size="96" font-weight="700" fill="#e6e9ef"
              text-anchor="middle">{{line|Chapter one}}</text>
      </g>
      <g>
        <animate attributeName="opacity" dur="3.2s" fill="freeze"
                 values="0;0;1;1" keyTimes="0;0.31;0.44;1"/>
        <text x="960" y="620" font-family="DejaVu Sans, Liberation Sans, sans-serif"
              font-size="48" fill="#e6e9ef" text-anchor="middle">{{sub}}</text>
      </g>
    </svg>

## Before you call it done

* Open it in anything that renders SVG. What you see is the card as it ends,
  with the places nobody has filled in yet showing what they are.
* No external file is referenced, and no placeholder is left visible.
* Everything is inside 1920x1080, including text at its longest plausible value.
* Every animation begins at 0, runs the same total, and freezes.
* The last movement ends about two seconds before the card does.
`
