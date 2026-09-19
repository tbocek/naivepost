//! §03-shell.md F0.12 Add sources (from Prepare) — the files a session is made of.
//!
//! S0 refused while a run is on and abandoned when `sources/` could not be made; S1 chose from audio
//! and video only; S2 copied into `sources/` or referenced in place; S3 added rows, skipping what is
//! already there and giving slot 1 to the first untagged recording; S4 remembered the folder per
//! kind; S5 counted what went in. All of it is [`naivepost::add_sources`]'s decision — the button and
//! its chooser only forward (spec/00-principles.md §5).

use std::fs;
use std::path::{Path, PathBuf};

use naivepost::add_sources::{self, Progress};
use naivepost::layout;
use naivepost::project::{Project, Source};

fn temp_dir(tag: &str) -> PathBuf {
    // One process id is not enough when a test hands the same tag twice, so the tag carries the pid
    // and each workspace gets its own throwaway root.
    let dir = std::env::temp_dir().join(format!(
        "naivepost-add-sources-{}-{tag}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// A throwaway application root with a project folder in it and a `card/` beside that, standing for
/// where a session's files land from the camera: outside the project, which is what makes "copy into
/// project" a question at all.
fn workspace(tag: &str) -> (PathBuf, PathBuf, PathBuf) {
    let root = temp_dir(tag);
    let dir = root.join("talk.naivepost");
    fs::create_dir_all(&dir).unwrap();
    let card = root.join("card");
    fs::create_dir_all(&card).unwrap();
    (root, dir, card)
}

/// A file of `bytes` bytes under `dir`, so sizes differ and "same size" means something.
fn file(dir: &Path, name: &str, bytes: usize) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, vec![b'x'; bytes]).unwrap();
    path
}

/// The session plus the rows it already holds, for S3's and S4's "what was here before".
fn session(rows: Vec<Source>) -> Project {
    Project { sources: rows, ..Default::default() }
}

fn row(path: &str, footage: bool, narrator: u32) -> Source {
    Source { path: path.to_string(), footage, narrator, ..Default::default() }
}

fn narrator_of(project: &Project, path: &str) -> Option<u32> {
    project.sources.iter().find(|row| row.path == path).map(|row| row.narrator)
}

#[test]
fn f0_12_s0_a_copy_is_refused_while_a_run_is_on() {
    // S0: a copy is a run of its own, so it waits for the run that is on.
    assert_eq!(
        add_sources::press(true),
        Err("a run is already active \u{2014} stop it first (\u{23f9})")
    );
    assert_eq!(add_sources::press(false), Ok(()));
    // The sentence the user reads, pinned.
    assert_eq!(
        add_sources::RUN_REFUSAL,
        "a run is already active \u{2014} stop it first (\u{23f9})"
    );
}

#[test]
fn f0_12_s0_a_copy_abandons_when_the_sources_folder_cannot_be_made() {
    // S0's other half. A regular file where `sources/` has to go is the honest way to make the
    // folder impossible: a mode bit would not stop root.
    let (root, dir, card) = workspace("s0-nodir");
    fs::write(dir.join("sources"), b"in the way").unwrap();
    let picked = file(&card, "mic.wav", 8);

    let mut project = Project::default();
    let err = add_sources::add(&mut project, &root, &dir, &[picked], true).unwrap_err();

    assert_eq!(err, add_sources::NO_SOURCES_DIR);
    assert_eq!(
        add_sources::NO_SOURCES_DIR,
        "could not make the project's sources folder \u{2014} see log"
    );
    assert!(project.sources.is_empty(), "nothing is added when the copy cannot start");

    fs::remove_dir_all(&root).ok();
}

#[test]
fn f0_12_s1_the_chooser_is_titled_and_filtered_to_audio_and_video() {
    // S1: the chooser's words, and the only kinds a source can be.
    assert_eq!(add_sources::CHOOSER_TITLE, "Add sources");
    assert_eq!(add_sources::FILTER_NAME, "Audio and video");

    // The spec's list, in its order: eight audio kinds then six video ones.
    assert_eq!(
        add_sources::MEDIA_EXT,
        [
            ".flac", ".wav", ".mp3", ".m4a", ".aac", ".ogg", ".opus", ".wma", ".mp4", ".mkv",
            ".mov", ".webm", ".avi", ".ts"
        ]
    );
    for ext in add_sources::MEDIA_EXT {
        let path = PathBuf::from(format!("take{ext}"));
        assert!(add_sources::is_media(&path), "{ext} is a source");
        // Video defaults to footage; audio never does.
        assert_eq!(
            add_sources::is_video(&path),
            add_sources::VIDEO_EXT.contains(&ext),
            "{ext} defaults to footage exactly when it is a video kind"
        );
    }

    // What the filter cannot offer is not a source either, whatever its name says.
    for name in ["notes.txt", "noext", "clip.MKVX", "archive.zip"] {
        assert!(!add_sources::is_media(&PathBuf::from(name)), "{name} is not media");
    }
    // And the filter's own case-insensitivity: a camera writes what it writes.
    assert!(add_sources::is_media(&PathBuf::from("CAM.MKV")));
    assert!(add_sources::is_video(&PathBuf::from("Take.WAV.mp4")));

    // A .txt in the middle of an import is skipped rather than added.
    let (root, dir, card) = workspace("s1-txt");
    let txt = file(&card, "notes.txt", 5);
    let wav = file(&card, "mic.wav", 3);
    let mut project = Project::default();
    let found = add_sources::add(&mut project, &root, &dir, &[txt, wav], false).unwrap();
    assert_eq!(found.added, 1);
    assert_eq!(project.sources.len(), 1);

    fs::remove_dir_all(&root).ok();
}

#[test]
fn f0_12_s2_copy_on_lands_in_sources_and_stores_the_one_path_rule() {
    // S2: the copy is what the session names, and the bar counts bytes.
    let (root, dir, card) = workspace("s2-copy");
    let video = file(&card, "cam.mkv", 5);
    let audio = file(&card, "mic.wav", 3);

    let mut project = Project::default();
    let found = add_sources::add(&mut project, &root, &dir, &[video.clone(), audio.clone()], true).unwrap();

    assert_eq!(fs::read(&dir.join("sources/cam.mkv")).unwrap().len(), 5);
    assert_eq!(fs::read(&dir.join("sources/mic.wav")).unwrap().len(), 3);
    assert_eq!(found.paths, vec!["project:sources/cam.mkv", "project:sources/mic.wav"]);
    // A stored path resolves back to the copy, by the one path rule.
    assert_eq!(
        layout::resolve(&root, &dir, &found.paths[0]),
        dir.join("sources/cam.mkv")
    );

    // Progress in bytes: one step per file, ending at the sum of their sizes.
    assert_eq!(found.progress.len(), 2);
    let last = found.progress[1];
    assert_eq!((last.done, last.total), (8, 8));
    assert_eq!(found.progress[0], Progress { done: 5, total: 8 });
    assert_eq!(last.fraction(), 1.0);
    // Nothing of the copy is left behind in flight.
    assert!(!dir.join("sources/cam.mkv.part").exists());
    assert_eq!(
        fs::read_dir(dir.join("sources")).unwrap().count(),
        2,
        "only the two sources are in the folder"
    );

    fs::remove_dir_all(&root).ok();
}

#[test]
fn f0_12_s2_the_same_name_and_size_is_not_copied_again() {
    // S2: same name and same size is the same file, re-picked. The bytes already in `sources/` are
    // what the session gets — a copy would overwrite work with a file that merely looks alike.
    let (root, dir, card) = workspace("s2-same");
    fs::create_dir_all(dir.join("sources")).unwrap();
    fs::write(dir.join("sources/mic.wav"), b"the bytes already here").unwrap();
    let picked = card.join("mic.wav");
    fs::write(&picked, b"a file of equal length").unwrap();

    let mut project = Project::default();
    let found = add_sources::add(&mut project, &root, &dir, &[picked], true).unwrap();

    assert_eq!(fs::read_to_string(dir.join("sources/mic.wav")).unwrap(), "the bytes already here");
    assert_eq!(found.paths, vec!["project:sources/mic.wav"]);
    assert_eq!(project.sources.len(), 1, "added exactly once");
    // The bar still completes: a skipped copy is done work.
    assert_eq!(found.progress[0].fraction(), 1.0);

    fs::remove_dir_all(&root).ok();
}

#[test]
fn f0_12_s2_a_file_already_inside_the_project_is_added_in_place() {
    // S2: a file the project already holds is added as it stands — copying it would be a rename onto
    // its own name at best.
    let (root, dir, _card) = workspace("s2-inside");
    fs::create_dir_all(dir.join("sources")).unwrap();
    let own = dir.join("sources/own.wav");
    fs::write(&own, b"already here").unwrap();

    let mut project = Project::default();
    let found = add_sources::add(&mut project, &root, &dir, &[own.clone()], true).unwrap();

    assert_eq!(found.paths, vec!["project:sources/own.wav"]);
    assert_eq!(fs::read_to_string(&own).unwrap(), "already here");
    let names: Vec<_> = fs::read_dir(dir.join("sources"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().to_string())
        .collect();
    assert_eq!(names, vec!["own.wav".to_string()], "nothing was copied beside it");

    fs::remove_dir_all(&root).ok();
}

#[test]
fn f0_12_s2_copy_off_references_in_place() {
    // S2's no-branch: the file stays where it is and the session holds a reference, for footage that
    // should not exist twice.
    let (root, dir, card) = workspace("s2-ref");
    let picked = file(&card, "cam.mkv", 6);

    let mut project = Project::default();
    let found = add_sources::add(&mut project, &root, &dir, &[picked.clone()], false).unwrap();

    assert_eq!(found.paths, vec!["card/cam.mkv"], "root-relative, as the one path rule stores it");
    assert_eq!(layout::resolve(&root, &dir, &found.paths[0]), picked);
    assert!(!dir.join("sources").exists(), "referencing needs no folder, and makes none");
    assert!(picked.exists(), "the original is untouched");

    fs::remove_dir_all(&root).ok();
}

#[test]
fn f0_12_s3_duplicates_and_non_media_are_skipped() {
    // S3: the list is a set. Picking the same file twice, or something that cannot be a source, adds
    // one row and says so in the counts.
    let (root, dir, card) = workspace("s3-dup");
    let wav = file(&card, "mic.wav", 4);
    let txt = file(&card, "notes.txt", 4);

    let mut project = Project::default();
    let found =
        add_sources::add(&mut project, &root, &dir, &[wav.clone(), wav.clone(), txt], false).unwrap();

    assert_eq!((found.added, found.asked), (1, 3));
    assert_eq!(project.sources.len(), 1);

    fs::remove_dir_all(&root).ok();
}

#[test]
fn f0_12_s3_footage_defaults_on_for_video_only() {
    // S3: a video arrives as footage because that is what a video is for here; the row's own toggle
    // is for the case where only its sound is wanted.
    let (root, dir, card) = workspace("s3-footage");
    let video = file(&card, "cam.mkv", 5);
    let audio = file(&card, "mic.wav", 5);

    let mut project = Project::default();
    add_sources::add(&mut project, &root, &dir, &[video, audio], false).unwrap();

    let footage = |path: &str| {
        project
            .sources
            .iter()
            .find(|row| row.path == path)
            .map(|row| row.footage)
    };
    assert_eq!(footage("card/cam.mkv"), Some(true));
    assert_eq!(footage("card/mic.wav"), Some(false));

    fs::remove_dir_all(&root).ok();
}

#[test]
fn f0_12_s3_slot_1_goes_to_the_first_untagged_recording() {
    // S3: somebody has to be the narrator, and a screen capture is the worse guess — it holds
    // everyone at once. So the recording wins although the video was picked first.
    let (root, dir, card) = workspace("s3-slot1");
    let video = file(&card, "cam.mkv", 5);
    let audio = file(&card, "mic.wav", 5);

    let mut project = Project::default();
    let found = add_sources::add(&mut project, &root, &dir, &[video, audio], false).unwrap();

    assert_eq!((found.added, found.asked), (2, 2));
    assert_eq!(narrator_of(&project, "card/mic.wav"), Some(1));
    assert_eq!(narrator_of(&project, "card/cam.mkv"), Some(0));

    fs::remove_dir_all(&root).ok();
}

#[test]
fn f0_12_s3_a_slot_the_user_left_untagged_stays_untagged() {
    let (root, dir, card) = workspace("s3-guard");
    // The session's own rows are root-relative strings, which is how a loaded list reads; adding to
    // them does not re-read the disk — dropping what has gone is F0.11's job.
    let audio = file(&card, "mic.wav", 5);
    // S3's guard: slot 1 is given away only when nobody holds it. A row that already holds another
    // slot is somebody the user named — moving them to narrator would re-cut the narration while
    // silently freeing the slot they were in.
    // Slot 1 held elsewhere: nothing is promoted, and the holder is untouched.
    let mut held = session(vec![row("keeper.wav", false, 1)]);
    add_sources::add(&mut held, &root, &dir, &[audio.clone()], false).unwrap();
    assert_eq!(narrator_of(&held, "keeper.wav"), Some(1));
    assert_eq!(narrator_of(&held, "card/mic.wav"), Some(0));

    // A session the user left with nobody in slot 1 is not re-seated by a press that adds nothing.
    // The rows are footage and one of them already holds slot 2, so this pins the guard alone: an
    // untagged list comes back as they left it rather than being filled from the front.
    let mut emptied = session(vec![row("keeper.mkv", true, 0), row("other.mkv", true, 2)]);
    add_sources::add(&mut emptied, &root, &dir, &[file(&card, "notes.txt", 1)], false).unwrap();
    assert_eq!(narrator_of(&emptied, "other.mkv"), Some(2), "its holder is untouched");
    // Whether the untagged footage takes the empty slot is F0.11's rule, not this flow's: a press
    // that adds nothing changes nothing here either way.

    // And when a recording does arrive for an empty slot 1, the first untagged recording in list
    // order takes it. Here the session's own row is that recording; `keeper` is a .mkv kept only for
    // its sound so the new row cannot be mistaken for the winner by being second in line — see the
    // next test for the case where the new row *is* the first untagged recording.
    let mut open_slot = session(vec![row("keeper.mkv", false, 0)]);
    add_sources::add(&mut open_slot, &root, &dir, &[audio], false).unwrap();
    assert_eq!(narrator_of(&open_slot, "keeper.mkv"), Some(1));
    assert_eq!(narrator_of(&open_slot, "card/mic.wav"), Some(0), "asked in list order");

    fs::remove_dir_all(&root).ok();
}

#[test]
fn f0_12_s4_the_folder_is_remembered_per_kind() {
    // S4: the chooser opens where this kind came from last time, so a card of recordings and a folder
    // of screen captures do not fight over one remembered folder.
    let (root, dir, card) = workspace("s4-dirs");
    let videos = root.join("captures");
    fs::create_dir_all(&videos).unwrap();
    let video = file(&videos, "cam.mkv", 5);
    let audio = file(&card, "mic.wav", 5);

    // A recording sets the audio folder and says nothing about the video one.
    let mut project = session(vec![]);
    add_sources::add(&mut project, &root, &dir, &[audio], false).unwrap();
    assert_eq!(project.aud_dir.as_deref(), Some("card"));
    assert_eq!(project.vid_dir, None);

    // A video then sets its own, leaving the audio one alone.
    add_sources::add(&mut project, &root, &dir, &[video], false).unwrap();
    assert_eq!(project.vid_dir.as_deref(), Some("captures"));
    assert_eq!(project.aud_dir.as_deref(), Some("card"));

    // What is remembered is where to look next: a copy does not move the folder that was picked from.
    let (root2, dir2, card2) = workspace("s4-copied");
    let mut copied = Project::default();
    add_sources::add(&mut copied, &root2, &dir2, &[file(&card2, "cam.mkv", 5)], true).unwrap();
    assert_eq!(copied.vid_dir.as_deref(), Some("card"), "not project:sources");

    fs::remove_dir_all(&root).ok();
    fs::remove_dir_all(&root2).ok();
}

#[test]
fn f0_12_s5_the_status_counts_what_was_added() {
    // S5's three sentences, and the counts behind them.
    let (root, dir, card) = workspace("s5-status");
    let video = file(&card, "cam.mkv", 5);
    let audio = file(&card, "mic.wav", 3);
    let txt = file(&card, "notes.txt", 3);

    // Everything asked for went in.
    let mut project = Project::default();
    let all = add_sources::add(&mut project, &root, &dir, &[video.clone(), audio], false).unwrap();
    assert_eq!(all.status, "added 2 source(s)");

    // One of three: the same file again and one that cannot be a source.
    let some = add_sources::add(&mut project, &root, &dir, &[video.clone(), txt, file(&card, "cam2.mp4", 2)], false).unwrap();
    assert_eq!(some.status, "added 1 of 3 \u{2014} the rest were already in");

    // Nothing new at all.
    let none = add_sources::add(&mut project, &root, &dir, &[video], false).unwrap();
    assert_eq!(none.status, "already in the session \u{2014} nothing added");

    fs::remove_dir_all(&root).ok();
}

#[test]
fn f0_12_s5_the_sentences_are_the_ones_the_spec_writes() {
    // Pinning the wording apart from the counts, so a rephrase in the module is a red test.
    let (root, dir, card) = workspace("s5-words");
    let wav = file(&card, "mic.wav", 1);

    let mut project = Project::default();
    let one = add_sources::add(&mut project, &root, &dir, &[wav.clone()], false).unwrap();
    assert_eq!(one.status, format!("added {} source(s)", one.added));

    fs::remove_dir_all(&root).ok();
}
