//! Shared helpers for the integration tests. Each helper here had one byte-identical copy in every
//! test file that used it; they live once now, so a change to how a test reads the shell's status line
//! or walks the parameter catalogue is one edit rather than two dozen.
//!
//! A test binary that declares `mod common;` compiles this whole module, so a helper another file
//! does not call would trip `dead_code`. The `#![allow(dead_code)]` below is what keeps that from
//! becoming a warning attributed to whoever added the last `mod common;`.

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::{
    cut::{Cut, Seg},
    edges::AlignedWord,
    shell::Page,
    params, ui,
};

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

/// Find a widget by the name its builder gave it, walking the whole tree below the window. Several
/// widget tests need this and none of them reached for `gtk::WidgetExt::child` chains, so the walk lives
/// here once; the three files whose version differs keep their own.
pub fn widget_in(window: &adw::ApplicationWindow, name: &str) -> Option<gtk::Widget> {
    fn walk(node: &gtk::Widget, name: &str) -> Option<gtk::Widget> {
        if node.widget_name() == name {
            return Some(node.clone());
        }
        if let Some(child) = node.first_child() {
            let mut cursor = Some(child);
            while let Some(current) = cursor {
                if let Some(found) = walk(&current, name) {
                    return Some(found);
                }
                cursor = current.next_sibling();
            }
        }
        None
    }
    walk(window.upcast_ref(), name)
}

/// One kept clip of the cut. The plain form — every other field at its default.
pub fn seg(s: f64, e: f64) -> Seg {
    Seg { s, e, ..Default::default() }
}

/// The same clip with the camera layer explicitly down, which is how the lane tests state "no drawing".
pub fn clip(s: f64, e: f64) -> Seg {
    Seg { s, e, cam: 0, ..Default::default() }
}

/// A parameter row from the PREPARE and CUT pages only — where a test wants to prove an id is catalogued
/// on exactly one of those two lists rather than anywhere in the catalogue.
pub fn row(id: &str) -> params::Param {
    let mut found = params::prepare()
        .into_iter()
        .chain(params::cut())
        .filter(|row| row.id == id)
        .collect::<Vec<_>>();
    assert_eq!(found.len(), 1, "{id} catalogued {} times", found.len());
    found.pop().unwrap()
}

/// The Cut page's source widget by name, panicking with the page's own words when it was never drawn.
pub fn widget(window: &adw::ApplicationWindow, name: &str) -> gtk::Widget {
    ui::find_source_widget(window, name).unwrap_or_else(|| panic!("the Cut page drew no {name}"))
}

/// That widget narrowed to an editable entry.
pub fn entry(window: &adw::ApplicationWindow, name: &str) -> gtk::Entry {
    widget(window, name)
        .downcast::<gtk::Entry>()
        .unwrap_or_else(|_| panic!("`{name}` is an Entry"))
}

/// A window sitting on the Cut page with `seeded` as its review cut and history open over it — the shape
/// every lane/effect widget test starts from, so the seeding order lives once.
pub fn cut_page(app: &adw::Application, seeded: &Cut) -> adw::ApplicationWindow {
    release_last_window();
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    assert!(
        model.sources.iter().any(|source| source.footage),
        "the fixture must carry a footage row or the Cut tab is locked (shell::lock)"
    );
    let window = ui::build_window(app, &model, "Prepare");
    hold_last_window(window.clone());
    window.present();
    ui::tab_button(&window, Page::Cut)
        .expect("the shell has a Cut tab")
        .emit_by_name::<()>("clicked", &[]);
    ui::reopen_history_on(&window, seeded);
    ui::seed_review_cut(&window, seeded);
    ui::refresh_effects_lane(&window);
    settle();
    window
}

/// One aligned word, as the aligner's file prints it.
pub fn word(text: &str, s: f64, e: f64) -> AlignedWord {
    AlignedWord { word: text.into(), s, e }
}

/// The parameter row with this id, from whichever page catalogues it — `all_rows()` first, so an id that
/// appears on two pages is caught by those rows' own uniqueness check rather than by this one.
pub fn anywhere(id: &str) -> params::Param {
    let found = all_rows().into_iter().filter(|row| row.id == id).collect::<Vec<_>>();
    assert!(!found.is_empty(), "{id} catalogued nowhere");
    found.into_iter().next().unwrap()
}
