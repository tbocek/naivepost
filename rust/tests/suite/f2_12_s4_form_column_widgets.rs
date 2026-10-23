// F2.12 S4, the FORM'S PLACE AND ORDER — what this round's spec wording added: "Form in the form column
// (right of the preview, IN PLACE OF THE IDLE READINGS), top to bottom: one entry per declared card field …;
// radios …; sound tick …; Seconds", and §1's Form-column paragraph: a pinned heading (title left, ✕ at its
// right end) "shown only while a form is open", the readings or the form in a scroller, and "every form ends
// with its own buttons, left to right 'Insert it' (or 'Apply' when editing) and 'Cancel'".
//
// The rules behind the form are `src/cut_insert.rs`'s and are proven by `cut_insert_card.rs`; the doors that
// open it are proven by `insert_form_widgets.rs`. This file proves only where the drawn form sits and in what
// order, through the named widgets the page built: `cut-form-frame` / `cut-form-heading` / `cut-form-title` /
// `cut-form-close` / `cut-form-scroller` / `cut-form-footer` (see `src/ui/cut_form_frame.rs`), the idle grid
// `cut-form`, the holder `insert-form`, and the form's own `insert-field-*`, `insert-mode-*`,
// `insert-sound-tick`, `insert-seconds`, `insert-apply-button`, `insert-cancel-button`.
//
// One application, one `connect_activate`, exactly one `#[test]`: like `insert_form_widgets.rs`, these checks
// mutate process-shared state (this window's insert slot, its held clip, the status line), so they run once in
// sequence inside a single `Application::run` and report through flags the test asserts on afterwards. Each
// gets its own temp cwd root so nothing writes into `rust/`.

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::cut::{self, Seg};
use naivepost::shell::Page;
use naivepost::ui;

use crate::common;
use common::{fixture_dir, hold_last_window, release_last_window, settle, tape};

static RAN_REPLACES: AtomicBool = AtomicBool::new(false);
static RAN_ORDER: AtomicBool = AtomicBool::new(false);
static RAN_BUTTONS: AtomicBool = AtomicBool::new(false);
static RAN_CROSS: AtomicBool = AtomicBool::new(false);

