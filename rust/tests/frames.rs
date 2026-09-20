//! F1.6 Frames per video — spec/04-prepare.md.
//!
//! The scene pass, the grid and the naming live in [`naivepost::frames`]; ▶ only runs the two ffmpeg
//! invocations it is handed. So these checks assert which seconds a recording becomes frames at, what
//! each command asks for, how the work is split across workers and which files end up on disk — never
//! that ffmpeg ran (spec/00-principles.md §5; ffmpeg is not even installed in this container).

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use naivepost::clock::{self, read_frame};
use naivepost::frames::{self, GRID, SCENE_MIN_GAP, SCENE_THRESHOLD, TINY_JOB_FRAMES, WORKERS_MAX};
use naivepost::layout::Tree;
use naivepost::requests;

const SOURCE: &str = "mic.wav";
const BASE: &str = "mic";

fn temp_dir(tag: &str) -> PathBuf {
    let dir = PathBuf::from(format!("/tmp/naivepost-frames-{}-{}", tag, std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// A project folder with the frames directory this flow writes into.
fn project(tag: &str) -> (PathBuf, Tree) {
    let root = temp_dir(tag);
    let dir = root.join("talk.naivepost");
    fs::create_dir_all(&dir).unwrap();
    let tree = Tree::new(&dir).expect("a .naivepost folder is a project");
    fs::create_dir_all(tree.frames_dir(SOURCE)).unwrap();
    (root, tree)
}

/// ffmpeg's `metadata=print` output for a short clip: four frames a second, one animated slide change
/// around 1.0 s (which scores over the threshold on several frames running), and quiet elsewhere.
fn scene_log() -> String {
    [
        "frame:0    pts:0       pts_time:0",
        "lavfi.scd.score=0.118",
        "frame:1    pts:400     pts_time:0.25",
        "lavfi.scd.score=0.104",
        // The transition: three frames over `P.machine.sceneThreshold` within half a second of each
        // other, which is what an animated slide change looks like. They merge into one change.
        "frame:4    pts:1600    pts_time:1.0",
        "streamindex:0   streamid:N/A",
        "tag:lavfi.scd.score=58.310",
        "frame:5    pts:2000    pts_time:1.25",
        "tag:lavfi.scd.score=41.200",
        "frame:6    pts:2400    pts_time:1.4",
        "tag:lavfi.scd.score=12.900",
        // Exactly on the threshold is not over it, so this is not a change.
        "frame:8    pts:3200    pts_time:2.0",
        "tag:lavfi.scd.score=1.0",
        "frame:9    pts:3600    pts_time:2.25",
        "tag:lavfi.scd.score=0.090",
        // A change later than the merge gap from the first one is its own scene.
        "frame:20   pts:8000    pts_time:5.0",
        "tag:lavfi.scd.score=77.400",
    ]
    .join("\n")
}

/// The extraction, as a test answers it. Every chunk of seconds handed over is recorded, so the split
/// across workers, each worker's share and the order are all assertable — and `mid_run` records whether
/// the marker existed yet at each call, which is what proves S5 writes it last.
#[derive(Clone, Default)]
struct Extract {
    chunks: Arc<Mutex<Vec<Vec<f64>>>>,
    mid_run: Arc<Mutex<Vec<bool>>>,
    marker: PathBuf,
    /// Chunks this many frames or fewer "write" one file each; a longer one writes its count plus one,
    /// so the log's total cannot be the chunk list's length by accident.
    fail_from: Option<usize>,
}

impl Extract {
    fn new(marker: PathBuf) -> Self {
        Self {
            marker,
            ..Default::default()
        }
    }

    /// Fail this chunk (counting from one) with ffmpeg's own words.
    fn failing(mut self, chunk: usize) -> Self {
        self.fail_from = Some(chunk);
        self
    }

    fn calls(&self) -> Vec<Vec<f64>> {
        self.chunks.lock().unwrap().clone()
    }

    /// Every second that was asked for, in the order the workers were handed them.
    fn seconds(&self) -> Vec<f64> {
        self.calls().into_iter().flatten().collect()
    }

    fn mid_run(&self) -> Vec<bool> {
        self.mid_run.lock().unwrap().clone()
    }

    fn run(&mut self, at: &[f64]) -> Result<usize, String> {
        let index = {
            let mut chunks = self.chunks.lock().unwrap();
            chunks.push(at.to_vec());
            chunks.len()
        };
        self.mid_run.lock().unwrap().push(self.marker.is_file());
        if self.fail_from == Some(index) {
            return Err("ffmpeg exited 1".into());
        }
        Ok(at.len())
    }
}

/// S1: the marker says whether these are the frames this grid and threshold would produce.
#[test]
fn f1_6_s1_a_marker_matching_grid_and_threshold_skips_the_pass() {
    // Both numbers, exactly. `P.eng.frameGridSeconds` and P.machine.sceneThreshold.
    assert!(frames::marker_matches(Some((GRID, SCENE_THRESHOLD)), GRID, SCENE_THRESHOLD));
    assert!(!frames::marker_matches(None, GRID, SCENE_THRESHOLD), "no marker, no skip");
    // A different grid means frames that were never extracted at this spacing.
    assert!(!frames::marker_matches(Some((0.5, SCENE_THRESHOLD)), GRID, SCENE_THRESHOLD));
    // And a marker written at another threshold re-runs the scene pass: those frames are missing
    // exactly the changes this threshold would have found.
    assert!(!frames::marker_matches(Some((GRID, 2.0)), GRID, SCENE_THRESHOLD));

    let already = frames::already_log(BASE, GRID, SCENE_THRESHOLD);
    // The exact sentence. Its parenthesised part is the spec's `…`, filled with the two numbers the
    // marker holds: `P.eng.frameGridSeconds` and P.machine.sceneThreshold.
    assert_eq!(
        already,
        ">>> [mic] frames already extracted (0.25 s grid, 1 scene threshold), skipping"
    );
}

/// S1: a skip touches nothing — no extraction, no rewritten scenes, and the marker left as it was.
#[test]
fn f1_6_s1_a_skip_rewrites_nothing() {
    let (_root, tree) = project("s1-skip");
    requests::write_frames_marker(&tree, SOURCE, GRID, SCENE_THRESHOLD).unwrap();
    requests::write_scenes(&tree, SOURCE, &[requests::SceneChange { time: 9.0, score: 9.0 }]).unwrap();
    let before = fs::read_to_string(tree.frames_marker(SOURCE)).unwrap();

    let mut extract = Extract::new(tree.frames_marker(SOURCE));
    let outcome = frames::run(
        &tree,
        SOURCE,
        BASE,
        10.0,
        0.0,
        SCENE_THRESHOLD,
        GRID,
        8,
        &scene_log(),
        |at| extract.run(at),
    )
    .expect("a skip is not a failure");

    assert!(outcome.skipped);
    assert_eq!(outcome.logs, vec![frames::already_log(BASE, GRID, SCENE_THRESHOLD)]);
    assert!(extract.calls().is_empty(), "the tool was asked anyway");
    assert!(extract.mid_run().is_empty());
    assert_eq!(fs::read_to_string(tree.frames_marker(SOURCE)).unwrap(), before);
    // scenes.tsv still holds the one row it held, not the ones this pass would have found.
    let scenes = requests::read_scenes(&tree, SOURCE).unwrap();
    assert_eq!(scenes.len(), 1);
    assert_eq!(scenes[0].time, 9.0);
}

/// S2: the scene pass looks at four frames a second, down-scaled for itself only, and reports every
/// score so the threshold is applied in one place.
#[test]
fn f1_6_s2_the_scene_pass_is_four_frames_a_second_with_every_score_reported() {
    // P.machine.sceneThreshold is applied by `scene_changes`, not by ffmpeg: scdet runs at threshold 0.
    let plan = frames::scene_pass_plan(SCENE_THRESHOLD);
    let filter = plan
        .iter()
        .skip_while(|arg| *arg != "-vf")
        .nth(1)
        .expect("a -vf");
    assert!(filter.contains("fps=4"), "{filter}");
    assert!(filter.contains("scale=640:-2"), "{filter}");
    assert!(filter.contains("scdet=threshold=0"), "{filter}");
    assert!(filter.contains("metadata=print"), "{filter}");
    // The whole video, and the pixels thrown away: only the scores are wanted.
    assert!(plan.contains(&"-an".to_string()), "{plan:?}");
    assert!(plan.windows(2).any(|pair| pair == ["-f", "null"]), "{plan:?}");

    // And the delivered frames carry none of that scaling: every frame stays at the video's own size,
    // because the model's smaller copies are F1.7's to make.
    let dir = Path::new("/tmp/naivepost-frames-s2-plan");
    for command in frames::extract_plan(SOURCE, &[0.0, 0.25], dir, 0.0) {
        let text = command.join(" ");
        assert!(!text.contains("scale"), "{text}");
        assert!(!text.contains("fps"), "{text}");
        assert!(!command.iter().any(|arg| arg == "-s" || arg == "-vf"), "{text}");
    }
}

/// S2: the scores out of ffmpeg's log, and which of them are changes.
#[test]
fn f1_6_s2_a_change_is_a_score_over_the_threshold_and_near_ones_merge_into_the_first() {
    let scored = frames::parse_scores(&scene_log());
    assert_eq!(scored.len(), 8, "{scored:?}");
    // The `frame:`/`streamindex:` noise is skipped and each score keeps its own second.
    assert_eq!(scored[0], (0.0, 0.118));
    assert_eq!(scored[2], (1.0, 58.310));
    // A score with no time before it is dropped rather than guessed at: a change at the wrong second
    // restarts the grid in the wrong place.
    assert_eq!(frames::parse_scores("tag:lavfi.scd.score=99.0").len(), 0);

    let changes = frames::scene_changes(&scored, SCENE_THRESHOLD, SCENE_MIN_GAP);
    let times: Vec<f64> = changes.iter().map(|change| change.time).collect();
    // Three over-threshold frames inside the transition become one change, at the first of them; then
    // the 5.0 s change, which is more than P.machine.sceneMinGapSeconds away.
    assert_eq!(times, vec![1.0, 5.0], "{changes:?}");
    assert_eq!(changes[0].score, 58.310, "the first of the merged frames keeps its score");

    // Exactly on the threshold is not over it.
    let on = [(2.0, 1.0)];
    assert!(frames::scene_changes(&on, SCENE_THRESHOLD, SCENE_MIN_GAP).is_empty());
    // A static recording has no changes at all...
    let quiet: Vec<(f64, f64)> = (0..8).map(|n| (n as f64 * 0.25, 0.1)).collect();
    assert!(frames::scene_changes(&quiet, SCENE_THRESHOLD, SCENE_MIN_GAP).is_empty());
    // ...and a change nearer than the gap than another is gone rather than moving it.
    let pair = [(1.0, 9.0), (1.25, 8.0)];
    let merged = frames::scene_changes(&pair, SCENE_THRESHOLD, SCENE_MIN_GAP);
    assert_eq!(merged.len(), 1);
    assert_eq!((merged[0].time, merged[0].score), (1.0, 9.0));

    // Through the driver: those rows are what `scenes.tsv` holds — times with two decimals are
    // `requests`' business, so they are read back rather than compared as text.
    let (_root, tree) = project("s2-scenes");
    let mut extract = Extract::new(tree.frames_marker(SOURCE));
    frames::run(
        &tree,
        SOURCE,
        BASE,
        10.0,
        0.0,
        SCENE_THRESHOLD,
        GRID,
        8,
        &scene_log(),
        |at| extract.run(at),
    )
    .expect("a pass");
    let saved = requests::read_scenes(&tree, SOURCE).unwrap();
    let saved_times: Vec<f64> = saved.iter().map(|change| change.time).collect();
    assert_eq!(saved_times, times);
}

/// S3: the grid, restarted at every scene change so a new slide's first picture is always there.
#[test]
fn f1_6_s3_frames_come_on_the_grid_and_the_grid_restarts_at_every_change() {
    // P.eng.frameGridSeconds: 0.25 s, fixed and independent of Freq.
    assert_eq!(GRID, 0.25);
    let at = frames::frame_seconds(&[1.0, 1.25], 3.0, GRID);
    assert_eq!(at[0], 0.0, "the start of the file is always a scene start");
    // Both changes are frames in their own right — that is what "the grid restarts at every change"
    // buys, and why the merge gap cannot simply drop them.
    assert!(at.contains(&1.0) && at.contains(&1.25), "{at:?}");
    for pair in at.windows(2) {
        assert!(pair[1] > pair[0], "strictly increasing: {at:?}");
    }
    assert!(at.iter().all(|second| *second < 3.0), "nothing past the end: {at:?}");
    // Between changes the spacing is exactly the grid.
    let steps: Vec<f64> = at.windows(2).map(|pair| pair[1] - pair[0]).collect();
    assert!(steps.iter().all(|step| (*step - GRID).abs() < 1e-9), "{steps:?}");

    // A take shorter than one grid step is its first frame alone.
    assert!(frames::short_take_seconds(0.1, GRID));
    assert_eq!(frames::frame_seconds(&[], 0.1, GRID), vec![0.0]);
    assert!(frames::frame_seconds(&[], 0.0, GRID).is_empty(), "nothing to name");
    // A change pair inside the merge gap costs one extra frame, which is what the spec says a false
    // trigger is worth — not the scene's whole run of pictures.
    let pair = frames::frame_seconds(&[1.0, 1.1], 2.0, GRID);
    assert!(pair.contains(&1.0) && pair.contains(&1.1), "{pair:?}");
    // The next grid step after the second change is 1.35 rather than 1.25: the grid restarted there.
    assert_eq!(pair.iter().filter(|second| **second >= 1.2).count(), 3, "{pair:?}");
}

/// S3: how the extraction is split across workers.
#[test]
fn f1_6_s3_extraction_runs_in_parallel_chunks_and_reports_progress() {
    // clamp(CPU/4, 2, 8), and one worker for a job too small to be worth splitting.
    assert_eq!(frames::workers(8, TINY_JOB_FRAMES + 1), 2);
    assert_eq!(frames::workers(16, TINY_JOB_FRAMES + 1), 4);
    assert_eq!(frames::workers(100, TINY_JOB_FRAMES + 1), WORKERS_MAX);
    assert_ne!(frames::workers(1, TINY_JOB_FRAMES + 1), 0, "no machine has no workers");
    assert_eq!(frames::workers(1, TINY_JOB_FRAMES + 1), 2, "the floor holds");
    assert_eq!(frames::workers(64, TINY_JOB_FRAMES), 1, "a tiny job is one worker's");

    let indexes: Vec<usize> = (0..23).collect();
    let chunks = frames::chunks(&indexes, 2);
    assert_eq!(chunks.len(), 2, "{chunks:?}");
    assert_eq!(chunks.concat(), indexes, "each frame exactly once, in order");
    assert!(chunks.iter().all(|chunk| !chunk.is_empty()), "{chunks:?}");
    // Even chunks: one worker taking twenty frames while another takes three is a pass that waits.
    let spread = chunks.iter().map(|chunk| chunk.len()).collect::<Vec<_>>();
    assert!(spread[0].abs_diff(*spread.last().unwrap()) <= 1, "{spread:?}");
    // Never more chunks than frames, none empty, and no workers at all for nothing to do.
    assert_eq!(frames::chunks(&[0, 1], 8).len(), 2);
    assert_eq!(frames::chunks(&[0], 4), vec![vec![0usize]]);
    assert!(frames::chunks(&[], 4).is_empty());

    // Through the driver: a 3 s recording is 12 frames, which is a tiny job on its own — so ask for a
    // long enough one to see two workers take it in order.
    let (_root, tree) = project("s3-workers");
    let mut extract = Extract::new(tree.frames_marker(SOURCE));
    let outcome = frames::run(
        &tree,
        SOURCE,
        BASE,
        60.0,
        0.0,
        SCENE_THRESHOLD,
        GRID,
        8,
        "",
        |at| extract.run(at),
    )
    .expect("two workers");
    assert_eq!(outcome.workers, 2);
    let calls = extract.calls();
    assert_eq!(calls.len(), 2, "{calls:?}");
    // The two shares are the whole plan, in the plan's own order.
    assert_eq!(extract.seconds().len(), 240, "{} chunks", calls.len());
    for pair in extract.seconds().windows(2) {
        assert!(pair[1] > pair[0], "{:?}", extract.seconds());
    }

    // ffmpeg's `-progress` line, read as seconds for the bar F0.5 owns. Both spellings are microseconds
    // in its output; a millisecond reading would put the bar a thousand times too fast.
    assert_eq!(frames::progress("out_time_ms=1234567\n"), Some(1.234_567));
    assert_eq!(frames::progress("out_time_us=1234567\n"), Some(1.234_567));
    assert_eq!(frames::progress("progress=continue\n"), None);
}

/// S4: a frame is named by its exact second on the file's own clock.
#[test]
fn f1_6_s4_every_frame_is_named_by_its_exact_second_on_the_files_clock() {
    // The session clock's zero, in its own shape.
    assert!(frames::frame_file(0.0, 0.0).starts_with("1970-01-01_00-00-00.000"), "{:?}", frames::frame_file(0.0, 0.0));

    // A recording stamped at two o'clock on the afternoon of 2026-09-18: the frame's name is that stamp
    // plus the second within it, never a position in a list — extracting one more frame must not rename
    // the ones already there.
    let start = 1_789_740_000.0;
    assert_eq!(frames::frame_file(start, 0.0), "2026-09-18_14-00-00.000.jpg");
    assert_eq!(frames::frame_file(start, 0.25), "2026-09-18_14-00-00.250.jpg");
    assert_eq!(frames::frame_file(start, 3661.5), "2026-09-18_15-01-01.500.jpg");

    // And the name reads back as the second it claims, which is what lets Cut place a picture from its
    // file name alone.
    for at in [0.0, 0.25, 3661.5] {
        let name = frames::frame_file(start, at).trim_end_matches(".jpg").to_string();
        let back = read_frame(&name).unwrap_or_else(|| panic!("{name} does not read back"));
        assert!((back - (start + at)).abs() < 0.001, "{name} -> {back}");
    }

    // A quarter-second grid and a millisecond stamp cannot ask for the same name twice — which is why
    // no `-n` collision suffix is needed anywhere in the plan.
    let names: Vec<String> = (0..8).map(|step| frames::frame_file(start, step as f64 * GRID)).collect();
    let unique: std::collections::HashSet<&String> = names.iter().collect();
    assert_eq!(unique.len(), names.len(), "{names:?}");

    // The plan puts those names in the source's own frames folder, seeked before its input.
    let (_root, tree) = project("s4-names");
    let dir = tree.frames_dir(SOURCE);
    let plan = frames::extract_plan(SOURCE, &[0.0, 0.25], &dir, start);
    assert_eq!(plan.len(), 2, "one command per frame");
    for (index, command) in plan.iter().enumerate() {
        assert_eq!(command[0], "-ss", "{command:?}");
        let input = command.iter().position(|arg| arg == "-i").expect("an input");
        assert!(command[0] == "-ss" && input > 1, "the seek leads: {command:?}");
        let frames_at = command.iter().position(|arg| arg == "-frames:v").expect("one frame");
        assert_eq!(command[frames_at + 1], "1", "{command:?}");
        assert!(
            command.last().unwrap().contains(dir.to_str().unwrap()),
            "{command:?}"
        );
        assert!(
            command.last().unwrap().ends_with(&names[index]),
            "{command:?} vs {}",
            names[index]
        );
    }
    // The clock module is the only naming rule, and this flow uses it.
    assert_eq!(clock::frame_name(start), "2026-09-18_14-00-00.000");
}

/// S5: the marker is written last, after every frame exists, and says what a pass did.
#[test]
fn f1_6_s5_the_marker_is_written_last_and_the_pass_says_what_it_made() {
    let (_root, tree) = project("s5-marker");
    let mut extract = Extract::new(tree.frames_marker(SOURCE));
    let outcome = frames::run(
        &tree,
        SOURCE,
        BASE,
        10.0,
        0.0,
        SCENE_THRESHOLD,
        GRID,
        8,
        &scene_log(),
        |at| extract.run(at),
    )
    .expect("a pass");

    // Every worker was handed its frames before the marker existed: it is this stage's resume marker,
    // and a marker for work never done would skip the next run too.
    let mid_run = extract.mid_run();
    assert!(!mid_run.is_empty(), "{mid_run:?}");
    assert!(mid_run.iter().all(|existed| !*existed), "{mid_run:?}");

    // Written whole, and it reads back as this grid and threshold. P.eng.frameGridSeconds and
    // P.machine.sceneThreshold, which is exactly what S1 compares on a later run.
    let marker = requests::read_frames_marker(&tree, SOURCE).unwrap();
    assert_eq!(marker, Some((GRID, SCENE_THRESHOLD)));

    // One log line, naming the frames it wrote and the merged changes it found.
    let written: usize = extract.calls().iter().map(|chunk| chunk.len()).sum();
    let changes = requests::read_scenes(&tree, SOURCE).unwrap().len();
    assert_eq!(outcome.logs, vec![frames::done_log(BASE, written, changes)]);
    let last = outcome.logs.last().unwrap();
    assert!(last.starts_with(">>> [mic] "), "{last}");
    assert!(last.contains(&format!("{written} frames")), "{last}");
    assert!(last.contains(&format!("{changes} scene changes")), "{last}");
    assert_eq!(outcome.frames.len(), written);

    // A recording with no changes at all still gets its grid, and says so.
    let (_root, tree) = project("s5-static");
    let mut extract = Extract::new(tree.frames_marker(SOURCE));
    let outcome = frames::run(
        &tree,
        SOURCE,
        BASE,
        1.0,
        0.0,
        SCENE_THRESHOLD,
        GRID,
        8,
        "frame:0 pts:0 pts_time:0\ntag:lavfi.scd.score=0.02\n",
        |at| extract.run(at),
    )
    .expect("a static recording is a normal recording");
    assert_eq!(outcome.changes, 0);
    assert_eq!(outcome.logs, vec![">>> [mic] 4 frames, 0 scene changes"]);
    assert!(requests::read_frames_marker(&tree, SOURCE).unwrap().is_some());

    // And a failure leaves no marker, so the next run extracts rather than skipping.
    let (_root, tree) = project("s5-failed");
    let mut extract = Extract::new(tree.frames_marker(SOURCE)).failing(1);
    let err = frames::run(
        &tree,
        SOURCE,
        BASE,
        60.0,
        0.0,
        SCENE_THRESHOLD,
        GRID,
        8,
        "",
        |at| extract.run(at),
    )
    .expect_err("a failed extraction is not a success");
    assert_eq!(err, "ffmpeg exited 1");
    assert!(requests::read_frames_marker(&tree, SOURCE).unwrap().is_none());
    assert!(!tree.frames_marker(SOURCE).exists());
}
