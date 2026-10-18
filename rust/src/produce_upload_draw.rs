//! F5.6 S6: the drawing server, over HTTP.
//!
//! §F5.6 says the picture is drawn by sd.cpp and asked about **once per second**, and §02-services#1 says
//! the address comes from the settings file re-read per request — so a settings edit lands on the next
//! poll rather than being frozen for the whole draw. This module is that wire: [`submit`] queues the job at
//! `POST /sdcpp/v1/img_gen`, [`poll`] asks about it at `GET /sdcpp/v1/jobs/{id}`, and [`wait_until_done`]
//! drives the one-second cadence and logs each status line through [`crate::produce_upload::poll_line`].
//! [`cancel`] gives up on a job — `POST /sdcpp/v1/jobs/{id}/cancel` — which is what makes ⏹ an abort
//! rather than a shrug (§02-services#1's last paragraph).
//! The endpoint rules come from [`crate::services`], the method/path from [`crate::services::Kind`], the
//! timing from [`crate::bodies::sd_waits`]; nothing here invents a shape.

use std::time::{Duration, Instant};

use crate::bodies;
use crate::layout::Tree;
use crate::produce_upload::{self as upload, DrawnJob, PollState};
use crate::server_leg;
use crate::services::{self, Kind, Server};
use crate::settings::{self, Conf, Paths};

/// The step name on this draw's rows in `requests.tsv`.
pub const STEP: &str = "thumbnail";

/// How often a job is asked about — §F5.6's "polled every second", taken from the same table the rest of
/// the sd.cpp waits come from so there is one answer to the question.
pub fn poll_interval() -> Duration {
    bodies::sd_waits(Kind::PollImage)
        .and_then(|wait| wait.interval)
        .unwrap_or(Duration::from_secs(1))
}

/// The settings folder, read from this process's environment for EVERY request so an address edited while
/// a draw is going is honoured by its next poll (§02-services#1).
fn current_paths() -> Option<Paths> {
    settings::from_environment()
}

/// The drawing server as it stands right now: settings box, then `SD_SERVER`, then loopback.
fn endpoint() -> Result<(Conf, services::Endpoint), String> {
    let conf = match current_paths().as_ref().map(settings::read) {
        Some(Ok(conf)) => conf,
        Some(Err(reason)) => return Err(format!("settings could not be read: {reason}")),
        None => Conf::default(),
    };
    let endpoint = services::image_endpoint(&conf, |key| std::env::var(key).ok());
    Ok((conf, endpoint))
}

/// One draw request, held to the drawing model's slot count and timed with the wait it spent waiting for one.
///
/// §4 lists the image model among the models that get a slot count, and sd.cpp is the one server this app
/// asks repeatedly (a job every second while a picture draws), so an ungated draw is exactly the queue-in-the-
/// server case §4 exists to prevent. The guard lives until `call_dialed` returns: the hold covers the call,
/// not the caller's thinking. The measured wait goes into the row's `wait_s` (§10), which is how a slot count
/// gets chosen from the log rather than guessed.
#[allow(clippy::too_many_arguments)]
fn dialed(
    tree: &Tree,
    base: &crate::requests::Request,
    endpoint: &services::Endpoint,
    kind: Kind,
    job: Option<&str>,
    body: Option<&str>,
    conf: &Conf,
    cancelled: &dyn Fn() -> bool,
) -> (crate::requests::Request, Result<server_leg::Sent, String>) {
    let step = base.step.as_str();
    let held = crate::slot_gate::hold(
        &endpoint.url,
        "sd.cpp",
        services::image_slots(conf),
        step,
        cancelled,
    );
    let (slot, waited) = match held {
        Ok(got) => got,
        // ⏹ landed while every drawing slot was busy. Nothing went out; the caller reports its own failure
        // wording around whatever `Err` comes back here.
        Err(crate::slot_gate::Wait::Cancelled) => {
            return (
                base.clone(),
                Err(format!("{step}: cancelled while waiting for a drawing slot")),
            )
        }
    };
    let out = server_leg::call_dialed(
        tree,
        base,
        &server_leg::Dial {
            server: Server::Image,
            endpoint,
            kind,
            job,
            body,
            filename: None,
            cancelled,
            // An image job has no silence bound of its own (§1 bounds it by kind); §4's watch is a chat rule.
            liveness: None,
            waited_ms: (waited * 1000.0).round() as i64,
            delta: None,
            budget: None,
            attempt: 1,
        },
    );
    drop(slot);
    out
}

/// Queue one draw: `POST /sdcpp/v1/img_gen` with the body `upload::request_body` built. `Err` names the
/// server and the reason when nothing answered (§Failure is specific); `Ok` carries the job id the two job
/// kinds act on. The address comes from [`endpoint`] — §1's precedence, settings box then `SD_SERVER` then the
/// loopback — re-read here, so a settings edit lands on the next request.
pub fn submit(tree: &Tree, body: &serde_json::Value) -> Result<DrawnJob, String> {
    let (conf, endpoint) = endpoint()?;
    submit_with(tree, &endpoint, body, &conf)
}

