//! §03-shell.md F0.10 Save as — naming the project, and moving what it has written with it.
//!
//! S1 refused during a run, S2 a dialog that names the project, S3 a different name renaming the
//! whole folder (never a copy, and failing when that folder already has work in it), S4 "project
//! saved". All of it is [`naivepost::save_as`]'s decision; the Save button only forwards
//! (spec/00-principles.md §5).

use std::fs;
use std::path::{Path, PathBuf};

use naivepost::project::{self, Project};
use naivepost::save_as::{self, SAVED};

/// A directory under the temp dir that no other test run shares.
fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "naivepost-save-as-{}-{tag}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// A project folder, written so a rename has something recognisable to carry along.
fn project_folder(dir: &Path) -> PathBuf {
    fs::create_dir_all(dir).unwrap();
    project::save(&session(), dir).unwrap();
    dir.to_path_buf()
}

/// The session under test: named by its context, so a written-over file shows it.
fn session() -> Project {
    Project {
        context: "the old session".to_string(),
        ..Default::default()
    }
}

#[test]
fn f0_10_s1_a_run_under_way_is_refused() {
    // S1: the save names the folder the run is writing into.
    assert_eq!(
        save_as::press(true),
        Err("stop the run first \u{2014} saving under a new name moves the folder it is writing into")
    );
    assert_eq!(save_as::press(false), Ok(()));
}

#[test]
fn f0_10_s2_the_dialog_is_titled_for_saving() {
    // S2: what the chooser and the status line say.
    assert_eq!(save_as::TITLE, "Save the project");
    assert_eq!(SAVED, "project saved");
}

