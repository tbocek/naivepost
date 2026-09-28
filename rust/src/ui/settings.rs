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

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use gtk4 as gtk;

use crate::checks;
use crate::model_list;

/// What a probe hands back for one row: the detail that goes on the ✓, or the reason that goes on ✗.
pub type Answer = Result<String, String>;

/// Who answers a row's probe. `key` is the row's key, `typed` the value read out of its box at the
/// press — the provider is expected to go and ask *that* address, not the one in the file, which
/// still lags the keyboard by [`checks::CONF_SAVE_WAIT`].
pub type Provider = Rc<dyn Fn(&str, &str) -> Answer>;

/// The six kinds of test §5 lists, in the order "Test All" runs them.
const KINDS: [(&str, &str); 6] = [
    ("llm", "LLM"),
    ("llm-vision", "LLM vision"),
    ("ffmpeg", "ffmpeg"),
    ("firefox", "firefox"),
    ("audio", "audio.cpp"),
    ("sd", "sd.cpp"),
];

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
    match phase {
        checks::Phase::Idle => {
            badge.set_text("");
            badge.set_tooltip_text(None::<&str>);
        }
        checks::Phase::Running => {
            badge.set_text("\u{2026}");
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
    entries: RefCell<Vec<(String, gtk::Entry)>>,
    badges: RefCell<Vec<(String, gtk::Label)>>,
    buttons: RefCell<Vec<(String, gtk::Button)>>,
    /// The ids the last Fetch brought back, kept so Use can check its pick against what the server
    /// actually served rather than trusting the widget's memory.
    fetched: RefCell<Vec<String>>,
    list: gtk::DropDown,
    log_view: gtk::TextView,
    log_expander: gtk::Expander,
    test_log: RefCell<checks::TestLog>,
    provider: Provider,
}

impl Dialog {
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
        self.test_log.borrow_mut().push(line.clone(), verdict.is_ok());
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
    fn ask(&self, key: &str) {
        let typed = self.typed(key);
        let answer = (self.provider)(key, &typed);
        let phase = press_test(key, &typed, answer);
        self.report(key, &phase);
    }

    /// Fetch models: ask the server what it serves and fill the dropdown.
    ///
    /// Not a pass/fail service test like the six rows — it discovers an id rather than proving a
    /// server healthy — but it reports through the same badge and log so an empty or unreachable
    /// listing is visible in exactly one place (§5's mirroring rule). The address asked is the one
    /// TYPED, not the one saved: the file lags the keyboard by `checks::CONF_SAVE_WAIT`.
    fn fetch_models(&self) {
        let typed = self.typed("llm");
        match (self.provider)(FETCH_KEY, &typed) {
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
        match model_list::use_choice(&ids, &chosen) {
            Ok(id) => {
                if let Some(entry) = self.entries.borrow().iter().find(|(k, _)| k == "llm").map(|(_, e)| e) {
                    entry.set_text(&id);
                }
                self.report(USE_KEY, &press_test(USE_KEY, &chosen, Ok(format!("Model set to {id}"))));
            }
            Err(reason) => {
                self.report(USE_KEY, &press_test(USE_KEY, &chosen, Err(reason)));
            }
        }
    }

    /// Test All: ask every kind, report each one. Nothing stops at a failure — one early ✗ hiding
    /// the rest would send the user through the dialog ten times.
    fn ask_all(&self) {
        for (key, _) in KINDS {
            self.ask(key);
        }
    }
}

/// Build the dialog. `provider` answers each row; see [`no_probe`] for what a build without one says.
pub fn build(parent: Option<&gtk::Window>, provider: Provider) -> adw::Window {
    build_owned(parent, provider).window.clone()
}

/// The same build, returning the bookkeeping so the caller can keep it live in [`OPEN`].
fn build_owned(parent: Option<&gtk::Window>, provider: Provider) -> Rc<Dialog> {
    let window = adw::Window::new();
    window.set_title(Some("Settings"));
    window.set_modal(true);
    window.set_transient_for(parent);
    window.set_default_width(680);

    let root = gtk::Box::new(gtk::Orientation::Vertical, 12);
    root.set_margin_start(18);
    root.set_margin_end(18);
    root.set_margin_top(12);
    root.set_margin_bottom(12);

    let grid = gtk::Grid::new();
    grid.set_row_spacing(6);
    grid.set_column_spacing(8);
    let mut row = 0;

    // Five columns: section | label | value | badge | Test. The section label appears once per
    // group, on its first row, as the inventory draws it.
    let mut add = |section: Option<&str>,
                  label: &str,
                  entry_key: &str,
                  placeholder: &str,
                  secret: bool,
                  test_key: Option<&str>,
                  tip: &str,
                  row: i32,
                  grid: &gtk::Grid| {
        if let Some(section) = section {
            let title = gtk::Label::new(Some(section));
            title.add_css_class("heading");
            title.set_halign(gtk::Align::Start);
            grid.attach(&title, 0, row, 1, 1);
        }
        let caption = gtk::Label::new(Some(label));
        caption.set_halign(gtk::Align::Start);
        grid.attach(&caption, 1, row, 1, 1);

        let entry = gtk::Entry::new();
        entry.set_widget_name(&format!("entry-{entry_key}"));
        entry.set_placeholder_text(Some(placeholder));
        entry.set_hexpand(true);
        if secret {
            entry.set_visibility(false);
        }
        grid.attach(&entry, 2, row, 2, 1);

        let badge = gtk::Label::new(None);
        badge.set_widget_name(&format!("badge-{entry_key}"));
        badge.set_width_chars(2);
        grid.attach(&badge, 4, row, 1, 1);

        if let Some(test_key) = test_key {
            let test = gtk::Button::with_label("Test");
            test.set_widget_name(&format!("test-{test_key}-button"));
            test.set_tooltip_text(Some(tip));
            grid.attach(&test, 5, row, 1, 1);
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
        "the model above, shown a red square",
        false,
        Some("llm-vision"),
        "Show this model a small sample image and check it names what it sees",
        row,
        &grid,
    );
    row += 1;

    // Fetch models: the list and the copy-into-Model button share the Writing block.
    let fetch = gtk::Button::with_label("Fetch models");
    fetch.set_widget_name("fetch-models-button");
    fetch.set_tooltip_text(Some("List the models this server serves"));
    grid.attach(&fetch, 1, row, 1, 1);
    let list = gtk::DropDown::from_strings(&[]);
    list.set_widget_name("model-list");
    list.set_hexpand(true);
    grid.attach(&list, 2, row, 2, 1);
    let use_choice = gtk::Button::with_label("Use");
    use_choice.set_widget_name("use-model-button");
    use_choice.set_tooltip_text(Some("Copy the chosen id into Model"));
    grid.attach(&use_choice, 4, row, 2, 1);
    // These two verdicts are not `add(...)` rows, so their badges are made here. Without a named
    // badge widget `Dialog::report` has nowhere to paint and a failed listing would show up only in
    // the log — §5 wants the ✓/✗ with its verdict as tooltip on the dialog itself.
    for key in [FETCH_KEY, USE_KEY] {
        let badge = gtk::Label::new(None);
        badge.set_widget_name(&format!("badge-{key}"));
        badge.set_width_chars(2);
        badge.set_valign(gtk::Align::Center);
        // Column 0 is the section-label column and sits empty on this row; the dropdown starts at 2.
        let column = if key == FETCH_KEY { 0 } else { 6 };
        grid.attach(&badge, column, row, 1, 1);
    }
    row += 1;

    // Cutting.
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

    root.append(&grid);

    let all = gtk::Button::with_label("Test All");
    all.set_widget_name("test-all-button");
    all.set_tooltip_text(Some("Run every test above and report each one"));
    root.append(&all);

    let log_expander = gtk::Expander::new(Some("Log"));
    log_expander.set_widget_name("settings-log-expander");
    let log_view = gtk::TextView::new();
    log_view.set_widget_name("settings-log");
    log_view.set_editable(false);
    log_view.set_monospace(true);
    log_view.set_height_request(140);
    log_view.set_vexpand(true);
    log_expander.set_child(Some(&log_view));
    root.append(&log_expander);

    window.set_content(Some(&root));

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
    });
    wire_buttons(&dialog);
    dialog
}
/// Each `test-<key>-button` gets the row whose entry shares its key.
fn wire_buttons(dialog: &Rc<Dialog>) {
    for key in test_keys() {
        let Some(button) = find_in(dialog.window.upcast_ref(), &format!("test-{key}-button")).and_then(|w| w.downcast::<gtk::Button>().ok()) else {
            continue;
        };
        let Some(entry) = find_in(dialog.window.upcast_ref(), &format!("entry-{key}")).and_then(|w| w.downcast::<gtk::Entry>().ok()) else {
            continue;
        };
        let Some(badge) = find_in(dialog.window.upcast_ref(), &format!("badge-{key}")).and_then(|w| w.downcast::<gtk::Label>().ok()) else {
            continue;
        };
        dialog.entries.borrow_mut().push((key.to_string(), entry));
        dialog.badges.borrow_mut().push((key.to_string(), badge));
        dialog.buttons.borrow_mut().push((key.to_string(), button.clone()));

        let owned = Rc::clone(dialog);
        let key = key.to_string();
        button.connect_clicked(move |_| owned.ask(&key));
    }

    if let Some(all) = find_in(dialog.window.upcast_ref(), "test-all-button").and_then(|w| w.downcast::<gtk::Button>().ok()) {
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

/// Every key that has a Test button, in dialog order.
fn test_keys() -> [&'static str; 11] {
    [
        "llm",
        "llm-vision",
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
    Rc::new(|key, _| Err(format!("nothing wired to probe {key} -- run this in a real session")))
}

/// Open the dialog over `parent`, answering from `provider`.
pub fn open_with(parent: &adw::ApplicationWindow, provider: Provider) -> adw::Window {
    let dialog = build_owned(Some(parent.upcast_ref()), provider);
    OPEN.with(|open| open.borrow_mut().push(Rc::clone(&dialog)));
    dialog.window.present();
    dialog.window.clone()
}

/// Whether a settings dialog is open this session.
pub fn dialog_open(_window: &adw::ApplicationWindow) -> bool {
    OPEN.with(|open| !open.borrow().is_empty())
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
    let Some(model) = list.model() else { return Vec::new() };
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

/// The dialog's log text as it stands in its own view.
fn buffer_text(view: &gtk::TextView) -> String {
    let buffer = view.buffer();
    let start = buffer.start_iter();
    let end = buffer.end_iter();
    buffer.text(&start, &end, false).to_string()
}