/// The submit itself: the slot hold, the row, the POST, the job id out of the answer.
fn submit_with(
    tree: &Tree,
    endpoint: &services::Endpoint,
    body: &serde_json::Value,
    conf: &Conf,
) -> Result<DrawnJob, String> {
    // sd.cpp has no model row of its own in the settings (§7 gives it a server and a key only), so the row
    // names the SERVER it went to rather than leaving the column blank.
    let model = "sd.cpp".to_string();
    let sent_body = body.to_string();
    let base = server_leg::base_row(Server::Image, &model, "img_gen", "", STEP, "draw");
    let (_row, reply) = dialed(
        tree,
        &base,
        endpoint,
        Kind::SubmitImage,
        None,
        Some(&sent_body),
        conf,
        &|| false,
    );
    let sent = reply.map_err(|why| format!("the drawing server at {}: {why}", endpoint.url))?;
    if !sent.ok() {
        return Err(format!(
            "the drawing server at {} answered {}",
            endpoint.url, sent.status
        ));
    }
    let parsed: serde_json::Value = serde_json::from_str(&sent.body)
        .map_err(|_| "the drawing server's answer is not JSON".to_string())?;
    let id = job_id(&parsed).ok_or("the drawing server gave no job id")?;
    Ok(DrawnJob { id })
}

/// One ask: `GET /sdcpp/v1/jobs/{id}`. The status word and how many jobs sit ahead are what §F5.6's log
/// line prints, and `done` is what stops the loop.
pub fn poll(tree: &Tree, job: &DrawnJob) -> Result<PollState, String> {
    let (conf, endpoint) = endpoint()?;
    let base = server_leg::base_row(Server::Image, "sd.cpp", "job", "", STEP, "poll");
    let (_row, reply) = dialed(
        tree,
        &base,
        &endpoint,
        Kind::PollImage,
        Some(&job.id),
        None,
        &conf,
        &|| false,
    );
    let sent = reply.map_err(|why| format!("the drawing server at {}: {why}", endpoint.url))?;
    if !sent.ok() {
        return Err(format!(
            "the drawing server at {} answered {}",
            endpoint.url, sent.status
        ));
    }
    let parsed: serde_json::Value = serde_json::from_str(&sent.body)
        .map_err(|_| "the drawing server's answer is not JSON".to_string())?;
    Ok(state_of(&parsed))
}

/// Ask once per second until the job reports done, logging each answer's line (§F5.6's cadence). Returns
/// the last state seen, whose `png` is the drawn picture; `Err` when a poll failed or the run was stopped
/// between two asks.
///
/// A stop cancels the job before returning: §1 puts the sd.cpp submit and poll on the run's cancel context
/// "so ⏹ aborts them", and the poll leg is where the stop is first noticed while a job id is still known.
/// Without this the app stops watching and sd.cpp draws a picture nobody asked for any more.
pub fn wait_until_done<FLog>(
    tree: &Tree,
    job: &DrawnJob,
    cancelled: &dyn Fn() -> bool,
    mut log: FLog,
) -> Result<PollState, String>
where
    FLog: FnMut(&str),
{
    let gap = poll_interval();
    loop {
        if cancelled() {
            // Best-effort: the stop wins whatever the cancel answers, and its reason is said rather than
            // swallowed, because "stopped" with a job still running on the server is worth one log line.
            if let Err(why) = cancel(tree, job) {
                log(&why);
            }
            return Err("stopped before the picture finished".to_string());
        }
        let asked = Instant::now();
        let state = poll(tree, job)?;
        log(&upload::poll_line(&state.status, state.ahead));
        if state.done {
            return Ok(state);
        }
        // Sleep out the rest of the second rather than busy-polling: the spec's cadence is one ask per
        // second, and a faster loop only makes the queue look shorter than it is. Measured from the ASK,
        // not from after the reply, or a slow answer would be paid for twice.
        let elapsed = asked.elapsed();
        if gap > elapsed {
            std::thread::sleep(gap - elapsed);
        }
    }
}

/// Give up on a job: `POST /sdcpp/v1/jobs/{id}/cancel`. Its row is logged like every other draw row.
///
/// The cancel is sent on an always-false cancel check on purpose: it exists BECAUSE the run stopped, so
/// gating it on the same flag would refuse the one request whose job is to serve that stop.
pub fn cancel(tree: &Tree, job: &DrawnJob) -> Result<(), String> {
    let (conf, endpoint) = endpoint()?;
    let base = server_leg::base_row(Server::Image, "sd.cpp", "job", "", STEP, "cancel");
    let (_row, reply) = dialed(
        tree,
        &base,
        &endpoint,
        Kind::CancelImage,
        Some(&job.id),
        None,
        &conf,
        &|| false,
    );
    let sent = reply.map_err(|why| format!("the drawing server at {}: {why}", endpoint.url))?;
    if !sent.ok() {
        return Err(format!(
            "the drawing server at {} answered {} to cancelling the draw",
            endpoint.url, sent.status
        ));
    }
    Ok(())
}

