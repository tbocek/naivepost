// §05-cut#7-rules bullet 1 through the WIDGETS. `tests/suite/cut_rules.rs` proves the rules over plain data;
// what lives here is what only the page can get wrong: the three holds (an edge, a clip, an effect) sharing ONE
// slot so a second pickup replaces the first, and one trim drag spending exactly one Undo step however many
// updates it fires.
//
// ONE application, one `connect_activate`, exactly ONE `#[test]`: both checks build windows and the app's run
// loop can only be held by one thread at a time (`g_application_run` refuses a second acquisition), so the two
// checks run in one round and report through flags the test asserts on — the shape
// `tests/suite/cut_trim_move_widgets.rs` uses.

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use naivepost::cut;
use naivepost::cut_line::Held;
use naivepost::cut_trim as trim;
use naivepost::shell::Page;
use naivepost::ui;

use crate::common;
use common::{clip, fixture_dir, settle};

static RAN_ONE_HOLD: AtomicBool = AtomicBool::new(false);
static RAN_TRIM_UNDO: AtomicBool = AtomicBool::new(false);
static RAN_DRAWN_SCOPE: AtomicBool = AtomicBool::new(false);

fn window_round(run: impl Fn(&adw::Application) + Send + 'static) {
    let thread = std::thread::spawn(move || {
        let app = adw::Application::builder()
            .application_id("org.naivepost.test.sec05cut7hand")
            .build();
        app.connect_activate(run);
        app.run_with_args::<String>(&[]);
    });
    let _ = thread.join();
}
/// A Cut-page window on the fixture with a three-scene cut seeded AND the history opened on that cut, so the
/// first edit the check makes is the first step the history holds. (`build_window` opens the history on the cut
/// the page read from disk, which is the fixture's, not this check's seed — the shape every widget test that
/// counts undo steps uses, `ui::reopen_history_on`.)
fn cut_page(app: &adw::Application) -> adw::ApplicationWindow {
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, "Prepare");
    window.present();
    ui::tab_button(&window, Page::Cut)
        .expect("the shell has a Cut tab")
        .emit_clicked();
    settle();
    let seeded = cut::Cut {
        segs: vec![clip(0.0, 9.5), clip(10.0, 30.0), clip(40.0, 70.0)],
        ..Default::default()
    };
    ui::seed_review_cut(&window, &seeded);
    ui::reopen_history_on(&window, &seeded);
    window
}

/// §7 bullet 1 (`One thing held at a time`): the page's three holds are ONE slot.
fn check_picking_up_replaces_what_was_held(app: &adw::Application) {
    let window = cut_page(app);

    // An effect first, then a clip EDGE, then the whole clip: each pickup replaces the one before. Before this
    // rule had a caller the three lived in three thread_locals, so the effect stayed held under the clip and
    // `held_now()`'s ladder picked a winner the user never chose.
    ui::set_held_effect(Some(cut::Fx {
        kind: "zoom".into(),
        t: 4.0,
        dur: 6.0,
        ..Default::default()
    }));
    assert_eq!(ui::held_now(), Some(Held::Effect), "the effect is in hand");
    assert!(ui::held_clip().is_none(), "nothing else came with it");

    ui::set_held_edge(Some(clip(10.0, 30.0)));
    assert_eq!(ui::held_now(), Some(Held::Edge), "the edge took the hand from the effect");
    assert!(ui::held_effect().is_none(), "and the effect is NOT still in hand");

    ui::set_held_clip(Some(clip(10.0, 30.0)));
    assert_eq!(ui::held_now(), Some(Held::Clip), "the whole clip took the hand from the edge");
    assert!(ui::held_edge().is_none(), "no edge survives the pickup");
    assert!(ui::held_effect().is_none(), "nor does the effect");
    // The clip reader answers for an edge pick too — both name the segment to move — but the KIND says Clip.
    assert_eq!(ui::held_clip().map(|seg| (seg.s, seg.e)), Some((10.0, 30.0)));

    // Clearing empties the one slot: no half-held remainder anywhere.
    ui::set_held_clip(None);
    assert_eq!(ui::held_now(), None, "the hand is empty");
    assert!(ui::held_clip().is_none() && ui::held_edge().is_none() && ui::held_effect().is_none());
    drop(window);
}

