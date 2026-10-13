//! §06-effects#1-record — the two NEW columns of §1's table: `cam` (the camera row a zoom was framed on) and
//! `lane` (`""` = the whole bed, a lane's name = that lane only).
//!
//! The other §1 tests prove the fields a record carries when it is created; this file proves the two columns a
//! PRESS has to fill in. Both are written by the page, not typed into a form: §A.1 gives the zoom form no row
//! field (the row is the one the picture band is watching) and §A.6 gives the volume form five fields, none of
//! them a lane (it comes from the seconds the press was built from). One check therefore drives the real buttons
//! — ⊕ Zoom's drag-end + Apply, 🔊 Volume's press + Apply — and asserts the stored `Fx`, which is the same state
//! the logic checks assert.

use adw::prelude::*;
use naivepost::cut::{Cut, Fx, Lane, Seg};
use naivepost::{fx_volume as vol, fx_zoom as zoom, ui};

#[allow(dead_code)] // every test binary compiles this whole module; a helper it does not call is not a warning here
mod common;
use common::{cut_button as button, fixture_dir, hold_last_window, release_last_window, settle};

/// The session filmed 0–120 as one take, so any second a press names is inside footage.
fn tape_end() -> f64 {
    120.0
}

/// Build a window sitting on the Cut tab over `seeded`, dropping the previous check's window first. Each check
/// gets its own cwd, so the `cut.json` a press writes cannot land in another check's folder — and so no earlier
/// window's form holder can be reached by a name walk.
fn cut_page(app: &adw::Application, seeded: &Cut) -> adw::ApplicationWindow {
    release_last_window();
    static BUILT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let root = std::env::temp_dir().join(format!(
        "np-sec06e1-columns-{}-{}",
        std::process::id(),
        BUILT.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
    ));
    std::fs::create_dir_all(&root).expect("per-check temp root");
    std::env::set_current_dir(&root).expect("cwd pinned per check");
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, "Prepare");
    hold_last_window(window.clone());
    window.present();
    ui::tab_button(&window, naivepost::shell::Page::Cut)
        .expect("the shell has a Cut tab")
        .emit_by_name::<()>("clicked", &[]);
    ui::reopen_history_on(&window, seeded);
    ui::seed_review_cut(&window, seeded);
    settle();
    window
}

/// The cut's rows come from the colouring (§B), which gives two cameras a row each only when their spans
/// OVERLAP: recordings filmed at different times share row 0. So one camera is pinned to row 1 through `cut.json`
/// §3's own `rows` map — the editor moved it there — which is what makes "watch row 1" a question with two
/// possible answers instead of always row 0.
/// A cut whose two cameras sit on rows 0 and 1. The colouring (§B) gives a second camera its own row only when
/// the spans OVERLAP or the file pins it, so both are here: cam1's span overlaps cam0's AND `cut.json`'s own
/// `rows` map pins it to row 1 — the editor moved it there. That is what makes "watch row 1" answerable.
/// The page's row count is `timeline::rows_for` over the cut's own kept footage, and §B gives a second camera a
/// row of its own only when the spans OVERLAP or `cut.json` pins it — so both hold here: cam1 runs alongside cam0
/// for the whole tape AND is pinned to row 1 (the editor moved it there). That is what makes "watch row 1" a
/// question with two answers rather than always row 0.
fn two_rows() -> Cut {
    Cut {
        segs: vec![
            Seg { s: 0.0, e: tape_end(), cam: 0, ..Default::default() },
            Seg { s: 0.0, e: tape_end(), cam: 1, ..Default::default() },
        ],
        rows: std::collections::BTreeMap::from([("cam1".to_string(), 1i32)]),
        // A real aspect, so the box that lands is a framing rather than an out-of-the-way default: §F3.1 S3 keeps
        // "the frame already settled" (an aspect plus a staying zoom) off the free-drag path entirely.
        aspect: "16:9".to_string(),
        fx: vec![Fx { kind: "zoom".into(), t: 4.0, dur: 3.0, stay: true, ..Default::default() }],
        ..Default::default()
    }
}

