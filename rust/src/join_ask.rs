//! F1.10 S3's live half: the ask itself.
//!
//! [`crate::joins`] owns every rule that settles with no server — which seams exist, what a reply
//! means, the ceilings, the marks and both files. What it never did was *telephone*: on a session
//! with seams the pass stopped at its own brief line, so the answers that decide the cut arrived
//! from nobody. This module is that leg. It builds the two messages §F1.10 spells, dials the LLM
//! over HTTP through the same [`crate::server_leg`] the settings probes use, and hands the reply
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
//! this build bakes no prompt text into the crate ([`crate::bench::SHIPPED_FOR_TESTS`] is empty
//! and [`crate::prompts::shipped_file`] answers only a file *name* that nothing in `src/` reads),
//! so — exactly as [`crate::narrate_call`] does — the door reads the two wordings from
//! [`crate::settings::prompt_text`] with an empty fallback, which is what the settings box itself
//! shows when this machine holds no edit.
//!
//! could not be read either way, and new code goes in a new file in a tree this size.

use crate::cancel_leg;
use crate::exchanges::{Message, Mode, Part};
use crate::joins::{self, Outcome};
use crate::layout::Tree;
use crate::llm_cache;
use crate::llm_request;
use crate::project::Project;
use crate::requests::{Request, Service};
use crate::seam_retries::{self, Asking};
use crate::server_leg;
use crate::services::{self, Kind, Server};
use crate::settings::{self, Conf, Paths};
use crate::tools::textedit::{Join, SEAM_REACH_WORDS};
use crate::word_list::{self, Word};

/// The shipped wording of one prompt key: the text of the fenced ``` block of
/// `spec/prompts/<key>.md`, which is what the model is meant to receive; the markdown around it
/// is for readers of the spec. `None` when the key has no shipped file or it cannot be read.
///
/// [`crate::prompts::shipped_file`] answers only the name (`prompts/system.md`), relative to the
/// tree root, and nothing in `src/` reads it; this is the reader. The root is found by walking up
/// from the executable or the working directory rather than hardcoded, so the same binary reads
/// its prompts from an install tree, a dev checkout, or a test run out of `rust/`.
pub fn shipped_file(key: &str) -> Option<String> {
    let rel = crate::prompts::shipped_file(key)?;
    let raw = read_shipped(rel)?;
    Some(fenced_block(&raw).unwrap_or_else(|| raw.trim().to_string()))
}

/// Find and read one shipped prompt by its root-relative name. Two starting points are tried
/// (executable, then working directory) because a test runs from `rust/` while the installed
/// binary runs from anywhere.
fn read_shipped(rel: &str) -> Option<String> {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.to_path_buf()))
        .unwrap_or_default();
    let cwd = std::env::current_dir().unwrap_or_default();
    // `prompts/system.md` is the name `prompts::shipped_file` gives; the files sit under the
    // tree root's `spec/` folder, so both `<root>/prompts/…` (an install that ships the prompts
    // beside the binary) and `<root>/spec/prompts/…` (this checkout) are honoured.
    for start in [exe_dir, cwd] {
        let mut dir = start.as_path();
        // Bounded walk: `rust/target/debug/naivepost` is three levels under the root, and a
        // nested dev tree is deeper; six covers both without hunting the filesystem.
        for _ in 0..6 {
            for candidate in [dir.join(rel), dir.join("spec").join(rel)] {
                if let Ok(text) = std::fs::read_to_string(&candidate) {
                    return Some(text);
                }
            }
            match dir.parent() {
                Some(parent) => dir = parent,
                None => break,
            }
        }
    }
    None
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

/// The first ``` fenced block of a markdown file, its contents trimmed. `None` when there is none.
pub fn fenced_block(markdown: &str) -> Option<String> {
    let opened = markdown.find("```")?;
    let rest = &markdown[opened + 3..];
    // Skip the language tag line (```text) if the fence carries one.
    let body = match rest.find('\n') {
        Some(newline) => &rest[newline + 1..],
        None => rest,
    };
    let closed = body.find("```")?;
    Some(body[..closed].trim().to_string())
}

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
    before: &[String],
    after: &[String],
    attempt: u32,
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
            };
        }
        None => Conf::default(),
    };
    ask_at(tree, &conf, user_context, k, n, before, after, attempt, cancelled)
}

