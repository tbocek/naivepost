// §01-project-and-files#7-machine-files — 7. Machine files.
//
// Spec: spec/01-project-and-files.md §7, with the keys and precedence of
// spec/03-shell.md §6 and the modes of spec/01-project-and-files.md §1.
// Cited settings keys: LLM_SERVER, LLM_MODEL, LLM_API_KEY, AUDIOCPP_SERVER,
//   AUDIOCPP_API_KEY, AUDIOCPP_VOICES, AUDIOCPP_ASR_MODEL, AUDIOCPP_DIAR_MODEL,
//   AUDIOCPP_TTS_MODEL, AUDIOCPP_SEP_MODEL, AUDIOCPP_ALIGN_MODEL, FFMPEG, FIREFOX,
//   SD_SERVER, SD_API_KEY, SUBTITLE_LANGUAGES, LLM_SLOTS, AUDIOCPP_ASR_SLOTS,
//   AUDIOCPP_DIAR_SLOTS, AUDIOCPP_ALIGN_SLOTS, AUDIOCPP_TTS_SLOTS, AUDIOCPP_SEP_SLOTS,
//   SD_SLOTS, PROJECT_<n>_ROOT, PROJECT_<n>_FILE.
// Cited env: NAIVEPOST_TTS_URL, XDG_CONFIG_HOME, XDG_DATA_HOME, HOME.
//
// Every test is handed its own directories, so nothing here reads or writes the real $HOME.

use std::fs;
use std::path::{Path, PathBuf};

use naivepost::settings::{self, Conf, Paths, ProjectRef, Slots};

/// A throwaway config/data pair standing in for `$XDG_CONFIG_HOME` and `$XDG_DATA_HOME`.
struct Dirs {
    root: PathBuf,
    paths: Paths,
}

impl Dirs {
    fn new(label: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "np-mach-{label}-{pid}-{n}",
            pid = std::process::id(),
            n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
        ));
        let _ = fs::remove_dir_all(&root);
        let paths = Paths {
            config_dir: root.join("cfg/naivepost"),
            data_dir: root.join("data/naivepost"),
        };
        Self { root, paths }
    }
}

static COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Every file under `root`, relative and sorted.
fn walk(root: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        // A folder that was never created holds nothing — `write_prompt` correctly writes no
        // file when the text is the shipped one, so `prompts/` may simply not exist.
        let Ok(entries) = fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                out.push(path.strip_prefix(root).unwrap().display().to_string());
            }
        }
    }
    out.sort();
    out
}

#[cfg(unix)]
fn mode(path: &Path) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    fs::metadata(path).unwrap().permissions().mode() & 0o777
}

