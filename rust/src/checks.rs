//! What the Settings dialog's ten Test buttons and "Test All" decide (§5).
//!
//! Nothing here opens a socket or runs a program: each function is handed what a request or a
//! binary answered — a catalogue, a stdout, a hit count — and returns the verdict. That is what
//! lets the pass/fail rule of every button be tested without a server, and it is why the wording
//! below is shorter than the prototype's while the decisions (gui/setup.go:520-815) are the same.

use std::time::Duration;

use serde_json::{json, Value};

use crate::services::{self, AudioModel};

/// The four rows on the audio.cpp server, each with its own Test button (§5): the TTS model, ASR,
/// diarization and separation. Named here so the dialog's list and [`model_verdict`]'s callers
/// cannot drift apart.
pub const AUDIO_ROWS: [(&str, &str); 4] = [
    ("TTS model", "clon"),
    ("ASR", "asr"),
    ("Diarization", "diar"),
    ("Separation", "sep"),
];

/// The LLM test asks for one completion of sixteen tokens: enough for a word and a stop, so a
/// model that rambles is cut off rather than charged for (§5).
pub const LLM_TEST_MAX_TOKENS: u32 = 16;
/// A slow text answer is the server loading the model, so the text test gets a minute.
pub const LLM_TEST_SECONDS: u64 = 60;
/// The vision test gets twice that: an image prompt makes a cold server load its vision encoder too.
pub const VISION_TEST_SECONDS: u64 = 120;
/// `/health` either answers at once or something is wrong with the port.
pub const HEALTH_SECONDS: u64 = 15;

/// The body of the LLM test request: one completion, `max_tokens` 16, thinking forced off.
///
/// Thinking is switched off three ways because servers read the flag in three places — top level,
/// and again inside `chat_template_kwargs` for a server that forwards only that to the template —
/// which is the same switch the pipeline's execute mode sends (prototype `thinkSwitch`,
/// gui/llm.go:267-276). It matters here because the test measures whether *answering at all*
/// works: a thinking model spends its sixteen tokens on reasoning and answers nothing, and the row
/// goes red over a server that is fine. Not streamed, unlike the pipeline: there is nothing to
/// watch arrive.
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

/// The four bytes of red the probe is filled with — a colour a model has to name correctly, and one
/// no caption of a stock photo would produce by accident.
const PROBE_RED: [u8; 3] = [0xDC, 0x00, 0x00];
/// The probe's edge in pixels: enough for any vision encoder's patch size, few enough tokens to
/// keep the vision test as quick as the text one (§5).
const PROBE_SIZE: u32 = 48;

/// A 48 px solid red square, as a PNG written out here rather than shipped as an asset.
///
/// A generated file is the point: nothing to lose in a packaging step, and it stays what it is
/// even when the test runs on a machine with no image library — which is also why the encoder below
/// is hand-written (a stored-deflate zlib stream: no compression is needed for 48×48 of one
/// colour). The prototype does the same thing with `image/png` + `draw` (gui/setup.go:541-547).
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
    chunk(&mut png, b"IDAT", &zlib_stored(&raw), );
    chunk(&mut png, b"IEND", b"");
    png
}

/// The data URL the vision test sends: §5's "a generated 48 px red square as a data URL".
pub fn vision_probe_url() -> String {
    format!("data:image/png;base64,{}", base64(&red_square_png()))
}

/// A PNG chunk: length, type, body, then the CRC of type + body (the type is inside the CRC and
/// outside the length, which is the one thing a hand-written writer gets wrong).
fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], body: &[u8]) {
    out.extend_from_slice(&(body.len() as u32).to_be_bytes());
    let mut crc_source = Vec::with_capacity(4 + body.len());
    crc_source.extend_from_slice(kind);
    crc_source.extend_from_slice(body);
    out.extend_from_slice(&crc_source);
    out.extend_from_slice(&crc32(&crc_source).to_be_bytes());
}

/// A zlib stream of stored (uncompressed) blocks: two header bytes, then 65 535-byte blocks each
/// preceded by its length and one's complement, then the Adler-32 of the input.
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

/// CRC-32 as PNG and zlib define it (polynomial 0xEDB88320, reflected, no output inversion).
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

