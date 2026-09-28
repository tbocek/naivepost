//! F5.4 §S3/§S7/§S9 — the Produce page's Translate row: its ticks, what a tick does to the state,
//! and the render's spawner seam. Split out of `produce_page` because that file is at its size budget;
//! `wire_settings` calls [`wire_ticks`] and `finish_produce` passes [`spawn`] to the render.
//!
//! F5.7 lives here too: [`legs`] builds the six presses' injected legs from the page's held state and the
//! scripted seams this module already owns, so a redraw and the run's own words half draw with the SAME legs
//! and cannot disagree about what was sent.
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

/// F5.7: the six page presses, driven through [`crate::produce_presses`]. Built here rather than in
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
        references: Box::new(move || crate::ui::produce_page::read_state().frames.iter().skip(1).cloned().collect()),
        // The band the title prints across comes from the record's own box when it has one; 0.25 (the upper
        // third) is the app's default, matching `details::thumbnail_box_without_aspect`.
        title_band: Box::new(|| {
            crate::publish::load(&session_tree())
                .ok()
                .and_then(|record| record.title_box)
                .map_or(0.25, |band| band.cy)
        }),
        brief: Box::new(move || upload_brief((*clip_spans)())),
        ask_draw: draw_leg(),
        poll: poll_leg(),
        print: print_leg(),
        ask_model: crate::produce_upload::scripted_ask(),
        encode: Box::new(spawn_argv),
        copy: Box::new(move |file: &str| copy_video_out(file, &for_copy())),
        jpeg_sizes: Box::new(jpeg_ladder_sizes),
        // Each press runs inside one click and this page owns no cancel flag of its own, so nothing reports
        // a stop; wiring the shell's flag replaces this closure and nothing else.
        was_stopped: Box::new(|| false),
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

/// The export ladder's weight at each rung of [`crate::produce_runs::JPEG_QUALITIES`]. No encoder runs in
/// this container, so every rung is measured as the picture that exists: the first fitting rung is then the
/// current file whenever it fits, and the LAST rung otherwise — §F5.7's "last attempt written even if it
/// still doesn't", reached without an encoder. A real ladder fills these with one measurement per rung.
fn jpeg_ladder_sizes() -> Vec<u64> {
    let bytes = session_tree()
        .thumbnail_png()
        .metadata()
        .map(|meta| meta.len())
        .unwrap_or(0);
    crate::produce_runs::JPEG_QUALITIES.iter().map(|_| bytes).collect()
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
