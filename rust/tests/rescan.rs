//! §03-shell.md F0.11 Rescan — noticing what changed on disk.
//!
//! S1 dropped the sources whose files are gone and re-assigned narrator slot 1 when its holder went;
//! S2 refreshed every tab's readouts, rebuilt Cut and re-read Narrate's file; S3 said "rescanned".
//! All of it is [`naivepost::rescan`]'s decision — the toolbar button only forwards (spec/00-principles.md §5).

use std::fs;
use std::path::{Path, PathBuf};

use naivepost::cut::{self, Cut, Seg};
use naivepost::layout::Tree;
use naivepost::narration::{self, Entry, Narration};
use naivepost::project::{Project, Source};
use naivepost::rescan::{self, Refreshed, RESCANNED};
use naivepost::shell::{Page, Shell};

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "naivepost-rescan-{}-{tag}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// A project folder with the three recordings a session names, so "gone" and "there" are both real
/// files rather than paths that merely look plausible.
fn project_folder(tag: &str) -> (PathBuf, Tree) {
    let root = temp_dir(tag);
    let dir = root.join("talk.naivepost");
    fs::create_dir_all(dir.join("sources")).unwrap();
    for name in ["mic.wav", "second.wav", "cam.mp4"] {
        fs::write(dir.join("sources").join(name), b"audio").unwrap();
    }
    let tree = Tree::new(&dir).expect("a .naivepost folder is a project");
    (root, tree)
}

fn source(path: &str, footage: bool, narrator: u32) -> Source {
    Source {
        path: path.to_string(),
        footage,
        narrator,
        sepvoice: true,
        tracks: vec![],
    }
}

/// A narrator, a recording and a screen capture: the shapes slot 1's re-assignment has to choose
/// between.
fn session() -> Project {
    Project {
        sources: vec![
            source("project:sources/mic.wav", false, 1),
            source("project:sources/cam.mp4", true, 0),
        ],
        ..Default::default()
    }
}

fn run(root: &Path, tree: Option<&Tree>, project: &mut Project) -> rescan::Rescan {
    let dir = tree.map(|t| t.dir().to_path_buf()).unwrap_or_else(|| root.join("talk.naivepost"));
    rescan::rescan(project, root, &dir, tree)
}

#[test]
fn f0_11_s1_a_source_whose_file_is_gone_is_dropped_by_name() {
    // S1: the row goes and says so, with the path the user recognises — the stored one, not the
    // absolute path it resolved to.
    let (root, tree) = project_folder("s1-drop");
    let mut project = session();
    fs::remove_file(tree.dir().join("sources/mic.wav")).unwrap();

    let found = run(&root, Some(&tree), &mut project);

    assert_eq!(found.dropped, vec!["project:sources/mic.wav".to_string()]);
    assert_eq!(
        found.logs,
        vec!["!!! dropped project:sources/mic.wav -- it is no longer there".to_string()]
    );
    // The survivor keeps its row and everything else about it.
    assert_eq!(project.sources.len(), 1);
    assert_eq!(project.sources[0].path, "project:sources/cam.mp4");

    fs::remove_dir_all(&root).ok();
}

#[test]
fn f0_11_s1_a_scan_that_finds_everything_there_drops_and_says_nothing() {
    let (root, tree) = project_folder("s1-intact");
    let mut project = session();

    let found = run(&root, Some(&tree), &mut project);

    assert!(found.dropped.is_empty());
    assert!(found.logs.is_empty());
    assert_eq!(project.sources.len(), 2);

    fs::remove_dir_all(&root).ok();
}

#[test]
fn f0_11_s1_slot_1_goes_to_the_first_surviving_recording() {
    // S1's other half. A recording is the better guess at the narrator than a screen capture, which
    // holds everyone at once; and a row that already holds a slot is never moved off it — so the
    // second microphone wins even though the camera comes to slot 2 in another session.
    let (root, tree) = project_folder("s1-slot1");
    let mut project = Project {
        sources: vec![
            source("project:sources/mic.wav", false, 1),
            source("project:sources/cam.mp4", true, 0),
            source("project:sources/second.wav", false, 0),
        ],
        ..Default::default()
    };
    fs::remove_file(tree.dir().join("sources/mic.wav")).unwrap();

    let found = run(&root, Some(&tree), &mut project);

    assert_eq!(found.reassigned.as_deref(), Some("project:sources/second.wav"));
    let narrator = |path: &str| {
        project
            .sources
            .iter()
            .find(|s| s.path == path)
            .map(|s| s.narrator)
    };
    assert_eq!(narrator("project:sources/second.wav"), Some(1));
    assert_eq!(narrator("project:sources/cam.mp4"), Some(0));

    fs::remove_dir_all(&root).ok();
}

#[test]
fn f0_11_s1_slot_1_falls_through_to_footage_when_no_recording_survives() {
    let (root, tree) = project_folder("s1-slot1-footage");
    let mut project = session();
    fs::remove_file(tree.dir().join("sources/mic.wav")).unwrap();

    let found = run(&root, Some(&tree), &mut project);

    assert_eq!(found.reassigned.as_deref(), Some("project:sources/cam.mp4"));
    assert_eq!(project.sources[0].narrator, 1);

    fs::remove_dir_all(&root).ok();
}