/// Adler-32, the checksum zlib carries instead of a CRC.
fn adler32(data: &[u8]) -> u32 {
    // 6552 is the largest n for which the sums cannot overflow a u32 before the modulo, so the
    // reduction happens on schedule rather than per byte.
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

/// Standard-alphabet base64 with padding — a data URL is read by the model's server, so it has to
/// be the dialect every decoder implements rather than a URL-safe one.
pub fn base64(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for group in data.chunks(3) {
        let bytes = [group[0], *group.get(1).unwrap_or(&0), *group.get(2).unwrap_or(&0)];
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

/// The vision row's verdict: pass only if the model saw red.
///
/// A text-only model, or a vision model served without its mmproj file, passes the text test and
/// then fails minutes into a run — so the answer either names the colour or is a confession.
/// Case-insensitive because models answer "Red", "red" and "It is red." alike.
pub fn vision_verdict(reply: &str) -> Result<String, String> {
    if reply.to_lowercase().contains("red") {
        return Ok(format!("the model saw the red square: {reply:?}"));
    }
    Err(format!(
        "shown a plain red square it answered {reply:?} -- it is not seeing the image. Prepare \
         sends video frames to this model, so it needs a vision model served with its mmproj file"
    ))
}

/// The TTS endpoint row: healthy (the caller's `/health` at [`HEALTH_SECONDS`]) and then able to
/// clone a voice — family `index_tts2` or task `clon`, either one, since a server can declare the
/// same model both ways.
///
/// This is what the row tests rather than "is there a model on that port": a server on the right
/// port serving only step-1 models answers every request and still cannot narrate.
pub fn tts_endpoint_verdict(models: &[AudioModel]) -> Result<String, String> {
    if models.is_empty() {
        return Err("healthy, but serving no models".to_string());
    }
    models
        .iter()
        .find(|model| model.family == "index_tts2" || model.task == "clon")
        .map(|model| format!("will narrate with {:?}, of {} model(s)", model.id, models.len()))
        .ok_or_else(|| {
            let mut ids: Vec<&str> = models.iter().map(|model| model.id.as_str()).collect();
            ids.sort();
            format!("serves {} -- none of them can clone a voice", ids.join(", "))
        })
}

/// One id per button — the TTS model, ASR, diarization, separation: is that id on the server, and
/// declared for the task this step will ask of it?
///
/// An empty declared task passes: many servers list an id with no task at all, and refusing those
/// would fail a working setup over a field the server left blank (prototype: `m.Task != "" &&
/// m.Task != task`, gui/setup.go:691). A *wrong* task is the interesting failure — a catalogue
/// entry copied from another model with its task left as it was.
pub fn model_verdict(models: &[AudioModel], wanted_id: &str, wanted_task: &str) -> Result<String, String> {
    match models.iter().find(|model| model.id == wanted_id) {
        None => Err(services::missing_model(
            "the audio.cpp server",
            models,
            wanted_id,
            wanted_task,
        )),
        Some(model) if !model.task.is_empty() && model.task != wanted_task => Err(format!(
            "{wanted_id:?} is declared task {:?}, and cannot be used for {wanted_task:?}",
            model.task
        )),
        Some(model) => Ok(format!("{wanted_id:?} is served (task {:?})", model.task)),
    }
}

/// The aligner row, which has no box beside it — and that is the point. Every other model here is
/// asked for by id because the request carries one; alignment is asked for by TASK, so the
/// catalogue is the whole of the answer and a name in a settings box could only ever disagree with
/// the server. [`services::pick_aligner`] is the same choice the run makes, so the row cannot go
/// green while the pipeline uses something else.
///
/// Having no aligner is not a failure: it is a smaller tool. The joins fall back to the waveform
/// ([`crate::degraded`]), which cuts wherever there is a silence and cannot cut between two words of
/// one breath.
pub fn aligner_verdict(configured: &str, models: &[AudioModel]) -> Result<String, String> {
    match services::pick_aligner(configured, models) {
        Some(id) => Ok(format!("{id:?} places the cut points, on the word")),
        None if !configured.trim().is_empty() => Err(format!(
            "{configured:?} is not served as task \"align\" -- the server's own list has: {}",
            declared(models)
        )),
        None => Ok("no model does forced alignment, so cut points come off the waveform: clean \
                    wherever there is a silence to cut in, unable to cut between two words of one \
                    breath"
            .to_string()),
    }
}

fn declared(models: &[AudioModel]) -> String {
    let mut ids: Vec<&str> = models.iter().map(|model| model.id.as_str()).collect();
    ids.sort();
    ids.join(", ")
}

/// What the pipeline asks ffmpeg for by name. Each is a real build option rather than a given:
/// rubberband and libx264 need `--enable-gpl`, subtitles needs libass, so a build missing one works
/// perfectly until the step that uses it — minutes into a render.
///
/// `drawtext` used to be in here for the Publish step's thumbnail title; the title is lettered by
/// the image model now, and requiring a filter nothing asks for would fail an otherwise good build.
pub const FF_FILTERS: [&str; 7] = [
    "rubberband",
    "subtitles",
    "loudnorm",
    "atempo",
    "amix",
    "adelay",
    "alimiter",
];

/// The same for encoders (`libx264`/`libx265` again need `--enable-gpl`).
pub const FF_ENCODERS: [&str; 4] = ["libx264", "libx265", "aac", "libopus"];

/// Which of `want` this build does not have, reading the `-filters`/`-encoders` listing. Both print
/// one component per line with the name in the second whitespace field, after a flags column; a
/// listing that is missing entirely (`None` upstream) counts as nothing having been checked. The
/// names come back borrowed from `want`, whose caller passes the `'static` constants below.
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

/// The ffmpeg row's verdict, in §5's order: which binary the box resolves to, that ffprobe is
/// beside it, then what the build has.
///
/// That order is a diagnosis order. A missing binary and a missing ffprobe are both fixed outside
/// this app, and neither should be reported as "built without rubberband"; the component list is
/// only worth reading once there is one ffmpeg whose build is being described.
pub fn ffmpeg_verdict(
    resolved_path: Option<&str>,
    ffprobe_beside: bool,
    version_line: &str,
    filters_list: Option<&str>,
    encoders_list: Option<&str>,
) -> Result<String, String> {
    let Some(ffmpeg) = resolved_path else {
        return Err("not on PATH -- every step shells out to it. Leave the box empty to use PATH, \
                    or name a binary"
            .to_string());
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
    Ok(format!("{version} at {ffmpeg}, with every filter and encoder the pipeline uses"))
}

/// The value in the firefox box that means "no web search", not "a binary called off".
pub const FIREFOX_OFF: &str = "off";

/// The firefox row's verdict: `off` is a pass (it is the box saying the model gets no search), else
/// that it runs *and* that a real headless search came back.
///
/// The second half is not pedantry: a firefox that prints its version and then cannot be driven —
/// no debugging port, a profile already open, a sandbox refusing to start — is the failure a
/// version check cannot see, and it shows up as a model writing with no sources minutes later.
pub fn firefox_verdict(
    box_value: &str,
    version_stdout: Option<&str>,
    search_hits: Option<usize>,
) -> Result<String, String> {
    if box_value.trim().eq_ignore_ascii_case(FIREFOX_OFF) {
        return Ok("off -- the model is offered no web search, and writes only what the material \
                   says"
            .to_string());
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
        Some(hits) => Ok(format!("{version}, drove a headless search: {hits} result(s)")),
        None => Err(format!(
            "{version} runs, but could not be driven headless"
        )),
    }
}

/// The only two things `GET /v1/models` on the LLM server is for.
///
/// Both are about a list rather than a completion: the model id has no default and must be one this
/// server actually serves, so "Fetch models" is how it is found; and sd.cpp falls back to probing
/// `/v1/models` when its capabilities call fails, since an OpenAI-only tenant on the same port
/// answers that and not `/sdcpp/v1/capabilities`. Nothing in a run reads the LLM's list — a wrong
/// id has to be proven by a completion, which is what the LLM row does.
pub const LIST_MODELS_LLM_USES: [&str; 2] = ["Fetch models", "sd.cpp fallback probe"];

/// Is the configured id one the server lists? Empty is its own answer: there is no default model id
/// to fall back to, so an empty box means nothing has been chosen yet.
pub fn llm_model_listed(models: &[String], wanted: &str) -> Result<String, String> {
    let wanted = wanted.trim();
    if wanted.is_empty() {
        return Err("no LLM model configured -- use Fetch models and pick one".to_string());
    }
    if models.iter().any(|id| id == wanted) {
        return Ok(format!("{wanted:?} is served by this server"));
    }
    Err(format!(
        "this server does not serve {wanted:?} -- it lists {}",
        models.join(", ")
    ))
}

/// One Test button's verdict: the row that asked, and whether it passed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub name: String,
    pub ok: bool,
    pub why: String,
}

/// "Test All" — every row's own verdict, in the order the rows are on screen (§5: ten buttons and
/// "Test All", each with its own verdict).
///
/// Nothing short-circuits: a failing LLM says nothing about the drawing server, and one early
/// failure hiding the rest would send the user through the dialog ten times. The order is kept so
/// the answers arrive as the rows are read.
pub fn test_all(verdicts: &[(&str, Result<String, String>)]) -> Vec<Row> {
    verdicts
        .iter()
        .map(|(name, verdict)| match verdict {
            Ok(detail) => Row {
                name: (*name).to_string(),
                ok: true,
                why: detail.clone(),
            },
            Err(reason) => Row {
                name: (*name).to_string(),
                ok: false,
                why: reason.clone(),
            },
        })
        .collect()
}

/// No Save button (§5): the file is written 600 ms after the last keystroke, and again on close if
/// a write is still owed.
///
/// A box that wrote per letter would rewrite a 0600 file for every character typed into a key, and
/// a dialog with a Save button has a state where what is on screen is not what is on disk — which
/// is exactly what "it saved when I closed it" removes.
pub const CONF_SAVE_WAIT: Duration = Duration::from_millis(600);

/// The pending write: when it is due, and whether anything is owed at all.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SaveState {
    owed: bool,
    due: Option<Duration>,
}

impl SaveState {
    /// A keystroke: the pending write is pushed back, so the file is written once the typing stops
    /// rather than once per letter.
    pub fn touched(&mut self, now: Duration) {
        self.owed = true;
        self.due = Some(now + CONF_SAVE_WAIT);
    }

    /// When the write is due, if one is pending. The UI arms its timer with this.
    pub fn due_at(&self) -> Option<Duration> {
        self.due
    }

    /// Whether the timer may write now. A row that has been touched again says no until the new
    /// beat has elapsed, which is what keeps a pause mid-word from producing two writes.
    pub fn writable(&self, now: Duration) -> bool {
        self.owed && self.due.is_some_and(|due| now >= due)
    }

    /// Closing the window: write if a beat has not elapsed, because losing the last thing typed is
    /// the one failure this dialog must not have. Consumes the debt either way.
    pub fn close(&mut self) -> bool {
        let owed = self.owed;
        self.owed = false;
        self.due = None;
        owed
    }

    /// The write happened, so nothing is owed until the next keystroke.
    pub fn saved(&mut self) {
        self.owed = false;
        self.due = None;
    }
}

/// What a save invalidates besides the file: the cached TTS model id and the "already listening on"
/// note, both of which belong to the server being replaced — a different server has a different
/// catalogue, so keeping either would narrate with an id that no longer exists, or report a port as
/// known from a run that never happened here (prototype gui/setup.go:1227-1231).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Draft {
    pub state: SaveState,
    pub cached_tts_model: String,
    pub audio_noted: String,
}

impl Draft {
    /// Write the file if the beat has elapsed. A save clears both caches, and only a save does: a
    /// keystroke that is still pending must not invalidate anything yet.
    pub fn save(&mut self, now: Duration) -> bool {
        if !self.state.writable(now) {
            return false;
        }
        self.state.saved();
        self.cached_tts_model.clear();
        self.audio_noted.clear();
        true
    }

    /// The close request: write when a write is owed, and invalidate the same two things.
    pub fn close(&mut self) -> bool {
        if !self.state.close() {
            return false;
        }
        self.cached_tts_model.clear();
        self.audio_noted.clear();
        true
    }
}

/// The LLM row's verdict: the answer and how long it took (§5's "<model> answered in X s: “ok”").
///
/// One completion is the whole test, so those two facts are the whole result. The wait is not
/// decoration — it is what tells a cold server loading its model from one that has hung, which are
/// fixed by waiting and by restarting respectively and look identical from a spinner (prototype
/// `gui/setup.go:534`).
pub fn llm_verdict(model: &str, seconds: f64, reply: &str) -> String {
    format!("{model} answered in {seconds:.1} s: {reply:?}")
}

/// The audio.cpp endpoint row: the port answered `/health` (at [`HEALTH_SECONDS`]) and its
/// catalogue names a voice — §5's "healthy in N ms, will narrate with <id>".
///
/// Health alone would green-light a server that cannot narrate, so the two are one sentence: the
/// number says the address is right, the id says something on it can clone a voice. The voice half
/// is [`tts_endpoint_verdict`], which the TTS endpoint's own button uses too.
pub fn audio_health_verdict(health_ms: u64, models: &[AudioModel]) -> Result<String, String> {
    if models.is_empty() {
        return Err("healthy, but serving no models".to_string());
    }
    Ok(format!(
        "healthy in {health_ms} ms, {}",
        tts_endpoint_verdict(models)?
    ))
}

/// The one thing the sd.cpp row reads out of `GET /sdcpp/v1/capabilities`: which weights the server
/// has loaded. Everything else the reply carries (sizes, steps, samplers) is what a draw request may
/// ask for, and this row only proves there is a model behind the port.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SdCaps {
    pub weights: String,
}

