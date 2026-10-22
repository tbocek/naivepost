//! §1's widget doors — the named lookups, readouts and tooltip readers every other module and test asks
//! the page for. Split out of `window.rs`, which is past its size budget; every name is re-exported from
//! `window` under the path callers already use.
//!
//! These resolve through the window's own content, so a closed window's surviving tree cannot answer for
//! a live one. No decisions: each reads a widget by its stable name or a slot this window published.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use gtk4 as gtk;

use crate::project::MarkingPass;
use crate::runqueue;
use crate::project::Project;
use crate::shell::Page;
use crate::ui::window::{depth, has_label, tab_buttons, UiState};
use crate::ui::window as ui;

/// The switching state as the window currently shows it — for the tests that drive a real click.
pub fn state(window: &adw::ApplicationWindow) -> UiState {
    let (Some(switcher), Some(status)) = (find_switcher(window.upcast_ref()), find_status(window.upcast_ref()))
    else {
        panic!("the window has no tab row");
    };
    let page = switcher
        .stack()
        .and_then(|stack| stack.visible_child_name().map(|name| name.to_string()))
        .unwrap_or_default();
    UiState {
        page: Page::all()
            .into_iter()
            .find(|candidate| candidate.label() == page)
            .unwrap_or(Page::Prepare),
        status: status.text().to_string(),
        log_expanded: log_expander(window).is_some_and(|expander| expander.is_expanded()),
        progress: progress_bar(window)
            .map(|bar| bar.fraction())
            .unwrap_or(0.0),
    }
}
/// The tooltip the tab button for `page` carries, which is how a test reads §1's "tooltip = the
/// reason" without reaching inside the switcher itself.
pub fn tab_tooltip(window: &adw::ApplicationWindow, page: Page) -> Option<String> {
    tab_button(window, page)?.tooltip_text().map(|text| text.to_string())
}
/// Whether the tab for `page` is greyed (§1's "greyed, not disabled" — the class, never `set_sensitive`).
pub fn tab_dimmed(window: &adw::ApplicationWindow, page: Page) -> bool {
    tab_button(window, page)
        .is_some_and(|button| button.has_css_class("dim-label"))
}
/// The tab button drawing `page`'s word, so a test can click it the way a user does.
pub fn tab_button(window: &adw::ApplicationWindow, page: Page) -> Option<gtk::Button> {
    let switcher = find_switcher(window.upcast_ref())?;
    tab_buttons(switcher.upcast_ref())
        .into_iter()
        .filter(|button| has_label(button, page.label()))
        .max_by_key(depth)?
        .downcast()
        .ok()
}
/// §1's badge **14** — the visible tab's `Inputs:` readout, by its stable name.
pub fn inputs_readout(window: &adw::ApplicationWindow) -> Option<gtk::Label> {
    find_widget_by_name(window.upcast_ref(), "inputs-readout")?
        .downcast()
        .ok()
}
/// §1's badge **15** — the `Outputs:` count beside its folder button.
pub fn outputs_readout(window: &adw::ApplicationWindow) -> Option<gtk::Label> {
    find_widget_by_name(window.upcast_ref(), "outputs-readout")?
        .downcast()
        .ok()
}
/// §1's badge **15** — the word "Outputs:" that leads it, its own label in front of the folder button.
pub fn outputs_label(window: &adw::ApplicationWindow) -> Option<gtk::Label> {
    find_widget_by_name(window.upcast_ref(), "outputs-label")?
        .downcast()
        .ok()
}
/// §1's badge **15** — the folder button that goes with the Outputs count.
pub fn outputs_folder_button(window: &adw::ApplicationWindow) -> Option<gtk::Button> {
    find_widget_by_name(window.upcast_ref(), "outputs-folder-button")?
        .downcast()
        .ok()
}
/// §1's badge **15** as a whole: the `Outputs:` label, the folder button (which draws no word of its own) and
/// the count after it, read back as ONE sentence so a caller sees the row the way it is drawn
/// (`Outputs: Prepare: 5 files, 1.4 MB`). The sentence is the label's text and the count joined by the space
/// the button's own edge supplies.
pub fn outputs_sentence(window: &adw::ApplicationWindow) -> Option<String> {
    let word = outputs_label(window)?.text();
    Some(format!("{word} {}", outputs_readout(window)?.text()))
}
/// Mark this window's narration write as owed — the state a half-typed line leaves behind.
///
/// S3 owns the decision whether leaving the tab writes (`shell::Pending::owe`); this only raises
/// the flag, through the same type the switch reads. The Narrate page's row boxes raise it from their
/// own `changed` handler (`crate::ui::narrate_text::note_typing`), which is what makes typing on the
/// page owe a write rather than only a test being able to.
pub fn mark_narration_owed(window: &adw::ApplicationWindow) {
    let _ = window;
    if let Some(shell) = ui::WINDOW_SHELLS.with(|shells| shells.borrow().last().cloned()) {
        // `now` is irrelevant here: leaving the tab writes whatever is owed, even a beat early.
        shell.borrow_mut().narration_pending.touched(std::time::Duration::from_secs(0));
    }
}
/// §1 badge **4** — the header's project name/path label, so a caller (a switch, an Open, a Save that
/// renamed the folder) can redraw it without reaching into the header tree. Newest slot wins, the same
/// rule every per-window door here uses: a process that built two windows shows the newest one's label.
/// The header's project label, by its window. Re-exported where every other §1 door lives.
pub fn project_title(window: &adw::ApplicationWindow) -> Option<gtk::Label> {
    let _ = window;
    project_title_of()
}

