//! §04-prepare#3-data-written — what Prepare leaves under prepare/ (spec/01-project-and-files.md §1): the
//! per-source products, the merged transcript files, the marks and final.txt, and inputs/meta.env with
//! INTERVAL/SCALE always and the chosen video/audio when they resolve.

use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use adw::prelude::*;
use naivepost::layout::Tree;
use naivepost::prepare_data as data;
use naivepost::project::{Project, Source};
use naivepost::requests;

const SOURCE: &str = "lecture.mkv";

fn tree(tag: &str) -> Tree {
    let dir = std::env::temp_dir().join(format!(
        "naivepost-prepdata-{tag}-{}-{}.naivepost",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    Tree::new(&dir).unwrap()
}

fn mode(path: &Path) -> u32 {
    std::fs::metadata(path).unwrap().permissions().mode() & 0o777
}

fn source(path: &str, footage: bool, narrator: u32) -> Source {
    Source { path: path.into(), footage, narrator, ..Default::default() }
}

/// `Project` has no `Default`: §1's file is what fills the rest in, so a test says what it means.
fn session(sources: Vec<Source>, interval: f64) -> Project {
    Project { sources, interval, ..serde_json::from_str("{}").unwrap() }
}

fn value<'a>(pairs: &'a [(String, String)], key: &str) -> Option<&'a str> {
    pairs.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
}

// --- S1: the prepare/ tree is what §1 spells, at the modes §1 states -------------------------------

#[test]
fn sec_04_prepare_3_data_written_s1_every_file_the_block_names_has_its_path() {
    let tree = tree("paths");
    // The spec's own spelling, checked against the accessor rather than trusted from it.
    let spelled = [
        "inputs/meta.env",
        "inputs/lecture.mkv/voice16k.wav",
        "inputs/lecture.mkv/transcript.txt",
        "inputs/lecture.mkv/transcript.tsv",
        "inputs/lecture.mkv/transcript.srt",
        "inputs/lecture.mkv/words.json",
        "inputs/lecture.mkv/asrchunks.json",
        "inputs/lecture.mkv/words.aligned.json",
        "inputs/lecture.mkv/turns.json",
        "inputs/lecture.mkv/asr",
        "inputs/lecture.mkv/diar",
        "inputs/frames/lecture.mkv/scenes.tsv",
        "inputs/frames/lecture.mkv/.frames",
        "describe/lecture.mkv/events.tsv",
        "describe/lecture.mkv/state.txt",
        "describe/lecture.mkv/.llmframes",
        "transcript/lecture.mkv/transcript.fixed.tsv",
        "transcript/lecture.mkv/commentary.fixed.tsv",
        "transcript/lecture.mkv/subtitles.srt",
        "transcript/session.tsv",
        "transcript/session.txt",
        "transcript/offsets.tsv",
        "transcript/retakes.tsv",
        "transcript/final.txt",
    ];
    let written = data::written(&tree, SOURCE);
    assert_eq!(written.len(), spelled.len());
    for ((what, where_), want) in written.iter().zip(spelled) {
        assert_eq!(where_, want, "prepare/{want} — {}", what);
    }

    // And each path really is under the project's prepare/.
    for (what, path) in data::paths(&tree, SOURCE) {
        assert!(path.starts_with(tree.prepare_dir()), "{what}: {}", path.display());
    }
}

#[test]
fn sec_04_prepare_3_data_written_s1_directories_are_0755_and_files_0644() {
    let tree = tree("modes");
    for (what, path) in data::paths(&tree, SOURCE) {
        // The two scratch folders and the scaled-frame folder are folders; everything else is a file.
        if matches!(what, "ASR scratch" | "diarization scratch" | "the frames sent, scaled") {
            std::fs::create_dir_all(&path).unwrap();
            assert_eq!(mode(&path), 0o755, "{what}");
            continue;
        }
        let parent = path.parent().unwrap();
        std::fs::create_dir_all(parent).unwrap();
        std::fs::write(&path, b"x").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(path.exists(), "{what}");
        assert_eq!(mode(&path), 0o644, "{what}");
    }
    // The folders §1 counts were made at 0755 by the way they came to exist.
    assert_eq!(mode(&tree.prepare_dir()), 0o755);
    assert_eq!(mode(&tree.meta_env().parent().unwrap()), 0o755);
}

