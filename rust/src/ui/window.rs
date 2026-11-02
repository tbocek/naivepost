//! The window: renders a [`Project`]'s state and nothing else. No rules live here
//! (spec/00-principles.md §5); every value on screen traces back to the model.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use gtk4 as gtk;

use crate::cut::{self, Cut};
use crate::cut_hear;
use crate::cut_line;
use crate::cut_play_leg;
use crate::cut_select;
use crate::cut_verbs;
use crate::exchanges;
use crate::layout;
use crate::narration::{self, Narration};
use crate::policy;
use crate::project::Project;
use crate::run;
use crate::runqueue;
use crate::shell::{Move, Outcome, Page, Refresh, Shell};
use crate::startup;
use crate::ui::cut_cameras::refresh_camera_rows;
use crate::ui::cut_folds::{
    page_recordings, press_fold_all, push_session_recordings, refresh_fold_badges,
};
use crate::ui::cut_form_frame;
use crate::ui::cut_label_hand::wire_label_esc;
use crate::ui::cut_picture;
use crate::ui::cut_speed_hand::{wire_band_esc, wire_speed_esc};
use crate::ui::cut_svg::wire_svg_esc;
use crate::ui::cut_text_hand::wire_text_esc;
use crate::ui::cut_volume_hand::wire_volume_esc;

pub(crate) const OUTPUTS_FOLDER_TIP: &str = "Open this project's output folder";
/// §1 badge **15**'s word, its own label before the folder button; the count carries no prefix.
pub(crate) const OUTPUTS_PREFIX: &str = "Outputs:";

pub const APP_ID: &str = "ch.bocek.naivepost";

pub(crate) use crate::ui::window_page_box::page_box;

pub use crate::ui::window_effect_item::press_effect_item;

pub use crate::ui::cut_zoom_hand::{
    note_zoom_place, press_zoom_item, preview_picture_px, set_zoom_armed, zoom_armed,
    zoom_form_box, zoom_form_open, PREVIEW_PANEL_H, PREVIEW_PANEL_W,
};
pub(crate) use crate::ui::cut_zoom_hand::{
    set_zoom_form, show_zoom_form, trim_seconds, zoom_field_row,
};

pub use crate::ui::cut_lane_rows::{cut_bar_width_px, effects_lane_box, refresh_effects_lane};
pub(crate) use crate::ui::cut_lane_rows::{preview_widget, set_class};

pub(crate) use crate::ui::window_bench::{
    depth, has_label, paint_readouts, paint_tabs, paint_tabs_for, refresh_cut_readouts, tab_buttons,
};
pub use crate::ui::window_bench::{paint_progress, paint_run_bar, set_bench_open_row};
/// Repaint the bottom bar for a folder: the door Open and Rescan go through.
pub use crate::ui::window_bench::{paint_readouts_for, readouts_read_here_for_test};

pub(crate) use crate::ui::window_session::{holder_children_named, session_reads, start_shell};

pub use crate::ui::window_selection::{
    apply_mix, clear_selection, clear_selection_button, drag_held_band, draw_selection,
    live_form_answer_is_undo, live_form_refused, nudge_held_effect, nudge_selection,
    press_band_esc, press_edit_effect, press_effect_bar, press_fx_lane, pressed_surface,
    preview_playhead, preview_volume, remove_effect_at, remove_held_effect, select_surface,
    selection, selection_gesture, selection_readout, selection_surface, selection_verbs,
    set_preview_volume,
};
pub(crate) use crate::ui::window_selection::{
    held_narration, refresh_selection_readout, refresh_verb_buttons,
};

pub use crate::ui::window_trim_strip::{
    copy_esc_controller, copy_hand, delete_key_controller, move_gesture, note_place, open_folds,
    press_copy, press_lane, press_move, press_move_at, press_paste, press_row, press_scene,
    press_trim_border, right_press_took, set_press_band, set_press_scene, set_press_x, track_strip,
    trim_gesture, TRACK_REC_END, TRACK_STRIP_PPS,
};
pub(crate) use crate::ui::window_trim_strip::{
    refresh_copy_buttons, wire_copy_esc, wire_delete_keys, wire_track_strip,
};

pub use crate::ui::cut_pass_gates::{
    land_pass, pass_inputs, run_captions_pass, run_captions_pass_with_reply,
    set_captions_pass_for_test, set_decorations_pass_for_test, set_speeds_pass_for_test,
    walk_captions, CaptionBatch, PassInputs, PassOut,
};
pub(crate) use crate::ui::window_run_press::{
    live_transport, wire_play, wire_play_cut, wire_play_recording, wire_preview_volume, wire_stop,
};

#[derive(Debug, PartialEq)]
pub struct UiState {
    pub page: Page,
    pub status: String,
    /// F0.5 S1: read from the widget, so a test checks what the user sees.
    pub log_expanded: bool,
    pub progress: f64,
}

/// Build the window showing `project`, living in the working copy beside the working directory; see [`build_window_at`].
pub fn build_window(
    app: &impl IsA<gtk::Application>,
    project: &Project,
    page: &str,
) -> adw::ApplicationWindow {
    build_window_at(
        app,
        project,
        page,
        &startup::session_dir(&std::env::current_dir().unwrap_or_default()),
    )
}

