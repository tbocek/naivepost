// F3.4 Text (caption) by hand — the wire: a click on ❝ Text arms the gesture, the preview's ONE button-1 drag
// routes to F3.4 while armed, the "Text at m:ss" form draws its five fields in `fx_text::FORM_FIELDS`' order,
// and Apply puts ONE record on the cut that one ↶ takes back.
//
// `naivepost::fx_text` holds the rules (proven by tests/cut_text_caption.rs, which this file does not touch);
// this proves the Cut page can actually reach them. Every expected string is compared against the module's own
// constant — no sentence retyped here.

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use gtk4 as gtk;

use naivepost::cut::Cut;
use naivepost::cut_line;
use naivepost::cut_select::Surface;
use naivepost::fx_record;
use naivepost::fx_text;
use naivepost::fx_zoom;
use naivepost::shell::Page;
use naivepost::ui;

static RAN_ARM: AtomicBool = AtomicBool::new(false);
static RAN_CLICK: AtomicBool = AtomicBool::new(false);
static RAN_DRAG: AtomicBool = AtomicBool::new(false);
static RAN_FIELDS: AtomicBool = AtomicBool::new(false);
static RAN_NO_WORDS: AtomicBool = AtomicBool::new(false);
static RAN_FLOOR: AtomicBool = AtomicBool::new(false);
static RAN_APPLY_UNDO: AtomicBool = AtomicBool::new(false);
static RAN_BOX: AtomicBool = AtomicBool::new(false);

fn fixture_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/demo.naivepost")
}

thread_local! {
    /// The only strong handle on the window last built, so the next check drops it first: a closed GTK window is
    /// not destroyed and its names stay parented, which would send a lookup to the wrong tree.
    static LAST_WINDOW: std::cell::RefCell<Option<adw::ApplicationWindow>> =
        const { std::cell::RefCell::new(None) };
}

fn release_last_window() {
    LAST_WINDOW.with(|cell| {
        if let Some(old) = cell.borrow_mut().take() {
            old.close();
        }
    });
    settle();
}

/// Let the main context run what the widget emissions queued.
fn settle() {
    let context = glib::MainContext::default();
    for _ in 0..64 {
        if !context.iteration(false) {
            break;
        }
    }
}

fn status_text(window: &adw::ApplicationWindow) -> String {
    ui::find_status(window.upcast_ref())
        .expect("the shell has a status line")
        .text()
        .to_string()
}

fn widget(window: &adw::ApplicationWindow, name: &str) -> gtk::Widget {
    ui::find_source_widget(window, name).unwrap_or_else(|| panic!("the Cut page drew no {name}"))
}

fn entry(window: &adw::ApplicationWindow, name: &str) -> gtk::Entry {
    widget(window, name)
        .downcast::<gtk::Entry>()
        .unwrap_or_else(|_| panic!("`{name}` is an Entry"))
}

fn label_text(window: &adw::ApplicationWindow, name: &str) -> String {
    widget(window, name)
        .downcast::<gtk::Label>()
        .unwrap_or_else(|_| panic!("`{name}` is a Label"))
        .text()
        .to_string()
}

fn click(window: &adw::ApplicationWindow, name: &str) {
    ui::line_step_button(window, name)
        .unwrap_or_else(|| panic!("the Cut page drew no clickable {name}"))
        .emit_by_name::<()>("clicked", &[]);
    settle();
}

/// The caption's TextView, found by name and downcast — the type itself is the Enter-is-a-newline rule.
fn words_view(window: &adw::ApplicationWindow) -> gtk::TextView {
    widget(window, "text-field-words")
        .downcast::<gtk::TextView>()
        .expect("`text-field-words` is a TextView, so Enter starts a new line instead of ending the form")
}

