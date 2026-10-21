//! F2.5 S3/S4 — the lane pipelines: one real process per separate recording, in sync.
//!
//! The spec's mix clause says each separate recording plays as ITS OWN pipeline, in sync; a lane the scene
//! under the line silences is never started; a lane started under a scene boundary is seeked, with a stop at
//! it. Every decision of that sentence is already arithmetic in [`crate::cut_hear`] (`lane_start`,
//! `lane_seconds`, `stop_at_boundary`, `mix_at`): this module holds none of it. What lives here is the hand
//! that starts and stops processes to match an answer already made — which is why nothing in here reads a
//! `quiet` list or a rate: it obeys [`cut_hear::LaneStart`] and nothing else.
//!
//! # Which program
//!
//! Same decision as [`crate::play_back`], for the same reason: no media library is linked (Cargo.toml has
//! no gstreamer/mpv/vlc dependency), so a lane is a subprocess of the box's own player, resolved through
//! [`play_back::player_program`] — Settings first, then `NAIVEPOST_PLAYER`, then the bare name. A test
//! points that same seam at a fake script (the shape `tests/suite/f2_1_recording_play_widgets.rs` uses), so no
//! test ever asks a helper for its answer and no program path asks a test.
//!
//! # Why `-s` and `--volume` are the whole sync story
//!
//! `gst-play-1.0 --help-all` lists `-s, --start-position  Start position in seconds.` and
//! `--volume  Volume`. So a lane joins the session by being STARTED AT ITS OWN SECOND of the same instant —
//! [`cut_hear::lane_seconds`] — and its loudness is playbin's volume PROPERTY rather than a stop, which is
//! what lets a gain follow an effect without tearing the stream down. A stop at a scene boundary is a stop
//! of the process: after the boundary the lane has nothing of its own left to say under that scene, and a
//! process kept alive past it would play on into silence nobody asked for.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use crate::cut::Lane;
use crate::cut_hear::{self, LaneStart};
use crate::play_back;

