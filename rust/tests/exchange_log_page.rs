//! §09-llm-and-tools#7-exchange-log — the exchange log page and its app-log verdicts.
//!
//! Spec: `spec/09-llm-and-tools.md` §7. Which clause each test pins:
//!
//! * s1 — one page per run (`<project>/llm/<MMDD-HHMMSS>-<first step>.html`), a run being the
//!   calls between two queue resets, and a call made outside a run getting its own page.
//! * s2 — the section's `h1` "N. step", each message as an `h2` role with text in `pre` and
//!   images inline.
//! * s3 — the meta line: model, thinking/execute, "reply pending" until the answer is in, then
//!   the outcome.
//! * s4 — the reply, reasoning folded in a `details` block, tool calls noted on the reply and
//!   their results reaching the next round as `tool` messages.
//! * s5 — the app log: sizes sent, the verdict wording, and the 110-character preview taken after
//!   the last `<|im_end|>`.
//! * s6 — written when the request goes out, streamed into an open `pre`, rewritten whole at the
//!   end; recording never fails the call.
//!
//! The same module's §03-shell wording tests live in `tests/model_exchange_log.rs`; this file is
//! the §09 §7 pass over the page itself.

use std::fs;
use std::path::PathBuf;

use naivepost::exchanges::{self, Call, Message, Mode, Page, Part, ToolCall, REPLY_PENDING};
use naivepost::layout::Tree;
use naivepost::tool_loop;

/// A directory no other test run shares.
fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("naivepost-xchg09-{tag}-{}", std::process::id()));
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

/// A call with `text_bytes` of prompt and `reply` of answer.
fn call(step: &str, text_bytes: usize, reply: &str) -> Call {
    Call {
        step: step.into(),
        model: "qwen3-32b".into(),
        mode: Mode::Thinking,
        took_secs: 0,
        thinking_secs: None,
        messages: vec![Message { role: "user".into(), parts: text(&"x".repeat(text_bytes)) }],
        reply: reply.into(),
        reasoning: None,
        tool_calls: Vec::new(),
        cut_off: false,
        error: None,
    }
}

// --- S1: one page per run, one page outside a run ------------------------------------------------------

#[test]
fn sec_09_llm_and_tools_7_exchange_log_s1_one_page_per_run_and_one_page_outside_a_run() {
    let first_tree = tree("s1");
    let mut log = exchanges::RunLog::new();
    assert!(!log.is_open());
    assert_eq!(log.page_rel(), None);
    assert!(!first_tree.dir().join("llm").exists(), "nothing recorded, nothing on disk");

    // The first call opens the run's page and gets the once-per-run link.
    let lines = log.record(&first_tree, "0307-142201", "Describe", &call("Describe", 32, "described"));
    assert!(log.is_open());
    assert_eq!(log.page_rel().as_deref(), Some("llm/0307-142201-Describe.html"));
    assert!(
        lines
            .iter()
            .filter(|line| line.starts_with(">>>   this run's exchanges"))
            .count()
            == 1,
        "{lines:?}"
    );

    // A second call joins the same page and does not repeat the link.
    let second = log.record(&first_tree, "0307-142201", "Narrate", &call("Narrate", 40, "voiced"));
    assert_eq!(second.iter().filter(|line| line.contains("this run's exchanges")).count(), 0, "{second:?}");
    assert_eq!(log.page_rel().as_deref(), Some("llm/0307-142201-Describe.html"), "the page keeps its first step's name");
    // Both calls are sections of the one file.
    let html = fs::read_to_string(first_tree.dir().join("llm/0307-142201-Describe.html")).unwrap();
    assert!(html.contains("<h1>1. Describe</h1>") && html.contains("<h1>2. Narrate</h1>"), "{html}");

    // A queue reset ends the run and hands back the finished page's name.
    assert_eq!(log.reset().as_deref(), Some("llm/0307-142201-Describe.html"));
    assert!(!log.is_open());

    // The next call is a new run: its own page, named after ITS first step, with the link again.
    let tree2 = tree("s1b");
    let fresh = log.record(&tree2, "0307-150000", "Narrate", &call("Narrate", 40, "voiced"));
    assert_eq!(log.page_rel().as_deref(), Some("llm/0307-150000-Narrate.html"));
    assert!(fresh.iter().any(|line| line.ends_with("llm/0307-150000-Narrate.html")), "{fresh:?}");
    assert!(tree2.dir().join("llm/0307-150000-Narrate.html").exists());
    assert_eq!(
        fs::read_dir(tree2.dir().join("llm")).unwrap().count(),
        1,
        "one page for the new run"
    );
}

