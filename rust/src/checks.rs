//! What the Settings dialog's ten Test buttons and "Test All" decide (§5). Functions are handed what a request or binary answered and return the verdict, so pass/fail is testable without a server (decisions follow go/setup.go:520-815).

use std::time::Duration;

use serde_json::{json, Value};

use crate::services::{self, AudioModel};

/// The four audio.cpp rows, each with a Test button (§5): TTS model, ASR, diarization, separation.
pub const AUDIO_ROWS: [(&str, &str); 4] = [
    ("TTS model", "clon"),
    ("ASR", "asr"),
    ("Diarization", "diar"),
    ("Separation", "sep"),
];

/// The LLM test asks for one completion of sixteen tokens, so a rambling model is cut off (§5).
pub const LLM_TEST_MAX_TOKENS: u32 = 16;
pub const LLM_TEST_SECONDS: u64 = 60;
pub const VISION_TEST_SECONDS: u64 = 120;
pub const HEALTH_SECONDS: u64 = 15;

/// The LLM test request body: one completion, `max_tokens` 16, thinking forced off in the three places servers read it (top level and `chat_template_kwargs`; prototype `thinkSwitch`), else a thinking model spends its tokens reasoning and the row goes red over a fine server. Not streamed.
pub fn llm_test_body(model: &str) -> Value {
    json!({
        "model": model,
        "messages": [{ "role": "user", "content": "Reply with the single word: ok" }],
        "temperature": 0.6,
        "max_tokens": LLM_TEST_MAX_TOKENS,
        "enable_thinking": false,
        "preserve_thinking": true,
        "chat_template_kwargs": { "enable_thinking": false, "preserve_thinking": true },
    })
}

/// The four bytes of red the probe is filled with.
const PROBE_RED: [u8; 3] = [0xDC, 0x00, 0x00];
/// The probe's edge in pixels: above any vision encoder's patch size, few tokens (§5).
const PROBE_SIZE: u32 = 48;

/// A 48 px solid red PNG written out here, not shipped as an asset; hence the hand-written encoder (stored-deflate zlib, no compression needed). The prototype uses `image/png` + `draw` (go/setup.go:541-547).
pub fn red_square_png() -> Vec<u8> {
    let row_len = PROBE_SIZE as usize * 3;
    let mut raw = Vec::with_capacity((row_len + 1) * PROBE_SIZE as usize);
    for _ in 0..PROBE_SIZE {
        raw.push(0); // filter type 0: none
        raw.extend_from_slice(&PROBE_RED.repeat(PROBE_SIZE as usize));
    }

    let mut png = Vec::new();
    png.extend_from_slice(b"\x89PNG\r\n\x1a\n");
    let mut header = Vec::with_capacity(13);
    header.extend_from_slice(&PROBE_SIZE.to_be_bytes()); // width
    header.extend_from_slice(&PROBE_SIZE.to_be_bytes()); // height
    header.push(8); // bit depth
    header.push(2); // colour type: truecolour, no alpha
    header.push(0);
    header.push(0);
    header.push(0); // compression, filter, interlace
    chunk(&mut png, b"IHDR", &header);
    chunk(&mut png, b"IDAT", &zlib_stored(&raw));
    chunk(&mut png, b"IEND", b"");
    png
}

pub fn vision_probe_url() -> String {
    format!("data:image/png;base64,{}", base64(&red_square_png()))
}

/// A PNG chunk: length, type, body, CRC of type + body (the type is inside the CRC but outside the length).
fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], body: &[u8]) {
    out.extend_from_slice(&(body.len() as u32).to_be_bytes());
    let mut crc_source = Vec::with_capacity(4 + body.len());
    crc_source.extend_from_slice(kind);
    crc_source.extend_from_slice(body);
    out.extend_from_slice(&crc_source);
    out.extend_from_slice(&crc32(&crc_source).to_be_bytes());
}

fn zlib_stored(data: &[u8]) -> Vec<u8> {
    let mut out = vec![0x78, 0x01]; // deflate, 32 KiB window: the pair every decoder accepts
    for (index, block) in data.chunks(65_535).enumerate() {
        let last = index + 1 == data.len().div_ceil(65_535);
        out.push(u8::from(last)); // BFINAL only on the last block
        let len = block.len() as u16;
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&(!len).to_le_bytes());
        out.extend_from_slice(block);
    }
    out.extend_from_slice(&adler32(data).to_be_bytes());
    out
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = !0u32;
    for byte in data {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            let low = crc & 1;
            crc >>= 1;
            if low == 1 {
                crc ^= 0xEDB8_8320;
            }
        }
    }
    !crc
}

