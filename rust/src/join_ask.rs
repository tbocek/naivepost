//! F1.10 S3's live half: the ask itself.
//!
//! [`crate::joins`] owns every rule that settles with no server — which seams exist, what a reply
//! means, the ceilings, the marks and both files. What it never did was *telephone*: on a session
//! with seams the pass stopped at its own brief line, so the answers that decide the cut arrived
//! from nobody. This module is that leg. It builds the two messages §F1.10 spells, dials the LLM
//! over HTTP through the same [`crate::llm_retry_leg`] every chat ask rides (which is [`crate::server_leg`]
//! plus §3's patience), and hands the reply
//! to [`crate::joins::answer_join`] so the tool's ceilings — not a copy of them — decide what
//! may go.
//!
//! The seam a test drives this through is the settings address: point `LLM_SERVER` (or the
//! `LLM_SERVER` line of `llm.conf`) at a local fake and the very same endpoint resolution a
//! typed address goes through reaches it. Nothing here takes a canned reply, and a server that is
//! down answers `Err` naming it, which the pass carries as a refusal — "A refusal still keeps the
//! stumble" (§F1.10), so the video keeps the stumble rather than losing words to a guess.
//!
//! Why the wording arrives as an argument to the builders rather than as something hunted for:
//! the shipped text lives in `spec/prompts/<key>.md` and is read through
//! [`crate::prompts::shipped_text`], so — exactly as [`crate::narrate_call`] does — the door reads
//! the two wordings from [`crate::settings::prompt_text`] with that shipped wording as the fallback,
//! which is what the settings box itself shows when this machine holds no edit.
//!
//! could not be read either way, and new code goes in a new file in a tree this size.

use crate::cancel_leg;
use crate::exchanges::{Message, Part};
use crate::joins::{self, Outcome};
use crate::layout::Tree;
use crate::llm_cache;
use crate::llm_request;
use crate::project::Project;
use crate::requests::{Request, Service};
use crate::seam_retries::{self, Asking};
use crate::services;
use crate::settings::{self, Conf, Paths};
use crate::tools::textedit::{Join, SEAM_REACH_WORDS};
use crate::word_list::{self, Word};

/// The shipped wording of one prompt key: the fenced ``` block of `spec/prompts/<key>.md`.
/// The reader moved to [`crate::prompts::shipped_text`] once the Prepare bench started showing the
/// same wording; this stays as the name every ask module already dials through.
pub fn shipped_file(key: &str) -> Option<String> {
    crate::prompts::shipped_text(key)
}

/// The wording to ask with, through the settings read path so an edit here wins (§04-prepare#6).
/// With no settings folder at all, the shipped wording is what goes out.
fn wording(paths: Option<&Paths>, key: &str) -> String {
    let fallback = shipped_file(key).unwrap_or_default();
    match paths {
        Some(paths) => settings::prompt_text(paths, key, &fallback).unwrap_or(fallback),
        None => fallback,
    }
}

/// The first ``` fenced block of a markdown file: [`crate::prompts::fenced_block`], re-exported
/// under the name this module's callers learned first.
pub use crate::prompts::fenced_block;

/// The settings folder, read from this process's environment each time an ask is built so a change
/// made while a run is going is honoured by its next request (§02-services#1: settings are
/// re-read per request).
fn current_paths() -> Option<Paths> {
    settings::from_environment()
}

/// What one seam's ask came to, and whether it is finished asking.
#[derive(Debug, Clone, PartialEq)]
pub struct Answered {
    /// What the answer means, in the pass's own terms.
    pub outcome: Outcome,
    /// Whether this seam may be asked again. `false` only when a retry is still worth making.
    pub settled: bool,
    /// The answer text, so the caller can log it as it arrived. Empty when nothing was answered.
    pub reply: String,
    /// The ask failed at the server rather than at the answer, so the join is NOT asked again:
    /// a down server answers the same way however often it is called, and `seam_retries` counts
    /// refusals of answers, not failures of the wire.
    pub failed: bool,
    /// Whether this answer came back from `cache/llm/textedit` instead of the model. §6's summary line
    /// ("N join(s) repaired, M from the cache") counts exactly this, so the pass cannot guess it from
    /// whether a reply arrived — a cached one arrives looking identical.
    pub from_cache: bool,
}

