//! F5.7 Page runs — the seven presses of §08's Produce page as **pure walks with injected legs**.

use crate::produce_runs as runs;
use crate::produce_upload::{self as upload};

/// A leg that answers with words: which kind is being asked (`"texts"`/`"title"`), the words of that kind.
pub type WordsOf = Box<dyn Fn(&str) -> Vec<String>>;
/// A leg that changes something on the page and on disk and may refuse: §F5.7's "clears `own`" is the one.
pub type ClearOwn = Box<dyn Fn() -> Result<(), String>>;
/// An argv-shaped subprocess: what [`crate::produce_tag_page::MakePoster`] takes, so one spawner serves both.
pub type RunArgv = Box<dyn Fn(&[String]) -> Result<(), String>>;
pub type CopyOut = Box<dyn Fn(&str) -> Result<(), String>>;
/// The export ladder: given where the JPEG is to be written, the weight of each rung's encode, best first.
pub type JpegLadder = dyn Fn(&str) -> Vec<u64>;

pub struct Legs {
    /// Is there a cut to draw from? The first question every press asks.
    pub has_cut: Box<dyn Fn() -> bool>,
    /// Has ▶ produced a video yet? ⤓ Save video's first question.
    pub has_video: Box<dyn Fn() -> bool>,
    /// Is there a thumbnail to export? ⤓ export's first question.
    pub has_picture: Box<dyn Fn() -> bool>,
    /// Whether a render is running right now. A copy taken mid-render is a short video that looks done.
    pub rendering: Box<dyn Fn() -> bool>,
    pub words_of: WordsOf,
    /// The edit instruction the picture is drawn from. Empty is refused rather than sent.
    pub instruction: Box<dyn Fn() -> String>,
    /// Print one pass of words onto the picture. Costs a decode, never a GPU run.
    pub print: Box<upload::PrintWords>,
    /// ffmpeg for ↻ Transcode. argv-shaped, like [`crate::produce_tag_page::MakePoster`].
    pub encode: RunArgv,
    pub copy: CopyOut,
    pub jpeg_sizes: Box<JpegLadder>,
    /// Was the run stopped (⊘ Cancel) rather than did a leg fail? Decides *"stopped"* vs *"failed — see log"*.
    pub was_stopped: Box<dyn Fn() -> bool>,
    /// §F5.7's "clears `own`": a finished ↻ hands the picture back to the model.
    pub clear_own: ClearOwn,
    pub count_prints: bool,
}

/// What a press did. Every field is something a test pins about ORDER or COST, not just outcome.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Outcome {
    pub calls: Vec<String>,
    /// sd.cpp calls. A redraw is exactly 1; a re-word, a set-from-image, an export and a transcode are 0.
    pub sd_calls: usize,
    /// LLM calls that rewrite text. A re-word is 1.
    pub llm_calls: usize,
    /// Reprint passes made (the marked texts and the title are each one pass). Costs a decode, never a GPU run.
    pub prints: u32,
    /// The status line the page shows — the spec's own ending sentence.
    pub status: String,
    /// Why the press failed, if it did. A refusal is not an error unless it came from a leg.
    pub error: Option<String>,
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

pub fn press_redraw_thumbnail(
    legs: &Legs,
    draw: impl FnOnce() -> Result<(), String>,
    mut log: impl FnMut(&str),
) -> Outcome {
    let mut out = Outcome::default();
    if !(legs.has_cut)() {
        return Outcome::refusal(runs::REDRAW_NO_CUT);
    }
    if (legs.instruction)().trim().is_empty() {
        return Outcome::refusal(upload::NO_INSTRUCTION);
    }
    log(runs::DRAW_AGAIN_LOG);
    if let Err(why) = draw() {
        return fail(out, "the thumbnail", &why, legs);
    }
    out.sd_calls = 1;
    out.calls.push("draw".to_string());
    if let Err(why) = (legs.clear_own)() {
        return fail(out, "the thumbnail", &why, legs);
    }
    out.calls.push("clear-own".to_string());
    out.status = runs::ending("the thumbnail", runs::REDRAW_DONE, true, false);
    out
}

pub fn press_reword(
    legs: &Legs,
    words: impl FnOnce() -> Result<(), String>,
    mut log: impl FnMut(&str),
) -> Outcome {
    let mut out = Outcome::default();
    if !(legs.has_cut)() {
        return Outcome::refusal(runs::REWORD_NO_CUT);
    }
    log(runs::REWORD_LOG);
    out.llm_calls = 1;
    if let Err(why) = words() {
        return fail(out, "the text", &why, legs);
    }
    out.calls.push("model".to_string());
    out.status = runs::ending("the text", runs::REWORD_DONE, true, false);
    out
}

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

pub fn press_export(path: &str, legs: &Legs) -> Outcome {
    if !(legs.has_picture)() {
        return Outcome::refusal(runs::EXPORT_NO_PICTURE);
    }
    let sizes = (legs.jpeg_sizes)(path);
    // No rung answered with a number — every encode failed, so the encoder left no file to point an uploader
    // at. That is the same "nothing to export" as no picture. (`jpeg_rung` keeps its old answer for an empty
    // list — an index into the rungs — because its own checks pin that rule; the press answers with the
    // refusal instead of exporting nothing.)
    let Some(rung) = runs::jpeg_rung(&sizes).filter(|_| !sizes.is_empty()) else {
        return Outcome::refusal(runs::EXPORT_NO_PICTURE);
    };
    // Read back what the encoder left: the rung's own number and the file's length must be one fact, so the
    // line quotes the file. A missing file is a failed export, not a zero-byte success.
    let measured = match std::fs::metadata(path) {
        Ok(meta) => meta.len(),
        Err(why) => {
            let mut refused = Outcome::refusal(runs::EXPORT_NO_PICTURE);
            refused.error = Some(format!("{path}: {why}"));
            return refused;
        }
    };
    // §F5.7 writes the last attempt "even if it still doesn't fit", so an over-cap file is NOT thrown away —
    // it stays on disk and the status still reports it. What the cap does buy here is a reason worth opening
    // the log for: `jpeg_target` names the three rungs it insisted on, and the page's `report` prints it as
    // the `!!!` line. The picture landed; the uploader may refuse it, and that is what the line is for.
    let overshoot = runs::jpeg_target(measured).err();
    Outcome {
        calls: vec![format!("export-rung{}", runs::JPEG_QUALITIES[rung])],
        status: runs::exported_log(path, measured),
        error: overshoot,
        ..Default::default()
    }
}

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