// --- S2: the section heading and the message roles -----------------------------------------------------

#[test]
fn sec_09_llm_and_tools_7_exchange_log_s2_section_heading_and_message_roles() {
    let mut c = call("Describe", 16, "the slide changed");
    c.messages = vec![
        Message { role: "user".into(), parts: text("the prompt words") },
        Message {
            role: "assistant".into(),
            parts: vec![
                Part::Image("data:image/jpeg;base64,AAAA".into()),
                Part::Text("what the picture shows".into()),
            ],
        },
    ];
    let html = exchanges::section_html(3, &c);

    // §7: an `h1` "N. step".
    assert!(html.contains("<h1>3. Describe</h1>"), "{html}");
    // §7: each message as an `h2` role.
    assert!(html.contains("<h2 class=\"role\">user:</h2>"), "{html}");
    assert!(html.contains("<h2 class=\"role\">assistant:</h2>"), "{html}");
    // Text in `pre`, images inline.
    assert!(html.contains("<pre>the prompt words</pre>"), "{html}");
    assert!(html.contains("<img src=\"data:image/jpeg;base64,AAAA\">"), "{html}");

    // Nothing the model or a user typed becomes markup: it is escaped where it is shown.
    let nasty = call("Cut", 8, "<script>alert(1)</script>");
    let safe = exchanges::section_html(1, &nasty);
    assert!(!safe.contains("<script>alert(1)</script>"), "{safe}");
    assert!(safe.contains("&lt;script&gt;alert(1)&lt;/script&gt;"), "{safe}");
}

// --- S3: the meta line, pending then outcome -----------------------------------------------------------

#[test]
fn sec_09_llm_and_tools_7_exchange_log_s3_meta_line_pending_then_outcome() {
    assert_eq!(REPLY_PENDING, "reply pending");

    let c = call("Describe", 32, "");
    let pending = exchanges::meta_line(&c, false);
    assert!(pending.contains(REPLY_PENDING), "{pending}");
    assert!(pending.contains("qwen3-32b"), "{pending}");
    assert!(pending.contains("thinking"), "{pending}");

    // Answered and normal: the duration stands in for the time field.
    let mut done = call("Cut", 32, "cut at 12 s");
    done.mode = Mode::Execute;
    done.took_secs = 45;
    let answered = exchanges::meta_line(&done, true);
    assert!(answered.contains("qwen3-32b") && answered.contains("execute") && answered.contains("45s"), "{answered}");
    assert!(!answered.contains(REPLY_PENDING), "{answered}");

    // Cut off mid-answer.
    let mut chopped = call("Cut", 32, "half an answer");
    chopped.cut_off = true;
    assert!(
        exchanges::meta_line(&chopped, true).contains("cut off at the model's token limit"),
        "{}",
        exchanges::meta_line(&chopped, true)
    );

    // An answer of nothing.
    let blank = call("Cut", 32, "   ");
    assert!(exchanges::meta_line(&blank, true).contains("answered nothing at all"), "{}", exchanges::meta_line(&blank, true));

    // A failed call names why.
    let failed = Call { error: Some("connection refused".into()), ..call("Cut", 32, "") };
    assert!(exchanges::meta_line(&failed, true).contains("connection refused"), "{}", exchanges::meta_line(&failed, true));

    // Through the page: pending while the request is out, the outcome once it is in.
    let t = tree("s3");
    let mut page = Page::start(&t, "0307-142201", "Cut");
    page.begin(&c);
    let opened = fs::read_to_string(page.path()).unwrap();
    assert!(opened.contains(REPLY_PENDING), "{opened}");
    assert!(!opened.contains("the slide changed"), "no reply before there is one: {opened}");

    page.stream("partial").unwrap();
    done.reply = "cut at 12 s".into();
    page.complete(&done);
    let closed = fs::read_to_string(page.path()).unwrap();
    assert!(closed.contains("cut at 12 s"), "{closed}");
    assert!(!closed.contains(REPLY_PENDING), "{closed}");
}

