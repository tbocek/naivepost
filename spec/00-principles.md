# 00 — Principles and rewrite directives

## 1. What the application is

A desktop editor for sessions that were already recorded: camera footage, separate microphone recordings, screen captures. It transcribes them, describes what was on screen, proposes a cut, writes and speaks a narration, renders the video and writes the upload text, subtitles and thumbnail. Every model output is a proposal the user can overrule on screen or in a file.

It is **linear on purpose**: the timeline is the recording in the order it was shot; the edit is which stretches stay. Cards, stills and sounds can be spliced in and a moment can be shown from another camera, but nothing is rearranged.

Four workflows, in order, each one tab with one ▶: **Prepare → Cut → Narrate → Produce**. Several can be ticked and run in one press.

## 2. Rules the prototype holds to (keep them)

1. **Models propose; the machine places.** A model is never asked for a timestamp it would have to compute. It answers in words, line numbers, clip numbers and offsets copied off the request, and the app turns that into seconds using the aligner's word times.
2. **Every cut lands on a word edge.** The audio envelope chooses where between two known words a splice falls, never which words it touches.
3. **Nothing is deleted, only marked.** The transcript keeps every word; retake marks are a file the user can read and edit; `final.txt` is the text of the video and deleting a word there removes it from the video.
4. **A step's output is its resume marker.** Every stage skips when its output is on disk; markers are written last and whole; a stopped run resumes.
5. **One box, every job.** What the user tells the editor is written once in the User Context and reaches every model call.
6. **Every model reply is walked back onto the timeline** (kept footage only, snapped edges, marked stretches removed, effects clamped) before the page sees it.
7. **Preview equals render.** The same functions compute the camera path, the text fitting, the fades, the sound ducking and the clip clock for the preview and for ffmpeg.
8. **Observability never breaks the work.** Every model exchange is written as a readable page; a failure to record is a log line, never a failed step.
9. **The GUI thread owns the widgets.** Runners read snapshots and caches; a fence test keeps pipeline code from reaching a widget.
10. **Measured, not assumed.** Changes to how a model is asked are scored against a hand-made cut with the unchanged prompt as control (the prototype's comments record several such measurements; the numbers are kept in `10-parameters.md`).

## 3. Rewrite directive A — no implicit behaviour

The prototype encodes hundreds of decisions as constants: a file under 2 s is silence, a clip under 0.5 s is not rendered, a caption under 0.3 s is dropped, three retake runs are pooled, a join shows 140 words each side. `10-parameters.md` lists all of them. In the rewrite each MUST have one of these homes:

| Home | Meaning | Examples |
|---|---|---|
| **Machine setting** | Settings dialog / settings file; about this computer | server URLs, model ids, ffmpeg path, voices folder |
| **Project setting** | Stored in the project file; shown on a tab | frame interval, language, style, encoder settings |
| **Editing policy** | Stored in the project; derived from the User Context by a model call, else defaults; editable as a form | minimum take length, dead-air threshold, review pad, words per caption row, target length |
| **Prompt** | The twelve prompts, per machine, resettable | how a job is worded |
| **Engineering constant** | Fixed in code because it is about the machine, not the edit (pixel sizes, timeouts, cache formats) | 6 px grab reach, 100 ms tick, cache magic |

The **editing policy** is new. It is a JSON object in the project (`policy` in `naivepost.json`) holding every value that decides what the video becomes. Its fields and defaults are in `10-parameters.md` §2. It is filled in three ways, in this order of precedence: (1) a value the user typed into the policy form; (2) a value a model derived from the User Context via `tool:set_policy` (flow F0.7 in `03-shell.md`); (3) the shipped default. The form shows where each value came from.

Rule of thumb for classifying: if changing the number could change which frames or words end up in the video, it is policy; if it only changes how the app looks, waits or caches, it is an engineering constant.

## 4. Rewrite directive B — tools first

The prototype asks the model for a whole answer as strict JSON, parses it, validates it, and on failure sends "Your answer failed validation: … Return corrected strict JSON only." The rewrite MUST instead offer each job a small set of **tools** and let the model finish the flow by calling them. The app validates inside the tool and returns the problem as the tool result, so correction happens in the same conversation, one item at a time, and a partial answer is never lost when the model runs out of tokens.

Design rules for tools:

- Every job has a `finish` tool (or a named terminal tool such as `finish_cut`) that ends the flow; a job that stops calling tools without finishing is asked once to finish, then treated as done with what it produced.
- Reading tools give the model the material in pieces (`get_lines(from, to)`, `get_frames(clip)`) so a 64-minute session is not one 450 kB prompt (`09-llm-and-tools.md` §5 on context budgets).
- Writing tools carry the same validation the prototype's parsers did; the result is either `ok` plus the normalised item, or a one-sentence problem the model can act on.
- Tools never take a time the model would have to compute: they take line numbers, clip numbers, frame numbers and offsets "as stamped on the request".
- The final walk-back onto the timeline (rule 2.6) still runs after `finish`: tools validate items; the walk-back reconciles the whole.
- The exchange log records every tool call and result.
- A server that refuses a `tools` field gets the prototype's fallback: the same job as one JSON answer (kept as a documented degraded mode, not the design).

`02-services.md` §3 lists the tool catalogue per job; each tab chapter names which tools its flows use.

## 5. Generalisations proposed (REVIEW)

- **Styles** become a list of named pipelines, not two hard-coded names: each style says which marking pass runs (joins / retakes / none), whether the cut is text-derived or model-chosen, and which policy defaults apply. Lecture and Gaming are the shipped two.
- **Languages** for subtitles become a list in the settings (code, ISO-639-2 tag, name) instead of three constants.
- **Cards** (SVG inserts) stay a folder of files with a declared-inputs convention; the two built-ins are shipped files, not code.
- **Narrator slots** stay four but the number is a policy default.
- **Frame-interval stops, scale presets, encoder option lists** stay shipped lists but live in one table the settings can extend.

## 6. Glossary

- **Session**: all recordings of one sitting, on one clock (the *session clock*, seconds from the earliest timestamped file).
- **Source**: one file in the session; it may be *footage* (frames come out of it, it can be cut) and/or a *recording* (heard); one source may hold *narrator slot* 1–4.
- **Row / camera**: a horizontal band of the timeline holding recordings that do not overlap; a scene names the row its picture comes from.
- **Lane**: a waveform of one audio track (a recording, or a further track of a capture).
- **Scene / clip / segment**: a kept stretch `[s, e)` of session time on one row; a *spliced insert* is a card with `s == e` and its own duration; an *overwriting insert* replaces footage seconds.
- **Cut**: the list of scenes plus effects, aspect, corrections and lanes (`cut/cut.json`).
- **Join / seam**: where one kept scene ends and the next begins; in a lecture, also where one recording stopped and the next started.
- **Retake / mark**: a stretch that was said, broken off and said again; marked, never deleted.
- **Effect**: zoom, speed (incl. stop), text, svg, volume, label; spans `[t, t+dur)` of session time.
- **Line**: one narration entry over one clip (`narrate/narration.json`).
- **Policy**: the project's editing parameters (§3).
- **Run**: one press of ▶: a chain of steps with one progress bar, one log page, one cancel.
