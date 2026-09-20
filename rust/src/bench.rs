//! The prompt bench: one heading row and one text box holding everything the models
//! are told, in the order the pipeline sends it (§04-prepare#1-screen).
//!
//! Row 0 is the project's User Context — what the editor knows about THIS session,
//! sent with every request and outranking the prompts, so it leads. Rows 1–12 are the
//! twelve prompts in pipeline order, per machine: an edit lives on this machine, in
//! `settings::Paths::prompts_dir`, which is why the row carries a mark rather than a
//! tick anywhere else. Storage is [`crate::settings`]'s, unchanged: it already knows
//! that text equal to the shipped wording means "no override".

use crate::{
    project::Project,
    settings::{self, Paths},
};

/// The mark on a row whose wording this machine holds (§1: "✎ = edited on this machine").
pub const EDITED_MARK: &str = "\u{270e}";

/// What the mark means, since ✎ alone does not say where the text went.
pub const EDITED_TIP: &str = "edited \u{2014} kept in your settings";

/// One row of the bench: what it is called on screen and the key its text is stored
/// under. An empty key means the User Context, which belongs to the project rather
/// than to this machine and so has no file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Row {
    pub title: &'static str,
    pub key: &'static str,
}

/// The rows in pipeline order — this page's two jobs, then the cut and its audit, then
/// the narration, then the upload text. Per machine: a different build could ship a
/// different set, which is why nothing here counts them at compile time beyond this list.
pub const ROWS: [Row; 13] = [
    Row { title: "User Context", key: "" },
    Row { title: "System context", key: "system" },
    Row { title: "Describe", key: "describe" },
    Row { title: "Transcript", key: "fix" },
    Row { title: "Retakes", key: "retake" },
    Row { title: "Text edit", key: "textedit" },
    Row { title: "Cut", key: "cut" },
    Row { title: "Captions", key: "captions" },
    Row { title: "Speed", key: "speed" },
    Row { title: "Effects", key: "effects" },
    Row { title: "Narration", key: "narrate" },
    Row { title: "Translate", key: "translate" },
    Row { title: "Upload text", key: "youtube" },
];

/// The heading: "\u2039Name\u203a prompt" for a prompt, and the plain name for the User
/// Context — it is not a prompt, and calling it one would hide that it outranks them.
pub fn title(row: &Row) -> String {
    if is_context(row) {
        return row.title.to_string();
    }
    format!("{} prompt", row.title)
}

/// Whether this row is the User Context: no key, so nothing on this machine to differ
/// from and nothing for Reset to restore (§1).
pub fn is_context(row: &Row) -> bool {
    row.key.is_empty()
}

/// Which row the bench shows. The User Context first, because it is what the editor
/// writes most and what every other row is read against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bench {
    pub selected: usize,
}

impl Default for Bench {
    fn default() -> Self {
        Self::new()
    }
}

impl Bench {
    pub fn new() -> Self {
        Self { selected: 0 }
    }

    pub fn row(&self) -> &'static Row {
        &ROWS[self.selected]
    }

    /// Choose a row. An index off the end is ignored rather than fatal: the picker's
    /// list comes from [`ROWS`], so one that does not match this build is a stale widget,
    /// and a bench that keeps showing what it was showing beats a panic in the UI thread.
    pub fn select(&mut self, index: usize) {
        if index < ROWS.len() {
            self.selected = index;
        }
    }
}

/// The mark shows only while this machine holds an edit (§1: "live only while this
/// machine holds an edit").
pub fn shows_mark(row: &Row, edited: bool) -> bool {
    !is_context(row) && edited
}

/// Reset is live under exactly the same condition: with no override there is nothing to
/// restore, and a button that does nothing invites a click.
pub fn shows_reset(row: &Row, edited: bool) -> bool {
    shows_mark(row, edited)
}

/// Does this machine hold this row's wording? An unreadable prompts folder answers
/// "no" rather than failing the screen: the bench is a view of the project, and a
/// directory this process cannot read must not stop it from opening. The write path
/// still reports its errors.
pub fn edited(paths: &Paths, row: &Row) -> bool {
    if is_context(row) {
        return false;
    }
    settings::read_prompt(paths, row.key).ok().flatten().is_some()
}

/// What the text box holds: the project's note for row 0, else this machine's text or
/// the shipped wording.
pub fn text(paths: &Paths, project: &Project, row: &Row) -> String {
    if is_context(row) {
        return project.context.clone();
    }
    settings::prompt_text(paths, row.key, SHIPPED).unwrap_or_else(|_| SHIPPED.to_string())
}

/// This build's shipped wording. Each step's round brings its own text — F1.7 the
/// describe prompt, F1.8 the fixer's, and so on — so until that round lands an empty
/// box is the shipped text and anything typed in is this machine's edit. Reset before
/// then returns to empty, which is what it means for a step with no wording yet.
const SHIPPED: &str = "";

/// Store what was typed. §1 asks for every keystroke to be stored: there is no Save
/// button here, and one would be the only way to lose an edit by closing the window.
///
/// A prompt whose text equals the shipped wording writes nothing — that is how a box
/// typed back to empty clears its mark without this module remembering anything.
pub fn store(paths: &Paths, project: &mut Project, row: &Row, text: &str) -> Result<(), String> {
    if is_context(row) {
        project.context = text.to_string();
        return Ok(());
    }
    settings::prompt_file_ok(row.key)?;
    settings::write_prompt(paths, row.key, text, SHIPPED)?;
    Ok(())
}

/// Restore the shipped wording by dropping this machine's override. The User Context
/// answers `None` and changes nothing: it has no built-in text to go back to, and a
/// "reset" that cleared the editor's own notes about their session would be data loss.
pub fn reset(paths: &Paths, row: &Row) -> Result<Option<String>, String> {
    if is_context(row) {
        return Ok(None);
    }
    settings::clear_prompt(paths, row.key)?;
    Ok(Some(SHIPPED.to_string()))
}

/// The picker's rows: index and title, in pipeline order. Titles rather than keys — the
/// picker is read by the person who named nothing of it.
pub fn picker_rows() -> Vec<(usize, String)> {
    ROWS.iter()
        .enumerate()
        .map(|(index, row)| (index, row.title.to_string()))
        .collect()
}
