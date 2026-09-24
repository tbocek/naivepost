//! §09-llm-and-tools#8-prompt-assembly — the per-job system cut and the two messages.
//!
//! Every clause of §8 is pinned against a fixture rather than the shipped wording alone, so a rule
//! is tested even where the real prompt happens not to exercise it; the shipped `system.md` is then
//! checked separately so the table cannot drift from what actually ships. Ids named here:
//! `prompt.system`, `prompt.describe`, `prompt.fix`, `prompt.retake`, `prompt.textedit`,
//! `prompt.cut`, `prompt.captions`, `prompt.speed`, `prompt.effects`, `prompt.narrate`,
//! `prompt.translate`, `prompt.youtube`, `prompt.policy`, and `tool:web_search` /
//! `tool:web_read` (what the TOOLS section describes).

use naivepost::narrate_pass;
use naivepost::prompt_assembly as asm;

const ITEM: &str = "§09-llm-and-tools#8-prompt-assembly";

/// The eight sections, spelled as the headings spell them.
const ANSWER: &str = "THE ANSWER";
const MATERIAL: &str = "THE MATERIAL";
const CLOCKS: &str = "THE CLOCKS";
const STEPS: &str = "THE FOUR STEPS";
const CUT: &str = "THE CUT";
const JOBS: &str = "WHAT EACH JOB IS GIVEN";
const TOOLS: &str = "TOOLS";
const NEVER: &str = "NEVER INVENT";

/// A system prompt with one block per known head, plus a preamble before the first heading and an
/// unknown upper-case section at the end. Each job line in the jobs list carries one indented
/// example, which is what makes "cut to this job's own line" observable rather than assumed.
///
/// The bodies are deliberately free of other heads' words: `contains` is the check here, and prose
/// like "the cut picks the segments" inside THE FOUR STEPS would read as THE CUT having survived.
fn fixture() -> String {
    [
        "You are called by an automated video editor, one job per call.",
        "",
        "THE ANSWER",
        "answer-shape prose here",
        "",
        "THE MATERIAL",
        "material prose here",
        "",
        "THE CLOCKS",
        "clock prose here",
        "",
        "THE FOUR STEPS",
        "steps prose here",
        "",
        "THE CUT",
        "segment prose here",
        "",
        // The shipped heading runs on into a lower-case tail; matching is by prefix.
        "WHAT EACH JOB IS GIVEN, AND WHAT IT ANSWERS WITH -- nothing around the answer:",
        "  describe: frames and STATE. Answers two lines:",
        "    EVENT: Calm; the tower fires.",
        "  transcript: N lines of TSV. Answers exactly those N lines.",
        "    1\t2\tspeaker\ttext",
        "  retake: every spoken line, numbered. Answers ABANDONED, strict JSON:",
        "    {\"abandoned\":[{\"from\":3,\"to\":5,\"again\":9}]}",
        "  textedit: ONE join. Answers strict JSON:",
        "    {\"joined\":\"one sentence\"}",
        "  cut: the footage range. Answers SEGMENTS, strict JSON:",
        "    {\"segments\":[{\"start\":1.0,\"end\":9.0}]}",
        "  captions: a few clips. Answers CAPTIONS, strict JSON:",
        "    {\"clips\":[{\"i\":1,\"fx\":[]}]}",
        "  speed: the clips with their lengths. Answers SPEEDS, strict JSON:",
        "    {\"speeds\":[{\"clip\":1,\"rate\":2.0}]}",
        "  effects: every clip of the cut. Answers EFFECTS, strict JSON:",
        "    {\"fx\":[{\"clip\":1,\"kind\":\"zoom\"}]}",
        "  narrate: one block per clip. Answers ENTRIES, strict JSON:",
        "    {\"entries\":[{\"start\":0.0,\"end\":4.0,\"at\":1.0,\"text\":\"hi\"}]}",
        "  translate: the numbered lines of one subtitle track.",
        "    1\tthe translated line",
        "  upload text: the clips of the finished video. Answers three parts:",
        "    TITLE: seven words or fewer",
        "",
        "TOOLS",
        "Some jobs are offered web_search and web_read.",
        "",
        "NEVER INVENT",
        "Only what the material shows.",
        "",
        "HOUSE STYLE",
        "an unknown section that must reach every job",
    ]
    .join("\n")
}

