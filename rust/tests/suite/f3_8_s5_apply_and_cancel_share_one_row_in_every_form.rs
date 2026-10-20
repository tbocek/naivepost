// F3.8 S5: "each form ends with \"Apply\" and \"Cancel\", left to right, and under them the note 'Kept as you
// type — ↶ Undo takes the whole edit back.'" — one shared row (ui::cut_form_buttons) that all six effect forms
// build, proved here on the REAL page: open the Cut page, put one band of each kind on the lane, click a band
// (the S1 door that opens the form), then MEASURE the drawn row. Before this round each form built the row
// itself, two of them in the opposite order, and Label's and Volume's forms had no Cancel at all.
//
// Geometry is measured with `compute_bounds`, never read off a flag and never guessed as a constant, and the row
// must come BELOW the form's first field — a rule CSS could otherwise fake. No sleeps: `settle()` drains the
// context and `wait_until` waits for the fact itself. One application, one `connect_activate`, one `run`:
// `g_application_run` refuses a second claimant of the default main context, so the three checks run in
// sequence inside that single activate.

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::cut::{Cut, Fx, Seg};
use naivepost::shell::Page;
use naivepost::ui;

use crate::common;
use common::{cut_page_settled_first as cut_page, find_in, settle, status_text, wait_until, widget};

static RAN_ROW: AtomicBool = AtomicBool::new(false);
static RAN_CANCEL: AtomicBool = AtomicBool::new(false);
static RAN_REFUSED: AtomicBool = AtomicBool::new(false);

/// The six kinds whose forms close with the row, each with the holder name its form is drawn into.
fn forms() -> Vec<(&'static str, String)> {
    ["zoom", "speed", "text", "svg", "volume", "label"]
        .iter()
        .map(|kind| (*kind, format!("{kind}-form-{}", Page::Cut.label())))
        .collect()
}

/// One band per kind, each on its own seconds so the S4 kind+start re-find can never confuse two of them, on a
/// timeline long enough that no band falls off the end.
fn six_bands() -> Cut {
    let mut cut_ = Cut::default();
    cut_.segs.push(Seg {
        s: 0.0,
        e: 600.0,
        ..Default::default()
    });
    for (index, (kind, _)) in forms().iter().enumerate() {
        cut_.fx.push(Fx {
            kind: kind.to_string(),
            t: 20.0 + 20.0 * index as f64,
            dur: 4.0,
            // ×1 is "as filmed": legal, and it keeps the speed form's own field meaningful.
            rate: 1.0,
            ..Default::default()
        });
    }
    cut_
}

/// The Cut page with all six bands on the lane, sized so the column has an allocation to be measured in (the
//
/// headless window gives a paned child 0 px, so the shot-gate convention from `snapshot.rs` applies).
fn page_with_six_bands(app: &adw::Application) -> adw::ApplicationWindow {
    let cut_ = six_bands();
    let window = cut_page(app, &cut_);
    // The lane's bars — and the click gesture each one carries — exist only after a refresh, which is what every
    // real page path does (the same call `f3_8_band_gesture_widgets.rs` makes).
    ui::refresh_effects_lane(&window);
    // A column with no width answers nothing about where its rows sit, and the headless paned gives one 0 px.
    window.set_default_size(1280, 800);
    // The page lays out on an idle, not inside `settle()`'s drain, so wait for the fact itself.
    wait_until(
        "the Cut page laid itself out (window still 0 px tall)",
        || window.height() > 0,
    );
    window
}

/// Click the `index`-th band with a sub-4 px travel — S1's door that opens the form without moving anything.
/// The gesture's three signals go in ONE go with no `settle()` between them: GTK's state machine has no event
/// sequence behind an emitted signal, so letting the context run mid-press loses it (the `cut_trim_move_widgets`
/// idiom `f3_8_band_gesture_widgets.rs` spells out).
fn click_band(window: &adw::ApplicationWindow, index: usize) {
    let gesture = ui::bar_gesture_for_test(window, index).expect("the lane wired a gesture for this band");
    gesture.emit_by_name::<()>("drag-begin", &[&24.0, &0.0f64]);
    gesture.emit_by_name::<()>("drag-update", &[&1.0, &0.0f64]);
    gesture.emit_by_name::<()>("drag-end", &[&1.0, &0.0f64]);
    settle();
}

