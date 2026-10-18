//! §AGENT#layout — where this checkout's pieces live, read off the filesystem.
//!
//! `spec/AGENT.md`'s Layout section names ten places and what lives in each: `spec/` with its numbered
//! chapters, `spec/prompts/` (the shipped prompt wording), `spec/inventory/` (per-screen widget lists),
//! `spec/12-decisions.md`, `rust/src` (app code, one module per concern), `rust/tests` (integration
//! tests, one file per spec flow, named `f1_13_s5_…` = flow, step, case), `rust/fixtures/demo.naivepost`
//! (what snapshots render from), and `Readme.md` beside `assets/` and `docs/`. [`naivepost::layout_manifest`]
//! answers all of it by looking, and `naivepost --layout` prints that answer, so these tests assert what
//! the binary says about the tree rather than repeating the prose.
//!
//! Nothing here needs a display or a server; the cases that go through the real binary use doors answered
//! before GTK opens.

use std::path::{Path, PathBuf};
use std::process::Command;

use naivepost::layout_manifest::{self, Kind};

/// The checkout root: `rust/` is the crate, one level below it is the repo.
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

/// Run the built binary from `dir`, returning stdout. Both doors used here print and exit before any
/// display is asked for.
fn run(args: &[&str], dir: &Path) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_naivepost"))
        .args(args)
        .current_dir(dir)
        .output()
        .expect("the built binary runs");
    assert!(
        output.status.success(),
        "`naivepost {}` exited {}: {}",
        args.join(" "),
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// The place list entry for a spec path.
fn place(path: &str) -> layout_manifest::Place {
    layout_manifest::places()
        .into_iter()
        .find(|entry| entry.path == path)
        .unwrap_or_else(|| panic!("{} is a place the layout names", path))
}

/// `.md` names inside a folder, sorted.
fn markdown(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .unwrap_or_else(|why| panic!("{} should be readable: {why}", dir.display()))
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".md"))
        .collect();
    names.sort();
    names
}

/// S1: every place the spec names is present in this checkout, as a folder or a file of the right kind.
#[test]
fn sec_agent_layout_s1_every_place_the_spec_names_is_found() {
    let root = root();
    let paths: Vec<&str> = layout_manifest::places()
        .iter()
        .map(|entry| entry.path)
        .collect();
    for want in [
        "spec",
        "spec/prompts",
        "spec/inventory",
        "spec/12-decisions.md",
        "rust/src",
        "rust/tests",
        "rust/fixtures/demo.naivepost",
        "Readme.md",
        "assets",
        "docs",
    ] {
        assert!(paths.contains(&want), "{want} is a place AGENT.md names, got {paths:?}");
    }
    for entry in layout_manifest::places() {
        assert!(
            !entry.purpose.trim().is_empty(),
            "{} says what lives there",
            entry.path
        );
    }

    assert!(
        layout_manifest::missing(&root).is_empty(),
        "this checkout has every place the spec names, missing: {:?}",
        layout_manifest::missing(&root)
    );
    for path in [
        "spec",
        "spec/prompts",
        "spec/inventory",
        "rust/src",
        "rust/tests",
        "rust/fixtures/demo.naivepost",
        "assets",
        "docs",
    ] {
        let entry = place(path);
        assert_eq!(entry.kind, Kind::Dir, "{path} is a folder");
        assert_eq!(
            layout_manifest::found(&entry, &root).as_deref(),
            Some(path),
            "{path} is found where the spec spells it"
        );
    }
    let decisions = place("spec/12-decisions.md");
    assert_eq!(decisions.kind, Kind::File, "a chapter is a file, not a folder");
    assert_eq!(
        layout_manifest::found(&decisions, &root).as_deref(),
        Some("spec/12-decisions.md"),
        "the decisions chapter is where the spec spells it"
    );
}

