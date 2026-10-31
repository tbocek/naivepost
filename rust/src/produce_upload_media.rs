//! F5.6 S3/S6: the PIXELS behind the image row.
//!
//! [`crate::produce_upload`] holds the words half and touches no tool, so rows arrive as strings (`frame:34.0`, `project:produce/publish/chosen-0.jpg`). This module keeps that promise: it resolves an entry to a file,
//! crops it to the video's shape, and renders the `data:image/...;base64,` URL the sd.cpp body carries (§F5.6 S6: "base cropped, references raw"); the result lands as `thumbnail-plain.png`,
//! which [`crate::produce_upload_print`] prints onto.
//!
//! ffmpeg runs through [`crate::subprocess::run`]; its program follows `produce_probe::ffprobe_program`: `NAIVEPOST_FFMPEG`, else the settings box, read per call.

use std::path::{Path, PathBuf};

use crate::produce_details as details;
use crate::produce_upload_frames as spread;
use crate::project::Publish;

/// The program ffmpeg runs from: `NAIVEPOST_FFMPEG` when set and non-empty, else the settings box, else the name itself (as [`crate::produce_probe`]'s ffprobe rule).
pub fn ffmpeg_program() -> String {
    let pointed = std::env::var("NAIVEPOST_FFMPEG").unwrap_or_default();
    if !pointed.trim().is_empty() {
        return pointed.trim().to_string();
    }
    let program = crate::settings::from_environment()
        .map(|paths| crate::settings::read(&paths).unwrap_or_default().ffmpeg)
        .unwrap_or_default()
        .trim()
        .to_string();
    // An empty box means "the usual one", not "run a program called nothing".
    if program.trim().is_empty() {
        return crate::produce_exec::FFMPEG.to_string();
    }
    program
}

/// The mime sd.cpp expects; the extension is the only claim available and every frame Prepare writes is a `.jpg`.
fn mime_of(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "png" => "image/png",
        _ => "image/jpeg",
    }
}

/// One row entry → the file it names. `frame:<seconds>` resolves through `frames` (moments on disk beside their paths); `project:`, absolute and root-relative entries go through [`crate::layout::resolve`].
/// `None` when the file is not there; the caller logs [`crate::produce_details::reference_skipped_log`].
pub fn entry_file(
    tree: &crate::layout::Tree,
    entry: &str,
    frames: &[(f64, PathBuf)],
) -> Option<PathBuf> {
    if let Some(named) = details::frame_line(entry) {
        // `clip n +s` means the same moment as the seconds it adds up to, so one resolution rule serves both.
        let at = details::frame_moment(named, &[])?;
        let (_, path) = frames.iter().min_by(|(a, _), (b, _)| {
            (a - at)
                .abs()
                .partial_cmp(&(b - at).abs())
                .unwrap_or(std::cmp::Ordering::Equal)
        })?;
        return path.is_file().then(|| path.clone());
    }
    let written = entry.trim();
    if written.is_empty() {
        return None;
    }
    let path = crate::layout::resolve(tree.dir(), tree.dir(), written);
    path.is_file().then_some(path)
}

/// A file as the data URL the image server takes: RAW bytes (§F5.6 S6), base64 from [`crate::checks::base64`].
pub fn data_url(path: &Path) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;
    if bytes.is_empty() {
        return None;
    }
    Some(format!(
        "data:{};base64,{}",
        mime_of(path),
        crate::checks::base64(&bytes)
    ))
}

/// ffmpeg's argv for one centre-crop-and-scale: the source scaled to cover `wide`x`high` then trimmed to it, centred on `crop` (a fraction of each axis, §5's `Crop`).
pub fn crop_args(from: &Path, to: &Path, crop: (f64, f64), wide: u32, high: u32) -> Vec<String> {
    vec![
        "-y".into(),
        "-i".into(),
        from.display().to_string(),
        "-vf".into(),
        crop_filter(crop, wide, high),
        "-frames:v".into(),
        "1".into(),
        to.display().to_string(),
    ]
}

/// `scale=W:H:force_original_aspect_ratio=increase,crop=W:H:x:y`, the crop centred on `crop` and clamped inside the source; one filter serves any frame size with no probe.
/// `wide`/`high` already ARE the video's shape (`frame_size_from_aspect`).
///
/// Scale comes FIRST: a crop is a fraction OF THE SOURCE, and feeding the draw size to `crop` makes ffmpeg refuse the filtergraph on a narrower frame
/// (`Invalid too big or non positive size`). Scaling to cover first makes every source at least target-sized.
///
/// The chain stays ONE unquoted argument: a quote inside `-vf` reads as an argument separator ("Error parsing filterchain"); no shell sits between.
pub fn crop_filter(crop: (f64, f64), wide: u32, high: u32) -> String {
    let cx = crop.0.clamp(0.0, 1.0);
    let cy = crop.1.clamp(0.0, 1.0);
    format!("scale={wide}:{high}:force_original_aspect_ratio=increase,crop={wide}:{high}:(iw-{wide})*{cx:.4}:(ih-{high})*{cy:.4}")
}

