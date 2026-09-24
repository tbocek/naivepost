//! §09-llm-and-tools#5-context-budgets-new — Context budgets.
//!
//! Spec: `spec/09-llm-and-tools.md` §5. What the rewrite must do is one bound plus two ways of
//! living inside it, and each clause is pinned where it is implemented:
//!
//! - **The bound** (`P.machine.promptMaxChars`, 120 000 per request) lives in
//!   [`naivepost::llm_budget`] and is applied by `llm_request::body` — s1.
//! - **Chars vs bytes**: the budget counts characters, the log reports bytes. s1's second test.
//! - **Batching** — captions 5 clips ([`naivepost::cut_captions`]), translation
//!   `P.machine.translateBatch` lines ([`naivepost::produce_subtitles`]), the cut brief folded one
//!   line per clip ([`naivepost::cut_speed_pass`]) — s2's two tests.
//! - **Reading the rest through tools** — `get_lines` / `get_events`
//!   ([`naivepost::tools::offered`]) — s3.
//! - **The size is logged, not put on the exchange page** — s4. The wording itself is already
//!   pinned whole by `tests/model_exchange_log.rs`; what this adds is that the same figure does
//!   NOT appear in the page's HTML.

use naivepost::exchanges::{self, Call, Message, Mode, Part};
use naivepost::llm_budget::{self, PROMPT_MAX_CHARS};
use naivepost::params;
use naivepost::produce_subtitles as subs;
use naivepost::services::Server;
use naivepost::tools::{self, clips::Clips, Tool};
use naivepost::{cut_captions, cut_speed_pass, roles};

/// Messages of `n` ASCII characters: one text part each, so a size is easy to state exactly.
fn ascii_messages(count: usize, parts_each: usize) -> Vec<Message> {
    let per = count / parts_each.max(1);
    (0..parts_each)
        .map(|i| Message {
            role: "user".to_string(),
            parts: vec![Part::Text("x".repeat(per + if i == 0 { count - per * parts_each } else { 0 }))],
        })
        .collect()
}

// --- S1: the bound -------------------------------------------------------------------------------------

#[test]
fn sec_09_llm_and_tools_5_context_budgets_new_s1_every_request_is_bounded() {
    // P.machine.promptMaxChars — §10 §1: the most one request may carry.
    assert_eq!(PROMPT_MAX_CHARS, 120_000);
    let row = params::find("P.machine.promptMaxChars").expect("the cap is catalogued");
    assert_eq!(row.spelled, "120000", "spelled plain, as P.eng.thumbnailJPEGMax spells its cap");
    assert_eq!(row.from, "llm_budget::PROMPT_MAX_CHARS", "one home for the number");
    assert_eq!(params::family("P.machine.promptMaxChars"), params::Family::Machine);

    // Inclusive boundary: exactly the cap goes, one char more does not.
    assert!(llm_budget::fits(0));
    assert!(llm_budget::fits(PROMPT_MAX_CHARS));
    assert!(!llm_budget::fits(PROMPT_MAX_CHARS + 1));

    let under = llm_budget::check(PROMPT_MAX_CHARS);
    assert!(under.is_ok(), "the cap itself is inside the budget: {under:?}");
    let over = llm_budget::check(PROMPT_MAX_CHARS + 1).unwrap_err();
    // The message names the size, the limit, and both ways out — "too big" with no remedy is a dead end.
    assert!(over.contains("120001"), "{over}");
    assert!(over.contains("120000"), "{over}");
    assert!(over.contains("get_lines"), "{over}");
    assert!(over.contains("batches"), "{over}");

    // And the request contract refuses it: body() is where every chat request is built, so a call
    // site cannot forget the cap.
    let too_big = ascii_messages(PROMPT_MAX_CHARS + 1, 1);
    let refused = naivepost::llm_request::body("m", &too_big, Mode::Execute, None, false)
        .expect_err("a request over the cap is not sent");
    assert_eq!(refused, over, "body reports the budget's own sentence");

    // Exactly at the cap still builds.
    let at_cap = ascii_messages(PROMPT_MAX_CHARS, 2);
    assert!(naivepost::llm_request::body("m", &at_cap, Mode::Execute, None, false).is_ok());
}

