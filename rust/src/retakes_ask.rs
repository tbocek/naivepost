//! F1.9's request leg: the retake marking pass reaching the LLM server over HTTP.
//!
//! [`crate::retakes`] holds every rule of F1.9 and dials nothing — by design, because a step that
//! decided inside a tool call could never be tested (spec/00-principles.md §5). This is the leg that
//! supplies the answers those rules act on: §F1.9 S2's brief goes out as the shipped `retake` prompt
//! plus the numbered lines, S3's three identical calls each get their own cache slot, and whatever
//! comes back is fed through §3.4's own tools (`mark_abandoned`, `unmark`, `get_lines`, `finish`) so
//! the TOOL decides what a valid mark is rather than a copy of that decision living here. The answer
//! therefore arrives two ways and both are honoured: as tool calls, and as the prototype's strict
//! JSON in the reply content (§S3 spells that shape; a model that was told to answer in JSON and is
//! then only read for tool calls loses the answer it gave).
//!
//! Nothing here computes a trim, a merge, a ceiling or a duration: those are `retakes::*` and
//! `tools::retakes::*` already, and this module calls them. What is new is only the wire, the pooling
//! of three runs into one set of marks, and reading the words S4 checks a mark against.

use crate::edges::AlignedWord;
use crate::exchanges::{Message, Mode, Part};
use crate::layout::Tree;
use crate::llm_request;
use crate::project::Project;
use crate::retakes::{self, Outcome};
use crate::roles::Job;
use crate::services::{self, Server};
use crate::settings::{self, Conf, Paths};
use crate::textfmt::Retake;
use crate::tool_loop;
use crate::tools::{self, Tool};
use crate::tools::retakes::{Line, Mark, Marks, Trim, AGAIN_REACH_SECONDS};

/// The step name this job's cache entries, timed rows and log lines carry.
pub const STEP: &str = retakes::STEP;

/// How much of the later take is read looking for the repeated tail. Beyond this the "repeat" is a
/// new passage, not the tail of the attempt. P.machine.againReachSeconds
const RETAKE_READ_SECONDS: f64 = AGAIN_REACH_SECONDS;

/// What one pooled run came back as.
#[derive(Debug, Clone, PartialEq)]
pub enum RunOutcome {
    /// The run answered and its marks went into the shared set: how many it named.
    Answered { marks: usize },
    /// Read back from `cache/llm` instead of asked: costs no wire and writes no row (§09 §10).
    Cached { marks: usize },
    /// The ask failed at the wire. Not a refusal of an answer: a server that is not there answers the
    /// same way however often it is called, so the pool carries on without re-dialling it.
    Failed { reason: String },
}

impl RunOutcome {
    /// Whether this run contributed an answer the pool can use.
    pub fn answered(&self) -> bool {
        !matches!(self, RunOutcome::Failed { .. })
    }
}

