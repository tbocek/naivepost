//! The four servers, spec/02-services.md §1.

use std::path::Path;
use std::time::Duration;

use serde::Deserialize;

use crate::settings::{self, Conf, Paths};

pub const LLM_PORT: u16 = 8731;

/// audio.cpp: speech to text, alignment, diarization, separation, text to speech.
pub const AUDIO_PORT: u16 = 8765;

/// sd.cpp (sd-server), which draws the thumbnail when one is drawn.
pub const SD_PORT: u16 = 1234;

/// The four audio model ids, compiled in: they are what an empty box means, and a server that
/// has not been configured still answers to them.
pub const ASR_MODEL: &str = "nemotron-asr";
pub const DIAR_MODEL: &str = "sortformer-diar";
pub const TTS_MODEL: &str = "index-tts2";
pub const SEP_MODEL: &str = "bs-roformer";

/// The aligner this stack is built around. A *preference*, not a demand: the server's own list
/// of models declared for `align` decides, and an empty box means no aligner is required at all
/// — see [`pick_aligner`] and the note on [`Conf::align_model`].
pub const ALIGN_PREFERENCE: &str = "qwen3-aligner";

/// The packages the shipped defaults install from, named in the hint §1 asks for. Only these
/// three exist: a hand-picked id's weights would be a guess.
pub const ASR_PACKAGE: &str = "nemotron_asr_q8_0";
pub const DIAR_PACKAGE: &str = "sortformer_diar_4spk_v1_q8_0";
pub const SEP_PACKAGE: &str = "bs_roformer_q8_0";

/// Housekeeping gets a timeout of its own (§1): unloading the models is not work, and twenty
/// seconds says as much.
pub const UNLOAD_TIMEOUT: Duration = Duration::from_secs(20);

/// `unload_all_models` is called quietly at the end of every run, even one that used no audio,
/// and its failure never fails a run (§1).
pub const UNLOAD_IS_BEST_EFFORT: bool = true;

/// The loopback default for each server. `http`, not `https`: this is the machine talking to
/// itself, which is why [server_url] hands `https` to a bare host and these do not — a server
/// named by host is out on the network.
pub fn llm_default() -> String {
    format!("http://127.0.0.1:{LLM_PORT}")
}

pub fn audio_default() -> String {
    format!("http://127.0.0.1:{AUDIO_PORT}")
}

pub fn image_default() -> String {
    format!("http://127.0.0.1:{SD_PORT}")
}

/// One server: its URL and the key that goes with it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoint {
    pub url: String,
    /// Already trimmed; empty means the server wants no key.
    pub key: String,
}

/// What one settings box says about a server, or `None` when it says nothing and the loopback
/// default applies (§1: "An empty server box means the loopback default").
pub fn server_url(raw: &str) -> Option<String> {
    let url = raw.trim().trim_end_matches('/');
    if url.is_empty() {
        return None;
    }
    Some(if url.contains("://") {
        url.to_string()
    } else {
        format!("https://{url}")
    })
}

/// The header a key becomes, or `None` when there is no key to send (§1: keys go as
/// `Authorization: Bearer …` only when non-empty — an empty one would be a credential that says
/// "none", which some servers read as a bad request).
pub fn authorization(key: &str) -> Option<String> {
    let key = key.trim();
    (!key.is_empty()).then(|| format!("Bearer {key}"))
}

/// Which of the four servers a request is aimed at. ffmpeg and ffprobe are the fourth row of
/// §1's table and are not a server: [`ffprobe_for`] is their half.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Server {
    Llm,
    Audio,
    Image,
}

/// The writing model's server. No environment override — §1 says so outright ("the LLM server
/// has no env override"), and it is the one endpoint a user can see and clear on the dialog, so
/// a stale export in the launching shell must not silently outrank what the box says.
pub fn llm_endpoint(conf: &Conf) -> Endpoint {
    Endpoint {
        url: server_url(&conf.server).unwrap_or_else(llm_default),
        key: conf.key.clone(),
    }
}

/// A server that refuses uploads at all (§03-shell#8-details-confirmed-against-the-code-verification-pass).
pub fn upload_refused(status: u16, body: &str) -> Option<String> {
    if !matches!(status, 401 | 403 | 405) {
        return None;
    }
    let mut fix =
        "the audio.cpp server refuses uploads -- start it with --ui-management".to_string();
    if let Some(folder) = named_path(body) {
        fix.push_str(&format!(
            ", and mount {folder} into the container so it can read what is uploaded"
        ));
    }
    Some(fix)
}

/// The first absolute path in a server's message. Paths are how audio.cpp explains a
/// folder it was not given, and quoting one back is the only part of its sentence that
/// survives translation into advice.
fn named_path(body: &str) -> Option<&str> {
    body.split_whitespace()
        .map(|word| {
            word.trim_matches(|c: char| {
                !c.is_ascii_alphanumeric() && !matches!(c, '/' | '.' | '_' | '-')
            })
        })
        .find(|word| word.starts_with('/') && word.len() > 1)
}

