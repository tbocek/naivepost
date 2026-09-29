//! F4.4's wire: the three audio.cpp calls one spoken line needs, over real HTTP.
//!
//! [`crate::narrate_tts`] owns every S1–S6 rule and takes its two network steps as closures; the
//! page supplied those closures with a literal path and a test thread-local, so nothing was dialled
//! and a scripted reply stood in for the work. This module is the program's side of that seam: it
//! reads the Settings file on **every** call (§02-services#1 — each URL, key and model id is
//! re-read per request, so an address typed while a run is going lands on the next line rather
//! than the next launch), dials through [`crate::server_leg`] so each trip leaves its timed
//! `requests.tsv` row (§09 §10), and hands the answers to `narrate_tts::speak_line`, which is
//! still what decides whether a take landed.
//!
//! The three legs, in the flowchart's order:
//! - [`health`] — S2: `GET /health`, then `GET /v1/models`. Both, because health alone green-lights
//!   a server serving only step-1 models, which cannot narrate; the verdict itself stays in
//!   [`checks::tts_endpoint_verdict`], shared with the Settings row so the two cannot disagree.
//! - [`upload_reference`] — S3: `POST /v1/ui/upload`, raw bytes with `X-Audiocpp-Filename`, read
//!   from `narrate/voice_ref.wav`. Called for EVERY line and never remembered across calls
//!   ([`bodies::VOICE_REF_REUPLOADED_EVERY_LINE`]): a server path dies with a restart.
//! - [`post_speech`] — S4/S5: `POST /v1/audio/speech`, answering `(status, bytes)`.
//!
//! A down server answers `Err` naming that server and the address tried (`server_leg`'s own words),
//! never `Ok` with nothing in it.

use crate::bodies::ServerPath;
use crate::layout::Tree;
use crate::narrate_tts::{self, Outcome};
use crate::requests::Request as TimedRow;
use crate::server_leg::{self, Dial};
use crate::services::{self, Endpoint, Kind, Server};
use crate::settings::{self, Conf};

/// The step column every row this module lands carries. `speak` is what a person reads beside the
/// job name in `requests.tsv`; the job stays `narrate` because that is the run they started.
pub const STEP: &str = "speak";

/// The job column: F4.4 speaks lines of the narration record, so its rows belong to `narrate`.
pub const JOB: &str = "narrate";

/// The settings as they are ON DISK right now, re-read per call. An unreadable file resolves as an
/// empty one rather than failing the line: a settings file caught mid-write must not stop a speak
/// when the loopback default is still right (the rule `settings_probe::current_conf` follows too).
fn current_conf() -> Conf {
    settings::from_environment()
        .and_then(|paths| settings::read(&paths).ok())
        .unwrap_or_default()
}

/// audio.cpp's endpoint for this call: the Settings box, then `NAIVEPOST_TTS_URL`, then
/// `AUDIOCPP_SERVER`, then the loopback — [`services::audio_endpoint`]'s precedence, unchanged.
/// This module adds no override of its own, which is what makes the address the test seam: a fake
/// server replaces the real one by being the address, and no program code knows the difference.
fn endpoint(conf: &Conf) -> Endpoint {
    services::audio_endpoint(conf, |name| std::env::var(name).ok())
}

/// Ask audio.cpp one thing, timed and recorded against this session's `requests.tsv`.
///
/// The cancel closure is the live switch ([`crate::cancel_leg::cancel_check_now`]), not a stub:
/// §02-services#1's last paragraph puts every request on the run context, so a ⏹ pressed while a
/// synthesis is in flight ends the wait instead of leaving the app hanging on a server it already
/// gave up on.
fn ask(
    tree: &Tree,
    base: &TimedRow,
    endpoint: &Endpoint,
    kind: Kind,
    filename: Option<&str>,
    body: Option<&[u8]>,
) -> Result<server_leg::Sent, String> {
    let cancelled = crate::cancel_leg::cancel_check_now();
    let text = body.map(|raw| String::from_utf8_lossy(raw).into_owned());
    server_leg::call_dialed(
        tree,
        base,
        &Dial {
            server: Server::Audio,
            endpoint,
            kind,
            job: None,
            body: text.as_deref(),
            filename,
            cancelled: &cancelled,
        },
    )
    .1
}

/// S2: is audio.cpp up, and what does it serve? Both questions, in that order, asked here rather
/// than read off cached state — speaking is not a repaint, so the answer has to be current.
///
/// A `/health` that does not answer returns `Err` carrying `server_leg`'s own sentence (which
/// names the address tried); the caller turns that into the refusal rather than treating it as
/// "unhealthy but worth trying".
pub fn health(tree: &Tree) -> Result<Vec<services::AudioModel>, String> {
    let conf = current_conf();
    let endpoint = endpoint(&conf);
    ask(tree, &row("health", ""), &endpoint, Kind::Health, None, None)?;
    let models = ask(
        tree,
        &row("models", ""),
        &endpoint,
        Kind::ListModels,
        None,
        None,
    )?;
    Ok(services::parse_models(&models.body))
}

