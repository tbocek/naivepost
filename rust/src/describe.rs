//! Describe — spec/04-prepare.md F1.7, the stage that turns a footage source's frames into an event
//! log. One chunk of `P.machine.describeFramesPerReq` frames at a time.
//!
//! Three files hold this flow and each owns one part of it, so nothing is written twice:
//!
//! * [`crate::tools::describe`] — the §3.2 TOOL half: the batch, `record_event`, `set_state`,
//!   `finish`, the speech block, and the lenient parse of a prose reply.
//! * [`crate::frames`] — F1.6: what is on disk in `prepare/inputs/frames/<source>/`, the scene
//!   changes, and the fact that stored frames are the video's own size.
//! * this module — the FLOW: which frames go (S0), how they are scaled for sending (S1), how a
//!   stopped run resumes (S2), what the request says (S3), how it is cached (S4), where the answers
//!   are written (S6) and what gets logged (S7).
//!
//! As in those two modules, no process is spawned and no socket opened: the ffmpeg invocation for
//! scaling is returned as arguments ([`scale_plan`]) and the model call is not made here at all —
//! a step that decided inside a tool call could never be tested (spec/00-principles.md §5).

use std::path::Path;

use sha2::{Digest, Sha256};

use crate::clock::frame_name;
use crate::layout::Tree;
use crate::prepare::EXTRACTION_GRID;
use crate::roles::VISION_FRAMES_PER_CALL;
use crate::tools::describe::{self, Batch, Event};

/// The cache folder name under `cache/llm/<step>/` and the name the progress line uses.
pub const STEP: &str = "describe";

/// Frames per vision request. P.machine.describeFramesPerReq
pub const FRAMES_PER_REQ: usize = VISION_FRAMES_PER_CALL as usize;

/// The width the sent copies are scaled to. P.machine.describeFrameWidth
pub const FRAME_WIDTH: u32 = describe_frame_width();

/// Read from F1.6's own constant rather than restated, so the two cannot drift apart.
const fn describe_frame_width() -> u32 {
    crate::frames::LLM_FRAME_WIDTH
}

// ---- S0: which frames go ------------------------------------------------------

/// S0: the frames that go to the model, out of the 250 ms grid F1.6 extracted.
///
/// Per scene its first frame, then one every `freq` (`P.project.frameInterval`) until the next
/// change. Two things follow that the prototype got wrong in opposite directions: a scene shorter
/// than Freq sends its first frame ONLY (it does not pad up to a full interval), and the first
/// picture of every new scene always reaches the model — a slide's opening second is exactly the
/// frame a description of that slide has to start from.
///
/// `grid` is the extraction grid (`P.eng.frameGridSeconds`, 0.25) and stays finer than `freq`:
/// a scene boundary lands on the grid but almost never on Freq, and rounding it to Freq would drop
/// the one frame that must not be dropped. A step landing exactly on the next scene belongs to that
/// scene instead of being written twice — the same dedupe [`crate::frames::frame_seconds`] does.
pub fn pick_frames(changes: &[f64], duration: f64, grid: f64, freq: f64) -> Vec<f64> {
    if duration <= 0.0 || grid <= 0.0 || freq <= 0.0 {
        return Vec::new();
    }
    // Scene starts: the file's beginning plus every change inside it.
    let mut starts: Vec<f64> = vec![0.0];
    starts.extend(changes.iter().filter(|at| **at > 0.0 && **at < duration).copied());
    starts.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let mut out: Vec<f64> = Vec::new();
    for (index, start) in starts.iter().enumerate() {
        let end = starts.get(index + 1).copied().unwrap_or(duration);
        // The scene's first frame: snapped onto the grid it was actually extracted on, up rather
        // than down, so a change at 3.12 s names the frame at 3.25 s and not one before the change.
        let first = snap_up(*start, grid, duration);
        push_frame(&mut out, first);
        // Then one every Freq until the next change. Starting from the scene's own first frame is
        // what makes a short scene send exactly one: its next step falls past `end` and stops.
        let mut step = 1u32;
        loop {
            let at = first + freq * step as f64;
            if at >= end - 1e-9 {
                break;
            }
            push_frame(&mut out, snap_up(at, grid, duration));
            step += 1;
        }
    }
    out
}

