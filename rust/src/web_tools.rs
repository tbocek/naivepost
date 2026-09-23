//! F6.1 — `spec/09-llm-and-tools.md` §2's web tools: `web_search` and `web_read`.
//!
//! The protocol around them is [`crate::tool_loop`]'s; this module holds what the two tools themselves promise,
//! which is everything in §2's "Web tools (where offered…)" paragraph except the browser. Fetching stays a
//! closure the caller passes — the same shape [`tool_loop::run`] uses for `ask` and `call` — so the ladder rule,
//! the hit cap, the page caps and the descriptions are testable with no network and no firefox, in a container
//! that has neither.
//!
//! # The engine is not here
//!
//! §2 says `web_search(broad, medium, narrow)` drives headless Firefox over WebDriver BiDi against DuckDuckGo's
//! result page, deduping by URL; the prototype does exactly that ([`gui/websearch.go`](../../gui/websearch.go)'s
//! `ddgExtract` script and its BiDi client), launching and killing one browser per call on ports from 9223 up.
//! That is a wire protocol, not a rule about what the model sees, so it lives behind the [`Query`] / [`ReadPage`]
//! closures: what this module decides is which query is asked, in what order, and what comes back when the web
//! answers, errors or blocks.
//!
//! # Ids used
//!
//! `P.eng.searchHits` — the hit cap, held as [`tools::WEB_SEARCH_MAX_HITS`] (8). The read cap (§3's row: page
//! text ≤ 6000 bytes) and the 45 s per call have no `P.` id in §10, so they are [`tools::WEB_READ_MAX_BYTES`]
//! and [`tools::WEB_CALL_SECONDS`], named once there and re-exported here rather than repeated as numbers.

use serde::Deserialize;

use crate::degraded;
use crate::tool_loop;
use crate::tools::{self, Tool};

/// One search's worth of answers, as the caller's browser hands them over.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    pub title: String,
    pub url: String,
    pub snippet: String,
}

impl Hit {
    fn is_empty(&self) -> bool {
        self.title.trim().is_empty() && self.url.trim().is_empty() && self.snippet.trim().is_empty()
    }
}

/// One query of the ladder. `Err` is what the web said — a timeout, a refused navigation; §2's "an erroring
/// query is skipped" turns that into a step to the next rung rather than an answer to the model.
pub type Query = dyn Fn(&str) -> Result<Vec<Hit>, String>;

/// One page's text. `Err` follows the same rule: the reason comes back as a sentence, never as a failed job.
pub type ReadPage = dyn Fn(&str) -> Result<String, String>;

// ---- S1: the descriptions, verbatim -----------------------------------------------------

/// §2: "descriptions verbatim in prompts/tools.md". `prompts/tools.md` is explicit that these *are* the
/// instructions — the only place the model is told when to reach for a tool — so this copies them rather than
/// paraphrasing, and its tests compare against the spec file itself.
pub const WEB_SEARCH_DESCRIPTION: &str = "Look up a fact you are about to write into the video and would \
otherwise guess: what a named thing is, does or costs, a name's spelling, a number. Only for something the \
material does not contain, and only when the user context asks for a detail you do not have -- not to \
understand the session, not to check what you have already been told. Give three queries at once, from broad to \
narrow -- the game, the game and the thing, the thing's exact name -- and you get the narrowest one that found \
anything.";

/// The three argument lines of prompts/tools.md, kept as the schema says them: `required` is all three because
/// the prompt tells the model to give three queries at once.
pub const WEB_SEARCH_ARGUMENTS: [(&str, &str); 3] = [
    ("broad", "the general subject, e.g. the game"),
    ("medium", "the subject and the thing"),
    ("narrow", "the thing's exact name, as the user context spells it"),
];

/// prompts/tools.md, verbatim; `url` is its only argument line.
pub const WEB_READ_DESCRIPTION: &str = "Read one page from a web_search result, by its URL, when the snippet did \
not say enough. Returns the page's text, shortened.";

/// The description that ships with the tool (§2 S1). Only these two have one yet: every other row of §3's
/// catalogue is to "be described by the same rule", and until that wording exists a name-only offer beats an
/// instruction this module invented.
pub fn description(tool: &Tool) -> Option<&'static str> {
    match tool {
        Tool::WebSearch => Some(WEB_SEARCH_DESCRIPTION),
        Tool::WebRead => Some(WEB_READ_DESCRIPTION),
        _ => None,
    }
}

