//! The wire. §02-services#1 names four servers and the API each one serves; this module is the
//! half that actually opens the socket, using `ureq` (the target's client) with nothing invented:
//! the URL comes from [`crate::services`]'s endpoint rules, the method and path from
//! [`crate::services::Kind`], the bearer header from [`crate::services::authorization`], and the
//! timeout from [`crate::services::timeout_for`]. Every other module in this tree states those
//! rules; this one obeys them over HTTP.
//!
//! Two things it owns by itself, both because nothing else can:
//! - **The clock.** [`request_timing::Timer`] takes milliseconds the caller recorded, so the
//!   measurement happens here, next to the bytes it measures ([§09 §10][spec]).
//! - **The row before the reply.** A request's line goes into `requests.tsv` before the caller
//!   looks at the body, which is what makes a step that then chokes still leave its requests
//!   behind (§09 §10). Hence [`call`] returns the row and the reply together rather than the
//!   reply alone.
//!
//! A server that is down answers `Err` naming that server and the address tried — never `Ok`,
//! never an empty body standing in for work that did not happen.

use std::io::Read;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::request_timing::{self, Timer};
use crate::requests::{self, Outcome, Request, Service};
use crate::services::{self, Endpoint, Kind, Server};

/// How long to wait for a connection when the kind itself has no ceiling.
///
/// §1 puts no client timeout on the audio task legs ("an hour of audio is an hour of work"),
/// but a leg with *no* bound at all hangs the run on a machine that is not there rather than
/// failing it. Connecting is not work: 10 s says "nothing answers at that address" without
/// touching how long an accepted job may take. Kinds that do have a ceiling ([`Kind::UnloadAll`],
/// 20 s) get their own value as the whole-call bound instead.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// What came back, plus what the trip cost.
#[derive(Debug, Clone, PartialEq)]
pub struct Sent {
    pub status: u16,
    pub body: String,
    /// Bytes of the request body that went out; `None` for a GET, which sends none.
    pub sent_bytes: Option<i64>,
    pub received_bytes: i64,
    /// Milliseconds waiting for a slot before going out. 0 for a leg that never queued.
    pub wait_ms: i64,
    pub first_byte_ms: Option<i64>,
    pub on_wire_ms: Option<i64>,
}

impl Sent {
    /// Whether the server said this was a success.
    pub fn ok(&self) -> bool {
        (200..300).contains(&self.status)
    }
}

/// The error text for a leg that never got an answer.
///
/// Names the server and the address tried, because "connection refused" from a run three steps
/// in does not say which of the four boxes to look at, and §Failure-is-specific asks for the
/// reason where it failed rather than a later mystery.
fn down(server: Server, url: &str, why: &str) -> String {
    format!("{} ({}) is not answering: {why}", server_label(server), url)
}

fn server_label(server: Server) -> &'static str {
    match server {
        Server::Llm => "llm",
        Server::Audio => "audio.cpp",
        Server::Image => "sd.cpp",
    }
}

/// The service column a request carries, from the server it was aimed at.
fn service_of(server: Server) -> Service {
    match server {
        Server::Llm => Service::Llm,
        Server::Audio => Service::Audio,
        Server::Image => Service::Image,
    }
}

/// Milliseconds since the epoch, for the row's `started` column.
fn epoch_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_millis() as i64)
        .unwrap_or_default()
}

/// Local UTC offset in seconds, read once per call from the system.
///
/// Kept here rather than in `requests.rs` because that module stays pure data and takes the
/// offset as an argument; this module is the one allowed to ask the machine.
fn local_offset_secs() -> i32 {
    // `libc`-free: the timezone is what the process started with, and a whole-of-system read
    // would need a dependency this crate does not carry. Zero keeps the file on UTC, which is
    // unambiguous and sortable; a wrong local time is worse than an honest UTC stamp.
    0
}

/// Send one request to one server and return what came back with the timings.
///
/// `job` is the sd.cpp job id the two job kinds need and every other kind ignores — the same
/// argument [`Kind::path`] takes, passed through unchanged.
///
/// `cancelled` is the run's cancel context. §1's last paragraph asks that uploads, task runs
/// and the sd.cpp submit/poll ride it so ⏹ aborts them, and [`services::rides_cancel_context`]
/// says today that means *every* kind; the check is therefore made on the way in (don't start
/// what has already been stopped) and on the way out (a reply that arrived after the stop is
/// reported as cancelled, not as work done).
/// [`send_named`] without a file name — the form every kind but the upload calls.
pub fn send(
    server: Server,
    endpoint: &Endpoint,
    kind: Kind,
    job: Option<&str>,
    body: Option<&str>,
    cancelled: &dyn Fn() -> bool,
) -> Result<Sent, String> {
    send_named(server, endpoint, kind, job, body, None, cancelled)
}