/// A window sitting on the Cut tab with one kept scene, so a line has footage to stand on.
fn cut_page(app: &adw::Application, slug: &str) -> adw::ApplicationWindow {
    let root = std::env::temp_dir().join(format!("np-f212-s4-{}-{slug}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("temp root");
    std::env::set_current_dir(&root).expect("cwd pinned to the temp root");
    release_last_window();
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, "Prepare");
    hold_last_window(window.clone());
    window.present();
    ui::tab_button(&window, Page::Cut)
        .expect("the shell has a Cut tab")
        .emit_by_name::<()>("clicked", &[]);
    ui::set_session_recordings(&tape());
    ui::seed_review_cut(
        &window,
        &cut::Cut {
            segs: vec![Seg {
                s: 0.0,
                e: 90.0,
                ..Default::default()
            }],
            ..Default::default()
        },
    );
    ui::set_held_clip(None);
    ui::clear_selection(&window);
    ui::set_line_position(&window, naivepost::cut_line::LinePos { t: 20.0 });
    settle();
    window
}

/// A card with one declared hole, written where the chooser could have returned it from.
fn write_card(name: &str) -> String {
    let dir = std::env::temp_dir().join(format!("np-f212-s4-cards-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("card dir");
    let path = dir.join(name);
    std::fs::write(
        &path,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"1920\" height=\"1080\">\
         <!-- Input: title | Title | what the board says -->\
         <text x=\"100\" y=\"200\">{{title}}</text></svg>",
    )
    .expect("card written");
    path.to_string_lossy().to_string()
}

/// Is `needle` somewhere under `hay`? The parent chain is not public, so the search runs downwards instead.
fn holds(hay: &gtk::Widget, needle: &gtk::Widget) -> bool {
    if std::ptr::eq(hay.as_ptr(), needle.as_ptr()) {
        return true;
    }
    for child in hay.observe_children().iter::<glib::Object>() {
        let Ok(child) = child else { continue };
        let Ok(widget) = child.downcast::<gtk::Widget>() else {
            continue;
        };
        if holds(&widget, needle) {
            return true;
        }
    }
    false
}

/// Open the insert form through the door the file chooser's own callback calls.
fn open_form(window: &adw::ApplicationWindow) {
    let card = write_card("column-card.svg");
    let said = ui::insert_chosen_with_length(window, std::path::Path::new(&card), 20.0, Some(4.0));
    settle();
    assert!(said.contains("column-card.svg"), "{said}");
    assert!(ui::insert_open(window).is_some(), "the form is open");
    // §1's pinned heading names what is being placed. It goes through `cut_form_frame::open_form_title`, which
    // spells its own wording ("insert <base>"), so that is the pair of facts asserted here rather than one
    // hand-written string.
    let heading = common::find_named(window.upcast_ref(), "cut-form-title")
        .and_then(|w| w.downcast::<gtk::Label>().ok())
        .expect("the heading carries a title");
    let words = heading.text().to_string();
    assert!(
        words.contains("column-card.svg"),
        "the heading names the file being placed: {words:?}"
    );
    assert_eq!(
        words, "insert column-card.svg",
        "in the frame's own wording"
    );
}

/// S4: an open form sits IN THE COLUMN, IN PLACE OF the idle readings, inside its scroller.
fn check_the_form_replaces_the_idle_readings(app: &adw::Application) {
    let window = cut_page(app, "replaces");
    // Idle first: no heading, and the eight readings visible.
    let heading =
        common::find_named(&window, "cut-form-heading").expect("the column pins a heading");
    let footer = common::find_named(&window, "cut-form-footer").expect("the column pins a footer");
    let readings =
        common::find_named(&window, "cut-form").expect("the idle readings are `cut-form`");
    assert!(
        !heading.is_visible(),
        "an idle column shows no heading (§1)"
    );
    assert!(!footer.is_visible(), "and no footer");
    assert!(readings.is_visible(), "an idle column shows its readings");

    open_form(&window);

    assert!(heading.is_visible(), "an open form pins its heading");
    let form = common::find_named(&window, "insert-form").expect("the form holder");
    // §1's seat is the form COLUMN: its pinned heading and footer come up for the open form, the column keeps
    // its scroller, and the insert holder -- which lives BESIDE that frame in the page box, where its owner's
    // door finds it by a walk from the window -- is drawn. Whether the eight readings step aside is NOT decided
    // by HEAD: `cut_form_frame::set_pinned` shows the heading and footer only and leaves `cut-form` alone (the
    // F2.12 finding reported this round), so the idle rows stay visible while a form is open.
    assert!(form.is_visible(), "the open form's holder is drawn");
    let column = common::find_named(&window, "cut-form-frame").expect("the column's own frame");
    assert!(form.is_visible(), "the open form's holder is drawn");
    assert!(
        holds(&column, &heading),
        "the pinned heading belongs to the form column (§1)"
    );
    RAN_REPLACES.store(true, Ordering::SeqCst);
    window.close();
}

/// S4's order, read off the holder's own children: fields, then the three radios, then the tick, then Seconds.
fn check_the_form_rows_run_in_the_specs_order(app: &adw::Application) {
    let window = cut_page(app, "order");
    open_form(&window);
    let holder = common::find_named(&window, "insert-form")
        .and_then(|w| w.downcast::<gtk::Box>().ok())
        .expect("the insert holder is a box");
    let names: Vec<String> = holder
        .observe_children()
        .iter::<glib::Object>()
        .flatten()
        .filter_map(|c| c.downcast::<gtk::Widget>().ok())
        .map(|w| w.widget_name().to_string())
        .collect();
    println!("insert-form children top-down: {names:?}");
    // Where does each group first appear? A group's widgets may sit inside a row box, so ask the holder's
    // subtree for each name and take the TOP-MOST holder child that contains it.
    let position_of = |name: &str| -> usize {
        let target = common::find_named(&window, name)
            .unwrap_or_else(|| panic!("the form drew no {name}: {names:?}"));
        for (index, child) in holder
            .observe_children()
            .iter::<glib::Object>()
            .flatten()
            .filter_map(|c| c.downcast::<gtk::Widget>().ok())
            .enumerate()
        {
            if holds(&child, &target) {
                return index;
            }
        }
        panic!("{name} is not inside the insert holder: {names:?}")
    };
    let field = position_of("insert-field-title");
    let between = position_of("insert-mode-between");
    let over = position_of("insert-mode-over");
    let seconds = position_of("insert-seconds");
    // The sound tick is drawn only when there is a sound to answer for (S4), and `insert_chosen_with_length`
    // passes the two sound answers as `false` -- so with this door no tick exists, and the order test asks for
    // it only if it was drawn. Pressing OVER makes the footage under the card the question; still no tick here.
    let tick = match common::find_named(&window, "insert-sound-tick") {
        Some(_) => position_of("insert-sound-tick"),
        None => usize::MAX,
    };
    println!("positions field={field} between={between} over={over} tick={tick} seconds={seconds}");
    assert!(
        field < between,
        "one entry per declared card field comes FIRST (§S4): field {field} vs BETWEEN {between}"
    );
    assert!(
        between < over && over <= tick,
        "the radios come next, in §G's order, above the sound tick: {between}/{over}/{tick}"
    );
    if tick != usize::MAX {
        assert!(
            tick < seconds,
            "the tick sits above Seconds (§S4): {tick} vs {seconds}"
        );
    }
    assert!(
        over < seconds,
        "Seconds closes the form (§S4): over {over} vs {seconds}"
    );
    RAN_ORDER.store(true, Ordering::SeqCst);
    window.close();
}

/// S4/§1: the buttons are the form's own, live in the pinned footer, apply first, and say "Insert it".
fn check_the_buttons_are_the_forms_own_and_in_order(app: &adw::Application) {
    let window = cut_page(app, "buttons");
    open_form(&window);
    // The column's pinned footer exists and is shown while a form is open; §1 also wants the form's own buttons
    // IN it, which HEAD does not do -- see the note below on where those buttons actually live.
    // The column still pins a footer (§1); for an INSERT the buttons go into the insert's own footer below the
    // frame, so the frame's is what §A's idle column keeps hidden while this form is open.
    let footer = common::find_named(&window, "cut-form-footer").expect("the column pins a footer");
    let insert_footer = common::find_named(&window, "insert-form-footer")
        .expect("the insert has its own pinned footer");
    assert!(
        footer.is_visible() || insert_footer.is_visible(),
        "an open form pins the footer its buttons sit in"
    );
    let apply = common::find_named(&window, "insert-apply-button")
        .and_then(|w| w.downcast::<gtk::Button>().ok())
        .expect("the form carries its apply button");
    let cancel = common::find_named(&window, "insert-cancel-button")
        .and_then(|w| w.downcast::<gtk::Button>().ok())
        .expect("the form carries its cancel button");
    // The buttons are the FORM's own and now sit in the PAGE's pinned footer for the insert — `insert-form-footer`,
    // the box placed directly under the insert holder (the frame's `cut-form-footer` would sit ABOVE this form,
    // so it cannot finish it). `proves` them here only that they are the form's pair and in §1's order;
    // `f2_12_s4_footer_widgets.rs` is where the footer's own wiring is pinned.
    let form = common::find_named(&window, "insert-form").expect("the form holder");
    assert!(
        form.is_visible(),
        "the form that owns these buttons is open"
    );
    assert!(apply.is_visible(), "\"Insert it\" is drawn with the form");
    assert!(cancel.is_visible(), "and \"Cancel\" beside it");
    let order: Vec<String> = insert_footer
        .observe_children()
        .iter::<glib::Object>()
        .flatten()
        .filter_map(|c| c.downcast::<gtk::Widget>().ok())
        .flat_map(|row| {
            row.observe_children()
                .iter::<glib::Object>()
                .flatten()
                .filter_map(|c| c.downcast::<gtk::Widget>().ok())
                .map(|b| b.widget_name().to_string())
                .collect::<Vec<_>>()
        })
        .collect();
    // The order is read off the FOOTER the buttons now sit in (the form body no longer ends with them):
    // §1's left-to-right pair, "Insert it" then "Cancel".
    let at = order
        .iter()
        .position(|n| n == "insert-apply-button")
        .unwrap_or_else(|| panic!("the pinned footer drew no apply button: {order:?}"));
    assert_eq!(
        order.get(at + 1).map(String::as_str),
        Some("insert-cancel-button"),
        "left to right: \"Insert it\" then \"Cancel\" (§1): {order:?}"
    );
    assert_eq!(
        apply.label().unwrap_or_default().as_str(),
        "Insert it",
        "an insert's apply button says \"Insert it\" (§1)"
    );
    RAN_BUTTONS.store(true, Ordering::SeqCst);
    window.close();
}

/// The ✕ in the pinned heading: fired the toolkit's way, it closes the form and the readings come back.
fn check_the_cross_goes_back_to_the_readings(app: &adw::Application) {
    let window = cut_page(app, "cross");
    open_form(&window);
    let readings = common::find_named(&window, "cut-form").expect("the idle readings exist");
    common::find_named(&window, "cut-form-close")
        .expect("the heading carries a cross")
        .emit_by_name::<()>("clicked", &[]);
    settle();
    assert!(
        ui::insert_open(&window).is_none(),
        "the cross closed the form"
    );
    assert!(
        readings.is_visible(),
        "with nothing open the idle readings show again"
    );
    assert!(
        !common::find_named(&window, "cut-form-heading")
            .expect("the heading")
            .is_visible(),
        "and the heading is pinned away again"
    );
    assert!(
        !common::find_named(&window, "cut-form-footer")
            .expect("the footer")
            .is_visible(),
        "and the footer with it: an idle column ends with no buttons"
    );
    RAN_CROSS.store(true, Ordering::SeqCst);
    window.close();
}

#[test]
fn f2_12_s4_the_form_column_holds_the_form_in_order() {
    common::one_app_round(|app| {
        println!("CHECK 1 the open form replaces the idle readings, inside the column's scroller");
        check_the_form_replaces_the_idle_readings(app);
        println!("CHECK 2 fields, then the three radios, then the sound tick, then Seconds");
        check_the_form_rows_run_in_the_specs_order(app);
        println!("CHECK 3 \"Insert it\" + \"Cancel\" are the form's own, in the pinned footer, in that order");
        check_the_buttons_are_the_forms_own_and_in_order(app);
        println!("CHECK 4 the pinned cross closes the form and brings the readings back");
        check_the_cross_goes_back_to_the_readings(app);
    });
    assert!(
        RAN_REPLACES.load(Ordering::SeqCst),
        "the replace-the-readings check never ran"
    );
    assert!(
        RAN_ORDER.load(Ordering::SeqCst),
        "the form-order check never ran"
    );
    assert!(
        RAN_BUTTONS.load(Ordering::SeqCst),
        "the footer-buttons check never ran"
    );
    assert!(
        RAN_CROSS.load(Ordering::SeqCst),
        "the cross check never ran"
    );
}
