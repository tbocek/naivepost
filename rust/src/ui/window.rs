//! The window: renders a [`Project`]'s state and nothing else.
//!
//! No rules live here (spec/00-principles.md §5) — a page is a label of the
//! state it was handed, so every value on screen traces back to the model.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use gtk4 as gtk;

// F3.5 ▨ SVG: the page half of the drawing flow lives in `cut_svg`; these are the doors this shell calls.
// F3.3 ⏩ Speed and stop by hand: the page half of that flow lives in `cut_speed_hand`; these are the
// doors this shell calls.
use crate::ui::cut_speed_hand::{wire_band_esc, wire_speed_esc};
use crate::ui::cut_volume_hand::wire_volume_esc;
use crate::ui::cut_text_hand::wire_text_esc;
use crate::ui::cut_label_hand::wire_label_esc;
use crate::ui::cut_folds::{
    page_recordings, press_fold_all, push_session_recordings, refresh_fold_badges,
};
use crate::ui::cut_cameras::refresh_camera_rows;
use crate::ui::cut_form_frame;
use crate::ui::cut_picture;
use crate::ui::save_apply;
// F3.5 ▨ SVG: the page half of the drawing flow lives in `cut_svg`; these are the doors this shell calls.
use crate::ui::cut_svg::wire_svg_esc;
use crate::cut::{self, Cut};
use crate::cut_hear;
use crate::cut_play_leg;
use crate::cut_select;
use crate::cut_verbs;
use crate::cut_line;
use crate::layout;
use crate::project::Project;
use crate::lucky;
use crate::run;
use crate::runqueue;
use crate::exchanges;
use crate::policy;
use crate::ui::policy_form;
use crate::startup;
use crate::narration::{self, Narration};
use crate::shell::{Move, Outcome, Page, Refresh, Shell};

/// §1's badge **15** is a folder button beside the count; its tooltip names what the button opens.
/// The page-specific wording §1 mentions lives in `Shell::outputs`' own doc — the button itself is
/// one control for every page, so it gets one sentence here rather than four near-duplicates.
pub(crate) const OUTPUTS_FOLDER_TIP: &str = "Open this project's output folder";

pub const APP_ID: &str = "ch.bocek.naivepost";

// The page's widget tree (one page per tab), the Cut toolbar groups and their effect popover, and
// Prepare's source list and frame controls live in `window_page_box` (window.rs is past its size
// budget); these are the same names, re-exported under the paths every other module reaches them through.
pub(crate) use crate::ui::window_page_box::page_box;

// The effect dropdown's record door lives in `window_effect_item` (window.rs is past its size budget);
// this is the same name, re-exported under the paths every other module and test already reaches it through.
pub use crate::ui::window_effect_item::press_effect_item;

// The §05-cut F3.1 zoom-by-hand arm group lives in `cut_zoom_hand` (window.rs is past its size budget);
// these are the doors the rest of the page and the other ui modules call, kept at the paths they had.
pub use crate::ui::cut_zoom_hand::{
    note_zoom_place, press_zoom_item, preview_picture_px, set_zoom_armed, zoom_armed,
    zoom_drag_ended, zoom_drag_ended_with_source, zoom_entry, zoom_form_box, zoom_form_open,
    PREVIEW_PANEL_H, PREVIEW_PANEL_W,
};
pub(crate) use crate::ui::cut_zoom_hand::{
    set_zoom_form, show_zoom_form, trim_seconds, zoom_field_row,
};




// The §05-cut effects-lane group lives in `cut_lane_rows` (window.rs is past its size budget); these are
// the doors the rest of the page and the other ui modules call, kept at the paths they had.
pub use crate::ui::cut_lane_rows::{cut_bar_width_px, effects_lane_box, refresh_effects_lane};
pub(crate) use crate::ui::cut_lane_rows::{preview_widget, set_class};



// The §1 bench, tabs and readouts group lives in `window_bench` (window.rs is past its size budget);
// these are the doors the rest of the page calls, kept at the paths they had.
pub use crate::ui::window_bench::{label, paint_progress, paint_run_bar, set_bench_open_row};
pub(crate) use crate::ui::window_bench::{
    depth, has_label, paint_readouts, paint_readouts_for,
    paint_tabs, paint_tabs_for, refresh_cut_readouts, tab_buttons, LOG_OPEN_PX,
};

// The §05-cut selection / held-effect / live-form group lives in `window_selection` (window.rs is past its
// size budget); these are the doors the rest of the page and the other ui modules call, kept at their paths.
pub use crate::ui::window_selection::{
    apply_mix, clear_selection, clear_selection_button, drag_held_band, draw_selection,
    live_form_answer_is_undo, live_form_refused, nudge_held_effect, nudge_selection,
    open_effect_form, press_band_esc, press_edit_effect, press_effect_bar, press_fx_lane,
    preview_playhead, preview_volume, pressed_surface, remove_effect_at, remove_held_effect,
    select_surface, selection, selection_gesture, selection_readout, selection_surface,
    selection_verbs, set_preview_volume,
};
pub(crate) use crate::ui::window_selection::{
    held_narration, refresh_selection_readout, refresh_verb_buttons,
    HELD_NARRATION, PREVIEW_VOLUMES, SELECT_SURFACES,
    SELECTED,
};

// The §F2.8 trim/move/copy/paste + track-strip group lives in `window_trim_strip` (window.rs is past its
// size budget); these are the doors the rest of the page and the other ui modules call, kept at their paths.
pub use crate::ui::window_trim_strip::{
    copy_esc_controller, copy_hand, delete_key_controller, move_gesture, note_place, open_folds,
    paste_line, press_band, press_copy, press_lane, press_move, press_move_at, press_paste,
    press_position, press_row, press_scene, press_trim_border, right_press_took, set_press_band,
    set_press_scene, set_press_x, track_strip, trim_gesture, TRACK_REC_END, TRACK_STRIP_PPS,
};
pub(crate) use crate::ui::window_trim_strip::{
    refresh_copy_buttons, wire_copy_esc, wire_delete_keys, wire_track_strip,
};

// The §02 run-bar presses live in `window_run_press` and the three pass jobs in `cut_pass_gates`
// (window.rs is past its size budget); these are the doors the rest of the page calls, kept at their paths.
pub use crate::ui::cut_pass_gates::{
    CaptionBatch, captions_pass_button, decorations_pass_button, refresh_captions_gate, refresh_decorations_gate,
    refresh_speeds_gate, run_captions_pass, run_captions_pass_with_reply, speeds_pass_button,
    set_captions_pass, set_decorations_pass, set_speeds_pass,
};
pub use crate::ui::cut_pass_gates::SPEEDS_PASS_LABEL;
pub(crate) use crate::ui::cut_pass_gates::{wire_captions_pass, wire_decorations_pass, wire_speeds_pass};
pub(crate) use crate::ui::window_run_press::{
    live_transport, wire_lucky, wire_play, wire_play_cut, wire_play_recording, wire_preview_volume,
    wire_stop,
};

/// The window's switching state and what it decided on arrival. `help_page` is read from the shell
/// by S4's own test; here only what is on screen.
#[derive(Debug, PartialEq)]
pub struct UiState {
    pub page: Page,
    pub status: String,
    /// F0.5 S1: whether the log expander is open — read from the widget, so a test checks the same
    /// thing the user sees rather than a flag that might not have been drawn.
    pub log_expanded: bool,
    /// F0.5 S2: how far the run's progress bar has got, 0 when idle.
    pub progress: f64,
}

