//! §F5.7 "Words on the thumbnail" (S5) through the REAL drag over the picture, which is the half of the
//! item no other file drives: the four corners a drag ends with are the ones the gesture sees, so a
//! regression here is invisible to the logic tests in `produce_screen.rs`.
//!
//! Three clauses, one test each:
//! - the box's corners line up with the PICTURE's edges and middles when they land within §A's
//!   `OVERLAY_SNAP_PX`, and a drag nowhere near a target is left alone (a snap that swallowed every drag
//!   would look identical to a working one in the positive case alone);
//! - corners line up with ANOTHER box's edges, which is the half of "the picture's and each other's"
//!   that needs the marks the page already holds;
//! - a box under §A's `OVERLAY_MIN_PX` floor is a look, not a box: nothing is stored and no dialog opens.
//!
//!   The fourth test pins the dialog §A draws: heading, explanation, the words box, and Remove / Cancel /
//!   Save in that order, answered by its real response the way `produce_screen_widgets.rs` answers a reword.
//!
//! ONE `Application::run` per binary, so the checks run back to back inside `run_round`, each recording
//! its own failure instead of poisoning the next. cwd is pinned to this test's private temp folder
//! BEFORE the run: the session folder a window writes is relative to cwd, and a nested
//! `<...>/session.naivepost/session.naivepost` is what a cwd already inside a project produces.

use std::sync::Mutex;

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::project::TextMark;
use naivepost::shell::Page;
use naivepost::{produce_screen, ui};
use crate::common;
use common::{
    fixture_dir, hold_last_window, pin_private_cwd, release_last_window, session_tree_of as session,
    settle, wait_until, widget_in,
};

thread_local! {
    static CURRENT_APP: std::cell::RefCell<Option<adw::Application>> =
        const { std::cell::RefCell::new(None) };
    /// Failures are collected, not thrown: the round runs every check so one red clause still reports
    /// the others.
    static FAILS: Mutex<Vec<String>> = const { Mutex::new(Vec::new()) };
}

fn fail(why: String) {
    FAILS.with(|held| held.lock().expect("failure log").push(why));
}

fn take_fails() -> Vec<String> {
    FAILS.with(|held| std::mem::take(&mut *held.lock().expect("failure log")))
}

/// cwd in its own folder under the pinned private cwd, so each check's session files cannot collide.
fn in_root(name: &str) {
    let base = std::env::current_dir().expect("cwd pinned before the run");
    let root = base.join(name);
    std::fs::create_dir_all(&root).expect("check root");
    std::env::set_current_dir(&root).expect("cwd to the check root");
}

/// §1's "a session folder in the working directory" without a project path: `set_state` + `refresh`
/// read the folder's narration state, and a folder that is no tree at all makes `refresh` return early.
fn session_here() {
    let cwd = std::env::current_dir().expect("cwd pinned before the run");
    session(&cwd);
}

fn produce_window(app: &adw::Application) -> adw::ApplicationWindow {
    release_last_window();
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, Page::Produce.label());
    hold_last_window(window.clone());
    // §13's rule for a drawn page: a window sized after `present()` never re-negotiates its rows.
    window.set_default_size(1280, 980);
    window.present();
    settle();
    // The page's wires ran once for the first window the process built, so the drag lives on THAT tree;
    // `draw_into` finds the plate through the held window, so this window has to be the held one or the
    // gesture below drags a plate of another window's size.
    ui::produce_page::hold_window_for_test(&window);
    window
}

/// Publish a page state carrying exactly the given word boxes, and repaint so the chips and the plate
/// show them. Repainting needs the window: `refresh` is the page's own door.
fn publish_texts(window: &adw::ApplicationWindow, texts: Vec<TextMark>) {
    let state = ui::produce_page::ProduceState { texts, ..Default::default() };
    ui::produce_page::set_state(state);
    ui::produce_page::refresh(window);
}

