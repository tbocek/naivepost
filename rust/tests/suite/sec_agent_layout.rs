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

/// The test-file count, computed the way the module does it but independently, so a change to either
/// shows up as a mismatch rather than as agreement between two copies of one idea.
fn listed_in_suite(text: &str) -> usize {
    text.lines()
        .filter(|line| line.starts_with("mod ") && line.ends_with(';'))
        .count()
}

/// `.rs` files directly in `rust/tests/suite`, `mod.rs` aside.
fn suite_files(root: &Path) -> usize {
    std::fs::read_dir(root.join("rust/tests/suite"))
        .expect("tests/suite reads")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".rs") && name != "mod.rs")
        .count()
}

/// `.rs` names that some file in `tests/suite` pulls in with `#[path = "<name>.rs"]`.
fn path_included_names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .expect("tests/suite reads")
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            name.strip_suffix(".rs").map(|stem| (stem.to_string(), entry.path()))
        })
        .flat_map(|(_stem, path)| {
            let text = std::fs::read_to_string(&path).unwrap_or_default();
            let mut found: Vec<String> = Vec::new();
            let mut rest = text.as_str();
            while let Some(at) = rest.find("#[path = \"") {
                rest = &rest[at + "#[path = \"".len()..];
                match rest.find('"') {
                    Some(end) => found.push(rest[..end].trim_end_matches(".rs").to_string()),
                    None => break,
                }
            }
            found
        })
        .collect();
    names.sort();
    names.dedup();
    names
}

/// Suite files neither listed by `tests/suite/mod.rs` nor pulled in by a `#[path]` attribute: the spec's
/// rule is that each test file needs its `mod` line, so anything left here is code nothing builds.
fn unaccounted_suite_files(root: &Path) -> Vec<String> {
    let dir = root.join("rust/tests/suite");
    let mod_rs = std::fs::read_to_string(dir.join("mod.rs")).expect("suite/mod.rs reads");
    let included = path_included_names(&dir);
    suite_stems(&dir)
        .into_iter()
        .filter(|stem| !mod_rs.contains(&format!("mod {stem};")) && !included.contains(stem))
        .collect()
}

/// Run a `just` recipe from `rust/`, returning its exit status and combined output.
fn run_just(args: &[&str]) -> (bool, String) {
    let output = Command::new("just")
        .args(args)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("the project's `just` runner is on PATH");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    (output.status.success(), text)
}

/// `.rs` stems directly in a folder, sorted.
fn suite_stems(dir: &Path) -> Vec<String> {
    let mut stems: Vec<String> = std::fs::read_dir(dir)
        .expect("folder reads")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".rs"))
        .map(|name| name.trim_end_matches(".rs").to_string())
        .filter(|stem| stem != "mod")
        .collect();
    stems.sort();
    stems
}

/// S4: one module per concern in `rust/src`, and `rust/tests` one test binary whose test files are
/// modules of `tests/suite`, beside the shared modules those files pull in by a path attribute.
#[test]
fn sec_agent_layout_s4_one_module_per_concern_tests_per_flow() {
    let root = root();
    let lines = layout_manifest::organisation(&root);
    assert_eq!(
        lines.len(),
        3,
        "one line per tree the spec describes, plus the test-tree rules it states: {lines:?}"
    );

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

    // rust/tests: test files vs shared modules. `_*` fixtures, `*_harness` socket harnesses, `*_shared`
    // files and `joined_answer` are pulled in by other tests with #[path], so tests/suite/mod.rs does not
    // list them.
    let listed = listed_in_suite(
        &std::fs::read_to_string(root.join("rust/tests/suite/mod.rs")).expect("suite/mod.rs reads"),
    );
    assert!(
        lines[1].contains(&format!("one test binary, tests/all.rs, holding {listed} test files")),
        "the test-file count printed matches tests/suite/mod.rs: {}\n{lines:?}",
        lines[1]
    );
    // The rule the spec states, not a count: every file under tests/suite is either a `mod` line of the
    // one binary or a shared module some test file includes by #[path]. How many of each drifts as tests
    // are added; an orphan never belongs.
    let orphans = unaccounted_suite_files(&root);
    assert!(
        orphans.is_empty(),
        "these tests/suite files are neither listed in tests/suite/mod.rs nor pulled in by a #[path] \
         attribute, so nothing builds them: {orphans:?}"
    );
    assert!(
        lines[1].contains(&format!("{} shared modules", shared_of(&root))),
        "the shared-module count printed matches what is there: {}",
        lines[1]
    );

    // And the naming the spec describes: flow, step, case — f1_13_s5_…
    let flow_named = std::fs::read_dir(root.join("rust/tests/suite"))
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
    let mod_rs =
        std::fs::read_to_string(root.join("rust/tests/suite/mod.rs")).expect("suite/mod.rs reads");
    suite_files(root) - listed_in_suite(&mod_rs)
}

