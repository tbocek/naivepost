# 05 — Cut

The session on a timeline: thumbnails per camera row, a waveform lane per sound, everything the cut keeps tinted green. Time nobody filmed takes no width. The page opens as soon as there is footage.

## 1. Screen

```
┌ Cut ───────────────────────────────────────────────────────────────┬──────────────────────────┐
│ ┌────────────────────────────────────────────────────────────────┐ │ Thumbnails:   [🖼−][🖼+]  │
│ │                                                                │ │ Aspect ratio: [source ▾] │
│ │                       preview (click = play/pause)             │ │ Playhead:     12:04.3    │
│ │            ┌───────── camera rect / text boxes ─────────┐      │ │ Selection:    --:--.- – --:--.- │
│ │            └────────────────────────────────────────────┘      │ │ Cut:          11:52.0    │
│ └────────────────────────────────────────────────────────────────┘ │ Cut at 1×:    12:10.0    │
│                                                                    │ Source:       64:02.0    │
│                                                                    │ Segments:     57         │
│                                                                    │ (or a form: Zoom at 12:04 …) │
├────────────────────────────────────────────────────────────────────┴──────────────────────────┤
│ [▶][▶✂][▶✂✂][‹‹f][‹f][f›][f››] 🔊━━━ │ [＋Add][|Split][－Remove][⧉Copy][⧉Paste][Insert][⇲Lane] [✚ Effect ▾] [↶][↷][Revert][✗] [−][+] │
├───────────────────────────────────────────────────────────────────────────────────────────────┤
│ gutter│0:00      0:30      1:00      1:30      2:00      2:30                                   ruler  │
│  [−]  │ ████ scene ✕ ████  −  ████████ scene ✕ ████████      + ▓▓▓ selection ✕ ▓▓▓            green bar / selection band │
│       │  ⊕ zoom 3.0s        ❝ “Welcome…”     ⏩ ×4 · sound 1×                                   effects lane │
│ 🔈 ◉  │ [thumb][thumb][thumb]▒▒▒▒[thumb][thumb]│││[thumb][thumb]     ← row 0 (camera)          pictures │
│       │ ▁▂▃▅▆▅▃▂▁▂▃▅▆▅▃▁▁▁▁▁▂▃▅▆▇▆▅▃▂▁▁                                                          paired wave strip │
│ 🔈 ◉  │ [thumb][thumb]…                                            ← row 1 (second camera / cut lane ✕) │
├───────────────────────────────────────────────────────────────────────────────────────────────┤
│ mic   │ ▁▂▃▅▆▅▃▂▁▂▃▅▆▅▃▁▁▁▁▁▂▃▅▆▇▆▅▃▂▁▁   (separate recordings' lanes; speaker badges per scene)     │
├───────────────────────────────────────────────────────────────────────────────────────────────┤
│ ◄━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━► scrollbar │
└───────────────────────────────────────────────────────────────────────────────────────────────┘
 Inputs: 2 videos · 41:12 · +1 recording          Outputs: [📁] 1 file, 12 kB
```

The red playhead is a 2 px line on its own layer across both bands. The timeline has no tooltips by design; the status line and cursor shapes say what a press would do.

**Toolbar groups** (left to right): transport (▶ recording, ▶✂ cut, ▶✂✂ review, frame steps; the wheel over the bar steps frames, Shift = 5); preview volume; verbs (＋ Add, | Split, － Remove, ⧉ Copy, ⧉ Paste, Insert/Edit, ⇲ Lane); the effects dropdown (✚ Effect: ⊕ Zoom, ❝ Text, ▨ SVG, ⏩ Speed, 🔊 Volume, 🏷 Label); history (Undo Ctrl+Z, Redo Ctrl+Shift+Z, Revert, ✗ Clear); zoom (−, +). Tooltips and greyed rules are in `inventory/cut.md` §A and are normative.

**Form column**: idle readings (thumbnail size, aspect, playhead clock, selection, cut length, cut at 1×, source length, segments) or the form of whatever is being placed or edited (an insert, an effect); forms are live (kept as you type; one Undo takes the whole edit back) with a pinned heading (✕ closes) and a pinned footer for buttons.

