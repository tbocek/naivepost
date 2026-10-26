//! F6.1 §2's web tools over a REAL browser: the Firefox this machine has (firefox-esr in the build container),
//! driven over WebDriver BiDi exactly as the program drives it, reading pages from a local HTTP server.
//!
//! The first version of this file pointed the settings at a shell script that printed a page to stdout,
//! imitating `firefox --dump-dom`. Firefox has no such flag, so every check passed while the program's own
//! searches and reads all timed out. A real browser is the only thing that can answer for one. Nothing here
//! touches the network: DuckDuckGo's result markup is served from 127.0.0.1, and the search door is asked for
//! that page ([`web_browser::search_with`]), the same door `query_for` uses with the engine's address.
//!
//! # Why one scenario at a time
//!
//! The settings file, `cwd` and the XDG folders are process-wide, so two checks may not be inside their
//! scenarios at the same moment ([`fixture`]'s guard, the same shape `tests/suite/wire.rs` needs).

use std::path::{Path, PathBuf};

use naivepost::settings::{self, Conf};
use naivepost::tools;
use naivepost::web_browser;
use naivepost::web_tools::{self, Hit};

use crate::common;

/// A page wide enough to clear `web_tools::MIN_PAGE_CHARS`, so an emptiness refusal can only come from the stub
/// printing nothing at all and not from the floor being high.
fn long_page(body: &str) -> String {
    format!(
        "<html><body><p>{body}</p><p>{}</p></body></html>",
        "x".repeat(250)
    )
}

/// The Firefox the checks drive.
fn firefox() -> PathBuf {
    common::firefox()
}

