//! F5.4 §S3/§S7/§S9 — the Produce page's Translate row: its ticks, what a tick does to the state,
//! and the render's spawner seam. Split out of `produce_page` because that file is at its size budget;
//! `wire_settings` calls [`wire_ticks`] and `finish_produce` passes [`spawn`] to the render.
//!
//! F5.7 lives here too: [`legs`] builds the six presses' injected legs from the page's held state and the
//! scripted seams this module already owns, so a redraw and the run's own words half draw with the SAME legs
//! and cannot disagree about what was sent.

use adw::prelude::*;
use gtk4 as gtk;
use std::rc::Rc;

use crate::produce_screen as screen;
use crate::project::TitleBox;
use crate::ui::produce_page::ProduceState;

/// What a tick hands back to the page: `guarded` reads the repaint guard, `changed` records the tick.
type Guard = Rc<dyn Fn() -> bool>;
type Changed = Rc<dyn Fn(&str, bool)>;

/// Tick every `translate-tick-<language>` the page drew. `guarded` answers the page's repaint guard (a
/// rebuild must not read as an edit) and `changed` records the tick — both stay the page's own, so this
/// module touches none of its thread-locals. A tick has no value string and the row holds several
/// languages at once, which is why the toggle's own state is the answer and `set_setting` is not used.
pub fn wire_ticks(window: &adw::ApplicationWindow, guarded: Guard, changed: Changed) {
    for language in screen::TRANSLATE_LANGUAGES {
        let name = format!("translate-tick-{language}");
        // The ticks are `CheckButton`s; `line_step_button` hands back a plain `Button`, so look the
        // widget up by name and downcast.
        let Some(tick) = crate::ui::produce_page::widget_in(window, &name)
            .and_then(|w| w.downcast::<gtk::CheckButton>().ok())
        else {
            continue;
        };
        let language = language.to_string();
        let guarded = Rc::clone(&guarded);
        let changed = Rc::clone(&changed);
        tick.connect_toggled(move |tick| {
            if guarded() {
                return;
            }
            changed(&language, tick.is_active());
        });
    }
}

/// One Translate tick applied to the page's state: the language joins or leaves `translate`, order kept
/// so the sidecars arrive in the order the row listed them (§F5.4 S9: one file per language, in order).
pub fn apply(state: &mut ProduceState, language: &str, on: bool) {
    state.translate.retain(|t| t != language);
    if on {
        state.translate.push(language.to_string());
    }
}

/// The status line a tick leaves behind.
pub fn status_line(language: &str, on: bool) -> String {
    format!("translate {language} {}", if on { "on" } else { "off" })
}

/// The render's spawner: the scripted one when a test has loaded it (this container has no ffmpeg —
/// `command -v ffmpeg` is empty), otherwise the real [`crate::produce_exec::spawn_tool`]. Mirrors
/// `ui::set_narrate_script`: a test drives the whole press without standing an encoder up.
pub fn spawn(command: &crate::produce_exec::Command) -> Result<(), String> {
    match crate::produce_translate::spawn_for_test() {
        Some(sink) => sink(command),
        None => crate::produce_exec::spawn_tool(command),
    }
}

/// F5.6: the run's WORDS half, driven from the page. Split out of `produce_page` because that file is at
/// its size budget; `finish_produce` calls this where its words closure used to report a hardcoded failure.
///
/// Every rule lives in [`crate::produce_upload`]; this gathers what the session folder holds and hands the
/// walk its legs. Nothing here decides an order, prints a spec sentence or spawns anything itself.
/// A source's prepare folder is filed under its base name without the extension; the inverse of
/// `produce_languages::source_stem`'s rule, kept here so a page leg can list frames from a source path.
fn stem_of(path: &str) -> String {
    let base = path.rsplit('/').next().unwrap_or(path);
    base.rsplit_once('.')
        .map_or(base, |(stem, _)| stem)
        .to_string()
}

