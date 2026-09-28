//! F5.6 Upload text and thumbnail — `spec/08-produce.md` §F5.6, the words half of ▶.
//!
//! The run's other half ([`crate::produce_exec`]) encodes video; this one asks the `youtube` prompt for a
//! title, a description and a picture, then lands them on disk. Every rule is either already owned by
//! another module (the peeler, the frame resolver, the stamp comparator, the sd.cpp body) or is plain data
//! here: **no GTK, no socket, no subprocess**. Each external leg arrives as a caller-supplied closure, the
//! convention this crate already uses ([`crate::narrate_tts::speak_line`], [`crate::produce_translate::Ask`]),
//! so a test asserts the ORDER and the sentences with no model and no image server standing.
//!
//! What decides the shape, in the spec's own words:
//! - **The text is written BEFORE the picture is drawn** (S5). A draw that fails keeps the thinking, because
//!   re-asking an LLM to recover from a GPU's bad night is the worse trade.
//! - **A picked frame means nothing is drawn** (S3): it is cut out of the source, so no model and no GPU.
//! - **The title is reprinted whenever its text changes**, until the picture is the user's own (S5, decision
//!   34) — the prototype printed it once, ever, so a re-suggested title and the picture disagreed forever.
//! - **Each tool answers what it did** (§3.9): the fitted size, the distance a frame landed, the refusal
//!   when a frame and an instruction are both claimed.

use std::cell::RefCell;

use crate::produce_details as details;
use crate::project::{Publish, TitleBox};

/// S1: the brief's opening word, so a test can pin the head without retyping the sentence.
pub const BRIEF_HEAD: &str = "THE FINISHED VIDEO";

/// S1: `P.machine.briefMaxChars` (§10, 120 000) — the most the upload brief may carry. Over it every clip
/// folds to its first lines and events and the model is told to read more with `get_lines`, which is the
/// only way a bound that does not say so differs from a truncated answer.
pub const BRIEF_MAX_CHARS: usize = 120_000;

/// How many folded lines each clip keeps when the brief is over the bound. Three is enough to say what the
/// clip is and leave the rest to `get_lines`; more and the fold stops being a fold.
/// (`machine.` — §10 gives no `P.` row; nothing outside this module asks the question.)
pub const FOLDED_LINES: usize = 3;

/// S4: the line a first run prints when the row is empty and no frame was picked. Leading spaces put it
/// under the run's own `>>>` line, the same indent every other publish line carries.
pub fn take_three_log() -> &'static str {
    "    publish: no images chosen \u{2014} taking 3 from the cut"
}

/// S6: the refusal when there is nothing to tell the image model. Verbatim, because it names the way out
/// (`▶ suggests one`) and a paraphrase loses it.
pub const NO_INSTRUCTION: &str = "nothing to tell the image model \u{2014} write an edit instruction first (\u{25b6} suggests one)";

/// S7: the order words go onto the picture — the marked texts first, the title last, so the title sits over
/// everything the user placed rather than pushing it around. `walk` iterates this list and hands each kind
/// to the print leg, so the order is what the program runs rather than a constant a test reads alone.
pub fn print_order() -> &'static [&'static str] {
    &["texts", "title"]
}

/// What the run says when the answer carried no title or no description. §3.9 makes `finish` require both,
/// while S8 makes their absence harmless — the previous value stands — so the walk names them and carries on
/// rather than failing over an empty box that is easier to notice than a wrong line.
pub fn kept_previous_log(missing: &[&str]) -> String {
    format!(
        "    publish: no {} in the answer \u{2014} keeping what was there before",
        missing.join(" or ")
    )
}

// ---- S1: the brief -----------------------------------------------------------------

/// One clip as the upload brief sees it. Everything in it is supplied by the caller: this module reads no
/// `session.tsv`, no `events.tsv` and no cut file, so the brief is a pure function of what the page holds.
#[derive(Debug, Clone)]
pub struct BriefClip {
    /// 1-based, the number the model writes back in `frame: clip <n> +<s>`.
    pub number: usize,
    /// Where the clip starts on the PRODUCED clock — the only clock this brief speaks.
    pub at_output: f64,
    /// Its length in output seconds.
    pub seconds: f64,
    /// The session span it came from, spelled (`"12:03\u{2013}12:41"`).
    pub session: String,
    /// What was seen in it, one string per event line.
    pub seen: Vec<String>,
    /// What was said in it, one string per transcript line.
    pub said: Vec<String>,
}

