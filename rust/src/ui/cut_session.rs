//! §05-cut#F2.10 — the session's recordings, from what the PROJECT holds, not from the kept clips.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::clock;
use crate::cut::Cut;
use crate::layout;
use crate::project::Project;
use crate::settings;
use crate::timeline::Recording;

thread_local! {
    static LENGTHS: RefCell<HashMap<String, f64>> = RefCell::new(HashMap::new());
}

/// The session's recordings, in the order the project lists its footage sources, then the cut's lanes.
pub fn session_recordings(
    project: &Project,
    cut_: &Cut,
    root: &Path,
    project_dir: &Path,
) -> Vec<Recording> {
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
        let Some(length) = length_of(
            &configured,
            &crate::layout::resolve(root, project_dir, &source.path),
        ) else {
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
/// which is exactly what a row needs.
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
/// door here uses) and the folder this window opened.
pub fn open_roots() -> Option<(PathBuf, PathBuf)> {
    let project_dir = crate::startup::open_dir_if_set()?;
    Some((std::env::current_dir().unwrap_or_default(), project_dir))
}

/// The session's recordings for the page, or `None` when there is no open folder to resolve paths against.
pub fn for_open_session(cut_: &Cut) -> Option<Vec<Recording>> {
    let (root, project_dir) = open_roots()?;
    let project = crate::project::load(&project_dir).ok()?;
    Some(session_recordings(&project, cut_, &root, &project_dir))
}

fn open_tree() -> Option<layout::Tree> {
    layout::Tree::new(&crate::startup::open_dir_if_set()?).ok()
}

/// The door an open or a Rescan calls: this window's session spans, asked of the OPEN PROJECT through
/// [`for_open_session`] and written into the page's own session slot, so every reader of
/// `page_recordings()` (the row list, folds, lanes, the painter) sees the same spans.
pub fn refresh_page_spans(cut_: &Cut) -> Option<Vec<Recording>> {
    // §7's `the page is rebuilt only when Prepare's output changed`: the spans come from what Prepare wrote
    // (`transcript/session.tsv` places every take on the session clock), so when that file has not moved since
    // the page last drew, this whole recompute — reload the project, ffprobe each footage file, rebuild the row
    // list and the badges — has a known answer and is skipped.
    if let Some(tree) = open_tree() {
        if !crate::ui::cut_rebuild::output_changed(&tree) {
            return Some(crate::ui::cut_folds::page_recordings());
        }
    }
    let spans = for_open_session(cut_)?;
    if spans.is_empty() {
        return None;
    }
    crate::ui::cut_folds::set_session_recordings(&spans);
    Some(spans)
}

/// What ffprobe says this file runs for, remembered per path.
fn length_of(configured: &str, path: &Path) -> Option<f64> {
    let key = path.display().to_string();
    if let Some(known) = LENGTHS.with(|cell| cell.borrow().get(&key).copied()) {
        return Some(known);
    }
    if UNREADABLE.with(|cell| cell.borrow().contains(&key)) {
        return None;
    }
    ask_length(configured.to_string(), key);
    None
}

thread_local! {
    static UNREADABLE: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    static MEASURING: RefCell<(Vec<String>, Option<crate::ui::settle::Owed>)> =
        const { RefCell::new((Vec::new(), None)) };
}

fn ask_length(configured: String, key: String) {
    let first = MEASURING.with(|cell| {
        let mut held = cell.borrow_mut();
        if held.0.contains(&key) {
            return None;
        }
        held.0.push(key.clone());
        Some(held.0.len() == 1)
    });
    let Some(first) = first else {
        return;
    };
    if first {
        if let Some(window) = crate::ui::window::built_window() {
            let owed = crate::ui::settle::owe("footage lengths", &window);
            MEASURING.with(|cell| cell.borrow_mut().1 = Some(owed));
        }
    }
    let probed = key.clone();
    super::background::run(
        "length",
        move || {
            crate::cut_lanes::probe_duration(&configured, &probed, &mut |line| {
                crate::ui::window::log_line(line)
            })
            .ok()
            .filter(|seconds| *seconds > 0.0)
        },
        move |answered: Option<f64>| {
            match answered {
                Some(seconds) => LENGTHS.with(|cell| {
                    cell.borrow_mut().insert(key.clone(), seconds);
                }),
                None => {
                    // Said by name and reason rather than dropped quietly: a missing row reads as a bug.
                    let name = std::path::Path::new(&key)
                        .file_stem()
                        .map(|stem| stem.to_string_lossy().into_owned())
                        .unwrap_or_else(|| key.clone());
                    crate::ui::window::log_line(&format!(
                        "{name}: no row \u{2014} ffprobe gave no length"
                    ));
                    UNREADABLE.with(|cell| cell.borrow_mut().push(key.clone()))
                }
            }
            let done = MEASURING.with(|cell| {
                let mut held = cell.borrow_mut();
                held.0.retain(|known| *known != key);
                if held.0.is_empty() {
                    held.1.take()
                } else {
                    None
                }
            });
            if MEASURING.with(|cell| cell.borrow().0.is_empty()) {
                repaint_measured();
                drop(done);
            }
        },
    );
}

fn repaint_measured() {
    let cut_ = crate::ui::window::newest_review_cut();
    let spans = for_open_session(&cut_).filter(|spans| !spans.is_empty());
    let Some(window) = crate::ui::window::built_window() else {
        if let Some(spans) = spans {
            crate::ui::cut_folds::set_session_recordings(&spans);
        }
        return;
    };
    if let Some(spans) = spans {
        crate::ui::cut_view::keep_line_through_layout(&window, || {
            crate::ui::cut_folds::set_session_recordings(&spans);
        });
    }
    crate::ui::refresh_camera_rows(&window);
    crate::ui::cut_folds::refresh_fold_badges(&window);
    gtk4::prelude::WidgetExt::queue_draw(&window);
}

fn configured_ffmpeg() -> String {
    settings::from_environment()
        .map(|paths| settings::read(&paths).unwrap_or_default().ffmpeg)
        .unwrap_or_default()
}

/// What a `Scope::Sound` / a name plate / the clock all call a source: its file name, without the folder.
fn base(source: &crate::project::Source) -> String {
    crate::audio_task::source_lane(&source.path)
}

/// The page's session spans: what this session actually filmed, asked of the OPEN PROJECT
/// ([`crate::ui::cut_session`] — stamped names + hand shifts + probed lengths), UNIONED with what the cut
/// implies.
pub fn session_or_kept(cut_: &Cut) -> Vec<Recording> {
    let mut spans = for_open_session(cut_).unwrap_or_default();
    let mut named: Vec<String> = spans.iter().map(|span| span.base.clone()).collect();
    // The cut's camera spans (`cam<N>`) stand in for recordings only when the session named no footage FILE (its
    // list also carries the cut's lanes, which are not files of the session): their names never match a file, so
    // added beside the real recordings they drew one phantom row spanning the cut.
    let lanes = lane_recordings(cut_);
    let files_named = spans
        .iter()
        .any(|span| !lanes.iter().any(|lane| lane.base == span.base));
    let implied = if !files_named {
        kept_cam_spans(cut_)
    } else {
        Vec::new()
    };
    for span in implied {
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
    spans.sort_by(|a, b| {
        a.start
            .partial_cmp(&b.start)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    spans
}

fn kept_cam_spans(cut_: &Cut) -> Vec<Recording> {
    let mut cams: Vec<i32> = Vec::new();
    let mut spans: Vec<Recording> = Vec::new();
    for seg in cut_
        .segs
        .iter()
        .filter(|seg| seg.ins.is_empty() && seg.e > seg.s)
    {
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
