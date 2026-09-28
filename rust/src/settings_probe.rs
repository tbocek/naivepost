//! The provider the Settings dialog runs on when there is a real machine under it: every row's
//! probe goes over HTTP through [`crate::server_leg`] to the address named in that row's box, and
//! the verdict comes from [`crate::checks`]'s own sentences.
//!
//! Why a separate module rather than a closure inside `ui/settings.rs`: the dialog file is at its
//! line ceiling and §5 keeps the *wording* of each verdict in `checks`, so what lives here is only
//! the routing — which kind, which server, which reply field turns into which verdict. Nothing in
//! this file decides whether a service is healthy; it asks, times, records, and hands the answer to
//! the function that already owns the sentence.
//!
//! The typed value wins over the saved one (§5: "each Test reads what is TYPED"; the file lags the
//! keyboard by [`checks::CONF_SAVE_WAIT`]), but the key does not: keys are read from the settings
//! file, because echoing a credential back out of a widget to put it on the wire would be a
//! different security story from reading the 0600 file the user saved it in.

use std::rc::Rc;

use crate::checks::{self, SdCaps, SdProbe};
use crate::services::{self, Endpoint, Kind, Server};
use crate::settings::Conf;
use crate::ui::settings::Provider;

/// The provider a real session installs: the three HTTP rows go out over the wire through
/// [`probe`]; every other row keeps `fallback`. Composed HERE rather than inside
/// `ui::settings::ask`, because a canned provider must answer every row it is asked — a
/// hard-coded HTTP branch in the dialog would send a test's LLM row to port 8731 and replace its
/// verdict with a connection error.
pub fn live_over(fallback: Provider) -> Provider {
    Rc::new(move |key: &str, typed: &str| match key {
        "llm" | "audio" | "sd" => probe(key, typed),
        _ => fallback(key, typed),
    })
}

/// The provider a real session installs with nothing else wired: the HTTP rows dial out, the rest
/// say they were never connected ([`crate::ui::settings::no_probe`]).
pub fn live_provider() -> Provider {
    live_over(crate::ui::settings::no_probe())
}

