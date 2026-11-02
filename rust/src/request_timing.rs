//! Timing every request that leaves the machine — `spec/09-llm-and-tools.md` §10.

use crate::requests::{Outcome, Request, Service};

/// The services whose traffic belongs in `requests.tsv`, in the order a summary lists them.
pub const OUTSIDE_SERVICES: [Service; 4] =
    [Service::Llm, Service::Audio, Service::Image, Service::Web];

/// A stopwatch for one request, driven by timestamps the caller supplies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timer {
    /// Milliseconds spent waiting its turn before going out. Zero means never queued.
    wait_ms: i64,
    /// When the request went out, in the caller's epoch milliseconds.
    sent_ms: i64,
    first_byte_ms: Option<i64>,
    end_ms: Option<i64>,
    thinking_ms: Option<i64>,
}

impl Timer {
    /// A timer for a request that waited `waited_ms` for a slot and went out at `sent_ms`.
    pub fn queued(waited_ms: i64, sent_ms: i64) -> Timer {
        Timer {
            wait_ms: waited_ms.max(0),
            sent_ms,
            first_byte_ms: None,
            end_ms: None,
            thinking_ms: None,
        }
    }

    /// A timer for a request that needed no slot — an upload, a web read — going out at `sent_ms`.
    pub fn sent(sent_ms: i64) -> Timer {
        Timer::queued(0, sent_ms)
    }

    /// Mark the first byte arriving at `at_ms`. Recorded once; a later call is ignored, because
    /// "first" means the first.
    pub fn first_byte(&mut self, at_ms: i64) {
        if self.first_byte_ms.is_none() {
            self.first_byte_ms = Some(at_ms);
        }
    }

    /// Mark the reply finished at `at_ms`.
    pub fn finished(&mut self, at_ms: i64) {
        self.end_ms = Some(at_ms);
    }

    /// How much of the wait was thinking, for a model that reports it separately.
    pub fn thought(&mut self, ms: i64) {
        self.thinking_ms = Some(ms.max(0));
    }

    /// Milliseconds waiting its turn.
    pub fn wait_ms(&self) -> i64 {
        self.wait_ms
    }

    /// Milliseconds from going out to the first byte, or `None` if nothing ever came back — a
    /// connection refused has no first byte, and writing `0.00` there would claim one arrived at once.
    pub fn first_byte_ms(&self) -> Option<i64> {
        self.first_byte_ms.map(|at| (at - self.sent_ms).max(0))
    }

    /// Total milliseconds on the wire, or `None` before [`Timer::finished`].
    pub fn on_wire_ms(&self) -> Option<i64> {
        self.end_ms.map(|end| (end - self.sent_ms).max(0))
    }

    /// Milliseconds of thinking, when the server said.
    pub fn thinking_ms(&self) -> Option<i64> {
        self.thinking_ms
    }
}

/// Milliseconds as the file's seconds. No rounding here: `requests::write_line` formats two decimals,
/// and rounding twice is how a total stops matching the sum of its parts.
pub fn secs(ms: i64) -> f64 {
    ms as f64 / 1000.0
}

/// The row to write when a request ends.
pub fn finish_row(base: &Request, timer: &Timer, outcome: Outcome, attempt: u32) -> Request {
    Request {
        started: base.started.clone(),
        run: base.run.clone(),
        step: base.step.clone(),
        job: base.job.clone(),
        service: base.service,
        model: base.model.clone(),
        kind: base.kind.clone(),
        sent_bytes: base.sent_bytes,
        images: base.images,
        received_bytes: base.received_bytes,
        tokens_in: base.tokens_in,
        tokens_out: base.tokens_out,
        wait_s: Some(secs(timer.wait_ms())),
        first_byte_s: timer.first_byte_ms().map(secs),
        on_wire_s: timer.on_wire_ms().map(secs),
        thinking_s: timer.thinking_ms().map(secs),
        outcome,
        attempt,
    }
}

/// §10's `run` column: the name of the run's `llm/` page, empty outside a run.
pub fn stamp_run(row: &mut crate::requests::Request) {
    if row.run.is_empty() {
        // A test may pin the column directly; the program leaves it empty and it is filled here.
        row.run = crate::exchange_leg::current_page_name();
    }
}

/// What the server said it spent, in tokens: `(in, out)`.
pub fn usage_of(body: &str) -> (Option<i64>, Option<i64>) {
    let Ok(asked) = serde_json::from_str::<serde_json::Value>(body) else {
        // Not one JSON object, so it is either a stream or noise; the scan below decides which.
        return stream_usage(body);
    };
    match usage_pair(asked.get("usage")) {
        Some(said) => said,
        // A JSON answer that says nothing about tokens may still be a stream-shaped body; a plain
        // answer with no `usage` is the server not saying, which is an empty pair.
        None => stream_usage(body),
    }
}

/// The same pair read out of an event stream's chunks: each `data:` line is a chunk, and the LAST one
/// carrying `usage` is the finished count (vLLM and llama.cpp repeat the running totals per chunk).
fn stream_usage(body: &str) -> (Option<i64>, Option<i64>) {
    let mut latest: Option<(Option<i64>, Option<i64>)> = None;
    for line in body.lines() {
        let Some(payload) = line.trim().strip_prefix("data:") else {
            continue;
        };
        let Ok(chunk) = serde_json::from_str::<serde_json::Value>(payload.trim()) else {
            continue;
        };
        if let Some(said) = usage_pair(chunk.get("usage")) {
            latest = Some(said);
        }
    }
    latest.unwrap_or((None, None))
}

