// F3.10 Speeds proposed by the model — the WIRE: a click on ⏩ Speeds reaches the rules in
// `naivepost::cut_speed_pass` through a real widget, and what they answer lands on the cut, the lane, the
// status line and the log instead of staying inside a callback.
//
// `rust/tests/cut_proposed_speeds.rs` proves the rules themselves (the one-call brief, the per-call refusal,
// rate ≈ 1 ignored, the ramp arithmetic, the gap merge, the two lines); this file proves the Cut page can
// actually reach them. Before this round `grep -n cut_speed_pass src/ui/window.rs` returned nothing at all.
//
// No live model is dialled here. The reply is fed through `ui::run_speeds_pass_with_reply`, the same split
// F3.9 makes with `run_captions_pass_with_reply`: one seam for the telephone, one for the rule.

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::cut::{Cut, Fx, Seg};
use naivepost::cut_speed_pass::{self, Call};
use naivepost::shell::Page;
use naivepost::ui;

static RAN_CLICK: AtomicBool = AtomicBool::new(false);
static RAN_PLACED_REFUSED_IGNORED: AtomicBool = AtomicBool::new(false);
static RAN_MERGED: AtomicBool = AtomicBool::new(false);
static RAN_GATE: AtomicBool = AtomicBool::new(false);
static RAN_UNDO: AtomicBool = AtomicBool::new(false);
static RAN_NO_ANSWER: AtomicBool = AtomicBool::new(false);

fn fixture_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/demo.naivepost")
}

thread_local! {
    /// Strong handle on the window last built, so the next check drops it first: a closed GTK window is not
    /// destroyed and its names stay parented, which would send a lookup to the wrong tree.
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

fn widget_in(window: &adw::ApplicationWindow, name: &str) -> Option<gtk::Widget> {
    fn walk(node: &gtk::Widget, name: &str) -> Option<gtk::Widget> {
        if node.widget_name() == name {
            return Some(node.clone());
        }
        if let Some(child) = node.first_child() {
            let mut cursor = Some(child);
            while let Some(current) = cursor {
                if let Some(found) = walk(&current, name) {
                    return Some(found);
                }
                cursor = current.next_sibling();
            }
        }
        None
    }
    walk(window.upcast_ref(), name)
}

fn status_text(window: &adw::ApplicationWindow) -> String {
    ui::find_status(window.upcast_ref())
        .expect("the shell has a status line")
        .text()
        .to_string()
}

/// Build a window sitting on the Cut tab with the given cut seeded, dropping the previous check's window first.
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
    ui::reopen_history_on(&window, seeded);
    ui::seed_review_cut(&window, seeded);
    ui::refresh_effects_lane(&window);
    settle();
    window
}

/// Three kept runs of ten seconds each: clip 1 = 0–10, clip 2 = 10–20, clip 3 = 20–30, so every second the
/// pass writes down is checkable by hand.
fn three_clips() -> Cut {
    let mut cut_ = Cut::default();
    cut_.segs = vec![
        Seg { s: 0.0, e: 10.0, cam: 0, ..Default::default() },
        Seg { s: 10.0, e: 20.0, cam: 0, ..Default::default() },
        Seg { s: 20.0, e: 30.0, cam: 0, ..Default::default() },
    ];
    cut_
}

/// The same three clips with one caption already over clip 3, which is what makes S2's refusal real rather than
/// theoretical: the pass reads captions off the cut it was given, not off a list someone remembered to pass.
fn three_clips_captioned() -> Cut {
    let mut cut_ = three_clips();
    cut_.fx.push(Fx {
        kind: "text".to_string(),
        t: 21.0,
        dur: 4.0,
        text: "a line already there".to_string(),
        ..Default::default()
    });
    cut_
}

// --- the round ----------------------------------------------------------------------------------------

