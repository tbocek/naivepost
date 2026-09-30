//! F6.1 §2's web tools over a REAL browser: the leg `web_tools` leaves open.
//!
//! [`crate::web_tools`] holds every rule about what the model sees — the ladder, the dedupe and the 8-hit cap,
//! the page caps, the bot-wall refusals, the log lines — and takes the fetching itself as a closure. This module
//! is that closure: it finds Firefox, launches ONE headless instance per call, reads the page, and hands back
//! hits or text. Nothing here decides policy; it only moves bytes between a browser and `web_tools`.
//!
//! # How the page is taken, and why not WebDriver BiDi
//!
//! §2 describes driving headless Firefox "over WebDriver BiDi" on ports from 9223 up, which is what the
//! prototype does (`gui/websearch.go`). The rewrite launches `firefox --headless --dump-dom <url>` instead: one
//! process per call, started and reaped, printing the RENDERED document to stdout. §2's own binding rule is
//! "one headless browser launched and killed per tool call", so a persistent BiDi session buys nothing a dump
//! does not, while costing a second protocol to get right and a port to collide on. `--dump-dom` still renders
//! JavaScript, so a search page that builds its results client-side arrives complete — which is the only reason
//! the prototype needed a driver at all. The debugging port is still allocated from 9223 upward as §2 says, so a
//! machine running the prototype's browser and this one's do not fight over it.
//!
//! Measured in the build container (Alpine 3.24, no display, no GPU): `/usr/bin/firefox-esr` exists and
//! `firefox-esr --headless --dump-dom <url>` runs and prints the DOM; the engine itself is NOT reachable from
//! there, so hit extraction is tested against a canned DuckDuckGo-shaped DOM rather than a live query. On a
//! user's machine the same code dials the real engine.
//!
//! # The budget
//!
//! §2 gives one 45 s per call, shared by the whole ladder and including browser boot and render wait. That is
//! [`tools::web_ladder_budget`]'s `(45, 3)`: three rungs split one budget, so each dump gets a third and the
//! ladder can never hold a step open for minutes. A dump that outlives its slice is KILLED — an orphaned
//! headless browser would sit on the port and the memory until the app exits.

use std::io::Read as IoRead;
use std::path::PathBuf;
use std::process::Child;
use std::sync::atomic::{AtomicU16, Ordering};
use std::time::{Duration, Instant};

use crate::tools;
use crate::web_tools::Hit;

/// §2's first port for a headless browser. Kept even if the prototype is also installed, and bumped per call so
/// two searches (two jobs asking at once) never share a profile lock.
pub const FIRST_PORT: u16 = 9223;

/// Seconds allowed for the browser to boot before anything is asked of it. §2 names 15 s inside the 45 s call.
pub const BOOT_SECONDS: u64 = 15;

/// Seconds the page is given to render after navigation. §2's other 15 s.
pub const RENDER_SECONDS: u64 = 15;

/// Where a named-but-missing binary is looked for besides PATH: the usual install locations, tried in order.
/// Flatpak's own firefox is deliberately absent — inside the sandbox the flatpak instance cannot be driven.
pub const INSTALL_PATHS: [&str; 4] = [
    "/usr/bin/firefox",
    "/usr/lib/firefox-esr/firefox-esr",
    "/snap/bin/firefox",
    "/opt/firefox/firefox",
];

/// The names tried on `PATH` when the settings box named nothing. The prototype accepts all three, and Alpine
/// ships `firefox-esr` where Debian ships `firefox` — a rule that found only one would call a working machine
/// "no firefox found".
pub const PATH_NAMES: [&str; 3] = ["firefox", "firefox-esr", "firefox-bin"];

fn is_executable(path: &std::path::Path) -> bool {
    std::fs::metadata(path).is_ok_and(|meta| {
        use std::os::unix::fs::PermissionsExt;
        meta.is_file() && meta.permissions().mode() & 0o111 != 0
    })
}

