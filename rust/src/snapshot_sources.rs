//! The `03-sources` screen's own staging: the source list §4 pictures (`spec/img/03-sources.png`, a lecture
//! day of eight stamped recordings, one narrator slot 1, a second voice, and the frame controls).
//!
//! The shipped fixture holds ONE source, so rows are staged into the window's live session and repainted
//! through the page's own painter ([`crate::ui::source_rows_refresh::redraw`]); no control is drawn by a
//! snapshot-only copy. Two things are REAL files because §4's row controls come from the file: the ⚠ needs a
//! name with no timestamp, and the track button needs a file ffprobe counts TWO audio streams in (written
//! with ffmpeg, as `write_silence_wav` in `snapshot.rs`).

use std::path::{Path, PathBuf};
use std::process::Command;

use adw::prelude::*;
use gtk4 as gtk;

use crate::project::{Project, Source};
use crate::ui;

/// The sources scroller's own floor (`window_page_box::SOURCES_MIN_H`, private there); rows beyond it scroll.
pub const SHOT_BAND_ROWS: i32 = 5;

/// One lecture day's recordings, newest first, with the prototype's stamp shape `YYYY-MM-DD HH-MM-SS`
/// (`clock::name_stamp`); the last two are off-pattern so the ⚠ badge and the audio row appear in the shot.
const DAY: &str = "2026-09-16";

/// The stamped camera recordings: slot 1 on the newest (the narration's voice), slot 2 on one later take,
/// nobody on the rest (§4's auto-fill start). `tracks` holds ffprobe's 0-based stream numbers; the popover
/// converts them to 1-based rows, so a row ticking nothing would show `0/2`.
fn camera_rows() -> Vec<Source> {
    [
        ("18-43-01", true, 1),
        ("18-39-36", false, 0),
        ("18-38-33", false, 0),
        ("18-37-22", false, 2),
        ("18-33-56", false, 0),
    ]
    .into_iter()
    .map(|(stamp, footage, narrator)| Source {
        path: format!("project:sources/{DAY} {stamp}.mkv"),
        footage,
        narrator,
        sepvoice: false,
        tracks: vec![0],
        sound_off: false,
    })
    .collect()
}

/// The two-stream recording: §4's track menu exists only when the FILE holds ≥ 2 audio streams, so this is
/// real and probed by the real ffprobe. Named WITHOUT a timestamp so the row also carries the ⚠.
pub const TWO_STREAM_NAME: &str = "mix-cam-mic.mkv";

/// Where a staged file goes: the folder `layout::resolve` answers for a `project:` path, where the row's probe looks.
fn stage_dir() -> PathBuf {
    let root = std::env::current_dir().unwrap_or_default();
    let project = crate::startup::session_dir(&root);
    crate::layout::resolve(&root, &project, "project:sources/stage.mkv")
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or(root)
}

/// Write the two-audio-stream clip with ffmpeg: two sine sources, each its own stream, one second long.
/// Returns whether the file is there afterwards; a machine without ffmpeg loses the row rather than showing
/// a track badge nothing measured.
pub fn write_two_stream(path: &Path) -> bool {
    let started = Command::new("ffmpeg")
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:duration=1",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=880:duration=1",
            "-map",
            "0:a",
            "-map",
            "1:a",
            "-c:a",
            "pcm_s16le",
        ])
        .arg(path)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
    matches!(started, Ok(status) if status.success()) && path.is_file()
}

/// The staged list: the camera day, the two-stream mix (if it could be written), and a separate mic (audio, no 🎥).
pub fn staged() -> Project {
    let mut sources = camera_rows();
    if write_two_stream(&stage_dir().join(TWO_STREAM_NAME)) {
        sources.push(Source {
            path: format!("project:sources/{TWO_STREAM_NAME}"),
            footage: true,
            narrator: 1,
            sepvoice: false,
            // Both streams ticked: §4's face reads "<on>/<total>"; one of two would hear half its own audio.
            tracks: vec![0, 1],
            sound_off: false,
        });
    }
    // The fixture's real separate recording: an audio row, which earns no footage toggle.
    sources.push(Source {
        path: "project:sources/mic-lav.wav".to_string(),
        footage: false,
        narrator: 1,
        sepvoice: true,
        tracks: vec![0],
        sound_off: false,
    });
    Project {
        sources,
        ..Default::default()
    }
}

/// Put §4's picture on this window: swap the live session's sources for the staged set and repaint through
/// the page's own painter. Returns how many rows the page drew, counted off the widget tree.
pub fn stage(window: &adw::ApplicationWindow) -> usize {
    let staged = staged();
    let session = ui::session_handle();
    *session.borrow_mut() = staged;
    if !ui::source_rows_refresh::redraw(window) {
        println!("snapshot: 03-sources found no sources-list to redraw");
    }
    ui::source_rows_refresh::drawn_rows(window)
}

/// Put §4's staged day on the window AND leave the shell at §1's minimum height (every screenshot is the
/// whole window at its minimum). In order: lay the rows out, bring the shell back to the minimum, then ask
/// the divider for its resting split at that size. Returns the number of rows drawn.
///
/// The window is never grown for the rows: a `gtk::Window` under xvfb keeps whatever height its content
/// asked for, so lowering it afterwards never restored §1's 900 px (measured 986). Rows below the fold scroll.
pub fn stage_for_shot(window: &adw::ApplicationWindow) -> usize {
    // Rows first, through the page's own painter: what it cannot draw is reported, not hidden.
    let rows = stage(window);
    crate::snapshot_face::pump_context(8);
    // The shell sits at §1's minimum before anything measures it, so the band's ask is taken at the shot's size.
    window.set_size_request(crate::shell::WINDOW_MIN_W, crate::shell::WINDOW_MIN_H);
    window.set_default_size(crate::shell::WINDOW_MIN_W, crate::shell::WINDOW_MIN_H);
    crate::snapshot_face::pump_context(8);
    if let Some(divider) =
        ui::find_source_widget(window, "main-divider").and_then(|w| w.downcast::<gtk::Paned>().ok())
    {
        crate::ui::window_divider::rest_split(&divider);
    }
    crate::snapshot_face::pump_context(8);
    rows
}

/// Paint §4's own subject, the LEFT column (rows and frame controls), into `out`: `spec/img/03-sources.png`
/// is that crop, and at §1's minimum a whole-window shot reads small. The page goes beside it as
/// `<name>-page.png` (`snapshot_subjects::subject_takes_the_name`).
pub fn write_column_png(window: &adw::ApplicationWindow, out: &Path) -> Result<(), String> {
    let column = ui::find_source_widget(window, "prepare-left")
        .ok_or("the Prepare page drew no prepare-left")?;
    // Painted in place, no reparenting (`prepare-left` comes back as a `gtk::Widget`, and the page sidecar must
    // still find it). Only the COLOURS differ: the `.prepare-left` CSS selects light for the crop while the
    // window shot, taken before it, keeps the dark theme.
    column.add_css_class("prepare-left-shot");
    let theme = gtk::CssProvider::new();
    theme.load_from_string(
        "window { background-color: #fafafa; }\n\
         .prepare-left, .prepare-left-shot, .prepare-left-shot label\n\
         { background-color: #fafafa; color: #1a1a1a; }\n\
         .prepare-left-shot button { color: #1a1a1a; }\n",
    );
    if let Some(display) = gtk::gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &theme,
            gtk::STYLE_PROVIDER_PRIORITY_USER,
        );
    }
    crate::snapshot_face::pump_context(32);
    let result = crate::snapshot_face::write_png(&column, out);
    column.remove_css_class("prepare-left-shot");
    result
}