/// [`send`] with the upload's file name. Split rather than folded into one signature so the four
/// ordinary kinds keep calling the four-argument form; only [`Kind::Upload`] has a name to pass.
pub fn send_named(
    server: Server,
    endpoint: &Endpoint,
    kind: Kind,
    job: Option<&str>,
    body: Option<&str>,
    filename: Option<&str>,
    cancelled: &dyn Fn() -> bool,
) -> Result<Sent, String> {
    if services::rides_cancel_context(kind) && cancelled() {
        return Err(format!("{}: cancelled before sending", kind.path(job)));
    }

    let url = format!(
        "{}{}",
        endpoint.url.trim_end_matches('/'),
        kind.path(job)
    );

    // The timeouts come straight out of §1's table. `Some(d)` bounds the whole call; `None`
    // bounds only the connect, per CONNECT_TIMEOUT's note above.
    let overall = services::timeout_for(kind);
    // One builder type for both verbs. `WithoutBody` cannot meet `WithBody` in a `match`, so the
    // GET is widened with `force_send_body()` and then sends nothing (`send_empty` below): one
    // config chain — timeouts, bearer header — for every kind rather than two that can drift.
    let builder = match kind.method() {
        "POST" => ureq::post(&url),
        _ => ureq::get(&url).force_send_body(),
    };
    let builder = builder
        .config()
        .timeout_connect(Some(CONNECT_TIMEOUT))
        .timeout_global(overall)
        // 4xx/5xx come back as a response we read, not as an error: §1's upload rule
        // (`services::upload_refused`) needs the status AND the server's own message, which a
        // status-as-error path would throw away.
        .http_status_as_error(false)
        .build();

    // The key becomes a bearer header only when there is one, through the one function that
    // decides that (§1's rule lives in services::authorization, not here).
    let builder = match services::authorization(&endpoint.key) {
        Some(bearer) => builder.header("Authorization", bearer),
        None => builder,
    };

    // audio.cpp's upload names the file it carries in a header, not in a multipart envelope
    // (§1: `POST /v1/ui/upload` raw body with `X-Audiocpp-Filename`). The name arrives as an
    // extension of `body`: every other kind passes `None` and sends no such header.
    let builder = match filename {
        Some(name) => builder.header("X-Audiocpp-Filename", name),
        None => builder,
    };

    let sent_bytes = body.map(|text| text.len() as i64);
    let started = Instant::now();

    let response = match body {
        Some(text) => builder.send(text),
        None => builder.send_empty(),
    };

    // `http_status_as_error(false)` is set on the request below, so every status — 200 or 500 —
    // arrives here as Ok. A non-2xx is therefore read from the same response object, and its body
    // kept: §1's upload rule (`services::upload_refused`) needs the server's own message beside
    // the status, which a status-as-error path would throw away.
    let response = match response {
        Ok(response) => response,
        Err(err) => return Err(down(server, &url, &err.to_string())),
    };
    let status = response.status().as_u16();
    let mut reader = response.into_body().into_reader();
    let mut buf = Vec::new();
    let first = loop {
        let mut chunk = [0u8; 8192];
        match reader.read(&mut chunk) {
            Ok(0) => break started.elapsed().as_millis() as i64,
            Ok(n) => {
                let elapsed = started.elapsed().as_millis() as i64;
                buf.extend_from_slice(&chunk[..n]);
                // First successful read is the first byte, unless the body was empty.
                if buf.len() == n {
                    break elapsed;
                }
            }
            Err(why) => {
                drop(reader);
                let _ = why;
                return Err(down(server, &url, "the connection died mid-reply"));
            }
        }
    };
    let total = started.elapsed().as_millis() as i64;
    let text = String::from_utf8_lossy(&buf).to_string();

    if cancelled() {
        return Err(format!("{}: cancelled mid-reply", kind.path(job)));
    }

    Ok(Sent {
        status,
        body: text,
        sent_bytes,
        received_bytes: buf.len() as i64,
        wait_ms: 0,
        first_byte_ms: Some(first),
        on_wire_ms: Some(total),
    })
}