/// One live process per lane NAME. A map, not a Vec, because two lanes of one file are two pipelines and
/// must be addressed apart (§A: one lane per separate recording or extra track).
fn held() -> &'static Mutex<HashMap<String, u32>> {
    static SLOT: OnceLock<Mutex<HashMap<String, u32>>> = OnceLock::new();
    SLOT.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The file behind a lane, resolved the one way this project resolves a path (`layout::resolve`, the
/// session folder as the project — the shape `ui::cut_fps::footage_file` uses), and only if it is really
/// there. A lane whose file vanished is not a pipeline: starting the player on a missing URI produces a
/// process that errors out and holds a pid, which is worse than not starting it.
pub fn lane_file(root: &Path, lane: &Lane) -> Option<PathBuf> {
    let project = crate::startup::session_dir(root);
    let path = crate::layout::resolve(root, &project, &lane.src);
    path.is_file().then_some(path)
}

/// The argv for one lane's pipeline: quiet, seeked to `from_secs` (its own second of this instant, from
/// [`cut_hear::lane_seconds`]), and at `gain` through the player's volume property.
///
/// Times go out to a tenth of a second like every other leg of this app ([`play_back::command_for`]): finer
/// is below what a preview can show, and a lane placed by a file-name stamp never means hundredths.
pub fn lane_command(file: &str, from_secs: f64, gain: f64) -> Vec<String> {
    vec![
        "--no-position".to_string(),
        "-s".to_string(),
        format!("{:.1}", (from_secs * 10.0).round() / 10.0),
        "--volume".to_string(),
        format!("{:.3}", gain.max(0.0)),
        file.to_string(),
    ]
}

/// Start one lane's pipeline and remember its pid, through the program the app resolves
/// ([`play_back::player_program`]: Settings, then `NAIVEPOST_PLAYER`, then the bare name). Anything already
/// running for that lane is stopped first: a re-place under a new scene replaces the run rather than
/// doubling the sound.
pub fn start_lane(
    name: &str,
    file: &str,
    from_secs: f64,
    gain: f64,
    log: &dyn Fn(&str),
) -> Result<u32, String> {
    stop_lane(name);
    let program = play_back::player_program();
    let args = lane_command(file, from_secs, gain);
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    log(&crate::subprocess::log_line(&program, &refs));
    let child = crate::subprocess::spawn_piped(&program, &refs)
        .map_err(|err| format!("{} failed: {err}", crate::subprocess::shown(&program, &refs)))?;
    let pid = child.id();
    held()
        .lock()
        .expect("the lane pipeline slot")
        .insert(name.to_string(), pid);
    // Not waited on: the lane plays until it is stopped. Dropping the child leaves it running.
    std::mem::forget(child);
    Ok(pid)
}

/// The same ask through a program the CALLER names, so a test can point a lane at a fake player script
/// without touching `NAIVEPOST_PLAYER`, which the footage leg shares with every other press in the same
/// binary (one process-wide env var, two legs wanting different answers). Nothing in `src/` calls this:
/// the live path goes through [`start_lane`] and the one resolved program.
fn start_lane_with(
    program: &str,
    name: &str,
    file: &str,
    from_secs: f64,
    gain: f64,
    log: &dyn Fn(&str),
) -> Result<u32, String> {
    stop_lane(name);
    let args = lane_command(file, from_secs, gain);
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    log(&crate::subprocess::log_line(program, &refs));
    let child = crate::subprocess::spawn_piped(program, &refs)
        .map_err(|err| format!("{program} failed: {err}"))?;
    let pid = child.id();
    held()
        .lock()
        .expect("the lane pipeline slot")
        .insert(name.to_string(), pid);
    std::mem::forget(child);
    Ok(pid)
}

/// Stop one lane's pipeline. Answers whether there was one. SIGTERM like every other leg here, so the
/// player tears its own pipeline down instead of leaving the file half-held for the next start.
///
/// The signal goes out and the slot is released without waiting for the child to die (nothing here `wait`s on
/// it), so a caller that stops one lane and starts another under a different name finds no live entry the
/// moment it returns -- but the old process can still be alive for a few milliseconds afterwards.
pub fn stop_lane(name: &str) -> bool {
    let mut slot = held().lock().expect("the lane pipeline slot");
    match slot.remove(name) {
        Some(pid) => {
            unsafe { libc::kill(pid as i32, libc::SIGTERM) };
            true
        }
        None => false,
    }
}

/// The pid running this lane, if any.
pub fn lane_pid(name: &str) -> Option<u32> {
    held().lock().expect("the lane pipeline slot").get(name).copied()
}

/// Every live lane pipeline as `(name, pid)`, sorted by name. Test-only reader: the program addresses a
/// pipeline by name through [`lane_pid`] and never needs the whole table.
pub fn lane_pids_for_test() -> Vec<(String, u32)> {
    let mut live: Vec<(String, u32)> = held()
        .lock()
        .expect("the lane pipeline slot")
        .iter()
        .map(|(name, pid)| (name.clone(), *pid))
        .collect();
    live.sort();
    live
}

/// Stop every lane pipeline; answers how many were running. Called when ▶ pauses: a paused preview must
/// hold no sound at all, and the tick cannot stop them itself because it returns before this once the
/// transport is off.
pub fn halt_all() -> usize {
    let names: Vec<String> = held()
        .lock()
        .expect("the lane pipeline slot")
        .keys()
        .cloned()
        .collect();
    names.iter().filter(|name| stop_lane(name)).count()
}

/// What one tick decides to DO with one lane's pipeline. Two refusals look the same from outside — silence —
/// but they differ in whether a process has to go away, which is what a caller acts on.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LaneAction {
    /// Start it at `from` (its own clock), stop at `stop` (0 = no stop), at `gain`.
    Started { from: f64, stop: f64, gain: f64 },
    /// It must not be playing: silence said so. Stop it if something IS running.
    Stopped,
    /// Nothing to do — already silent, nothing running.
    Held,
}

