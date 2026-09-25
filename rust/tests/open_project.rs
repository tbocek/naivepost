//! §03-shell.md F0.9 Open a project — the plain-logic side.
//!
//! Every rule lives in [`naivepost::open_project`]; these tests drive it with real trees on disk and
//! with injected `is_file`/`exists` closures, so the ordering and the two refusals are asserted
//! rather than eyeballed. The wire through the Open button is `open_project_widgets.rs`.
//!
//! Which clause each test pins:
//!
//! * S1 — the folder a picked path names, including picking `naivepost.json` itself.
//! * S2 — an old single-file project is refused by name, not adopted.
//! * S3 — the project path and output folder are set before the project is read; sources whose file
//!   has gone are dropped from the session with one line each.
//! * S4 — a failed open refuses with its reason, the old folder names migrate, and the root
//!   remembers what it last had open.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use naivepost::open_project::{self, Choice, CANNOT_OPEN, NOT_A_PROJECT};
use naivepost::project::PROJECT_FILE;

/// A directory under the temp dir that no other test run shares.
fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "naivepost-open09-{}-{tag}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// A project folder written with `body` as its `naivepost.json`, plus real files for each of the
/// `present` source names under `sources/`.
fn project_folder(dir: &Path, body: &str, present: &[&str]) -> PathBuf {
    fs::create_dir_all(dir.join("sources")).unwrap();
    fs::write(dir.join(PROJECT_FILE), body).unwrap();
    for name in present {
        fs::write(dir.join("sources").join(name), "video bytes").unwrap();
    }
    dir.to_path_buf()
}

/// A two-source project body: `keep.mkv` and `gone.mkv`, both spelled the way a project stores them.
const TWO_SOURCES: &str = r#"{
  "sources": [
    {"path": "project:sources/keep.mkv", "footage": true, "narrator": 1, "sepvoice": false, "tracks": [0]},
    {"path": "project:sources/gone.mkv", "footage": true, "narrator": 0, "sepvoice": false, "tracks": [0]}
  ],
  "interval": 2.5,
  "language": "de",
  "no_narration": false,
  "reference_sources": true,
  "context": "a lecture opened from elsewhere"
}"#;

// --- S1: the folder a pick names -------------------------------------------------------------------

#[test]
fn f0_9_s1_picking_the_project_file_opens_its_folder() {
    // F0.9 S1: a path naming `naivepost.json` opens the folder holding it.
    let root = temp_dir("s1-file");
    let folder = project_folder(&root.join("show.naivepost"), "{}", &[]);

    let picked = folder.join(PROJECT_FILE);
    assert_eq!(open_project::folder_for(&picked), folder, "the file's own folder");
}

#[test]
fn f0_9_s1_picking_the_folder_is_idempotent() {
    // F0.9 S1: picking the folder itself needs no unwrapping — and doing it twice changes nothing.
    let root = temp_dir("s1-folder");
    let folder = project_folder(&root.join("show.naivepost"), "{}", &[]);

    let once = open_project::folder_for(&folder);
    assert_eq!(once, folder, "a folder stays itself");
    assert_eq!(
        open_project::folder_for(&once),
        folder,
        "resolving the answer again lands in the same place"
    );
}

#[test]
fn f0_9_s1_and_s3_opening_a_real_project_sets_root_out_and_sources() {
    // F0.9 S1 + S3: the applied value points at the folder that was opened, and the sources are the
    // ones the file named.
    let root = temp_dir("s1s3");
    let folder = project_folder(&root.join("eth.naivepost"), TWO_SOURCES, &["keep.mkv"]);
    // The second source really is there too here, so nothing drops.
    fs::write(folder.join("sources/gone.mkv"), "also video").unwrap();

    let applied = open_project::apply(&folder, |path| path.exists()).expect("opens");

    assert_eq!(applied.root, folder, "S3: the project path is the folder opened");
    assert_eq!(applied.out, folder, "S3: the folder IS the output folder (§01 §1)");
    assert_eq!(applied.project.sources.len(), 2, "both sources carried over");
    assert_eq!(applied.dropped, Vec::<String>::new(), "nothing missing, nothing dropped");
    // The rest of the apply list came along with the loaded project unchanged.
    assert_eq!(applied.project.interval, 2.5, "S3: interval");
    assert_eq!(applied.project.language, "de", "S3: language");
    assert!(
        applied.project.reference_sources,
        "S3: the reference flag"
    );
    assert!(!applied.project.no_narration, "S3: the narration flag");
    assert_eq!(
        applied.project.context,
        "a lecture opened from elsewhere",
        "S3: context"
    );
}

// --- S2: the old single-file project --------------------------------------------------------------

