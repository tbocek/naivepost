//! Probing a file for the facts its header does not carry
//! (§03-shell#8-details-confirmed-against-the-code-verification-pass).
//!
//! A recorder killed mid-write leaves a container whose header has no duration — the
//! length is written when the file is closed. The only way to get one is to decode the
//! file, which costs the whole of it, so it happens once and the answer is remembered per
//! path + size + mtime exactly like every other probe here (`wave`'s header carries the
//! same pair for the same reason). A file that changed is a different file: its old
//! duration says nothing about it.

use std::collections::HashMap;

/// What identifies one measurement. `mtime` and `size` come from the file's own metadata,
/// so a re-recorded or truncated file misses rather than answering with stale seconds.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Key {
    pub path: String,
    pub size: i64,
    pub mtime: i64,
}

/// A probe key. Named so a caller cannot pass the three fields in the wrong order — a
/// swapped size and mtime still looks like a number.
pub fn key(path: &str, size: i64, mtime: i64) -> Key {
    Key { path: path.to_string(), size, mtime }
}

/// One probe line, tab-separated and complete, the way §6's other sidecars are written:
/// a reader should not have to know how many columns this format has. Whole seconds — the
/// decode answers in whole seconds, and a float would print differently on each side of
/// the round-trip that `wave`'s header avoids by storing an integer.
pub fn render(key: &Key) -> String {
    format!("{}\t{}\t{}", key.path, key.size, key.mtime)
}

/// How a duration was arrived at, because the two costs are not comparable and a tool that
/// decoded a 40-minute file should say so.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Header,
    Decoded,
}

/// The header's answer wins; only a header that does not carry one pays for a decode.
/// Neither present is an error naming the file — "no duration" is not a fact about the
/// rest of the pipeline, and a message without the path cannot be acted on.
pub fn duration(
    path: &str,
    header: Option<f64>,
    decoded: Option<f64>,
) -> Result<(f64, Source), String> {
    if let Some(secs) = header {
        return Ok((secs, Source::Header));
    }
    if let Some(secs) = decoded {
        return Ok((secs, Source::Decoded));
    }
    Err(format!("{path}: no duration in the header and nothing to decode it from"))
}

/// Durations measured this launch, keyed by [`Key`]. In memory: a duration is cheap to
/// re-measure compared with a transcript, and a cache file would outlive the recording it
/// describes without saying so.
#[derive(Debug, Default)]
pub struct Cache {
    durations: HashMap<Key, f64>,
}

impl Cache {
    pub fn new() -> Self {
        Self::default()
    }

    /// The measurement for this exact file as it stands now. A different size or mtime is
    /// a miss, not a stale answer.
    pub fn lookup(&self, key: &Key) -> Option<f64> {
        self.durations.get(key).copied()
    }

    pub fn store(&mut self, key: Key, secs: f64) {
        self.durations.insert(key, secs);
    }
}
