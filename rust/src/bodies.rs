//! The request bodies §5 confirms, and the sd.cpp waits.
//!
//! Pinned here: which keys a body has at all, and whether a path in it is the server's or the local machine's.

use std::time::Duration;

use serde_json::{json, Map, Value};

use crate::services::Kind;

/// A path on the audio.cpp server, returned by `POST /v1/ui/upload`.
///
/// This machine and the server are different filesystems (audio.cpp runs in a container), so a local path is no answer: the server looks in its own root and reports a missing file minutes later.
/// Uploading is the only way to get one, so [`ServerPath::from_upload`] is the only constructor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerPath(String);

impl ServerPath {
    /// The path an upload answered with. Only an absolute one is accepted: a relative answer is not a path, and would turn a protocol disagreement into "file not found".
    pub fn from_upload(reply: &str) -> Option<Self> {
        let path = reply.trim();
        if path.is_empty() || !path.starts_with('/') {
            return None;
        }
        Some(Self(path.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The voice reference is uploaded again before every line, not once per run (§5).
///
/// The path a server hands out lives as long as its temp folder: a restart (model swap, container update) leaves a remembered path naming nothing. One small wav per line costs
/// milliseconds against seconds of synthesis. The prototype's `serverFile` (go/audiocpp.go:63-93) does this before every request, sending the base name as `X-Audiocpp-Filename`.
pub const VOICE_REF_REUPLOADED_EVERY_LINE: bool = true;

/// `POST /v1/tasks/run` for speech-to-text: `{"audio","language"}`.
///
/// The envelope is the server's (`{"model","request"}`, [Kind::RunTask]); these return the whole body, built with [`json!`] so key order follows §5.
pub fn asr_body(model: &str, audio: &ServerPath, language: &str) -> Value {
    json!({
        "model": model,
        "request": { "audio": audio.as_str(), "language": language },
    })
}

/// `POST /v1/tasks/run` for diarization: `{"audio"}` and nothing else (a speaker label has no language).
pub fn diar_body(model: &str, audio: &ServerPath) -> Value {
    json!({ "model": model, "request": { "audio": audio.as_str() } })
}

/// `POST /v1/tasks/run` for forced alignment: `{"audio","text","language"}`; the text is what the words are placed against.
pub fn align_body(model: &str, audio: &ServerPath, text: &str, language: &str) -> Value {
    json!({
        "model": model,
        "request": { "audio": audio.as_str(), "text": text, "language": language },
    })
}

/// The one envelope `POST /v1/audio/speech` takes: `{"model","input","voice_ref","language","options"}`.
///
/// Split from [`speech_body`] because what belongs in `options` is a flow of its own (F4.4's two emotion branches); the endpoint drops unknown keys, so a top-level extra is lost.
pub fn speech_request(
    model: &str,
    text: &str,
    voice_ref: &ServerPath,
    language: &str,
    options: Value,
) -> Value {
    json!({
        "model": model,
        "input": text,
        "voice_ref": voice_ref.as_str(),
        "language": language,
        "options": options,
    })
}

/// `POST /sdcpp/v1/img_gen`: the prompt, the optional framing, always a seed, and the references.
///
/// The four optional keys are omitted rather than sent empty (sd.cpp reads an empty string as an instruction); `seed` and `auto_resize_ref_image` are always present, so a thumbnail redraws identically.
///
/// References go as `ref_images` data URLs, never `init_image`: an edit model is *conditioned* on them (keeps a face, changes the background), while `init_image` means "redraw this".
/// Steps, sampler and cfg stay with the server's start-up flags: the app asks for a picture, the machine decides how.
#[allow(clippy::too_many_arguments)]
pub fn img_gen_body(
    prompt: &str,
    negative_prompt: Option<&str>,
    width: Option<u32>,
    height: Option<u32>,
    seed: i64,
    ref_image_data_urls: &[String],
    output_format: Option<&str>,
) -> Value {
    let mut body = Map::new();
    body.insert("prompt".into(), json!(prompt));
    if let Some(negative) = negative_prompt {
        body.insert("negative_prompt".into(), json!(negative));
    }
    if let Some(width) = width {
        body.insert("width".into(), json!(width));
    }
    if let Some(height) = height {
        body.insert("height".into(), json!(height));
    }
    // Always: 0 and -1 are seeds like any other, so "no seed" has no representation here.
    body.insert("seed".into(), json!(seed));
    if !ref_image_data_urls.is_empty() {
        body.insert("ref_images".into(), json!(ref_image_data_urls));
    }
    body.insert("auto_resize_ref_image".into(), json!(true));
    if let Some(format) = output_format {
        body.insert("output_format".into(), json!(format));
    }
    Value::Object(body)
}

/// How long one sd.cpp call waits, and how often a job is asked about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SdWait {
    pub timeout: Duration,
    /// Only a poll has one: the gap between two asks.
    pub interval: Option<Duration>,
}

/// §5's own numbers for sd.cpp.
///
/// Not [`crate::services::timeout_for`]: §1 gives audio task calls no client timeout (an hour of audio is an hour of work), while §5 gives the drawing server four short waits (a thumbnail job is queued, drawing, or gone).
pub fn sd_waits(kind: Kind) -> Option<SdWait> {
    let wait = |seconds: u64, interval: Option<Duration>| {
        Some(SdWait {
            timeout: Duration::from_secs(seconds),
            interval,
        })
    };
    match kind {
        // Fifteen seconds is the slowest a healthy server takes to describe itself; longer means starting up or not sd.cpp.
        Kind::Capabilities => wait(15, None),
        // Submitting only queues the work; a minute-long submit is the model loading, which every later request would pay again.
        Kind::SubmitImage => wait(60, None),
        // One ask per second: the answer is a status line, so polling faster buys nothing and slower makes a finished thumbnail appear late.
        Kind::PollImage => wait(30, Some(Duration::from_secs(1))),
        // Best effort: cancel hands the card back either way, so waiting on a busy server only holds up the UI.
        Kind::CancelImage => wait(10, None),
        _ => None,
    }
}
