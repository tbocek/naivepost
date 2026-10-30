# 02 — Services and the tool catalogue

<!-- nav -->
[← 01 The project on disk](01-project-and-files.md) · [↑ Contents](README.md) · [03 The shell →](03-shell.md)

**In depth:** [3.1 Shared](#31-shared) · [3.2 Describe (per chunk of frames)](#32-describe-per-chunk-of-frames) · [3.3 Fix (per block of transcript lines)](#33-fix-per-block-of-transcript-lines) · [3.4 Retake marking](#34-retake-marking) · [3.5 Text edit (per join)](#35-text-edit-per-join) · [3.6 Cut (model-chosen)](#36-cut-model-chosen) · [3.7 Captions / speed / decorations (per clip)](#37-captions--speed--decorations-per-clip) · [3.8 Narration](#38-narration) · [3.9 Upload text and thumbnail](#39-upload-text-and-thumbnail) · [3.10 Translate (per batch of numbered lines)](#310-translate-per-batch-of-numbered-lines)
<!-- /nav -->

## 1. The four servers

Every request to any of them — and every web search and page read — is timed and written to the project's `requests.tsv` (new; [09 §10](09-llm-and-tools.md#10-every-request-timed-new)); each server is given one of this app's requests at a time, and requests to different servers run side by side: a narration speaking on audio.cpp while the LLM writes a title, never two LLM jobs at once ([09 §4](09-llm-and-tools.md#4-liveness-and-the-gate-f63)).

| what | default | serves | API |
|---|---|---|---|
| OpenAI-compatible LLM | `http://127.0.0.1:8731` | every text and vision job | `POST /v1/chat/completions` (streaming, tools), `GET /v1/models` |
| audio.cpp | `http://127.0.0.1:8765` (or, below the box, `NAIVEPOST_TTS_URL` then `AUDIOCPP_SERVER`) | speech to text, alignment, diarization, voice separation, text to speech | `GET /health`, `GET /v1/models` (id, family, task), `POST /v1/ui/upload` (raw body, `X-Audiocpp-Filename`), `POST /v1/tasks/run` `{"model","request"}`, `POST /v1/audio/speech`, `POST /v1/tasks/unload_all_models` |
| sd.cpp (sd-server) | `http://127.0.0.1:1234` (or `SD_SERVER` below the settings box) | the thumbnail when one is drawn | `GET /sdcpp/v1/capabilities`, `POST /sdcpp/v1/img_gen` → job id, `GET /sdcpp/v1/jobs/{id}`, `POST /sdcpp/v1/jobs/{id}/cancel` |
| ffmpeg / ffprobe | on PATH (or a configured path; ffprobe from the same folder) | frames, waveforms, cuts, encodes | subprocesses |

An empty server box means the loopback default. A host with no scheme is read as `https://host`. Keys are sent as `Authorization: Bearer …` only when non-empty. Every JSON body is sent with `Content-Type: application/json` (the upload's raw body is the one body that is not JSON): the llama.cpp router reads an unlabelled body anyway, but a proxy in front of a model (FastAPI) answers 400 "Input should be a valid dictionary" to the same bytes. An upload's body is the file's own bytes, byte for byte (first rewrite: the file went through a lossy UTF-8 copy, every non-UTF-8 byte became three, and audio.cpp answered 500 "invalid WAV RIFF header" to an ASR chunk 1.9 times its size). The settings file is re-read per request (each URL, key and model id), so a settings change lands on the next request; the LLM server has no env override.

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

**Prototype:** of everything below only `web_search` and `web_read` exist, offered to the cut, the narration and the upload text; every other job answers with one JSON reply, parsed and re-asked on failure ([`09-llm-and-tools.md` §2](09-llm-and-tools.md#2-tool-protocol-f61)). The rest is the rewrite. Tools are offered per job, each WITH its arguments as a JSON schema (`parameters`: the args of its table below, typed — line, clip and frame numbers integers, seconds and rates numbers, `calm`/`changed` booleans, `side`/`kind`/`pos` their listed words — and every arg without `?` required). A llama.cpp server builds its reply grammar from that schema, and a tool offered by name alone is answered with `{}`: the first rewrite offered names only, so every call reached its executor with no arguments (probed 2026-10-25). Each result is a short JSON object; problems come back as `{"error": "<one sentence the model can act on>"}` and never end the flow. Numbers the model must not compute are always identifiers from the request (line, clip, frame numbers; offsets as stamped).

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

`finish` answers with the frames still without an event, plus the batch's first frame if left "same" with no history to be the same as. A reply in prose, as the shipped describe prompt shows it ("EVENT [+14.2s]: …" lines and a "STATE: …" line), is read into the `record_event`/`set_state` calls it stands for: each stamped line to the nearest frame within 1 s of its stamp, otherwise dropped; with no stamps, an "EVENT:" label or the words in front of the STATE line are the first frame's line; nothing labelled records nothing. A reply whose calls record nothing (a bare `finish` beside such text) is read as prose, and so is a cached reply that is not a call list; a reply with both is read text first and calls after, so a call naming a frame of the batch wins and the text fills the frames the calls left without an event (the user's model numbers `record_event` by the frame's seconds, `frame: 22` in a batch of four, which is refused); the reply cached is that merged list, and a cached list is read in the `{"0": name, "1": args}` form it is filed in (first rewrite: read as pairs only, so every replayed tool reply was written "same" throughout) (first rewrite: a prose reply was parsed as a call list and every frame written "same"; a prose reply with a bare `finish` was refused for frames without an event until Prepare failed, 08-dex 2026-10-29, the same shape in 3 of 3 replays). Prototype, all invisible to the model: the stamps come back as prose; each line is snapped to the nearest frame within half an interval and **dropped without a word when further off**; an unparseable offset is dropped silently; a reply with no label at all is filed as the description; a multi-line answer is flattened to one; the batch's first frame is force-written as "Calm; same view." when the model said "same".

### 3.3 Fix (per block of transcript lines)

No tools. One chat call per block, as the prototype asked it: the User Context, the context around the block and its rows (`start⇥end⇥speaker⇥text`), and the answer is the same rows with only the text changed. The enforcement is the prototype's comparison, kept strict: a count, a time (±0.01) or a speaker that moved, or a row emptied, refuses the answer; unlike the prototype, the refusal's reason is put in front of the model on its one retry, and a block refused twice keeps its ASR lines. The first rewrite offered `fix_line(n, text)`, `flag_line(n, why)`, `get_lines` and `finish` instead; on a real model the reads answered nothing, the model answered nothing after them, and the blocks kept the ASR's text.

### 3.4 Retake marking

| tool | args | result |
|---|---|---|
| `mark_abandoned` | `from`, `to`, `again` (line numbers; `again` 0 = never picked up) | **ok with the stretch that will actually be removed** — the seconds left after trimming to the words the later take repeats and placing the edges on the audio — plus the running share of the session's speech now marked; or the refusal ("lines a-b say they are said again at line c, which is inside them -- not a mark", out of range, farther than P.machine.retakeReachSeconds, shorter than P.machine.retakeMinSeconds) |
| `unmark` | `from`, `to` | ok |
| `get_lines` | `from`, `to` | the lines again, with the pause before each in seconds — including those under P.machine.retakePauseSeconds, which the brief omits |

`finish` answers with the total marked against P.machine.retakeCeil, so a model over the ceiling can take some back. Prototype: three identical calls are pooled and deduped; each mark is trimmed to the repeated tail by a fuzzy matcher; a whole take gets `again` rewritten to 0; a rephrase is trimmed to the broken-off fragment; a refused mark is re-heard by a second ASR pass and sometimes resurrected; overlapping marks are merged; over the ceiling **every mark from all three runs is thrown away**. The model is told none of it.

### 3.5 Text edit (per join)

No tools. One reply, the words that go, quoted: `{"drop_before":"<the last words of BEFORE, copied exactly, or empty>","drop_after":"<the first words of AFTER, or empty>"}`. The app matches each quote bare against the end of BEFORE and the start of AFTER and refuses a quote that does not match, a removal over P.machine.seamMaxWords, or one over P.machine.seamCeil of the words shown; the counts it finds go through the same ceilings a hand edit's do ([`04-prepare.md` F1.10](04-prepare.md#f110-repair-the-joins)).

First rewrite: `drop_words(side, count)`, `keep_join`, `get_words` offered as tools while the prompt asked for the joined text; the model, told two things, answered nothing usable at 125 of 127 asks on 07-news (2026-10-26), and with a consistent tool prompt it still scored below the quoted answer (15/18 against 17/18). Prototype: the model returned the joined text and the app derived the counts by matching backwards.

### 3.6 Cut (model-chosen)

| tool | args | result |
|---|---|---|
| `add_segment` | `start`, `end` (session seconds copied off lines), `why` | ok with **what the segment became**: the snapped edges and how far each moved, the seconds a mark will take out of it, whether it survives P.policy.minSceneSeconds, and the running footage total against the target window — or the error (no footage under an edge, so the segment would be dropped; past the end, with the mm:ss reading of the model's own number; ends before start; overlaps an added segment) |
| `remove_segment` | `start` | ok |
| `set_speed` | `start`, `rate` | ok with the rate as applied (P.eng.minRate…maxRate, lowered where the clip would otherwise render under P.eng.minClipSeconds) and the resulting on-screen length |
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
| `get_lines` | `from`, `to` | the transcript around a clip, beyond the brief's ±P.machine.narrationContextSeconds |
| `describe_insert` | `clip` | what is actually on an inserted card: its text, parameters, length. The prototype passes only the **file name**, so "tier.svg?S=Dust II" is all the writer knows about a full-screen graphic |

`finish` names the clips with no answer and those whose lines will not fit, so they can be rewritten shorter instead of squeezed by the render. Prototype: one JSON answer matched to clips by echoed bounds within 0.5 s, scanning forward; **a single clip skipped, a single echo more than half a second out, or entries out of clip order rejects the whole reply**, three times, then the run fails with nothing written; `pos` outside five known words silently becomes bottom; entries are re-sorted by (clip, offset) though out-of-order clips were just rejected; and the user's hand-made "this clip plays its own audio" list is wiped every run.

### 3.9 Upload text and thumbnail

| tool | args | result |
|---|---|---|
| `set_title` | `text` | ok with the words as printed across the picture and the size they take — or the warning they will overflow the band (over seven words) |
| `set_description` | `text` | ok |
| `pick_frame` | `clip`, `offset` | ok with **the frame actually taken and how far it is from the moment asked for** (frames are offered one a second, `prepare::MODEL_FRAME_STEP`, out of the 250 ms grid; first rewrite: every P.project.frameInterval seconds; the prototype takes the nearest at any distance), and a note that a picked frame means nothing is drawn |
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

### 3.12 Text gate (per pair of frames, F1.6 S6)

| tool | args | result |
|---|---|---|
| `judge_text` | `changed` (true/false), `what` (one short line) | the verdict is the answer; no result is sent back |

Asked of the decision model (Settings → Decision; empty, the main model) with two frames, FIRST and SECOND, and the [textgate](prompts/textgate.md) prompt. One call per question; an answer without `judge_text`, or a `changed` that is not true or false, is a failed judge (the gate keeps the pixel pass's change). The tool belongs to no job, like §3.11's.

## 4. Degraded modes

- No aligner: cut points come off the waveform ("clean wherever there is a silence to cut in, and unable to cut between two words of one breath").
- No firefox, or inside Flatpak: no web tools; the model writes only what the material says.
- sd.cpp down: the render completes; the thumbnail half fails and is logged. A job the server forgot (404/410) is reported as "sd.cpp forgot job <id> (<status>) -- it was probably restarted mid-draw"; a server reporting no `supported_modes` is assumed able to draw; the Settings test falls back to `/v1/models` to name an OpenAI-shaped tenant of the port.
- An LLM server refusing `tools` (an error on the first round that is not a stop; a `tools` error on a later round fails the call): the job runs as one JSON answer with the prototype's parsers and retry turns.
- Flatpak: no web tools; no desktop entry, icon or MIME package written (the manifest exports them); voices folder in the sandbox's data dir.
- A reply cut off at the token limit: with tools, everything already called stands; without, the prototype's "answer again with far fewer items" retry.

## 5. Details confirmed against the code (verification pass)

- **Request bodies.** `/v1/tasks/run`: ASR `{"audio","language"}`, diarization `{"audio"}`, alignment `{"audio","text","language"}` — `audio` is always the server-side path an upload returned, never local. `/v1/audio/speech`: `{"model","input","voice_ref","language","options"}` with `voice_ref` the uploaded path of `narrate/voice_ref.wav` (re-uploaded before every line: a remembered path dies with a server restart), `options` the emotion vector plus `seed`, `language` = P.policy.ttsLanguage (P.policy.language, as ASR and alignment send; prototype: hard-coded "en"). `img_gen`: `prompt`, `negative_prompt?`, `width?`, `height?`, `seed` (always), `ref_images` (data URLs — an edit model is conditioned on references, not `init_image`), `auto_resize_ref_image` (always), `output_format?`; everything else deliberately left to the server's start-up flags.
- **Diarization reply.** The turns list is read under `speaker_turns` (audio.cpp's nemotron diarizer), `turns`, `speakers` or `diarization`, each turn `start_sample`/`end_sample` (seconds and milliseconds also read) and `speaker_id`: a number, or a name ending in one (`speaker_0` is voice 0, transcribed SPEAKER_00), any other name hashed to a number (first rewrite: `speaker_turns` unknown, so a good answer failed the run with "no speaker turns in it"). A reply with no list but audio.cpp's `timing` is nobody speaking, an empty list: that is how the nemotron diarizer answers silence (probed 2026-10-28: `{"timing":{"wall_ms":15.65}}` on 2.5 s of silence; first rewrite: failed Prepare with "no speaker turns in it"). A reply with neither is the model's failure.
- **sd.cpp waits.** Capabilities 15 s; submit 60 s; each poll 30 s at a 1 s interval; cancel 10 s.
- **Settings tests** (ten buttons and "Test All", each with its own verdict): LLM — one completion, `max_tokens` 16, thinking forced off, 60 s; LLM vision — a generated 48 px red square as a data URL, 120 s, passes only if the reply contains "red"; TTS endpoint — `/health` (15 s) then the catalogue, looking for family `index_tts2` or task `clon`; TTS model, ASR, diarization, separation — one id each against `/v1/models`, checking the declared task; aligner — by task, never by id (a box could only disagree with the server); ffmpeg — `LookPath`, `ffprobe` beside it, then `-filters`/`-encoders` against `rubberband, subtitles, loudnorm, atempo, amix, adelay, alimiter` and `libx264, libx265, aac, libopus`; firefox — `--version`, then a real headless search. `GET /v1/models` on the LLM backs only "Fetch models" (the id has no default and must be one the server lists) and the sd.cpp fallback probe.
- **No Save in Settings.** The file is written 600 ms after the last keystroke and again on close if a write is owed; a save resets the cached TTS model id and the "already listening on" note.

<!-- nav -->
---
[← 01 The project on disk](01-project-and-files.md) · [↑ top](#02--services-and-the-tool-catalogue) · [↑ Contents](README.md) · [03 The shell →](03-shell.md)
<!-- /nav -->