/// The shipped system prompt's fenced `text` block, or `None` when `spec/` is not beside the crate
/// (same skip rule as `tests/prompts_catalogue.rs`).
fn shipped_system_prompt() -> Option<String> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()?
        .join("spec")
        .join("prompts")
        .join("system.md");
    let markdown = std::fs::read_to_string(path).ok()?;
    let open = markdown.find("```text")?;
    let body = &markdown[open + "```text".len()..];
    let close = body.find("```")?;
    Some(body[..close].trim().to_string())
}

#[test]
fn sec_09_llm_and_tools_8_prompt_assembly_s1_prepare_jobs_get_answer_material_clocks_jobs_never() {
    assert_eq!(ITEM, "§09-llm-and-tools#8-prompt-assembly");
    // prompt.describe / prompt.fix / prompt.retake: Prepare writes the lines, so it reads the
    // material and the clocks but nothing downstream of itself.
    for key in ["describe", "fix", "retake"] {
        let cut = asm::cut_system(&fixture(), key);
        assert!(cut.contains(ANSWER), "{key}: {cut}");
        assert!(cut.contains(MATERIAL), "{key}: {cut}");
        assert!(cut.contains(CLOCKS), "{key}: {cut}");
        assert!(cut.contains(JOBS), "{key}: {cut}");
        assert!(cut.contains(NEVER), "{key}: {cut}");
        // Not the later steps' business.
        assert!(!cut.contains(STEPS), "{key} must not read THE FOUR STEPS: {cut}");
        assert!(!cut.contains(CUT), "{key} must not read THE CUT: {cut}");
        // tool:web_search / tool:web_read are not offered to Prepare.
        assert!(!cut.contains(TOOLS), "{key} must not read TOOLS: {cut}");
    }
}

#[test]
fn sec_09_llm_and_tools_8_prompt_assembly_s1b_sentence_jobs_get_only_answer_jobs_never() {
    // prompt.textedit / prompt.translate work on sentences, not pictures: no MATERIAL, no CLOCKS.
    for key in ["textedit", "translate"] {
        let cut = asm::cut_system(&fixture(), key);
        assert!(cut.contains(ANSWER), "{key}: {cut}");
        assert!(cut.contains(JOBS), "{key}: {cut}");
        assert!(cut.contains(NEVER), "{key}: {cut}");
        assert!(!cut.contains(MATERIAL), "{key} gets no THE MATERIAL: {cut}");
        assert!(!cut.contains(CLOCKS), "{key} gets no THE CLOCKS: {cut}");
        assert!(!cut.contains(STEPS), "{key} gets no THE FOUR STEPS: {cut}");
        assert!(!cut.contains(CUT), "{key} gets no THE CUT: {cut}");
        assert!(!cut.contains(TOOLS), "{key} gets no TOOLS: {cut}");
    }
}

