//! F3.5 S5: the one place a drawing's preview raster is made (§S5: 512 px, transparent, cached per file).
//! [`crate::fx_svg`] holds the rule halves; this runs ffmpeg and keeps the result. The output png is the resume marker (spec/00-principles.md).

use std::path::{Path, PathBuf};

use crate::fx_svg;
use crate::subprocess;

/// Make (or find) the 512 px transparent preview copy of this drawing and return where it lives.
/// `program` is the session ffmpeg ([`crate::audio_task::ffmpeg_program`]); `dir` is [`crate::layout::Tree::svg_raster_dir`]. An existing png there is returned untouched.
/// Failure comes back as `Err` for the caller to say once (§A.5).
pub fn make(program: &str, path: &str, dir: &Path, log: &dyn Fn(&str)) -> Result<PathBuf, String> {
    let out = dir.join(fx_svg::raster_file_name(path));
    if out.exists() {
        return Ok(out);
    }
    std::fs::create_dir_all(dir).map_err(|err| format!("{}: {err}", dir.display()))?;
    let argv = fx_svg::ffmpeg_raster(program, path, &out.display().to_string());
    let refs: Vec<&str> = argv.iter().map(String::as_str).skip(1).collect();
    // This box has no svg decoder (Alpine ffmpeg without librsvg), so a real .svg fails here; reporting that is the honest outcome.
    subprocess::run(program, &refs, |line| log(line))?;
    // Exit 0 without a file is a failure too: it would mark the file ready and paint nothing.
    if !out.exists() {
        return Err(format!(
            "{} produced no raster at {}",
            program,
            out.display()
        ));
    }
    Ok(out)
}
