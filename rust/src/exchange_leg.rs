//! The caller [`crate::exchanges`] never had: the door that turns a chat call's request body and
//! reply into one §7 exchange, files it in the run's page, and hands the app-log lines to whoever is
//! logging.
//!
//! It lives beside the leg rather than inside `exchanges` for the same reason `server_leg` sits apart
//! from `requests`: `exchanges` owns the FORMAT (page name, meta line, sections, log wording) and must
//! stay free of any socket or wire type; this module owns the reading of a real request and reply and
//! the keeping of the live page for the session. Everything it returns is already-worded text —
//! recording can never fail the call it records (§7), so nothing here hands back an error.

use serde_json::Value;

use crate::exchanges::{self, Call, Message, Mode, Part, RunLog, ToolCall};
use crate::layout::Tree;
use crate::llm_request;

/// Read one chat call out of what went out and what came back.
///
/// The request half comes from the JSON body the program built (`llm_request::body`): the mode is the
/// thinking switch's own flag, so the page says which job the model was doing by the same fact that
/// chose its temperature. The reply half is read through [`llm_request::reply_from`], the door every
/// chat route already uses, rather than re-parsed here: two readers would be two dialects of one
/// server's answer, and the cut-off rule would end up living twice.
///
/// A reply that is not JSON at all (a refused 400 whose body is prose, a proxy's HTML) still produces a
/// call: its `reply` is the body as it came, so the page shows what the server actually said rather than
/// an empty answer that looks like a model with nothing to say.
pub fn call_of(
    step: &str,
    model: &str,
    request_body: &str,
    reply_body: &str,
    content_type: &str,
    took_secs: u64,
    error: Option<String>,
) -> Call {
    let asked: Value = serde_json::from_str(request_body).unwrap_or(Value::Null);
    // `enable_thinking` appears twice in a real body (top level and `chat_template_kwargs`) and both
    // copies always agree, so either answers the question. A body with no switch at all is execute.
    let thinking = asked
        .get("enable_thinking")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let messages = asked
        .get("messages")
        .and_then(Value::as_array)
        .map(|list| list.iter().filter_map(message_of).collect())
        .unwrap_or_default();
    // Tool calls the model asked for come back on the REPLY; the results the app answered them with are
    // separate `role: "tool"` messages on the next request, so a single call's page shows the asks with
    // empty results and the following section shows the answers. Pairing them across requests would
    // mean guessing which later call belongs to this one, which is not what the page is for.
    let reply = llm_request::reply_from(content_type, reply_body).ok();
    let mut tool_calls: Vec<ToolCall> = reply.as_ref().map(calls_of).unwrap_or_default();
    if tool_calls.is_empty() {
        tool_calls = asked_tool_results(asked_messages(&asked));
    }
    let answered = match content_type {
        // A call that got no reply at all has nothing to read.
        "" => llm_request::Reply::default(),
        // An event stream whose chunks never carried a `finish_reason` still has an answer: the
        // reassembled deltas. `reply_from` refuses to call non-JSON a reply, which is right for a step
        // parsing words it must trust, wrong for a page whose job is to show what the model said.
        _ if llm_request::is_event_stream(content_type) => {
            let lines: Vec<&str> = reply_body.lines().collect();
            llm_request::from_events(&lines).reply
        }
        _ => match llm_request::reply_from(content_type, reply_body) {
            Ok(reply) => reply,
            // Not a chat answer at all (a refusal in prose, a proxy's HTML): keep the server's own words as
            // the reply so the page can show them.
            Err(_) => llm_request::Reply {
                content: reply_body.to_string(),
                ..Default::default()
            },
        },
    };
    let cut_off = answered.cut_off();
    Call {
        step: step.to_string(),
        model: model.to_string(),
        mode: if thinking {
            Mode::Thinking
        } else {
            Mode::Execute
        },
        took_secs,
        // The row knows the reasoning split only when the server reported it; §8 prints it when known.
        thinking_secs: None,
        messages,
        reply: answered.content.clone(),
        reasoning: (!answered.reasoning.is_empty()).then(|| answered.reasoning.clone()),
        tool_calls,
        cut_off,
        error,
    }
}