fn speeds_round(app: &adw::Application) {
    // --- (1) a real click answers, and invents nothing when no model was dialled -------------------
    let window = cut_page(app, &three_clips());
    let before = ui::review_cut_of(&window).fx.len();
    let button = widget_in(&window, "speeds-pass-button").expect("⏩ Speeds is on the run-bar row");
    button
        .clone()
        .downcast::<gtk::Button>()
        .expect("it is a button")
        .emit_by_name::<()>("clicked", &[]);
    settle();
    assert_eq!(
        ui::review_cut_of(&window).fx.len(),
        before,
        "with no proposal from the model no rate is invented into the cut"
    );
    let said = status_text(&window);
    assert!(
        said.contains("clip") || said.contains("speed"),
        "the status says what the pass did, got {said:?}"
    );
    RAN_CLICK.store(true, Ordering::SeqCst);
    window.close();
    settle();

    // --- (2) placed / refused / ignored, all three branches, through the SAME route --------------
    let window = cut_page(app, &three_clips_captioned());
    let seeded_fx = ui::review_cut_of(&window).fx.len();
    let calls = [
        Call { clip: 1, rate: 2.0 },
        // Clip 3 carries a caption: a fast rate over it is REFUSED per call, and the reason MUST be said.
        Call { clip: 3, rate: 2.0 },
        // Rate ≈ 1 is silence, not even a refusal.
        Call { clip: 2, rate: 1.0 },
    ];
    let said = ui::run_speeds_pass_with_reply(&window, &calls);
    let fx: Vec<Fx> = ui::review_cut_of(&window).fx.clone();
    let added: Vec<&Fx> = fx.iter().skip(seeded_fx).collect();
    assert_eq!(added.len(), 1, "only the ×2 over clip 1 landed, got {added:?}");
    // What `place` really writes: kind "speed", the clip's OWN start and length, the applied rate.
    assert_eq!(added[0].kind, "speed", "a proposed speed is a speed effect");
    assert_eq!(added[0].t, 0.0, "clip 1 starts at session second 0");
    assert_eq!(added[0].dur, 10.0, "the effect covers the whole clip");
    assert_eq!(added[0].rate, 2.0, "the rate the tool reported back");
    assert!(
        said.contains("cannot be sped up"),
        "S2: the refusal is SAID, not silently dropped -- got {said:?}"
    );
    assert!(
        !added.iter().any(|e| (e.t - 20.0).abs() < 0.001),
        "nothing covers clip 3, whose caption held the rate back"
    );
    assert!(
        !added.iter().any(|e| (e.t - 10.0).abs() < 0.001),
        "the rate-1 call left no record over clip 2 -- it was ignored, not written"
    );
    RAN_PLACED_REFUSED_IGNORED.store(true, Ordering::SeqCst);
    window.close();
    settle();

    // --- (3) same-rate stretches nearer than the gap merge into ONE run --------------------------
    // Two ×2 stretches with a 3 s GAP between them: nearer than P.policy.speedGapSeconds (4), so they fold.
    // The segments are laid end-to-gap-end so the merged pair leaves a real gap; the count is asserted against
    // `cut_speed_pass::merge` itself rather than a hand-tallied number, so the test pins the behaviour and
    // follows the rule if the rule moves.
    let mut gap_cut = Cut::default();
    gap_cut.segs = vec![
        Seg { s: 0.0, e: 10.0, cam: 0, ..Default::default() },
        // clip 2 is NOT asked about -- it stays at 1x, which is what leaves the two fast runs separated.
        Seg { s: 10.0, e: 13.0, cam: 0, ..Default::default() },
        Seg { s: 13.0, e: 23.0, cam: 0, ..Default::default() },
    ];
    let window = cut_page(app, &gap_cut);
    let calls = [Call { clip: 1, rate: 2.0 }, Call { clip: 3, rate: 2.0 }];
    let _ = ui::run_speeds_pass_with_reply(&window, &calls);
    let after: Vec<Fx> = ui::review_cut_of(&window).fx.clone();
    let placed_now: Vec<Fx> = after.into_iter().filter(|e| e.kind == "speed").collect();
    // Raw, unmerged, the two runs start 13 s apart and each is 10 s long, so the second begins 3 s after the
    // first ends -- inside the gap.
    let raw = vec![
        Fx { kind: "speed".into(), t: 0.0, dur: 10.0, rate: 2.0, ..Default::default() },
        Fx { kind: "speed".into(), t: 13.0, dur: 10.0, rate: 2.0, ..Default::default() },
    ];
    assert_eq!(raw[1].t - (raw[0].t + raw[0].dur), 3.0, "the two fast runs are 3 s apart");
    let merged_len = cut_speed_pass::merge(&raw).len();
    assert_eq!(merged_len, 1, "and the module folds that pair into one run");
    assert_eq!(
        placed_now.len(),
        merged_len,
        "what landed equals what the module's own merge folds them to (a gap of at most {} s merges)",
        cut_speed_pass::GAP_SECONDS
    );
    // P.policy.speedGapSeconds
    assert_eq!(
        naivepost::params::cut()
            .into_iter()
            .find(|param| param.id == "P.policy.speedGapSeconds")
            .expect("the Cut rows carry the speed gap")
            .spelled,
        "4",
        "// P.policy.speedGapSeconds -- params.rs reads the same constant the merge rule uses"
    );
    assert_eq!(cut_speed_pass::GAP_SECONDS, 4.0, "// P.policy.speedGapSeconds");
    RAN_MERGED.store(true, Ordering::SeqCst);
    window.close();
    settle();

    // --- (4) the gate: speedPass off greys the control and refuses the press ---------------------
    let window = cut_page(app, &three_clips());
    ui::set_speeds_pass(&window, false);
    ui::refresh_speeds_gate(&window);
    let button = ui::speeds_pass_button(&window).expect("⏩ Speeds is on the run-bar row");
    assert!(
        !button.is_sensitive(),
        "the control greys out when the pass is off -- a grey button always means the pass would refuse"
    );
    let refused = ui::run_speeds_pass(&window);
    assert!(refused.contains("off"), "the pass itself refuses, got {refused}");
    assert_eq!(ui::review_cut_of(&window).fx.len(), 0, "and places nothing");
    // Also with a scripted reply: the gate is checked in the shared body, so no door gets around it.
    let still_refused = ui::run_speeds_pass_with_reply(&window, &[Call { clip: 1, rate: 3.0 }]);
    assert!(still_refused.contains("off"), "nor does the with_reply seam, got {still_refused}");
    // An insensitive button swallows the emission, so what is asserted is the state rather than a
    // fired-and-ignored click: GTK will not run the handler of a control it has greyed out.
    assert!(!button.is_sensitive(), "still grey after the refused presses");
    ui::set_speeds_pass(&window, true);
    ui::refresh_speeds_gate(&window);
    assert!(button.is_sensitive(), "switching the policy back on re-enables the control");
    RAN_GATE.store(true, Ordering::SeqCst);
    window.close();
    settle();

    // --- (5) ↶ takes the whole pass back --------------------------------------------------------
    let window = cut_page(app, &three_clips());
    let said = ui::run_speeds_pass_with_reply(
        &window,
        &[Call { clip: 1, rate: 2.0 }, Call { clip: 2, rate: 3.0 }],
    );
    assert!(said.contains("run fast"), "the pass reported its result, got {said}");
    // ADJACENT clips at DIFFERENT rates are two decisions and do not merge (§F3.10's rule as written in
    // `merge`: only same-rate stretches fold), so both are on the lane before the undo.
    assert_eq!(
        ui::review_cut_of(&window)
            .fx
            .iter()
            .filter(|e| e.kind == "speed")
            .count(),
        2,
        "a x2 next to a x3 stays two runs -- different rates are different decisions"
    );
    ui::press_undo(&window);
    settle();
    assert_eq!(
        ui::review_cut_of(&window).fx.len(),
        0,
        "one ↶ undoes the whole pass -- it went through record_edit, not seed_review_cut"
    );
    RAN_UNDO.store(true, Ordering::SeqCst);
    window.close();
    settle();

    // --- (6) two rounds then the failure line, and the run is NOT failed -------------------------
    let window = cut_page(app, &three_clips());
    // A clip number outside the batch faults the whole reply (§F3.10's red box), twice, then gives up.
    let said = ui::run_speeds_pass_with_reply(&window, &[Call { clip: 99, rate: 2.0 }]);
    assert_eq!(
        said,
        cut_speed_pass::no_answer(),
        "§F3.10's own give-up line, spelled by the module, got {said}"
    );
    assert_eq!(
        said,
        "!!! speed: no usable answer \u{2014} every clip plays at 1",
        "EM dash and all, matching the spec's bytes"
    );
    assert_eq!(
        ui::review_cut_of(&window).fx.len(),
        0,
        "an unusable reply changes nothing -- a cut at 1x is a cut, not a failed run"
    );
    let log = ui::window_logs();
    assert!(
        log.iter()
            .any(|line| line.starts_with(">>> speed rejected:") && line.contains("clip 99")),
        "the fault was said back before the retry, got {log:?}"
    );
    assert!(
        log.iter().any(|line| line == &cut_speed_pass::no_answer()),
        "and the give-up line reached the log, got {log:?}"
    );
    // The budget is exactly two: one rejection owes one retry, a third ask never happens.
    assert!(cut_captions_retries_once(), "two rounds at most, pinned at the seam level too");
    RAN_NO_ANSWER.store(true, Ordering::SeqCst);
    window.close();
    settle();
}

