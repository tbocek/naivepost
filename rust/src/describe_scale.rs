//! Describe's S1 media leg: the scaled copies the vision request sends, made by ffmpeg for real.
//!
//! [`crate::describe`] holds the F1.7 rules and spawns nothing; this runs them, as [`crate::frames_run`] does
//! for `frames::extract_plan`: one ffmpeg per frame needing a copy, written into `.llmframes/` and checked
//! for the file before the pass claims the frame exists. Stored frames stay at the video's size; only the
//! copies sent are 896 wide (a 71-minute take: ~0.9 GB sent instead of 2.7 GB, §F1.1 S1).

use std::path::Path;

use crate::layout::Tree;
use crate::settings;
use crate::{clock, describe, frames, separate_media, subprocess};

/// Which of these frames have no scaled copy yet, the pure half of [`ensure`]. Resume is the copy's own
/// existence ("scaled ONCE", §F1.7 S1), so a half-written run costs only the frames it had not reached.
pub fn missing(dir: &Path, base: f64, frames: &[f64]) -> Vec<f64> {
    frames
        .iter()
        .copied()
        .filter(|at| !dir.join(describe::scaled_name(base, *at)).exists())
        .collect()
}

/// The instant the stored folder's first picture names: what a picked second (counted from that picture,
/// `describe::frames_on_disk`) is added to for a scaled copy's name. 0 for a folder with nothing readable.
pub fn name_base(tree: &Tree, lane: &str) -> f64 {
    earliest_name(&tree.frames_dir(lane))
        .and_then(|name| {
            clock::read_frame(name.to_string_lossy().strip_suffix(".jpg").unwrap_or(""))
        })
        .unwrap_or(0.0)
}

/// Scale every frame that still needs it into `.llmframes/` and say how many were made.
///
/// `start` is this source's first second on the session clock (as F1.6 gave `frames::extract_plan`): stored
/// files are named on the FILE's clock (`frame_name(start + at)`), while `describe` works in seconds from the
/// start of the take. The offset finds the input file and is NOT used to name the copy: `read_images` looks it
/// up as `describe::scaled_name(at)` at the rebased second, and both halves must agree.
///
/// A command that exits 0 having written nothing is an error naming the file (as `frames_run::extract_chunk`),
/// or it would surface three chunks later as "no scaled frame to describe".
pub fn ensure(
    tree: &Tree,
    conf: &settings::Conf,
    lane: &str,
    start: f64,
    frames: &[f64],
    log: &mut impl FnMut(&str),
) -> Result<usize, String> {
    let out_dir = tree.llm_frames_dir(lane);
    let base = name_base(tree, lane);
    let todo = missing(&out_dir, base, frames);
    if todo.is_empty() {
        return Ok(0);
    }
    std::fs::create_dir_all(&out_dir).map_err(|why| format!("{}: {why}", out_dir.display()))?;
    let ffmpeg = separate_media::program(&conf.ffmpeg);
    let stored = tree.frames_dir(lane);
    // Undo the rebasing `describe::frames_on_disk` does: its seconds are measured from the earliest picture,
    // while files carry their own clock names. A scene may plant frames at any instant, so the offset is read
    // off the folder; otherwise ffmpeg is asked for a file whose name was never written.
    let zero = earliest_name(&stored)
        .and_then(|name| stored_seconds(&name.to_string_lossy(), start))
        .unwrap_or(0.0);
    let mut scaled = 0usize;
    for at in &todo {
        let input = stored.join(frames::frame_file(start + zero, *at));
        if !input.exists() {
            return Err(format!(
                "{}: no stored frame to scale -- {} is not there",
                out_dir.display(),
                input.display()
            ));
        }
        // The scale filter is an OUTPUT option: ffmpeg applies options to the file named AFTER them, so a `-vf`
        // before the `-i` becomes an input option, scales nothing, and ffmpeg COPIES the JPEG (exit 0, native
        // width). That silent no-op is why the width written is READ BACK below instead of trusted.
        // `-update 1` lets one still image be written under a plain literal name, which must be the frame's own
        // clock name because `missing`, `read_images` and S2's resume rule resolve `describe::scaled_name`.
        let out = out_dir.join(describe::scaled_name(base, *at));
        let mut plan: Vec<String> = vec![
            "-y".to_string(),
            "-an".to_string(),
            "-i".to_string(),
            input.display().to_string(),
            "-frames:v".to_string(),
            "1".to_string(),
        ];
        plan.extend(describe::scale_plan(describe::FRAME_WIDTH));
        plan.push("-update".to_string());
        plan.push("1".to_string());
        plan.push(out.display().to_string());
        let owned: Vec<&str> = plan.iter().map(String::as_str).collect();
        subprocess::run(&ffmpeg, &owned, &mut *log)?;
        // The width is read back rather than trusted, but a probe that cannot answer (no ffprobe beside the
        // configured ffmpeg, or a stub) is not a failed scale: the copy exists and ffmpeg exited 0. Only a
        // number that DISAGREES is an error.
        if let Some(made) = image_width(&out, &conf.ffmpeg) {
            if made != describe::FRAME_WIDTH {
                return Err(format!(
                    "{}: ffmpeg wrote a {made}-wide copy, not the {}-wide one the frame was asked to be \
                     scaled to -- the filter did not reach the picture",
                    out.display(),
                    describe::FRAME_WIDTH
                ));
            }
        } else {
            // The probe could not answer: say so once instead of failing (absence is `checks.rs`'s diagnosis).
            log(&format!(
                "    {} scaled without a measurable width: no ffprobe at {}",
                out.display(),
                ffprobe_for(&conf.ffmpeg)
            ));
        }
        scaled += 1;
    }
    Ok(scaled)
}

