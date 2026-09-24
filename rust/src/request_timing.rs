//! Timing every request that leaves the machine — `spec/09-llm-and-tools.md` §10.
//!
//! [`crate::requests`] owns the file: its eighteen columns, the four services, the five outcomes,
//! how a row is written and read back. This module owns the *numbers* that go into those columns —
//! how long a request queued behind the gate (§4), how long until the first byte came back, how long
//! it was on the wire in all, how much of that was thinking — plus the cache row that has no time at
//! all, the per-service summary a run ends with, and the wait-against-wire ratio the slot counts are
//! chosen by.
//!
//! Nothing here reads a clock or opens a socket. Every timestamp arrives as an argument from the
//! caller's HTTP layer, which is what makes the whole thing testable without sleeping and keeps one
//! stopwatch definition shared by the LLM, audio.cpp, the image server and the web tools.

use crate::requests::{Outcome, Request, Service};

/// The services whose traffic belongs in `requests.tsv`, in the order a summary lists them.
///
/// Four, and only four: §10 counts the LLM, audio.cpp, the image server and the web tools as
/// requests sent outside. Local work — ffmpeg, ffprobe and the other subprocesses — is deliberately
/// absent, because §10 logs those as the commands they ran rather than as timed rows; giving them a
/// service here would put two records of the same thing in two places. Fixed order so two runs'
/// summaries line up diffably instead of following whichever service happened to finish first.
pub const OUTSIDE_SERVICES: [Service; 4] = [Service::Llm, Service::Audio, Service::Image, Service::Web];

/// A stopwatch for one request, driven by timestamps the caller supplies.
///
/// The queue wait comes in at construction because the gate measured it before this request existed
/// as a wire call (§4): a step waits for a slot, gets one, and only then does the request go out.
/// The rest are marked as they happen. No method reads a clock — pass the milliseconds your client
/// recorded and the arithmetic below is all there is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timer {
    /// Milliseconds spent waiting for a slot before going out. Zero means never queued.
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

    /// Milliseconds waiting for a slot.
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

/// Milliseconds as the file's seconds. No rounding here: [`requests::write_line`] formats two decimals,
/// and rounding twice is how a total stops matching the sum of its parts.
fn secs(ms: i64) -> f64 {
    ms as f64 / 1000.0
}

/// The row to write when a request ends.
///
/// Identity fields come from `base`; the four timing columns come from `timer`; the outcome and the
/// attempt number come from the caller. That split is deliberate: §10 requires the line to be written
/// *before the reply is used*, so a step that chokes on the content afterwards still leaves its
/// request behind. Hence this function takes the outcome as an argument and knows nothing about what
/// came back — the caller decides the outcome, writes the row, and only then looks at the body.
pub fn finish_row(
    base: &Request,
    timer: &Timer,
    outcome: Outcome,
    attempt: u32,
) -> Request {
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

/// The row for a reply served from the cache.
///
/// §10: "A reply served from the cache is a line too, marked as such with no time on the wire, so
/// the file shows what a re-run saved." Every timing column is left empty rather than zero — an
/// empty field says *this never went out*, `0.00` would say it went out and came back instantly, and
/// the difference is the whole point of counting cache hits. There is no queue wait either: a hit is
/// answered before the gate (§6's cache rule), so it never held a slot.
///
/// What the cache does know — the size and token counts it stored with the answer — stays on the row,
/// because those describe the reply rather than the trip.
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
///
/// A retry is a line of its own (§10), never an edit of the previous one, so the count comes from
/// the last row of the same request rather than being tracked somewhere that can drift. `None` is a
/// first try: attempts start at 1, so a missing number and a zero are not the same thing.
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
    /// Seconds spent waiting for a slot.
    pub wait_s: f64,
    /// How many of the rows were served from the cache.
    pub cached: usize,
}

/// Group rows by service, in [`OUTSIDE_SERVICES`]'s order, keeping only services that appear.
///
/// A service with no rows contributes no entry rather than a zero one: a run that never touched the
/// image server has nothing to report about it, and `image 0 in 0s` would read as a failed batch
/// rather than an absent one.
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
///
/// [`requests::write_line`] gets this by serialising the enum and trimming the quotes; a summary line
/// needs the same word without going through JSON, and both must agree or the log names a service the
/// file does not. `Service`'s own serde spelling is the source of truth here — if that ever changes,
/// this follows it rather than carrying a second list.
fn service_name(service: Service) -> String {
    serde_json::to_string(&service)
        .expect("a service name is a word")
        .trim_matches('"')
        .to_string()
}

/// A duration as the closing line spells it: `2h 13m`, `6m 10s`, `12s`.
///
/// This exists rather than reusing [`crate::llm_liveness::duration_spelling`] because that one stops
/// at minutes and always prints the seconds (`6m0s`), while §10's example carries hours with no
/// seconds (`2h 13m`) and a bare minute of waiting with no seconds either (`4m`). Below an hour,
/// whole minutes drop their zero seconds here and sub-minute values stay bare seconds; rounding to
/// whole seconds happens once, at the end.
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
///
/// §10 spells it: `>>> requests: llm 57 in 2h 13m on the wire, 4m waiting for a slot; audio 38 in
/// 6m 10s; web 3 in 12s`. Note which parts are said once and which repeat: "on the wire" rides only
/// on the first clause of the sentence — repeating it per service would triple the words for no
/// added information — while each service does carry its own count and its own duration. The waiting
/// clause appears only where there was some waiting, because a service nobody queued behind has
/// nothing to say about slots and printing `0s waiting for a slot` everywhere buries the one number
/// that matters.
///
/// Nothing outside was used → `">>> requests: none"`: a run that sent no request anywhere should
/// read as that rather than as an empty sentence after the prefix.
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
                line.push_str(&format!(", {} waiting for a slot", spell(tally.wait_s)));
            }
            line
        })
        .collect();
    format!(">>> requests: {}", parts.join("; "))
}

/// The share of a service's time that went on waiting for a slot rather than working.
///
/// `wait / (wait + on_wire)`. `None` when both are zero: with no time spent either way there is
/// nothing to judge, and reporting `0.0` would read as "no waiting" rather than "no data".
///
/// This is the number §10 says the slot counts are chosen by — the comparison is here, the threshold
/// is not. The spec names the ratio and the decision but gives no figure, and inventing one under a
/// `P.*` id would catalogue a value the spec never set. Keeping the threshold out means there is only
/// ever one place it can live once someone picks it.
pub fn wait_share(tally: &ServiceTally) -> Option<f64> {
    let total = tally.wait_s + tally.on_wire_s;
    if total <= 0.0 {
        return None;
    }
    Some(tally.wait_s / total)
}

/// Whether a wait share crosses the threshold the caller considers worth another slot for.
///
/// Strictly greater: a share exactly at the threshold is advice, not a demand. See
/// [`wait_share`] for why the number arrives as an argument.
pub fn wants_more_slots(share: f64, threshold: f64) -> bool {
    share > threshold
}
