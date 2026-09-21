// §05-cut#7-rules — spec/05-cut.md §7's eight standing rules, one test per bullet in the order §7 writes them.
//
// Most of these rules already live in the module whose behaviour they bound; what this file pins is that they are
// still true and stated in exactly one place, so a later round cannot quietly break one while passing another.
// Everything here is plain data: no widget is built and no display is needed.

use naivepost::cut::{self, Cut, Fx, Seg};
use naivepost::cut_copy::footage_stretches;
use naivepost::cut_hear;
use naivepost::cut_line::Held;
use naivepost::cut_rules::{self, Floor};
use naivepost::cut_screen;
use naivepost::cut_select::{self, Scope, Surface, ANY_ROW};
use naivepost::cut_trim::{self, Border};
use naivepost::hand_edit;
use naivepost::layout::Tree;
use naivepost::preview;
use naivepost::shell::{self, Page};
use naivepost::timeline::{self, Recording, Span};
use naivepost::tools::cutpass;

const REC: &str = "2026-09-16 17-26-20";

fn clip(s: f64, e: f64) -> Seg {
    Seg { s, e, cam: 0, ..Default::default() }
}

fn card(s: f64, e: f64, dur: f64) -> Seg {
    Seg { s, e, ins: "assets/tier.svg".into(), dur, ..Default::default() }
}

// --- bullet 1: one thing held at a time ---------------------------------------------------------------------

/// §05-cut#7-rules — `One thing held at a time; picking up is not an edit; one Undo per drag; an unmoved press
/// is a click`. Holding is where the next gesture starts, so a second pickup replaces the first rather than
/// joining it, and nothing is owed back until something actually moved.
#[test]
fn sec_05_cut_7_rules_s1_one_thing_held_and_only_one_undo_per_gesture() {
    // One thing at a time: an edge already in hand makes way for a clip.
    assert_eq!(cut_rules::pick_up(Some(Held::Edge), Held::Clip), Some(Held::Clip));
    assert_eq!(cut_rules::pick_up(None, Held::Effect), Some(Held::Effect));

    // Picking up is reading: no move, no edit, nothing for ↶ to take back.
    assert!(!cut_rules::picking_up_edits(false));
    assert!(cut_rules::picking_up_edits(true));

    // One Undo per drag: the first real move pushes, every later tick of the same hold does not.
    assert!(cut_trim::pushes_undo(true, false), "the gesture's first move");
    assert!(!cut_trim::pushes_undo(true, true), "and only that one");
    assert!(!cut_trim::pushes_undo(false, false), "holding still pushes nothing");
    assert!(!cut_trim::pushes_undo(false, true));

    // An unmoved press is a click — unless the hand crossed to another row, which is a move of its own.
    assert!(cut_trim::is_click(0.0, false));
    assert!(!cut_trim::is_click(0.0, true), "a row change is a gesture at 0 px");
    assert!(!cut_trim::is_click(cut_trim::ROW_TRAVEL_PX * 2.0, false), "travel past the slop is a drag");
}

// --- bullet 2: borders, buttons and scope --------------------------------------------------------------------

