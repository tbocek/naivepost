//! F5.5 The `<video>` tag — `spec/08-produce.md` §F5.5, wording in `spec/inventory/produce.md:35`. Browsers play no in-band subtitle tracks and `<track>` parses WebVTT alone,
//! so the page needs the `.vtt` files the render wrote beside the video and a tag pointing at them; they are written out with the video.
//!
//! When it runs:
//! - **After both halves of ▶.** The poster is the thumbnail, drawn by the other half of the press.
//! - **The tracks come off the disk, not out of the render.** A ▶ that found everything up to date wrote no subtitles, but the earlier ones are still this video's ([`crate::produce_stamp`]).
//! - **Never a failed render.** A tag that could not be written is said in the log and changes nothing else ([`render_verdict`]).
//!
//! webm carries no subtitle track ([`crate::produce_screen::apply_container_rules`]), yet its `.vtt` files sit beside it and its tag lists them.
//! No GTK, nothing spawned: this builds argv and text; the poster is an ffmpeg job ([`poster_command`]).

use std::fs;
use std::path::{Path, PathBuf};

use crate::layout::Tree;
use crate::project::{Codec, Container};
use crate::roles::Language;

// ---- the tag ---------------------------------------------------------------------

/// One caption track: `srclang` code, the name in a player's menu, and the **base name** of its file (the html lives beside the video).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Track {
    pub code: String,
    pub name: String,
    pub file: String,
}

/// F5.5's tag, exactly as the spec writes it: a bare `<video poster controls src preload="none">` and one
/// `<track>` per language, each indented two spaces, the file ending `</video>\n`.
///
/// *Bare* is the spec's word: a track carries `src`, `srclang` and `default` and nothing else. The prototype's `label="…" kind="subtitles"` and self-closed tag
/// (`spec/inventory/produce.md:35`) are left out: a browser labels a track from `srclang`.
pub fn tag(video: &str, poster: Option<&str>, tracks: &[Track]) -> String {
    let mut out = String::from("<video ");
    if let Some(poster) = poster {
        out.push_str(&format!("poster=\"{}\" ", escape(poster)));
    }
    out.push_str(&format!(
        "controls src=\"{}\" preload=\"none\">\n",
        escape(video)
    ));
    for (index, track) in tracks.iter().enumerate() {
        // Only the first is `default`: two defaults draw two sets of captions over each other in some players.
        let default = if index == 0 { " default" } else { "" };
        out.push_str(&format!(
            "  <track src=\"{}\" srclang=\"{}\"{default}>\n",
            escape(&track.file),
            escape(&track.code)
        ));
    }
    out.push_str("</video>\n");
    out
}

/// The characters that would end an attribute or start a tag, plus `&` for entities.
fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            other => out.push(other),
        }
    }
    out
}

// ---- which tracks, read off the files that are there ------------------------------

/// The `.vtt` sidecars that are actually beside the video, in the order a player should offer them: the
/// video's own language first (and `default`), the translations after it by name.
///
/// `names` is what the caller found on disk (pure, so ordering is testable). `<stem>.vtt` is the session's own language; `<stem>.de.vtt` names its own between the dots;
/// anything not starting from this stem, or an `.srt`, stays out. The stem is [`crate::produce_render::final_srt_name`] minus its extension (§A), so `<stem>.html`, `<stem>.jpg` and the sidecars are one family.
///
/// An empty answer is legal — a video with no captions still gets a tag.
pub fn tracks(names: &[String], own_language: &str, known: &[Language]) -> Vec<Track> {
    // §A fixes the video's name, so its sidecars are `final.vtt` (own language, no code) and `final.<code>.vtt` per translation; anything else is another video's file or an `.srt`.
    let stem = crate::produce_render::final_srt_name()
        .strip_suffix(".srt")
        .unwrap_or_default()
        .to_string();
    let mut own: Option<Track> = None;
    let mut translations: Vec<Track> = Vec::new();
    for name in names {
        // A code sits between the stem and the extension; none is the video's own track (stripping the prefix alone would read `final.vtt` as a language called "vtt").
        let code = match name.strip_prefix(&format!("{stem}.")).and_then(|rest| {
            if rest == "vtt" {
                Some("")
            } else {
                rest.strip_suffix(".vtt")
            }
        }) {
            Some("") => {
                own = Some(Track {
                    file: name.clone(),
                    name: known
                        .iter()
                        .find(|language| language.code == own_language)
                        .map_or_else(
                            || own_language.to_string(),
                            |language| language.name.clone(),
                        ),
                    code: own_language.to_string(),
                });
                continue;
            }
            Some(code) if !code.is_empty() => code.to_string(),
            _ => continue,
        };
        translations.push(Track {
            name: known
                .iter()
                .find(|language| language.code == code)
                // §2's fill-in rule: a code nobody knows stands in as its own label rather than an empty menu entry.
                .map_or_else(|| code.clone(), |language| language.name.clone()),
            file: name.clone(),
            code,
        });
    }
    // Translations by name (what a player's menu shows); by code `final.de.vtt` would beat the video's own track and make German the `default`.
    translations.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.code.cmp(&b.code)));
    let mut found = Vec::with_capacity(translations.len() + 1);
    if let Some(own) = own {
        found.push(own);
    }
    found.extend(translations);
    found
}

