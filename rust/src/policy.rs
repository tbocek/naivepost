//! F0.7 (Derive the editing policy) — `spec/03-shell.md` F0.7.
//!
//! The User Context is a paragraph of prose; the pipeline runs on five switches. This module is the
//! translation and nothing else: it turns a context into a [`Policy`], validates what a model proposes
//! against that catalogue, refuses what it does not know, keeps what a person set by hand, and answers
//! which flows the result turns on.
//!
//! Nothing here imports UI. [`crate::ui::window`] draws the form (§S5), forwards the presses to these
//! functions and prints what comes back; the derivation request is built here ([`request`]) but sent by
//! the run layer, so every rule below is testable without a server, a window or a display.
//!
//! Three rules from §00-principles hold the module together:
//! * models propose, the machine places — the model names a field and gives a reason, this decides
//!   whether that field exists and whether that value fits it (S3);
//! * nothing is deleted, only marked — a hand-set value outranks a derived one forever (S4), through
//!   [`decision_homes::overridable_by_derivation`], never a second copy of that predicate;
//! * failure is specific and local — a refusal names the field and what was wrong with the value, and
//!   says nothing about the fields that were accepted alongside it (S3).

use crate::decision_homes;
use crate::project::{CutMode, Field, MarkingPass, Origin, Policy};
use crate::prompts;
use crate::tool_loop;
use crate::tools::Tool;

/// How long the app waits after the last keystroke in the User Context before re-deriving.
///
/// `policy.debounceMs` — §03 says "debounced" without a number. 500 ms is ours: long enough that a
/// person typing a sentence causes one derivation rather than one per word, short enough that the
/// answer is waiting by the time they look at the form. A `P.*` row would imply §10 chose it; it did
/// not, so this carries a bare prefix like `machine.`/`preview.` values do.
pub const DEBOUNCE_MS: u64 = 500;

/// The prompt the derivation asks with, and the file its wording ships in.
///
/// Naming it here rather than spelling `"policy"` at the call site means S2's two facts — this prompt,
/// thinking off — are read from [`prompts`], the one place the thirteen keys' thinking flags live
/// (`prompts::thinking("policy") == false`).
pub const PROMPT: &str = "policy";

/// What the model put forward through `tool:set_policy`: a field, a value as it typed it, and its
/// reason. Kept as strings because that is what arrives over the wire; validation is what turns them
/// into a typed field or a refusal.
#[derive(Debug, Clone, PartialEq)]
pub struct Proposal {
    pub field: String,
    pub value: String,
    pub because: String,
}

/// What became of one proposal.
#[derive(Debug, Clone, PartialEq)]
pub enum Applied {
    /// Stored with `source: model` and its `because` (S3).
    Set,
    /// A person had set this field; the old value stands untouched (S4).
    KeptUser,
    /// Not stored, with the sentence the model gets back so it can correct itself (S3).
    Refused(String),
}

impl Applied {
    /// The line the status bar shows for this outcome. A kept-user answer is not an error and reads that
    /// way: the field still holds what the person chose, which is the point of S4.
    pub fn status(&self) -> String {
        match self {
            Self::Set => "set".to_string(),
            Self::KeptUser => "kept yours".to_string(),
            Self::Refused(say) => say.clone(),
        }
    }
}

/// S1: an empty User Context leaves every field at its default — `markingPass retakes`, `cutMode
/// model`, every cut-stage pass on. That is the prototype's blank-project behaviour, and it is why
/// `defaults()` returns a whole [`Policy`] instead of patching one: with no context there is nothing
/// to derive, so the caller stops here and the defaults stand as a set rather than field by field.
pub fn defaults() -> Policy {
    Policy::default()
}

/// Is this context empty enough to skip the model? Whitespace counts as empty: a context of spaces is
/// the same absence of information, and asking a model to derive a style from it invents one.
pub fn is_empty(context: &str) -> bool {
    context.trim().is_empty()
}

/// One row of the catalogue the form lists and `tool:set_policy` validates against: the field's name as
/// the spec spells it, the `P.policy.*` id it answers to, and the values it accepts.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CatalogueRow {
    pub field: &'static str,
    pub id: &'static str,
    pub values: &'static [&'static str],
}

