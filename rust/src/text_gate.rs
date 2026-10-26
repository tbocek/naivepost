//! §04-prepare F1.6 S6 (new): the text gate. The pixel pass (S2's `scdet`) is the rough cut: it catches every
//! hard switch for free, but a bullet added to a slide or a menu closing scores under its threshold, and a scroll
//! or a webcam person moving scores over it. The gate asks the decision model (Settings → Decision, a fast vision
//! model; empty, the main model) one question about two frames of the same recording: did the TEXT on screen change? With it:
//!
//! 1. every pixel-pass change is judged across (the frame before against the frame after); a SAME drops it;
//! 2. every scene left is probed at its start, its end and every [`PROBE_GAP_S`] between, and each pair of
//!    neighbouring probes judged; a CHANGED pair is halved and both halves judged again, down to [`MIN_SPAN_S`],
//!    so each change inside a scene is found within a second;
//! 3. changes no further apart than [`RUN_MERGE_S`] are one run (a scroll), kept as its LAST point: the settled
//!    view.
//!
//! What it finds goes to `textscenes.tsv` beside `scenes.tsv`; F1.7 then sends one frame per change and one every
//! [`HEARTBEAT_S`] (instead of one every Freq), see [`changes_for_describe`]. A probe of the 06-news footage
//! (2026-10-25): 9 of 9 text changes, no false alarm, 54 calls for two recordings, against 250 frames described
//! at Freq 1 s.
//!
//! The algorithm is pure over a `judge` closure ([`run`]); the live judge is [`crate::text_gate_call`].

use std::path::PathBuf;

use crate::layout::Tree;

/// The step and prompt key ([`crate::prompts::KEYS`]).
pub const STEP: &str = "textgate";
/// The walk's program word for a gate row ([`crate::run_stages`]).
pub const TEXT_GATE_TASK: &str = "naivepost:textgate";
/// Longest stretch of a scene left unjudged: probes are at most this far apart.
pub const PROBE_GAP_S: f64 = 20.0;
/// A CHANGED pair is halved until it is this short; the change is placed at its later frame.
pub const MIN_SPAN_S: f64 = 1.0;
/// Changes no further apart than this are one run (a scroll), kept as the run's last point.
pub const RUN_MERGE_S: f64 = 1.0;
/// With the gate's changes, F1.7 sends one frame per change and one every this many seconds between.
pub const HEARTBEAT_S: f64 = 20.0;
/// The gate gives up after this many calls if every one failed (the server is not there): the scenes stay the
/// pixel pass's rather than every judge being asked into a dead socket.
const GIVE_UP_AFTER: usize = 3;

/// Who found a change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Found {
    /// The pixel pass, confirmed by the gate.
    Pixel,
    /// The gate, inside a scene the pixel pass saw as one.
    Text,
}

impl Found {
    fn word(self) -> &'static str {
        match self {
            Found::Pixel => "pixel",
            Found::Text => "text",
        }
    }
}

/// What the gate came to for one recording.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Gate {
    /// The scene changes, in seconds into the file, oldest first, each with who found it.
    pub changes: Vec<(f64, Found)>,
    /// Judge calls asked (cached answers included).
    pub calls: usize,
    /// Pixel-pass changes the gate confirmed and dropped.
    pub kept: usize,
    pub dropped: usize,
    /// Judges that failed, each its own sentence; a failed judge keeps a pixel change and finds nothing.
    pub failures: Vec<String>,
}

/// How far the gate has got through one recording, said after every judge: the bar moves inside the stage and
/// the status line says where, instead of standing on the recording's name for the minutes its judges take.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Reached {
    /// 1 while the pixel pass's changes are judged, 2 while the scenes are probed and halved.
    pub pass: u8,
    /// Seconds into the recording of the last frame judged.
    pub at: f64,
    pub duration: f64,
    /// Judges asked so far.
    pub judges: usize,
}

