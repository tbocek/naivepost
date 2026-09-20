//! The sources list (spec/03-shell.md §4) — one row per file, and what each file is for.
//!
//! The list lives on Prepare but is specified with the shell because the shell is what snapshots it:
//! ▶ freezes these rows as the session a run works through (F0.2 S3), so every rule here has to hold
//! between two presses, not merely at load. This module is that state and those rules; the rows in
//! `ui` draw them and forward clicks (spec/00-principles.md §5).
//!
//! This item cites no `P.*` parameter and no `tool:*`.

use std::path::Path;

use crate::add_sources::is_video;
use crate::clock;
use crate::project::{Project, Source};
use crate::rescan;
use crate::run::NARRATOR_SLOTS;

/// The key to a row's four symbols, appended to each of their tooltips so the three the pointer is
/// *not* on are explained where the eye already is. The track button and ⚠ do not carry it: they are
/// self-evident once the four are, and §4 says so outright.
pub const ROW_KEY: &str = "\n\nThe symbols on every row, in order:\n\
    🎥 footage — frames come out of it, and the cut is made of it\n\
    🎤 narrator — which of the voices this is; 1 speaks the narration\n\
    ✂ split the voice off — ▶ separates it into the voice and the rest\n\
    🗑 remove — off this list; the file itself is left alone";

/// §4's strings, in the words the spec gives them.
pub const ADD_TIP: &str = "Add recordings or footage \u{2014} several at once";
pub const COPY_TIP: &str = "Ticked, an added file is copied into the project's sources/ folder, so the \
    project holds everything it needs. Unticked, the file is referenced where it is: nothing is \
    duplicated, and the session breaks if it moves.";
pub const LIST_TIP: &str = "Every file here is transcribed, and placed on the session clock by the \
    timestamp in its name";
pub const FOOTAGE_TIP: &str = "Footage \u{2014} frames come out of this file and it can be cut.\n\
    Off: it is only listened to, which is what a video kept for its audio wants.";
pub const NARRATOR_TIP: &str = "Which voice this is. 0 clicks in: nobody yet.\n\
    1 is the voice the narration is spoken in; 2\u{2013}4 are the rest of the group.";
pub const SEPARATE_TIP: &str = "Split the voice off \u{2014} on \u{25b6} this recording is separated into the\n\
    voice and everything else. This row keeps everything else; the voice\n\
    is added as a track of its own, so it can be cut and mixed apart.";
pub const REMOVE_TIP: &str = "Remove from this session \u{2014} the file itself is left alone";
/// The track popover's head. Its three-line explanation belongs to §5's lane rules; the face of the
/// button is `<on>/<total>`, which says what is ticked without opening anything.
pub const TRACKS_TITLE: &str = "Audio tracks in this file";
pub const WARN_TIP: &str = "No timestamp in the file name \u{2014} this file starts where the\n\
    session does, which is only right if it was rolling from the start.\n\
    Rename it with when the recording STARTED, like\n\
    clip_2026-08-08_19-55-15.mkv \u{2014} most recorders already do \u{2014} or drag it\n\
    into place with the right mouse button on the Cut page.";

/// One audio stream of a file, as the probe found it. Probing is F1.x's; this module only draws what
/// it is told about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackInfo {
    pub index: u32,
    /// The stream's own title, or a placeholder when the container has none.
    pub title: String,
    pub channels: u8,
}

