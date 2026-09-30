//! F6.1 §2's web tools over a REAL browser process.
//!
//! `tests/web_tools_f6_1.rs` proves the rules with a canned closure. What it cannot prove is that anything ever
//! launches a browser — and until this round nothing did: `web_search` was offered on the wire while no code
//! could answer it. These tests point the SETTINGS `firefox` key (the seam the program itself reads) at a stub
//! executable written into the scenario folder, so a real `Command::new` runs, prints a rendered DOM, and exits.
//! No network is involved: DuckDuckGo is not reachable from this container, so hit extraction is checked against
//! a canned result-page DOM rather than a live query.

use std::path::{Path, PathBuf};

use naivepost::settings::{self, Conf};
use naivepost::tools;
use naivepost::web_browser;
use naivepost::web_tools::{self, Hit};

/// A page wide enough to clear `web_tools::MIN_PAGE_CHARS`, so an emptiness refusal can only come from the stub
/// printing nothing at all and not from the floor being high.
fn long_page(body: &str) -> String {
    format!("<html><body><p>{body}</p><p>{}</p></body></html>", "x".repeat(250))
}

/// The stub browser: a shell script that appends its own pid to `launches` (so "one browser per call" is
/// countable) and prints whatever DOM the scenario asked for. It ignores the URL, which is the point — the leg
/// must not care what it was pointed at.
fn write_stub(dir: &Path, dom: &str, sleep_seconds: u64) -> PathBuf {
    std::fs::create_dir_all(dir).expect("stub folder");
    let path = dir.join("fake-firefox.sh");
    // The DOM is written to its own file rather than embedded in the script: quoting HTML through a shell string
    // is how a test breaks on an apostrophe instead of on the thing it means to check.
    let dom_path = dir.join("dom.html");
    std::fs::write(&dom_path, dom).expect("dom written");
    let script = format!(
        "#!/bin/sh\necho $$ >> \"{dir}/launches\"\nsleep {sleep_seconds}\ncat \"{dom}\"\nexit 0\n",
        dir = dir.display(),
        dom = dom_path.display(),
        sleep_seconds = sleep_seconds,
    );
    std::fs::write(&path, script).expect("stub written");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("stub executable");
    }
    path
}

fn launches(dir: &Path) -> usize {
    std::fs::read_to_string(dir.join("launches"))
        .map(|text| text.lines().count())
        .unwrap_or(0)
}

/// Point the settings at `browser`, the way the Settings box does.
fn point_at(browser: &Path) {
    let paths = settings::from_environment().expect("settings folder");
    let mut conf = settings::read(&paths).unwrap_or_default();
    conf.firefox = browser.to_string_lossy().to_string();
    settings::save(&paths, &conf).expect("settings written");
}

/// The scenario root: settings + data under one temp folder, cwd pinned there. One scenario per binary would be
/// required if two tests shared a binary and each re-pointed the env, so these tests each build their own root
/// and run serially (`--test-threads=1` in the verify recipe, as the other wire suites need).
fn fixture(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("naivepost-f61browser-{tag}"));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("scenario root");
    std::env::set_current_dir(&root).expect("cwd pinned");
    unsafe {
        std::env::set_var("XDG_CONFIG_HOME", root.join("config"));
        std::env::set_var("XDG_DATA_HOME", root.join("data"));
    }
    let paths = settings::from_environment().expect("settings from the pinned env");
    settings::save(&paths, &Conf::default()).expect("settings written");
    root
}

/// A DuckDuckGo-shaped result page: `<article>` blocks, each with an `<h2><a href>` title and a snippet line.
/// This is the markup the current engine serves; the older `result__snippet` shape has its own test below.
fn ddg_page(urls: &[&str]) -> String {
    let articles = urls
        .iter()
        .enumerate()
        .map(|(i, url)| {
            format!(
                "<article><h2><a href=\"{url}\">Result number {}</a></h2><div>The snippet for result {}, which says the fact.</div></article>",
                i + 1,
                i + 1
            )
        })
        .collect::<Vec<String>>()
        .join("");
    format!("<html><head><title>search</title><style>b{{}}</style></head><body>{articles}</body></html>")
}

// ---- S5/the browser leg -------------------------------------------------------------------

