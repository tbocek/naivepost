//! Describe's S1 media leg: the scaled copies the vision request sends, made by ffmpeg for real.
//!
//! [`crate::describe`] holds every rule of F1.7 and spawns nothing — its own header says so — and
//! [`crate::describe::scale_plan`] only builds arguments. This is the half that runs them, the way
//! [`crate::frames_run`] is the half that runs `frames::extract_plan`'s commands: one ffmpeg per
//! frame that still needs a copy, each writing into `.llmframes/`, each checked for the file it was
//! supposed to write before the pass may claim the frame exists.
//!
//! The stored frames are untouched. F1.6 wrote them at the video's own size and they stay that way;
//! only the copies sent to the model are 896 wide, which is what keeps a 71-minute take at about
//! 0.9 GB of sent pixels instead of 2.7 GB of stored ones (the measurement §F1.1 S1 quotes).

use std::path::Path;

use crate::layout::Tree;
use crate::settings;
use crate::{clock, describe, frames, separate_media, subprocess};

/// Which of these frames have no scaled copy yet — the pure half of [`ensure`].
///
/// Resume lives here rather than in a marker file: "scaled ONCE" (§F1.7 S1) means a copy already on
/// disk is not paid for twice, and the copy's existence is exactly the fact that rule needs. A
/// half-written run therefore costs only the frames it had not reached.
pub fn missing(dir: &Path, frames: &[f64]) -> Vec<f64> {
    frames
        .iter()
        .copied()
        .filter(|at| !dir.join(describe::scaled_name(*at)).exists())
        .collect()
}

/// Scale every frame that still needs it into `.llmframes/` and say how many were made.
///
/// `start` is this source's first second on the session clock, the same number F1.6 handed
/// `frames::extract_plan`: the stored files are named on the FILE's clock (`frame_name(start + at)`),
/// while everything in `describe` works in seconds-from-the-start of the take, because
/// `describe::frames_on_disk` rebases the names it reads onto the earliest picture. So the offset is
/// needed to find the input file and deliberately NOT to name the copy: `read_images` looks the copy
/// up as `describe::scaled_name(at)`, at the rebased second, and both halves have to agree or every
/// chunk after the first asks for pictures that were never written.
///
/// A command that exits 0 having written nothing is an error naming the file, the same proof
/// `frames_run::extract_chunk` applies: an absent copy would otherwise surface three chunks later as
/// "no scaled frame to describe", far from the ffmpeg that failed.
pub fn ensure(
    tree: &Tree,
    conf: &settings::Conf,
    lane: &str,
    start: f64,
    frames: &[f64],
    log: &mut impl FnMut(&str),
) -> Result<usize, String> {
    let out_dir = tree.llm_frames_dir(lane);
    let todo = missing(&out_dir, frames);
    if todo.is_empty() {
        return Ok(0);
    }
    std::fs::create_dir_all(&out_dir).map_err(|why| format!("{}: {why}", out_dir.display()))?;
    let ffmpeg = separate_media::program(&conf.ffmpeg);
    let stored = tree.frames_dir(lane);
    // The rebasing `describe::frames_on_disk` does, undone: the seconds it hands back are measured from
    // the earliest picture that exists, while the files on disk carry their own clock names. Scaling a
    // frame whose name sits before that first picture would ask ffmpeg for a file that is not there.
    // The rebasing `describe::frames_on_disk` does, undone here: the seconds it hands back are measured
    // from the earliest picture that exists, while the files on disk carry their own clock names. A
    // scene may plant its frames at any instant, so the offset is read off the folder rather than
    // assumed -- without this, scaling asks ffmpeg for a file whose name was never written.
    let zero = earliest_name(&stored)
        .and_then(|name| stored_seconds(&name.to_string_lossy(), start))
        .unwrap_or(start);
    let mut scaled = 0usize;
    for at in &todo {
        let input = stored.join(frames::frame_file(zero, *at));
        if !input.exists() {
            return Err(format!(
                "{}: no stored frame to scale -- {} is not there",
                out_dir.display(),
                input.display()
            ));
        }
        // One frame per call: `scale_plan` ends with the output name, so the plan for one second is
        // exactly this frame's filter plus the file it writes, beside the input it scales.
        let mut args: Vec<String> = vec!["-i".to_string(), input.display().to_string()];
        args.extend(describe::scale_plan(describe::FRAME_WIDTH, &[*at]));
        args.push(out_dir.display().to_string());
        let owned: Vec<&str> = args.iter().map(String::as_str).collect();
        subprocess::run(&ffmpeg, &owned, &mut *log)?;
        let written = out_dir.join(describe::scaled_name(*at));
        if !written.exists() {
            return Err(format!("{}: ffmpeg wrote no scaled frame", written.display()));
        }
        scaled += 1;
    }
    Ok(scaled)
}

/// The offset a source's stored frames are named at: its first second on the session clock, computed
/// over the WHOLE session because leaving one file out moves zero and slides every other file
/// (`fix_transcripts::placement`'s own doc). A project that cannot be read sits at the session's
/// start, which is where an unstamped file sits anyway (§8).
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

/// The recordings heard alongside this video, each with the offset its words are placed at: §F1.7 S1
/// wants the sentence before the count, because the words are what the descriptions are built from.
///
/// "Alongside" is every non-footage row of the session — the microphone, the desktop track — since
/// §4 gives a video the hearing of the audio rows rather than a per-pair tick box.
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
            let name = source.path.rsplit('/').next().unwrap_or(&source.path).to_string();
            (name, offsets.get(position).copied().unwrap_or(0.0) - video)
        })
        .collect()
}

/// The earliest frame name in a folder -- the instant `describe::frames_on_disk` calls second nought.
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

/// The instant a stored frame's name claims, as seconds-from-the-start of the take: `frame_name`'s
/// inverse minus this source's clock offset. `describe::frames_on_disk` rebases what it reads onto the
/// earliest picture, so a caller holding one picked second needs this to say which file on disk that
/// second is — the two directions of the same naming rule, kept beside each other.
pub fn stored_seconds(name: &str, start: f64) -> Option<f64> {
    clock::read_frame(name.strip_suffix(".jpg").unwrap_or(name)).map(|at| at - start)
}