/// Round a wanted second onto the extraction grid, upwards, clamped inside the take.
fn snap_up(at: f64, grid: f64, duration: f64) -> f64 {
    let steps = (at / grid).ceil();
    let snapped = steps * grid;
    if snapped >= duration {
        // The last grid step inside the take; a scene change in the final quarter second still has
        // to be described by the frame that exists rather than by one that was never extracted.
        let last = ((duration - grid).max(0.0) / grid).floor() * grid;
        last.max(0.0)
    } else {
        snapped
    }
}

/// Add a frame unless the list already ends with it — the "one frame, named once" rule where a Freq
/// step lands exactly on the next scene's start.
fn push_frame(out: &mut Vec<f64>, at: f64) {
    if out.last().is_some_and(|last| (*last - at).abs() < 1e-9) {
        return;
    }
    out.push(at);
}

/// S0: the refusal for a folder extracted frame-by-frame. Describe needs a known interval because
/// its stamps, its resume and its chunk boundaries are all arithmetic on it.
pub fn every_frame_refusal(base: &str) -> String {
    format!(
        "{base} was extracted as every-frame; describe needs a fixed interval -- rerun Prepare with e.g. 1s"
    )
}

/// S0: does this folder look like an every-frame extraction rather than F1.6's?
///
/// Decided from the frames themselves, not from a count against a guessed take length: the length is
/// not knowable here without paying for a media probe, and inferring it from a count would call a
/// short take every-frame and refuse work that is perfectly describable. What an every-frame set
/// cannot hide is its SPACING — its pictures sit `grid` apart all the way through because nothing
/// thinned them. A folder picked at any Freq coarser than the grid has gaps of about `freq`, so
/// comparing each consecutive pair against the grid settles it.
///
/// The test is deliberately strict: EVERY step must be within half a grid step. A scene change
/// restarts the grid, which makes one step shorter or longer legitimately, but F1.6 restarts it at
/// the change itself, so the steps around a change stay on the grid too; only an unthinned folder
/// has every step at exactly `grid`. A loose majority rule would refuse real extractions, and a
/// refusal is the expensive mistake here — it stops work on a folder that describes fine.
pub fn is_every_frame(frames: &[f64], grid: f64) -> bool {
    if frames.len() < 2 || grid <= 0.0 {
        return false;
    }
    frames
        .windows(2)
        .all(|pair| (pair[1] - pair[0] - grid).abs() <= grid / 2.0)
}

// ---- S1: sorted, scaled, and heard alongside ---------------------------------

/// S1: refuse when there are no frames to describe. An empty or missing folder is the case; a scene
/// shorter than Freq is NOT — that one sends its single first frame and is a normal thing.
pub fn refuse_missing(frames: &[f64]) -> Option<String> {
    if frames.is_empty() {
        return Some("no frames to describe -- the frames folder is empty".to_string());
    }
    None
}

/// S1: sort by stamp. The request's stamps and `events.tsv`'s rows both come off this order, so an
/// unsorted list would number frames against the wrong seconds.
pub fn sorted(frames: &[f64]) -> Vec<f64> {
    let mut out = frames.to_vec();
    out.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    out
}

/// S1: the ffmpeg arguments that scale each frame ONCE into `.llmframes/`.
///
/// `-vf scale=<width>:-2` keeps the aspect ratio and lands on an even height, which JPEG and every
/// codec downstream wants; `-vsync vfr` keeps one output per input. Nothing is spawned here, for
/// the reason [`crate::frames::extract_plan`] gives: the caller runs it and can test what would run.
///
/// The stored frames stay at the video's own size — [`crate::frames::extract_plan`] is untouched by
/// this function and by this stage. Measured on the ETH lecture a 1080p frame is 147 kB and its
/// 896-wide copy 55 kB, so keeping the grid native and scaling only the copies is what keeps 71
/// minutes at about 0.9 GB of sent pixels instead of 2.7 GB of stored ones.
pub fn scale_plan(width: u32, frames: &[f64]) -> Vec<String> {
    let mut args = vec![
        "-vf".to_string(),
        format!("scale={width}:-2"),
        "-fps_mode".to_string(),
        "passthrough".to_string(),
    ];
    for at in sorted(frames) {
        args.push(scaled_name(at));
    }
    args
}

