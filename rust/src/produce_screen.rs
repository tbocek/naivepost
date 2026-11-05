//! §08-produce#1-screen — 1. Screen: the Produce page's settings, images row, thumbnail editor and readouts.
//!
//! Nothing here draws: each rule §1 states is one plain function or constant, and the widget layer renders
//! them. `P.policy.gameVolume` (0.22) is the one §10 number shown; which picture is the base stays
//! [`crate::publish`]'s answer.

use crate::produce_runs::even;
use crate::project::{Codec, Container, Subtitles, TextMark};
use crate::tools::mm_ss;

/// The rows of the settings scroller, in §A's order, so a test can pin the list against the table.
pub const SETTINGS_ROWS: [&str; 12] = [
    "Container",
    "Codec",
    "Preset",
    "Resolution",
    "Frame rate",
    "Audio",
    "Subtitles",
    "Translate",
    "Game audio",
    "Quality (CRF)",
    "Channels",
    "Frame edges",
];

/// The encoder rows the Produce page draws: every stored row but `Game audio`, which lives on the Narrate
/// page (§07 §1) but is still stored with the encoder settings.
pub fn settings_rows_shown() -> Vec<&'static str> {
    SETTINGS_ROWS
        .iter()
        .copied()
        .filter(|row| *row != "Game audio")
        .collect()
}

pub const CONTAINERS: [&str; 3] = ["mp4", "mkv", "webm"];
pub const CODECS: [&str; 3] = ["h264", "h265", "vp9"];
pub const PRESETS: [&str; 6] = [
    "ultrafast",
    "veryfast",
    "fast",
    "medium",
    "slow",
    "veryslow",
];
pub const RESOLUTIONS: [&str; 3] = ["720p", "1080p", "original"];
/// §A's Frame rate row. `source` keeps the recording's own rate.
pub const FRAME_RATES: [&str; 4] = ["source", "60", "30", "24"];
pub const AUDIO_KBPS: [&str; 4] = ["128", "192", "256", "320"];
/// §A's Subtitles row: what the VIDEO carries. An .srt and a .vtt are written beside it regardless (§C).
pub const SUBTITLE_CHOICES: [&str; 3] = ["burned in", "track in file", "none in the video"];
pub const TRANSLATE_LANGUAGES: [&str; 3] = ["English", "German", "French"];
/// §A's Quality row: CRF runs 14..34 and the slider carries a mark at 24, the default.
pub const CRF_MIN: u32 = 14;
pub const CRF_MAX: u32 = 34;
pub const CRF_MARK: u32 = 24;
pub const MONO_TICK: &str = "mono";
pub const EDGES_TICK: &str = "blurred";

/// The options of one row, as §A lists them. An unknown row has no list.
pub fn options(row: &str) -> Option<&'static [&'static str]> {
    match row {
        "Container" => Some(&CONTAINERS),
        "Codec" => Some(&CODECS),
        "Preset" => Some(&PRESETS),
        "Resolution" => Some(&RESOLUTIONS),
        "Frame rate" => Some(&FRAME_RATES),
        "Audio" => Some(&AUDIO_KBPS),
        "Subtitles" => Some(&SUBTITLE_CHOICES),
        // The session's language is removed from this list.
        "Translate" => Some(&TRANSLATE_LANGUAGES),
        _ => None,
    }
}

/// The default of one row, as §A spells it; `off`/`on` for the two tick rows.
pub fn defaults() -> [(&'static str, &'static str); 12] {
    [
        ("Container", "mp4"),
        ("Codec", "h264"),
        ("Preset", "slow"),
        ("Resolution", "1080p"),
        ("Frame rate", "30"),
        ("Audio", "128"),
        ("Subtitles", "none in the video"),
        // No language ticked: translating is asked for, never assumed.
        ("Translate", ""),
        ("Game audio", "0.22"),
        ("Quality (CRF)", "24"),
        ("Channels", "off"),
        // The one tick that starts on: a black band is the uglier answer to an odd-shaped frame.
        ("Frame edges", "on"),
    ]
}

