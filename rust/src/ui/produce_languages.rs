//! F5.4 §S3/§S7/§S9 — the Produce page's Translate row: its ticks, what a tick does to the state,
//! and the render's spawner seam. Split out of `produce_page` because that file is at its size budget;
//! `wire_settings` calls [`wire_ticks`] and `finish_produce` passes [`spawn`] to the render.

use adw::prelude::*;
use gtk4 as gtk;
use std::rc::Rc;

use crate::produce_screen as screen;
use crate::ui::produce_page::ProduceState;

/// Tick every `translate-tick-<language>` the page drew. `guarded` answers the page's repaint guard (a
/// rebuild must not read as an edit) and `changed` records the tick — both stay the page's own, so this
/// module touches none of its thread-locals. A tick has no value string and the row holds several
/// languages at once, which is why the toggle's own state is the answer and `set_setting` is not used.
pub fn wire_ticks(
    window: &adw::ApplicationWindow,
    guarded: Rc<dyn Fn() -> bool>,
    changed: Rc<dyn Fn(&str, bool)>,
) {
    for language in screen::TRANSLATE_LANGUAGES {
        let name = format!("translate-tick-{language}");
        // The ticks are `CheckButton`s; `line_step_button` hands back a plain `Button`, so look the
        // widget up by name and downcast.
        let Some(tick) = crate::ui::produce_page::widget_in(window, &name)
            .and_then(|w| w.downcast::<gtk::CheckButton>().ok())
        else {
            continue;
        };
        let language = language.to_string();
        let guarded = Rc::clone(&guarded);
        let changed = Rc::clone(&changed);
        tick.connect_toggled(move |tick| {
            if guarded() {
                return;
            }
            changed(&language, tick.is_active());
        });
    }
}

/// One Translate tick applied to the page's state: the language joins or leaves `translate`, order kept
/// so the sidecars arrive in the order the row listed them (§F5.4 S9: one file per language, in order).
pub fn apply(state: &mut ProduceState, language: &str, on: bool) {
    state.translate.retain(|t| t != language);
    if on {
        state.translate.push(language.to_string());
    }
}

/// The status line a tick leaves behind.
pub fn status_line(language: &str, on: bool) -> String {
    format!("translate {language} {}", if on { "on" } else { "off" })
}

/// The render's spawner: the scripted one when a test has loaded it (this container has no ffmpeg —
/// `command -v ffmpeg` is empty), otherwise the real [`crate::produce_exec::spawn_tool`]. Mirrors
/// `ui::set_narrate_script`: a test drives the whole press without standing an encoder up.
pub fn spawn(command: &crate::produce_exec::Command) -> Result<(), String> {
    match crate::produce_translate::spawn_for_test() {
        Some(sink) => sink(command),
        None => crate::produce_exec::spawn_tool(command),
    }
}
