//! The Settings dialog's Test rows (spec/03-shell.md §5, spec/inventory/shell.md §D).
//!
//! This module owns **widgets and nothing else**. Every pass rule lives in [`crate::checks`], which
//! is handed what a probe answered and returns the verdict — so nothing here opens a socket, runs a
//! binary or reads a file. What a Test button does is: read its own box, hand that value plus an
//! answer to [`press_test`], paint the badge with what came back, and mirror the line into both the
//! dialog's log and the main window's.
//!
//! The answer comes from an injected provider rather than a live server because this container has no
//! LLM, no ffmpeg and no firefox to ask; a real session hands in one that actually probes. A press
//! with nothing wired fails loudly instead of going green.
//!
//! A probe is a network call or a subprocess (60 s for the LLM, 120 s for vision, a headless browser for
//! firefox), so it never runs on the window's thread: each press runs its provider on a thread of its own and
//! the verdict is painted when it lands, the row's spinner turning meanwhile. The first port called the
//! provider inline, so one Test froze the window until it answered and Test All froze it for minutes, with
//! the spinner painted and replaced inside the same call.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use adw::prelude::*;
use gtk4 as gtk;

use super::settings_info;
use super::settings_servers;
use crate::checks;
use crate::machine_store;
use crate::model_list;
use crate::server_entries::{self, Section};
use crate::settings::Conf;

/// What a probe hands back for one row: the detail that goes on the ✓, or the reason that goes on ✗.
pub type Answer = Result<String, String>;

/// Who answers a row's probe. `key` is the row's key, `typed` the value read out of its box at the
/// press — the provider is expected to go and ask *that* address, not the one in the file, which
/// still lags the keyboard by [`checks::CONF_SAVE_WAIT`].
///
/// `Send + Sync` because a press runs it on a thread of its own (see the module's last paragraph).
pub type Provider = Arc<dyn Fn(&str, &str) -> Answer + Send + Sync>;

/// Every row with a Test button, in dialog order, with the name its log line carries. "Test All runs every
/// test" (§5): all twelve, not the six service kinds the first port ran.
const KINDS: [(&str, &str); 12] = [
    ("llm", "LLM"),
    ("llm-vision", "LLM vision"),
    ("decision", "decision model"),
    ("ffmpeg", "ffmpeg"),
    ("firefox", "firefox"),
    ("audio", "audio.cpp"),
    ("tts", "TTS model"),
    ("asr", "ASR model"),
    ("diar", "diarization model"),
    ("sep", "separation model"),
    ("aligner", "forced aligner"),
    ("sd", "sd.cpp"),
];