/// S3: put `narrate/voice_ref.wav` on the server and bring back the path the speech call will name.
///
/// The upload carries the file's own bytes, not a local path: audio.cpp only sees inside its own
/// container. A 401/403/405 is answered by [`services::upload_refused`] — the fix ("start it
/// with --ui-management"), not the status — because nobody reads a 403 as a missing start-up flag.
pub fn upload_reference(tree: &Tree) -> Result<ServerPath, String> {
    let conf = current_conf();
    let endpoint = endpoint(&conf);
    let reference = tree.voice_ref_wav();
    let bytes =
        std::fs::read(&reference).map_err(|why| format!("{}: {why}", reference.display()))?;
    let name = reference
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "voice_ref.wav".to_string());
    let reply = ask(
        tree,
        &row("upload", "voice_ref.wav"),
        &endpoint,
        Kind::Upload,
        Some(&name),
        Some(&bytes),
    )?;
    if let Some(refusal) = services::upload_refused(reply.status, &reply.body) {
        return Err(refusal);
    }
    if reply.status != 200 {
        return Err(format!(
            "uploading {name} to {} answered {}: {}",
            endpoint.url,
            reply.status,
            trim(&reply.body)
        ));
    }
    let parsed: serde_json::Value = serde_json::from_str(&reply.body).map_err(|_| {
        format!(
            "audio.cpp took {name} but answered something that is not JSON: {}",
            trim(&reply.body)
        )
    })?;
    let path = services::upload_path(&parsed)?;
    ServerPath::from_upload(&path)
        .ok_or_else(|| format!("the upload named {path:?}, which is not a server path"))
}

/// S4/S5: one `POST /v1/audio/speech`, answering the status and the reply BYTES.
///
/// Bytes rather than a string because S5 counts them and a wav is not UTF-8: read through
/// `Sent.body` a synthesized voice sample would arrive lossily converted, and the floor
/// ([`narrate_tts::MIN_WAV_BYTES`]) would then be measured against a different length than the
/// file the user gets. The session folder comes from [`hold_tree`], which [`speak`] sets first.
pub fn post_speech(body: &serde_json::Value) -> Result<(u16, Vec<u8>), String> {
    let tree = held_tree().expect("a speak sets its session tree before its speech leg runs");
    let conf = current_conf();
    speech_call(&tree, &endpoint(&conf), body)
}

/// The speech POST itself, tree explicit so [`speak`] can bind its own session into the closure
/// `narrate_tts::speak_line` takes. Same endpoint rules, same bearer header and same cancel switch
/// as every other leg; the dial is written by hand rather than through `server_leg::send` because a
/// wav is binary and `Sent.body` is a `String` — reading a voice sample lossily would put S5's
/// 1000-byte floor against a different length than the file the user gets.
pub fn speech_call(
    tree: &Tree,
    endpoint: &Endpoint,
    body: &serde_json::Value,
) -> Result<(u16, Vec<u8>), String> {
    if crate::cancel_leg::cancelled() {
        return Err("speech: cancelled before sending".to_string());
    }
    let url = format!(
        "{}{}",
        endpoint.url.trim_end_matches('/'),
        Kind::Speech.path(None)
    );
    let builder = ureq::post(&url)
        .config()
        .timeout_connect(Some(server_leg::CONNECT_TIMEOUT))
        // `Kind::Speech` has no ceiling of its own: a long line is long work (§1). Only the connect
        // is bounded, so a machine that is not there fails fast.
        .timeout_global(services::timeout_for(Kind::Speech))
        .http_status_as_error(false)
        .build();
    let builder = match services::authorization(&endpoint.key) {
        Some(bearer) => builder.header("Authorization", bearer),
        None => builder,
    };
    let payload = body.to_string();
    // The model id goes on the row: it is the one column a reader needs to know WHICH voice was
    // dialled, and the request body is where it lives.
    let model = body
        .get("model")
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .to_string();
    let base = row("speech", &model);
    let attempt = crate::request_timing::next_attempt(None);
    let started_ms = epoch_ms();
    let mut timer = crate::request_timing::Timer::sent(started_ms);
    // One writer for the row, whichever way the dial ended: §09 §10 wants the request on file even
    // when it died, and written before the caller looks at the bytes.
    let landed = |timer: &crate::request_timing::Timer,
                 outcome: crate::requests::Outcome,
                 got: i64| {
        let mut row = base.clone();
        row.sent_bytes = Some(payload.len() as i64);
        row.received_bytes = Some(got);
        let _ = crate::requests::record(
            tree,
            &crate::request_timing::finish_row(&row, timer, outcome, attempt),
        );
    };
    let response = match builder.send(payload.clone()) {
        Ok(response) => response,
        Err(why) => {
            timer.finished(epoch_ms());
            landed(
                &timer,
                crate::requests::Outcome::Error(why.to_string()),
                0,
            );
            return Err(format!("audio.cpp ({url}) is not answering: {why}"));
        }
    };
    let status = response.status().as_u16();
    let mut reader = response.into_body().into_reader();
    let mut bytes = Vec::new();
    match std::io::Read::read_to_end(&mut reader, &mut bytes) {
        Ok(_) => timer.first_byte(started_ms),
        Err(why) => {
            timer.finished(epoch_ms());
            landed(
                &timer,
                crate::requests::Outcome::Error(format!("mid-reply: {why}")),
                bytes.len() as i64,
            );
            return Err(format!("audio.cpp died mid-reply at {url}: {why}"));
        }
    }
    timer.finished(epoch_ms());
    let outcome = if (200..300).contains(&status) {
        crate::requests::Outcome::Ok
    } else {
        crate::requests::Outcome::Error(format!("{status}"))
    };
    landed(&timer, outcome, bytes.len() as i64);
    if crate::cancel_leg::cancelled() {
        return Err("speech: cancelled mid-reply".to_string());
    }
    Ok((status, bytes))
}

