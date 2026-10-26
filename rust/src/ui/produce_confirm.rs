//! F5.1 S2's confirmation — the dialog behind "Overwrite ‹base›?".
//!
//! Thin by design, in the shape `produce_words` set: it builds an `adw::MessageDialog`, presents it, and
//! each button calls its seam and closes. No modal `run()` — nesting a second main loop inside a widget
//! callback is what this codebase avoids (see that module's header), and a wired answer is also the answer
//! a test can give. The rule the dialog serves is not here: the wording and which response means "encode
//! anyway" are [`crate::produce_flow`]'s (`overwrite_asked`, `overwrite_proceeds`).

use adw::prelude::*;

use crate::produce_flow;

// The dialog this module has open, if any. An `adw::MessageDialog` is its OWN top-level window rather than
// a child of the page, so it is not reachable by walking the page's content tree; holding it here is how a
// caller finds the box it just opened, through [`open_dialog`].
thread_local! {
    static OPEN_DIALOG: std::cell::RefCell<Option<adw::MessageDialog>> =
        const { std::cell::RefCell::new(None) };
}

/// The overwrite dialog currently open, if any. Two callers: [`ask_overwrite`]'s own guard below (a question
/// already on screen is not asked twice) and a test finding the box the press just opened — an
/// `adw::MessageDialog` is its own top level, so it is not reachable by walking the page's content tree.
pub fn open_dialog() -> Option<adw::MessageDialog> {
    OPEN_DIALOG.with(|held| held.borrow().clone())
}

/// Record (or clear) the open dialog. Called by the opener and by the response handler.
fn remember(dialog: Option<&adw::MessageDialog>) {
    OPEN_DIALOG.with(|held| *held.borrow_mut() = dialog.cloned());
}

/// §S2's confirmation end to end: ask about `video`, and run `on_yes` only if the answer is Overwrite; any
/// other answer goes to `on_no` with the question that was asked, so the press's open run can be closed.
/// Returns the question that was said, so the press hands it back as its own answer. The closure is the
/// encode, which lives on the page — this module owns asking, not producing.
///
/// The body goes to the log as well as into the dialog: the dialog closes on its answer and the size-and-age
/// line is the one thing a person wants back when they read the run afterwards.
pub fn confirm_then(
    parent: &adw::ApplicationWindow,
    video: &std::path::Path,
    on_yes: impl Fn() + 'static,
    on_no: impl Fn(&str) + 'static,
) -> String {
    // `ask_overwrite` computes the same pair and puts both halves on screen: the body inside the dialog,
    // the question on the status line, and the body also in the log — the dialog closes on its answer and
    // the size-and-age line is the one thing a person wants back when they read the run afterwards.
    let (asked, _) = produce_flow::overwrite_asked(video);
    say(parent, &asked);
    let question = asked.clone();
    ask_overwrite(parent, video, move |answer| {
        if produce_flow::overwrite_proceeds(answer) {
            on_yes();
        } else {
            // Anything else leaves the standing video alone: the question was the whole answer.
            on_no(&question);
        }
    });
    asked
}

/// §S2's question on the window's status line. `ask_overwrite` shows the body in the dialog and logs it;
/// this is the short half, the one that stays up after the answer so a glance at the bar says where the
/// press stopped (the same route every page's answer takes — `produce_page::say`'s shape, private there).
fn say(window: &adw::ApplicationWindow, said: &str) {
    if let Some(status) = crate::ui::window::find_status(window.upcast_ref()) {
        status.set_text(said);
    }
}

/// Ask §S2's question about `video` and hand the answer to `on_answer`: the response id arrives there, and
/// `produce_flow::overwrite_proceeds` decides whether the encode runs. Returns nothing on purpose — the
/// press that asked is still on the stack when the answer lands, so the run continues from `on_answer`.
pub fn ask_overwrite(
    parent: &adw::ApplicationWindow,
    video: &std::path::Path,
    on_answer: impl Fn(&str) + 'static,
) {
    // §S2 asks ONCE. A second ▶ (or ↻ then ▶) over a question still on screen would stand a second,
    // identical dialog up: two questions, two answers, and a person who answers both encodes twice.
    // The first question is the one, so a standing dialog means there is nothing to ask.
    if open_dialog().is_some() {
        return;
    }
    let (title, body) = produce_flow::overwrite_asked(video);
    let dialog = adw::MessageDialog::new(Some(parent), Some(&title), Some(&body));
    dialog.set_widget_name("overwrite-dialog");
    // × answers Cancel, the same convention `new_project_confirm` sets: closing without saying is not a yes.
    dialog.set_property("close-response", produce_flow::ANSWER_CANCEL);
    dialog.add_response(produce_flow::ANSWER_CANCEL, "Cancel");
    dialog.add_response(produce_flow::ANSWER_OVERWRITE, "Overwrite");
    dialog.set_response_appearance(
        produce_flow::ANSWER_OVERWRITE,
        adw::ResponseAppearance::Destructive,
    );
    // Enter answers the default, and the encode has no undo (the question says so): the default is Cancel. The
    // first rewrite made the destructive Overwrite the default, so Enter replaced the render.
    dialog.set_default_response(Some(produce_flow::ANSWER_CANCEL));
    remember(Some(&dialog));
    dialog.connect_response(None, move |dialog: &adw::MessageDialog, response| {
        remember(None);
        let answer = response.to_string();
        dialog.close();
        on_answer(&answer);
    });
    dialog.present();
}
