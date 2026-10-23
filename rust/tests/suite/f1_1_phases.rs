//! F1.1 S6 — the bar's three phase shares, as plain numbers and through a real walk.
//!
//! §S6 gives Prepare's progress bar its shape: voice separation 0–10 % when asked, ingest to 30 %,
//! understand 30–100 %. Those boundaries live in `prepare_run::share`; what this file pins is that
//! they are the numbers the run actually walks with — which stage gets which slice (`phase_of_stage`),
//! and that a queue given those slices shows them on its fraction (`runqueue::Queue`, no window).

use naivepost::prepare_run::{self, Phase};
use naivepost::project::{Project, Source};
use naivepost::run::Subprocesses;
use naivepost::run_stages::{self, Stage, Task};
use naivepost::runqueue::{Queue, TRACK_FRAMES, TRACK_STT};

fn source(path: &str, footage: bool, sepvoice: bool) -> Source {
    Source {
        path: path.to_string(),
        footage,
        narrator: 0,
        sepvoice,
        tracks: vec![],
    }
}

fn project(rows: Vec<Source>) -> Project {
    Project {
        sources: rows,
        ..Project::default()
    }
}

/// One task that does nothing but return: `/bin/true` exists everywhere this suite runs, so the walk
/// is the program's own and only the tool is trivial.
fn noop_task(label: &str) -> Task {
    Task {
        label: label.to_string(),
        program: "/bin/true".to_string(),
        args: vec![],
        already_done: false,
        session: None,
    }
}

fn stage(name: &'static str, track: usize, kind: &'static str) -> Stage {
    Stage {
        track,
        name,
        phase: 1,
        of: 2,
        kind,
        tasks: vec![noop_task(name)],
    }
}

// ---- the numbers themselves ---------------------------------------------------

/// §S6's three shares, exactly as the spec writes them, separation asked for.
#[test]
fn f1_1_s6_the_bar_slices_prepare_three_ways() {
    assert_eq!(prepare_run::share(Phase::Separate, true), (0.0, 0.10));
    assert_eq!(prepare_run::share(Phase::Ingest, true), (0.10, 0.30));
    assert_eq!(prepare_run::share(Phase::Understand, true), (0.30, 1.0));
}

/// Separation not asked: its slice is zero-width, and the other two keep their numbers — the bar moves
/// straight from 0 into ingest rather than crawling through an empty tenth.
#[test]
fn f1_1_s6_a_run_nobody_asked_to_separate_starts_at_ingest() {
    assert_eq!(prepare_run::share(Phase::Separate, false), (0.0, 0.0));
    assert_eq!(prepare_run::share(Phase::Ingest, false), (0.10, 0.30));
    assert_eq!(prepare_run::share(Phase::Understand, false), (0.30, 1.0));

    // Through a real queue: with no separation stage the first news the bar gets is inside ingest's
    // slice. `Queue::phase` seeds BOTH halves at `base/2` (the two tracks are concurrent, §S6), so one
    // track a quarter through ingest already reads as base + a quarter of the slice.
    let mut queue = Queue::new();
    let (base, share) = prepare_run::share(Phase::Ingest, false);
    queue.phase(base, share);
    queue.push(TRACK_STT, 4, "chunk");
    queue.take(TRACK_STT);
    queue.prog(TRACK_STT, 0.25, "transcribing lecture");
    let expected = base + 0.25 * share;
    assert!(
        (queue.fraction() - expected).abs() < f64::EPSILON,
        "ingest's slice is what the bar shows: {} vs {expected}",
        queue.fraction()
    );
    assert!(
        queue.fraction() > 0.0 && queue.fraction() <= 0.40,
        "inside ingest's slice, and nowhere near understand's: {}",
        queue.fraction()
    );
}

/// The end of a Prepare run reaches the top of the bar: understand's slice tops out at 1.0, so both
/// tracks done inside it sum to a full bar (§S7's "fraction 1" comes from these same numbers).
#[test]
fn f1_1_s6_understand_finished_is_a_full_bar() {
    let mut queue = Queue::new();
    let (base, share) = prepare_run::share(Phase::Understand, false);
    queue.phase(base, share);
    queue.done(TRACK_STT, 1.0);
    queue.done(TRACK_FRAMES, 1.0);
    assert!(
        (queue.fraction() - 1.0).abs() < f64::EPSILON,
        "the last phase's finished tracks fill the bar: {}",
        queue.fraction()
    );
}

// ---- which stage sits in which slice -----------------------------------------

