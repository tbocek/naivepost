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

## §06-effects#2-the-lane-and-the-preview · This item did not pass its attempts. How should the rounds go on?

Asked 2026-10-24 by codehalter v177 (2026-10-24, c75be84) while building 2. The lane and the preview.

The spec comes closest in `06-effects.md:28`:

> 2. The lane and the preview

What stopped the last attempt:

```
codehalter rendered the screen itself and an independent look put it beside the spec's picture: the arrangement differs. rust/shots/06-lane.png beside spec/img/06-lane.png: Left edge, thumbnail row: the render draws a lane label that is clipped at the left edge — it reads 'ession-tape', with the first letter cut off / hidden behind the dark gutter column; the picture has no such clipped text at the left of the picture row.; Left gutter / rows: the picture shows round speaker sound switches at the lower-left of the rows (one at the far lower-left corner of the waveform strip and a second one partway along it at the start of the next recording's sound strip), but the render has only a single round speaker at the very bottom-left and lacks the per-row / per-lane sound switches in the gutter.; Bottom of the window: the render has a tall, empty lavender band labelled 'mic-lav mono' occupying roughly the bottom fifth of the window with no waveform or other content drawn in it, whereas in the picture the bottom region is filled by the waveform strip running to the bottom edge.. Fix each in the program, the way a user sees the screen: changing only how the snapshot stages it fixes nothing. One exception: where the picture shows something open over the window (a menu, a popover, a dialog), that is a surface of its own, and a render of the window alone does not contain it. The snapshot then draws the window first and the open surface on top of it, at the place it opens and at its own size; a render showing the menu alone, stretched over the whole window, is fixed in the snapshot
```

Options:

1. Try again as the spec says. Example: right when the cause was elsewhere and is gone now, such as a build another item broke or a tool installed since: answer `1`, and the next /spec gives it fresh attempts
2. Change what the spec asks. Example: edit the item's section in `spec/06-effects.md` (a smaller step, another format, a service left out) and answer `2`; the next /spec builds the new text
3. Tell the rounds how to get past it. Example: answer `3: test the upload against a local fake server on port 8080` or `3: install libadwaita 1.5 in the Dockerfile first`, and every round of the item reads it

**Answer:** 3: The lane label is drawn whole ("session-tape"); it only reads as clipped because its dark text sits on a dark-blue frame, so give the label a light backing behind its text. The real gap: the tape row (the pictures and their waveform) needs its own sound switch in the gutter at its lower left, as the mic-lav lane has one. The 🔊 partway along the row in the picture, at the start of the next clip's sound strip, is the per-clip speaker badge on a clip boundary (05-cut.md Tracks), not part of this item.