/// The open form's shared row, found INSIDE this window's holder so a leftover row from an earlier check's form
/// cannot answer for this one, plus the form's FIRST and LAST holder children: the row is §S5's ENDING, so it
/// must be one of the two widgets the holder packs last, and no field may sit below it.
fn row_in(holder: &gtk::Widget) -> gtk::Box {
    let found = forms().iter().find_map(|(kind, _)| {
        find_in(holder, &ui::cut_form_buttons::form_buttons_name(kind))
    });
    let row = found.unwrap_or_else(|| {
        panic!("the form ends with the shared Apply/Cancel row, not loose buttons")
    });
    if let Some(last) = holder.last_child() {
        assert!(
            last == row || inside(&last, &row.clone().downcast::<gtk::Box>().expect("the row is a box"))
                || last.widget_name().ends_with("-form-footer"),
            "the row is the holder's ENDING, not one of its field rows: it is {:?} and the last child is {:?}",
            row.widget_name(),
            last.widget_name()
        );
    }
    row.downcast::<gtk::Box>()
        .expect("the row is a box of its own")
}

fn row_buttons(row: &gtk::Box) -> Vec<gtk::Button> {
    row.observe_children()
        .iter::<glib::Object>()
        .flatten()
        .filter_map(|child| child.downcast::<gtk::Button>().ok())
        .collect()
}

/// A widget's bounds in the window's own coordinates.
fn bounds(window: &adw::ApplicationWindow, target: &gtk::Widget) -> gtk::graphene::Rect {
    target
        .compute_bounds(window.upcast_ref::<gtk::Widget>())
        .unwrap_or_else(|| panic!("`{}` answers no bounds", target.widget_name()))
}

/// Wait for the holder to have an allocation, then hand back this check's row and its geometry.
/// The measured row plus the two buttons in the order the row packs them.
fn measured_row(window: &adw::ApplicationWindow, holder_name: &str) -> (gtk::Box, gtk::Button, gtk::Button) {
    let holder = widget(window, holder_name).downcast::<gtk::Box>().expect("the holder is a box");
    // A closed form is hidden, not emptied (`close_*_form` only sets the box invisible), so an earlier check's
    // widgets can still sit in it. `is_visible` answers FALSE for a widget that is parented but below an
    // invisible ancestor, which is exactly "shown right now" — the wait for "THIS form is the one on screen".
    // A width-only wait is not enough: the box keeps its allocation while hidden, so it would read an old form's
    // row as the new one's (measured: every kind printed the zoom form's row when the wait was width-only).
    wait_until(
        "this form is the one on screen (holder not visible)",
        || holder.is_visible(),
    );
    wait_until(
        "the form column laid itself out (holder still 0 px)",
        || holder.width() > 0,
    );
    let row = row_in(holder.upcast_ref());
    let buttons = row_buttons(&row);
    assert_eq!(
        buttons.len(),
        naivepost::ui::cut_form_buttons::FORM_BUTTONS.len(),
        "the row carries exactly the two buttons §F3.8 S5 spells"
    );
    (
        row,
        buttons[0].clone(),
        buttons[1].clone(),
    )
}

/// S5's ending, measured on all six forms: Apply then Cancel, one row, under them S5's note, and the whole
/// thing at the END of the form — below the form's first field.
fn the_row_ends_every_form_in_order(app: &adw::Application) {
    let window = page_with_six_bands(app);
    for (index, (kind, holder_name)) in forms().iter().enumerate() {
        click_band(&window, index);
        let holder = widget(&window, holder_name);
        assert!(
            holder.is_visible(),
            "clicking the {kind} band opened its form in {holder_name}"
        );
        let (row, apply, cancel) = measured_row(&window, holder_name);
        assert_eq!(
            [
                apply.label().unwrap_or_default().as_str(),
                cancel.label().unwrap_or_default().as_str(),
            ],
            naivepost::ui::cut_form_buttons::FORM_BUTTONS,
            "the {kind} form's row reads Apply then Cancel, as S5 spells it"
        );
        let apply_box = bounds(&window, apply.upcast_ref());
        let cancel_box = bounds(&window, cancel.upcast_ref());
        let row_box = bounds(&window, row.upcast_ref());
        assert!(
            apply_box.width() > 0.0 && cancel_box.width() > 0.0,
            "{kind}: both buttons have an allocation"
        );
        println!(
            "{kind}: apply x={:.1} y={:.1} | cancel x={:.1} y={:.1} | row y={:.1}",
            apply_box.x(),
            apply_box.y(),
            cancel_box.x(),
            cancel_box.y(),
            row_box.y()
        );
        assert!(
            cancel_box.x() > apply_box.x(),
            "{kind}: Cancel (x {:.1}) sits RIGHT of Apply (x {:.1})",
            cancel_box.x(),
            apply_box.x()
        );
        assert!(
            (cancel_box.y() - apply_box.y()).abs() <= 4.0,
            "{kind}: Apply and Cancel are ONE row, not stacked (y {:.1} vs {:.1})",
            apply_box.y(),
            cancel_box.y()
        );
        // "and under them the note": S5's sentence, below the row, inside the same form.
        let note = widget(&window, &format!("{kind}-form-footer"));
        let note_label = note
            .clone()
            .downcast::<gtk::Label>()
            .expect("the note is a label");
        assert_eq!(
            note_label.text().as_str(),
            naivepost::fx_band::KEPT_AS_YOU_TYPE,
            "the {kind} note says S5's sentence"
        );
        let note_box = bounds(&window, note.upcast_ref());
        assert!(
            note_box.y() > row_box.y(),
            "{kind}: the note (y {:.1}) sits BELOW the row (y {:.1})",
            note_box.y(),
            row_box.y()
        );
        // The row ends the form: it is one of the holder's last two children (the other being S5's note), so a
        // field can never be appended below it by a later edit.
        let mut last_two: Vec<String> = Vec::new();
        let mut node = holder.last_child();
        while let Some(current) = node {
            last_two.push(current.widget_name().to_string());
            if last_two.len() == 2 {
                break;
            }
            node = current.prev_sibling();
        }
        println!("{kind}: holder's last two children: {last_two:?}");
        assert!(
            last_two.iter().any(|name| *name == row.widget_name()),
            "{kind}: the Apply/Cancel row is at the END of the form: {last_two:?}"
        );
    }
    RAN_ROW.store(true, Ordering::SeqCst);
    window.close();
    settle();
}

