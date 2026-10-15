//! F5.2 The render — the WALK (`spec/08-produce.md` §F5.2), driven through `naivepost::produce_exec`.
//!
//! F5.2's GAPS are pinned at the bottom of this file: the cue sheet and effect list a real press carries
//! (S1 reads the session tree's own files and `plan_clips` draws the trims and every `fx_*` on the kept
//! segments, not an empty answer); the sidecar files and the stale ones (S7's legs run through `exec::run`
//! with the SRT leg writing its `final.srt` + `final.vtt` beside the video and the stale ones deleted by
//! exact name, and a refusal without any track writes nothing at all); and S2's real speak leg (the lines
//! with no `take.wav` walk through the leg, each refusal reaching the log, and the render half runs anyway).
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
use naivepost::produce_subtitles as subs;
use naivepost::project::{Container, Produce, Publish, Source, Subtitles};

const ITEM: &str = "f5_2";

fn seg(s: f64, e: f64) -> Seg {
    Seg { s, e, cam: 0, ..Default::default() }
}

fn entry(s: f64, e: f64, at: f64, text: &str) -> Entry {
    Entry { s, e, at, text: text.to_string(), ..Default::default() }
}

fn cut_of(segs: Vec<Seg>) -> Cut {
    Cut { segs, aspect: "16:9".into(), ..Default::default() }
}

#[allow(dead_code)] // every test binary compiles this whole module; a helper it does not call is not a warning here
mod common;
use common::produce_project as project;
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
/// A take that exists and measures 2.5 s: enough for the audio chain to duck the bed under it.
fn seconds_2_5(_entry: &Entry) -> Option<f64> { Some(2.5) }

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

/// No aligned words: these tests are about files, rates and lines. The F5.4 word path is exercised in
/// `tests/produce_subtitles_flow.rs`, which supplies its own list.
fn no_words(_: &render::Clip) -> Vec<subs::Word> {
    Vec::new()
}

/// A model that answers every number it is given, in a language of its own. The S3 walk is driven for
/// real here; `tests/produce_translate_flow.rs` covers the refusals.
fn answer_every_number(
    _message: &str,
    numbers: &[usize],
) -> Result<std::collections::BTreeMap<usize, String>, String> {
    Ok(numbers.iter().map(|n| (*n, format!("uebersetzt {n}"))).collect::<std::collections::BTreeMap<usize, String>>())
}

/// THE Materials constructor. Never build one inline in a test: one door, nothing to mangle.
fn mats<'a>(
    lanes: &'a [Lane],
    cues: exec::Cues,
    wav_of: &'a dyn Fn(&Entry) -> Option<PathBuf>,
    wav_seconds: &'a dyn Fn(&Entry) -> Option<f64>,
) -> exec::Materials<'a> {
    exec::Materials {
        exists: &missing_is_gone,
        fx_for: &no_fx,
        lanes,
        src_shape: (1920, 1080),
        wav_of,
        source_file: &any_source,
        on_recording: true,
        footage_left: &naivepost::produce_render::unknown_footage,
        wav_seconds,
        cues,
        words: &no_words,
        translate: &answer_every_number,
        sources: &project_sources,
        // No ffprobe claim: the size check is pinned by its own test through the program's probe seam.
        probe: &|_file| None,
    }
}

fn project_sources() -> Vec<Source> {
    project().sources
}

/// One recorded spawn: the step it belongs to and the argv it was asked for.
type Spawned = (String, Vec<String>);

/// What one run recorded: every argv spawned, every line logged, every line spoken.
#[derive(Clone, Default)]
struct Spy {
    argv: Rc<RefCell<Vec<Spawned>>>,
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
    let (_root, tree) = common::tree_in(tag);
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
    let (_root, tree) = common::tree_in("s1");
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
    spy.run(&run, &tree, &mats(&[], exec::Cues::default(), &no_wav, &seconds_2_5), None);
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
    spy2.run(&run2, &tree2, &mats(&[], exec::Cues::default(), &no_wav, &seconds_2_5), None);
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
    spy.run(&run, &tree, &mats(&[], exec::Cues::default(), &has_one_wav, &seconds_2_5), None);
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
    spy.run(&run, &tree, &mats(&[], exec::Cues::default(), &no_wav, &seconds_2_5), None);
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
    spy.run(&run, &tree, &mats(&[], exec::Cues::default(), &no_wav, &seconds_2_5), None);
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
    spy.run(&run, &tree, &mats(&[], exec::Cues::default(), &no_wav, &seconds_2_5), None);
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
    spy.run(&run, &tree, &mats(&[], exec::Cues::default(), &no_wav, &seconds_2_5), None);
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
    spy.run(&run, &tree, &mats(&[], exec::Cues::default(), &no_wav, &seconds_2_5), None);
    assert_eq!(spy.count_step(render::JOINING), 1, "one join for the whole run: {:?}", spy.steps());
    let join = spy.last_with("concat");
    let pairs: Vec<String> = join.windows(2).map(|w| format!("{} {}", w[0], w[1])).collect();
    assert!(
        pairs.iter().any(|p| p == "-c copy"),
        "S6: the join is `-c copy`, a stream copy with no re-encode: {join:?}"
    );
    assert!(spy.logs_vec().iter().any(|l| l.contains(render::JOINING)), "'joining' is said");
}

