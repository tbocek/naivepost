# Inventory: Cut tab — timeline, transport, editing, suggest, inserts, audio

<!-- nav -->
<span>← start</span> · [↑ Contents](../README.md) · [Inventory: effects (Cut page) and their rendering →](effects.md)
<!-- /nav -->

Raw material for the spec, read off gui/cut*.go (non-effects), player.go, player_spare.go.

## A. UI layout

Page = vertical paned (380): top = horizontal paned (660) of preview (GtkPicture contain-fit, min height 160, click toggles play) and the form column; bottom = toolbar, tracks, scrollbar.
Form column: pinned heading (title + ✕ "close this form — nothing is lost that was not already saved"), a scroller holding either the idle rows or a form body, pinned footer for buttons. Idle rows: Thumbnails [🖼− 🖼+] ("smaller|larger thumbnails on the tracks", ×3/4, ×4/3, clamp 40..160), Aspect ratio [dropdown], Playhead [clock], Selection [marks], Cut [total], Cut at 1× [totalRaw], Source [totalSrc], Segments [totalSegs].

Toolbar (wheel over it steps frames; Shift = 5):

1. linked(▶, ▶✂, ▶✂✂, ‹‹f, ‹f, f›, f››). ▶ "play or pause the preview at the playhead" (never greyed). ▶✂ (stock icon + "✂"): "pause the cut preview" / "play the CUT instead of the recording: the removed stretches are skipped, so this runs the finished video. The clock reads the cut's own time while it does. Changes nothing that is saved."; sensitive when segs > 0; lit while cutOnly and no review. ▶✂✂ (icon + "✂✂"): sensitive with ≥2 clips; "review every cut in one go: plays 10 s of the finished video before each join and 10 s after it, one join after the other, and stops after the last. The removed stretches are skipped as under ▶✂. Changes nothing that is saved." / "pause the cut review"; lit while reviewing. Frame buttons: "back 5 frames (pauses) — or whatever is held, 5 frames" etc.
2. volume control (icon + 0..100, 120 px, shared tooltip).
3. separator.
4. linked(＋ Add, | Split, － Remove, ⧉ Copy, ⧉ Paste, Insert/Edit, ⇲ Lane) with state-dependent tooltips (Add: "keep the selected region (Undo takes it back)" / "＋ Add keeps footage, and this selection is <base>'s sound" / "drag a region on a track, then ＋ Add keeps it" / "the selection is under 1 s — too short to keep as a scene"; Split: "cut the selected region free: a border at each end, nothing removed, so those seconds become a scene of their own. With nothing selected it cuts once, at the red line (Undo takes it back)"; Remove: "drop the selected region — through the middle of a scene it leaves two, one either side (Undo takes it back)"; Paste: footage/sound variants naming the seconds, "Esc drops the copy"; Insert: "put a file in the cut at the playhead — a video sting, a still, or an SVG that animates itself. A selected region gives it its length; otherwise the file's own. With the selection drawn in a lane's own wave it offers sounds instead… Right-click a card on the track to hold it, and this becomes Edit."; held card → "change the held card — what it says, and whether it plays over the footage (overwrite) or between it (insert)"; held effect → "change the held effect — <label>"; Lane: "put a copy on a row of its own — ⧉ takes one first" / long variant ending "select on the new row and press ＋ Add, the same way you would cut to a second camera").
5. Effects dropdown: ✚ Effect, ⊕ Zoom, ❝ Text, ▨ SVG, ⏩ Speed, 🔊 Volume, 🏷 Label (fires and snaps back).
6. linked(Undo "Undo — take back the last Add, Remove or Suggest (Ctrl+Z)", Redo "(Ctrl+Shift+Z)", Revert "Revert edits — drop everything you added or removed by hand and go back to the last suggestion — or, if you have not suggested yet, to the cut this page opened with", Clear "Clear: take every kept stretch and every effect off the timeline, leaving the recordings as they were loaded (↶ Undo brings them back)").
7. linked(zoom out "zoom the timeline out — it stops where the whole session is on screen (the scroll wheel does the same, around the cursor)", zoom in "…around the middle of what is on screen…"), factor 1.25.

