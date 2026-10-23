//! F3.5 S5 — the ONE place a drawing's preview raster is actually made.
//!
//! §S5 asks for "Preview raster via ffmpeg, 512 px, transparent, cached per file". [`crate::fx_svg`] holds the
//! rule halves — the size ([`crate::fx_svg::PREVIEW_RASTER_PX`]), the argument vector
//! ([`crate::fx_svg::ffmpeg_raster`]) and the per-file cache ([`crate::fx_svg::Rasters`]) — but a vector is not a
//! picture: someone has to run ffmpeg and keep what it wrote. That is this module, and nothing else: no wording,
//! no geometry, no state of its own.
//!
//! The output file is the resume marker, which is `spec/00-principles.md`'s rule for every step with an output: if
//! the png exists, the work is done, so ffmpeg is not asked again and a drawing placed six times costs one raster.

use std::path::{Path, PathBuf};

use crate::fx_svg;
use crate::subprocess;

/// Make (or find) this drawing's 512 px transparent preview copy and hand back where it lives.
///
/// `program` is the ffmpeg this session cuts with — the caller passes [`crate::audio_task::ffmpeg_program`], so a
/// path typed in Settings wins over the bare name exactly as it does for every other ffmpeg job, and a test can
/// point it at a stub through that same door rather than through anything test-shaped here.
///
/// `dir` is the project's raster folder ([`crate::layout::Tree::svg_raster_dir`]). An existing png there for this
/// file is returned untouched: no ffmpeg, no re-read of the svg, which is what makes the second placement of a logo
/// free (§S5's `cached per file`).
///
/// On failure the reason comes back as `Err` and it is the caller's line to say — §A.5 wants it said ONCE, and
/// "once" is a property of the cache, not of this call.
pub fn make(program: &str, path: &str, dir: &Path, log: &dyn Fn(&str)) -> Result<PathBuf, String> {
    let out = dir.join(fx_svg::raster_file_name(path));
    if out.exists() {
        return Ok(out);
    }
    std::fs::create_dir_all(dir).map_err(|err| format!("{}: {err}", dir.display()))?;
    let argv = fx_svg::ffmpeg_raster(program, path, &out.display().to_string());
    // Strings to &strs without a second list: `subprocess::run` takes refs, `ffmpeg_raster` owns the args.
    let refs: Vec<&str> = argv.iter().map(String::as_str).skip(1).collect();
    // A real raster on this box needs an svg decoder. It does not have one: ffmpeg 8.1.2 here reports zero svg
    // decoders (`ffmpeg -decoders | grep -c svg` prints 0), because Alpine's build ships without librsvg. So on
    // this container a genuine .svg legitimately fails here, and reporting that failure is the honest outcome —
    // the effect still places, §A.5 says the drawing cannot be shown once, and nothing pretends a card appeared.
    subprocess::run(program, &refs, |line| log(line))?;
    // ffmpeg exiting 0 without writing the file is a failure too: an empty success would mark the file ready and
    // leave the panel painting nothing, which is the exact lie this module exists to avoid.
    if !out.exists() {
        return Err(format!(
            "{} produced no raster at {}",
            program,
            out.display()
        ));
    }
    Ok(out)
}