thread_local! {
    /// Tests pressed and not yet answered, across every open dialog: what [`wait_for_tests`] waits out.
    static IN_FLIGHT: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// How often the window's thread looks for a probe's answer: short enough to feel immediate, long enough to
/// cost nothing while a 120 s vision probe runs.
const ANSWER_POLL: std::time::Duration = std::time::Duration::from_millis(50);

/// Pump the main loop until every pressed Test has answered, or `limit` passes; answers whether they all
/// did. The door a widget test uses after a press, since a press now returns before its verdict.
pub fn wait_for_tests(limit: std::time::Duration) -> bool {
    let started = std::time::Instant::now();
    let context = glib::MainContext::default();
    while IN_FLIGHT.with(|n| n.get()) > 0 {
        if started.elapsed() >= limit {
            return false;
        }
        if !context.iteration(false) {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }
    true
}

/// The row's name in the log for its widget key. The log is read after the dialog has closed, so the
/// row spells itself out there rather than only sitting beside the mark on screen.
fn name_for(key: &str) -> &'static str {
    KINDS
        .iter()
        .find(|(kind, _)| *kind == key)
        .map(|(_, name)| *name)
        .unwrap_or("setting")
}

/// The seam every Test button goes through: the TYPED value in, a finished [`checks::Phase`] out.
///
/// Deliberately a free function over [`checks::TestRow`] rather than a method with a server behind
/// it — §5's "each Test reads what is TYPED" is the whole input side, and keeping it that way is
/// what lets a headless test assert that a press tested the box and not the file.
pub fn press_test(key: &str, typed: &str, answer: Answer) -> checks::Phase {
    let mut row = checks::TestRow::new(name_for(key));
    row.press(typed);
    row.finish(answer);
    row.phase.clone()
}

/// One Test All answer: the kind asked and what came back.
pub type AllAnswer = (String, Answer);

/// "Test All": every kind's own verdict, none short-circuiting (§5).
pub fn press_all(answers: &[AllAnswer]) -> Vec<checks::Row> {
    let pairs: Vec<(&str, Answer)> = answers
        .iter()
        .map(|(kind, verdict)| (name_for(kind), verdict.clone()))
        .collect();
    checks::test_all(&pairs)
}

/// Paint one badge from a phase: blank while idle, `…` while spinning, ✓/✗ with the verdict as its
/// tooltip once answered (§5: "✓ / ✗ · verdict as tooltip" — the mark says *that* row failed, only
/// its sentence says which of the reasons it failed).
pub fn paint_badge(badge: &gtk::Label, phase: &checks::Phase) {
    // The row's spinner sits just before its badge in the same cell (`spinner-<key>`); rows without one (Fetch
    // models, Use) show the mark alone.
    let spinner = badge
        .prev_sibling()
        .and_then(|w| w.downcast::<gtk::Spinner>().ok());
    let running = matches!(phase, checks::Phase::Running);
    if let Some(spinner) = spinner.as_ref() {
        spinner.set_visible(running);
        spinner.set_spinning(running);
    }
    match phase {
        checks::Phase::Idle => {
            badge.set_text("");
            badge.set_tooltip_text(None::<&str>);
        }
        checks::Phase::Running => {
            // §5: "shows a spinner then ✓/✗". The mark is blank while the spinner turns; a row with no spinner
            // of its own keeps the ellipsis so the wait is still visible.
            badge.set_text(if spinner.is_some() { "" } else { "\u{2026}" });
            badge.set_tooltip_text(Some("testing\u{2026}"));
        }
        checks::Phase::Finished(verdict) => {
            let passed = verdict.is_ok();
            badge.set_text(if passed { "\u{2713}" } else { "\u{2717}" });
            badge.remove_css_class("test-ok");
            badge.remove_css_class("test-bad");
            badge.add_css_class(if passed { "test-ok" } else { "test-bad" });
            let detail = verdict.as_ref().unwrap_or_else(|reason| reason);
            badge.set_tooltip_text(Some(detail));
        }
    }
}

thread_local! {
    /// The dialogs open this session. The dialog is its own top-level window, so a name walk from the
    /// main window cannot reach it; the accessors below look here first.
    static OPEN: RefCell<Vec<Rc<Dialog>>> = const { RefCell::new(Vec::new()) };
}

/// The dialog: the grid of rows, their badges, Test All, and the log pane that opens on the first
/// failure ([`checks::TestLog`]).
struct Dialog {
    window: adw::Window,
    /// §5's no-Save write: what is on disk, when the next one is due, and where it goes. Built over
    /// the conf as it stood when the dialog opened, which is also what seeds the boxes.
    store: RefCell<Option<machine_store::Store>>,
    /// Wall-clock origin for the 600 ms beat, so `flush`/`touched` get one consistent "now".
    started: std::time::Instant,
    /// The dialog's own bookkeeping, so a timer can reach it after `build_owned` returns.
    this: RefCell<Option<Rc<Dialog>>>,
    entries: RefCell<Vec<(String, gtk::Entry)>>,
    badges: RefCell<Vec<(String, gtk::Label)>>,
    buttons: RefCell<Vec<(String, gtk::Button)>>,
    /// The ids the last Fetch brought back, kept so Use can check its pick against what the server
    /// actually served rather than trusting the widget's memory.
    fetched: RefCell<Vec<String>>,
    list: gtk::DropDown,
    /// Set while a save timer is waiting, so a burst of keystrokes arms one timeout, not fifty.
    save_armed: Rc<std::cell::Cell<bool>>,
    log_view: gtk::TextView,
    log_expander: gtk::Expander,
    test_log: RefCell<checks::TestLog>,
    provider: Provider,
}

impl Dialog {
    /// The moment the dialog opened, as the beat measures it.
    fn now(&self) -> std::time::Duration {
        self.started.elapsed()
    }

    /// Every box's key and current text — the whole store as the user sees it, which is what a save
    /// writes (§5 has no per-row save: the file always holds all of the boxes).
    fn typed_all(&self) -> Vec<(String, String)> {
        self.entries
            .borrow()
            .iter()
            .map(|(key, entry)| (key.clone(), entry.text().to_string()))
            .collect()
    }

    /// Put the conf's own values into the boxes, so the dialog shows what is on disk rather than
    /// placeholders. A key with no conf field (Vision) keeps its placeholder.
    fn seed_entries(&self) {
        let base = self.store_base();
        let seeded: Vec<(String, String)> = self
            .entries
            .borrow()
            .iter()
            .filter_map(|(key, _)| {
                machine_store::text_for(&base, key).map(|value| (key.clone(), value))
            })
            .collect();
        for (key, value) in seeded {
            if let Some(entry) = self.entries.borrow().iter().find(|(row, _)| *row == key) {
                entry.1.set_text(&value);
            }
        }
    }

    /// The settings as the boxes stand: every typed value laid over the saved ones, which is what a Test reads
    /// (§5: "each Test reads what is typed"), the Server box and the keys included, not only its own box.
    fn typed_conf(&self) -> Conf {
        let mut conf = if self.store.borrow().is_some() {
            self.store_base()
        } else {
            crate::settings::from_environment()
                .and_then(|paths| crate::settings::read(&paths).ok())
                .unwrap_or_default()
        };
        for (key, value) in self.typed_all() {
            machine_store::set_field(&mut conf, &key, &value);
        }
        conf
    }

    /// The conf the boxes were seeded from, for the spin seeding above.
    fn store_base(&self) -> Conf {
        self.store
            .borrow()
            .as_ref()
            .map(|store| store.base_conf())
            .unwrap_or_default()
    }

    /// Put `conf`'s ids and counts into the boxes WITHOUT a store, so a picture of the dialog can show
    // what a real machine's settings look like while §5's no-Save beat stays unarmed (nothing to write
    // to: a snapshot must not touch `$XDG_CONFIG_HOME`).
    fn paint_from(&self, conf: &Conf) {
        for (key, entry) in self.entries.borrow().iter() {
            if let Some(value) = machine_store::text_for(conf, key) {
                entry.set_text(&value);
            }
        }
    }

    /// A keystroke anywhere: arm the beat, and make sure a timer is waiting to land it.
    fn touched(self: &Rc<Self>) {
        let owed = {
            let mut store = self.store.borrow_mut();
            match store.as_mut() {
                Some(store) => {
                    store.touched(self.now());
                    store.due_at()
                }
                None => return,
            }
        };
        if owed.is_none() || self.save_armed.get() {
            return;
        }
        self.save_armed.set(true);
        let owned = Rc::clone(
            &self
                .this
                .borrow()
                .clone()
                .expect("the dialog holds itself while open"),
        );
        glib::timeout_add_local(checks::CONF_SAVE_WAIT, move || owned.land_save());
    }

    /// The timer's job: write if the beat elapsed, say so in both logs, and stop until the next key.
    fn land_save(self: &Rc<Self>) -> glib::ControlFlow {
        self.save_armed.set(false);
        let (line, still_due) = {
            let mut store = self.store.borrow_mut();
            match store.as_mut() {
                Some(store) => (store.flush(self.now(), &self.typed_all()), store.due_at()),
                None => return glib::ControlFlow::Break,
            }
        };
        if let Ok(Some(line)) = line {
            self.note(&line);
        }
        settings_servers::persist(&|line: &str| self.note(line));
        // A keystroke inside the beat moved the write back, and `touched` arms no second timer while this one
        // waits: wait out the rest of the beat, or the file would only be written on close.
        if let Some(due) = still_due {
            self.save_armed.set(true);
            let owned = Rc::clone(self);
            glib::timeout_add_local(due.saturating_sub(self.now()), move || owned.land_save());
        }
        // One shot: a later keystroke arms a fresh beat, so the file is written once typing stops.
        glib::ControlFlow::Break
    }

    /// Closing the dialog: land whatever is still owed, then forget the store.
    fn close_store(&self) {
        let taken = self.store.borrow_mut().take();
        if let Some(mut store) = taken {
            if let Ok(Some(line)) = store.on_close(&self.typed_all()) {
                self.note(&line);
            }
        }
        settings_servers::close(&|line: &str| self.note(line));
    }

    /// One line into the dialog's log pane and the main log, the way a Test verdict reports (§5).
    fn note(&self, line: &str) {
        let buffer = self.log_view.buffer();
        let mut end = buffer.end_iter();
        buffer.insert(&mut end, &format!("settings: {line}\n"));
        crate::ui::window::log_line(&format!("settings: {line}"));
    }

    /// What the box named `entry-<key>` holds right now — the value the press tests.
    fn typed(&self, key: &str) -> String {
        self.entries
            .borrow()
            .iter()
            .find(|(row, _)| row == key)
            .map(|(_, entry)| entry.text().to_string())
            .unwrap_or_default()
    }

    fn badge(&self, key: &str) -> Option<gtk::Label> {
        self.badges
            .borrow()
            .iter()
            .find(|(row, _)| row == key)
            .map(|(_, label)| label.clone())
    }

    /// The value box for `key` — what a Test press reads and what a test types into.
    fn entry(&self, key: &str) -> Option<gtk::Entry> {
        self.entries
            .borrow()
            .iter()
            .find(|(row, _)| row == key)
            .map(|(_, entry)| entry.clone())
    }

    /// One verdict, painted and mirrored: badge, dialog log, main window log.
    fn report(&self, key: &str, phase: &checks::Phase) {
        if let Some(badge) = self.badge(key) {
            paint_badge(&badge, phase);
        }
        let checks::Phase::Finished(verdict) = phase else {
            return;
        };
        let line = checks::log_line(name_for(key), verdict);
        // The dialog's own expander opens on the first failure and never closes itself again.
        self.test_log
            .borrow_mut()
            .push(line.clone(), verdict.is_ok());
        self.paint_log();
        self.log_expander.set_expanded(self.test_log.borrow().open);
        // And the same line goes to the main log, where it is read after this dialog is gone (§5).
        crate::ui::window::log_line(&line);
    }

    fn paint_log(&self) {
        let buffer = self.log_view.buffer();
        let body: String = self
            .test_log
            .borrow()
            .lines
            .iter()
            .map(|line| format!("{line}\n"))
            .collect();
        buffer.set_text(&body);
    }

    /// Ask the provider for one row and report what it said.
    ///
    /// The provider alone answers, whatever the row: a test's canned provider must get its own rows
    /// back unchanged. The live HTTP routing lives in [`crate::settings_probe::live_provider`],
    /// which the window installs — hard-coding it here would send a test's LLM row to port 8731 and
    /// replace its verdict with a connection error.
    fn ask(self: &Rc<Self>, key: &str) {
        let typed = self.typed(key);
        self.report(key, &checks::Phase::Running);
        let row = key.to_string();
        let asked = typed.clone();
        self.off_thread(key, typed, move |dialog, answer| {
            dialog.report(&row, &press_test(&row, &asked, answer));
        });
    }

    /// Run the provider for `key` on a thread of its own with the boxes as typed, and hand its answer to
    /// `then` back on the window's thread. A probe that panics answers with a failure rather than leaving
    /// its spinner turning for ever.
    fn off_thread(
        self: &Rc<Self>,
        key: &str,
        typed: String,
        then: impl FnOnce(&Rc<Self>, Answer) + 'static,
    ) {
        let provider = Arc::clone(&self.provider);
        let conf = self.typed_conf();
        let row = key.to_string();
        let worker = std::thread::spawn(move || {
            // The typed boxes are a thread-local of the probe module, so they are laid over on THIS thread.
            crate::settings_probe::with_typed(conf, || provider(&row, &typed))
        });
        IN_FLIGHT.with(|n| n.set(n.get() + 1));
        let dialog = Rc::clone(self);
        let mut worker = Some(worker);
        let mut then = Some(then);
        glib::timeout_add_local(ANSWER_POLL, move || {
            if !worker.as_ref().is_some_and(|w| w.is_finished()) {
                return glib::ControlFlow::Continue;
            }
            let answer = worker
                .take()
                .expect("polled until finished, taken once")
                .join()
                .unwrap_or_else(|_| {
                    Err("the test stopped unexpectedly -- see the terminal".to_string())
                });
            if let Some(then) = then.take() {
                then(&dialog, answer);
            }
            IN_FLIGHT.with(|n| n.set(n.get().saturating_sub(1)));
            glib::ControlFlow::Break
        });
    }

    /// Fetch models: ask the server what it serves and fill the dropdown.
    ///
    /// Not a pass/fail service test like the six rows — it discovers an id rather than proving a
    /// server healthy — but it reports through the same badge and log so an empty or unreachable
    /// listing is visible in exactly one place (§5's mirroring rule). The address asked is the one
    /// TYPED, not the one saved: the file lags the keyboard by `checks::CONF_SAVE_WAIT`.
    fn fetch_models(self: &Rc<Self>) {
        // The Server box: the address to list from. The Model box had been passed here, so a listing asked
        // the model's name as if it were a host.
        let typed = self.typed("server");
        self.report(FETCH_KEY, &checks::Phase::Running);
        let asked = typed.clone();
        self.off_thread(FETCH_KEY, typed, move |dialog, listed| {
            dialog.show_listing(&asked, listed)
        });
    }

    /// Fill the dropdown from what the listing answered, and report it.
    fn show_listing(&self, typed: &str, listed: Answer) {
        let typed = typed.to_string();
        match listed {
            Ok(body) => {
                let ids = model_list::parse_ids(
                    &serde_json::from_str(&body).unwrap_or_else(|_| serde_json::json!({})),
                );
                // The list is rebuilt wholesale from a `gtk::StringList`, which is what gives a
                // clean replace: DropDown has no remove-all, and appending to the old model would
                // leave last fetch's ids under this one's.
                let strings = gtk::StringList::new(&[]);
                for id in &ids {
                    strings.append(id);
                }
                self.list.set_model(Some(&strings));
                // Start on nothing chosen: adopting an id is a deliberate press of Use.
                self.list.set_selected(gtk::INVALID_LIST_POSITION);
                *self.fetched.borrow_mut() = ids;
                let verdict = model_list::list_verdict(self.fetched.borrow().as_slice());
                self.report(FETCH_KEY, &press_test(FETCH_KEY, &typed, verdict));
            }
            Err(reason) => {
                self.report(FETCH_KEY, &press_test(FETCH_KEY, &typed, Err(reason)));
            }
        }
    }

    /// Use: copy the chosen id into Model, only if the server really listed it.
    ///
    /// Checked against [`Self::fetched`] rather than the widget, because the dropdown can hold a
    /// selection from a listing that has since been replaced.
    fn use_model(&self) {
        let chosen = if self.list.selected() == gtk::INVALID_LIST_POSITION {
            String::new()
        } else {
            // A `StringList` hands back a `StringObject`; `string()` is the only way to read it
            // (`GStr` is unsized and cannot be a downcast target).
            self.list
                .selected_item()
                .and_then(|item| item.downcast::<gtk::StringObject>().ok())
                .map(|obj| obj.string().to_string())
                .unwrap_or_default()
        };
        let ids = self.fetched.borrow().clone();
        // Into the model box the person was last in, Model or Decision: the press itself takes the focus, so it
        // is the box entered last that names the target (Model when neither was).
        let target = USE_TARGET.with(|target| target.borrow().clone());
        match model_list::use_choice(&ids, &chosen) {
            Ok(id) => {
                if let Some(entry) = self
                    .entries
                    .borrow()
                    .iter()
                    .find(|(k, _)| *k == target)
                    .map(|(_, e)| e)
                {
                    entry.set_text(&id);
                }
                let label = if target == "decision" {
                    "Decision"
                } else {
                    "Model"
                };
                self.report(
                    USE_KEY,
                    &press_test(USE_KEY, &chosen, Ok(format!("{label} set to {id}"))),
                );
            }
            Err(reason) => {
                self.report(USE_KEY, &press_test(USE_KEY, &chosen, Err(reason)));
            }
        }
    }

    /// Test All: every row, all started at once, each reporting on its own as it lands. Nothing stops at a
    /// failure — one early ✗ hiding the rest would send the user through the dialog ten times.
    fn ask_all(self: &Rc<Self>) {
        for (key, _) in KINDS {
            self.ask(key);
        }
    }
}

/// Esc closes the window it is put on, through the same close request as the close button.
pub fn close_on_escape() -> gtk::ShortcutController {
    let keys = gtk::ShortcutController::new();
    keys.set_name(Some("settings-escape"));
    keys.add_shortcut(gtk::Shortcut::new(
        gtk::ShortcutTrigger::parse_string("Escape"),
        Some(gtk::CallbackAction::new(|widget, _| {
            if let Some(window) = widget.downcast_ref::<gtk::Window>() {
                window.close();
            }
            glib::Propagation::Stop
        })),
    ));
    keys
}

/// The dialog's opening width (§5, inventory §D).
pub const DIALOG_WIDTH_PX: i32 = 960;

/// How many lines the dialog's log shows before it scrolls (§5).
pub const LOG_LINES: i32 = 10;

/// [`LOG_LINES`] lines of `view`'s own font, with the view's top and bottom margins.
pub fn log_lines_px(view: &gtk::TextView) -> i32 {
    let rows = vec!["x"; LOG_LINES as usize].join("\n");
    view.create_pango_layout(Some(&rows)).pixel_size().1 + view.top_margin() + view.bottom_margin()
}

/// §5's line between two sections: a horizontal rule across every cell column, named for the section
/// it introduces ("the sections … divided by separator lines"). Writing, the first section, gets none.
fn divider(grid: &gtk::Grid, row: i32, section: &str, cells: u32) {
    let key = settings_info::section_key(section).unwrap_or(section);
    let line = gtk::Separator::new(gtk::Orientation::Horizontal);
    line.set_widget_name(&format!("settings-separator-{key}"));
    grid.attach(&line, 0, row, cells as i32, 1);
}

/// Build the dialog. `provider` answers each row; see [`no_probe`] for what a build without one says.
pub fn build(parent: Option<&gtk::Window>, provider: Provider) -> adw::Window {
    build_owned(parent, provider).window.clone()
}

/// The same dialog showing `conf`'s own values, for the `03-settings` picture only: a real session
/// seeds from the file through [`Dialog::seed_entries`], and this is the one door that paints a conf
/// handed in rather than read. It writes nothing anywhere.
pub fn build_from(parent: Option<&gtk::Window>, provider: Provider, conf: &Conf) -> adw::Window {
    let dialog = build_owned(parent, provider);
    dialog.paint_from(conf);
    dialog.window.clone()
}

/// The same build, returning the bookkeeping so the caller can keep it live in [`OPEN`].
fn build_owned(parent: Option<&gtk::Window>, provider: Provider) -> Rc<Dialog> {
    let window = adw::Window::new();
    window.set_title(Some("Settings"));
    window.set_modal(true);
    window.set_transient_for(parent);
    // §5 (inventory §D): 960 wide, so the longest placeholder and a model id read whole in their boxes.
    window.set_default_width(DIALOG_WIDTH_PX);

    let root = gtk::Box::new(gtk::Orientation::Vertical, 12);
    root.set_margin_start(18);
    root.set_margin_end(18);
    root.set_margin_top(12);
    root.set_margin_bottom(12);

    let grid = gtk::Grid::new();
    grid.set_row_spacing(6);
    grid.set_column_spacing(8);
    // §5's five columns: section | label | box | ✓/✗ | Test. Every row attaches its badge at `COL_BADGE`
    // and its Test button at `COL_TEST`, so those two read as one column down the whole dialog.
    const COL_SECTION: i32 = 0;
    const COL_LABEL: i32 = 1;
    const COL_BOX: i32 = 2;
    const COL_BADGE: i32 = 3;
    const COL_TEST: i32 = 4;
    /// Cells in one row, which is how wide a section divider spans.
    const CELLS: u32 = 6;
    let mut row = 0;

    // Five columns: section | label | value | badge | Test. The section label appears once per
    // group, on its first row, as the inventory draws it.
    let add = |section: Option<&str>,
               label: &str,
               entry_key: &str,
               placeholder: &str,
               secret: bool,
               test_key: Option<&str>,
               tip: &str,
               row: i32,
               grid: &gtk::Grid| {
        if let Some(section) = section {
            // §5: "Each section's ⓘ explains at length what the server is expected to speak." The
            // heading, its ⓘ and (for the two optional servers) the word "optional" share one box in
            // column 0, which is how spec/img/03-settings.png draws them: `Audio ⓘ optional`.
            let head = gtk::Box::new(gtk::Orientation::Horizontal, 4);
            // An unknown heading gets no ⓘ and keeps its own text as the box name: no other section's
            // name is borrowed for it.
            let key = settings_info::section_key(section);
            head.set_widget_name(&format!("settings-section-{}", key.unwrap_or(section)));
            let title =
                gtk::Label::new(Some(section.strip_suffix(" (optional)").unwrap_or(section)));
            title.add_css_class("heading");
            title.set_halign(gtk::Align::Start);
            head.append(&title);
            if let Some(key) = key {
                if let Some(info) = settings_info::info_button(key) {
                    head.append(&info);
                }
                if settings_info::is_optional(section) {
                    let optional = gtk::Label::new(Some(settings_info::OPTIONAL_WORD));
                    optional.add_css_class("dim-label");
                    head.append(&optional);
                }
            }
            // §5: Writing, Audio and Drawing carry their named server entries under the heading: the
            // dropdown, ＋ New and 🗑. The heading box keeps its name; a column holds the two lines.
            match key.and_then(settings_servers::section_of) {
                Some(servers) => {
                    let column = gtk::Box::new(gtk::Orientation::Vertical, 4);
                    column.append(&head);
                    column.append(&settings_servers::row(servers));
                    column.set_valign(gtk::Align::Start);
                    // Over the section's first TWO rows (server and key: every server section has both),
                    // so the two lines do not make the server row twice as tall. Attached over one row,
                    // they did, and the server box stretched to fill it.
                    grid.attach(&column, COL_SECTION, row, 1, 2);
                }
                None => grid.attach(&head, COL_SECTION, row, 1, 1),
            }
        }
        // §5: the field label is right-aligned, so every one of them ends on the same edge.
        let caption = gtk::Label::new(Some(label));
        caption.set_halign(gtk::Align::End);
        grid.attach(&caption, COL_LABEL, row, 1, 1);

        let entry = gtk::Entry::new();
        entry.set_widget_name(&format!("entry-{entry_key}"));
        entry.set_placeholder_text(Some(placeholder));
        entry.set_hexpand(true);
        // A box is one line tall whatever its row holds beside it.
        entry.set_valign(gtk::Align::Center);
        if secret {
            entry.set_visibility(false);
        }
        grid.attach(&entry, COL_BOX, row, 1, 1);

        // The badge cell holds the row's spinner (shown only while its Test runs) and its ✓/✗ mark.
        let cell = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        cell.set_halign(gtk::Align::Center);
        let spinner = gtk::Spinner::new();
        spinner.set_widget_name(&format!("spinner-{entry_key}"));
        spinner.set_visible(false);
        cell.append(&spinner);
        let badge = gtk::Label::new(None);
        badge.set_widget_name(&format!("badge-{entry_key}"));
        badge.set_width_chars(2);
        cell.append(&badge);
        grid.attach(&cell, COL_BADGE, row, 1, 1);

        if let Some(test_key) = test_key {
            let test = gtk::Button::with_label("Test");
            test.set_widget_name(&format!("test-{test_key}-button"));
            test.set_tooltip_text(Some(tip));
            grid.attach(&test, COL_TEST, row, 1, 1);
        }
    };

    // Writing.
    add(
        Some("Writing"),
        "Server",
        "server",
        &format!("empty = {}", crate::services::llm_default()),
        false,
        None,
        "Ask this server and model for one short completion",
        row,
        &grid,
    );
    row += 1;
    add(
        None,
        "API key",
        "llm-key",
        "",
        true,
        None,
        "Sent as a bearer token to the server above",
        row,
        &grid,
    );
    row += 1;
    add(
        None,
        "Model",
        "llm",
        "model id exactly as the server lists it",
        false,
        Some("llm"),
        "Ask this server and model for one short completion",
        row,
        &grid,
    );
    row += 1;
    add(
        None,
        "Vision",
        "llm-vision",
        "nothing to type: Test shows the Model a red square",
        false,
        Some("llm-vision"),
        "Show this model a small sample image and check it names what it sees",
        row,
        &grid,
    );
    row += 1;

    add(
        None,
        "Decision",
        "decision",
        "empty = the Model above; a fast vision model, e.g. Clef",
        false,
        Some("decision"),
        "Prepare asks this model whether a slide's text changed between two frames (the text gate); empty \
asks the Model above. Test sends the gate's own question with one picture shown twice, and checks the \
model answers through judge_text that nothing changed",
        row,
        &grid,
    );
    row += 1;

    // Fetch models: the list and the copy-into-Model button share the Writing block. Nothing on this row
    // reaches past the Test buttons' column, where its two marks sit side by side: the dropdown takes the
    // box column alone (a wider span pushed Use out to the dialog's edge), and an id longer than a box is
    // ellipsised in the closed widget exactly as every other box here ellipsises long text.
    let fetch = gtk::Button::with_label("Fetch models");
    fetch.set_widget_name("fetch-models-button");
    fetch.set_tooltip_text(Some("List the models this server serves"));
    grid.attach(&fetch, COL_LABEL, row, 1, 1);
    let list = gtk::DropDown::from_strings(&[]);
    list.set_widget_name("model-list");
    grid.attach(&list, COL_BOX, row, 1, 1);
    let use_choice = gtk::Button::with_label("Use");
    use_choice.set_widget_name("use-model-button");
    use_choice.set_tooltip_text(Some(
        "Copy the chosen id into the model box you were last in: Model, or Decision",
    ));
    // §5 puts Use in the Test column — it is this line's action button, beside every other row's Test.
    grid.attach(&use_choice, COL_TEST, row, 1, 1);
    // These two verdicts are not `add(...)` rows, so their badges are made here. Without a named
    // badge widget `Dialog::report` has nowhere to paint and a failed listing would show up only in
    // the log — §5 wants the ✓/✗ with its verdict as tooltip on the dialog itself. Both marks take
    // the badge cell every other row's mark sits in, side by side in the order the controls they
    // report on sit in: this row has no Test button, so the pair cannot share that cell with one.
    let marks = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    marks.set_halign(gtk::Align::Center);
    for key in [FETCH_KEY, USE_KEY] {
        let badge = gtk::Label::new(None);
        badge.set_widget_name(&format!("badge-{key}"));
        badge.set_width_chars(2);
        badge.set_valign(gtk::Align::Center);
        marks.append(&badge);
    }
    grid.attach(&marks, COL_BADGE, row, 1, 1);
    row += 1;

    // Cutting.
    divider(&grid, row, "Cutting", CELLS);
    row += 1;
    add(
        Some("Cutting"),
        "ffmpeg",
        "ffmpeg",
        "empty = PATH (ffprobe taken from the same folder)",
        false,
        Some("ffmpeg"),
        "Check the build has the filters and encoders the steps use",
        row,
        &grid,
    );
    row += 1;
    add(
        None,
        "firefox",
        "firefox",
        "path, or \"off\" for no web search",
        false,
        Some("firefox"),
        "Run one headless search, or accept \"off\"",
        row,
        &grid,
    );
    row += 1;

    // Audio (optional): the endpoint, then one row per model box.
    divider(&grid, row, "Audio (optional)", CELLS);
    row += 1;
    add(
        Some("Audio (optional)"),
        "Server",
        "audio",
        &format!("empty = {}", crate::services::audio_default()),
        false,
        Some("audio"),
        "Check the server answers and can narrate",
        row,
        &grid,
    );
    row += 1;
    add(
        None,
        "API key",
        "audio-key",
        "",
        true,
        None,
        "Sent as a bearer token to the audio server above",
        row,
        &grid,
    );
    row += 1;
    for (label, task) in checks::AUDIO_ROWS {
        let key = match task {
            "clon" => "tts",
            "asr" => "asr",
            "diar" => "diar",
            _ => "sep",
        };
        add(
            None,
            label,
            key,
            "model id as the audio.cpp server lists it",
            false,
            Some(key),
            &format!("Check this id is served and declared for task {task:?}"),
            row,
            &grid,
        );
        row += 1;
    }
    add(
        None,
        "Forced aligner",
        "aligner",
        "empty = whatever the server serves for \"align\"",
        false,
        Some("aligner"),
        "What aligns; none is a success \u{2014} cut points come off the waveform",
        row,
        &grid,
    );
    row += 1;

    // Drawing (optional).
    divider(&grid, row, "Drawing (optional)", CELLS);
    row += 1;
    add(
        Some("Drawing (optional)"),
        "Server",
        "sd",
        &format!("empty = {}", crate::services::image_default()),
        false,
        Some("sd"),
        "Report the weights sd-server has loaded",
        row,
        &grid,
    );
    row += 1;
    add(
        None,
        "API key",
        "sd-key",
        "",
        true,
        None,
        "Sent as a bearer token to the drawing server above",
        row,
        &grid,
    );

    root.append(&grid);

    let all = gtk::Button::with_label("Test All");
    all.set_widget_name("test-all-button");
    all.set_tooltip_text(Some("Run every test above and report each one"));
    // §5: Test All sits at the bottom right, under the sections and above the log.
    all.set_halign(gtk::Align::End);
    root.append(&all);

    let log_expander = gtk::Expander::new(Some("Log"));
    log_expander.set_widget_name("settings-log-expander");
    let log_view = gtk::TextView::new();
    crate::ui::margins::text_box(&log_view);
    log_view.set_widget_name("settings-log");
    log_view.set_editable(false);
    log_view.set_monospace(true);
    // §5: the log is LOG_LINES lines tall however many it holds, and scrolls. A bare TextView asks for its
    // whole text's height, so every Test All pushed Test All and the sections further up the dialog.
    let log_scroller = gtk::ScrolledWindow::new();
    log_scroller.set_widget_name("settings-log-scroller");
    let tall = log_lines_px(&log_view);
    log_scroller.set_min_content_height(tall);
    log_scroller.set_max_content_height(tall);
    log_scroller.set_child(Some(&log_view));
    super::log_paint::follow_end(&log_scroller, &log_view);
    log_expander.set_child(Some(&log_scroller));
    root.append(&log_expander);

    // §5's picture: a title bar reading "Settings" with its close button. Without one the dialog could only
    // be closed by the window manager.
    let view = adw::ToolbarView::new();
    let header = adw::HeaderBar::new();
    header.set_widget_name("settings-header");
    view.add_top_bar(&header);
    view.set_content(Some(&root));
    window.set_content(Some(&view));
    // Esc closes it the way the close button does, so the owed write lands on the same close.
    window.add_controller(close_on_escape());

    // Wire every Test button to the row it belongs to. The pairing is read back off the widget names
    // rather than kept in a second list, so a row added above cannot be left unwired.
    let dialog = Rc::new(Dialog {
        window: window.clone(),
        entries: RefCell::new(vec![]),
        badges: RefCell::new(vec![]),
        buttons: RefCell::new(vec![]),
        fetched: RefCell::new(vec![]),
        list: find_in(window.upcast_ref(), "model-list")
            .and_then(|w| w.downcast::<gtk::DropDown>().ok())
            .expect("the dialog builds model-list"),
        log_view,
        log_expander,
        test_log: RefCell::new(checks::TestLog::new()),
        provider,
        // §5's no-Save write, over the conf as it stands now. No config folder at all (a bare test
        // box with no XDG_HOME) leaves it `None`: the dialog still tests, it just has nowhere to save.
        store: RefCell::new(
            crate::settings::from_environment()
                .and_then(|paths| machine_store::Store::open(&paths).ok()),
        ),
        started: std::time::Instant::now(),
        save_armed: Rc::new(std::cell::Cell::new(false)),
        this: RefCell::new(None),
    });
    *dialog.this.borrow_mut() = Some(Rc::clone(&dialog));
    // The Test wiring is what fills `entries`, so the save wiring has to come after it: a box created
    // before this point would be typed into with nothing listening.
    wire_buttons(&dialog);
    register_value_boxes(&dialog);
    wire_use_target(&dialog);
    // The boxes show what is on disk, and every keystroke arms the 600 ms beat that writes it back.
    dialog.seed_entries();
    wire_entries_to_store(&dialog);
    wire_servers(&dialog);
    dialog
}

/// §5's named server entries: picking one fills its section's boxes and puts every one of its settings (the
/// voices folder, which has no box, included) into what the next write puts in `llm.conf`.
fn wire_servers(dialog: &Rc<Dialog>) {
    let fill = {
        let dialog = Rc::downgrade(dialog);
        move |section: Section, entry: &server_entries::Entry| {
            let Some(dialog) = dialog.upgrade() else {
                return;
            };
            for conf_key in section.conf_keys() {
                let Some(box_key) = server_entries::box_of(conf_key) else {
                    continue;
                };
                let value = entry.get(*conf_key).map(String::as_str).unwrap_or("");
                if let Some((_, input)) = dialog
                    .entries
                    .borrow()
                    .iter()
                    .find(|(key, _)| key == box_key)
                {
                    input.set_text(value);
                }
            }
            if let Some(store) = dialog.store.borrow_mut().as_mut() {
                store.adopt(|conf| server_entries::apply(section, entry, conf));
            }
            // The same text set again emits no change, and the voices folder has no box: arm the beat here.
            dialog.touched();
        }
    };
    let values_now = {
        let dialog = Rc::downgrade(dialog);
        move |section: Section| {
            dialog
                .upgrade()
                .map(|dialog| server_entries::values_from(section, &dialog.typed_conf()))
                .unwrap_or_default()
        }
    };
    let note = {
        let dialog = Rc::downgrade(dialog);
        move |line: &str| {
            if let Some(dialog) = dialog.upgrade() {
                dialog.note(line);
            }
        }
    };
    let conf = dialog.store_base();
    settings_servers::wire(
        settings_servers::Lent {
            fill: Rc::new(fill),
            values_now: Rc::new(values_now),
            note: Rc::new(note),
        },
        &conf,
    );
}

/// Every value box the dialog draws, the ones without a Test of their own included: the Server and API key
/// boxes had been registered only through their row's Test button, which they do not have, so they were
/// neither filled from the file nor written back, and a Test could not read them as typed.
fn register_value_boxes(dialog: &Rc<Dialog>) {
    fn walk(node: &gtk::Widget, out: &mut Vec<(String, gtk::Entry)>) {
        if let (Some(key), Some(entry)) = (
            node.widget_name()
                .strip_prefix("entry-")
                .map(str::to_string),
            node.downcast_ref::<gtk::Entry>(),
        ) {
            out.push((key, entry.clone()));
        }
        let mut child = node.first_child();
        while let Some(c) = child {
            child = c.next_sibling();
            walk(&c, out);
        }
    }
    let mut found = Vec::new();
    walk(dialog.window.upcast_ref(), &mut found);
    let mut entries = dialog.entries.borrow_mut();
    for (key, entry) in found {
        if !entries.iter().any(|(known, _)| *known == key) {
            entries.push((key, entry));
        }
    }
}

/// Use fills the model box entered last: Model or Decision. Wired after every value box is registered, the
/// Decision box having no Test button of its own.
fn wire_use_target(dialog: &Rc<Dialog>) {
    for key in ["llm", "decision"] {
        if let Some(entry) = dialog
            .entries
            .borrow()
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, e)| e.clone())
        {
            let focus = gtk::EventControllerFocus::new();
            let key = key.to_string();
            focus.connect_enter(move |_| {
                USE_TARGET.with(|target| *target.borrow_mut() = key.clone());
            });
            entry.add_controller(focus);
        }
    }
}

