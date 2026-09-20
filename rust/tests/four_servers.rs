//! §02-services#1-the-four-servers: where each server lives, which key goes with it, which path
//! a given call takes and how long it may take — plus the two rules that are not about servers at
//! all (the aligner picked by task, ffprobe from ffmpeg's folder).
//!
//! Nothing here opens a socket. The row each call leaves behind is [requests]'s, tested in
//! tests/text_formats_sidecars.rs; what is pinned below is the table a request is built from.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use naivepost::layout::Tree;
use naivepost::requests::{self, Outcome, Request, Service};
use naivepost::services as svc;
use naivepost::settings::{self, Conf, Paths};

/// A settings tree with nothing in it: the state a fresh install is in, where every box is empty.
fn paths(root: &Path) -> Paths {
    let text = |path: PathBuf| path.to_str().expect("utf-8 temp path").to_string();
    settings::paths_from(
        Some(&text(root.join("config"))),
        Some(&text(root.join("home"))),
        Some(&text(root.join("data"))),
        Some(&text(root.join("xdg-data"))),
    )
    .expect("absolute temp paths are a machine")
}

/// A settings file written line by line, so a test states the keys rather than the writer.
fn write_conf_lines(paths: &Paths, lines: &[&str]) -> Result<(), String> {
    let body = lines
        .iter()
        .map(|line| format!("{line}\n"))
        .collect::<String>();
    // Parse it first: a test that writes keys this build cannot read would be asserting nothing.
    let conf = settings::parse(&body).expect("the test's own keys parse");
    settings::save(paths, &conf).map(|_| ())
}

/// Apply a change to the settings file: what a dialog's OK button does.
fn update(paths: &Paths, change: impl FnOnce(&mut Conf)) -> Result<(), String> {
    let mut conf = settings::read(paths)?;
    change(&mut conf);
    settings::save(paths, &conf).map(|_| ())
}

/// The four servers as a fresh install sees them: three loopback URLs and no keys.
#[test]
fn sec_02_services_1_the_four_servers_s1_empty_boxes_mean_loopback() {
    assert_eq!(svc::llm_default(), "http://127.0.0.1:8731");
    assert_eq!(svc::audio_default(), "http://127.0.0.1:8765");
    assert_eq!(svc::image_default(), "http://127.0.0.1:1234");

    let conf = Conf::default();
    fn none(_: &str) -> Option<String> { None }
    assert_eq!(
        svc::llm_endpoint(&conf),
        svc::Endpoint {
            url: "http://127.0.0.1:8731".into(),
            key: String::new()
        }
    );
    assert_eq!(svc::audio_endpoint(&conf, none).url, "http://127.0.0.1:8765");
    assert_eq!(svc::image_endpoint(&conf, none).url, "http://127.0.0.1:1234");

    // http, not https: this is the machine talking to itself.
    for url in [svc::llm_default(), svc::audio_default(), svc::image_default()] {
        assert!(url.starts_with("http://"), "{url}");
    }
}

/// A host with no scheme is read as `https://host`, and the LLM box has no environment override.
#[test]
fn sec_02_services_1_the_four_servers_s2_bare_host_is_https_and_llm_has_no_env_override() {
    assert_eq!(
        svc::server_url("llm.example.com").as_deref(),
        Some("https://llm.example.com")
    );
    // A real URL keeps its own scheme, and the trailing slash goes because a path joins to it.
    assert_eq!(
        svc::server_url(" http://gpu:8765/ ").as_deref(),
        Some("http://gpu:8765")
    );
    // Empty means "not configured", which is how the default gets its turn.
    assert_eq!(svc::server_url("   "), None);

    // Every env var an LLM box could plausibly be overridden by is ignored — `llm_endpoint` does
    // not even take a way to read one, which is §1's "the LLM server has no env override" made
    // unimplementable rather than merely unchecked.
    let mut conf = Conf::default();
    conf.server = "gpu.example.com".into();
    assert_eq!(svc::llm_endpoint(&conf).url, "https://gpu.example.com");
    conf.server.clear();
    assert_eq!(svc::llm_endpoint(&conf).url, svc::llm_default());
}