/// Build §1 badge **4** — the header's project name/path label, packed after Save — publish
/// it as this window's title label, and draw it once so a freshly built window never opens blank. Every
/// later repaint goes through [`paint_project_title`], which `window_bench::paint_readouts` calls beside
/// the readouts on every switch.
pub fn install_project_title(header: &adw::HeaderBar) {
    let label = super::header_title::install(header);
    push_project_title(&label);
    paint_project_title();
}

/// Re-draw §1 badge **4** from the folder the window believes is open and the page its shell shows.
/// Read through the same two slots every other door here uses, so a flow that moved the session or
/// switched the page has nothing left to remember to do. With no folder published yet (a window built
/// before any flow took one on) the launch's own session folder answers — the same fallback `save_target`
/// uses — so the header never opens blank.
pub fn paint_project_title() {
    let Some(title) = project_title_of() else { return };
    let dir = crate::ui::save_apply::open_dir()
        .unwrap_or_else(|| crate::startup::session_dir(&std::env::current_dir().unwrap_or_default()));
    let tab = ui::WINDOW_SHELLS
        .with(|shells| shells.borrow().last().cloned())
        .map(|shell| shell.borrow().page.label().to_string())
        .unwrap_or_default();
    super::header_title::paint(&title, &dir, &tab);
}

/// The same read, for a painter that has no window handle (the shared readout repaint in
/// `window_bench::paint_readouts`).
pub(crate) fn project_title_of() -> Option<gtk::Label> {
    HEADER_TITLES.with(|titles| titles.borrow().last().cloned())
}

/// Publish this window's header label. Private to this module: only [`install_project_title`] hands one out.
fn push_project_title(label: &gtk::Label) {
    HEADER_TITLES.with(|titles| titles.borrow_mut().push(label.clone()));
}

thread_local! {
    static HEADER_TITLES: RefCell<Vec<gtk::Label>> = const { RefCell::new(Vec::new()) };
}

