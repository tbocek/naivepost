//! §01-project-and-files#1-layout — the project folder's layout, its modes, and
//! the one path rule.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use naivepost::layout::{self, Tree};

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "naivepost-layout-{}-{tag}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

/// A project folder inside a throwaway application root: `<root>/<name>.naivepost`.
fn fixture(tag: &str) -> (PathBuf, Tree) {
    let root = temp_dir(tag);
    let dir = root.join("demo.naivepost");
    std::fs::create_dir_all(&dir).unwrap();
    let tree = Tree::new(&dir).expect("a .naivepost folder is a project");
    (root, tree)
}

fn mode(path: &Path) -> u32 {
    std::fs::metadata(path).unwrap().permissions().mode() & 0o777
}

#[test]
fn sec_01_project_and_files_1_layout_folder_suffix_is_the_project_check() {
    let root = temp_dir("suffix");
    assert!(Tree::new(root.join("demo.naivepost")).is_ok());
    // The folder name is the whole test: no marker file, no stored location.
    let err = Tree::new(root.join("demo")).unwrap_err();
    assert!(err.contains(".naivepost"), "{err}");
    assert!(Tree::new(root.join("input_video")).is_err());

    let tree = Tree::new(root.join("my talk.naivepost").to_str().unwrap()).unwrap();
    assert_eq!(tree.name(), Some("my talk"));
    assert_eq!(tree.dir(), root.join("my talk.naivepost").as_path());
}

