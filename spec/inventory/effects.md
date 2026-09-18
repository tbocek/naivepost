# Inventory: effects (Cut page) and their rendering

<!-- nav -->
[← Inventory: Cut tab — timeline, transport, editing, suggest, inserts, audio](cut.md) · [↑ Contents](../README.md) · [Inventory: Narrate tab (extracted from the prototype, gui/narrate*.go) →](narrate.md)
<!-- /nav -->

Raw material for the spec, read off gui/cut_fx*.go, fxsvg.go, fxtext.go, svg*.go, produce_fx.go, produce_snd.go, produce_stamp.go and the effect passes of cut_suggest.go.

## A. Effect kinds

### A.0 The struct cutFx (all fields omitempty except kind/t)

| json | type | meaning |
|---|---|---|
| kind | string | zoom, speed, text, svg, volume, label (legacy "view" migrated to zoom) |
| t | float | session second it starts |
| trans | float | fade-in seconds, inside dur |
| tout | float | fade-out seconds, inside dur |
| ease | string | fade shape by name; "" = linear (only one implemented) |
| dur | float | total length incl. fades = the bar on the lane |
| stay | bool | zoom only: false = pull back, true = reframing that holds until the next zoom |
| cx, cy | float | zoom: centre as fraction of the SOURCE frame; text/svg: box centre as fraction of the OUTPUT frame |
| hf | float | zoom: rect height / source height; text/svg: box height / output height |
| wf | float | text/svg only: box width / output width (a camera window has no stored width: width = hf·srcH·outAspect) |
| rate | float | speed: 1 own clock, 0.5 half, 8 eightfold; 0 = stop/freeze |
| snd | string | speed: "", pitch, own, scene, mute |
| mute | bool | legacy; migrated to snd=mute |
| gain | float | volume: linear gain, 1 as recorded, 0 silence (0 is real) |
| text | string | text: the words (newlines break lines); label: the name |
| src | string | svg: absolute path |

Span = [t, t+max(dur,0)] for every kind. frozen = speed with rate ≤ 0. Migration: mute → snd=mute; view → zoom with dur = trans+dur+tout, stay = (tout ≤ 0).

### A.1 Zoom (camera)

Rect normalised against the source; may exceed the frame (render pads black); hf may exceed 1. Camera path: start from the centred full-fill slice; walk zooms by t; each glides from where the camera is at its t to its rect over trans, holds, and (unless stay) glides back to the settled rect over tout; a staying zoom becomes the new settled rect. Nothing reaches backwards. Fade-in wins any overlap. Clamp hf ∈ [0.02, 12], cx, cy ∈ [−2, 3].
Creation: dropdown ⊕ Zoom → arm (refuses "click a track first — the effect needs a moment to happen at"; same kind twice disarms). Arm words: "Drag a box on the video: the picture zooms there and comes back out on its own, or stays on it — the form that opens is where that is said. The box keeps the cut's shape; let go near the full width or height to snap to it. " + "It starts at the red line and runs 3 s; the form that opens says how long." | "It covers the marked stretch — a – b, X s — which the form that opens can change." Column panel with Cancel. Drag < 12 px is ignored. Defaults trans=tout=1, dur=3 (or the marked stretch), stay = an aspect is set and no staying zoom exists yet (then fades 0). Status "<label> — ↶ Undo takes it back" / "… — the video shows this region from here on; ↶ Undo takes it back". Choosing a non-source aspect with no staying zoom appends a staying zoom at 0 for 1 s holding the whole frame.
Form "Zoom at m:ss": Length (s) "how long the camera move lasts altogether, fades included"; At the end: Pull back ("A passing close-up: the picture closes in, holds for its seconds and opens back out on its own, leaving the rest of the video framed as it was.") / Stay on it ("A reframing: from here on the finished video shows this region. This is how a vertical short is made out of widescreen footage — say where the action is, and say it again when it moves."); Fade in (s) "how long the camera takes to arrive: 0 cuts straight to the region, 1 glides over a second"; Fade out (s) "how long it takes to come back off the region again: 0 cuts straight back" (greyed when staying: "A camera that stays has no way back, so no fade out."); Curve "the shape both fades travel in. Straight is all there is so far…". Apply: dur ≥ 0.4, clampFades.
Lane: band colour (0.25,0.72,0.82), stay (0.95,0.62,0.15); envelope from the glides; plate when > 40 px; mark zoom/stay; label "X.Xs". Preview overlay: outside the camera rect dimmed black 0.45; rect stroked white 1.5 px; plate with the aspect or the held label.

