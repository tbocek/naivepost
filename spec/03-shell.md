# 03 — The shell: window, tabs, run bar, log, settings, projects

## 1. The window

```
┌──────────────────────────────────────────────────────────────────────────────────────────┐
│ [＋New] [Open] [Save]  lecture-2026-09-16.naivepost     ( Prepare | Cut | Narrate | Produce )   [⟳] [⚙] [ⓘ] │  header bar
├──────────────────────────────────────────────────────────────────────────────────────────┤
│                                                                                          │
│                              ( the visible tab's page )                                  │
│                                                                                          │
├──────────────────────────────────────────────────────────────────────────────────────────┤
│ [▶ | 2 steps ▾] [⏹]  ▓▓▓▓▓▓░░░░░░░ describe 1/2: chunk 4/12   Inputs: 3 files…  Outputs: 41 files, 220 MB │  run bar
│ ▸ Log                                                              status line, right-aligned │
│   >>> run: Prepare → Cut                                                                 │  log (expander)
│   >>> prepare: 3 input files                                                              │
└──────────────────────────────────────────────────────────────────────────────────────────┘
```

- One window, title "Naivepost", default 1240×740. Header bar: New ("New project — name it, put it where you want it, and start over"), Open ("Load a project — sources, prompts and settings"), Save ("Save this project to a file"), the project name/path (fits: path + tab words → file name + tab words → file name + icons only as the window narrows), the tab row as title widget, then Rescan ("Rescan inputs and outputs"), Settings ("Settings — the LLM and audio.cpp endpoints"), ⓘ (tooltip = the current tab's label and its help text).
- Tabs: Prepare (view-list), Cut (edit-cut), Narrate (microphone), Produce (multimedia). A locked tab is greyed, not disabled, its tooltip is the reason, and clicking it bounces and puts the reason on the status line. Cut is locked until a source is marked footage ("Add footage on the Prepare step first — the cut is laid out from the recordings"). Narrate and Produce are never locked; their ▶ refuses without a cut.
- Bottom: the run bar (§2), the "Inputs:" and "Outputs:" readouts the visible tab provides, then the log expander whose header carries the status line. The log is read-only monospace; lines start `>>> `, `!!! ` or four spaces; paths in it are clickable and open in the file manager.
- Closing the window flushes the narration autosave, the red line, the project and the prompts, then closes. No prompt.
- A watchdog dumps every goroutine's stack after 3 s without a GUI heartbeat (engineering constant).

### F0.1 Switch tab
S1 Click a tab (or the chain moves to it). S2 If locked: bounce, status = the lock reason; stop. S3 Flush the narration autosave. S4 Show the page and its Inputs/Outputs readouts; sync ⓘ; the lone chain tick follows the page (F0.4 S6). S5 Page refresh: Cut rebuilds when Prepare's output changed since it was built; Narrate refits its lines to the cut; Produce refreshes its readouts and the publish panel.

## 2. The run bar

```
[ ▶  | 2 steps ▾ ]  [ ⏹ ]   ▓▓▓▓▓░░░░░░  speech: recognising 2/3  ·  frames: extracting 40%
                              ┌────────────────────┐
                              │ ☑ Prepare           │   the chain popover
                              │ ☑ Cut               │
                              │ ☐ Narrate           │
                              │ ☐ Produce           │
                              └────────────────────┘
```

- ▶ (suggested-action): "Run the ticked steps — or resume what is paused"; while a run or a page transport is busy it shows ⏸ "Pause". (Prototype: every control refresh rewrites the idle tooltip to "Run this step — or resume what is paused"; the rewrite uses one wording.) ⏹: "Stop the run or the playback — ⏸ is what parks one to carry on later". Chain button label: none / "N steps" / all; tooltip "Which steps ▶ runs, in this order. Narrate skips itself when the video has no narration, and Cut skips itself when the cut has hand edits."
- Progress bar: two tracks summed (0 = speech/describe, 1 = frames/fix); the text is each track's line joined by "  ·  ", e.g. "describe 1/2: chunk 4/12", and "<job> done" for a finished track; tooltip per track "<job>: task i of n, k waiting" / "…, none waiting" / "<job>: N task(s), all done"; with nothing queued the standing tooltip "The run: the job, which of the run's jobs it is, and the task it is on" stays. A model thinking with nothing countable pulses the bar.

### F0.2 Press ▶
S1 If a run is under way → toggle pause ("pausing after the current stage…" / "resumed"); stop. S2 If the visible tab's transport is playing or has been started (Cut once its preview started; Narrate once its preview started) → toggle that transport; stop. S3 Snapshot the sources. S4 Run the chain (F0.4).

### F0.3 Press ⏹
S1 If the page transport is playing or cued → stop it, status "playback stopped". S2 Else if no run → nothing. S3 Else set the stop flag, cancel the run context (aborts model and audio calls), kill registered subprocesses; status "stopping…". S4 Pause/stop take effect between subprocesses only; a stopped subprocess is not a failure. S5 A stop that reached Describe arms "describe from the start" for the next Prepare run.

### F0.4 The chain
S1 Refuse if busy ("a run is already active — stop it first (⏹)"). S2 Nothing ticked → "nothing ticked beside ▶ — tick the steps to run". S3 Log ">>> run: Prepare → Cut → Produce". S4 For each ticked step in page order: skip Narrate when narration is off (">>> run: Narrate skipped — this video has no narration"); skip Cut when it has hand edits (">>> run: Cut skipped — the cut has hand edits, which are kept"); otherwise switch to the page synchronously (F0.1), log ">>> run: <Name>", run the page's step (Prepare F1.1, Cut F2.14, Narrate F4.1, Produce F5.1). A step that declines is skipped, not waited for. S5 A stop ends the chain. REVIEW: in the prototype a step that FAILS logs its failure and the chain walks on to the next ticked step; the rewrite SHOULD stop the chain on a failure. S6 End line, mirrored on the status line: ">>> run: all done in T — Prepare 2m 03s, Cut 12s" (status "all done in T") / ">>> run: stopped after T — … — N step(s) left undone" (status "stopped after T") / ">>> run: T — … — N step(s) left undone" when steps were skipped or declined (status "ran T, N left undone"); no line for a single finished step. Durations read "45s" under a minute, else "9m 12s". S7 The lone tick follows the page: when exactly one step is ticked and the user opens another tab, the tick moves there quietly and is saved.

### F0.5 A run's bookkeeping (every step uses it)
S1 startRun: running, flags cleared, a fresh cancel context, queue reset (also closes the model log page), controls, log expanded. S2 Each stage: `qJob(track, name, phase, of)`, `qPush(track, n, kind)`, per task `qTake` (also for tasks skipped because their output exists), `prog(track, fraction, text)`, `qDone(track, share)`. S3 Between subprocesses: `checkpoint()` (pause polls every 200 ms; stop returns the stop error). S4 endRun: running off, controls, the chain continues, audio models unloaded off-thread.

## 3. Projects

### F0.6 Open at start
S1 A file handed by the desktop (only the first); else S2 the last project remembered for this root in the settings file (if it still exists); else S3 `<root>/session.naivepost` if it exists; else S4 `<root>/project.json` (legacy, adopted into a folder); else a blank session at `<root>/session.naivepost`.

### F0.7 Derive the editing policy (REVIEW — new)
Runs after the User Context changes (debounced, on the next ▶ or on demand from the policy form). S1 If the context is empty → every policy field is its default; stop. S2 Ask the LLM (thinking off) with `tool:get_context` and `tool:set_policy`, system prompt "policy" (new, `prompts/` to add): read the context and set only the fields it speaks to. S3 Each `set_policy` is validated (known field, in range) and stored with `source: model` and its `because`. S4 Fields the user set by hand (`source: user`) are never overwritten. S5 The policy form (reachable from the ⚙ menu and from each tab's ⓘ) shows every field, its value, its source and the reason.

```
┌ Editing policy ──────────────────────────────────────────────┐
│ field                     value   source   because           │
│ minTakeSeconds            2.0     default                    │
│ targetLengthSeconds       720     model    "about 12 minutes"│
│ keepSwearing              yes     model    "keep it raw"     │
│ deadAirMaxSeconds         8.0     user                       │
│ …                                                             │
│                                   [Reset to defaults] [Close] │
└──────────────────────────────────────────────────────────────┘
```

### F0.8 New project
S1 Refused during a run ("stop the run first — a new project would pull its inputs out from under it"). S2 If the session is empty → S4. S3 Confirm "Start a new project?" with the detail "The sources, the session context and every prompt edit go back to empty. Files already written to the output folder are left alone." (+ "<base> stays on disk as it is…" or "This session has never been saved under a name of its own…"); button "Start new…". S4 Save dialog "New project", default name today's date (-2, -3 … while taken), `.naivepost` appended. S5 An existing project at that name → "<base> is a project already — open it, or pick another name". S6 Apply the blank project through the same apply list a load uses; chooser folders follow the chosen folder when it is outside the root; save; status "new project — <base>"; log ">>> new project <path> -- the session is empty; outputs on disk are untouched".

### F0.9 Open a project
S1 Folder chooser "Open a project" (legacy files open by double-click or command line only). S2 Adopt a legacy file into a folder if needed (staged; refuses when the target exists or a stale `.adopting` is left). S3 Set the project path and output folder first, then apply the project (sources, missing ones logged "!!! <file> is not there any more -- dropped from the session", interval, scale, style, chain, language, prompts, context, narration flag, reference flag, hints, produce, publish). S4 Migrate old folder names; refresh every page; remember the project for this root.

### F0.10 Save as
S1 Refused during a run ("stop the run first — saving under a new name moves the folder it is writing into"). S2 Save dialog "Save the project". S3 A different name renames the whole folder (never a copy; a rename onto a non-empty folder fails and the files stay; log where). S4 Status "project saved".

### F0.11 Rescan
S1 Drop sources whose files are gone ("!!! dropped <path> -- it is no longer there" each); slot 1 is re-assigned if its holder went. S2 Refresh every tab's readouts; Cut rebuilds; Narrate re-reads its file. S3 Status "rescanned".

### F0.12 Add sources (from Prepare)
S1 File chooser "Add sources" filtered to audio and video (`.flac .wav .mp3 .m4a .aac .ogg .opus .wma .mp4 .mkv .mov .webm .avi .ts`). S2 If "copy into project" is on: copy each file into `sources/` as a run of its own (progress in bytes; `.part` then rename; a same-name-same-size file is not copied again; files already inside the project are added in place); else reference in place. S3 Add to the list (duplicates and non-media skipped; footage defaults to video; narrator slot 1 auto-assigned to the first untagged row, recordings before footage). S4 Remember the folder per kind. S5 Status "added N source(s)" / "added N of M — the rest were already in" / "already in the session — nothing added".

## 4. Sources list (lives on Prepare, specified here because the shell snapshots it)

```
┌ sources ─────────────────────────────────────────────────────────────┐
│ [Add source files…]  ☑ copy into project                              │
│ ┌──────────────────────────────────────────────────────────────────┐ │
│ │ 🎥 🎤1  2026-09-16 17-25-06.mkv                        [⇶2/2] ✂ 🗑 │ │
│ │ 🎥 🎤   2026-09-16 17-25-45.mkv                              ✂ 🗑 │ │
│ │ ▢  🎤2  mic.flac                                     ⚠       ✂ 🗑 │ │
│ └──────────────────────────────────────────────────────────────────┘ │
│ Freq: [ 1s ][−][+]   [Original ▾]   Language: [en]   Style: [Lecture ▾] │
└──────────────────────────────────────────────────────────────────────┘
```

Row controls, in order: 🎥 footage toggle (video only; "Footage — frames come out of this file and it can be cut. Off: it is only listened to…"); 🎤 narrator button cycling the free slots 1..N and back to none ("1 is the voice the narration is spoken in; 2–4 are the rest of the group"; slot 1 highlighted); the file name (ellipsized middle; tooltip = path); a track menu when the file holds ≥ 2 audio streams ("Track N — <title> (stereo|mono)"; the last ticked track cannot be unticked; each ticked track becomes a lane and is mixed like a separate recording); ⚠ when the name carries no timestamp (tooltip explains renaming or dragging into place on Cut); ✂ split-the-voice toggle (greyed on a split product); 🗑 remove from the session ("the file itself is left alone"). Every control's tooltip ends with the four-line legend of the row's symbols.

Rules: only a video may be footage; one row per narrator slot; two sources with one base name refuse the run ("A and B are both inputs/<base> -- rename one", status "A and B have the same name — rename one"); a missing file is dropped loudly. Slot 1 is filled automatically whenever nobody holds it — after an add, a removal, a rescan that dropped a row, or a load that had to strip a bad tag — choosing the first untagged recording, footage last; a loaded project with two rows in one slot or footage on an audio file has the offending flag cleared on the way in. Strings: "Add source files…" ("Add recordings or footage — several at once"); "copy into project" ("Ticked, an added file is copied into the project's sources/ folder, so the project holds everything it needs. Unticked, the file is referenced where it is: nothing is duplicated, and the session breaks if it moves."); the list's tooltip "Every file here is transcribed, and placed on the session clock by the timestamp in its name".

## 5. Settings dialog

```
┌ Settings ────────────────────────────────────────────────────────────────────┐
│ Writing ⓘ        Server:   [ai.jos.li                      ] ✓ [Test]        │
│                  API key:  [••••••••••••••••••••• 👁]                          │
│                  Model:    [Qwen3.8 (27B…)                 ] ✓ [Test]        │
│                  [Fetch models] [ (fetch models first) ▾ ]    [Use]          │
│ ─────────────────────────────────────────────────────────────────────────── │
│ Cutting ⓘ        ffmpeg:   [empty = /usr/bin/ffmpeg         ]   [Test]        │
│                  firefox:  [empty = /usr/bin/firefox; off = no web search] [Test] │
│ ─────────────────────────────────────────────────────────────────────────── │
│ Audio ⓘ optional Server:   [empty = http://127.0.0.1:8765  ]   [Test]        │
│                  API key:  [                             👁]                  │
│                  TTS model:[index-tts2] ASR model:[nemotron-asr] Diarization:[sortformer-diar] │
│                  Voice split:[bs-roformer] Forced aligner:[qwen3-aligner if the server has it] │
│ ─────────────────────────────────────────────────────────────────────────── │
│ Drawing ⓘ optional Server: [empty = http://127.0.0.1:1234  ]   [Test]        │
│                  API key:  [                             👁]                  │
│                                                            [Test All]        │
│ ▸ Log                                                                        │
└──────────────────────────────────────────────────────────────────────────────┘
```

No Save/Cancel: every box is written 600 ms after the last keystroke and on close ("settings saved to <path>"). Each Test reads what is typed, shows a spinner then ✓/✗ with the verdict as tooltip, and mirrors its lines into the main log as "settings: …". Test All runs every test. The ⓘ per section carries the long explanation of what the server is expected to speak.

### F0.13 Tests
- LLM: one completion "Reply with the single word: ok" (thinking off, max 16 tokens, 60 s) → "<model> answered in X s: “ok”".
- LLM vision: a generated 48×48 red square with "In one word: what colour is this square?" (120 s); the reply must contain "red", else "…it is not seeing the image. Prepare sends video frames to this model: it needs a vision model, served with its mmproj/vision file".
- Fetch models: GET /v1/models → dropdown; Use copies the id into Model.
- ffmpeg: resolves the binary (and ffprobe beside it), reads the version, scans filters (rubberband, subtitles, loudnorm, atempo, amix, adelay, alimiter) and encoders (libx264, libx265, aac, libopus).
- firefox: "off" is a success ("the model is offered no web search…"); otherwise the version and one real headless search.
- Audio: health + catalogue → "healthy in N ms, will narrate with <id>"; per model box: the id is served and declared for its task (clon/asr/diar/sep), with install hints; aligner: what aligns (none is a success: cut points come off the waveform; several: the preferred one named).
- sd.cpp: capabilities → "<weights> is loaded and can draw"; an OpenAI-shaped server is called out as not sd-server.

## 6. The settings file
`~/.config/naivepost/llm.conf`, `KEY="value"` lines, written whole: `LLM_SERVER, LLM_MODEL, LLM_API_KEY, AUDIOCPP_SERVER, AUDIOCPP_API_KEY, AUDIOCPP_VOICES, AUDIOCPP_ASR_MODEL, AUDIOCPP_DIAR_MODEL, AUDIOCPP_TTS_MODEL, AUDIOCPP_SEP_MODEL, AUDIOCPP_ALIGN_MODEL, FFMPEG, FIREFOX, SD_SERVER, SD_API_KEY, PROJECT_<n>_ROOT/FILE`. Precedence: the dialog box → this file → (legacy `<root>/llm.conf`, migrated once) → built-in default. The audio URL additionally honours `NAIVEPOST_TTS_URL` and `AUDIOCPP_SERVER` below the dialog; sd `SD_SERVER`. REVIEW: the rewrite MAY add `SUBTITLE_LANGUAGES` (code:tag:name list) and a `STYLES` table per `00-principles.md` §5.

## 7. The model exchange log (llm/)
One HTML page per run named `MMDD-HHMMSS-<first step>.html`; every call is a numbered section (meta line with model, thinking/execute mode and duration; each message with inline images; the reply with reasoning folded in a details block; tool calls and results listed; notes when cut off or empty). The app log gets two lines per call (">>> <step>: 451.4 kB of text and 0 image(s) went to the LLM"; ">>> <step>: 28.6 kB came back in 2m39s — cut off at the model's token limit" + ">>>   the reply begins: …") and, once per run, the clickable link ">>>   this run's exchanges, images included: llm/<name>". Recording never fails the call.

## 8. Details confirmed against the code (verification pass)
- **Subprocess logging**: every subprocess is written to the log before it runs as a shell would take it (quoted arguments) at the four-space indent; a failure repeats the command and the last 400 characters of its output inside the error.
- **F0.9 legacy adoption**: a path naming `naivepost.json` opens the folder holding it; adopting a legacy file stages `<name>.data` aside, rewrites absolute paths into that folder to `project:<rel>`, writes `naivepost.json`, removes the old file and renames into place (">>> <name> is a folder now, with the project inside it as naivepost.json — everything this session writes is in there"). A project carrying a legacy `out_dir` that is not the project folder loads with "!!! this project used to write into A and now writes into B -- the old folder is untouched". Folder migration runs in two passes (step1..step6 → named folders; inputs/ and understand/{describe,transcript} → prepare/…), one log line per move, failures logged and left in place, the emptied `understand/` removed.
- **F0.10**: a rename that will not go through logs "!!! could not move the output folder to <to>: <err> -- the N file(s) are still in <from>"; nothing is logged when there was nothing to move.
- **Settings file**: rewritten whole with its explanatory comments; cleared boxes are written back as the shipped defaults, except the four server URLs and the aligner id, where empty is a real answer. Legacy `~/.config/naivepost/settings.json` (`{"projects": {root: file}}`) is read, never written, only when `llm.conf` has no `PROJECT_*` lines. Legacy keys `AUDIOCPP_MODELS` (its `voices/` subfolder is the voices folder when `AUDIOCPP_VOICES` is absent), `PROMPT_*`, `SD_MODEL`, `AUDIOCPP_LANGUAGE` are read and ignored.
- **Settings dialog log**: collapsed until a test fails, which opens it.
- **Icons**: the icons/ tree is searched beside the binary, under the app root and in the working directory; when the desktop entry or the MIME package is (re)written the path is logged once and `update-mime-database` / `update-desktop-database` run in the background.
- **Exchange page**: written when the request goes out and appended to as the reply streams, then rewritten whole; verdicts "…came back in T[, after X of thinking]", "— cut off at the model's token limit", "— the model answered nothing at all", "the call failed after T: …"; the run link is logged once, on the first call.
- **Audio server messages**: a missing model is reported with the catalogue the server does serve and, only when the id is the shipped default, the exact `docker compose exec audio python3 tools/model_manager_v2.py install <pkg> --models-root models` command; a model served under another task is refused by name ("\"X\" on <url> is declared task \"Y\", but Prepare needs \"Z\" there"). A server refusing uploads (403) is told how to fix it (start with `--ui-management`, or mount the folder into the container); an upload answering 200 without a `path` is a failure.
- **Duration probing**: a file whose header carries no duration (a recorder killed mid-write) is measured by decoding it once; cached per path+size+mtime like every probe.