/// audio.cpp's server: box, then `NAIVEPOST_TTS_URL`, then `AUDIOCPP_SERVER`, then the loopback.
#[test]
fn sec_02_services_1_the_four_servers_s3_audio_server_precedence() {
    let env = |pairs: &[(&str, &str)]| {
        let map: BTreeMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        move |name: &str| map.get(name).cloned()
    };

    // The box wins over both variables: it is the one of them a user can see and clear.
    let mut conf = Conf::default();
    conf.audio_server = "box.example.com".into();
    assert_eq!(
        svc::audio_endpoint(&conf, env(&[
            ("NAIVEPOST_TTS_URL", "http://tts-var:1"),
            ("AUDIOCPP_SERVER", "http://cpp-var:2"),
        ]))
        .url,
        "https://box.example.com"
    );

    // No box: the app's own variable moves this app alone.
    let conf = Conf::default();
    assert_eq!(
        svc::audio_endpoint(&conf, env(&[
            ("NAIVEPOST_TTS_URL", "http://tts-var:1"),
            ("AUDIOCPP_SERVER", "http://cpp-var:2"),
        ]))
        .url,
        "http://tts-var:1"
    );

    // Only audio.cpp's own variable set: one server for both frontends.
    assert_eq!(
        svc::audio_endpoint(&conf, env(&[("AUDIOCPP_SERVER", "http://cpp-var:2")])).url,
        "http://cpp-var:2"
    );

    // Nothing at all: the loopback. A variable set to empty is not a setting.
    assert_eq!(
        svc::audio_endpoint(&conf, env(&[("AUDIOCPP_SERVER", ""), ("NAIVEPOST_TTS_URL", "  ")]))
            .url,
        svc::audio_default()
    );

    // sd.cpp: box, then SD_SERVER, then the loopback.
    let mut conf = Conf::default();
    conf.sd_server = "gpu:1234".into();
    assert_eq!(svc::image_endpoint(&conf, env(&[("SD_SERVER", "http://var")])).url, "https://gpu:1234");
    let conf = Conf::default();
    assert_eq!(svc::image_endpoint(&conf, env(&[("SD_SERVER", "http://var")])).url, "http://var");
    assert_eq!(svc::image_endpoint(&conf, env(&[])).url, svc::image_default());
}

/// Keys travel as `Authorization: Bearer …`, and only when there is one.
#[test]
fn sec_02_services_1_the_four_servers_s4_keys_are_bearer_only_when_non_empty() {
    assert_eq!(svc::authorization("sk-abc").as_deref(), Some("Bearer sk-abc"));
    // Trimmed, so a key pasted with a newline still authenticates.
    assert_eq!(svc::authorization("  sk-abc \n").as_deref(), Some("Bearer sk-abc"));
    // Empty sends no header at all rather than a credential that says "none".
    assert_eq!(svc::authorization(""), None);
    assert_eq!(svc::authorization("   "), None);

    let mut conf = Conf::default();
    conf.key = "  llm-key ".into();
    conf.sd_key = "sd-key".into();
    fn none(_: &str) -> Option<String> { None }
    assert_eq!(svc::llm_endpoint(&conf).key, "  llm-key ");
    assert_eq!(svc::authorization(&svc::llm_endpoint(&conf).key).as_deref(), Some("Bearer llm-key"));
    assert_eq!(svc::audio_endpoint(&conf, none).key, "");
    assert_eq!(svc::audio_endpoint(&conf, none).key.trim(), "");
    assert_eq!(svc::image_endpoint(&conf, none).key, "sd-key");
}

/// §1's API table: method and path for every kind of call.
#[test]
fn sec_02_services_1_the_four_servers_s5_api_table_paths_and_methods() {
    let cases = [
        (svc::Kind::Chat, "POST", "/v1/chat/completions"),
        (svc::Kind::ListModels, "GET", "/v1/models"),
        (svc::Kind::Health, "GET", "/health"),
        (svc::Kind::Upload, "POST", "/v1/ui/upload"),
        (svc::Kind::RunTask, "POST", "/v1/tasks/run"),
        (svc::Kind::Speech, "POST", "/v1/audio/speech"),
        (
            svc::Kind::UnloadAll,
            "POST",
            "/v1/tasks/unload_all_models",
        ),
        (svc::Kind::Capabilities, "GET", "/sdcpp/v1/capabilities"),
        (svc::Kind::SubmitImage, "POST", "/sdcpp/v1/img_gen"),
    ];
    for (kind, method, path) in cases {
        assert_eq!(kind.method(), method, "{kind:?}");
        assert_eq!(kind.path(None), path, "{kind:?}");
    }

    // The two sd.cpp job kinds take the id this request is about.
    assert_eq!(
        svc::Kind::PollImage.path(Some("job-7")).as_str(),
        "/sdcpp/v1/jobs/job-7"
    );
    assert_eq!(
        svc::Kind::CancelImage.path(Some("job-7")).as_str(),
        "/sdcpp/v1/jobs/job-7/cancel"
    );
}

