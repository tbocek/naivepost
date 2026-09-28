//! F5.2 The render — the WALK (`spec/08-produce.md` §F5.2), driven through `naivepost::produce_exec`.
//!
//! `tests/produce_render_flow.rs` pins the builders in `produce_render`; this proves the ten steps are
//! walked in order, that every subprocess is asked for with its own argv, and that a stop lands between
//! them rather than inside one. No ffmpeg anywhere: the spawner is an injected closure that records argv
//! and writes the file each command names, so sizes and outputs are real without an encoder (this
//! container has none).
//!
//! Every assertion reads what the code EMITS — commands by their `step`, not by a filename suffix, since
//! the mux's own output also ends in `.mp4`.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::LazyLock;

use naivepost::cut::{Cut, Fx, Lane, Seg};
use naivepost::layout::Tree;
use naivepost::narration::Entry;
use naivepost::produce_exec as exec;
use naivepost::produce_flow as flow;
use naivepost::produce_render as render;
use naivepost::produce_screen as screen;
use naivepost::project::{Container, Produce, Publish, Source, Subtitles};

const ITEM: &str = "f5_2";

fn temp_root(tag: &str) -> PathBuf {
    // Removed at the start, not the end: a failing case never reaches its own cleanup.
    let dir = std::env::temp_dir().join(format!("np-f52w-{}-{tag}", std::process::id()));
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

fn entry(s: f64, e: f64, at: f64, text: &str) -> Entry {
    Entry { s, e, at, text: text.to_string(), ..Default::default() }
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

/// A take synthesized in an earlier run, standing in for a line that needs no voice. Created by the test
/// that uses it, because `speech_plan` asks whether the file is actually there.
static SYNTHESIZED: LazyLock<PathBuf> =
    LazyLock::new(|| std::env::temp_dir().join("np-f52w-synthesized.wav"));

// --- the lookups every Materials is built from ---------------------------------------------------

/// S3's `exists`: every asset present except one named "gone", which is what the drop test needs.
fn missing_is_gone(file: &str) -> bool {
    !file.contains("gone")
}

/// No effects on any segment: this walk is about files, rates and lines, not the filter graph.
fn no_fx(_: &Seg) -> Vec<Fx> {
    Vec::new()
}

/// Nothing synthesized yet: every non-empty line still owes a take.
fn no_wav(_: &Entry) -> Option<PathBuf> {
    None
}

/// One line already has its take and the other does not — the state S2's "every line with no wav"
/// question is asked over.
fn has_one_wav(line: &Entry) -> Option<PathBuf> {
    if line.text == "already synthesized" {
        Some(SYNTHESIZED.clone())
    } else {
        None
    }
}

/// Every clip reads the same recording: what a single-camera session hands the encoder.
fn any_source(_: &render::Clip) -> Option<String> {
    Some("/media/take.mp4".to_string())
}

/// THE Materials constructor. Never build one inline in a test: one door, nothing to mangle.
fn mats<'a>(
    lanes: &'a [Lane],
    cues: exec::Cues,
    wav_of: &'a dyn Fn(&Entry) -> Option<PathBuf>,
) -> exec::Materials<'a> {
    exec::Materials {
        exists: &missing_is_gone,
        fx_for: &no_fx,
        lanes,
        src_shape: (1920, 1080),
        wav_of,
        source_file: &any_source,
        cues,
        sources: &project_sources,
    }
}

fn project_sources() -> Vec<Source> {
    project().sources
}

/// What one run recorded: every argv spawned, every line logged, every line spoken.
#[derive(Clone, Default)]
struct Spy {
    argv: Rc<RefCell<Vec<(String, Vec<String>)>>>,
    logs: Rc<RefCell<Vec<String>>>,
    spoken: Rc<RefCell<Vec<String>>>,
}

impl Spy {
    fn new() -> Spy {
        Spy::default()
    }

