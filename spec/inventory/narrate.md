# Inventory: Narrate tab (extracted from the prototype, go/narrate*.go)

<!-- nav -->
[← Inventory: effects (Cut page) and their rendering](effects.md) · [↑ Contents](../README.md) · [Inventory: Prepare tab and pipeline →](prepare.md)
<!-- /nav -->

Raw material for the spec: what the Narrate page does, first read off the Go source of the prototype. Prototype-only facts are named as such.

## A. UI

### A.0 Tab registration

Step entry: name `narrate`, label "Narrate", icon `audio-input-microphone-symbolic`, subtitle "The narration, and the voice it is spoken in".

- Never locked: with no cut the page opens and ▶ refuses.
- On entering the tab: refit lines to the cut, update inputs/outputs readouts. On leaving any tab: flush pending narration save.
- Run bar: ▶ on this page runs the step, unless the preview is playing or has been started (then the run bar is the preview's transport). A clip merely cued by a row click does not take ▶.

### A.1 Page layout

Horizontal paned: left = vertical paned (top: Game audio row + preview + transport; bottom: voice picker), right = the lines column (list of per-clip rows). Split 560 px; inner split 404 px; the window's extra width goes to the lines.

### A.2 The "Narration" tick

Prototype only; the app has no such tick and ignores `no_narration`. In its place the column's top row is "Game audio:", a 0..1 slider (step 0.02). In the prototype: check button "Narration", initially on unless the project says no narration.
Tooltip: "Whether this video has a narration. Unticked, ▶ writes none, the lines already written are left alone, and Produce drops the game-volume slider and the subtitle choices -- all three exist to carry a voice-over."
Off → lines list, preview column and voice picker are greyed (not removed). Produce hides only the game-volume slider (the tooltip's mention of subtitle choices is stale). Produce treats the narration as absent (no lines laid over clips). Persisted as project field `no_narration`.

### A.3 Preview

Video frame showing the finished picture (crop, zoom, freeze, titles via the shared fx screen). Tooltip: "The finished frame with the narration mixed over the cut's sound". Click toggles play. 100 ms tick follows playback. Two players: video (n.player) and narration audio (n.voice). No effect editing here.

### A.4 Transport row

| widget | icon/label | tooltip | action |
|---|---|---|---|
| back | "↚ 3 s" | "back three seconds — the reach the row's ▶ auditions from" | seek −3 s snapped to the cut |
| play/pause | ▶ / ⏸ | "play or pause the preview" | toggle |
| forward | "3 s ↣" | "forward three seconds" | seek +3 s snapped to the cut |
| add line | list-add | "Add a line at this second" | add line at playhead |
| slider | scale, no value | "The cut's own clock — what the edit removed is not on this bar" | range 0..cut length, value the session second; seek debounced 120 ms; paused, the wheel over the picture steps a frame |
| time | label | — | "mm:ss · cut-mm:ss/cut-length" (session time, then position in the finished video) |
| volume | shared preview volume 0–100 | "preview volume — the players only; nothing that is rendered, and the same setting wherever it is shown" | shared with every preview |

### A.5 Per-clip rows

List box, single selection. Selecting a row seeks the preview to the line's lead-in (only place the preview jumps by itself).
Row = header + text box.
Header, in order:

1. Time entry (7 chars): "mm:ss.s" of line start on the session clock. Tooltip: "when this line's audio starts (mm:ss.s on the session clock) — after the dash is when it stops speaking. A time inside another clip moves the line there; one outside the cut is refused and the box goes back to where the line really is". Enter commits (may move to another clip; a time in a gap is refused and written back); typing alone or leaving the field does nothing.
2. Status label: "(no line — this clip plays on its own audio)" | "(caption — the viewer reads it; never spoken)" | "– mm:ss.s (~)", today the line's start second, always with the "(~)". Warnings appended and the row marked red: "⚠ ~N s of speech, M s before the clip ends|the next line"; "⚠ this clip's lines run N s past it — the render will have them moved earlier|moved earlier and sped up".
3. ▶ speak this line (tooltip "speak this line"; one face).
4. ⟳ re-roll: "a new take, same words".
5. ＋ add below: "Add a line below this one".
6. 🗑 delete: "remove this line".
Text box: monospace, word wrap, framed, at least 2 lines tall. Tooltip: `[emotion] words` — the emotion is sent to the voice as the delivery and never spoken; the field on the left is when the line starts. A plain tag is read by a judge ("angry", "surprised, happy"); add weights to skip it and set the mix exactly ("[angry=1]", "[happy=0.8, surprised=0.4]"). The eight it mixes: happy, angry, sad, afraid, disgusted, melancholic, surprised, calm. A tag may end in @N, the second the line starts; [top|center|bottom] words is a caption the viewer reads and the voice never speaks. Text = `[tag] words`; see B.4 for what is read as a tag.

### A.6 Voice picker

Row 1: dropdown "who speaks": "No audio — captions only" (id `captions`); narrator slots 1 up to the highest slot tagged, at most 4 (none when nothing is tagged), labelled "Narrator N — everything they say in the cut" (every recording tagged N, as far as the cut keeps it; first rewrite: "Narrator N — cut from the recording", which drew one recording); then every .wav in the voices folder. Tooltip: "Who speaks the narration. Every line is spoken by cloning the selected recording; switching voices keeps what you already synthesized". A stored voice not on offer → an empty list, nothing selected and a status ("narrator N is not tagged on the Prepare step — tag a recording, or pick another voice" / "voice "X" is no longer in DIR — pick another"). Beside it: the take band's ＋ － ▶ and "Add file…" ("Copy one recording into the voices folder and use it"; file chooser "Choose a voice sample", `.wav` only).
Row 2: the take band (below).
Row 3: sample entry (default "This is the voice the narration will be spoken in. It should stay clear and easy to follow for a couple of minutes.", tooltip "Spoken in the selected voice"; Enter does nothing), ▶ "speak the sample in this voice", ⏹ "stop the sample", ⟳ "a new take of the sample"; "pitch:" slider −6..+6 semitones step 0.5, the value printed beside it, tooltip "Shift the reference recording before it is cloned — a different speaker, not the same one transposed", each move applied 400 ms after it.

### A.7 The take band

Waveform of everything the narrator says in the cut, or of one file whole; visible for a voice with a recording. Tooltip "With no takes the seconds are chosen for you.", none once there are takes. Drag selects; click (<3 px) sets the red bar; wheel zooms 1.25^−dy (max 200 px/s); Shift+wheel pans by w/8. ＋ "drag across the wave first — ＋ makes the selection a take" (min 0.4 s: "that is N s — a take has to be at least 0.4 s"); － "drag across the takes you want gone — － removes those seconds" (subtracts seconds, splitting takes); ▶ "Play the takes from the red bar" (becomes ⏹). With no takes, ▶ plays the raw recording from the bar. Takes are cleaned: sorted, ≥0.4 s, merged when overlapping/touching. Status after commit: "takes — N take(s), X s of reference (14 s is plenty). Re-cut on the next line spoken." Drawing colours documented in source (dark background, blue envelope, green takes, red cue bar).

### A.8 Bottom bar

Inputs label: "N clip(s) · mm:ss" or "no cut yet — build one on the Cut step"; "· ⚠ N clip(s) unwritten, N line(s) off the cut" when stale; "· no timeline" without session.tsv. An unwritten clip is one with no entry and not silent; the row's count treats every clip as silent once any silent mark exists. Tooltip details: cut file, why stale, session.tsv line count and the ±4 s rule, the session context, "Spoken by <voice> at ±N semitones (narrate/voice_ref.wav)".
Outputs: folder button "narrate/ — narration.json, the voice reference and the synthesis cache", the same sentence as the label, whatever the folder holds.

### A.9 Enabled/disabled rules

Nothing greys for narration. No cut → ▶ refuses "no cut yet — build one on the Cut step first". No session.tsv → "run Transcript first — no session timeline". Busy → "pausing after the current stage…". Take band only for a narrator slot with a recording. Sample ▶/⟳ refuse while busy ("still synthesizing the last sample…"), for captions ("no audio is chosen — there is no voice to sample"), empty text ("type a sample sentence to hear the voice"), no selection ("pick a voice first"). Re-roll refuses empty ("clip N has no line to re-roll") and captions-only.

## B. DATA

### B.1 narrate/narration.json

```json
{ "entries": [ {"s":0,"e":0,"at":0,"text":"","emotion":"","pos":"","roll":0} ], "silent": [ {"S":0,"E":0} ] }
```

- s, e: the clip's start/end, session seconds, copied verbatim from the cut.
- at: seconds from the clip's start where the line begins (omitted = 0).
- text: the words; "" = deliberately silent clip.
- emotion: delivery tag, e.g. "calm", "happy=0.8, surprised=0.4".
- pos: caption placement "top" | "center" | "bottom" as typed in a box; "" is no caption, what a written line always gets.
- roll: re-roll count; salts the TTS cache key.
- silent: clips whose last line was deliberately deleted, kept by bounds; pruned on save; a clip written leaves it.
Reading is case-insensitive on keys. Entries always sorted by (s, at). Previous file kept one deep as narration.prev.json once a valid answer is in, before ▶ overwrites ("the narration it replaced is kept at …").

### B.2 TTS cache: narrate/tts/<16 hex>.wav

Key: emotion tag normalised to the 8-float vector when weighted and fully known; `text|emo`; prefixed `voice|` unless the voice is "own" or "narrator1"; prefixed `roll#` when roll>0; final `25e0.85|…` (era + emotion alpha). File = first 8 bytes of SHA-1 as hex. Seed = bytes 8..11 of the same digest (big-endian uint32). The voice part is the voice id alone: pitch and takes are not in the key.

### B.3 Voice files under narrate/

voice.txt (voice id), pitch.txt (semitones), takes.json (map recording stem → [{s,e}], in that recording's seconds), voice_ref_base.wav (reference as cut, unshifted, mono pcm_s16le 48 kHz, loudnorm I=-16 TP=-1.5 LRA=7), voice_ref.wav (pitch-shifted with rubberband formant=preserved; byte copy when 0), tts/*.wav, samples/<voice>_<12 hex>.wav, reference/ref_part_N.wav and reference/ref_parts.txt for a stitched reference.
Reference building: file voice → level the file; narrator slot → hand-picked takes win outright (no cap, no diarization needed); else automatic picks from diarization turns + transcript: takes ≥5 s, ≥2 s from other speakers, ≥1.5 words/s, whole pieces until 14 s is reached or 3 are taken. Errors: "nothing is tagged as narrator N on the Prepare step", "no diarization for X -- run Prepare, or pick the seconds by hand under the video", "no clean solo stretch found for the voice reference". Log ">>> voice reference built: X s, N words|N hand-picked take(s) from <file>". A reference wav with no readable header, format tag 0xFFFE or a rate above 48 kHz is re-cut. Changing voice/pitch/takes removes the shifted reference (and the base where needed). Importing a voice file: ffmpeg to mono pcm_s16le in the voices folder, id sanitised, never overwriting (-2, -3 …).

### B.4 Text ⇄ entry

lineText: "" if blank; tag = pos, else emotion; "[tag] text". Parsing a typed box: leading "[…]" tags, several allowed; "@N" in a tag asks for that session second, refused outside the clip or in its last second; "top|center|bottom" sets pos with no emotion; an emotion of the bases and kin, weighted or not, sets emotion; any other bracket stays in the words. No tag clears the emotion; emptying a box keeps the old emotion. Pulling half-typed rows before ▶ reads the box by a second parser, which also takes centre and middle and stores every placement as "bottom".

## C. FLOWS

### C.1 ▶ narrateRun

1. Refuse if busy; no cut → "no cut yet — build one on the Cut step first"; no session.tsv → "run Transcript first — no session timeline".
2. Pull half-typed rows; compute the stale reason (log only). ▶ writes the clips with no entry, every clip when P.policy.narrationRewrite is on.
3. Save project, start run; queue job "narration" 1/2; log ">>> narrate: <why> — writing N clip(s), one LLM call, then speaking them"; progress "thinking about it" pulsing every 150 ms until an answer is read.
4. writeNarration (D). On success: keep previous file; replace the written clips' entries and silent marks; save; rebuild rows; log ">>> narration written for N clips"; status "narration written — speaking it".
5. Captions-only → done ("narrate: captions only — N line(s) written, none spoken").
6. Else job "speaking" 2/2 over the lines with words that are not captions: ⏹ checked between lines, the bar moving only at the end, skip a cached wav, else synthesize. A refused line is logged and the pass goes on. Log "narrate: N line(s) spoken, M already in the cache".
7. Done: "narration ready and spoken — ▶ the preview hears it in place" | step 5's sentence | "narration stopped" | "speaking stopped" | "narration failed — see log" | "!!! narrate: no usable answer — nothing was written".

### C.2 Fitting a line to a clip (the render's algorithm, mirrored by the page's warnings)

Constants: lead 0.3 s (earliest a line may start), gap 0.3 s between lines, tail 0.2 s, maxExtend 4 s, maxTempo 1.25.

1. Each line's delay = max(lead, at/speed), clamped inside the clip; lines packed in order, never before the previous line's end + gap.
2. Need = packed end + tail. If need > clip length: grow the clip by min(need−length, 4 s), never past the footage (an insert is unbounded).
3. Still over: slide the whole schedule earlier down to the 0.3 s lead ("clip N: the narration does not fit where it was placed — moved X s earlier").
4. Still over: speed the speech up to at most 1.25× ("clip N: narration X s does not fit Y s — sped up Zx"), repack, slide once more.
5. Lines are matched to clips by overlap ≥ half the shorter span (exact bounds within 0.05 for zero-length card clips). A clip with entries but no wav: "clip N: no synthesis for a line — it is captioned only". A clip with no entry: "clip N at T s has no narration entry — it keeps its own audio".
Speech length: measured from the wav when it exists, else chars / 15 per second; on the page a wav's own rate is clamped to 8..28 chars/s.

### C.3 Re-roll and takes

Re-roll: roll++, save, rebuild, "line N: new take, speaking it", speak. Old wav stays on disk.
Take band walk: from the red bar, the takes in order (trimmed to the bar); with none, the raw recording from the bar. Statuses: "no takes yet — drag across the wave and press ＋", "playing N take(s), X s from mm:ss", "playing the recording from mm:ss — nothing is picked after it", "takes played", "stopped playing the takes".

### C.4 Preview playback

- cue(t): the cut's video at t (not the Cut page's watched row); same file → set rate then seek; other file → set mix, rate, load.
- Tick (100 ms): follow position; hold clip boundary while a line still speaks (up to 4 s past the clip end); gap skipping forward only (past last clip → pause); reload when two touching scenes are on different lanes; rate sync for speed effects; when entering a line: play its wav from the offset the picture is at; wav missing → pause picture, synthesize, then resume at the line's start ("synthesizing line N", "line N ready"); failed → sticky per wav, "line N failed to synthesize — see log; its ▶ retries".
- Row ▶: blank or caption → says its sentence, nothing plays; else audition from 3 s ahead, floored at the start of the line above on the clip, and carry on down the cut; no recording covering → the take with no bed. Row selection's lead-in is 3 s ahead, or the line itself when another line owns that second.
- Sound equals the render's: whole clip ducked by game volume when it speaks; hush and mute follow the cut; speed effects applied at seeks.
- Seeking snaps to the cut: a target in a gap lands on the previous clip's end −0.05 (backwards) or the next clip's start (forwards). Frame step is a fixed 30 fps.
- ▶ starts from the narration line under the playhead, else the first clip; pause holds both players (SIGSTOP) and ▶ thaws them.
- ⏹: stop both players, forget state, hand ▶ back to the step ("▶ is the step's again — the preview stopped").

### C.5 Editing lines

- Add at playhead: between clips → "the playhead is between clips — the cut has nothing to narrate here"; within 1 s of an existing line → jump to it ("a line already starts here — edit it, or move the playhead (it is at m:ss.s)"); inside a speaking line (+0.3 s) → "a line is speaking here until mm:ss — add after it". Insert in placement order.
- Add below: at = previous audio end, the line's start with no take, + 0.5 (wordless row: +1.2); "no room after this line — the clip ends first".
- Move (time field commit): to another clip adopts its bounds; a gap is refused: "mm:ss is outside the cut — this line stays in its clip (a–b), at c"; "moved this line to the clip at a — it now starts at b". A time in the clip's last second keeps the line's old offset.
- Delete: "line removed" | "line removed — the clip at mm:ss plays its own audio" (clip added to silent list).
- Autosave: text edits debounced 400 ms; decisions save at once; flush on tab leave, window close, and before Produce reads the file.
- Refit on tab entry: lines keep their place against the video (most-overlap clip); "the cut moved — N line(s) followed their clips" / "…, N sit on video the cut no longer has".

## D. LLM CALL (prompt key "narrate")

One call per run. System = house rules + narrate prompt + "THIS VIDEO PLAYS WHAT PEOPLE SAID OUT LOUD…" addendum when speech is heard in the clips + captions addendum when captions-only (write for the eye; emotion ""; every entry has pos) + the precedence rule when the context box is non-empty.
User = context block (+ speech rule) + "THE CLIPS AND WHAT IS KNOWN ABOUT EACH:" + per clip of the whole cut: "CLIP n: a–b (N s [of footage at Rx, M s on screen], at most W words -- fewer is better, none is fine)"; inserts: "(not footage: a graphic or clip inserted here, "file". Narrate what is ON it, or say nothing …)"; transcript rows within ±4 s as "[+Ns] LABEL: text" (narrator mic → NARRATOR); none → "(no lines over this clip: nothing said, nothing described)"; a "ON THE CARD: …" row per text on a card; effects: "[+Ns] TAG: …", TAG one of MARKED, CAPTION, ZOOM, STOP, SPEED, VOLUME, GRAPHIC, EFFECT.
Word budget: 8..30 words, slope 0.75·seconds.
Tools: write_line, leave_silent, list_emotions, get_lines, describe_insert, finish, and web_search + web_read when available. A line's pos is checked and not kept; an emotion must be a base or kin word.
Progress: written clips / clips, reported once an answer is read.
Reply with no tool used: `{"entries":[{"start","end","at","text","emotion"}]}`, a bare list or one entry.
Tool rounds: matched by clip number in any order; "clip n was written twice — one line per clip, rewrite it once"; a clip the fit ladder would move fails the attempt.
Prose validation: entries matched to clips by echoed start/end within 0.5 s scanning forward (out of order → rejected: "an entry says a-b, which matches no clip (or is out of order)"); skipped clip → "clip n (a-b) got no entry"; at clamped to [0, len−1]; pos normalised; clip bounds stored verbatim; all empty → "every clip came back with no line at all". Up to 3 attempts, each a fresh conversation, logged ">>> narration attempt N rejected: <problem>"; an attempt that wrote nothing turns thinking off for the rest; the last problem is the run's failure. Not served from the LLM cache.

## E. TTS CALL (audio.cpp)

Preflight: reference on disk, built first when missing; GET /health; GET /v1/models must list the TTS model (default index-tts2) and it must be able to clone; upload the reference every line: POST /v1/ui/upload → server path (a 401/403/405 is answered with the fix).
POST /v1/audio/speech: `{"model","input":<text>,"voice_ref":<server path>,"language":"en","options":{"emotion_alpha":"0.85","seed":"<uint32>", then one of: "emotion_vector":"<8 floats>" with emotion_alpha "1" | "use_emotion_text":"true","emotion_text":"<names>"}}`. Language = the project's, "en" when empty (prototype: hard-coded "en"). An empty tag sends neither emotion option. Response = wav bytes; <1000 bytes or non-200 = failure. No client timeout. After a run: POST /v1/tasks/unload_all_models (20 s timeout, quiet).
Emotion vocabulary: 8 bases (happy, angry, sad, afraid, disgusted, melancholic, surprised, calm) with kin lists; 21 named blends (excited, ecstatic, playful, proud, relieved, hopeful, tender, nostalgic, solemn, awed, alarmed, horrified, desperate, confused, frustrated, bitter, contemptuous, dismayed, heartbroken, ominous, tense) as recipes over the 8 axes; weights clamped 0..1; unknown names fall to the judge (text) path; never an error.
Sample: key from the text and roll, file samples/<voice>_<hash>.wav, seed from the key; logs "sample: … (N kB, spoken in just now | spoken earlier)"; "!!! sample: … is N bytes — no audio came back" under 46 bytes.

## F. IMPLICIT CONSTANTS

| name | value | meaning |
|---|---|---|
| ttsPort | 8765 | default audio.cpp port |
| emoAlpha | 0.85 | emotion_alpha on the judge path (part of cache key) |
| narrRunIn | 3.0 s | row ▶ / row click lead-in |
| cutEdge | 0.05 s | how far inside a clip a backwards scrub lands |
| speechRate | 15 chars/s (8..28) | spoken-length estimate |
| narrWriteShare | 0.5 | bar's phase share writing/speaking |
| narrBudget | 8..30 words, 0.75·s | per-clip word ceiling |
| transcript window | ±4 s | rows joining a clip's brief |
| clip-bounds slack | 0.05 s | "same clip" |
| bind slack | 0.5 s | echoed start/end tolerance |
| one line per second | 1.0 s | ＋ jumps to an existing line within 1 s |
| still-speaking pad | 0.3 s | |
| marker ＋ offset | 1.2 s; line ＋ offset +0.5 s | |
| last-second rule | at ≤ len−1 | |
| seek debounce | 120 ms; tick 100 ms; jump 3 s; frame step 30 fps | |
| editWait | 400 ms | autosave and pitch apply debounce |
| narrLead/narrGap/narrTail | 0.3/0.3/0.2 s | render packing |
| maxExtend | 4.0 s | clip growth |
| maxTempo | 1.25 | speech speed-up cap |
| voice ids | "own" (a "narrator1" reads as the same voice), "narrator2".."narrator4", "captions" | frozen |
| narratorSlots | 4 | |
| pitchRange | ±6 st | |
| refLoud | loudnorm I=-16 TP=-1.5 LRA=7; 48 kHz | reference levelling |
| refMinLen/refPad/refWant/refTakeMax/refMinRate | 5 s / 2 s / 14 s / 3 / 1.5 words/s | automatic reference picks |
| takeMin | 0.4 s | |
| take band | ruler 12 px, lane 56 px, max 200 px/s, click 3 px | |
| TTS key prefix | "25e" | cache era |
| TTS failure floor | 1000 bytes (sample 46) | |
| defTTSModel | index-tts2 | |
| voices dir | Settings' `AUDIOCPP_VOICES`, else the legacy models root's voices/, else the platform's own | |
| LLM attempts | 3; backoff 5 s, 20 s, 1, 2, 4 min | |
| unload timeout | 20 s | |

## G. INVARIANTS

1. Entries always sorted by (s, at).
2. A line's s/e are the cut's numbers, never the model's.
3. at ≥ 0 and never in the clip's last second.
4. Empty text is an answer (silent clip), skipped by playback, shown as a row.
5. Emptying a box keeps the old emotion; a placement tag clears the emotion.
6. "@N" typed in the box moves the line; the box never prints it back.
7. A time edit takes effect on Enter; it may move clips; a gap is refused.
8. Rebuilds are deferred to idle and coalesced; programmatic widget changes are guarded.
9. The preview plays the cut; gaps skipped forward only; slider range the cut length, value the session second.
10. Clip boundary held while a line still speaks (≤4 s), mirroring the render.
11. A synthesis hold resumes at the line's start.
12. Failed synthesis is sticky per wav until its ▶ retries.
13. An audition (solo) is not paused by the tick; whoever takes over claims the voice.
14. Run bar becomes the preview's transport only once the preview has started; ⏹ hands it back.
15. Staleness is coverage, not count; edited text is not staleness.
16. Cache keys are stable across spellings of a blend; nothing deletes old wavs.
17. Reference wav judged by header, not existence.
18. Hand-picked takes are never re-ranked or capped and need no diarization.
19. Takes: sorted, ≥0.4 s, merged; subtraction splits rather than deletes.
20. Preview sound equals the render's sound.
21. Captions-only short-circuits everything that speaks.
22. Nothing greys for narration; a narration with no lines with words renders with no voice.
23. ▶ writes only the clips with no entry unless P.policy.narrationRewrite is on (prototype: always every line, though its Readme said only the missing ones).
24. subwords.go is subtitles, not narration.

<!-- nav -->
---
[← Inventory: effects (Cut page) and their rendering](effects.md) · [↑ top](#inventory-narrate-tab-extracted-from-the-prototype-guinarratego) · [↑ Contents](../README.md) · [Inventory: Prepare tab and pipeline →](prepare.md)
<!-- /nav -->
