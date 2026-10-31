//! F5.1 ▶ Produce (`spec/08-produce.md` F5.1) — the run's own decisions, with no window in them.
//!
//! Six steps: refuse or not, ask to overwrite or not, take the snapshot, open the run with one line, run
//! the two halves (words F5.6, render F5.2) in parallel, write the tag page after both, and end with one of
//! three words. No socket, ffmpeg or widget: both halves arrive as closures ([`run_with`]) so a test asserts
//! the ORDER and the sentences. Which settings make a file stale is the stamp's question ([`crate::produce_stamp`]).

use crate::cut::{self, Cut, NO_CUT_YET};
use crate::layout::Tree;
use crate::narration::Entry;
use crate::produce_embed;
use crate::produce_render as render;
use crate::produce_stamp;
use crate::project::{Codec, Container, Produce, Publish};
use crate::run::{self, Snapshot};

/// S1: the first thing ▶ says when it will not run, or `None` when both gates stand open.
///
/// Busy comes first: a second run would overwrite the first one's progress line. The cut's absence is
/// [`cut::NO_CUT_YET`] verbatim (shared with the tabs' grey-out).
pub fn refuse(busy: bool, has_cut: bool) -> Option<&'static str> {
    if busy {
        return Some(run::PAUSING);
    }
    if !has_cut {
        return Some(NO_CUT_YET);
    }
    None
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gate {
    Skip,
    ConfirmOverwrite,
    Encode,
}

/// S2's skip line, byte for byte. The parenthetical tells the person ↻ beside Transcode forces the encode.
pub const SKIP_LOG: &str =
    ">>> the video is already what this page describes \u{2014} not encoding it again \
(\u{21bb} beside Transcode encodes anyway)";

/// S2's confirmation title. `<base>` is the file's name without its folder; the full path goes in the body.
pub fn overwrite_title(base: &str) -> String {
    format!("Overwrite {base}?")
}

/// S2's confirmation body: what is replaced on one line, a blank, then the cost (two paragraphs, §S2).
pub fn overwrite_detail(path: &str, size: &str, age: &str) -> String {
    format!(
        "{path} \u{2014} {size}, {age}\n\nThe encode takes minutes and there is no undo for it."
    )
}

/// S2's confirmation: the title for the status line and the two-paragraph body for the dialog and the log.
///
/// The size is spelled as the render's own lines spell a weight (`MiB`); the age is said as an AGE ("3 minutes
/// old"), not a timestamp.
pub fn overwrite_asked(video: &std::path::Path) -> (String, String) {
    let base = video
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "final".to_string());
    let (bytes, mtime) = produce_stamp::disk_facts(video);
    (
        overwrite_title(&base),
        overwrite_detail(
            &video.to_string_lossy(),
            &human_weight(bytes),
            &age_of(mtime),
        ),
    )
}

/// A file's weight as the produce log spells it: whole MiB, one decimal below, so a short encode is never
/// "0 MiB". Same rule as [`crate::produce_exec`]'s done line.
fn human_weight(bytes: u64) -> String {
    const MIB: f64 = 1024.0 * 1024.0;
    let mib = bytes as f64 / MIB;
    if mib >= 10.0 || bytes.is_multiple_of(1024 * 1024) {
        format!("{} MiB", mib as u64)
    } else {
        format!("{mib:.1} MiB")
    }
}

/// How long ago an epoch second was, coarsely (seconds, minutes, hours, days).
fn age_of(mtime: u64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|span| span.as_secs())
        .unwrap_or(0);
    let ago = now.saturating_sub(mtime);
    let unit = |count: u64, name: &str| format!("{count} {name} old");
    if ago < 60 {
        unit(ago.max(1), if ago == 1 { "second" } else { "seconds" })
    } else if ago < 3600 {
        let m = ago / 60;
        unit(m, if m == 1 { "minute" } else { "minutes" })
    } else if ago < 86_400 {
        let h = ago / 3600;
        unit(h, if h == 1 { "hour" } else { "hours" })
    } else {
        let d = ago / 86_400;
        unit(d, if d == 1 { "day" } else { "days" })
    }
}

/// The response ids §S2's confirmation offers; a third spelling would be a silent new answer nobody handles.
pub const ANSWER_OVERWRITE: &str = "overwrite";
pub const ANSWER_CANCEL: &str = "cancel";

/// Does this answer mean "encode anyway"? Only the deliberate word: Cancel, × and any unknown id leave the
/// standing video alone.
pub fn overwrite_proceeds(answer: &str) -> bool {
    answer == ANSWER_OVERWRITE
}

/// S2: which way the gate falls. Up-to-date is [`produce_stamp::skip_encode`]'s question, asked by the
/// caller. The added case: a matching stamp with no video behind it is missing, and gets encoded.
pub fn gate(stamp_matches: bool, video_exists: bool) -> Gate {
    // `stamp_matches` is `skip_encode(stored, current)` from the caller.
    if stamp_matches && video_exists {
        return Gate::Skip;
    }
    if video_exists {
        Gate::ConfirmOverwrite
    } else {
        Gate::Encode
    }
}

