//! §02-services.md §5 — the Settings rows that do not ride the LLM/audio/sd HTTP path: the LLM's two
//! completion tests and the seven rows [`crate::settings_probe`] used to leave unanswered.
//!
//! Same rule as `settings_probe`: nothing here decides what a healthy service means. Every verdict is
//! the sentence the function in [`crate::checks`] that owns it returns; this file only makes the call —
//! one HTTP request, or one subprocess with a bounded wait — and hands over what came back. A row that
//! cannot run says so in `checks`' words, never in a stub's.
//!
//! WHY the LLM row sends a completion instead of `GET /v1/models`: §5 keeps that endpoint for the two
//! list jobs (`checks::LIST_MODELS_LLM_USES` — Fetch models, and sd.cpp's fallback). A model that
//! lists itself and answers nothing is exactly the failure the row exists to catch.

use std::time::{Duration, Instant};

use crate::checks;
use crate::services::{self, Endpoint, Kind, Server};
use crate::settings::Conf;
use crate::subprocess;

/// A subprocess that outlives this is a hung dialog, and a Test button has no ⏹ of its own — the
/// cancel context only reaches sockets. ffmpeg's own listings and firefox's `--version` are both
/// instant on a machine that has them, so a minute means it is stuck.
const SUBPROCESS_WAIT: Duration = Duration::from_secs(60);
/// One headless search rung: the same slice the pipeline's ladder gives a rung (§2's 45 s over three).
fn search_slice() -> Duration {
    crate::web_browser::ladder_slice()
}

/// What a row's dial looks like: hand it a body and the whole-call ceiling §9 gives that probe, get the
/// reply back. Borrowed lifetime `'a` rather than `'static`: the closure borrows the conf and the typed
/// address of the press that made it.
pub type Ask<'a> = dyn Fn(&str, Option<u64>) -> Result<String, String> + 'a;

/// The LLM row: ONE completion at [`crate::model_list::probes`]' first entry — `max_tokens` 16, thinking
/// forced off, 60 s — timed for real.
///
/// The body is [`checks::llm_test_body`], so the switch that keeps a thinking model from spending the
/// sixteen tokens on reasoning is the same one the pipeline sends, and a test can read the shape off
/// that function rather than from a copy here. The numbers the row REPORTS and the ceiling it dials with
/// both come from [`crate::model_list::probes`]: this call used to print "answered in 60.0 s" whatever the
/// server took, which is §1's budget wearing §5's sentence — a person reading the log learns the timeout,
/// not the model.
pub fn llm_row(conf: &Conf, ask: &Ask<'_>) -> Result<String, String> {
    let model = conf.model.trim();
    if model.is_empty() {
        return Err("no model id -- the box is empty, so there is nothing to ask. Use \"Fetch models\" \
                    to pick one this server lists"
            .to_string());
    }
    let probe = crate::model_list::probes()[0];
    let body = checks::llm_test_body(model).to_string();
    let began = Instant::now();
    let reply = ask(&body, Some(probe.seconds))?;
    let seconds = elapsed_seconds(began);
    let content = crate::llm_request::reply_from("application/json", &reply)
        .map(|reply| reply.content)
        .unwrap_or_default();
    // An answer of nothing is a failure, not a green tick beside an empty quote: F0.13 reports what came
    // back, and "nothing came back" is the thing worth reading.
    crate::model_list::probe_passes(&probe, &content)?;
    // F0.13's sentence is "<model> answered in X s: “ok”" — the MODEL'S WORDS quoted, not a report about
    // them. `probe_passes` above decides pass/fail (blank is a failure); its "answered …" phrasing is this
    // row's own business and stays here.
    Ok(checks::llm_verdict(model, seconds, &content))
}

/// How long a probe took, in the seconds [`checks::llm_verdict`] prints.
fn elapsed_seconds(began: Instant) -> f64 {
    // `as_millis` then a divide rather than `as_secs_f64`'s full precision: the log reads "answered in
    // 1.2 s", and a nanosecond-precise duration would only make the assertion noisy.
    u64::try_from(began.elapsed().as_millis()).unwrap_or(u64::MAX) as f64 / 1000.0
}

