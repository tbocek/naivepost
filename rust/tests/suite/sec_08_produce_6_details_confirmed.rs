//! §08-produce#6-details-confirmed-against-the-code (verification pass), bullet 1: the CLIP PLANNING
//! refusals and log lines §6 spells are said by the render itself.
//!
//! Before this round every one of those wordings sat spelled-out in `produce_details` with nothing reaching
//! them, so a person reading a render log could not tell a clip the plan refused from a clip that never
//! existed. The sentences arrive through the real plan: each check drives `produce_exec::plan` over a scratch
//! session and reads `Plan::plan_logs`, which is the list `walk` prints and `Ran::logs` hands back — so what
//! is asserted here is the line a run actually says, in the order the clips play.
//!
//! Determinism: no socket, no clock. Recordings are made here (a `Recording` is a span plus a name), the
//! session folder is a scratch dir, and the "take exists" door is the program's own `wav_of` door pointed at a
//! file this test writes or does not write.

use naivepost::cut::{Fx, Seg};
use naivepost::narration::Entry;
use naivepost::produce_details as details;
use naivepost::produce_exec as exec;
use naivepost::produce_flow as flow;
use naivepost::produce_plan_logs as plan_logs;
use naivepost::produce_render as render;
use naivepost::project::{Produce, Publish};

const ITEM: &str = "§08-produce#6-details-confirmed-against-the-code";

use crate::common;
use common::produce_project as project;

/// Tape that covers the whole session, so a check about "no recording" can take it away.
const TAPE: &[(f64, f64)] = &[(0.0, 600.0)];

fn seg(s: f64, e: f64) -> Seg {
    Seg {
        s,
        e,
        ..Default::default()
    }
}

fn copy_seg(s: f64, e: f64) -> Seg {
    Seg {
        s,
        e,
        ins: format!("copy:{}", e - s),
        ..Default::default()
    }
}

fn card_seg(s: f64, e: f64) -> Seg {
    Seg {
        s,
        e,
        ins: "project:media/card.png".to_string(),
        ..Default::default()
    }
}

fn line(s: f64, e: f64, text: &str) -> Entry {
    Entry {
        s,
        e,
        text: text.to_string(),
        ..Default::default()
    }
}

fn zoom_at(t: f64) -> Fx {
    // A rectangle of its own, so the camera really travels: a zoom that lands where the settled frame already
    // is moves nothing and the render correctly says no word about a fixed grid.
    Fx {
        kind: "zoom".to_string(),
        t,
        dur: 2.0,
        cx: Some(0.3),
        cy: Some(0.4),
        hf: Some(0.5),
        ..Default::default()
    }
}

fn stop_at(t: f64) -> Fx {
    Fx {
        kind: "speed".to_string(),
        t,
        dur: 1.0,
        rate: 0.0,
        snd: "mute".to_string(),
        ..Default::default()
    }
}

/// One planned run's log lines, over the tape the check names. `runs` is the filmed spans — the same list the
/// produce page answers `footage_left` from — and `fx` the cut's own effect list.
struct Planned {
    logs: Vec<String>,
}