/// One seam's live ask: build the two messages, honour the cache, dial, read the reply.
///
/// `attempt` is which ask this is, 0-based: the first, then each retry. It selects the cache slot
/// ([`seam_retries::ask_slot`]) so a retry cannot be served by the reply that was just refused —
/// the whole reason `P.machine.seamRetries` needed a parameter round.
///
/// A cache hit returns without a socket being opened: the answer was paid for already, and §6 keys
/// on the exact request so an edited prompt misses instead.
#[allow(clippy::too_many_arguments)]
pub fn ask(
    tree: &Tree,
    user_context: &str,
    k: usize,
    n: usize,
    wide: &str,
    before: &[String],
    after: &[String],
    attempt: u32,
    refused: Option<&str>,
    cancelled: &dyn Fn() -> bool,
) -> Answered {
    let conf = match current_paths().as_ref().map(settings::read) {
        Some(Ok(conf)) => conf,
        Some(Err(reason)) => {
            // An unreadable settings file is reported the way a down server is: the pass keeps the
            // stumble and says why, rather than dialling a guessed address.
            return Answered {
                outcome: Outcome::Refused(format!("settings could not be read: {reason}")),
                settled: true,
                reply: String::new(),
                failed: true,
                from_cache: false,
            };
        }
        None => Conf::default(),
    };
    ask_at(
        tree,
        &conf,
        user_context,
        k,
        n,
        wide,
        before,
        after,
        attempt,
        refused,
        cancelled,
    )
}

/// [`ask`] with the settings already read, for a caller that resolved them itself.
#[allow(clippy::too_many_arguments)]
pub fn ask_at(
    tree: &Tree,
    conf: &Conf,
    user_context: &str,
    k: usize,
    n: usize,
    wide: &str,
    before: &[String],
    after: &[String],
    attempt: u32,
    refused: Option<&str>,
    cancelled: &dyn Fn() -> bool,
) -> Answered {
    let model = conf.model.clone();
    let system = join_system(user_context);
    let user = joins::request(user_context, k, n, wide, before, after, refused);
    let parts = seam_parts(&system, &user, &model, attempt);
    if let Some(cached) = llm_cache::read(tree, joins::STEP, &parts) {
        // §09 §10: the cached answer writes its own `cache` line, with no time on the wire; the row is
        // the record that this ask cost nothing, not a second count of the request that earned it.
        let outcome = judge(&cached, before, after);
        crate::request_timing::record_cached(tree, base_row(&model));
        return Answered {
            outcome,
            settled: true,
            reply: cached,
            failed: false,
            from_cache: true,
        };
    }
    // No model configured: asking again answers the same way, so this settles. Checked before the
    // loop dials, because a body cannot even be built without one.
    if let Err(reason) = chat_body_with(&model, &messages0(&system, &user)) {
        return Answered {
            outcome: Outcome::Refused(reason.clone()),
            settled: true,
            reply: String::new(),
            failed: true,
            from_cache: false,
        };
    }
    let base = base_row(&model);
    let endpoint = services::llm_endpoint(conf);
    let body = match chat_body_with(&model, &messages0(&system, &user)) {
        Ok(body) => body.to_string(),
        Err(reason) => {
            return Answered {
                outcome: Outcome::Refused(reason),
                settled: true,
                reply: String::new(),
                failed: true,
                from_cache: false,
            }
        }
    };
    // §3.5: one ask, no tools: the answer is the quoted words, read in one place (`joins::answer_join`). The
    // first rewrite offered drop_words/keep_join beside a prompt that asked for the joined text, and the model,
    // told two things at once, answered nothing usable at 125 of 127 asks.
    let sent = match crate::llm_retry_leg::chat(
        joins::STEP,
        tree,
        &base,
        &endpoint,
        &body,
        cancelled,
        &crate::ui::window::log_line,
    ) {
        Ok(sent) => sent,
        // A transport failure is not a refusal of an answer: the join is not retried against a server that
        // is not there, and nothing is cached.
        Err(reason) => {
            return Answered {
                outcome: Outcome::Refused(reason),
                settled: true,
                reply: String::new(),
                failed: true,
                from_cache: false,
            }
        }
    };
    let reply = answer_of(&sent.content_type, &sent.body);
    let outcome = judge(&reply, before, after);
    if outcome.usable() {
        // Cached only when usable (§F1.10): a refusal must go out again rather than be replayed from the slot
        // it failed in. What is filed is the answer itself, which the cache branch above reads through `judge`.
        llm_cache::store(tree, joins::STEP, &parts, reply.as_str());
    }
    Answered {
        outcome,
        settled: true,
        reply,
        failed: false,
        from_cache: false,
    }
}

