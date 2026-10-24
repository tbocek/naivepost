# Questions

The /spec loop writes a question here when the spec does not decide something a round needs, and skips that item until the question has an answer. Write the answer after **Answer:**, in your own words or as the number of an option, then run /spec. An answered question is part of the spec: every later round of the item reads it, and changing the answer rebuilds the item.

## §08-produce#1-screen · The amended sentence says the encoder settings are "laid out as label + control pairs in this order, left to right and then down, two pairs to a line as the picture shows", but spec/img/08-produce.png draws THREE pairs across each of its first two lines (Container: mp4 | Codec: h264 | Preset: veryslow, then Resolution: 1080p | Frame rate: 30 | Audio: 128) and only two across the lines after (Subtitles | Translate, Quality (CRF) | Frame timing, Channels | Frame edges) — which of the two governs the grid?

Asked 2026-10-22 by codehalter v164 (2026-10-21, fe8a10b) while building 1. Screen.

The spec comes closest in `08-produce.md:20`:

> two pairs to a line as the picture shows; each label sits directly left of its own control, and no label or control overlaps another.

Options (the first is the planner's pick):

1. Follow the picture: three dropdown pairs on the first two lines, two on each line after (today's grid); the sentence's real demand is read as the no-overlap half. Example: Line 1 stays `Container: mp4  Codec: h264  Preset: slow`, line 3 `Subtitles: none in the video  Translate`, and this round adds tests that each label sits directly left of its own control with no label or control overlapping another, plus `just snapshot 08-produce` reviewed beside the spec image.
2. Follow the sentence: exactly two pairs on every line, whatever the picture shows. Example: Line 1 `Container: mp4  Codec: h264`, line 2 `Preset: slow  Resolution: 1080p`, line 3 `Frame rate: 30  Audio: 128`, line 4 `Subtitles  Translate`, then wider sliders and ticks two per line — six or seven lines instead of five, and the existing tests that pin three on the first two lines get rewritten.

**Answer:** 1. Follow the picture: three pairs on each of the first two lines, two on each line after. The sentence is corrected in `08-produce.md` to say so; what it must still hold is that each label sits directly left of its own control and nothing overlaps.

---

## §06-effects#2-the-lane-and-the-preview · This item did not pass its attempts. How should the rounds go on?

Asked 2026-10-23 by codehalter v170 (2026-10-22, 26776f3) while building 2. The lane and the preview.

The spec comes closest in `06-effects.md:28`:

> 2. The lane and the preview

What stopped the last attempt:

```
codehalter rendered the screen itself and an independent look put it beside the spec's picture: the arrangement differs. rust/shots/06-lane.png beside spec/img/06-lane.png: Bottom of window: the spec shows an audio waveform strip running along the bottom with a round speaker icon at the far lower-left corner, but the render is cut off above that row — neither the waveform nor the speaker icon is drawn.; Lower-middle region (under the effects lane): the spec shows a full-width row of video frame thumbnails separated from the lane by a dashed divider, but the render shows only a pale, mostly empty grey band with faint horizontal stripes instead of the thumbnail row.; Left edge: the spec has a thin dark gutter column spanning the full height with a small '−' control at its top, but the render lacks this gutter (only a dark rectangle at mid-height with clipped text such as 't-tape').; rust/shots/06-effect-menu.png beside spec/img/06-effect-menu.png: Top of window: the picture shows a header bar across the top (a lightning/dropdown control at the far left, the '+ Effect ▾' button, and undo/redo plus other tool icons at the right); the render has no header bar at all — the menu title '+ Effect ✓' sits flush at the very top edge.; Background/around the menu: the picture shows the timeline area behind and around the open menu (time ruler numbers, green clip bands, video-thumbnail rows and blue audio-waveform rows, and text such as 'Input'); the render shows none of this — the menu alone fills the window.; Menu placement/size: in the picture the menu is a compact popup anchored under the '+ Effect' button in the upper-left, taking only a fraction of the window; in the render the same menu is scaled up to occupy the entire window width and height.. Fix each in the program, the way a user sees the screen: changing only how the snapshot stages it fixes nothing
```

Options:

1. Try again as the spec says. Example: right when the cause was elsewhere and is gone now, such as a build another item broke or a tool installed since: answer `1`, and the next /spec gives it fresh attempts
2. Change what the spec asks. Example: edit the item's section in `spec/06-effects.md` (a smaller step, another format, a service left out) and answer `2`; the next /spec builds the new text
3. Tell the rounds how to get past it. Example: answer `3: test the upload against a local fake server on port 8080` or `3: install libadwaita 1.5 in the Dockerfile first`, and every round of the item reads it

**Answer:** 3: (a) 06-lane is the track area from the ruler down to the bottom of the picture row and its waveform, with the dark gutter running the whole height at the left and the fold-all − at its top: the render stops 132 px down, inside the tape lane, so the picture row, its waveform and the speaker switch are cut off. Make the shot end where the track stack ends, not at a fixed height. (b) 06-effect-menu is the Cut page with the "+ Effect" menu open under its button, the toolbar and the tracks around it: the render shows the menu alone, stretched over the whole window. The menu is a surface of its own, so the snapshot draws the window first and the open menu on top of it, where it opens and at its own size; that is a fix in the snapshot.

---

## F0.7 · This item did not pass its attempts. How should the rounds go on?

Asked 2026-10-24 by codehalter v175 (2026-10-23, 9ef5bbd) while building F0.7 Derive the editing policy (new).

The spec comes closest in `03-shell.md:163`:

> F0.7 Derive the editing policy (new)

What stopped the last attempt:

```
screens of items already done matched their pictures before and no longer do: §05-cut#1-screen's rust/shots/05-cut.png beside spec/img/05-cut.png: Tracks area: the spec picture shows a tall picture row filled with camera-frame thumbnails (with amber striped start/end edges) and a waveform audio strip directly beneath it; the render has no such picture/thumbnail row at all — below the selection band it shows only two flat lane rows (a grey/yellow lane with a thin blue strip and a 'mono' lane with a purple band), with no thumbnails and no waveform strip.; Below the tracks: the render leaves a large empty white band between the last lane row and the bottom run bar, whereas in the spec picture the track content (picture row, waveform strip, scrollbar) fills that vertical space down to the status line.; Bottom run bar (left of centre): the spec picture shows a '1 steps' dropdown button next to the blue play button; the render shows an 'I'm feeling lucky' button in that position instead.. Put each back the way its picture shows, without undoing this item's own work: those items are done and their screens must stay as they were
```

Options:

1. Try again as the spec says. Example: right when the cause was elsewhere and is gone now, such as a build another item broke or a tool installed since: answer `1`, and the next /spec gives it fresh attempts
2. Change what the spec asks. Example: edit the item's section in `spec/03-shell.md` (a smaller step, another format, a service left out) and answer `2`; the next /spec builds the new text
3. Tell the rounds how to get past it. Example: answer `3: test the upload against a local fake server on port 8080` or `3: install libadwaita 1.5 in the Dockerfile first`, and every round of the item reads it

**Answer:** 1. The attempts failed on the done-screen check of 05-cut, and that screen's render was not stable: the snapshot took its picture after a fixed 300 ms and read what earlier runs had left beside the launch, so the same code drew the form column scrolled in one run and at its top in the next. The snapshot now takes its picture at the screen's ready event, in a working directory of its own, and two renders of every screen are identical. Build the item again as the spec says.

---

## §06-effects#2-the-lane-and-the-preview · This item did not pass its attempts. How should the rounds go on?

Asked 2026-10-24 by codehalter v175 (2026-10-23, 9ef5bbd) while building 2. The lane and the preview.

The spec comes closest in `06-effects.md:28`:

> 2. The lane and the preview

What stopped the last attempt:

```
codehalter rendered the screen itself and an independent look put it beside the spec's picture: the arrangement differs. rust/shots/06-lane.png beside spec/img/06-lane.png: Bottom-left corner: the spec shows a round speaker/mute icon at the far lower-left of the waveform strip, but the render has no such round speaker control there.; Left edge: the spec shows a thin dark gutter column running the full height of the track area with a small '−' fold control at its top, but in the render the gutt
…
he way a user sees the screen: changing only how the snapshot stages it fixes nothing. One exception: where the picture shows something open over the window (a menu, a popover, a dialog), that is a surface of its own, and a render of the window alone does not contain it. The snapshot then draws the window first and the open surface on top of it, at the place it opens and at its own size; a render showing the menu alone, stretched over the whole window, is fixed in the snapshot; screens of items already done matched their pictures before and no longer do: §05-cut#1-screen's rust/shots/05-cut.png beside spec/img/05-cut.png: Top-right form column: the picture shows the 'Thumbnails:' row (with its size −/+ buttons) and the 'Aspect ratio:' dropdown above the readings, but the render's form column begins at 'Playhead:', whose top is clipped at the panel's upper edge, with the thumbnail-size and aspect-ratio rows absent (scrolled/cut out of view).; Left edge between the preview and the toolbar: the render draws a vertical stack of small overlapping red/pink button fragments (transport / cut-review / effect labels, e.g. '⏸✂…', '▶✂…', 'Effect') piled on top of one another over the preview's lower-left corner and the track gutter; the picture has no such stack — those controls appear only inside the single horizontal toolbar row.. Put each back the way its picture shows, without undoing this item's own work: those items are done and their screens must stay as they were
```

Options:

1. Try again as the spec says. Example: right when the cause was elsewhere and is gone now, such as a build another item broke or a tool installed since: answer `1`, and the next /spec gives it fresh attempts
2. Change what the spec asks. Example: edit the item's section in `spec/06-effects.md` (a smaller step, another format, a service left out) and answer `2`; the next /spec builds the new text
3. Tell the rounds how to get past it. Example: answer `3: test the upload against a local fake server on port 8080` or `3: install libadwaita 1.5 in the Dockerfile first`, and every round of the item reads it

**Answer:** 3: The earlier picture checks saw an unstable render; the snapshot is deterministic now, and 06-lane shows the gutter with the fold-all − at its top, the thumbnail row and the waveform strip. One difference is real: the gutter at the left also holds the per-lane and per-row sound switches (05-cut.md §1, Tracks: "gutter (…: fold-all badge, per-lane and per-row sound switches, empty-row ✕)"); the picture shows them as round 🔊 buttons at the bottom left of each row, and the render has none. Put them in the gutter.

---

## §07-narrate#1-screen · This item did not pass its attempts. How should the rounds go on?

Asked 2026-10-24 by codehalter v175 (2026-10-23, 9ef5bbd) while building 1. Screen.

The spec comes closest in `07-narrate.md:11`:

> 1. Screen

What stopped the last attempt:

```
codehalter rendered the screen itself and an independent look put it beside the spec's picture: the arrangement differs. rust/shots/07-narrate.png beside spec/img/07-narrate.png: Right 'lines' column: each row's header-line action buttons (▶ speak, ↻ re-roll, ＋ add-below, 🗑 remove) that sit fully inside the window at the right end of each header in the picture are pushed off the right edge in the render, leaving only a thin clipped sliver of a box at the window border.; Right 'lines' column: the row text boxes overflow the window's right edge so their text is cut off mid-word (e.g. row 2 ends '...what a 51 percent', row 3 ends '...the archite', row 4 ends '...the pa'), whereas in the picture every line's text wraps inside the panel.; Left column: there is a large empty vertical band between the transport row (≈y460) and the voice picker (≈y620); in the picture the voice picker begins directly under the transport row with no such gap.. Fix each in the program, the way a user sees the screen: changing only how the snapshot stages it fixes nothing. One exception: where the picture shows something open over the window (a menu, a popover, a dialog), that is a surface of its own, and a render of the window alone does not contain it. The snapshot then draws the window first and the open surface on top of it, at the place it opens and at its own size; a render showing the menu alone, stretched over the whole window, is fixed in the snapshot
```

Options:

1. Try again as the spec says. Example: right when the cause was elsewhere and is gone now, such as a build another item broke or a tool installed since: answer `1`, and the next /spec gives it fresh attempts
2. Change what the spec asks. Example: edit the item's section in `spec/07-narrate.md` (a smaller step, another format, a service left out) and answer `2`; the next /spec builds the new text
3. Tell the rounds how to get past it. Example: answer `3: test the upload against a local fake server on port 8080` or `3: install libadwaita 1.5 in the Dockerfile first`, and every round of the item reads it

**Answer:** 3: Two parts of the screen differ, and each is in the program. (a) The lines column: every line's header row ends with its ▶ speak, ↻ re-roll, + and 🗑 buttons inside the column, and the line's text box wraps its text within the column; the render pushes the buttons past the column's right edge and lets the text run out of it ("…the pa"). (b) The take band under the narrator row (badge 10, the voice reference's waveform with its time ruler) is missing: the snapshot reports "take band seeded = false", so it stages no voice reference; stage one through the same door the app uses, so the band is drawn.
