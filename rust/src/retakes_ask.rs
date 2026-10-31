//! F1.9's request leg: the retake marking pass reaching the LLM server over HTTP. [`crate::retakes`] holds the rules and dials nothing (spec/00-principles.md §5).
//! §F1.9 S2's brief goes out as the shipped `retake` prompt plus numbered lines, S3's three identical calls get their own cache slots, and answers go through §3.4's tools (`mark_abandoned`, `unmark`, `get_lines`, `finish`) so the TOOL decides what a valid mark is. Answers arrive as tool calls and as the prototype's strict JSON in the content (§S3); both are honoured.
//! New here is only the wire, pooling three runs into one set of marks, and reading the words S4 checks against.

use crate::edges::AlignedWord;
use crate::exchanges::{Message, Part};
use crate::layout::Tree;
use crate::llm_request;
use crate::project::Project;
use crate::retakes::{self, Outcome};
use crate::roles::Job;
use crate::services::{self, Server};
use crate::settings::{self, Conf, Paths};
use crate::textfmt::Retake;
use crate::tool_loop;
use crate::tools::retakes::{Line, Mark, Marks, Trim, AGAIN_REACH_SECONDS};
use crate::tools::{self, Tool};

pub const STEP: &str = retakes::STEP;

/// How much of the later take is read for the repeated tail; beyond it the "repeat" is a new passage. P.machine.againReachSeconds
const RETAKE_READ_SECONDS: f64 = AGAIN_REACH_SECONDS;

#[derive(Debug, Clone, PartialEq)]
pub enum RunOutcome {
    Answered {
        marks: usize,
    },
    Cached {
        marks: usize,
    },
    /// The ask failed at the wire; a server that is not there answers the same each time, so the pool carries on without re-dialling.
    Failed {
        reason: String,
    },
}

impl RunOutcome {
    pub fn answered(&self) -> bool {
        !matches!(self, RunOutcome::Failed { .. })
    }
}

