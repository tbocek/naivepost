//! F5.7 Page runs — `spec/08-produce.md` §F5.7, wording in `spec/inventory/produce.md`. Eight buttons, one rule each: what a press asks the models for and leaves alone.
//!
//! A picture is the only expensive thing (one sd.cpp call; the other runs are at most an LLM call), so a redraw keeps the words and a re-word keeps the picture.
//! [`transcode_args`] needs no model and reuses [`crate::produce_render`]'s argv builders. No GTK here and nothing is spawned; the dialog's drag geometry lives in [`crate::fx_text`].

use crate::project::{Container, Produce, TitleBox};

// ---- S1: Redraw ------------------------------------------------------------------

pub const DRAW_AGAIN_LOG: &str =
    "publish: drawing the thumbnail again — one sd.cpp call, nothing rewritten";

/// The redraw's own no-cut answer; the thumbnail can also come from the cut's own frames, so this says which one is awaited.
pub const REDRAW_NO_CUT: &str =
    "no cut yet \u{2014} the thumbnail is drawn from the cut's own frames";

/// The redraw's ending; `\u{25b6}` is spelled in because a video still has to render.
pub const REDRAW_DONE: &str = "thumbnail drawn \u{2014} \u{25b6} renders the video";

/// ↻ beside Title's no-cut answer: the Cut step's own sentence, re-exported so both presses read refusals from this module.
pub const REWORD_NO_CUT: &str = crate::cut::NO_CUT_YET;

/// …and its ending, naming both things it rewrote.
pub const REWORD_DONE: &str = "title and description rewritten \u{2014} \u{25b6} renders the video";

/// S3's sentence: the chosen picture arrives with the words printed on it, ↻ takes them off. [`take_from_image`]'s log records the *copy*.
pub fn taken_log(file: &str) -> String {
    let base = file.rsplit('/').next().unwrap_or(file);
    format!(
        "thumbnail taken from {base} \u{2014} the words are printed on it; \u{21bb} draws over it"
    )
}

/// §F5.7: both runs end through ONE path. Success hands back the caller's done line; a stop is reported apart from a failure (the log behind "see log" differs).
pub fn ending(what: &str, done: &str, ok: bool, stopped: bool) -> String {
    if ok {
        return done.to_string();
    }
    if stopped {
        return format!("{what} stopped");
    }
    format!("{what} failed \u{2014} see log")
}

/// What ⤓ prints once the JPEG is written: the path and the weight that decided the rung of [`JPEG_QUALITIES`].
pub fn exported_log(path: &str, bytes: u64) -> String {
    format!(">>> exported {path} ({})", human_size(bytes))
}

/// What ⤓ Save video says with no video to save; names ▶, the button that fixes it.
pub const SAVE_NO_VIDEO: &str = "nothing to save yet \u{2014} \u{25b6} renders the video first";

/// The copy's opening line: a whole-video copy is slow enough that silence reads as a hang.
pub fn saving_log(file: &str) -> String {
    format!("saving {file}\u{2026}")
}

/// …and its close, with the size.
pub fn saved_log(file: &str, bytes: u64) -> String {
    format!("saved {file} \u{2014} {}", human_size(bytes))
}

/// A file's weight for the logs: whole MiB, one decimal below that so 1.4 MiB never reads "0 MiB".
fn human_size(bytes: u64) -> String {
    const MIB: f64 = 1024.0 * 1024.0;
    let mib = bytes as f64 / MIB;
    if mib >= 10.0 || bytes.is_multiple_of(1024 * 1024) {
        format!("{} MiB", mib as u64)
    } else {
        format!("{mib:.1} MiB")
    }
}

/// S6: which rung of [`JPEG_QUALITIES`] the export lands on, given each attempt's encoded size in ladder order; the FIRST fitting rung wins.
///
/// When nothing fits, the LAST attempt is returned (§F5.7 writes it over the cap): no smaller picture is left to offer. The caller reports the overshoot.
pub fn jpeg_rung(sizes: &[u64]) -> Option<usize> {
    if sizes.is_empty() {
        return None;
    }
    match sizes.iter().position(|bytes| *bytes <= JPEG_MAX_BYTES) {
        Some(rung) => Some(rung),
        None => Some(sizes.len() - 1),
    }
}

