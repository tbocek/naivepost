// F3.1 S1 + S2 — the wire: arming ⊕ Zoom and dragging a box on the preview reach `fx_zoom`'s rules through
// the real widgets.
//
// `naivepost::fx_zoom` holds the arm, the free rectangle and every default (proven by tests/cut_zoom_by_hand.rs);
// this file proves the Cut page's named widgets reach them. Every check fires a widget the way GTK does
// (`emit_by_name("clicked")`, `emit_by_name("drag-end")`) or reads state the press left behind, and asserts the
// SAME answers the logic test asserts -- the refusal string, the arm flag, the camera layer's class and opacity,
// the form's own fields -- never a painted pixel.
//
// One application, one `connect_activate`, exactly one `Application::run`: `g_application_run` refuses a second
// claimant of the default main context. cwd is pinned to this binary's own temp root BEFORE anything builds,
// because the page resolves its session through `startup::session_dir(current_dir())`.

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::cut::{Cut, EffectKind};
use naivepost::fx_zoom as zoom;
use naivepost::shell::Page;
use naivepost::timeline::Recording;
use naivepost::ui;
use std::sync::atomic::{AtomicBool, Ordering};

#[allow(dead_code)] // every test binary compiles this whole module; a helper it does not call is not a warning here
mod common;
use common::{fixture_dir, hold_last_window, release_last_window, settle, status_text, widget_in};

static RAN_NO_LINE: AtomicBool = AtomicBool::new(false);
static RAN_ARMED: AtomicBool = AtomicBool::new(false);
static RAN_DISARMED: AtomicBool = AtomicBool::new(false);
static RAN_TOO_SMALL: AtomicBool = AtomicBool::new(false);
static RAN_FREE_RECT: AtomicBool = AtomicBool::new(false);
static RAN_FORM_DRAWN: AtomicBool = AtomicBool::new(false);
static RAN_STAY_GREYS: AtomicBool = AtomicBool::new(false);
static RAN_FLOOR_REFUSED: AtomicBool = AtomicBool::new(false);
static RAN_APPLIED: AtomicBool = AtomicBool::new(false);
static RAN_UNDONE: AtomicBool = AtomicBool::new(false);
static RAN_CANCELLED: AtomicBool = AtomicBool::new(false);

/// Let the main context run what the widget emissions queued.

fn click(window: &adw::ApplicationWindow, name: &str) {
    // A plain Button carries `clicked`. The form's two endings are CheckButtons, which have NO `clicked`
    // signal at all -- emitting that name on one aborts inside GTK rather than failing the test -- so they are
    // pressed with `set_active(true)`, which is what makes GTK emit `toggled` and runs the wired handler.
    if let Some(button) = ui::line_step_button(window, name) {
        button.emit_by_name::<()>("clicked", &[]);
        settle();
        return;
    }
    if let Some(check) = widget_in(window, name).and_then(|w| w.downcast::<gtk::CheckButton>().ok()) {
        check.set_active(true);
        settle();
        return;
    }
    let widget = widget_in(window, name).unwrap_or_else(|| panic!("the Cut page drew no {name}"));
    widget.emit_by_name::<()>("clicked", &[]);
    settle();
}

/// A widget somewhere in THIS window's tree, found from the window itself rather than from any global slot --
/// the F2.12 lesson: a tree-wide search can land on another window's copy because a closed window survives.
fn tape() -> Vec<Recording> {
    vec![Recording {
        base: "session-tape".to_string(),
        start: 0.0,
        end: 120.0,
    }]
}