/// §7 bullet 1 (`one Undo per drag`): a border drag of three moves, ONE step to take back.
fn check_one_undo_per_trim_drag(app: &adw::Application) {
    let window = cut_page(app);
    let trimmer = ui::trim_gesture(&window).expect("the strip carries the trim gesture");
    assert!(
        trimmer.upcast_ref::<gtk4::EventController>().widget().is_some(),
        "the trim gesture is attached to the strip, not floating unclaimed"
    );
    // The undo button is greyed until an edit exists; the one-step check reads the history through ↶ itself
    // rather than through that button, whose sensitivity is a redraw and therefore lags the write.
    // The history is open on the seeded cut (see `cut_page`), so nothing behind this drag can be mistaken for
    // the drag's own step.
    assert!(
        !ui::line_step_button(&window, "undo-button")
            .expect("the toolbar drew undo-button")
            .is_sensitive(),
        "the page sits on its opened state: nothing to undo"
    );

    let boxes = trim::clip_boxes(&ui::review_cut_segs(&window), ui::window::strip_pps());
    let start_px = boxes[1].x;
    assert_eq!(
        trim::border_at(&boxes, start_px + 2.0),
        Some((1, trim::Border::Start)),
        "the press has to fall on clip 2's start border or this proves nothing"
    );
    trimmer.emit_by_name::<()>("drag-begin", &[&start_px, &0.0f64]);
    settle();
    trimmer.emit_by_name::<()>("drag-begin", &[&start_px, &0.0f64]);
    settle();
    // The hold's moves. Only the FIRST of them may spend a step (§7), and the door that decides is the one the
    // drag's own ticks go through — so the drag travels to a second the first tick reaches, and a later tick at
    // a further second proves no second step was taken. Which ticks the scrub throttle (§6's `preview.scrubMs`,
    // 90 ms) lets through is a clock fact, not the rule, so every tick waits out that gate before it is fired.
    let mut edges = Vec::new();
    for target in [11.0, 13.0, 15.0] {
        std::thread::sleep(std::time::Duration::from_millis(trim::SCRUB_MS + 40));
        trimmer.emit_by_name::<()>("drag-update", &[&(target * ui::window::strip_pps()), &0.0f64]);
        settle();
        edges.push(ui::review_cut_segs(&window)[1].s);
    }
    // The release repeats the last second, so it changes nothing and costs nothing of its own.
    trimmer.emit_by_name::<()>("drag-end", &[&(15.0 * ui::window::strip_pps()), &0.0f64]);
    settle();
    let dragged = ui::review_cut_segs(&window)[1].s;
    assert!(
        dragged > 10.0,
        "the drag moved the border at all: pressed 10.0, released at {dragged}, ticks {edges:?}"
    );
    // One step, and only one: the drag's FIRST move saved the state the hand pressed in (10.0), so a single ↶
    // lands the border back there. That is what distinguishes one step from three — three would stop at the
    // second-to-last tick — and the ↶ behind it refusing says there is nothing else behind it.
    let said = ui::press_undo(&window);
    settle();
    let back = ui::review_cut_segs(&window)[1].s;
    assert!(
        !said.contains("nothing to undo"),
        "and the drag left something to take back: ↶ said {said}"
    );
    assert!(
        (back - 10.0).abs() < 1e-9,
        "ONE step took the whole drag back to where the hand pressed (the first move's snapshot); a step per \
         update would have stopped at the tick before the last: back at {back}, ticks {edges:?}, released {dragged}"
    );
    assert!(
        ui::press_undo(&window).contains("nothing to undo"),
        "and the history holds no step behind the drag's one"
    );
    drop(window);
}

