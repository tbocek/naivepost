//! F5.7 Page runs — `spec/08-produce.md` §F5.7, wording in `spec/inventory/produce.md`. Eight buttons, one
//! rule each: what a press asks the models for and what it leaves alone.
//!
//! The page's whole cost story is here, so the numbers are worth naming once:
//! - **A picture is the only expensive thing.** One sd.cpp call draws it; every other run on this page is an LLM
//!   call at most, and four of the eight ask for nothing at all. That is why `⊙ Thumbnail only` exists as a
//!   checkbox rather than as two buttons: redrawing without re-writing is the common case after a first good
//!   picture, and paying for the words again is the mistake the box stops.
//! - **Nothing here rewrites anything else.** A redraw keeps the title, the description, the publish record and
//!   the chapter times; a re-word keeps the picture. The two runs are complements, and each test pins what the
//!   other one must not touch.
//! - **`tool:ffmpeg.encode` is the only tool the last run needs**, and it needs no model at all — a transcode
//!   reads its settings off the same row the render does, which is why [`transcode_args`] reuses
//!   [`crate::produce_render`]'s argv builders instead of owning a second copy of the codec table.
//!
//! No GTK here and nothing is spawned: like [`crate::produce_render`] and [`crate::produce_embed`] this builds
//! argv, paths and log lines. The dialog's drag geometry lives in [`crate::fx_text`]; what belongs to this page
//! is only which box the words get printed into and that printing them costs no model call.

use crate::project::{Container, Produce, TitleBox};

// ---- S1: Thumbnail only -----------------------------------------------------------

/// The checkbox's label (`spec/inventory/produce.md`: `⊙ Thumbnail only`).
pub const ONLY_THUMBNAIL: &str = "⊙ Thumbnail only";

/// Its tooltip. Written from the ticked side because that is what the box is for: ticking it is the deliberate
/// act, and the sentence has to say what the tick *saves*.
pub const ONLY_THUMBNAIL_TIP: &str = "Ticked: the next run draws the picture again and re-asks for the words \
too. Leave it ticked to redraw from the instruction as it stands.";

/// What a press with the box ticked costs and keeps, in the log.
pub const DRAW_AGAIN_LOG: &str = "publish: drawing the thumbnail again — one sd.cpp call, nothing rewritten";

/// The words box when a redraw is about to ask for them again: the previous text is offered back rather than an
/// empty field, because the usual reason for a second run is that the picture was wrong, not the words.
pub const WORDS_ASK_AGAIN: &str = "Words: <previous text>";

/// What one redraw press does. `keeps` is the list a test pins: everything the run must leave untouched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Redraw {
    /// sd.cpp calls — one, whatever the box says. A second draw would be a second picture, not a redraw.
    pub sd_calls: usize,
    /// LLM calls that rewrite text. Ticked asks for the words again; the picture never comes from the LLM.
    pub rewrites: usize,
    /// What survives the press unchanged.
    pub keeps: &'static [&'static str],
}

/// S1: `⊙ Thumbnail only` ticked means one sd.cpp call and no rewritten text. Unticked, the same press also
/// re-asks for the title, the instruction and the description — which is [`reword`]'s single LLM call, so the
/// box's whole job is to skip it.
pub fn redraw(only_thumbnail: bool) -> Redraw {
    Redraw {
        sd_calls: 1,
        rewrites: if only_thumbnail { 0 } else { 1 },
        keeps: &["title", "description", "publish record", "chapter times"],
    }
}

// ---- S2: Reword ------------------------------------------------------------------

/// The one-LLM-call run's log line. Naming the three answers is the point: a reader of the log can tell this
/// press from a redraw without knowing which button was pressed.
pub const REWORD_LOG: &str = "publish: rewriting the title, instruction and description — one LLM call";

/// What a re-word costs and keeps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reword {
    pub llm_calls: usize,
    /// The three answers the one call gives back.
    pub answers: &'static [&'static str],
    pub keeps: &'static [&'static str],
}

/// S2: ask again for the words — one LLM call, three answers, and the picture it prints stays as it is. The
/// mirror image of [`redraw`]: this run never costs an sd.cpp call.
pub fn reword() -> Reword {
    Reword {
        llm_calls: 1,
        answers: &["title", "instruction", "description"],
        keeps: &["thumbnail", "publish record", "chapter times"],
    }
}

// ---- S3: Set from image ----------------------------------------------------------

/// The tooltip on a chosen picture. Two halves because the copy is what makes it survive and the redraw is
/// what makes it not a file operation: the app never edits the user's own file.
pub const CHOSEN_TIP: &str = "A copy of this picture is kept, so it survives reopening the project. Use it to \
redraw from — it does not replace your file.";

/// What the row says when the copy has gone missing between sessions. The choice stays; only its answer is gone.
pub const CHOSEN_REMOVED: &str = "thumbnail not found";