/// Timeouts: none for the work, twenty seconds for housekeeping.
#[test]
fn sec_02_services_1_the_four_servers_s6_timeouts_and_cancel_context() {
    // An hour of audio is an hour of work; a client that gave up at five minutes would fail a
    // job that was going fine.
    let untimed = [
        svc::Kind::Chat,
        svc::Kind::ListModels,
        svc::Kind::Health,
        svc::Kind::Upload,
        svc::Kind::RunTask,
        svc::Kind::Speech,
        svc::Kind::Capabilities,
        svc::Kind::SubmitImage,
        svc::Kind::PollImage,
        svc::Kind::CancelImage,
    ];
    for kind in untimed {
        assert_eq!(svc::timeout_for(kind), None, "{kind:?}");
    }
    // Housekeeping is the exception, and it is best-effort: its failure never fails a run.
    assert_eq!(
        svc::timeout_for(svc::Kind::UnloadAll),
        Some(Duration::from_secs(20))
    );
    assert_eq!(svc::UNLOAD_TIMEOUT, Duration::from_secs(20));
    assert!(svc::UNLOAD_IS_BEST_EFFORT);

    // Every kind rides the cancel context. The prototype left health, model lists, speech,
    // unload and the sd.cpp cancel off it, which is why a ⏹ during a long TTS call left the app
    // waiting for a request it had already given up on.
    for kind in [
        svc::Kind::Chat,
        svc::Kind::ListModels,
        svc::Kind::Health,
        svc::Kind::Upload,
        svc::Kind::RunTask,
        svc::Kind::Speech,
        svc::Kind::UnloadAll,
        svc::Kind::Capabilities,
        svc::Kind::SubmitImage,
        svc::Kind::PollImage,
        svc::Kind::CancelImage,
    ] {
        assert!(svc::rides_cancel_context(kind), "{kind:?}");
    }
}

/// The settings file is re-read per request, so a change lands on the next one.
#[test]
fn sec_02_services_1_the_four_servers_s7_settings_are_reread_per_request() {
    let root = temp_dir("per-request");
    let paths = paths(&root);
    update(&paths, |conf| conf.audio_server = "http://first:8765".into())
        .expect("write first server url");

    fn none(_: &str) -> Option<String> { None }
    assert_eq!(
        svc::endpoint_for(&paths, svc::Server::Audio, &none).url,
        "http://first:8765"
    );

    // Change the file between two requests: nothing is cached, so the next one goes elsewhere.
    update(&paths, |conf| conf.audio_server = "http://second:8765".into())
        .expect("write second server url");
    assert_eq!(
        svc::endpoint_for(&paths, svc::Server::Audio, &none).url,
        "http://second:8765"
    );

    // A model id moves the same way: the read is per request, not per run.
    update(&paths, |conf| conf.asr_model = "whisper-large".into())
        .expect("write asr model");
    let conf = settings::read(&paths).expect("read back");
    assert_eq!(svc::asr_model(&conf), "whisper-large");

    // And a file that cannot be read resolves to the default rather than failing the request.
    std::fs::remove_file(paths.conf_path()).expect("remove conf");
    assert_eq!(
        svc::endpoint_for(&paths, svc::Server::Llm, &none).url,
        svc::llm_default()
    );
}

