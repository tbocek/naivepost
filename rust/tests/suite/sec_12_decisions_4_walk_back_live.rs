//! §12-decisions.md §4, fourth bullet — **the walk-back runs after every reply and the page shows what it
//! did** — proved on the live words door rather than only on the model arm.
//!
//! The five mechanical steps (edge snapping, hole closing, dead-air removal, coalescing, clamping) used to
//! live inside [`naivepost::suggest::apply`], which only `cut_suggest_model_run` reaches. A words-mode press
//! (§S2a, no model asked) ran `words_cut`, which trims to words, takes marks and dead air out and
//! coalesces — so two of the five steps never happened on that reply at all, and nothing said what the walk
//! had done either way. This file pins the three things that close that gap:
//!
//! * s1 — [`suggest::walk_back_spans`] is the whole walk in one function, and its account names both halves;
//! * s2 — the words door (`cut_suggest_words_run::run_words`) lands the walked spans and logs that account;
//! * s3 — a real click on the Cut page's ▶ reaches it, over a session whose wave cache makes hole closing
//!   change the answer.

use adw::prelude::*;
use naivepost::cut_effect_decisions::Candidate;
use naivepost::suggest::{self, Walk};
use naivepost::ui;
use naivepost::ui::cut_suggest_land;

#[path = "_f2_14_press_fixture.rs"]
mod press_fixture;
#[allow(unused_imports)]
// the wire scenario's helpers live in the shared walk-back fixture now
use press_fixture::retake;

use crate::common;
use common::settle;

/// A word edge at each second given, which is the candidate kind the walk trusts most after a shot change.
fn edges(seconds: &[f64]) -> Vec<(f64, Candidate)> {
    seconds
        .iter()
        .map(|at| (*at, Candidate::WordEdge))
        .collect()
}

/// The pair BOTH live checks start from: two stretches with a 1.2 s hole between them, i.e. under
/// `suggest::SEAM_MAX_SECONDS` (1.5 s) — a pause the speaker made that the walk is allowed to join back up.
/// Both its edges sit off the word edges below, so both move, which is what makes the walk's own count
/// checkable rather than always-zero.
pub fn stamped_pair() -> Vec<(f64, f64)> {
    vec![(4.0, 39.5), (19.2, 38.0)]
}

/// …and the walk that closes it: speech across the gap (under `SEAM_MAX_SECONDS`), word edges just outside both
/// stamps, nothing marked and no silence long enough to be dead air.
pub fn spoken_hole_walk() -> Walk {
    Walk {
        // The hole's OWN speech, which is what `hole_has_speech` asks about: a stretch that runs across the gap
        // from before it to after it. A span merely touching the gap does not count — `spans_overlap` is
        // half-open on the right even with `TALK_PAD_SECONDS` widening it — so this cannot be the trimmed run
        // next to the hole; it is somebody talking inside it.
        talked: vec![(17.0, 21.0)],
        silences: vec![],
        snap_points: edges(&[5.0, 39.0]),
        marks: vec![],
    }
}

/// The fixture both live checks use, shared so the wire test can compute the same `words_cut` the door does
/// and say exactly what the walk changed. Lives in its own module (see the bottom of this file) because GTK
/// allows one `run_with_args` per process, so the two window scenarios are two binaries.
#[path = "_sec_12_walk_back_fixture.rs"]
mod walk_fixture;
use walk_fixture::{base_cut, publish_tape, words_fixture};

/// Entry 4 / S1: the walk-back as ONE function — hole closed because somebody talked in it, both edges
/// snapped onto word edges, and an account that says both. Nothing here needs a window or a server.
#[test]
fn sec_12_decisions_4_what_stays_out_of_the_models_hands_on_purpose_s1_the_words_reply_runs_the_whole_walk_back(
) {
    // Two stamped stretches with a spoken hole between them: the 1.2 s gap sits inside the talk span and is
    // under `SEAM_MAX_SECONDS`, so the walk has to join them back together.
    let stamped = stamped_pair();
    let walk = spoken_hole_walk();
    let before = suggest::close_holes(&stamped, &walk.talked, suggest::SEAM_MAX_SECONDS);
    let walked = suggest::walk_back_spans(&stamped, &walk);
    assert_eq!(
        walked.segs.len(),
        1,
        "the spoken hole was closed: {:?} ({} before closing)",
        walked.segs,
        before.len()
    );
    assert_eq!(
        walked.segs,
        vec![(5.0, 39.0)],
        "the mid-word start moved onto the word edge outside it, and the end past the last word edge moved back"
    );
    assert_eq!(
        walked.closed_len, 2,
        "`closed_len` counts the stretches LEFT after closing, not the holes shut: {walked:?}"
    );
    assert_eq!(
        walked.moved, 2,
        "both stamped edges moved onto word edges, and the count says so: {walked:?}"
    );
    // §4's "the page shows what it did": the account names the stretch count AND the edge moves.
    let said = suggest::walk_report(walked.closed_len, walked.moved);
    assert!(
        said.contains("2 stretch") && said.contains("2 edge(s) moved"),
        "{said}"
    );
    // And a walk that changes nothing says nothing rather than claiming work: `apply` prints its own §S3
    // line unconditionally, but the words door appends this one only when the spans actually moved.
    let still = suggest::walk_back_spans(&[(5.0, 39.0)], &walk);
    assert_eq!(
        still.moved, 0,
        "edges already on candidates do not move: {still:?}"
    );
    assert_eq!(still.segs, vec![(5.0, 39.0)]);
}

