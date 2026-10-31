//! The wire. §02-services#1 names four servers and the API each one serves.

use std::io::Read;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::request_timing::{self, Timer};
use crate::requests::{self, Outcome, Request, Service};
use crate::services::{self, Endpoint, Kind, Server};

pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, PartialEq)]
pub struct Sent {
    pub status: u16,
    pub body: String,
    /// The response's `content-type` header, lower-cased, empty when the server sent none. §09 §1
    /// routes the answer on it: only a `text/event-stream` reply is read as an event stream,
    /// anything else as one JSON object.
    pub content_type: String,
    pub sent_bytes: Option<i64>,
    pub received_bytes: i64,
    /// Milliseconds waiting its turn before going out. 0 for a leg that never queued.
    pub wait_ms: i64,
    pub first_byte_ms: Option<i64>,
    pub on_wire_ms: Option<i64>,
    /// For a thinking model that STREAMS its reasoning before its answer: how much of the trip was
    /// reasoning.
    pub thinking_ms: Option<i64>,
}

impl Sent {
    pub fn ok(&self) -> bool {
        (200..300).contains(&self.status)
    }
}

/// The error text for a leg that never got an answer.
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

fn service_of(server: Server) -> Service {
    match server {
        Server::Llm => Service::Llm,
        Server::Audio => Service::Audio,
        Server::Image => Service::Image,
    }
}

/// Milliseconds since the epoch, for the row's `started` column.
pub fn epoch_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_millis() as i64)
        .unwrap_or_default()
}

/// The machine's offset from UTC in seconds, shared with the exchange page's stamp so a page and a
/// `requests.tsv` row cannot disagree about what time it was.
pub fn local_offset_secs() -> i32 {
    0
}

pub fn send(
    server: Server,
    endpoint: &Endpoint,
    kind: Kind,
    job: Option<&str>,
    body: Option<&str>,
    cancelled: &dyn Fn() -> bool,
) -> Result<Sent, String> {
    send_named(
        server,
        endpoint,
        kind,
        job,
        body.map(str::as_bytes),
        None,
        cancelled,
    )
}

pub fn send_budget(
    server: Server,
    endpoint: &Endpoint,
    kind: Kind,
    job: Option<&str>,
    body: Option<&str>,
    budget: Option<std::time::Duration>,
    cancelled: &dyn Fn() -> bool,
) -> Result<Sent, String> {
    send_pinged(
        server,
        endpoint,
        kind,
        job,
        body.map(str::as_bytes),
        None,
        cancelled,
        None,
        None,
        budget,
    )
}

pub fn send_named(
    server: Server,
    endpoint: &Endpoint,
    kind: Kind,
    job: Option<&str>,
    body: Option<&[u8]>,
    filename: Option<&str>,
    cancelled: &dyn Fn() -> bool,
) -> Result<Sent, String> {
    send_pinged(
        server, endpoint, kind, job, body, filename, cancelled, None, None, None,
    )
}

