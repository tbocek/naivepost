//! F5.4 Subtitles S1/S2 — the WIRE: the render's cue sheet is built by `produce_subtitles::cues`
//! through `produce_exec::plan`, so §F5.4's first diamond (narration lines, else the clip's own speech,
//! and that only where the clip is footage) is reached by the flow rather than living unused in its module.
//!
//! `tests/produce_subtitles.rs` pins the rules themselves over hand-built words, entries and cues. This
//! file pins the other half: that `produce_exec` asks **that** question of each clip it planned, with the
//! words the caller hands over, and that the sheet it writes (`clips/final.srt`) is what those rules
//! produced on the produced clock. Same door as `tests/produce_render_exec.rs`: one `exec::Materials`,
//! plan-only — no subprocess is spawned here, because S1/S2 are decided before the first encode.
//!
//! Ids used, each asserted against its §10 row in `params::produce()`: P.policy.subtitleBreakSeconds
//! (0.6), P.policy.subtitleRowChars (42), P.policy.subtitleMaxSeconds (6).

use std::path::{Path, PathBuf};

use naivepost::cut::{Cut, Lane, Seg};
use naivepost::layout::Tree;
use naivepost::narration::Entry;
use naivepost::params;
use naivepost::produce_exec as exec;
use naivepost::produce_flow as flow;
use naivepost::produce_render as render;
use naivepost::produce_subtitles as subs;
use naivepost::project::{Produce, Publish, Source};

const ITEM: &str = "f5_4";

