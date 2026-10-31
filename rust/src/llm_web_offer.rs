//! §2's "whether tools are offered" half: a step that gets no web tools says WHY, and says it ONCE.

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
pub fn unavailable_log(step: &str, firefox: &str) -> Option<String> {
    if told(step) {
        return None;
    }
    let flatpak = crate::machine_store::inside_flatpak();
    let (found, reason) = availability(firefox);
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
pub fn offer(step: &str, firefox: &str, offered: &[Tool]) -> (Vec<Tool>, Option<String>) {
    let line = unavailable_log(step, firefox);
    let (found, _) = availability(firefox);
    (crate::web_tools::filter(offered, found), line)
}