/// The names `plan(Step::Prepare, …)` builds are the names `phase_of_stage` matches. Asserting the set
/// first means this test cannot pass quietly against stage names `plan` no longer uses.
#[test]
fn f1_1_s6_the_stage_names_map_to_phases() {
    let with_split = project(vec![
        source("project:sources/lecture.mkv", true, true),
        source("project:sources/mic.wav", false, false),
    ]);
    let names: Vec<&str> = run_stages::plan(naivepost::run::Step::Prepare, &with_split)
        .iter()
        .map(|stage| stage.name)
        .collect();
    assert!(
        names.contains(&"frames") && names.contains(&"speech"),
        "ingest stages are planned for this session: {names:?}"
    );
    assert!(
        names.contains(&"separation"),
        "and 🗣 plans separation: {names:?}"
    );
    for name in names.clone() {
        assert!(
            run_stages::phase_of_stage(name).is_some(),
            "`plan` produced {name:?}, which maps to no §S6 phase -- the mapping has drifted: {names:?}"
        );
    }
    assert_eq!(
        run_stages::phase_of_stage("separation"),
        Some(Phase::Separate)
    );
    assert_eq!(run_stages::phase_of_stage("frames"), Some(Phase::Ingest));
    assert_eq!(run_stages::phase_of_stage("speech"), Some(Phase::Ingest));
    assert_eq!(
        run_stages::phase_of_stage("describe"),
        Some(Phase::Understand)
    );
    assert_eq!(run_stages::phase_of_stage("fix"), Some(Phase::Understand));
    // A stage outside Prepare's phases keeps the whole bar rather than being forced into a slice.
    assert_eq!(run_stages::phase_of_stage("align"), None);
}

/// The slices a plan gets, in walk order: separation first when asked, then both ingest stages, then
/// understanding — one `(base, share)` per stage, matching `share`'s numbers.
#[test]
fn f1_1_s6_a_planned_run_gets_one_slice_per_stage_in_order() {
    let with_split = project(vec![
        source("project:sources/lecture.mkv", true, true),
        source("project:sources/mic.wav", false, false),
    ]);
    let stages = run_stages::plan(naivepost::run::Step::Prepare, &with_split);
    let slices = run_stages::phase_slices(&stages, prepare_run::separation_asked(&with_split));
    assert_eq!(slices.len(), stages.len());
    assert!(
        prepare_run::separation_asked(&with_split),
        "a footage row with \u{1f5e3} asks for separation"
    );
    for (stage, slice) in stages.iter().zip(&slices) {
        match run_stages::phase_of_stage(stage.name) {
            Some(phase) => assert_eq!(*slice, prepare_run::share(phase, true), "{}", stage.name),
            None => panic!("{} mapped to no phase", stage.name),
        }
    }
    // Without the tick the same plan starts at ingest's slice, because there is no separation stage.
    let plain = project(vec![source("project:sources/lecture.mkv", true, false)]);
    let plain_stages = run_stages::plan(naivepost::run::Step::Prepare, &plain);
    let plain_slices =
        run_stages::phase_slices(&plain_stages, prepare_run::separation_asked(&plain));
    assert!(
        !plain_slices.contains(&(0.0, 0.10)),
        "no separation stage means no separation slice: {plain_slices:?}"
    );
}

// ---- the walk applies them ----------------------------------------------------

/// The wire inside the driver: walking a Prepare-shaped plan puts the queue in the stage's slice, so
/// the bar's fraction during the FIRST stage is inside that stage's slice and not a whole bar.
#[test]
fn f1_1_s6_the_walk_puts_each_stage_in_its_slice() {
    let stages = vec![
        stage("separation", TRACK_STT, "file"),
        stage("frames", TRACK_FRAMES, "frame"),
    ];
    let mut queue = Queue::new();
    let mut procs = Subprocesses::default();
    let slices = run_stages::phase_slices(&stages, true);
    // `drive_page` is what hands these numbers to the walk; the same list, driven directly here so the
    // assertion is about the queue rather than about a window.
    let drive = run_stages::drive_walk_with_slices_for_test(
        &stages,
        &mut queue,
        || false,
        || false,
        |_| {},
        &mut procs,
        &slices,
    );
    assert_eq!(
        drive,
        naivepost::run_stages::Drive::Done { ran: 2, skipped: 0 }
    );
    // The last stage walked was frames, whose slice re-seeded the queue at ingest's base; its own half
    // finished inside that slice, so the bar reads as ingest done — partway up the run, not a full bar.
    let (ingest_base, ingest_share) = prepare_run::share(Phase::Ingest, true);
    let expected = ingest_base + ingest_share;
    assert!(
        (queue.fraction() - expected).abs() < 1e-9,
        "the frames stage finished inside ingest's slice: {} vs {expected}",
        queue.fraction()
    );
    assert!(
        queue.fraction() > 0.0 && queue.fraction() < 1.0,
        "a run still inside ingest does not read as finished: {}",
        queue.fraction()
    );
}