/// One message of a request body as the page shows it. A plain string content is one text part; an
/// array carries typed parts, and an `image_url` becomes [`Part::Image`] because that variant already
/// holds the data URL the wire sent — so the page shows exactly the picture the model saw.
fn message_of(value: &Value) -> Option<Message> {
    let role = value.get("role")?.as_str()?.to_string();
    let parts = match value.get("content")? {
        Value::String(text) => vec![Part::Text(text.clone())],
        Value::Array(list) => list
            .iter()
            .filter_map(|part| match part.get("type").and_then(Value::as_str) {
                Some("text") => part
                    .get("text")
                    .and_then(Value::as_str)
                    .map(|t| Part::Text(t.to_string())),
                Some("image_url") => part
                    .get("image_url")
                    .and_then(|url| url.get("url"))
                    .and_then(Value::as_str)
                    .map(|url| Part::Image(url.to_string())),
                _ => None,
            })
            .collect(),
        _ => return None,
    };
    Some(Message { role, parts })
}

/// The messages array of a request body, empty when it holds none.
fn asked_messages(asked: &Value) -> &[Value] {
    asked
        .get("messages")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
}

/// The tool results sitting in a request body, for a reply that named no calls of its own (a retried
/// ask whose answer arrived as an event stream without the reassembled calls).
///
/// # Why the parts are joined and not just read as a string
///
/// A `role: "tool"` message's content reaches here in whichever shape the caller sent it, and this app sends
/// both spellings: a plain string from a leg that only had words, and a typed part list from one built with
/// [`crate::llm_request::message_parts`]. Reading only the string form silently filed every part-list result
/// as EMPTY, which the page then called "answered nothing" — so a refused call and a full answer looked alike.
/// Joining the text of every part is what makes the two spellings mean the same thing here.
fn asked_tool_results(messages: &[Value]) -> Vec<exchanges::ToolCall> {
    messages
        .iter()
        .filter(|message| message.get("role").and_then(Value::as_str) == Some("tool"))
        .map(|message| exchanges::ToolCall {
            name: message
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            args: String::new(),
            result: message_content(message),
        })
        .collect()
}

