//! F0.7 (Derive the editing policy) — `spec/03-shell.md` F0.7.
//!
//! The User Context is prose; the pipeline runs on a few switches. This module translates a context into a [`Policy`], validates what a model proposes
//! against the catalogue, refuses what it does not know, and answers which flows the result turns on. The derivation request is built here ([`request`])
//! but sent by the run layer, so nothing here needs a server or display. Models propose, the machine places; a refusal names the field and says nothing of fields accepted alongside (S3).

use crate::project::{CutMode, Field, MarkingPass, Origin, Policy};
use crate::prompts;
use crate::tool_loop;
use crate::tools::Tool;

/// The prompt the derivation asks with; its thinking flag is read from [`prompts`] (`prompts::thinking("policy") == false`).
pub const PROMPT: &str = "policy";

/// What the model put forward through `tool:set_policy`: field, value as typed, and reason; strings as they arrive over the wire.
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
    pub fn status(&self) -> String {
        match self {
            Self::Set => "set".to_string(),
            Self::Refused(say) => say.clone(),
        }
    }
}

/// Is this context empty enough to skip the model? Whitespace counts as empty.
pub fn is_empty(context: &str) -> bool {
    context.trim().is_empty()
}

/// One row of the catalogue `tool:set_policy` validates against: field name, `P.policy.*` id, accepted values.
///
/// An empty `values` list is a NUMBER: `policy::apply` reads it as seconds (`P.policy.targetLengthSeconds`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CatalogueRow {
    pub field: &'static str,
    pub id: &'static str,
    pub values: &'static [&'static str],
}

/// The seven fields F0.7 derives, in the order the log says them; S3 validates against this list.
pub fn derived_fields() -> [CatalogueRow; 7] {
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
        CatalogueRow {
            field: "targetLengthSeconds",
            id: "P.policy.targetLengthSeconds",
            values: &[],
        },
        // P.policy.language — the recordings' language code. Open-ended, so no `values`: `apply` checks the shape of a code.
        CatalogueRow {
            field: "language",
            id: "P.policy.language",
            values: &[],
        },
    ]
}

/// A language code as the recogniser and voice take it: two or three lower-case letters (ISO 639).
fn is_language_code(code: &str) -> bool {
    (2..=3).contains(&code.len()) && code.chars().all(|c| c.is_ascii_lowercase())
}

/// The accepted switch spellings; `true`/`false` too, because JSON booleans arrive unquoted.
const ON_OFF: &[&str] = &["on", "off", "true", "false"];

fn lookup(field: &str) -> Option<CatalogueRow> {
    // A model reading the list may send a field's id for its name: both say the one field.
    let field = field.trim();
    derived_fields()
        .into_iter()
        .find(|row| row.field == field || row.id == field)
}