/// S2: the two folders the spec calls normative hold what they are for — prompt wording and widget
/// lists, one file per screen — counted off disk rather than from a list typed into a test.
#[test]
fn sec_agent_layout_s2_the_prompts_and_inventory_are_the_shipped_wording() {
    let root = root();
    let prompts = markdown(&root.join("spec/prompts"));
    assert!(
        prompts.len() >= 12,
        "spec/prompts ships the twelve job prompts plus the system and tools wording, got {}",
        prompts.len()
    );
    assert!(prompts.iter().any(|n| n == "system.md"), "the system prompt ships there");

    let inventory = markdown(&root.join("spec/inventory"));
    assert!(
        inventory.len() >= 5,
        "spec/inventory holds a widget list per screen, got {inventory:?}"
    );
    for screen in ["cut.md", "narrate.md"] {
        assert!(
            inventory.iter().any(|name| name == screen),
            "{screen} is one of the per-screen widget lists, got {inventory:?}"
        );
    }

    let chapters: Vec<String> = markdown(&root.join("spec"))
        .into_iter()
        .filter(|name| name.starts_with(|c: char| c.is_ascii_digit()))
        .collect();
    assert_eq!(chapters.len(), 13, "00-principles.md \u{2026} 12-decisions.md: {chapters:?}");
    assert_eq!(
        chapters.first().map(String::as_str),
        Some("00-principles.md"),
        "the first chapter is the principles one"
    );
    assert_eq!(
        chapters.last().map(String::as_str),
        Some("12-decisions.md"),
        "the last chapter is the decisions one"
    );
}

/// S3: the behaviour document is named where it actually lives, and the snapshot fixture is the project
/// the render recipe points at.
#[test]
fn sec_agent_layout_s3_the_behaviour_doc_is_named_where_it_lives() {
    let root = root();
    let readme = place("Readme.md");
    // This checkout keeps the behaviour document under spec/ and has no root Readme.md, so the honest
    // answer is the alternative — which is what makes the printed line true instead of wrong.
    let where_it_is = layout_manifest::found(&readme, &root).expect("the behaviour doc is somewhere");
    assert_eq!(
        where_it_is, "spec/Readme.md",
        "AGENT.md's Readme.md resolves to the copy this checkout keeps"
    );
    let text = std::fs::read_to_string(root.join(&where_it_is)).expect("it reads");
    assert!(
        text.lines().count() > 100,
        "it is the behaviour document, not a stub: {} lines",
        text.lines().count()
    );

    let fixture = place("rust/fixtures/demo.naivepost");
    let project = root.join(fixture.path).join("naivepost.json");
    assert!(project.is_file(), "{:?} is the project snapshots render from", project);
    let value: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&project).expect("the project file reads"))
            .expect("and it parses as JSON");
    assert!(value.is_object(), "a project file is an object of settings");
}

/// The registered-binary count, computed the way the module does it but independently, so a change to
/// either shows up as a mismatch rather than as agreement between two copies of one idea.
fn registered_from_toml(text: &str) -> usize {
    text.lines()
        .filter(|line| line.contains(r#"path = "tests/"#) && line.contains(".rs\""))
        .count()
}

/// S4: one module per concern in `rust/src`, and `rust/tests` split into registered binaries and the
/// shared modules Cargo was deliberately not told about.
#[test]
fn sec_agent_layout_s4_one_module_per_concern_tests_per_flow() {
    let root = root();
    let lines = layout_manifest::organisation(&root);
    assert_eq!(lines.len(), 2, "one line per tree the spec describes: {lines:?}");

    // rust/src: every .rs directly in it is declared by lib.rs — that is what "one module per concern"
    // has to mean, since an undeclared file is code nothing builds.
    let src = root.join("rust/src");
    let declared = std::fs::read_to_string(src.join("lib.rs")).expect("lib.rs reads");
    let orphans: Vec<String> = std::fs::read_dir(&src)
        .expect("src reads")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".rs"))
        .filter_map(|name| name.strip_suffix(".rs").map(str::to_string))
        .filter(|name| name != "lib" && name != "main")
        .filter(|name| !declared.contains(&format!("pub mod {name};")))
        .collect();
    assert!(orphans.is_empty(), "these modules exist but nothing declares them: {orphans:?}");
    assert!(
        lines[0].contains(&format!("{} modules", orphans_free_count(&src))),
        "the module count printed matches what is on disk: {}\n{lines:?}",
        lines[0]
    );

    // rust/tests: registered binaries vs shared modules. `_shared` is this checkout's third spelling of
    // the same convention the head of Cargo.toml describes (`_*` fixtures and `*_harness` socket
    // harnesses being the other two): a file another test pulls in with #[path], not its own binary.
    let cargo = std::fs::read_to_string(root.join("rust/Cargo.toml")).expect("Cargo.toml reads");
    let registered = registered_from_toml(&cargo);
    let files = std::fs::read_dir(root.join("rust/tests"))
        .expect("tests reads")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".rs"))
        .count();
    assert!(
        lines[1].contains(&format!("{registered} integration test binaries")),
        "the registered count printed matches Cargo.toml: {}\n{lines:?}",
        lines[1]
    );
    assert_eq!(
        registered + 10,
        files,
        "ten shared modules sit outside Cargo's target list (fixtures, harnesses, _shared)"
    );
    assert!(
        lines[1].contains(&format!("{} shared modules", shared_of(&root))),
        "the shared-module count printed matches what is there: {}",
        lines[1]
    );

    // And the naming the spec describes: flow, step, case — f1_13_s5_…
    let flow_named = std::fs::read_dir(root.join("rust/tests"))
        .expect("tests reads")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| {
            let stem = name.trim_end_matches(".rs");
            let mut parts = stem.split('_');
            let Some(first) = parts.next() else { return false };
            let Some(second) = parts.next() else { return false };
            first.starts_with('f')
                && first[1..].chars().all(|c| c.is_ascii_digit())
                && second.chars().all(|c| c.is_ascii_digit())
                && !second.is_empty()
        })
        .count();
    assert!(
        flow_named >= 100,
        "flow-named test files (f<N>_<step>\u{2026}) are how tests map onto spec flows: {flow_named}"
    );
}