/// The whole of F4.4 for one line, over the real wire: S2 asked here, S3 uploaded here, S4 dialled
/// here, and every rule — including which step refused and which therefore never ran — decided by
/// [`narrate_tts::speak_line`].
///
/// `project_language` is P.policy.ttsLanguage and reaches [`narrate_tts::language`] unchanged, so
/// the project's language is what the model is told to speak; only an empty one falls back to "en".
pub fn speak(
    tree: &Tree,
    text: &str,
    emotion: &str,
    seed: u32,
    key: &str,
    project_language: &str,
) -> Outcome {
    hold_tree(tree);
    // S1 first, before ANY dial: with no reference there is nothing to clone, and asking a live
    // server about its health would put two requests on the wire for a line that cannot be spoken.
    if let Some(problem) = narrate_tts::reference_problem(tree) {
        return Outcome::Refused(problem);
    }
    let conf = current_conf();
    let model = services::tts_model(&conf).to_string();
    let dial = endpoint(&conf);
    let models = match health(tree) {
        Ok(models) => models,
        Err(why) => return Outcome::Refused(why),
    };
    narrate_tts::speak_line(
        tree,
        text,
        emotion,
        seed,
        key,
        project_language,
        true,
        &models,
        &model,
        upload_reference,
        // The tree and endpoint are bound here so the closure keeps `post_speech`'s one-argument
        // shape while still recording into this session and dialling this address.
        move |body| speech_call(tree, &dial, body),
    )
}

thread_local! {
    /// The session the current speak records into, read by [`post_speech`]: the closure signature
    /// `narrate_tts::speak_line` takes for its speech leg carries the body alone, so the project
    /// folder travels here rather than as a ninth argument.
    static SPEAK_TREE: std::cell::RefCell<Option<Tree>> = const { std::cell::RefCell::new(None) };
}

/// Remember the session for [`post_speech`]. Set by [`speak`] before it starts any leg.
pub fn hold_tree(tree: &Tree) {
    SPEAK_TREE.with(|cell| *cell.borrow_mut() = Some(tree.clone()));
}

/// The session [`post_speech`] will write its row into.
pub fn held_tree() -> Option<Tree> {
    SPEAK_TREE.with(|cell| cell.borrow().clone())
}

/// A row for one of this module's calls: service audio, step [`STEP`], job [`JOB`]. The address is
/// already in the row's own `kind`/url-free identity — `server_leg` records the URL it dialled in
/// the timing columns, so only the model id travels here.
fn row(kind: &str, model: &str) -> TimedRow {
    server_leg::base_row(Server::Audio, model, kind, "", STEP, JOB)
}

/// A reply trimmed and shortened enough for a log line.
fn trim(body: &str) -> String {
    let one = body.trim();
    if one.chars().count() > 200 {
        let cut: String = one.chars().take(200).collect();
        format!("{cut}…")
    } else {
        one.to_string()
    }
}

/// Milliseconds since the epoch, for the row's `started` column. `server_leg` keeps its own copy
/// private, and this leg times its own dial.
fn epoch_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_millis() as i64)
        .unwrap_or_default()
}