/// The legs the words half drives, built from the run's own snapshot: `tree` for the folders, `run` for the
/// cut the frames are mapped through. Lives in `produce_languages` because every piece it reads is already
/// this module's (`extracted_frames`, `starts_of`, the seams below). The legs are OWNED here and moved into
/// an [`Owned`] so the walk can be handed one borrowing them, rather than this function returning references
/// to values that die with it.
pub fn upload_legs(tree: &crate::layout::Tree, run: &crate::produce_flow::Run) -> Owned {
    // S6's size question has one answer in the app: the cut's aspect at long side 1280.
    let (wide, high) = crate::produce_upload::frame_size_from_aspect(&run.aspect);
    let publish = publish_of_run(run);
    // Each media leg carries its own copy of the folder and the frame list: two closures cannot share one
    // moved value, and both must read the same snapshot taken at the press.
    let tree_draw = tree.clone();
    let tree_own = tree.clone();
    let frames = std::rc::Rc::new(frame_images(tree, run));
    let frames_own = std::rc::Rc::clone(&frames);
    // The same rule the walk reads off `Materials.edited_by_hand`: the picture is the user's own once they
    // hold a frame or have written an edit instruction of their own.
    let edited_by_hand = publish.own || !publish.prompt.trim().is_empty();
    // §1's text rewriter and F5.6 S2's words half dial the SAME leg: the real `youtube` call over HTTP.
    // Nothing here answers for the model — a test replaces the SERVER (the settings' address pointed at a
    // loopback fake), never this leg, so every green test goes through the request, its tools and the prose
    // fallback exactly as a live server drives them.
    let ask_model = upload_ask(
        tree,
        &publish,
        extracted_frames(tree, run),
        starts_of(run),
        edited_by_hand,
    );
    Owned {
        save_publish: Box::new({
            let tree = tree.clone();
            move |next: &crate::project::Publish| save_publish(&tree, next)
        }),
        ask_model,
        ask_draw: draw_leg(),
        poll: poll_leg(),
        save_picture: Box::new(save_drawn),
        print: print_leg(title_band_of(&publish)),
        nearest_frame: Box::new({
            let tree = tree.clone();
            let run = run.clone();
            move |at: f64| nearest_frame(&tree, &run, at)
        }),
        spread_frames: Box::new({
            let segs = run.cut.segs.clone();
            move || crate::produce_upload_frames::spread_rows(&segs)
        }),
        stored_stamp: read_picture_stamp(tree),
        write_stamp: Box::new({
            let tree = tree.clone();
            move |hash: &str| write_picture_stamp(&tree, hash)
        }),
        // Until the picture is the user's own, a title change reprints it (S5).
        edited_by_hand,
        // S3/S6: the row's entries rendered as the pictures they name. The crop size travels with the leg,
        // because the size is decided once per press (S6: the video's aspect at long side 1280) and must not
        // be recomputed differently by the two doors that ask.
        request_images: Box::new(move |publish: &crate::project::Publish| {
            crate::produce_upload_media::request_images(
                &tree_draw,
                publish,
                &frames,
                wide,
                high,
                crate::ui::window::log_line,
            )
        }),
        own_picture: Box::new(move |publish: &crate::project::Publish| {
            crate::produce_upload_media::write_own_picture(
                &tree_own,
                publish,
                &frames_own,
                wide,
                high,
                crate::ui::window::log_line,
            )
        }),
    }
}

/// A [`crate::produce_upload::Materials`] borrowing the legs this call built. The one door the words half
/// uses: `upload_half` builds the legs, hands the walk this view of them, and drops them when the walk ends.
#[allow(clippy::needless_pass_by_value)] // `run` is read for the cut its legs already copied
pub fn upload_mats<'a>(
    legs: &'a Owned,
    _run: &crate::produce_flow::Run,
) -> crate::produce_upload::Materials<'a> {
    crate::produce_upload::Materials {
        save_publish: &legs.save_publish,
        ask_model: &legs.ask_model,
        // `upload_half` has already dialled this brief through `legs.ask_model` and parsed the answers, so the
        // walk must not dial it a second time for the same press.
        model_asked: false,
        ask_draw: &legs.ask_draw,
        poll: &legs.poll,
        save_picture: &legs.save_picture,
        print: &legs.print,
        nearest_frame: &legs.nearest_frame,
        spread_frames: &legs.spread_frames,
        stored_stamp: legs.stored_stamp.clone(),
        write_stamp: &legs.write_stamp,
        edited_by_hand: legs.edited_by_hand,
        request_images: &legs.request_images,
        own_picture: &legs.own_picture,
    }
}

/// The legs a words half owns for the length of one walk. Every field is one of [`crate::produce_upload`]'s
/// leg types, held by value so a `Materials` over `&Owned` has something to point at.
pub struct Owned {
    pub save_publish: Box<PublishSaver>,
    pub ask_model: Box<crate::produce_upload::AskModel>,
    pub ask_draw: Box<crate::produce_upload::AskDraw>,
    pub poll: Box<crate::produce_upload::Poll>,
    pub save_picture: Box<crate::produce_upload::SavePicture>,
    pub print: Box<crate::produce_upload::PrintWords>,
    pub nearest_frame: Box<Nearest>,
    pub spread_frames: Box<Spread>,
    pub stored_stamp: Option<String>,
    pub write_stamp: Box<StampWriter>,
    pub edited_by_hand: bool,
    /// S6: the row as the images the request carries. The frames are the session's, not the record's, so the
    /// leg closes over the list `upload_legs` built at the press rather than taking it as an argument.
    pub request_images: Box<Sender>,
    /// S3: the chosen frame's cropped bytes onto the plain picture.
    pub own_picture: Box<Taker>,
}

// The leg shapes `Owned` holds. Named once each so the struct reads as a list of doors, not as six lines of
// `Box<dyn Fn …>` (clippy's `type_complexity` rightly objects to the inline spelling).
type PublishSaver = dyn Fn(&crate::project::Publish) -> Result<(), String>;
type Nearest = dyn Fn(f64) -> Option<f64>;
type Spread = dyn Fn() -> Vec<String>;
type StampWriter = dyn Fn(&str) -> Result<(), String>;
type Sender = dyn Fn(&crate::project::Publish) -> crate::produce_upload_media::Sent;
type Taker = dyn Fn(&crate::project::Publish) -> Result<(), String>;

/// The record the words half works from: the run's publish state. A run built without one has nothing
/// written, which is the same state as an empty record (§5's reading of a missing `publish.json`).
pub fn publish_of_run(run: &crate::produce_flow::Run) -> crate::project::Publish {
    run.publish.clone().unwrap_or_default()
}

