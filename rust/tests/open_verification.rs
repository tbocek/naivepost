//! §03-shell#8-details-confirmed-against-the-code-verification-pass — the open
//! details: a path naming `naivepost.json` opens its folder, a project that used
//! to write elsewhere says which folder it now writes into and that the old one
//! is untouched, and an old folder layout is moved in two logged passes.

use std::fs;
use std::path::{Path, PathBuf};

use naivepost::layout::Tree;
use naivepost::{migrate, project, startup};

/// A directory under the temp dir that no other test run shares.
fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("naivepost-open-{}-{tag}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn folder(dir: &Path, name: &str) -> PathBuf {
    let path = dir.join(name);
    fs::create_dir_all(path.join("x")).unwrap();
    path
}

/// A project written with the given JSON in it.
fn project_at(dir: &Path, body: &str) -> PathBuf {
    fs::create_dir_all(dir).unwrap();
    fs::write(dir.join(project::PROJECT_FILE), body).unwrap();
    dir.to_path_buf()
}

/// A file's contents as text.
fn read(path: PathBuf) -> String {
    fs::read_to_string(path).unwrap()
}

#[test]
fn sec_03_shell_8_details_confirmed_against_the_code_verification_pass_open_a_path_naming_the_project_file_opens_its_folder()
{
    let root = temp_dir("json");
    let project = project_at(&root.join("show.naivepost"), "{}");
    let handed = project.join(project::PROJECT_FILE);

    assert_eq!(startup::project_folder(&handed), project);
    assert_eq!(startup::project_folder(&project), project);

    // The desktop handing over the file is asking for that project's folder.
    let opened = startup::decide(&root, &Default::default(), &[handed]);
    assert_eq!(opened, startup::Opened::Desktop { path: project });
}

#[test]
fn sec_03_shell_8_details_confirmed_against_the_code_verification_pass_a_remembered_project_file_opens_its_folder_too()
{
    let root = temp_dir("remembered");
    let project = project_at(&root.join("show.naivepost"), "{}");
    let mut conf = naivepost::settings::Conf::default();
    conf.projects
        .insert(root.to_string_lossy().to_string(), project.join(project::PROJECT_FILE).display().to_string());

    assert_eq!(
        startup::decide(&root, &conf, &[]),
        startup::Opened::Remembered { path: project }
    );
}

