# 00 — Principles and rewrite directives

<!-- nav -->
[← Contents](README.md) · [01 The project on disk →](01-project-and-files.md)
<!-- /nav -->

## 1. What the application is

A desktop editor for already-recorded sessions: camera footage, separate microphone recordings, screen captures. It transcribes them, describes the screen, proposes a cut, writes and speaks a narration, renders the video, and writes upload text, subtitles and thumbnail. Every model output is a proposal the user can overrule on screen or in a file.

**Linear on purpose**: the timeline is the recording in shooting order; the edit is which stretches stay. Cards, stills and sounds can be spliced in and a moment shown from another camera; nothing is rearranged.

Four workflows, in order, one tab and one ▶ each: **Prepare → Cut → Narrate → Produce**. Several can be ticked and run in one press.

## 2. Rules the prototype holds to (keep them)

1. **Models propose; the machine places.** A model is never asked for a timestamp it would have to compute. It answers in words, line numbers, clip numbers and offsets copied off the request; the app converts these to seconds via the aligner's word times.
2. **Every cut lands on a word edge.** The audio envelope picks where between two known words a splice falls, never which words it touches.
3. **Nothing is deleted, only marked.** The transcript keeps every word; retake marks are a user-editable file; `final.txt` is the video's text — deleting a word there removes it from the video.
4. **A step's output is its resume marker.** Every stage skips when its output is on disk; markers are written last and whole; a stopped run resumes.
5. **One box, every job.** What the user tells the editor is written once in the User Context and reaches every model call.
6. **Every model reply is walked back onto the timeline** (kept footage only, snapped edges, marked stretches removed, effects clamped) before the page sees it.
7. **Preview equals render.** The same functions compute camera path, text fitting, fades, sound ducking and clip clock for preview and ffmpeg.
8. **Observability never breaks the work.** Every model exchange is written as a readable page; a failure to record is a log line, never a failed step.
9. **The GUI thread owns the widgets.** Runners read snapshots and caches; a fence test keeps pipeline code from reaching a widget.
10. **Measured, not assumed.** Changes to how a model is asked are scored against a hand-made cut, the unchanged prompt as control (several such measurements are in the prototype's comments; numbers in [`10-parameters.md`](10-parameters.md)).

## 3. Rewrite directive A — no implicit behaviour

The prototype encodes hundreds of decisions as constants: a file under 2 s is silence, a clip under 0.5 s is not rendered, a caption under 0.3 s is dropped, three retake runs are pooled, a join shows 140 words each side. [`10-parameters.md`](10-parameters.md) lists them all. In the rewrite each MUST have one of these homes:

| Home | Meaning | Examples |
|---|---|---|
| **Machine setting** | Settings dialog / settings file; about this computer | server URLs, model ids, ffmpeg path, voices folder |
| **Project setting** | Stored in the project file; shown on a tab | frame interval, language, style, encoder settings |
| **Editing policy** | Stored in the project; derived from the User Context by a model call, else defaults; editable as a form | minimum take length, dead-air threshold, review pad, words per caption row, target length |
| **Prompt** | The twelve prompts, per machine, resettable | how a job is worded |
| **Engineering constant** | Fixed in code: about the machine, not the edit (pixel sizes, timeouts, cache formats) | 6 px grab reach, 100 ms tick, cache magic |

The **editing policy** is new: a JSON object in the project (`policy` in `naivepost.json`) holding every value that decides what the video becomes. Fields and defaults: [`10-parameters.md` §2](10-parameters.md#2-editing-policy-project-derived-from-the-user-context-by-f07-else-defaults). Filled three ways, by precedence: (1) a value the user typed into the policy form; (2) a value a model derived from the User Context via `tool:set_policy` (flow [F0.7](03-shell.md#f07-derive-the-editing-policy-review--new) in [`03-shell.md`](03-shell.md)); (3) the shipped default. The form shows each value's origin.

Rule of thumb: if changing the number could change which frames or words end up in the video, it is policy; if it only changes how the app looks, waits or caches, it is an engineering constant.

## 4. Rewrite directive B — tools first

The prototype asks for a whole answer as strict JSON, parses and validates it, and on failure sends "Your answer failed validation: … Return corrected strict JSON only." The rewrite MUST instead give each job a small set of **tools** and let the model finish the flow by calling them. The app validates inside the tool and returns the problem as the tool result: correction happens in the same conversation, one item at a time, and a partial answer survives the model running out of tokens.

Design rules for tools:

- Every job has a `finish` tool (or a named terminal tool such as `finish_cut`) ending the flow; a job that stops calling tools without finishing is asked once to finish, then treated as done with what it produced.
- Reading tools hand out material in pieces (`get_lines(from, to)`, `get_frames(clip)`) so a 64-minute session is not one 450 kB prompt (context budgets: [`09-llm-and-tools.md` §5](09-llm-and-tools.md#5-context-budgets-review--new)). Every job handed an extract — a window of words, two lines of context a side, four frames — MUST also have the tool fetching more, or the window silently bounds what the model can get right.
- Writing tools carry the prototype parsers' validation; the result is `ok` plus the normalised item, or a one-sentence actionable problem.
- **A tool result says what the app made of the item**, not merely that it was accepted: the snapped edges and how far they moved, the seconds a mark really takes out, the rate after clamping, the caption after being pulled inside its clip. Wherever the prototype changes an answer behind the model's back, the rewrite returns that change as the tool's answer or, if it comes later, reports it at `finish`. Audit of every such place: [`12-decisions.md`](12-decisions.md).
- **Nothing is dropped in silence.** The prototype sometimes skips a failing item without a word, sometimes kills the whole reply over it, and sometimes throws a whole answer away over a ceiling. In the rewrite an unusable item comes back as that item's error; a ceiling is a reading the model can ask for (`cut_status`) and is told at `finish` — so it can take one mark back instead of losing thirty.
- Tools never take a time the model would have to compute: they take line, clip and frame numbers and offsets "as stamped on the request".
- The final walk-back onto the timeline (rule 2.6) still runs after `finish`: tools validate items; the walk-back reconciles the whole.
- The exchange log records every tool call and result.
- A server refusing a `tools` field gets the prototype's fallback: the same job as one JSON answer (a documented degraded mode, not the design).

[`02-services.md` §3](02-services.md#3-tool-catalogue-rewrite-directive-b) lists the tool catalogue per job; each tab chapter names its flows' tools; [`12-decisions.md`](12-decisions.md) audits, per job, which decisions the model makes and which the app keeps.

## 5. Generalisations proposed (REVIEW)

- **Styles** become a list of named pipelines, not two hard-coded names: each says which marking pass runs (joins / retakes / none), whether the cut is text-derived or model-chosen, and which policy defaults apply. Shipped: Lecture and Gaming.
- **Languages** for subtitles become a settings list (code, ISO-639-2 tag, name), not three constants.
- **Cards** (SVG inserts) stay a folder of files with a declared-inputs convention; the two built-ins are shipped files, not code.
- **Narrator slots** stay four; the number is a policy default.
- **Frame-interval stops, scale presets, encoder option lists** stay shipped lists, in one table the settings can extend.

## 6. Glossary

- **Session**: all recordings of one sitting, on one clock (the *session clock*, seconds from the earliest timestamped file).
- **Source**: one session file; *footage* (frames come out of it, it can be cut) and/or a *recording* (heard); one source may hold *narrator slot* 1–4.
- **Row / camera**: a horizontal timeline band of non-overlapping recordings; a scene names the row its picture comes from.
- **Lane**: the waveform of one audio track (a recording, or a further track of a capture).
- **Scene / clip / segment**: a kept stretch `[s, e)` of session time on one row; a *spliced insert* is a card with `s == e` and its own duration; an *overwriting insert* replaces footage seconds.
- **Cut**: the list of scenes plus effects, aspect, corrections and lanes (`cut/cut.json`).
- **Join / seam**: where one kept scene ends and the next begins; in a lecture, also where one recording stopped and the next started.
- **Retake / mark**: a stretch that was said, broken off and said again; marked, never deleted.
- **Effect**: zoom, speed (incl. stop), text, svg, volume, label; spans `[t, t+dur)` of session time.
- **Line**: one narration entry over one clip (`narrate/narration.json`).
- **Policy**: the project's editing parameters ([§3](#3-rewrite-directive-a--no-implicit-behaviour)).
- **Run**: one press of ▶: a chain of steps with one progress bar, one log page, one cancel.

<!-- nav -->
---
[← Contents](README.md) · [↑ top](#00--principles-and-rewrite-directives) · [01 The project on disk →](01-project-and-files.md)
<!-- /nav -->