/// Every value box reports its keystrokes to the store (§5: written 600 ms after the last one).
fn wire_entries_to_store(dialog: &Rc<Dialog>) {
    for (key, entry) in dialog.entries.borrow().iter() {
        let owned = Rc::clone(dialog);
        let key = key.clone();
        entry.connect_changed(move |entry| {
            // §5: typing into a box changes the server entry in use as well.
            settings_servers::edited(&key, &entry.text());
            owned.touched();
        });
    }
}
/// Each `test-<key>-button` gets the row whose entry shares its key.
fn wire_buttons(dialog: &Rc<Dialog>) {
    for key in test_keys() {
        let Some(button) = find_in(dialog.window.upcast_ref(), &format!("test-{key}-button"))
            .and_then(|w| w.downcast::<gtk::Button>().ok())
        else {
            continue;
        };
        let Some(entry) = find_in(dialog.window.upcast_ref(), &format!("entry-{key}"))
            .and_then(|w| w.downcast::<gtk::Entry>().ok())
        else {
            continue;
        };
        let Some(badge) = find_in(dialog.window.upcast_ref(), &format!("badge-{key}"))
            .and_then(|w| w.downcast::<gtk::Label>().ok())
        else {
            continue;
        };
        dialog.entries.borrow_mut().push((key.to_string(), entry));
        dialog.badges.borrow_mut().push((key.to_string(), badge));
        dialog
            .buttons
            .borrow_mut()
            .push((key.to_string(), button.clone()));

        let owned = Rc::clone(dialog);
        let key = key.to_string();
        button.connect_clicked(move |_| owned.ask(&key));
    }

    if let Some(all) = find_in(dialog.window.upcast_ref(), "test-all-button")
        .and_then(|w| w.downcast::<gtk::Button>().ok())
    {
        let owned = Rc::clone(dialog);
        all.connect_clicked(move |_| owned.ask_all());
    }

    // Fetch models and Use: the two buttons that share the Writing block. They are looked up by name
    // like every other control, so neither can be drawn without being wired.
    if let Some(fetch) = find_in(dialog.window.upcast_ref(), "fetch-models-button")
        .and_then(|w| w.downcast::<gtk::Button>().ok())
    {
        let owned = Rc::clone(dialog);
        fetch.connect_clicked(move |_| owned.fetch_models());
    }
    if let Some(use_choice_button) = find_in(dialog.window.upcast_ref(), "use-model-button")
        .and_then(|w| w.downcast::<gtk::Button>().ok())
    {
        let owned = Rc::clone(dialog);
        use_choice_button.connect_clicked(move |_| owned.use_model());
    }
    // The two badges drawn outside `add(...)` join the same key -> badge map, so `report` paints
    // them exactly like a per-row verdict instead of dropping straight into the log.
    for key in [FETCH_KEY, USE_KEY] {
        if let Some(badge) = find_in(dialog.window.upcast_ref(), &format!("badge-{key}"))
            .and_then(|w| w.downcast::<gtk::Label>().ok())
        {
            dialog.badges.borrow_mut().push((key.to_string(), badge));
        }
    }
}