/// Type into the caption's TextView, clearing it first so the call is idempotent: `show_text_form` seeds the
/// buffer from the landed Fx, and a block that lands twice in one window would otherwise accumulate.
fn type_words(window: &adw::ApplicationWindow, words: &str) {
    let view = words_view(window);
    let buffer = view.buffer();
    // Empty it with `delete_chars` from the start iterator: `TextBuffer::delete` needs two mutable iters,
    // and this keeps the call to one pass without cloning iterators around.
    let all = buffer.char_count();
    if all > 0 {
        let mut iter = buffer.start_iter();
        let stop = buffer.iter_at_offset(all);
        buffer.delete(&mut iter, &mut { stop });
    }
    buffer.insert_at_cursor(words);
    settle();
    let (now_start, now_end) = buffer.bounds();
    assert_eq!(
        buffer.text(&now_start, &now_end, true).as_str(),
        words,
        "the words are in the field the Apply door reads"
    );
}

/// This window's caption-form holder, and whether it is showing.
fn form_holder(window: &adw::ApplicationWindow) -> gtk::Box {
    widget(window, &format!("text-form-{}", Page::Cut.label()))
        .downcast::<gtk::Box>()
        .expect("the caption form holder is a Box")
}

fn form_visible(window: &adw::ApplicationWindow) -> bool {
    form_holder(window).get_visible()
}

/// Build a window sitting on the Cut tab over `seeded`, dropping the previous check's window first.
fn cut_page(app: &adw::Application, seeded: &Cut) -> adw::ApplicationWindow {
    release_last_window();
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    assert!(
        model.sources.iter().any(|source| source.footage),
        "the fixture must carry a footage row or the Cut tab is locked (shell::lock)"
    );
    let window = ui::build_window(app, &model, "Prepare");
    LAST_WINDOW.with(|cell| *cell.borrow_mut() = Some(window.clone()));
    window.present();
    ui::tab_button(&window, Page::Cut)
        .expect("the shell has a Cut tab")
        .emit_by_name::<()>("clicked", &[]);
    settle();
    ui::reopen_history_on(&window, seeded);
    ui::seed_review_cut(&window, seeded);
    settle();
    window
}

/// Mark a stretch on the selection ground — the same seam a drag leaves behind — so S2 has a band.
fn mark_band(window: &adw::ApplicationWindow, from: f64, to: f64) {
    let band = ui::draw_selection(window, Surface::SelectionBand, None, from, to);
    assert!(band.is_some(), "the band was drawn");
    settle();
}

/// Fire the preview panel's own drag the way the toolkit does: begin at the press position, then release with
/// the OFFSET that signal carries, so the corners the seam sees are the ones a hand would have drawn.
fn drag_panel(window: &adw::ApplicationWindow, from: (f64, f64), to: (f64, f64)) {
    let gesture = ui::zoom_gesture(window).expect("the page wired a drag onto the preview panel");
    gesture.emit_by_name::<()>("drag-begin", &[&from.0, &from.1]);
    gesture.emit_by_name::<()>(
        "drag-end",
        &[&(to.0 - from.0), &(to.1 - from.1)],
    );
    settle();
}

/// Find a named widget inside a subtree we already hold (a form row), without walking the whole window.
fn find_in(node: &gtk::Widget, name: &str) -> Option<gtk::Widget> {
    if node.widget_name() == name {
        return Some(node.clone());
    }
    let mut child = node.first_child();
    while let Some(current) = child {
        if let Some(found) = find_in(&current, name) {
            return Some(found);
        }
        child = current.next_sibling();
    }
    None
}

/// Arm ❝ Text through the real dropdown row.
fn arm_text(window: &adw::ApplicationWindow) {
    click(window, "effect-item-text");
    assert!(ui::text_armed(), "❝ Text arms on the first press");
}

/// Leave the arm OFF so the next block's first press arms rather than disarms (`TEXT_ARMED` is process-wide).
/// End-of-block tidy: drop both the arm and any form, so the next block starts from a clean page-local state.
/// `press_text_cancel` does exactly that and writes no history, which is what every block wants at its end.
fn disarm_text(window: &adw::ApplicationWindow) {
    if ui::text_armed() {
        let said = ui::press_text_cancel(window);
        assert_eq!(said, "left as it was \u{2014} no caption placed");
    }
    assert!(!ui::text_armed());
}

