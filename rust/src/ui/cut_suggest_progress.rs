//! §F2.14 S2b — "Streaming progress counts segments", painted.
//!
//! [`crate::ui::cut_suggest_watch::Call::report`] already answers what the progress is: the status sentence,
//! the bar's fraction, and whether the bar is still pulsing because no segment has closed yet. What was
//! missing was the half that puts those three on the run bar, so the clause described a rule with no door and
//! §S2b's call ran from press to answer showing nothing but a standing bar.
//!
//! Like the rest of the suggest seam, the page hands itself in rather than this module reaching into
//! `ui::window` (which is past its size budget): [`install`] runs from `build_window`, and [`watch`] answers
//! the same `paint` for a window built by a test. Nothing here decides a number — every field comes from
//! [`Call::report`].

use crate::ui::cut_suggest_watch::Progress;

/// The page's door: paint one reply's progress. `install` is how it gets set.
type Paint = fn(&adw::ApplicationWindow, &Progress);

thread_local! {
    static PAINT: std::cell::Cell<Option<Paint>> = const { std::cell::Cell::new(None) };
    /// Every reading this thread's doors painted, oldest first. The bar's sentence is a transient — the press
    /// overwrites the row with its own answer the moment the call ends, and GTK's `ProgressBar` has no getter
    /// to read a fraction or a pulse back — so the door keeps what it was told. A history, not a cache: the
    /// paint still happens, and a test of a press reads the line that already came and went.
    static SAID: std::cell::RefCell<Vec<Progress>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Install this window's progress door, from `ui::window::build_window` — the same moment the suggest edit
/// door goes on (`cut_suggest_land::install`), so a page that can land a suggestion can also show one coming.
pub fn install(paint: Paint) {
    PAINT.with(|slot| slot.set(Some(paint)));
}

/// Paint one reply's progress on the newest window, through the installed door. No window and no door means
/// no page to tell — the state a headless logic test runs the attempt loop in — so the reading is simply
/// dropped rather than failing the call.
pub fn watch(progress: &Progress) {
    let Some(window) = crate::ui::window::built_window() else {
        return;
    };
    if let Some(paint) = PAINT.with(|slot| slot.get()) {
        paint(&window, progress);
    }
    SAID.with(|held| held.borrow_mut().push(progress.clone()));
}

/// The readings painted so far on this thread, oldest first. For a test that must see a progress line the page
/// has already overwritten; the program never reads it.
pub fn painted_for_test() -> Vec<Progress> {
    SAID.with(|held| held.borrow().clone())
}

/// Forget them, so the next press's readings are only its own.
pub fn clear_painted_for_test() {
    SAID.with(|held| held.borrow_mut().clear());
}
