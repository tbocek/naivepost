//! F0.15's button: ↺ beside ▶ and ⏹. [`crate::reset`] decides what goes; this asks first, removes on a worker
//! (a project's frames are tens of thousands of files) and refreshes every page the way ⟳ does.

use adw::prelude::*;
use gtk4 as gtk;

use crate::layout::Tree;
use crate::reset;
use crate::ui::window;

thread_local! {
    /// The question open now, so a test can answer it as a person would.
    static OPEN_QUESTION: std::cell::RefCell<Option<adw::MessageDialog>> =
        const { std::cell::RefCell::new(None) };
}

/// The question open now, if any.
pub fn open_question() -> Option<adw::MessageDialog> {
    OPEN_QUESTION.with(|held| held.borrow().clone())
}

/// ↺, its tooltip and its press.
pub(crate) fn button(window: &adw::ApplicationWindow) -> gtk::Button {
    let reset_ = gtk::Button::from_icon_name("edit-clear-all-symbolic");
    reset_.set_widget_name("reset-button");
    reset_.set_tooltip_text(Some(
        "Reset this step: delete what it generated, so it can be done again",
    ));
    let window = window.clone();
    reset_.connect_clicked(move |_| ask(&window));
    reset_
}

/// S1-S2: nothing moves under a run, nothing is asked when there is nothing to remove, and nothing goes unasked.
fn ask(window: &adw::ApplicationWindow) {
    let say = |text: &str| {
        if let Some(status) = crate::ui::find_status(window.upcast_ref()) {
            status.set_text(text);
        }
    };
    if window::running_step(window).is_some() || crate::ui::produce_worker::busy() {
        say("stop the run first");
        return;
    }
    let page = window::state(window).page;
    let Ok(tree) = Tree::new(crate::startup::open_dir()) else {
        say("no project to reset");
        return;
    };
    let paths = reset::plan(&tree, page);
    if paths.is_empty() {
        say(&format!("{} has nothing to reset", page.label()));
        return;
    }
    let dialog = adw::MessageDialog::new(
        Some(window),
        Some(&format!("Reset {}?", page.label())),
        Some(reset::what_goes(page)),
    );
    dialog.set_widget_name("reset-dialog");
    dialog.add_response("cancel", "Cancel");
    dialog.add_response("reset", "Reset");
    dialog.set_response_appearance("reset", adw::ResponseAppearance::Destructive);
    // Closing without saying is not a yes.
    dialog.set_default_response(Some("cancel"));
    dialog.set_close_response("cancel");
    let main = window.clone();
    dialog.connect_response(None, move |_, answer| {
        OPEN_QUESTION.with(|held| *held.borrow_mut() = None);
        if answer == "reset" {
            remove(&main, tree.clone(), paths.clone(), page);
        }
    });
    OPEN_QUESTION.with(|held| *held.borrow_mut() = Some(dialog.clone()));
    dialog.present();
}

/// S3-S4: the removal on a worker, then every page re-read from what is left, through ⟳'s own door.
fn remove(
    window: &adw::ApplicationWindow,
    tree: Tree,
    paths: Vec<std::path::PathBuf>,
    page: crate::shell::Page,
) {
    let main = window.clone();
    crate::ui::background::run(
        "reset",
        move || reset::remove(&tree, &paths),
        move |removed| {
            window::log_line(&format!(
                ">>> reset {}: removed {}",
                page.label(),
                removed.gone.join(", ")
            ));
            for (path, error) in &removed.failed {
                window::log_line(&format!("!!! reset {}: {path}: {error}", page.label()));
            }
            if let Some(rescan) = window::rescan_button(&main) {
                rescan.emit_clicked();
            }
            if let Some(status) = crate::ui::find_status(main.upcast_ref()) {
                status.set_text(&if removed.failed.is_empty() {
                    format!("{} reset", page.label())
                } else {
                    format!("{} reset, but some files would not go -- see the log", page.label())
                });
            }
        },
    );
}