/// How the capabilities call failed, since the two failures are fixed differently.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SdProbe {
    /// `/v1/models` answered and `/sdcpp/v1/capabilities` did not: an OpenAI-shaped server, not
    /// sd-server.
    OpenAiOnly,
    /// Nothing answered at all.
    Unreachable,
}

/// The sd.cpp row's verdict: capabilities → "<weights> is loaded and can draw" (§5). `capabilities`
/// is what the caller got back, `Err` when the call itself failed — the shape of the failure *is* the
/// advice, because a port serving `/v1/models` and nothing else is an OpenAI-only tenant, which is a
/// different server to start than the one Prepare draws through ([`crate::services::Endpoint`]’s own
/// comment records that `/v1/models` is this row's fallback probe).
pub fn sd_verdict(capabilities: Result<&SdCaps, SdProbe>) -> Result<String, String> {
    match capabilities {
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

/// The prefix every Test line takes in the main log (§5: "mirrors its lines into the main log as
/// \"settings: …\""). The log is where a verdict is read after the dialog has closed, so the row's
/// name is inside the line rather than only beside it on screen.
pub const LOG_PREFIX: &str = "settings:";

/// One Test line in the main log: `settings: <row>: <verdict>`, or `-- <reason>` when it failed so a
/// failure reads like every other line in the log. The caller adds the `!!!` marker the run's own
/// failures carry — this module does not own the log, only what it says.
pub fn log_line(name: &str, verdict: &Result<String, String>) -> String {
    match verdict {
        Ok(detail) => format!("{LOG_PREFIX} {name}: {detail}"),
        Err(reason) => format!("{LOG_PREFIX} {name} -- {reason}"),
    }
}

/// What one Test button is doing.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Phase {
    #[default]
    Idle,
    Running,
    Finished(Result<String, String>),
}

/// One Test button: what its box said when it was pressed, and what came back.
///
/// `press` takes the value the box holds at that moment — §5's "reads what is TYPED" — because the
/// file lags the keyboard by [`CONF_SAVE_WAIT`], so a test that read the file would report the health
/// of the address the box had before this one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestRow {
    pub name: &'static str,
    pub typed: String,
    pub phase: Phase,
}

