//! F2.10 (Cameras and hearing) — "row 1 at the top, each further row below the one before", measured on a
//! realized page rather than in the colouring's arithmetic (`f2_10_row_order.rs` pins that half).
//!
//! `spec/img/05-rows.png` puts camera 1's plate (**1**) ABOVE camera 2's (**2**), so the order is something a
//! person reads off the screen: the row a status calls "row 1" must be the topmost box in `camera-rows`, and
//! its plate must name camera 1. The band paints rows the same way (`cut_tracks::Rows`: a row's y is
//! `pictures + index * row_h`, so index 0 is topmost), which is what makes the plate list and the pictures
//! agree about which row is which.
//!
//! Staged through `naivepost::snapshot_rows::stage` — the recipe door `just snapshot 05-rows` uses — so the
//! two cameras, the −19.00 s shift and the watched row come from the program's own staging rather than from
//! a state poked here. One live scenario per test: the stage re-points the process-wide cwd and fills
//! process-wide page slots, so each check gets its own temp folder (the shape `f2_10_rows_shot.rs` uses).

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::shell::Page;
use naivepost::snapshot_rows;
use naivepost::ui;

use crate::common;
use common::{fixture_dir, hold_last_window, release_last_window, settle, status_text};

static RAN_ORDER: AtomicBool = AtomicBool::new(false);
static RAN_WIRE: AtomicBool = AtomicBool::new(false);

