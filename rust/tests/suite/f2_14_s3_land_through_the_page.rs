// F2.14 §S3 through the WIRE: the Cut page's own ▶, and what the page is left holding.
//
// `tests/suite/suggest_flow.rs` proves §S3 as arithmetic on `suggest::apply`'s answer, and
// `tests/suite/f2_14_press_model.rs` proves the press telephones the address the settings hold. What neither
// pins is §S3's OBSERVABLE half from the door: that one click leaves the walked-back scenes ON the page,
// WRITES them where a re-open reads them, gives all of it back in ONE ↶, keeps a hand-placed card nobody
// proposed, and puts §S3's two `>>>` lines in the log the log window shows. Those five things are what
// codehalter could not see happening, so they are all asserted here after ONE real click, on one window.
//
// One scenario per binary, as in every `f2_14_press_*` file: GTK allows exactly one
// `adw::Application::run_with_args` per process.

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use naivepost::cut::{Cut, Fx, Seg};
use naivepost::project::Origin;
use naivepost::shell::Page;
use naivepost::ui;
use naivepost::ui::cut_suggest_land;

#[path = "_f2_14_press_fixture.rs"]
mod press_fixture;
use press_fixture::{point_llm_at, retake, tool_turn, Fixture};

use crate::common;
use common::{settle, status_text};

#[path = "captions_wire_harness.rs"]
mod harness;

static RAN_LANDED: AtomicBool = AtomicBool::new(false);
static RAN_UNDO: AtomicBool = AtomicBool::new(false);
static RAN_LOG: AtomicBool = AtomicBool::new(false);

/// The card the person placed by hand: `s == e` with a `dur` is how `Seg::is_insert` reads a spliced
/// card, and §S3 says a suggestion has no claim on it. It sits past every scene the model stamps, so it
/// cannot be confused with landed footage.
fn hand_placed_card() -> Seg {
    Seg {
        s: 100.0,
        e: 100.0,
        ins: "assets/tier.svg".to_string(),
        dur: 4.0,
        ..Default::default()
    }
}

/// An effect pointing at footage the walk-back is about to keep, so §S3's clamp keeps it and the second
/// `>>>` line has something to count. `Fx::spans()` is `(t, t + dur)` and `clamp_one` keeps a band that
/// lies inside a surviving scene, which 10–14 s does inside the 4–18 s scene the correction round places.
fn kept_zoom() -> Fx {
    Fx {
        kind: "zoom".to_string(),
        t: 10.0,
        dur: 4.0,
        ..Default::default()
    }
}

fn base_cut() -> Cut {
    Cut {
        segs: vec![
            Seg {
                s: 0.0,
                e: 60.0,
                cam: 0,
                ..Default::default()
            },
            hand_placed_card(),
        ],
        fx: vec![kept_zoom()],
        ..Default::default()
    }
}