/// §A's Tooltip column, one sentence per row.
pub fn tooltip(row: &str) -> &'static str {
    match row {
        "Container" => "mp4 plays everywhere; mkv keeps subtitle tracks best; webm forces VP9 + Opus",
        "Codec" => "h264 is the safe upload; h265 is smaller but slower; vp9 is for webm",
        "Preset" => "how long the encoder may think — slower means smaller at the same quality",
        "Resolution" => "the short side of the frame — the cut page's aspect sets its shape; original keeps the footage's own size",
        "Frame rate" => "output frame rate",
        "Audio" => "audio bitrate in kbit/s",
        "Subtitles" => "what the VIDEO carries; an .srt is written beside the video either way, one per language ticked",
        "Translate" => "Also write the subtitles in these languages, translated — this video's own language is not offered",
        "Game audio" => "how loud the original game audio sits under the narration",
        "Quality (CRF)" => "quality: lower is better and bigger (18–24 is the usual range; 24 is the default, marked)",
        "Channels" => "Mix the finished audio down to a single channel",
        "Frame edges" => "Fill the empty edges of the frame with a blown-up, blurred copy of the picture itself, instead of black",
        _ => "",
    }
}

/// §1: the session's own language is never offered for translation. An unknown language removes nothing.
pub fn translate_options(session_language: &str) -> Vec<&'static str> {
    TRANSLATE_LANGUAGES
        .into_iter()
        .filter(|language| !session_language.eq_ignore_ascii_case(language))
        .collect()
}

/// §A's two forcing rules: webm carries vp9 and nothing else, and holds no subtitle track, so "track in
/// file" becomes "none in the video". Other containers leave both alone.
pub fn apply_container_rules(
    container: Container,
    codec: Codec,
    subtitles: Subtitles,
) -> (Codec, Subtitles) {
    match container {
        Container::Webm => (Codec::Vp9, Subtitles::None),
        _ => (codec, subtitles),
    }
}

/// §A's ↻ Transcode: an encode with no model call.
pub const TRANSCODE_TIP: &str = "Encode the video again from the cut and these settings — no model call, and the thumbnail and the upload text are left alone";

/// §A's ⤓ Save video: a copy out, so `produce/final` stays the one output that is stamped.
pub const SAVE_VIDEO_TIP: &str =
    "Save the finished video somewhere else — a copy; produce/final stays where it is";

/// §1: up to 8 pictures go to the image model, the first being the base it edits.
pub const MAX_IMAGES: usize = 8;

pub const EMPTY_IMAGES: &str = "No image — the thumbnail will be drawn from the instruction alone.";

pub const EMPTY_IMAGES_HINT: &str = "Add image… to edit one of your own frames instead.";

/// What ＋ Add image and Change… say when there is nothing to choose from; names the button that fixes it.
pub const NO_FRAME_TO_ADD: &str =
    "no extracted frame to add \u{2014} Prepare takes frames before a picture can be chosen";

pub const IMAGES_HEADING_TIP: &str = "What the image model is given, in order. The FIRST is the base — the picture being edited — and the others are references the instruction can name.";

/// §1: adding never replaces anything and a ninth picture is refused rather than dropping one.
pub fn add_image(frames: &mut Vec<String>, path: &str) -> bool {
    if frames.len() >= MAX_IMAGES {
        return false;
    }
    frames.push(path.to_string());
    true
}

/// §10's `P.policy.publishFrames`: the first run hands the model three Prepare frames, not none. Three is a
/// start, not a cap ([`MAX_IMAGES`]); the first is the base ([`crate::publish::base`]).
pub const FIRST_IMAGES: usize = 3;

/// The first run's frames: up to [`FIRST_IMAGES`] of what the session offers, in order, so the earliest is
/// the base. None means an empty row ([`EMPTY_IMAGES`]).
pub fn seed_frames(available: &[String]) -> Vec<String> {
    available.iter().take(FIRST_IMAGES).cloned().collect()
}