/// The four model ids, compiled in, and what an empty box means.
#[test]
fn sec_02_services_1_the_four_servers_s8_model_ids_default_then_override() {
    assert_eq!(svc::ASR_MODEL, "nemotron-asr");
    assert_eq!(svc::DIAR_MODEL, "sortformer-diar");
    assert_eq!(svc::TTS_MODEL, "index-tts2");
    assert_eq!(svc::SEP_MODEL, "bs-roformer");

    let conf = Conf::default();
    assert_eq!(svc::asr_model(&conf), "nemotron-asr");
    assert_eq!(svc::diar_model(&conf), "sortformer-diar");
    assert_eq!(svc::tts_model(&conf), "index-tts2");
    assert_eq!(svc::sep_model(&conf), "bs-roformer");

    // AUDIOCPP_ASR_MODEL and friends override; blank is absent, so the default stays.
    let mut conf = Conf::default();
    conf.asr_model = "whisper-large-v3".into();
    conf.diar_model = "   ".into();
    conf.tts_model = "my-voice-model".into();
    assert_eq!(svc::asr_model(&conf), "whisper-large-v3");
    assert_eq!(svc::diar_model(&conf), "sortformer-diar");
    assert_eq!(svc::tts_model(&conf), "my-voice-model");
    assert_eq!(svc::sep_model(&conf), "bs-roformer");
}

/// The aligner is selected by task, and an empty box stays empty.
#[test]
fn sec_02_services_1_the_four_servers_s9_aligner_is_picked_by_task() {
    let model = |id: &str, task: &str| svc::AudioModel {
        id: id.into(),
        family: "qwen3".into(),
        task: task.into(),
    };
    // A server serving two aligners. Sorted plainly, the weaker one comes first by name — which
    // is exactly what the preference exists to prevent.
    let models = vec![
        model("aab-alignment-ctc", "align"),
        model(svc::ALIGN_PREFERENCE, "align"),
        model("nemotron-asr", "asr"),
    ];

    assert_eq!(svc::ALIGN_PREFERENCE, "qwen3-aligner");
    // Empty box: declared aligners only, preference first, the rest by name. The asr model is
    // not offered at all — a row that can be filled wrongly is worse than one left empty.
    assert_eq!(
        svc::pick_aligner("", &models).as_deref(),
        Some("qwen3-aligner")
    );
    let only_weak = vec![model("aab-alignment-ctc", "align"), model("nemotron-asr", "asr")];
    assert_eq!(
        svc::pick_aligner("", &only_weak).as_deref(),
        Some("aab-alignment-ctc")
    );

    // A name in the box wins when the server declares it for align.
    let three = vec![
        model("aab-alignment-ctc", "align"),
        model("qwen3-aligner", "align"),
        model("other-aligner", "align"),
    ];
    assert_eq!(
        svc::pick_aligner("aab-alignment-ctc", &three).as_deref(),
        Some("aab-alignment-ctc")
    );

    // A name the server does not declare is a miss, and nothing else is substituted: guessing
    // another aligner would align through a model the user never chose. Same for one it serves
    // for something other than align.
    assert_eq!(svc::pick_aligner("not-here", &three), None);
    assert_eq!(svc::pick_aligner("nemotron-asr", &models), None);
    // No declared aligner at all is a working setup, not an error to paper over.
    assert_eq!(svc::pick_aligner("", &[model("nemotron-asr", "asr")]), None);
    assert_eq!(svc::pick_aligner("", &[]), None);

    // The catalog parser: `GET /v1/models` answers with `data`, and the task is what the rule
    // keys off. Unparseable is an empty list, which the missing model's own message reports.
    let body = r#"{"object":"list","data":[
        {"id":"nemotron-asr","object":"model","owned_by":"audio.cpp","family":"nemotron","task":"asr"},
        {"id":"qwen3-aligner","object":"model","owned_by":"audio.cpp","family":"qwen3","task":"align"}
    ]}"#;
    let parsed = svc::parse_models(body);
    assert_eq!(parsed.len(), 2);
    assert_eq!(parsed[1].id, "qwen3-aligner");
    assert_eq!(parsed[1].task, "align");
    assert_eq!(svc::parse_models("not json"), Vec::<svc::AudioModel>::new());
}