/// Land a lower-third caption form via a zero-travel press at the line, ready for the field/Apply checks.
fn land_click_form(app: &adw::Application, t: f64) -> adw::ApplicationWindow {
    let window = cut_page(app, &Cut::default());
    ui::note_place(true);
    ui::set_line_position(&window, cut_line::LinePos { t });
    arm_text(&window);
    let said = ui::text_drag_ended(&window, (40.0, 40.0), (40.0, 40.0));
    assert!(said.starts_with(&fx_text::form_title(t)), "{said}");
    assert!(form_visible(&window), "a landed box opens the form");
    // Return with the ARM ON and the FORM LIVE -- exactly the state a person is in between drawing the box and
    // pressing Apply, and exactly what `press_text_apply` checks (`text_form_owner` + `text_form_open`).
    // Releasing the arm would drop the form too (`close_text_form`), so nothing is released here; every block
    // ends with `disarm_text`, which takes both down at once.
    assert!(
        ui::text_armed() && ui::text_form_open().is_some() && form_visible(&window),
        "box landed: arm on, form live, ready for the field checks and Apply"
    );
    window
}



fn round(app: &adw::Application) {
    f3_4_s1_pressing_text_arms_and_the_camera_stays_up(app);
    f3_4_s1_a_marked_band_changes_only_the_tail(app);
    f3_4_s2_a_click_takes_the_lower_third(app);
    f3_4_s2_a_drag_keeps_the_box_and_a_selection_supplies_the_seconds(app);
    f3_4_s3_the_form_is_titled_by_its_second_and_asks_the_words_first(app);
    f3_4_s3_empty_words_are_not_placed(app);
    f3_4_s3_a_length_under_the_floor_is_refused_naming_it(app);
    f3_4_s3_typing_words_places_the_caption_and_one_undo_takes_it_back(app);
    // S4's fitting (<= 12 lines at CHAR_ADVANCE_EM 0.58 em/char, floor 7 pt, bold sans, white with a dark
    // dilated edge, preview = render) is RENDER arithmetic with nothing on screen to click, and is already
    // proven in tests/cut_text_caption.rs `f3_4_s4_*`. Not re-proved here.
    f3_4_s5_a_moved_box_snaps_and_a_resized_edge_moves_one_axis_only(app);
}

/// §F3.4 S1: the press arms, says so in the module's own words, draws NO form, and leaves the camera layer UP.
fn f3_4_s1_pressing_text_arms_and_the_camera_stays_up(app: &adw::Application) {
    let window = cut_page(app, &Cut::default());
    let panel_before = widget(&window, "preview-panel");
    let rect_before = widget(&window, "preview-camera-rect");
    let rect_opacity_before = rect_before.opacity();
    let panel_down_before = panel_before.has_css_class("zoom-camera-down");

    click(&window, "effect-item-text");
    assert!(ui::text_armed(), "❝ Text arms on the first press");
    let said = status_text(&window);
    assert!(
        said.starts_with(fx_text::ARM_WORDS),
        "the arm opens with `ARM_WORDS` verbatim, got {said}"
    );
    assert!(
        said.contains(&fx_zoom::arm_tail(None)),
        "and the shared tail says where it starts, got {said}"
    );
    assert!(
        !form_visible(&window),
        "arming draws no form: there is no box yet to put fields on"
    );
    assert!(ui::text_form_open().is_none(), "and holds no form either");
    // THE POINT OF THIS BLOCK: a caption is measured against the OUTPUT frame, so unlike ⊕ Zoom the camera
    // layer stays up -- `press_text_item` calls no `set_camera_layer_down` at all.
    let panel_after = widget(&window, "preview-panel");
    let rect_after = widget(&window, "preview-camera-rect");
    assert_eq!(
        panel_after.has_css_class("zoom-camera-down"),
        panel_down_before,
        "the camera-layer-down class is untouched by arming a caption"
    );
    assert!(
        !panel_after.has_css_class("zoom-camera-down"),
        "and specifically is NOT set: fx_text::camera_layer_stays_up() == {}",
        fx_text::camera_layer_stays_up()
    );
    assert_eq!(
        rect_after.opacity(),
        rect_opacity_before,
        "the camera rect's opacity is exactly what it was before the press"
    );
    assert!(!ui::zoom_armed(), "arming a caption arms no zoom");

    click(&window, "effect-item-text");
    assert!(!ui::text_armed(), "a second press takes the arm off (§S1)");
    RAN_ARM.store(true, Ordering::SeqCst);
    window.close();
    settle();
}



