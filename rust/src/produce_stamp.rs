//! F5.3 What "up to date" means — `spec/08-produce.md` §F5.3. One hash of everything the render reads,
//! kept in `produce/final.stamp` beside the video, so a ▶ over an unchanged project costs nothing: the
//! encode is skipped and the file is left exactly as it was.
//!
//! The seven groups the flowchart lists are hashed **separately** ([`parts`]) before they are hashed
//! together ([`stamp`]), because "not up to date" is worth more than a bare yes/no: [`changed_parts`] can
//! say whether it was the encoder settings or one narration wav that moved. That costs nothing and stops
//! the stamp being one opaque blob nobody can argue with.
//!
//! What is deliberately **not** in it:
//! - the output path — §A fixes the file as `produce/final.<container>` ([`crate::layout::Tree::final_video`]), so
//!   the container is already one of the settings and a renamed or moved output can never look like a new
//!   render. Renaming the video must not force a re-encode; that is the whole point of the stamp.
//! - the upload text — no title, no description, no thumbnail, no upload record. Those are written once per
//!   project and gated by their own file, `produce/publish/publish.json` ([`crate::publish::is_written]);
//!   deleting `produce/publish/` ([`crate::publish::start_over]) starts the text over without touching the
//!   video, and re-drawing a thumbnail must never make an encode look stale.
//!
//! This module only *stores* the answer to "the same hash?". **When** a run may write it is F5.2's rule and
//! stays in [`crate::produce_render::stamp_written`]: a run that skipped the encode, or failed, writes none.
//! Nothing here spawns a process, and no GTK: like `produce_render`, this is arithmetic over what the render
//! would read, so it can be tested with no recording and no encoder.

use std::fs;
use std::path::{Path, PathBuf};

use crate::cut::Seg;
use crate::layout::Tree;
use crate::narration::Entry;
use crate::narration;
use crate::narrate_tts;
use crate::project::{self, Produce};

/// The stamp's own name, spelled out once so a test can pin that it sits beside the video rather than in
/// the scratch folder with the clips (which are cleared at the start of every run).
pub const STAMP_FILE: &str = "final.stamp";

/// §F5.2 S10 (`the run writes the stamp once the render returns without error`): F5.3 stores what ▶ has to
/// compare, and this says whether a comparison was even made — kept as one rule in `produce_render` so the
/// two files cannot disagree about a run that encoded nothing.
pub use crate::produce_render::stamp_written;

// ---- the input -------------------------------------------------------------------

/// The wav a line was spoken into, or `None` when it never was. A take is named by what was spoken — the
/// same key its TTS request was cached under ([`crate::narration::tts_key`] →
/// [`crate::narrate_tts::take_path`]) — so re-rolling a line or switching voice keeps its words and changes
/// its sound, and the stamp follows. `voice` is the page's selected voice (`None` = the session's own), and
/// `roll` comes from `narrate/takes.json`, which this module does not read: the caller knows both, and a
/// stamp built from anything else would call a re-drawn take up to date. A line with no words has no wav.
pub fn wav_of(tree: &Tree, line: &Entry, voice: Option<&str>, roll: i32) -> Option<PathBuf> {
    if line.text.trim().is_empty() {
        return None;
    }
    let mut salted = line.clone();
    salted.roll = roll;
    Some(narrate_tts::take_path(
        tree,
        &narration::tts_key(&salted, voice, None),
    ))
}

/// Everything the render reads, borrowed rather than owned: the stamp is a summary of state the caller
/// already holds (the project, the cut's segments, the narration's lines) plus what the file system says
/// about the files those refer to.
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
    /// §1's narration flag (`Project::no_narration`): with it on, no line is spoken at all.
    pub no_narration: bool,
}

impl<'a> Input<'a> {
    /// The stamp as it stands on disk: the real pair of lookups, [`wav_of`] for where a line's synthesis is
    /// and [`disk_facts`] for what the file system says about it. `voice`/`roll` are the two things a take
    /// depends on that the line itself does not carry (see [`wav_of`]).
    pub fn stamp_with(&self, tree: &Tree, voice: Option<&str>, roll: i32) -> String {
        self.stamp_of(|line| wav_of(tree, line, voice, roll), disk_facts)
    }

