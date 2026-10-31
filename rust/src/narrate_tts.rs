//! F4.4 Speak a line (TTS) — `spec/07-narrate.md` F4.4, steps S1–S6.

use std::path::PathBuf;

use serde_json::{json, Value};

use crate::bodies::ServerPath;
use crate::checks;
use crate::layout::Tree;
use crate::narrate_pass::EMOTION_KIN;
use crate::narration;
use crate::services::AudioModel;

pub const EMOTION_ALPHA: &str = "0.85";

pub const VECTOR_ALPHA: &str = "1";

pub const MIN_WAV_BYTES: usize = 1000;

pub const LANGUAGE_FALLBACK: &str = "en";

pub const LANGUAGE_SOURCE: &str = "narrate_tts::LANGUAGE_FALLBACK";

const PROBLEM_CHARS: usize = 300;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Blend {
    pub names: &'static [&'static str],
    pub weights: [f64; 8],
}

pub const BLENDS: [Blend; 21] = [
    Blend {
        names: &[
            "excited",
            "thrilled",
            "exhilarated",
            "eager",
            "enthusiastic",
        ],
        weights: mix(&[(0, 1.0), (6, 0.55)]),
    },
    Blend {
        names: &["ecstatic", "overjoyed", "elated", "jubilant"],
        weights: mix(&[(0, 1.0), (6, 0.8)]),
    },
    Blend {
        names: &["playful", "amused", "teasing", "cheeky"],
        weights: mix(&[(0, 1.0), (6, 0.3), (7, 0.35)]),
    },
    Blend {
        names: &["proud", "triumphant", "victorious"],
        weights: mix(&[(0, 1.0), (7, 0.4)]),
    },
    Blend {
        names: &["relieved", "reassured"],
        weights: mix(&[(0, 0.6), (7, 1.0)]),
    },
    Blend {
        names: &["hopeful", "encouraging", "optimistic"],
        weights: mix(&[(0, 0.7), (7, 1.0)]),
    },
    Blend {
        names: &["tender", "warm", "affectionate", "gentle", "fond"],
        weights: mix(&[(0, 0.5), (7, 1.0)]),
    },
    Blend {
        names: &["nostalgic", "bittersweet", "reminiscent"],
        weights: mix(&[(0, 0.4), (5, 1.0)]),
    },
    Blend {
        names: &["solemn", "grave", "reverent", "serious"],
        weights: mix(&[(1, 0.3), (5, 0.45), (7, 1.0)]),
    },
    Blend {
        names: &["awed", "awestruck", "breathless", "wonder"],
        weights: mix(&[(0, 0.5), (3, 0.25), (6, 1.0)]),
    },
    Blend {
        names: &["alarmed", "urgent", "frantic"],
        weights: mix(&[(3, 0.85), (6, 1.0)]),
    },
    Blend {
        names: &["horrified", "appalled", "aghast"],
        weights: mix(&[(3, 1.0), (4, 0.7), (6, 0.5)]),
    },
    Blend {
        names: &["desperate", "pleading", "begging"],
        weights: mix(&[(2, 0.7), (3, 1.0)]),
    },
    Blend {
        names: &["confused", "puzzled", "baffled", "bewildered", "perplexed"],
        weights: mix(&[(3, 0.3), (6, 1.0), (7, 0.4)]),
    },
    Blend {
        names: &["frustrated", "exasperated"],
        weights: mix(&[(1, 1.0), (4, 0.35), (5, 0.5)]),
    },
    Blend {
        names: &["bitter", "resentful", "sullen"],
        weights: mix(&[(1, 0.7), (4, 0.4), (5, 1.0)]),
    },
    Blend {
        names: &["contemptuous", "scornful", "sneering", "dismissive"],
        weights: mix(&[(1, 0.6), (4, 1.0)]),
    },
    Blend {
        names: &["dismayed", "crestfallen", "disappointed"],
        weights: mix(&[(2, 1.0), (6, 0.5)]),
    },
    Blend {
        names: &["heartbroken", "devastated", "grief", "anguished"],
        weights: mix(&[(2, 1.0), (5, 0.8)]),
    },
    Blend {
        names: &["ominous", "foreboding", "menacing"],
        weights: mix(&[(1, 0.3), (3, 0.5), (7, 1.0)]),
    },
    Blend {
        names: &["tense", "suspenseful", "wary"],
        weights: mix(&[(3, 0.7), (6, 0.35), (7, 1.0)]),
    },
];

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