/// §1 badge **6** — the header ⓘ (not the per-page copies), newest slot wins so a test reads the
/// button this window built rather than one left over from an earlier window in the same process.
pub fn tab_info_button(window: &adw::ApplicationWindow) -> Option<gtk::Button> {
    let _ = window;
    ui::INFO_BUTTON.with(|slots| slots.borrow().last().cloned())
}
/// The §1 header ⓘ's tooltip — the door a test reads instead of matching a raw glyph. §1: the ⓘ's
/// tooltip is "current tab's label + help text", which is `Shell::info_tip`'s sentence joined by that
/// dash. `None` when the button carries no tooltip at all.
pub fn tab_info_parts(window: &adw::ApplicationWindow) -> Option<(String, String)> {
    let tip = tab_info_button(window)?.tooltip_text()?.to_string();
    let join = tip.find(" \u{2014} ")?;
    Some((tip[..join].to_string(), tip[join + " \u{2014} ".len()..].to_string()))
}
/// The switcher inside the window: it is the one thing in here with a stack of its own.
pub(crate) fn find_switcher(root: &gtk::Widget) -> Option<adw::ViewSwitcher> {
    if let Some(switcher) = root.downcast_ref::<adw::ViewSwitcher>() {
        return Some(switcher.clone());
    }
    for child in root.observe_children().iter::<glib::Object>() {
        let Ok(child) = child else { continue };
        let Ok(widget) = child.downcast::<gtk::Widget>() else { continue };
        if let Some(found) = find_switcher(&widget) {
            return Some(found);
        }
    }
    None
}
/// The Save button, named so a test can press it the way a user does.
pub fn save_button(window: &adw::ApplicationWindow) -> Option<gtk::Button> {
    find_widget_by_name(window.upcast_ref(), "save-button")?
        .downcast()
        .ok()
}
/// The tooltip Save carries, which is how a test reads §1's wording for badge **3**.
pub fn save_tooltip(window: &adw::ApplicationWindow) -> Option<String> {
    save_button(window)?.tooltip_text().map(|text| text.to_string())
}
/// The Rescan button, named so a test can press it the way a user does.
pub fn rescan_button(window: &adw::ApplicationWindow) -> Option<gtk::Button> {
    find_widget_by_name(window.upcast_ref(), "rescan-button")?
        .downcast()
        .ok()
}
/// One widget from Prepare's sources list, found by the name §4's rows are built with — `sources-list`,
/// `source-row-<i>`, `footage-<i>`, `narrator-<i>`, `remove-<i>`, `source-name-<i>`. One accessor for
/// the whole list rather than six, because the row count is the project's business: a test asking for
/// `remove-7` of a two-row session gets `None`, which is the honest answer.
pub fn find_source_widget(window: &adw::ApplicationWindow, name: &str) -> Option<gtk::Widget> {
    find_widget_by_name(window.upcast_ref(), name)
}
/// The Open button (F0.9), found the same way as [`play_button`].
pub fn open_button(window: &adw::ApplicationWindow) -> Option<gtk::Button> {
    find_widget_by_name(window.upcast_ref(), "open-button")?
        .downcast()
        .ok()
}
/// The sources of the project this window currently holds. `build_window` keeps its own `Rc`, so the
/// handle is published here at build time for a test to read back — the same copy the Prepare page's
/// rows mutate, not a snapshot of it.
/// The live session — the last window's handle, which is the one this check just opened into. Each
/// `build_window` publishes its own, so reading the newest avoids mixing in earlier windows' copies.
/// The live session handle itself, newest-slot like [`session_sources`]. F0.8's apply needs the `Rc`,
/// not a read of it: swapping this exact copy is what every page sees.
pub fn session_handle(window: &adw::ApplicationWindow) -> Rc<RefCell<Project>> {
    let _ = window;
    ui::SESSION.with(|slots| slots.borrow().last().cloned()).expect("the window has a session")
}
pub fn session_sources(window: &adw::ApplicationWindow) -> Vec<String> {
    let _ = window;
    ui::SESSION.with(|slots| {
        slots
            .borrow()
            .last()
            .map(|project| {
                project
                    .borrow()
                    .sources
                    .iter()
                    .map(|source| source.path.clone())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    })
}
/// The run bar's queue (F0.5) this window shows, so a test reads the state `drive_page` writes
/// rather than only the painted fraction. Newest slot wins, as every published handle here does.
pub fn run_queue(_window: &adw::ApplicationWindow) -> Option<Rc<RefCell<runqueue::Queue>>> {
    ui::WINDOW_QUEUE.with(|slots| slots.borrow().last().cloned())
}
/// The tooltip Open carries, which is how a test reads §1's wording for badge **2**.
pub fn open_tooltip(window: &adw::ApplicationWindow) -> Option<String> {
    open_button(window)?.tooltip_text().map(|text| text.to_string())
}
/// The run's progress bar (F0.5 S2), found the same way as [`play_button`]. Window-level, so a name
/// lookup cannot land on a per-page copy.
pub fn progress_bar(window: &adw::ApplicationWindow) -> Option<gtk::ProgressBar> {
    find_widget_by_name(window.upcast_ref(), "run-progress")?
        .downcast()
        .ok()
}
/// The log expander (F0.5 S1), whose header carries the status line and whose body holds the run's
/// lines. `is_expanded()` is what `state().log_expanded` reports.
pub fn log_expander(window: &adw::ApplicationWindow) -> Option<gtk::Expander> {
    find_widget_by_name(window.upcast_ref(), "log-expander")?
        .downcast()
        .ok()
}
/// The tooltip the progress bar carries — where F0.5 puts the counting ("task 4 of 12, 8 waiting").
pub fn progress_tooltip(window: &adw::ApplicationWindow) -> Option<String> {
    progress_bar(window)?.tooltip_text().map(|text| text.to_string())
}
/// The tooltip Rescan carries, which is how a test reads §1's wording for badge **8**.
pub fn rescan_tooltip(window: &adw::ApplicationWindow) -> Option<String> {
    rescan_button(window)?.tooltip_text().map(|text| text.to_string())
}
/// Prepare's "Add source files…" button, named so a test can press it the way a user does.
pub fn add_sources_button(window: &adw::ApplicationWindow) -> Option<gtk::Button> {
    find_widget_by_name(window.upcast_ref(), "add-sources-button")?
        .downcast()
        .ok()
}
/// The "copy into project" tick beside it (F0.12 S2), which is what the press reads to decide
/// between a copy and a reference.
pub fn copy_into_project(window: &adw::ApplicationWindow) -> Option<gtk::CheckButton> {
    find_widget_by_name(window.upcast_ref(), "copy-into-project")?
        .downcast()
        .ok()
}
/// The ASR code the page's Language box holds (§1's badge **10**) — how a test sees that typing
/// reached [`Project::language`] rather than stopping at the widget.
pub fn bench_language(window: &adw::ApplicationWindow) -> Option<String> {
    let entry = find_widget_by_name(window.upcast_ref(), "language-entry")?
        .downcast::<gtk::Entry>()
        .ok()?;
    Some(entry.text().to_string())
}
/// Freq as the live session holds it, read off `Project::interval` rather than off the dropdown's
/// selected index. A widget test needs the SAME field `prepare::set_freq` writes, otherwise it
/// would only prove the dropdown remembers its own selection.
pub fn session_freq(window: &adw::ApplicationWindow) -> f64 {
    let _ = window;
    ui::SESSION.with(|slots| {
        slots
            .borrow()
            .last()
            .map(|project| project.borrow().interval)
            .unwrap_or(naivepost_interval_default())
    })
}
/// The step the run bar is holding, if any — read off `RunBar::running` rather than off a button's
/// face. A widget test needs the same field `runqueue::start_run` writes and F1.1's refusal skips,
/// otherwise it would only prove that ⏹ looked sensitive.
pub fn running_step(window: &adw::ApplicationWindow) -> Option<crate::run::Step> {
    let _ = window;
    ui::RUN_BARS.with(|bars| {
        bars.borrow()
            .last()
            .and_then(|bar| bar.borrow().running.as_ref().map(|run| run.step))
    })
}
/// What a project with no opinion carries, reached through the type rather than retyped here so the
/// two cannot drift apart.
fn naivepost_interval_default() -> f64 {
    crate::project::Project::default().interval
}
/// The marking pass the LIVE session runs under — the same field `▶` reads when it decides whether
/// F1.9's retake pass or F1.10's joins pass speaks. Read from `ui::PLAY_SESSION`, not from a window
/// widget: the policy has no control of its own on this page (it lives in the gear), so the session
/// is the truth and a test that read anything else would be reading a copy.
pub fn marking_pass(_window: &adw::ApplicationWindow) -> MarkingPass {
    ui::PLAY_SESSION
        .with(|slots| slots.borrow().last().cloned())
        .map(|session| session.borrow().policy.marking_pass.value)
        .unwrap_or(MarkingPass::Retakes)
}
/// Move the live session's marking pass, as saving a project that names Joins would.
///
/// This is the seam a person's saved project arrives through: `build_window` publishes the argument
/// into `ui::PLAY_SESSION`, and F0.7's derive door may then replace it. A test that wants to know which
/// pass a press will run changes it HERE, after the window exists, because changing the build-time
/// argument can be overwritten before the handler ever sees it.
pub fn set_marking_pass(window: &adw::ApplicationWindow, pass: MarkingPass) {
    let _ = window;
    if let Some(session) = ui::PLAY_SESSION.with(|slots| slots.borrow().last().cloned()) {
        session.borrow_mut().policy.marking_pass.value = pass;
    }
}
/// The New button, named so a test can press it the way a user does.
pub fn new_button(window: &adw::ApplicationWindow) -> Option<gtk::Button> {
    find_widget_by_name(window.upcast_ref(), "new-button")?
        .downcast()
        .ok()
}
/// The tooltip ＋ New carries, which is how a test reads §1's wording for badge **1**.
pub fn new_tooltip(window: &adw::ApplicationWindow) -> Option<String> {
    new_button(window)?.tooltip_text().map(|text| text.to_string())
}
/// The ▶ button, named rather than searched for by icon: the icon changes to ⏸ while a run works,
/// and an unnamed busy button would then be findable no more.
pub fn play_button(window: &adw::ApplicationWindow) -> Option<gtk::Button> {
    find_widget_by_name(window.upcast_ref(), "play-button")?.downcast().ok()
}
/// The ⏹ button (F0.3), found the same way as [`play_button`].
pub fn stop_button(window: &adw::ApplicationWindow) -> Option<gtk::Button> {
    find_widget_by_name(window.upcast_ref(), "stop-button")?.downcast().ok()
}
/// The "I'm feeling lucky" button (F0.4), found the same way as [`play_button`].
pub fn lucky_button(window: &adw::ApplicationWindow) -> Option<gtk::Button> {
    find_widget_by_name(window.upcast_ref(), "lucky-button")?.downcast().ok()
}
/// The tooltip ▶ carries, which is how a test reads §2's wording without reaching into the button.
pub fn play_tooltip(window: &adw::ApplicationWindow) -> Option<String> {
    play_button(window)?.tooltip_text().map(|text| text.to_string())
}
/// The first widget under `root` carrying `name`, found the same walk as [`find_status`].
pub(crate) fn find_widget_by_name(root: &gtk::Widget, name: &str) -> Option<gtk::Widget> {
    if root.widget_name() == name {
        return Some(root.clone());
    }
    for child in root.observe_children().iter::<glib::Object>() {
        let Ok(child) = child else { continue };
        let Ok(widget) = child.downcast::<gtk::Widget>() else { continue };
        if let Some(found) = find_widget_by_name(&widget, name) {
            return Some(found);
        }
    }
    None
}
/// The window's page stack, so a flow that moved the shell can move what is on screen too. Found by
/// type rather than by name: the stack is the divider's top half and carries no widget name of its own.
pub fn view_stack(window: &adw::ApplicationWindow) -> Option<adw::ViewStack> {
    fn find(node: &gtk::Widget) -> Option<adw::ViewStack> {
        if let Some(stack) = node.downcast_ref::<adw::ViewStack>() {
            return Some(stack.clone());
        }
        for child in node.observe_children().iter::<glib::Object>().filter_map(Result::ok) {
            let Ok(child) = child.downcast::<gtk::Widget>() else { continue };
            if let Some(stack) = find(&child) {
                return Some(stack);
            }
        }
        None
    }
    find(window.upcast_ref())
}

/// The newest window's re-entrancy latch (`wire_switching`'s own), so a flow that moves the stack
/// itself can hold off the notify handler exactly as a bounce does. `None` only before any window exists.
pub fn switch_latch() -> Option<Rc<std::cell::Cell<bool>>> {
    ui::WINDOW_SWITCH_LATCH.with(|latch| latch.borrow().clone())
}

/// The status line, named rather than searched for by text: an empty label is the ordinary state and
/// would match any label in the tree. Public so a widget test can read the sentence a press printed —
/// the page's one output channel, and the thing F2.7 pins.
pub fn find_status(root: &gtk::Widget) -> Option<gtk::Label> {
    if root.widget_name() == "status-line" {
        return root.downcast_ref::<gtk::Label>().cloned();
    }
    for child in root.observe_children().iter::<glib::Object>() {
        let Ok(child) = child else { continue };
        let Ok(widget) = child.downcast::<gtk::Widget>() else { continue };
        if let Some(found) = find_status(&widget) {
            return Some(found);
        }
    }
    None
}
