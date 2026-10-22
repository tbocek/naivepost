//! The geometry one screen of §03–§09 owes the user: every control of a laid-out page **inside** the
//! window, with nothing hidden and nothing clipped.
//!
//! §03's rule is written at the size §01#1 sets — "every control of the page that is up is ON SCREEN
//! at 1280×900: the page area does not scroll as a whole (a column that does not fit scrolls **inside
//! itself**)". §05#4 says the same of the Cut page's three columns, and each page repeats it in its
//! own §1. This is the one place that wording is turned into a measurement, so every page's test states
//! the rule once and calls here rather than re-deriving it: one page that stops fitting fails the same
//! way and says which named widget is off the bottom.
//!
//! Test-only on purpose: measuring a widget tree is not app behaviour, so none of this is reachable from
//! `src/`.
//!
//! The page and the bottom band are the two halves of the shell's divider, and the band is what makes
//! this tight: with the log closed the band is the run bar and the log's header line; at the minimum
//! 900-px height that leaves the page a little over 340 px. A column that spills is not a bug to fix
//! by growing the window — at 1280×900 the window is the size §01 says to open it at, so the columns
//! have to fit inside what is left.

use gtk4 as gtk;
use gtk4::prelude::*;

/// The size §01#1 says the app opens at, and the minimum it never goes below — the same pair
/// `sec_03_shell_1_the_window.rs` measures, and the size a snapshot lays out at.
pub const WINDOW_SIZE: (i32, i32) = (1280, 900);

/// How far past a boundary counts as clipped. A row's last line of text can overrun the box that holds
/// it by a pixel or two depending on the font; a whole control off screen is far past this.
pub const SLACK_PX: f64 = 4.0;

/// Show a window at the size §01 opens it at and wait until GTK has laid it out there. A window that was
/// never mapped has the geometry of an empty box, and every measurement below would be vacuous.
pub fn show_and_lay_out(window: &adw::ApplicationWindow) {
    window.set_default_size(WINDOW_SIZE.0, WINDOW_SIZE.1);
    window.present();
    super::wait_for_stable_layout(window);
    let surface = window.surface().expect("a presented window has a surface");
    assert_eq!(
        (surface.width(), surface.height()),
        WINDOW_SIZE,
        "the window laid out at another size than it opens at: its content asks for more"
    );
}