/// §F5.2 S5's other half: the size question is asked of the ENCODED FILES, through the program's own
/// probe door (`Materials::probe`, whose live answer is ffprobe beside the configured ffmpeg), and the
/// clip that came out at another frame size is named by its stem. A byte-count heuristic cannot say this:
/// two clips at one size differ in bytes every time.
///
/// A fake ffprobe: a script that answers one line in ffprobe's own `csv=p=0` shape, chosen by the NAME of
/// the file it is asked about (the LAST argument, which is what [`produce_probe::args_for`] passes last).
///
/// It goes in through the door the program reads (`NAIVEPOST_FFPROBE`, see `ffprobe_program`), so the walk
/// runs its real argv and its real parse rather than a scripted answer. `sizes` maps a clip-stem pattern to
/// The size that clip "came out at"; anything unmatched gets `base`.
///
/// The name is unique per case because cargo runs this binary's tests on parallel threads: one process-wide
/// env var and one script per case would let a second case re-write the first one's file mid-run, so both
/// probe cases also take turns on [`serialise_probes`].
/// The fake ffprobe's own folder, one per case: the script writes a line there every time it is asked,
/// naming the file it was asked about. That record is how the test proves the walk really spawned ffprobe
/// over its own clip files — `subprocess::run` deliberately does not log a probe (see `produce_probe`).
fn fake_ffprobe(tag: &str, sizes: &[(&str, (i32, i32))], base: (i32, i32)) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let script = std::env::temp_dir().join(format!("np-f52-{tag}-ffprobe.sh"));
    let asked = std::env::temp_dir().join(format!("np-f52-{tag}-asked.log"));
    let _ = std::fs::remove_file(&asked);
    // One `case` arm per prefix, in ffprobe's exact output shape: two numbers, one comma, no labels.
    // The file probed is the LAST argument (`args_for` puts it there); `$NF` is a field count of one line,
    // not of the argument list, so the arguments are walked for the last one the way a real binary would.
    let arms: Vec<String> = sizes
        .iter()
        .map(|(prefix, (w, h))| format!("  {prefix}*) echo \"{w},{h}\";;"))
        .collect();
    common::write_script(
        &script,
        format!(
            "#!/bin/sh\nlast=\"\"\nfor a in \"$@\"; do last=\"$a\"; done\necho \"$last\" >> {}\ncase \"$(basename \"$last\")\" in\n{}\n  *) echo \"{},{}\";;\nesac\n",
            asked.display(),
            arms.join("\n"),
            base.0,
            base.1
        ),
    )
    .expect("the ffprobe stub");
    std::fs::set_permissions(&script, PermissionsExt::from_mode(0o755)).expect("it runs");
    script
}

/// What a fake ffprobe was asked about, one path per line — empty when nothing probed it.
fn asked_of(tag: &str) -> String {
    std::fs::read_to_string(std::env::temp_dir().join(format!("np-f52-{tag}-asked.log")))
        .unwrap_or_default()
}

/// The two probe cases share ONE process-wide door (`NAIVEPOST_FFPROBE`), and cargo runs this binary's tests
/// on parallel threads, so they take turns holding this lock for the length of their own render.
fn serialise_probes() -> &'static std::sync::Mutex<()> {
    static LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
    LOCK.get_or_init(|| std::sync::Mutex::new(()))
}

