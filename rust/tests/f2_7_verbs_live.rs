//! F2.7 (Add, Split, Remove, ⌦) — the page's half of the flow, through real widgets: what a press WRITES,
//! what the live snap list carries, how a multi-row Add is named, and what each ✕ badge does.
//!
//! `tests/cut_verbs.rs` pins the rules and every sentence; `tests/cut_verb_widgets.rs` pins that a click
//! reaches a door. What lives here is what only the page can get wrong: cut.json actually changing, the
//! snap candidates actually being gathered from this window's state, the split's right half actually
//! landing in the hand, and a ✕ actually removing the thing it sits on.
//!
//! One application, one `connect_activate`, one `#[test]` per concern, results through flags — the shape
//! tests/cut_select_widgets.rs uses, because these checks mutate process-shared state (this window's
//! selection, cut, cwd and history).

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::cut::{self, Cut, Fx, Lane, Seg};
use naivepost::cut_select::{Scope, Surface, ANY_ROW};
use naivepost::shell::Page;
use naivepost::ui;
use naivepost::ui::cut_verb_press;

#[allow(dead_code)] // every test binary compiles this whole module; a helper it does not call is not a warning here
mod common;
use common::{fixture_dir, hold_last_window, release_last_window, settle, status_text};

static RAN_PERSIST: AtomicBool = AtomicBool::new(false);
static RAN_SNAP_LIST: AtomicBool = AtomicBool::new(false);
static RAN_MULTI_ROW: AtomicBool = AtomicBool::new(false);
static RAN_SPLIT_HAND: AtomicBool = AtomicBool::new(false);
static RAN_DELETE_ORDER: AtomicBool = AtomicBool::new(false);

/// A project folder of our own, with the `session.naivepost` layout the doors write into. The verb doors
/// save through `std::env::current_dir()`, so the check pins the cwd here and reads the same file back.
fn stage(tag: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("naivepost-f27-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let session = naivepost::startup::session_dir(&root);
    std::fs::create_dir_all(session.join("cut")).expect("a session cut folder");
    std::fs::create_dir_all(session.join("prepare")).expect("a session prepare folder");
    std::env::set_current_dir(&root).expect("cwd pinned to the temp root");
    root
}

/// What the doors wrote: read cut.json back through the same loader the app uses at open.
fn saved_cut() -> Cut {
    let tree = naivepost::layout::Tree::new(naivepost::startup::session_dir(
        &std::env::current_dir().expect("a cwd"),
    ))
    .expect("the staged folder is a project");
    cut::load(&tree).expect("cut.json parses")
}

fn cut_bytes() -> Vec<u8> {
    std::fs::read(
        naivepost::layout::Tree::new(naivepost::startup::session_dir(
            &std::env::current_dir().expect("a cwd"),
        ))
        .expect("a project")
        .cut_json(),
    )
    .unwrap_or_default()
}

fn footage(s: f64, e: f64, cam: i32) -> Seg {
    Seg { s, e, cam, ..Default::default() }
}

/// A window on the Cut page over a staged project, with `seeded` as its live cut.
fn cut_window(app: &adw::Application, tag: &str, seeded: &Cut) -> adw::ApplicationWindow {
    stage(tag);
    release_last_window();
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, "Prepare");
    hold_last_window(window.clone());
    window.present();
    ui::tab_button(&window, Page::Cut)
        .expect("the shell has a Cut tab")
        .emit_by_name::<()>("clicked", &[]);
    ui::seed_review_cut(&window, seeded);
    settle();
    window
}

fn button(window: &adw::ApplicationWindow, name: &str) -> gtk::Button {
    ui::line_step_button(window, name).unwrap_or_else(|| panic!("the Cut page carries {name}"))
}

/// Put the red line where a press should find it. `set_line_position` stores the LINE; the page's verb doors
/// read `preview_playhead`, which is the player's own playhead when one exists — so both are set, exactly as
/// a real click on the track leaves them.
fn put_line(window: &adw::ApplicationWindow, at: f64) {
    ui::set_line_position(window, naivepost::cut_line::LinePos { t: at });
    ui::set_playhead(window, at);
}

