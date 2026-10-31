//! F4.5 S1/S3 — what the picture's player is actually asked to play, and at what level.

use crate::cut::{Cut, Seg};
use crate::{cut_hear, layout, narrate_preview, play_back, preview, project};

/// The footage list the picture plays: one entry per kept clip, in cut order, each resolved to a
/// file that exists.
pub fn picture_files(tree: &layout::Tree, cut: &Cut) -> Vec<String> {
    let root = std::env::current_dir().unwrap_or_default();
    let sources = project::load(tree.dir())
        .map(|held| held.sources)
        .unwrap_or_default();
    let mut files: Vec<String> = Vec::new();
    for seg in cut
        .segs
        .iter()
        .filter(|seg| !seg.is_insert() && seg.e > seg.s)
    {
        let Some(path) = footage_path(&root, tree, &sources, seg, cut) else {
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

/// The files the player is asked for when the cue lands at `at`: the recording covering `at` FIRST,
/// then the rest of the cut's footage in cut order.
pub fn cue_files(tree: &layout::Tree, cut: &Cut, at: f64) -> Vec<String> {
    let all = picture_files(tree, cut);
    let Some(first) = covering_source(tree, cut, at) else {
        return all;
    };
    rotate_to(&all, &first)
}

fn covering_source(tree: &layout::Tree, cut: &Cut, at: f64) -> Option<String> {
    let root = std::env::current_dir().unwrap_or_default();
    let sources = project::load(tree.dir())
        .map(|held| held.sources)
        .unwrap_or_default();
    let covering = cut_hear::scene_at(&cut.segs, at)
        .and_then(|scene| cut.segs.get(scene))
        .filter(|seg| !seg.is_insert() && seg.e > seg.s)?;
    footage_path(&root, tree, &sources, covering, cut)
        .map(|path| path.to_string_lossy().into_owned())
}

/// Move `wanted` to the front, keeping the rest in order; unchanged when it is already first or
/// absent.
fn rotate_to(files: &[String], wanted: &str) -> Vec<String> {
    let Some(at) = files.iter().position(|file| file == wanted) else {
        return files.to_vec();
    };
    let mut rotated = files[at..].to_vec();
    rotated.extend_from_slice(&files[..at]);
    rotated
}

pub fn footage_second(cut: &Cut, at: f64) -> f64 {
    let Some(scene) = scene_at(cut, at) else {
        return at.max(0.0);
    };
    if let Some(recording) = recording_under(cut, scene, at) {
        let start =
            (scene.s - recording.start + crate::timeline::shift_of(cut, &recording.base)).max(0.0);
        return start + (at - scene.s).max(0.0) * narrate_preview::seek_rate(&cut.fx, at);
    }
    let travelled = (at - scene.s).max(0.0) * narrate_preview::seek_rate(&cut.fx, at);
    (travelled + scene.ss.max(0.0)).max(0.0)
}

/// The one file covering one kept clip, or `None` after saying why there is none.
fn footage_path(
    root: &std::path::Path,
    tree: &layout::Tree,
    sources: &[project::Source],
    seg: &Seg,
    cut: &Cut,
) -> Option<std::path::PathBuf> {
    if let Some(recording) = recording_under(cut, seg, seg.s.max(0.0)) {
        if let Some(source) = sources
            .iter()
            .find(|source| file_stem(&source_name(&source.path)) == recording.base)
        {
            let path = layout::resolve(root, tree.dir(), &source.path);
            if path.is_file() {
                return Some(path);
            }
        }
    }
    let base = format!("cam{}", seg.cam);
    let footage: Vec<&project::Source> = sources.iter().filter(|source| source.footage).collect();
    let named: Option<&project::Source> = footage
        .iter()
        .copied()
        .find(|source| {
            let name = file_stem(&source_name(&source.path));
            name == base || lane_of(&source.path).as_deref() == Some(base.as_str())
        })
        .or_else(|| {
            footage
                .first()
                .copied()
                .filter(|_| base == "cam0" && footage.len() == 1)
        });
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

/// A stored path's last element, and that element without its extension.
fn source_name(path: &str) -> String {
    path.rsplit('/').next().unwrap_or(path).to_string()
}

fn lane_of(path: &str) -> Option<String> {
    let (parent, _) = path.rsplit_once('/')?;
    parent
        .rsplit('/')
        .next()
        .map(str::to_string)
        .filter(|held| !held.is_empty())
}

fn file_stem(name: &str) -> String {
    match name.rfind('.') {
        Some(at) if at > 0 => name[..at].to_string(),
        _ => name.to_string(),
    }
}

pub fn bed_gain(cut: &Cut, at: f64, line_speaking: bool, game_volume: f64, volume: f64) -> f64 {
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

pub fn shown_at(cut: &Cut, at: f64) -> Option<(String, f64)> {
    let scene = scene_at(cut, at).filter(|scene| !scene.is_insert())?;
    let recording = recording_under(cut, scene, at)?;
    Some((recording.base.clone(), footage_second(cut, at)))
}

fn recording_under(cut: &Cut, scene: &Seg, at: f64) -> Option<crate::timeline::Recording> {
    let recordings = crate::ui::page_recordings();
    let rows = crate::timeline::rows_for(&recordings, cut);
    let row = scene.cam.max(0) as usize;
    recordings
        .into_iter()
        .zip(rows)
        .filter(|(_, placed)| *placed == row)
        .map(|(recording, _)| recording)
        .find(|recording| recording.start <= at && at < recording.end)
}

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

pub fn said_argv(program: &str, args: &[String]) -> String {
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let line = crate::subprocess::log_line(program, &refs);
    crate::ui::window::log_line(&line);
    line
}

pub fn session_tree() -> Option<layout::Tree> {
    let dir = crate::startup::open_dir();
    layout::Tree::new(&dir).ok()
}

pub fn pause_pids(pids: &[u32]) {
    for pid in pids {
        signal(*pid, libc::SIGSTOP);
    }
}

pub fn resume_pids(pids: &[u32]) {
    for pid in pids {
        signal(*pid, libc::SIGCONT);
    }
}

fn signal(pid: u32, signal: i32) {
    // SAFETY: `kill` on an integer pid cannot corrupt memory; every pid here came from `spawn_piped`, so the
    // only processes ever signalled are ones this app started in this user's session.
    unsafe { libc::kill(pid as i32, signal) };
}

pub fn hold_pid(pid: u32, hold: bool) {
    signal(pid, if hold { libc::SIGSTOP } else { libc::SIGCONT });
}

pub fn signal_pids(pids: &[u32], hold: bool) {
    if hold {
        pause_pids(pids);
    } else {
        resume_pids(pids);
    }
}

/// The cue the program last asked the player for: its argv and the pid it produced, kept so a
/// tick's answer can be read back as the number it changed (`--volume=` after a duck, `-s` after a
/// seek) instead of only being logged.
#[derive(Debug, Clone, Default)]
pub struct Cue {
    pub args: Vec<String>,
    pub pid: u32,
}

/// A cue's two freezes, spelled instead of passed as a bare `bool` (`f(true, false)` at a call site
/// says nothing): the preview's own pause, and §6's no-footage still frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Freeze {
    Play,
    /// Freeze the player at once because the PREVIEW is paused — a seek under the thumb shows a new
    /// frame, not a clip starting.
    Paused,
    /// Freeze the player at once because there is NOTHING to play: §6's "a line auditioned from its
    /// own ▶ over a clip with no footage … speaks against a still frame".
    StillFrame,
}

impl Freeze {
    /// Whether the player must be stopped the instant it is spawned.
    fn hold(self) -> bool {
        self != Freeze::Play
    }

    fn allows_no_footage(self) -> bool {
        self == Freeze::StillFrame
    }
}

pub fn cue_picture(
    program: &str,
    tree: &layout::Tree,
    cut: &Cut,
    at: f64,
    gain: f64,
    freeze: Freeze,
    spawn: &mut crate::narrate_preview_leg::Spawn,
) -> Result<(std::process::Child, Cue), String> {
    let files = cue_files(tree, cut, at);
    if files.is_empty() && !freeze.allows_no_footage() {
        return Err(narrate_preview::NO_RECORDING_AT_START.to_string());
    }
    let args = bed_argv(&files, footage_second(cut, at), gain);
    said_argv(program, &args);
    let child = spawn(program, &args)?;
    let cue = Cue {
        args,
        pid: child.id(),
    };
    // A cue made while the preview is paused is a STILL: freeze the process at once so the new
    // frame shows rather than a clip starting under the person's thumb. §6's no-footage audition
    // asks for the same freeze unconditionally, because its whole point is a frame that does not
    // move while the line speaks.
    if freeze.hold() {
        hold_pid(cue.pid, true);
    }
    Ok((child, cue))
}