/// The same rule read off `apply`, so the model arm's behaviour is unchanged by pulling the steps out: the
/// audit gate still refuses an unseen walk, and the clamp still runs on the walked spans.
#[test]
fn sec_12_decisions_4_what_stays_out_of_the_models_hands_on_purpose_s1_apply_and_the_new_door_agree(
) {
    let stamped = stamped_pair();
    let walk = spoken_hole_walk();
    let applied = suggest::apply(&stamped, &[], &[], &walk);
    let walked = suggest::walk_back_spans(&stamped, &walk);
    assert_eq!(
        applied.segs, walked.segs,
        "one implementation of the walk: apply must land exactly what the new door computes"
    );
    // An empty walk has nothing to snap to and nothing to remove: §1's fourth row refuses it, and the
    // refusal is `apply`'s alone — the audit gate stays where the model can be told about it.
    let bare = suggest::apply(&stamped, &[], &[], &Walk::default());
    assert!(
        bare.segs.is_empty(),
        "a walk with no materialisation places nothing: {bare:?}"
    );
    assert!(
        bare.log[0].starts_with("!!!"),
        "and says why: {:?}",
        bare.log
    );
}

/// Entry 4 / S2: the WORDS door itself, on the live path `press_words` uses.
#[test]
fn sec_12_decisions_4_what_stays_out_of_the_models_hands_on_purpose_s2_the_words_door_lands_the_walked_cut_and_says_so(
) {
    static RAN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(|app| {
            door_round(app);
            RAN.store(true, std::sync::atomic::Ordering::SeqCst);
            app.quit();
        });
        app.run_with_args::<String>(&[]);
    });
    assert!(
        RAN.load(std::sync::atomic::Ordering::SeqCst),
        "the words-door round never ran inside activate"
    );
}

/// A real window is needed because `run_words` lands its cut on the page, and libadwaita initialises only
/// inside the one `run_with_args` per process — so this runs in `activate`, beside s3's click scenario.
fn door_round(app: &adw::Application) {
    let f = words_fixture("door");
    let project = f.project.clone();
    let base = base_cut();
    publish_tape();
    let read = naivepost::cut_suggest_words_run::inputs(&f.tree, &project, &base);
    let plain = suggest::words_cut(&read.runs, &read.marks, &read.silences);
    // The fixture's own claim, checked against the walk's inputs rather than a guess: `close_holes` acts on
    // the trimmed RUNS (one per filmed stretch), so those are what must show the spoken gap.
    assert!(
        read.runs.len() > 1,
        "the base needs two filmed stretches for there to be a hole between them: {:?}",
        read.runs
    );
    let trimmed: Vec<(f64, f64)> = read.runs.iter().map(suggest::trim_run_to_words).collect();
    // The walk's OWN lists are what `walk_of` builds from these runs, so the hole has to be closed by exactly
    // that pair — not by a hand-written table the program never sees.
    let walk = naivepost::ui::cut_suggest_model_run::walk_of(&read);
    let closed = suggest::close_holes(&trimmed, &walk.talked, suggest::SEAM_MAX_SECONDS);
    assert!(
        closed.len() < trimmed.len(),
        "the envelope really puts a spoken hole between the trimmed runs: {trimmed:?} -> {closed:?}"
    );
    // And the seam is one this rule is allowed to close: over the silence floor the envelope reports (so it is
    // a pause the speaker made, not the gap between two words) and under the seam maximum (so closing it does
    // not join two distant thoughts). Stated as numbers here so a later change to either constant fails this
    // fixture loudly instead of quietly leaving it proving nothing.
    let seam = trimmed[1].0 - trimmed[0].1;
    assert!(
        seam > naivepost::cut_wave_edges::SILENCE_MIN_SECONDS
            && seam < suggest::SEAM_MAX_SECONDS
            && seam < suggest::DEAD_AIR_MAX_SECONDS,
        "the seam is {seam}s: outside ({}, {}, {}) the walk would not close it",
        naivepost::cut_wave_edges::SILENCE_MIN_SECONDS,
        suggest::SEAM_MAX_SECONDS,
        suggest::DEAD_AIR_MAX_SECONDS
    );

    let window = ui::build_window(app, &f.project, "Prepare");
    window.present();
    ui::seed_review_cut(&window, &base);
    cut_suggest_land::point_session_at(Some(f.tree.clone()));
    settle();
    publish_tape();
    let landed = naivepost::cut_suggest_words_run::run_words(&window, &f.tree, &project);
    settle();
    assert!(landed.wrote, "the words door wrote a cut: {landed:?}");
    assert_ne!(
        landed.cut.segs.len(),
        plain.segs.len(),
        "the walk-back changed what `words_cut` settled on: plain {:?} -> landed {:?}",
        plain.segs,
        landed.cut.segs
    );
    assert_eq!(
        ui::review_cut_of(&window).segs,
        landed.cut.segs,
        "the page holds the walked cut, not the unwalked one"
    );
    assert!(
        landed.log.contains("walk-back:") && landed.log.contains("edge(s) moved"),
        "§4's 'the page shows what it did', in the line the door returns: {:?}",
        landed.log
    );
    cut_suggest_land::point_session_at(None);
    window.close();
}

// ---- the fixture ----------------------------------------------------------------------------------------------
//
// The session folder both live checks dial lives in `_sec_12_walk_back_fixture.rs`, included by this file and
// by `tests/suite/sec_12_decisions_4_walk_back_live_b.rs`: libadwaita initialises only inside the one
// `run_with_args` a process gets, so the two window scenarios must be separate binaries that share a fixture.
