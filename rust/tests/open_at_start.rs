//! §03-shell.md F0.6 Open at start — which project a launch opens.
//!
//! Four questions in order, and the first yes wins: S1 a file handed by the desktop (the first
//! only), S2 the project this root last had open if it is still on disk, S3 `<root>/session.naivepost`
//! if it exists, S4 a blank session at that path. All of it is [`naivepost::startup`]'s decision;
//! `main` only opens what it returns, so every step is testable without a window or a display
//! (spec/00-principles.md §5).

use std::fs;
use std::path::{Path, PathBuf};

use naivepost::project::{self, Project};
use naivepost::settings::{self, Conf};
use naivepost::startup::{self, Opened};

/// A directory under the temp dir that no other test run shares.
fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "naivepost-open-at-start-{}-{tag}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    dir
}

/// A project folder on disk — what S2's "still on disk" and S3's "exists" both mean: the folder is
/// there, since a project is a folder holding `naivepost.json`.
fn project_folder(dir: &Path) -> PathBuf {
    fs::create_dir_all(dir).unwrap();
    project::save(&Project::default(), dir).unwrap();
    dir.to_path_buf()
}

/// A conf remembering one root → file pair. Built from `Conf::default()` rather than by literal
/// because `unknown` (the keys a newer build left behind) is private to settings.rs.
fn remembering(root: &Path, file: &Path) -> Conf {
    let mut conf = Conf::default();
    conf.projects
        .insert(root.display().to_string(), file.display().to_string());
    conf
}