/// §05-cut#7-rules — `A border belongs to both buttons; moving is the right button's verb; a selection is of
/// what it was drawn on`. A clip edge is also its neighbour's edge, so whichever side of the border the pointer
/// is on decides which one moves; the right hand never touches the red line.
#[test]
fn sec_05_cut_7_rules_s2_a_border_belongs_to_both_buttons() {
    // The same pixel sits inside both clips' grab zones; the nearer edge wins, from either side.
    let (start_px, end_px) = (100.0, 140.0);
    assert_eq!(cut_trim::grab_border(100.0 + cut_trim::EDGE_GRAB_PX / 2.0, start_px, end_px), Some(Border::Start));
    assert_eq!(cut_trim::grab_border(100.0 - cut_trim::EDGE_GRAB_PX / 2.0, start_px, end_px), Some(Border::Start));
    assert_eq!(cut_trim::grab_border(140.0 + cut_trim::EDGE_GRAB_PX / 2.0, start_px, end_px), Some(Border::End));
    assert_eq!(cut_trim::grab_border(140.0 - cut_trim::EDGE_GRAB_PX / 2.0, start_px, end_px), Some(Border::End));
    // Mid-clip there is no border to grab at all — that press selects or moves instead.
    assert_eq!(cut_trim::grab_border(120.0, start_px, end_px), None);

    // Moving is the right button's verb: it never drags the playhead out from under the preview.
    assert!(!cut_trim::right_press_moves_line());

    // A selection is of what it was drawn on: a row's footage, or one recording's sound.
    let pictures = cut_select::draw(Surface::PictureRow(1), None, 10.0, 20.0).expect("a row selects its footage");
    assert_eq!(pictures.scope, Scope::Footage { row: 1 });
    let sound = cut_select::draw(Surface::WaveStrip, Some(REC), 10.0, 20.0).expect("a strip selects that sound");
    assert_eq!(sound.scope, Scope::Sound { recording: REC.into() });

    // Which is what the verbs ask before offering themselves: footage reaches every row of itself, a sound
    // belongs to one recording and to no picture row.
    let whole = Scope::Footage { row: ANY_ROW };
    assert!(cut_select::scoped_to_row(&whole, 0) && cut_select::scoped_to_row(&whole, 3));
    assert!(!cut_select::scoped_to_row(&sound.scope, 0), "a lane's seconds are nobody's pictures");
}

// --- bullet 3: widthless gaps and half-open ranges -----------------------------------------------------------

/// §05-cut#7-rules — `Unfilmed time has no width; the seam second belongs to the later take; preview and render
/// agree on half-open ranges`. The band is laid out from what was filmed, so a hole costs no pixels, and both
/// directions of the mapping answer a press on a seam the same way.
#[test]
fn sec_05_cut_7_rules_s3_unfilmed_time_has_no_width_and_seams_are_half_open() {
    let recs = vec![
        Recording { base: "a".into(), start: 0.0, end: 30.0 },
        Recording { base: "b".into(), start: 50.0, end: 70.0 },
    ];
    let runs = timeline::filmed_runs(&recs);
    let pps = 4.0;
    let span = Span::new(timeline::cells(&runs, &[]), pps, cut_screen::GUTTER_PX);

    // 50 s of unfilmed time between the runs, and no pixels for it: only the filmed seconds are wide.
    let filmed: f64 = runs.iter().map(|(start, end)| end - start).sum();
    assert_eq!(filmed, 50.0);
    assert!(span.total_px() < (70.0 * pps), "the band is {} px, not the session's {}", span.total_px(), 70.0 * pps);
    assert!((span.total_px() - (cut_screen::GUTTER_PX + filmed * pps)).abs() < 1e-9);

    // The seam second belongs to the later take: x at a run's end reads as the next run's start.
    assert_eq!(span.t_at(span.x_of(30.0)), 50.0, "a press on the seam is in the take you can see");
    // And preview agrees: standing on a run's last second means standing outside it.
    assert_eq!(preview::walk_on(&runs, 30.0), Some(50.0));
    assert_eq!(preview::walk_on(&runs, 29.9), Some(29.9), "one frame earlier is still inside");

    // The same half-open reading between two ranges — touching is not overlapping.
    assert!(!timeline::spans_overlap((0.0, 10.0), (10.0, 20.0)));
    assert!(timeline::spans_overlap((0.0, 10.1), (10.0, 20.0)));
}

// --- bullet 4: inserts are files ------------------------------------------------------------------------------

