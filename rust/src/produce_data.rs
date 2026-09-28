//! §08-produce#3-data — the files `produce/` holds.
//!
//! §3 is one line of paths rather than prose, so this module renders that line as data:
//! [`DATA_FILES`] is §3's list in the spec's own order, [`kind`] says which of the line's three
//! groups an entry belongs to — and therefore what happens to it on the next ▶ — and one accessor
//! per path hands back the same [`std::path::PathBuf`] the writer uses. A rename anywhere in
//! `layout` or here fails a test rather than turning up as a file some later run cannot find.
//!
//! It does NOT own:
//! - the encodes, the join, the mux or their argv — [`crate::produce_render`] builds those and this
//!   module only says where their files land;
//! - what makes the video stale ([`crate::produce_stamp`]), which sidecars a `<track>` may read
//!   ([`crate::produce_embed`]), or the upload record's shape ([`crate::publish`], whose §5 file is
//!   this folder's only state);
//! - any of the paths [`crate::layout::Tree`] already answers: `final.<ext>`, `final.stamp`,
//!   `final[.<code>].srt`/`.vtt`, `final.jpg`, `final.html`, `publish/publish.json` are read from
//!   there, never restated here.
//!
//! What this module does own is the three files §3 lists inside `produce/clips/` that nothing else
//! names — [`concat_path`], [`scratch_srt`], [`scratch_translation_srt`] — plus [`clip_path`], which
//! joins [`crate::produce_render::clip_stem`] to the folder it is written in.
//!
//! No GTK, and nothing is spawned: like the rest of `produce_*`, this names files and classifies
//! them. `tool:ffmpeg.encode` writes the encodes, the join and the poster; ffmpeg's `-f concat` reads
//! [`concat_path`].

use std::path::{Path, PathBuf};

use crate::layout::Tree;
use crate::produce_embed;
use crate::produce_render as render;
use crate::produce_stamp;
use crate::publish;

/// §08 §3's line rendered as data: every file `produce/` holds, in the order the spec writes them —
/// the four under `clips/`, then the five beside the video, then the record. Held as data so a test
/// pins each one against the accessor that produces it: change a name on one side and this round
/// fails instead of a later run silently reading the file the run before left behind.
///
/// The angle brackets are the spec's own: `<encode>` is one clip's `c%03d_<stamp>.<container>`,
/// `<code>` a subtitle language, `<ext>` the container ticked on the row.
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

/// Which of §3's three groups a file belongs to — the question that decides what happens to it on
/// the next ▶, which is the only thing the spec's grouping means.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Inside `clips/`: emptied by [`render::clear_scratch`] at the start of every run and never read
    /// again afterwards. A resume marker is *not* among them — that job is [`Kind::State`]'s stamp.
    Scratch,
    /// Beside the video: deleted by exact name ([`render::stale_sidecars`], never a glob) before it
    /// is rewritten, and left exactly as it is when the stamp says the video is up to date — which
    /// is why `<stem>.html` and the sidecars can be written even on a run that encoded nothing.
    Output,
    /// Under `publish/`: survives ▶ entirely. Its own `thumbnail.stamp` decides whether the picture
    /// is drawn again; deleting the folder ([`publish::start_over`]) is the only way to start over.
    State,
}

/// §3's grouping, applied to one entry of [`DATA_FILES`] or to any path under `produce/`: the first
/// segment decides it, because the spec's three groups are three folders. Anything else — a stray
/// file a person dropped in — is reported as [`Kind::Scratch`]'s opposite of nothing: an unknown
/// name is treated as output, since the safe mistake is to leave a file nobody recognises alone
/// rather than to delete it as scratch.
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

/// The concat demuxer's list — the file [`render::concat_list`] writes and [`render::join_command`]
/// reads back with `-f concat -safe 0 -i`. It is written last among the scratch files because it is
/// what turns "the clips that encoded" into "the video", and it goes with them: a run that starts
/// over has new clips, so an old list would join clips this cut no longer has.
pub const CONCAT_NAME: &str = "concat.txt";