impl TestRow {
    /// A button for `name`, untouched.
    pub fn new(name: &'static str) -> Self {
        Self { name, typed: String::new(), phase: Phase::Idle }
    }

    /// The press: remember what was typed, drop any earlier verdict, start spinning. An old ✓ beside
    /// a box that has since been edited would be a lie about the address now in it.
    pub fn press(&mut self, typed: &str) {
        self.typed = typed.to_string();
        self.phase = Phase::Running;
    }

    /// The answer arrived, becoming the row's tooltip.
    pub fn finish(&mut self, verdict: Result<String, String>) {
        self.phase = Phase::Finished(verdict);
    }

    /// Whether the spinner is showing.
    pub fn spinning(&self) -> bool {
        matches!(self.phase, Phase::Running)
    }

    /// The ✓ / ✗ mark, or nothing while idle or spinning.
    pub fn mark(&self) -> Option<char> {
        match &self.phase {
            Phase::Finished(Ok(_)) => Some('✓'),
            Phase::Finished(Err(_)) => Some('✗'),
            _ => None,
        }
    }

    /// The tooltip: the verdict, which is the whole reason the mark is not enough (§5's "verdict as
    /// tooltip") — ✗ says a row failed, only its sentence says which of the four reasons it failed.
    pub fn tooltip(&self) -> Option<String> {
        match &self.phase {
            Phase::Finished(Ok(detail)) | Phase::Finished(Err(detail)) => Some(detail.clone()),
            _ => None,
        }
    }

    /// The line this row's verdict puts in the main log; nothing while spinning or idle.
    pub fn log(&self) -> Option<String> {
        match &self.phase {
            Phase::Finished(verdict) => Some(log_line(self.name, verdict)),
            Phase::Running | Phase::Idle => None,
        }
    }
}
