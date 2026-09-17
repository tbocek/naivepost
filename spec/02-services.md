# 02 — Services and the tool catalogue

## 1. The four servers

| what | default | serves | API |
|---|---|---|---|
| OpenAI-compatible LLM | `http://127.0.0.1:8731` | every text and vision job | `POST /v1/chat/completions` (streaming, tools), `GET /v1/models` |
| audio.cpp | `http://127.0.0.1:8765` | speech to text, alignment, diarization, voice separation, text to speech | `GET /health`, `GET /v1/models` (id, family, task), `POST /v1/ui/upload` (raw body, `X-Audiocpp-Filename`), `POST /v1/tasks/run` `{"model","request"}`, `POST /v1/audio/speech`, `POST /v1/tasks/unload_all_models` |
| sd.cpp (sd-server) | `http://127.0.0.1:1234` (or `SD_SERVER` below the settings box) | the thumbnail when one is drawn | `GET /sdcpp/v1/capabilities`, `POST /sdcpp/v1/img_gen` → job id, `GET /sdcpp/v1/jobs/{id}`, `POST /sdcpp/v1/jobs/{id}/cancel` |
| ffmpeg / ffprobe | on PATH (or a configured path; ffprobe from the same folder) | frames, waveforms, cuts, encodes | subprocesses |

A server box left empty means the loopback default. A box with a host and no scheme is read as `https://host`. Keys are sent as `Authorization: Bearer …` only when non-empty. `readConf` is re-read per step so a settings change takes effect on the next step.

Audio model ids (settings, with defaults): ASR `nemotron-asr`, diarization `sortformer-diar`, TTS `index-tts2`, separation `bs-roformer`, aligner: none by default (any model declared task `align`, preferring `qwen3-aligner`). A missing model is reported with the server's own install hint. Nothing is compiled in; no call leaves the machine unless a box points elsewhere.

Every request rides the run's cancel context, so ⏹ aborts it. Audio task calls have no client timeout (an hour of audio is an hour of work). LLM calls have a silence rule (streamed) or a whole-call ceiling (`09-llm-and-tools.md` §4).

## 2. Model roles (which job asks which server)

| job | model | thinking | answers with |
|---|---|---|---|
| speech to text | ASR (audio.cpp) | – | text per chunk (≤ 60 s for qwen3-family, else ≤ 300 s) |
| word times | aligner (audio.cpp) | – | start/end per word, the chunk's own words in hand |
| who spoke when | diarization (audio.cpp) | – | speaker turns per window (90 → 45 → 25 s ladder) |
| voice split | separation (audio.cpp) | – | voice and rest stems |
| what is on screen | LLM with vision | off | one EVENT per frame + a running STATE, four frames a call |
| clean transcript | LLM | off | the same lines respelled; times and speakers unchanged, enforced |
| lecture joins | LLM | on | the two takes run on as one; only deletions accepted |
| retakes (gaming) | LLM | off | line numbers of abandoned stretches, three runs pooled |
| gaming cut | LLM (+ web tools) | on | segments in session seconds copied off the timeline |
| captions, speed, effects | LLM | off | per clip, in the clip's own seconds |
| narration | LLM (+ web tools) → TTS | on | a line per clip with an emotion; spoken in the cloned voice |
| upload text | LLM (+ web tools) | on | title, thumbnail (frame or instruction), description |
| thumbnail | sd.cpp | – | an image edited from real frames; only when an instruction exists and no frame was named |
| subtitles in other languages | LLM | off | numbered lines translated, one call per language |

## 3. Tool catalogue (rewrite directive B)

Tools are offered per job. Each tool's result is a short JSON object; problems come back as `{"error": "<one sentence the model can act on>"}` and never end the flow. Numbers the model must not compute are always identifiers from the request (line, clip, frame numbers; offsets as stamped).

### 3.1 Shared

| tool | args | result |
|---|---|---|
| `get_context` | – | the User Context and the policy fields relevant to this job |
| `get_lines` | `from`, `to` (line numbers of the session timeline) | the stamped lines |
| `get_events` | `from`, `to` (session seconds) | EVENT lines in the range |
| `web_search` | `broad`, `medium`, `narrow` | up to 8 hits with snippets (offered only where the prototype offered it: cut, narrate, upload text) |
| `web_read` | `url` | page text (≤ 6000 chars) |
| `finish` | – | ends the job |