/// The provider key for the model listing. Not one of [`KINDS`]: Test All proves services, while
/// this looks up an id — it still reports through the same badge and the `settings:` log line.
pub const FETCH_KEY: &str = "fetch-models";

/// The provider key for adopting the chosen id into Model.
pub const USE_KEY: &str = "use-model";

thread_local! {
    /// The model box Use fills: the one of Model (`llm`) and Decision (`decision`) entered last.
    static USE_TARGET: RefCell<String> = RefCell::new("llm".to_string());
}

/// Every key that has a Test button, in dialog order.
fn test_keys() -> [&'static str; 12] {
    [
        "llm",
        "llm-vision",
        "decision",
        "ffmpeg",
        "firefox",
        "audio",
        "tts",
        "asr",
        "diar",
        "sep",
        "aligner",
        "sd",
    ]
}

/// The provider a build with nothing wired uses: every row fails with a reason that says so, rather
/// than showing a ✓ nobody probed for.
pub fn no_probe() -> Provider {
    Arc::new(|key, _| {
        Err(format!(
            "nothing wired to probe {key} -- run this in a real session"
        ))
    })
}

/// Open the dialog over `parent`, answering from `provider`.
pub fn open_with(parent: &adw::ApplicationWindow, provider: Provider) -> adw::Window {
    let dialog = build_owned(Some(parent.upcast_ref()), provider);
    // Closing lands whatever the last keystroke still owes: "settings saved to <path>", again on close.
    let owned = Rc::clone(&dialog);
    dialog.window.connect_close_request(move |_| {
        owned.close_store();
        // A closed dialog is open no more: off the list `dialog_open` and the accessors read (the first
        // rewrite never took it off, so "is the dialog open" stayed yes for the rest of the session).
        OPEN.with(|open| open.borrow_mut().retain(|held| !Rc::ptr_eq(held, &owned)));
        glib::Propagation::Proceed
    });
    OPEN.with(|open| open.borrow_mut().push(Rc::clone(&dialog)));
    dialog.window.present();
    dialog.window.clone()
}

