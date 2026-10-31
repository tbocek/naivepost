//! F5.6 S6: the drawing server over HTTP. sd.cpp is asked about once per second (§F5.6) and the address is re-read per request (§02-services#1), so a settings edit lands on the next poll.
//! [`submit`] queues `POST /sdcpp/v1/img_gen`, [`poll`] asks `GET /sdcpp/v1/jobs/{id}`, [`wait_until_done`] drives the cadence logging through [`crate::produce_upload::poll_line`], [`cancel`] makes ⏹ an abort (`POST /sdcpp/v1/jobs/{id}/cancel`).

use std::time::{Duration, Instant};

use crate::bodies;
use crate::layout::Tree;
use crate::produce_upload::{self as upload, DrawnJob, PollState};
use crate::server_leg;
use crate::services::{self, Kind, Server};
use crate::settings::{self, Conf};

pub const STEP: &str = "thumbnail";

pub fn poll_interval() -> Duration {
    bodies::sd_waits(Kind::PollImage)
        .and_then(|wait| wait.interval)
        .unwrap_or(Duration::from_secs(1))
}

fn endpoint() -> Result<services::Endpoint, String> {
    let conf = match settings::from_environment().as_ref().map(settings::read) {
        Some(Ok(conf)) => conf,
        Some(Err(reason)) => return Err(format!("settings could not be read: {reason}")),
        None => Conf::default(),
    };
    Ok(services::image_endpoint(&conf, |key| {
        std::env::var(key).ok()
    }))
}

/// One draw request, held to its server's one turn (§02 §1) and timed; sd.cpp is asked every second, so ungating would queue in the server (§4). The guard lasts until `call_dialed` returns; the wait goes into `wait_s` (§10).
#[allow(clippy::too_many_arguments)]
fn dialed(
    tree: &Tree,
    base: &crate::requests::Request,
    endpoint: &services::Endpoint,
    kind: Kind,
    job: Option<&str>,
    body: Option<&str>,
    cancelled: &dyn Fn() -> bool,
) -> (crate::requests::Request, Result<server_leg::Sent, String>) {
    let step = base.step.as_str();
    let held = crate::slot_gate::hold(&endpoint.url, step, cancelled);
    let (slot, waited) = match held {
        Ok(got) => got,
        // ⏹ landed while every slot was busy; nothing went out and the caller words its own failure.
        Err(crate::slot_gate::Wait::Cancelled) => {
            return (
                base.clone(),
                Err(format!(
                    "{step}: cancelled while waiting for a drawing slot"
                )),
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
            body: body.map(str::as_bytes),
            filename: None,
            cancelled,
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

/// Queue one draw: `POST /sdcpp/v1/img_gen`. `Err` names the server and reason when nothing answered; `Ok` carries the job id. Address from [`endpoint`], re-read each time.
pub fn submit(tree: &Tree, body: &serde_json::Value) -> Result<DrawnJob, String> {
    let endpoint = endpoint()?;
    submit_with(tree, &endpoint, body)
}

fn submit_with(
    tree: &Tree,
    endpoint: &services::Endpoint,
    body: &serde_json::Value,
) -> Result<DrawnJob, String> {
    // sd.cpp has no model row in settings (§7), so the row names the SERVER.
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

pub fn poll(tree: &Tree, job: &DrawnJob) -> Result<PollState, String> {
    let endpoint = endpoint()?;
    let base = server_leg::base_row(Server::Image, "sd.cpp", "job", "", STEP, "poll");
    let (_row, reply) = dialed(
        tree,
        &base,
        &endpoint,
        Kind::PollImage,
        Some(&job.id),
        None,
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

/// Ask once per second until the job reports done; returns the last state (`png` is the picture) or `Err` when a poll failed or the run was stopped.
/// A stop cancels the job first (§1 puts submit and poll on the run's cancel context), else sd.cpp keeps drawing a picture nobody wants.
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
            // Best-effort: the stop wins whatever the cancel answers; the reason is logged.
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
        // Sleep out the rest of the second, measured from the ASK, not the reply.
        let elapsed = asked.elapsed();
        if gap > elapsed {
            std::thread::sleep(gap - elapsed);
        }
    }
}

/// Give up on a job: `POST /sdcpp/v1/jobs/{id}/cancel`. Sent with an always-false cancel check on purpose: it exists BECAUSE the run stopped.
pub fn cancel(tree: &Tree, job: &DrawnJob) -> Result<(), String> {
    let endpoint = endpoint()?;
    let base = server_leg::base_row(Server::Image, "sd.cpp", "job", "", STEP, "cancel");
    let (_row, reply) = dialed(
        tree,
        &base,
        &endpoint,
        Kind::CancelImage,
        Some(&job.id),
        None,
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

/// The job id out of one answer: `{"id": …}`, also `job_id`/`job` since a proxy has renamed it.
pub fn job_id(parsed: &serde_json::Value) -> Option<String> {
    ["id", "job_id", "job"]
        .iter()
        .find_map(|key| parsed.get(*key).and_then(|v| v.as_str()))
        .map(|id| id.trim().to_string())
        .filter(|id| !id.is_empty())
}

/// One job answer as the walk needs it: status word, queue position, readiness, picture. `done` covers `done`, `completed`, `success`, or a job that already returned its image.
/// The bytes ride in the REPLY as `result.images[0].b64_json`; `spec/02-services.md` §1 lists no picture URL.
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
        "done"
            | "completed"
            | "complete"
            | "success"
            | "finished"
            | "failed"
            | "error"
            | "canceled"
            | "cancelled"
    ) || parsed
        .get("images")
        .and_then(|v| v.as_array())
        .is_some_and(|list| !list.is_empty())
        || !picture.is_empty();
    PollState {
        status,
        ahead,
        done,
        png: picture,
    }
}

/// The drawn picture in a job answer: `result.images[0].b64_json` or bare `images[0].b64_json` (proxy). Base64 is read leniently, so wrapped blobs decode; an empty payload is "no picture".
pub fn picture_of(parsed: &serde_json::Value) -> Vec<u8> {
    let found = ["b64_json", "b64", "base64", "image", "data"]
        .iter()
        .find_map(|key| {
            parsed
                .get("result")
                .and_then(|r| first_image(r))
                .and_then(|img| img.get(*key))
        })
        .or_else(|| {
            ["b64_json", "b64", "base64", "image", "data"]
                .iter()
                .find_map(|key| {
                    parsed
                        .get("images")
                        .and_then(|l| l.as_array()?.first())
                        .and_then(|img| img.get(*key))
                })
        });
    let encoded = match found {
        Some(value) if value.is_string() => value.as_str().unwrap_or_default().to_string(),
        Some(_) => String::new(),
        None => return Vec::new(),
    };
    decode_base64(&encoded)
}

fn first_image(parsed: &serde_json::Value) -> Option<&serde_json::Value> {
    parsed
        .get("images")
        .and_then(|list| list.as_array()?.first())
}

/// Standard-alphabet base64, inverse of [`crate::checks::base64`]: padding and non-alphabet bytes are ignored.
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
