//! Which tree is which — [`spec/AGENT.md`](../spec/AGENT.md#which-tree-is-which).
//!
//! The spec's own words: "`rust/` is the application being built, written against `spec/`. `gui/` is
//! the Go prototype `spec/` was written from: read it for observed behaviour; new work goes in
//! `rust/`." Three trees, three different jobs, and the difference between them decides where a line
//! of code may be written — so the answer belongs to the program rather than to a README someone
//! stops reading.
//!
//! # Why the app answers this at all
//!
//! Two doors use this module, both in `main`, both before any display is opened: `--trees` prints the
//! three sentences, `--trees <path>…` answers the same rule for a path the caller is holding
//! ([`role_of`] plus [`Role::takes_new_work`]), and `--help` carries the sentences under a `Trees:`
//! block. That is the contract a person reads when they ask "where does this change go?", which is why
//! [`Role::sentence`]'s wording is asserted verbatim by a test rather than left to drift.
//!
//! The second question the same table answers is one the code has to ask of itself: a doc comment that
//! cites a prototype file is claiming the prototype says something, and a citation into a file that
//! does not exist claims nothing ([`dead_prototype_citations_for_test`] finds those, run by a test over
//! the shipped sources). Prototype citations are how this rewrite records observed behaviour —
//! `src/edges.rs` cites `gui/retake.go` for a bound it could not have guessed — so a stale one is a
//! broken pointer, not a cosmetic miss.

use std::path::{Component, Path};

/// The application folder named by the spec: `rust/`.
pub const APP_DIR: &str = "rust";

/// The Go prototype's folder named by the spec: `gui/`.
pub const PROTOTYPE_DIR: &str = "gui";

/// The rewrite spec's folder: `spec/`, the source of truth and read-only to the code.
pub const SPEC_DIR: &str = "spec";

/// One of the checkout's three trees and what it is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Role {
    /// `rust/` — the application being built. New work lands here and nowhere else.
    App,
    /// `gui/` — the Go prototype the spec was written from: evidence about behaviour, not a target.
    Prototype,
    /// `spec/` — the rewrite spec: what the app is written against.
    Spec,
}

impl Role {
    /// The three trees in the order the spec names them, which is also the order `--trees` prints.
    pub fn all() -> [Role; 3] {
        [Self::App, Self::Prototype, Self::Spec]
    }

    /// The folder name this tree lives under at the repo root.
    pub fn dir(self) -> &'static str {
        match self {
            Self::App => APP_DIR,
            Self::Prototype => PROTOTYPE_DIR,
            Self::Spec => SPEC_DIR,
        }
    }

    /// One line of the answer: the folder, then what it is for. Asserted word-for-word by a test, so
    /// changing one of these is a deliberate edit to what the program tells a person.
    pub fn sentence(self) -> &'static str {
        match self {
            Self::App => "rust/ the application being built, written against spec/",
            Self::Prototype => {
                "gui/ the Go prototype spec/ was written from: read it for observed behaviour, write no new work here"
            }
            Self::Spec => "spec/ the rewrite spec: the source of truth, read-only to the code",
        }
    }

    /// Whether new work may be written into this tree. Only the app's: the spec says new work goes in
    /// `rust/`, and `spec/` is read-only while `/spec` runs (the prototype is read for behaviour only).
    pub fn takes_new_work(self) -> bool {
        matches!(self, Self::App)
    }

    /// A citation of this tree's own files, spelled the way every doc comment in `rust/src` spells one:
    /// `<dir>/<name>` from the repo root, which is what [`crate::tree_roles::role_of`] recognises in an
    /// absolute checkout path, a relative path, and a comment alike.
    pub fn citation(self, name: &str) -> String {
        format!("{}/{name}", self.dir())
    }
}

/// Which tree a path belongs to: the first component that names one of the three trees wins, so this
/// answers for a relative path, an absolute checkout path, and a doc-style `../gui/cut.go` alike.
/// `None` is an honest "this path is in none of the three" — a scratch file, a fixture, an install path.
pub fn role_of(path: &Path) -> Option<Role> {
    path.components().find_map(|part| match part {
        Component::Normal(name) => Role::all()
            .into_iter()
            .find(|role| name.to_str() == Some(role.dir())),
        _ => None,
    })
}

/// The prototype files a piece of text cites that are NOT files under `gui_root`.
///
/// The program never calls this: a test runs it over every shipped source file, and the `_for_test`
/// suffix says so out loud. It lives here rather than in the test because the spelling rule below
/// depends on the tree names this module owns — a citation into the wrong tree is only decidable
/// against them.
///
/// A citation is spelled `gui/<name>.go`, sometimes with a line range behind it (`gui/align.go:220`).
/// Only a citation at the START of a path segment counts: prose writes "the prototype `wordPad`,
/// gui/retake.go" and "(`gui/cut_audio.go`)" — where the name follows a backtick, a bracket or a space
/// — while a checkout path ending in `.../gui/cut.go` names this container's own folder, not a claim
/// about the prototype. Each distinct cited file is checked as `gui_root.join(name)`; the ones that are
/// not files come back sorted, for the caller to report. Deciding the spelling here rather than in a
/// test keeps it beside the tree names it depends on.
pub fn dead_prototype_citations_for_test(text: &str, gui_root: &Path) -> Vec<String> {
    let mut dead: Vec<String> = Vec::new();
    let bytes = text.as_bytes();
    let mut from = 0usize;
    while let Some(offset) = text[from..].find(PROTOTYPE_DIR).map(|at| from + at) {
        // The citation is `<dir>/<name>`; the tree's own folder name is spelled once, here.
        let cited_at = offset + PROTOTYPE_DIR.len();
        if text.get(cited_at..cited_at + 1) != Some("/") {
            from = cited_at;
            continue;
        }
        // A segment start is the beginning of the text or a byte no path segment can contain: prose
        // writes "the prototype `wordPad`, gui/retake.go", while `/workspaces/.../gui/cut.go` is a
        // checkout path naming this container's own folder, not a claim about the prototype.
        let at_segment_start = offset == 0
            || !matches!(
                bytes[offset - 1],
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'_' | b'.' | b'-' | b'/'
            );
        from = cited_at + 1;
        if !at_segment_start {
            continue;
        }
        let mut end = from;
        while end < bytes.len() && is_citation_byte(bytes[end]) {
            end += 1;
        }
        let cited = &text[from..end];
        let Some(base) = cited.split(':').next() else { continue };
        if !base.ends_with(".go") || dead.iter().any(|seen| seen == base) {
            continue;
        }
        if !gui_root.join(base).is_file() {
            dead.push(format!("{PROTOTYPE_DIR}/{base}"));
        }
    }
    dead.sort();
    dead
}

/// Characters a `gui/…go` citation may contain: a path segment plus its separators, stopping at a
/// space, a quote, a bracket or the `:` that starts a line range.
fn is_citation_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'_' | b'.' | b'-')
}
