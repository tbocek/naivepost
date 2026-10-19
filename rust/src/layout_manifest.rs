//! Where this checkout's pieces live — [`spec/AGENT.md`](../spec/AGENT.md#layout).
//!
//! The spec's Layout section names ten places and says what lives in each: `spec/` the rewrite spec
//! (`00-principles.md` … `12-decisions.md`, one file per screen or flow), `spec/prompts/` the shipped
//! prompt wording, `spec/inventory/` the per-screen widget lists, `spec/12-decisions.md` decisions
//! already taken, `rust/src` app code one module per concern, `rust/tests` integration tests one file
//! per spec flow, `rust/fixtures/demo.naivepost` what snapshots render from, and `Readme.md` beside
//! `assets/` and `docs/`.
//!
//! # Why the program answers this
//!
//! A layout written down drifts: a document moves, a test file is added, and the prose quietly becomes
//! false. This module reads the filesystem instead, so what `naivepost --layout` prints is what is
//! there — including the case where a place the spec names is absent, which is said out loud rather
//! than hidden. One door uses it, in `main`, answered before any display is asked for.

use std::path::Path;

/// What a named place is: a folder or a single file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Dir,
    File,
}

impl Kind {
    pub fn word(self) -> &'static str {
        match self {
            Self::Dir => "folder",
            Self::File => "file",
        }
    }
}

/// One place the spec's Layout section names, and what it holds.
pub struct Place {
    /// The path as the spec spells it, from the checkout root: `spec/prompts`, `rust/tests`.
    pub path: &'static str,
    /// What the spec says lives there, in its own words.
    pub purpose: &'static str,
    pub kind: Kind,
    /// Where else this place may actually be found, tried in order when `path` is absent. Only the
    /// behaviour document needs it — see the note on that entry below.
    pub alt: &'static [&'static str],
}

/// The places AGENT.md names, in the order it names them.
pub fn places() -> Vec<Place> {
    vec![
        Place {
            path: "spec",
            purpose: "the rewrite spec, one file per screen or flow",
            kind: Kind::Dir,
            alt: &[],
        },
        Place {
            path: "spec/prompts",
            purpose: "the shipped prompt wording",
            kind: Kind::Dir,
            alt: &[],
        },
        Place {
            path: "spec/inventory",
            purpose: "per-screen widget lists",
            kind: Kind::Dir,
            alt: &[],
        },
        Place {
            path: "spec/12-decisions.md",
            purpose: "decisions already taken \u{2014} do not re-litigate what is written there",
            kind: Kind::File,
            alt: &[],
        },
        Place {
            path: "rust/src",
            purpose: "app code, one module per concern",
            kind: Kind::Dir,
            alt: &[],
        },
        Place {
            path: "rust/tests",
            purpose: "integration tests, one file per spec flow, named flow_step_case",
            kind: Kind::Dir,
            alt: &[],
        },
        Place {
            path: "rust/fixtures/demo.naivepost",
            purpose: "the project snapshots render from",
            kind: Kind::Dir,
            alt: &[],
        },
        // DECISION: AGENT.md lists `Readme.md` ("the app's behaviour as it works today") beside
        // `assets/` and `docs/`, but this checkout keeps that document under `spec/` and has no root
        // one. Rather than print a path that does not exist, the entry carries the places it may really
        // be, so the answer says where it was found. Nothing here creates or edits a document.
        Place {
            path: "Readme.md",
            purpose: "the app's behaviour as it works today",
            kind: Kind::File,
            alt: &["spec/Readme.md", "rust/README.md"],
        },
        Place {
            path: "assets",
            purpose: "icons and the built-in cards",
            kind: Kind::Dir,
            alt: &[],
        },
        Place {
            path: "docs",
            purpose: "icons",
            kind: Kind::Dir,
            alt: &[],
        },
    ]
}

fn exists(path: &Path, kind: Kind) -> bool {
    match kind {
        Kind::Dir => path.is_dir(),
        Kind::File => path.is_file(),
    }
}

/// Where a place actually is: the spec's own path if it is there, otherwise the first of its
/// alternatives. `None` means this checkout does not have the place at all.
pub fn found(place: &Place, root: &Path) -> Option<String> {
    let spelled = root.join(place.path);
    if exists(&spelled, place.kind) {
        return Some(place.path.to_string());
    }
    place
        .alt
        .iter()
        .find(|alt| exists(&root.join(alt), place.kind))
        .map(|alt| alt.to_string())
}

/// The places the spec names that this checkout does not have, by the spec's own spelling.
pub fn missing(root: &Path) -> Vec<String> {
    places()
        .iter()
        .filter(|place| found(place, root).is_none())
        .map(|place| place.path.to_string())
        .collect()
}

/// Text of a file, empty when it cannot be read: a missing file makes a smaller count, never a failed
/// answer (§00 rule 8 — recording never breaks the work).
fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_default()
}

/// `.rs` names directly inside a folder, without their extension.
fn rust_names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(|entry| entry.ok())
                .filter_map(|entry| {
                    let name = entry.file_name().to_string_lossy().into_owned();
                    name.strip_suffix(".rs").map(str::to_string)
                })
                .filter(|name| name != "lib" && name != "main")
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

