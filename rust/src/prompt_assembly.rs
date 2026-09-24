//! Prompt assembly — `spec/09-llm-and-tools.md` §8.
//!
//! One call gets two messages. The system message is the "system" prompt cut down to the sections
//! this job can use, then the job's own prompt, then whatever rides on that job alone, then the
//! precedence rule if there is a User Context to outrank anything. The user message is the User
//! Context block, the speech rule for the two jobs that decide what is spoken, and the material.
//!
//! Why a table rather than one long prompt for everyone: every section costs attention. A job that
//! never chooses segments has no use for THE CUT, and a job with no web tools offered has no use
//! for TOOLS — telling it about either invites it to reason about something it cannot act on. The
//! trade is that the table can drift from the shipped wording, which is what the tests in
//! `tests/prompt_assembly.rs` are for: they check each row against the real headings in
//! `spec/prompts/system.md`.
//!
//! Nothing here reads a file. The caller brings the system text and the job's wording (both read by
//! [`crate::settings::stored_prompt`] / [`crate::prompts::shipped_file`]) and the User Context;
//! this module only decides which parts of them reach the model and in what order.
//!
//! Ids this module touches: `prompt.system` (the text being cut), the twelve shipped job prompts
//! plus `policy` ([`crate::prompts::KEYS`]), and `tool:web_search` / `tool:web_read`, which are
//! what the TOOLS section describes.

use crate::narrate_pass;

/// Split into blocks on blank lines, as the shipped prompt writes them.
///
/// Runs of blank lines separate blocks; leading and trailing blanks are dropped so an empty input
/// yields no blocks rather than one empty one. The jobs list is not split further here: in the
/// shipped prompt its heading stands alone above the job lines, so the list body arrives as its own
/// block with a `None` head and [`cut_system`] folds it into the heading block by [`is_job_line`].
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

/// The heading of the jobs list inside the system prompt.
///
/// The shipped line carries a lower-case tail ("WHAT EACH JOB IS GIVEN, AND WHAT IT ANSWERS WITH --
/// nothing around the answer:"), so a heading is matched by prefix on its first line rather than by
/// equality.
pub const JOBS_HEAD: &str = "WHAT EACH JOB IS GIVEN";

/// Every section the per-job table below has an opinion on.
///
/// Ordered longest-first where one head is a prefix of another, so a prefix match cannot land on the
/// wrong one: "THE CLOCKS" must not be read as "THE CUT", and both stay distinct from
/// "THE FOUR STEPS". No entry here prefixes any other, so the order is not load-bearing today —
/// `head_of` still walks this list in order, so keep it sorted longest-first if a new head is added.
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

