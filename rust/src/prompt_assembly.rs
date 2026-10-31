//! Prompt assembly — `spec/09-llm-and-tools.md` §8.
//!
//! One call gets two messages. System: the "system" prompt cut to the sections this job can use, the job's prompt, job-only addenda, then the precedence rule if there is a User Context.
//! User: the User Context block, the speech rule for the two jobs that decide what is spoken, and the material.
//!
//! A per-job section table rather than one prompt for all: every section costs attention. The table can drift from the shipped wording; `tests/suite/prompt_assembly.rs` checks each row against `spec/prompts/system.md`.
//! Nothing here reads a file; the caller brings the texts and the User Context.

use crate::narrate_pass;

/// Split into blocks on blank lines; leading and trailing blanks are dropped, so empty input yields no blocks.
/// The jobs list is not split further: its body arrives as its own `None`-head block and [`cut_system`] folds it in by [`is_job_line`].
fn split_blocks(text: &str) -> Vec<String> {
    let mut blocks: Vec<String> = Vec::new();
    let mut current: Vec<&str> = Vec::new();
    for line in text.lines() {
        if line.trim().is_empty() {
            if !current.is_empty() {
                blocks.push(current.join("\n"));
                current.clear();
            }
        } else {
            current.push(line);
        }
    }
    if !current.is_empty() {
        blocks.push(current.join("\n"));
    }
    blocks
}

/// Whether a line opens with some job's label — i.e. it is one of the jobs list's lines.
pub fn is_job_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    JOB_LABEL.iter().any(|(_, l)| trimmed.starts_with(l))
}

/// The heading of the jobs list inside the system prompt; matched by prefix, since the shipped line has a lower-case tail.
pub const JOBS_HEAD: &str = "WHAT EACH JOB IS GIVEN";

/// Every section the per-job table below has an opinion on.
///
/// Longest-first where one head prefixes another (`head_of` walks in order); no entry prefixes another today.
pub const KNOWN_HEADS: [&str; 8] = [
    "WHAT EACH JOB IS GIVEN",
    "THE FOUR STEPS",
    "NEVER INVENT",
    "THE MATERIAL",
    "THE ANSWER",
    "THE CLOCKS",
    "THE CUT",
    "TOOLS",
];

/// Which sections one job reads. `None` means no row: `system` and `policy` get the whole system prompt (§8's last clause).
///
/// Cross-checked against the prototype's `sysSections` (`go/syscontext.go`); §8's wording governs.
/// `speed` deliberately lacks NEVER INVENT: it answers with rates.
pub fn sections_for(key: &str) -> Option<&'static [&'static str]> {
    match key {
        // Prepare's three write the lines every later job reads: material and clocks, not the downstream steps.
        "describe" | "fix" | "retake" => Some(&[
            "THE ANSWER",
            "THE MATERIAL",
            "THE CLOCKS",
            JOBS_HEAD,
            "NEVER INVENT",
        ]),
        // The two that work on sentences, not on pictures or segments.
        "textedit" | "translate" => Some(&["THE ANSWER", JOBS_HEAD, "NEVER INVENT"]),
        // The jobs that write over clips already chosen: they need THE CUT but not THE FOUR STEPS.
        "captions" | "effects" => Some(&[
            "THE ANSWER",
            "THE MATERIAL",
            "THE CLOCKS",
            "THE CUT",
            JOBS_HEAD,
            "NEVER INVENT",
        ]),
        "cut" => Some(&[
            "THE ANSWER",
            "THE MATERIAL",
            "THE CLOCKS",
            "THE FOUR STEPS",
            "THE CUT",
            JOBS_HEAD,
            "TOOLS",
            "NEVER INVENT",
        ]),
        "narrate" | "youtube" => Some(&[
            "THE ANSWER",
            "THE MATERIAL",
            "THE CLOCKS",
            "THE FOUR STEPS",
            JOBS_HEAD,
            "TOOLS",
            "NEVER INVENT",
        ]),
        "speed" => Some(&["THE ANSWER", "THE CUT", JOBS_HEAD]),
        _ => None,
    }
}

/// How the jobs list names each key's own line.
/// `fix` is "transcript:" and `youtube` is "upload text:" in the list: it speaks of the job, not the prompt file.
pub const JOB_LABEL: [(&str, &str); 11] = [
    ("describe", "describe:"),
    ("fix", "transcript:"),
    ("retake", "retake:"),
    ("textedit", "textedit:"),
    ("cut", "cut:"),
    ("captions", "captions:"),
    ("speed", "speed:"),
    ("effects", "effects:"),
    ("narrate", "narrate:"),
    ("translate", "translate:"),
    ("youtube", "upload text:"),
];