/// One object's token pair, `None` when that key is missing or carries neither count: an object with
/// nothing in it is the server saying nothing, which is not the same as a zero.
fn usage_pair(usage: Option<&serde_json::Value>) -> Option<(Option<i64>, Option<i64>)> {
    let usage = usage?;
    let taken = usage
        .get("prompt_tokens")
        .and_then(serde_json::Value::as_i64);
    let made = usage
        .get("completion_tokens")
        .and_then(serde_json::Value::as_i64);
    (taken.is_some() || made.is_some()).then_some((taken, made))
}

/// The row for a reply served from the cache.
pub fn cached_row(base: &Request, attempt: u32) -> Request {
    Request {
        started: base.started.clone(),
        run: base.run.clone(),
        step: base.step.clone(),
        job: base.job.clone(),
        service: base.service,
        model: base.model.clone(),
        kind: base.kind.clone(),
        sent_bytes: base.sent_bytes,
        images: base.images,
        received_bytes: base.received_bytes,
        tokens_in: base.tokens_in,
        tokens_out: base.tokens_out,
        wait_s: None,
        first_byte_s: None,
        on_wire_s: None,
        thinking_s: None,
        outcome: Outcome::Cache,
        attempt,
    }
}

/// Which attempt number this request should carry.
pub fn next_attempt(previous: Option<&Request>) -> u32 {
    previous.map(|row| row.attempt + 1).unwrap_or(1)
}

/// One service's share of a run, as the closing log line needs it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ServiceTally {
    pub service: Service,
    /// Rows written for this service, cache hits included.
    pub count: usize,
    /// Seconds on the wire across those rows. Cache rows add nothing — they were never on the wire.
    pub on_wire_s: f64,
    /// Seconds spent waiting its turn.
    pub wait_s: f64,
    /// How many of the rows were served from the cache.
    pub cached: usize,
}

/// Group rows by service, in [`OUTSIDE_SERVICES`]'s order, keeping only services that appear.
pub fn tally(rows: &[Request]) -> Vec<ServiceTally> {
    OUTSIDE_SERVICES
        .iter()
        .filter_map(|service| {
            let mine: Vec<&Request> = rows.iter().filter(|row| row.service == *service).collect();
            if mine.is_empty() {
                return None;
            }
            Some(ServiceTally {
                service: *service,
                count: mine.len(),
                on_wire_s: mine.iter().filter_map(|row| row.on_wire_s).sum(),
                wait_s: mine.iter().filter_map(|row| row.wait_s).sum(),
                cached: mine
                    .iter()
                    .filter(|row| row.outcome == Outcome::Cache)
                    .count(),
            })
        })
        .collect()
}

/// The service's name as the file writes it.
fn service_name(service: Service) -> String {
    serde_json::to_string(&service)
        .expect("a service name is a word")
        .trim_matches('"')
        .to_string()
}

/// A duration as the closing line spells it: `2h 13m`, `6m 10s`, `12s`.
fn spell(seconds: f64) -> String {
    let whole = seconds.round().max(0.0) as u64;
    if whole >= 3600 {
        let hours = whole / 3600;
        let minutes = (whole % 3600) / 60;
        if minutes == 0 {
            return format!("{hours}h");
        }
        return format!("{hours}h {minutes}m");
    }
    if whole >= 60 {
        let minutes = whole / 60;
        let rest = whole % 60;
        // `4m` when the remainder is nothing, `6m 10s` when there is one -- §10 prints both shapes
        // in the same sentence, so the zero-remainder form drops the unit rather than writing 0s.
        if rest == 0 {
            return format!("{minutes}m");
        }
        return format!("{minutes}m {rest}s");
    }
    format!("{whole}s")
}

/// The line a run ends with, one clause per service that was used.
pub fn summary_line(tallies: &[ServiceTally]) -> String {
    if tallies.is_empty() {
        return ">>> requests: none".to_string();
    }
    let parts: Vec<String> = tallies
        .iter()
        .enumerate()
        .map(|(index, tally)| {
            let name = service_name(tally.service);
            // "on the wire" is said once, on the first clause; later services just give their total.
            let tail = if index == 0 { " on the wire" } else { "" };
            let mut line = format!("{name} {} in {}{tail}", tally.count, spell(tally.on_wire_s));
            if tally.wait_s > 0.0 {
                line.push_str(&format!(", {} waiting its turn", spell(tally.wait_s)));
            }
            line
        })
        .collect();
    format!(">>> requests: {}", parts.join("; "))
}

/// File the line a cache hit owes §10: "A reply served from the cache is a line too, marked as such
/// with no time on the wire, so the file shows what a re-run saved."
pub fn record_cached(tree: &crate::layout::Tree, mut base: crate::requests::Request) {
    if base.started.is_empty() {
        base.started = crate::requests::format_started(
            crate::server_leg::epoch_ms(),
            crate::server_leg::local_offset_secs(),
        );
    }
    stamp_run(&mut base);
    let _ = crate::requests::record(tree, &cached_row(&base, 1));
}
