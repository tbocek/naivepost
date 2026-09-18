# 01 — The project on disk

<!-- nav -->
[← 00 Principles and rewrite directives](00-principles.md) · [↑ Contents](README.md) · [02 Services and the tool catalogue →](02-services.md)
<!-- /nav -->

A project is a folder ending in `.naivepost`. The folder **is** the output folder: nothing stores where work goes, so nothing can disagree. Every file inside is readable.

## 1. Layout

```text
<root>/assets/                     SVG cards and other inserts, shared by every project under the root (a card is reused across sessions); CARDS.md guide; the two built-ins written when the insert chooser is first opened
<name>.naivepost/
  naivepost.json                     the project (§2)
  sources/                           copies of added files (when "copy into project" is on); <name>.part while a copy is in flight
  stems/                             voice-separation products: <base>.split-voice.wav, <base>.split-novoice.wav|.mkv
  prepare/
    inputs/meta.env                  INTERVAL, SCALE always; VIDEO_FILE, VIDEO_BASE when there is footage; AUDIO_FILE, AUDIO_BASE when narrator 1 resolves
    inputs/<source>/                 voice16k.wav, transcript.txt, transcript.tsv, transcript.srt,
                                     words.json, asrchunks.json, words.aligned.json, turns.json
                                     (scratch while running: asr/c00.wav… chunks, kept on failure, removed on success; diar/ s.wav, a00.wav…, anchor.list, anchor.wav, seg.wav, cc.list, win.wav)
    inputs/frames/<source>/          <YYYY-MM-DD_HH-MM-SS[-n]>.jpg per interval, .interval marker
    describe/<source>/events.tsv     one row per frame; state.txt; .llmframes/ (scaled frames)
    transcript/<source>/             transcript.fixed.tsv | commentary.fixed.tsv; subtitles.srt for video sources only
    transcript/session.tsv           the merged timeline (machine copy)
    transcript/session.txt           the timeline as sent to the cut model
    transcript/offsets.tsv           video, audio, offset seconds
    transcript/retakes.tsv           the marks
    transcript/final.txt             the words of the finished video (lecture style)
  cache/llm/<step>/<sha256>          cached model replies
  cache/waves/<lane>.wave            waveform envelopes
  cache/edges/                       mono envelopes for edge placement
  cut/cut.json                       the cut (§3)
  cut/line.json                      {"t": <session second>} — the red line
  narrate/narration.json             the lines (§4); narration.prev.json (one generation back)
  narrate/voice.txt, pitch.txt, takes.json, voice_ref_base.wav, voice_ref.wav
  narrate/tts/<16 hex>.wav           spoken lines; samples/<voice>_<12 hex>.wav
  produce/clips/                     per-clip encodes c<NNN>_<YYYY-MM-DD_HH-MM-SS> (footage) / c<NNN>_<stem> (inserts), c<NNN>_<stem>.srt burn cues, c<NNN>_<stem>.svg filled cards, c<NNN>_<stem>.frames/ baked animations, final.srt, final.<code>.srt, concat.txt
  produce/final.<container>          the video; final.stamp; final[.<code>].srt/.vtt; final.jpg; final.html
  produce/publish/publish.json       the upload text and thumbnail state (§5); a project written before the move keeps them in <name>.naivepost/publish/, read there for ever, never migrated
  produce/publish/thumbnail.png, thumbnail-plain.png, thumbnail.stamp, description.txt
  llm/<MMDD-HHMMSS>-<step>.html      one readable page per run
```

Directories 0755, files 0644, except prompts, the settings file and the machine's watchdog dumps (0700 / 0600).

Paths written into any project file follow one rule: inside the project → `project:<slash-relative>`; under the application root → root-relative; else absolute. One reader resolves them.

## 2. naivepost.json

```json
{
  "sources": [{"path": "project:sources/a.mkv", "footage": true, "narrator": 1, "sepvoice": false, "tracks": [0, 1]}],
  "interval": 1.0,
  "frame_scale": "original",
  "style": "read",
  "run_steps": ["prep", "cut"],
  "language": "en",
  "no_narration": false,
  "reference_sources": false,
  "vid_dir": "…", "aud_dir": "…",
  "context": "The weekly blockchain lecture …",
  "policy": { … },
  "produce": { … },
  "publish": { … }
}
```