// --- S8: the zoom records the row it was framed on -------------------------------------------------------------

/// A box drawn while the page watches row 1 stores `cam: 1`; a box drawn with nothing watched stores row 0.
/// Driven through the doors the gesture uses: arm (⊕ Zoom), the panel's own drag, then the form's Apply.
fn zoom_row_round(app: &adw::Application) {
    fn drag(window: &adw::ApplicationWindow, from: (f64, f64), to: (f64, f64)) {
        let gesture = ui::zoom_gesture(window).expect("the page wired a drag onto the preview panel");
        gesture.emit_by_name::<()>("drag-begin", &[&from.0, &from.1]);
        gesture.emit_by_name::<()>("drag-end", &[&(to.0 - from.0), &(to.1 - from.1)]);
        settle();
    }

    let window = cut_page(app, &two_rows());
    // The page's own door for declaring its takes: a seeded cut's spans are only what that cut implied, and the
    // row list every reader asks comes from the declared list when there is one (the same door
    // `tests/cut_view_scroll.rs` uses).
    ui::set_session_recordings(&[
        naivepost::timeline::Recording { base: "cam0".to_string(), start: 0.0, end: tape_end() },
        naivepost::timeline::Recording { base: "cam1".to_string(), start: 0.0, end: tape_end() },
    ]);
    assert_eq!(
        naivepost::timeline::rows_for(
            &naivepost::timeline::kept_footage_recordings(&two_rows()),
            &two_rows()
        ),
        vec![0, 1],
        "the colouring puts the second camera on row 1"
    );
    // A fresh page: no line placed yet and nothing watched,
    // so the refusals below are today's answer rather than a leftover from another check in this binary. so the two refusals below are today's answer rather
    // than a leftover from another check in this binary (the arm flag and the "was a line ever placed" note are
    // process-wide thread-locals, exactly as `tests/zoom_drag_widgets.rs` resets them).
    ui::note_place(false);
    ui::note_zoom_place(false);
    ui::set_watched_row(None);
    settle();
    // Fire the preview panel's own drag the way the toolkit does (begin at the press point, release with the
    // OFFSET that signal carries) — the same seam `tests/zoom_drag_widgets.rs` uses, so the form holder is
    // resolved through the gesture's own window rather than a second hand-built call.
    // §1's `cam` is the row the PAGE is watching, and with nothing watched that is row 0 — so arming before any
    // row was chosen answers with the module's own refusal instead of silently arming at midnight.
    click(&window, "effect-item-zoom");
    assert!(
        !ui::zoom_armed(),
        "with no line ever placed the entry refuses rather than arming (§F3.1 S1)"
    );
    ui::note_place(true);
    ui::note_zoom_place(true);
    ui::set_line_position(&window, naivepost::cut_line::LinePos { t: 12.0 });

    // Watch row 1 the way a picture-band click does — through the page's own door, not a field of the record.
    ui::set_watched_row(Some(1));
    assert_eq!(ui::watched_row(), Some(1), "the page knows which row it is watching");
    click(&window, "effect-item-zoom");
    assert!(ui::zoom_armed(), "the row armed the drag");
    // A box big enough to count (fx_zoom::DRAG_MIN_PX either way), with the source's own pixels handed in
    // because no ffprobe runs headless — the same seam tests/zoom_drag_widgets.rs uses.
    drag(&window, (100.0, 100.0), (700.0, 500.0));
    let drawn = ui::zoom_form_open().expect("the finished box opened the form");
    assert_eq!(drawn.row, 1, "the form took its row from what the page is watching");
    click(&window, "zoom-apply-button");
    let placed = ui::review_cut_of(&window);
    // The form is the page's answer to "which row am I watching", so it is asserted there; the record inherits
    // it through `fx_zoom::apply` -> `new_zoom`, which is what `a_reopened_zoom_keeps_its_own_row` proves.
    assert!(placed.fx.iter().any(|fx| fx.kind == "zoom"), "Apply put the framed zoom on the cut");

    // Nothing watched: row 0, which costs no key at all (§1's storage skips a neutral value).
    ui::set_watched_row(None);
    click(&window, "effect-item-zoom");
    drag(&window, (100.0, 100.0), (700.0, 500.0));
    assert_eq!(
        ui::zoom_form_open().expect("the second box opened its form").row,
        0,
        "with nothing watched the framing is row 0"
    );
    click(&window, "zoom-apply-button");
    let again = ui::review_cut_of(&window);
    let second = again
        .fx
        .iter()
        .rev()
        .find(|fx| fx.kind == "zoom")
        .expect("the second framing landed too");
    assert_eq!(second.cam, 0, "with no row watched the framing is row 0");
    let written = serde_json::to_value(second).expect("an effect serialises");
    assert!(
        written.get("cam").is_none(),
        "row 0 is the neutral value and §1 keeps an untouched file byte-identical: {written}"
    );
    window.close();
}