#[test]
fn sec_09_llm_and_tools_5_context_budgets_new_s1_chars_counts_characters_not_bytes() {
    // A Cyrillic transcript is twice the bytes of a Latin one and the same context. The budget reads
    // chars so the language costs nothing; the log prints bytes because that is what went on the wire.
    let cyrillic = "я".repeat(1000);
    assert_eq!(cyrillic.len(), 2000, "two bytes per char in UTF-8");
    let messages = vec![Message { role: "user".into(), parts: vec![Part::Text(cyrillic.clone())] }];
    assert_eq!(llm_budget::chars(&messages), 1000, "the budget sees 1000 chars");

    let mut call = Call {
        step: "Describe".into(),
        model: "m".into(),
        mode: Mode::Execute,
        took_secs: 0,
        thinking_secs: None,
        messages,
        reply: String::new(),
        reasoning: None,
        tool_calls: Vec::new(),
        cut_off: false,
        error: None,
    };
    assert_eq!(call.sent().0, 2000, "the log figure is bytes, deliberately not the budget's number");
    // An image is no context at all: it cannot push a request over the cap by itself.
    call.messages.push(Message {
        role: "user".into(),
        parts: vec![Part::Image("data:image/png;base64,AAAA".repeat(50_000))],
    });
    assert_eq!(llm_budget::chars(&call.messages), 1000, "images count zero against the cap");
    assert!(llm_budget::fits(llm_budget::chars(&call.messages)));
}

// --- S2: jobs over the cap run in batches ---------------------------------------------------------------

#[test]
fn sec_09_llm_and_tools_5_context_budgets_new_s2_jobs_over_the_cap_run_in_batches() {
    // Captions: 5 clips per request.
    assert_eq!(cut_captions::BATCH, 5);
    assert_eq!(params::find("P.machine.captionBatch").expect("catalogued").spelled, "5");
    let forty = cut_captions::batches(40);
    assert_eq!(forty.len(), 8, "40 clips is eight requests of five");
    assert!(forty.iter().all(|(first, last)| last - first == 4), "every batch holds five");
    let odd = cut_captions::batches(23);
    assert_eq!(odd.last(), Some(&(21, 23)), "the final batch is short rather than dropped");

    // Translation: P.machine.translateBatch lines per request.
    assert_eq!(subs::BATCH, 150);
    let row = params::produce()
        .into_iter()
        .find(|p| p.id == "P.machine.translateBatch")
        .expect("catalogued in the Produce rows");
    assert_eq!(row.spelled, "150");

    // The prototype's oversized call, reproduced with the real formatter: 778 lines of ~160 chars —
    // §5's "778-line translation call" — is well past the cap as ONE message.
    let track: Vec<(usize, String)> =
        (1..=778).map(|n| (n, format!("{n}: {}", "word ".repeat(32)))).collect();
    let whole = subs::numbered(&track);
    assert!(whole.chars().count() > PROMPT_MAX_CHARS, "the whole track would be refused");
    assert!(!llm_budget::fits(whole.chars().count()));

    // Cut to batches, every request fits and the batches tile the track — nothing is lost by splitting.
    let ranges = subs::batches(track.len());
    assert_eq!(ranges.len(), 6, "778 lines is six requests of at most 150");
    assert!(ranges.iter().all(|(first, last)| last - first <= subs::BATCH));
    assert_eq!(ranges.first(), Some(&(0, 150)));
    assert_eq!(ranges.last(), Some(&(750, 778)));
    let covered: usize = ranges.iter().map(|(f, l)| l - f).sum();
    assert_eq!(covered, 778, "the batches add up to the whole track");
    for range in &ranges {
        let slice: Vec<(usize, String)> = track[range.0..range.1].to_vec();
        let sent = subs::numbered(&slice);
        assert!(
            llm_budget::fits(sent.chars().count()),
            "batch {range:?} is {} chars",
            sent.chars().count()
        );
    }
}

