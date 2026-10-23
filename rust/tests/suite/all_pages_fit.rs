//! §1's page-fitting rule, stated ONCE and measured on every page.
//!
//! Each spec screen says its page fits at the size §01 opens the window at: nothing hidden, nothing
//! clipped, and a column that does not fit scrolls **inside itself** rather than pushing a control off
//! the bottom (`spec/03-shell.md` §1's page rule, `spec/05-cut.md` §1's three columns, and every page's
//! own §1). This file states that rule as a measurement over the real widget tree — the shared parts live
//! in `tests/common/fit_geometry.rs` — and runs it on all four pages, so a page that stops fitting fails
//! here naming its own control instead of only showing up in a snapshot nobody diffed.
//!
//! §04's own geometry (which column holds what, the divider's opening position) is pinned in
//! `sec_04_prepare_1_screen_split.rs` and `sec_05_cut_1_screen_layout.rs`; what is NEW here is the
//! whole-page-inside-the-window half, applied to every page and to the band under it.

use crate::common::fit_geometry as fit_checks;

use std::sync::atomic::{AtomicU32, Ordering};

use crate::common;
use adw::prelude::*;
use gtk4 as gtk;
use naivepost::project::Project;
use naivepost::shell;
use naivepost::ui;

const ITEM: &str = "all_pages_fit";

/// The four tabs the shell builds (`Page::all()`): Prepare, Cut, Narrate, Produce. Settings is a
/// dialog (F0.13), not a tab, so nothing here pretends it is.
const PAGES: [&str; 4] = ["Prepare", "Cut", "Narrate", "Produce"];

// Which pages the round below actually measured, and every check on each of them passed. One round per
// process (`one_app_round`: a GTK main loop may only be run by the thread that starts it), so every
// test drives that one round and reads its result out of this record.
static RAN: AtomicU32 = AtomicU32::new(0);

fn page_bit(page: &str) -> u32 {
    1 << PAGES.iter().position(|p| *p == page).unwrap()
}

/// One application for the whole file; each page gets a window of its own inside the single round,
/// because a window can only show one page at a time and every number is read off a laid-out tree.
fn fit_round() {
    common::one_app_round(|app| {
        for page in PAGES {
            let window = page_window(app, page);
            fit_checks::show_and_lay_out(&window);
            let content = fit_checks::page_of(&window, page);
            fit_checks::assert_all_inside(&format!("{page} page"), &window, &content);
            check_the_band_is_under_the_page(&window, page);
            check_scrollers_do_the_scrolling(&window, page);
            window.close();
            RAN.fetch_or(page_bit(page), Ordering::SeqCst);
        }
    });
}

/// Every page measured once: the whole page inside the window, the band under it, nothing clipped.
#[test]
fn all_pages_fit_at_the_size_the_shell_opens_at() {
    common::pin_private_cwd();
    fit_round();
    let ran = RAN.load(Ordering::SeqCst);
    for page in PAGES {
        assert_eq!(
            ran & page_bit(page),
            page_bit(page),
            "{ITEM}: the {page} page was never measured, or one of its checks failed"
        );
    }
}

/// Each spec screen's §1 says its page fits; this is that rule as one test per page, so a failure names
/// the page rather than the loop. The window is built ON that page (`build_window`'s third argument),
/// so each test measures the page it is named for. Every one of them calls `fit_round()` first: it is
/// `Once`-guarded, so the repeat calls are free, but without one a test could run before the driver.
#[test]
fn the_prepare_page_fits_at_the_size_the_shell_opens_at() {
    common::pin_private_cwd();
    fit_round();
    assert_ne!(
        RAN.load(Ordering::SeqCst) & page_bit("Prepare"),
        0,
        "{ITEM}: the Prepare page was never measured, or one of its checks failed"
    );
}

#[test]
fn the_cut_page_fits_at_the_size_the_shell_opens_at() {
    common::pin_private_cwd();
    fit_round();
    assert_ne!(
        RAN.load(Ordering::SeqCst) & page_bit("Cut"),
        0,
        "{ITEM}: the Cut page was never measured, or one of its checks failed"
    );
}