/// §10's `P.machine.thumbnailLongSide`: 1280, the size every uploader displays. The *long* side: a 9:16
/// thumbnail is 720×1280.
pub const THUMB_LONG_SIDE: u32 = 1280;

/// §2's frame-box rule: no aspect (or `source`) means the picture's own shape, else its ratio; with nothing
/// to read it off, 16:9. The longer edge stops at [`THUMB_LONG_SIDE`] and both sides come out even.
///
/// `aspect` is `"w:h"`, or empty / `source`; then `wide`/`high` (what ffprobe reported) decide the shape.
pub fn thumb_box(aspect: &str, wide: u32, high: u32) -> (u32, u32) {
    let own = (wide.max(1), high.max(1));
    let (w, h) = match ratio(aspect) {
        Some(shape) => shape,
        // "source", empty, or unparseable: the picture's own edges, and 16:9 if nothing is known.
        None if wide > 0 && high > 0 => own,
        None => (16, 9),
    };
    let (w, h) = (w.max(1), h.max(1));
    // Scale from the longer edge so neither side exceeds the tier.
    if w >= h {
        (THUMB_LONG_SIDE, even(THUMB_LONG_SIDE * h / w))
    } else {
        (even(THUMB_LONG_SIDE * w / h), THUMB_LONG_SIDE)
    }
}

fn ratio(aspect: &str) -> Option<(u32, u32)> {
    let (w, h) = aspect.split_once(':')?;
    let w: u32 = w.trim().parse().ok()?;
    let h: u32 = h.trim().parse().ok()?;
    (w > 0 && h > 0).then_some((w, h))
}

/// §1: `Make base` on every non-base slot: the chosen reference goes to the front and the old base joins the
/// references. Out-of-range slots are refused.
pub fn make_base(frames: &mut Vec<String>, slot: usize) -> bool {
    if slot == 0 || slot >= frames.len() {
        return false;
    }
    let chosen = frames.remove(slot);
    frames.insert(0, chosen);
    true
}

/// §1's `Change…`: the slot keeps its place, only its picture changes.
pub fn change_image(frames: &mut Vec<String>, slot: usize, path: &str) -> bool {
    let Some(slot) = frames.get_mut(slot) else {
        return false;
    };
    *slot = path.to_string();
    true
}

/// What a slot offers. `Set Thumbnail` crops to the video's shape, no model and no GPU (§A), so it is on
/// every slot; `Make base` only on a reference.
pub fn slot_actions(slot: usize, count: usize) -> &'static [&'static str] {
    const BASE: [&str; 3] = ["Set Thumbnail", "Change…", "remove"];
    const REFERENCE: [&str; 4] = ["Make base", "Set Thumbnail", "Change…", "remove"];
    if slot == 0 || slot >= count {
        &BASE
    } else {
        &REFERENCE
    }
}

/// §1: the base wears a crop box when its shape differs from the video's. A video shape with a zero side
/// means NO SHAPE IS KNOWN YET (the page holds `(0, 0)`), which is not a difference.
pub fn shows_crop_box(base_shape: Option<(u32, u32)>, video_shape: (u32, u32)) -> bool {
    if video_shape.0 == 0 || video_shape.1 == 0 {
        return false;
    }
    match base_shape {
        Some(shape) => shape != video_shape,
        None => false,
    }
}

/// §A: the instruction box is four lines, the negative prompt two.
pub const INSTRUCTION_LINES: usize = 4;
pub const NEGATIVE_LINES: usize = 2;

/// §A's Edit instruction tooltip.
pub const INSTRUCTION_TIP: &str = "What the image model is told to make out of the images. No words: the title and the marked texts are printed on afterwards.";

pub const NEGATIVE_TIP: &str =
    "What must stay out of the picture — watermarks, logos, lettering, extra limbs.";

/// §A's ⤓ export: YouTube takes a JPEG under 2 MB, so the export says which limit it keeps under.
pub const EXPORT_TIP: &str =
    "Export the thumbnail as a JPEG — what YouTube takes, under its 2 MB limit";