/// Find the browser to drive: the settings value if it names something runnable, else the first of
/// [`PATH_NAMES`] on `PATH`, else the first of [`INSTALL_PATHS`].
///
/// The error is the filesystem's own reason ("No such file or directory", "Permission denied"), because
/// [`crate::web_tools::unavailable_log`] prints it verbatim as `firefox: <stat error>` and a person has to be
/// able to act on it. An empty setting is not an error but the "name one in the settings" case, which
/// `unavailable_log` handles from the empty string itself — so this returns the stat reason only for a name
/// that WAS given and could not be run.
pub fn firefox_path(setting: &str) -> Result<PathBuf, String> {
    let setting = setting.trim();
    if !setting.is_empty() && !setting.eq_ignore_ascii_case("off") {
        let named = PathBuf::from(setting);
        if is_executable(&named) {
            return Ok(named);
        }
        // A bare command name in the box still means a PATH lookup, not a path under cwd.
        if let Some(found) = on_path(setting) {
            return Ok(found);
        }
        let reason = std::fs::metadata(&named).err().map(|e| e.to_string()).unwrap_or_else(|| "not executable".to_string());
        return Err(reason);
    }
    for name in PATH_NAMES {
        if let Some(found) = on_path(name) {
            return Ok(found);
        }
    }
    for path in INSTALL_PATHS {
        let candidate = PathBuf::from(path);
        if is_executable(&candidate) {
            return Ok(candidate);
        }
    }
    Err("no firefox on PATH or in its usual install paths".to_string())
}

/// The first executable called `name` on `PATH`. Written out rather than shelling out to `which`, which is not
/// guaranteed to exist and would be a second process per lookup.
fn on_path(name: &str) -> Option<PathBuf> {
    if name.contains('/') {
        let direct = PathBuf::from(name);
        return is_executable(&direct).then_some(direct);
    }
    let var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&var) {
        let candidate = dir.join(name);
        if is_executable(&candidate) {
            return Some(candidate);
        }
    }
    None
}

/// Ports from [`FIRST_PORT`] upward, one per launched browser. Atomic because two jobs may search at once.
fn next_port() -> u16 {
    static NEXT: AtomicU16 = AtomicU16::new(FIRST_PORT);
    NEXT.fetch_add(1, Ordering::SeqCst)
}

/// One headless browser launch, killed on drop: §2's "one headless browser launched and killed per tool call"
/// made structural, so no exit path — answer, error, or timeout — can leave a firefox behind holding a port.
struct Launched {
    child: Child,
}