fn adler32(data: &[u8]) -> u32 {
    // 6552 is the largest n for which the sums cannot overflow a u32 before the modulo.
    const CHUNK: usize = 5552;
    let (mut a, mut b) = (1u32, 0u32);
    for block in data.chunks(CHUNK) {
        for byte in block {
            a = (a + u32::from(*byte)) % 65521;
            b = (b + a) % 65521;
        }
    }
    (b << 16) | a
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Standard-alphabet base64 with padding: a data URL read by the model's server must be the dialect every decoder implements.
pub fn base64(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for group in data.chunks(3) {
        let bytes = [
            group[0],
            *group.get(1).unwrap_or(&0),
            *group.get(2).unwrap_or(&0),
        ];
        let joined = (u32::from(bytes[0]) << 16) | (u32::from(bytes[1]) << 8) | u32::from(bytes[2]);
        for index in 0..4 {
            out.push(if index > group.len() && index > 2 {
                '=' // padding stands in for the bytes the last group did not have
            } else {
                B64[(joined >> (18 - 6 * index) & 0x3F) as usize] as char
            });
        }
    }
    out
}

/// The vision row's verdict: pass only if the model saw red (a text-only model, or vision without its mmproj, otherwise fails minutes into a run). Case-insensitive.
pub fn vision_verdict(reply: &str) -> Result<String, String> {
    if reply.to_lowercase().contains("red") {
        return Ok(format!("the model saw the red square: {reply:?}"));
    }
    Err(format!(
        "shown a plain red square it answered {reply:?} -- it is not seeing the image. Prepare \
         sends video frames to this model, so it needs a vision model served with its mmproj file"
    ))
}

/// The TTS endpoint row: healthy (`/health` at [`HEALTH_SECONDS`]) and able to clone a voice (family `index_tts2` or task `clon`); a server serving only step-1 models answers requests but cannot narrate.
pub fn tts_endpoint_verdict(models: &[AudioModel]) -> Result<String, String> {
    if models.is_empty() {
        return Err("healthy, but serving no models".to_string());
    }
    models
        .iter()
        .find(|model| model.family == "index_tts2" || model.task == "clon")
        .map(|model| {
            format!(
                "will narrate with {:?}, of {} model(s)",
                model.id,
                models.len()
            )
        })
        .ok_or_else(|| {
            let mut ids: Vec<&str> = models.iter().map(|model| model.id.as_str()).collect();
            ids.sort();
            format!(
                "serves {} -- none of them can clone a voice",
                ids.join(", ")
            )
        })
}

/// The verdict for one model id against the server's catalogue: is it there and declared for the task this step asks? An empty declared task passes (prototype `m.Task != "" && m.Task != task`, go/setup.go:691); a wrong one fails. `step` names who is asking (§8).
pub fn model_verdict(
    models: &[AudioModel],
    wanted_id: &str,
    needed: services::Need<'_>,
) -> Result<String, String> {
    let services::Need {
        task: wanted_task,
        step,
    } = needed;
    match models.iter().find(|model| model.id == wanted_id) {
        None => Err(services::missing_model(
            "the audio.cpp server",
            models,
            wanted_id,
            needed,
        )),
        Some(model) if !model.task.is_empty() && model.task != wanted_task => Err(format!(
            "{wanted_id:?} is declared task {:?}, and {step} needs {wanted_task:?}",
            model.task
        )),
        Some(model) => Ok(format!("{wanted_id:?} is served (task {:?})", model.task)),
    }
}

/// The aligner row, with no box: alignment is asked by TASK, so the catalogue is the whole answer. [`services::pick_aligner`] is the run's own choice. No aligner is not a failure: joins fall back to the waveform ([`crate::degraded`]).
pub fn aligner_verdict(configured: &str, models: &[AudioModel]) -> Result<String, String> {
    match services::pick_aligner(configured, models) {
        Some(id) => Ok(format!("{id:?} places the cut points, on the word")),
        None if !configured.trim().is_empty() => Err(format!(
            "{configured:?} is not served as task \"align\" -- the server's own list has: {}",
            declared(models)
        )),
        None => Ok(
            "no model does forced alignment, so cut points come off the waveform: clean \
                    wherever there is a silence to cut in, unable to cut between two words of one \
                    breath"
                .to_string(),
        ),
    }
}

fn declared(models: &[AudioModel]) -> String {
    let mut ids: Vec<&str> = models.iter().map(|model| model.id.as_str()).collect();
    ids.sort();
    ids.join(", ")
}

/// What the pipeline asks ffmpeg for by name; each is a build option (rubberband and libx264 need `--enable-gpl`, subtitles needs libass) that would otherwise fail minutes into a render.
pub const FF_FILTERS: [&str; 7] = [
    "rubberband",
    "subtitles",
    "loudnorm",
    "atempo",
    "amix",
    "adelay",
    "alimiter",
];

pub const FF_ENCODERS: [&str; 4] = ["libx264", "libx265", "aac", "libopus"];

/// Which of `want` this build lacks, from the `-filters`/`-encoders` listing (name in the second whitespace field); a missing listing (`None`) counts as nothing checked.
pub fn ffmpeg_missing<'a>(list: &str, want: &[&'a str]) -> Vec<&'a str> {
    want.iter()
        .filter(|name| {
            !list.lines().any(|line| {
                line.split_whitespace()
                    .nth(1)
                    .is_some_and(|field| field == **name)
            })
        })
        .copied()
        .collect()
}