/// Everything the run works from, gathered once while the page still holds it: the page stays editable
/// during the render, and re-reading the source list is how a render ends up missing an angle (prototype
/// `snapSources`). Every field arrives as an argument; nothing reaches into a widget.
#[derive(Debug, Clone)]
pub struct Run {
    /// The cut this run renders, from [`what_is_the_cut`].
    pub cut: Cut,
    pub lines: Vec<Entry>,
    pub settings: Produce,
    /// The sources as they stood at the press (F0.x's snapshot shape).
    pub sources: Snapshot,
    pub publish: Option<Publish>,
    pub aspect: String,
    /// How many clips the render will place, counted here so the opening line can say it.
    pub clips: usize,
    /// Whether `publish.json` exists: "left as they are" versus "written beside them".
    pub publish_written: bool,
    /// The voice in use, so the stamp question needs no page.
    pub voice: String,
}

impl Run {
    /// The file this run writes, spelled by the container (the output path is not a setting, §F5.3).
    pub fn video_file(&self) -> String {
        format!("produce/final.{}", container_name(self.settings.container))
    }
}

/// S3: **one function answers "what is the cut"** for the render, subtitles, brief and stamp. The Cut
/// page's own segments win when it holds any (an unsaved tweak still renders); else the file answers; a
/// project with neither gets an empty cut, which S1 has already refused.
///
/// Shared so a render and its own subtitle file cannot describe different cuts.
pub fn what_is_the_cut(page_cut: Option<Cut>, tree: &Tree) -> Cut {
    match page_cut {
        Some(cut) if !cut.segs.is_empty() => cut,
        _ => cut::load(tree).unwrap_or_default(),
    }
}

/// S3: gather the run from what the caller hands over (the GUI thread); nothing is read from a widget.
#[allow(clippy::too_many_arguments)]
pub fn snapshot(
    page_cut: Option<Cut>,
    tree: &Tree,
    lines: Vec<Entry>,
    settings: Produce,
    project: &crate::project::Project,
    publish: Option<Publish>,
    voice: String,
) -> Run {
    let cut = what_is_the_cut(page_cut, tree);
    // The aspect travels with the cut (§A).
    let aspect = cut.aspect.clone();
    Run {
        clips: cut.segs.len(),
        publish_written: publish.is_some(),
        sources: run::snapshot_sources(project),
        cut,
        lines,
        settings,
        publish,
        aspect,
        voice,
    }
}

/// S4: the line the run opens with. Three shapes, differing only in their tail:
/// - no `publish.json`: nothing has been written yet, so the thumbnail and the upload text are left alone.
/// - `publish.json` and the picture is being redrawn: "… and the thumbnail redrawn beside them".
/// - `publish.json` and only the words move: "… and the upload text and thumbnail written beside them".
///
/// The middle clause is spelled out (`N clips at <container>/<codec> crf N`): the log is read on its own.
pub fn opening_line(run: &Run, redraw_pic: bool) -> String {
    let middle = format!(
        "{} clips at {}/{} crf {}",
        run.clips,
        container_name(run.settings.container),
        codec_name(run.settings.codec),
        run.settings.crf
    );
    if !run.publish_written {
        return format!(
            ">>> transcoding {}: {middle} \u{2014} the thumbnail and the upload text are left as they are",
            run.video_file()
        );
    }
    if redraw_pic {
        return format!(
            ">>> producing {}: {middle} and the thumbnail redrawn beside them",
            run.video_file()
        );
    }
    format!(
        ">>> producing {}: {middle} and the upload text and thumbnail written beside them",
        run.video_file()
    )
}

/// S4: the words half failed. Logged, never fatal: a missing picture is no reason to throw away a finished
/// video. `{reason}` is filled in by [`words_failed`].
pub const WORDS_FAILED_TAIL: &str = "-- the render carries on";

/// S4: the failure line with the reason named, so nobody hunts the log for what sd.cpp said.
pub fn words_failed(reason: &str) -> String {
    format!("!!! the upload text and thumbnail failed: {reason} {WORDS_FAILED_TAIL}")
}

/// §B's render-failure line, `produce FAILED: …`: capitals kept because this is the one line that says there is no video.
pub fn failed_log(reason: &str) -> String {
    format!("produce FAILED: {reason}")
}

/// S5/S6: what the render half reports back; the ending is read from this one struct.
#[derive(Debug, Clone, PartialEq)]
pub struct Rendered {
    /// The render returned without error. This, not the tag page's outcome, is the run's verdict (S6).
    pub ok: bool,
    /// S5's tag page: written or not. An input to the ending, never the verdict.
    pub tag_ok: bool,
    /// S10: a checkpoint answered "stopped". A stop outranks a failure: nothing was left half-written.
    pub stopped: bool,
    /// Wall-clock seconds the render took, and the produced file's size, for the closing lines.
    pub seconds: f64,
    pub size: String,
}