/// Build the window showing `project`, which lives in `dir`: the folder every step, save and read works on ([`startup::open_dir`]).
pub fn build_window_at(
    app: &impl IsA<gtk::Application>,
    project: &Project,
    page: &str,
    dir: &std::path::Path,
) -> adw::ApplicationWindow {
    // FIRST: everything below asks `startup::open_dir`, which still names the previous window's folder.
    startup::set_open_dir(dir);
    let window = adw::ApplicationWindow::new(app);
    // §1: minimum size 1240×900. GTK 4 refuses to shrink under its size request, which makes it a floor
    // (`tests/suite/window_shrinks.rs`).
    window.set_default_size(crate::shell::WINDOW_MIN_W, crate::shell::WINDOW_MIN_H);
    window.set_size_request(crate::shell::WINDOW_MIN_W, crate::shell::WINDOW_MIN_H);
    window.set_title(Some(crate::shell::WINDOW_TITLE));
    note_startup(&[]);

    let shell = Rc::new(RefCell::new(start_shell(page)));
    WINDOW_PAGE.with(
        |WindowPage {
             window_shells: shells,
             ..
         }| shells.borrow_mut().push(Rc::clone(&shell)),
    );

    // S5 refits on entering Narrate and S3 writes it out on leaving, so the lines must survive between the
    // two; re-reading per switch (`session_reads`) would flush the pre-refit file. F4.7's text view edits this copy too.
    let held_narration = Rc::new(RefCell::new(session_reads().2));
    WINDOW_PAGE.with(
        |WindowPage {
             held_narration: held,
             ..
         }| held.borrow_mut().push(Rc::clone(&held_narration)),
    );

    // F0.7: which User Context this window's policy answers to; fresh, so the first ▶ asks.
    WINDOW_PAGE.with(
        |WindowPage {
             policy_trackers: trackers,
             ..
         }| {
            trackers
                .borrow_mut()
                .push(Rc::new(RefCell::new(policy::Tracker::new())))
        },
    );
    WINDOW_PAGE.with(
        |WindowPage {
             policy_watch_window: watch,
             ..
         }| *watch.borrow_mut() = Some(window.clone()),
    );

    // Rescan (F0.11) and Add sources (F0.12) work on one private session copy shared between them;
    // made before the pages so Prepare's rows can hold a handle to it.
    let session = Rc::new(RefCell::new(project.clone()));
    WINDOW_PAGE
        .with(|WindowPage { session: slots, .. }| slots.borrow_mut().push(Rc::clone(&session)));
    // F1.1: ▶ asks `prepare_run` about the LIVE session, not the build-time `project`.
    WINDOW_PAGE.with(
        |WindowPage {
             play_session: slots,
             ..
         }| slots.borrow_mut().push(Rc::clone(&session)),
    );

    // §2 S6: remembered before anything can move the cwd; the close hook must not write into `project_autosave::session_dir()`'s cwd-based folder.
    let session_folder = dir.to_path_buf();
    WINDOW_PAGE.with(
        |WindowPage {
             session_dirs: held, ..
         }| held.borrow_mut().push(session_folder.clone()),
    );
    crate::project_autosave::install(Rc::clone(&session), session_folder.clone());
    crate::project_autosave::start();
    crate::watchdog::arm();
    // A fresh window installs its own session rather than one an earlier window left behind.
    crate::ui::window_line_step::push_session_tree(layout::Tree::new(&session_folder).ok());

    let status = gtk::Label::new(Some(""));
    status.set_widget_name("status-line");
    status.set_xalign(1.0);
    status.set_hexpand(true);
    status.set_ellipsize(gtk::pango::EllipsizeMode::End);
    status.add_css_class("dim-label");
    status.set_margin_end(8);

    let stack = adw::ViewStack::new();
    // §1: measure each tab on its own, so `main-divider` shrinks to what THIS page needs.
    super::window_log::per_tab_stack(&stack);
    let mut prepare_add: Option<(gtk::Button, gtk::CheckButton)> = None;
    for page in Page::all() {
        let name = page.label();
        let (child, add_, copy_) = page_box(name, project, &session, &status, &window);
        stack.add_titled_with_icon(&child, Some(name), name, page.icon());
        if let (Some(add_), Some(copy_)) = (add_, copy_) {
            prepare_add = Some((add_, copy_));
        }
    }
    if !page.is_empty() {
        stack.set_visible_child_name(page);
    }

    let switcher = adw::ViewSwitcher::builder()
        .stack(&stack)
        .policy(adw::ViewSwitcherPolicy::Wide)
        .build();
    paint_tabs(&switcher, &shell.borrow(), project);

    let bar = Rc::new(RefCell::new(run::RunBar::default()));
    // Published so a test can read which step the bar holds; `last()` is this window.
    WINDOW_PAGE.with(|WindowPage { run_bars: bars, .. }| bars.borrow_mut().push(Rc::clone(&bar)));
    WINDOW_PAGE.with(
        |WindowPage {
             window_bar: slots, ..
         }| slots.borrow_mut().push(Rc::clone(&bar)),
    );

    // The children a run has out there, so ⏹ can reach them (F0.3 S3).
    let procs = Rc::new(RefCell::new(run::Subprocesses::default()));

    let guard = Rc::new(std::cell::Cell::new(false));
    WINDOW_PAGE.with(
        |WindowPage {
             window_switch_latch: latch,
             ..
         }| *latch.borrow_mut() = Some(Rc::clone(&guard)),
    );

    // §1's badges **14** and **15**: window-level so a switch updates one pair; built before the switch is wired
    // because a tab click repaints them. Their texts come from `Shell::inputs` / `Shell::outputs`.
    let readout_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    readout_row.set_widget_name("readout-row");
    readout_row.set_margin_start(8);
    readout_row.set_margin_end(8);
    let inputs_readout = gtk::Label::new(None);
    inputs_readout.set_widget_name("inputs-readout");
    inputs_readout.set_xalign(0.0);
    inputs_readout.add_css_class("dim-label");
    readout_row.append(&inputs_readout);
    let outputs_label = gtk::Label::new(Some(OUTPUTS_PREFIX));
    outputs_label.set_widget_name("outputs-label");
    outputs_label.add_css_class("dim-label");
    readout_row.append(&outputs_label);
    let outputs_folder = gtk::Button::from_icon_name("folder-symbolic");
    outputs_folder.set_widget_name("outputs-folder-button");
    outputs_folder.set_tooltip_text(Some(OUTPUTS_FOLDER_TIP));
    outputs_folder.set_valign(gtk::Align::Center);
    // §1 badge **15**: the press opens the folder the row names, read at press time (`window_outputs`).
    super::window_outputs::wire(&outputs_folder);
    readout_row.append(&outputs_folder);
    let outputs_readout = gtk::Label::new(None);
    outputs_readout.set_widget_name("outputs-readout");
    outputs_readout.set_xalign(0.0);
    // Ellipsize the count: at §1's minimum GTK must shrink something, and a label without `ellipsize(End)` is drawn
    // 0 px wide. It stays an expander: a non-expanding count makes `readout-row` natural 0,
    // which `window_run_scroller::share_the_free_width` sizes §1's progress bar from.
    outputs_readout.set_hexpand(true);
    outputs_readout.set_ellipsize(gtk::pango::EllipsizeMode::End);
    outputs_readout.add_css_class("dim-label");
    readout_row.append(&outputs_readout);

    wire_switching(
        &stack,
        &switcher,
        &status,
        &shell,
        &guard,
        &session,
        &inputs_readout,
        &outputs_folder,
        &outputs_readout,
        &held_narration,
    );

    // S4 at startup: same read and painter as the switch handler.
    {
        let (tree, cut, narration) = session_reads();
        paint_readouts(
            &shell.borrow(),
            project,
            tree.as_ref(),
            &cut,
            &narration,
            &inputs_readout,
            &outputs_folder,
            &outputs_readout,
        );
    }

    let queue = Rc::new(RefCell::new(runqueue::Queue::new()));
    WINDOW_PAGE.with(
        |WindowPage {
             window_queue: slots,
             ..
         }| slots.borrow_mut().push(Rc::clone(&queue)),
    );
    let exchange_log = Rc::new(RefCell::new(exchanges::RunLog::new()));

    // Idle is 0 rather than hidden: a vanishing bar makes the row jump when a run starts.
    let progress = gtk::ProgressBar::new();
    progress.set_widget_name("run-progress");
    progress.set_hexpand(true);
    progress.set_valign(gtk::Align::Center);
    progress.set_tooltip_text(Some(runqueue::STANDING_TIP));

    // §1: the log header is "Log" plus the status label; `state()` still reads the label by name.
    let log_view = gtk::TextView::new();
    crate::ui::margins::text_box(&log_view);
    log_view.set_widget_name("log-view");
    super::log_link::install(&log_view);
    log_view.set_editable(false);
    log_view.set_cursor_visible(false);
    log_view.set_monospace(true);
    log_view.set_wrap_mode(gtk::WrapMode::WordChar);
    log_view.set_vexpand(true);
    // §10 gives the log heights only in prose ("log heights 220/110"), so no `P.` id.
    // A TextView without its own scroller reports the whole document's height as its minimum.
    let log_scroller = gtk::ScrolledWindow::new();
    log_scroller.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    // 220 px is the OPEN height, not a floor: the text view vexpands, so the shell can be dragged under it.
    log_scroller.set_widget_name("log-scroller");
    log_scroller.set_min_content_height(super::window_bench::LOG_MIN_BODY_PX);
    log_scroller.set_child(Some(&log_view));
    super::log_paint::follow_end(&log_scroller, &log_view);

    let log_header = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    log_header.set_widget_name("log-header");
    let log_title = gtk::Label::new(Some("Log"));
    log_title.add_css_class("heading");
    log_header.append(&log_title);
    log_header.append(&status);

    let log = gtk::Expander::new(Some("Log"));
    log.set_widget_name("log-expander");
    log.set_label_widget(Some(&log_header));
    log.set_child(Some(&log_scroller));
    log.set_vexpand(true);
    log.set_expanded(false);
    // A collapsed expander must claim nothing, or the band it belongs to is charged a body that is never drawn.
    super::window_log::hide_body_while_collapsed(&log, log_scroller.upcast_ref());
    log.connect_expanded_notify({
        let view = log_view.clone();
        move |log| {
            if log.is_expanded() {
                super::log_paint::open_at_end(&view, &window_logs());
            }
        }
    });

    let play = gtk::Button::from_icon_name(run::PLAY_ICON);
    play.set_widget_name("play-button");
    play.add_css_class("suggested-action");
    let stop_ = gtk::Button::from_icon_name(run::STOP_ICON);
    stop_.set_widget_name("stop-button");
    stop_.set_tooltip_text(Some(run::STOP_TOOLTIP));
    paint_run_bar(
        &play,
        &stop_,
        &bar.borrow(),
        shell.borrow().page,
        live_transport(),
    );

    let controls = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    // §1: this box is what scrolls; the scroller around it (`window_run_scroller`) carries the name `run-controls`.
    controls.set_widget_name("run-controls-inner");
    controls.append(&play);
    controls.append(&stop_);
    // F0.15's ↺ is last; `paint_run_bar` greys it during a run by finding it after ⏹.
    controls.append(&super::window_reset::button(&window));
    // §2's row holds only the controls and the readouts: F0.7's derivation and F3.9-F3.11 are jobs of the run, not buttons.
    // The controls live in `run-controls-inner`, which `window_divider::install` puts in a scroller so a
    // narrow window scrolls them sideways; the progress bar stays in this OUTER row so shrinking squeezes the buttons.
    let run_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    run_row.set_widget_name("run-bar");
    run_row.append(
        &super::window_run_scroller::wrap_controls(controls.clone().upcast::<gtk::Widget>())
            .upcast::<gtk::Widget>(),
    );
    run_row.append(&progress);

    wire_play(
        &play,
        &stop_,
        &bar,
        &shell,
        &status,
        &progress,
        &log,
        &log_view,
        &queue,
        &exchange_log,
        project,
    );
    wire_stop(
        &stop_, &play, &bar, &shell, &status, &progress, &log, &log_view, &queue, &procs,
    );

    let box_ = gtk::Box::new(gtk::Orientation::Vertical, 0);
    // §1's header bar: **1** New · **2** Open · **3** Save · **8** Rescan, with Settings (F0.13) and Profiles (F0.14).
    // Open replaces the window's one live project (`session`) so every flow sharing the handle reads it.
    let header = adw::HeaderBar::new();
    let new_ = gtk::Button::from_icon_name("document-new-symbolic");
    new_.set_widget_name("new-button");
    new_.set_tooltip_text(Some(NEW_TIP));
    header.pack_start(&new_);
    wire_new(&new_, &bar, &status, &session);

    let open_ = gtk::Button::from_icon_name("document-open-symbolic");
    open_.set_widget_name("open-button");
    open_.set_tooltip_text(Some(OPEN_TIP));
    header.pack_start(&open_);
    wire_open(&open_, &bar, &status, &session);

    let save_ = gtk::Button::from_icon_name("document-save-symbolic");
    save_.set_widget_name("save-button");
    save_.set_tooltip_text(Some(SAVE_TIP));
    header.pack_start(&save_);
    wire_save(&save_, &bar, &status, &session);

    super::window_doors::install_project_title(&header);

    // **8** is `pack_end`, so it sits at the right end and not with New and Save.
    let rescan_ = gtk::Button::from_icon_name("view-refresh-symbolic");
    rescan_.set_widget_name("rescan-button");
    rescan_.set_tooltip_text(Some(RESCAN_TIP));
    header.pack_end(&rescan_);
    wire_rescan(&rescan_, &status, &session, &shell);

    // F0.13's Settings, packed after Rescan so Rescan stays rightmost.
    let settings_ = gtk::Button::from_icon_name("emblem-system-symbolic");
    settings_.set_widget_name("settings-button");
    settings_.set_tooltip_text(Some(SETTINGS_TIP));
    header.pack_end(&settings_);
    wire_settings(&settings_, &window);

    let profiles_ = gtk::Button::from_icon_name("folder-templates-symbolic");
    profiles_.set_widget_name("profiles-button");
    profiles_.set_tooltip_text(Some(
        "Profiles \u{2014} templates for a kind of video: the User Context with fields, every step's prompt and \
the Produce settings",
    ));
    header.pack_end(&profiles_);
    let parent = window.clone();
    profiles_.connect_clicked(move |_| {
        crate::ui::profiles_dialog::open_edit(Some(parent.upcast_ref()), None);
    });

    // §1's badge **6**: the only ⓘ, packed after Settings (each `pack_end` lands left of the previous). F0.7 has no
    // policy form, so its whole job is the tooltip `Shell::info_tip`, synced by `paint_readouts` on every switch.
    let tab_info = gtk::Button::from_icon_name("help-about-symbolic");
    tab_info.set_widget_name("tab-info-button");
    tab_info.set_tooltip_text(Some(&shell.borrow().info_tip()));
    WINDOW_PAGE.with(
        |WindowPage {
             info_button: slots, ..
         }| slots.borrow_mut().push(tab_info.clone()),
    );
    header.pack_end(&tab_info);

    // Handed over rather than found by name: `page_box` runs once per tab, so `find` would return an arbitrary page's button.
    if let Some((add_, copy_)) = prepare_add {
        wire_add(&window, &add_, &copy_, &bar, &status, &session);
    }

    box_.append(&header);
    // §1 badge **5**: the tab row is the header bar's TITLE widget, in its centre.
    switcher.set_widget_name("page-tabs");
    switcher.set_hexpand(false);
    switcher.set_halign(gtk::Align::Center);
    // Keep the tab words whole; badge 4 (the path) ellipsizes first (§1).
    super::window_tabs::keep_labels_whole(&switcher);
    super::window_tabs::style_as_joined(&switcher);
    // STRICT: libadwaita's LOOSE default slid the tab row ~80 px right of centre (§1: "in its centre").
    header.set_centering_policy(adw::CenteringPolicy::Strict);
    header.set_title_widget(Some(&switcher.clone().upcast::<gtk::Widget>()));
    // §1's draggable divider; `window_divider` owns the band's height ask (`band_px`).
    super::window_divider::install(
        &box_,
        stack.upcast_ref(),
        run_row.upcast_ref(),
        readout_row.upcast_ref(),
        log.upcast_ref(),
    );
    super::window_run_fit::fit_into_row(&run_row, &controls, readout_row.upcast_ref());

    super::margins::install_once();
    window.set_content(Some(&box_));

    // Registered only now that the content tree exists: a widget handle from before `set_content` leaves the
    // click handler unreached.
    if let Some(play_) = play_recording_button(&window) {
        register_preview_player_slot();
        wire_play_recording(&play_, &window);
    }
    // F2.5 S6: this window's one preview volume, after `set_content`; only the Cut page builds one slider.
    if preview_volume_scale(&window).is_some() {
        WINDOW_PAGE.with(
            |WindowPage {
                 preview_volumes: slots,
                 ..
             }| {
                slots.borrow_mut().push(Rc::new(std::cell::RefCell::new(
                    cut_hear::PreviewVolume::default(),
                )))
            },
        );
        // §05-cut#8 bullet 2: stream volume to full ONCE, through this app's own element, not the sound server's remembered one.
        crate::ui::preview_sound::reset_stream_volume_once();
        if let Some(scale) = preview_volume_scale(&window) {
            wire_preview_volume(&scale);
        }
    }
    // F2.6: drag surface and cross, wired after `set_content`; the seam takes the drag's seconds so both paths meet in `draw_selection`.
    if select_surface(&window).is_some() {
        WINDOW_PAGE.with(
            |WindowPage {
                 selected: slots, ..
             }| {
                slots.borrow_mut().push(Rc::new(std::cell::RefCell::new(
                    None::<cut_select::Selection>,
                )))
            },
        );
        WINDOW_PAGE.with(
            |WindowPage {
                 select_surfaces: slots,
                 ..
             }| {
                slots.borrow_mut().push(Rc::new(std::cell::RefCell::new(
                    None::<cut_select::Surface>,
                )))
            },
        );
        wire_select_surface(&window);
        // F2.4 S1: the click that places the red line; the drag above selects.
        wire_line_click_surface(&window);
        if let Some(cross) = clear_selection_button(&window) {
            wire_clear_selection(&cross, &window);
        }
        // Verb buttons are built insensitive; read the state once so none sits greyed until the first drag.
        refresh_verb_buttons(&window);
    }
    // F2.2: ▶✂ shares ▶'s player slot: two views of one preview.
    if let Some(cut_) = play_cut_button(&window) {
        wire_play_cut(&cut_, &window);
    }
    // F2.2 S5/S6: the tick loop that skips removed stretches and arms the spare pipeline.
    cut_play_leg::start_cut_preview_tick(Rc::new(CutPreviewSlot(window.clone())));
    crate::ui::cut_record_tick::start(Rc::new(window.clone()));
    crate::ui::cut_review_play::start(Rc::new(window.clone()));
    // F2.3: this window's cut slot, seeded from the project's `cut/cut.json` (the file IS the cut, §3).
    // F2.4 S4: the saved line is restored once per project open, kept only while a filmed span still covers it.
    let project_tree = layout::Tree::new(crate::startup::open_dir()).ok();
    let opened_cut = project_tree
        .as_ref()
        .and_then(|tree| cut::load(tree).ok())
        .unwrap_or_default();
    WINDOW_PAGE.with(
        |WindowPage {
             review_cuts: slots, ..
         }| {
            slots
                .borrow_mut()
                .push(Rc::new(std::cell::RefCell::new(opened_cut.clone())))
        },
    );
    // F2.13: the history opens on the SAME cut just published, so Revert returns to what this page opened with.
    reopen_history_on(&opened_cut);
    // F2.11/F2.10: SESSION recordings, pushed beside the cut. A dropped stretch only exists inside a run LONGER than
    // the kept clips, so the fold half reads these ([`crate::ui::cut_session`]); the cut's own spans are the fallback.
    push_session_recordings(crate::ui::cut_session::session_or_kept(&opened_cut));
    // The spans come from the session list just given, so the line and its tape cannot disagree on folder.
    let runs = crate::timeline::filmed_runs(page_recordings().as_slice());
    if let Some(review_) = review_cuts_button(&window) {
        wire_review_cuts(&review_, &window);
    }
    // The cut slot is filled only AFTER the Cut page's first refresh, so grey both skipping buttons again.
    crate::ui::cut_cut_play::paint_both_skipping_buttons(&window);
    let project_root = project_tree.as_ref().map(|tree| tree.dir().to_path_buf());
    if let Some(root) = project_root.clone() {
        WINDOW_PAGE.with(
            |WindowPage {
                 line_roots: slots, ..
             }| slots.borrow_mut().push(root),
        );
    }
    let restored = project_root
        .as_deref()
        .and_then(|root| cut_line::restore(root, &runs));
    if let Some(pos) = restored {
        log_line(&format!("line restored to {}", crate::tools::mm_ss(pos.t)));
    }
    WINDOW_PAGE.with(
        |WindowPage {
             line_states: slots, ..
         }| {
            slots.borrow_mut().push(Rc::new(std::cell::RefCell::new((
                restored.unwrap_or(cut_line::LinePos { t: 0.0 }),
                cut_line::LineWriter::default(),
            ))))
        },
    );
    if let Some(pos) = restored {
        crate::ui::cut_picture::note_line_moved(&window, pos.t);
        crate::ui::cut_view::follow_playhead(&window, pos.t);
    }
    for (name, _label, frames) in LINE_STEP_BUTTONS {
        if let Some(step) = line_step_button(&window, name) {
            wire_line_step(&step, &window, frames);
        }
    }
    wire_line_keys(&window);
    // F2.4 S4: closing saves the line where it stands, whatever the rate limit says; `Proceed` so the close happens.
    {
        let closing = window.clone();
        window.connect_close_request(move |_| {
            // One non-blocking main-loop beat first: `set_text` during a repaint fires `changed`, and a missed beat leaves the close nothing to flush.
            let pending = glib::MainContext::default();
            while pending.iteration(false) {}
            flush_line_on_close();
            crate::ui::narrate_text::flush_owed(&closing);
            // Also flush the WINDOW'S OWN held record: `flush_owed` reads process-global page state of the last Narrate page refreshed.
            crate::ui::window_close::flush_held_narration();
            // §2 S6: resolved through this window's own folder, not the process cwd.
            flush_session_on_close();
            // §6/§5: a settings dialog is a separate top level, so its own close hook does not run.
            crate::ui::settings::flush_owed();
            glib::Propagation::Proceed
        });
    }
    for (name, _, _) in cut_verbs::BUTTONS {
        if let Some(button) = line_step_button(&window, name) {
            let seam: fn(&adw::ApplicationWindow) -> cut_verbs::Outcome = match name {
                "add-button" => press_add,
                "split-button" => press_split,
                _ => press_remove,
            };
            wire_verb_button(&button, &window, seam);
        }
    }
    wire_delete_keys(&window);
    if let Some(button) = line_step_button(&window, "insert-button") {
        let win = window.clone();
        button.connect_clicked(move |_| {
            let status = press_insert_from_button(&win);
            if let Some(status_line) = find_status(win.upcast_ref()) {
                status_line.set_text(&status);
            }
            refresh_insert_button(&win);
        });
    }
    for (name, seam) in [
        (
            "copy-button",
            press_copy as fn(&adw::ApplicationWindow) -> String,
        ),
        ("paste-button", press_paste),
        (
            "lane-button",
            crate::ui::window_trim_strip::press_lane_from_button,
        ),
    ] {
        if let Some(button) = line_step_button(&window, name) {
            let win = window.clone();
            button.connect_clicked(move |_| {
                let status = seam(&win);
                if let Some(status_line) = find_status(win.upcast_ref()) {
                    status_line.set_text(&status);
                }
                refresh_copy_buttons(&win);
            });
        }
    }
    wire_copy_esc(&window);
    // F3.8 S3: wire FIRST among the Esc controllers: the first to stop wins, so a per-kind controller wired earlier
    // would claim the key and the hold would never drop. Order is load-bearing.
    wire_band_esc(&window);
    wire_zoom_drag(&window);
    wire_zoom_esc(&window);
    wire_speed_esc(&window);
    wire_volume_esc(&window);
    wire_label_esc(&window);
    wire_text_esc(&window);
    wire_svg_esc(&window);
    // Esc's last layer: the clip or edge in hand, then the selection.
    crate::ui::window_trim_strip::wire_hand_esc(&window);
    wire_aspect_choice(&window);
    wire_history_and_zoom(&window);
    wire_history_keys(&window);
    wire_track_strip(&window);
    crate::ui::cut_band_trim::wire(&window);
    // F2.10: fill `camera-rows` once the page is in the tree, so lookups by name succeed.
    refresh_camera_rows(&window);
    // §06-effects#2: `page_box` builds the lane and preview holders empty, as the window is not in the tree yet.
    refresh_effects_lane(&window);
    wire_fold_all(&window);
    refresh_fold_badges(&window);
    crate::ui::cut_view::refresh_page_scrollbar(&window);
    wire_wheel_gestures(&window);
    crate::ui::cut_view::attach_scrollbar(&window);
    crate::ui::window::wire_band_line_press(&window);
    crate::ui::cut_select_press::attach(&window);
    cut_form_frame::wire_close(&window);
    cut_picture::wire_click(&window);
    crate::ui::cut_preview_frame::install_draw(&window);
    crate::ui::cut_suggest_land::install(newest_review_cut, |window, landed| {
        publish_cut(landed);
        // note_change before suggested: Revert then keeps the model's work and drops only hand edits after it.
        cut_history(window).borrow_mut().note_change(landed);
        cut_history(window).borrow_mut().suggested(landed);
        refresh_history_buttons(window);
        crate::ui::refresh_effects_lane(window);
        crate::ui::window_bench::refresh_cut_readouts(window);
        crate::ui::cut_view::repaint_page(window);
    });
    crate::ui::cut_suggest_progress::install(|window, progress| {
        let (Some(bar), Some(status)) = (
            crate::ui::window::progress_bar(window),
            crate::ui::window::find_status(window.upcast_ref()),
        ) else {
            return;
        };
        if progress.pulsing {
            bar.pulse();
        } else {
            bar.set_fraction(progress.fraction.clamp(0.0, 1.0));
        }
        status.set_text(&progress.line);
    });
    // §07-narrate#1-screen: the volume slider needs a slot before it is wired (F2.5 S6), or a drag writes into nothing.
    crate::ui::narrate_volume::ensure_slot(&window);
    crate::ui::narrate_page::wire(&window);
    crate::ui::produce_page::wire(&window);
    // The Cut page opens on the cut the project holds: readings and the frame at the restored line.
    refresh_cut_readouts(&window);
    // The line restored above went into the preview slot that was newest THEN; seat it in the Narrate page's
    // slot too, or ▶ plays from 0:00. Inventory §D: one of ▶, ▶✂, ▶✂✂ is lit from the first frame.
    crate::ui::cut_record_play::light_lit(&window);
    match restored {
        Some(pos) => set_playhead(&window, pos.t),
        None => {
            crate::ui::cut_picture::refresh(&window);
        }
    }
    window
}

