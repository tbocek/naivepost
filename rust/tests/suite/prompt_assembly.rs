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

use crate::common;

/// The env guard the live tests use: `XDG_*` and the retry scale belong to the process, and cargo runs
/// these cases as threads of one process.
static ENV: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// A fake LLM on a loopback port that records every body it was asked with and answers each the same.
struct Fake {
    url: String,
    asks: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
}

impl Fake {
    fn start(answer: String) -> Fake {
        use std::io::{BufRead, BufReader, Read, Write};
        use std::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
        let port = listener.local_addr().expect("an address").port();
        let asks = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let recorder = asks.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let recorder = recorder.clone();
                let answer = answer.clone();
                std::thread::spawn(move || {
                    let mut reader = BufReader::new(stream.try_clone().expect("a socket clone"));
                    let mut line = String::new();
                    let _ = reader.read_line(&mut line);
                    let mut length = 0usize;
                    loop {
                        let mut header = String::new();
                        if reader.read_line(&mut header).ok().filter(|n| *n > 0).is_none()
                            || header.trim().is_empty()
                        {
                            break;
                        }
                        if header.to_ascii_lowercase().starts_with("content-length:") {
                            length = header.split(':').nth(1).unwrap_or("0").trim().parse().unwrap_or(0);
                        }
                    }
                    let mut body = vec![0u8; length];
                    if length > 0 {
                        let _ = reader.read_exact(&mut body);
                    }
                    let payload = answer;
                    let framed = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",
                        payload.len()
                    );
                    let _ = stream.write_all(framed.as_bytes());
                    let _ = stream.flush();
                    if let Ok(mut row) = recorder.lock() {
                        row.push(String::from_utf8_lossy(&body).into_owned());
                    }
                });
            }
        });
        Fake { url: format!("http://127.0.0.1:{port}"), asks }
    }

    /// Wait until `count` asks have been recorded — recording happens on the connection's own thread.
    fn waited(&self, count: usize) -> Vec<String> {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        loop {
            let seen = self.asks.lock().expect("the record lock").clone();
            if seen.len() >= count || std::time::Instant::now() > deadline {
                return seen;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }
}

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
/// (same skip rule as `tests/suite/prompts_catalogue.rs`).
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

// ---- the jobs §8 reached only from their own side ----------------------------------------------------
//
// The tests above prove what `prompt_assembly` does. These prove the program ASKS through it: retakes,
// describe, the cut's user turn and the policy ask each used to build their message at home, so none of
// the table, the jobs-list cut, the preamble rule or the precedence rule reached them. Each test below
// calls the job's own door and asserts §8's clauses on the text that goes out.

/// A settings folder with no edited wording in it, so every read falls back to the shipped file — which is
/// what makes these assertions about §8 rather than about somebody's edits.
fn plain_paths(tag: &str) -> naivepost::settings::Paths {
    let root = std::env::temp_dir().join(format!("naivepost-pa8-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    naivepost::settings::Paths {
        config_dir: root.join("cfg/naivepost"),
        data_dir: root.join("data/naivepost"),
    }
}

/// One shipped job prompt's text, through the same reader the program uses.
fn shipped_job(key: &str) -> String {
    naivepost::prompts::shipped_text(key).unwrap_or_else(|| panic!("spec/prompts/{key}.md ships"))
}

/// Every head `sections_for(key)` names, checked present; every other known head, checked absent. A body
/// of prose could otherwise smuggle a heading's words in, so the check is on the heading lines themselves.
fn assert_heads(cut: &str, key: &str) {
    let want = asm::sections_for(key).expect("this job has a row in the table");
    for head in asm::KNOWN_HEADS {
        let wanted = want.contains(&head);
        // A heading line is the head at the start of a line, not the phrase somewhere in prose.
        let present = cut.lines().any(|line| line.starts_with(head));
        assert_eq!(
            present, wanted,
            "{key}: {head} {}",
            if wanted { "must be on the wire" } else { "must not reach this job" }
        );
    }
}

#[test]
fn sec_09_llm_and_tools_8_prompt_assembly_s10_retakes_system_goes_through_the_table() {
    let paths = plain_paths("retake");
    let got = naivepost::retakes_ask::system_message(&paths, "");
    assert_heads(&got, "retake");
    // The job's own wording rides after the house rules, which is §8's "+" order.
    let job = shipped_job("retake");
    let opener = job.lines().next().expect("the retake prompt opens with a line");
    assert!(got.contains(opener), "the retake wording is missing:\n{got}");
    // The order is decided by where the JOB's own text starts: everything before it is the cut house
    // prompt, which is why §8 says the job prompt is appended rather than interleaved.
    let house = asm::cut_system(&shipped_job("system"), "retake");
    assert!(got.starts_with(house.trim()), "the cut house prompt leads:\n{got}");
    assert!(got.contains(opener), "and the job prompt follows it");
    // The jobs list keeps ONLY this job's line: its own label survives, another job's does not.
    assert!(got.contains("retake:"), "own line missing:\n{got}");
    for (_, label) in asm::JOB_LABEL {
        if label == "retake:" {
            continue;
        }
        assert!(!got.contains(label), "another job's line survived ({label}):\n{got}");
    }
    // And §8's preamble (anything before the first heading) reaches it too.
    assert!(got.contains("automated video editor"), "preamble dropped:\n{got}");
}

#[test]
fn sec_09_llm_and_tools_8_prompt_assembly_s11_describe_system_goes_through_the_table() {
    let paths = plain_paths("describe");
    let got = naivepost::describe_call::system_message(&paths, "");
    assert_heads(&got, "describe");
    let job = shipped_job("describe");
    let opener = job.lines().next().expect("the describe prompt opens with a line");
    assert!(got.contains(opener), "the describe wording is missing:\n{got}");
    assert!(got.contains("describe:"), "own jobs-list line missing:\n{got}");
    for (_, label) in asm::JOB_LABEL {
        if label == "describe:" {
            continue;
        }
        assert!(!got.contains(label), "another job's line survived ({label}):\n{got}");
    }
    // Describe reads frames, not the downstream steps: THE FOUR STEPS and TOOLS stay out (§8's row).
    assert!(!got.lines().any(|line| line.starts_with(STEPS)), "describe must not read THE FOUR STEPS");
    assert!(!got.lines().any(|line| line.starts_with(CUT)), "describe must not read THE CUT");
}

#[test]
fn sec_09_llm_and_tools_8_prompt_assembly_s12_cut_user_message_carries_the_context_and_the_speech_rule() {
    let material_tail = "[0.0s-5.0s] A: the timeline";
    let with = naivepost::ui::cut_suggest_model_run::user_turn("make it shorter", 1200.0, 300.0, material_tail);
    let ctx = with
        .find(narrate_pass::CTX_INTRO)
        .expect("the User Context block opens the user turn");
    let speech = with
        .find(narrate_pass::CTX_SPEECH)
        .expect("the cut carries the speech rule");
    let length = with.find("SESSION LENGTH").expect("then the material");
    let tail = with.find(material_tail).expect("and the timeline last");
    assert!(ctx < speech, "context before the speech rule:\n{with}");
    assert!(speech < length, "the speech rule precedes the material:\n{with}");
    assert!(length < tail, "the material keeps its own order:\n{with}");
    assert!(with.contains("make it shorter"), "the context text itself is there:\n{with}");

    // No context: neither the block nor the rule appears — nothing is said about a block the model will not get.
    let without = naivepost::ui::cut_suggest_model_run::user_turn("   ", 1200.0, 300.0, material_tail);
    assert!(!without.contains("USER CONTEXT"), "empty context leaked the block:\n{without}");
    assert!(!without.contains(narrate_pass::CTX_SPEECH), "empty context leaked the speech rule:\n{without}");
    assert!(without.starts_with("SESSION LENGTH"), "the material stands alone:\n{without}");
}

#[test]
fn sec_09_llm_and_tools_8_prompt_assembly_s13_policy_user_message_keeps_the_refusal_tail() {
    let req = naivepost::policy::request("cut the boring parts");
    let first = naivepost::policy_call::user_turn(&req, &[]);
    assert!(first.starts_with(narrate_pass::CTX_INTRO), "§8's door writes the context block:\n{first}");
    assert!(first.contains("cut the boring parts"), "the context text itself:\n{first}");
    // policy decides nothing about what is spoken, so the speech rule is not its line to carry.
    assert!(!asm::carries_speech_rule(req.prompt), "policy must not carry the speech rule");
    assert!(!first.contains(narrate_pass::CTX_SPEECH), "policy carried a speech rule:\n{first}");
    assert!(first.contains("Set only the fields it settles"), "the setter instruction is part of the material");

    let again = naivepost::policy_call::user_turn(&req, &["target_length is not a field the list allows".into()]);
    assert!(again.starts_with(narrate_pass::CTX_INTRO), "the retry still leads with the context block");
    assert!(again.contains("Your previous answer was refused"), "the refusal note is appended:\n{again}");
    assert!(again.contains("target_length is not a field the list allows"), "and says what was refused");
    assert!(
        again.trim_end().ends_with("Send set_policy again with the field names and values the list allows."),
        "and closes with the correction ask:\n{again}"
    );
}

#[test]
fn sec_09_llm_and_tools_8_prompt_assembly_s14_precedence_rule_reaches_the_new_jobs_only_with_a_context() {
    let paths = plain_paths("precedence");
    for (name, built_with, built_without) in [
        (
            "retakes",
            naivepost::retakes_ask::system_message(&paths, "keep the demos"),
            naivepost::retakes_ask::system_message(&paths, ""),
        ),
        (
            "describe",
            naivepost::describe_call::system_message(&paths, "keep the demos"),
            naivepost::describe_call::system_message(&paths, ""),
        ),
    ] {
        assert!(
            built_with.ends_with(&asm::precedence_rule()),
            "{name}: the precedence rule closes the system message"
        );
        // §8 names the second half specifically: another step's instruction is that step's to act on.
        assert!(built_with.contains(asm::PRECEDENCE_OTHER_JOBS), "{name}: second half missing");
        assert!(!built_without.contains("USER CONTEXT"), "{name}: empty context invented a rule");
        assert!(!built_without.contains(asm::PRECEDENCE_OTHER_JOBS), "{name}: empty context leaked the rule");
    }

    // And the jobs read that context from their OWN project folder, not from a caller who happens to carry
    // it: retakes and describe both resolve it through `naivepost.json`, so §8's branch is decided by the
    // session — which is where a person actually writes the note.
    let (_root, tree, paths) = session_with_context("pa8-ctx", "keep the demos, cut the setup");
    assert_eq!(
        project_context(&tree),
        "keep the demos, cut the setup",
        "the session's own note is what a job reads"
    );
    let from_project = naivepost::retakes_ask::system_message(&paths, &project_context(&tree));
    assert!(from_project.ends_with(&asm::precedence_rule()), "the project's context earned the rule");
    let bare = naivepost::retakes_ask::system_message(&paths, &project_context(&empty_session("pa8-noctx")));
    assert!(!bare.contains("USER CONTEXT"), "a session with no note invents nothing");
}

/// The User Context exactly as the program reads it: the session's own note in `naivepost.json`, which is
/// what `retakes_ask::ask_run` resolves through its private `user_context` before building the system turn.
fn project_context(tree: &naivepost::layout::Tree) -> String {
    naivepost::project::load(tree.dir())
        .map(|project| project.context.clone())
        .unwrap_or_default()
}

/// A session folder whose project has no User Context at all.
fn empty_session(tag: &str) -> naivepost::layout::Tree {
    let (_root, tree, _paths) = session_with_context(tag, "");
    tree
}

/// A session folder whose `naivepost.json` carries a User Context, plus its settings paths. Needed by any
/// door that reads the context from the project rather than being handed it.
fn session_with_context(tag: &str, context: &str) -> (std::path::PathBuf, naivepost::layout::Tree, naivepost::settings::Paths) {
    let root = common::temp_dir_tagged(tag);
    let session = naivepost::startup::session_dir(&root);
    std::fs::create_dir_all(&session).expect("the session folder");
    let paths = naivepost::settings::Paths {
        config_dir: root.join("cfg/naivepost"),
        data_dir: root.join("data/naivepost"),
    };
    let tree = naivepost::layout::Tree::new(session).expect("the session is a project");
    let project = naivepost::project::Project {
        context: context.to_string(),
        ..Default::default()
    };
    naivepost::project::save(&project, tree.dir()).expect("the project is written");
    (root, tree, paths)
}

/// §8 names describe among the jobs that must assemble their system message through the table. It now does:
/// house rules cut to `describe`'s sections, then the `describe` wording whole — and nothing else on the
/// system turn, because the frames ride on the user turn (§06#4), not here.
#[test]
fn sec_09_llm_and_tools_8_prompt_assembly_s11b_describe_system_goes_through_the_table() {
    let built = naivepost::describe_call::system_message(&plain_paths("pa8-desc"), "keep the demos, cut the setup");
    assert_heads(&built, "describe");
    assert!(
        built.contains(shipped_job("describe").lines().next().expect("job opener")),
        "the describe wording is missing"
    );
    assert!(built.ends_with(&asm::precedence_rule()), "a context earned the rule");
    // The frames never appear in the system message (§06#4): this module's share of the request is the
    // system turn plus the clip list, and the list belongs to the caller that owns the clips.
    assert!(!built.contains("data:image"), "no frame belongs on the system turn");
}

/// The wire half: the retakes pool really sends what its door assembles. `ask_run` takes the settings
/// row, so pointing `conf.server` at a loopback fake is the program's own seam — nothing in the program
/// knows a test exists.
#[test]
fn sec_09_llm_and_tools_8_prompt_assembly_s15_retakes_wire_carries_the_assembled_system() {
    let _turn: std::sync::MutexGuard<'_, ()> = ENV
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    // `finish` with no marks is a whole answer, so the run ends after one round and exactly one body
    // reaches the fake — the request whose system message this check reads.
    let fake = Fake::start(
        serde_json::json!({"choices": [{"message": {"content": "", "tool_calls": [
            {"id": "c1", "type": "function", "function": {"name": "finish", "arguments": "{}"}}
        ]}}]})
        .to_string(),
    );
    // A User Context in the project file: §8 says it decides whether the precedence rule rides along, and
    // the wire has to show that decision rather than only a builder.
    let (root, tree, paths) = session_with_context("pa8-wire", "keep the demos, cut the setup");
    let mut conf = naivepost::settings::Conf::default();
    conf.server = fake.url.clone();
    conf.model = "retakes-model".into();
    naivepost::settings::save(&paths, &conf).expect("the conf is written");
    // The env and cwd are process-wide; hold them for this dial only, exactly as the live cache tests do.
    std::env::set_var("NAIVEPOST_RETRY_SCALE", "1000");
    std::env::set_var("XDG_CONFIG_HOME", root.join("cfg"));
    std::env::set_var("XDG_DATA_HOME", root.join("data"));

    use naivepost::tools::retakes::{Line, Marks};
    let lines: Vec<Line> = (1..=6u32)
        .map(|n| Line { n, start: f64::from(n) - 1.0, end: f64::from(n) - 0.5 })
        .collect();
    let speech = naivepost::retakes::speech_seconds(&lines);
    let brief = naivepost::retakes::brief(&lines, &[]);
    let mut marks = Marks::new(lines, speech);
    // `ask_run` builds its system turn from the project's own note (see `user_context` there), which is what
    // puts the precedence rule on this request at all.
    let outcome = naivepost::retakes_ask::ask_run(&tree, &paths, &conf, &brief, 0, &mut marks);
    std::env::remove_var("XDG_CONFIG_HOME");
    std::env::remove_var("XDG_DATA_HOME");
    std::env::remove_var("NAIVEPOST_RETRY_SCALE");
    assert!(
        matches!(outcome, naivepost::retakes_ask::RunOutcome::Answered { .. }),
        "the fake answered: {outcome:?}"
    );
    let asks = fake.waited(1);
    assert_eq!(asks.len(), 1, "one request went out");
    let sent: serde_json::Value = serde_json::from_str(&asks[0]).expect("a JSON chat body");
    let messages = sent["messages"].as_array().expect("the chat messages");
    assert_eq!(messages[0]["role"].as_str(), Some("system"), "the system message leads:\n{sent}");
    // `llm_request::body` writes every turn's content as the typed part array, so the text is the first part.
    let system = messages[0]["content"][0]["text"]
        .as_str()
        .unwrap_or_else(|| panic!("the system turn carries a text part: {sent}"))
        .to_string();
    // What the door assembles is what the socket carried — headings, own jobs-list line, job wording,
    // and the precedence rule the project's context earned.
    assert_heads(&system, "retake");
    assert!(system.contains("retake:"), "own jobs-list line missing:\n{system}");
    assert!(
        system.contains(shipped_job("retake").lines().next().expect("job opener")),
        "the retake wording is missing:\n{system}"
    );
    // §8's last clause reaches the wire too, because the job read its context from the session it was asked about.
    assert!(system.contains(narrate_pass::CTX_INTRO), "no precedence rule on the wire:\n{system}");
    assert!(system.ends_with(&asm::precedence_rule()), "and the rule closes the system turn");
}
