# 04 — Prepare

<!-- nav -->
[← 03 The shell](03-shell.md) · [↑ Contents](README.md) · [05 Cut →](05-cut.md)

**Flows:** [F1.1](#f11--prepare) · [F1.2](#f12-voice-separation-rows-with-) · [F1.3](#f13-per-source-audio--text--word-times--speakers--segments) · [F1.4](#f14-asr-in-chunks) · [F1.5](#f15-forced-alignment) · [F1.6](#f16-frames-per-video) · [F1.7](#f17-describe-per-footage-source-chunks-of-ppolicydescribeframesperreq--4) · [F1.8](#f18-fix-the-transcripts-blocks-of-ppolicyfixblocklines--25-lines) · [F1.9](#f19-mark-retakes-gaming-style) · [F1.10](#f110-repair-the-joins-lecture-style) · [F1.11](#f111-place-the-edges-of-a-mark) · [F1.12](#f112-hand-edit-the-text-lecture) · [F1.13](#f113-the-sessions-word-list-shared-by-retakes-joins-finaltxt-and-subtitles)
<!-- /nav -->

Sources, transcripts, frames, and what the models make of them. One ▶ runs the whole step; each stage skips when its output exists.

## 1. Screen

```text
┌ Prepare ─────────────────────────────────────────────┬──────────────────────────────────────────────┐
│ [Add source files…]  ☑ copy into project             │ User Context                    [User Context ▾]│
│ ┌──────────────────────────────────────────────────┐ │ ┌──────────────────────────────────────────┐ │
│ │ 🎥 🎤1  2026-09-16 17-25-06.mkv           ✂ 🗑    │ │ │ The weekly blockchain lecture at OST: one │ │
│ │ 🎥      2026-09-16 17-25-45.mkv           ✂ 🗑    │ │ │ person speaking to camera, mostly in      │ │
│ │ …                                                 │ │ │ German … about 12 minutes …               │ │
│ └──────────────────────────────────────────────────┘ │ │                                          │ │
│ Freq:[1s][−][+] [Original ▾] Language:[de] Style:[Lecture ▾] │ └────────────────────────────────────┘ │
│                                                       │  (rows: User Context, System context, Describe prompt,│
│                                                       │   Transcript, Retakes, Text edit, Cut, Captions,│
│                                                       │   Speed, Effects, Narration, Translate,        │
│   Upload text — each headed "<Name> prompt")   │
└───────────────────────────────────────────────────────┴──────────────────────────────────────────────┘
 Inputs: 2418 frames → 605 vision · 778 lines → 32 fixer      Outputs: [📁] Prepare: 41 files, 220 MB
```

Left: the sources list ([`03-shell.md` §4](03-shell.md#4-sources-list-lives-on-prepare-specified-here-because-the-shell-snapshots-it)) and, under it, Freq (frame interval stepper: each, 0.1, 0.2, 0.5, 1s, 2s, 3s, 4s, 5s; "Seconds between frames — type 0.1 to 5, or 'each' for every frame"), Frame size (Original, 896w (LLM), 480p, 720p, 1080p; "Frame size — Original keeps the video's own size"), Language (ASR code; "…the wrong one transcribes into gibberish. Empty means en"), Style (style table; shipped Lecture and Gaming; a blank project stores "" = **Gaming** — REVIEW: with no project loaded the prototype's dropdown opens on "Lecture", so picker and stored value disagree until touched; the rewrite MUST ship and show an explicit default; "Lecture: a read to camera -- the speech is the video, cut by removing the words said twice and the false starts. Gaming: a session -- the cut picks the moments worth keeping. Both describe the picture; only what the cut is chosen from differs.").

Right: the prompt bench — one heading row (title, "edited — kept in your settings" mark, row picker, Reset), one text box. The User Context row hides mark and Reset (no built-in wording to differ from). Row 0 = **User Context** (per project, sent with every request, outranks the prompts; empty sends nothing). Rows 1–12 = the twelve prompts in pipeline order, per machine; ✎ = edited on this machine; Reset restores the shipped wording. Every keystroke is stored. REVIEW: add a row "Editing policy" opening the form of [F0.7](03-shell.md#f07-derive-the-editing-policy-review--new).

Inputs readout: "N frames → M vision · L lines → K fixer", per-file tooltip. Outputs: one folder button for `prepare/`, label "Prepare:" + count; its tooltip names the three subfolders.

## 2. Flows

### F1.1 ▶ Prepare

<sub><!-- back -->[← F0.13](03-shell.md#f013-tests) · [↑ 04 Prepare](#04--prepare) · [all flows](11-flow-index.md#3-all-flows) · [F1.2 →](#f12-voice-separation-rows-with-)</sub>

```text
 ▶ ──► no sources                 ──► "add at least one source"
       two sources, one base name ──► "A and B have the same name — rename one"
         │
         ▼
       save the project  ──►  stopped inside Describe last time?  ──yes──►  clear events.tsv + state.txt
         │                                                                  (the scaled frames are kept)
         ▼
       preflight: audio.cpp healthy · ASR and diarization (and separation when asked) served
         │
         ▼
   0%            10%                        30%                                            100%
   ├── separate ──┼────────── ingest ────────┼──────────────── understand ───────────────────┤
   │    F1.2      │   speech F1.3 ║ frames F1.6   describe F1.7 → fix F1.8 → mark F1.9|F1.10
   │  (rows ✂)    │   two tracks, in parallel
         │
         ▼
       ">>> prepare wrote:" + a tree of the three folders · status "prepared — N files"
       ⏹ → "stopped — finished work is kept"      ✗ → "prepare FAILED: …"
```

S1 Ignore when running. No sources → "add at least one source". Two sources with one base name → refuse (log "!!! A and B are both inputs/<base> -- rename one", status "A and B have the same name — rename one"). S2 Save the project. S3 Last run stopped inside Describe → remove every `events.tsv`/`state.txt` (">>> stopped last time — describing from the start again: … (scaled frames kept)"); a failed clear is logged (">>> could not clear the last run (…) -- resuming it") and the run resumes from disk. S4 startRun; log ">>> prepare: N input files", each file, the inputs summary, ">>> prepare: sending the session context from Prepare (N characters)". S5 Check the audio server; ASR and diarization models (and separation when asked) served. S6 Phases, share of the bar: voice separation ([F1.2](#f12-voice-separation-rows-with-)) 0–10 % when asked; ingest ([F1.3](#f13-per-source-audio--text--word-times--speakers--segments) + [F1.6](#f16-frames-per-video) in parallel) to 30 %; understand = describe ([F1.7](#f17-describe-per-footage-source-chunks-of-ppolicydescribeframesperreq--4)) → fix ([F1.8](#f18-fix-the-transcripts-blocks-of-ppolicyfixblocklines--25-lines)) → marking ([F1.9](#f19-mark-retakes-gaming-style)/F1.10) 30–100 %. S7 Success: fraction 1, ">>> prepare wrote:" + tree of the three folders, status "prepared — N files"; stopped → "stopped — finished work is kept"; failed → "prepare FAILED: …" / "prepare failed — see log". S8 Refresh the page, gates and Cut page; unload audio models.

### F1.2 Voice separation (rows with ✂)

<sub><!-- back -->[← F1.1](#f11--prepare) · [↑ 04 Prepare](#04--prepare) · [all flows](11-flow-index.md#3-all-flows) · [F1.3 →](#f13-per-source-audio--text--word-times--speakers--segments)</sub>

```text
 <source> ──► decode to 44.1 kHz stereo ──► chunks ≤ P.policy.sepChunkMaxSeconds, cut at silences
                                                 │
              ┌──────────────────── per chunk ───┴──────────────────┐
              │ upload ──► separation model ──┬──► "vocals|voice"  ──┼──► VOICE
              │                               └──► "instrumental"   │    (else every other stem,
              └───────────────────────────────────────── amix ──────┘     normalize=0) ──► REST
                                                 │
                                                 ▼
   stems/<base>.split-voice.wav        stems/<base>.split-novoice.wav | .mkv (picture copied, sound FLAC)
                                                 │
                                                 ▼
   the row becomes two rows:   REST keeps 🎥 footage      VOICE takes the 🎤 slot      ✂ wish cleared
   rest ≥ 10 dB under the mix → a warning        both halves already there → ">>> [base] already split"
```

S1 Decode to 44.1 kHz stereo pcm. S2 Chunk at P.policy.sepChunkMaxSeconds (300 s) on silences. S3 Per chunk: upload, run the separation model; voice = `vocals|voice` stem, rest = `instrumental` stem (or every other stem mixed with amix normalize=0). S4 Join the parts; a video's rest muxed as mkv, picture copied. S5 Write `stems/<base>.split-voice.wav` and `<base>.split-novoice.wav|.mkv` (names keep the timestamp). S6 Log ">>> [base] splitting the voice off X s in N part(s) (<sep model>)" before the first chunk goes up, then ">>> [base] split into A and B" and a loudness report; warn when the rest is ≥ 10 dB under the mix. S7 Source row → two rows: rest keeps footage, voice takes the narrator slot; wish cleared; project saved. Skip when both halves exist.

### F1.3 Per source: audio → text → word times → speakers → segments

<sub><!-- back -->[← F1.2](#f12-voice-separation-rows-with-) · [↑ 04 Prepare](#04--prepare) · [all flows](11-flow-index.md#3-all-flows) · [F1.4 →](#f14-asr-in-chunks)</sub>

```text
 <source> ──► voice16k.wav (mono 16 kHz)          ">>> [base] X s of audio"
                  │
                  ├─ shorter than P.policy.minTakeSeconds? ──yes──► written up as silence, no server asked
                  ▼ no
             ASR       F1.4     — skipped when words.json is there
                  ▼
             align     F1.5     — when an aligner is served; ASR word times missing AND no aligner = hard error
                  ▼
             diarize            — skipped when turns.json is there; fatal after the 90 → 45 → 25 s ladder
                  ▼
             segments   words (aligned times, else the ASR's) + speaker by greatest turn overlap,
                        else the nearest turn within 1 s, else SPEAKER_00, else "?"
                        break on: speaker change · gap > 0.7 s · 12 s      word end clamped to start + 2 s
                  ▼
             transcript.tsv · transcript.srt        ">>> [base] N segments"
```

S1 `voice16k.wav` (mono 16 kHz) unless present; log ">>> [base] X s of audio". S2 **Short take**: < P.policy.minTakeSeconds (2.0) → written up as silence, no server asked (">>> [base] X s long -- a start/stop, not a take: written up as silence"). S3 ASR ([F1.4](#f14-asr-in-chunks)) unless `words.json` exists. S4 Alignment ([F1.5](#f15-forced-alignment)) when an aligner is served and `words.aligned.json` absent; failure = warning if the ASR gave word times, hard error if not, silent with no transcript either (nothing to align). S5 Diarization unless `turns.json` exists (fatal after the window ladder). S6 Segments: words from aligned or ASR times; speaker by greatest turn overlap, else nearest turn within 1 s, else SPEAKER_00 when no turns, else "?"; word end clamped to start + 2 s; segment breaks on speaker change, gap > 0.7 s, or 12 s; write `transcript.tsv`, `transcript.srt`; log ">>> [base] N segments".

### F1.4 ASR in chunks

<sub><!-- back -->[← F1.3](#f13-per-source-audio--text--word-times--speakers--segments) · [↑ 04 Prepare](#04--prepare) · [all flows](11-flow-index.md#3-all-flows) · [F1.5 →](#f15-forced-alignment)</sub>

```text
 voice16k.wav ──► fits the limit? ──yes──► one request ──────────────────────┐
  limit: 60 s for a qwen3 model,    │ no                                     │
         else 300 s                 ▼                                        │
                    cut at silence midpoints into even pieces                │
                    (seek ≤ 20 s, capped at limit/6 and step/3) · scratch in asr/
                                    │                                        │
                    ┌───────────── ⟲ per chunk ──────────────┐               │
                    │ checkpoint · "recognising speech i/n"   │               │
                    │ cut (-ss before -i) ──► upload ──► ASR {audio, language}│
                    └────────────────────┬───────────────────┘               │
                          out of memory ─┴─► halve the limit, down to 20 s, retry
                                    │                                        │
                                    ▼◄───────────────────────────────────────┘
                    stitch {text, words}, sample offsets shifted
                                    ▼
      transcript.txt ──► asrchunks.json ──► words.json        (last: it is the resume marker)
      no text at all is a real case: ">>> [base] no speech found -- an empty transcript"
```

S1 Chunk limit 60 s for a qwen3-family model, else 300 s; out-of-memory → halve down to 20 s, retry (logged). S2 File fits → one request; else cut at silence midpoints into even pieces (seek ≤ 20 s, capped at limit/6 and step/3); scratch in `asr/`, removed on success. S3 Per chunk: checkpoint, progress "recognising speech i/n", cut with -ss before -i, upload, run ASR with `{audio, language}`. S4 Stitch `{text, words}`, sample offsets shifted; write `transcript.txt`, `asrchunks.json`, `words.json` last. Empty text is a real case (">>> [base] no speech found -- an empty transcript").

### F1.5 Forced alignment

<sub><!-- back -->[← F1.4](#f14-asr-in-chunks) · [↑ 04 Prepare](#04--prepare) · [all flows](11-flow-index.md#3-all-flows) · [F1.6 →](#f16-frames-per-video)</sub>

```text
 which aligner?   the configured id ──► else any model declared task "align" ──► preferring qwen3-aligner
                  a configured id the server does not serve for alignment: alignment is SKIPPED for the run
                  "!!! align: <url> does not serve \"X\" for alignment -- cut points come off the waveform"
                       │  the first that answers is used for the rest of the run
                       ▼
 pieces      the ASR's own chunks when the word counts agree
             else 60 s windows cut at silences, words shared out by voiced seconds
                       ▼
 per window  trim to where sound is (0.25 s pad)
             over the limit and > 15 s ──► split at the quietest moment nearest the middle
                                           (inside the middle three quarters)
             out of memory ──► halve, never under 15 s
                       ▼
 request     {audio, text, language} ──► words from words|alignment|segments|result.*
             times read as samples → milliseconds → seconds → a bare number (> 120 s = samples)
                       ▼
 words.aligned.json written whole at the end   ·   lower-cased here, dressed again by F1.13
                       ▼
 "!!! align: N s of the M s spoken has no word over it -- the times are wrong, and the cut will
  drop that footage"          when bare voiced seconds ≥ 10 s AND ≥ 8 %
```

S1 Aligner = configured id, else every model declared task align, preferring qwen3-aligner; the first to answer serves the rest of the run. A configured id not served for alignment is not replaced: alignment skipped for the run with "!!! align: <url> does not serve \"X\" for alignment -- cut points come off the waveform". S2 Pieces = the ASR's own chunks when word counts agree, else 60 s windows cut at silences, words shared by voiced seconds. S3 Each window trimmed to sound (0.25 s pad); over the limit and > 15 s → split at the quietest moment nearest the middle (inside the middle three quarters); out-of-memory → halve, ≥ 15 s. S4 Request `{audio, text, language}`; reply words from `words|alignment|segments|result.*`; times read as samples, then milliseconds, then seconds, then a bare number (samples when > 120 s); words stored lower-cased, written form restored by the word list ([F1.13](#f113-the-sessions-word-list-shared-by-retakes-joins-finaltxt-and-subtitles)). S5 Write `words.aligned.json` whole at the end. S6 Warn "!!! align: N s of the M s spoken has no word over it -- the times are wrong, and the cut will drop that footage" when bare voiced seconds ≥ 10 s and ≥ 8 %.

### F1.6 Frames per video

<sub><!-- back -->[← F1.5](#f15-forced-alignment) · [↑ 04 Prepare](#04--prepare) · [all flows](11-flow-index.md#3-all-flows) · [F1.7 →](#f17-describe-per-footage-source-chunks-of-ppolicydescribeframesperreq--4)</sub>

```text
 <video> ──► .interval == "<interval>|<scale>"? ──yes──► rename legacy numbered frames only, skip
               │ no                                      ">>> [base] frames already extracted (Xs, scale)"
               ▼
             clear the folder · filter  fps=1/interval + the scale
             a take shorter than one interval ──► its first frame alone
               ▼
             chunks of whole intervals, workers = clamp(CPU/4, 2, 8)      progress from ffmpeg -progress
             1 worker for a tiny job or every-frame mode
               │
               ▼
             f000001.jpg f000002.jpg …  ──rename──►  <start + (n−1)·interval>.jpg   (-1, -2 within a second)
               ▼
             write .interval        ">>> [base] N frames extracted"
```

S1 Marker `.interval` = "<interval>|<scale>" matches → only rename legacy numbered frames; skip (">>> [base] frames already extracted (Xs, scale), skipping"). S2 Else log the plan (">>> [base] extracting a frame every Xs at <scale>" / "…extracting EVERY frame -- gigabytes"), clear the folder; filter `fps=1/interval` + scale (take shorter than one interval → its first frame alone). S3 Parallel chunks of whole intervals (workers clamp(CPU/4, 2, 8); 1 for tiny or every-frame jobs); progress from ffmpeg's `-progress`. S4 Rename `f000001.jpg…` to `<start + (n−1)·interval>` stamps (-1, -2 within one second). S5 Write the marker; log ">>> [base] N frames extracted".

### F1.7 Describe (per footage source, chunks of P.policy.describeFramesPerReq = 4)

<sub><!-- back -->[← F1.6](#f16-frames-per-video) · [↑ 04 Prepare](#04--prepare) · [all flows](11-flow-index.md#3-all-flows) · [F1.8 →](#f18-fix-the-transcripts-blocks-of-ppolicyfixblocklines--25-lines)</sub>

```text
 frames sorted by stamp ──► interval 0? ──► refused: "<base> was extracted as every-frame; describe
        │                                   needs a fixed interval — rerun Prepare with e.g. 1s"
        ▼
 scaled to 896 wide into .llmframes/ (unless the preset was already 896w or 480p)
        ▼
 resume: chunks whose start is in events.tsv are skipped · the last 3 rows seed the window
         state.txt seeds the STATE ("Recording just started.")
        ▼
 ┌ one request per P.policy.describeFramesPerReq frames ──────────────────────────────┐
 │  User Context                                                                      │
 │  STATE so far: …                                                                   │
 │  Frames cover t=As to t=Bs, X s apart.                                             │
 │  Just before this: <the last events>                                               │
 │  --- context before (do not describe) ---   --- spoken during these frames ---     │
 │  --- context after (do not describe) ---    (always all three, "(none)" when empty) │
 │  [+0.0s] FRAME 1 of 4  <image>   [+1.0s] FRAME 2 of 4  <image>   …                  │
 └────────────────────────────────────────────────────────────────────────────────────┘
        ▼                    cached on the whole content, images inline
 tools: record_event(frame, text, calm) · set_state(text) · finish
 prototype: one "EVENT [+Ns]: …" line per frame + "STATE: …", parsed leniently
        ▼
 events.tsv: one row per frame (no event → "same"; a batch's first frame with no history may not be)
 state.txt after every chunk       ">>> [base] event log complete (N chunks, M answered from the cache)"
```

S1 Frames sorted by stamp (interval-0 folder refused: "<base> was extracted as every-frame; describe needs a fixed interval — rerun Prepare with e.g. 1s"; missing or empty folders likewise); scaled once to 896 wide into `.llmframes/` unless the preset was 896w (LLM) or 480p. Each recording heard alongside the video logged with its offset (">>> [video] hearing X alongside it, starting Y s in"). S2 Resume: chunks whose start is in `events.tsv` skipped; last 3 rows seed the rolling window; `state.txt` seeds the STATE ("Recording just started."). S3 Message: User Context; "STATE so far: …"; "Frames cover t=As to t=Bs, X s apart."; "Just before this:" + last events; speech block (always three sections: context before, spoken during, context after; before/after within 10 s, ≤ 2 per source); then per frame "[+N.Ns] FRAME i of n" + image. S4 Cache on the whole content (images inline). S5 **Tools** ([`02-services.md` §3.2](02-services.md#32-describe-per-chunk-of-frames)): `record_event(frame, text, calm)`, `set_state(text)`, `finish`. Prototype: one "EVENT [+Ns]: …" line per frame + "STATE: …", parsed leniently. S6 One `events.tsv` row per frame (no event → "same"; a batch's first frame with no history must not be "same"); `state.txt` after every chunk. S7 Log ">>> [base] event log complete (N chunks)", or "(N chunks, M answered from the cache)" when any were.

### F1.8 Fix the transcripts (blocks of P.policy.fixBlockLines = 25 lines)

<sub><!-- back -->[← F1.7](#f17-describe-per-footage-source-chunks-of-ppolicydescribeframesperreq--4) · [↑ 04 Prepare](#04--prepare) · [all flows](11-flow-index.md#3-all-flows) · [F1.9 →](#f19-mark-retakes-gaming-style)</sub>

```text
 every source on the session clock ──► transcripts + event logs loaded
        ▼
 ┌ per block of P.policy.fixBlockLines lines ─────────────────────────────────┐
 │ context ±5 s from every other source (its events and lines) + own events   │
 │ "Context around these lines:" … "Transcript lines to clean (N, return N):" │
 │        ▼   tools: fix_line(n, text) · finish                               │
 │ prototype: N TSV lines back — discarded unless the count, the times (±0.01)│
 │ and the speakers are identical; two tries; the originals kept on failure   │
 └────────────────────────────────────────────────────────────────────────────┘
        ▼   only a valid block is cached
 <source>/transcript.fixed.tsv (+ subtitles.srt)  |  commentary.fixed.tsv      ·  offsets.tsv
        ▼
 session.tsv — every row sorted by start, events labelled EVENT
        ▼   ">>> session timeline: N rows across M source(s)"
 the style's marking pass:  Lecture → F1.10   ·   Gaming → F1.9
        │   a failure leaves it unmarked: "!!! retakes: … -- the timeline stands unmarked"
        ▼
 session.txt — exactly what the cut model sees, marked stretches folded into one line each
```

S1 Place every source on the session clock; load transcripts and event logs. S2 Per block: checkpoint; context ±5 s from every other source (events and lines) + own events; message "Context around these lines:\n…\nTranscript lines to clean (N lines, return exactly N):\n…". S3 **Tools**: `fix_line(n, text)` per line; `finish`. Prototype: N TSV lines back, discarded unless count, times (±0.01) and speakers identical; two tries; originals kept on failure. Only valid blocks cached. S4 Write `<source>/transcript.fixed.tsv` (+ `subtitles.srt` for a video) or `commentary.fixed.tsv`. (`offsets.tsv` is written earlier, in S1's placement pass, before any block goes out.) S5 Merge into `session.tsv` (sorted by start; events labelled EVENT); log ">>> session timeline: N rows across M source(s)". S6 Run the style's marking pass ([F1.9](#f19-mark-retakes-gaming-style) or [F1.10](#f110-repair-the-joins-lecture-style)); failure leaves the timeline unmarked ("!!! retakes: … -- the timeline stands unmarked"). S7 Write `session.txt` exactly as the cut model sees it (marked stretches folded to one line each: "<stamp> (abandoned attempt to <stamp>, said again at <stamp> -- already removed, read straight past it)").

### F1.9 Mark retakes (Gaming style)

<sub><!-- back -->[← F1.8](#f18-fix-the-transcripts-blocks-of-ppolicyfixblocklines--25-lines) · [↑ 04 Prepare](#04--prepare) · [all flows](11-flow-index.md#3-all-flows) · [F1.10 →](#f110-repair-the-joins-lecture-style)</sub>

```text
 spoken lines  ──► fewer than 4? ──► an empty retakes.tsv, done
        ▼
 brief: every spoken line numbered, "(N.Ns pause)" at gaps ≥ P.policy.retakePauseSeconds,
        "--- the recording stops here; the next one begins ---" at a source change
        ▼
 P.policy.retakeRuns identical calls (thinking off), each its own cache slot ──► pooled, deduped
        │      tools: mark_abandoned(from, to, again) · unmark · finish
        │      the tool refuses: out of range · "again" inside the stretch · past the timeline
        │                       · farther than P.policy.retakeReachSeconds
        ▼
 verify against the words
        ├─ removal < P.policy.retakeMinSeconds                    ──► dropped (a breath, not an attempt)
        ├─ again = 0                                              ──► only for a whole take
        ├─ the repeated tail, fuzzy match (≥ 70 % run, ≤ 3 skips) ──► the mark trimmed to it
        ├─ a rephrase                                             ──► trimmed to the broken-off tail (≤ 6 s)
        └─ a refused mark                                         ──► both attempts re-heard by a second ASR pass
        ▼
 edges placed (F1.11) ──► marks merged
        ▼
 more than P.policy.retakeCeil of the speech would go? ──yes──► everything refused
        ▼
 retakes.tsv (empty on purpose when none)
 ">>> retakes: N abandoned stretch(es), m:ss of speech, kept out of the cut"
```

S1 Spoken lines (< 4 → empty `retakes.tsv`). S2 Brief: spoken lines numbered, "(N.Ns pause)" at gaps ≥ P.policy.retakePauseSeconds (1.5 s), "--- the recording stops here; the next one begins ---" at source changes. S3 P.policy.retakeRuns (3) identical calls (thinking off), each its own cache slot (run index in the key), answers pooled, deduped. Prototype: strict JSON `{"abandoned":[{"from","to","again"}]}`, 1-based line numbers. **Tools**: `mark_abandoned(from, to, again)`, `unmark`, `finish` (tool validates: range, `again` inside the stretch, beyond the timeline, farther than P.policy.retakeReachSeconds 180 s). S4 Verify against the words: removal < P.policy.retakeMinSeconds (0.3 s) dropped; `again = 0` only for a whole take; repeated tail found by fuzzy match (≥ 70 % run, ≤ 3 skips; words equal, prefix ≥ 3 bytes, or one edit), mark trimmed to it; rephrase trimmed to the broken-off tail (fragments ≤ 6 s); refused marks re-heard by a second ASR pass on both attempts. S5 Place edges ([F1.11](#f111-place-the-edges-of-a-mark)), merge marks. S6 > P.policy.retakeCeil (40 %) of the speech would go → refuse everything: "!!! retakes: <m:ss> of <m:ss> of speech called abandoned -- refused, nothing is marked"; empty `retakes.tsv` written. S7 Write `retakes.tsv` (empty on purpose when none); log ">>> retakes: N abandoned stretch(es), m:ss of speech, kept out of the cut".

### F1.10 Repair the joins (Lecture style)

<sub><!-- back -->[← F1.9](#f19-mark-retakes-gaming-style) · [↑ 04 Prepare](#04--prepare) · [all flows](11-flow-index.md#3-all-flows) · [F1.11 →](#f111-place-the-edges-of-a-mark)</sub>

```text
 seams = the source changes          none ──► ">>> text edit: one recording, no seam to repair"
        ▼
 ┌ per seam, thinking ON ─────────────────────────────────────────────────────────────────┐
 │ JOIN k of n.                                                                           │
 │ BEFORE (the end of the take that was interrupted):  … the last 140 words …             │
 │ AFTER (the beginning of the take that follows):     … the first 140 words …            │
 └────────────────────────────────────────────────────────────────────────────────────────┘
      … das heißt, hier habe ich das Über-  ┃  Das heißt, hier habe ich das Datum der …
      ◄──── P.policy.seamReachWords ───────►┃◄──── P.policy.seamReachWords ────►
                                          join
        ▼  tools: drop_words(side, count) · keep_join · finish
        ▼  prototype: {"joined": "…"} matched backwards against the PRINTED words,
           preferring the later saying
   one stretch, at the join within P.policy.seamSnapWords?  ──no──► refused, nothing removed there
        │ yes    (other stretches ≤ P.policy.seamNoiseWords are respellings, ignored)
        ▼
   ≤ P.policy.seamMaxWords and ≤ P.policy.seamCeil of what was shown? ──no──► refused
        │ yes
        ▼
 words repeated straight across a cut deduped (3 words either side; the earlier one goes)
        ▼
 marks ──► edges (F1.11) ──► merged ──► final.txt (|cut N| at every join) + retakes.tsv, together
 ">>> text edit: N of M words removed in K stretch(es)"
```

S1 Words of every recording except the narrator mic (< 4 → empty). S2 Seams = source changes; none → ">>> text edit: one recording, no seam to repair -- every word stands". S3 Per seam (thinking ON): "JOIN k of n." + "BEFORE (the end of the take that was interrupted):" + last P.policy.seamReachWords (140) words + "AFTER (the beginning of the take that follows):" + first 140 words. **Tools**: `drop_words(side, count)`, `keep_join`, `finish`. Prototype: `{"joined": "…"}` matched backwards (preferring the later saying); dropped words must be one stretch at the join (snap 3 words; other stretches ≤ 2 words are respellings); refusals logged "!!! text edit: join k: <reason> -- nothing removed there" (no words; not at the join; several stretches; > P.policy.seamMaxWords 40 or > P.policy.seamCeil 60 %). Cached only when usable. S4 Dedupe words repeated straight across a cut (3 words either side; earlier goes). S5 Marks from dropped runs; edges ([F1.11](#f111-place-the-edges-of-a-mark)); merge. S6 Write `final.txt` (surviving words as written; `|cut N|` / `|cut|` at every join) and `retakes.tsv` together; log ">>> text edit: N of M words removed in K stretch(es)".

### F1.11 Place the edges of a mark

<sub><!-- back -->[← F1.10](#f110-repair-the-joins-lecture-style) · [↑ 04 Prepare](#04--prepare) · [all flows](11-flow-index.md#3-all-flows) · [F1.12 →](#f112-hand-edit-the-text-lecture)</sub>

```text
  the last surviving word                              the retake's first word
   ▁▂▄▆█▆▄▂▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁                    ▁▁▁▁▁▁▁▂▄▇█▆▃▁
          └ word end                                       └ word start
            ├─ 0.08 s pad                                  │
            ├─ follow the word's own sound, up to 0.25 s    │
            ├─ then the quietest moment within 0.4 s        │
            ▼                                               ▼
        cut lands here                          resumes just before it, when nothing
                                                was said in between
  no word times at all: the mono envelope alone — reach 0.8 s, pad 0.05 s, late stamp 0.6 s
  floor = the 20th percentile of ±4 s, raised to max(3×, +8)     tail level = peak − 12 dB
```

With word times: cut ends after the last surviving word (0.08 s pad; follows the word's own sound up to 0.25 s, then the quietest moment within 0.4 s), resumes just before the retake's first word when nothing was said between. Without: mono envelope alone (reach 0.8 s, pad 0.05 s, late stamp 0.6 s). Envelope floor = 20th percentile of ±4 s raised to max(3×, +8); tail level = peak − 12 dB.

### F1.12 Hand-edit the text (Lecture)

<sub><!-- back -->[← F1.11](#f111-place-the-edges-of-a-mark) · [↑ 04 Prepare](#04--prepare) · [all flows](11-flow-index.md#3-all-flows) · [F1.13 →](#f113-the-sessions-word-list-shared-by-retakes-joins-finaltxt-and-subtitles)</sub>

```text
 prepare/transcript/final.txt        the words of the video, |cut N| at every join
        │  the user deletes words in an editor (the marks may stay or go)
        ▼
 next Cut ▶ ──► final.txt newer than retakes.tsv? ──no──► the model's marks stand
        │ yes
        ▼
 the marks are remade from the TEXT, no model asked
 ">>> final.txt was edited after Prepare -- the marks are remade from it, no model asked"
        ├─ a word that was never said ──► dropped, with a warning
        └─ more than 40 % removed     ──► refused, nothing is marked
```

S1 User edits `prepare/transcript/final.txt` (deletes words; join marks may stay or go). S2 Next Cut ▶, if its mtime is after `retakes.tsv`'s: marks remade from the text, no model (">>> final.txt was edited after Prepare -- the marks are remade from it, no model asked"); never-said words dropped with a warning; > 40 % removed refused. REVIEW: a rewrite SHOULD also offer this file in an editor on the Prepare page, join marks as clickable seams.

## 3. Data written

See [`01-project-and-files.md` §1](01-project-and-files.md#1-layout) (prepare/…).

## 4. Parameters used

P.policy: minTakeSeconds, retakePause, retakeRuns, retakeReach, retakeMin, retakeCeil, retakeFrag, againReach, seamReach, seamMaxWords, seamCeil, seamSnap, seamNoise, describeFramesPerReq, describeRecentEvents, describeCtxSegs, describeCtxWindow, fixBlock. P.audio: chunk sizes, silence threshold, diarization windows and anchor sizes, merge gap/length. Engineering: frame workers, quality, scratch handling. Full list in [`10-parameters.md`](10-parameters.md).

## 5. Rules

- Every stage skips when its output exists; resume markers written last and whole; scratch dirs survive failure, go on success.
- Server's own documents never rewritten; derived files live beside them.
- Only usable answers are cached; the fix pass changes text only; the retake pass answers in line numbers; the join pass can only delete; "prefer the later saying" is the direction of the match, not a request to the model.
- Marks are markings, never deletions; an empty marks file means "asked, none".
- No aligner is a working setup; no ASR word times and no aligner is a hard failure named plainly; diarization failure is fatal after the ladder; no speech is a real case at every layer.
- ⏸ resumes; ⏹ restarts only the describing, and only on the next ▶.

### F1.13 The session's word list (shared by retakes, joins, final.txt and subtitles)

<sub><!-- back -->[← F1.12](#f112-hand-edit-the-text-lecture) · [↑ 04 Prepare](#04--prepare) · [all flows](11-flow-index.md#3-all-flows) · [F2.1 →](05-cut.md#f21-play-the-recording-)</sub>

```text
 aligner (or ASR) tokens          " sen" "tence"      whole words or pieces:
        ▼                                             a leading space anywhere means piece tokens
 glued into words on the session clock                punctuation-only tokens ride the previous word
        ▼
 dressed with the case and punctuation of the RAW transcript          (look-ahead 8 words)
        ▼
 re-dressed with the spelling the FIX pass settled on                 (resync window 6 words)
        │  a whole respelling is printed on the first word of the run, the rest emptied
        │  a line with no word in common is left alone
        ▼
 one word list for the session — times never change, only the written form
        ▼
 the join pass shows the model THIS spelling and matches the answer back against these tokens
 (a word printed as two owns both; it survives if either does)
```

After the fix pass, one word list for the whole session: aligner (or ASR) tokens glued into words on the session clock (a leading space anywhere → piece tokens, none → whole words; punctuation-only tokens ride the previous word without moving its end), dressed with the raw transcript's case and punctuation (look-ahead 8), then re-dressed with the fix pass's spelling (resync window 6; a whole respelling printed on a run's first word, the rest emptied; a line with no word in common left alone). Times never change, only the written form. The join pass shows the model this spelling and matches its answer back against exactly these tokens (a word printed as two owns both; survives if either does).

## 6. Details confirmed against the code (verification pass)

- **Diarization ([F1.3](#f13-per-source-audio--text--word-times--speakers--segments) S5) is a two-pass anchored scan**: pass 1 scans windows at hop ⅔ window (min 10 s) — "finding voices i/n"; anchor window = most voices with ≥ 4 s of speech, ties → best-represented quietest voice; up to min(12, win/4) s of each voice cut and concatenated into an anchor (">>> [base] anchor: N voice(s) in X s of a Y s window -- Z s of new audio each"); pass 2 prepends the anchor to every (win − anchor − 1) s window — "placing speakers i/n" — and matches slots to anchor blocks one-to-one by overlap ≥ 0.5 s, strongest claim first; unmatched slots get their own ids; < 15 s of new audio per window → error "anchor too long (X s of Y s window)"; no voice long enough → ">>> [base] diarization: no voice long enough to anchor -- no turns" and `[]`.
- **Segments**: a word between turns takes the running segment's speaker; a segment started unknown is renamed once one word identifies it; `transcript.srt` prefixes each cue with `[<speaker>] ` unless unknown. Hard failures: "N words carry no usable start_sample/end_sample -- the answer changed shape"; "<ASR> transcribed this recording but timed no words, no aligner is registered to time them -- register one (task \"align\") -- or use an ASR that answers with word timings" (or "…and the aligner left no times either, which the line above this one says why").
- **meta.env**: VIDEO_FILE/BASE only when there is footage; AUDIO_FILE/BASE from the recording tagged narrator 1; INTERVAL and SCALE always.
- **Separation**: refusals "stems A, B -- none of them is the voice" and "only \"vocals\" came back -- the recording without the voice is the other half of this, and there is no other half"; ">>> [base] already split" on resume; a video's rest muxed with picture copied, sound as FLAC.
- **Describe**: the speech block's literal headings `--- context before (do not describe) ---`, `--- spoken during these frames ---`, `--- context after (do not describe) ---` with `(none)` / `(no speech during these frames)`; a frame matches the event stamped within half an interval; a batch's first frame with no history is written "Calm; same view."; a reply with no label stored "(no event line: …)"; an old-shape single EVENT becomes the first frame's line.
- **Fix**: ">>> [base] block i/n failed validation, keeping original lines"; ">>> [base] N block(s) answered from the cache"; `offsets.tsv`: one row per footage/recording pair, each logged ">>> offset: A starts X s into B". `session.txt` lines read `[<start>s-<end>s | mm:ss] LABEL: text` with LABEL = EVENT, NARRATOR (the narrator's mic) or the speaker id.
- **Retakes**: "!!! retakes: run i: … -- its answer is set aside"; "… -- going on with N"; ">>> retakes: N of M run(s) answered from the cache"; ">>> retakes: none".
- **Joins**: refusals also "the stretch left out (\"…\") stops N words short of the join" / "…starts N words past the join" beyond seamSnap; ">>> text edit: N join(s) repaired, M from the cache"; one line per mark ">>> text edit: a-b goes (\"…\")"; dedupe note "<t>: \"…\" said again straight after the cut -- the earlier one goes"; one recording still writes `final.txt` with every word and an empty `retakes.tsv`. Hand edit: "!!! text edit: N word(s) in the text were never said -- left out, there is no sound for them"; "!!! text edit: N of M words removed -- refused, that is not an edit, nothing is marked" (the same 40 % ceiling as retakes).
- **Prompt bench**: each row's tooltip says what the job gets and answers; the mark's tooltip "Your wording is kept in ~/.config/naivepost/prompts, so a newer built-in prompt will not replace it. Reset puts it back."; Reset live only while this machine holds an edit ("Put the built-in wording back" / "This is the built-in wording, unchanged"); legacy [`prompts/<key>/General.txt|Default.txt`](prompts) are read once and rewritten as [`prompts/<key>.txt`](prompts); an empty prompt file is no prompt.
- **Readouts**: "no input files — add some above" (tooltip "(no sources)"); Outputs "nothing yet" / "N files, size" with "newest <ago>".

<!-- nav -->
---
[← 03 The shell](03-shell.md) · [↑ top](#04--prepare) · [↑ Contents](README.md) · [05 Cut →](05-cut.md)
<!-- /nav -->
