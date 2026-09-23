//! §08-produce#1-screen — 1. Screen: the Produce page's settings, images row, thumbnail editor and readouts.
//!
//! The page is two columns (§A): left the images/thumbnail column, right the written column above the
//! encoder settings in a scroller. Nothing here draws; each rule §1 states is one plain function or
//! constant, so the widget layer only renders these answers and forwards presses. `P.policy.gameVolume`
//! (0.22) is the one §10 number this page shows; every encode number is ffmpeg's own (`tool:ffmpeg.encode`).
//!
//! What the pictures *are* — which is the base, which are references — stays [`crate::publish`]'s answer;
//! this module only edits the list and says what each slot offers.

use crate::project::{Codec, Container, Publish, Subtitles, TextMark};
use crate::tools::mm_ss;

// ---- A. encoder settings (§1's "Encoder settings", options/tooltips in inventory §A) --

/// The rows of the settings scroller, in §A's order — the page draws them in this order and nowhere else
/// keeps it, so a test can pin the list against the table.
pub const SETTINGS_ROWS: [&str; 13] = [
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
    "Frame timing",
    "Channels",
    "Frame edges",
];

/// §A's Container row: mp4, mkv, webm.
pub const CONTAINERS: [&str; 3] = ["mp4", "mkv", "webm"];
/// §A's Codec row: h264, h265, vp9.
pub const CODECS: [&str; 3] = ["h264", "h265", "vp9"];
/// §A's Preset row, fastest to smallest.
pub const PRESETS: [&str; 6] = ["ultrafast", "veryfast", "fast", "medium", "slow", "veryslow"];
/// §A's Resolution row: the short side of the frame, or the footage's own size.
pub const RESOLUTIONS: [&str; 3] = ["720p", "1080p", "original"];
/// §A's Frame rate row. `source` keeps the recording's own rate.
pub const FRAME_RATES: [&str; 4] = ["source", "60", "30", "24"];
/// §A's Audio row, in kbit/s.
pub const AUDIO_KBPS: [&str; 4] = ["128", "192", "256", "320"];
/// §A's Subtitles row: what the VIDEO carries. An .srt and a .vtt are written beside it regardless (§C).
pub const SUBTITLE_CHOICES: [&str; 3] = ["burned in", "track in file", "none in the video"];
/// §A's Translate row: the three languages the app can write subtitles in.
pub const TRANSLATE_LANGUAGES: [&str; 3] = ["English", "German", "French"];
/// §A's Quality row: CRF runs 14..34 and the slider carries a mark at 24, where the default sits.
pub const CRF_MIN: u32 = 14;
pub const CRF_MAX: u32 = 34;
pub const CRF_MARK: u32 = 24;
/// §A's three tick rows, each with the label the tick itself carries.
pub const VFR_TICK: &str = "peak rate (VFR)";
pub const MONO_TICK: &str = "mono";
pub const EDGES_TICK: &str = "blurred";

/// The options of one row, exactly as §A lists them. An unknown row has no list rather than a guessed one.
pub fn options(row: &str) -> Option<&'static [&'static str]> {
    match row {
        "Container" => Some(&CONTAINERS),
        "Codec" => Some(&CODECS),
        "Preset" => Some(&PRESETS),
        "Resolution" => Some(&RESOLUTIONS),
        "Frame rate" => Some(&FRAME_RATES),
        "Audio" => Some(&AUDIO_KBPS),
        "Subtitles" => Some(&SUBTITLE_CHOICES),
        // The list is the session's language removed from it; this is the whole menu.
        "Translate" => Some(&TRANSLATE_LANGUAGES),
        _ => None,
    }
}