#[test]
fn f0_10_s3_the_same_name_writes_the_project_file_and_moves_nothing() {
    // S3's "the same name?" yes-branch: an ordinary save. The folder keeps everything it holds and
    // nothing is logged, because nothing moved.
    let root = temp_dir("same");
    let folder = project_folder(&root.join("jan.naivepost"));
    fs::write(folder.join("render.mp4"), b"the whole video").unwrap();

    let saved = save_as::save_as(&session(), &folder, "jan").expect("the same name always saves");
    assert_eq!(saved.path, folder);
    assert!(saved.logs.is_empty(), "nothing moved: {:?}", saved.logs);
    assert_eq!(fs::read(folder.join("render.mp4")).unwrap(), b"the whole video");
    assert_eq!(
        project::load(&folder).unwrap().context,
        "the old session",
        "the project file was written"
    );

    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn f0_10_s3_a_different_name_renames_the_folder_rather_than_copying_it() {
    // S3: the work follows the name. A rename, so the old path is gone — a copy would leave two
    // folders of frames and no way to tell which one tonight's render came from — and the nested
    // frames are what makes the count recursive.
    let root = temp_dir("rename");
    let jan = project_folder(&root.join("jan.naivepost"));
    fs::write(jan.join("render.mp4"), b"the whole video").unwrap();
    fs::create_dir_all(jan.join("frames/cam")).unwrap();
    fs::write(jan.join("frames/cam/frame-0001.jpg"), b"a frame").unwrap();

    let feb = root.join("feb.naivepost");
    let saved = save_as::save_as(&session(), &jan, "feb").expect("a free name renames");
    assert_eq!(saved.path, feb);
    assert!(!jan.exists(), "never a copy: the old folder is gone");
    assert_eq!(fs::read(feb.join("render.mp4")).unwrap(), b"the whole video");
    assert_eq!(
        fs::read(feb.join("frames/cam/frame-0001.jpg")).unwrap(),
        b"a frame"
    );
    assert!(feb.join(project::PROJECT_FILE).is_file());
    assert_eq!(
        saved.logs,
        vec![format!(">>> moved the output folder to {}", feb.display())]
    );
    assert_eq!(saved.status, SAVED);

    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn f0_10_s3_renaming_onto_a_folder_that_has_work_in_it_fails_and_the_files_stay() {
    // S3: the rename is the whole check. It fails onto a folder with work in it, and a save that
    // went ahead anyway would leave the project file pointing at a folder its outputs are not in.
    let root = temp_dir("taken");
    let jan = project_folder(&root.join("jan.naivepost"));
    fs::write(jan.join("render.mp4"), b"the whole video").unwrap();
    fs::create_dir_all(jan.join("frames/cam")).unwrap();
    fs::write(jan.join("frames/cam/frame-0001.jpg"), b"a frame").unwrap();
    let taken = root.join("taken.naivepost");
    fs::create_dir_all(&taken).unwrap();
    fs::write(taken.join("somebody-elses.mp4"), b"not ours").unwrap();

    let err = save_as::save_as(&session(), &jan, "taken")
        .err()
        .expect("a folder with work in it is not overwritten");
    assert!(
        err.starts_with("!!! could not move the output folder to "),
        "{err}"
    );
    assert!(err.contains(&format!("file(s) are still in {}", jan.display())), "{err}");
    // 3 = naivepost.json, render.mp4 and the frame under frames/cam.
    assert!(err.contains("the 3 file(s)"), "the count is what it says: {err}");

    assert!(jan.exists() && jan.join("render.mp4").is_file());
    assert!(jan.join("frames/cam/frame-0001.jpg").is_file());
    assert_eq!(fs::read(taken.join("somebody-elses.mp4")).unwrap(), b"not ours");
    assert_eq!(
        project::load(&jan).unwrap().context,
        "the old session",
        "a failed move writes nothing"
    );

    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn f0_10_s3_renaming_onto_an_empty_folder_goes_through() {
    // …and onto an empty one it succeeds, which is the other half of the answer the rename gives:
    // a folder somebody made and never wrote into is not work to lose.
    let root = temp_dir("empty-target");
    let jan = project_folder(&root.join("jan.naivepost"));
    fs::write(jan.join("render.mp4"), b"the whole video").unwrap();
    let empty = root.join("feb.naivepost");
    fs::create_dir_all(&empty).unwrap();

    let saved = save_as::save_as(&session(), &jan, "feb").expect("an empty folder takes the rename");
    assert!(!jan.exists());
    assert_eq!(fs::read(empty.join("render.mp4")).unwrap(), b"the whole video");
    assert_eq!(saved.path, empty);

    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn f0_10_s3_a_project_that_has_never_written_moves_without_a_word() {
    // A project saved before it was ever run has no output folder at all. There is nothing to move,
    // and saying so on every first save would be noise.
    let root = temp_dir("never");
    let never = root.join("never.naivepost");

    let saved = save_as::save_as(&session(), &never, "fresh").expect("nothing to move, so nothing fails");
    assert!(saved.logs.is_empty(), "{:?}", saved.logs);
    assert!(!never.exists());
    assert!(root
        .join("fresh.naivepost")
        .join(project::PROJECT_FILE)
        .is_file());

    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn f0_10_s4_a_saved_project_says_so() {
    // S4: both ways of finishing end on the same sentence, and a refused move never reaches it —
    // the flowchart's refusal stops the flow before the status.
    let root = temp_dir("status");
    let jan = project_folder(&root.join("jan.naivepost"));
    assert_eq!(save_as::save_as(&session(), &jan, "jan").unwrap().status, SAVED);

    let feb = project_folder(&root.join("feb.naivepost"));
    fs::write(feb.join("render.mp4"), b"the whole video").unwrap();
    assert_eq!(save_as::save_as(&session(), &feb, "mar").unwrap().status, SAVED);

    let taken = root.join("taken.naivepost");
    fs::create_dir_all(&taken).unwrap();
    fs::write(taken.join("somebody-elses.mp4"), b"not ours").unwrap();
    let old = project_folder(&root.join("apr.naivepost"));
    fs::write(old.join("render.mp4"), b"the whole video").unwrap();
    assert!(save_as::save_as(&session(), &old, "taken").is_err());

    fs::remove_dir_all(&root).unwrap();
}
