# 06 — Effects

Zoom, speed (including stop), text, SVG drawing, volume and label. Each is one record in `cut.json`'s `fx` list, drawn as a bar on the effects lane, placed by hand on the preview or the lane, or proposed by the model after the cut. The preview and the render share the functions that turn an effect into a camera path, a fitted title, a fade or a gain.

## 1. Record

| field | zoom | speed/stop | text | svg | volume | label |
|---|---|---|---|---|---|---|
| t, dur | session second; total length incl. fades (the bar) | ✓ | ✓ | ✓ | ✓ | ✓ |
| trans, tout, ease | glide in/out | ramp in/out (or the still's fades) | fade in/out | fade in/out | ramp in/out | – |
| cx, cy, hf | rect centre + height as fractions of the SOURCE frame (width follows the aspect) | – | box centre + height as fractions of the OUTPUT frame | same | – | – |
| wf | – | – | box width / output width | same | – | – |
| stay | pull back (false) or reframe until the next zoom (true) | – | – | – | – | – |
| rate, snd | – | rate (1 own clock; 0 = stop); sound answer "" / pitch / own / scene / mute | – | – | – | – |
| gain | – | – | – | – | linear gain, 0 = silence | – |
| text / src | – | – | the words | the file | – | the name |

Legacy: `view` → zoom; `mute` → `snd: "mute"`. `ease` stores "" for linear so old files stay byte-identical. A text or svg record with no box reads as its kind's default box (text: the lower third; svg: the middle) — captions the model proposes carry no box and land in the lower third.

## 2. The lane and the preview

```
 effects lane   ⊕ 3.0s ████████╲     ❝ “Welcome to…” ▓▓▓▓▓▓   ⏩ ×4 · sound 1× ▒▒▒▒▒▒▒▒╌╌╌╌|   🏷 “boss fight” ┆╌╌╌╌
                (envelope = the glides/ramps/fades the render will make; ✕ in the middle when ≥ 32 px; grips at the ends when ≥ 30 px)
```

Colours: zoom (0.25,0.72,0.82), staying zoom (0.95,0.62,0.15), speed/stop (0.92,0.42,0.6), text (0.6,0.55,0.95), svg (0.4,0.8,0.5), volume (0.95,0.85,0.2), label grey-white. Rows: first-fit by seconds (span floor 0.4 s); the lane is one row deep even when empty.

Preview (paused): everything outside the camera rect dimmed black 0.45, the rect stroked white with a plate; visible overlays at their real alpha, the held one at full with a dashed violet outline; a box being drawn dashed. Playing: only the black mask over what the finished frame does not show, and the titles; the camera layer runs on the smoothed live clock. A stop's still is rendered from the scene's own camera and fitted on the same transform as the footage.

Preview vs render, deliberately different: the preview runs a speed effect at one flat rate (a rate change is a flushing seek); the render follows the ramps and averages overlaps; the two 1× sound answers cannot be previewed (the lane's dashed tail shows the debt).

## 3. Flows

### F3.1 Zoom by hand
S1 ✚ Effect ▾ → ⊕ Zoom arms a drag (refuses without a line: "click a track first — the effect needs a moment to happen at"; the same entry again disarms). Status/panel text: "Drag a box on the video: the picture zooms there and comes back out on its own, or stays on it — the form that opens is where that is said. The box keeps the cut's shape; let go near the full width or height to snap to it. It starts at the red line and runs 3 s…" (or "It covers the marked stretch — a – b, X s"). The camera layer goes down so the whole source is visible. S2 Drag a box on the preview (< 12 px is ignored; the smallest output-shaped window containing the box is taken; near-full width/height snaps). S3 Defaults: glide 1 s in and out, length 3 s (or the marked stretch); `stay` when an aspect is set and no staying zoom exists yet (then no glides). S4 Form "Zoom at m:ss": Length (s); At the end: Pull back / Stay on it; Fade in (s); Fade out (s) (greyed when staying: "A camera that stays has no way back, so no fade out."); Curve (Linear). Live; length ≥ 0.4. S5 Status "<label> — ↶ Undo takes it back" (or "… — the video shows this region from here on…").
Camera path: from the centred full-fill slice; each zoom glides from where the camera is at its start to its rect, holds, and (unless staying) glides back; a staying zoom becomes the new settled frame; nothing reaches backwards; fade-in wins overlaps. Rect clamp hf ∈ [0.02, 12], centre ∈ [−2, 3].

### F3.2 Aspect ratio
Dropdown source / 9:16 / 1:1 / 4:5 / 16:9 in the form column ("the shape of the finished video — source is the footage's own, 9:16 is a vertical short. The whole frame fits inside it (bars either side) until ▭ View frames a region; the outline on the preview is what the finished video shows"). Choosing a non-source aspect with no staying zoom appends a staying zoom at 0:00 holding the whole frame for 1 s, in the same Undo step ("aspect 9:16 — a ⊕ zoom at 0:00 holds the whole frame, centred").

### F3.3 Speed and stop by hand
S1 ⏩ Speed: with a selection → t/dur from it, rate 0.5; with only a line → a stop at the line for 2 s with 0.5 s fades; neither → "click a track or mark a stretch first — speed needs seconds to work on". S2 Form "Speed a – b": Speed × (×0 — stop, ×0.25, ×0.5, ×0.75, ×1 — as filmed, ×1.5, ×2, ×4, ×8, ×20, ×100, Custom…); Sound (With the picture / With the picture, pitched / 1× to the effect's end / 1× to the scene's end / Silent); Length (s); Fade in (s) ("…A ramp needs about 0.6s of footage for every × of the rate…"); Fade out (s); Curve; a cost note ("N s on screen: the sound ends N s behind the picture, and going back in sync skips those seconds."). S3 Rate 0 → a stop with length ≥ 0.5 and no clamp; else clampSpeed: rate ∈ [P.policy.minRate 0.05, P.policy.maxRate 100] and on-screen length ≥ P.policy.minClipSeconds (0.5), the rate giving way. S4 Status "… — the footage plays at that rate there and the cut gets longer or shorter to match; ↶ Undo takes it back" / "… — the picture stands still there while the clock runs…".
Arithmetic: overlapping rates average per span; spans that would render under 0.5 s are healed into the longer neighbour; ramps are geometric staircases with each stair ≥ P.policy.rampStepSeconds (0.6) on screen, built whole or not at all; footage under a still runs at 1×.

### F3.4 Text (caption) by hand
S1 ❝ Text arms: "Drag the box the words go in — anywhere on the picture, any shape. A click puts one across the lower third." The camera layer stays up (the box is on the output frame). S2 Drag or click (default box: the lower third {0.5, 0.78, 0.8, 0.16}); t/dur from the line (3 s) or the selection; fades 0.3. S3 Form "Text at m:ss": the words (3-line box; Enter starts a new line), Length, Fade in, Fade out, Curve; length ≥ 0.3. Empty words are not placed ("type the words and they go on the picture — the form applies as you type it"). S4 Fitting: the largest size where ≤ 12 lines of 0.58 em per character fit the box (min 7 pt); bold sans-serif, white with a dark dilated edge; the same function for preview and render. S5 On the preview a box moves (snapping to the frame's thirds within 10 px) and resizes (independent axes, 16 px floor); a press without travel toggles play/pause.

### F3.5 SVG drawing by hand
▨ SVG asks for the file first ("Choose a drawing to lay over the video", from `assets/`), then arms ("Drag the box <file> goes in — … the drawing keeps its own shape inside it. A click puts one across the middle."). Default box the middle {0.5, 0.5, 0.6, 0.6}. Form "SVG at m:ss": file + Choose…, Length, fades, Curve. Preview raster via ffmpeg at 512 px with transparency, cached per file. An svg with no file is not placed.

### F3.6 Volume by hand
🔊 Volume: selection or line..+2 s ("click a track or mark a stretch first — volume needs seconds to work on"); defaults gain 200 %, ramps 0.25 s. Form "Volume a – b": Volume % (0..1000; "…up to 1000 for a passage recorded too quietly to hear — though a passage lifted that far brings its hiss up with it"), Length (≥ 0.1), Fade in ("0 is the hard step, which on a big change is audible as a click"), Fade out, Curve. Overlapping gains multiply; the preview applies the gain even while paused. Status "… — the picture is untouched".

### F3.7 Label by hand
🏷 Label: selection or line..+2 s; Form "Label at m:ss": Name ("what you call this moment -- "the reveal", "boss fight". It changes nothing in the video: it is written into the brief the narration writer is given…"), Length (the stretch a clip must overlap). Empty name not placed. Drawn as a tag; never rendered; the narration brief lists it as MARKED.

### F3.8 Hold, move, resize, edit, remove
S1 A press on a band picks it up (the line moves to its start; status "<label> picked up"; Insert becomes ✎ Edit); ends are grips when the band is ≥ 30 px; a press under 4 px of travel is a click and opens the form. S2 Drag moves the whole band (clamped inside the timeline; both ends offered to snap marks: segment ends and other effects' ends, 8 px) or an end (length ≥ 0.1 s). Frame steps and ←/→ nudge a held effect unsnapped. The hold drops when the line walks more than 1/24 s off the band. S3 The ✕ in the band's middle (≥ 32 px) or ⌦ removes it ("removed <label> — ↶ Undo takes it back"). Esc drops the hold and disarms. S4 Edit re-finds the effect by kind and start (|Δt| < 1 ms; "that effect is no longer in the cut — nothing was changed") and always takes the box from the live effect, never from the form's snapshot. S5 Forms are live: the first answer pushes Undo; every keystroke lands after a debounce; "Kept as you type — ↶ Undo takes the whole edit back."

### F3.9 Captions proposed by the model (after the cut)
Batches of P.policy.captionBatch (5) clips; message = User Context + "THE CLIPS, AND WHAT WAS SAID OVER EACH:" + "CLIP n: X s long" + the lines as offsets. **Tools**: `add_caption(clip, start, end, text)`, `finish` (`02-services.md` §3.7). Validation in the tool: the clip must be one given; end−start ≥ P.policy.captionMinSeconds (0.3). Prompt rules: captions only if the context asks; clean like a subtitler (no ehm, no stutters, sentence case, swearing kept, never a paraphrase); never caption an aside to the editor. Fades min(0.3, d/4). A failed batch is skipped ("!!! captions: clips a–b skipped -- the cut stands without them").

### F3.10 Speeds proposed by the model
One call; briefs note captions ("…, N caption(s) -- runs at 1") and the footage total. **Tools**: `set_clip_speed(clip, rate)` (rate > 1 on a captioned clip refused; rate ≈ 1 ignored), `finish`. Prompt rules: the longer the dull stretch the higher the rate; a clip with speech plays at 1 unless the context asks for more; slow motion only on a short clip the video is about; one rate per clip. Same-rate stretches nearer than P.policy.speedGapSeconds (4) are merged. Log ">>> speed: N clip(s) run fast — a of video from b of footage". No length gate.

### F3.11 Decorations proposed by the model
One call over every clip, told the captions and speeds. **Tools**: `add_effect(clip, kind zoom|stop|volume, start, end, gain?)`, `finish`. Prompt: few and deliberate (about three or four per five minutes unless the context says otherwise); zoom 2–4 s; one stop per video; volume for level, ducking or muting; never zoom past captions. Defaults the app applies: zoom centre 60 % height with glides min(1, d/3); stop default 2 s with fades min(0.3, d/4); volume ramps min(1, d/4); a gain of 1 or none ignored, 0 kept.

### F3.12 Clamp to the cut as applied
After snapping, dead-air and mark removal and coalescing: a point effect needs a footage clip under it; a band is trimmed to the clip it overlaps most and dropped under P.policy.effectMinSurvivingSeconds (1.0); a speed is re-clamped unless it is a stop; fades shrink proportionally; ">>> N effect(s) pointed at footage the final cut does not keep — dropped". Suggested effects replace the list, in the same Undo step as the segments.

## 4. Render (how each effect becomes ffmpeg)
- **Speed** is the only effect that changes the clip list: clips are split at rate boundaries and the middle carries the rate (`setpts=PTS/rate`; sound by an atempo chain, or asetrate when pitched, or a mute expression when silent, or a planned 1× read head with 0.15 s dips at run ends for the own/scene answers). Inserts and spliced cards keep their own clock.
- **Stop**: a still of the frame at t (from the scene's own camera) overlaid with alpha fades for the frozen spans while the footage runs on at 1×.
- **Zoom**: sampled at clip ends and every zoom's breakpoints, straight lines between; static → crop + scale; moving → pad black as needed, crop to the union box, `zoompan` with piecewise-linear expressions at a fixed fps (default 30); more than 10× is capped and logged.
- **Text/SVG**: composited after the camera and the burned subtitles, each from a looped input (a title as a generated transparent SVG the frame's size: black round-joined stroke then white fill; an svg effect from the user's file scaled into its box), with alpha fades and an enable window.
- **Volume**: one `volume=<expr>:eval=frame` per cue applied in turn after the lane mix and before the narration.
- **Label**: nothing.
Frame box for every clip: no aspect → the footage's own; with an aspect the resolution tier names the short side (1080p on 9:16 = 1080×1920), even sides.

## 5. Cards (SVG inserts)
An insert path may carry `?key=value&…` (order matters; escapes `% & = ?` only). Documents declare inputs as `<!-- Input: key[flags] | Label | hint -->` (flags `keep`, `logo`) and holes as `{{name}}` / `{{name|fallback}}` (outside comments). Two shipped cards: **tier** (a board of rows S A B C D F with items "Name|logo.png" and a `new` list of arrivals with per-item timing) and **badge** (one big letter with a caption); seeds `s a b c d f .svg` and `tier.svg` plus `CARDS.md` are written into `assets/` on first use, never overwriting. Canvas 1920×1080, dark background, 2.2 s of stillness at the end. Every animation starts at 0 and waits inside `keyTimes`, so the static file is the finished card. The render bakes SMIL (animate/set/animateTransform on numbers, lengths, colours; values/keyTimes; repeat; freeze; additive) and a CSS subset (opacity, transform, fill; simple selectors; animation longhands; standard easings) to a frame sequence at the render fps (default 25). A card's own length = the last moving moment, offered as the insert's length.

## 6. Parameters used
P.policy: minRate, maxRate, minClipSeconds, rampStepSeconds, maxGain (10), speedGapSeconds, captionBatch, captionMinSeconds, effectMinSurvivingSeconds, default lengths (zoom/text/svg 3 s; label/speed/volume 2 s), default fades (text/svg 0.3; volume 0.25; stop 0.5; zoom 1), default gain 2, default rate 0.5, suggested zoom hf 0.6, decorations density (prompt). Engineering: lane height 26, grip/kill widths, snap 8/10 px, text metrics (0.58, 1.25, 0.95, 7 pt, 12 lines), edge dilation, svg preview 512 px, card constants, bake fps.

## 7. Rules
One list, one owner; nothing reaches backwards; dur is the bar for every kind; one fade rule (both fades ≥ 0, together ≤ dur, trimmed proportionally); rates average, gains multiply, sound answers do not merge; a stop is not a rate; staircases are built whole or not at all; slivers are healed, not dropped; only speed touches the clip list; camera windows have no width, overlay boxes do; the box belongs to the picture not the form; forms find effects by value; one Undo per visit, drag or hold; empty effects are never placed; a click is not a drag; framing is done paused; hit order on the picture: held box → held camera → texts top-down → the framing zoom; snapping is pixel-constant and nudging unsnapped; effects cannot leave the timeline; the lane draws what the render does; static card files are finished pictures; a path with `?` is not a file; suggested effects are clamped against the cut as applied.

## 8. Details confirmed against the code (verification pass)
- ▨ SVG with no line: "click a track first — the drawing needs a moment to appear at"; no file chosen: "choose a drawing and it goes on the picture"; a preview raster that fails logs ">>> the drawing <file> cannot be shown: <err>" once; a still that cannot be rendered ">>> the stop frame at m:ss cannot be shown in the preview: <err>"; neither stops the effect.
- A rate given for a whole segment by the cut model is a speed effect spanning it and is not counted against any decorations ceiling.
- **Cards**: a shipped card carries `data-naivepost=<kind>` and `data-naivepost-args=<defaults>` on its root; opening one as an insert redraws it from the generator with the path's parameters merged over those defaults; an unstamped file only has its holes filled. How a card was arrived at is logged per render: "drawn by the built-in <name> card[, except that <note>]", "filled in", or "has no {{placeholders}} and was not drawn by a card, so its parameters were ignored". The `CARDS.md` contract is normative: 1920×1080 with an opaque background; self-contained (images as `data:` URLs — the render folder resolves no relative href, an absolute one is refused); machine fonts as a family list, no `@font-face`; ~0.58 em per character for sizing by eye; placeholders in content and attributes, values XML-escaped, never left in the picture; `<!-- Input: key[flags] | Label | hint -->` declares the insert form. Seeds are written badges first then the board (the board embeds badge files as logos; its default `new` is "A[1.2s]: a.svg, B[1.8s]: b.svg"), the board as its template, nothing overwritten, what was written logged. A document repeating indefinitely has no length of its own: it takes the default insert length and loops for its slot; the bake writes one static SVG per frame (`f%05d.svg`). A CSS animation with no `@keyframes` is drawn as a still ("<file>: a CSS animation with no @keyframes in the file — drawn as a still").
- The 1× sound read head only opens where the debt is ≥ 0.05 s; a card, a held frame or a clip on no recording closes it, with the dip half at each side of the join.
