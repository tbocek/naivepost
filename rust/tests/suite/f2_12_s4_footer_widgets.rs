// F2.12 S4/S5, THE FORM'S OWN PINNED FOOTER — what this round's fix added: the buttons that finish an insert
// are no longer the last rows of the scrolling body (where they fell below the painted frame and could not be
// seen or clicked), they go into the page's pinned footer through `cut_form_frame::set_footer`, and while a
// form is open the idle readings step aside (`cut_form_frame::set_readings`, §S4's "in place of the idle
// readings"). The rules behind the form stay `src/cut_insert.rs`'s; `insert_form_widgets.rs` proves the doors
// and `f2_12_s4_form_column_widgets.rs` proves the row order. This file proves only the footer: where the
// buttons sit, and that clicking them does what Apply and Cancel do.
//
// Widgets fired by name: `insert-apply-button`, `insert-cancel-button`; widgets asserted on:
// `insert-form-footer` (the page's own footer box under the insert holder), `cut-form` (the idle grid),
// `insert-heading`, `insert-seconds`. The footer is named apart from §A's `cut-form-footer` because the frame's
// footer would sit ABOVE the insert form, not under it.
//
// One application, one `connect_activate`, exactly one `#[test]`: like its siblings, these checks mutate
// process-shared state (this window's insert slot, its held clip, the status line), so they run once in
// sequence inside a single `Application::run` and report through flags the test asserts on afterwards. Each
// gets its own temp cwd root so nothing writes into `rust/`.

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::cut::{self, Seg};
use naivepost::shell::Page;
use naivepost::ui;

use crate::common;
use common::{fixture_dir, hold_last_window, release_last_window, settle, status_text, tape};

static RAN_READINGS: AtomicBool = AtomicBool::new(false);
static RAN_FOOTER: AtomicBool = AtomicBool::new(false);
static RAN_APPLY: AtomicBool = AtomicBool::new(false);
static RAN_CANCEL: AtomicBool = AtomicBool::new(false);

/// A window sitting on the Cut tab with one kept scene, so a line has footage to stand on.
fn cut_page(app: &adw::Application, slug: &str) -> adw::ApplicationWindow {
    let root = std::env::temp_dir().join(format!("np-f212-foot-{}-{slug}", std::process::id()));
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
            segs: vec![Seg { s: 0.0, e: 90.0, ..Default::default() }],
            ..Default::default()
        },
    );
    ui::set_held_clip(None);
    ui::clear_selection(&window);
    // A claimed line: without it every door answers §S1's refusal and no form is drawn at all.
    ui::note_place(true);
    ui::set_line_position(&window, naivepost::cut_line::LinePos { t: 20.0 });
    settle();
    window
}