fn plate_size(window: &adw::ApplicationWindow) -> (f64, f64) {
    let picture = widget_in(window, "thumbnail-picture").expect("the Produce page draws the plate");
    let bound = picture.compute_bounds(&picture).expect("a measured plate");
    (f64::from(bound.width()), f64::from(bound.height()))
}

/// Fire the picture's own drag the way the toolkit does: `drag-begin` carries the ABSOLUTE press and
/// `drag-end` only the OFFSET from it (F2.8's rule, the same one `zoom_drag_widgets.rs` follows).
fn drag(window: &adw::ApplicationWindow, from: (f64, f64), to: (f64, f64)) {
    let gesture = ui::produce_words_press::words_gesture_for_test(window)
        .expect("the page wired a drag onto the thumbnail picture");
    gesture.emit_by_name::<()>("drag-begin", &[&from.0, &from.1]);
    gesture.emit_by_name::<()>("drag-end", &[&(to.0 - from.0), &(to.1 - from.1)]);
    settle();
}

/// The dialog, once it has actually appeared: the wait is on the fact itself, never on a pause.
fn opened_dialog() -> adw::MessageDialog {
    wait_until("the words dialog opened", || {
        ui::produce_words::open_dialog().is_some()
    });
    ui::produce_words::open_dialog().expect("the dialog is held by the module that opened it")
}

fn answer(dialog: &adw::MessageDialog, response: &str, words: &str) {
    let view = dialog
        .extra_child()
        .and_then(|child| child.downcast::<gtk::Frame>().ok())
        .and_then(|frame| frame.child())
        .and_then(|child| child.downcast::<gtk::ScrolledWindow>().ok())
        .and_then(|scroller| scroller.child())
        .and_then(|child| child.downcast::<gtk::TextView>().ok())
        .expect("the dialog holds its multi-line editor");
    view.buffer().set_text(words);
    dialog.response(response);
    settle();
}

/// A mark's left / right edge and top / bottom edge, in the plate's own pixels — the units §A's
/// snap distance and minimum box are stated in.
fn edges(mark: &TextMark, wide: f64, high: f64) -> (f64, f64, f64, f64) {
    let w = mark.wf * wide;
    let h = mark.hf * high;
    (
        mark.cx * wide - w / 2.0,
        mark.cx * wide + w / 2.0,
        mark.cy * high - h / 2.0,
        mark.cy * high + h / 2.0,
    )
}

fn near(value: f64, target: f64) -> bool {
    (value - target).abs() <= 1.0
}

/// The nearest of the plate's own three x-targets to a pixel, in pixels.
fn distance_to_plate_target(value: f64, wide: f64) -> f64 {
    [0.0, wide / 2.0, wide]
        .iter()
        .map(|target| (value - target).abs())
        .fold(f64::INFINITY, f64::min)
}

/// §A: "boxes snap to the picture's and each other's edges and middles". A corner inside
/// `OVERLAY_SNAP_PX` of a target lands ON it; a corner further away keeps the pixel it was drawn at.
fn held_app() -> adw::Application {
    APP.with(|held| held.borrow().clone().expect("run_round published the application"))
}

