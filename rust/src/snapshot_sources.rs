//! The `03-sources` screen's own staging: the source list §4 pictures.
//!
//! `spec/img/03-sources.png` is a lecture day — eight stamped recordings, one narrator slot 1, a second
//! voice, and the frame controls under the list. The shipped fixture cannot show any of that: it holds ONE
//! source (`project:sources/lecture.mkv`, a file the fixture does not even carry), so an unseeded shot was
//! an empty scroller and nothing in §4 could be compared with its picture. This stages rows into the window's
//! live session and repaints through the page's own painter ([`crate::ui::source_rows_refresh::redraw`], which
//! calls the same [`crate::ui::source_rows::repaint`] a 🗑 press does) — so every control on the shot is drawn
//! by the code the app uses, never by a snapshot-only copy of it.
//!
//! Two things are staged as REAL files rather than as names, because §4's row controls come from the file and
//! not from the project: the ⚠ needs a name with no timestamp (nothing to write), and the track button needs a
//! file ffprobe really counts TWO audio streams in — a faked count would draw a badge the live app can only
//! ever answer with after measuring. The two-stream clip is written with ffmpeg next to the other generated
//! media the snapshot writes (see `write_silence_wav`'s precedent in `snapshot.rs`).

use std::path::{Path, PathBuf};
use std::process::Command;

use adw::prelude::*;
use gtk4 as gtk;

use crate::project::{Project, Source};
use crate::ui;

/// Width this screen asks for: wide enough that a stamped name, the track face and the four symbol controls
/// all read on one line once the column gets its share (§1's minimum is only a floor, not a ceiling).
const WIDE_ENOUGH_W: i32 = 1240;

/// One row's height in the crop ask (§4's row is one line; 48 px is what the page settles them at).
const ROW_PX: i32 = 48;

/// How many rows §1's minimum height already shows in the sources scroller's band (measured off the shot:
/// at 900 px the band fits five). Each staged row beyond this is what the ask has to add.
const ROWS_AT_MIN_H: i32 = 5;

/// The sources scroller's own floor (`window_page_box::SOURCES_MIN_H`, private there). At §1's minimum the
/// page gives the list more than this, so a taller window only helps if the scroller is asked for the extra
/// too -- which is what [`widen_for_rows`] does through the page's own repaint.
pub const SHOT_BAND_ROWS: i32 = 5;

/// One lecture day's recordings, in the order §4's picture lists them (newest first). Names carry the
/// prototype's own stamp shape, `YYYY-MM-DD HH-MM-SS`, which is what `clock::name_stamp` reads; the last
/// two are deliberately off-pattern so the ⚠ badge and the audio row (no 🎥 at all) appear in the shot.
const DAY: &str = "2026-09-16";

/// The stamped camera recordings: slot 1 on the newest (the narration's voice), slot 2 on one later take,
/// and nobody on the rest — which is the state §4's auto-fill rule starts from. `tracks` holds ffprobe's
/// own 0-based stream numbers (§4's "<on>/<total>" face prints `<on>` as the count of these); the popover
/// converts them to its 1-based rows, so a row that ticks nothing would show `0/2`.
fn camera_rows() -> Vec<Source> {
    [
        // hour-minute-second stamps, as the prototype named its takes.
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
    })
    .collect()
}

/// The two-stream recording: §4's track menu exists only when the FILE holds ≥ 2 audio streams, so this is
/// a real file written now, probed by the real ffprobe, and pointed at by its own row. Named WITHOUT a
/// timestamp on purpose: it then carries §4's ⚠ as well, so one row shows both of the file-derived badges.
pub const TWO_STREAM_NAME: &str = "mix-cam-mic.mkv";

/// Where a staged file goes: the folder `layout::resolve` answers for a `project:` path, which is where the
/// row's own probe looks (the same pair `source_rows::resolved` uses).
fn stage_dir() -> PathBuf {
    let root = std::env::current_dir().unwrap_or_default();
    let project = crate::startup::session_dir(&root);
    crate::layout::resolve(&root, &project, "project:sources/stage.mkv")
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or(root)
}

/// Write the two-audio-stream clip with ffmpeg: two sine sources, each mapped as its own stream, one second
/// long. Silent-ish tones keep the file tiny; what matters to §4 is that ffprobe counts two audio streams.
/// Returns whether the file is there afterwards — a machine without ffmpeg loses the row rather than showing
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