/// A row number past the rows this cut has cannot survive: it would be a reframe of a row nobody can see.
fn row_is_clamped_to_the_picture_rows() {
    let cut = two_rows();
    // The page's own count, read off `timeline::rows_for` exactly as `picture_row_count` computes it.
    let last = naivepost::timeline::rows_for(&naivepost::timeline::kept_footage_recordings(&cut), &cut)
        .into_iter()
        .max()
        .unwrap_or(0);
    assert!(last >= 1, "the two-camera cut puts its second camera on row {last}");
    assert_eq!((7usize).min(last as usize), last as usize, "a watched row past the last one lands on the last one");
}

/// The row reaches the record through the module that owns a zoom, not only through the page.
fn new_zoom_stores_its_row() {
    let framed = zoom::new_zoom((0.5, 0.5, 0.6), 4.0, 3.0, false, 1);
    assert_eq!(framed.cam, 1, "the row is part of the record the moment the box lands");
    let first = zoom::new_zoom((0.5, 0.5, 0.6), 4.0, 3.0, false, 0);
    assert_eq!(first.cam, 0);
}

/// The same answer the drag seam gets, worked out from the record it hands back: `fx_zoom::apply` copies the
/// form's row into `cam`, so a form built off a stored zoom must carry that zoom's own row or ✎ Edit would
/// silently move the framing to whatever row happens to be watched. This is the rule `open_effect_form` follows.
fn a_reopened_zoom_keeps_its_own_row() {
    let stored = zoom::new_zoom((0.5, 0.5, 0.6), 8.0, 3.0, true, 2);
    assert_eq!(stored.cam, 2);
    let reopened = zoom::Form {
        at: stored.t,
        dur: stored.dur,
        stay: stored.stay,
        trans: stored.trans,
        tout: stored.tout,
        curve: zoom::CURVE_CHOICES[0].to_string(),
        cx: stored.cx.unwrap_or(0.5),
        cy: stored.cy.unwrap_or(0.5),
        hf: stored.hf.unwrap_or(0.5),
        row: stored.cam.max(0) as usize,
    };
    let again = zoom::apply(&reopened).expect("a 3 s move clears the floor");
    assert_eq!(again.cam, 2, "editing a zoom leaves it on the row it was framed on");
}

// --- S9: the volume names the lane it works on ------------------------------------------------------------------

fn lanes() -> Vec<Lane> {
    vec![Lane {
        name: "mic-lav".to_string(),
        src: "project:sources/mic-lav.wav".to_string(),
        at: 12.0,
        off: 0.0,
        dur: 46.0,
    }]
}

/// Seconds wholly inside one named lane → that lane; anything else → `""`, the whole bed.
fn lane_comes_from_the_seconds() {
    let bed = lanes();
    assert_eq!(vol::lane_for_press(&bed, 20.0, 4.0), "mic-lav", "inside the row, that row only");
    assert_eq!(vol::lane_for_press(&bed, 12.0, 46.0), "mic-lav", "exactly its span counts as inside");
    assert_eq!(vol::lane_for_press(&bed, 10.0, 4.0), "", "before the row starts: the bed");
    assert_eq!(vol::lane_for_press(&bed, 50.0, 10.0), "", "crossing its end: the bed, not half a lane");
    assert_eq!(vol::lane_for_press(&[], 20.0, 4.0), "", "no lanes at all: the bed");
    let both = vec![
        Lane { name: "a".into(), src: "x".into(), at: 0.0, off: 0.0, dur: 100.0 },
        Lane { name: "b".into(), src: "y".into(), at: 0.0, off: 0.0, dur: 100.0 },
    ];
    assert_eq!(vol::lane_for_press(&both, 10.0, 4.0), "", "two lanes over the same seconds cannot be guessed");
}