/// The ffmpeg row's verdict in §5's order: the binary, ffprobe beside it, then the build. This is a diagnosis order: a missing binary is not "built without rubberband".
pub fn ffmpeg_verdict(
    resolved_path: Option<&str>,
    ffprobe_beside: bool,
    version_line: &str,
    filters_list: Option<&str>,
    encoders_list: Option<&str>,
) -> Result<String, String> {
    let Some(ffmpeg) = resolved_path else {
        return Err(
            "not on PATH -- every step shells out to it. Leave the box empty to use PATH, \
                    or name a binary"
                .to_string(),
        );
    };
    if !ffprobe_beside {
        let probe = services::ffprobe_for(ffmpeg, "ffprobe");
        return Err(format!(
            "found ffmpeg at {ffmpeg}, but no ffprobe beside it ({probe}) -- both are used, and \
             they have to be one build"
        ));
    }
    let version = version_line.lines().next().unwrap_or("").trim();
    if version.is_empty() {
        return Err(format!("{ffmpeg} printed nothing for -version"));
    }
    let mut missing = match filters_list {
        Some(list) => ffmpeg_missing(list, &FF_FILTERS),
        None => FF_FILTERS.to_vec(),
    };
    missing.extend(match encoders_list {
        Some(list) => ffmpeg_missing(list, &FF_ENCODERS),
        None => FF_ENCODERS.to_vec(),
    });
    if !missing.is_empty() {
        return Err(format!(
            "{version} at {ffmpeg} is built without {} -- the steps that need those will fail \
             mid-run",
            missing.join(", ")
        ));
    }
    Ok(format!(
        "{version} at {ffmpeg}, with every filter and encoder the pipeline uses"
    ))
}

pub const FIREFOX_OFF: &str = "off";

/// The firefox row's verdict: `off` passes, else it must run AND a real headless search must come back, since a firefox that prints its version but cannot be driven shows up as a model writing with no sources.
pub fn firefox_verdict(
    box_value: &str,
    version_stdout: Option<&str>,
    search_hits: Option<usize>,
) -> Result<String, String> {
    if box_value.trim().eq_ignore_ascii_case(FIREFOX_OFF) {
        return Ok(
            "off -- the model is offered no web search, and writes only what the material \
                   says"
                .to_string(),
        );
    }
    let version = match version_stdout {
        Some(stdout) => {
            let line = stdout.lines().next().unwrap_or("").trim();
            if line.is_empty() {
                return Err("it printed nothing for --version".to_string());
            }
            line.to_string()
        }
        None => return Err("will not run: --version failed".to_string()),
    };
    match search_hits {
        Some(hits) => Ok(format!(
            "{version}, drove a headless search: {hits} result(s)"
        )),
        None => Err(format!("{version} runs, but could not be driven headless")),
    }
}

/// The only two things `GET /v1/models` on the LLM server is for: finding the model id ("Fetch models"), and sd.cpp's fallback probe when its capabilities call fails. A wrong id is proven by a completion, not this list.
pub const LIST_MODELS_LLM_USES: [&str; 2] = ["Fetch models", "sd.cpp fallback probe"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub name: String,
    pub ok: bool,
    pub why: String,
}