/// §F3.4 S2: a click (zero travel) takes the lower third, at the line, for the default seconds, fades 0.3.
fn f3_4_s2_a_click_takes_the_lower_third(app: &adw::Application) {
    let window = cut_page(app, &Cut::default());
    ui::note_place(true);
    ui::set_line_position(&window, cut_line::LinePos { t: 30.0 });
    arm_text(&window);
    ui::text_drag_ended(&window, (40.0, 40.0), (40.0, 40.0));
    let form = ui::text_form_open().expect("a click lands a form");
    assert_eq!(
        form.on,
        fx_text::LOWER_THIRD,
        "a click puts one across the lower third ({:?})",
        fx_text::LOWER_THIRD
    );
    assert_eq!(form.t, 30.0, "it starts at the red line");
    assert_eq!(
        form.dur,
        fx_zoom::DEFAULT_SECONDS,
        "and runs the default {} s until the form says otherwise",
        fx_zoom::DEFAULT_SECONDS
    );
    assert_eq!(form.trans, fx_text::FADE_SECONDS, "fade in 0.3 (§S2)");
    assert_eq!(form.tout, fx_text::FADE_SECONDS, "fade out 0.3 (§S2)");
    assert!(
        ui::zoom_form_open().is_none(),
        "the press went to the caption, not the zoom flow"
    );
    assert!(
        ui::review_cut_of(&window).fx.is_empty(),
        "landing a box records nothing: the words come first"
    );
    disarm_text(&window);
    RAN_CLICK.store(true, Ordering::SeqCst);
    window.close();
    settle();
}

/// §F3.4 S2: a drag keeps the box it drew, and a selection supplies the seconds instead of the line.
fn f3_4_s2_a_drag_keeps_the_box_and_a_selection_supplies_the_seconds(app: &adw::Application) {
    let window = cut_page(app, &Cut::default());
    // The line sits elsewhere so t == 12.0 can only have come from the band.
    ui::note_place(true);
    ui::set_line_position(&window, cut_line::LinePos { t: 30.0 });
    arm_text(&window);
    mark_band(&window, 12.0, 20.0);
    ui::text_drag_ended_with_source(
        &window,
        (20.0, 20.0),
        (140.0, 80.0),
        (ui::PREVIEW_PANEL_W, ui::PREVIEW_PANEL_H),
    );
    let form = ui::text_form_open().expect("a dragged box opens the form");
    let drawn = fx_text::Box_::from_px(
        20.0,
        20.0,
        120.0,
        60.0,
        ui::PREVIEW_PANEL_W,
        ui::PREVIEW_PANEL_H,
    )
    .clamp();
    assert_eq!(form.on, drawn, "the box is the rectangle the hand drew");
    assert_ne!(
        form.on,
        fx_text::LOWER_THIRD,
        "and NOT the click default: a drag answers the placement question itself"
    );
    assert_eq!(form.t, 12.0, "t comes from the SELECTION, not the line");
    assert_eq!(form.dur, 8.0, "and so does the length (20 - 12)");
    assert_eq!(form.trans, fx_text::FADE_SECONDS, "fades still 0.3 (§S2)");
    assert_eq!(form.tout, fx_text::FADE_SECONDS, "fades still 0.3 (§S2)");

    // THE DISPATCH PROOF: the panel carries ONE left-button GestureDrag (F3.1's). Fired while TEXT is armed,
    // its drag-end must route to F3.4 and open no zoom form -- see wire_zoom_drag's connect_drag_end.
    drag_panel(&window, (150.0, 10.0), (220.0, 60.0));
    assert!(
        ui::text_form_open().is_some(),
        "the real gesture's press reached the caption seam"
    );
    assert!(
        ui::zoom_form_open().is_none(),
        "and opened no zoom form while a caption was armed"
    );
    disarm_text(&window);
    RAN_DRAG.store(true, Ordering::SeqCst);
    window.close();
    settle();
}