### A.2 Speed and stop

rate > 0 = a clock; 0 = a stop (still on the frame at t while footage runs underneath; the cut keeps its length). clampSpeed(rate, dur): dur ≥ 0.5; rate ∈ [0.05, 100]; if dur/rate < 0.5 the rate gives way. Ramps: least ramp = 0.6·√max(1,rate); shorter → 0; in+out > dur scaled down; if the flat middle would render < 0.5 s the ramps fill the band. Stairs: geometric, each stair ≥ 0.6 s on screen; a staircase that cannot be built whole is not built (plain change instead).
Overlap: rateSpans cuts at every boundary and takes the arithmetic mean of covering rates (a stop contributes 0), merges equal neighbours, heals spans that would render < 0.5 s into the longer neighbour. Footage under a still runs at 1×. frozenSpans = sub-stretches of a stop where the mean is 0.
Creation: ⏩ Speed: marked band → t, dur, rate 0.5; nothing marked and no line → "click a track or mark a stretch first — speed needs seconds to work on"; nothing marked → a stop at the line for 2 s with 0.5 s fades. Status "… — the footage plays at that rate there and the cut gets longer or shorter to match; ↶ Undo takes it back" / "… — the picture stands still there while the clock runs; ↶ Undo takes it back".
Form "Speed a – b": Speed × (dropdown ×0 — stop, ×0.25, ×0.5, ×0.75, ×1 — as filmed, ×1.5, ×2, ×4, ×8, ×20, ×100, Custom… with an entry; tooltip about 1 own speed, below slowed, above fast, 0 stops, Custom any rate); Sound (A.3); Length (s); Fade in (s) ("…A ramp needs about 0.6s of footage for every × of the rate — 2.4s at ×4 — …under that it is treated as 0."); Fade out (s); Curve; a cost note ("A stop's footage runs on at 1× under the held frame, so every answer but Silent sounds the same here." / "N s on screen: the sound ends N s behind the picture, and going back in sync skips those seconds." / "…runs N s ahead…plays those seconds again."). Apply: rate 0 → stop with dur ≥ 0.5 (no clampSpeed); else clampSpeed; clampFades.
Lane: fill RGBA(0.92,0.42,0.6,0.4); envelope = the stairs the render builds (or the glides for a stop); sound tail dashed to the sync point with "sound Ns behind|ahead" plate when ≥ 60 px; mark speed/stop/hush; label "×N" + sound suffix or "X.Xs".
Preview: runs at the first covering non-frozen effect's rate, flat (a rate change is a flushing seek); a stop reads as 1 with the still overlaid.

### A.3 Sound over a speed effect

| snd | dropdown | lane suffix |
|---|---|---|
| "" | With the picture | |
| pitch | With the picture, pitched | pitched |
| own | 1× to the effect's end | · sound 1× |
| scene | 1× to the scene's end | · sound 1× to the scene's end |
| mute | Silent | silent |

The earliest covering effect's answer wins. Debt = dur − Σ on-screen seconds (positive: sound behind). A scene tail is drawn only when |debt| ≥ 0.05 and the scene runs on. sndDip 0.15 s fades at the rejoin. The two 1× answers cannot be previewed; mute mutes the preview.

### A.4 Text (caption)

Box = fraction of the OUTPUT frame, composited after the camera. Default lower third {0.5, 0.78, 0.8, 0.16}; clamp wf ∈ [0.04,1], hf ∈ [0.03,1], centre inside. fitText: binary search (40 iterations) for the largest size with ≤ 12 lines and lines·size·1.25 ≤ boxH; char advance 0.58 em; min 7 pt; explicit newlines break; long words hard-split. Fades linear; textAlpha is also the volume envelope and the still opacity.
Creation: ❝ Text → arm ("Drag the box the words go in — anywhere on the picture, any shape. A click puts one across the lower third. " + when); drag or click → t/dur from the line or band (3 s), fades 0.3; empty text not placed ("type the words and they go on the picture — the form applies as you type it"); placed "… — the words are on the picture for those seconds; ↶ Undo takes it back". The camera layer stays up while a text is armed.
Form "Text at m:ss": a 3-line text view ("what is written over the picture. The words are fitted to the box you drew — a longer line comes out smaller, and Enter starts a new line"); Length (s); Fade in/out (s) ("0 cuts them straight on|off"); Curve. dur ≥ 0.3.
Lane: colour (0.6,0.55,0.95); mark text; label = the words truncated. Preview: bold sans-serif, white, dark dilated edge (16 directions, radius 0.08·size, alpha 0.85).

