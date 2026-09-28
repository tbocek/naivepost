//! Shared helpers for the integration tests. Each helper here had one byte-identical copy in every
//! test file that used it; they live once now, so a change to how a test reads the shell's status line
//! or walks the parameter catalogue is one edit rather than two dozen.
//!
//! A test binary that declares `mod common;` compiles this whole module, so a helper another file
//! does not call would trip `dead_code`. The `#![allow(dead_code)]` below is what keeps that from
//! becoming a warning attributed to whoever added the last `mod common;`.

use adw::prelude::*;
use naivepost::{params, ui};

/// The shell's status line as plain text — where the app says what a press did, so most widget tests
/// end by reading this rather than reaching into the widget that was pressed.
pub fn status_text(window: &adw::ApplicationWindow) -> String {
    ui::find_status(window.upcast_ref())
        .expect("the shell has a status line")
        .text()
        .to_string()
}

/// Every list that carries rows, chained across all five pages so uniqueness is checked against the
/// whole catalogue rather than one page (`tests/hands_off.rs`'s `row()` makes the same choice).
pub fn all_rows() -> Vec<params::Param> {
    params::prepare()
        .into_iter()
        .chain(params::cut())
        .chain(params::effects())
        .chain(params::narrate())
        .chain(params::produce())
        .collect()
}

/// The project every test loads: `fixtures/demo.naivepost`, resolved from the crate root. `env!` is
/// expanded where this module is compiled, which is the same `rust/` directory for every test binary,
/// so moving the 42 copies here leaves each file's path exactly where it was.
pub fn fixture_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/demo.naivepost")
}

/// Let the main context run what the widget emissions queued. Bounded and NON-blocking:
/// `iteration(false)` returns immediately when nothing is ready, where a blocking wait would park the
/// test thread forever. Three files keep their own `settle` because theirs pumps `while pending()`
/// instead of a fixed 64 turns — converting those would change how many turns run, not just where the
/// code lives.
pub fn settle() {
    let context = glib::MainContext::default();
    for _ in 0..64 {
        if !context.iteration(false) {
            break;
        }
    }
}

thread_local! {
    /// Strong handle on the window last built, so the next check drops it first: a closed GTK window is not
    /// destroyed and its names stay parented, which would send a lookup to the wrong tree.
    pub static LAST_WINDOW: std::cell::RefCell<Option<adw::ApplicationWindow>> =
        const { std::cell::RefCell::new(None) };
}

/// Remember the window a check is about to work in, so the next check can release it first.
pub fn hold_last_window(window: adw::ApplicationWindow) {
    LAST_WINDOW.with(|cell| *cell.borrow_mut() = Some(window));
}

/// Close the window the previous check left standing, then let the close's own emissions run.
pub fn release_last_window() {
    LAST_WINDOW.with(|cell| {
        if let Some(old) = cell.borrow_mut().take() {
            old.close();
        }
    });
    settle();
}