/// Crop one picture to the video's shape and the draw size, writing a PNG at `out`. `Err` carries ffmpeg's complaint ([`crate::subprocess::failure`]'s shape).
pub fn crop_to_shape(
    from: &Path,
    out: &Path,
    crop: (f64, f64),
    wide: u32,
    high: u32,
    mut log: impl FnMut(&str),
) -> Result<(), String> {
    if let Some(dir) = out.parent() {
        std::fs::create_dir_all(dir).map_err(|why| format!("{}: {why}", dir.display()))?;
    }
    let owned = crop_args(from, out, crop, wide, high);
    let args: Vec<&str> = owned.iter().map(String::as_str).collect();
    crate::subprocess::run(&ffmpeg_program(), &args, &mut log).map(|_| ())
}

/// The base cropped to the video's shape, then the data URL. Written to a scratch file beside the pictures so the crop that went out can be inspected.
pub fn cropped_data_url(
    from: &Path,
    scratch: &Path,
    crop: (f64, f64),
    wide: u32,
    high: u32,
    log: impl FnMut(&str),
) -> Option<String> {
    match crop_to_shape(from, scratch, crop, wide, high, log) {
        Ok(()) => data_url(scratch),
        // A failed crop is said where a person reads, and the base is sent uncropped: the wrong shape still shows the server the video.
        Err(why) => {
            crate::ui::window::log_line(&why);
            data_url(from)
        }
    }
}

/// The request's images as sd.cpp takes them. `base_url` is `None` when the row holds nothing that exists (§S6's "drawn from the instruction alone"); `urls` holds only what DID resolve.
#[derive(Debug, Default, PartialEq)]
pub struct Sent {
    /// The base, cropped to the video's shape and rendered as a data URL. `None` = nothing to edit.
    pub base: Option<String>,
    /// The base plus every reference, in the row's order — exactly what `ref_images` gets.
    pub urls: Vec<String>,
    /// The base file's name, which is what the request line prints as "editing <base>".
    pub base_name: String,
    /// The base's crop as a percentage of the source's width, measured off that file's own edges.
    pub crop_percent: u32,
    /// The crop centre that went into the filter, so the request line states what was actually asked for.
    pub centre: (f64, f64),
    /// Row entries with no picture behind them, already logged by name.
    pub skipped: Vec<String>,
}

pub fn request_images(
    tree: &crate::layout::Tree,
    publish: &Publish,
    frames: &[(f64, PathBuf)],
    wide: u32,
    high: u32,
    mut log: impl FnMut(&str),
) -> Sent {
    let scratch = tree.publish_dir().join("base-cropped.png");
    let crop = (publish.crop.x, publish.crop.y);
    let mut urls: Vec<String> = Vec::new();
    let mut skipped: Vec<String> = Vec::new();
    let mut base_name = String::new();
    let mut base_url: Option<String> = None;
    let mut base_source: Option<PathBuf> = None;
    for (index, entry) in publish.frames.iter().enumerate() {
        let Some(file) = entry_file(tree, entry, frames) else {
            log(&details::reference_skipped_log(entry));
            skipped.push(entry.clone());
            continue;
        };
        if index == 0 {
            base_name = crate::cut_insert::base_name(&file.display().to_string());
            base_source = Some(file.clone());
            base_url = cropped_data_url(&file, &scratch, crop, wide, high, &mut log);
            if base_url.is_none() {
                skipped.push(entry.clone());
                continue;
            }
            urls.extend(base_url.clone());
            continue;
        }
        match data_url(&file) {
            Some(url) => urls.push(url),
            None => {
                log(&details::reference_skipped_log(entry));
                skipped.push(entry.clone());
            }
        }
    }
    // Measured off the file that was actually cropped: 100% means the source already had the video's shape.
    let percent = match &base_source {
        Some(source) => crop_percent_of(source, (wide, high)),
        None => 100,
    };
    Sent {
        base: base_url,
        urls,
        base_name,
        crop_percent: percent,
        centre: crop,
        skipped,
    }
}

/// The shape back down to its ratio (1280x720 is 16:9, 1080x1080 is 1:1), so a percentage reads proportions rather than a pixel box.
fn ratio_of(shape: (u32, u32)) -> (u32, u32) {
    let (wide, high) = (shape.0.max(1), shape.1.max(1));
    let gcd = (1..=wide.min(high))
        .rev()
        .find(|d| wide % d == 0 && high % d == 0)
        .unwrap_or(1);
    (wide / gcd, high / gcd)
}

/// How much of the source's width the crop to `shape` keeps, measured off the file actually cropped (ffprobe reads its edges back): a 4:3 frame cropped to 16:9 keeps three quarters.
/// No probe (no ffprobe, an unreadable file) answers 100: absence is "nothing was trimmed", never a guessed fraction.
pub fn crop_percent_of(source_file: &Path, shape: (u32, u32)) -> u32 {
    let Some(source) = crate::produce_probe::size_of(source_file) else {
        return 100;
    };
    let source_ratio = f64::from(source.0.max(1)) / f64::from(source.1.max(1));
    // Compare ratios, never pixel boxes, so a 1080p source and a 4K one answer the same percentage.
    let (rw, rh) = ratio_of(shape);
    let want = f64::from(rw) / f64::from(rh);
    // Wider than the shape: all the height survives and the width is trimmed to `height * want`.
    let kept = if source_ratio >= want {
        want / source_ratio
    } else {
        1.0
    };
    ((kept * 100.0).round() as u32).clamp(1, 100)
}