/// Which sections one job reads. `None` means the key has no row, and a key with no row gets the
/// whole system prompt (§8's last clause) — that is `system` itself and `policy`, neither of which
/// is a section-by-section reader: the first *is* the house rules, the second derives numbers from
/// a description and needs all of the context around them.
///
/// Cross-checked against the prototype's `sysSections` (`gui/syscontext.go`); §8's wording governs.
/// Note `speed` deliberately lacks NEVER INVENT — it answers with rates, and the invention ban is
/// about words and moments.
pub fn sections_for(key: &str) -> Option<&'static [&'static str]> {
    match key {
        // Prepare's three: they write the lines every later job reads, so they need the material and
        // the clocks but not the downstream steps.
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
///
/// Two keys are named differently in the list than in the catalogue because the list speaks of the
/// job, not the prompt file: `fix` is "transcript:" and `youtube` is "upload text:".
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
/// Prefix, not equality: the shipped jobs-list heading runs on into prose, and a known head with a
/// tail is still that section. Returns `Some("")` for nothing recognised but upper-case — see
/// [`is_heading`].
fn known_head(first_line: &str) -> Option<&'static str> {
    let line = first_line.trim();
    KNOWN_HEADS.iter().find(|head| line.starts_with(**head)).copied()
}

/// Whether a block's first line is a heading at all, known or not.
///
/// An unknown section still goes somewhere, so it has to be recognisable: upper-case words on their
/// own line, and not a sentence (which ends in a full stop). This is what lets somebody add a
/// section to the system box without editing the table — §8 sends an unknown heading, and anything
/// before the first heading, to every job.
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
        && line.chars().all(|c| matches!(c, 'A'..='Z' | ' ' | ',' | '-'))
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
/// Blocks are separated by blank lines, as the shipped prompt writes them. A block is kept when its
/// head is wanted by [`sections_for`], when it is a preamble (anything before the first heading —
/// it sets up every section, so dropping it would leave the rest unexplained), or when its heading
/// is unknown (an unrecognised section is assumed to matter, which is also why a key with no row at
/// all keeps everything).
///
/// Inside the jobs list only this job's own line survives, together with the example lines indented
/// under it; every other job's line and its example go (§8: "jobs list cut to this job's own line,
/// its example indented under it"). Because the shipped prompt puts no blank between two jobs'
/// lines, that part is lifted out of the list by [`own_lines_in_jobs_block`] and re-issued under the
/// list's own heading rather than filtered line-by-line out of one long block.
///
/// §8 is silent on whether the jobs-list heading itself survives when the job has no line there (a
/// key like `system`, whose whole prompt is kept anyway): the decision here is that the heading
/// follows its content — a block reduced to nothing contributes nothing, including the heading,
/// because a bare heading over an empty body reads as a rule with no referent.
///
/// Deterministic: blocks keep their original order, nothing is sorted or merged.
pub fn cut_system(system: &str, key: &str) -> String {
    let want = sections_for(key);
    // Fold the jobs list into one block when the shipped layout leaves its body standing apart from
    // its heading: a `None` block whose first line is a job line belongs to the list above it.
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
        // The jobs list. Whole-prompt keys keep it as written; everyone else gets the heading plus
        // only their own line and its examples.
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

/// The jobs-list block with every other job's line taken out.
///
/// The heading is one line of the block and the job lines follow it, so this walks the lines: the
/// heading stays, this job's line stays, the rest go. A blank line inside the list belongs to the
/// job above it — kept or dropped along with that job — because a prompt may write each job as its
/// own paragraph.
fn own_job_line(block: &str, key: &str) -> String {
    let label: &str = match job_label(key) {
        Some(label) => label,
        // Nothing of ours to keep: the block contributes nothing.
        None => "",
    };
    let mut out: Vec<String> = Vec::new();
    let mut mine = false;
    for line in block.lines() {
        if is_heading(line) {
            out.push(line.to_string());
            mine = false;
            continue;
        }
        if line.trim().is_empty() {
            if mine {
                out.push(String::new());
            }
            continue;
        }
        if mine {
            out.push(line.to_string());
        }
    }
    out.join("\n")
}

/// This job's own lines within the jobs list: its line plus every example indented under it, and
/// nothing of anyone else's. Kept separate from [`own_job_line`]'s walk so a caller (and a test)
/// can ask for just this job's part of the shipped list.
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
        // The shipped list writes every job at two spaces ("  retake: every spoken line …"), which
        // is the same indent as an example under some other heading, so a job line is recognised by
        // carrying some job's label rather than by sitting at column zero. Examples are indented
        // further and travel with the line above them.
        if JOB_LABEL.iter().any(|(_, l)| trimmed.starts_with(l)) {
            mine = trimmed.starts_with(label);
        }
        if mine {
            out.push(line.to_string());
        }
    }
    out
}

/// The second half of the precedence rule: what a job does when the User Context talks about
/// something another job makes. Quoted from the prototype's `ctxRule` (`gui/syscontext.go`), which
/// ships as the wording rather than being paraphrased here.
pub const PRECEDENCE_OTHER_JOBS: &str = "The context is written once for the whole session and every job is given the same copy, so it carries instructions for jobs that are not yours: a title, a description, a thumbnail, what the transcript should spell, how long the video should be. An instruction that names a step or a thing another job makes is that job's. Where you meet one, it is not an instruction to you and it does not change what your answer is: answer your own question, in your own shape, and leave it alone.";

/// The precedence rule that rides on the end of the system message when there is a User Context.
///
/// Two halves: the opener [`crate::narrate_pass`] already holds (what the context is and how far it
/// outranks the job's own rules) and [`PRECEDENCE_OTHER_JOBS`] (that a context instruction naming
/// another step is that step's to act on, not this job's). Assembled here rather than written into
/// each prompt file so every wording carries it, including a project's edited copy.
pub fn precedence_rule() -> String {
    format!("{}\n\n{}", narrate_pass::CTX_INTRO, PRECEDENCE_OTHER_JOBS)
}

/// Assemble one call's system message.
///
/// Order: the cut system prompt, the job's own prompt, the narrate-only addenda, the precedence
/// rule. Each piece is joined with "\n\n" and an empty piece adds nothing — so a job with no User
/// Context is told nothing about a block it will not get, matching
/// [`narrate_pass::user_message`]'s rule for the other message.
///
/// The narrate addenda come from [`narrate_pass::system`]: the no-microphone note
/// ([`narrate_pass::SPEECH_ADDENDUM`]) when the clips play what people said out loud, and the
/// captions addendum ([`narrate_pass::CAPTIONS_ADDENDUM`]) for a captions-only voice. They ride on
/// the job's own wording, not on the house rules, because they qualify what the writer is asked to
/// produce rather than how the answer is read.
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
/// Only the two jobs that decide what is spoken: the cut chooses which stretches of speech survive,
/// and narration decides what to say over them. Every other job works on what those two settled.
pub fn carries_speech_rule(key: &str) -> bool {
    matches!(key, "cut" | "narrate")
}

/// Assemble one call's user message: the User Context block, the speech rule where it applies, and
/// the job's material.
///
/// An empty context adds nothing at all — no header over nothing — and the speech rule rides with
/// the context because it is a statement about what the context says regarding the speakers.
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