    /// Run the walk. `stops_at = Some(n)` makes the n-th checkpoint answer "stopped".
    fn run(
        &self,
        run: &flow::Run,
        tree: &Tree,
        materials: &exec::Materials,
        stops_at: Option<usize>,
    ) -> flow::Rendered {
        let argv = self.argv.clone();
        let logs = self.logs.clone();
        let spoken = self.spoken.clone();
        let asked = Rc::new(RefCell::new(0usize));
        exec::run_render(
            run,
            tree,
            materials,
            |line| logs.borrow_mut().push(line.to_string()),
            |line| {
                spoken.borrow_mut().push(line.text.clone());
                Ok(())
            },
            move |_at, _which| match stops_at {
                Some(n) => {
                    let mut count = asked.borrow_mut();
                    *count += 1;
                    *count < n
                }
                None => true,
            },
            |command: &exec::Command| {
                argv.borrow_mut().push((command.step.to_string(), command.argv.clone()));
                // The tool "writes" the file its last argument names, so size and existence are real.
                if let Some(out) = command.argv.last() {
                    let path = Path::new(out);
                    if let Some(dir) = path.parent() {
                        let _ = std::fs::create_dir_all(dir);
                    }
                    let _ = std::fs::write(path, b"a rendered file of some length");
                }
                Ok(())
            },
        )
    }

    fn steps(&self) -> Vec<String> {
        self.argv.borrow().iter().map(|(step, _)| step.clone()).collect()
    }

    fn count_step(&self, step: &str) -> usize {
        self.argv.borrow().iter().filter(|(s, _)| s == step).count()
    }

    fn last_with(&self, needle: &str) -> Vec<String> {
        self.argv
            .borrow()
            .iter()
            .rev()
            .find(|(_, args)| args.iter().any(|a| a.contains(needle)))
            .map(|(_, args)| args.clone())
            .unwrap_or_default()
    }

    fn joined(&self) -> String {
        self.argv
            .borrow()
            .iter()
            .map(|(_, args)| args.join(" "))
            .collect::<Vec<String>>()
            .join("\n")
    }

    fn logs_vec(&self) -> Vec<String> {
        self.logs.borrow().clone()
    }

    fn spoken_vec(&self) -> Vec<String> {
        self.spoken.borrow().clone()
    }
}

/// A run over `segs` with `lines` narrated, these settings, this voice.
fn run_over(
    tag: &str,
    segs: Vec<Seg>,
    lines: Vec<Entry>,
    settings: Produce,
    voice: &str,
) -> (Tree, flow::Run) {
    let (_root, tree) = tree_in(tag);
    let run = flow::snapshot(
        Some(cut_of(segs)),
        &tree,
        lines,
        settings,
        &project(),
        Some(Publish::default()),
        voice.to_string(),
    );
    (tree, run)
}

// ---- S1 -----------------------------------------------------------------------------------------

#[test]
fn f5_2_s1_the_run_clears_the_scratch_folder() {
    assert_eq!(ITEM, "f5_2");
    let (_root, tree) = tree_in("s1");
    let scratch = tree.clips_dir();
    std::fs::create_dir_all(&scratch).unwrap();
    std::fs::write(scratch.join("old-a.mp4"), b"x").unwrap();
    std::fs::write(scratch.join("old-b.mp4"), b"y").unwrap();
    assert_eq!(exec::clear_scratch(&scratch), Ok(2), "both files are removed");
    assert!(scratch.is_dir(), "the folder stays: the render writes into it seconds later");
    assert_eq!(exec::clear_scratch(&scratch), Ok(0), "an empty folder has nothing to clear");
    let never = scratch.join("never-existed");
    assert_eq!(exec::clear_scratch(&never), Ok(0), "a missing folder is made, not an error");
    assert!(never.is_dir());
}

// ---- S2 -----------------------------------------------------------------------------------------