impl TrackInfo {
    /// The popover's row for this track: numbered, titled, and saying what it holds.
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

/// The row's name as §4 shows it: the file's own name, the last part of the stored path whichever of
/// the one path rule's three spellings it uses (01 §2). Nothing is stripped off it — neither the
/// extension nor a recording's timestamp — because the stamp is what §4 places the file by (F1.1) and
/// two files differing only by extension are two files (see `clash`). The middle-ellipsis and the
/// path-carrying tooltip belong to the row's label in `ui`.
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

/// The controls this row shows, in order. `stream_count` and `tracks_out` come from the caller's
/// probe (F1.x owns probing), which is why they are arguments rather than something read here.
///
/// Two of §4's numbered badges are deliberately absent: **9** frame size and **11** Style, both "gone
/// in the rewrite" — frames are stored at the video's own size and the policy derives style from the
/// User Context. **8** Freq and **10** Language are this page's other section (the frame controls),
/// not rows of this list.
pub fn row_controls(source: &Source, stream_count: usize) -> Vec<Control> {
    let mut controls = vec![];
    // Only a video may be footage, so on an audio file the control is not off — it does not exist.
    if is_video(Path::new(base_name(&source.path))) {
        controls.push(Control::Footage);
    }
    controls.push(Control::Narrator);
    controls.push(Control::Name);
    // One stream needs no menu: there is nothing to choose.
    if stream_count >= 2 {
        controls.push(Control::Tracks);
    }
    // A name with no stamp starts where the session does, which is a guess worth warning about.
    if clock::name_stamp(base_name(&source.path)).is_none() {
        controls.push(Control::Warning);
    }
    // Greyed on a split product (see `separable`), never dropped: a row without it would pull the
    // trash to a different x than its neighbours.
    controls.push(Control::Separate);
    controls.push(Control::Remove);
    controls
}

/// The last part of a stored path, whichever of the one path rule's three spellings it uses.
fn base_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// Whether this row's voice can still be split off: a half of a split has no voice left to take off a
/// voice, so its scissors are dead rather than gone. The suffix is the stem's, before the extension —
/// `mic.split-voice.wav` is a product, `mic.wav` is not (the prototype's `splitProduct`, which lists
/// the same four stems).
pub fn separable(source: &Source) -> bool {
    const SUFFIXES: [&str; 4] = [".split-voice", ".split-novoice", ".voice", ".novoice"];
    let name = base_name(&source.path);
    // Compare against the stem, so the container's own extension never looks like a split suffix.
    let stem = name.rsplit_once('.').map(|(stem, _)| stem).unwrap_or(name);
    !SUFFIXES.iter().any(|suffix| stem.ends_with(suffix))
}

/// The footage toggle. Refuses on a row that is not a video: only a video may be footage, and a
/// toggle that silently never sticks reads as a bug in the list rather than a rule about the file.
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

/// The narrator button's cycle: the lowest free slot, and once it holds one (or all four are taken)
/// back to nobody — "cycling free slots 1..N then none".
///
/// Nobody else is moved to make room: a row that already holds a slot is somebody the user named. But
/// a cycle can vacate slot 1, and slot 1 is auto-filled whenever unheld, so it is asked for here.
pub fn cycle_narrator(project: &mut Project, index: usize) -> u32 {
    let held = project.sources.get(index).map(|source| source.narrator).unwrap_or(0);
    let next = if held != 0 {
        0
    } else {
        (1..=NARRATOR_SLOTS as u32)
            .find(|slot| slot_holder(project, *slot).is_none())
            .unwrap_or(0)
    };
    if let Some(source) = project.sources.get_mut(index) {
        source.narrator = next;
    }
    if slot_holder(project, 1).is_none() {
        rescan::fill_slot_1(project);
    }
    next
}

/// Which row holds this narrator slot.
pub fn slot_holder(project: &Project, slot: u32) -> Option<usize> {
    project.sources.iter().position(|source| source.narrator == slot && slot != 0)
}

/// The tooltip §4 puts on the narrator button: what the row is now, and that a click moves it on. Slot
/// 1 gets the extra sentence because it is the one that matters (F2.14 reads it as the narration's
/// voice).
pub fn slot_tip(slot: u32) -> String {
    match slot {
        0 => "no narrator \u{2014} click to make this voice 1".into(),
        1 => "the narration is spoken by this voice (1) \u{2014} click for the next slot".into(),
        slot => format!("narrator slot {slot} \u{2014} click back to no narrator"),
    }
}

/// The row's warning badge, or nothing when there is nothing to warn about: §4 puts ⚠ on a file whose
/// name carries no timestamp, because such a file starts where the session does — which is only right
/// if it was rolling from the start.
pub fn warning(source: &Source) -> Option<&'static str> {
    clock::name_stamp(base_name(&source.path)).is_none().then_some(WARN_TIP)
}

/// 🗑 Remove from this session. The list *is* the session (spec/01 §1), so dropping a row is how a
/// file leaves a project; the file itself is left alone, which is why nothing here touches a path.
/// Slot 1 is re-filled when its holder was this row — "after an add, a removal, a rescan that dropped
/// a row" (§4).

/// The stored path of whoever speaks this slot — §5's lane naming and Produce's readouts both want it.
pub fn narrator_of(project: &Project, slot: u32) -> Option<&str> {
    slot_holder(project, slot).map(|index| project.sources[index].path.as_str())
}

