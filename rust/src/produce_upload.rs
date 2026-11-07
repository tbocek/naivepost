//! F5.6 Upload text and thumbnail — `spec/08-produce.md` §F5.6.
//!
//! No GTK, socket or subprocess: each external leg is a caller-supplied closure, so a test asserts the
//! order and the sentences with no model or image server. The text is written before the picture is
//! drawn (S5); a picked frame means nothing is drawn (S3).

use crate::produce_details as details;
use crate::project::{Publish, TitleBox};

pub const BRIEF_HEAD: &str = "THE FINISHED VIDEO";

/// S1: `P.machine.briefMaxChars` (§10, 120 000). Over it every clip folds to its first lines and the model
/// reads more with `get_lines`.
pub const BRIEF_MAX_CHARS: usize = 120_000;

/// Folded lines kept per clip when the brief is over the bound.
pub const FOLDED_LINES: usize = 3;

pub fn take_three_log() -> &'static str {
    "    publish: no images chosen \u{2014} taking 3 from the cut"
}

pub const NO_INSTRUCTION: &str = "nothing to tell the image model \u{2014} write an edit instruction first (\u{25b6} suggests one)";

/// S7: marked texts first, the title last so it sits over everything the user placed.
pub fn print_order() -> &'static [&'static str] {
    &["texts", "title"]
}

/// A missing title or description keeps the previous value (S8); the walk names it and carries on.
pub fn kept_previous_log(missing: &[&str]) -> String {
    format!(
        "    publish: no {} in the answer \u{2014} keeping what was there before",
        missing.join(" or ")
    )
}

/// One clip as the upload brief sees it; all of it supplied by the caller.
#[derive(Debug, Clone)]
pub struct BriefClip {
    pub number: usize,
    /// Start on the PRODUCED clock, the only clock this brief speaks.
    pub at_output: f64,
    pub seconds: f64,
    pub session: String,
    pub seen: Vec<String>,
    /// Where each `seen` line is in the clip, in session seconds from the clip's start: printed as `SEEN +12.5:`, the
    /// offset a `frame: clip n +s` answer copies. The middle of the event's stretch inside the clip, because its first
    /// frame can be the crossfade into it (09-news: "+11" was guessed with no stamp and took the fade into the title
    /// slide). Same order as `seen`; empty, or shorter, where there is no time to give.
    pub seen_at: Vec<f64>,
    pub said: Vec<String>,
    /// Rate on the produced clock; printed in the header only when not 1.0 (§12 #30).
    pub rate: f64,
    /// Decorations on the clip as `crate::narrate_pass::effect_tag` spells them, printed under `MARKED:`
    /// (§12 #30).
    pub marked: Vec<String>,
}

/// S1: the upload brief. No `CAPTION` lines (the writer places no subtitles), but decorations as `MARKED:`
/// lines and the clip's rate in its header: §12 #30 overrides F5.6 S1 on that.
pub fn brief(clips: &[BriefClip], lines: &[crate::narration::Entry], max_chars: usize) -> String {
    let total = clips.iter().map(|c| c.seconds.max(0.0)).sum::<f64>();
    let mut out = format!(
        "{BRIEF_HEAD}: {} clips, {} long.\n\nWHAT IS IN EACH CLIP:\n",
        clips.len(),
        crate::tools::mm_ss(total)
    );
    // Fold over every clip at once rather than clipping the tail, where the last clips would vanish.
    let folded = unbounded_length(clips, lines) > max_chars;
    for clip in clips {
        // Its own line: the first SEEN line ran on from the header and lost its place in the clip's list.
        out.push_str(&clip_head(clip));
        out.push('\n');
        let seen_owned: Vec<String> = if folded {
            clip.seen.iter().take(FOLDED_LINES).cloned().collect()
        } else {
            Vec::new()
        };
        let said_owned: Vec<String> = if folded {
            clip.said.iter().take(FOLDED_LINES).cloned().collect()
        } else {
            Vec::new()
        };
        let marked_owned: Vec<String> = if folded {
            clip.marked.iter().take(FOLDED_LINES).cloned().collect()
        } else {
            Vec::new()
        };
        let seen: &[String] = if folded { &seen_owned } else { &clip.seen };
        let said: &[String] = if folded { &said_owned } else { &clip.said };
        let marked: &[String] = if folded { &marked_owned } else { &clip.marked };
        for (index, event) in seen.iter().enumerate() {
            match clip.seen_at.get(index) {
                Some(at) => out.push_str(&format!("  SEEN +{at:.1}: {event}\n")),
                None => out.push_str(&format!("  SEEN: {event}\n")),
            }
        }
        for spoken in said {
            out.push_str(&format!("  SAID: {spoken}\n"));
        }
        for decoration in marked {
            out.push_str(&format!("  MARKED: {decoration}\n"));
        }
        if folded
            && (clip.seen.len() > FOLDED_LINES
                || clip.said.len() > FOLDED_LINES
                || clip.marked.len() > FOLDED_LINES)
        {
            out.push_str("  (folded \u{2014} read the rest with get_lines)\n");
        }
    }
    out.push_str("\nTHE NARRATION SPOKEN OVER IT, at its time in the finished video:\n");
    let spoken: Vec<&crate::narration::Entry> = lines
        .iter()
        .filter(|line| !line.text.trim().is_empty())
        .collect();
    if spoken.is_empty() {
        out.push_str("(no narration has been written for this video)\n");
        return out;
    }
    for (index, line) in spoken.iter().enumerate() {
        // Output seconds: the brief speaks one clock (01 §4).
        let at = crate::narration::output_seconds(line.at, 1.0);
        if !folded || index < FOLDED_LINES {
            out.push_str(&format!("  {}: {}\n", crate::tools::mm_ss(at), line.text));
        }
    }
    if folded && spoken.len() > FOLDED_LINES {
        out.push_str("(folded \u{2014} read the rest with get_lines)\n");
    }
    out
}

