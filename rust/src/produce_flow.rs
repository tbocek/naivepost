//! F5.1 ▶ Produce (`spec/08-produce.md` F5.1) — the run's own decisions, with no window in them.
//!
//! One press of ▶ on the Produce page and six steps: refuse or not, ask to overwrite or not, take the
//! snapshot, open the run with one line saying what it is about to do, let the two halves (words F5.6 and
//! render F5.2) go in parallel, write the `<video>` tag page after both, and end with one of three words.
//! What can be decided ahead of any subprocess is here; nothing else.
//!
//! Like [`crate::narrate_run`] this opens no socket, spawns no ffmpeg and touches no widget. The words are
//! F5.6's call and the render is F5.2's, so both arrive through injected closures ([`run_with`]) and a
//! test asserts the ORDER and the sentences with no model and no encoder standing.
//!
//! What it deliberately does NOT own: the render itself ([`crate::produce_render`]), the upload text and
//! picture ([`crate::publish`] + F5.6), the tag page ([`crate::produce_embed`]), the stamp's contents
//! ([`crate::produce_stamp`]), and which settings make a file stale — that is the stamp's question and
//! this module only asks it.

use crate::cut::{self, Cut, NO_CUT_YET};
use crate::layout::Tree;
use crate::narration::Entry;
use crate::produce_embed;
use crate::produce_render as render;
use crate::project::{Codec, Container, Produce, Publish};
use crate::run::{self, Snapshot};

// --- S1: the refusal ---------------------------------------------------------------------------

/// S1: the first thing ▶ says when it will not run, or `None` when both gates stand open.
///
/// Busy comes first because a second run started during the first would overwrite the first one's
/// progress line — the person has to hear about the run they already have before they hear about the cut
/// they do not. The cut's absence is [`cut::NO_CUT_YET`] verbatim (§3's sentence, shared with the tabs'
/// own grey-out), never restated here, so the two doors cannot drift apart.
pub fn refuse(busy: bool, has_cut: bool) -> Option<&'static str> {
    if busy {
        return Some(run::PAUSING);
    }
    if !has_cut {
        return Some(NO_CUT_YET);
    }
    None
}

// --- S2: is the video already what this page describes? ----------------------------------------

/// What the gate between "up to date" and "encode" decides to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gate {
    /// S2: the stamp matches, so the encode is skipped and the skip is SAID rather than silent.
    Skip,
    /// S2: a video is there and it is stale, so overwriting it needs an answer first.
    ConfirmOverwrite,
    /// S2: nothing to overwrite — go straight to the snapshot.
    Encode,
}

/// S2's skip line, byte for byte. The parenthetical matters: without it a person who wants the encode
/// anyway has no idea that ↻ beside Transcode gives it to them.
pub const SKIP_LOG: &str = ">>> the video is already what this page describes \u{2014} not encoding it again \
(\u{21bb} beside Transcode encodes anyway)";

/// S2's confirmation title. `<base>` is the file's name without its folder, which is what a person
/// recognises; the full path goes in the body where it can be read.
pub fn overwrite_title(base: &str) -> String {
    format!("Overwrite {base}?")
}

/// S2's confirmation body: what is being replaced on one line, then a blank, then what the cost is.
/// Two paragraphs because §S2 spells them apart and a dialog that runs them together reads as one clause.
pub fn overwrite_detail(path: &str, size: &str, age: &str) -> String {
    format!("{path} \u{2014} {size}, {age}\n\nThe encode takes minutes and there is no undo for it.")
}

/// S2: which way the gate falls. Up-to-date means the stamp on disk equals the stamp of what this page
/// describes right now — that equality is [`produce_stamp::skip_encode`]'s question, asked by the caller
/// with the two hashes it already holds, so this adds no rule of its own. What it adds is the third case:
/// a matching stamp with no video behind it is not up to date, it is missing, and gets encoded like any
/// other absent file.
pub fn gate(stamp_matches: bool, video_exists: bool) -> Gate {
    // `stamp_matches` is `skip_encode(stored, current)` from the caller; the second half of the rule is
    // here: no file behind a matching stamp means missing, not up to date.
    if stamp_matches && video_exists {
        return Gate::Skip;
    }
    if video_exists {
        Gate::ConfirmOverwrite
    } else {
        Gate::Encode
    }
}

// --- S3: the snapshot taken on the GUI thread --------------------------------------------------