#[test]
fn sec_09_llm_and_tools_8_prompt_assembly_s2_downstream_jobs_add_steps_tools_and_the_cut() {
    // prompt.cut additionally reads THE CUT: it produces the segments.
    let cut_job = asm::cut_system(&fixture(), "cut");
    assert!(cut_job.contains(STEPS), "cut: {cut_job}");
    assert!(cut_job.contains(CUT), "cut: {cut_job}");
    // tool:web_search / tool:web_read are offered to cut, narrate and youtube only.
    assert!(cut_job.contains(TOOLS), "cut: {cut_job}");

    for key in ["narrate", "youtube"] {
        let out = asm::cut_system(&fixture(), key);
        assert!(out.contains(STEPS), "{key}: {out}");
        assert!(out.contains(TOOLS), "{key}: {out}");
        // They write over chosen clips but do not choose or check segments.
        assert!(!out.contains(CUT), "{key} must not read THE CUT: {out}");
    }

    // prompt.captions / prompt.effects write over the clips, so they need THE CUT but not the steps
    // or the web tools.
    for key in ["captions", "effects"] {
        let out = asm::cut_system(&fixture(), key);
        assert!(out.contains(CUT), "{key}: {out}");
        assert!(out.contains(MATERIAL) && out.contains(CLOCKS), "{key}: {out}");
        assert!(!out.contains(STEPS), "{key}: {out}");
        assert!(!out.contains(TOOLS), "{key}: {out}");
    }

    // prompt.speed: exactly THE ANSWER, THE CUT and the jobs list. No NEVER INVENT — it answers with
    // rates, and the invention ban is about words and moments.
    let speed = asm::cut_system(&fixture(), "speed");
    assert!(speed.contains(ANSWER), "speed: {speed}");
    assert!(speed.contains(CUT), "speed: {speed}");
    assert!(speed.contains(JOBS), "speed: {speed}");
    assert!(!speed.contains(NEVER), "speed must NOT read NEVER INVENT: {speed}");
    assert!(!speed.contains(MATERIAL), "speed: {speed}");
    assert!(!speed.contains(CLOCKS), "speed: {speed}");
    assert!(!speed.contains(STEPS), "speed: {speed}");
    assert!(!speed.contains(TOOLS), "speed: {speed}");
}

#[test]
fn sec_09_llm_and_tools_8_prompt_assembly_s3_jobs_list_keeps_only_this_jobs_line_and_example() {
    let out = asm::cut_system(&fixture(), "retake");
    // Its own line, with the example indented under it.
    assert!(out.contains("retake: every spoken line, numbered"), "{out}");
    assert!(out.contains("{\"abandoned\""), "its example survives: {out}");
    // Every other job's line AND its example are gone.
    assert!(!out.contains("narrate:"), "{out}");
    assert!(!out.contains("cut: the footage range"), "{out}");
    assert!(!out.contains("{\"segments\""), "{out}");
    assert!(!out.contains("{\"entries\""), "{out}");
    assert!(!out.contains("EVENT: Calm", ), "{out}");
    // The heading itself stays.
    assert!(out.contains(JOBS), "{out}");

    // Two keys whose label differs from the prompt file's name: fix is "transcript:", youtube is
    // "upload text:".
    assert_eq!(asm::job_label("fix"), Some("transcript:"));
    assert_eq!(asm::job_label("youtube"), Some("upload text:"));
    let fixed = asm::cut_system(&fixture(), "fix");
    assert!(fixed.contains("transcript: N lines of TSV"), "{fixed}");
    assert!(!fixed.contains("retake:"), "{fixed}");
    let upload = asm::cut_system(&fixture(), "youtube");
    assert!(upload.contains("upload text: the clips of the finished video"), "{upload}");
    assert!(upload.contains("TITLE: seven words or fewer"), "{upload}");
    assert!(!upload.contains("translate:"), "{upload}");

    // A key with no line in the list contributes nothing there, and no bare heading is left behind.
    assert_eq!(asm::job_label("system"), None);
    assert_eq!(asm::job_label("policy"), None);
}

#[test]
fn sec_09_llm_and_tools_8_prompt_assembly_s4_unknown_heading_and_preamble_reach_every_job() {
    // HOUSE STYLE is in no row of the table, so it goes everywhere — an unrecognised section is
    // assumed to matter. Same for the line before the first heading.
    for key in ["speed", "describe", "translate", "cut"] {
        let out = asm::cut_system(&fixture(), key);
        assert!(
            out.contains("an unknown section that must reach every job"),
            "{key} lost the unknown section: {out}"
        );
        assert!(
            out.contains("automated video editor"),
            "{key} lost the preamble: {out}"
        );
    }
    // And the unknown heading is recognised as a heading at all, not swallowed as prose.
    assert_eq!(asm::head_of("HOUSE STYLE\nsome prose"), Some("?"));
    assert_eq!(asm::head_of("THE CLOCKS\nclock prose"), Some("THE CLOCKS"));
    // A lower-case line is not a heading; neither is a sentence ending in a full stop.
    assert!(!asm::is_heading("just prose"));
    assert!(!asm::is_heading("AN UNKNOWN. WITH A STOP"));
    // Prefix matching keeps the jobs heading's lower-case tail inside that section.
    assert_eq!(asm::head_of("WHAT EACH JOB IS GIVEN, AND WHAT IT ANSWERS WITH"), Some(JOBS));
}