/// S3: validate one proposal and store it if it passes.
/// Two refusals, each naming what was wrong: an unknown field (the catalogue is closed) and a value outside the field's range.
/// Neither touches the policy, so landed proposals stay (failure is local).
///
/// §F0.7 has no hand-set precedence: a valid proposal always stores, whatever `source` the field carries (an old
/// `naivepost.json` may say `"source": "user"`; the next derivation overwrites it).
pub fn apply(policy: &mut Policy, proposal: &Proposal) -> Applied {
    if proposal.field.trim().is_empty() {
        return Applied::Refused(format!(
            "that set_policy named no field \u{2014} the fields are {}",
            derived_fields()
                .iter()
                .map(|r| r.field)
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
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
        // A number row: the value is seconds as text; a non-number is refused, naming field and value.
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
        "language" if is_language_code(&proposal.value) => {
            policy.language = Field {
                value: proposal.value.clone(),
                origin: Origin::Model,
                because,
            }
        }
        "language" => {
            return Applied::Refused(format!(
                "{} is not a language code for {} \u{2014} send its two-letter code, lower case: en, de, fr",
                proposal.value, row.field
            ))
        }
        // Unreachable: `lookup` returned this row from the same catalogue; reaching it means this match drifted from it.
        _ => unreachable!("catalogue row {} has no writer", row.field),
    }
    Applied::Set
}

/// The value half of S3 for the two enums; range was already checked, so an unmatched string is the drift the `unreachable!` guards.
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

/// What the derivation sends: tools, thinking off, and the prompt key. Built, never sent; the run layer owns the HTTP.
#[derive(Debug, Clone, PartialEq)]
pub struct DerivationRequest {
    /// `policy`, the prompt S2 names. Its wording ships in `spec/prompts/policy.md`.
    pub prompt: &'static str,
    /// Thinking off, read from [`prompts::thinking`] so it cannot disagree with the table.
    pub thinking: bool,
    pub tools: Vec<Tool>,
    pub context: String,
}

/// S2: build the derivation request for this context.
///
/// Tools come from [`crate::tools::policy_tools`] (§3.11) with schemas through [`tool_loop::schemas`]; thinking is read from the prompt table.
pub fn request(context: &str) -> DerivationRequest {
    DerivationRequest {
        prompt: PROMPT,
        thinking: prompts::thinking(PROMPT),
        tools: crate::tools::policy_tools(),
        context: context.to_string(),
    }
}

impl DerivationRequest {
    pub fn tool_schemas(&self) -> Vec<serde_json::Value> {
        tool_loop::schemas(&self.tools, &|_| None)
    }
}

// ---- the consequences: which flows the policy turns on ---------------------------------------

/// Which cut-stage pass a switch governs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pass {
    /// F3.9 — captions proposed by the model after the cut.
    Captions,
    /// F3.10 — speeds proposed by the model.
    Speeds,
    /// F3.11 — decorations proposed by the model.
    Decorations,
}

/// The pipeline fields decide which flows run (§03); these readers keep the switch-to-flow mapping in one file.
pub fn marking_pass_of(policy: &Policy) -> MarkingPass {
    policy.marking_pass.value
}

/// A cut mode nobody chose follows the marking pass: a scripted recording's joins are repaired from its words, so
/// its cut comes from them too. An answer that named a mode, or a person's, stands.
pub fn cut_mode_of(policy: &Policy) -> CutMode {
    if policy.cut_mode.origin == crate::project::Origin::Default
        && policy.marking_pass.value == MarkingPass::Joins
    {
        return CutMode::Words;
    }
    policy.cut_mode.value
}

/// Whether this cut-stage pass is called at all; off means never asked, not asked-and-ignored.
pub fn pass_runs(policy: &Policy, pass: Pass) -> bool {
    match pass {
        Pass::Captions => policy.captions_pass.value,
        Pass::Speeds => policy.speed_pass.value,
        Pass::Decorations => policy.decorations_pass.value,
    }
}

/// Does changing `markingPass` invalidate the existing marks?
///
/// True when the two differ: `retakes.tsv` and `final.txt` hold the previous pass's output, which a different pass would read as its own.
/// A pure question; the run layer drops the files ([`crate::layout::Tree::final_txt`], [`crate::layout::Tree::retakes_tsv`]).
///
/// Same value is false: re-deriving an agreeing policy must not throw away correct work.
pub fn invalidated_by_marking_change(old: MarkingPass, new: MarkingPass) -> bool {
    old != new
}

/// Whether the policy still matches the context it was derived from.
///
/// A never-derived project needs one even with an empty context: S1's defaults are an answer worth having on record.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Tracker {
    derived_from: Option<String>,
}

impl Tracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Does `context` need a derivation? True when none has happened or the text differs from the last one read, byte-for-byte.
    pub fn needs_derive(&self, context: &str) -> bool {
        self.derived_from.as_deref() != Some(context)
    }

    /// Record that `context` has been derived from.
    pub fn mark_derived(&mut self, context: &str) {
        self.derived_from = Some(context.to_string());
    }

    /// Forget what was derived; used when a project is opened or replaced.
    pub fn forget(&mut self) {
        self.derived_from = None;
    }
}

/// What one derivation did.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Derived {
    /// The model could not be reached, so this context is still not derived and the next ▶ asks again.
    pub model_fault: bool,
    /// S1: the context was empty, so the defaults stand and the model was never asked.
    pub used_defaults: bool,
    /// §12 entry 21: whether a target length was READ from the User Context and stored; false keeps "no target" a said fact.
    pub target_read: bool,
    /// §12 entry 21: a length was mentioned but could not be spelled in seconds, so the gap is said rather than silent (see [`target_length_log`]).
    pub target_gap: bool,
    /// Each accepted proposal, by field name (S3).
    pub applied: Vec<(&'static str, Applied)>,
    /// Each proposal that LANDED, in order, index-aligned with `applied` (refusals are in neither); S4's log reads value and `because` off these.
    pub proposals: Vec<Proposal>,
    /// Every refusal the loop collected, oldest first — the sentences the model was handed back.
    pub refusals: Vec<String>,
}

