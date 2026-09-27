//! F3.5 SVG drawing by hand — `spec/06-effects.md` F3.5, steps S1–S5, and `spec/inventory/effects.md` §A.5.
//!
//! ❝ A drawing is a text with ink instead of words (§A.5's opening line), so almost everything it does belongs to
//! [`crate::fx_text`] already: the same box on the output frame, the same 0.3 s fades and 0.3 s floor, the same curve
//! that only has one shape, the same `clampFades` when two overrunning fades have to share a band. What differs is
//! the order of the questions — ▨ SVG asks for the file *first*, before it arms, because arming someone to place a
//! drawing they have not chosen yet puts the box down for nothing — and where an untouched drawing lands: the middle
//! rather than the middle, since a drawing is as often the subject as a decoration and the middle is the one
//! place that is not a guess about which. The widget layer holds no rule: `rust/src/ui/window.rs` routes the
//! dropdown's ▨ SVG row through `press_svg_item` (→ [`press`], which opens the chooser or refuses and records
//! nothing), answers the dialog in `svg_chosen` (→ [`chose`], which arms and never toggles off), dispatches the
//! preview's single left-button drag by which entry is armed (→ [`place`], via `svg_drag_ended_with_source`),
//! draws the "SVG at m:ss" fields from this module's own lists (`show_svg_form`), and applies them in
//! `press_svg_apply` (→ [`apply`] + `record_edit`, so one ↶ takes the drawing back).

use std::collections::HashMap;
use std::path::Path;

use crate::cut::Fx;
use crate::cut_speed;
use crate::fx_text::{self, Box_, BoxChoice};
use crate::fx_zoom;
use crate::layout;
use crate::tools;

// --- S1: press ▨ SVG -----------------------------------------------------------------------------------------------

/// F3.5 S1 (`no line? → "click a track first — the drawing needs a moment to appear at"`): its own wording, in the
/// family of per-button refusals beside [`fx_zoom::NO_LINE`] ("the effect needs a moment to happen at"),
/// [`crate::cut_copy::NO_LINE_YET`] (⧉ Paste) and [`crate::cut_insert::NO_LANE_LINE_YET`] (⇲ Lane). Four buttons
/// asking for the same missing thing in the words of what each one does — so this is deliberately not an import of
/// zoom's sentence, and a test keeps them apart.
pub const NO_LINE: &str = "click a track first \u{2014} the drawing needs a moment to appear at";

/// F3.5 S1 (`"Choose a drawing to lay over the video"`): the chooser's title. Spoken before the arm, because the file
/// is what the arm sentence names.
pub const CHOOSE_TITLE: &str = "Choose a drawing to lay over the video";

/// F3.5 S1 (§A.5 `filter svg`): the chooser's filter name; its only suffix is `svg`. The prototype passes exactly
/// this pair (`extFilter("SVG drawing", "svg")`).
pub const CHOOSE_FILTER: &str = "SVG drawing";

/// F3.5 S1 (§A.5 `assets folder`): the folder the chooser opens at — root-level cards shared by every project, which
/// is [`layout::assets_dir`]'s rule and [`crate::cut_insert::chooser_dir`]'s precedent, not a second one here.
pub fn chooser_dir(root: &Path) -> std::path::PathBuf {
    layout::assets_dir(root)
}

/// F3.5 S1: what pressing ▨ SVG did. Unlike ⊕ Zoom's [`fx_zoom::Press`] there is no `Armed`: the press cannot arm
/// anything yet, because the drawing has not been picked — it opens a dialog or refuses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Press {
    OpenChooser,
    Refused(&'static str),
}

/// F3.5 S1 (`a line?`): the red line is asked for *before* the file, so nobody spends a chooser trip on a second that
/// is not there. The refusal names what the drawing needs the line for — a moment to appear at.
pub fn press(line: Option<f64>) -> Press {
    match line {
        None => Press::Refused(NO_LINE),
        Some(_) => Press::OpenChooser,
    }
}

// --- S2: the file first, then the arm ------------------------------------------------------------------------------