/// The `mod <name>;` lines of `tests/suite/mod.rs`: the test files of the one test binary,
/// `tests/all.rs`. Every other file in `tests/suite` is a shared module pulled in by a path attribute.
fn suite_tests(mod_rs: &str) -> Vec<String> {
    let mut names: Vec<String> = mod_rs
        .lines()
        .filter_map(|line| line.strip_prefix("mod ")?.strip_suffix(';').map(str::to_string))
        .collect();
    names.sort();
    names
}

/// `[[test]]` tables in a `Cargo.toml`. The spec forbids one — every test file is a module of the one
/// binary — so a target means a file was added without its `mod` line and now relinks as its own program.
fn test_targets(cargo_toml: &str) -> usize {
    cargo_toml
        .lines()
        .filter(|line| line.trim() == "[[test]]")
        .count()
}

/// `.rs` names that some file in `dir` pulls in with `#[path = "<name>.rs"]`, sorted and de-duplicated:
/// the shared harnesses and fixtures that build INSIDE the listed test files instead of beside them.
fn path_included(dir: &Path) -> Vec<String> {
    let mut names = std::collections::BTreeSet::new();
    for stem in rust_names(dir) {
        let text = read(&dir.join(format!("{stem}.rs")));
        let mut rest = text.as_str();
        while let Some(at) = rest.find("#[path = \"") {
            rest = &rest[at + "#[path = \"".len()..];
            match rest.find('"') {
                Some(end) => {
                    names.insert(rest[..end].trim_end_matches(".rs").to_string());
                }
                None => break,
            }
        }
    }
    names.into_iter().collect()
}

/// The test-tree rules AGENT.md states, checked against the folder rather than repeated from the prose:
/// one binary, no `[[test]]` target, and every file under `tests/suite` either listed by
/// `tests/suite/mod.rs` or pulled in by a `#[path]` attribute. A layout written down drifts; this line
/// cannot, because it is computed from what is there.
fn test_tree_rules(root: &Path) -> String {
    let all_rs = read(&root.join("rust/tests/all.rs"));
    let binary = if root.join("rust/tests/all.rs").is_file() {
        if all_rs.contains("mod suite;") {
            "tests/all.rs is the one test binary".to_string()
        } else {
            "tests/all.rs does not declare mod suite;".to_string()
        }
    } else {
        "tests/all.rs is missing".to_string()
    };
    let targets = test_targets(&read(&root.join("rust/Cargo.toml")));
    let cargo = if targets == 0 {
        "Cargo.toml declares no [[test]] target".to_string()
    } else {
        format!("Cargo.toml declares {targets} [[test]] targets, which the spec forbids")
    };
    let suite = root.join("rust/tests/suite");
    let listed = suite_tests(&read(&suite.join("mod.rs")));
    let included = path_included(&suite);
    let unaccounted: Vec<String> = rust_names(&suite)
        .into_iter()
        .filter(|name| name != "mod" && name != "all")
        .filter(|name| !listed.contains(name) && !included.contains(name))
        .collect();
    let shared = rust_names(&suite)
        .into_iter()
        .filter(|name| name != "mod" && name != "all")
        .filter(|name| !listed.contains(name) && included.contains(name))
        .count();
    let tail = if unaccounted.is_empty() {
        "all of them included by some test file".to_string()
    } else {
        format!("these tests/suite files nothing builds: {}", unaccounted.join(", "))
    };
    format!(
        "rust/tests: {binary}, declaring the suite; {cargo}; {} test files listed in \
         tests/suite/mod.rs, {shared} shared modules pulled in by a #[path] attribute, {tail}",
        listed.len()
    )
}

/// How the checkout's own two trees are organised, counted from what is there rather than asserted.
pub fn organisation(root: &Path) -> Vec<String> {
    let modules = rust_names(&root.join("rust/src"));
    let declared = read(&root.join("rust/src/lib.rs"));
    let orphanless = modules
        .iter()
        .filter(|name| declared.contains(&format!("pub mod {name};")))
        .count();
    let listed = suite_tests(&read(&root.join("rust/tests/suite/mod.rs")));
    let all_tests = rust_names(&root.join("rust/tests/suite"));
    let shared = all_tests
        .iter()
        .filter(|name| *name != "mod" && !listed.contains(name))
        .count();
    vec![
        format!(
            "rust/src: {} modules, one per concern ({} of them declared in src/lib.rs)",
            modules.len(),
            orphanless
        ),
        format!(
            "rust/tests: one test binary, tests/all.rs, holding {} test files as modules of tests/suite, \
             {} shared modules pulled in by a path attribute instead (one file per spec flow, named \
             flow_step_case like f1_13_s5_\u{2026})",
            listed.len(),
            shared
        ),
        test_tree_rules(root),
    ]
}

/// The whole answer: one line per place the spec names, then how the two trees are organised, then
/// whether anything the spec names is absent.
pub fn lines(root: &Path) -> Vec<String> {
    let mut out: Vec<String> = places()
        .iter()
        .map(|place| match found(place, root) {
            Some(where_it_is) => format!(
                "{}: {} ({}, at {where_it_is})",
                place.path,
                place.purpose,
                place.kind.word()
            ),
            None => format!("{}: {} -- NOT PRESENT", place.path, place.purpose),
        })
        .collect();
    out.extend(organisation(root));
    let absent = missing(root);
    out.push(if absent.is_empty() {
        "layout: every place the spec names is present".to_string()
    } else {
        format!("layout: missing {}", absent.join(", "))
    });
    out
}