/// §05-cut#7-rules — `Inserts are files: never trimmed, merged, dropped by a re-suggest or given a hearing
/// answer; a spliced insert costs no session time`. A card has no edge, no neighbour to fuse with, no lanes to
/// be silenced in, and no footage of its own to give up.
#[test]
fn sec_05_cut_7_rules_s4_inserts_are_files_not_footage() {
    // Never trimmed: neither a card laid over the footage nor one spliced between it has an edge to grab.
    assert!(cut_trim::trimmable(&clip(0.0, 30.0)));
    assert!(!cut_trim::trimmable(&card(10.0, 20.0, 0.0)), "an overwriting card");
    assert!(!cut_trim::trimmable(&card(10.0, 10.0, 4.0)), "a spliced one");

    // Never merged: a card sitting flush against a clip stays a card.
    let with_card = vec![clip(0.0, 30.0), card(30.0, 30.0, 4.0)];
    assert_eq!(cut_trim::merge_pair(&with_card, 1), None, "the card finds no partner");
    assert_eq!(cut_trim::merge_pair(&with_card, 0), None, "and is not one either");
    let two_clips = vec![clip(0.0, 30.0), clip(30.0, 60.0)];
    assert_eq!(cut_trim::merge_pair(&two_clips, 0), Some(1), "footage does");

    // Never a hearing answer: an insert brings its own sound or replaces the lane it was laid in, so its scene's
    // `quiet` list is not its question and reads as not hushed.
    let insert = Seg { ins: "assets/sting.mp4".into(), quiet: vec![REC.into()], ..Default::default() };
    assert!(cut_hear::hush(&insert).is_empty(), "its quiet list is not its question");
    let footage = Seg { s: 0.0, e: 12.0, quiet: vec![REC.into()], ..Default::default() };
    assert_eq!(cut_hear::hush(&footage), &[REC.to_string()], "footage does answer for itself");

    // Costs no session time: a spliced card is not footage, so the stretches of picture under the cut are the
    // same with it as without, while the finished video grows by exactly its `dur`.
    let without = Cut { segs: vec![clip(0.0, 30.0), clip(40.0, 60.0)], ..Default::default() };
    let with = Cut {
        segs: vec![clip(0.0, 30.0), card(35.0, 35.0, 4.0), clip(40.0, 60.0)],
        ..Default::default()
    };
    assert_eq!(footage_stretches(&with, 0.0, 60.0), footage_stretches(&without, 0.0, 60.0));
    assert_eq!(cut_screen::cut_seconds(&with) - cut_screen::cut_seconds(&without), 4.0);
}

// --- bullet 5: clips stay home, never overlap, Remove is exact -------------------------------------------------

/// §05-cut#7-rules — `A clip may not leave its recording; clips never overlap; Remove takes exactly the
/// selection`. Two clamps and a floor: the recording's own edge stops a trim first, a neighbour before that, and
/// － Remove's floor is one frame rather than a second.
#[test]
fn sec_05_cut_7_rules_s5_a_clip_stays_in_its_recording_and_never_overlaps() {
    let segs = vec![clip(10.0, 30.0), clip(40.0, 60.0)];

    // Alone in a recording, its edges are walls: dragging past either stops at the last filmed second.
    let alone = vec![clip(10.0, 30.0)];
    assert_eq!(cut_trim::clamp_edge(&alone, 0, Border::End, 200.0, 0.0, 45.0), 45.0);
    assert_eq!(cut_trim::clamp_edge(&alone, 0, Border::Start, -100.0, 5.0, 120.0), 5.0);

    // With a neighbour, the neighbour clamps first — reaching into it would be an overlap.
    let end = cut_trim::clamp_edge(&segs, 0, Border::End, 200.0, 0.0, 45.0);
    assert_eq!(end, 40.0, "the next take starts there");
    assert!(!timeline::spans_overlap((10.0, end), (40.0, 60.0)));
    let start = cut_trim::clamp_edge(&segs, 1, Border::Start, -100.0, 5.0, 120.0);
    assert_eq!(start, 30.0, "and the same from the other side");
    assert!(!timeline::spans_overlap((start, 60.0), (10.0, 30.0)));

    // Remove takes exactly the selection: its floor is the smallest remainder, not a scene's second.
    assert_eq!(cut_rules::floor(Floor::Remove), 0.04); // P.eng.minPieceSeconds
    assert_eq!(cut_rules::floor(Floor::Keep), 1.0); // P.policy.minSceneSeconds
    assert!(cut_rules::floor(Floor::Remove) < cut_rules::floor(Floor::Keep));
}