### 3.2 Describe (per chunk of frames)
| tool | args | result |
|---|---|---|
| `record_event` | `frame` (1..n as stamped), `text`, `calm` (bool) | ok; a frame recorded twice replaces |
| `set_state` | `text` | ok |
`finish` validates that every frame has an event (missing frames become "same"; the first frame of a batch with no history must not be "same").

### 3.3 Fix (per block of transcript lines)
| tool | args | result |
|---|---|---|
| `fix_line` | `n` (line number in the block), `text` | ok, or error when `text` contains a tab |
Times and speakers are not arguments: the tool cannot change them, which is the enforcement the prototype did by comparison.

### 3.4 Retake marking (gaming)
| tool | args | result |
|---|---|---|
| `mark_abandoned` | `from`, `to`, `again` (line numbers; `again` 0 = never picked up) | ok, or the prototype's refusal ("lines a-b say they are said again at line c, which is inside them -- not a mark", out of range, too far (> policy.retakeReach)) |
| `unmark` | `from`, `to` | ok |

### 3.5 Text edit (lecture, per join)
| tool | args | result |
|---|---|---|
| `drop_words` | `side` (before/after), `count` | ok with the words that would go; error when the join would lose more than policy.seamMaxWords / policy.seamCeil |
| `keep_join` | – | nothing removed here (a whole answer) |
Prototype: the model returned the joined text and the app derived the counts by matching backwards. With tools the counts are stated directly, and the app still re-derives the deleted stretch to check it is one stretch at the join.

### 3.6 Cut (gaming)
| tool | args | result |
|---|---|---|
| `add_segment` | `start`, `end` (session seconds copied off lines), `why` | ok with the snapped segment, or error (past the end with the mm:ss hint; ends before start; no EVENT line inside; overlaps an existing segment) |
| `remove_segment` | `start` | ok |
| `set_speed` | `start`, `rate` | ok (a rate over a whole segment) |
| `finish_cut` | – | runs the prototype's whole-cut checks (count within [min,max], footage within the target window) and returns the problem list or ok |

### 3.7 Captions / speed / decorations (per clip)
| tool | args | result |
|---|---|---|
| `add_caption` | `clip`, `start`, `end` (offsets), `text` | ok or error (clip not given; under policy.captionMin) |
| `set_clip_speed` | `clip`, `rate` | ok or error (rate > 1 on a captioned clip) |
| `add_effect` | `clip`, `kind` (zoom/stop/volume), `start`, `end`, `gain?` | ok or error (kind not one of the three) |

### 3.8 Narration
| tool | args | result |
|---|---|---|
| `write_line` | `clip`, `at` (offset), `text`, `emotion`, `pos?` | ok (at clamped inside the clip, never its last second; word ceiling reported), or error |
| `leave_silent` | `clip` | ok |
`finish` reports clips with no answer.

### 3.9 Upload text and thumbnail
| tool | args | result |
|---|---|---|
| `set_title` | `text` | ok (warns over seven words) |
| `set_description` | `text` | ok |
| `pick_frame` | `clip`, `offset` | ok with the frame taken as the thumbnail (no drawing) |
| `set_thumbnail_instruction` | `text`, `negative?` | ok (an image model will draw) |
`finish` requires a title and a description.

### 3.10 Translate (per batch of numbered lines)
| tool | args | result |
|---|---|---|
| `translate_line` | `n`, `text` | ok or error (empty) |
`finish` lists the numbers still missing; the app re-asks once for those.

### 3.11 Policy derivation (once per project, F0.7)
| tool | args | result |
|---|---|---|
| `set_policy` | `field`, `value`, `because` | ok or error (unknown field, out of range) |
The model reads the User Context and sets only the fields the context speaks to ("clips under 2 s", "about 12 minutes", "keep the swearing"). Every other field keeps its default. The form shows the `because`.

## 4. Degraded modes
- No aligner: cut points come off the waveform ("clean wherever there is a silence to cut in, and unable to cut between two words of one breath").
- No firefox or inside Flatpak: no web tools offered; the model writes only what the material says.
- sd.cpp down: the render completes; the thumbnail half fails and is logged. A job the server forgot (404/410) is reported as "sd.cpp forgot job <id> (<status>) -- it was probably restarted mid-draw"; a server reporting no `supported_modes` at all is assumed able to draw; the Settings test falls back to `/v1/models` to name an OpenAI-shaped tenant of the port.
- An LLM server that refuses `tools`: the job runs as one JSON answer with the prototype's parsers and retry turns.
- A model whose reply is cut off at the token limit: with tools, everything already called stands; without, the prototype's "answer again with far fewer items" retry.