/// The whole marking pass with the wire in it: §S2's brief, §S3's pooled calls, §S4's check against
/// the words, §S5's edges and merge, §S6's ceiling and §S7's file and log line.
///
/// Every line it speaks goes to `log`, oldest first, so the caller (and the window's ▶ handler)
/// contributes nothing of its own about this pass. A dial that fails at the wire is reported per run
/// and the pass still owes `retakes.tsv`, which is what tells Cut the pass ran (§Failure-is-specific:
/// name the reason where it failed, carry on elsewhere).
pub fn run_pass(
    tree: &Tree,
    project: &Project,
    paths: &Paths,
    conf: &Conf,
    log: &mut impl FnMut(&str),
) -> Result<(), String> {
    // ---- S2: the brief ------------------------------------------------------
    let (lines, source_changes) = numbered_lines(tree, project);
    if retakes::too_few(&lines) {
        // S1 again, from the side that would otherwise ask: fewer than four lines and nobody is
        // dialled, but the empty file is still owed.
        retakes::write_marks(tree, &[], &[], &[])?;
        log(&retakes::empty_marks_log());
        return Ok(());
    }
    let speech = retakes::speech_seconds(&lines);
    let brief = retakes::brief(&lines, &source_changes);
    log(&briefing_log(&lines));

    // ---- S3: the pooled runs -----------------------------------------------
    // ONE Marks across every run: the tool's own merge and its running share have to see what the
    // earlier runs already marked, or three runs agreeing on one stretch count it three times
    // against the ceiling and refuse a session the runs actually agreed about.
    let mut marks = Marks::new(lines.clone(), speech);
    let mut last: Option<String> = None;
    let mut standing = 0usize;
    let mut cached = 0usize;
    for run in 0..retakes::RUNS {
        match ask_run(tree, paths, conf, &brief, run, &mut marks) {
            RunOutcome::Answered { marks: named } => {
                standing += 1;
                let _ = named;
            }
            RunOutcome::Cached { marks: named } => {
                cached += 1;
                standing += 1;
                let _ = named;
            }
            RunOutcome::Failed { reason } => {
                // Set aside, not fatal: losing one draw of three is not a reason to lose the two that
                // answered. But if NONE of them answers, saying it once is enough — the run-by-run
                // line is news only when the pool still goes on without that run.
                if standing + (retakes::RUNS - run - 1) > 0 {
                    for line in
                        tools::retakes::run_set_aside_log(run, &reason, standing + (retakes::RUNS - run - 1))
                    {
                        log(&line);
                    }
                }
                last = Some(reason);
            }
        }
    }
    if cached > 0 {
        log(&tools::retakes::cached_runs_log(cached, retakes::RUNS));
    }
    // No run answered at all: the reason is said ONCE, not three times, and the empty file is still
    // owed because that is what tells Cut this pass ran (§Failure-is-specific).
    if standing == 0 {
        if let Some(reason) = last {
            log(&format!("!!! retakes: the fixer answered nothing -- {reason}"));
        }
        retakes::write_marks(tree, &[], &[], &[])?;
        return Ok(());
    }
    let pooled = retakes::dedupe(marks.marks().to_vec());

    // ---- S4: checked against the words -------------------------------------
    let words = session_words(tree, project);
    let mut applied: Vec<Mark> = Vec::new();
    for mark in &pooled {
        let outcome = check_against_words(&marks, &words, &lines, mark);
        match outcome {
            Outcome::Applied { removed, trimmed } => {
                if trimmed {
                    log(&format!(
                        ">>> retakes: lines {}-{} trimmed to the words the later take repeats",
                        mark.from, mark.to
                    ));
                }
                applied.push(Mark {
                    removed,
                    trimmed,
                    ..mark.clone()
                });
            }
            Outcome::Dropped(reason) | Outcome::Refused(reason) => {
                log(&format!("!!! retakes: lines {}-{} -- {reason}", mark.from, mark.to));
            }
            Outcome::Rehear { from, to } => {
                // The second hearing (§S4's last clause) — see `rehear` for why it re-asks the
                // material rather than re-transcribing the span.
                match rehear(tree, paths, conf, &brief, &mut marks, from, to) {
                    Some(reheard) => {
                        log(&format!(
                            ">>> retakes: lines {from}-{to} re-heard, now marked from {} to {}",
                            reheard.removed.0, reheard.removed.1
                        ));
                        applied.push(reheard);
                    }
                    None => log(&format!(
                        "!!! retakes: lines {from}-{to} re-heard and still no repeat found -- left in the cut"
                    )),
                }
            }
        }
    }

    // ---- S5: edges placed by F1.11, then merged ----------------------------
    let spoken: Vec<(f64, f64)> = lines.iter().map(|line| (line.start, line.end)).collect();
    let aligned: Vec<AlignedWord> = words
        .iter()
        .map(|word| AlignedWord { word: word.match_word.clone(), s: word.start, e: word.end })
        .collect();
    let as_retakes: Vec<Retake> = applied
        .iter()
        .map(|mark| Retake {
            s: mark.removed.0,
            e: mark.removed.1,
            again: again_second(&lines, mark),
            to: mark.removed.1,
            text: String::new(),
            whole: String::new(),
        })
        .collect();
    let (placed, notes) = retakes::place_edges(as_retakes, &aligned, |_| None, &spoken);
    for note in &notes {
        log(note);
    }
    let merged = retakes::merge_marks(applied);
    let marked: f64 = placed.iter().map(|row| (row.e - row.s).max(0.0)).sum();

    // ---- S6 / S7: the ceiling, the file, the line --------------------------
    // `stage_marking` owns the three endings (refusal, none-found, completion) so their wording lives
    // in one place; every line it returns is spoken here.
    let ending = retakes::stage_marking(true, &lines, &merged, marked, speech);
    let refused = ending.iter().any(|line| line.starts_with("!!!"));
    for line in &ending {
        log(line);
    }
    if refused {
        // S6: refuse EVERYTHING — and still write the file, empty on purpose, so Cut can tell "ran,
        // refused" from "never ran".
        retakes::write_marks(tree, &[], &[], &[])?;
        return Ok(());
    }
    let texts: Vec<String> = placed.iter().map(|row| row.text.clone()).collect();
    let again_seconds: Vec<f64> = placed.iter().map(|row| row.again).collect();
    if merged.is_empty() {
        retakes::write_marks(tree, &[], &[], &[])?;
        return Ok(());
    }
    retakes::write_marks(tree, &merged, &texts, &again_seconds)?;
    Ok(())
}

