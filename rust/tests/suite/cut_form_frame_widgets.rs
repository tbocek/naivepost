//! §05-cut#1-screen — the form column's PINNED parts. `spec/inventory/cut.md` §A: "Form column: pinned
//! heading (title + ✕ \"close this form — nothing is not already saved\"), a scroller holding either the
//! idle rows or a form body, pinned footer for buttons". The rules are `ui::cut_form_frame`'s; this proves
//! the widgets are really there and that a real click on the cross reaches the door that owns the open form.
//!
//! One application, one `connect_activate`, exactly one `#[test]`: these checks mutate process-shared state
//! (this window's insert slot and status line), so they run once inside a single `Application::run` and
//! report through flags the test asserts on — the shape `tests/suite/cut_screen_toolbar_widgets.rs` uses.

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::shell::Page;
use naivepost::ui;

use crate::common;
use common::{fixture_dir, hold_last_window, release_last_window, settle, status_text};

static RAN_FRAME: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static RAN_CLOSE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// First widget in the tree with this name.
fn find_named(window: &adw::ApplicationWindow, name: &str) -> Option<gtk::Widget> {
    fn walk(widget: &gtk::Widget, name: &str) -> Option<gtk::Widget> {
        if widget.widget_name() == name {
            return Some(widget.clone());
        }
        for child in widget.observe_children().iter::<glib::Object>() {
            let Ok(child) = child else { continue };
            let Some(child) = child.downcast_ref::<gtk::Widget>() else { continue };
            if let Some(found) = walk(child, name) {
                return Some(found);
            }
        }
        None
    }
    walk(window.upcast_ref(), name)
}

/// A window on the Cut page, cwd pinned to its own temp root so nothing writes into `rust/`.
fn cut_window(app: &adw::Application) -> adw::ApplicationWindow {
    release_last_window();
    let root = std::env::temp_dir().join(format!("cut-frame-w-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("temp root");
    std::env::set_current_dir(&root).expect("cwd pinned away from rust/");

    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, "Prepare");
    hold_last_window(window.clone());
    window.present();
    ui::tab_button(&window, Page::Cut)
        .expect("the shell has a Cut tab")
        .emit_clicked();
    assert_eq!(ui::state(&window).page, Page::Cut);
    settle();
    window
}

/// S15: the column is the three §A parts, and while nothing is being placed only the body shows.
fn check_the_column_pins_a_heading_and_a_footer(app: &adw::Application) {
    let window = cut_window(app);
    let scroller = find_named(&window, "cut-form-scroller").expect("§A's scroller holds the column body");
    let heading = find_named(&window, "cut-form-heading").expect("§A's pinned heading row");
    let footer = find_named(&window, "cut-form-footer").expect("§A's pinned button footer");
    assert!(
        find_named(&window, "cut-form-close").is_some(),
        "the heading carries the ✕ that closes it"
    );
    // The eight idle readings live INSIDE the scroller, which is what makes them scrollable under a pinned
    // heading rather than fixed to the column. A `ScrolledWindow` wraps its child in a viewport, so the
    // walk goes up until it reaches the scroller itself.
    let grid = find_named(&window, "cut-form").expect("the idle readings grid");
    let mut holder = grid.parent().expect("the grid sits inside something");
    while holder.widget_name() != "cut-form-scroller" {
        holder = holder.parent().unwrap_or_else(|| {
            panic!("the readings grid is not under the scroller: {:?}", grid.parent())
        });
    }
    for label in naivepost::cut_screen::IDLE_FORM_ROWS {
        let name = naivepost::cut_screen::readout_widget(label);
        assert!(find_named(&window, &name).is_some(), "no {name} readout under the scroller");
    }
    // Idle means no title and no buttons: the pinned parts exist but show nothing until a form opens.
    assert!(!heading.is_visible(), "an idle column shows no heading");
    assert!(!footer.is_visible(), "an idle column shows no footer");
    assert!(scroller.is_visible(), "the body is what an idle column shows");
    RAN_FRAME.store(true, std::sync::atomic::Ordering::SeqCst);
}

/// S16: open the insert form through the program's own door, then press the cross the toolkit's way and
/// assert the SAME state the logic test asserts — the insert slot empty and the form box hidden again.
fn check_the_pinned_cross_closes_the_open_form(app: &adw::Application) {
    let window = cut_window(app);
    // The chooser seam the F2.12 wire tests use: what the native dialog's callback calls.
    let video = {
        let dir = std::env::temp_dir().join("np-frame-cards");
        std::fs::create_dir_all(&dir).expect("video dir");
        let path = dir.join("sting.mp4");
        std::fs::write(&path, b"not really a video, only a name").expect("video written");
        path
    };
    let said = ui::insert_chosen_with_length(&window, &video, 20.0, Some(7.5));
    settle();
    assert!(said.contains("sting.mp4"), "{said}");
    assert!(ui::insert_open(&window).is_some(), "the insert form is open");
    let heading = find_named(&window, "cut-form-heading").expect("the pinned heading");
    let title = find_named(&window, "cut-form-title")
        .and_then(|w| w.downcast::<gtk::Label>().ok())
        .expect("the heading carries a title label");
    assert!(heading.is_visible(), "an open form pins its heading");
    assert!(
        title.text().contains("sting.mp4"),
        "the heading names what is being placed: {:?}",
        title.text()
    );
    // An insert's buttons sit in the INSERT's own pinned footer (`insert-form-footer`, the box under its form);
    // the frame's `cut-form-footer` is what every other form pins, so exactly one of the two is shown.
    let column_footer = find_named(&window, "cut-form-footer").expect("the column pins a footer");
    let insert_footer = find_named(&window, "insert-form-footer")
        .expect("the insert has its own pinned footer for its buttons");
    assert!(
        column_footer.is_visible() || insert_footer.is_visible(),
        "an open form pins the footer its buttons sit in"
    );

    // The cross, fired the way GTK fires a click.
    find_named(&window, "cut-form-close")
        .expect("the heading's ✕")
        .emit_by_name::<()>("clicked", &[]);
    settle();

    assert!(ui::insert_open(&window).is_none(), "the ✕ closed the insert form");
    assert!(
        !find_named(&window, "insert-form")
            .expect("the insert holder")
            .is_visible(),
        "the form box is hidden again"
    );
    assert!(!heading.is_visible(), "with nothing open the heading is pinned away again");
    assert!(!find_named(&window, "cut-form-footer").expect("the footer").is_visible());
    let said = status_text(&window);
    assert!(
        said.contains("insert") && said.contains("cancelled"),
        "the press said what it did: {said}"
    );
    RAN_CLOSE.store(true, std::sync::atomic::Ordering::SeqCst);
}

fn round() {
    common::one_app_round(|app| {
        println!("CHECK 1 the column pins a heading and a footer");
        check_the_column_pins_a_heading_and_a_footer(app);
        println!("CHECK 2 the pinned cross closes the open form");
        check_the_pinned_cross_closes_the_open_form(app);
    });
}

#[test]
fn sec_05_cut_1_screen_s15_and_s16_the_form_column_pins_a_heading_and_a_footer() {
    round();
    for (flag, what) in [
        (&RAN_FRAME, "the pinned-frame check"),
        (&RAN_CLOSE, "the cross-closes-the-form click check"),
    ] {
        assert!(flag.load(std::sync::atomic::Ordering::SeqCst), "{what} never ran");
    }
}