#[test]
fn f0_6_s1_the_first_handed_file_is_the_one() {
    // S1: the desktop hands over one or more files and only the first is opened — one window, one
    // project. Nothing on disk sways it: the second path being a whole session beside the root
    // would be enough to open had nothing been handed over.
    let root = temp_dir("first");
    fs::create_dir_all(&root).unwrap();
    let first = project_folder(&root.join("first.naivepost"));
    let second = project_folder(&root.join("second.naivepost"));

    let opened = startup::decide(
        &root,
        &Conf::default(),
        &[first.clone(), second.clone()],
    );
    assert_eq!(opened, Opened::Desktop { path: first });

    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn f0_6_s1_a_handed_file_beats_what_was_remembered() {
    // A double-click is somebody asking for THAT project, not for whatever was open last — so S1
    // answers before either of the fallbacks is even asked.
    let root = temp_dir("handed-wins");
    fs::create_dir_all(&root).unwrap();
    let remembered = project_folder(&root.join("jan.naivepost"));
    let session = startup::session_dir(&root);
    fs::create_dir_all(&session).unwrap();
    let handed = project_folder(&root.join("feb.naivepost"));

    let conf = remembering(&root, &remembered);
    let opened = startup::decide(&root, &conf, &[handed.clone()]);
    assert_eq!(opened, Opened::Desktop { path: handed });

    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn f0_6_s1_a_handed_file_wins_even_when_it_is_not_there() {
    // No fallback here on purpose: opening last night's project because the one that was named
    // cannot be read is worse than an opener that says it could not read it. Reporting the failure
    // is the caller's, and reading the file is F0.9's round.
    let root = temp_dir("handed-missing");
    fs::create_dir_all(&root).unwrap();
    let remembered = project_folder(&root.join("jan.naivepost"));
    let session = startup::session_dir(&root);
    fs::create_dir_all(&session).unwrap();
    let gone = root.join("moved-away.naivepost");

    let conf = remembering(&root, &remembered);
    let opened = startup::decide(&root, &conf, &[gone.clone()]);
    assert_eq!(
        opened,
        Opened::Desktop { path: gone },
        "a missing handed file must not silently become the remembered project"
    );

    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn f0_6_s2_the_project_remembered_for_this_root_opens() {
    // S2: the setting is keyed by root, because every path inside a project is relative to it — so
    // starting from another folder opens that folder's own last project, not this one's.
    let root = temp_dir("remembered");
    fs::create_dir_all(&root).unwrap();
    let here = project_folder(&root.join("jan.naivepost"));
    let elsewhere_root = temp_dir("remembered-elsewhere");
    fs::create_dir_all(&elsewhere_root).unwrap();
    let elsewhere = project_folder(&elsewhere_root.join("feb.naivepost"));

    let mut conf = remembering(&root, &here);
    conf.projects.insert(
        elsewhere_root.display().to_string(),
        elsewhere.display().to_string(),
    );

    assert_eq!(startup::last_project(&conf, &root), Some(here.clone()));
    assert_eq!(
        startup::last_project(&conf, &elsewhere_root),
        Some(elsewhere)
    );
    assert_eq!(
        startup::last_project(&conf, Path::new("/never/remembered")),
        None,
        "a root nobody has opened anything in has no last project"
    );

    let opened = startup::decide(&root, &conf, &[]);
    assert_eq!(opened, Opened::Remembered { path: here });

    fs::remove_dir_all(&root).unwrap();
    fs::remove_dir_all(&elsewhere_root).unwrap();
}

#[test]
fn f0_6_s2_a_remembered_project_no_longer_on_disk_falls_through() {
    // S2's "if still on disk": gone means fall through to the working copy, and fall through
    // WITHOUT forgetting. An external drive that is not mounted this morning looks exactly like a
    // deletion, and dropping the name would make that absence permanent — so `decide` cannot write.
    let root = temp_dir("remembered-gone");
    fs::create_dir_all(&root).unwrap();
    let gone = root.join("jan.naivepost");
    let session = startup::session_dir(&root);
    fs::create_dir_all(&session).unwrap();

    let conf = remembering(&root, &gone);
    let opened = startup::decide(&root, &conf, &[]);
    assert_eq!(opened, Opened::Session { path: session });
    assert_eq!(
        conf.projects.get(&root.display().to_string()),
        Some(&gone.display().to_string()),
        "a project that is merely unmounted must stay remembered"
    );

    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn f0_6_s2_a_root_remembered_without_a_file_is_no_project() {
    // `parse` keeps a lone `PROJECT_<n>_ROOT` as a folder whose last file is unknown rather than
    // throwing the entry away, so S2 has to read an empty value as "nothing to open".
    let root = temp_dir("remembered-no-file");
    fs::create_dir_all(&root).unwrap();

    let conf = remembering(&root, Path::new(""));
    assert_eq!(startup::last_project(&conf, &root), None);
    assert!(matches!(
        startup::decide(&root, &conf, &[]),
        Opened::Blank { .. }
    ));

    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn f0_6_s3_the_session_folder_beside_the_root_opens() {
    // S3: nothing remembered for this root, but the working copy is there — that is this session.
    let root = temp_dir("session");
    fs::create_dir_all(&root).unwrap();
    let session = project_folder(&startup::session_dir(&root));

    let opened = startup::decide(&root, &Conf::default(), &[]);
    assert_eq!(opened, Opened::Session { path: session });
    assert_eq!(startup::session_dir(&root), root.join("session.naivepost"));

    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn f0_6_s4_a_session_that_is_not_there_is_a_blank_session() {
    // S4: a first launch. The blank session is the same path S3 looked for and found missing, and
    // opening it writes nothing — the folder appears when the session is first saved, so a launch
    // that is abandoned leaves no project behind in whatever folder it happened to start from.
    let root = temp_dir("blank");
    fs::create_dir_all(&root).unwrap();

    let opened = startup::decide(&root, &Conf::default(), &[]);
    assert_eq!(
        opened,
        Opened::Blank {
            path: startup::session_dir(&root)
        }
    );
    let session = startup::session_dir(&root);
    assert!(!session.exists(), "a blank session is a decision, not a file");
    assert_eq!(
        fs::read_dir(&root).unwrap().count(),
        0,
        "deciding to open a blank session must not create anything"
    );

    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn f0_6_s2_the_remembered_project_comes_from_the_settings_file() {
    // S2 read the way it happens at startup: `llm.conf`'s `PROJECT_<n>_ROOT`/`_FILE` pairs (§7) in,
    // and the remembered folder out — while a different root falls through to a blank session.
    let root = temp_dir("conf-root");
    let cfg = temp_dir("conf-config");
    fs::create_dir_all(&root).unwrap();
    let remembered = project_folder(&root.join("jan.naivepost"));

    let paths = settings::paths_from(
        Some(&cfg.display().to_string()),
        None,
        None,
        Some("/xdg/data"),
    )
    .unwrap();
    settings::save(&paths, &remembering(&root, &remembered)).unwrap();
    let conf = settings::read(&paths).unwrap();

    assert_eq!(
        startup::decide(&root, &conf, &[]),
        Opened::Remembered {
            path: remembered
        }
    );

    let other = temp_dir("conf-other-root");
    fs::create_dir_all(&other).unwrap();
    assert!(matches!(
        startup::decide(&other, &conf, &[]),
        Opened::Blank { .. }
    ));

    fs::remove_dir_all(&root).unwrap();
    fs::remove_dir_all(&cfg).unwrap();
    fs::remove_dir_all(&other).unwrap();
}