/// The mix's answer turned into process work, for every lane in the order the lanes were given. PURE: it
/// starts and stops nothing, so "a silenced lane is never started" is checkable without a player.
///
/// A lane with no readable file is `Held`: nothing could be started anyway, and inventing a start for it
/// would report a pipeline that does not exist.
///
/// The `Stopped` arm tests what is LIVE rather than what was asked (`lane_pid`), which is what makes this
/// function honest when called twice over one unchanged mix — a ten-times-a-second tick asks the same thing
/// nine times out of ten, and the second through ninth call must do nothing:
///
/// ```
/// use naivepost::cut::Lane;
/// use naivepost::cut_hear::{self, LaneStart};
/// use naivepost::lane_hear::{plan_lanes, LaneAction};
/// use std::collections::BTreeMap;
/// let lanes = vec![Lane { name: "mic.wav".into(), src: "mic.wav".into(), ..Default::default() }];
/// let plan = |start| {
///     plan_lanes(
///         &cut_hear::Mix {
///             footage_muted: true,
///             lane_starts: vec![("mic.wav".into(), start)],
///             gain: 1.0,
///             rate: 1.0,
///             bed_silenced: false,
///             sound_needs_two_clocks: false,
///         },
///         &lanes,
///         &BTreeMap::new(),
///         1.0,
///     )
/// };
/// // Nothing running and nothing to start: silence costs no process at all.
/// assert_eq!(plan(LaneStart::Silenced)[0], ("mic.wav".into(), LaneAction::Held));
/// // A seek with no readable file is held the same way — there is nothing to hand a pipeline.
/// assert_eq!(
///     plan(LaneStart::Seek { from: 3.0, stop: 0.0 })[0],
///     ("mic.wav".into(), LaneAction::Held)
/// );
/// // The same seek WITH a file on disk is a start, at the lane's own second and the mix's gain.
/// let root = std::env::temp_dir().join(format!("naivepost-lane-hear-doc-{}", std::process::id()));
/// std::fs::create_dir_all(root.join("audio")).expect("an audio folder");
/// std::fs::write(root.join("audio/mic.wav"), b"pcm").expect("the recording");
/// let found = [("mic.wav".to_string(), root.join("audio/mic.wav"))]
///     .into_iter()
///     .collect();
/// let heard = plan_lanes(
///     &cut_hear::Mix {
///         footage_muted: false,
///         lane_starts: vec![("mic.wav".into(), LaneStart::Seek { from: 4.5, stop: 9.0 })],
///         gain: 0.8,
///         rate: 1.0,
///         bed_silenced: false,
///         sound_needs_two_clocks: false,
///     },
///     &lanes,
///     &found,
///     0.8,
/// );
/// assert_eq!(
///     heard[0],
///     ("mic.wav".into(), LaneAction::Started { from: 4.5, stop: 9.0, gain: 0.8 })
/// );
/// let _ = std::fs::remove_dir_all(&root);
/// ```
pub fn plan_lanes(
    mix: &cut_hear::Mix,
    lanes: &[Lane],
    files: &BTreeMap<String, PathBuf>,
    gain: f64,
) -> Vec<(String, LaneAction)> {
    lanes
        .iter()
        .map(|lane| {
            let answer = mix
                .lane_starts
                .iter()
                .find(|(name, _)| *name == lane.name)
                .map(|(_, start)| *start);
            // Only the SEEK answer starts a pipeline: `Silenced` (the scene under the line does not hear this
            // lane) and `Nothing` (its file has nothing at this second) are both silence, which is what makes
            // "never started when silenced" true without this function knowing which of the two it saw.
            let action = match answer {
                Some(LaneStart::Seek { from, stop }) if files.contains_key(&lane.name) => {
                    LaneAction::Started { from, stop, gain }
                }
                // Silenced, out of file, or its file is gone: silent either way, and only a live process
                // needs stopping.
                _ if lane_pid(&lane.name).is_some() => LaneAction::Stopped,
                _ => LaneAction::Held,
            };
            (lane.name.clone(), action)
        })
        .collect()
}

/// Drive the real pipelines to match one tick's mix: resolve each lane's file, ask [`plan_lanes`], and
/// start or stop for real. Returns the sentences the caller should put on the status row — empty today,
/// because a lane starting or stopping is not news the spec asks to be said; the silence sentence is
/// `cut_hear`'s business (F2.5 S1's badge **8**).
pub fn apply(
    mix: &cut_hear::Mix,
    lanes: &[Lane],
    root: &Path,
    gain: f64,
    log: &dyn Fn(&str),
) -> Vec<String> {
    apply_with(mix, lanes, root, gain, log, None)
}

/// [`apply`], with the player program overridable for a test ([`start_lane_for_test`]). `None` means the
/// live path, which goes through [`start_lane`] and the one resolved program. A test that names its own
/// script keeps it out of the env var the footage leg shares with every other press in the same binary.
fn apply_with(
    mix: &cut_hear::Mix,
    lanes: &[Lane],
    root: &Path,
    gain: f64,
    log: &dyn Fn(&str),
    program: Option<&str>,
) -> Vec<String> {
    let files: BTreeMap<String, PathBuf> = lanes
        .iter()
        .filter_map(|lane| lane_file(root, lane).map(|file| (lane.name.clone(), file)))
        .collect();
    for (name, action) in plan_lanes(mix, lanes, &files, gain) {
        match action {
            LaneAction::Started { from, stop: _, gain } => {
                let Some(file) = files.get(&name) else { continue };
                let text = file.display().to_string();
                // Two doors, one argv: the live call resolves the program itself, the test hands one over.
                let asked = match program {
                    Some(program) => start_lane_with(program, &name, &text, from, gain, log),
                    None => start_lane(&name, &text, from, gain, log),
                };
                if let Err(why) = asked {
                    // §05-cut#8 bullet 2: a lane that drops out is NAMED, because the rest of the preview
                    // carries on and a silent row nobody can identify is not a local failure — it is a mystery.
                    log(&crate::preview::mix_lane_failure(&name, &why));
                }
            }
            LaneAction::Stopped => {
                stop_lane(&name);
            }
            LaneAction::Held => {}
        }
    }
    Vec::new()
}