/// S1: the name the scaled copy is written under — the same clock name the stored frame has, so a
/// row in `events.tsv` and a file in `.llmframes/` point at the same instant. [`crate::clock`]
/// owns that one spelling; this only adds the extension.
pub fn scaled_name(at: f64) -> String {
    format!("{}.jpg", frame_name(at))
}

/// S1: the alongside-recording note. A recording's words are placed against this video's clock by
/// its offset, and the log says which offset was used so a description that lines up oddly can be
/// checked against the number rather than guessed at.
pub fn hearing_log(video: &str, audio: &str, offset_secs: f64) -> String {
    format!(">>> [{video}] hearing {audio} alongside it, starting {} s in", trim_seconds(offset_secs))
}

/// Seconds without a tail of zeroes: `1.5`, not `1.500000`.
fn trim_seconds(secs: f64) -> String {
    if (secs - secs.round()).abs() < 1e-9 {
        return format!("{}", secs.round() as i64);
    }
    let text = format!("{secs:.3}");
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

// ---- S2: resume ------------------------------------------------------------

/// One row of `events.tsv` as far as resuming is concerned: the frame's session second and the text
/// written against it.
#[derive(Debug, Clone, PartialEq)]
pub struct EventRow {
    pub at: f64,
    pub text: String,
}

/// S2: read `events.tsv`. Tolerant by design — a half-written last line from a killed run must not
/// stop a resume. A line with no parsable leading second is dropped; a line whose text was cut off
/// mid-write still describes its frame, so it is kept rather than thrown away.
pub fn parse_events(tsv: &str) -> Vec<EventRow> {
    let mut out = Vec::new();
    for line in tsv.lines() {
        let line = line.trim_end();
        if line.is_empty() {
            continue;
        }
        let mut fields = line.splitn(2, '\t');
        let at = match fields.next().and_then(|head| head.parse::<f64>().ok()) {
            Some(at) => at,
            None => continue,
        };
        let text = fields.next().unwrap_or("").to_string();
        out.push(EventRow { at, text });
    }
    out
}

/// S2: the chunks that are already done, as start indices into `frames`.
///
/// A chunk is skipped when its START second already has a row: the start is the one frame whose
/// presence proves the whole chunk was answered, because the row is written after the answer arrives
/// (§6). Matching on any frame of the chunk instead would skip a chunk that died halfway and leave
/// its middle unanswered forever.
pub fn starts_already_logged(rows: &[EventRow], frames: &[f64], per_req: usize) -> Vec<usize> {
    chunk_plan(frames, per_req)
        .into_iter()
        .filter(|start| {
            let at = match frames.get(*start) {
                Some(at) => *at,
                None => return false,
            };
            rows.iter().any(|row| (row.at - at).abs() < 1e-9)
        })
        .collect()
}

/// S2: the rolling window seeded from disk — the LAST [`describe::RECENT_EVENTS`] rows, oldest
/// first. More than that would repeat the whole log in every request; fewer would leave the model
/// unable to say "same" meaningfully.
pub fn seed_history(rows: &[EventRow]) -> Vec<String> {
    let keep = describe::RECENT_EVENTS;
    rows.iter()
        .rev()
        .take(keep)
        .rev()
        .map(|row| row.text.clone())
        .collect()
}

/// S2: the STATE the next chunk carries. With no `state.txt` the recording has just begun, which is
/// a real answer and not an empty one: it tells the model there is nothing behind the first frame.
pub fn seed_state(state_txt: Option<&str>) -> String {
    match state_txt.map(str::trim) {
        Some(text) if !text.is_empty() => text.to_string(),
        _ => "Recording just started.".to_string(),
    }
}

/// S2: read `state.txt` if it is there.
pub fn read_state(tree: &Tree, lane: &str) -> Option<String> {
    std::fs::read_to_string(tree.describe_state(lane)).ok()
}

/// S2: read `events.tsv` if it is there.
pub fn read_events(tree: &Tree, lane: &str) -> Vec<EventRow> {
    std::fs::read_to_string(tree.events_tsv(lane)).map(|text| parse_events(&text)).unwrap_or_default()
}

// ---- S3: the message --------------------------------------------------------

/// One frame's place in the request: its own session second and whether it opens a scene.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stamp {
    pub at: f64,
    pub new_scene: bool,
}