/// No Save button (§5): the file is written 600 ms after the last keystroke and again on close if owed, so screen and disk never differ.
pub const CONF_SAVE_WAIT: Duration = Duration::from_millis(600);

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SaveState {
    owed: bool,
    due: Option<Duration>,
}

impl SaveState {
    /// A keystroke pushes the pending write back so typing writes once.
    pub fn touched(&mut self, now: Duration) {
        self.owed = true;
        self.due = Some(now + CONF_SAVE_WAIT);
    }

    pub fn due_at(&self) -> Option<Duration> {
        self.due
    }

    /// Whether the timer may write now; a touched row says no until its new beat elapses.
    pub fn writable(&self, now: Duration) -> bool {
        self.owed && self.due.is_some_and(|due| now >= due)
    }

    /// Closing: write if a beat has not elapsed (losing the last thing typed is the one failure to avoid). Consumes the debt.
    pub fn close(&mut self) -> bool {
        let owed = self.owed;
        self.owed = false;
        self.due = None;
        owed
    }

    pub fn saved(&mut self) {
        self.owed = false;
        self.due = None;
    }
}

/// What a save invalidates besides the file: the cached TTS model id and the "already listening on" note, which belong to the server being replaced (prototype go/setup.go:1227-1231).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Draft {
    pub state: SaveState,
    pub cached_tts_model: String,
    pub audio_noted: String,
}

impl Draft {
    /// Write if the beat has elapsed. Only a save clears both caches, not a pending keystroke.
    pub fn save(&mut self, now: Duration) -> bool {
        if !self.state.writable(now) {
            return false;
        }
        self.state.saved();
        self.cached_tts_model.clear();
        self.audio_noted.clear();
        true
    }

    pub fn close(&mut self) -> bool {
        if !self.state.close() {
            return false;
        }
        self.cached_tts_model.clear();
        self.audio_noted.clear();
        true
    }
}

/// The LLM row's verdict: the answer and how long it took (§5). The wait tells a cold server loading from a hung one (prototype `go/setup.go:534`).
pub fn llm_verdict(model: &str, seconds: f64, reply: &str) -> String {
    format!("{model} answered in {seconds:.1} s: {reply:?}")
}

/// The audio.cpp endpoint row: `/health` answered (at [`HEALTH_SECONDS`]) and its catalogue names a voice (§5), in one sentence; the voice half is [`tts_endpoint_verdict`].
pub fn audio_health_verdict(health_ms: u64, models: &[AudioModel]) -> Result<String, String> {
    if models.is_empty() {
        return Err("healthy, but serving no models".to_string());
    }
    Ok(format!(
        "healthy in {health_ms} ms, {}",
        tts_endpoint_verdict(models)?
    ))
}

/// What the sd.cpp row reads from `GET /sdcpp/v1/capabilities`: the loaded weights and what it can generate; it only proves a model that draws is behind the port.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SdCaps {
    pub weights: String,
    pub modes: Vec<String>,
}