/// The default of one row, spelled as §A spells it. `off`/`on` because three rows are ticks and a tick has
/// no option list to name its starting point with.
pub fn defaults() -> [(&'static str, &'static str); 13] {
    [
        ("Container", "mp4"),
        ("Codec", "h264"),
        // The spec's default; §A records that the prototype shipped veryslow.
        ("Preset", "slow"),
        ("Resolution", "1080p"),
        ("Frame rate", "30"),
        ("Audio", "128"),
        ("Subtitles", "none in the video"),
        // No language ticked: translating is asked for, never assumed.
        ("Translate", ""),
        // P.policy.gameVolume.
        ("Game audio", "0.22"),
        ("Quality (CRF)", "24"),
        ("Frame timing", "off"),
        ("Channels", "off"),
        // The one tick that starts on: a black band is the uglier answer to an odd-shaped frame.
        ("Frame edges", "on"),
    ]
}

/// §A's Tooltip column, one sentence per row — the reason for the choice, not a restatement of it.
pub fn tooltip(row: &str) -> &'static str {
    match row {
        "Container" => "mp4 plays everywhere; mkv keeps subtitle tracks best; webm forces VP9 + Opus",
        "Codec" => "h264 is the safe upload; h265 is smaller but slower; vp9 is for webm",
        "Preset" => "how long the encoder may think — slower means smaller at the same quality",
        "Resolution" => "the short side of the frame — the cut page's aspect sets its shape; original keeps the footage's own size",
        "Frame rate" => "output frame rate — a ceiling rather than a target with VFR on",
        "Audio" => "audio bitrate in kbit/s",
        "Subtitles" => "what the VIDEO carries; an .srt is written beside the video either way, one per language ticked",
        "Translate" => "Also write the subtitles in these languages, translated — this video's own language is not offered",
        // P.policy.gameVolume: the value, not a second copy of it.
        "Game audio" => "how loud the original game audio sits under the narration",
        "Quality (CRF)" => "quality: lower is better and bigger (18–24 is the usual range; 24 is the default, marked)",
        "Frame timing" => "Treat the frame rate above as a ceiling rather than a target",
        "Channels" => "Mix the finished audio down to a single channel",
        "Frame edges" => "Fill the empty edges of the frame with a blown-up, blurred copy of the picture itself, instead of black",
        _ => "",
    }
}

/// §1: the session's own language is never offered for translation — a track that repeats what the viewer
/// already hears is not a translation. An unknown language removes nothing, so all three stay offered.
pub fn translate_options(session_language: &str) -> Vec<&'static str> {
    TRANSLATE_LANGUAGES
        .into_iter()
        .filter(|language| !session_language.eq_ignore_ascii_case(language))
        .collect()
}

/// §A's two forcing rules, applied to the page's own controls: webm carries vp9 and nothing else, and a
/// subtitle track is not something webm holds, so "track in file" becomes "none in the video". Every other
/// container leaves both choices alone.
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

/// §A: the output file is not a question — `produce/final.<container>`, which is
/// [`crate::layout::Tree::final_video`]. The name only, because the folder is the project's business.
pub fn output_name(container: Container) -> String {
    let stem = match container {
        Container::Mp4 => "mp4",
        Container::Mkv => "mkv",
        Container::Webm => "webm",
    };
    format!("final.{stem}")
}

/// §A's ↻ Transcode: an encode with no model call, and the two things the render does not own left alone.
pub const TRANSCODE_TIP: &str = "Encode the video again from the cut and these settings — no model call, and the thumbnail and the upload text are left alone";

/// §A's ⤓ Save video: a copy out, so `produce/final` keeps its place as the one output that is stamped.
pub const SAVE_VIDEO_TIP: &str =
    "Save the finished video somewhere else — a copy; produce/final stays where it is";

// ---- B. the images row (§1's "Images row") ---------------------------------------

/// §1: up to 8 pictures go to the image model, the first being the base it edits.
pub const MAX_IMAGES: usize = 8;

/// §A: the empty state's first sentence — with nothing given, the instruction is all the model has.
pub const EMPTY_IMAGES: &str = "No image — the thumbnail will be drawn from the instruction alone.";

/// §A: its second sentence, the action that clears the empty state.
pub const EMPTY_IMAGES_HINT: &str = "Add image… to edit one of your own frames instead.";