/// F2.11 S1: create `fold-all-button` in the `gutter-badges` overlay (§B's 30 px gutter) and wire it to
/// [`press_fold_all`]; the beside-the-rows holder is the fallback for a page without a band.
fn wire_fold_all(window: &adw::ApplicationWindow) {
    // Look in the holder, not tree-wide: a closed window's widgets live while another window's tree exists, so
    // a name search can return a button parented to a DIFFERENT window and this one never gets its badge.
    let Some(holder) = crate::ui::cut_tracks::gutter_badges_box(window) else {
        return;
    };
    if holder_children_named(&holder, "fold-all-button") {
        return;
    }
    let button = gtk::Button::new();
    button.set_widget_name("fold-all-button");
    button.set_label("\u{2212}\u{25a4}");
    button.set_tooltip_text(Some(
        "fold every stretch the cut drops \u{00b7} press again to open them all",
    ));
    let win = window.clone();
    button.connect_clicked(move |_| {
        press_fold_all(&win);
    });
    holder.append(&button);
}

/// Where a tab click is decided: ask the shell, redraw from what it says, and put the strip back if it refused.
#[allow(clippy::too_many_arguments)] // one handler needs every tab's own widgets: the switcher, its
                                     // guard, and the readouts a tab change repaints — none of them belong together in a struct
