//! §07-narrate#1-screen, number **9**: the voices folder's own arithmetic.
//!
//! "Add file…" copies a recording INTO this folder and the picker then lists it ("every `.wav` in the
//! voices folder"), so two questions have to be answered the same way by whoever asks: which name a
//! copied recording takes, and what is already in there. Both live here as plain string work; the
//! folder itself is [`crate::settings::voices_folder`]'s answer (Settings' `AUDIOCPP_VOICES`, else the
//! legacy models root's `voices/`, else the platform's), never a path spelled out in a widget.

use std::path::Path;

/// The suffix every listed voice carries (§A.6: "every .wav in the voices folder").
pub const VOICE_SUFFIX: &str = ".wav";

/// The name a copied recording takes inside the folder: its base name with everything that is not a
/// letter, a digit, `-` or `_` folded to `-`, runs of them collapsed, and no leading dot (a dotted name
/// would vanish from a listing and re-appear as hidden). §S1's "under a sanitised base name".
pub fn sanitised_base(path: &Path) -> String {
    let raw = path
        .file_stem()
        .map(|stem| stem.to_string_lossy().to_string())
        .unwrap_or_default();
    let mut out = String::new();
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    // An all-punctuation name ("!!!.wav") sanitises to nothing; a folder cannot hold "" and a voice
    // called "-" is worse than one called `voice`.
    if trimmed.is_empty() || trimmed.starts_with('.') {
        return "voice".to_string();
    }
    trimmed
}

/// The name that does NOT overwrite anything already in the folder: `base`, else `base-2`, `base-3`, …
/// (§S1's "never overwriting (`name-2`, …)"). Comparison is on the bare base, since every candidate
/// gets the same suffix.
pub fn unique_name(taken: &[String], base: &str) -> String {
    let held: Vec<String> = taken
        .iter()
        .map(|name| name.trim_end_matches(VOICE_SUFFIX).to_string())
        .collect();
    if !held.iter().any(|name| name == base) {
        return base.to_string();
    }
    let mut n = 2usize;
    loop {
        let candidate = format!("{base}-{n}");
        if !held.contains(&candidate) {
            return candidate;
        }
        n += 1;
    }
}

/// The folder's `.wav` names, sorted — the order the picker lists them in, and the list a copy is
/// checked against so a second add of the same recording lands as `name-2.wav`. A missing or
/// unreadable folder answers empty rather than failing: an empty folder is a real state.
pub fn list_wavs(dir: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .flatten()
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| name.ends_with(VOICE_SUFFIX))
        .collect();
    names.sort();
    names
}

/// The full file name a copy lands under: the sanitised base, made unique, plus the suffix.
pub fn target_name(taken: &[String], source: &Path) -> String {
    format!(
        "{}{VOICE_SUFFIX}",
        unique_name(taken, &sanitised_base(source))
    )
}
