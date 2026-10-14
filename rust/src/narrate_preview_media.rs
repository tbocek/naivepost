//! F4.5 S1/S3 — what the picture's player is actually asked to play, and at what level.
//!
//! [`crate::narrate_preview_leg`] owns the two children and the tick; this file answers the two questions it
//! must not answer itself: WHICH files the cut's picture is made of, and WHAT GAIN that sound runs at. Both
//! answers go into one argv, so a duck typed into the project is a number the player was given rather than a
//! number in a log line (the gate's complaint about `--volume` never leaving the log).
//!
//! # What the player's argv can express
//!
//! Measured on this container's `gst-play-1.0 --help-all` (GStreamer 1.28.3): there IS `--volume`, and there
//! is NO rate flag and NO mute flag (`-i/--instant-rate-changes` only makes *its own* rate changes instant).
//! So of S3's three promises:
//! - **duck** → `--volume=<gain>` (this module's [`bed_gain`]);
//! - **lanes and hushes** → also `--volume`: a second whose scene does not hear its own recording gets
//!   `0.000`, which is muting by property exactly as §05-cut#8 requires — the stream is never torn down per
//!   tick, the same reason [`crate::cut_hear::footage_sound_heard`] is a flag and not a stop;
//! - **speed at seeks** → `-s`: the re-cue lands on the second the app's clock says, at the rate
//!   [`crate::narrate_preview::seek_rate`] answered, which is what "apply speed effects at seeks" means for a
//!   player that cannot be told a rate. This mirrors [`crate::cut_play_back::set_rate`]'s documented decision
//!   for the Cut page's player, and for the same reason: the app's arithmetic is unaffected.
//!
//! The flag spelling otherwise stays the Cut page's ([`crate::play_back::command_for`]) so the two players can
//! never drift in how they are addressed.

use crate::cut::{Cut, Seg};
use crate::{cut_hear, layout, narrate_preview, play_back, preview, project, startup};

/// The footage list the picture plays: one entry per kept clip, in cut order, each resolved to a file that
/// exists. DECISION (the spec names no file for a kept clip — inventory §A.3 says only "Two players: video
/// (n.player) and narration audio (n.voice)"): the render builds each kept clip from the recording its camera
/// row carries ([`crate::timeline::kept_footage_recordings`]'s `cam<N>` naming), so the preview asks for the
/// same thing the render will make. A clip whose file is missing is dropped and named in the log (§0: failure
/// is specific and local) — nine clips of ten are worth more than a refusal, and the hole is visible.
///
/// Two spellings are accepted for "this camera's recording", because a session names its footage either by the
/// lane Prepare filed it under (`<base>/<something>.mkv`, which is what `prepare::lane` and
/// [`crate::timeline::kept_footage_recordings`] mean by a `cam<N>` base) or, as most projects' single-camera
/// sessions do, by the source's own stem: a source whose file is called `take.mp4` IS camera 0's recording.
/// Refusing to read the second spelling would leave a one-camera session with no picture at all.
///
/// Adjacent kept clips off one camera collapse to one entry: the player reads the file gaplessly and `-s`
/// already lands inside it, so listing the same file twice would replay its first seconds.
pub fn picture_files(tree: &layout::Tree, cut: &Cut) -> Vec<String> {
    let root = std::env::current_dir().unwrap_or_default();
    let sources = project::load(tree.dir())
        .map(|held| held.sources)
        .unwrap_or_default();
    let mut files: Vec<String> = Vec::new();
    for seg in cut.segs.iter().filter(|seg| !seg.is_insert() && seg.e > seg.s) {
        let Some(path) = footage_path(&root, tree, &sources, seg) else {
            continue;
        };
        let text = path.to_string_lossy().into_owned();
        if files.last().map(String::as_str) == Some(text.as_str()) {
            continue;
        }
        files.push(text);
    }
    files
}