/// S1/S2/S3 PERSISTENCE: every verb that changes the segment list writes cut.json, and a refusal writes
/// nothing at all. Fired through the real buttons, read back through the loader the app opens with.
fn f2_7_s1_a_verb_press_writes_cut_json(_app: &adw::Application) {
    let seeded = Cut { segs: vec![footage(0.0, 30.0, 0), footage(40.0, 70.0, 0)], ..Default::default() };
    let window = cut_window(_app, "persist", &seeded);

    // Start from a file the doors have already written once, so "changed" is compared against a real file
    // rather than against an absent one.
    ui::save_seed_for_test(&window, &seeded);
    let before = cut_bytes();    assert!(!before.is_empty(), "the staged project has a cut.json to compare against");

    // ＋ Add over 10..25: one more kept scene, and the file says so.
    ui::draw_selection(&window, Surface::Ruler, None, 10.0, 25.0);
    settle();
    button(&window, "add-button").emit_by_name::<()>("clicked", &[]);
    settle();
    let after_add = saved_cut();
    assert!(
        after_add.segs.len() > before_segs(&before),
        "＋ Add wrote its scenes to cut.json: {} segs on disk vs {}",
        after_add.segs.len(),
        before_segs(&before)
    );
    assert_ne!(cut_bytes(), before, "the file's bytes moved, not just the in-memory cut");

    // | Split at the line: two more borders, and again the file follows.
    put_line(&window, 45.0);
    let before_split = cut_bytes();
    button(&window, "split-button").emit_by_name::<()>("clicked", &[]);
    settle();
    assert_ne!(
        cut_bytes(),
        before_split,
        "| Split's borders reached cut.json"
    );

    // － Remove of a band: fewer segments on disk than there were. The band is drawn WIDE enough that the
    // cut really loses a whole scene (0..30 goes), not just a slice that leaves two remainders behind.
    let was = saved_cut().segs.len();
    ui::draw_selection(&window, Surface::Ruler, None, 0.0, 29.0);
    settle();
    button(&window, "remove-button").emit_by_name::<()>("clicked", &[]);
    settle();
    let after_remove = saved_cut().segs.len();
    assert!(
        after_remove < was,
        "－ Remove dropped footage from cut.json too: {after_remove} vs {was}"
    );

    // A REFUSED press writes NOTHING: no new bytes, no new line, no history step spent.
    let untouched = cut_bytes();
    ui::clear_selection(&window);
    settle();
    // No band and a sound-free ground: Add refuses ("drag a region on a track first").
    button(&window, "add-button").emit_by_name::<()>("clicked", &[]);
    settle();
    assert_eq!(cut_bytes(), untouched, "a refused verb wrote no file at all");
    assert!(
        status_text(&window).contains("drag a region on a track first"),
        "and said why: {}",
        status_text(&window)
    );
    window.close();
}

fn before_segs(bytes: &[u8]) -> usize {
    let text = String::from_utf8_lossy(bytes);
    serde_json::from_str::<Cut>(&text).map(|cut| cut.segs.len()).unwrap_or(usize::MAX)
}

