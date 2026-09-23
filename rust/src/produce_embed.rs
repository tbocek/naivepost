//! F5.5 The `<video>` tag — `spec/08-produce.md` §F5.5, wording in `spec/inventory/produce.md:35`. A browser
//! plays none of the subtitle tracks muxed into an mp4 — in-band text is not something Firefox or Chrome
//! offer at all — and `<track>` parses WebVTT alone, so the `.srt` a desktop player reads is the wrong file
//! for a page. What a page needs is the `.vtt` files the render already wrote beside the video and a tag
//! pointing at them; getting those four lines right is five minutes of somebody's afternoon every time, so
//! they are written out with the video.
//!
//! Three rules about *when* this runs, because each one decides what the file may contain:
//! - **After both halves of ▶.** The poster is the thumbnail, and the thumbnail is drawn by the other half of
//!   the same press; a tag written at the end of the render would point at a `.jpg` that does not exist yet.
//! - **The tracks come off the disk, not out of the render.** A ▶ that found everything up to date skipped the
//!   encode and wrote no subtitles — but the subtitles from the render before are still beside the video, and
//!   they are this video's subtitles ([`crate::produce_stamp`]'s stamp is what made skipping legitimate).
//! - **Never a failed render.** The video and its subtitles are on disk either way; a tag that could not be
//!   written is said in the log and changes nothing else ([`render_verdict`]).
//!
//! webm carries no subtitle track at all ([`crate::produce_screen::apply_container_rules`] turns "track in
//! file" into "none in the video"), yet its `.vtt` files sit beside it and its tag lists them — a page reads
//! those, not the container.
//!
//! No GTK here, and nothing is spawned: like [`crate::produce_render`], this builds argv and text. The JPEG
//! poster is an ffmpeg job ([`poster_command`]) and the container has no ffmpeg to run it with.

use std::fs;
use std::path::{Path, PathBuf};

use crate::layout::Tree;
use crate::project::{Codec, Container};
use crate::roles::Language;

// ---- the tag ---------------------------------------------------------------------

/// One caption track: the language code for `srclang`, the name it is offered under in a player's menu, and
/// the **base name** of its file — the html lives beside the video, so the page needs no folder in front.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Track {
    pub code: String,
    pub name: String,
    pub file: String,
}

/// F5.5's tag, exactly as the spec writes it: a bare `<video poster controls src preload="none">` and one
/// `<track>` per language, each indented two spaces, the file ending `</video>\n`.
///
/// *Bare* is the word the spec uses, and its own example is the answer: a track carries `src`, `srclang` and
/// `default` and nothing else. The prototype also wrote `label="…" kind="subtitles"` and self-closed the tag
/// (recorded in `spec/inventory/produce.md:35`); those are left out here because a browser labels a track
/// from `srclang` on its own, and every extra attribute is one more thing for a person editing the pasted
/// four lines to notice and change.
pub fn tag(video: &str, poster: Option<&str>, tracks: &[Track]) -> String {
    let mut out = String::from("<video ");
    if let Some(poster) = poster {
        out.push_str(&format!("poster=\"{}\" ", escape(poster)));
    }
    out.push_str(&format!("controls src=\"{}\" preload=\"none\">\n", escape(video)));
    for (index, track) in tracks.iter().enumerate() {
        // Only the first is `default`: two defaults is two sets of captions drawn over each other in some
        // players, and nothing on the page says which won.
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

/// The three characters that would otherwise end an attribute or start a tag, plus the ampersand that starts
/// an entity. A spoken "R&D" in a filename is rare; a mangled page that says nothing about why is worse.
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
/// `names` is what the caller found on disk — this stays pure so the ordering and the coding are testable
/// without a folder. `<stem>.vtt` is the session's own language, whatever its code; `<stem>.de.vtt` names its
/// own between the two dots; anything not starting from this stem — another video's file, or an `.srt` a
/// `<track>` would fetch, fail to parse and show nothing of — stays out. The stem is
/// [`crate::produce_render::final_srt_name`] minus its extension: the name §A fixes for the video, which is
/// what makes `<stem>.html`, `<stem>.jpg` and these sidecars one family rather than four names kept in step by
/// hand.
///
/// An empty answer is legal — a video with no captions still gets a tag.
pub fn tracks(names: &[String], own_language: &str, known: &[Language]) -> Vec<Track> {
    // §A fixes the video's name, so its sidecars are `final.vtt` (the language the video speaks — the
    // absence of a code is the point) and `final.<code>.vtt` for each translation. Anything else in the folder
    // is another video's file or an `.srt`, which a <track> would fetch, fail to parse and show nothing of.
    let stem = crate::produce_render::final_srt_name()
        .strip_suffix(".srt")
        .unwrap_or_default()
        .to_string();
    let mut own: Option<Track> = None;
    let mut translations: Vec<Track> = Vec::new();
    for name in names {
        // A code is what sits between the stem and the extension; empty means this is the video's own track.
        // `.html` first: the video's own sidecar is `final.vtt`, which has no code between the dots, so
        // stripping the prefix alone would read its name as a language called "vtt".
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
                    .map_or_else(|| own_language.to_string(), |language| language.name.clone()),
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
                // §2's fill-in rule again: a code nobody knows names nothing, so the code stands in rather
                // than the menu offering an empty label.
                .map_or_else(|| code.clone(), |language| language.name.clone()),
            file: name.clone(),
            code,
        });
    }
    // Translations by name, because that is what a player's menu shows; alphabetically `final.de.vtt` would
    // otherwise beat the video's own track and offer German as the `default`.
    translations.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.code.cmp(&b.code)));
    let mut found = Vec::with_capacity(translations.len() + 1);
    if let Some(own) = own {
        found.push(own);
    }
    found.extend(translations);
    found
}