/// One row's probe, written out per row so each says which call it made.
///
/// `typed` is the address in the box; `key` picks the row. A row whose probe needs two calls (the
/// audio rows need `/health` and `/v1/models`) makes both, because one without the other would
/// green-light a server that cannot do the job the row claims to prove.
pub fn probe(key: &str, typed: &str) -> Result<String, String> {
    let conf = current_conf();
    // No project open means nowhere to write `requests.tsv`; the probe still runs, since a
    // down server is worth knowing about before a project exists. The timings just go unrecorded.
    let tree = crate::layout::Tree::new(crate::startup::session_dir(
        &std::env::current_dir().unwrap_or_default(),
    ))
    .ok();

    match key {
        "llm" => {
            let endpoint = llm_at(&conf, typed);
            let body = ask(&tree, Server::Llm, &endpoint, Kind::ListModels, "models", None)?;
            Ok(checks::llm_verdict(
                conf.model.as_str(),
                0.0,
                summarise(&body).as_str(),
            ))
        }
        "audio" => {
            let endpoint = audio_at(&conf, typed);
            ask(&tree, Server::Audio, &endpoint, Kind::Health, "health", None)?;
            let models = services::parse_models(
                &ask(&tree, Server::Audio, &endpoint, Kind::ListModels, "models", None)?,
            );
            Ok(checks::audio_health_verdict(0, &models)?)
        }
        "sd" => {
            let endpoint = image_at(&conf, typed);
            match ask(&tree, Server::Image, &endpoint, Kind::Capabilities, "capabilities", None) {
                Ok(body) => {
                    let caps = sd_caps(&body);
                    Ok(checks::sd_verdict(Ok(&caps))?)
                }
                Err(_) => {
                    // The two failures are fixed differently, so tell them apart rather than
                    // reporting one word for both: an OpenAI-shaped tenant answers /v1/models and
                    // not the sdcpp API; nothing answering is simply not up.
                    let openai_only = ask(&tree, Server::Image, &endpoint, Kind::ListModels, "models", None)
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
        other => Err(format!("no live probe for {other:?} -- that row is not an HTTP service")),
    }
}

/// Ask one server something for a settings probe and return its body.
///
/// With no project open there is nowhere to append `requests.tsv`, so the leg is called directly
/// and the row dropped — a down server is still worth knowing about before a project exists.
/// Ask one server something for a settings probe and return its body.
///
/// With no project open there is nowhere to append `requests.tsv`, so the leg is called directly
/// and the row dropped — a down server is still worth knowing about before a project exists.
fn ask(
    tree: &Option<crate::layout::Tree>,
    server: Server,
    endpoint: &Endpoint,
    kind: Kind,
    model: &str,
    body: Option<&str>,
) -> Result<String, String> {
    ask_job(tree, server, endpoint, kind, None, model, body)
}

/// [`ask`] with the sd.cpp job id a poll or cancel acts on. Settings probes never pass one today;
/// the parameter exists so the job-scoped kinds reach the same leg the flows use, not a copy of it.
fn ask_job(
    tree: &Option<crate::layout::Tree>,
    server: Server,
    endpoint: &Endpoint,
    kind: Kind,
    job: Option<&str>,
    model: &str,
    body: Option<&str>,
) -> Result<String, String> {
    let base = crate::server_leg::base_row(server, model, kind.method(), "", "settings", "");
    match tree {
        Some(tree) => {
            crate::server_leg::call(tree, &base, server, endpoint, kind, job, body, &never).1
        }
        None => crate::server_leg::send(server, endpoint, kind, job, body, &never),
    }
    .map(|reply| reply.body)
}

/// End-of-run housekeeping: unload every audio model, quietly, off-thread, best-effort.
///
/// Takes the [`Kind`] [`runqueue::end_run`] handed over, so the run says what it wants unloaded and
/// this decides how. §1: called at the end of EVERY run, even one that used no audio, with a 20 s
/// ceiling ([`services::timeout_for`] on [`Kind::UnloadAll`]), and it must never fail the run that
/// made it — hence a thread whose only other job is to say the outcome where an operator can read
/// it. Leaving weights loaded would be memory nothing is waiting on any more.
pub fn fire_unload(kind: Kind) {
    if kind != Kind::UnloadAll {
        return;
    }
    std::thread::spawn(|| {
        let conf = current_conf();
        let endpoint = Endpoint {
            url: services::server_url(&conf.audio_server).unwrap_or_else(services::audio_default),
            key: conf.audio_key.clone(),
        };
        let tree = crate::layout::Tree::new(crate::startup::session_dir(
            &std::env::current_dir().unwrap_or_default(),
        ))
        .ok();
        // Best-effort: the result chooses a log line and nothing else. No `?`, no retry, no wait.
        match ask(&tree, Server::Audio, &endpoint, Kind::UnloadAll, "unload", None) {
            Ok(_) => crate::ui::window::note_unload("models unloaded"),
            Err(why) => crate::ui::window::note_unload(&format!("unload skipped: {why}")),
        }
    });
}

/// The run's cancel context as seen from the settings dialog: nothing cancels a probe.
fn never() -> bool {
    false
}

/// The settings as they are ON DISK right now — re-read per request, which is what makes a change
/// land on the next call rather than the next launch (§1).
fn current_conf() -> Conf {
    crate::settings::from_environment()
        .and_then(|paths| crate::settings::read(&paths).ok())
        .unwrap_or_default()
}

/// The LLM endpoint with the TYPED address overriding the file's, keeping the saved key.
fn llm_at(conf: &Conf, typed: &str) -> Endpoint {
    Endpoint {
        url: services::server_url(typed).unwrap_or_else(services::llm_default),
        key: conf.key.clone(),
    }
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

/// The weights sd-server names, out of its capabilities reply.
fn sd_caps(body: &str) -> SdCaps {
    #[derive(serde::Deserialize)]
    struct Caps {
        #[serde(default)]
        weights: String,
        #[serde(default)]
        model: String,
    }
    let parsed = serde_json::from_str::<Caps>(body).unwrap_or(Caps {
        weights: String::new(),
        model: String::new(),
    });
    SdCaps {
        weights: if parsed.weights.is_empty() {
            parsed.model
        } else {
            parsed.weights
        },
    }
}

/// A reply shortened enough to sit in a tooltip.
fn summarise(body: &str) -> String {
    let one = body.lines().next().unwrap_or_default().trim();
    if one.len() > 80 {
        format!("{}…", &one[..80])
    } else {
        one.to_string()
    }
}