/// Whether a settings dialog is open this session.
pub fn dialog_open(_window: &adw::ApplicationWindow) -> bool {
    OPEN.with(|open| !open.borrow().is_empty())
}

/// Land every open dialog's owed write, from wherever the app is being brought down. Returns how many
/// dialogs had a store to land.
///
/// The dialog is its OWN top level, so closing the main window never runs the hook `open_with` puts on
/// it — without this door the last ≤600 ms of typing would be dropped when the process exits (§5 "on
/// close", §6 precedence "dialog box → this file"). [`Dialog::close_store`] takes the store out, so a
/// dialog closed later (or by its own hook first) lands nothing twice.
pub fn flush_owed() -> usize {
    let dialogs: Vec<Rc<Dialog>> = OPEN.with(|open| open.borrow().clone());
    let mut landed = 0usize;
    for dialog in dialogs {
        if dialog.store.borrow().is_some() {
            landed += 1;
        }
        dialog.close_store();
    }
    landed
}

/// The header bar's Settings button.
pub fn settings_button(window: &adw::ApplicationWindow) -> Option<gtk::Button> {
    find_in(window.upcast_ref(), "settings-button")?
        .downcast()
        .ok()
}

/// The ids the open dialog's model list currently offers, newest first by fetch order.
///
/// A test reads the listing this way rather than reaching into the DropDown's `StringList`, so the
/// widget shape stays private to this module.
pub fn listed_models(_window: &adw::ApplicationWindow) -> Vec<String> {
    let Some(list) = find_named("model-list")
        .and_then(|w| w.downcast::<gtk::DropDown>().ok())
        .or_else(|| {
            last_dialog()
                .and_then(|d| find_in(d.window.upcast_ref(), "model-list"))
                .and_then(|w| w.downcast::<gtk::DropDown>().ok())
        })
    else {
        return Vec::new();
    };
    let Some(model) = list.model() else {
        return Vec::new();
    };
    let strings = match model.downcast::<gtk::StringList>() {
        Ok(strings) => strings,
        Err(_) => return Vec::new(),
    };
    (0..strings.n_items())
        .filter_map(|i| strings.string(i).map(|text| text.to_string()))
        .collect()
}

