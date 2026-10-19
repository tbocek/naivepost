//! The rules [`spec/AGENT.md`](../spec/AGENT.md#rules-the-code-holds-to) says this code holds to,
//! answered from live doors rather than from the prose.
//!
//! A rule written down is a claim; a rule printed by the program is a fact recomputed on every call.
//! Each of the nine bullets gets one line, in the spec's own order, and each line names the door that
//! actually enforces it — the function a cut really passes through, the counter a resume really checks,
//! the key a cache really stores under. Nothing here retypes a number or re-states a prompt: the counts
//! come from [`crate::params`], the pad from [`crate::edges::WORD_PAD`], the path answer from a real
//! round-trip through [`crate::layout`] in a scratch folder this call makes and removes.
//!
//! One door uses it: `naivepost --rules` in `main`, answered before any display is asked for, so it
//! works in the headless container the same way `--stack` does. Every computation is
//! panic-free: a rule whose evidence cannot be gathered says so on its line instead of failing.

use std::path::Path;

use crate::degraded;
use crate::edges;
use crate::llm_cache;
use crate::params;
use crate::prompts;
use crate::retakes;
use crate::run_stages;

/// Rule 1: models propose in words and line numbers; the machine places the cut.
fn models_propose() -> String {
    let known = prompts::KEYS
        .iter()
        .filter(|key| prompts::is_known(key))
        .count();
    format!(
        "models propose, the machine places: {} prompt keys ({}) ask in words, line numbers or \
         clip-relative seconds; edges::place_edges turns the aligner's word times into the cut and \
         cut_clamp::clamp_to_cut fences what a model asked for to the cut it belongs to",
        known,
        prompts::KEYS.len()
    )
}

/// Rule 2: every cut lands on a word edge; the envelope only chooses inside the fence.
fn word_edges() -> String {
    format!(
        "every cut lands on a word edge: edges::place_edges fences both edges on word times and the \
         envelope (Edges::end_after / Edges::start_before) only chooses where between two known words \
         the splice falls; with no envelope a hair of edges::WORD_PAD = {} s stands in",
        edges::WORD_PAD
    )
}

/// Rule 3: nothing is deleted, only marked.
fn only_marked() -> String {
    // Prove the read half live: the documented shape parses into line-number ranges.
    let sample = retakes::parse_answer(r#"{"abandoned": [{"from": 7, "to": 12, "again": 15}]}"#);
    format!(
        "nothing is deleted, only marked: tools::retakes::Marks::mark_abandoned adds a range and \
         leaves the transcript's lines untouched; retakes::parse_answer reads the model's answer as \
         1-based line ranges (sample {{\"abandoned\": [...]}} -> {sample:?}, one mark)"
    )
}

/// Rule 4: a step's own output file is its resume marker.
fn resume_marker() -> String {
    let there = run_stages::Task::already_there("extracting frames");
    let must = run_stages::Task::run_it("extracting frames", "ffmpeg", Vec::new());
    format!(
        "a step's own output file is its resume marker: run_stages::Task::already_there sets \
         already_done={} (counted, never run again) where Task::run_it sets {} -- an interrupted \
         run resumes from what is on disk instead of restarting",
        there.already_done, must.already_done
    )
}

/// Rule 5: a tuning number lives once, in the module whose rule uses it; params.rs catalogues.
fn params_catalogue() -> String {
    let sections: [(&str, Vec<params::Param>); 5] = [
        ("prepare", params::prepare()),
        ("cut", params::cut()),
        ("effects", params::effects()),
        ("narrate", params::narrate()),
        ("produce", params::produce()),
    ];
    let total: usize = sections.iter().map(|(_, rows)| rows.len()).sum();
    let named = sections
        .iter()
        .flat_map(|(_, rows)| rows.iter())
        .filter(|row| row.from.contains("::"))
        .count();
    let parts: Vec<String> = sections
        .iter()
        .map(|(name, rows)| format!("{name} {}", rows.len()))
        .collect();
    format!(
        "a tuning number lives once, in the module whose rule uses it: params.rs catalogues {total} \
         rows ({}), holds none of them itself -- {named} of {total} name the constant they are built \
         from (module::CONST); a value §10 gives no P. id takes a bare prefix (machine., preview., \
         project.)",
        parts.join(", ")
    )
}

/// A request shaped like every real one, so the key facts are computed from the same door callers use.
fn probe_request(images: Vec<String>) -> llm_cache::Request<'static> {
    llm_cache::Request {
        system: "system",
        user: "user",
        state: "",
        speech: "",
        context: "",
        images,
        run_index: None,
        model: "probe-model",
        thinking: false,
    }
}

