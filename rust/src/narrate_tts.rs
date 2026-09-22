//! F4.4 Speak a line (TTS) — `spec/07-narrate.md` F4.4, steps S1–S6.
//!
//! One line of narration in, one wav out. The order is the flowchart's: the reference has to be on disk
//! (S1, built by [F4.6](crate::layout::Tree::voice_ref_wav)), the audio server has to be healthy and serving
//! a model that can clone (S2), the reference goes up again for this line (S3 — `bodies`'
//! [`crate::bodies::VOICE_REF_REUPLOADED_EVERY_LINE`: a remembered server path dies with a server restart),
//! then one `POST /v1/audio/speech` whose `options` carry the emotion either as eight floats or as words for
//! the judge (S4, S6), and the reply is a take only if it is a 200 of at least [`MIN_WAV_BYTES`] bytes (S5).
//!
//! Everything here is a plain function over data the caller already has: no socket, no widget, no thread. The
//! HTTP call belongs to [`crate::requests`] with the address and key from [`crate::services`]; this module
//! says what goes on the wire and what came back means.
//!
//! What this module does **not** own:
//! - the reference itself — [F4.6](crate::narrate_screen) chooses the voice and builds `narrate/voice_ref.wav`;
//!   [`reference_problem`] only asks whether that file exists yet;
//! - the request's address, key and timeout ([`crate::services`], [`crate::requests`]) and the body's five
//!   keys, which are [`crate::bodies::speech_request`]'s envelope;
//! - the take's name and seed — [`crate::narration::tts_key`] and [`crate::narration::tts_seed`] decide both,
//!   and this module only turns a key into the file it names ([`take_path`]);
//! - playing or auditioning the result (F4.5), which reads these wavs and never writes them;
//! - what the *model* is told about emotions ([`crate::narrate_pass::emotion_known`] and
//!   [`crate::narrate_pass::list_emotions`]). Those answer "is this word in the vocabulary"; this module is
//!   the one that turns a tag into wire bytes, and it never refuses one — an unknown name goes to the judge.

use std::path::PathBuf;

use serde_json::{json, Value};

use crate::bodies::ServerPath;
use crate::checks;
use crate::layout::Tree;
use crate::narrate_pass::EMOTION_KIN;
use crate::narration;
use crate::services::AudioModel;

/// S4: how hard the engine leans on a spoken-emotion judge's guess. A stronger blend is a different
/// performance of the same words, and this multiplier is also what a take's cache key is salted with
/// ([`crate::narration`]'s prefix), so changing it retunes every line in the project at once.
pub const EMOTION_ALPHA: &str = "0.85";

/// S4: the alpha that rides with an exact vector. Alpha is a multiplier over the weights, not a separate
/// dial — at 0.85 a written `angry=1` would arrive as 0.85 anger plus 0.15 of whatever the sample sounds
/// like, which is the one thing an exact weight is written to prevent. So the vector path sends 1.
pub const VECTOR_ALPHA: &str = "1";

/// S5: the smallest reply that can be a wav. A server that ran out of VRAM answers 200 with a JSON error,
/// and a few hundred bytes of prose written to `narrate/tts/` would be replayed forever as a silent take.
pub const MIN_WAV_BYTES: usize = 1000;

/// S4's P.policy.ttsLanguage fallback: the prototype hard-codes `"en"` here, which is the thing this replaces
/// — a model told to speak English reads Polish spelling as English spelling. It survives as what an empty
/// project language falls back to, never as what a project with one is overridden by.
pub const LANGUAGE_FALLBACK: &str = "en";

/// §10's `from` for [`LANGUAGE_FALLBACK`], spelled by the module that owns it so the catalogue cannot name a
/// constant that no longer exists. A row's `from` is a `&'static str` like every other, and this is `'static`.
pub const LANGUAGE_SOURCE: &str = "narrate_tts::LANGUAGE_FALLBACK";

/// How many characters of a server's own error are quoted back (S5): the diagnosis, without a stack trace
/// taking the log line over.
const PROBLEM_CHARS: usize = 300;

/// A named blend: a mix over the eight bases that no single base reaches — "excited" is happy plus surprise,
/// not a kin of happy. Recipes, not synonyms, and every one peaks at 1 so a weight means the same as on a
/// base. Nothing outside the eight axes can be invented (smug, sarcastic and whispered stay with the judge).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Blend {
    pub names: &'static [&'static str],
    /// The eight weights, in [`crate::narrate_screen::EMOTIONS`]' order: happy, angry, sad, afraid,
    /// disgusted, melancholic, surprised, calm.
    pub weights: [f64; 8],
}