impl Drop for Launched {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Launch `browser --headless --dump-dom <url>` and return its stdout, killing it if `budget` runs out first.
///
/// The budget covers everything §2 counts: boot, navigation and render. `--profile` gets a private temp folder
/// so a user's real profile is never touched and two calls never contend for its lock.
pub fn dump_with(browser: &std::path::Path, url: &str, budget: Duration) -> Result<String, String> {
    let port = next_port();
    let profile = std::env::temp_dir().join(format!("naivepost-web-{}-{port}", std::process::id()));
    let _ = std::fs::create_dir_all(&profile);
    let child = crate::subprocess::spawn_piped(
        &browser.to_string_lossy(),
        &[
            "--headless",
            "--no-remote",
            "--profile",
            &profile.to_string_lossy(),
            "--remote-debugging-port",
            &port.to_string(),
            "--window-size",
            "1280,1024",
            "--dump-dom",
            url,
        ],
    )
    .map_err(|why| format!("could not start {}: {why}", browser.display()))?;
    let mut launched = Launched { child };
    // Reading stdout on THIS thread with the deadline checked around the wait: `--dump-dom` writes once and
    // exits, so a blocked read past the budget means the browser hung, and §2's answer is to kill it.
    let started = Instant::now();
    loop {
        match launched.child.try_wait() {
            Ok(Some(status)) => {
                let mut text = String::new();
                if let Some(mut out) = launched.child.stdout.take() {
                    let _ = out.read_to_string(&mut text);
                }
                drop(launched);
                let _ = std::fs::remove_dir_all(&profile);
                if !status.success() && text.trim().is_empty() {
                    return Err(format!("the browser exited {status} without printing a page"));
                }
                return Ok(text);
            }
            Ok(None) | Err(_) => {
                if started.elapsed() >= budget {
                    // Killed through Drop. The reason names the budget so a reader knows whether to raise the
                    // setting, close tabs, or blame the network.
                    drop(launched);
                    return Err(format!(
                        "the page did not finish within {} s",
                        budget.as_secs()
                    ));
                }
                std::thread::sleep(Duration::from_millis(50));
            }
        }
    }
}

/// [`dump_with`] with the browser resolved from the settings box — the door the tool closures use.
pub fn dump(setting: &str, url: &str, budget: Duration) -> Result<String, String> {
    let browser = firefox_path(setting)?;
    dump_with(&browser, url, budget)
}

// ---- HTML to what the rules want ---------------------------------------------------------

// ---- HTML to what the rules want ---------------------------------------------------------

/// Drop `<script>`/`<style>`/`<head>` blocks and every tag, then collapse whitespace: page text, which is what
/// [`crate::web_tools::page_issue`] and [`crate::web_tools::clip_page`] are written to judge.
///
/// The element scan runs on a lower-cased COPY while the slices index the original, so HTML outside ASCII (a
/// title in Cyrillic, say) cannot shift an offset and cut a tag in half — `make_ascii_lowercase` widens bytes.
pub fn page_text(html: &str) -> String {
    let mut out = String::with_capacity(html.len() / 4 + 16);
    // `<script>` and `<style>` hold RAW TEXT in HTML: their contents are not markup, so a tag scanner must skip
    // to the element's own close rather than read the source as content. `<head>`/`<title>` nest normally and
    // are counted, since a title is an element whose text simply should not be read.
    let mut head_hidden = 0usize;
    // The raw element we are inside, remembered by its full tag name so `</script>` closes exactly `<script ...>`
    // and nothing else. Empty means not inside one.
    let mut raw_element = String::new();
    // Lower-cased on a copy: `make_ascii_lowercase` can widen bytes, and every slice below indexes the original.
    let mut lower = html.to_string();
    lower.make_ascii_lowercase();
    let mut offset = 0usize;
    while let Some(rel) = lower[offset..].find('<') {
        let start = offset + rel;
        let Some(end_rel) = lower[start..].find('>') else { break };
        let tag_end = start + end_rel + 1;
        if !raw_element.is_empty() {
            // Inside a script or a style there is nothing to emit until its closing tag shows up.
            let close = format!("</{raw_element}>");
            match lower[start..].find(&close) {
                Some(rel_close) => {
                    offset = start + rel_close + close.len();
                    raw_element.clear();
                }
                None => offset = html.len(),
            }
            continue;
        }
        out.push_str(&html[offset..start]);
        let name_start = start + 1;
        let name_full = lower[name_start..start + end_rel].trim();
        // The tag's NAME, without its attributes or its closing slash: `<p class="x">` is a `p`, and comparing
        // the whole attribute string against BLOCK_TAGS matched nothing, which flattened every page to one line.
        let name = name_full.trim_start_matches('/').split([' ', '\t', '\n']).next().unwrap_or("");
        let closing = name_full.starts_with('/');
        if RAW_ELEMENTS.iter().any(|element| name.starts_with(*element)) {
            // Reached only when not already inside a raw element (that case is skipped above), so an opening tag
            // here always starts one. A stray `</script>` outside one matches the branch and changes nothing.
            if !closing {
                raw_element = name.trim().to_string();
            }
        } else if name.starts_with("head") || name.starts_with("title") {
            if closing {
                head_hidden = head_hidden.saturating_sub(1);
            } else {
                head_hidden += 1;
            }
        } else if head_hidden == 0 {
            // A block-level tag is a line break in the reading order; anything else just separates words.
            if BLOCK_TAGS.iter().any(|block| name.starts_with(*block)) {
                out.push('\n');
            } else {
                out.push(' ');
            }
        }
        offset = tag_end;
    }
    if offset < html.len() {
        out.push_str(&html[offset..]);
    }
    // Anything still inside an unclosed raw element is swallowed with it, as a browser would.
    let unescaped = unescape(&out);
    unescaped
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<&str>>()
        .join("\n")
}

/// Tags whose end means a line break for the reader. A page flattened to one long line makes the 200-character
/// floor meaningless, so paragraph structure has to survive the strip.
const BLOCK_TAGS: [&str; 12] = [
    "p", "div", "li", "tr", "h1", "h2", "h3", "h4", "br", "article", "section", "table",
];

/// Elements whose contents are raw text rather than markup.
const RAW_ELEMENTS: [&str; 2] = ["script", "style"];

/// The handful of entities a rendered page actually contains. Anything else stays as written: guessing at an
/// unknown `&xyz;` would silently change somebody's text.
fn unescape(text: &str) -> String {
    text.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#x27;", "'")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
}

/// DuckDuckGo's result anchors. The current layout wraps each result in `<article>` with its title link under
/// an `<h2>`; the older markup put `result__a`/`result__snippet` classes on the anchors directly. Both shapes
/// are read, because the engine changes this markup without asking anyone.
pub fn search_hits(html: &str) -> Vec<Hit> {
    let mut hits: Vec<Hit> = Vec::new();
    for block in article_blocks(html) {
        let Some((href, title)) = first_link(&block) else { continue };
        if href.is_empty() || title.trim().is_empty() {
            continue;
        }
        hits.push(Hit {
            title: clean(&title),
            url: clean(&href),
            snippet: clean(&first_snippet(&block)),
        });
    }
    hits
}

/// Each `<article>…</article>` of the result page, or the whole document when the layout has none (the older
/// markup has no articles, and its links are found in the same pass).
fn article_blocks(html: &str) -> Vec<String> {
    let lower = html.to_ascii_lowercase();
    let mut blocks = Vec::new();
    let mut cursor = 0usize;
    while let Some(rel) = lower[cursor..].find("<article") {
        let start = cursor + rel;
        let Some(end_rel) = lower[start..].find("</article>") else { break };
        blocks.push(html[start..start + end_rel].to_string());
        cursor = start + end_rel;
    }
    if blocks.is_empty() {
        blocks.push(html.to_string());
    }
    blocks
}

/// The first anchor with an href and text: the result's title link.
fn first_link(block: &str) -> Option<(String, String)> {
    let mut rest = block;
    while let Some(rel) = rest.to_ascii_lowercase().find("<a ") {
        let start = rel + 2;
        let Some(close) = rest[start..].find('>') else { break };
        let attrs = &rest[start..start + close];
        let href = attribute(attrs, "href").unwrap_or_default();
        let after = &rest[start + close + 1..];
        let Some(text_end) = after.to_ascii_lowercase().find("</a>") else { break };
        let text = &after[..text_end];
        if !href.trim().is_empty() && !text.trim().is_empty() {
            return Some((href, text.to_string()));
        }
        rest = after;
    }
    None
}

/// The block's snippet: the older `result__snippet` element, else the block's own text after the title link.
fn first_snippet(block: &str) -> String {
    let lower = block.to_ascii_lowercase();
    if let Some(rel) = lower.find("result__snippet") {
        let tail = &block[rel..];
        if let Some(close) = tail.find('>') {
            let body = &tail[close + 1..];
            if let Some(end) = body.find('<') {
                return body[..end].to_string();
            }
        }
    }
    let (_, after_title) = match (lower.find("</a>"), block.find("</a>")) {
        (Some(rel), _) => (0, &block[rel + 4..]),
        _ => (0, block),
    };
    page_text(after_title).lines().next().unwrap_or_default().to_string()
}

/// One attribute out of a tag's attribute list, quoted or bare.
fn attribute(attrs: &str, name: &str) -> Option<String> {
    let lower = attrs.to_ascii_lowercase();
    let rel = lower.find(name)?;
    let after_name = &attrs[rel + name.len()..];
    let after_eq = after_name.trim_start();
    let rest = after_eq.strip_prefix('=')?.trim_start();
    if let Some(stripped) = rest.strip_prefix('"') {
        let end = stripped.find('"')?;
        return Some(stripped[..end].to_string());
    }
    let end = rest.find([' ', '>']).unwrap_or(rest.len());
    Some(rest[..end].to_string())
}

/// Collapse the whitespace a DOM dump sprinkles everywhere and drop the entity noise from one field.
fn clean(text: &str) -> String {
    page_text(text).replace('\n', " ").split_whitespace().collect::<Vec<&str>>().join(" ")
}

// ---- the closures the rules take ---------------------------------------------------------

/// One search rung: navigate, extract, hand back the hits. `budget` is the WHOLE call's slice for this rung —
/// the caller splits [`tools::web_ladder_budget`]'s 45 s across the three queries, so a slow first rung cannot
/// spend a later one's share.
pub fn query_for(setting: &str, budget: Duration) -> impl Fn(&str) -> Result<Vec<Hit>, String> {
    let browser = firefox_path(setting);
    move |query: &str| {
        let browser = browser.clone()?;
        let url = duckduckgo(query);
        let html = dump_with(&browser, &url, budget)?;
        Ok(crate::web_tools::dedupe(search_hits(&html)))
    }
}

/// One page: navigate, strip to text. The wall and emptiness checks are `web_tools`' and run on the result.
pub fn read_page_for(setting: &str, budget: Duration) -> impl Fn(&str) -> Result<String, String> {
    let browser = firefox_path(setting);
    move |url: &str| {
        let browser = browser.clone()?;
        let html = dump_with(&browser, url, budget)?;
        Ok(page_text(&html))
    }
}

/// Whether a step's web tools can be offered, and why not when they cannot.
///
/// §2 logs the reason ONCE per step, with the filesystem's own words for a named-but-missing binary — which is
/// the only reason [`crate::web_tools::unavailable_log`] takes `found`/`stat_error` at all. The lookup is
/// [`crate::web_browser::firefox_path`], so the box that says "no firefox found" is said by the same code that
/// would have launched it.
pub fn availability(firefox: &str) -> (bool, String) {
    if firefox.trim().is_empty() || firefox.trim().eq_ignore_ascii_case("off") {
        return (false, String::new());
    }
    match crate::web_browser::firefox_path(firefox) {
        Ok(_) => (true, String::new()),
        Err(reason) => (false, reason),
    }
}

/// One web tool call as §2 S6 wants it: a problem is TEXT the model can act on, never a failed job. Returns the
/// answer plus the line §2 asks for ("searched a / b / c -- N result(s) for <used>", "read <url> (N
/// characters)"); `None` means the call was not a web tool, leaving the caller's own dispatch the authority.
pub fn run_tool(
    step: &str,
    firefox: &str,
    asked: &crate::exchanges::ToolCall,
) -> Option<(String, Option<String>)> {
    use crate::tools::Tool;
    let name = asked.name.as_str();
    if name == Tool::WebSearch.name() {
        let ask = query_for(firefox, ladder_slice());
        return Some(crate::web_tools::search(step, &asked.args, &ask));
    }
    if name == Tool::WebRead.name() {
        let fetch = read_page_for(firefox, read_budget());
        return Some(crate::web_tools::read_call(step, &asked.args, &fetch));
    }
    None
}

/// Run one web tool for a step and log what it did on the sink every headless flow writes to (a tool closure
/// cannot reach the run's own `log` closure, which borrows the loop's state).
pub fn dispatch(step: &str, firefox: &str, asked: &crate::exchanges::ToolCall) -> Option<String> {
    let (answer, line) = run_tool(step, firefox, asked)?;
    if let Some(line) = line {
        crate::ui::window::log_line(&line);
    }
    Some(answer)
}

/// DuckDuckGo's result page for one query. `kl=noah` is the plain desktop layout; the engine serves the same
/// markup regardless of locale, and a redirect-free URL keeps the budget for the page itself.
pub fn duckduckgo(query: &str) -> String {
    format!("https://duckduckgo.com/?q={}", percent_encode(query))
}

/// Percent-encode a query for a URL. Hand-rolled to avoid a dependency for one call site: everything outside
/// the unreserved set of RFC 3986 becomes `%XX`, which is what a query string needs.
fn percent_encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(*byte as char)
            }
            b' ' => out.push('+'),
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

/// The whole ladder's budget divided among its rungs (§2's one 45 s per call, three queries sharing it).
pub fn ladder_slice() -> Duration {
    let (total, rungs) = tools::web_ladder_budget();
    Duration::from_secs(total / rungs.max(1) as u64)
}

/// A read's whole budget: one page, one browser, the full 45 s.
pub fn read_budget() -> Duration {
    Duration::from_secs(tools::WEB_CALL_SECONDS)
}