Tracks (two drawing areas under a red-line overlay, no tooltips by design), top to bottom in the picture band: gutter (first 30 px of timeline space; fold-all badge, lane sound switches, row switches, empty-row ✕); ruler (18 px, ticks at tickStep(pps): 0.2 0.5 1 2 5 10 30 60 120 300 600 s, first with s·pps ≥ 70); selection band (22 px: green bar per kept scene, blue selection, fold −/+ badges); effects lane (26 px per row); picture rows (thumbHt+4, then that row's paired wave strip 30 px per channel, gap 3); audArea (separate recordings' lanes); scrollbar (hidden when everything fits; thumb geared at high zoom, 40 px of drag per screenful).
Red line 2 px RGB(0.9,0.15,0.15) on its own layer.
Bottom bar: inputs "2 videos · 41:12 · +1 recording · no timeline" with a tooltip listing every video and lane, line counts and the session context; "nothing to cut — no source on Inputs is marked as footage" when none. Outputs: folder "cut/ — the cut, as cut.json" + summary. Run bar ▶ = Suggest until the preview has been started, then the preview's transport until ⏹.

## B. Timeline model

Session clock from srcClock over the snapshot; tlVideo{base, path, start, wall, dur, interval, fps (default 30), frames, w, h, lane, off}; at(t)=t−start+off; sessionAt(local)=start+local−off. tlAudio{base, path, start, off, dur, chans, master, track}.
Rows = greedy interval colouring of recordings in start order, pins (cut.json rows) applied first; nRows floor keeps an emptied bottom row until its ✕. Filmed runs = merged recording spans; cells = runs cut at folded gaps; spans carry pixel origins (gutter first; a folded cell has zero width).
x↔time: xOf walks spans (folded cell → its seam x; past the end → totalW); tAt is the inverse, half-open on the right (the seam second belongs to the later take); left of the first run → its start.
Folds are a view, not an edit: persisted as [t0,t1] pairs matched to gaps by overlap, rewritten on save, never over a whole recording; badge − in the gap middle (or inside the adjoining bar for the first/last/folded gap), + on the seam; fold-all badge in the gutter; statuses "folded N seam(s)", "unfolded m:ss – m:ss (X s)"; ▶ playing into a fold opens it ("unfolded m:ss — ▶ ran into it"); ▶✂ never enters one; a right-drag opens folds within 6 px of what it holds and refolds on release.
Shift corrections: shift map base→seconds and rows map base→row in cut.json and in every undo snapshot; applied on load; sliding moves the video, every lane of the base and every further track, and the speech-gap points; clamped to ±(60 + Σ durations); rows are frozen (pinned) on the first drag because segments carry a row number; copies follow a moved row.
Cut lanes: {name, src, at, off, dur} windows of a file on a row of their own; built from the source's metadata or probed; name, name-2 …; killing a lane removes its pins, shift, pictures and sound and closes the row.
Zoom: pps starts at 4; floor fits the filmed duration into the view minus the gutter; ceiling 240 px/s; zoomAt keeps the second under the anchor; wheel deltas banked and applied once per idle as 1.25^−dy; buttons zoom about the view centre.
Scroll: adjustment upper=totalW, page=viewW, step viewW/8; Shift+wheel pans by dx·viewW/8; revealPlayhead recentres when the line leaves the view.
Thumbnails: step = max(1, thumbHt·aspect/(pps·interval)); decoded in a worker in batches of 6 at the row height, converted once to cairo surfaces, generation-checked; unreadable files cached as empty; clipped to picture width, row end and cell end.
Visual-change scores per recording in the background from 24×14 brightness postages of consecutive frames.

## C. Cut model