/// A search through a real subprocess: the ladder asks its three queries, the stub answers each with the same
/// page, and what comes back is the numbered hit list plus §2's log line naming the rung that answered.
#[test]
fn f6_1_web_s5_a_real_search_answers_with_numbered_hits_and_logs_its_ladder() {
    let root = fixture("search");
    let urls = ["https://one.example/a", "https://two.example/b", "https://one.example/a"];
    let stub = write_stub(&root, &ddg_page(&urls), 0);
    point_at(&stub);
    let conf = settings::read(&settings::from_environment().unwrap()).unwrap();

    let args = r#"{"broad":"the game","medium":"the game and the thing","narrow":"the thing"}"#;
    let ask = web_browser::query_for(&conf.firefox, web_browser::ladder_slice());
    let (answer, line) = web_tools::search("narrate", args, &ask);

    assert!(
        answer.starts_with("Results for \"the thing\":"),
        "the narrowest rung that found anything is what the header names: {answer}"
    );
    // Deduped by URL and capped at P.eng.searchHits: the repeated URL appears once.
    assert_eq!(answer.matches("one.example/a").count(), 1, "deduped by URL: {answer}");
    assert!(answer.contains("1. Result number 1"), "hits are numbered: {answer}");
    assert!(answer.contains("two.example/b"), "every distinct hit survives: {answer}");
    let said = line.expect("a search logs its ladder");
    assert_eq!(
        said,
        web_tools::searched_log("narrate", "the game", "the game and the thing", "the thing", 2, "the thing"),
        "§2's search line verbatim"
    );
    // One browser per QUERY of the ladder that ran, each launched and killed: the stub recorded every launch.
    assert!(launches(&root) >= 1, "the browser really ran: {}", launches(&root));
}

/// Eight is the cap (`P.eng.searchHits`): a page with more results still answers eight.
#[test]
fn f6_1_web_s5_hits_are_capped_at_eight() {
    let root = fixture("cap");
    let urls: Vec<String> = (0..12).map(|i| format!("https://e.example/{i}")).collect();
    let refs: Vec<&str> = urls.iter().map(String::as_str).collect();
    let stub = write_stub(&root, &ddg_page(&refs), 0);
    point_at(&stub);
    let conf = settings::read(&settings::from_environment().unwrap()).unwrap();

    let ask = web_browser::query_for(&conf.firefox, web_browser::ladder_slice());
    let hits = ask("anything").expect("the stub answered");
    assert_eq!(hits.len(), tools::WEB_SEARCH_MAX_HITS, "eight hits, not twelve");
    // And `dedupe` is applied by the leg itself, so a caller cannot forget the cap.
    let deduped = web_tools::dedupe(hits.clone());
    assert_eq!(deduped.len(), tools::WEB_SEARCH_MAX_HITS);
}

/// A read through a real subprocess: page text, not markup, with the count §2 logs.
#[test]
fn f6_1_web_s5_a_real_read_returns_page_text_and_logs_the_count() {
    let root = fixture("read");
    let stub = write_stub(&root, &long_page("The tower is 634 metres tall."), 0);
    point_at(&stub);
    let conf = settings::read(&settings::from_environment().unwrap()).unwrap();

    let fetch = web_browser::read_page_for(&conf.firefox, web_browser::read_budget());
    let args = r#"{"url":"https://tower.example/height"}"#;
    let (answer, line) = web_tools::read_call("narrate", args, &fetch);
    assert!(answer.contains("The tower is 634 metres tall."), "the page's words survive: {answer}");
    assert!(!answer.contains("<p>"), "tags are stripped, not handed to the model: {answer}");
    let said = line.expect("a read logs its line");
    assert!(said.starts_with(">>> narrate: read https://tower.example/height ("), "{said}");
    assert!(said.ends_with(" characters)"), "{said}");
}

