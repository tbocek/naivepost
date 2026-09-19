//! §01-project-and-files#5-producepublishpublishjson — the upload text and thumbnail
//! state as a file: what it holds, that its existence is the whole flag, that deleting
//! the folder starts the step over, and that the legacy keys are read once.
//!
//! This item's text cites no `P.*` parameter and no `tool:*`, so there is none to name.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use naivepost::project::{self, Crop, Publish, TextMark, TitleBox};
use naivepost::publish;
use naivepost::layout::Tree;

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "naivepost-publish-{}-{tag}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

/// A project folder inside a throwaway root, holding the given file text at wherever
/// this project's publish.json lives (or no file).
fn with_file(tag: &str, text: Option<&str>) -> (PathBuf, Tree) {
    let root = temp_dir(tag);
    let dir = root.join("demo.naivepost");
    std::fs::create_dir_all(&dir).unwrap();
    let t = Tree::new(&dir).unwrap();
    if let Some(text) = text {
        std::fs::create_dir_all(t.publish_dir()).unwrap();
        std::fs::write(t.publish_json(), text).unwrap();
    }
    (root, t)
}

fn read(t: &Tree) -> String {
    std::fs::read_to_string(t.publish_json()).unwrap()
}

fn mode(path: &Path) -> u32 {
    std::fs::metadata(path).unwrap().permissions().mode() & 0o777
}

/// §5's example, verbatim.
const SPEC_PUBLISH: &str = r#"{"frames": ["project:prepare/inputs/frames/a/2026-09-16_17-25-30.jpg"], "crop": {"x": 0.5, "y": 0.5}, "own": false,
 "title_box": {"cx": 0.5, "cy": 0.25, "wf": 1, "hf": 0.4}, "thumb_title": "…", "title_seeded": true,
 "texts": [{"cx": 0.3, "cy": 0.8, "wf": 0.4, "hf": 0.1, "text": "…"}],
 "title": "…", "prompt": "…", "negative": "…", "description": "…"}"#;

#[test]
fn sec_01_project_and_files_5_producepublishpublishjson_the_specs_example_round_trips() {
    let (_root, t) = with_file("spec-example", Some(SPEC_PUBLISH));
    let p = publish::load(&t).expect("the spec's own file loads");

    assert_eq!(
        p.frames,
        ["project:prepare/inputs/frames/a/2026-09-16_17-25-30.jpg".to_string()]
    );
    assert_eq!(p.crop, Crop { x: 0.5, y: 0.5 });
    assert!(!p.own);
    assert_eq!(
        p.title_box,
        Some(TitleBox { cx: 0.5, cy: 0.25, wf: 1.0, hf: 0.4 })
    );
    assert_eq!(p.thumb_title, "…");
    assert!(p.title_seeded);
    assert_eq!(
        p.texts,
        [TextMark { cx: 0.3, cy: 0.8, wf: 0.4, hf: 0.1, text: "…".to_string() }]
    );
    assert_eq!((p.title.as_str(), p.prompt.as_str()), ("…", "…"));
    assert_eq!((p.negative.as_str(), p.description.as_str()), ("…", "…"));

    // One writer, and what it writes reads back the same.
    publish::save(&p, &t).unwrap();
    assert_eq!(publish::load(&t).unwrap(), p);
}

#[test]
fn sec_01_project_and_files_5_producepublishpublishjson_the_first_frame_is_the_base_and_the_rest_are_references() {
    let frames = ["f0", "f1", "f2"];
    let p = Publish { frames: frames.iter().map(|f| f.to_string()).collect(), ..Default::default() };

    // Order is the whole answer: the base is the first, and there is no index beside it.
    assert_eq!(publish::base(&p), Some("f0"));
    assert_eq!(publish::references(&p), ["f1".to_string(), "f2".to_string()]);

    let none = Publish::default();
    assert_eq!(publish::base(&none), None);
    assert!(publish::references(&none).is_empty());

    // Choosing another frame moves it to the front and keeps the rest's order, so the
    // references stay in the order the model is given them.
    let mut chosen = p.clone();
    assert!(publish::set_base(&mut chosen, "f2"));
    assert_eq!(chosen.frames, ["f2", "f0", "f1"]);
    assert_eq!(publish::base(&chosen), Some("f2"));
    assert_eq!(publish::references(&chosen), ["f0".to_string(), "f1".to_string()]);

    // A name that is not one of the frames chooses nothing.
    let mut stranger = p.clone();
    assert!(!publish::set_base(&mut stranger, "project:sources/nope.jpg"));
    assert_eq!(stranger.frames, p.frames);

    // And the choice is what reaches the file: the base on disk is the first entry.
    let (_root, t) = with_file("base-order", None);
    publish::save(&chosen, &t).unwrap();
    let back = publish::load(&t).unwrap();
    assert_eq!(back.frames[0], "f2");
    assert_eq!(publish::base(&back), Some("f2"));
}