/// S1: the upload brief. Head, then the clips with what was seen and said, then the narration at its time
/// in the finished video, or the explicit "(no narration has been written for this video)".
///
/// Unlike the narration brief (F4.2) there are **no `MARKED` or `CAPTION` lines and no effects**: the writer
/// of a YouTube description is not placing decorations, and §3's decision 30 asks only that the rate be
/// printed and the moments named — which the clip header and the narration times do between them.
pub fn brief(clips: &[BriefClip], lines: &[crate::narration::Entry], max_chars: usize) -> String {
    let total = clips.iter().map(|c| c.seconds.max(0.0)).sum::<f64>();
    let mut out = format!(
        "{BRIEF_HEAD}: {} clips, {} long.\n\nWHAT IS IN EACH CLIP:\n",
        clips.len(),
        crate::tools::mm_ss(total)
    );
    // Fold only if the whole thing would not fit: folding costs detail, so it is paid for once, over every
    // clip, rather than clipping the tail of the brief where the last clips would vanish entirely.
    let folded = unbounded_length(clips, lines) > max_chars;
    for clip in clips {
        out.push_str(&clip_head(clip));
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
        let seen: &[String] = if folded { &seen_owned } else { &clip.seen };
        let said: &[String] = if folded { &said_owned } else { &clip.said };
        for event in seen {
            out.push_str(&format!("  SEEN: {event}\n"));
        }
        for spoken in said {
            out.push_str(&format!("  SAID: {spoken}\n"));
        }
        if folded && (clip.seen.len() > FOLDED_LINES || clip.said.len() > FOLDED_LINES) {
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
        // Output seconds, never the recording's: the brief speaks one clock (01 §4), and a line's `at` is
        // on the recording's own until `output_seconds` divides it exactly once.
        let at = crate::narration::output_seconds(line.at, 1.0);
        // Folded: only the first FOLDED_LINES lines of the narration are printed, the rest is `get_lines`.
        if !folded || index < FOLDED_LINES {
            out.push_str(&format!("  {}: {}\n", crate::tools::mm_ss(at), line.text));
        }
    }
    if folded && spoken.len() > FOLDED_LINES {
        out.push_str("(folded \u{2014} read the rest with get_lines)\n");
    }
    out
}

/// One clip's header line, exactly as S1 spells it.
fn clip_head(clip: &BriefClip) -> String {
    format!(
        "CLIP {} (at {} in the video, {}): session {}",
        clip.number,
        crate::tools::mm_ss(clip.at_output),
        crate::tools::tenths(clip.seconds),
        clip.session
    )
}

/// The brief's length if nothing were folded — the number compared against `max_chars`. Measured rather than
/// guessed so the fold decision reads the same text it is about to shorten.
fn unbounded_length(clips: &[BriefClip], lines: &[crate::narration::Entry]) -> usize {
    let mut chars = 128;
    for clip in clips {
        chars += clip_head(clip).len()
            + clip.seen.iter().map(|s| s.len() + 8).sum::<usize>()
            + clip.said.iter().map(|s| s.len() + 8).sum::<usize>();
    }
    chars += lines.iter().map(|l| l.text.len() + 16).sum::<usize>();
    chars
}

// ---- S6: the draw -----------------------------------------------------------------

/// S6: the band the title will be printed across, read off the title box's centre. A `cy` below 1/3 is the
/// **upper** part of the picture, above 2/3 the **lower**, anything between the **middle** — the thirds are
/// the frame's own, not the box's height, because the instruction tells the image model where to keep
/// clear and that place is measured from the picture's edges.
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

    /// The word the spec's sentence puts in `<upper|middle|lower>`.
    pub fn word(self) -> &'static str {
        match self {
            Band::Upper => "upper",
            Band::Middle => "middle",
            Band::Lower => "lower",
        }
    }

    /// The band the record's title box sits in.
    pub fn of_box(box_: &TitleBox) -> Self {
        Band::of(box_.cy)
    }
}