/// The label this key's own line carries in the jobs list, when it has one.
///
/// `system` and `policy` have no line there: neither appears in the list of reply shapes.
pub fn job_label(key: &str) -> Option<&'static str> {
    JOB_LABEL.iter().find(|(k, _)| *k == key).map(|(_, l)| *l)
}

/// Which known head a block's first line opens with, if any.
///
/// Prefix, not equality: the jobs-list heading runs on into prose. `Some("")` for nothing recognised but upper-case (see [`is_heading`]).
fn known_head(first_line: &str) -> Option<&'static str> {
    let line = first_line.trim();
    KNOWN_HEADS
        .iter()
        .find(|head| line.starts_with(**head))
        .copied()
}

/// Whether a block's first line is a heading at all, known or not.
///
/// An unknown section still goes somewhere, so it must be recognisable: upper-case words on their own line, not a sentence. This lets someone add a
/// section to the system box without editing the table (§8 sends an unknown heading, and anything before the first heading, to every job).
pub fn is_heading(first_line: &str) -> bool {
    if known_head(first_line).is_some() {
        return true;
    }
    let line = first_line.trim();
    if line.is_empty() || line.ends_with('.') {
        return false;
    }
    !line.is_empty()
        && line == line.to_uppercase()
        && line
            .chars()
            .all(|c| matches!(c, 'A'..='Z' | ' ' | ',' | '-'))
}

/// The head a block is filed under: a known head, `"?"` for an unknown heading, or `None` when the
/// block is not a heading (a preamble line, or a continuation).
pub fn head_of(block: &str) -> Option<&'static str> {
    let first_line = block.lines().next().unwrap_or("");
    if let Some(head) = known_head(first_line) {
        return Some(head);
    }
    if is_heading(first_line) {
        return Some("?");
    }
    None
}

/// Cut the system prompt down to one job's sections.
///
/// A block is kept when its head is wanted by [`sections_for`], when it is a preamble (before the first heading; it sets up every section), or when its heading is unknown
/// (assumed to matter, which is why a key with no row keeps everything).
///
/// Inside the jobs list only this job's line and its indented examples survive (§8); [`own_lines_in_jobs_block`] lifts them out and re-issues them under the heading,
/// because the shipped prompt puts no blank between two jobs' lines.
///
/// §8 is silent on the jobs-list heading when the job has no line there: the heading follows its content, since a bare heading over an empty body has no referent.
pub fn cut_system(system: &str, key: &str) -> String {
    let want = sections_for(key);
    // Fold the jobs list into one block: a `None` block whose first line is a job line belongs to the list above it.
    let mut blocks: Vec<String> = Vec::new();
    for block in split_blocks(system) {
        if head_of(&block).is_none() && block.lines().next().map(is_job_line).unwrap_or(false) {
            if let Some(last) = blocks.last_mut() {
                last.push('\n');
                last.push_str(&block);
                continue;
            }
        }
        blocks.push(block);
    }

    let mut out: Vec<String> = Vec::new();
    let mut seen_heading = false;

    for block in &blocks {
        let head = head_of(block);
        if head.is_some() {
            seen_heading = true;
        }
        let keep = match head {
            // Before the first heading: the opening lines every section depends on.
            None => !seen_heading,
            Some("?") => true,
            Some(head) => match want {
                // No row for this key: send the whole system prompt.
                None => true,
                Some(rows) => rows.contains(&head),
            },
        };
        if !keep {
            continue;
        }
        if head != Some(JOBS_HEAD) {
            if !block.trim().is_empty() {
                out.push(block.clone());
            }
            continue;
        }
        // The jobs list: whole-prompt keys keep it as written; others get the heading plus only their own line and examples.
        if want.is_none() {
            out.push(block.clone());
            continue;
        }
        let heading_line = block.lines().next().unwrap_or("").to_string();
        let mine = own_lines_in_jobs_block(block, key);
        if mine.is_empty() {
            continue;
        }
        let mut rebuilt = vec![heading_line];
        rebuilt.extend(mine);
        out.push(rebuilt.join("\n"));
    }

    out.join("\n\n")
}