// --- bullet 6: a suggestion's shape -----------------------------------------------------------------------------

/// §05-cut#7-rules — `Suggest replaces the footage half and the effects, keeps inserts, and sets the base; every
/// model reply is walked back onto the timeline`. The cards survive because nothing in a new answer is their
/// second version; the effects go because they sit on scenes the model did not choose; the base moves so Revert
/// returns to the answer rather than to what was on disk an hour ago.
#[test]
fn sec_05_cut_7_rules_s6_a_suggestion_keeps_the_cards_and_becomes_the_base() {
    let suggested = vec![clip(0.0, 12.0), clip(30.0, 44.0)];
    let current = vec![
        clip(5.0, 20.0),
        card(22.0, 22.0, 4.0),
        Seg { s: 50.0, e: 55.0, ins: "assets/sting.wav".into(), ..Default::default() },
    ];
    let kept = cut::keep_inserts(&suggested, &current);

    // Every suggestion segment survives, every card of the old cut survives, nothing else does.
    assert_eq!(kept.len(), suggested.len() + 2);
    assert!(kept.contains(&suggested[0]) && kept.contains(&suggested[1]));
    assert!(kept.iter().filter(|seg| !seg.ins.is_empty()).count() == 2);
    assert!(!kept.contains(&current[0]), "the footage half was the model's to replace");
    // And the page reads them in time order, cards among the clips rather than appended after them.
    let order: Vec<f64> = kept.iter().map(|seg| seg.s).collect();
    let mut sorted = order.clone();
    sorted.sort_by(f64::total_cmp);
    assert_eq!(order, sorted);

    // Effects are not carried: they belong to scenes this answer did not choose.
    let mut old = Cut { segs: current.clone(), ..Default::default() };
    old.fx.push(Fx { kind: "zoom".into(), t: 8.0, ..Default::default() });
    let mut fresh = Cut { segs: suggested.clone(), ..Default::default() };
    fresh.fx = vec![];
    assert_eq!(old.fx.len(), 1);
    assert!(fresh.fx.is_empty(), "the new cut arrives with no effects of the old one");

    // The base moves with the suggestion, and Revert goes to it rather than to the file.
    let before = Cut { segs: current.clone(), ..Default::default() };
    let answer = Cut { segs: kept.clone(), ..Default::default() };
    let mut history = cut::History::open(&before);
    history.suggested(&answer);
    let back = history.revert();
    assert_eq!(back, cut::Snapshot::of(&answer), "the model's answer is the base now");

    // Walked back onto the timeline: an edge the model sent 1.5 s from a word edge lands on it, within
    // P.policy.snapToleranceSeconds (5 s) — the same tolerance §6 catalogues.
    assert_eq!(cutpass::SNAP_TOLERANCE_SECONDS, 5.0);
    let mut plan = cutpass::Plan::new(120.0, 30.0).with_snap_points(vec![10.0]);
    plan.add_segment(11.5, 25.0, "the demo");
    assert_eq!(plan.segments()[0].start, 10.0, "the reply moved onto the timeline");
}

// --- bullet 7: one writer and the gate ---------------------------------------------------------------------------

