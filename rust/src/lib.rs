pub mod add_sources;
pub mod align;
pub mod audio_task;
pub mod asr;pub mod bench;
pub mod bodies;
pub mod checks;
pub mod clock;
pub mod decode_audio;
pub mod decision_audit;
pub mod decision_homes;
pub mod degraded;
pub mod cut;
pub mod captions_ask;
pub mod speeds_ask;
pub mod effects_ask;
pub mod produce_translate_ask;
pub mod cut_captions;
pub mod cut_copy;
/// §05-cut#2 — a cut lane built from a source's own metadata: the naming ladder and the ffprobe ask that
/// gives a footage file a row of its own when nothing was copied.
pub mod cut_lanes;
pub mod cut_delete;
pub mod cut_effect_decisions;
pub mod hands_off;
pub mod audit_gaps;
pub mod cut_effects_pass;
pub mod cut_hear;
/// F2.5 S3/S4 — the lane pipelines: one real player process per separate recording, obeying `cut_hear`'s
/// per-tick answer (started at its own second, never started when silenced, stopped at a scene boundary).
pub mod lane_hear;
pub mod cut_line;
pub mod cut_play;
pub mod cut_play_leg;
/// F2.2 S5's media leg: the cut preview's own player process and the prerolled spare waiting at the next
/// jump's second (two `gst-play-1.0` processes, swapped by signal — see the module doc).
pub mod cut_play_back;
pub mod cut_review;
pub mod cut_select;
pub mod cut_trim;
pub mod cut_verbs;
pub mod cut_wave_edges;
/// F0.7 — the User Context in, the editing policy out: derivation, validation, and which flows the
/// result turns on. No UI here; `ui::window` draws the form and forwards to this.
pub mod policy;
/// F0.7 S2's live half: the derivation's own HTTP leg, so a non-empty User Context reaches a model.
pub mod policy_call;
/// The policy form's value column: one editor per derived field (F0.7 S4/S5).
pub mod policy_form_edit;
/// §12 entry 21: reading a plain target length (`45 min`, `half an hour`, `1h20`) out of the User Context's
/// own words, so the number lands in `P.policy.targetLengthSeconds` instead of silently staying 0.
pub mod policy_length;
pub mod cut_cam;
pub mod cut_clamp;
pub mod cut_clamp_line;
pub mod cut_speed_split;
pub mod cut_speed_play;
pub mod cut_fold;
pub mod cut_cards;
pub mod cut_insert;
pub mod cut_rules;
/// §05-cut#7-rules bullet 7 — the ONE door that writes `cut.json`: it resolves the session folder and hands the
/// cut to [`crate::cut::save`], so no editing door repeats the path logic or skips the write.
pub mod cut_save;
pub mod effect_details;
pub mod effect_overlap;
pub mod effect_rules;
pub mod fx_band;
pub mod narrate_pass;
pub mod narrate_preview;
pub mod narrate_preview_leg;
/// F4.5's media answers for the preview: which files the picture plays and at what gain, so the leg that owns
/// the two children asks instead of deciding.
pub mod narrate_preview_media;
pub mod narrate_frame;
pub mod narrate_screen;
pub mod narrate_band;
/// §F4.6 / inventory §A.7: when the take band shows and how many seconds it spans, read off the project
/// rather than off whichever reference file happens to be on disk.
pub mod narrate_band_face;
pub mod voices;
pub mod narrate_data;
pub mod narrate_details;
pub mod narrate_off;
pub mod narrate_call;
pub mod narrate_reply;
pub mod narrate_rewrite;
pub mod narrate_run;
pub mod narrate_edit;
pub mod narrate_sample;
pub mod narrate_live;
pub mod narrate_ask;
pub mod narrate_rules;
pub mod narrate_tts;
pub mod cut_suggest;
pub mod suggest;
pub mod cut_speed;
pub mod cut_speed_pass;
pub mod cut_suggest_passes;
pub mod cut_suggest_words;
pub mod cut_suggest_words_run;
pub mod cut_screen;
pub mod timeline;
pub mod diar_scan;
pub mod diarize;
pub mod edges;
pub mod exchange_leg;
pub mod exchanges;
pub mod fix_transcripts;
pub mod hand_edit;
pub mod frames;
pub mod frames_run;
pub mod render_fx;
pub mod fx_aspect;
pub mod fx_lane;
pub mod fx_label;
pub mod fx_record;
pub mod fx_svg;
pub mod fx_text;
pub mod fx_volume;
pub mod fx_zoom;
pub mod icons;
pub mod layout;
pub mod llm_budget;
pub mod llm_cache;
pub mod llm_request;
pub mod llm_stream_leg;
pub mod llm_tool_refusal_leg;
pub mod clip_rules_tools_leg;
pub mod llm_tools_offer;
pub mod llm_web_offer;
pub mod migrate;
pub mod model_calls;
pub mod model_list;
pub mod model_list_fetch;
pub mod narration;
pub mod params;
pub mod params_produce;
pub mod new_project;
pub mod open_project;
pub mod project;
pub mod project_autosave;
pub mod project_produce;
pub mod project_settings;
pub mod prompts;
pub mod prepare;
pub mod prepare_data;
pub mod prepare_decisions;
pub mod prepare_run;
pub mod describe;
pub mod describe_call;
pub mod describe_scale;
pub mod fix_call;
pub mod fix_session;
pub mod tool_reads;
pub mod joins;
pub mod join_ask;
pub mod retakes;
pub mod retakes_ask;
pub mod preview;
pub mod prompt_assembly;
pub mod produce_embed;
pub mod produce_flow;
pub mod produce_data;
pub mod produce_exec;
pub mod produce_probe;
pub mod produce_fx_chain;
pub mod produce_plan_logs;
pub mod produce_cards;
pub mod produce_details;
pub mod produce_render;
pub mod produce_read_head;
pub mod produce_runs;
pub mod produce_screen;
pub mod produce_clip_cues;
pub mod produce_subtitles;
/// F5.4's write side: which clips are burned, what a cue-less run says, and the sentence naming the sheet.
pub mod produce_subtitles_run;
pub mod produce_tag_page;
pub mod produce_translate;
pub mod produce_stamp;
pub mod produce_upload;
pub mod produce_upload_call;
pub mod produce_upload_draw;
pub mod produce_upload_frames;
pub mod produce_upload_media;
pub mod produce_upload_print;
pub mod produce_presses;
pub mod publish;
pub mod probes;
pub mod rescan;
pub mod requests;
pub mod request_timing;
pub mod server_leg;
pub mod stop_legs;
pub mod settings_probe;
/// §02-services.md §5 — the Settings rows that are not LLM/audio/sd HTTP: the LLM's two completion
/// tests, the four audio.cpp model ids, the aligner, ffmpeg and firefox.
pub mod settings_probe_rows;
pub mod speak_leg;
pub mod roles;
pub mod lucky;
pub mod cancel_leg;
pub mod run;
pub mod run_stages;
pub mod runqueue;
pub mod save_as;
pub mod seam_retries;
pub mod separate;
/// §4's track menu: how many audio streams a file really holds, asked of the real ffprobe.
pub mod source_probe;
/// F2.4 S2: the frame rate of the recording under the line, asked of that file with the real ffprobe.
pub mod cut_fps;
pub mod separate_media;
pub mod separate_run;
/// F2.1's media leg: the `gst-play-1.0` subprocess ▶ starts and stops over the session's footage.
pub mod play_back;
pub mod services;
pub mod settings;
pub mod slot_gate;
pub mod shell;
pub mod snapshot;
pub mod snapshot_effect_menu;
pub mod snapshot_effects;
pub mod snapshot_face;
pub mod snapshot_insert;
pub mod snapshot_narrate;
pub mod snapshot_preview;
pub mod snapshot_produce;
pub mod snapshot_rows;
pub mod snapshot_settings;
pub mod snapshot_text;
pub mod snapshot_suggest;
pub mod snapshot_trim;
pub mod snapshot_verbs;
pub mod snapshot_words;
pub mod sources;
pub mod startup;
pub mod subprocess;
pub mod tab_states;
pub mod textfmt;
pub mod transcribe;
pub mod tool_loop;

/// Which tree is which: `rust/` the app, `gui/` the Go prototype, `spec/` the spec (AGENT.md).
pub mod tree_roles;

pub mod llm_liveness;
pub mod llm_watch_leg;
pub mod llm_repair_leg;
pub mod llm_retry;
pub mod llm_retry_leg;
pub mod machine_store;
pub mod web_browser;
pub mod web_request_log;
pub mod web_tools;
pub mod tools;
pub mod voice_ref;
pub mod word_list;
pub mod wave;
pub mod wave_build;
pub mod ui;

/// The four pages, in the order the user works through them (spec/00-principles.md §1).
pub const PAGES: [&str; 4] = ["Prepare", "Cut", "Narrate", "Produce"];
