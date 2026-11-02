# Inventory: Cut tab — timeline, transport, editing, suggest, inserts, audio

<!-- nav -->
<span>← start</span> · [↑ Contents](../README.md) · [Inventory: effects (Cut page) and their rendering →](effects.md)
<!-- /nav -->

Raw material for the spec. Names in code style are the prototype's (go/cut*.go, non-effects, player.go, player_spare.go); the behaviour, labels and numbers are the app's.

## A. UI layout

Page = vertical paned (380): top = horizontal paned (660) of preview (GtkPicture contain-fit, min height 160, click toggles play) and the form column; bottom = toolbar, tracks, scrollbar.
Form column: pinned heading (title + ✕ "close this form — nothing is lost that was not already saved"), a scroller holding either the idle rows or a form body, pinned footer for buttons. Idle rows: Aspect ratio [dropdown], Playhead [clock], Selection [marks], Cut [total], Cut at 1× [totalRaw], Source [totalSrc], Segments [totalSegs].

Toolbar (wheel over it steps frames; Shift = 5):

1. linked(▶, ▶✂, ▶✂✂, ‹‹f, ‹f, f›, f››). ▶ "Play the recording from the red line — every second of it, cuts and all" (never greyed). ▶✂ (glyph): "play the CUT instead of the recording: the removed stretches are skipped, so this runs the finished video. The clock reads the cut's own time while it does. Changes nothing that is saved.", replaced while it plays by the cut clock's sentence (§D); sensitive when segs > 0; lit while cutOnly and no review. ▶✂✂ (glyph): sensitive with ≥2 clips; "review every cut in one go: plays 10 s of the finished video before each join and 10 s after it, one join after the other, and stops after the last. The removed stretches are skipped as under ▶✂. Changes nothing that is saved."; lit while reviewing. The four frame buttons share "step the red line by a frame (pauses) — Shift holds five".
2. volume control (icon + 0..100, 120 px, shared tooltip).
3. separator.
4. linked(＋ Add, | Split, － Remove, ⧉ Copy, ⧉ Paste, Insert/Edit, ⇲ Lane) with one tooltip each (Add: "keep the selection as scenes — one per filmed run", or "＋ Add keeps footage — this selection is <base>'s sound" under a sound selection; Split: "a border at each end of the selection — nothing is removed"; Remove: "drop exactly the selection — the cut keeps everything else until you undo"; Copy: "take the selection in hand — a second at least; the band keeps showing it"; Paste: "put what is in hand where the red line is — footage splices in, sound lays over kept footage"; Insert: "put a clip, image, animation or sound into the cut — between the footage, over it, or on a lane"; Lane: "give the copy a row of its own — nothing is cut to it yet").
5. Effects dropdown: ✚ Effect, ⊕ Zoom, ❝ Text, ▨ SVG, ⏩ Speed, 🔊 Volume, 🏷 Label (fires and snaps back).
6. linked(Undo "Undo — take back the last Add, Remove or Suggest (Ctrl+Z)", Redo "Redo — put it back again (Ctrl+Shift+Z, or Ctrl+Y)", Revert "Revert edits — drop everything you added or removed by hand and go back to the last suggestion, or to the cut this page opened with if you have not suggested yet", Clear "Clear: take every kept stretch and every effect off the timeline, leaving the recordings as they were loaded (↶ Undo brings them back)").
7. linked(zoom out "zoom the timeline out — it stops where the whole session is on screen (the scroll wheel does the same, around the cursor)", zoom in "zoom the timeline in — around the middle of what is on screen, and it stops at 240 px a second"), factor 1.25.

Tracks (two drawing areas under a red-line overlay, no tooltips by design), top to bottom in the picture band: gutter (first 30 px of timeline space; fold-all badge, lane sound switches, row switches, empty-row ✕); ruler (18 px, ticks at tickStep(pps): 0.2 0.5 1 2 5 10 30 60 120 300 600 s, first with s·pps ≥ 70); selection band (22 px: green bar per kept scene, blue selection, fold −/+ badges); effects lane (26 px per row); picture rows (pitch thumbHt+4, the row's sound strip inside it: a third of the opening pitch, moved by its grip, 6 px at least and leaving the pictures 16 px); audArea (separate recordings' lanes); scrollbar (hidden when everything fits; thumb geared at high zoom, 40 px of drag per screenful).
Red line 2 px RGB(0.9,0.15,0.15) on its own layer.
Bottom bar: inputs "N clip(s)", no tooltip; "nothing to cut — no source on Inputs is marked as footage" when none. Outputs: the page's summary, and the folder button "Open this project's output folder: <project>/cut". Run bar ▶ = Suggest until the preview has been started, then the preview's transport until ⏹.