/// The heading's sentence (§A): what the row is for, in the order that decides what is base.
pub const IMAGES_HEADING_TIP: &str = "What the image model is given, in order. The FIRST is the base — the picture being edited — and the others are references the instruction can name.";

/// §1: adding never replaces anything and a ninth picture is refused rather than dropping one — which
/// frame was chosen first is not something an add should be able to decide.
pub fn add_image(frames: &mut Vec<String>, path: &str) -> bool {
    if frames.len() >= MAX_IMAGES {
        return false;
    }
    frames.push(path.to_string());
    true
}

/// §10's `P.policy.publishFrames` (prototype `defPubFrames`): the first run hands the model three of the
/// frames Prepare extracted rather than none — a thumbnail drawn from an instruction alone is recognisably
/// not this video. Three is a start, not a cap: [`MAX_IMAGES`] stays the row's ceiling, and the first of
/// these three is the base the model edits ([`crate::publish::base`]).
pub const FIRST_IMAGES: usize = 3;

/// The first run's frames: up to [`FIRST_IMAGES`] of what the session offers, in the order offered, so the
/// earliest is the base. Fewer available means fewer taken; none means an empty row, which is the state
/// [`EMPTY_IMAGES`] describes rather than a reason to draw from nothing by default.
pub fn seed_frames(available: &[String]) -> Vec<String> {
    available.iter().take(FIRST_IMAGES).cloned().collect()
}

// ---- the thumbnail's own box --------------------------------------------------------

/// §10's `P.machine.thumbnailLongSide` (prototype `pubLongSide`): 1280 because that is the size every
/// uploader displays a thumbnail at, so a bigger picture is rescaled by someone else with worse settings.
/// The *long* side — a 9:16 thumbnail is 720×1280, not 1280 turned sideways.
pub const THUMB_LONG_SIDE: u32 = 1280;

/// §2's frame-box rule for the thumbnail: no aspect (or the row's `source`) means the video's own shape,
/// which with nothing to read it off is 16:9; otherwise the picture's own ratio. The longer edge stops at
/// [`THUMB_LONG_SIDE`] and both sides come out even — a chosen picture keeps its shape rather than being
/// cropped to the project's, since choosing it *is* the choice of shape.
///
/// `aspect` is `"w:h"`, or empty / `source` for "the picture's own", in which case `wide`/`high` — the
/// picture ffprobe reported — decide the shape and 16:9 is only the last resort when even those are unknown.
pub fn thumb_box(aspect: &str, wide: u32, high: u32) -> (u32, u32) {
    let own = (wide.max(1), high.max(1));
    let (w, h) = match ratio(aspect) {
        Some(shape) => shape,
        // "source", empty, or an unparseable shape: the picture's own edges, and 16:9 if nothing is known.
        None if wide > 0 && high > 0 => own,
        None => (16, 9),
    };
    let (w, h) = (w.max(1), h.max(1));
    // Scale from the longer edge so neither side is ever asked to exceed the tier.
    if w >= h {
        (THUMB_LONG_SIDE, even(THUMB_LONG_SIDE * h / w))
    } else {
        (even(THUMB_LONG_SIDE * w / h), THUMB_LONG_SIDE)
    }
}

/// `w:h` as two whole numbers, or `None` for the row's non-shapes.
fn ratio(aspect: &str) -> Option<(u32, u32)> {
    let (w, h) = aspect.split_once(':')?;
    let w: u32 = w.trim().parse().ok()?;
    let h: u32 = h.trim().parse().ok()?;
    (w > 0 && h > 0).then_some((w, h))
}

/// Rounded down to even: the encoders this page offers are all 4:2:0, which refuses a chroma sample on an
/// odd edge, and rounding up would push the long side past [`THUMB_LONG_SIDE`].
fn even(side: u32) -> u32 {
    side - side % 2
}

/// §1: `Make base` on every non-base slot. The chosen reference goes to the front and what was the base
/// joins the references behind it, so no picture is lost by re-pointing the edit. Slot 0 has no such
/// button and out-of-range slots are refused.
pub fn make_base(frames: &mut Vec<String>, slot: usize) -> bool {
    if slot == 0 || slot >= frames.len() {
        return false;
    }
    let chosen = frames.remove(slot);
    frames.insert(0, chosen);
    true
}