/// The retry budget the seam shares with F3.9: yes after one rejection, no after two.
fn cut_captions_retries_once() -> bool {
    naivepost::cut_captions::retries(1) && !naivepost::cut_captions::retries(2)
}

#[test]
fn f3_10_widget_the_speeds_button_reaches_the_pass() {
    // Pin cwd to our own temp root BEFORE anything builds: the press resolves the project through
    // `startup::session_dir(current_dir())`, and leaving it at rust/ writes a stray
    // `rust/session.naivepost/` into the repo.
    let root = std::env::temp_dir().join(format!("np-speeds-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("temp root created");
    std::env::set_current_dir(&root).expect("cwd pinned to the temp root");

    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(speeds_round);
        app.run();
    });

    assert!(RAN_CLICK.load(Ordering::SeqCst), "the real click never ran");
    assert!(
        RAN_PLACED_REFUSED_IGNORED.load(Ordering::SeqCst),
        "placed/refused/ignored never ran"
    );
    assert!(RAN_MERGED.load(Ordering::SeqCst), "the gap merge never ran");
    assert!(RAN_GATE.load(Ordering::SeqCst), "the gate never ran");
    assert!(RAN_UNDO.load(Ordering::SeqCst), "the undo of the pass never ran");
    assert!(RAN_NO_ANSWER.load(Ordering::SeqCst), "the two-round give-up never ran");
}
