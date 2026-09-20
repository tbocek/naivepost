//! §03-shell.md **§6 The settings file** — `~/.config/naivepost/llm.conf`, `KEY="value"` lines,
//! written whole; every key of §6, the seven slot counts that mean 1 when absent, the precedence
//! dialog box → this file → legacy `<root>/llm.conf` (migrated once) → built-in default, the audio
//! and sd URLs honouring their environment variables below the dialog, and `SUBTITLE_LANGUAGES` as a
//! `code:tag:name` list.
//!
//! Item id: `sec_03_shell_6_the_settings_file`. The file's own rules (mode, folders) are §7 of
//! spec/01-project-and-files.md — already covered by `tests/machine_files.rs`, which this does not
//! repeat; the languages list is spec/00-principles.md §6.
//!
//! This item cites no `P.*` parameter and no `tool:*`.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use naivepost::roles;
use naivepost::settings::{self, Conf, Paths, Slots};
use naivepost::services as svc;

/// The item's id, interpolated into every assertion message so a failure names the item.
const ITEM: &str = "sec_03_shell_6_the_settings_file";

/// A throwaway config/data pair standing in for `$XDG_CONFIG_HOME`/`$XDG_DATA_HOME`, built by hand so
/// no test reads the real `$HOME`.
struct Dirs {
    root: PathBuf,
    paths: Paths,
}

impl Dirs {
    fn new(label: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "np-set-{label}-{pid}-{n}",
            pid = std::process::id(),
            n = COUNTER.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&root);
        let paths = Paths {
            config_dir: root.join("cfg/naivepost"),
            data_dir: root.join("data/naivepost"),
        };
        Self { root, paths }
    }
}

static COUNTER: AtomicUsize = AtomicUsize::new(0);

#[cfg(unix)]
fn mode(path: &Path) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).unwrap().permissions().mode() & 0o777
}

/// A `Conf` with every §6 key set to something, so "written whole" means all of them.
fn full_conf() -> Conf {
    let mut conf = Conf::default();
    conf.server = "http://llm:8731/v1".into();
    conf.model = "qwen".into();
    conf.key = "sk-llm".into();
    conf.audio_server = "http://cpp:8765".into();
    conf.audio_key = "sk-cpp".into();
    conf.voices = "/voices".into();
    conf.asr_model = "whisper".into();
    conf.diar_model = "pyannote".into();
    conf.tts_model = "indextts2".into();
    conf.sep_model = "demucs".into();
    conf.align_model = "qwen3-aligner".into();
    conf.ffmpeg = "/usr/bin/ffmpeg".into();
    conf.firefox = "/usr/bin/firefox".into();
    conf.sd_server = "http://sd:1234".into();
    conf.sd_key = "sk-sd".into();
    conf.subtitle_languages = "de:ger:German,en:eng:English".into();
    conf.slots = Slots { llm: 2, asr: 3, diar: 4, align: 5, tts: 6, sep: 7, sd: 8 };
    conf.projects
        .insert("/sessions/lecture".into(), "/sessions/lecture/naivepost.json".into());
    conf
}