/// Read a reply the way every chat job reads it, then let the rules already written decide.
fn judge(reply: &str, before: &[String], after: &[String]) -> Outcome {
    let mut join = Join::new(
        &before.iter().map(String::as_str).collect::<Vec<&str>>(),
        &after.iter().map(String::as_str).collect::<Vec<&str>>(),
    );
    joins::answer_join(&mut join, reply, before, after)
}

/// The body a seam's ask sends, for a test that pins its shape: thinking on, and no tools offered (§3.5).
pub fn body_for_test(shown: &[String]) -> String {
    chat_body_with("test-model", &messages0("system wording", &shown.join(" ")))
        .map(|v| v.to_string())
        .unwrap_or_default()
}

/// SYSTEM for a join call: the house rules cut to textedit's sections, the job's own wording, and
/// the precedence rule only when there is a User Context to outrank anything (§09 §8 puts it last).
fn join_system(context: &str) -> String {
    let paths = current_paths();
    crate::prompt_assembly::system_message(
        &wording(paths.as_ref(), "system"),
        "textedit",
        &wording(paths.as_ref(), "textedit"),
        context,
        false,
        false,
    )
}

/// The two messages of a join ask: the system prompt and the request.
fn messages0(system: &str, user: &str) -> Vec<Message> {
    vec![
        Message {
            role: "system".to_string(),
            parts: vec![Part::Text(system.to_string())],
        },
        Message {
            role: "user".to_string(),
            parts: vec![Part::Text(user.to_string())],
        },
    ]
}

/// The chat body of a join ask: thinking on (§4, `joins::thinking_on`), streamed, and no tools offered (§3.5).
fn chat_body_with(model: &str, conversation: &[Message]) -> Result<serde_json::Value, String> {
    // §4's "every call is streamed": the request ASKS for `stream`, and the reader routes on the RESPONSE's
    // content type (`llm_request::reply_from` / `is_event_stream`), so a server that answers plain JSON to a
    // streaming request is still read correctly. That is what lets a one-shot fake keep answering while the
    // real llama.cpp gets chunks a person can watch arrive.
    // §4 gives the text edit thinking ON; read from the prompt table (§10 §4) rather than named here.
    llm_request::body(
        joins::STEP,
        model,
        conversation,
        crate::prompts::mode_of("textedit"),
        None,
        true,
    )
}

/// The answer text out of a chat reply: `content`, never the reasoning. Routed by §1's rule — an
/// event-stream body is read as chunks, anything else as one JSON envelope.
fn answer_of(content_type: &str, reply: &str) -> String {
    llm_request::reply_from(content_type, reply)
        .map(|parsed| parsed.answer().to_string())
        // A body that is neither JSON nor a stream answers with nothing here; `judge` turns the empty
        // answer into the step's own degraded verdict, which is what it did before this door existed.
        .unwrap_or_default()
}

/// The deciding parts for one join ask, and so the cache key [`ask_at`] looks up. The retry's slot rides
/// in `state` — the texts, model and thinking flag are otherwise identical, and the slot is what keeps the
/// two asks apart (§6). Public because a caller that wants to plant or check an answer must reach the very
/// door the pass hashes: the bytes on the wire, the system prompt's wording included, decide the key, so
/// the caller passes the two finished messages rather than the seam's numbers (`joins::request` and
/// `join_system` are private, and re-deriving them here would let a planted reply drift from the ask).
pub fn seam_parts(system: &str, user: &str, model: &str, attempt: u32) -> Vec<serde_json::Value> {
    let state = seam_retries::ask_slot(system, attempt);
    let request = llm_cache::Request {
        system,
        user,
        state: &state,
        speech: "",
        context: "",
        images: Vec::new(),
        run_index: None,
        model,
        thinking: joins::thinking_on(),
    };
    llm_cache::parts(&request)
}