fn planned(
    tag: &str,
    segs: Vec<Seg>,
    mut fx: Vec<Fx>,
    lines: Vec<Entry>,
    runs: &[(f64, f64)],
    take_on_disk: bool,
) -> Planned {
    let (root, tree) = common::tree_in(tag);
    // A card the plan can find: §6's insert sentences are about a card that EXISTS and has nothing under it,
    // and `plan_clips` drops a missing one with its own line first, so the asset has to be on disk here.
    let asset = root.join("card.png");
    std::fs::write(&asset, b"fake png").expect("a card can be staged");
    // The take door is the program's own: a real file on disk, or nothing to find.
    let take = if take_on_disk {
        Some(root.join("take.wav"))
    } else {
        None
    };
    if let Some(path) = &take {
        std::fs::write(path, b"RIFFfake").expect("a take can be staged");
    }
    let cut = naivepost::cut::Cut {
        segs,
        fx: fx.clone(),
        ..Default::default()
    };
    // The one door `plan_clips` and this walk both ask per segment; the cut carries the same list, so the
    // render's own answer and the planning answer are the same list, not two opinions about it.
    fx.sort_by(|a, b| a.t.total_cmp(&b.t));
    let fx_for = move |_seg: &Seg| fx.clone();
    let run = flow::snapshot(
        Some(cut),
        &tree,
        lines,
        Produce::default(),
        &project(),
        Some(Publish::default()),
        "1".to_string(),
    );
    let wav_of = move |_entry: &Entry| take.clone();
    let exists =
        move |file: &str| file.ends_with("card.png") || std::path::Path::new(file).exists();
    // The same arithmetic the produce page's own `footage_left` door answers with: the run covering the
    // segment's start, less the segment's end — so each clip gets its OWN leftover, and a clip no run covers
    // gets the page's "I do not know" (0.0), which is never read as a shortening.
    let left = move |seg: &Seg| {
        runs.iter()
            .filter(|(start, _end)| *start <= seg.s)
            .map(|(_start, end)| *end)
            .max_by(f64::total_cmp)
            .map_or(0.0, |run_end| (run_end - seg.e).max(0.0))
    };
    let mats = exec::Materials {
        exists: &exists,
        fx_for: &fx_for,
        lanes: &[] as &[naivepost::cut::Lane],
        src_shape: (1920, 1080),
        wav_of: &wav_of,
        wav_seconds: &|_entry| None,
        footage_left: &left,
        source_file: &|clip| Some(("project:media/in.mp4".to_string(), clip.seg_s)),
        on_recording: !runs.is_empty(),
        cues: exec::Cues::default(),
        sources: &project_sources,
        words: &|_clip| Vec::new(),
        translate: static_refuse(),
        probe: &|_file| None,
        filmed_runs: runs,
        // The asset door answers with the staged card's real path, which is what §6's insert sentences read.
        base_of: &move |seg: &Seg| {
            if seg.ins.is_empty() || seg.ins.starts_with("copy:") {
                "cam0.mp4".to_string()
            } else {
                naivepost::cut_insert::base_name(&asset.to_string_lossy())
            }
        },
    };
    let plan = exec::plan(&run, &tree, &mats);
    Planned {
        logs: plan.plan_logs,
    }
}

fn project_sources() -> Vec<naivepost::project::Source> {
    project().sources
}

/// A translation leg nothing here asks: the plan never dials it, the walk does, and these checks stop at the
/// plan. Leaked once for the process, which is what `Materials::translate`' `&'static` shape asks for.
fn static_refuse() -> &'static naivepost::produce_translate::Ask {
    Box::leak(Box::new(|_m: &str, _n: &[usize]| {
        Err("a planning check never translates".to_string())
    }))
}

/// The clip-planning lines only: `plan_clips` also says its own fit logs, and the checks below read what
/// §6's sentences look like, so everything else is filtered out by the detail module's own wording.
fn says(logs: &[String], line: &str) -> bool {
    logs.iter().any(|l| l == line)
}

#[test]
fn sec_08_produce_6_details_confirmed_against_the_code_verification_pass_s1_copy_in_no_recording_is_said(
) {
    assert_eq!(ITEM, "§08-produce#6-details-confirmed-against-the-code");
    let got = planned(
        "p6-s1-copy-nowhere",
        vec![copy_seg(10.0, 20.0)],
        Vec::new(),
        Vec::new(),
        // Tape, but nowhere near the paste's seconds.
        &[(400.0, 500.0)],
        false,
    );
    assert!(
        says(&got.logs, &details::copy_in_no_recording_log(1, 10.0)),
        "the paste's refusal is in the plan's own lines: {:?}",
        got.logs
    );
}

#[test]
fn sec_08_produce_6_details_confirmed_against_the_code_verification_pass_s1_copy_past_the_end_is_shortened_out_loud(
) {
    let got = planned(
        "p6-s1-copy-past",
        vec![copy_seg(10.0, 30.0)],
        Vec::new(),
        Vec::new(),
        // The tape stops 6.251 s into the paste, so that is the length §6 says it keeps.
        &[(0.0, 16.251)],
        false,
    );
    assert!(
        says(&got.logs, &details::copy_past_end_log(1, "cam0.mp4", 6.251)),
        "{:?}",
        got.logs
    );
}