/// S1's escape hatch: `get_lines(from, to)` over the rows the brief was built from, in produced-clock
/// seconds, stamped like the brief's own lines. Truncation and an empty window are both said.
pub fn read_back(rows: &[crate::textfmt::SessionLine], from: f64, to: f64) -> String {
    let mut hits: Vec<&crate::textfmt::SessionLine> = rows
        .iter()
        .filter(|row| row.end > from && row.start < to)
        .collect();
    hits.sort_by(|a, b| a.start.total_cmp(&b.start));
    let truncated = hits.len() > crate::narrate_pass::GET_LINES_MAX_ROWS;
    hits.truncate(crate::narrate_pass::GET_LINES_MAX_ROWS);
    if hits.is_empty() {
        return crate::tools::ok(&serde_json::json!({
            "from": from,
            "to": to,
            "lines": [],
            "note": format!(
                "nothing was said between {} and {} of the finished video",
                crate::tools::mm_ss(from),
                crate::tools::mm_ss(to)
            ),
        }));
    }
    let lines: Vec<String> = hits
        .iter()
        .map(|row| {
            format!(
                "{} {}: {}",
                crate::tools::mm_ss(row.start),
                row.who,
                row.text
            )
        })
        .collect();
    crate::tools::ok(&serde_json::json!({
        "from": from,
        "to": to,
        "lines": lines,
        "truncated": truncated,
    }))
}

fn clip_head(clip: &BriefClip) -> String {
    let mut head = format!(
        "CLIP {} (at {} in the video, {}): session {}",
        clip.number,
        crate::tools::mm_ss(clip.at_output),
        crate::tools::tenths(clip.seconds),
        clip.session
    );
    if (clip.rate - 1.0).abs() > f64::EPSILON {
        head.push_str(&format!(
            ", the source runs at {}x here",
            crate::narrate_pass::rate_words(clip.rate)
        ));
    }
    head
}

fn unbounded_length(clips: &[BriefClip], lines: &[crate::narration::Entry]) -> usize {
    let mut chars = 128;
    for clip in clips {
        chars += clip_head(clip).len()
            + clip.seen.iter().map(|s| s.len() + 8).sum::<usize>()
            + clip.said.iter().map(|s| s.len() + 8).sum::<usize>()
            + clip.marked.iter().map(|s| s.len() + 11).sum::<usize>();
    }
    chars += lines.iter().map(|l| l.text.len() + 16).sum::<usize>();
    chars
}

/// A rate of nought is a stop, which leaves the source running at 1× under a still (as `produce_render::plan_clips`).
pub fn brief_rate(seg: &crate::cut::Seg) -> f64 {
    if seg.rate > 0.0 {
        seg.rate
    } else {
        1.0
    }
}