/// Every widget inside `root` that has a name. `widget_name` does not inherit, so this walks the whole
/// subtree; the tests find one widget by name through `ui::find_source_widget`, and these checks need
/// every name on a page.
pub fn named_inside(root: &gtk4::Widget) -> Vec<(String, gtk4::Widget)> {
    let mut out = Vec::new();
    walk(root, &mut out);
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

fn walk(widget: &gtk4::Widget, out: &mut Vec<(String, gtk4::Widget)>) {
    let name = widget.widget_name();
    if !name.is_empty() {
        out.push((name.to_string(), widget.clone()));
    }
    if let Some(scroller) = widget.downcast_ref::<gtk4::ScrolledWindow>() {
        if let Some(child) = scroller.child() {
            walk(child.upcast_ref(), out);
        }
        return;
    }
    if let Some(list_box) = widget.downcast_ref::<gtk4::ListBox>() {
        let model = list_box.observe_children();
        for i in 0..model.n_items() {
            if let Some(row) = model.item(i).and_then(|o| o.downcast::<gtk4::Widget>().ok()) {
                walk(&row, out);
            }
        }
        return;
    }
    if let Some(paned) = widget.downcast_ref::<gtk4::Paned>() {
        if let Some(start) = paned.start_child() {
            walk(start.upcast_ref(), out);
        }
        if let Some(end) = paned.end_child() {
            walk(end.upcast_ref(), out);
        }
        return;
    }
    if let Some(box_) = widget.downcast_ref::<gtk::Box>() {
        let model = box_.observe_children();
        for i in 0..model.n_items() {
            if let Some(child) = model.item(i).and_then(|o| o.downcast::<gtk4::Widget>().ok()) {
                walk(&child, out);
            }
        }
    }
}

/// A widget's box in window coordinates: what is actually on screen, read through `compute_bounds` so a
/// widget inside a scroller reports where it sits and not where its scrolled content starts.
pub fn window_box(widget: &gtk4::Widget) -> (f64, f64, f64, f64) {
    widget
        .compute_bounds(&widget.clone().upcast::<gtk4::Widget>())
        .map_or(
            (0.0, 0.0, f64::from(widget.width()), f64::from(widget.height())),
            |r| {
                (
                    f64::from(r.x()),
                    f64::from(r.y()),
                    f64::from(r.width()),
                    f64::from(r.height()),
                )
            },
        )
}

/// The page `page_name` ("Prepare", "Cut", …): the widget the tab shows. Every page is a `gtk::Box`
/// added to the shell's ViewStack by name, so this is the box whose children §1's geometry rules talk about.
pub fn page_of(window: &adw::ApplicationWindow, page_name: &str) -> gtk4::Widget {
    let stack = find_stack(window.upcast_ref())
        .and_then(|w| w.downcast::<adw::ViewStack>().ok())
        .unwrap_or_else(|| panic!("no view stack in the window"));
    let page = stack
        .child_by_name(page_name)
        .unwrap_or_else(|| panic!("no page named {page_name}"));
    // The window may be showing another tab: a tab's contents are only laid out while it is the visible
    // one (that is what `per_tab_stack` buys the shell — each page measured on its own), so a check of
    // page X has to put the window on page X first.
    if stack.visible_child().as_ref() != Some(page.upcast_ref()) {
        stack.set_visible_child_name(page_name);
        super::wait_for_stable_layout(window);
    }
    page
}

/// Every direct child of any container, found the only way that works for all of them: the
/// `gtk_widget_first_child`/`next_sibling` pair, which GTK fills in whatever the container's own kind.
fn widget_children(widget: &gtk4::Widget) -> Vec<gtk4::Widget> {
    let mut out = Vec::new();
    let mut cursor = gtk4::Widget::first_child(widget);
    while let Some(child) = cursor {
        out.push(child.clone());
        cursor = child.next_sibling();
    }
    out
}

/// Depth-first search for the ViewStack: the pages are the stack's children, and the stack is buried
/// under the header/toolbar/content boxes, so the walk has to know every container the shell builds with.
fn find_stack(root: &gtk4::Widget) -> Option<gtk4::Widget> {
    if root.downcast_ref::<adw::ViewStack>().is_some() {
        return Some(root.clone());
    }
    // `observe_children` hands back a `ListModel` (not an iterator), and only `gtk::Box` exposes it.
    let kids: Vec<gtk4::Widget> = if let Some(b) = root.downcast_ref::<gtk::Box>() {
        let model = b.observe_children();
        (0..model.n_items())
            .filter_map(|i| model.item(i).and_then(|o| o.downcast::<gtk4::Widget>().ok()))
            .collect()
    } else {
        let mut kids: Vec<gtk4::Widget> = [
            root.downcast_ref::<gtk::CenterBox>().and_then(|c| c.start_widget()),
            root.downcast_ref::<gtk::CenterBox>().and_then(|c| c.center_widget()),
            root.downcast_ref::<gtk::CenterBox>().and_then(|c| c.end_widget()),
            root.downcast_ref::<gtk::HeaderBar>().and_then(|h| h.title_widget()),
            root.downcast_ref::<gtk::Paned>().and_then(|p| p.start_child()),
            root.downcast_ref::<gtk::Paned>().and_then(|p| p.end_child()),
            root.downcast_ref::<gtk::ScrolledWindow>().and_then(|s| s.child()),
            root.downcast_ref::<gtk::Overlay>().and_then(|o| o.child()),
        ]
        .into_iter()
        .flatten()
        .collect();
        // Buttons and labels on a header bar (New/Open/Save, the tab row) live in its own internal
        // box, which GTK exposes as the bar's descendants: walk them generically.
        kids.extend(widget_children(root));
        kids
    };
    for child in kids {
        if let Some(found) = find_stack(&child) {
            return Some(found);
        }
    }
    None
}

/// Fail if any named widget of `page` sits outside the window. `page_label` names the page in the
/// message; the failure names the control.
pub fn assert_all_inside(page_label: &str, window: &adw::ApplicationWindow, page: &gtk4::Widget) {
    let (_, _, win_w, win_h) = window_box(window.upcast_ref());
    assert!(
        win_w > 100.0 && win_h > 100.0,
        "{page_label}: the window was never allocated a real size, so \"nothing clipped\" would prove nothing"
    );
    let mut problems: Vec<String> = Vec::new();
    for (name, widget) in named_inside(page) {
        let (x, y, w, h) = window_box(&widget);
        if w <= 0.0 || h <= 0.0 {
            // Not drawn at all: §1's "nothing hidden" is about controls the page is showing. A widget
            // the page deliberately keeps off (a second toolbar row, §06#1) reports 0×0 and is that
            // page's own rule, checked where that rule is written.
            continue;
        }
        if x + SLACK_PX < 0.0
            || y + SLACK_PX < 0.0
            || x + w - SLACK_PX > win_w
            || y + h - SLACK_PX > win_h
        {
            problems.push(format!(
                "`{name}` sits at x={x:.0}..{:.0}, y={y:.0}..{:.0} while the window is 0..{win_w:.0}, 0..{win_h:.0}",
                x + w,
                y + h
            ));
        }
    }
    assert!(
        problems.is_empty(),
        "{page_label} is clipped at {}x{} — {} widget(s) fall outside the window:\n  {}",
        WINDOW_SIZE.0,
        WINDOW_SIZE.1,
        problems.len(),
        problems.join("\n  ")
    );
}

/// True when some `ScrolledWindow` above `widget` in the tree would scroll it: that is the box §1's
/// "scrolls inside itself" refers to, and the only reason content past the window's bottom is not lost.
pub fn under_a_scroller(widget: &gtk4::Widget) -> bool {
    widget
        .ancestor(gtk4::ScrolledWindow::static_type())
        .is_some()
}
