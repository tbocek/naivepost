// F3.4 S3 — the words box's SHAPE in the form column, measured on a presented window.
//
// §S3 says "the words (3-line box, THE COLUMN'S FULL WIDTH, ABOVE THE OTHER FIELDS; Enter = new line)". The
// three-line height and Enter-as-newline have had owners (`caption_text_widgets.rs` pins the TextView type and
// `FORM_FIELDS`' order); what nothing pinned was the two placement promises: that the box takes the column's whole
// width instead of a fixed 220 px beside its label, and that it stands above Length rather than among them.
//
// Both are measured from `compute_bounds`, not read off an `hexpand()` flag: a widget can be told to expand and
// still be laid out narrow, which is exactly the bug this wording rules out. One application, one activate, one
// `#[test]`, harness copied from `caption_text_widgets.rs` (process-shared arm state means these blocks must run
// once, in sequence, inside a single `Application::run`).

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use gtk4 as gtk;

use naivepost::cut::Cut;
use naivepost::cut_line;
use naivepost::fx_text;
use naivepost::shell::Page;
use naivepost::ui;

use crate::common;
pub use common::find_in;
use common::{cut_page_settled_first as cut_page, settle, widget};

static RAN_WIDTH: AtomicBool = AtomicBool::new(false);
static RAN_ORDER: AtomicBool = AtomicBool::new(false);

/// How far a child may fall short of the column before it stops being "the column's full width". A GtkBox lays
/// its children at its own width minus nothing, but the holder carries spacing and the scroller a border, so a
/// small allowance is the difference between pinning the rule and pinning GTK's chrome.
const EDGE_TOLERANCE_PX: f32 = 12.0;

/// Arm ❝ Text through the real dropdown row and land a lower-third box by a zero-travel press at `t`, exactly
/// as `caption_text_widgets.rs` does — the same doors, so this file proves placement and nothing else.
/// The window is then WIDENED: `build_window` opens at the shell's own size and the Cut page's form column gets
/// whatever the paned leaves, which headless is nought (measured: holder w=0.0). A column with no width cannot
/// answer whether a child fills it, so the shot-gate convention from `snapshot.rs` applies here too.
fn land_form(app: &adw::Application, t: f64) -> adw::ApplicationWindow {
    let window = cut_page(app, &Cut::default());
    // The holder is hidden until its form opens, and GTK gives an invisible box no allocation at all (w=0.0
    // measured with the arm up but no box landed yet). So the size ask goes AFTER the landing below.
    ui::note_place(true);
    ui::set_line_position(&window, cut_line::LinePos { t });
    common::click(&window, "effect-item-text");
    assert!(ui::text_armed(), "❝ Text arms on the first press");
    let said = ui::text_drag_ended(&window, (40.0, 40.0), (40.0, 40.0));
    assert!(said.starts_with(&fx_text::form_title(t)), "{said}");
    // Now the column has a live child to lay out; ask for a real width so "fills the column" can be measured.
    window.set_default_size(1280, 800);
    // GTK lays the page out on an idle, not inside `settle()`'s pending-only drain, so wait for the fact itself
    // -- the form holder having a width -- rather than guessing how many turns the layout needs.
    let probe = form_holder(&window);
    common::wait_until(
        "the Cut page laid its form column out (holder still 0 px wide)",
        || probe.width() > 0,
    );
    window
}

/// Leave the arm and the form down so the next block starts clean (same tidy the sibling file uses).
fn disarm(window: &adw::ApplicationWindow) {
    if ui::text_armed() {
        let said = ui::press_text_cancel(window);
        assert_eq!(said, "left as it was \u{2014} no caption placed");
    }
    assert!(!ui::text_armed());
}

fn form_holder(window: &adw::ApplicationWindow) -> gtk::Box {
    widget(window, &format!("text-form-{}", Page::Cut.label()))
        .downcast::<gtk::Box>()
        .expect("the caption form holder is a Box")
}