| key | meaning | default |
|---|---|---|
| sources | in order; `footage` only on video; `narrator` 1..N (exclusive); `sepvoice` is a wish cleared when granted; `tracks` = audio stream indices in the session (empty = first) | [] |
| interval | seconds between frames; 0 = every frame; always written | 1.0 |
| frame_scale | preset name (original, 896w (LLM), 480p, 720p, 1080p) | original |
| style | a style name from the style table ([`00-principles.md` §5](00-principles.md#5-generalisations-proposed-review)); prototype: "read" or "" | "" |
| run_steps | ticked chain pages | ["prep"] |
| language | ASR language code | en |
| no_narration | narration off | false |
| reference_sources | inverted on purpose: absent = copy sources in | false |
| vid_dir / aud_dir | last chooser folders | root/input_video, root/input_audio |
| context | the User Context, verbatim | "" |
| policy | REVIEW (new, not in the prototype): the editing policy ([`10-parameters.md` §2](10-parameters.md#2-editing-policy-project-derived-from-the-user-context-by-f07-else-defaults)) with a `source` per field (user / model / default) | defaults |
| produce | encoder settings ([`08-produce.md` §2](08-produce.md#2-flows)); `out_file` is never stored | defaults |
| publish | upload text and thumbnail state ([§5](#5-producepublishpublishjson)) | absent |

Legacy keys read and migrated once, never written: `videos`, `audios` (first recording → narrator 1), `in_dir`, `out_dir`, `*_hints` (folded into the prompts with their historical lead-ins), `prompts` (adopted where the machine has none); inside `publish`: `base` (an index, turned into the frames list) and `title_off` (the old spelling of "not printed"). An old `pitch` key is ignored (the field is gone).

Autosave: the project is marshalled every 2 s, written only when the bytes differ from the last write, and flushed on window close. No "unsaved changes" prompt.

## 3. cut/cut.json

```json
{
  "segs": [{"s": 1.5, "e": 34.7, "cam": 0, "quiet": ["mic"]},
           {"s": 40.0, "e": 40.0, "ins": "assets/tier.svg?S=Dust II", "dur": 4.0, "mute": true},
           {"s": 60.0, "e": 65.0, "ins": "project:assets/sting.wav", "ss": 2.0, "lane": "mic"}],
  "aspect": "9:16",
  "fx": [{"kind": "zoom", "t": 12.0, "dur": 3.0, "trans": 1, "tout": 1, "cx": 0.5, "cy": 0.5, "hf": 0.6}],
  "shift": {"cam2": -1.25},
  "rows": {"cam2": 1},
  "lanes": [{"name": "cam-2", "src": "project:sources/cam.mkv", "at": 100.0, "off": 30.0, "dur": 20.0}],
  "nrows": 2,
  "folds": [[34.7, 40.0]]
}
```

Segment fields: `s`, `e` session seconds (`s == e` with `dur > 0` = spliced insert); `ins` asset path by the one path rule of [§1](#1-layout) (`project:…` for a file inside the project, root-relative for a card under `<root>/assets`, else absolute; may carry `?key=value&…`; `copy:<seconds>` = a pasted stretch of the session); `dur` (spliced length); `rate` (written only by the render planner); `ss` (start inside an inserted sound); `mute` (spliced: silent; overwriting: the footage's sound stays); `cam` (picture row); `lane` (which recording an overlaid sound replaces; "" = everything audible); `quiet` (lanes this scene does not hear); `split` (starts at a Split border; never merged automatically).

Effects: see [`06-effects.md` §1](06-effects.md#1-record). `shift`/`rows`: hand corrections of where a recording sits and on which row. `lanes`: windows of a file given a row of their own. `nrows`: floor under the row count. `lanes` rows always write `name`, `src`, `at`, `dur`; only `off` is omitted when 0. `folds`: view only, ignored by the render. Legacy `sound` (whole-cut hearing) is migrated into per-scene `quiet` on first save.

Existing segments unlock the Narrate and Produce ▶: the live editor's if it has any, else those parsed from `cut.json` — a file holding `{"segs":[]}` unlocks nothing, an unsaved editor with segments does; refusal: "no cut yet — build one on the Cut step first" (the tabs always open). One function writes `cut.json`.

## 4. narrate/narration.json

```json
{"entries": [{"s": 1.5, "e": 34.7, "at": 2.0, "text": "We start with …", "emotion": "calm", "pos": "", "roll": 0}],
 "silent": [{"s": 40.0, "e": 65.0}]}
```

`s`/`e` the clip's bounds, copied verbatim from the cut; `at` seconds from the clip's start — REVIEW: on a clip with a rate the prototype reads it three ways (clamped against the on-screen length when the line is written, a session offset when lines are refitted, divided by the rate again in the render), which cannot all be right; the rewrite MUST fix one meaning and state it here; `text` "" = deliberately silent; `emotion` a delivery tag; `pos` caption placement top/center/"" (bottom); `roll` re-roll count (salts the TTS cache). `silent` = clips whose last line was deleted on purpose, kept by bounds. Entries always sorted by (s, at).

TTS cache key (deliberately stable across versions): `25e<alpha>|[<roll>#][<voiceKey>|]<text>|<emotion or 8-float vector>`; file = first 8 bytes of SHA-1 as hex; seed = bytes 8..11.

## 5. produce/publish/publish.json

```json
{"frames": ["project:prepare/inputs/frames/a/2026-09-16_17-25-30.jpg"], "crop": {"x": 0.5, "y": 0.5}, "own": false,
 "title_box": {"cx": 0.5, "cy": 0.25, "wf": 1, "hf": 0.4}, "thumb_title": "…", "title_seeded": true,
 "texts": [{"cx": 0.3, "cy": 0.8, "wf": 0.4, "hf": 0.1, "text": "…"}],
 "title": "…", "prompt": "…", "negative": "…", "description": "…"}
```

The first frame is the base the image model edits; the rest are references. `own` = the thumbnail is a chosen frame, not drawn. An existing `publish.json` means the upload text is written; deleting the folder starts it over.

## 6. Text formats

- `transcript.tsv` / `*.fixed.tsv`: `start\tend\tspeaker\ttext`, times `%.2f`.
- `session.tsv`: `start\tend\tsource\tspeaker|EVENT\ttext` on the session clock.
- `events.tsv`: `start\tend\ttext` per frame; a `same` row extends the previous row when read.
- `retakes.tsv`: `S\tE\tAgain\tTo\tText` (3-, 4- and 5-column files accepted).
- `final.txt`: the surviving words as written, `|cut N|` or `|cut|` at every join.
- `meta.env`: `KEY=VALUE` lines; its existence says the sources were read. Prototype: the only reader (`loadMeta`) looks in the old place, `inputs/meta.env`, and nothing calls it — the file is for humans and scripts; the rewrite MAY drop it.
- `words.json`: the ASR server's own document (`{"text", "words":[{word,start_sample,end_sample}]}`); `words.aligned.json`: `{"words":[…]}` from the aligner; `turns.json`: `[{start_sample,end_sample,speaker_id}]`; `asrchunks.json`: a bare array `[{"s","e","text"}]` — the seconds each ASR request covered and exactly what came back for them. A recording under P.policy.minTakeSeconds is written as silence, no server: `transcript.txt` = "\n", `words.json` = `{"text":""}`, `turns.json` = `[]`, no `asrchunks.json`.
- `.interval`: `<interval>|<scale name>`.
- `cache/waves/*.wave`: magic `AWV4`, header {chans u8, hz u16, count u32, size i64, mtime i64}, then peak bytes.

## 7. Machine files

- `~/.config/naivepost/llm.conf` (0600): bash-sourceable `KEY="value"`; keys in [`03-shell.md` §6](03-shell.md#6-the-settings-file). Also holds state: `PROJECT_<n>_ROOT` / `PROJECT_<n>_FILE` pairs (last project per root), sorted so an unchanged save is byte-identical.
- `~/.config/naivepost/settings.json`: legacy, read once (never written) on the launch after the merge into llm.conf, to recover remembered projects when the conf has no `PROJECT_*` keys.
- The voices folder (`AUDIOCPP_VOICES`): the one folder read outside a project; no GUI box; default `/mnt/models/audiocpp/voices` (the dev box), inside Flatpak `$XDG_DATA_HOME/naivepost/voices`.
- `~/.config/naivepost/prompts/<key>.txt`: a prompt edited on this machine (only if it differs from the shipped text).
- `~/.config/naivepost/hang-*.txt`: watchdog dumps.
- `$XDG_DATA_HOME/applications/ch.bocek.naivepost.desktop` and `$XDG_DATA_HOME/mime/packages/ch.bocek.naivepost.xml` (type `application/x-naivepost-project`, glob `*.naivepost`), followed by `update-mime-database` and `update-desktop-database` — installed unless the app runs from a build cache or a Flatpak (Flatpak exports both from the manifest; a file written from inside would point into the sandbox).

## 8. Session clock

Every source is placed by its file-name timestamp; the earliest is zero. Accepted spellings (year first, century 19/20): `2026-08-08 19-55-15`, `…-20260808-195900-0`, `VID_20250814_213311`, `20250814213311`, `2025.08.14 - 21.33.11.03`, `2026-08-08 at 7.55.15 PM`, `2026-08-08T19:55:15`, and bare unix seconds 2017–2033. An unstamped file sits at the session start (never at its mtime); its row shows a warning. Nothing stamped → everything starts at 0:00. The clock always spans the whole session; a subset would move zero. Frames are named from the frame number (`start + (n−1)·interval`), never a sorted position.

<!-- nav -->
---
[← 00 Principles and rewrite directives](00-principles.md) · [↑ top](#01--the-project-on-disk) · [↑ Contents](README.md) · [02 Services and the tool catalogue →](02-services.md)
<!-- /nav -->
