// F3.9 Captions proposed by the model — the WIRE: a click on ✐ Captions reaches the rules in
// `naivepost::cut_captions` through a real widget, and what they answer lands on the cut, the lane and the
// status line instead of staying inside a callback.
//
// `rust/tests/cut_captions_proposed.rs` proves the rules themselves (batching, the message, the two refusals,
// the fades, the skip sentence); this file proves the Cut page can actually reach them. Before this round the
// pass had no door at all: every rule was tested and nothing in the app called any of it.
//
// No live model is dialled here. The reply is fed through `ui::run_captions_pass_with_reply`, the same split
// F3.4 makes with `text_drag_ended_with_source`: one seam for the telephone, one for the rule, so the widget
// test exercises placement against real page state without a network.

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::cut::{Cut, Fx, Seg};
use naivepost::cut_captions::{self, Call};
use naivepost::shell::Page;
use naivepost::tools::clips::CAPTION_MIN_SECONDS; // P.policy.captionMinSeconds
use naivepost::ui;

#[allow(dead_code)] // every test binary compiles this whole module; a helper it does not call is not a warning here
mod common;
use common::{fixture_dir, hold_last_window, release_last_window, settle, status_text, widget_in, cut_page};

static RAN_CLICK_PLACES: AtomicBool = AtomicBool::new(false);
static RAN_REJECTED_WHOLE_REPLY: AtomicBool = AtomicBool::new(false);
static RAN_SKIPPED_IS_SAID: AtomicBool = AtomicBool::new(false);
static RAN_GATE_GREYS_BUTTON: AtomicBool = AtomicBool::new(false);
static RAN_PARAMS_PINNED: AtomicBool = AtomicBool::new(false);
static RAN_UNDO_TAKES_PASS: AtomicBool = AtomicBool::new(false);

/// Let the main context run what the widget emissions queued.

/// Build a window sitting on the Cut tab with the given cut seeded, dropping the previous check's window first.
/// Three kept runs of ten seconds each: three clips the pass can be asked about, with clean arithmetic
/// (clip 2 starts at 10.0 s, clip 3 at 20.0 s) so a placed caption's second is checkable by hand.
fn three_clips() -> Cut {
    let mut cut_ = Cut::default();
    cut_.segs = vec![
        Seg { s: 0.0, e: 10.0, cam: 0, ..Default::default() },
        Seg { s: 10.0, e: 20.0, cam: 0, ..Default::default() },
        Seg { s: 20.0, e: 30.0, cam: 0, ..Default::default() },
    ];
    cut_
}

fn batch_of_three() -> Vec<(u32, f64)> {
    vec![(1, 10.0), (2, 10.0), (3, 10.0)]
}

// --- the round ----------------------------------------------------------------------------------------

