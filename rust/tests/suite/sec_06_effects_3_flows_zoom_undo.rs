// §06-effects#3-flows — a zoom Apply that is REFUSED must not spend an undo entry.
//
// `zoom_drag_widgets.rs` already drives the whole F3.1 apply path through the real widgets and pins more of it
// than this file could add: S5's placement and its fields (`f3_1_s5_apply_places_the_zoom_and_says_so`), the ↶
// after a placed zoom (`f3_1_s5_one_undo_takes_the_zoom_back`), Cancel changing nothing byte-identically
// (`f3_1_s5_cancel_changes_nothing`), and S4's floor refusal down to the status line being `fx_zoom::apply`'s own
// text, unchanged (`f3_1_s4_apply_under_the_floor_refuses_and_keeps_the_form_open`). None of those asks the history
// a question. So the gap this file pins is exactly one thing, §F3.8 S5's rule as it applies to THIS door: an Apply
// refused for a Length under `fx_zoom::MIN_SECONDS` writes no record AND leaves ↶ with nothing to take back — the
// refusal gives the visit's Undo entry rather than spending it. A first Apply in a visit therefore has to stay the
// first answer (`fx_band::LiveForm::first_answer`, reset by `ui::live_form_refused`).
//
// The depth of the history is never read: there is no seam for it and none is added. "Nothing behind ↶" is asked
// through the app's own verb, whose refusal is §F2.13's sentence.
//
// One application, one `connect_activate`, exactly one `Application::run` — `rust/justfile`'s `_files` recipe runs
// this file as a process of its own (`"$bin" "suite::<file>::"`), so nothing else competes for the main context.
// cwd is pinned per check before anything builds, because the page resolves its session through
// `startup::session_dir(current_dir())` and Apply saves through it.

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use naivepost::cut::{Cut, Seg};
use naivepost::ui;
use naivepost::{fx_zoom, shell::Page};

use crate::common;
use common::{cut_page_settled_first as cut_page, settle, wait_until, widget_in};

static RAN_REFUSED: AtomicBool = AtomicBool::new(false);
static RAN_APPLIED: AtomicBool = AtomicBool::new(false);
/// Why the activation stopped, if it did: a panic inside GTK's activate is non-unwinding (an abort with nothing
/// worth reading), so the reason is carried into the entry test instead of vanishing.
static STOP_REASON: std::sync::LazyLock<std::sync::Arc<std::sync::Mutex<Option<String>>>> =
    std::sync::LazyLock::new(|| std::sync::Arc::new(std::sync::Mutex::new(None)));

/// Run one check and remember why it stopped rather than taking the whole binary down with an unhelpful "never ran".
macro_rules! step {
    ($which:expr, $body:expr) => {{
        if let Err(why) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| $body)) {
            let text = why
                .downcast_ref::<&str>()
                .map(|s| (*s).to_string())
                .or_else(|| why.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "a panic with no message".to_string());
            *STOP_REASON.lock().unwrap() = Some(format!("{}: {text}", $which));
            return;
        }
    }};
}

/// Its own cwd, so this check's save cannot land in another check's directory. `temp_dir()` already carries the
/// process id, so the tag alone keeps the checks apart.
fn pin_cwd(tag: &str) {
    let root = std::env::temp_dir().join(format!("np-s06e3undo-{tag}"));
    std::fs::create_dir_all(&root).expect("per-check temp root");
    std::env::set_current_dir(&root).expect("cwd pinned per check");
}

/// How many zoom records the page's cut holds right now.
fn zoom_count(window: &adw::ApplicationWindow) -> usize {
    ui::review_cut_of(window)
        .fx
        .iter()
        .filter(|effect| effect.kind == "zoom")
        .count()
}

/// Does ↶ answer its own §F2.13 refusal? True means the stack has nothing below its pointer.
fn nothing_behind_undo(window: &adw::ApplicationWindow) -> bool {
    ui::press_undo(window).contains("nothing to undo")
}

/// Fire the preview panel's own drag the way the toolkit does (`zoom_drag_widgets.rs`'s recipe): begin at the
/// press position, then release with the OFFSET that signal carries — GTK's state machine has no event sequence
/// behind an emitted signal, so both go in one go with no `settle()` between them.
fn drag_panel(window: &adw::ApplicationWindow, from: (f64, f64), to: (f64, f64)) {
    let gesture = ui::zoom_gesture(window).expect("the page wired a drag onto the preview panel");
    gesture.emit_by_name::<()>("drag-begin", &[&from.0, &from.1]);
    gesture.emit_by_name::<()>("drag-end", &[&(to.0 - from.0), &(to.1 - from.1)]);
    settle();
}

/// Is this window's zoom form on screen? The holder's name is tab-scoped (`page_box` builds one per tab), and it
/// is found from THIS window because a closed window survives with its tree intact.
fn form_visible(window: &adw::ApplicationWindow) -> bool {
    widget_in(window, &format!("zoom-form-{}", Page::Cut.label()))
        .expect("the page drew no zoom-form holder")
        .is_visible()
}

/// A timeline long enough that a zoom at 81 s has room, with footage so the Cut tab is unlocked.
fn seeded() -> Cut {
    Cut {
        segs: vec![Seg {
            s: 0.0,
            e: 600.0,
            cam: 0,
            ..Default::default()
        }],
        ..Default::default()
    }
}