pub const REDRAW_TIP: &str =
    "Draw the thumbnail again from the images and the instruction as they stand — a fresh draw";

/// §A ("Text overlays"): the picture column's width, the smallest box that holds words, the snap distance
/// to edges, middles and other boxes, and the ✎ chip's size.
pub const OVERLAY_EDGE_PX: f64 = 200.0;
pub const OVERLAY_MIN_PX: f64 = 28.0;
pub const OVERLAY_SNAP_PX: f64 = 10.0;
pub const OVERLAY_CHIP_PX: f64 = 22.0;

/// A drag that ends smaller than [`OVERLAY_MIN_PX`] in either direction is not a box. Pixels, the hand's unit.
pub fn box_is_too_small(w_px: f64, h_px: f64) -> bool {
    w_px < OVERLAY_MIN_PX || h_px < OVERLAY_MIN_PX
}

/// How near a box's border a press takes that border rather than the box: a 2 px dashed line is not a mouse target.
pub const OVERLAY_GRAB_PX: f64 = 9.0;

/// What a press at a point has hold of on one box: a side border (`horiz`; `left` says which), a top or bottom
/// border (`vert`, `top`), both at a corner, or the middle (`inside`). Nothing set is a press beside the box.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Grip {
    pub horiz: bool,
    pub vert: bool,
    pub left: bool,
    pub top: bool,
    pub inside: bool,
}

impl Grip {
    /// The press took the box: a border to resize or the middle to move.
    pub fn holds(&self) -> bool {
        self.horiz || self.vert || self.inside
    }

    /// The pointer over the box, named before any press: resize arrows on borders and corners, move hand inside.
    pub fn cursor(&self) -> &'static str {
        match (self.horiz, self.vert) {
            (true, true) if self.left == self.top => "nwse-resize",
            (true, true) => "nesw-resize",
            (true, false) => "col-resize",
            (false, true) => "row-resize",
            _ if self.inside => "move",
            _ => "default",
        }
    }
}

pub fn grip_at(x: f64, y: f64, (rx, ry, rw, rh): (f64, f64, f64, f64)) -> Grip {
    let reach = OVERLAY_GRAB_PX;
    let in_x = x >= rx - reach && x <= rx + rw + reach;
    let in_y = y >= ry - reach && y <= ry + rh + reach;
    let near_left = (x - rx).abs() <= reach;
    let near_right = (x - (rx + rw)).abs() <= reach;
    let near_top = (y - ry).abs() <= reach;
    let near_bottom = (y - (ry + rh)).abs() <= reach;
    Grip {
        horiz: (near_left || near_right) && in_y,
        vert: (near_top || near_bottom) && in_x,
        left: near_left,
        top: near_top,
        inside: x >= rx && x <= rx + rw && y >= ry && y <= ry + rh,
    }
}

/// A border drag on a box of words: the hand at `(x, y)`, the box as it was `was`, the grip it was taken by.
/// A side moves one edge, a corner two; the far edge stays. No box gets smaller than [`OVERLAY_MIN_PX`].
pub fn resize_box(x: f64, y: f64, was: (f64, f64, f64, f64), grip: Grip) -> (f64, f64, f64, f64) {
    let (mut nx, mut ny, mut nw, mut nh) = was;
    let (x0, y0, w0, h0) = was;
    if grip.horiz {
        if grip.left {
            let anchor = x0 + w0;
            nx = x.min(anchor - OVERLAY_MIN_PX);
            nw = anchor - nx;
        } else {
            nx = x0;
            nw = (x - x0).max(OVERLAY_MIN_PX);
        }
    }
    if grip.vert {
        if grip.top {
            let anchor = y0 + h0;
            ny = y.min(anchor - OVERLAY_MIN_PX);
            nh = anchor - ny;
        } else {
            ny = y0;
            nh = (y - y0).max(OVERLAY_MIN_PX);
        }
    }
    (nx, ny, nw, nh)
}

