//! F5.1 S2's confirmation — the dialog behind "Overwrite ‹base›?".

use adw::prelude::*;

use crate::produce_flow;

thread_local! {
    static OPEN_DIALOG: std::cell::RefCell<Option<adw::MessageDialog>> =
        const { std::cell::RefCell::new(None) };
}

pub fn open_dialog() -> Option<adw::MessageDialog> {
    OPEN_DIALOG.with(|held| held.borrow().clone())
}

fn remember(dialog: Option<&adw::MessageDialog>) {
    OPEN_DIALOG.with(|held| *held.borrow_mut() = dialog.cloned());
}

/// §S2's confirmation end to end: ask about `video`, and run `on_yes` only if the answer is
/// Overwrite.
pub fn confirm_then(
    parent: &adw::ApplicationWindow,
    video: &std::path::Path,
    on_yes: impl Fn() + 'static,
    on_no: impl Fn(&str) + 'static,
) -> String {
    let (asked, _) = produce_flow::overwrite_asked(video);
    say(parent, &asked);
    let question = asked.clone();
    ask_overwrite(parent, video, move |answer| {
        if produce_flow::overwrite_proceeds(answer) {
            on_yes();
        } else {
            on_no(&question);
        }
    });
    asked
}

/// §S2's question on the window's status line.
fn say(window: &adw::ApplicationWindow, said: &str) {
    if let Some(status) = crate::ui::window::find_status(window.upcast_ref()) {
        status.set_text(said);
    }
}

/// Ask §S2's question about `video` and hand the answer to `on_answer`: the response id arrives
/// there, and `produce_flow::overwrite_proceeds` decides whether the encode runs.
pub fn ask_overwrite(
    parent: &adw::ApplicationWindow,
    video: &std::path::Path,
    on_answer: impl Fn(&str) + 'static,
) {
    // §S2 asks ONCE.
    if open_dialog().is_some() {
        return;
    }
    let (title, body) = produce_flow::overwrite_asked(video);
    let dialog = adw::MessageDialog::new(Some(parent), Some(&title), Some(&body));
    dialog.set_widget_name("overwrite-dialog");
    // × answers Cancel, the same convention `new_project_confirm` sets: closing without saying is
    // not a yes.
    dialog.set_property("close-response", produce_flow::ANSWER_CANCEL);
    dialog.add_response(produce_flow::ANSWER_CANCEL, "Cancel");
    dialog.add_response(produce_flow::ANSWER_OVERWRITE, "Overwrite");
    dialog.set_response_appearance(
        produce_flow::ANSWER_OVERWRITE,
        adw::ResponseAppearance::Destructive,
    );
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
