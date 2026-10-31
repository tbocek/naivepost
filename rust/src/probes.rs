//! Probing a file for the facts its header does not carry
//! (§03-shell#8-details-confirmed-against-the-code-verification-pass).
//!
//! A recorder killed mid-write leaves a container with no duration in its header. The only way to get one is
//! to decode the file, so it happens once and is remembered per path + size + mtime (as `wave`'s header does);
//! a changed file is a different file.

use std::collections::HashMap;

/// What identifies one measurement. `mtime` and `size` come from the file's metadata, so a re-recorded or
/// truncated file misses rather than answering with stale seconds.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Key {
    pub path: String,
    pub size: i64,
    pub mtime: i64,
}

/// A probe key. Named so a caller cannot pass the three fields in the wrong order.
pub fn key(path: &str, size: i64, mtime: i64) -> Key {
    Key {
        path: path.to_string(),
        size,
        mtime,
    }
}

/// One probe line, tab-separated and complete, as §6's other sidecars are written. Whole seconds: a float
/// would print differently on each side of the round-trip.
pub fn render(key: &Key) -> String {
    format!("{}\t{}\t{}", key.path, key.size, key.mtime)
}

/// How a duration was arrived at: the two costs are not comparable and a 40-minute decode should say so.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Header,
    Decoded,
}

/// The header's answer wins; only a header without one pays for a decode. Neither present is an error
/// naming the file.
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
    Err(format!(
        "{path}: no duration in the header and nothing to decode it from"
    ))
}

/// Durations measured this launch, keyed by [`Key`]. In memory: re-measuring is cheap next to a transcript,
/// and a cache file would outlive the recording it describes.
#[derive(Debug, Default)]
pub struct Cache {
    durations: HashMap<Key, f64>,
}

impl Cache {
    pub fn new() -> Self {
        Self::default()
    }

    /// The measurement for this exact file as it stands now; a different size or mtime is a miss.
    pub fn lookup(&self, key: &Key) -> Option<f64> {
        self.durations.get(key).copied()
    }

    pub fn store(&mut self, key: Key, secs: f64) {
        self.durations.insert(key, secs);
    }
}