/// A widget's bounds in the window's own coordinates. `compute_bounds` answers None until the widget shares a
/// realized root with the target, so a window that never measured is reported, not guessed at.
fn bounds(window: &adw::ApplicationWindow, name: &str) -> gtk::graphene::Rect {
    let target = widget(window, name);
    target
        .compute_bounds(window.upcast_ref::<gtk::Widget>())
        .unwrap_or_else(|| panic!("`{name}` answers no bounds on a presented window"))
}

/// §S3: the words box spans the FORM COLUMN — same left edge, same width, within the box's own chrome.
fn check_the_words_box_spans_the_form_column(app: &adw::Application) {
    let window = land_form(app, 30.0);
    let holder = form_holder(&window);
    let holder_box = holder
        .compute_bounds(window.upcast_ref::<gtk::Widget>())
        .expect("the form holder measures on a presented window");
    let words = bounds(&window, "text-field-words");
    println!(
        "column x={:.1} w={:.1} | words x={:.1} w={:.1}",
        holder_box.x(),
        holder_box.width(),
        words.x(),
        words.width()
    );
    // A zero-width column would make every assertion below pass by accident (0 >= 0 - 12), so the column being
    // laid out at all is itself part of what this check proves.
    assert!(
        holder_box.width() > 100.0,
        "the form column has a width to fill: {:.1} px",
        holder_box.width()
    );
    assert!(
        words.width() >= holder_box.width() - EDGE_TOLERANCE_PX,
        "the words box is the column's full width (§S3): {:.1} px in a {:.1} px column",
        words.width(),
        holder_box.width()
    );
    assert!(
        (words.x() - holder_box.x()).abs() <= EDGE_TOLERANCE_PX,
        "and starts at the column's left edge, not after a label column: x {:.1} vs {:.1}",
        words.x(),
        holder_box.x()
    );
    // Three rows tall is the other half of the sentence; the ROW COUNT is the rule, the pixel height is not.
    assert!(
        words.height() >= 18.0 * fx_text::WORDS_ROWS as f32 - 4.0,
        "still {} rows tall: {:.1} px",
        fx_text::WORDS_ROWS,
        words.height()
    );
    disarm(&window);
    RAN_WIDTH.store(true, Ordering::SeqCst);
    window.close();
    settle();
}

/// §S3: the words box sits ABOVE the other fields — both by y and in the holder's own child order.
fn check_the_words_box_sits_above_the_other_fields(app: &adw::Application) {
    let window = land_form(app, 30.0);
    let words = bounds(&window, "text-field-words");
    let length = bounds(&window, "text-field-length");
    println!(
        "words top y={:.1}, length top y={:.1}",
        words.y(),
        length.y()
    );
    assert!(
        words.y() < length.y(),
        "the words come above Length (§S3): y {:.1} vs {:.1}",
        words.y(),
        length.y()
    );
    // And the holder really packs them in that order, so a later re-append cannot quietly swap them.
    let holder = form_holder(&window);
    let mut seen: Vec<String> = Vec::new();
    for row in holder.observe_children().iter::<glib::Object>().flatten() {
        let Ok(row) = row.downcast::<gtk::Widget>() else {
            continue;
        };
        for field in ["text-field-words", "text-field-length"] {
            if find_in(&row, field).is_some() {
                seen.push(field.to_string());
            }
        }
    }
    println!("holder order: {seen:?}");
    assert_eq!(
        seen.first().map(String::as_str),
        Some("text-field-words"),
        "Words is the holder's first field row: {seen:?}"
    );
    disarm(&window);
    RAN_ORDER.store(true, Ordering::SeqCst);
    window.close();
    settle();
}

#[test]
fn f3_4_s3_the_words_box_fills_the_column_above_the_other_fields() {
    let root = std::env::temp_dir().join(format!("np-f34-col-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("temp root created");
    std::env::set_current_dir(&root).expect("cwd pinned to the temp root");

    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(|app| {
            check_the_words_box_spans_the_form_column(app);
            check_the_words_box_sits_above_the_other_fields(app);
        });
        app.run_with_args::<&str>(&[]);
    });

    assert!(
        RAN_WIDTH.load(Ordering::SeqCst),
        "the full-width check never ran"
    );
    assert!(
        RAN_ORDER.load(Ordering::SeqCst),
        "the above-the-fields check never ran"
    );
}