// --- S2: inputs/meta.env, §1's keys and its conditionals ------------------------------------------

#[test]
fn sec_04_prepare_3_data_written_s2_footage_and_a_narrator_write_all_six_keys() {
    let tree = tree("full");
    let project = session(
        vec![
            source("project:sources/lecture.mkv", true, 0),
            source("project:sources/mic.wav", false, 1),
        ],
        1.0,
    );

    let pairs = data::meta(&project, Path::new("/tmp"), tree.dir());
    assert_eq!(
        pairs.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>(),
        vec![
            "INTERVAL", "SCALE", "VIDEO_FILE", "VIDEO_BASE", "AUDIO_FILE", "AUDIO_BASE"
        ]
    );
    // INTERVAL is a plain number: the file is for humans and scripts.
    assert_eq!(value(&pairs, "INTERVAL"), Some("1"));
    assert_eq!(value(&pairs, "SCALE"), Some(data::SCALES));
    assert_eq!(value(&pairs, "VIDEO_FILE"), Some("project:sources/lecture.mkv"));
    assert_eq!(value(&pairs, "VIDEO_BASE"), Some("lecture"));
    assert_eq!(value(&pairs, "AUDIO_FILE"), Some("project:sources/mic.wav"));
    assert_eq!(value(&pairs, "AUDIO_BASE"), Some("mic"));

    // A frame interval with a fraction keeps it.
    let pairs = data::meta(&session(sources_of(&project), 0.25), Path::new("/tmp"), tree.dir());
    assert_eq!(value(&pairs, "INTERVAL"), Some("0.25"));
}

fn sources_of(project: &Project) -> Vec<Source> {
    project.sources.clone()
}

#[test]
fn sec_04_prepare_3_data_written_s2_a_recording_only_session_has_no_video_keys() {
    let tree = tree("audio");
    let project = session(vec![source("project:sources/mic.wav", false, 1)], 1.0);
    let pairs = data::meta(&project, Path::new("/tmp"), tree.dir());

    assert_eq!(
        pairs.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>(),
        vec!["INTERVAL", "SCALE", "AUDIO_FILE", "AUDIO_BASE"]
    );
    assert!(pairs.iter().all(|(k, _)| k != "VIDEO_FILE"), "{pairs:?}");
    assert!(pairs.iter().all(|(k, _)| k != "VIDEO_BASE"));
}

#[test]
fn sec_04_prepare_3_data_written_s2_no_narrator_in_slot_one_writes_no_audio_keys() {
    let tree = tree("nonarr");
    // Every row untagged: narrator 1 does not resolve, so §1's "when narrator 1 resolves" holds the pair out.
    let project = session(
        vec![
            source("project:sources/lecture.mkv", true, 0),
            source("project:sources/mic.wav", false, 0),
        ],
        1.0,
    );
    let pairs = data::meta(&project, Path::new("/tmp"), tree.dir());

    assert_eq!(value(&pairs, "VIDEO_FILE"), Some("project:sources/lecture.mkv"));
    assert!(pairs.iter().all(|(k, _)| k != "AUDIO_FILE"), "{pairs:?}");
    assert!(pairs.iter().all(|(k, _)| k != "AUDIO_BASE"));
}

#[test]
fn sec_04_prepare_3_data_written_s2_footage_means_the_row_with_the_tick() {
    let tree = tree("tick");
    // A video row with 🎥 off is footage for nobody: the ticked one is what the run reads.
    let project = session(
        vec![
            source("project:sources/b-roll.mp4", false, 0),
            source("project:sources/lecture.mkv", true, 0),
        ],
        1.0,
    );
    let pairs = data::meta(&project, Path::new("/tmp"), tree.dir());

    assert_eq!(value(&pairs, "VIDEO_FILE"), Some("project:sources/lecture.mkv"));
    assert_eq!(value(&pairs, "VIDEO_BASE"), Some("lecture"));

    // And a ticked row that is not a video at all is not the footage file either.
    let project = session(vec![source("project:sources/mic.wav", true, 0)], 1.0);
    let pairs = data::meta(&project, Path::new("/tmp"), tree.dir());
    assert!(pairs.iter().all(|(k, _)| k != "VIDEO_FILE"), "{pairs:?}");
}