/// F3.5 S2 (`a file chosen? → no → "choose a drawing and it goes on the picture"`): cancelling the chooser places
/// nothing. There is no default drawing to fall back on and no point arming a gesture with no ink.
pub const NO_FILE: &str = "choose a drawing and it goes on the picture";

/// F3.5 S2: what the chooser answered. `Chose::Armed` carries the file because the arm sentence prints it, and
/// because the record's `src` is that same string — one value from the dialog to the file on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Chose {
    Armed(String),
    Refused(&'static str),
}

/// F3.5 S2 (`asks for the file FIRST … then arms`): a file back arms the gesture, and never toggles it off — the
/// prototype clears its arm flag on this path with the comment "never a toggle-off: a file was just chosen on
/// purpose", which is what keeps a second ▨ SVG press from quietly disarming the drawing someone just picked. So
/// this takes no `already_armed`, where [`fx_zoom::arm`] and [`crate::fx_text::arm`] both do.
pub fn chose(file: Option<&str>) -> Chose {
    match file.map(str::trim).filter(|name| !name.is_empty()) {
        Some(name) => Chose::Armed(name.to_string()),
        None => Chose::Refused(NO_FILE),
    }
}

/// F3.5 S2 (`arms ("Drag the box <file> goes in — anywhere on the picture, any shape; the drawing keeps its own
/// shape inside it. A click puts one across the middle.")`): the gesture's own sentence with the file named in it,
/// so the arm says what is about to be drawn and not merely that something is. One constant with a hole for the file,
/// so a test compares the sentence rather than hoping three `contains` cover it. The semicolon, not a full stop,
/// before "the drawing keeps its own shape" is §A.5's punctuation.
pub const ARM_WORDS: &str = "Drag the box {} goes in \u{2014} anywhere on the picture, any shape; the drawing keeps \
                             its own shape inside it. A click puts one across the middle. ";

/// F3.5 S2: the arm's whole paragraph — this flow's gesture with the file named, then the shared tail saying where it
/// starts and for how long. The tail is [`fx_zoom::arm_tail`]'s, as in F3.4: both flows say the same thing about the
/// red line and the marked stretch, and two copies of a sentence are two sentences to keep true.
pub fn arm_words(file: &str, marked: Option<(f64, f64)>) -> String {
    format!("{}{}", arm_head(file), fx_zoom::arm_tail(marked))
}

/// F3.5 S2: [`ARM_WORDS`] with the file filled in — the first half of [`arm_words`], exposed so a test can hold the
/// sentence up against §A.5 without also asserting the tail's seconds.
pub fn arm_head(file: &str) -> String {
    ARM_WORDS.replace("{}", &format!("{file}"))
}

// --- S3: where it lands -------------------------------------------------------------------------------------------

/// F3.5 S3 (§A.5 `default box centred {0.5,0.5,0.6,0.6}`; §1 `svg: middle`): a drawing placed without a box drawn
/// goes in the middle of the frame, a bit over half of it. The middle rather than [`fx_text::LOWER_THIRD`] because a
/// drawing is as often the subject as a decoration, and the lower third is a guess about which one it is.
/// [`crate::cut::Fx::centre`] already answers `{0.5, 0.5}` for an svg that stored nothing; this is the same centre
/// with its size spelled, so a drawing placed by click and one read back from a file cannot diverge.
pub const MIDDLE: Box_ = Box_ { cx: 0.5, cy: 0.5, wf: 0.6, hf: 0.6 };

/// F3.5 S3 (`Camera layer stays up`): §A.5 says a drawing is the same as text, and text's box is measured against the
/// output frame — so the framing holds its place while a drawing is placed, exactly as in [`fx_text`] and for the
/// same reason. The opposite of ⊕ Zoom's [`fx_zoom::whole_source_shown`], whose box is on the source frame.
pub fn camera_layer_stays_up() -> bool {
    fx_text::camera_layer_stays_up()
}

// --- S4: the form --------------------------------------------------------------------------------------------------