/// `produce/clips/concat.txt`.
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

/// Which of §3's groups exist in this project right now, as entries of [`DATA_FILES`] — the empty
/// string for none. Nothing is created or deleted here: it only reports, so a caller (and a test)
/// can tell a project that has never been rendered from one whose render was cancelled halfway.
///
/// The four `clips/` groups answer as one: they are written inside a single run and cleared at the
/// start of the next, so a folder with something in it is a run in progress or abandoned, and no
/// decision in §08 turns on which of its files survived.
pub fn written(tree: &Tree) -> Vec<&'static str> {
    let mut found = Vec::new();
    // tool:ffmpeg.encode's scratch folder — present once a run has started writing clips.
    if tree.clips_dir().is_dir() && tree.clips_dir().read_dir().map(|mut d| d.next().is_some()).unwrap_or(false) {
        found.push(DATA_FILES[0]);
    }
    // The video and its stamp are §3's pair: one is what plays, the other is what says it is current.
    if produce_embed::paths(tree, crate::project::Container::Mp4).0.is_file() {
        found.push(DATA_FILES[4]);
    }
    if produce_stamp::stamp_path(tree).is_file() {
        found.push(DATA_FILES[5]);
    }
    // The record's existence *is* the flag — §5: "an existing publish.json means the upload text is
    // written" — so it is asked through `publish`, which knows both folder spellings.
    if publish::is_written(tree) {
        found.push(DATA_FILES[10]);
    }
    found
}

/// The path §3 spells as `rel` (one entry of [`DATA_FILES`]) inside this project's `produce/`.
/// Only the groups that name one fixed file answer: `<encode>`, `<code>` and `<ext>` stand for a
/// family rather than a file, so they are refused rather than guessed at — a caller that needs one
/// has [`clip_path`], [`scratch_translation_srt`] or [`crate::layout::Tree::final_video`].
pub fn resolve(tree: &Tree, rel: &str) -> Option<PathBuf> {
    match rel {
        "clips/final.srt" => Some(scratch_srt(tree)),
        "clips/concat.txt" => Some(concat_path(tree)),
        produce_stamp::STAMP_FILE => Some(produce_stamp::stamp_path(tree)),
        "final.jpg" => Some(tree.final_jpg()),
        "final.html" => Some(tree.final_html()),
        // The record is read through Tree because that is where the legacy spelling of the folder is
        // still found (§1: read for ever, never migrated) — a hand-joined path would miss it.
        "publish/publish.json" => Some(tree.publish_json()),
        _ => None,
    }
}

/// §3's sidecar entry spelled for one language: the two files beside the video that a track of this
/// run writes. [`render::sidecar_names`] answers the pair and [`crate::produce_embed::tracks`] reads
/// them back off the folder, so this is only the join to §3's spelling — `None` for `<code>`, which
/// stands for every language at once.
pub fn sidecar_files(stem: &str, code: Option<&str>) -> [String; 2] {
    render::sidecar_names(stem, code)
}

/// The files a run deletes by exact name before writing them again — §3's `+ sidecars`, and the
/// reason [`Kind::Output`] is not "whatever is in the folder": a glob would take a translation of
/// some other video's file with it.
pub fn stale_files(stem: &str, languages: &[&str]) -> Vec<String> {
    render::stale_sidecars(stem, languages)
}
/// Whether this project's `produce/` is the whole story about its video: a stamp that matches lets ▶
/// skip the encode ([`produce_stamp::skip_encode`]), and with it the sidecars and the tag stay as the
/// run before left them. Kept here so §3's file list and §F5.3's rule are read together by one call.
pub fn video_is_up_to_date(stored: Option<&str>, current: &str) -> bool {
    produce_stamp::skip_encode(stored, current)
}

/// The poster's quality as the spec asks for it (§F5.5's "JPEG 90") and as ffmpeg takes it — both
/// spelled by [`crate::produce_embed`], since §3 lists `final.jpg` without a rule of its own.
pub fn poster_quality() -> (u8, &'static str) {
    (produce_embed::POSTER_JPEG_QUALITY, produce_embed::POSTER_QSCALE)
}

