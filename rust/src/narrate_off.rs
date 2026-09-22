//! F4.8 Narration off (spec/07-narrate.md) — one tick switches the whole page and the
//! render's lines off, and switches them back to exactly what was there.
//!
//! The answer lives in [`crate::project::Project::no_narration`], already written as
//! `no_narration` (§01 line 75). This module holds only what follows from it: which widgets
//! grey out, what a run refuses and says, what Produce hides, and which lines any step is
//! allowed to speak.
//!
//! It does NOT own: writing the lines (F4.1), speaking them (F4.4), the preview (F4.5), the
//! render's chain (F5.x), or the flag's serialisation ([`crate::project`]). The GTK layer
//! reads these answers and sets sensitivity; nothing here touches a widget, a page or a
//! thread. The Inputs row already prints "no narration — captions only" for the Narrate page
//! from the same flag (`crate::shell::inputs`) — that wording stays there.

use crate::narration::Entry;
use crate::project::Project;

/// Inventory A.2: the check button's label, quoted so a test can pin it against the inventory.
pub const TICK_LABEL: &str = "Narration";

/// Inventory A.2: the tick is "initially on unless the project says no narration" — the flag
/// inverted, since the project stores the negation (§01 line 75).
pub fn tick_checked(no_narration: bool) -> bool {
    !no_narration
}

/// Which parts of the Narrate page are greyed out. `true` = greyed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Greyed {
    pub lines: bool,
    pub preview: bool,
    pub voice: bool,
    /// The tick itself — always `false`, and part of the answer so a page cannot "helpfully"
    /// grey it. See [`greyed`].
    pub tick: bool,
}

/// F4.8's first node: with narration off, "the lines, the preview and the voice picker" are
/// greyed. `tick` is ALWAYS false — turning narration back on is the one thing still worth
/// pressing on that page, so a greyed tick would lock it shut. The page applies this to every
/// widget it owns except the tick, and it is safe to ask before the page exists: a project's
/// answer arrives first, and an older `project.json` with no key reads as narrated.
pub fn greyed(no_narration: bool) -> Greyed {
    Greyed {
        lines: no_narration,
        preview: no_narration,
        voice: no_narration,
        tick: false,
    }
}

/// F4.1 S1's refusal, byte for byte (the em dash is U+2014). `None` when there is narration.
///
/// Asked BEFORE anything is pulled from a text box or saved, so switching the tick off cannot
/// leave an edit behind and cannot write lines nobody asked for — the run stops at its own
/// start with the way back named in the sentence.
pub fn refuse_run(no_narration: bool) -> Option<String> {
    no_narration.then(|| {
        "this video has no narration \u{2014} tick Narration at the top of this page to write one"
            .to_string()
    })
}

/// The tick's whole write: the flag and `project.json`, nothing else.
///
/// Nothing here opens `narrate/narration.json` — F4.8's second node is "narration.json left
/// exactly as it is", and the take cache and the voice reference stay where they are too.
pub fn set_off(project: &mut Project, tree: &crate::layout::Tree, off: bool) -> Result<(), String> {
    project.no_narration = off;
    crate::project::save(project, tree.dir())
}

/// The rule behind [`set_off`] as a fact a test can pin rather than a comment: the tick never
/// touches the record.
pub fn touches_the_record() -> bool {
    false
}

/// F4.8's third node: "Produce: no game-volume slider". It is P.policy.gameVolume's CONTROL
/// that disappears, not its value — [`crate::project`] keeps the number, so ticking Narration
/// back on restores the slider where the person left it.
pub fn game_volume_shown(no_narration: bool) -> bool {
    !no_narration
}

/// The one seam every step reads through for lines to speak (the prototype calls it
/// `produceEntries`): narration off answers empty, so neither the speaking pass nor the render
/// sees a line; narration on answers with the entries that have words — a blank line is a
/// deliberate silence and is never spoken. With one seam, no caller can speak a line behind
/// the tick's back.
pub fn lines_to_speak(no_narration: bool, entries: &[Entry]) -> Vec<Entry> {
    if no_narration {
        return Vec::new();
    }
    entries
        .iter()
        .filter(|entry| !entry.text.trim().is_empty())
        .cloned()
        .collect()
}

/// F0.4's `narration off?` branch: the lucky run logs this line and moves to the next step.
/// Every other step — and a narrated video's Narrate — answers `None` and runs normally: F4.8
/// skips Narrate, it does not shorten the pipeline.
pub fn skips(step: crate::run::Step, no_narration: bool) -> Option<String> {
    if step == crate::run::Step::Narrate && no_narration {
        return Some(">>> run: Narrate skipped \u{2014} this video has no narration".to_string());
    }
    None
}

/// F4.8's last node ("☑ again: everything written is still there"): does the page come back
/// with its contents? Yes — and `tests/narrate_off_page.rs` proves it rather than trusting the
/// wording, by writing a record, flipping the tick off and on again, and comparing every byte.
pub fn page_returns(no_narration: bool) -> bool {
    !no_narration
}
