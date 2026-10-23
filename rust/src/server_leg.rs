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
    /// The response's `content-type` header, lower-cased, empty when the server sent none. §09 §1 routes the
    /// answer on it: only a `text/event-stream` reply is read as an event stream, anything else as one JSON
    /// object — so this has to survive from the wire to whoever parses the body.
    pub content_type: String,
    /// Bytes of the request body that went out; `None` for a GET, which sends none.
    pub sent_bytes: Option<i64>,
    pub received_bytes: i64,
    /// Milliseconds waiting for a slot before going out. 0 for a leg that never queued.
    pub wait_ms: i64,
    pub first_byte_ms: Option<i64>,
    pub on_wire_ms: Option<i64>,
    /// For a thinking model that STREAMS its reasoning before its answer: how much of the trip was
    /// reasoning. Measured on the wire ([`send_pinged`]), empty for every other reply — §10 asks for it
    /// "for a thinking model", and a server that sends no reasoning never says where it ended.
    pub thinking_ms: Option<i64>,
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
pub fn epoch_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_millis() as i64)
        .unwrap_or_default()
}

/// The machine's offset from UTC in seconds, shared with the exchange page's stamp so a page and a
/// `requests.tsv` row cannot disagree about what time it was.
///
/// Kept here rather than in `requests.rs` because that module stays pure data and takes the
/// offset as an argument; this module is the one allowed to ask the machine.
pub fn local_offset_secs() -> i32 {
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

/// [`send`] with a whole-call ceiling of this caller's own.
///
/// §1's table ([`services::timeout_for`]) answers `None` for most kinds — an hour of audio is an hour of
/// work — but a few calls have a bound that belongs to the SCREEN rather than to the service: Fetch models
/// gets fifteen seconds because a person waits on the dialog ([`crate::model_list::LIST_MODELS_SECONDS`]),
/// and F0.13's two probes get 60 s and 120 s for the same reason. The budget rides here rather than at
/// the call site because the socket is the only thing that can enforce it; §1's `None` stays untouched for
/// every other caller.
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
        server, endpoint, kind, job, body, None, cancelled, None, None, budget,
    )
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
    send_pinged(
        server, endpoint, kind, job, body, filename, cancelled, None, None, None,
    )
}