/// A `Request` carrying what only the caller knows, for the timed row `server_leg::call` writes.
fn base_row(model: &str) -> Request {
    Request {
        started: String::new(),
        run: String::new(),
        step: "prepare".to_string(),
        job: "textedit".to_string(),
        service: Service::Llm,
        model: model.to_string(),
        kind: "chat".to_string(),
        ..Default::default()
    }
}

/// What the whole pass came to.
#[derive(Debug, Clone, Default)]
pub struct Pass {
    /// Per word of the input list: whether it survives into `final.txt`.
    pub kept: Vec<bool>,
    /// Every line the pass spoke, in order, for the run's page.
    pub logs: Vec<String>,
    /// Words removed at the seams, before dedupe.
    pub by_seams: usize,
    /// Words removed in all, seams and dedupe together — S6's first number.
    pub removed: usize,
    /// Separate dropped runs — S6's "stretch(es)".
    pub stretches: usize,
    /// Recordings this pass took out entire, for the yellow wash on Cut.
    pub whole: Vec<String>,
}

/// The whole join pass with the wire in it: every seam asked in order, each answer matched back,
/// then S4's dedupe, S5's marks and S6's two files written together.
///
/// The BEFORE and AFTER windows are taken off the words still standing, which is what makes a
/// later seam see a clean join: showing it what an earlier seam already removed made the
/// one-stretch check count that stretch here and refuse an answer that was right (join 22 of the
/// ETH lecture, fixed 2026-09-18).
pub fn run_seams(
    tree: &Tree,
    project: &Project,
    words: &[Word],
    times: &[(f64, f64)],
    reached: &mut dyn FnMut(f64, &str),
) -> Pass {
    let total = words.len();
    let mut kept = vec![true; total];
    // What was standing when the pass began, so the whole-take flag below can tell a word this pass
    // removed from one that was already gone.
    let started_kept: Vec<bool> = kept.clone();
    let mut pass = Pass {
        kept: kept.clone(),
        ..Default::default()
    };
    if total == 0 {
        return pass;
    }
    // S1: too few joinable words to hold a join. `press_joins` has already reported the count, so
    // this says nothing rather than printing the same sentence twice.
    if joins::too_few(words) {
        return pass;
    }
    let seams = joins::seams(words);
    let n = seams.len();
    let retries = seam_retries::SEAM_RETRIES;
    let cancelled = cancel_leg::cancel_check_now();
    let mut applied = 0usize;
    let mut from_cache = 0usize;
    // Joins whose every ask ended without a usable answer: kept as recorded, and said loudly at the end, since
    // a pass that quietly kept every stumble reads exactly like one that found none.
    let mut unanswered = 0usize;
    for (index, seam) in seams.iter().enumerate() {
        let k = index + 1;
        // Each join is a thinking ask that can take minutes: say which one is asked, so the bar moves and the
        // status names it, instead of standing still for the whole pass.
        reached(
            index as f64 / n as f64,
            &format!("repairing join {k} of {n}"),
        );
        let before = window_before(words, &kept, *seam);
        // AFTER is the take that follows and stops where the next one starts: past it lies the NEXT join's
        // stumble, which the model took for this one's and cut into the good take (07-news, 2026-10-26).
        let until = seams.get(index + 1).copied().unwrap_or(words.len());
        let after = window_after(words, &kept, *seam, until);
        let wide = joins::wide_view(words, &kept, *seam, &seams, WIDE_WORDS);
        if before.is_empty() || after.is_empty() {
            // Nothing standing on one side: there is no join left to ask about here.
            continue;
        }
        let mut asking = Asking::start(retries);
        let mut last_refusal: Option<String> = None;
        loop {
            let answered = ask(
                tree,
                &project.context,
                k,
                n,
                &wide,
                &before,
                &after,
                asking.attempt,
                last_refusal.as_deref(),
                &cancelled,
            );
            match answered.outcome {
                Outcome::Applied {
                    before: off_before,
                    after: off_after,
                } => {
                    take_off_end(&mut kept, *seam, off_before);
                    take_off_start(&mut kept, *seam, off_after);
                    applied += 1;
                    if answered.from_cache {
                        from_cache += 1;
                    }
                    pass.logs.push(applied_log(k, n, off_before, off_after));
                    break;
                }
                Outcome::Kept => {
                    if answered.from_cache {
                        from_cache += 1;
                    }
                    pass.logs.push(kept_log(k, n));
                    break;
                }
                Outcome::Refused(reason) => {
                    if answered.failed {
                        pass.logs
                            .push(joins::refusal_line(k, &reason, "nothing removed there"));
                        unanswered += 1;
                        break;
                    }
                    match asking.refused() {
                        Some(tail) => pass.logs.push(joins::refusal_line(k, &reason, tail)),
                        None => {
                            pass.logs.push(joins::refusal_line(
                                k,
                                &reason,
                                "nothing removed there",
                            ));
                            unanswered += 1;
                            break;
                        }
                    }
                    // The retry is told why, so it does not answer the same way again.
                    last_refusal = Some(reason);
                }
            }
        }
    }
    if unanswered > 0 {
        pass.logs.push(unanswered_log(unanswered, n));
    }
    pass.by_seams = kept.iter().filter(|keep| !**keep).count();
    // The seconds actually spoken, which F1.11 asks for when it decides whether a retake's onset is
    // clear of speech — read off the same word list the marks came from so the two never disagree.
    let spoken: Vec<(f64, f64)> = words.iter().map(|word| (word.start, word.end)).collect();
    // S4: a saying doubled straight across a cut loses its earlier copy. The notes come back in the
    // prototype's wording (`seconds: "text" said again...`); the pass re-says each one with its own prefix
    // so every line this pass speaks is greppable as its own (§12's dedupe row: named in the log).
    let written: Vec<String> = words.iter().map(|word| word.written.clone()).collect();
    let sources: Vec<String> = words.iter().map(|word| word.source.clone()).collect();
    for note in joins::dedupe_across(&written, times, &mut kept) {
        pass.logs
            .push(crate::tools::textedit::dedupe_log_head(&note));
    }
    // S5: the dropped runs become marks, and F1.11 places their edges on the sound — the words fence
    // both edges and each lane's cached envelope chooses where inside the fence. No envelope (no wave
    // cache for that recording) leaves the mark on its word times, which is the pass's documented
    // fallback, not a failure.
    let aligned: Vec<crate::edges::AlignedWord> = words
        .iter()
        .map(|word| crate::edges::AlignedWord {
            word: word.match_word.clone(),
            s: word.start,
            e: word.end,
        })
        .collect();
    let held = crate::edges::load(tree, &joins::session_lanes(tree, words));
    let (marks, edge_notes) =
        joins::marks_for(times, &kept, &aligned, crate::edges::finder(&held), &spoken);
    pass.logs.extend(edge_notes);
    // Only words THIS pass removed count toward the flag: `kept` also carries words that were
    // already gone before the joins ran, and blaming those would hang a whole-take wash on a
    // recording no join touched.
    let removed_by_pass: Vec<bool> = kept
        .iter()
        .enumerate()
        .map(|(index, keep)| !*keep && started_kept[index])
        .collect();
    let flagged = joins::whole_takes(words, &removed_by_pass);
    for (base, start, end) in &flagged {
        pass.logs.push(joins::whole_take_log(base, *start, *end));
        pass.whole.push(base.clone());
    }
    // S6: both files, in one step, together.
    if let Err(error) = joins::write_pair(tree, &sources, &written, &kept, &pass.whole, &marks) {
        pass.logs.push(format!(
            "!!! text edit: could not write the text files -- {error}"
        ));
        pass.kept = kept;
        return pass;
    }
    pass.removed = kept.iter().filter(|keep| !**keep).count();
    pass.stretches = stretches(&kept);
    // The completion line is spoken only when this pass changed the text: each seam already said
    // "nothing removed there", and a line of two zeroes on top of that reads as a repair that
    // forgot what it did.
    if pass.removed > 0 {
        // §6's own summary of the asks, said before the arithmetic line so a reader learns where the
        // answers came from before being told what they cost. `applied` counts the seams repaired; a
        // reply read back from `cache/llm/textedit` is counted in both halves, which is the point.
        pass.logs.push(crate::tools::textedit::joins_repaired_log(
            applied, from_cache,
        ));
        pass.logs
            .push(joins::written_log(pass.removed, total, pass.stretches));
    }
    pass.kept = kept;
    pass
}