/// [`ask`] with the settings already read, for a caller that resolved them itself.
#[allow(clippy::too_many_arguments)]
pub fn ask_at(
    tree: &Tree,
    conf: &Conf,
    user_context: &str,
    k: usize,
    n: usize,
    before: &[String],
    after: &[String],
    attempt: u32,
    cancelled: &dyn Fn() -> bool,
) -> Answered {
    let model = conf.model.clone();
    let system = join_system(user_context);
    let user = joins::request(user_context, k, n, before, after);
    let parts = cache_parts(&system, &user, &model, attempt);
    if let Some(cached) = llm_cache::read(tree, joins::STEP, &parts) {
        // A reply served from the cache costs no wire and writes no row: the request that earned it
        // is already in `requests.tsv`, and a second row for the same answer would double-count a
        // call that did not happen (§09 §10).
        return Answered {
            outcome: judge(&cached, before, after),
            settled: true,
            reply: cached,
            failed: false,
        };
    }
    let body = match chat_body(&model, &system, &user) {
        Ok(body) => body,
        Err(reason) => {
            // No model configured: asking again answers the same way, so this settles.
            return Answered {
                outcome: Outcome::Refused(reason.clone()),
                settled: true,
                reply: String::new(),
                failed: true,
            };
        }
    };
    let base = base_row(&model);
    let endpoint = services::llm_endpoint(conf);
    let (_row, reply) = server_leg::call(
        tree,
        &base,
        Server::Llm,
        &endpoint,
        Kind::Chat,
        None,
        Some(&body),
        cancelled,
    );
    match reply {
        Ok(sent) => {
            let text = answer_of(&sent.body);
            let outcome = judge(&text, before, after);
            if outcome.usable() {
                llm_cache::store(tree, joins::STEP, &parts, &text);
            }
            Answered {
                outcome,
                settled: true,
                reply: text,
                failed: false,
            }
        }
        Err(reason) => Answered {
            // The ask itself failed. seam_retries says a transport error is not a refusal, so the
            // join is not retried and spends none of its retries on a server that is not there.
            outcome: Outcome::Refused(reason),
            settled: true,
            reply: String::new(),
            failed: true,
        },
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

/// The chat body: thinking on (`joins::thinking_on`), no tools offered — this job is given none.
fn chat_body(model: &str, system: &str, user: &str) -> Result<String, String> {
    let messages = vec![
        Message {
            role: "system".to_string(),
            parts: vec![Part::Text(system.to_string())],
        },
        Message {
            role: "user".to_string(),
            parts: vec![Part::Text(user.to_string())],
        },
    ];
    let body = llm_request::body(model, &messages, Mode::Thinking, None, false)?;
    Ok(body.to_string())
}

/// The answer text out of a chat reply: `content`, never the reasoning.
fn answer_of(reply: &str) -> String {
    let parsed: serde_json::Value = match serde_json::from_str(reply) {
        Ok(parsed) => parsed,
        Err(_) => return String::new(),
    };
    llm_request::parse_reply(&parsed).answer().to_string()
}

/// The deciding parts for one join ask. The retry's slot rides in `state` — the texts, model and
/// thinking flag are otherwise identical, and the slot is what keeps the two asks apart (§6).
fn cache_parts(system: &str, user: &str, model: &str, attempt: u32) -> Vec<serde_json::Value> {
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
) -> Pass {
    let total = words.len();
    let mut kept = vec![true; total];
    // What was standing when the pass began, so the whole-take flag below can tell a word this pass
    // removed from one that was already gone.
    let started_kept: Vec<bool> = kept.clone();
    let mut pass = Pass { kept: kept.clone(), ..Default::default() };
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
    for (index, seam) in seams.iter().enumerate() {
        let k = index + 1;
        let before = window_before(words, &kept, *seam);
        let after = window_after(words, &kept, *seam);
        if before.is_empty() || after.is_empty() {
            // Nothing standing on one side: there is no join left to ask about here.
            continue;
        }
        let mut asking = Asking::start(retries);
        loop {
            let answered = ask(
                tree,
                &project.context,
                k,
                n,
                &before,
                &after,
                asking.attempt,
                &cancelled,
            );
            match answered.outcome {
                Outcome::Applied { before: off_before, after: off_after } => {
                    take_off_end(&mut kept, *seam, off_before);
                    take_off_start(&mut kept, *seam, off_after);
                    pass.logs.push(applied_log(k, n, off_before, off_after));
                    break;
                }
                Outcome::Kept => {
                    pass.logs.push(kept_log(k, n));
                    break;
                }
                Outcome::Refused(reason) => {
                    if answered.failed {
                        pass.logs.push(joins::refusal_line(k, &reason, "nothing removed there"));
                        break;
                    }
                    match asking.refused() {
                        Some(tail) => pass.logs.push(joins::refusal_line(k, &reason, tail)),
                        None => {
                            pass.logs.push(joins::refusal_line(k, &reason, "nothing removed there"));
                            break;
                        }
                    }
                }
            }
        }
    }
    pass.by_seams = kept.iter().filter(|keep| !**keep).count();
    // S4: a saying doubled straight across a cut loses its earlier copy.
    let written: Vec<String> = words.iter().map(|word| word.written.clone()).collect();
    let sources: Vec<String> = words.iter().map(|word| word.source.clone()).collect();
    pass.logs.extend(joins::dedupe_across(&written, times, &mut kept));
    // S5: the dropped runs become marks, and F1.11 places their edges on the sound. No envelope
    // is asked for here; a session with waveforms gets them moved by the same call.
    let (marks, edge_notes) = joins::marks_for(times, &kept, &[], |_| None, &[]);
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
        pass.logs.push(format!("!!! text edit: could not write the text files -- {error}"));
        pass.kept = kept;
        return pass;
    }
    pass.removed = kept.iter().filter(|keep| !**keep).count();
    pass.stretches = stretches(&kept);
    // The completion line is spoken only when this pass changed the text: each seam already said
    // "nothing removed there", and a line of two zeroes on top of that reads as a repair that
    // forgot what it did.
    if pass.removed > 0 {
        pass.logs.push(joins::written_log(pass.removed, total, pass.stretches));
    }
    pass.kept = kept;
    pass
}

/// What every line this pass speaks starts with. One constant so a reader, and the pass's own
/// "did I speak at all" check, have one thing to look for.
pub const JOIN_PREFIX: &str = ">>> text edit:";

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

/// The AFTER window: the first [`SEAM_REACH_WORDS`] surviving words from the seam on.
pub fn window_after(words: &[Word], kept: &[bool], seam: usize) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let start = seam.min(words.len());
    let mut index = start;
    while index < words.len() && out.len() < SEAM_REACH_WORDS {
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