fn wire_switching(
    stack: &adw::ViewStack,
    switcher: &adw::ViewSwitcher,
    status: &gtk::Label,
    shell: &Rc<RefCell<Shell>>,
    guard: &Rc<std::cell::Cell<bool>>,
    session: &Rc<RefCell<Project>>,
    inputs_readout: &gtk::Label,
    outputs_folder: &gtk::Button,
    outputs_readout: &gtk::Label,
    held_narration: &Rc<RefCell<Narration>>,
) {
    let stack = stack.clone();
    let switcher = switcher.clone();
    let status = status.clone();
    let shell = shell.clone();
    let guard = guard.clone();
    let session = Rc::clone(session);
    let inputs_readout = inputs_readout.clone();
    let outputs_folder = outputs_folder.clone();
    let outputs_readout = outputs_readout.clone();
    let held_narration = held_narration.clone();
    stack.connect_visible_child_name_notify(move |stack| {
        if guard.get() {
            return;
        }
        let shown = stack.visible_child_name().unwrap_or_default().to_string();
        let Some(target) = Page::all().into_iter().find(|page| page.label() == shown) else {
            return;
        };
        // The LIVE session, read at the click: the build-time copy knows nothing of later added sources.
        let project = session.borrow().clone();
        // S1-S5 are in shell.rs. A folder that is not a project leaves `tree` None: the page shows what it has (§1).
        let (tree, cut, _) = session_reads();
        // The window's held copy: S5 refits on arrival and S3 writes on leaving, two separate switches.
        let held = Rc::clone(&held_narration);
        // S3: ask BEFORE the switch clears the flag.
        let owed = shell.borrow().narration_pending.owe();

        let outcome = shell.borrow_mut().switch(
            target,
            Move::Click,
            &project,
            tree.as_ref(),
            &cut.segs,
            &mut held.borrow_mut().entries,
        );

        if owed {
            if let Some(tree) = tree.as_ref() {
                if let Err(why) = narration::save_held(&held.borrow(), tree) {
                    crate::ui::window_logs::say_failure(&format!(
                        "!!! could not save the narration -- {why}"
                    ));
                }
            }
        }
        // §F4.7 (Refit): the refit is work the app did, so it reaches disk even if nobody typed; the `owed` flush covers typing only.
        if let Outcome::Shown {
            refresh: Refresh::NarrateRefit { moved, orphan },
            ..
        } = outcome
        {
            if moved + orphan > 0 {
                if let Some(tree) = tree.as_ref() {
                    if let Err(why) = narration::save_held(&held.borrow(), tree) {
                        crate::ui::window_logs::say_failure(&format!(
                            "!!! could not save the narration -- {why}"
                        ));
                    }
                    // The rows draw from the page's own copy: publish the refitted lines there too.
                    crate::ui::narrate_page::publish_lines(held.borrow().entries.clone());
                }
            }
        }
        paint_tabs(&switcher, &shell.borrow(), &project);
        paint_readouts(
            &shell.borrow(),
            &project,
            tree.as_ref(),
            &cut,
            &held.borrow(),
            inputs_readout.upcast_ref(),
            &outputs_folder,
            outputs_readout.upcast_ref(),
        );
        status.set_text(&shell.borrow().status);

        if let Outcome::Bounced { .. } = outcome {
            guard.set(true);
            stack.set_visible_child_name(shell.borrow().page.label());
            guard.set(false);
        }
    });
}

