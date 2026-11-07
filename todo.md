# Todo

## Simplify the code

Biggest gain first. No behaviour changes; the full suite stays green.

1. **Shared state.** The Produce, Narrate, Window and Cut pages each keep theirs in one struct (`ui/*_page_state.rs`). Global state stays: it is the simple choice here. About 60 values remain outside them, mostly in non-UI modules.
2. **Large files.** `rust/src/ui/produce_page.rs`, `cut_tracks.rs` and `settings.rs` are split. What is left to thin: `build_owned` in `settings.rs`, about 400 lines in one function.
3. **Remaining duplicates.** Exact copies are shared. Left on purpose, because the names or values differ per module: `stat_recording`, `two_messages`, `human_weight`, `panel_height`, `seeded`. Worth a look by hand: the form hands (`press_*_apply`, `wire_*_buttons`), `text_gate::stage` and `describe_call::stage`, `drive_walk` and its `_for_test` twin.
4. **Functions only tests call.** The rules nothing else used are gone, with the tests that only checked them. About 70 non-seam functions are left, kept because shared test helpers call them (`encode_command`, `checkpoints`, `click_outcome`, `part_at`, ...), so removing them means rewriting those helpers. The rest are `_for_test` seams, which are fine.

## Suspected bugs

Found by reading the code while updating the spec. None reproduced or run yet. The spec describes what the code does today, so a fix also changes the spec.

**Prepare**
- `run_stages.rs:94-104`: separation runs after frames and speech; the bar jumps back and split halves go unprocessed that run.
- `fix_call.rs:101`: per-recording Fix context lacks other sources' lines.
- `fix_call.rs:142`: twice-refused blocks are cached as ASR text and never re-asked.
- `prepare.rs:110`: the Inputs readout overstates vision requests.
- `prepare.rs:185`, `prepare_run.rs:306`: `rfind('.')` lanes disagree with `source_lane` for dotted names.
- `fix_transcripts.rs:492`: the fix cache key omits the model. `fix_transcripts.rs:552`: `Reply::Flag` appears never constructed.

**Project and LLM**
- `layout.rs:400`: nothing creates `produce/publish`, so new projects write to the legacy `publish/`.
- `project.rs:797,815`: `*_hints` and `prompts` logged as adopted, but the text is discarded.
- `project.rs:141,144`: `narrationRewrite` has no setter; `keepSwearing` has no setter or production reader.
- `llm_cache.rs:151`: `uses_cache` says ClipRules is uncached; `clip_pass_ask.rs:60` caches it.
- `settings_probe.rs:251`: with a project open, the 60/120 s Test ceilings are not applied.

**Cut**
- `ui/cut_line_press.rs:121`: matches rows by `cam<N>`, so real recordings never take the scene.
- `ui/cut_select_press.rs:142`: wave-strip scope is `cam<N>`, not the recording name.
- `cut_line.rs:152`: a click far from any clip still holds the nearest one.
- `cut_verbs.rs:150`: a ruler selection adds overlapping scenes per row and doesn't split at gaps; Add snaps up to 5 s to any clip edge.
- `suggest.rs:232`: m:ss values followed by "seconds".
- `ui/window_history_zoom.rs:107`: Redo says "undone".
- `cut_screen.rs:279`: zoom tooltips say middle/cursor; the code holds the red line.

**Effects**
- `ui/window_history_zoom.rs:~580`, `ui/cut_speed_hand.rs:~356`: Apply always appends, so editing an effect likely duplicates it.
- `ui/cut_speed_hand.rs:44`: a line-only stop opens at 0:00, not the line.
- `clip_rules_tools_leg.rs:226`, `cut_effects_pass.rs:74`: the zoom box is always `None`, so zooms always centre.
- `fx_zoom.rs:39`: arm words promise a shape and snap that don't happen. `fx_zoom.rs:280`: a marked stretch sets length but start stays the line. `ui/cut_zoom_hand.rs:430`: armed drag ignores the marked stretch.
- `ui/cut_lane_paint.rs:64`: matches "pitched", stored value is "pitch".
- `produce_render.rs:594,638`: hush appears to run before the lane mix.