impl Reached {
    /// The share of this recording's gate done: each pass sweeps the recording once, and they share it evenly.
    pub fn fraction(&self) -> f64 {
        let swept = if self.duration > 0.0 {
            (self.at / self.duration).clamp(0.0, 1.0)
        } else {
            1.0
        };
        ((f64::from(self.pass.clamp(1, 2)) - 1.0 + swept) / 2.0).clamp(0.0, 1.0)
    }

    /// "pass 2 of 2, 12:30 of 45:00, 87 judge(s)".
    pub fn words(&self) -> String {
        let clock = |s: f64| {
            let s = s.max(0.0).round() as u64;
            format!("{}:{:02}", s / 60, s % 60)
        };
        format!(
            "pass {} of 2, {} of {}, {} judge(s)",
            self.pass,
            clock(self.at),
            clock(self.duration),
            self.judges
        )
    }
}

fn snap_down(at: f64, grid: f64) -> f64 {
    ((at / grid + 1e-9).floor() * grid).max(0.0)
}

fn snap_up(at: f64, grid: f64) -> f64 {
    (at / grid - 1e-9).ceil() * grid
}

/// The gate over one recording: `rough` the pixel pass's changes, `duration` the file's length as F1.7 reads it
/// (last frame plus one grid step), `grid` the extraction grid; `judge(a, b)` answers whether the text changed
/// between the frames at `a` and `b`. Errs only when every one of the first calls failed.
pub fn run(
    rough: &[f64],
    duration: f64,
    grid: f64,
    judge: &mut dyn FnMut(f64, f64) -> Result<bool, String>,
    reached: &mut dyn FnMut(Reached),
) -> Result<Gate, String> {
    let mut gate = Gate::default();
    let last = snap_down(duration - grid, grid);
    if last <= 0.0 {
        return Ok(gate);
    }
    let mut ask = |gate: &mut Gate, a: f64, b: f64| -> Option<bool> {
        gate.calls += 1;
        match judge(a, b) {
            Ok(changed) => Some(changed),
            Err(why) => {
                gate.failures.push(why);
                None
            }
        }
    };
    let dead = |gate: &Gate| gate.calls >= GIVE_UP_AFTER && gate.failures.len() == gate.calls;

    // 1. The pixel pass's changes, each judged across.
    let mut starts = vec![0.0];
    for &at in rough.iter().filter(|at| **at > 0.0 && **at < duration) {
        let (before, after) = (snap_down(at - grid, grid), snap_up(at, grid).min(last));
        let keep = after <= before || ask(&mut gate, before, after) != Some(false);
        if dead(&gate) {
            return Err(gate.failures[0].clone());
        }
        reached(Reached {
            pass: 1,
            at: after,
            duration,
            judges: gate.calls,
        });
        if keep {
            gate.kept += 1;
            gate.changes.push((after, Found::Pixel));
            starts.push(after);
        } else {
            gate.dropped += 1;
        }
    }

    // 2. Each scene probed, CHANGED pairs halved.
    let mut found: Vec<f64> = Vec::new();
    for (index, start) in starts.iter().enumerate() {
        let end = starts.get(index + 1).copied().unwrap_or(duration);
        let (first, final_) = (snap_up(*start, grid), snap_down(end - grid, grid).min(last));
        if final_ - first < grid {
            continue;
        }
        let pieces = ((final_ - first) / PROBE_GAP_S).ceil().max(2.0) as usize;
        let mut probes: Vec<f64> = (0..=pieces)
            .map(|i| snap_down(first + (final_ - first) * i as f64 / pieces as f64, grid))
            .collect();
        probes.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
        for pair in probes.windows(2) {
            if ask(&mut gate, pair[0], pair[1]) == Some(true) {
                halve(&mut gate, &mut ask, pair[0], pair[1], grid, &mut found);
            }
            if dead(&gate) {
                return Err(gate.failures[0].clone());
            }
            reached(Reached {
                pass: 2,
                at: pair[1],
                duration,
                judges: gate.calls,
            });
        }
    }
    gate.changes
        .extend(found.into_iter().map(|at| (at, Found::Text)));
    gate.changes
        .sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

    // 3. Runs kept as their last point.
    let mut merged: Vec<(f64, Found)> = Vec::new();
    for change in gate.changes.drain(..) {
        match merged.last_mut() {
            Some(previous) if change.0 - previous.0 <= RUN_MERGE_S + 1e-9 => *previous = change,
            _ => merged.push(change),
        }
    }
    gate.changes = merged;
    Ok(gate)
}