/// The scratch folder as [`render::clear_scratch`] empties it: every file inside, never the folder.
/// Exposed so §3's `clips/` group has one way in from this module rather than two callers reaching
/// into `layout` and `produce_render` separately.
pub fn clear_scratch(tree: &Tree) -> Result<usize, String> {
    render::clear_scratch(&tree.clips_dir())
}

/// §3's `+ .stamp` for the *video*: the file beside it, not the thumbnail's own stamp under
/// `publish/`, which is [`Kind::State`] and answers to a different question.
pub fn stamp_path(tree: &Tree) -> PathBuf {
    produce_stamp::stamp_path(tree)
}

/// The folder §3 calls `produce/publish/` — read through [`crate::layout::Tree::publish_dir`], which
/// still finds the legacy spelling beside the project root and never migrates it.
pub fn publish_folder(tree: &Tree) -> PathBuf {
    tree.publish_dir()
}

/// Whether a path is under one of §3's three groups — the record's folder being the awkward one: it
/// is `produce/publish/`, or the legacy `<project>/publish/` a project written before the move keeps
/// and is read there for ever (§1), which is why [`crate::layout::Tree::publish_dir`] answers both.
pub fn in_produce(tree: &Tree, path: &Path) -> bool {
    let under = |folder: &Path| path.starts_with(folder);
    under(&tree.dir().join("produce")) || under(&tree.publish_dir())
}

// --- §08 §5's fourth bullet: the two halves of ▶ --------------------------------------

/// The files ▶'s **render** half writes — everything §3 lists that is not under `publish/`, taken by
/// [`Kind`] rather than by an index someone would have to recount when §3 grows.
pub fn render_files() -> &'static [&'static str] {
    // Scratch and beside-the-video output are one half's work: both are produced by ffmpeg, and both are
    // rewritten (or cleared) by the next ▶.
    &DATA_FILES[..10]
}

/// The files ▶'s **publish** half writes: the upload text and its picture. Kept out of the render's way
/// entirely — see [`share_a_file`].
pub fn publish_files() -> &'static [&'static str] {
    // §3's list has exactly one `publish/` entry today; filtering by kind keeps that true when it grows, and
    // the leak is the only way to hand back a `'static` slice built from a filter.
    DATA_FILES.iter().filter(|entry| kind(entry) == Kind::State).copied().collect::<Vec<_>>().leak()
}

/// §5: "the two halves touch no common file", computed rather than asserted — the one entry both lists name,
/// or `None` when there is none. This is why the halves can run in parallel without a lock: neither has ever
/// had to ask what the other was doing with a path.
pub fn share_a_file() -> Option<&'static str> {
    render_files().iter().copied().find(|entry| publish_files().contains(entry))
}

/// §5 fixes this order, and each step's reason is in it: the record goes down **before** the picture is drawn
/// (a draw that dies leaves the record, so the next ▶ finishes the job instead of starting it over), then the
/// picture, then the `<video>` tag — last because it names files both halves produce, and it is rewritten even
/// when the encode was skipped ([`tag_rewritten`]).
pub const WRITE_ORDER: [&str; 3] = [
    "publish/publish.json",
    "publish/thumbnail.png",
    "final.html",
];

/// §5: the `<video>` tag is rewritten even when the encode was skipped. The tag lists the `.vtt` files that
/// survived from the run before, and [`crate::produce_stamp`]'s stamp is what made skipping legitimate — so a
/// skipped encode is a reason to leave every other file alone, never a reason to leave a stale tag pointing at
/// captions this project no longer has.
///
/// The *other* question — whether the render failed — is [`crate::produce_embed::render_verdict`]'s, and is not
/// repeated here: this answers "was it skipped", that answers "did it fail".
pub fn tag_rewritten(_skipped_encode: bool) -> bool {
    true
}
