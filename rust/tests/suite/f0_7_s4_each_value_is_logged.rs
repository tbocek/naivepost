//! F0.7 S4 — "Each stored value is said in the log with its field, value and `because`".
//!
//! §F0.7 ends without a form: the log is where what the model decided becomes visible, so the line itself is
//! the requirement. Each line must carry all three parts (the field, the value that was stored, the reason the
//! model gave) and only `Set` outcomes count — a refusal was already said out loud, and a field that did not
//! change must not be reported as if it had. No window and no server: this drives [`policy::derive`] through
//! the same injected-answer seam `policy_derive.rs` uses and reads the lines back.

use naivepost::policy::{self, Proposal};
use naivepost::project::{CutMode, MarkingPass, Policy};

/// A proposal as it arrives over `tool:set_policy`: three strings, unvalidated.
fn proposal(field: &str, value: &str, because: &str) -> Proposal {
    Proposal {
        field: field.to_string(),
        value: value.to_string(),
        because: because.to_string(),
    }
}

/// Run a derivation whose model answers exactly these proposals on its first round.
fn derive_with(policy: &mut Policy, rounds: Vec<Vec<Proposal>>) -> policy::Derived {
    let rounds = std::cell::RefCell::new(rounds.into_iter());
    policy::derive(
        policy,
        "cut on the words, keep every pass on",
        &mut |_, _| rounds.borrow_mut().next().unwrap_or_default(),
    )
}

#[test]
fn f0_7_s4_a_stored_field_is_said_with_its_value_and_because() {
    let mut policy = Policy::default();
    let derived = derive_with(
        &mut policy,
        vec![vec![
            proposal("markingPass", "joins", "the seams are what matter here"),
            proposal("cutMode", "words", "no model needed for this cut"),
        ]],
    );
    assert_eq!(
        derived.applied.len(),
        2,
        "both proposals were valid, so both should have landed: {derived:?}"
    );
    let lines = policy::derived_log(&derived, &policy);
    assert_eq!(lines.len(), 2, "one line per stored field, got {lines:?}");
    // Each part of S4's sentence, on the line for its own field.
    assert!(
        lines[0].contains("markingPass") && lines[0].contains("joins"),
        "the first line must name the field and the stored value: {:?}",
        lines[0]
    );
    assert!(
        lines[0].contains("the seams are what matter here"),
        "the line carries the `because` the model sent: {:?}",
        lines[0]
    );
    assert!(
        lines[1].contains("cutMode")
            && lines[1].contains("words")
            && lines[1].contains("no model needed for this cut"),
        "so does the second: {:?}",
        lines[1]
    );
    // The value on the line is the value that was STORED, not an echo of whatever was typed: the two enums
    // spell the same way here, so say it with the state as well as the string.
    assert_eq!(policy.marking_pass.value, MarkingPass::Joins);
    assert_eq!(policy.cut_mode.value, CutMode::Words);
}

#[test]
fn f0_7_s4_a_switch_lands_as_on_or_off_in_the_line() {
    // P.policy.captionsPass is a switch, and §10 spells its values on/off — a log line reading `false` or
    // `0` would not match how the parameter is written anywhere else in the spec.
    let mut policy = Policy::default();
    let derived = derive_with(
        &mut policy,
        vec![vec![proposal(
            "captionsPass",
            "off",
            "the lecture is captioned already",
        )]],
    );
    let lines = policy::derived_log(&derived, &policy);
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(
        lines[0].contains("captionsPass") && lines[0].contains("off"),
        "a switch is said as on/off: {lines:?}"
    );
    assert!(lines[0].contains("the lecture is captioned already"));
}

#[test]
fn f0_7_s4_a_refused_field_is_not_logged_as_stored() {
    // Round one proposes a value outside the catalogue's range; round two fixes it. Only the fixed one may
    // be said as stored — and it is said with the SECOND round's reason, since that is the proposal that landed.
    let mut policy = Policy::default();
    let derived = derive_with(
        &mut policy,
        vec![
            vec![proposal("markingPass", "sideways", "I insist")],
            vec![proposal("markingPass", "none", "there is nothing to mark")],
        ],
    );
    assert_eq!(derived.refusals.len(), 1, "{derived:?}");
    let lines = policy::derived_log(&derived, &policy);
    assert_eq!(
        lines.len(),
        1,
        "the refused proposal earns no stored line: {lines:?}"
    );
    assert!(
        !lines.iter().any(|line| line.contains("sideways")),
        "a refused value must not appear as stored: {lines:?}"
    );
    assert!(
        lines[0].contains("none") && lines[0].contains("there is nothing to mark"),
        "the accepted proposal is the one said: {lines:?}"
    );
}

#[test]
fn f0_7_s4_an_empty_context_logs_nothing_because_nothing_was_set() {
    // S1 stops before any proposal exists, so there is no field, value or because to say. The defaults stand
    // untouched and the log stays silent about them rather than claiming decisions nobody made.
    let mut policy = Policy::default();
    let derived = policy::derive(&mut policy, "   \n ", &mut |_, _| Vec::new());
    assert!(derived.used_defaults, "{derived:?}");
    assert!(
        policy::derived_log(&derived, &policy).is_empty(),
        "a defaults-only derivation has nothing to report"
    );
}