#[test]
fn sec_01_project_and_files_5_producepublishpublishjson_own_says_the_picture_was_chosen_not_drawn() {
    let p = Publish {
        frames: vec!["project:prepare/inputs/frames/a/2026-09-16_17-25-30.jpg".to_string()],
        own: true,
        ..Default::default()
    };
    let (_root, t) = with_file("own", None);
    publish::save(&p, &t).unwrap();

    let text = read(&t);
    assert!(text.contains("\"own\": true"), "{text}");
    let back = publish::load(&t).unwrap();
    assert!(back.own);
    // The chosen frame is still the base — a picture that was picked has nothing to edit.
    assert_eq!(publish::base(&back), publish::base(&p));
    assert!(publish::references(&back).is_empty());

    // Not chosen, so said as false: unlike §3 and §4, this record writes every key it
    // has (§5's example shows `"own": false` too), which is what lets a save agree with
    // §2's byte comparison. Absent reads back false all the same.
    let drawn = Publish { frames: p.frames.clone(), ..Default::default() };
    publish::save(&drawn, &t).unwrap();
    assert!(read(&t).contains("\"own\": false"), "{}", read(&t));
    assert!(!publish::load(&t).unwrap().own);
    let (_root, t) = with_file("own-absent", Some(r#"{"frames":[]}"#));
    assert!(!publish::load(&t).unwrap().own);

    // Read from a file that spelled it out, too.
    let (_root, t) = with_file("own-read", Some(r#"{"frames":[],"own":true}"#));
    assert!(publish::load(&t).unwrap().own);
}

#[test]
fn sec_01_project_and_files_5_producepublishpublishjson_an_existing_publish_json_means_the_upload_text_is_written() {
    // Neither folder: nothing written.
    let (_root, t) = with_file("flag", None);
    assert!(!publish::is_written(&t));

    // The folder alone is not the text — ▶ would still ask the model.
    std::fs::create_dir_all(t.publish_dir()).unwrap();
    assert!(t.publish_dir().exists());
    assert!(!publish::is_written(&t));

    // A file planted by hand counts: the flag is the file, not any project field. The
    // record it holds does not matter to it.
    std::fs::write(t.publish_json(), "{}").unwrap();
    assert!(publish::is_written(&t));

    // And neither does the folder's age: save writes it, so it is written from then on.
    let (_root, t) = with_file("flag-save", None);
    assert!(!publish::is_written(&t));
    publish::save(&Publish::default(), &t).unwrap();
    assert!(publish::is_written(&t));
}

#[test]
fn sec_01_project_and_files_5_producepublishpublishjson_deleting_the_folder_starts_the_step_over() {
    let (_root, t) = with_file("start-over", None);
    // Something of the cut beside it, to show only the publish folder goes.
    t.write_file(std::path::Path::new("cut/cut.json"), br#"{"segs":[]}"#).unwrap();

    let p = Publish { title: "…".to_string(), description: "…".to_string(), ..Default::default() };
    publish::save(&p, &t).unwrap();
    assert!(publish::is_written(&t));
    assert!(t.description_txt().exists());

    // The whole folder: JSON, description and both pictures go together.
    std::fs::write(t.thumbnail_png(), b"png").unwrap();
    std::fs::write(t.thumbnail_plain_png(), b"png").unwrap();
    publish::start_over(&t).unwrap();

    assert!(!publish::is_written(&t));
    assert!(!t.publish_dir().exists());
    assert!(!t.description_txt().exists());
    assert!(!t.thumbnail_png().exists());
    // The project itself is untouched.
    assert!(t.dir().exists());
    assert!(t.cut_json().exists());

    // A step that never ran is already started over, and that is not an error.
    let (_root, fresh) = with_file("start-over-fresh", None);
    publish::start_over(&fresh).unwrap();
    assert!(!publish::is_written(&fresh));
    assert!(fresh.dir().exists());

    // Twice in a row is the same answer.
    publish::start_over(&t).unwrap();
}

#[test]
fn sec_01_project_and_files_5_producepublishpublishjson_the_description_goes_beside_the_json_and_only_when_there_is_one() {
    let (_root, t) = with_file("description", None);
    publish::save(&Publish { description: "the upload text".to_string(), ..Default::default() }, &t)
        .unwrap();

    // The copy the uploader selects and pastes, beside the file it belongs to.
    let txt = t.description_txt();
    assert_eq!(txt.parent().unwrap(), t.publish_json().parent().unwrap());
    assert_eq!(std::fs::read_to_string(&txt).unwrap(), "the upload text\n");
    assert_eq!(mode(&txt), 0o644);

    // An empty description writes none, so a clean project gains no empty file.
    let (_root, clean) = with_file("description-clean", None);
    publish::save(&Publish::default(), &clean).unwrap();
    assert!(publish::is_written(&clean));
    assert!(!clean.description_txt().exists());

    // And it is not deleted behind the record's back either: what says "start over" is
    // the folder (see start_over), so a save that has nothing to say leaves the previous
    // text standing rather than destroying a file this record does not own.
    let (_root, stale) = with_file("description-stale", None);
    publish::save(&Publish { description: "first".to_string(), ..Default::default() }, &stale)
        .unwrap();
    publish::save(&Publish::default(), &stale).unwrap();
    assert_eq!(std::fs::read_to_string(stale.description_txt()).unwrap(), "first\n");
}

#[test]
fn sec_01_project_and_files_5_producepublishpublishjson_the_file_lands_where_publish_dir_says_and_at_0644() {
    // A project written by this build keeps it under produce/.
    let (_root, t) = with_file("new-place", None);
    std::fs::create_dir_all(t.dir().join("produce/publish")).unwrap();
    publish::save(&Publish { title: "new".to_string(), ..Default::default() }, &t).unwrap();
    assert_eq!(t.publish_json(), t.dir().join("produce/publish/publish.json"));
    assert!(t.publish_json().exists());
    assert_eq!(mode(&t.publish_json()), 0o644);

    // A project written before the move is read there for ever and never migrated: both
    // halves of that are asserted — the read, and that no save moved it.
    let (_root, old) = with_file("legacy-place", Some(r#"{"title":"old"}"#));
    assert_eq!(old.publish_json(), old.dir().join("publish/publish.json"));
    let p = publish::load(&old).unwrap();
    assert_eq!(p.title, "old");

    publish::save(&Publish { title: "still old".to_string(), ..Default::default() }, &old).unwrap();
    assert!(old.dir().join("publish/publish.json").exists());
    assert!(!old.dir().join("produce/publish").exists(), "{} was migrated", old.publish_json().display());
    assert_eq!(publish::load(&old).unwrap().title, "still old");
}

#[test]
fn sec_01_project_and_files_5_producepublishpublishjson_a_missing_file_is_no_upload_text_and_one_function_writes_it() {
    let (_root, t) = with_file("io", None);
    // No file yet: nothing was written and nothing was drawn, which is not an error.
    let empty = publish::load(&t).unwrap();
    assert_eq!(empty, Publish::default());
    assert_eq!(empty.crop, Crop { x: 0.5, y: 0.5 });
    assert!(empty.frames.is_empty());
    assert_eq!(empty.title_box, None);

    let p = Publish { frames: vec!["f0".to_string()], ..Default::default() };
    publish::save(&p, &t).unwrap();
    assert_eq!(publish::load(&t).unwrap(), p);
    assert_eq!(mode(&t.publish_json()), 0o644);

    // A file that is not JSON is an error naming the path, never a silent empty record.
    std::fs::write(t.publish_json(), "{ not json").unwrap();
    let err = publish::load(&t).unwrap_err();
    assert!(err.contains("publish.json"), "{err}");

    // Byte-stable, which is what lets §2's autosave skip a write that changed nothing.
    publish::save(&p, &t).unwrap();
    let first = read(&t);
    publish::save(&p, &t).unwrap();
    assert_eq!(read(&t), first);
}

#[test]
fn sec_01_project_and_files_5_producepublishpublishjson_the_legacy_base_index_and_title_off_are_read_and_never_written_again() {
    // The old radio's answer: which frame was the base, read as an index into the list.
    let (_root, t) = with_file(
        "legacy-base",
        Some(r#"{"frames":["f0","f1","f2"],"base":2,"title":"T"}"#),
    );
    let p = publish::load(&t).unwrap();
    assert_eq!(p.frames, ["f2", "f0", "f1"]);
    // Read forward once: the index is gone and the order carries the answer.
    publish::save(&p, &t).unwrap();
    let text = read(&t);
    assert!(!text.contains("\"base\":"), "{text}");

    // The old spelling of "not printed": the question has been answered once already, so
    // it comes up as a title that was decided about, and with no words to print.
    let (_root, off) = with_file(
        "legacy-title-off",
        Some(r#"{"frames":[],"title":"the entry's words","title_off":true}"#),
    );
    let p = publish::load(&off).unwrap();
    assert!(p.title_seeded);
    assert_eq!(p.thumb_title, "");

    // A bare title is NOT the old shape on its own: whether the picture gets words is
    // the Produce page's answer, and §2 settled that a bare title seeds nothing
    // (project_legacy.rs asserts it too). Only `title_off` retires, counting the title
    // as answered.
    let (_root, bare) = with_file("legacy-title", Some(r#"{"frames":[],"title":"T"}"#));
    let p = publish::load(&bare).unwrap();
    assert!(!p.title_seeded);
    assert_eq!(p.thumb_title, "");

    for tree in [&t, &off, &bare] {
        let p = publish::load(tree).unwrap();
        publish::save(&p, tree).unwrap();
        let text = read(tree);
        // `"base":` with its colon: no other key of this record contains that word.
        assert!(!text.contains("\"base\":"), "{text}");
        assert!(!text.contains("\"title_off\""), "{text}");
        assert_eq!(publish::load(tree).unwrap(), p);
    }
}

#[test]
fn sec_01_project_and_files_5_producepublishpublishjson_the_record_written_here_is_the_one_naivepost_json_holds() {
    // §2 embeds this record in naivepost.json and §5 keeps it in a file of its own. One
    // struct, so the two cannot drift: what publish::save writes, project::load reads
    // back as the same value.
    let p = Publish {
        frames: vec!["project:prepare/inputs/frames/a/f.jpg".to_string(), "f1".to_string()],
        crop: Crop { x: 0.4, y: 0.6 },
        own: true,
        title_box: Some(TitleBox { cx: 0.5, cy: 0.25, wf: 1.0, hf: 0.4 }),
        thumb_title: "printed".to_string(),
        title_seeded: true,
        texts: vec![TextMark {
            cx: 0.3,
            cy: 0.8,
            wf: 0.4,
            hf: 0.1,
            text: "marked".to_string(),
        }],
        title: "Title".to_string(),
        prompt: "an instruction".to_string(),
        negative: "no words".to_string(),
        description: "the description".to_string(),
    };

    let (_root, t) = with_file("one-record", None);
    publish::save(&p, &t).unwrap();

    // The file's own text, embedded in naivepost.json as §2 spells it.
    let project_dir = t.dir().join("embedded.naivepost");
    std::fs::create_dir_all(&project_dir).unwrap();
    std::fs::write(
        project_dir.join("naivepost.json"),
        format!("{{\"publish\": {}}}", read(&t)),
    )
    .unwrap();

    let from_project = project::load(&project_dir).unwrap();
    assert_eq!(from_project.publish, Some(p.clone()));
    assert_eq!(publish::load(&t).unwrap(), p);
}