/// Build the window showing `project`. `page` selects the visible tab.
pub fn build_window(app: &impl IsA<gtk::Application>, project: &Project, page: &str) -> adw::ApplicationWindow {

    let window = adw::ApplicationWindow::new(app);
    // spec/03-shell.md §1: "Its minimum size is 1240×900: it opens at that size and cannot be made
    // smaller." The default is that size, and the size REQUEST is what makes it a floor: GTK 4 refuses to
    // shrink a window under its own minimum, so `set_size_request` here is the whole of "cannot be made
    // smaller" (`tests/suite/window_shrinks.rs` measures that no page pushes it higher).
    window.set_default_size(crate::shell::WINDOW_MIN_W, crate::shell::WINDOW_MIN_H);
    window.set_size_request(crate::shell::WINDOW_MIN_W, crate::shell::WINDOW_MIN_H);
    // §1's other half of that sentence: "One window, title \"Naivepost\"". Without it GTK titles the
    // window from the application id, which read "naivepost" in the title bar and the task switcher.
    window.set_title(Some(crate::shell::WINDOW_TITLE));
    // §7's launch lines wait until there is a log to put them in.
    note_startup(&[]);

    // Where the window is and what it has to say: every rule behind a switch lives in shell.rs.
    let shell = Rc::new(RefCell::new(start_shell(page)));
    // S3's pending write lives inside this shell; publishing the handle is what lets the seam below
    // mark one owed. The Narrate page will call the same seam from its text view when F4.7 lands one.
    WINDOW_SHELLS.with(|shells| shells.borrow_mut().push(Rc::clone(&shell)));

    // S5 refits the narration lines when the page is *entered*, and S3 writes that refit out when
    // the page is *left* — two switches, so the lines have to survive between them. Reading them
    // afresh on every switch (what `session_reads` does) makes the refit land in a local that is
    // dropped at the end of the handler, and leaving Narrate then re-reads the pre-refit file: the
    // flush would write back what the disk already had. This is the window's copy; F4.7's text view
    // edits it too.
    let held_narration = Rc::new(RefCell::new(session_reads().2));
    HELD_NARRATION.with(|held| held.borrow_mut().push(Rc::clone(&held_narration)));

    // F0.7's record of which User Context this window's policy answers to, pushed beside the other
    // per-window slots so `derive_policy` reaches the newest one. Fresh: a window has derived nothing
    // yet, which is exactly what makes the first ▶ ask.
    POLICY_TRACKERS.with(|trackers| trackers.borrow_mut().push(Rc::new(RefCell::new(policy::Tracker::new()))));
    POLICY_WATCH_WINDOW.with(|watch| *watch.borrow_mut() = Some(window.clone()));

    // The window is handed an immutable `&Project` and holds no live project yet, so the flows that
    // change the session — Rescan (F0.11) and Add sources (F0.12) — work on one private copy shared
    // between them. F0.9's live project state replaces this; until then it is what keeps the session
    // each flow leaves behind for the next. Made before the pages so Prepare's rows can hold a handle
    // to it: a row that moved the session has to move the one copy every other flow reads.
    let session = Rc::new(RefCell::new(project.clone()));
    SESSION.with(|slots| slots.borrow_mut().push(Rc::clone(&session)));
    // F1.1: the ▶ handler asks `prepare_run` about the LIVE session rather than the build-time
    // `project` argument, so a source added or removed since the window opened is what gets asked
    // about. Published here for the same reason `SESSION` is.
    PLAY_SESSION.with(|slots| slots.borrow_mut().push(Rc::clone(&session)));

    // §2 S6's folder for THIS window, remembered before anything can move the cwd: the close hook writes
    // the session here rather than into `project_autosave::session_dir()`, which asks the process cwd at
    // close time — wrong once a flow has moved the session (F0.10's Save as, F0.11's Rescan) or a test has
    // pinned its cwd, and a close must never create or rewrite somebody else's folder.
    let session_folder = startup::session_dir(&std::env::current_dir().unwrap_or_default());
    SESSION_DIRS.with(|held| held.borrow_mut().push(session_folder.clone()));
    crate::project_autosave::install(Rc::clone(&session), session_folder.clone());
    crate::project_autosave::start();
    // The folder this window has open: the working copy beside its launch root. A rename or an Open
    // moves it afterwards -- that is what `save_target` answers -- but a fresh window always starts on
    // its own root's session, whatever an earlier window in this process left behind.
    save_apply::set_open_dir(&startup::session_dir(&std::env::current_dir().unwrap_or_default()));
    // The Cut page's ▶ resolves its session through `cut_suggest_land`'s own slot (§12 §4's walk-back
    // reads the words from there), so a fresh window installs its root's session rather than inheriting
    // whatever an earlier window in this process left behind.
    crate::ui::window_line_step::push_session_tree(layout::Tree::new(&session_folder).ok());

    // The status line: the shell's sentence, right-aligned in the bottom row (§1's "status line").
    // Also before the pages, for the same reason — a row's press reports itself there.
    let status = gtk::Label::new(Some(""));
    status.set_widget_name("status-line");
    status.set_xalign(1.0);
    status.set_hexpand(true);
    status.set_ellipsize(gtk::pango::EllipsizeMode::End);
    status.add_css_class("dim-label");
    status.set_margin_end(8);

    let stack = adw::ViewStack::new();
    let mut prepare_add: Option<(gtk::Button, gtk::CheckButton)> = None;
    for page in Page::all() {
        let name = page.label();
        let (child, add_, copy_) = page_box(name, project, &session, &status, &window);
        // The tab row draws each page's icon; a page added without one gets libadwaita's
        // "image-missing" glyph.
        stack.add_titled_with_icon(&child, Some(name), name, page.icon());
        if let (Some(add_), Some(copy_)) = (add_, copy_) {
            prepare_add = Some((add_, copy_));
        }
    }
    if !page.is_empty() {
        stack.set_visible_child_name(page);
    }

    // The tab strip is the ViewStack's own title widget: a TabBar needs a TabView.
    let switcher = adw::ViewSwitcher::builder()
        .stack(&stack)
        .policy(adw::ViewSwitcherPolicy::Wide)
        .build();
    paint_tabs(&switcher, &shell.borrow(), project);

    // Where ▶ is decided: the run bar's own state, beside the shell's. Nothing about which of
    // pause / transport / start applies is worked out here — run.rs does that (F0.2).
    let bar = Rc::new(RefCell::new(run::RunBar::default()));
    // Published the same way `SESSION` is, so a test can read which step the bar holds instead of
    // inferring it from a button's icon. One window per test binary here, so `last()` is this one.
    RUN_BARS.with(|bars| bars.borrow_mut().push(Rc::clone(&bar)));
    WINDOW_BAR.with(|slots| slots.borrow_mut().push(Rc::clone(&bar)));

    // The children a run has out there, so ⏹ can reach them (F0.3 S3). Held here rather than inside
    // the bar because the flows that spawn register into it and the bar is what decides to stop;
    // nothing spawns yet, so this stays empty until the runner's round arrives.
    let procs = Rc::new(RefCell::new(run::Subprocesses::default()));

    // The tab row is a click; the stack is where that click is decided. `guard` keeps the bounce's
    // own write-back from re-entering this handler, which would otherwise recurse through two more
    // notify signals (the prototype does the same with tabGuard, gui/main.go:1209-1224).
    let guard = Rc::new(std::cell::Cell::new(false));
    WINDOW_SWITCH_LATCH.with(|latch| *latch.borrow_mut() = Some(Rc::clone(&guard)));

    // §1's badges **14** and **15**: the visible tab's `Inputs:` readout, then its `Outputs:`
    // count with the folder button beside it. They are built before the switch is wired because a
    // tab click repaints them, and they are window-level rather than page-level so a switch updates
    // one pair instead of four. Both texts come from `Shell::inputs` / `Shell::outputs`; nothing
    // here composes a sentence.
    let readout_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    readout_row.set_widget_name("readout-row");
    readout_row.set_margin_start(8);
    readout_row.set_margin_end(8);
    let inputs_readout = gtk::Label::new(None);
    inputs_readout.set_widget_name("inputs-readout");
    inputs_readout.set_xalign(0.0);
    inputs_readout.add_css_class("dim-label");
    readout_row.append(&inputs_readout);
    let outputs_folder = gtk::Button::from_icon_name("folder-symbolic");
    outputs_folder.set_widget_name("outputs-folder-button");
    outputs_folder.set_tooltip_text(Some(OUTPUTS_FOLDER_TIP));
    outputs_folder.set_valign(gtk::Align::Center);
    // §1 badge **15**: the press opens the folder the row is naming, read at press time so a switch or a
    // Save-as moves what opens (`window_outputs`).
    super::window_outputs::wire(&outputs_folder);
    readout_row.append(&outputs_folder);
    let outputs_readout = gtk::Label::new(None);
    outputs_readout.set_widget_name("outputs-readout");
    outputs_readout.set_xalign(0.0);
    outputs_readout.set_hexpand(true);
    outputs_readout.add_css_class("dim-label");
    readout_row.append(&outputs_readout);

    wire_switching(
        &stack,
        &switcher,
        &status,
        &shell,
        &guard,
        project,
        &inputs_readout,
        &outputs_folder,
        &outputs_readout,
        &held_narration,
    );

    // S4 at startup: the row is drawn for the page the window opened on, not left blank until a tab
    // is clicked. Same read and same painter as the switch handler, so the two can never disagree. The
    // header's project name/path is drawn by the same repaint (`paint_readouts`), so a switch and this
    // startup call leave the same text.
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

    // F0.5's bookkeeping, drawn: the bar that says how far the run has got and the log it writes
    // into. Both are window-level (built once here, not per page), so a name lookup cannot land on a
    // second copy the way a same-named page button could.
    let queue = Rc::new(RefCell::new(runqueue::Queue::new()));
    WINDOW_QUEUE.with(|slots| slots.borrow_mut().push(Rc::clone(&queue)));
    let exchange_log = Rc::new(RefCell::new(exchanges::RunLog::new()));

    // The progress bar sits between ⏹ and the status line: §2's bar reads play, stop, gears, then
    // the run's fraction and its words. Idle is 0 rather than hidden — a bar that vanishes makes the
    // row jump when a run starts.
    let progress = gtk::ProgressBar::new();
    progress.set_widget_name("run-progress");
    progress.set_hexpand(true);
    progress.set_valign(gtk::Align::Center);
    // §2: with nothing queued the bar keeps this standing tooltip; a track's counting replaces it
    // while there is one, and leaves when the queue empties.
    progress.set_tooltip_text(Some(runqueue::STANDING_TIP));

    // §1: "the log expander, its header carrying the status line". The header is a row of two: the
    // word "Log" so the control is never an unlabelled caret (at rest the status is empty), and the
    // status label itself filling the rest. `state()` still reads the label by name, so every existing
    // assertion on `state().status` keeps working unchanged — only its parent moved.
    let log_view = gtk::TextView::new();
    log_view.set_widget_name("log-view");
    // §1: the run's exchange page is the log's one clickable line; everything else stays plain text.
    super::log_link::install(&log_view);
    log_view.set_editable(false);
    log_view.set_cursor_visible(false);
    log_view.set_monospace(true);
    log_view.set_wrap_mode(gtk::WrapMode::WordChar);
    log_view.set_vexpand(true);
    // §10 lists the log's heights only in prose ("log heights 220/110") and gives no `P.` id, so the
    // numbers go on the widget with this note rather than as invented parameter rows.
    //
    // "the page and bottom bar are two halves of a draggable divider").
    // A TextView is not scrollable by itself: without its own scroller it reports the whole document's
    // height as its minimum, which would pin the window tall again the moment the log had lines in it.
    let log_scroller = gtk::ScrolledWindow::new();
    log_scroller.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    // 220 px is the OPEN height the scroller offers, not a floor on the window: a ScrolledWindow's
    // minimum is what `min_content_height` sets only while its content asks for more, and here the
    // content is a text view that vexpands, so the shell can be dragged under 220 px and the log
    // scrolls. That is what makes the run bar + log stop pinning the window's height.
    log_scroller.set_min_content_height(LOG_OPEN_PX);
    log_scroller.set_child(Some(&log_view));
    log_scroller.set_child(Some(&log_view));

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
    log.set_expanded(false);

    // §2's run bar: ▶ at the left of the row above the log, ⏹ next to it (F0.3), then the "I'm
    // feeling lucky" gears (F0.4), then the progress bar F0.5 adds.
    let play = gtk::Button::from_icon_name(run::PLAY_ICON);
    play.set_widget_name("play-button");
    play.add_css_class("suggested-action");
    // ⏹ beside ▶, in the order §2's run bar reads: play, stop, then the status line.
    let stop_ = gtk::Button::from_icon_name(run::STOP_ICON);
    stop_.set_widget_name("stop-button");
    stop_.set_tooltip_text(Some(run::STOP_TOOLTIP));
    // The gears after ▶: labelled rather than icon-only, because §03 names the control by its words and
    // the spec's run-bar image shows the phrase on the button. §2's order is ▶ , lucky , ⏹ .
    let lucky = gtk::Button::with_label(lucky::LUCKY_LABEL);
    lucky.set_widget_name("lucky-button");
    lucky.set_tooltip_text(Some(lucky::LUCKY_TOOLTIP));
    paint_run_bar(
        &play,
        &lucky,
        &stop_,
        &bar.borrow(),
        shell.borrow().page,
        live_transport(),
    );

    let controls = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    // §1: this box is what scrolls. The scroller around it (`window_run_scroller`) carries the name
    // `run-controls`, and the box inside it carries `run-controls-inner`, so a door or a test that wants
    // ▶ / 🎲 / ⏹ finds the buttons' own row without guessing which of the two wrappers it walked into.
    controls.set_widget_name("run-controls-inner");
    controls.append(&play);
    controls.append(&lucky);
    controls.append(&stop_);
    // F0.7's policy gear, right beside the "I'm feeling lucky" gears rather than in a toolbar of its own:
    // §03 puts both kinds of ⚙ on the same bar, and a second row of controls would read as a new area
    // of the window that no spec page drew.
    let policy_button = gtk::Button::from_icon_name("emblem-system-symbolic");
    policy_button.set_widget_name("policy-button");
    policy_button.set_tooltip_text(Some(policy_form::OPEN_TIP));
    controls.append(&policy_button);
    wire_policy(&policy_button, &window);
    // F3.9's ✐ Captions lives on THIS bar, not on the Cut toolbar: §05 #1-screen pins that toolbar to six
    // declared groups (`cut_screen::TOOLBAR_GROUPS`), and F3.9 names no §1 item, so its control cannot join
    // that order without inventing a seventh group the inventory does not list. A caption pass is a JOB run
    // after the cut, which is what this row is for -- beside ▶ / ⏹ / 🎲 / ⚙, the other things you start.
    // Created once per window here (before any lookup) so re-entering the tab cannot stack a second button;
    // the widget NAME is unchanged from the toolbar attempt, so tests and snapshots find it where they always did.
    let captions = gtk::Button::with_label(CAPTIONS_PASS_LABEL);
    captions.set_widget_name("captions-pass-button");
    captions.set_tooltip_text(Some(
        "ask the model for captions over the kept clips \u{2014} \u{21b6} Undo takes the pass back",
    ));
    controls.append(&captions);
    refresh_captions_gate(&window);
    wire_captions_pass(&captions, &window);
    // F3.10's ⏩ Speeds beside it, for the same reason: one call after the cut asking which stretches run
    // dull is a job, not one of §1's toolbar items, so it sits with the other jobs and NOT in `cut-toolbar`
    // (whose six groups §05 #1-screen pins).
    let speeds = gtk::Button::with_label(SPEEDS_PASS_LABEL);
    speeds.set_widget_name("speeds-pass-button");
    speeds.set_tooltip_text(Some(
        "ask the model which clips run fast \u{2014} \u{21b6} Undo takes the pass back",
    ));
    controls.append(&speeds);
    refresh_speeds_gate(&window);
    wire_speeds_pass(&speeds, &window);
    // F3.11's ✨ Decorations beside those two, for the same reason: one call after the cut asking for zooms,
    // a stop and some volume is a job, not one of §1's toolbar items, so it sits with the other jobs and NOT
    // in `cut-toolbar` (whose six groups §05 #1-screen pins via `cut_screen::TOOLBAR_GROUPS`). Created once
    // per window here, before any lookup, so re-entering the tab cannot stack a second button.
    let decorations = gtk::Button::with_label(DECORATIONS_PASS_LABEL);
    decorations.set_widget_name("decorations-pass-button");
    decorations.set_tooltip_text(Some(
        "ask the model for zooms, a stop and some volume over the kept clips \u{2014} \u{21b6} Undo takes the pass back",
    ));
    controls.append(&decorations);
    refresh_decorations_gate(&window);
    wire_decorations_pass(&decorations, &window);
    // §2's bar reads ▶ , I'm feeling lucky , ⏹ , then the progress bar taking the row's free width --
    // it stays in the OUTER row, beside the readouts, so shrinking squeezes the buttons and not the bar.
    // §1: the run bar's controls live in a box of their own (`run-controls-inner`), which
    // `window_divider::install` puts inside a scroller named `run-controls` so a narrow window scrolls them
    // sideways instead of holding the shell wide. The row keeps its name and its children; only their
    // parent moved.
    let run_row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    run_row.set_widget_name("run-bar");
    run_row.append(&super::window_run_scroller::wrap_controls(controls.upcast::<gtk::Widget>()).upcast::<gtk::Widget>());
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
        &procs,
        project,
    );
    wire_stop(
        &stop_,
        &play,
        &bar,
        &shell,
        &status,
        &progress,
        &log,
        &log_view,
        &queue,
        &procs,
    );
    wire_lucky(
        &lucky,
        &bar,
        &shell,
        &stack,
        &guard,
        &status,
        &progress,
        &log,
        &log_view,
        &queue,
        &exchange_log,
        &procs,
        project.clone(),
    );

    let box_ = gtk::Box::new(gtk::Orientation::Vertical, 0);
    // §1's header bar: **1** New · **2** Open · **3** Save · **8** Rescan. New is F0.8, Open F0.9,
    // Save F0.10, Rescan F0.11 and Settings F0.13; ⓘ (F0.1's S4) stays out for its own round.
    // The window holds one live project — `session` below — which is what makes Open possible: the
    // chooser hands over a folder and that single copy is replaced with what was read from it, so
    // every flow already sharing the handle reads the opened project rather than the one the launch
    // started with.
    let header = adw::HeaderBar::new();
    let new_ = gtk::Button::from_icon_name("document-new-symbolic");
    new_.set_widget_name("new-button");
    new_.set_tooltip_text(Some(NEW_TIP));
    header.pack_start(&new_);
    wire_new(&new_, &bar, &status, &session);

    // **2** Open, between New and Save as §1 reads the bar.
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

    // §1's badge **4**, the project name/path, in the header's centre slot between Save and the tab row.
    // Installed, published and drawn by one call (`window_doors`); every switch repaints it beside the
    // readouts.
    super::window_doors::install_project_title(&header);

    // **8** sits at the RIGHT end of the bar, next to where Settings goes — `pack_end`, not
    // `pack_start`, so it does not join New and Save.
    let rescan_ = gtk::Button::from_icon_name("view-refresh-symbolic");
    rescan_.set_widget_name("rescan-button");
    rescan_.set_tooltip_text(Some(RESCAN_TIP));
    header.pack_end(&rescan_);
    wire_rescan(&rescan_, &status, &session, &shell);

    // F0.13's Settings button, packed after Rescan so Rescan stays rightmost as §1 reads the bar.
    // The dialog itself is built on the press, in `settings.rs`; nothing here decides anything.
    let settings_ = gtk::Button::from_icon_name("emblem-system-symbolic");
    settings_.set_widget_name("settings-button");
    settings_.set_tooltip_text(Some(SETTINGS_TIP));
    header.pack_end(&settings_);
    wire_settings(&settings_, &window);

    // §1's badge **6**: ⓘ, once, at header position 6 — read left to right the bar ends ⓘ ⚙ ⟳, so
    // it is packed after Settings (each `pack_end` lands left of the previous one). It opens the
    // same policy form as the run bar's gear through [`press_policy`]; its tooltip follows
    // `Shell::info_tip`, which `paint_readouts` syncs on every switch and a bounce leaves on the page
    // still shown (§1: "tooltip = current tab's label + help text", F0.1 S4's "sync ⓘ").
    // The four per-page ⓘ buttons stay as shortcuts: `cut_policy_form_widgets` presses
    // `tab-info-button-Cut`, and §1's badge **6** is the header's, pinned by
    // `sec_03_shell_1_the_window` rather than enforced by deleting the copies.
    let tab_info = gtk::Button::from_icon_name("help-about-symbolic");
    tab_info.set_widget_name("tab-info-button");
    tab_info.set_tooltip_text(Some(&shell.borrow().info_tip()));
    INFO_BUTTON.with(|slots| slots.borrow_mut().push(tab_info.clone()));
    header.pack_end(&tab_info);
    wire_policy(&tab_info, &window);

    // F0.12 lives on the Prepare page, so its handler gets the widgets that page built. They are
    // handed over rather than found by name: `page_box` runs once per tab, and a window with four
    // same-named buttons makes `find` return whichever one it reaches first — an arbitrary page's
    // button, wired to nothing or wired twice.
    if let Some((add_, copy_)) = prepare_add {
        wire_add(&window, &add_, &copy_, &bar, &status, &session);
    }

    box_.append(&header);
    // §1 badge **5**: the tab row is the header bar's TITLE widget, in its centre — "not a row of its own
    // under the header". `adw::HeaderBar` centres whatever is not packed at an end, so this is where the
    // four tabs belong; the project label went to the start side with badge 4.
    switcher.set_widget_name("page-tabs");
    switcher.set_hexpand(false);
    switcher.set_halign(gtk::Align::Center);
    header.set_title_widget(Some(&switcher.clone().upcast::<gtk::Widget>()));
    // §1's draggable divider: the page stack above, the bottom bar below. Built in `window_divider`.
    super::window_divider::install(&box_, stack.upcast_ref(), run_row.upcast_ref(), readout_row.upcast_ref(), log.upcast_ref());

    window.set_content(Some(&box_));

    // F2.1: this window's preview is registered only now that the content tree exists. A `Rc` clone of
    // a GTK widget handed out earlier keeps the ref floating outside the tree, and GTK then hands the
    // button's own strong reference back to the caller — which leaves the click handler never reached,
    // because nothing inside the realized window is the object the test fires. Registering after
    // `set_content` puts the wired button in the tree and makes the newest slot the live one.
    if let Some(play_) = play_recording_button(&window) {
        register_preview_player_slot();
        wire_play_recording(&play_, &window);
    }
    // F2.5 S6: this window's one preview volume, pushed the way the player slot is — after
    // `set_content`, so the slider being wired is the one inside the realized tree. Guarded by the
    // same lookup as the slider itself rather than a page check: only the Cut page builds one, so a
    // missing widget *is* "not this page", and every window still gets exactly one volume slot.
    if preview_volume_scale(&window).is_some() {
        PREVIEW_VOLUMES.with(|slots| {
            slots
                .borrow_mut()
                .push(Rc::new(std::cell::RefCell::new(cut_hear::PreviewVolume::default())))
        });
        // §05-cut#8 bullet 2: the stream volume goes to full ONCE, here, before any tick has asked for a
        // gain — through this app's own element, never the sound server's remembered stream volume.
        crate::ui::preview_sound::reset_stream_volume_once(&window);
        if let Some(scale) = preview_volume_scale(&window) {
            wire_preview_volume(&scale, &window);
        }
    }
    // F2.6: the drag surface and the cross are wired here for the same reason ▶ is — a handler on a
    // widget outside the realized tree never fires. The gesture's own press-to-drag slop is GTK's;
    // `cut_select::is_drag` is what the page would consult for its own slop rule, and the seam below
    // takes the drag's seconds directly so both paths meet in `draw_selection`.
    if select_surface(&window).is_some() {
        SELECTED.with(|slots| {
            slots
                .borrow_mut()
                .push(Rc::new(std::cell::RefCell::new(None::<cut_select::Selection>)))
        });
        SELECT_SURFACES.with(|slots| {
            slots
                .borrow_mut()
                .push(Rc::new(std::cell::RefCell::new(None::<cut_select::Surface>)))
        });
        wire_select_surface(&window);
        // F2.4 S1: the click that places the red line, on the same surface. The drag above selects;
        // this places and takes into hand. Both are needed — a page where only the drag fires has no way
        // for a press to move the line at all.
        wire_line_click_surface(&window);
        if let Some(cross) = clear_selection_button(&window) {
            wire_clear_selection(&cross, &window);
        }
        // The verb buttons exist on this page but were built insensitive. Nothing has been drawn yet, so
        // read the state once here to put them in the state the rules actually give a window with no
        // band — otherwise they would sit greyed until the first drag, and a greyed button that could
        // have been live is exactly the button that lies about the state.
        refresh_verb_buttons(&window);
    }
    // F2.2: ▶✂ is wired to the same player slot ▶ moves, because they are two views of one preview —
    // pressing one switches what the other would show, never a second transport.
    if let Some(cut_) = play_cut_button(&window) {
        wire_play_cut(&cut_, &window);
    }
    // F2.2 S5/S6: the tick loop that skips removed stretches, arms the spare pipeline ahead of each
    // jump and prints the cut's own clock. No-ops until ▶✂ makes the preview the plain cut.
    cut_play_leg::start_cut_preview_tick(&window, Rc::new(CutPreviewSlot(window.clone())));
    // F2.1 S3/S4: ▶'s own tick on the same cadence — no-ops until the recording preview is playing.
    crate::ui::cut_record_tick::start(Rc::new(window.clone()));
    // F2.3 S3/S4/S5: ▶✂✂'s tick, which feeds the line back into `cut_review::tick` so a review actually
    // advances from join to join. No-ops until a review is running.
    crate::ui::cut_review_play::start(Rc::new(window.clone()));
    // F2.3: this window's cut slot is registered here for the same reason the player slot is — after
    // `set_content`, so the button being wired is the one inside the realized tree. Seeded from the
    // project's own `cut/cut.json` when it has one, because that file IS the cut (§3): the strip paints
    // from this slot, so a project with cuts must show them and a project without one must show the
    // empty hint rather than a blank box. A missing or unreadable file is no cut today, which is what
    // `cut::load` already answers with `Cut::default`.
    // F2.4 S4: this window's line slot, registered with the others so the newest window is the live
    // one. The project root DOES reach the page -- `project_tree` above is what the cut was loaded
    // from -- so the saved position is restored here rather than starting at zero: `cut_line::restore`
    // keeps it only while some filmed span still covers it (the spans come from this same opened cut
    // through `timeline::kept_footage_recordings` + `filmed_runs`, so nothing new is invented).
    // One-shot per project open: it happens at build, never on re-entering the tab.
    let project_tree = layout::Tree::new(startup::session_dir(
        &std::env::current_dir().unwrap_or_default(),
    ))
    .ok();
    let opened_cut = project_tree
        .as_ref()
        .and_then(|tree| cut::load(tree).ok())
        .unwrap_or_default();
    REVIEW_CUTS.with(|slots| slots.borrow_mut().push(Rc::new(std::cell::RefCell::new(opened_cut.clone()))));
    // F2.13: this window's edit history, opened on the SAME cut that was just published, so the base
    // Revert returns to is what this page opened with (§2) rather than whatever a previous window left
    // behind. Done through `reopen_history_on` rather than a second push because the history may already
    // have been touched (a refresh ran during the build) -- replacing beats stacking another baseline.
    reopen_history_on(&window, &opened_cut);
    // F2.11/F2.10: this window's SESSION recordings, pushed beside the cut. The fold half reads these
    // rather than `kept_footage_recordings(&opened_cut)`: a dropped stretch only exists inside a run LONGER
    // than the union of the kept clips inside it. F2.10 asks that list of the OPEN PROJECT (stamped names,
    // hand shifts, probed lengths — [`crate::ui::cut_session`]), so a second camera that filmed the same
    // seconds gets its own row; what the cut implies is the fallback when nothing can be resolved.
    push_session_recordings(crate::ui::cut_session::session_or_kept(&opened_cut));
    // F2.4 S4 asks that a remembered line survives while some filmed span still covers it. The spans come from
    // the SESSION list this window was just given (`session_or_kept`: the open project's takes where ffprobe
    // answered, else what the opened cut implies) — a take keeps rolling past the last kept clip, and a line
    // out there is still on footage. Where nothing could be resolved that list IS the opened cut's own spans,
    // so the line and its tape cannot disagree about which folder they belong to.
    let runs = crate::timeline::filmed_runs(page_recordings().as_slice());
    if let Some(review_) = review_cuts_button(&window) {
        wire_review_cuts(&review_, &window);
    }
    // F2.3 S1 (and F2.2 S1): the cut slot above is filled only AFTER the Cut page's first refresh ran, so
    // both skipping buttons have their grey asked once more here — otherwise a project opened with one clip
    // would show them live until the first drag, which is a button lying about the state. The pairing lives
    // in the page half, so this file grows by nothing of its own beyond the call.
    crate::ui::cut_cut_play::paint_both_skipping_buttons(&window);
    // F3.9's ✐ Captions is wired where it is BUILT -- in the run-bar row above, next to the other jobs -- and
    // deliberately NOT appended into `cut-toolbar`: §05 #1-screen asserts that bar holds exactly the six
    // groups `cut_screen::TOOLBAR_GROUPS` declares, in that order, and F3.9 adds no §1 item to any of them.
    // Only the gate is refreshed here, because the policy may have moved since the page was built.
    refresh_captions_gate(&window);
    refresh_speeds_gate(&window);
    // F2.4 S4: this window's line slot, registered with the others so the newest window is the live
    // one. The project root DOES reach the page — `session_root` above is what the cut was loaded from —
    // so the saved position is restored here rather than starting at zero: `cut_line::restore` keeps it
    // only while some filmed span still covers it (the spans come from this same opened cut through
    // `timeline::kept_footage_recordings` + `filmed_runs`, so nothing new is invented). One-shot per
    // project open: it happens at build, never on re-entering the tab.
    let project_root = project_tree.as_ref().map(|tree| tree.dir().to_path_buf());
    if let Some(root) = project_root.clone() {
        LINE_ROOTS.with(|slots| slots.borrow_mut().push(root));
    }
    let restored = project_root
        .as_deref()
        .and_then(|root| cut_line::restore(root, &runs));
    if let Some(pos) = restored {
        log_line(&format!("line restored to {}", crate::tools::mm_ss(pos.t)));
    }
    LINE_STATES.with(|slots| {
        slots.borrow_mut().push(Rc::new(std::cell::RefCell::new((
            restored.unwrap_or(cut_line::LinePos { t: 0.0 }),
            cut_line::LineWriter::default(),
        ))))
    });
    // S4's other half: a restored line is CUED to that frame and scrolled into view, not merely logged.
    if let Some(pos) = restored {
        crate::ui::cut_picture::note_line_moved(&window, pos.t);
        crate::ui::cut_view::follow_playhead(&window, pos.t);
    }
    // F2.4: the four line-step buttons get their handlers now that they sit in the realized tree.
    for (name, _label, frames) in LINE_STEP_BUTTONS {
        if let Some(step) = line_step_button(&window, name) {
            wire_line_step(&step, &window, frames);
        }
    }
    wire_line_keys(&window);
    // F2.4 S4: closing this window saves the line where it stands, whatever the rate limit says.
    // `Propagation::Proceed` so the close still happens — same shape as policy_form.rs's own hook.
    {
        let closing = window.clone();
        window.connect_close_request(move |_| {
            // One beat of the main loop FIRST, so a box that was just typed in has been painted and its
            // `changed` handler has already run: `set_text` during a repaint fires `changed`, and
            // `note_typing` ignores text equal to the entry's own, so a beat missed here means the close
            // has nothing to flush. `iteration(false)` is a non-blocking main-loop step -- the same
            // primitive `tests/common::settle` runs -- so this is a wait for a condition already queued,
            // not a fixed sleep.
            let pending = glib::MainContext::default();
            while pending.iteration(false) {}
            flush_line_on_close(&closing);
            // §F4.7 (**Text**): closing the window flushes a half-typed line too, not only a tab switch.
            crate::ui::narrate_text::flush_owed(&closing);
            // The same flush against the WINDOW'S OWN held record. `flush_owed` reads the page's state,
            // which is process-global and belongs to whichever Narrate page was refreshed last; this
            // window's copy is the one its switch would write on a tab leave (S3), so closing without it
            // loses exactly the lines this window was shown. Whichever copy holds the typed text wins --
            // a switch always carries both together (`carry_into_held_record`).
            flush_held_narration_on_close(&closing);
            // §2 S6: and the project itself, which is what an interrupted session would lose — resolved
            // through this window's own folder rather than the process cwd, which a moved session or a
            // test that pinned its cwd can point somewhere else entirely.
            flush_session_on_close();
            // §6/§5: and any settings dialog still open — it is a separate top level, so its own close
            // hook never runs when the app comes down through this window.
            crate::ui::settings::flush_owed();
            glib::Propagation::Proceed
        });
    }
    // F2.7: the three verb buttons and ⌦ / Delete / BackSpace, wired after `set_content` like every
    // other control on this page.
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
    // F2.12: ⧉ Insert, wired the same way — after `set_content`, so the button being wired is the one
    // inside the realized tree. The button forwards and prints; every rule is in `cut_insert`.
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
    // F2.9: ⧉ Copy / ⧉ Paste / ⇲ Lane, wired the same way — after `set_content`, so the buttons being
    // wired are the ones inside the realized tree.
    for (name, seam) in [
        ("copy-button", press_copy as fn(&adw::ApplicationWindow) -> String),
        ("paste-button", press_paste),
        ("lane-button", press_lane),
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
    // S2: Esc drops what is in hand, and only that key.
    wire_copy_esc(&window);
    // F3.8 S3: Esc drops a HELD EFFECT **and disarms** whatever hand-armed drag is up, so it must be wired
    // FIRST among the Esc controllers: GTK runs a window's `key-pressed` controllers in add order and the first
    // that stops wins, so a per-kind arm controller wired before this one would claim the key and the hold would
    // never drop. With nothing held [`press_band_esc`] answers `None` and proceeds to the kind's own controller,
    // which keeps its job unchanged. The order here is load-bearing, not cosmetic.
    wire_band_esc(&window);
    // F3.1: the zoom's own drag on the preview panel, plus Esc releasing the arm. Wired after
    // `set_content` like every other control, so the panel being wired is the one inside the realized tree.
    wire_zoom_drag(&window);
    wire_zoom_esc(&window);
    // F3.3: Esc drops an open speed form too. Wired here (after `set_content`) rather than only inside
    // `show_speed_form`, so the key is claimed by the window that drew the form even before a test looks it up.
    wire_speed_esc(&window);
    // F3.6: Esc drops an open volume form, wired beside the speed form's for the same reason — the controller
    // must sit on the realized window, and it claims the key only when this window owns the form.
    wire_volume_esc(&window);
    // F3.7: Esc drops an open label form, wired beside the volume form's for the same reason — the controller
    // must sit on the realized window, and it claims the key only when this window owns the form.
    wire_label_esc(&window);
    // F3.4: Esc releases the caption arm / drops an open caption form. Wired here (after `set_content`) like
    // every other control on this page.
    wire_text_esc(&window);
    // F3.5: Esc releases the drawing arm / drops an open drawing form. Wired here (after `set_content`) like
    // every other control on this page.
    wire_svg_esc(&window);
    // F3.2: the Aspect ratio dropdown, wired by name after `set_content` for the same reason — the widget
    // the lookup finds must be the one in the realized tree.
    wire_aspect_choice(&window);
    // §05-cut#1-screen: the history group (Undo / Redo / Revert / Clear), the zoom pair and their
    // chords, wired after `set_content` like every other control on this page.
    wire_history_and_zoom(&window);
    wire_history_keys(&window);
    // F2.8: the trim and move gestures on `track-strip`, wired after `set_content` for the same reason.
    wire_track_strip(&window);
    // F2.10: fill `camera-rows` now that the page is inside the tree, so the search for it succeeds
    // and the plates, badges and switches a test fires by name are actually there.
    refresh_camera_rows(&window);
    // §06-effects#2: the lane's one row and the preview's frame are filled here for the same reason —
    // `page_box` builds both holders empty because the window is not in the tree yet at that point.
    refresh_effects_lane(&window);
    // F2.11: the gutter's fold-all badge, then the per-gap − / + badges and each row's ✕. Drawn here,
    // after `set_content`, because a button outside the realized tree is found by no name and fired by
    // no click.
    wire_fold_all(&window);
    refresh_fold_badges(&window);
    // §1 badge **31**: the bar takes its numbers now the page is in the tree and the session's length
    // is known; §1's three wheel gestures need their widgets realized to receive anything.
    repaint_scrollbar(&window);
    wire_wheel_gestures(&window);
    crate::ui::cut_view::attach_scrollbar(&window);
    // F2.4 S1/S1b: the painted band takes the line press too.
    crate::ui::window::wire_band_line_press(&window);
    // F2.6 S1/S2: and the selection drag, on the same band, reading the same painted geometry.
    crate::ui::cut_select_press::attach(&window);
    cut_form_frame::wire_close(&window);
    cut_picture::wire_click(&window);
    crate::ui::cut_preview_frame::install_draw(&window);
    // §2: land a suggestion — its cut AND the new base — through the page's own doors.
    crate::ui::cut_suggest_land::install(newest_review_cut, |window, landed| {
        publish_cut(landed);
        // note_change before suggested: Revert then keeps the model's work and drops only hand edits after it.
        cut_history(window).borrow_mut().note_change(landed);
        cut_history(window).borrow_mut().suggested(landed);
        refresh_history_buttons(window);
    });
    // §F2.14 S2b: the Suggest call's progress, painted (§S2b's "streaming progress counts segments"). One
    // reply's reading goes back through the page's one progress door, so the bar and the status row are drawn
    // exactly as every other beat draws them, with no second painting path.
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
    // §07-narrate#1-screen: the Narrate page's controls, wired after `set_content` like every other
    // control in this window, so the widgets being wired are inside the realized tree.
    crate::ui::narrate_page::wire(&window);
    crate::ui::produce_page::wire(&window);
    window
}

