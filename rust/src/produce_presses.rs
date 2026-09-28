//! F5.7 Page runs — the seven presses of §08's Produce page as **pure walks with injected legs**.
//!
//! `produce_runs` answers "what does this press cost"; this module answers "what does it *do*", in the
//! order the spec draws it. Both stay free of GTK, sockets and subprocesses so a headless test can drive a
//! press end to end: every external act arrives as a closure on [`Legs`], mirroring
//! [`crate::produce_upload::Materials`] (F5.6) and the spawner seam of [`crate::produce_exec`] (F5.2).
//!
//! The order inside each function is the same everywhere and is load-bearing:
//! **refuse → open → do → end.**
//! - *refuse* first, before any log line or leg runs: a press with no cut, no picture or no video says
//!   its one sentence and touches nothing (§F5.7's arrows all begin with their refusal).
//! - *open* second: the `>>>` line that announces the run, logged before work starts so an interrupted
//!   run still shows what it was about.
//! - *do*: the legs, in the spec's order. A redraw draws then prints; a re-word prints and never draws.
//! - *end* through [`runs::ending`], the ONE path §line 186 names, so both buttons report the same
//!   trouble in the same voice.
//!
//! The cost story this file exists to make assertable: a redraw is one sd.cpp call, a re-word is one LLM
//! call and **no** draw, Set Thumbnail is a print and no draw, and export / save / transcode ask a model
//! for nothing at all. Each of those is a count on the returned [`Outcome`], not a comment.

use crate::produce_runs as runs;
use crate::produce_upload::{self as upload, DrawnJob};

/// A leg that answers with words: which kind is being asked (`"texts"`/`"title"`), the words of that kind.
pub type WordsOf = Box<dyn Fn(&str) -> Vec<String>>;
/// sd.cpp submit: the request body in, the accepted job out.
pub type AskDraw = Box<dyn Fn(&serde_json::Value) -> Result<DrawnJob, String>>;
/// An argv-shaped subprocess: what [`crate::produce_tag_page::MakePoster`] takes, so one spawner serves both.
pub type RunArgv = Box<dyn Fn(&[String]) -> Result<(), String>>;
/// A file operation with no encoder in it — ⤓ Save video's copy, kept apart from `RunArgv` so a test cannot
/// read a copy as a transcode.
pub type CopyOut = Box<dyn Fn(&str) -> Result<(), String>>;

pub struct Legs {
    /// Is there a cut to draw from? The first question every press asks.
    pub has_cut: Box<dyn Fn() -> bool>,
    /// Has ▶ produced a video yet? ⤓ Save video's first question.
    pub has_video: Box<dyn Fn() -> bool>,
    /// Is there a thumbnail to export? ⤓ export's first question.
    pub has_picture: Box<dyn Fn() -> bool>,
    /// Whether a render is running right now. A copy taken mid-render is a short video that looks done.
    pub rendering: Box<dyn Fn() -> bool>,
    /// What to print: the marked texts of `kind` (`"texts"` / `"title"`, the two entries of
    /// [`upload::print_order`]). Empty means nothing of that kind, which is not an error.
    pub words_of: WordsOf,
    /// The edit instruction the picture is drawn from. Empty is refused rather than sent.
    pub instruction: Box<dyn Fn() -> String>,
    /// The reference images after the base, in order.
    pub references: Box<dyn Fn() -> Vec<String>>,
    /// The vertical band the title prints across, as the box's centre fraction — [`upload::Band::of`] reads
    /// it the same way for the draw prompt's "keep the <band> calm" tail.
    pub title_band: Box<dyn Fn() -> f64>,
    /// The brief a re-word sends. Built by the caller (`produce_upload::brief` over the live cut), because
    /// only the page holds the segments, the events and the narration lines together.
    pub brief: Box<dyn Fn() -> String>,
    /// sd.cpp submit: the request body in, the job out. One call per redraw, never two.
    pub ask_draw: AskDraw,
    /// One poll of a draw job, repeated until it reports done.
    pub poll: Box<upload::Poll>,
    /// Print one pass of words onto the picture. Costs a decode, never a GPU run.
    pub print: Box<upload::PrintWords>,
    /// The model leg: the brief in, the raw reply out. One call for a re-word, none anywhere else.
    pub ask_model: Box<upload::AskModel>,
    /// ffmpeg for ↻ Transcode. argv-shaped, like [`crate::produce_tag_page::MakePoster`].
    pub encode: RunArgv,
    /// Copy the rendered video out to a chosen path — a file operation with no encoder in it, kept apart
    /// from `encode` so a test cannot read a copy as a transcode.
    pub copy: CopyOut,
    /// The ladder's weight at each rung of [`runs::JPEG_QUALITIES`], best first. Data rather than a codec
    /// call, so the rung choice is a pure decision a test can pin without an encoder.
    pub jpeg_sizes: Box<dyn Fn() -> Vec<u64>>,
    /// Was the run stopped (⊘ Cancel) rather than did a leg fail? Decides *"stopped"* vs *"failed — see log"*.
    pub was_stopped: Box<dyn Fn() -> bool>,
    /// Whether this press counts its reprint passes onto [`Outcome::prints`]. The page sets it true; the
    /// F5.6 words half leaves it false, because that walk already reports its own passes and double-counting
    /// one print across two owners would make neither number mean anything.
    pub count_prints: bool,
}

