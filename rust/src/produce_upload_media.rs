//! F5.6 S3/S6: the PIXELS behind the image row.
//!
//! [`crate::produce_upload`] holds every rule of the words half and touches no tool (its doc says so), so
//! the row arrives there as strings — `frame:34.0`, `project:produce/publish/chosen-0.jpg`. Those strings
//! are a PROMISE about a picture; this module is the half that keeps it: it resolves a row entry to the file
//! on disk, crops it to the video's shape, and renders it as the `data:image/...;base64,` URL the sd.cpp
//! body carries (§F5.6 S6: "base cropped, references raw"). The drawn or chosen bytes then land as
//! `thumbnail-plain.png`, which is the only thing [`crate::produce_upload_print`] can print onto — without
//! this module the walk can describe a thumbnail but never produce one.
//!
//! ffmpeg comes through [`crate::subprocess::run`] like every other tool step, so its command line is in the
//! log before it runs. Where it runs from follows `produce_probe::ffprobe_program` exactly: `NAIVEPOST_FFMPEG`
//! when set, else the settings box, read per call so a re-typed box takes effect on the next crop — and a
//! test points the SAME door at a fake rather than at a slot only tests fill.

use std::path::{Path, PathBuf};

use crate::produce_details as details;
use crate::produce_upload_frames as spread;
use crate::project::Publish;

/// The program ffmpeg runs from: `NAIVEPOST_FFMPEG` when set and non-empty, else the settings box, else the
/// name itself. Mirrors [`crate::produce_probe`]'s ffprobe rule so one environment pair relocates both.
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

/// A JPEG read into a surface needs the mime the sd.cpp server expects; the extension is the only claim
/// available and every frame Prepare writes is a `.jpg`.
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

/// One row entry → the file it names. `frame:<seconds>` (any spelling `produce_details::frame_line` reads)
/// resolves through `frames`, the moments actually on disk beside their paths; `project:` and absolute and
/// root-relative entries go through §1's ONE path rule ([`crate::layout::resolve`]). `None` when the file is
/// not there — the caller logs [`crate::produce_details::reference_skipped_log`] rather than sending an
/// image that does not exist.
pub fn entry_file(
    tree: &crate::layout::Tree,
    entry: &str,
    frames: &[(f64, PathBuf)],
) -> Option<PathBuf> {
    if let Some(named) = details::frame_line(entry) {
        // `clip n +s` means the same moment as the seconds it adds up to, so one resolution rule serves both.
        let at = details::frame_moment(named, &[])?;
        let (_, path) = frames
            .iter()
            .min_by(|(a, _), (b, _)| (a - at).abs().partial_cmp(&(b - at).abs()).unwrap_or(std::cmp::Ordering::Equal))?;
        return path.is_file().then(|| path.clone());
    }
    let written = entry.trim();
    if written.is_empty() {
        return None;
    }
    let path = crate::layout::resolve(tree.dir(), tree.dir(), written);
    path.is_file().then_some(path)
}

/// A file as the data URL the image server takes. RAW bytes (§F5.6 S6 says references go raw), base64 from
/// [`crate::checks::base64`] so every decoder the server might be implements it.
pub fn data_url(path: &Path) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;
    if bytes.is_empty() {
        return None;
    }
    Some(format!("data:{};base64,{}", mime_of(path), crate::checks::base64(&bytes)))
}

/// ffmpeg's argv for one centre-crop-and-scale: the source scaled to cover the draw size `wide`x`high` and
/// then trimmed back to it, centred on `crop` (a fraction of each axis, §5's `Crop`). The scale is what makes
/// the crop honest — a base sent at its own size is the server's problem to fit, and §F5.6 asks for the video's
/// shape at long side 1280 — which is why the shape arrives only as that pixel pair.
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

