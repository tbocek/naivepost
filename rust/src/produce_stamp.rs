//! F5.3 What "up to date" means — `spec/08-produce.md` §F5.3. One hash of everything the render reads,
//! kept in `produce/final.stamp` beside the video, so a ▶ over an unchanged project costs nothing: the
//! encode is skipped and the file is left exactly as it was.

use std::fs;
use std::path::{Path, PathBuf};

use crate::cut::Seg;
use crate::layout::Tree;
use crate::narrate_tts;
use crate::narration;
use crate::narration::Entry;
use crate::project::{self, Produce};

/// The stamp's own name, spelled out once so a test can pin that it sits beside the video rather than in
/// the scratch folder with the clips (which are cleared at the start of every run).
pub const STAMP_FILE: &str = "final.stamp";

pub use crate::produce_render::stamp_written;

// ---- the input -------------------------------------------------------------------

pub fn wav_of(tree: &Tree, line: &Entry, voice: Option<&str>, roll: i32) -> Option<PathBuf> {
    if line.text.trim().is_empty() {
        return None;
    }
    let mut salted = line.clone();
    salted.roll = roll;
    Some(narrate_tts::take_path(
        tree,
        &narration::key_for(&salted, voice),
    ))
}

#[derive(Debug)]
pub struct Input<'a> {
    /// §A's encoder settings. Hashed in full; the output path is not a setting (§ module doc).
    pub settings: &'a Produce,
    /// The segments the render would place, after the speed splits (the same list F5.2 S3 is handed).
    pub segs: &'a [Seg],
    /// The narration lines, in the record's order — order is meaning here, so a re-order is a change.
    pub lines: &'a [Entry],
    /// The project's sources; only the footage ones carry a file whose size and mtime are weighed.
    pub sources: &'a [project::Source],
    /// The cut's aspect, which decides the frame box of every clip.
    pub aspect: &'a str,
    /// The selected voice: switching voices keeps every line and every setting and changes only the sound.
    pub voice: &'a str,
    pub fx: &'a [crate::cut::Fx],
    /// The bed lanes the render mixes into its clips (§F5.2 S3), files included: a replaced bed file changes
    /// what is heard without touching a single segment.
    pub lanes: &'a [crate::cut::Lane],
    /// The footage frame the encoder is given (`Materials::src_shape`), which decides the frame box of every
    /// clip when the cut names no aspect. A different camera shape is a different picture.
    pub shape: (i32, i32),
}

impl<'a> Input<'a> {
    pub fn stamp_with(&self, tree: &Tree, voice: Option<&str>, roll: i32) -> String {
        self.stamp_of(|line| wav_of(tree, line, voice, roll), disk_facts)
    }

    pub fn stamp_of(
        &self,
        wav: impl Fn(&Entry) -> Option<PathBuf>,
        file_facts: impl Fn(&Path) -> (u64, u64),
    ) -> String {
        hash_text(&self.parts_of(wav, file_facts).join("\n"))
    }

    /// The seven groups as `name=hash` lines, in order: what [`Input::stamp_of`] hashes, exposed so a test
    /// can read one group's line without hashing anything.
    pub fn parts_of(
        &self,
        wav: impl Fn(&Entry) -> Option<PathBuf>,
        file_facts: impl Fn(&Path) -> (u64, u64),
    ) -> Vec<String> {
        self.parts_named(wav, file_facts)
            .into_iter()
            .map(|(name, hash)| format!("{name}={hash}"))
            .collect()
    }