/// A card with a moving moment of its own, so the form has a length of its own to offer. The same file the
/// `05-insert-form` snapshot stages: an SVG with an `<animateTransform>` ending at 1.6 s.
fn write_timed_card() -> String {
    let dir = std::env::temp_dir().join(format!("np-f212-foot-cards-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("card dir");
    let path = dir.join("timed.svg");
    std::fs::write(
        &path,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"1920\" height=\"1080\" viewBox=\"0 0 1920 1080\">\
         <!-- Input: title | Title | what the card says -->\
         <rect width=\"1920\" height=\"1080\" fill=\"#0d1117\"/>\
         <g transform=\"translate(0,40)\"><text x=\"960\" y=\"720\" font-size=\"240\" text-anchor=\"middle\" fill=\"#ffffff\">{{title}}</text>\
         <animateTransform attributeName=\"transform\" type=\"translate\" values=\"0 40;0 0\" keyTimes=\"0;1\" dur=\"1.6s\" fill=\"freeze\" additive=\"sum\"/></g></svg>",
    )
    .expect("timed card written");
    path.to_string_lossy().to_string()
}


/// Is `needle` somewhere under `hay`? The parent chain is not public, so the search runs downwards instead.
fn holds(hay: &gtk::Widget, needle: &gtk::Widget) -> bool {
    if std::ptr::eq(hay.as_ptr(), needle.as_ptr()) {
        return true;
    }
    for child in hay.observe_children().iter::<glib::Object>() {
        let Ok(child) = child else { continue };
        let Ok(widget) = child.downcast::<gtk::Widget>() else { continue };
        if holds(&widget, needle) {
            return true;
        }
    }
    false
}

/// Open the insert form through the door the file chooser's own callback calls. The card is the animated one:
/// the snapshot's own `05-insert-form` stage, and the only staging that actually reaches the form drawing —
/// the chooser seam refuses a card with no length of its own, so a plain still never draws a form at all.
fn open_form(window: &adw::ApplicationWindow) {
    let card = write_timed_card();
    let said = ui::insert_chosen_with_length(window, std::path::Path::new(&card), 20.0, None);
    settle();
    assert!(said.contains("timed.svg"), "{said}");
    assert!(ui::insert_open(window).is_some(), "the form is open: {said}");
}

/// This window's cut as a whole, so its segments are read back the way the page measures them.
fn cut_of(window: &adw::ApplicationWindow) -> cut::Cut {
    cut::Cut {
        segs: ui::review_cut_segs(window),
        lanes: ui::review_lanes(window),
        ..Default::default()
    }
}

/// §S4: opening a form puts it IN PLACE OF the idle readings — the eight-row grid is not drawn while the
/// form is, and comes back when the form closes.
fn check_the_form_replaces_the_idle_readings(app: &adw::Application) {
    let window = cut_page(app, "readings");
    let readings = common::find_named(&window, "cut-form").expect("the idle readings are `cut-form`");
    assert!(readings.is_visible(), "an idle column shows its readings");

    open_form(&window);

    assert!(
        !readings.is_visible(),
        "§S4: the form sits in the form column IN PLACE OF THE IDLE READINGS, so the grid is not drawn"
    );
    assert!(
        common::find_named(&window, "insert-heading").is_some(),
        "the form drew its own heading line"
    );
    assert!(
        common::find_named(&window, "insert-seconds").is_some(),
        "the form drew its Seconds entry"
    );

    common::find_named(&window, "insert-cancel-button")
        .expect("the form carries its cancel button")
        .emit_by_name::<()>("clicked", &[]);
    settle();
    assert!(
        readings.is_visible(),
        "closing the form brings the idle readings back (§S4)"
    );
    RAN_READINGS.store(true, Ordering::SeqCst);
    window.close();
}

/// §05#1: the buttons live in the PINNED footer, not at the end of the scrolling body — found by walking the
/// footer's own children, which is what "pinned" has to mean for them to be on screen.
fn check_the_footer_holds_the_buttons_that_place_the_insert(app: &adw::Application) {
    let window = cut_page(app, "footer");
    open_form(&window);
    let footer = common::find_named(&window, "insert-form-footer").expect("the page pins a footer for the insert");
    assert!(
        footer.is_visible(),
        "§05#1: an open form pins its footer, so its buttons are on screen and clickable"
    );
    let apply = common::find_named(&window, "insert-apply-button")
        .and_then(|w| w.downcast::<gtk::Button>().ok())
        .expect("the form carries its apply button");
    let cancel = common::find_named(&window, "insert-cancel-button")
        .and_then(|w| w.downcast::<gtk::Button>().ok())
        .expect("the form carries its cancel button");
    assert!(
        holds(&footer, apply.upcast_ref()),
        "§05#1's pinned footer holds the button that places the insert"
    );
    assert!(
        holds(&footer, cancel.upcast_ref()),
        "and the button that refuses it"
    );
    assert_eq!(
        apply.label().unwrap_or_default().as_str(),
        "Insert it",
        "an insert's apply button says \"Insert it\" (§1)"
    );
    // The body no longer ends with them: the scroller's content is the form alone.
    let holder = common::find_named(&window, "insert-form").expect("the insert holder");
    assert!(
        !holds(&holder, apply.upcast_ref()),
        "the buttons left the scrolling body for the pinned footer"
    );
    RAN_FOOTER.store(true, Ordering::SeqCst);
    window.close();
}

/// The WIRE: `insert-apply-button` fired the toolkit's way places the card, says so, and puts the column back.
fn check_clicking_insert_it_places_the_card(app: &adw::Application) {
    let window = cut_page(app, "apply");
    open_form(&window);
    let before = cut_of(&window);
    assert_eq!(before.segs.len(), 1, "one kept scene before the insert");

    common::find_named(&window, "insert-apply-button")
        .expect("the footer holds the apply button")
        .emit_by_name::<()>("clicked", &[]);
    settle();

    let said = status_text(&window);
    assert!(
        said.contains("inserted at") && said.contains("\u{21b6} Undo"),
        "the press said what it did, in cut_insert's own words: {said}"
    );
    let after = cut_of(&window);
    assert_eq!(after.segs.len(), 2, "a BETWEEN insert splices one more segment: {after:?}");
    assert!(
        after.segs.iter().any(|seg| seg.is_insert()),
        "the new segment is the spliced card (s == e with a dur): {after:?}"
    );
    assert!(ui::insert_open(&window).is_none(), "Apply closed the form");
    assert!(
        !common::find_named(&window, "insert-form-footer")
            .expect("the footer")
            .is_visible(),
        "and took its pinned footer away with it"
    );
    assert!(
        common::find_named(&window, "cut-form")
            .expect("the idle readings")
            .is_visible(),
        "and brought the idle readings back (§S4)"
    );
    RAN_APPLY.store(true, Ordering::SeqCst);
    window.close();
}

/// The other button: Cancel closes the form and writes nothing.
fn check_clicking_cancel_closes_the_form_without_placing(app: &adw::Application) {
    let window = cut_page(app, "cancel");
    open_form(&window);
    let before = cut_of(&window);

    common::find_named(&window, "insert-cancel-button")
        .expect("the footer holds the cancel button")
        .emit_by_name::<()>("clicked", &[]);
    settle();

    let after = cut_of(&window);
    assert_eq!(
        after.segs.len(),
        before.segs.len(),
        "Cancel places nothing: {:?}",
        after.segs
    );
    assert!(ui::insert_open(&window).is_none(), "Cancel closed the form");
    assert!(
        !common::find_named(&window, "insert-form-footer")
            .expect("the footer")
            .is_visible(),
        "and hid its pinned footer"
    );
    assert!(
        common::find_named(&window, "cut-form")
            .expect("the idle readings")
            .is_visible(),
        "and brought the idle readings back (§S4)"
    );
    let said = status_text(&window);
    assert!(
        said.contains("insert") && said.contains("cancelled"),
        "the press said what it did: {said}"
    );
    RAN_CANCEL.store(true, Ordering::SeqCst);
    window.close();
}

#[test]
fn f2_12_s4_the_pinned_footer_places_the_insert() {
    common::one_app_round(|app| {
        println!("CHECK 1 the open form replaces the idle readings (§S4)");
        check_the_form_replaces_the_idle_readings(app);
        println!("CHECK 2 the pinned footer holds Insert it + Cancel (§05#1)");
        check_the_footer_holds_the_buttons_that_place_the_insert(app);
        println!("CHECK 3 clicking Insert it places the card through the footer button");
        check_clicking_insert_it_places_the_card(app);
        println!("CHECK 4 clicking Cancel closes the form and writes nothing");
        check_clicking_cancel_closes_the_form_without_placing(app);
    });
    assert!(RAN_READINGS.load(Ordering::SeqCst), "the replace-the-readings check never ran");
    assert!(RAN_FOOTER.load(Ordering::SeqCst), "the pinned-footer check never ran");
    assert!(RAN_APPLY.load(Ordering::SeqCst), "the apply-click check never ran");
    assert!(RAN_CANCEL.load(Ordering::SeqCst), "the cancel-click check never ran");
}