/// `scale=W:H:force_original_aspect_ratio=increase,crop=W:H:x:y` — the crop centred on `crop` and clamped
/// inside the source. ffmpeg's expressions keep the maths out of here, so one filter serves a 1280x720 and a
/// 3840x2160 frame with no probe first. The draw size arrives as `wide`/`high` alone: they already ARE the
/// shape (`frame_size_from_aspect` builds both from the cut's aspect), so no second shape argument exists to
/// disagree with them.
///
/// WHY scale comes FIRST: a crop is a fraction OF THE SOURCE, so it cannot express "the video's shape at long
/// side 1280" on its own — feeding the draw size to `crop` asks for a box 1280 pixels wide and ffmpeg refuses
/// the whole filtergraph on a narrower frame (`Invalid too big or non positive size`). Scaling to cover first
/// makes every source at least as big as the target on both axes, and the crop then trims the overshoot while
/// honouring the centre as a fraction of that frame.
///
/// The chain stays ONE unquoted argument: ffmpeg's own filtergraph parser reads a quote inside `-vf` as an
/// argument separator, and a mis-closed one rejects the graph ("Error parsing filterchain") before a pixel is
/// read. No shell sits between this string and ffmpeg, so nothing needs shell quoting either.
pub fn crop_filter(crop: (f64, f64), wide: u32, high: u32) -> String {
    let cx = crop.0.clamp(0.0, 1.0);
    let cy = crop.1.clamp(0.0, 1.0);
    format!("scale={wide}:{high}:force_original_aspect_ratio=increase,crop={wide}:{high}:(iw-{wide})*{cx:.4}:(ih-{high})*{cy:.4}")
}

/// Crop one picture to the video's shape and the draw size, writing a PNG at `out`. `Err` carries ffmpeg's
/// own complaint (via [`crate::subprocess::failure`]'s shape) so the log can name the file that failed.
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

/// The base as the request wants it: cropped to the video's shape, then the data URL. Written into a scratch
/// file beside the pictures so the crop that went out is a file a person can look at afterwards, not a
/// pipe nobody can inspect.
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
        // A crop that failed says so where a person reads: the base is then sent uncropped rather than not
        // sent, because a picture at the wrong shape still shows the server what the video looks like.
        Err(why) => {
            crate::ui::window::log_line(&why);
            data_url(from)
        }
    }
}

/// The whole image row as the request wants it: the base cropped, everything after it RAW (§F5.6 S6).
/// `frames` is every extracted moment on disk with its path, so a `frame:` entry resolves to pixels; an
/// entry with no file is skipped and NAMED (the log line says which), because sending a URL for a missing
/// file would read as the server ignoring an image rather than the app never having it.
///
/// The draw size arrives as `wide`/`high` alone — they already ARE the video's shape, since
/// `frame_size_from_aspect` builds both from the cut's aspect — except where a percentage of the SOURCE's own
/// width is reported (`crop_percent_of` reads that file's edges back).
///
/// The request's images as sd.cpp takes them: `(base_url, every_url, base_name, skipped)`.
/// `base_url` is `None` when the row holds nothing that exists, which is §S6's "drawn from the instruction
/// alone, no images" case; `every_url` then holds only what DID resolve, so the body never carries a phantom.
#[derive(Debug, Default, PartialEq)]
pub struct Sent {
    /// The base, cropped to the video's shape and rendered as a data URL. `None` = nothing to edit.
    pub base: Option<String>,
    /// The base plus every reference, in the row's order — exactly what `ref_images` gets.
    pub urls: Vec<String>,
    /// The base file's name, which is what the request line prints as "editing <base>".
    pub base_name: String,
    /// The base's crop as a percentage of the source's width, measured off that file's own edges (§S6:
    /// "base cropped").
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
    // The crop the request line reports is measured off the file that was actually cropped, so a 100% claim
    // means the source already had the video's shape.
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

/// The shape back down to the ratio it came from, so a percentage reads proportions rather than a pixel box:
/// a shape produced by `frame_size` (long side 1280) is that same ratio with the common factor taken out —
/// 1280x720 is 16:9, 1080x1080 is 1:1, and a shape with no common factor keeps its own pair.
fn ratio_of(shape: (u32, u32)) -> (u32, u32) {
    let (wide, high) = (shape.0.max(1), shape.1.max(1));
    let gcd = (1..=wide.min(high)).rev().find(|d| wide % d == 0 && high % d == 0).unwrap_or(1);
    (wide / gcd, high / gcd)
}

/// How much of the source's width the crop to `shape` keeps, as the request line prints it: what fraction
/// survives. Measured off the file that was actually cropped (ffprobe reads its edges back), so the number in
/// the log is a fact about that JPEG and not about the shape the video happens to be — a 4:3 frame cropped to
/// 16:9 keeps three quarters of its width, a source already at the video's shape keeps all of it. No probe
/// (no ffprobe, an unreadable file) answers 100: absence is "nothing was trimmed", never a guessed fraction.
pub fn crop_percent_of(source_file: &Path, shape: (u32, u32)) -> u32 {
    let Some(source) = crate::produce_probe::size_of(source_file) else {
        return 100;
    };
    let source_ratio = f64::from(source.0.max(1)) / f64::from(source.1.max(1));
    // The shape's own ratio, with the draw size's common factor taken out: 1280x720 IS 16:9, and comparing
    // ratios (never pixel boxes) is what makes a 1080p source and a 4K one answer the same percentage.
    let (rw, rh) = ratio_of(shape);
    let want = f64::from(rw) / f64::from(rh);
    // Wider than the shape: all the height survives and the width is trimmed to `height * want`.
    let kept = if source_ratio >= want { want / source_ratio } else { 1.0 };
    ((kept * 100.0).round() as u32).clamp(1, 100)
}

/// The chosen or picked frame as the plain picture (§F5.6 S3: "the nearest frame, cropped to the video's
/// shape, IS the thumbnail", no model, no GPU). This is the leg the walk's own-frame branch calls: without it
/// `thumbnail-plain.png` never exists, so nothing can be printed onto it and no stamp can be written.
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
    let plain = tree.thumbnail_plain_png();
    let owned = crop_args(
        &file,
        &plain,
        (publish.crop.x, publish.crop.y),
        wide,
        high,
    );
    let args: Vec<&str> = owned.iter().map(String::as_str).collect();
    crate::subprocess::run(&ffmpeg_program(), &args, log).map(|_| ())
}