/// F2.11 S1 (`the gutter badge folds/unfolds all`): create `fold-all-button` in the gutter holder and
//  wire it to [`press_fold_all`]. §B's seat is the band's own 30 px gutter, so it goes into
/// `gutter-badges`, an overlay child of `track-layers` that sits over the inked column; the older
/// beside-the-rows holder is only the fallback for a page built without a band.
fn wire_fold_all(window: &adw::ApplicationWindow) {
    // Look the button up IN THE HOLDER rather than by a tree-wide name search. A window's `close()` does
    // not destroy its widgets while another window's tree is still around, so searching by name can hand
    // back a `fold-all-button` parented to a DIFFERENT window's box: this one would then look already
    // built and never be appended, leaving the second window with no gutter badge at all.
    let Some(holder) = crate::ui::cut_tracks::gutter_badges_box(window) else {
        return;
    };
    if holder_children_named(&holder, "fold-all-button") {
        return;
    }
    let button = gtk::Button::new();
    button.set_widget_name("fold-all-button");
    button.set_label("\u{2212}\u{25a4}");
    button.set_tooltip_text(Some("fold every stretch the cut drops \u{00b7} press again to open them all"));
    let win = window.clone();
    button.connect_clicked(move |_| {
        press_fold_all(&win);
    });
    holder.append(&button);
}