/// The S1 brief for one run: the two sides of every clip, read once over the whole cut.
/// `seen` comes from Describe's events.tsv for the recording the clip reads, `said` from the session
/// transcript; both are read once over the whole cut rather than per clip, so a long session costs three
/// file reads and not one per clip.
fn upload_brief_for(tree: &crate::layout::Tree, run: &crate::produce_flow::Run) -> String {
    let rows = crate::textfmt::read_session(&tree.session_tsv()).unwrap_or_default();
    let events = read_all_events(tree, run);
    let clips = brief_clips(run, &rows, &events);
    crate::produce_upload::brief(&clips, &run.lines, crate::produce_upload::BRIEF_MAX_CHARS)
}

/// Every extracted JPEG this session has, as `(produced-clock moment, file)` pairs. [`extracted_frames`
/// already maps the moments; this pairs each with the file behind it, which is what
/// [`crate::produce_upload_media::entry_file`] resolves a `frame:` entry against.
pub fn frame_images(
    tree: &crate::layout::Tree,
    run: &crate::produce_flow::Run,
) -> Vec<(f64, std::path::PathBuf)> {
    let mut out: Vec<(f64, std::path::PathBuf)> = Vec::new();
    let mut at = 0.0;
    let zero = crate::frames_run::session_zero(tree);
    for seg in &run.cut.segs {
        let kept = output_length(seg);
        if !seg.ins.is_empty() {
            at += kept;
            continue;
        }
        for source in run.sources.footage.iter() {
            let dir = tree.frames_dir(&stem_of(source));
            for (seconds, path) in crate::produce_upload_media::listed_frames(&dir, zero) {
                if seconds >= seg.s && seconds < seg.e {
                    out.push((at + (seconds - seg.s), path));
                }
            }
        }
        at += kept;
    }
    out.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    out
}

/// F5.6: the run's WORDS half, driven from the page. Split out of `produce_page` because that file is at
/// its size budget; `finish_produce` calls this where its words closure used to report a hardcoded failure.
///
/// Every rule lives in [`crate::produce_upload`]; this gathers what the session folder holds and hands the
/// walk its legs. Nothing here decides an order, prints a spec sentence or spawns anything itself.
pub fn upload_half(run: &crate::produce_flow::Run) -> Result<(), String> {
    let tree = session_tree();
    let brief = upload_brief_for(&tree, run);
    // The real `youtube` call goes out (§3.9): thinking on, its tools offered, each answered by the rule that
    // owns it. A test replaces the SERVER (the settings' address), never the leg. A refusal names the model
    // and stops before anything is written, which `finish_produce` logs as `words_failed`.
    // The publish state is the RUN's, not the file's: §S3 snapshots it at the press, and the page is what
    // holds it. Reading `publish.json` here would describe what an earlier run left — on a fresh session,
    // an empty image row, no edit instruction and no title — instead of what the person has on screen now,
    // and would then write that emptiness back over their typing.
    let publish = publish_of_run(run);
    // ONE dial per press (F5.6 S1's "one LLM call"): the legs carry the real leg, so the half asks THAT and
    // hands `walk` the answers it produced rather than making the walk dial a second time for the same brief.
    let legs = upload_legs(&tree, run);
    let reply = (legs.ask_model)(&brief)?;
    let answers = answers_from(&reply, starts_of(run), |at| nearest_frame(&tree, run, at));
    let mats = upload_mats(&legs, run);
    let outcome = crate::produce_upload::walk(
        &brief,
        &publish,
        &answers,
        &mats,
        crate::ui::window::log_line,
    );
    outcome.error.map_or(Ok(()), Err)
}

/// The clip starts on the produced clock, in the cut's order — the numbers `frame: clip n +s` resolves
/// against (§3.9), built here because only the page holds the live cut.
fn starts_of(run: &crate::produce_flow::Run) -> Vec<f64> {
    let mut starts = Vec::with_capacity(run.cut.segs.len());
    let mut at = 0.0;
    for seg in &run.cut.segs {
        starts.push(at);
        at += output_length(seg);
    }
    starts
}

/// A segment's length in OUTPUT seconds: rate applied once (`narration::output_seconds`' own rule), with
/// the splice's explicit `dur` winning over the footage span when it has one.
fn output_length(seg: &crate::cut::Seg) -> f64 {
    let raw = if seg.dur > 0.0 {
        seg.dur
    } else {
        (seg.e - seg.s).max(0.0)
    };
    crate::narration::output_seconds(raw, seg.rate)
}

/// The S1 clip list: every kept segment with what was seen and said inside it, on the session clock.
fn brief_clips(
    run: &crate::produce_flow::Run,
    rows: &[crate::textfmt::SessionLine],
    events: &[crate::textfmt::FrameEvent],
) -> Vec<crate::produce_upload::BriefClip> {
    let mut out = Vec::with_capacity(run.cut.segs.len());
    let mut at = 0.0;
    for (index, seg) in run.cut.segs.iter().enumerate() {
        let seen: Vec<String> = events
            .iter()
            .filter(|e| overlaps(e.start, e.end, seg.s, seg.e))
            .map(|e| e.text.clone())
            .collect();
        let said: Vec<String> = rows
            .iter()
            .filter(|r| overlaps(r.start, r.end, seg.s, seg.e))
            .map(|r| format!("{}: {}", r.who, r.text))
            .collect();
        out.push(crate::produce_upload::BriefClip {
            number: index + 1,
            at_output: at,
            seconds: output_length(seg),
            session: format!(
                "{}\u{2013}{}",
                crate::tools::mm_ss(seg.s),
                crate::tools::mm_ss(seg.e)
            ),
            seen: if seen.is_empty() {
                vec!["nothing was seen".to_string()]
            } else {
                seen
            },
            said,
            rate: crate::produce_upload::brief_rate(seg),
            marked: crate::produce_upload::brief_marks(&run.cut.fx, seg),
        });
        at += output_length(seg);
    }
    out
}