/// The move half of the snapping: any of a whole box's three lines on one axis (near edge, middle, far edge)
/// may land on a target. `near` is the near edge, `length` the box's size on that axis; the closest line
/// within [`OVERLAY_SNAP_PX`] wins.
pub fn snap_slide(near: f64, length: f64, targets: &[f64]) -> f64 {
    let mut best = (near, OVERLAY_SNAP_PX);
    for mine in [near, near + length / 2.0, near + length] {
        for target in targets {
            let off = (mine - target).abs();
            if off < best.1 {
                best = (near + (target - mine), off);
            }
        }
    }
    best.0
}

/// A box in picture pixels `(x, y, w, h)` back into stored fractions, held inside the picture (never narrower
/// than 4 % nor shorter than 3 %, never off an edge).
pub fn box_fractions(rect: (f64, f64, f64, f64), wide: f64, high: f64) -> crate::project::TitleBox {
    let (x, y, w, h) = rect;
    let wf = (w / wide).clamp(0.04, 1.0);
    let hf = (h / high).clamp(0.03, 1.0);
    crate::project::TitleBox {
        cx: ((x + w / 2.0) / wide).clamp(wf / 2.0, 1.0 - wf / 2.0),
        cy: ((y + h / 2.0) / high).clamp(hf / 2.0, 1.0 - hf / 2.0),
        wf,
        hf,
    }
}

/// §A's snap targets for one drag: the picture's edges and middles plus EVERY box already on it, in the
/// plate's pixels (marks store fractions, so the conversion happens at the plate's size). `(x targets, y targets)`.
pub fn overlay_snap_targets(
    plate_w: f64,
    plate_h: f64,
    marks: &[crate::project::TextMark],
) -> (Vec<f64>, Vec<f64>) {
    let mut xs = vec![0.0, plate_w / 2.0, plate_w];
    let mut ys = vec![0.0, plate_h / 2.0, plate_h];
    for mark in marks {
        xs.push((mark.cx - mark.wf / 2.0) * plate_w);
        xs.push(mark.cx * plate_w);
        xs.push((mark.cx + mark.wf / 2.0) * plate_w);
        ys.push((mark.cy - mark.hf / 2.0) * plate_h);
        ys.push(mark.cy * plate_h);
        ys.push((mark.cy + mark.hf / 2.0) * plate_h);
    }
    (xs, ys)
}