/// The speech the request carries, already split into the three sections. Kept as three lists
/// because the caps differ per side (≤ [`describe::CTX_SEGS`] per source within
/// [`describe::CTX_WINDOW_SECONDS`]) and the middle is uncapped — see
/// [`Batch::speech_around`].
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Speech {
    pub before: Vec<String>,
    pub during: Vec<String>,
    pub after: Vec<String>,
}

/// S3: the whole user message, in the spec's exact order.
///
/// The `Just before this:` block is left out entirely when there is no history rather than printed
/// with nothing after it: the spec shows it carrying the last events, and an empty heading invites
/// the model to invent a lead-in. The three speech sections are NEVER omitted — §3.2 says all three
/// are always present, with `(none)` where a side is empty, because a section the model cannot see
/// is a section it wonders about.
///
/// Stamps are written from each frame's own second through [`stamp`], never derived as
/// `first + i * interval`: the frames that go are scene-starts plus Freq steps, so their spacing is
/// uneven by construction and a derived stamp would misdate every frame after a scene change.
pub fn message(
    context: &str,
    state: &str,
    from: f64,
    to: f64,
    last_events: &[String],
    speech: &Speech,
    stamps: &[Stamp],
) -> String {
    let mut out = String::new();
    if !context.trim().is_empty() {
        out.push_str(context.trim_end());
        out.push('\n');
    }
    out.push_str(&format!("STATE so far: {state}\n"));
    out.push_str(&format!(
        "Frames cover t={}s to t={}s. A frame marked NEW SCENE is the first after a scene change.\n",
        trim_seconds(from),
        trim_seconds(to)
    ));
    if !last_events.is_empty() {
        out.push_str("Just before this: ");
        out.push_str(&last_events.join(" | "));
        out.push('\n');
    }
    out.push_str(&describe::speech_block(&speech.before, &speech.during, &speech.after));
    let total = stamps.len();
    for (index, stamp) in stamps.iter().enumerate() {
        let label = if stamp.new_scene { ", NEW SCENE" } else { "" };
        out.push_str(&format!(
            "[+{}s] FRAME {} of {}{label}\n",
            stamp_text(stamp.at),
            index + 1,
            total
        ));
    }
    out
}

/// A frame's stamp: its own second to one decimal, no unit inside the brackets — `[+0.8s]`, the
/// shape §F1.7's layout shows. [`crate::tools::tenths`] is not used because it appends a space
/// before the `s` for status lines, which would print `[+0.8 s s]` here.
fn stamp_text(at: f64) -> String {
    format!("{at:.1}")
}

// ---- S4: cache on the whole content ----------------------------------------

/// S4: the key naming this request's reply in `cache/llm/describe/`.
///
/// Hashed over the whole request content with the images inline — the scaled bytes, not their file
/// names — because §6 keys on what was asked: two identical requests cost nothing the second time,
/// and a re-encoded frame is a different question even under the same file name. Names alone would
/// make a changed picture a cache hit, which is the worst kind of stale answer: invisible.
///
/// [`crate::llm_cache::key`] is the shared implementation for the parts shape the other jobs use;
/// this stage hashes a plain string because its content is one text blob plus N images, and the
/// digest is fed to the same [`Tree::cache_llm`] path.
pub fn cache_key(content: &str, images: &[Vec<u8>]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    hasher.update([0u8]);
    for image in images {
        hasher.update(image);
        hasher.update([0u8]);
    }
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        hex.push_str(&format!("{byte:02x}"));
    }
    hex
}