fn overlaps(a_s: f64, a_e: f64, b_s: f64, b_e: f64) -> bool {
    a_s < b_e && b_s < a_e
}

/// Every event of every footage recording, concatenated. Read once per run: `events.tsv` is per source,
/// and the brief speaks of the video rather than of files, so the source boundary is dropped here.
fn read_all_events(
    tree: &crate::layout::Tree,
    run: &crate::produce_flow::Run,
) -> Vec<crate::textfmt::FrameEvent> {
    let mut all = Vec::new();
    for source in run.sources.footage.iter() {
        let stem = source_stem(source);
        if let Ok(events) = crate::textfmt::read_events(&tree.events_tsv(&stem)) {
            all.extend(events);
        }
    }
    all
}

/// The name a source's prepare folder is filed under: its base name without the extension. `Tree::input_dir`
/// and friends key every per-source artefact on this, not on the full path.
fn source_stem(path: &str) -> String {
    let base = path.rsplit('/').next().unwrap_or(path);
    base.rsplit_once('.')
        .map_or(base, |(stem, _)| stem)
        .to_string()
}

/// Turn the model's reply into the tool answers: tools first if the reply carried them, prose otherwise.
/// A `frame:` line resolves to the nearest extracted frame, so `picked_frame` is a moment that exists.
fn answers_from(
    reply: &str,
    starts: Vec<f64>,
    nearest: impl Fn(f64) -> Option<f64>,
) -> crate::produce_upload::Answers {
    if let Some(json) = reply.strip_prefix('{') {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&format!("{{{json}")) {
            return crate::produce_upload::Answers {
                title: string_field(&value, "title"),
                instruction: string_field(&value, "instruction"),
                negative: string_field(&value, "negative"),
                description: string_field(&value, "description"),
                picked_frame: value
                    .get("frame")
                    .and_then(|f| f.as_str())
                    .and_then(|line| crate::produce_details::frame_line(&format!("frame: {line}")))
                    .and_then(|named| crate::produce_details::frame_moment(named, &starts))
                    .and_then(&nearest),
            };
        }
    }
    let (title, thumbnail, description) = crate::produce_upload::peel(reply);
    let picked = thumbnail
        .as_deref()
        .and_then(|line| crate::produce_details::frame_line(frame_body(line)))
        .and_then(|named| crate::produce_details::frame_moment(named, &starts))
        .and_then(&nearest);
    let instruction = if picked.is_some() { None } else { thumbnail };
    crate::produce_upload::Answers {
        title,
        instruction,
        negative: None,
        description,
        picked_frame: picked,
    }
}

/// A `THUMBNAIL: frame: clip 2 +12` line arrives with the label still on it; `frame_line` wants the value.
fn frame_body(line: &str) -> &str {
    line.strip_prefix("THUMBNAIL:")
        .map(str::trim)
        .unwrap_or(line)
}

fn string_field(value: &serde_json::Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .filter(|s| !s.trim().is_empty())
}

/// The frames actually on disk for this session, as produced-clock moments. Frames live per recording under
/// `prepare/inputs/frames/<stem>/` named by their SECONDS IN THE RECORDING, so they are mapped onto the
/// produced clock through the cut: a frame survives into the video only if its recording second falls in a
/// kept footage segment, and lands at that segment's start plus how far into the kept stretch it sits.
/// An empty list is a legal answer — S4 says take three from the cut and S3 reports there is nothing near.
fn extracted_frames(tree: &crate::layout::Tree, run: &crate::produce_flow::Run) -> Vec<f64> {
    let mut out = Vec::new();
    let mut at = 0.0;
    let zero = crate::frames_run::session_zero(tree);
    for seg in &run.cut.segs {
        let kept = output_length(seg);
        if !seg.ins.is_empty() {
            at += kept;
            continue;
        }
        for source in run.sources.footage.iter() {
            let dir = tree.frames_dir(&source_stem(source));
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                // The one naming rule (`clock::frame_session_seconds`): a frame is named by its real instant, read
                // back as a session second against the session's zero. The copy that stood here read the time of
                // day alone and dropped the date, which only matched a session that started at midnight.
                let Some(seconds) = name
                    .strip_suffix(".jpg")
                    .and_then(|stem| crate::clock::frame_session_seconds(stem, zero))
                else {
                    continue;
                };
                if seconds >= seg.s && seconds < seg.e {
                    out.push(at + (seconds - seg.s));
                }
            }
        }
        at += kept;
    }
    out.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    out
}