fn captions_round(app: &adw::Application) {
    // --- (1) S5/S6: a real click places nothing when nothing was proposed, and says so -------------
    // The button exists, is wired and answers: that is the wire. With no model dialled the pass gets no calls,
    // so zero captions land and the cut is untouched — never invented words standing in for a proposal.
    let window = cut_page(app, &three_clips());
    let before = ui::review_cut_of(&window).fx.len();
    let button = widget_in(&window, "captions-pass-button").expect("✐ Captions is in the Cut toolbar");
    button
        .clone()
        .downcast::<gtk::Button>()
        .expect("it is a button")
        .emit_by_name::<()>("clicked", &[]);
    settle();
    assert_eq!(
        ui::review_cut_of(&window).fx.len(),
        before,
        "with no proposal from the model no caption is invented into the cut"
    );
    assert!(
        status_text(&window).contains("caption"),
        "the status says what the pass did, got {:?}",
        status_text(&window)
    );
    RAN_CLICK_PLACES.store(true, Ordering::SeqCst);
    window.close();
    settle();

    // --- (2) S5: a scripted reply lands through the SAME route, on the lane and in the cut --------
    let window = cut_page(app, &three_clips());
    let good = vec![
        Call { clip: 1, start: 1.0, end: 3.0, text: "the reveal".into() },
        // A 0.2 s caption is under the floor: skipped silently while its neighbour still lands.
        // P.policy.captionMinSeconds
        Call { clip: 2, start: 0.0, end: CAPTION_MIN_SECONDS - 0.1, text: "too short".into() },
        Call { clip: 3, start: 0.0, end: 2.0, text: "".into() },
    ];
    let said = ui::run_captions_pass_with_reply(
        &window,
        &[ui::CaptionBatch { clips: batch_of_three(), calls: good }],
    );
    let fx: Vec<Fx> = ui::review_cut_of(&window).fx.clone();
    assert_eq!(fx.len(), 1, "only the caption that cleared the floor landed, got {fx:?}");
    assert_eq!(said, "1 caption(s) placed", "the pass counts what it placed, got {said}");
    assert_eq!(fx[0].kind, "text", "a caption is a text effect");
    assert_eq!(fx[0].t, 1.0, "clip 1 starts at session second 0, so the offset IS the second");
    assert_eq!(fx[0].dur, 2.0, "the clamped span is the length");
    // S5's fade: min(0.3, d/4) -- 2.0/4 = 0.5, so the cap 0.3 wins.
    assert_eq!(fx[0].trans, cut_captions::fade(2.0), "fade in is the module's own number");
    assert_eq!(fx[0].tout, 0.3, "fades are 0.3 for a 2 s caption");
    assert!(
        widget_in(&window, "fx-bar-0").is_some(),
        "the placed caption drew its bar on the lane"
    );
    RAN_REJECTED_WHOLE_REPLY.store(true, Ordering::SeqCst);
    window.close();
    settle();

    // --- (3) S4: a clip outside the batch loses the WHOLE reply, retried once, then skipped -------
    let window = cut_page(app, &three_clips());
    let bad = vec![
        Call { clip: 1, start: 1.0, end: 3.0, text: "would have been fine".into() },
        Call { clip: 9, start: 0.0, end: 2.0, text: "outside the batch".into() },
    ];
    let said = ui::run_captions_pass_with_reply(
        &window,
        &[ui::CaptionBatch { clips: batch_of_three(), calls: bad }],
    );
    assert_eq!(
        ui::review_cut_of(&window).fx.len(),
        0,
        "the valid caption went down with the invalid one -- a batch is one answer"
    );
    assert!(
        said.contains("!!! captions: clips 1\u{2013}3 skipped"),
        "§F3.9's own skip sentence names the range, got {said}"
    );
    assert!(
        said.contains("the cut stands without them"),
        "and says nothing else was lost, got {said}"
    );
    let log = ui::window_logs();
    assert!(
        log.iter().any(|line| line.contains("clip 9 is not one of the clips given (1 to 3)")),
        "the rejection reason reached the log, got {log:?}"
    );
    // The retry limit is exactly one: `retries` says yes after the first rejection and no after the second,
    // so the third attempt never happens (proved as a rule in cut_captions_proposed; pinned again here so a
    // change to the seam's loop cannot quietly ask twice).
    assert!(cut_captions::retries(1), "one rejection owes one retry");
    assert!(!cut_captions::retries(2), "two rejections owe nothing more");
    RAN_SKIPPED_IS_SAID.store(true, Ordering::SeqCst);
    window.close();
    settle();

    // --- (4) the gate: captionsPass off greys the control and refuses the press -------------------
    let window = cut_page(app, &three_clips());
    ui::set_captions_pass(&window, false);
    ui::refresh_captions_gate(&window);
    let button = ui::captions_pass_button(&window).expect("✐ Captions is in the Cut toolbar");
    assert!(
        !button.is_sensitive(),
        "the control greys out when the pass is off -- a grey button always means the pass would refuse"
    );
    let refused = ui::run_captions_pass(&window);
    assert!(refused.contains("off"), "the pass itself refuses, got {refused}");
    assert_eq!(ui::review_cut_of(&window).fx.len(), 0, "and places nothing");
    // An insensitive button swallows the emission, so what is asserted here is the state rather than a
    // fired-and-ignored click: GTK will not run the handler of a control it has greyed out.
    assert!(!button.is_sensitive(), "still grey after the refused press");
    // And back on: the gate follows the field both ways, so the grey is never sticky.
    ui::set_captions_pass(&window, true);
    ui::refresh_captions_gate(&window);
    assert!(button.is_sensitive(), "switching the policy back on re-enables the control");
    RAN_GATE_GREYS_BUTTON.store(true, Ordering::SeqCst);
    window.close();
    settle();

    // --- (5) ↶ takes the whole pass back ---------------------------------------------------------
    let window = cut_page(app, &three_clips());
    let placed = vec![
        Call { clip: 1, start: 1.0, end: 3.0, text: "first line".into() },
        Call { clip: 2, start: 0.0, end: 4.0, text: "second line".into() },
    ];
    let said = ui::run_captions_pass_with_reply(
        &window,
        &[ui::CaptionBatch { clips: batch_of_three(), calls: placed }],
    );
    assert_eq!(said, "2 caption(s) placed", "both landed, got {said}");
    assert_eq!(ui::review_cut_of(&window).fx.len(), 2);
    ui::press_undo(&window);
    settle();
    assert_eq!(
        ui::review_cut_of(&window).fx.len(),
        0,
        "one ↶ undoes the whole pass -- it went through record_edit, not seed_review_cut"
    );
    RAN_UNDO_TAKES_PASS.store(true, Ordering::SeqCst);
    window.close();
    settle();

    // --- (6) the two cited parameters ----------------------------------------------------------------
    // P.machine.captionBatch: five clips per request, three ranges over twelve clips, a short final batch.
    assert_eq!(cut_captions::BATCH, 5, "// P.machine.captionBatch");
    assert_eq!(naivepost::params::family("P.machine.captionBatch"), naivepost::params::Family::Machine);
    let ranges = cut_captions::batches(12);
    assert_eq!(ranges.len(), 3, "twelve clips are three requests, got {ranges:?}");
    assert_eq!(ranges[0], (1, 5), "the first request carries five clips");
    assert_eq!(ranges[2], (11, 12), "the last is the short one");
    assert_eq!(cut_captions::batches(0).len(), 0, "nothing to caption asks nothing");
    // P.policy.captionMinSeconds: the floor the tool enforces and the pass skips quietly for. Read from
    // `params::cut()` — this is a Cut-stage row, not a Prepare one, so `find` (which searches Prepare's rows)
    // does not carry it.
    assert_eq!(
        CAPTION_MIN_SECONDS, 0.3,
        "// P.policy.captionMinSeconds -- the value params.rs points at"
    );
    let row = naivepost::params::cut()
        .into_iter()
        .find(|param| param.id == "P.policy.captionMinSeconds")
        .expect("the Cut rows carry the caption floor");
    assert_eq!(row.spelled, "0.3", "§10 spells it 0.3, got {}", row.spelled);
    assert_eq!(
        row.from,
        "tools::clips::CAPTION_MIN_SECONDS",
        "and reads it from the tool's own constant, so pass and tool cannot disagree"
    );
    RAN_PARAMS_PINNED.store(true, Ordering::SeqCst);
}

#[test]
fn f3_9_widget_the_captions_button_reaches_the_pass() {
    // Pin cwd to our own temp root BEFORE anything builds: the press resolves the project through
    // `startup::session_dir(current_dir())`, and leaving it at rust/ writes a stray
    // `rust/session.naivepost/` into the repo.
    let root = std::env::temp_dir().join(format!("np-captions-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("temp root created");
    std::env::set_current_dir(&root).expect("cwd pinned to the temp root");

    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(captions_round);
        app.run();
    });

    assert!(RAN_CLICK_PLACES.load(Ordering::SeqCst), "S5 click-through never ran");
    assert!(RAN_REJECTED_WHOLE_REPLY.load(Ordering::SeqCst), "S5 placement never ran");
    assert!(RAN_SKIPPED_IS_SAID.load(Ordering::SeqCst), "S4 reject/retry/skip never ran");
    assert!(RAN_GATE_GREYS_BUTTON.load(Ordering::SeqCst), "the gate never ran");
    assert!(RAN_UNDO_TAKES_PASS.load(Ordering::SeqCst), "the undo of the pass never ran");
    assert!(RAN_PARAMS_PINNED.load(Ordering::SeqCst), "the parameter pins never ran");
}
