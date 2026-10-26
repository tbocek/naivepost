// 03-shell §1's margins: "nothing on a page or in the bottom bar touches the window's edge or a divider. A page
// keeps 12 px from the header bar above it (no more: a deeper band reads as an empty border) and at least 12 px
// from the window's left and right edges and from each side of a column divider; the bottom bar keeps 12 px from
// the window's left, right and bottom edges and 8 px under the divider above it, with 8 px between the controls
// of its run bar row and 8 px between that row and the log header under it; a text box … keeps 8 px between its
// frame and its text". Measured on the realized window, on every tab, at §1's minimum size.

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::shell::Page;
use naivepost::ui;

use crate::common;

const ITEM: &str = "sec_03_shell_1_the_window";
static RAN: AtomicBool = AtomicBool::new(false);

/// Every widget under `node`, at any depth.
fn all(node: &gtk::Widget, out: &mut Vec<gtk::Widget>) {
    let mut child = node.first_child();
    while let Some(c) = child {
        child = c.next_sibling();
        all(&c, out);
        out.push(c);
    }
}

fn check_margins(app: &adw::Application) {
    let model = naivepost::project::load(&common::fixture_dir()).expect("the fixture loads");
    let window = ui::build_window(app, &model, "Prepare");
    common::size_to_minimum(&window);
    common::wait_for_stable_layout(&window);
    let root: gtk::Widget = window.clone().upcast();
    let width = window.width() as f32;
    let bar = ui::find_source_widget(&window, "bottom-bar").expect("the bottom bar");
    let at = bar
        .compute_bounds(&root)
        .expect("the bottom bar is laid out");
    assert!(
        at.x() >= 12.0 && at.x() + at.width() <= width - 12.0,
        "{ITEM}: the bottom bar keeps 12 px from both window edges: {at:?} in {width} px"
    );
    let height = window.height() as f32;
    // The divider's 1 px handle comes out of the band (`rest_position` places it at `total − band`), so the
    // margin reads 11 to 13 px.
    let bottom_gap = height - (at.y() + at.height());
    assert!(
        (11.0..=13.0).contains(&bottom_gap),
        "{ITEM}: the bottom bar keeps 12 px from the window's bottom edge: {bottom_gap} px under {at:?} in {height} px"
    );
    let drawn = |name: &str| {
        ui::find_source_widget(&window, name)
            .and_then(|w| w.compute_bounds(&root))
            .unwrap_or_else(|| panic!("{ITEM}: {name} is laid out"))
    };
    let (row, log) = (drawn("run-bar"), drawn("log-expander"));
    let rows_gap = log.y() - (row.y() + row.height());
    assert!(
        (8.0..=9.0).contains(&rows_gap),
        "{ITEM}: 8 px between the run bar row and the log header: {rows_gap} px"
    );
    let (play, stop) = (drawn("play-button"), drawn("stop-button"));
    let controls_gap = stop.x() - (play.x() + play.width());
    assert!(
        (8.0..=9.0).contains(&controls_gap),
        "{ITEM}: 8 px between ▶ and ⏹: {controls_gap} px"
    );
    let mut widgets = Vec::new();
    all(&root, &mut widgets);
    let header = widgets
        .iter()
        .find(|w| w.is::<adw::HeaderBar>())
        .and_then(|w| w.compute_bounds(&root))
        .expect("the header bar is laid out");
    for page in [Page::Prepare, Page::Narrate, Page::Produce] {
        ui::tab_button(&window, page)
            .expect("the shell has the tab")
            .emit_clicked();
        common::wait_for_stable_layout(&window);
        // The page's first drawn row sits 12 px under the header bar: no deeper empty band above it.
        let scroller = ui::find_source_widget(&window, &format!("page-scroll-{}", page.label()))
            .expect("the page has its scroller");
        let mut inside = Vec::new();
        all(&scroller, &mut inside);
        let first = inside
            .iter()
            .filter(|w| {
                w.is_visible()
                    && w.parent().is_some_and(|p| {
                        p.is::<gtk::Box>() && p.parent().is_some_and(|v| v.is::<gtk::Viewport>())
                    })
            })
            .filter_map(|w| w.compute_bounds(&root))
            .map(|b| b.y())
            .fold(f32::MAX, f32::min);
        let top_gap = first - (header.y() + header.height());
        assert!(
            (12.0..=13.0).contains(&top_gap),
            "{ITEM}: {page:?} starts {top_gap} px under the header bar, §1 wants 12"
        );
        let mut widgets = Vec::new();
        all(&root, &mut widgets);
        for view in widgets
            .iter()
            .filter_map(|w| w.downcast_ref::<gtk::TextView>())
        {
            assert!(
                [
                    view.left_margin(),
                    view.right_margin(),
                    view.top_margin(),
                    view.bottom_margin()
                ]
                .iter()
                .all(|m| *m >= 8),
                "{ITEM}: {} on {page:?} keeps its text 8 px off its frame",
                view.widget_name()
            );
        }
        // Each column divider on the visible page: its two sides keep 12 px each off it.
        for paned in widgets
            .iter()
            .filter_map(|w| w.downcast_ref::<gtk::Paned>())
        {
            if paned.orientation() != gtk::Orientation::Horizontal || !paned.is_mapped() {
                continue;
            }
            let (Some(start), Some(end)) = (paned.start_child(), paned.end_child()) else {
                continue;
            };
            let (Some(a), Some(b)) = (start.compute_bounds(&root), end.compute_bounds(&root))
            else {
                continue;
            };
            let gap = b.x() - (a.x() + a.width());
            assert!(
                gap >= 24.0,
                "{ITEM}: the columns of {} on {page:?} are {gap} px apart, the divider needs 12 on each side",
                paned.widget_name()
            );
        }
    }
    RAN.store(true, Ordering::SeqCst);
    window.close();
    common::settle();
}

#[test]
fn sec_03_shell_1_the_window_margins_nothing_touches_an_edge_or_a_divider() {
    common::pin_private_cwd();
    common::one_app_round(check_margins);
    assert!(
        RAN.load(Ordering::SeqCst),
        "{ITEM}: the margin check never ran"
    );
}