/// A CHANGED pair `a`..`b`: shorter than [`MIN_SPAN_S`] places the change at `b`; longer is halved and each
/// half that changed is halved again, so two changes inside one pair are both found.
fn halve(
    gate: &mut Gate,
    ask: &mut dyn FnMut(&mut Gate, f64, f64) -> Option<bool>,
    a: f64,
    b: f64,
    grid: f64,
    found: &mut Vec<f64>,
) {
    let middle = snap_down((a + b) / 2.0, grid);
    if b - a <= MIN_SPAN_S + 1e-9 || middle <= a || middle >= b {
        found.push(b);
        return;
    }
    for (from, to) in [(a, middle), (middle, b)] {
        if ask(gate, from, to) == Some(true) {
            halve(gate, ask, from, to, grid, found);
        }
    }
}

/// `textscenes.tsv`, beside the pixel pass's `scenes.tsv`.
pub fn path(tree: &Tree, lane: &str) -> PathBuf {
    let scenes = tree.scenes_tsv(lane);
    scenes.with_file_name("textscenes.tsv")
}

/// Write what the gate found: one `seconds\tpixel|text` line per change, through the tree's writer.
pub fn write(tree: &Tree, lane: &str, gate: &Gate) -> Result<(), String> {
    let body: String = gate
        .changes
        .iter()
        .map(|(at, found)| format!("{at:.3}\t{}\n", found.word()))
        .collect();
    let file = path(tree, lane);
    let rel = file
        .strip_prefix(tree.dir())
        .map_err(|_| format!("{}: not inside the project", file.display()))?;
    tree.write_file(rel, body.as_bytes())
}

/// The scene changes F1.7 describes by, and the spacing between frames inside a scene: the gate's, with
/// [`HEARTBEAT_S`], when `textscenes.tsv` exists and is no older than `scenes.tsv` (a re-extraction makes it
/// stale); else `None`, and F1.7 keeps the pixel pass's changes and Freq.
pub fn changes_for_describe(tree: &Tree, lane: &str) -> Option<Vec<f64>> {
    let gated = path(tree, lane);
    let modified = |file: &std::path::Path| std::fs::metadata(file).and_then(|m| m.modified()).ok();
    let (Some(gate_time), scenes_time) = (modified(&gated), modified(&tree.scenes_tsv(lane)))
    else {
        return None;
    };
    if scenes_time.is_some_and(|scenes| scenes > gate_time) {
        return None;
    }
    let text = std::fs::read_to_string(&gated).ok()?;
    Some(
        text.lines()
            .filter_map(|line| line.split('\t').next()?.trim().parse::<f64>().ok())
            .collect(),
    )
}

/// The log line for one recording.
pub fn done_log(lane: &str, gate: &Gate) -> String {
    let text = gate
        .changes
        .iter()
        .filter(|(_, found)| *found == Found::Text)
        .count();
    let mut line = format!(
        ">>> [{lane}] text gate: {} scene change(s) -- {} of the pixel pass's kept, {} dropped as the same text, {text} found in the text; {} judge(s)",
        gate.changes.len(),
        gate.kept,
        gate.dropped,
        gate.calls
    );
    if let Some(first) = gate.failures.first() {
        line.push_str(&format!(
            "; {} judge(s) failed, the first: {first}",
            gate.failures.len()
        ));
    }
    line
}

