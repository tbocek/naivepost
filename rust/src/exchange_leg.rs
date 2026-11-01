//! The caller [`crate::exchanges`] never had: the door that turns a chat call's request body and
//! reply into one §7 exchange, files it in the run's page, and hands the app-log lines to whoever is
//! logging.

use serde_json::Value;

use crate::exchanges::{self, Call, Message, Mode, Part, RunLog, ToolCall};
use crate::layout::Tree;
use crate::llm_request;

/// The content type [`call_of`] takes for a reply that is the model's words rather than a server's body.
pub const WORDS_AS_RECEIVED: &str = "text/plain";

/// Read one chat call out of what went out and what came back.
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
    let reply = llm_request::reply_from(content_type, reply_body).ok();
    let mut tool_calls: Vec<ToolCall> = reply.as_ref().map(calls_of).unwrap_or_default();
    if tool_calls.is_empty() {
        tool_calls = asked_tool_results(asked_messages(&asked));
    }
    let answered = match content_type {
        // A call that got no reply at all has nothing to read.
        "" => llm_request::Reply::default(),
        // Words a stream already gave up: they are the answer as they stand. Read as a chat reply, an answer that is
        // itself JSON ({"drop_before":…}) parsed as an envelope with no choices, and the page said it answered nothing.
        WORDS_AS_RECEIVED => llm_request::Reply {
            content: reply_body.to_string(),
            ..Default::default()
        },
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
pub fn call_line(call: &ToolCall) -> String {
    match exchanges::result_outcome(&call.result) {
        Some(outcome) => format!("{} — {outcome}", crate::tool_loop::call_note(call)),
        None => crate::tool_loop::call_note(call),
    }
}

/// One round of a tool conversation as §7's page shows it: the messages the model was shown, what it answered,
/// and the calls it made — each carrying the result that answered it.
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
    let tag = PAGE_TAG.with(|tag| tag.borrow().clone());
    format!("{month:02}{day:02}{hours:02}{minutes:02}{seconds:02}{year:04}{tag}")
}

/// The page the CALLING THREAD's run is writing, open from its first call and closed when the next run starts.
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

pub fn record(tree: &Tree, step: &str, call: &Call, log: &dyn Fn(&str)) {
    let stamp = stamp();
    let lines = {
        let run = page();
        let mut run = write_to(&run);
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
    static RUN_OPEN: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// What a row running beside others adds to its page's name, so two rows starting in one second keep two pages.
    static PAGE_TAG: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
}

/// A worker thread's place in the run's record, given back when it is dropped. A page takes one open call at a time,
/// so calls on the wire together cannot share one: each row running beside others keeps a page of its own.
pub struct RowPage;

impl RowPage {
    /// This thread's calls go on a new page of the open run, its name carrying `tag`.
    pub fn open(tag: &str) -> Self {
        RUN_OPEN.with(|cell| cell.set(true));
        PAGE_TAG.with(|held| *held.borrow_mut() = format!("-{tag}"));
        Self
    }
}

impl Drop for RowPage {
    fn drop(&mut self) {
        RUN_OPEN.with(|cell| cell.set(false));
        PAGE_TAG.with(|held| held.borrow_mut().clear());
        if let Ok(mut held) = RUN.lock() {
            held.remove(&std::thread::current().id());
        }
    }
}

/// The row page this thread writes, for a helper thread the row dials through; `None` outside a row.
pub fn row_page_to_share() -> Option<(std::sync::Arc<std::sync::Mutex<RunLog>>, String)> {
    let tag = PAGE_TAG.with(|tag| tag.borrow().clone());
    (!tag.is_empty()).then(|| (page(), tag))
}

/// A helper thread writes `shared`, its row's page, until the answer is dropped. The row waits for it, so their calls never overlap.
pub fn share_row_page(
    shared: Option<(std::sync::Arc<std::sync::Mutex<RunLog>>, String)>,
) -> Option<RowPage> {
    let (run, tag) = shared?;
    if let Ok(mut held) = RUN.lock() {
        held.insert(std::thread::current().id(), run);
    }
    RUN_OPEN.with(|cell| cell.set(true));
    PAGE_TAG.with(|held| *held.borrow_mut() = tag);
    Some(RowPage)
}

/// Is a run open right now? Read by [`record`] to decide whether the page it just opened belongs to a
/// run or stands alone.
fn run_open() -> bool {
    RUN_OPEN.with(|cell| cell.get())
}

/// The open run's page name — §10's `run` column — or empty when this thread is not inside a run whose
/// page has been opened yet.
pub fn current_page_name() -> String {
    if !run_open() {
        return String::new();
    }
    let held_page = page();
    let held = write_to(&held_page);
    held.page_rel().unwrap_or_default()
}

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