#[test]
fn sec_08_produce_6_details_confirmed_against_the_code_verification_pass_s1_sounds_on_no_recording_and_past_the_end(
) {
    let nowhere = planned(
        "p6-s1-sound-nowhere",
        vec![seg(120.0, 130.0)],
        Vec::new(),
        Vec::new(),
        &[(0.0, 100.0)],
        false,
    );
    assert!(
        says(&nowhere.logs, &details::sound_in_no_recording_log(1, 120.0)),
        "{:?}",
        nowhere.logs
    );
    let short = planned(
        "p6-s1-sound-past",
        vec![seg(10.0, 40.0)],
        Vec::new(),
        Vec::new(),
        // Tape that stops 12 s into this clip: 12 s is what the run kept.
        &[(0.0, 22.0)],
        false,
    );
    assert!(
        says(
            &short.logs,
            &details::sound_past_end_log(1, "cam0.mp4", 12.0)
        ),
        "{:?}",
        short.logs
    );
    // Tape that runs on 12 s past the clip: nothing ran out, so nothing is said. The first rewrite said
    // "shortened to 12 s" here, of every clip longer than the tape left after it.
    let covered = planned(
        "p6-s1-sound-covered",
        vec![seg(10.0, 40.0)],
        Vec::new(),
        Vec::new(),
        &[(0.0, 52.0)],
        false,
    );
    assert!(
        !covered.logs.iter().any(|l| l.contains("past the end")),
        "{:?}",
        covered.logs
    );
}

#[test]
fn sec_08_produce_6_details_confirmed_against_the_code_verification_pass_s1_an_insert_with_nothing_under_it_plays_silent(
) {
    let got = planned(
        "p6-s1-silent",
        vec![card_seg(10.0, 14.0)],
        Vec::new(),
        Vec::new(),
        // No tape at all under the card: what it keeps is silence.
        plan_logs::NO_TAPE,
        false,
    );
    assert!(
        says(&got.logs, &details::insert_plays_silent_log(1)),
        "{:?}",
        got.logs
    );
}

#[test]
fn sec_08_produce_6_details_confirmed_against_the_code_verification_pass_s1_a_stop_on_no_recording_loses_its_still(
) {
    let got = planned(
        "p6-s1-stop",
        vec![seg(200.0, 210.0)],
        vec![stop_at(205.0)],
        Vec::new(),
        &[(0.0, 100.0)],
        false,
    );
    assert!(
        says(&got.logs, &details::stop_in_no_recording_log(205.0)),
        "{:?}",
        got.logs
    );
    // The same stop with tape under it says nothing: the still has a frame to hold.
    let on_tape = planned(
        "p6-s1-stop-tape",
        vec![seg(200.0, 210.0)],
        vec![stop_at(205.0)],
        Vec::new(),
        TAPE,
        false,
    );
    assert!(
        !says(&on_tape.logs, &details::stop_in_no_recording_log(205.0)),
        "a stop with tape keeps its still: {:?}",
        on_tape.logs
    );
}

#[test]
fn sec_08_produce_6_details_confirmed_against_the_code_verification_pass_s1_a_card_with_no_line_keeps_its_own_audio(
) {
    // §6's exact-bounds half: the line carries the CARD's bounds, so nothing was ever written for it.
    let got = planned(
        "p6-s1-no-entry",
        vec![card_seg(30.0, 36.0)],
        Vec::new(),
        vec![line(40.0, 46.0, "")],
        TAPE,
        false,
    );
    // An empty line is silence by choice and counts as nothing written for the clip.
    assert!(
        says(&got.logs, &details::no_entry_keeps_own_audio_log(1, 30.0)),
        "{:?}",
        got.logs
    );
}

#[test]
fn sec_08_produce_6_details_confirmed_against_the_code_verification_pass_s1_a_line_with_no_take_is_captioned_only(
) {
    let spoken = planned(
        "p6-s1-take",
        vec![seg(10.0, 15.0)],
        Vec::new(),
        vec![line(10.0, 14.0, "a line with a take")],
        TAPE,
        true,
    );
    assert!(
        !says(&spoken.logs, &details::captioned_only_log(1)),
        "a spoken line is not §6's captioned-only case: {:?}",
        spoken.logs
    );
    let voiceless = planned(
        "p6-s1-no-take",
        vec![seg(10.0, 15.0)],
        Vec::new(),
        vec![line(10.0, 14.0, "a line with no take")],
        TAPE,
        false,
    );
    assert!(
        says(&voiceless.logs, &details::captioned_only_log(1)),
        "{:?}",
        voiceless.logs
    );
}