**Narrate**
- `narrate_ask.rs:41`: brief numbers the whole cut, tools a subset.
- `narrate_call.rs:278`: nothing to write fails; `:228` prose is never re-asked.
- `narrate_run.rs:94`: silent clips rewritten. `narrate_live.rs:338`: `pos` dropped.
- `narrate_pass.rs:384`, `narrate_edit.rs:119`: blends rejected. `narrate_edit.rs:66`: a last-second move keeps the old offset.
- `narration.rs:64`: key lacks pitch and takes.
- `ui/narrate_page.rs:371`: slider uses session seconds; `:760,:773`: row ▶ never pauses, wordless plays nothing; `:966`: pitch not debounced.
- `ui/narrate_row.rs:37`, `narrate_screen.rs:357`: end shows start.
- `ui/narrate_inputs_tally.rs:54`: one mark silences all. `ui/narrate_picker.rs:114`: an empty list blocks picking.
- `ui/window_narrate.rs:598`: "spoken" despite refusals; `:630`: placement becomes bottom.
- `ui/narrate_page_transport.rs:23`: ▶ ignores the playhead. `ui/narrate_page_take.rs:385`: ⟳ uses the same seed.

**Produce**
- `produce_flow.rs:366`: halves run in sequence, doc says parallel; ⏹ can't stop the words half (`ui/produce_languages.rs:611,706`).
- `produce_upload_draw.rs:101,126`: no cancel and no timeout, so ⏹ can't abort a hung draw.
- `produce_exec.rs:1040`: planned before speaking; new lines missing from the voice mix.
- `produce_exec.rs:432`: stamp roll 0; re-rolled lines never up to date. `produce_upload.rs:307`: stamp ignores base and crop.
- `produce_exec.rs:548,670`: burned sheets relative, cleared, misnumbered. `produce_exec.rs:389`: own track tagged `und`.
- `ui/produce_press_wiring.rs:416`: Transcode input equals output.
- `ui/produce_page_run.rs:179`: flag never set, always "transcoding". `ui/produce_page_rows.rs:32`: "FAILED: <size>".
- `produce_translate.rs:122`: false cache line; `:132` " / " stays literal.
- `produce_upload_call.rs`: `get_events` unanswered.
- `produce_screen.rs:136`: webm clears burned-in; `:251` pixel compare; `:519` wrong tooltip.
- `ui/produce_slots.rs:100`: crop box undraggable. `ui/produce_page_presses.rs:118`: empty box kept.

### Found in the 2026-11-07 spec sync

Same rules as above: read, not run.

**Prepare**
- `transcribe.rs:309`: no aligner plus an ASR without word times gives an empty transcript, no hard error.
- `diar_scan.rs:67`: "new audio each" prints anchor_per, not the hop. `align.rs:589`: the aligner is picked per recording, not per run.
- `decode_audio.rs:143`: a lead out of range keeps `transcript.fixed.tsv` on the old times. `asr.rs:256`: no ⏸ checkpoint between ASR chunks. `asr.rs:65`: "oom" also matches "room"/"zoom".
- `describe_call.rs:30`, `text_gate.rs:282`: a footage-only index used as a row index, so the "hearing X" offsets are wrong. `describe_call.rs:327`: every chunk's first frame is NEW SCENE. `:416`: read tools' answers never reach the model. `:547`: speech block reads only the video's own `words.json`. `:606`: a chunk without `set_state` blanks `state.txt`.
- `fix_session.rs:296`: any "!!!" line leaves `session.txt` unmarked; `:82`: on resume the marking pass runs twice.
- `joins.rs:425`: `|cut N|` counts misattributed; `:461`: with 1–3 words `final.txt` and `retakes.tsv` are removed. `join_ask.rs:481`: whole takes never reach the sixth column or the yellow tint. `hand_edit.rs:242`: the remake compares against raw `words.json` on each recording's own clock.
- `retakes.rs:48,162`, `retakes_ask.rs:178,238,248`: the retake brief has no words and no User Context, cached runs lose their marks, the rephrase trim never runs, edges are never placed. `retakes_ask.rs:389`: log says `cache/retakes/<key>`, file is under `cache/llm/retakes/`.