/// The chosen or picked frame as the plain picture (§F5.6 S3: "the nearest frame, cropped to the video's shape, IS the thumbnail"). Without it `thumbnail-plain.png` never exists.
pub fn write_own_picture(
    tree: &crate::layout::Tree,
    publish: &Publish,
    frames: &[(f64, PathBuf)],
    wide: u32,
    high: u32,
    log: impl FnMut(&str),
) -> Result<(), String> {
    let entry = publish
        .frames
        .first()
        .cloned()
        .or_else(|| spread::spread_rows(&[]).first().cloned())
        .ok_or("no frame to take as the thumbnail")?;
    let file = entry_file(tree, &entry, frames)
        .ok_or_else(|| format!("{entry} is not a picture this session has"))?;
    crop_to_shape(
        &file,
        &tree.thumbnail_plain_png(),
        (publish.crop.x, publish.crop.y),
        wide,
        high,
        log,
    )?;
    // Stands as the thumbnail until words are printed on it; a picture with none gets no print pass.
    std::fs::copy(tree.thumbnail_plain_png(), tree.thumbnail_png())
        .map(|_| ())
        .map_err(|why| format!("{}: {why}", tree.thumbnail_png().display()))
}

/// The JPEG ladder's rung as ffmpeg's `-q:v` (mjpeg takes 2..31, LOWER is better): linear in the useful band and clamped, so 92 asks for near-lossless (2) and 40 for 12.
/// §F5.7 names the rungs, not their encoder dial, so this table is the app's decision.
pub fn q_v_of_quality(quality: u32) -> u32 {
    let rung = 2u32.saturating_add(60u32.saturating_sub(quality.min(100)) / 5);
    rung.clamp(2, 31)
}

/// ffmpeg's argv for one rung: the PNG re-encoded as a JPEG at that quality. Single frame, no audio.
pub fn jpeg_args(png: &Path, out: &Path, quality: u32) -> Vec<String> {
    vec![
        "-y".into(),
        "-i".into(),
        png.display().to_string(),
        "-frames:v".into(),
        "1".into(),
        "-q:v".into(),
        q_v_of_quality(quality).to_string(),
        out.display().to_string(),
    ]
}

/// ⤓ export's real ladder: encode `png` at each rung of [`crate::produce_runs::JPEG_QUALITIES`] INTO `out`, stopping at the FIRST rung whose weight fits [`crate::produce_runs::JPEG_MAX_BYTES`] (§F5.7).
/// The file left on disk IS the rung chosen; the vec holds the weights of the rungs actually encoded, best first. When nothing fits, the last attempt stays written.
///
/// Every rung is measured, so [`crate::produce_runs::jpeg_rung`] answers from encoder output; a failed rung contributes the last good weight, never a free win.
pub fn jpeg_ladder(
    png: &Path,
    out: &Path,
    qualities: &[u32],
    mut log: impl FnMut(&str),
) -> Vec<u64> {
    let mut sizes: Vec<u64> = Vec::with_capacity(qualities.len());
    let mut last = 0u64;
    for quality in qualities {
        let args = jpeg_args(png, out, *quality);
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        if crate::subprocess::run(&ffmpeg_program(), &refs, &mut log).is_ok() {
            last = out.metadata().map(|meta| meta.len()).unwrap_or(0);
        }
        sizes.push(last);
        // Stop the moment a rung fits: the file on disk is that rung, and no lower quality is wanted.
        if last <= crate::produce_runs::JPEG_MAX_BYTES {
            break;
        }
    }
    sizes
}

/// Every frame this session has on disk, as `(produced-clock moment, path)` pairs for `entry_file`; `listed` is [`listed_frames`] per source with each moment already moved onto the produced clock by the caller.
/// Duplicates collapse: the moment makes an instant, not the file name a second source writes it into.
pub fn frame_files(listed: &[(f64, PathBuf)]) -> Vec<(f64, PathBuf)> {
    let mut out: Vec<(f64, PathBuf)> = Vec::with_capacity(listed.len());
    for (moment, path) in listed {
        if !out.iter().any(|(seen, _)| (seen - moment).abs() < 0.001) {
            out.push((*moment, path.clone()));
        }
    }
    out
}

/// List one source's extracted frames as `(session seconds, path)` pairs, oldest first, by Prepare's naming rule: `zero` is the session's ([`crate::frames_run::session_zero`])
/// that turns the name back into a session second ([`crate::clock::frame_session_seconds`]).
pub fn listed_frames(dir: &Path, zero: f64) -> Vec<(f64, PathBuf)> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<(f64, PathBuf)> = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            let stem = path.file_stem()?.to_str()?;
            crate::clock::frame_session_seconds(stem, zero).map(|seconds| (seconds, path))
        })
        .collect();
    out.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    out
}