/// One pooled call: build the two messages, honour the cache, dial, run what was asked for.
///
/// The run index rides in the cache key ([`crate::llm_cache::Request::run_index`], the field §6 added
/// for exactly this pool): three identical questions whose answers are three separate draws must not
/// collapse into one answer read back twice. Thinking is OFF — §02 §2's row for this job — which is
/// `Mode::Execute`, the same switch every non-thinking job takes.
pub fn ask_run(
    tree: &Tree,
    paths: &Paths,
    conf: &Conf,
    brief: &str,
    draw: usize,
    marks: &mut Marks,
) -> RunOutcome {
    let system = system_wording(paths);
    // §6's slot for this draw, named by the one door that spells it. `ask_run` files its answer under
    // exactly this digest — the store below hands `llm_cache` the same parts this key came from.
    let slot = run_key(&system, brief, &conf.model, draw).expect("a brief and a model always key");
    let parts = cache_parts(&system, brief, &conf.model, draw);
    if let Some(cached) = crate::llm_cache::read(tree, STEP, &parts) {
        let named = apply_answer(&cached, marks);
        return RunOutcome::Cached { marks: named };
    }
    let messages = vec![
        Message { role: "system".to_string(), parts: vec![Part::Text(system.clone())] },
        Message { role: "user".to_string(), parts: vec![Part::Text(brief.to_string())] },
    ];
    let offered: Vec<Tool> = tools::offered(Job::Retakes);
    let endpoint = services::llm_endpoint(conf);
    // What the rounds collected: the raw body of the round that ENDED the flow, which is the pair this
    // run's cache entry stands for, and how many marks the tool surface accepted.
    let mut final_body: Option<String> = None;
    let mut named = 0usize;
    let mut ask = |conversation: &[Message], tools: &[Tool]| -> Result<tool_loop::Turn, String> {
        let base = crate::server_leg::base_row(Server::Llm, &conf.model, "POST", "", STEP, "retakes");
        // §2 S5: a server that refuses the `tools` field is asked again without it, once, and never offered the
        // field again — the same door the fixer and the cut use, so no step re-logs the refusal per attempt.
        let sent = crate::llm_tool_refusal_leg::dial_honouring_refusal(
            STEP,
            &endpoint.url,
            Some(tools),
            &|offer| {
                Ok(llm_request::body(STEP, &conf.model, conversation, Mode::Execute, offer, true)?.to_string())
            },
            &|body| {
                crate::llm_retry_leg::chat(
                    STEP,
                    tree,
                    &base,
                    &endpoint,
                    body,
                    &crate::cancel_leg::cancel_check_now(),
                    &crate::ui::window::log_line,
                )
            },
            &crate::ui::window::log_line,
        )?;
        let reply = llm_request::reply_from(&sent.content_type, &sent.body)
            .map_err(|why| format!("{STEP}: {why}"))?;
        let finishing = reply.tool_calls.iter().any(|call| call.name == Tool::Finish.name());
        if finishing || reply.tool_calls.is_empty() {
            final_body = Some(sent.body.clone());
        }
        Ok(tool_loop::Turn {
            content: reply.content.clone(),
            calls: reply.tool_calls.clone(),
            finish: finishing,
        })
    };
    let mut run_tool = |asked: &crate::exchanges::ToolCall| -> String {
        let args: serde_json::Value = serde_json::from_str(&asked.args)
            .unwrap_or_else(|_| serde_json::json!({}));
        let number = |key: &str| args.get(key).and_then(|v| v.as_u64()).map(|v| v as u32);
        match asked.name.as_str() {
            // The TOOL decides validity — range, `again` inside the stretch, the reach, the minimum —
            // and its `{"error": …}` sentence goes straight back to the model, which is §S3's refusal
            // loop. Swallowing it here would leave the model unable to correct itself.
            "mark_abandoned" => match (number("from"), number("to")) {
                (Some(from), Some(to)) => {
                    // `again` 0 or absent means "never picked up", which the tool spells None.
                    let again = number("again").filter(|n| *n != 0);
                    let answer = marks.mark_abandoned(from, to, again, Trim::none());
                    if !crate::fix_transcripts::refused(&answer) {
                        named += 1;
                    }
                    answer
                }
                _ => tools::error("mark_abandoned needs \"from\" and \"to\""),
            },
            "unmark" => match (number("from"), number("to")) {
                (Some(from), Some(to)) => marks.unmark(from, to),
                _ => tools::error("unmark needs \"from\" and \"to\""),
            },
            // §3.4's read: the lines again WITH every pause, including the ones under the threshold
            // the brief leaves out — those short stops are where a speaker restarts.
            "get_lines" => match (number("from"), number("to")) {
                (Some(from), Some(to)) => lines_answer(marks, from, to),
                _ => tools::error("get_lines needs \"from\" and \"to\""),
            },
            "finish" => {
                let total = marks.finish();
                tools::ok(&serde_json::json!({
                    "finished": true,
                    "marks": total.marks,
                    "speech_marked": total.share,
                    "ceiling": total.ceiling,
                    "over_ceiling": total.over,
                }))
            }
            other => tools::error(&format!("{other} is not one of this job's tools")),
        }
    };
    let run = tool_loop::run(STEP, messages, &offered, &mut ask, &mut run_tool);
    match run.outcome {
        tool_loop::Outcome::Exhausted => {
            // §2 S4's line is the loop's own; the run answered nothing, so say which and move on.
            for line in &run.logs {
                crate::ui::window::log_line(line);
            }
            RunOutcome::Failed { reason: "no answer from it".to_string() }
        }
        tool_loop::Outcome::Finished { .. } => {
            let Some(body) = final_body else {
                return RunOutcome::Failed { reason: "answered with nothing to read".to_string() };
            };
            // Only a readable answer is stored: an unreadable one must go out again rather than be
            // replayed from the slot it failed in. The slot is the one named above.
            let kept = crate::llm_cache::store(tree, STEP, &parts, &body);
            if kept == crate::llm_cache::Stored::Written {
                // Said where it went: a resumed run reads this file back, and the name is what proves
                // which draw it belongs to.
                crate::ui::window::log_line(&format!(
                    ">>> retakes draw {draw}: kept in cache/{STEP}/{slot}"
                ));
            }
            let parsed = apply_answer(&body, marks);
            RunOutcome::Answered { marks: parsed.max(named) }
        }
    }
}