pub fn weights_of(tag: &str) -> Option<String> {
    emotion_vector(tag).map(|vector| vector_string(&vector))
}

pub fn blend_recites(blend: &Blend) -> String {
    let recipe: Vec<String> = blend
        .weights
        .iter()
        .enumerate()
        .filter(|(_, share)| **share > 0.0)
        .map(|(axis, share)| {
            let base = crate::narrate_pass::EMOTION_KIN[axis].0;
            let rounded = (share * 100.0).round() / 100.0;
            format!("{base} {rounded}")
        })
        .collect();
    format!("{} = {}", blend.names.join(" / "), recipe.join(" + "))
}

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

pub fn language(project: &str) -> String {
    let trimmed = project.trim();
    if trimmed.is_empty() {
        return LANGUAGE_FALLBACK.to_string();
    }
    trimmed.to_string()
}

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

pub fn server_problem(healthy: bool, models: &[AudioModel]) -> Option<String> {
    if !healthy {
        return Some(format!(
            "the audio.cpp server did not answer /health within {} s -- nothing can be spoken until it does",
            checks::HEALTH_SECONDS
        ));
    }
    checks::tts_endpoint_verdict(models).err()
}

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

pub fn take_path(tree: &Tree, key: &str) -> PathBuf {
    tree.tts_wav(&narration::tts_file(key))
}

/// How long this take's wav actually runs, or `None` when it is not on disk or its header cannot be read.
pub fn take_seconds(tree: &Tree, key: &str) -> Option<f64> {
    crate::voice_ref::wav_seconds(&take_path(tree, key))
}

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Refused(String),
    Take(PathBuf),
}

impl Outcome {
    /// The refusal's sentence, or `None` when the line was spoken.
    pub fn refused(&self) -> Option<&str> {
        match self {
            Self::Refused(why) => Some(why),
            Self::Take(_) => None,
        }
    }

    /// The take's path, or `None` when nothing was written.
    pub fn wav(&self) -> Option<&std::path::Path> {
        match self {
            Self::Refused(_) => None,
            Self::Take(file) => Some(file),
        }
    }
}

#[allow(clippy::too_many_arguments)] // eleven: the line, its key and S2's two answers are all data, not behaviour
pub fn speak_line<FUpload, FPost>(
    tree: &Tree,
    text: &str,
    emotion: &str,
    seed: u32,
    key: &str,
    project_language: &str,
    healthy: bool,
    models: &[AudioModel],
    model: &str,
    upload_reference: FUpload,
    post_speech: FPost,
) -> Outcome
where
    FUpload: FnOnce(&Tree) -> Result<ServerPath, String>,
    FPost: FnOnce(&Value) -> Result<(u16, Vec<u8>), String>,
{
    // S1 · the reference on disk (F4.6 built it; without it there is nothing to clone).
    if let Some(problem) = reference_problem(tree) {
        return Outcome::Refused(problem);
    }
    // S2 · audio.cpp healthy, serving a model able to clone.
    if let Some(problem) = server_problem(healthy, models) {
        return Outcome::Refused(problem);
    }
    // S3 · up it goes again, for this line.
    let voice_ref = match upload_reference(tree) {
        Ok(path) => path,
        Err(problem) => return Outcome::Refused(problem),
    };
    // S4 · one POST, the emotion riding inside `options`.
    let body = request(
        model,
        text,
        &voice_ref,
        &language(project_language),
        emotion,
        seed,
    );
    let (status, bytes) = match post_speech(&body) {
        Ok(reply) => reply,
        Err(problem) => return Outcome::Refused(problem),
    };
    // S5 · 200 and at least MIN_WAV_BYTES, or it is not a take.
    if let Some(problem) = reply_problem(status, &bytes) {
        return Outcome::Refused(problem);
    }
    match write_take(tree, key, &bytes) {
        Ok(file) => Outcome::Take(file),
        Err(problem) => Outcome::Refused(problem),
    }
}

pub fn outcome_said(outcome: &Outcome) -> String {
    match outcome {
        Outcome::Refused(why) => why.clone(),
        Outcome::Take(file) => format!("spoken \u{2014} {}", file.display()),
    }
}