/// The job's tools as the wire wants them, with these two descriptions in place. This is [`tool_loop::schemas`]
/// with §2's hook wired — the call site §1's `tools` array should use.
pub fn schemas(offered: &[Tool]) -> Vec<serde_json::Value> {
    tool_loop::schemas(offered, &description)
}

// ---- S1/S6: the arguments ---------------------------------------------------------------

/// What `web_search` takes. Missing rungs are tolerated rather than refused: a model that only knows the game's
/// name asked a legitimate question, and answering one query costs less than re-asking for three.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct SearchArgs {
    #[serde(default)]
    pub broad: String,
    #[serde(default)]
    pub medium: String,
    #[serde(default)]
    pub narrow: String,
}

impl SearchArgs {
    /// Parse the model's argument JSON. A failure comes back as [`tools::error`]'s shape naming what is wrong —
    /// S6's rule that a problem is an answer the model can act on, not a failed job.
    pub fn from_json(args: &str) -> Result<Self, String> {
        // The reason is the parse error itself: a model that wrote `"queries": [...]` needs to see that `broad`
        // is what it left out, and one sentence naming only "bad arguments" would send it guessing.
        serde_json::from_str(args)
            .map_err(|problem| tools::error(&format!("web_search: could not read the arguments: {problem}")))
    }
}

/// What `web_read` takes. A blank URL is refused before a browser is started, because there is nothing to read.
pub fn read_url(args: &str) -> Result<String, String> {
    #[derive(Deserialize)]
    struct Args {
        #[serde(default)]
        url: String,
    }
    // Same shape as SearchArgs::from_json's problem: the parse error is the thing the model can act on.
    let parsed: Args = serde_json::from_str(args)
        .map_err(|problem| tools::error(&format!("web_read: could not read the arguments: {problem}")))?;    if parsed.url.trim().is_empty() {
        return Err(tools::error("web_read: no url given"));
    }
    Ok(parsed.url)
}

// ---- The ladder -----------------------------------------------------------------------

/// The rungs in the order they are asked: broad, medium, narrow, blank ones gone. Trimming happens here so a
/// rung of spaces is treated as not given rather than searched for.
pub fn queries<'a>(broad: &'a str, medium: &'a str, narrow: &'a str) -> Vec<&'a str> {
    [broad, medium, narrow]
        .into_iter()
        .map(str::trim)
        .filter(|rung| !rung.is_empty())
        .collect()
}

/// The ladder's answer: which rung produced hits, and those hits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ladder {
    /// The query the hits came from — logged as `<used>`, because "no results for `narrow`" and "no results for
    /// anything" are different news to whoever reads the log.
    pub used: String,
    pub hits: Vec<Hit>,
}

/// Ask the ladder broad → medium → narrow, stopping at the first rung that found nothing after one had already
/// answered (§2's "the ladder climbs while the web answers"). `ask` is the browser; a query that errors is
/// skipped and remembered.
///
/// Three endings, each with its own line in §2: a rung answered → those hits; every rung errored → the last
/// error, so a broken network reads as a failure rather than as an empty web; nothing found anywhere → `Ok`
/// with no hits, which [`format_hits`] turns into the one sentence that says so.
pub fn climb(ask: &Query, broad: &str, medium: &str, narrow: &str) -> Result<Ladder, String> {
    let mut best: Option<Ladder> = None;
    let mut last_error: Option<String> = None;
    for rung in queries(broad, medium, narrow) {
        match ask(rung) {
            Err(problem) => last_error = Some(problem),
            Ok(hits) => {
                let hits = dedupe(hits);
                if hits.is_empty() {
                    // The web stopped answering. The rung below the one that did answer is the answer.
                    if best.is_some() {
                        break;
                    }
                    continue;
                }
                best = Some(Ladder { used: rung.to_string(), hits });
            }
        }
    }
    match best {
        Some(found) => Ok(found),
        // Only a ladder that found nothing anywhere reports the error: one dead query in three is not news.
        None => match last_error {
            Some(problem) => Err(problem),
            None => Ok(Ladder { used: String::new(), hits: Vec::new() }),
        },
    }
}