cutSeg: s, e (session seconds; s==e with dur>0 = spliced insert), ins (asset path, project-relative, or "copy:SECONDS"), dur (spliced insert length), rate (written only by produceSegs), ss (start inside an inserted sound), mute (spliced: silent; overwriting: footage sound kept), cam (picture row), lane (which recording an overlaid sound replaces; "" = everything), quiet (lanes this scene does not hear), split (starts at a Split border; blocks coalesce). length() = dur if spliced, else (e−s)/rate.
cut.json: {segs, aspect, fx, sound (legacy read-only), shift, rows, lanes, nrows, folds}; beside it cut/line.json {"t"}.
Edits: rangePieces (snap both ends, one piece per filmed run, drop < 1 s, cam = selection row); addRange (steal the span off other rows, append, coalesce, persist); removeSpan (inserts dropped whole or kept whole; remainders ≥ 0.04 s); stealSpan; layOver; layOverSound (one sound per kept piece, ss walking; floor 0.05 s); addSplice; splitBorder (strictly inside a footage clip, halves ≥ 0.04; the right half carries split); mergeTouching/mergeDropped (same-camera neighbours within 0.04 s; clears split); coalesce (sort by s; merge non-insert same-camera clips touching within 0.04 s unless split, across spliced cards); splitSpliced (render view: a spliced card inside a clip cuts it in two).
Undo: snapshot {segs, fx, aspect, shift, rows, lanes, nRows}, deep copies; depth 50; pushUndo clears redo; restore re-slides sources only when corrections differ, rebuilds lanes, relayouts; statuses "undone — N segment(s) left", "redone — N segment(s)", "nothing to undo/redo". Base set on load and after every suggestion; Revert restores it ("nothing to revert — the cut is as it was" / "reverted — N hand-made segment(s) gone, the cut is empty" / "reverted to the N segment(s) of the last suggestion (↶ Undo brings your edits back)"). Clear: one undo step, segs=fx=nil ("cleared N scene(s) and M effect(s)"; refuses "nothing to clear — the timeline holds no cut yet").
persist: syncFolds → write cut.json → totals → outputs → updateGates (the only place cut.json comes into existence) → buttons → showInsert → redraw.
Lengths: cutLen = Σ length of applyFx(splitSpliced(segs), fx); "%.1f s" or "%.1f s, %.1f s in the video" when they differ ≥ 0.05.

## D. Transport