/// F3.5 S4 (`Form "SVG at m:ss"`): titled by the second it belongs to, through [`tools::mm_ss`] — the app's one
/// spelling of a second, so a form and a lane bar never disagree about which moment they are talking about.
pub fn form_title(t: f64) -> String {
    format!("SVG at {}", tools::mm_ss(t))
}

/// F3.5 S4 (`file name + Choose…, Length, fades, Curve`): the fields in the order the form reads them — the file
/// first because it is what a drawing is made of, and because it can be changed here after the arm took one. Order
/// is the sentence a person reads, not a layout detail, as in [`fx_zoom::FORM_FIELDS`] and
/// [`crate::fx_text::FORM_FIELDS`].
pub const FORM_FIELDS: [&str; 5] =
    ["File", "Length (s)", "Fade in (s)", "Fade out (s)", "Curve"];

/// F3.5 S4 (`+ Choose…`): the button beside the file's name, which re-opens [`CHOOSE_TITLE`]'s dialog. A drawing is
/// the only effect whose form can change what it is made of, so its first row is a picker rather than a label.
pub const CHOOSE_BUTTON: &str = "Choose\u{2026}";

/// F3.5 S4: what the form holds when Apply is pressed — the seconds and fades as typed, the curve's display name, the
/// file (possibly re-chosen here), and the box the gesture left.
#[derive(Debug, Clone, PartialEq)]
pub struct Form {
    pub t: f64,
    pub dur: f64,
    pub trans: f64,
    pub tout: f64,
    /// The curve's display name; [`fx_zoom::curve_stored`] decides what gets written.
    pub curve: String,
    pub file: String,
    pub on: Box_,
}

/// F3.5 S4 (`No file → not placed`): the record the form writes back. An empty `src` is refused with §F3.5's own
/// words — a drawing with no ink cannot be drawn, and placing one would put a bar on the lane that draws nothing and
/// says nothing about why. Empty is judged on `trim()`, so a space typed into the file row is the same answer as
/// leaving it alone.
///
/// Everything else is [`fx_text::apply`]'s rule, not a second version of it: the 0.3 s floor is §6's "text/svg 0.3"
/// ([`fx_text::MIN_SECONDS`]), the fades are [`fx_text::FADE_SECONDS`] shared out by [`cut_speed::clamp_fades`], the
/// curve goes through [`fx_zoom::curve_stored`] so Linear writes no `ease` key, and the box is clamped on the way in
/// so no later step has to guard against a hand-edited file. The words field stays empty: an svg's content is its
/// file, and §1 keeps that in `src` precisely so a caption that looks like a path cannot be mistaken for one.
pub fn apply(form: &Form) -> Result<Fx, String> {
    if form.file.trim().is_empty() {
        return Err(NO_FILE.to_string());
    }
    if form.dur < fx_text::MIN_SECONDS {
        return Err(format!(
            "a drawing on screen for {:.2} s is shorter than the shortest one this form takes ({}) s",
            form.dur, fx_text::MIN_SECONDS
        ));
    }
    let (trans, tout) = cut_speed::clamp_fades(form.trans, form.tout, form.dur);
    let (cx, cy, hf, wf) = form.on.clamp().stored();
    Ok(Fx {
        kind: "svg".into(),
        t: form.t,
        dur: form.dur,
        trans,
        tout,
        cx: Some(cx),
        cy: Some(cy),
        hf: Some(hf),
        wf: Some(wf),
        src: form.file.trim().to_string(),
        ease: fx_zoom::curve_stored(&form.curve).to_string(),
        ..Default::default()
    })
}