/// Which probe answers the scaling leg: ffprobe beside the configured ffmpeg ([`crate::services::ffprobe_for`]),
/// so a made file is probed by the install that made it. An empty box means the plain name off PATH.
fn ffprobe_for(configured: &str) -> String {
    crate::services::ffprobe_for(&crate::separate_media::program(configured), "ffprobe")
}

/// The width of a picture on disk, read with ffprobe, or `None` when nothing readable is there. A test that
/// only checked the file exists could not tell a scaled copy from a byte-for-byte copy.
///
/// A probe that cannot be asked is also `None`, and `ensure` keeps the proof it had (the copy exists).
pub fn image_width(path: &Path, configured_ffmpeg: &str) -> Option<u32> {
    let probe = std::process::Command::new(ffprobe_for(configured_ffmpeg))
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            // ONE field: a width is all this leg enforces, and `width,height` would make a stub ffprobe invent a height.
            "stream=width",
            "-of",
            "csv=p=0",
        ])
        .arg(path)
        .output()
        .ok()?;
    if !probe.status.success() {
        return None;
    }
    // Trimmed before parsing: ffprobe answers `896` plus a newline, which read as "the filter did not reach the picture".
    String::from_utf8_lossy(&probe.stdout).trim().parse().ok()
}

/// The offset a source's stored frames are named at: its first second on the session clock, computed over
/// the WHOLE session because leaving one file out slides every other (`fix_transcripts::placement`). An
/// unreadable project sits at the session's start, where an unstamped file sits anyway (§8).
pub fn clock_start(project: Option<&crate::project::Project>, index: usize) -> f64 {
    let Some(project) = project else {
        return 0.0;
    };
    let names: Vec<&str> = project
        .sources
        .iter()
        .map(|source| source.path.rsplit('/').next().unwrap_or(&source.path))
        .collect();
    crate::fix_transcripts::placement(&names)
        .into_iter()
        .nth(index)
        .unwrap_or(0.0)
}

/// The recordings heard alongside this video, each with the offset its words are placed at (§F1.7 S1 wants
/// the sentence before the count). "Alongside" is every non-footage row of the session (§4).
pub fn alongside(project: Option<&crate::project::Project>, index: usize) -> Vec<(String, f64)> {
    let Some(project) = project else {
        return Vec::new();
    };
    let names: Vec<&str> = project
        .sources
        .iter()
        .map(|source| source.path.rsplit('/').next().unwrap_or(&source.path))
        .collect();
    let offsets = crate::fix_transcripts::placement(&names);
    let video = offsets.get(index).copied().unwrap_or(0.0);
    project
        .sources
        .iter()
        .enumerate()
        .filter(|(_, source)| !source.footage)
        .map(|(position, source)| {
            let name = source
                .path
                .rsplit('/')
                .next()
                .unwrap_or(&source.path)
                .to_string();
            (name, offsets.get(position).copied().unwrap_or(0.0) - video)
        })
        .collect()
}

/// The earliest frame name in a folder: the instant `describe::frames_on_disk` calls second nought.
pub fn earliest_name(dir: &Path) -> Option<std::ffi::OsString> {
    let mut names: Vec<std::ffi::OsString> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name())
        .filter(|name| name.to_string_lossy().ends_with(".jpg"))
        .collect();
    names.sort();
    names.into_iter().next()
}

/// The instant a stored frame's name claims, in seconds from the start of the take: `frame_name`'s inverse
/// minus this source's clock offset, to say which file on disk a picked second is.
pub fn stored_seconds(name: &str, start: f64) -> Option<f64> {
    clock::read_frame(name.strip_suffix(".jpg").unwrap_or(name)).map(|at| at - start)
}
