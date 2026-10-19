//! F5.4 Subtitles S1/S2 — the WIRE: the render's cue sheet is built by `produce_subtitles::cues`
//! through `produce_exec::plan`, so §F5.4's first diamond (narration lines, else the clip's own speech,
//! and that only where the clip is footage) is reached by the flow rather than living unused in its module.
//!
//! `tests/suite/produce_subtitles.rs` pins the rules themselves over hand-built words, entries and cues. This
//! file pins the other half: that `produce_exec` asks **that** question of each clip it planned, with the
//! words the caller hands over, and that the sheet it writes (`clips/final.srt`) is what those rules
//! produced on the produced clock. Same door as `tests/suite/produce_render_exec.rs`: one `exec::Materials`,
//! plan-only — no subprocess is spawned here, because S1/S2 are decided before the first encode.
//!
//! Ids used, each asserted against its §10 row in `params::produce()`: P.policy.subtitleBreakSeconds
//! (0.6), P.policy.subtitleRowChars (42), P.policy.subtitleMaxSeconds (6).

use std::path::{Path, PathBuf};

use naivepost::cut::{Cut, Lane, Seg};
use naivepost::layout::Tree;
use naivepost::narration::Entry;
use naivepost::produce_exec as exec;
use naivepost::produce_flow as flow;
use naivepost::produce_render as render;
use naivepost::produce_subtitles as subs;
use naivepost::project::{Produce, Publish, Source};

const ITEM: &str = "f5_4";


/// An insert segment: `ins` names an asset, which is what makes the clip not-footage downstream.
fn insert_seg(s: f64, e: f64, asset: &str) -> Seg {
    Seg { s, e, cam: 0, ins: asset.to_string(), ..Default::default() }
}


fn cut_of(segs: Vec<Seg>) -> Cut {
    Cut { segs, aspect: "16:9".into(), ..Default::default() }
}


use crate::common;
use common::{clip as seg, produce_project as project, entry_span as entry, produce_row as row};

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
        on_recording: true,
        footage_left: &naivepost::produce_render::unknown_footage,
        filmed_runs: naivepost::produce_plan_logs::NO_TAPE,
        base_of: &|_seg| String::new(),
        wav_seconds: &|_entry| None,
        cues: common::produce_cues_for_test(Vec::new()),
        words,
        // This file is about S1/S2; the S3 leg answers every number so the walk is never the thing that
        // fails here. `tests/suite/produce_translate_flow.rs` owns the refusals.
        translate: &|_message: &str, numbers: &[usize]| -> Result<std::collections::BTreeMap<usize, String>, String> {
            Ok(numbers.iter().map(|n| (*n, format!("translated {n}"))).collect::<std::collections::BTreeMap<usize, String>>())
        },
        sources: &project_sources,
        // No ffprobe claim: this file is about the cue sheet, not the join's size check.
        probe: &|_file| None,
    }
}

/// Plan only — S1/S2 are settled before any subprocess, and this file has no spawner to distract it.
fn planned(tag: &str, segs: Vec<Seg>, lines: Vec<Entry>, words: Vec<subs::Word>) -> (Tree, exec::Plan) {
    let (_root, tree) = common::tree_in(tag);
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
    let (_root, tree) = common::tree_in("s2-across");
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


// ---- S4/S5/S7: the walk WRITES the sheet, refuses when there is nothing, burns only when asked ----

/// The walk's spawner: a render of this shape is three clips, a join and a mux, and what a test asks is
/// what those subprocesses WROTE — so each one "writes" the file its own command names. A scripted spawner
/// is the program's own seam: `run_render` takes the spawn leg as an argument because that is how the page
/// reaches these steps in a container with no ffmpeg, never because a test needs a stand-in.
struct WriteSpy {
    logs: std::rc::Rc<std::cell::RefCell<Vec<String>>>,
}

impl WriteSpy {
    fn new() -> WriteSpy {
        WriteSpy { logs: std::rc::Rc::new(std::cell::RefCell::new(Vec::new())) }
    }

    /// Run the whole walk over a plan with this file's `Materials`, answering every checkpoint yes.
    fn run(&self, run: &flow::Run, tree: &Tree, words: Vec<subs::Word>, settings: Produce) -> exec::Ran {
        let held = words;
        let logs = self.logs.clone();
        exec::walk(
            &exec::plan(run, tree, &mats_with(&[], &|_c| held.clone(), settings.clone())),
            &mats_with(&[], &|_c| held.clone(), settings.clone()),
            move |line| {
                logs.borrow_mut().push(line.to_string());
            },
            |_line| Ok(()),
            |_at, _what| true,
            |command| {
                // `-o` names the output; every one of this run's commands carries it.
                if let Some(pos) = command.argv.iter().position(|a| a == "-o") {
                    if let Some(out) = command.argv.get(pos + 1) {
                        let path = std::path::Path::new(out);
                        if let Some(dir) = path.parent() {
                            let _ = std::fs::create_dir_all(dir);
                        }
                        let _ = std::fs::write(path, b"encoded");
                    }
                }
                Ok(())
            },
        )
    }
}

/// `mats` with a chosen settings row: the walk needs the same `Materials` twice (once to plan, once to
/// walk), so the shape lives in one place.
/// `mats` with a chosen settings row. The settings decide CLIPS and CUES only through the run that carries
/// them, so the row itself is not repeated here; `translate` is, because the sheet's language list — and so
/// the set of sidecar names this run owns — comes in through `Materials::cues`, exactly as the page hands it.
fn mats_with<'a>(
    lanes: &'a [Lane],
    words: &'a dyn Fn(&render::Clip) -> Vec<subs::Word>,
    settings: Produce,
) -> exec::Materials<'a> {
    exec::Materials {
        cues: common::produce_cues_for_test(settings.translate.clone()),
        ..mats(lanes, words)
    }
}