## B. Timeline model

Session clock from srcClock over the snapshot; tlVideo{base, path, start, wall, dur, interval, fps (default 30), frames, w, h, lane, off}; at(t)=t−start+off; sessionAt(local)=start+local−off. tlAudio{base, path, start, off, dur, chans, master, track}.
Rows = greedy interval colouring of recordings in start order, pins (cut.json rows) applied first; nRows floor keeps an emptied bottom row until its ✕. Filmed runs = merged recording spans; cells = runs cut at folded gaps; spans carry pixel origins (gutter first; a folded cell has zero width).
x↔time: xOf walks spans (folded cell → its seam x; past the end → totalW); tAt is the inverse, half-open on the right (the seam second belongs to the later take); left of the first run → its start.
Folds are a view, not an edit: persisted as [t0,t1] pairs matched to gaps by overlap, rewritten on save, never over a whole recording; badge − in the gap middle (or inside the adjoining bar for the first/last/folded gap), + on the seam; fold-all badge in the gutter; statuses "folded N seam(s)", "unfolded m:ss – m:ss (X s)"; ▶ playing into a fold opens it ("unfolded m:ss — ▶ ran into it"); ▶✂ never enters one; a right-drag opens the folds its travel crosses and refolds them on release.
Shift corrections: shift map base→seconds and rows map base→row in cut.json and in every undo snapshot; applied on load; sliding moves the video, every lane of the base and every further track, and the speech-gap points; not clamped; a drag to another row pins only the recording dragged, the rest keep the colouring; copies follow a moved row.
Cut lanes: {name, src, at, off, dur} windows of a file on a row of their own; built from the source's metadata or probed; name, name-2 …; killing a lane removes its pins, shift, pictures and sound and closes the row.
Zoom: a project opens at the zoom it was left at (`cut/zoom.txt`), else with the whole session fitted to the band (rewrite: the prototype started at pps 4); floor fits the filmed duration into the view minus the gutter; ceiling 240 px/s; zoomAt keeps the second under the anchor; each wheel event applies 1.25^−dy, anchored on the red line while it is on screen, else the pointer; buttons zoom about the red line ([`05-cut.md`](../05-cut.md) §1).
Scroll: adjustment upper=totalW, page=viewW, step viewW/8; Shift+wheel pans by dx·viewW/8, as a glide of a few frames, and a touchpad pans by its own pixels; revealPlayhead scrolls just enough to hold a moving line at three quarters of the view, and recentres a line that left by the left edge.
Thumbnails: cells at the row's film height × 16:9; each asks the on-the-fly picture on a 0.25 s grid doubled until a step covers the cell (three ffmpeg at a time, newest ask first, at most 96 waiting), else Prepare's frame, else the slate; decoded in a worker in batches of 6 at the row height, converted once to cairo surfaces; unreadable files cached as empty; clipped to the row and the recording's end.

## C. Cut model

cutSeg: s, e (session seconds; s==e with dur>0 = spliced insert), ins (asset path, project-relative, or "copy:SECONDS"), dur (spliced insert length), rate (written only by produceSegs), ss (start inside an inserted sound), mute (spliced: silent; overwriting: footage sound kept), cam (picture row), lane (which recording an overlaid sound replaces; "" = everything), quiet (lanes this scene does not hear), split (starts at a Split border; blocks coalesce). length() = dur if spliced, else (e−s)/rate.
cut.json: {segs, aspect, fx, sound (legacy read-only), shift, rows, lanes, nrows, folds}; beside it cut/line.json {"t"}.
Edits: rangePieces (snap both ends, one piece per filmed run, drop < 1 s, cam = selection row); addRange (steal the span off other rows, append, coalesce, persist); removeSpan (inserts dropped whole or kept whole; remainders ≥ 0.04 s); stealSpan; layOver; layOverSound (one sound per kept piece, ss walking; floor 0.05 s); addSplice; splitBorder (strictly inside a footage clip, halves ≥ 0.04; the right half carries split); mergeTouching/mergeDropped (same-camera neighbours within 0.04 s; clears split); coalesce (sort by s; merge non-insert same-camera clips touching within 0.04 s unless split, across spliced cards); splitSpliced (render view: a spliced card inside a clip cuts it in two).
Undo: snapshot {segs, fx, aspect, shift, rows, lanes, nRows}, deep copies; depth 50; pushUndo clears redo; restore re-slides sources only when corrections differ, rebuilds lanes, relayouts; statuses "undone — N segment(s) left" for both directions, "nothing to undo — you are at the state this page opened with", "nothing to redo — a new edit threw that branch away". Base set on load and after every suggestion; Revert restores it ("nothing to revert — the cut is as it was" / "reverted — N hand-made segment(s) gone, the cut is empty" / "reverted to the N segment(s) of the last suggestion (↶ Undo brings your edits back)"). Clear: one undo step, segs=fx=nil ("cleared N scene(s) and M effect(s)"; refuses "nothing to clear — the timeline holds no cut yet").
persist: syncFolds → write cut.json → totals → outputs → updateGates (the only place cut.json comes into existence) → buttons → showInsert → redraw.
Lengths: cutLen = Σ length of applyFx(splitSpliced(segs), fx); the Cut, Cut at 1× and Source readings are mm:ss.

