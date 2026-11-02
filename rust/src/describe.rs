//! Describe — spec/04-prepare.md F1.7, the stage that turns a footage source's frames into an event
//! log.

use std::path::Path;

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
pub fn pick_frames(changes: &[f64], duration: f64, grid: f64, freq: f64) -> Vec<f64> {
    if duration <= 0.0 || grid <= 0.0 || freq <= 0.0 {
        return Vec::new();
    }
    let mut starts: Vec<f64> = vec![0.0];
    starts.extend(
        changes
            .iter()
            .filter(|at| **at > 0.0 && **at < duration)
            .copied(),
    );
    starts.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let mut out: Vec<f64> = Vec::new();
    for (index, start) in starts.iter().enumerate() {
        let end = starts.get(index + 1).copied().unwrap_or(duration);
        // The scene's first frame: snapped onto the grid it was actually extracted on, up rather
        // than down, so a change at 3.12 s names the frame at 3.25 s and not one before the change.
        let first = snap_up(*start, grid, duration);
        push_frame(&mut out, first);
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

fn push_frame(out: &mut Vec<f64>, at: f64) {
    if out.last().is_some_and(|last| (*last - at).abs() < 1e-9) {
        return;
    }
    out.push(at);
}

/// S0: the refusal for a folder extracted frame-by-frame.
pub fn every_frame_refusal(base: &str) -> String {
    format!("{base} was extracted as every-frame; describe needs the 250 ms grid -- rerun Prepare")
}

/// S0: does this folder look like an every-frame extraction rather than F1.6's?
pub fn is_every_frame(frames: &[f64], grid: f64) -> bool {
    if frames.len() < 2 || grid <= 0.0 {
        return false;
    }
    frames.windows(2).all(|pair| pair[1] - pair[0] < grid / 2.0)
}

// ---- S1: sorted, scaled, and heard alongside ---------------------------------

/// S1: refuse when there are no frames to describe.
pub fn refuse_missing(frames: &[f64]) -> Option<String> {
    if frames.is_empty() {
        return Some("no frames to describe -- the frames folder is empty".to_string());
    }
    None
}

/// S1: sort by stamp.
pub fn sorted(frames: &[f64]) -> Vec<f64> {
    let mut out = frames.to_vec();
    out.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    out
}

/// S1: the ffmpeg arguments that scale each frame ONCE into `.llmframes/`.
pub fn scale_plan(width: u32) -> Vec<String> {
    vec!["-vf".to_string(), format!("scale={width}:-2")]
}

/// S1: the name the scaled copy is written under — the same clock name the stored frame has (`base` is the
/// instant the stored folder's first picture names, [`crate::describe_scale::name_base`]; `at` counts from it), so a
/// row in `events.tsv` and a file in `.llmframes/` point at the same instant.
pub fn scaled_name(base: f64, at: f64) -> String {
    format!("{}.jpg", frame_name(base + at))
}

/// S1: the alongside-recording note.
pub fn hearing_log(video: &str, audio: &str, offset_secs: f64) -> String {
    format!(
        ">>> [{video}] hearing {audio} alongside it, starting {} s in",
        trim_seconds(offset_secs)
    )
}

pub(crate) fn trim_seconds(secs: f64) -> String {
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

/// S2: read `events.tsv`.
pub fn parse_events(tsv: &str) -> Vec<EventRow> {
    let mut out = Vec::new();
    for line in tsv.lines() {
        let line = line.trim_end();
        if line.is_empty() {
            continue;
        }
        let mut fields = line.splitn(3, '\t');
        let at = match fields.next().and_then(|head| head.parse::<f64>().ok()) {
            Some(at) => at,
            None => continue,
        };
        let first = match fields.next() {
            Some(first) => first,
            None => continue,
        };
        // Three fields and a numeric middle: §6's shape, so the text is the third field.
        let text = match (first.parse::<f64>(), fields.next()) {
            (Ok(_), Some(text)) => text,
            _ => first,
        };
        out.push(EventRow {
            at,
            text: text.to_string(),
        });
    }
    out
}

/// S2: the chunks that are already done, as start indices into `frames`.
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
/// first.
pub fn seed_history(rows: &[EventRow]) -> Vec<String> {
    let keep = describe::RECENT_EVENTS;
    rows.iter()
        .rev()
        .take(keep)
        .rev()
        .map(|row| row.text.clone())
        .collect()
}

/// S2: the STATE the next chunk carries.
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
    std::fs::read_to_string(tree.events_tsv(lane))
        .map(|text| parse_events(&text))
        .unwrap_or_default()
}

// ---- S3: the message --------------------------------------------------------

/// One frame's place in the request: its own session second and whether it opens a scene.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stamp {
    pub at: f64,
    pub new_scene: bool,
}

/// The speech the request carries, already split into the three sections.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Speech {
    pub before: Vec<String>,
    pub during: Vec<String>,
    pub after: Vec<String>,
}