#[test]
fn sec_04_prepare_3_data_written_s2_the_file_is_key_value_lines() {
    let tree = tree("lines");
    let project = session(
        vec![
            source("project:sources/lecture.mkv", true, 0),
            source("project:sources/mic.wav", false, 1),
        ],
        1.0,
    );

    data::write_meta(&tree, &project, Path::new("/tmp")).unwrap();
    let written = std::fs::read_to_string(tree.meta_env()).unwrap();
    assert_eq!(
        written,
        "INTERVAL=1\nSCALE=native\nVIDEO_FILE=project:sources/lecture.mkv\nVIDEO_BASE=lecture\n\
         AUDIO_FILE=project:sources/mic.wav\nAUDIO_BASE=mic\n"
    );
    // Read back by the reader every step uses.
    assert_eq!(requests::read_meta(&tree).unwrap(), data::meta(&project, Path::new("/tmp"), tree.dir()));
}

// --- S3: the file is the record of a read ----------------------------------------------------------

#[test]
fn sec_04_prepare_3_data_written_s3_its_existence_is_what_says_the_sources_were_read() {
    let tree = tree("record");
    assert!(!tree.meta_env().exists());
    assert_eq!(requests::read_meta(&tree).unwrap(), Vec::<(String, String)>::new());

    let project = session(vec![source("project:sources/lecture.mkv", true, 0)], 1.0);
    data::write_meta(&tree, &project, Path::new("/tmp")).unwrap();
    assert!(tree.meta_env().exists());
    assert_eq!(mode(&tree.meta_env()), 0o644);

    // The latest read is the only one that means anything: a rewrite replaces rather than appends.
    let edited = session(sources_of(&project), 2.0);
    data::write_meta(&tree, &edited, Path::new("/tmp")).unwrap();
    let pairs = requests::read_meta(&tree).unwrap();
    assert_eq!(value(&pairs, "INTERVAL"), Some("2"));
    assert_eq!(pairs.iter().filter(|(k, _)| k == "INTERVAL").count(), 1);
}

#[test]
fn sec_04_prepare_3_data_written_s3_the_stored_form_survives_a_save_as() {
    // §1's one path rule: inside the project → `project:`, under the root → root-relative, else absolute.
    // meta.env stores what the session stores, so all three forms have to come back unchanged — that is
    // what makes a moved project (F0.10) still find its sources.
    let tree = tree("forms");
    let root = std::env::temp_dir().join(format!("npd-root-{}", std::process::id()));
    std::fs::create_dir_all(root.join("shared")).unwrap();
    let far = std::env::temp_dir().join(format!("npd-far-{}", std::process::id()));
    std::fs::create_dir_all(&far).unwrap();

    let under_root = format!("{}/shared/clip.mkv", root.display());
    let outside = format!("{}/elsewhere.mkv", far.display());
    let cases = [
        ("project:sources/lecture.mkv", "project:sources/lecture.mkv", "lecture"),
        (under_root.as_str(), "shared/clip.mkv", "clip"),
        (outside.as_str(), outside.as_str(), "elsewhere"),
    ];
    for (written, want, base) in cases {
        let project = session(vec![source(written, true, 0)], 1.0);
        let pairs = data::meta(&project, &root, tree.dir());
        assert_eq!(value(&pairs, "VIDEO_FILE"), Some(want), "{written}");
        assert_eq!(value(&pairs, "VIDEO_BASE"), Some(base), "{written}");
    }
}

#[test]
fn sec_04_prepare_3_data_written_s2_the_keys_are_the_six_the_block_lists() {
    // §1 lists exactly these and in this order.
    assert_eq!(
        data::META_KEYS,
        ["INTERVAL", "SCALE", "VIDEO_FILE", "VIDEO_BASE", "AUDIO_FILE", "AUDIO_BASE"]
    );
    // SCALE is a value rather than a preset: frames stay at the video's own size (spec/01 §7).
    assert_eq!(data::SCALES, "native");
}