/// S4: the stored reply for this key, if any. Any unreadable file is a miss, so the step asks again
/// rather than failing — the rule [`crate::llm_cache::read`] states for every cache here.
pub fn cached_reply(tree: &Tree, key: &str) -> Option<String> {
    std::fs::read_to_string(tree.cache_llm(STEP, key)).ok()
}

/// S4: store one reply under its key. A reply that will not write is reported, not swallowed: the
/// answer is still real, only unpaid-for next time.
pub fn store_reply(tree: &Tree, key: &str, reply: &str) -> Result<(), String> {
    let path = tree.cache_llm(STEP, key);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| format!("{}: {err}", parent.display()))?;
    }
    std::fs::write(&path, reply).map_err(|err| format!("{}: {err}", path.display()))
}

/// S4: how many were answered from the cache, phrased for S7. Empty when none were, so the caller
/// picks [`complete_log`] instead.
pub fn cached_note(cached: usize) -> String {
    format!(", {cached} answered from the cache")
}

// ---- S5: the tools, through one door ---------------------------------------

/// S5: apply one tool call to the batch. The three names are §3.2's; their rules live in
/// [`crate::tools::describe`] and are not restated here.
///
/// An unknown name comes back as an error rather than a panic: a model that calls something that is
/// not offered has to be told in the channel it is listening on.
pub fn apply_tool(batch: &mut Batch, name: &str, args: &serde_json::Value) -> String {
    match name {
        "record_event" => {
            let frame = args.get("frame").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            let text = args.get("text").and_then(|v| v.as_str()).unwrap_or("");
            let calm = args.get("calm").and_then(|v| v.as_bool()).unwrap_or(false);
            batch.record_event(frame, text, calm)
        }
        "set_state" => {
            let text = args.get("text").and_then(|v| v.as_str()).unwrap_or("");
            batch.set_state(text)
        }
        "finish" => {
            // `Missing` carries no Serialize impl (it is a test-facing type), so the answer is built
            // here rather than by serialising it: what the model needs is which frames are still
            // unanswered and why, in the same words `Batch::finish` decided.
            let missing = batch.finish();
            if missing.is_empty() {
                crate::tools::ok(&serde_json::json!({"missing": 0}))
            } else {
                let frames: Vec<usize> = missing.iter().map(|m| m.frame).collect();
                crate::tools::ok(&serde_json::json!({
                    "missing": frames.len(),
                    "frames": frames,
                    "first_frame_has_no_history": missing.iter().any(|m| m.reason
                        == describe::MissingReason::FirstFrameWithNoHistory),
                }))
            }
        }
        other => crate::tools::error(&format!("unknown tool {other} -- describe offers record_event, set_state, finish")),
    }
}

// ---- S6: writing the log ---------------------------------------------------

/// S6: the rows to append for one chunk — one per frame, in frame order.
///
/// A frame the model left unanswering is written `same`, which is what the word means here: nothing
/// changed since the row before. The exception is the batch's FIRST frame with no history: there is
/// nothing behind it to be the same as, so writing `same` would assert a fact about a second nobody
/// watched. That frame is left OUT of these rows, which is what makes it visible to the caller as
/// still unanswered — the same condition [`Batch::finish`] reports as
/// [`describe::MissingReason::FirstFrameWithNoHistory`].
pub fn events_rows(frames: &[f64], events: &[Event], has_history: bool) -> Vec<String> {
    let mut rows = Vec::new();
    for (index, at) in frames.iter().enumerate() {
        let frame = index + 1;
        match events.iter().find(|event| event.frame == frame) {
            Some(event) => rows.push(format!("{}\t{}", trim_seconds(*at), event.text)),
            None if frame == 1 && !has_history => continue,
            None => rows.push(format!("{}\tsame", trim_seconds(*at))),
        }
    }
    rows
}