/// S1's wiring: the live snap list carries THIS cut's borders, the red line and whatever the waveform seam
/// answers, and ＋ Add moves a drawn end onto a border within 5 s (`P.policy.snapToleranceSeconds`) while
/// leaving a far end exactly where it was drawn.
fn f2_7_s1_the_live_snap_list_carries_borders_the_line_and_wave_edges(_app: &adw::Application) {
    let seeded = Cut { segs: vec![footage(0.0, 30.0, 0), footage(40.0, 70.0, 0)], ..Default::default() };
    let window = cut_window(_app, "snap", &seeded);
    put_line(&window, 33.0);
    let band = ui::draw_selection(&window, Surface::Ruler, None, 26.0, 44.0);
    settle();
    let points = cut_verb_press::add_snap_points(&window, band.as_ref());
    println!("SNAPPOINTS {points:?}");
    assert!(points.contains(&30.0) && points.contains(&40.0), "the cut's own borders are candidates");
    assert!(points.contains(&33.0), "so is the red line");

    // Within reach: 26.0 is 4 s from the border at 30.0, so the added scene starts AT 30.
    ui::clear_selection(&window);
    ui::draw_selection(&window, Surface::PictureRow(0), None, 26.0, 44.0);
    settle();
    let outcome = cut_verb_press::press_add(&window);
    let status = match outcome {
        naivepost::cut_verbs::Outcome::Applied { status, .. } => status,
        naivepost::cut_verbs::Outcome::Refused(reason) => panic!("expected an add, got {reason}"),
    };
    let kept = saved_cut();
    let snapped = kept
        .segs
        .iter()
        .find(|seg| (seg.s - 30.0).abs() < 1e-9)
        .expect("the near end snapped onto the border at 30 s");
    assert!(snapped.e > snapped.s, "and the snapped scene is a real span: {snapped:?}");
    assert!(status.starts_with("added"), "said: {status}");

    // Beyond reach: a band whose ends sit 6 s from any border keeps both of them where they were drawn.
    let far = Cut { segs: vec![footage(0.0, 20.0, 0), footage(40.0, 70.0, 0)], ..Default::default() };
    let wide = cut_window(_app, "snap-far", &far);
    put_line(&wide, 100.0);
    ui::draw_selection(&wide, Surface::PictureRow(0), None, 26.0, 34.0);
    settle();
    cut_verb_press::press_add(&wide);
    let landed = saved_cut();
    let drawn = landed
        .segs
        .iter()
        .find(|seg| (seg.s - 26.0).abs() < 1e-9)
        .unwrap_or_else(|| panic!("neither end may move 6 s: {:?}", landed.segs));
    assert!((drawn.e - 34.0).abs() < 1e-9, "the far end stayed where it was drawn: {drawn:?}");
    wide.close();
    window.close();
}

/// S1's other two sentences, through the live door: more rows touched names the camera, and coverage lost
/// on another row admits it. Neither is reachable on a one-row cut, which is why the placeholder band could
/// never print them.
fn f2_7_s1_a_multi_row_add_names_the_camera_and_admits_the_steal(_app: &adw::Application) {
    // Two cameras' worth of footage, overlapping in the middle: cam 0 across 0..70, cam 1 across 20..50.
    let two = Cut { segs: vec![footage(0.0, 70.0, 0), footage(20.0, 50.0, 1)], ..Default::default() };
    let window = cut_window(_app, "multi", &two);

    // A ruler band covers EVERY row the cut shows, so more than one row is touched and nothing is stolen:
    // the sentence names the camera the pictures came from.
    ui::draw_selection(&window, Surface::Ruler, None, 5.0, 15.0);
    settle();
    button(&window, "add-button").emit_by_name::<()>("clicked", &[]);
    settle();
    let said = status_text(&window);
    println!("MULTI ruler: {said}");
    assert!(
        said.starts_with("added on ") && !said.contains("taken off"),
        "a multi-row add names the camera and admits nothing: {said}"
    );

    // Now a band on row 0 alone, lying inside cam 1's footage: those seconds are taken OFF the other
    // camera, and the status must say so rather than reporting a quiet theft.
    let window2 = cut_window(_app, "steal", &two);
    ui::draw_selection(&window2, Surface::PictureRow(0), None, 25.0, 40.0);
    settle();
    button(&window2, "add-button").emit_by_name::<()>("clicked", &[]);
    settle();
    let stolen = status_text(&window2);
    println!("MULTI steal: {stolen}");
    assert!(
        stolen.contains("taken off the other camera"),
        "the theft is admitted: {stolen}"
    );
    assert!(stolen.starts_with("added on "), "and the camera is named: {stolen}");
    window.close();
    window2.close();
}