/// The one file covering one kept clip, or `None` after saying why there is none.
///
/// Two spellings are accepted for "this camera's recording": the lane Prepare filed it under
/// (`cam0/whatever.mkv`, which is what `prepare::lane` and
/// [`crate::timeline::kept_footage_recordings`] mean by a `cam<N>` base), or — as most single-camera sessions
/// do — the source's own stem. Refusing the second would leave a one-camera session with no picture at all.
fn footage_path(
    root: &std::path::Path,
    tree: &layout::Tree,
    sources: &[project::Source],
    seg: &Seg,
) -> Option<std::path::PathBuf> {
    let base = format!("cam{}", seg.cam);
    let footage: Vec<&project::Source> = sources.iter().filter(|source| source.footage).collect();
    let named: Option<&project::Source> = footage
        .iter()
        .copied()
        .find(|source| {
            let name = file_stem(&source_name(&source.path));
            name == base || lane_of(&source.path).as_deref() == Some(base.as_str())
        })
        // No lane carries this camera's name: only a session that filmed ONE camera can answer, and then its
        // single footage row is this camera whatever it was called. With several rows and no `camN` to read,
        // guessing would play the wrong footage, so nothing is guessed.
        .or_else(|| (base == "cam0" && footage.len() == 1).then_some(footage[0]));
    let Some(written) = named.map(|source| source.path.clone()) else {
        crate::ui::window::log_line(&preview::playback_failure(
            "Narrate",
            &format!(
                "camera {} of the clip at {:.1} s is not in this session's sources",
                seg.cam, seg.s
            ),
        ));
        return None;
    };
    let path = layout::resolve(root, tree.dir(), &written);
    if !path.is_file() {
        crate::ui::window::log_line(&preview::playback_failure(
            "Narrate",
            &format!("{} is not on disk", path.display()),
        ));
        return None;
    }
    Some(path)
}

/// A stored path's last element, and that element without its extension — the lane name Prepare files
/// everything under, which is what a `Recording`'s `base` holds (`prepare::lane`'s rule).
fn source_name(path: &str) -> String {
    path.rsplit('/').next().unwrap_or(path).to_string()
}

/// The folder a stored path was filed under, if it was filed under one.
fn lane_of(path: &str) -> Option<String> {
    let (parent, _) = path.rsplit_once('/')?;
    parent.rsplit('/').next().map(str::to_string).filter(|held| !held.is_empty())
}

fn file_stem(name: &str) -> String {
    match name.rfind('.') {
        Some(at) if at > 0 => name[..at].to_string(),
        _ => name.to_string(),
    }
}

/// S3's one number for the bed: the game duck while a line speaks × the slider × the cut's own gain under
/// this second, and silence when this second's scene does not hear its recording. Every part is a rule that
/// already exists elsewhere, so a retune there moves the preview with it: [`narrate_preview::duck`] is the
/// whole-clip duck, [`cut_hear::gain_under`] is what the render reads, [`narrate_preview::lanes_heard`] is
/// the hush list. The voice keeps its own floor ([`crate::narrate_preview_leg::voice_gain`]) — ducking the
/// take by the game volume would make a loud game mute the narration instead of sitting under it.
pub fn bed_gain(cut: &Cut, at: f64, line_speaking: bool, game_volume: f64, volume: f64) -> f64 {
    // A second the cut removed has no hearing of its own (S1), so nothing here lowers it; the scene decides.
    let Some(scene) = scene_at(cut, at) else {
        return narrate_preview::duck(line_speaking, game_volume)
            * cut_hear::mix_gain(volume, cut_hear::gain_under(&cut.fx, at));
    };
    // Hushed by property, never by stopping the stream: `--volume=0.000` is the mute (§05-cut#8).
    if !cut_hear::hush(scene).is_empty() || !cut_hear::footage_sound_heard(scene) {
        return 0.0;
    }
    let duck = narrate_preview::duck(line_speaking, game_volume);
    let mix = cut_hear::mix_gain(volume, cut_hear::gain_under(&cut.fx, at));
    (duck * mix).clamp(0.0, cut_hear::MAX_GAIN)
}

/// The scene deciding this second, if the cut has one (a removed stretch has no hearing of its own).
fn scene_at(cut: &Cut, at: f64) -> Option<&Seg> {
    cut_hear::scene_at(&cut.segs, at).and_then(|scene| cut.segs.get(scene))
}