// --- S4: a run stamps the marker -------------------------------------------------------------------
//
// §3 says a ▶ Prepare run writes the tree, and `meta.env` is the first file §1 lists: "the sources were
// read". Above, `meta`/`write_meta` are proven as data; below they are proven as something the step does.
// Every check here takes the ENV guard: `stamp_meta` resolves the session and the root off the process's
// working folder, and cargo runs these as threads.

/// A session folder pinned as the process's working folder, which is how a ▶ resolves the session and
/// the root its paths are written against (§F1.1's own `startup::session_dir(cwd)`). `stamp_meta` reads
/// both off the process, so a test that wants to drive it has to move the process — hence the guard:
/// `std::env` is process state and cargo runs these as threads.
struct Pinned {
    /// Held so no other thread moves the process's working folder mid-check; read only by the `Drop`
    /// below, which is why the field is never otherwise named.
    #[allow(dead_code)]
    guard: std::sync::MutexGuard<'static, ()>,
    /// Same: kept so the scene's folder outlives the checks that pinned it.
    #[allow(dead_code)]
    root: std::path::PathBuf,
    tree: Tree,
}

impl Pinned {
    fn new(tag: &str) -> Self {
        // The env and the working folder belong to the PROCESS and cargo runs these tests as threads, so
        // whoever moves them moves them one at a time (the same rule `tests/f1_3_run_path.rs` holds to).
        let guard = ENV
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let root = std::env::temp_dir().join(format!(
            "npd-pinned-{tag}-{}-{}",
            std::process::id(),
            STD_COUNT.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&root);
        let session = naivepost::startup::session_dir(&root);
        std::fs::create_dir_all(&session).unwrap();
        std::env::set_current_dir(&root).expect("the working folder is the scene");
        Pinned { guard, root, tree: Tree::new(&session).unwrap() }
    }

    /// The `root` a ▶ would resolve from this scene's working folder — what `stamp_meta` uses.
    fn root_dir(&self) -> std::path::PathBuf {
        data::default_root()
    }

    /// Plant a recording inside the project so a row can point at a file that exists.
    fn plant(&self, name: &str) {
        let sources = self.tree.sources_dir();
        std::fs::create_dir_all(&sources).unwrap();
        // A real silent 16 kHz mono wav: any stage that decodes this row reads RIFF back, and placeholder
        // bytes fail honestly ("not a RIFF/WAVE file") rather than standing in for audio.
        std::fs::write(sources.join(name), silent_wav_bytes(1.0)).unwrap();
    }
}

impl Drop for Pinned {
    fn drop(&mut self) {
        std::env::set_current_dir(env!("CARGO_MANIFEST_DIR")).expect("back to the crate folder");
    }
}

static ENV: std::sync::Mutex<()> = std::sync::Mutex::new(());
static STD_COUNT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

#[test]
fn sec_04_prepare_3_data_written_s4_a_prepare_run_writes_meta_env() {
    let scene = Pinned::new("run");
    // One footage row and one narrator in slot 1: both of §1's conditionals resolve.
    let project = session(
        vec![
            source("project:sources/lecture.mkv", true, 0),
            source("project:sources/mic.wav", false, 1),
        ],
        0.5,
    );

    let started = naivepost::prepare_run::begin(&project, &scene.tree);
    assert!(
        matches!(started, naivepost::prepare_run::Start::Started { .. }),
        "a valid session starts and stamps its marker: {started:?}"
    );
    assert!(!started_lines(&started).iter().any(|line| line.contains("meta.env")), "nothing to say about a marker that wrote: {:?}", started_lines(&started));

    assert!(scene.tree.meta_env().exists(), "the run left §1's marker behind");
    assert_eq!(mode(&scene.tree.meta_env()), 0o644, "§1: files are 0644");
    // Round-tripped through the reader §1's one reader is, not off a string the test made up.
    let back = requests::read_meta(&scene.tree).expect("readable");
    assert_eq!(back, data::meta(&project, &scene.root_dir(), scene.tree.dir()));
    assert_eq!(value(&back, "INTERVAL"), Some("0.5"), "the session's own Freq");
    assert_eq!(value(&back, "SCALE"), Some(data::SCALES));
    assert_eq!(value(&back, "VIDEO_BASE"), Some("lecture"));
    assert_eq!(value(&back, "AUDIO_BASE"), Some("mic"));
    // §1's path rule: inside the project → `project:` + root-relative.
    assert_eq!(value(&back, "VIDEO_FILE"), Some("project:sources/lecture.mkv"));
}

#[test]
fn sec_04_prepare_3_data_written_s4_a_recording_only_run_writes_no_video_keys() {
    let scene = Pinned::new("audio-only");
    // No 🎥 row at all: §1 writes the audio pair and leaves the video pair out entirely.
    let project = session(vec![source("project:sources/mic.wav", false, 1)], 1.0);

    assert!(
        matches!(
            naivepost::prepare_run::begin(&project, &scene.tree),
            naivepost::prepare_run::Start::Started { .. }
        ),
        "a recording-only session still starts"
    );
    let back = requests::read_meta(&scene.tree).expect("the marker is there");
    assert_eq!(value(&back, "AUDIO_BASE"), Some("mic"), "the narrator resolved: {back:?}");
    assert_eq!(value(&back, "VIDEO_FILE"), None, "nothing was footage: {back:?}");
    assert_eq!(value(&back, "VIDEO_BASE"), None);
}

#[test]
fn sec_04_prepare_3_data_written_s4_a_meta_env_that_will_not_write_does_not_stop_the_run() {
    let scene = Pinned::new("blocked");
    // A FILE where `prepare/inputs` has to be: the marker cannot be written, and nothing else about the
    // session is wrong.
    std::fs::create_dir_all(scene.tree.prepare_dir()).unwrap();
    std::fs::write(scene.tree.prepare_dir().join("inputs"), b"not a folder").unwrap();
    let project = session(vec![source("project:sources/lecture.mkv", true, 0)], 1.0);

    let started = naivepost::prepare_run::begin(&project, &scene.tree);
    let lines = match &started {
        naivepost::prepare_run::Start::Started { lines, .. } => lines.clone(),
        other => panic!("a marker that will not write is not a reason to refuse the press: {other:?}"),
    };
    assert!(
        lines.iter().any(|line| line.contains("meta.env") && line.starts_with(">>>")),
        "the refusal is said where the press is answered: {lines:?}"
    );
    assert!(!scene.tree.meta_env().exists(), "and it really did not write");
    // The run's own opening lines are all still there: the marker is a marker, not a gate.
    assert!(
        lines.iter().any(|line| line.starts_with(">>> prepare: 1 input file")),
        "S4's opening still went out: {lines:?}"
    );
}

/// `seconds` of silence as a 16 kHz mono 16-bit PCM wav. §F1.3 S1's decode reads its input with ffprobe
/// and ffmpeg for real, so a scene that lets a stage run needs a real file, not a placeholder.
fn silent_wav_bytes(seconds: f64) -> Vec<u8> {
    let samples = (seconds * 16000.0) as usize;
    let data = samples * 2;
    let mut bytes = Vec::with_capacity(44 + data);
    let put = |bytes: &mut Vec<u8>, value: u32| bytes.extend_from_slice(&value.to_le_bytes());
    let put16 = |bytes: &mut Vec<u8>, value: u16| bytes.extend_from_slice(&value.to_le_bytes());
    bytes.extend_from_slice(b"RIFF");
    put(&mut bytes, 36 + data as u32);
    bytes.extend_from_slice(b"WAVEfmt ");
    put(&mut bytes, 16);
    put16(&mut bytes, 1);
    put16(&mut bytes, 1);
    put(&mut bytes, 16000);
    put(&mut bytes, 32000);
    put16(&mut bytes, 2);
    put16(&mut bytes, 16);
    bytes.extend_from_slice(b"data");
    put(&mut bytes, data as u32);
    bytes.resize(44 + data, 0);
    bytes
}

/// The lines a `Start::Started` carries (empty for any other ending, which the caller asserts against).
fn started_lines(started: &naivepost::prepare_run::Start) -> Vec<String> {
    match started {
        naivepost::prepare_run::Start::Started { lines, .. } => lines.clone(),
        _ => Vec::new(),
    }
}

/// §3's marker left behind by a click on the real ▶ (`ui::play_button`, pressed the way GTK does).
///
/// This is the wire check the logic doors above cannot make: `prepare_run::begin` is public, so a rule
/// test can call it and pass — and a ▶ that never reached it would still have passed every one of them.
/// The session is one short take (§F1.3 S2's floor: nothing is asked of any server for it), so the press
/// walks to its end with no audio.cpp behind it and the only files it leaves are the ones the step writes
/// before and around its stages.
#[test]
fn sec_04_prepare_3_data_written_s4_a_prepare_press_leaves_meta_env_behind() {
    // A fake audio.cpp answering `/health` and `/v1/models`: §S5's preflight refuses a press whose
    // server is silent, so a press that is supposed to walk needs one that answers. One conf for the
    // whole binary — `XDG_CONFIG_HOME` is process state, and every check here presses ▶.
    use std::io::{BufRead, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let port = listener.local_addr().expect("an address").port();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            std::thread::spawn(move || {
                let mut reader = std::io::BufReader::new(stream.try_clone().expect("clone"));
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap_or(0) == 0 {
                    return;
                }
                loop {
                    let mut header = String::new();
                    if reader.read_line(&mut header).unwrap_or(0) == 0 || header.trim().is_empty() {
                        break;
                    }
                }
                let payload = if line.contains("/v1/models") {
                    r#"{"data":[{"id":"nemotron-asr","task":"asr"},{"id":"sortformer-diar","task":"diar"}]}"#
                } else {
                    "{}"
                };
                let reply = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{payload}",
                    payload.len()
                );
                let _ = stream.write_all(reply.as_bytes());
                let _ = stream.flush();
            });
        }
    });
    let config = std::env::temp_dir().join(format!("npd-wcfg-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&config);
    let paths = naivepost::settings::Paths {
        config_dir: config.join("naivepost"),
        data_dir: config.join("data"),
    };
    let mut conf = naivepost::settings::Conf::default();
    conf.audio_server = format!("http://127.0.0.1:{port}");
    naivepost::settings::save(&paths, &conf).expect("the conf points at the fake server");

    let scene = Pinned::new("press");
    std::env::set_var("XDG_CONFIG_HOME", &config);
    std::env::set_var("XDG_DATA_HOME", config.join("data"));
    scene.plant("mic.wav");
    let project = session(vec![source("project:sources/mic.wav", false, 1)], 1.0);
    assert!(!scene.tree.meta_env().exists(), "nothing stamped before the click");
    // The same path the press will resolve off cwd; `connect_activate` needs a `'static` capture, and
    // the pinned scene's guard is held by this test until after `run_with_args` returns.
    let session_path = naivepost::startup::session_dir(&std::env::current_dir().unwrap());

    let app = adw::Application::builder()
        .application_id(naivepost::ui::APP_ID)
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    // `connect_activate`'s closure must be `'static`, so what it reports through is shared rather than
    // borrowed, and the path it checks is a copy, not a borrow of the pinned scene.
    let stamped = std::rc::Rc::new(std::cell::Cell::new(false));
    let took = stamped.clone();
    app.connect_activate(move |app| {
        let window = naivepost::ui::build_window(app, &project, "Prepare");
        window.present();
        naivepost::ui::play_button(&window)
            .expect("the run bar has a ▶")
            .emit_clicked();
        took.set(
            Tree::new(&session_path)
                .expect("the session the press worked in")
                .meta_env()
                .exists(),
        );
        app.quit();
    });
    app.run_with_args::<String>(&[]);
    std::env::remove_var("XDG_CONFIG_HOME");
    std::env::remove_var("XDG_DATA_HOME");

    assert!(
        stamped.get(),
        "a click on ▶ left {} behind",
        scene.tree.meta_env().display()
    );
    let back = requests::read_meta(&scene.tree).expect("the marker reads back");
    assert_eq!(value(&back, "AUDIO_BASE"), Some("mic"), "with this session's own keys: {back:?}");
    assert_eq!(value(&back, "INTERVAL"), Some("1"));
}
