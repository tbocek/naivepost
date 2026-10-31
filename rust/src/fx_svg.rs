//! F3.5 SVG drawing by hand (`spec/06-effects.md` F3.5 S1–S5, `spec/inventory/effects.md` §A.5).
//! A drawing is a text with ink, so most of it is [`crate::fx_text`]'s (box, 0.3 s fades and floor, `clampFades`). Differences: ▨ SVG asks for the file FIRST, before arming, and an untouched drawing lands in the middle. The widget layer holds no rule: `window.rs` routes ▨ SVG via `press_svg_item` (→ [`press`]), the dialog via `svg_chosen` (→ [`chose`]), the drag via `svg_drag_ended_with_source` (→ [`place`]), the form via `show_svg_form` and `press_svg_apply` (→ [`apply`] + `record_edit`, one ↶).

use std::collections::HashMap;
use std::path::Path;

use crate::cut::Fx;
use crate::cut_speed;
use crate::fx_text::{self, BoxChoice, Box_};
use crate::fx_zoom;
use crate::layout;
use crate::tools;

/// F3.5 S1 (`no line? → "click a track first — the drawing needs a moment to appear at"`): its own wording beside [`fx_zoom::NO_LINE`], [`crate::cut_copy::NO_LINE_YET`] and `crate::cut_insert::NO_LANE_LINE_YET`; deliberately not zoom's sentence, and a test keeps them apart.
pub const NO_LINE: &str = "click a track first \u{2014} the drawing needs a moment to appear at";

/// F3.5 S1: the chooser's title, spoken before the arm since the arm sentence names the file.
pub const CHOOSE_TITLE: &str = "Choose a drawing to lay over the video";

/// F3.5 S1 (§A.5 `filter svg`): the chooser filter name; only suffix `svg` (prototype `extFilter("SVG drawing", "svg")`).
pub const CHOOSE_FILTER: &str = "SVG drawing";

/// F3.5 S1 (§A.5 `assets folder`): the chooser opens at the root-level shared cards ([`layout::assets_dir`], as [`crate::cut_insert::chooser_dir`]).
pub fn chooser_dir(root: &Path) -> std::path::PathBuf {
    layout::assets_dir(root)
}

/// F3.5 S1: what pressing ▨ SVG did. No `Armed` (unlike [`fx_zoom::Press`]): nothing is picked yet, so it opens a dialog or refuses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Press {
    OpenChooser,
    Refused(&'static str),
}

/// F3.5 S1 (`a line?`): the line is asked for BEFORE the file, so no chooser trip is wasted.
pub fn press(line: Option<f64>) -> Press {
    match line {
        None => Press::Refused(NO_LINE),
        Some(_) => Press::OpenChooser,
    }
}

/// F3.5 S2 (`a file chosen? → no → "choose a drawing and it goes on the picture"`): cancelling places nothing.
pub const NO_FILE: &str = "choose a drawing and it goes on the picture";

/// F3.5 S2: what the chooser answered. `Chose::Armed` carries the file because the arm sentence prints it and the record's `src` is that same string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Chose {
    Armed(String),
    Refused(&'static str),
}

/// F3.5 S2: a file back arms the gesture and never toggles it off (prototype: "never a toggle-off: a file was just chosen on purpose"), so this takes no `already_armed`, unlike [`fx_zoom::arm`] and [`crate::fx_text::arm`].
pub fn chose(file: Option<&str>) -> Chose {
    match file.map(str::trim).filter(|name| !name.is_empty()) {
        Some(name) => Chose::Armed(name.to_string()),
        None => Chose::Refused(NO_FILE),
    }
}

/// F3.5 S2 (`arms ("Drag the box <file> goes in — ...")`): the gesture sentence with the file named; one constant with a hole so a test compares the whole sentence. The semicolon is §A.5's punctuation.
pub const ARM_WORDS: &str =
    "Drag the box {} goes in \u{2014} anywhere on the picture, any shape; the drawing keeps \
                             its own shape inside it. A click puts one across the middle. ";

/// F3.5 S2: the arm's whole paragraph: this gesture with the file named, then [`fx_zoom::arm_tail`] shared with F3.4.
pub fn arm_words(file: &str, marked: Option<(f64, f64)>) -> String {
    format!("{}{}", arm_head(file), fx_zoom::arm_tail(marked))
}