/// Everything the run works from, gathered once while the page still holds it.
///
/// A run works on this rather than asking the page again half-way through, because the page keeps being
/// editable while the render runs: the usual way a render ends up missing an angle is a flow that re-reads
/// the source list after the person scrolled away from it (prototype `snapSources`). Every field arrives
/// as an argument, so nothing here reaches into a widget at all — the caller gathers, this stores.
#[derive(Debug, Clone)]
pub struct Run {
    /// The cut this run renders — answered by [`what_is_the_cut`], so render, subtitles, brief and stamp
    /// all read the same one.
    pub cut: Cut,
    /// The narration lines, in the record's order.
    pub lines: Vec<Entry>,
    /// The encoder settings as they stood at the press.
    pub settings: Produce,
    /// The sources as they stood at the press (F0.x's snapshot shape, reused rather than re-derived).
    pub sources: Snapshot,
    /// The publish state, if the project has one.
    pub publish: Option<Publish>,
    /// The cut's aspect, which decides every clip's frame box.
    pub aspect: String,
    /// How many clips the render will place. Counted here so the opening line can say it before any work.
    pub clips: usize,
    /// Whether `publish.json` exists: the difference between "left as they are" and "written beside them".
    pub publish_written: bool,
    /// The voice in use, carried so the stamp question can be asked without going back to the page.
    pub voice: String,
    /// §1's narration flag.
    pub no_narration: bool,
}

impl Run {
    /// The file this run writes, spelled by the container rather than stored: the output path is a
    /// consequence of the container, not a setting (§F5.3).
    pub fn video_file(&self) -> String {
        format!("produce/final.{}", container_name(self.settings.container))
    }
}

/// S3: **one function answers "what is the cut"** for the render, the subtitles, the brief and the
/// stamp. The Cut page's own segments win when it holds any, so an unsaved tweak still renders — a person
/// who moved a cut and pressed ▶ means the cut they moved, not the one on disk. With nothing live the file
/// answers, and a project with neither gets an empty cut, which S1 has already refused.
///
/// Shared rather than duplicated because four readers asking the question separately is how a render and
/// its own subtitle file end up describing different cuts.
pub fn what_is_the_cut(page_cut: Option<Cut>, tree: &Tree) -> Cut {
    match page_cut {
        Some(cut) if !cut.segs.is_empty() => cut,
        _ => cut::load(tree).unwrap_or_default(),
    }
}

/// S3: gather the run from what the caller hands over. Nothing is read from a widget and nothing is
/// spawned: the caller is the GUI thread, and this is where its values become one owned snapshot.
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
    // The aspect travels with the cut (§A: the cut page's own aspect sets the frame's shape).
    let aspect = cut.aspect.clone();
    Run {
        clips: cut.segs.len(),
        publish_written: publish.is_some(),
        sources: run::snapshot_sources(project),
        no_narration: project.no_narration,
        cut,
        lines,
        settings,
        publish,
        aspect,
        voice,
    }
}

// --- S4: the one opening line ------------------------------------------------------------------

/// S4: the line the run opens with. Three shapes, differing only in their tail:
/// - no `publish.json`: nothing has been written yet, so the thumbnail and the upload text are left alone.
/// - `publish.json` and the picture is being redrawn: "\u2026 and the thumbnail redrawn beside them".
/// - `publish.json` and only the words move: "\u2026 and the upload text and thumbnail written beside them".
///
/// The middle clause is shared and spelled out (`N clips at <container>/<codec> crf N`) rather than
/// elided, because the log is read on its own: a line that says "producing" without saying what is being
/// produced is not an opening line.
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

/// S4: the words half failed. Logged, never fatal: the render carries on, because a picture that did not
/// come back is not a reason to throw away a finished video. `{reason}` is filled in by
/// [`words_failed`] because the spec's string names the reason as part of the sentence.
pub const WORDS_FAILED_TAIL: &str = "-- the render carries on";

/// S4: the failure line with the reason named. Naming it is the point — "the words failed" without the
/// reason sends a person to the top of the log to find what sd.cpp said.
pub fn words_failed(reason: &str) -> String {
    format!("!!! the upload text and thumbnail failed: {reason} {WORDS_FAILED_TAIL}")
}

/// §B's render-failure line, `produce FAILED: …`. It keeps the prototype's capitals because it is the one
/// line that says the *render* died rather than one half of it: "!!!" marks every logged failure, and
/// this is the one that means there is no video.
pub fn failed_log(reason: &str) -> String {
    format!("produce FAILED: {reason}")
}

/// S5/S6: what the render half reports back — the one struct the ending is read from, so a run's numbers
/// are assembled once and cannot disagree with themselves.
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



// --- S5/S6: how the run ends -------------------------------------------------------------------

/// How the two halves came back, as [`run_with`] collected them.
#[derive(Debug, Clone, PartialEq)]
pub struct Halves {
    /// F5.6's words half: `true` when it wrote what was asked.
    pub words_ok: bool,
    /// Why it did not, when it did not.
    pub words_reason: Option<String>,
    /// F5.2's render half: `true` when the render returned without error.
    pub render_ok: bool,
    /// F5.5's tag page: written or not. An input, never the verdict.
    pub tag_ok: bool,
    /// Whether somebody pressed ⏹ along the way.
    pub stopped: bool,
    /// Wall-clock seconds the run took, and the file's size, for the closing lines.
    pub seconds: f64,
    pub size: String,
}

