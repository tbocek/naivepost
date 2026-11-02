//! Everything the window and its shell hold between presses, in one place. One per window thread.

pub(crate) struct WindowPage {
    /// The clock every beat is measured against: elapsed real time from an `Instant` plus whatever [`advance_clock_for_test`] added. The shell's `Pending` stores absolute deadlines on it so a second clock would make every `writable` answer "not yet".
    pub(crate) clock: std::cell::OnceCell<std::time::Instant>,
    pub(crate) shift: std::cell::Cell<std::time::Duration>,
    /// Whether an autosave timer stands. Typing re-arms the beat, not the timer, so a burst of keystrokes costs one write.
    pub(crate) autosave_standing: std::cell::Cell<bool>,
    pub(crate) pulse: std::cell::Cell<Option<glib::SourceId>>,
    pub(crate) window_bar:
        std::cell::RefCell<Vec<std::rc::Rc<std::cell::RefCell<crate::run::RunBar>>>>,
    /// A copy is a run of its own (F0.12 S0): it reports through the bookkeeping rather than painting its own bar.
    pub(crate) window_queue:
        std::cell::RefCell<Vec<std::rc::Rc<std::cell::RefCell<crate::runqueue::Queue>>>>,
    /// §1's badge **6** (the header ⓘ), newest last; a switch repaints its tooltip from `Shell::help_page`.
    pub(crate) info_button: std::cell::RefCell<Vec<gtk4::Button>>,
    /// The live `Shell` of each window, newest last; the S3 pending-write seam marks a write owed on the one showing.
    pub(crate) window_shells:
        std::cell::RefCell<Vec<std::rc::Rc<std::cell::RefCell<crate::shell::Shell>>>>,
    /// The newest window's re-entrancy latch for `wire_switching`; a flow that moves the stack itself sets it while writing.
    pub(crate) window_switch_latch: std::cell::RefCell<Option<std::rc::Rc<std::cell::Cell<bool>>>>,
    /// The live project handle `build_window` created, so a test can read what Open put in place.
    pub(crate) session:
        std::cell::RefCell<Vec<std::rc::Rc<std::cell::RefCell<crate::project::Project>>>>,
    pub(crate) review_cuts:
        std::cell::RefCell<Vec<std::rc::Rc<std::cell::RefCell<crate::cut::Cut>>>>,
    pub(crate) line_states: crate::ui::window::LineStates,
    /// F2.12 S1: `None` means no red line was ever placed ("click the timeline where the insert goes first").
    pub(crate) window_insert_place_known: std::cell::RefCell<Option<bool>>,
    pub(crate) run_bars:
        std::cell::RefCell<Vec<std::rc::Rc<std::cell::RefCell<crate::run::RunBar>>>>,
    pub(crate) play_session:
        std::cell::RefCell<Vec<std::rc::Rc<std::cell::RefCell<crate::project::Project>>>>,
    pub(crate) bench_open_row: std::cell::RefCell<Option<usize>>,
    /// How many readout repaints were asked; a landing paints only when it answers the newest.
    pub(crate) readout_asks: std::cell::Cell<u64>,
    /// Readouts read on the window thread itself: none ever should; a test reads it.
    pub(crate) read_here: std::cell::Cell<usize>,
    pub(crate) header_titles: std::cell::RefCell<Vec<gtk4::Label>>,
    pub(crate) cut_histories:
        std::cell::RefCell<Vec<std::rc::Rc<std::cell::RefCell<crate::cut::History>>>>,
    pub(crate) history_keys: std::cell::RefCell<Option<gtk4::EventControllerKey>>,
    pub(crate) zoom_gestures: std::cell::RefCell<Option<gtk4::GestureDrag>>,
    /// F2.4 S1: which picture row this window watches. Only a picture-band click sets it, while nothing plays
    /// with sources loaded (`cut_line::watches_row` owns that).
    pub(crate) watched_row: std::cell::RefCell<Option<usize>>,
    pub(crate) insert_open: std::cell::RefCell<Option<crate::ui::window_insert::InsertOpen>>,
    pub(crate) logo_asks: std::cell::Cell<usize>,
    pub(crate) insert_asks: std::cell::Cell<usize>,
    pub(crate) line_roots: crate::ui::window_line_step::WindowFolders,
    pub(crate) session_dirs: crate::ui::window_line_step::WindowFolders,
    pub(crate) session_trees: crate::ui::window_line_step::WindowTrees,
    pub(crate) line_keys: std::cell::RefCell<Option<gtk4::EventControllerKey>>,
    /// The window's log lines, standing in for F0.5's log expander: `>>> moved the output folder to …` and the `!!!` refusals must be visible to a test.
    pub(crate) logs: std::cell::RefCell<Vec<String>>,
    /// The narration run's own view of the bar: `narration 1/2` then `speaking 2/2`, each owning half (§F4.1 S3/S6).
    pub(crate) narrate_queue: std::cell::RefCell<crate::runqueue::Queue>,
    pub(crate) narrate_script: std::cell::RefCell<Vec<crate::ui::window_narrate::ScriptedLine>>,
    pub(crate) slow_narrate_script: std::cell::RefCell<crate::ui::window_narrate::SlowScript>,
    pub(crate) narrate_stage: std::cell::RefCell<String>,
    pub(crate) narrate_pulsed: std::cell::Cell<bool>,
    /// Whether the last run reached its speaking job; ⏹ reads it to say `narration stopped` or `speaking stopped`.
    pub(crate) narrate_wrote: std::cell::Cell<bool>,
    pub(crate) narrate_stages: std::cell::RefCell<Vec<String>>,
    pub(crate) policy_senders: crate::ui::window_policy::PolicySenders,
    /// Per-window record of which User Context the policy was derived from (F0.7). Newest last.
    pub(crate) policy_trackers:
        std::cell::RefCell<Vec<std::rc::Rc<std::cell::RefCell<crate::policy::Tracker>>>>,
    pub(crate) policy_debounce: std::cell::RefCell<Vec<glib::SourceId>>,
    pub(crate) policy_watch_window: std::cell::RefCell<Option<adw::ApplicationWindow>>,
    pub(crate) derive_generation: std::cell::Cell<u64>,
    /// One window's preview state, beside `SESSION`; newest slot wins.
    pub(crate) preview_players:
        std::cell::RefCell<Vec<std::rc::Rc<std::cell::RefCell<crate::preview::Player>>>>,
    /// The naming sheet the last press opened, for [`last_ask`].
    pub(crate) last_ask: std::cell::RefCell<Option<gtk4::FileDialog>>,
    pub(crate) last_open: std::cell::RefCell<Option<gtk4::FileDialog>>,
    /// How many times this thread has raised the Add sources chooser: a headless test can never answer one, so
    /// the press getting that far is only observable as a count.
    pub(crate) choosers_shown: std::cell::Cell<usize>,
    /// What this launch has opened, per root: the pairs settings writes as `PROJECT_<n>_ROOT` / `PROJECT_<n>_FILE`.
    /// Persisting belongs to `settings`' writer.
    pub(crate) remembered: std::cell::RefCell<std::collections::BTreeMap<String, String>>,
    /// The question open now, so a test can answer it as a person would.
    pub(crate) open_question: std::cell::RefCell<Option<adw::MessageDialog>>,
    /// The narration this window holds, newest last (as [`WINDOW_SHELLS`]); S5's refit and S3's flush share one set of lines.
    pub(crate) held_narration:
        std::cell::RefCell<Vec<std::rc::Rc<std::cell::RefCell<crate::narration::Narration>>>>,
    pub(crate) preview_volumes:
        std::cell::RefCell<Vec<std::rc::Rc<std::cell::RefCell<crate::cut_hear::PreviewVolume>>>>,
    pub(crate) selected: std::cell::RefCell<
        Vec<std::rc::Rc<std::cell::RefCell<Option<crate::cut_select::Selection>>>>,
    >,
    pub(crate) select_surfaces: std::cell::RefCell<
        Vec<std::rc::Rc<std::cell::RefCell<Option<crate::cut_select::Surface>>>>,
    >,
    pub(crate) select_gestures: std::cell::RefCell<Option<gtk4::GestureDrag>>,
    pub(crate) live_form: std::cell::RefCell<crate::fx_band::LiveForm>,
    pub(crate) line_clicks: std::cell::RefCell<Option<gtk4::GestureClick>>,
    pub(crate) band_line_clicks: std::cell::RefCell<Option<gtk4::GestureClick>>,
    /// The trim drag in flight; `None` means the press fell where no border was in reach.
    pub(crate) trim_drags: std::cell::RefCell<Option<crate::ui::window_trim_strip::TrimDrag>>,
    pub(crate) trim_gestures: std::cell::RefCell<Option<gtk4::GestureDrag>>,
    pub(crate) move_gestures: std::cell::RefCell<Option<gtk4::GestureDrag>>,
    /// F2.8 S2: where the move drag was PRESSED, (x, y) in strip px, newest window first. Fixed at the press
    /// so a pointer wandering off the band mid-drag cannot change what was picked up.
    pub(crate) move_pressed: crate::ui::window_trim_strip::MovePressed,
    pub(crate) press_band: std::cell::RefCell<Option<String>>,
    pub(crate) press_scene: std::cell::RefCell<Option<usize>>,
    /// The copy in hand (F2.9 S1): one app-wide, so ⧉ Paste has one answer.
    pub(crate) copy_hand: std::cell::RefCell<Option<crate::cut_copy::Hand>>,
    pub(crate) copy_esc_keys: std::cell::RefCell<Option<gtk4::EventControllerKey>>,
    pub(crate) delete_keys: std::cell::RefCell<Option<gtk4::EventControllerKey>>,
}