/// Feed one run's answer into the shared marks. Both shapes §S3 allows are read: the tool calls the
/// loop already ran (counted by the caller), and the strict JSON the prototype demanded in the
/// content. Returns how many stretches the JSON named.
fn apply_answer(body: &str, marks: &mut Marks) -> usize {
    let content = llm_request::reply_from("application/json", body)
        .map(|reply| reply.answer().to_string())
        .unwrap_or_default();
    let rows = retakes::parse_answer(&content);
    for (from, to, again) in &rows {
        marks.mark_abandoned(*from, *to, (*again != 0).then_some(*again), Trim::none());
    }
    rows.len()
}

/// §S4: what the words say about one pooled mark.
///
/// The repeat is looked for in the later take's words starting at `again`, read no further than
/// [`RETAKE_READ_SECONDS`] — beyond that the echo is a new passage rather than the tail of the
/// attempt. No `again` and no repeat means the attempt broke off and was never picked up, which is
/// only honest for a whole take (the rule `retakes::verify` states); a partial mark claiming no
/// replacement is a rephrase, and only its broken-off tail goes.
fn check_against_words(marks: &Marks, words: &[SessionWord], lines: &[Line], mark: &Mark) -> Outcome {
    let attempt = words_in(lines, words, mark.from, mark.to);
    let whole_take = mark.from <= 1 && mark.to >= lines.len() as u32;
    let (tail, rephrase) = match mark.again {
        Some(n) => {
            let start = lines.iter().find(|line| line.n == n).map(|line| line.start);
            let end = start.map(|at| at + RETAKE_READ_SECONDS);
            let later = words_window(words, start, end);
            (retakes::repeated_tail_match(&attempt, &later), false)
        }
        None => (None, !whole_take),
    };
    // A mark the words cannot confirm goes through `retakes::reheat`, so the second-hearing rule is
    // stated once, in the module that owns the rule, rather than being implied by this branch.
    let verdict = retakes::verify(marks, mark.from, mark.to, mark.again, tail, rephrase, whole_take);
    if matches!(verdict, Outcome::Refused(_)) && !words.is_empty() {
        return retakes::reheat(mark.from, mark.to);
    }
    verdict
}