fn f5_7_s5_a_dragged_box_snaps_to_the_pictures_edges_and_middles() {
    session_here();
    let window = produce_window(&held_app());
    let (wide, high) = plate_size(&window);
    println!(
        "f5_7_s5 | plate measures {wide:.1} x {high:.1} px; §A snap radius {} px, floor {} px",
        produce_screen::OVERLAY_SNAP_PX, produce_screen::OVERLAY_MIN_PX
    );
    publish_texts(&window, vec![]);
    let snap = produce_screen::OVERLAY_SNAP_PX;
    // Left corner 6 px right of the plate's left edge; right corner 4 px left of its middle. Both are
    // inside the radius, and neither is the pixel a hand released on.
    let from = (6.0, high * 0.2);
    let to = (wide / 2.0 - 4.0, high * 0.45);
    drag(&window, from, to);
    let dialog = opened_dialog();
    answer(&dialog, "save", "snapped line");
    let marks = ui::produce_page::read_state().texts;
    if marks.len() != 1 {
        return fail(format!("one drag must place exactly one box, got {}", marks.len()));
    }
    let (left, right, _, _) = edges(&marks[0], wide, high);
    println!(
        "f5_7_s5 | drawn left {:+.1} -> stored left {left:.1} (plate edge 0); drawn right {:.1} -> stored right {right:.1} (plate middle {:.1})",
        from.0, to.0, wide / 2.0
    );
    if !near(left, 0.0) {
        return fail(format!("left corner {left:.1} px did not snap to the plate's left edge 0"));
    }
    if !near(right, wide / 2.0) {
        return fail(format!(
            "right corner {right:.1} px did not snap to the plate's middle {:.1}",
            wide / 2.0
        ));
    }
    // And the release that started the box must not have been nudged past the radius in the other
    // direction: the width is exactly the plate's half, so the snap moved each corner by its own slack.
    let width = right - left;
    if !near(width, wide / 2.0) {
        return fail(format!("the snapped box is {width:.1} px wide, not half the plate ({:.1})", wide / 2.0));
    }

    // A free drag: both corners well outside the radius of every target (edges 0 / mid / right, and no
    // marks are published), so nothing may move.
    publish_texts(&window, vec![]);
    let free_from = (wide * 0.25 + snap + 30.0, high * 0.6);
    let free_to = (wide * 0.25 + snap + 120.0, high * 0.8);
    drag(&window, free_from, free_to);
    let dialog = opened_dialog();
    answer(&dialog, "save", "free line");
    let marks = ui::produce_page::read_state().texts;
    if marks.len() != 1 {
        return fail(format!("the free drag must place one box, got {}", marks.len()));
    }
    let (left, right, _, _) = edges(&marks[0], wide, high);
    println!(
        "f5_7_s5 | free drag stored left {left:.1} ({:.1} px from the nearest plate target) and right {right:.1} ({:.1} px from the nearest)",
        distance_to_plate_target(left, wide),
        distance_to_plate_target(right, wide)
    );
    if distance_to_plate_target(left, wide) <= snap || distance_to_plate_target(right, wide) <= snap {
        return fail(format!(
            "a drag {:.1} px from every target was swallowed: left {left:.1}, right {right:.1}",
            distance_to_plate_target(left, wide).min(distance_to_plate_target(right, wide))
        ));
    }
    if !near(left, free_from.0) || !near(right, free_to.0) {
        fail(format!(
            "a free drag must keep its own pixels: drawn {} and {}, stored {left:.1} and {right:.1}",
            free_from.0, free_to.0
        ));
    }
}

fn f5_7_s5_a_dragged_box_snaps_to_another_boxes_edge() {
    session_here();
    let window = produce_window(&held_app());
    let (wide, high) = plate_size(&window);
    // One box already on the plate: centred, 40 % wide, so its right edge sits at 0.7 of the plate.
    let existing = TextMark {
        cx: 0.5,
        cy: 0.5,
        wf: 0.4,
        hf: 0.2,
        text: "already there".into(),
    };
    publish_texts(&window, vec![existing.clone()]);
    let (ex_left, ex_right, _, _) = edges(&existing, wide, high);
    println!(
        "f5_7_s5 | existing box edges: left {ex_left:.1} right {ex_right:.1} px on a {wide:.1} px plate"
    );
    // A new box whose RIGHT corner lands 5 px short of the existing box's right edge, inside the radius,
    // while its left corner is nowhere near a target.
    let from = (wide * 0.15, high * 0.7);
    let to = (ex_right - 5.0, high * 0.9);
    drag(&window, from, to);
    let dialog = opened_dialog();
    answer(&dialog, "save", "aligned line");
    let marks = ui::produce_page::read_state().texts;
    if marks.len() != 2 {
        return fail(format!("the drag adds a box beside the existing one, got {}", marks.len()));
    }
    let (new_left, new_right, _, _) = edges(&marks[1], wide, high);
    println!("f5_7_s5 | new box right edge {new_right:.1} px vs the existing box's {ex_right:.1} px");
    if !near(new_right, ex_right) {
        return fail(format!(
            "the new box's right edge {new_right:.1} did not line up with the existing box's {ex_right:.1}"
        ));
    }
    if near(new_left, ex_left) || near(new_left, 0.0) || near(new_left, wide / 2.0) || near(new_left, wide) {
        return fail(format!(
            "the new box's left edge {new_left:.1} was dragged onto a target it was not near"
        ));
    }
    // The earlier box is untouched: a placed box is not moved by placing another.
    let (still_left, still_right, _, _) = edges(&marks[0], wide, high);
    if !near(still_left, ex_left) || !near(still_right, ex_right) {
        fail("placing a second box moved the first one".to_string());
    }
}

