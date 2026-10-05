//! §7's exchange lines as they reach the shell's log, and the log's own text view.
//!
//! Split out of `window.rs`, which is past its size budget. Both doors are small but neither belongs in
//! `log_link` (which owns only which line is clickable) nor in `exchanges` (which owns the page format and
//! must stay free of widgets): this is the moment a recorded line becomes something on the screen.

use adw::prelude::*;
use gtk4 as gtk;

use super::window;

/// The log's text view, for a reader that has the window but not the expander: an expander keeps its
/// body behind an internal widget, so every caller reaches it this way and none should re-derive it.
pub fn log_view(window: &adw::ApplicationWindow) -> Option<gtk::TextView> {
    window::log_expander(window)?.child()?.downcast().ok()
}

/// A §7 exchange line written into the shell's log: the door §7's recorder writes its two per-call lines
/// and its once-per-run link through. A line that reaches the log is not left invisible -- the expander
/// opens and the body is repainted through [`window::paint_progress`], the only door that does both, so
/// the link sits where §1 says it must be clickable while the run that wrote it is still going.
///
/// With no window on this thread (a headless walk dialling a chat call) the line still goes to the sink
/// and nothing is painted: recording never fails the thing that recorded.
pub fn note_exchange_line(line: &str) {
    window::log_line(line);
    let Some(window) = window::built_window() else { return };
    let (Some(progress), Some(expander), Some(view), Some(queue), Some(status)) = (
        window::progress_bar(&window),
        window::log_expander(&window),
        log_view(&window),
        window::run_queue(&window),
        window::find_status(window.upcast_ref()),
    ) else {
        return;
    };
    window::paint_progress(
        &progress,
        &expander,
        &view,
        &queue.borrow(),
        crate::ui::shell_beats::StatusLine { status: &status, owned: false },
        true,
    );
}