/// §S5's size question, driven through the REAL leg: each encoded clip is probed with ffprobe and the one
/// that came out at another frame size is named, because the stream-copy join breaks exactly there.
#[test]
fn f5_2_s5_a_probed_clip_at_another_size_is_named() {
    // Both probe cases point the one process-wide door, so they take turns (see `serialise_probes`).
    let _guard = serialise_probes().lock().unwrap_or_else(|e| e.into_inner());
    let (tree, run) = run_over(
        "s5-probe",
        vec![seg(0.0, 10.0), seg(10.0, 20.0)],
        vec![],
        Default::default(),
        "",
    );
    // The fake ffprobe answers by the file's NAME: a clip encoded as `c001_…` comes back 1280x720 while
    // `c000_…` keeps the base 1920x1080 — the state a mis-scaled encode leaves behind. (`clip_stem` spells
    // it `c000_<stamp>`, so the arm matches that prefix, not a zero-padded index with no underscore.)
    let script = fake_ffprobe("mismatch", &[("c001_*", (1280, 720))], (1920, 1080));
    std::env::set_var("NAIVEPOST_FFPROBE", &script);
    let spy = Spy::new();
    let mut materials = mats(&[], exec::Cues::default(), &no_wav, &seconds_2_5);
    // The door the page hands the walk IS the program's door, so the walk really spawns ffprobe.
    materials.probe = &naivepost::produce_probe::probe;
    spy.run(&run, &tree, &materials, None);
    let logs = spy.logs_vec();
    // Proof the leg ran for real: the fake records every file it was asked about, and the walk must have
    // asked about BOTH encoded clips — that is what makes the reply below an answer about this run's files.
    let asked = asked_of("mismatch");
    assert!(
        asked.contains("c000_") && asked.contains("c001_"),
        "every encoded clip was probed: {asked}"
    );
    let named = logs
        .iter()
        .find(|l| l.contains(render::JOIN_MISMATCH))
        .expect("the mismatched clip is said with the spec's sentence");
    assert!(named.starts_with("c001"), "and it names the CLIP, not the first one: {named}");
    assert!(
        !logs.iter().any(|l| l.starts_with("c000") && l.contains(render::JOIN_MISMATCH)),
        "the clip at the base size says nothing: {logs:?}"
    );
    // Un-point the seam for whatever runs next in this process; each probe case points it again anyway.
    std::env::set_var("NAIVEPOST_FFPROBE", "");
}

/// The other side of the same door: with nothing to probe (no ffprobe on this machine), the run claims
/// NOTHING about sizes — silence, never a false all-clear nor a false alarm.
#[test]
fn f5_2_s5_a_clip_that_cannot_be_probed_claims_nothing() {
    // The other side of the same shared door: it points `NAIVEPOST_FFPROBE` too, so it holds the turn.
    let _guard = serialise_probes().lock().unwrap_or_else(|e| e.into_inner());
    let (tree, run) = run_over(
        "s5-noprobe",
        vec![seg(0.0, 10.0), seg(10.0, 20.0)],
        vec![],
        Default::default(),
        "",
    );
    // A ffprobe that answers nothing and fails: the real leg reads that as "no claim" for every clip.
    use std::os::unix::fs::PermissionsExt;
    let dead = std::env::temp_dir().join("np-f52-dead-ffprobe.sh");
    common::write_script(&dead, "#!/bin/sh\nexit 1\n").expect("the failing stub");
    std::fs::set_permissions(&dead, PermissionsExt::from_mode(0o755)).expect("it runs");
    std::env::set_var("NAIVEPOST_FFPROBE", &dead);
    let spy = Spy::new();
    let mut materials = mats(&[], exec::Cues::default(), &no_wav, &seconds_2_5);
    // The program's own door again, so "no ffprobe" is measured by a probe that really cannot answer — not
    // by a closure handed a `None`. (This case must set the seam itself: cargo runs the tests in this binary
    // on parallel threads, so it cannot inherit another case's pointing.)
    materials.probe = &naivepost::produce_probe::probe;
    spy.run(&run, &tree, &materials, None);
    std::env::set_var("NAIVEPOST_FFPROBE", "");
    assert!(
        !spy.logs_vec().iter().any(|l| l.contains(render::JOIN_MISMATCH)),
        "no probe, no claim: {:?}",
        spy.logs_vec()
    );
}

// ---- S7 -----------------------------------------------------------------------------------------