/// The five fields F0.7 derives, in the order the form shows them.
///
/// This is the list S3 validates against and S5 renders, so "known field" and "shown in the form" are
/// one fact: a field cannot be settable-but-hidden or shown-but-unsettable. `hands_off::pipeline_fields`
/// describes the same three-way choice from the other side (what the model must not decide per request);
/// this is the writable face of it, spelled per field because validation needs each field's range.
pub fn derived_fields() -> [CatalogueRow; 5] {
    [
        // P.policy.markingPass — picks F1.9 / F1.10 / no marking pass in Prepare.
        CatalogueRow {
            field: "markingPass",
            id: "P.policy.markingPass",
            values: &["joins", "retakes", "none"],
        },
        // P.policy.cutMode — picks the words-derived or the model-chosen cut in F2.14.
        CatalogueRow {
            field: "cutMode",
            id: "P.policy.cutMode",
            values: &["words", "model"],
        },
        // P.policy.captionsPass — whether F3.9 is asked at all.
        CatalogueRow {
            field: "captionsPass",
            id: "P.policy.captionsPass",
            values: ON_OFF,
        },
        // P.policy.speedPass — whether F3.10 is asked at all.
        CatalogueRow {
            field: "speedPass",
            id: "P.policy.speedPass",
            values: ON_OFF,
        },
        // P.policy.decorationsPass — whether F3.11 is asked at all.
        CatalogueRow {
            field: "decorationsPass",
            id: "P.policy.decorationsPass",
            values: ON_OFF,
        },
    ]
}

/// The two spellings a switch accepts. `true`/`false` as well as `on`/`off` because JSON booleans reach
/// the tool call unquoted and a model that sends `true` is not wrong — refusing it would be pedantry
/// that costs a round trip.
const ON_OFF: &[&str] = &["on", "off", "true", "false"];

/// Look a proposed field name up in the catalogue.
fn lookup(field: &str) -> Option<CatalogueRow> {
    derived_fields().into_iter().find(|row| row.field == field)
}

/// S3: validate one proposal and store it if it passes.
///
/// Two refusals, both naming what was wrong so the model can fix it in its next round: an unknown field
/// (the catalogue is closed — a field the form does not show cannot be set), and a value outside that
/// field's own range. Neither refusal touches the policy, so a batch where three proposals land and one
/// does not leaves exactly the three landed (failure is local).
///
/// S4 is checked first: a field a person set is never overwritten, so a valid proposal for it reports
/// `KeptUser` and changes nothing. That goes through
/// [`decision_homes::overridable_by_derivation`] — the same predicate [`crate::hands_off`] uses — so
/// the rule lives once.
pub fn apply(policy: &mut Policy, proposal: &Proposal) -> Applied {
    let Some(row) = lookup(&proposal.field) else {
        return Applied::Refused(format!(
            "{} is not a policy field \u{2014} the fields are {}",
            proposal.field,
            derived_fields()
                .iter()
                .map(|r| r.field)
                .collect::<Vec<_>>()
                .join(", ")
        ));
    };
    if !row.values.contains(&proposal.value.as_str()) {
        return Applied::Refused(format!(
            "{} is out of range for {} \u{2014} it takes {}",
            proposal.value,
            row.field,
            row.values.join(" / ")
        ));
    }
    if !decision_homes::overridable_by_derivation(current_origin(policy, row.field)) {
        return Applied::KeptUser;
    }
    let because = (!proposal.because.is_empty()).then(|| proposal.because.clone());
    match row.field {
        "markingPass" => policy.marking_pass = Field {
            value: parse_marking(&proposal.value),
            origin: Origin::Model,
            because,
        },
        "cutMode" => policy.cut_mode = Field {
            value: parse_cut_mode(&proposal.value),
            origin: Origin::Model,
            because,
        },
        "captionsPass" => policy.captions_pass = bool_field(&proposal.value, because),
        "speedPass" => policy.speed_pass = bool_field(&proposal.value, because),
        "decorationsPass" => policy.decorations_pass = bool_field(&proposal.value, because),
        // Unreachable: `lookup` returned this row from the same five arms. Reaching it would mean the
        // catalogue and this match drifted apart, which is a bug in this file, not a user error.
        _ => unreachable!("catalogue row {} has no writer", row.field),
    }
    Applied::Set
}

/// S3/S5: the origin currently on a field, read by name so the S4 check needs no match of its own.
fn current_origin(policy: &Policy, field: &str) -> Origin {
    match field {
        "markingPass" => policy.marking_pass.origin,
        "cutMode" => policy.cut_mode.origin,
        "captionsPass" => policy.captions_pass.origin,
        "speedPass" => policy.speed_pass.origin,
        "decorationsPass" => policy.decorations_pass.origin,
        _ => Origin::Default,
    }
}