/// S7: the spec's own words — "`rust/tests` integration tests in ONE test binary, `tests/all.rs`: each
/// test file is a module in `tests/suite/` (a new file needs its `mod <name>;` line in
/// `tests/suite/mod.rs`, never a `[[test]]` in Cargo.toml)". Checked from disk, and then through the
/// door that prints it: the program states the same rule the test checks.
#[test]
fn sec_agent_layout_s7_the_one_test_binary_and_no_test_targets() {
    let root = root();
    let all_rs =
        std::fs::read_to_string(root.join("rust/tests/all.rs")).expect("the one test binary reads");
    assert!(all_rs.contains("mod suite;"), "tests/all.rs declares the suite");
    assert!(all_rs.contains("mod common;"), "and the shared helper module");

    let cargo = std::fs::read_to_string(root.join("rust/Cargo.toml")).expect("Cargo.toml reads");
    assert!(
        !cargo.contains("[[test]]"),
        "no test file gets a [[test]] target of its own; it joins tests/suite instead"
    );

    // The other half of the sentence: a test file without its `mod` line is a file nothing builds.
    let mod_rs =
        std::fs::read_to_string(root.join("rust/tests/suite/mod.rs")).expect("suite/mod.rs reads");
    let listed_names: Vec<String> = mod_rs
        .lines()
        .filter_map(|line| line.strip_prefix("mod ")?.strip_suffix(';').map(str::to_string))
        .collect();
    for name in &listed_names {
        assert!(
            root.join(format!("rust/tests/suite/{name}.rs")).is_file(),
            "tests/suite/mod.rs lists {name}, so the file has to exist"
        );
    }
    assert!(
        listed_names.iter().any(|name| name == "sec_agent_layout"),
        "this very file is listed, as every test file must be"
    );

    let rules = layout_manifest::organisation(&root)
        .into_iter()
        .nth(2)
        .expect("the third organisation line carries the test-tree rules");
    for claim in [
        "tests/all.rs is the one test binary",
        "no [[test]] target",
        "test files listed in tests/suite/mod.rs",
        "all of them included",
    ] {
        assert!(rules.contains(claim), "the answer says {claim}:\n{rules}");
    }
    for wrong in ["nothing builds", "forbids", "is missing"] {
        assert!(!rules.contains(wrong), "nothing here is broken ({wrong}):\n{rules}");
    }

    // The wire: `naivepost --layout` prints that line, so the check runs from the binary too.
    let printed = run(&["--layout"], &root);
    assert!(
        printed.contains(&rules),
        "`--layout` prints the test-tree rules; missing:\n{rules}\ngot:\n{printed}"
    );
}