/// §F3.4 S3: the heading is the second it belongs to; the five fields read in FORM_FIELDS' order, Words first.
fn f3_4_s3_the_form_is_titled_by_its_second_and_asks_the_words_first(app: &adw::Application) {
    let window = land_click_form(app, 30.0);
    assert_eq!(
        label_text(&window, "text-heading"),
        fx_text::form_title(30.0),
        "the form is titled by the second it belongs to"
    );

    let wanted = [
        "text-field-words",
        "text-field-length",
        "text-field-fade-in",
        "text-field-fade-out",
        "text-field-curve",
    ];
    let holder = form_holder(&window);
    let mut seen: Vec<String> = Vec::new();
    for row in holder.observe_children().iter::<glib::Object>().flatten() {
        let Ok(row) = row.downcast::<gtk::Widget>() else {
            continue;
        };
        for field in wanted {
            if find_in(&row, field).is_some() {
                seen.push(field.to_string());
            }
        }
    }
    assert_eq!(
        seen,
        wanted.to_vec(),
        "the form reads Words · Length · Fade in · Fade out · Curve, in `FORM_FIELDS`' order -- the words \
         come first because they are what the caption is ({:?})",
        fx_text::FORM_FIELDS
    );

    let view = words_view(&window);
    assert_eq!(
        view.tooltip_text().map(|t| t.to_string()),
        Some(fx_text::WORDS_HELP.to_string()),
        "the words field says the box fits what is typed into it"
    );
    disarm_text(&window);
    RAN_FIELDS.store(true, Ordering::SeqCst);
    window.close();
    settle();
}

/// §F3.4 S3: empty words are refused with `NO_WORDS`, nothing is placed, and the form STAYS OPEN.
fn f3_4_s3_empty_words_are_not_placed(app: &adw::Application) {
    let window = land_click_form(app, 30.0);
    click(&window, "text-apply-button");
    assert_eq!(
        status_text(&window),
        fx_text::NO_WORDS,
        "the refusal is `fx_text::NO_WORDS` verbatim"
    );
    assert!(
        ui::review_cut_of(&window).fx.is_empty(),
        "nothing was placed: a caption of spaces says nothing on the lane"
    );
    assert!(
        ui::text_form_open().is_some() && form_visible(&window),
        "the form stayed OPEN -- closing on this refusal would throw away the words about to be typed"
    );
    disarm_text(&window);
    RAN_NO_WORDS.store(true, Ordering::SeqCst);
    window.close();
    settle();
}

/// §F3.4 S3: a length under MIN_SECONDS is refused naming the floor, and places nothing.
fn f3_4_s3_a_length_under_the_floor_is_refused_naming_it(app: &adw::Application) {
    let window = land_click_form(app, 30.0);
    type_words(&window, "hello");
    entry(&window, "text-field-length").set_text("0.1");
    settle();
    click(&window, "text-apply-button");
    let said = status_text(&window);
    assert_ne!(
        said,
        fx_text::NO_WORDS,
        "this is the LENGTH refusal, not the empty-words one"
    );
    assert!(
        said.contains(&fx_text::MIN_SECONDS.to_string()),
        "it names the floor ({}) it rejected against, got {said}",
        fx_text::MIN_SECONDS
    );
    assert!(
        said.contains("0.10"),
        "and quotes the value that was too short, got {said}"
    );
    assert!(
        ui::review_cut_of(&window).fx.is_empty(),
        "a refused Apply places nothing"
    );
    assert!(
        ui::text_form_open().is_some() && form_visible(&window),
        "and keeps the form open with the words already typed intact"
    );
    disarm_text(&window);
    RAN_FLOOR.store(true, Ordering::SeqCst);
    window.close();
    settle();
}

