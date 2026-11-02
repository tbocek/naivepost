//! §09-llm-and-tools#6-cache — the reply cache under `cache/llm/<step>/`.
//!
//! Spec: `spec/09-llm-and-tools.md` §6. This module owns what §6 decides: the **key** ([`parts`] + [`key`]), **what may be stored** ([`usable`], [`store`]) and **who caches** ([`uses_cache`], `fixer_caches` / [`gap_fill_caches`]).
//!
//! Two deliberate divergences from what else is in this tree:
//!
//! * [`crate::fix_transcripts::block_key`] keeps its own 16-char hash (F1.8 S3 pins it; folding it in would move every answer under `cache/llm/fix`).
//! * The prototype keyed on the texts but not the model id or thinking flag, which §6 overturns; both are parts here, so a swapped model asks again.

use serde_json::{json, Value};

use crate::layout::Tree;
use sha2::{Digest, Sha256};

/// What decides an answer — §6's list, in §6's own order, with the rewrite's two additions last.
///
/// Everything that changes the question must be present and nothing else: a missing field makes a stale answer look fresh.
#[derive(Debug, Clone)]
pub struct Request<'a> {
    /// The system prompt as the job received it (sections cut per §8). An edited prompt misses.
    pub system: &'a str,
    /// The user text: the job's material plus the User Context block riding with it.
    pub user: &'a str,
    /// Rolling state — describe's running STATE, the summary carried into the next chunk.
    pub state: &'a str,
    /// The speech the answer was grounded in.
    pub speech: &'a str,
    /// Surrounding context sent with the request (the grounding lines, the clip brief).
    pub context: &'a str,
    /// Image data URLs, each a part of its own and positional (the frame in slot 2 changing is a different question). Owned so callers needn't thread a lifetime.
    pub images: Vec<String>,
    /// Which pooled draw this is (the retake pool asks the same question N times); `None` if not pooled.
    pub run_index: Option<u32>,
    /// The rewrite's addition (§6): the model id, so a model swap does not replay old answers.
    pub model: &'a str,
    /// The rewrite's addition (§6): whether the model was asked to think.
    pub thinking: bool,
}

/// The deciding parts, in order, as JSON values — the exact bytes [`key`] hashes.
///
/// Separate from [`key`] so a test can see the list, and a caller can tell "nothing to key on" from an odd key.
pub fn parts(request: &Request) -> Vec<Value> {
    let mut out = vec![
        json!(request.system),
        json!(request.user),
        json!(request.state),
        json!(request.speech),
        json!(request.context),
    ];
    // One part per image in sending order: swapping two frames changes the answer, so the key.
    for image in &request.images {
        out.push(json!(image));
    }
    // A `null` part, not a skipped one: `Some(0)` and `None` must not collide.
    out.push(match request.run_index {
        Some(index) => json!(index),
        None => Value::Null,
    });
    out.push(json!(request.model));
    out.push(json!(request.thinking));
    out
}

/// The cache key: lowercase hex sha256 over each part's JSON encoding followed by one NUL byte.
///
/// The NUL pins the boundary between parts, so no split of one text across two parts can produce another split's digest.
///
/// `None` means no key: an empty part list, or a part `serde_json` refuses (`f64::NAN`, infinity). On `None`, [`read`] finds nothing and [`store`] writes nothing.
pub fn key(parts: &[Value]) -> Option<String> {
    if parts.is_empty() {
        return None;
    }
    let mut hasher = Sha256::new();
    for part in parts {
        let bytes = serde_json::to_vec(part).ok()?;
        hasher.update(&bytes);
        hasher.update([0u8]);
    }
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        hex.push_str(&format!("{byte:02x}"));
    }
    Some(hex)
}

/// Whether a reply is worth storing. An empty reply never is: caching it would turn a step that could still be answered into a permanent "nothing".
pub fn usable(reply: &str) -> bool {
    !reply.trim().is_empty()
}

/// What [`store`] came to. `Failed` carries a log sentence, not an error: an unwritable cache makes a slower step, never a failed one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stored {
    Written,
    RefusedEmpty,
    RefusedNoKey,
    Failed(String),
}

/// The stored answer for these parts, if any. An unreadable file is a miss, so the step asks again (as [`crate::wave::read`]).
pub fn read(tree: &Tree, step: &str, parts: &[Value]) -> Option<String> {
    let key = key(parts)?;
    std::fs::read_to_string(tree.cache_llm(step, &key)).ok()
}

/// The answer filed under a key a caller computed itself ([`store_at`]'s twin), for jobs whose digest has no field in §6's [`Request`] (describe folds its frames in). A name `key` could never produce is a miss.
pub fn read_at(tree: &Tree, step: &str, key: &str) -> Option<String> {
    std::fs::read_to_string(tree.cache_llm(step, key)).ok()
}

/// File an answer under a key the caller holds, with §6's store rules: an empty reply is never written, and a failed write comes back as [`Stored::Failed`].
pub fn store_at(tree: &Tree, step: &str, key: &str, reply: &str) -> Stored {
    if !usable(reply) {
        return Stored::RefusedEmpty;
    }
    write_at(tree, step, key, reply)
}

/// The write both [`store`] and [`store_at`] come down to: make the step's folder, then the file.
fn write_at(tree: &Tree, step: &str, key: &str, reply: &str) -> Stored {
    let path = tree.cache_llm(step, key);
    if let Some(parent) = path.parent() {
        if let Err(err) = std::fs::create_dir_all(parent) {
            return Stored::Failed(format!("{}: {err}", parent.display()));
        }
    }
    match std::fs::write(&path, reply) {
        Ok(()) => Stored::Written,
        Err(err) => Stored::Failed(format!("{}: {err}", path.display())),
    }
}

/// Store an answer under its key; nothing touches the filesystem unless it is both keyable and usable.
pub fn store(tree: &Tree, step: &str, parts: &[Value], reply: &str) -> Stored {
    let Some(key) = key(parts) else {
        return Stored::RefusedNoKey;
    };
    if !usable(reply) {
        return Stored::RefusedEmpty;
    }
    write_at(tree, step, &key, reply)
}

/// Which jobs use this cache: the five §6 names — describe, transcript (the fixer), textedit
/// (joins), retake, translate — and nobody else.
///
/// The cut, narration and upload text are non-users: their answers must change when the user moves a cut or edits the prompt
/// ([`crate::narrate_pass::served_from_cache`] answers `false` for narrate). The thumbnail gets no cache either: §6 does not name it.
pub fn uses_cache(job: crate::roles::Job) -> bool {
    matches!(
        job,
        crate::roles::Job::Describe
            | crate::roles::Job::CleanTranscript
            | crate::roles::Job::Joins
            | crate::roles::Job::Retakes
            | crate::roles::Job::Subtitles
    )
}

/// §6's first irregularity, the half [`crate::produce_subtitles::cached`] does not cover: completeness gates the first call, and the gap-filling second call is never cached.
pub fn gap_fill_caches(second_call: bool) -> bool {
    !second_call
}

/// What translate puts in the file: the reconstructed numbered text, not the reply, so a replayed batch fits [`crate::produce_subtitles::merge`] without re-parsing prose.
pub fn translate_store_value(reconstructed: &str, _reply: &str) -> String {
    reconstructed.to_string()
}

/// The two things a cache hit comes ahead of.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ahead {
    /// The slot gate of §4 — a hit never takes a slot, so it never queues behind a busy model.
    Gate,
    /// The exchange log of §7 — a hit opens no page section.
    ExchangeLog,
}