**Tracks**: gutter (fold-all badge, per-lane and per-row sound switches, empty-row ✕), ruler, selection band (green bar per kept scene with its ✕ and draggable ends; the blue selection with its ✕ and ends; fold −/+ badges), effects lane (one row per overlapping group), picture rows (thumbnails; amber striped edges where a recording starts/ends; each row's own first audio track as a strip below it; camera and speaker badges on the scene under the line or held), the separate recordings' lanes below, then the scrollbar (hidden when everything fits; its thumb is geared at high zoom so one screenful is always 40 px of drag).

## 2. Model (summary; full schema in `01-project-and-files.md` §3)
Session clock; rows by greedy interval colouring with pins; filmed runs and cells; folds have zero width; x↔time through the cells (half-open on the right); zoom 4 px/s at open, floor = fit the filmed length, ceiling 240 px/s; scenes `[s,e)` on a row with inserts, lanes, quiet lists; effects; undo snapshots (segments, effects, aspect, shift, rows, lanes, nrows), depth 50; base = the last suggestion or what the page opened with.

## 3. Transport flows

### F2.1 Play the recording (▶)
S1 If the preview is the cut → switch it to the recording ("preview is the recording again — everything plays, cuts and all"); if already playing, carry on and stop here. S2 Else toggle: pause when playing; play from the red line otherwise. S3 While playing every second plays, cuts and all; playback into a folded seam opens it ("unfolded m:ss — ▶ ran into it"); at the end of a recording the line walks on to the next one (or a camera still rolling at that second). S4 The clock shows session time; the line follows the player ten times a second with a smoothed live clock in between.

### F2.2 Play the cut (▶✂)
S1 Greyed with no clips. S2 If the preview is the recording → switch to the cut (the line snaps onto kept material; "preview is the cut — the clock reads the finished video"); if playing, carry on. S3 If a review is running → end the review and carry on as the plain cut. S4 Else toggle. S5 While playing: removed stretches are skipped (the line jumps to the next clip's first playable second; past the last clip playback pauses); the next jump is preloaded three seconds ahead in a spare pipeline so the join plays without a freeze (P.preview.preloadLead); speed effects run at their flat rate; stops show their still; volume effects apply; the clock reads the cut's own time. S6 The dropped stretches are dimmed on the tracks.

### F2.3 Review every cut (▶✂✂)
S1 Greyed with fewer than two clips ("nothing to review — a cut needs two clips to have a join between them"). S2 Start from the red line: inside a join's window (P.policy.reviewPad = 10 s before the join to 10 s after it, clamped to the clips) → play on; between windows → seek to the next join's run-up; past the last → the first join. S3 Play as the cut; when the seconds after a join have played, seek to the next join's run-up unless it is already behind the line, in which case play on into it. S4 Status "reviewing cut N of M — 10 s before and after the join at m:ss"; after the last: pause, "reviewed all N cuts". S5 Pressing ▶✂✂ while it runs pauses and ends the review; ▶ or ▶✂ switch over without stopping; moving the line by hand ends it ("the line was moved — the cut review is over; ▶✂✂ starts it again").

**One rule for the three buttons**: each wears ⏸ only while its own thing runs; pressing that one pauses; pressing another switches the preview over without stopping; exactly one of the three is lit (▶ recording, ▶✂ cut, ▶✂✂ review).

### F2.4 Place and step the line
S1 A click on a track puts the red line there, clears the selection, watches that row, and takes the scene under the click in hand. S2 ‹f f› step one frame (Shift/‹‹f f›› five) of the recording under the line and pause; with an edge, clip or effect held they nudge that instead. S3 ←/→ do the same only while something is held; Space toggles play. S4 The line's position is remembered in `cut/line.json` (written at most once a second while it moves, flushed on close) and restored once per project on the next open when a recording still covers it, cued so the picture is that frame and scrolled into view.

### F2.5 Hush and mix (what the preview hears)
The separate recordings overlapping the footage play as their own pipelines in sync; a lane the scene under the line silences is never started; the footage's own sound is muted by property when silenced; a lane started under a scene boundary is seeked with a stop at it. Rate and gain follow the effects under the line. The preview volume is one number shared by every preview.

## 4. Editing flows

### F2.6 Select
S1 Left-drag on a picture row, a wave strip or a lane draws a selection scoped to what it was drawn on (footage of that row, or one recording's sound). S2 The band's ends resize it, its middle moves it, its ✕ clears it; ends snap to clip borders, recording ends, effect ends and the playhead within 8 px. S3 In/out marks and the Selection readout follow the band. S4 A sound selection greys Add/Split/Remove and re-aims Copy and Insert at sound.

### F2.7 Add, Split, Remove, ⌦
- **Add**: keep the selection as scenes (one per filmed run; under P.policy.minSceneSeconds = 1 s refused: "nothing to add: X s selected, a scene is 1 s or more"); both ends snap to word edges, silences, line edges and visual cuts within 5 s; the span is taken off every other row ("added on <cam>, and taken off the other camera — ↶ Undo (Ctrl+Z) takes it back").
- **Split**: a border at each end of the selection, nothing removed (halves ≥ 0.04 s; the right half is marked so coalescing does not undo it); with no selection, one border at the red line and the right half taken in hand.
- **Remove**: drop exactly the selection ("removed X s — the scene it went through is two now (↶ Undo takes it back)"); remainders ≥ 0.04 s survive.
- **⌦ / Delete**: in order: a held effect; a held clip (the only way to remove a spliced card); a selection; the scene under the line; else "nothing selected — click a kept scene, or drag a region on a track".
- **✕ badges**: on a green bar (drop that scene), on the selection, on a cut lane, on an empty row, on an effect.

### F2.8 Trim and move
S1 Wherever the pointer shows a resize arrow (within 6 px of a clip border on the pictures or the green bar), a drag with either button trims that border: an end may not go below start + 1 s, past the next clip or the recording's end; a start not above end − 1 s, below the previous clip or the recording's start; the picture scrubs live (throttled); on release a neighbour within 0.04 s merges, a Split border between them is cleared by the drag ("joined into one scene, a – b (X s) — ↶ Undo puts the border back"), and the picture lands on the edge ("clip N: a – b (X s)"). S2 The right button moves: a green bar's middle or a clip on the green slides the scene along its recording (keeping its length, clear of neighbours, snapping flush within 8 px); a press elsewhere on a row slides the whole camera row along the clock (the shift correction), a lane or a strip slides that one recording, and a selection of footage slides the selected scenes; a row change happens when the recording fits; folds near the grab open for the drag and refold after; one Undo per gesture; status "<what> moved +1.23 s". S3 An unmoved press is a click; a right click never moves the line.

### F2.9 Copy, Paste, Lane
S1 Copy takes the selection in hand (≥ 1 s; "copied m:ss – m:ss (X s) — click where it goes, then ⧉ Paste"); the selection survives. S2 Paste at the red line: footage → a spliced insert `copy:<seconds>` (the cut is opened there, those seconds play again, the video gets longer); a sound → laid over the kept footage at the line, one piece per kept stretch, replacing the recording it was copied from ("laid X s of <base> over the footage at m:ss"; refused when no footage is kept there). Pasting consumes the copy; Esc drops it. S3 ⇲ Lane puts a footage copy on a row of its own (a cut lane windowing the file), nothing cut yet ("X s from a is now the <name> lane, starting at b").

### F2.10 Cameras and hearing
S1 The lens badge on a scene chooses which row its picture comes from ("the scene at m:ss is shown from <cam> now"). S2 The speaker badges on each lane of the held/current scene toggle whether that scene hears that lane ("<base> is silent in the scene at m:ss"); the gutter switch toggles a lane for the whole cut. S3 A click on a row watches it in the preview until ▶ takes the preview back to the cut; when the line stands in a kept scene shown from another row the status says once "watching camera N — the cut shows camera M here; ▶ plays the cut".

### F2.11 Folds and rows
S1 The − badge in a gap folds the unfilmed time to a seam; + on the seam unfolds; the gutter badge folds or unfolds all. Folds are a view, never an edit, and never used to measure a drag. S2 An emptied bottom row stays until its ✕; a cut lane's ✕ removes the lane, its pins, shift, pictures and sound.

### F2.12 Insert a card, still, video or sound
S1 Needs a line or a selection ("click the timeline where the insert goes first"). S2 Chooser "Insert a clip, image, animation or sound" (or "Insert a sound over the selected seconds" for a sound-scoped selection), opening in `assets/`, where the built-in SVG cards and `CARDS.md` are written on first use. S3 Defaults: a selection gives the length and means overwrite; none means splice with the file's own length (video/audio duration, an SVG's animation length, else P.policy.insertDefaultSeconds = 4). S4 Form in the column: one entry per declared card field (a Logo… picker for logo fields); radios Insert BETWEEN the footage / Play OVER the footage / Put it on a LANE of its own (video only); the sound tick ("Play it SILENT — the insert's own sound is not used" when spliced; "Keep the sound running under it — only the picture is replaced" when over); Seconds. S5 Placed: a spliced insert `s == e` with `dur`; an overwriting one replaces those seconds; a sound lays over kept footage; a lane adds a row. Status "<file> inserted at m:ss for X s, <how> — the cut is now a (was b) — ↶ Undo takes it back". S6 Right-click a card on the track to hold it; Insert becomes Edit and re-opens the form; switching modes returns or takes footage.
S7 Preview: cards render at 8 fps (one frame for stills) through ffmpeg, the nearest rendered frame shown; a spliced card holds the footage while it plays on the wall clock; card sound is its own pipeline.

### F2.13 Undo, Redo, Revert, Clear
Undo/Redo walk the snapshots ("undone — N segment(s) left"); Revert restores the base ("reverted to the N segment(s) of the last suggestion (↶ Undo brings your edits back)"); Clear takes every scene and effect off in one step ("cleared N scene(s) and M effect(s)"; sources, rows, shifts and lanes stay).

## 5. Suggest (▶ on the run bar while the preview is not started)

### F2.14 Suggest a cut
S1 Guards: busy; hand edits ("you have hand edits — press Revert first for a fresh suggestion"); no session timeline ("run Describe first — the suggestion reads the session timeline, and there is none").
S2a **Text-derived styles (Lecture)**: no model. Marks from `retakes.tsv`, or remade from `final.txt` when it was edited later (F1.12). Cut = hand-placed inserts + every filmed run trimmed to its words (from just before the first to just after the last, placed by the sound within 0.4 s, never outside the run) → marked stretches removed → dead air removed (silences longer than P.policy.deadAirMaxSeconds = 8 inside a clip are cut out leaving P.policy.deadAirKeepSeconds = 0.5) → coalesce → persist → base. Status "cut by the words: N segments"; log ">>> cut by the words: N stretch(es) taken out, m:ss of silence, N segments, M:SS total".
S2b **Model-chosen styles (Gaming)**: four jobs on the bar. (1) The cut: target length from P.policy.targetLengthSeconds (derived from the context by F0.7; prototype: a regex over the context); message = User Context + "SESSION LENGTH: N seconds…" + the target block ("KEEP between A and B seconds of footage, in at most K segments…" or "NO TARGET LENGTH…") + "SESSION TIMELINE:" + `session.txt`; thinking on; web tools offered (dropped after a first rejection). **Tools**: `add_segment`, `remove_segment`, `set_speed`, `finish_cut` (`02-services.md` §3.6); the checks the prototype ran on the whole reply (timestamps past the end with the mm:ss hint; fewer than min(1 + target/30, 4) segments; more than max(target/5, 40); footage outside target × [0.6, 1.2 | 1.5] × [1, 4]) become `finish_cut`'s answer. Up to P.llm.attempts = 3 rounds of correction; "no valid cut after 3 attempts". Streaming progress counts segments. (2) Captions, (3) speeds, (4) decorations: `06-effects.md` F3.9–F3.11.
S3 Apply: one Undo; hand-placed inserts kept; holes ≤ P.policy.seamMaxSeconds (1.5) closed only when somebody talked in them; both edges of every segment snapped (silence midpoint 0.8, word edge 0.9, line edge 0.95, visual cut where nobody talks; within 5 s; outward preferred); marked stretches removed; dead air removed; coalesce; effects clamped to the footage kept and replaced as a list; persist; base. Log ">>> suggested N segments, M:SS total" and ">>> …and N effect(s): the speeds, the captions and the decorations".

```
 ▶ (Cut) ─► guards ─► style?
   Lecture: marks ─► words fence the cut ─► drop marked ─► drop dead air ─► coalesce ─► done (no model)
   Gaming:  cut (LLM, tools) ─► captions (LLM) ─► speeds (LLM) ─► decorations (LLM) ─► walk-back ─► done
```

## 6. Parameters used
P.policy: minSceneSeconds (1.0), minPieceSeconds (0.04), snapToleranceSeconds (5.0), talkPadSeconds (0.2), deadAirMaxSeconds (8), deadAirKeepSeconds (0.5), seamMaxSeconds (1.5), reviewPadSeconds (10), targetLengthSeconds (0 = none), suggestMinSegments/maxSegments formulas, footageWindow factors, maxSpeedRate (4), insertDefaultSeconds (4), captionBatch (5). P.preview: playTick 100 ms, preloadLead 3 s, rateSeekGap 250 ms, thumb batch 6, card fps 8. Engineering: pixel reaches (6, 8, 10, 12 px), band heights, colours, zoom limits, undo depth, waveform cache format.

## 7. Rules
- One thing held at a time; picking up is not an edit; one Undo per drag; an unmoved press is a click.
- A border belongs to both buttons; moving is the right button's own verb; a selection is of what it was drawn on.
- Unfilmed time has no width; the seam second belongs to the later take; preview and render agree on half-open ranges.
- Inserts are files: never trimmed, merged, dropped by a re-suggest or given a hearing answer; a spliced insert costs no session time.
- A clip may not leave its recording; clips never overlap; Remove takes exactly the selection.
- Suggest replaces the footage half and the effects, keeps inserts, and sets the base; every model reply is walked back onto the timeline.
- `cut.json` is written by one function and its existence unlocks Narrate and Produce.
- Nothing decodes inside a draw; the tracks are a window drawn under a translate; the page is rebuilt only when Prepare's output changed.

## 8. Details confirmed against the code (verification pass)
- **Refusals** (normative strings, full list in `inventory/cut.md` §E): Copy "select a stretch of the pictures or of a lane first — ⧉ Copy takes the selection in hand" / "the selection is X s — under 1 s there is nothing worth copying"; sound copy "copied X s of <lane> (a – b) — click where it goes, then ⧉ Paste"; Lane "click the timeline where the new lane starts first" / "nothing is rolling at m:ss any more" / "that copy is too short to be a lane of its own"; Paste "<base> is not in the session any more — the copied sound has nowhere to come from"; ⌦ "⌦ drops footage — the selection is <base>'s sound" / "the playhead is not on a kept scene — click a green one, or drag a region". A sound copy that began before its recording did is read from the lane's first second; a paste with nowhere to go leaves the copy in hand.
- **Preview sound**: each change of the footage's own mute is one log line naming the reason (">>> preview: the footage's own sound is heard again" / "… is muted -- a card or a stop stands over the picture" / "… is muted -- the scene under the line does not hear it"); gain and mute go to an element the app owns, never the player's per-application stream volume (the sound server remembers that between runs), which is reset to full once at build; a failing pipeline says so ("!!! <page>: playback failed — <reason>", status "<page> would not play — see log"; a mix lane "!!! preview: <base> will not play — …").
- **Suggest**: thinking on for the first attempt, off after a reply that was all reasoning (">>> suggest: the model spent the whole call thinking and wrote nothing — asking again with thinking off"); web tools withdrawn on the first rejection (">>> suggest: asking again without the web tools"); progress "N moments, at m:ss of m:ss" placed by the last closed segment's end, never below 0.02 nor backwards, pulsing under "thinking over the whole session" until the first segment closes; segments with no recording at either end are dropped before the length is judged (">>> suggest attempt N: M segment(s) dropped for having no footage"); the checks answer with every fault at once, worst first, joined by "; ", and the past-the-end fault shows the conversion on the model's own number ("2804 is not a second: a stamp [28:04] is mm*60+ss, 1684"); a `speed`/`rate` on a segment is a speed effect over it, exempt from any decorations cap. Streamed progress everywhere is read by counting objects closed inside the last `"<key>": [` of the text (braces inside strings ignored) and the last closed object's `end`.