### A.5 SVG (drawing)

Same as text with ink from a file; default box centred {0.5,0.5,0.6,0.6}. ▨ SVG asks for the file FIRST ("Choose a drawing to lay over the video", assets folder, filter svg) then arms ("Drag the box <file> goes in — anywhere on the picture, any shape; the drawing keeps its own shape inside it. A click puts one across the middle."). Form "SVG at m:ss": file name + Choose…, Length, fades, Curve. Preview raster via ffmpeg at 512 px rgba, cached per file; failure logged once. Lane colour (0.4,0.8,0.5).

### A.6 Volume

No box, no drag. Max gain 10 (playbin's ceiling). fxGainAt multiplies 1 + (gain−1)·alpha over every volume effect. 🔊 Volume: marked band or line..+2 s ("click a track or mark a stretch first — volume needs seconds to work on"); defaults gain 2, fades 0.25 ("a gain that arrives on one sample is a click"); status "… — the picture is untouched".
Form "Volume a – b": Volume % ("100 is untouched, 50 half as loud, 0 silent, and up to 1000…"); Length (s); Fade in ("0 is the hard step, which on a big change is audible as a click"); Fade out; Curve. dur ≥ 0.1. Lane colour (0.95,0.85,0.2); envelope = the fades; label the percentage. Preview gain applies even paused.

### A.7 Label

Changes nothing; read by the narration brief as MARKED. 🏷 Label: band or line..+2 s ("click a track or mark a stretch first — a label names a moment, so it needs one"); empty name not placed ("type a name and it is marked — nothing is placed until then"); placed "… — nothing changes in the video; the narration is told about it". Form "Label at m:ss": Name (10 chars; tooltip: "what you call this moment -- \"the reveal\", \"boss fight\". It changes nothing in the video: it is written into the brief the narration writer is given…"), Length (s) (the stretch a clip must overlap). dur ≥ 0.4. Lane: grey-white tag (fill 0.22, tick, dashed line).

### A.8 fxLabel sentences

"zoom at m:ss for X.Xs (N.Ns in, N.Ns out | stays)"; "stop at m:ss for X.Xs (… silent)"; "m:ss slowed|sped up ×N for X.Xs (in, out)<sound suffix>"; "text “…” at m:ss for X.Xs"; "svg <file> at m:ss for X.Xs"; "label “…” at m:ss"; "m:ss louder|quieter|silent N% at … for X.Xs".

### A.9 Holding, moving, resizing, deleting

Rows: first-fit colouring in seconds (span floor 0.4 s); lane height = rows × 26, min one row; lane at y 40 under the selection band. Hit: narrowest band in the row under x; hover ring white 0.4; held ring white 0.9; end grips only when ≥ 30 px wide (6 px reach); ✕ in the middle only when ≥ 32 px. Press: ✕ → kill; band → hold (puts the playhead on t, status "<label> picked up", Insert becomes ✎ Edit); empty lane → drop. Drag under 4 px is a click; a click on a band opens its form (on idle). moveFxTo clamps the whole band inside [0, session end]; resize floors 0.1 s; snapping to segment ends and other effects' ends within 8 px (both ends offered when sliding; the held effect's own ends excluded); nudging by frames is unsnapped. The hold drops when the line walks > 1/24 s outside the band. Kill: "removed <label> — ↶ Undo takes it back"; ⌦ with an effect held removes it; Esc drops everything and disarms ("cancelled" for a bare disarm). Edit re-finds by kind + |Δt| < 0.001 ("that effect is no longer in the cut — nothing was changed") and always takes cx/cy/wf/hf from the live effect. Forms are live: every keystroke lands after a debounce; note "Kept as you type — ↶ Undo takes the whole edit back."; the first answer pushes undo; refusals reset that.

## B. The preview screen (shared by Cut and Narrate)