#[test]
fn f5_2_s2_captions_only_speaks_nothing_and_warns_when_no_track() {
    let captions = naivepost::narrate_screen::CAPTIONS;
    // Captions only + "none in the video": the words exist nowhere but the sheet beside it.
    let (tree, run) = run_over(
        "s2-none",
        vec![seg(0.0, 10.0)],
        vec![entry(0.0, 5.0, 0.0, "a line")],
        Produce { subtitles: Subtitles::None, ..Default::default() },
        captions,
    );
    let spy = Spy::new();
    spy.run(&run, &tree, &mats(&[], exec::Cues::default(), &no_wav), None);
    assert!(spy.spoken_vec().is_empty(), "captions only speaks NOTHING: {:?}", spy.spoken_vec());
    assert!(
        spy.logs_vec()
            .iter()
            .any(|l| l.contains("captions only and nothing in the video")),
        "and the spec's warning is logged: {:?}",
        spy.logs_vec()
    );

    // Same voice, a tracked subtitle: the words are in the file, so no warning.
    let (tree2, run2) = run_over(
        "s2-track",
        vec![seg(0.0, 10.0)],
        vec![entry(0.0, 5.0, 0.0, "a line")],
        Produce { subtitles: Subtitles::TrackInFile, ..Default::default() },
        captions,
    );
    let spy2 = Spy::new();
    spy2.run(&run2, &tree2, &mats(&[], exec::Cues::default(), &no_wav), None);
    assert!(
        !spy2.logs_vec().iter().any(|l| l.contains("nothing in the video")),
        "a tracked subtitle needs no warning: {:?}",
        spy2.logs_vec()
    );
}

#[test]
fn f5_2_s2_a_line_with_no_wav_is_spoken_and_the_job_line_counts_up() {
    std::fs::write(&*SYNTHESIZED, b"a take from an earlier run").unwrap();
    let (tree, run) = run_over(
        "s2-speak",
        vec![seg(0.0, 10.0)],
        vec![
            entry(0.0, 4.0, 0.0, "already synthesized"),
            entry(5.0, 9.0, 0.0, "needs a voice"),
        ],
        Default::default(),
        "",
    );
    let spy = Spy::new();
    spy.run(&run, &tree, &mats(&[], exec::Cues::default(), &has_one_wav), None);
    let spoken = spy.spoken_vec();
    assert_eq!(spoken, vec!["needs a voice".to_string()], "only the line with no wav is spoken");
    // One line owed a take, so the job line reads 1 of 1 — the total is what had to be spoken, not the
    // number of rows on the page.
    assert!(
        spy.logs_vec().iter().any(|l| l == "speaking 1/1"),
        "the speaking job counts up in the render's own shape: {:?}",
        spy.logs_vec()
    );
}

// ---- S3 -----------------------------------------------------------------------------------------

#[test]
fn f5_2_s3_a_missing_insert_is_skipped_and_named() {
    let mut gone = seg(10.0, 20.0);
    gone.ins = "media/gone.mp4".into();
    gone.dur = 10.0;
    let (tree, run) = run_over(
        "s3",
        vec![seg(0.0, 10.0), gone, seg(20.0, 30.0)],
        vec![],
        Default::default(),
        "",
    );
    let spy = Spy::new();
    spy.run(&run, &tree, &mats(&[], exec::Cues::default(), &no_wav), None);
    assert!(
        spy.logs_vec()
            .iter()
            .any(|l| l.contains("clip 2: media/gone.mp4 is not there any more")),
        "the missing insert is named with its clip number: {:?}",
        spy.logs_vec()
    );
    // Counted by step, not by extension: the mux's output is a .mp4 too.
    assert_eq!(spy.count_step("clip"), 2, "the two live clips encode: {:?}", spy.steps());
    assert!(
        !spy.joined().contains("gone"),
        "no command was asked for the file that is not there"
    );
}

// ---- S4 -----------------------------------------------------------------------------------------

#[test]
fn f5_2_s4_final_srt_lands_in_the_scratch_folder() {
    let (tree, run) = run_over(
        "s4",
        vec![seg(0.0, 10.0)],
        vec![entry(0.0, 8.0, 0.0, "words worth reading")],
        Default::default(),
        "",
    );
    let scratch = tree.clips_dir();
    std::fs::create_dir_all(&scratch).unwrap();
    // The foreign file goes beside the VIDEO, not in the scratch folder: S1 clears `produce/clips/` whole,
    // so a stranger there is gone by S1's own rule and proves nothing. What S4 must prove is that its
    // stale-name deletion touches only the names it lists, next door to the finished file.
    let produce = tree.final_video("mp4").parent().unwrap().to_path_buf();
    std::fs::create_dir_all(&produce).unwrap();
    let foreign = produce.join("someone-else.txt");
    std::fs::write(&foreign, b"not mine").unwrap();
    let stale = scratch.join(render::final_srt_name());
    std::fs::write(&stale, b"an old sheet").unwrap();
    let spy = Spy::new();
    spy.run(&run, &tree, &mats(&[], exec::Cues::default(), &no_wav), None);
    let written = std::fs::read_to_string(&stale).expect("final.srt is written in produce/clips");
    assert!(written.contains("-->"), "and it is a cue sheet, not leftovers: {written}");
    assert!(
        !written.contains("an old sheet"),
        "the stale sheet was replaced, not appended to"
    );
    assert!(foreign.exists(), "a file whose name the run does not list survives: deleted by exact name only");
}