/// Does this holder already own a child with exactly this widget name? Scoped to the holder on purpose —
/// see [`wire_fold_all`] for what a tree-wide search costs.
fn holder_children_named(holder: &gtk::Box, name: &str) -> bool {
    holder
        .observe_children()
        .iter::<glib::Object>()
        .flatten()
        .filter_map(|child| child.downcast::<gtk::Widget>().ok())
        .any(|child| child.widget_name() == name)
}

/// The shell as the window starts: on the page named, or on Prepare for `""` and for a name that is
/// not one of the four (a page nobody asked for is not a reason to fail to open).
fn start_shell(page: &str) -> Shell {
    match Page::all().into_iter().find(|candidate| candidate.label() == page) {
        Some(page) => Shell { page, help_page: page, ..Shell::default() },
        None => Shell::default(),
    }
}

/// Where a tab click is decided: ask the shell, redraw from what it says, and put the strip back if
/// it refused. Split out of [`build_window`] so the handler has one job and can be driven directly.
#[allow(clippy::too_many_arguments)] // one handler needs every tab's own widgets: the switcher, its
// guard, and the readouts a tab change repaints — none of them belong together in a struct
fn wire_switching(
    stack: &adw::ViewStack,
    switcher: &adw::ViewSwitcher,
    status: &gtk::Label,
    shell: &Rc<RefCell<Shell>>,
    guard: &Rc<std::cell::Cell<bool>>,
    project: &Project,
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
    let project = project.clone();
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
        // S1-S5, all of it in shell.rs. The folder is read here rather than remembered: which
        // project is open is the window's business, and `session_dir` is the same stand-in Rescan
        // uses. A folder that is not a project leaves `tree` None — §1 wants the page to show what
        // it has rather than fail to open, so there is no error path out of the switch.
        let (tree, cut, _) = session_reads();
        // The lines themselves are the window's held copy, not a fresh read: S5 refits them on
        // arrival and S3 writes that refit on leaving, and the two are separate switches.
        let held = Rc::clone(&held_narration);
        // S3 asks whether a narration write is owed BEFORE the switch clears the flag: leaving the
        // tab writes what is half-typed even a beat early.
        let owed = shell.borrow().narration_pending.owe();

        let outcome = shell.borrow_mut().switch(
            target,
            Move::Click,
            &project,
            tree.as_ref(),
            &cut.segs,
            &mut held.borrow_mut().entries,
        );

        // S3: the half-typed lines reach disk before the page is left. Only a flush that was owed
        // writes, so a switch over an untouched narration costs no file.
        if owed {
            if let Some(tree) = tree.as_ref() {
                let _ = narration::save(&held.borrow(), tree);
            }
        }
        // §F4.7 (Refit): entering Narrate refitted the held lines against the cut and said so — work the
        // app did, so it reaches disk whether or not anybody typed. The `owed` flush above only covers
        // typing; without this the refit lands in a copy dropped at the end of the handler and the next
        // visit re-reads the pre-refit file and says "the cut moved" forever.
        if let Outcome::Shown {
            refresh: Refresh::NarrateRefit { moved, orphan },
            ..
        } = outcome
        {
            if moved + orphan > 0 {
                if let Some(tree) = tree.as_ref() {
                    let _ = narration::save(&held.borrow(), tree);
                    // The rows are drawn from the page's own state, which is a separate copy: publish the
                    // refitted lines there too, or a later text flush writes the un-refit ones back.
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
            // S2: the page did not move.
            guard.set(true);
            stack.set_visible_child_name(shell.borrow().page.label());
            guard.set(false);
        }
    });
}

