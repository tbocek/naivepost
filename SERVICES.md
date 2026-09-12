# Services, and what waits for what

Naivepost computes nothing itself. Every expensive thing in it is a request to
one of four servers, or a subprocess:

| service | default endpoint | configured as | does |
|---|---|---|---|
| **LLM** (halogen-flash-server, llama.cpp, anything OpenAI-compatible) | `http://127.0.0.1:8731` | `LLM_SERVER`, `LLM_MODEL` | every text job, and the vision job that describes frames |
| **audio.cpp** | `http://127.0.0.1:8765` | `AUDIOCPP_SERVER` + one model id per task | speech-to-text, forced alignment, diarization, voice separation, TTS |
| **sd.cpp** | `http://127.0.0.1:1234` | `SD_SERVER` | the thumbnail, when one is drawn |
| **ffmpeg / ffprobe** | subprocess | `FFMPEG` | audio extraction, frames, silence detection, every encode |
| *(firefox)* | subprocess | `FIREFOX` | headless web search, offered to three jobs as a tool |

A blank endpoint means the loopback default above. Nothing is started by the
app — running the stack is the stack's job.

---

## The flow

Time runs downward. `│` is a dependency, `‖` marks things that already run at
the same time.

```
PREPARE  ────────────────────────────────────────────────────────────────────
  catalog check                                            audio.cpp
  voice separation (only sources flagged for it)           audio.cpp   sep
        │
        ├─────────────────────────────┐
        │                             │
  frames, per video          ‖   per recording, in turn:
  ffmpeg, sharded                     wav 16k mono          ffmpeg
  over 2–8 workers                    speech to text        audio.cpp  asr
        │                             word alignment        audio.cpp  align
        │                             diarization           audio.cpp  diar
  │                             (window 90/45/25 s, first that fits)
        │                             (2 passes, ffmpeg cuts each window)
        │                             merge into segments   local
        └─────────────┬───────────────┘
                      │  frames AND transcripts
              describe, 4 frames per request                LLM (vision)
                      │  one chunk at a time: each carries the last chunk's STATE
              fix the transcripts, block by block           LLM
                      │  needs the event log and the other mics
              merge → session.tsv                           local
                      │
              which seconds go:                             LLM
                Lecture → textedit, one call over every word
                Gaming  → retake, 3 identical calls pooled
                      │
CUT  ────────────────────────────────────────────────────────────────────────
  Lecture: no model at all — every filmed second minus the marks
  Gaming:  cut → captions → speed → effects                 LLM, in that order
           (speed is told what got captioned; effects sees both)
                      │
NARRATE  ─────────────────────────────────────────────────────────────────────
  write the narration, one call                             LLM
                      │
  speak each line, one at a time                            audio.cpp  clon
                      │
PRODUCE  ─────────────────────────────────────────────────────────────────────
  ┌───────────────────────────────┬──────────────────────────────────────┐
  │ upload text (title, thumbnail │ subtitles from the aligned words     │
  │ line, description)      LLM   │        │                             │
  │        │                      │ translate, one call per language LLM │
  │ a frame named → cropped       │        │                             │
  │ locally, no GPU               │ encode every clip, in turn    ffmpeg │
  │ otherwise → draw       sd.cpp │        │                             │
  │                               │ join (stream copy)            ffmpeg │
  │                               │ loudness + mux subs           ffmpeg │
  │                               │ .srt + .vtt per language      local  │
  └───────────────────────────────┴──────────────────────────────────────┘
        the two halves already run side by side; neither reads the other's files
                      │
  poster (the thumbnail, as jpeg) and the <video> tag        local
  written once both halves are in
```

---

## What runs in parallel today

1. **Frames ‖ speech** (`ingest`). Frame extraction is ffmpeg on the CPU, the
   speech track is the GPU server: they do not contend, so they are two
   goroutines with two progress tracks.
2. **Frame extraction itself** is sharded across `NumCPU/4` workers, clamped to
   2–8, each decoding its own time range of the same file. One worker when
   there are fewer than four frames per worker, or in every-frame mode.
3. **Upload text + thumbnail ‖ the render** (`produceRun`). The words half and
   the video half touch no common file, so a model thinking about a title no
   longer leaves the encoder idle.

Everything else is a loop.

---

## What could run in parallel, and what makes it safe

Ordered by what it would actually save on a session like a lecture.