#[test]
fn sec_03_shell_8_details_confirmed_against_the_code_verification_pass_legacy_out_dir_names_both_folders_and_says_the_old_one_is_untouched()
{
    assert_eq!(
        project::out_dir_note("/old/out", "/new/show.naivepost"),
        "!!! this project used to write into /old/out and now writes into /new/show.naivepost -- the old folder is untouched"
    );

    let root = temp_dir("outdir");
    let dir = root.join("show.naivepost");
    project_at(&dir, r#"{"out_dir":"/old/out"}"#);
    let (loaded, report) = project::load_report(&dir).unwrap();
    assert_eq!(loaded.aud_dir.as_deref(), Some("/old/out"));
    assert!(
        report.iter().any(|line| line
            == "!!! this project used to write into /old/out and now writes into show.naivepost -- the old folder is untouched"
            || line.contains("used to write into /old/out")),
        "{report:?}"
    );

    // A project that already wrote into itself has nothing to say.
    let same = root.join("same.naivepost");
    for spelling in [".", "same.naivepost"] {
        project_at(&same, &format!(r#"{{"out_dir":"{spelling}"}}"#));
        let (_, report) = project::load_report(&same).unwrap();
        assert!(
            !report.iter().any(|line| line.contains("used to write into")),
            "{spelling}: {report:?}"
        );
    }
}

#[test]
fn sec_03_shell_8_details_confirmed_against_the_code_verification_pass_folder_migration_moves_in_two_passes_and_logs_each_move()
{
    let root = temp_dir("two-passes");
    let dir = root.join("show.naivepost");
    fs::create_dir_all(&dir).unwrap();
    // An old project: `step1`…`step6` beside each other, and an `understand/` that
    // is still Describe's and Transcript's own folder — pass one cannot rename
    // `step2` onto it, and pass two empties it from underneath.
    for step in 1..=6 {
        folder(&dir, &format!("step{step}"));
    }
    fs::create_dir_all(dir.join("understand/describe")).unwrap();
    fs::write(dir.join("understand/describe/events.tsv"), "x").unwrap();
    fs::create_dir_all(dir.join("understand/transcript")).unwrap();
    fs::write(dir.join("understand/transcript/session.tsv"), "y").unwrap();

    let (one, fails_one) = migrate::pass_one(&dir);
    // `step2` had nowhere to go: `understand/` is still holding Describe's and
    // Transcript's own work, so renaming onto it would have buried it. Logged, with
    // `step2` left exactly where it was; pass two gets those files under `prepare/`.
    assert_eq!(
        fails_one,
        vec![format!(
            "!!! could not move step2/ to understand/: {} already exists -- the files are still under the old name",
            dir.join("understand").display()
        )]
    );
    let (two, fails_two) = migrate::pass_two(&dir);
    assert!(fails_two.is_empty(), "{fails_two:?}");

    // Every move logged once, and nothing else. `step2` is absent: it could not be
    // renamed onto the `understand/` that still held work (asserted above).
    let lines = migrate::log_lines(&one);
    assert_eq!(
        lines,
        vec![
            ">>> moved step1/ to inputs/",
            ">>> moved step3/ to cut/",
            ">>> moved step4/ to narrate/",
            ">>> moved step5/ to produce/",
            ">>> moved step6/ to publish/",
        ]
    );
    assert_eq!(
        migrate::log_lines(&two),
        vec![
            ">>> moved understand/describe/ to prepare/describe/",
            ">>> moved understand/transcript/ to prepare/transcript/",
            ">>> moved inputs/ to prepare/inputs/",
        ],
        "understand/'s halves move before it is checked for leftovers"
    );

    // And they landed where the layout says they live.
    let tree = Tree::new(&dir).unwrap();
    assert!(dir.join("prepare/inputs/x").is_dir());
    assert_eq!(read(dir.join("prepare/describe/events.tsv")), "x");
    assert_eq!(
        read(tree.session_tsv()),
        "y",
        "the merged timeline is prepare/transcript/session.tsv"
    );
    for gone in ["step1", "step3", "step4", "step5", "step6"] {
        assert!(!dir.join(gone).exists(), "{gone} is still there");
    }
    assert!(dir.join("step2/x").is_dir(), "the one failure left its folder in place");

    // A second open has nothing left to move and says nothing — even the folder it
    // could not rename is now an empty `step2`, which pass two takes as `inputs/`.
    let (again, fails) = migrate::pass_two(&dir);
    assert!(again.is_empty() && fails.is_empty(), "{again:?} {fails:?}");
    assert!(dir.join("prepare/inputs/x").is_dir());
}

#[test]
fn sec_03_shell_8_details_confirmed_against_the_code_verification_pass_folder_migration_leaves_what_it_cannot_move_and_logs_it()
{
    let root = temp_dir("stuck");
    let dir = root.join("show.naivepost");
    fs::create_dir_all(dir.join("step3")).unwrap();
    fs::write(dir.join("step3/cut.json"), "old").unwrap();
    // The new name is already taken by work of its own.
    fs::create_dir_all(dir.join("cut")).unwrap();
    fs::write(dir.join("cut/cut.json"), "new").unwrap();

    let (moved, failures) = migrate::pass_one(&dir);
    assert!(moved.is_empty(), "{moved:?}");
    assert_eq!(failures.len(), 1, "{failures:?}");
    assert_eq!(
        failures[0],
        format!(
            "!!! could not move step3/ to cut/: {} already exists -- the files are still under the old name",
            dir.join("cut").display()
        )
    );

    // Left in place, both of them: nothing was overwritten.
    assert_eq!(read(dir.join("step3/cut.json")), "old");
    assert_eq!(read(dir.join("cut/cut.json")), "new");
}

#[test]
fn sec_03_shell_8_details_confirmed_against_the_code_verification_pass_folder_migration_removes_understand_only_when_it_is_empty()
{
    let root = temp_dir("understand");
    let emptied = root.join("a.naivepost");
    fs::create_dir_all(emptied.join("understand/describe")).unwrap();
    migrate::pass_two(&emptied);
    assert!(!emptied.join("understand").exists(), "the emptied folder is gone");

    let stray = root.join("b.naivepost");
    fs::create_dir_all(stray.join("understand/describe")).unwrap();
    fs::write(stray.join("understand/notes.txt"), "not ours").unwrap();
    migrate::pass_two(&stray);
    assert!(stray.join("understand").is_dir(), "one file left is not empty");
    assert_eq!(read(stray.join("understand/notes.txt")), "not ours");
}