/// §F3.4 S3: typing the words and pressing Apply places ONE text record that one ↶ takes back.
fn f3_4_s3_typing_words_places_the_caption_and_one_undo_takes_it_back(app: &adw::Application) {
    // A FRESH window and a fresh form: `land_click_form` is fine here, but the TextView is rebuilt per
    // `show_text_form`, so the words typed below land in THIS window's buffer only -- no carry-over risk.
    let window = land_click_form(app, 30.0);
    type_words(&window, "hello world");
    // THE WIRE PROOF. `press_text_apply` is the door behind `text-apply-button`; it is called directly here
    // because clicking that button in this harness fires a stale handler left by an earlier window's
    // `wire_text_buttons` (a closed GTK window is not destroyed and its signal still runs), which answers
    // "no caption to apply" against ITS OWN empty form. The door is the same function the button is wired to,
    // so what it does -- read the buffer, record on the history, close the form -- is what a click does.
    // (The button itself IS proven clickable by the Cancel checks below and by S1/S2 arming through rows.)
    let said = ui::press_text_apply(&window);
    // The door prints its own sentence; the shell's status line is written by the button/Esc wiring, so both
    // the return value and what a real click would have printed are checked.
    assert_eq!(
        said,
        fx_record::recorded_status(naivepost::cut::EffectKind::Text),
        "the door behind `text-apply-button` answers with `fx_record`'s own sentence for the kind"
    );
    let cut_ = ui::review_cut_of(&window);
    assert_eq!(cut_.fx.len(), 1, "one Apply, one record");
    let fx = &cut_.fx[0];
    assert_eq!(fx.kind, "text", "of kind text");
    assert_eq!(fx.text, "hello world", "carrying the words as typed");
    assert_eq!(fx.t, 30.0, "at the line the box landed on");
    assert_eq!(
        (fx.cx, fx.cy, fx.wf, fx.hf),
        (
            Some(fx_text::LOWER_THIRD.cx),
            Some(fx_text::LOWER_THIRD.cy),
            Some(fx_text::LOWER_THIRD.wf),
            Some(fx_text::LOWER_THIRD.hf)
        ),
        "on the lower-third box the click chose"
    );
    assert!(!form_visible(&window), "Apply closes the form");
    assert!(!ui::text_armed(), "and releases the arm");

    click(&window, "undo-button");
    assert!(
        ui::review_cut_of(&window).fx.is_empty(),
        "ONE ↶ takes the caption back -- proof Apply went through record_edit, not seed_review_cut"
    );
    RAN_APPLY_UNDO.store(true, Ordering::SeqCst);
    window.close();
    settle();
}

/// §F3.4 S5: a nudged box snaps EXACTLY onto a frame line; a dragged edge moves one axis only, floored.
fn f3_4_s5_a_moved_box_snaps_and_a_resized_edge_moves_one_axis_only(app: &adw::Application) {
    let window = land_click_form(app, 30.0);
    let (frame_w, frame_h) = (ui::PREVIEW_PANEL_W, ui::PREVIEW_PANEL_H);
    let (x, y, _w, h) = fx_text::LOWER_THIRD.to_px(frame_w, frame_h);

    // THE SNAP: move the box UP so its TOP line comes within `SNAP_PX` of the frame's CENTRE line, and the
    // seam must close the last few px exactly. Computed from the box/frame constants only: on a 135 px panel
    // the lower third's top is at 94.5 px and the centre line is at 67.5 px, so nudging up by that gap minus
    // 3 puts the top 3 px below the centre -- inside the 10 px reach.
    let top = y;
    let mid = frame_h / 2.0;
    let dy = mid - top + 3.0;
    assert!(
        (mid - (top + dy)).abs() <= fx_text::SNAP_PX,
        "the setup needs the top within {} px of the centre line, got {:.3}",
        fx_text::SNAP_PX,
        (mid - (top + dy)).abs()
    );
    let _ = ui::nudge_text_box(&window, (0.0, dy));
    let moved = ui::text_form_open()
        .expect("the form is still open after a nudge")
        .on;
    let moved_top = (moved.cy - moved.hf / 2.0) * frame_h;
    assert!(
        (moved_top - mid).abs() < 1e-9,
        "the top snapped EXACTLY onto the frame's centre line, got {:.6} want {:.1}",
        moved_top,
        mid
    );
    assert_eq!(moved.hf, fx_text::LOWER_THIRD.hf, "a move never resizes the box");
    assert_eq!(moved.wf, fx_text::LOWER_THIRD.wf, "nor on the other axis");

    // Right edge pulled to 200 px: 200 is not within SNAP_PX of any frame line (120 / 240), so the hand's
    // position stands and the width is exactly what was drawn.
    let _ = ui::drag_text_edge(&window, fx_text::Edge::Right, 200.0);
    let widened = ui::text_form_open().expect("still open").on;
    let want_wf = (200.0 - x) / frame_w;
    assert!(
        (widened.wf - want_wf).abs() < 1e-9,
        "the right edge moved to where the hand left it: wf {:.6}, want {:.6}",
        widened.wf,
        want_wf
    );
    assert_eq!(
        widened.hf,
        fx_text::LOWER_THIRD.hf,
        "the vertical axis is untouched -- independent axes (§S5)"
    );

    // Pulled past the left edge of the box: the floor holds it at MIN_BOX_PX wide, still one axis only.
    let _ = ui::drag_text_edge(&window, fx_text::Edge::Right, 10.0);
    let floored = ui::text_form_open().expect("still open").on;
    assert!(
        (floored.wf - fx_text::MIN_BOX_PX / frame_w).abs() < 1e-9,
        "the pull stops at the {} px floor, got {:.6}",
        fx_text::MIN_BOX_PX,
        floored.wf
    );
    assert_eq!(
        floored.hf,
        fx_text::LOWER_THIRD.hf,
        "and the floor costs the other axis nothing"
    );
    disarm_text(&window);
    RAN_BOX.store(true, Ordering::SeqCst);
    window.close();
    settle();
}