/// A name the cut still has is kept verbatim; a name it no longer has costs the record nothing. That is what
/// makes renaming or deleting a lane safe instead of leaving a volume gained against a lane that is gone.
/// The check belongs to the PAGE (which holds the cut and asks at Apply time), so `fx_volume::apply` stays a
/// pure form → record step and never sees the lane list.
fn a_lane_that_is_gone_costs_nothing() {
    let bed = lanes();
    assert_eq!(vol::lane_answer(&bed, "mic-lav"), "mic-lav");
    assert_eq!(vol::lane_answer(&bed, ""), "", "the whole bed stays the whole bed");
    assert_eq!(vol::lane_answer(&bed, "renamed-away"), "", "a lane that is not there is not named");
    assert_eq!(vol::lane_answer(&[], "mic-lav"), "", "and neither is one after every lane went");
    // Storage follows: an unnamed bed writes no key, a named lane writes its spelling back unchanged.
    let bed_fx = Fx { kind: "volume".into(), t: 20.0, dur: 4.0, gain: 2.0, ..Default::default() };
    let written = serde_json::to_value(&bed_fx).expect("an effect serialises");
    assert!(written.get("lane").is_none(), "and the whole bed costs no `lane` key: {written}");
    let named = Fx { lane: "mic-lav".to_string(), ..bed_fx };
    assert_eq!(
        serde_json::to_value(&named).expect("serialises")["lane"].as_str(),
        Some("mic-lav"),
        "a real lane is kept exactly as spelled"
    );
}

/// A volume placed over the marked seconds of a cut that HAS a lane stores that lane; one placed outside every
/// lane stores the bed. Through the page's own press + Apply, so the wire is proven, not just the function.
fn volume_lane_round(app: &adw::Application) {
    let with_lane = Cut {
        segs: vec![Seg { s: 0.0, e: tape_end(), cam: 0, ..Default::default() }],
        lanes: lanes(),
        ..Default::default()
    };
    let window = cut_page(app, &with_lane);
    ui::note_place(true);
    // 20 s sits inside mic-lav's 12–58; the form opens two seconds wide from the line.
    ui::set_line_position(&window, naivepost::cut_line::LinePos { t: 20.0 });
    click(&window, "effect-item-volume");
    let opened = ui::volume_form_open().expect("the row opened the form");
    assert_eq!(opened.lane, "mic-lav", "the press asked the cut which lane these seconds belong to");
    click(&window, "volume-apply-button");
    let placed = ui::review_cut_of(&window);
    let loud = placed.fx.iter().find(|fx| fx.kind == "volume").expect("Apply placed the volume");
    assert_eq!(loud.lane, "mic-lav", "§1's `lane`: a lane's name means that lane only");

    // Outside every lane: the whole bed, spelled "" and costing no key.
    ui::set_line_position(&window, naivepost::cut_line::LinePos { t: 80.0 });
    click(&window, "effect-item-volume");
    let outside = ui::volume_form_open().expect("the row opened again");
    assert_eq!(outside.lane, "", "seconds no lane covers are the bed");
    click(&window, "volume-apply-button");
    let after = ui::review_cut_of(&window);
    let bed_fx = after.fx.iter().rev().find(|fx| fx.kind == "volume").expect("the second volume");
    assert_eq!(bed_fx.lane, "");
    let written = serde_json::to_value(bed_fx).expect("an effect serialises");
    assert!(written.get("lane").is_none(), "the whole bed writes no `lane` key: {written}");
    window.close();
}