/// The `.vtt` sidecars beside the video, as base names — the raw material for [`tracks`], which is what
/// decides their order. Sorted so a listing from two runs reads the same; **not** with the video's own
/// language first, because that is `tracks`' decision and doing it twice lets the two drift apart. A folder
/// that cannot be read reads as empty — which is what lets a ▶ that skipped the encode still get its tag
/// rather than failing on a missing listing.
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

/// §F5.5's "JPEG 90" — the requirement, in the units the spec writes it. This app does not encode the JPEG
/// itself ([`poster_command`] hands the job to ffmpeg), so the two constants name one requirement at its two
/// levels: what is asked for here, and the step ffmpeg takes to get there.
pub const POSTER_JPEG_QUALITY: u8 = 90;

/// `tool:ffmpeg.encode` — ffmpeg's own quality ladder for `-q:v` runs 2..31 with lower meaning better, and 2
/// is its nearest step to the 90 §F5.5 asks of the poster. There is no 90 to pass: the encoder takes a ladder
/// rung, which is why this constant exists separately from [`POSTER_JPEG_QUALITY`].
pub const POSTER_QSCALE: &str = "2";

/// The poster: `<stem>.jpg` beside the video, made from `produce/publish/thumbnail.png`. A `.jpg` and not the
/// PNG it comes from because a poster is one frame of decoration on a page that already carries the video,
/// and 200 kB of PNG is not worth it.
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
pub fn no_poster_reason() -> &'static str {
    "no thumbnail has been drawn yet"
}

// ---- what the log has to say ------------------------------------------------------

/// The tag is a bonus: F5.2's render verdict is the run's verdict, and a page helper that could not be
/// written leaves a finished video and its subtitles on disk. So the tag's failure is an input here and
/// never the answer — which is why this is a function rather than a call site someone might get wrong.
pub fn render_verdict(render_failed: bool, _tag_failed: bool) -> bool {
    render_failed
}

/// F5.5's first note: a container or codec no browser plays. The wording is the prototype's
/// (`gui/produce_embed.go`), because it names the browser to blame and the format to move to rather than
/// saying "unsupported". mp4 + h264 and webm + vp9 are a page's own formats and say nothing.
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

/// F5.5's second note, and the one people need: the tag works when the folder is served and does nothing
/// when it is opened as a file — a browser refuses every `<track>` across origins, and `file://` counts as a
/// different origin for each file. The page looks fine and shows no captions, so the log has to say it.
pub fn needs_http_log() -> &'static str {
    "    (subtitles need it served over http \u{2014} opened as a file:// page the browser refuses every <track>)"
}

/// The note is worth its line only when there is a track to refuse: a tag with no captions has nothing for
/// the browser to withhold.
pub fn note_http(subtitles: &[Track]) -> Option<&'static str> {
    (!subtitles.is_empty()).then(needs_http_log)
}

/// The line that tells a person the four lines they want exist, and where.
pub fn wrote_tag_log(path: &str) -> String {
    format!(">>> the <video> tag for it: {path}")
}

/// A missing poster is not a failed page — the video plays without one — so this is said at the same quiet
/// indent as the other notes rather than with the `!!!` a real failure gets.
pub fn no_poster_log(reason: &str) -> String {
    format!("    produce: no poster for the <video> tag ({reason})")
}

pub fn unwritable_log(path: &str) -> String {
    format!("    produce: could not write {path}")
}

// ---- where the three files live, and writing the one we own -----------------------

/// The video, its poster and its tag — one place saying the three live beside each other, since the tag's
/// whole value is that its `src` and `poster=` need no path in front of them.
pub fn paths(tree: &Tree, container: Container) -> (PathBuf, PathBuf, PathBuf) {
    let stem = match container {
        Container::Mp4 => "mp4",
        Container::Mkv => "mkv",
        Container::Webm => "webm",
    };
    (tree.final_video(stem), tree.final_jpg(), tree.final_html())
}

/// Write `<stem>.html`. The folder is created because a first render has never had one — the tag is written
/// after both halves of ▶, but nothing about this module depends on that order.
pub fn write_tag(tree: &Tree, body: &str) -> Result<(), String> {
    let path = tree.final_html();
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|why| format!("{}: {why}", dir.display()))?;
    }
    fs::write(&path, body).map_err(|why| format!("{}: {why}", path.display()))
}