/// The 21 blends, in the order the prototype lists them (gui/narrate_tts.go's `emoBlends`).
pub const BLENDS: [Blend; 21] = [
    Blend { names: &["excited", "thrilled", "exhilarated", "eager", "enthusiastic"], weights: mix(&[(0, 1.0), (6, 0.55)]) },
    Blend { names: &["ecstatic", "overjoyed", "elated", "jubilant"], weights: mix(&[(0, 1.0), (6, 0.8)]) },
    Blend { names: &["playful", "amused", "teasing", "cheeky"], weights: mix(&[(0, 1.0), (6, 0.3), (7, 0.35)]) },
    Blend { names: &["proud", "triumphant", "victorious"], weights: mix(&[(0, 1.0), (7, 0.4)]) },
    Blend { names: &["relieved", "reassured"], weights: mix(&[(0, 0.6), (7, 1.0)]) },
    Blend { names: &["hopeful", "encouraging", "optimistic"], weights: mix(&[(0, 0.7), (7, 1.0)]) },
    Blend { names: &["tender", "warm", "affectionate", "gentle", "fond"], weights: mix(&[(0, 0.5), (7, 1.0)]) },
    Blend { names: &["nostalgic", "bittersweet", "reminiscent"], weights: mix(&[(0, 0.4), (5, 1.0)]) },
    Blend { names: &["solemn", "grave", "reverent", "serious"], weights: mix(&[(1, 0.3), (5, 0.45), (7, 1.0)]) },
    Blend { names: &["awed", "awestruck", "breathless", "wonder"], weights: mix(&[(0, 0.5), (3, 0.25), (6, 1.0)]) },
    Blend { names: &["alarmed", "urgent", "frantic"], weights: mix(&[(3, 0.85), (6, 1.0)]) },
    Blend { names: &["horrified", "appalled", "aghast"], weights: mix(&[(3, 1.0), (4, 0.7), (6, 0.5)]) },
    Blend { names: &["desperate", "pleading", "begging"], weights: mix(&[(2, 0.7), (3, 1.0)]) },
    Blend { names: &["confused", "puzzled", "baffled", "bewildered", "perplexed"], weights: mix(&[(3, 0.3), (6, 1.0), (7, 0.4)]) },
    Blend { names: &["frustrated", "exasperated"], weights: mix(&[(1, 1.0), (4, 0.35), (5, 0.5)]) },
    Blend { names: &["bitter", "resentful", "sullen"], weights: mix(&[(1, 0.7), (4, 0.4), (5, 1.0)]) },
    Blend { names: &["contemptuous", "scornful", "sneering", "dismissive"], weights: mix(&[(1, 0.6), (4, 1.0)]) },
    Blend { names: &["dismayed", "crestfallen", "disappointed"], weights: mix(&[(2, 1.0), (6, 0.5)]) },
    Blend { names: &["heartbroken", "devastated", "grief", "anguished"], weights: mix(&[(2, 1.0), (5, 0.8)]) },
    Blend { names: &["ominous", "foreboding", "menacing"], weights: mix(&[(1, 0.3), (3, 0.5), (7, 1.0)]) },
    Blend { names: &["tense", "suspenseful", "wary"], weights: mix(&[(3, 0.7), (6, 0.35), (7, 1.0)]) },
];

/// A recipe with these axes lit and every other one silent. `const fn` so [`BLENDS`] is data, not work done
/// at startup: the table is the spec's, and a number typed wrong here is a number a test can see.
const fn mix(lit: &[(usize, f64)]) -> [f64; 8] {
    let mut v = [0.0; 8];
    let mut i = 0;
    while i < lit.len() {
        v[lit[i].0] = lit[i].1;
        i += 1;
    }
    v
}

/// S6: the recipe behind one name, if it names a blend.
pub fn blend(name: &str) -> Option<&'static [f64; 8]> {
    let lower = name.to_lowercase();
    BLENDS
        .iter()
        .find(|blend| blend.names.contains(&lower.as_str()))
        .map(|blend| &blend.weights)
}

/// S6: which of the eight axes a base or kin word names, `None` for anything else. The vocabulary is
/// [`EMOTION_KIN`]'s — one table for what the writer may ask for and what this module can deliver as floats.
pub fn base(name: &str) -> Option<usize> {
    let lower = name.trim().to_lowercase();
    EMOTION_KIN
        .iter()
        .position(|(base, kin)| *base == lower.as_str() || kin.iter().any(|word| *word == lower))
}