/// 🗑 Remove from this session. The list *is* the session (spec/01 §1), so dropping a row is how a
/// file leaves a project; the file itself is left alone, which is why nothing here touches a path.
/// Slot 1 is re-filled when its holder was this row — "after an add, a removal, a rescan that dropped
/// a row" (§4).
pub fn remove(project: &mut Project, index: usize) -> Option<Source> {
    if index >= project.sources.len() {
        return None;
    }
    let gone = project.sources.remove(index);
    if slot_holder(project, 1).is_none() {
        rescan::fill_slot_1(project);
    }
    Some(gone)
}

/// The status line after a row was removed: which file left the list, and how many are left in it.
pub fn removal_status(path: &str, left: usize) -> String {
    format!("removed {} -- {left} source(s) left", base_name(path))
}

/// The first two rows sharing one base name. Two sources of one name would write into one folder
/// under `inputs/`, and the second run would quietly find the first's transcript, so ▶ refuses (§4's
/// "two sources with one base name refuse the run").
///
/// Compared on the whole base name including the extension, which is what makes `project:sources/
/// clip.mkv` clash with `/media/clip.mkv`: they would both land in `inputs/clip`.
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

/// The log's line for a clash: it names both rows and the folder they would share. `base` is the name
/// without its extension — the folder either file would be written into.
pub fn clash_log(a: &str, b: &str) -> String {
    let base = base_name(a).rsplit_once('.').map_or(base_name(a), |(stem, _)| stem);
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

/// Clear what a hand-edited project cannot mean: two rows in one slot, a slot outside the range the
/// narrator button can reach, or frames promised out of a file that has none. Each clearing is reported
/// — silently losing a tag the user typed is worse than a line about it — and only then may slot 1 be
/// auto-filled, because stripping a bad tag is one of the four moments §4 names (§4: "a load that
/// stripped a bad tag"). A list that loads clean keeps whatever the user left in slot 1.
pub fn clean_on_load(project: &mut Project, report: &mut Vec<String>) {
    let mut stripped = false;
    for index in 0..project.sources.len() {
        // Read the slot before writing it: whether this row is the holder is a question about the
        // whole list, and answering it while holding a `&mut` to one row is how this turns into a
        // borrow checker argument.
        let slot = project.sources[index].narrator;
        if slot != 0 && (slot > NARRATOR_SLOTS as u32 || slot_holder(project, slot) != Some(index)) {
            report.push(format!(
                "{}: slot {slot} is taken -- untagged",
                base_name(&project.sources[index].path)
            ));
            project.sources[index].narrator = 0;
            stripped = true;
        }
    }
    for source in &mut project.sources {
        if source.footage && !is_video(Path::new(base_name(&source.path))) {
            report.push(format!(
                "{}: an audio file has no frames -- not footage",
                base_name(&source.path)
            ));
            source.footage = false;
            stripped = true;
        }
    }
    if stripped && slot_holder(project, 1).is_none() {
        rescan::fill_slot_1(project);
    }
}

/// The track popover's checks. The last ticked track cannot be unticked: a row with no track would
/// hear nothing, and an empty `tracks` is what "not chosen yet" means elsewhere.
pub fn tick_track(project: &mut Project, index: usize, track: u32, on: bool) -> bool {
    let Some(source) = project.sources.get_mut(index) else {
        return false;
    };
    if !on && source.tracks.len() == 1 && source.tracks.contains(&track) {
        return false;
    }
    source.tracks.retain(|held| *held != track);
    if on {
        source.tracks.push(track);
    }
    // Sorted and deduped, so the lane order follows the file's own stream order.
    source.tracks.sort_unstable();
    source.tracks.dedup();
    true
}

/// Whether the row's track button is dimmed: §4 dims it "while any track is out", and which streams
/// are still being read is probe state (§5), so the caller passes it in rather than this module
/// guessing at it.
pub fn tracks_dimmed(any_track_out: bool) -> bool {
    any_track_out
}

/// The face of the track button: what is ticked over what the file holds, or nothing at all for a file
/// with a single stream — that row has no menu to open.
pub fn tracks_face(source: &Source, stream_count: usize) -> Option<String> {
    if stream_count < 2 {
        return None;
    }
    Some(format!("{}/{}", source.tracks.len(), stream_count))
}

/// Each ticked track becomes a lane, mixed like a separate recording (§4). Cut's §5 is the authority on
/// what a lane is called; this is the list's own view of which rows will be split and after what — a
/// file whose ticks are down to one stream is used as it stands, so it contributes no lanes. Spelled
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
            source.tracks.iter().map(move |track| format!("{stem}-t{track}"))
        })
        .collect()
}