/// A Cut-page window over its own temp session folder, so the stage has somewhere to write its lane file and
/// its one extracted frame without racing another check in this binary.
fn shot_window(app: &adw::Application, slug: &str) -> adw::ApplicationWindow {
    let root = std::env::temp_dir().join(format!(
        "naivepost-f210-order-{}-{slug}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    let session = naivepost::startup::session_dir(&root);
    std::fs::create_dir_all(session.join("cut")).expect("a session cut folder");
    std::fs::create_dir_all(session.join("prepare")).expect("a session prepare folder");
    std::env::set_current_dir(&root).expect("cwd pinned to the temp root");
    release_last_window();
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, "Prepare");
    hold_last_window(window.clone());
    window.present();
    ui::tab_button(&window, Page::Cut)
        .expect("the shell has a Cut tab")
        .emit_by_name::<()>("clicked", &[]);
    settle();
    window
}

fn find_named(window: &adw::ApplicationWindow, name: &str) -> gtk::Widget {
    fn walk(root: &gtk::Widget, name: &str) -> Option<gtk::Widget> {
        if root.widget_name() == name {
            return Some(root.clone());
        }
        for child in root.observe_children().iter::<glib::Object>() {
            let Ok(child) = child else { continue };
            let Ok(widget) = child.downcast::<gtk::Widget>() else { continue };
            if let Some(found) = walk(&widget, name) {
                return Some(found);
            }
        }
        None
    }
    walk(window.upcast_ref(), name).unwrap_or_else(|| panic!("no widget named `{name}` on the page"))
}

/// A widget's top edge in window coordinates — the one frame every widget on the page shares.
#[allow(deprecated)] // 4.12's replacement needs a surface node this tree does not hand out
fn top_edge(widget: &gtk::Widget) -> i32 {
    let native = widget.native().expect("the page is inside a realized window");
    let (_, y) = widget
        .translate_coordinates(&native, 0.0, 0.0)
        .expect("the widget is laid out");
    y as i32
}

/// A row's name plate label, read off the widget the page built.
fn plate(window: &adw::ApplicationWindow, name: &str) -> String {
    ui::line_step_button(window, name)
        .unwrap_or_else(|| panic!("the row list drew no `{name}` plate"))
        .label()
        .unwrap_or_default()
        .to_string()
}

/// "row 1 at the top, each further row below the one before": the FIRST child of `camera-rows` is
/// `camera-row-1`, it sits above `camera-row-2`, and the plates say which camera each is — so the claim is
/// checked by name and by y, not by the numbering alone.
fn check_the_row_plates_run_row_one_at_the_top(app: &adw::Application) {
    let window = shot_window(app, "order");
    let said = snapshot_rows::stage(&window, &fixture_dir());
    println!("stage said: {}", said.join(" | "));
    settle();

    let rows_box = find_named(&window, "camera-rows");
    let order: Vec<String> = rows_box
        .observe_children()
        .iter::<glib::Object>()
        .filter_map(Result::ok)
        .filter_map(|child| child.downcast::<gtk::Widget>().ok())
        .map(|child| child.widget_name().to_string())
        .collect();
    println!("camera-rows children top-down: {order:?}");
    assert_eq!(
        order.first().map(String::as_str),
        Some("camera-row-1"),
        "the first row box is the one a status calls row 1: {order:?}"
    );

    let row1 = find_named(&window, "camera-row-1");
    let row2 = find_named(&window, "camera-row-2");
    let (y1, y2) = (top_edge(&row1), top_edge(&row2));
    println!("ROW ORDER camera-row-1 top y={y1}, camera-row-2 top y={y2}");
    assert!(
        y1 < y2,
        "row 1 is the TOP row, each further row below the one before: {y1} vs {y2}"
    );

    // And the plates carry the figure's own names: **1** camera 1, **2** camera 2 with its correction.
    let (first, second) = (plate(&window, "watch-row-1"), plate(&window, "watch-row-2"));
    println!("plates top-down: [{first}] [{second}]");
    assert_eq!(
        first, snapshot_rows::CAM1,
        "badge **1**: the TOP row's plate names camera 1"
    );
    assert!(
        second.starts_with(snapshot_rows::CAM2) && second.ends_with("-19.00 s"),
        "badge **2**: the row below names camera 2 with its shift correction: {second}"
    );
    RAN_ORDER.store(true, Ordering::SeqCst);
    window.close();
}

/// The wire behind S3, fired the toolkit's way: a click on the LOWER plate watches that row (state AND the
/// dashed outline), the status says once what the cut shows there, and ▶ hands the preview back.
fn check_clicking_the_top_plate_watches_the_top_row(app: &adw::Application) {
    let window = shot_window(app, "wire");
    snapshot_rows::stage(&window, &fixture_dir());
    settle();
    // The stage leaves row 2 watched (the figure's **4**); start from a known watch on row 1 so the press
    // below is the thing that moves it, not the staging.
    ui::press_watch_row(&window, 0);
    settle();
    assert_eq!(ui::preview_row(&window), 0, "starting point: row 1 watched");

    // The real click, on the row BELOW.
    ui::line_step_button(&window, "watch-row-2")
        .expect("watch-row-2")
        .emit_by_name::<()>("clicked", &[]);
    settle();
    let line = status_text(&window);
    println!("after clicking watch-row-2: watch={:?} status={line}", ui::preview_row(&window));
    assert_eq!(ui::preview_row(&window), 1, "the click watched the lower row");
    assert!(
        find_named(&window, "camera-row-2").has_css_class("watched-row"),
        "and the dashed outline moved onto it"
    );
    assert!(
        !find_named(&window, "camera-row-1").has_css_class("watched-row"),
        "off the row it was on"
    );
    // The kept scene is shown from camera 1 while the preview now looks at camera 2, so §'s **7** is owed.
    assert!(
        line.starts_with("watching camera 2"),
        "**7**: the status says which camera the cut shows where the line stands: {line}"
    );
    assert!(
        line.contains("▶ plays the cut"),
        "and says ▶ hands the preview back: {line}"
    );

    // ▶ takes the preview back to the cut — the same button a person presses.
    ui::play_cut_button(&window)
        .expect("the Cut page carries play-cut-button")
        .emit_by_name::<()>("clicked", &[]);
    settle();
    let after = status_text(&window);
    println!("after ▶: watch={:?} status={after}", ui::preview_row(&window));
    assert_ne!(
        ui::preview_row(&window),
        1,
        "▶ handed the watched row back to the cut (S3)"
    );
    assert!(
        !find_named(&window, "camera-row-2").has_css_class("watched-row"),
        "and the outline went with it"
    );
    RAN_WIRE.store(true, Ordering::SeqCst);
    window.close();
}

fn round() {
    common::one_app_round(|app| {
        println!("CHECK 1 row 1 is the top row, by box order, y and plate name");
        check_the_row_plates_run_row_one_at_the_top(app);
        println!("CHECK 2 the click on the lower plate watches it, and ▶ hands the preview back");
        check_clicking_the_top_plate_watches_the_top_row(app);
    });
}

#[test]
fn f2_10_s1_the_row_plates_run_row_one_at_the_top() {
    common::pin_private_cwd();
    round();
    assert!(RAN_ORDER.load(Ordering::SeqCst), "the row-order check never ran");
}

#[test]
fn f2_10_s3_clicking_the_top_plate_watches_the_top_row() {
    common::pin_private_cwd();
    round();
    assert!(RAN_WIRE.load(Ordering::SeqCst), "the watch-the-wire check never ran");
}