/// S6: the prompt sent to sd.cpp — the instruction plus the no-words tail, verbatim. The model writes no
/// letters because the app prints them itself (S7), so an instruction asking for a caption would be
/// painted over anyway and is asked not to try.
pub fn draw_prompt(instruction: &str, band: Band) -> String {
    format!(
        "{instruction} Do not write any words, letters, titles, logos or captions into the picture. \
Keep the {} part of the picture calm and uncluttered: a title will be printed across it afterwards.",
        band.word()
    )
}

/// S6: the frame size — the video's aspect at long side `P.machine.thumbnailLongSide` (1280), both sides
/// even. Delegated to [`crate::produce_screen::thumb_box`] so one rule sizes every thumbnail in the app.
pub fn frame_size(aspect_w: u32, aspect_h: u32) -> (u32, u32) {
    crate::produce_screen::thumb_box(
        &format!("{aspect_w}:{aspect_h}"),
        aspect_w.max(1),
        aspect_h.max(1),
    )
}

/// S6: the sd.cpp request body, built by [`crate::bodies::img_gen_body`] so the shape is that module's and
/// not a second copy: seed −1 (a fresh draw each time), references as data URLs, `auto_resize_ref_image`,
/// png out, and steps/cfg left with the server exactly as §5 says.
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

/// S6: the poll line. With nobody ahead it says only the status; with a queue it names how far back this
/// job sits, because "drawing" for two minutes reads very differently when the log already said 7 ahead.
pub fn poll_line(status: &str, ahead: usize) -> String {
    if ahead == 0 {
        format!("drawing ({status})")
    } else {
        format!("drawing ({status}, {ahead} ahead in the queue)")
    }
}

/// S6/S7: what the picture's own stamp covers — the images, the instruction, the negative, the aspect and
/// whether the picture is a chosen frame at all. Same FNV as the render stamp
/// ([`crate::produce_stamp::hash_text`]), shared rather than copied so the two can never drift apart.
pub fn thumbnail_stamp_text(
    images: &[String],
    instruction: &str,
    negative: &str,
    aspect: &str,
    own: bool,
) -> String {
    // Sorted: the same images in another order are the same reference set, and re-ordering the row must not
    // cost a redraw. The instruction is hashed as written, since a word changed there IS a different picture.
    let mut sorted = images.to_vec();
    sorted.sort();
    format!(
        "images={}\ninstruction={instruction}\nnegative={negative}\naspect={aspect}\nown={own}\n",
        sorted.join(",")
    )
}

/// The stamp's 16 hex characters over [`thumbnail_stamp_text`].
pub fn thumbnail_stamp(
    images: &[String],
    instruction: &str,
    negative: &str,
    aspect: &str,
    own: bool,
) -> String {
    crate::produce_stamp::hash_text(&thumbnail_stamp_text(images, instruction, negative, aspect, own))
}

// ---- the walk --------------------------------------------------------------------

/// What the model answered, in the shape the four tools leave behind. Built by the caller (or by
/// [`peel`] from prose) so the walk never parses a reply itself.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Answers {
    pub title: Option<String>,
    pub instruction: Option<String>,
    pub negative: Option<String>,
    pub description: Option<String>,
    /// A frame the model picked: the moment on the produced clock, already resolved.
    pub picked_frame: Option<f64>,
}

/// One sd.cpp job: its id. Kept opaque — the walk only needs to know a job was accepted; the bytes land
/// through the drawing leg on the page's side.
#[derive(Debug, Clone, PartialEq)]
pub struct DrawnJob {
    pub id: String,
}

/// One poll's answer: the status word, how many jobs sit ahead, and whether the picture is ready.
#[derive(Debug, Clone, PartialEq)]
pub struct PollState {
    pub status: String,
    pub ahead: usize,
    pub done: bool,
}

/// The model leg: the brief in, the raw reply out (`Err` = the call did not happen).
pub type AskModel = dyn Fn(&str) -> Result<String, String>;
/// The sd.cpp submit leg: the request body in, the job id out.
pub type AskDraw = dyn Fn(&serde_json::Value) -> Result<DrawnJob, String>;
/// One poll of a draw job.
pub type Poll = dyn Fn(&DrawnJob) -> Result<PollState, String>;
/// Print the marked texts, then the title, onto the picture.
/// Print one pass of words onto the picture: which kind is printing first (`"texts"` or `"title"`, the two
/// entries of [`print_order`]), then the words of that kind. The kind comes first because S7 orders the
/// passes, so the walk drives the leg from that order rather than from a fixed argument list.
pub type PrintWords = dyn Fn(&str, &[String]) -> Result<(), String>;

