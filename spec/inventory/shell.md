# Inventory: application shell (window, run bar, project, settings, sources, LLM log)

Raw material for the spec, read off the prototype's Go source (gui/main.go, runbar.go, runchain.go, runqueue.go, pipeline.go, project.go, project_import.go, setup.go, widgets.go, icon.go, llmlog.go, headfit.go, ffprobe.go, seam.go, sources.go).

## A. Window and layout

### Application object
- GTK4, no libadwaita. App id `ch.bocek.naivepost`. Single instance: a second launch forwards its file to the running one; only the first file is opened.
- Root discovery: if `cwd/input_video` does not exist, root = the executable's grandparent directory. `vidDir = root/input_video`, `audDir = root/input_audio`, default project = `root/session.naivepost`.

### Window
- Title "Naivepost", default size 1240×740.
- CSS: `.stamp-warn` amber #e5a50a; `.test-ok` #26a269; `.test-bad` #c01c28; `.videoframe` background #101010; scale value/mark labels use theme fg; `.frame` rounded 6px; entries monospace.
- Layout top to bottom: header bar; vertical paned (page stack resizable / bottom box not); bottom box = separator, run bar row, log expander.
- Page stack: crossfade, non-homogeneous both ways; children prep, cut, narrate, produce.

### Header bar
Left: New (`document-new-symbolic`, "New project — name it, put it where you want it, and start over"); Open ("Load a project — sources, prompts and settings"); Save ("Save this project to a file"); project label (ellipsized, dim).
Right: Rescan ("Rescan inputs and outputs"); Settings ("Settings — the LLM and audio.cpp endpoints"); ⓘ image whose tooltip is the current step's label + help text.
Title widget = the tab row.

### Tabs
Hand-rolled linked toggle buttons (icon + word), one radio group.
| # | name | label | icon | tip | locked message |
|---|---|---|---|---|---|
| 0 | prep | Prepare | view-list-symbolic | The sources, their transcripts, their frames, and what the models make of them | never locked |
| 1 | cut | Cut | edit-cut-symbolic | Choose the clips the video is made of | Add footage on the Prepare step first — the cut is laid out from the recordings |
| 2 | narrate | Narrate | audio-input-microphone-symbolic | The narration, and the voice it is spoken in | Finish Cut first — narration is written for the cut's clips |
| 3 | produce | Produce | applications-multimedia-symbolic | Write the upload text, draw the thumbnail and render the video | Finish Cut first — there is no cut to produce a video from |
Each step also carries a long help text (the ⓘ tooltip) in main.go:36-334; carry over verbatim.
Gating: Cut locked unless at least one source is marked footage. Narrate and Produce are never locked (their ▶ refuses instead: "no cut yet — build one on the Cut step first"). Locked = greyed (dim-label on the child), tooltip = locked message; clicking bounces back and sets the status to the same sentence. If the visible step becomes locked the window returns to Prepare.
showStep(name): flush narration save; set tab; switch stack, inputs and outputs stacks; update run controls; sync help; followChainTick; per page: cut → rebuild if stale or no vids, else update inputs; narrate → refit, update inputs/outputs; produce → updateProduceInfo + publish refresh.

### Bottom bar
Row spacing 8, margins 4/2/8/8: [▶ | chain menu] linked; ⏹; progress bar (no text, expands, tooltip "The run: the job, which of the run's jobs it is, and the task it is on"); "Inputs:" + per-page inputs stack; "Outputs:" + per-page outputs stack. No volume slider here.