// --- S4: reply, reasoning folded, tool calls noted -----------------------------------------------------

#[test]
fn sec_09_llm_and_tools_7_exchange_log_s4_reply_reasoning_folded_and_tool_calls_noted() {
    let tool = ToolCall {
        name: "get_lines".into(),
        args: "{\"from\":1}".into(),
        result: "1\tword".into(),
    };
    let mut c = call("Cut", 32, "cut at 12 s");
    c.reasoning = Some("thinking about the seam".into());
    c.tool_calls = vec![tool.clone()];
    let html = exchanges::section_html(1, &c);

    // The reply, in a pre.
    assert!(html.contains("<pre>cut at 12 s</pre>"), "{html}");
    // Reasoning folded away rather than sitting in the reply.
    assert!(html.contains("<details><summary>reasoning</summary>"), "{html}");
    assert!(html.contains("thinking about the seam"), "{html}");
    let reply_block = html.split("<details>").next().expect("a details block follows the reply");
    assert!(!reply_block.contains("thinking about the seam"), "reasoning stays out of the reply: {reply_block}");

    // §7's note shape on the reply — built from the crate's own formatter so the two cannot drift.
    // The page escapes it like everything else it shows, so what is searched for is the escaped form.
    let note = tool_loop::call_note(&tool);
    assert!(note.starts_with("[tool call get_lines("), "{note}");
    let shown = note.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;");
    assert!(html.contains(&shown), "note {note:?} not found in:\n{html}");
    // And the result readable on the page.
    assert!(html.contains("1\tword"), "{html}");

    // How the result reaches the next round: a `tool` message carrying it.
    let message = tool_loop::tool_message(&tool);
    assert_eq!(message.role, "tool");
    assert_eq!(message.parts.len(), 1);
    match &message.parts[0] {
        Part::Text(body) => assert_eq!(body, "1\tword"),
        Part::Image(_) => panic!("a tool result is text"),
    }
}

// --- S5: the app log's verdicts and the preview rule ---------------------------------------------------