/// S8: "`just test` runs each file in a process of its own" — what `just _files` relies on when it calls
/// the binary once per suite file with a `suite::<file>::` filter. A child run of THIS binary filtered to
/// one pure-logic file may only ever report that file's tests.
#[test]
fn sec_agent_layout_s8_each_test_file_runs_in_a_process_of_its_own() {
    let output = std::process::Command::new(
        std::env::current_exe().expect("the test binary knows where it is"),
    )
    .arg("suite::align_bare_share::")
    .output()
    .expect("the test binary re-runs itself");
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    assert!(
        output.status.success(),
        "one file's tests, filtered, exit 0:\n{stdout}"
    );
    assert!(
        stdout.contains("test result: ok. 5 passed"),
        "the filter ran exactly align_bare_share's five tests:\n{stdout}"
    );
    for line in stdout.lines().filter(|line| line.starts_with("test suite::")) {
        let path = line.split_whitespace().nth(1).unwrap_or_default();
        // The name is `suite::<file>::<test>`; the file is the second segment, and a test name itself
        // never contains `::`.
        let segments: Vec<&str> = path.split("::").collect();
        assert_eq!(
            segments.get(1).copied(),
            Some("align_bare_share"),
            "a per-file run reports no other file's test: {line}"
        );
    }
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

/// The scratch root S6 and the two tests below each make their own of: an empty folder stands for a tree
/// with no justfile and no test list, which is how the negative branches get checked without touching this
/// checkout.
fn scratch_root(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "naivepost-layout-{}-{}",
        tag,
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch root");
    dir
}

/// S9: the spec's words — "`just test` runs each file in a process of its own". Checked by running the real
/// recipe over a filter that matches two files, so what is asserted is the loop the recipe actually has,
/// not a copy of its text.
#[test]
fn sec_agent_layout_s9_just_test_runs_each_file_in_its_own_process() {
    let root = root();
    let mod_rs =
        std::fs::read_to_string(root.join("rust/tests/suite/mod.rs")).expect("suite/mod.rs reads");
    let listed: Vec<String> = mod_rs
        .lines()
        .filter_map(|line| line.strip_prefix("mod ")?.strip_suffix(';').map(str::to_string))
        .collect();
    let expected: Vec<&String> = listed.iter().filter(|name| name.contains("align_bare")).collect();
    assert!(
        expected.len() >= 2,
        "the filter picks at least align_bare_share and align_bare_warn, got {expected:?}"
    );

    let justfile = std::fs::read_to_string(root.join("rust/justfile")).expect("justfile reads");
    for claim in ["for file in $files", "\"suite::$file::\""] {
        assert!(
            justfile.contains(claim),
            "the recipe holds the per-file loop ({claim}):\n{justfile}"
        );
    }

    // The recipe itself, filtered to those files: one header per file, nothing else's, each block green.
    let (ok, out) = run_just(&["test", "align_bare"]);
    assert!(ok, "`just test align_bare` exits 0:\n{out}");
    let headers: Vec<&str> = out
        .lines()
        .filter_map(|line| line.trim().strip_prefix("Running tests/suite/").map(|rest| rest.trim_end_matches(".rs")))
        .collect();
    for name in &expected {
        assert_eq!(
            headers.iter().filter(|h| **h == **name).count(),
            1,
            "{name} gets exactly one per-file run: {headers:?}\n{out}"
        );
    }
    for header in &headers {
        assert!(
            expected.iter().any(|name| name.as_str() == *header),
            "the filter ran {header}, which its name does not match: {headers:?}"
        );
    }
    assert_eq!(
        out.matches("test result: ok.").count(),
        expected.len(),
        "each per-file process reports its own green result: {headers:?}\n{out}"
    );

    // A filter matching nothing says so instead of passing quietly.
    let (none_ok, none_out) = run_just(&["test", "zzz_no_such_test_file"]);
    assert!(
        !none_ok && none_out.contains("no test file in tests/suite/mod.rs matches"),
        "an unmatched filter fails with the recipe's own sentence: exit {} \n{none_out}",
        if none_ok { "ok" } else { "non-zero" }
    );

    // The program answers the same rule, and prints it through the door.
    let rules = layout_manifest::test_runner_rules(&root);
    assert!(
        rules.contains(&format!("each of the {} test files", listed.len())),
        "the answer counts the files tests/suite/mod.rs lists ({}):\n{rules}",
        listed.len()
    );
    assert!(
        rules.contains("xvfb-run") && rules.contains("GSK_RENDERER=cairo"),
        "the answer names the wrapper the recipe uses:\n{rules}"
    );
    let printed = run(&["--layout"], &root);
    assert!(
        printed.contains(&rules),
        "`--layout` prints the test-runner rule; missing:\n{rules}\ngot:\n{printed}"
    );

    let nowhere = scratch_root("runner");
    assert!(
        layout_manifest::test_runner_rules(&nowhere).contains("does not run each test file"),
        "a root with no justfile says the rule does not hold there"
    );
    let _ = std::fs::remove_dir_all(&nowhere);
}

/// S10: the spec's words — "the fixture at `rust/fixtures/demo.naivepost` is what snapshots render from".
/// The claim is read off the snapshot recipe's own `--project`, so a recipe pointing elsewhere cannot pass.
#[test]
fn sec_agent_layout_s10_the_snapshot_recipe_renders_from_the_fixture() {
    let root = root();
    let justfile = std::fs::read_to_string(root.join("rust/justfile")).expect("justfile reads");
    let token = justfile
        .split_once("--project")
        .and_then(|(_, rest)| rest.split_whitespace().next())
        .expect("the snapshot recipe passes --project")
        .to_string();
    let spelled = format!("rust/{token}");
    let fixture = place("rust/fixtures/demo.naivepost");
    let where_it_is =
        layout_manifest::found(&fixture, &root).expect("the fixture place resolves, as S3 checks");
    assert_eq!(
        spelled, where_it_is,
        "the recipe renders from the fixture the layout names, not somewhere else"
    );
    assert!(
        root.join(&spelled).join("naivepost.json").is_file(),
        "{} holds the project file snapshots read",
        spelled
    );

    let line = layout_manifest::snapshot_source(&root);
    assert!(
        line.contains("rust/fixtures/demo.naivepost") && line.contains("naivepost.json is there"),
        "the answer names the fixture and says its project file is present:\n{line}"
    );
    let printed = run(&["--layout"], &root);
    assert!(
        printed.contains(&line),
        "`--layout` prints the snapshot source; missing:\n{line}\ngot:\n{printed}"
    );

    let nowhere = scratch_root("snapshot-source");
    assert_eq!(
        layout_manifest::snapshot_source(&nowhere),
        "rust/justfile: no snapshot recipe --project to read",
        "a root with no justfile has no recipe to read"
    );
    let _ = std::fs::remove_dir_all(&nowhere);
}