/// The open settings dialog's window, for tests that need to reach a control inside it. Its own
/// top-level, so the main window's tree does not contain it.
pub fn open_settings_window() -> Option<adw::Window> {
    last_dialog().map(|dialog| dialog.window.clone())
}

/// One row's Test button, by key (`test-<key>-button`).
pub fn test_button(window: &adw::ApplicationWindow, key: &str) -> Option<gtk::Button> {
    find_named(&format!("test-{key}-button"))?
        .downcast()
        .ok()
        .or_else(|| {
            find_in(window.upcast_ref(), &format!("test-{key}-button"))?
                .downcast()
                .ok()
        })
}

/// One row's badge, by key (`badge-<key>`).
pub fn badge(window: &adw::ApplicationWindow, key: &str) -> Option<gtk::Label> {
    last_dialog()
        .and_then(|dialog| dialog.badge(key))
        .or_else(|| {
            find_in(window.upcast_ref(), &format!("badge-{key}"))?
                .downcast()
                .ok()
        })
}

/// One row's value box, by key (`entry-<key>`) — the box a Test press reads what was typed from.
pub fn entry(window: &adw::ApplicationWindow, key: &str) -> Option<gtk::Entry> {
    last_dialog()
        .and_then(|dialog| dialog.entry(key))
        .or_else(|| {
            find_in(window.upcast_ref(), &format!("entry-{key}"))?
                .downcast()
                .ok()
        })
}