/// The gate's stage: one row per footage source, always (the decision model judges, else the main one). Runs
/// after the frames and before describe, for every recording first: the decision model and the describe model
/// may be two models on one server that holds one at a time, and alternating them per recording would load
/// each again for every one.
pub fn stage(project: &crate::project::Project) -> crate::run_stages::Stage {
    let tasks = project
        .sources
        .iter()
        .filter(|source| source.footage)
        .enumerate()
        .map(|(index, source)| crate::run_stages::Task {
            label: format!(
                "reading the text of {}",
                crate::audio_task::source_lane(&source.path)
            ),
            program: TEXT_GATE_TASK.to_string(),
            args: vec![source.path.clone(), index.to_string()],
            already_done: false,
            session: None,
        })
        .collect();
    crate::run_stages::Stage {
        track: crate::runqueue::TRACK_DESCRIBE,
        name: "textgate",
        phase: 2,
        of: 2,
        kind: "chunk",
        tasks,
    }
}

/// One row of the walk: the gate over one footage source, its answer written to `textscenes.tsv`. A gate that
/// cannot run (no frames, a server that never answers) says why and leaves the pixel pass's
/// scenes in force: F1.7 describes either way.
pub fn run_task(
    task: &crate::run_stages::Task,
    log: &mut impl FnMut(&str),
    reached: &mut dyn FnMut(Reached),
) -> Result<(), String> {
    let paths = crate::settings::from_environment()
        .ok_or("nowhere to keep the settings -- neither XDG_CONFIG_HOME nor HOME is set")?;
    let conf = crate::settings::read(&paths).unwrap_or_default();
    let session = crate::startup::open_dir();
    let tree =
        Tree::new(&session).map_err(|_| format!("{}: not a project folder", session.display()))?;
    let source = task
        .args
        .first()
        .cloned()
        .ok_or("the gate row names no source")?;
    let lane = crate::audio_task::source_lane(&source);
    let index: usize = task.args.get(1).and_then(|n| n.parse().ok()).unwrap_or(0);
    gate_source(&tree, &paths, &conf, &lane, index, log, reached);
    Ok(())
}

/// The gate for one source, from a tree and settings the caller resolved (a test drives this directly).
pub fn gate_source(
    tree: &Tree,
    paths: &crate::settings::Paths,
    conf: &crate::settings::Conf,
    lane: &str,
    index: usize,
    log: &mut impl FnMut(&str),
    reached: &mut dyn FnMut(Reached),
) -> Option<Gate> {
    let frames = crate::describe::frames_on_disk(&tree.frames_dir(lane));
    if frames.is_empty() {
        log(&format!(
            ">>> [{lane}] text gate: no frames -- the pixel pass's scenes stay"
        ));
        return None;
    }
    let project = crate::project::load(tree.dir()).ok();
    let start = crate::describe_scale::clock_start(project.as_ref(), index);
    // The judge is shown the same 896-wide copies describe sends; made once, kept for describe.
    if let Err(why) = crate::describe_scale::ensure(tree, conf, lane, start, &frames, log) {
        log(&format!(
            "!!! [{lane}] text gate: {why} -- the pixel pass's scenes stay"
        ));
        return None;
    }
    // The spacing of the frames this source HAS: the 250 ms grid, or one a second from an older extraction; the
    // probes and the halving land only on pictures that exist.
    let grid = frames
        .windows(2)
        .map(|pair| pair[1] - pair[0])
        .filter(|step| *step > 1e-6)
        .fold(f64::INFINITY, f64::min);
    let grid = if grid.is_finite() {
        grid
    } else {
        crate::describe::GRID
    };
    let duration = frames.last().copied().unwrap_or(0.0) + grid;
    let rough = crate::describe::read_pixel_changes(tree, lane);
    // The scaled copies are named from the stored folder's first instant: read once, not per judge.
    let base = crate::describe_scale::name_base(tree, lane);
    let mut judge =
        |a: f64, b: f64| crate::text_gate_call::judge(tree, paths, conf, lane, base, a, b);
    match run(&rough, duration, grid, &mut judge, reached) {
        Ok(gate) => {
            if let Err(why) = write(tree, lane, &gate) {
                log(&format!("!!! [{lane}] text gate: {why}"));
                return None;
            }
            log(&done_log(lane, &gate));
            Some(gate)
        }
        Err(why) => {
            log(&format!(
                "!!! [{lane}] text gate: the model did not answer ({why}) -- the pixel pass's scenes stay"
            ));
            None
        }
    }
}