/// One part of a tag: its name and the weight written after `=`, or 1.0 when none was.
fn split_weight(part: &str) -> Option<(String, f64)> {
    let (name, weight) = match part.split_once('=') {
        Some((name, weight)) => (name.trim(), weight.trim().parse::<f64>().ok()?),
        None => (part, 1.0_f64),
    };
    // Weights are 0..1; a tag that shouts is not louder than the loudest ask, and one that inverts is not
    // the opposite emotion. Clamped rather than refused, because the clamp changes the reading while an
    // unknown name changes the whole route.
    Some((name.to_lowercase(), weight.clamp(0.0, 1.0)))
}

/// S4's `emotion_vector` and S6's vocabulary together: the eight floats a weighted tag asks for, or `None`
/// when the tag is not one this client can resolve.
///
/// A vector is exact and repeatable, so it is what a *weighted* tag is worth: a bare `"[angry]"` goes through
/// the judge like any other word. Weights are the opt-in. Every reason to fall back is a `None`, never an
/// error — S6 says unknown names go to the judge as words, and a name this client does not know may still be
/// a word that server's judge scores. An empty tag, a weight that is not a number, an unknown name and a tag
/// that asks for nothing (`"angry=0"`, which leaves the engine to read the line itself) all answer `None`.
pub fn emotion_vector(tag: &str) -> Option<[f64; 8]> {
    let tag = tag.trim();
    if tag.is_empty() {
        return None;
    }
    let mut weighted = false;
    let mut parts = Vec::new();
    for part in tag.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let (name, weight) = split_weight(part)?;
        weighted |= part.contains('=');
        parts.push((name, weight));
    }
    // Only weights reach a vector: without one there is nothing exact to be repeatable about.
    if !weighted || parts.is_empty() {
        return None;
    }
    let mut v = [0.0_f64; 8];
    let mut sum = 0.0;
    for (name, weight) in parts {
        if let Some(axis) = base(&name) {
            // Named twice: the louder ask wins.
            v[axis] = v[axis].max(weight);
            sum += weight;
            continue;
        }
        let recipe = blend(&name)?;
        for (axis, share) in recipe.iter().enumerate() {
            // A blend spends its weight across several axes at once, scaled so "=0.5" is the same recipe read
            // half as hard.
            v[axis] = v[axis].max(share * weight);
            sum += share * weight;
        }
    }
    (sum > 0.0).then_some(v)
}

/// S4's `emotion_text`: the same tag with the weights taken off, so a tag that names an axis this client does
/// not know (`"wistful=1, smug=0.3"`) still arrives as words the judge can score. An unweighted tag is those
/// words already and passes through untouched.
pub fn emotion_text(tag: &str) -> String {
    let parts: Vec<String> = tag
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(|part| part.split('=').next().unwrap_or(part).trim().to_string())
        .collect();
    if parts.len() <= 1 && !tag.contains(',') {
        return tag.trim().to_string();
    }
    parts.join(", ")
}

/// S4's `options` block. The emotion belongs *here*: the speech endpoint drops unknown top-level fields, and
/// a top-level `"emotion"` was silently ignored for a long time before anyone noticed a line had no feeling
/// in it.
///
/// Exactly one branch is ever sent, because the engine tests `use_emotion_text` first: a known weighted tag
/// gets `emotion_vector` with [`VECTOR_ALPHA`], anything else gets `use_emotion_text` + `emotion_text` riding
/// on [`EMOTION_ALPHA`], and a line with no emotion at all gets neither — just the alpha and the seed that
/// makes the take re-speakable.
pub fn options(tag: &str, seed: u32) -> Value {
    let mut out = serde_json::Map::new();
    out.insert("emotion_alpha".into(), json!(EMOTION_ALPHA));
    out.insert("seed".into(), json!(seed.to_string()));
    if tag.trim().is_empty() {
        return Value::Object(out);
    }
    match emotion_vector(tag) {
        Some(vector) => {
            out.insert("emotion_vector".into(), json!(vector_string(&vector)));
            out.insert("emotion_alpha".into(), json!(VECTOR_ALPHA));
        }
        None => {
            out.insert("use_emotion_text".into(), json!("true"));
            out.insert("emotion_text".into(), json!(emotion_text(tag)));
        }
    }
    Value::Object(out)
}