/// S6: the whole contents of `state.txt` after a chunk. The running state, or the empty string when
/// the model cleared it — clearing has to reach the file, or the old state is read forward by the
/// next chunk as though it still applied.
pub fn state_line(state: Option<&str>) -> String {
    state.unwrap_or("").to_string()
}

/// S6: append this chunk's rows and rewrite `state.txt`. Both happen after EVERY chunk, which is
/// what makes the resume in S2 correct: the file pair describes exactly the frames answered so far.
pub fn write_chunk(
    tree: &Tree,
    lane: &str,
    rows: &[String],
    state: Option<&str>,
) -> Result<(), String> {
    std::fs::create_dir_all(tree.describe_dir(lane))
        .map_err(|err| format!("{}: {err}", tree.describe_dir(lane).display()))?;
    if !rows.is_empty() {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(tree.events_tsv(lane))
            .map_err(|err| format!("{}: {err}", tree.events_tsv(lane).display()))?;
        for row in rows {
            writeln!(file, "{row}").map_err(|err| format!("events.tsv: {err}"))?;
        }
    }
    std::fs::write(tree.describe_state(lane), state_line(state))
        .map_err(|err| format!("state.txt: {err}"))
}

// ---- S7: what gets said ----------------------------------------------------

/// S7: the pass finished with everything asked live.
pub fn complete_log(base: &str, chunks: usize) -> String {
    format!(">>> [{base}] event log complete ({chunks} chunks)")
}

/// S7: the same, when some chunks came from the cache — the sentence that tells a reader why this
/// run took four seconds.
pub fn complete_log_cached(base: &str, chunks: usize, cached: usize) -> String {
    format!(">>> [{base}] event log complete ({chunks} chunks, {cached} answered from the cache)")
}

// ---- the chunk plan --------------------------------------------------------

/// The start index of each chunk: `per_req` frames at a time, the last one allowed to be short.
pub fn chunk_plan(frames: &[f64], per_req: usize) -> Vec<usize> {
    if per_req == 0 {
        return Vec::new();
    }
    (0..frames.len()).step_by(per_req).collect()
}

/// How many requests these frames take. Delegated to [`crate::prepare::vision_calls`] so this and
/// the `Inputs:` readout F1.1 logs cannot disagree about what a chunk costs.
pub fn requests_for(frames: usize) -> usize {
    crate::prepare::vision_calls(frames)
}

/// Whether a frame opens a scene: it is the first frame of the take, or a change falls between it
/// and the frame before. Measured against the changes rather than against equal spacing, because
/// that is the only truth about where a scene starts.
pub fn is_new_scene(at: f64, previous: Option<f64>, changes: &[f64]) -> bool {
    match previous {
        None => true,
        Some(previous) => changes.iter().any(|change| *change > previous && *change <= at + 1e-9),
    }
}

/// The stage's opening summary for one footage source: what goes, and therefore what it costs.
pub fn pick_log(base: &str, frames: &[f64]) -> String {
    format!(">>> [{base}] describe: {} frames -> {} requests", frames.len(), requests_for(frames.len()))
}