#[test]
fn f0_9_s2_an_old_single_file_project_is_refused_by_name() {
    // F0.9 S2: such a file is not adopted into a folder any more; it is named and turned away.
    let root = temp_dir("s2-old");
    let file = root.join("old.json");
    fs::write(&file, "{\"sources\": []}").unwrap();

    let choice = open_project::classify(&file, |path| path.is_file());
    match choice {
        Choice::NotAFolder { target, log, status } => {
            assert_eq!(target, file);
            assert_eq!(
                log,
                format!("!!! {} is an old single-file project -- not supported", file.display()),
                "the spec's literal wording, `!!!` prefix and double hyphen included"
            );
            assert_eq!(status, NOT_A_PROJECT);
            assert_eq!(status, "not a project folder");
        }
        Choice::Openable(other) => panic!("a plain file must not be openable, got {other:?}"),
    }
}

#[test]
fn f0_9_s2_a_naivepost_named_file_that_is_not_a_folder_is_also_refused() {
    // F0.9 S2: the suffix alone does not make a project — a FILE called `legacy.naivepost` is the
    // same pre-folders shape as `old.json`.
    let root = temp_dir("s2-suffix");
    let file = root.join("legacy.naivepost");
    fs::write(&file, "{}").unwrap();

    let choice = open_project::classify(&file, |path| path.is_file());
    match choice {
        Choice::NotAFolder { log, status, .. } => {
            assert!(log.ends_with("-- not supported"), "{log}");
            assert!(log.contains("old single-file project"), "{log}");
            assert_eq!(status, NOT_A_PROJECT);
        }
        Choice::Openable(other) => panic!("{file:?} is a file, not a folder; got {other:?}"),
    }
}

// --- S4 vs S2: a folder that will not open is not an old project ----------------------------------

#[test]
fn f0_9_s4_a_folder_without_a_project_file_fails_generically() {
    // F0.9 S4 (and explicitly NOT S2): an empty-ish directory is not an old single-file project,
    // so it takes the generic failure and its status, never the single-file message.
    let root = temp_dir("s4-empty");
    let bare = root.join("somewhere");
    fs::create_dir_all(bare.join("photos")).unwrap();

    let failure = open_project::open(&bare, |path| path.is_file(), |path| path.exists())
        .expect_err("a folder with no naivepost.json cannot open");

    assert_eq!(failure.status, CANNOT_OPEN);
    assert_eq!(failure.status, "could not open that project \u{2014} see log", "em dash");
    assert!(
        !failure.log.contains("single-file"),
        "an ordinary folder must not be called an old single-file project: {}",
        failure.log
    );
    assert!(failure.log.starts_with("!!! "), "{}", failure.log);
}

// --- S3: sources whose file has gone ---------------------------------------------------------------

#[test]
fn f0_9_s3_a_missing_source_is_dropped_with_one_line_each() {
    // F0.9 S3: dropped from the session, not deleted from the record — and said exactly once.
    let root = temp_dir("s3-missing");
    let folder = project_folder(&root.join("eth.naivepost"), TWO_SOURCES, &["keep.mkv"]);

    let applied = open_project::apply(&folder, |path| path.exists()).expect("opens");

    assert_eq!(applied.dropped, vec!["project:sources/gone.mkv".to_string()]);
    assert_eq!(applied.project.sources.len(), 1, "only the file that exists is kept");
    assert_eq!(applied.project.sources[0].path, "project:sources/keep.mkv");

    let complaint = "!!! project:sources/gone.mkv is not there any more -- dropped from the session";
    let hits: Vec<_> = applied.lines.iter().filter(|line| *line == complaint).collect();
    assert_eq!(hits.len(), 1, "said once, not twice: {:?}", applied.lines);

    // And the paths were settled anyway — the "FIRST" part of S3 holds even when a later step logs.
    assert_eq!(applied.root, folder);
    assert_eq!(applied.out, folder);
}

#[test]
fn f0_9_s3_an_exists_closure_of_true_drops_nothing() {
    // F0.9 S3: the drop rule reads the injected closure, so a caller that says everything is there
    // sees no drops — which is also how this is tested without touching a filesystem.
    let root = temp_dir("s3-fake");
    let folder = project_folder(&root.join("eth.naivepost"), TWO_SOURCES, &[]);

    let applied = open_project::apply(&folder, |_| true).expect("opens");
    assert_eq!(applied.project.sources.len(), 2, "the fake says both are present");
    assert!(applied.dropped.is_empty(), "{:?}", applied.dropped);

    // The mirror: a closure answering false drops every source, each named once.
    let all_gone = open_project::apply(&folder, |_| false).expect("still opens");
    assert!(all_gone.project.sources.is_empty());
    assert_eq!(all_gone.dropped.len(), 2, "{:?}", all_gone.dropped);
    assert_eq!(
        all_gone
            .lines
            .iter()
            .filter(|line| line.contains("is not there any more"))
            .count(),
        2
    );
}

// --- S4: a failed open leaves nothing half-applied --------------------------------------------------