thread_local! {
    pub(crate) static PAGE: WindowPage = WindowPage {
        clock: std::cell::OnceCell::new(),
        shift: std::cell::Cell::new(std::time::Duration::ZERO),
        autosave_standing: std::cell::Cell::new(false),
        pulse: std::cell::Cell::new(None),
        window_bar: std::cell::RefCell::new(Vec::new()),
        window_queue: std::cell::RefCell::new(Vec::new()),
        info_button: std::cell::RefCell::new(Vec::new()),
        window_shells: std::cell::RefCell::new(Vec::new()),
        window_switch_latch: std::cell::RefCell::new(None),
        session: std::cell::RefCell::new(Vec::new()),
        review_cuts: std::cell::RefCell::new(Vec::new()),
        line_states: std::cell::RefCell::new(Vec::new()),
        window_insert_place_known: std::cell::RefCell::new(None),
        run_bars: std::cell::RefCell::new(Vec::new()),
        play_session: std::cell::RefCell::new(Vec::new()),
        bench_open_row: std::cell::RefCell::new(None),
        readout_asks: std::cell::Cell::new(0),
        read_here: std::cell::Cell::new(0),
        header_titles: std::cell::RefCell::new(Vec::new()),
        cut_histories: std::cell::RefCell::new(Vec::new()),
        history_keys: std::cell::RefCell::new(None),
        zoom_gestures: std::cell::RefCell::new(None),
        watched_row: std::cell::RefCell::new(None),
        logo_asks: std::cell::Cell::new(0),
        insert_open: std::cell::RefCell::new(None),
        insert_asks: std::cell::Cell::new(0),
        line_roots: std::cell::RefCell::new(Vec::new()),
        session_dirs: std::cell::RefCell::new(Vec::new()),
        session_trees: std::cell::RefCell::new(Vec::new()),
        line_keys: std::cell::RefCell::new(None),
        logs: std::cell::RefCell::new(Vec::new()),
        narrate_queue: std::cell::RefCell::new(crate::runqueue::Queue::new()),
        narrate_script: std::cell::RefCell::new(Vec::new()),
        slow_narrate_script: std::cell::RefCell::new(None),
        narrate_stage: std::cell::RefCell::new(String::new()),
        narrate_pulsed: std::cell::Cell::new(false),
        narrate_wrote: std::cell::Cell::new(false),
        narrate_stages: std::cell::RefCell::new(Vec::new()),
        policy_senders: std::cell::RefCell::new(Vec::new()),
        policy_trackers: std::cell::RefCell::new(Vec::new()),
        policy_debounce: std::cell::RefCell::new(Vec::new()),
        policy_watch_window: std::cell::RefCell::new(None),
        derive_generation: std::cell::Cell::new(0),
        preview_players: std::cell::RefCell::new(Vec::new()),
        last_ask: std::cell::RefCell::new(None),
        last_open: std::cell::RefCell::new(None),
        choosers_shown: std::cell::Cell::new(0),
        remembered: std::cell::RefCell::new(std::collections::BTreeMap::new()),
        open_question: std::cell::RefCell::new(None),
        held_narration: std::cell::RefCell::new(Vec::new()),
        preview_volumes: std::cell::RefCell::new(Vec::new()),
        selected: std::cell::RefCell::new(Vec::new()),
        select_surfaces: std::cell::RefCell::new(Vec::new()),
        select_gestures: std::cell::RefCell::new(None),
        live_form: std::cell::RefCell::new(crate::fx_band::LiveForm::default()),
        line_clicks: std::cell::RefCell::new(None),
        band_line_clicks: std::cell::RefCell::new(None),
        trim_drags: std::cell::RefCell::new(None),
        trim_gestures: std::cell::RefCell::new(None),
        move_gestures: std::cell::RefCell::new(None),
        move_pressed: std::cell::RefCell::new(Vec::new()),
        press_band: std::cell::RefCell::new(None),
        press_scene: std::cell::RefCell::new(None),
        copy_hand: std::cell::RefCell::new(None),
        copy_esc_keys: std::cell::RefCell::new(None),
        delete_keys: std::cell::RefCell::new(None),
    };
}
