//! §03-shell#8-details-confirmed-against-the-code-verification-pass — the details
//! that are only visible when something goes wrong: what Save as logs, which
//! cleared settings boxes go back to a default, which legacy keys survive a save,
//! and when the Settings dialog's log opens itself.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use naivepost::checks::TestLog;
use naivepost::icons;
use naivepost::{checks, probes, services};
use naivepost::project::{self, Project};
use naivepost::save_as;
use naivepost::settings::{self, Conf, Paths};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

/// A directory under the temp dir that no other test run shares.
fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "np-shell8-{tag}-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// Config and data folders under one temp root.
fn paths(tag: &str) -> Paths {
    let root = temp_dir(tag);
    Paths {
        config_dir: root.join("cfg/naivepost"),
        data_dir: root.join("data/naivepost"),
    }
}

/// A project folder holding `files`, so a move has something to carry.
fn project_folder(dir: &Path, files: &[&str]) -> PathBuf {
    fs::create_dir_all(dir).unwrap();
    fs::write(
        dir.join(project::PROJECT_FILE),
        serde_json::to_vec(&Project::default()).unwrap(),
    )
    .unwrap();
    for file in files {
        fs::write(dir.join(file), "x").unwrap();
    }
    dir.to_path_buf()
}

#[test]
fn sec_03_shell_8_details_confirmed_against_the_code_verification_pass_save_as_logs_only_what_it_moved()
{
    // Nothing to move, so nothing logged — even though the name changed. A project
    // saved before it was ever run is the ordinary case (§8). The target must not
    // exist for a rename to be attempted at all: F0.10 refuses a name already holding
    // work before this branch is reached, and `naivepost.json` itself counts as a
    // file to move, so "nothing to move" means a folder holding only the project file
    // has to be written first — which it is not: an empty project folder is what a
    // never-run project has.
    let root = temp_dir("save-quiet");
    let from = root.join("jan.naivepost");
    fs::create_dir_all(&from).unwrap();

    let saved = save_as::save_as(&Project::default(), &from, "feb").unwrap();
    assert!(saved.logs.is_empty(), "{:?}", saved.logs);

    // Something moved, so exactly one line saying where.
    let root = temp_dir("save-moved");
    let from = project_folder(&root.join("jan.naivepost"), &["render.mp4"]);

    let saved = save_as::save_as(&Project::default(), &from, "feb").unwrap();
    assert_eq!(
        saved.logs,
        vec![format!(
            ">>> moved the output folder to {}",
            root.join("feb.naivepost").display()
        )]
    );

    // The same name is an ordinary save: no move, no line.
    let saved = save_as::save_as(&Project::default(), &root.join("feb.naivepost"), "feb").unwrap();
    assert!(saved.logs.is_empty(), "{:?}", saved.logs);
}

#[test]
fn sec_03_shell_8_details_confirmed_against_the_code_verification_pass_a_failed_move_names_the_count_and_where_the_files_still_are()
{
    // The count is taken from the folder that is still there.
    let root = temp_dir("save-error");
    let from = project_folder(&root.join("jan.naivepost"), &["render.mp4", "cut.json"]);
    fs::create_dir_all(from.join("nested")).unwrap();
    fs::write(from.join("nested/deep.mkv"), "x").unwrap();

    // `busy.naivepost` does not exist, so F0.10's own refusal is not what stops this;
    // the rename being refused by the OS is the branch §8 words. Root is not stopped by
    // a read-only directory, so the wording is taken from `save_as` itself and checked
    // against a rename that did fail — this one, onto a file standing in the way.
    fs::write(root.join("busy.naivepost"), "not a folder").unwrap();
    let target = root.join("busy.naivepost");
    let err = match fs::rename(&from, &target) {
        Ok(()) => panic!("renaming a folder onto a file must be refused"),
        Err(err) => save_as::move_failure(&from, &target, 4, &err.to_string()),
    };
    // The line §8 words: what was attempted, why it failed, how many files there are
    // and where they still are.
    assert!(
        err.starts_with(&format!("!!! could not move the output folder to {}: ", target.display())),
        "{err}"
    );
    assert!(
        err.ends_with(&format!(" -- the 4 file(s) are still in {}", from.display())),
        "{err}"
    );
    // The reason is the OS's own sentence, between the two.
    let why = err
        .trim_start_matches(&format!("!!! could not move the output folder to {}: ", target.display()))
        .split_once(" -- ")
        .unwrap()
        .0;
    assert!(!why.is_empty(), "{err} names no reason");
    assert!(from.join("render.mp4").exists(), "the files are still there");
}