pub use crate::ui::window_policy::{
    built_window, derive_policy, live_session_policy, main_window, policy_is_current,
    set_policy_sender_for_tests, set_session_context,
};
pub(crate) use crate::ui::window_policy::{schedule_policy_derive, user_context, wire_settings};

pub(crate) type LineStates =
    std::cell::RefCell<Vec<Rc<std::cell::RefCell<(cut_line::LinePos, cut_line::LineWriter)>>>>;

pub use crate::ui::window_line_step::{
    arrow_steps, flush_line_on_close, line_key_controller, line_position, line_root,
    line_step_button, place_line_press, set_line_position, space_toggles, step_frames_of, Step,
    NO_RECORDING_STATUS,
};
pub(crate) use crate::ui::window_line_step::{
    flush_session_on_close, wire_line_keys, wire_line_step, LINE_STEP_BUTTONS,
};

pub use crate::ui::window_surfaces::{
    band_line_gesture_for_test, line_click_gesture, wire_band_line_press, SELECT_SURFACE_PPS,
};
pub(crate) use crate::ui::window_surfaces::{
    store_band_line_gesture, wire_clear_selection, wire_line_click_surface, wire_select_surface,
};

fn wire_verb_button(
    button: &gtk::Button,
    window: &adw::ApplicationWindow,
    press: fn(&adw::ApplicationWindow) -> cut_verbs::Outcome,
) {
    let window = window.clone();
    button.connect_clicked(move |_| {
        press(&window);
    });
}

