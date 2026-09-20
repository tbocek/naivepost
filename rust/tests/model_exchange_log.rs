//! §03-shell#7-the-model-exchange-log-llm — one readable HTML page per run under
//! `llm/`, two log lines per call, and a page link once per run.
//!
//! S1 the page named after the first step, S2 the two lines with sizes and
//! duration, S3 cut-off and empty replies called out, S4 the reply preview,
//! S5 the link once per run, S6 the numbered sections with reasoning folded
//! away, S7 recording never failing the call. All of it is
//! [`naivepost::exchanges`]' decision; the log view only renders it.

use std::fs;
use std::path::PathBuf;

use naivepost::exchanges::{self, Call, Message, Mode, Page, Part, ToolCall};
use naivepost::layout::Tree;

/// A directory under the temp dir that no other test run shares.
fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "naivepost-exchanges-{}-{tag}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// A project folder the page can be filed under.
fn tree(tag: &str) -> Tree {
    Tree::new(temp_dir(tag).join("show.naivepost")).unwrap()
}

fn text(body: &str) -> Vec<Part> {
    vec![Part::Text(body.to_string())]
}

/// A call with `text` bytes of prompt and `reply` bytes of answer.
fn call(step: &str, text_bytes: usize, reply: &str) -> Call {
    Call {
        step: step.into(),
        model: "qwen3-32b".into(),
        mode: Mode::Thinking,
        took_secs: 0,
        thinking_secs: None,
        messages: vec![Message {
            role: "user".into(),
            parts: text(&"x".repeat(text_bytes)),
        }],
        reply: reply.into(),
        reasoning: None,
        tool_calls: Vec::new(),
        cut_off: false,
        error: None,
    }
}

#[test]
fn sec_03_shell_7_the_model_exchange_log_llm_s1_one_page_per_run_named_after_the_first_step() {
    let tree = tree("s1");
    let mut page = Page::start(&tree, "0307-142201", "Describe");

    assert_eq!(page.name(), "0307-142201-Describe.html");
    assert_eq!(page.rel(), "llm/0307-142201-Describe.html");
    assert!(page.path().ends_with("llm/0307-142201-Describe.html"));

    // A run that asked the model nothing leaves no empty page behind. Checked first:
    // §8 writes the page as the request goes out, so a recorded call means a folder.
    let mut quiet = Page::start(&tree, "0307-142201", "Describe");
    assert_eq!(quiet.flush(), None);
    assert!(!tree.dir().join("llm").exists());

    // A later step adds a section, it never renames the page.
    let mut second = call("Narrate", 10, "voiced");
    second.took_secs = 7;
    page.record(&second);
    assert_eq!(page.rel(), "llm/0307-142201-Describe.html");
}

#[test]
fn sec_03_shell_7_the_model_exchange_log_llm_s2_two_log_lines_per_call_with_sizes_and_duration() {
    // The spec's own numbers: 451.4 kB out, 28.6 kB back after 2m39s.
    assert_eq!(exchanges::size_of(462_234), "451.4 kB");
    assert_eq!(exchanges::size_of(29_287), "28.6 kB");
    assert_eq!(exchanges::duration_of(159), "2m39s");
    assert_eq!(exchanges::duration_of(45), "45s");
    assert_eq!(exchanges::duration_of(3_723), "1h02m03s");
    assert_eq!(exchanges::size_of(512), "512 B");
    assert_eq!(exchanges::size_of(3_145_728), "3.0 MB");

    let mut first = call("Describe", 462_234, &"y".repeat(29_287));
    first.took_secs = 159;

    let preview = format!(">>>   the reply begins: {}\u{2026}", "y".repeat(110));
    assert_eq!(
        exchanges::log_lines(&first),
        vec![
            ">>> Describe: 451.4 kB of text and 0 image(s) went to the LLM",
            ">>> Describe: 28.6 kB came back in 2m39s",
            // The preview is the first 110 chars of a 28.6 kB answer.
            preview.as_str(),
        ]
    );
}