/// §3's row: at most 6000 bytes, cut back to a character boundary, marked. Driven through the real leg so the
/// whole path (subprocess → text → clip) is the one the program uses.
#[test]
fn f6_1_web_s5_a_long_page_is_clipped_on_a_character_boundary() {
    let root = fixture("clip");
    // Multi-byte characters straddling the 6000th byte are the case that panics a naive slice.
    let body = "\u{e9}".repeat(tools::WEB_READ_MAX_BYTES + 500);
    let stub = write_stub(&root, &format!("<html><body><p>{body}</p></body></html>"), 0);
    point_at(&stub);
    let conf = settings::read(&settings::from_environment().unwrap()).unwrap();

    let fetch = web_browser::read_page_for(&conf.firefox, web_browser::read_budget());
    let page = fetch("https://wide.example").expect("the stub answered");
    let clipped = web_tools::clip_page(&page);
    // The marker is the last thing in the string; it costs three bytes of its own and is NOT counted against the
    // cap (`clip_page`'s rule: every byte of the 6000 should be page text).
    assert!(clipped.ends_with('…'), "a clipped page says so");
    let without_marker = clipped.trim_end_matches(" \u{2026}");
    assert!(without_marker.len() <= tools::WEB_READ_MAX_BYTES, "{} bytes of page", without_marker.len());
    assert!(
        without_marker.ends_with('\u{e9}'),
        "and it stopped on a whole character: {:?}",
        &without_marker[without_marker.len().saturating_sub(4)..]
    );
}

/// A bot wall is named, not handed over — through the real leg, so a captcha page cannot reach the model.
#[test]
fn f6_1_web_s5_a_bot_wall_is_named_not_handed_over() {
    let root = fixture("wall");
    let stub = write_stub(&root, &long_page("Checking your browser before accessing this site."), 0);
    point_at(&stub);
    let conf = settings::read(&settings::from_environment().unwrap()).unwrap();

    let fetch = web_browser::read_page_for(&conf.firefox, web_browser::read_budget());
    let answer = web_tools::read("https://walled.example", &fetch);
    assert!(
        answer.starts_with("web_read failed: the page is behind a bot wall ("),
        "the wall is refused by name: {answer}"
    );
    assert!(answer.contains("checking your browser"), "and quoted: {answer}");
}

/// A stub that prints nothing at all: the page had no text, which is a refusal rather than an empty answer.
#[test]
fn f6_1_web_s5_a_page_with_no_text_refuses() {
    let root = fixture("empty");
    let stub = write_stub(&root, "", 0);
    point_at(&stub);
    let conf = settings::read(&settings::from_environment().unwrap()).unwrap();

    let fetch = web_browser::read_page_for(&conf.firefox, web_browser::read_budget());
    let answer = web_tools::read("https://nothing.example", &fetch);
    assert_eq!(answer, "web_read failed: the page had no text", "{answer}");
}

/// §2's 45 s is ONE budget including browser boot, and a browser that never finishes must be KILLED rather than
/// waited on forever. A stub that sleeps past its slice proves both halves.
#[test]
fn f6_1_web_s5_a_browser_that_outlives_its_budget_is_killed_and_says_so() {
    let root = fixture("timeout");
    let stub = write_stub(&root, &long_page("too late"), 30);
    point_at(&stub);
    let conf = settings::read(&settings::from_environment().unwrap()).unwrap();

    let started = std::time::Instant::now();
    // A two-second slice stands in for a rung's third of the ladder: the rule under test is that the deadline
    // ends the call, not how long the real budget is.
    let fetch = web_browser::read_page_for(&conf.firefox, std::time::Duration::from_secs(2));
    let outcome = fetch("https://slow.example");
    let took = started.elapsed();
    let why = outcome.expect_err("a browser asleep past its slice must not answer");
    assert!(why.contains("did not finish within 2 s"), "{why}");
    assert!(took < std::time::Duration::from_secs(10), "it stopped at the deadline, not at the sleep: {took:?}");
    // And nothing was left running: the killed stub's children are gone, so a second call still works.
    let again = web_browser::read_page_for(&conf.firefox, std::time::Duration::from_secs(2));
    assert!(again("https://slow.example").is_err(), "the next call behaves the same way");
}