    /// The stamp with the file system handed in: `wav` says which file a line's synthesis lives in and
    /// `file_facts` reads `(size, mtime seconds)` out of it. Both rules are pure arithmetic given these two,
    /// which is what lets a test move a wav's size without owning a recording. The real pair is
    /// [`wav_of`] + [`disk_facts`].
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
            ("lines", hash_text(&lines_text(self.lines, &wav, &file_facts))),
            ("sources", hash_text(&sources_text(self.sources, &file_facts))),
            ("aspect", hash_text(self.aspect)),
            ("voice", hash_text(self.voice)),
            // The flag alone: with narration off no line is spoken, and the lines themselves must not be
            // able to hold the video up-to-date behind the tick.
            ("narration", hash_text(if self.no_narration { "off" } else { "on" })),
        ]
    }
}

/// The seven groups in the flowchart's order, hashed one by one. `stamp` is the hash of these joined, so a
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

/// §A's settings, one named line each so a change in one row moves the hash and only that row's line is
/// rewritten. Written as `name=value` text rather than by hashing the struct: serde field order is a
/// compile-time accident of declaration, and this list is the deliberate answer to "which settings make a
/// render stale" — dropping a field from it is then visible in the diff.
///
/// In: container, codec, preset, resolution, frame rate with its two ticks, audio bitrate, subtitle choice,
/// the translation ticks, `game_volume` (P.policy.gameVolume), CRF and the frame-edges tick. Not in it: the
/// output path, which is not a setting but a consequence of the container (§ module doc).
fn settings_text(settings: &Produce) -> String {
    let mut text = String::new();
    text.push_str(&format!("container={:?}\n", settings.container));
    text.push_str(&format!("codec={:?}\n", settings.codec));
    text.push_str(&format!("preset={:?}\n", settings.preset));
    text.push_str(&format!("resolution={:?}\n", settings.resolution));
    // The number and what it means: 30 as a target and 30 as a ceiling are different files.
    text.push_str(&format!("frame_rate={:?}\n", settings.frame_rate));
    text.push_str(&format!("vfr={}\n", settings.vfr));
    text.push_str(&format!("mono={}\n", settings.mono));
    text.push_str(&format!("audio_kbps={}\n", settings.audio_kbps));
    text.push_str(&format!("subtitles={:?}\n", settings.subtitles));
    // Sorted so a re-order of the same ticks is not a change; the languages themselves are.
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

/// The segments the render reads: bounds, what is inserted over them (`ins` — empty for plain footage, an
/// asset path, or a `copy:` stretch of the session), how long that runs, its rate, whether it is silent, and
/// which lane's sound an insert replaces. Nothing else: `cam` (which picture row shows the segment) and
/// `quiet` (which lanes a *preview* hushes) change nothing about the encoded file, so moving a segment
/// between rows must not force the whole video to re-encode.
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

/// The lines: bounds, the words, and the synthesis's `(size, mtime)`. `no wav` is a value of its own — a
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

/// The footage sources: the path as §1 spells it, plus `(size, mtime)`. A re-recorded or replaced recording
/// moves the hash even when every segment still points at the same second of it. Non-footage sources are
/// left out — a narration reference is read by the Narrate page's voice sample, not by the encoder, and the
/// voice that clones it is hashed under `voice`.
fn sources_text(sources: &[project::Source], file_facts: &impl Fn(&Path) -> (u64, u64)) -> String {
    let mut text = String::new();
    for source in sources.iter().filter(|source| source.footage) {
        let (size, mtime) = file_facts(Path::new(&source.path));
        text.push_str(&format!("{}\t{size}+{mtime}\n", source.path));
    }
    text
}

/// A short hash of our own rather than a crate — the same reasoning as
/// [`crate::fix_transcripts::block_key`]: the value only has to be stable and unique enough to answer "the
/// same render?", it is read by nothing but this module, and `Hasher`'s output is documented as an API
/// surface that may change between compilers. FNV-1a over the canonical text; 16 hex characters.
fn hash_text(text: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.as_bytes() {
        hash = (hash ^ u64::from(*byte)).wrapping_mul(0x100_0000_01b3);
    }
    format!("{hash:016x}")
}

/// Size and mtime of a file, as the two numbers the stamp spells. A missing file is `(0, 0)`, which is how
/// "not spoken yet" differs from "spoken as an empty file": the size says nothing on its own, and `Err` has
/// to read as *nothing there* rather than as a change every run.
///
/// Not [`crate::shell`]'s helper of the same kind: that one packs size and mtime into a single number for
/// Prepare's transcripts, where only equality matters. Here both fields go on the line separately so a log
/// or a reader can tell a re-record from a touch.
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

/// `<stem>.stamp` beside the video: `produce/final.stamp`, next to `produce/final.<container>` (§A fixes
/// both names, so one stamp file per project serves every container — switching mp4→webm is a settings
/// change inside the hash, not a second stamp nobody would ever clear).
pub fn stamp_path(tree: &Tree) -> PathBuf {
    tree.final_stamp()
}

/// The stored hash. Anything unreadable — no file, a hand-deleted one, a half-written line from a crash —
/// reads as `None`, and `None` means encode: the stamp may only ever let a run skip work it can prove was
/// done, never hide that it cannot tell.
pub fn read_stamp(tree: &Tree) -> Option<String> {
    let text = fs::read_to_string(stamp_path(tree)).ok()?;
    let hash = text.trim().to_string();
    (!hash.is_empty()).then_some(hash)
}

/// Write the new stamp after a successful encode — F5.2 S10's step, and only ever reached through
/// [`crate::produce_render::stamp_written`]. The folder is created because a first render has never had one.
pub fn write_stamp(tree: &Tree, hash: &str) -> Result<(), String> {
    let path = stamp_path(tree);
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|why| format!("{}: {why}", dir.display()))?;
    }
    fs::write(&path, format!("{hash}\n")).map_err(|why| format!("{}: {why}", path.display()))
}

