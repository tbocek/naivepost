//! The provider the Settings dialog runs on when there is a real machine under it: every row's
//! probe goes over HTTP through [`crate::server_leg`] to the address named in that row's box, and
//! the verdict comes from [`crate::checks`]'s own sentences.

use crate::checks::{self, SdProbe};
use crate::services::{self, Endpoint, Kind, Server};
use crate::settings::Conf;
use crate::settings_probe_rows;
use crate::ui::settings::Provider;

pub fn live_over(fallback: Provider) -> Provider {
    std::sync::Arc::new(move |key: &str, typed: &str| match key {
        "llm" | "llm-vision" | "decision" | "tts" | "asr" | "diar" | "sep" | "aligner" => {
            probe(key, typed)
        }
        "ffmpeg" => settings_probe_rows::ffmpeg_row(typed),
        "firefox" => settings_probe_rows::firefox_row(typed),
        // §9's list job. Not the fallback's to answer: a real session pressing Fetch models has to get
        // `GET /v1/models` on the wire, which is what fills the dropdown and gives `model_list`'s parse,
        // verdict and Use rules a path a person can actually reach.
        crate::ui::settings::FETCH_KEY => fetch_models(typed),
        "audio" | "sd" => probe(key, typed),
        _ => fallback(key, typed),
    })
}

/// Fetch models: ask the server in the Server box what it serves, and hand the body back for
/// [`crate::model_list`] to read. The dialog does the widget half (dropdown, badge, log line); this is the
/// socket half that was missing.
pub fn fetch_models(typed: &str) -> Result<String, String> {
    let endpoint = settings_probe_rows::llm_endpoint(&current_conf(), typed);
    crate::model_list_fetch::models_ask(&endpoint, &crate::web_request_log::session_tree())
}

/// The provider a real session installs with nothing else wired: the HTTP rows dial out, the rest
/// say they were never connected ([`crate::ui::settings::no_probe`]).
pub fn live_provider() -> Provider {
    live_over(crate::ui::settings::no_probe())
}

/// The four audio.cpp rows as (settings key, declared task): the key is the first word of the row's label
/// in lowercase, and the task is [`checks::AUDIO_ROWS`]' own pair, so the dialog's list, the probe below
/// and a test asking "which task does the TTS row check?" all read one table — the TTS row is `clon`.
pub const AUDIO_ROW_TASKS: [(&str, &str); 4] = [
    ("tts", "clon"),
    ("asr", "asr"),
    ("diar", "diar"),
    ("sep", "sep"),
];

/// The (label, task) pair behind one row key, or `None` for a key that is not an audio row.
fn audio_row_pair(key: &str) -> Option<(&'static str, &'static str)> {
    let (_, task) = AUDIO_ROW_TASKS
        .into_iter()
        .find(|(row_key, _)| *row_key == key)?;
    checks::AUDIO_ROWS
        .into_iter()
        .find(|(_, row_task)| *row_task == task)
}