/// One request message's text, however its content was spelled: a string, or the text of each typed part.
fn message_content(message: &Value) -> String {
    match message.get("content") {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(parts)) => parts
            .iter()
            .filter(|part| part.get("type").and_then(Value::as_str) != Some("image_url"))
            .filter_map(|part| part.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

/// The call's tool traffic, one entry each, in the order the model asked for them.
fn calls_of(answered: &llm_request::Reply) -> Vec<ToolCall> {
    answered
        .tool_calls
        .iter()
        .map(|asked| ToolCall {
            name: asked.name.clone(),
            args: asked.args.clone(),
            result: String::new(),
        })
        .collect()
}

/// §7's wording for one call on the page line, with its outcome when it has one. A tool that answered normally
/// reads exactly as [`crate::tool_loop::call_note`] spells it; one that refused or came back empty says so, so a
/// reader scanning the reply need not open the block to learn the call was useless.
///
/// # Why it lives here and not inside [`crate::tool_loop::call_note`]
///
/// `call_note` is the scrolling log's wording and the log has no outcome to show: it is written at the moment the
/// tool answers, by a caller that has the result but is printing one line. The page prints the ask and its result
/// together, so the verdict belongs to the page's module pair, not to the shared note. Both readings of a result
/// come from [`exchanges::result_outcome`], so the line above a block and the note under it cannot disagree.
pub fn call_line(call: &ToolCall) -> String {
    match exchanges::result_outcome(&call.result) {
        Some(outcome) => format!("{} — {outcome}", crate::tool_loop::call_note(call)),
        None => crate::tool_loop::call_note(call),
    }
}

/// One round of a tool conversation as §7's page shows it: the messages the model was shown, what it answered,
/// and the calls it made — each carrying the result that answered it.
///
/// This is the door a step that runs §2's loop hands to the recorder ([`crate::clip_rules_tools_leg`] files every
/// round through here). Without it a tool round reached the page only through [`call_of`], which reads a reply's
/// calls with BLANK results and never sees the answers, so every ask on a live page read `answered nothing` and
/// the page never said which call had really been used.
pub fn round_of(
    step: &str,
    model: &str,
    messages: &[Message],
    turn: &crate::tool_loop::Turn,
) -> Call {
    Call {
        step: step.to_string(),
        model: model.to_string(),
        mode: Mode::Execute,
        took_secs: 0,
        thinking_secs: None,
        messages: messages.to_vec(),
        reply: turn.content.clone(),
        reasoning: None,
        tool_calls: turn.calls.clone(),
        cut_off: false,
        error: None,
    }
}

/// The page stamp §7 names the file with: `MMDD-HHMMSS`, month first.
///
/// Built from the epoch the way [`crate::requests::format_started`] is (no date-time dependency), but
/// in the six-digit-per-field shape §7 spells rather than that function's sortable ISO form, so it is
/// written here once and shared by every caller.
fn stamp() -> String {
    let now = crate::server_leg::epoch_ms();
    let local = now + i64::from(crate::server_leg::local_offset_secs()) * 1000;
    let (days, millis) = (local.div_euclid(86_400_000), local.rem_euclid(86_400_000));
    // Howard Hinnant's civil-from-days, the same algorithm `requests` uses, so the two stamps cannot
    // disagree about what day it is.
    let days = days + 719_468;
    let era = if days >= 0 { days } else { days - 146_096 } / 146_097;
    let day_of_era = days - era * 146_097;
    let year_of_day =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_day + era * 400;
    let day_of_year = day_of_era - (365 * year_of_day + year_of_day / 4 - day_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * shifted_month + 2) / 5 + 1;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    };
    let year = if month <= 2 { year + 1 } else { year };
    let (hours, rest) = ((millis / 3_600_000) as u32, millis % 3_600_000);
    let (minutes, seconds) = ((rest / 60_000) as u32, (rest % 60_000) / 1000);
    format!("{month:02}{day:02}{hours:02}{minutes:02}{seconds:02}{year:04}")
}

/// The page the CALLING THREAD's run is writing, open from its first call and closed when the next run starts.
///
/// This holder is THE run's page, not a second bookkeeping copy: `start_run` closes it (§7's "a run
/// is the calls between two queue resets") and every chat call files itself here, so a page holds
/// every call of the run that opened it whatever step or thread made them.
///
/// Each thread gets its own `Arc<Mutex<RunLog>>`, and both halves of that matter:
///
/// - **per-thread** because a run's page belongs to one run. A single shared holder would be used by every
///   thread in the process — including the unrelated runs cargo's test threads drive at the same time — and
///   their sections would interleave into one file. So each thread is given its OWN `Arc` on first use, and
///   that is the only page it ever writes.
/// - **`Arc`** because §4 puts the socket read on a detached worker (`llm_watch_leg::guarded`). [`Live::hook`]
///   is built on the thread that started the run, so it can clone that thread's `Arc` and carry the very page
///   its caller opened across to the worker. A plain `RefCell<RunLog>` would hand the worker an empty page of
///   its own: request filed on one thread, words landing nowhere — the silent failure this item exists to stop.
///
/// The registry is process-wide ONLY as a factory handing each thread its own page; no two runs share a
/// `RunLog`. It is a map rather than a `thread_local!` because a `thread_local!` cannot be borrowed out of
/// inside a closure that has to return the value.
static RUN: std::sync::LazyLock<
    std::sync::Mutex<
        std::collections::HashMap<std::thread::ThreadId, std::sync::Arc<std::sync::Mutex<RunLog>>>,
    >,
> = std::sync::LazyLock::new(Default::default);

/// This thread's run page, ready to clone into a hook. Each recorder locks it for ONE operation and never
/// holds the lock across a `log(...)` call — the log sink is the caller's own thread-local list, and a lock
/// carried into it would let one run's write wait on another's logging.
fn page() -> std::sync::Arc<std::sync::Mutex<RunLog>> {
    let me = std::thread::current().id();
    let mut held = match RUN.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    held.entry(me)
        .or_insert_with(|| std::sync::Arc::new(std::sync::Mutex::new(RunLog::new())))
        .clone()
}

/// Lock the page for one recorder call. Poisoned is taken as-is: what a panic can leave held here is a page of
/// already-written text, and losing an exchange page is not worth failing the call recording it (§7).
fn write_to(run: &std::sync::Arc<std::sync::Mutex<RunLog>>) -> std::sync::MutexGuard<'_, RunLog> {
    match run.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

/// The live half of one streamed chat call, in the shape the dialling leg can actually hand over.
///
/// # Why it has to exist at all
///
/// §7's live rule is that the request goes on the page BEFORE the answer arrives and the reply grows as
/// chunks land. A leg cannot do that with [`record`] alone: `record` files a finished exchange, which means
/// the reader sees nothing until the call ends — exactly what "the page fills up while you wait" says must
/// not happen. So the dial door asks for a sink ([`crate::server_leg::Dial::delta`]) and this type owns
/// everything about it: when to open the section, how to turn raw SSE bytes into words, and how to close it.
///
/// # Why the hook is asked for per attempt
///
/// The dial loop retries, and each attempt that goes out owes its own section to the page (§7: two calls, two
/// sections). A caller that handed over one plain closure could not open a fresh section per trip; asking the
/// leg to call [`Live::hook`] immediately before each send makes it state what it already knows — which attempt
/// this is — and keeps the identity of the section (model, mode, messages) inside this module.
///
/// # What the hook does NOT do
///
/// It never fails a call. Decoding, writing and closing all swallow their own trouble (§7's last sentence).
/// # Why the hook needs no token and no ownership check
///
/// A call §4 gave up on keeps producing bytes on its detached worker after its own dial has returned, so a hook
/// can outlive its section. That is already harmless: [`RunLog::stream`] grows only an OPEN section, and
/// [`Live::finish`] closes the section before the dial returns, so a late write finds nothing open and does
/// nothing. No second mechanism is needed to make a stale hook safe.
pub struct Live {
    tree: Tree,
    step: String,
    model: String,
    body: String,
    /// The page THIS call grows. A clone of the calling thread's holder, taken while `Live` is built — that is
    /// what lets [`Live::hook`] hand the same page to §4's reader thread (see [`RUN`]).
    run: std::sync::Arc<std::sync::Mutex<RunLog>>,
    /// Everything this call streamed, kept for the closing rewrite (see [`Live::streamed_words`]).
    streamed: std::sync::Arc<std::sync::Mutex<String>>,
    /// Set when this attempt ends — by [`Live::close_attempt`] or [`Live::finish`] — so a §4 give-up's
    /// abandoned reader thread stops writing: its late words would land inside the NEXT attempt's section.
    closed: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// Whether THIS run owns the page the live section went into. Read at open time: a call made outside a run
    /// gets a page that ends with itself, the same rule [`record`] applies, and the flag says so at the close.
    owned_run: std::cell::Cell<bool>,
}

/// The hook [`crate::server_leg::Dial::delta`] carries: fed every byte of the body received so far, after
/// each read. `Send + Sync` because §4's watched send reads on its detached worker thread.
pub type ServerHook = std::sync::Arc<dyn Fn(&[u8]) + Send + Sync>;

/// Whether one request body asked for a streamed reply. Read from the JSON actually going out rather than
/// passed in as a flag, so the page cannot claim a call was live when the body says it was not — and so a
/// caller that never thinks about §7 still gets the right answer, because the body is the fact.
fn stream_asked(body: &str) -> bool {
    serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|asked| asked.get("stream").and_then(Value::as_bool))
        .unwrap_or(false)
}