impl Derived {
    /// The status-bar line for a whole derivation; a refusal is said out loud.
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
/// Bare prefix (`machine.deriveRounds`): §03 spells the loop without a bound, and an unbounded one would hang on a model inventing a field.
/// Three gives two chances to correct a refusal.
pub const DERIVE_ROUNDS: u32 = 3;

/// S1-S4 in one call. `answer` is the model, handed the request and the refusals so far; injected so the flow is testable without a server.
///
/// An empty context never calls `answer` (S1). Each round runs every proposal through [`apply`]; refusals are fed back into the next round,
/// accepted values stay put.
pub fn derive(
    policy: &mut Policy,
    context: &str,
    answer: &mut dyn FnMut(&DerivationRequest, &[String]) -> Vec<Proposal>,
) -> Derived {
    if is_empty(context) {
        // S1: the defaults are the answer.
        return Derived {
            used_defaults: true,
            ..Derived::default()
        };
    }
    let mut out = read_target_length(policy, context);
    read_language(policy, context, &mut out);
    let req = request(context);
    for _ in 0..DERIVE_ROUNDS {
        let proposals = answer(&req, &out.refusals);
        // A round that could not be dialled says so itself: its sentence goes straight into the refusals (failure is specific and local).
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
            out.model_fault = true;
            // One dead model is one fault: asking again would spend §3's patience ladder per round.
            break;
        }
        let before = out.refusals.len();
        for proposal in proposals {
            match apply(policy, &proposal) {
                Applied::Refused(say) => out.refusals.push(say),
                other => {
                    // The `'static` catalogue name lets a test assert per-field outcomes; only landed proposals are recorded, keeping the two lists index-aligned.
                    let name = lookup(&proposal.field)
                        .map(|row| row.field)
                        .unwrap_or("unknown");
                    out.proposals.push(proposal.clone());
                    out.applied.push((name, other));
                }
            }
        }
        if out.refusals.len() == before {
            // Nothing refused this round: the policy is settled.
            break;
        }
    }
    out
}

/// §12 entry 21: read `P.policy.targetLengthSeconds` out of the User Context's own words and store it.
/// Runs through [`apply`], so it passes the same validation. The log says which happened: read and stored ([`derived_log`]), mentioned but unread ([`target_length_log`]), or nothing mentioned.
pub fn read_target_length(policy: &mut Policy, context: &str) -> Derived {
    // Not the script: "stolen in about 20 minutes" is the news, not how long the video should run.
    let reading =
        crate::policy_length::target_length_of(&crate::prompt_assembly::without_script(context));
    let mut out = Derived::default();
    // A length mentioned but not spellable in seconds is still logged (§12), so `mentioned` proceeds even with 0 s; no mention stops here.
    if !reading.mentioned {
        return out;
    }
    // The seconds go through the same door as a model's answer, so validation still applies.
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

/// `P.policy.language` out of the User Context's own words ("The talk is in German"), so the session's language does not hang on a model being reachable.
/// Not the script, which may quote any language; a context that does not say leaves the field alone, and the model may still set it.
fn read_language(policy: &mut Policy, context: &str, out: &mut Derived) {
    let Ok(said) = regex::Regex::new(
        r"(?i)\b(?:talk|lecture|video|session|speech|recordings?)\s+(?:is|are)\s+(?:spoken\s+)?in\s+(English|German|French)\b",
    ) else {
        return;
    };
    let words = crate::prompt_assembly::without_script(context);
    let Some(name) = said.captures(&words).and_then(|found| found.get(1)) else {
        return;
    };
    let proposal = Proposal {
        field: "language".to_string(),
        value: crate::roles::language_code(name.as_str()),
        because: "the User Context's own words".to_string(),
    };
    let landed = apply(policy, &proposal);
    out.proposals.push(proposal);
    out.applied.push(("language", landed));
}

/// The extra line one target-length read earns — only the cases [`derived_log`] cannot say.
///
/// A stored length already has `policy: targetLengthSeconds = <seconds> — <because>`; what is left to say is the refusal and the gap.
pub fn target_length_log(derived: &Derived) -> Option<String> {
    if let Some(Applied::Refused(say)) = derived
        .applied
        .iter()
        .find(|(field, _)| *field == "targetLengthSeconds")
        .map(|(_, outcome)| outcome.clone())
    {
        return Some(format!("!!! policy: {say}"));
    }
    // A landed read is said by `derived_log`; only mentioned-but-unread needs saying here.
    derived.target_gap.then(|| {
        "policy: a length was mentioned in the User Context but could not be read \
         \u{2014} the target length stays none"
            .to_string()
    })
}

/// What a derivation could not settle, each said once. Silence here read as a policy the model chose.
pub fn refused_log(derived: &Derived) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for refusal in &derived.refusals {
        let line = format!("!!! policy: {refusal} \u{2014} that field keeps its default");
        if !lines.contains(&line) {
            lines.push(line);
        }
    }
    lines
}

/// S4: one line per value this derivation STORED: field, value and `because`, nothing else.
///
/// Only `Applied::Set` earns a line. The value printed is read back through [`stored_word`], not echoed from the proposal.
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

/// The value a policy holds for one field, spelled as §10 does: enums by words, switches as on/off, a length in seconds (`0` is none).
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
        "language" => policy.language.value.clone(),
        // Outside the catalogue: an empty value is a visible gap in the line, not a crash.
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

/// Which marks a changed `markingPass` throws away (§03's closing paragraph).
///
/// Pure: the caller drops the files. The file's existence IS the resume marker (§00), so invalidating makes the marking pass run again.
pub fn invalidated_marks(before: &Policy, after: &Policy) -> Vec<&'static str> {
    if invalidated_by_marking_change(before.marking_pass.value, after.marking_pass.value) {
        vec!["retakes.tsv", "final.txt"]
    } else {
        Vec::new()
    }
}