    /// The same seven groups with their names kept — [`changed_parts`] needs the names.
    pub fn parts_named(
        &self,
        wav: impl Fn(&Entry) -> Option<PathBuf>,
        file_facts: impl Fn(&Path) -> (u64, u64),
    ) -> Vec<(&'static str, String)> {
        vec![
            ("settings", hash_text(&settings_text(self.settings))),
            ("segments", hash_text(&segments_text(self.segs))),
            (
                "lines",
                hash_text(&lines_text(self.lines, &wav, &file_facts)),
            ),
            (
                "sources",
                hash_text(&sources_text(self.sources, &file_facts)),
            ),
            ("aspect", hash_text(self.aspect)),
            ("voice", hash_text(self.voice)),
            ("narration", hash_text("on")),
            // Appended after the flowchart's seven: the three things the render reads that the diagram's
            // wording folds into "the segments" — the effects drawn over them, the beds mixed under them and
            // the frame they are drawn onto. Order is fixed here, like the seven above.
            ("effects", hash_text(&effects_text(self.fx))),
            ("beds", hash_text(&beds_text(self.lanes, &file_facts))),
            // Only the shape, not `cut.shift`: a row shift says which camera a segment is SHOWN on, and
            // `segments_text` already leaves `cam` out for the same reason — moving a recording between rows
            // must not force the whole video to re-encode.
            (
                "frame",
                hash_text(&format!("{}x{}", self.shape.0, self.shape.1)),
            ),
        ]
    }
}

/// The seven groups in the flowchart's order plus the render's own three, hashed one by one. `stamp` is the hash of these joined, so a
/// group that changes alone still moves it and still names itself.
pub fn parts(
    input: &Input,
    wav: impl Fn(&Entry) -> Option<PathBuf>,
    file_facts: impl Fn(&Path) -> (u64, u64),
) -> Vec<(&'static str, String)> {
    input.parts_named(wav, file_facts)
}

/// The stamp of one state: 16 hex characters.
pub fn stamp(
    input: &Input,
    wav: impl Fn(&Entry) -> Option<PathBuf>,
    file_facts: impl Fn(&Path) -> (u64, u64),
) -> String {
    input.stamp_of(wav, file_facts)
}

// ---- what goes into each group ---------------------------------------------------

fn settings_text(settings: &Produce) -> String {
    let mut text = String::new();
    text.push_str(&format!("container={:?}\n", settings.container));
    text.push_str(&format!("codec={:?}\n", settings.codec));
    text.push_str(&format!("preset={:?}\n", settings.preset));
    text.push_str(&format!("resolution={:?}\n", settings.resolution));
    text.push_str(&format!("frame_rate={:?}\n", settings.frame_rate));
    text.push_str(&format!("vfr={}\n", settings.vfr));
    text.push_str(&format!("mono={}\n", settings.mono));
    text.push_str(&format!("audio_kbps={}\n", settings.audio_kbps));
    text.push_str(&format!("subtitles={:?}\n", settings.subtitles));
    let mut languages = settings.translate.clone();
    languages.sort();
    text.push_str(&format!("translate={}\n", languages.join(",")));
    // P.policy.gameVolume (§10, 0.22 by default): how loud the original sits under the narration. Hidden
    // when narration is off, but still hashed — a tick that changes the mix changes the file.
    text.push_str(&format!("game_volume={:.6}\n", settings.game_volume));
    text.push_str(&format!("crf={}\n", settings.crf));
    text.push_str(&format!("blurred_edges={}\n", settings.blurred_edges));
    text
}

fn segments_text(segs: &[Seg]) -> String {
    let mut text = String::new();
    for seg in segs {
        text.push_str(&format!(
            "{}\t{}\t{}\t{:.6}\t{:.6}\t{}\t{}\t{}\n",
            seg.s, seg.e, seg.ins, seg.dur, seg.rate, seg.mute, seg.lane, seg.ss
        ));
    }
    text
}

//  line that has never been spoken is not a line whose wav is zero bytes, and conflating them would let a
/// failed synthesis look up to date.
fn lines_text(
    lines: &[Entry],
    wav: &impl Fn(&Entry) -> Option<PathBuf>,
    file_facts: &impl Fn(&Path) -> (u64, u64),
) -> String {
    let mut text = String::new();
    for line in lines {
        let sound = match wav(line) {
            Some(path) => {
                let (size, mtime) = file_facts(&path);
                format!("{size}+{mtime}")
            }
            None => "no wav".to_string(),
        };
        text.push_str(&format!("{}\t{}\t{}\t{sound}\n", line.s, line.e, line.text));
    }
    text
}