/// [`send_named`] with the §4 probe: every byte this read comes off the wire is a heartbeat's
/// evidence, so the reader pings here rather than anywhere the event parser would have to be
/// involved.
fn send_pinged(
    server: Server,
    endpoint: &Endpoint,
    kind: Kind,
    job: Option<&str>,
    body: Option<&[u8]>,
    filename: Option<&str>,
    cancelled: &dyn Fn() -> bool,
    probe: Option<&crate::llm_watch_leg::Probe>,
    delta: Option<&Delta>,
    budget: Option<std::time::Duration>,
) -> Result<Sent, String> {
    // The eighth argument is §4's byte ping, which only a watched leg has.
    #![allow(clippy::too_many_arguments)]
    if services::rides_cancel_context(kind) && cancelled() {
        return Err(format!("{}: cancelled before sending", kind.path(job)));
    }

    let url = format!("{}{}", endpoint.url.trim_end_matches('/'), kind.path(job));

    // The timeouts come straight out of §1's table. A caller with a bound of its own (a Settings
    // row a person is watching) supplies one through [`send_budget`].
    let overall = services::timeout_for(kind).or(budget);
    // Timeouts straight out of §1's table, and deliberately nothing about silence: a chat ask has
    // no whole-call ceiling (§1), and §4's silence rule is NOT the socket's business.
    let builder = match kind.method() {
        "POST" => ureq::post(&url),
        _ => ureq::get(&url).force_send_body(),
    };
    let builder = builder
        .config()
        .timeout_connect(Some(CONNECT_TIMEOUT))
        .timeout_recv_body(overall)
        .timeout_global(overall)
        // 4xx/5xx come back as a response we read, not as an error: §1's upload rule
        // (`services::upload_refused`) needs the status AND the server's own message, which a
        // status-as-error path would throw away.
        .http_status_as_error(false)
        .build();

    let sent_bytes = body.map(|text| text.len() as i64);
    let started = Instant::now();

    let mut builder = builder;
    if let Some(bearer) = services::authorization(&endpoint.key) {
        builder = builder.header("Authorization", bearer);
    }
    // audio.cpp's upload names the file it carries in a header, not in a multipart envelope
    // (§1: `POST /v1/ui/upload` raw body with `X-Audiocpp-Filename`). The name arrives as an
    // extension of `body`: every other kind passes `None` and sends no such header.
    if let Some(name) = filename {
        builder = builder.header("X-Audiocpp-Filename", name);
    }
    // Every other body is JSON and says so (§1). ureq sends a string body with no content type at all; the
    // llama.cpp router reads it anyway, but a FastAPI proxy in front of a model answers 400 "body: Input should
    // be a valid dictionary or object" to the very same bytes.
    if body.is_some() && filename.is_none() {
        builder = builder.header("Content-Type", "application/json");
    }

    let response = match body {
        Some(text) => builder.send(text),
        None => builder.send_empty(),
    };

    let response = match response {
        Ok(response) => response,
        Err(err) => return Err(down(server, &url, &err.to_string())),
    };
    let status = response.status().as_u16();
    let header = response
        .headers()
        .get("content-type")
        .map(|value| value.to_str().unwrap_or_default().trim().to_string())
        .unwrap_or_default();
    let mut reader = response.into_body().into_reader();
    // §7's live half: a caller that wants the words as they arrive hands over somewhere to put
    // them, and the read loop below becomes a pump instead of a drain.
    let on_chunk = delta;
    let mut buf = Vec::new();
    // The first byte's moment, kept separately because the loop must run to the END of the body:
    // breaking on the first read (the shape this had) truncates any reply that arrives over more
    // than one read to its first chunk, which is exactly what cut a streamed answer off after its
    // first word.
    let mut first: Option<i64> = None;
    // §10's thinking column, for the one reply that can say where its reasoning ended: an event
    // stream carries `reasoning_content` deltas before its `content` deltas, so the first read that
    // brings answer text after reasoning text is the moment the model stopped thinking.
    let mut saw_reasoning = false;
    let mut thinking: Option<i64> = None;
    let total_reads = loop {
        let mut chunk = [0u8; 8192];
        match reader.read(&mut chunk) {
            Ok(0) => break started.elapsed().as_millis() as i64,
            Ok(n) => {
                let elapsed = started.elapsed().as_millis() as i64;
                // Any byte counts (§4), except a stream of nothing but blank text: a model stuck on
                // newlines is not answering, and only the stall watch can end it.
                if let Some(probe) = probe {
                    if crate::llm_liveness::says_something(&chunk[..n]) {
                        probe.byte();
                    }
                }
                buf.extend_from_slice(&chunk[..n]);
                if thinking.is_none() {
                    let fresh = String::from_utf8_lossy(&chunk[..n]);
                    if fresh.contains("reasoning_content") {
                        saw_reasoning = true;
                    }
                    if saw_reasoning && fresh.contains("\"content\"") {
                        thinking = Some(elapsed);
                    }
                }
                // §7's live half, part 1: hand the sink everything received so far.
                if let Some(sink) = &on_chunk {
                    sink(&buf);
                }
                if first.is_none() {
                    first = Some(elapsed);
                }
            }
            Err(why) => {
                drop(reader);
                let _ = why;
                return Err(down(server, &url, "the connection died mid-reply"));
            }
        }
    };
    let total = total_reads;
    let text = String::from_utf8_lossy(&buf).to_string();
    // The header was read off the response before its body reader was taken; keep it for §1's
    // routing rule.
    let content_type = header.to_ascii_lowercase();

    if cancelled() {
        return Err(format!("{}: cancelled mid-reply", kind.path(job)));
    }

    Ok(Sent {
        status,
        body: text,
        content_type,
        sent_bytes,
        received_bytes: buf.len() as i64,
        wait_ms: 0,
        first_byte_ms: Some(first.unwrap_or_else(|| started.elapsed().as_millis() as i64)),
        on_wire_ms: Some(total),
        thinking_ms: thinking,
    })
}

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
    // `Dial` carries the upload's file name, which this form never has.
    #![allow(clippy::too_many_arguments)]
    call_dialed(
        tree,
        base,
        &Dial {
            server,
            endpoint,
            kind,
            job,
            body: body.map(str::as_bytes),
            filename: None,
            cancelled,
            liveness: None,
            waited_ms: 0,
            delta: None,
            budget: None,
            attempt: 1,
        },
    )
}