/// §1's remove. The last picture going leaves the empty state, not a base of nothing.
pub fn remove_image(frames: &mut Vec<String>, slot: usize) -> bool {
    if slot >= frames.len() {
        return false;
    }
    frames.remove(slot);
    true
}

/// §1's `Change…`: the slot keeps its place, only its picture changes — a reference the instruction has
/// already named stays the second image when it is replaced.
pub fn change_image(frames: &mut Vec<String>, slot: usize, path: &str) -> bool {
    let Some(slot) = frames.get_mut(slot) else {
        return false;
    };
    *slot = path.to_string();
    true
}

/// What a slot offers. `Set Thumbnail` puts the picture on the thumbnail as it is — cropped to the video's
/// shape, no model and no GPU (§A) — so it belongs on every slot; `Make base` only on a reference. The
/// remove button's label is §1's word, "remove".
pub fn slot_actions(slot: usize, count: usize) -> &'static [&'static str] {
    const BASE: [&str; 3] = ["Set Thumbnail", "Change…", "remove"];
    const REFERENCE: [&str; 4] = ["Make base", "Set Thumbnail", "Change…", "remove"];
    if slot == 0 || slot >= count {
        &BASE
    } else {
        &REFERENCE
    }
}

/// §1: the base wears a crop box when its shape differs from the video's, because the thumbnail keeps only
/// the video's own shape of it. No image, no box; the same shape needs none either.
pub fn shows_crop_box(base_shape: Option<(u32, u32)>, video_shape: (u32, u32)) -> bool {
    match base_shape {
        Some(shape) => shape != video_shape,
        None => false,
    }
}

/// §A: the instruction box is four lines, the negative prompt two — the instruction has a picture to
/// describe and the negative only has things to keep out.
pub const INSTRUCTION_LINES: usize = 4;
pub const NEGATIVE_LINES: usize = 2;

/// §A's Edit instruction tooltip: what the model is told, and the rule that words never come from here.
pub const INSTRUCTION_TIP: &str = "What the image model is told to make out of the images. No words: the title and the marked texts are printed on afterwards.";

/// §A's Negative prompt tooltip.
pub const NEGATIVE_TIP: &str = "What must stay out of the picture — watermarks, logos, lettering, extra limbs.";

// ---- C. the thumbnail and its text overlays (§1) ---------------------------------

/// §A's ⤓ export: what YouTube takes is a JPEG under 2 MB, so the export says which limit it keeps under.
pub const EXPORT_TIP: &str = "Export the thumbnail as a JPEG — what YouTube takes, under its 2 MB limit";

/// §A's ↻ redraw: the same images and instruction, a fresh draw.
pub const REDRAW_TIP: &str = "Draw the thumbnail again from the images and the instruction as they stand — a fresh draw";

/// §A ("Text overlays"): the picture column's width, the smallest box that can hold words, the snap
/// distance to the picture's edges and middles and to other boxes, and the ✎ chip's size.
pub const OVERLAY_EDGE_PX: f64 = 200.0;
pub const OVERLAY_MIN_PX: f64 = 28.0;
pub const OVERLAY_SNAP_PX: f64 = 10.0;
pub const OVERLAY_CHIP_PX: f64 = 22.0;

/// A box smaller than [`OVERLAY_MIN_PX`] in either direction cannot be read as words, so a drag that ends
/// there is not a box. Pixels, because the minimum is what a hand leaves behind on the picture.
pub fn box_is_too_small(w_px: f64, h_px: f64) -> bool {
    w_px < OVERLAY_MIN_PX || h_px < OVERLAY_MIN_PX
}

/// A drag lands on the nearest snap target — a picture edge or middle, or another box's edge — when it came
/// within [`OVERLAY_SNAP_PX`] of one; otherwise it stays where the hand left it. §A's snapping rule, in the
/// page's own pixels.
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