/// §S4's last clause: a mark the words refuse is put to the model AGAIN over the same material before
/// it is thrown away, because the commonest reason no repeat is found is that one of the two
/// transcripts is wrong. `retakes::reheat` names the outcome this branch acts on.
///
/// It is a re-ASK of the material rather than a fresh transcription of the span on purpose: F1.3/F1.4's
/// ASR pass owns `words.json` and that file's existence is ITS resume marker ("a step's own output file
/// is its resume marker"), so a span re-transcription started here would rewrite another step's marker
/// file out from under the rule that makes an interrupted run resume. Asking again costs one call and
/// disturbs nothing.
fn rehear(
    tree: &Tree,
    paths: &Paths,
    conf: &Conf,
    brief: &str,
    marks: &mut Marks,
    from: u32,
    to: u32,
) -> Option<Mark> {
    let message = format!(
        "{brief}\nThe previous answer was refused: lines {from}-{to} claim a retake but no repeated \
         tail was found in the words. Re-read the two attempts and answer again."
    );
    // A different message is a different question, which is what puts this ask in its own cache slot
    // rather than replaying the refusal it is meant to overcome. The answer lands in a scratch set so
    // a second hearing cannot add marks nobody asked about; only the refused stretch is read back.
    let mut scratch = Marks::new(marks.lines().to_vec(), 0.0);
    match ask_run(tree, paths, conf, &message, retakes::RUNS, &mut scratch) {
        RunOutcome::Failed { .. } => None,
        _ => scratch
            .marks()
            .iter()
            .find(|candidate| candidate.from == from && candidate.to == to)
            .cloned(),
    }
}