/// S3: the whole user message, in the spec's exact order.
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
    out.push_str(&describe::speech_block(
        &speech.before,
        &speech.during,
        &speech.after,
    ));
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
/// shape §F1.7's layout shows.
fn stamp_text(at: f64) -> String {
    format!("{at:.1}")
}

// ---- S4: cache on the whole content ----------------------------------------

/// S4: the key naming this request's reply in `cache/llm/describe/`.
pub fn cache_key(system: &str, content: &str, images: &[Vec<u8>], model: &str) -> String {
    let request = crate::llm_cache::Request {
        system,
        user: content,
        state: "",
        speech: "",
        context: "",
        images: images
            .iter()
            .map(|bytes| crate::llm_request::data_url(bytes, "image/jpeg"))
            .collect(),
        run_index: None,
        // §6's rewrite addition: whose answer this was.
        model,
        // §2's row says thinking OFF for this job, so the flag never changes here — but it stays a
        // part, spelled once in [`crate::llm_cache::parts`] rather than assumed away.
        thinking: false,
    };
    // Text parts always key: `key` refuses only an empty list or a value JSON cannot encode, and
    // neither is reachable from a message plus a list of strings.
    crate::llm_cache::key(&crate::llm_cache::parts(&request)).expect("a describe ask always keys")
}

/// S4: the stored reply for this key, if any.
pub fn cached_reply(tree: &Tree, key: &str) -> Option<String> {
    crate::llm_cache::read_at(tree, STEP, key)
}

/// S4: store one reply under its key.
pub fn store_reply(tree: &Tree, key: &str, reply: &str) -> Result<(), String> {
    match crate::llm_cache::store_at(tree, STEP, key, reply) {
        crate::llm_cache::Stored::Failed(reason) => Err(reason),
        _ => Ok(()),
    }
}

/// S4: how many were answered from the cache, phrased for S7.
pub fn cached_note(cached: usize) -> String {
    format!(", {cached} answered from the cache")
}

// ---- S5: the tools, through one door ---------------------------------------

/// §3.2's prose floor: a reply written as the shipped prompt shows it ("EVENT [+14.2s]: …" lines and a
/// "STATE: …" line) read into the `record_event` / `set_state` calls it stands for, so it goes through the
/// same tool surface as a reply made of calls.
pub fn prose_calls(reply: &str, stamps: &[f64]) -> Vec<(String, serde_json::Value)> {
    let mut calls = Vec::new();
    let (rest, state) = match reply.find("STATE:") {
        Some(at) => (
            &reply[..at],
            Some(flatten_line(&reply[at + "STATE:".len()..])),
        ),
        None => (reply, None),
    };
    let stamped =
        regex::Regex::new(r"(?m)EVENT\s*\[\s*([+-]?[0-9]+(?:\.[0-9]+)?)\s*s?\s*\]\s*:\s*(.*)$")
            .expect("a fixed pattern");
    for line in stamped.captures_iter(rest) {
        let Ok(at) = line[1].parse::<f64>() else {
            continue;
        };
        let nearest = stamps
            .iter()
            .enumerate()
            .min_by(|a, b| (a.1 - at).abs().total_cmp(&(b.1 - at).abs()));
        if let Some((index, offset)) = nearest {
            if (offset - at).abs() <= PROSE_SNAP_SECONDS {
                calls.push((
                    "record_event".to_string(),
                    serde_json::json!({ "frame": index + 1, "text": flatten_line(&line[2]) }),
                ));
            }
        }
    }
    if calls.is_empty() && !stamps.is_empty() {
        let event = match rest.find("EVENT:") {
            Some(at) => Some(flatten_line(&rest[at + "EVENT:".len()..])),
            None if state.is_some() => Some(flatten_line(rest)),
            None => None,
        };
        if let Some(text) = event.filter(|text| !text.is_empty()) {
            calls.push((
                "record_event".to_string(),
                serde_json::json!({ "frame": 1, "text": text }),
            ));
        }
    }
    if let Some(text) = state {
        calls.push(("set_state".to_string(), serde_json::json!({ "text": text })));
    }
    calls
}

/// How far a prose line's stamp may sit from a frame's and still be that frame's: the request prints the
/// stamps to a tenth of a second, so a model copying them lands well inside this.
pub const PROSE_SNAP_SECONDS: f64 = 1.0;

pub(crate) fn flatten_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// S5: apply one tool call to the batch.
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
        other => crate::tools::error(&format!(
            "unknown tool {other} -- describe answers record_event, set_state and finish here; get_events, get_frames, get_context and speech_around are answered by the caller (see describe_call::read_answers)"
        )),
    }
}

// ---- S6: writing the log ---------------------------------------------------