// ---- S5 -----------------------------------------------------------------------------------------

#[test]
fn f5_2_s5_every_clip_gets_its_own_encode_command_logged_before_it_runs() {
    let (tree, run) = run_over(
        "s5",
        vec![seg(0.0, 10.0), seg(10.0, 20.0), seg(20.0, 30.0)],
        vec![],
        Default::default(),
        "",
    );
    let spy = Spy::new();
    spy.run(&run, &tree, &mats(&[], exec::Cues::default(), &no_wav), None);
    let held = spy.argv.clone();
    let guard = held.borrow();
    let encodes: Vec<&Vec<String>> = guard
        .iter()
        .filter(|(step, _)| step == "clip")
        .map(|(_, args)| args)
        .collect();
    assert_eq!(encodes.len(), 3, "three segments, three encode commands: {:?}", spy.steps());
    for (index, args) in encodes.iter().enumerate() {
        let stem = format!("c{index:03}");
        let out = args.last().unwrap();
        assert!(out.contains(&stem), "{stem} should be in the output name: {out}");
        let first_input = args.iter().position(|a| a == "-i");
        let ss = args.iter().position(|a| a == "-ss");
        match (ss, first_input) {
            // Clip 0 starts at 0 and `encode_command` omits `-ss` there, which is right.
            (None, _) if index == 0 => {}
            (Some(at), Some(input)) => assert!(
                at < input,
                "S5: -ss must come BEFORE -i, got {at} vs {input} in {stem}"
            ),
            (Some(_), None) => panic!("S5: a clip was encoded with no input at all: {args:?}"),
            (None, Some(_)) => panic!("S5: clip {index} has an input but no seek where one is due: {args:?}"),
            (None, None) => panic!("S5: clip {index} was encoded with no input file at all: {args:?}"),
        }
    }
    let logs = spy.logs_vec();
    for args in &encodes {
        let out = args.last().unwrap();
        assert!(logs.iter().any(|l| l.contains(out)), "each command is logged before it runs: {out}");
    }
}

#[test]
fn f5_2_s5_the_narration_sits_at_the_game_volume() {
    let game_volume = Produce::default().game_volume;
    assert_eq!(game_volume, 0.22, "P.policy.gameVolume is 0.22 (§10)");
    let row = naivepost::params::produce()
        .into_iter()
        .find(|r| r.id == "P.policy.gameVolume")
        .expect("P.policy.gameVolume is catalogued");
    assert_eq!(row.spelled, "0.22");
    let (tree, run) = run_over(
        "s5-vol",
        vec![seg(0.0, 12.0)],
        vec![entry(0.0, 8.0, 0.0, "spoken over the bed")],
        Default::default(),
        "",
    );
    let spy = Spy::new();
    spy.run(&run, &tree, &mats(&[], exec::Cues::default(), &no_wav), None);
    assert!(
        spy.joined().contains(&format!("volume={:.2}", game_volume)),
        "the bed is turned down to P.policy.gameVolume in the audio chain: {}",
        spy.joined()
    );
}

// ---- S6 -----------------------------------------------------------------------------------------

