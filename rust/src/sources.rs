//! The sources list (spec/03-shell.md §4) — one row per file, and what each file is for.

use std::path::Path;

use crate::add_sources::is_video;
use crate::clock;
use crate::project::{Project, Source};
use crate::run::NARRATOR_SLOTS;

pub const ROW_KEY: &str = "\n\nThe symbols on every row, in order:\n\
    🎥 footage — frames come out of it, and the cut is made of it\n\
    🎤 narrator — which of the voices this is; 1 speaks the narration\n\
    ✂ split the voice off — ▶ separates it into the voice and the rest\n\
    🗑 remove — off this list; the file itself is left alone";

/// §4's strings, in the words the spec gives them.
pub const ADD_TIP: &str = "Add recordings or footage \u{2014} several at once";
pub const COPY_TIP: &str =
    "Ticked, an added file is copied into the project's sources/ folder, so the \
    project holds everything it needs. Unticked, the file is referenced where it is: nothing is \
    duplicated, and the session breaks if it moves.";
pub const LIST_TIP: &str =
    "Every file here is transcribed, and placed on the session clock by the \
    timestamp in its name";
pub const FOOTAGE_TIP: &str = "Footage \u{2014} frames come out of this file and it can be cut.\n\
    Off: it is only listened to, which is what a video kept for its audio wants.";
pub const NARRATOR_TIP: &str = "Which voice this is. 0 clicks in: nobody yet.\n\
    1 is the voice the narration is spoken in; 2\u{2013}4 are the rest of the group.";
pub const SEPARATE_TIP: &str =
    "Split the voice off \u{2014} on \u{25b6} this recording is separated into the\n\
    voice and everything else. This row keeps everything else; the voice\n\
    is added as a track of its own, so it can be cut and mixed apart.";
pub const REMOVE_TIP: &str = "Remove from this session \u{2014} the file itself is left alone";
pub const TRACKS_TITLE: &str = "Audio tracks in this file";
pub const WARN_TIP: &str = "No timestamp in the file name \u{2014} this file starts where the\n\
    session does, which is only right if it was rolling from the start.\n\
    Rename it with when the recording STARTED, like\n\
    clip_2026-08-08_19-55-15.mkv \u{2014} most recorders already do \u{2014} or drag it\n\
    into place with the right mouse button on the Cut page.";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackInfo {
    pub index: u32,
    pub title: String,
    pub channels: u8,
}

impl TrackInfo {
    pub fn label(&self) -> String {
        let shape = if self.channels >= 2 { "stereo" } else { "mono" };
        format!("Track {} \u{2014} {} ({shape})", self.index, self.title)
    }
}

/// One control on a row, in the order §4 lists them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Control {
    Footage,
    Narrator,
    Name,
    Tracks,
    Warning,
    Separate,
    Remove,
}

/// The row's name as §4 shows it: the file's own name, the last part of the stored path whichever
/// of the one path rule's three spellings it uses (01 §2).
pub fn row_name(path: &str) -> String {
    base_name(path).to_owned()
}

/// The row's controls as §4 lists them, with the two probe-dependent ones resolved: `stream_count` is
/// how many audio streams the file turned out to have and `any_track_out` whether any ticked one is
/// still being read (F1.x owns probing). A track button on a row whose tracks are all out would open a
/// menu over audio that is not there yet, so it is dropped rather than dimmed to a dead click.
pub fn controls(source: &Source, stream_count: usize, any_track_out: bool) -> Vec<Control> {
    let mut controls = row_controls(source, stream_count);
    if controls.contains(&Control::Tracks) && tracks_dimmed(any_track_out) {
        controls.retain(|control| *control != Control::Tracks);
    }
    controls
}

/// The controls this row shows, in order.
pub fn row_controls(source: &Source, stream_count: usize) -> Vec<Control> {
    let mut controls = vec![];
    if is_video(Path::new(base_name(&source.path))) {
        controls.push(Control::Footage);
    }
    controls.push(Control::Narrator);
    controls.push(Control::Name);
    if stream_count >= 2 {
        controls.push(Control::Tracks);
    }
    if clock::name_stamp(base_name(&source.path)).is_none() {
        controls.push(Control::Warning);
    }
    // Greyed on a split product (see `separable`), never dropped: a row without it would pull the
    // trash to a different x than its neighbours.
    controls.push(Control::Separate);
    controls.push(Control::Remove);
    controls
}

