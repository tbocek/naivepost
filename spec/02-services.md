# 02 — Services and the tool catalogue

<!-- nav -->
[← 01 The project on disk](01-project-and-files.md) · [↑ Contents](README.md) · [03 The shell →](03-shell.md)

**In depth:** [3.1 Shared](#31-shared) · [3.2 Describe (per chunk of frames)](#32-describe-per-chunk-of-frames) · [3.3 Fix (per block of transcript lines)](#33-fix-per-block-of-transcript-lines) · [3.4 Retake marking](#34-retake-marking) · [3.5 Text edit (per join)](#35-text-edit-per-join) · [3.6 Cut (model-chosen)](#36-cut-model-chosen) · [3.7 Captions / speed / decorations (per clip)](#37-captions--speed--decorations-per-clip) · [3.8 Narration](#38-narration) · [3.9 Upload text and thumbnail](#39-upload-text-and-thumbnail) · [3.10 Translate (per batch of numbered lines)](#310-translate-per-batch-of-numbered-lines)
<!-- /nav -->

## 1. The four servers

Every request to any of them — and every web search and page read — is timed and written to the project's `requests.tsv` (REVIEW, new; [09 §10](09-llm-and-tools.md#10-every-request-timed-review--new)); each model is given at most its Settings slot count of requests at once ([09 §4](09-llm-and-tools.md#4-liveness-and-the-gate-f63)).

| what | default | serves | API |
|---|---|---|---|
| OpenAI-compatible LLM | `http://127.0.0.1:8731` | every text and vision job | `POST /v1/chat/completions` (streaming, tools), `GET /v1/models` |
| audio.cpp | `http://127.0.0.1:8765` (or, below the box, `NAIVEPOST_TTS_URL` then `AUDIOCPP_SERVER`) | speech to text, alignment, diarization, voice separation, text to speech | `GET /health`, `GET /v1/models` (id, family, task), `POST /v1/ui/upload` (raw body, `X-Audiocpp-Filename`), `POST /v1/tasks/run` `{"model","request"}`, `POST /v1/audio/speech`, `POST /v1/tasks/unload_all_models` |
| sd.cpp (sd-server) | `http://127.0.0.1:1234` (or `SD_SERVER` below the settings box) | the thumbnail when one is drawn | `GET /sdcpp/v1/capabilities`, `POST /sdcpp/v1/img_gen` → job id, `GET /sdcpp/v1/jobs/{id}`, `POST /sdcpp/v1/jobs/{id}/cancel` |
| ffmpeg / ffprobe | on PATH (or a configured path; ffprobe from the same folder) | frames, waveforms, cuts, encodes | subprocesses |

An empty server box means the loopback default. A host with no scheme is read as `https://host`. Keys are sent as `Authorization: Bearer …` only when non-empty. The settings file is re-read per request (each URL, key and model id), so a settings change lands on the next request; the LLM server has no env override.

Audio model ids (settings, with defaults): ASR `nemotron-asr`, diarization `sortformer-diar`, TTS `index-tts2`, separation `bs-roformer`, aligner: none by default (any model declared task `align`, preferring `qwen3-aligner`). A missing model is reported with a compiled-in install hint ("the weights install with: docker compose exec audio python3 tools/model_manager_v2.py install <package> --models-root models", package per shipped default: nemotron_asr_q8_0, sortformer_diar_4spk_v1_q8_0, bs_roformer_q8_0) — offered only when the configured id is the shipped default; for a hand-picked id the weights would be a guess. Compiled-in defaults: the four model ids, the aligner preference and the dev-box voices path; no call leaves the machine unless a box points elsewhere. An aligner is chosen by task, not id: the Settings verdict names which of several the run tries first.

Uploads, task runs and the sd.cpp submit/poll ride the run's cancel context, so ⏹ aborts them. Prototype: `/health`, `/v1/models`, `/v1/audio/speech` (a synthesis in flight cannot be stopped), `unload_all_models` and the sd.cpp cancel do not — the rewrite SHOULD put every request on the run context. Audio task calls have no client timeout (an hour of audio is an hour of work), except `unload_all_models` at 20 s (housekeeping), called quietly and best-effort at the end of every run, even one that used no audio. LLM calls have a silence rule (streamed) or a whole-call ceiling ([`09-llm-and-tools.md` §4](09-llm-and-tools.md#4-liveness-and-the-gate-f63)).

## 2. Model roles (which job asks which server)

| job | model | thinking | answers with |
|---|---|---|---|
| speech to text | ASR (audio.cpp) | – | text per chunk (≤ 60 s for qwen3-family, else ≤ 300 s, halved on an out-of-memory answer down to 20 s) |
| word times | aligner (audio.cpp) | – | start/end per word, the chunk's own words in hand |
| who spoke when | diarization (audio.cpp) | – | speaker turns per window (90 → 45 → 25 s ladder) |
| voice split | separation (audio.cpp) | – | voice and rest stems |
| what is on screen | LLM with vision | off | one EVENT per frame + a running STATE, four frames a call |
| clean transcript | LLM | off | the same lines respelled; times and speakers unchanged, enforced |
| joins (markingPass joins) | LLM | on | the two takes run on as one; only deletions accepted |
| retakes (markingPass retakes) | LLM | off | line numbers of abandoned stretches, three runs pooled |
| model cut (cutMode model) | LLM (+ web tools) | on | segments in session seconds copied off the timeline; three attempts, web tools withdrawn after the first rejected one, thinking off for a retry when the model reasoned and wrote nothing |
| captions, speed, effects | LLM | off | per clip, in the clip's own seconds |
| narration | LLM (+ web tools) → TTS | on | a line per clip with an emotion; spoken in the cloned voice |
| upload text | LLM (+ web tools) | on | title, thumbnail (frame or instruction), description |
| thumbnail | sd.cpp | – | an image edited from real frames; only when an instruction exists and no frame was named |
| subtitles in other languages | LLM | off | numbered lines translated, one call per language |

## 3. Tool catalogue (rewrite directive B)

**Prototype:** of everything below only `web_search` and `web_read` exist, offered to the cut, the narration and the upload text; every other job answers with one JSON reply, parsed and re-asked on failure ([`09-llm-and-tools.md` §2](09-llm-and-tools.md#2-tool-protocol-f61)). The rest is the rewrite. Tools are offered per job. Each result is a short JSON object; problems come back as `{"error": "<one sentence the model can act on>"}` and never end the flow. Numbers the model must not compute are always identifiers from the request (line, clip, frame numbers; offsets as stamped).

### 3.1 Shared

| tool | args | result |
|---|---|---|
| `get_context` | – | the User Context and the policy fields relevant to this job |
| `get_lines` | `from`, `to` (line numbers of the session timeline) | the stamped lines |
| `get_events` | `from`, `to` (session seconds) | EVENT lines in the range |
| `web_search` | `broad`, `medium`, `narrow` | up to 8 hits with snippets (only where the prototype offered it: cut, narrate, upload text); 45 s per call |
| `web_read` | `url` | page text (≤ 6000 bytes); 45 s per call |
| `get_frames` | `clip` or `from`, `to` (session seconds) | that stretch's frames as images, stamped as the request stamps them |
| `finish` | – | ends the job; answers with the job's own check list (below) |

A call may run P.eng.llmToolRounds (8) tool-call rounds; beyond that the step gets no answer from it.

### 3.2 Describe (per chunk of frames)

Reads: `get_frames`, `get_events`, `get_context`, `speech_around`.

| tool | args | result |
|---|---|---|
| `record_event` | `frame` (1..n as stamped), `text`, `calm` (bool) | ok, naming the frame the event landed on and its session second; a frame recorded twice replaces; error when `frame` is not in this batch |
| `set_state` | `text` | ok; empty text clears the state rather than leaving the old one |
| `speech_around` | `from`, `to` (session seconds) | every line spoken in that stretch, from every recording — not the brief's two-per-side extract |

`finish` answers with the frames still without an event, plus the batch's first frame if left "same" with no history to be the same as. Prototype, all invisible to the model: the stamps come back as prose; each line is snapped to the nearest frame within half an interval and **dropped without a word when further off**; an unparseable offset is dropped silently; a reply with no label at all is filed as the description; a multi-line answer is flattened to one; the batch's first frame is force-written as "Calm; same view." when the model said "same".

### 3.3 Fix (per block of transcript lines)

| tool | args | result |
|---|---|---|
| `fix_line` | `n` (line number in the block), `text` | ok, or error when `n` is not in this block or the text has a tab |
| `get_lines` | `from`, `to` | the lines around the block from this and every other source, beyond the brief's ±P.policy.fixContextSeconds |
| `flag_line` | `n`, `why` | ok — the line stands, the note is logged: the channel the prototype lacks, where a wrong speaker or mis-timed row is "not yours to fix" with nowhere to say so |

Times and speakers are not arguments: the tool cannot change them — the enforcement the prototype did by comparison. A line no `fix_line` names keeps its ASR text. That is the point: the prototype threw away **the whole block of 25 lines** when one row came back with a changed time, speaker or lost tab, dropped reply lines with fewer than four tab fields before counting, then re-asked the identical question once more without saying what was wrong.

### 3.4 Retake marking

| tool | args | result |
|---|---|---|
| `mark_abandoned` | `from`, `to`, `again` (line numbers; `again` 0 = never picked up) | **ok with the stretch that will actually be removed** — the seconds left after trimming to the words the later take repeats and placing the edges on the audio — plus the running share of the session's speech now marked; or the refusal ("lines a-b say they are said again at line c, which is inside them -- not a mark", out of range, farther than P.policy.retakeReachSeconds, shorter than P.policy.retakeMinSeconds) |
| `unmark` | `from`, `to` | ok |
| `get_lines` | `from`, `to` | the lines again, with the pause before each in seconds — including those under P.policy.retakePauseSeconds, which the brief omits |

`finish` answers with the total marked against P.policy.retakeCeil, so a model over the ceiling can take some back. Prototype: three identical calls are pooled and deduped; each mark is trimmed to the repeated tail by a fuzzy matcher; a whole take gets `again` rewritten to 0; a rephrase is trimmed to the broken-off fragment; a refused mark is re-heard by a second ASR pass and sometimes resurrected; overlapping marks are merged; over the ceiling **every mark from all three runs is thrown away**. The model is told none of it.

### 3.5 Text edit (per join)

| tool | args | result |
|---|---|---|
| `drop_words` | `side` (before/after), `count` | ok with the exact words that would go and the sentence left reading across the join; error when the stretch does not touch the join, `count` runs past what was shown, or it would lose more than P.policy.seamMaxWords or P.policy.seamCeil |
| `keep_join` | – | nothing removed here (a whole answer) |
| `get_words` | `side`, `count` | more words than the brief's P.policy.seamReachWords, on either side |

Prototype: the model returned the joined text; the app derived the counts by matching backwards. With tools the counts are stated directly; the app still re-derives the deleted stretch to check it is one stretch at the join.

### 3.6 Cut (model-chosen)

| tool | args | result |
|---|---|---|
| `add_segment` | `start`, `end` (session seconds copied off lines), `why` | ok with **what the segment became**: the snapped edges and how far each moved, the seconds a mark will take out of it, whether it survives P.policy.minSceneSeconds, and the running footage total against the target window — or the error (no footage under an edge, so the segment would be dropped; past the end, with the mm:ss reading of the model's own number; ends before start; overlaps an added segment) |
| `remove_segment` | `start` | ok |
| `set_speed` | `start`, `rate` | ok with the rate as applied (P.policy.minRate…maxRate, lowered where the clip would otherwise render under P.policy.minClipSeconds) and the resulting on-screen length |
| `cut_status` | – | kept footage so far, the target window, segment count against its floor and ceiling, and the marks and dead air still to come out — the reading `finish_cut` would give, at any time |
| `finish_cut` | – | the whole-cut checks (count within [min, max], footage within the target window) as a problem list, worst first, or ok |

### 3.7 Captions / speed / decorations (per clip)

| tool | args | result |
|---|---|---|
| `add_caption` | `clip`, `start`, `end` (offsets), `text` | ok with the caption as placed — clamped into the clip, with its fades — or error (clip not in this batch; under P.policy.captionMinSeconds; no words) |
| `set_clip_speed` | `clip`, `rate` | ok with the rate as applied and the on-screen length; error when the clip has a caption and the rate is over 1 — the prompt's own rule, enforced again: it MUST be said, not silently dropped |
| `add_effect` | `clip`, `kind` (zoom/stop/volume), `start`, `end`, `gain?`, `box?` | ok with the effect as placed (clamped into the clip, the app's fades, for a zoom the box used); error when the kind is not one of the three, the span is empty, or a volume gain of 1 would do nothing |
| `get_frames` | `clip`, `at?` | the clip's frames, so a zoom can be aimed — the prototype shows this pass no pictures, which is why every zoom it proposes is a centred punch-in |

### 3.8 Narration

| tool | args | result |
|---|---|---|
| `write_line` | `clip`, `at` (offset), `text`, `emotion`, `pos?` | ok with **the line as it will play**: where it lands after packing behind the line above (lead, gap, tail), how many words fit against the ceiling, whether the clip will be grown, the schedule slid earlier or the speech sped up to make room — and the emotion resolved into its eight weights; error when the clip is not one given, `pos` is not top/center/bottom, or the emotion is not in the vocabulary |
| `leave_silent` | `clip` | ok |
| `list_emotions` | – | the eight bases with their kin words and the named blends with recipes — the table the prototype keeps to itself |
| `get_lines` | `from`, `to` | the transcript around a clip, beyond the brief's ±P.policy.narrationContextSeconds |
| `describe_insert` | `clip` | what is actually on an inserted card: its text, parameters, length. The prototype passes only the **file name**, so "tier.svg?S=Dust II" is all the writer knows about a full-screen graphic |

`finish` names the clips with no answer and those whose lines will not fit, so they can be rewritten shorter instead of squeezed by the render. Prototype: one JSON answer matched to clips by echoed bounds within 0.5 s, scanning forward; **a single clip skipped, a single echo more than half a second out, or entries out of clip order rejects the whole reply**, three times, then the run fails with nothing written; `pos` outside five known words silently becomes bottom; entries are re-sorted by (clip, offset) though out-of-order clips were just rejected; and the user's hand-made "this clip plays its own audio" list is wiped every run.

### 3.9 Upload text and thumbnail

| tool | args | result |
|---|---|---|
| `set_title` | `text` | ok with the words as printed across the picture and the size they take — or the warning they will overflow the band (over seven words) |
| `set_description` | `text` | ok |
| `pick_frame` | `clip`, `offset` | ok with **the frame actually taken and how far it is from the moment asked for** (frames exist only every P.project.frameInterval seconds; the prototype takes the nearest at any distance), and a note that a picked frame means nothing is drawn |
| `set_thumbnail_instruction` | `text`, `negative?` | ok; error when a frame was already picked — instead of the prototype silently clearing one with the other |
| `get_frames` | `clip`, `at?` | the candidate frames, so the picture is chosen by looking, not position — the prototype picks the middles of three equal bands and shows the writer none |

`finish` requires a title and a description. Prototype: the three labelled lines are peeled off the front of prose (at most three, quotes stripped, a first description line under 40 characters ending in a colon dropped); a thumbnail line folding a `frame:` into itself clears the instruction unconditionally; a frame naming a clip not in the cut leaves neither frame nor instruction; a missing title or instruction silently keeps the previous run's.

### 3.10 Translate (per batch of numbered lines)

| tool | args | result |
|---|---|---|
| `translate_line` | `n`, `text` | ok with the line as wrapped into its cue, or error (no such line number in this batch; empty text) |

`finish` lists the numbers still missing; the app re-asks once for those, then ships the original text for any still missing — a track is never dropped for one bad line, but the log and `finish` name the lines still in the other language. Two prototype behaviours the rewrite MUST fix, not keep: the cue's placement tag (`{\an8}`) is sent into the translation as a word and never checked on return, so a model dropping it loses that caption's placement; and the model's line breaks are re-wrapped at P.policy.subtitleRowChars on return, contradicting the prompt asking to keep them.

### 3.11 Policy derivation (once per project, F0.7)

| tool | args | result |
|---|---|---|
| `set_policy` | `field`, `value`, `because` | ok or error (unknown field, out of range) |

The model reads the User Context and sets only the fields it speaks to ("clips under 2 s", "about 12 minutes", "keep the swearing"); every other field keeps its default. The pipeline fields are among them: markingPass ("I read from a script, stumbled and started again" → joins; "a gaming session, cut out the dead stretches" → retakes), cutMode, captionsPass, speedPass, decorationsPass. They decide which flows and jobs run at all. The form shows the `because`.

## 4. Degraded modes

- No aligner: cut points come off the waveform ("clean wherever there is a silence to cut in, and unable to cut between two words of one breath").
- No firefox, or inside Flatpak: no web tools; the model writes only what the material says.
- sd.cpp down: the render completes; the thumbnail half fails and is logged. A job the server forgot (404/410) is reported as "sd.cpp forgot job <id> (<status>) -- it was probably restarted mid-draw"; a server reporting no `supported_modes` is assumed able to draw; the Settings test falls back to `/v1/models` to name an OpenAI-shaped tenant of the port.
- An LLM server refusing `tools` (an error on the first round that is not a stop; a `tools` error on a later round fails the call): the job runs as one JSON answer with the prototype's parsers and retry turns.
- Flatpak: no web tools; no desktop entry, icon or MIME package written (the manifest exports them); voices folder in the sandbox's data dir.
- A reply cut off at the token limit: with tools, everything already called stands; without, the prototype's "answer again with far fewer items" retry.

## 5. Details confirmed against the code (verification pass)

- **Request bodies.** `/v1/tasks/run`: ASR `{"audio","language"}`, diarization `{"audio"}`, alignment `{"audio","text","language"}` — `audio` is always the server-side path an upload returned, never local. `/v1/audio/speech`: `{"model","input","voice_ref","language","options"}` with `voice_ref` the uploaded path of `narrate/voice_ref.wav` (re-uploaded before every line: a remembered path dies with a server restart), `options` the emotion vector plus `seed`, `language` hard-coded "en" (REVIEW: the project language, as ASR and alignment send). `img_gen`: `prompt`, `negative_prompt?`, `width?`, `height?`, `seed` (always), `ref_images` (data URLs — an edit model is conditioned on references, not `init_image`), `auto_resize_ref_image` (always), `output_format?`; everything else deliberately left to the server's start-up flags.
- **sd.cpp waits.** Capabilities 15 s; submit 60 s; each poll 30 s at a 1 s interval; cancel 10 s.
- **Settings tests** (ten buttons and "Test All", each with its own verdict): LLM — one completion, `max_tokens` 16, thinking forced off, 60 s; LLM vision — a generated 48 px red square as a data URL, 120 s, passes only if the reply contains "red"; TTS endpoint — `/health` (15 s) then the catalogue, looking for family `index_tts2` or task `clon`; TTS model, ASR, diarization, separation — one id each against `/v1/models`, checking the declared task; aligner — by task, never by id (a box could only disagree with the server); ffmpeg — `LookPath`, `ffprobe` beside it, then `-filters`/`-encoders` against `rubberband, subtitles, loudnorm, atempo, amix, adelay, alimiter` and `libx264, libx265, aac, libopus`; firefox — `--version`, then a real headless search. `GET /v1/models` on the LLM backs only "Fetch models" (the id has no default and must be one the server lists) and the sd.cpp fallback probe.
- **No Save in Settings.** The file is written 600 ms after the last keystroke and again on close if a write is owed; a save resets the cached TTS model id and the "already listening on" note.

<!-- nav -->
---
[← 01 The project on disk](01-project-and-files.md) · [↑ top](#02--services-and-the-tool-catalogue) · [↑ Contents](README.md) · [03 The shell →](03-shell.md)
<!-- /nav -->