#[test]
fn f5_2_s6_join_is_a_stream_copy() {
    let (tree, run) = run_over(
        "s6",
        vec![seg(0.0, 10.0), seg(10.0, 20.0)],
        vec![],
        Default::default(),
        "",
    );
    let spy = Spy::new();
    spy.run(&run, &tree, &mats(&[], exec::Cues::default(), &no_wav), None);
    assert_eq!(spy.count_step(render::JOINING), 1, "one join for the whole run: {:?}", spy.steps());
    let join = spy.last_with("concat");
    let pairs: Vec<String> = join.windows(2).map(|w| format!("{} {}", w[0], w[1])).collect();
    assert!(
        pairs.iter().any(|p| p == "-c copy"),
        "S6: the join is `-c copy`, a stream copy with no re-encode: {join:?}"
    );
    assert!(spy.logs_vec().iter().any(|l| l.contains(render::JOINING)), "'joining' is said");
}

// ---- S7 -----------------------------------------------------------------------------------------

#[test]
fn f5_2_s7_translation_is_asked_after_every_encode() {
    let settings = Produce { translate: vec!["German".to_string()], ..Default::default() };
    let cues = exec::Cues { all: Vec::new(), languages: vec!["German".to_string()] };
    let (tree, run) = run_over(
        "s7",
        vec![seg(0.0, 10.0), seg(10.0, 20.0)],
        vec![entry(0.0, 8.0, 0.0, "a line")],
        settings,
        "",
    );
    let spy = Spy::new();
    spy.run(&run, &tree, &mats(&[], cues.clone(), &no_wav), None);
    let steps = spy.steps();
    let logs = spy.logs_vec();
    let last_encode = steps
        .iter()
        .rposition(|s| s == "clip")
        .expect("the encodes ran before anything else could be translated");
    let first_translate = logs
        .iter()
        .position(|l| l.contains("translating"))
        .expect("S7: the translation was asked for");
    assert!(
        last_encode < steps.len() && first_translate >= 0,
        "both halves of the order exist"
    );
    // The translation is logged after every encode command was logged too, so the encoder never idles
    // behind the model gate.
    let last_encode_log = logs
        .iter()
        .rposition(|l| l.starts_with(">>> clip"))
        .expect("every encode logged");
    assert!(
        last_encode_log < first_translate,
        "S7: every encode came BEFORE the translation: {logs:?}"
    );
}

// ---- S8 -----------------------------------------------------------------------------------------

#[test]
fn f5_2_s8_the_mux_carries_the_loudnorm_and_the_container_tracks() {
    let tracked = Produce { subtitles: Subtitles::TrackInFile, ..Default::default() };
    let cues = exec::Cues { all: Vec::new(), languages: Vec::new() };
    let (tree, run) = run_over("s8", vec![seg(0.0, 10.0)], vec![entry(0.0, 8.0, 0.0, "a line")], tracked, "");
    let spy = Spy::new();
    spy.run(&run, &tree, &mats(&[], cues.clone(), &no_wav), None);
    let mux = spy.last_with("loudnorm");
    let joined = mux.join(" ");
    assert!(joined.contains(render::LOUDNORM), "the loudness filter is the spec's own string");
    assert!(joined.contains("48000"), "resampled to 48 kHz");
    assert!(joined.contains("+faststart"), "mp4 gets the faststart flags");
    assert!(joined.contains("mov_text"), "mp4's subtitle codec is mov_text");

    // webm forces both halves of §A's rule: vp9, and no text track at all.
    let webm = Produce {
        container: Container::Webm,
        subtitles: Subtitles::TrackInFile,
        ..Default::default()
    };
    let (codec, choice) = screen::apply_container_rules(webm.container, webm.codec, webm.subtitles);
    assert_eq!(codec, naivepost::project::Codec::Vp9, "webm forces vp9");
    assert_eq!(choice, Subtitles::None, "webm forces 'none in the video'");
    let (tree2, run2) = run_over("s8-webm", vec![seg(0.0, 10.0)], vec![entry(0.0, 8.0, 0.0, "a line")], webm, "");
    let spy2 = Spy::new();
    spy2.run(&run2, &tree2, &mats(&[], cues.clone(), &no_wav), None);
    let mux2 = spy2.last_with("loudnorm");
    assert!(!mux2.is_empty(), "the webm mux ran");
    assert!(
        !mux2.iter().any(|a| a == "-c:s"),
        "webm carries no subtitle track: {:?}",
        mux2.last().unwrap_or(&String::new())
    );
}

// ---- S9 -----------------------------------------------------------------------------------------