/// The dialog's own log text, which is what its expander shows.
pub fn dialog_log(window: &adw::ApplicationWindow) -> String {
    if let Some(dialog) = last_dialog() {
        return buffer_text(&dialog.log_view);
    }
    let _ = window;
    String::new()
}

/// Whether the dialog's log expander is open — the rule that opened it lives in
/// [`checks::TestLog::push`], this only reports what the widget shows.
pub fn dialog_log_open(window: &adw::ApplicationWindow) -> bool {
    if let Some(dialog) = last_dialog() {
        return dialog.log_expander.is_expanded();
    }
    let _ = window;
    false
}

/// The open dialog's boxes as typed, `(key, value)`, which is what a Test reads and the save writes.
pub fn typed_values() -> Vec<(String, String)> {
    last_dialog()
        .map(|dialog| dialog.typed_all())
        .unwrap_or_default()
}

fn last_dialog() -> Option<Rc<Dialog>> {
    OPEN.with(|open| open.borrow().last().cloned())
}

/// A widget by name inside the open dialog.
fn find_named(name: &str) -> Option<gtk::Widget> {
    let dialog = last_dialog()?;
    find_in(dialog.window.upcast_ref(), name)
}

/// The first widget under `root` carrying `name` — the same walk the window uses.
fn find_in(root: &gtk::Widget, name: &str) -> Option<gtk::Widget> {
    crate::ui::window::find_widget_by_name(root, name)
}

fn buffer_text(view: &gtk::TextView) -> String {
    let buffer = view.buffer();
    let start = buffer.start_iter();
    let end = buffer.end_iter();
    buffer.text(&start, &end, false).to_string()
}