/// §6 lists the keys and says the file is "written whole": every one survives a render + parse, the
/// slot counts are absent at their default of 1, a value that would break `source` is quoted and
/// read back verbatim, and a key from a newer build is carried through rather than dropped.
#[test]
fn sec_03_shell_6_the_settings_file_s1_every_key_of_the_settings_file_is_written_and_read_back() {
    let text = settings::render(&full_conf());
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
        "LLM_SLOTS",
        "AUDIOCPP_ASR_SLOTS",
        "AUDIOCPP_DIAR_SLOTS",
        "AUDIOCPP_ALIGN_SLOTS",
        "AUDIOCPP_TTS_SLOTS",
        "AUDIOCPP_SEP_SLOTS",
        "SD_SLOTS",
        "SUBTITLE_LANGUAGES",
    ] {
        assert!(text.contains(&format!("{key}=")), "{ITEM}: §6's key {key} is not written:\n{text}");
    }
    // The remembered project, as a numbered pair.
    assert!(text.contains("PROJECT_1_ROOT=") && text.contains("PROJECT_1_FILE="), "{ITEM}: no PROJECT_1 pair:\n{text}");

    // Whole-file round trip: parse(render(conf)) == conf.
    let back = settings::parse(&text).expect("the rendered file parses");
    assert_eq!(back, full_conf(), "{ITEM}: a key did not survive the round trip");

    // "absent = 1": a default machine writes none of the seven slot keys...
    let bare = settings::render(&Conf::default());
    for key in Slots::KEY_NAMES {
        assert!(!bare.contains(&format!("{key}=")), "{ITEM}: the default {key} should not be written:\n{bare}");
    }
    // ...and a file with none reads back as all 1.
    let defaults = settings::parse("LLM_SERVER=http://x\n").expect("one key parses");
    assert_eq!(defaults.slots, Slots::default(), "{ITEM}: absent slots are not all 1");

    // A value that would break `source` — quote, dollar, backtick, backslash — round-trips verbatim.
    let mut tricky = Conf::default();
    tricky.key = "a\"b$c`d\\e".into();
    let quoted = settings::render(&tricky);
    assert_eq!(
        settings::parse(&quoted).expect("the quoted key parses").key,
        "a\"b$c`d\\e",
        "{ITEM}: a credential was not carried verbatim:\n{quoted}"
    );

    // A newer build's key is passed through untouched: the file is written whole.
    let newer = settings::parse("TUNE_WHATEVER=7\n").expect("an unknown key parses");
    assert!(settings::render(&newer).contains("TUNE_WHATEVER="), "{ITEM}: a newer key was dropped");
}

/// §6's "(legacy `<root>/llm.conf`, migrated once)": a machine that has been cutting for months keeps
/// its endpoints, taken over into the config folder the first launch that finds no conf there — and
/// never read again once one is.
#[test]
fn sec_03_shell_6_the_settings_file_s2_a_legacy_file_beside_the_videos_is_taken_over_once() {
    // (a) No conf, a legacy file beside the videos: taken over, 0600, and left where it was.
    let d = Dirs::new("legacy-move");
    std::fs::create_dir_all(&d.root).unwrap();
    std::fs::write(settings::legacy_conf_path(&d.root), "LLM_SERVER=http://old:1\nLLM_API_KEY=k\n").unwrap();

    let moved = settings::migrate_legacy(&d.paths, &d.root).expect("the migration runs");
    assert_eq!(moved.as_deref(), Some(settings::legacy_conf_path(&d.root).as_path()), "{ITEM}: the legacy path is what gets logged");

    let conf = settings::read(&d.paths).expect("the migrated file reads");
    assert_eq!(conf.server, "http://old:1", "{ITEM}: the endpoint did not come across");
    assert_eq!(conf.key, "k", "{ITEM}: the key did not come across");
    assert_eq!(mode(&d.paths.conf_path()), 0o600, "{ITEM}: a file holding a key is 0600");
    assert!(settings::legacy_conf_path(&d.root).exists(), "{ITEM}: copied, never moved");

    // (b) A conf already present: the legacy is not read, and the existing file is not rewritten.
    let d = Dirs::new("legacy-kept");
    std::fs::create_dir_all(&d.root).unwrap();
    std::fs::write(settings::legacy_conf_path(&d.root), "LLM_SERVER=http://old:1\n").unwrap();
    let mut current = Conf::default();
    current.server = "http://new:1".into();
    settings::save(&d.paths, &current).expect("a conf exists");

    assert_eq!(settings::migrate_legacy(&d.paths, &d.root).expect("nothing to do"), None, "{ITEM}: a legacy file must not be taken over once a conf exists");
    assert_eq!(settings::read(&d.paths).expect("reads").server, "http://new:1", "{ITEM}: the existing conf was overwritten");

    // (c) No legacy file at all: nothing to migrate, and no conf created.
    let d = Dirs::new("legacy-none");
    assert_eq!(settings::migrate_legacy(&d.paths, &d.root).expect("nothing to do"), None, "{ITEM}: a legacy file must not be taken over once a conf exists");
    assert!(!d.paths.conf_path().exists(), "{ITEM}: an empty migration writes no file");
}