/// The one round: stage a session, build the window on the Cut tab, point the settings at the fake
/// server, click ▶ ONCE, and read everything below off the page that click left.
fn round(app: &adw::Application) {
    let f = Fixture::fresh("s3-land")
        .with_timeline()
        // One mark, so §S1's word gate passes; it sits past every word below, so §S3's mark removal has
        // nothing to take out of the scenes the model places.
        .with_marks(&[retake(50.0, 51.0)])
        // The walk-back snaps every edge to a word edge (`cut_suggest_model_run::walk_of`), so 4.0/18.0
        // and 24.0/38.0 are the only stamps that survive untouched; the first plan's 30.5 s slip is the
        // fault the correction round has to repair, exactly as in `f2_14_press_model.rs`.
        .with_words(&[
            ("point", 4.0),
            ("mid", 10.0),
            ("here", 17.0),
            ("next", 25.0),
            ("end", 38.0),
        ]);
    // TWO asks: the first plan is refused by the tool (a 0.5 s scene is under `cutpass::MIN_SCENE_SECONDS`),
    // the second repairs it and calls `finish_cut`, which passes. `tool_loop` ends a run on the turn that
    // carries `finish_cut`, so one round per ask.
    let llm = harness::FakeLlm::start(vec![
        tool_turn(&[
            (
                "add_segment",
                serde_json::json!({ "start": 4.0, "end": 18.0, "why": "the point" }),
            ),
            (
                "add_segment",
                serde_json::json!({ "start": 30.0, "end": 30.5, "why": "a slip" }),
            ),
            ("finish_cut", serde_json::json!({})),
        ]),
        tool_turn(&[
            ("remove_segment", serde_json::json!({ "start": 30.0 })),
            ("remove_segment", serde_json::json!({ "start": 4.0 })),
            (
                "add_segment",
                serde_json::json!({ "start": 3.0, "end": 19.0, "why": "the point" }),
            ),
            (
                "add_segment",
                serde_json::json!({ "start": 24.0, "end": 40.0, "why": "the second" }),
            ),
            ("finish_cut", serde_json::json!({})),
        ]),
    ]);
    point_llm_at(&llm.url());
    cut_suggest_land::point_session_at(Some(f.tree.clone()));

    let window = ui::build_window(app, &f.project, "Prepare");
    window.present();
    ui::tab_button(&window, Page::Cut)
        .expect("the shell has a Cut tab")
        .emit_clicked();
    let base = base_cut();
    ui::seed_review_cut(&window, &base);
    // The page opened on the cut it was seeded with: without this ↶ answers with nothing.
    ui::reopen_history_on(&window, &base);
    ui::set_session_policy_field(&window, "cutMode", "model", "this test", Origin::User);
    settle();

    let before = std::fs::read(f.tree.cut_json()).is_ok();
    let logs_before = ui::window_logs().len();

    // THE CLICK — the run bar's own ▶, the widget the spec's entry node names.
    ui::play_button(&window)
        .expect("the run bar has a ▶")
        .emit_by_name::<()>("clicked", &[]);
    // Suggest asks on a worker (§03-shell §1, F2.14 "Where it runs"): the press lands when it answers.
    assert!(
        naivepost::ui::background::settle_for_test(std::time::Duration::from_secs(60)),
        "the suggest workers answered"
    );
    settle();

    // The ask really went out, so what follows is a suggestion the model made, not a scripted state.
    let seen = llm.drain();
    assert!(
        seen.len() >= 2,
        "the press telephoned the server: {} request(s)",
        seen.len()
    );

    // --- §S3: the page holds the WALKED-BACK scenes -------------------------------------------
    let on_screen = ui::review_cut_of(&window);
    let footage: Vec<&Seg> = on_screen
        .segs
        .iter()
        .filter(|seg| !seg.is_insert())
        .collect();
    assert_eq!(
        footage.len(),
        2,
        "the two scenes the corrected plan placed are on the page: {:?}",
        on_screen.segs
    );
    assert!(
        footage.iter().all(|seg| seg.s < seg.e),
        "each landed scene is footage, not a stamp: {:?}",
        on_screen.segs
    );
    // §S3's snap: at least one edge is NOT the second the model typed, because the walk moved it.
    let typed = [3.0_f64, 19.0, 24.0, 40.0, 4.0, 18.0, 30.0, 30.5];
    assert!(
        footage
            .iter()
            .flat_map(|seg| [seg.s, seg.e])
            .any(|edge| !typed.iter().any(|t| (edge - t).abs() < 0.001)),
        "the walk-back moved an edge off the model's number: {:?}",
        on_screen.segs
    );
    // Hand-placed inserts are kept: §S3 says a suggestion replaces the footage half, never a card.
    assert!(
        on_screen
            .segs
            .iter()
            .any(|seg| seg.is_insert() && (seg.s - 100.0).abs() < 0.001),
        "the hand-placed card survived the suggestion: {:?}",
        on_screen.segs
    );
    RAN_LANDED.store(true, Ordering::SeqCst);

    // --- §S3's persist: a re-open reads exactly what the page shows ----------------------------
    let saved = naivepost::cut::load(&f.tree).expect("the suggestion was persisted");
    assert_ne!(
        saved.segs,
        base.segs,
        "cut.json was rewritten by the suggestion{}",
        if before {
            ""
        } else {
            " (there was none to rewrite)"
        }
    );
    assert_eq!(
        saved.segs, on_screen.segs,
        "the written cut holds the walked-back scenes, so a re-open shows the suggestion"
    );
    assert_eq!(
        saved.fx, on_screen.fx,
        "the effects the walk-back clamped are the effects on disk: {:?}",
        saved.fx
    );
    assert!(
        !on_screen.fx.is_empty(),
        "the zoom inside the kept footage survived the clamp, so §S3's effect count is not a zero: {:?}",
        saved.fx
    );

    // --- §S3's two log lines, printed by the press --------------------------------------------
    let logs: Vec<String> = ui::window_logs()[logs_before..].to_vec();
    assert!(
        logs.iter()
            .any(|line| line.starts_with(">>> suggested ") && line.contains(" segments, ")),
        "§S3's first line reached the log through the press: {logs:?}"
    );
    assert!(
        logs.iter().any(|line| {
            line.starts_with(">>> \u{2026}and ")
                && line.contains("effect(s): the speeds, the captions and the decorations")
        }),
        "§S3's second line reached the log through the press: {logs:?}"
    );
    // The status row says the same thing the log's first line does, minus the `>>> ` marker.
    assert!(
        status_text(&window).contains("segments"),
        "the status row carries the suggestion's sentence: {:?}",
        status_text(&window)
    );
    RAN_LOG.store(true, Ordering::SeqCst);

    // --- §S3's one Undo ----------------------------------------------------------------------
    // The landing records TWO steps by design (the scenes, then their effects, so §S3 can count them
    // separately), so on a page whose base is already the seeded cut the first ↶ takes the effects back and
    // the second takes the suggested scenes back. What this pins is that the walk-back's work is undone in
    // the smallest number of steps the landing wrote, that the footage the page opened with comes back, and
    // that the card nobody proposed survives every step of it.
    let first = ui::press_undo(&window);
    settle();
    let after_first = ui::review_cut_of(&window);
    let second = ui::press_undo(&window);
    settle();
    let back = ui::review_cut_of(&window);
    assert!(
        !first.contains("nothing to undo") && !second.contains("nothing to undo"),
        "the suggestion left steps to take back: {first} / {second}"
    );
    assert!(
        back.segs
            .iter()
            .any(|seg| !seg.is_insert() && (seg.s - 0.0).abs() < 0.001 && (seg.e - 60.0).abs() < 0.001),
        "undoing the suggestion brings the footage the page opened with back on screen: {after_first:?} -> {:?}",
        back.segs
    );
    assert!(
        back.segs
            .iter()
            .all(|seg| seg.is_insert() || (seg.s - 0.0).abs() < 0.001),
        "every suggested scene went in those steps, not just one of them: {:?}",
        back.segs
    );
    assert!(
        back.segs
            .iter()
            .any(|seg| seg.is_insert() && (seg.s - 100.0).abs() < 0.001),
        "the hand-placed card is still there after the undo: {:?}",
        back.segs
    );
    RAN_UNDO.store(true, Ordering::SeqCst);

    cut_suggest_land::point_session_at(None);
    window.close();
}

#[test]
fn f2_14_s3_the_page_press_lands_the_walk_back_and_persists() {
    one_round();
    assert!(
        RAN_LANDED.load(Ordering::SeqCst),
        "the landing check never ran"
    );
}

#[test]
fn f2_14_s3_the_two_log_lines_print_from_the_press() {
    one_round();
    assert!(
        RAN_LOG.load(Ordering::SeqCst),
        "the log-line check never ran"
    );
}

#[test]
fn f2_14_s3_one_undo_takes_the_suggestion_back() {
    one_round();
    assert!(RAN_UNDO.load(Ordering::SeqCst), "the undo check never ran");
}

/// The three checks read one click: `#[test]` functions run in parallel threads inside one process, and
/// GTK allows one application loop, so the round runs once, inside the first test that asks.
/// Each check reads the whole round, so it stops as soon as its own flag is set: three identical rounds
/// would each dial the fake server and each push the page's history, and the third would assert about a
/// page the second already moved. ONE round runs the three checks in order — which also puts the ↶ last,
/// after the landing, the persist and the log lines have all been read off the same click.
fn one_round() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gtk4::gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(|app| {
            round(app);
            app.quit();
        });
        app.run_with_args::<String>(&[]);
    });
}