fn base_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// §4's three-line explanation over the track checks: what a tick means, said once instead of on
/// every check.
pub const TRACKS_HELP: &str =
    "Each ticked track becomes a lane of its own, mixed like a separate\n\
    recording. Untick one and its audio leaves the session; the last one\n\
    cannot come off, because a row that hears nothing has no transcript to fix.";

pub fn last_track_says(track: u32) -> String {
    format!("track {track} is the only one left -- a row must hear something")
}

/// Whether this row's voice can still be split off: a half of a split has no voice left to take off
/// a voice, so its scissors are dead rather than gone.
pub fn separable(source: &Source) -> bool {
    const SUFFIXES: [&str; 4] = [".split-voice", ".split-novoice", ".voice", ".novoice"];
    let name = base_name(&source.path);
    // Compare against the stem, so the container's own extension never looks like a split suffix.
    let stem = name.rsplit_once('.').map(|(stem, _)| stem).unwrap_or(name);
    !SUFFIXES.iter().any(|suffix| stem.ends_with(suffix))
}

pub fn set_sepvoice(project: &mut Project, index: usize, on: bool) -> bool {
    let Some(source) = project.sources.get_mut(index) else {
        return false;
    };
    if !separable(source) {
        return false;
    }
    source.sepvoice = on;
    true
}

pub fn toggle_footage(project: &mut Project, index: usize, on: bool) -> bool {
    let Some(source) = project.sources.get_mut(index) else {
        return false;
    };
    if !is_video(Path::new(base_name(&source.path))) {
        return false;
    }
    source.footage = on;
    true
}

pub fn cycle_narrator(project: &mut Project, index: usize) -> u32 {
    let held = project
        .sources
        .get(index)
        .map(|source| source.narrator)
        .unwrap_or(0);
    let next = if held >= NARRATOR_SLOTS as u32 {
        1
    } else {
        held + 1
    };
    if let Some(source) = project.sources.get_mut(index) {
        source.narrator = next;
    }
    next
}

pub fn slot_holder(project: &Project, slot: u32) -> Option<usize> {
    project
        .sources
        .iter()
        .position(|source| source.narrator == slot && slot != 0)
}

pub fn is_narrator_mic(source: &Source) -> bool {
    source.narrator != 0 && !source.footage
}

/// The tooltip §4 puts on the narrator button: what the row is now, and that a click moves it on.
/// Slot 1 gets the extra sentence because it is the one that matters (F2.14 reads it as the
/// narration's voice).
pub fn slot_tip(slot: u32) -> String {
    match slot {
        0 | 1 => "voice 1: the narration is spoken in this voice \u{2014} click for voice 2".into(),
        slot if slot >= NARRATOR_SLOTS as u32 => {
            format!("voice {slot} \u{2014} click for voice 1")
        }
        slot => format!("voice {slot} \u{2014} click for voice {}", slot + 1),
    }
}

/// The row's warning badge, or nothing when there is nothing to warn about: §4 puts ⚠ on a file
/// whose name carries no timestamp, because such a file starts where the session does.
pub fn warning(source: &Source) -> Option<&'static str> {
    clock::name_stamp(base_name(&source.path))
        .is_none()
        .then_some(WARN_TIP)
}

/// The recording a voice is taken from: its dedicated microphone when it has one, else the first
/// recording with that number (§5's lane naming, Narrate's voice and Produce's readouts all want
/// one file).
pub fn narrator_of(project: &Project, slot: u32) -> Option<&str> {
    if slot == 0 {
        return None;
    }
    project
        .sources
        .iter()
        .find(|source| source.narrator == slot && !source.footage)
        .or_else(|| {
            project
                .sources
                .iter()
                .find(|source| source.narrator == slot)
        })
        .map(|source| source.path.as_str())
}