#[test]
fn sec_09_llm_and_tools_5_context_budgets_new_s2_the_cut_brief_is_folded_per_clip() {
    // The cut brief carries one folded line per clip rather than the session's transcript, so its
    // size scales with the number of clips and stays inside the cap whatever was recorded.
    let many: Vec<(u32, f64, f64)> = (1..=200u32).map(|n| (n, f64::from(n) * 12.0, 12.0)).collect();
    let clips = Clips::new(&many);
    let brief = cut_speed_pass::brief(&clips, 2400.0);
    assert!(brief.chars().count() < PROMPT_MAX_CHARS, "a 200-clip brief is {} chars", brief.chars().count());

    // One entry per clip, each naming its own number: the fold is per clip, not one blob.
    let lines: Vec<&str> = brief.lines().filter(|line| line.starts_with("CLIP ")).collect();
    assert_eq!(lines.len(), 200, "one line per clip");
    assert!(lines[0].starts_with("CLIP 1:"), "{}", lines[0]);
    assert!(lines[199].starts_with("CLIP 200:"), "{}", lines[199]);
    // Plus the header lines above the list (the footage total and the list's own heading).
    let total = brief.lines().count();
    assert_eq!(total, 200 + 3, "the two headers and a blank line above the clip list");

    // The effects pass folds the same way: one line per clip, with the speed noted on it.
    let speeds: Vec<(u32, f64)> = (1..=200u32).map(|n| (n, 2.0)).collect();
    let fx_brief = naivepost::cut_effects_pass::brief(&clips, &speeds);
    assert!(fx_brief.chars().count() < PROMPT_MAX_CHARS);
    let fx_lines: Vec<&str> = fx_brief.lines().filter(|line| line.starts_with("CLIP ")).collect();
    assert_eq!(fx_lines.len(), 200);
    assert!(fx_lines[7].contains("plays at 2x"), "{}", fx_lines[7]);
}

// --- S3: the rest is read through tools -----------------------------------------------------------------

#[test]
fn sec_09_llm_and_tools_5_context_budgets_new_s3_the_rest_is_read_through_tools() {
    // The other half of the budget rule: what is not sent may be asked for. Every job the LLM
    // answers is offered the two reading tools §5 names.
    let llm_jobs: Vec<roles::Job> = roles::Job::all()
        .into_iter()
        .filter(|job| roles::server(*job) == Server::Llm)
        .collect();
    assert_eq!(llm_jobs.len(), 9, "four audio jobs go to audio.cpp and the thumbnail to sd.cpp");

    for job in &llm_jobs {
        let offered = tools::offered(*job);
        assert!(offered.contains(&Tool::GetEvents), "{job:?} cannot read events: {offered:?}");
        if *job == roles::Job::Describe {
            // §3.2's Reads row: a chunk of frames has no transcript lines to read, so get_lines is
            // dropped for it and get_frames stands in. Asserted rather than papered over.
            assert!(!offered.contains(&Tool::GetLines), "{job:?} has no lines to offer");
            assert!(offered.contains(&Tool::GetFrames), "{job:?} reads frames instead");
        } else {
            assert!(offered.contains(&Tool::GetLines), "{job:?} cannot read lines: {offered:?}");
        }
    }
}

// --- S4: the size is logged, not put on the page --------------------------------------------------------

#[test]
fn sec_09_llm_and_tools_5_context_budgets_new_s4_size_logged_not_on_the_page() {
    let text = "z".repeat(1200);
    let images = vec![
        Part::Image("data:image/jpeg;base64,AAA".to_string()),
        Part::Image("data:image/jpeg;base64,BBB".to_string()),
    ];
    let call = Call {
        step: "Describe".into(),
        model: "vision".into(),
        mode: Mode::Thinking,
        took_secs: 3,
        thinking_secs: None,
        messages: vec![
            Message { role: "user".into(), parts: vec![Part::Text(text.clone())] },
            Message { role: "user".into(), parts: images },
        ],
        reply: "an answer".into(),
        reasoning: None,
        tool_calls: Vec::new(),
        cut_off: false,
        error: None,
    };
    // The app log gets the size: 1200 bytes of text is 1.2 kB, and two pictures.
    let lines = exchanges::log_lines(&call);
    assert_eq!(lines[0], ">>> Describe: 1.2 kB of text and 2 image(s) went to the LLM");
    assert_eq!(lines[0], format!(">>> {}: {} of text and 2 image(s) went to the LLM", call.step, exchanges::size_of(text.len())));

    // The exchange page never says it: §5 puts the size in the log, not on the page.
    let page = exchanges::section_html(1, &call);
    assert!(!page.contains("went to the LLM"), "{page}");
    assert!(!page.contains("of text"), "{page}");
    // The page does show what was sent, which is the point of having it at all.
    assert!(page.contains(&text), "the page shows the text itself");
    assert!(page.contains("<img"), "and the pictures");
}