/// §6's precedence: dialog box → this file → legacy (migrated once) → built-in default, and an empty
/// answer is not an answer at any level. Plus the path §6 names is the one `paths_from` builds.
#[test]
fn sec_03_shell_6_the_settings_file_s3_precedence_is_dialog_then_file_then_legacy_then_default() {
    let (dialog, file, legacy, default) =
        (Some("http://dialog:1"), Some("http://file:2"), Some("http://legacy:3"), "http://default:4");

    assert_eq!(settings::resolve(dialog, file, legacy, default), "http://dialog:1", "{ITEM}: the box wins");
    assert_eq!(settings::resolve(None, file, legacy, default), "http://file:2", "{ITEM}: then this file");
    assert_eq!(settings::resolve(None, None, legacy, default), "http://legacy:3", "{ITEM}: then the legacy");
    assert_eq!(settings::resolve(None, None, None, default), "http://default:4", "{ITEM}: then the default");

    // An empty string is not an answer at any level.
    assert_eq!(settings::resolve(Some(""), file, legacy, default), "http://file:2", "{ITEM}: an empty box falls through");
    assert_eq!(settings::resolve(None, Some(""), legacy, default), "http://legacy:3", "{ITEM}: an empty file value falls through");
    assert_eq!(settings::resolve(None, None, Some(""), default), "http://default:4", "{ITEM}: an empty legacy value falls through");

    // The path §6 names — `~/.config/naivepost/llm.conf` — is the one a bare `$HOME` builds.
    let paths = settings::paths_from(None, Some("/home/bocek"), None, None).expect("$HOME gives paths");
    assert_eq!(
        paths.conf_path().to_str().unwrap(),
        "/home/bocek/.config/naivepost/llm.conf",
        "{ITEM}: not the path §6 names"
    );
}