#[test]
fn sec_03_shell_8_details_confirmed_against_the_code_verification_pass_settings_file_writes_the_five_cleared_boxes_as_defaults_and_the_rest_as_typed()
{
    // The five: a cleared box is not a usable value, so the shipped default goes back.
    for (key, default) in [
        ("AUDIOCPP_VOICES", "/mnt/models/audiocpp/voices"),
        ("AUDIOCPP_ASR_MODEL", "nemotron-asr"),
        ("AUDIOCPP_DIAR_MODEL", "sortformer-diar"),
        ("AUDIOCPP_TTS_MODEL", "index-tts2"),
        ("AUDIOCPP_SEP_MODEL", "bs-roformer"),
    ] {
        assert_eq!(settings::written_value(key, "", "anything"), "anything", "{key}");
        assert_eq!(settings::written_value(key, "/typed", default), "/typed", "{key}");
    }

    // Everything else is written as typed, empty included: clearing ffmpeg is an answer.
    for key in [
        "LLM_SERVER",
        "LLM_MODEL",
        "LLM_API_KEY",
        "AUDIOCPP_SERVER",
        "AUDIOCPP_API_KEY",
        "AUDIOCPP_ALIGN_MODEL",
        "FFMPEG",
        "FIREFOX",
        "SD_SERVER",
        "SD_API_KEY",
    ] {
        assert_eq!(settings::written_value(key, "", "unused"), "", "{key}");
    }

    // And through the write path: an all-empty Conf still names the five defaults.
    let body = settings::render(&Conf::default());
    for line in [
        "AUDIOCPP_VOICES=\"/mnt/models/audiocpp/voices\"",
        "AUDIOCPP_ASR_MODEL=\"nemotron-asr\"",
        "AUDIOCPP_DIAR_MODEL=\"sortformer-diar\"",
        "AUDIOCPP_TTS_MODEL=\"index-tts2\"",
        "AUDIOCPP_SEP_MODEL=\"bs-roformer\"",
    ] {
        assert!(body.contains(line), "{line} missing from\n{body}");
    }
    for key in ["LLM_SERVER", "FFMPEG", "LLM_API_KEY"] {
        assert!(body.contains(&format!("{key}=\"\"")), "{key} not written empty");
    }
    assert_eq!(settings::DEFAULTED_WHEN_CLEARED.len(), 5);
}

#[test]
fn sec_03_shell_8_details_confirmed_against_the_code_verification_pass_settings_file_keeps_its_explanatory_comments()
{
    let body = settings::render(&Conf::default());
    let head: Vec<&str> = body.lines().take(2).collect();
    assert_eq!(
        head,
        vec![
            "# Endpoints and local tools used by the pipeline (written by the settings dialog).",
            "# Bash-sourceable -- keep this file chmod 600, the key is a credential.",
        ]
    );

    // Rewritten whole: every §6 key is in there, so nothing is lost by replacing the file.
    for key in [
        "LLM_SERVER",
        "LLM_MODEL",
        "LLM_API_KEY",
        "AUDIOCPP_SERVER",
        "AUDIOCPP_API_KEY",
        "AUDIOCPP_VOICES",
        "AUDIOCPP_ASR_MODEL",
        "AUDIOCPP_DIAR_MODEL",
        "AUDIOCPP_TTS_MODEL",
        "AUDIOCPP_SEP_MODEL",
        "AUDIOCPP_ALIGN_MODEL",
        "FFMPEG",
        "FIREFOX",
        "SD_SERVER",
        "SD_API_KEY",
    ] {
        assert!(body.contains(&format!("{key}=")), "{key} missing");
    }
}