## D. Transport

Three ▶ rule: each wears ⏸ only while its thing runs; pressing it pauses; pressing another switches over without stopping; exactly one lit. setCutOnly statuses: "preview is the cut — and the cut is empty, so ▶✂ has nothing to play until a clip is added" / "preview is the cut — the clock reads the finished video" / "preview is the recording again — everything plays, cuts and all". toggle refuses an empty cut in ✂ mode ("the cut is empty — add a clip to play it, or press ▶ to play the recording instead"), clears the watched row, snaps onto kept material, hushes before moving, ends a review on pause. stop = review off, player stop, started=false.
setPlayhead: set, re-base the live clock, hasPlay, cancel card hold, sync held effect, clock, note line, buttons, gain; if a recording covers t: rate before seek, mix on file change, hush, in-place seek (unless the spare holds the target) else PlaySegment; then showInsert, redraw.
Clock: "mm:ss.d"; "--:--.-" with no playhead; under ▶✂ the cut's own clock with tooltip "X into the cut, of Y — the finished video's own clock (the ▶✂ preview), the speed effects included. Session time here is Z." on the ▶✂ button; else the Playhead reading, counting footage as the ruler does, with no tooltip.
Tick 100 ms; liveClock extrapolates ≤ one tick at the current rate, monotone; bands repaint only when the clip under the line changes.
followPlayback order: held card → tickHold; walkOn; not playing → return; position → playhead, noteLine, held effect, clock; walkFold unless cutOnly; reviewTick; spliced card crossed → hold, else showInsert; skipGap; camera change → re-cue; preloadAhead; rate; gain; reveal; repaint.
Gap skip (cutOnly): past last clip → pause; new gap → seek to playable(next.S); same gap again → re-ask only if the player is not on the target's file; no recording → pause.
Walk-on after a file ends: another camera rolling at this second, else the next recording start; re-cue, snap, toggle.
Review: pad 10 s; window [max(S_i, E_i−10), min(E_{i+1}, S_{i+1}+10)]; stay/seek/done/lost with an armed flag (lost only once the line was seen inside the window); run-ups already behind the line are played into; starts from the red line (inside a window → play on; between → next seam; past all → first); statuses "reviewing cut N of M — 10 s before and after the join at m:ss", "reviewed all N cuts", "the line was moved — the cut review is over; ▶✂✂ starts it again", "nothing to review — a cut needs two clips to have a join between them".
Preload: 3 s ahead of the next jump (clip end → next clip; seam window close → next run-up if ahead; recording end → next recording) the spare pipeline prerolls at the target; PlaySegment swaps it in when it matches within 0.01 s; a stop drops the spare.
Frame step: held edge → nudge edge; held clip → nudge clip; held effect → nudge effect; else pause and seek by n/fps (the footage file's own rate, else 25 with a log line).
Hush/mix: mixUnder(v) = every non-master lane overlapping v with delta/lo/hi/track; hush = the scene's quiet list + whether the picture's own sound is silenced + until (the scene's end in file seconds); a hushed lane is never started (READY), the footage's own sound uses the mute property; lanes under an until are seeked with a stop at the boundary. Rate changes under playback = flushing seek throttled to one per 250 ms; gain = fxGainAt × previewVol, clamped to 10.
Remembered line: cut/line.json written at most once per second, flushed on close, restored once per project when a recording covers it.

## E. Editing flows

Left press order: gutter fold-all; selection band (fold badge, selection ✕, selection ends/middle, green-bar ✕, green-bar ends → trim, else drop selection); effects lane (✕, hold/part, else drop); selection ends on the pictures within 10 px; lane/row sound switches; speaker badge; camera badge; cut-lane ✕; empty-row ✕; trim grab on a clip border (6 px) → trimming; else drop holds and start a new selection (scope = the lane or row it was drawn on).
Drag: trim after 4 px slop; effect drag; selection move/resize; new selection. Drag end: trim drop (merge touching, persist, land on the edge, status "clip N: a – b (X s[, Y s in the video])"); effect persist/status/click opens its form; real drag (≥5 px) keeps the selection; click → clear selection and marks, set watched row when on a row, set playhead, hold the scene under the click.
Click on a picture row with no scene of that row under it: the nearest clip's edge within 12 px, else that clip. Double click or right click: hold the card under the pointer ("<card> is held — Edit now re-opens it").
Right button = moving: a lane recording; a row strip's recording; the selected scenes; a green bar end (trim) or middle (move); a clip on the green; else the whole camera row. Read in pixels over the zoom (folds have no width), snapped to marks within 8 px, row change when it fits, one undo per gesture; statuses "<what> moved +1.23 s" / "<what> is back where it started" / "<what> moved to row N — its kept scenes came along". Unmoved right press = "this one" (status only). Folds opened around the grab and refolded on release.
Trim: edges within 6 px (own side first), cards skipped; clamp: end ≥ S+1 s, ≤ next.S, ≤ recording end; start ≤ E−1 s, ≥ prev.E, ≥ recording start; live scrub throttled 90 ms; the end edge shows the frame before the boundary.
Move clip: keeps length, inside its recording, clear of neighbours, snapped flush within 8 px; inserts may go anywhere the neighbours allow.
Selection band: ends (6 px; 10 on pictures), ✕ (only when ≥50 px wide), middle; snaps to clip borders, recording ends, effect ends, playhead; min 0.04 s; marks (green in / red out flags) follow the band; readout "mm:ss.d – mm:ss.d".
Verbs and statuses: Add ("added — ↶ Undo (Ctrl+Z) takes it back", "…on <cam>…", "…and taken off the other camera…"; refusals "drag a region on a track first", "＋ Add keeps footage — the selection is <base>'s sound", "nothing to add: X s selected, a scene is 1 s or more"); Remove ("removed X s — the scene it went through is two now (↶ Undo takes it back)" / "removed X s — N scene(s), was M"; "nothing to remove: the cut keeps nothing between…"); Split ("split at m:ss[ and m:ss] — N scenes, was M"; keeps the selection); Copy ("copied m:ss – m:ss (X s) — click where it goes, then ⧉ Paste"; refuses under 1 s); Paste ("pasted X s from a at b — the cut is c, was d"; "click the timeline where the copy goes first"); Lane ("X s from a is now the <name> lane, starting at b"); pasteSound ("laid X s of <base> over the footage|N stretches of footage at m:ss"; "the cut keeps no footage at m:ss — a sound needs a picture under it"); ⌦ order: held effect, held clip (the only way to remove a card), sound selection refused, selection, scene under the line, else "nothing selected — click a kept scene, or drag a region on a track"; ✕ badges: scene ("removed — 1 segment(s), was M — ↶ Undo takes it back"), selection ("clear the selection — nothing is removed"), lane ("removed the <name> lane"), empty row ("removed the empty row N"), effect; camera badge ("the scene at m:ss is shown from <cam> now"); speaker badge ("<base> is silent in|heard in the scene at m:ss"); whole-lane switch ("<name> off for the whole cut — N scene(s) changed" / "<name> is on for the whole cut — every scene hears it (N changed)" / "<name> is in no scene yet — cut something first").
Keys: Space play/pause; `,` / `.` frame back / forward (Shift 5); Ctrl+Z undo; Ctrl+Shift+Z / Ctrl+Y redo; Delete/BackSpace ⌦; ←/→ frame step (Shift 5) only while something is held; Esc drops edge, clip, effect, selection, copy and arm.
Cursors on the band: pointer over marks (badges, switches, ✕); row-resize over the two row grips; col-resize over clip borders on the pictures or the green bar; grab over the rest of a green bar; else the page's own.

## F. Suggest (▶)

Guards: busy; hand edits → "you have hand edits — press Revert first for a fresh suggestion"; no session timeline → "run Describe first — the suggestion reads the session timeline, and there is none".
Read style: no model. marks = retakes.tsv, or remade from final.txt when it was edited later; textCut = inserts + each filmed run trimmed to its words (placed by sound within 0.4 s, never outside the run) → dropMarked → dropDeadAir; one undo, coalesce, persist, base; "cut by the words: N segments"; log ">>> cut by the words: N stretch(es) taken out, m:ss of silence, N segments, M:SS total".
Model style: up to 4 jobs: suggest (LLM "cut"), then captions, speed, decorations as the policy switches them on (see fx inventory). Target length from P.policy.targetLengthSeconds (0 = none). suggestCut: system prompt "cut"; user = context block + rows and heard lanes when several + "SESSION LENGTH: N seconds" + target block ("KEEP between A and B seconds of footage, in at most K segments.", A and B as m:ss | "NO TARGET LENGTH: keep what is worth keeping.") + dead-air line + "SESSION TIMELINE:" + session.txt. Web tools offered, dropped after the first rejection. Streaming progress counts closed segments. Thinking on; off after a call that wrote nothing. 3 attempts with retryTurn; "no valid cut after 3 attempts".
Tools add_segment, remove_segment, set_speed, cut_status, finish_cut. Faults, from the tools and finish_cut: timestamps past the end (+ mm:ss hint); fewer than min(1+target/30, 4) segments; more than max(target/5, 40); a segment ends before it starts; footage length outside target×[0.6,1.2 (≤60 s) | 0.6,1.5]×[1,4] after keepFilmed (segments with no recording at either end dropped).
Apply: undo; keep hand-placed inserts; joinSeams (close holes ≤ 1.5 s only if somebody talked in them); snap edges; dropMarked (">>> N marked stretch(es) taken out of the cut — said twice, kept once"); dropDeadAir (silences > 8 s inside a clip cut out leaving 0.5 s; ">>> m:ss of silence taken out"); coalesce; clampFxToSegs; fx replaced; persist; base; "suggested N segments".
snapEdge scoring within 5 s: kind weight − 0.18·d, outward +0.05; candidates are silence midpoint 0.8 and word edge 0.9.

## G. Inserts

Insert needs a line or a selection ("click the timeline where the insert goes first"); chooser "Insert a clip, image, animation or sound" (sound-scoped selection: "Insert a sound over the selected seconds"), starting in <project>/assets where the built-in SVG cards are written on first open. Extensions: audio mp3 wav ogg oga flac m4a aac opus; the chooser offers mp4 mkv mov webm avi svg svgz png jpg jpeg mp3 wav ogg flac m4a (a sound-scoped selection: the audio list only). insKind by extension: video (also m4v mpg mpeg ts)|svg|audio|still.
Mode: a selection gives the length and means overwrite; no selection means splice with the file's own length (video/audio duration; SVG animation length; else 4 s). mute default = file has no sound; the sound question is asked only when relevant.
Form in the column (askInsertParams): one entry per declared card field (Logo… picker for logo fields); radios "Insert BETWEEN the footage — the video gets longer by the card, nothing filmed is lost" / "Play OVER the footage — the card replaces those seconds (the same as Remove)" / "Put it on a LANE of its own — a row of the band to cut to, and nothing is cut yet" (video only); tick "Play it SILENT — the insert's own sound is not used" (spliced) or "Keep the sound running under it — only the picture is replaced" (over); Seconds entry ("at least 1.0 s — shorter keeps the length it opened with"); pinned footer "Insert it" ("Apply" on a held card) + "Cancel". Status "<file> inserted at m:ss for X s, <how> — the cut is now a (was b) — ↶ Undo takes it back".
Edit re-opens with current answers, re-finds the card by identity ("that card is no longer in the cut"); switching splice↔over returns or takes footage.
Preview of cards: 8 fps for video/animated SVG, one frame for stills; nearest rendered frame shown; 48-texture cap; rendered via ffmpeg at ≤960 px; a spliced card holds the footage while it plays on the wall clock ("<card> — the footage is held while it plays"); card sound is its own pipeline cued once.
Black frame when paused on a row with no footage.

## H. Audio

Envelopes: 200 buckets/s from an 8 kHz decode, peak bytes, cached as cache/waves/<lane>.wave (magic AWV4, header chans/hz/count/size/mtime). Dual mono collapsed when channel difference ≤ 1/100 of peak or the two envelopes match (mean diff < 1, max 8): "L=R"; "mono"; "L"/"R".
Drawing: 1 px columns from the baseline, IEC 60268-18 meter scale stretched so −48 dBFS sits on the floor; a ground shows where the recording is; paired strips dimmed.
Lanes vs strips: a row's own first track is the master strip under its pictures; further tracks and separate recordings are lanes ("<base> #N"). Selection scope is the lane/row it was drawn on; a sound selection greys Add/Split/Remove and aims Copy/Insert at sound.
Cut lanes' sound: the source's master track windowed. Audio inserts drawn violet on the sound only (hatched when spliced), footage tint kept.
Per-scene hearing: speaker badges per lane on the held/current scene; green wash where heard, grey where silenced; whole-lane switch in the gutter; legacy whole-cut sound migrated once with a log line.

## I. Implicit constants (Cut)

rulerH 18; selBandH 22; laneGap 3; snapTol 5 s; talkPad 0.2 s; minSegLn 1.0 s; minPieceLn 0.04 s; undoDeep 50; maxPps 240; edgeGrab 6 px; dragSlop 4 px; edgeMove 12 px; splicePx 22; snapPx 8; scrubEvery 90 ms; suggestChooseShare 0.7; insDefault 4 s; sndMinLn 0.05; playTick 100 ms; thumb 40..160; zoom 1.25; tick steps; pan viewW/8; paned 660/380; preview floor 160; segKill radius 4 pad 3 hit 10; killIn 16; killMin 32; mergeTol 0.04; selGripPx 6/10; selMinBand 50; selMinLen 0.04; foldMin 20; gutterPx 30; hear badge r 4.5 hit 10; waveHz 200; waveRate 8000; waveLaneH 30; waveGap 4; wavePad 3; dualMonoRatio 100; thumbBatch 6; insPreviewFPS 8; insPreviewW 960; insFilmMax 48; reviewPad 10 s; preloadLead 3 s; preloadTol 0.01; gearPx 40; lineSaveMs 1000; rateSeekGap 250 ms; suggest min/max segments; maxSpeedRate 4; window 0.6/1.2|1.5; shortTarget 60; captionBatch 5; seamMax 1.5; deadAirMax 8; deadAirKeep 0.5; attempts 3 (cut) / 2 (passes). Colours listed in the source (kept tint RGBA(0.2,0.8,0.3,0.3), insert violet, scrim, speed tint, selection blue, marks green/red, recording edges amber striped).

## J. Invariants

1. One thing held at a time; holds are indices and are dropped on renumbering; things picked from the toolbar are re-found by identity.
2. Picking up is not an edit; one undo per drag.
3. An unmoved press is a click; a right click never moves the line.
4. A border belongs to both buttons; the right button's own verb is moving.
5. A selection is of what it was drawn on.
6. Unfilmed time has no width; seams belong to the later take; preview and render agree on half-open ranges.
7. Folds are views, never edits, never used to measure drags.
8. Inserts are files: never trimmed, merged, dropped by re-suggest, or given a quiet answer; a spliced insert costs no session time and is removed only via ⌦ on a held card.
9. length()/cutLen are the only truth about video length; only produceSegs writes rate.
10. quiet lists silent lanes; compared as a set; fresh slices per toggle.
11. Only a recording dragged to another row is pinned; nRows holds an emptied bottom row.
12. A clip may not leave its recording; clips never overlap.
13. Remove takes exactly the selection (floor 0.04 s); 1 s is only for proposing/copying.
14. A Split border survives coalesce until merged by a drag.
15. Suggest replaces footage and effects, keeps inserts, sets the base.
16. Every model reply is walked back onto the timeline before the page sees it.
17. Rates take hold at a seek; hushed lanes are never started; nothing decodes inside a draw; the tracks are a window drawn under a translate.
18. cut.json existing gates Narrate and Produce; persist is its only writer.
19. Exactly one ▶ lit; the page is a snapshot rebuilt only when Prepare's output changed.

<!-- nav -->
---
<span>← start</span> · [↑ top](#inventory-cut-tab--timeline-transport-editing-suggest-inserts-audio) · [↑ Contents](../README.md) · [Inventory: effects (Cut page) and their rendering →](effects.md)
<!-- /nav -->