/// Dedupe by URL and cap at [`tools::WEB_SEARCH_MAX_HITS`] (`P.eng.searchHits`). A search engine repeats itself
/// constantly — the same docs page under two tracking URLs — and a hit list that spends its eight rows on one
/// page has no room left for the fact.
pub fn dedupe(hits: impl IntoIterator<Item = Hit>) -> Vec<Hit> {
    let mut seen: Vec<String> = Vec::new();
    let mut kept = Vec::new();
    for hit in hits {
        if hit.is_empty() || seen.contains(&hit.url) {
            continue;
        }
        seen.push(hit.url.clone());
        kept.push(hit);
        if kept.len() == tools::WEB_SEARCH_MAX_HITS {
            break;
        }
    }
    kept
}

/// What the model reads back: a header naming the query that answered, then hits numbered from 1 with the URL
/// on its own line so a `web_read` can quote it exactly. No hits at all is one sentence, not an empty list —
/// an empty reply reads to a model as "the tool broke" and it stops using it.
pub fn format_hits(ladder: &Ladder) -> String {
    if ladder.hits.is_empty() {
        return "No results for any of the three queries.".to_string();
    }
    let mut out = format!("Results for {:?}:\n", ladder.used);
    for (index, hit) in ladder.hits.iter().enumerate() {
        out.push_str(&format!("{}. {}\n   {}\n", index + 1, hit.title, hit.url));
        if !hit.snippet.trim().is_empty() {
            out.push_str(&format!("   {}\n", hit.snippet));
        }
    }
    out
}

// ---- The page -------------------------------------------------------------------------

/// Phrases that mean the page is a wall rather than an article. A captcha handed to the model as "the page's
/// text" makes it write "verify you are human" into a narration, so each is refused by name and the model is
/// told which one it hit — enough to pick a different result.
pub const BOT_WALLS: [&str; 11] = [
    "just a moment",
    "checking your browser",
    "verify you are human",
    "verifying you are human",
    "are you human",
    "press and hold",
    "please enable javascript and cookies",
    "attention required",
    "access denied",
    "403 forbidden",
    // Listed after the others on purpose: a busy page is worth retrying, so its name has to survive the check.
    "too many requests",
];

/// Why this text is not readable, in one clause — or nothing when it is. Checked on lowercased text because
/// walls shout ("ACCESS DENIED") and the comparison should not care. A wall's clause names the phrase that
/// matched ([`bot_wall_issue`] wraps it); the other two reasons stand alone.
pub fn page_issue(text: &str) -> Option<String> {
    let lowered = text.trim().to_lowercase();
    if lowered.is_empty() {
        return Some("the page had no text".to_string());
    }
    if let Some(wall) = BOT_WALLS.iter().find(|wall| lowered.contains(*wall)) {
        return Some(bot_wall_issue(wall));
    }
    // Under this the page is a shell: a bot check that names itself differently, or a render that never
    // finished. Either way there is no fact in it, and 200 characters of navigation is not a page.
    if lowered.chars().count() < MIN_PAGE_CHARS {
        return Some("the page had almost no text (blocked, or not loaded)".to_string());
    }
    None
}

/// Characters under which a page counts as empty rather than short — §2's "almost no text" bound.
pub const MIN_PAGE_CHARS: usize = 200;

/// A wall named inside the refusal sentence: `the page is behind a bot wall (just a moment)`.
pub fn bot_wall_issue(wall: &str) -> String {
    format!("the page is behind a bot wall ({wall})")
}

/// Cut page text to [`tools::WEB_READ_MAX_BYTES`] (§3's row), back off to a UTF-8 character boundary so the
/// result is valid text, and mark that something was removed. A page under the cap comes back untouched — an
/// ellipsis after a whole page would tell the model the fact it needs might be in what it cannot see.
pub fn clip_page(text: &str) -> String {
    if text.len() <= tools::WEB_READ_MAX_BYTES {
        return text.to_string();
    }
    let mut cut = tools::WEB_READ_MAX_BYTES;
    while cut > 0 && !text.is_char_boundary(cut) {
        cut -= 1;
    }
    // The marker is not counted against the cap: an ellipsis inside the 6000 would push out real page text to
    // make room for itself, and every byte of page text in the result should be text the page had.
    format!("{} …", &text[..cut])
}

/// One page as the model sees it: read, checked for a wall, clipped. Every problem is `web_read failed: <reason>`
/// (§2's wording) — S6 again, an answer rather than a failure, because the model has other results to try.
pub fn read(url: &str, fetch: &ReadPage) -> String {
    match fetch(url) {
        Err(problem) => format!("web_read failed: {problem}"),
        Ok(text) => match page_issue(&text) {
            Some(reason) => format!("web_read failed: {reason}"),
            None => clip_page(&text),
        },
    }
}