/// The `.vtt` sidecars beside the video as base names, sorted for stable listings; NOT own-language first, that is [`tracks`]' decision. An unreadable folder reads as empty,
/// so a ▶ that skipped the encode still gets its tag.
pub fn list_vtt(dir: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().to_string();
            (name.ends_with(".vtt") && entry.path().is_file()).then_some(name)
        })
        .collect();
    names.sort();
    names
}

// ---- the poster -------------------------------------------------------------------

/// §F5.5's "JPEG 90" in the spec's units; ffmpeg gets it as [`POSTER_QSCALE`] ([`poster_command`]).
pub const POSTER_JPEG_QUALITY: u8 = 90;

/// `tool:ffmpeg.encode` — `-q:v` runs 2..31, lower is better, and 2 is the nearest step to the 90 §F5.5 asks of the poster; hence separate from [`POSTER_JPEG_QUALITY`].
pub const POSTER_QSCALE: &str = "2";

/// The poster: `<stem>.jpg` beside the video, made from `produce/publish/thumbnail.png`; a `.jpg` because 200 kB of PNG is not worth it for one frame of decoration.
pub fn poster_command(thumbnail: &Path, poster: &Path) -> Vec<String> {
    vec![
        "ffmpeg".into(),
        "-y".into(),
        "-i".into(),
        thumbnail.display().to_string(),
        // The ladder rung, not a percentage: see POSTER_QSCALE.
        "-q:v".into(),
        POSTER_QSCALE.into(),
        poster.display().to_string(),
    ]
}

/// Why the poster could not be written, in the one case the caller cannot work out for itself.
///
/// "Drawn" covers BOTH files the run can leave behind (`thumbnail.png` and `thumbnail-plain.png`); only reached when neither exists.
pub fn no_poster_reason() -> &'static str {
    "no thumbnail has been drawn yet"
}

// ---- what the log has to say ------------------------------------------------------

/// The tag is a bonus: F5.2's render verdict is the run's verdict, so the tag's failure is an input here and never the answer.
pub fn render_verdict(render_failed: bool, _tag_failed: bool) -> bool {
    render_failed
}

/// F5.5's first note: a container or codec no browser plays, worded as the prototype's (`go/produce_embed.go`) naming the browser to blame and the format to move to. mp4 + h264 and webm + vp9 say nothing.
pub fn web_unplayable(container: Container, codec: Codec) -> Option<&'static str> {
    match container {
        Container::Mkv => Some("no browser plays Matroska \u{2014} render to mp4 or webm for a page"),
        _ if codec == Codec::H265 => Some(
            "Firefox plays no h265 at all, and the others only where the machine decodes it in hardware \
             \u{2014} h264 is the one that plays everywhere",
        ),
        _ => None,
    }
}

/// F5.5's second note: the tag works when the folder is served and not as a `file://` page (a browser refuses `<track>` across origins). The page looks fine and shows no captions, so the log says it.
pub fn needs_http_log() -> &'static str {
    "    (subtitles need it served over http \u{2014} opened as a file:// page the browser refuses every <track>)"
}

/// The note is worth its line only when there is a track to refuse.
pub fn note_http(subtitles: &[Track]) -> Option<&'static str> {
    (!subtitles.is_empty()).then(needs_http_log)
}

/// The line that tells a person the four lines they want exist, and where.
pub fn wrote_tag_log(path: &str) -> String {
    format!(">>> the <video> tag for it: {path}")
}

/// A missing poster is not a failed page, so it is said at the quiet indent of the other notes, not with `!!!`.
pub fn no_poster_log(reason: &str) -> String {
    format!("    produce: no poster for the <video> tag ({reason})")
}

pub fn unwritable_log(path: &str) -> String {
    format!("    produce: could not write {path}")
}

// ---- where the three files live, and writing the one we own -----------------------

/// The video, its poster and its tag, beside each other so `src` and `poster=` need no path.
pub fn paths(tree: &Tree, container: Container) -> (PathBuf, PathBuf, PathBuf) {
    let stem = match container {
        Container::Mp4 => "mp4",
        Container::Mkv => "mkv",
        Container::Webm => "webm",
    };
    (tree.final_video(stem), tree.final_jpg(), tree.final_html())
}

/// Write `<stem>.html`. The folder is created because a first render has never had one.
pub fn write_tag(tree: &Tree, body: &str) -> Result<(), String> {
    let path = tree.final_html();
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|why| format!("{}: {why}", dir.display()))?;
    }
    fs::write(&path, body).map_err(|why| format!("{}: {why}", path.display()))
}
