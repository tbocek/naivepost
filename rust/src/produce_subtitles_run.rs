//! F5.4 §S4/S5 — what the render WRITES for the subtitles, kept out of `produce_exec` so that
//! module stays about the order of subprocesses. Three decisions live here and nowhere else:

use std::path::{Path, PathBuf};

use crate::produce_subtitles as subs;
use crate::project::{Produce, Subtitles};

/// Where each cue in the sheet came from, counted per clip by [`count_cue_source`] and read back by
/// §S4's sentence.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CueCounts {
    pub from_lines: usize,
    pub from_speech: usize,
}

/// S1's count, one clip at a time: `build` answers narration lines ONE per line it was given (a line with
/// no entry to read is dropped there), so exactly-one-cue-per-line is what a clip that had lines looks
/// like — and anything else the clip put on the sheet came from its own speech.
pub fn count_cue_source(counts: &mut CueCounts, cues: &[subs::Cue], lines: usize) {
    if cues.is_empty() {
        return;
    }
    if lines > 0 && cues.len() == lines {
        counts.from_lines += 1;
    } else {
        counts.from_speech += 1;
    }
}

/// §S4's own sentence: what the sheet holds, in the counts §F5.4's tree draws from.
pub fn cue_count_log(cues: usize, counts: &CueCounts) -> String {
    format!(
        ">>> subtitles: {cues} cues from {} spoken line(s) and {} clip(s) of their own speech.",
        counts.from_lines, counts.from_speech
    )
}

/// The rule, one place: burn-in is a choice about the PICTURE, and only one of §1's three answers
/// makes it.
pub fn burns(settings: &Produce) -> bool {
    settings.subtitles == Subtitles::BurnedIn
}

pub fn burn_file_name(index: usize, _stamp: &str, settings: &Produce) -> Option<String> {
    burns(settings).then(|| format!("c{index:03}.srt"))
}

/// Write one clip's burned sheet from its OWN cues, on its OWN seconds: the encode cuts that span
/// out of the recording and never sees the video's clock, so a cue list already lifted onto it
/// would letter every caption at the wrong moment.
pub fn write_burn_file(name: &str, cues: &[subs::Cue], scratch: &Path) -> Option<PathBuf> {
    if cues.is_empty() {
        return None;
    }
    let path = scratch.join(name);
    if let Err(why) = std::fs::write(&path, subs::srt_text(cues)) {
        eprintln!("produce: {}: {why}", path.display());
        return None;
    }
    Some(path)
}

/// §S5: the burned sheets, one entry per clip in plan order, written beside the plan from the
/// render's per-clip cue lists.
pub fn write_burn_files(
    per_clip: &[Vec<subs::Cue>],
    stamp: &str,
    settings: &Produce,
    scratch: &Path,
) -> Vec<Option<PathBuf>> {
    per_clip
        .iter()
        .enumerate()
        .map(
            |(index, cues)| match burn_file_name(index, stamp, settings) {
                Some(name) => write_burn_file(&name, cues, scratch),
                None => None,
            },
        )
        .collect()
}

/// §S4 (`stale sidecars beside the video deleted by exact name`): delete every file this run could
/// leave behind from an earlier one, before anything of this run is written.
pub fn clear_stale_sidecars(video: &Path, keep: &Path, stems: &[String]) -> usize {
    // The folder beside the finished video is the one §3's sidecars live in.
    let produce_dir = video.parent().map(Path::to_path_buf).unwrap_or_default();
    let mut gone = 0;
    // One call per track stem this run owns: `final` for the session's language, `final.de` for a translated
    // one. §3's lister answers both files of each.
    for stem in stems.iter() {
        // `final` answers `final.srt`/`final.vtt`; a translated stem answers its own pair. §3's
        // lister is called with no languages, because the STEMS already carry the language.
        for name in crate::produce_data::stale_files(stem, &[]) {
            let victim = produce_dir.join(&name);
            // The scratch sheet is this run's own output and gets rewritten after this.
            if victim != keep && victim.is_file() {
                let _ = std::fs::remove_file(&victim);
                gone += 1;
            }
        }
    }
    gone
}