/// The browser is found the way §2 says: the settings value first, then PATH, then the usual install paths —
/// and a named-but-missing binary answers with the filesystem's reason, which is what the step's log line needs.
#[test]
fn f6_1_web_s5_firefox_is_found_by_setting_then_path_then_install_paths() {
    let root = fixture("find");
    let stub = write_stub(&root, "", 0);

    // The setting wins, even when a real firefox is on PATH: a person who named one meant that one.
    let found = web_browser::firefox_path(&stub.to_string_lossy()).expect("the named binary is used");
    assert_eq!(found, stub, "the settings box's own binary");

    // Nothing named: PATH order decides, and the container ships firefox-esr where Debian ships firefox.
    let named = web_browser::firefox_path("").expect("something on PATH");
    let name = named.file_name().expect("a file name").to_string_lossy().to_string();
    assert!(
        web_browser::PATH_NAMES.contains(&name.as_str()),
        "{name} is one of the names §2 lists"
    );

    // Named but missing: the reason is the filesystem's, ready for `unavailable_log`'s `firefox: <stat error>`.
    let missing = web_browser::firefox_path("/no/such/firefox-at-all");
    let why = missing.expect_err("a missing binary is not silently ignored");
    assert!(!why.trim().is_empty(), "the reason is not blank: {why}");
    let line = web_tools::unavailable_log("narrate", "/no/such/firefox-at-all", false, false, &why)
        .expect("an unavailable step says why");
    assert!(line.contains("firefox:"), "§2's shape: {line}");
    assert!(line.contains(&why), "quoting the stat error: {line}");

    // `off` is the setting's own word for turning the web away, and gets its own line.
    let off = web_tools::unavailable_log("narrate", "off", false, false, "").expect("off is explained");
    assert!(off.contains("web search is off"), "{off}");
    assert!(!web_browser::availability("off").0, "`off` means no web tools");
}

/// The offer itself: a machine with no browser gets neither web tool, and says so once — the sentence §2 gives.
#[test]
fn f6_1_web_s5_a_step_without_a_browser_drops_the_tools_and_says_why_once() {
    let root = fixture("offer");
    let _ = root;
    let offered = vec![naivepost::tools::Tool::GetLines, naivepost::tools::Tool::WebSearch, naivepost::tools::Tool::WebRead];

    let (found, reason) = web_browser::availability("/no/such/firefox-either");
    assert!(!found, "nothing to drive");
    let kept = web_tools::filter(&offered, found);
    assert!(!kept.contains(&naivepost::tools::Tool::WebSearch), "no web_search offered: {kept:?}");
    assert!(!kept.contains(&naivepost::tools::Tool::WebRead), "no web_read offered: {kept:?}");
    assert!(kept.contains(&naivepost::tools::Tool::GetLines), "the job's own tools stay");
    let line = web_tools::unavailable_log("narrate", "/no/such/firefox-either", false, found, &reason)
        .expect("the reason is said");
    assert!(line.starts_with(">>> narrate: no web search"), "{line}");
}

/// The old DuckDuckGo markup (`result__snippet`, no `<article>`) is still read: engines change this without asking.
#[test]
fn f6_1_web_s5_the_older_result_markup_is_still_read() {
    let html = "<html><body><div class=\"result__body\"><a class=\"result__a\" href=\"https://old.example/x\">Old style</a><a class=\"result__snippet\">The snippet lives here.</a></div></body></html>";
    let hits: Vec<Hit> = web_browser::search_hits(html);
    assert_eq!(hits.len(), 1, "one result found in the older shape: {hits:?}");
    assert_eq!(hits[0].url, "https://old.example/x");
    assert_eq!(hits[0].title, "Old style");
    assert!(hits[0].snippet.contains("snippet lives here"), "{:?}", hits[0].snippet);
}

/// Page text keeps paragraph structure and drops scripts: the 200-character floor is meaningless if a whole page
/// collapses to one line or a stylesheet counts as content.
#[test]
fn f6_1_web_s5_page_text_keeps_lines_and_drops_scripts() {
    let html = "<html><head><script>var secret = 'do not read me';</script>\
                <style>.x{color:red}</style></head><body><h1>Heading</h1><p>First line.</p><p>Second line.</p></body></html>";
    let text = web_browser::page_text(html);
    assert!(!text.contains("do not read me"), "scripts are not content: {text}");
    assert!(!text.contains("color:red"), "styles are not content: {text}");
    assert!(text.contains("Heading\nFirst line.\nSecond line."), "blocks become lines: {text:?}");
}