#[test]
fn the_narrate_page_fits_at_the_size_the_shell_opens_at() {
    common::pin_private_cwd();
    fit_round();
    assert_ne!(
        RAN.load(Ordering::SeqCst) & page_bit("Narrate"),
        0,
        "{ITEM}: the Narrate page was never measured, or one of its checks failed"
    );
}

#[test]
fn the_produce_page_fits_at_the_size_the_shell_opens_at() {
    common::pin_private_cwd();
    fit_round();
    assert_ne!(
        RAN.load(Ordering::SeqCst) & page_bit("Produce"),
        0,
        "{ITEM}: the Produce page was never measured, or one of its checks failed"
    );
}

/// A window opened on one page at the size §01 fixes, then forced to a real allocation: under xvfb a
/// mapped window is given what the X server offers, so the allocation has to be asked for (the same
/// trick `sec_04_prepare_1_screen_split.rs` uses, which is why the numbers here mean something).
fn page_window(app: &adw::Application, page: &str) -> adw::ApplicationWindow {
    let window = ui::build_window(app, &Project::default(), page);
    fit_checks::show_and_lay_out(&window);
    window
}

/// §03's §1: the bottom half is "exactly as tall as the run bar row plus the log expander's header,
/// with no empty space under them: the page takes all the rest of the height". So the band has to be
/// inside the window AND below the page — a page whose minimum eats the whole height leaves the band
/// with zero pixels, which no name lookup can see.
fn check_the_band_is_under_the_page(window: &adw::ApplicationWindow, page: &str) {
    let floor = window.measure(gtk::Orientation::Vertical, -1).0;
    assert!(
        floor <= shell::WINDOW_MIN_H,
        "{ITEM}: the {page} page cannot be shrunk below {floor} px, over §01's {} px minimum — \
         below that the run bar and the log header would be outside the window",
        shell::WINDOW_MIN_H
    );
    let win_h = f64::from(window.height());
    for name in ["run-bar", "inputs-readout", "log-expander"] {
        let widget = ui::find_source_widget(window, name)
            .unwrap_or_else(|| panic!("{ITEM}: no widget named {name} on the {page} page"));
        let (x, y, w, h) = fit_checks::window_box(&widget);
        assert!(
            w > 0.0 && h > 0.0,
            "{ITEM}: on {page}, {name} was given no pixels — the bottom band is outside the window"
        );
        assert!(
            y + h <= win_h + fit_checks::SLACK_PX,
            "{ITEM}: on {page}, {name} ends at y={y:.0}..{:.0}, past the bottom of a {win_h:.0} px window",
            y + h
        );
        assert!(
            x >= -fit_checks::SLACK_PX,
            "{ITEM}: on {page}, {name} starts off the left edge at x={x:.0}"
        );
    }
}

/// The rule behind "a column that does not fit scrolls **inside itself**": anything that may spill has
/// to live under a `ScrolledWindow`; a spilling widget with no scroller above it is clipped by the
/// window in silence.
fn check_scrollers_do_the_scrolling(window: &adw::ApplicationWindow, page: &str) {
    let content = fit_checks::page_of(window, page);
    let win_h = f64::from(window.height());
    let mut spilled: Vec<String> = Vec::new();
    for (name, widget) in fit_checks::named_inside(&content) {
        let (_, y, _, h) = fit_checks::window_box(&widget);
        if h <= 0.0 || y + h <= win_h + fit_checks::SLACK_PX {
            continue;
        }
        if !fit_checks::under_a_scroller(&widget) {
            spilled.push(format!("`{name}` ends at y={:.0}", y + h));
        }
    }
    assert!(
        spilled.is_empty(),
        "{ITEM}: on {page}, content runs past the bottom of the window with no scroller above it, so it \
         is clipped rather than scrollable:\n  {}",
        spilled.join("\n  ")
    );
}
