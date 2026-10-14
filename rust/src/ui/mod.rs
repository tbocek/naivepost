/// §05-cut#7-rules bullet 1 — the page's ONE hand: one thread-local slot, so `cut_rules::pick_up`'s rule
/// (`One thing held at a time`) binds across an edge, a clip and an effect instead of three independent slots.
pub mod cut_hand;
/// §02-services.md §4.1 — the Cut page's snap points when no aligner is named: read out of
/// `cache/waves`. Split out of `window.rs`, at its size budget.
pub mod wave_snap;
/// §04-prepare#6 — the bench mark and Reset, painted from what this machine holds. Split out of
/// `window.rs`, which is past its size budget.
pub mod bench_paint;
pub mod settings;
/// F0.1 §S2/§S4 and §2 — the narration autosave timer, the bar's idle pulse and the run line that goes
/// to the status row. Split out of `window.rs`, which is at its size budget.
pub mod shell_beats;
/// §05-cut#1-screen — the Cut page's two drawing areas: the track band that paints every §1 mark, and the
/// playhead's own layer. Split out of `window.rs`, which is past its size budget.
pub mod cut_colours;
pub mod cut_tracks;
/// F0.4 §S2/§S6 — the driver that feeds [`crate::lucky`]'s chain real outcomes, and the CSS rule that
/// makes the gears turn. Split out of `window.rs`, which is past its size budget.
pub mod lucky_run;
/// F0.5 S4 — the one door a run closes through, for every way a run ends.
pub mod run_close;
/// §7's link line — the log's one clickable line, and the launcher that opens it. Split out of
/// `window.rs`, which is past its size budget.
pub mod log_link;
pub mod log_paint;
/// §08-produce#1-screen — the Produce page's surface: its 15 numbered widgets and their thin wires.
pub mod produce_page;
/// §08-produce#1-screen — the render legs the Produce page hands to `produce_exec`. Split out of
/// `produce_page.rs`, which is past its size budget.
mod produce_page_rows;
/// F5.4 §S3/§S7/§S9 — the Produce page's Translate row (its ticks, the state they write) and the
/// render's spawner seam. Split out of `produce_page`, which is at its size budget.
pub mod produce_languages;
/// F5.7 — the wiring that turns a Produce-page click into a `produce_presses` walk: the legs it reads the
/// page for, and the save path the widget's own default supplies. Also split out of `produce_page`.
pub mod produce_press_wiring;
/// §08-produce#1-screen item **7** — the ✎ dialog of the thumbnail's text-overlay editor.
pub mod produce_words;
/// §08-produce#1-screen items **4**, **5**, **8**, **9** — the four boxes a person TYPES into: their
/// `changed` wires. Split out of `produce_page`, which is at its size budget.
pub mod produce_text;
/// §08-produce#1-screen items **2**/**3** — the Images row's slots: each slot's picture, which one is
/// the base, the crop box the base wears, and its four buttons. Split out of `produce_page`, at its budget.
pub mod produce_slots;
/// F0.6 Open at start — the launch's door: which project a start opens, and what it leaves remembered.
pub mod launch;
pub mod new_apply;
/// F0.10 Save as — the window's own half of S3/S4: what moved, which folder the window now believes
/// is open, and where the next Save goes.
pub mod save_apply;
/// F0.9 Open a project — the window's own half of S4: refresh every page, remember this root, say it.
pub mod open_apply;
/// F0.11 Rescan — the window's own half of S1–S3: name the drops, write the pruned list back, and
/// refresh every tab from what the scan re-read.
pub mod rescan_apply;
/// §4's source rows: the controls a file gets, the track menu over the streams ffprobe counted, and the
/// repaint that keeps row indices honest after a removal.
pub mod source_rows;
/// §07-narrate#1-screen — the Narrate page's surface: its 22 widgets and their thin wires.
pub mod narrate_page;
/// F2.10 cameras and hearing: the page half of the flow, split out of `window.rs`. Its doors are
/// re-exported below under the `ui::` names the tests and the shell already call.
mod cut_cameras;
/// F2.8 S1/S2/S3: the strip's real bands, the name a press in each resolves to, and what a gesture costs
/// (persist, one Undo, folds, the live re-seek). Split out of `window.rs`, past its size budget.
pub mod cut_trim_press;
/// §05-cut#1-screen badge **31** and the wheel's three gestures: the band's live zoom, its scroll
/// position and the geared scrollbar. Split out of `window.rs`, which is past its size budget.
pub mod cut_view;
/// §05-cut#2 S2b — the model mode's cut call: the attempts, the five tools answered by
/// `tools::cutpass::Plan`, and the walk-back of whatever passed. Split out of `window.rs`, which is
/// past its size budget; it lands nothing (that is `cut_suggest_land`'s write).
pub mod cut_suggest_model_run;
/// §05-cut#8 bullet 3 — the Suggest call's own progress and retry rules, in the shape the attempt loop can
/// carry. Split out of `cut_suggest_model_run`, which is past its size budget.
pub mod cut_suggest_watch;
/// §05-cut#2 — landing a suggestion: the walk-back's answer becomes the cut on screen AND the history's
/// base in one write. Split out of `window.rs`, which is past its size budget.
pub mod cut_suggest_land;
/// §05-cut#2 S1/S2 — what the Cut page's ▶ says and shows when it is Suggest, including why a started
/// preview is not this press's to take. Split out of `window.rs`, which is past its size budget.
pub mod cut_suggest_press;
/// §F2.14 S2b — painting the Suggest call's progress ("streaming progress counts segments") on the run bar.
/// Split out of `window.rs`, which is past its size budget.
pub mod cut_suggest_progress;
pub mod cut_form_frame;
pub mod cut_picture;
pub mod cut_stop_frame;
/// §06-effects#2-the-lane-and-the-preview — the preview panel's painted frame: the dim outside the camera
/// rect, the stroked rect, the overlays at their real alpha, and the mask plus titles while playing.
/// Its widgets are built here and its draw func installed from `window.rs`, which keeps its own budget.
pub mod cut_preview_frame;
/// F3.4 S4 — the caption's preview draw: its box, the shared fitter's size and lines, and the white bold words
/// with their dark dilated edge. Split out so `cut_preview_frame` stays a painter that composes no rule of its own.
pub mod cut_preview_caption;
/// F3.4 S5 (`a press without travel toggles play/pause wherever it lands`) — the preview panel's one button-1
/// gesture asks this whether a click landed on an overlay's box, and if so plays or pauses through the picture's own
/// door instead of falling through to the framing flow.
pub mod cut_preview_press;
/// §06-effects#2-the-lane-and-the-preview — assembling the preview panel and the effects lane under it into
/// the Cut page, and keeping the overlay stack in step with the frame. Split out of `window.rs`, which is
/// past its size budget.
pub mod cut_preview_panel;
/// §06-effects#2-the-lane-and-the-preview — whether a lane bar earns its dashed debt tail, and the class it
/// wears. Split out of `window.rs`, which is past its size budget.
pub mod cut_debt_tail;
pub mod cut_zoom_hand;
/// F2.12 S7 — the card preview pass: ffmpeg renders a card's frames at 8 fps (one for a still), the nearest
/// rendered one goes on the panel, and a spliced card holds the footage while it plays. Split out of
/// `window.rs`, which is past its size budget.
pub mod cut_card_hold;
pub mod cut_card_preview;
/// F2.6 S1/S2 on the PAINTED band: the gesture that draws, resizes and moves a selection, read off the
/// geometry the painter uses. Split out of `window.rs`, which is past its size budget.
pub mod cut_select_press;
/// F3.8 S1/S2/S3 — the effects-lane bar's own drag gesture: when a press is a click, when it picks the band
/// up, and how far the hand may take it. Split out of `window.rs`, which is past its size budget.
pub mod cut_band_press;
/// F2.7: the page's half of Add / Split / Remove / ⌦ — the four doors, what each writes, the snap-list
/// gather and the ✕ dispatch. Split out of `window.rs`, which is past its size budget.
pub mod cut_verb_press;
/// F2.12 Insert — the page's half: ⧉ Insert's door, the chooser, the form and Apply. Split out of
/// `window.rs`, which is past its size budget.
mod cut_insert_form;
/// §05-cut#1-screen — the effects lane's rows, their CSS and the preview panel redrawn from them. Split out
/// of `window.rs`, which is past its size budget. Its doors are re-exported through `window`.
mod cut_lane_rows;
/// §1's bench, the tab buttons and the readouts they repaint. Split out of `window.rs`, which is past its
/// size budget. Its doors are re-exported through `window`.
mod window_bench;
/// §05-cut#1-screen — the selection band, the held effect and its live form, the preview's volume. Split
/// out of `window.rs`, which is past its size budget. Its doors are re-exported through `window`.
/// F0.7's policy doors and F0.13's gear: the ⚙/ⓘ form, its open slot, the User Context tracker and
/// the debounced derivation. Split out of `window.rs`, which is past its size budget.
mod window_policy;
mod window_selection;
/// §F2.8 — trim and move on the track strip, the copy/paste hand and the lane press. Split out of
/// `window.rs`, which is past its size budget. Its doors are re-exported through `window`.
mod window_trim_strip;
/// §02's run bar — the ▶ ⏹ 🎲 ▶✂ presses and their wires. Split out of `window.rs`, which is past its
/// size budget. Its doors are re-exported through `window`.
mod window_run_press;
/// F2.12 Insert — the form's half: its holder, what is open, the plan drawn and Apply. Split out of
/// `window.rs`, which is past its size budget. Its doors are re-exported through `window`.
mod window_insert;
/// §05-cut#1-screen — the history buttons and the zoom pair: the door from a press to the rule and
/// the repaint from the answer. Split out of `window.rs`, which is past its size budget. Its doors
/// are re-exported through `window`.
mod window_history_zoom;
/// F4.1 ▶ Write and speak — the window seam of one narration run: what a press is refused, the bar it
/// paints, and the status lines it leaves. Split out of `window.rs`, which is past its size budget.
/// Its doors are re-exported through `window`.
/// §F2.4 — the red line and its frame stepping: where the line stands, what a step or an arrow does
/// to it, the four step buttons and their key controller. Split out of `window.rs`, which is past its
/// size budget; its doors are re-exported through `window`.
mod window_line_step;
mod window_narrate;
/// §1's widget doors — the named lookups, readouts and tooltip readers. Split out of `window.rs`,
/// which is past its size budget. Every name is re-exported through `window`.
mod window_doors;
/// §F3.9/§F3.10/§F3.11 — the ✐ Captions, ⏩ Speeds and ✨ Decorations run-bar jobs and their gates. Split
/// out of `window.rs`, which is past its size budget. Its doors are re-exported through `window`.
mod cut_pass_gates;
/// F2.4 S1/S1b — one press on the painted band, read where it actually landed (row, gutter, zoom, scroll).
pub mod cut_line_press;
/// §05-cut#2 §B — ⇲ Lane with nothing in hand: a footage file on a row of its own, probed. Split out of
/// `window.rs`, which is past its size budget.
pub mod cut_lane;
/// F2.10: the session's recordings — stamped names, hand shifts and probed lengths — which is what gives a
/// second camera a row of its own. Split out of `window.rs`, past its size budget.
/// §05-cut#7-rules bullet 8 — `the page is rebuilt only when Prepare's output changed`: the identity of
/// `transcript/session.tsv`, cheap enough to ask on every refresh.
pub mod cut_rebuild;
/// F2.10: the session's recordings — stamped names, hand shifts and probed lengths — which is what gives a
/// second camera a row of its own. Split out of `window.rs`, past its size budget.
pub mod cut_session;
/// §05-cut#7-rules bullet 8 — the strip is a window drawn under a translate: the one place its scroll, its
/// px-per-second and the pixel↔second conversion live, so the painter and the press cannot disagree.
pub mod cut_strip_view;
/// §05-cut#8 bullet 2: the mute line, the stream-volume reset, and the reason each names. Split out of
/// `window.rs`, which is past its size budget.
pub mod preview_sound;
/// F3.6 S5: the door that hands a PAUSED second's gain to the footage leg, so a parked line inside a raised
/// passage is heard at that loudness. No rule of its own — it asks `ui::paused_preview_gain` and `cut_hear::mix_gain`.
pub mod cut_volume_hear;
/// F3.10 ⏩ Speeds: the pass's own half of the shell — read the page, telephone, write back. Split out of
/// `window.rs`, which is past its size budget.
pub mod cut_speeds_pass;
/// F3.11 ✨ Decorations: the pass's own half of the shell — read the page, telephone, write back. Split out
/// of `window.rs`, which is past its size budget.
pub mod cut_effects_pass;
/// F2.1 ▶'s page half: the filmed runs and footage files a press hands the media leg, and the button's face.
/// Split out of `window.rs`, which is past its size budget.
pub mod cut_record_play;
/// F2.2 ▶✂'s page half: the button's grey (S1) and ⏸ face, and the ✂ clock's tooltip.
pub mod cut_cut_play;
pub mod cut_review_play;
/// F3.5 ▨ SVG drawing by hand: the page half of the flow, split out of `window.rs`. Its doors are
/// re-exported below under the `ui::` names the tests and the shell already call.
mod cut_speed_hand;
mod cut_volume_hand;
mod cut_label_hand;
mod cut_text_hand;
mod cut_folds;
mod cut_svg;
/// F3.5 S5: the ffmpeg call that makes a drawing's 512 px preview raster, run for real from [`cut_svg`]'s ask.
mod cut_svg_raster;
/// F0.7 S5 — the policy form: every field with its value, source and reason.
mod narrate_lines;
/// F4.7 (**Text**) — the row's text box: what a typed tag does to the line, and when it reaches disk.
mod narrate_text;
pub mod policy_form;
pub mod prepare_guard;
pub mod window;