pub fn call_budget(
    tree: &crate::layout::Tree,
    base: &Request,
    server: Server,
    endpoint: &Endpoint,
    kind: Kind,
    job: Option<&str>,
    body: Option<&str>,
    budget: Option<std::time::Duration>,
    cancelled: &dyn Fn() -> bool,
) -> (Request, Result<Sent, String>) {
    #![allow(clippy::too_many_arguments)]
    call_dialed(
        tree,
        base,
        &Dial {
            server,
            endpoint,
            kind,
            job,
            body: body.map(str::as_bytes),
            filename: None,
            cancelled,
            liveness: None,
            waited_ms: 0,
            delta: None,
            budget,
            attempt: 1,
        },
    )
}

pub struct Dial<'a> {
    pub server: Server,
    pub endpoint: &'a Endpoint,
    pub kind: Kind,
    pub job: Option<&'a str>,
    /// The request's bytes as they go on the wire: JSON for every kind but the upload, whose body is the
    /// file itself. Bytes, not text: a WAV pushed through a lossy UTF-8 string arrives with every invalid
    /// byte replaced (audio.cpp answered 500 "invalid WAV RIFF header" to a chunk 1.9 times its size).
    pub body: Option<&'a [u8]>,
    pub filename: Option<&'a str>,
    pub cancelled: &'a dyn Fn() -> bool,
    /// §4's watch for this call: `Some` puts the request under the stall rule and the heartbeat. Only a chat ask
    /// sets it — an audio or image job has no silence bound of its own (§1 bounds those by kind).
    pub liveness: Option<crate::llm_watch_leg::Liveness>,
    /// Milliseconds this request spent waiting its turn before the caller got here, 0 when it never
    /// queued.
    pub waited_ms: i64,
    /// §7's live reply, seen WHILE it arrives: called after every read with EVERY byte of the body
    /// received so far, so the caller only decodes what it has not seen.
    pub delta: Option<Delta>,
    /// A per-call ceiling on the whole exchange, `Some` only when § bounds this request tighter
    /// than §1's table does ([`Kind::ListModels`] at fifteen seconds is that case).
    pub budget: Option<std::time::Duration>,
    /// Which try of this request the row says: 1 is the first, and §10 gives every retry a line of
    /// its own with its own number, so [`crate::llm_retry_leg`] passes its own count rather than
    /// letting the leg guess.
    pub attempt: u32,
}

pub type Delta = std::sync::Arc<dyn Fn(&[u8]) + Send + Sync>;

