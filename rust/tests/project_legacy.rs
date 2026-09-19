//! §01-project-and-files#2-naivepostjson, the second half: the legacy keys that are
//! "read and migrated once, never written", and the autosave rule.

use std::path::{Path, PathBuf};

use naivepost::layout::Tree;
use naivepost::project::{self, Autosave};

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "naivepost-legacy-{}-{tag}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

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

/// Every report line mentioning one of these strings.
fn mentions(report: &[String], word: &str) -> bool {
    report.iter().any(|line| line.contains(word))
}

#[test]
fn sec_01_project_and_files_2_naivepostjson_legacy_videos_and_audios_become_sources_with_narrator_one() {
    let (_root, t) = with_file(
        "videos",
        r#"{"videos":["project:sources/a.mkv"],
            "audios":["project:sources/mic.wav","project:sources/mic2.wav"]}"#,
    );
    let p = project::load(t.dir()).unwrap();

    // "in order": the footage first, then the recordings as they were listed.
    assert_eq!(p.sources.len(), 3);
    assert_eq!(p.sources[0].path, "project:sources/a.mkv");
    assert!(p.sources[0].footage, "videos are footage");
    assert!(!p.sources[1].footage, "audios are recordings");
    // The first recording is the voice the narration is spoken in.
    assert_eq!(p.sources[1].narrator, 1);
    assert_eq!(p.sources[2].narrator, 0);

    project::save(&p, t.dir()).unwrap();
    let text = read(&t);
    assert!(!text.contains("\"videos\""), "{text}");
    assert!(!text.contains("\"audios\""), "{text}");

    // A file that already has sources keeps them.
    let (_root, t) = with_file(
        "videos-kept",
        r#"{"sources":[{"path":"project:sources/keep.mkv","footage":true}],
            "videos":["project:sources/old.mkv"]}"#,
    );
    let p = project::load(t.dir()).unwrap();
    assert_eq!(p.sources.len(), 1);
    assert_eq!(p.sources[0].path, "project:sources/keep.mkv");
}

