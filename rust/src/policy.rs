//! F0.7 (Derive the editing policy) — `spec/03-shell.md` F0.7.
//!
//! The User Context is a paragraph of prose; the pipeline runs on five switches. This module is the
//! translation and nothing else: it turns a context into a [`Policy`], validates what a model proposes
//! against that catalogue, refuses what it does not know, and answers which flows the result turns on.
//!
//! Nothing here imports UI. The shell forwards its presses to these functions and prints what comes back
//! (F0.7 S4: every stored value is said in the log); the derivation request is built here ([`request`])
//! but sent by the run layer, so every rule below is testable without a server, a window or a display.
//!
//! Two rules from §00-principles hold the module together:
//! * models propose, the machine places — the model names a field and gives a reason, this decides
//!   whether that field exists and whether that value fits it (S3);
//! * failure is specific and local — a refusal names the field and what was wrong with the value, and
//!   says nothing about the fields that were accepted alongside it (S3).

use crate::project::{CutMode, Field, MarkingPass, Origin, Policy};
use crate::prompts;
use crate::tool_loop;
use crate::tools::Tool;

/// How long the app waits after the last keystroke in the User Context before re-deriving.
///
/// `policy.debounceMs` — §03 says "debounced" without a number. 500 ms is ours: long enough that a
/// person typing a sentence causes one derivation rather than one per word, short enough that the
/// answer is waiting by the time the next ▶ runs. A `P.*` row would imply §10 chose it; it did
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
    /// Stored with its `because` (S3).
    Set,
    /// Not stored, with the sentence the model gets back so it can correct itself (S3).
    Refused(String),
}

