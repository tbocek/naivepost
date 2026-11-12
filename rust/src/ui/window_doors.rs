//! §1's widget doors — the named lookups, readouts and tooltip readers every other module and test asks
//! the page for. Split out of `window.rs`, which is past its size budget; every name is re-exported from
//! `window` under the path callers already use.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use gtk4 as gtk;

use crate::project::MarkingPass;
use crate::project::Project;
use crate::runqueue;
use crate::shell::Page;
use crate::ui::window::{depth, has_label, tab_buttons, UiState};
use crate::ui::window_page_state::{WindowPage, PAGE as WINDOW_PAGE};

/// The switching state as the window currently shows it — for the tests that drive a real click.
pub fn state(window: &adw::ApplicationWindow) -> UiState {
    let (Some(switcher), Some(status)) = (
        find_switcher(window.upcast_ref()),
        find_status(window.upcast_ref()),
    ) else {
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
    tab_button(window, page)?
        .tooltip_text()
        .map(|text| text.to_string())
}
/// Whether the tab for `page` is greyed (§1's "greyed, not disabled" — the class, never `set_sensitive`).
pub fn tab_dimmed(window: &adw::ApplicationWindow, page: Page) -> bool {
    tab_button(window, page).is_some_and(|button| button.has_css_class("dim-label"))
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
/// (`Outputs: Prepare: 5 files, 1.4 MB`).
pub fn outputs_sentence(window: &adw::ApplicationWindow) -> Option<String> {
    let word = outputs_label(window)?.text();
    Some(format!("{word} {}", outputs_readout(window)?.text()))
}
/// Mark this window's narration write as owed — the state a half-typed line leaves behind.
pub fn mark_narration_owed() {
    if let Some(shell) = WINDOW_PAGE.with(
        |WindowPage {
             window_shells: shells,
             ..
         }| shells.borrow().last().cloned(),
    ) {
        shell
            .borrow_mut()
            .narration_pending
            .touched(std::time::Duration::from_secs(0));
    }
}

/// Build §1 badge **4** — the header's project name/path label, packed after Save — publish
/// it as this window's title label, and draw it once so a freshly built window never opens blank.
pub fn install_project_title(header: &adw::HeaderBar) {
    let label = super::header_title::install(header);
    push_project_title(&label);
    paint_project_title();
}

/// Re-draw §1 badge **4** from the folder the window believes is open and the page its shell shows.
pub fn paint_project_title() {
    let Some(title) = project_title_of() else {
        return;
    };
    let dir = crate::startup::open_dir_if_set().unwrap_or_else(crate::startup::open_dir);
    let tab = WINDOW_PAGE
        .with(
            |WindowPage {
                 window_shells: shells,
                 ..
             }| shells.borrow().last().cloned(),
        )
        .map(|shell| shell.borrow().page.label().to_string())
        .unwrap_or_default();
    super::header_title::paint(&title, &dir, &tab);
}

/// The same read, for a painter that has no window handle (the shared readout repaint in
/// `window_bench::paint_readouts`).
pub fn project_title_of() -> Option<gtk::Label> {
    WINDOW_PAGE.with(
        |WindowPage {
             header_titles: titles,
             ..
         }| titles.borrow().last().cloned(),
    )
}

fn push_project_title(label: &gtk::Label) {
    WINDOW_PAGE.with(
        |WindowPage {
             header_titles: titles,
             ..
         }| titles.borrow_mut().push(label.clone()),
    );
}

/// §1 badge **6** — the header ⓘ (not the per-page copies), newest slot wins so a test reads the
/// button this window built rather than one left over from an earlier window in the same process.
pub fn tab_info_button() -> Option<gtk::Button> {
    WINDOW_PAGE.with(
        |WindowPage {
             info_button: slots, ..
         }| slots.borrow().last().cloned(),
    )
}
/// The §1 header ⓘ's tooltip — the door a test reads instead of matching a raw glyph.
pub fn tab_info_parts() -> Option<(String, String)> {
    let tip = tab_info_button()?.tooltip_text()?.to_string();
    let join = tip.find(" \u{2014} ")?;
    Some((
        tip[..join].to_string(),
        tip[join + " \u{2014} ".len()..].to_string(),
    ))
}
/// The switcher inside the window: it is the one thing in here with a stack of its own.
pub(crate) fn find_switcher(root: &gtk::Widget) -> Option<adw::ViewSwitcher> {
    if let Some(switcher) = root.downcast_ref::<adw::ViewSwitcher>() {
        return Some(switcher.clone());
    }
    for child in root.observe_children().iter::<glib::Object>() {
        let Ok(child) = child else { continue };
        let Ok(widget) = child.downcast::<gtk::Widget>() else {
            continue;
        };
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
    save_button(window)?
        .tooltip_text()
        .map(|text| text.to_string())
}
/// The Rescan button, named so a test can press it the way a user does.
pub fn rescan_button(window: &adw::ApplicationWindow) -> Option<gtk::Button> {
    find_widget_by_name(window.upcast_ref(), "rescan-button")?
        .downcast()
        .ok()
}
/// One widget from Prepare's sources list, found by the name §4's rows are built with — `sources-list`,
/// `source-row-<i>`, `footage-<i>`, `narrator-<i>`, `remove-<i>`, `source-name-<i>`.
pub fn find_source_widget(window: &adw::ApplicationWindow, name: &str) -> Option<gtk::Widget> {
    find_widget_by_name(window.upcast_ref(), name)
}
/// The Open button (F0.9), found the same way as [`play_button`].
pub fn open_button(window: &adw::ApplicationWindow) -> Option<gtk::Button> {
    find_widget_by_name(window.upcast_ref(), "open-button")?
        .downcast()
        .ok()
}
/// The sources of the project this window currently holds.
pub fn session_handle() -> Rc<RefCell<Project>> {
    WINDOW_PAGE
        .with(|WindowPage { session: slots, .. }| slots.borrow().last().cloned())
        .expect("the window has a session")
}
pub fn session_sources() -> Vec<String> {
    WINDOW_PAGE.with(|WindowPage { session: slots, .. }| {
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
/// rather than only the painted fraction.
pub fn run_queue(_window: &adw::ApplicationWindow) -> Option<Rc<RefCell<runqueue::Queue>>> {
    WINDOW_PAGE.with(
        |WindowPage {
             window_queue: slots,
             ..
         }| slots.borrow().last().cloned(),
    )
}
/// The run's progress bar (F0.5 S2), found the same way as [`play_button`].
pub fn progress_bar(window: &adw::ApplicationWindow) -> Option<gtk::ProgressBar> {
    find_widget_by_name(window.upcast_ref(), "run-progress")?
        .downcast()
        .ok()
}
/// The log expander (F0.5 S1), whose header carries the status line and whose body holds the run's
/// lines.
pub fn log_expander(window: &adw::ApplicationWindow) -> Option<gtk::Expander> {
    find_widget_by_name(window.upcast_ref(), "log-expander")?
        .downcast()
        .ok()
}
/// The tooltip the progress bar carries — where F0.5 puts the counting ("task 4 of 12, 8 waiting").
pub fn progress_tooltip(window: &adw::ApplicationWindow) -> Option<String> {
    progress_bar(window)?
        .tooltip_text()
        .map(|text| text.to_string())
}
/// The tooltip Rescan carries, which is how a test reads §1's wording for badge **8**.
pub fn rescan_tooltip(window: &adw::ApplicationWindow) -> Option<String> {
    rescan_button(window)?
        .tooltip_text()
        .map(|text| text.to_string())
}
/// Prepare's "Add source files…" button, named so a test can press it the way a user does.
pub fn add_sources_button(window: &adw::ApplicationWindow) -> Option<gtk::Button> {
    find_widget_by_name(window.upcast_ref(), "add-sources-button")?
        .downcast()
        .ok()
}
/// The step the run bar is holding, if any — read off `RunBar::running` rather than off a button's
/// face.
pub fn running_step() -> Option<crate::run::Step> {
    WINDOW_PAGE.with(|WindowPage { run_bars: bars, .. }| {
        bars.borrow()
            .last()
            .and_then(|bar| bar.borrow().running.as_ref().map(|run| run.step))
    })
}
/// The marking pass the LIVE session runs under — the same field `▶` reads when it decides whether
/// F1.9's retake pass or F1.10's joins pass speaks.
pub fn marking_pass(_window: &adw::ApplicationWindow) -> MarkingPass {
    WINDOW_PAGE
        .with(
            |WindowPage {
                 play_session: slots,
                 ..
             }| slots.borrow().last().cloned(),
        )
        .map(|session| session.borrow().policy.marking_pass.value)
        .unwrap_or(MarkingPass::Retakes)
}
/// Move the live session's marking pass, as saving a project that names Joins would.
pub fn set_marking_pass(pass: MarkingPass) {
    if let Some(session) = WINDOW_PAGE.with(
        |WindowPage {
             play_session: slots,
             ..
         }| slots.borrow().last().cloned(),
    ) {
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
    new_button(window)?
        .tooltip_text()
        .map(|text| text.to_string())
}
/// The ▶ button, named rather than searched for by icon: the icon changes to ⏸ while a run works,
/// and an unnamed busy button would then be findable no more.
pub fn play_button(window: &adw::ApplicationWindow) -> Option<gtk::Button> {
    find_widget_by_name(window.upcast_ref(), "play-button")?
        .downcast()
        .ok()
}
/// The ⏹ button (F0.3), found the same way as [`play_button`].
pub fn stop_button(window: &adw::ApplicationWindow) -> Option<gtk::Button> {
    find_widget_by_name(window.upcast_ref(), "stop-button")?
        .downcast()
        .ok()
}
/// The tooltip ▶ carries, which is how a test reads §2's wording without reaching into the button.
pub fn play_tooltip(window: &adw::ApplicationWindow) -> Option<String> {
    play_button(window)?
        .tooltip_text()
        .map(|text| text.to_string())
}
/// The first widget under `root` carrying `name`, found the same walk as [`find_status`].
pub(crate) fn find_widget_by_name(root: &gtk::Widget, name: &str) -> Option<gtk::Widget> {
    if root.widget_name() == name {
        return Some(root.clone());
    }
    for child in root.observe_children().iter::<glib::Object>() {
        let Ok(child) = child else { continue };
        let Ok(widget) = child.downcast::<gtk::Widget>() else {
            continue;
        };
        if let Some(found) = find_widget_by_name(&widget, name) {
            return Some(found);
        }
    }
    None
}
/// The window's page stack, so a flow that moved the shell can move what is on screen too.
pub fn view_stack(window: &adw::ApplicationWindow) -> Option<adw::ViewStack> {
    fn find(node: &gtk::Widget) -> Option<adw::ViewStack> {
        if let Some(stack) = node.downcast_ref::<adw::ViewStack>() {
            return Some(stack.clone());
        }
        for child in node
            .observe_children()
            .iter::<glib::Object>()
            .filter_map(Result::ok)
        {
            let Ok(child) = child.downcast::<gtk::Widget>() else {
                continue;
            };
            if let Some(stack) = find(&child) {
                return Some(stack);
            }
        }
        None
    }
    find(window.upcast_ref())
}

/// The newest window's re-entrancy latch (`wire_switching`'s own), so a flow that moves the stack
/// itself can hold off the notify handler exactly as a bounce does.
pub fn switch_latch() -> Option<Rc<std::cell::Cell<bool>>> {
    WINDOW_PAGE.with(
        |WindowPage {
             window_switch_latch: latch,
             ..
         }| latch.borrow().clone(),
    )
}

/// The status line, named rather than searched for by text: an empty label is the ordinary state and
/// would match any label in the tree.
pub fn find_status(root: &gtk::Widget) -> Option<gtk::Label> {
    if root.widget_name() == "status-line" {
        return root.downcast_ref::<gtk::Label>().cloned();
    }
    for child in root.observe_children().iter::<glib::Object>() {
        let Ok(child) = child else { continue };
        let Ok(widget) = child.downcast::<gtk::Widget>() else {
            continue;
        };
        if let Some(found) = find_status(&widget) {
            return Some(found);
        }
    }
    None
}