#[test]
fn sec_01_project_and_files_7_machine_files_s1_llm_conf_is_bash_sourceable_and_0600() {
    // "…(0600): bash-sourceable `KEY="value"`; keys in 03-shell.md §6."
    let d = Dirs::new("conf");
    let mut conf = Conf::default();
    conf.server = "http://127.0.0.1:1234/v1".into();
    conf.model = "qwen3-6".into();
    conf.key = "hunter2".into();
    conf.audio_server = "http://localhost:8888".into();
    conf.voices = "/mnt/models/audiocpp/voices".into();
    conf.align_model = "mfa".into();
    conf.firefox = "/usr/lib/firefox/firefox".into();
    conf.subtitle_languages = "en:English:surto|pl:Polski:kolejorz".into();

    settings::save(&d.paths, &conf).unwrap();
    let written = fs::read_to_string(d.paths.conf_path()).unwrap();
    for key in [
        "LLM_SERVER=", "LLM_MODEL=", "LLM_API_KEY=", "AUDIOCPP_SERVER=", "AUDIOCPP_API_KEY=",
        "AUDIOCPP_VOICES=", "AUDIOCPP_ASR_MODEL=", "AUDIOCPP_DIAR_MODEL=", "AUDIOCPP_TTS_MODEL=",
        "AUDIOCPP_SEP_MODEL=", "AUDIOCPP_ALIGN_MODEL=", "FFMPEG=", "FIREFOX=", "SD_SERVER=",
        "SD_API_KEY=", "SUBTITLE_LANGUAGES=",
    ] {
        assert!(written.contains(key), "{key} missing from:\n{written}");
    }

    // Every value is one double-quoted word, so the file sources in bash.
    for line in written.lines().filter(|l| !l.starts_with('#') && !l.is_empty()) {
        let (_, value) = line.split_once('=').unwrap_or_else(|| panic!("not KEY=value: {line}"));
        assert!(value.starts_with('"') && value.ends_with('"'), "unquoted: {line}");
    }

    // Round trip, including the keys that need quoting.
    let back = settings::read(&d.paths).unwrap();
    assert_eq!(back.server, conf.server);
    assert_eq!(back.model, conf.model);
    assert_eq!(back.key, conf.key);
    assert_eq!(back.voices, conf.voices);
    assert_eq!(back.subtitle_languages, conf.subtitle_languages);

    // The modes §1 excepts for this file: 0600 in a directory at 0700.
    assert_eq!(mode(&d.paths.conf_path()), 0o600);
    assert_eq!(mode(&d.paths.config_dir), 0o700);

    // Written whole each time: a second save replaces the file rather than appending.
    conf.model = "qwen3-7".into();
    settings::save(&d.paths, &conf).unwrap();
    let written = fs::read_to_string(d.paths.conf_path()).unwrap();
    assert_eq!(written.matches("LLM_MODEL=").count(), 1, "{written}");
    assert!(!written.contains("qwen3-6"), "old value still there:\n{written}");

    // No file at all is the empty config — a machine that never opened the dialog. The fresh
    // root stands beside the one just written, so it exists but holds no conf.
    fs::create_dir_all(Dirs::new("conf-fresh").root).unwrap();
    let fresh = Paths {
        config_dir: d.root.join("nowhere/naivepost"),
        data_dir: d.root.join("nowhere-data/naivepost"),
    };
    assert_eq!(settings::read(&fresh).unwrap(), Conf::default());

    fs::remove_dir_all(&d.root).unwrap();
}

#[test]
fn sec_01_project_and_files_7_machine_files_s1_slots_absent_means_one() {
    // 03-shell.md §6: the seven `*_SLOTS` keys, "absent = 1".
    let d = Dirs::new("slots");
    assert_eq!(settings::read(&d.paths).unwrap().slots.llm, 1);

    // A default machine gets no slot lines at all, so the file does not grow on every save.
    settings::save(&d.paths, &Conf::default()).unwrap();
    let written = fs::read_to_string(d.paths.conf_path()).unwrap();
    assert!(!written.contains("_SLOTS="), "{written}");

    // Set one and it is the only slot key written; read back, the others are still 1.
    let mut conf = Conf::default();
    conf.slots.tts = 4;
    settings::save(&d.paths, &conf).unwrap();
    let written = fs::read_to_string(d.paths.conf_path()).unwrap();
    assert!(written.contains("AUDIOCPP_TTS_SLOTS=4"), "{written}");
    assert_eq!(written.matches("_SLOTS=").count(), 1, "{written}");

    let back = settings::read(&d.paths).unwrap();
    assert_eq!(back.slots.tts, 4);
    assert_eq!(back.slots.llm, 1);
    for key in [
        "LLM_SLOTS",
        "AUDIOCPP_ASR_SLOTS",
        "AUDIOCPP_DIAR_SLOTS",
        "AUDIOCPP_ALIGN_SLOTS",
        "AUDIOCPP_SEP_SLOTS",
        "SD_SLOTS",
    ] {
        assert!(
            Slots::KEY_NAMES.contains(&key),
            "{key} is not a slot settings key"
        );
    }

    fs::remove_dir_all(&d.root).unwrap();
}