/// The folder this window is working on, with its cut and narration read from it.
///
/// Read fresh every time rather than remembered: which project is open is the window's business, and
/// `session_dir` is the same stand-in Rescan, New and Save use, so the flows agree on what "this
/// project" means. A folder that is not a project leaves `tree` None — §1 wants the page to show
/// what it has rather than fail to open, so nothing here errors out. Both the tab switch (S3-S5) and
/// the first paint call this, so the row cannot disagree with itself between them.
pub(crate) fn session_reads() -> (Option<layout::Tree>, Cut, Narration) {
    let root = std::env::current_dir().unwrap_or_default();
    let dir = startup::session_dir(&root);
    let tree = if dir.join(crate::project::PROJECT_FILE).is_file() {
        layout::Tree::new(&dir).ok()
    } else {
        None
    };
    let cut = tree.as_ref().and_then(|tree| cut::load(tree).ok()).unwrap_or_default();
    let narration = tree
        .as_ref()
        .and_then(|tree| narration::load(tree).ok())
        .unwrap_or_default();
    (tree, cut, narration)
}








pub use crate::ui::window_policy::{
    built_window, close_policy_forms, derive_policy, forget_policy_form, main_window,
    open_policy_form, policy_form_open, policy_is_current, press_policy, refresh_policy_form,
    reset_policy_to_defaults, session_policy, set_policy_sender_for_tests, set_session_context,
    set_session_policy_field,
};
pub(crate) use crate::ui::window_policy::{
    schedule_policy_derive, user_context, wire_policy, wire_settings, POLICY_TRACKERS,
    POLICY_WATCH_WINDOW,
};


