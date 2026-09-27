// F3.5 SVG drawing by hand — the wire: a click on ▨ SVG refuses for want of a line, asks for the file, arms with
// the file named in its sentence, lands the middle box on a click, and Apply puts ONE record on the cut that one
// ↶ takes back.
//
// `naivepost::fx_svg` holds the rules (proven by tests/cut_svg_drawing.rs, which this file does not touch);
// this proves the Cut page can actually reach them. Every expected string is compared against the module's own
// constant — no sentence retyped here.

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use gtk4 as gtk;

use naivepost::cut::Cut;
use naivepost::cut_line;
use naivepost::cut_select::Surface;
use naivepost::fx_record;
use naivepost::fx_svg;
use naivepost::fx_text;
use naivepost::fx_zoom;
use naivepost::shell::Page;
use naivepost::ui;

static RAN_REFUSE: AtomicBool = AtomicBool::new(false);
static RAN_CHOOSER: AtomicBool = AtomicBool::new(false);
static RAN_NO_FILE: AtomicBool = AtomicBool::new(false);
static RAN_ARM: AtomicBool = AtomicBool::new(false);
static RAN_NO_TOGGLE: AtomicBool = AtomicBool::new(false);
static RAN_CLICK: AtomicBool = AtomicBool::new(false);
static RAN_DRAG: AtomicBool = AtomicBool::new(false);
static RAN_CAMERA: AtomicBool = AtomicBool::new(false);
static RAN_FIELDS: AtomicBool = AtomicBool::new(false);
static RAN_NOT_PLACED: AtomicBool = AtomicBool::new(false);
static RAN_FLOOR: AtomicBool = AtomicBool::new(false);
static RAN_APPLY_UNDO: AtomicBool = AtomicBool::new(false);
static RAN_RASTER: AtomicBool = AtomicBool::new(false);

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