#[test]
fn sec_09_llm_and_tools_7_exchange_log_s5_app_log_verdicts_and_the_110_char_preview() {
    // Sizes sent.
    let sized = call("Describe", 462_234, "back");
    assert_eq!(
        exchanges::log_lines(&sized)[0],
        ">>> Describe: 451.4 kB of text and 0 image(s) went to the LLM"
    );

    // The verdict with the thinking cost named.
    let mut thinker = call("Cut", 4_096, "cut at the word edges");
    thinker.took_secs = 182;
    thinker.thinking_secs = Some(62);
    assert_eq!(
        exchanges::log_lines(&thinker)[1],
        ">>> Cut: 21 B came back in 3m02s, after 1m02s of thinking"
    );

    // Cut off.
    let mut chopped = call("Cut", 4_096, "half an answer");
    chopped.took_secs = 61;
    chopped.cut_off = true;
    assert!(
        exchanges::log_lines(&chopped)[1].ends_with("\u{2014} cut off at the model's token limit"),
        "{:?}",
        exchanges::log_lines(&chopped)
    );

    // Nothing at all.
    let blank = call("Narrate", 128, "   ");
    assert!(
        exchanges::log_lines(&blank).iter().any(|l| l == ">>> Narrate: the model answered nothing at all"),
        "{:?}",
        exchanges::log_lines(&blank)
    );

    // Failed: exactly two lines, the reason named.
    let failed = Call {
        error: Some("connection refused".into()),
        took_secs: 90,
        ..call("Narrate", 128, "")
    };
    let lines = exchanges::log_lines(&failed);
    assert_eq!(lines.len(), 2, "{lines:?}");
    assert_eq!(lines[1], ">>> Narrate: the call failed after 1m30s: connection refused");

    // The preview is what comes after the LAST `<|im_end|>`, cut at 110 characters.
    let answer = format!("answer {}", "z".repeat(200));
    let long = format!("reasoning one <|im_start|>noise </think>{answer}");
    let c = call("Cut", 64, &long);
    let preview = exchanges::log_lines(&c).pop().expect("a preview line");
    // 110 characters of what came after the LAST `<|im_end|>`, nothing before it.
    assert_eq!(preview, format!(">>>   the reply begins: {}…", answer.chars().take(110).collect::<String>()));
    assert!(!preview.contains("reasoning one"), "the earlier think block is skipped: {preview}");
    assert!(!preview.contains("noise"), "and so is the second one: {preview}");

    // Short enough to show whole, with no ellipsis. The `<|im_start|>` is written with the tag broken
    // up so this file's own text does not read as a chat marker; the string it builds is the same.
    let open_tag = "<".to_string() + "im_start|" + "think" + ">";
    let close_tag = "<".to_string() + "/" + "think" + ">";
    let short = call("Cut", 64, &format!("{open_tag}think{close_tag}done"));
    assert_eq!(exchanges::log_lines(&short).pop().unwrap(), ">>>   the reply begins: done");
}

// --- S6: written at send, streamed into an open pre, rewritten at the end ------------------------------

#[test]
fn sec_09_llm_and_tools_7_exchange_log_s6_written_at_send_streamed_into_an_open_pre_rewritten_at_the_end() {
    let t = tree("s6");
    let mut page = Page::start(&t, "0307-142201", "Cut");
    assert!(!page.path().exists(), "naming the page writes nothing");

    let mut c = call("Cut", 64, "");
    c.took_secs = 5;
    page.begin(&c);
    let sent = fs::read_to_string(page.path()).unwrap();
    assert!(sent.contains(&"x".repeat(64)), "the prompt is on disk before any reply: {sent}");
    assert!(!sent.contains("came back"), "no verdict yet: {sent}");

    // The streamed reply grows inside a `<pre>`, not loose in the section.
    page.stream("Split ").unwrap();
    page.stream("at 12 s").unwrap();
    let streaming = fs::read_to_string(page.path()).unwrap();
    let open = streaming.rfind("<pre>").expect("an open pre holds the stream");
    let close = streaming[open..].find("</pre>").map(|at| open + at).expect("and it is closed");
    let inside = &streaming[open + "<pre>".len()..close];
    assert!(inside.contains("Split at 12 s"), "the reply sits in the pre: {inside:?}");

    // Model text is escaped on the way in: a tag arrives as text, never as markup.
    page.stream("<b>bold</b>").unwrap();
    let escaped = fs::read_to_string(page.path()).unwrap();
    assert!(escaped.contains("&lt;b&gt;"), "{escaped}");
    assert!(!escaped.contains("<b>bold</b>"), "no raw markup from the reply: {escaped}");

    // Finished: the whole file rewritten, one copy of the reply, no internal marker left.
    c.reply = "Split at 12 s".into();
    let verdict = page.complete(&c);
    assert_eq!(verdict[1], ">>> Cut: 13 B came back in 5s", "{verdict:?}");
    let whole = fs::read_to_string(page.path()).unwrap();
    assert_eq!(whole.matches("Split at 12 s").count(), 1, "replaced, not repeated:\n{whole}");
    assert!(!whole.contains("<!--streamed"), "the marker is internal: {whole}");

    // Nothing open any more, so a late chunk goes nowhere.
    assert_eq!(page.stream("late"), None);
    assert_eq!(page.flush(), None);
}