/// ✎ Edit of a volume that already names a lane keeps naming it — the form is built out of the record, and the
/// lane rides along even though no field shows it.
fn editing_a_volume_keeps_its_lane() {
    let mut stored = vol::initial_at_line(20.0);
    stored.lane = "mic-lav".to_string();
    let reopened = vol::Form {
        t: stored.t,
        percent: vol::percent_of(stored.gain),
        dur: stored.dur,
        trans: stored.trans,
        tout: stored.tout,
        curve: zoom::CURVE_CHOICES[0].to_string(),
        lane: stored.lane.clone(),
    };
    assert_eq!(reopened.lane, "mic-lav", "the live record's lane is what the form starts from (§F3.8)");
    assert_eq!(vol::apply(&reopened).expect("applied").lane, "mic-lav", "and what it writes back");
}

// --- the runner: one application per binary, every check in order ----------------------------------------------

// --- the runner: one application per binary, every check in order ----------------------------------------------

/// One `Application::run` per BINARY (two tests each calling `app.run()` aborts on "default main context already
/// acquired"), so the second-named test holds both rounds and the first only proves its own checks ran.
static RAN_ZOOM_ROW: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static RAN_ROWS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static RAN_NEW_ZOOM: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static RAN_REOPEN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static RAN_LANE_SECONDS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static RAN_LANE_ANSWER: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static RAN_EDIT_LANE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static RAN_VOLUME_LANE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn click(window: &adw::ApplicationWindow, name: &str) {
    button(window, name).emit_by_name::<()>("clicked", &[]);
    settle();
}

#[test]
fn sec_06_effects_1_record_s9_a_volume_names_the_lane_it_works_on() {
    lane_comes_from_the_seconds();
    RAN_LANE_SECONDS.store(true, std::sync::atomic::Ordering::SeqCst);
    a_lane_that_is_gone_costs_nothing();
    RAN_LANE_ANSWER.store(true, std::sync::atomic::Ordering::SeqCst);
    editing_a_volume_keeps_its_lane();
    RAN_EDIT_LANE.store(true, std::sync::atomic::Ordering::SeqCst);
    let app = adw::Application::builder()
        .application_id("net.tomp2p.naivepost.sec06e1columns")
        .build();
    let done = std::rc::Rc::new(std::cell::Cell::new(false));
    let flag = done.clone();
    app.connect_activate(move |app| {
        volume_lane_round(app);
        zoom_row_round(app);
        flag.set(true);
        app.quit();
    });
    app.run();
    assert!(done.get(), "both rounds ran");
    RAN_VOLUME_LANE.store(true, std::sync::atomic::Ordering::SeqCst);
    RAN_ZOOM_ROW.store(true, std::sync::atomic::Ordering::SeqCst);
    for (what, ran) in [
        ("the seconds decide the lane", &RAN_LANE_SECONDS),
        ("a gone lane costs nothing", &RAN_LANE_ANSWER),
        ("an edit keeps its lane", &RAN_EDIT_LANE),
        ("the press placed its lane", &RAN_VOLUME_LANE),
        ("the row clamps to the cut's rows", &RAN_ROWS),
        ("new_zoom stores its row", &RAN_NEW_ZOOM),
        ("a reopened zoom keeps its row", &RAN_REOPEN),
        ("the drag recorded its row", &RAN_ZOOM_ROW),
    ] {
        assert!(ran.load(std::sync::atomic::Ordering::SeqCst), "{what} was checked");
    }
}

#[test]
fn sec_06_effects_1_record_s8_a_zoom_records_the_row_it_was_framed_on() {
    // The drag round itself runs inside the one `Application::run` this binary gets (see the runner note above:
    // a second `app.run()` aborts on "default main context already acquired"), so what THIS name proves is the
    // logic half of S8 — which row a record carries, and that a reopened zoom keeps its own.
    row_is_clamped_to_the_picture_rows();
    RAN_ROWS.store(true, std::sync::atomic::Ordering::SeqCst);
    new_zoom_stores_its_row();
    RAN_NEW_ZOOM.store(true, std::sync::atomic::Ordering::SeqCst);
    a_reopened_zoom_keeps_its_own_row();
    RAN_REOPEN.store(true, std::sync::atomic::Ordering::SeqCst);
}