/// The hint for a model the server does not have: names the server, lists what it serves, and
/// only names an install command for a shipped default.
#[test]
fn sec_02_services_1_the_four_servers_s10_missing_model_names_the_server_and_its_models() {
    let model = |id: &str, task: &str| svc::AudioModel {
        id: id.into(),
        family: String::new(),
        task: task.into(),
    };
    let models = vec![model("sortformer-diar", "diar"), model("nemotron-asr", "asr")];
    let url = "http://127.0.0.1:8765";

    // A shipped default: the message carries the install command for its weights, and lists what
    // the server does serve so the user can see how far off they are.
    let msg = svc::missing_model(url, &models, svc::SEP_MODEL, svc::Need { task: "sep", step: "Prepare" });
    assert!(msg.contains(url), "{msg}");
    assert!(msg.contains("nemotron-asr"), "{msg}");
    assert!(msg.contains("sortformer-diar"), "{msg}");
    // Sorted, so the same server answers the same way twice.
    assert!(
        msg.find("nemotron-asr").unwrap() < msg.find("sortformer-diar").unwrap(),
        "{msg}"
    );
    assert!(
        msg.contains(
            "docker compose exec audio python3 tools/model_manager_v2.py install bs_roformer_q8_0 --models-root models"
        ),
        "{msg}"
    );

    // A hand-picked id gets the same message with no command: its weights would be a guess.
    let msg = svc::missing_model(url, &models, "my-separation-model", svc::Need { task: "sep", step: "Prepare" });
    assert!(msg.contains("my-separation-model"), "{msg}");
    assert!(!msg.contains("model_manager_v2.py"), "{msg}");

    // Present but declared for another task says so — the usual cause is a catalog entry copied
    // from another model with its task left as it was.
    let msg = svc::missing_model(url, &models, "nemotron-asr", svc::Need { task: "sep", step: "Prepare" });
    assert!(msg.contains("task"), "{msg}");
    assert!(msg.contains("\"asr\""), "{msg}");
    assert!(msg.contains("\"sep\""), "{msg}");

    // The three shipped defaults have packages; the TTS model does not get a hint.
    assert_eq!(svc::install_package(svc::ASR_MODEL), Some("nemotron_asr_q8_0"));
    assert_eq!(
        svc::install_package(svc::DIAR_MODEL),
        Some("sortformer_diar_4spk_v1_q8_0")
    );
    assert_eq!(svc::install_package(svc::SEP_MODEL), Some("bs_roformer_q8_0"));
    assert_eq!(svc::install_package(svc::TTS_MODEL), None);
    assert_eq!(svc::install_package("whisper-large-v3"), None);
}

/// ffprobe comes from the configured ffmpeg's own folder, so frames, waveforms, cuts and encodes
/// are all run by one build.
#[test]
fn sec_02_services_1_the_four_servers_s11_ffprobe_comes_from_ffmpegs_folder() {
    // Nothing configured: PATH for both.
    assert_eq!(svc::ffprobe_for("", "/usr/bin/ffprobe"), "/usr/bin/ffprobe");

    // A configured ffmpeg answers with ffprobe from the same folder.
    assert_eq!(
        svc::ffprobe_for("/opt/ffmpeg-7/bin/ffmpeg", "ffprobe"),
        "/opt/ffmpeg-7/bin/ffprobe"
    );
    // A renamed build keeps its suffix, so a versioned pair stays a pair.
    assert_eq!(
        svc::ffprobe_for("/opt/ffmpeg-7/bin/ffmpeg-7.1", "ffprobe"),
        "/opt/ffmpeg-7/bin/ffprobe-7.1"
    );
    // A bare name has no folder to join, and asks for ffprobe on PATH.
    assert_eq!(svc::ffprobe_for("ffmpeg", "ffprobe"), "ffprobe");

    // Never a different build: the sibling of the configured binary is what comes back, whatever
    // happens to be first on PATH.
    let probe = svc::ffprobe_for("/srv/tools/ffmpeg", "ffprobe");
    let probe = Path::new(&probe);
    assert_eq!(probe.parent(), Some(Path::new("/srv/tools")));
    assert_eq!(probe.file_name().unwrap().to_str().unwrap(), "ffprobe");
}