/// The argv the picture's player is asked for: `--volume` first (the one number [`bed_gain`] answered — a
/// hushed second arrives as `0.000`, which is muting by property), then the Cut page's flags so both players
/// are addressed the same way, then the files.
pub fn bed_argv(files: &[String], at: f64, gain: f64) -> Vec<String> {
    let mut args = vec![format!("--volume={:.3}", gain.clamp(0.0, 1.0))];
    args.extend(play_back::command_for(files, at));
    args
}

/// Log the argv the way [`crate::cut_play_back::spawn_at`] logs its spawn, so the log shows what the player was
/// really handed. Returns the string so a caller can keep it without logging.
pub fn said_argv(program: &str, args: &[String]) -> String {
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let line = crate::subprocess::log_line(program, &refs);
    crate::ui::window::log_line(&line);
    line
}

/// Where a preview's takes and cues live when no window published a session: the folder the process was started
/// in, resolved the one way every flow resolves it ([`crate::startup::session_dir`]). A page that HAS a session
/// hands its own tree to these doors instead; this exists so the leg can cue real footage whenever a project
/// folder is open at all, rather than only after some flow remembered to publish one.
pub fn session_tree() -> Option<layout::Tree> {
    let dir = startup::session_dir(&std::env::current_dir().ok()?);
    layout::Tree::new(&dir).ok()
}

/// S6/S2: freeze both players with `SIGSTOP`, keeping the processes and their positions.
///
/// This is what "pause" means for a subprocess player: the pipeline stays built, the frame stays decoded, and
/// [`resume_pids`] walks it on from the same second. Killing them (what this leg used to do) throws the position
/// away, so the next ▶ had to re-cue — and a re-cue is a black frame where a pause should be a still. Same
/// signal pair [`crate::cut_play_back::hold_still`] uses for the Cut page's player, for the same reason.
pub fn pause_pids(pids: &[u32]) {
    for pid in pids {
        signal(*pid, libc::SIGSTOP);
    }
}

/// Release a frozen player. Only ever sent to pids this leg spawned.
pub fn resume_pids(pids: &[u32]) {
    for pid in pids {
        signal(*pid, libc::SIGCONT);
    }
}

/// Send `signal` to one pid of this leg's own children.
fn signal(pid: u32, signal: i32) {
    // SAFETY: `kill` on an integer pid cannot corrupt memory; every pid here came from `spawn_piped`, so the
    // only processes ever signalled are ones this app started in this user's session.
    unsafe { libc::kill(pid as i32, signal) };
}

/// Freeze or release ONE player by pid — the door a seek uses to keep a freshly-cued picture a still (the
/// preroll trick [`crate::cut_play_back::preroll`] uses for the Cut page's spare).
pub fn hold_pid(pid: u32, hold: bool) {
    signal(pid, if hold { libc::SIGSTOP } else { libc::SIGCONT });
}

/// Freeze or release a whole set of players with one call.
pub fn signal_pids(pids: &[u32], hold: bool) {
    if hold {
        pause_pids(pids);
    } else {
        resume_pids(pids);
    }
}

/// Cue the picture over the cut's own footage at `at`, through the same argv as any other cue, and hand back
/// the child. Public so the page can move the picture by a second (a transport button, a wheel notch)
/// without reaching into the leg's preview slot.
pub fn cue_picture(
    program: &str,
    tree: &layout::Tree,
    cut: &Cut,
    at: f64,
    gain: f64,
    hold: bool,
    spawn: &mut crate::narrate_preview_leg::Spawn,
) -> Result<std::process::Child, String> {
    let files = picture_files(tree, cut);
    if files.is_empty() {
        return Err(narrate_preview::NO_RECORDING_AT_START.to_string());
    }
    let args = bed_argv(&files, at, gain);
    said_argv(program, &args);
    let child = spawn(program, &args)?;
    // A cue made while the preview is paused is a STILL: freeze the process at once so the new frame shows
    // rather than a clip starting under the person's thumb.
    if hold {
        hold_pid(child.id(), true);
    }
    Ok(child)
}