#[test]
fn f5_2_s9_sidecars_exist_per_language_and_each_is_logged() {
    let settings = Produce { translate: vec!["German".to_string()], ..Default::default() };
    let cues = exec::Cues { all: Vec::new(), languages: vec!["German".to_string()] };
    let (tree, run) = run_over(
        "s9",
        vec![seg(0.0, 10.0)],
        vec![entry(0.0, 8.0, 0.0, "words for the sheet")],
        settings,
        "",
    );
    let spy = Spy::new();
    spy.run(&run, &tree, &mats(&[], cues.clone(), &no_wav), None);
    assert!(tree.final_srt(None).exists(), "the session's own .srt beside the video");
    assert!(tree.final_vtt(None).exists(), "…and its .vtt");
    assert!(tree.final_srt(Some("German")).exists(), "the ticked language's .srt");
    assert!(tree.final_vtt(Some("German")).exists(), "…and its .vtt");
    let logs = spy.logs_vec();
    let lines: Vec<&String> = logs
        .iter()
        .filter(|l| l.starts_with(">>> subtitles:") && l.contains(".vtt"))
        .collect();
    assert_eq!(lines.len(), 2, "one line per language: {lines:?}");
}

// ---- S10 ----------------------------------------------------------------------------------------

#[test]
fn f5_2_s10_a_stop_at_any_checkpoint_ends_the_run_without_the_stamp() {
    let (tree, run) = run_over(
        "s10",
        vec![seg(0.0, 10.0), seg(10.0, 20.0)],
        vec![],
        Default::default(),
        "",
    );
    let stamp = tree.final_stamp();
    // No narration lines, so the first checkpoint the walk reaches is the one before the join.
    let spy = Spy::new();
    let made = spy.run(&run, &tree, &mats(&[], exec::Cues::default(), &no_wav), Some(1));
    assert!(made.stopped, "the report says stopped");
    assert!(!made.ok, "and not ok");
    assert_eq!(spy.count_step("clip"), 2, "the encodes had already run: {:?}", spy.steps());
    assert_eq!(spy.count_step(render::JOINING), 0, "the join never started");
    assert!(!stamp.exists(), "a stopped run writes no stamp");

    // Stopped one checkpoint later: the join ran, the mux did not, still no stamp.
    let spy2 = Spy::new();
    let made2 = spy2.run(&run, &tree, &mats(&[], exec::Cues::default(), &no_wav), Some(2));
    assert!(made2.stopped, "stopped again, later in the run");
    assert_eq!(spy2.count_step(render::JOINING), 1, "two encodes plus the join");
    assert_eq!(spy2.count_step("muxing"), 0, "no mux: {:?}", spy2.steps());
    assert!(!stamp.exists(), "still no stamp: what stands there was never finished");
}

// ---- the clean ending -----------------------------------------------------------------------------

#[test]
fn f5_2_a_clean_run_reports_its_file_seconds_and_size() {
    let (tree, run) = run_over(
        "clean",
        vec![seg(0.0, 10.0), seg(10.0, 20.0)],
        vec![],
        Default::default(),
        "",
    );
    let spy = Spy::new();
    let made = spy.run(&run, &tree, &mats(&[], exec::Cues::default(), &no_wav), None);
    assert!(made.ok, "a run whose mux finished is ok: {:?}", made);
    assert!(!made.stopped);
    assert!(made.seconds > 0.0, "the run reports how long it took: {}", made.seconds);
    assert_ne!(made.size, "0 B", "and what the produced file weighs: {}", made.size);
    assert!(tree.final_video("mp4").exists(), "produce/final.mp4 is there");
    assert!(tree.final_stamp().exists(), "and the stamp was written by the run that made it");
    // The same numbers feed the run's closing line and the bar's progress text.
    let ending = flow::ending_for(
        "produce/final.mp4",
        &flow::Halves {
            words_ok: true,
            words_reason: None,
            render_ok: made.ok,
            tag_ok: made.tag_ok,
            stopped: made.stopped,
            seconds: made.seconds,
            size: made.size.clone(),
        },
    );
    assert_eq!(ending.status, render::STAGE_DONE);
    assert_eq!(ending.progress, flow::progress_text("produce/final.mp4", made.seconds, &made.size));
}
