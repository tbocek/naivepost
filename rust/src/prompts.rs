//! The prompt keys and their thinking flags — `spec/10-parameters.md` §4.
//!
//! §4 is one sentence, and it is a catalogue rather than a flow: thirteen prompts, named in the
//! order the pipeline sends them, and which four of them are asked to think first. Everything here
//! is a lookup on that sentence; the wording itself ships as files under `prompts/`, and storing an
//! edit on this machine is [`crate::settings`]'s business.
//!
//! Two other modules answer questions near this one and neither owns it:
//! [`crate::roles::thinking`] answers per *job* (which is who serves the request), while
//! [`crate::bench`] lists the rows the Prepare page shows. This module answers per *prompt key*,
//! which is what a person edits and what `~/.config/naivepost/prompts/<key>.txt` is named after.
//! The tests hold the three tables level rather than folding them into one another: they describe
//! different things, and `system` and `policy` exist only at this level.

use crate::roles::Job;

/// The thirteen prompts §4 names, in the order the spec lists them — which is pipeline order:
/// the shared preamble, then Prepare's three, then the text edit, the cut and its three passes,
/// then the narration, the translation and the upload text, and last the new policy derivation.
///
/// `tools` is deliberately absent although `prompts/tools.md` sits beside these files: that file
/// holds the web tools' descriptions ([`crate::web_tools`] copies them verbatim), not a system
/// prompt anyone can send.
pub const KEYS: [&str; 13] = [
    "system",
    "describe",
    "fix",
    "retake",
    "textedit",
    "cut",
    "captions",
    "speed",
    "effects",
    "narrate",
    "translate",
    "youtube",
    "policy",
];

/// The four §4 gives thinking: "Thinking on for textedit, cut, narrate, youtube; off elsewhere."
/// Named once, as a set, so adding a fifth is a deliberate edit to this line rather than a default
/// flipping somewhere else.
pub const THINKING_ON: [&str; 4] = ["textedit", "cut", "narrate", "youtube"];

/// Whether this build knows this key. An unknown key is not an error to the storage layer — it is
/// simply a file nothing reads — but the bench and every caller that picks a wording asks first, so
/// a typo cannot quietly create a prompt nobody sends.
pub fn is_known(key: &str) -> bool {
    KEYS.contains(&key)
}

/// Whether this prompt is asked to think. True for exactly [`THINKING_ON`].
///
/// This does not call [`crate::roles::thinking`], even though the two agree wherever they overlap:
/// two of the thirteen keys have no job row at all, so the mapping would have to invent one.
/// - `system` is a preamble sent in front of the others; it is never a call of its own, so its
///   own thinking flag means nothing and stays off.
/// - `policy` is F0.7's derivation, which [F0.7 S2](../spec/03-shell.md) asks with thinking
///   off — the fields it sets are small and the answer has to be cheap enough to re-run on every
///   context change.
///
/// The tests pin that the two tables agree for every key that *does* have a job, which is where a
/// drift (a job retuned to think while its prompt still says off) would otherwise go unnoticed.
pub fn thinking(key: &str) -> bool {
    THINKING_ON.contains(&key)
}

/// The non-thinking switch, spelled once: §4 lists twelve keys and every ask that is told not to think
/// (§3's `thinkAgain` retry, and any key outside the four) gets exactly the mode this unnamed key maps
/// to. Public so an ask module can take its execute mode from the table instead of naming a `Mode`.
pub const NOT_THINKING: &str = "__execute__";

/// §4's flag as the one switch a request body actually carries.
///
/// Every live ask builds its body through [`crate::llm_request::body`], whose `Mode` argument IS the
/// thinking switch (§09 §1: the two modes are the whole of it). Reading the flag here instead of naming
/// a `Mode` at each call site is what makes §4's "off elsewhere" true on the wire rather than only in
/// this catalogue: a prompt retuned in [`THINKING_ON`] changes what goes out, and a call site cannot
/// quietly disagree with the table the way `captions_ask` used to (it sent `Mode::Thinking` for a job
/// §4 lists as thinking-off).
pub fn mode_of(key: &str) -> crate::exchanges::Mode {
    if thinking(key) {
        crate::exchanges::Mode::Thinking
    } else {
        crate::exchanges::Mode::Execute
    }
}