/// F3.5 S2: [`ARM_WORDS`] with the file filled in, for a test to compare with §A.5 without the tail's seconds.
pub fn arm_head(file: &str) -> String {
    ARM_WORDS.replace("{}", file)
}

/// F3.5 S3 (§A.5 `default box centred {0.5,0.5,0.6,0.6}`; §1 `svg: middle`): a drawing placed without a drawn box goes in the middle, not [`fx_text::LOWER_THIRD`]. Same centre as [`crate::cut::Fx::centre`] with its size spelled.
pub const MIDDLE: Box_ = Box_ {
    cx: 0.5,
    cy: 0.5,
    wf: 0.6,
    hf: 0.6,
};

/// F3.5 S3 (`Camera layer stays up`): a drawing's box is on the output frame, as in [`fx_text`]; opposite of ⊕ Zoom's [`fx_zoom::whole_source_shown`].
pub fn camera_layer_stays_up() -> bool {
    fx_text::camera_layer_stays_up()
}

/// F3.5 S4 (`Form "SVG at m:ss"`): titled via [`tools::mm_ss`], the one spelling of a second.
pub fn form_title(t: f64) -> String {
    format!("SVG at {}", tools::mm_ss(t))
}

/// F3.5 S4 (`file name + Choose…, Length, fades, Curve`): the fields in the order the form reads them; the file first because it can be changed here.
pub const FORM_FIELDS: [&str; 5] = ["File", "Length (s)", "Fade in (s)", "Fade out (s)", "Curve"];

/// F3.5 S4 (`+ Choose…`): re-opens [`CHOOSE_TITLE`]'s dialog; a drawing is the one effect whose form changes what it is made of.
pub const CHOOSE_BUTTON: &str = "Choose\u{2026}";

/// F3.5 S4: what the form holds on Apply: seconds and fades as typed, the curve's display name, the file (maybe re-chosen) and the box the gesture left.
#[derive(Debug, Clone, PartialEq)]
pub struct Form {
    pub t: f64,
    pub dur: f64,
    pub trans: f64,
    pub tout: f64,
    pub curve: String,
    pub file: String,
    pub on: Box_,
}

/// F3.5 S4 (`No file → not placed`): the record the form writes back. An empty `src` (judged on `trim()`) is refused with §F3.5's words, else the lane gets a bar that draws nothing.
/// Otherwise [`fx_text::apply`]'s rule: 0.3 s floor ([`fx_text::MIN_SECONDS`]), fades [`fx_text::FADE_SECONDS`] shared by [`cut_speed::clamp_fades`], curve via [`fx_zoom::curve_stored`] (Linear writes no `ease`), box clamped on the way in. The words field stays empty: §1 keeps an svg's content in `src`.
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

