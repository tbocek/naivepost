//! §03-shell.md F0.8 New project — ＋ New, emptied into a folder of its own.
//!
//! S1 refused during a run, S2 an empty session goes straight to naming, S3 otherwise confirm what
//! goes back to empty, S4 a default name of today's date with `.naivepost` on the end, S5 refuse a
//! name that is a project already, S6 write the blank session and say so. All of it is
//! [`naivepost::new_project`]'s decision; the button only draws what it returns
//! (spec/00-principles.md §5).

use std::fs;
use std::path::{Path, PathBuf};

use naivepost::new_project::{self as new_project, Gate};
use naivepost::project::{self, Project};
use naivepost::startup;

/// A directory under the temp dir that no other test run shares.
fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "naivepost-new-project-{}-{tag}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// A folder that is a project already, holding something recognisable so an overwrite would show.
fn project_folder(dir: &Path) -> PathBuf {
    fs::create_dir_all(dir).unwrap();
    let old = Project {
        context: "the old session".to_string(),
        ..Default::default()
    };
    project::save(&old, dir).unwrap();
    dir.to_path_buf()
}

#[test]
fn f0_8_s1_a_run_under_way_is_refused() {
    // S1: the run is reading this session's sources, so New would take them out from under it. The
    // refusal outranks every other branch, including an empty session.
    let root = temp_dir("run");
    let open = startup::session_dir(&root);

    let gate = new_project::press(true, false, &open);
    assert_eq!(
        gate,
        Gate::Refused {
            reason: "stop the run first \u{2014} a new project would pull its inputs out from under it"
        }
    );
    assert_eq!(
        new_project::press(true, true, &open),
        Gate::Refused {
            reason: new_project::RUN_REFUSAL
        },
        "a run is refused even when there is nothing to lose"
    );

    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn f0_8_s2_an_empty_session_goes_straight_to_naming() {
    // S2: an empty session has nothing to throw away, so it is not asked first — named or working
    // copy, the answer is the same.
    let root = temp_dir("empty");
    for open in [startup::session_dir(&root), root.join("jan.naivepost")] {
        assert_eq!(new_project::press(false, true, &open), Gate::Name);
    }

    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn f0_8_s3_a_session_with_something_in_it_is_confirmed_first() {
    // S3: the question names what goes back to empty, and then says what happens to the project
    // that is open now — which for a named one is "nothing at all".
    let root = temp_dir("confirm");
    let open = root.join("jan.naivepost");

    let gate = new_project::press(false, false, &open);
    let Gate::Confirm { detail } = &gate else {
        panic!("a session with something in it has to be asked: {gate:?}");
    };
    assert!(
        detail.starts_with(new_project::DETAIL),
        "the first paragraph is the spec's own: {detail}"
    );
    assert!(
        detail.contains("jan.naivepost stays on disk as it is, with everything it has written -- this session simply stops being it."),
        "{detail}"
    );
    assert_eq!(new_project::QUESTION, "Start a new project?");
    assert_eq!(new_project::BUTTON, "Start new\u{2026}");

    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn f0_8_s3_a_session_that_never_had_a_name_says_there_is_nothing_to_come_back_to() {
    // The working copy has no name to stay on disk under, so the other sentence applies — and it is
    // the one that says this press really is the last word on the session.
    let root = temp_dir("working");
    let open = startup::session_dir(&root);

    let detail = new_project::confirm_detail(&open);
    assert!(
        detail.contains(
            "This session has never been saved under a name of its own, so there is nothing to come back to."
        ),
        "{detail}"
    );
    assert!(!detail.contains("stays on disk as it is"), "{detail}");

    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn f0_8_s3_a_question_asks_and_writes_nothing() {
    // The reason a button one click from Load and Save is safe: the refusal and the question have no
    // effect to undo, because Cancel is simply not pressing on.
    let root = temp_dir("no-write");
    let open = root.join("jan.naivepost");

    for gate in [
        new_project::press(true, false, &open),
        new_project::press(false, true, &open),
        new_project::press(false, false, &open),
    ] {
        // Every branch is a question or a refusal — none of them may reach the disk.
        assert!(matches!(
            gate,
            Gate::Refused { .. } | Gate::Name | Gate::Confirm { .. }
        ));
    }
    assert_eq!(fs::read_dir(&root).unwrap().count(), 0);

    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn f0_8_s4_the_default_name_is_todays_date_and_the_next_free_one() {
    // S4: the date is the name, and two sessions on one day are ordinary — so the taken ones get a
    // number rather than a refusal.
    let root = temp_dir("name");

    assert_eq!(new_project::free_name(&root, "2026-09-19"), "2026-09-19.naivepost");
    fs::create_dir_all(root.join("2026-09-19.naivepost")).unwrap();
    assert_eq!(
        new_project::free_name(&root, "2026-09-19"),
        "2026-09-19-2.naivepost"
    );
    fs::create_dir_all(root.join("2026-09-19-2.naivepost")).unwrap();
    assert_eq!(
        new_project::free_name(&root, "2026-09-19"),
        "2026-09-19-3.naivepost"
    );

    // The suffix is added when it is missing and not twice over when it is there — a name typed in
    // full, or spelled by a file manager, is the same project.
    assert_eq!(new_project::with_suffix("talk"), "talk.naivepost");
    assert_eq!(
        new_project::with_suffix("2026-09-19.naivepost"),
        "2026-09-19.naivepost"
    );
    assert_eq!(
        new_project::with_suffix("talk.NAIVEPOST"),
        "talk.NAIVEPOST"
    );

    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn f0_8_s5_a_project_already_at_that_name_is_refused_and_left_alone() {
    // S5: the name is taken by a project, so it is opened rather than written over — and "left
    // alone" has to mean the file, not just the intent.
    let root = temp_dir("taken");
    let taken = project_folder(&root.join("taken.naivepost"));

    let err = new_project::create(&root, &root, "taken")
        .err()
        .expect("a project already at that name is refused");
    assert_eq!(
        err,
        "taken.naivepost is a project already \u{2014} open it, or pick another name"
    );
    assert_eq!(
        project::load(&taken).unwrap().context,
        "the old session",
        "the refusal must not have written over the project"
    );

    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn f0_8_s6_a_new_project_is_written_and_said_so() {
    // S6: the blank project on disk, the status line's sentence, the log's line, and the choosers
    // following the folder it was put in — that is where its footage almost certainly is.
    let root = temp_dir("created");
    let videos = root.join("Videos");
    fs::create_dir_all(&videos).unwrap();

    let created = new_project::create(&root, &videos, "2026-09-19").expect("a free name creates");
    let path = videos.join("2026-09-19.naivepost");
    assert_eq!(created.path, path);
    assert_eq!(created.base, "2026-09-19.naivepost");
    assert!(path.join(project::PROJECT_FILE).is_file());
    assert_eq!(created.project, Project::default());
    assert!(created.project.sources.is_empty());
    assert!(created.project.context.is_empty());
    assert_eq!(created.status, "new project \u{2014} 2026-09-19.naivepost");
    assert_eq!(
        created.log,
        format!(
            ">>> new project {} -- the session is empty; outputs on disk are untouched",
            path.display()
        )
    );
    assert_eq!(created.chooser, Some(videos));

    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn f0_8_s6_a_session_named_under_the_root_keeps_its_choosers() {
    // A project under the root is where the defaults already point, so nothing follows it — and New
    // creates the project file and no folders: each step makes its own when it first writes.
    let root = temp_dir("under-root");

    let created = new_project::create(&root, &root, "scratch").expect("a free name creates");
    assert_eq!(created.chooser, None);
    assert_eq!(fs::read_dir(&created.path).unwrap().count(), 1);
    assert!(created.path.join(project::PROJECT_FILE).is_file());

    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn f0_8_s6_the_outputs_already_on_disk_are_untouched() {
    // S3 promises it and S6 keeps it: the project that was open, and everything it wrote, stay
    // exactly as they were. Going back to them is Open, not undo.
    let root = temp_dir("untouched");
    let old = project_folder(&root.join("old.naivepost"));
    let render = old.join("render.mp4");
    fs::write(&render, b"the whole video").unwrap();

    new_project::create(&root, &root.join("elsewhere.naivepost"), "new")
        .expect("a free name creates");

    assert_eq!(fs::read(&render).unwrap(), b"the whole video");
    assert_eq!(project::load(&old).unwrap().context, "the old session");

    fs::remove_dir_all(&root).unwrap();
}