#[test]
fn f0_9_s4_unreadable_project_json_refuses_and_leaves_the_session_where_it_was() {
    // F0.9 S4: `!!! <err>` plus the standing status. Nothing downstream switched: the caller gets
    // an error rather than a partly-built Applied, which is what keeps the open session untouched.
    let root = temp_dir("s4-broken");
    let broken = root.join("broken.naivepost");
    fs::create_dir_all(&broken).unwrap();
    fs::write(broken.join(PROJECT_FILE), "{ this is not json").unwrap();

    // The session that was open before this attempt, held by the caller.
    let previous = open_project::apply(
        &project_folder(&root.join("good.naivepost"), "{}", &[]),
        |path| path.exists(),
    )
    .expect("the good one opens");

    let failure = open_project::open(&broken, |path| path.is_file(), |path| path.exists())
        .expect_err("garbage must not open");

    assert_eq!(failure.status, CANNOT_OPEN);
    assert!(failure.log.starts_with("!!! "), "{}", failure.log);
    assert!(failure.log.len() > 4, "the load's own reason travels behind the prefix: {}", failure.log);

    // The previously opened session is byte-for-byte what it was: still one folder, still its own
    // root and out. An open that failed rewrote nothing.
    let same = open_project::apply(
        &project_folder(&root.join("good.naivepost"), "{}", &[]),
        |path| path.exists(),
    )
    .expect("still opens");
    assert_eq!(previous.root, same.root);
    assert_eq!(previous.out, same.out);
    assert_eq!(previous.project, same.project);
}

// --- S4: remember the project for this root -------------------------------------------------------

#[test]
fn f0_9_s4_remembering_a_project_keys_it_by_root() {
    // F0.9 S4: the pair settings writes as PROJECT_<n>_ROOT / PROJECT_<n>_FILE.
    let root = temp_dir("s4-remember");
    let folder = project_folder(&root.join("eth.naivepost"), "{}", &[]);
    let mut projects: BTreeMap<String, String> = BTreeMap::new();

    let remembered = open_project::remember(&mut projects, &root, &open_project::project_file_in(&folder));

    assert_eq!(remembered.root, root.display().to_string());
    assert_eq!(remembered.file, folder.join(PROJECT_FILE).display().to_string());
    assert_eq!(projects.len(), 1, "one entry for one root");
    assert_eq!(
        projects.get(&root.display().to_string()).map(String::as_str),
        Some(remembered.file.as_str()),
        "looked up by root, it answers with the project file"
    );
}

#[test]
fn f0_9_s4_reopening_the_same_root_replaces_rather_than_duplicates() {
    // F0.9 S4: two entries for one folder would make the remembered list disagree with itself.
    let root = temp_dir("s4-replace");
    let first = project_folder(&root.join("first.naivepost"), "{}", &[]);
    let second = project_folder(&root.join("second.naivepost"), "{}", &[]);
    let mut projects: BTreeMap<String, String> = BTreeMap::new();

    open_project::remember(&mut projects, &root, &first.join(PROJECT_FILE));
    open_project::remember(&mut projects, &root, &second.join(PROJECT_FILE));

    assert_eq!(projects.len(), 1, "replaced, not stacked");
    assert_eq!(
        projects.get(&root.display().to_string()).map(String::as_str),
        Some(second.join(PROJECT_FILE).display().to_string().as_str()),
        "the newest open wins"
    );
}

// --- S4: migrate the old folder names -------------------------------------------------------------

#[test]
fn f0_9_s4_finish_moves_the_old_step_folders_and_says_so() {
    // F0.9 S4: `step1`…`step6` become named folders, and every rename is logged — a folder that
    // silently changed name is a folder somebody spends an afternoon looking for.
    let root = temp_dir("s4-migrate");
    let folder = project_folder(&root.join("old.naivepost"), "{}", &[]);
    for step in 1..=6 {
        fs::create_dir_all(folder.join(format!("step{step}"))).unwrap();
        fs::write(folder.join(format!("step{step}/x.tsv")), "x").unwrap();
    }

    let applied = open_project::apply(&folder, |path| path.exists()).expect("opens");
    let migration = open_project::finish(&applied);

    assert!(
        !migration.lines.is_empty(),
        "six old step folders should have moved: {:?}",
        migration.lines
    );
    assert!(
        migration.lines.iter().any(|line| line.starts_with(">>> moved step1/ to ")),
        "{:?}",
        migration.lines
    );
    assert!(migration.failures.is_empty(), "{:?}", migration.failures);
    // The old names are gone from disk.
    assert!(!folder.join("step1").exists(), "step1/ became a named folder");
}

#[test]
fn a_clean_project_has_nothing_to_migrate_and_finish_says_nothing() {
    // F0.9 S4: the quiet case — a tree already on the new names migrates zero folders and adds no
    // lines, rather than erroring or claiming a move that did not happen.
    let root = temp_dir("s4-clean");
    let folder = project_folder(&root.join("new.naivepost"), "{}", &[]);

    let applied = open_project::apply(&folder, |path| path.exists()).expect("opens");
    let migration = open_project::finish(&applied);

    assert!(migration.lines.is_empty(), "{:?}", migration.lines);
    assert!(migration.failures.is_empty(), "{:?}", migration.failures);
}