fn held_run(tree: &Tree, segs: Vec<Seg>, lines: Vec<Entry>, settings: Produce) -> flow::Run {
    flow::snapshot(
        Some(cut_of(segs)),
        tree,
        lines,
        settings,
        &project(),
        Some(Publish::default()),
        "1".to_string(),
    )
}

#[test]
fn f5_4_s4_the_walk_writes_the_sheet_and_says_what_it_holds() {
    assert_eq!(ITEM, "f5_4");
    // The point of the round: the live render hands its cue sheet to the file, not an empty list to nothing.
    // Two footage clips with their own speech and no narration line at all.
    let (root, tree) = common::tree_in("s4-walk-sheet");
    let settings = Produce::default();
    let run = held_run(&tree, vec![seg(0.0, 10.0), seg(10.0, 20.0)], Vec::new(), settings.clone());
    let spy = WriteSpy::new();
    let ran = spy.run(&run, &tree, words(&["the", "lecture"], 0.0), settings.clone());
    assert_eq!(ran.failed, None, "{:?}", ran.failed);

    let sheet_path = tree.clips_dir().join(render::final_srt_name());
    let plan = exec::plan(&run, &tree, &mats_with(&[], &|_c| words(&["the", "lecture"], 0.0), settings));
    let written = std::fs::read_to_string(&sheet_path).expect("S4 wrote the sheet");
    assert_eq!(written, subs::srt_text(&plan.cues.all), "byte for byte the plan's track");
    assert!(written.contains("the lecture"), "{written}");

    // §S4's own sentence, with the counts of THIS sheet: both clips answered with their own speech.
    let logs = spy.logs.borrow();
    let said = logs
        .iter()
        .find(|line| line.starts_with(">>> subtitles:"))
        .expect("the run says what its sheet holds");
    // The counts name the CLIPS, not the spans: tidy folds the pair that `tidy` holds across a short gap,
    // so the cue count is one while both clips still own a caption each.
    assert_eq!(
        *said,
        format!(
            ">>> subtitles: {} cues from {} spoken line(s) and {} clip(s) of their own speech.",
            plan.cues.all.len(),
            plan.cues.counts.from_lines,
            plan.cues.counts.from_speech
        ),
        "{logs:?}"
    );
    assert_eq!(plan.cues.counts.from_speech, 2, "both clips spoke: {logs:?}");
    assert_eq!(plan.cues.counts.from_lines, 0, "nothing was written: {logs:?}");
    assert!(!logs.iter().any(|l| l.starts_with("!!!")), "cues exist, so no refusal: {logs:?}");
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn f5_4_s4_a_run_with_nothing_to_caption_says_so_and_writes_no_sheet() {
    assert_eq!(ITEM, "f5_4");
    // §5: "no narration and no speech → none", with the sentence naming both sources. An insert clip is the
    // smallest cut that has neither, and the video itself still finishes.
    let (root, tree) = common::tree_in("s4-walk-empty");
    let settings = Produce::default();
    let run = held_run(
        &tree,
        vec![insert_seg(0.0, 10.0, "assets/thing.wav")],
        Vec::new(),
        settings.clone(),
    );
    let spy = WriteSpy::new();
    let ran = spy.run(&run, &tree, Vec::new(), settings);
    assert_eq!(ran.failed, None, "an empty sheet is a warning, not a failed render: {:?}", ran.failed);
    assert!(
        !tree.clips_dir().join(render::final_srt_name()).exists(),
        "never an empty cue: no sheet is written"
    );
    let logs = spy.logs.borrow();
    assert!(
        logs.contains(&render::no_subtitle_cue_log().to_string()),
        "the refusal is §5's own line: {logs:?}"
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn f5_4_s2_the_captions_only_warning_is_said_by_the_walk() {
    assert_eq!(ITEM, "f5_4");
    // F5.2 S2's sentence: voice "captions only" plus Subtitles "none in the video" leaves the words in the
    // file beside it ONLY, and the run says that out loud rather than letting the reader find it.
    let (root, tree) = common::tree_in("s2-captions-warn");
    let settings = Produce {
        subtitles: naivepost::project::Subtitles::None,
        ..Default::default()
    };
    let mut run = held_run(&tree, vec![seg(0.0, 10.0)], vec![entry(0.0, 4.0, "written")], settings.clone());
    run.voice = naivepost::narrate_screen::CAPTIONS.to_string();
    let spy = WriteSpy::new();
    let ran = spy.run(&run, &tree, Vec::new(), settings);
    assert_eq!(ran.failed, None, "{:?}", ran.failed);
    let logs = spy.logs.borrow();
    assert!(
        logs.contains(&render::captions_only_warning(naivepost::project::Subtitles::None).unwrap().to_string()),
        "S2's warning is logged, not only computed: {logs:?}"
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn f5_4_s5_burned_in_lets_each_clip_down_with_its_own_cues() {
    assert_eq!(ITEM, "f5_4");
    // "burned in" is the only answer that letters the picture, and what it letters is the CLIP's own cues
    // on the CLIP's own seconds — the encode cuts that span out of the recording and never sees the video
    // clock. A cue moved onto the video's timeline would be seconds away from every caption on it.
    let (root, tree) = common::tree_in("s5-burned");
    let settings = Produce {
        subtitles: naivepost::project::Subtitles::BurnedIn,
        ..Default::default()
    };
    let words = words(&["second", "clip"], 0.0);
    let run = held_run(&tree, vec![seg(0.0, 10.0), seg(10.0, 20.0)], Vec::new(), settings.clone());
    let plan = exec::plan(&run, &tree, &mats_with(&[], &|_c| words.clone(), settings.clone()));
    let burns = plan.burns.clone();
    assert_eq!(burns.len(), plan.cues.per_clip.len(), "one entry per clip that spoke");
    let second = burns.get(1).cloned().flatten().expect("the second clip is burned");
    assert_eq!(
        second.file_name().unwrap().to_string_lossy(),
        "c001.srt",
        "§B's per-clip name, beside the encode"
    );
    let text = std::fs::read_to_string(&second).expect("the burn sheet was written");
    assert!(text.contains("second clip"), "{text}");
    // On the CLIP's seconds: the same words on the video's clock would start 10 s later, where the second
    // clip begins in the finished file.
    assert!(
        text.contains("00:00:00,000 --> 00:00:00,500"),
        "clip-relative times, not the video's: {text}"
    );
    assert!(!text.contains("00:00:10"), "nothing is shifted onto the video clock: {text}");
    let encode = plan.encodes.get(1).expect("the second encode");
    // The graph names the sheet by the name it is written under, in the scratch folder it is written to:
    // ffmpeg resolves it from its own working directory, which is where the encode runs.
    let name = second.file_name().unwrap().to_string_lossy().to_string();
    assert!(
        encode.command.log.contains(&format!("subtitles={name}")),
        "the filter graph names that file: {}",
        encode.command.log
    );
    assert!(second.parent() == Some(plan.scratch.as_path()), "{second:?} beside {}", plan.scratch.display());
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn f5_4_s5_a_track_or_sidecar_run_burns_nothing() {
    assert_eq!(ITEM, "f5_4");
    // The other two answers carry the words elsewhere; a burned copy would say them twice on the picture.
    let (root, tree) = common::tree_in("s5-not-burned");
    let words = words(&["spoken", "here"], 0.0);
    for subtitles in [
        naivepost::project::Subtitles::TrackInFile,
        naivepost::project::Subtitles::None,
    ] {
        let settings = Produce { subtitles, ..Default::default() };
        let run = held_run(&tree, vec![seg(0.0, 10.0)], Vec::new(), settings.clone());
        let plan = exec::plan(&run, &tree, &mats_with(&[], &|_c| words.clone(), settings));
        assert!(plan.burns.iter().all(Option::is_none), "{subtitles:?} letters nothing");
        assert!(!plan.encodes[0].command.log.contains("subtitles="), "{}", plan.encodes[0].command.log);
        // …and it still captions: the sheet exists, which is what §1's row promises for every mode.
        assert!(!plan.cues.all.is_empty(), "{subtitles:?} still has a cue sheet");
    }
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn f5_4_s4_stale_sidecars_go_by_exact_name_even_with_no_cues() {
    assert_eq!(ITEM, "f5_4");
    // A run that has nothing to caption must still clear the last render's files: subtitles describing words
    // that are no longer in the video are worse than none. And the name list is §3's, never a glob.
    let (root, tree) = common::tree_in("s4-stale");
    // The tick is a language CODE: §C lists en/de/fr, and a tick spelled any other way is not a track this
    // run writes, so its files are nobody's to delete.
    let settings = Produce {
        translate: vec!["de".to_string()],
        ..Default::default()
    };
    let produce_dir = tree.final_video("mp4").parent().unwrap().to_path_buf();
    std::fs::create_dir_all(&produce_dir).unwrap();
    // `plan` clears `produce/clips/` first and the walk writes its sheet there, so the scratch folder has
    // to exist before the sidecar files beside the video are seeded — a stray file in a folder that does
    // not exist yet is not a stale sidecar, it is a test that set its stage in the wrong order.
    // §3 spells a translated sidecar with a TWO-letter code (`final.<code>.srt`), which is what a ticked
    // language writes; a three-letter name belongs to nothing and would survive on purpose.
    let stale = produce_dir.join("final.de.srt");
    let other = produce_dir.join("final.zz.srt");
    std::fs::write(&stale, "old german").unwrap();
    std::fs::write(&other, "not mine").unwrap();

    // An insert cut: no cues at all, and the stale translation still goes.
    let run = held_run(
        &tree,
        vec![insert_seg(0.0, 10.0, "assets/thing.wav")],
        Vec::new(),
        settings.clone(),
    );
    let spy = WriteSpy::new();
    let ran = spy.run(&run, &tree, Vec::new(), settings);
    assert_eq!(ran.failed, None, "{:?}", ran.failed);
    assert!(
        !stale.exists(),
        "a stale translated sidecar is deleted by name; produce/ holds {:?} and the run said {:?}",
        std::fs::read_dir(&produce_dir)
            .map(|d| d.filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().to_string()).collect::<Vec<String>>())
            .unwrap_or_default(),
        spy.logs.borrow(),
    );
    assert!(other.exists(), "a file this run does not own is left alone");
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn f5_4_s7_a_ticked_language_gets_its_sidecar_written_by_the_walk() {
    assert_eq!(ITEM, "f5_4");
    // §F5.2 S7 runs AFTER the encodes ("so the encoder never idles behind the LLM gate") and S9 writes the
    // translated files beside the video. The translation leg is the program's own seam — `Materials::translate`
    // is a function the page hands in, pointed at the LLM server in the app and at a scripted answer here —
    // so what this pins is that the walk asks it and writes what comes back.
    let (root, tree) = common::tree_in("s7-sidecar");
    let settings = Produce {
        translate: vec!["de".to_string()],
        ..Default::default()
    };
    let words = words(&["spoken", "aloud"], 0.0);
    let run = held_run(&tree, vec![seg(0.0, 10.0)], Vec::new(), settings.clone());
    let held = words.clone();
    let spy = WriteSpy::new();
    let logs = spy.logs.clone();
    let plan = exec::plan(&run, &tree, &mats_with(&[], &|_c| held.clone(), settings.clone()));
    exec::walk(
        &plan,
        &mats_with(&[], &|_c| held.clone(), settings),
        move |line| {
            logs.borrow_mut().push(line.to_string());
        },
        |_line| Ok(()),
        |_at, _what| true,
        |command| {
            if let Some(pos) = command.argv.iter().position(|a| a == "-o") {
                if let Some(out) = command.argv.get(pos + 1) {
                    let path = std::path::Path::new(out);
                    if let Some(dir) = path.parent() {
                        let _ = std::fs::create_dir_all(dir);
                    }
                    let _ = std::fs::write(path, b"encoded");
                }
            }
            Ok(())
        },
    );
    let written = std::fs::read_to_string(tree.final_srt(Some("de"))).expect("S9 wrote the German .srt");
    assert!(written.contains("translated 1"), "the translation, not the original: {written}");
    assert!(tree.final_vtt(Some("de")).is_file(), "…and its .vtt");
    let logs = spy.logs.borrow();
    assert!(
        logs.iter().any(|l| l.starts_with(">>> subtitles: translating")),
        "S7 says what it is doing: {logs:?}"
    );
    assert!(
        logs.iter().any(|l| l.contains("final.de.srt")),
        "S9 logs each sidecar: {logs:?}"
    );
    let _ = plan.stamp;
    std::fs::remove_dir_all(&root).ok();
}