/// Arm ⊕ Zoom and draw a box, so the form that opens is the one a hand gets. The page has ONE notion of "a line
/// exists" (`press_zoom_item` consults both seams), so both are told before arming.
fn open_zoom_form(app: &adw::Application, tag: &str) -> adw::ApplicationWindow {
    pin_cwd(tag);
    let window = cut_page(app, &seeded());
    ui::note_place(true);
    ui::note_zoom_place(true);
    // Whatever an earlier check left behind: Esc releases a held arm, Cancel drops a held form.
    ui::press_zoom_esc(window.upcast_ref());
    ui::press_zoom_cancel(window.upcast_ref());
    ui::set_line_position(&window, naivepost::cut_line::LinePos { t: 81.0 });
    let said = ui::press_zoom_item(&window);
    assert!(ui::zoom_armed(), "⊕ Zoom did not arm -- {said}");
    drag_panel(&window, (90.0, 70.0), (40.0, 30.0));
    let form = ui::zoom_form_open().expect("the drag landed a form");
    assert_eq!(
        form.at, 81.0,
        "the form belongs at the red line this check parked"
    );
    wait_until("the zoom form is on screen", || form_visible(&window));
    window
}

/// Type into one of the form's fields through the door `press_zoom_apply` itself reads.
fn set_entry(name: &str, value: &str) {
    naivepost::ui::cut_zoom_hand::zoom_entry(name)
        .unwrap_or_else(|| panic!("the form drew no {name}"))
        .set_text(value);
}

/// S4 + §F3.8 S5: the refusal writes nothing and spends nothing.
fn a_refused_apply_spends_no_undo_entry(app: &adw::Application) {
    let window = open_zoom_form(app, "refused");
    let before = zoom_count(&window);
    assert!(
        nothing_behind_undo(&window),
        "a page that has taken no edit starts with nothing behind ↶"
    );
    set_entry(
        "zoom-field-length",
        &format!("{}", fx_zoom::MIN_SECONDS / 2.0),
    );
    let said = ui::press_zoom_apply(&window);
    settle();
    assert!(
        said.contains(&format!("{}", fx_zoom::MIN_SECONDS)),
        "the refusal names the floor it broke: {said}"
    );
    assert_eq!(
        zoom_count(&window),
        before,
        "a refused Apply wrote no record"
    );
    assert!(form_visible(&window), "and leaves the form open to correct");
    assert!(
        nothing_behind_undo(&window),
        "the refusal gave the visit's Undo entry back instead of spending it (§F3.8 S5)"
    );
    // And the correction that follows is still the visit's FIRST answer: the same Apply that would have been the
    // first had the refusal never happened places its zoom and earns its own ↶.
    set_entry(
        "zoom-field-length",
        &format!("{}", fx_zoom::MIN_SECONDS * 4.0),
    );
    let applied = ui::press_zoom_apply(&window);
    settle();
    assert_eq!(
        zoom_count(&window),
        before + 1,
        "the corrected Apply placed one zoom ({applied})"
    );
    assert!(
        !nothing_behind_undo(&window),
        "the placement earned its undo step: {applied}"
    );
    RAN_REFUSED.store(true, Ordering::SeqCst);
    window.close();
    settle();
}

/// The mirror image, which is what makes the check above mean something: an ACCEPTED Apply does put a snapshot
/// behind ↶, so "nothing behind ↶" in the refusal case is a verdict about the refusal, not a dead assertion.
/// The record's own fields belong to `zoom_drag_widgets.rs`; only the history step is asked here.
fn an_accepted_apply_earns_the_undo_it_promises(app: &adw::Application) {
    let window = open_zoom_form(app, "accepted");
    let before = zoom_count(&window);
    let applied = ui::press_zoom_apply(&window);
    settle();
    assert_eq!(
        zoom_count(&window),
        before + 1,
        "Apply landed one zoom ({applied})"
    );
    // The promise is in the sentence Apply RETURNS. The label is not asserted here: a second window on the same
    // shell leaves more than one `status-line` in this process (a closed window survives) and `common::status_text`
    // finds the first, which need not be ours — `zoom_drag_widgets.rs` pins that write against its own page.
    assert!(
        applied.contains("Undo takes it back"),
        "the status sentence Apply handed back promises the undo it owes: {applied}"
    );
    assert!(!nothing_behind_undo(&window), "and a snapshot is behind ↶");
    ui::line_step_button(&window, "undo-button")
        .expect("the toolbar drew undo-button")
        .emit_by_name::<()>("clicked", &[]);
    settle();
    assert_eq!(
        zoom_count(&window),
        before,
        "↶ pressed on the toolbar took the zoom back"
    );
    assert!(
        nothing_behind_undo(&window),
        "and the stack is empty again -- one step in, one step out"
    );
    RAN_APPLIED.store(true, Ordering::SeqCst);
    window.close();
    settle();
}

fn round(app: &adw::Application) {
    step!(
        "s4 refused apply",
        a_refused_apply_spends_no_undo_entry(app)
    );
    step!(
        "s5 accepted apply",
        an_accepted_apply_earns_the_undo_it_promises(app)
    );
}

#[test]
fn sec_06_effects_3_flows_s4_a_refused_zoom_apply_spends_no_undo_entry() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(round);
        app.run_with_args::<&str>(&[]);
    });

    let stopped = STOP_REASON
        .lock()
        .unwrap()
        .clone()
        .map(|why| format!(" -- {why}"))
        .unwrap_or_default();
    assert!(
        RAN_REFUSED.load(Ordering::SeqCst),
        "the refused-apply check never ran{stopped}"
    );
    assert!(
        RAN_APPLIED.load(Ordering::SeqCst),
        "the accepted-apply check never ran{stopped}"
    );
}