/// What every line this pass speaks starts with. One constant so a reader, and the pass's own
/// "did I speak at all" check, have one thing to look for.
pub const JOIN_PREFIX: &str = ">>> text edit:";

/// How many standing words the wide view shows either side of a join: enough for the sentence a restart
/// started in, and the one before it.
pub const WIDE_WORDS: usize = 60;

/// The pass's loud line when joins ended with no usable answer: they are kept as recorded, stumbles and all.
pub fn unanswered_log(unanswered: usize, n: usize) -> String {
    format!("!!! text edit: {unanswered} of {n} join(s) got no usable answer -- kept as recorded")
}

/// The `>>> text edit: join k of n: ...` line for an applied seam.
pub fn applied_log(k: usize, n: usize, before: usize, after: usize) -> String {
    format!("{JOIN_PREFIX} join {k} of {n}: {before} word(s) off BEFORE, {after} off AFTER")
}

/// The line for a `keep_join` answer: the whole answer, said as such.
pub fn kept_log(k: usize, n: usize) -> String {
    format!("{JOIN_PREFIX} join {k} of {n}: kept whole -- nothing removed there")
}

/// The BEFORE window: the last [`SEAM_REACH_WORDS`] surviving words before the seam.
pub fn window_before(words: &[Word], kept: &[bool], seam: usize) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let end = seam.min(words.len());
    let mut index = end;
    while index > 0 && out.len() < SEAM_REACH_WORDS {
        index -= 1;
        if kept.get(index).copied().unwrap_or(true) {
            out.push(words[index].written.clone());
        }
    }
    out.reverse();
    out
}

