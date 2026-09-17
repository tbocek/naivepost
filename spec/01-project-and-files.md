# 01 — The project on disk

A project is a folder ending in `.naivepost`. The folder **is** the output folder: nothing stores where work goes, so nothing can disagree. Every file inside is readable.

## 1. Layout

```
<name>.naivepost/
  naivepost.json                     the project (§2)
  sources/                           copies of added files (when "copy into project" is on)
  stems/                             voice-separation products: <base>.split-voice.wav, <base>.split-novoice.wav|.mkv
  assets/                            SVG cards and other inserts; CARDS.md guide; built-ins written on first use
  prepare/
    inputs/meta.env                  VIDEO_FILE, VIDEO_BASE, AUDIO_FILE, AUDIO_BASE, INTERVAL, SCALE
    inputs/<source>/                 voice16k.wav, transcript.txt, transcript.tsv, transcript.srt,
                                     words.json, asrchunks.json, words.aligned.json, turns.json
                                     (asr/, diar/ scratch while running)
    inputs/frames/<source>/          <YYYY-MM-DD_HH-MM-SS[-n]>.jpg per interval, .interval marker
    describe/<source>/events.tsv     one row per frame; state.txt; .llmframes/ (scaled frames)
    transcript/<source>/             transcript.fixed.tsv | commentary.fixed.tsv, subtitles.srt
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
  produce/clips/                     per-clip encodes, final.srt, final.<code>.srt, concat.txt
  produce/final.<container>          the video; final.stamp; final[.<code>].srt/.vtt; final.jpg; final.html
  produce/publish/publish.json       the upload text and thumbnail state (§5)
  produce/publish/thumbnail.png, thumbnail-plain.png, thumbnail.stamp, description.txt
  llm/<MMDD-HHMMSS>-<step>.html      one readable page per run
```

Directories are created 0755, files 0644, except prompts and the settings file on the machine (0700 / 0600).

Paths written into any project file go through one rule: inside the project → `project:<slash-relative>`; under the application root → root-relative; else absolute. One reader resolves them.

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
| style | a style name from the style table (`00-principles.md` §5); prototype: "read" or "" | "" |
| run_steps | ticked chain pages | ["prep"] |
| language | ASR language code | en |
| no_narration | narration off | false |
| reference_sources | inverted on purpose: absent = copy sources in | false |
| vid_dir / aud_dir | last chooser folders | root/input_video, root/input_audio |
| context | the User Context, verbatim | "" |
| policy | the editing policy (`10-parameters.md` §2) with a `source` per field (user / model / default) | defaults |
| produce | encoder settings (`08-produce.md` §2); `out_file` is never stored | defaults |
| publish | upload text and thumbnail state (§5) | absent |

Legacy keys read and migrated once, never written: `videos`, `audios` (first recording → narrator 1), `in_dir`, `out_dir`, `*_hints` (folded into the prompts with their historical lead-ins), `prompts` (adopted where the machine has none), `pitch`.

Autosave: the project is marshalled every 2 s and written only when the bytes differ from the last write. Also flushed on window close. There is no "unsaved changes" prompt.

## 3. cut/cut.json

```json
{
  "segs": [{"s": 1.5, "e": 34.7, "cam": 0, "quiet": ["mic"]},
           {"s": 40.0, "e": 40.0, "ins": "project:assets/tier.svg?S=Dust II", "dur": 4.0, "mute": true},
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

Segment fields: `s`, `e` session seconds (`s == e` with `dur > 0` = spliced insert); `ins` asset path (project-relative; may carry `?key=value&…`; `copy:<seconds>` = a pasted stretch of the session); `dur` (spliced length); `rate` (written only by the render planner); `ss` (start inside an inserted sound); `mute` (spliced: silent; overwriting: the footage's sound stays); `cam` (picture row); `lane` (which recording an overlaid sound replaces; "" = everything audible); `quiet` (lanes this scene does not hear); `split` (starts at a Split border; never merged automatically).

Effects: see `06-effects.md` §1. `shift`/`rows`: hand corrections of where a recording sits and on which row. `lanes`: windows of a file given a row of their own. `nrows`: floor under the row count. `folds`: view only, ignored by the render. Legacy `sound` (whole-cut hearing) is migrated into per-scene `quiet` on first save.

`cut.json` existing is what unlocks the Narrate and Produce steps' ▶ (the tabs themselves are always openable). It is written by exactly one function.

## 4. narrate/narration.json

```json
{"entries": [{"s": 1.5, "e": 34.7, "at": 2.0, "text": "We start with …", "emotion": "calm", "pos": "", "roll": 0}],
 "silent": [{"S": 40.0, "E": 65.0}]}