#[test]
fn sec_05_cut_7_rules_s1_one_hold_and_one_undo_through_the_page() {
    common::pin_private_cwd();
    window_round(|app| {
        check_picking_up_replaces_what_was_held(app);
        RAN_ONE_HOLD.store(true, Ordering::SeqCst);
        check_one_undo_per_trim_drag(app);
        RAN_TRIM_UNDO.store(true, Ordering::SeqCst);
        check_a_press_resolves_to_the_drawn_lane(app);
        RAN_DRAWN_SCOPE.store(true, Ordering::SeqCst);
        app.quit();
    });
    assert!(
        RAN_ONE_HOLD.load(Ordering::SeqCst),
        "the one-hold check never ran"
    );
    assert!(
        RAN_TRIM_UNDO.load(Ordering::SeqCst),
        "the one-undo-per-trim check never ran"
    );
    assert!(
        RAN_DRAWN_SCOPE.load(Ordering::SeqCst),
        "the drawn-scope check never ran"
    );
}

/// §7 bullet 2 (`a selection is of what it was DRAWN on`) on the PAINTED band: a y that lies in the effects
/// lane answers the effects lane, and the lane's depth comes from the cut being drawn — so a cut with two
/// overlapping effects has a deeper lane, and a press inside that deeper part is a lane press rather than a
/// picture row. The placeholder constant would answer both of those y values as rows of its own fixed split.
fn check_a_press_resolves_to_the_drawn_lane(app: &adw::Application) {
    use naivepost::cut_select::Surface;
    use naivepost::ui::cut_line_press;

    let window = cut_page(app);
    // Two effects that cannot share a row: same kind, overlapping seconds -> the lane packs them on two rows.
    let seeded = cut::Cut {
        segs: vec![clip(0.0, 60.0)],
        fx: vec![
            cut::Fx { kind: "zoom".into(), t: 4.0, dur: 6.0, ..Default::default() },
            cut::Fx { kind: "zoom".into(), t: 8.0, dur: 6.0, ..Default::default() },
        ],
        ..Default::default()
    };
    ui::seed_review_cut(&window, &seeded);
    settle();

    let lanes = naivepost::fx_lane::row_count(&seeded.fx);
    assert!(lanes >= 2, "two overlapping effects need a lane {lanes} deep for this to mean anything");
    // The lane's own span, off the painter's table: below the ruler and the selection band.
    let bands = naivepost::ui::cut_tracks::bands_for(lanes);
    let shallow = bands.fx.0 + 2.0; // first lane row
    let deep = bands.fx.1 - 2.0; // last lane row, which only exists because the cut has two
    assert!(deep > shallow, "the lane must span more than one row: {:?}", bands.fx);
    assert_eq!(
        cut_line_press::surface_at_band(&window, shallow),
        Surface::FxLane,
        "the top of the drawn lane"
    );
    assert_eq!(
        cut_line_press::surface_at_band(&window, deep),
        Surface::FxLane,
        "and its bottom — a fixed one-row lane would call this a picture row"
    );
    let gesture = crate::suite::sec_05_cut_7_hand_widgets::ui::window::band_line_gesture_for_test(&window)
        .expect("the painted band carries its line-click gesture");
    let x_px = naivepost::ui::cut_view::gutter_px() + 15.0 * naivepost::ui::cut_view::pps();
    ui::set_held_clip(None);
    gesture.emit_by_name::<()>("released", &[&1i32, &x_px, &deep]);
    settle();
    assert!(ui::held_clip().is_none(), "a press on the drawn lane took no footage into hand");

    gesture.emit_by_name::<()>("released", &[&1i32, &x_px, &(bands.fx.1 + 2.0)]);
    settle();
    let held = ui::held_clip().expect("the same press one pixel lower took the scene under it");
    assert_eq!((held.s, held.e), (0.0, 60.0), "and it is the scene under the click");
    drop(window);
}