fn orphans_free_count(src: &Path) -> usize {
    std::fs::read_dir(src)
        .expect("src reads")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".rs"))
        .filter(|name| name != "lib.rs" && name != "main.rs")
        .count()
}

fn shared_of(root: &Path) -> usize {
    let cargo = std::fs::read_to_string(root.join("rust/Cargo.toml")).expect("Cargo.toml reads");
    let files = std::fs::read_dir(root.join("rust/tests"))
        .expect("tests reads")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".rs"))
        .count();
    files - registered_from_toml(&cargo)
}

/// S5: the wire — `naivepost --layout` prints exactly those lines, and `--help` names the door.
#[test]
fn sec_agent_layout_s5_the_binary_prints_the_layout() {
    let root = root();
    let printed = run(&["--layout"], &root);
    for line in layout_manifest::lines(&root) {
        assert!(
            printed.contains(&line),
            "`--layout` prints every line the module produces; missing:\n{line}\ngot:\n{printed}"
        );
    }
    for path in [
        "spec/prompts",
        "rust/src",
        "rust/tests",
        "rust/fixtures/demo.naivepost",
        "assets",
    ] {
        assert!(printed.contains(path), "the answer names {path}:\n{printed}");
    }
    assert!(
        printed.contains("every place the spec names is present"),
        "nothing the spec names is missing here:\n{printed}"
    );
    assert!(
        printed.contains("spec/Readme.md"),
        "the behaviour doc is named where it lives:\n{printed}"
    );

    let help = run(&["--help"], &root);
    assert!(help.contains("--layout"), "--help names the door that prints the layout");
}

/// S6: a root without the trees says so out loud — an absent place is named, never hidden.
#[test]
fn sec_agent_layout_s6_a_root_without_the_trees_says_so() {
    let nowhere = std::env::temp_dir().join(format!(
        "naivepost-layout-nowhere-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&nowhere);
    std::fs::create_dir_all(&nowhere).expect("scratch root");
    std::fs::write(nowhere.join("scratch.txt"), "not a checkout").expect("scratch written");

    let absent = layout_manifest::missing(&nowhere);
    assert_eq!(
        absent.len(),
        layout_manifest::places().len(),
        "an empty folder has none of the places: {absent:?}"
    );

    let printed = run(&["--layout", &nowhere.to_string_lossy()], &root());
    assert!(printed.contains("missing"), "the answer says something is missing:\n{printed}");
    assert!(
        printed.contains("rust/tests"),
        "and names the places that are absent:\n{printed}"
    );
    assert!(
        printed.contains("NOT PRESENT"),
        "each absent place is marked, not skipped:\n{printed}"
    );
    let _ = std::fs::remove_dir_all(&nowhere);
}