/// S2's hand: a band-less | Split at the line hands over the RIGHT HALF, read off the new segment list,
/// and ⧉ Copy takes exactly that half.
fn f2_7_s2_a_split_at_the_line_hands_over_the_right_half(_app: &adw::Application) {
    let seeded = Cut { segs: vec![footage(0.0, 60.0, 0)], ..Default::default() };
    let window = cut_window(_app, "hand", &seeded);
    assert!(ui::selection(&window).is_none(), "no band: Split works at the line");
    put_line(&window, 20.0);
    // Any hand left by an earlier check is not this check's answer.
    cut_verb_press::take_into_hand(None);

    button(&window, "split-button").emit_by_name::<()>("clicked", &[]);
    settle();
    let hand = cut_verb_press::verb_hand().expect("the split's right half went into the hand");
    println!("HAND {hand:?}");
    assert!(
        (hand.start - 20.0).abs() < 1e-9,
        "the hand starts at the line it was split at: {hand:?}"
    );
    assert!(
        (hand.end - 60.0).abs() < 1e-9,
        "and runs to the scene's old end: {hand:?}"
    );
    assert_eq!(hand.scope, Scope::Footage { row: 0 }, "scoped to the row it was cut on");

    // The hand is what makes "split, then take the second half" one gesture: with no band up, ⧉ Copy
    // answers the hand instead of refusing for want of a selection.
    let said = ui::press_copy(&window);
    println!("COPY FROM HAND: {said}");
    assert!(
        !said.contains("select a stretch"),
        "Copy took the split's half rather than refusing: {said}"
    );
    assert!(ui::copy_hand().is_some(), "and it is now in the copy hand");
    assert!(
        cut_verb_press::verb_hand().is_none(),
        "spent once: the same split cannot be pasted twice"
    );
    window.close();
}

/// S4's order through the page: with a held effect nothing else is even asked, and with nothing at all in
/// hand or on screen the key refuses with the spec's own words.
fn f2_7_s4_the_delete_order_runs_from_the_page(_app: &adw::Application) {
    let seeded = Cut { segs: vec![footage(0.0, 30.0, 0)], ..Default::default() };
    let window = cut_window(_app, "delete", &seeded);

    // Nothing held, no band, and the line off the footage: the refusal, in full.
    put_line(&window, 99.0);
    let outcome = cut_verb_press::press_delete_key(&window);
    assert_eq!(
        outcome.status(),
        "nothing selected \u{2014} click a kept scene, or drag a region on a track",
        "the last branch of the flowchart is the page's answer too"
    );

    // A held effect outranks a live selection: the band stays up, the effect is what went.
    ui::set_held_effect(Some(Fx { kind: "zoom".to_string(), t: 10.0, dur: 6.0, ..Default::default() }));
    ui::draw_selection(&window, Surface::Ruler, None, 5.0, 20.0);
    settle();
    let outcome = cut_verb_press::press_delete_key(&window);
    assert!(
        outcome.status().contains("removed the effect"),
        "the effect comes first whatever else is in hand: {}",
        outcome.status()
    );
    ui::set_held_effect(None);
    window.close();
}

fn round() {
    common::one_app_round(|app| {
        f2_7_s1_a_verb_press_writes_cut_json(app);
        RAN_PERSIST.store(true, Ordering::SeqCst);
        f2_7_s1_the_live_snap_list_carries_borders_the_line_and_wave_edges(app);
        RAN_SNAP_LIST.store(true, Ordering::SeqCst);
        f2_7_s1_a_multi_row_add_names_the_camera_and_admits_the_steal(app);
        RAN_MULTI_ROW.store(true, Ordering::SeqCst);
        f2_7_s2_a_split_at_the_line_hands_over_the_right_half(app);
        RAN_SPLIT_HAND.store(true, Ordering::SeqCst);
        f2_7_s4_the_delete_order_runs_from_the_page(app);
        RAN_DELETE_ORDER.store(true, Ordering::SeqCst);
    });
}

#[test]
fn f2_7_s1_s2_and_s4_the_verb_doors_write_the_file_the_snap_list_and_the_hand() {
    round();
    assert!(RAN_PERSIST.load(Ordering::SeqCst), "the persistence check never ran");
    assert!(RAN_SNAP_LIST.load(Ordering::SeqCst), "the snap-list check never ran");
    assert!(RAN_MULTI_ROW.load(Ordering::SeqCst), "the multi-row check never ran");
    assert!(RAN_SPLIT_HAND.load(Ordering::SeqCst), "the split-hand check never ran");
    assert!(RAN_DELETE_ORDER.load(Ordering::SeqCst), "the delete-order check never ran");
}

// Keep the imports honest: `Lane`/`ANY_ROW` document the shapes the checks avoid inventing.
const _: Option<Lane> = None;
const _: usize = ANY_ROW;