```
`s`/`e` are the clip's bounds copied verbatim from the cut; `at` seconds from the clip's start; `text` "" = deliberately silent; `emotion` a delivery tag; `pos` caption placement top/center/"" (bottom); `roll` re-roll count (salts the TTS cache). `silent` = clips whose last line was deleted on purpose, kept by bounds. Entries are always sorted by (s, at).

TTS cache key (kept stable across versions on purpose): `25e<alpha>|[<roll>#][<voiceKey>|]<text>|<emotion or 8-float vector>`; file = first 8 bytes of SHA-1 as hex; seed = bytes 8..11.

## 5. produce/publish/publish.json

```json
{"frames": ["project:prepare/inputs/frames/a/2026-09-16_17-25-30.jpg"], "crop": {"x": 0.5, "y": 0.5}, "own": false,
 "title_box": {"cx": 0.5, "cy": 0.25, "wf": 1, "hf": 0.4}, "thumb_title": "…", "title_seeded": true,
 "texts": [{"cx": 0.3, "cy": 0.8, "wf": 0.4, "hf": 0.1, "text": "…"}],
 "title": "…", "prompt": "…", "negative": "…", "description": "…"}
```
The first frame is the base the image model edits; the rest are references. `own` = the thumbnail is a chosen frame, not drawn. `publish.json` existing means the upload text has been written; deleting the folder starts the text over.

## 6. Text formats

- `transcript.tsv` / `*.fixed.tsv`: `start\tend\tspeaker\ttext`, times `%.2f`.
- `session.tsv`: `start\tend\tsource\tspeaker|EVENT\ttext` on the session clock.
- `events.tsv`: `start\tend\ttext` per frame; a `same` row extends the previous row when read.
- `retakes.tsv`: `S\tE\tAgain\tTo\tText` (3-, 4- and 5-column files accepted).
- `final.txt`: the surviving words as written, `|cut N|` or `|cut|` at every join.
- `meta.env`: `KEY=VALUE` lines; its existence says the sources have been read.
- `words.json`: the ASR server's own document (`{"text", "words":[{word,start_sample,end_sample}]}`); `words.aligned.json`: `{"words":[…]}` from the aligner; `turns.json`: `[{start_sample,end_sample,speaker_id}]`.
- `.interval`: `<interval>|<scale name>`.
- `cache/waves/*.wave`: magic `AWV4`, header {chans u8, hz u16, count u32, size i64, mtime i64}, then peak bytes.

## 7. Machine files

- `~/.config/naivepost/llm.conf` (0600): bash-sourceable `KEY="value"`; keys in `03-shell.md` §6.
- `~/.config/naivepost/prompts/<key>.txt`: a prompt edited on this machine (only when it differs from the shipped text).
- `~/.config/naivepost/hang-*.txt`: watchdog dumps.
- `~/.local/share/applications/ch.bocek.naivepost.desktop` and the mime XML for `*.naivepost` (installed when the app is not run from a build cache or a Flatpak).

## 8. Session clock

Every source is placed by the timestamp in its file name; the earliest is zero. Accepted spellings (year first, century 19/20): `2026-08-08 19-55-15`, `…-20260808-195900-0`, `VID_20250814_213311`, `20250814213311`, `2025.08.14 - 21.33.11.03`, `2026-08-08 at 7.55.15 PM`, `2026-08-08T19:55:15`, and bare unix seconds 2017–2033. A file with no stamp sits at the session start (never at its mtime) and its row shows a warning. Nothing stamped → everything starts at 0:00. The clock is always computed over the whole session; a subset would move zero. Frames are named from the frame number (`start + (n−1)·interval`), never from a sorted position.