/// A picture chosen as the thumbnail.
#[derive(Debug, Clone, PartialEq)]
pub struct Chosen {
    /// Where the copy lives — the app's own images folder ([`images_dir`]), named from the chosen file's stem.
    pub stored: String,
    /// The box it is drawn into: the picture's own aspect, scaled to fit [`WIDTH`] wide.
    pub box_: (u32, u32),
    pub log: String,
}

/// The width every thumbnail is drawn at — §08's thumbnail row, and the widest side of the frame tier in
/// [`crate::render_fx::TIER_SHORT_SIDE`]'s terms.
pub const WIDTH: u32 = 1920;

/// S3: use this file as the thumbnail instead of drawing one. The copy is what makes it survive a reopening;
/// the box is the picture's own shape rather than the project's, because choosing a picture *is* the choice of
/// shape. `at_seconds` is where the frame came from and only reaches the log — a chosen file has no clock in it.
pub fn take_from_image(file: &str, wide: u32, high: u32, at_seconds: f64) -> Chosen {
    let name = file.rsplit('/').next().unwrap_or(file);
    // The stem is everything up to the *last* dot: a frame named `shot.final.png` keeps its middle.
    let stem = name.rsplit_once('.').map_or(name, |(stem, _)| stem);
    let (w, h) = fit(wide.max(1), high.max(1));
    Chosen {
        stored: format!("{}/{stem}.png", images_dir()),
        box_: (w, h),
        log: format!(
            "thumbnail taken from {name} — scaled to {w}×{h} at {at_seconds:.1}s"
        ),
    }
}

/// The app's own images folder: `XDG_CONFIG_HOME`/naivepost/images (`~/.config` when unset), so a picture the
/// page keeps lives where its settings live. Kept off [`crate::settings::Paths`] because nothing else in the
/// project reads it — this is scratch that outlives a session, not configuration.
pub fn images_dir() -> String {
    let root = match std::env::var("XDG_CONFIG_HOME") {
        Ok(root) if !root.is_empty() => root,
        _ => match std::env::var("HOME") {
            Ok(home) if !home.is_empty() => format!("{home}/.config"),
            _ => String::new(),
        },
    };
    format!("{root}/naivepost/images")
}

/// S3's other half: what the row says about a choice whose copy is gone. The choice itself stays — the next run
/// still uses the picture if it comes back — so this reports the loss rather than undoing the press.
pub fn chosen_state(copy_exists: bool) -> Option<&'static str> {
    (!copy_exists).then_some(CHOSEN_REMOVED)
}

/// Scale into `WIDTH` wide, keeping the shape and landing on even sides — 4:2:0 encoders refuse an odd edge, so
/// a picture that came out 1919×1079 would be refused at encode time rather than here.
fn fit(wide: u32, high: u32) -> (u32, u32) {
    if wide >= high {
        (WIDTH, even(WIDTH * high / wide))
    } else {
        (even(WIDTH * wide / high), WIDTH)
    }
}

/// Round down to the nearest even number: 4:2:0 chroma needs it, and rounding up would exceed `WIDTH`.
fn even(side: u32) -> u32 {
    side - side % 2
}

// ---- S4: Re-ask ------------------------------------------------------------------

/// S4: the ask-again button hands back an empty box, never the text it is replacing. Offering the old words
/// again would be the same mistake a redraw-without-the-box makes: this press says those words were wrong.
pub fn re_ask(text: &str) -> String {
    let _ = text;
    String::new()
}

// ---- S5: Words on the thumbnail ---------------------------------------------------

/// The dialog's title.
pub const WORDS_DIALOG: &str = "Words on the thumbnail";

/// Its hint: one line, resized to fill whatever box is marked. A longer line therefore comes out smaller — the
/// only lever a person has once the box is set.
pub const WORDS_DIALOG_TIP: &str = "Printed to fill the box you marked — a longer line comes out smaller.";

/// Why the box stops on a line: words that run off the picture are words a player crops or its controls cover.
pub const WORDS_SNAP_TIP: &str = "The box snaps to a grid, and back inside the picture when it would leave it.";

/// What the preview promises: it is drawn with the export's text, width and position, not with whatever sd.cpp
/// painted. The model writes no letters (the instruction asks for none), so there is nothing to disagree with.
pub const WORDS_PREVIEW_TIP: &str = "The same text, width and position as the exported picture — this is what \
will be printed, over the drawn image.";

/// Where the words land relative to the drawn picture.
pub const WORDS_PLACEMENT: &str = "Printed over the drawn one — the machine never re-asks the model to write them";

/// The box a press starts with when the project has never had one: the lower third, the same default every other
/// words box in the app opens at ([`crate::fx_text::LOWER_THIRD`] — named here so this page's geometry is read
/// from that module rather than copied).
pub const fn words_box() -> TitleBox {
    let lower = crate::fx_text::LOWER_THIRD;
    TitleBox { cx: lower.cx, cy: lower.cy, wf: lower.wf, hf: lower.hf }
}