#[test]
fn sec_08_produce_6_details_confirmed_against_the_code_verification_pass_s1_a_moving_camera_names_its_fixed_grid(
) {
    let got = planned(
        "p6-s1-camera",
        vec![seg(0.0, 20.0)],
        vec![zoom_at(5.0)],
        Vec::new(),
        TAPE,
        false,
    );
    assert!(
        says(
            &got.logs,
            &details::fixed_rate_log("clip 1", details::fixed_rate())
        ),
        "{:?}",
        got.logs
    );
    // A parked camera (no zoom at all) says nothing: only a moving one needs the grid.
    let still = planned(
        "p6-s1-no-camera",
        vec![seg(0.0, 20.0)],
        Vec::new(),
        Vec::new(),
        TAPE,
        false,
    );
    assert!(
        !says(
            &still.logs,
            &details::fixed_rate_log("clip 1", details::fixed_rate())
        ),
        "{:?}",
        still.logs
    );
}

#[test]
fn sec_08_produce_6_details_confirmed_against_the_code_verification_pass_s1_clip_numbers_follow_the_same_list_as_every_other_line(
) {
    // Three clips, the second one the one that runs out of tape: the number in the line has to be 2, the
    // same number the Cut page's rows and the plan's other lines use.
    let got = planned(
        "p6-s1-numbers",
        vec![seg(0.0, 10.0), seg(10.0, 40.0), seg(40.0, 50.0)],
        Vec::new(),
        Vec::new(),
        // The first run stops at 15 s and the next starts at 40 s: only clip 2 (10→40) starts inside a run AND
        // runs past its end, so it is the one clip §6 says a word about, and the number in the line is 2.
        &[(0.0, 15.0), (40.0, 50.0)],
        false,
    );
    assert!(
        says(&got.logs, &details::sound_past_end_log(2, "cam0.mp4", 5.0)),
        "{:?}",
        got.logs
    );
    // The `probe`/`plan_clips` path also keeps its own numbering: clip 2 is the only one named.
    let named = got.logs.iter().filter(|l| l.contains("cam0.mp4")).count();
    assert_eq!(
        named, 1,
        "one refusal for the one clip that runs out: {:?}",
        got.logs
    );
}

#[test]
fn sec_08_produce_6_details_confirmed_against_the_code_verification_pass_s1_a_walk_prints_these_lines_too(
) {
    // The wire from the plan to the log: `walk` prints `Plan::plan_logs` and `Ran::logs` carries them back,
    // so the sentence a person reads after ▶ is the one asserted above.
    let (root, tree) = common::tree_in("p6-s1-walk");
    let _ = root;
    let cut = naivepost::cut::Cut {
        segs: vec![copy_seg(500.0, 520.0)],
        ..Default::default()
    };
    let run = flow::snapshot(
        Some(cut),
        &tree,
        Vec::new(),
        Produce::default(),
        &project(),
        Some(Publish::default()),
        "1".to_string(),
    );
    let mats = exec::Materials {
        exists: &|_f| true,
        fx_for: &|_s: &Seg| Vec::new(),
        lanes: &[] as &[naivepost::cut::Lane],
        src_shape: (1920, 1080),
        wav_of: &|_e| None,
        wav_seconds: &|_e| None,
        footage_left: &render::unknown_footage,
        source_file: &|c| Some(("project:media/in.mp4".to_string(), c.seg_s)),
        on_recording: false,
        cues: exec::Cues::default(),
        sources: &project_sources,
        words: &|_c| Vec::new(),
        translate: static_refuse(),
        probe: &|_f| None,
        filmed_runs: plan_logs::NO_TAPE,
        base_of: &|_s| "cam0.mp4".to_string(),
    };
    let plan = exec::plan(&run, &tree, &mats);
    let refused = details::copy_in_no_recording_log(1, 500.0);
    assert!(plan.plan_logs.contains(&refused), "{:?}", plan.plan_logs);
    let sink: std::cell::RefCell<Vec<String>> = std::cell::RefCell::new(Vec::new());
    let ran = exec::walk(
        &plan,
        &mats,
        |line: &str| sink.borrow_mut().push(line.to_string()),
        |_l| Ok(()),
        |_at, _what| true,
        |_cmd| Ok(()),
    );
    assert!(
        ran.logs.contains(&refused) || sink.borrow().contains(&refused),
        "the walk prints the plan's own lines: {:?}",
        ran.logs
    );
}