/// The JPEG quality ladder's rung as ffmpeg's `-q:v`. The mjpeg encoder takes a 2..31 scale where LOWER is
/// better, roughly the inverse of the 0–100 quality the ladder is written in; the mapping is linear in the
/// useful band and clamped, so quality 92 asks for near-lossless (2) and 40 for a heavy compression (12).
/// §F5.7 names the rungs, not their encoder dial, so this table is the app's decision and is stated here
/// rather than left implied by a magic number.
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

/// ⤓ export's real ladder: encode `png` at each rung of [`crate::produce_runs::JPEG_QUALITIES`] and hand
/// back the weight of the file each rung WROTE, best first. Every rung is measured, so
/// [`crate::produce_runs::jpeg_rung`] answers with a number an encoder produced rather than with the size
/// of the file that already existed (which made every ladder collapse to its first or last rung). A rung
/// whose encode failed contributes the last good weight, so a failed rung never reads as a free win.
pub fn jpeg_ladder(png: &Path, out: &Path, qualities: &[u32], mut log: impl FnMut(&str)) -> Vec<u64> {
    let mut sizes: Vec<u64> = Vec::with_capacity(qualities.len());
    let mut last = 0u64;
    for quality in qualities {
        let args = jpeg_args(png, out, *quality);
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        if crate::subprocess::run(&ffmpeg_program(), &refs, &mut log).is_ok() {
            last = out.metadata().map(|meta| meta.len()).unwrap_or(0);
        }
        sizes.push(last);
    }
    sizes
}

/// Every frame this session has on disk, as `(produced-clock moment, path)` pairs for `entry_file`.
/// `listed` is [`listed_frames`] per source, with each moment already moved onto the produced clock by the
/// caller (the page holds the cut, so only it can do that mapping). Duplicates collapse: two cameras showing
/// the same instant is one picture to send — the moment is what makes an instant, not the file name a second
/// source happens to write it into.
pub fn frame_files(listed: &[(f64, PathBuf)]) -> Vec<(f64, PathBuf)> {
    let mut out: Vec<(f64, PathBuf)> = Vec::with_capacity(listed.len());
    for (moment, path) in listed {
        if !out.iter().any(|(seen, _)| (seen - moment).abs() < 0.001) {
            out.push((*moment, path.clone()));
        }
    }
    out
}

/// List one source's extracted frames as `(seconds in the recording, path)` pairs, oldest first, using the
/// ONE naming rule Prepare wrote them with ([`crate::clock::read_frame`], the inverse of `clock::frame_name`).
pub fn listed_frames(dir: &Path) -> Vec<(f64, PathBuf)> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<(f64, PathBuf)> = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            let stem = path.file_stem()?.to_str()?;
            crate::clock::read_frame(stem).map(|seconds| (seconds, path))
        })
        .collect();
    out.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    out
}
