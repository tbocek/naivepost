//! §2's "whether tools are offered" half: a step that gets no web tools says WHY, and says it ONCE.
//!
//! [`crate::web_tools`] holds the wording ([`crate::web_tools::unavailable_log`]) and the filter, but nothing
//! remembered that a step had already said it. Each flow therefore hand-rolled the same three calls
//! (`web_browser::availability` → `web_tools::filter` → `unavailable_log`) and logged whatever came back — which
//! is right for one ask and wrong for a step: narrate asks up to three attempts, the cut loop runs many rounds
//! per attempt, so an unremembered line repeats itself and buries the log in news nobody can act on. §2's words
//! are "logged once per step with the reason".
//!
//! Two doors, mirroring [`crate::llm_tools_offer`] (which remembers a refused `tools` FIELD per server; this one
//! remembers that a STEP was told it has no browser):
//!
//! * [`offer`] — the job's catalogue without the web pair when no browser can be driven, plus the reason line
//!   the FIRST time this step is asked.
//! * [`told`] — has this step already been told? For a caller that filters the list itself (§8's withdrawal
//!   rules own the cut's list) and only needs to know whether to speak.
//!
//! The memory is process-wide and keyed on the step name because the fact is about the machine, not the call:
//! firefox does not appear mid-run, and every later ask from the same step would repeat the same sentence. Like
//! the refusal table, it is never cleared — restarting the app resets it.

use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};

use crate::tools::Tool;

/// The steps that have already been told they have no web tools.
fn told_steps() -> &'static Mutex<HashSet<String>> {
    static TOLD: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    TOLD.get_or_init(|| Mutex::new(HashSet::new()))
}

/// Has this step already heard why it has no web tools? True means the line was said, so say nothing more.
pub fn told(step: &str) -> bool {
    told_steps()
        .lock()
        .map(|seen| seen.contains(step))
        .unwrap_or(false)
}

/// Mark this step as told. Idempotent; a lock failure simply means the line may be said again, which is the
/// lesser harm compared with swallowing the only explanation the user gets.
fn mark_told(step: &str) {
    if let Ok(mut seen) = told_steps().lock() {
        seen.insert(step.to_string());
    }
}

/// Whether this machine can drive a browser at all, and what the filesystem said when it could not.
///
/// Flatpak comes from [`crate::machine_store::inside_flatpak`] — the same door §7's desktop registration uses,
/// reading `FLATPAK_ID`/`/.flatpak-info` — because inside the sandbox there is no browser to reach whatever the
/// setting names, and §2 puts that reason first.
///
/// An EMPTY setting is reported as available here even though nothing was named: §2's own line for that case is
/// "no firefox found -- name one in the settings", but an empty box also means "look for firefox on PATH", which
/// is [`crate::web_browser::firefox_path`]'s job. A flow asks this before it would have launched anything, so it
/// must not decide the question the lookup answers; a machine with a browser installed keeps both web tools.
pub fn availability(firefox: &str) -> (bool, String) {
    if crate::machine_store::inside_flatpak() {
        return (false, String::new());
    }
    if firefox.trim().is_empty() {
        return (true, String::new());
    }
    crate::web_browser::availability(firefox)
}

/// The reason a step got no web tools, ONCE per step: `Some(line)` the first time, `None` ever after (and `None`
/// while there is nothing to explain). A flow logs what it gets and nothing else.
///
/// An empty setting says "name one in the settings" only when the machine has no browser to find either — see
/// [`availability`].
pub fn unavailable_log(step: &str, firefox: &str) -> Option<String> {
    if told(step) {
        return None;
    }
    let flatpak = crate::machine_store::inside_flatpak();
    let (found, reason) = availability(firefox);
    // The wording door handles the two settings-only cases itself and takes the lookup's answer for the rest.
    let stat = if firefox.trim().is_empty() || flatpak {
        String::new()
    } else {
        reason
    };
    let line = crate::web_tools::unavailable_log(
        step,
        if firefox.trim().is_empty() && found {
            "firefox"
        } else {
            firefox
        },
        flatpak,
        found,
        &stat,
    )?;
    mark_told(step);
    Some(line)
}

/// The web half of a job's offer: the catalogue minus `web_search`/`web_read` when no browser can be driven, and
/// the reason line if this step has not heard it yet.
///
/// Callers that build their list elsewhere (the cut's §8 withdrawal rules own that list) use [`told`] +
/// [`unavailable_log`] instead and keep their own filtering.
pub fn offer(step: &str, firefox: &str, offered: &[Tool]) -> (Vec<Tool>, Option<String>) {
    let line = unavailable_log(step, firefox);
    let (found, _) = availability(firefox);
    (crate::web_tools::filter(offered, found), line)
}
