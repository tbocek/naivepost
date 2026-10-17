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
        // One frame per call, and where the pieces sit is the whole trick. ffmpeg reads an option as
        // belonging to the file named AFTER it, so the scale filter has to reach the picture through
        // the OUTPUT slot: a `-vf` ahead of the `-i` becomes an input option, is applied to nothing,
        // and ffmpeg then COPIES the JPEG -- exit 0, a 1280-wide "scaled" copy. That silent no-op is
        // exactly what this item exists to catch, which is why the width written is READ BACK below
        // and compared instead of being taken on trust from the arguments that were passed.
        // `-update 1` is the other half: it lets one still image be written under a plain, literal
        // name, and that name has to be the frame's own clock name, because `missing`, `read_images`
        // and S2's resume rule all resolve `describe::scaled_name`.
        let out = out_dir.join(describe::scaled_name(*at));
        // The scale filter is an OUTPUT option, which in ffmpeg means it has to appear AFTER the `-i`
        // whose stream it acts on: a `-vf` sitting before the `-i` becomes an option of the input, is
        // applied to nothing, and ffmpeg then COPIES the JPEG -- exit 0, a native-width "scaled" copy,
        // which is the silent no-op this item exists to catch. `-update 1` is the other half: it lets
        // one still image be written under a plain literal name instead of demanding a `%d` pattern.
        // The scale filter is an OUTPUT option: ffmpeg reads the options belonging to a file in the
        // slot AFTER that file's `-i`, so a `-vf` placed before the `-i` becomes an option of the INPUT,
        // scales nothing, and ffmpeg then COPIES the JPEG -- exit 0, a native-width "scaled" copy. That
        // silent no-op is what this item exists to catch, which is why the width written is READ BACK
        // below rather than trusted from the arguments that were passed.
        let mut plan: Vec<String> = vec![
            "-y".to_string(),
            "-an".to_string(),
            "-i".to_string(),
            input.display().to_string(),
            "-frames:v".to_string(),
            "1".to_string(),
        ];
        plan.extend(describe::scale_plan(describe::FRAME_WIDTH));
        // `-update 1` is what lets ffmpeg write one still image under a plain literal name at all (an
        // image output otherwise has to carry a `%d` pattern), and the name is the frame's own clock name
        // because `missing`, `read_images` and S2's resume rule all resolve it through
        // `describe::scaled_name`.
        // `-update 1` is what lets ffmpeg write one still image under a plain literal name at all (an
        // image output otherwise has to carry a `%d` pattern), and the name is the frame's own clock name
        // because `missing`, `read_images` and S2's resume rule all resolve it through
        // `describe::scaled_name`. Naming the FILE rather than handing ffmpeg a directory keeps the
        // contract explicit for anything standing in for ffmpeg too: it is told exactly which path the
        // stage then reads back.
        plan.push("-update".to_string());
        plan.push("1".to_string());
        plan.push(out.display().to_string());
        let owned: Vec<&str> = plan.iter().map(String::as_str).collect();
        subprocess::run(&ffmpeg, &owned, &mut *log)?;
        // The width is read back off the file rather than trusted from the arguments -- but a probe that
        // cannot answer (no ffprobe beside the configured ffmpeg, or a stub that does not know the
        // question) is not a failed scale: the copy exists and ffmpeg exited 0, which is the proof
        // `frames_run::extract_chunk` applies. Only a number that DISAGREES is an error.
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
            // The probe could not answer at all (no ffprobe beside the configured ffmpeg, or a stub that
            // does not know the question). Say so once instead of failing: absence is `checks.rs`'s
            // diagnosis, and the copy's existence plus ffmpeg's exit 0 is the proof that remains.
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

/// Which probe answers the scaling leg: ffprobe beside the ffmpeg the settings name (§1's rule, and
/// [`crate::services::ffprobe_for`] is that rule), so a probe of a made file comes from the same install
/// as the tool that made it. An empty box means the plain name off PATH.
fn ffprobe_for(configured: &str) -> String {
    crate::services::ffprobe_for(&crate::separate_media::program(configured), "ffprobe")
}

/// The width of a picture on disk, read with ffprobe, or `None` when nothing readable is there. The
/// scaling leg uses it to prove it wrote a smaller picture than it was given; a test that only checked
/// the file exists cannot tell a scaled copy from a byte-for-byte copy of the frame.
///
/// A probe that cannot be asked -- no ffprobe beside the configured ffmpeg, a stub that does not know
/// the question -- is also `None`, and `ensure` then keeps the proof it already had (the copy exists):
/// an absent measuring tool is `checks.rs`'s diagnosis, not a reason to refuse work ffmpeg did do.
pub fn image_width(path: &Path, configured_ffmpeg: &str) -> Option<u32> {
    let probe = std::process::Command::new(ffprobe_for(configured_ffmpeg))
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            // ONE field: the rule this leg enforces is a width, and asking for `width,height` would make
            // a stub ffprobe have to invent a height nothing here reads. A single field answers one line,
            // which is what is parsed below.
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
    // Trimmed before parsing: ffprobe answers `896\n`, and a trailing newline is not a number -- losing
    // it read as "the filter did not reach the picture" instead of as an unparseable reply.
    String::from_utf8_lossy(&probe.stdout).trim().parse().ok()
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
