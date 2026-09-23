//! F6.1 — §2's web tools, checked against `naivepost::web_tools`.
//!
//! The loop around them is `tool_protocol_f6_1.rs`; these tests cover what the two tools themselves promise:
//! the descriptions that are the instructions, the ladder that climbs while the web answers, the caps, and the
//! refusal sentences. Both tools are closures here — a canned browser answering with canned hits — so nothing
//! needs a network or a firefox, which this container has neither of.
//!
//! # Ids used
//!
//! `P.eng.searchHits` (8) is the hit cap; the read cap (6000 bytes) and the 45 s per call come from §3's rows
//! and have no `P.` id. Tool names `web_search` / `web_read` are cited as the offer spells them.

use std::path::Path;

use naivepost::tool_loop;
use naivepost::tools::{self, Tool};
use naivepost::web_tools::{self, Hit, Ladder};

const ITEM: &str = "F6.1";

/// A browser's memory of which rungs it was asked for. `Rc` rather than `Arc`: the closure never leaves the
/// thread the test runs on, and a shared handle is what lets the same list be read after the climb.
type Recorder = std::rc::Rc<std::cell::RefCell<Vec<String>>>;

fn hit(url: &str) -> Hit {
    Hit { title: format!("title of {url}"), url: url.to_string(), snippet: "a snippet".to_string() }
}

fn hits(from: usize, count: usize) -> Vec<Hit> {
    (0..count).map(|i| hit(&format!("https://x/{}/{from}", i + 1))).collect()
}

/// `spec/prompts/tools.md`'s fenced block — the wording §2 says ships verbatim. A checkout without `spec/`
/// beside it (an installed tree) skips those comparisons rather than inventing them.
fn shipped_prompts() -> Option<String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../spec/prompts/tools.md");
    std::fs::read_to_string(path).ok()
}

/// The tool's own words as the spec file writes them: `<tool> — <text>`, up to its argument lines. The tool's
/// name and `—` are dropped, since what ships in a schema is the instruction, not the heading naming it.
fn shipped_description(spec: &str, tool: &str) -> String {
    let heading = format!("{tool} \u{2014} ");
    let start = spec.find(&heading).unwrap_or_else(|| panic!("{tool} in prompts/tools.md"));
    let body = &spec[start + heading.len()..];
    let line_end = body.find("\n ").or(body.find("\n```")).expect("the block continues");
    body[..line_end].trim().to_string()
}

/// A page that comes back as it stands, for the wall tests.
fn page(text: &str) -> String {
    let text = text.to_string();
    web_tools::read("https://x", &move |_| Ok(text.clone()))
}

// ---- S1: the descriptions ship with the tools -----------------------------------------

#[test]
fn f6_1_web_s1_descriptions_are_the_prompts_wording() {
    assert_eq!(ITEM, "F6.1");
    // The offer carries a description for these two and for nothing else yet: §3 says its job-specific tools are
    // "to be described by the same rule", so until that wording exists an invented instruction is worse than a
    // bare name.
    assert_eq!(web_tools::description(&Tool::WebSearch), Some(web_tools::WEB_SEARCH_DESCRIPTION));
    assert_eq!(web_tools::description(&Tool::WebRead), Some(web_tools::WEB_READ_DESCRIPTION));
    for tool in [Tool::GetLines, Tool::GetEvents, Tool::Finish, Tool::GetContext, Tool::SetState] {
        assert_eq!(web_tools::description(&tool), None, "{tool:?}");
    }

    // Verbatim against the spec file: these are prompts, and a paraphrase changes what the model is told.
    let Some(spec) = shipped_prompts() else { return };
    assert_eq!(web_tools::WEB_SEARCH_DESCRIPTION, shipped_description(&spec, "web_search"));
    assert_eq!(web_tools::WEB_READ_DESCRIPTION, shipped_description(&spec, "web_read"));

    // The three argument lines the prompt gives, in order, each with what it is copied off.
    let arguments: Vec<&str> = web_tools::WEB_SEARCH_ARGUMENTS.iter().map(|(name, _)| *name).collect();
    assert_eq!(arguments, ["broad", "medium", "narrow"]);
    for (name, text) in web_tools::WEB_SEARCH_ARGUMENTS {
        // The file aligns the names with padding ("broad:  the general subject"), so what has to match is the
        // description after the whitespace — a schema carries no padding.
        let line = spec.find(&format!("{name}:")).and_then(|at| spec[at..].lines().next()).expect("an argument line");
        assert_eq!(line[name.len() + 1..].trim(), text, "{name} is not prompts/tools.md's");
    }
    // And the sentence that makes three queries one call.
    assert!(web_tools::WEB_SEARCH_DESCRIPTION.contains("you get the narrowest one that found anything"));
}