pub fn remove(project: &mut Project, index: usize) -> Option<Source> {
    if index >= project.sources.len() {
        return None;
    }
    Some(project.sources.remove(index))
}

/// The status line after a row was removed: which file left the list, and how many are left in it.
pub fn removal_status(path: &str, left: usize) -> String {
    format!("removed {} -- {left} source(s) left", base_name(path))
}

pub fn clash(project: &Project) -> Option<(String, String)> {
    let mut seen: Vec<(&str, &str)> = vec![];
    for source in &project.sources {
        let base = base_name(&source.path);
        if let Some((_, first)) = seen.iter().find(|(earlier, _)| *earlier == base) {
            return Some(((*first).to_string(), source.path.clone()));
        }
        seen.push((base, &source.path));
    }
    None
}

pub fn clash_log(a: &str, b: &str) -> String {
    let base = base_name(a)
        .rsplit_once('.')
        .map_or(base_name(a), |(stem, _)| stem);
    format!("!!! {a} and {b} are both inputs/{base} -- rename one")
}

/// The status line for a clash, in the words §4 gives it.
pub fn clash_status(a: &str, b: &str) -> String {
    format!(
        "{} and {} have the same name \u{2014} rename one",
        base_name(a),
        base_name(b)
    )
}

pub fn clean_on_load(project: &mut Project, report: &mut Vec<String>) {
    for source in &mut project.sources {
        if source.narrator > NARRATOR_SLOTS as u32 {
            report.push(format!(
                "{}: voice {} is past the last one -- voice 1",
                base_name(&source.path),
                source.narrator
            ));
            source.narrator = 1;
        } else if source.narrator == 0 {
            source.narrator = 1;
        }
    }
    for source in &mut project.sources {
        if source.footage && !is_video(Path::new(base_name(&source.path))) {
            report.push(format!(
                "{}: an audio file has no frames -- not footage",
                base_name(&source.path)
            ));
            source.footage = false;
        }
    }
}

pub fn tick_track(project: &mut Project, index: usize, track: u32, on: bool) -> bool {
    let Some(source) = project.sources.get_mut(index) else {
        return false;
    };
    // An empty list is the first stream alone (01 §2), so a tick on such a row starts from that.
    let mut ticked = ticked_tracks(source);
    if !on && ticked.len() == 1 && ticked.contains(&track) {
        return false;
    }
    ticked.retain(|held| *held != track);
    if on {
        ticked.push(track);
    }
    // Sorted and deduped, so the lane order follows the file's own stream order.
    ticked.sort_unstable();
    ticked.dedup();
    source.tracks = ticked;
    true
}

/// The audio streams a row hears, as 0-based stream indices (01 §2's `tracks`): what it lists, or
/// the first stream alone when it lists none, which is how a project written before the menu
/// existed reads.
pub fn ticked_tracks(source: &Source) -> Vec<u32> {
    if source.tracks.is_empty() {
        vec![0]
    } else {
        source.tracks.clone()
    }
}

/// Whether the row's track button is dimmed: §4 dims it "while any track is out", and which streams
/// are still being read is probe state (§5), so the caller passes it in rather than this module
/// guessing at it.
pub fn tracks_dimmed(any_track_out: bool) -> bool {
    any_track_out
}

pub fn tracks_face(source: &Source, stream_count: usize) -> Option<String> {
    if stream_count < 2 {
        return None;
    }
    Some(format!("{}/{}", ticked_tracks(source).len(), stream_count))
}

/// Each ticked track becomes a lane, mixed like a separate recording (§4). Spelled
/// `{stem}-t{track}` so two tracks of one file never share a name.
pub fn split_lanes(project: &Project) -> Vec<String> {
    project
        .sources
        .iter()
        .filter(|source| source.tracks.len() > 1)
        .flat_map(|source| {
            let stem = base_name(&source.path).rsplit_once('.').map_or_else(
                || base_name(&source.path).to_string(),
                |(stem, _)| stem.to_string(),
            );
            source
                .tracks
                .iter()
                .map(move |track| format!("{stem}-t{track}"))
        })
        .collect()
}