use crate::ui::cut_verb_press::{press_add, press_remove, press_split};

pub use crate::ui::window_review::clamp_effects_to_cut;
pub(crate) use crate::ui::window_review::set_review_segs;

pub use crate::ui::window_holds::{
    apply_pick, set_held_clip, set_held_edge, set_held_effect, set_watched_row, watched_row,
};

pub use crate::ui::cut_view::{band_clip_boxes_for_test, strip_clip_boxes_for_test, strip_pps};

pub use crate::ui::window_insert::{
    close_insert_form, insert_chosen_with_length, insert_open, press_insert, press_insert_apply,
    visible_insert_form,
};
pub(crate) use crate::ui::window_insert::{
    press_insert_from_button, refresh_insert_button, save_insert_cut,
};

pub const STRIP_HEIGHT: i32 = crate::ui::cut_trim_press::STRIP_H as i32;

/// F2.8 S1: a mark every 30 s is every 120 px at the open zoom, as `spec/img/05-trim.png` prints.
pub const RULER_EVERY_SECONDS: f64 = 30.0;

/// F2.8 S1: placeholder strip width; at [`TRACK_STRIP_PPS`] 480 px covers two minutes, keeping the first ticks and the clips on screen.
pub const STRIP_WIDTH: i32 = 480;