/// S6: the rows to append for one chunk — one per frame, in frame order, each an §6 events row
/// ([`crate::textfmt::event_row`], times with two decimals).
pub fn events_rows(
    frames: &[f64],
    events: &[Event],
    has_history: bool,
    batch_end: f64,
) -> Vec<String> {
    let mut rows = Vec::new();
    for (index, at) in frames.iter().enumerate() {
        let frame = index + 1;
        let end = frames.get(index + 1).copied().unwrap_or(batch_end);
        let cold_opening = frame == 1 && !has_history;
        let text = match events.iter().find(|event| event.frame == frame) {
            Some(event) if cold_opening && describe::is_same(&event.text) => {
                Batch::SAME_WITHOUT_HISTORY.to_string()
            }
            Some(event) => event.text.clone(),
            None if cold_opening => Batch::SAME_WITHOUT_HISTORY.to_string(),
            None => "same".to_string(),
        };
        rows.push(crate::textfmt::event_row(*at, end, &text));
    }
    rows
}

/// S6: the whole contents of `state.txt` after a chunk.
pub fn state_line(state: Option<&str>) -> String {
    state.unwrap_or("").to_string()
}

/// S6: append this chunk's rows and rewrite `state.txt`.
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

/// Whether a frame opens a scene: it is the first frame of the take, or a change falls between it
/// and the frame before.
pub fn is_new_scene(at: f64, previous: Option<f64>, changes: &[f64]) -> bool {
    match previous {
        None => true,
        Some(previous) => changes
            .iter()
            .any(|change| *change > previous && *change <= at + 1e-9),
    }
}

/// The stage's opening summary for one footage source: what goes, and therefore what it costs.
pub fn pick_log(base: &str, frames: &[f64]) -> String {
    format!(
        ">>> [{base}] describe: {} frames -> {} requests",
        frames.len(),
        crate::roles::vision_batches(frames.len())
    )
}

/// The stage's entry point: what ▶ prints for one footage source before any request is made.
pub fn stage_log(
    base: &str,
    frames_on_disk: &[f64],
    changes: &[f64],
    duration: f64,
    freq: f64,
    alongside: &[(String, f64)],
) -> Vec<String> {
    let grid = GRID;
    // S0: an every-frame extraction stops here.
    if is_every_frame(frames_on_disk, grid) {
        return vec![every_frame_refusal(base)];
    }
    if frames_on_disk.is_empty() {
        return Vec::new();
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

/// The frames that go to the model for one footage source, read off disk: S0's pick over the frames F1.6
/// extracted and the scene changes the text gate (else the pixel pass) logged, in stamp order.
pub fn picked_for(tree: &Tree, lane: &str, frames_on_disk: &[f64]) -> Vec<f64> {
    let changes = read_changes(tree, lane);
    let duration = frames_on_disk.last().copied().unwrap_or(0.0) + GRID;
    sorted(&pick_frames(
        &changes,
        duration,
        GRID,
        crate::text_gate::HEARTBEAT_S,
    ))
}

/// How many requests describing one footage source takes, read off disk: its chunks, four picked frames a
/// request.
pub fn planned_requests(tree: &Tree, lane: &str) -> usize {
    let frames = frames_on_disk(&tree.frames_dir(lane));
    if frames.is_empty() || is_every_frame(&frames, GRID) {
        return 0;
    }
    chunk_plan(&picked_for(tree, lane, &frames), FRAMES_PER_REQ).len()
}

/// The stage's entry point for one footage source, read off disk: what ▶ prints before any request
/// is made.
pub fn stage_log_for(tree: &Tree, lane: &str, alongside: &[(String, f64)]) -> Vec<String> {
    let frames = frames_on_disk(&tree.frames_dir(lane));
    let changes = read_changes(tree, lane);
    let duration = frames.last().copied().unwrap_or(0.0) + GRID;
    stage_log(
        base_of_lane(lane),
        &frames,
        &changes,
        duration,
        crate::text_gate::HEARTBEAT_S,
        alongside,
    )
}

fn base_of_lane(lane: &str) -> &str {
    lane
}

/// This source's scene changes as F1.7 describes by: the text gate's (`textscenes.tsv`, F1.6 S6) when it ran and
/// is no older than the pixel pass, else the pixel pass's own ([`read_pixel_changes`]).
pub fn read_changes(tree: &Tree, lane: &str) -> Vec<f64> {
    crate::text_gate::changes_for_describe(tree, lane)
        .unwrap_or_else(|| read_pixel_changes(tree, lane))
}

/// The pixel pass's scene changes out of `scenes.tsv` (`time\tscore`, one row per change).
pub fn read_pixel_changes(tree: &Tree, lane: &str) -> Vec<f64> {
    let Ok(text) = std::fs::read_to_string(tree.scenes_tsv(lane)) else {
        return Vec::new();
    };
    text.lines()
        .filter_map(|line| line.split('\t').next()?.trim().parse::<f64>().ok())
        .collect()
}

/// The grid this stage assumes when none is given: the extraction grid, which is the finer of the two.
pub const GRID: f64 = EXTRACTION_GRID;

/// Read the frames this source has, as offsets into the take, from the names F1.6 wrote.
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
        return Vec::new();
    };
    for at in out.iter_mut() {
        *at -= first;
    }
    out
}
