//! §04-prepare#4-parameters-used — spec/04-prepare.md's list of what Prepare may be tuned by, checked
//! against [`naivepost::params`]: every id present, each value the one the code actually reads.
//!
//! The catalogue holds no numbers, so what these tests pin is that the two cannot drift: a row's spelling
//! comes from the constant its rule uses, and §10's row for the same id says the same thing.

use std::fs;
use std::path::{Path, PathBuf};

use naivepost::asr::{self, Answer};
use naivepost::layout::Tree;
use naivepost::params;
use naivepost::tools::{describe, retakes, textedit};
use naivepost::{align, degraded, frames, hand_edit, prepare, requests, roles, separate, transcribe};

/// A project folder of §1's shape: the scratch paths are only interesting under a real `.naivepost` tree.
fn tree(tag: &str) -> Tree {
    let dir = PathBuf::from(format!("/tmp/naivepost-params-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("inputs")).unwrap();
    Tree::new(dir.join("talk.naivepost")).expect("a .naivepost folder is a project")
}

/// The catalogue entry for one §10 id, spelled the way §10 spells it.
fn spelled(id: &str) -> String {
    params::find(id)
        .unwrap_or_else(|| panic!("{id} is in §4's list and so must be catalogued"))
        .spelled
}

/// The value of one entry, read as a number — for the rows this test compares against a constant.
fn number(id: &str) -> f64 {
    spelled(id).parse().unwrap_or_else(|_| panic!("{id} spells a number"))
}

// --- S1: §4's own eighteen, by id and spelling -----------------------------------------------------

#[test]
fn sec_04_prepare_4_parameters_used_s1_every_parameter_the_section_names_is_catalogued() {
    // §4 names these eighteen by hand; the order here is the order it names them in.
    let named = [
        "P.policy.minTakeSeconds",
        "P.machine.retakePauseSeconds",
        "P.machine.retakeRuns",
        "P.machine.retakeReachSeconds",
        "P.machine.retakeMinSeconds",
        "P.machine.retakeCeil",
        "P.machine.retakeFragmentSeconds",
        "P.machine.againReachSeconds",
        "P.machine.seamReachWords",
        "P.machine.seamMaxWords",
        "P.machine.seamCeil",
        "P.machine.seamSnapWords",
        "P.machine.seamNoiseWords",
        "P.machine.describeFramesPerReq",
        "P.machine.describeRecentEvents",
        "P.machine.describeCtxSegs",
        "P.machine.describeCtxWindowSeconds",
        "P.machine.fixBlockLines",
    ];
    let ids: Vec<&str> = params::prepare().into_iter().map(|param| param.id).collect();
    for id in named {
        assert!(ids.contains(&id), "{id} missing from the catalogue");
    }
    // In order, and no row twice — a duplicate would mean two homes for one bound.
    let first: Vec<&str> = named.iter().copied().collect();
    assert_eq!(&ids[..named.len()], &first[..]);
    let unique: std::collections::HashSet<&str> = ids.iter().copied().collect();
    assert_eq!(unique.len(), ids.len(), "one row per parameter");

    // §10's own spellings of the values, quoted from its rows.
    let rows = [
        ("P.machine.retakePauseSeconds", "1.5"),
        ("P.machine.retakeRuns", "3"),
        ("P.machine.retakeReachSeconds", "180"),
        ("P.machine.retakeMinSeconds", "0.3"),
        ("P.machine.retakeCeil", "0.4"),
        ("P.machine.retakeFragmentSeconds", "6"),
        ("P.machine.againReachSeconds", "25"),
        ("P.machine.seamReachWords", "140"),
        ("P.machine.seamMaxWords", "40"),
        ("P.machine.seamCeil", "0.6"),
        ("P.machine.seamSnapWords", "3"),
        ("P.machine.seamNoiseWords", "2"),
        ("P.machine.describeFramesPerReq", "4"),
        ("P.machine.describeRecentEvents", "3"),
        ("P.machine.describeCtxSegs", "2"),
        ("P.machine.describeCtxWindowSeconds", "10"),
        ("P.machine.fixBlockLines", "25"),
        ("P.policy.minTakeSeconds", "2"),
    ];
    for (id, want) in rows {
        assert_eq!(spelled(id), want, "{id}");
    }

    // And every row says where it came from, since that is the whole point of the list.
    for param in params::prepare() {
        assert!(!param.from.is_empty(), "{} names no constant", param.id);
        assert!(
            param.from.contains("::"),
            "{}: {} is not module::CONST",
            param.id,
            param.from
        );
    }

    // An id Prepare never reads is simply not there.
    assert_eq!(params::find("P.policy.deadAirMaxSeconds"), None);
    assert_eq!(params::family("P.policy.markingPass"), params::Family::Policy);
    assert_eq!(params::family("P.machine.slots"), params::Family::Machine);
    assert_eq!(params::family("P.eng.llmAttempts"), params::Family::Eng);
    // The JPEG quality is §10's unlisted engineering value, so it claims no `P.` prefix.
    assert_eq!(params::family("machine.jpegQuality"), params::Family::Other);
}

// --- S2: the retake family --------------------------------------------------------------------------

#[test]
fn sec_04_prepare_4_parameters_used_s2_the_retake_numbers_are_the_ones_the_pass_uses() {
    // P.policy.minTakeSeconds — a shorter recording is a start/stop, not a take.
    assert_eq!(number("P.policy.minTakeSeconds"), requests::MIN_TAKE_SECONDS);
    assert_eq!(spelled("P.policy.minTakeSeconds"), "2");

    // P.machine.retakePauseSeconds / retakeRuns / retakeReachSeconds / retakeMinSeconds / retakeCeil /
    // retakeFragmentSeconds / againReachSeconds.
    assert_eq!(number("P.machine.retakePauseSeconds"), retakes::RETAKE_PAUSE_SECONDS);
    assert_eq!(spelled("P.machine.retakeRuns"), roles::RETAKE_RUNS_POOLED.to_string());
    assert_eq!(number("P.machine.retakeReachSeconds"), retakes::RETAKE_REACH_SECONDS);
    assert_eq!(number("P.machine.retakeMinSeconds"), retakes::RETAKE_MIN_SECONDS);
    assert_eq!(number("P.machine.retakeCeil"), retakes::RETAKE_CEIL);
    assert_eq!(number("P.machine.retakeFragmentSeconds"), retakes::RETAKE_FRAGMENT_SECONDS);
    assert_eq!(number("P.machine.againReachSeconds"), retakes::AGAIN_REACH_SECONDS);

    // The ceiling is a share, not seconds: it has to stay under one or every mark would be refused.
    assert!(retakes::RETAKE_CEIL < 1.0);
    // A fragment is longer than the shortest thing that counts as an attempt at all.
    assert!(retakes::RETAKE_FRAGMENT_SECONDS > retakes::RETAKE_MIN_SECONDS);
    // And a replacement is found within the reach, so the fragment rule can never out-reach it.
    assert!(retakes::AGAIN_REACH_SECONDS < retakes::RETAKE_REACH_SECONDS);
}

// --- S3: the seam family ---------------------------------------------------------------------------

#[test]
fn sec_04_prepare_4_parameters_used_s3_the_seam_numbers_are_the_join_pass_owns() {
    // P.machine.seamReachWords / seamMaxWords / seamCeil / seamSnapWords / seamNoiseWords.
    assert_eq!(textedit::SEAM_REACH_WORDS, 140);
    assert_eq!(spelled("P.machine.seamReachWords"), textedit::SEAM_REACH_WORDS.to_string());
    assert_eq!(spelled("P.machine.seamMaxWords"), textedit::SEAM_MAX_WORDS.to_string());
    assert_eq!(number("P.machine.seamCeil"), textedit::SEAM_CEIL);
    assert_eq!(spelled("P.machine.seamSnapWords"), textedit::SEAM_SNAP_WORDS.to_string());
    assert_eq!(spelled("P.machine.seamNoiseWords"), textedit::SEAM_NOISE_WORDS.to_string());

    // One rule read three ways: what a stretch may be off by is smaller than the most a join may remove,
    // and that in turn sits inside what is shown on either side of it.
    assert!(textedit::SEAM_SNAP_WORDS < textedit::SEAM_MAX_WORDS);
    assert!(textedit::SEAM_NOISE_WORDS < textedit::SEAM_MAX_WORDS);
    assert!(textedit::SEAM_MAX_WORDS < textedit::SEAM_REACH_WORDS);

    // P.eng.keepReachWords — the hand edit's backward match, the same family of look-back.
    assert_eq!(spelled("P.eng.keepReachWords"), hand_edit::KEEP_REACH.to_string());
    assert_eq!(hand_edit::KEEP_REACH, 200);
}

// --- S4: describe -----------------------------------------------------------------------------------

#[test]
fn sec_04_prepare_4_parameters_used_s4_describe_s_frames_per_call_and_context_sizes() {
    // P.machine.describeFramesPerReq: four frames a call, and the readout counts requests against it.
    assert_eq!(spelled("P.machine.describeFramesPerReq"), "4");
    assert_eq!(roles::VISION_FRAMES_PER_CALL, 4);
    assert_eq!(prepare::vision_calls(12), 3, "twelve frames are three calls of four");
    assert_eq!(prepare::vision_calls(9), 3, "and a part-call still costs one");
    assert_eq!(prepare::vision_calls(0), 0);

    // P.machine.describeRecentEvents / describeCtxSegs / describeCtxWindowSeconds.
    assert_eq!(spelled("P.machine.describeRecentEvents"), describe::RECENT_EVENTS.to_string());
    assert_eq!(describe::RECENT_EVENTS, 3);
    assert_eq!(spelled("P.machine.describeCtxSegs"), describe::CTX_SEGS.to_string());
    assert_eq!(describe::CTX_SEGS, 2);
    assert_eq!(number("P.machine.describeCtxWindowSeconds"), describe::CTX_WINDOW_SECONDS);
    assert_eq!(describe::CTX_WINDOW_SECONDS, 10.0);

    // The context window is per side per source, so the brief it builds is bounded by segs × window
    // rather than by the whole recording: two segments within ten seconds either side.
    assert!(describe::CTX_WINDOW_SECONDS > 0.0);
}

// --- S5: P.audio ------------------------------------------------------------------------------------

#[test]
fn sec_04_prepare_4_parameters_used_s5_the_audio_numbers_are_the_servers_bounds() {
    // Chunk sizes: qwen3 takes 60 s, everything else 300 s, and an out-of-memory answer halves to 20.
    assert_eq!(spelled("P.machine.asrChunkQwenSeconds"), asr::CHUNK_QWEN.to_string());
    assert_eq!(spelled("P.machine.asrChunkMaxSeconds"), asr::CHUNK_MAX.to_string());
    assert_eq!(spelled("P.machine.asrChunkMinSeconds"), asr::CHUNK_MIN.to_string());
    // `asr::chunk_limit` matches the family name as a plain substring, so it is the lowercase spelling
    // that gets the short chunk; [`roles::asr_chunk_is_qwen_family`] is the case-insensitive read of the
    // same fact, since the value is whatever the audio box declared in its own model list (§1).
    assert_eq!(asr::chunk_limit("qwen3-asr"), 60.0);
    assert!(roles::asr_chunk_is_qwen_family("Qwen3-ASR"));
    assert!(roles::asr_chunk_is_qwen_family("qwen3-asr-1.7b"));
    assert!(roles::asr_chunk_is_qwen_family("qwen3-asr-1.7b"));
    assert!(!roles::asr_chunk_is_qwen_family("nemotron-asr"));
    assert_eq!(asr::chunk_limit("nemotron-asr"), 300.0);

    // The alignment window and its floor, and separation's chunk.
    assert_eq!(number("P.machine.alignChunkMaxSeconds"), align::WINDOW);
    assert_eq!(number("P.machine.alignChunkMinSeconds"), align::MIN_PIECE);
    assert_eq!(number("P.machine.sepChunkMaxSeconds"), separate::CHUNK_MAX_SECONDS);

    // The silence pair, from both modules that state it.
    assert_eq!(spelled("P.machine.silenceThresholdDB"), "-35");
    assert_eq!(number("P.machine.silenceMinSeconds"), 0.4);
    let (db, min) = params::silence();
    assert_eq!((db, min), (degraded::SILENCE_THRESHOLD_DB, degraded::SILENCE_MIN_SECONDS));
    assert_eq!(db, asr::QUIET_DB);
    assert_eq!(min, asr::QUIET_MIN);

    // Diarization: the ladder biggest first, the stride a share of it, and the anchor's seconds.
    assert_eq!(spelled("P.machine.diarWindowsSeconds"), "90, 45, 25");
    assert_eq!(roles::diar_window(0), 90.0);
    assert_eq!(roles::diar_window(1), 45.0);
    assert_eq!(roles::diar_window(2), 25.0);
    assert_eq!(spelled("P.machine.diarHopShare"), "2/3");
    assert_eq!(number("P.machine.anchorPerSeconds"), roles::ANCHOR_PER_SECONDS);
    // A quarter of the biggest window is 22.5, so the anchor is what §10 says at that rung: twelve
    // seconds a voice, less than the window it was measured at.
    assert!(roles::ANCHOR_PER_SECONDS < roles::diar_window(0));

    // Merge gap/length for segment building, and the two engineering numbers beside them.
    assert_eq!(number("P.eng.mergeGapSeconds"), transcribe::MERGE_GAP);
    assert_eq!(number("P.eng.mergeMaxSeconds"), transcribe::MERGE_MAX_LEN);
    assert_eq!(number("P.eng.mergeMaxWordSeconds"), transcribe::MERGE_MAX_WORD);
    assert_eq!(number("P.eng.mergeNearSeconds"), transcribe::MERGE_NEAR);
    assert_eq!(spelled("P.eng.asrSampleRate"), "16000");
    assert_eq!(number("P.eng.asrCutSeekSeconds"), asr::SEEK_MAX);

    // A segment ends at a gap, and what counts as "near" for a join is a bigger gap than that.
    assert!(transcribe::MERGE_NEAR > transcribe::MERGE_GAP);
    assert!(transcribe::MERGE_MAX_WORD < transcribe::MERGE_MAX_LEN);
}

// --- S6: engineering -------------------------------------------------------------------------------

#[test]
fn sec_04_prepare_4_parameters_used_s6_frames_workers_quality_and_scratch() {
    // P.eng.frameGridSeconds: extraction's fixed grid, which Freq no longer sets.
    assert_eq!(spelled("P.eng.frameGridSeconds"), "0.25");
    assert_eq!(frames::GRID, prepare::EXTRACTION_GRID);

    // The worker count never leaves its band, whatever the machine reports or the job holds — including
    // a job with nothing in it, which must still extract rather than run zero workers.
    for cpus in [0usize, 1, 4, 8, 64] {
        for jobs in [0usize, 1, frames::TINY_JOB_FRAMES, 1000] {
            let got = frames::workers(cpus, jobs);
            assert!((1..=frames::WORKERS_MAX).contains(&got), "cpus {cpus} jobs {jobs} -> {got}");
        }
    }
    assert_eq!(frames::workers(32, 1000), frames::WORKERS_MAX);
    assert_eq!(frames::workers(8, frames::TINY_JOB_FRAMES + 1), frames::WORKERS_MIN);
    assert_eq!(spelled("P.eng.frameWorkersMinFrames"), frames::WORKER_MIN_FRAMES.to_string());
    // Four frames a worker at the ceiling is what "a tiny job is one process" means in practice.
    assert!(frames::TINY_JOB_FRAMES <= frames::WORKER_MIN_FRAMES * frames::WORKERS_MAX);

    // The scene pair that restarts the grid.
    assert_eq!(number("P.machine.sceneThreshold"), frames::SCENE_THRESHOLD);
    assert_eq!(number("P.machine.sceneMinGapSeconds"), frames::SCENE_MIN_GAP);

    // Quality: the frames sent are scaled to 896 wide (§1's `.llmframes/`), and what a project keeps is
    // written at -q:v 2 with no scale anywhere in the command.
    assert_eq!(spelled("P.machine.describeFrameWidth"), "896");
    assert_eq!(params::find("machine.jpegQuality").unwrap().spelled, "2");
    let plans = frames::extract_plan("/tmp/in.mkv", &[0.0, 0.25], Path::new("/tmp/out"), 0.0);
    let args = plans
        .iter()
        .map(|plan| plan.join(" "))
        .collect::<Vec<_>>()
        .join("\n");
    assert_eq!(plans.len(), 2);
    assert!(!args.contains("scale"), "the delivered frames stay full size: {args}");
    assert!(args.contains("-q:v 2"), "at the stored quality: {args}");

    // Scratch handling: all three scratch folders live under prepare/inputs/<source>/, and the one
    // stage that cleans up here does so on success only — a stopped run leaves `asr/c03.wav` behind.
    let tree = tree("scratch");
    fs::create_dir_all(tree.input_dir(SOURCE)).unwrap();
    for path in [tree.asr_scratch(SOURCE), tree.diar_scratch(SOURCE), align_dir(&tree)] {
        assert!(path.starts_with(tree.prepare_dir().join("inputs").join(SOURCE)), "{}", path.display());
    }

    let mut asked = 0;
    asr::run(&tree, SOURCE, 10.0, "nemotron-asr", "en", &[], |_file| {
        asked += 1;
        Ok(Answer { text: "one take".into(), words: vec![] })
    })
    .expect("one request");
    assert_eq!(asked, 1);
    assert!(!tree.asr_scratch(SOURCE).exists(), "success clears its own mess");

    let stopped = asr::run(&tree, SOURCE, 10.0, "nemotron-asr", "en", &[], |_file| {
        Err("no room for the model".into())
    });
    assert!(stopped.is_err(), "a refusal is a failure");
}

const SOURCE: &str = "mic.wav";

/// Diarization has no stage of its own in `rust/` yet (F1.3's turn), so its scratch path is taken from
/// the layout directly — which is the point: §4 counts it as engineering regardless of who writes it.
fn align_dir(tree: &Tree) -> PathBuf {
    tree.align_scratch(SOURCE)
}