Layers: player picture; a zoom layer (second picture bound to the same paintable, transformed so the camera window fills the output box); a still layer for a stop on the same transform; a drawing area for the black mask and overlays. Live clock: extrapolated ≤ one tick, monotone, re-based on seeks; camera fitted from the frame clock while playing. Camera layer shown only with a player, no card, a non-source aspect or any zoom, and the page allowing it (Cut: line placed, nothing armed except text/svg, nothing held). Freeze still: rendered from the scene's own camera via ffmpeg, opacity = textAlpha, fitted on the live transform; failure logged once. Mask: black everywhere the finished frame is not. Paused: camera rect + dim + plate; overlays at real alpha, the held one at 1 with a dashed violet outline; boxes being drawn dashed. Gestures: drag kinds draw/move/size; text move snaps to frame thirds (10 px); resize independent axes (16 px floor); camera move snaps edges; camera resize drags height, width follows the aspect (12 px floor); an unmoved press toggles play/pause; no right button on the picture. Aspects: source, 9:16, 1:1, 4:5, 16:9 (dropdown tooltip: "the shape of the finished video — source is the footage's own, 9:16 is a vertical short…"); statuses "aspect: the source's own — …", "aspect X — a ⊕ zoom at 0:00 holds the whole frame, centred", "aspect X — the zooms on the lane decide the framing".

## C. SVG cards (inserts)