/// The nearest extracted frame to a produced-clock moment (S3 / decision 32), through the ONE nearest rule
/// [`crate::produce_upload_frames::frame_at`] so a spread frame and a picked frame resolve the same way.
fn nearest_frame(
    tree: &crate::layout::Tree,
    run: &crate::produce_flow::Run,
    at: f64,
) -> Option<f64> {
    crate::produce_upload_frames::frame_at(&extracted_frames(tree, run), at)
}

/// publish.json + description.txt, both through the existing writer so one spelling of each path backs
/// them. Called BEFORE any draw (S5), which is why a failed draw keeps the thinking.
pub fn save_publish(
    tree: &crate::layout::Tree,
    publish: &crate::project::Publish,
) -> Result<(), String> {
    crate::publish::save(publish, tree)
}

fn read_picture_stamp(tree: &crate::layout::Tree) -> Option<String> {
    std::fs::read_to_string(tree.thumbnail_stamp())
        .ok()
        .map(|text| text.trim().to_string())
        .filter(|hash| !hash.is_empty())
}

fn write_picture_stamp(tree: &crate::layout::Tree, hash: &str) -> Result<(), String> {
    let path = tree.thumbnail_stamp();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|why| format!("{}: {why}", dir.display()))?;
    }
    std::fs::write(&path, format!("{hash}\n")).map_err(|why| format!("{}: {why}", path.display()))
}

thread_local! {
    /// A scripted sd.cpp answer, loaded by a test in place of the drawing server. Same shape as
    /// `produce_translate::SPAWN`: the page cannot tell the two apart, and no test dials a socket.
    static DRAW: std::cell::RefCell<Option<(DrawSlot, Vec<u8>)>> = const { std::cell::RefCell::new(None) };
}

/// Load a scripted draw for the next words half on this thread, with no picture: the walk then reports the
/// job finished with no image, exactly as it does for a live server that answers `done` without one.
pub fn set_draw_for_test(draw: DrawSlot) {
    set_draw_picture_for_test(draw, Vec::new());
}

/// Load a scripted draw AND the PNG its job hands back. Only the submit answer is scripted; the bytes ride
/// into the poll answer, so `save_drawn`, the print leg and the stamp all run for real over them.
pub fn set_draw_picture_for_test(draw: DrawSlot, png: Vec<u8>) {
    DRAW.with(|cell| cell.replace(Some((draw, png))));
}

/// Unload a scripted draw: the real submit and poll legs stand again on this thread (a test that dials a fake
/// sd.cpp over HTTP after another check scripted the draw).
pub fn clear_draw_for_test() {
    DRAW.with(|cell| cell.replace(None));
}

/// The scripted draw's picture, or `None` when no script is loaded and the real legs stand.
fn scripted_png() -> Option<Vec<u8>> {
    DRAW.with(|cell| cell.borrow().as_ref().map(|(_, png)| png.clone()))
}

/// The scripted sd.cpp submit leg. `Arc` and `Send + Sync` for the same reason as
/// [`crate::produce_translate::SpawnSlot`]: the words half runs on the run's worker.
pub type DrawSlot = std::sync::Arc<
    dyn Fn(&serde_json::Value) -> Result<crate::produce_upload::DrawnJob, String> + Send + Sync,
>;

/// The scripted draw loaded on this thread, to load again on a worker ([`load_draw`]).
pub(crate) fn carried_draw() -> Option<(DrawSlot, Vec<u8>)> {
    DRAW.with(|cell| cell.borrow().clone())
}

/// Load a carried scripted draw on this (worker) thread; `None` leaves the real legs standing.
pub(crate) fn load_draw(carried: Option<(DrawSlot, Vec<u8>)>) {
    if let Some((draw, png)) = carried {
        set_draw_picture_for_test(draw, png);
    }
}

/// The scripted draw leg: the script when one is loaded, otherwise the REAL submit at the address the
/// settings name (§F5.6 S6). A server that is down answers `Err` naming it, which the walk turns into the
/// failed-draw branch — publish.json already written, so the thinking survives (S5).
fn draw_leg() -> Box<crate::produce_upload::AskDraw> {
    match DRAW.with(|cell| cell.borrow().clone()) {
        Some((script, _)) => Box::new(move |body| script(body)),
        None => {
            let tree = session_tree();
            Box::new(move |body| crate::produce_upload_draw::submit(&tree, body))
        }
    }
}

/// The poll leg's cadence (§F5.6 S6) lives inside [`crate::produce_upload_draw::wait_until_done`], which logs
/// each status line as it goes and answers only once the job reports done, so `walk`'s loop sees a finished
/// state on its first ask. With a scripted draw the job is reported done at once and the scripted bytes are
/// the picture: a test asserts what landed rather than dialling a GPU.
fn poll_leg() -> Box<crate::produce_upload::Poll> {
    match scripted_png() {
        Some(png) => Box::new(move |job| {
            Ok(crate::produce_upload::PollState {
                status: format!("done {}", job.id),
                ahead: 0,
                done: true,
                // The scripted draw's own picture rides on the answer, so `save_drawn` writes those bytes
                // through the program's real write path. A script that laid its picture down instead of handing
                // it on has the file read back here, so no scripted shape skips the write path; a script with
                // neither leaves them empty and the walk says the job finished with no image — the same
                // sentence a live server earns for that.
                png: if png.is_empty() {
                    std::fs::read(session_tree().thumbnail_plain_png()).unwrap_or_default()
                } else {
                    png.clone()
                },
            })
        }),
        None => {
            let tree = session_tree();
            Box::new(move |job| {
                crate::produce_upload_draw::wait_until_done(
                    &tree,
                    job,
                    &|| false,
                    crate::ui::window::log_line,
                )
            })
        }
    }
}

