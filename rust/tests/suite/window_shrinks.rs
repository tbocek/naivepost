//! The window has a MINIMUM, not a shrink target: spec/03-shell.md §1 says "Its minimum size is 1240×900:
//! it opens at that size and cannot be made smaller, and at that size every control and lane a page names
//! is on screen at once. No page scrolls as a whole." A GTK4 window cannot go below its content's minimum
//! size either, so a hard `set_size_request` in a PAGE would silently widen or lengthen the shell past the
//! spec's number — which is what this measures now, together with the other half of the sentence: at the
//! minimum the page's own scroller must have nothing left to scroll.
//!
//! So this prints, on every page: the window's own floor (must be EXACTLY §1's 1240×900 — no lower, since
//! `build_window` requests it, and no higher, since no page may hold the shell open wider or taller), and
//! the travel of `page-scroll-‹tab›` (zero means everything the page names is on screen at that size).
//! Per-widget minimums are printed with `--nocapture`, because when this fails the useful half of the
//! message is WHICH widget moved the floor.

use std::sync::atomic::{AtomicBool, Ordering};

use crate::common;
use adw::prelude::*;
use common::fixture_dir;
use gtk4 as gtk;
use naivepost::shell::Page;
use naivepost::ui;

static RAN_PAGES: AtomicBool = AtomicBool::new(false);

/// Every widget under `node` carrying a name, with its horizontal and vertical MINIMUM (`measure(..).0`).
/// Unnamed widgets are skipped: the app names everything the spec names, and a walk that printed all ~2k
/// nodes could not be read. The window itself is NOT included — it always reports the sum of its own
/// children, so it would sit at the top of every list without saying which child to blame.
fn floors(node: &gtk::Widget, out: &mut Vec<(String, i32, i32)>) {
    let name = node.widget_name();
    if !name.is_empty() && node.parent().is_some() {
        let w = node.measure(gtk::Orientation::Horizontal, -1).0;
        let h = node.measure(gtk::Orientation::Vertical, -1).0;
        out.push((name.to_string(), w, h));
    }
    for child in node
        .observe_children()
        .iter::<glib::Object>()
        .filter_map(Result::ok)
    {
        if let Some(widget) = child.downcast_ref::<gtk::Widget>() {
            floors(widget, out);
        }
    }
}

/// The window's own minimum size, which is what the window manager refuses to go under.
fn window_floor(window: &adw::ApplicationWindow) -> (i32, i32) {
    (
        window.measure(gtk::Orientation::Horizontal, -1).0,
        window.measure(gtk::Orientation::Vertical, -1).0,
    )
}

/// Lay the window out once so every widget has been sized; the numbers read after it are minimums, so the
/// size it is laid out at does not change them.
fn lay_out(window: &adw::ApplicationWindow) {
    common::size_to_minimum(window);
}

/// The visible page's content box: what the ViewStack shows right now. Measuring only this subtree is how
/// the test blames one page instead of the sum of all four (`AdwViewStack` measures homogeneously).
fn visible_page(window: &adw::ApplicationWindow) -> gtk::Widget {
    fn find_stack(node: &gtk::Widget) -> Option<adw::ViewStack> {
        if let Some(stack) = node.downcast_ref::<adw::ViewStack>() {
            return Some(stack.clone());
        }
        for child in node
            .observe_children()
            .iter::<glib::Object>()
            .filter_map(Result::ok)
        {
            if let Some(widget) = child.downcast_ref::<gtk::Widget>() {
                if let Some(stack) = find_stack(widget) {
                    return Some(stack);
                }
            }
        }
        None
    }
    let stack =
        find_stack(window.upcast_ref()).expect("the shell switches pages through an AdwViewStack");
    stack.visible_child().expect("a page is always visible")
}