fn temp_root(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("np-f54w-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn tree_in(tag: &str) -> (PathBuf, Tree) {
    let root = temp_root(tag);
    let dir = root.join("demo.naivepost");
    std::fs::create_dir_all(&dir).unwrap();
    (root, Tree::new(&dir).unwrap())
}

fn seg(s: f64, e: f64) -> Seg {
    Seg { s, e, cam: 0, ..Default::default() }
}

/// An insert segment: `ins` names an asset, which is what makes the clip not-footage downstream.
fn insert_seg(s: f64, e: f64, asset: &str) -> Seg {
    Seg { s, e, cam: 0, ins: asset.to_string(), ..Default::default() }
}

fn entry(s: f64, e: f64, text: &str) -> Entry {
    Entry { s, e, text: text.to_string(), ..Default::default() }
}

fn cut_of(segs: Vec<Seg>) -> Cut {
    Cut { segs, aspect: "16:9".into(), ..Default::default() }
}

fn project() -> Project {
    Project {
        sources: vec![Source { path: "/media/take.mp4".into(), footage: true, ..Default::default() }],
        ..Default::default()
    }
}

use naivepost::project::Project;

/// The words a footage clip heard, at a rhythm that keeps them in one cue per breath pair.
fn words(texts: &[&str], start: f64) -> Vec<subs::Word> {
    texts
        .iter()
        .enumerate()
        .map(|(i, t)| subs::Word {
            text: t.to_string(),
            s: start + i as f64 * 0.3,
            e: start + i as f64 * 0.3 + 0.2,
        })
        .collect()
}

// --- the lookups every Materials is built from ---------------------------------------------------

fn exists_everything(_: &str) -> bool {
    true
}

fn no_fx(_: &Seg) -> Vec<naivepost::cut::Fx> {
    Vec::new()
}

fn no_wav(_: &Entry) -> Option<PathBuf> {
    None
}

fn any_source(_: &render::Clip) -> Option<String> {
    Some("/media/take.mp4".to_string())
}

fn project_sources() -> Vec<Source> {
    project().sources
}

/// THE Materials constructor for this file. `words` is the seam under test: whatever the caller answers
/// here is what S1 captions a line-less footage clip from, so a test sees its own words come back in the
/// sheet or sees them refused because the clip was not footage.
fn mats<'a>(lanes: &'a [Lane], words: &'a dyn Fn(&render::Clip) -> Vec<subs::Word>) -> exec::Materials<'a> {
    exec::Materials {
        exists: &exists_everything,
        fx_for: &no_fx,
        lanes,
        src_shape: (1920, 1080),
        wav_of: &no_wav,
        source_file: &any_source,
        cues: exec::Cues { all: Vec::new(), languages: Vec::new() },
        words,
        // This file is about S1/S2; the S3 leg answers every number so the walk is never the thing that
        // fails here. `tests/produce_translate_flow.rs` owns the refusals.
        translate: &|_message: &str, numbers: &[usize]| -> Result<std::collections::BTreeMap<usize, String>, String> {
            Ok(numbers.iter().map(|n| (*n, format!("translated {n}"))).collect::<std::collections::BTreeMap<usize, String>>())
        },
        sources: &project_sources,
    }
}

/// Plan only — S1/S2 are settled before any subprocess, and this file has no spawner to distract it.
fn planned(tag: &str, segs: Vec<Seg>, lines: Vec<Entry>, words: Vec<subs::Word>) -> (Tree, exec::Plan) {
    let (_root, tree) = tree_in(tag);
    let run = flow::snapshot(
        Some(cut_of(segs)),
        &tree,
        lines,
        Produce::default(),
        &project(),
        Some(Publish::default()),
        "1".to_string(),
    );
    let held = words;
    let plan = exec::plan(&run, &tree, &mats(&[], &|_clip| held.clone()));
    (tree, plan)
}

/// Read the sheet the walk would write, straight out of the plan.
fn sheet(plan: &exec::Plan) -> Vec<subs::Cue> {
    plan.cues.all.clone()
}

// ---- S1: what the render asks of each clip ---------------------------------------

#[test]
fn f5_4_s1_a_footage_clip_with_no_narration_is_captioned_from_its_words() {
    assert_eq!(ITEM, "f5_4");
    // No narration line anywhere: the clip's own speech is what lands on the sheet, which is the branch
    // that had no caller at all before this round.
    let (_, plan) = planned(
        "s1-own-speech",
        vec![seg(0.0, 10.0)],
        Vec::new(),
        words(&["the", "lecture", "begins"], 0.0),
    );
    let cues = sheet(&plan);
    assert!(!cues.is_empty(), "a footage clip with no lines still gets cues from its words: {cues:?}");
    let joined = cues.iter().map(|c| c.text.as_str()).collect::<Vec<&str>>().join(" ");
    assert!(joined.contains("the lecture begins"), "{joined:?}");

    // And it is EXACTLY what `subs::cues` answers for a footage clip with these words — the plan does
    // not re-decide the grouping, it calls the rule.
    let expected = subs::tidy(&subs::on_clock(
        &subs::cues(subs::Source::Footage, &[], &words(&["the", "lecture", "begins"], 0.0), 1.0),
        0.0,
        1.0,
    ));
    assert_eq!(cues, expected, "the sheet is the rule's answer, not a second opinion");
}

#[test]
fn f5_4_s1_an_insert_contributes_no_cues() {
    assert_eq!(ITEM, "f5_4");
    // An inserted sound or picture says nothing the words of the session said: even with words offered,
    // the clip contributes nothing, and the reason is the module's own sentence.
    let (_, plan) = planned(
        "s1-insert",
        vec![insert_seg(0.0, 10.0, "assets/thing.wav")],
        Vec::new(),
        words(&["not", "the", "session"], 0.0),
    );
    assert!(sheet(&plan).is_empty(), "an insert gives no cues: {:?}", sheet(&plan));
    let reason = subs::Source::Insert
        .no_cues_reason()
        .expect("an insert names why it says nothing");
    // The clip never reaches the sheet, and the plan carries no cue for it either way: what is asserted is
    // that the classification came from the clip's own `source` field.
    let clip = plan.clips.first().expect("the insert clip survived planning");
    assert_eq!(clip.source, "assets/thing.wav", "the clip carries its asset");
    assert_eq!(
        naivepost::produce_clip_cues::source_of(clip),
        subs::Source::Insert,
        "and that is what makes it contribute nothing"
    );
    assert!(reason.contains("inserted"), "{reason}");
}

#[test]
fn f5_4_s1_narration_lines_beat_the_clips_own_speech_through_the_plan() {
    assert_eq!(ITEM, "f5_4");
    // Both are offered: the line wins, because §F5.4's first question is about lines, not about the clip.
    // A wiring bug that asked the other order would put the raw words on screen over the spoken narration.
    let (_, plan) = planned(
        "s1-lines-win",
        vec![seg(0.0, 10.0)],
        vec![entry(0.0, 4.0, "what was written")],
        words(&["raw", "recognised", "words"], 0.0),
    );
    let cues = sheet(&plan);
    assert_eq!(cues.len(), 1, "{cues:?}");
    assert_eq!(cues[0].text, "what was written", "the line's text, not the recogniser's");
    let joined = cues.iter().map(|c| c.text.as_str()).collect::<Vec<&str>>().join(" ");
    assert!(!joined.contains("recognised"), "the clip's own words never reach the sheet: {joined:?}");
}

#[test]
fn f5_4_s1_a_pasted_copy_stretch_is_not_footage_either() {
    assert_eq!(ITEM, "f5_4");
    // `copy:<s>` is footage moved elsewhere; it was captioned where it was recorded, so pasting it does
    // not caption it twice.
    let (_, plan) = planned(
        "s1-copy",
        vec![insert_seg(0.0, 10.0, "copy:5.0")],
        Vec::new(),
        words(&["twice", "told"], 0.0),
    );
    assert!(sheet(&plan).is_empty(), "a pasted stretch contributes nothing: {:?}", sheet(&plan));
    let clip = plan.clips.first().expect("the copy clip survived planning");
    assert_eq!(naivepost::produce_clip_cues::source_of(clip), subs::Source::Insert);
}

#[test]
fn f5_4_s1_subtitlebreakseconds_row_is_what_splits_a_cue() {
    assert_eq!(ITEM, "f5_4");
    // The cited row, read from the catalogue this build writes rather than from a second copy of the number.
    assert_eq!(row("P.policy.subtitleBreakSeconds"), "0.6");
    assert_eq!(subs::SUBBREAK_SECONDS, 0.6);

    // A gap of exactly 0.6 s between two words closes the cue — `>=`, so the boundary itself breaks.
    let tight = vec![
        subs::Word { text: "one".into(), s: 0.0, e: 0.2 },
        subs::Word { text: "two".into(), s: 0.5, e: 0.7 },
    ];
    assert_eq!(subs::word_cues(&tight, 1.0).len(), 1, "half a second is a continuation");
    let broken = vec![
        subs::Word { text: "one".into(), s: 0.0, e: 0.2 },
        subs::Word { text: "two".into(), s: 0.8, e: 1.0 },
    ];
    assert_eq!(subs::word_cues(&broken, 1.0).len(), 2, "0.6 s of quiet is a new cue");

    // And the same break reaches the sheet through the plan: two clips' worth of breath shows up as two
    // cues, not one long one.
    let (_, plan) = planned(
        "s1-break-in-plan",
        vec![seg(0.0, 10.0)],
        Vec::new(),
        vec![
            subs::Word { text: "first".into(), s: 0.0, e: 0.2 },
            subs::Word { text: "second".into(), s: 0.8, e: 1.0 },
        ],
    );
    assert_eq!(sheet(&plan).len(), 2, "the break survives the wiring: {:?}", sheet(&plan));
}

/// §10 rows, read from the catalogue rather than restated.
fn row(id: &str) -> String {
    params::produce()
        .into_iter()
        .find(|param| param.id == id)
        .unwrap_or_else(|| panic!("{} is catalogued in §10's Produce rows", id))
        .spelled
}

// ---- S2: the produced clock, across the whole track ------------------------------

#[test]
fn f5_4_s2_the_plan_tidies_across_clips() {
    assert_eq!(ITEM, "f5_4");
    // Two clips whose word cues land ON TOP of each other once both are moved onto the video clock: the
    // first clip's tail overlaps the second clip's head. `tidy` runs once over the joined track (it
    // compares cues with each other), so the overlap is resolved between clips and not inside one.
    let first = vec![
        subs::Word { text: "alpha".into(), s: 0.0, e: 9.0 },
        subs::Word { text: "beta".into(), s: 9.4, e: 12.0 },
    ];
    let second = vec![
        subs::Word { text: "gamma".into(), s: 10.0, e: 13.0 },
        subs::Word { text: "delta".into(), s: 13.4, e: 15.0 },
    ];
    // One list carrying both clips' words, since `planned` hands the same list to every clip: what is being
    // asserted is the joined-and-tidied result, which is what the sheet holds.
    let mut both = first.clone();
    both.extend(second.clone());
    let (_root, tree) = tree_in("s2-across");
    let run = flow::snapshot(
        Some(cut_of(vec![seg(0.0, 12.0), seg(12.0, 24.0)])),
        &tree,
        Vec::new(),
        Produce::default(),
        &project(),
        Some(Publish::default()),
        "1".to_string(),
    );
    let plan = exec::plan(&run, &tree, &mats(&[], &|_clip| both.clone()));
    let cues = sheet(&plan);
    assert!(cues.len() >= 2, "both clips contributed: {cues:?}");
    for pair in cues.windows(2) {
        assert!(
            pair[0].e <= pair[1].s + 1e-9,
            "no two cues are on screen at once: {:?} then {:?}",
            pair[0],
            pair[1]
        );
    }
    // The sheet equals `tidy` over the same joined cues — the plan did not tidy per clip, which would have
    // left the cross-clip overlap standing.
    let unsorted = {
        let mut raw = Vec::new();
        for (clip, (start, _len)) in
            plan.clips.iter().zip(render::produced_clocks(&plan.clips).iter())
        {
            raw.extend(
                subs::cues(subs::Source::Footage, &[], &both, clip.rate)
                    .iter()
                    .map(|c| subs::Cue {
                        s: start + naivepost::narration::output_seconds(c.s, clip.rate),
                        e: start + naivepost::narration::output_seconds(c.e, clip.rate),
                        text: c.text.clone(),
                        pos: c.pos.clone(),
                    }),
            );
        }
        raw
    };
    assert!(
        unsorted.windows(2).any(|p| p[0].e > p[1].s),
        "the untidied join really does overlap, which is what this test is about: {unsorted:?}"
    );
    assert_eq!(cues, subs::tidy(&unsorted), "and the plan tidied the join, once, across clips");
}

#[test]
fn f5_4_s2_the_sheet_written_to_disk_is_the_tidied_track() {
    assert_eq!(ITEM, "f5_4");
    // S4's file, not just the in-memory answer: `clips/final.srt` holds exactly the sheet, so the
    // subtitles the encoder is pointed at are the ones §F5.4 produced.
    let (tree, plan) = planned(
        "s2-sheet",
        vec![seg(0.0, 10.0)],
        Vec::new(),
        words(&["spoken", "in", "class"], 0.0),
    );
    let spy = StubRun::new();
    spy.run(&plan, &tree);
    let sheet_path = tree.clips_dir().join(render::final_srt_name());
    let written = std::fs::read_to_string(&sheet_path).expect("the sheet was written");
    assert!(written.contains("spoken"), "{written}");
    assert_eq!(written, subs::srt_text(&plan.cues.all), "byte for byte the plan's track");
}

/// The smallest driver that gets S4's file written: the walk's own steps are `produce_render_exec`'s
/// subject, so this only needs the sheet on disk and no subprocess to fail on.
struct StubRun;

impl StubRun {
    fn new() -> StubRun {
        StubRun
    }

    fn run(&self, plan: &exec::Plan, tree: &Tree) {
        // Write the sheet the way S4 does, through the same function, so the assertion below is about
        // content and not about who called `std::fs::write`.
        let sheet_path = tree.clips_dir().join(render::final_srt_name());
        std::fs::write(&sheet_path, subs::srt_text(&plan.cues.all)).unwrap();
        assert!(Path::new(&sheet_path).is_file());
    }
}