#[test]
fn sec_03_shell_7_the_model_exchange_log_llm_s3_cut_off_and_empty_reply_are_called_out_in_the_log() {
    let mut cut = call("Cut", 2_048, "half an answer");
    cut.took_secs = 61;
    cut.cut_off = true;
    assert_eq!(
        exchanges::log_lines(&cut)[1],
        ">>> Cut: 14 B came back in 1m01s \u{2014} cut off at the model's token limit"
    );

    let empty = call("Cut", 2_048, "   ");
    assert_eq!(
        exchanges::log_lines(&empty),
        vec![
            ">>> Cut: 2.0 kB of text and 0 image(s) went to the LLM",
            ">>> Cut: 3 B came back in 0s",
            ">>> Cut: the model answered nothing at all",
        ]
    );

    let failed = Call {
        error: Some("connection refused".into()),
        ..call("Cut", 2_048, "")
    };
    assert_eq!(
        exchanges::log_lines(&failed),
        vec![
            ">>> Cut: 2.0 kB of text and 0 image(s) went to the LLM",
            ">>> Cut: the call failed after 0s: connection refused",
        ]
    );
}

#[test]
fn sec_03_shell_7_the_model_exchange_log_llm_s4_the_reply_preview_skips_the_reasoning_and_the_ellipsis()
{
    assert_eq!(
        exchanges::preview("think    hard</think> Yes,\n  split at 12 s"),
        "Yes, split at 12 s"
    );

    let long = "word ".repeat(200);
    let shown = exchanges::preview(&long);
    assert_eq!(shown.chars().count(), 111);
    assert!(shown.ends_with('\u{2026}'));
    assert!(shown.starts_with("word word word"));

    // The reasoning is in the page, never in the preview.
    let mut thought = call("Describe", 32, "think hard</think> Yes, split at 12 s");
    thought.messages = vec![Message {
        role: "user".into(),
        parts: text("look at these frames"),
    }];
    assert_eq!(
        exchanges::log_lines(&thought)[2],
        ">>>   the reply begins: Yes, split at 12 s"
    );
}

#[test]
fn sec_03_shell_7_the_model_exchange_log_llm_s5_the_page_link_appears_once_per_run() {
    let tree = tree("s5");
    let mut page = Page::start(&tree, "0307-142201", "Describe");

    let first = page.record(&call("Describe", 32, "described"));
    assert_eq!(first.len(), 5, "{first:?}");
    assert_eq!(
        first[1],
        ">>>   this run's exchanges, images included: llm/0307-142201-Describe.html"
    );

    let second = page.record(&call("Narrate", 32, "voiced"));
    assert_eq!(second.len(), 4, "{second:?}");
    assert!(
        !second.iter().any(|line| line.contains("this run's exchanges")),
        "{second:?}"
    );
}