impl Applied {
    /// The line the status bar shows for this outcome.
    pub fn status(&self) -> String {
        match self {
            Self::Set => "set".to_string(),
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

/// One row of the catalogue `tool:set_policy` validates against: the field's name as
/// the spec spells it, the `P.policy.*` id it answers to, and the values it accepts.
///
/// A row with an empty `values` list is a NUMBER rather than a choice: `policy::apply` reads its value as
/// seconds instead of offering a pick-one list. That is what makes
/// `P.policy.targetLengthSeconds` a row of this catalogue without pretending to be a pick-one list.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CatalogueRow {
    pub field: &'static str,
    pub id: &'static str,
    pub values: &'static [&'static str],
}

/// The six fields F0.7 derives, in the order the log says them.
///
/// This is the list S3 validates against, so "known field" and "a field the derivation may set" are
/// one fact: a field cannot be settable-but-unsayable. `hands_off::pipeline_fields`
/// describes the same three-way choice from the other side (what the model must not decide per request);
/// this is the writable face of it, spelled per field because validation needs each field's range.
pub fn derived_fields() -> [CatalogueRow; 6] {
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
        // P.policy.targetLengthSeconds — §12 entry 21. A number, so no `values`: the derivation writes the seconds
        // and the derivation writes what the User Context's own words say (or a model's proposal), instead of
        // the prototype's regex that silently left 0 = none.
        CatalogueRow {
            field: "targetLengthSeconds",
            id: "P.policy.targetLengthSeconds",
            values: &[],
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
/// (the catalogue is closed — a value that cannot be set cannot be proposed), and a value outside that
/// field's own range. Neither refusal touches the policy, so a batch where three proposals land and one
/// does not leaves exactly the three landed (failure is local).
///
/// §F0.7 has no hand-set precedence any more — "There is no policy form: a value changes when the User
/// Context says otherwise" — so a valid proposal always stores, whatever `source` the field carries. An
/// old `naivepost.json` may still say `"source": "user"`; loading it is fine, and the next derivation
/// overwrites that field like any other.
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
    if !row.values.is_empty() && !row.values.contains(&proposal.value.as_str()) {
        return Applied::Refused(format!(
            "{} is out of range for {} \u{2014} it takes {}",
            proposal.value,
            row.field,
            row.values.join(" / ")
        ));
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
        // A number row: the value is seconds, written as text over the wire. A value that is not a number is
        // the one refusal this row can earn, and it names the field and the bad value like every other does.
        "targetLengthSeconds" => match proposal.value.parse::<f64>() {
            Ok(seconds) if seconds >= 0.0 => policy.target_length_seconds = Field {
                value: seconds,
                origin: Origin::Model,
                because,
            },
            _ => {
                return Applied::Refused(format!(
                    "{} is not a length in seconds for {} \u{2014} send a number of seconds, or 0 for no target",
                    proposal.value, row.field
                ))
            }
        },
        // Unreachable: `lookup` returned this row from the same five arms. Reaching it would mean the
        // catalogue and this match drifted apart, which is a bug in this file, not a user error.
        _ => unreachable!("catalogue row {} has no writer", row.field),
    }
    Applied::Set
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
    Field {
        value: value != "off" && value != "false",
        origin: Origin::Model,
        because,
    }
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

/// Whether the policy still matches the context it was derived from.
///
/// This makes §03's "runs after the User Context changed" a checkable fact rather than a hope: the
/// debounce in the UI is only about *when* to ask, this is the truth about whether asking is needed at
/// all. A never-derived project needs one even with an empty context — S1's defaults are themselves an
/// answer worth having on record before the first ▶ reads them.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Tracker {
    derived_from: Option<String>,
}

impl Tracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Does `context` need a derivation? True when none has happened yet, or the text is not what the
    /// last derivation read. Compared byte-for-byte on purpose: whitespace changes are edits, and a
    /// re-derivation over an unchanged context costs nothing because the LLM cache keys on the request.
    pub fn needs_derive(&self, context: &str) -> bool {
        self.derived_from.as_deref() != Some(context)
    }

    /// Record that `context` has been derived from.
    pub fn mark_derived(&mut self, context: &str) {
        self.derived_from = Some(context.to_string());
    }

    /// Forget what was derived, so the next question says yes. Used when a project is opened or
    /// replaced: its policy came from somewhere else and tells us nothing about this context.
    pub fn forget(&mut self) {
        self.derived_from = None;
    }
}

/// What one derivation did.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Derived {
    /// S1: the context was empty, so the defaults stand and the model was never asked.
    pub used_defaults: bool,
    /// §12 entry 21: whether a target length was READ from the User Context's words and stored. A read that
    /// understood nothing leaves this false, so "no target" stays a said fact rather than a silent 0.
    pub target_read: bool,
    /// §12 entry 21: a length WAS mentioned in the User Context but this build could not spell it in seconds
    /// ("the length of a train journey"). The entry's complaint is silence, so the gap has to be a said fact;
    /// "read" is only true when seconds actually landed, which is what keeps [`target_length_log`] from
    /// claiming a reading that understood nothing.
    pub target_gap: bool,
    /// Each accepted proposal, by field name (S3).
    pub applied: Vec<(&'static str, Applied)>,
    /// Each proposal that LANDED, by field name, in the order they landed — index-aligned with [`applied`],
    /// which is why refusals are left out of both. S4's log line reads the value and the `because` off these,
    /// so a field the model corrected after a refusal is said with the reason that actually won.
    pub proposals: Vec<Proposal>,
    /// Every refusal the loop collected, oldest first — the sentences the model was handed back.
    pub refusals: Vec<String>,
}

impl Derived {
    /// The line the status bar prints for a whole derivation. A refusal is worth saying out loud:
    /// it is the one case where the policy did not follow what the model proposed.
    pub fn status(&self) -> String {
        if self.used_defaults {
            return "no User Context \u{2014} every field at its default".to_string();
        }
        if let Some(last) = self.refusals.last() {
            return last.clone();
        }
        format!("policy derived \u{2014} {} field(s)", self.applied.len())
    }
}

/// How many times a derivation lets the model answer before giving up on it.
///
/// Bare prefix (`machine.deriveRounds`), because §03 spells the loop ("error back to the model")
/// without a bound. An unbounded one is not a retry, it is a hang: a model that keeps proposing a
/// field it invented would spin against the server forever. Three gives it two chances to read the
/// refusal and correct itself, then reports what it could not settle.
pub const DERIVE_ROUNDS: u32 = 3;

/// S1-S4 in one call. `answer` is the model: it is handed the request and the refusals so far, and
/// returns the proposals for this round. Injected rather than sent here so the whole flow is testable
/// with no server, no window and no display — the run layer owns the HTTP, exactly as [`request`] does.
///
/// S1 short-circuits before anything else: an empty context means there is nothing to derive, so
/// `answer` is never called at all (a test counts the calls). Otherwise each round runs every
/// proposal through [`apply`], which holds the rule that matters: validation names the field and what
/// was wrong with the value (S3). Refusals are fed back into the next round's `answer`; accepted values
/// stay put whatever the next round says.
pub fn derive(
    policy: &mut Policy,
    context: &str,
    answer: &mut dyn FnMut(&DerivationRequest, &[String]) -> Vec<Proposal>,
) -> Derived {
    if is_empty(context) {
        // S1: stop here. The defaults are the answer, and asking a model to derive a style from
        // nothing invents one.
        return Derived {
            used_defaults: true,
            ..Derived::default()
        };
    }
    // §12 entry 21 first, before any model is asked: a length written in plain words is arithmetic, and the
    // prototype got it wrong by reading it with a regex that had no fallback. Read here rather than in the
    // window so every door (the debounce and the next ▶) derives the same number.
    let mut out = read_target_length(policy, context);
    let req = request(context);
    for _ in 0..DERIVE_ROUNDS {
        let proposals = answer(&req, &out.refusals);
        // A round that could not be dialled at all says so itself (§failure is specific and local): the
        // sentence naming the model and its fault goes straight into the refusals, rather than being
        // re-derived from a proposal about a field that does not exist.
        if let Some(fault) = proposals
            .iter()
            .find_map(|proposal| match proposal.field.as_str() {
                crate::policy_call::FAULT_FIELD => Some(proposal.because.clone()),
                _ => None,
            })
        {
            if !out.refusals.contains(&fault) {
                out.refusals.push(fault);
            }
            // One dead model is one fault: asking again would spend §3's patience ladder per round over a
            // server that was missing on the first try and is no more reachable now.
            break;
        }
        let before = out.refusals.len();
        for proposal in proposals {
            match apply(policy, &proposal) {
                Applied::Refused(say) => out.refusals.push(say),
                other => {
                    // The field name is `'static` in the catalogue, so handing it out costs nothing
                    // and lets a test assert per-field outcomes without re-reading the policy. Only a proposal
                    // that landed is recorded beside it: that keeps the two lists index-aligned.
                    let name = lookup(&proposal.field)
                        .map(|row| row.field)
                        .unwrap_or("unknown");
                    out.proposals.push(proposal.clone());
                    out.applied.push((name, other));
                }
            }
        }
        if out.refusals.len() == before {
            // Nothing refused this round: the policy is settled, however many fields landed.
            break;
        }
    }
    out
}

/// §12 entry 21: read `P.policy.targetLengthSeconds` out of the User Context's own words and store it.
///
/// This runs through [`apply`], so it passes the same validation a model's answer passes. The log says which
/// of the three happened — read and stored (by [`derived_log`], with its reason), mentioned but unread (by
/// [`target_length_log`]), or nothing mentioned — because the whole point of the entry is that a missed
/// reading must not be silent.
pub fn read_target_length(policy: &mut Policy, context: &str) -> Derived {
    let reading = crate::policy_length::target_length_of(context);
    let mut out = Derived::default();
    // A length that was mentioned but cannot be spelled in seconds is still a fact worth logging ("the length
    // of a train journey": §12's complaint was silence), so `mentioned` proceeds even with 0 seconds. Only a
    // context that never mentions a duration stops here, because most contexts do not and that is normal.
    if !reading.mentioned {
        return out;
    }
    // The seconds go through the same door a model's answer goes through, so the cheaper reader cannot
    // slip past the validation that door holds.
    let proposal = Proposal {
        field: "targetLengthSeconds".to_string(),
        value: format!("{}", reading.seconds),
        because: "the User Context's own words".to_string(),
    };
    match apply(policy, &proposal) {
        Applied::Refused(say) => out.refusals.push(say),
        other => {
            out.proposals.push(proposal.clone());
            out.applied.push(("targetLengthSeconds", other));
            out.target_read = reading.seconds > crate::tools::cutpass::NO_TARGET;
            // The store took 0 = none, which is not a read: the mention stands as a gap for the log.
            out.target_gap = !out.target_read;
        }
    }
    out
}

/// The extra line one target-length read earns — only the cases [`derived_log`] cannot say.
///
/// §12 entry 21 now ends "said in the log with its reason": a length that was read and stored already has
/// that line (`policy: targetLengthSeconds = <seconds> — <because>`), so printing a second, vaguer row about
/// it would only risk contradicting the first. What is left to say here is the refusal, and the gap: a length
/// the User Context mentioned that this build could not spell, which must not pass for "no target".
pub fn target_length_log(derived: &Derived) -> Option<String> {
    if let Some(Applied::Refused(say)) = derived
        .applied
        .iter()
        .find(|(field, _)| *field == "targetLengthSeconds")
        .map(|(_, outcome)| outcome.clone())
    {
        return Some(format!("!!! policy: {say}"));
    }
    // A read that landed is said by `derived_log`, with its value and reason; only a mentioned-but-unread
    // length still needs saying here.
    derived.target_gap.then(|| {
        "policy: a length was mentioned in the User Context but could not be read \
         \u{2014} the target length stays none"
            .to_string()
    })
}

/// S4: one line per value this derivation STORED, spelling the field, the value and the `because` that
/// came with it — "Each stored value is said in the log with its field, value and `because`, so what the
/// model decided is visible". Those three parts are the whole requirement: no source word, no numbering,
/// nothing else, because a person reading the log is asking what changed and why, not how it arrived.
///
/// Only `Applied::Set` earns a line. A refusal was already said (it is handed back to the model and
/// reported by [`Derived::status`]), and a kept field did not change, so logging it as stored would say
/// something false. The value printed is the one now on the policy — read back through [`stored_word`]
/// rather than echoed from the proposal — so the line cannot claim a value the store rejected or reformatted.
pub fn derived_log(derived: &Derived, policy: &Policy) -> Vec<String> {
    derived
        .applied
        .iter()
        .enumerate()
        .filter(|(_, (_, outcome))| matches!(outcome, Applied::Set))
        .map(|(index, (field, _))| {
            let because = derived
                .proposals
                .get(index)
                .map(|proposal| proposal.because.clone())
                .filter(|reason| !reason.is_empty())
                .unwrap_or_else(|| "no reason given".to_string());
            format!(
                "policy: {field} = {} \u{2014} {because}",
                stored_word(policy, field)
            )
        })
        .collect()
}

/// The value a policy holds for one field, spelled the way §10 spells it: enums by their words, switches
/// as on/off, a length as its seconds (`0` is §10's "none"). The log prints it (S4) and `get_context`
/// offers the same spelling to the model, so neither depends on a screen existing.
pub fn stored_word(policy: &Policy, field: &str) -> String {
    match field {
        "markingPass" => {
            let word = match policy.marking_pass.value {
                MarkingPass::Joins => "joins",
                MarkingPass::Retakes => "retakes",
                MarkingPass::None => "none",
            };
            word.to_string()
        }
        "cutMode" => {
            let word = match policy.cut_mode.value {
                CutMode::Words => "words",
                CutMode::Model => "model",
            };
            word.to_string()
        }
        "captionsPass" => pass_word(policy.captions_pass.value).to_string(),
        "speedPass" => pass_word(policy.speed_pass.value).to_string(),
        "decorationsPass" => pass_word(policy.decorations_pass.value).to_string(),
        "targetLengthSeconds" => format!("{}", policy.target_length_seconds.value),
        // A name outside the catalogue has no stored shape to print; an empty value is a visible gap in the
        // line rather than a crash, and `derived_log` can only ever be handed a catalogue field.
        _ => String::new(),
    }
}

fn pass_word(on: bool) -> &'static str {
    if on {
        "on"
    } else {
        "off"
    }
}

/// Which marks a changed `markingPass` throws away (§03's closing paragraph)
/// the marked text they were written into.
///
/// Pure — this answers the question, the caller drops the files. It follows §00's resume-marker rule:
/// the file's existence IS the marker, so invalidating means the marker goes and Prepare's marking
/// pass runs again rather than resuming off output made under a different pass.
pub fn invalidated_marks(before: &Policy, after: &Policy) -> Vec<&'static str> {
    if invalidated_by_marking_change(before.marking_pass.value, after.marking_pass.value) {
        vec!["retakes.tsv", "final.txt"]
    } else {
        Vec::new()
    }
}