/// This job's own lines within the jobs list: its line plus every example indented under it, nothing of anyone else's.
pub fn own_lines_in_jobs_block(block: &str, key: &str) -> Vec<String> {
    let label: &str = match job_label(key) {
        Some(label) => label,
        None => return Vec::new(),
    };
    let mut out: Vec<String> = Vec::new();
    let mut mine = false;
    for line in block.lines() {
        if is_heading(line) {
            continue;
        }
        let trimmed = line.trim_start();
        if trimmed.is_empty() {
            if mine {
                out.push(String::new());
            }
            continue;
        }
        // Job lines are two-space indented, like examples under other headings, so a job line is recognised by carrying some job's label; deeper-indented examples travel with it.
        if JOB_LABEL.iter().any(|(_, l)| trimmed.starts_with(l)) {
            mine = trimmed.starts_with(label);
        }
        if mine {
            out.push(line.to_string());
        }
    }
    out
}

/// The second half of the precedence rule: what a job does when the User Context talks about something another job makes; the prototype's `ctxRule` (`go/syscontext.go`), quoted.
pub const PRECEDENCE_OTHER_JOBS: &str = "The context is written once for the whole session and every job is given the same copy, so it carries instructions for jobs that are not yours: a title, a description, a thumbnail, what the transcript should spell, how long the video should be. An instruction that names a step or a thing another job makes is that job's. Where you meet one, it is not an instruction to you and it does not change what your answer is: answer your own question, in your own shape, and leave it alone.";

/// The precedence rule that rides on the end of the system message when there is a User Context.
///
/// Two halves: the opener [`crate::narrate_pass`] holds (what the context is and how far it outranks the job's rules) and [`PRECEDENCE_OTHER_JOBS`].
/// Assembled here rather than in each prompt file so every wording carries it, including a project's edited copy.
pub fn precedence_rule() -> String {
    format!("{}\n\n{}", narrate_pass::CTX_INTRO, PRECEDENCE_OTHER_JOBS)
}

/// Assemble one call's system message.
///
/// Order: the cut system prompt, the job's own prompt, the narrate-only addenda, the precedence rule, joined with "\n\n"; an empty piece adds nothing.
/// The addenda come from [`narrate_pass::system`] ([`narrate_pass::SPEECH_ADDENDUM`], [`narrate_pass::CAPTIONS_ADDENDUM`]); they ride on the job's wording because they qualify what the writer produces.
pub fn system_message(
    system: &str,
    key: &str,
    job_prompt: &str,
    context: &str,
    speech: bool,
    captions_only: bool,
) -> String {
    let job = if key == "narrate" {
        narrate_pass::system(job_prompt, speech, captions_only)
    } else {
        job_prompt.to_string()
    };

    let mut pieces: Vec<String> = Vec::new();
    let cut = cut_system(system, key);
    if !cut.trim().is_empty() {
        pieces.push(cut);
    }
    if !job.trim().is_empty() {
        pieces.push(job);
    }
    if !context.trim().is_empty() {
        pieces.push(precedence_rule());
    }
    pieces.join("\n\n")
}

/// Whether this job's user message carries the speech rule.
///
/// Only the two jobs that decide what is spoken: the cut and narration.
pub fn carries_speech_rule(key: &str) -> bool {
    matches!(key, "cut" | "narrate")
}

/// The User Context with its script taken out: the block between a `SCRIPT START` and a `SCRIPT STOP` line, stars
/// What the transcript fixer is shown: with the script in its request the model takes what was said for a mishearing and rewrites the line to match, in every wording tried
/// (measured on the 27B's 08-dex block: with the script 3 lines kept the spoken USDC, without 8). A context with no such block is unchanged.
pub fn without_script(context: &str) -> String {
    let Ok(block) = regex::Regex::new(r"(?s)\*+\s*SCRIPT START\s*\*+.*?\*+\s*SCRIPT STOP\s*\*+")
    else {
        return context.to_string();
    };
    block.replace_all(context, "").into_owned()
}

/// Assemble one call's user message: the User Context block, the speech rule where it applies, and
/// the job's material.
///
/// An empty context adds nothing, no header over nothing; the speech rule rides with the context.
pub fn user_message(context: &str, key: &str, material: &str) -> String {
    let mut pieces: Vec<String> = Vec::new();
    if !context.trim().is_empty() {
        pieces.push(format!("{}\n\n{}", narrate_pass::CTX_INTRO, context.trim()));
        if carries_speech_rule(key) {
            pieces.push(narrate_pass::CTX_SPEECH.to_string());
        }
    }
    if !material.trim().is_empty() {
        pieces.push(material.trim().to_string());
    }
    pieces.join("\n\n")
}