#[test]
fn sec_03_shell_7_the_model_exchange_log_llm_s6_the_page_numbers_its_sections_and_folds_the_reasoning()
{
    let tree = tree("s6");
    let mut page = Page::start(&tree, "0307-142201", "Describe");

    let mut first = call("Describe", 64, "<script>alert(1)</script> was not the answer");
    first.mode = Mode::Thinking;
    first.reasoning = Some("the frame shows a slide".into());
    first.messages = vec![Message {
        role: "user".into(),
        parts: vec![
            Part::Text("<script>alert(1)</script>".into()),
            Part::Image("data:image/jpeg;base64,AAAA".into()),
        ],
    }];
    first.tool_calls = vec![ToolCall {
        name: "web_search".into(),
        args: "{\"q\":\"naivepost\"}".into(),
        result: "3 results".into(),
    }];
    page.record(&first);

    let mut second = call("Narrate", 64, "half a sentence");
    second.mode = Mode::Execute;
    second.took_secs = 120;
    second.cut_off = true;
    page.record(&second);

    assert_eq!(page.flush(), None);
    let html = fs::read_to_string(page.path()).unwrap();

    assert!(html.contains("<title>0307-142201-Describe.html -- Naivepost run</title>"));
    assert!(html.contains("<h2>1. Describe</h2>"));
    assert!(html.contains("<h2>2. Narrate</h2>"));
    assert!(html.contains("qwen3-32b"));
    assert!(html.contains("thinking"));
    assert!(html.contains("execute"));
    assert!(html.contains("<details><summary>reasoning</summary>"));
    assert!(html.contains("<img src=\"data:image/jpeg;base64,AAAA\">"));
    assert!(html.contains("web_search"));
    assert!(html.contains("cut off at the model's token limit"));
    assert!(html.ends_with("</body></html>\n"));
    // A message that is itself markup stays text.
    assert!(html.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
    assert!(!html.contains("<script>alert(1)</script>"));
}

#[test]
fn sec_03_shell_7_the_model_exchange_log_llm_s7_recording_never_fails_the_call() {
    // `llm` already exists as a file, so the folder cannot be made.
    let dir = temp_dir("s7");
    let project = dir.join("show.naivepost");
    fs::create_dir_all(&project).unwrap();
    fs::write(project.join("llm"), "not a folder").unwrap();
    let tree = Tree::new(&project).unwrap();

    let mut page = Page::start(&tree, "0307-142201", "Describe");
    let lines = page.record(&call("Describe", 32, "described"));
    assert_eq!(lines.len(), 5, "{lines:?}");

    let failure = page.flush().expect("a page that cannot be written must say so");
    assert!(failure.starts_with("could not keep the exchange: "), "{failure}");
    assert!(!project.join("llm/0307-142201-Describe.html").exists());
}

// §8 re-words two of §7's verdicts and asks for the page to appear as the call is
// made rather than only at the end. Both live in `Page::begin`/`stream`/`complete`.

#[test]
fn sec_03_shell_8_details_confirmed_against_the_code_verification_pass_s7_the_page_is_written_before_the_reply_arrives()
{
    let tree = tree("s8-progressive");
    let mut page = Page::start(&tree, "0307-142201", "Describe");
    assert!(!page.path().exists(), "naming the page writes nothing");

    let mut call = call("Describe", 64, "");
    call.took_secs = 5;
    let lines = page.begin(&call);
    assert_eq!(lines.len(), 2, "{lines:?}");
    assert_eq!(lines[0], ">>> Describe: 64 B of text and 0 image(s) went to the LLM");
    assert_eq!(
        lines[1],
        ">>>   this run's exchanges, images included: llm/0307-142201-Describe.html"
    );

    // The prompt is on disk already — a run killed here still leaves what it asked.
    let written = fs::read_to_string(page.path()).unwrap();
    assert!(written.contains("<h2>1. Describe</h2>"), "{written}");
    assert!(!written.contains("came back"), "no verdict before the reply");

    page.stream("Split ").unwrap();
    page.stream("at 12 s").unwrap();
    let streamed = fs::read_to_string(page.path()).unwrap();
    assert!(streamed.contains("Split at 12 s"), "{streamed}");

    call.reply = "Split at 12 s".into();
    let verdict = page.complete(&call);
    assert_eq!(verdict[1], ">>> Describe: 13 B came back in 5s", "{verdict:?}");
    let whole = fs::read_to_string(page.path()).unwrap();
    assert_eq!(
        whole.matches("Split at 12 s").count(),
        1,
        "the streamed fragment is replaced, not repeated:\n{whole}"
    );
    assert!(!whole.contains("<!--streamed"), "the marker is internal");

    // Nothing is open any more, so a late chunk goes nowhere.
    assert_eq!(page.stream("late"), None);
    assert_eq!(page.flush(), None);
}

#[test]
fn sec_03_shell_8_details_confirmed_against_the_code_verification_pass_s7_the_verdict_says_how_long_and_what_it_cost()
{
    let mut long_thinker = call("Cut", 4_096, "cut at the word edges");
    long_thinker.took_secs = 182;
    long_thinker.thinking_secs = Some(62);
    assert_eq!(
        exchanges::log_lines(&long_thinker)[1],
        ">>> Cut: 21 B came back in 3m02s, after 1m02s of thinking"
    );

    // A model that does not separate the two gets the plain verdict.
    let plain = call("Cut", 4_096, "cut at the word edges");
    assert_eq!(exchanges::log_lines(&plain)[1], ">>> Cut: 21 B came back in 0s");
}

#[test]
fn sec_03_shell_8_details_confirmed_against_the_code_verification_pass_s7_the_verdict_for_an_answer_of_nothing_and_a_failed_call()
{
    let nothing = call("Narrate", 128, "   ");
    assert_eq!(
        exchanges::log_lines(&nothing),
        vec![
            ">>> Narrate: 128 B of text and 0 image(s) went to the LLM",
            ">>> Narrate: 3 B came back in 0s",
            ">>> Narrate: the model answered nothing at all",
        ]
    );

    let mut failed = Call {
        error: Some("connection refused".into()),
        ..call("Narrate", 128, "")
    };
    failed.took_secs = 90;
    assert_eq!(
        exchanges::log_lines(&failed),
        vec![
            ">>> Narrate: 128 B of text and 0 image(s) went to the LLM",
            ">>> Narrate: the call failed after 1m30s: connection refused",
        ]
    );

    // The page carries the same two notes.
    let tree = tree("s8-notes");
    let mut page = Page::start(&tree, "0307-142201", "Narrate");
    page.record(&nothing);
    page.record(&failed);
    page.flush();
    let html = fs::read_to_string(page.path()).unwrap();
    assert!(html.contains("the model answered nothing at all"), "{html}");
    assert!(html.contains("connection refused"), "{html}");
}