/// Which job this prompt belongs to, `None` for the two that have none (`system`, `policy`).
///
/// Three prompts share one job: captions, speed and effects are the same `ClipRules` call shape
/// asked three times over, clip by clip, so §4's three keys land on [`Job::ClipRules`] together.
pub fn job(key: &str) -> Option<Job> {
    match key {
        "describe" => Some(Job::Describe),
        "fix" => Some(Job::CleanTranscript),
        "retake" => Some(Job::Retakes),
        "textedit" => Some(Job::Joins),
        "cut" => Some(Job::ModelCut),
        "captions" | "speed" | "effects" => Some(Job::ClipRules),
        "narrate" => Some(Job::Narrate),
        "translate" => Some(Job::Subtitles),
        "youtube" => Some(Job::UploadText),
        _ => None,
    }
}

/// The first ``` fenced block of a markdown file, its contents trimmed. `None` when there is none.
pub fn fenced_block(markdown: &str) -> Option<String> {
    let opened = markdown.find("```")?;
    let rest = &markdown[opened + 3..];
    // Skip the language tag line (```text) if the fence carries one.
    let body = match rest.find('\n') {
        Some(newline) => &rest[newline + 1..],
        None => rest,
    };
    let closed = body.find("```")?;
    Some(body[..closed].trim().to_string())
}

/// One prompt's shipped wording, read from the tree: the text of the fenced ``` block of
/// `spec/prompts/<key>.md`, which is what the model is meant to receive — the markdown around it
/// is for readers of the spec. A file with no fence is taken whole, trimmed; `None` when the key
/// has no shipped file or it cannot be read.
///
/// [`shipped_file`] answers only the name (`prompts/system.md`), relative to the tree root; this is
/// the reader. The root is found by walking up from the executable or the working directory rather
/// than hardcoded, so the same binary reads its prompts from an install tree, a dev checkout, or a
/// test run out of `rust/`. Every caller that asks a model something goes through here, and so does
/// the Prepare bench, which shows exactly this wording in its box (§04-prepare#1-screen).
pub fn shipped_text(key: &str) -> Option<String> {
    let raw = read_shipped(shipped_file(key)?)?;
    Some(fenced_block(&raw).unwrap_or_else(|| raw.trim().to_string()))
}

/// Find and read one shipped prompt by its root-relative name. Two starting points are tried
/// (executable, then working directory) because a test runs from `rust/` while the installed
/// binary runs from anywhere.
fn read_shipped(rel: &str) -> Option<String> {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.to_path_buf()))
        .unwrap_or_default();
    let cwd = std::env::current_dir().unwrap_or_default();
    // `prompts/system.md` is the name `shipped_file` gives; the files sit under the tree root's
    // `spec/` folder, so both `<root>/prompts/…` (an install that ships the prompts beside the
    // binary) and `<root>/spec/prompts/…` (this checkout) are honoured.
    for start in [exe_dir, cwd] {
        let mut dir = start.as_path();
        // Bounded walk: `rust/target/debug/naivepost` is three levels under the root, and a
        // nested dev tree is deeper; six covers both without hunting the filesystem.
        for _ in 0..6 {
            for candidate in [dir.join(rel), dir.join("spec").join(rel)] {
                if let Ok(text) = std::fs::read_to_string(&candidate) {
                    return Some(text);
                }
            }
            match dir.parent() {
                Some(parent) => dir = parent,
                None => break,
            }
        }
    }
    None
}

/// Where this build keeps one prompt's shipped wording, relative to the tree root:
/// `prompts/<key>.md`. Known keys answer `Some`; anything else answers `None` rather than
/// building a path to a file that cannot exist.
pub fn shipped_file(key: &str) -> Option<&'static str> {
    if is_known(key) {
        // Boxed-free: the string is made from a const literal per key, so a static table backs it
        // rather than a format!() on a hot path.
        Some(match key {
            "system" => "prompts/system.md",
            "describe" => "prompts/describe.md",
            "fix" => "prompts/fix.md",
            "retake" => "prompts/retake.md",
            "textedit" => "prompts/textedit.md",
            "cut" => "prompts/cut.md",
            "captions" => "prompts/captions.md",
            "speed" => "prompts/speed.md",
            "effects" => "prompts/effects.md",
            "narrate" => "prompts/narrate.md",
            "translate" => "prompts/translate.md",
            "youtube" => "prompts/youtube.md",
            "policy" => "prompts/policy.md",
            _ => return None,
        })
    } else {
        None
    }
}
