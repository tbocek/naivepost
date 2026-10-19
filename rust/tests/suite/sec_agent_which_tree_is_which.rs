//! §AGENT#which-tree-is-which — the checkout's three trees, answered by the program.
//!
//! `spec/AGENT.md` says: "`rust/` is the application being built, written against `spec/`. `gui/` is
//! the Go prototype `spec/` was written from: read it for observed behaviour; new work goes in
//! `rust/`." This checks the three claims the way the app exposes them: [`naivepost::tree_roles`] holds
//! the table, and `naivepost --trees` / `--help` print it, so what a test asserts is what the binary
//! answers — not a copy of the spec sentence in a comment.
//!
//! Nothing here needs a display, a server or a clock: two cases run the built binary as a subprocess
//! (both doors are answered before GTK opens), and the citation sweep reads files that ship with the
//! crate.

use std::path::{Path, PathBuf};
use std::process::Command;

use naivepost::tree_roles::{self, Role};

/// The repo root beside the crate, when the checkout has the prototype beside it. A tree installed
/// without `gui/` skips the citation sweep rather than inventing it — same guard
/// `tests/suite/web_tools_f6_1.rs` uses for `../spec`.
fn prototype_root() -> Option<PathBuf> {
    // Absolute, because `Path::join` throws away a relative base when the piece it joins is absolute
    // (a citation spelled `/workspaces/.../gui/cut.go`), and the sweep must not then be checking paths
    // against the crate's own directory.
    let gui = Path::new(env!("CARGO_MANIFEST_DIR")).join("../gui");
    let absolute = std::path::absolute(&gui).unwrap_or(gui);
    absolute.is_dir().then_some(absolute)
}

/// S1: the three trees are named, in the spec's own order, each saying what it is for — and only the
/// app's tree takes new work.
#[test]
fn sec_agent_which_tree_is_which_s1_the_three_trees_are_named() {
    let dirs: Vec<&str> = Role::all().iter().map(|role| role.dir()).collect();
    assert_eq!(
        dirs,
        vec!["rust", "gui", "spec"],
        "the spec names the trees in this order, and --trees prints them in it"
    );

    for role in Role::all() {
        let line = role.sentence();
        assert!(
            line.starts_with(&format!("{}/", role.dir())),
            "{role:?}'s sentence starts with its own folder: {line}"
        );
    }
    assert!(
        Role::App.sentence().contains("application being built"),
        "the app's tree says what it is: {}",
        Role::App.sentence()
    );
    assert!(
        Role::Prototype.sentence().contains("Go prototype")
            && Role::Prototype.sentence().contains("read it for observed behaviour"),
        "the prototype is evidence to read, which is what its sentence must say: {}",
        Role::Prototype.sentence()
    );
    assert!(
        Role::Spec.sentence().contains("read-only"),
        "the spec is the source of truth and read-only to the code: {}",
        Role::Spec.sentence()
    );

    // "new work goes in `rust/`" — the one rule the other two sentences exist to state.
    assert!(Role::App.takes_new_work(), "new work goes in rust/");
    assert!(
        !Role::Prototype.takes_new_work() && !Role::Spec.takes_new_work(),
        "neither the prototype nor the spec takes new work"
    );
}

/// S2: a path answers which tree it belongs to, whichever way it is spelled — relative, absolute, or
/// a doc-style `../gui/…` — and the answer decides whether new work may go there.
#[test]
fn sec_agent_which_tree_is_which_s2_a_path_says_which_tree_it_is() {
    let app_paths = [
        Role::App.citation("src/ui/window.rs"),
        format!("/workspaces/naivepost/{}/Cargo.toml", Role::App.dir()),
    ];
    let prototype_paths = [
        format!("../{}/cut.go", Role::Prototype.dir()),
        Role::Prototype.citation("subtitles.go"),
    ];
    let spec_paths = [
        Role::Spec.citation("05-cut.md"),
        format!("/workspaces/naivepost/{}/AGENT.md", Role::Spec.dir()),
    ];

    for spelled in &app_paths {
        assert_eq!(
            tree_roles::role_of(Path::new(spelled.as_str())),
            Some(Role::App),
            "{spelled} is the application's own tree"
        );
    }
    for spelled in &prototype_paths {
        assert_eq!(
            tree_roles::role_of(Path::new(spelled.as_str())),
            Some(Role::Prototype),
            "{spelled} is the prototype's tree"
        );
    }
    for spelled in &spec_paths {
        assert_eq!(
            tree_roles::role_of(Path::new(spelled.as_str())),
            Some(Role::Spec),
            "{spelled} is the spec's tree"
        );
    }

    // Neither a fixture nor scratch space belongs to any of the three, so neither can be mistaken for
    // one: the answer is "no tree", not a guess.
    for spelled in ["fixtures/demo.naivepost/naivepost.json", "/tmp/scratch/out.wav"] {        assert_eq!(
            tree_roles::role_of(Path::new(spelled)),
            None,
            "{spelled} is in none of the three trees"
        );
    }

    // The rule the table exists for, applied to both sides of the boundary.
    assert!(
        prototype_paths
            .iter()
            .all(|spelled| !tree_roles::role_of(Path::new(spelled.as_str())).unwrap().takes_new_work()),
        "a file under the prototype tree is never a file new work writes"
    );
    assert!(
        app_paths
            .iter()
            .all(|spelled| tree_roles::role_of(Path::new(spelled.as_str())).unwrap().takes_new_work()),
        "a file under rust/ is where new work goes"
    );
}