/// A press inside the words dialog.
#[derive(Debug, Clone, PartialEq)]
pub struct Reprint {
    /// The box in pixels of the drawn picture: x, y, width, height.
    pub box_px: (u32, u32, u32, u32),
    /// The type size that fills the box, from [`crate::fx_text::fit`] — smaller for a longer line, which is the
    /// whole of what "a longer line comes out smaller" means.
    pub size: f64,
    /// The line as it will be printed, already wrapped to the box.
    pub lines: Vec<String>,
    /// sd.cpp calls — none: printing letters is a local draw.
    pub sd_calls: usize,
}

/// S5: change the words or move their box. Nothing about it costs a model call — the text is printed over the
/// picture that already exists, so moving the box never redraws it. The box is stored as a fraction of the
/// picture ([`words_box`]) and comes out in pixels here, because the preview and the export are the same draw at
/// different scales and only the fractions travel between them.
pub fn reprint(words: &str, box_: TitleBox) -> Reprint {
    let band = crate::fx_text::Box_ { cx: box_.cx, cy: box_.cy, wf: box_.wf, hf: box_.hf };
    let (x, y, wide, high) = band.to_px(f64::from(WIDTH), f64::from(HEIGHT));
    // One call to the Cut page's own fitter: the thumbnail prints words exactly as an effect prints them, so a
    // second size rule here would be two answers to "how big can these letters get".
    let (size, lines) = crate::fx_text::fit(words, wide, high);
    Reprint {
        box_px: (x as u32, y as u32, wide as u32, high as u32),
        size,
        lines,
        sd_calls: 0,
    }
}

/// The grid a dragged box stops on, in pixels of the drawn picture. [`crate::fx_text::SNAP_PX`] is measured on
/// the Cut page's preview; the thumbnail is drawn far larger than that preview, so the same tenth-of-the-picture
/// allowance is scaled up rather than reused as a raw pixel count — 10 px of snap on a 1920-wide picture would
/// not be felt under the hand.
pub fn words_snap_px() -> u32 {
    (crate::fx_text::SNAP_PX * SNAP_SCALE) as u32
}

/// How much larger the drawn thumbnail is than the preview [`crate::fx_text::SNAP_PX`] was measured on: 1920 over
/// that page's 640-wide preview.
pub const SNAP_SCALE: f64 = 3.0;

/// The height every thumbnail is drawn at — the other side of the frame tier [`WIDTH`] names.
pub const HEIGHT: u32 = 1080;

// ---- S6: export a JPEG ------------------------------------------------------------

/// The quality ladder, best first — §F5.7's full run of them, and the three that must be tried before the
/// export admits defeat ([`jpeg_target`]'s message names exactly these). Quality drops before anything else
/// because the size is already right: a rescaled picture that fits 2 MiB is a smaller thumbnail, not a lighter
/// file of the same one. Two further rungs exist (`60`, `40`) and are tried after these; the last attempt is
/// written even when it still does not fit.
pub const JPEG_QUALITIES: [u32; 5] = [92, 85, 75, 60, 40];

/// The rungs whose failure is worth reporting: everything past [`JPEG_RETRY_LIMIT`] is a rung nobody chose to
/// show the user, so naming three of them keeps the message about what was *tried*, not about an encoder's table.
pub const JPEG_RETRY_LIMIT: usize = 3;

/// The largest thumbnail an upload accepts. Catalogued as `P.eng.jpegMaxBytes`.
pub const JPEG_MAX_BYTES: u64 = 2 * 1024 * 1024;

/// S6: is a picture of this size worth handing to an uploader? YouTube refuses above [`JPEG_MAX_BYTES`] with no
/// reason on screen, so the refusal happens here, after every rung of [`JPEG_QUALITIES`] has been tried.
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
    Err(format!("JPEG exceeds {mib} MiB after {tried} — check the sd.cpp encoder"))
}

/// S6's default name: `<stem>.jpg` beside the video, so the exported file is a sibling of the `.html` and the
/// `.vtt` files rather than a fourth name kept in step by hand.
pub fn export_name(video_stem: &str) -> String {
    format!("{video_stem}.jpg")
}

/// What ⤓ says when there is no picture to export. Naming both ways out of it is the point — one of the two is
/// usually what the user meant.
pub const EXPORT_NO_PICTURE: &str = "nothing to export yet — draw a thumbnail, or use one of the images";

/// The name ⤓ offers in the save dialog: the project's own name rather than the video's stem, because at this
/// point the user is naming a picture they are about to upload somewhere else. `⊕ Set from image` keeps its own
/// name instead — that file is a copy of something already on disk.
pub fn export_default(project_name: &str) -> String {
    format!("{project_name}-thumbnail.jpg")
}