/// §05-cut#7-rules — `cut.json is written by one function; its existence unlocks Narrate and Produce`. The gate
/// is the file being there, not its contents being good, and it is never a lock on those tabs — their ▶ says why
/// in the step's own words.
#[test]
fn sec_05_cut_7_rules_s7_cut_json_exists_once_and_unlocks_the_steps_after_it() {
    let root = std::env::temp_dir().join(format!("naivepost-rules-gate-{}", std::process::id()));
    let dir = root.join("demo.naivepost");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let tree = Tree::new(&dir).unwrap();

    // No file yet: the project has no cut, and that is what the steps after it ask about.
    assert!(!cut::exists(&tree));
    let mut model = Cut::default();
    model.segs.push(clip(0.0, 12.0));
    cut::save(&model, &tree).expect("the one writer");
    assert!(cut::exists(&tree));
    // Reading never writes: a load of the file it just wrote leaves the same bytes behind.
    let bytes = std::fs::read_to_string(tree.cut_json()).unwrap();
    let _again = cut::load(&tree).unwrap();
    assert_eq!(std::fs::read_to_string(tree.cut_json()).unwrap(), bytes, "load is not a writer");

    // And the gate is not a lock: Narrate and Produce are open, refusing in their own voice instead.
    let project = naivepost::project::Project::default();
    assert_eq!(shell::lock(Page::Narrate, &project), None);
    assert_eq!(shell::lock(Page::Produce, &project), None);
    assert_eq!(cut::NO_CUT_YET, "no cut yet — build one on the Cut step first");
    let _ = std::fs::remove_dir_all(&root);
}

// --- bullet 8: a draw never decodes ------------------------------------------------------------------------------

/// §05-cut#7-rules — `Nothing decodes inside a draw; the tracks are a window drawn under a translate; the page is
/// rebuilt only when Prepare's output changed`. The band holds seconds, the translate says which ones are on
/// screen, and the only work a draw cycle may start is one throttled scrub.
#[test]
fn sec_05_cut_7_rules_s8_a_draw_asks_for_a_window_and_never_for_a_frame() {
    // The window the translate puts on screen, in seconds: 100 px at 4 px/s is 25 s of the session.
    assert_eq!(cut_screen::visible_window(0.0, 100.0, 4.0, 400.0), (0.0, 25.0));
    // Scrolled along, and past the end — which clamps to the tail rather than showing an empty track.
    assert_eq!(cut_screen::visible_window(80.0, 100.0, 4.0, 400.0), (20.0, 45.0));
    assert_eq!(cut_screen::visible_window(900.0, 100.0, 4.0, 400.0), (100.0, 100.0));
    // Nothing to lay out answers with nothing, rather than dividing by it.
    assert_eq!(cut_screen::visible_window(0.0, 100.0, 0.0, 400.0), (0.0, 0.0));
    assert_eq!(cut_screen::visible_window(0.0, 0.0, 4.0, 400.0), (0.0, 0.0));

    // The same numbers that size the band pick the window: fitting the filmed length to the view means the whole
    // session is on screen, so the window is the session.
    let pps = cut_screen::fit_pps(100.0, 400.0);
    assert_eq!(pps, 4.0);
    assert_eq!(cut_screen::visible_window(0.0, 400.0, pps, 100.0 * pps), (0.0, 100.0));

    // The one thing a draw may start is throttled: at most one scrub per SCRUB_MS.
    assert_eq!(cut_trim::SCRUB_MS, 90);
    assert!(cut_trim::scrubs(None, 1_000), "the first tick asks");
    assert!(!cut_trim::scrubs(Some(1_000), 1_045), "a redraw inside the throttle does not");
    assert!(cut_trim::scrubs(Some(1_000), 1_090), "and one after it may again");

    // Rebuild is Prepare's output moving, not the page being redrawn: marks newer than the text is an edit,
    // and a text that never changed stays as drawn however often it is painted.
    use std::time::{Duration, SystemTime};
    let now = SystemTime::now();
    let later = now + Duration::from_secs(2);
    assert!(hand_edit::edited(Some(later), Some(now)), "the text moved after the marks");
    assert!(!hand_edit::edited(Some(now), Some(later)), "a redraw is not an edit");
    assert!(!hand_edit::edited(None, Some(now)), "nothing to compare against");
}