pub(crate) use crate::ui::window_narrate::live_project;
pub use crate::ui::window_narrate::{
    narrate_pulsed, narrate_script_loaded, narrate_session_tree, narrate_stage, narrate_stages,
    narrate_wrote, press_narrate_run, refuse_narrate_run, set_narrate_script,
    set_narrate_script_slow, set_status_line,
};

fn wire_review_cuts(button: &gtk::Button, window: &adw::ApplicationWindow) {
    let window = window.clone();
    button.connect_clicked(move |_| {
        let _ = press_review_cuts(&window, &newest_review_cut());
    });
}
pub(crate) use crate::ui::window_save::wire_save;

pub(crate) use crate::ui::window_logs::log_line;
pub use crate::ui::window_logs::{
    clear_logs_for_test, logs_for_test, note_startup, note_unload, window_logs,
};

pub(crate) use crate::ui::window_history_zoom::{
    cut_history, publish_cut, record_edit, refresh_history_buttons, wire_aspect_choice,
    wire_history_and_zoom, wire_history_keys, wire_wheel_gestures, wire_zoom_drag, wire_zoom_esc,
    zoom_form_owner,
};
pub use crate::ui::window_history_zoom::{
    history_key_controller, note_edit, press_clear_cut, press_revert, press_undo, press_wave,
    press_zoom, press_zoom_apply, press_zoom_cancel, press_zoom_esc, reopen_history_on,
    zoom_gesture,
};