#[test]
fn sec_01_project_and_files_7_machine_files_s1_project_pairs_sorted_so_save_is_identical() {
    // "Also holds state: `PROJECT_<n>_ROOT` / `PROJECT_<n>_FILE` pairs (last project per
    // root), sorted so an unchanged save is byte-identical."
    let d = Dirs::new("pairs");
    let mut conf = Conf::default();
    // Inserted out of order on purpose: the numbering must not depend on insertion.
    conf.projects
        .insert("/mnt/media/NP".into(), "/mnt/media/NP/b.naivepost/project.json".into());
    conf.projects.insert(
        "/home/bocek/Videos".into(),
        "/home/bocek/Videos/a.naivepost/project.json".into(),
    );

    settings::save(&d.paths, &conf).unwrap();
    let first = fs::read_to_string(d.paths.conf_path()).unwrap();
    assert!(first.contains("PROJECT_1_ROOT=\"/home/bocek/Videos\""), "{first}");
    assert!(first.contains("PROJECT_2_FILE=\"/mnt/media/NP/b.naivepost/project.json\""), "{first}");

    // An unchanged save leaves the bytes alone.
    let again = settings::save(&d.paths, &conf).unwrap();
    assert!(!again, "nothing changed, so nothing to write");
    assert_eq!(fs::read_to_string(d.paths.conf_path()).unwrap(), first);

    // Read back through the pairs: same list, same order.
    let back = settings::read(&d.paths).unwrap();
    assert_eq!(back.projects(), conf.projects());
    assert_eq!(
        back.projects(),
        vec![
            ProjectRef {
                root: "/home/bocek/Videos".into(),
                file: "/home/bocek/Videos/a.naivepost/project.json".into(),
            },
            ProjectRef {
                root: "/mnt/media/NP".into(),
                file: "/mnt/media/NP/b.naivepost/project.json".into(),
            },
        ]
    );

    fs::remove_dir_all(&d.root).unwrap();
}

#[test]
fn sec_01_project_and_files_7_machine_files_s1_a_half_pair_is_no_project() {
    // The pairs are read as pairs: a root with no file (or the reverse) is not a project, and
    // it must not come back as one with an empty path.
    let d = Dirs::new("half");
    fs::create_dir_all(&d.paths.config_dir).unwrap();
    fs::write(
        d.paths.conf_path(),
        "LLM_MODEL=qwen3-6\nPROJECT_1_ROOT=/home/bocek/Videos\n\
         PROJECT_2_ROOT=/mnt/media/NP\nPROJECT_2_FILE=/mnt/media/NP/b.naivepost/project.json\n",
    )
    .unwrap();

    let conf = settings::read(&d.paths).unwrap();
    assert_eq!(conf.model, "qwen3-6");
    assert_eq!(
        conf.projects(),
        vec![ProjectRef {
            root: "/mnt/media/NP".into(),
            file: "/mnt/media/NP/b.naivepost/project.json".into(),
        }]
    );

    fs::remove_dir_all(&d.root).unwrap();
}

#[test]
fn sec_01_project_and_files_7_machine_files_s1_unknown_keys_are_carried_through() {
    // A key this build does not know belongs to a newer one. Writing the file whole must not
    // erase a setting its reader still needs.
    let d = Dirs::new("unknown");
    fs::create_dir_all(&d.paths.config_dir).unwrap();
    fs::write(d.paths.conf_path(), "LLM_MODEL=qwen3-6\nWHATEVER_NEW=42\n").unwrap();

    let mut conf = settings::read(&d.paths).unwrap();
    conf.model = "qwen3-7".into();
    settings::save(&d.paths, &conf).unwrap();

    let written = fs::read_to_string(d.paths.conf_path()).unwrap();
    assert!(written.contains("WHATEVER_NEW=\"42\""), "{written}");
    assert_eq!(settings::read(&d.paths).unwrap().model, "qwen3-7");

    fs::remove_dir_all(&d.root).unwrap();
}