pub fn upload_path(reply: &serde_json::Value) -> Result<String, String> {
    match reply.get("path").and_then(|value| value.as_str()) {
        Some(path) if !path.is_empty() => Ok(path.to_string()),
        _ => Err(format!(
            "the audio.cpp server accepted the upload without naming where it put it: {reply}"
        )),
    }
}

pub fn audio_endpoint(conf: &Conf, env: impl Fn(&str) -> Option<String>) -> Endpoint {
    let url = server_url(&conf.audio_server)
        .or_else(|| server_url(&env("NAIVEPOST_TTS_URL").unwrap_or_default()))
        .or_else(|| server_url(&env("AUDIOCPP_SERVER").unwrap_or_default()))
        .unwrap_or_else(audio_default);
    Endpoint {
        url,
        key: conf.audio_key.clone(),
    }
}

/// sd.cpp's server: the settings box, then `SD_SERVER`, then the loopback.
pub fn image_endpoint(conf: &Conf, env: impl Fn(&str) -> Option<String>) -> Endpoint {
    let url = server_url(&conf.sd_server)
        .or_else(|| server_url(&env("SD_SERVER").unwrap_or_default()))
        .unwrap_or_else(image_default);
    Endpoint {
        url,
        key: conf.sd_key.clone(),
    }
}

pub fn endpoint_for(
    paths: &Paths,
    which: Server,
    env: impl Fn(&str) -> Option<String>,
) -> Endpoint {
    let conf = settings::read(paths).unwrap_or_default();
    match which {
        Server::Llm => llm_endpoint(&conf),
        Server::Audio => audio_endpoint(&conf, &env),
        Server::Image => image_endpoint(&conf, env),
    }
}

/// One kind of request in §1's API column. The path and the timeout follow from the kind, which
/// is what lets a test pin the table and a caller stay honest about what it is sending.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// `POST /v1/chat/completions` — every text and vision job, streaming, with tools.
    Chat,
    /// `GET /v1/models` — the LLM's model list, and audio.cpp's catalog.
    ListModels,
    /// `GET /health`.
    Health,
    /// `POST /v1/ui/upload` — raw body with `X-Audiocpp-Filename`.
    Upload,
    /// `POST /v1/tasks/run` with `{"model","request"}`.
    RunTask,
    /// `POST /v1/audio/speech`.
    Speech,
    /// `POST /v1/tasks/unload_all_models`.
    UnloadAll,
    /// `GET /sdcpp/v1/capabilities`.
    Capabilities,
    /// `POST /sdcpp/v1/img_gen` → a job id.
    SubmitImage,
    /// `GET /sdcpp/v1/jobs/{id}`.
    PollImage,
    /// `POST /sdcpp/v1/jobs/{id}/cancel`.
    CancelImage,
}

impl Kind {
    /// The method this kind is sent with.
    pub fn method(self) -> &'static str {
        match self {
            Self::Chat
            | Self::Upload
            | Self::RunTask
            | Self::Speech
            | Self::UnloadAll
            | Self::SubmitImage
            | Self::CancelImage => "POST",
            Self::ListModels | Self::Health | Self::Capabilities | Self::PollImage => "GET",
        }
    }

    /// The path this kind is sent to. `job` is the sd.cpp job id, which the two job kinds need
    /// and every other kind ignores.
    pub fn path(self, job: Option<&str>) -> String {
        match self {
            Self::Chat => "/v1/chat/completions".into(),
            Self::ListModels => "/v1/models".into(),
            Self::Health => "/health".into(),
            Self::Upload => "/v1/ui/upload".into(),
            Self::RunTask => "/v1/tasks/run".into(),
            Self::Speech => "/v1/audio/speech".into(),
            Self::UnloadAll => "/v1/tasks/unload_all_models".into(),
            Self::Capabilities => "/sdcpp/v1/capabilities".into(),
            Self::SubmitImage => "/sdcpp/v1/img_gen".into(),
            // A job kind with no id is a caller bug, and an empty id would ask for `/jobs/` —
            // which answers 404 and reads like the job having gone away. Better to say so.
            Self::PollImage => format!("/sdcpp/v1/jobs/{}", job.unwrap_or("<no job id>")),
            Self::CancelImage => {
                format!("/sdcpp/v1/jobs/{}/cancel", job.unwrap_or("<no job id>"))
            }
        }
    }
}

/// How long this kind may take, `None` meaning no client timeout at all.
pub fn timeout_for(kind: Kind) -> Option<Duration> {
    match kind {
        Kind::UnloadAll => Some(UNLOAD_TIMEOUT),
        _ => None,
    }
}

/// Whether this kind is sent on the run's cancel context, so ⏹ aborts it.
pub fn rides_cancel_context(kind: Kind) -> bool {
    // The parameter stays because the answer is a table the spec could change per kind; today
    // every row says yes, and §1 asks that it stays that way.
    let _ = kind;
    true
}

/// The speech-to-text model to ask for: `AUDIOCPP_ASR_MODEL`, or the compiled-in id when the box
/// is empty.
pub fn asr_model(conf: &Conf) -> &str {
    pick(&conf.asr_model, ASR_MODEL)
}