### Status line and log
- Status label right-aligned, ellipsized, dim, inside the log expander's header row ("Log" label + status).
- Log expander starts collapsed; a run expands it. Log pane: read-only monospace text view, word-char wrap, min height 220 (110 in settings), CSS frame. Expanding gives it vertical expansion; collapsing forgets the divider position.
- logf prints to stderr and appends to the view, scrolling to the end; logfIdle marshals to the GUI thread.
- Conventions: `>>> ` progress/decision, `!!! ` failure/warning, 4-space indent for detail (`    $ command`, `    path  (size)`).
- Clickable paths: tag `link` (#1a5fb4 underlined); click opens via the portal file launcher, fallback xdg-open ("portal launch failed (%v), trying xdg-open").

### Dialogs
- modal(title, detail, width, body, buttons): hand-rolled window, transient, modal, 16 px margins, heading + dim detail + body + right-aligned buttons.
- confirm(question, detail, okLabel, ok): width 420, OK destructive-action, Cancel focused so a blind Enter does nothing.
- File dialogs: pickFile, pickFiles, pickFolder, saveAs (with initial name); dismissal never calls back.

### Keyboard shortcuts
None at shell level. Page keys belong to Cut and Narrate.

### Close
Close request: flush narration save, flush the red line, flush project, flush prompts; then allow close. No unsaved-changes prompt.

### Autosave
Every 2000 ms: flushProject + flushPrompts. flushProject marshals the project and writes only if the bytes differ from the last written (bytes are the change detection). saveProjectNow sets the saved bytes before writing so an unwritable target is not retried every tick; logs "save project: %v" on error.

### Startup order
window → CSS → size → header → run controls → page stack + four builders → tab row + header fit watcher → log + status + expander → bottom → showStep("prep") → paned → set child → icons → migrate conf → load global prompts → project pick → updateGates → updateCutInfo → updateProduceInfo → startAutosave → show.
Project pick order: a file handed by the desktop; the last project remembered for this root (if it exists); root/session.naivepost if it exists; root/project.json (legacy, adopted).

### Rescan
Prune missing sources (log "!!! dropped %s -- it is no longer there"), updateGates, prep refresh, updateCutInfo, updateNarrateInfo (re-read narration from disk), updateProduceInfo, status "rescanned".

### Header fit
Three rungs as the window narrows: path + tab words → file name + tab words → file name + icons only. tabGap 6, headSlack 90, project name capped at 28 chars. Measured with Pango on idle, watching the surface width.

### Hang watchdog
Heartbeat every 200 ms on the GTK thread; a goroutine dumps every goroutine's stack after 3 s of silence to stderr and `<config>/hang-MMDD-HHMMSS.txt` (0600), once per hang.

### Desktop integration
Icon theme dirs: exe/icons, root/gui/icons, cwd/icons. Writes `~/.local/share/applications/ch.bocek.naivepost.desktop` (Name Naivepost; Exec with %f; MimeType application/x-naivepost-project; Categories AudioVideo;Video;AudioVideoEditing) and the mime XML (glob *.naivepost), then runs update-mime-database / update-desktop-database off-thread. Skipped for go-run builds and inside Flatpak.

## B. Run bar and transport

Buttons:
- ▶/⏸ (suggested-action): initial tooltip "Run the ticked steps — or resume what is paused"; busy → pause icon "Pause"; else "Run this step — or resume what is paused".
- Chain menu button: label none / "N steps" / all; tooltip "Which steps ▶ runs, in this order. Narrate skips itself when the video has no narration, and Cut skips itself when the cut has hand edits."; popover with four check buttons Prepare, Cut, Narrate, Produce.
- ⏹: "Stop the run or the playback — ⏸ is what parks one to carry on later"; sensitive when running, busy, or a page transport is cued.
busy = running and not paused, or the page transport is playing. setPlayIcon rule: one button in two states, never a dead ⏸.

▶ priority: (1) a run is under way → toggle pause ("pausing after the current stage…" / "resumed"); (2) the page transport is playing or has been started → toggle it (Cut editor when playing or started; Narrate when playing or started; Prepare and Produce have none); (3) otherwise snapshot sources and run the ticked chain.
runPageNow: prep → prepRun; cut → suggestClicked; narrate → narrateRun; produce → produceClicked.
⏹: playback playing/cued → stop it ("playback stopped"); else if running: stopFlag, cancel run context (aborts LLM calls), kill registered subprocesses; status "stopping…".
Pause takes effect between subprocesses (checkpoint polls every 200 ms); stop abandons. A stop that reached Describe arms a restart of describing on the next Prepare run.

### Run chain
Steps in page order. chainRun: refuse if busy; nothing ticked → "nothing ticked beside ▶ — tick the steps to run"; log ">>> run: Prepare → Cut → Produce"; chainNext.
chainNext: skip Narrate when narration is off (">>> run: Narrate skipped — this video has no narration"); skip Cut when it has hand edits (">>> run: Cut skipped — the cut has hand edits, which are kept"); else showStep(page) synchronously, log ">>> run: <Name>", run the page; a step that declined (not running afterwards) is skipped rather than waited for. End: chainEnd("done").
chainDone (from endRun): record "<Name> <time>"; stopped → chainEnd("stopped"); else next.
chainEnd lines: ">>> run: stopped after T — steps — N step(s) left undone"; ">>> run: T — steps — N step(s) left undone"; ">>> run: all done in T — steps". Time "%.0fs" under a minute else "%dm %02ds". No line for a single-step chain.
followChainTick: when exactly one step is ticked and it is not the page just opened, the tick moves to that page, quietly, then saved. Not suppressed during a run; suppressed while the chain itself moves pages.
Default ticks: Prepare only. Project field run_steps; absent → Prepare only.

### Skip-if-output-exists
Each stage checks its own file: frames `.interval` marker "%g|%s"; per source voice16k.wav, words.json (written last and whole), words.aligned.json, turns.json; separation both halves; describe per chunk in events.tsv then cache; fix per block cache; textedit/retake/translate cache; TTS wav per line; upload text publish.json; thumbnail stamp; video stamp. Existence, not content. qTake is still called for skipped work.

### Progress bar
Two tracks (0 = STT/describe, 1 = frames/fix); each reports an absolute contribution; the bar shows the clamped sum. Track line: "<job> [phase/of]: <what|kind> [taken/queued]" e.g. "describe 1/2: chunk 4/12", "speech: recognising 2/3". API: qReset, qPhase(base, share), qJob, qPush, qTake, qDone, prog. showProg sets fraction, status text and a tooltip per track ("%s: task %d of %d, %d waiting"). pulseUntilCounted pulses every 150 ms until something is counted. busy() sets "a run is already active — stop it first (⏹)". startRun: running, flags, fresh context, qReset, controls, expand log. endRun: running=false, controls, chainDone, unload audio models off-thread.
Prepare phases: separation 0..0.10 (when asked), ingest to 0.30, understand 0.30..1.0; inside ingest each track owns 0.5.
Copy-in progress reports directly from its goroutine, ticking every 0.5 %.

### Volume
volumeCtl on Cut and Narrate: icon + scale 0..100 step 1, 120 px, tooltip "preview volume — the players only; nothing that is rendered, and the same setting wherever it is shown"; one global previewVol (default 1.0), all scales mirrored.

## C. Project model

Project JSON (naivepost.json):
| field | json | notes |
|---|---|---|
| Sources | sources | [{path, footage, narrator 1..4, sepvoice (a wish), tracks []int}] |
| Videos/Audios | videos/audios | legacy, read only; audios[0] became narrator 1 |
| Interval | interval | seconds between frames; 0 = every frame; always written |
| FrameScale | frame_scale | preset name |
| Style | style | "read" = lecture; "" = ordinary session |
| RunSteps | run_steps | ticked chain pages |
| Language | language | ASR language; absent → "en" |
| NoNarration | no_narration | |
| RefSources | reference_sources | inverted on purpose: absent = copy sources into the project |
| VidDir/AudDir | vid_dir/aud_dir | chooser folders (storePath) |
| InDir/OutDir | in_dir/out_dir | legacy read only |
| *Hints | describe_hints … narrate_hints | legacy, folded into prompts once |
| Context | context | the user context box, stored in full |
| Prompts | prompts | legacy read only; adopted once per key |
| Produce | produce | prodSettings |
| Publish | publish | pubSettings |
blankProject: interval 1.0, frame scale "original", default produce settings.
Paths stored with prefix `project:` when inside the project folder (relative, slash-separated), root-relative when under root, else absolute.
A project is a folder `<name>.naivepost` containing naivepost.json and every step's work; the project IS the output folder. Unsaved session: root/session.naivepost. New-project default name: today's date, -2, -3 … while taken.

Data folder layout:
```
<name>.naivepost/
  naivepost.json
  sources/                      copied footage
  prepare/inputs/meta.env       VIDEO_FILE/BASE, AUDIO_FILE/BASE (narrator 1), INTERVAL, SCALE
  prepare/inputs/<source>/      voice16k.wav, transcript.txt/.tsv/.srt, words.json, asrchunks.json, words.aligned.json, turns.json (+ asr/, diar/ scratch)
  prepare/inputs/frames/<source>/  <stamp>.jpg per interval + .interval
  prepare/describe/<source>/events.tsv (+ state.txt, .llmframes/)
  prepare/transcript/<source>/transcript.fixed.tsv + subtitles.srt; session.tsv, session.txt, offsets.tsv, retakes.tsv, final.txt
  cache/llm/<step>/, cache/waves/, cache/edges/
  cut/cut.json, cut/line.json
  narrate/narration.json (+ voice.txt, pitch.txt, takes.json, voice_ref*.wav, tts/, samples/)
  produce/clips/, produce/final.<container> (+ .stamp, .srt/.vtt per language, .jpg poster, .html tag)
  produce/publish/ (publish.json, thumbnail.png, thumbnail-plain.png, thumbnail.stamp, description.txt)
  llm/                          one HTML page per run
```
Formats: meta.env KEY=VALUE; transcript.tsv 4 columns start end speaker text (%.2f); transcript.srt with [SPEAKER_NN] prefix; words.json the server's document; turns.json [{start_sample,end_sample,speaker_id}]; silence case transcript.txt "\n", words.json {"text":""}, turns.json []. Frames named `YYYY-MM-DD_HH-MM-SS.jpg` (+ -1, -2 within a second) from the frame number.

Legacy migrations: adoptLegacy (file → folder, staged via `.adopting`, rewrites absolute data paths to project:); migrateFolders (step1..6 → inputs/understand/cut/narrate/produce/publish → prepare/…); projectSources (videos/audios → sources, first recording = narrator 1); migrateHints (four notes folded into describe/fix/cut/narrate prompts with historical lead-ins).

Flows:
- New: refused during a run ("stop the run first — a new project would pull its inputs out from under it"); confirm unless the session is empty ("Start a new project?" … "Start new…"); Save dialog with a free name; existing project → "<base> is a project already — open it, or pick another name"; applyProject(blank), vidDir/audDir follow the chosen folder when outside root, status "new project — <base>".
- Open: folder picker "Open a project"; adoptLegacy; set projPath/outDir before applying (so project: paths resolve); log a moved out_dir; remember.
- Save As: refused during a run; Save dialog; renames the whole output folder (os.Rename; a rename onto a non-empty folder fails and the files stay; log where); status "project saved".
- Add sources: files dialog "Add sources" with media filter; copy-in unless referencing (copyInto with .part + rename, same-name-same-size skipped, progress in bytes, a run of its own); statuses "already in the session — nothing added" / "added N source(s)" / "added N of M — the rest were already in".
- applyProject order: dirs, sources (log missing), prune, interval, scale, style, chain, language, prompts, context, narration off, ref sources, hints, produce, publish.
- setProject: paths, migrateFolders, label, follow out dir, clear voice caches, load narration, prep refresh, produce info, publish refresh, refreshCut, gates.

Helpers: countOutputs, summarizeOutputs ("nothing yet" / "N file(s), size"), humanSize (MB/kB/B), humanAgo, logOutputs (dirs over 12 files collapse to a count), openFolder (mkdir first, portal then xdg-open).

Session clock: filename timestamp regexes (OBS `2026-08-08 19-55-15`, Quest, phone `VID_20250814_213311`, dashcam 14 digits, ShadowPlay, QuickTime, ISO; year first; century 19|20; am/pm) plus bare unix seconds 2017–2033. srcClock(all paths): stamped files sit at their moment, zero = earliest; unstamped files sit at zero; nothing stamped → zero 0. Always handed the whole session. Frames stamped from `start + (n-1)*interval` in local time.

## D. Settings dialog
Modal "Settings", 680 wide, a five-column grid (section | label | value | badge | Test). No Save/Cancel: every box is written 600 ms after the last keystroke, flushed on close; status "settings saved to <path>"; failure logged.
Sections and fields (placeholders/tooltips verbatim in setup.go:864+):
- Writing: Server (placeholder "empty = http://127.0.0.1:8731", Test "Ask this server and model for one short completion"); API key (password); Model (placeholder "model id exactly as the server lists it", Test = vision: "Show this model a small sample image and check it names what it sees"); Fetch models + dropdown + Use.
- Cutting: ffmpeg path (empty = PATH; ffprobe taken from the same folder; Test checks filters rubberband, subtitles, loudnorm, atempo, amix, adelay, alimiter and encoders libx264, libx265, aac, libopus); firefox path or "off" (web search; Test drives a headless search).
- Audio (optional): Server (empty = http://127.0.0.1:8765), API key, TTS model (index-tts2), ASR model (nemotron-asr), Diarization model (sortformer-diar), Voice split model (bs-roformer), Forced aligner (empty = qwen3-aligner where served, else any task "align"); each with a Test that checks the id is served for its task.
- Drawing (optional): sd.cpp Server (empty = http://127.0.0.1:1234), API key; Test reports the loaded weights.
- Test All; a log pane last.
Badge: idle / spinner / ✓ / ✗ with the verdict as tooltip. Tests read what is typed; the dialog log mirrors into the main log as "settings: …".
Test details: LLM one completion "Reply with the single word: ok" (temperature 0.6, max_tokens 16, thinking off, 60 s); vision sends a generated 48×48 red square (120 s) and expects "red"; audio health + catalog; per-model catalog/task checks with install hints; aligner lists what aligns (none is a success: "cut points come off the waveform"); ffmpeg version + capability scan; firefox version + a real headless search; sd.cpp capabilities.
Fetch models: GET /v1/models, 15 s; fills the dropdown; Use copies the id.
Settings file: `~/.config/naivepost/llm.conf` (0600, dir 0700), bash-sourceable KEY="value", written whole. Keys: LLM_SERVER, LLM_MODEL, LLM_API_KEY, AUDIOCPP_SERVER, AUDIOCPP_API_KEY, AUDIOCPP_VOICES, AUDIOCPP_ASR_MODEL, AUDIOCPP_DIAR_MODEL, AUDIOCPP_TTS_MODEL, AUDIOCPP_SEP_MODEL, AUDIOCPP_ALIGN_MODEL, FFMPEG, FIREFOX, SD_SERVER, SD_API_KEY, PROJECT_n_ROOT/FILE (remembered last project per root). Legacy read-only keys: AUDIOCPP_MODELS, PROMPT_*, AUDIOCPP_LANGUAGE, SD_MODEL; legacy `<root>/llm.conf` migrated once. Defaults: voices /mnt/models/audiocpp/voices (Flatpak: data home), model ids as above; the aligner deliberately undefaulted; servers/keys/ffmpeg/firefox empty = real answers. Env vars: only XDG/HOME; audio URL also honours NAIVEPOST_TTS_URL and AUDIOCPP_SERVER, sd SD_SERVER, below the dialog. readConf re-reads per step. Ports 8731 / 8765 / 1234.

## E. Sources model
narratorSlots 4. Extensions: audio .flac .wav .mp3 .m4a .aac .ogg .opus .wma; video .mp4 .mkv .mov .webm .avi .ts. Item: path, footage, narrator, sepVoice (wish), tracks. add (dedupe, media only, footage = isVideo, autoTag), addDir, autoTag (slot 1 to the first untagged row, recordings before footage, never moving a held slot), remove (autoTag after), setFootage (video only), setSepVoice, cycleNarrator (next free slot, past 4 → none), clash (two sources sharing a base name refuse the run), prune, load (strip out-of-range/duplicate slots, force footage=video, autoTag only if something was stripped). Every change renders the list wholesale then notifies.
Row: 🎥 footage toggle (video only); 🎤 narrator button with a one-char slot label (slot 1 suggested-action); file name (ellipsize middle, tooltip = path); track menu button when ≥2 audio streams ("Track N — title (stereo|mono)", last one cannot be unticked); ⚠ when no timestamp in the name; ✂ split toggle (greyed on a split product); 🗑 remove. Every control tooltip ends with the four-line legend of the row symbols.
Snapshot for runners: selItems/selVid/selAud/selNarr/selTracks under a mutex; readers snappedSources, snappedItems, snappedTracks, sepWanted, narratorPath, voiceSource.
ffprobe: one process per file answering duration, size, fps, audio tracks (channels capped at 2, titles), cached by path+size+mtime; fps accepted 1..240; duration 0 falls back to a full decode (recorders that never finalise headers). ffTool: configured ffmpeg path, ffprobe from the same folder.

## F. LLM exchange log
Folder `<project>/llm/`, one HTML page per run named `MMDD-HHMMSS-<step>.html` (step = the run's first call); a run = the calls between two qReset. recordChatStart logs ">>> <step>: <size> of text and N image(s) went to the LLM" and, once per run, the clickable link ">>>   this run's exchanges, images included: llm/<name>". Replies stream into the open section; done logs ">>> <step>: <size> came back in <dur>[, after <size> of thinking][ — cut off at the model's token limit| — the model answered nothing at all]" or "the call failed after <dur>: <err>", then ">>>   the reply begins: <110 runes>". Page: one section per call (meta line, one h2 per message with pre blocks and inline images, thinking in details, the answer, cut-off/empty notes). Recording never fails the call.

## G. Implicit constants (shell)
frameStops {0,0.1,0.2,0.5,1,2,3,4,5} labels each/0.1/0.2/0.5/1s..5s, default 1 s; scalePresets original / 896w (LLM)=scale=896:-2 / 480p / 720p / 1080p; defLanguage en; window 1240×740; log 220/110; hangBeat 200 ms, hangStall 3 s; autosaveTick 2000 ms; projNameChars 28; logListMax 12; confirm width 420; editWait 400 ms; tabGap 6; headSlack 90; confSaveWait 600 ms; LLM test 60 s, vision 120 s, fetch/health 15 s; pipeline: sampleRate 16000, diarWins {90,45,25}, anchorPer 12, shortTake 2.0, anchorCut 0.3, anchorMin 4, minAnchorOv 0.5, diarTurnGap 0.5, diarHopShare 2/3, asrChunkMax 300, asrChunkQwen 60, asrChunkMin 20, asrCutSeek 20, asrQuietDB -35, asrQuietMin 0.4, mergeGap 0.7, mergeMaxLen 12, mergeMaxWord 2, mergeNear 1, checkpoint 200 ms, frame workers clamp(NumCPU/4, 2, 8), frame quality -q:v 4, frame count round(dur/interval); noRoom words: failed to allocate / out of memory / alloc_tensor_range / cudamalloc.

## H. Invariants
1. Widgets are the GUI thread's; runners use logfIdle/prog/IdleAdd and snapshots under mutexes. Page controllers are nil until built; every reader checks.
2. Seam fence: pipeline files may take from the App only the runner interface (logf, logfIdle, prog, qPush, qTake, qDone, checkpoint, readConf, dirs, snapSources, snappedSources, sessionRows, sessionCtx, asrLanguage, videoStyleName, narratorMic, voiceID, produceCut, ttsWav, prompt, ctxBlockFor, runCmd, quietSpots, recordChatStart, takeLLM, giveLLM) and nine named fields; a test enforces it.
3. Pause/stop only between subprocesses; a killed subprocess under stop is errStopped, not a failure; two stopped tracks report one stop.
4. One LLM request on the wire at a time.
5. A chain stops at the first failure or stop; a declining step does not stall the chain; the chain moves pages via showStep synchronously.
6. busy() is the single refusal point; qReset also closes the LLM run page; qTake once per task whatever happens; the bar is the weighted sum, never the queue length.
7. There is always an open project; outDir derives from it. Save As renames, never copies. Sources gone are loud. New goes through applyProject(blank). adoptLegacy leaves no half state. New/Save refused during a run.
8. Resume markers are written last and whole. Two sources with one base name refuse the run. Frame names come from frame numbers. srcClock is handed the whole session. Only video is footage; one row per narrator slot.
9. llm.conf is written whole; readConf re-reads per step; ffprobe beside ffmpeg; tests read what is typed.
10. Recording an exchange never fails the call; the log is read-only; locked tabs are greyed not insensitive; the stack is non-homogeneous.