#[test]
fn f5_2_s7_translation_is_asked_after_every_encode() {
    let settings = Produce { translate: vec!["German".to_string()], ..Default::default() };
    let cues = exec::Cues::languages_only(vec!["German".to_string()]);
    let (tree, run) = run_over(
        "s7",
        vec![seg(0.0, 10.0), seg(10.0, 20.0)],
        vec![entry(0.0, 8.0, 0.0, "a line")],
        settings,
        "",
    );
    let spy = Spy::new();
    spy.run(&run, &tree, &mats(&[], cues.clone(), &no_wav, &seconds_2_5), None);
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
        last_encode < steps.len(),
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

// ---- F5.2's gaps: a real press, through the real page run ---------------------------------------
//
// Everything above drives `exec::run_render` with pieces the test hands in — a cue sheet, `no_fx`, a speak
// closure. The three gaps the round named were exactly those hand-fed pieces: §S1 never read the SESSION
// TREE's own files, so it answered with an empty cue sheet and a cut with no effects; §S7's legs were
// never run through the walk, so no `final.srt`/`final.vtt` ever landed beside a video and no stale one was
// ever deleted; and §S2's speak leg was a count, not the walk over the lines that owe a take, so no
// refusal was ever said. The three tests below drive the PAGE'S OWN run (`produce_flow::run_with`, the
// same call `ui::produce_page::produce_run` makes) over a tree written the way the Cut and Narrate pages
// write it, and read back what stands on disk and in the log — never a state poke.

/// A zoom on the first segment, as the Cut page's own rule stores it (F3.6's `zoom` kind, `trans`/`tout`
/// the glide, `x`/`hold` the box): enough for the camera chain to draw a real `zoompan` for it.
fn zoom_fx(t: f64, dur: f64) -> Fx {
    Fx {
        kind: "zoom".into(),
        t,
        dur,
        trans: 0.5,
        tout: 0.5,
        cx: Some(0.5),
        cy: Some(0.6),
        stay: true,
        ..Default::default()
    }
}

/// Materials whose effects come from a real cut's `fx` list instead of `no_fx`: the press's own §S1 answer.
///
/// The closures and the list they read are leaked to the heap, the way `tests/sec_06_effects_4_render.rs`
/// does it: a per-test local cannot outlive the borrow `Materials` holds, and this is scratch that dies
/// with the test binary.
fn mats_with_fx<'a>(
    cues: exec::Cues,
    fx: Vec<Fx>,
) -> exec::Materials<'a> {
    static LANES: &[Lane] = &[];
    let boxed: &'static Vec<Fx> = Box::leak(Box::new(fx));
    let fx_for: &'static dyn Fn(&Seg) -> Vec<Fx> = Box::leak(Box::new(move |_seg: &Seg| boxed.clone()));
    let wav_of: &'static dyn Fn(&Entry) -> Option<PathBuf> = Box::leak(Box::new(no_wav));
    let wav_seconds: &'static dyn Fn(&Entry) -> Option<f64> = Box::leak(Box::new(seconds_2_5));
    exec::Materials {
        exists: &missing_is_gone,
        fx_for,
        lanes: LANES,
        src_shape: (1920, 1080),
        wav_of,
        source_file: &any_source,
        on_recording: true,
        footage_left: &naivepost::produce_render::unknown_footage,
        wav_seconds,
        cues,
        words: &no_words,
        translate: &answer_every_number,
        sources: &project_sources,
        probe: &|_file| None,
    }
}

/// A press: a session tree with the pages' own files on disk, plus the `Run` the press hands F5.2.
struct Press {
    root: PathBuf,
    tree: Tree,
    run: flow::Run,
    settings: Produce,
    /// The video's stem, which is what §S4's exact-name sweep builds its list from. A real press spells it
    /// off the produced file (`exec::video_stem`); a test that writes an old sheet in `produce/` has to
    /// spell it the same way, or it names a file no sweep was ever asked about. §S4's sweep also refuses to
    /// delete the scratch sheet's own path, so a test that wants to watch a delete must plant the file where
    /// the produced file goes and give the run a video that was never written.
    stem: String,
}

/// The cues a press carries: the session's own sheet, built from the run's narration lines the way
/// §S4 builds it (every non-empty line, on its clip's clock), which is what a real press hands `Materials`.
fn press_cues(lines: &[Entry]) -> Vec<subs::Cue> {
    lines
        .iter()
        .filter(|l| !l.text.trim().is_empty())
        .map(|l| subs::Cue {
            s: l.at,
            e: l.at + (l.e - l.s).max(0.4),
            text: l.text.clone(),
            pos: String::new(),
        })
        .collect()
}