//  "matter" for which kind is how an effect change would slip past the hash.
fn effects_text(fx: &[crate::cut::Fx]) -> String {
    let mut text = String::new();
    for effect in fx {
        text.push_str(&format!(
            "{}\t{:.6}\t{:.6}\t{:.6}\t{:.6}\t{}\t{}\t{:?}\t{:?}\t{:?}\t{:?}\t{:.6}\t{}\t{:.6}\t{}\t{}\t{}\t{}\n",
            effect.kind, effect.t, effect.dur, effect.trans, effect.tout, effect.ease, effect.stay,
            effect.cx, effect.cy, effect.hf, effect.wf, effect.rate, effect.snd, effect.gain,
            effect.text, effect.src, effect.cam, effect.lane
        ));
    }
    text
}

/// The bed lanes: name, the file it plays plus that file's `(size, mtime)`, where it starts, where in the
/// file it starts and how long it runs. A bed is read by the render, so its bytes weigh here as a source's do.
fn beds_text(lanes: &[crate::cut::Lane], file_facts: &impl Fn(&Path) -> (u64, u64)) -> String {
    let mut text = String::new();
    for lane in lanes {
        let (size, mtime) = file_facts(Path::new(&lane.src));
        text.push_str(&format!(
            "{}\t{}\t{}+{}\t{:.6}\t{:.6}\t{:.6}\n",
            lane.name, lane.src, size, mtime, lane.at, lane.off, lane.dur
        ));
    }
    text
}

fn sources_text(sources: &[project::Source], file_facts: &impl Fn(&Path) -> (u64, u64)) -> String {
    let mut text = String::new();
    for source in sources.iter().filter(|source| source.footage) {
        let (size, mtime) = file_facts(Path::new(&source.path));
        text.push_str(&format!("{}\t{size}+{mtime}\n", source.path));
    }
    text
}

pub fn hash_text(text: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.as_bytes() {
        hash = (hash ^ u64::from(*byte)).wrapping_mul(0x100_0000_01b3);
    }
    format!("{hash:016x}")
}

pub fn disk_facts(path: &Path) -> (u64, u64) {
    let Ok(meta) = fs::metadata(path) else {
        return (0, 0);
    };
    let seconds = meta
        .modified()
        .ok()
        .and_then(|when| when.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |span| span.as_secs());
    (meta.len(), seconds)
}

// ---- where it lives and how it is read -------------------------------------------

pub fn gate_stamp(
    tree: &Tree,
    settings: &Produce,
    cut: &crate::cut::Cut,
    sources: &[project::Source],
    voice: Option<&str>,
    shape: (i32, i32),
) -> String {
    let entries = crate::narration::load(tree).unwrap_or_default().entries;
    let input = Input {
        settings,
        segs: &cut.segs,
        lines: &entries,
        sources,
        aspect: &cut.aspect,
        voice: voice.unwrap_or(""),
        fx: &cut.fx,
        lanes: &cut.lanes,
        shape,
    };
    input.stamp_of(|line| wav_of(tree, line, voice, line.roll), disk_facts)
}

pub fn read_stamp(tree: &Tree) -> Option<String> {
    let text = fs::read_to_string(tree.final_stamp()).ok()?;
    let hash = text.trim().to_string();
    (!hash.is_empty()).then_some(hash)
}

/// Write the new stamp after a successful encode — F5.2 S10's step, and only ever reached through
/// [`crate::produce_render::stamp_written`]. The folder is created because a first render has never had one.
pub fn write_stamp(tree: &Tree, hash: &str) -> Result<(), String> {
    let path = tree.final_stamp();
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|why| format!("{}: {why}", dir.display()))?;
    }
    fs::write(&path, format!("{hash}\n")).map_err(|why| format!("{}: {why}", path.display()))
}

//  then write the new stamp. `None` on the left is a project that has never been produced.
pub fn skip_encode(stored: Option<&str>, current: &str) -> bool {
    stored == Some(current)
}

pub fn changed_parts<'a>(
    stored: &[(&'a str, String)],
    current: &[(&'a str, String)],
) -> Vec<&'a str> {
    stored
        .iter()
        .zip(current.iter())
        .filter(|(before, after)| before.1 != after.1)
        .map(|(_, after)| after.0)
        .collect()
}

// ---- what the log says -----------------------------------------------------------

pub fn wrote_stamp_log(hash: &str) -> String {
    format!(">>> {STAMP_FILE}: {hash}")
}