/// The LLM vision row: the 48 px red square as a data URL, [`crate::model_list::probes`]' second entry
/// (120 s, no token cap of its own), and it passes only when the reply says "red".
///
/// No `max_tokens`: the table says the vision probe sets none, because the answer is a colour word and
/// capping an image prompt cuts the answer off inside the reasoning. This row used to build its body from
/// [`checks::llm_test_body`] and swap the messages, which quietly sent the text probe's 16-token cap with
/// a picture — enough to spend on reasoning and answer nothing about a model that saw the square fine.
pub fn vision_row(conf: &Conf, ask: &Ask<'_>) -> Result<String, String> {
    let model = conf.model.trim();
    if model.is_empty() {
        return Err("no model id -- the vision test asks the model in the Model box".to_string());
    }
    let probe = crate::model_list::probes()[1];
    let mut body = checks::llm_test_body(model);
    // Drop the text probe's cap rather than inherit it: `Probe.max_tokens` is `None` here by §5's rule.
    if let Some(cap) = probe.max_tokens {
        body["max_tokens"] = serde_json::json!(cap);
    } else if let Some(map) = body.as_object_mut() {
        map.remove("max_tokens");
    }
    body["messages"] = serde_json::json!([{
        "role": "user",
        "content": [
            { "type": "text", "text": "What colour is this image? Answer with the colour alone." },
            { "type": "image_url", "image_url": { "url": checks::vision_probe_url() } },
        ],
    }]);
    let reply = ask(&body.to_string(), Some(probe.seconds))?;
    let content = crate::llm_request::reply_from("application/json", &reply)
        .map(|reply| reply.content)
        .unwrap_or_default();
    checks::vision_verdict(&content)
}

/// One of the four audio.cpp model rows (TTS/ASR/diarization/separation): the id in the box against
/// the server's own catalogue, checking the DECLARED TASK. `task` comes from [`checks::AUDIO_ROWS`].
pub fn audio_model_row(
    models: &[services::AudioModel],
    typed_id: &str,
    task: &str,
    step: &str,
) -> Result<String, String> {
    let wanted = typed_id.trim();
    if wanted.is_empty() {
        return Err(format!(
            "no id in the box -- the server lists: {}",
            catalogue_ids(models)
        ));
    }
    checks::model_verdict(models, wanted, services::Need { task, step })
}

/// The forced-aligner row, which has no box of its own to trust: alignment is asked for by TASK, so
/// the catalogue is the whole answer and a name typed here could only disagree with the server.
pub fn aligner_row(models: &[services::AudioModel], configured: &str) -> Result<String, String> {
    checks::aligner_verdict(configured, models)
}

/// The ffmpeg row: which binary, that ffprobe is beside it, then what the build has. Every list is
/// read by running THAT binary (`-version`, `-filters`, `-encoders`), so the verdict describes the
/// build the pipeline will actually shell out to, not the one on PATH.
pub fn ffmpeg_row(typed: &str) -> Result<String, String> {
    let Some(program) = resolve(typed) else {
        return checks::ffmpeg_verdict(None, false, "", None, None);
    };
    let shown = program.display().to_string();
    let probe = services::ffprobe_for(&shown, "ffprobe");
    let ffprobe_beside = std::path::Path::new(&probe).is_file() || resolve_on_path("ffprobe").is_some();
    let version = run(&shown, &["-version"]).unwrap_or_default();
    // A listing that will not come is reported as nothing checked: `ffmpeg_verdict` answers "built
    // without X" from `None`, which is the honest reading of a binary that would not print it.
    let filters = run(&shown, &["-hide_banner", "-filters"]).ok();
    let encoders = run(&shown, &["-hide_banner", "-encoders"]).ok();
    checks::ffmpeg_verdict(
        Some(&shown),
        ffprobe_beside,
        &version,
        filters.as_deref(),
        encoders.as_deref(),
    )
}

/// The firefox row: `off` is a pass, else that it runs (`--version`) AND that a real headless search
/// came back — a browser that prints its version and cannot be driven is the failure a version check
/// cannot see, and it shows up minutes later as a model writing with no sources.
pub fn firefox_row(typed: &str) -> Result<String, String> {
    if typed.trim().eq_ignore_ascii_case(checks::FIREFOX_OFF) {
        return checks::firefox_verdict(typed, None, None);
    }
    let Some(program) = resolve(typed) else {
        return checks::firefox_verdict(typed, None, None);
    };
    let version = run(&program.display().to_string(), &["--version"])?;
    // The program's own search door: one headless launch of the engine's own DuckDuckGo page, hits
    // counted the way the pipeline counts them. No network is a failure of THIS row, said as such.
    let search = crate::web_browser::query_for(&program.display().to_string(), search_slice())("naivepost");
    let hits = search.ok().map(|hits| hits.len());
    checks::firefox_verdict(typed, Some(&version), hits)
}

/// Which binary a box names: a path as typed, else the name looked up on PATH, else the shipped name on
/// PATH. `None` only when the box is empty, which each row's verdict turns into its missing sentence.
pub fn resolve(typed: &str) -> Option<std::path::PathBuf> {
    let wanted = typed.trim();
    if wanted.is_empty() {
        return None;
    }
    let named = std::path::PathBuf::from(wanted);
    if named.is_file() {
        return Some(named);
    }
    // A bare command in the box means "find it on PATH", not "a file under the working folder".
    resolve_on_path(wanted)
}