/// Put a finished draw's bytes on disk as the plain picture (§F5.6 S6). When a scripted draw already laid its
/// picture down, this is the step that reads those bytes back and re-writes them, so the scripted path stays
/// the one the print leg prints from rather than a side door around it.
fn save_drawn(job: &crate::produce_upload::DrawnJob, png: &[u8]) -> Result<(), String> {
    let path = session_tree().thumbnail_plain_png();
    let bytes: Vec<u8> = if png.is_empty() {
        std::fs::read(&path).map_err(|why| format!("{}: {why}", path.display()))?
    } else {
        png.to_vec()
    };
    write_plain(&path, &bytes).map_err(|why| format!("job {}: {why}", job.id))
}

/// The write itself, split out so the folder is created for a project that has never drawn before.
fn write_plain(path: &std::path::Path, png: &[u8]) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|why| format!("{}: {why}", dir.display()))?;
    }
    std::fs::write(path, png).map_err(|why| format!("{}: {why}", path.display()))
}

/// The print leg: one pass of words onto the picture, drawn for real (§F5.6 S7). It starts from
/// `thumbnail-plain.png` (the plain picture is the base of both passes, so the second pass never prints over
/// the first's letters) and writes `thumbnail.png` through [`crate::produce_upload_print::print_words`],
/// which sizes the words with the repo's ONE fitter. Refuses when nothing has been drawn, which reads as the
/// same honest refusal as the draw leg's absence.
///
/// The band comes from the record the press started from, not from `publish.json`: §S3 snapshots the page's
/// state at the press, so a title box dragged since the last write prints where the person sees it, and only
/// a door with no run to ask (a bare `Run`) falls back to the file.
pub fn print_leg(band: TitleBox) -> Box<crate::produce_upload::PrintWords> {
    Box::new(move |_kind: &str, words: &[String]| {
        let tree = session_tree();
        let plain = tree.thumbnail_plain_png();
        if !plain.is_file() {
            return Err("nothing has been drawn to print onto".to_string());
        }
        let out = tree.thumbnail_png();
        crate::produce_upload_print::print_words(&plain, &out, words, &band)
    })
}

/// The title band to print across: the record's own box when its publish state carries one, else the app
/// default `{0.5, 0.25, 1, 0.4}` that `TitleBox::default()` spells. §S3 snapshots the page at the press, so a
/// box dragged since the last write prints where the person sees it rather than where an earlier run left it.
pub fn title_band_of(publish: &crate::project::Publish) -> TitleBox {
    publish.title_box.unwrap_or_default()
}

/// The band the title prints across, for a door that holds no run: the record on disk when it has one, else
/// the app default. The words half never asks this — `upload_legs` hands its print leg the run's own box.
fn stored_title_band() -> TitleBox {
    crate::publish::load(&session_tree())
        .ok()
        .and_then(|record| record.title_box)
        .unwrap_or_default()
}

/// The session's folder, resolved the way the page resolves everything else on this path.
pub fn session_tree() -> crate::layout::Tree {
    crate::layout::Tree::new(crate::startup::open_dir()).unwrap_or_else(|_| {
        crate::layout::Tree::new(std::path::Path::new("session.naivepost")).unwrap()
    })
}

/// The page's model leg: the REAL `youtube` call, always. Nothing here consults a scripted answer — a test
/// replaces the SERVER, not the work, by pointing the settings' address at a local fake LLM (the same seam
/// `tests/suite/produce_upload_wire.rs` proves), so every green test goes through the request, the tools and the
/// prose fallback a live server drives. The closure returns the reply text `upload_half` reads, and every
/// text-rewriting door on this page goes through it — ↻ suggest (`legs()` below) and the run's words half
/// (`upload_legs`) alike — so both dial the same `youtube` call at the address the settings name.
pub fn upload_ask(
    tree: &crate::layout::Tree,
    publish: &crate::project::Publish,
    moments: Vec<f64>,
    starts: Vec<f64>,
    edited_by_hand: bool,
) -> Box<crate::produce_upload::AskModel> {
    let tree = tree.clone();
    let publish = publish.clone();
    Box::new(move |brief: &str| {
        // Read at call time, like the interval below: these are the rows the brief was built from,
        // so `get_lines` answers with the same lines the fold dropped rather than a stale copy.
        let rows = crate::textfmt::read_session(&tree.session_tsv()).unwrap_or_default();
        let frames = crate::produce_upload_call::Frames {
            moments: &moments,
            starts: &starts,
            // Read at call time, from the session folder this tree names: Freq is a project field,
            // so a run that started before a Freq change still answers on the grid in force now.
            interval: crate::prepare::MODEL_FRAME_STEP,
            lines: &rows,
        };
        let cancelled = || false;
        let collected = crate::produce_upload_call::call(
            &tree,
            brief,
            &publish,
            &frames,
            edited_by_hand,
            &cancelled,
            crate::ui::window::log_line,
        )?;
        // The conversation wrote the record directly; hand back the text form `answers_from` reads, so
        // prose and tools land through the same parser and cannot drift apart.
        Ok(render_answers(&collected))
    })
}

