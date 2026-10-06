//! §05-cut#F2.10 — the session's recordings, from what the PROJECT holds, not from the kept clips.
//!
//! The page lays its rows from `page_recordings()`, and until now that list was derived from the cut:
//! `kept_footage_recordings(&opened_cut)` turns kept segments into `cam0`/`cam1` spans, so a project with
//! two cameras' worth of files drew ONE row (every kept clip says `cam<N>` by its own number, and the union
//! of kept clips is never longer than itself). F2.10's whole picture — camera 1 and camera 2 overlapping
//! 0:20–0:37, each on its own row, the microphone on a third — therefore only existed because a snapshot
//! staged it. This module answers the question the app must answer on its own: WHICH spans did this session
//! actually film?
//!
//! Three sources of truth, in the order §B and §F1 put them:
//! * **the stamp in the file's own name** places it on the session clock (`clock::clock`, whose zero is the
//!   earliest stamp in the WHOLE session — that is why the list is built from every footage source at once);
//! * **a hand correction** (`cut.shift`) slides it afterwards, which is how an unstamped take gets lined up
//!   by ear and how an overlap made by a drag appears at all;
//! * **the file's length**, which no project file stores (§B says "built from the source's metadata or
//!   probed"), so ffprobe answers it — through the program's one probe runner, cached per path because a
//!   repaint must not re-run a subprocess.
//!
//! Cut lanes join the same list: `⇲ Lane` gives a copy "a row of its own", and a row that is not in this
//! list cannot be watched (S3), heard (S2) or lens-picked (S1). A source ffprobe cannot answer is skipped
//! and said by name — failure specific and local, not a silent missing row.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::clock;
use crate::cut::Cut;
use crate::project::Project;
use crate::settings;
use crate::sources;
use crate::timeline::Recording;
use crate::ui::save_apply;

// Probed lengths, keyed by the resolved path. One entry per file for the life of the process: a probe is a
// subprocess, and the page repaints far more often than a session's files change.
thread_local! {
    // `HashMap::new` is not a const fn, so the cell starts empty the plain way.
    static LENGTHS: RefCell<HashMap<String, f64>> = RefCell::new(HashMap::new());
}

/// The session's recordings, in the order the project lists its footage sources, then the cut's lanes.
///
/// `root` and `project` resolve a stored path (`project:` + root-relative, absolute nowhere else); pass
/// [`open_roots`] to have this read where the window left the open folder.
pub fn session_recordings(project: &Project, cut_: &Cut, root: &Path, project_dir: &Path) -> Vec<Recording> {
    let names: Vec<String> = project
        .sources
        .iter()
        .filter(|source| source.footage)
        .map(base)
        .collect();
    // One clock over the WHOLE session: zero is the earliest stamp in the list, so a shorter list would move
    // zero and slide every other file to a different second (`clock::clock`'s own rule).
    let clock = clock::clock(&names.iter().map(String::as_str).collect::<Vec<_>>());
    let configured = configured_ffmpeg();
    let mut recordings: Vec<Recording> = Vec::new();
    for (index, source) in project
        .sources
        .iter()
        .enumerate()
        .filter(|(_, source)| source.footage)
    {
        let name = base(source);
        let Some(length) = length_of(&configured, &crate::layout::resolve(root, project_dir, &source.path), &name)
        else {
            // Said by name and reason rather than dropped quietly: a missing row reads as a bug.
            crate::ui::window::log_line(&format!("{name}: no row \u{2014} ffprobe gave no length"));
            continue;
        };
        let shift = cut_.shift.get(&name).copied().unwrap_or(0.0);
        let start = clock.offset(index) + shift;
        recordings.push(Recording {
            base: name.clone(),
            start,
            end: start + length,
        });
    }
    recordings.extend(lane_recordings(cut_));
    recordings
}

/// The cut's own lanes as recordings: a lane IS a span of a file laid on the session clock (`at`, `dur`),
/// which is exactly what a row needs. Its `off` stays out of here — that is which second of the FILE the row
/// windows from, and the row's place on the clock is what colouring and watching ask about.
pub fn lane_recordings(cut_: &Cut) -> Vec<Recording> {
    cut_.lanes
        .iter()
        .filter(|lane| lane.dur > 0.0)
        .map(|lane| Recording {
            base: lane.name.clone(),
            start: lane.at,
            end: lane.at + lane.dur,
        })
        .collect()
}

/// Where to resolve a stored path from: the application root (the process cwd, which is what every other
/// door here uses) and the folder this window opened. `None` when nothing is open, which callers treat as
/// "no session yet" rather than guessing a folder.
pub fn open_roots() -> Option<(PathBuf, PathBuf)> {
    let project_dir = save_apply::open_dir()?;
    Some((std::env::current_dir().unwrap_or_default(), project_dir))
}