/// The run's ending: the word on the status line and the line for the log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ending {
    pub status: &'static str,
    pub log: String,
    /// The run bar's progress text ([`progress_text`]). Empty for a stopped or failed ending: only a run
    /// that finished has something to show there.
    pub progress: String,
}

/// S6: **the render's error is the run's verdict**, and the tag page's is not — a helper page that could
/// not be written leaves a finished video and its subtitles on disk, which is exactly what
/// [`produce_embed::render_verdict`] says. So the tag's outcome is passed in and discarded on purpose.
///
/// S5 is the ordering rule this function exists to make visible: the tag page is written only after BOTH
/// halves have come back, which is why it is an input here rather than something done in between.
///
/// A stop outranks everything: nothing was left half-written that the next press cannot redo, so it is not
/// a failure and no stamp is written.
pub fn finish(halves: &Halves) -> Ending {
    if halves.stopped {
        return Ending {
            status: render::stopped_log(),
            log: render::stopped_log().to_string(),
            progress: String::new(),
        };
    }
    // The tag's failure changes nothing: render_verdict takes it and ignores it, which is the rule.
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
        // The file name comes from `ending_for`, which knows it; `finish` alone reports the word only.
        log: render::STAGE_DONE.to_string(),
        progress: String::new(),
    }
}

/// S6: the progress text, `produced <file> — X s, size`. Kept separate from the log line because the bar
/// shows this while the log keeps [`render::finished_log`]'s two-space form; both are derived from the
/// same numbers so they cannot disagree.
pub fn progress_text(file: &str, seconds: f64, size: &str) -> String {
    format!("produced {file} \u{2014} {seconds:.1} s, {size}")
}

/// S6 with the file named: the done line leads with the produced file, in [`render::finished_log`]'s own
/// spacing. A stop and a failure keep their single-word lines whatever the file is called.
pub fn ending_for(file: &str, halves: &Halves) -> Ending {
    let mut ending = finish(halves);
    if ending.status == render::STAGE_DONE {
        ending.log = render::finished_log(file, halves.seconds, &halves.size);
        ending.progress = progress_text(file, halves.seconds, &halves.size);
    }
    ending
}

/// Whether the run lays down its stamp: only an encode that returned without error writes one, so a press
/// that skipped the encode writes none and a failed run leaves the old stamp standing (F5.3).
pub fn stamp_written(encoded: bool, failed: bool) -> bool {
    render::stamp_written(encoded, failed)
}

/// S4/S5: run both halves and join them.
///
/// `words` performs F5.6 and reports `(ok, reason)`; `render_half` performs F5.2 and reports
/// `(ok, tag_ok, seconds, size)` — the render tells us its own elapsed time and file size because only
/// F5.2's encoder knows them. Both are closures so a headless test injects stubs and asserts the ORDER:
/// the opening line first, the two halves each once, the tag page only after both, and the closing
/// sentence last. A failing words half never stops the render — that is the whole reason the two are
/// separate calls rather than one function that could bail.
pub fn run_with<FWords, FRender>(run: &Run, mut words: FWords, mut render_half: FRender) -> Ending
where
    FWords: FnMut(&Run) -> (bool, Option<String>),
    FRender: FnMut(&Run) -> Rendered,
{
    let (words_ok, words_reason) = words(run);
    // The render starts regardless of what the words half did. Its own error is the verdict (S6); the
    // words' error is a logged line on its own progress.
    let made = render_half(run);
    // One `Halves`, so the done log line and the bar's progress text are read off the same numbers.
    let halves = Halves {
        words_ok,
        words_reason,
        render_ok: made.ok,
        tag_ok: made.tag_ok,
        stopped: made.stopped,
        seconds: made.seconds,
        size: made.size,
    };
    ending_for(&run.video_file(), &halves)
}

// --- names ------------------------------------------------------------------------------------

/// The container as the log spells it. Mirrors the page's private helper; kept here because the opening
/// line is a log string and must not reach into a UI module for its own wording.
fn container_name(c: Container) -> &'static str {
    match c {
        Container::Mp4 => "mp4",
        Container::Mkv => "mkv",
        Container::Webm => "webm",
    }
}

/// The codec as the log spells it. Same reason as [`container_name`].
fn codec_name(c: Codec) -> &'static str {
    match c {
        Codec::H264 => "h264",
        Codec::H265 => "h265",
        Codec::Vp9 => "vp9",
    }
}
