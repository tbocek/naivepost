//! The prompt keys and their thinking flags — `spec/10-parameters.md` §4.

use crate::roles::Job;

pub const KEYS: [&str; 14] = [
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
    "textgate",
];

pub const THINKING_ON: [&str; 4] = ["textedit", "cut", "narrate", "youtube"];

pub fn is_known(key: &str) -> bool {
    KEYS.contains(&key)
}

/// Whether this prompt is asked to think. True for exactly [`THINKING_ON`].
pub fn thinking(key: &str) -> bool {
    THINKING_ON.contains(&key)
}

pub const NOT_THINKING: &str = "__execute__";

/// §4's flag as the one switch a request body actually carries.
pub fn mode_of(key: &str) -> crate::exchanges::Mode {
    if thinking(key) {
        crate::exchanges::Mode::Thinking
    } else {
        crate::exchanges::Mode::Execute
    }
}

/// Which job this prompt belongs to, `None` for the two that have none (`system`, `policy`).
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

pub fn shipped_text(key: &str) -> Option<String> {
    let raw = read_shipped(shipped_file(key)?)?;
    Some(fenced_block(&raw).unwrap_or_else(|| raw.trim().to_string()))
}

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
            "textgate" => "prompts/textgate.md",
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