impl Live {
    /// Open the live recorder for one chat ask. `body` is the exact JSON going out — the same string the leg
    /// sends — because the page's request half and its thinking-mode label are both read from it.
    pub fn new(tree: &Tree, step: &str, model: &str, body: &str) -> Self {
        Self {
            tree: tree.clone(),
            step: step.to_string(),
            model: model.to_string(),
            body: body.to_string(),
            run: page(),
            streamed: std::sync::Arc::new(std::sync::Mutex::new(String::new())),
            closed: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            owned_run: std::cell::Cell::new(false),
        }
    }

    /// Whether this request body deserves a live page at all: only a body that ASKED to be streamed
    /// (`"stream": true`, §9 §1) has chunks to show. A one-shot ask has no incremental reply, so filing it at
    /// the end through [`record`] is the whole of what §7 asks for it.
    pub fn asked_for_live(&self) -> bool {
        stream_asked(&self.body)
    }

    /// File this ask's REQUEST on the run's page, open its section, and announce it through `log` (the send
    /// line, plus the once-per-run link if this is the run's first call). Must be called before [`Live::hook`]
    /// can grow anything, and once per attempt: each retry re-opens its own section, which is why the leg calls
    /// this and [`Live::hook`] together rather than once per ask.
    pub fn opened(&self, log: &dyn Fn(&str)) {
        self.owned_run.set(run_open());
        let call = call_of(&self.step, &self.model, &self.body, "", "", 0, None);
        let stamp = stamp();
        let lines = write_to(&self.run).begin(&self.tree, &stamp, &self.step, &call);
        for line in lines {
            log(&line);
        }
    }