fn f5_7_s5_a_box_under_the_minimum_is_not_a_box() {
    session_here();
    let window = produce_window(&held_app());
    let (wide, high) = plate_size(&window);
    publish_texts(&window, vec![]);
    let floor = produce_screen::OVERLAY_MIN_PX;
    // 20 px wide, plenty tall: the smaller direction is what §A's floor measures.
    drag(&window, (wide * 0.2, high * 0.2), (wide * 0.2 + floor - 8.0, high * 0.7));
    if ui::produce_words::open_dialog().is_some() {
        return fail("a box under the floor opened the words dialog".to_string());
    }
    if !ui::produce_page::read_state().texts.is_empty() {
        return fail("a box under the floor stored a mark".to_string());
    }
    println!("f5_7_s5 | a {} px-wide box: no dialog, no mark", floor - 8.0);
    // Just over the floor: the same drag shape now counts as a box.
    drag(&window, (wide * 0.2, high * 0.2), (wide * 0.2 + floor + 8.0, high * 0.7));
    let dialog = opened_dialog();
    answer(&dialog, "save", "just big enough");
    let marks = ui::produce_page::read_state().texts;
    if marks.len() != 1 {
        return fail(format!(
            "a box of {} px must land; the page holds {} marks",
            floor + 8.0,
            marks.len()
        ));
    }
    println!(
        "f5_7_s5 | a {} px-wide box landed: stored width {:.1} px",
        floor + 8.0,
        marks[0].wf * wide
    );
}

/// The dialog's three answers are buttons in its action area, which `adw::MessageDialog` does not expose
/// by id (`response_widget` belongs to the GtkDialog this replaced). Reading the labels off the real
/// buttons is the only way to pin §A's order — "Remove, Cancel, Save" — from the widget side.
fn answer_labels(dialog: &adw::MessageDialog) -> Vec<String> {
    fn walk(node: &gtk::Widget, out: &mut Vec<String>) {
        if let Some(button) = node.downcast_ref::<gtk::Button>() {
            if let Some(label) = button.label() {
                if ["Remove", "Cancel", "Save"].contains(&label.as_str()) {
                    out.push(label.to_string());
                }
            }
        }
        if let Some(child) = node.first_child() {
            let mut cursor = Some(child);
            while let Some(current) = cursor {
                walk(&current, out);
                cursor = current.next_sibling();
            }
        }
    }
    let mut labels = Vec::new();
    walk(dialog.upcast_ref(), &mut labels);
    labels
}