#[test]
fn f6_1_web_s1_the_offer_carries_them_and_nothing_else_gains_one() {
    assert_eq!(ITEM, "F6.1");
    let offered = tools::offered(naivepost::roles::Job::ModelCut);
    let schemas = web_tools::schemas(&offered);
    assert_eq!(schemas.len(), offered.len());

    let named = |name: &str| schemas.iter().find(|s| s["function"]["name"] == serde_json::json!(name));
    // The two web tools arrive with their instructions; every other row of the same offer stays name-only.
    for name in ["web_search", "web_read"] {
        let schema = named(name).unwrap_or_else(|| panic!("{name} offered to a job that gets the web"));
        assert_eq!(schema["type"], serde_json::json!("function"));
        assert_eq!(
            schema["function"]["description"],
            serde_json::json!(web_tools::description(&if name == "web_search" { Tool::WebSearch } else { Tool::WebRead }).expect("these two have one"))
        );
    }
    for name in ["get_lines", "get_events", "finish_cut"] {
        let schema = named(name).unwrap_or_else(|| panic!("{name} offered to the cut job"));
        assert!(schema["function"].get("description").is_none(), "{name} has no written description: {schema}");
    }

    // This is tool_loop::schemas with §2's hook, so the two paths cannot drift.
    assert_eq!(schemas, tool_loop::schemas(&offered, &web_tools::description));
}

// ---- S1/S6: arguments ------------------------------------------------------------------

