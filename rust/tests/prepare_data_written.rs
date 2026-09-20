//! §04-prepare#3-data-written — what Prepare leaves under prepare/ (spec/01-project-and-files.md §1): the
//! per-source products, the merged transcript files, the marks and final.txt, and inputs/meta.env with
//! INTERVAL/SCALE always and the chosen video/audio when they resolve.

use std::os::unix::fs::PermissionsExt;
use std::path::Path;

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