/// §1: dragging a box over the picture puts words on it, printed to fill the box like a Text effect in Cut.
/// The geometry is stored as fractions of the picture — [`crate::project::TextMark`]'s own shape — so a
/// resize of the column never rescales the words' position. An empty text is not a mark: ✎ with nothing in
/// it removes rather than keeps a blank box.
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

/// §1's ✎ remove: the box is gone, the others keep their order — a reword dialog that removed the wrong box
/// would otherwise move words nobody touched.
pub fn remove_words(marks: &mut Vec<TextMark>, index: usize) -> bool {
    if index >= marks.len() {
        return false;
    }
    marks.remove(index);
    true
}

/// §A: the title band's default box, in fractions of the picture — across the top, on its own. The title is
/// printed there and nowhere else, which is why it is a box rather than part of the picture.
pub const TITLE_BOX_DEFAULT: (f64, f64, f64, f64) = (0.5, 0.25, 1.0, 0.4);

// ---- D. title, description, suggest (§1) -----------------------------------------

/// §A's ↻ suggest: the only thing on this page that asks a model to rewrite text, and it rewrites all three
/// of the written things at once so they cannot disagree about what the video is.
pub const SUGGEST_TIP: &str = "Ask the model for a fresh title, thumbnail instruction and description — the only thing that does";

/// §A's title entry placeholder.
pub const TITLE_PLACEHOLDER: &str =
    "the video's title, also printed on the thumbnail — ▶ suggests one";

/// §A's title tooltip: the first thumbnail takes the title as its printed line; after that the two are
/// separate, because a title retyped for YouTube should not silently repaint a published picture.
pub const TITLE_TIP: &str = "Four to seven words. The first thumbnail takes it as its printed line; after that the two are separate.";

/// §A's description box: eight lines, which is roughly what YouTube shows before "more".
pub const DESCRIPTION_LINES: usize = 8;

/// §1 (`printed across the thumbnail the first time only; then separate`): the first draw prints the title,
/// and after that the printed line is the thumbnail's own. `title_seeded` is the page's memory of having
/// printed it, so a retitled project never repaints a picture somebody chose. True when this call copied.
pub fn seed_title(publish: &mut Publish) -> bool {
    if publish.title_seeded || !publish.thumb_title.trim().is_empty() {
        return false;
    }
    if publish.title.trim().is_empty() {
        return false;
    }
    publish.thumb_title = publish.title.clone();
    publish.title_seeded = true;
    true
}

// ---- E. Inputs / Outputs readouts (§1's last bullet) ------------------------------

/// §1: with no cut there is nothing to produce, and the page says it in the other pages' words.
pub const NO_CUT_LINE: &str = crate::narrate_details::NO_CUT_LINE;

/// The Inputs row: `N clip(s) · mm:ss`, then which of the two narration states applies — never both — then
/// whether the upload text exists yet. `to_speak` counts the lines ▶ has to speak, which is what makes the
/// wait legible before anything runs.
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

/// The Inputs tooltip: what will be spoken first, which recordings are mixed, the voice, and whether
/// `publish.json` exists. A row is dropped when there is nothing to say for it — a tooltip of "none"s reads
/// as a broken page rather than an empty one.
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

/// The Outputs row counts, and only counts (§1): the page owns a folder of files whose sizes mean nothing
/// until one of them is opened. MB above 1, KB below — a folder of 200 kB should not read as "0 MB".
pub fn outputs_summary(files: usize, bytes: u64) -> String {
    const MIB: u64 = 1024 * 1024;
    if files == 0 {
        return "0 files".to_string();
    }
    if bytes < MIB {
        return format!("{files} files, {} KB", (bytes + 1023) / 1024);
    }
    format!("{files} files, {} MB", (bytes + MIB / 2) / MIB)
}

/// §1's folder-button tooltip: the four things this step writes, in the order a person looks for them.
pub const FOLDER_TIP: &str =
    "produce/ \u{2014} the finished video, the per-clip encodes, the thumbnail and the upload text";