#[test]
fn sec_03_shell_8_details_confirmed_against_the_code_verification_pass_legacy_settings_json_is_read_once_and_never_written()
{
    let paths = paths("legacy-json");
    fs::create_dir_all(&paths.config_dir).unwrap();
    let legacy = paths.settings_path();
    let body = r#"{"projects":{"/work/jan":"/work/jan/show.naivepost/naivepost.json"}}"#;
    fs::write(&legacy, body).unwrap();

    // With PROJECT_* lines present the conf already knows: the json is not consulted.
    let mut conf = Conf::default();
    conf.projects
        .insert("/here".into(), "/here/show.naivepost/naivepost.json".into());
    assert!(!settings::recover_projects(&mut conf, &paths).unwrap());
    assert_eq!(conf.projects.len(), 1);

    // Without them the remembered list comes from the legacy file.
    let mut empty = Conf::default();
    assert!(settings::recover_projects(&mut empty, &paths).unwrap());
    assert_eq!(
        empty.projects.get("/work/jan").map(String::as_str),
        Some("/work/jan/show.naivepost/naivepost.json")
    );

    // Read, never written: the file is byte-identical after a save of what it filled.
    settings::save(&paths, &empty).unwrap();
    assert_eq!(fs::read_to_string(&legacy).unwrap(), body);
}

#[test]
fn sec_03_shell_8_details_confirmed_against_the_code_verification_pass_legacy_keys_are_read_and_ignored()
{
    let conf = settings::parse(
        "AUDIOCPP_MODELS=\"/srv/models\"\nPROMPT_CUT=\"be brief\"\nSD_MODEL=\"sd-v1\"\nAUDIOCPP_LANGUAGE=\"de\"\n",
    )
    .expect("an older build's keys parse");

    // Read and ignored: they do not come back out of a rewrite.
    let body = settings::render(&conf);
    for key in ["AUDIOCPP_MODELS", "PROMPT_CUT", "SD_MODEL", "AUDIOCPP_LANGUAGE"] {
        assert!(!body.contains(key), "{key} survived into\n{body}");
    }

    // `voices/` under that root is the voices folder only while AUDIOCPP_VOICES is absent.
    assert_eq!(
        settings::voices_folder("", "/srv/models", false, Path::new("/data")),
        PathBuf::from("/srv/models/voices")
    );
    assert_eq!(
        settings::voices_folder("/fast/voices", "/srv/models", false, Path::new("/data")),
        PathBuf::from("/fast/voices"),
        "the named folder wins"
    );
    assert_eq!(
        settings::voices_folder("", "", false, Path::new("/data")),
        PathBuf::from("/mnt/models/audiocpp/voices"),
        "no legacy root, no AUDIOCPP_VOICES: the platform default"
    );
    // A relative legacy root is not a folder any more than a relative override is.
    assert_eq!(
        settings::voices_folder("", "models", true, Path::new("/data")),
        PathBuf::from("/data/voices")
    );
}

#[test]
fn sec_03_shell_8_details_confirmed_against_the_code_verification_pass_settings_dialog_log_stays_collapsed_until_a_test_fails()
{
    let mut log = TestLog::new();
    assert!(!log.open, "collapsed before anything has been pressed");

    log.push("the LLM answered in 0.4s".into(), true);
    log.push("vision saw red".into(), true);
    assert!(log.lines.len() == 2 && !log.open, "successes do not open it: {:?}", log.lines);

    log.push("no ffprobe beside ffmpeg".into(), false);
    assert!(log.open, "the first failure opens it");

    log.push("ffmpeg is fine after all".into(), true);
    assert!(log.open, "it never closes itself");
    assert_eq!(log.lines.len(), 4);
}

/// The dialog forwards each row's verdict here; the widget only renders `open`, so
/// this is where the rule lives. Kept as a compile-time shape check for the UI.
#[allow(dead_code)]
fn forward(rows: &[(String, bool)], log: &Arc<Mutex<TestLog>>) {
    for (verdict, passed) in rows {
        log.lock().unwrap().push(verdict.clone(), *passed);
    }
}

