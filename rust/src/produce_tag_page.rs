//! F5.5 S5 — the `<video>` tag page the run writes after BOTH halves of ▶ (spec/08-produce.md §F5.1 S5,
//! §F5.5). `produce_embed` owns every rule (the tag's text, which files count as tracks, the two notes);
//! this file is the walk that gathers what is on disk, asks for the poster and writes `final.html`.
//!
//! Two things decide the shape, both straight from the spec:
//! - **The tracks come off THE DISK, not out of the render.** A ▶ that found everything up to date skipped
//!   the encode and wrote no subtitles, yet the `.vtt`s beside the video are that video's subtitles (that
//!   is what the F5.3 stamp made legitimate), so the page is rewritten either way.
//! - **The poster is a JOB, not a lookup.** `embed::poster_command` builds the ffmpeg argv and the
//!   caller's spawner runs it, so nothing here spawns anything — the same reason `produce_render` builds
//!   argv instead of running it, and why a test can answer the poster leg without an encoder.

use std::path::PathBuf;

use crate::layout::Tree;
use crate::produce_embed as embed;
use crate::project::{Codec, Container};
use crate::roles::Language;

/// The poster's ffmpeg leg, caller-supplied exactly as `narrate_tts::speak_line` supplies both of its
/// network legs: it takes the argv [`embed::poster_command`] built, so no test needs an encoder standing.
pub type MakePoster = dyn Fn(&[String]) -> Result<(), String>;

/// Adapt the page's spawner (which takes a [`crate::produce_exec::Command`]) to this module's argv shape,
/// so the poster goes through the same door the encodes do — scripted in a test, real otherwise.
pub fn poster_through<F>(spawn: F) -> Box<MakePoster>
where
    F: Fn(&crate::produce_exec::Command) -> Result<(), String> + 'static,
{
    Box::new(move |argv: &[String]| {
        let command = crate::produce_exec::Command {
            step: "poster",
            log: format!("poster: {}", argv.join(" ")),
            argv: argv.to_vec(),
        };
        spawn(&command)
    })
}

/// What the walk did. `poster` says whether there is a `final.jpg` to point at, `written` whether the
/// page itself landed. A failed page is said in the log and is never the run's verdict — that is
/// [`embed::render_verdict`], and it takes the render's outcome alone.
#[derive(Debug, Clone, PartialEq)]
pub struct TagPage {
    pub path: PathBuf,
    pub written: bool,
    pub poster: bool,
    pub tracks: usize,
}

/// The base name only: the html lives beside the files it names, so its `src=` and `poster=` need no
/// folder in front of them — which is the whole reason the page can be pasted anywhere.
fn base(path: &std::path::Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_default()
}

/// Build the page and write it. Every step's rule is `embed`'s; this function only puts the calls in the
/// spec's order and turns "is it on disk?" answers into the tag's arguments.
pub fn build_and_write(
    tree: &Tree,
    container: Container,
    codec: Codec,
    own_language: &str,
    known: &[Language],
    make_poster: &MakePoster,
    mut log: impl FnMut(&str),
) -> TagPage {
    let (_, jpg, html) = embed::paths(tree, container);

    // 1. The poster: JPEG 90 off the thumbnail the words half drew. No thumbnail is said quietly and the
    //    tag goes out without a `poster=` — a page with no picture still plays.
    let thumbnail = tree.thumbnail_png();
    let poster = if !thumbnail.is_file() {
        log(&embed::no_poster_log(embed::no_poster_reason()));
        false
    } else {
        match make_poster(&embed::poster_command(&thumbnail, &jpg)) {
            // Ok alone is not enough: the file has to be there for `poster="final.jpg"` to be true.
            Ok(()) => jpg.is_file(),
            Err(_) => false,
        }
    };

    // 2. The tracks: whatever `.vtt` files are beside the video, read off the folder rather than out of
    //    the run, ordered by `embed::tracks` with this video's own language first (and `default`).
    //    `final.vtt`'s parent is the produce folder, since `Tree::produce_dir` is private.
    let folder = jpg.parent().unwrap_or(std::path::Path::new(".")).to_path_buf();
    let tracks = embed::tracks(&embed::list_vtt(&folder), own_language, known);

    // 3. The page itself: the bare tag, base names only.
    let body = embed::tag(
        &base(&tree.final_video("mp4")),
        poster.then(|| base(&jpg)).as_deref(),
        &tracks,
    );
    let written = embed::write_tag(tree, &body).is_ok();
    if !written {
        // Said and done: the video and its subtitles are on disk either way, and throwing away the page is
        // not worth failing a run that already produced a file.
        log(&embed::unwritable_log(&html.display().to_string()));
        return TagPage { path: html, written, poster, tracks: tracks.len() };
    }
    log(&embed::wrote_tag_log(&html.display().to_string()));

    // 4. §F5.5's two notes, after the tag line so they read underneath it: the container/codec no browser
    //    plays, and the http-vs-file:// trap that shows a caption-less page rather than an error.
    if let Some(note) = embed::web_unplayable(container, codec) {
        log(note);
    }
    if let Some(note) = embed::note_http(&tracks) {
        log(note);
    }

    TagPage { path: html, written, poster, tracks: tracks.len() }
}