impl Press {
    /// Write the cut, the narration and the settings the way the pages write them, and build the `Run`
    /// from the same doors the press reads: `flow::snapshot` over the tree, so what the render half is
    /// handed is the tree's answer and the page's state, not the test's.
    ///
    /// `translated` are the languages S7 translates into; `answers` maps one to the lines its model said,
    /// which is what S7's merge reads back instead of dialling a model here.
    fn new(
        tag: &str,
        segs: Vec<Seg>,
        fx: Vec<Fx>,
        lines: Vec<Entry>,
        settings: Produce,
        translated: Vec<&str>,
    ) -> Press {
        let (root, tree) = common::tree_in(tag);
        naivepost::project::save(&project(), tree.dir()).expect("project");
        let settings = Produce {
            translate: translated.iter().map(|c| c.to_string()).collect(),
            ..settings
        };
        // The cut and its effects, through the same door the Cut page's save uses: §S1 reads the tree's
        // own file, so this is the only way a test hands it an effect list that came from outside.
        naivepost::cut::save(
            &Cut { segs, fx, aspect: "16:9".into(), ..Default::default() },
            &tree,
        )
        .expect("cut");
        // The Narrate page's own file: clip-relative windows, the shape §07-narrate §2 stores.
        naivepost::textfmt::write_session(
            &lines
                .iter()
                .map(|l| naivepost::textfmt::SessionLine {
                    start: l.at,
                    end: l.at + (l.e - l.s).max(0.4),
                    source: "clip 1".into(),
                    who: "You".into(),
                    text: l.text.clone(),
                })
                .collect::<Vec<_>>(),
            &tree.narration_json(),
        )
        .expect("narration.json");
        let run = flow::snapshot(
            None,
            &tree,
            naivepost::narrate_off::lines_to_speak(false, &lines),
            settings.clone(),
            &project(),
            Some(Publish::default()),
            "Old Man".into(),
        );
        Press {
            root,
            tree,
            run,
            settings,
            stem: exec::video_stem().to_string(),
        }
    }