/// A drag lands on the nearest snap target within [`OVERLAY_SNAP_PX`]; otherwise it stays where the hand left it.
pub fn snap(value: f64, targets: &[f64]) -> f64 {
    let nearest = targets
        .iter()
        .filter(|target| (value - **target).abs() <= OVERLAY_SNAP_PX)
        .min_by(|left, right| {
            (**left - value)
                .abs()
                .partial_cmp(&(**right - value).abs())
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    nearest.copied().unwrap_or(value)
}

/// §1: dragging a box over the picture puts words on it, printed to fill the box. Stored as fractions of the
/// picture ([`crate::project::TextMark`]), so a column resize never rescales position. An empty text is not
/// a mark: ✎ with nothing in it removes the box.
pub fn place_words(mark: &mut TextMark, cx: f64, cy: f64, wf: f64, hf: f64, text: &str) {
    mark.cx = cx;
    mark.cy = cy;
    mark.wf = wf;
    mark.hf = hf;
    mark.text = text.trim().to_string();
}

/// §1's ✎ reword: the box keeps its place and size, only the words change.
pub fn set_words(mark: &mut TextMark, text: &str) {
    mark.text = text.trim().to_string();
}

/// §1's ✎ remove: the others keep their order.
pub fn remove_words(marks: &mut Vec<TextMark>, index: usize) -> bool {
    if index >= marks.len() {
        return false;
    }
    marks.remove(index);
    true
}

/// §A: the title band's default box, in fractions of the picture: across the top, on its own.
pub const TITLE_BOX_DEFAULT: (f64, f64, f64, f64) = (0.5, 0.25, 1.0, 0.4);

/// §A's ↻ suggest rewrites all three written things at once so they cannot disagree about the video.
pub const SUGGEST_TIP: &str =
    "Ask the model for a fresh title and description — the only thing that does";

pub const TITLE_PLACEHOLDER: &str =
    "the video's title, also printed on the thumbnail — ▶ suggests one";

/// §A's title tooltip: the first thumbnail takes the title as its printed line; after that the two are separate.
pub const TITLE_TIP: &str = "Four to seven words. The first thumbnail takes it as its printed line; after that the two are separate.";

/// §A's description box: eight lines, roughly what YouTube shows before "more".
pub const DESCRIPTION_LINES: usize = 8;

/// §1: with no cut there is nothing to produce; the page says it in the other pages' words.
pub const NO_CUT_LINE: &str = crate::narrate_details::NO_CUT_LINE;

/// The Inputs row: `N clip(s) · mm:ss`, then which narration state applies (never both), then whether the
/// upload text exists. `no_narration`: no line has words (there is no switch, §07 §1); `to_speak`: lines ▶ has to speak.
pub fn inputs_line(
    clips: usize,
    seconds: f64,
    no_narration: bool,
    to_speak: usize,
    publish_written: bool,
) -> String {
    if clips == 0 {
        return NO_CUT_LINE.to_string();
    }
    let mut line = format!("{} clip(s) \u{b7} {}", clips, mm_ss(seconds));
    if no_narration {
        line.push_str(" \u{b7} no narration");
    } else if to_speak > 0 {
        line.push_str(&format!(" \u{b7} {to_speak} to speak"));
    }
    if !publish_written {
        line.push_str(" \u{b7} no upload text");
    }
    line
}

/// The Inputs tooltip: what is spoken first, which recordings are mixed, the voice, whether `publish.json`
/// exists. A row with nothing to say is dropped.
pub fn inputs_tooltip(
    cut_line: &str,
    first_line: Option<&str>,
    mixed: &[String],
    voice: Option<&str>,
    publish_written: bool,
) -> String {
    let mut rows: Vec<String> = Vec::new();
    if !cut_line.is_empty() {
        rows.push(cut_line.to_string());
    }
    match first_line {
        Some(first) => rows.push(format!("Spoken first: \u{201c}{first}\u{201d}")),
        None => rows.push("nothing will be spoken".to_string()),
    }
    if !mixed.is_empty() {
        rows.push(format!("Mixed recordings: {}", mixed.join(", ")));
    }
    if let Some(voice) = voice {
        rows.push(format!("Voice: {voice}"));
    }
    rows.push(if publish_written {
        "publish.json is written \u{2014} ▶ replaces it".to_string()
    } else {
        "no publish.json yet \u{2014} the upload text is written with the video".to_string()
    });
    rows.join("\n\n")
}

/// The page's `publish_written`, answered as §A asks: does `publish.json` exist.
///
/// The page cannot answer from its own rows: a session whose first ▶ already ran holds an upload text a page
/// reload would not see, and rows alone would print "no upload text" beside a file the next ▶ rewrites.
/// `held` is what a flow published to the page; either side saying "written" makes the row say it.
pub fn publish_written(held: bool, file_exists: bool) -> bool {
    held || file_exists
}

/// The Outputs row counts, and only counts (§1). MB above 1, KB below, so 200 kB does not read as "0 MB".
pub fn outputs_summary(files: usize, bytes: u64) -> String {
    const MIB: u64 = 1024 * 1024;
    if files == 0 {
        return "0 files".to_string();
    }
    if bytes < MIB {
        return format!("{files} files, {} KB", bytes.div_ceil(1024));
    }
    format!("{files} files, {} MB", (bytes + MIB / 2) / MIB)
}

/// §1's folder-button tooltip: the four things this step writes.
pub const FOLDER_TIP: &str =
    "produce/ \u{2014} the finished video, the per-clip encodes, the thumbnail and the upload text";