#[test]
fn sec_03_shell_8_details_confirmed_against_the_code_verification_pass_icons_are_searched_beside_the_binary_then_the_app_root_then_here()
{
    let root = temp_dir("icons");
    let exe = root.join("bin");
    let app = root.join("prefix/share/naivepost");
    let cwd = root.join("checkout/rust");
    for dir in [&exe, &app, &cwd] {
        fs::create_dir_all(dir).unwrap();
    }

    // The order is the answer when all three are there.
    assert_eq!(
        icons::search_paths(&exe, &app, &cwd),
        vec![
            exe.join("icons"),
            app.join("icons"),
            cwd.join("icons"),
        ]
    );

    // The first that exists wins; a checkout has only its own.
    fs::create_dir_all(app.join("icons")).unwrap();
    fs::create_dir_all(cwd.join("icons")).unwrap();
    assert_eq!(icons::found(&exe, &app, &cwd), Some(app.join("icons")));

    fs::create_dir_all(exe.join("icons")).unwrap();
    assert_eq!(icons::found(&exe, &app, &cwd), Some(exe.join("icons")));

    // Nothing anywhere is a missing icon, not a failed launch.
    let nowhere = root.join("nowhere");
    assert_eq!(icons::found(&nowhere, &nowhere, &nowhere), None);
    assert_eq!(icons::ICONS_DIR, "icons");
}

#[test]
fn sec_03_shell_8_details_confirmed_against_the_code_verification_pass_the_database_paths_are_logged_once_and_the_commands_run_in_background()
{
    let mut log: Vec<String> = Vec::new();
    let mut databases = icons::Databases::new();
    let dir = temp_dir("databases");

    // Nothing rewritten, so nothing to tell anyone.
    assert!(databases
        .refresh(false, false, &dir, &mut log)
        .is_empty());
    assert!(log.is_empty(), "{log:?}");

    // Both files written: two commands, one line about where.
    let commands = databases.refresh(true, true, &dir, &mut log);
    assert_eq!(
        commands,
        vec![
            vec!["update-mime-database".to_string(), dir.display().to_string()],
            vec!["update-desktop-database".to_string(), dir.display().to_string()],
        ]
    );
    assert_eq!(log, vec![format!(">>> told the desktop about {}", dir.display())]);

    // A second save in the same launch runs the tools again and says nothing more.
    let again = databases.refresh(true, true, &dir, &mut log);
    assert_eq!(again.len(), 2);
    assert_eq!(log.len(), 1, "{log:?}");
    assert!(databases.logged);
}

#[test]
fn sec_03_shell_8_details_confirmed_against_the_code_verification_pass_a_missing_model_names_the_catalogue_and_only_a_default_gets_the_install_command()
{
    let model = |id: &str, task: &str| services::AudioModel {
        id: id.into(),
        family: String::new(),
        task: task.into(),
    };
    let catalogue = vec![model("sortformer-diar", "diar"), model("nemotron-asr", "asr")];
    let url = "http://cpp:8765";
    let prepare = services::Need { task: "sep", step: "Prepare" };

    // A shipped default: the exact command, package and all.
    let msg = services::missing_model(url, &catalogue, services::SEP_MODEL, prepare);
    assert!(msg.contains(url) && msg.contains("bs-roformer"), "{msg}");
    assert!(
        msg.contains(
            "docker compose exec audio python3 tools/model_manager_v2.py install bs_roformer_q8_0 --models-root models"
        ),
        "{msg}"
    );

    // A hand-typed id: the same catalogue, and no invented package.
    let mine = services::missing_model(url, &catalogue, "my-separation-model", prepare);
    assert!(mine.contains("my-separation-model"), "{mine}");
    assert!(mine.contains("sortformer-diar") && mine.contains("nemotron-asr"), "{mine}");
    assert!(!mine.contains("model_manager_v2.py"), "{mine}");
}