/// The value half of S3 for the two enums. Range was already checked against the catalogue's spellings,
/// so an unmatched string here is the same drift the `unreachable!` below guards.
fn parse_marking(value: &str) -> MarkingPass {
    match value {
        "joins" => MarkingPass::Joins,
        "none" => MarkingPass::None,
        _ => MarkingPass::Retakes,
    }
}

fn parse_cut_mode(value: &str) -> CutMode {
    match value {
        "words" => CutMode::Words,
        _ => CutMode::Model,
    }
}

fn bool_field(value: &str, because: Option<String>) -> Field<bool> {
    Field { value: value != "off" && value != "false", origin: Origin::Model, because }
}

// ---- S2: the request ------------------------------------------------------------------------

/// What the derivation sends: the tools offered, thinking off, and the prompt's key. Built, never
/// sent — the run layer owns the HTTP, and keeping it out of here is what makes S2 checkable without a
/// server.
#[derive(Debug, Clone, PartialEq)]
pub struct DerivationRequest {
    /// `policy`, the prompt S2 names. Its wording ships in `spec/prompts/policy.md`.
    pub prompt: &'static str,
    /// Thinking off. Read from [`prompts::thinking`], the one table of the thirteen keys' flags, so
    /// this cannot claim off while the table says on.
    pub thinking: bool,
    /// `get_context` + `set_policy` — the context and the fields it may set, then the setter.
    pub tools: Vec<Tool>,
    /// The user turn: the context text itself.
    pub context: String,
}

/// S2: build the derivation request for this context.
///
/// Tools come from [`crate::tools::policy_tools`] (§3.11's pair — `tool:get_context` and
/// `tool:set_policy`) and their schemas through [`tool_loop::schemas`], so the wire shape is the
/// loop's, not a hand-rolled copy. Thinking is read from the prompt table rather than hard-coded,
/// which is the difference between asserting a wish and asserting the shipped flag.
pub fn request(context: &str) -> DerivationRequest {
    DerivationRequest {
        prompt: PROMPT,
        thinking: prompts::thinking(PROMPT),
        tools: crate::tools::policy_tools(),
        context: context.to_string(),
    }
}

impl DerivationRequest {
    /// The tool schemas the wire wants, through the loop's own builder.
    pub fn tool_schemas(&self) -> Vec<serde_json::Value> {
        tool_loop::schemas(&self.tools, &|_| None)
    }
}

// ---- the consequences: which flows the policy turns on ---------------------------------------

/// Which cut-stage pass a switch governs. Named so `pass_runs` reads as a question about a pass rather
/// than about a boolean field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pass {
    /// F3.9 — captions proposed by the model after the cut.
    Captions,
    /// F3.10 — speeds proposed by the model.
    Speeds,
    /// F3.11 — decorations proposed by the model.
    Decorations,
}

/// The pipeline fields decide which flows run (§03's closing paragraph). These four readers are how a
/// caller asks that question without reaching into `Policy`'s field names, so the mapping from switch
/// to flow lives in one file.
pub fn marking_pass_of(policy: &Policy) -> MarkingPass {
    policy.marking_pass.value
}

pub fn cut_mode_of(policy: &Policy) -> CutMode {
    policy.cut_mode.value
}

/// Whether this cut-stage pass is called at all. Off means the flow is never asked, not asked-and-
/// ignored — which is the whole point of deriving it: "The rewrite asks no job the context rules out."
pub fn pass_runs(policy: &Policy, pass: Pass) -> bool {
    match pass {
        Pass::Captions => policy.captions_pass.value,
        Pass::Speeds => policy.speed_pass.value,
        Pass::Decorations => policy.decorations_pass.value,
    }
}

/// Does changing `markingPass` invalidate the existing marks?
///
/// True when the two differ, because the marks themselves are the thing that goes stale: `retakes.tsv`
/// and `final.txt` hold what the previous pass decided, and a different pass reads them as its own
/// output and skips its work. Answering here rather than deleting anything keeps this a pure question —
/// the run layer decides when the marks are actually dropped, via
/// [`crate::layout::Tree::final_txt`] and [`crate::layout::Tree::retakes_tsv`].
///
/// Same value ⇒ false: re-deriving the same policy from an edited context that happens to agree must
/// not throw away work that is still correct.
pub fn invalidated_by_marking_change(old: MarkingPass, new: MarkingPass) -> bool {
    old != new
}
