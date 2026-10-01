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

/// Queue one draw: `POST /sdcpp/v1/img_gen` with the body `upload::request_body` built. `Err` names the
/// server and the reason when nothing answered (§Failure is specific); `Ok` carries the job id the two job
/// kinds act on.
pub fn submit(tree: &Tree, body: &serde_json::Value) -> Result<DrawnJob, String> {
    let (conf, endpoint) = endpoint()?;
    // sd.cpp has no model row of its own in the settings (§7 gives it a server and a key only), so the row
    // names the SERVER it went to rather than leaving the column blank.
    let model = "sd.cpp".to_string();
    let _ = conf;
    let sent_body = body.to_string();
    let base = server_leg::base_row(Server::Image, &model, "img_gen", "", STEP, "draw");
    let (_row, reply) = server_leg::call(
        tree,
        &base,
        Server::Image,
        &endpoint,
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

/// One ask: `GET /sdcpp/v1/jobs/{id}`. The status word and how many jobs sit ahead are what §F5.6's log
/// line prints, and `done` is what stops the loop.
pub fn poll(tree: &Tree, job: &DrawnJob) -> Result<PollState, String> {
    let (conf, endpoint) = endpoint()?;
    let _ = conf;
    let base = server_leg::base_row(Server::Image, "sd.cpp", "job", "", STEP, "poll");
    let (_row, reply) = server_leg::call(
        tree,
        &base,
        Server::Image,
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

/// Ask once per second until the job reports done, logging each answer's line (§F5.6's cadence). Returns
/// the last state seen; `Err` when a poll failed or the run was stopped between two asks.
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
    let (_conf, endpoint) = endpoint()?;
    let base = server_leg::base_row(Server::Image, "sd.cpp", "job", "", STEP, "cancel");
    let (_row, reply) = server_leg::call(
        tree,
        &base,
        Server::Image,
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

/// The job id out of one answer. sd.cpp answers with `{"id": …}` for a fresh job and echoes it on later
/// asks; `job_id`/`job` are read too because a proxy in front of it has been seen to rename the field.
pub fn job_id(parsed: &serde_json::Value) -> Option<String> {
    ["id", "job_id", "job"]
        .iter()
        .find_map(|key| parsed.get(*key).and_then(|v| v.as_str()))
        .map(|id| id.trim().to_string())
        .filter(|id| !id.is_empty())
}

/// One job answer as the walk needs it: the status word, the queue position, and whether the picture is
/// ready. `done` covers the words a healthy server uses for "finished" (`done`, `completed`, `success`)
/// and counts a job that already handed back its image as done too.
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
    let done = matches!(
        lowered.as_str(),
        "done" | "completed" | "complete" | "success" | "finished" | "failed" | "error" | "canceled" | "cancelled"
    ) || parsed.get("images").and_then(|v| v.as_array()).is_some_and(|list| !list.is_empty());
    PollState { status, ahead, done }
}