Insert path may carry ?key=value&… (order matters; escapes only % & = ? → %25 %26 %3D %3F). {{name}} / {{name|fallback}} holes, outside comments only. Declared inputs: `<!-- Input: key[flags] | Label | hint -->`, flags keep and logo. Two built-ins: tier (a board: six rows S A B C D F, colours #ff7f7f #ffbf7f #ffdf7f #ffff7f #bfff7f #7fffff, six places per row, items "Name|logo.png", a "new" list of arrivals with per-item timing) and badge (one big letter with a caption). Seeds written to assets/: s a b c d f .svg (badges) then tier.svg, plus CARDS.md guide; never overwriting. Canvas 1920×1080, bg #14171c, fg #e6e9ef, ink #16181d, font DejaVu Sans…, 2.2 s hold at the end. Animation: every animation starts at 0 and waits inside keyTimes so the static file is the finished card; baked to a PNG/SVG frame sequence at 25 fps (or the render fps) by evaluating SMIL (animate/set/animateTransform on numbers, lists, lengths, colours; values/keyTimes; repeat; freeze; additive) and a CSS subset (opacity, transform, fill; simple selectors; animation longhands; easings). Card length = last moving moment.

## D. LLM passes (after the cut, in this order)

Captions (prompt "captions"): batches of 5 clips, progress 0.7–0.9 "captions, clips a–b of n"; user = context + "THE CLIPS, AND WHAT WAS SAID OVER EACH:" + briefs "CLIP n: X s long"; reply {"clips":[{"i","fx":[{start,end,text}]}]}; wrong clip number rejects the batch; entries < 0.3 s dropped; fade min(0.3, d/4); 2 tries; a failed batch is skipped ("!!! captions: clips a–b skipped -- the cut stands without them"). Prompt rules: captions only if the context asks; clean like a subtitler, keep swearing, never paraphrase.
Speed (prompt "speed"): one request; briefs note captions ("…, N caption(s) -- runs at 1") and "FOOTAGE: N seconds over K clips."; reply {"speeds":[{clip,rate}]}; rate ≤ 0 or ≈1 skipped; rate > 1 on a captioned clip dropped; joinSpeeds merges same-rate stretches nearer than 4 s; log ">>> speed: N clip(s) run fast — a of video from b of footage"; no length gate.
Decorations (prompt "effects"): one request; briefs note "plays at Nx"; reply {"fx":[{clip,kind zoom|stop|volume,start,end,gain}]}; other kinds dropped; ">>> effects: N decoration(s)"; prompt: few and deliberate (three or four per five minutes), zoom 2–4 s, one stop per video, volume for level/ducking/muting; never zoom past captions.
fxFrom defaults: zoom cx=cy=0.5 hf=0.6 glide min(1,d/3); speed rate default 0.5, clampSpeed, ramp min(1,d/4) zeroed under 0.6·max(1,rate); stop d default 2, fade min(0.3,d/4); volume gain nil/1 dropped (0 kept; Gain is a pointer), ramp min(1,d/4); text empty dropped, d default 3. Cap 1000 (none in practice).
clampFxToSegs (last, against the cut as applied): point effects need a footage clip; bands trimmed to the most-overlapping footage clip, dropped under 1 s; speed re-clamped unless frozen; fades trimmed proportionally; ">>> N effect(s) pointed at footage the final cut does not keep — dropped". Suggested effects replace the list; one undo with the segments.

## E. Render

Speed is the only effect that touches the segment list (applyFx splits clips at rate boundaries and writes rate; inserts and spliced cards keep their clock; a freeze is an overlay). Everything else is a per-clip cue mapped through (sessS, span, rate, length); span = 0 for freezes/inserts (held moment: cue covers the whole clip with no fades); cues shorter than 0.04 s dropped; fin+fout clipped to the cue.
Frame: no aspect → the footage's own box; with an aspect the tier names the short side (1080p on 9:16 = 1080×1920), even sides; every clip gets the box.
Camera: samples at clip ends and every zoom's t, t+in, (unless stay) t+dur−out, t+dur, mapped to clip time, deduped; between samples straight lines; static → crop+scale; moving → pad black as needed, crop to the union box, zoompan with piecewise-linear expressions on in_time at a fixed fps (default 30); zoom > 10× logged and capped.
Filter order per clip: setpts (speed) → freeze trim/tpad → fps → stop stills overlaid (trim, tpad, rgba, optional transparent pad, alpha fades, enable between) → backdrop (blur or black) → camera crop/zoompan or fit/scale → setsar → burned subtitles → text/svg overlays last (each -loop input, format rgba, svg scaled into its box, alpha fades, overlay enable between). Title SVG written per cue (transparent frame-sized document, two passes: black stroke 2×radius round-join at 0.85, then white fill), svg effects use the user's file.
Stills: per frozen span, fades only at the effect's own ends; frame from the scene's own camera; missing recording → "a stop at N s falls in no recording — its still is skipped".
Audio: with picture → atempo chain (halvings/doublings); pitched → asetrate+aresample; own-clock runs (own/scene) are a plan: one read head opens in sync at the effect and advances by screen time; runs close at cards/freezes/other recordings; 0.15 s fades at run ends; silent → volume=0 enable expression combining stop mutes and mute bands; gains → one volume=… eval=frame per cue, after the lane bed mix and before the narration.
Render stamp: sha1 of {settings without OutFile, segs, lines (S,E,text, wav size@mtime), sources (path size@mtime), aspect, voice, narrOff} beside the video as <stem>.stamp; not covering title/description/thumbnail; effects only via aspect and rates.

## F. Implicit constants

fxMaxRate 100; fxMinRate 0.05; fxMinPlay = minClipLn 0.5; rampStep 0.6; fxRates list; speedGapMin 4; sndDip 0.15; fxMaxGain 10; aspectStayLn 1; fxMinDur 0.1; fxMinSel 0.2; fxPackMin 0.4; fxHoldSlack 1/24; default durations 3 (zoom/text/svg), 2 (label, unmarked speed/volume); floors 0.4 zoom/label, 0.3 text/svg, 0.1 volume, 0.5 stop; fades 0.3 text/svg, 0.25 volume, 0.5 stop, 1 zoom; gain 2; rate 0.5; fxLaneH 26; fxMinBand 30; killMin 32; fxGrab 9; fxSnapPx 10; camera resize floor 12, box 16; tiny drag 12; textAdvance 0.58; textLine 1.25; textAscent 0.95; textMinPt 7; textMaxLines 12; fxEdgeR 0.08; fxEdgeA 0.85; fxEdgeSteps 16; svgPreviewPx 512; svgFPS 25; card constants; captionBatch 5; caption min 0.3 s; clamp min 1 s; retries 2; progress 0.7–0.9/0.85/0.93.

## G. Invariants

One list, one owner (the cut editor); nothing reaches backwards; dur is the bar for every kind; one fade rule (clampFades, proportional trimming); rates average, gains multiply, sound answers do not merge; a stop is not a rate; footage under a still runs at 1×; staircases are built whole or not at all; slivers are healed into neighbours; only speed touches segments; camera windows have no width, overlay boxes do; the box belongs to the picture not the form; forms find effects by value; one undo per visit/drag/hold; one thing held; a hold keeps the line on its effect; empty effects are never placed; bands always have width; ends are handles only when wide; a click is not a drag; framing is done paused; hit order box → camera → texts → framing zoom; snapping is pixel-constant, nudging unsnapped; effects cannot leave the timeline; the lane draws what the render does; preview/render divergences are deliberate (flat rate, live clock, 1× sound not previewable); migration is lossless; gain 0 means silence; aspect and framing are one press; static card files are finished pictures; a path with ? is not a file; suggested effects are clamped to the cut as applied.

<!-- nav -->
---
[← Inventory: Cut tab — timeline, transport, editing, suggest, inserts, audio](cut.md) · [↑ top](#inventory-effects-cut-page-and-their-rendering) · [↑ Contents](../README.md) · [Inventory: Narrate tab (extracted from the prototype, gui/narrate*.go) →](narrate.md)
<!-- /nav -->
