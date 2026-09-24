//! §09-llm-and-tools#6-cache — the reply cache under `cache/llm/<step>/`.
//!
//! Spec: `spec/09-llm-and-tools.md` §6. The folder itself is already
//! [`crate::layout::Tree::cache_llm`]'s; this module owns the three things §6 actually decides:
//! the **key** that names a file inside it ([`parts`] + [`key`]), **what may be stored**
//! ([`usable`], [`store`]) and **who caches** ([`uses_cache`], plus the two irregularities in
//! [`fixer_caches`] / [`gap_fill_caches`]).
//!
//! Two deliberate divergences from what else is in this tree:
//!
//! * [`crate::fix_transcripts::block_key`] keeps its own 16-char hash. F1.8 S3 pins that length,
//!   the fixer's files are read by nothing outside that module, and folding it in here would move
//!   every answer already written under `cache/llm/fix`. This module is §6's sha256 format for the
//!   shared cache; the fixer is its own first draft of the same idea.
//! * The prototype keyed on the deciding texts but **not** on the model id or the thinking flag,
//!   which is exactly the behaviour §6 overturns ("changing the model in Settings replays the old
//!   model's cached answers"). Both are parts here, so a swapped model asks again instead of
//!   replaying another model's work as if it were this one's.

use serde_json::{json, Value};

use crate::layout::Tree;
use sha2::{Digest, Sha256};

/// What decides an answer — §6's list, in §6's own order, with the rewrite's two additions last.
///
/// Everything a caller can change without changing the question must be absent, and everything that
/// changes it must be present: a missing field makes a stale answer look like a fresh one.
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
    /// Image data URLs, each one a part of its own. Positional: the frame in slot 2 changing is a
    /// different question, so they are never merged into one blob. Owned rather than borrowed so a
    /// caller can build a request from lists it just made without threading a lifetime through.
    pub images: Vec<String>,
    /// Which pooled draw this is — the retake pool asks the same question N times and each draw is
    /// its own answer. `None` for a call that is not pooled.
    pub run_index: Option<u32>,
    /// The rewrite's addition (§6): the model id the answer came from. Without it a model swap
    /// replays the previous model's answers forever.
    pub model: &'a str,
    /// The rewrite's addition (§6): whether the model was asked to think. A thinking answer and an
    /// executing answer to the same texts are not the same answer.
    pub thinking: bool,
}

/// The deciding parts, in order, as JSON values — the exact bytes [`key`] hashes.
///
/// Kept separate from [`key`] so a test can see the list rather than only its digest, and so a
/// caller can tell "nothing to key on" apart from a key that happens to look odd.
pub fn parts(request: &Request) -> Vec<Value> {
    let mut out = vec![
        json!(request.system),
        json!(request.user),
        json!(request.state),
        json!(request.speech),
        json!(request.context),
    ];
    // One part per image, in the order they were sent: swapping two frames changes the answer, so
    // it has to change the key.
    for image in &request.images {
        out.push(json!(image));
    }
    // A `null` part rather than a skipped one: an unpooled call is a different shape of question
    // from draw 0 of a pooled one, and `Some(0)`/`None` must not collide.
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
/// The NUL is the point of the format, not decoration: it pins the boundary between parts, so no
/// way of splitting one text across two parts can produce the digest of a different split.
///
/// `None` means "an uncomputable key is no key": an empty part list (a caller that has not asked a
/// question must not share a file with the digest of empty input) or a part `serde_json` refuses —
/// a `f64::NAN` or infinity, which has no JSON encoding. On `None`, [`read`] finds nothing and
/// [`store`] writes nothing.
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

/// Whether a reply is worth storing. An empty reply never is: caching it turns a step that could
/// still be answered later into a permanent "nothing", and the second attempt never happens.
pub fn usable(reply: &str) -> bool {
    !reply.trim().is_empty()
}

/// What [`store`] came to. `Failed` carries a sentence for the log rather than being propagated:
/// an unwritable cache makes a slower step, never a failed one — the same rule
/// [`crate::fix_transcripts`] spells as `!!! could not keep the fix`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stored {
    Written,
    RefusedEmpty,
    RefusedNoKey,
    Failed(String),
}

/// The stored answer for these parts, if there is one. Any unreadable file is a miss, so the step
/// asks again rather than failing — the rule [`crate::wave::read`] states for the wave cache: a
/// cache that cannot be read is measured again.
pub fn read(tree: &Tree, step: &str, parts: &[Value]) -> Option<String> {
    let key = key(parts)?;
    std::fs::read_to_string(tree.cache_llm(step, &key)).ok()
}

/// Store an answer under its key. Nothing touches the filesystem unless the answer is both keyable
/// and usable, so a refused answer leaves no empty file behind.
pub fn store(tree: &Tree, step: &str, parts: &[Value], reply: &str) -> Stored {
    let Some(key) = key(parts) else {
        return Stored::RefusedNoKey;
    };
    if !usable(reply) {
        return Stored::RefusedEmpty;
    }
    let path = tree.cache_llm(step, &key);
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

/// Which jobs use this cache: the five §6 names — describe, transcript (the fixer), textedit
/// (joins), retake, translate — and nobody else.
///
/// The cut, the narration and the upload text are named as non-users: their answers have to change
/// when the user moves a cut or edits the prompt, and [`crate::narrate_pass::served_from_cache`]
/// already answers `false` for narrate for that reason (its cache is the TTS one, where the words
/// are cheap and the voice is not). The thumbnail is in neither list and gets no cache either:
/// inventing one for a job §6 does not name would replay pictures nobody asked for.
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

/// §6's second irregularity: the transcript fixer caches only its first attempt. An answer repaired
/// after a refusal depends on the refusal it was shown, and replaying it would skip the validation
/// that made it right — so the retry is paid for again rather than trusted from disk.
pub fn fixer_caches(attempt: u32) -> bool {
    attempt == 1
}

/// §6's first irregularity, the half that [`crate::produce_subtitles::cached`] does not cover: the
/// gap-filling second call is never cached. The two rules are different and both hold — completeness
/// gates the first call (a partial batch cached is a partial batch forever), and this one says the
/// repair pass never earns a place in the cache at all.
pub fn gap_fill_caches(second_call: bool) -> bool {
    !second_call
}

/// What translate puts in the file: the reconstructed numbered text, not the reply. A replayed
/// batch then answers in the shape [`crate::produce_subtitles::merge`] wants and never re-parses
/// prose, which is why a model that worded its answer oddly cannot break a resumed run.
pub fn translate_store_value(reconstructed: &str, _reply: &str) -> String {
    reconstructed.to_string()
}

/// The two things a cache hit comes ahead of.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ahead {
    /// The slot gate of §4 — a hit never takes a slot, so it never queues behind a busy model.
    Gate,
    /// The exchange log of §7 — a hit opens no page section, so a resumed run's page holds only
    /// the calls that were actually made.
    ExchangeLog,
}

/// A hit is answered before either of them. What this encodes is the caller's order: check the
/// cache, return on a hit, and only then take a slot and open a page section. The hit is still
/// recorded where a program can read it — `requests.tsv`'s `Outcome::Cache` row, with no time on
/// the wire (§10) — just not on the human-facing page.
pub fn hit_answered_ahead(_stage: Ahead) -> bool {
    true
}