/// The §3.4 `get_lines` answer: each line with its seconds and the pause in front of it.
fn lines_answer(marks: &Marks, from: u32, to: u32) -> String {
    let rows: Vec<String> = marks
        .lines_with_pause(from, to)
        .into_iter()
        .map(|(line, pause)| {
            format!("{} [{}-{}] ({}s pause)", line.n, line.start, line.end, trim(pause))
        })
        .collect();
    tools::ok(&serde_json::json!({ "lines": rows }))
}

/// The session's spoken lines, numbered across every source, plus the numbers that BEGIN a source
/// after the first — the seams §S2's separator marks. `retakes::spoken_lines` does the numbering;
/// this only records where each source starts so the seam list can be built beside it.
pub fn numbered_lines(tree: &Tree, project: &Project) -> (Vec<Line>, Vec<u32>) {
    let mut changes: Vec<u32> = Vec::new();
    let mut seen = 0usize;
    for source in &project.sources {
        let lane = lane_of(source.path.rsplit('/').next().unwrap_or(&source.path));
        let count = transcript_count(tree, &lane);
        if count == 0 {
            continue;
        }
        if seen > 0 {
            // The line that begins this recording is the first of its own block: §S2's separator goes
            // in front of that number and nowhere else.
            changes.push(seen as u32 + 1);
        }
        seen += count;
    }
    (retakes::spoken_lines(tree, project), changes)
}

/// How many transcript rows one lane has — the same filter `spoken_lines` applies, so the seam lands
/// on the number that row actually got.
fn transcript_count(tree: &Tree, lane: &str) -> usize {
    let Ok(text) = std::fs::read_to_string(tree.transcript_tsv(lane)) else {
        return 0;
    };
    text.lines()
        .filter(|row| {
            let fields: Vec<&str> = row.split('\t').collect();
            fields.len() >= 4
                && fields[0].parse::<f64>().is_ok()
                && fields[1].parse::<f64>().is_ok()
        })
        .count()
}

/// One word of the session, wherever it came from: the spelled list when F1.13 wrote one, the ASR's
/// own words otherwise. Same preference `joins::session_words_with_list` states, for the same reason —
/// the spellings every other pass works from, on the aligned times.
#[derive(Debug, Clone)]
pub struct SessionWord {
    pub match_word: String,
    pub start: f64,
    pub end: f64,
}

/// The session's words in session order, narrator mic left out (its words are heard, not played).
pub fn session_words(tree: &Tree, project: &Project) -> Vec<SessionWord> {
    if let Ok(Some(list)) = crate::word_list::load(tree) {
        let lanes: Vec<String> = project
            .sources
            .iter()
            .filter(|source| source.narrator == 0)
            .map(|source| lane_of(source.path.rsplit('/').next().unwrap_or(&source.path)))
            .collect();
        let words: Vec<SessionWord> = list
            .iter()
            .filter(|word| lanes.is_empty() || lanes.contains(&word.source))
            .map(|word| SessionWord {
                match_word: word.match_word.clone(),
                start: word.start,
                end: word.end,
            })
            .collect();
        if !words.is_empty() {
            return words;
        }
    }
    session_words_from_asr(tree, project)
}

/// The fallback: each lane's `words.json`, samples turned into seconds at the ASR's own rate.
fn session_words_from_asr(tree: &Tree, project: &Project) -> Vec<SessionWord> {
    let hz = crate::transcribe::SAMPLE_RATE as f64;
    let mut out = Vec::new();
    for source in &project.sources {
        if source.narrator != 0 {
            continue;
        }
        let lane = lane_of(source.path.rsplit('/').next().unwrap_or(&source.path));
        let Ok(Some(doc)) = crate::requests::read_words(tree, &lane) else {
            continue;
        };
        for word in doc.words {
            if word.word.trim().is_empty() {
                continue;
            }
            out.push(SessionWord {
                match_word: crate::word_list::bare(&word.word),
                start: word.start_sample as f64 / hz,
                end: word.end_sample as f64 / hz,
            });
        }
    }
    out
}