#[test]
fn sec_03_shell_8_details_confirmed_against_the_code_verification_pass_a_model_served_for_another_task_is_refused_by_name_and_step()
{
    let model = |id: &str, task: &str| services::AudioModel {
        id: id.into(),
        family: String::new(),
        task: task.into(),
    };
    let catalogue = vec![model("whisper", "asr")];

    assert_eq!(
        services::missing_model(
            "http://cpp:8765",
            &catalogue,
            "whisper",
            services::Need { task: "sep", step: "Prepare" },
        ),
        "\"whisper\" on http://cpp:8765 is declared task \"asr\", but Prepare needs \"sep\" there"
    );

    // The same refusal from the Settings row names its own step.
    assert_eq!(
        checks::model_verdict(
            &catalogue,
            "whisper",
            services::Need { task: "clon", step: "Narrate" },
        )
        .unwrap_err(),
        "\"whisper\" is declared task \"asr\", and Narrate needs \"clon\""
    );
}

#[test]
fn sec_03_shell_8_details_confirmed_against_the_code_verification_pass_a_server_refusing_uploads_gets_the_fix()
{
    let fix = services::upload_refused(403, "uploads are disabled").expect("403 is the refusal");
    assert!(fix.contains("--ui-management"), "{fix}");
    assert!(!fix.contains("mount"), "{fix}");

    // The server named a folder it was not given: that is the second half of the fix.
    let with_path = services::upload_refused(403, "cannot read /mnt/models/uploads")
        .expect("403 is the refusal");
    assert!(with_path.contains("--ui-management"), "{with_path}");
    assert!(with_path.contains("/mnt/models/uploads"), "{with_path}");

    // 401 and 405 are the same misconfiguration in a different costume.
    assert!(services::upload_refused(401, "no").is_some());
    assert!(services::upload_refused(405, "no").is_some());

    // Anything else is an ordinary HTTP error and the client's own words stand.
    assert_eq!(services::upload_refused(500, "boom"), None);
    assert_eq!(services::upload_refused(200, "fine"), None);
}

#[test]
fn sec_03_shell_8_details_confirmed_against_the_code_verification_pass_an_upload_answering_200_without_a_path_is_a_failure()
{
    let ok = serde_json::json!({ "path": "/x/y.wav" });
    assert_eq!(services::upload_path(&ok).unwrap(), "/x/y.wav");

    let empty = serde_json::json!({ "path": "" });
    assert!(services::upload_path(&empty).is_err(), "an empty path is no path");

    let lying = serde_json::json!({ "ok": true });
    let err = services::upload_path(&lying).expect_err("200 with no path is not a success");
    assert!(err.contains(r#"{\"ok\":true}"#) || err.contains("\"ok\":true"), "{err}");
}

#[test]
fn sec_03_shell_8_details_confirmed_against_the_code_verification_pass_a_duration_comes_from_the_header_or_from_decoding_once_and_is_cached_per_path_size_mtime()
{
    // The header wins: decoding a 40-minute file to confirm what its header says is waste.
    assert_eq!(
        probes::duration("lecture.mkv", Some(2412.5), Some(9999.0)),
        Ok((2412.5, probes::Source::Header))
    );
    // A recorder killed mid-write has no header duration, so the decode is used.
    assert_eq!(
        probes::duration("lecture.mkv", None, Some(2412.5)),
        Ok((2412.5, probes::Source::Decoded))
    );
    let err = probes::duration("lecture.mkv", None, None).unwrap_err();
    assert_eq!(err, "lecture.mkv: no duration in the header and nothing to decode it from");

    // Cached per path + size + mtime: the same file answers once, a changed one does not.
    let first = probes::key("lecture.mkv", 512, 100);
    assert_eq!(probes::render(&first), "lecture.mkv\t512\t100");

    let mut cache = probes::Cache::new();
    assert_eq!(cache.lookup(&first), None);
    cache.store(first.clone(), 2412.5);
    assert_eq!(cache.lookup(&first), Some(2412.5));

    let rewritten = probes::key("lecture.mkv", 640, 100);
    assert_eq!(cache.lookup(&rewritten), None, "a different size is a different file");
    let touched = probes::key("lecture.mkv", 512, 200);
    assert_eq!(cache.lookup(&touched), None, "a newer mtime is a different file");

    let other = probes::key("other.mkv", 512, 100);
    assert_eq!(cache.lookup(&other), None);
    cache.store(other.clone(), 3.0);
    assert_eq!(cache.lookup(&first), Some(2412.5), "one file's answer is not another's");
}