#[test]
fn f6_1_web_s1_arguments_parse_and_answer_a_bad_one() {
    assert_eq!(ITEM, "F6.1");
    let args = web_tools::SearchArgs::from_json(
        r#"{"broad":"a game","medium":"a game load times","narrow":"\"A Game\" load time"}"#,
    )
    .expect("three rungs");
    assert_eq!((args.broad.as_str(), args.medium.as_str(), args.narrow.as_str()), ("a game", "a game load times", "\"A Game\" load time"));

    // A missing rung is tolerated: the model asked a legitimate question with less to go on.
    let one = web_tools::SearchArgs::from_json(r#"{"broad":"a game"}"#).expect("one rung");
    assert_eq!((one.medium.as_str(), one.narrow.as_str()), ("", ""));

    // A broken argument is an answer naming what is wrong, in tools::error's shape — S6's rule that a problem
    // never ends the flow.
    let broken = web_tools::SearchArgs::from_json("{\"broad\":").expect_err("truncated JSON");
    assert!(broken.starts_with("{\"error\":\"web_search: could not read the arguments:"), "{broken}");
    assert_eq!(broken, tools::error(&format!("web_search: could not read the arguments: {}", serde_json::from_str::<serde_json::Value>("{\"broad\":").unwrap_err())));

    // web_read: a URL or a refusal; blank counts as none given, since there is nothing to navigate to.
    assert_eq!(web_tools::read_url(r#"{"url":"https://x/y"}"#).expect("a url"), "https://x/y");
    assert_eq!(web_tools::read_url(r#"{"url":"   "}"#).unwrap_err(), tools::error("web_read: no url given"));
    assert_eq!(web_tools::read_url("{}").unwrap_err(), tools::error("web_read: no url given"));
}

// ---- The ladder ------------------------------------------------------------------------

#[test]
fn f6_1_web_s2_queries_are_three_at_once_broad_to_narrow() {
    assert_eq!(ITEM, "F6.1");
    // Order is the prompt's: broad, medium, narrow — the climb only makes sense upward.
    assert_eq!(web_tools::queries("the game", "the game load times", "\"A Game\" load time"), ["the game", "the game load times", "\"A Game\" load time"]);

    // A blank or whitespace rung is not a query: it would be searched for, and its emptiness would end the
    // climb early by looking like a web that stopped answering.
    assert_eq!(web_tools::queries("the game", "", "  "), ["the game"]);
    assert!(web_tools::queries("", "", "").is_empty());
    // Trimmed on the way out, so what is searched and what the log names are the same string.
    assert_eq!(web_tools::queries("  the game  ", "", ""), ["the game"]);
}

#[test]
fn f6_1_web_s2_the_ladder_climbs_while_the_web_answers() {
    assert_eq!(ITEM, "F6.1");
    // The narrow rung answers: that is what the model gets, because it is the one that fits the question.
    let asked: Recorder = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let ladder = web_tools::climb(
        &{
            let asked = asked.clone();
            move |query: &str| {
                asked.borrow_mut().push(query.to_string());
                Ok(if query == "narrow" { hits(1, 2) } else { Vec::new() })
            }
        },
        "broad",
        "medium",
        "narrow",
    )
    .expect("a ladder that found something");
    assert_eq!(ladder.used, "narrow");
    assert_eq!(ladder.hits.len(), 2);
    assert_eq!(*asked.borrow(), ["broad", "medium", "narrow"]);

    // A narrower rung finding nothing after a wider one found hits ends the climb with the wider hits — the web
    // stopped answering, so the next rung would not either.
    let asked: Recorder = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let ladder = web_tools::climb(
        &{
            let asked = asked.clone();
            move |query: &str| {
                asked.borrow_mut().push(query.to_string());
                Ok(if query == "broad" { hits(2, 3) } else { Vec::new() })
            }
        },
        "broad",
        "medium",
        "narrow",
    )
    .expect("the wider hits");
    assert_eq!(ladder.used, "broad");
    assert_eq!(ladder.hits.len(), 3);
    assert_eq!(*asked.borrow(), ["broad", "medium"], "stopped at the rung that stopped answering");

    // An erroring query is skipped: one dead rung in three is not news to the model.
    let ladder = web_tools::climb(
        &|query: &str| match query {
            "broad" => Err("navigation timed out".to_string()),
            "medium" => Ok(hits(3, 1)),
            // Nothing below answers, so the climb ends with medium's hits rather than an empty narrow rung.
            _ => Ok(Vec::new()),
        },
        "broad",
        "medium",
        "narrow",
    )
    .expect("the second rung answered");
    assert_eq!(ladder.used, "medium");

    // Every rung erroring is the one case that reports: a broken network must not read as an empty web, or the
    // model concludes the fact does not exist.
    let dead = web_tools::climb(&|_: &str| Err("no route to host".to_string()), "broad", "medium", "narrow");
    assert_eq!(dead.unwrap_err(), "no route to host");

    // Nothing found anywhere is an answer, not an error — and it names no query.
    let none = web_tools::climb(&|_: &str| Ok(Vec::new()), "broad", "medium", "narrow").expect("an empty web");
    assert_eq!(none, Ladder { used: String::new(), hits: Vec::new() });
}

#[test]
fn f6_1_web_s2_eight_hits_and_deduped() {
    assert_eq!(ITEM, "F6.1");
    // P.eng.searchHits: nine in, eight out. The cap is what keeps a search from costing the round its context.
    assert_eq!(tools::WEB_SEARCH_MAX_HITS, 8);
    assert_eq!(web_tools::dedupe(hits(9, 9)).len(), tools::WEB_SEARCH_MAX_HITS);

    // Deduped by URL: an engine returns the same page under two tracking URLs constantly, and a second row for
    // it is a row with no new fact in it.
    let mut repeated = hits(1, 2);
    let second = repeated[1].url.clone();
    let second_again = Hit { url: second, title: "same page, tracking tag".into(), snippet: "again".into() };
    repeated.push(second_again.clone());
    repeated.push(hit("https://x/3"));
    let kept = web_tools::dedupe(repeated);
    assert_eq!(kept.len(), 3, "{kept:?}");
    assert!(!kept.contains(&second_again), "the first of a URL wins: {kept:?}");
    assert!(kept.iter().any(|h| h.url == "https://x/3"), "a new page after the duplicate survives");

    // A hit with nothing in it is not a result.
    let empty = Hit { title: " ".into(), url: String::new(), snippet: "".into() };
    assert!(web_tools::dedupe(vec![empty]).is_empty());
}

#[test]
fn f6_1_web_s2_the_answer_names_the_query_that_worked() {
    assert_eq!(ITEM, "F6.1");
    // `Results for "<query>":` then numbered hits, URL on its own line so a web_read can quote it exactly.
    let text = web_tools::format_hits(&Ladder { used: "the game".to_string(), hits: hits(4, 2) });
    assert_eq!(text, "Results for \"the game\":\n1. title of https://x/1/4\n   https://x/1/4\n   a snippet\n2. title of https://x/2/4\n   https://x/2/4\n   a snippet\n");

    // A query holding quotes survives the header: `"A Game" load time` is one query, and a reader must be able
    // to tell where it started and ended. (Rust's own escaping, which is also what §2's log line uses.)
    let quoted = web_tools::format_hits(&Ladder { used: "\"A Game\" load time".to_string(), hits: hits(1, 1) });
    assert!(quoted.starts_with("Results for \"\\\"A Game\\\" load time\":\n"), "{quoted}");

    // The header quotes the query (§2's log line does: `searched a / b / c -- N result(s) for <used>`), so one
    // that holds quotes survives being read back. Rust's own escaping is what the prototype's `%q` prints.
    let quoted = web_tools::format_hits(&Ladder { used: "\"A Game\" load time".to_string(), hits: hits(1, 1) });
    assert!(quoted.starts_with("Results for \"\\\"A Game\\\" load time\":\n"), "{quoted}");

    // A snippet only when there is one — an empty line the model reads as a gap in its own reading.
    let bare = Ladder { used: "q".to_string(), hits: vec![Hit { title: "t".into(), url: "u".into(), snippet: "  ".into() }] };
    assert_eq!(web_tools::format_hits(&bare), "Results for \"q\":\n1. t\n   u\n");

    // Nothing anywhere: one sentence, and it says three queries were asked. An empty reply reads as a broken
    // tool, and the model stops reaching for it.
    assert_eq!(web_tools::format_hits(&Ladder { used: String::new(), hits: Vec::new() }), "No results for any of the three queries.");
}

// ---- The page --------------------------------------------------------------------------

#[test]
fn f6_1_web_s2_read_is_clipped_on_a_character_boundary() {
    assert_eq!(ITEM, "F6.1");
    // §3's row: page text ≤ 6000 bytes. Under it the page comes back untouched — an ellipsis after a whole page
    // would tell the model the fact might be in text it was never given.
    assert_eq!(tools::WEB_READ_MAX_BYTES, 6000);
    let exact = "a".repeat(tools::WEB_READ_MAX_BYTES);
    assert_eq!(web_tools::clip_page(&exact), exact);

    // Past it: cut at the cap plus the marker, on a boundary.
    let long = "b".repeat(tools::WEB_READ_MAX_BYTES + 500);
    let clipped = web_tools::clip_page(&long);
    assert!(clipped.ends_with(" \u{2026}"), "{} bytes", clipped.len());
    assert_eq!(clipped.len(), tools::WEB_READ_MAX_BYTES + " \u{2026}".len());

    // Multi-byte text: byte 6000 lands inside a character, and slicing there would be invalid UTF-8 — so the cut
    // walks back to a boundary rather than panicking or emitting half a letter.
    let multibyte = "\u{00e9}".repeat(tools::WEB_READ_MAX_BYTES); // 2 bytes each: the cap is mid-character.
    let clipped = web_tools::clip_page(&multibyte);
    let body = clipped.trim_end_matches(" \u{2026}");
    assert!(body.len() <= tools::WEB_READ_MAX_BYTES, "{} bytes", body.len());
    assert_eq!(body.chars().count() * 2, body.len(), "every letter whole");
    assert_ne!(body.len(), tools::WEB_READ_MAX_BYTES - 1, "one byte back would be half a letter");
}

#[test]
fn f6_1_web_s2_a_wall_is_named_not_handed_over() {
    assert_eq!(ITEM, "F6.1");
    // A captcha passed off as "the page's text" ends up written into a narration, so each wall is refused by
    // the phrase that gave it away — enough for the model to pick another result instead.
    assert_eq!(page(""), "web_read failed: the page had no text");
    assert_eq!(page("   \n "), "web_read failed: the page had no text");
    assert_eq!(page("Just a moment..."), "web_read failed: the page is behind a bot wall (just a moment)");
    // Lowercased comparison: walls shout.
    assert_eq!(page("ACCESS DENIED"), "web_read failed: the page is behind a bot wall (access denied)");
    // Short text that is not a named wall is still not a page — 200 characters of navigation has no fact in it.
    assert_eq!(page("A short shell of a page."), "web_read failed: the page had almost no text (blocked, or not loaded)");
    // A fetch that failed is its reason, and the job carries on.
    assert_eq!(web_tools::read("https://x", &|_| Err("navigation timed out".to_string())), "web_read failed: navigation timed out");

    // The bound itself: 200 characters of ordinary text is a page.
    let enough = "n".repeat(web_tools::MIN_PAGE_CHARS);
    assert_eq!(web_tools::page_issue(&enough), None);
    assert!(web_tools::page_issue(&"n".repeat(web_tools::MIN_PAGE_CHARS - 1)).is_some());

    // The list is §2's, kept whole: eleven phrases.
    assert_eq!(web_tools::BOT_WALLS.len(), 11);
    assert!(web_tools::BOT_WALLS.contains(&"too many requests"));
}

// ---- Whether the tools exist at all ---------------------------------------------------

#[test]
fn f6_1_web_s1_no_firefox_or_flatpak_means_no_web_tools() {
    assert_eq!(ITEM, "F6.1");
    // The matrix: a sandbox has no host browser to launch; `off` is the setting's own word for it; an empty
    // setting means nothing was named.
    assert!(!web_tools::offered("firefox", true));
    assert!(!web_tools::offered("", false), "an empty setting names nothing");
    assert!(!web_tools::offered("  ", false));
    assert!(!web_tools::offered("off", false));
    assert!(!web_tools::offered("OFF", false), "the setting is read case-insensitively");
    assert!(web_tools::offered("/usr/bin/firefox", false));

    // With them gone, a job's catalogue loses exactly those two rows and keeps their order.
    let offered = tools::offered(naivepost::roles::Job::Narrate);
    assert!(offered.contains(&Tool::WebSearch) && offered.contains(&Tool::WebRead));
    let offline = web_tools::filter(&offered, false);
    assert!(!offline.contains(&Tool::WebSearch) && !offline.contains(&Tool::WebRead));
    assert_eq!(offline.len(), offered.len() - 2);
    let expected: Vec<Tool> = offered.iter().copied().filter(|t| !matches!(t, Tool::WebSearch | Tool::WebRead)).collect();
    assert_eq!(offline, expected);
    assert_eq!(web_tools::filter(&offered, true), offered);

    // A job the web was never offered to is unaffected either way.
    let policy = tools::offered(naivepost::roles::Job::CleanTranscript);
    assert!(!policy.contains(&Tool::WebSearch));
    assert_eq!(web_tools::filter(&policy, false), policy);

    // And the reason is said once, in §2's words, so a run with no searches in it says why.
    assert_eq!(web_tools::unavailable_log("narrate", "firefox", true, true, ""), Some(">>> narrate: no web search inside Flatpak".to_string()));
    assert_eq!(web_tools::unavailable_log("suggest", "off", false, true, ""), Some(">>> suggest: no web search (web search is off)".to_string()));
    // Nothing set at all and `off` are different news with different fixes, and §2 gives them different lines.
    assert_eq!(
        web_tools::unavailable_log("publish", "", false, true, ""),
        Some(">>> publish: no web search (no firefox found -- name one in the settings, or set it to off)".to_string())
    );
    // A named binary that is not one quotes the filesystem's reason. `off` and empty are unavailable by the
    // setting alone; a path is only unavailable once the filesystem has said so, which is what `stat_error` is.
    let missing = web_tools::unavailable_log("narrate", "/opt/firefox", false, false, "No such file or directory").expect("a reason");
    assert_eq!(missing, ">>> narrate: no web search (firefox: No such file or directory)");
    // A firefox at hand: nothing to say.
    assert_eq!(web_tools::unavailable_log("narrate", "firefox", false, true, ""), None);
}

// ---- The two calls --------------------------------------------------------------------

#[test]
fn f6_1_web_s2_a_search_call_answers_and_logs_its_ladder() {
    assert_eq!(ITEM, "F6.1");
    // §2's line: the three rungs as asked, how many came back, and which rung they came from — that last is
    // what tells a reader the narrow query was useless.
    let (text, log) = web_tools::search(
        "suggest",
        r#"{"broad":"a game","medium":"a game load times","narrow":""}"#,
        &|query: &str| Ok(if query == "a game" { hits(5, 2) } else { Vec::new() }),
    );
    assert!(text.starts_with("Results for \"a game\":"), "{text}");
    assert_eq!(log.expect("a line"), ">>> suggest: searched \"a game\" / \"a game load times\" / \"\" -- 2 result(s) for \"a game\"");

    // A dead ladder says so and the step survives: S6's rule, seen from the tool's side.
    let (text, log) = web_tools::search("narrate", r#"{"broad":"x","medium":"y","narrow":"z"}"#, &|_: &str| Err("no route to host".to_string()));
    assert_eq!(text, "web_search failed: no route to host");
    assert_eq!(log, None, "nothing was searched, so there is nothing to log");

    // A bad argument reaches the model as an error sentence and starts no browser.
    let (text, log) = web_tools::search("publish", "{", &|_: &str| Ok(hits(6, 1)));
    assert!(text.starts_with("{\"error\":\"web_search: could not read the arguments:"), "{text}");
    assert_eq!(log, None);
}

#[test]
fn f6_1_web_s2_a_read_call_answers_and_counts_characters() {
    assert_eq!(ITEM, "F6.1");
    let page = "n".repeat(300);
    let wanted = page.clone();
    let (text, log) = web_tools::read_call("publish", r#"{"url":"https://x/y"}"#, &move |_: &str| Ok(wanted.clone()));
    assert_eq!(text, page);
    let _ = &page;
    assert_eq!(log.expect("a line"), ">>> publish: read https://x/y (300 characters)");

    // The count is of what the model is handed, clipped — that is the cost the log is reporting.
    let long = "z".repeat(tools::WEB_READ_MAX_BYTES + 10);
    let (text, log) = web_tools::read_call("narrate", r#"{"url":"https://x/long"}"#, &move |_: &str| Ok(long.clone()));
    assert!(text.ends_with(" \u{2026}"));
    let url = "https://x/long";
    assert_eq!(log.expect("a line"), format!(">>> narrate: read {url} ({} characters)", text.chars().count()));
    assert!(text.len() > tools::WEB_READ_MAX_BYTES, "the marker is outside the cap, not inside it");

    // No url given: a refusal, and no browser started.
    let (text, log) = web_tools::read_call("narrate", "{}", &|_: &str| Ok(String::new()));
    assert_eq!(text, tools::error("web_read: no url given"));
    assert_eq!(log, None);
}