struct CutPreviewSlot(adw::ApplicationWindow);

impl cut_play_leg::CutPreviewPage for CutPreviewSlot {
    fn cut_only(&self) -> bool {
        preview_player().cut_only
    }
    fn player_second(&self) -> Option<f64> {
        crate::ui::cut_record_play::playing_second()
    }
    fn playing(&self) -> bool {
        preview_player().transport.playing
    }
    fn playhead(&self) -> Option<f64> {
        preview_player().playhead
    }
    fn set_playhead(&self, at: f64) {
        set_playhead(&self.0, at)
    }
    fn cut(&self) -> Cut {
        newest_review_cut()
    }
    fn volume(&self) -> f64 {
        preview_volume()
    }
    fn arm_spare_at(&self, at: f64) -> bool {
        crate::ui::cut_cut_play::preroll_spare(at)
    }
    fn cross_into_spare(&self) -> bool {
        crate::ui::cut_cut_play::swap_spare()
    }
    fn drop_spare(&self) {
        crate::cut_play_back::drop_spare();
    }
    fn hold_still(&self, hold: bool) -> bool {
        crate::ui::cut_cut_play::freeze(hold)
    }
    fn play_rate(&self, rate: f64, at: f64) -> bool {
        crate::ui::cut_cut_play::hand_rate(rate, at)
    }
    fn apply_gain(&self, gain: f64, at_secs: f64) {
        crate::ui::cut_cut_play::hand_gain(gain, at_secs);
    }
    fn pause_at_end(&self) {
        crate::ui::cut_cut_play::pause_media();
        crate::ui::window_line_step::pause_for_step(&self.0);
    }
    fn say_clock(&self, clock: f64, ended: bool) {
        if let Some(status) = find_status(self.0.upcast_ref()) {
            status.set_text(&cut_play_leg::clock_line(clock, ended));
        }
        // §D: the ✂ clock carries its own explanation as a tooltip: cut time, total, session second.
        let len = cut_play_leg::kept_length(&newest_review_cut());
        crate::ui::cut_cut_play::set_clock_tooltip(&self.0, clock, len, preview_playhead());
    }
}

pub use crate::ui::window_doors::{
    add_sources_button, copy_into_project, find_source_widget, find_status, inputs_readout,
    log_expander, mark_narration_owed, new_button, new_tooltip, open_button, outputs_folder_button,
    outputs_label, outputs_readout, outputs_sentence, paint_project_title, play_button,
    play_tooltip, progress_bar, progress_tooltip, rescan_button, rescan_tooltip, run_queue,
    running_step, save_button, save_tooltip, session_handle, session_sources, set_marking_pass,
    state, stop_button, tab_button, tab_dimmed, tab_info_button, tab_info_parts, tab_tooltip,
};
pub(crate) use crate::ui::window_doors::{find_switcher, find_widget_by_name};
pub use crate::ui::window_doors::{switch_latch, view_stack};

pub use crate::ui::window_project::{
    add_chooser_shown_count, add_files, last_ask, last_open_ask_for_test, name_chosen,
    new_project_confirm, new_project_dialog, open_folder, open_from, reset_add_chooser_shown,
    OPEN_STOP_FIRST,
};
pub(crate) use crate::ui::window_project::{
    remember_opened, wire_add, wire_new, wire_open, wire_rescan, NEW_TIP, OPEN_TIP, RESCAN_TIP,
    SAVE_TIP, SETTINGS_TIP,
};

use crate::ui::window_page_state::{WindowPage, PAGE as WINDOW_PAGE};
pub(crate) use crate::ui::window_preview::live_player;
pub use crate::ui::window_preview::newest_review_cut;
pub use crate::ui::window_preview::{
    play_cut_button, play_recording_button, press_play_cut, press_play_recording,
    press_review_cuts, preview_player, preview_volume_scale, publish_preview_transport,
    register_preview_player_slot, review_cut_segs, review_cut_segs_count, review_cut_shift,
    review_cuts_button, review_lanes, seed_review_cut, set_playhead, set_preview_state,
};