**Shell and LLM**
- `llm_request.rs:130`: tools go on the wire as name and parameters only (`web_tools::schemas` has no callers), so `prompts/tools.md`'s descriptions never reach the model. `llm_web_offer.rs:35`: an empty firefox setting counts as available without a PATH check.
- `ui/window_page_box.rs:90`: the "copy into project" tick is never written back. `add_sources.rs:146`: `vid_dir`/`aud_dir` stored, never read.
- `sources.rs:14`: legend names ✂ for the voice split (button is 🗣); `:28`: tooltip promises a "nobody yet" state the button never reaches. `ui/source_rows.rs:169`: `any_track_out` hard-coded false.
- `ui/window_project.rs:580`: Open refuses a running job only after the pick; `:62`: New treats "no sources" as empty and drops a typed User Context without asking. `ui/save_apply.rs:19`: a renaming Save leaves the remembered project on the old folder.
- `checks.rs:1`: comment says ten Test buttons; there are twelve.

**Cut**
- `ui/cut_suggest_land.rs:304`: the words cut gives `cut_breaths::apply` no inserts, so joins at hand-placed cards can be shortened. `suggest.rs:206`: status counts segments before the walk-back.
- `ui/window_line_step.rs:382`: Space only flips the flag; `:474`: Shift+`,`/`.` arrive as `<`/`>`, so the five-frame step never fires. `ui/cut_preview_press.rs:181`: a picture click skips `after_press`.
- `ui/cut_record_tick.rs:107`: lanes mixed only under ▶, `lane_hear.rs:224` restarts each lane every tick, ▶ ignores rates; `:145`: in-play statuses overwritten within a tick.
- `ui/cut_band_trim.rs:232,250,263,296`: left border drag never merges; right press near a border trims; right-drag starts at 4 px, not 3; no row change reachable. `ui/cut_trim_press.rs:231,258`: a row slide shifts only recordings in `cut.shift`; a slid scene can leave its recording.
- `cut_select.rs:251`: selection ends don't snap to recording ends. `cut_verbs.rs:302`: Split cuts overlay cards; `:607`: "added on cam 0" is 0-based and always row 0.
- `ui/window_history_zoom.rs:647`: Clear greyed on an opened cut until the first edit. `ui/cut_tracks_draw.rs:77,426`: sound pieces ignore gaps and folds; in/out marks stop after the first picture row. `ui/cut_view.rs:137`: `total_px` counts folded seconds. `ui/cut_page_layout.rs:132`: insert form footer below the tracks.

**Effects and Narrate**
- `cut_captions.rs:149`: caption starts summed from batch clip lengths, restarting each batch. `ui/clip_pass.rs:14`, `ui/cut_effects_pass.rs:133`: clip rates never set, so the brief never says "plays at Nx".
- `effect_overlap.rs:39`: no check that the lens switch is inside the band. `ui/cut_band_press.rs:169`: a press without motion opens no form. `ui/cut_speed_hand.rs:147`: cost note never refreshes.
- `cut_cards.rs:440`, `produce_cards.rs:881`: a tier row prints its letter, not its items; logos never drawn. `produce_cards.rs:136`: cards bake at 25 fps, not the render fps.
- `fx_svg.rs:108`, `fx_volume.rs:139`, `fx_label.rs:116`: refusal reads "(0.3) s". `ui/cut_zoom_hand.rs:423`: literal "{:.0} px".
- `voice_ref.rs:263`: prints "X" for the recording name. `narrate_call.rs:266,276`: a rejection counts as "wrote nothing" and turns thinking off. `ui/narrate_lines.rs:101`: an unparseable time is refused as "00:00 is outside the cut". `narrate_screen.rs:208`: the empty-list note uses ＋, which draws as a box.

**Produce**
- `produce_exec.rs:784`: lines spoken so far printed as seconds in "synthesis for Ns". `produce_render.rs:456`: a lane is mixed only if it plays at the clip's first second (`LANE_MIN_MIX_SECONDS` unused).
- `produce_upload.rs:114`: narration `at` (offset in its clip) printed as a video time; `:494`: "taking 3 from the cut" after an empty spread; `:566`: "no images" while references are sent; `:655`: a failed `thumbnail.stamp` write uses the render-stamp sentence.