/// What one redraw press does; `keeps` is what a test pins as untouched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Redraw {
    /// sd.cpp calls — one, whatever the box says. A second draw would be a second picture, not a redraw.
    pub sd_calls: usize,
    pub keeps: &'static [&'static str],
}

/// S1: one sd.cpp call and no rewritten text.
pub fn redraw() -> Redraw {
    Redraw {
        sd_calls: 1,
        keeps: &["title", "description", "publish record", "chapter times"],
    }
}

// ---- S2: Reword ------------------------------------------------------------------

/// The one-LLM-call run's log line, naming both answers.
pub const REWORD_LOG: &str = "publish: rewriting the title and description — one LLM call";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reword {
    pub llm_calls: usize,
    pub answers: &'static [&'static str],
    pub keeps: &'static [&'static str],
}

/// S2: ask again for the words: one LLM call, two answers; never an sd.cpp call (mirror of [`redraw`]).
pub fn reword() -> Reword {
    Reword {
        llm_calls: 1,
        answers: &["title", "description"],
        keeps: &["thumbnail", "publish record", "chapter times"],
    }
}

// ---- S3: Set from image ----------------------------------------------------------

/// The tooltip on a chosen picture: the copy makes it survive, and the redraw means the user's own file is never edited.
pub const CHOSEN_TIP: &str =
    "A copy of this picture is kept, so it survives reopening the project. Use it to \
redraw from — it does not replace your file.";

/// What the row says when the copy has gone missing between sessions; the choice stays.
pub const CHOSEN_REMOVED: &str = "thumbnail not found";

#[derive(Debug, Clone, PartialEq)]
pub struct Chosen {
    /// Where the copy lives — the app's own images folder ([`images_dir`]), named from the chosen file's stem.
    pub stored: String,
    /// The box it is drawn into: the picture's own aspect, scaled to fit [`WIDTH`] wide.
    pub box_: (u32, u32),
    pub log: String,
}

/// The width every thumbnail is drawn at (§08's thumbnail row; see [`crate::render_fx::TIER_SHORT_SIDE`]).
pub const WIDTH: u32 = 1920;

/// Round down to the nearest even number: 4:2:0 chroma needs it, and rounding up would exceed `WIDTH`.
pub(crate) fn even(side: u32) -> u32 {
    side - side % 2
}

// ---- S4: Re-ask ------------------------------------------------------------------

// ---- S5: Words on the thumbnail ---------------------------------------------------

pub const WORDS_DIALOG: &str = "Words on the thumbnail";

/// Its hint: one line, resized to fill the box, so a longer line comes out smaller.
pub const WORDS_DIALOG_TIP: &str =
    "Printed to fill the box you marked — a longer line comes out smaller.";

/// Why the box stops on a line: words that run off the picture are words a player crops or its controls cover.
pub const WORDS_SNAP_TIP: &str =
    "The box snaps to a grid, and back inside the picture when it would leave it.";

/// The preview is drawn with the export's text, width and position, not sd.cpp's; the model writes no letters.
pub const WORDS_PREVIEW_TIP: &str =
    "The same text, width and position as the exported picture — this is what \
will be printed, over the drawn image.";

pub const WORDS_PLACEMENT: &str =
    "Printed over the drawn one — the machine never re-asks the model to write them";