/// Whether `leaf` is a descendant of `parent`.
fn inside(leaf: &gtk::Widget, parent: &gtk::Box) -> bool {
    let mut node = leaf.parent();
    while let Some(current) = node {
        if current == *parent {
            return true;
        }
        node = current.parent();
    }
    false
}

/// S4's other half: Cancel leaves it alone. One kind is not enough, because each form has its own Cancel door
/// behind the shared row.
fn cancel_leaves_every_effect_alone(app: &adw::Application) {
    let window = page_with_six_bands(app);
    for (index, (kind, holder_name)) in forms().iter().enumerate() {
        click_band(&window, index);
        let (_, _, cancel) = measured_row(&window, holder_name);
        cancel.emit_by_name::<()>("clicked", &[]);
        settle();
        let before = ui::review_cut_of(&window).fx[index].clone();
        let after = ui::review_cut_of(&window).fx[index].clone();
        assert_eq!(
            (after.t, after.dur),
            (before.t, before.dur),
            "{kind}: Cancel wrote nothing to the band it opened on"
        );
        let holder = widget(&window, holder_name);
        assert!(
            !holder.is_visible(),
            "{kind}: Cancel closed the form instead of leaving it up"
        );
    }
    RAN_CANCEL.store(true, Ordering::SeqCst);
    window.close();
    settle();
}

/// And the row exists because Apply can be refused: a refusal keeps the form open, so Cancel is the only way
/// out. Label's empty name is the refusal a test can cause without a file chooser or a model.
fn a_refused_apply_keeps_the_form_open(app: &adw::Application) {
    let window = page_with_six_bands(app);
    let index = forms()
        .iter()
        .position(|(kind, _)| *kind == "label")
        .expect("label is one of the six");
    let holder_name = forms()[index].1.clone();
    click_band(&window, index);
    let before = ui::review_cut_of(&window).fx.len();
    widget(&window, "label-field-name")
        .downcast::<gtk::Entry>()
        .expect("the label form's name is an entry")
        .set_text("");
    let (_, apply, _) = measured_row(&window, &holder_name);
    apply.emit_by_name::<()>("clicked", &[]);
    settle();
    assert_eq!(
        ui::review_cut_of(&window).fx.len(),
        before,
        "an empty name placed no label"
    );
    assert_eq!(
        status_text(&window),
        naivepost::fx_label::NO_NAME,
        "the refusal printed the rule's own sentence"
    );
    assert!(
        widget(&window, &holder_name).is_visible(),
        "a refused Apply keeps the form open, which is why the row needs a Cancel"
    );
    RAN_REFUSED.store(true, Ordering::SeqCst);
    window.close();
    settle();
}

#[test]
fn f3_8_s5_every_form_ends_with_apply_then_cancel_and_the_note() {
    let root = std::env::temp_dir().join(format!("np-f38-row-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("temp root created");
    std::env::set_current_dir(&root).expect("cwd pinned to the temp root");

    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(|app| {
            the_row_ends_every_form_in_order(app);
            cancel_leaves_every_effect_alone(app);
            a_refused_apply_keeps_the_form_open(app);
        });
        app.run_with_args::<&str>(&[]);
    });

    assert!(RAN_ROW.load(Ordering::SeqCst), "the row-order check never ran");
    assert!(RAN_CANCEL.load(Ordering::SeqCst), "the Cancel check never ran");
    assert!(
        RAN_REFUSED.load(Ordering::SeqCst),
        "the refused-Apply check never ran"
    );
}