/// [`call`] from a [`Dial`], so a caller names each leg instead of counting positions.
pub fn call_dialed(
    tree: &crate::layout::Tree,
    base: &Request,
    dial: &Dial<'_>,
) -> (Request, Result<Sent, String>) {
    let attempt = dial.attempt.max(1);
    let timer_start = epoch_ms();
    // Set when this call goes out under §4's watch, so the outcome can ask the watch whether it
    // ended the call.
    let mut watched: Option<std::sync::Arc<crate::llm_watch_leg::Probe>> = None;
    let result = match &dial.liveness {
        // Under §4's watch: the send runs where a blocked read cannot stop the ticker, and the
        // probe is what tells the ticker that bytes are arriving.
        Some(liveness) => {
            let probe = crate::llm_watch_leg::Probe::new();
            // Remembered so the outcome below can tell §4's give-up from every other failure: the
            // watch marks its own probe when it ends a call, and nothing else writes that flag.
            watched = Some(probe.clone());
            // The worker is detached (see `guarded`), so it must OWN everything: the endpoint, and
            // copies of the two borrowed strings it sends.
            let ping = Some(probe.clone());
            let cancelled_now = crate::cancel_leg::cancel_check_now();
            let server = dial.server;
            let endpoint = dial.endpoint.clone();
            let kind = dial.kind;
            let job = dial.job.map(str::to_owned);
            let body = dial.body.map(<[u8]>::to_vec);
            let filename = dial.filename.map(str::to_owned);
            // A watched send runs detached, so the live-reply sink crosses into it by clone,
            // exactly as the two owned strings above do.
            let delta = dial.delta.clone();
            crate::llm_watch_leg::guarded(liveness, &probe, move || {
                send_pinged(
                    server,
                    &endpoint,
                    kind,
                    job.as_deref(),
                    body.as_deref(),
                    filename.as_deref(),
                    &cancelled_now,
                    ping.as_deref(),
                    delta.as_ref(),
                    None,
                )
            })
        }
        None => send_pinged(
            dial.server,
            dial.endpoint,
            dial.kind,
            dial.job,
            dial.body,
            dial.filename,
            dial.cancelled,
            None,
            dial.delta.as_ref(),
            dial.budget,
        ),
    };
    let mut timer = Timer::queued(dial.waited_ms, timer_start);
    match &result {
        Ok(reply) => {
            if let Some(first) = reply.first_byte_ms {
                timer.first_byte(timer_start + first);
            }
            if let Some(total) = reply.on_wire_ms {
                timer.finished(timer_start + total);
            }
            // §10's thinking column, measured where the bytes were read.
            if let Some(thinking) = reply.thinking_ms {
                timer.thought(thinking);
            }
        }
        Err(_) => timer.finished(epoch_ms()),
    }

    let outcome = match &result {
        Ok(reply) if reply.ok() => Outcome::Ok,
        Ok(reply) => Outcome::Error(format!("{}", reply.status)),
        // §4 ended this call for silence.
        Err(_) if watched.as_ref().is_some_and(|probe| probe.gave_up()) => Outcome::Stalled,
        Err(reason) if reason.contains("cancelled") => Outcome::Cancelled,
        Err(reason) => Outcome::Error(reason.clone()),
    };

    let mut row = fill_sizes(
        request_timing::finish_row(base, &timer, outcome, attempt),
        dial.body,
    );
    // What came back, off the wire rather than guessed: §10 wants the size of the reply beside the
    // size of the request.
    if let Ok(reply) = &result {
        if row.received_bytes.is_none() {
            row.received_bytes = Some(reply.received_bytes);
        }
        // "tokens in and out when the server says": read back off the body that arrived, empty when
        // the server never spoke about tokens (§10), which is why nothing here estimates them.
        if row.tokens_in.is_none() && row.tokens_out.is_none() {
            let (taken, made) = request_timing::usage_of(&reply.body);
            row.tokens_in = taken;
            row.tokens_out = made;
        }
    }
    // §10's run column: the run's `llm/` page name, empty outside a run. Every call site leaves it
    // blank because only the exchange recorder knows the page, so it is stamped here instead.
    request_timing::stamp_run(&mut row);
    // Written first, used after.
    let _ = requests::record(tree, &row);
    (row, result)
}

/// The two size columns §09 §5 asks for, on whatever [`request_timing::finish_row`] built.
pub fn fill_sizes(mut row: Request, body: Option<&[u8]>) -> Request {
    let Some(text) = body else { return row };
    if row.sent_bytes.is_none() {
        let asked: serde_json::Value =
            serde_json::from_slice(text).unwrap_or(serde_json::Value::Null);
        let bytes = asked
            .get("messages")
            .and_then(|messages| messages.as_array())
            .map(|list| {
                list.iter()
                    .flat_map(|message| {
                        message
                            .get("content")
                            .and_then(|content| content.as_array())
                            .map(|parts| parts.to_vec())
                            .unwrap_or_default()
                    })
                    .filter(|part| {
                        part.get("type").and_then(serde_json::Value::as_str) == Some("text")
                    })
                    .map(|part| {
                        part.get("text")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or_default()
                            .len()
                    })
                    .sum::<usize>()
            });
        row.sent_bytes = Some(bytes.unwrap_or(text.len()) as i64);
    }
    if row.images.is_none() {
        let images = serde_json::from_slice::<serde_json::Value>(text)
            .ok()
            .and_then(|asked| {
                Some(
                    asked
                        .get("messages")?
                        .as_array()?
                        .iter()
                        .flat_map(|message| {
                            message
                                .get("content")
                                .and_then(|content| content.as_array())
                                .map(|parts| parts.to_vec())
                                .unwrap_or_default()
                        })
                        .filter(|part| {
                            part.get("type").and_then(serde_json::Value::as_str)
                                == Some("image_url")
                        })
                        .count(),
                )
            })
            .unwrap_or_else(|| {
                String::from_utf8_lossy(text)
                    .matches("\"image_url\"")
                    .count()
                    / 2
            });
        if images > 0 {
            row.images = u32::try_from(images).ok();
        }
    }
    row
}

pub fn base_row(
    server: Server,
    model: &str,
    kind: &str,
    run: &str,
    step: &str,
    job: &str,
) -> Request {
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
