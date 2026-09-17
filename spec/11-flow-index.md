# 11 — Flow index and UI flow map

## 1. The top-level flow

```
                 ┌──────────────────────────────────────────────────────────────────────┐
                 │  Settings (⚙): servers, models, ffmpeg, firefox — F0.13 tests          │
                 └──────────────────────────────────────────────────────────────────────┘
   open/new ──► ┌─────────┐   ▶    ┌─────────┐   ▶    ┌─────────┐   ▶    ┌─────────┐
   F0.6/8/9     │ Prepare │ ─────► │   Cut   │ ─────► │ Narrate │ ─────► │ Produce │ ──► video, subtitles,
                │  F1.x   │        │  F2.x   │        │  F4.x   │        │  F5.x   │     thumbnail, upload text
                └────┬────┘        └────┬────┘        └────┬────┘        └─────────┘
      sources, context,          timeline, cut.json,    narration.json,
      transcripts, frames,       effects (F3.x)         voice
      events, marks, final.txt
                 ▲                      ▲
                 └── edit final.txt ────┘  (F1.12: a hand edit re-cuts with no model)

   run bar: [▶ | chain ▾] [⏹] progress ─ one press runs the ticked steps in order (F0.4); ⏸ parks between subprocesses
   every model call: one gate, one exchange page, tools first (F6.x)
```

## 2. Tab states

```
 Prepare ── always open ──────────────────────────────────────────────────────────────
 Cut ────── locked until a source is footage ("Add footage on the Prepare step first…")
            preview modes: recording ▶ | cut ▶✂ | review ▶✂✂  (one lit, one ⏸)
            run-bar ▶ = Suggest until the preview started, then the preview's transport until ⏹
 Narrate ── open; ▶ refuses without a cut; greyed when narration is off
            run-bar ▶ = write+speak until the preview started
 Produce ── open; ▶ refuses without a cut
```

## 3. All flows

| id | flow | chapter |
|---|---|---|
| F0.1 | Switch tab | 03 |
| F0.2 | Press ▶ (run / pause / transport) | 03 |
| F0.3 | Press ⏹ | 03 |
| F0.4 | The chain | 03 |
| F0.5 | A run's bookkeeping (progress, checkpoints) | 03 |
| F0.6 | Open at start | 03 |
| F0.7 | Derive the editing policy from the context (new) | 03 |
| F0.8 | New project | 03 |
| F0.9 | Open a project | 03 |
| F0.10 | Save as | 03 |
| F0.11 | Rescan | 03 |
| F0.12 | Add sources | 03 |
| F0.13 | Settings tests | 03 |
| F1.1 | ▶ Prepare | 04 |
| F1.2 | Voice separation | 04 |
| F1.3 | Per source: audio, ASR, alignment, diarization, segments | 04 |
| F1.4 | ASR in chunks | 04 |
| F1.5 | Forced alignment | 04 |
| F1.6 | Frames | 04 |
| F1.7 | Describe | 04 |
| F1.8 | Fix the transcripts | 04 |
| F1.9 | Mark retakes (Gaming) | 04 |
| F1.10 | Repair the joins (Lecture) | 04 |
| F1.11 | Place the edges of a mark | 04 |
| F1.12 | Hand-edit final.txt | 04 |
| F1.13 | The session's word list (glue, dress, respell) | 04 |
| F2.1 | Play the recording | 05 |
| F2.2 | Play the cut | 05 |
| F2.3 | Review every cut | 05 |
| F2.4 | Place and step the line | 05 |
| F2.5 | Hush and mix | 05 |
| F2.6 | Select | 05 |
| F2.7 | Add, Split, Remove, ⌦ | 05 |
| F2.8 | Trim and move | 05 |
| F2.9 | Copy, Paste, Lane | 05 |
| F2.10 | Cameras and hearing | 05 |
| F2.11 | Folds and rows | 05 |
| F2.12 | Insert a card, still, video or sound | 05 |
| F2.13 | Undo, Redo, Revert, Clear | 05 |
| F2.14 | Suggest a cut | 05 |
| F3.1 | Zoom by hand | 06 |
| F3.2 | Aspect ratio | 06 |
| F3.3 | Speed and stop by hand | 06 |
| F3.4 | Text by hand | 06 |
| F3.5 | SVG by hand | 06 |
| F3.6 | Volume by hand | 06 |
| F3.7 | Label by hand | 06 |
| F3.8 | Hold, move, resize, edit, remove an effect | 06 |
| F3.9 | Captions proposed | 06 |
| F3.10 | Speeds proposed | 06 |
| F3.11 | Decorations proposed | 06 |
| F3.12 | Clamp effects to the cut | 06 |
| F4.1 | ▶ Write and speak | 07 |
| F4.2 | The narration call | 07 |
| F4.3 | Fit a line to its clip | 07 |
| F4.4 | Speak a line (TTS) | 07 |
| F4.5 | Preview the cut with narration | 07 |
| F4.6 | Choose the voice and build the reference | 07 |
| F4.7 | Edit lines | 07 |
| F4.8 | Narration off | 07 |
| F5.1 | ▶ Produce | 08 |
| F5.2 | The render | 08 |
| F5.3 | What "up to date" means | 08 |
| F5.4 | Subtitles and translation | 08 |
| F5.5 | The `<video>` tag | 08 |
| F5.6 | Upload text and thumbnail | 08 |
| F5.7 | Page runs (redraw, suggest, set thumbnail, words, export, save, transcode) | 08 |
| F6.1 | Tool protocol | 09 §2 |
| F6.2 | Retries | 09 §3 |
| F6.3 | Liveness and the gate | 09 §4 |

## 4. Model calls at a glance

```
 Prepare   describe ×(frames/4)   fix ×(lines/25)   retake ×3 | textedit ×(joins)        [cache]
 Cut       cut ×1 (+web)  captions ×(clips/5)  speed ×1  effects ×1                       (gaming only)
 Narrate   narrate ×1 (+web)                                             TTS ×(lines)
 Produce   youtube ×1 (+web)                sd.cpp ×1 (when drawn)      translate ×(languages × batches)
 Setup     policy ×1 (new, when the context changes)
```

## 5. Where each kind of decision lives (directive A)
- **What the video becomes** (lengths, thresholds, budgets): editing policy (`10-parameters.md` §2), derived from the User Context, editable.
- **How a job is worded**: the prompts (`prompts/`).
- **Which servers and binaries**: machine settings.
- **How the app looks, waits and caches**: engineering constants.