    /// File this ask's REQUEST on the run's page, open its section, and hand back the hook that grows it —
    /// [`Live::opened`] and [`Live::hook`] in the one call the dialling leg makes.
    ///
    /// Returns `None` (and files nothing) for a body with no stream to show, which is how a caller gets "send
    /// without a live page" for free: pass the result straight to [`crate::server_leg::Dial::delta`].
    pub fn opened_then_hook(&self, log: &dyn Fn(&str)) -> Option<ServerHook> {
        if !self.asked_for_live() {
            return None;
        }
        self.opened(log);
        self.hook()
    }

    /// The hook that grows the section [`Live::opened`] put up, or `None` when this body gets no live page.
    pub fn hook(&self) -> Option<ServerHook> {
        if !self.asked_for_live() {
            return None;
        }
        // One decoder per attempt, shared between this closure and the socket thread's reads. Its mutex is held
        // only for the decode, never across a write: two threads may reach one hook only if a retry overlapped a
        // late read, and then the worst outcome is a fragment written twice, not a deadlock in the read loop.
        let held = std::sync::Arc::new(std::sync::Mutex::new((
            crate::llm_stream_leg::Delta::default(),
            0usize,
        )));
        // The page travels by clone, which is what lets a hook built here reach §4's worker thread at all.
        // The page itself is a mutex for exactly that reason (see `write_to`), so the words go straight onto it
        // from wherever they are decoded: waiting for the caller's thread to notice them would keep the section
        // at `reply pending` until the call ended — the failure §7's live rule forbids.
        let run = self.run.clone();
        // Every word this attempt produced, kept for [`Live::finish`]: the closing rewrite prints the answer,
        // not the wire, and it can only do that if someone remembered what the reader decoded.
        let seen = self.streamed.clone();
        // This attempt's own stop flag: a §4 give-up leaves the reader thread running (see `guarded`), and its
        // late words belong to a section the page has already closed.
        let closed = self.closed.clone();
        Some(std::sync::Arc::new(move |whole: &[u8]| {
            if closed.load(std::sync::atomic::Ordering::Relaxed) {
                return;
            }
            let fresh = {
                let mut guard = match held.lock() {
                    Ok(guard) => guard,
                    Err(poisoned) => poisoned.into_inner(),
                };
                let (decoder, read_so_far) = &mut *guard;
                let fresh = decoder.push(&whole[*read_so_far..]);
                *read_so_far = whole.len();
                fresh
            };
            if fresh.is_empty() {
                return;
            }
            if let Ok(mut words) = seen.lock() {
                words.push_str(&fresh);
            }
            let _ = write_to(&run).stream(&fresh);
        }))
    }

    /// This attempt is over: its hook must write nothing more. The leg calls this the moment an attempt
    /// returns — success, refusal or §4 give-up — because a given-up call's reader thread is still alive
    /// (`guarded` detaches it on purpose) and may keep decoding into this closure.
    pub fn close_attempt(&self) {
        self.closed
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }

    /// Every word this call streamed, in arrival order — what the reader watched appear.
    pub fn streamed_words(&self) -> String {
        match self.streamed.lock() {
            Ok(seen) => seen.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        }
    }

    /// The call [`Live::hook`] was growing is over: rewrite its section from the finished answer and hand the
    /// verdict lines to `log`. Every dial ending calls this, including the ones with no reply — a call the
    /// server never answered is the one a reader opens the page to find.
    ///
    /// With nothing open this still files the exchange whole (see [`RunLog::complete`]), which is what lets the
    /// leg drive one path whether or not the attempt ever got a hook.
    pub fn finish(
        &self,
        reply_body: &str,
        content_type: &str,
        took_secs: u64,
        error: Option<String>,
        log: &dyn Fn(&str),
    ) {
        // Nothing from this attempt may touch the page again once its finished section is written.
        self.close_attempt();
        let call = call_of(
            &self.step,
            &self.model,
            &self.body,
            reply_body,
            content_type,
            took_secs,
            error,
        );
        let stamp = stamp();
        let lines = {
            let mut run = write_to(&self.run);
            let lines = run.complete(&self.tree, &stamp, &self.step, &call);
            // A call made outside a run keeps the rule [`record`] applies: its page ends with itself, so the
            // next run's calls cannot soak into a page nobody opened for them.
            if !self.owned_run.get() {
                run.reset();
            }
            lines
        };
        for line in lines {
            log(&line);
        }
    }
}

