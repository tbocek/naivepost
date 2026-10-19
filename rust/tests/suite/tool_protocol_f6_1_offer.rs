//! F6.1 S1/S5's offer half: §2's "whether tools are offered" line, said ONCE PER STEP with the reason.
//!
//! The wording itself is `web_tools::unavailable_log`'s (pinned in `tests/suite/web_tools_f6_1.rs`). What that file
//! cannot prove is the part this item adds: a step asks many times — narrate runs up to three attempts, the cut
//! loop many rounds per attempt — and §2 says the reason is logged once per STEP, not once per ask. That memory
//! lives in `crate::llm_web_offer`, and these checks drive it the way the flows do.
//!
//! Every case names its own step, because the memory is process-wide by design (a machine does not grow a
//! browser mid-run) and one binary holds all the cases.

use naivepost::llm_web_offer;
use naivepost::roles::Job;
use naivepost::tools::{self, Tool};

/// A name no filesystem can stat: the settings box filled with a path that is not a browser.
const MISSING: &str = "/definitely/not/a/browser/naivepost-test-firefox";

#[test]
fn f6_1_s1_web_offer_reason_is_said_once_per_step() {
    // A step that has never been told hears the reason; the same step asking again (narrate's second attempt,
    // the cut's next round) hears nothing, while a DIFFERENT step still gets its own line.
    let first = llm_web_offer::unavailable_log("f6_1_once_narrate", MISSING).expect("the first ask explains");
    assert!(
        first.starts_with(">>> f6_1_once_narrate: no web search (firefox:"),
        "§2 quotes the filesystem's reason: {first}"
    );
    assert_eq!(
        llm_web_offer::unavailable_log("f6_1_once_narrate", MISSING),
        None,
        "the same step says it once, however many asks follow"
    );
    assert_eq!(
        llm_web_offer::unavailable_log("f6_1_once_narrate", MISSING),
        None,
        "and keeps saying nothing on every later ask"
    );
    let other = llm_web_offer::unavailable_log("f6_1_once_publish", MISSING).expect("another step is told too");
    assert!(other.starts_with(">>> f6_1_once_publish: "), "one line per step: {other}");
    assert!(llm_web_offer::told("f6_1_once_narrate"), "the step is marked as told");
}

#[test]
fn f6_1_s1_web_tools_drop_when_no_browser_and_the_step_says_why() {
    let offered = tools::offered(Job::Narrate);
    assert!(
        offered.contains(&Tool::WebSearch) && offered.contains(&Tool::WebRead),
        "narrate's catalogue carries the web pair when there is a browser"
    );
    let (filtered, line) = llm_web_offer::offer("f6_1_s1_dropped_offer", MISSING, &offered);
    assert_eq!(
        line.expect("the step is told why two of its tools are gone"),
        ">>> f6_1_s1_dropped_offer: no web search (firefox: No such file or directory (os error 2))"
    );
    assert!(!filtered.contains(&Tool::WebSearch), "web_search is not promised: {filtered:?}");
    assert!(!filtered.contains(&Tool::WebRead), "web_read is not promised: {filtered:?}");
    // The job's OWN tools survive, in the catalogue's order — only the two web rows went.
    let kept: Vec<Tool> = offered.iter().copied().filter(|t| !tools::is_web_tool(*t)).collect();
    assert_eq!(filtered, kept, "exactly the web pair is missing");
    // And a second ask from the same step gets the same filtered list with no repeated line.
    let (again, repeat) = llm_web_offer::offer("f6_1_s1_dropped_offer", MISSING, &offered);
    assert_eq!(again, kept, "the offer stays filtered");
    assert_eq!(repeat, None, "§2: once per step, not once per ask");
}

#[test]
fn f6_1_s1_the_four_reasons_come_out_in_s2_words() {
    // Each reason wants a different fix, so each gets its own sentence. Flatpak cannot be entered from a test,
    // so it is checked where the wording lives (`web_tools::unavailable_log`'s own argument), and the three the
    // program can meet on this machine are checked through the once-per-step door.
    assert_eq!(
        naivepost::web_tools::unavailable_log("f6_1_s1_flatpak", "firefox", true, true, ""),
        Some(">>> f6_1_s1_flatpak: no web search inside Flatpak".to_string())
    );
    assert_eq!(
        llm_web_offer::unavailable_log("f6_1_s1_off", "off"),
        Some(">>> f6_1_s1_off: no web search (web search is off)".to_string())
    );
    assert_eq!(
        llm_web_offer::unavailable_log("f6_1_s1_missing", MISSING),
        Some(">>> f6_1_s1_missing: no web search (firefox: No such file or directory (os error 2))".to_string())
    );
    // Nothing named AND nothing on PATH is §2's "name one in the settings" line; a machine that does have a
    // firefox answers nothing, which is why the empty box defers to the lookup.
    let empty = llm_web_offer::unavailable_log("f6_1_s1_empty", "");
    match empty {
        None => assert!(
            naivepost::web_browser::firefox_path("").is_ok(),
            "an empty box is silent only because a browser was found"
        ),
        Some(line) => assert_eq!(
            line,
            ">>> f6_1_s1_empty: no web search (no firefox found -- name one in the settings, or set it to off)"
        ),
    }
}

#[test]
fn f6_1_s1_the_cut_step_reports_why_it_has_no_web() {
    use naivepost::ui::cut_suggest_watch::Call;
    // The cut page's own attempt state decides what goes out (§8's withdrawal rides on it), so the wire check is
    // that a run whose machine has no browser never offers the web pair — and says §2's line for the step `cut`.
    let mut call = Call::new();
    call.with_web(false);
    let (offered, withdrawn) = call.offer();
    assert!(
        !offered.iter().any(|tool| tools::is_web_tool(*tool)),
        "no browser means no web tools offered: {offered:?}"
    );
    assert_eq!(withdrawn, None, "nothing was withdrawn, so §8's line is not said");
    for name in ["add_segment", "remove_segment", "set_speed", "cut_status", "finish_cut"] {
        assert!(
            offered.iter().any(|tool| tool.name() == name),
            "{name} survives: the model must still be able to build a cut"
        );
    }
    // The step's own reason line, through the door `run_model` calls: once, in §2's words, for the step `cut`.
    let line = llm_web_offer::unavailable_log("cut", MISSING).expect("the cut step says why");
    assert_eq!(line, ">>> cut: no web search (firefox: No such file or directory (os error 2))");
    assert_eq!(llm_web_offer::unavailable_log("cut", MISSING), None, "once per step");
}