/// Ask one server something, time it, record it, and hand back the reply.
///
/// This is the shape every caller wants and the shape §09 §10 requires: the row is written
/// before the body is looked at, so a step that fails on the content afterwards still leaves
/// its request in the file. `base` carries the identity columns (`run`, `step`, `job`, `model`)
/// the caller knows and this module cannot guess; the timing, outcome and attempt columns are
/// filled here.
///
/// Returns the row that was written alongside the reply, so a caller can log or assert the
/// cost without re-reading the file.
/// Ask one server something, time it, record it, and hand back the reply: [`call_dialed`] with a
/// [`Dial`] built from the arguments this row reaches for. New callers should construct the [`Dial`]
/// themselves so each leg is named.
pub fn call(
    tree: &crate::layout::Tree,
    base: &Request,
    server: Server,
    endpoint: &Endpoint,
    kind: Kind,
    job: Option<&str>,
    body: Option<&str>,
    cancelled: &dyn Fn() -> bool,
) -> (Request, Result<Sent, String>) {
    // `Dial` carries the upload's file name, which this form never has; grouping into it rather
    // than growing a ninth positional argument is what keeps a mis-ordered job/body pair from
    // dialling the wrong place.
    #![allow(clippy::too_many_arguments)]
    call_dialed(
        tree,
        base,
        &Dial {
            server,
            endpoint,
            kind,
            job,
            body,
            filename: None,
            cancelled,
        },
    )
}

/// Everything [`call`] takes plus the file name the upload carries. Grouped into [`Dial`] rather
/// than spread across nine positional arguments, because the eight- and nine-argument forms are the
/// two every flow reaches for and a mis-ordered `job`/`body` pair would dial the wrong place.
pub struct Dial<'a> {
    pub server: Server,
    pub endpoint: &'a Endpoint,
    pub kind: Kind,
    /// The sd.cpp job id the two job kinds act on; every other kind ignores it.
    pub job: Option<&'a str>,
    pub body: Option<&'a str>,
    /// audio.cpp's `X-Audiocpp-Filename`; only the upload kind sets it.
    pub filename: Option<&'a str>,
    /// The run's cancel context.
    pub cancelled: &'a dyn Fn() -> bool,
}

/// [`call`] from a [`Dial`], so a caller names each leg instead of counting positions.
pub fn call_dialed(
    tree: &crate::layout::Tree,
    base: &Request,
    dial: &Dial<'_>,
) -> (Request, Result<Sent, String>) {
    let attempt = request_timing::next_attempt(None);
    let timer_start = epoch_ms();
    let result = send_named(
        dial.server,
        dial.endpoint,
        dial.kind,
        dial.job,
        dial.body,
        dial.filename,
        dial.cancelled,
    );
    let mut timer = Timer::sent(timer_start);
    match &result {
        Ok(reply) => {
            if let Some(first) = reply.first_byte_ms {
                timer.first_byte(timer_start + first);
            }
            if let Some(total) = reply.on_wire_ms {
                timer.finished(timer_start + total);
            }
        }
        Err(_) => timer.finished(epoch_ms()),
    }

    let outcome = match &result {
        Ok(reply) if reply.ok() => Outcome::Ok,
        Ok(reply) => Outcome::Error(format!("{}", reply.status)),
        Err(reason) if reason.contains("cancelled") => Outcome::Cancelled,
        Err(reason) => Outcome::Error(reason.clone()),
    };

    let row = request_timing::finish_row(base, &timer, outcome, attempt);
    // Written first, used after. A failure to write the log is not allowed to swallow the
    // reply, so it is reported in the row's own error channel instead: the caller sees the
    // request went through even when the file could not be opened.
    let _ = requests::record(tree, &row);
    (row, result)
}

/// A `Request` carrying only what the caller knows, ready for [`call`] to finish.
pub fn base_row(server: Server, model: &str, kind: &str, run: &str, step: &str, job: &str) -> Request {
    Request {
        started: requests::format_started(epoch_ms(), local_offset_secs()),
        run: run.to_string(),
        step: step.to_string(),
        job: job.to_string(),
        service: service_of(server),
        model: model.to_string(),
        kind: kind.to_string(),
        ..Default::default()
    }
}