thread_local! {
    /// The run bar `build_window` created, published for the same reason as [`SESSION`]: so the
    /// seam in [`open_from`] can reach the copy this window actually uses.
    pub(crate) static WINDOW_BAR: std::cell::RefCell<Vec<Rc<std::cell::RefCell<run::RunBar>>>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

thread_local! {
    /// The F0.5 queue `build_window` created, published for the same reason as [`WINDOW_BAR`]: so
    /// the seam in [`add_files`] can drive the bar this window actually shows. A copy is a run of
    /// its own (F0.12 S0), so it reports through the bookkeeping rather than painting its own bar.
    pub(crate) static WINDOW_QUEUE: std::cell::RefCell<Vec<Rc<std::cell::RefCell<runqueue::Queue>>>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

thread_local! {
    /// §1's badge **6** — the header ⓘ, newest last, the same newest-slot arrangement as
    /// [`POLICY_FORMS`](crate::ui::window_policy) in the policy module. Held so a switch can repaint its tooltip from `Shell::help_page` without
    /// searching the widget tree, and so a test reads the tooltip of the button that is on screen.
    pub(crate) static INFO_BUTTON: std::cell::RefCell<Vec<gtk::Button>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

thread_local! {
    /// The live `Shell` of each window, newest last — the newest-slot rule every other window-level
    /// piece here uses. Held so the S3 pending-write seam can mark a write owed on the shell that is
    /// actually showing, instead of a copy the caller happens to have.
    pub(crate) static WINDOW_SHELLS: std::cell::RefCell<Vec<Rc<RefCell<Shell>>>> =
        const { std::cell::RefCell::new(Vec::new()) };
    /// The newest window's re-entrancy latch, the one `wire_switching` guards its bounce with. A flow
    /// that moves the stack itself (a rescan sending a vanished page back to Prepare) sets it while it
    /// writes, so the notify handler does not read the shell mid-borrow.
    pub(crate) static WINDOW_SWITCH_LATCH: std::cell::RefCell<Option<Rc<std::cell::Cell<bool>>>> =
        const { std::cell::RefCell::new(None) };
}

thread_local! {
    /// The live project handle `build_window` created, published so a test can read what Open put in
    /// place. One window per test binary here, so one slot is enough; the page widgets keep their own
    /// clone of the same `Rc`, which is why replacing the contents is visible everywhere.
    pub(crate) static SESSION: std::cell::RefCell<Vec<Rc<std::cell::RefCell<Project>>>> =
        const { std::cell::RefCell::new(Vec::new()) };
}



thread_local! {
    /// One window's cut, held beside [`PREVIEW_PLAYERS`] with the same newest-slot rule.
    pub(crate) static REVIEW_CUTS: std::cell::RefCell<Vec<Rc<std::cell::RefCell<cut::Cut>>>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

type LineStates =
    std::cell::RefCell<Vec<Rc<std::cell::RefCell<(cut_line::LinePos, cut_line::LineWriter)>>>>;

thread_local! {
    /// One window's red line and the writer that throttles its saves, newest slot winning like
    /// [`PREVIEW_PLAYERS`].
    pub(crate) static LINE_STATES: LineStates =
        const { std::cell::RefCell::new(Vec::new()) };
}

// The §F2.4 red line + frame stepping group lives in `window_line_step` (window.rs is past its size
// budget); these are the doors this file and its callers still reach them through.
pub use crate::ui::window_line_step::{
    arrow_steps, flush_line_on_close, line_key_controller, line_position, line_root, line_step_button,
    move_line_and_save, place_line_press, set_line_position, space_toggles, step_frames_of,
    step_the_line, Step, NO_RECORDING_STATUS,
};
pub(crate) use crate::ui::window_line_step::{
    flush_session_on_close, LINE_ROOTS, LINE_STEP_BUTTONS, SESSION_DIRS,
    wire_line_keys, wire_line_step,
};

// The drag surface and the line-placing click (their gestures, storage and the ✕ that clears the band)
// live in `window_surfaces` (window.rs is past its size budget); these are the same names, re-exported
// under the paths every other module and test already reaches them through.
pub use crate::ui::window_surfaces::{
    band_line_gesture_for_test, line_click_gesture, line_now_ms, SELECT_SURFACE_PPS,
    wire_band_line_press,
};
pub(crate) use crate::ui::window_surfaces::{
    store_band_line_gesture, wire_clear_selection, wire_line_click_surface, wire_select_surface,
};

/// F2.7: wire one verb button to its seam. The callback holds no rule — it presses, prints the answer
/// and leaves the state where [`cut_verbs`] said to leave it.
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

/// The doors moved to [`crate::ui::cut_verb_press`] (this file is past its size budget); naming them here
/// is what the button wires below use, so no copy of a verb door lives in two files.
use crate::ui::cut_verb_press::{press_add, press_remove, press_split};

// Closing writes this window's OWN held narration, which lives in `window_close` (window.rs is past its
// size budget) beside the other close-time flushes.
fn flush_held_narration_on_close(window: &adw::ApplicationWindow) {
    crate::ui::window_close::flush_held_narration(window)
}

// F3.12's clamp seam lives in `window_review` (window.rs is past its size budget). This is the same
// name at its old path: `ui::clamp_effects_to_cut` for `cut_verb_press::report_verb` and the tests, and
// the re-export in `ui/mod.rs` still resolves through here.
pub use crate::ui::window_review::clamp_effects_to_cut;
/// Write this window's cut's segment list, keeping everything else about the cut (lanes, shifts, rows,
/// folds) exactly as it was. It lives beside the clamp seam in `window_review`; this keeps the path
/// `cut_verb_press` calls.
pub(crate) use crate::ui::window_review::set_review_segs;

// What the hand holds and which picture row this window watches (the effect / clip / edge slots, the
// pick that fills them, the watched-row read and write) live in `window_holds` (window.rs is past its
// size budget); these are the same names, re-exported under their old paths.
pub use crate::ui::window_holds::{
    apply_pick, held_clip, held_edge, held_effect, held_now, held_segment, set_held_clip,
    set_held_edge, set_held_effect, set_watched_row, watched_row,
};

thread_local! {
    /// F2.12 S1: whether this session has been told where its red line stands. `None` means no line was
    /// ever placed, which is the state that makes "click the timeline where the insert goes first" true.
    pub(crate) static INSERT_PLACE_KNOWN: std::cell::RefCell<Option<bool>> =
        const { std::cell::RefCell::new(None) };
}

// --- F2.8: trim and move on the track strip -------------------------------------------------------------

pub use crate::ui::cut_view::{band_clip_boxes_for_test, strip_clip_boxes_for_test, strip_pps};






















// --- F2.9: copy, paste and lane --------------------------------------------------------------------------






// The F2.12 insert group lives in `window_insert` (window.rs is past its size budget); these are
// the doors the rest of the page calls, kept at their paths.
pub use crate::ui::window_insert::{
    close_insert_form, insert_chosen, insert_chosen_with_length, insert_open, press_insert,
    press_insert_apply, save_seed_for_test, visible_insert_form,
};
pub(crate) use crate::ui::window_insert::{
    press_insert_from_button, refresh_insert_button, save_insert_cut,
};


/// F2.8 S1: the strip's height — the ruler row plus the kept-bar row, one number the size request and
/// the painter both read so they cannot drift apart.
pub const STRIP_HEIGHT: i32 = crate::ui::cut_trim_press::STRIP_H as i32;

/// F2.8 S1: a ruler mark every 30 s at the open zoom is a mark every 120 px, which is what
/// `spec/img/05-trim.png` prints (`0:00`, `0:30`, `1:00`). Zoomed far in the spacing would crowd, but
/// the strip's zoom is fixed until F2.10/F2.11 own the real rows, so one constant covers it.
pub const RULER_EVERY_SECONDS: f64 = 30.0;

/// F2.8 S1: how wide the placeholder strip is drawn. The spec's picture runs the whole session across
/// the window; this strip is a placeholder for the rows F2.10/F2.11 own, and its width has to stay a
/// placeholder too — but wide enough that the ruler's first marks AND the clips they measure are both on
/// screen. At [`TRACK_STRIP_PPS`] 480 px covers two minutes of the session, which is where the three
/// seeded clips live; a narrower strip cut the third tick off at 240 px and left a label-less bar.
pub const STRIP_WIDTH: i32 = 480;





// F3.10 S1–S5, the pass itself, lives in [`crate::ui::cut_speeds_pass`]: window.rs is past its size budget,
// and the pass needs none of this file's state — only the doors it already exports.





/// F3.11's label. As with ✐ Captions and ⏩ Speeds, the spec's flowchart starts at a model call and names no
/// control: a named button is the decision taken in its spirit, so the pass can be started and its gate seen.
pub const DECORATIONS_PASS_LABEL: &str = "\u{2728} Decorations";

// F3.11 S1–S5, the decorations pass itself, lives in [`crate::ui::cut_effects_pass`]: window.rs is
// past its size budget, and the pass needs none of this file's state — only the doors it exports.

// The F4.1 ▶ Write and speak group lives in `window_narrate` (window.rs is past its size budget);
// these are the doors this file still calls, kept at their paths. Everything else reaches the group
// through `crate::ui::window_narrate` directly.
pub use crate::ui::window_narrate::{
    narrate_pulsed, narrate_script_loaded, narrate_session_tree, narrate_stage, narrate_stages,
    narrate_wrote,
    press_narrate_run,
    pull_half_typed, refuse_narrate_run, refuse_narrate_run_with_busy, set_narrate_script,
    set_narrate_script_slow,
    set_status_line, show_narrate_progress, show_narrate_stage,
};
pub(crate) use crate::ui::window_narrate::live_project;



/// F2.3: the Cut page's ▶✂✂ forwards to [`press_review_cuts`], reading the cut this window holds.
fn wire_review_cuts(button: &gtk::Button, window: &adw::ApplicationWindow) {
    let window = window.clone();
    button.connect_clicked(move |_| {
        let _ = press_review_cuts(&window, &newest_review_cut());
    });
}
// The Save door (where a press goes, and the dialog that names the project) lives in `window_save`
// (window.rs is past its size budget); this is the same name, re-exported under its old path.
pub(crate) use crate::ui::window_save::wire_save;



// The window's log (the launch lines held until a window exists, the sink every flow writes into, and
// the readers a test asserts against) live in `window_logs` (window.rs is past its size budget); these
// are the same names, re-exported under the paths every other module and test already reaches them through.
pub use crate::ui::window_logs::{
    clear_logs_for_test, logs_for_test, note_startup, note_unload, window_logs,
};
pub(crate) use crate::ui::window_logs::log_line;




























thread_local! {
    /// The run bar `build_window` created, published so a test can read the step it holds. Same
    /// reason as [`SESSION`]: the bar lives inside the window's closure and nothing else can reach it.
    pub(crate) static RUN_BARS: std::cell::RefCell<Vec<Rc<std::cell::RefCell<crate::run::RunBar>>>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

thread_local! {
    /// The live session the newest window works on, handed to the ▶ handler so F1.1's refusal asks
    /// about what the page holds now rather than what `build_window` was called with.
    pub(crate) static PLAY_SESSION: std::cell::RefCell<Vec<Rc<std::cell::RefCell<Project>>>> =
        const { std::cell::RefCell::new(Vec::new()) };
}















// The §05-cut#1-screen history + zoom group lives in `window_history_zoom` (window.rs is past its
// size budget); these are the doors this file and its callers still reach them through.
pub use crate::ui::window_history_zoom::{
    history_key_controller, note_edit, press_clear_cut, press_redo, press_revert, press_thumb,
    press_undo, press_zoom, press_zoom_apply, press_zoom_cancel, press_zoom_esc, reopen_history_on,
    zoom_gesture,
};
pub(crate) use crate::ui::window_history_zoom::{
    cut_history, publish_cut, record_edit, refresh_history_buttons, repaint_scrollbar,
    wire_aspect_choice, wire_history_and_zoom, wire_history_keys, wire_wheel_gestures,
    wire_zoom_drag, wire_zoom_esc, zoom_form_owner,
};


/// F2.2 S5: this window's side of the ▶✂ tick loop, over the SAME player and cut slots the buttons use.
struct CutPreviewSlot(adw::ApplicationWindow);

impl cut_play_leg::CutPreviewPage for CutPreviewSlot {
    fn cut_only(&self) -> bool { preview_player(&self.0).cut_only }
    fn playhead(&self) -> Option<f64> { preview_player(&self.0).playhead }
    fn set_playhead(&self, at: f64) { set_playhead(&self.0, at) }
    fn cut(&self) -> Cut { review_cut_of(&self.0) }
    fn volume(&self) -> f64 { preview_volume(&self.0) }
    // S5's six doors forward to `cut_cut_play`, the F2.2 page half: `window.rs` is past its size budget.
    fn arm_spare_at(&self, at: f64) -> bool { crate::ui::cut_cut_play::preroll_spare(&self.0, at) }
    fn cross_into_spare(&self) -> bool { crate::ui::cut_cut_play::swap_spare(&self.0) }
    fn drop_spare(&self) { crate::ui::cut_cut_play::release_spare(); }
    fn hold_still(&self, hold: bool) -> bool { crate::ui::cut_cut_play::freeze(hold) }
    fn play_rate(&self, rate: f64, at: f64) -> bool { crate::ui::cut_cut_play::hand_rate(&self.0, rate, at) }
    fn apply_gain(&self, gain: f64, at_secs: f64) { crate::ui::cut_cut_play::hand_gain(&self.0, gain, at_secs); }
    fn say_clock(&self, clock: f64, ended: bool) {
        if let Some(status) = find_status(self.0.upcast_ref()) {
            status.set_text(&cut_play_leg::clock_line(clock, ended));
        }
        // §D: the ✂ clock carries its own explanation as a tooltip: cut time, total, session second.
        let len = cut_play_leg::kept_length(&review_cut_of(&self.0));
        crate::ui::cut_cut_play::set_clock_tooltip(&self.0, clock, len, preview_playhead(&self.0));
    }
}






/// F3.9's label. The spec's flowchart starts at "after the cut" and names no control, so nothing existed to
/// wire the pass to: a named button is the decision taken in its spirit — the pass has to be startable, and its
/// gate (P.policy.captionsPass) has to be visible rather than inferred from a silent grey bar.
pub const CAPTIONS_PASS_LABEL: &str = "\u{2710} Captions";

// The §1 widget doors live in `window_doors` (window.rs is past its size budget); these are the same names,
// re-exported under the paths every other module and test already reaches them through.
pub use crate::ui::window_doors::{
    state, tab_tooltip, tab_dimmed, tab_button, inputs_readout, outputs_readout, outputs_folder_button, tab_info_parts, mark_narration_owed, tab_info_button, save_button, save_tooltip, rescan_button, find_source_widget, open_button, session_handle, session_sources, run_queue, open_tooltip, progress_bar, log_expander, progress_tooltip, rescan_tooltip, add_sources_button, copy_into_project, bench_language, session_freq, running_step, marking_pass, set_marking_pass, new_button, new_tooltip, play_button, stop_button, lucky_button, play_tooltip, find_status, project_title, paint_project_title,

};
pub(crate) use crate::ui::window_doors::{find_switcher, find_widget_by_name};
// `view_stack` and `switch_latch` are the same doors, reached by `rescan_apply` as `window::view_stack`
// and `window::switch_latch` when a rescan moves a vanished page back to Prepare.
pub use crate::ui::window_doors::{switch_latch, view_stack};

// The project dialogs (＋ New / Open / Save / Rescan / Add sources) live in `window_project`
// (window.rs is past its size budget); these are the same names, re-exported under the paths every
// other module and test already reaches them through.
pub use crate::ui::window_project::{
    add_files, add_chooser_shown_count, last_ask, last_open_ask_for_test, name_chosen,
    new_project_confirm, new_project_dialog, open_folder, open_from, reset_add_chooser_shown,
};
pub(crate) use crate::ui::window_project::{
    wire_add, wire_new, wire_open, wire_rescan, remember_opened,
    NEW_TIP, OPEN_TIP, RESCAN_TIP, SAVE_TIP, SETTINGS_TIP,
};

// The preview player and the review cut (▶ / ▶✂ / ▶✂✂ presses, their widget doors and reads) live in
// `window_preview` (window.rs is past its size budget); these are the same names, re-exported under
// the paths every other module and test already reaches them through.
pub use crate::ui::window_preview::{
    play_cut_button, play_recording_button,
    press_play_cut, press_play_recording, press_review_cuts, preview_player, preview_volume_scale,
    publish_preview_transport, register_preview_player_slot, review_cut_of, review_cut_segs,
    review_cut_segs_count, review_cut_shift, review_lanes, review_cuts_button, seed_review_cut,
    set_playhead, set_preview_state,
};
pub(crate) use crate::ui::window_preview::{live_player, newest_review_cut};