#[test]
fn sec_01_project_and_files_7_machine_files_s2_legacy_settings_read_once_never_written() {
    // "`~/.config/naivepost/settings.json`: legacy, read once (never written) on the launch
    // after the merge into llm.conf, to recover remembered projects when the conf has no
    // `PROJECT_*` keys."
    let d = Dirs::new("legacy");
    fs::create_dir_all(&d.paths.config_dir).unwrap();
    let legacy = r#"{"projects":{"/mnt/media/NP":"/mnt/media/NP/b.naivepost/project.json"}}"#;
    fs::write(d.paths.settings_path(), legacy).unwrap();

    // The conf says nothing about projects, so the legacy file answers once.
    let mut conf = settings::read(&d.paths).unwrap();
    assert!(settings::recover_projects(&mut conf, &d.paths).unwrap());
    assert_eq!(conf.projects().len(), 1);

    // "Never written": saving leaves the legacy file byte-for-byte as it was, and the new
    // file is plain KEY=value, not JSON.
    settings::save(&d.paths, &conf).unwrap();
    assert_eq!(fs::read_to_string(d.paths.settings_path()).unwrap(), legacy);
    let written = fs::read_to_string(d.paths.conf_path()).unwrap();
    assert!(!written.contains('{'), "must not be JSON:\n{written}");
    assert!(written.contains("PROJECT_1_FILE=\"/mnt/media/NP/b.naivepost/project.json\""));

    // Once the conf has its own pairs, the legacy file is not consulted again.
    let mut conf = settings::read(&d.paths).unwrap();
    assert!(!conf.projects().is_empty());
    fs::write(d.paths.settings_path(), r#"{"projects":{"/x":"/x/y.naivepost/project.json"}}"#)
        .unwrap();
    assert!(!settings::recover_projects(&mut conf, &d.paths).unwrap());
    assert_eq!(conf.projects()[0].root, "/mnt/media/NP");

    // No legacy file is the normal case, not an error. The path stands beside the one just
    // used, so the folder exists and only the file is missing.
    let fresh = Paths {
        config_dir: d.root.join("nowhere/naivepost"),
        data_dir: d.root.join("nowhere-data/naivepost"),
    };
    assert!(!settings::recover_projects(&mut Conf::default(), &fresh).unwrap());

    fs::remove_dir_all(&d.root).unwrap();
}

#[test]
fn sec_01_project_and_files_7_machine_files_s3_voices_folder_default_and_override() {
    // "The voices folder (`AUDIOCPP_VOICES`): the one folder read outside a project; no GUI
    // box; default `/mnt/models/audiocpp/voices` (the dev box), inside Flatpak
    // `$XDG_DATA_HOME/naivepost/voices`."
    let d = Dirs::new("voices");
    let data = d.paths.data_dir.clone();

    assert_eq!(
        settings::voices_folder("", "", true, &data),
        data.join("voices"),
        "inside Flatpak the default sits under the data dir"
    );
    assert_eq!(
        settings::voices_folder("", "", false, &data),
        PathBuf::from("/mnt/models/audiocpp/voices"),
        "outside it the default is the dev box's folder"
    );
    assert_eq!(
        settings::voices_folder("", "", false, &data),
        PathBuf::from("/mnt/models/audiocpp/voices"),
        "an empty setting is no setting"
    );
    assert_eq!(
        settings::voices_folder("/fast/models/voices", "", true, &data),
        PathBuf::from("/fast/models/voices"),
        "the override wins in both sandbox and dev box"
    );
    assert_eq!(
        settings::voices_folder("relative/voices", "", false, &data),
        PathBuf::from("/mnt/models/audiocpp/voices"),
        "a relative setting would resolve against whatever the process was started from"
    );

    // Creating it is idempotent and leaves it at 0700 — nothing else writes here.
    let folder = settings::voices_folder("", "", true, &data);
    settings::ensure_voices_folder(&folder).unwrap();
    settings::ensure_voices_folder(&folder).unwrap();
    assert!(folder.is_dir());
    assert_eq!(mode(&folder), 0o700);

    fs::remove_dir_all(&d.root).unwrap();
}

#[test]
fn sec_01_project_and_files_7_machine_files_s4_prompt_saved_only_when_it_differs() {
    // "`~/.config/naivepost/prompts/<key>.txt`: a prompt edited on this machine (only if it
    // differs from the shipped text)."
    let d = Dirs::new("prompts");
    let shipped = "Rewrite these sentences for a reader who has not seen the video.";

    // Nothing on disk: the shipped text runs.
    assert_eq!(settings::read_prompt(&d.paths, "narrate.rephrase").unwrap(), None);
    assert_eq!(
        settings::prompt_text(&d.paths, "narrate.rephrase", shipped).unwrap(),
        shipped
    );

    // Saving the shipped wording back is not an edit: no file appears.
    assert!(!settings::write_prompt(&d.paths, "narrate.rephrase", shipped, shipped).unwrap());
    assert_eq!(walk(&d.root), Vec::<String>::new());

    // An actual edit lands in `<key>.txt` and wins over the shipped text.
    assert!(settings::write_prompt(&d.paths, "narrate.rephrase", "terse, no preamble", shipped).unwrap());
    assert_eq!(
        settings::prompt_text(&d.paths, "narrate.rephrase", shipped).unwrap(),
        "terse, no preamble"
    );
    assert_eq!(
        fs::read_to_string(d.paths.prompt_file("narrate.rephrase")).unwrap(),
        "terse, no preamble"
    );
    // The exception in §1's mode list: prompts are private too.
    assert_eq!(mode(&d.paths.prompt_file("narrate.rephrase")), 0o600);
    assert_eq!(mode(&d.paths.prompts_dir()), 0o700);

    // Reverting to the shipped wording drops the override rather than leaving a copy of it
    // behind that would shadow a future change to the shipped text.
    assert!(settings::clear_prompt(&d.paths, "narrate.rephrase").unwrap());
    assert_eq!(settings::read_prompt(&d.paths, "narrate.rephrase").unwrap(), None);
    assert!(!settings::clear_prompt(&d.paths, "narrate.rephrase").unwrap());

    // A key is a file name: one that climbs out of `prompts/` is refused.
    assert!(settings::prompt_file_ok("narrate.rephrase").is_ok());
    assert!(settings::prompt_file_ok("../escape").is_err());
    assert!(settings::prompt_file_ok("a/b").is_err());
    assert!(settings::prompt_file_ok("").is_err());

    fs::remove_dir_all(&d.root).unwrap();
}

#[test]
fn sec_01_project_and_files_7_machine_files_s5_hang_dump_names_the_stalled_component() {
    // "`~/.config/naivepost/hang-*.txt`: watchdog dumps" — 0600 by §1's exception list. What
    // has to be readable from one is which component stopped answering, its PID and how long
    // it had been unresponsive.
    let d = Dirs::new("hang");

    let path = settings::write_hang_dump(
        &d.paths,
        "gtk",
        4242,
        std::time::Duration::from_millis(3200),
        &settings::hang_stamp(9, 19, 14, 30, 12),
    )
    .unwrap();
    let name = path.file_name().unwrap().to_str().unwrap().to_string();
    assert_eq!(name, "hang-0919-143012.txt", "{name}");
    assert_eq!(path.parent().unwrap(), d.paths.config_dir);

    let text = fs::read_to_string(&path).unwrap();
    assert!(text.contains("gtk"), "component missing:\n{text}");
    assert!(text.contains("4242"), "pid missing:\n{text}");
    assert!(text.contains("3.2s"), "how late:\n{text}");
    assert_eq!(mode(&path), 0o600);

    // One dump per hang: a second stall at a different second is its own file, and the
    // month-first stamp sorts them in the order they happened.
    let second = settings::write_hang_dump(
        &d.paths,
        "ffmpeg",
        99,
        std::time::Duration::from_secs(4),
        &settings::hang_stamp(9, 19, 14, 35, 0),
    )
    .unwrap();
    assert_ne!(second, path);
    let dumps: Vec<String> = fs::read_dir(&d.paths.config_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| n.starts_with("hang-"))
        .collect();
    assert_eq!(dumps.len(), 2, "{dumps:?}");
    assert!(fs::read_to_string(&second).unwrap().contains("4s"), "whole seconds print plain");

    fs::remove_dir_all(&d.root).unwrap();
}

#[test]
fn sec_01_project_and_files_7_machine_files_s6_precedence_dialog_then_file_then_default() {
    // 03-shell.md §6: "Precedence: dialog box → this file → (legacy `<root>/llm.conf`,
    // migrated once) → built-in default." An empty answer is not an answer.
    let dialog = Some("http://dialog:1");
    let file = Some("http://file:2");
    let legacy = Some("http://legacy:3");

    assert_eq!(settings::resolve(dialog, file, legacy, "default"), "http://dialog:1");
    assert_eq!(settings::resolve(None, file, legacy, "default"), "http://file:2");
    assert_eq!(settings::resolve(None, None, legacy, "default"), "http://legacy:3");
    assert_eq!(settings::resolve(None, None, None, "default"), "default");
    // A cleared box means "back to the default", so it must not shadow anything below it.
    assert_eq!(settings::resolve(Some(""), file, legacy, "default"), "http://file:2");
    assert_eq!(settings::resolve(Some(""), Some(""), Some(""), "default"), "default");
}

#[test]
fn sec_01_project_and_files_7_machine_files_s6_audio_url_honours_the_environment() {
    // "The audio URL also honours `NAIVEPOST_TTS_URL` and `AUDIOCPP_SERVER` below the dialog;
    // sd `SD_SERVER`." So: dialog, then the variable, then the file's key, then the default.
    assert_eq!(
        settings::audio_url(
            Some("http://dialog:1"),
            Some("http://env:2"),
            Some("http://conf:3"),
            "http://default:4",
        ),
        "http://dialog:1"
    );
    assert_eq!(
        settings::audio_url(None, Some("http://env:2"), Some("http://conf:3"), "http://default:4"),
        "http://env:2"
    );
    assert_eq!(
        settings::audio_url(None, None, Some("http://conf:3"), "http://default:4"),
        "http://conf:3"
    );
    assert_eq!(settings::audio_url(None, None, None, "http://default:4"), "http://default:4");
}

#[test]
fn sec_01_project_and_files_7_machine_files_s6_the_two_folders_come_from_the_environment() {
    // §7 puts everything under `~/.config/naivepost` and `$XDG_DATA_HOME`, where `~` is
    // `$XDG_CONFIG_HOME` when set (the prototype uses Go's `os.UserConfigDir`). A relative
    // value is not a location, so it is ignored rather than made absolute by the current
    // directory.
    let p = settings::paths_from(
        Some("/home/bocek/.config"),
        Some("/home/bocek"),
        Some("/srv/share"),
        None,
    )
    .unwrap();
    assert_eq!(p.config_dir, PathBuf::from("/home/bocek/.config/naivepost"));
    assert_eq!(p.data_dir, PathBuf::from("/srv/share/naivepost"));

    let p = settings::paths_from(None, Some("/home/bocek"), None, None).unwrap();
    assert_eq!(p.config_dir, PathBuf::from("/home/bocek/.config/naivepost"));
    assert_eq!(p.data_dir, PathBuf::from("/home/bocek/.local/share/naivepost"));

    // `XDG_DATA_HOME` beats the `$HOME` fallback and loses to `data_home`.
    let p = settings::paths_from(None, Some("/app"), None, Some("/xdg/data")).unwrap();
    assert_eq!(p.data_dir, PathBuf::from("/xdg/data/naivepost"));

    // Nothing usable: no paths at all, rather than a relative one. This is the case the
    // prototype refuses to write under ("nowhere to write the settings").
    assert!(settings::paths_from(Some("relative"), None, Some("relative-share"), None).is_none());
}

#[test]
fn sec_01_project_and_files_7_machine_files_s7_desktop_entry_points_at_the_real_launcher() {
    // "$XDG_DATA_HOME/applications/ch.bocek.naivepost.desktop … installed unless the app runs
    // from a build cache or a Flatpak (Flatpak exports both from the manifest; a file written
    // from inside would point into the sandbox)."
    let d = Dirs::new("desktop");

    // Inside a Flatpak, and out of a build cache: nothing is registered.
    assert!(!settings::wants_desktop_file(Path::new("/usr/bin/naivepost"), true, false));
    assert!(!settings::wants_desktop_file(Path::new("/usr/bin/naivepost"), false, true));
    // A source tree that was never installed has no launcher for the entry to name.
    assert!(!settings::wants_desktop_file(&d.root.join("target/debug/naivepost"), false, false));

    // An installed copy: yes.
    let exe = d.root.join("bin/naivepost");
    fs::create_dir_all(exe.parent().unwrap()).unwrap();
    fs::write(&exe, "#!/bin/sh\n").unwrap();
    assert!(settings::wants_desktop_file(&exe, false, false));

    let entry = settings::desktop_file(&exe);
    assert!(entry.contains("[Desktop Entry]"), "{entry}");
    assert!(entry.contains(&format!("Exec={}", exe.display())), "{entry}");
    assert!(entry.contains("MimeType=application/x-naivepost-project;"), "{entry}");

    // An install path with a space has to survive as one argument.
    let spaced = settings::desktop_file(Path::new("/home/bocek my app/naivepost"));
    assert!(spaced.contains("Exec=\"/home/bocek my app/naivepost\""), "{spaced}");

    // Installed under the data dir, at 0644 like every ordinary file.
    settings::install_desktop_file(&d.paths, &exe).unwrap();
    assert_eq!(mode(&d.paths.desktop_file()), 0o644);
    assert_eq!(fs::read_to_string(d.paths.desktop_file()).unwrap(), entry);

    fs::remove_dir_all(&d.root).unwrap();
}

#[test]
fn sec_01_project_and_files_7_machine_files_s7_mime_package_declares_the_project_type() {
    // "$XDG_DATA_HOME/mime/packages/ch.bocek.naivepost.xml (type
    // `application/x-naivepost-project`, glob `*.naivepost`)".
    let d = Dirs::new("mime");

    let xml = settings::mime_package();
    assert!(xml.contains("application/x-naivepost-project"), "{xml}");
    assert!(xml.contains("<glob pattern=\"*.naivepost\""), "{xml}");

    // Written only when the desktop entry is; a build cache writes neither.
    assert!(!d.paths.mime_package().exists());
    settings::install_mime_package(&d.paths).unwrap();
    assert_eq!(mode(&d.paths.mime_package()), 0o644);
    assert_eq!(fs::read_to_string(d.paths.mime_package()).unwrap(), xml);

    fs::remove_dir_all(&d.root).unwrap();
}

#[test]
fn sec_01_project_and_files_7_machine_files_s8_every_machine_file_is_under_the_two_folders() {
    // §1: "Directories 0755, files 0644, except prompts, the settings file and the machine's
    // watchdog dumps (0700 / 0600)." So exactly three kinds of file here are private, and no
    // machine-wide file lands in a project folder.
    let d = Dirs::new("modes");
    fs::create_dir_all(d.root.join("session.naivepost")).unwrap();

    settings::save(&d.paths, &Conf::default()).unwrap();
    settings::write_prompt(&d.paths, "narrate.rephrase", "terse", "shipped").unwrap();
    let hang = settings::write_hang_dump(
        &d.paths,
        "gtk",
        1,
        std::time::Duration::from_secs(3),
        &settings::hang_stamp(9, 19, 14, 30, 12),
    )
    .unwrap();
    settings::install_desktop_file(&d.paths, Path::new("/usr/bin/naivepost")).unwrap();
    settings::install_mime_package(&d.paths).unwrap();

    assert_eq!(mode(&d.paths.conf_path()), 0o600, "the settings file");
    assert_eq!(mode(&d.paths.prompt_file("narrate.rephrase")), 0o600, "prompts");
    assert_eq!(mode(&hang), 0o600, "watchdog dumps");
    assert_eq!(mode(&d.paths.desktop_file()), 0o644);
    assert_eq!(mode(&d.paths.mime_package()), 0o644);

    assert_eq!(
        walk(&d.root),
        vec![
            "cfg/naivepost/hang-0919-143012.txt",
            "cfg/naivepost/llm.conf",
            "cfg/naivepost/prompts/narrate.rephrase.txt",
            "data/naivepost/applications/ch.bocek.naivepost.desktop",
            "data/naivepost/mime/packages/ch.bocek.naivepost.xml",
        ],
        "everything machine-wide under the config or data folder"
    );
    assert_eq!(fs::read_dir(d.root.join("session.naivepost")).unwrap().count(), 0);

    fs::remove_dir_all(&d.root).unwrap();
}