/// The extension ⤓ writes, whatever the save dialog was typed with — an uploader that reads `.jpg` and nothing
/// else is the reason the name is forced rather than suggested.
pub const EXPORT_SUFFIX: &str = ".jpg";

/// What ↻ Save video offers as its default name.
pub fn save_default(project_name: &str, container: Container) -> String {
    format!("{project_name}.{}", container_name(container))
}

// ---- S7: Save a copy of the video --------------------------------------------------

/// A request to copy the rendered video somewhere else.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Copy {
    pub from: String,
    /// The copy is not the render's output: [`crate::produce_stamp`] reads the file ▶ wrote, so a copy written
    /// beside it or on another disk leaves the video up to date.
    pub copy_is_source: bool,

    pub log: String,
}

/// S7: ⤓ Save video. Refused while a render runs — a half-written file copied out is a short video that looks
/// finished. `bytes` is what the source weighs, so the log can say what was copied.
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

/// Bytes as the whole MiB the log prints. Rounding up would promise a file bigger than what was copied.
fn mib(bytes: u64) -> u64 {
    bytes / (1024 * 1024)
}

// ---- S8: Transcode a file ---------------------------------------------------------

/// The first log line of a transcode, naming the three settings it read off the row — the ones that decide what
/// comes out.
pub fn transcode_started(file: &str, settings: &Produce) -> String {
    format!(
        "transcode started — {file}, {}/{}, {}",
        container_name(settings.container),
        codec_name(settings.codec),
        resolution_name(settings.resolution)
    )
}

/// The last log line of a transcode. A transcode has no stamp to write, so this is the only record it leaves.
pub fn transcode_finished(file: &str) -> String {
    format!("transcode finished — {file}")
}

/// `tool:ffmpeg.encode` at its two ends: the run starts at 5% and reports nothing until 95%, because ffmpeg's
/// own percentage is against a duration this app does not know for an arbitrary file.
pub const TRANSCODE_START: f64 = 0.05;

/// …and the same bar's far end.
pub const TRANSCODE_END: f64 = 0.95;

/// S8: ↻ Transcode is refused while a render runs — two ffmpeg encodes at once halve both of them, and the page
/// has no way to say which progress line belongs to which. `args` is what [`transcode_args`] built: a run with
/// nothing in it means there was no file to read settings from, which is refused for the same reason.
pub fn transcode_plan(video: &str, args: &[String], running: bool) -> Result<(), String> {
    if running {
        return Err("wait for the current render to finish — ⊘ Cancel first".to_string());
    }
    if args.is_empty() {
        return Err(format!("nothing to transcode — {}", video));
    }
    Ok(())
}

/// The command for a standalone file: [`crate::produce_render`]'s codec, frame-rate and container arguments with
/// `-i <source>` where the render has `-ss` + `-i <clip>`, and **no subtitle track** — even when the row says
/// `track in file`, because a file picked from disk has no transcript beside it to mux. The rest of the shape is
/// deliberately shared with [`crate::produce_render::encode_command`] so a change to the codec table lands on
/// both paths at once.
pub fn transcode_args(source: &str, out: &str, settings: &Produce) -> Vec<String> {
    let mut argv = vec!["ffmpeg".into(), "-i".into(), source.to_string()];
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
        if settings.mono { "1".into() } else { "2".into() },
    ]);
    // `track in file` is deliberately not consulted: the render's mux adds a caption track from sidecars this
    // transcode has never written, and a file picked off disk has no transcript beside it to mux.
    if settings.container == Container::Mp4 {
        argv.extend([
            "-movflags".into(),
            "+faststart".into(),
            "-use_editlist".into(),
            "0".into(),
            "-negative_cts_offsets".into(),
            "on".into(),
        ]);
    }
    argv.push(out.to_string());
    argv
}

/// The container as the log and a filename spell it.
fn container_name(container: Container) -> &'static str {
    match container {
        Container::Mp4 => "mp4",
        Container::Mkv => "mkv",
        Container::Webm => "webm",
    }
}

/// The codec as the log spells it.
fn codec_name(codec: crate::project::Codec) -> &'static str {
    match codec {
        crate::project::Codec::H264 => "h264",
        crate::project::Codec::H265 => "h265",
        crate::project::Codec::Vp9 => "vp9",
    }
}

/// The resolution as the log spells it. A transcode never scales: `720p`/`1080p` on a file that is already
/// smaller would rescale, and §F5.7's no-rescaling rule for pictures holds for the video too.
fn resolution_name(resolution: crate::project::Resolution) -> &'static str {
    use crate::project::Resolution;
    match resolution {
        Resolution::P720 => "720p",
        Resolution::P1080 => "1080p",
        Resolution::Original => "original",
    }
}
