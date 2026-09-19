//! §01-project-and-files#2-naivepostjson — the project file's current keys, their
//! defaults, and the prototype-only keys that are read and ignored, never written.

use std::path::{Path, PathBuf};

use naivepost::layout::Tree;
use naivepost::project::{self, Crop, Publish, Source, TextMark};

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "naivepost-json-{}-{tag}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

/// A project folder holding the given file text; returns its tree and the root.
fn with_file(tag: &str, text: &str) -> (PathBuf, Tree) {
    let root = temp_dir(tag);
    let dir = root.join("demo.naivepost");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(project::PROJECT_FILE), text).unwrap();
    (root, Tree::new(&dir).unwrap())
}

fn read(t: &Tree) -> String {
    std::fs::read_to_string(t.project_file()).unwrap()
}

#[test]
fn sec_01_project_and_files_2_naivepostjson_empty_file_holds_the_table_defaults() {
    let (root, t) = with_file("defaults", "{}");
    let p = project::load(t.dir()).expect("loads");

    assert!(p.sources.is_empty());
    assert_eq!(p.interval, 1.0);
    assert_eq!(p.language, "en");
    assert!(!p.no_narration);
    // Inverted on purpose: absent = copy sources in.
    assert!(!p.reference_sources);
    assert_eq!(p.context, "");
    assert_eq!(p.vid_dir(&root), root.join("input_video"));
    assert_eq!(p.aud_dir(&root), root.join("input_audio"));

    // P.policy.markingPass defaults to retakes, P.policy.cutMode to model, both
    // with origin default so the User Context can still override them.
    assert_eq!(
        p.policy.marking_pass.value,
        project::MarkingPass::Retakes
    );
    assert_eq!(p.policy.marking_pass.origin, project::Origin::Default);
    assert_eq!(p.policy.cut_mode.value, project::CutMode::Model);
    assert_eq!(p.policy.cut_mode.origin, project::Origin::Default);

    let want = project::Produce::default();
    assert_eq!(p.produce.container, want.container);
    assert_eq!(p.produce.codec, want.codec);
    // The spec's preset default is slow, not the prototype's veryslow.
    assert_eq!(p.produce.preset, project::Preset::Slow);
    assert_eq!(p.produce.resolution, project::Resolution::P1080);
    assert_eq!(p.produce.frame_rate, project::FrameRate::F30);
    assert_eq!(p.produce.audio_kbps, 128);
    assert_eq!(p.produce.subtitles, project::Subtitles::None);
    assert!(p.produce.translate.is_empty());
    assert_eq!(p.produce.game_volume, 0.22);
    assert_eq!(p.produce.crf, 24);

    // Upload text is absent until it is written.
    assert_eq!(p.publish, None);
}

#[test]
fn sec_01_project_and_files_2_naivepostjson_round_trip_keeps_every_written_key() {
    let (root, t) = with_file("round-trip", "{}");
    let p = project::Project {
        sources: vec![Source {
            path: "project:sources/a.mkv".to_string(),
            footage: true,
            narrator: 1,
            sepvoice: true,
            tracks: vec![0, 1],
        }],
        interval: 0.5,
        language: "de".to_string(),
        no_narration: true,
        reference_sources: true,
        vid_dir: Some("clips".to_string()),
        aud_dir: Some("/srv/mic".to_string()),
        context: "The weekly blockchain lecture".to_string(),
        policy: project::Policy {
            marking_pass: project::Field {
                value: project::MarkingPass::Joins,
                origin: project::Origin::User,
            },
            cut_mode: project::Field {
                value: project::CutMode::Words,
                origin: project::Origin::Model,
            },
        },
        produce: project::Produce {
            container: project::Container::Mkv,
            codec: project::Codec::Vp9,
            preset: project::Preset::Veryfast,
            resolution: project::Resolution::P720,
            frame_rate: project::FrameRate::F24,
            audio_kbps: 320,
            subtitles: project::Subtitles::TrackInFile,
            translate: vec!["de".to_string()],
            game_volume: 0.5,
            crf: 18,
        },
        publish: Some(Publish {
            frames: vec!["project:prepare/inputs/frames/a/f.jpg".to_string()],
            crop: Crop { x: 0.4, y: 0.6 },
            own: true,
            title_box: None,
            thumb_title: "a title".to_string(),
            title_seeded: true,
            texts: vec![TextMark {
                cx: 0.3,
                cy: 0.8,
                wf: 0.4,
                hf: 0.1,
                text: "printed".to_string(),
            }],
            title: "Title".to_string(),
            prompt: "an instruction".to_string(),
            negative: "no words".to_string(),
            description: "the description".to_string(),
        }),
    };

    project::save(&p, t.dir()).unwrap();
    let text = read(&t);
    for key in [
        "\"sources\"",
        "\"interval\"",
        "\"language\"",
        "\"no_narration\"",
        "\"reference_sources\"",
        "\"vid_dir\"",
        "\"aud_dir\"",
        "\"context\"",
        "\"policy\"",
        "\"produce\"",
        "\"publish\"",
        "\"markingPass\"",
        "\"cutMode\"",
        "\"source\": \"user\"",
    ] {
        assert!(text.contains(key), "{key} missing from\n{text}");
    }

    let got = project::load(t.dir()).unwrap();
    assert_eq!(p, got);
    // A stored chooser folder resolves against the root; an absolute one does not.
    assert_eq!(got.vid_dir(&root), root.join("clips"));
    assert_eq!(got.aud_dir(&root), PathBuf::from("/srv/mic"));
}