#[test]
fn sec_09_llm_and_tools_8_prompt_assembly_s5_keys_with_no_row_get_the_whole_system_prompt() {
    // prompt.system and prompt.policy have no row: §8's last clause sends them everything.
    for key in ["system", "policy"] {
        assert_eq!(asm::sections_for(key), None, "{key} has no row");
        let out = asm::cut_system(&fixture(), key);
        for head in asm::KNOWN_HEADS {
            assert!(out.contains(head), "{key} lost {head}: {out}");
        }
        assert!(out.contains("HOUSE STYLE"), "{key}: {out}");
        assert!(out.contains("automated video editor"), "{key}: {out}");
        // Whole means whole: every job line still there, nothing cut from the jobs list.
        assert!(out.contains("narrate:") && out.contains("retake:"), "{key}: {out}");
    }
    assert_eq!(asm::KNOWN_HEADS.len(), 8);
}

#[test]
fn sec_09_llm_and_tools_8_prompt_assembly_s6_narrate_order_is_cut_then_prompt_then_addenda_then_rule() {
    // prompt.narrate: the addenda ride on the job's own wording, after the house rules, before the
    // precedence rule.
    let system = fixture();
    let job = "narrate job wording";
    let out = asm::system_message(&system, "narrate", job, "the context", true, true);

    let cut = asm::cut_system(&system, "narrate");
    let i_cut = out.find(&cut).expect("the cut system is first");
    let i_job = out.find(job).expect("the job prompt follows");
    let i_speech = out
        .find(narrate_pass::SPEECH_ADDENDUM)
        .expect("the no-microphone note follows the prompt");
    let i_caps = out
        .find(narrate_pass::CAPTIONS_ADDENDUM)
        .expect("the captions addendum follows it");
    let i_rule = out
        .find(asm::precedence_rule().as_str())
        .expect("the precedence rule is last");
    assert!(i_cut < i_job, "house rules before the job wording");
    assert!(i_job < i_speech, "the prompt before its addenda");
    assert!(i_speech < i_caps, "speech note before the captions addendum");
    assert!(i_caps < i_rule, "the rule rides last");

    // Separators are exactly "\n\n", and each piece appears once.
    assert_eq!(i_job, i_cut + cut.len() + 2, "exactly one blank line between pieces");
    assert_eq!(i_speech, i_job + job.len() + 2);
    assert_eq!(i_caps, i_speech + narrate_pass::SPEECH_ADDENDUM.len() + 2);
    assert_eq!(out.matches(job).count(), 1);
    assert_eq!(out.matches(narrate_pass::SPEECH_ADDENDUM).count(), 1);

    // Addenda are opt-in: neither asked for, neither present.
    let bare = asm::system_message(&system, "narrate", job, "ctx", false, false);
    assert!(!bare.contains(narrate_pass::SPEECH_ADDENDUM), "{bare}");
    assert!(!bare.contains(narrate_pass::CAPTIONS_ADDENDUM), "{bare}");

    // A non-narrate job never gains them, even when the flags are set.
    let other = asm::system_message(&system, "cut", "cut wording", "ctx", true, true);
    assert!(!other.contains(narrate_pass::SPEECH_ADDENDUM), "{other}");
    assert!(!other.contains(narrate_pass::CAPTIONS_ADDENDUM), "{other}");
}

