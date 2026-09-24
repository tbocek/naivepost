//! Where each kind of decision lives — directive A, `spec/11-flow-index.md` §5.
//!
//! Directive A sorts every decision the app makes into four kinds and gives each one home:
//!
//! | kind | where it lives | who owns it here |
//! |---|---|---|
//! | what the video becomes | editing policy (§10 §2) | [`crate::params`] rows, [`crate::project::Field`]/[`Origin`] |
//! | how a job is worded | the prompts | [`crate::prompts`] keys, [`crate::settings`]'s prompt files |
//! | which servers and binaries | machine settings | [`crate::settings`]'s `llm.conf` keys |
//! | how the app looks, waits and caches | engineering constants | the module whose rule reads the value |
//!
//! This module holds no values of its own — that would break the very rule it describes. It only
//! routes an id to its kind, so a caller (a doc test, a form, a review script) can ask "whose
//! decision is this?" without knowing which prefix table answered. The routing delegates to the
//! owners: [`prompts::is_known`], [`settings::Conf::key_names`] + [`settings::Slots::KEY_NAMES`],
//! and [`params::family`], so none of the four homes can be claimed twice.

use crate::{
    params::{self, Family},
    project::Origin,
    prompts, settings,
};

/// One of directive A's four homes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Home {
    /// "What the video becomes (lengths, thresholds, budgets)": the editing policy, derived from
    /// the User Context by F0.7 and editable on top of that.
    Policy,
    /// "How a job is worded": the shipped prompts, overridable per machine.
    Prompt,
    /// "Which servers and binaries": the Settings dialog and `llm.conf`.
    Machine,
    /// "How the app looks, waits and caches": fixed in code, in the module whose rule uses it.
    Engineering,
}

impl Home {
    pub fn all() -> [Home; 4] {
        [Self::Policy, Self::Prompt, Self::Machine, Self::Engineering]
    }

    /// The home as §5 names it.
    pub fn label(self) -> &'static str {
        match self {
            Self::Policy => "editing policy",
            Self::Prompt => "the prompts",
            Self::Machine => "machine settings",
            Self::Engineering => "engineering constants",
        }
    }

    /// What belongs here, in §5's own words.
    pub fn what_lives_here(self) -> &'static str {
        match self {
            Self::Policy => "what the video becomes \u{2014} lengths, thresholds, budgets",
            Self::Prompt => "how a job is worded",
            Self::Machine => "which servers and binaries",
            Self::Engineering => "how the app looks, waits and caches",
        }
    }

    /// Where a person changes it — the question that decides whether a value is *settable* or only
    /// readable. Engineering values have no place to change them at run time, which is the point of
    /// calling them constants rather than leaving that unsaid.
    pub fn where_you_change_it(self) -> &'static str {
        match self {
            Self::Policy => {
                "the policy form (F0.7's fields, each with its value, source and reason); the \
                 default stands until the User Context speaks to that field"
            }
            Self::Prompt => {
                "Prepare's prompt bench \u{2014} kept per machine under `prompts/<key>.txt`, and \
                 only while it differs from the shipped wording"
            }
            Self::Machine => "the Settings dialog / `llm.conf`",
            Self::Engineering => "nowhere: fixed in code, in the module whose rule reads it",
        }
    }
}

/// Whether this conf or slot key is one of the machine's own settings (§10 §1).
///
/// These are `llm.conf` KEYNAMES rather than §10 parameter ids — `LLM_SERVER`, `FFMPEG`,
/// `LLM_SLOTS` — so they cannot be routed by the `P.*` prefix rule and need their own check. Both
/// lists are read from the store itself, so a key added there is a machine setting here too.
pub fn is_machine_setting(conf_key: &str) -> bool {
    settings::Conf::key_names()
        .iter()
        .chain(settings::Slots::KEY_NAMES.iter())
        .any(|key| *key == conf_key)
}

/// Which home an id belongs to, `None` when the id names nothing the app knows.
///
/// Order matters, and each step defers to a different owner:
/// 1. a prompt key ([`prompts::is_known`]) — first because a prompt key is a bare word like `cut`
///    that matches no prefix rule at all.
/// 2. a `llm.conf` key name ([`is_machine_setting`]) — also bare (`FFMPEG`, `LLM_SLOTS`), and
///    checked before the dotted-bare rule below so a conf key is never mistaken for an unrowed
///    engineering value.
/// 3. a rowed §10 id, routed through [`params::family`] so the two prefix tables cannot drift:
///    `P.policy.` → Policy, `P.machine.` → Machine, `P.eng.` → Engineering.
/// 4. any other dotted id — every value §10 leaves unrowed and this tree therefore gives a bare
///    prefix (`layout.`, `preview.`, `llm.`, `narrate.`, `effects.`, `card.`, `machine.`,
///    `project.`) → Engineering. That is inheritance, not a guess: [`Family::Other`]'s own doc
///    calls such an id "an engineering value §10 has no row for yet".
///
/// Note the one case that looks wrong and is not: bare `machine.jpegQuality` is *engineering*, not
/// a machine setting. The bare `machine.` prefix names which rule sizes the value; only a `P.`-spelled
/// `P.machine.*` row is a Settings entry. Same reason `P.unknown.thing` lands in Engineering — a
/// prefix §10 has no section for inherits the home that needs no section, which is exactly why a
/// new home means spelling a new prefix in §10 first.
pub fn home_of(id: &str) -> Option<Home> {
    if prompts::is_known(id) {
        return Some(Home::Prompt);
    }
    if is_machine_setting(id) {
        return Some(Home::Machine);
    }
    match params::family(id) {
        Family::Policy => return Some(Home::Policy),
        Family::Machine => return Some(Home::Machine),
        Family::Eng => return Some(Home::Engineering),
        // `Family::Project` and `Family::Other` both fall through to the bare-prefix rule below:
        // §3's tab controls and §10's unrowed values are both "fixed in code" as far as directive
        // A's fourth bullet goes — the project stores a *choice* made from a list this build ships,
        // and the list itself is the constant.
        Family::Project | Family::Other => {}
    }
    // A dot with something on both sides is a prefixed id; the prefix says which module owns it, and
    // owning it in code is what makes it an engineering value.
    let (prefix, rest) = id.split_once('.')?;
    if prefix.is_empty() || rest.is_empty() {
        return None;
    }
    Some(Home::Engineering)
}

/// Whether F0.7's derivation may replace this field's value.
///
/// F0.7 S4: a hand-set field (`source: user`) is never overwritten. That single exception is what
/// makes the policy home both halves of §5's sentence at once — derived from the User Context, and
/// still editable, because a person's answer outranks the model's on the field they touched.
pub fn overridable_by_derivation(origin: Origin) -> bool {
    origin != Origin::User
}