/// The eight floats as the engine reads them: comma-joined, three decimals at most, shortest form. A blend
/// scaled by a weight lands on numbers like 0.16499999999999998, and this string is both what the server
/// parses and part of how the take would have been named — neither wants the tail. Same rule as
/// [`narration`]'s own key formatting, which is why a vector and a key never disagree.
fn vector_string(vector: &[f64; 8]) -> String {
    vector
        .iter()
        .map(|value| {
            let rounded = (value * 1000.0).round() / 1000.0;
            if rounded == 0.0 {
                // -0.0 would print as "-0", and a zero axis has no sign.
                return "0".to_string();
            }
            format!("{rounded}")
        })
        .collect::<Vec<_>>()
        .join(",")
}

/// S4's `language` (P.policy.ttsLanguage): the project's language, the same one ASR and alignment send. A
/// project with none says nothing about how its words are pronounced, and `"en"` is then the honest guess —
/// the prototype's whole answer, kept as a fallback rather than as a rule.
pub fn language(project: &str) -> String {
    let trimmed = project.trim();
    if trimmed.is_empty() {
        return LANGUAGE_FALLBACK.to_string();
    }
    trimmed.to_string()
}

/// S1: is there a reference to clone from? The file is F4.6's to build — the pitch-shifted
/// `narrate/voice_ref.wav` the model actually speaks from — so this asks only whether it exists, and says
/// where to go when it does not.
pub fn reference_problem(tree: &Tree) -> Option<String> {
    if tree.voice_ref_wav().exists() {
        return None;
    }
    Some(
        "narrate/voice_ref.wav is not there yet -- choose the voice and build the reference (F4.6) before \
         asking for a line to be spoken"
            .to_string(),
    )
}

/// S2: can this server narrate at all? Two questions in the order they have to be asked — is anything
/// answering `/health`, and does what it serve include a model that can clone a voice. The second is not
/// "is there a model on that port": a server serving only step-1 models answers every request and still
/// cannot narrate, which is the same test the Settings row runs ([`checks::tts_endpoint_verdict`]), so the
/// two cannot disagree about whether a box will speak.
pub fn server_problem(healthy: bool, models: &[AudioModel]) -> Option<String> {
    if !healthy {
        return Some(format!(
            "the audio.cpp server did not answer /health within {} s -- nothing can be spoken until it does",
            checks::HEALTH_SECONDS
        ));
    }
    checks::tts_endpoint_verdict(models).err()
}

/// S5: is this reply a wav? A 200 of at least [`MIN_WAV_BYTES`] bytes is; everything else is named with the
/// status and the server's own words, because "the model is not served" and "there was no room" want
/// opposite fixes and both arrive as a short body.
pub fn reply_problem(status: u16, body: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(body);
    let some = text.chars().take(PROBLEM_CHARS).collect::<String>();
    if status != 200 {
        return Some(format!("tts ({status}): {some}"));
    }
    if body.len() < MIN_WAV_BYTES {
        return Some(format!(
            "tts answered 200 with {} bytes, which is not a wav: {some}",
            body.len()
        ));
    }
    None
}

/// S5: where this take lives — `narrate/tts/‹hash›.wav`, the hash being the first sixteen digits of the
/// take's key ([`crate::narration::tts_file`]). One filename per (text, voice, emotion, roll), which is why
/// re-speaking a line nobody changed costs nothing.
pub fn take_path(tree: &Tree, key: &str) -> PathBuf {
    tree.tts_wav(&narration::tts_file(key))
}

/// S5's write: refuse what [`reply_problem`] refuses — so a server's JSON error can never be filed as a take
/// and replayed forever as silence — then put the bytes where [`take_path`] says they belong.
pub fn write_take(tree: &Tree, key: &str, data: &[u8]) -> Result<PathBuf, String> {
    if let Some(problem) = reply_problem(200, data) {
        return Err(problem);
    }
    let file = take_path(tree, key);
    let folder = file.parent().expect("a take path has a folder");
    std::fs::create_dir_all(folder).map_err(|err| format!("{}: {err}", folder.display()))?;
    std::fs::write(&file, data).map_err(|err| format!("{}: {err}", file.display()))?;
    Ok(file)
}

/// The request this line is spoken with: [`crate::bodies::speech_request`]'s five keys and this module's
/// `options`. Split out so a caller can log or cache-key what it is about to send, and so the wire shape is
/// one function's answer rather than two.
///
/// `voice_ref` is the path the upload returned *for this line* — see
/// [`crate::bodies::VOICE_REF_REUPLOADED_EVERY_LINE`].
pub fn request(
    model: &str,
    text: &str,
    voice_ref: &ServerPath,
    language: &str,
    emotion: &str,
    seed: u32,
) -> Value {
    crate::bodies::speech_request(model, text, voice_ref, language, options(emotion, seed))
}