/// The job id out of one answer. sd.cpp answers with `{"id": …}` for a fresh job and echoes it on later
/// asks; `job_id`/`job` are read too because a proxy in front of it has been seen to rename the field.
pub fn job_id(parsed: &serde_json::Value) -> Option<String> {
    ["id", "job_id", "job"]
        .iter()
        .find_map(|key| parsed.get(*key).and_then(|v| v.as_str()))
        .map(|id| id.trim().to_string())
        .filter(|id| !id.is_empty())
}

/// One job answer as the walk needs it: the status word, the queue position, whether the picture is ready,
/// and the picture itself. `done` covers the words a healthy server uses for "finished" (`done`, `completed`,
/// `success`) and counts a job that already handed back its image as done too.
///
/// The bytes ride in the REPLY, not on their own endpoint: the prototype's working draw reads them off the job
/// answer as `result.images[0].b64_json` (`gui/sdcpp.go:137-150`, `sdGenerate` at :222-230), and
/// `spec/02-services.md` §1 lists only the four sd.cpp paths — capabilities, `POST /sdcpp/v1/img_gen`,
/// `GET /sdcpp/v1/jobs/{id}`, `POST /sdcpp/v1/jobs/{id}/cancel` — so there is no picture URL to fetch.
pub fn state_of(parsed: &serde_json::Value) -> PollState {
    let status = ["status", "state", "phase"]
        .iter()
        .find_map(|key| parsed.get(*key).and_then(|v| v.as_str()))
        .map(|word| word.to_string())
        .unwrap_or_else(|| "queued".to_string());
    let ahead = ["queue_position", "ahead", "waiting"]
        .iter()
        .find_map(|key| parsed.get(*key).and_then(|v| v.as_u64()))
        .map(|n| n as usize)
        .unwrap_or(0);
    let lowered = status.to_ascii_lowercase();
    let picture = picture_of(parsed);
    let done = matches!(
        lowered.as_str(),
        "done" | "completed" | "complete" | "success" | "finished" | "failed" | "error" | "canceled" | "cancelled"
    ) || parsed.get("images").and_then(|v| v.as_array()).is_some_and(|list| !list.is_empty())
        || !picture.is_empty();
    PollState { status, ahead, done, png: picture }
}

/// The drawn picture inside one job answer, decoded: `result.images[0].b64_json`, with the bare
/// `images[0].b64_json` a proxy in front of the server has been seen to answer with. Base64 is read
/// leniently and every non-alphabet byte skipped, so a server that wraps its blob at 76 columns decodes the
/// same as one that does not; a payload that decodes to nothing reads as "no picture", which the walk says.
pub fn picture_of(parsed: &serde_json::Value) -> Vec<u8> {
    let found = ["b64_json", "b64", "base64", "image", "data"]
        .iter()
        .find_map(|key| parsed.get("result").and_then(|r| first_image(r)).and_then(|img| img.get(*key)))
        .or_else(|| {
            ["b64_json", "b64", "base64", "image", "data"]
                .iter()
                .find_map(|key| parsed.get("images").and_then(|l| l.as_array()?.first()).and_then(|img| img.get(*key)))
        });
    let encoded = match found {
        Some(value) if value.is_string() => value.as_str().unwrap_or_default().to_string(),
        // A server that answers the bytes as a plain string under `result.image` rather than an object.
        Some(_) => String::new(),
        None => return Vec::new(),
    };
    decode_base64(&encoded)
}

/// The first entry of a job answer's image list, whether the server spelled it `result.images` or `images`.
fn first_image(parsed: &serde_json::Value) -> Option<&serde_json::Value> {
    parsed.get("images").and_then(|list| list.as_array()?.first())
}

/// Standard-alphabet base64, the inverse of [`crate::checks::base64`]: padding and any byte outside the
/// alphabet (a newline from a server that wraps its blob) are ignored rather than refused, because the only
/// reader here is a picture the server just drew.
fn decode_base64(text: &str) -> Vec<u8> {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut bits: u32 = 0;
    let mut held = 0u32;
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    for byte in text.as_bytes() {
        let Some(value) = ALPHABET.iter().position(|letter| letter == byte) else {
            continue; // padding, line breaks, whitespace: nothing here is a byte of the picture
        };
        bits = (bits << 6) | value as u32;
        held += 6;
        if held >= 8 {
            held -= 8;
            out.push((bits >> held) as u8);
        }
    }
    out
}