/// What a press did. Every field is something a test pins about ORDER or COST, not just outcome.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Outcome {
    /// The acts, in the order they ran — `"draw"`, `"poll"`, `"model"`, `"print-texts"`, `"copy"`,
    /// `"encode"`. A press's cost story is this list's contents.
    pub calls: Vec<String>,
    /// sd.cpp calls. A redraw is exactly 1; a re-word, a set-from-image, an export and a transcode are 0.
    pub sd_calls: usize,
    /// LLM calls that rewrite text. A re-word is 1; a ticked redraw is 0.
    pub llm_calls: usize,
    /// Reprint passes made (the marked texts and the title are each one pass). Costs a decode, never a GPU run.
    pub prints: u32,
    /// The status line the page shows — the spec's own ending sentence.
    pub status: String,
    /// Why the press failed, if it did. A refusal is not an error unless it came from a leg.
    pub error: Option<String>,
    /// True when the press ended on its own refusal rather than on a leg failing: no cut, no picture, no
    /// video, no instruction. Those leave everything untouched and are not failures.
    pub refused: bool,
}

impl Outcome {
    /// A press that stopped at its own refusal: the sentence is the whole answer, and no leg ran.
    fn refusal(sentence: &str) -> Self {
        Outcome {
            status: sentence.to_string(),
            refused: true,
            ..Default::default()
        }
    }
}

// ---- ↻ over the thumbnail ---------------------------------------------------------