/// The whole marking pass with the wire in it: §S2 brief, §S3 pooled calls, §S4 check against the words, §S5 edges and merge, §S6 ceiling, §S7 file and log line.
/// Every line goes to `log`, oldest first. A wire failure is reported per run and the pass still owes `retakes.tsv`, which tells Cut the pass ran.
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
        // Fewer than four lines: nobody is dialled, but the empty file is still owed.
        retakes::write_marks(tree, &[], &[], &[])?;
        log(&retakes::empty_marks_log());
        return Ok(());
    }
    let speech = retakes::speech_seconds(&lines);
    let brief = retakes::brief(&lines, &source_changes);
    log(&briefing_log(&lines));

    // ---- S3: the pooled runs -----------------------------------------------
    // ONE Marks across every run: the tool's merge and running share must see earlier runs' marks, else three runs agreeing on one stretch count it three times against the ceiling.
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
                // Set aside, not fatal; if NONE answers it is said once instead.
                if standing + (retakes::RUNS - run - 1) > 0 {
                    for line in tools::retakes::run_set_aside_log(
                        run,
                        &reason,
                        standing + (retakes::RUNS - run - 1),
                    ) {
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
    // No run answered: said ONCE; the empty file is still owed (tells Cut this pass ran).
    if standing == 0 {
        if let Some(reason) = last {
            log(&format!(
                "!!! retakes: the fixer answered nothing -- {reason}"
            ));
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
                log(&format!(
                    "!!! retakes: lines {}-{} -- {reason}",
                    mark.from, mark.to
                ));
            }
            Outcome::Rehear { from, to } => {
                // The second hearing (§S4's last clause); see `rehear`.
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
        .map(|word| AlignedWord {
            word: word.match_word.clone(),
            s: word.start,
            e: word.end,
        })
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
    // `stage_marking` owns the three endings (refusal, none-found, completion).
    let ending = retakes::stage_marking(true, &lines, &merged, marked, speech);
    let refused = ending.iter().any(|line| line.starts_with("!!!"));
    for line in &ending {
        log(line);
    }
    if refused {
        // S6: refuse EVERYTHING but still write the empty file, so Cut can tell "ran, refused" from "never ran".
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

/// The answer `finish` gives: a model over the ceiling must hear it IN THE REPLY, where it can still `unmark`.
pub fn finish_answer(marks: &Marks) -> String {
    let total = marks.finish();
    tools::ok(&serde_json::json!({
        "finished": true,
        "marks": total.marks,
        "speech_marked": total.share,
        "ceiling": total.ceiling,
        "over_ceiling": total.over,
        "marked_seconds": marks.marked_seconds(),
        "speech_seconds": marks.speech_seconds(),
        // §12 §4: the three pooled runs are thrown away together, and the model is told so with the way out named.
        "all_runs_dropped": total.over,
        "why": if total.over {
            "over the ceiling: every mark from all three runs is thrown away -- take some back with \
             unmark and call finish again"
        } else {
            "inside the ceiling: the marks stand"
        },
    }))
}

/// One pooled call: build the two messages, honour the cache, dial, run what was asked for. The run index rides in the cache key ([`crate::llm_cache::Request::run_index`]) so three identical questions stay three draws. Thinking is OFF (§02 §2), `Mode::Execute`.
pub fn ask_run(
    tree: &Tree,
    paths: &Paths,
    conf: &Conf,
    brief: &str,
    draw: usize,
    marks: &mut Marks,
) -> RunOutcome {
    let system = system_message(paths, &user_context(tree));
    // §6's slot for this draw, named by the one door that spells it; `ask_run` files under exactly this digest. Keyed on the ASSEMBLED system message, so an edited house prompt misses.
    let slot = run_key(&system, brief, &conf.model, draw).expect("a brief and a model always key");
    let parts = cache_parts(&system, brief, &conf.model, draw);
    if let Some(cached) = crate::llm_cache::read(tree, STEP, &parts) {
        // §09 §10: a cached draw writes its own `cache` line, no wire time.
        crate::request_timing::record_cached(
            tree,
            crate::server_leg::base_row(Server::Llm, &conf.model, "POST", "", STEP, "retakes"),
        );
        let named = apply_answer(&cached, marks);
        return RunOutcome::Cached { marks: named };
    }
    let messages = vec![
        Message {
            role: "system".to_string(),
            parts: vec![Part::Text(system.clone())],
        },
        Message {
            role: "user".to_string(),
            parts: vec![Part::Text(brief.to_string())],
        },
    ];
    let offered: Vec<Tool> = tools::offered(Job::Retakes);
    let endpoint = services::llm_endpoint(conf);
    // The raw body of the round that ENDED the flow (what this run's cache entry stands for) and how many marks the tools accepted.
    let mut final_body: Option<String> = None;
    let mut named = 0usize;
    let mut ask = |conversation: &[Message], tools: &[Tool]| -> Result<tool_loop::Turn, String> {
        let base =
            crate::server_leg::base_row(Server::Llm, &conf.model, "POST", "", STEP, "retakes");
        // §2 S5: a server that refuses `tools` is asked again without it, once, and not offered it again (shared with the fixer and cut).
        let sent = crate::llm_tool_refusal_leg::dial_honouring_refusal(
            STEP,
            &endpoint.url,
            Some(tools),
            &|offer| {
                // §4 gives the retake hunt thinking OFF; read from the prompt table (§10 §4).
                Ok(llm_request::body(
                    STEP,
                    &conf.model,
                    conversation,
                    crate::prompts::mode_of(STEP),
                    offer,
                    true,
                )?
                .to_string())
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
        let finishing = reply
            .tool_calls
            .iter()
            .any(|call| call.name == Tool::Finish.name());
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
        // §3.1's shared reads answered from the batch. `get_lines` is NOT taken here: §3.4 wants it with the pause before every line, which this job answers below.
        if !matches!(asked.name.as_str(), "get_lines") {
            let ctx = crate::tool_reads::Context {
                user_context: &user_context(tree),
                job: Job::Retakes,
                // The two fields §3.4's retake rules read, spelled as `naivepost.json` does, from the settings dialog's live fields (`ask_run` holds `Marks`, not the project; §12 §4).
                policy: &marking_policy_fields(),
                // The transcript rows in the request numbering; `get_context` reads this batch's extent.
                lines: &shared_lines(marks.lines()),
                events: &[],
                frames: &[],
            };
            if let Some(answer) = crate::tool_reads::dispatch(&ctx, asked) {
                return answer;
            }
        }
        let args: serde_json::Value =
            serde_json::from_str(&asked.args).unwrap_or_else(|_| serde_json::json!({}));
        let number = |key: &str| args.get(key).and_then(|v| v.as_u64()).map(|v| v as u32);
        match asked.name.as_str() {
            // The TOOL decides validity; its `{"error": …}` goes straight back to the model (§S3 refusal loop).
            "mark_abandoned" => match (number("from"), number("to")) {
                (Some(from), Some(to)) => {
                    // `again` 0 or absent means "never picked up" (None).
                    let again = number("again").filter(|n| *n != 0);
                    // The retake's first second goes to the tool with the mark, since the answer must name what the CUT loses (§12 §2).
                    let onset = again
                        .and_then(|n| marks.lines().iter().find(|line| line.n == n))
                        .map(|line| line.start)
                        .unwrap_or(0.0);
                    let answer = marks.mark_abandoned_at(from, to, again, Trim::none(), onset);
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
            // §3.4's read: the lines again WITH every pause, including those under the brief's threshold (where a speaker restarts).
            "get_lines" => match (number("from"), number("to")) {
                (Some(from), Some(to)) => lines_answer(marks, from, to),
                _ => tools::error("get_lines needs \"from\" and \"to\""),
            },
            // §12 §4: the ceiling verdict is spoken IN THE REPLY so the model can still `unmark`; `finish_answer` is its one home, `stage_marking` drops the marks.
            "finish" => finish_answer(marks),
            other => tools::error(&format!("{other} is not one of this job's tools")),
        }
    };
    let run = tool_loop::run(STEP, messages, &offered, &mut ask, &mut run_tool);
    match run.outcome {
        tool_loop::Outcome::Exhausted => {
            // §2 S4's line is the loop's own; say which run answered nothing.
            for line in &run.logs {
                crate::ui::window::log_line(line);
            }
            RunOutcome::Failed {
                reason: "no answer from it".to_string(),
            }
        }
        tool_loop::Outcome::Finished { .. } => {
            let Some(body) = final_body else {
                return RunOutcome::Failed {
                    reason: "answered with nothing to read".to_string(),
                };
            };
            // Only a readable answer is stored; an unreadable one must go out again.
            let kept = crate::llm_cache::store(tree, STEP, &parts, &body);
            if kept == crate::llm_cache::Stored::Written {
                // Said where it went: a resumed run reads this file back.
                crate::ui::window::log_line(&format!(
                    ">>> retakes draw {draw}: kept in cache/{STEP}/{slot}"
                ));
            }
            let parsed = apply_answer(&body, marks);
            RunOutcome::Answered {
                marks: parsed.max(named),
            }
        }
    }
}

/// Feed one run's answer into the shared marks. Reads both §S3 shapes: tool calls (counted by the caller) and strict JSON in the content. Returns how many stretches the JSON named.
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

/// §S4: what the words say about one pooled mark. The repeat is looked for from `again`, no further than [`RETAKE_READ_SECONDS`]. No `again` and no repeat is honest only for a whole take (`retakes::verify`); a partial mark claiming no replacement is a rephrase and only its broken-off tail goes.
fn check_against_words(
    marks: &Marks,
    words: &[SessionWord],
    lines: &[Line],
    mark: &Mark,
) -> Outcome {
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
    // A mark the words cannot confirm goes through `retakes::reheat`, which owns the second-hearing rule.
    let verdict = retakes::verify(
        marks, mark.from, mark.to, mark.again, tail, rephrase, whole_take,
    );
    if matches!(verdict, Outcome::Refused(_)) && !words.is_empty() {
        return retakes::reheat(mark.from, mark.to);
    }
    verdict
}

/// §S4's last clause: a refused mark is put to the model AGAIN over the same material, since the commonest cause is a wrong transcript. A re-ASK, not a re-transcription: F1.3/F1.4's `words.json` is the ASR pass's resume marker and must not be rewritten from here.
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
    // A different message is a different question, so it gets its own cache slot. The answer lands in a scratch set; only the refused stretch is read back.
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

/// The §3.4 `get_lines` answer: each line with its seconds and the pause in front of it, in `tool_reads`' shape so the shared door answers `get_context`/`get_events`/`get_frames`.
fn shared_lines(lines: &[Line]) -> Vec<crate::textfmt::Line> {
    lines
        .iter()
        .map(|line| crate::textfmt::Line {
            start: line.start,
            end: line.end,
            speaker: String::new(),
            text: String::new(),
        })
        .collect()
}

/// The policy fields `get_context` names: the marking pass (§12 §4) and the target length, spelled as `naivepost.json`. The reach, floor and ceiling are the tool's own rules, not settable.
pub(crate) fn marking_policy_fields() -> Vec<(&'static str, String)> {
    let policy = crate::ui::live_session_policy();
    vec![
        (
            "markingPass",
            crate::policy::stored_word(&policy, "markingPass"),
        ),
        (
            "targetLengthSeconds",
            crate::policy::stored_word(&policy, "targetLengthSeconds"),
        ),
    ]
}

pub fn lines_answer(marks: &Marks, from: u32, to: u32) -> String {
    let rows: Vec<String> = marks
        .lines_with_pause(from, to)
        .into_iter()
        .map(|(line, pause)| {
            format!(
                "{} [{}-{}] ({}s pause)",
                line.n,
                line.start,
                line.end,
                trim(pause)
            )
        })
        .collect();
    tools::ok(&serde_json::json!({ "lines": rows }))
}

/// The session's spoken lines numbered across sources, plus the numbers that BEGIN each source after the first (§S2 separator seams). `retakes::spoken_lines` numbers.
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
            // §S2's separator goes in front of the line that begins a recording only.
            changes.push(seen as u32 + 1);
        }
        seen += count;
    }
    (retakes::spoken_lines(tree, project), changes)
}

/// How many transcript rows one lane has, by `spoken_lines`' filter.
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

/// One word of the session: the spelled list when F1.13 wrote one, else the ASR's own words (as `joins::session_words_with_list`).
#[derive(Debug, Clone)]
pub struct SessionWord {
    pub match_word: String,
    pub start: f64,
    pub end: f64,
}

pub fn session_words(tree: &Tree, project: &Project) -> Vec<SessionWord> {
    if let Ok(Some(list)) = crate::word_list::load(tree) {
        let lanes: Vec<String> = project
            .sources
            .iter()
            .filter(|source| !crate::sources::is_narrator_mic(source))
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

fn session_words_from_asr(tree: &Tree, project: &Project) -> Vec<SessionWord> {
    let hz = crate::transcribe::SAMPLE_RATE as f64;
    let mut out = Vec::new();
    for source in &project.sources {
        if crate::sources::is_narrator_mic(source) {
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

fn words_in(lines: &[Line], words: &[SessionWord], from: u32, to: u32) -> Vec<String> {
    let first = lines
        .iter()
        .find(|line| line.n == from)
        .map(|line| line.start);
    let last = lines.iter().find(|line| line.n == to).map(|line| line.end);
    let (Some(start), Some(end)) = (first, last) else {
        return Vec::new();
    };
    words_window(words, Some(start), Some(end))
}

fn words_window(words: &[SessionWord], start: Option<f64>, end: Option<f64>) -> Vec<String> {
    words
        .iter()
        .filter(|word| {
            start.map(|at| word.start >= at).unwrap_or(true)
                && end.map(|at| word.start <= at).unwrap_or(true)
        })
        .map(|word| word.match_word.clone())
        .collect()
}

/// The session second the kept attempt begins: the `again` line's start, 0.0 when simply dropped.
fn again_second(lines: &[Line], mark: &Mark) -> f64 {
    mark.again
        .and_then(|n| lines.iter().find(|line| line.n == n))
        .map(|line| line.start)
        .unwrap_or(0.0)
}

fn briefing_log(lines: &[Line]) -> String {
    let pauses = lines
        .iter()
        .filter(|line| {
            let previous = lines.iter().find(|other| other.n + 1 == line.n);
            previous.is_some_and(|previous| {
                line.start - previous.end >= crate::tools::retakes::RETAKE_PAUSE_SECONDS
            })
        })
        .count();
    format!(
        ">>> retakes: briefing on {} spoken lines, {pauses} pause(s) of {} s or more",
        lines.len(),
        crate::tools::retakes::RETAKE_PAUSE_SECONDS
    )
}

/// The JOB's own wording: the shipped `retake` prompt, overridable in the settings folder (§04-prepare#6); an edit misses the cache (§6). On the wire it is wrapped by [`system_message`].
pub fn system_wording(paths: &Paths) -> String {
    let shipped = crate::prompts::shipped_text("retake").unwrap_or_default();
    settings::prompt_text(paths, "retake", &shipped).unwrap_or(shipped)
}

/// The system message §8 asks for: the house rules cut to this job's sections, its own wording, and the precedence rule when there is a User Context ([`crate::prompt_assembly`]).
pub fn system_message(paths: &Paths, context: &str) -> String {
    crate::prompt_assembly::system_message(
        &wording(paths, "system"),
        "retake",
        &system_wording(paths),
        context,
        false,
        false,
    )
}

fn wording(paths: &Paths, key: &str) -> String {
    let fallback = crate::prompts::shipped_text(key).unwrap_or_default();
    settings::prompt_text(paths, key, &fallback).unwrap_or(fallback)
}

/// The User Context from the session's note in `naivepost.json`; absent means none, not an invented one.
fn user_context(tree: &Tree) -> String {
    crate::project::load(tree.dir())
        .map(|project| project.context.clone())
        .unwrap_or_default()
}

/// The deciding parts for one pooled run; `run_index` gives three identical questions three slots.
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

/// §6's file name for one pooled draw: the digest over [`cache_parts`], which is how the run index reaches the key.
pub fn run_key(system: &str, brief: &str, model: &str, run: usize) -> Option<String> {
    crate::llm_cache::key(&cache_parts(system, brief, model, run))
}

fn trim(secs: f64) -> String {
    if (secs - secs.round()).abs() < 1e-9 {
        return format!("{}", secs.round() as i64);
    }
    format!("{:.1}", secs)
}

fn lane_of(name: &str) -> String {
    match name.split_once('.') {
        Some((stem, _)) => stem.to_string(),
        None => name.to_string(),
    }
}
