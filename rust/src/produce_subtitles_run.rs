//! F5.4 §S4/S5 — what the render WRITES for the subtitles, kept out of `produce_exec` so that module
//! stays about the order of subprocesses. Three decisions live here and nowhere else:
//!
//! - **The counts of the run's own sentence.** "How many cues, and where did they come from" can only be
//!   answered while each clip is being asked, so the counts are built in the same loop that builds the
//!   sheet and read from there afterwards: a sentence counted later would be counting a different thing.
//! - **Which clips get burned.** Only "burned in" letters captions on the picture: "track in file" puts
//!   them in a subtitle track, "none in the video" leaves them in the files beside it, and in both of those
//!   a burned copy would put the same words on the screen twice. §1's row says an .srt is written beside
//!   the video either way, so refusing to burn is not refusing to caption.
//! - **Which older files must go, and when there is nothing to write.**
//!
//! `produce_data::stale_files` answers the names this clears, so the delete list and §3's file list are
//! one list rather than two that can drift.

use std::path::{Path, PathBuf};

use crate::produce_subtitles as subs;
use crate::project::{Produce, Subtitles};

/// Where each cue in the sheet came from, counted per clip by [`count_cue_source`] and read back by
/// §S4's sentence. Held apart from the cue list itself because §S2's tidy folds and extends cues after
/// this is known: the counts describe the CLIPS, which is what the sentence says, not the final spans.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CueCounts {
    /// Clips whose cues were narration lines.
    pub from_lines: usize,
    /// Footage clips whose cues were their own speech.
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

/// §S4's own sentence: what the sheet holds, in the counts §F5.4's tree draws from. Spoken lines and
/// clips-of-own-speech are counted where the cues were built, so a track that surprises a reader still
/// matches the file beside the log line.
pub fn cue_count_log(cues: usize, counts: &CueCounts) -> String {
    format!(
        ">>> subtitles: {cues} cues from {} spoken line(s) and {} clip(s) of their own speech.",
        counts.from_lines, counts.from_speech
    )
}

/// The rule, one place: burn-in is a choice about the PICTURE, and only one of §1's three answers makes it.
pub fn burns(settings: &Produce) -> bool {
    settings.subtitles == Subtitles::BurnedIn
}

/// The burned sheet's NAME for one clip, or `None` where the page asked for anything but burn-in. The
/// encode command and the plan's record both go through here, so the filter graph can never name a file
/// this round did not decide to write. §B's `c%03d_<stamp>.srt`, the same stem the encode's own file
/// carries: a clip's picture and its captions sit beside each other and say which is which.
/// The burned sheet's NAME for one clip — `None` where the page asked for anything but burn-in.
///
/// The stamp is deliberately NOT in the name. The plan builds the encode commands in one pass and computes
/// the stamp in another, and a name carrying it would mean a filter graph naming a file nobody wrote — which
/// would fail the clip, since ffmpeg refuses a subtitle filter whose file is missing. `produce/clips/` is
/// emptied at the start of every run (§B), so these per-clip sheets cannot outlive the run that wrote them
/// and the stamp would only name what is already unique by folder. §B's `c%03d.srt` sits beside the encode's
/// own `c%03d_<stamp>.<container>` for the same clip.
pub fn burn_file_name(index: usize, _stamp: &str, settings: &Produce) -> Option<String> {
    burns(settings).then(|| format!("c{index:03}.srt"))
}

/// Write one clip's burned sheet from its OWN cues, on its OWN seconds: the encode cuts that span out of
/// the recording and never sees the video's clock, so a cue list already lifted onto it would letter every
/// caption at the wrong moment. `None`, and nothing written, where there is no name (no burn-in asked) or
/// nothing to say — §5 refuses an empty cue outright.
///
/// A sheet that cannot be written is not fatal and is not silent: the encode goes out WITHOUT the filter,
/// because a command naming a missing file would fail the whole clip over one caption. The reason goes to
/// stderr beside the other scratch-write failures.
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

/// §S5: the burned sheets, one entry per clip in plan order, written beside the plan from the render's
/// per-clip cue lists. Writing happens in one pass rather than while each command is built, because the
/// names are needed for the filter graph and the cues are only final once every clip has been asked.
pub fn write_burn_files(
    per_clip: &[Vec<subs::Cue>],
    stamp: &str,
    settings: &Produce,
    scratch: &Path,
) -> Vec<Option<PathBuf>> {
    per_clip
        .iter()
        .enumerate()
        .map(|(index, cues)| match burn_file_name(index, stamp, settings) {
            Some(name) => write_burn_file(&name, cues, scratch),
            None => None,
        })
        .collect()
}

/// §S4 (`stale sidecars beside the video deleted by exact name`): delete every file this run could leave
/// behind from an earlier one, before anything of this run is written. `stems` are the track stems this run
/// owns — the session's own language plus each ticked translation — and `produce_data::stale_files` turns
/// each into its `.srt`/`.vtt` pair, so the delete and §3's file list are one list. A language the page
/// unticked is not among them and its file is left alone: it describes a translation somebody asked for,
/// and this run was not told to drop it.
///
/// Runs whether or not there are cues: a cut that lost its narration since the last render must not keep a
/// subtitle file describing words that are no longer in the video. Returns how many files went.
/// §S4 (`stale sidecars beside the video deleted by exact name`): delete every file this run could leave
/// behind from an earlier one, before anything of this run is written. `stems` are the track stems this run
/// writes — `final` for the session's own language, `final.de` for a translated one — and §3's lister
/// answers both files of each, so the delete and the inventory cannot drift. A language the page unticked is
/// not among them and its file stays: it describes a translation somebody asked for, and this run was not
/// told to drop it.
///
/// Runs whether or not there are cues: a cut that lost its narration since the last render must not keep a
/// subtitle file describing words that are no longer in the video. Returns how many files went.
pub fn clear_stale_sidecars(video: &Path, keep: &Path, stems: &[String]) -> usize {
    // The folder beside the finished video is the one §3's sidecars live in.
    let produce_dir = video.parent().map(Path::to_path_buf).unwrap_or_default();
    let mut gone = 0;
    // One call per track stem this run owns: `final` for the session's language, `final.de` for a translated
    // one. §3's lister answers both files of each.
    for stem in stems.iter() {
        // `final` answers `final.srt`/`final.vtt`; a translated stem answers its own pair. §3's lister is
        // called with no languages, because the STEMS already carry the language.
        for name in crate::produce_data::stale_files(stem, &[]) {
            let victim = produce_dir.join(&name);
            // The scratch sheet is this run's own output and gets rewritten after this; only the copies
            // beside the finished video go first, so a stale sidecar never outlives the render that made it.
            if victim != keep && victim.is_file() {
                let _ = std::fs::remove_file(&victim);
                gone += 1;
            }
        }
    }
    gone
}