/// The diarization model (`AUDIOCPP_DIAR_MODEL`).
pub fn diar_model(conf: &Conf) -> &str {
    pick(&conf.diar_model, DIAR_MODEL)
}

/// The voice-cloning model (`AUDIOCPP_TTS_MODEL`).
pub fn tts_model(conf: &Conf) -> &str {
    pick(&conf.tts_model, TTS_MODEL)
}

/// The separation model (`AUDIOCPP_SEP_MODEL`).
pub fn sep_model(conf: &Conf) -> &str {
    pick(&conf.sep_model, SEP_MODEL)
}

fn pick<'a>(value: &'a str, default: &'a str) -> &'a str {
    if value.trim().is_empty() {
        default
    } else {
        value
    }
}

/// One model a server declares. `task` is what the server says it does — and what §1's aligner
/// rule keys off, which is why it is kept rather than thrown away after parsing.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct AudioModel {
    pub id: String,
    #[serde(default)]
    pub family: String,
    #[serde(default)]
    pub task: String,
}

/// The catalog `GET /v1/models` answers with. A body that does not parse is an empty list: a
/// server speaking some other dialect is reported by the missing model's own message, which
/// names what the server does serve — nothing here should fail before then.
pub fn parse_models(body: &str) -> Vec<AudioModel> {
    #[derive(Deserialize)]
    struct Catalog {
        #[serde(default)]
        data: Vec<AudioModel>,
    }
    serde_json::from_str::<Catalog>(body)
        .map(|c| c.data)
        .unwrap_or_default()
}

/// The aligner to try, by task rather than by id (§1).
pub fn pick_aligner(configured: &str, models: &[AudioModel]) -> Option<String> {
    let declared = |id: &str| models.iter().any(|m| m.task == "align" && m.id == id);
    let wanted = configured.trim();
    if !wanted.is_empty() {
        // A hand-picked id the server does not serve stays a miss: guessing another one would
        // align through a model the user never chose.
        return declared(wanted).then(|| wanted.to_string());
    }
    let mut ids: Vec<&str> = models
        .iter()
        .filter(|m| m.task == "align")
        .map(|m| m.id.as_str())
        .collect();
    ids.sort_by_key(|id| (*id != ALIGN_PREFERENCE, *id));
    ids.first().map(|id| id.to_string())
}

/// The package a shipped default's weights install from, `None` for any other id — the hint is
/// only honest when the id is the one this build ships with.
pub fn install_package(id: &str) -> Option<&'static str> {
    match id {
        ASR_MODEL => Some(ASR_PACKAGE),
        DIAR_MODEL => Some(DIAR_PACKAGE),
        SEP_MODEL => Some(SEP_PACKAGE),
        _ => None,
    }
}

/// What to say when a server does not have the model a step needs.
#[derive(Debug, Clone, Copy)]
pub struct Need<'a> {
    pub task: &'a str,
    pub step: &'a str,
}

pub fn missing_model(
    url: &str,
    models: &[AudioModel],
    wanted_id: &str,
    needed: Need<'_>,
) -> String {
    let Need {
        task: wanted_task,
        step,
    } = needed;
    if let Some(model) = models.iter().find(|m| m.id == wanted_id) {
        // Present, declared for something else — the usual cause is a catalog entry copied from
        // another model with its task left as it was.
        return format!(
            "{wanted_id:?} on {url} is declared task {:?}, but {step} needs {wanted_task:?} there",
            model.task
        );
    }
    let mut ids: Vec<&str> = models.iter().map(|m| m.id.as_str()).collect();
    ids.sort();
    let how = match install_package(wanted_id) {
        Some(package) => format!(
            " (the weights install with: docker compose exec audio python3 tools/model_manager_v2.py install {package} --models-root models)"
        ),
        None => String::new(),
    };
    format!(
        "the audio.cpp server at {url} serves {}, but not {wanted_id:?} -- add it on that \
         server's own model page in the browser, or in the config-audiocpp.json it reads at \
         startup followed by docker compose up -d --force-recreate audio{how}",
        ids.join(", ")
    )
}

pub fn ffprobe_for(configured_ffmpeg: &str, on_path: &str) -> String {
    let ffmpeg = configured_ffmpeg.trim();
    if ffmpeg.is_empty() {
        return on_path.to_string();
    }
    match Path::new(ffmpeg).file_name().and_then(|n| n.to_str()) {
        Some(name) if name.starts_with("ffmpeg") => {
            let dir = Path::new(ffmpeg).parent().unwrap_or(Path::new(""));
            dir.join(name.replacen("ffmpeg", "ffprobe", 1))
                .display()
                .to_string()
        }
        // A configured path that is not an ffmpeg binary gets its own directory and the plain
        // name: better to look for ffprobe beside what the user pointed at than to fail.
        _ => Path::new(ffmpeg)
            .parent()
            .unwrap_or(Path::new(""))
            .join("ffprobe")
            .display()
            .to_string(),
    }
}

/// Which audio job a request is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioTask {
    Asr,
    Diar,
    Tts,
    Sep,
    Align,
}
