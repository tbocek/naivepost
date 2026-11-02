//! §F2.14 S2b: "Streaming progress counts segments", painted on the run bar.
//! Everything comes from [`Call::report`]; [`install`] runs from `build_window`, and [`watch`] answers the same `paint` for a test window.

use crate::ui::cut_page_state::{CutPage, PAGE as CUT_PAGE};
use crate::ui::cut_suggest_watch::Progress;

/// The page's door: paint one reply's progress. `install` is how it gets set.
pub(crate) type Paint = fn(&adw::ApplicationWindow, &Progress);

/// Install this window's progress door, from `ui::window::build_window`.
pub fn install(paint: Paint) {
    CUT_PAGE.with(|CutPage { paint: slot, .. }| slot.set(Some(paint)));
}

/// Paint one reply's progress on the newest window; with no window or door the reading is dropped.
pub fn watch(progress: &Progress) {
    if crate::ui::background::on_worker() {
        let reading = progress.clone();
        crate::ui::background::on_window_thread(move || watch(&reading));
        return;
    }
    let Some(window) = crate::ui::window::built_window() else {
        return;
    };
    if let Some(paint) = CUT_PAGE.with(|CutPage { paint: slot, .. }| slot.get()) {
        paint(&window, progress);
    }
    CUT_PAGE.with(|CutPage { said: held, .. }| held.borrow_mut().push(progress.clone()));
}

/// Readings painted so far on this thread; test-only.
pub fn painted_for_test() -> Vec<Progress> {
    CUT_PAGE.with(|CutPage { said: held, .. }| held.borrow().clone())
}

pub fn clear_painted_for_test() {
    CUT_PAGE.with(|CutPage { said: held, .. }| held.borrow_mut().clear());
}