/// [`send_named`] with the §4 probe: every byte this read comes off the wire is a heartbeat's evidence, so the
/// reader pings here rather than anywhere the event parser would have to be involved.
fn send_pinged(
    server: Server,
    endpoint: &Endpoint,
    kind: Kind,
    job: Option<&str>,
    body: Option<&str>,
    filename: Option<&str>,
    cancelled: &dyn Fn() -> bool,
    probe: Option<&crate::llm_watch_leg::Probe>,
    delta: Option<&Delta>,
    budget: Option<std::time::Duration>,
) -> Result<Sent, String> {
    // The eighth argument is §4's byte ping, which only a watched leg has; folding it into `Dial` would make every
    // unwatched caller carry a field it never sets.
    #![allow(clippy::too_many_arguments)]
    if services::rides_cancel_context(kind) && cancelled() {
        return Err(format!("{}: cancelled before sending", kind.path(job)));
    }

    let url = format!("{}{}", endpoint.url.trim_end_matches('/'), kind.path(job));

    // The timeouts come straight out of §1's table. `Some(d)` bounds the whole call; `None`
    // bounds only the connect, per CONNECT_TIMEOUT's note above. A caller with a bound of its own (a
    // Settings row a person is watching) supplies one through [`send_budget`]; the table never says so.
    let overall = services::timeout_for(kind).or(budget);
    // Timeouts straight out of §1's table, and deliberately nothing about silence: a chat ask has no whole-call
    // ceiling (§1), and §4's silence rule is NOT the socket's business — `guarded` watches it from outside, so this
    // read may block as long as the server makes it and the call still ends on time. Bounding the read here would put
    // ureq's timeout in the race for the caller's error sentence, which is §4's to say.
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

    // ureq 3's `.header()` takes `self` by value and returns the builder, so each header has to be
    // threaded through a binding that is then actually sent: a chain that discards its result (the
    // shape this had) leaves the request going out with neither the bearer nor the upload's file name.
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
    // ureq drops the headers once the body is taken, so the content type is grabbed here.
    let header = response
        .headers()
        .get("content-type")
        .map(|value| value.to_str().unwrap_or_default().trim().to_string())
        .unwrap_or_default();
    let mut reader = response.into_body().into_reader();
    // §7's live half: a caller that wants the words as they arrive hands over somewhere to put them, and
    // the read loop below becomes a pump instead of a drain. Everything else (every other leg, every
    // non-streaming reply) takes the `None` path unchanged and reads straight through.
    let on_chunk = delta;
    let mut buf = Vec::new();
    // The first byte's moment, kept separately because the loop must run to the END of the body: breaking on the
    // first read (the shape this had) truncates any reply that arrives over more than one read to its first chunk,
    // which is exactly what cut a streamed answer off after its first word.
    let mut first: Option<i64> = None;
    // §10's thinking column, for the one reply that can say where its reasoning ended: an event stream
    // carries `reasoning_content` deltas before its `content` deltas, so the first read that brings
    // answer text after reasoning text is the moment the model stopped thinking. A plain-JSON answer has
    // no such moment on the wire — its reasoning arrives inside the same body as its answer — so this
    // stays empty there rather than claiming the whole call was thinking.
    let mut saw_reasoning = false;
    let mut thinking: Option<i64> = None;
    let total_reads = loop {
        let mut chunk = [0u8; 8192];
        match reader.read(&mut chunk) {
            Ok(0) => break started.elapsed().as_millis() as i64,
            Ok(n) => {
                let elapsed = started.elapsed().as_millis() as i64;
                // Any byte at all counts (§4), including one the event parser never turns into anything.
                if let Some(probe) = probe {
                    probe.byte();
                }
                buf.extend_from_slice(&chunk[..n]);
                // The split is looked for in THIS read's bytes only: re-scanning the whole buffer would
                // find the answer marker again on every later read and move the boundary to the end.
                if thinking.is_none() {
                    let fresh = String::from_utf8_lossy(&chunk[..n]);
                    if fresh.contains("reasoning_content") {
                        saw_reasoning = true;
                    }
                    if saw_reasoning && fresh.contains("\"content\"") {
                        thinking = Some(elapsed);
                    }
                }
                // §7's live half, part 1: hand the sink everything received so far. The sink keeps its own
                // read offset, so a caller decodes only what it has not seen; handing it `&chunk[..n]` instead
                // would make every caller re-invent that offset for one field's convenience.
                if let Some(sink) = &on_chunk {
                    sink(&buf);
                }
                // First successful read is the first byte. Nothing breaks here: only `Ok(0)` ends the body.
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
    // The header was read off the response before its body reader was taken; keep it for §1's routing rule.
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
        // An empty body has no first byte of its own; it keeps the old meaning of "the call ended".
        first_byte_ms: Some(first.unwrap_or_else(|| started.elapsed().as_millis() as i64)),
        on_wire_ms: Some(total),
        thinking_ms: thinking,
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
            liveness: None,
            waited_ms: 0,
            delta: None,
            budget: None,
            attempt: 1,
        },
    )
}

/// Everything [`call`] takes plus the file name the upload carries. Grouped into [`Dial`] rather
/// than spread across nine positional arguments, because the eight- and nine-argument forms are the
/// two every flow reaches for and a mis-ordered `job`/`body` pair would dial the wrong place.
/// [`call`] with a per-call ceiling on the whole exchange, for a request § bounds tighter than §1's table
/// does ([`crate::model_list_fetch`]’s fifteen-second listing is the one that needed it). `None` keeps §1's
/// bound, which is what every other caller wants; passing one cannot leave a logged call unbounded.
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
            body,
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
    /// The sd.cpp job id the two job kinds act on; every other kind ignores it.
    pub job: Option<&'a str>,
    pub body: Option<&'a str>,
    /// audio.cpp's `X-Audiocpp-Filename`; only the upload kind sets it.
    pub filename: Option<&'a str>,
    /// The run's cancel context.
    pub cancelled: &'a dyn Fn() -> bool,
    /// §4's watch for this call: `Some` puts the request under the stall rule and the heartbeat. Only a chat ask
    /// sets it — an audio or image job has no silence bound of its own (§1 bounds those by kind).
    pub liveness: Option<crate::llm_watch_leg::Liveness>,
    /// Milliseconds this request spent waiting for a slot before the caller got here, 0 when it never queued.
    /// The socket cannot report that wait — it happens outside the trip — so the holder passes it through here and
    /// [`call_dialed`] writes it into the row's `wait_s` column (§10's slot-count evidence).
    pub waited_ms: i64,
    /// §7's live reply, seen WHILE it arrives: called after every read with EVERY byte of the body received so
    /// far, so the caller only decodes what it has not seen. `None` (every other leg) reads straight through.
    ///
    /// It runs wherever the bytes are read, which under §4 is a detached worker — hence an owned sink
    /// (`crate::exchange_leg::ServerHook`) rather than a borrow: borrowed data cannot cross into that worker.
    /// The sink writes the exchange page itself; the page is a mutex for exactly that reason.
    pub delta: Option<Delta>,
    /// A per-call ceiling on the whole exchange, `Some` only when § bounds this request tighter than §1's
    /// table does ([`Kind::ListModels`] at fifteen seconds is that case). `call` leaves it `None`, so the
    /// ordinary legs keep §1's bound untouched.
    pub budget: Option<std::time::Duration>,
    /// Which try of this request the row says: 1 is the first, and §10 gives every retry a line of its own
    /// with its own number, so [`crate::llm_retry_leg`] passes its own count rather than letting the leg
    /// guess. A leg that only ever dials once leaves it 1.
    pub attempt: u32,
}

