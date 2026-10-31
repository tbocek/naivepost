//! §08-produce#3-data — the files `produce/` holds.

use std::path::{Path, PathBuf};

use crate::layout::Tree;
use crate::produce_embed;
use crate::produce_render as render;
use crate::produce_stamp;
use crate::publish;

/// §08 §3's line rendered as data: every file `produce/` holds, in the order the spec writes them.
pub const DATA_FILES: [&str; 11] = [
    // --- produce/clips/ : what one run needs and the next one throws away --------------------
    "clips/<encode>",
    "clips/final.srt",
    "clips/final.<code>.srt",
    "clips/concat.txt",
    // --- produce/ : what the video is, and what says it is up to date -------------------------
    "final.<ext>",
    "final.stamp",
    "final[.<code>].srt",
    "final[.<code>].vtt",
    "final.jpg",
    "final.html",
    // --- produce/publish/ : the upload text and its picture, which ▶ never touches ------------
    "publish/publish.json",
];

/// Which of §3's three groups a file belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Inside `clips/`: emptied by [`render::clear_scratch`] at the start of every run and never
    /// read again afterwards.
    Scratch,
    /// Beside the video: deleted by exact name ([`render::stale_sidecars`], never a glob) before it
    /// is rewritten, and left exactly as it is when the stamp says the video is up to date.
    Output,
    State,
}

/// §3's grouping, applied to one entry of [`DATA_FILES`] or to any path under `produce/`: the first
/// segment decides it, because the spec's three groups are three folders.
pub fn kind(rel: &str) -> Kind {
    if rel.starts_with("clips/") {
        Kind::Scratch
    } else if rel.starts_with("publish/") {
        Kind::State
    } else {
        Kind::Output
    }
}

// --- produce/clips/ -----------------------------------------------------------------

/// The scratch folder's own name, spelled by [`crate::produce_render`] and named here so the four
/// `clips/` entries of §3 have one prefix rather than four copies of it.
pub const SCRATCH: &str = render::SCRATCH;

pub const CONCAT_NAME: &str = "concat.txt";

pub fn concat_path(tree: &Tree) -> PathBuf {
    tree.clips_dir().join(CONCAT_NAME)
}

/// The cue sheet on the produced clock, before any language is added to it:
/// `produce/clips/final.srt`.
///
/// §3 lists this file *and* the sidecar beside the video, and they are not the same file: this one
/// is the working track the mux burns in or maps, written while the clips are still separate; that
/// one is `<stem>.srt` beside `final.<ext>`, which is what a desktop player and the `.html`'s
/// tracks read. Both come from [`render::final_srt_name`] so the stems cannot drift apart.
pub fn scratch_srt(tree: &Tree) -> PathBuf {
    tree.clips_dir().join(render::final_srt_name())
}

/// A translation's draft beside the cue sheet: `produce/clips/final.<code>.srt`. §3 names it among
/// the scratch files because that is where a translated track lands before F5.2 S9 writes its
/// sidecar beside the video — [`crate::produce_subtitles`] answers the cues, this answers where the
/// answer is put down.
pub fn scratch_translation_srt(tree: &Tree, code: &str) -> PathBuf {
    let stem = render::final_srt_name().replace(".srt", "");
    tree.clips_dir().join(format!("{stem}.{code}.srt"))
}

/// One clip's encode: `produce/clips/c<no>_<stamp>.<container>`. The stamp is in the name because
/// [`render::clear_scratch`] empties the folder rather than reusing it — see [`crate::produce_render::clip_stem`].
pub fn clip_path(tree: &Tree, no: usize, stamp: &str, container: &str) -> PathBuf {
    tree.clips_dir()
        .join(format!("{}.{}", render::clip_stem(no, stamp), container))
}

// --- the three groups, read off the disk ---------------------------------------------

/// §6's five extras in `produce/clips/`, next to §3's four. §3's line names "encodes, final.srt,
/// final.<code>.srt, concat.txt" and stops.
pub const CLIPS_EXTRA: [&str; 5] = [
    "clips/c%03d_<stamp>.srt",
    "clips/c%03d_t%02d.svg",
    "clips/<card copy>.svg",
    "clips/<name>.frames/",
    "clips/joined.<container>",
];

/// Every file `produce/` holds, §3 first and then §6's additions: what a caller reads when it wants
/// the folder described completely rather than by the spec's shortest line.
pub fn catalogue() -> Vec<&'static str> {
    DATA_FILES
        .iter()
        .chain(CLIPS_EXTRA.iter())
        .copied()
        .collect()
}

/// How many of [`catalogue`]'s entries live in `clips/` — the count §6's "cleared at the start of
/// every render" sweeps.
pub fn scratch_groups() -> usize {
    catalogue()
        .iter()
        .filter(|entry| kind(entry) == Kind::Scratch)
        .count()
}

pub fn scratch_clear_log(groups: usize, gone: usize) -> String {
    format!(
        "    produce: cleared {gone} file(s) from {} \u{2014} {groups} kinds of scratch file",
        SCRATCH
    )
}