/// §A's dialog shape, read off the real box the drag opened: heading text, the dim explanation, the
/// multi-line editor, and the three answers in Remove / Cancel / Save order. Save goes through the
/// dialog's own response, the route `produce_screen_widgets.rs` uses for a reword.
fn f5_7_s5_the_dialog_offers_remove_cancel_and_save_in_order() {
    session_here();
    let window = produce_window(&held_app());
    let (wide, high) = plate_size(&window);
    publish_texts(&window, vec![]);
    drag(&window, (wide * 0.1, high * 0.1), (wide * 0.6, high * 0.4));
    let dialog = opened_dialog();
    if dialog.property::<String>("heading").as_str() != naivepost::ui::produce_words::DIALOG_TITLE {
        return fail(format!(
            "the dialog's heading is {:?}, not §A's heading {:?}",
            dialog.property::<String>("heading"),
            naivepost::ui::produce_words::DIALOG_TITLE
        ));
    }
    let body = dialog.body();
    if body.trim().is_empty() {
        return fail("the dialog has no explanation line above the box".to_string());
    }
    let labels = answer_labels(&dialog);
    println!(
        "f5_7_s5 | a NEW box's answers: [{}]; explanation {} chars",
        labels.join(", "),
        body.chars().count()
    );
    // A NEW box has no Remove: the destructive answer belongs to a box that already exists. Save and
    // Cancel are the pair this dialog is drawn with, and they are the right-hand two of §A's three.
    if labels != ["Remove", "Cancel", "Save"] && labels != ["Cancel", "Save"] {
        return fail(format!(
            "the dialog's answers are [{}], which is not §A's order Remove, Cancel, Save",
            labels.join(", ")
        ));
    }
    // Save through the widget's response, and the words land in the box the drag drew.
    answer(&dialog, "save", "typed over the drag");
    let marks = ui::produce_page::read_state().texts;
    if marks.len() != 1 || marks[0].text != "typed over the drag" {
        return fail(format!(
            "Save did not store the words the drag placed: {:?}",
            marks.iter().map(|m| &m.text).collect::<Vec<_>>()
        ));
    }
    if ui::produce_words::open_dialog().is_some() {
        return fail("answering left the dialog standing; the next press would be refused by its guard".to_string());
    }
    // Now a box EXISTS: reopening it (the ✎ chip the refresh drew) must offer Remove first.
    let chip = widget_in(&window, "words-chip-0").expect("every box on the plate gets its ✎ chip");
    chip.downcast_ref::<gtk::Button>()
        .expect("the chip is a button")
        .emit_by_name::<()>("clicked", &[]);
    settle();
    let reword = opened_dialog();
    let labels = answer_labels(&reword);
    println!("f5_7_s5 | an EXISTING box's answers: [{}]", labels.join(", "));
    if labels != ["Remove", "Cancel", "Save"] {
        return fail(format!(
            "an existing box's dialog answers are [{}], not §A's Remove, Cancel, Save",
            labels.join(", ")
        ));
    }
    reword.response("cancel");
    settle();
    assert!(ui::produce_words::open_dialog().is_none(), "Cancel closes the dialog");
}

thread_local! {
    static APP: std::cell::RefCell<Option<adw::Application>> = const { std::cell::RefCell::new(None) };
}

fn run_round(app: &adw::Application) {
    APP.with(|held| *held.borrow_mut() = Some(app.clone()));
    for (name, check) in [
        (
            "picture snap",
            f5_7_s5_a_dragged_box_snaps_to_the_pictures_edges_and_middles as fn(),
        ),
        ("box-to-box snap", f5_7_s5_a_dragged_box_snaps_to_another_boxes_edge),
        ("minimum box", f5_7_s5_a_box_under_the_minimum_is_not_a_box),
        (
            "dialog answers",
            f5_7_s5_the_dialog_offers_remove_cancel_and_save_in_order,
        ),
    ] {
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            in_root(name.replace(' ', "_").as_str());
            check()
        }));
        if outcome.is_err() {
            // The panic message already went to stderr when it unwound; record which clause it belongs to.
            fail(format!("{name}: panicked (see the panic message above)"));
        }
        release_last_window();
        settle();
    }
}

#[test]
fn f5_7_s5_the_checks_run_through_the_real_drag() {
    pin_private_cwd();
    let app = adw::Application::builder()
        .application_id("org.example.naivepost.f57ssnap")
        .build();
    app.connect_activate(run_round);
    app.run_with_args::<&str>(&[]);
    let fails = take_fails();
    assert!(
        fails.is_empty(),
        "F5.7 S5 checks failed:\n{}",
        fails.join("\n")
    );
}
