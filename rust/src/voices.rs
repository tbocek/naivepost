//! §07-narrate#1-screen, number **9**: the voices folder's own arithmetic.

use std::path::Path;

/// The suffix every listed voice carries (§A.6: "every .wav in the voices folder").
pub const VOICE_SUFFIX: &str = ".wav";

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
    if trimmed.is_empty() || trimmed.starts_with('.') {
        return "voice".to_string();
    }
    trimmed
}

pub fn unique_name(taken: &[String], base: &str) -> String {
    let held: Vec<String> = taken
        .iter()
        .map(|name| name.trim_end_matches(VOICE_SUFFIX).to_string())
        .collect();
    crate::narrate_details::added_file_name(base, &held)
}

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