/// A local server holding `pages`: its base address.
fn serve(pages: Vec<(&'static str, String)>) -> String {
    let site = common::Site::start();
    for (path, body) in pages {
        site.set(path, body);
    }
    // The serving thread holds the page table, so the server outlives this handle.
    site.url("")
}

/// Point the settings at `browser`, the way the Settings box does.
fn point_at(browser: &Path) {
    let paths = settings::from_environment().expect("settings folder");
    let mut conf = settings::read(&paths).unwrap_or_default();
    conf.firefox = browser.to_string_lossy().to_string();
    settings::save(&paths, &conf).expect("settings written");
}

/// A scenario: the temp root plus the guard that keeps this the only test inside the process-wide env. Derefs to
/// the root so a caller joins paths onto it and never has to know about the guard.
struct Scenario {
    root: PathBuf,
    /// Dropped last, which is why the folder goes in `Drop`: the removal must happen while no other test can be
    /// writing its own settings into the same place.
    _scenario: std::sync::MutexGuard<'static, ()>,
}

impl std::ops::Deref for Scenario {
    type Target = std::path::Path;
    fn deref(&self) -> &std::path::Path {
        &self.root
    }
}

impl Drop for Scenario {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// The scenario root: settings + data under one temp folder, cwd pinned there, exclusive until the returned
/// scenario is dropped — see the module's "why one scenario at a time".
fn fixture(tag: &str) -> Scenario {
    let scenario = common::scenario_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
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
    Scenario {
        root,
        _scenario: scenario,
    }
}

/// A DuckDuckGo-shaped result page, in the markup the engine serves today: `<article data-testid="result">`
/// blocks, each with its `result-title-a` anchor and a `data-result="snippet"` line. The older `result__a`
/// shape has its own check below.
fn ddg_page(urls: &[&str]) -> String {
    let articles = urls
        .iter()
        .enumerate()
        .map(|(i, url)| {
            format!(
                "<article data-testid=\"result\"><h2><a data-testid=\"result-title-a\" href=\"{url}\">Result number {}</a></h2><div data-result=\"snippet\">The snippet for result {}, which says the fact.</div></article>",
                i + 1,
                i + 1
            )
        })
        .collect::<Vec<String>>()
        .join("");
    format!("<html><head><title>search</title></head><body>{articles}</body></html>")
}

/// The search door on the local server: what `query_for` does with the engine's address, with this page's.
fn local_search(base: String) -> impl Fn(&str) -> Result<Vec<Hit>, String> {
    move |query: &str| {
        web_browser::search_with(
            &firefox(),
            &format!("{base}/search?q={}", query.replace(' ', "+")),
            web_browser::ladder_slice(),
        )
        .map(web_tools::dedupe)
    }
}

// ---- S5/the browser leg -------------------------------------------------------------------

/// A search through a real browser: the ladder asks its three queries, each page is read for its result
/// anchors, and what comes back is the numbered hit list plus §2's log line naming the rung that answered.
#[test]
fn f6_1_web_s5_a_real_search_answers_with_numbered_hits_and_logs_its_ladder() {
    let _root = fixture("search");
    let urls = [
        "https://one.example/a",
        "https://two.example/b",
        "https://one.example/a",
    ];
    let base = serve(vec![("/search", ddg_page(&urls))]);

    let args = r#"{"broad":"the game","medium":"the game and the thing","narrow":"the thing"}"#;
    let ask = local_search(base.clone());
    let (answer, line) = web_tools::search("narrate", args, &ask);

    assert!(
        answer.starts_with("Results for \"the thing\":"),
        "the narrowest rung that found anything is what the header names: {answer}"
    );
    assert_eq!(
        answer.matches("one.example/a").count(),
        1,
        "deduped by URL: {answer}"
    );
    assert!(
        answer.contains("1. Result number 1"),
        "hits are numbered: {answer}"
    );
    assert!(
        answer.contains("The snippet for result 1"),
        "the snippet is read off the page: {answer}"
    );
    assert!(
        answer.contains("two.example/b"),
        "every distinct hit survives: {answer}"
    );
    let said = line.expect("a search logs its ladder");
    assert_eq!(
        said,
        web_tools::searched_log(
            "narrate",
            "the game",
            "the game and the thing",
            "the thing",
            2,
            "the thing"
        ),
        "§2's search line verbatim"
    );
}

/// Eight is the cap (`P.eng.searchHits`): a page with more results still answers eight.
#[test]
fn f6_1_web_s5_hits_are_capped_at_eight() {
    let _root = fixture("cap");
    let urls: Vec<String> = (0..12).map(|i| format!("https://e.example/{i}")).collect();
    let refs: Vec<&str> = urls.iter().map(String::as_str).collect();
    let base = serve(vec![("/search", ddg_page(&refs))]);

    let hits = local_search(base.clone())("anything").expect("the browser answered");
    assert_eq!(
        hits.len(),
        tools::WEB_SEARCH_MAX_HITS,
        "eight hits, not twelve"
    );
}

/// The old DuckDuckGo markup (`result__a`/`result__snippet`, no `<article>`) is still read: engines change this
/// without asking.
#[test]
fn f6_1_web_s5_the_older_result_markup_is_still_read() {
    let _root = fixture("older");
    let base = serve(vec![(
        "/search",
        "<html><body><div class=\"result__body\"><a class=\"result__a\" href=\"https://old.example/x\">Old style</a><a class=\"result__snippet\">The snippet lives here.</a></div></body></html>".to_string(),
    )]);
    let hits = local_search(base.clone())("old").expect("the browser answered");
    assert_eq!(
        hits.len(),
        1,
        "one result found in the older shape: {hits:?}"
    );
    assert_eq!(hits[0].url, "https://old.example/x");
    assert_eq!(hits[0].title, "Old style");
    assert!(
        hits[0].snippet.contains("snippet lives here"),
        "{:?}",
        hits[0].snippet
    );
}

/// A read through a real browser: page text, not markup, with the count §2 logs.
#[test]
fn f6_1_web_s5_a_real_read_returns_page_text_and_logs_the_count() {
    let _root = fixture("read");
    let base = serve(vec![(
        "/height",
        long_page("The tower is 634 metres tall."),
    )]);

    let fetch =
        web_browser::read_page_for(&firefox().to_string_lossy(), web_browser::read_budget());
    let url = format!("{base}/height");
    let args = format!(r#"{{"url":"{url}"}}"#);
    let (answer, line) = web_tools::read_call("narrate", &args, &fetch);
    assert!(
        answer.contains("The tower is 634 metres tall."),
        "the page's words survive: {answer}"
    );
    assert!(
        !answer.contains("<p>"),
        "tags are stripped, not handed to the model: {answer}"
    );
    let said = line.expect("a read logs its line");
    assert!(
        said.starts_with(&format!(">>> narrate: read {url} (")),
        "{said}"
    );
    assert!(said.ends_with(" characters)"), "{said}");
}

/// §3's row: at most 6000 bytes, cut back to a character boundary, marked. Driven through the real leg so the
/// whole path (browser → text → clip) is the one the program uses.
#[test]
fn f6_1_web_s5_a_long_page_is_clipped_on_a_character_boundary() {
    let _root = fixture("clip");
    // Multi-byte characters straddling the 6000th byte are the case that panics a naive slice.
    let body = "\u{e9}".repeat(tools::WEB_READ_MAX_BYTES + 500);
    let base = serve(vec![(
        "/wide",
        format!("<html><body><p>{body}</p></body></html>"),
    )]);

    let fetch =
        web_browser::read_page_for(&firefox().to_string_lossy(), web_browser::read_budget());
    let page = fetch(&format!("{base}/wide")).expect("the browser answered");
    let clipped = web_tools::clip_page(&page);
    assert!(clipped.ends_with('…'), "a clipped page says so");
    let without_marker = clipped.trim_end_matches(" \u{2026}");
    assert!(
        without_marker.len() <= tools::WEB_READ_MAX_BYTES,
        "{} bytes of page",
        without_marker.len()
    );
    assert!(
        without_marker.ends_with('\u{e9}'),
        "and it stopped on a whole character: {:?}",
        &without_marker[without_marker.len().saturating_sub(4)..]
    );
}

/// A bot wall is named, not handed over — through the real leg, so a captcha page cannot reach the model.
#[test]
fn f6_1_web_s5_a_bot_wall_is_named_not_handed_over() {
    let _root = fixture("wall");
    let base = serve(vec![(
        "/walled",
        long_page("Checking your browser before accessing this site."),
    )]);

    let fetch =
        web_browser::read_page_for(&firefox().to_string_lossy(), web_browser::read_budget());
    let answer = web_tools::read(&format!("{base}/walled"), &fetch);
    assert!(
        answer.starts_with("web_read failed: the page is behind a bot wall ("),
        "the wall is refused by name: {answer}"
    );
    assert!(
        answer.contains("checking your browser"),
        "and quoted: {answer}"
    );
}

/// A page with nothing in it: the page had no text, which is a refusal rather than an empty answer.
#[test]
fn f6_1_web_s5_a_page_with_no_text_refuses() {
    let _root = fixture("empty");
    let base = serve(vec![("/nothing", String::new())]);

    let fetch =
        web_browser::read_page_for(&firefox().to_string_lossy(), web_browser::read_budget());
    let answer = web_tools::read(&format!("{base}/nothing"), &fetch);
    assert_eq!(answer, "web_read failed: the page had no text", "{answer}");
}

/// §2's 45 s is ONE budget including browser boot, and a page that never arrives must end the call at the
/// deadline with a reason, the browser killed rather than waited on.
#[test]
fn f6_1_web_s5_a_browser_that_outlives_its_budget_is_killed_and_says_so() {
    let _root = fixture("timeout");
    let base = serve(Vec::new());

    let started = std::time::Instant::now();
    let fetch = web_browser::read_page_for(
        &firefox().to_string_lossy(),
        std::time::Duration::from_secs(8),
    );
    let outcome = fetch(&format!("{base}/slow"));
    let took = started.elapsed();
    let why = outcome.expect_err("a page that never arrives must not answer");
    assert!(why.contains("did not finish within 8 s"), "{why}");
    assert!(
        took < std::time::Duration::from_secs(14),
        "it stopped at the deadline, not at the server's 30 s: {took:?}"
    );
}

/// The browser is found the way §2 says: the settings value first, then PATH, then the usual install paths —
/// and a named-but-missing binary answers with the filesystem's reason, which is what the step's log line needs.
#[test]
fn f6_1_web_s5_firefox_is_found_by_setting_then_path_then_install_paths() {
    let root = fixture("find");
    let named = root.join("my-firefox");
    common::write_script(&named, "#!/bin/sh\nexit 0\n".to_string()).expect("a named binary");
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&named, std::fs::Permissions::from_mode(0o755))
            .expect("executable");
    }

    // The setting wins, even when a real firefox is on PATH: a person who named one meant that one.
    let found =
        web_browser::firefox_path(&named.to_string_lossy()).expect("the named binary is used");
    assert_eq!(found, named, "the settings box's own binary");

    // Nothing named: PATH order decides, and the container ships firefox-esr where Debian ships firefox.
    let on_path = web_browser::firefox_path("").expect("something on PATH");
    let name = on_path
        .file_name()
        .expect("a file name")
        .to_string_lossy()
        .to_string();
    assert!(
        web_browser::PATH_NAMES.contains(&name.as_str())
            || web_browser::INSTALL_PATHS.contains(&on_path.to_string_lossy().as_ref()),
        "{name} is one of the names or places §2 lists"
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
    let off =
        web_tools::unavailable_log("narrate", "off", false, false, "").expect("off is explained");
    assert!(off.contains("web search is off"), "{off}");
    assert!(
        !web_browser::availability("off").0,
        "`off` means no web tools"
    );
}

/// The offer itself: a machine with no browser gets neither web tool, and says so once — the sentence §2 gives.
#[test]
fn f6_1_web_s5_a_step_without_a_browser_drops_the_tools_and_says_why_once() {
    let _root = fixture("offer");
    let offered = vec![
        naivepost::tools::Tool::GetLines,
        naivepost::tools::Tool::WebSearch,
        naivepost::tools::Tool::WebRead,
    ];

    let (found, reason) = web_browser::availability("/no/such/firefox-either");
    assert!(!found, "nothing to drive");
    let kept = web_tools::filter(&offered, found);
    assert!(
        !kept.contains(&naivepost::tools::Tool::WebSearch),
        "no web_search offered: {kept:?}"
    );
    assert!(
        !kept.contains(&naivepost::tools::Tool::WebRead),
        "no web_read offered: {kept:?}"
    );
    assert!(
        kept.contains(&naivepost::tools::Tool::GetLines),
        "the job's own tools stay"
    );
    let line =
        web_tools::unavailable_log("narrate", "/no/such/firefox-either", false, found, &reason)
            .expect("the reason is said");
    assert!(line.starts_with(">>> narrate: no web search"), "{line}");
}

/// The setting is honoured, not merely consulted: the browser the box names is the one launched. The named
/// binary is a wrapper that records its launch and then runs the real firefox, so the page coming back AND the
/// record together prove the leg used the setting rather than the firefox on PATH.
#[test]
fn f6_1_web_s5_a_named_browser_is_the_one_that_answers() {
    let root = fixture("named");
    let marker = "the named browser answered with this marker";
    let base = serve(vec![("/named", long_page(marker))]);
    let wrapper = root.join("named-firefox");
    common::write_script(
        &wrapper,
        format!(
            "#!/bin/sh\necho $$ >> \"{}\"\nexec \"{}\" \"$@\"\n",
            root.join("launches").display(),
            firefox().display()
        ),
    )
    .expect("the wrapper");
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o755))
            .expect("executable");
    }
    point_at(&wrapper);
    let conf =
        settings::read(&settings::from_environment().unwrap()).expect("the settings the box holds");
    assert_eq!(conf.firefox, wrapper.to_string_lossy());

    let fetch = web_browser::read_page_for(&conf.firefox, web_browser::read_budget());
    let page = fetch(&format!("{base}/named")).expect("the named browser answered");
    assert!(page.contains(marker), "the page came back: {page}");
    let launched = std::fs::read_to_string(root.join("launches"))
        .map(|text| text.lines().count())
        .unwrap_or(0);
    assert_eq!(launched, 1, "and it was the named binary that ran, once");
}

/// Page text keeps paragraph structure and drops scripts: the 200-character floor is meaningless if a whole page
/// collapses to one line or a stylesheet counts as content.
#[test]
fn f6_1_web_s5_page_text_keeps_lines_and_drops_scripts() {
    let html = "<html><head><script>var secret = 'do not read me';</script>\
                <style>.x{color:red}</style></head><body><h1>Heading</h1><p>First line.</p><p>Second line.</p></body></html>";
    let text = web_browser::page_text(html);
    assert!(
        !text.contains("do not read me"),
        "scripts are not content: {text}"
    );
    assert!(
        !text.contains("color:red"),
        "styles are not content: {text}"
    );
    assert!(
        text.contains("Heading\nFirst line.\nSecond line."),
        "blocks become lines: {text:?}"
    );
}