/// The legs the walk drives, all caller-supplied: nothing here opens a socket, spawns ffmpeg or touches a
/// widget. Mirrors `produce_exec::Materials` so the page builds both the same way.
pub struct Materials<'a> {
    /// publish.json + description.txt. Called BEFORE any draw (S5).
    pub save_publish: &'a dyn Fn(&Publish) -> Result<(), String>,
    /// The `youtube` call: the brief in, the raw reply out.
    pub ask_model: &'a AskModel,
    /// sd.cpp `POST /sdcpp/v1/img_gen`: the body in, the job id out.
    pub ask_draw: &'a AskDraw,
    /// One `GET /sdcpp/v1/jobs/{id}`, once per second while the job runs.
    pub poll: &'a Poll,
    /// Print the marked texts, then the title, onto `thumbnail.png`.
    pub print: &'a PrintWords,
    /// The nearest extracted frame to a moment on the produced clock (S3).
    pub nearest_frame: &'a dyn Fn(f64) -> Option<f64>,
    /// The picture stamp already on disk, if any.
    pub stored_stamp: Option<String>,
    /// Write the new picture stamp.
    pub write_stamp: &'a dyn Fn(&str) -> Result<(), String>,
    /// Whether the picture is the user's own — an own frame, or an edit instruction of their own. While it
    /// holds, a changed title is NOT reprinted: the picture is theirs (S5, decision 34).
    pub edited_by_hand: bool,
}

/// What the walk did. Every field is a thing a test pins about ORDER, not just outcome.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Outcome {
    /// The calls made, in order — the record the tests assert against.
    pub calls: Vec<String>,
    /// S3: the thumbnail is a chosen frame, so nothing was drawn.
    pub own: bool,
    /// S4: three frames were taken off the cut because the row was empty.
    pub took_three: bool,
    /// S6: the draw was skipped because the picture's own stamp matched.
    pub already_drawn: bool,
    /// S5: the title needed reprinting.
    pub reprinted: bool,
    /// The publish files landed. False means nothing after it ran.
    pub published: bool,
    /// Why the half failed, if it did. `finish_produce` turns it into `words_failed`'s line.
    pub error: Option<String>,
}

/// The aspect a thumbnail is drawn at when nothing in the record says otherwise: 16:9, the shape
/// `details::thumbnail_box_without_aspect` falls back to as well.
pub const DEFAULT_ASPECT: &str = "16:9";

