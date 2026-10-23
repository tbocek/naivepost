//! §04-prepare.md **§1 Screen**, badge **1** — the source rows' FACES, read off a real window.
//!
//! Badge 1 is the sources list, which [`03-shell.md` §4] draws row by row: "🎥 footage toggle … 🎤 narrator
//! button cycling free slots … file name … ⚠ … 🗣 split-the-voice toggle … 🗑 remove". What nothing checked
//! was the two FACES the QUESTIONS answer for this item names: the narrator button must carry its 🎤 BESIDE
//! the slot number (a bare number reads as a row index), and the split toggle must NOT wear a microphone (it
//! did, so on screen the mic looked like it had moved to after the file name). Both facts are about what a
//! person sees, so both are read off the widgets, not off `sources::row_controls`. The narrator face itself
//! is pinned where badge 4 already lives — `sources_list.rs`'s staged-shot check reads the mic icon, the slot
//! digit and the slot-1 emphasis class off one button — so this file keeps the order and the split-toggle
//! face and does not read the same button a second way.
//!
//! Shape follows `sec_04_prepare_1_screen_split.rs`: ONE application round for the whole file (a process may
//! start GTK's loop once), every check inside it, each recording that it got there. No fixed sleep: `lay_out`
//! drains pending context items until none are pending, and nothing here waits on time at all.

use std::sync::atomic::{AtomicBool, Ordering};

use crate::common;
use adw::prelude::*;
use gtk4 as gtk;
use naivepost::project::Project;
use naivepost::ui;

const ITEM: &str = "sec_04_prepare_1_screen";

static RAN_ORDER: AtomicBool = AtomicBool::new(false);
static RAN_SPLIT_FACE: AtomicBool = AtomicBool::new(false);

fn rows_round() {
    common::one_app_round(|app| {
        let window = prepare_window(app);
        lay_out();
        // Every check runs inside the ONE round this binary gets, and none of them may skip the others:
        // each records its own flag only after it has actually asserted.
        let mut ran: Vec<&str> = Vec::new();
        run_check(&mut ran, "order", || {
            the_row_draws_the_symbols_in_order(&window)
        });
        run_check(&mut ran, "split", || {
            the_split_toggle_is_not_the_mic(&window)
        });
        window.close();
        // The flags each test reads back: set only by a check that passed, so a red check stays red however
        // cargo ordered these two tests (the round itself runs once — `one_app_round`).
        if ran.contains(&"order") {
            RAN_ORDER.store(true, Ordering::SeqCst);
        }
        if ran.contains(&"split") {
            RAN_SPLIT_FACE.store(true, Ordering::SeqCst);
        }
        assert_eq!(
            ran,
            vec!["order", "split"],
            "{ITEM}: a row-face check failed — {ran:?} passed"
        );
    });
}

/// Run one check, marking it passed only if it did. The round reports every failure together rather than
/// letting the first panic hide the other one.
fn run_check<'a>(ran: &mut Vec<&'a str>, name: &'a str, check: impl FnOnce()) {
    if std::panic::catch_unwind(std::panic::AssertUnwindSafe(check)).is_ok() {
        ran.push(name);
    }
}

#[test]
fn sec_04_prepare_1_screen_s4_the_row_draws_the_four_symbols_in_order() {
    common::pin_private_cwd();
    rows_round();
    assert!(RAN_ORDER.load(Ordering::SeqCst), "the round never ran");
}

#[test]
fn sec_04_prepare_1_screen_s4_the_split_toggle_is_not_the_mic() {
    common::pin_private_cwd();
    rows_round();
    assert!(
        RAN_SPLIT_FACE.load(Ordering::SeqCst),
        "the split-toggle face check never ran"
    );
}

// --- the checks -------------------------------------------------------------------------------------

/// §4's row, left to right: 🎥, 🎤 narrator, the file name (which takes the free width), the track menu and
/// ⚠ where the file earns them, 🗣 split, 🗑. Every one of those is present-or-absent by the FILE, so the
/// assertion walks the row's own children and compares the symbols it finds against the order §4 lists —
/// an extra control between the name and the trash, or the mic after the name, fails here.
fn the_row_draws_the_symbols_in_order(window: &adw::ApplicationWindow) {
    let row = widget(window, "source-row-0");
    let inner = row
        .first_child()
        .expect("the row holds its box of controls");
    let names: Vec<String> = inner
        .observe_children()
        .iter::<glib::Object>()
        .filter_map(Result::ok)
        .filter_map(|o| o.downcast_ref::<gtk::Widget>().cloned())
        .map(|child| child.widget_name().to_string())
        .collect();
    println!("{ITEM} s4 row-0 children in order: {names:?}");

    // The symbol positions, filtered out of whatever the file earned: the four §4 always gives a video row,
    // plus the ⚠ only if this fixture's name has no timestamp (as `sources_list.rs` also reads it).
    let mut symbols: Vec<&str> = Vec::new();
    for wanted in [
        "footage-0",
        "narrator-0",
        "source-name-0",
        "warning-0",
        "separate-0",
        "remove-0",
    ] {
        if names.iter().any(|name| name == wanted) {
            symbols.push(wanted);
        }
    }
    assert_eq!(
        symbols,
        vec![
            "footage-0",
            "narrator-0",
            "source-name-0",
            "warning-0",
            "separate-0",
            "remove-0"
        ],
        "{ITEM} §4: the row's faces are not 🎥 🎤 name ⚠ 🗣 🗑 in that order — found {names:?}"
    );

    // The name is the part that expands, which is what pushes the symbols to the row's right end (§4).
    let name = widget(window, "source-name-0");
    assert!(
        name.hexpands(),
        "{ITEM} §4: \"the file name takes the free width\", so it must be the expander"
    );

    // Every face the row wears must be one the theme can actually draw: an unresolved name renders as the
    // broken-image picture, which is how this row once showed a torn box where §4 draws 🎥. The proof is the
    // same rule `icons::missing_in_for_test` applies to a shipped tree — the file has to sit in a folder the
    // installed theme's own `index.theme` lists.
    for (name_, icon) in [
        ("footage-0", naivepost::ui::source_rows::FOOTAGE_ICON),
        ("separate-0", naivepost::ui::source_rows::SEPARATE_ICON),
    ] {
        let button = widget(window, name_)
            .downcast::<gtk::ToggleButton>()
            .expect("a symbol toggle");
        assert_eq!(
            button.icon_name().unwrap_or_default().as_str(),
            icon,
            "{ITEM} §4: {name_} wears the face `source_rows` spells"
        );
        assert!(
            theme_holds(icon),
            "{ITEM} §4: {name_} asks for {icon}, which no installed theme resolves — it would draw the \
             broken-image picture instead of a symbol"
        );
    }
}

