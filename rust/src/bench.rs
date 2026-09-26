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
    prepare,
    project::Project,
    roles,
    settings::{self, Paths},
    tools::describe,
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
///
/// Through [`settings::stored_prompt`], so an edit kept under the old folder-per-job layout
/// still shows its mark here rather than looking untouched next to replaced wording.
pub fn edited(paths: &Paths, row: &Row) -> bool {
    if is_context(row) {
        return false;
    }
    settings::stored_prompt(paths, row.key).ok().flatten().is_some()
}

/// What the text box holds: the project's note for row 0, else this machine's text or
/// the shipped wording.
pub fn text(paths: &Paths, project: &Project, row: &Row) -> String {
    if is_context(row) {
        return project.context.clone();
    }
    settings::stored_prompt(paths, row.key)
        .ok()
        .flatten()
        .unwrap_or_else(|| SHIPPED.to_string())
}

/// This build's shipped wording. Each step's round brings its own text — F1.7 the
/// describe prompt, F1.8 the fixer's, and so on — so until that round lands an empty
/// box is the shipped text and anything typed in is this machine's edit. Reset before
/// then returns to empty, which is what it means for a step with no wording yet.
/// This build's shipped wording, published so a widget test can assert what Reset repaints the
/// box with without retyping the constant.
pub const SHIPPED_FOR_TESTS: &str = "";

const SHIPPED: &str = SHIPPED_FOR_TESTS;

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

/// What the mark on a row means, spelled out because ✎ alone does not say where the
/// text went — and where it went is the whole question when a newer build ships better
/// wording (§04-prepare#6: "the mark's tooltip …"). Written on one line so the sentence
/// a person reads matches §6 character for character.
pub const KEPT_TIP: &str =
    "Your wording is kept in ~/.config/naivepost/prompts, so a newer built-in prompt will not replace it. Reset puts it back.";

/// Reset live: what the click will do, since the button is next to a box that may hold
/// an hour of the editor's own tuning (§04-prepare#6).
pub const RESET_TIP: &str = "Put the built-in wording back";

/// Reset idle. A live button under no mark reads as "there is something stored", which
/// is exactly what the empty mark is denying, so the tooltip says so instead.
pub const UNCHANGED_TIP: &str = "This is the built-in wording, unchanged";

/// Which of the two Reset tooltips this row carries. The User Context never gets the
/// live one: it has no built-in wording to go back to, and a Reset that cleared the
/// editor's notes about their session would be data loss rather than a restore.
pub fn reset_tip(row: &Row, edited: bool) -> &'static str {
    if shows_reset(row, edited) {
        RESET_TIP
    } else {
        UNCHANGED_TIP
    }
}

/// What the heading row's tooltip says about the selected row: what the job gets and
/// what it answers (§04-prepare#6). The two rows whose wording is an arithmetic fact —
/// how many frames, how many lines — read theirs out of the constants that build the
/// request, so a retuned request cannot leave this text behind.
pub fn tooltip(row: &Row) -> String {
    match row.key {
        "" => "What kind of video this is, who is in it, what they were doing, how names are \
            spelled, how long it should run, what has to end up in it.\n\nSent with every \
            request this project makes — the frame describer, the transcript fixer, the cut \
            and its three passes, the narration, the upload text — and it outranks the prompts \
            below, which are written to be true of any session. Left empty, nothing is sent."
            .to_string(),
        "system" => "The formats every job works to: the three kinds of line, which clock a \
            request stamps them on, and that the answer is read by a machine.\n\nSent in front \
            of every prompt below, so a fact about this tool is written once instead of in each \
            of them."
            .to_string(),
        "describe" => format!(
            "{} frames per request, plus the last {} descriptions and up to {} spoken lines \
             either side as context. No frame is ever sent twice: those descriptions are the \
             model's only memory of what it already saw.",
            roles::VISION_FRAMES_PER_CALL,
            describe::RECENT_EVENTS,
            describe::CTX_SEGS
        ),
        "fix" => format!(
            "The fixer: {} transcript lines per request, each block given what every other \
             source showed or said at the same moment.",
            prepare::FIX_BLOCK_LINES
        ),
        "retake" => "Last in Prepare, on the merged timeline: which stretches were said, broken \
            off and said again. Marked, never deleted — the transcript keeps every word, and \
            the cut is handed one line saying those seconds were an attempt. The user context's \
            script, where there is one, says what was meant to be said."
            .to_string(),
        "textedit" => "The edit of a read to camera: one join between takes at a time, the last \
            words of the take that was interrupted and the first words of the take that \
            follows, answered with how many come off each side. The session's style decides \
            whether this or Retakes runs."
            .to_string(),
        "cut" => "How ▶ Suggest chooses the moments: read what the session is, place what the \
            context names, fill the rest, shape the whole. It assumes nothing about the kind of \
            video — that is what the context above is for."
            .to_string(),
        "captions" => "The second pass, clip by clip: what was said over each kept clip, on \
            screen as text effects. Cleaned as a subtitler would; the user context says fewer, \
            or none."
            .to_string(),
        "speed" => "The third pass: how fast each kept clip plays, and the arithmetic that lands \
            the cut on the target length. A clip with captions on it runs at 1."
            .to_string(),
        "effects" => "The last pass, over the kept clips: the zooms, stops and volume that make \
            a moment land. Speed and captions are the passes before."
            .to_string(),
        "narrate" => "The craft the narration is written to: what a line is about, how it is \
            placed, how a pause is made. Who the voice IS comes from the context above."
            .to_string(),
        "translate" => "The subtitle track in another language: the finished video's own lines, \
            numbered, answered one for one with the times untouched. Which languages is chosen \
            on the Produce page."
            .to_string(),
        "youtube" => "Gets the cut and the narration — no images — and answers with the YouTube \
            title, the thumbnail instruction and the description."
            .to_string(),
        _ => row.title.to_string(),
    }
}