/// S5–S7: the whole words half, in the spec's order.
///
/// brief → model → **publish.json + description.txt** → (own frame | 3 frames off the cut) → draw unless
/// `own`, or the picture's own stamp says these images and this instruction were already drawn → print the
/// marked texts, then the title → `thumbnail.png` + `thumbnail.stamp`.
///
/// The publish step comes first on purpose: a draw that fails keeps the thinking (S5), so the run reports
/// the error but leaves the text behind rather than losing an LLM call to a GPU's bad night.
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

    // S2: the model got the brief and answered with tools or with prose; the caller has already turned that
    // into `Answers`. A model failure stops here, before anything is written.
    if let Err(why) = (mats.ask_model)(brief).map(|_| ()) {
        out.error = Some(why);
        return out;
    }
    out.calls.push("model".to_string());

    // S8: a missing title or instruction keeps the previous value; the description is always replaced.
    let mut next = publish.clone();
    if let Some(title) = &answers.title {
        next.title = title.clone();
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

    // S8/§3.9: `finish` wants a title and a description. A missing one keeps the previous value (S8), so
    // this names what came over from last run instead of aborting — the requirement belongs to `finish`'
    // answer to the model, not to a run that already has a record worth writing.
    let missing = finish_needs(&next.title, &next.description);
    if !missing.is_empty() {
        log(&kept_previous_log(&missing));
    }

    // S3: a picked frame becomes the row — one frame, `own` true, nothing drawn.
    if let Some(at) = answers.picked_frame {
        match (mats.nearest_frame)(at) {
            Some(frame) => {
                next.own = true;
                next.frames = vec![format!("frame:{frame:.1}")];
                out.own = true;
                log(&details::chosen_frame_log(frame));
            }
            None => {
                // Nothing to take: say so and fall through to drawing rather than ship an empty row.
                log(&details::chosen_frame_failed_log("no extracted frame anywhere near it"));
            }
        }
    }

    // S4: nothing in the row and no frame chosen → three frames spread over the kept footage.
    if next.frames.is_empty() && !out.own {
        out.took_three = true;
        log(take_three_log());
    }

    // S5: the text lands BEFORE any drawing.
    if let Err(why) = (mats.save_publish)(&next) {
        out.error = Some(why);
        return out;
    }
    out.published = true;
    out.calls.push("publish".to_string());

    // S6: draw unless the picture is the user's own, or its own stamp says these images and this
    // instruction were already drawn.
    let aspect = DEFAULT_ASPECT.to_string();
    let current = thumbnail_stamp(&next.frames, &next.prompt, &next.negative, &aspect, next.own);
    // S5/S7: the title that goes onto the picture. From the answer when there is one; otherwise the line the
    // record already carries, since S8 keeps the previous value and a missing title must not reprint nothing.
    let printed_title = answers
        .title
        .clone()
        .unwrap_or_else(|| publish.thumb_title.clone());
    // S5: whether the picture has to carry the new words. One answer for both doors — `set_title` reports it
    // to the model and the walk acts on it — so the two can never disagree about what got printed.
    let reprint = crate::audit_gaps::reprint_title_on_change(
        &publish.thumb_title,
        &printed_title,
        mats.edited_by_hand,
    );
    if next.own {
        out.calls.push("own-frame".to_string());
    } else if details::already_drawn(true, mats.stored_stamp.as_deref() == Some(current.as_str())) {
        out.already_drawn = true;
        out.calls.push("already-drawn".to_string());
    } else {
        if next.prompt.trim().is_empty() {
            out.error = Some(NO_INSTRUCTION.to_string());
            return out;
        }
        let (wide, high) = frame_size_from_aspect(&aspect);
        let body = request_body(
            &draw_prompt(&next.prompt, Band::of_box(&title_box(&next))),
            Some(&next.negative),
            wide,
            high,
            &refs_of(&next),
        );
        log(&details::request_log(
            wide,
            high,
            &base_name(&base_of(&next)),
            body_refs(&body),
            100,
            0.5,
            0.5,
        ));
        let job = match (mats.ask_draw)(&body) {
            Ok(job) => job,
            Err(why) => {
                out.error = Some(why);
                return out;
            }
        };
        out.calls.push("draw".to_string());
        // One poll per second until the job reports done; each poll prints its own status line.
        loop {
            match (mats.poll)(&job) {
                Ok(state) => {
                    log(&poll_line(&state.status, state.ahead));
                    if state.done {
                        break;
                    }
                }
                Err(why) => {
                    out.error = Some(why);
                    return out;
                }
            }
        }
    }

    // S7: print the marked texts, then the title, onto thumbnail.png; then the picture's stamp. The passes
    // come from `print_order`, so changing that order changes what the program does.
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
                if reprint && !printed_title.trim().is_empty() {
                    vec![printed_title.clone()]
                } else {
                    Vec::new()
                }
            }
        };
        if words.is_empty() {
            // Nothing of this kind to print: no pass, no call, no claim that something landed.
            continue;
        }
        if let Err(why) = (mats.print)(kind, &words) {
            // Printing failed: keep the plain picture (`details::print_failed_keeps_plain`) and say so.
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

// ---- small readers over the record -------------------------------------------------

/// `w:h` → the draw size. An unparseable shape falls back to the aspect-free rule.
fn frame_size_from_aspect(aspect: &str) -> (u32, u32) {
    match aspect.split_once(':') {
        Some((w, h)) => {
            let w = w.trim().parse::<u32>().unwrap_or(16);
            let h = h.trim().parse::<u32>().unwrap_or(9);
            frame_size(w, h)
        }
        None => details::thumbnail_box_without_aspect(),
    }
}

/// The title band: the record's own box when it has one, the app default otherwise.
fn title_box(publish: &Publish) -> TitleBox {
    publish.title_box.unwrap_or_default()
}

/// The base image the model edits: the first frame, or empty when the row is empty.
fn base_of(publish: &Publish) -> String {
    crate::publish::base(publish).unwrap_or_default().to_string()
}

/// The reference images: everything after the base, in order.
fn refs_of(publish: &Publish) -> Vec<String> {
    crate::publish::references(publish).to_vec()
}

fn base_name(path: &str) -> String {
    path.rsplit('/').next().unwrap_or(path).to_string()
}

/// How many references actually went out in this body — read back off the JSON so the log states what was
/// sent rather than what was intended.
fn body_refs(body: &serde_json::Value) -> usize {
    body.get("ref_images")
        .and_then(|v| v.as_array())
        .map_or(0, |a| a.len())
}

// A scripted model reply, loaded by a test in place of the `youtube` call — the same seam
// [`crate::produce_translate::set_reply_for_test`] uses, so the page cannot tell the two apart.
thread_local! {
    static REPLY: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// Load a scripted reply for the next asks on this thread.
pub fn set_reply_for_test(reply: String) {
    REPLY.with(|cell| cell.replace(Some(reply)));
}

/// The scripted reply, if any.
pub fn reply_for_test() -> Option<String> {
    REPLY.with(|cell| cell.borrow().clone())
}

/// The page's real model leg: the script when one is loaded, otherwise an honest refusal. A refusal is a
/// reachable answer — `walk` stops before writing anything and `finish_produce` logs it with the spec's own
/// tail, which is what makes the branch exercised rather than stubbed out.
pub fn scripted_ask() -> Box<AskModel> {
    match reply_for_test() {
        Some(script) => Box::new(move |_brief: &str| Ok(script.clone())),
        None => Box::new(|_brief: &str| Err("no llm server here".to_string())),
    }
}

// ---- S2 / §3.9: what each tool answers -------------------------------------------

/// Words over a band, as `set_title` reports them: the size they take and whether they overflow.
#[derive(Debug, Clone, PartialEq)]
pub struct TitleAnswer {
    /// The type size that fills the band ([`crate::produce_runs::reprint`]'s own answer).
    pub size: f64,
    /// Word count, the thing "four to seven words" is about.
    pub words: usize,
    /// Over [`OVERFLOW_WORDS`]: the title keeps its words and warns (decision 35), never refused.
    pub overflows: bool,
    /// Whether the picture has to be reprinted because this text differs from the last.
    pub changed: bool,
    /// What to say back to the model — always non-empty, since §5 asks every write to carry more than `ok`.
    pub answer: String,
}

/// The word count past which a title is said to overflow its band. Seven because the shipped prompt asks for
/// four to seven and decision 35 keeps that as advice: longer prints, and says so.
pub const OVERFLOW_WORDS: usize = 7;

/// `set_title`: fit the words into the title band and answer with the size they take, or the warning.
///
/// `edited_by_hand` is the user having their own picture (an own frame, or an edit instruction of their
/// own); while that holds the picture is theirs and the title is not reprinted even when it changes (S5).
pub fn set_title(publish: &Publish, text: &str, box_: &TitleBox, edited_by_hand: bool) -> TitleAnswer {
    let printed = crate::produce_runs::reprint(text, *box_);
    let words = text.split_whitespace().count();
    let overflows = words > OVERFLOW_WORDS;
    let changed = crate::audit_gaps::reprint_title_on_change(&publish.title, text, edited_by_hand);
    let answer = if overflows {
        format!(
            "{words} words at {:.0}px \u{2014} that overflows the band (over {OVERFLOW_WORDS})",
            printed.size
        )
    } else {
        format!("{words} words at {:.0}px", printed.size)
    };
    TitleAnswer {
        size: printed.size,
        words,
        overflows,
        changed,
        answer,
    }
}

/// `set_description`: always replaced (S8) — there is no "keep the old description" branch, because a
/// blank reply means the writer had nothing to say, not that the page should keep last run's prose.
pub fn set_description(publish: &Publish, text: &str) -> String {
    let _ = publish;
    format!("description set ({} chars)", text.chars().count())
}

/// `pick_frame`: the frame ACTUALLY taken, and how far it sits from the moment asked for.
#[derive(Debug, Clone, PartialEq)]
pub struct Picked {
    /// The moment that was asked for, on the produced clock.
    pub asked: f64,
    /// The extracted frame nearest to it — possibly twenty seconds away (decision 32).
    pub taken: f64,
    /// |taken − asked|, in seconds. Printed, not hidden: the model is told where its frame really is.
    pub distance: f64,
    /// Always true: a picked frame is the thumbnail as it is, so nothing is drawn (S3).
    pub own: bool,
    pub answer: String,
}

/// `pick_frame(clip, offset)`: resolve the moment with the existing resolver, then take the nearest frame at
/// ANY distance. Frames exist only every `P.project.frameInterval` seconds, so "nearest" is the only
/// promise that can be kept — refusing over a wide gap would throw away a frame the video contains.
///
/// `Err` when the cut has no such clip or no frames were extracted: naming no frame is better than naming a
/// wrong one (`details::frame_moment` refuses the same way for the same reason).
pub fn pick_frame(clip: usize, offset: f64, starts: &[f64], frames: &[f64]) -> Result<Picked, String> {
    let named = details::FrameAt::Clip(clip, offset);
    let asked = details::frame_moment(named, starts).ok_or_else(|| format!("the cut has no clip {clip}"))?;
    let taken = nearest(frames, asked).ok_or("no frames have been extracted")?;
    let distance = (taken - asked).abs();
    Ok(Picked {
        asked,
        taken,
        distance,
        own: true,
        answer: format!(
            "frame taken at {} \u{2014} {} from the moment asked for; a picked frame means nothing is drawn",
            crate::tools::mm_ss(taken),
            crate::tools::tenths(distance)
        ),
    })
}

/// The nearest of `frames` to `at`, by absolute distance. Ties go to the earlier frame: deterministic, and
/// an earlier picture is the one that was already there.
pub fn nearest(frames: &[f64], at: f64) -> Option<f64> {
    frames.iter().min_by(|a, b| {
        (**a - at)
            .abs()
            .partial_cmp(&(**b - at).abs())
            .unwrap_or(std::cmp::Ordering::Equal)
    }).copied()
}

/// `set_thumbnail_instruction`: ok, unless a frame was already picked — then an error, instead of the
/// prototype silently clearing the frame with the instruction (decision 33).
pub fn set_thumbnail_instruction(
    own: bool,
    text: &str,
    negative: Option<&str>,
) -> Result<String, String> {
    if own {
        return Err("a frame was already picked \u{2014} clear it before setting an instruction".to_string());
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

/// `finish`'s requirement: a title AND a description. Answers the list of what is still missing, empty
/// when both are there. A missing title or instruction deliberately KEEPS the previous value (S8) — an
/// empty box is easier to notice than a wrong line — but `finish` still names it, so the rewrite fixes it
/// rather than shipping last run's answer under this run's video.
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

/// The prose fallback (§3.9): peel the labelled lines off a reply that used no tools at all. Returns
/// `(title, thumbnail_line, description)` with any part absent as `None`.
///
/// All the peeling rules stay in `details` — at most three labels, quotes stripped, a whole-reply fence
/// removed first, a short first description line ending in ":" dropped as a heading. A `THUMBNAIL:` line
/// that folds a `frame:` into itself comes back as the frame line so the caller sets a frame rather than
/// an instruction.
pub fn peel(reply: &str) -> (Option<String>, Option<String>, Option<String>) {
    let (labels, rest) = details::peel_labels(reply);
    let mut title = None;
    let mut thumbnail = None;
    for label in labels {
        // `thumbnail_line_is_frame` reads a WHOLE labelled line (`THUMBNAIL: frame: clip 2 +12`), so the
        // frame test runs on the line as it was written, not on the peeled value.
        let whole = format!("THUMBNAIL: {label}");
        let is_frame = label.starts_with("frame:") || details::thumbnail_line_is_frame(&whole);
        if is_frame {
            // A frame line always owns the thumbnail slot: folding a `frame:` into the thumbnail line means
            // the model answered with a picture from the video, and that beats any instruction (§3.9).
            thumbnail = Some(label);
        } else if title.is_none() {
            title = Some(label);
        } else if thumbnail.is_none() {
            // The second non-frame label is the instruction, when no frame claimed it.
            thumbnail = Some(label);
        }
    }
    let body: Vec<&str> = rest.lines().skip_while(|l| l.trim().is_empty()).collect();
    let description = match body.first() {
        Some(first) if details::drops_description_heading(first) && body.len() > 1 => body[1..].join("\n"),
        Some(_) => body.join("\n"),
        None => String::new(),
    };
    (title, thumbnail, (!description.trim().is_empty()).then_some(description))
}