/// A bare name on `PATH`. `web_browser` has its own version of this because firefox is also looked
/// for by three names and in the install folders; ffmpeg has exactly one, so this is three lines.
fn resolve_on_path(name: &str) -> Option<std::path::PathBuf> {
    let path = std::env::var("PATH").ok()?;
    std::env::split_paths(&path).find_map(|dir| {
        let candidate = dir.join(name);
        candidate.is_file().then_some(candidate)
    })
}

/// One subprocess, bounded: [`subprocess::spawn_piped`] so a wedged binary can be killed rather than
/// being waited on forever, and the deadline is the caller's [`SUBPROCESS_WAIT`].
fn run(program: &str, args: &[&str]) -> Result<String, String> {
    let mut child = subprocess::spawn_piped(program, args)
        .map_err(|err| format!("{} failed: {err}", subprocess::shown(program, args)))?;
    let deadline = Instant::now() + SUBPROCESS_WAIT;
    loop {
        match child.try_wait().map_err(|err| err.to_string())? {
            Some(status) => {
                let out = child.wait_with_output().map_err(|err| err.to_string())?;
                let text = String::from_utf8_lossy(&out.stdout).into_owned();
                if status.success() {
                    return Ok(text);
                }
                return Err(subprocess::failure(
                    program,
                    args,
                    &String::from_utf8_lossy(&out.stderr),
                ));
            }
            // Past the deadline the child is killed, and the row says it could not be asked.
            None if Instant::now() >= deadline => {
                let _ = child.kill();
                return Err(format!("{} did not answer in {} s", subprocess::shown(program, args), SUBPROCESS_WAIT.as_secs()));
            }
            None => std::thread::sleep(Duration::from_millis(20)),
        }
    }
}

/// The catalogue's ids, for a refusal that names what IS there.
fn catalogue_ids(models: &[services::AudioModel]) -> String {
    let mut ids: Vec<&str> = models.iter().map(|model| model.id.as_str()).collect();
    ids.sort();
    ids.join(", ")
}

/// The endpoint a row dials: the TYPED address wins over the file, the key comes from the file.
pub fn llm_endpoint(conf: &Conf, typed: &str) -> Endpoint {    Endpoint {
        url: services::server_url(typed).unwrap_or_else(services::llm_default),
        key: conf.key.clone(),
    }
}

/// audio.cpp's endpoint: the FILE's address, because §5 puts an id in this box and the address in the
/// row above it — the four model rows and the aligner row type an id, so dialling what was typed would
/// dial the id as a host. The address in the Server box (or the shipped default) is what gets probed.
pub fn audio_endpoint(conf: &Conf, typed: &str) -> Endpoint {
    let _ = typed;
    Endpoint {
        url: services::server_url(&conf.audio_server).unwrap_or_else(services::audio_default),
        key: conf.audio_key.clone(),
    }
}

/// The session folder a settings probe records its timings into, `None` when there is no project open —
/// the same rule [`crate::settings_probe::probe`] states: a down server is worth knowing about before a
/// project exists, the `requests.tsv` line is simply not written.
pub fn probe_tree() -> Option<crate::layout::Tree> {
    crate::layout::Tree::new(crate::startup::session_dir(
        &std::env::current_dir().unwrap_or_default(),
    ))
    .ok()
}

/// Build the chat caller a row sends through: the same leg the flows use, so ⏹ reaches it and
/// `requests.tsv` records it. The body is the caller's — [`llm_row`] and [`vision_row`] build it, and each
/// hands over the ceiling its [`crate::model_list::Probe`] states so §9's 60 s and 120 s bound the socket
/// rather than sitting in a comment.
pub fn chat_ask<'a>(
    conf: &'a Conf,
    typed: &'a str,
) -> impl Fn(&str, Option<u64>) -> Result<String, String> + 'a {
    let endpoint = llm_endpoint(conf, typed);
    let model = conf.model.clone();
    move |body: &str, budget: Option<u64>| {
        crate::settings_probe::chat_post(&probe_tree(), &endpoint, &model, body, budget)
    }
}

/// The catalogue audio.cpp serves, for the four model rows and the aligner row: `GET /v1/models`,
/// parsed by the same reader the run uses so a row cannot green-light a catalogue the run would fail.
pub fn audio_catalogue(conf: &Conf, typed: &str) -> Result<Vec<services::AudioModel>, String> {
    let endpoint = audio_endpoint(conf, typed);
    let body = crate::settings_probe::get(
        &probe_tree(),
        Server::Audio,
        &endpoint,
        Kind::ListModels,
        "models",
        None,
    )?;
    Ok(services::parse_models(&body))
}