/// Rule 6: `cache/llm` is keyed on the exact request, images included.
fn cache_key() -> String {
    let plain = llm_cache::parts(&probe_request(Vec::new()));
    let imaged = llm_cache::parts(&probe_request(vec!["data:a".into(), "data:b".into()]));
    format!(
        "cache/llm is keyed on the exact request, images included: llm_cache::parts sends {} parts \
         with no image and {} with two (one part per image, positional), and llm_cache::key hashes \
         them (sha256, NUL-pinned) -- an unchanged step costs nothing, an edited prompt misses",
        plain.len(),
        imaged.len()
    )
}

/// Rule 7: failure is specific and local.
fn specific_failure() -> String {
    let message = degraded::forgot_job_message("job7", 404);
    let (outcome, lines) = degraded::finish_render(false, Err("sd.cpp job job7 HTTP 404 gone".into()));
    format!(
        "failure is specific and local: degraded::forgot_job_message names the job and the status \
         (\"{message}\"); degraded::finish_render with a failed thumbnail returns \
         {outcome:?} with {} log line(s) and leaves the render complete -- the rest carries on",
        lines.len()
    )
}

/// Rule 8: a project is a folder ending in `.naivepost`; a path in it is stored `project:` +
/// root-relative, absolute nowhere else. Computed by a real round-trip — and written with no folder at
/// all, because `write_path`/`resolve` are pure naming rules and creating a scratch directory here made
/// two calls of this door in the same second collide on one path (the CLI run and a test run). A test
/// that really creates the folders is `sec_agent_rules_the_code_holds_to_s8_paths_store_project_relative`.
fn path_rule() -> String {
    let root = Path::new("/root");
    let project = root.join("demo.naivepost");
    let inside = project.join("sub").join("take.wav");
    let outside = root.join("other.wav");
    let far = Path::new("/elsewhere/take.wav");
    let wrote = |path: &Path| crate::layout::write_path(root, &project, path);
    let inside_written = wrote(&inside);
    let back = crate::layout::resolve(root, &project, &inside_written);
    let ok = inside_written == "project:sub/take.wav" && back == inside;
    format!(
        "a project is a folder ending in .naivepost and a path in it is stored as project: + \
         root-relative, absolute nowhere else: layout::write_path + layout::resolve say the rule {} -- \
         inside the project {} -> {}, which resolves back to the same path; under the root but outside \
         the project {}; elsewhere, absolute and kept only there ({})",
        if ok { "holds" } else { "BROKEN" },
        inside.display(),
        inside_written,
        wrote(&outside),
        wrote(far)
    )
}

/// Rule 9: tests pin reasons, not only outputs — the shipped wording is reachable for every key.
fn pinned_reasons() -> String {
    let readable: Vec<&str> = prompts::KEYS
        .iter()
        .copied()
        .filter(|key| prompts::shipped_file(key).is_some() && prompts::shipped_text(key).is_some())
        .collect();
    format!(
        "tests pin reasons, not only outputs: prompts::shipped_text answers for {} of {} keys \
         (spec/prompts/<key>.md is the wording those tests quote), so a prompt edit shows up as a \
         test failure rather than a silent behaviour change",
        readable.len(),
        prompts::KEYS.len()
    )
}

/// The whole answer: one line per rule, in the spec's order.
pub fn lines() -> Vec<String> {
    vec![
        models_propose(),
        word_edges(),
        only_marked(),
        resume_marker(),
        params_catalogue(),
        cache_key(),
        specific_failure(),
        path_rule(),
        pinned_reasons(),
    ]
}