#[test]
fn sec_01_project_and_files_2_naivepostjson_legacy_in_dir_out_dir_fill_the_chooser_folders_once() {
    let (root, t) = with_file("dirs", r#"{"in_dir":"/old/in","out_dir":"/old/out"}"#);
    let p = project::load(t.dir()).unwrap();
    assert_eq!(p.vid_dir(&root), PathBuf::from("/old/in"));
    assert_eq!(p.aud_dir(&root), PathBuf::from("/old/out"));

    project::save(&p, t.dir()).unwrap();
    let text = read(&t);
    assert!(!text.contains("in_dir"), "{text}");
    assert!(!text.contains("out_dir"), "{text}");

    // The new key wins: a migration must not overwrite what the file already says.
    let (_root, t) = with_file(
        "dirs-new-wins",
        r#"{"vid_dir":"new/in","in_dir":"/old/in"}"#,
    );
    let p = project::load(t.dir()).unwrap();
    assert_eq!(p.vid_dir.as_deref(), Some("new/in"));
}

#[test]
fn sec_01_project_and_files_2_naivepostjson_legacy_style_sets_the_policy_as_a_default_the_context_can_override()
{
    // P.policy.markingPass and P.policy.cutMode: "read" is a lecture, where the
    // mistakes sit at the joins and the cut comes from the marked text.
    let (_root, t) = with_file("style-read", r#"{"style":"read"}"#);
    let p = project::load(t.dir()).unwrap();
    assert_eq!(p.policy.marking_pass.value, project::MarkingPass::Joins);
    assert_eq!(p.policy.cut_mode.value, project::CutMode::Words);
    // As a source: default, so the User Context can still override both.
    assert_eq!(p.policy.marking_pass.origin, project::Origin::Default);
    assert_eq!(p.policy.cut_mode.origin, project::Origin::Default);

    // "" is the other prototype style: retakes marked, the cut from a model.
    let (_root, t) = with_file("style-empty", r#"{"style":""}"#);
    let p = project::load(t.dir()).unwrap();
    assert_eq!(p.policy.marking_pass.value, project::MarkingPass::Retakes);
    assert_eq!(p.policy.cut_mode.value, project::CutMode::Model);

    // A file that carries a policy needs no style: the policy is the answer.
    let (_root, t) = with_file(
        "style-and-policy",
        r#"{"style":"read","policy":{"markingPass":{"value":"none","source":"user"}}}"#,
    );
    let (p, report) = project::load_report(t.dir()).unwrap();
    assert_eq!(p.policy.marking_pass.value, project::MarkingPass::None);
    assert_eq!(p.policy.marking_pass.origin, project::Origin::User);
    assert!(mentions(&report, "style"), "{report:?}");

    // An unknown style leaves the defaults and says so.
    let (_root, t) = with_file("style-unknown", r#"{"style":"cinematic"}"#);
    let (p, report) = project::load_report(t.dir()).unwrap();
    assert_eq!(p.policy.marking_pass.value, project::MarkingPass::Retakes);
    assert!(mentions(&report, "style"), "{report:?}");

    // Never written again.
    let (_root, t) = with_file("style-save", r#"{"style":"read"}"#);
    let p = project::load(t.dir()).unwrap();
    project::save(&p, t.dir()).unwrap();
    assert!(!read(&t).contains("\"style\""));
}

#[test]
fn sec_01_project_and_files_2_naivepostjson_legacy_hints_fold_into_prompts_with_their_lead_ins() {
    let (_root, t) = with_file(
        "hints",
        r#"{"describe_hints":"the HUD number top left is ammo",
            "transcript_hints":"  SPEAKER_00 is Jan  ",
            "cut_hints":"keep the last round whole",
            "narrate_hints":"open with an intro over the first clip"}"#,
    );
    let (_p, report) = project::load_report(t.dir()).unwrap();

    // The lead-ins are the runners' own wording, so the model reads what it read
    // before; the folds are reported rather than applied because the prompt store is
    // machine-level and arrives with its own item — there is nothing to append to yet.
    for (prompt, lead_in) in [
        ("describe", "Editor's notes about this footage -- trust them:"),
        ("fix", "Editor's notes -- trust them:"),
        (
            "cut",
            "Editor's notes about this session -- trust them and let them guide what matters:",
        ),
        ("narrate", "Editor's goals and context -- honor them:"),
    ] {
        assert!(mentions(&report, prompt), "{prompt} missing from {report:?}");
        assert!(mentions(&report, lead_in), "{lead_in} missing from {report:?}");
    }

    // Whitespace folds nothing.
    let (_root, t) = with_file("hints-blank", r#"{"describe_hints":"   "}"#);
    let (_p, report) = project::load_report(t.dir()).unwrap();
    assert!(!mentions(&report, "hints"), "{report:?}");

    // And a hint key never comes back.
    let (_root, t) = with_file("hints-save", r#"{"cut_hints":"keep it short"}"#);
    let p = project::load(t.dir()).unwrap();
    project::save(&p, t.dir()).unwrap();
    assert!(!read(&t).contains("_hints"), "{}", read(&t));
}

#[test]
fn sec_01_project_and_files_2_naivepostjson_legacy_prompts_are_adopted_not_stored() {
    let (_root, t) = with_file(
        "prompts",
        r#"{"prompts":{"cut":"ask for the best bits","narrate":"be brief"}}"#,
    );
    let (p, report) = project::load_report(t.dir()).unwrap();

    // Adopted where the machine has none: named for adoption, not kept on the project.
    assert!(mentions(&report, "cut"), "{report:?}");
    assert!(mentions(&report, "narrate"), "{report:?}");
    assert!(mentions(&report, "adopt"), "{report:?}");

    project::save(&p, t.dir()).unwrap();
    assert!(!read(&t).contains("\"prompts\""), "{}", read(&t));
}

#[test]
fn sec_01_project_and_files_2_naivepostjson_legacy_publish_base_and_title_off_migrate() {
    for (base, want) in [
        // The old radio named a frame by its 0-based list index; the front of the list
        // is the base now, so index 2 of [f0,f1,f2] names f2.jpg and that becomes it.
        (2usize, ["f2.jpg", "f0.jpg", "f1.jpg"]),
        // An index naming no other frame leaves the list alone: 0 names the frame
        // already at the front, and an index past the end names nothing.
        (0, ["f0.jpg", "f1.jpg", "f2.jpg"]),
        (9, ["f0.jpg", "f1.jpg", "f2.jpg"]),
    ] {
        let (_root, t) = with_file(
            "base-edge",
            &format!(
                r#"{{"publish":{{"frames":["f0.jpg","f1.jpg","f2.jpg"],"base":{base}}}}}"#
            ),
        );
        let p = project::load(t.dir()).unwrap();
        let frames: Vec<String> = p.publish.unwrap().frames;
        assert_eq!(
            frames,
            want.iter().map(|f| f.to_string()).collect::<Vec<_>>(),
            "index {base}"
        );
    }

    // title_off is the old spelling of today's "not printed" checkbox. It clears the
    // printed line and counts the title as answered, then retires: that is what the
    // flag said, and no other value in a current file says it.
    let (_root, t) = with_file(
        "title-off",
        r#"{"publish":{"title":"a name","title_off":true,"thumb_title":"a name"}}"#,
    );
    let p = project::load(t.dir()).unwrap();
    let publish = p.publish.clone().unwrap();
    assert_eq!(publish.thumb_title, "");
    assert!(publish.title_seeded);

    // `title_off` is read and dropped, never guessed at: it is the old spelling of
    // today's "not printed" checkbox, whose state the Produce page owns (08 §2, F5.6).
    // A bare title is left as it is, so a fresh save cannot disagree with what loaded.
    let (_root, t) = with_file("title-on", r#"{"publish":{"title":"a name"}}"#);
    let p = project::load(t.dir()).unwrap();
    let publish = p.publish.clone().unwrap();
    assert_eq!(publish.thumb_title, "");
    assert!(!publish.title_seeded);

    // A printed line is the current spelling of "printed", and stands untouched.
    let (_root, t) = with_file(
        "title-printed",
        r#"{"publish":{"title":"a name","thumb_title":"a name"}}"#,
    );
    let p = project::load(t.dir()).unwrap();
    let publish = p.publish.clone().unwrap();
    assert_eq!(publish.thumb_title, "a name");

    // The frame named by the index comes back at the front, which is what "the first
    // frame is the base" (§5) needs.
    let (_root, t) = with_file(
        "base-two",
        r#"{"publish":{"frames":["f0.jpg","f1.jpg","f2.jpg"],"base":2}}"#,
    );
    let p = project::load(t.dir()).unwrap();
    assert_eq!(p.publish.clone().unwrap().frames[0], "f2.jpg");

    // Neither legacy key survives a save.
    project::save(&p, t.dir()).unwrap();
    let text = read(&t);
    assert!(!text.contains("title_off"), "{text}");
    assert!(!text.contains("\"base\""), "{text}");
}

#[test]
fn sec_01_project_and_files_2_naivepostjson_legacy_pitch_is_ignored() {
    let (_root, t) = with_file("pitch", r#"{"pitch": 1.2}"#);
    let (p, report) = project::load_report(t.dir()).unwrap();
    assert_eq!(p, project::Project::default());
    assert!(mentions(&report, "pitch"), "{report:?}");

    project::save(&p, t.dir()).unwrap();
    assert!(!read(&t).contains("pitch"), "{}", read(&t));
}

#[test]
fn sec_01_project_and_files_2_naivepostjson_autosave_writes_only_when_the_bytes_differ() {
    let (_root, t) = with_file("autosave", "{}");
    let dir = t.dir().to_path_buf();
    let mut model = project::Project::default();
    let file = dir.join(project::PROJECT_FILE);

    // A fresh Autosave knows nothing, so the first tick writes.
    let mut autosave = Autosave::new();
    assert!(autosave.save_if_changed(&model, &dir).unwrap(), "first tick writes");
    // An Autosave seeded by a write holds those bytes: the next comparison is against
    // what is on disk, not against what the model held when it was opened.
    let first = std::fs::read_to_string(&file).unwrap();

    // Same bytes: nothing to write, and the file is left alone.
    assert!(!autosave.save_if_changed(&model, &dir).unwrap());
    assert_eq!(std::fs::read_to_string(&file).unwrap(), first);

    model.context = "a lecture".to_string();
    assert!(autosave.save_if_changed(&model, &dir).unwrap());
    let second = std::fs::read_to_string(&file).unwrap();
    assert_ne!(second, first);
    assert!(second.contains("a lecture"));

    // Changed and changed back to what the file already holds: the bytes match the
    // last write once they are marshalled again, so nothing more is written.
    model.context.clear();
    assert!(autosave.save_if_changed(&model, &dir).unwrap(), "a lecture -> \"\" differs");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), first);
    // The file still holds the last thing that WAS different; only the write is
    // skipped once the bytes match again.
    assert!(read_in(&file) == first);
    assert!(!autosave.save_if_changed(&model, &dir).unwrap());

    // Window close: same rule, and it must not be skippable when something changed.
    model.context = "written on close".to_string();
    assert!(autosave.flush(&model, &dir).unwrap());
    assert!(read_in(&file).contains("written on close"));
    assert!(!autosave.flush(&model, &dir).unwrap());

    // Seeded from what a load just read, so the first tick after opening a project
    // writes nothing even though this Autosave never saw a write of its own.
    let loaded = project::load(&dir).unwrap();
    let mut seeded = Autosave::from_project(&loaded).unwrap();
    assert!(!seeded.save_if_changed(&loaded, &dir).unwrap());
}

fn read_in(file: &Path) -> String {
    std::fs::read_to_string(file).unwrap()
}

#[test]
fn sec_01_project_and_files_2_naivepostjson_autosave_interval_is_two_seconds() {
    use std::time::Duration;
    // The shell's tick reads this constant, so "every 2 s" is not a UI detail.
    assert_eq!(project::AUTOSAVE_INTERVAL, Duration::from_secs(2));
}

#[test]
fn sec_01_project_and_files_2_naivepostjson_a_legacy_file_loads_and_its_report_is_the_whole_migration() {
    let (_root, t) = with_file(
        "all-legacy",
        r#"{
             "videos": ["project:sources/a.mkv"],
             "audios": ["project:sources/mic.wav"],
             "in_dir": "/old/in",
             "out_dir": "/old/out",
             "style": "read",
             "describe_hints": "the slides are in the top right",
             "prompts": {"cut": "keep the last round whole"},
             "pitch": 1.0,
             "publish": {"frames": ["f0.jpg", "f1.jpg", "f2.jpg"], "base": 2, "title_off": true}
           }"#,
    );

    let (p, report) = project::load_report(t.dir()).unwrap();

    // Every migration is in the report — nothing was folded in silently.
    for word in ["videos", "in_dir", "out_dir", "style", "describe_hints", "prompts", "pitch"] {
        assert!(mentions(&report, word), "{word} missing from {report:?}");
    }
    assert_eq!(p.sources.len(), 2);
    assert_eq!(p.policy.marking_pass.value, project::MarkingPass::Joins);
    assert_eq!(p.publish.clone().unwrap().frames[0], "f2.jpg");

    // Migrated once: saving drops every legacy key, so a reload has nothing left to do.
    project::save(&p, t.dir()).unwrap();
    let (again, report) = project::load_report(t.dir()).unwrap();
    assert!(report.is_empty(), "still migrating: {report:?}");
    assert_eq!(p, again);
}