/// F3.5 S2/S4 (`Drag or click (default box: the middle); t/dur from the line (3 s) or selection; fades 0.3`): what the gesture left before the form was touched. Seconds follow [`fx_text::place`]: a marked stretch is the length, else 3 s from the line ([`fx_zoom::DEFAULT_SECONDS`], P.policy.effectDefaultSeconds); a drag under [`fx_zoom::DRAG_MIN_PX`] takes the middle. With no selection and no line, or no file ([`NO_FILE`]), nothing is placed.
pub fn place(
    choice: BoxChoice,
    file: &str,
    line: Option<f64>,
    selection: Option<(f64, f64)>,
    frame: (f64, f64),
) -> Option<Fx> {
    let on = match choice {
        BoxChoice::Click => MIDDLE,
        BoxChoice::Dragged { w, h, .. } if w < fx_zoom::DRAG_MIN_PX || h < fx_zoom::DRAG_MIN_PX => {
            MIDDLE
        }
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

/// F3.5 S4 (`file name`): a drawing's file short enough for a label: the last `/` component kept WHOLE (a card's `?S=Dust II` is part of it). Empty or blank gives `(no file)`.
pub fn base(path: &str) -> String {
    if path.trim().is_empty() {
        return "(no file)".to_string();
    }
    path.rsplit('/').next().unwrap_or(path).to_string()
}

/// F3.5 S5 (`Preview raster via ffmpeg, 512 px`): bigger than any preview box, small enough to keep. §10 has no `P.` row (`svgPreviewPx`), so it takes the bare prefix `effects.previewRasterPx`, catalogued once in `params::cut()`.
pub const PREVIEW_RASTER_PX: f64 = 512.0;

/// F3.5 S5 (`Preview raster via ffmpeg … transparent`): ffmpeg's arguments for a drawing's preview as an argument vector (running stays with [`crate::subprocess`]).
/// Needs the librsvg input options for size (`-width`/`-height`, `-keep_ar 1`) and rgba output so transparency survives.
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

/// F3.5 S5 (`cached per file`): the name of a drawing's preview copy, one per SOURCE file. Hashed because two files can share a base name in different folders; `llm_cache::key` over the path, `.png` for ffmpeg's muxer.
pub fn raster_file_name(path: &str) -> String {
    let digest = crate::llm_cache::key(&[serde_json::Value::String(path.to_string())]);
    // A path always encodes, so `None` means a non-path; fall back to a fixed name rather than panic.
    let hex = digest.unwrap_or_else(|| "drawing".to_string());
    format!("{hex}.png")
}

/// F3.5 S5 (`the drawing keeps its own shape inside it`): the largest rectangle inside the box with aspect kept, centred on the short axis, as `(x, y, w, h)` in the units given. Shared by preview and render ([`fx_text::preview_is_the_render`]); an unknown aspect (`<= 0` or sizeless box) returns the box itself.
pub fn fit_inside(box_w: f64, box_h: f64, aspect: f64) -> (f64, f64, f64, f64) {
    if box_w <= 0.0 || box_h <= 0.0 || aspect <= 0.0 {
        return (0.0, 0.0, box_w.max(0.0), box_h.max(0.0));
    }
    let w = box_w.min(box_h * aspect);
    let h = w / aspect;
    ((box_w - w) / 2.0, (box_h - h) / 2.0, w, h)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    Ready,
    Busy,
    Failed,
    NeedsRaster,
}

#[derive(Debug, Default)]
struct Entry {
    ready: bool,
    busy: bool,
    failed: bool,
    /// Whether this file's failure was already printed (§A.5 "failure logged once"); an unreadable drawing is asked about every frame.
    logged: bool,
    /// Where this file's 512 px copy lives, read via [`Rasters::raster_of`]; `None` when its raster failed ("no picture, box only").
    raster: Option<std::path::PathBuf>,
}

/// F3.5 S5 (`cached per file`): the preview's rasters, one per FILE (the same logo in six places is one ffmpeg run), plus which files were already complained about. Keyed on the path as given.
#[derive(Debug, Default)]
pub struct Rasters {
    entries: HashMap<String, Entry>,
}

impl Rasters {
    /// F3.5 S5: ask where this file's copy stands and take on the job if nobody has: the first call says [`Slot::NeedsRaster`] and marks it busy, later ones [`Slot::Busy`]. A failed file is never retried (re-asking re-prints the log).
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

    pub fn done(&mut self, path: &str) {
        let entry = self.entries.entry(path.to_string()).or_default();
        entry.busy = false;
        entry.ready = true;
    }

    /// F3.5 S5: the raster arrived AND where; `done` alone would mark a file ready with nothing to paint.
    pub fn done_with(&mut self, path: &str, file: &std::path::Path) {
        let key = path.to_string();
        let entry = self.entries.entry(key).or_default();
        entry.busy = false;
        entry.ready = true;
        entry.raster = Some(file.to_path_buf());
    }

    /// Where this file's copy is, `None` while unmade or failed. Read-only (takes on no job), so the painter can ask every frame.
    pub fn raster_of(&self, path: &str) -> Option<std::path::PathBuf> {
        self.entries
            .get(path)
            .and_then(|entry| entry.raster.clone())
    }

    /// F3.5 S5 (`failure logged once`): the line for a drawing that cannot be shown, `Some` only the first time. Names it as [`base`] spells it ("the drawing X cannot be shown: …", §A.5).
    pub fn failure_line(&mut self, path: &str, reason: &str) -> Option<String> {
        let entry = self.entries.entry(path.to_string()).or_default();
        entry.busy = false;
        entry.failed = true;
        if entry.logged {
            return None;
        }
        entry.logged = true;
        Some(format!(
            "{}the drawing {} cannot be shown: {}",
            crate::effect_details::PREFIX,
            base(path),
            reason
        ))
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}