/// S1: ↻ over the thumbnail — redraw from the images and the instruction as they stand, always drawing.
/// Unticked adds the one LLM call that re-asks for the words; either way there is exactly **one** sd.cpp
/// call, because a second draw would be a second picture rather than a redraw of this one.
///
/// The draw/poll loop is deliberately NOT a copy of `produce_upload::walk`'s: that walk owns F5.6's order
/// (brief → publish → draw → print) and takes the model's answers as input. This press has no answers
/// unless the box is unticked, so it drives the same two legs directly instead of dragging the whole
/// words half along behind one button.
pub fn press_redraw_thumbnail(
    only_thumbnail: bool,
    legs: &Legs,
    mut log: impl FnMut(&str),
) -> Outcome {
    let mut out = Outcome::default();
    if !(legs.has_cut)() {
        return Outcome::refusal(runs::REDRAW_NO_CUT);
    }
    // Open before doing: the announcement lands even if the very first leg fails.
    log(runs::DRAW_AGAIN_LOG);

    // Unticked: the words are asked for again — one LLM call, and BEFORE the draw so the new instruction
    // is what gets painted. Ticked skips it entirely (`runs::redraw(true).rewrites == 0`).
    if !only_thumbnail {
        match (legs.ask_model)(&(legs.brief)()) {
            Ok(_) => {
                out.llm_calls = 1;
                out.calls.push("model".to_string());
            }
            Err(why) => return fail(out, "the thumbnail", &why, legs),
        }
    }

    // An empty instruction is refused rather than sent: sd.cpp would paint noise and charge for it.
    let instruction = (legs.instruction)();
    if instruction.trim().is_empty() {
        return Outcome::refusal(upload::NO_INSTRUCTION);
    }
    let body = upload::request_body(
        &upload::draw_prompt(&instruction, upload::Band::of((legs.title_band)())),
        None,
        runs::WIDTH,
        runs::HEIGHT,
        &(legs.references)(),
    );
    let job = match (legs.ask_draw)(&body) {
        Ok(job) => job,
        Err(why) => return fail(out, "the thumbnail", &why, legs),
    };
    out.sd_calls = 1;
    out.calls.push("draw".to_string());
    loop {
        match (legs.poll)(&job) {
            Ok(state) => {
                log(&upload::poll_line(&state.status, state.ahead));
                if state.done {
                    out.calls.push("poll".to_string());
                    break;
                }
            }
            Err(why) => return fail(out, "the thumbnail", &why, legs),
        }
    }

    // Then the words go back over the fresh picture — a reprint costs a decode, never a GPU run.
    if let Err(why) = print_words(legs, &mut out) {
        return fail(out, "the thumbnail", &why, legs);
    }
    out.status = runs::ending("the thumbnail", runs::REDRAW_DONE, true, false);
    out
}

// ---- ↻ beside Title ---------------------------------------------------------------

/// S2: ↻ beside Title — rewrite the title, the instruction and the description, and NOTHING else. In
/// particular no draw: the picture stays exactly as it is, which is what makes the press cheap and why
/// `ask_draw` must never be reached here.
pub fn press_reword(legs: &Legs, mut log: impl FnMut(&str)) -> Outcome {
    let mut out = Outcome::default();
    if !(legs.has_cut)() {
        return Outcome::refusal(runs::REWORD_NO_CUT);
    }
    log(runs::REWORD_LOG);
    match (legs.ask_model)(&(legs.brief)()) {
        Ok(_) => {
            out.llm_calls = 1;
            out.calls.push("model".to_string());
        }
        Err(why) => return fail(out, "the text", &why, legs),
    }
    // Success re-prints the words onto the picture that already exists (§line 186). Still no draw.
    if let Err(why) = print_words(legs, &mut out) {
        return fail(out, "the text", &why, legs);
    }
    out.status = runs::ending("the text", runs::REWORD_DONE, true, false);
    out
}

// ---- Set Thumbnail on an image ----------------------------------------------------

/// S3: Set Thumbnail on an image — use that picture as it is, cropped, and print the marked words over
/// it. No draw and no model call: the user's file *is* the picture.
pub fn press_set_thumbnail(file: &str, legs: &Legs, mut log: impl FnMut(&str)) -> Outcome {
    let mut out = Outcome::default();
    if !(legs.has_cut)() {
        // Even a chosen picture needs a cut: the crop is to the video's shape, and with no video there is
        // no shape to crop to. Same refusal the redraw gives, for the same missing frame.
        return Outcome::refusal(runs::REDRAW_NO_CUT);
    }
    if let Err(why) = print_words(legs, &mut out) {
        return fail(out, "the thumbnail", &why, legs);
    }
    let status = runs::taken_log(file);
    log(&status);
    out.status = status;
    out
}

// ---- ⤓ export ---------------------------------------------------------------------