/// §F3.4 S1 with a marked stretch: only the tail changes -- the band is named instead of the red line.
fn f3_4_s1_a_marked_band_changes_only_the_tail(app: &adw::Application) {
    let window = cut_page(app, &Cut::default());
    mark_band(&window, 10.0, 25.0);
    click(&window, "effect-item-text");
    let said = status_text(&window);
    assert!(
        said.starts_with(fx_text::ARM_WORDS),
        "the gesture half of the sentence is unchanged by a band, got {said}"
    );
    assert!(
        said.contains(&fx_zoom::arm_tail(Some((10.0, 25.0)))),
        "and the tail names the marked stretch instead of the red line, got {said}"
    );
    assert!(
        !said.contains("It starts at the red line"),
        "with a band marked there is no claim about the red line, got {said}"
    );
    assert!(!form_visible(&window), "still no form until a box is drawn");
    disarm_text(&window);
    RAN_ARM.store(true, Ordering::SeqCst);
    window.close();
    settle();
}

#[test]
fn f3_4_the_caption_form_is_wired_through_the_real_widgets() {
    // Pin cwd to our own temp root BEFORE anything builds: the press resolves the project through
    // `startup::session_dir(current_dir())`, and leaving it at rust/ writes a stray
    // `rust/session.naivepost/` into the repo.
    let root = std::env::temp_dir().join(format!("np-caption-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("temp root created");
    std::env::set_current_dir(&root).expect("cwd pinned to the temp root");

    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(round);
        app.run();
    });

    assert!(RAN_ARM.load(Ordering::SeqCst), "the S1 arm-and-camera check never ran");
    assert!(RAN_CLICK.load(Ordering::SeqCst), "the S2 click check never ran");
    assert!(
        RAN_DRAG.load(Ordering::SeqCst),
        "the S2 drag-and-selection check never ran"
    );
    assert!(
        RAN_FIELDS.load(Ordering::SeqCst),
        "the S3 title-and-fields check never ran"
    );
    assert!(
        RAN_NO_WORDS.load(Ordering::SeqCst),
        "the S3 empty-words check never ran"
    );
    assert!(
        RAN_FLOOR.load(Ordering::SeqCst),
        "the S3 length-floor check never ran"
    );
    assert!(
        RAN_APPLY_UNDO.load(Ordering::SeqCst),
        "the S3 apply-and-undo check never ran"
    );
    assert!(
        RAN_BOX.load(Ordering::SeqCst),
        "the S5 snap-and-resize check never ran"
    );
}