#[test]
fn f0_11_s1_slot_1_is_left_alone_when_its_holder_stayed() {
    // The re-assignment is for a slot that *lost its holder*. Untagging the narrator and rescanning
    // must not quietly put somebody back in it.
    let (root, tree) = project_folder("s1-slot1-kept");
    let mut project = Project {
        sources: vec![
            source("project:sources/mic.wav", false, 0),
            source("project:sources/cam.mp4", true, 0),
        ],
        ..Default::default()
    };

    let found = run(&root, Some(&tree), &mut project);

    assert_eq!(found.reassigned, None);
    assert!(project.sources.iter().all(|s| s.narrator == 0));

    fs::remove_dir_all(&root).ok();
}

#[test]
fn f0_11_s2_the_readouts_cut_and_narration_are_all_refreshed() {
    // S2's three clauses, and the two files are re-read from disk rather than remembered: narration
    // still on screen after its folder changed is the bug this flow exists to avoid.
    let (root, tree) = project_folder("s2-refresh");
    let mut project = session();
    fs::remove_file(tree.dir().join("sources/mic.wav")).unwrap();

    let found = run(&root, Some(&tree), &mut project);

    assert_eq!(
        found.refreshed,
        vec![
            Refreshed::Readouts,
            Refreshed::CutRebuild,
            Refreshed::NarrateReRead,
        ]
    );
    fs::remove_dir_all(&root).ok();
}

#[test]
fn f0_11_s2_cut_and_narrate_re_read_what_is_on_disk_now() {
    let (root, tree) = project_folder("s2-files");
    let mut project = session();
    cut::save(
        &Cut {
            segs: vec![Seg {
                s: 0.0,
                e: 12.5,
                ..Default::default()
            }],
            ..Default::default()
        },
        &tree,
    )
    .unwrap();
    narration::save(
        &Narration {
            entries: vec![Entry {
                s: 0.0,
                e: 12.5,
                text: "the line on disk".to_string(),
                ..Default::default()
            }],
            silent: vec![],
        },
        &tree,
    )
    .unwrap();

    let found = run(&root, Some(&tree), &mut project);

    let cut = found.cut.expect("the cut on disk is re-read");
    assert_eq!(cut.segs.len(), 1);
    assert_eq!(cut.segs[0].e, 12.5);
    let narration_file = found.narration.expect("narration's file is re-read");
    assert_eq!(narration_file.entries[0].text, "the line on disk");

    // And a file that went since the last scan is gone from the refresh too, not remembered.
    fs::remove_file(tree.narration_json()).unwrap();
    let after = run(&root, Some(&tree), &mut project);
    assert_eq!(after.narration, Some(Narration::default()));

    fs::remove_dir_all(&root).ok();
}

#[test]
fn f0_11_s2_the_refreshed_files_drive_the_tabs_readouts() {
    // "Refresh every tab's readouts" means the numbers on the tabs come from what the scan re-read.
    let (root, tree) = project_folder("s2-readouts");
    let mut project = session();
    cut::save(
        &Cut {
            segs: vec![Seg {
                s: 0.0,
                e: 4.0,
                ..Default::default()
            }],
            ..Default::default()
        },
        &tree,
    )
    .unwrap();
    narration::save(
        &Narration {
            entries: vec![Entry {
                s: 0.0,
                e: 4.0,
                text: "a line".to_string(),
                ..Default::default()
            }],
            silent: vec![],
        },
        &tree,
    )
    .unwrap();
    fs::remove_file(tree.dir().join("sources/mic.wav")).unwrap();

    let found = run(&root, Some(&tree), &mut project);

    // Prepare counts the surviving rows; Cut and Narrate count what the scan re-read, each on its
    // own page — `inputs` reads the page it is asked about.
    let cut = found.cut.clone().unwrap();
    let narration_file = found.narration.clone().unwrap();
    let on = |page: Page| Shell { page, ..Default::default() };
    assert_eq!(
        on(Page::Prepare).inputs(&project, &cut, &narration_file),
        "1 source(s), 1 of them footage"
    );
    assert_eq!(on(Page::Cut).inputs(&project, &cut, &Narration::default()), "1 clip(s)");
    assert_eq!(
        on(Page::Narrate).inputs(&project, &cut, &narration_file),
        "1 line(s) over 1 clip(s)"
    );

    fs::remove_dir_all(&root).ok();
}

#[test]
fn f0_11_s2_a_session_with_no_project_folder_reads_back_no_files() {
    // Nothing on disk to re-read: the readouts are still refreshed, but Cut and Narrate report
    // nothing rather than inventing an empty file.
    let root = temp_dir("s2-nofolder");
    let mut project = session();

    let found = run(&root, None, &mut project);

    assert_eq!(found.refreshed, vec![Refreshed::Readouts]);
    assert_eq!(found.cut, None);
    assert_eq!(found.narration, None);
    // Every source resolves against a folder that does not exist, so all of them are gone.
    assert_eq!(found.dropped.len(), 2);

    fs::remove_dir_all(&root).ok();
}

#[test]
fn f0_11_s3_the_status_is_rescanned_whatever_the_scan_found() {
    let (root, tree) = project_folder("s3-status");
    let mut project = session();
    assert_eq!(run(&root, Some(&tree), &mut project).status, RESCANNED);

    fs::remove_file(tree.dir().join("sources/cam.mp4")).unwrap();
    assert_eq!(run(&root, Some(&tree), &mut project).status, RESCANNED);
    assert_eq!(RESCANNED, "rescanned");

    fs::remove_dir_all(&root).ok();
}