/// The session's recordings for the page, or `None` when there is no open folder to resolve paths against.
/// This is the door `window.rs` calls: one line at the seam, all the rules here.
pub fn for_open_session(cut_: &Cut) -> Option<Vec<Recording>> {
    let (root, project_dir) = open_roots()?;
    let project = crate::project::load(&project_dir).ok()?;
    Some(session_recordings(&project, cut_, &root, &project_dir))
}

/// The door an open or a Rescan calls: this window's session spans, asked of the OPEN PROJECT through
/// [`for_open_session`] and written into the page's own session slot, so every reader of
/// `page_recordings()` (the row list, folds, lanes, the painter) sees the same spans.
///
/// Nothing is overwritten when there is no folder to resolve against or nothing in it answered ffprobe:
/// the page keeps the spans it was seeded with rather than collapsing to no rows at all.
pub fn refresh_page_spans(cut_: &Cut) -> Option<Vec<Recording>> {
    let spans = for_open_session(cut_)?;
    if spans.is_empty() {
        return None;
    }
    crate::ui::cut_folds::set_session_recordings(&spans);
    Some(spans)
}

/// What ffprobe says this file runs for, remembered per path. `None` carries the refusal upward as a skip.
fn length_of(configured: &str, path: &Path, name: &str) -> Option<f64> {
    let key = path.display().to_string();
    if let Some(known) = LENGTHS.with(|cell| cell.borrow().get(&key).copied()) {
        return Some(known);
    }
    let answered = crate::cut_lanes::probe_duration(configured, &key, &mut |line| {
        crate::ui::window::log_line(line)
    })
    .ok()
    .filter(|seconds| *seconds > 0.0);
    if let Some(seconds) = answered {
        LENGTHS.with(|cell| {
            cell.borrow_mut().insert(key, seconds);
        });
    }
    let _ = name;
    answered
}

/// The configured ffmpeg decides where ffprobe lives; `""` means no settings home, which the probe reports
/// as its own reason (`cut_lane::probe_file_seconds` follows the same rule).
fn configured_ffmpeg() -> String {
    settings::from_environment()
        .map(|paths| settings::read(&paths).unwrap_or_default().ffmpeg)
        .unwrap_or_default()
}

/// What a `Scope::Sound` / a name plate / the clock all call a source: its file name, without the folder.
fn base(source: &crate::project::Source) -> String {
    sources::row_name(&source.path)
}

/// The page's session spans: what this session actually filmed, asked of the OPEN PROJECT
/// ([`crate::ui::cut_session`] — stamped names + hand shifts + probed lengths), UNIONED with what the cut
/// implies. A second camera that filmed the same seconds only gets its own row if it is in this list at all,
/// and a lane's file likewise; a span for a name already present wins from the SESSION side, because that is
/// the one that knows where the tape really sits on the clock.
///
/// What the cut contributes is ONE span per camera it shows, spanning that camera's earliest and latest kept
/// second — not one per scene. `timeline::rows_for` colours every entry it is handed, so three clips of one
/// camera would otherwise read as three recordings and invent two rows; the merged span still overlaps another
/// camera's, which is what puts an overlap on a row of its own.
pub fn session_or_kept(cut_: &Cut) -> Vec<Recording> {
    let mut spans = for_open_session(cut_).unwrap_or_default();
    let mut named: Vec<String> = spans.iter().map(|span| span.base.clone()).collect();
    for span in kept_cam_spans(cut_) {
        if named.contains(&span.base) {
            continue;
        }
        named.push(span.base.clone());
        spans.push(span);
    }
    // A lane is a window of a file on a row of its own (§B): same standing as a take for what rows exist.
    for span in lane_recordings(cut_) {
        if named.contains(&span.base) {
            continue;
        }
        named.push(span.base.clone());
        spans.push(span);
    }
    spans.sort_by(|a, b| a.start.partial_cmp(&b.start).unwrap_or(std::cmp::Ordering::Equal));
    spans
}

/// One span per camera a cut's kept scenes read their pictures from: `[earliest s, latest e]` of that camera.
fn kept_cam_spans(cut_: &Cut) -> Vec<Recording> {
    let mut cams: Vec<i32> = Vec::new();
    let mut spans: Vec<Recording> = Vec::new();
    for seg in cut_.segs.iter().filter(|seg| seg.ins.is_empty() && seg.e > seg.s) {
        match cams.iter().position(|cam| *cam == seg.cam) {
            Some(known) => spans[known].end = spans[known].end.max(seg.e),
            None => {
                cams.push(seg.cam);
                spans.push(Recording {
                    base: format!("cam{}", seg.cam),
                    start: seg.s,
                    end: seg.e,
                });
            }
        }
    }
    spans
}
