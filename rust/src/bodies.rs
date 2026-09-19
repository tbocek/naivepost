//! The request bodies §5 confirms, and the sd.cpp waits.
//!
//! Two things are worth pinning down here rather than leaving to whoever writes the next call:
//! which keys a body has at all (an extra one is a server's problem to ignore, a missing one is a
//! different job), and whether a path in it is the server's or the local machine's.

use std::time::Duration;

use serde_json::{json, Map, Value};

use crate::services::Kind;

/// A path on the audio.cpp server, returned by `POST /v1/ui/upload`.
///
/// The type exists because "the `audio` field is a path" hides the one mistake that matters: this
/// machine and the server are different filesystems (audio.cpp runs in a container), so a local
/// path is not a wrong answer but no answer at all — the server looks for it in its own root and
/// reports a missing file minutes later. Uploading first is the only way to get one, which is why
/// [`ServerPath::from_upload`] is the only constructor: a body builder cannot be handed
/// `std::path::Path::new("take.wav")` even by accident.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerPath(String);

impl ServerPath {
    /// The path an upload answered with. Only an absolute one is accepted: a relative answer means
    /// the server replied with something other than a path, and passing it through would turn a
    /// protocol disagreement into a "file not found" from a task run.
    pub fn from_upload(reply: &str) -> Option<Self> {
        let path = reply.trim();
        if path.is_empty() || !path.starts_with('/') {
            return None;
        }
        Some(Self(path.to_string()))
    }

    /// The value the request carries.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The voice reference is uploaded again before every line, not once per run (§5).
///
/// The path a server hands out lives as long as its temp folder: restart audio.cpp — which a model
/// swap or a container update does — and a remembered path names nothing. Re-uploading one small
/// wav per line costs milliseconds against the seconds the synthesis takes, and it is what makes
/// the first line after a restart work like the rest. The prototype's `serverFile`
/// (gui/audiocpp.go:63-93) does exactly this before every request, sending the file's base name as
/// `X-Audiocpp-Filename`.
pub const VOICE_REF_REUPLOADED_EVERY_LINE: bool = true;

/// `POST /v1/tasks/run` for speech-to-text: `{"audio","language"}`.
///
/// The envelope is the server's (`{"model","request"}`, [Kind::RunTask]); these builders return the
/// whole body so a test sees what goes on the wire, and they build with [`json!`] so key order
/// follows the order §5 lists the fields in.
pub fn asr_body(model: &str, audio: &ServerPath, language: &str) -> Value {
    json!({
        "model": model,
        "request": { "audio": audio.as_str(), "language": language },
    })
}

/// `POST /v1/tasks/run` for diarization: `{"audio"}` and nothing else — a speaker label has no
/// language to be told, and sending one invites a server to read meaning into it.
pub fn diar_body(model: &str, audio: &ServerPath) -> Value {
    json!({ "model": model, "request": { "audio": audio.as_str() } })
}

/// `POST /v1/tasks/run` for forced alignment: `{"audio","text","language"}`. The text is what the
/// words are placed against, so it travels with the audio rather than being checked beforehand.
pub fn align_body(model: &str, audio: &ServerPath, text: &str, language: &str) -> Value {
    json!({
        "model": model,
        "request": { "audio": audio.as_str(), "text": text, "language": language },
    })
}

/// `POST /v1/audio/speech`: `{"model","input","voice_ref","language","options"}`.
///
/// `voice_ref` is the uploaded path of `narrate/voice_ref.wav` — the pitch-shifted reference, which
/// is the file the model actually clones from ([`crate::layout::Tree::voice_ref_wav`]) — and it
/// must be re-uploaded per line, see [`VOICE_REF_REUPLOADED_EVERY_LINE`]. `options` is the emotion
/// vector plus the `seed` that makes a take reproducible.
///
/// `language` is P.policy.ttsLanguage: the project's language, the same one ASR and alignment send.
/// The prototype hard-codes `"en"` here (gui/narrate_tts.go:325-329), which is the thing this
/// replaces — a model told to speak English reads Polish words as English spelling.
pub fn speech_body(
    model: &str,
    text: &str,
    voice_ref: &ServerPath,
    language: &str,
    emotion: &[f64],
    seed: i64,
) -> Value {
    json!({
        "model": model,
        "input": text,
        "voice_ref": voice_ref.as_str(),
        "language": language,
        "options": { "emotion": emotion, "seed": seed },
    })
}

/// `POST /sdcpp/v1/img_gen`: the prompt, the optional framing, always a seed, and the references.
///
/// The four optional keys are omitted rather than sent empty — sd.cpp reads an empty string as an
/// instruction ("draw with no negative prompt" is not what `""` means) — while `seed` and
/// `auto_resize_ref_image` are always present: a thumbnail must be redrawn identically, and a
/// reference resized one way on one server and another way on the next changes the picture.
///
/// References go as `ref_images` data URLs, never `init_image`: an edit model is *conditioned* on
/// them, which is what lets it keep a face and change the background, whereas `init_image` means
/// "redraw this" at the strength of a denoise parameter. Everything else — steps, sampler, cfg —
/// stays with the server's start-up flags deliberately: the app asks for a picture, the machine it
/// runs on decides how.
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
/// This table is *not* [`crate::services::timeout_for`], and the two are kept apart on purpose:
/// §1 says audio task calls carry no client timeout (an hour of audio is an hour of work), while
/// §5 gives the drawing server four short waits, because a thumbnail job is never long — it is
/// either queued, drawing, or gone. `timeout_for` answers "may this kind run for an hour", this
/// answers "how long until sd.cpp has said something".
pub fn sd_waits(kind: Kind) -> Option<SdWait> {
    let wait = |seconds: u64, interval: Option<Duration>| Some(SdWait {
        timeout: Duration::from_secs(seconds),
        interval,
    });
    match kind {
        // Fifteen seconds is the slowest a healthy server takes to describe itself; longer means
        // it is starting up or not sd.cpp at all.
        Kind::Capabilities => wait(15, None),
        // Submitting only queues the work, so a minute is generous — and a submit that takes a
        // minute is the model loading, which every later request would pay for again.
        Kind::SubmitImage => wait(60, None),
        // One ask per second: the answer is a status line, so polling faster buys nothing and a
        // longer gap would make a finished thumbnail appear late.
        Kind::PollImage => wait(30, Some(Duration::from_secs(1))),
        // Ten seconds, best effort: cancel hands the card back either way, so waiting longer for a
        // server that is clearly busy would only hold up the UI.
        Kind::CancelImage => wait(10, None),
        _ => None,
    }
}