/// A test's way to drive one lane's real pipeline through a program IT names, so a lane can be aimed at a
/// fake player script without touching `NAIVEPOST_PLAYER`, which the footage leg shares with every other
/// press in the same test binary. Records exactly what a live start would record: the pid under the lane's
/// name, and nothing else.
pub fn start_lane_for_test(
    program: &str,
    name: &str,
    file: &str,
    from_secs: f64,
    gain: f64,
) -> Result<u32, String> {
    start_lane_with(program, name, file, from_secs, gain, &|_| {})
}

/// [`apply`] with the player program named by the caller — the same seam as [`start_lane_for_test`], for the
/// case where a whole tick's worth of lanes has to land on one fake script.
pub fn apply_for_test(
    mix: &cut_hear::Mix,
    lanes: &[Lane],
    root: &Path,
    gain: f64,
    log: &dyn Fn(&str),
    program: &str,
) -> Vec<String> {
    apply_with(mix, lanes, root, gain, log, Some(program))
}

/// S4's stop at the boundary, applied: a lane whose seek carried a stop that the line has now passed gets
/// its process stopped, so it falls silent AT the boundary rather than waiting for the next re-place to
/// notice. Returns the lanes it stopped, by name.
///
/// The comparison is in the lane's own clock on both sides: `stop` is a file second
/// ([`cut_hear::stop_at_boundary`]) and the line's present second for that lane is
/// [`cut_hear::lane_seconds`], so a lane slid by a right-drag still lands on its own boundary.
pub fn stop_past_boundary(mix: &cut_hear::Mix, at_session_s: f64, lanes: &[Lane]) -> Vec<String> {
    let mut stopped = Vec::new();
    for lane in lanes {
        let Some((_, LaneStart::Seek { stop, .. })) = mix
            .lane_starts
            .iter()
            .find(|(name, _)| *name == lane.name)
        else {
            continue;
        };
        if stop <= &0.0 {
            continue; // no stop: the lane runs to the end of what its file has
        }
        if cut_hear::lane_seconds(lane, at_session_s) >= *stop && stop_lane(&lane.name) {
            stopped.push(lane.name.clone());
        }
    }
    stopped
}

/// One tick's worth of hearing, in the order the spec's clauses fall: boundaries first, then the whole mix,
/// then the footage's own sound. PURE apart from the processes it drives — every decision came from
/// `cut_hear::mix_at`, and nothing here reads a `quiet` list, a rate or a gain of its own.
///
/// Order matters once: a lane whose boundary the line just crossed is stopped BEFORE the mix is applied,
/// because `apply` would otherwise see a live pid and a fresh `Seek` under the NEW scene and restart the
/// lane the boundary was meant to silence. The footage ask goes last so it lands at the second the tick
/// actually settled on.
pub fn tick(
    mix: &cut_hear::Mix,
    lanes: &[Lane],
    root: &Path,
    at_session_s: f64,
    log: &dyn Fn(&str),
) {
    stop_past_boundary(mix, at_session_s, lanes);
    apply(mix, lanes, root, mix.gain, log);
    play_back::set_muted(mix.footage_muted, mix.gain, at_session_s);
}

/// [`tick`] with the player program named by the caller — the seam a test uses to drive a whole tick against
/// a fake script instead of the resolved player (see [`apply_for_test`] for why the program is an argument).
/// Same three steps in the same order; only where the program comes from differs.
pub fn tick_for_test(
    mix: &cut_hear::Mix,
    lanes: &[Lane],
    root: &Path,
    at_session_s: f64,
    log: &dyn Fn(&str),
    program: &str,
) {
    stop_past_boundary(mix, at_session_s, lanes);
    apply_with(mix, lanes, root, mix.gain, log, Some(program));
    play_back::set_muted(mix.footage_muted, mix.gain, at_session_s);
}