/// The record a finished call left behind, in the labelled-line form `peel`/`answers_from` read.
fn render_answers(collected: &crate::produce_upload_call::Collected) -> String {
    let publish = &collected.publish;
    let mut lines = Vec::new();
    if !publish.thumb_title.trim().is_empty() {
        lines.push(format!("TITLE: {}", publish.thumb_title));
    }
    if publish.own {
        if let Some(frame) = publish.frames.first() {
            lines.push(format!("THUMBNAIL: {frame}"));
        }
    } else if !publish.prompt.trim().is_empty() {
        lines.push(format!("THUMBNAIL: {}", publish.prompt));
    }
    // §3.9's prose shape is "the three labelled lines, then the description as plain text": a fourth label
    // would not be peeled at all (`peel_labels` takes at most three), so the body rides unlabelled.
    lines.extend(publish.description.lines().map(str::to_string));
    lines.join("\n")
}

/// F5.7: the six page presses, driven through [`crate::produce_presses`]. Built here rather than in
/// `produce_page` because that file is at its size budget, and because three of the legs are already this
/// `produce_page` because that file is at its size budget, and because three of the legs are already this
/// module's private seams for the words half — reading them from one place keeps a redraw and the run's own
/// words half drawing with the SAME legs, so the two can never disagree about what was sent.
///
/// The page passes its own readers in as arguments rather than this module reaching back into it, so the
/// dependency runs one way: `produce_page` → `produce_presses`, with this module supplying the legs.
pub fn legs(
    has_cut: impl Fn() -> bool + 'static,
    rendering: impl Fn() -> bool + 'static,
    container: impl Fn() -> String + 'static,
    spans: impl Fn() -> Vec<(f64, f64)> + 'static,
) -> crate::produce_presses::Legs {
    use crate::produce_presses::Legs;
    // Each reader is shared through an `Rc` before it is captured: an `impl Fn` argument moves into the first
    // closure that takes it, and two of these legs read the same container (the video's name, above all).
    let video_container = std::rc::Rc::new(container);
    let for_video = Rc::clone(&video_container);
    let for_copy = video_container;
    let clip_spans = std::rc::Rc::new(spans);
    // The page's held state is read through `read_state` directly rather than passed in: it is a thread-local
    // clone, so every closure below can take its own copy of the reader without one move fighting another.
    Legs {
        // A cut exists when the Cut page's live segments do — read live, not off the Inputs row's count, so
        // a tweak that was never saved still counts as a cut (§F5.1 S3's same rule).
        has_cut: Box::new(has_cut),
        has_video: Box::new(move || session_tree().final_video(&for_video()).is_file()),
        has_picture: Box::new(|| session_tree().thumbnail_png().is_file()),
        rendering: Box::new(rendering),
        // The marked texts, or the title line, depending on which pass `print_order` is on. Whitespace-only
        // entries drop out so an empty box cannot claim a print pass it never had.
        words_of: Box::new(move |kind: &str| match kind {
            "texts" => crate::ui::produce_page::read_state()
                .texts
                .iter()
                .map(|mark| mark.text.clone())
                .filter(|text| !text.trim().is_empty())
                .collect(),
            _ => vec![crate::ui::produce_page::read_state().thumb_title]
                .into_iter()
                .filter(|text| !text.trim().is_empty())
                .collect(),
        }),
        instruction: Box::new(move || crate::ui::produce_page::read_state().instruction),
        // Everything after the base, in order — the same split `publish::references` makes.
        references: Box::new(move || {
            crate::ui::produce_page::read_state()
                .frames
                .iter()
                .skip(1)
                .cloned()
                .collect()
        }),
        // The band the title prints across comes from the record's own box when it has one; 0.25 (the upper
        // third) is the app's default, matching `details::thumbnail_box_without_aspect`.
        title_band: Box::new(|| stored_title_band().cy),
        brief: Box::new(move || upload_brief((*clip_spans)())),
        ask_draw: draw_leg(),
        poll: poll_leg(),
        save_picture: Box::new(save_drawn),
        // §S3: a press with no run in front of it still reads the record on disk, which is what "the page left
        // it" means for a door that snapshots nothing (↻ redraw and ↻ suggest build their own legs).
        print: print_leg(stored_title_band()),
        ask_model: upload_ask(
            &session_tree(),
            &crate::publish::load(&session_tree()).unwrap_or_default(),
            Vec::new(),
            Vec::new(),
            true,
        ),
        encode: Box::new(spawn_argv),
        copy: Box::new(move |file: &str| copy_video_out(file, &for_copy())),
        jpeg_sizes: Box::new(|path: &str| crate::ui::produce_languages::jpeg_export(path).0),
        // Was the run stopped (⏹) rather than did a leg fail? The shell's own switch answers: ⏹ writes
        // `cancel_leg::cancel_now` and `runqueue::end_run` clears it once the run has unwound, so "stopped"
        // and "failed — see log" are told apart by what the person actually pressed.
        was_stopped: Box::new(crate::cancel_leg::cancelled),
        // §F5.7's "clears `own`", gathered by the page that owns the flag (`produce_press_wiring::legs`
        // overrides this default with the real door). A door with no page to clear answers `Ok`: there is no
        // held state and nothing on disk to contradict.
        clear_own: Box::new(|| Ok(())),
        // The F5.6 words half does not count reprint passes here; it reports its own order elsewhere. The
        // page's presses turn this on in `produce_page::press_legs`.
        count_prints: false,
    }
}

