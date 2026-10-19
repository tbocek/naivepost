//! The window must be SHRINKABLE: a GTK4 window cannot go below its content's minimum size, so a
//! single hard `set_size_request` anywhere in the tree pins the whole shell, and the user's complaint
//! — "I cannot make the window smaller" — is exactly that floor meeting the screen. spec/03-shell.md §1
//! gives the DEFAULT (1240×740); nothing in the spec gives a page the right to demand more than that.
//!
//! So this measures what the user actually hits: the window's own minimum width and height on every
//! page, plus the per-widget minimums that hold it there (printed with `--nocapture`, because when this
//! fails the useful half of the message is WHICH widget pins the floor).

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::shell::Page;
use naivepost::ui;
use crate::common;
use common::{fixture_dir, settle};

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
    for child in node.observe_children().iter::<glib::Object>().filter_map(Result::ok) {
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

/// Force a layout pass so every widget has been sized once, and report the tallest minimums among the
/// page's own descendants (the shell chrome — header, run bar, log — is measured by `window_floor`).
fn lay_out(window: &adw::ApplicationWindow) {
    window.present();
    settle();
    let widget = window.upcast_ref::<gtk::Widget>();
    if !widget.is_mapped() {
        widget.map();
    }
    // Give it a real box to lay its children out in; the numbers below are minimums, so the size chosen
    // here does not change them — it only makes GTK walk the tree.
    widget.size_allocate(&gtk::Allocation::new(0, 0, 1400, 900), -1);
    settle();
}

/// The visible page's content box: what the ViewStack shows right now. Measuring only this subtree is how
/// the test blames one page instead of the sum of all four (`AdwViewStack` measures homogeneously).
fn visible_page(window: &adw::ApplicationWindow) -> gtk::Widget {
    fn find_stack(node: &gtk::Widget) -> Option<adw::ViewStack> {
        if let Some(stack) = node.downcast_ref::<adw::ViewStack>() {
            return Some(stack.clone());
        }
        for child in node.observe_children().iter::<glib::Object>().filter_map(Result::ok) {
            if let Some(widget) = child.downcast_ref::<gtk::Widget>() {
                if let Some(stack) = find_stack(widget) {
                    return Some(stack);
                }
            }
        }
        None
    }
    let stack = find_stack(window.upcast_ref()).expect("the shell switches pages through an AdwViewStack");
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
        for child in node.observe_children().iter::<glib::Object>().filter_map(Result::ok) {
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

/// The four tabs, each measured after being shown. Bounds: 1024×640, comfortably inside the spec's
/// 1240×740 default — a window whose floor exceeded its own default size is the bug this test exists for.
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
        let mut held: Vec<(String, i32, i32)> = Vec::new();
        floors(&visible_page(&window), &mut held);
        floors(&bottom_box(&window), &mut held);
        held.sort_by_key(|(_, w, h)| -(w.max(h)));
        eprintln!(
            "{page:?}: window minimum {min_w}x{min_h}\n  widest/tallest holders:\n{}",
            held.iter()
                .take(12)
                .map(|(name, w, h)| format!("    {name}: min {w}x{h}"))
                .collect::<Vec<_>>()
                .join("\n")
        );
        assert!(
            min_w <= 1024,
            "the {page:?} page pins the window's minimum WIDTH at {min_w} px, above the 1024 px a \
             person must be able to shrink to (spec/03-shell.md §1 defaults the window to 1240x740): \
             the top holders are {:?}",
            &held[..held.len().min(6)]
        );
        assert!(
            min_h <= 640,
            "the {page:?} page pins the window's minimum HEIGHT at {min_h} px, above the 640 px a \
             person must be able to shrink to (spec/03-shell.md §1 defaults the window to 1240x740): \
             the top holders are {:?}",
            &held[..held.len().min(6)]
        );
    }
    window.close();
}

#[test]
fn every_page_leaves_the_window_shrinkable() {
    common::pin_private_cwd();
    common::one_app_round(|app| {
        check_every_page_shrinks(app);
        RAN_PAGES.store(true, Ordering::SeqCst);
    });
    assert!(RAN_PAGES.load(Ordering::SeqCst), "the shrinkability round never ran");
}