/// The stage's entry point: what ▶ prints for one footage source before any request is made.
///
/// One function so the UI callback stays a forwarder (spec/00-principles.md §5): it returns lines,
/// it never decides in the caller. It PLANS and reports — nothing is spawned and no vision server is
/// needed, because every rule here is arithmetic over files F1.6 already wrote and over the scene
/// changes read from `scenes.tsv`. A source whose frames are missing answers with a refusal line
/// rather than a panic: the run carries on with the sources that do have frames, and the failure
/// names the source it happened to.
///
/// The order of the returned lines is the order the spec puts them in: S0's every-frame refusal
/// first if that is what the folder holds; otherwise the S1 note for each recording heard alongside,
/// then S0's pick summary naming how many frames go and therefore how many requests that costs.
pub fn stage_log(
    base: &str,
    frames_on_disk: &[f64],
    changes: &[f64],
    duration: f64,
    freq: f64,
    alongside: &[(String, f64)],
) -> Vec<String> {
    let grid = GRID;
    // S0: an every-frame extraction stops here. Describing it would stamp frames whose spacing we
    // never chose, and the resume arithmetic would skip chunks that were never written that way.
    if is_every_frame(frames_on_disk, grid) {
        return vec![every_frame_refusal(base)];
    }
    // No frames on disk at all: there is no take to measure, so nothing is picked and the source is
    // reported. A duration of zero would otherwise still yield one frame at t=0, which describes a
    // picture that does not exist.
    if frames_on_disk.is_empty() {
        // The sentence is built here rather than through `refuse_missing`, which answers only for an
        // empty LIST and would say nothing about a source that has no folder at all.
        return vec![format!(">>> [{base}] no frames to describe -- the frames folder is empty")];
    }
    let picked = sorted(&pick_frames(changes, duration, grid, freq));
    if let Some(refusal) = refuse_missing(&picked) {
        return vec![format!(">>> [{base}] {refusal}")];
    }
    let mut lines = Vec::new();
    // S1: which recordings are heard alongside this video, and from what offset — said before the
    // count, because the words are what the descriptions will be built from.
    for (audio, offset) in alongside {
        lines.push(hearing_log(base, audio, *offset));
    }
    lines.push(pick_log(base, &picked));
    lines
}

/// The stage's entry point for one footage source, read off disk: what ▶ prints before any request
/// is made. [`stage_log`] holds the rules; this only gathers what they read — the frames F1.6
/// extracted and the scene changes it logged.
///
/// The take's length comes from the frames themselves rather than from a probe of the media file: the
/// grid stops at the last picture that exists, so describing against that is describing against what
/// will actually be sent, and no ffprobe run is paid for at press time.
pub fn stage_log_for(
    tree: &Tree,
    lane: &str,
    freq: f64,
    alongside: &[(String, f64)],
) -> Vec<String> {
    let frames = frames_on_disk(&tree.frames_dir(lane));
    let changes = read_changes(tree, lane);
    // Length as the last frame plus one grid step: that is where the extraction stopped.
    let duration = frames.last().copied().unwrap_or(0.0) + GRID;
    stage_log(base_of_lane(lane), &frames, &changes, duration, freq, alongside)
}

/// The name a log line uses for a lane: the lane is the file's base name already, so this only says so.
fn base_of_lane(lane: &str) -> &str {
    lane
}

/// Read this source's scene changes out of `scenes.tsv` (`time\tscore`, one row per change).
pub fn read_changes(tree: &Tree, lane: &str) -> Vec<f64> {
    let Ok(text) = std::fs::read_to_string(tree.scenes_tsv(lane)) else {
        return Vec::new();
    };
    text.lines()
        .filter_map(|line| line.split('\t').next()?.trim().parse::<f64>().ok())
        .collect()
}

/// The grid this stage assumes when none is given: the extraction grid, which is the finer of the two.
pub const GRID: f64 = EXTRACTION_GRID;

/// Read the frames this source has, as offsets into the take, from the names F1.6 wrote. Kept small
/// and separate so the pure rules above never touch the filesystem.
pub fn frames_on_disk(dir: &Path) -> Vec<f64> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    // The seconds are read out of the file names with `clock::read_frame`, the inverse of the
    // `frame_name` F1.6 wrote them with — parsing the fields rather than a slice of the tail, so a
    // name that is not the fixed shape reports nothing instead of reporting nonsense.
    let mut out: Vec<f64> = entries
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().to_string();
            let stem = name.strip_suffix(".jpg")?;
            crate::clock::read_frame(stem)
        })
        .collect();
    // Rebased on the earliest picture that IS on disk: the grid is named from the recording's own
    // clock, but every rule here works in seconds-from-the-start of the take, which is what the
    // request's `t=` line and the `[+N.Ns]` stamps count from.
    out.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let Some(&first) = out.first() else {
        // Nothing readable in the folder means nothing to rebase: report an empty list so the caller
        // answers with the missing-frames refusal rather than inventing a frame at t=0.
        return Vec::new();
    };
    for at in out.iter_mut() {
        *at -= first;
    }
    out
}