/// S3: the wire. `naivepost --trees` prints the three sentences, and only the app's tree is offered as
/// a place new work goes. Run through the real binary, headless: both CLI doors are answered before
/// GTK is asked for anything.
#[test]
fn sec_agent_which_tree_is_which_s3_the_binary_prints_which_tree_is_which() {
    let output = Command::new(env!("CARGO_BIN_EXE_naivepost"))
        .arg("--trees")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("the built binary runs");
    assert!(
        output.status.success(),
        "--trees exits 0: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let printed = String::from_utf8_lossy(&output.stdout).into_owned();
    for role in Role::all() {
        assert!(
            printed.contains(role.sentence()),
            "`--trees` prints {role:?}'s sentence verbatim, got:\n{printed}"
        );
    }
    // Exactly one line offers a home for new work, and it is the app's.
    let homes: Vec<&str> = printed
        .lines()
        .filter(|line| line.starts_with("rust/") || line.starts_with("gui/") || line.starts_with("spec/"))
        .filter(|line| !line.contains("read it for observed behaviour") && !line.contains("read-only"))
        .collect();
    assert_eq!(
        homes,
        vec![Role::App.sentence()],
        "only rust/ is named as the tree new work goes in, got:\n{printed}"
    );
}

/// S4: the same answer where a person actually looks first — `--help` carries the three trees.
#[test]
fn sec_agent_which_tree_is_which_s4_the_help_names_the_trees() {
    let output = Command::new(env!("CARGO_BIN_EXE_naivepost"))
        .arg("--help")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("the built binary runs");
    assert!(output.status.success(), "--help exits 0");
    let printed = String::from_utf8_lossy(&output.stdout).into_owned();
    assert!(
        printed.contains("Trees:"),
        "the help has a Trees block, got:\n{printed}"
    );
    for role in Role::all() {
        assert!(
            printed.contains(role.sentence()),
            "the help prints {role:?}'s sentence verbatim, got:\n{printed}"
        );
    }
    assert!(
        printed.contains("--trees"),
        "the help names the door that prints the answer on its own"
    );
    assert!(
        printed.contains("With paths, says which tree each one belongs to"),
        "the help also names the per-path form, which is the question people actually have"
    );
}

/// S6: the per-path question, through the binary — "can new work go here?", answered by the same rule
/// the module states and printed by `naivepost --trees <path>…`.
#[test]
fn sec_agent_which_tree_is_which_s6_the_binary_answers_for_a_path() {
    let asked = ["gui/cut.go", "rust/src/main.rs", "spec/AGENT.md", "/tmp/scratch/out.wav"];
    let mut command = Command::new(env!("CARGO_BIN_EXE_naivepost"));
    command.arg("--trees").args(asked);
    let output = command
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("the built binary runs");
    assert!(
        output.status.success(),
        "--trees with paths exits 0: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let printed = String::from_utf8_lossy(&output.stdout).into_owned();
    let line_for = |path: &str| {
        printed
            .lines()
            .find(|line| line.starts_with(&format!("{path}: ")))
            .unwrap_or_else(|| panic!("`--trees` answered for {path}, got:\n{printed}"))
            .to_string()
    };

    let prototype = line_for("gui/cut.go");
    assert!(
        prototype.contains(Role::Prototype.sentence()) && prototype.contains("new work here: false"),
        "a prototype file is read, never written into: {prototype}"
    );
    let app = line_for("rust/src/main.rs");
    assert!(
        app.contains(Role::App.sentence()) && app.contains("new work here: true"),
        "a file under rust/ is where new work goes: {app}"
    );
    let spec = line_for("spec/AGENT.md");
    assert!(
        spec.contains("read-only") && spec.contains("new work here: false"),
        "the spec's tree takes no new work: {spec}"
    );
    assert!(
        line_for("/tmp/scratch/out.wav").contains("none of the three trees"),
        "scratch space is not mistaken for one of the trees: {}",
        line_for("/tmp/scratch/out.wav")
    );
}

/// S5: every prototype citation in the app's own comments points at a file that exists.
///
/// A citation like `gui/retake_edge.go` is how this rewrite records observed behaviour, so one naming a
/// file the prototype does not have is a pointer to nothing — the kind of claim a reader cannot check.
/// The sweep is the module's own door ([`tree_roles::dead_prototype_citations_for_test`]), run over every
/// shipped source file.
#[test]
fn sec_agent_which_tree_is_which_s5_every_prototype_citation_points_at_a_file() {
    let Some(gui) = prototype_root() else { return };
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut dead: Vec<String> = Vec::new();
    let mut checked = 0usize;
    for entry in std::fs::read_dir(&src).expect("src is there") {
        let path = entry.expect("one directory entry").path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("rs") {
            continue;
        }
        let text = std::fs::read_to_string(&path).expect("a source file reads");
        checked += 1;
        dead.extend(
            tree_roles::dead_prototype_citations_for_test(&text, &gui)
                .into_iter()
                .map(|name| format!("{}: {name}", path.file_name().unwrap().to_string_lossy())),
        );
    }
    assert!(checked > 100, "the sweep walked {checked} source files");
    assert!(
        dead.is_empty(),
        "these prototype citations name no file in gui/: {:?}",
        dead
    );
}