/// F3.5 S2/S4 (`Drag or click (default box: the middle); t/dur from the line (3 s) or selection; fades 0.3`): what
/// the gesture left, before the form has been touched. The seconds follow [`fx_text::place`]'s rule exactly — a
/// marked stretch is the length, else 3 s from the red line ([`fx_zoom::DEFAULT_SECONDS`],
/// P.policy.effectDefaultSeconds) — and so does the box: a drag under [`fx_zoom::DRAG_MIN_PX`] is a press that did
/// not travel, which for a drawing takes the middle instead of the lower third. With neither a selection nor a line
/// there is no second to place it at, so nothing is placed; and with no file either ([`NO_FILE`], checked by the
/// caller before arming) there would be nothing to place *with*.
pub fn place(
    choice: BoxChoice,
    file: &str,
    line: Option<f64>,
    selection: Option<(f64, f64)>,
    frame: (f64, f64),
) -> Option<Fx> {
    let on = match choice {
        BoxChoice::Click => MIDDLE,
        // A press that did not travel is a click wherever it landed.
        BoxChoice::Dragged { w, h, .. } if w < fx_zoom::DRAG_MIN_PX || h < fx_zoom::DRAG_MIN_PX => MIDDLE,
        BoxChoice::Dragged { x, y, w, h } => Box_::from_px(x, y, w, h, frame.0, frame.1).clamp(),
    };
    let (t, dur) = match (selection, line) {
        (Some((from, to)), _) => (from.min(to), (to - from).abs()),
        (None, Some(at)) => (at, fx_zoom::DEFAULT_SECONDS),
        (None, None) => return None,
    };
    apply(&Form {
        t,
        dur,
        trans: fx_text::FADE_SECONDS,
        tout: fx_text::FADE_SECONDS,
        curve: fx_zoom::CURVE_CHOICES[0].to_string(),
        file: file.to_string(),
        on,
    })
    .ok()
}

/// F3.5 S4 (`file name`, and the arm's `<file>`): a drawing's file as something short enough to put on a label. The
/// last `/`-separated component kept WHOLE — a card's `?S=Dust II` is part of which card it is, so stripping the
/// query the way [`crate::cut_insert::kind`] has to would rename the drawing. Empty or blank answers `(no file)`,
/// which is what an effect with its `src` cleared shows on the lane.
pub fn base(path: &str) -> String {
    if path.trim().is_empty() {
        return "(no file)".to_string();
    }
    path.rsplit('/').next().unwrap_or(path).to_string()
}

// --- S5: the preview's raster --------------------------------------------------------------------------------------

/// F3.5 S5 (`Preview raster via ffmpeg, 512 px`): how big the preview's copy of a drawing is rendered — bigger than
/// any box on a preview widget, so the drawing is never blown up on screen, and small enough that keeping one costs
/// nothing. §10 lists this as an implicit constant (`svgPreviewPx`) with no `P.` row, so the id takes the bare prefix
/// of the rule that reads it — `// effects.previewRasterPx` — as [`crate::fx_aspect::HOLD_SECONDS`] and
/// [`crate::cut_speed::DEFAULT_RATE`] do. Catalogued once, in `params::cut()`.
pub const PREVIEW_RASTER_PX: f64 = 512.0;

/// F3.5 S5 (`Preview raster via ffmpeg … transparent`): how ffmpeg is asked for a drawing's preview — an argument
/// vector, no subprocess, so the shape of the call is testable and the running of it stays with
/// [`crate::subprocess`], following [`crate::cut_insert::ffmpeg_frames`]'s precedent.
///
/// Two things a drawing needs that a frame of footage does not: the librsvg input options that set the size the
/// vector is rendered at (`-width`/`-height`, plus `-keep_ar 1` so the aspect survives), and an output kept in rgba,
/// so a transparent background stays transparent instead of coming out as a black card over the video.
pub fn ffmpeg_raster(ffmpeg: &str, path: &str, out: &str) -> Vec<String> {
    let px = PREVIEW_RASTER_PX as u32;
    vec![
        ffmpeg.to_string(),
        "-hide_banner".into(),
        "-loglevel".into(),
        "error".into(),
        "-width".into(),
        px.to_string(),
        "-height".into(),
        px.to_string(),
        "-keep_ar".into(),
        "1".into(),
        "-i".into(),
        path.to_string(),
        "-pix_fmt".into(),
        "rgba".into(),
        "-f".into(),
        "image2".into(),
        "-c:v".into(),
        "png".into(),
        out.to_string(),
    ]
}