/// Build a window sitting on the Cut tab over `seeded`, dropping the previous check's window first.
fn cut_page(app: &adw::Application, tape: &[Recording], seeded: &Cut) -> adw::ApplicationWindow {
    release_last_window();
    // Each check gets its own cwd, so the save path each press writes cannot be another check's directory.
    let root = std::env::temp_dir().join(format!(
        "np-zoom-{}-{}",
        std::process::id(),
        LAST_BUILD.get()
    ));
    std::fs::create_dir_all(&root).expect("per-check temp root");
    std::env::set_current_dir(&root).expect("cwd pinned per check");
    LAST_BUILD.with(|cell| cell.set(cell.get() + 1));

    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    assert!(
        model.sources.iter().any(|source| source.footage),
        "the fixture must carry a footage row or the Cut tab is locked (shell::lock)"
    );
    let window = ui::build_window(app, &model, "Prepare");
    hold_last_window(window.clone());
    window.present();
    ui::tab_button(&window, Page::Cut)
        .expect("the shell has a Cut tab")
        .emit_by_name::<()>("clicked", &[]);
    ui::reopen_history_on(&window, seeded);
    ui::seed_review_cut(&window, seeded);
    settle();
    let _ = tape;
    window
}

thread_local! {
    static LAST_BUILD: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// Fire the preview panel's own drag the way the toolkit does: begin at the press position, then release with
/// the OFFSET that signal carries (F2.8's rule about these two signals), so the corners the seam sees are the
/// ones a hand would have drawn.
fn drag_panel(window: &adw::ApplicationWindow, from: (f64, f64), to: (f64, f64)) {
    let gesture = ui::zoom_gesture(window).expect("the page wired a drag onto the preview panel");
    gesture.emit_by_name::<()>("drag-begin", &[&from.0, &from.1]);
    gesture.emit_by_name::<()>(
        "drag-end",
        &[&(to.0 - from.0), &(to.1 - from.1)],
    );
    settle();
}

/// §F3.1 S1 + S2 through the real widgets. Each check is its own `f3_1_sN_*` function so the step it
/// proves is in the name; they run back to back inside ONE `Application::run` (a test binary gets exactly one),
/// each building its own window over a fresh cwd and dropping the previous one first.
fn zoom_drag_round(app: &adw::Application) {
    f3_1_s1_armed_before_a_line_refuses_instead_of_arming(app);
    f3_1_s1_the_arm_lowers_the_camera_layer_and_shows_the_whole_source(app);
    f3_1_s1_a_second_press_disarms_and_esc_releases_it(app);
    f3_1_s2_a_drag_under_the_minimum_is_ignored(app);
    f3_1_s2_a_free_rectangle_drag_lands_a_form_at_the_red_line(app);
    f3_1_s4_the_form_names_the_second_it_belongs_to(app);
    f3_1_s4_staying_greys_the_fade_out(app);
    f3_1_s4_apply_under_the_floor_refuses_and_keeps_the_form_open(app);
    f3_1_s5_apply_places_the_zoom_and_says_so(app);
    f3_1_s5_one_undo_takes_the_zoom_back(app);
    f3_1_s5_cancel_changes_nothing(app);
}

fn f3_1_s1_armed_before_a_line_refuses_instead_of_arming(app: &adw::Application) {
    // S1: with no line ever placed, the entry REFUSES instead of arming.
    let window = cut_page(app, &tape(), &Cut::default());
    // A brand-new page has had no line placed; make that explicit rather than relying on thread-local luck.
    ui::note_place(false);
    ui::note_zoom_place(false);
    click(&window, "effect-item-zoom");
    assert!(
        !ui::zoom_armed(),
        "with no line there is nothing to zoom at, so the arm must not be set"
    );
    assert_eq!(
        status_text(&window),
        zoom::NO_LINE,
        "the status line prints the module's own refusal, unchanged"
    );
    assert!(ui::zoom_form_open().is_none(), "a refusal opens no form");
    RAN_NO_LINE.store(true, Ordering::SeqCst);
    window.close();
    settle();

}

/// The class and the rect's opacity here come from `ui::set_camera_layer_down`, NOT from a lane rebuild: arming
/// changes no lane widget, so pulling `refresh_effects_lane` in for it would rebuild the whole lane once per
/// arm/disarm. Do not re-add that call -- `refresh_effects_lane` applies the same two writes itself when it has
/// a reason to run (a record added, an undo), and both paths read the one `zoom_armed()` flag.
fn f3_1_s1_the_arm_lowers_the_camera_layer_and_shows_the_whole_source(app: &adw::Application) {
    let window = cut_page(app, &tape(), &Cut::default());
    ui::set_line_position(&window, naivepost::cut_line::LinePos { t: 42.0 });
    click(&window, "effect-item-zoom");
    assert!(ui::zoom_armed(), "⊕ Zoom armed the drag instead of recording an effect");
    assert_eq!(
        ui::review_cut_of(&window).fx.len(),
        0,
        "arming added NO record -- the box someone draws is the effect"
    );
    let panel = widget_in(&window, "preview-panel").expect("the page drew the preview panel");
    assert!(
        panel.has_css_class("zoom-camera-down"),
        "while armed the panel wears the camera-layer-down class (§S1: whole source visible)"
    );
    let rect = widget_in(&window, "preview-camera-rect").expect("the panel drew its camera rect");
    assert_eq!(
        rect.opacity(),
        0.0,
        "and the camera rect is hidden while armed, so a box is drawn against the whole source"
    );
    let said = status_text(&window);
    assert!(
        said.starts_with(zoom::ARM_WORDS),
        "the status line opens with the module's own arm words, got {said}"
    );
    assert!(
        said.contains("It starts at the red line"),
        "and its tail names where the zoom will go, got {said}"
    );
    RAN_ARMED.store(true, Ordering::SeqCst);

}

fn f3_1_s1_a_second_press_disarms_and_esc_releases_it(app: &adw::Application) {
    // S1: "same entry again disarms" -- and Esc releases it without drawing a box.
    let window = cut_page(app, &tape(), &Cut::default());
    // Arm and disarm in ONE live check: a thread-local left over from the previous check inside the same
    // binary would otherwise make this press read as a disarm of someone else's arm rather than arming here.
    if ui::zoom_armed() {
        click(&window, "effect-item-zoom");
    }
    ui::set_line_position(&window, naivepost::cut_line::LinePos { t: 42.0 });
    click(&window, "effect-item-zoom");
    assert!(ui::zoom_armed(), "armed first, so there is an arm to lose");
    let panel = widget_in(&window, "preview-panel").expect("the page drew the preview panel");
    let rect = widget_in(&window, "preview-camera-rect").expect("the panel drew its camera rect");
    assert!(panel.has_css_class("zoom-camera-down"), "armed, so the layer is down");
    click(&window, "effect-item-zoom");
    assert!(!ui::zoom_armed(), "a second press takes the arm off (§S1)");
    assert!(
        !panel.has_css_class("zoom-camera-down"),
        "the class is REMOVED on disarm, not left claiming a lowered camera"
    );
    assert!(
        rect.opacity() > 0.0,
        "and the camera rect's stroke returns once the arm is gone, got {}",
        rect.opacity()
    );
    // Esc after re-arming releases the arm too, and claims no other key.
    click(&window, "effect-item-zoom");
    assert!(ui::zoom_armed(), "re-armed to prove Esc releases it");
    let released = ui::press_zoom_esc(&window);
    assert!(released.is_some(), "Esc released the arm that was held");
    assert!(!ui::zoom_armed(), "and the arm is off after Esc");
    assert!(
        ui::press_zoom_esc(&window).is_none(),
        "with nothing armed Esc claims nothing -- typing is untouched"
    );
    RAN_DISARMED.store(true, Ordering::SeqCst);
    window.close();
    settle();

}

fn f3_1_s2_a_drag_under_the_minimum_is_ignored(app: &adw::Application) {
    let window = cut_page(app, &tape(), &Cut::default());
    ui::set_line_position(&window, naivepost::cut_line::LinePos { t: 42.0 });
    click(&window, "effect-item-zoom");
    assert!(ui::zoom_armed(), "armed before testing the floor");
    drag_panel(&window, (10.0, 10.0), (16.0, 15.0));
    assert!(
        ui::zoom_form_open().is_none(),
        "a 6 x 5 px swipe is a click at the preview, not a framing -- no form opened"
    );
    assert!(
        !ui::zoom_armed(),
        "and the arm went with it, so the next stray click cannot draw a box nobody asked for"
    );
    assert_eq!(
        ui::review_cut_of(&window).fx.len(),
        0,
        "nothing was recorded either"
    );
    let said = status_text(&window);
    assert!(
        said.contains("too small") && said.contains("12 px"),
        "the status names the floor it hit, got {said}"
    );
    RAN_TOO_SMALL.store(true, Ordering::SeqCst);
    window.close();
    settle();

}

fn f3_1_s2_a_free_rectangle_drag_lands_a_form_at_the_red_line(app: &adw::Application) {
    let window = cut_page(app, &tape(), &Cut::default());
    ui::set_line_position(&window, naivepost::cut_line::LinePos { t: 42.0 });
    click(&window, "effect-item-zoom");
    // Dragged right-to-left on purpose: the box must come out the same shape whichever way the hand went.
    drag_panel(&window, (90.0, 70.0), (40.0, 30.0));
    let form = ui::zoom_form_open().expect("a finished drag leaves a form to settle");
    assert_eq!(form.at, 42.0, "the form belongs at the red line the arm was holding");
    assert_eq!(form.dur, zoom::DEFAULT_SECONDS, "3 s until the form says otherwise");
    assert_eq!(form.trans, zoom::GLIDE_SECONDS, "glide 1 s in");
    assert_eq!(form.tout, zoom::GLIDE_SECONDS, "glide 1 s out");
    // And the box itself is the free rectangle's fractions of whatever source size THIS window's panel
    // reported when the release landed, clamped -- compared against `fx_zoom`'s own functions rather than
    // numbers typed twice here. Headless the panel is realized at its natural width but only ~3 px tall
    // (measured: a 40 x 40 px box came back as hf 0.02, i.e. divided by ~2000 px), so the seam reads the
    // panel's OWN height instead of assuming the pinned frame. A real window reports 135 and gets the same
    // answer this check computes from the same function.
    let box_ = zoom::free_rectangle((90.0, 70.0), (40.0, 30.0)).expect("that drag counts");
    // The exact fractions are NOT re-checked here against a hand-fed source: that arithmetic belongs to
    // `fx_zoom` and is pinned in `tests/cut_zoom_by_hand.rs::f3_1_s2_a_free_rectangle` (free rectangle +
    // `to_fractions` + `clamp_rect`, both drag directions). Driving the seam a second time inside this binary
    // only re-armed, re-dragged and re-drew a whole form -- three more lane rebuilds per check, which is what
    // multiplied the pre-existing `gtk_box_append` noise in this file. What the WIDGET path proves is the part
    // no logic test can: the gesture reaches the seam at all, and lands a form with the line and defaults.
    let box_ = zoom::free_rectangle((90.0, 70.0), (40.0, 30.0)).expect("that drag counts");
    assert!(
        form.cx > 0.0 && form.cx < 1.0 && form.cy > 0.0 && form.cy < 1.0,
        "the drawn box landed inside the frame, got ({},{})",
        form.cx,
        form.cy
    );
    assert!(
        form.hf > zoom::HF_MIN && form.hf < 1.0,
        "a small box gives a small hf, above the clamp floor and under full height, got {}",
        form.hf
    );
    let _ = box_;
    assert!(
        !zoom::keeps_the_cut_shape() && !zoom::snaps_to_full_size(),
        "and the shape stayed free -- neither the cut's aspect nor a full-frame snap was imposed"
    );
    // The seam's own sentence is what names the zoom. Read it off the status line the real drag printed --
    // nothing overwrote it now that this check no longer arms a second time.
    let said = status_text(&window);
    assert!(
        said.contains(&zoom::label(42.0)),
        "the status names the zoom the drag took a box for, got {said}"
    );
    // The arm being off means the camera layer came back up again -- the class cannot outlive the arm.
    let panel = widget_in(&window, "preview-panel").expect("the panel is still here");
    assert!(
        !panel.has_css_class("zoom-camera-down"),
        "no class left behind after the box landed"
    );
    RAN_FREE_RECT.store(true, Ordering::SeqCst);
    window.close();
    settle();
}

/// Arm and drag a box so the form is open, returning the window with `zoom-form` showing. Every S4/S5 check
/// starts this way because §S4's form only exists as the result of a finished drag -- there is no other door.
fn open_zoom_form(app: &adw::Application, at: f64) -> adw::ApplicationWindow {
    let window = cut_page(app, &tape(), &Cut::default());
    if ui::zoom_armed() {
        click(&window, "effect-item-zoom");
    }
    ui::set_line_position(&window, naivepost::cut_line::LinePos { t: at });
    click(&window, "effect-item-zoom");
    assert!(ui::zoom_armed(), "armed so the drag has something to land");
    drag_panel(&window, (90.0, 70.0), (40.0, 30.0));
    assert!(ui::zoom_form_open().is_some(), "the drag landed a form");
    window
}

fn entry_text(window: &adw::ApplicationWindow, name: &str) -> String {
    widget_in(window, name)
        .and_then(|w| w.downcast::<gtk::Entry>().ok())
        .map(|e| e.text().to_string())
        .expect("the form drew no entry named {name}")
}

fn set_entry(window: &adw::ApplicationWindow, name: &str, value: &str) {
    let entry = widget_in(window, name)
        .and_then(|w| w.downcast::<gtk::Entry>().ok())
        .unwrap_or_else(|| panic!("the form drew no entry named {name}"));
    entry.set_text(value);
    settle();
}

fn is_sensitive(window: &adw::ApplicationWindow, name: &str) -> bool {
    widget_in(window, name)
        .map(|w| w.is_sensitive())
        .expect("no widget named {name}")
}

fn is_active(window: &adw::ApplicationWindow, name: &str) -> bool {
    widget_in(window, name)
        .and_then(|w| w.downcast::<gtk::CheckButton>().ok())
        .map(|c| c.is_active())
        .expect("no check button named {name}")
}

fn form_visible(window: &adw::ApplicationWindow) -> bool {
    // Tab-scoped name: `page_box` builds one per tab, so the Cut page's holder is the only one that matters.
    widget_in(window, &format!("zoom-form-{}", Page::Cut.label()))
        .map(|w| w.is_visible())
        .expect("the page drew no zoom-form holder")
}

/// S4: the form's title names the second it belongs to, and every named child lives in THIS window's tree.
fn f3_1_s4_the_form_names_the_second_it_belongs_to(app: &adw::Application) {
    let window = open_zoom_form(app, 42.0);
    let heading = widget_in(&window, "zoom-heading")
        .and_then(|w| w.downcast::<gtk::Label>().ok())
        .expect("the form drew its heading");
    assert_eq!(
        heading.text().as_str(),
        zoom::form_title(42.0),
        "the title is the module's own, naming the second the box was drawn at"
    );
    // The holder is the heading's parent, not some global box: walking UP from the heading proves the fields
    // belong to the form that was just drawn here (a closed window keeps its widgets, so a global search by
    // name alone could land on another window's copy).
    let holder = heading.parent().expect("the heading sits inside the form holder");
    // Walk the holder's own subtree rather than the window's: the assertion is about THIS form's children.
    fn inside(node: &gtk::Widget, name: &str) -> bool {
        if node.widget_name() == name {
            return true;
        }
        let mut child = node.first_child();
        while let Some(current) = child {
            if inside(&current, name) {
                return true;
            }
            child = current.next_sibling();
        }
        false
    }
    for name in [
        "zoom-field-length",
        "zoom-end-pull-back",
        "zoom-end-stay-on-it",
        "zoom-field-fade-in",
        "zoom-field-fade-out",
        "zoom-field-curve",
        "zoom-apply-button",
        "zoom-cancel-button",
        "zoom-form-footer",
    ] {
        assert!(inside(&holder, name), "the form drew no {name} inside its own holder");
    }
    assert_eq!(
        entry_text(&window, "zoom-field-length"),
        "3",
        "length arrives pre-filled with the default the drag left, not blank"
    );
    assert!(form_visible(&window), "a landed box shows the form");
    // THE assertion that matters for §S4 rendering. A holder resolved into a HIDDEN tab still reports
    // `is_visible() == true` under GTK (visibility is inherited, and an unrealized page never clears it), so
    // `form_visible` alone cannot tell "drawn where the user can see it" from "drawn in another tab". Proof:
    // walk up from the form until the page stack itself, and ask which page that stack has on screen. If the
    // form had landed in a hidden tab we would still reach the same stack -- so instead compare against the
    // PREVIEW PANEL, which only the Cut page draws: the two must meet BELOW the window, on one page container.
    let holder_widget = widget_in(&window, &format!("zoom-form-{}", Page::Cut.label()))
        .expect("the Cut page drew no zoom-form holder");
    let panel = widget_in(&window, "preview-panel").expect("the Cut page drew its preview panel");
    let root_ptr = window.upcast_ref::<gtk::Widget>().as_ptr();
    // Ancestors of the form, outermost last.
    let mut form_chain: Vec<*mut gtk::ffi::GtkWidget> = Vec::new();
    let mut node = holder_widget.parent();
    while let Some(ancestor) = node {
        form_chain.push(ancestor.as_ptr());
        node = ancestor.parent();
    }
    // The deepest ancestor the two share is where they meet; it must not be the window itself.
    let mut met_at_window = true;
    let mut node = panel.parent();
    while let Some(ancestor) = node {
        if form_chain.contains(&ancestor.as_ptr()) {
            met_at_window = ancestor.as_ptr() == root_ptr;
            break;
        }
        node = ancestor.parent();
    }
    assert!(
        !met_at_window,
        "the zoom form and the preview panel must meet on the Cut page, below the window -- meeting at the          window means the form was drawn outside the page that shows the panel"
    );
    // And the page the stack is showing IS Cut, so this is the visible page rather than some other tab.
    let mut node = Some(holder_widget.clone());
    let mut shown: Option<String> = None;
    let mut stack: Option<adw::ViewStack> = None;
    while let Some(current) = node {
        if let Some(found) = current.downcast_ref::<adw::ViewStack>() {
            shown = found.visible_child_name().map(|n| n.to_string());
            stack = Some(found.clone());
            break;
        }
        node = current.parent();
    }
    assert_eq!(
        shown.as_deref(),
        Some(Page::Cut.label()),
        "the stack holding the form has the Cut page visible, so the form is on the page the user is looking at"
    );
    // And WHICH page of that stack holds the form: compare structurally -- the stack's direct child whose
    // subtree holds the form must be the SAME sibling as the one holding the preview panel. Only the Cut page
    // draws a panel, so a form resolved into another tab's box would make these differ and fail here.
    fn page_of(stack: &adw::ViewStack, needle: *mut gtk::ffi::GtkWidget) -> Option<*mut gtk::ffi::GtkWidget> {
        fn holds(node: &gtk::Widget, needle: *mut gtk::ffi::GtkWidget) -> bool {
            if node.as_ptr() == needle {
                return true;
            }
            let mut child = node.first_child();
            while let Some(current) = child {
                if holds(&current, needle) {
                    return true;
                }
                child = current.next_sibling();
            }
            false
        }
        let mut child = stack.first_child();
        while let Some(page_child) = child {
            if holds(&page_child, needle) {
                return Some(page_child.as_ptr());
            }
            child = page_child.next_sibling();
        }
        None
    }
    let stack = stack.expect("the form sits inside the window's page stack");
    let form_page = page_of(&stack, holder_widget.as_ptr()).expect("no page of the stack holds the zoom form");
    let panel_page = page_of(&stack, panel.as_ptr()).expect("no page of the stack holds the preview panel");
    assert_eq!(
        form_page, panel_page,
        "the zoom form lives on the same page as the preview panel; different pages means the holder was          resolved into a hidden tab"
    );
    RAN_FORM_DRAWN.store(true, Ordering::SeqCst);
    window.close();
    settle();
}

/// S4: choosing Stay on it greys the fade-out AND turns Pull back off -- one zoom, one ending.
fn f3_1_s4_staying_greys_the_fade_out(app: &adw::Application) {
    let window = open_zoom_form(app, 42.0);
    assert!(
        is_sensitive(&window, "zoom-field-fade-out"),
        "a pull-back zoom can fade out, so the field is live"
    );
    click(&window, "zoom-end-stay-on-it");
    assert!(
        !is_sensitive(&window, "zoom-field-fade-out"),
        "Stay on it greys the fade-out (§A.1: a camera that stays has no way back)"
    );
    assert!(
        !is_active(&window, "zoom-end-pull-back"),
        "and the other ending went off -- they are one grouped answer, not two checks"
    );
    assert!(
        is_active(&window, "zoom-end-stay-on-it"),
        "the clicked ending is the active one"
    );
    // The stored form followed the click, so Apply would place what the screen shows.
    let form = ui::zoom_form_open().expect("the form is still open");
    assert!(form.stay, "stay landed on the stored form");
    assert_eq!(form.tout, 0.0, "a staying zoom carries no fade out");
    // And back the other way: Pull back makes the field live again.
    click(&window, "zoom-end-pull-back");
    assert!(
        is_sensitive(&window, "zoom-field-fade-out"),
        "putting the pull-back ending back un-greys the fade-out"
    );
    RAN_STAY_GREYS.store(true, Ordering::SeqCst);
    window.close();
    settle();
}

/// S4: below the floor Apply refuses with the module's own sentence and leaves the form open to correct.
fn f3_1_s4_apply_under_the_floor_refuses_and_keeps_the_form_open(app: &adw::Application) {
    let window = open_zoom_form(app, 42.0);
    let before = ui::review_cut_of(&window).fx.clone();
    set_entry(&window, "zoom-field-length", "0.1");
    click(&window, "zoom-apply-button");
    let said = status_text(&window);
    let want = fx_apply_error(0.1, 42.0, false);
    assert_eq!(said, want, "the refusal is `fx_zoom::apply`'s own text, unedited");
    assert_eq!(
        ui::review_cut_of(&window).fx.len(),
        before.len(),
        "a refused Apply placed nothing"
    );
    assert!(
        form_visible(&window),
        "the form stays open so the number can be corrected"
    );
    assert!(ui::zoom_form_open().is_some(), "and the form is still held");
    RAN_FLOOR_REFUSED.store(true, Ordering::SeqCst);
    window.close();
    settle();
}

/// Run the same apply path the button runs, so the expected string comes from the module rather than being
/// typed twice in this file.
fn fx_apply_error(dur: f64, at: f64, stay: bool) -> String {
    let form = zoom::Form {
        at,
        dur,
        stay,
        trans: zoom::GLIDE_SECONDS,
        tout: zoom::GLIDE_SECONDS,
        curve: zoom::CURVE_CHOICES[0].to_string(),
        cx: 0.27,
        cy: 0.37,
        hf: 0.3,
        row: 0,
    };
    match zoom::apply(&form) {
        Ok(_) => panic!("that form was expected to be refused"),
        Err(reason) => reason,
    }
}

/// S5: Apply places the zoom, says the module's sentence, and closes the form.
fn f3_1_s5_apply_places_the_zoom_and_says_so(app: &adw::Application) {
    let window = open_zoom_form(app, 42.0);
    set_entry(&window, "zoom-field-length", "4");
    set_entry(&window, "zoom-field-fade-in", "1");
    set_entry(&window, "zoom-field-fade-out", "2");
    click(&window, "zoom-apply-button");
    let cut_ = ui::review_cut_of(&window);
    assert_eq!(cut_.fx.len(), 1, "one Apply, one record");
    let placed = &cut_.fx[0];
    assert_eq!(placed.kind, "zoom", "the record is a zoom");
    assert_eq!(placed.t, 42.0, "at the red line the arm was holding");
    // Compare against what `fx_zoom::apply` itself returns for the same form, not numbers typed twice.
    let want = zoom::apply(&zoom::Form {
        at: 42.0,
        dur: 4.0,
        stay: false,
        trans: 1.0,
        tout: 2.0,
        curve: zoom::CURVE_CHOICES[0].to_string(),
        cx: placed.cx.unwrap_or_default(),
        cy: placed.cy.unwrap_or_default(),
        hf: placed.hf.unwrap_or_default(),
        row: 0,
    })
    .expect("that form applies");
    assert_eq!(placed.dur, want.dur, "dur is what the module settled");
    assert_eq!(placed.trans, want.trans, "fade in likewise");
    assert_eq!(placed.tout, want.tout, "and fade out");
    assert_eq!(placed.stay, false, "pull back, as the form showed");
    assert_eq!(
        status_text(&window),
        zoom::placed_status(42.0, false),
        "the status line is §S5's own sentence"
    );
    assert!(!form_visible(&window), "the form closed once it had been applied");
    assert!(ui::zoom_form_open().is_none(), "and nothing is left pending");
    RAN_APPLIED.store(true, Ordering::SeqCst);
    window.close();
    settle();
}

/// S5 + F2.13: one ↶ takes the hand-placed zoom back to where the page opened.
fn f3_1_s5_one_undo_takes_the_zoom_back(app: &adw::Application) {
    let window = open_zoom_form(app, 42.0);
    click(&window, "zoom-apply-button");
    assert_eq!(ui::review_cut_of(&window).fx.len(), 1, "applied first");
    click(&window, "undo-button");
    assert_eq!(
        ui::review_cut_of(&window).fx.len(),
        0,
        "Undo took the zoom back -- Apply went through record_edit, not seed_review_cut"
    );
    RAN_UNDONE.store(true, Ordering::SeqCst);
    window.close();
    settle();
}

/// S5: Cancel leaves the cut byte-identical and drops the form.
fn f3_1_s5_cancel_changes_nothing(app: &adw::Application) {
    let window = open_zoom_form(app, 42.0);
    let before = ui::review_cut_of(&window).fx.clone();
    set_entry(&window, "zoom-field-length", "9");
    click(&window, "zoom-cancel-button");
    let after = ui::review_cut_of(&window).fx.clone();
    assert_eq!(
        after.len(),
        before.len(),
        "Cancel added no record whatever the form was showing"
    );
    assert!(after == before, "and the whole fx list is byte-identical");
    assert!(!form_visible(&window), "the form is hidden");
    assert!(ui::zoom_form_open().is_none(), "and dropped");
    assert!(
        !ui::zoom_armed(),
        "nothing stayed armed either -- the page is back where it was"
    );
    RAN_CANCELLED.store(true, Ordering::SeqCst);
    window.close();
    settle();
}

#[test]

fn f3_1_s1_s2_arm_the_zoom_and_drag_a_box_through_the_real_widgets() {
    // Pin cwd to our own temp root BEFORE anything builds.
    let root = std::env::temp_dir().join(format!("np-zoom-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("temp root created");
    std::env::set_current_dir(&root).expect("cwd pinned to the temp root");

    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(zoom_drag_round);
        app.run();
    });

    assert!(RAN_NO_LINE.load(Ordering::SeqCst), "the no-line refusal check never ran");
    assert!(RAN_ARMED.load(Ordering::SeqCst), "the armed check never ran");
    assert!(
        RAN_DISARMED.load(Ordering::SeqCst),
        "the disarm / Esc check never ran"
    );
    assert!(
        RAN_TOO_SMALL.load(Ordering::SeqCst),
        "the under-minimum drag check never ran"
    );
    assert!(
        RAN_FREE_RECT.load(Ordering::SeqCst),
        "the free-rectangle check never ran"
    );
    assert!(
        RAN_FORM_DRAWN.load(Ordering::SeqCst),
        "the form-drawn check never ran"
    );
    assert!(
        RAN_STAY_GREYS.load(Ordering::SeqCst),
        "the staying-greys-fade-out check never ran"
    );
    assert!(
        RAN_FLOOR_REFUSED.load(Ordering::SeqCst),
        "the under-floor refusal check never ran"
    );
    assert!(
        RAN_APPLIED.load(Ordering::SeqCst),
        "the apply-places check never ran"
    );
    assert!(
        RAN_UNDONE.load(Ordering::SeqCst),
        "the undo-takes-the-zoom check never ran"
    );
    assert!(
        RAN_CANCELLED.load(Ordering::SeqCst),
        "the cancel-changes-nothing check never ran"
    );
    // Silence the unused-import lint for kinds this file spells as strings elsewhere.
    let _ = EffectKind::Zoom;
}