/// One row's probe, written out per row so each says which call it made.
pub fn probe(key: &str, typed: &str) -> Result<String, String> {
    let conf = current_conf();
    // No project open means nowhere to write `requests.tsv`; the probe still runs, since a
    // down server is worth knowing about before a project exists. The timings just go unrecorded.
    let tree = crate::layout::Tree::new(crate::startup::open_dir()).ok();

    match key {
        "llm" | "llm-vision" => {
            // The row's own box is a MODEL id (the Model row; the Vision row, empty for "the model above"),
            // never an address: it names the model asked, and the Server box names where.
            let mut conf = conf;
            if !typed.trim().is_empty() {
                conf.model = typed.trim().to_string();
            }
            let ask = settings_probe_rows::chat_ask(&conf);
            if key == "llm" {
                // §5: ONE completion, `max_tokens` 16, thinking forced off, at [`checks::LLM_TEST_SECONDS`].
                let verdict = settings_probe_rows::llm_row(&conf, &ask)?;
                // The same `/health` count the parallel steps go by, so the row says what they will use.
                let endpoint = services::llm_endpoint(&conf);
                let health = get(&tree, Server::Llm, &endpoint, Kind::Health, "health", None);
                Ok(
                    match health
                        .ok()
                        .as_deref()
                        .and_then(crate::rows_at_once::slots_in)
                    {
                        Some(slots) => format!("{verdict} \u{b7} {slots} slot(s)"),
                        None => format!("{verdict} \u{b7} no slot count, one request at a time"),
                    },
                )
            } else {
                settings_probe_rows::vision_row(&conf, &ask)
            }
        }
        "decision" => {
            // The Decision box names the model; empty, the gate asks the Model box's ([`Conf::decider`]).
            let mut conf = conf;
            conf.decision_model = typed.trim().to_string();
            conf.model = conf.decider();
            let ask = settings_probe_rows::chat_ask(&conf);
            let paths = crate::settings::from_environment();
            crate::text_gate_call::probe(&conf, paths.as_ref(), &ask)
        }
        "tts" | "asr" | "diar" | "sep" => {
            let models = settings_probe_rows::audio_catalogue(&conf)?;
            let (label, task) = audio_row_pair(key).unwrap_or(("the model row", key));
            settings_probe_rows::audio_model_row(&models, typed, task, label)
        }
        "aligner" => {
            let models = settings_probe_rows::audio_catalogue(&conf)?;
            settings_probe_rows::aligner_row(&models, typed)
        }
        "ffmpeg" => settings_probe_rows::ffmpeg_row(typed),
        "firefox" => settings_probe_rows::firefox_row(typed),
        "audio" => {
            let endpoint = audio_at(&conf, typed);
            // §5's sentence carries a number: "healthy in N ms". The health call is timed here rather
            // than assumed, so the row reports what this address actually took to answer.
            let began = std::time::Instant::now();
            get(
                &tree,
                Server::Audio,
                &endpoint,
                Kind::Health,
                "health",
                None,
            )?;
            let health_ms = u64::try_from(began.elapsed().as_millis()).unwrap_or(u64::MAX);
            let models = services::parse_models(&get(
                &tree,
                Server::Audio,
                &endpoint,
                Kind::ListModels,
                "models",
                None,
            )?);
            Ok(checks::audio_health_verdict(health_ms, &models)?)
        }
        "sd" => {
            let endpoint = image_at(&conf, typed);
            match get(
                &tree,
                Server::Image,
                &endpoint,
                Kind::Capabilities,
                "capabilities",
                None,
            ) {
                Ok(body) => checks::sd_verdict(Ok(&checks::sd_caps(&body)?)),
                Err(_) => {
                    // The two failures are fixed differently, so tell them apart rather than
                    // reporting one word for both: an OpenAI-shaped tenant answers /v1/models and
                    // not the sdcpp API; nothing answering is simply not up.
                    let openai_only = get(
                        &tree,
                        Server::Image,
                        &endpoint,
                        Kind::ListModels,
                        "models",
                        None,
                    )
                    .is_ok();
                    let probe = if openai_only {
                        SdProbe::OpenAiOnly
                    } else {
                        SdProbe::Unreachable
                    };
                    Err(checks::sd_verdict(Err(probe)).unwrap_err())
                }
            }
        }
        other => Err(format!(
            "no live probe for {other:?} -- that row is not an HTTP service"
        )),
    }
}

/// Ask one server something for a settings probe and return its body.
pub fn get(
    tree: &Option<crate::layout::Tree>,
    server: Server,
    endpoint: &Endpoint,
    kind: Kind,
    model: &str,
    body: Option<&str>,
) -> Result<String, String> {
    let base = crate::server_leg::base_row(server, model, kind.method(), "", "settings", "");
    let cancelled = crate::cancel_leg::cancel_check_now();
    // A budget only bites when nothing else bounds the call: `send_budget` falls back to §1's table,
    // and the recorded path goes through `call`, which carries no per-call ceiling of its own.
    let reply = match tree {
        Some(tree) => {
            crate::server_leg::call(tree, &base, server, endpoint, kind, None, body, &cancelled).1
        }
        None => {
            crate::server_leg::send_budget(server, endpoint, kind, None, body, None, &cancelled)
        }
    }?;
    if !reply.ok() {
        return Err(format!("{} answered {}", kind.path(None), reply.status));
    }
    Ok(reply.body)
}

/// A chat completion from the settings dialog: the LLM and vision rows (§5 — one completion, `max_tokens`
/// 16, thinking off; the picture is in the body the caller already built).
pub fn chat_post(
    tree: &Option<crate::layout::Tree>,
    endpoint: &Endpoint,
    model: &str,
    body: &str,
    budget: Option<u64>,
) -> Result<String, String> {
    // F0.13's two probes each carry a ceiling (§9: 60 s for the completion, 120 s for the picture), and
    // only the socket can enforce it — §1 leaves an ordinary chat call unbounded on purpose.
    let budget = budget.map(std::time::Duration::from_secs);
    ask_job(
        tree,
        Server::Llm,
        endpoint,
        Kind::Chat,
        None,
        model,
        Some(body),
        budget,
    )
}

/// [`ask_job`] with the sd.cpp job id a poll or cancel acts on. Settings probes never pass one today;
/// The parameter stays so the job-scoped kinds reach the same leg the flows use, not a copy of it.
#[allow(clippy::too_many_arguments)] // one leg's whole request: which server, endpoint, kind, job, model,
                                     // body and per-call ceiling the caller already holds; a struct here would only rename them.