#[test]
fn sec_01_project_and_files_1_layout_paths_match_the_spec_block() {
    let (root, t) = fixture("spec-block");
    let here = |rel: &str| root.join("demo.naivepost").join(rel);

    assert_eq!(t.project_file(), here("naivepost.json"));
    assert_eq!(t.sources_dir(), here("sources"));
    assert_eq!(t.copy_part("cam.mkv"), here("sources/cam.mkv.part"));
    assert_eq!(t.stems_dir(), here("stems"));
    assert_eq!(t.split_voice("mic"), here("stems/mic.split-voice.wav"));
    assert_eq!(t.split_novoice("cam", "mkv"), here("stems/cam.split-novoice.mkv"));

    assert_eq!(t.meta_env(), here("prepare/inputs/meta.env"));
    assert_eq!(t.voice16k_wav("mic"), here("prepare/inputs/mic/voice16k.wav"));
    assert_eq!(t.transcript_txt("mic"), here("prepare/inputs/mic/transcript.txt"));
    assert_eq!(t.transcript_tsv("mic"), here("prepare/inputs/mic/transcript.tsv"));
    assert_eq!(t.transcript_srt("mic"), here("prepare/inputs/mic/transcript.srt"));
    assert_eq!(t.words_json("mic"), here("prepare/inputs/mic/words.json"));
    assert_eq!(t.asrchunks_json("mic"), here("prepare/inputs/mic/asrchunks.json"));
    assert_eq!(
        t.words_aligned_json("mic"),
        here("prepare/inputs/mic/words.aligned.json")
    );
    assert_eq!(t.turns_json("mic"), here("prepare/inputs/mic/turns.json"));
    assert_eq!(
        t.frames_dir("cam"),
        here("prepare/inputs/frames/cam")
    );
    assert_eq!(t.events_tsv("cam"), here("prepare/describe/cam/events.tsv"));
    assert_eq!(t.describe_state("cam"), here("prepare/describe/cam/state.txt"));
    // P.machine.describeFrameWidth scales what goes into .llmframes/
    assert_eq!(t.llm_frames_dir("cam"), here("prepare/describe/cam/.llmframes"));
    assert_eq!(
        t.transcript_src_dir("mic"),
        here("prepare/transcript/mic")
    );
    assert_eq!(t.session_tsv(), here("prepare/transcript/session.tsv"));
    assert_eq!(t.session_txt(), here("prepare/transcript/session.txt"));
    assert_eq!(t.offsets_tsv(), here("prepare/transcript/offsets.tsv"));
    assert_eq!(t.retakes_tsv(), here("prepare/transcript/retakes.tsv"));
    assert_eq!(t.final_txt(), here("prepare/transcript/final.txt"));

    assert_eq!(
        t.cache_llm("cut", "ab12"),
        here("cache/llm/cut/ab12")
    );
    assert_eq!(t.wave("cam-2"), here("cache/waves/cam-2.wave"));
    assert_eq!(t.edges_dir(), here("cache/edges"));

    assert_eq!(t.cut_json(), here("cut/cut.json"));
    assert_eq!(t.cut_line_json(), here("cut/line.json"));

    assert_eq!(t.narration_json(), here("narrate/narration.json"));
    assert_eq!(t.narration_prev_json(), here("narrate/narration.prev.json"));
    assert_eq!(t.voice_txt(), here("narrate/voice.txt"));
    assert_eq!(t.pitch_txt(), here("narrate/pitch.txt"));
    assert_eq!(t.takes_json(), here("narrate/takes.json"));
    assert_eq!(t.voice_ref_base_wav(), here("narrate/voice_ref_base.wav"));
    assert_eq!(t.voice_ref_wav(), here("narrate/voice_ref.wav"));
    assert_eq!(
        t.tts_wav("0123456789abcdef"),
        here("narrate/tts/0123456789abcdef.wav")
    );
    assert_eq!(
        t.sample_wav("voice1", "a1b2c3d4e5f6"),
        here("narrate/samples/voice1_a1b2c3d4e5f6.wav")
    );

    assert_eq!(t.clips_dir(), here("produce/clips"));
    assert_eq!(t.final_video("mp4"), here("produce/final.mp4"));
    assert_eq!(t.final_stamp(), here("produce/final.stamp"));
    assert_eq!(t.final_srt(None), here("produce/final.srt"));
    assert_eq!(t.final_srt(Some("de")), here("produce/final.de.srt"));
    assert_eq!(t.final_vtt(None), here("produce/final.vtt"));
    assert_eq!(t.final_vtt(Some("de")), here("produce/final.de.vtt"));
    assert_eq!(t.final_jpg(), here("produce/final.jpg"));
    assert_eq!(t.final_html(), here("produce/final.html"));

    // Nothing written yet: the new place is the answer, created by create_dirs.
    t.create_dirs().unwrap();
    assert_eq!(t.publish_json(), here("produce/publish/publish.json"));
    assert_eq!(t.thumbnail_png(), here("produce/publish/thumbnail.png"));
    assert_eq!(
        t.thumbnail_plain_png(),
        here("produce/publish/thumbnail-plain.png")
    );
    assert_eq!(t.thumbnail_stamp(), here("produce/publish/thumbnail.stamp"));
    assert_eq!(t.description_txt(), here("produce/publish/description.txt"));

    assert_eq!(
        t.llm_page("0519-142530", "describe"),
        here("llm/0519-142530-describe.html")
    );
    assert_eq!(t.requests_tsv(), here("requests.tsv"));

    // The cards belong to the root, shared by every project under it.
    let assets = layout::assets_dir(&root);
    assert_eq!(assets, root.join("assets"));
    assert!(!assets.starts_with(t.dir()));
    assert_eq!(assets.join("tier.svg"), root.join("assets/tier.svg"));
}