/// §6: "The audio URL also honours `NAIVEPOST_TTS_URL` and `AUDIOCPP_SERVER` below the dialog; sd
/// `SD_SERVER`." The environment sits under the box, and the LLM server is the exception — it has no
/// env override at all (spec/01 §1), so a stale export cannot outrank what the dialog shows.
#[test]
fn sec_03_shell_6_the_settings_file_s4_the_environment_sits_below_the_dialog_box() {
    let env = |pairs: &[(&str, &str)]| {
        let owned: Vec<(String, String)> =
            pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        move |name: &str| owned.iter().find(|(k, _)| k == name).map(|(_, v)| v.clone())
    };

    // audio.cpp: the box, then NAIVEPOST_TTS_URL, then AUDIOCPP_SERVER, then the loopback.
    let empty = Conf::default();
    assert_eq!(
        svc::audio_endpoint(&empty, env(&[("NAIVEPOST_TTS_URL", "http://tts-var:1")])).url,
        "http://tts-var:1",
        "{ITEM}: NAIVEPOST_TTS_URL is honoured below the box"
    );
    assert_eq!(
        svc::audio_endpoint(&empty, env(&[("AUDIOCPP_SERVER", "http://cpp-var:2")])).url,
        "http://cpp-var:2",
        "{ITEM}: AUDIOCPP_SERVER is honoured below the box"
    );
    // NAIVEPOST_TTS_URL moves only this app, so it wins over the shared variable.
    assert_eq!(
        svc::audio_endpoint(&empty, env(&[("AUDIOCPP_SERVER", "http://cpp-var:2"), ("NAIVEPOST_TTS_URL", "http://tts-var:1")])).url,
        "http://tts-var:1",
        "{ITEM}: NAIVEPOST_TTS_URL outranks AUDIOCPP_SERVER"
    );
    // The box beats both.
    let mut boxed = Conf::default();
    boxed.audio_server = "http://box:9".into();
    assert_eq!(
        svc::audio_endpoint(&boxed, env(&[("AUDIOCPP_SERVER", "http://cpp-var:2"), ("NAIVEPOST_TTS_URL", "http://tts-var:1")])).url,
        "http://box:9",
        "{ITEM}: the box wins over the environment"
    );
    // Blank env values fall through to the loopback.
    assert_eq!(
        svc::audio_endpoint(&empty, env(&[("AUDIOCPP_SERVER", ""), ("NAIVEPOST_TTS_URL", "  ")])).url,
        svc::audio_default(),
        "{ITEM}: blank env values are not answers"
    );

    // sd.cpp: the box, then SD_SERVER, then the loopback.
    assert_eq!(svc::image_endpoint(&empty, env(&[("SD_SERVER", "http://sd-var:3")])).url, "http://sd-var:3", "{ITEM}: SD_SERVER is honoured below the box");
    let mut sd_boxed = Conf::default();
    sd_boxed.sd_server = "http://sd-box:9".into();
    assert_eq!(svc::image_endpoint(&sd_boxed, env(&[("SD_SERVER", "http://sd-var:3")])).url, "http://sd-box:9", "{ITEM}: the sd box wins");
    assert_eq!(svc::image_endpoint(&empty, env(&[])).url, svc::image_default(), "{ITEM}: sd falls back to the loopback");

    // The LLM server has NO env override (spec/01 §1): `llm_endpoint` takes only a conf — there is no
    // closure through which an exported variable could reach it. Assert the box and the default.
    let mut llm = Conf::default();
    llm.server = "http://llm-box:8731".into();
    assert_eq!(svc::llm_endpoint(&llm).url, "http://llm-box:8731", "{ITEM}: the LLM box is used");
    assert_eq!(svc::llm_endpoint(&Conf::default()).url, svc::llm_default(), "{ITEM}: an empty LLM box falls to the default, not an env var");
}

/// §6's `SUBTITLE_LANGUAGES` (code:tag:name list), which spec/00-principles.md §6 makes "a settings
/// list (code, ISO-639-2 tag, name), not three constants" — so its length is whatever the value says.
#[test]
fn sec_03_shell_6_the_settings_file_s5_subtitle_languages_is_a_codetagname_list() {
    let conf = settings::parse("SUBTITLE_LANGUAGES=\"de:ger:German, fr:fre:French, en:eng:English\"\n")
        .expect("the list parses");

    let langs = roles::subtitle_languages(&conf.subtitle_languages);
    assert_eq!(langs.len(), 3, "{ITEM}: three entries in the value, three back");
    // Order kept, and each field split.
    assert_eq!((langs[0].code.as_str(), langs[0].tag.as_str(), langs[0].name.as_str()), ("de", "ger", "German"), "{ITEM}: the first entry's fields");
    assert_eq!((langs[2].code.as_str(), langs[2].tag.as_str(), langs[2].name.as_str()), ("en", "eng", "English"), "{ITEM}: the last entry's fields, order kept");

    // `ask_for` is the name a prompt reads; the code and tag are for filenames.
    assert_eq!(langs[1].ask_for(), "French", "{ITEM}: ask_for is the name");

    // A settings list, not three constants: five entries give five back.
    let five = roles::subtitle_languages("de:ger:German,fr:fre:French,en:eng:English,pl:pol:Polish,es:spa:Spanish");
    assert_eq!(five.len(), 5, "{ITEM}: the list is not capped at three");

    // An empty value is an empty list.
    assert!(roles::subtitle_languages("").is_empty(), "{ITEM}: empty means none");

    // A shorter entry fills what it left out from the code (the documented rule).
    let alone = &roles::subtitle_languages("pl")[0];
    assert_eq!((alone.code.as_str(), alone.tag.as_str(), alone.name.as_str()), ("pl", "pl", "pl"), "{ITEM}: a bare code fills tag and name");
}