fn ask_job(
    tree: &Option<crate::layout::Tree>,
    server: Server,
    endpoint: &Endpoint,
    kind: Kind,
    job: Option<&str>,
    model: &str,
    body: Option<&str>,
    budget: Option<std::time::Duration>,
) -> Result<String, String> {
    let base = crate::server_leg::base_row(server, model, kind.method(), "", "settings", "");
    let cancelled = crate::cancel_leg::cancel_check_now();
    // A budget only bites when nothing else bounds the call: `send_budget` falls back to §1's table,
    // and the recorded path goes through `call`, which carries no per-call ceiling of its own.
    let reply = match tree {
        Some(tree) => {
            crate::server_leg::call(tree, &base, server, endpoint, kind, job, body, &cancelled).1
        }
        None => {
            crate::server_leg::send_budget(server, endpoint, kind, job, body, budget, &cancelled)
        }
    };
    let reply = reply?;
    // A non-2xx is the server's refusal, said with its own words: handed on as a body it read as a reply with
    // nothing in it, and a row reported "answered without judge_text" for a model that never answered at all.
    if !reply.ok() {
        let said: String = reply.body.trim().chars().take(300).collect();
        return Err(format!("{model} answered {}: {said}", reply.status));
    }
    Ok(reply.body)
}

/// End-of-run housekeeping: unload every audio model, quietly, off-thread, best-effort.
pub fn fire_unload(kind: Kind) {
    if kind != Kind::UnloadAll {
        return;
    }
    // F0.5 S4 / F0.3 S4: opening the housekeeping leg IS the hand-off — the run that ended does
    // not wait on a server, and this thread owns the request from here.
    let handle = std::thread::spawn(|| {
        // F0.3 S3: this leg rides the cancel context too — a ⏹ while it waits means the user wants
        // nothing more from the servers, so the wait is abandoned and nothing is dialled.
        if crate::cancel_leg::cancelled() {
            return;
        }
        // Counted so [`crate::cancel_leg::await_stop`], which a stopping run calls before it walks
        // away, knows whether there is anything left out on the wire to give up on.
        crate::cancel_leg::leg_started();
        let conf = current_conf();
        let endpoint = Endpoint {
            url: services::server_url(&conf.audio_server).unwrap_or_else(services::audio_default),
            key: conf.audio_key.clone(),
        };
        let tree = crate::layout::Tree::new(crate::startup::open_dir()).ok();
        // Best-effort: the result chooses a log line and nothing else. No `?`, no retry, no wait.
        match get(
            &tree,
            Server::Audio,
            &endpoint,
            Kind::UnloadAll,
            "unload",
            None,
        ) {
            Ok(_) => crate::ui::window::note_unload("models unloaded"),
            Err(why) => crate::ui::window::note_unload(&format!("unload skipped: {why}")),
        };
        crate::cancel_leg::leg_finished();
    });
    *UNLOADING.lock().unwrap() = Some(handle);
}

/// The end-of-run housekeeping thread, kept so a stop can say whether one is still out there rather
/// than leaving it uncounted and invisible.
static UNLOADING: std::sync::Mutex<Option<std::thread::JoinHandle<()>>> =
    std::sync::Mutex::new(None);

/// Wait for the end-of-run unload to land, or give up when ⏹ says to — whichever comes first.
pub fn drain_unload(budget: std::time::Duration) -> crate::cancel_leg::StoppedLegs {
    let stopped = crate::cancel_leg::await_stop(budget);
    // The handle is released either way: once the run is gone nothing downstream waits on this
    // thread, and joining here would put the 20 s ceiling back where §1 asked for none.
    UNLOADING.lock().unwrap().take();
    crate::cancel_leg::StoppedLegs {
        waited: stopped.waited,
        legs: crate::cancel_leg::legs_outstanding(),
    }
}

thread_local! {
    /// The dialog's boxes as typed, while one of its probes runs ([`with_typed`]).
    static TYPED: std::cell::RefCell<Option<Conf>> = const { std::cell::RefCell::new(None) };
}

/// Run `probe` with `typed` as the settings it reads: the dialog's boxes laid over the saved file.
pub fn with_typed<T>(typed: Conf, probe: impl FnOnce() -> T) -> T {
    let before = TYPED.with(|slot| slot.replace(Some(typed)));
    let answer = probe();
    TYPED.with(|slot| *slot.borrow_mut() = before);
    answer
}

/// The settings a probe reads: what the dialog has typed while it asks ([`with_typed`]), else the file
/// as it is ON DISK right now, re-read per request, which is what makes a change land on the next call
/// rather than the next launch (§1).
fn current_conf() -> Conf {
    if let Some(typed) = TYPED.with(|slot| slot.borrow().clone()) {
        return typed;
    }
    crate::settings::from_environment()
        .and_then(|paths| crate::settings::read(&paths).ok())
        .unwrap_or_default()
}

/// audio.cpp's endpoint, same rule, its own key.
fn audio_at(conf: &Conf, typed: &str) -> Endpoint {
    Endpoint {
        url: services::server_url(typed).unwrap_or_else(services::audio_default),
        key: conf.audio_key.clone(),
    }
}

/// sd.cpp's endpoint, same rule, its own key.
fn image_at(conf: &Conf, typed: &str) -> Endpoint {
    Endpoint {
        url: services::server_url(typed).unwrap_or_else(services::image_default),
        key: conf.sd_key.clone(),
    }
}