#[test]
fn sec_01_project_and_files_2_naivepostjson_interval_is_always_written_and_clamped() {
    let (_root, t) = with_file("interval", "{}");
    project::save(&project::Project::default(), t.dir()).unwrap();
    assert!(read(&t).contains("\"interval\": 1.0"), "{}", read(&t));

    for (written, want) in [(0.0, 0.25), (60.0, 5.0), (2.0, 2.0)] {
        let (_root, t) = with_file("interval-clamp", &format!("{{\"interval\": {written}}}"));
        assert_eq!(project::load(t.dir()).unwrap().interval, want, "{written}");
    }
    assert_eq!(project::INTERVAL_MIN, 0.25);
    assert_eq!(project::INTERVAL_MAX, 5.0);
}

#[test]
fn sec_01_project_and_files_2_naivepostjson_prototype_only_keys_are_ignored_and_never_written() {
    let (_root, t) = with_file(
        "prototype-only",
        r#"{"frame_scale": 2, "run_steps": ["cut"], "out_file": "/tmp/x.mp4"}"#,
    );

    // Read and ignored: the file still describes an empty project.
    let p = project::load(t.dir()).expect("loads");
    assert_eq!(p, project::Project::default());

    project::save(&p, t.dir()).unwrap();
    let text = read(&t);
    for gone in ["frame_scale", "run_steps", "out_file"] {
        assert!(!text.contains(gone), "{gone} was written back\n{text}");
    }
}

#[test]
fn sec_01_project_and_files_2_naivepostjson_policy_default_field_origin_is_default_and_survives_a_user_value()
 {
    let (_root, t) = with_file("policy", "{}");
    let p = project::load(t.dir()).unwrap();
    assert_eq!(p.policy.marking_pass.value, project::MarkingPass::Retakes);
    assert_eq!(p.policy.marking_pass.origin, project::Origin::Default);

    let (_root, t) = with_file(
        "policy-user",
        r#"{"policy":{"markingPass":{"value":"joins","source":"user"}}}"#,
    );
    let p = project::load(t.dir()).unwrap();
    assert_eq!(p.policy.marking_pass.value, project::MarkingPass::Joins);
    assert_eq!(p.policy.marking_pass.origin, project::Origin::User);
    // Untouched fields keep their default and its origin.
    assert_eq!(p.policy.cut_mode.value, project::CutMode::Model);
    assert_eq!(p.policy.cut_mode.origin, project::Origin::Default);

    // The form shows each value's origin, so a save has to carry it.
    project::save(&p, t.dir()).unwrap();
    let text = read(&t);
    assert!(text.contains("\"source\": \"user\""), "{text}");
    assert!(text.contains("\"joins\""), "{text}");
    assert_eq!(project::load(t.dir()).unwrap(), p);
}

#[test]
fn sec_01_project_and_files_2_naivepostjson_publish_absent_by_default_and_present_when_written() {
    let (_root, t) = with_file("publish-absent", "{}");
    let p = project::load(t.dir()).unwrap();
    assert_eq!(p.publish, None);

    project::save(&p, t.dir()).unwrap();
    assert!(!read(&t).contains("\"publish\""), "{}", read(&t));

    let mut p = p;
    p.publish = Some(Publish {
        frames: vec!["project:prepare/inputs/frames/a/f.jpg".to_string()],
        title: "Title".to_string(),
        ..Publish::default()
    });
    project::save(&p, t.dir()).unwrap();
    assert!(read(&t).contains("\"publish\""));

    let got = project::load(t.dir()).unwrap();
    assert_eq!(got, p);
    let publish = got.publish.expect("present once written");
    assert_eq!(publish.frames.len(), 1);
    assert_eq!(publish.title, "Title");
    // §5: the first frame is the base, so its position is the whole answer.
    assert_eq!(publish.crop.x, 0.5);
    assert!(!publish.own);
}

/// The path rule (§1) reaches into this file's keys: a stored folder is written
/// by the one rule and read back by the one reader.
#[test]
fn sec_01_project_and_files_2_naivepostjson_chooser_folders_follow_the_path_rule() {
    let root = Path::new("/data/naivepost");
    let mut p = project::Project::default();
    assert_eq!(p.vid_dir(root), PathBuf::from("/data/naivepost/input_video"));

    p.vid_dir = Some("shots".to_string());
    p.aud_dir = Some("/srv/mics".to_string());
    assert_eq!(p.vid_dir(root), PathBuf::from("/data/naivepost/shots"));
    assert_eq!(p.aud_dir(root), PathBuf::from("/srv/mics"));
}