impl Default for Rendered {
    fn default() -> Self {
        Rendered {
            ok: false,
            tag_ok: false,
            stopped: false,
            seconds: 0.0,
            size: "0 B".to_string(),
        }
    }
}

/// How the two halves came back, as [`run_with`] collected them.
#[derive(Debug, Clone, PartialEq)]
pub struct Halves {
    pub words_ok: bool,
    pub words_reason: Option<String>,
    pub render_ok: bool,
    pub tag_ok: bool,
    pub stopped: bool,
    /// Wall-clock seconds the run took, and the file's size, for the closing lines.
    pub seconds: f64,
    pub size: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ending {
    pub status: &'static str,
    pub log: String,
    /// The run bar's progress text ([`progress_text`]); empty unless the run finished.
    pub progress: String,
}

/// S6: **the render's error is the run's verdict**, not the tag page's: a helper page that could not be
/// written leaves a finished video on disk ([`produce_embed::render_verdict`]), so the tag outcome is
/// passed in and discarded on purpose.
///
/// S5: the tag page is written only after BOTH halves have come back, hence an input here.
///
/// A stop outranks everything and is not a failure; no stamp is written.
pub fn finish(halves: &Halves) -> Ending {
    if halves.stopped {
        return Ending {
            status: render::stopped_log(),
            log: render::stopped_log().to_string(),
            progress: String::new(),
        };
    }
    // The tag's failure changes nothing: render_verdict takes it and ignores it.
    let failed = produce_embed::render_verdict(!halves.render_ok, !halves.tag_ok);
    if failed {
        return Ending {
            status: render::failure_log(),
            log: render::failure_log().to_string(),
            progress: String::new(),
        };
    }
    Ending {
        status: render::STAGE_DONE,
        // The file name comes from `ending_for`; `finish` alone reports the word only.
        log: render::STAGE_DONE.to_string(),
        progress: String::new(),
    }
}

/// S6: the progress text, `produced <file> — X s, size`; the log keeps [`render::finished_log`]'s two-space form.
pub fn progress_text(file: &str, seconds: f64, size: &str) -> String {
    format!("produced {file} \u{2014} {size}, took {seconds:.1} s")
}

/// S6 with the file named: the done line leads with the produced file. A stop and a failure keep their single-word lines.
pub fn ending_for(file: &str, halves: &Halves) -> Ending {
    let mut ending = finish(halves);
    if ending.status == render::STAGE_DONE {
        ending.log = render::finished_log(file, halves.seconds, &halves.size);
        ending.progress = progress_text(file, halves.seconds, &halves.size);
    }
    ending
}

/// Whether the run lays down its stamp: only an encode that returned without error writes one (F5.3).
pub fn stamp_written(encoded: bool, failed: bool) -> bool {
    render::stamp_written(encoded, failed)
}

/// S4/S5: run both halves and join them.
///
/// `words` performs F5.6 and reports `(ok, reason)`; `render_half` performs F5.2 and reports
/// `(ok, tag_ok, seconds, size)`. Both are closures so a headless test injects stubs and asserts the ORDER:
/// opening line, the two halves once each, the tag page only after both, the closing sentence last. A
/// failing words half never stops the render.
pub fn run_with<FWords, FRender>(run: &Run, words: FWords, render_half: FRender) -> Ending
where
    FWords: FnMut(&Run) -> (bool, Option<String>),
    FRender: FnMut(&Run) -> Rendered,
{
    run_with_named(&run.video_file(), run, words, render_half)
}

/// [`run_with`] with the produced file spelled out, for a case that only wants the halves' ordering and
/// should not need a folder for the done line's path.
pub fn run_with_named<FWords, FRender>(
    file: &str,
    run: &Run,
    mut words: FWords,
    mut render_half: FRender,
) -> Ending
where
    FWords: FnMut(&Run) -> (bool, Option<String>),
    FRender: FnMut(&Run) -> Rendered,
{
    let (words_ok, words_reason) = words(run);
    // The render starts regardless of the words half; its own error is the verdict (S6).
    let made = render_half(run);
    // One `Halves`, so the done log line and the bar's progress text read the same numbers.
    let halves = Halves {
        words_ok,
        words_reason,
        render_ok: made.ok,
        tag_ok: made.tag_ok,
        stopped: made.stopped,
        seconds: made.seconds,
        size: made.size,
    };
    ending_for(file, &halves)
}

/// The container as the log spells it; kept here so the opening line need not reach into a UI module.
fn container_name(c: Container) -> &'static str {
    match c {
        Container::Mp4 => "mp4",
        Container::Mkv => "mkv",
        Container::Webm => "webm",
    }
}

/// The codec as the log spells it.
fn codec_name(c: Codec) -> &'static str {
    match c {
        Codec::H264 => "h264",
        Codec::H265 => "h265",
        Codec::Vp9 => "vp9",
    }
}
