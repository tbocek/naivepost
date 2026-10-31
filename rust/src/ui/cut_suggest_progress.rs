//! §F2.14 S2b: "Streaming progress counts segments", painted on the run bar.
//! Everything comes from [`Call::report`]; [`install`] runs from `build_window`, and [`watch`] answers the same `paint` for a test window.

use crate::ui::cut_suggest_watch::Progress;

/// The page's door: paint one reply's progress. `install` is how it gets set.
type Paint = fn(&adw::ApplicationWindow, &Progress);

thread_local! {
    static PAINT: std::cell::Cell<Option<Paint>> = const { std::cell::Cell::new(None) };
    /// Every reading painted on this thread, oldest first: GTK's `ProgressBar` has no getter and the press overwrites the sentence.
    static SAID: std::cell::RefCell<Vec<Progress>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Install this window's progress door, from `ui::window::build_window`.
pub fn install(paint: Paint) {
    PAINT.with(|slot| slot.set(Some(paint)));
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
    if let Some(paint) = PAINT.with(|slot| slot.get()) {
        paint(&window, progress);
    }
    SAID.with(|held| held.borrow_mut().push(progress.clone()));
}

/// Readings painted so far on this thread; test-only.
pub fn painted_for_test() -> Vec<Progress> {
    SAID.with(|held| held.borrow().clone())
}

pub fn clear_painted_for_test() {
    SAID.with(|held| held.borrow_mut().clear());
}