/// The words a numbered range of lines covers.
fn words_in(lines: &[Line], words: &[SessionWord], from: u32, to: u32) -> Vec<String> {
    let first = lines.iter().find(|line| line.n == from).map(|line| line.start);
    let last = lines.iter().find(|line| line.n == to).map(|line| line.end);
    let (Some(start), Some(end)) = (first, last) else {
        return Vec::new();
    };
    words_window(words, Some(start), Some(end))
}

/// The words between two seconds, inclusive at the front edge.
fn words_window(words: &[SessionWord], start: Option<f64>, end: Option<f64>) -> Vec<String> {
    words
        .iter()
        .filter(|word| start.map(|at| word.start >= at).unwrap_or(true)
            && end.map(|at| word.start <= at).unwrap_or(true))
        .map(|word| word.match_word.clone())
        .collect()
}

/// The session second the kept attempt begins: the `again` line's own start, 0.0 when the attempt was
/// simply dropped and has nothing to point at.
fn again_second(lines: &[Line], mark: &Mark) -> f64 {
    mark.again
        .and_then(|n| lines.iter().find(|line| line.n == n))
        .map(|line| line.start)
        .unwrap_or(0.0)
}

/// S2 as a log line: what the brief numbers and how many pauses worth drawing it found.
fn briefing_log(lines: &[Line]) -> String {
    let pauses = lines
        .iter()
        .filter(|line| {
            let previous = lines.iter().find(|other| other.n + 1 == line.n);
            previous.is_some_and(|previous| line.start - previous.end >= crate::tools::retakes::RETAKE_PAUSE_SECONDS)
        })
        .count();
    format!(
        ">>> retakes: briefing on {} spoken lines, {pauses} pause(s) of {} s or more",
        lines.len(),
        crate::tools::retakes::RETAKE_PAUSE_SECONDS
    )
}

/// The wording to ask with: the shipped `retake` prompt, overridable through the settings folder so an
/// edit there wins (§04-prepare#6) and an edited prompt misses the cache (§6).
///
/// This is also §6's naming door: [`ask_run`] reads its slot from this same text before it dials, so
/// the wording that went out and the digest filed under it cannot drift.
pub fn system_wording(paths: &Paths) -> String {
    let shipped = crate::prompts::shipped_text("retake").unwrap_or_default();
    settings::prompt_text(paths, "retake", &shipped).unwrap_or(shipped)
}

/// The deciding parts for one pooled run. `run_index` is the whole point: three identical questions
/// are three draws, and a shared slot would make runs two and three read run one's reply back.
fn cache_parts(system: &str, user: &str, model: &str, run: usize) -> Vec<serde_json::Value> {
    let request = crate::llm_cache::Request {
        system,
        user,
        state: "",
        speech: "",
        context: "",
        images: Vec::new(),
        run_index: Some(run as u32),
        model,
        thinking: false,
    };
    crate::llm_cache::parts(&request)
}

/// §6's file name for one pooled draw: the digest over [`cache_parts`]' parts, which is how the run
/// index reaches the key. `ask_run` asks for it before it dials and files its answer under exactly that
/// name, so the rule has one spelling rather than a second copy of the digest somewhere else.
pub fn run_key(system: &str, brief: &str, model: &str, run: usize) -> Option<String> {
    crate::llm_cache::key(&cache_parts(system, brief, model, run))
}

/// Seconds without a tail of zeroes, for a log line.
fn trim(secs: f64) -> String {
    if (secs - secs.round()).abs() < 1e-9 {
        return format!("{}", secs.round() as i64);
    }
    format!("{:.1}", secs)
}

/// The lane a source's files live under: its file name minus the extension.
fn lane_of(name: &str) -> String {
    match name.split_once('.') {
        Some((stem, _)) => stem.to_string(),
        None => name.to_string(),
    }
}