pub use settings::badge;
// F3.5 ▨ SVG drawing by hand: the doors this shell and the widget tests call, under the `ui::` names they
// always had (they lived in `window.rs` until they moved to `cut_svg`).
pub use cut_svg::{
    make_here, press_svg_apply, press_svg_cancel, press_svg_choose, press_svg_esc, press_svg_item,
    raster_dir, raster_of, release_svg_arm_only, svg_armed, svg_chosen, svg_chosen_file,
    svg_drag_ended, svg_drag_ended_with_source, svg_form_open, svg_raster_state, swap_svg_file,
};
// §07-narrate#1-screen — the page's seams: the state it renders, the refresh that paints it, and one
// function per press so a widget test can compare what the click printed with what the rule answered.
pub use narrate_page::{
    press_back, press_forward, press_line_reroll,
    press_line_speak, press_narrate_add_line, press_narrate_play, press_pitch, press_sample_play, press_sample_reroll,
    press_sample_stop, press_take_add, press_take_remove, press_voice, read_state, refresh,
    set_narration_off, set_state, NarrateState,
};
// F4.7's row edits live in their own file; re-exported here so `ui::press_line_*` resolves as before.
pub use narrate_lines::{press_line_add_below, press_line_move, press_line_remove};
// F4.7 (**Text**): the box's parse-and-write seam, reached from the row's `changed` handler and from
// the two flush doors (window close, Produce's read).
pub use narrate_text::{apply_box, flush_owed, note_typing};
pub use settings::dialog_log;
pub use settings::dialog_log_open;
pub use settings::dialog_open;
pub use settings::entry;
pub use settings::open_with as open_settings;
pub use settings::press_all;
pub use settings::press_test;
pub use settings::settings_button;
pub use settings::test_button;
/// F0.7 S5 — the policy form and the two ways into it (the run bar's ⚙ and each tab's ⓘ).
pub use window::run_queue;
pub use shell_beats::run_bar;
pub use window::close_policy_forms;
// The shell's own seams and the beats `shell_beats` stands: a test drives these rather than reaching for a
// private flag, which is what makes the autosave and the pulse observable from outside `ui`. The two whose
// name says `_for_test` are read by a check only; every other rule here runs inside the program's own timers.
pub use shell_beats::{advance_clock_for_test, needs_timer, pulse_standing_for_test, should_pulse};
pub use window::open_policy_form;
pub use window::forget_policy_form;
pub use window::derive_policy;
pub use window::main_window;
pub use window::policy_is_current;
pub use window::refresh_policy_form;
pub use window::set_policy_sender_for_tests;
pub use window::set_session_context;
pub use window::reset_policy_to_defaults;
pub use window::policy_form_open;
pub use window::press_policy;
pub use window::session_policy;
pub use window::set_session_policy_field;
pub use window::arrow_steps;
pub use window::line_key_controller;
pub use window::line_position;
pub use window::line_root;
pub use window::flush_line_on_close;
pub use window::line_step_button;
pub use window::move_line_and_save;
pub use window::place_line_press;
pub use window::set_line_position;
pub use window::space_toggles;
pub use window::step_frames_of;
pub use window::step_the_line;
pub use window::Step;
pub use window::add_files;
pub use window::add_sources_button;
pub use window::bench_language;
pub use window::running_step;
pub use window::session_freq;
pub use window::set_bench_open_row;
pub use window::build_window;
pub use window::copy_into_project;
pub use window::find_source_widget;
pub use window::lucky_button;
pub use window::log_expander;
pub use crate::ui::log_paint::log_view;
pub use crate::ui::log_paint::note_exchange_line;
pub use window::new_button;
pub use window::open_button;
pub use window::open_folder;
pub use window::open_from;
pub use window::open_tooltip;
pub use window::new_project_confirm;
pub use window::new_tooltip;
pub use window::marking_pass;
pub use window::play_button;
pub use window::play_tooltip;
pub use window::progress_bar;
pub use window::progress_tooltip;
pub use window::rescan_button;
pub use window::session_handle;
pub use window::session_sources;
pub use window::set_marking_pass;
pub use window::rescan_tooltip;
pub use window::save_button;
pub use window::save_tooltip;
pub use window::state;
pub use window::stop_button;
pub use window::press_play_cut;
pub use window::press_review_cuts;
// F3.12: the clamp seam, so a test can run "effects held to the cut as applied" by name and read the same
// cut state `tests/cut_clamped_to_cut.rs` checks.
pub use window::clamp_effects_to_cut;
pub use window::press_play_recording;
pub use window::play_cut_button;
pub use window::review_cuts_button;
pub use window::seed_review_cut;
/// F2.7: the write-to-cut.json door, so a check can start from a file on disk (every editing press already
/// uses it; this only makes it reachable from outside `ui`).
pub use cut_insert_form::save_seed_for_test;
pub use window::play_recording_button;
pub use window::set_preview_volume;
pub use window::preview_volume;
pub use window::apply_mix;
pub use window::preview_volume_scale;
pub use window::draw_selection;
pub use window::press_fx_lane;
pub use window::pressed_surface;
pub use window::selection;
pub use window::clear_selection;
pub use window::nudge_selection;
pub use window::selection_verbs;
pub use window::selection_surface;
pub use window::selection_gesture;
pub use cut_select_press::band_selection_gesture_for_test;
pub use window::clear_selection_button;
pub use window::selection_readout;
// F2.8: the strip's recording bound, so a test compares against the same wall the seam used.
pub use window::TRACK_REC_END;
pub use window::TRACK_STRIP_PPS;
pub use window::review_cut_segs;
pub use window::copy_hand;
pub use window::review_lanes;
pub use window::copy_esc_controller;
pub use window::press_copy;
pub use window::press_paste;
pub use window::press_lane;
pub use window::review_cut_segs_count;
// F2.7: the three verb buttons, the ⌦ key and the two "what is in hand" seams.
pub use window::find_status;
pub use window::held_clip;
/// The segment in hand either way — whole clip or held edge — for doors that move a segment without asking
/// which the hand took. `held_clip` alone answers only the whole-clip case (§7's mutual exclusion).
pub use window::held_segment;
pub use window::held_edge;
pub use window::held_now;
pub use window::set_held_edge;
pub use window::apply_pick;
pub use window::watched_row;
pub use window::set_watched_row;
pub use cut_cameras::{
    camera_rows_box, camera_watch, hand_preview_back, press_gutter_switch, press_lens_scene,
    press_speaker_badge, press_watch_row, preview_row, refresh_camera_rows, toggle_button,
};
// F2.10: the row list itself, rebuilt from the live cut by every refresh above.
// F2.11: the fold badges and the two ✕, drawn as named widgets and forwarded by these seams.
/// F2.12 S7: the card preview pass, called from `cut_picture::refresh` (the panel's one frame door).
pub use cut_card_preview::{card_hearing, card_tick, set_card_snapshot_dir};
pub use cut_folds::{
    fold_badges_box, fold_gaps, fold_overlay, folded_gap_at, page_recordings,
    press_fold_all, press_fold_gap, press_lane_cross, press_row_cross, refresh_fold_badges,
    review_fold_rows, set_session_recordings, unfold_fold_for_playback,
};
/// F2.1's tick: the line following the player at 10 Hz, walk-on and folds opening under ▶.
pub mod cut_record_tick;
pub use window::delete_key_controller;
pub use window::line_click_gesture;
pub use window::held_effect;
// F2.12: the insert button's wire, the form's seams and its read-back for tests.
pub use cut_insert_form::{
    close_insert_form, insert_chosen, insert_chosen_with_length, insert_open, note_place,
    press_insert, press_insert_apply, refresh_insert_button,
};
pub use window::press_effect_item;
pub use window::press_zoom;
pub use window::press_zoom_item;
pub use cut_speed_hand::{
    band_esc_controller, press_speed_apply, press_speed_cancel, press_speed_esc, press_speed_item,
    speed_custom_rate_visible, speed_form_open,
};
/// F3.6 Volume by hand — the form's doors, so a test can press them the way the dropdown row does.
pub use cut_volume_hand::{
    paused_preview_gain, press_volume_apply, press_volume_cancel, press_volume_esc,
    press_volume_item, volume_form_open,
};
/// F3.7 Label by hand — the form's doors, so a test can press them the way the dropdown row does.
pub use cut_label_hand::{
    label_form_open, press_label_apply, press_label_cancel, press_label_esc, press_label_item,
};
// F3.9 Captions proposed by the model: the pass's seams, so a test can fire ✐ Captions by name and read the
// placed captions through the same cut state the logic tests check.
pub use window::run_captions_pass;
pub use window::run_captions_pass_with_reply;
pub use window::captions_pass_button;
pub use window::refresh_captions_gate;
pub use window::set_captions_pass;
// F3.10 Speeds proposed by the model: the pass's seams, so a test can fire ⏩ Speeds by name and read the
// placed rates through the same cut state the logic tests check. The pass itself is `cut_speeds_pass`.
pub use cut_speeds_pass::{run_speeds_pass, run_speeds_pass_with_reply};
pub use window::speeds_pass_button;
pub use window::refresh_speeds_gate;
pub use window::set_speeds_pass;
// F3.11 Decorations proposed by the model: the pass's seams, so a test can fire ✨ Decorations by name and read
// the same cut state the logic test in `tests/cut_effects_proposed.rs` checks. The pass itself is
// `cut_effects_pass`.
pub use window::decorations_pass_button;
pub use cut_effects_pass::run_effects_pass;
pub use cut_effects_pass::run_effects_pass_with_reply;
pub use window::narrate_stage;
pub use window::show_narrate_stage;
pub use window::press_narrate_run;
pub use window::set_narrate_script;
pub use window::narrate_script_loaded;
pub use window::pull_half_typed;
pub use window::refuse_narrate_run;
pub use window::set_decorations_pass;
pub use window::CaptionBatch;
/// F3.4 Text (caption) by hand, and F3.2's aspect door — the same seams, so a test presses them by name.
pub use cut_text_hand::{
    drag_text_edge, nudge_text_box, press_aspect, press_text_apply, press_text_cancel,
    press_text_esc, press_text_item, text_armed, text_drag_ended, text_drag_ended_with_source,
    text_form_open,
};
pub use window::note_zoom_place;
pub use window::zoom_armed;
pub use window::PREVIEW_PANEL_H;
pub use window::PREVIEW_PANEL_W;
pub use window::zoom_drag_ended;
pub use window::zoom_drag_ended_with_source;
pub use window::zoom_form_open;
pub use window::zoom_gesture;
pub use window::press_zoom_apply;
pub use window::press_zoom_cancel;
pub use window::press_zoom_esc;
pub use window::set_zoom_armed;
pub use window::refresh_effects_lane;
// F3.1's module-level seams: the press on the paused preview, and the plate beside the rect.
pub use cut_zoom_hand::{
    plate_shown_for_test as zoom_plate_shown,
};
pub use window::effects_lane_box;
pub use cut_verb_press::{press_add, press_delete_key, press_remove, press_split};
pub use window::note_edit;
pub use window::press_clear_cut;
pub use window::press_redo;
pub use window::press_revert;
pub use window::reopen_history_on;
pub use window::press_undo;
pub use window::review_cut_of;
pub use window::history_key_controller;
pub use window::set_held_clip;
pub use window::set_held_effect;
// F2.8: the trim/move strip, its two gestures, and the seams that feed a right-press's ground.
pub use window::open_folds;
pub use window::press_band;
pub use window::press_move;
pub use window::review_cut_shift;
pub use window::right_press_took;
pub use window::press_position;
pub use window::press_scene;
pub use window::press_move_at;
pub use window::press_trim_border;
pub use window::set_press_band;
pub use window::set_press_x;
pub use window::set_press_scene;
pub use window::track_strip;
pub use window::trim_gesture;
pub use window::move_gesture;
pub use window::select_surface;
pub use window::SELECT_SURFACE_PPS;
pub use window::preview_player;
pub use window::set_playhead;
pub use window::set_preview_state;
pub use window::tab_button;
pub use window::mark_narration_owed;
pub use window::tab_info_button;
pub use window::inputs_readout;
pub use window::outputs_readout;
pub use window::outputs_folder_button;
pub use window::tab_dimmed;
pub use window::tab_tooltip;
pub use window::window_logs;
pub use window::APP_ID;
// F3.8 Hold, move, resize, edit, remove: the lane's seams, so a test can fire a band press by name and read
// the effect back through the same state the logic tests check.
pub use window::cut_bar_width_px;
pub use window::drag_held_band;
pub use window::live_form_answer_is_undo;
pub use window::live_form_refused;
pub use window::nudge_held_effect;
pub use window::open_effect_form;
pub use window::press_band_esc;
pub use window::press_effect_bar;
pub use window::press_edit_effect;
pub use window::remove_effect_at;
pub use window::remove_held_effect;
/// F3.8: the bar gesture's own doors — `wire_bar` is called from `refresh_effects_lane`, and the two state
/// readers are what `drag_held_band` asks about the drag in flight.
pub use cut_band_press::{bar_gesture_for_test, band_drag_in_flight, take_history_step, wire_bar};