/// File one call in the run's page and hand every line it produced to `log` — the two per-call lines
/// plus, on the run's first call, the once-per-run link (§7). Returns nothing: a page that could not
/// be written is already worded as a line by `exchanges`, and failing the call it records is not its
/// job (§7's last sentence).
///
/// # Two doors for one page, and which calls which
///
/// A non-streamed exchange (every audio/image/upload leg, and any chat ask whose body did not ask for a
/// stream) has no incremental reply to show, so this whole-file door is all §7 asks of it. A STREAMED chat
/// call instead walks [`Live::opened`] → [`Live::hook`] → [`Live::finish`], because filing it here would mean
/// the reader sees nothing until the answer is finished — what "the page fills up while you wait" forbids.
/// [`Live::finish`] keeps this function's ending as its fallback, so a streamed call that never got a hook
/// still lands on the page.
pub fn record(tree: &Tree, step: &str, call: &Call, log: &dyn Fn(&str)) {
    let stamp = stamp();
    let lines = {
        let run = page();
        let mut run = write_to(&run);
        // Only the FIRST call of a page owes the once-per-run link, and `record` hands it out with the
        // send line. Closing the page afterwards retires this call's own boundary (§7: a run is the
        // calls between two resets), so a call with no run open gets a page of its own instead of
        // soaking up the next run's calls into it.
        let open = run_open();
        let lines = run.record(tree, &stamp, step, call);
        if !open {
            run.reset();
        }
        lines
    };
    for line in lines {
        log(&line);
    }
}

/// Test seam: end the open page so a later check of this binary does not inherit it. The program's own
/// boundary is [`note_run_started`], which closes the previous run's page from `runqueue::start_run`.
pub fn reset_run_for_test() -> Option<String> {
    RUN_OPEN.with(|cell| cell.set(false));
    write_to(&page()).reset()
}

thread_local! {
    /// Whether a run stands, and WHY it stays thread-local while [`RUN`] does not: only the thread that owns
    /// the run raises or reads it (`runqueue::start_run`/`end_run`), whereas the page is written by whichever
    /// thread holds the socket. A call found outside a run is still recorded, in a page that ends with
    /// itself: §7 would rather keep a stray exchange than drop it.
    static RUN_OPEN: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Is a run open right now? Read by [`record`] to decide whether the page it just opened belongs to a
/// run or stands alone.
fn run_open() -> bool {
    RUN_OPEN.with(|cell| cell.get())
}

/// The open run's page name — §10's `run` column — or empty when this thread is not inside a run whose
/// page has been opened yet.
///
/// A call made outside a run still gets its own page (§7 keeps a stray exchange rather than dropping it),
/// but that page is not what §10 means: the column says "the run it belongs to", so an unattached call
/// leaves it blank. The name arrives only once the run's first call has opened the page, which is why a
/// leg dialling before any chat ask sees empty here — and writes empty, exactly as §10 spells it.
pub fn current_page_name() -> String {
    if !run_open() {
        return String::new();
    }
    let held_page = page();
    let held = write_to(&held_page);
    held.page_rel().unwrap_or_default()
}

/// Note for the recorder that a run has begun (§7's page boundary): the previous run's page is closed,
/// so this run's first call opens its own, and the return says whether anything was closed. Called
/// from [`crate::runqueue::start_run`], the one call every ▶ and ↻ makes, so no flow has to remember
/// it.
pub fn note_run_started() -> bool {
    RUN_OPEN.with(|cell| cell.set(true));
    write_to(&page()).reset().is_some()
}

/// Note for the recorder that the run ended, so a later stray call does not land on a finished run's
/// page. The page itself stays on disk — closing the bookkeeping does not retire the record. Called from
/// [`crate::runqueue::end_run`], where `RunBar::running` goes back to `None`.
pub fn note_run_ended() {
    RUN_OPEN.with(|cell| cell.set(false));
}