/// The staged list: the camera day, the two-stream mix (if it could be written — it also carries the ⚠ for
/// an unstamped name), and a separate mic (audio, so no 🎥 anywhere on its row).
pub fn staged() -> Project {
    let mut sources = camera_rows();
    if write_two_stream(&stage_dir().join(TWO_STREAM_NAME)) {
        sources.push(Source {
            path: format!("project:sources/{TWO_STREAM_NAME}"),
            footage: true,
            narrator: 0,
            sepvoice: false,
            // Both streams ticked: §4's face reads "<on>/<total>", and one stream in of two is a row that
            // hears half its own audio — not what a freshly added file looks like.
            tracks: vec![0, 1],
        });
    }
    // The fixture's real separate recording: an audio row, which earns no footage toggle at all.
    sources.push(Source {
        path: "project:sources/mic-lav.wav".to_string(),
        footage: false,
        narrator: 0,
        sepvoice: true,
        tracks: vec![0],
    });
    Project {
        sources,
        interval: 1.0,
        language: "en".to_string(),
        ..Default::default()
    }
}

/// Put §4's picture on this window: swap the live session's sources for the staged set and repaint the list
/// through the page's own painter. Returns how many rows the page drew, counted off the widget tree.
pub fn stage(window: &adw::ApplicationWindow) -> usize {
    let staged = staged();
    let session = ui::session_handle(window);
    *session.borrow_mut() = staged;
    if !ui::source_rows_refresh::redraw(window) {
        println!("snapshot: 03-sources found no sources-list to redraw");
    }
    ui::source_rows_refresh::drawn_rows(window)
}

/// Ask for a window big enough to show every staged row, so the ordinary page shot has them all above the
/// fold (the recipe's own file name is kept: this changes the size the shot is taken at, nothing else).
///
/// `spec/img/03-sources.png` is a crop of one column at a height that fits eight rows; in a 1240×900 window
/// the scroller shows only the rows its band happens to fit and the rest of the staged day sits below it.
/// §1 lets a page grow with the window (only the width has a floor), so asking is the legal way — and
/// reparenting a live child into a throwaway top level is not, which is what an earlier attempt printed as
/// "window produced no render node". Returns whether there was a list to stage at.
pub fn widen_for_rows(window: &adw::ApplicationWindow) -> bool {
    // Rows first: the ask has to cover them all, and the count is read off the widget tree rather than from
    // the project this module just staged, so a row the painter skipped shrinks the ask instead of hiding.
    let rows = i32::try_from(ui::source_rows_refresh::drawn_rows(window)).unwrap_or(ROWS_AT_MIN_H);
    // §1 fixes 1240×900 as the window's FLOOR and several screens already ask wider for their subject (the
    // `wide` pass-screen ask in `snapshot.rs`). What hides a row here is the sources scroller's own band, so
    // the only honest way to show the whole staged day is a taller window: ask for what §1's minimum leaves
    // the list, plus one row-height per row beyond the ones that fit at the floor. `set_default_size` is the
    // OPENING size; `set_size_request` would be a minimum, which §1 forbids above 1240×900.
    let extra = ROW_PX * (rows - ROWS_AT_MIN_H).max(0);
    let wanted = crate::shell::WINDOW_MIN_H + extra;
    // The page/bottom-bar divider is what decides how tall the page band is, and headless nothing drags it
    // (§1: "Above the minimum the layout grows with the window"). Give the page the whole ask so the sources
    // scroller gets the rows instead of scrolling them away.
    if let Some(divider) =
        ui::find_source_widget(window, "main-divider").and_then(|w| w.downcast::<gtk::Paned>().ok())
    {
        divider.set_position(wanted);
    }
    // `set_default_size` alone never reaches the pixels: `snapshot.rs` clamps to
    // `default_height().max(WINDOW_MIN_H)` just before the shot, so the same floor has to be lifted here too.
    if window.default_height() < wanted {
        window.set_default_size(window.default_width().max(WIDE_ENOUGH_W), wanted);
        window.set_size_request(crate::shell::WINDOW_MIN_W, wanted);
    }
    true
}

/// Paint §4's own subject — the LEFT column: the list of rows and the frame controls under it — into `out`.
/// `spec/img/03-sources.png` is that crop, and a whole-window shot cannot be it: at §1's minimum the column is
/// half the page beside the prompt bench, so its rows read small and the right half is not the subject at all.
/// The page goes beside it as `<name>-page.png` (see `snapshot_subjects::subject_takes_the_name`).
pub fn write_column_png(window: &adw::ApplicationWindow, out: &Path) -> Result<(), String> {
    let column = ui::find_source_widget(window, "prepare-left")
        .ok_or("the Prepare page drew no prepare-left")?;
    // The live column is painted in place — no reparenting (the page box hands `prepare-left` back as a
    // `gtk::Widget`, so it cannot be taken out as a `gtk::Box`), and the page sidecar below must still find it
    // where the page drew it. Only the COLOURS differ: the CSS provider below selects on `.prepare-left`, so
    // this crop comes out light like the reference while the window shot, taken before it, keeps the dark theme
    // the app really draws in.
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