/// The box a press starts with when the project has none: the lower third ([`crate::fx_text::LOWER_THIRD`]).
pub const fn words_box() -> TitleBox {
    let lower = crate::fx_text::LOWER_THIRD;
    TitleBox {
        cx: lower.cx,
        cy: lower.cy,
        wf: lower.wf,
        hf: lower.hf,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Reprint {
    /// The box in pixels of the drawn picture: x, y, width, height.
    pub box_px: (u32, u32, u32, u32),
    /// The type size that fills the box, from [`crate::fx_text::fit`].
    pub size: f64,
    pub lines: Vec<String>,
    /// sd.cpp calls — none: printing letters is a local draw.
    pub sd_calls: usize,
}

/// S5: change the words or move their box. No model call: the text is printed over the existing picture. The box is stored as a fraction of
/// the picture ([`words_box`]) and comes out in pixels here, since preview and export are the same draw at different scales.
pub fn reprint(words: &str, box_: TitleBox) -> Reprint {
    let band = crate::fx_text::Box_ {
        cx: box_.cx,
        cy: box_.cy,
        wf: box_.wf,
        hf: box_.hf,
    };
    let (x, y, wide, high) = band.to_px(f64::from(WIDTH), f64::from(HEIGHT));
    // The same fitter the Cut page uses, so the thumbnail prints words as an effect does.
    let (size, lines) = crate::fx_text::fit(words, wide, high);
    Reprint {
        box_px: (x as u32, y as u32, wide as u32, high as u32),
        size,
        lines,
        sd_calls: 0,
    }
}

/// The grid a dragged box stops on, in pixels of the drawn picture: [`crate::fx_text::SNAP_PX`] scaled up (10 px on a 1920-wide picture would not be felt).
pub fn words_snap_px() -> u32 {
    (crate::fx_text::SNAP_PX * SNAP_SCALE) as u32
}

/// How much larger the drawn thumbnail is than the preview [`crate::fx_text::SNAP_PX`] was measured on: 1920 over 640.
pub const SNAP_SCALE: f64 = 3.0;

pub const HEIGHT: u32 = 1080;

// ---- S6: export a JPEG ------------------------------------------------------------

/// The quality ladder, best first. Quality drops before anything else because the size is already right (a rescaled picture is a smaller thumbnail, not a lighter file).
/// The last attempt is written even when it still does not fit.
pub const JPEG_QUALITIES: [u32; 5] = [92, 85, 75, 60, 40];

/// The rungs worth reporting: [`jpeg_target`]'s message names the first three that were tried.
pub const JPEG_RETRY_LIMIT: usize = 3;

/// The largest thumbnail an upload accepts. Catalogued as `P.eng.jpegMaxBytes`.
pub const JPEG_MAX_BYTES: u64 = 2 * 1024 * 1024;

/// S6: is a picture of this size worth handing to an uploader? YouTube refuses above [`JPEG_MAX_BYTES`] silently, so the refusal happens here.
pub fn jpeg_target(bytes: u64) -> Result<(), String> {
    if bytes <= JPEG_MAX_BYTES {
        return Ok(());
    }
    let tried = JPEG_QUALITIES[..JPEG_RETRY_LIMIT]
        .iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join("/");
    let mib = JPEG_MAX_BYTES / (1024 * 1024);
    Err(format!(
        "JPEG exceeds {mib} MiB after {tried} — check the sd.cpp encoder"
    ))
}

/// S6's default name: `<stem>.jpg` beside the video, a sibling of the `.html` and `.vtt` files.
pub fn export_name(video_stem: &str) -> String {
    format!("{video_stem}.jpg")
}

/// What ⤓ says when there is no picture to export; names both ways out.
pub const EXPORT_NO_PICTURE: &str =
    "nothing to export yet — draw a thumbnail, or use one of the images";

/// The name ⤓ offers in the save dialog: the project's own name, not the video's stem. `⊕ Set from image` keeps its own name (a copy of a file on disk).
pub fn export_default(project_name: &str) -> String {
    format!("{project_name}-thumbnail.jpg")
}

/// The extension ⤓ writes, whatever the dialog was typed with: an uploader reads `.jpg` and nothing else.
pub const EXPORT_SUFFIX: &str = ".jpg";

/// Put `suffix` on `name`, replacing any extension (§F5.7: "extension forced to `.jpg`"); a name whose only dot is first (`.thumbs`) gets it appended.
pub fn force_suffix(name: &str, suffix: &str) -> String {
    // Only a dot with something before it is an extension: `.git` is a whole name, not a nameless stem.
    match name.rfind('.') {
        Some(dot) if dot > 0 => format!("{}{suffix}", &name[..dot]),
        _ => format!("{name}{suffix}"),
    }
}

/// What ↻ Save video offers as its default name.
pub fn save_default(project_name: &str, container: Container) -> String {
    format!("{project_name}.{}", container_name(container))
}

// ---- S7: Save a copy of the video --------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Copy {
    pub from: String,
    /// The copy is not the render's output: [`crate::produce_stamp`] reads the file ▶ wrote, so a copy elsewhere leaves the video up to date.
    pub copy_is_source: bool,

    pub log: String,
}

/// S7: ⤓ Save video. Refused while a render runs (a half-written file would look finished). `bytes` is the source's weight, for the log.
pub fn copy_plan(video: &str, _to: &str, running: bool, bytes: u64) -> Result<Copy, String> {
    if running {
        return Err("finish the render first".to_string());
    }
    Ok(Copy {
        from: video.to_string(),
        copy_is_source: false,
        log: format!("copy written — {} MiB copied", mib(bytes)),
    })
}

/// Bytes as the whole MiB the copy log prints (rounding up would overstate).
fn mib(bytes: u64) -> u64 {
    bytes / (1024 * 1024)
}

// ---- S8: Transcode a file ---------------------------------------------------------

/// The first log line of a transcode, naming the three settings it read off the row.
pub fn transcode_started(file: &str, settings: &Produce) -> String {
    format!(
        "transcode started — {file}, {}/{}, {}",
        container_name(settings.container),
        codec_name(settings.codec),
        resolution_name(settings.resolution)
    )
}

/// The last log line of a transcode, the only record it leaves (no stamp).
pub fn transcode_finished(file: &str) -> String {
    format!("transcode finished — {file}")
}

/// `tool:ffmpeg.encode` at its two ends: starts at 5%, reports nothing until 95%, since ffmpeg's percentage is against a duration unknown for an arbitrary file.
pub const TRANSCODE_START: f64 = 0.05;

pub const TRANSCODE_END: f64 = 0.95;

/// S8: ↻ Transcode is refused while a render runs (two encodes at once halve both and progress lines would mix). `args` is what [`transcode_args`] built;
/// an empty list means there was no file to read settings from, also refused.
pub fn transcode_plan(video: &str, args: &[String], running: bool) -> Result<(), String> {
    if running {
        return Err("wait for the current render to finish — ⊘ Cancel first".to_string());
    }
    if args.is_empty() {
        return Err(format!("nothing to transcode — {}", video));
    }
    Ok(())
}

/// The command for a standalone file: [`crate::produce_render`]'s codec, frame-rate and container arguments with `-i <source>`, and **no subtitle track**:
/// a file picked from disk has no transcript to mux. Shared with [`crate::produce_render::encode_command`] so codec-table changes land on both.
pub fn transcode_args(source: &str, out: &str, settings: &Produce) -> Vec<String> {
    // `-y`: the file it writes is the one an earlier transcode wrote, and ffmpeg asks before overwriting.
    let mut argv = vec![
        "ffmpeg".into(),
        "-y".into(),
        "-i".into(),
        source.to_string(),
    ];
    // No -ss: this file is the whole thing, seeked or not.
    argv.extend(crate::produce_render::codec_args(settings));
    argv.extend(crate::produce_render::frame_rate_args(
        crate::produce_render::frame_rate_name(settings.frame_rate),
        settings.vfr,
    ));
    argv.extend([
        "-c:a".into(),
        crate::produce_render::audio_codec(settings.container).to_string(),
        "-b:a".into(),
        format!("{}k", settings.audio_kbps),
        "-ac".into(),
        if settings.mono {
            "1".into()
        } else {
            "2".into()
        },
    ]);
    // `track in file` is not consulted: a file picked off disk has no transcript to mux.
    if settings.container == Container::Mp4 {
        argv.extend(
            crate::produce_render::MP4_STREAM_FLAGS
                .iter()
                .chain(crate::produce_render::MP4_START_FLAGS.iter())
                .map(|flag| flag.to_string()),
        );
    }
    argv.push(out.to_string());
    argv
}

fn container_name(container: Container) -> &'static str {
    match container {
        Container::Mp4 => "mp4",
        Container::Mkv => "mkv",
        Container::Webm => "webm",
    }
}

fn codec_name(codec: crate::project::Codec) -> &'static str {
    match codec {
        crate::project::Codec::H264 => "h264",
        crate::project::Codec::H265 => "h265",
        crate::project::Codec::Vp9 => "vp9",
    }
}

/// The resolution as the log spells it. A transcode never scales (§F5.7's no-rescaling rule).
fn resolution_name(resolution: crate::project::Resolution) -> &'static str {
    use crate::project::Resolution;
    match resolution {
        Resolution::P720 => "720p",
        Resolution::P1080 => "1080p",
        Resolution::Original => "original",
    }
}