/// The re-word's brief: the same [`crate::produce_upload::brief`] the run's words half sends, over the
/// spans the page holds. `at_output` accumulates in the order given, because the produced clock is the only
/// clock the brief speaks. The events and transcript are left out: only the title, instruction and
/// description come back from this call — the full brief belongs to ▶, not to ↻ beside Title.
fn upload_brief(spans: Vec<(f64, f64)>) -> String {
    let clips: Vec<crate::produce_upload::BriefClip> = spans
        .into_iter()
        .enumerate()
        .map(|(index, (start, end))| crate::produce_upload::BriefClip {
            number: index + 1,
            at_output: start,
            seconds: (end - start).max(0.0),
            session: format!(
                "{}\u{2013}{}",
                crate::tools::mm_ss(start),
                crate::tools::mm_ss(end)
            ),
            seen: vec![],
            said: vec![],
            // The re-word's brief is a title question over spans the page holds, with no cut segment behind
            // each one: nothing to read a rate off and nothing to mark, so the header keeps S1's plain form.
            rate: 1.0,
            marked: vec![],
        })
        .collect();
    crate::produce_upload::brief(&clips, &[], crate::produce_upload::BRIEF_MAX_CHARS)
}

/// Spawn an argv-shaped command through the page's spawner, so a transcode goes through the same door the
/// encodes do — scripted in a test, real otherwise.
fn spawn_argv(argv: &[String]) -> Result<(), String> {
    let command = crate::produce_exec::Command {
        step: "transcode",
        log: format!("transcode: {}", argv.join(" ")),
        argv: argv.to_vec(),
    };
    spawn(&command)
}

/// ⤓ export's written JPEG: the ladder runs into the file the uploader will be pointed at, and the weight
/// reported is that file's own size. Returns `(sizes, exported_path)`; the chosen rung's size and the path
/// come from one call, so the sentence and the bytes on disk cannot disagree.
pub fn jpeg_export(to: &str) -> (Vec<u64>, std::path::PathBuf) {
    let tree = session_tree();
    let out = std::path::PathBuf::from(to);
    // The export folder is not guaranteed to exist yet: a session can export its thumbnail before any encode
    // has written a byte into `produce/`, and ffmpeg will not create a folder for its output. Without this
    // every rung fails to write and the press wrongly answers "nothing to export".
    if let Some(dir) = out.parent() {
        std::fs::create_dir_all(dir)
            .map_err(|why| format!("{}: {why}", dir.display()))
            .ok();
    }
    let sizes = crate::produce_upload_media::jpeg_ladder(
        &tree.thumbnail_png(),
        &out,
        &crate::produce_runs::JPEG_QUALITIES,
        crate::ui::window::log_line,
    );
    (sizes, out)
}

/// ⤓ Save video's copy: the render's own output out to the chosen path. The stamp stays where ▶ wrote it,
/// so a copy elsewhere leaves the video up to date (§F5.7 S7).
fn copy_video_out(file: &str, container: &str) -> Result<(), String> {
    let from = session_tree().final_video(container);
    std::fs::copy(&from, file)
        .map(|_| ())
        .map_err(|why| format!("{file}: {why}"))
}

/// Put a press's failure where it can be read. The status line already carries `runs::ending`'s "see log",
/// so this writes the reason the log is worth opening for; a refusal brought no error and adds nothing.
pub fn report(out: &crate::produce_presses::Outcome) {
    if let Some(why) = &out.error {
        crate::ui::window::log_line(&format!("!!! {why}"));
    }
}

/// F5.5 S5: write the `<video>` tag page beside the video. Called from both doors of the run — after both
/// halves, and on the up-to-date skip too, because §F5.5 rewrites the page either way (its tracks are read
/// off disk, not out of the render). The known-language list comes from the settings file's
/// `subtitle_languages` (`code:tag:name`, 03-shell §6) so a track is offered under its proper name; with
/// no settings file there is no list and `embed::tracks` names each track by its code instead.
///
/// `language` is the session's, taken at the press: the page is written on the run's worker, which holds no
/// window session to read it from (§03-shell §1).
pub fn tag_page(settings: &crate::project::Produce, language: &str) {
    let tree = crate::layout::Tree::new(crate::startup::open_dir()).unwrap_or_else(|_| {
        crate::layout::Tree::new(std::path::Path::new("session.naivepost"))
            .expect("a session folder")
    });
    let known = crate::roles::subtitle_languages(
        &crate::settings::from_environment()
            .and_then(|paths| crate::settings::read(&paths).ok())
            .map(|conf| conf.subtitle_languages)
            .unwrap_or_default(),
    );
    crate::produce_tag_page::build_and_write(
        &tree,
        settings.container,
        settings.codec,
        language,
        &known,
        &*crate::produce_tag_page::poster_through(spawn),
        crate::ui::window::log_line,
    );
}