/// sd-server's capabilities reply: the weights are an OBJECT (`"model": {"name": …, "path": …, "stem": …}`) named by its stem, else file name; modes are `"supported_modes": ["img_gen"]`. A non-JSON reply is said so with serde's reason (reading `model` as a string once failed on every real server).
pub fn sd_caps(body: &str) -> Result<SdCaps, String> {
    #[derive(serde::Deserialize, Default)]
    struct Model {
        #[serde(default)]
        name: String,
        #[serde(default)]
        stem: String,
    }
    #[derive(serde::Deserialize)]
    struct Caps {
        #[serde(default)]
        model: Option<Model>,
        #[serde(default)]
        supported_modes: Vec<String>,
    }
    let caps: Caps = serde_json::from_str(body)
        .map_err(|why| format!("it answered something that is not sd.cpp capabilities: {why}"))?;
    let model = caps.model.unwrap_or_default();
    let stem = model.stem.trim();
    Ok(SdCaps {
        weights: if stem.is_empty() {
            model.name.trim()
        } else {
            stem
        }
        .to_string(),
        modes: caps.supported_modes,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SdProbe {
    OpenAiOnly,
    Unreachable,
}

/// The sd.cpp row's verdict: capabilities → "<weights> is loaded and can draw" (§5). `Err` is the call failing; its shape is the advice, since a port serving only `/v1/models` is an OpenAI-only tenant ([`crate::services::Endpoint`]).
pub fn sd_verdict(capabilities: Result<&SdCaps, SdProbe>) -> Result<String, String> {
    match capabilities {
        // A build reporting modes must list `img_gen` (a video-model server fails every draw); one reporting none is older, not broken.
        Ok(caps)
            if !caps.weights.trim().is_empty()
                && !caps.modes.is_empty()
                && !caps.modes.iter().any(|mode| mode == "img_gen") =>
        {
            Err(format!(
                "{} is loaded, but this server cannot draw images -- it supports {}, not img_gen",
                caps.weights.trim(),
                caps.modes.join(", ")
            ))
        }
        Ok(caps) if !caps.weights.trim().is_empty() => {
            Ok(format!("{} is loaded and can draw", caps.weights.trim()))
        }
        Ok(_) => Err(
            "it answers capabilities but names no weights -- nothing is loaded to draw with"
                .to_string(),
        ),
        Err(SdProbe::OpenAiOnly) => Err(
            "that port answers /v1/models and not /sdcpp/v1/capabilities -- it is an OpenAI-shaped \
             server, not sd-server, and Prepare draws images through sd-server's own API"
                .to_string(),
        ),
        Err(SdProbe::Unreachable) => Err(
            "nothing answered /sdcpp/v1/capabilities -- sd-server is not up at that address"
                .to_string(),
        ),
    }
}

/// The prefix of every Test line in the main log (§5: `settings: …`), with the row's name inside the line.
pub const LOG_PREFIX: &str = "settings:";

/// One Test line in the main log: `settings: <row>: <verdict>`, or `-- <reason>` on failure. The caller adds the `!!!` marker.
pub fn log_line(name: &str, verdict: &Result<String, String>) -> String {
    match verdict {
        Ok(detail) => format!("{LOG_PREFIX} {name}: {detail}"),
        Err(reason) => format!("{LOG_PREFIX} {name} -- {reason}"),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Phase {
    #[default]
    Idle,
    Running,
    Finished(Result<String, String>),
}

/// One Test button: `press` takes the value the box holds at that moment (§5 "reads what is TYPED"), as the file lags the keyboard by [`CONF_SAVE_WAIT`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestRow {
    pub name: &'static str,
    pub typed: String,
    pub phase: Phase,
}

impl TestRow {
    pub fn new(name: &'static str) -> Self {
        Self {
            name,
            typed: String::new(),
            phase: Phase::Idle,
        }
    }

    /// The press: remember what was typed, drop any earlier verdict, spin (a stale ✓ would lie about the new address).
    pub fn press(&mut self, typed: &str) {
        self.typed = typed.to_string();
        self.phase = Phase::Running;
    }

    pub fn finish(&mut self, verdict: Result<String, String>) {
        self.phase = Phase::Finished(verdict);
    }

    pub fn spinning(&self) -> bool {
        matches!(self.phase, Phase::Running)
    }

    pub fn mark(&self) -> Option<char> {
        match &self.phase {
            Phase::Finished(Ok(_)) => Some('✓'),
            Phase::Finished(Err(_)) => Some('✗'),
            _ => None,
        }
    }

    /// The tooltip: the verdict (§5), as ✗ says a row failed and only the sentence says why.
    pub fn tooltip(&self) -> Option<String> {
        match &self.phase {
            Phase::Finished(Ok(detail)) | Phase::Finished(Err(detail)) => Some(detail.clone()),
            _ => None,
        }
    }

    pub fn log(&self) -> Option<String> {
        match &self.phase {
            Phase::Finished(verdict) => Some(log_line(self.name, verdict)),
            Phase::Running | Phase::Idle => None,
        }
    }
}

/// The Settings dialog's own log of test verdicts; the first failure opens it ([`TestLog::open`], [`TestLog::push`]; spec/03-shell.md §8).
#[derive(Debug, Clone, Default)]
pub struct TestLog {
    pub lines: Vec<String>,
    pub open: bool,
}

impl TestLog {
    /// Collapsed until a row reports something.
    pub fn new() -> Self {
        Self::default()
    }

    /// One verdict; the first failure opens the log and it never closes itself, so the reason stays visible.
    pub fn push(&mut self, verdict: String, passed: bool) {
        if !passed {
            self.open = true;
        }
        self.lines.push(verdict);
    }
}