/// F3.5 S5 (`the drawing keeps its own shape inside it`): the rectangle a drawing is drawn into — as large as it can
/// be inside the box, aspect kept, centred on the axis the fit left short. Returns (x, y, w, h) in the same units it
/// was given.
///
/// The one place either side works this geometry out, which is what lets the preview and the render put a drawing in
/// the same place — [`fx_text::preview_is_the_render`]'s bargain, restated for ink: an unknown aspect (`<= 0`, or a
/// box with no size) returns the box itself rather than a zero-sized draw for a drawer to guard against.
pub fn fit_inside(box_w: f64, box_h: f64, aspect: f64) -> (f64, f64, f64, f64) {
    if box_w <= 0.0 || box_h <= 0.0 || aspect <= 0.0 {
        return (0.0, 0.0, box_w.max(0.0), box_h.max(0.0));
    }
    let w = box_w.min(box_h * aspect);
    let h = w / aspect;
    ((box_w - w) / 2.0, (box_h - h) / 2.0, w, h)
}

// --- S5: one raster per file, one log line per failure ---------------------------------------------------------------

/// F3.5 S5: what asking about a file's preview copy answers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    /// Rastered and on screen; nothing to do.
    Ready,
    /// A raster for this file is already being made — the same logo placed six times asks once.
    Busy,
    /// This file failed to raster, was said once, and is left alone from here on.
    Failed,
    /// Nothing known about it: make one.
    NeedsRaster,
}

/// One drawing's preview copy, and how far getting it got.
#[derive(Debug, Default)]
struct Entry {
    ready: bool,
    busy: bool,
    failed: bool,
    /// Whether this file's failure has already been printed. §A.5's "failure logged once": a drawing that cannot be
    /// read is asked about on every frame the preview draws, and a log full of the same line hides what else broke.
    logged: bool,
}

/// F3.5 S5 (`cached per file`): the preview's rasters, one per FILE rather than one per effect — the same logo in six
/// places is one ffmpeg run and one surface — plus the memory of which files have already been complained about.
///
/// Keyed on the path as given: two projects pointing at the same card want the same picture, and a different path is
/// a different file even when both end in `logo.svg`.
#[derive(Debug, Default)]
pub struct Rasters {
    entries: HashMap<String, Entry>,
}

impl Rasters {
    /// F3.5 S5: ask where this file's preview copy stands, and take on the job if nobody has. The first call for a
    /// file says [`Slot::NeedsRaster`] and marks it busy, so a second call — another effect on the same logo, or the
    /// next preview frame — gets [`Slot::Busy`] instead of a second raster. A file that failed is never retried: the
    /// reason (a broken svg, no ffmpeg) does not fix itself between frames, and re-asking re-prints the log line.
    pub fn request(&mut self, path: &str) -> Slot {
        let entry = self.entries.entry(path.to_string()).or_default();
        if entry.ready {
            return Slot::Ready;
        }
        if entry.failed {
            return Slot::Failed;
        }
        if entry.busy {
            return Slot::Busy;
        }
        entry.busy = true;
        Slot::NeedsRaster
    }

    /// F3.5 S5: the raster arrived. The file stops being busy and is never asked for again.
    pub fn done(&mut self, path: &str) {
        let entry = self.entries.entry(path.to_string()).or_default();
        entry.busy = false;
        entry.ready = true;
    }

    /// F3.5 S5 (`failure logged once`): the line for a drawing that cannot be shown, `Some` only the first time this
    /// file fails. The message names the drawing as [`base`] spells it, because the path is long and the person's
    /// question is *which* drawing; §A.5's own wording is "the drawing X cannot be shown: …".
    pub fn failure_line(&mut self, path: &str, reason: &str) -> Option<String> {
        let entry = self.entries.entry(path.to_string()).or_default();
        entry.busy = false;
        entry.failed = true;
        if entry.logged {
            return None;
        }
        entry.logged = true;
        Some(format!("the drawing {} cannot be shown: {}", base(path), reason))
    }

    /// How many files this holds a copy of — one per file, which is the invariant §A.5 states and a test pins.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Nothing asked about yet. `len`/`is_empty` come as a pair; an app that has never opened a project has none.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}