Three ▶ rule: each wears ⏸ only while its thing runs; pressing it pauses; pressing another switches over without stopping; exactly one lit. setCutOnly statuses: "preview is the cut — and the cut is empty, so ▶✂ has nothing to play until a clip is added" / "preview is the cut — the clock reads the finished video" / "preview is the recording again — everything plays, cuts and all". toggle refuses an empty cut in ✂ mode ("the cut is empty — add a clip to play it, or press ▶ to play the recording instead"), clears the watched row, snaps onto kept material, hushes before moving, ends a review on pause. stop = review off, player stop, started=false.
setPlayhead: set, re-base the live clock, hasPlay, cancel card hold, sync held effect, clock, note line, buttons, gain; if a recording covers t: rate before seek, mix on file change, hush, in-place seek (unless the spare holds the target) else PlaySegment; then showInsert, redraw.
Clock: "mm:ss.d"; "--:--.-" with no playhead; under ▶✂ the cut's own clock with tooltip "X into the cut, of Y — the finished video's own clock (the ▶✂ preview), the speed effects included. Session time here is Z."; else "The red line: X s into the session — <file> at mm:ss.d, frame N — kept|cut away here" or "in the gap between recordings".
Tick 100 ms; liveClock extrapolates ≤ one tick at the current rate, monotone; bands repaint only when the clip under the line changes.
followPlayback order: held card → tickHold; walkOn; not playing → return; position → playhead, noteLine, held effect, clock; walkFold unless cutOnly; reviewTick; spliced card crossed → hold, else showInsert; skipGap; camera change → re-cue; preloadAhead; rate; gain; reveal; repaint.
Gap skip (cutOnly): past last clip → pause; new gap → seek to playable(next.S); same gap again → re-ask only if the player is not on the target's file; no recording → pause.
Walk-on after a file ends: another camera rolling at this second, else the next recording start; re-cue, snap, toggle.
Review: pad 10 s; window [max(S_i, E_i−10), min(E_{i+1}, S_{i+1}+10)]; stay/seek/done/lost with an armed flag (lost only once the line was seen inside the window); run-ups already behind the line are played into; starts from the red line (inside a window → play on; between → next seam; past all → first); statuses "reviewing cut N of M — 10 s before and after the join at m:ss", "reviewed all N cuts", "the line was moved — the cut review is over; ▶✂✂ starts it again", "nothing to review — a cut needs two clips to have a join between them".
Preload: 3 s ahead of the next jump (clip end → next clip; seam window close → next run-up if ahead; recording end → next recording) the spare pipeline prerolls at the target; PlaySegment swaps it in when it matches within 0.01 s; a stop drops the spare.
Frame step: held edge → nudge edge; held clip → nudge clip; held effect → nudge effect; else pause and seek by n/fps (default 30); "click a track first to place the playhead".
Hush/mix: mixUnder(v) = every non-master lane overlapping v with delta/lo/hi/track; hush = the scene's quiet list + whether the picture's own sound is silenced + until (the scene's end in file seconds); a hushed lane is never started (READY), the footage's own sound uses the mute property; lanes under an until are seeked with a stop at the boundary. Rate changes under playback = flushing seek throttled to one per 250 ms; gain = fxGainAt × previewVol, clamped to 10.
Remembered line: cut/line.json written at most once per second, flushed on close, restored once per project when a recording covers it.

## E. Editing flows

Left press order: gutter fold-all; selection band (fold badge, selection ✕, selection ends/middle, green-bar ✕, green-bar ends → trim, else drop selection); effects lane (✕, hold/part, else drop); selection ends on the pictures within 10 px; lane/row sound switches; speaker badge; camera badge; cut-lane ✕; empty-row ✕; trim grab on a clip border (6 px) → trimming; else drop holds and start a new selection (scope = the lane or row it was drawn on).
Drag: trim after 4 px slop; effect drag; selection move/resize; new selection. Drag end: trim drop (merge touching, persist, land on the edge, status "clip N: a – b (X s[, Y s in the video])"); effect persist/status/click opens its form; real drag (≥5 px) keeps the selection; click → clear selection and marks, set watched row when on a row, set playhead, hold the scene under the click.
Double click: pick up an edge within 12 px (held) or 6 px, or a clip.
Right button = moving: a lane recording; a row strip's recording; the selected scenes; a green bar end (trim) or middle (move); a clip on the green; else the whole camera row. Read in pixels over the zoom (folds have no width), snapped to marks within 8 px, row change when it fits, one undo per gesture; statuses "<what> moved +1.23 s" / "<what> is back where it started" / "<what> moved to row N — its kept scenes came along". Unmoved right press = "this one" (status only). Folds opened around the grab and refolded on release.
Trim: edges within 6 px (own side first), cards skipped; clamp: end ≥ S+1 s, ≤ next.S, ≤ recording end; start ≤ E−1 s, ≥ prev.E, ≥ recording start; live scrub throttled 90 ms; the end edge shows the frame before the boundary; status "clip N's start|end picked up at mm:ss.d — right-drag to trim".
Move clip: keeps length, inside its recording, clear of neighbours, snapped flush within 8 px; inserts may go anywhere the neighbours allow.
Selection band: ends (6 px; 10 on pictures), ✕ (only when ≥50 px wide), middle; snaps to clip borders, recording ends, effect ends, playhead; min 0.04 s; marks (green in / red out flags) follow the band; readout "mm:ss.d – mm:ss.d".
Verbs and statuses: Add ("added — ↶ Undo (Ctrl+Z) takes it back", "…on <cam>…", "…and taken off the other camera…"; refusals "drag a region on a track first", "＋ Add keeps footage — the selection is <base>'s sound", "nothing to add: X s selected, a scene is 1 s or more"); Remove ("removed X s — the scene it went through is two now (↶ Undo takes it back)" / "removed X s — N scene(s), was M"; "nothing to remove: the cut keeps nothing between…"); Split ("split at m:ss[ and m:ss] — N scenes, was M"; keeps the selection); Copy ("copied m:ss – m:ss (X s) — click where it goes, then ⧉ Paste"; refuses under 1 s); Paste ("pasted X s from a at b — the cut is c, was d"; "click the timeline where the copy goes first"); Lane ("X s from a is now the <name> lane, starting at b"); pasteSound ("laid X s of <base> over the footage|N stretches of footage at m:ss"; "the cut keeps no footage at m:ss — a sound needs a picture under it"); ⌦ order: held effect, held clip (the only way to remove a card), sound selection refused, selection, scene under the line, else "nothing selected — click a kept scene, or drag a region on a track"; ✕ badges: scene ("removed the scene at m:ss (X s) — ↶ Undo takes it back"), selection, lane ("removed the <name> lane"), empty row ("removed the empty row N"), effect; camera badge ("the scene at m:ss is shown from <cam> now"); speaker badge ("<base> is silent in|heard in the scene at m:ss"); whole-lane switch ("<name> off for the whole cut — N scene(s) changed" / "<name> is on for the whole cut — every scene hears it" / "<name> is in no scene yet — cut something first").
Keys: Space play/pause; Ctrl+Z undo; Ctrl+Shift+Z / Ctrl+Y redo; Delete/BackSpace ⌦; ←/→ frame step (Shift 5) only while something is held; Esc drops edge, clip, effect, selection, copy and arm.
Cursors: pointer over badges/switches/✕; ew-resize over borders and selection ends; grab over band middles/effect bodies; crosshair while an effect is armed.

## F. Suggest (▶)

Guards: busy; hand edits → "you have hand edits — press Revert first for a fresh suggestion"; no session timeline → "run Describe first — the suggestion reads the session timeline, and there is none".
Read style: no model. marks = retakes.tsv, or remade from final.txt when it was edited later; textCut = inserts + each filmed run trimmed to its words (placed by sound within 0.4 s, never outside the run) → dropMarked → dropDeadAir; one undo, coalesce, persist, base; "cut by the words: N segments"; log ">>> cut by the words: N stretch(es) taken out, m:ss of silence, N segments, M:SS total".
Gaming style: 4 queue jobs: suggest (LLM "cut"), captions, speed, decorations (see fx inventory). Target length only from the context text (regex "N min|m|s|sec"; a bare number is never a length); none → ">>> suggest: the user context names no length — everything worth keeping goes in". suggestCut: system prompt "cut"; user = context block + "SESSION LENGTH: N seconds, which the timeline writes as mm:ss. Every start and end you give is a number of SECONDS between 0 and N." + target block ("TARGET LENGTH: N seconds … KEEP between A and B seconds of footage, in at most K segments. Stop at the first set of moments that lands in that range." | "NO TARGET LENGTH. …") + "SESSION TIMELINE:" + session.txt. Web tools offered, dropped after the first rejection. Streaming progress counts closed segments. Thinking on; off after a call that wrote nothing. 3 attempts with retryTurn; "no valid cut after 3 attempts".
Reply {"segments":[{start,end[,speed|rate]}],"fx":[…]}. checkCutReply faults: timestamps past the end (+ mm:ss hint); fewer than min(1+target/30, 4) segments; more than max(target/5, 40); a segment ends before it starts; footage length outside target×[0.6,1.2 (≤60 s) | 0.6,1.5]×[1,4] after keepFilmed (segments with no recording at either end dropped).
Apply: undo; keep hand-placed inserts; joinSeams (close holes ≤ 1.5 s only if somebody talked in them); snap edges; dropMarked (">>> N marked stretch(es) taken out of the cut — said twice, kept once"); dropDeadAir (silences > 8 s inside a clip cut out leaving 0.5 s; ">>> m:ss of silence taken out"); coalesce; clampFxToSegs; fx replaced; persist; base; "suggested N segments".
snapEdge scoring within 5 s: baseline 0.35, distance penalty −0.4·d/5, outward +0.3; silence midpoint 0.8, word edge 0.9, line start/end 0.95, visual peak min(1, score/(4·mean)) only where nobody is talking (±0.2 s).

## G. Inserts

Insert needs a line or a selection ("click the timeline where the insert goes first"); chooser "Insert a clip, image, animation or sound" (sound-scoped selection: "Insert a sound over the selected seconds"), starting in <project>/assets where the built-in SVG cards are written on first open. Extensions: audio mp3 wav ogg oga flac m4a aac opus; picture mp4 mkv mov webm avi m4v png jpg jpeg webp bmp gif svg. insKind by extension: video|svg|audio|still.
Mode: a selection gives the length and means overwrite; no selection means splice with the file's own length (video/audio duration; SVG animation length; else 4 s). mute default = file has no sound; the sound question is asked only when relevant.
Form in the column (askInsertParams): one entry per declared card field (Logo… picker for logo fields); radios "Insert BETWEEN the footage — the video gets longer by the card, nothing filmed is lost" / "Play OVER the footage — the card replaces those seconds (the same as Remove)" / "Put it on a LANE of its own — a row of the band to cut to, and nothing is cut yet" (video only); tick "Play it SILENT — the insert's own sound is not used" (spliced) or "Keep the sound running under it — only the picture is replaced" (over); Seconds entry ("how long the card runs…"); Cancel + Insert/Save. Status "<file> inserted at m:ss for X s, <how> — the cut is now a (was b) — ↶ Undo takes it back".
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
11. Rows are pinned once anything is dragged; nRows holds an emptied bottom row.
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