/// Slot counts: one per model, absent meaning 1.
#[test]
fn sec_02_services_1_the_four_servers_s12_slot_counts_are_per_model() {
    // Two models on one server are two slot counts, since a GPU shared between them is not two
    // GPUs — so each task reads its own key, and an absent key is 1.
    let conf = Conf::default();
    assert_eq!(svc::slots_for(&conf, svc::AudioTask::Asr), 1);
    assert_eq!(svc::llm_slots(&conf), 1);

    let mut conf = Conf::default();
    conf.slots.llm = 3;
    conf.slots.asr = 2;
    conf.slots.tts = 4;
    assert_eq!(svc::llm_slots(&conf), 3);
    assert_eq!(svc::slots_for(&conf, svc::AudioTask::Asr), 2);
    assert_eq!(svc::slots_for(&conf, svc::AudioTask::Diar), 1);
    assert_eq!(svc::slots_for(&conf, svc::AudioTask::Tts), 4);
    assert_eq!(svc::slots_for(&conf, svc::AudioTask::Sep), 1);
    assert_eq!(svc::slots_for(&conf, svc::AudioTask::Align), 1);

    // And the keys are the ones §7 defines, so a hand-edited file works too.
    let root = temp_dir("slots");
    let paths = paths(&root);
    write_conf_lines(
        &paths,
        &[
            "AUDIOCPP_SERVER=http://127.0.0.1:8765",
            "AUDIOCPP_ASR_SLOTS=2",
            "LLM_SLOTS=3",
        ],
    )
    .expect("write slots");
    let conf = settings::read(&paths).expect("read back");
    assert_eq!(svc::slots_for(&conf, svc::AudioTask::Asr), 2);
    assert_eq!(svc::llm_slots(&conf), 3);
}

/// §1's last paragraph: every request is recorded with what it cost. That half already exists in
/// [crate::requests], and the four servers' rows are what makes the table worth having — so the
/// kinds named in §1's API column are pinned against the log they land in, including one that
/// was cancelled.
#[test]
fn sec_02_services_1_the_four_servers_s13_every_kind_is_recorded_with_its_cost() {
    let root = temp_dir("log");
    let tree = Tree::new(root.join("demo.naivepost")).expect("a project folder");

    // One row per kind of call §1 lists, plus one that was cancelled. The row's shape is
    // requests.tsv's own (09 §10) and is tested in tests/text_formats_sidecars.rs; what matters
    // here is that each server answers at the path §1 names, and that a cancelled call leaves a
    // row like any other.
    let row = |kind: svc::Kind, service: Service, job: Option<&str>, outcome: Outcome| Request {
        started: "2026-09-19 12:00:00.000".into(),
        run: "2026-09-19T12-00-00Z".into(),
        step: "narrate".into(),
        job: job.unwrap_or_default().to_string(),
        service,
        model: svc::ASR_MODEL.into(),
        kind: match kind {
            svc::Kind::Chat => "chat",
            svc::Kind::ListModels => "models",
            svc::Kind::Upload => "upload",
            svc::Kind::RunTask => "asr",
            svc::Kind::Speech => "tts",
            svc::Kind::SubmitImage | svc::Kind::PollImage => "image",
            _ => "health",
        }
        .into(),
        sent_bytes: Some(4096),
        received_bytes: Some(512),
        first_byte_s: Some(0.012),
        on_wire_s: Some(1.234),
        outcome,
        attempt: 1,
        ..Default::default()
    };

    let calls = [
        (svc::Kind::Chat, Service::Llm, None, Outcome::Ok),
        (svc::Kind::ListModels, Service::Llm, None, Outcome::Ok),
        (svc::Kind::Health, Service::Audio, None, Outcome::Ok),
        (svc::Kind::Upload, Service::Audio, None, Outcome::Ok),
        (svc::Kind::RunTask, Service::Audio, None, Outcome::Ok),
        (svc::Kind::Speech, Service::Audio, None, Outcome::Ok),
        (svc::Kind::UnloadAll, Service::Audio, None, Outcome::Ok),
        (svc::Kind::Capabilities, Service::Image, None, Outcome::Ok),
        (svc::Kind::SubmitImage, Service::Image, None, Outcome::Ok),
        (svc::Kind::PollImage, Service::Image, Some("job-7"), Outcome::Ok),
        (
            svc::Kind::Speech,
            Service::Audio,
            None,
            Outcome::Cancelled,
        ),
    ];
    let count = calls.len();
    for (kind, service, job, outcome) in calls.into_iter() {
        requests::record(&tree, &row(kind, service, job, outcome)).expect("record a request");
    }

    let rows = requests::read(&tree).expect("read the log");
    assert_eq!(rows.len(), count);
    // The cost is what the row is for: the same numbers however the call ended.
    for row in &rows {
        assert_eq!(row.on_wire_s, Some(1.23));
        assert_eq!(row.sent_bytes, Some(4096));
        assert_eq!(row.received_bytes, Some(512));
        assert_eq!(row.model, "nemotron-asr");
    }
    // A cancelled call is recorded too — §1 asks for that outright.
    let cancelled = rows
        .iter()
        .find(|r| r.outcome == Outcome::Cancelled)
        .expect("a cancelled row");
    assert_eq!(cancelled.kind, "tts");
    assert_eq!(cancelled.service, Service::Audio);

    // And the paths §1 gives each kind are the ones a caller would send it to.
    let paths_of = |kind: svc::Kind, job: Option<&str>| {
        format!(
            "{}{}",
            match kind {
                svc::Kind::Chat | svc::Kind::ListModels => svc::llm_default(),
                svc::Kind::Capabilities | svc::Kind::SubmitImage | svc::Kind::PollImage =>
                    svc::image_default(),
                _ => svc::audio_default(),
            },
            kind.path(job)
        )
    };
    assert_eq!(
        paths_of(svc::Kind::Chat, None),
        "http://127.0.0.1:8731/v1/chat/completions"
    );
    assert_eq!(
        paths_of(svc::Kind::UnloadAll, None),
        "http://127.0.0.1:8765/v1/tasks/unload_all_models"
    );
    assert_eq!(
        paths_of(svc::Kind::PollImage, Some("job-7")).as_str(),
        "http://127.0.0.1:1234/sdcpp/v1/jobs/job-7"
    );
}