/// S6: ⤓ export — the JPEG an uploader takes. The rung comes from [`runs::jpeg_rung`]: the first of
/// 92/85/75/60/40 whose weight fits 2 MiB, or the last attempt when none does ("never rescaled").
pub fn press_export(path: &str, legs: &Legs) -> Outcome {
    if !(legs.has_picture)() {
        return Outcome::refusal(runs::EXPORT_NO_PICTURE);
    }
    let sizes = (legs.jpeg_sizes)();
    let Some(rung) = runs::jpeg_rung(&sizes) else {
        return Outcome::refusal(runs::EXPORT_NO_PICTURE);
    };
    Outcome {
        calls: vec![format!("export-rung{}", runs::JPEG_QUALITIES[rung])],
        status: runs::exported_log(path, sizes[rung]),
        ..Default::default()
    }
}

// ---- ⤓ Save video -----------------------------------------------------------------

/// S7: ⤓ Save video — a copy out to a chosen path, so `produce/final` keeps its place as the stamped
/// output. Announced first: copying a whole video is slow enough that silence reads as a hung button.
pub fn press_save_video(file: &str, bytes: u64, legs: &Legs, mut log: impl FnMut(&str)) -> Outcome {
    let mut out = Outcome::default();
    if !(legs.has_video)() {
        return Outcome::refusal(runs::SAVE_NO_VIDEO);
    }
    // A render in flight means the source is still being written: refuse rather than copy a half file out.
    if let Err(reason) = runs::copy_plan(file, file, (legs.rendering)(), bytes) {
        let mut refused = Outcome::refusal(&reason);
        refused.error = Some(reason);
        return refused;
    }
    log(&runs::saving_log(file));
    if let Err(why) = (legs.copy)(file) {
        return fail(out, "the copy", &why, legs);
    }
    out.calls.push("copy".to_string());
    out.status = runs::saved_log(file, bytes);
    out
}

// ---- ↻ Transcode ------------------------------------------------------------------

/// S8: ↻ Transcode — encode again with the row's own settings and **no model call at all**. That absence
/// is the point of the press: `ask_model` and `ask_draw` are never touched here, only `encode`.
pub fn press_transcode(
    source: &str,
    out_path: &str,
    settings: &crate::project::Produce,
    legs: &Legs,
    mut log: impl FnMut(&str),
) -> Outcome {
    let mut out = Outcome::default();
    let args = runs::transcode_args(source, out_path, settings);
    if let Err(reason) = runs::transcode_plan(source, &args, (legs.rendering)()) {
        let mut refused = Outcome::refusal(&reason);
        refused.error = Some(reason);
        return refused;
    }
    log(&runs::transcode_started(source, settings));
    if let Err(why) = (legs.encode)(&args) {
        return fail(out, "the transcode", &why, legs);
    }
    out.calls.push("encode".to_string());
    let done = runs::transcode_finished(out_path);
    log(&done);
    out.status = done;
    out
}

// ---- the shared pieces ------------------------------------------------------------

/// Print the marked texts, then the title, in `print_order()`'s order. An empty kind prints no pass at all,
/// so a picture with no marks costs nothing and claims nothing. The page counts its own passes through
/// `count_prints`, which is what lets a test tell "no words to print" from "the print leg never ran".
fn print_words(legs: &Legs, out: &mut Outcome) -> Result<(), String> {
    for kind in upload::print_order() {
        let words = (legs.words_of)(kind);
        if words.is_empty() {
            continue;
        }
        if legs.count_prints {
            out.prints += 1;
        }
        (legs.print)(kind, &words)?;
        out.calls.push(format!("print-{kind}"));
    }
    Ok(())
}

/// Turn a leg failure into the ending §line 186 prescribes, keeping the calls made so far visible.
fn fail(mut out: Outcome, what: &str, why: &str, legs: &Legs) -> Outcome {
    let stopped = (legs.was_stopped)();
    out.error = Some(why.to_string());
    out.status = runs::ending(what, "", false, stopped);
    out
}