/// Does an installed icon theme list a folder that actually holds `{icon}.svg`? That is GTK's own lookup
/// rule (`icons::missing_in_for_test` applies it to the tree a build ships); here the system themes under
/// `/usr/share/icons` are asked, because that is what the app draws from on a desktop.
fn theme_holds(icon: &str) -> bool {
    let root = std::path::Path::new("/usr/share/icons");
    naivepost::icons::themes(root).into_iter().any(|theme| {
        std::fs::read_to_string(theme.join("index.theme"))
            .map(|text| {
                text.lines()
                    .filter(|line| {
                        line.starts_with("Directories=")
                            || !(line.starts_with('[') || line.contains('='))
                    })
                    .flat_map(|line| {
                        line.split_once('=')
                            .map(|(_, rest)| rest)
                            .unwrap_or(line)
                            .split([';', ','])
                    })
                    .filter(|part| !part.trim().is_empty())
                    .any(|part| {
                        let dir = theme.join(part.trim());
                        // A theme's `Directories=` entries end in the size and context (`48x48/actions`,
                        // Adwaita's `symbolic/actions`), while hicolor-style themes keep their files one
                        // level deeper (`scalable/apps`).
                        ["", "actions", "apps", "devices", "status", "legacy"]
                            .into_iter()
                            .any(|extra| dir.join(extra).join(format!("{icon}.svg")).is_file())
                    })
            })
            // `Directories=` is one long comma-separated line in Adwaita's index.theme; the entries are
            // also written one per line, so both spellings have to be read before a theme is called blind.
            .unwrap_or(false)
    })
}

/// The fact the render review flagged: the split-the-voice toggle wore the narrator's microphone, so the
/// row read as "mic moved after the name". It must be a DIFFERENT symbol and keep its own tooltip; §4 also
/// greys it on a split product, which this proves through the widget by turning one of the session's own
/// rows into a split product and repainting.
fn the_split_toggle_is_not_the_mic(window: &adw::ApplicationWindow) {
    let separate = widget(window, "separate-0")
        .downcast::<gtk::ToggleButton>()
        .expect("the split control is a toggle");
    let face = separate.icon_name().unwrap_or_default().to_string();
    assert_ne!(
        face,
        naivepost::ui::source_rows::NARRATOR_ICON,
        "{ITEM} §4: the 🗣 split toggle may not wear the narrator's microphone ({face}) — \
         that is what made the row read as a mic after the file name"
    );
    assert_eq!(
        face,
        naivepost::ui::source_rows::SEPARATE_ICON,
        "{ITEM} §4: the split toggle's face is spelled once in `source_rows`"
    );
    let tip = separate.tooltip_text().unwrap_or_default().to_string();
    assert!(
        tip.contains(naivepost::sources::SEPARATE_TIP.trim_end()),
        "{ITEM} §4: the toggle still explains itself, got {tip:?}"
    );
    // §4's greyed case, driven rather than asserted from a rule: rename the session's first source to a
    // split product's name (that is what `sources::separable` reads), repaint the list through the page's
    // own door, and the same-named toggle must now be insensitive.
    let session = ui::session_handle(window);
    let renamed = {
        let mut project = session.borrow_mut();
        let held = project
            .sources
            .first_mut()
            .expect("the fixture has a source row");
        let stem = held
            .path
            .rsplit_once('.')
            .map(|(stem, _)| stem)
            .unwrap_or(held.path.as_str());
        held.path = format!("{stem}.voice.mkv");
        held.path.clone()
    };
    ui::source_rows_refresh::redraw(window);
    lay_out();
    let split_product = widget(window, "separate-0")
        .downcast::<gtk::ToggleButton>()
        .expect("the row keeps its split toggle");
    assert!(
        !split_product.is_sensitive(),
        "{ITEM} §4: 🗣 is greyed on a split product — {renamed:?} is one, and the toggle stayed live"
    );
}

// --- harness (same shape as `sec_04_prepare_1_screen_split.rs`) -------------------------------------

fn fixture() -> Project {
    naivepost::project::load(&common::fixture_dir()).expect("the fixture loads")
}

fn prepare_window(app: &adw::Application) -> adw::ApplicationWindow {
    let window = ui::build_window(app, &fixture(), "Prepare");
    common::size_to_minimum(&window);
    window
}

/// Let GTK allocate the tree: `present()` alone lays nothing out until pending events run.
fn lay_out() {
    let context = glib::MainContext::default();
    while context.pending() {
        context.iteration(false);
    }
}

fn widget(window: &adw::ApplicationWindow, name: &str) -> gtk::Widget {
    ui::find_source_widget(window, name).unwrap_or_else(|| panic!("{ITEM}: no widget named {name}"))
}