/// §1's model ids and the aligner preference are what an empty box means, so they must survive a
/// round trip through the settings file: `AUDIOCPP_*_MODEL` keys written by one version have to
/// read back as the same choice in the next.
#[test]
fn sec_02_services_1_the_four_servers_s14_model_choice_survives_the_settings_file() {
    let root = temp_dir("model-round-trip");
    let paths = paths(&root);

    update(&paths, |conf| {
        conf.audio_server = "gpu:8765".into();
        conf.audio_key = "audio-key".into();
        conf.asr_model = "whisper-large-v3".into();
        conf.align_model = "aab-alignment-ctc".into();
    })
    .expect("write model choice");

    let raw = std::fs::read_to_string(&paths.conf_path()).expect("read the file");
    for line in [
        "AUDIOCPP_SERVER=\"gpu:8765\"",
        "AUDIOCPP_API_KEY=\"audio-key\"",
        "AUDIOCPP_ASR_MODEL=\"whisper-large-v3\"",
        "AUDIOCPP_ALIGN_MODEL=\"aab-alignment-ctc\"",
    ] {
        assert!(raw.contains(line), "{raw}");
    }

    let conf = settings::read(&paths).expect("read back");
    fn none(_: &str) -> Option<String> { None }
    let endpoint = svc::audio_endpoint(&conf, none);
    assert_eq!(endpoint.url, "https://gpu:8765");
    assert_eq!(svc::authorization(&endpoint.key).as_deref(), Some("Bearer audio-key"));
    assert_eq!(svc::asr_model(&conf), "whisper-large-v3");
    // The aligner the file names is the one to try, given a server that declares it.
    let models = vec![
        svc::AudioModel {
            id: "aab-alignment-ctc".into(),
            family: String::new(),
            task: "align".into(),
        },
        svc::AudioModel {
            id: "qwen3-aligner".into(),
            family: String::new(),
            task: "align".into(),
        },
    ];
    assert_eq!(
        svc::pick_aligner(&conf.align_model, &models).as_deref(),
        Some("aab-alignment-ctc")
    );

    // And a file naming nothing at all still means the shipped defaults, not an error.
    update(&paths, |conf| {
        conf.asr_model.clear();
        conf.align_model.clear();
    })
    .expect("clear model choice");
    let conf = settings::read(&paths).expect("read back again");
    assert_eq!(svc::asr_model(&conf), "nemotron-asr");
    assert_eq!(svc::pick_aligner(&conf.align_model, &models).as_deref(), Some("qwen3-aligner"));
}

fn temp_dir(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "naivepost-services-{tag}-{}-{}",
        std::process::id(),
        tag
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("create temp dir");
    root
}