/// The ▶ test of the flowchart: the same hash means the encode is skipped, anything else means encode and
//  then write the new stamp. `None` on the left is a project that has never been produced.
pub fn skip_encode(stored: Option<&str>, current: &str) -> bool {
    stored == Some(current)
}

/// Which groups differ between two runs, in the flowchart's order. A run that has to encode can say why —
/// "the lines moved" is a different sentence from "the encoder settings did" — and the answer falls out of
/// hashing the groups separately rather than being guessed from one number.
pub fn changed_parts<'a>(stored: &[(&'a str, String)], current: &[(&'a str, String)]) -> Vec<&'a str> {
    stored
        .iter()
        .zip(current.iter())
        .filter(|(before, after)| before.1 != after.1)
        .map(|(_, after)| after.0)
        .collect()
}

// ---- what the log says -----------------------------------------------------------

/// §F5.3 (`yes → the encode is skipped`): the run says so instead of finishing in a third of a second and
/// leaving the person wondering whether anything happened. Naming the file keeps it parallel with F5.2's
/// `>>> <file>` lines.
pub const UP_TO_DATE_LOG: &str = ">>> produce/final.mp4 is up to date \u{2014} nothing to encode";

/// The other branch, said before the work starts so a long run has an explanation on its first line.
pub const NOT_UP_TO_DATE_LOG: &str = ">>> not up to date \u{2014} encoding";

/// F5.2 S10's last word: the stamp written after a render that returned without error, with the hash that
/// will make the next ▶ cheap. The hash goes in because these lines are compared between runs, and a stale
/// stamp somebody wants to delete has to be findable by eye.
pub fn wrote_stamp_log(hash: &str) -> String {
    format!(">>> {STAMP_FILE}: {hash}")
}
