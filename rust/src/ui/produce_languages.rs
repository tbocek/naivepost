//! F5.4 §S3/§S7/§S9 — the Produce page's Translate row: its ticks, what a tick does to the state,
//! and the render's spawner seam. Split out of `produce_page` because that file is at its size budget;
//! `wire_settings` calls [`wire_ticks`] and `finish_produce` passes [`spawn`] to the render.

use adw::prelude::*;
use gtk4 as gtk;
use std::rc::Rc;

use crate::produce_screen as screen;
use crate::ui::produce_page::ProduceState;

/// What a tick hands back to the page: `guarded` reads the repaint guard, `changed` records the tick.
type Guard = Rc<dyn Fn() -> bool>;
type Changed = Rc<dyn Fn(&str, bool)>;

/// Tick every `translate-tick-<language>` the page drew. `guarded` answers the page's repaint guard (a
/// rebuild must not read as an edit) and `changed` records the tick — both stay the page's own, so this
/// module touches none of its thread-locals. A tick has no value string and the row holds several
/// languages at once, which is why the toggle's own state is the answer and `set_setting` is not used.
pub fn wire_ticks(
    window: &adw::ApplicationWindow,
    guarded: Guard,
    changed: Changed,
) {
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
pub fn upload_half(run: &crate::produce_flow::Run) -> Result<(), String> {
    let tree = session_tree();
    // S1: the brief's two sides. `seen` comes from Describe's events.tsv for the recording the clip reads,
    // `said` from the session transcript; both are read once over the whole cut rather than per clip, so a
    // long session costs three file reads and not one per clip.
    let rows = crate::textfmt::read_session(&tree.session_tsv()).unwrap_or_default();
    let events = read_all_events(&tree, run);
    let clips = brief_clips(run, &rows, &events);
    let brief = crate::produce_upload::brief(&clips, &run.lines, crate::produce_upload::BRIEF_MAX_CHARS);
    // The model answers through the scripted seam when a test loaded one, and honestly refuses otherwise:
    // `walk` then stops before writing anything and `finish_produce` logs `words_failed`, which keeps the
    // failure branch reachable instead of stubbed out.
    let ask = crate::produce_upload::scripted_ask();
    let reply = ask(&brief)?;
    let answers = answers_from(&reply, starts_of(run), |at| nearest_frame(&tree, run, at));
    let publish = crate::publish::load(&tree).unwrap_or_default();
    let mats = crate::produce_upload::Materials {
        save_publish: &|next| save_publish(&tree, next),
        ask_model: &|_| Ok(String::new()),
        ask_draw: &draw_leg(),
        poll: &poll_leg(),
        print: &print_leg(),
        nearest_frame: &|at| nearest_frame(&tree, run, at),
        stored_stamp: read_picture_stamp(&tree),
        write_stamp: &|hash| write_picture_stamp(&tree, hash),
        // The picture is the user's own once they hold a frame or have written an edit instruction of their
        // own; until then a title change reprints (S5).
        edited_by_hand: publish.own || !publish.prompt.trim().is_empty(),
    };
    let outcome = crate::produce_upload::walk(&brief, &publish, &answers, &mats, crate::ui::window::log_line);
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
    let raw = if seg.dur > 0.0 { seg.dur } else { (seg.e - seg.s).max(0.0) };
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
    base.rsplit_once('.').map_or(base, |(stem, _)| stem).to_string()
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
    for seg in &run.cut.segs {
        let kept = output_length(seg);
        if !seg.ins.is_empty() {
            at += kept;
            continue;
        }
        for source in run.sources.footage.iter() {
            let dir = tree.frames_dir(&source_stem(source));
            let Ok(entries) = std::fs::read_dir(&dir) else { continue };
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                let Some(seconds) = frame_seconds(&name) else { continue };
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

/// The seconds a frame file names. `frames::frame_file` writes `<YYYY-MM-DD_HH-MM-SS.mmm>.jpg` off the
/// recording's own clock; the inverse reads those fields back rather than trusting a bare number, because
/// the date part is not a duration and parsing it as one would put every frame in the wrong year.
fn frame_seconds(name: &str) -> Option<f64> {
    let stem = name.strip_suffix(".jpg")?;
    let (date, rest) = stem.split_once('_')?;
    let parts: Vec<&str> = rest.split('-').collect();
    if parts.len() != 3 {
        return None;
    }
    let hours: f64 = parts[0].parse().ok()?;
    let minutes: f64 = parts[1].parse().ok()?;
    let secs: f64 = parts[2].parse().ok()?;
    // The date is calendar, not elapsed time; only the time-of-day field is meaningful as a recording
    // offset, which is what the prototype's frame names carry for a session started at midnight.
    let _ = date;
    Some(hours * 3600.0 + minutes * 60.0 + secs)
}

/// The nearest extracted frame to a produced-clock moment (S3 / decision 32).
fn nearest_frame(tree: &crate::layout::Tree, run: &crate::produce_flow::Run, at: f64) -> Option<f64> {
    crate::produce_upload::nearest(&extracted_frames(tree, run), at)
}

/// publish.json + description.txt, both through the existing writer so one spelling of each path backs
/// them. Called BEFORE any draw (S5), which is why a failed draw keeps the thinking.
fn save_publish(tree: &crate::layout::Tree, publish: &crate::project::Publish) -> Result<(), String> {
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
    static DRAW: std::cell::RefCell<Option<DrawSlot>> = const { std::cell::RefCell::new(None) };
}

/// The scripted sd.cpp submit leg.
pub type DrawSlot = std::rc::Rc<dyn Fn(&serde_json::Value) -> Result<crate::produce_upload::DrawnJob, String>>;

/// Load a scripted draw for the next words half on this thread.
pub fn set_draw_for_test(draw: DrawSlot) {
    DRAW.with(|cell| cell.replace(Some(draw)));
}

/// The scripted draw leg: the script when one is loaded, otherwise an honest refusal that leaves the
/// already-saved text standing (S5) and gets logged by `produce_flow::words_failed`.
fn draw_leg() -> Box<crate::produce_upload::AskDraw> {
    match DRAW.with(|cell| cell.borrow().clone()) {
        Some(script) => Box::new(move |body| script(body)),
        None => Box::new(|_| Err("no image server here".to_string())),
    }
}

/// The poll leg. With a scripted draw the job is reported done at once — a test asserts the status line,
/// not the waiting — and without one there is nothing to poll, which the walk turns into the same failure
/// the submit leg gave.
fn poll_leg() -> Box<crate::produce_upload::Poll> {
    match DRAW.with(|cell| cell.borrow().clone()) {
        Some(_) => Box::new(|job| {
            Ok(crate::produce_upload::PollState {
                status: format!("done {}", job.id),
                ahead: 0,
                done: true,
            })
        }),
        None => Box::new(|_| Err("no image server here".to_string())),
    }
}

/// The print leg: one pass of words onto the picture. It copies `thumbnail-plain.png` over `thumbnail.png`
/// on every call (idempotent — the plain picture is the base of both passes) and refuses when nothing has
/// been drawn, which reads as the same honest refusal as the draw leg's absence. The lettering of a kind's
/// words is the local draw that `crate::produce_runs::reprint` sizes; this leg stands in for it until that
/// draw is wired to the page.
fn print_leg() -> Box<crate::produce_upload::PrintWords> {
    Box::new(|_kind: &str, _words: &[String]| {
        let tree = session_tree();
        let plain = tree.thumbnail_plain_png();
        if !plain.is_file() {
            return Err("nothing has been drawn to print onto".to_string());
        }
        let out = tree.thumbnail_png();
        std::fs::copy(&plain, &out).map_err(|why| format!("{}: {why}", out.display()))?;
        Ok(())
    })
}

/// The session's folder, resolved the way the page resolves everything else on this path.
fn session_tree() -> crate::layout::Tree {
    crate::layout::Tree::new(crate::startup::session_dir(
        &std::env::current_dir().unwrap_or_default(),
    ))
    .unwrap_or_else(|_| crate::layout::Tree::new(std::path::Path::new("session.naivepost")).unwrap())
}

/// F5.5 S5: write the `<video>` tag page beside the video. Called from both doors of the run — after both
/// halves, and on the up-to-date skip too, because §F5.5 rewrites the page either way (its tracks are read
/// off disk, not out of the render). The known-language list comes from the settings file's
/// `subtitle_languages` (`code:tag:name`, 03-shell §6) so a track is offered under its proper name; with
/// no settings file there is no list and `embed::tracks` names each track by its code instead.
pub fn tag_page(settings: &crate::project::Produce) {
    let tree = crate::layout::Tree::new(crate::startup::session_dir(
        &std::env::current_dir().unwrap_or_default(),
    ))
    .unwrap_or_else(|_| {
        crate::layout::Tree::new(std::path::Path::new("session.naivepost")).expect("a session folder")
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
        &crate::ui::window::live_project().language,
        &known,
        &*crate::produce_tag_page::poster_through(spawn),
        crate::ui::window::log_line,
    );
}