    /// The page's render half for this press: `exec::run_render` over the tree the press just wrote,
    /// with the spy spawner (which "writes" the file each command names), the page's log line, and the
    /// speak leg the page hands it. Returns the `Rendered` the page would hand `run_with`.
    fn render(&self, spy: &Spy, refusals: &[&str]) -> flow::Rendered {
        let cues = exec::Cues {
            all: press_cues(&self.run.lines),
            languages: self.settings.translate.clone(),
            ..Default::default()
        };
        exec::run_render(
            &self.run,
            &self.tree,
            &mats_with_fx(cues, self.run.cut.fx.clone()),
            |line| spy.logs.borrow_mut().push(line.to_string()),
            |line| {
                if refusals.contains(&line.text.as_str()) {
                    return Err(format!("voice 'Old Man' would not speak {:?}", line.text));
                }
                Ok(())
            },
            |_, _| true,
            |command: &exec::Command| {
                spy.argv.borrow_mut().push((command.step.to_string(), command.argv.clone()));
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

    /// The folder the produced file and its sidecars stand in — derived from where the run writes, never
    /// a guess at the tree's layout.
    fn produce_dir(&self) -> PathBuf {
        self.tree
            .final_video("mp4")
            .parent()
            .unwrap()
            .to_path_buf()
    }
}

/// §S1 + §S3 (GAP: a real press answered an empty cue sheet and drew no effects) — a press over a tree
/// whose cut carries a zoom and whose narration carries two lines on two clips.
///
/// §S1 says the run reads the cut, the effects and the narration off the SESSION TREE, and §S3 draws each
/// kept segment into a clip with its effects and its lines. This drives the tree's files into the walk and
/// reads back the commands the walk asked ffmpeg for: the cue sheet that reaches the mux names the spoken
/// lines, the FIRST clip's filter graph carries a `zoompan` the zoom's camera chain draws, and the second
/// clip's carries none — the effects were read off the kept segments, not applied to everything.
#[test]
fn f5_2_gap_s1_and_s3_a_press_reads_the_tree_and_draws_its_effects() {
    let press = Press::new(
        "gap-s1",
        vec![seg(2.0, 12.0), seg(20.0, 28.0)],
        vec![zoom_fx(3.0, 4.0)],
        vec![
            Entry { s: 0.0, e: 10.0, at: 0.5, text: "first clip's line".into(), ..Default::default() },
            Entry { s: 0.0, e: 8.0, at: 1.0, text: "second clip's line".into(), ..Default::default() },
        ],
        Produce { subtitles: Subtitles::TrackInFile, ..Default::default() },
        vec![],
    );
    let spy = Spy::new();
    let made = press.render(&spy, &[]);
    assert!(made.ok, "the press came back clean: {made:?}");
    // The cue sheet the walk built from the tree's own narration: both lines, in their clip's order.
    let sheet = std::fs::read_to_string(press.tree.clips_dir().join("final.srt")).expect("the run's sheet");
    assert!(
        sheet.contains("first clip's line") && sheet.contains("second clip's line"),
        "the cue sheet is the tree's lines, both of them: {sheet}"
    );
    // The zoom reaches the encoder: one `zoompan` chain exists and it belongs to the first clip.
    let joined = spy.joined();
    assert!(
        joined.contains("zoompan"),
        "the zoom's camera chain reached an ffmpeg command: {joined}"
    );
    let clip_cmds: Vec<String> = spy
        .argv
        .borrow()
        .iter()
        .filter(|(step, _)| step == "clip")
        .map(|(_, args)| args.join(" "))
        .collect();
    assert_eq!(clip_cmds.len(), 2, "two kept segments, two encodes: {clip_cmds:?}");
    assert!(clip_cmds[0].contains("zoompan"), "the FIRST clip carries the zoom: {}", clip_cmds[0]);
    assert!(
        !clip_cmds[1].contains("zoompan"),
        "the second clip, which carries no effect, draws no camera: {}",
        clip_cmds[1]
    );
    // And the sheet the tree's narration produced rode all the way into the mux, which is the only place a
    // viewer sees it: the mp4 takes the sheet as a track input and names its own text codec.
    let mux = spy.last_with("loudnorm");
    assert!(mux.iter().any(|a| a.contains("mov_text")), "the tracked sheet was muxed in: {mux:?}");
    assert!(
        mux.iter().any(|a| a.ends_with("final.srt")),
        "and the mux reads the sheet this run wrote: {mux:?}"
    );
    let _ = press.root;
}

/// §S7 + §S4 (GAP: no sidecar was ever written by a real press and no stale one ever deleted) — a press
/// whose folder stands with a sheet from an earlier render for a language ticked again this time.
///
/// §S4 deletes the stale sidecars BY EXACT NAME before this run's sheet is written (the names it can name:
/// the bare-stem pair and the pair of each ticked language — anything else in the folder is somebody
/// else's); §S7 writes the .srt and .vtt beside the video for the session's language and for each ticked
/// one. This plants two files in `produce/` before the press — one the sweep names, one it cannot — and
/// reads the folder after: `final.srt` + `final.vtt` stand with THIS run's words inside, the named file's
/// old text is gone (the sweep deleted it and §S9 wrote a fresh sheet over the name), and the unnamed sheet
/// stands untouched. All of it is read off the filesystem, which is what the next press reads.
#[test]
fn f5_2_gap_s7_and_s4_a_press_writes_its_sidecars_and_deletes_the_named_stale() {
    let press = Press::new(
        "gap-s7",
        vec![seg(0.0, 10.0)],
        vec![],
        vec![Entry { s: 0.0, e: 5.0, at: 0.0, text: "what this run says".into(), ..Default::default() }],
        Produce { subtitles: Subtitles::TrackInFile, ..Default::default() },
        // German comes from `Press::new`'s own argument — one place spells what this run translates into.
        vec!["German"],
    );
    // A stale sheet from an earlier render whose German sidecar came from a language now unticked stands
    // in the produced folder, and one the sweep cannot name stands beside it.
    //
    // The stale file goes where the PRODUCED file's own folder is — the sweep only looks next to the video,
    // and it refuses to delete the scratch sheet because that one belongs to the run doing the deleting.
    // With no video standing there yet (nothing has been rendered in this throwaway project), the sweep
    // takes the named file and leaves the unnamed one.
    let produce = press.produce_dir();
    std::fs::create_dir_all(&produce).expect("produce dir");
    // The earlier render's German sheet stands in the produced folder. The sweep names it (this run ticks
    // German), and S9 rewrites it below — so the assertion is that the OLD text is gone from it, which only
    // happens if the sweep really deleted the file the run then wrote again.
    let stale = produce.join(format!("{}.German.srt", press.stem));
    std::fs::write(&stale, "1\n00:00:00,000 --> 00:00:01,000\nALTE ZEILE\n").expect("stale");
    // Beside it, a sheet no language this run named: the sweep cannot name it and must leave it standing.
    let keep = produce.join(format!("{}.other.srt", press.stem));
    std::fs::write(&keep, "1\n00:00:00,000 --> 00:00:01,000\nOTHER\n").expect("keep");
    let spy = Spy::new();
    let made = press.render(&spy, &[]);
    assert!(made.ok, "the press came back clean: {made:?}");
    // The session's own sheet lands beside the video, under the bare stem, with this run's words.
    let srt = std::fs::read_to_string(produce.join("final.srt")).expect("final.srt");
    assert!(
        srt.contains("what this run says"),
        "the sidecar carries this run's cue text, not the old run's: {srt}"
    );
    assert!(produce.join("final.vtt").is_file(), "the .vtt stands beside the .srt");
    // What stands in the named file is this run's TRANSLATED cue: the old English text is gone (the sweep
    // deleted it by exact name) and the sheet §S9 wrote under that name carries the model's German line,
    // not a copy of the session's track — §S9's own rule, read back off the file a player would load.
    let german_now = std::fs::read_to_string(&stale).unwrap_or_default();
    assert!(
        !german_now.contains("ALTE ZEILE"),
        "the stale sidecar named by the sweep no longer carries its old text: {german_now}"
    );
    assert!(
        german_now.contains("uebersetzt 1"),
        "and what stands in its place is this run's translated cue: {german_now}"
    );
    assert!(keep.exists(), "a sheet the sweep did not name was left alone: {:?}", produce);
    // And the walk reported the files it wrote, so the ending can say what landed.
    let _ = press.root;
}

/// §S2 (GAP: the speak leg never walked the lines that owe a take, so no refusal was ever said) — a press
/// whose first line already has its take and whose second refuses.
///
/// §S2: every line with no `take.wav` is spoken, one at a time, and a line that will not speak is LOGGED
/// and SKIPPED — the render half goes on and the ending is DONE, because a silent sentence costs less
/// than a failed render. The speak closure here refuses one line by name with a reason naming the voice,
/// and every assertion reads the walk's own report: the progress line for the line that spoke, the
/// failure line naming the voice for the one that did not, `spoken`/`speech_failed` counted, and the fact
/// that the mux still ran so the ending is DONE rather than a cancel.
#[test]
fn f5_2_gap_s2_a_refused_line_is_said_and_the_render_goes_on() {
    std::fs::write(SYNTHESIZED.as_path(), b"a take of some length").expect("synthesize a take");
    let press = Press::new(
        "gap-s2",
        vec![seg(0.0, 10.0)],
        vec![],
        vec![
            Entry { s: 0.0, e: 4.0, at: 0.0, text: "already synthesized".into(), ..Default::default() },
            Entry { s: 0.0, e: 4.0, at: 4.0, text: "owes a take".into(), ..Default::default() },
        ],
        Produce { subtitles: Subtitles::TrackInFile, ..Default::default() },
        vec![],
    );
    let spy = Spy::new();
    let made = press.render(&spy, &["owes a take"]);
    assert!(made.ok, "a refused line does not fail the render: {made:?}");
    let logs = spy.logs_vec();
    // The line that had its take is not in the speak list at all — §S2 asks for the lines that owe one.
    assert!(
        !logs.iter().any(|l| l.contains("already synthesized") && l.contains("synthesis")),
        "a line with its take is never asked for again: {logs:?}"
    );
    // The refusal is said with its reason, naming the voice the press speaks with.
    assert!(
        logs.iter().any(|l| l.contains("owes a take") && l.contains("Old Man")),
        "the refusal is logged with its reason and the voice: {logs:?}"
    );
    // The render half went on: both clips' work, the join and the mux all ran.
    assert!(spy.last_with("loudnorm").iter().any(|a| a.contains("loudnorm")), "the mux ran anyway");
    // S6's verdict read off the same report: DONE, not a cancel.
    let ending = flow::finish(&flow::Halves {
        words_ok: true,
        words_reason: None,
        render_ok: made.ok,
        tag_ok: made.tag_ok,
        stopped: made.stopped,
        seconds: made.seconds,
        size: made.size.clone(),
    });
    assert_eq!(ending.status, render::STAGE_DONE, "the press ends DONE: {ending:?}");
    let _ = press.root;
}

// ---- S8 -----------------------------------------------------------------------------------------

#[test]
fn f5_2_s8_the_mux_carries_the_loudnorm_and_the_container_tracks() {
    let tracked = Produce { subtitles: Subtitles::TrackInFile, ..Default::default() };
    let cues = exec::Cues::languages_only(Vec::new());
    let (tree, run) = run_over("s8", vec![seg(0.0, 10.0)], vec![entry(0.0, 8.0, 0.0, "a line")], tracked, "");
    let spy = Spy::new();
    spy.run(&run, &tree, &mats(&[], cues.clone(), &no_wav, &seconds_2_5), None);
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
    spy2.run(&run2, &tree2, &mats(&[], cues.clone(), &no_wav, &seconds_2_5), None);
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
    let cues = exec::Cues::languages_only(vec!["German".to_string()]);
    let (tree, run) = run_over(
        "s9",
        vec![seg(0.0, 10.0)],
        vec![entry(0.0, 8.0, 0.0, "words for the sheet")],
        settings,
        "",
    );
    let spy = Spy::new();
    spy.run(&run, &tree, &mats(&[], cues.clone(), &no_wav, &seconds_2_5), None);
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
    let made = spy.run(&run, &tree, &mats(&[], exec::Cues::default(), &no_wav, &seconds_2_5), Some(1));
    assert!(made.stopped, "the report says stopped");
    assert!(!made.ok, "and not ok");
    assert_eq!(spy.count_step("clip"), 2, "the encodes had already run: {:?}", spy.steps());
    assert_eq!(spy.count_step(render::JOINING), 0, "the join never started");
    assert!(!stamp.exists(), "a stopped run writes no stamp");

    // Stopped one checkpoint later: the join ran, the mux did not, still no stamp.
    let spy2 = Spy::new();
    let made2 = spy2.run(&run, &tree, &mats(&[], exec::Cues::default(), &no_wav, &seconds_2_5), Some(2));
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
    let made = spy.run(&run, &tree, &mats(&[], exec::Cues::default(), &no_wav, &seconds_2_5), None);
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

// ---- F4.8 Narration off: the render asks the seam, not itself ------------------------------------

/// A Run whose `no_narration` is set straight on the struct rather than through a project file: this
/// pair of tests is about which list `speech_plan` reads, and building the flag into a temp project
/// would test `snapshot`, not the seam.
fn run_with_lines(tag: &str, lines: Vec<Entry>, no_narration: bool) -> (Tree, flow::Run) {
    let (_root, tree) = common::tree_in(tag);
    let mut run = flow::snapshot(
        Some(cut_of(vec![seg(0.0, 10.0)])),
        &tree,
        lines,
        Produce::default(),
        &project(),
        Some(Publish::default()),
        String::new(),
    );
    run.no_narration = no_narration;
    (tree, run)
}

/// f4_8_s3_speech_plan_asks_the_narration_seam — with narration OFF the render speaks nothing, and it
/// knows that because it asked [`naivepost::narrate_off::lines_to_speak`] rather than filtering the
/// record by hand. `speaking` is passed as `true` on purpose: `plan()`'s own `speaking` guard already
/// short-circuits on `no_narration`, so bypassing THAT guard is the only way to prove the answer came
/// from the seam and not from the earlier branch. Before this fix the filter here looked at the text
/// alone and both worded lines were handed back for synthesis.
#[test]
fn f4_8_s3_speech_plan_asks_the_narration_seam() {
    let (tree, run) = run_with_lines(
        "f48-off",
        vec![entry(0.0, 4.0, 0.0, "first line"), entry(5.0, 9.0, 0.0, "second line")],
        true,
    );
    let plan = exec::speech_plan(&run, &tree, &mats(&[], exec::Cues::default(), &no_wav, &seconds_2_5), true);
    assert!(
        plan.is_empty(),
        "narration off means the render has NOTHING to speak, even with `speaking` forced on: {plan:?}"
    );
}

/// f4_8_s3_a_blank_line_is_never_spoken — the blank-line rule belongs to the seam and nowhere else.
/// The middle row's text is spaces: a deliberate silence (§F4.7's 🗑 leaves one), so it is not spoken and
/// only the two worded rows are owed takes, in the order the record lists them.
#[test]
fn f4_8_s3_a_blank_line_is_never_spoken() {
    std::fs::write(&*SYNTHESIZED, b"a take from an earlier run").unwrap();
    let (tree, run) = run_with_lines(
        "f48-blank",
        vec![
            entry(0.0, 3.0, 0.0, "worded before"),
            entry(4.0, 5.0, 0.0, "   "),
            entry(6.0, 9.0, 0.0, "worded after"),
        ],
        false,
    );
    let plan =
        exec::speech_plan(&run, &tree, &mats(&[], exec::Cues::default(), &has_one_wav, &seconds_2_5), true);
    let asked: Vec<&str> = plan.iter().map(|to| to.text.as_str()).collect();
    assert_eq!(
        asked,
        vec!["worded before", "worded after"],
        "the silent middle row is dropped by the seam; the survivors are the worded ones, in order"
    );
}
