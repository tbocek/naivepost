# 06 — Effects

<!-- nav -->
[← 05 Cut](05-cut.md) · [↑ Contents](README.md) · [07 Narrate →](07-narrate.md)

**Flows:** [F3.1](#f31-zoom-by-hand) · [F3.2](#f32-aspect-ratio) · [F3.3](#f33-speed-and-stop-by-hand) · [F3.4](#f34-text-caption-by-hand) · [F3.5](#f35-svg-drawing-by-hand) · [F3.6](#f36-volume-by-hand) · [F3.7](#f37-label-by-hand) · [F3.8](#f38-hold-move-resize-edit-remove) · [F3.9](#f39-captions-proposed-by-the-model-after-the-cut) · [F3.10](#f310-speeds-proposed-by-the-model) · [F3.11](#f311-decorations-proposed-by-the-model) · [F3.12](#f312-clamp-to-the-cut-as-applied)
<!-- /nav -->

Zoom, speed (incl. stop), text, SVG drawing, volume, label. Each: one record in `cut.json`'s `fx` list, a bar on the effects lane, placed by hand (preview or lane) or proposed by the model after the cut. Preview and render share the functions turning an effect into a camera path, fitted title, fade or gain.

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
| cam (new) | the camera row it was framed on | – | – | – | – | – |
| lane (new) | – | – | – | – | "" = the whole bed; a lane's name = that lane only | – |

Legacy: `view` → zoom; `mute` → `snd: "mute"`. `ease` stores "" for linear, keeping old files byte-identical. Text/svg with no box → its kind's default (text: lower third; svg: middle); model-proposed captions carry no box → lower third.

## 2. The lane and the preview

![The effects lane with a zoom, a text, a speed, a volume and a label](img/06-lane.png)

<sub>Screenshot of the prototype on the ETH lecture project.</sub> Effects placed by hand for the shot: zoom and text at 1:21, speed ×0.5 at 2:23 (the rose wash on the pictures), volume at 3:55, label at 4:32. The picture shows the lane in its place among the tracks, and what surrounds it is the Cut page's, specified there and not here: the gutter with its fold-all badge and its lane and row sound switches, the selection band and the picture rows with their sound strips in [05 §1](05-cut.md#1-screen), and the 🔈 speaker badges on the rows in [05 F2.10](05-cut.md#f210-cameras-and-hearing).

![The preview during the zoom and the caption](img/06-preview.png)

<sub>Screenshot of the prototype on the ETH lecture project.</sub> At 1:22: the zoom frames the lecturer, the caption is fitted into its box — the same functions the render uses.

![The Effect menu](img/06-effect-menu.png)

<sub>Screenshot of the prototype on the ETH lecture project.</sub>

Colours: zoom (0.25,0.72,0.82), staying zoom (0.95,0.62,0.15), speed/stop (0.92,0.42,0.6), text (0.6,0.55,0.95), svg (0.4,0.8,0.5), volume (0.95,0.85,0.2), label grey-white. Rows (stacked top to bottom inside the lane, which sits between the selection band above and the picture rows below): first-fit by seconds, an effect going on the topmost row where it fits; a bar under 0.4 s is drawn on row 0 and claims no seconds there; a bar is drawn at least 48 px wide; lane one row deep even when empty.

Preview (paused): outside the camera rect dimmed black 0.45; rect stroked white with a plate (measured on the picture as the panel shows it: contained and centred, so on a panel wider or taller than the footage the rect and every mark are drawn on the centred picture, not from the panel's corner); visible overlays at real alpha, the held one full with a dashed violet outline; a box being drawn dashed. Playing: only the black mask over what the finished frame hides, plus titles; camera layer on the smoothed live clock. A stop's still: rendered from the scene's own camera, fitted on the footage's transform.

Preview vs render, deliberately different: preview runs speed at one flat rate (a rate change is a flushing seek); render follows ramps and averages overlaps; the two 1× sound answers cannot be previewed. Only "1× to the scene's end" draws the dashed debt tail ("1× to the effect's end" closes its gap on the effect's last frame); tail suppressed under 0.05 s of debt; "sound X s behind|ahead" plate once wider than 60 px.

## 3. Flows

**Tools, for [F3.9](#f39-captions-proposed-by-the-model-after-the-cut)–[F3.11](#f311-decorations-proposed-by-the-model):** each pass offers its tools and reads the calls. A server refusing the `tools` field gets the same body without it and the reply is parsed as JSON; a reply carrying a document instead of calls while tools are on the wire is asked again once. The passes run after the cut in the order captions, speeds, decorations, each only where the policy lets it run (a pass ruled out says e.g. "decorations are off — the context ruled them out"); each pass that places something is its own Undo step.

### F3.1 Zoom by hand

<sub><!-- back -->[← F2.15](05-cut.md#f215-the-camera-follows-the-speaker-new) · [↑ 06 Effects](#06--effects) · [all flows](11-flow-index.md#3-all-flows) · [F3.2 →](#f32-aspect-ratio)</sub>

![Zoom: the box drawn on the preview and its form](img/06-zoom.png)

<sub>Screenshot of the prototype on the ETH lecture project.</sub> A free rectangle, not the cut's shape the arm's words promise (S2).

```mermaid
flowchart TD
  A(["Effect ▾ → Zoom"]) --> L{"a line?"}
  L -- no --> R["“click a track first — the effect needs a moment to happen at”"]:::refuse
  L -- yes --> ARM["armed · the camera layer goes down so the whole source shows"]
  ARM --> D["drag a box on the preview · under 12 px on either axis: disarmed, “too small to zoom — …”"]
  D --> DEF["defaults: glide 1 s in and out · 3 s<br/>stay when an aspect is set and no staying zoom exists"]
  DEF --> F["form “Zoom at m:ss”: Length · Pull back / Stay on it · Fade in · Fade out · Curve"]:::done
  classDef refuse fill:#fde2e1,stroke:#c01c28,color:#1a1a1a
  classDef done fill:#e3f1e6,stroke:#2e7d32,color:#1a1a1a
  classDef ask fill:#e8eefc,stroke:#3a63c8,color:#1a1a1a
```

S1 ✚ Effect ▾ → ⊕ Zoom arms a drag (no line → "click a track first — the effect needs a moment to happen at"; same entry again disarms: "zoom disarmed — the picture stops waiting for a box"; Esc: "zoom released — no box was drawn"). Status/panel text: "Drag a box on the video: the picture zooms there and comes back out on its own, or stays on it — the form that opens is where that is said. The box keeps the cut's shape; let go near the full width or height to snap to it. It starts at the red line and runs 3 s; the form that opens says how long." The armed path always names and uses 3 s, whatever is marked. Camera layer goes down; whole source visible. S2 Drag a box on the preview (under 12 px on either axis: the arm goes, "too small to zoom — a box has to be at least 12 px"). The gesture is a free rectangle, though the words above still promise the cut's shape and a snap: neither happens, for zoom, text or svg. Only the box's centre and height are stored; the window's width follows the aspect. S3 Defaults: glide 1 s in and out, length 3 s; `stay` when an aspect is set and no staying zoom exists yet (then no glides). S4 Form "Zoom at m:ss": Length (s); At the end: Pull back / Stay on it (the answer in force listed first); Fade in (s); Fade out (s) (greyed when staying: "A camera that stays has no way back, so no fade out."); Curve (Linear). Apply refuses a length under 0.4 ("a camera move of 0.30 s is shorter than the shortest one this form takes (0.4 s)"). S5 The drag ends with "zoom at m:ss — the form says how it ends"; Apply places the record and says "zoom at m:ss — ↶ Undo takes it back" (or "… — the video shows this region from here on; ↶ Undo takes it back"). A band crossing to another camera row is trimmed at Apply ([§9](#9-where-recordings-overlap)).
Nothing armed or held: a click on the preview picture (press and release without passing GTK's drag threshold) only plays and pauses, whatever is placed under it ([05 F2.1](05-cut.md#f21-play-the-recording-) S2). Framing needs travel. On a paused preview a drag that starts inside a zoom's box takes that zoom (moves or sizes it, [F3.8](#f38-hold-move-resize-edit-remove) S3; under 12 px it only takes it in hand: "zoom at m:ss re-framed — ↶ Undo takes it back", nothing written); a drag clear of every box draws a new rectangle and opens its form "Zoom at m:ss", whatever zoom stands there, saying "zoom at m:ss — ↶ Undo takes it back" before anything is placed; a marked stretch covering the line sets the length, while the start stays the line. Under 12 px on either axis, past the click threshold: "no zoom armed — pick ⊕ Zoom first". First rewrite: one click ran both, the toggle and the framing press, so a click over a placed zoom started play and took the zoom, a click where no zoom stood opened a zoom form instead of playing, and a drag clear of every box under a placed zoom was refused.
Camera path: from the centred full-fill slice; each zoom glides from the camera's position at its start to its rect, holds, and (unless staying) glides back; a staying zoom becomes the new settled frame; nothing reaches backwards; fade-in wins overlaps. Rect clamp hf ∈ [0.02, 12], centre ∈ [−2, 3].

### F3.2 Aspect ratio

<sub><!-- back -->[← F3.1](#f31-zoom-by-hand) · [↑ 06 Effects](#06--effects) · [all flows](11-flow-index.md#3-all-flows) · [F3.3 →](#f33-speed-and-stop-by-hand)</sub>

```mermaid
flowchart TD
  A(["Aspect ratio ▾"]) --> K{"which?"}
  K -- source --> S["“aspect: the source's own — …”"]:::done
  K -- "9:16 · 1:1 · 4:5 · 16:9" --> Z{"a staying zoom already?"}
  Z -- no --> ADD["a staying zoom at 0:00 holds the whole frame for 1 s, same Undo step<br/>“aspect 9:16 — a ⊕ zoom at 0:00 holds the whole frame, centred”"]:::done
  Z -- yes --> KEEP["“aspect ‹s› — the zooms on the lane decide the framing”"]:::done
  classDef refuse fill:#fde2e1,stroke:#c01c28,color:#1a1a1a
  classDef done fill:#e3f1e6,stroke:#2e7d32,color:#1a1a1a
  classDef ask fill:#e8eefc,stroke:#3a63c8,color:#1a1a1a
```

Dropdown source / 9:16 / 1:1 / 4:5 / 16:9 in the form column ("the shape of the finished video — source is the footage's own, 9:16 is a vertical short. The whole frame fits inside it (bars either side) until ▭ View frames a region; the outline on the preview is what the finished video shows"). Non-source aspect, no staying zoom → a staying zoom appended at 0:00 holding the whole frame for 1 s, same Undo step ("aspect 9:16 — a ⊕ zoom at 0:00 holds the whole frame, centred"); one already there → "aspect <s> — the zooms on the lane decide the framing"; source → "aspect: the source's own — the video comes out the shape it was filmed".

### F3.3 Speed and stop by hand

<sub><!-- back -->[← F3.2](#f32-aspect-ratio) · [↑ 06 Effects](#06--effects) · [all flows](11-flow-index.md#3-all-flows) · [F3.4 →](#f34-text-caption-by-hand)</sub>

![Speed ×0.5 on a selected stretch](img/06-speed.png)

<sub>Screenshot of the prototype on the ETH lecture project.</sub>

```mermaid
flowchart TD
  A(["Effect ▾ → Speed"]) --> S{"a selection ≥ 0.2 s?"}
  S -- yes --> SL["t and dur from it · rate 0.5"]
  S -- no --> L{"a line?"}
  L -- yes --> ST["a stop · 2 s · 0.5 s fades · form opens at 0:00"]
  L -- no --> R["“click a track or mark a stretch first — …”"]:::refuse
  SL --> F["form “Speed a – b”: Speed × · Sound · Length · Fade in · Fade out · Curve"]
  ST --> F
  F --> Z{"rate 0?"}
  Z -- yes --> STOP["a stop · length ≥ 0.5 · no clamp"]:::done
  Z -- no --> CL["clampSpeed: rate into P.eng.minRate … maxRate<br/>on screen ≥ P.eng.minClipSeconds, the rate gives way"]:::done
  classDef refuse fill:#fde2e1,stroke:#c01c28,color:#1a1a1a
  classDef done fill:#e3f1e6,stroke:#2e7d32,color:#1a1a1a
  classDef ask fill:#e8eefc,stroke:#3a63c8,color:#1a1a1a
```

S1 ⏩ Speed: selection ≥ 0.2 s (every effect's floor for a band to count as marked) → t/dur from it, rate 0.5; only a line → a stop, 2 s, 0.5 s fades, its form opening at 0:00 rather than at the line; neither → "click a track or mark a stretch first — speed needs seconds to work on". S2 Form "Speed a – b": Speed × (×0 — stop, ×0.25, ×0.5, ×0.75, ×1 — as filmed, ×1.5, ×2, ×4, ×8, ×20, ×100, Custom…, with a rate entry beside it greyed until Custom… is picked); Sound (With the picture / With the picture, pitched / 1× to the effect's end / 1× to the scene's end / Silent); under Sound a cost note, worded when the form opens ("N s on screen: the sound ends N s behind the picture, and going back in sync skips those seconds."; for a stop "A stop's footage runs on at 1× under the held frame, so every answer but Silent sounds the same here."); Length (s); Fade in (s) ("…A ramp needs about 0.6s of footage for every × of the rate…"); Fade out (s); Curve. S3 Rate 0 → stop, length ≥ 0.5, no clamp; else clampSpeed: rate ∈ [P.eng.minRate 0.05, P.eng.maxRate 100], on-screen length ≥ P.eng.minClipSeconds (0.5); the rate gives way. S4 The press says "Speed a – b — say the rate and ▸ Apply places it"; Apply refuses a stop on a second the cut does not keep ([§9](#9-where-recordings-overlap)). Status "… — the footage plays at that rate there and the cut gets longer or shorter to match; ↶ Undo takes it back" / "… — the picture stands still there while the clock runs…".
Arithmetic: overlapping rates average per span; spans rendering under 0.5 s heal into the longer neighbour; ramps are geometric staircases, each stair ≥ P.eng.rampStepSeconds (0.6) on screen, built whole or not at all; footage under a still runs at 1×.

### F3.4 Text (caption) by hand

<sub><!-- back -->[← F3.3](#f33-speed-and-stop-by-hand) · [↑ 06 Effects](#06--effects) · [all flows](11-flow-index.md#3-all-flows) · [F3.5 →](#f35-svg-drawing-by-hand)</sub>

![A caption placed by a click in the lower third](img/06-text.png)

<sub>Screenshot of the prototype on the ETH lecture project.</sub>

```mermaid
flowchart TD
  A(["Effect ▾ → Text"]) --> ARM["armed · the camera layer stays up"]
  ARM --> K{"drag or click?"}
  K -- drag --> BOX["the box as drawn"]
  K -- click --> LT["the lower third 0.5, 0.78, 0.8, 0.16"]
  BOX --> S{"a line or a selection?"}
  LT --> S
  S -- no --> R["“click a track first — the effect needs a moment to happen at”"]:::refuse
  S -- yes --> F["form “Text at m:ss”: the words · Length · fades 0.3 · Curve"]
  F --> W{"any words?"}
  W -- no --> N["not placed · “type the words and they go on the picture — …”"]:::refuse
  W -- yes --> FIT["fitted: the largest size where ≤ 12 lines fit, floor 7 pt<br/>bold sans, white with a dark edge · preview = render"]:::done
  classDef refuse fill:#fde2e1,stroke:#c01c28,color:#1a1a1a
  classDef done fill:#e3f1e6,stroke:#2e7d32,color:#1a1a1a
  classDef ask fill:#e8eefc,stroke:#3a63c8,color:#1a1a1a
```

S1 ❝ Text arms: "Drag the box the words go in — anywhere on the picture, any shape. A click puts one across the lower third.", then ⊕ Zoom's tail ("It starts at the red line and runs 3 s; the form that opens says how long."); same entry again disarms ("text disarmed — the picture stops waiting for a box"). Camera layer stays up (box is on the output frame). S2 Drag or click (default box: lower third {0.5, 0.78, 0.8, 0.16}); under 12 px on either axis counts as the click; t/dur from the line (3 s) or selection; fades 0.3; neither a line nor a selection → "click a track first — the effect needs a moment to happen at", no form. The drag says "Text at m:ss — type the words and ▸ Apply puts them on the picture". S3 Form "Text at m:ss": Words (3-line box, the column's full width, above the other fields; Enter = new line), Length, Fade in, Fade out, Curve; Apply refuses a length under 0.3 ("a caption of 0.20 s is shorter than the shortest one this form takes (0.3 s)"). Empty words not placed ("type the words and they go on the picture — the form applies as you type it"). Apply says "Text recorded — ↶ Undo takes it back". S4 Fitting: largest size where ≤ 12 lines of 0.58 em per character fit the box (min 7 pt); bold sans-serif, white, dark dilated edge; same function for preview and render. S5 On the preview a placed caption's box (travel ≥ 12 px on one axis; within 8 px of an edge it resizes, inside it moves) moves — snapping within 10 px to the finished frame's left edge, centre, right edge (and top, middle, bottom); a moved box offers all three of its own lines, a dragged edge only itself — and resizes (independent axes, 16 px floor); a press without travel toggles play/pause wherever it lands.

### F3.5 SVG drawing by hand

<sub><!-- back -->[← F3.4](#f34-text-caption-by-hand) · [↑ 06 Effects](#06--effects) · [all flows](11-flow-index.md#3-all-flows) · [F3.6 →](#f36-volume-by-hand)</sub>

```mermaid
flowchart TD
  A(["Effect ▾ → SVG"]) --> L{"a line?"}
  L -- no --> R1["“click a track first — the drawing needs a moment to appear at”"]:::refuse
  L -- yes --> CH["“Choose a drawing to lay over the video” · opens in assets/"]
  CH --> F{"a file chosen?"}
  F -- no --> R2["“choose a drawing and it goes on the picture”"]:::refuse
  F -- yes --> ARM["drag the box, or click: the middle 0.5, 0.5, 0.6, 0.6"]
  ARM --> FORM["form “SVG at m:ss”: file + Choose… · Length · fades · Curve<br/>preview raster 512 px, cached per file"]:::done
  classDef refuse fill:#fde2e1,stroke:#c01c28,color:#1a1a1a
  classDef done fill:#e3f1e6,stroke:#2e7d32,color:#1a1a1a
  classDef ask fill:#e8eefc,stroke:#3a63c8,color:#1a1a1a
```

▨ SVG asks for the file first ("Choose a drawing to lay over the video", from `assets/`), then arms ("Drag the box <file> goes in — … the drawing keeps its own shape inside it. A click puts one across the middle."). Default box: middle {0.5, 0.5, 0.6, 0.6}; the drag says "SVG at m:ss — press ▸ Apply puts the drawing on the picture". Form "SVG at m:ss": File + Choose… (a new file keeps the box and the seconds), Length, fades, Curve, then the raster's picture with its note ("preview raster 512 px, ready — one per file"); the box is fixed by the drag, since a placed drawing's box has no drag on the picture. Preview raster via ffmpeg, 512 px, transparent, cached per file; made on a worker (new, [03 §1](03-shell.md#1-the-window)): the form says "being made" and gets its picture when the raster lands. No file → not placed. Apply says "SVG recorded — ↶ Undo takes it back".

### F3.6 Volume by hand

<sub><!-- back -->[← F3.5](#f35-svg-drawing-by-hand) · [↑ 06 Effects](#06--effects) · [all flows](11-flow-index.md#3-all-flows) · [F3.7 →](#f37-label-by-hand)</sub>

![The volume form](img/06-volume.png)

<sub>Screenshot of the prototype on the ETH lecture project.</sub> Defaults: 200 %, 2 s, ramps of 0.2–0.25 s.

```mermaid
flowchart TD
  A(["Effect ▾ → Volume"]) --> S{"a selection, or a line?"}
  S -- neither --> R["“click a track or mark a stretch first — volume needs seconds to work on”"]:::refuse
  S -- yes --> F["form “Volume a – b”: Volume % 0…1000 · Length ≥ 0.1 · Fade in · Fade out · Curve"]
  F --> OK["overlapping gains multiply · applied in the preview even while paused<br/>“… — the picture is untouched”"]:::done
  classDef refuse fill:#fde2e1,stroke:#c01c28,color:#1a1a1a
  classDef done fill:#e3f1e6,stroke:#2e7d32,color:#1a1a1a
  classDef ask fill:#e8eefc,stroke:#3a63c8,color:#1a1a1a
```

🔊 Volume: selection or line..+2 s ("click a track or mark a stretch first — volume needs seconds to work on"); defaults gain 200 %, ramps 0.25 s; the press says "Volume a – b — say the percent and ▸ Apply places it". Form "Volume a – b": Volume % (0..1000; "…up to 1000 for a passage recorded too quietly to hear — though a passage lifted that far brings its hiss up with it"), Length (≥ 0.1), Fade in ("0 is the hard step, which on a big change is audible as a click"), Fade out, Curve. Overlapping gains multiply; preview applies gain even paused. Status "m:ss louder|quieter|silent at N% for X.Xs — the picture is untouched".

### F3.7 Label by hand

<sub><!-- back -->[← F3.6](#f36-volume-by-hand) · [↑ 06 Effects](#06--effects) · [all flows](11-flow-index.md#3-all-flows) · [F3.8 →](#f38-hold-move-resize-edit-remove)</sub>

![The label form](img/06-label.png)

<sub>Screenshot of the prototype on the ETH lecture project.</sub>

```mermaid
flowchart TD
  A(["Effect ▾ → Label"]) --> S{"a selection, or a line?"}
  S -- neither --> R["“click a track or mark a stretch first — a label names a moment, …”"]:::refuse
  S -- yes --> F["form “Label at m:ss”: Name · Length ≥ 0.4"]
  F --> N{"a name?"}
  N -- no --> NO["“type a name and it is marked — nothing is placed until then”"]:::refuse
  N -- yes --> TAG["a tag on the lane · never rendered · the narration brief lists it as MARKED"]:::done
  classDef refuse fill:#fde2e1,stroke:#c01c28,color:#1a1a1a
  classDef done fill:#e3f1e6,stroke:#2e7d32,color:#1a1a1a
  classDef ask fill:#e8eefc,stroke:#3a63c8,color:#1a1a1a
```

🏷 Label: selection or line..+2 s ("click a track or mark a stretch first — a label names a moment, so it needs one"); Form "Label at m:ss" (Length floor 0.4 s): Name (at most 10 characters, held at the keyboard; "what you call this moment -- "the reveal", "boss fight". It changes nothing in the video: it is written into the brief the narration writer is given…"), Length (the stretch a clip must overlap). Empty name not placed ("type a name and it is marked — nothing is placed until then"). Apply says "label “…” at m:ss — nothing changes in the video; the narration is told about it". Drawn as a tag; never rendered; narration brief lists it as MARKED.

### F3.8 Hold, move, resize, edit, remove

<sub><!-- back -->[← F3.7](#f37-label-by-hand) · [↑ 06 Effects](#06--effects) · [all flows](11-flow-index.md#3-all-flows) · [F3.9 →](#f39-captions-proposed-by-the-model-after-the-cut)</sub>

```mermaid
flowchart TD
  P(["press a band"]) --> T{"moved under 4 px?"}
  T -- yes --> FORM["a click: the form opens"]:::done
  T -- no --> PU["picked up: the line moves to its start · “‹label› picked up”"]
  PU --> K{"where did the drag start?"}
  K -- "the middle" --> MV["the band moves · both ends snap within 8 px"]
  K -- "an end, band ≥ 30 px" --> RS["that end moves · length ≥ 0.1 s"]
  MV --> H["held until the line walks 1/24 s off it · Esc drops it"]
  RS --> H
  H --> X["✕ or ⌦: “removed ‹label› — ↶ Undo takes it back”"]
  H --> E["✎ Edit: found by kind and start · the box from the live effect"]
  classDef refuse fill:#fde2e1,stroke:#c01c28,color:#1a1a1a
  classDef done fill:#e3f1e6,stroke:#2e7d32,color:#1a1a1a
  classDef ask fill:#e8eefc,stroke:#3a63c8,color:#1a1a1a
```

S1 A drag on a band picks it up (line moves to its start; status "<label> picked up", where <label> in this flow is the kind's word: Zoom, Speed, Text, SVG, Volume, Label; Insert becomes ✎ Edit); ends are grips (6 px reach) when the band is ≥ 30 px; 4 px of travel or less is a click → opens the form, the column's pinned heading reading "edit <Kind>". S2 Drag moves the whole band (clamped inside the timeline; both ends offered to snap marks: segment ends, other effects' ends, 8 px) or an end (length ≥ 0.1 s), saying "<label> moved — ↶ Undo takes it back" (an end: "<label> resized — ↶ Undo takes it back"). Frame steps and ←/→ nudge a held effect unsnapped ("<label> moved — …"). Hold drops when the line walks > 1/24 s off the band. S3 The ✕ in the band's middle (≥ 32 px; tooltip "remove this effect — ↶ Undo takes it back") or ⌦ removes it ("removed <label> — ↶ Undo takes it back"). A text box or camera rect dragged on the picture ends with "<label> moved — ↶ Undo takes it back" or "<label> resized — …", written once at release when travel is ≥ 12 px on one axis; a camera rect's top or bottom edge (8 px) sizes its height, its left or right edge and its interior slide it, unsnapped. A band drag on the lane spends its one Undo on the first 2 px of travel. Esc drops the hold and disarms ("cancelled"). S4 Edit re-finds the effect by kind and start (|Δt| < 1 ms; "that effect is no longer in the cut — nothing was changed"); box always from the live effect, never the form's snapshot. S5 Forms (in the Cut page's form column, right of the preview, in place of the idle readings; label + box pairs in the order listed, one to a line, except that the text form puts Fade in and Fade out on one line, the volume form Volume % and Length on one and Fade in, Fade out and Curve on the next, and the label form Name and Length on one; Curve is a dropdown in the volume form and the plain word Linear elsewhere) show the typed number after a 300 ms debounce ("N — kept as you type") without touching the cut: the record is written by Apply, which appends a new record, also when the form was opened on an existing effect; a refusal (a length under the form's floor names the floor) leaves the form open; Cancel (Esc too, except on the zoom form, whose Esc only drops the arm) writes nothing ("left as it was — no zoom placed", likewise caption, drawing, speed, volume; a label: "left as it was — nothing marked"); each form ends with "Apply" and "Cancel", left to right, and under them the note "Kept as you type — ↶ Undo takes the whole edit back."

### F3.9 Captions proposed by the model (after the cut)

<sub><!-- back -->[← F3.8](#f38-hold-move-resize-edit-remove) · [↑ 06 Effects](#06--effects) · [all flows](11-flow-index.md#3-all-flows) · [F3.10 →](#f310-speeds-proposed-by-the-model)</sub>

```mermaid
flowchart TD
  A(["after the cut"]) --> B["clips in batches of P.machine.captionBatch · layout below"]
  B --> T["tools: add_caption · finish"]
  T --> V{"the clip one of this batch?"}
  V -- no --> RJ["the whole reply rejected, retried once"]:::refuse
  V -- yes --> L{"≥ P.policy.captionMinSeconds, with words?"}
  L -- no --> SK["skipped"]
  L -- yes --> OK["placed · fades min 0.3, d/4"]:::done
  RJ -. still failing .-> FB["“!!! captions: clips a–b skipped -- the cut stands without them”"]
  classDef refuse fill:#fde2e1,stroke:#c01c28,color:#1a1a1a
  classDef done fill:#e3f1e6,stroke:#2e7d32,color:#1a1a1a
  classDef ask fill:#e8eefc,stroke:#3a63c8,color:#1a1a1a
```

```text
User Context
THE CLIPS, AND WHAT WAS SAID OVER EACH:
CLIP 1: 14.2 s long
  [+0.4s] …   (offsets inside the clip, never session seconds)
CLIP 2: …
```

Batches of P.machine.captionBatch (5) clips (the cut's kept runs, touching scenes merged, numbered from 1 in play order); message = User Context + "THE CLIPS, AND WHAT WAS SAID OVER EACH:" + "CLIP n: X s long" + the lines as offsets. **Tools**: `add_caption(clip, start, end, text)`, `finish` ([`02-services.md` §3.7](02-services.md#37-captions--speed--decorations-per-clip)). Validated in the tool: clip must be one given; the span is clamped into the clip, then end−start ≥ P.policy.captionMinSeconds (0.3). Prototype, kept by the rewrite: a clip number outside the batch rejects the **whole** reply ("clip N is not one of the clips given (a to b)") and costs a retry; a caption under the floor or with no words is silently skipped. Prompt rules: captions only if the context asks; clean like a subtitler (no ehm, no stutters, sentence case, swearing kept, never a paraphrase); never caption an aside to the editor. Fades min(0.3, d/4). A rejection is logged (">>> captions: <reason>"); a batch rejected twice is skipped ("!!! captions: clips a–b skipped -- the cut stands without them"); an ask failing at the wire logs "!!! captions: <reason>" and places nothing for its batch. The pass says "N caption(s) placed", each skipped batch's line appended (no kept scene: "nothing kept to caption").

### F3.10 Speeds proposed by the model

<sub><!-- back -->[← F3.9](#f39-captions-proposed-by-the-model-after-the-cut) · [↑ 06 Effects](#06--effects) · [all flows](11-flow-index.md#3-all-flows) · [F3.11 →](#f311-decorations-proposed-by-the-model)</sub>

```mermaid
flowchart TD
  A(["one call · the captions and the footage total"]) --> T["tools: set_clip_speed · finish"]
  T --> C{"rate over 1 on a captioned clip?"}
  C -- yes --> R["refused"]:::refuse
  C -- no --> N{"rate ≈ 1?"}
  N -- yes --> IG["ignored"]
  N -- no --> OK["a speed effect over the clip"]
  OK --> M["same-rate stretches nearer than P.policy.speedGapSeconds merged<br/>“>>> speed: N clip(s) run fast — …”"]:::done
  T -. two rounds fail .-> F["“!!! speed: no usable answer — every clip plays at 1”"]:::refuse
  classDef refuse fill:#fde2e1,stroke:#c01c28,color:#1a1a1a
  classDef done fill:#e3f1e6,stroke:#2e7d32,color:#1a1a1a
  classDef ask fill:#e8eefc,stroke:#3a63c8,color:#1a1a1a
```

One call over the cut's scenes in session order, numbered from 1; briefs note captions ("…, N caption(s) -- runs at 1") and the footage total. **Tools**: `set_clip_speed(clip, rate)` (rate > 1 on a captioned clip refused; rate ≈ 1 ignored), `finish`. Up to two rounds (fault appended and logged ">>> speed rejected: <fault>", call put again), then "!!! speed: no usable answer — every clip plays at 1". Prompt rules: longer dull stretch → higher rate; a clip with speech plays at 1 unless the context asks for more; slow motion only on a short clip the video is about; one rate per clip. A rate ≤ 0 is ignored like one ≈ 1; a refusal is said in the status and the log and costs no round. Same-rate stretches nearer than P.policy.speedGapSeconds (4) merge. Log ">>> speed: N clip(s) run fast — a of video from b of footage" (N counts clips at any rate but 1, slowed ones too), or "nothing runs fast — every clip plays at 1" when nothing was placed. No length gate.

### F3.11 Decorations proposed by the model

<sub><!-- back -->[← F3.10](#f310-speeds-proposed-by-the-model) · [↑ 06 Effects](#06--effects) · [all flows](11-flow-index.md#3-all-flows) · [F3.12 →](#f312-clamp-to-the-cut-as-applied)</sub>

```mermaid
flowchart TD
  A(["one call over every clip · told the captions and speeds"]) --> T["tools: add_effect zoom · stop · volume · finish"]
  T --> V{"a clip in the list?"}
  V -- no --> RJ["the whole reply rejected"]:::refuse
  V -- yes --> K{"a known kind, a real span?"}
  K -- no --> SK["refused, the sentence said"]
  K -- yes --> D["the app's defaults: zoom centred at 60 % · volume ramps min 1, d/4<br/>gain 1 ignored, 0 kept"]:::done
  T -. two rounds fail .-> F["“!!! effects: no usable answer -- the cut stands without them”"]:::refuse
  classDef refuse fill:#fde2e1,stroke:#c01c28,color:#1a1a1a
  classDef done fill:#e3f1e6,stroke:#2e7d32,color:#1a1a1a
  classDef ask fill:#e8eefc,stroke:#3a63c8,color:#1a1a1a
```

One call over every clip, told captions and speeds. **Tools**: `add_effect(clip, kind zoom|stop|volume, start, end, gain?)`, `finish`. A clip outside the list rejects the whole reply ("clip N is not one of the clips given (1 to M)"); an unknown kind or a span empty after clamping into the clip → the tool's sentence, appended to the pass's status and logged, costing no round; a volume with gain 1 or none is dropped in silence. A reply proposing over P.machine.maxProposedEffects (1000) is unusable ("the reply proposes N effects, over P.machine.maxProposedEffects (1000)"). Clips as [F3.10](#f310-speeds-proposed-by-the-model)'s. Up to two rounds (fault logged ">>> effects rejected: <fault>"), then "!!! effects: no usable answer -- the cut stands without them". Otherwise ">>> effects: N decoration(s)" or "nothing proposed — the cut stands as it is", with " — past the guide: three or four across five minutes of finished video (a-b for m:ss)" when the count passes the scaled density. Prompt: few and deliberate (about three or four per five minutes unless the context says otherwise); zoom 2–4 s; one stop per video; volume for level, ducking or muting; never zoom past captions. App-applied defaults: zoom centre, 60 % height, glides min(1, d/3); stop fades min(0.3, d/4); volume ramps min(1, d/4); gain of 1 or none ignored, 0 kept.

### F3.12 Clamp to the cut as applied

<sub><!-- back -->[← F3.11](#f311-decorations-proposed-by-the-model) · [↑ 06 Effects](#06--effects) · [all flows](11-flow-index.md#3-all-flows) · [F4.1 →](07-narrate.md#f41--write-and-speak)</sub>

```mermaid
flowchart TD
  A(["after snapping, dead air, marks, coalescing · or a hand edit of the scenes"]) --> K{"what kind?"}
  K -- "a point effect" --> P{"a footage clip under it?"}
  P -- no --> DR["dropped"]
  P -- yes --> KP["kept"]:::done
  K -- "zoom · text · svg · volume" --> TR["trimmed only where footage is gone: crosses scene and Split borders"]
  TR --> S{"≥ P.eng.effectMinSurvivingSeconds?"}
  S -- no --> DR
  S -- yes --> KP
  K -- speed --> SP["trimmed like the rest, then re-clamped unless a stop"]:::done
  K -- label --> DL["trimmed like the rest<br/>prototype: dropped outright"]
  DR --> LOG["“>>> N effect(s) pointed at footage the final cut does not keep — dropped”"]
  classDef refuse fill:#fde2e1,stroke:#c01c28,color:#1a1a1a
  classDef done fill:#e3f1e6,stroke:#2e7d32,color:#1a1a1a
  classDef ask fill:#e8eefc,stroke:#3a63c8,color:#1a1a1a
```

After snapping, dead-air and mark removal, coalescing: a point effect needs a footage clip under it; a zoom/text/svg/volume band crosses scene and Split borders freely and is trimmed only to the first and last footage second it still covers, dropped when under P.eng.effectMinSurvivingSeconds (1.0) of it is left in the cut (a band the cut keeps whole stays, however short); a trimmed speed is re-clamped unless a stop; a label is trimmed like the rest (prototype: dropped outright, never trimmed — yet it marks a moment for the narration brief, and the moment may still be in the cut); fades shrink proportionally; ">>> N effect(s) pointed at footage the final cut does not keep — dropped", then a line naming up to six of them, and a dropped stop's own "there is no frame at m:ss — that second is not in the cut, so nothing can be held on it". Suggested effects replace the list, same Undo step as the segments; the three passes then add to it.
The same clamp runs after every hand edit that rewrites the scenes (a verb, a scene's ✕, a trim's release), amended onto that edit's Undo step, with the same log lines and the status "effects clamped to the cut — N dropped" (only trimmed: "effects trimmed to the cut — N kept"); when nothing changes it writes nothing.

## 4. Render (how each effect becomes ffmpeg)

- **Speed**: the only effect changing the clip list: clips split at rate boundaries, the middle carries the rate (`setpts=PTS/rate`; sound by atempo chain, asetrate when pitched, a mute expression when silent, or a planned 1× read head with 0.15 s dips at run ends for own/scene). Inserts and spliced cards keep their own clock.
- **Stop**: a still of the frame at t (scene's own camera) overlaid with alpha fades for the frozen spans; footage runs on at 1×.
- **Zoom**: sampled at clip ends and every zoom's breakpoints, straight lines between; static → crop + scale; moving → pad black as needed, crop to the union box, `zoompan` with piecewise-linear expressions at fixed fps (default 30); > 10× capped and logged.
- **Text/SVG**: composited after the camera and burned subtitles, each from a looped input (title: generated transparent SVG the size of its box, laid out by the preview's fitting, a black round-joined stroke painted under the white fill; svg effect: the user's file scaled into its box), with alpha fades and an enable window.
- **Volume**: one `volume=<expr>:eval=frame` per cue, in turn: a whole-bed cue after the lane mix and before the narration, on every clip including a card; a lane-named cue before the mix, only where that lane is heard.
- **Label**: nothing.
Frame box for every clip: no aspect → the footage's own; with an aspect the resolution tier names the short side (1080p on 9:16 = 1080×1920), even sides.

## 5. Cards (SVG inserts)

An insert path may carry `?key=value&…` (order matters; escapes `% & = ?` only). Documents declare inputs as `<!-- Input: key[flags] | Label | hint -->` (flags `keep`, `logo`) and holes as `{{name}}` / `{{name|fallback}}` (outside comments). Two shipped cards: **tier** (a board of rows S A B C D F with items "Name|logo.png" and a `new` list of arrivals with per-item timing) and **badge** (one big letter with a caption); seeds `s a b c d f .svg`, `tier.svg` and `CARDS.md` written into `assets/` on first use, never overwriting. Canvas 1920×1080, dark background, 2.2 s stillness at the end. Every animation starts at 0 and waits inside `keyTimes`, so the static file is the finished card. Render bakes SMIL (animate/set/animateTransform on numbers, lengths, colours; values/keyTimes; repeat; freeze; additive) and a CSS subset (opacity, transform, fill; simple selectors; animation longhands; standard easings) to frames at the render fps (default 25). Card length = last moving moment, offered as the insert's length.

## 6. Parameters used

Parameters ([10](10-parameters.md)): minRate, maxRate, minClipSeconds, rampStepSeconds, maxGain (10), speedGapSeconds, captionBatch, captionMinSeconds, effectMinSurvivingSeconds, default lengths (zoom/text/svg 3 s; label/speed/volume 2 s), default fades (text/svg 0.3; volume 0.25; stop 0.5; zoom 1), default gain 2, default rate 0.5, suggested zoom hf 0.6, maxProposedEffects (1000), forms' typed floors (zoom 0.4, text/svg 0.3, speed 0.5 for a stop else clampSpeed, volume 0.1, label 0.4), the 0.2 s floor under which a band is not a marked stretch, decorations density (prompt). Engineering: lane height 26, grip/kill widths, snap 8/10 px, form debounce 300 ms, text metrics (0.58, 1.25, 0.95, 7 pt, 12 lines), edge dilation, svg preview 512 px, card constants, bake fps.

## 7. Rules

One list, one owner; nothing reaches backwards; dur is the bar for every kind; one fade rule (both fades ≥ 0, together ≤ dur, trimmed proportionally); rates average, gains multiply, sound answers do not merge; a stop is not a rate; staircases are built whole or not at all; slivers are healed, not dropped; only speed touches the clip list; camera windows have no width, overlay boxes do; the box belongs to the picture not the form; forms find effects by value; one Undo per visit, drag or hold; empty effects are never placed; a click is not a drag; framing is done paused; with an effect held or anything armed a picture drag edits no placed box; otherwise a text box (first in list order, grown by 8 px) goes before the framing zoom; snapping is pixel-constant and nudging unsnapped; effects cannot leave the timeline; the lane draws what the render does; static card files are finished pictures; a path with `?` is not a file; suggested effects are clamped against the cut as applied.

## 8. Details confirmed against the code (verification pass)

- ▨ SVG with no line: "click a track first — the drawing needs a moment to appear at"; no file chosen: "choose a drawing and it goes on the picture"; failed preview raster logs ">>> the drawing <file> cannot be shown: <err>" once; unrenderable still ">>> the stop frame at m:ss cannot be shown in the preview: <err>"; neither stops the effect.
- A cut-model rate for a whole segment is a speed effect spanning it, not counted against any decorations ceiling.
- **Cards**: a shipped card carries `data-naivepost=<kind>` and `data-naivepost-args=<defaults>` on its root; as an insert it is redrawn from the generator with the path's parameters merged over those defaults; an unstamped file only gets its holes filled. Logged per render, how a card was arrived at: "drawn by the built-in <name> card[, except that <note>]", "filled in", or "has no {{placeholders}} and was not drawn by a card, so its parameters were ignored". The `CARDS.md` contract is normative: 1920×1080, opaque background; self-contained (images as `data:` URLs — the render folder resolves no relative href, refuses an absolute one); machine fonts as a family list, no `@font-face`; ~0.58 em per character for sizing by eye; placeholders in content and attributes, values XML-escaped, never left in the picture; `<!-- Input: key[flags] | Label | hint -->` declares the insert form. Seeds written badges first, then the board (embeds badge files as logos; default `new` "A[1.2s]: a.svg, B[1.8s]: b.svg") as its template; nothing overwritten; what was written logged. An indefinitely repeating document has no own length: takes the default insert length, loops for its slot; the bake writes one static SVG per frame (`f%05d.svg`). A CSS animation with no `@keyframes` is drawn as a still ("<file>: a CSS animation with no @keyframes in the file — drawn as a still").
- The 1× sound read head opens only where debt ≥ 0.05 s; a card, held frame or clip on no recording closes it, the dip half on each side of the join.

## 9. Where recordings overlap

Two cameras can film the same seconds ([05 F2.10](05-cut.md#f210-cameras-and-hearing)), and several sounds can run at once ([05 F2.5](05-cut.md#f25-hush-and-mix-what-the-preview-hears)). A zoom carries the camera row it was framed on and a volume its lane; every other effect has neither, and the clip it lands on decides. Each kept scene shows one camera (its lens) and hears the lanes its badges allow. The other cameras' own sound is never in the mix.

| effect | where it started (first rewrite: the prototype's render) | now |
|---|---|---|
| Zoom | the box is fractions of the scene's lens camera, per clip. After a lens switch the same fractions land on the other camera's frame ("effects stay"). Across a cut to another camera the box goes on in the new frame and jumps. Over inserts: none. | A zoom carries `cam`, the row it was framed on, and the render draws it only on clips shown from that camera, never over an insert. At the form's Apply a band reaching another camera's clip is trimmed there and logs "the zoom at m:ss was framed on camera N — it no longer applies here" (the status then reads Apply's), unless under 1 s would be left, when it is not trimmed. |
| Stop (rate 0) | the frame at `t` from the lens of the scene holding `t`, resolved at render time, so it follows a lens switch. A stop reaching into a clip from another camera is scaled to that clip. At a second the cut drops: no frame (skipped in the render, a failure in the preview). | Kept. A stop at a second the cut does not keep is refused at Apply, form left open: "there is no frame at m:ss — that second is not in the cut, so nothing can be held on it". |
| Speed, sound "" or pitch | the lens camera's sound and every heard lane are sped together (tempo held, or pitch shifted). | Kept. Apply logs "the preview plays this sound the way the render does" for "" and pitch. |
| Speed, sound own or scene | the lens camera's sound and every heard lane read at 1×, with a 0.15 s dip where they rejoin the picture. | Kept. The preview plays sound at picture speed here and says "sound at 1× in the render only". |
| Speed, sound mute; Stop | the render silences only the lens camera's sound: the hush runs before the lanes are mixed in, so separate recordings and extra tracks stay audible. The preview mutes everything. | MUST: silence the whole bed, as the preview does. |
| Volume | one gain on the bed (the lens camera's sound plus every heard lane), after the mix, before the narration. No way to reach one lane. Over a card or a freeze the preview applies it, the render does not. | A volume carries `lane`: "" the whole bed, or one lane's name, gained before the mix; the form fills in the lane when the volume's seconds lie inside exactly one. The render applies a whole-bed volume over a card's own sound, as the preview does. |
| Text, SVG | the output frame, after the camera: no camera, no lane. Over an insert or freeze: all or nothing, by the clip's start second. | Keep. |
| Label | read only by the model briefs ("MARKED"); nothing rendered. | Keep. |

Preview and render agree on two more things:

- **Which camera a zoom is framed on.** The preview frames on the watched row when one is watched; the render uses the scene's lens. While a zoom, speed, volume or text form is open, the preview shows the lens camera.
- **The output frame under aspect "source".** With cameras of different shapes, both use the first footage clip's frame (the preview falls back to the playing camera's shape when there is none).

Suggested zooms are always centred at height 0.6, and a proposed effect names no camera or lane, so a proposed volume hits the whole bed. With more than one camera row or lane the cut, speed and decoration briefs carry a line "SHOWN FROM: camera …, HEARD LANES: …" (the caption brief does not), and a suggested scene keeps a row it names that the session has, else row 0.

<!-- nav -->
---
[← 05 Cut](05-cut.md) · [↑ top](#06--effects) · [↑ Contents](README.md) · [07 Narrate →](07-narrate.md)
<!-- /nav -->