| # | what | why it is safe | what it costs |
|---|---|---|---|
| 1 | **Translation ‖ clip encodes** | The translated `.srt` is only needed by the final mux, which happens after every clip is encoded. Today the translation (LLM, GPU) finishes before the first clip is encoded (ffmpeg, CPU). | On the 10:29 lecture: translate took 3m 03s of a 5m 18s Produce. Nearly all of it is recoverable. |
| 2 | **Describe, one recording per request in flight** | Chunks within one recording chain through the rolling `STATE` and cannot be split. Recordings cannot: each has its own `events.tsv` and its own state file. | Describe is the longest job in the app. 14 recordings at 4 frames a request, currently strictly one after another. |
| 3 | **Fix the transcripts, block by block** | Blocks carry no state between them — each request is a block plus the window of context around it, and each answer is validated against its own block. | Second-longest LLM job; every block is independent. |
| 4 | **The retake pool, 3 at once** | The three calls are already identical by design and their answers are pooled. Running them concurrently changes nothing about the result. | Gaming only. Cuts that pass to a third of its wall clock. |
| 5 | **Clip encodes, N at a time** | Each clip is its own ffmpeg invocation writing its own file; only the concat list needs them all. | The whole encode is CPU-bound and single-threaded per clip today. |
| 6 | **Diarization ‖ ASR+alignment**, within one recording | Diarization reads the wav and answers turns; it never reads `words.json`. Only `mergeSegments` needs both. | Both hit audio.cpp, so this only helps if that server will hold two models and serve them at once. |
| 7 | **Speech track, one recording per worker** | Recordings are independent until the merge. | Same caveat as 6 — it is one server, and it is the bottleneck already. |
| 8 | **Caption / speed / effect batches** | Within each pass the batches are independent (5 clips each); the *passes* are not — speed is told what got captioned and effects sees both. | Gaming only. |

## What cannot be parallelised

- **Chunks of one recording's description.** Each request carries the previous
  chunk's `STATE` line. That is the whole reason the output reads as *what is
  happening* rather than *what is in this picture*.
- **The four cut passes.** Cut → captions → speed → effects is a chain of
  inputs, not a preference.
- **Prepare's last pass** (textedit / retake) needs the merged timeline, which
  needs every recording's transcript and every video's event log.
- **join and mux** need every clip.

---

## Memory, which is the real limit

Parallelism here is not free: three of the four services want the same GPU and
the same RAM.

- **The LLM server** is the big one. The halogen build this is developed
  against pins about 68 GiB of weights and reserves a KV pool of 524288
  positions; `/health` reports `"slots": 4`, so it will accept four requests at
  once and schedule them itself. Items 2, 3, 4 and 8 above are all "send more
  than one request" — the server, not the app, decides how well that goes.
- **audio.cpp** loads models lazily and holds a few at a time
  (`max_loaded_models`, `idle_unload_ms` in `config-audiocpp.json` — 3 and five
  minutes in this stack). ASR, align, diar, sep and TTS are five different
  models: running two tasks at once can mean a load and an unload between every
  request, which is slower than doing them in turn. Item 6 and 7 are only worth
  it on a server configured to hold both.
- **sd.cpp** holds whatever model it was started with, for the one thumbnail.
- **ffmpeg** is CPU and memory-cheap, and is the one thing that can always
  overlap with a GPU job — which is what items 1 and 5 exploit, and what the
  two parallel tracks that already exist were built for.

The app unloads the audio models when a run ends (`freeAudioModels`, after the
chain's next step has started — the next step usually wants them back).

---

## What is skipped on a re-run

None of the above costs anything if the answer is already on disk. Re-running
is a resume:

| stage | skipped when |
|---|---|
| voice separation | both halves exist |
| frames | `.interval` marker matches the interval and scale |
| wav, ASR, alignment, diarization | `voice16k.wav`, `words.json`, `words.aligned.json`, `turns.json` exist |
| describe | the chunk is already in `events.tsv`; otherwise the request is looked up in `cache/llm/describe` |
| fix | per block, `cache/llm/transcript` |
| textedit / retake / translate | `cache/llm/<step>`, keyed on the whole request (retake also on the run number, so the pool stays a pool) |
| narration TTS | the wav is named after the text, emotion, voice and re-roll |
| upload text | `produce/publish/publish.json` exists |
| thumbnail | `thumbnail.stamp` matches the images, instruction and crop |
| the video | `<name>.stamp` matches the settings, cut, narration wavs and source files |

Two of those are **existence** checks and not input checks: change the ASR
model or the aligner in Settings and Prepare will not redo a recording that has
`words.json` / `words.aligned.json` already. Delete the file to re-run that
stage.