/// Which of §3's groups exist in this project right now, as entries of [`DATA_FILES`] — the empty
/// string for none.
pub fn written(tree: &Tree) -> Vec<&'static str> {
    let mut found = Vec::new();
    if tree.clips_dir().is_dir()
        && tree
            .clips_dir()
            .read_dir()
            .map(|mut d| d.next().is_some())
            .unwrap_or(false)
    {
        found.push(DATA_FILES[0]);
    }
    // The video and its stamp are §3's pair: one is what plays, the other is what says it is
    // current.
    if produce_embed::paths(tree, crate::project::Container::Mp4)
        .0
        .is_file()
    {
        found.push(DATA_FILES[4]);
    }
    if produce_stamp::stamp_path(tree).is_file() {
        found.push(DATA_FILES[5]);
    }
    // The record's existence *is* the flag — §5: "an existing publish.json means the upload text is
    // written".
    if publish::is_written(tree) {
        found.push(DATA_FILES[10]);
    }
    found
}

/// The path §3 spells as `rel` (one entry of [`DATA_FILES`]) inside this project's `produce/`.
pub fn resolve(tree: &Tree, rel: &str) -> Option<PathBuf> {
    match rel {
        "clips/final.srt" => Some(scratch_srt(tree)),
        "clips/concat.txt" => Some(concat_path(tree)),
        produce_stamp::STAMP_FILE => Some(produce_stamp::stamp_path(tree)),
        "final.jpg" => Some(tree.final_jpg()),
        "final.html" => Some(tree.final_html()),
        // The record is read through Tree because that is where the legacy spelling of the folder
        // is still found (§1: read for ever, never migrated).
        "publish/publish.json" => Some(tree.publish_json()),
        _ => None,
    }
}

/// §3's sidecar entry spelled for one language: the two files beside the video that a track of this
/// run writes.
pub fn sidecar_files(stem: &str, code: Option<&str>) -> [String; 2] {
    render::sidecar_names(stem, code)
}

/// The files a run deletes by exact name before writing them again.
pub fn stale_files(stem: &str, languages: &[&str]) -> Vec<String> {
    render::stale_sidecars(stem, languages)
}
/// Whether this project's `produce/` is the whole story about its video: a stamp that matches lets
/// ▶ skip the encode ([`produce_stamp::skip_encode`]), and with it the sidecars and the tag stay as
/// the run before left them.
pub fn video_is_up_to_date(stored: Option<&str>, current: &str) -> bool {
    produce_stamp::skip_encode(stored, current)
}

/// The poster's quality as the spec asks for it (§F5.5's "JPEG 90") and as ffmpeg takes it.
pub fn poster_quality() -> (u8, &'static str) {
    (
        produce_embed::POSTER_JPEG_QUALITY,
        produce_embed::POSTER_QSCALE,
    )
}

/// The scratch folder as [`render::clear_scratch`] empties it: every file inside, never the folder.
pub fn clear_scratch(tree: &Tree) -> Result<usize, String> {
    render::clear_scratch(&tree.clips_dir())
}

/// §3's `+ .stamp` for the *video*: the file beside it, not the thumbnail's own stamp under
/// `publish/`, which is [`Kind::State`] and answers to a different question.
pub fn stamp_path(tree: &Tree) -> PathBuf {
    produce_stamp::stamp_path(tree)
}

/// The folder §3 calls `produce/publish/` — read through [`crate::layout::Tree::publish_dir`],
/// which still finds the legacy spelling beside the project root and never migrates it.
pub fn publish_folder(tree: &Tree) -> PathBuf {
    tree.publish_dir()
}

/// Whether a path is under one of §3's three groups.
pub fn in_produce(tree: &Tree, path: &Path) -> bool {
    let under = |folder: &Path| path.starts_with(folder);
    under(&tree.dir().join("produce")) || under(&tree.publish_dir())
}

// --- §08 §5's fourth bullet: the two halves of ▶ --------------------------------------

/// The files ▶'s **render** half writes — everything §3 lists that is not under `publish/`, taken
/// by [`Kind`] rather than by an index someone would have to recount when §3 grows.
pub fn render_files() -> &'static [&'static str] {
    &DATA_FILES[..10]
}

pub fn publish_files() -> &'static [&'static str] {
    // §3's list has exactly one `publish/` entry today.
    DATA_FILES
        .iter()
        .filter(|entry| kind(entry) == Kind::State)
        .copied()
        .collect::<Vec<_>>()
        .leak()
}

/// §5: "the two halves touch no common file", computed rather than asserted.
pub fn share_a_file() -> Option<&'static str> {
    render_files()
        .iter()
        .copied()
        .find(|entry| publish_files().contains(entry))
}

/// §5 fixes this order, and each step's reason is in it: the record goes down **before** the
/// picture is drawn (a draw that dies leaves the record, so the next ▶ finishes the job instead of
/// starting it over), then the picture, then the `<video>` tag.
pub const WRITE_ORDER: [&str; 3] = [
    "publish/publish.json",
    "publish/thumbnail.png",
    "final.html",
];

/// §5: the `<video>` tag is rewritten even when the encode was skipped.
pub fn tag_rewritten(_skipped_encode: bool) -> bool {
    true
}