/// What [`Dial::delta`] holds: somewhere to put the body as it grows. A `'static` `Arc`, not a plain `&dyn Fn`,
/// because §4's watched send runs on its own detached thread (`llm_watch_leg::guarded`) and borrowed data cannot
/// cross into it — the same reason that worker copies the endpoint and the body instead of borrowing them.
pub type Delta = std::sync::Arc<dyn Fn(&[u8]) + Send + Sync>;

/// [`call`] from a [`Dial`], so a caller names each leg instead of counting positions.
pub fn call_dialed(
    tree: &crate::layout::Tree,
    base: &Request,
    dial: &Dial<'_>,
) -> (Request, Result<Sent, String>) {
    let attempt = dial.attempt.max(1);
    let timer_start = epoch_ms();
    // Set when this call goes out under §4's watch, so the outcome can ask the watch whether it ended the call.
    let mut watched: Option<std::sync::Arc<crate::llm_watch_leg::Probe>> = None;
    let result = match &dial.liveness {
        // Under §4's watch: the send runs where a blocked read cannot stop the ticker, and the probe is what tells
        // the ticker that bytes are arriving.
        Some(liveness) => {
            let probe = crate::llm_watch_leg::Probe::new();
            // Remembered so the outcome below can tell §4's give-up from every other failure: the watch
            // marks its own probe when it ends a call, and nothing else writes that flag.
            watched = Some(probe.clone());
            // The worker is detached (see `guarded`), so it must OWN everything: the endpoint, and copies of the two
            // borrowed strings it sends. Nothing here may borrow the caller's `Dial`.
            let ping = Some(probe.clone());
            // `Dial.cancelled` is a `&dyn Fn`, which cannot cross into the worker thread. The switch itself is a
            // process-wide atomic, so the worker takes the same live read the callers elsewhere use.
            let cancelled_now = crate::cancel_leg::cancel_check_now();
            let server = dial.server;
            let endpoint = dial.endpoint.clone();
            let kind = dial.kind;
            let job = dial.job.map(str::to_owned);
            let body = dial.body.map(str::to_owned);
            let filename = dial.filename.map(str::to_owned);
            // A watched send runs detached, so the live-reply sink crosses into it by clone, exactly as the
            // two owned strings above do; an unwatched send borrows the caller's `Dial` and needs no copy.
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
    // The reply's own timings arrive as elapsed times from the socket; the row wants moments on one clock,
    // so they are shifted onto `timer_start` here and nowhere else.
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
        // §4 ended this call for silence. Checked before the cancellation test because the two are
        // different endings with different words — "nothing arrived in 5m0s" is not something ⏹ did.
        Err(_) if watched.as_ref().is_some_and(|probe| probe.gave_up()) => Outcome::Stalled,
        Err(reason) if reason.contains("cancelled") => Outcome::Cancelled,
        Err(reason) => Outcome::Error(reason.clone()),
    };

    let mut row = fill_sizes(
        request_timing::finish_row(base, &timer, outcome, attempt),
        dial.body,
    );
    // What came back, off the wire rather than guessed: §10 wants the size of the reply beside the size
    // of the request. A caller that already filled the column (a leg that counts its own payload) is left.
    if let Ok(reply) = &result {
        if row.received_bytes.is_none() {
            row.received_bytes = Some(reply.received_bytes);
        }
        // "tokens in and out when the server says": read back off the body that arrived, empty when the
        // server never spoke about tokens (§10), which is why nothing here estimates them.
        if row.tokens_in.is_none() && row.tokens_out.is_none() {
            let (taken, made) = request_timing::usage_of(&reply.body);
            row.tokens_in = taken;
            row.tokens_out = made;
        }
    }
    // §10's run column: the run's `llm/` page name, empty outside a run. Every call site leaves it blank
    // because only the exchange recorder knows the page, so it is stamped here instead.
    request_timing::stamp_run(&mut row);
    // Written first, used after. A failure to write the log is not allowed to swallow the
    // reply, so it is reported in the row's own error channel instead: the caller sees the
    // request went through even when the file could not be opened.
    let _ = requests::record(tree, &row);
    (row, result)
}

/// The two size columns §09 §5 asks for, on whatever [`request_timing::finish_row`] built.
///
/// `P.machine.promptMaxChars` is judged at the request builder ([`crate::llm_budget::check`],
/// applied in [`crate::llm_request::body`]); the row is where the OTHER half of that rule lives —
/// how big what actually went out was, so a person can see how close a job ran to the bound and
/// which step's request blew it. Bytes and pictures are read back off the same JSON that went on
/// the wire, so the row records the request rather than a caller's estimate of it: `sent_bytes`
/// counts every text part, and `images` counts the `image_url` parts the server was handed. A
/// caller that filled either column itself (a test, an upload leg that knows its payload size) is
/// left alone; a body with no pictures leaves `images` empty rather than writing a zero, because
/// §10's reader must be able to tell "no images" from "not recorded".
pub fn fill_sizes(mut row: Request, body: Option<&str>) -> Request {
    let Some(text) = body else { return row };
    if row.sent_bytes.is_none() {
        let asked: serde_json::Value =
            serde_json::from_str(text).unwrap_or(serde_json::Value::Null);
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
        // A body that is not the chat shape (an audio task's JSON, a raw upload) has no message
        // list to measure: then what went out IS the body, and saying so beats leaving it blank.
        row.sent_bytes = Some(bytes.unwrap_or(text.len()) as i64);
    }
    if row.images.is_none() {
        // One picture is spelled `"image_url"` TWICE in the JSON the wire carried — as the part's
        // `type` and again as its key — so the parts are counted, not the string.
        let images = serde_json::from_str::<serde_json::Value>(text)
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
            .unwrap_or_else(|| text.matches("\"image_url\"").count() / 2);
        if images > 0 {
            row.images = u32::try_from(images).ok();
        }
    }
    row
}

/// A `Request` carrying only what the caller knows, ready for [`call`] to finish.
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