/// §12 #30: decorations on one clip, spelled by `crate::narrate_pass::effect_tag`; overlap as [`crate::cut::Fx::spans`].
pub fn brief_marks(fx: &[crate::cut::Fx], seg: &crate::cut::Seg) -> Vec<String> {
    fx.iter()
        .filter(|effect| {
            let (from, to) = effect.spans();
            to > seg.s && from < seg.e
        })
        .filter_map(|effect| crate::narrate_pass::effect_tag(effect))
        // A caption is the viewer's text, not a decoration on the picture.
        .filter(|(tag, _)| *tag != "CAPTION")
        .map(|(tag, words)| format!("{tag}: {words}"))
        .collect()
}

/// S6: the band the title is printed across, from the title box's centre. Thirds of the frame, not of the box.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Band {
    Upper,
    Middle,
    Lower,
}

impl Band {
    pub fn of(cy: f64) -> Self {
        if cy < 1.0 / 3.0 {
            Band::Upper
        } else if cy > 2.0 / 3.0 {
            Band::Lower
        } else {
            Band::Middle
        }
    }

    pub fn word(self) -> &'static str {
        match self {
            Band::Upper => "upper",
            Band::Middle => "middle",
            Band::Lower => "lower",
        }
    }

    pub fn of_box(box_: &TitleBox) -> Self {
        Band::of(box_.cy)
    }
}

/// S6: the instruction plus the no-words tail, verbatim (the app prints the letters itself, S7).
pub fn draw_prompt(instruction: &str, band: Band) -> String {
    format!(
        "{instruction} Do not write any words, letters, titles, logos or captions into the picture. \
Keep the {} part of the picture calm and uncluttered: a title will be printed across it afterwards.",
        band.word()
    )
}

/// S6: the video's aspect at long side `P.machine.thumbnailLongSide` (1280), both sides even.
pub fn frame_size(aspect_w: u32, aspect_h: u32) -> (u32, u32) {
    crate::produce_screen::thumb_box(
        &format!("{aspect_w}:{aspect_h}"),
        aspect_w.max(1),
        aspect_h.max(1),
    )
}

/// S6: the sd.cpp request body via [`crate::bodies::img_gen_body`]: seed −1, references as data URLs, png out.
pub fn request_body(
    instruction: &str,
    negative: Option<&str>,
    wide: u32,
    high: u32,
    refs: &[String],
) -> serde_json::Value {
    crate::bodies::img_gen_body(
        instruction,
        negative.filter(|n| !n.trim().is_empty()),
        Some(wide),
        Some(high),
        -1,
        refs,
        Some("png"),
    )
}

pub use crate::produce_upload_media::Sent;

pub fn poll_line(status: &str, ahead: usize) -> String {
    if ahead == 0 {
        format!("drawing ({status})")
    } else {
        format!("drawing ({status}, {ahead} ahead in the queue)")
    }
}

/// S6/S7: what the picture's stamp covers; hashed with [`crate::produce_stamp::hash_text`].
pub fn thumbnail_stamp_text(
    images: &[String],
    instruction: &str,
    negative: &str,
    aspect: &str,
    own: bool,
) -> String {
    // Sorted: reordering the row must not cost a redraw.
    let mut sorted = images.to_vec();
    sorted.sort();
    format!(
        "images={}\ninstruction={instruction}\nnegative={negative}\naspect={aspect}\nown={own}\n",
        sorted.join(",")
    )
}

