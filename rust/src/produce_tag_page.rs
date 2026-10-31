//! F5.5 S5 — the `<video>` tag page the run writes after BOTH halves of ▶ (spec/08-produce.md §F5.1
//! S5, §F5.5). `produce_embed` owns every rule (the tag's text, which files count as tracks, the
//! two notes).

use std::path::PathBuf;

use crate::layout::Tree;
use crate::produce_embed as embed;
use crate::project::{Codec, Container};
use crate::roles::Language;

/// The poster's ffmpeg leg, caller-supplied exactly as `narrate_tts::speak_line` supplies both of
/// its network legs: it takes the argv [`embed::poster_command`] built, so no test needs an encoder
/// standing.
pub type MakePoster = dyn Fn(&[String]) -> Result<(), String>;

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

#[derive(Debug, Clone, PartialEq)]
pub struct TagPage {
    pub path: PathBuf,
    pub written: bool,
    pub poster: bool,
    pub tracks: usize,
}

fn base(path: &std::path::Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_default()
}

pub fn build_and_write(
    tree: &Tree,
    container: Container,
    codec: Codec,
    own_language: &str,
    known: &[Language],
    make_poster: &MakePoster,
    mut log: impl FnMut(&str),
) -> TagPage {
    let (video, jpg, html) = embed::paths(tree, container);

    let printed = tree.thumbnail_png();
    let plain = tree.thumbnail_plain_png();
    // `Option`, not a path that does not exist: "no poster" must be an answer the type carries, so
    // the leg below cannot be reached with a made-up file name.
    let source = match (printed.is_file(), plain.is_file()) {
        (true, _) => Some(printed),
        (false, true) => Some(plain),
        (false, false) => None,
    };
    let poster = match &source {
        None => {
            log(&embed::no_poster_log(embed::no_poster_reason()));
            false
        }
        Some(from) => match make_poster(&embed::poster_command(from, &jpg)) {
            Ok(()) => jpg.is_file(),
            Err(_) => false,
        },
    };

    let folder = jpg
        .parent()
        .unwrap_or(std::path::Path::new("."))
        .to_path_buf();
    let tracks = embed::tracks(&embed::list_vtt(&folder), own_language, known);

    let body = embed::tag(
        &base(&video),
        poster.then(|| base(&jpg)).as_deref(),
        &tracks,
    );
    let written = embed::write_tag(tree, &body).is_ok();
    if !written {
        log(&embed::unwritable_log(&html.display().to_string()));
        return TagPage {
            path: html,
            written,
            poster,
            tracks: tracks.len(),
        };
    }
    log(&embed::wrote_tag_log(&html.display().to_string()));

    if let Some(note) = embed::web_unplayable(container, codec) {
        log(note);
    }
    if let Some(note) = embed::note_http(&tracks) {
        log(note);
    }

    TagPage {
        path: html,
        written,
        poster,
        tracks: tracks.len(),
    }
}