#[test]
fn sec_01_project_and_files_1_layout_dirs_are_0755_files_are_0644() {
    let (_root, t) = fixture("modes");
    t.create_dirs().unwrap();

    // A nested entry is where a recursive create would leave the parents at the
    // umask default instead of 0755.
    for rel in [
        "sources",
        "prepare",
        "prepare/inputs",
        "prepare/transcript",
        "cache/llm",
        "produce/publish",
        "llm",
    ] {
        assert_eq!(mode(&t.dir().join(rel)), 0o755, "{rel}");
    }

    t.write_file(Path::new("cut/line.json"), br#"{"t": 12.5}"#)
        .unwrap();
    assert_eq!(mode(&t.cut_line_json()), 0o644);

    let model = naivepost::project::Project::default();
    naivepost::project::save(&model, t.dir()).unwrap();
    assert_eq!(mode(&t.project_file()), 0o644);
}

#[test]
fn sec_01_project_and_files_1_layout_publish_legacy_is_read_never_migrated() {
    let (root, t) = fixture("publish");
    let legacy = root.join("demo.naivepost/publish");
    std::fs::create_dir_all(&legacy).unwrap();
    let legacy_file = legacy.join("publish.json");
    std::fs::write(&legacy_file, br#"{"title":"old"}"#).unwrap();

    assert_eq!(t.publish_dir(), legacy);
    assert_eq!(t.publish_json(), legacy_file);
    assert_eq!(std::fs::read_to_string(t.publish_json()).unwrap(), r#"{"title":"old"}"#);

    // Read there for ever: nothing moved, nothing copied, the new place untouched.
    assert!(legacy.exists());
    assert!(!root.join("demo.naivepost/produce/publish/publish.json").exists());

    // Once the new place exists it wins — that is a project written after the move.
    let now = root.join("demo.naivepost/produce/publish");
    std::fs::create_dir_all(&now).unwrap();
    std::fs::write(now.join("publish.json"), br#"{"title":"new"}"#).unwrap();
    assert_eq!(t.publish_dir(), now);
    assert!(legacy_file.exists());
}

#[test]
fn sec_01_project_and_files_1_layout_path_rule_three_forms_round_trip() {
    let root = Path::new("/data/naivepost");
    let project = Path::new("/data/naivepost/lecture.naivepost");

    // Read side: the three forms the rule names.
    assert_eq!(
        layout::resolve(root, project, "project:sources/a.mkv"),
        PathBuf::from("/data/naivepost/lecture.naivepost/sources/a.mkv")
    );
    assert_eq!(
        layout::resolve(root, project, "assets/tier.svg"),
        PathBuf::from("/data/naivepost/assets/tier.svg")
    );
    assert_eq!(
        layout::resolve(root, project, "/home/dev/talk.mkv"),
        PathBuf::from("/home/dev/talk.mkv")
    );

    // Write side, and one reader for all three: what is written reads back the same.
    for path in [
        "/data/naivepost/lecture.naivepost/sources/a.mkv",
        "/data/naivepost/assets/tier.svg",
        "/home/dev/talk.mkv",
    ] {
        let path = Path::new(path);
        let written = layout::write_path(root, project, path);
        assert_eq!(layout::resolve(root, project, &written), path, "{written}");
    }

    assert_eq!(
        layout::write_path(root, project, Path::new("/data/naivepost/lecture.naivepost/sources/a.mkv")),
        "project:sources/a.mkv"
    );
    assert_eq!(
        layout::write_path(root, project, Path::new("/data/naivepost/assets/tier.svg")),
        "assets/tier.svg"
    );
    assert_eq!(
        layout::write_path(root, project, Path::new("/home/dev/talk.mkv")),
        "/home/dev/talk.mkv"
    );
}

#[test]
fn sec_01_project_and_files_1_layout_copy_part_and_scratch_are_in_the_project() {
    let (_root, t) = fixture("scratch");

    // A copy in flight sits beside where the copy will land.
    let part = t.copy_part("cam.mkv");
    assert!(part.starts_with(t.dir()));
    assert_eq!(part.file_name().unwrap(), "cam.mkv.part");

    // ASR and diarization scratch live under the source's input folder; when each
    // is removed is a Prepare flow rule (F1.x), not a layout one.
    for scratch in [t.asr_scratch("mic"), t.diar_scratch("mic")] {
        assert!(scratch.starts_with(t.dir()));
        assert_eq!(scratch.parent().unwrap(), t.input_dir("mic"));
    }
    assert_eq!(t.asr_scratch("mic").file_name().unwrap(), "asr");
    assert_eq!(t.diar_scratch("mic").file_name().unwrap(), "diar");
}