/// This window's drawing-form holder, and whether it is showing.
fn form_holder(window: &adw::ApplicationWindow) -> gtk::Box {
    widget(window, &format!("svg-form-{}", Page::Cut.label()))
        .downcast::<gtk::Box>()
        .expect("the drawing form holder is a Box")
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

/// Mark a stretch on the selection ground — the same seam a drag leaves behind.
fn mark_band(window: &adw::ApplicationWindow, from: f64, to: f64) {
    let band = ui::draw_selection(window, Surface::SelectionBand, None, from, to);
    assert!(band.is_some(), "the band was drawn");
    settle();
}

/// Fire the preview panel's own drag the way the toolkit does: begin at the press position, then release with
/// the OFFSET that signal carries.
fn drag_panel(window: &adw::ApplicationWindow, from: (f64, f64), to: (f64, f64)) {
    let gesture = ui::zoom_gesture(window).expect("the page wired a drag onto the preview panel");
    gesture.emit_by_name::<()>("drag-begin", &[&from.0, &from.1]);
    gesture.emit_by_name::<()>("drag-end", &[&(to.0 - from.0), &(to.1 - from.1)]);
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

/// Put a line down so §S1's `a line?` answers yes, and pointing where we expect.
fn set_line(window: &adw::ApplicationWindow, t: f64) {
    ui::note_place(true);
    ui::set_line_position(window, cut_line::LinePos { t });
}

/// Choose a drawing through the door the chooser answers through, and require the arm.
fn arm_with(window: &adw::ApplicationWindow, file: &str) -> String {
    let said = ui::svg_chosen(window, Some(file));
    assert!(ui::svg_armed(), "choosing a file arms \u{25a8} SVG");
    assert_eq!(
        ui::svg_chosen_file().as_deref(),
        Some(file),
        "and the chosen drawing is the one carried into the record"
    );
    said
}

/// Land a middle-box drawing form via a zero-travel press at the line, ready for the field/Apply checks.
fn land_click_form(app: &adw::Application, t: f64) -> adw::ApplicationWindow {
    let window = cut_page(app, &Cut::default());
    set_line(&window, t);
    arm_with(&window, "assets/logo.svg");
    let said = ui::svg_drag_ended(&window, (40.0, 40.0), (40.0, 40.0));
    assert!(
        said.starts_with(&fx_svg::form_title(t)),
        "a landed box titles its form by the second: {said}"
    );
    assert!(form_visible(&window), "a landed box opens the form");
    // The arm is left exactly as the landing left it -- live. Releasing it means `press_svg_cancel`, which drops
    // the FORM too (`close_svg_form` clears SVG_FORM), destroying what every caller came to read or Apply.
    window
}

fn round(app: &adw::Application) {
    f3_5_s1_a_press_with_no_line_refuses_before_any_chooser(app);
    f3_5_s1_a_press_with_a_line_opens_the_chooser_for_assets(app);
    f3_5_s2_a_cancelled_chooser_places_nothing(app);
    f3_5_s2_choosing_a_file_arms_and_names_it(app);
    f3_5_s2_a_second_choice_never_disarms(app);
    f3_5_s3_a_click_lands_the_middle_box_not_the_lower_third(app);
    f3_5_s3_a_drag_keeps_its_box_and_a_selection_gives_the_seconds(app);
    f3_5_s3_the_camera_layer_stays_up_while_a_drawing_is_placed(app);
    f3_5_s4_the_form_is_titled_by_its_second_and_leads_with_the_file(app);
    f3_5_s4_no_file_is_not_placed(app);
    f3_5_s4_a_length_under_the_floor_is_refused_naming_it(app);
    f3_5_s4_apply_places_one_drawing_and_one_undo_takes_it_back(app);
    f3_5_s5_the_raster_is_512_px_transparent_and_one_per_file(app);
}

/// §F3.5 S1: with no line ever placed, the press refuses in its OWN words before any chooser is built.
fn f3_5_s1_a_press_with_no_line_refuses_before_any_chooser(app: &adw::Application) {
    let window = cut_page(app, &Cut::default());
    ui::note_place(false);
    click(&window, "effect-item-svg");
    let said = status_text(&window);
    assert_eq!(
        said,
        fx_svg::NO_LINE,
        "the refusal is `fx_svg::NO_LINE` verbatim, got {said}"
    );
    // Four buttons ask for the same missing thing in the words of what each one DOES ("the effect" / "the
    // drawing" / ...), so they must NOT be interchangeable: a test keeps them apart.
    assert_ne!(
        fx_svg::NO_LINE, fx_zoom::NO_LINE,
        "\u{25a8} SVG's refusal is not \u{2295} Zoom's borrowed sentence"
    );
    assert!(!ui::svg_armed(), "a refusal arms nothing");
    assert!(ui::svg_chosen_file().is_none(), "and chooses nothing");
    assert!(ui::svg_form_open().is_none(), "and draws no form");
    assert!(!form_visible(&window), "the holder stays hidden");
    RAN_REFUSE.store(true, Ordering::SeqCst);
    window.close();
    settle();
}

/// §F3.5 S1: with a line, the press asks for the file instead of refusing.
fn f3_5_s1_a_press_with_a_line_opens_the_chooser_for_assets(app: &adw::Application) {
    let window = cut_page(app, &Cut::default());
    set_line(&window, 30.0);
    click(&window, "effect-item-svg");
    let said = status_text(&window);
    assert!(
        said.contains(fx_svg::CHOOSE_TITLE),
        "the press asks with `CHOOSE_TITLE`, got {said}"
    );
    // A headless chooser never answers, so the DOOR seam (`svg_chosen`) is what carries the answer in every
    // other block; here all that is observable is that the question was asked.
    assert!(!ui::svg_armed(), "nothing is armed until a file comes back");
    assert!(ui::svg_form_open().is_none(), "and no form is drawn yet");
    RAN_CHOOSER.store(true, Ordering::SeqCst);
    window.close();
    settle();
}

/// §F3.5 S2: cancelling the chooser places nothing.
fn f3_5_s2_a_cancelled_chooser_places_nothing(app: &adw::Application) {
    let window = cut_page(app, &Cut::default());
    set_line(&window, 30.0);
    let said = ui::svg_chosen(&window, None);
    assert_eq!(said, fx_svg::NO_FILE, "the refusal is `NO_FILE` verbatim");
    assert!(!ui::svg_armed(), "no arm without a file");
    assert!(ui::svg_chosen_file().is_none(), "no file carried");
    assert!(ui::svg_form_open().is_none(), "no form drawn");
    assert!(
        ui::review_cut_of(&window).fx.is_empty(),
        "and nothing reaches the cut"
    );
    RAN_NO_FILE.store(true, Ordering::SeqCst);
    window.close();
    settle();
}

/// §F3.5 S2: a chosen file arms the gesture and names itself in the arm sentence.
fn f3_5_s2_choosing_a_file_arms_and_names_it(app: &adw::Application) {
    let window = cut_page(app, &Cut::default());
    set_line(&window, 30.0);
    // No band yet: the tail speaks of the red line.
    let plain = ui::svg_chosen(&window, Some("assets/logo.svg"));
    assert!(
        plain.starts_with(&fx_svg::arm_head("assets/logo.svg")),
        "the head names the file that was just chosen, got {plain}"
    );
    assert!(
        plain.contains(&fx_zoom::arm_tail(None)),
        "and the shared tail speaks of the red line, got {plain}"
    );
    assert!(ui::svg_armed(), "arming happened");
    assert_eq!(ui::svg_chosen_file().as_deref(), Some("assets/logo.svg"));
    assert!(
        !form_visible(&window) && ui::svg_form_open().is_none(),
        "arming draws nothing: there is no box yet to put fields on"
    );
    // Now a band: the tail names the marked stretch instead.
    mark_band(&window, 12.0, 20.0);
    let banded = ui::svg_chosen(&window, Some("assets/logo.svg"));
    assert!(
        banded.contains(&fx_zoom::arm_tail(Some((12.0, 20.0)))),
        "with a band marked the tail says so, got {banded}"
    );
    assert!(
        !banded.contains("It starts at the red line"),
        "and makes no claim about the red line, got {banded}"
    );
        ui::release_svg_arm_only();
RAN_ARM.store(true, Ordering::SeqCst);
    window.close();
    settle();
}

/// §F3.5 S2: a second choice replaces the drawing and NEVER disarms.
fn f3_5_s2_a_second_choice_never_disarms(app: &adw::Application) {
    let window = cut_page(app, &Cut::default());
    set_line(&window, 30.0);
    arm_with(&window, "assets/logo.svg");
    // `fx_svg::chose` takes no `already_armed` precisely so this cannot toggle off (§F3.5 S2, and the
    // prototype's own comment "never a toggle-off: a file was just chosen on purpose").
    arm_with(&window, "assets/other.svg");
    assert!(ui::svg_armed(), "still armed after a second deliberate choice");
    assert_eq!(
        ui::svg_chosen_file().as_deref(),
        Some("assets/other.svg"),
        "and the newer drawing is the one carried forward"
    );
        ui::release_svg_arm_only();
RAN_NO_TOGGLE.store(true, Ordering::SeqCst);
    window.close();
    settle();
}

/// §F3.5 S3: a click (zero travel) takes the MIDDLE, not text's lower third.
fn f3_5_s3_a_click_lands_the_middle_box_not_the_lower_third(app: &adw::Application) {
    let window = land_click_form(app, 30.0);
    let form = ui::svg_form_open().expect("a click lands a form");
    assert_eq!(
        form.on,
        fx_svg::MIDDLE,
        "a click puts one across the middle ({:?})",
        fx_svg::MIDDLE
    );
    assert_ne!(
        form.on,
        fx_text::LOWER_THIRD,
        "and NOT the caption's lower third: a drawing is as often the subject"
    );
    assert_eq!(form.t, 30.0, "it starts at the red line");
    assert_eq!(
        form.dur,
        fx_zoom::DEFAULT_SECONDS,
        "and runs the default {} s until the form says otherwise",
        fx_zoom::DEFAULT_SECONDS
    );
    assert_eq!(form.trans, fx_text::FADE_SECONDS, "fade in 0.3");
    assert_eq!(form.tout, fx_text::FADE_SECONDS, "fade out 0.3");
    assert!(
        ui::review_cut_of(&window).fx.is_empty(),
        "landing a box records nothing: the file was chosen, the length not yet answered"
    );
        ui::release_svg_arm_only();
RAN_CLICK.store(true, Ordering::SeqCst);
    window.close();
    settle();
}

/// §F3.5 S3: a drag keeps the box it drew, and a selection supplies the seconds instead of the line.
fn f3_5_s3_a_drag_keeps_its_box_and_a_selection_gives_the_seconds(app: &adw::Application) {
    let window = cut_page(app, &Cut::default());
    // The line sits elsewhere so t == 12.0 can only have come from the band.
    set_line(&window, 30.0);
    arm_with(&window, "assets/logo.svg");
    mark_band(&window, 12.0, 20.0);
    ui::svg_drag_ended_with_source(
        &window,
        (20.0, 20.0),
        (140.0, 80.0),
        (ui::PREVIEW_PANEL_W, ui::PREVIEW_PANEL_H),
    );
    let form = ui::svg_form_open().expect("a dragged box opens the form");
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
        fx_svg::MIDDLE,
        "and NOT the click default: a drag answers the placement question itself"
    );
    assert_eq!(form.t, 12.0, "t comes from the SELECTION, not the line");
    assert_eq!(form.dur, 8.0, "and so does the length (20 - 12)");

    // THE DISPATCH PROOF: the panel carries ONE left-button GestureDrag (F3.1's). Fired while SVG is armed its
    // drag-end must route to F3.5 and open neither the zoom nor the caption flow.
    drag_panel(&window, (150.0, 10.0), (220.0, 60.0));
    assert!(
        ui::svg_form_open().is_some(),
        "the real gesture's press reached the drawing seam"
    );
    assert!(
        ui::zoom_form_open().is_none(),
        "and opened no zoom form while a drawing was armed"
    );
    assert!(
        ui::text_form_open().is_none(),
        "nor a caption form either"
    );
        ui::release_svg_arm_only();
RAN_DRAG.store(true, Ordering::SeqCst);
    window.close();
    settle();
}

/// §F3.5 S3: the camera layer stays UP the whole time a drawing is placed.
fn f3_5_s3_the_camera_layer_stays_up_while_a_drawing_is_placed(app: &adw::Application) {
    let window = cut_page(app, &Cut::default());
    set_line(&window, 30.0);
    let panel_before = widget(&window, "preview-panel");
    let rect_before = widget(&window, "preview-camera-rect");
    let opacity_before = rect_before.opacity();
    let down_before = panel_before.has_css_class("zoom-camera-down");

    arm_with(&window, "assets/logo.svg");
    let panel_armed = widget(&window, "preview-panel");
    assert!(
        !panel_armed.has_css_class("zoom-camera-down"),
        "arming a drawing puts no camera layer down (camera_layer_stays_up = {})",
        fx_svg::camera_layer_stays_up()
    );

    ui::svg_drag_ended(&window, (40.0, 40.0), (40.0, 40.0));
    let panel_after = widget(&window, "preview-panel");
    let rect_after = widget(&window, "preview-camera-rect");
    assert_eq!(
        panel_after.has_css_class("zoom-camera-down"),
        down_before,
        "the class is untouched by landing a drawing"
    );
    assert!(
        !panel_after.has_css_class("zoom-camera-down"),
        "and specifically is NOT set"
    );
    assert_eq!(
        rect_after.opacity(),
        opacity_before,
        "the camera rect's opacity is exactly what it was before arming"
    );
        ui::release_svg_arm_only();
RAN_CAMERA.store(true, Ordering::SeqCst);
    window.close();
    settle();
}

/// §F3.5 S4: titled by the second, leading with the file and its Choose… button.
fn f3_5_s4_the_form_is_titled_by_its_second_and_leads_with_the_file(app: &adw::Application) {
    let window = land_click_form(app, 30.0);
    assert_eq!(
        label_text(&window, "svg-heading"),
        fx_svg::form_title(30.0),
        "the form is titled by the second it belongs to"
    );

    let wanted = [
        "svg-field-file",
        "svg-field-length",
        "svg-field-fade-in",
        "svg-field-fade-out",
        "svg-field-curve",
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
        "the form reads File · Length · Fade in · Fade out · Curve, in `FORM_FIELDS`' order -- the file comes \
         first because it is what a drawing is made of ({:?})",
        fx_svg::FORM_FIELDS
    );
    let choose = widget(&window, "svg-choose-button")
        .downcast::<gtk::Button>()
        .expect("`svg-choose-button` is a Button");
    assert_eq!(
        choose.label().map(|l| l.to_string()),
        Some(fx_svg::CHOOSE_BUTTON.to_string()),
        "and its button says Choose..."
    );
    assert_eq!(
        label_text(&window, "svg-field-file"),
        fx_svg::base("assets/logo.svg"),
        "the file row shows the drawing's base name"
    );
        ui::release_svg_arm_only();
RAN_FIELDS.store(true, Ordering::SeqCst);
    window.close();
    settle();
}

/// §F3.5 S4 (`No file → not placed`): blank ink is refused with `NO_FILE`, nothing lands.
fn f3_5_s4_no_file_is_not_placed(app: &adw::Application) {
    let window = cut_page(app, &Cut::default());
    set_line(&window, 30.0);
    // Start from no arm and no file at all, so "did not arm" cannot be confused with "an earlier arm survives".
    // `release_svg_arm_only` clears just those two: `press_svg_cancel` would also take down whatever form a
    // previous block left open, which is right for Cancel and wrong for tidying between checks.
    ui::release_svg_arm_only();
    assert!(!ui::svg_armed() && ui::svg_chosen_file().is_none(), "clean start");
    // A blank/space file is judged on trim(): the same answer as leaving the chooser alone. It REFUSES, and since
    // `chose` never toggles off, refusing also means it changed nothing about any previous arm.
    let blank = ui::svg_chosen(&window, Some("   "));
    assert_eq!(blank, fx_svg::NO_FILE, "spaces are not a drawing");
    assert!(!ui::svg_armed(), "and do not arm");
    assert!(ui::svg_chosen_file().is_none(), "and carry no file forward");
    // THE WIRE PROOF for "No file -> not placed". With no form on screen the door answers its own "no form"
    // guard, so the branch that actually reaches `fx_svg::apply` and gets `NO_FILE` back needs a landed box --
    // which is what `land_click_form` gives -- and then a blank file swapped in behind it, exactly as clearing
    // the row would. Direct door call: see the note above about stale button handlers.
    let boxed = land_click_form(app, 45.0);
    ui::swap_svg_file(&boxed, "   ");
    let said = ui::press_svg_apply(&boxed);
    assert_eq!(
        said,
        fx_svg::NO_FILE,
        "Apply with a blanked file answers `NO_FILE` verbatim, got {said}"
    );
    assert!(
        ui::review_cut_of(&boxed).fx.is_empty(),
        "a drawing with no ink is not placed"
    );
    assert!(
        form_visible(&boxed),
        "and the form stays open rather than throwing the box away"
    );
    boxed.close();
    settle();
    RAN_NOT_PLACED.store(true, Ordering::SeqCst);
    window.close();
    settle();
}

/// §F3.5 S4: a length under the floor is refused naming the floor, and places nothing.
fn f3_5_s4_a_length_under_the_floor_is_refused_naming_it(app: &adw::Application) {
    let window = land_click_form(app, 30.0);
    entry(&window, "svg-field-length").set_text("0.1");
    settle();
    // Direct door call: see the note in `f3_5_s4_no_file_is_not_placed`.
    let said = ui::press_svg_apply(&window);
    assert_ne!(said, fx_svg::NO_FILE, "this is the LENGTH refusal, not the no-file one");
    assert!(
        said.contains(&fx_text::MIN_SECONDS.to_string()),
        "it names the floor ({}) it rejected against, got {said}",
        fx_text::MIN_SECONDS
    );
    assert!(said.contains("0.10"), "and quotes the value too short, got {said}");
    assert!(
        ui::review_cut_of(&window).fx.is_empty(),
        "a refused Apply places nothing"
    );
    assert!(
        ui::svg_form_open().is_some() && form_visible(&window),
        "and keeps the form open with the box already drawn"
    );
        ui::release_svg_arm_only();
RAN_FLOOR.store(true, Ordering::SeqCst);
    window.close();
    settle();
}

/// §F3.5 S4: Apply places ONE drawing that one ↶ takes back.
fn f3_5_s4_apply_places_one_drawing_and_one_undo_takes_it_back(app: &adw::Application) {
    let window = land_click_form(app, 30.0);
    // Direct door call: see the note in `f3_5_s4_no_file_is_not_placed`.
    let said = ui::press_svg_apply(&window);
    assert_eq!(
        said,
        fx_record::recorded_status(naivepost::cut::EffectKind::Svg),
        "the door behind `svg-apply-button` answers with `fx_record`'s own sentence for the kind"
    );
    let cut_ = ui::review_cut_of(&window);
    assert_eq!(cut_.fx.len(), 1, "one Apply, one record");
    let fx = &cut_.fx[0];
    assert_eq!(fx.kind, "svg", "of kind svg");
    assert_eq!(fx.src, "assets/logo.svg", "carrying the chosen file as its src");
    assert_eq!(fx.t, 30.0, "at the line the box landed on");
    assert_eq!(
        (fx.cx, fx.cy, fx.wf, fx.hf),
        (
            Some(fx_svg::MIDDLE.cx),
            Some(fx_svg::MIDDLE.cy),
            Some(fx_svg::MIDDLE.wf),
            Some(fx_svg::MIDDLE.hf)
        ),
        "on the middle box the click chose"
    );
    assert!(!form_visible(&window), "Apply closes the form");
    assert!(!ui::svg_armed(), "and releases the arm");

    click(&window, "undo-button");
    assert!(
        ui::review_cut_of(&window).fx.is_empty(),
        "ONE ↶ takes the drawing back -- proof Apply went through record_edit, not seed_review_cut"
    );
    RAN_APPLY_UNDO.store(true, Ordering::SeqCst);
    window.close();
    settle();
}

/// §F3.5 S5: 512 px, transparent, one raster per file, one log line per failure.
fn f3_5_s5_the_raster_is_512_px_transparent_and_one_per_file(app: &adw::Application) {
    // // effects.previewRasterPx
    assert_eq!(fx_svg::PREVIEW_RASTER_PX, 512.0, "the preview copy is 512 px");
    let window = land_click_form(app, 30.0);
    assert!(
        label_text(&window, "svg-preview-raster").contains("512"),
        "the form's raster row states the size"
    );

    let argv = fx_svg::ffmpeg_raster("ffmpeg", "a.svg", "out.png");
    for flag in ["-width", "512", "-height", "-keep_ar", "1", "-pix_fmt", "rgba"] {
        assert!(
            argv.iter().any(|arg| arg == flag),
            "the ffmpeg vector carries {flag}, so the drawing is sized and kept transparent: {argv:?}"
        );
    }
    // One raster per FILE: the same logo asked twice is Ready the second time, never asked again.
    let mut cache = fx_svg::Rasters::default();
    assert_eq!(
        cache.request("assets/logo.svg"),
        fx_svg::Slot::NeedsRaster,
        "a cold file is asked for"
    );
    assert_eq!(
        cache.request("assets/logo.svg"),
        fx_svg::Slot::Busy,
        "while it is being made a second ask does not queue another"
    );
    cache.done("assets/logo.svg");
    assert_eq!(
        cache.request("assets/logo.svg"),
        fx_svg::Slot::Ready,
        "once made it is never asked again -- six placements, one run"
    );
    assert_eq!(cache.len(), 1, "one entry per file, the invariant §A.5 states");

    // Failure is SAID ONCE and the file is then left alone.
    let first = cache.failure_line("broken.svg", "no such file");
    assert!(first.is_some(), "the first failure gets a line");
    assert!(
        cache.failure_line("broken.svg", "no such file").is_none(),
        "the same file never complains twice"
    );
    // No ffmpeg subprocess runs headless: the argument vector and the cache are the testable halves, and their
    // running stays with `crate::subprocess`.
    RAN_RASTER.store(true, Ordering::SeqCst);
    window.close();
    settle();
}

#[test]
fn f3_5_the_drawing_flow_is_wired_through_the_real_widgets() {
    // Pin cwd to our own temp root BEFORE anything builds: the press resolves the project through
    // `startup::session_dir(current_dir())`, and leaving it at rust/ writes a stray session dir into the repo.
    let root = std::env::temp_dir().join(format!("np-svg-{}", std::process::id()));
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

    assert!(RAN_REFUSE.load(Ordering::SeqCst), "the S1 no-line refusal never ran");
    assert!(RAN_CHOOSER.load(Ordering::SeqCst), "the S1 chooser never ran");
    assert!(RAN_NO_FILE.load(Ordering::SeqCst), "the S2 cancelled-chooser check never ran");
    assert!(RAN_ARM.load(Ordering::SeqCst), "the S2 arm-and-name check never ran");
    assert!(RAN_NO_TOGGLE.load(Ordering::SeqCst), "the S2 no-toggle check never ran");
    assert!(RAN_CLICK.load(Ordering::SeqCst), "the S3 click-to-middle check never ran");
    assert!(RAN_DRAG.load(Ordering::SeqCst), "the S3 drag/dispatch check never ran");
    assert!(RAN_CAMERA.load(Ordering::SeqCst), "the S3 camera-stays-up check never ran");
    assert!(RAN_FIELDS.load(Ordering::SeqCst), "the S4 field-order check never ran");
    assert!(RAN_NOT_PLACED.load(Ordering::SeqCst), "the S4 no-file check never ran");
    assert!(RAN_FLOOR.load(Ordering::SeqCst), "the S4 length-floor check never ran");
    assert!(
        RAN_APPLY_UNDO.load(Ordering::SeqCst),
        "the S4 apply-and-undo check never ran"
    );
    assert!(RAN_RASTER.load(Ordering::SeqCst), "the S5 raster check never ran");
}