#[test]
fn sec_09_llm_and_tools_8_prompt_assembly_s7_precedence_rule_rides_only_with_a_context() {
    let system = fixture();
    let with = asm::system_message(&system, "cut", "cut wording", "make it shorter", false, false);
    assert!(with.contains("USER CONTEXT"), "{with}");
    // The second half names the other-step clause §8 requires.
    assert!(with.contains("that job's"), "{with}");
    assert!(with.contains("answer your own question"), "{with}");

    // Empty or whitespace-only context: no rule, and no empty header standing over nothing.
    for empty in ["", "   ", "\n\t "] {
        let out = asm::system_message(&system, "cut", "cut wording", empty, false, false);
        assert!(!out.contains("USER CONTEXT"), "empty context leaked a rule: {out}");
        assert!(!out.contains("that job's"), "{out}");
        // What is left is the cut system followed by the job wording, unchanged.
        assert_eq!(
            out,
            format!("{}\n\n{}", asm::cut_system(&system, "cut"), "cut wording"),
            "empty context must change nothing else"
        );
    }
}

#[test]
fn sec_09_llm_and_tools_8_prompt_assembly_s8_user_message_context_then_speech_rule_then_material() {
    let material = "THE CLIPS AND WHAT IS KNOWN ABOUT EACH:\n  CLIP 1 ...";

    // The two jobs that decide what is spoken carry the speech rule; nobody else does.
    for key in ["cut", "narrate"] {
        assert!(asm::carries_speech_rule(key), "{key}");
        let out = asm::user_message("a context", key, material);
        let i_ctx = out.find("a context").expect("context first");
        let i_rule = out
            .find(narrate_pass::CTX_SPEECH)
            .expect("speech rule after the context");
        let i_mat = out.find(material).expect("material last");
        assert!(i_ctx < i_rule, "{key}: {out}");
        assert!(i_rule < i_mat, "{key}: {out}");
        assert_eq!(out.matches(narrate_pass::CTX_SPEECH).count(), 1, "{key}");
    }

    for key in ["describe", "fix", "retake", "textedit", "captions", "speed", "effects", "translate", "youtube"] {
        assert!(!asm::carries_speech_rule(key), "{key}");
        let out = asm::user_message("a context", key, material);
        assert!(!out.contains(narrate_pass::CTX_SPEECH), "{key} gained the rule: {out}");
        assert!(out.contains("a context") && out.contains(material), "{key}: {out}");
    }

    // No context: the material alone, with no header and no rule.
    let alone = asm::user_message("", "cut", material);
    assert_eq!(alone, material, "empty context yields only the material");
    let blank = asm::user_message("  \n ", "narrate", material);
    assert_eq!(blank, material);
    // No material either: nothing at all.
    assert_eq!(asm::user_message("", "cut", ""), "");
}

#[test]
fn sec_09_llm_and_tools_8_prompt_assembly_s9_shipped_wording_has_every_head_the_table_names() {
    // The table cuts `prompt.system` by heading, so the headings it names have to be the ones that
    // ship. Skipped only when spec/ is absent beside the checkout.
    let Some(shipped) = shipped_system_prompt() else {
        eprintln!("no spec/prompts beside this checkout — skipping the shipped-wording check");
        return;
    };
    for head in asm::KNOWN_HEADS {
        assert!(shipped.contains(head), "shipped system.md lacks {head}");
    }
    // The shipped jobs heading carries the lower-case tail the matcher has to tolerate.
    assert!(
        shipped.contains("WHAT EACH JOB IS GIVEN, AND WHAT IT ANSWERS WITH"),
        "the jobs heading changed shape"
    );
    // Every label the cut looks for is really how the shipped list names that job.
    for (_, label) in asm::JOB_LABEL {
        assert!(shipped.contains(label), "shipped jobs list lacks {label}");
    }
    // Cutting the real prompt keeps each job's own line and drops another's.
    let retake = asm::cut_system(&shipped, "retake");
    assert!(retake.contains("retake:"), "real retake line missing");
    assert!(!retake.contains("narrate:"), "another job's line survived: {retake}");
    let speed = asm::cut_system(&shipped, "speed");
    assert!(speed.contains("speed:") && !speed.contains(NEVER), "real speed cut wrong");
    // And the real system prompt's preamble reaches a minimal row.
    assert!(speed.contains("automated video editor"), "preamble dropped: {speed}");
}
