//! The waveform cache of spec/01-project-and-files.md §6: `cache/waves/<lane>.wave`, magic
//! `AWV4`, a header naming the recording it was made from, then the peak bytes.
//!
//! A cache file is disposable, so nothing in here fails on a bad one — an unreadable wave
//! is a miss, and the lane is measured again. Only writing our own garbage is an error,
//! because that is a bug rather than a stale file.

use std::path::Path;

use crate::layout;

/// The format's name in its first four bytes. It changes when the layout does, so an old
/// cache is simply not read rather than read as something it is not — the prototype has
/// moved AWV2 → AWV3 → AWV4 for exactly that reason (gui/cut_audio.go:250-256).
pub const MAGIC: &[u8; 4] = b"AWV4";

/// The header's length after the magic: `chans` u8, `hz` u16, `count` u32, `size` i64,
/// `mtime` i64 — little-endian and unpadded, as §6 spells it. 23 bytes, 27 from the start
/// of the file.
pub const HEADER_LEN: usize = 1 + 2 + 4 + 8 + 8;

/// A lane's peaks: one block of bytes per channel, all the same length, at `hz` samples of
/// envelope per second.
#[derive(Debug, Clone, PartialEq)]
pub struct Wave {
    /// Peaks per second. The header keeps whole hertz (§6's `hz` is a u16), so a fractional
    /// rate is the writer's to round before it gets here.
    pub hz: f64,
    pub chans: Vec<Vec<u8>>,
}

/// The bytes of one cache file.
///
/// An empty or ragged `Wave` is an `Err`, not a miss: this build would have written the
/// nonsense itself, and a cache it cannot read back is a bug to be fixed at the writer, not
/// papered over by silently recomputing on every start.
pub fn encode(wave: &Wave, size: i64, mtime: i64) -> Result<Vec<u8>, String> {
    let count = match wave.chans.first() {
        None => return Err("a wave with no channels has nothing to cache".to_string()),
        Some(first) => first.len(),
    };
    if wave.chans.iter().any(|c| c.len() != count) {
        // One `count` in the header covers every block, so blocks of different lengths
        // could not be read back even by whoever wrote them.
        return Err("a wave's channels must hold the same number of peaks".to_string());
    }
    if count > u32::MAX as usize {
        return Err(format!("{} peaks is past what the header holds", count));
    }
    if wave.hz < 1.0 || wave.hz > f64::from(u16::MAX) {
        // `hz` is a whole number in the header and nought means "not a rate" to the reader,
        // so a rate that cannot be stored is a caller's mistake, not a cache miss.
        return Err(format!("{} peaks per second does not fit the header", wave.hz));
    }

    let mut out = Vec::with_capacity(HEADER_LEN + wave.chans.len() * count);
    out.extend_from_slice(MAGIC);
    out.push(wave.chans.len() as u8);
    // Whole hertz, and nought means "not a rate" — which is why the reader rejects it.
    out.extend_from_slice(&(wave.hz as u16).to_le_bytes());
    out.extend_from_slice(&(count as u32).to_le_bytes());
    out.extend_from_slice(&size.to_le_bytes());
    out.extend_from_slice(&mtime.to_le_bytes());
    for chan in &wave.chans {
        out.extend_from_slice(chan);
    }
    Ok(out)
}

/// Read a cache file, or say it is not one. `size` and `mtime` are the source recording's:
/// a wave cached against a different cut of the same file is a different answer, so a
/// mismatch is a miss rather than a wrong picture on the page.
///
/// Every rejection — wrong magic, a short header, an impossible channel count or rate, a
/// truncated block, trailing bytes, a stale `size`/`mtime` — returns `None`. That is the
/// prototype's rule (gui/cut_audio.go:290-318): a cache that cannot be read is measured
/// again, and an error would stop a page from opening over a file whose only job is to save
/// some work.
pub fn decode(bytes: &[u8], size: i64, mtime: i64) -> Option<Wave> {
    if bytes.len() < MAGIC.len() + HEADER_LEN || &bytes[..MAGIC.len()] != MAGIC {
        return None;
    }
    let head = &bytes[MAGIC.len()..MAGIC.len() + HEADER_LEN];
    let chans = head[0];
    // A u16 at 1, not at 2: the header is packed, with no alignment padding.
    let hz = u16::from_le_bytes([head[1], head[2]]);
    let count = u32::from_le_bytes(head[3..7].try_into().ok()?) as usize;
    let stored_size = i64::from_le_bytes(head[7..15].try_into().ok()?);
    let stored_mtime = i64::from_le_bytes(head[15..23].try_into().ok()?);

    if chans == 0 || chans > 2 || hz == 0 {
        return None;
    }
    if stored_size != size || stored_mtime != mtime {
        return None;
    }
    let body = &bytes[MAGIC.len() + HEADER_LEN..];
    // Exactly `chans` blocks of `count`: a short file was cut off mid-write, and a long one
    // says something about the format this reader does not know. A zero count is allowed —
    // it is a recording too short to hold a peak — and its blocks are empty.
    if body.len() != usize::from(chans) * count {
        return None;
    }
    // A zero count is allowed — it is a recording too short to hold a peak — and its blocks
    // are empty, so there is nothing to cut.
    let chans = if count == 0 {
        vec![Vec::new(); usize::from(chans)]
    } else {
        body.chunks(count).map(<[u8]>::to_vec).collect()
    };
    Some(Wave { hz: f64::from(hz), chans })
}

/// Cache one lane's peaks, beside the lane's own file in `cache/waves`.
pub fn write(tree: &layout::Tree, lane: &str, wave: &Wave, size: i64, mtime: i64) -> Result<(), String> {
    let bytes = encode(wave, size, mtime)?;
    let path = tree.wave(lane);
    // The path comes from layout and goes through write_file, so §1's 0755 directory spine
    // and 0644 file mode hold without this module repeating them.
    let rel = relative(&path, tree.dir());
    tree.write_file(&rel, &bytes)
}

/// Read one lane's cached peaks: `None` when there is no cache or it does not apply to the
/// recording that is being drawn now.
pub fn read(tree: &layout::Tree, lane: &str, size: i64, mtime: i64) -> Result<Option<Wave>, String> {
    let path = tree.wave(lane);
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(format!("{}: {err}", path.display())),
    };
    Ok(decode(&bytes, size, mtime))
}

/// The path as `Tree::write_file` wants it: inside the project, with the project taken off
/// the front.
fn relative(path: &Path, dir: &Path) -> std::path::PathBuf {
    path.strip_prefix(dir)
        .unwrap_or(path)
        .to_path_buf()
}