pub fn thumbnail_stamp(
    images: &[String],
    instruction: &str,
    negative: &str,
    aspect: &str,
    own: bool,
) -> String {
    crate::produce_stamp::hash_text(&thumbnail_stamp_text(
        images,
        instruction,
        negative,
        aspect,
        own,
    ))
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Answers {
    pub title: Option<String>,
    pub instruction: Option<String>,
    pub negative: Option<String>,
    pub description: Option<String>,
    pub picked_frame: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DrawnJob {
    pub id: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PollState {
    pub status: String,
    pub ahead: usize,
    pub done: bool,
    pub png: Vec<u8>,
}

pub type AskModel = dyn Fn(&str) -> Result<String, String>;
pub type SavePublish = dyn Fn(&Publish) -> Result<(), String>;
pub type AskDraw = dyn Fn(&serde_json::Value) -> Result<DrawnJob, String>;
pub type Poll = dyn Fn(&DrawnJob) -> Result<PollState, String>;
pub type SavePicture = dyn Fn(&DrawnJob, &[u8]) -> Result<(), String>;
pub type PrintWords = dyn Fn(&str, &[String]) -> Result<(), String>;

/// The legs the walk drives; mirrors `produce_exec::Materials`.
pub struct Materials<'a> {
    pub save_publish: &'a dyn Fn(&Publish) -> Result<(), String>,
    pub ask_model: &'a AskModel,
    /// False when the CALLER already dialled this brief and hands the answers in (one LLM call per press).
    pub model_asked: bool,
    pub ask_draw: &'a AskDraw,
    /// One `GET /sdcpp/v1/jobs/{id}`, once per second.
    pub poll: &'a Poll,
    /// Without this leg the picture's stamp is neither written nor read (§F5.6 S6, S7).
    pub save_picture: &'a SavePicture,
    pub print: &'a PrintWords,
    pub nearest_frame: &'a dyn Fn(f64) -> Option<f64>,
    pub spread_frames: &'a dyn Fn() -> Vec<String>,
    pub stored_stamp: Option<String>,
    pub write_stamp: &'a dyn Fn(&str) -> Result<(), String>,
    pub edited_by_hand: bool,
    /// The row's entries as the pixels the image server takes: base cropped to the video's shape, references raw.
    pub request_images: &'a dyn Fn(&Publish) -> crate::produce_upload_media::Sent,
    /// S3: put the chosen frame's cropped pixels where the print leg reads them.
    pub own_picture: &'a dyn Fn(&Publish) -> Result<(), String>,
    pub aspect: String,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Outcome {
    pub calls: Vec<String>,
    pub own: bool,
    pub took_three: bool,
    pub already_drawn: bool,
    pub reprinted: bool,
    pub published: bool,
    pub error: Option<String>,
}

pub const DEFAULT_ASPECT: &str = "16:9";

pub fn picked_row(at: f64, nearest: impl Fn(f64) -> Option<f64>) -> Result<Vec<String>, String> {
    let frame = nearest(at).ok_or("no extracted frame anywhere near it")?;
    Ok(vec![format!("frame:{frame:.1}")])
}

/// §12 #32: the frame actually taken and its distance from what was asked; shared by `pick_frame` and the walk.
pub fn picked_frame_answer(asked: f64, taken: f64, named: &str) -> String {
    let distance = (taken - asked).abs();
    format!(
        "frame taken at {} \u{2014} {} from the moment asked for ({named}); a picked frame means nothing is drawn",
        crate::tools::mm_ss(taken),
        crate::tools::tenths(distance),
    )
}

pub const FIRST_IMAGE_AS_IS_LOG: &str =
    "    publish: no edit instruction \u{2014} the first image is the thumbnail as it is (nothing drawn)";

pub fn apply_text(publish: &Publish, answers: &Answers) -> Publish {
    let mut next = publish.clone();
    if let Some(title) = &answers.title {
        next.title = title.clone();
    }
    if !next.title_seeded && !next.title.trim().is_empty() {
        next.thumb_title = next.title.trim().to_string();
        next.title_seeded = true;
    }
    if let Some(instruction) = &answers.instruction {
        next.prompt = instruction.clone();
    }
    if let Some(negative) = &answers.negative {
        next.negative = negative.clone();
    }
    if let Some(description) = &answers.description {
        next.description = description.clone();
    }
    next
}

/// S5–S7: the whole words half. The publish step comes first so a failed draw keeps the text (S5).
pub fn walk(
    brief: &str,
    publish: &Publish,
    answers: &Answers,
    mats: &Materials,
    mut log: impl FnMut(&str),
) -> Outcome {
    let mut out = Outcome::default();
    log(brief);
    out.calls.push("brief".to_string());

    // S2: a caller that already dialled sets `model_asked` false, so one press never pays for two calls.
    if mats.model_asked {
        if let Err(why) = (mats.ask_model)(brief) {
            out.error = Some(why);
            return out;
        }
    }
    out.calls.push("model".to_string());

    let mut next = apply_text(publish, answers);

    // S8/§3.9: a missing title or description keeps the previous value; name it rather than abort.
    let missing = finish_needs(&next.title, &next.description);
    if !missing.is_empty() {
        log(&kept_previous_log(&missing));
    }

    if let Some(at) = answers.picked_frame {
        match (mats.nearest_frame)(at) {
            Some(frame) => {
                next.own = true;
                match picked_row(at, |_| Some(frame)) {
                    Ok(row) => next.frames = row,
                    Err(why) => {
                        log(&details::chosen_frame_failed_log(&why));
                        return out;
                    }
                }
                out.own = true;
                log(&details::chosen_frame_log(frame));
            }
            None => {
                log(&details::chosen_frame_failed_log(
                    "no extracted frame anywhere near it",
                ));
            }
        }
    }

    if next.frames.is_empty() && !out.own {
        let spread = (mats.spread_frames)();
        if spread.is_empty() {
            log(crate::produce_details::NO_FRAMES_LOG);
        } else {
            next.frames = spread;
        }
        out.took_three = true;
        log(take_three_log());
    }

    if !next.own && next.prompt.trim().is_empty() && !next.frames.is_empty() {
        next.own = true;
        out.own = true;
        log(FIRST_IMAGE_AS_IS_LOG);
    }

    // S5: the text lands BEFORE any drawing.
    if let Err(why) = (mats.save_publish)(&next) {
        out.error = Some(why);
        return out;
    }
    out.published = true;
    out.calls.push("publish".to_string());

    let aspect = mats.aspect.as_str();
    let current = thumbnail_stamp(&next.frames, &next.prompt, &next.negative, aspect, next.own);
    let printed_title = next.thumb_title.clone();
    // S5: one answer for both doors (`set_title` and this walk), so they cannot disagree on what got printed.
    let mut reprint = crate::audit_gaps::reprint_title_on_change(
        &publish.thumb_title,
        &printed_title,
        mats.edited_by_hand,
    );
    if next.own {
        // S3: the picked frame must land on disk; later steps read a file, not the row.
        match (mats.own_picture)(&next) {
            Ok(()) => {
                if let Some(entry) = next.frames.first() {
                    if let Some(taken) = entry
                        .strip_prefix("frame:")
                        .and_then(|t| t.parse::<f64>().ok())
                    {
                        if let Some(asked) = answers.picked_frame {
                            log(&picked_frame_answer(
                                asked,
                                taken,
                                "the moment the answer named",
                            ));
                        }
                    }
                }
                out.calls.push("own-frame".to_string());
                reprint = true;
            }
            Err(why) => {
                log(&details::chosen_frame_failed_log(&why));
                out.error = Some(why);
                return out;
            }
        }
    } else if details::already_drawn(true, mats.stored_stamp.as_deref() == Some(current.as_str())) {
        out.already_drawn = true;
        out.calls.push("already-drawn".to_string());
    } else {
        if next.prompt.trim().is_empty() {
            out.error = Some(NO_INSTRUCTION.to_string());
            return out;
        }
        let (wide, high) = frame_size_from_aspect(aspect);
        // S6: `ref_images` holds bytes read off disk, not the row's file names.
        let sent = (mats.request_images)(&next);
        let body = request_body(
            &draw_prompt(&next.prompt, Band::of_box(&title_box(&next))),
            Some(&next.negative),
            wide,
            high,
            &sent.urls,
        );
        if sent.base.is_none() {
            log(details::REQUEST_NO_IMAGES_LOG);
        } else {
            log(&details::request_log(
                wide,
                high,
                &sent.base_name,
                body_refs(&body),
                sent.crop_percent,
                sent.centre.0,
                sent.centre.1,
            ));
        }
        let job = match (mats.ask_draw)(&body) {
            Ok(job) => job,
            Err(why) => {
                out.error = Some(why);
                return out;
            }
        };
        out.calls.push("draw".to_string());
        // The "done" reply carries the picture (`result.images[0].b64_json`), so the hand-off happens inside this arm.
        let mut png: Vec<u8> = Vec::new();
        let ended: Result<(), String> = loop {
            match (mats.poll)(&job) {
                Ok(state) => {
                    log(&poll_line(&state.status, state.ahead));
                    if state.done {
                        png = state.png;
                        break Ok(());
                    }
                }
                Err(why) => break Err(why),
            }
        };
        if let Err(why) = ended {
            out.error = Some(why);
            return out;
        }
        if png.is_empty() {
            out.error = Some(format!(
                "the drawing server finished job {} with no image",
                job.id
            ));
            return out;
        }
        if let Err(why) = (mats.save_picture)(&job, &png) {
            out.error = Some(why);
            return out;
        }
        out.calls.push("picture".to_string());
        reprint = true;
    }

    // S7: passes come from `print_order`.
    let marked: Vec<String> = next
        .texts
        .iter()
        .map(|t| t.text.clone())
        .filter(|t| !t.trim().is_empty())
        .collect();
    for kind in print_order() {
        let words: Vec<String> = match *kind {
            "texts" => marked.clone(),
            _ => {
                // The texts pass prints from the plain picture, so with marks the title goes back on too.
                if (reprint || !marked.is_empty()) && !printed_title.trim().is_empty() {
                    vec![printed_title.clone()]
                } else {
                    Vec::new()
                }
            }
        };
        if words.is_empty() {
            continue;
        }
        if let Err(why) = (mats.print)(kind, &words) {
            log(&format!(
                "    publish: could not print the words ({why}) \u{2014} keeping the plain picture"
            ));
            out.error = Some(why);
            return out;
        }
        if *kind == "title" {
            out.reprinted = true;
        }
        out.calls.push(format!("print-{kind}"));
    }
    match (mats.write_stamp)(&current) {
        Ok(()) => out.calls.push("stamp".to_string()),
        Err(why) => log(&details::stamp_unwritable_log(&why)),
    }
    out
}

pub fn frame_size_from_aspect(aspect: &str) -> (u32, u32) {
    match aspect.split_once(':') {
        Some((w, h)) => {
            let w = w.trim().parse::<u32>().unwrap_or(16);
            let h = h.trim().parse::<u32>().unwrap_or(9);
            frame_size(w, h)
        }
        None => details::thumbnail_box_without_aspect(),
    }
}

fn title_box(publish: &Publish) -> TitleBox {
    publish.title_box.unwrap_or_default()
}

pub fn base_of(publish: &Publish) -> String {
    crate::publish::base(publish)
        .unwrap_or_default()
        .to_string()
}

fn body_refs(body: &serde_json::Value) -> usize {
    body.get("ref_images")
        .and_then(|v| v.as_array())
        .map_or(0, |a| a.len())
}

#[derive(Debug, Clone, PartialEq)]
pub struct TitleAnswer {
    pub size: f64,
    /// Whether the band forced a smaller size (§12 #35).
    pub shrunk: bool,
    pub words: usize,
    pub overflows: bool,
    pub changed: bool,
    pub answer: String,
}

/// Words past which a title is said to overflow; advice only (decision 35).
pub const OVERFLOW_WORDS: usize = 7;

/// `set_title`: fit the words into the title band and answer with the size, or the warning.
/// `edited_by_hand`: the picture is the user's own, so a changed title is not reprinted (S5).
pub fn set_title(
    publish: &Publish,
    text: &str,
    box_: &TitleBox,
    edited_by_hand: bool,
) -> TitleAnswer {
    let printed = crate::produce_runs::reprint(text, *box_);
    let words = text.split_whitespace().count();
    // §12 #35: the word count (advice) and the band (pixels) fail separately, so the answer tells them apart.
    // A title is never refused (decision 35).
    let overflows = words > OVERFLOW_WORDS;
    // "Fitted down": the band made the letters smaller than one line as tall as the band, as `fx_text::fit`'s first try.
    let band_px = box_.hf * f64::from(crate::produce_runs::HEIGHT);
    let full_height =
        band_px > 0.0 && printed.size >= band_px / crate::fx_text::LINE_HEIGHT_EM - f64::EPSILON;
    let shrunk = printed.size > 0.0 && !full_height;
    let changed = crate::audit_gaps::reprint_title_on_change(&publish.title, text, edited_by_hand);
    let answer = match (overflows, shrunk) {
        (true, _) => format!(
            "{words} words at {:.0}px \u{2014} that overflows the band (over {OVERFLOW_WORDS})",
            printed.size
        ),
        (false, true) => format!(
            "{words} words at {:.0}px \u{2014} fitted down to stand inside the band",
            printed.size
        ),
        (false, false) => format!("{words} words at {:.0}px", printed.size),
    };
    TitleAnswer {
        size: printed.size,
        shrunk,
        words,
        overflows,
        changed,
        answer,
    }
}

/// `set_description`: always replaced (S8); a blank reply does not keep last run's prose.
pub fn set_description(text: &str) -> String {
    format!("description set ({} chars)", text.chars().count())
}

#[derive(Debug, Clone, PartialEq)]
pub struct Picked {
    pub asked: f64,
    /// Nearest extracted frame, possibly far away (decision 32).
    pub taken: f64,
    pub distance: f64,
    pub own: bool,
    pub answer: String,
}

/// `pick_frame(clip, offset)`: resolve the moment, then take the nearest frame at any distance.
/// `Err` when the cut has no such clip or no frames were extracted.
pub fn pick_frame(
    clip: usize,
    offset: f64,
    starts: &[f64],
    frames: &[f64],
) -> Result<Picked, String> {
    let named = details::FrameAt::Clip(clip, offset);
    let asked = details::frame_moment(named, starts)
        .ok_or_else(|| format!("the cut has no clip {clip}"))?;
    let taken = nearest(frames, asked).ok_or("no frames have been extracted")?;
    let distance = (taken - asked).abs();
    Ok(Picked {
        asked,
        taken,
        distance,
        own: true,
        answer: crate::produce_upload::picked_frame_answer(
            asked,
            taken,
            &format!("clip {clip} +{}", crate::tools::tenths(offset)),
        ),
    })
}

/// The nearest of `frames` to `at`; ties go to the earlier frame.
pub fn nearest(frames: &[f64], at: f64) -> Option<f64> {
    frames
        .iter()
        .min_by(|a, b| {
            (**a - at)
                .abs()
                .partial_cmp(&(**b - at).abs())
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .copied()
}

/// `set_thumbnail_instruction`: an error if a frame was already picked (decision 33).
pub fn set_thumbnail_instruction(
    own: bool,
    text: &str,
    negative: Option<&str>,
) -> Result<String, String> {
    if own {
        return Err(
            "a frame was already picked \u{2014} clear it before setting an instruction"
                .to_string(),
        );
    }
    if text.trim().is_empty() {
        return Err(NO_INSTRUCTION.to_string());
    }
    Ok(match negative {
        Some(neg) if !neg.trim().is_empty() => {
            format!("instruction set with a negative ({})", neg.trim())
        }
        _ => "instruction set".to_string(),
    })
}

/// `finish`'s requirement: what is still missing of title and description. Missing values keep the previous one (S8).
pub fn finish_needs(title: &str, description: &str) -> Vec<&'static str> {
    let mut need = Vec::new();
    if title.trim().is_empty() {
        need.push("title");
    }
    if description.trim().is_empty() {
        need.push("description");
    }
    need
}

/// A title with no `TITLE:` label: one short line directly above `THUMBNAIL:`. A line ending in a colon is the
/// model talking about what comes next.
fn bare_title(reply: &str) -> Option<String> {
    let text = details::strip_whole_fence(reply);
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    let [first, second, ..] = lines.as_slice() else {
        return None;
    };
    let is_label = |line: &str, label: &str| {
        line.get(..label.len())
            .is_some_and(|start| start.eq_ignore_ascii_case(label))
    };
    (is_label(second, "THUMBNAIL:")
        && !lines.iter().any(|line| is_label(line, "TITLE:"))
        && !first.ends_with(':')
        && first.chars().count() <= 120)
        .then(|| first.to_string())
}

/// The prose fallback (§3.9): peel labelled lines off a reply with no tools, as `(title, thumbnail_line,
/// description)`. The peeling rules live in `details`.
pub fn peel(reply: &str) -> (Option<String>, Option<String>, Option<String>) {
    let (labels, rest) = details::peel_labels(reply);
    let bare = bare_title(reply);
    let mut title = None;
    let mut thumbnail = None;
    let labels = if bare.is_some() {
        thumbnail = labels.first().cloned();
        title = bare;
        Vec::new()
    } else {
        labels
    };
    for label in labels {
        // `thumbnail_line_is_frame` reads a WHOLE labelled line, so test the line as written.
        let whole = format!("THUMBNAIL: {label}");
        let is_frame = label.starts_with("frame:") || details::thumbnail_line_is_frame(&whole);
        if is_frame {
            // A frame line owns the thumbnail slot over any instruction (§3.9).
            thumbnail = Some(label);
        } else if title.is_none() {
            title = Some(label);
        } else if thumbnail.is_none() {
            thumbnail = Some(label);
        }
    }
    let body: Vec<&str> = rest.lines().skip_while(|l| l.trim().is_empty()).collect();
    let description = match body.first() {
        Some(first) if details::drops_description_heading(first) && body.len() > 1 => {
            body[1..].join("\n")
        }
        Some(_) => body.join("\n"),
        None => String::new(),
    };
    (
        title,
        thumbnail,
        (!description.trim().is_empty()).then_some(description),
    )
}