/// The AFTER window: the first [`SEAM_REACH_WORDS`] surviving words from the seam on, stopping at `until` (where
/// the next take starts), so a window never runs on into the next join.
pub fn window_after(words: &[Word], kept: &[bool], seam: usize, until: usize) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let start = seam.min(words.len());
    let end = until.min(words.len());
    let mut index = start;
    while index < end && out.len() < SEAM_REACH_WORDS {
        if kept.get(index).copied().unwrap_or(true) {
            out.push(words[index].written.clone());
        }
        index += 1;
    }
    out
}

/// Take `count` surviving words off the end of the run before `seam`. Only surviving words are
/// counted, so a second join on the same seam cannot charge for the same word twice.
pub fn take_off_end(kept: &mut [bool], seam: usize, count: usize) {
    let edge = seam.min(kept.len());
    for _ in 0..count {
        match kept[..edge].iter().rposition(|keep| *keep) {
            Some(index) => kept[index] = false,
            None => break,
        }
    }
}

/// Take `count` surviving words off the start of the run after `seam`.
pub fn take_off_start(kept: &mut [bool], seam: usize, count: usize) {
    let start = seam.min(kept.len());
    for _ in 0..count {
        match kept[start..].iter().position(|keep| *keep) {
            Some(offset) => kept[start + offset] = false,
            None => break,
        }
    }
}

/// How many separate dropped runs there are — S6's "stretch(es)".
pub fn stretches(kept: &[bool]) -> usize {
    let mut out = 0usize;
    let mut inside = false;
    for keep in kept {
        if !*keep && !inside {
            out += 1;
            inside = true;
        } else if *keep {
            inside = false;
        }
    }
    out
}

/// The bare form a window's words are matched on, exposed so a caller can check a window against
/// the session list it came from.
pub fn bare(text: &str) -> String {
    word_list::bare(text)
}