// ---- Whether the tools exist at all ---------------------------------------------------

/// Whether a step has web tools at all: Flatpak never does, `off` is the setting's own word for it, and an
/// empty setting names nothing.
pub fn offered(firefox: &str, flatpak: bool) -> bool {
    degraded::web_available(firefox, flatpak)
}

/// The reason a step got no web tools, once, as §2 logs it — `None` when the step does get them. Flatpak first:
/// it is the fact that decides before anyone looks at a setting, and naming firefox inside a sandbox would send
/// someone hunting a binary the sandbox cannot reach anyway.
///
/// Three reasons, three lines, because they want different fixes: `off` is somebody having turned the web away;
/// nothing set at all means no browser was ever named; and a name that is not a binary quotes the filesystem's
/// reason — §2's `firefox: <stat error>` — which only the caller can supply. That last one is why `found` is an
/// argument: `/opt/firefox` is unavailable only once the filesystem has said so, and this module reads no disk.
pub fn unavailable_log(step: &str, firefox: &str, flatpak: bool, found: bool, stat_error: &str) -> Option<String> {
    if flatpak {
        return Some(format!(">>> {step}: no web search inside Flatpak"));
    }
    let firefox = firefox.trim();
    if firefox.is_empty() {
        return Some(format!(">>> {step}: no web search (no firefox found -- name one in the settings, or set it to off)"));
    }
    if firefox.eq_ignore_ascii_case("off") {
        return Some(format!(">>> {step}: no web search (web search is off)"));
    }
    if found {
        return None;
    }
    // Empty means the caller could not say more, which is still worth one line.
    let reason = if stat_error.is_empty() { "not found" } else { stat_error };
    Some(format!(">>> {step}: no web search (firefox: {reason})"))
}

/// A job's catalogue with the web gone when it is unavailable. `tools::offered`'s order is kept so a catalogue
/// reads the same with and without a browser apart from the two missing rows; [`degraded::offered_offline`] is
/// the same rule read straight from the settings string, and this is the form for a caller that has already
/// decided.
pub fn filter(tools: &[Tool], available: bool) -> Vec<Tool> {
    if available {
        return tools.to_vec();
    }
    tools
        .iter()
        .copied()
        .filter(|tool| !matches!(tool, Tool::WebSearch | Tool::WebRead))
        .collect()
}

/// A search as §2 logs it: the three rungs as asked, how many came back, and which one they came from — the
/// last of those is what tells a reader the narrow query was useless.
pub fn searched_log(step: &str, broad: &str, medium: &str, narrow: &str, hits: usize, used: &str) -> String {
    format!(">>> {step}: searched {broad:?} / {medium:?} / {narrow:?} -- {hits} result(s) for {used:?}")
}

/// A read as §2 logs it. The count is in characters, not bytes: a person judging a page counts words, and the
/// cap that follows is about how much text the model has to read through.
pub fn read_log(step: &str, url: &str, characters: usize) -> String {
    format!(">>> {step}: read {url} ({characters} characters)")
}

// ---- One call -------------------------------------------------------------------------

/// `web_search` as the loop calls it: parse, climb, log, answer. The ladder's line is built here because it is
/// this function that knows which rung answered; a caller appends it to the run's log.
pub fn search(step: &str, args: &str, ask: &Query) -> (String, Option<String>) {
    let args = match SearchArgs::from_json(args) {
        Ok(args) => args,
        Err(problem) => return (problem, None),
    };
    match climb(ask, &args.broad, &args.medium, &args.narrow) {
        Err(problem) => (format!("web_search failed: {problem}"), None),
        Ok(ladder) => {
            let hits = ladder.hits.len();
            let log = searched_log(step, &args.broad, &args.medium, &args.narrow, hits, &ladder.used);
            (format_hits(&ladder), Some(log))
        }
    }
}

/// `web_read` as the loop calls it: parse the URL, fetch, check, clip. The line counts characters of what the
/// model is handed back, clipped — that is the cost a reader of the log is being told about.
pub fn read_call(step: &str, args: &str, fetch: &ReadPage) -> (String, Option<String>) {
    let url = match read_url(args) {
        Ok(url) => url,
        Err(problem) => return (problem, None),
    };
    let page = read(&url, fetch);
    let log = read_log(step, &url, page.chars().count());
    (page, Some(log))
}