/// The shell's bottom box — run bar, readouts, log expander — found by its run bar rather than by a name
/// it does not carry: §1 makes it the other half of the vertical divider, so its height is part of what
/// the window must be able to shrink under.
fn bottom_box(window: &adw::ApplicationWindow) -> gtk::Widget {
    fn find_run_row(node: &gtk::Widget) -> Option<gtk::Widget> {
        if node.widget_name() == "run-bar" {
            // The run bar's own parent is the bottom box (§1: separator, run bar row, log expander).
            return node.parent();
        }
        for child in node
            .observe_children()
            .iter::<glib::Object>()
            .filter_map(Result::ok)
        {
            if let Some(widget) = child.downcast_ref::<gtk::Widget>() {
                if let Some(found) = find_run_row(widget) {
                    return Some(found);
                }
            }
        }
        None
    }
    find_run_row(window.upcast_ref()).expect("the shell has a run bar inside its bottom box")
}

/// Travel left in the visible tab's own page scroller: `upper - page_size`. Zero means the whole page fits
/// in what the shell gave it, which is §1's "every control and lane a page names is on screen at once";
/// anything over it is page content sitting off-screen behind a scrollbar.
fn page_travel(window: &adw::ApplicationWindow, page: Page) -> f64 {
    let name = format!("page-scroll-{}", page.label());
    let scroller = common::widget_in(window, &name)
        .unwrap_or_else(|| {
            panic!("the shell wraps {page:?}'s content in a named `{name}` scroller")
        })
        .downcast::<gtk::ScrolledWindow>()
        .expect("the page's wrapper is a gtk::ScrolledWindow");
    let adjustment = scroller.vadjustment();
    (adjustment.upper() - adjustment.page_size()).max(0.0)
}

/// The four tabs, each measured after being shown, against §1's minimum.
fn check_every_page_shrinks(app: &adw::Application) {
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, "Prepare");
    lay_out(&window);

    for page in [Page::Prepare, Page::Cut, Page::Narrate, Page::Produce] {
        ui::tab_button(&window, page)
            .unwrap_or_else(|| panic!("the shell has a {page:?} tab"))
            .emit_clicked();
        lay_out(&window);
        let (min_w, min_h) = window_floor(&window);
        let travel = page_travel(&window, page);
        let mut held: Vec<(String, i32, i32)> = Vec::new();
        floors(&visible_page(&window), &mut held);
        floors(&bottom_box(&window), &mut held);
        held.sort_by_key(|(_, w, h)| -(w.max(h)));
        eprintln!(
            "{page:?}: window minimum {min_w}x{min_h}, page scroll travel {travel:.0} px\n  widest/tallest holders:\n{}",
            held.iter()
                .take(12)
                .map(|(name, w, h)| format!("    {name}: min {w}x{h}"))
                .collect::<Vec<_>>()
                .join("\n")
        );
        // §1: the floor IS the spec's number. Below it would mean `build_window` stopped asking for it;
        // above it means this page holds the shell wider or taller than the size §1 fixes.
        assert_eq!(
            min_w,
            naivepost::shell::WINDOW_MIN_W,
            "the {page:?} page moves the window's minimum WIDTH off §1's 1240 px (measured {min_w}): \
             the top holders are {:?}",
            &held[..held.len().min(6)]
        );
        assert_eq!(
            min_h,
            naivepost::shell::WINDOW_MIN_H,
            "the {page:?} page moves the window's minimum HEIGHT off §1's 900 px (measured {min_h}): \
             the top holders are {:?}",
            &held[..held.len().min(6)]
        );
        assert_eq!(
            (window.default_width(), window.default_height()),
            (
                naivepost::shell::WINDOW_MIN_W,
                naivepost::shell::WINDOW_MIN_H
            ),
            "§1: the window OPENS at its minimum"
        );
        assert!(
            travel <= 1.0,
            "§1: at the minimum size every control and lane a page names is on screen at once, but \
             {page:?}'s own scroller still has {travel:.0} px to scroll: the tallest holders are {:?}",
            &held[..held.len().min(6)]
        );
    }
    window.close();
}

#[test]
fn sec_03_shell_1_the_window_the_window_opens_at_its_minimum_and_no_page_scrolls_as_a_whole() {
    common::pin_private_cwd();
    common::one_app_round(|app| {
        check_every_page_shrinks(app);
        RAN_PAGES.store(true, Ordering::SeqCst);
    });
    assert!(
        RAN_PAGES.load(Ordering::SeqCst),
        "the shrinkability round never ran"
    );
}
