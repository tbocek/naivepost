//! P.machine.maxProposedEffects — the cap on effects per cut reply.
//!
//! §10's row: default 1000, "cap on effects per cut reply (effectively none: the prompt decides)",
//! prototype constant `fxMaxProposed`. The tests below check the number, what counts against it, and
//! that the catalogue holds exactly one row for the id.
//!
//! The exemption half is not this parameter's own rule but 05-cut#8's, reached through
//! [`naivepost::cut_suggest::exempt_from_decorations_cap`]: a rate the cut asked for decides how long
//! the segment runs, so it is not decoration and is not charged.

use naivepost::cut::{EffectKind, Fx};
use naivepost::cut_effects_pass as pass;
use naivepost::cut_suggest;
use naivepost::params;

fn fx(kind: &str, t: f64, dur: f64) -> Fx {
    Fx { kind: kind.into(), t, dur, ..Default::default() }
}

/// Look an id up across every catalogue section and insist on exactly one row.
fn row(id: &str) -> params::Param {
    let mut found = params::prepare()
        .into_iter()
        .chain(params::cut())
        .chain(params::effects())
        .chain(params::narrate())
        .chain(params::produce())
        .filter(|row| row.id == id)
        .collect::<Vec<_>>();
    assert_eq!(found.len(), 1, "{id} catalogued {} times", found.len());
    found.pop().unwrap()
}

// --- S1: the number -------------------------------------------------------------------------------------

#[test]
fn p_machine_maxproposedeffects_s1_the_cap_is_a_thousand() {
    // P.machine.maxProposedEffects = 1000, the prototype's fxMaxProposed.
    assert_eq!(pass::MAX_PROPOSED_EFFECTS, 1000);
    // Inclusive at the boundary, rejected one over: the cap means "<= cap", not "< cap".
    assert!(pass::effect_cap_allows(pass::MAX_PROPOSED_EFFECTS));
    assert!(!pass::effect_cap_allows(pass::MAX_PROPOSED_EFFECTS + 1));
    // Zero is allowed: an empty reply has never been a problem.
    assert!(pass::effect_cap_allows(0));
}

// --- S2: what the cap can see ---------------------------------------------------------------------------

#[test]
fn p_machine_maxproposedeffects_s2_a_whole_segment_rate_is_exempt() {
    // A speed is exempt whether it is a rate or a stop (rate 0 is the same kind), per 05-cut#8.
    assert_eq!(pass::counts_against_effect_cap(EffectKind::Speed), 0);
    // Everything the decoration pass actually adds on top is charged one.
    for kind in [
        EffectKind::Zoom,
        EffectKind::Text,
        EffectKind::Svg,
        EffectKind::Volume,
        EffectKind::Label,
    ] {
        assert_eq!(pass::counts_against_effect_cap(kind), 1, "{kind:?}");
    }
    // Same source of truth: this cannot drift from the exemption helper it delegates to.
    for kind in [
        EffectKind::Zoom,
        EffectKind::Speed,
        EffectKind::Text,
        EffectKind::Svg,
        EffectKind::Volume,
        EffectKind::Label,
    ] {
        let expected = if cut_suggest::exempt_from_decorations_cap(kind) { 0 } else { 1 };
        assert_eq!(pass::counts_against_effect_cap(kind), expected, "{kind:?}");
    }
}

// --- S3: counting a list --------------------------------------------------------------------------------

#[test]
fn p_machine_maxproposedeffects_s3_the_count_ignores_exempt_kinds() {
    let mixed = vec![
        fx("speed", 1.0, 4.0),
        fx("zoom", 2.0, 3.0),
        fx("text", 3.0, 3.0),
        fx("volume", 4.0, 3.0),
        fx("label", 5.0, 0.0),
    ];
    assert_eq!(pass::proposed_effect_count(&mixed), 4, "the speed cost nothing");

    let all_speeds = vec![fx("speed", 1.0, 4.0), fx("speed", 6.0, 2.0)];
    assert_eq!(pass::proposed_effect_count(&all_speeds), 0);

    assert_eq!(pass::proposed_effect_count(&[]), 0);

    // An unrecognised kind is charged rather than waved through: a cap that cannot see a kind is no cap.
    let unknown = vec![fx("blur", 1.0, 3.0)];
    assert_eq!(pass::proposed_effect_count(&unknown), 1);
    // And the unknown really is unknown to the enum, so this is not secretly a known kind.
    assert!(unknown[0].effect_kind().is_none());
}

// --- S4: a normal reply passes, a runaway does not --------------------------------------------------------

#[test]
fn p_machine_maxproposedeffects_s4_a_normal_reply_passes_and_a_runaway_is_bounded() {
    // What the prompt asks for — three or four across five minutes — is miles under the bound.
    let deliberate = vec![
        fx("zoom", 10.0, 3.0),
        fx("text", 40.0, 3.0),
        fx("volume", 90.0, 3.0),
    ];
    assert!(pass::effect_cap_allows(pass::proposed_effect_count(&deliberate)));

    // Exactly at the cap: allowed. One over: refused. Asserted against the constant, so a retune moves
    // the expectation with the number rather than leaving a stale literal behind.
    assert!(pass::effect_cap_allows(pass::MAX_PROPOSED_EFFECTS));
    assert!(!pass::effect_cap_allows(pass::MAX_PROPOSED_EFFECTS + 1));

    // A thousand zooms is the largest reply the cap admits; a thousand-and-first is where it stops.
    let at_cap = vec![fx("zoom", 1.0, 1.0); pass::MAX_PROPOSED_EFFECTS];
    assert_eq!(pass::proposed_effect_count(&at_cap), pass::MAX_PROPOSED_EFFECTS);
    assert!(pass::effect_cap_allows(pass::proposed_effect_count(&at_cap)));
    let mut over = at_cap;
    over.push(fx("zoom", 2.0, 1.0));
    assert!(!pass::effect_cap_allows(pass::proposed_effect_count(&over)));
}

// --- S5: the catalogue row ------------------------------------------------------------------------------

#[test]
fn p_machine_maxproposedeffects_s5_it_is_catalogued_once_under_its_own_id() {
    let r = row("P.machine.maxProposedEffects");
    assert_eq!(r.spelled, "1000");
    assert_eq!(r.from, "cut_effects_pass::MAX_PROPOSED_EFFECTS");
    // §10 files it under Machine: a bound on the app's reply handling, not a judgement about the video.
    assert_eq!(params::family("P.machine.maxProposedEffects"), params::Family::Machine);
    // And the row spells what the constant holds, so the two cannot be tuned apart.
    assert_eq!(r.spelled, pass::MAX_PROPOSED_EFFECTS.to_string());
}

// --- S6: one home ---------------------------------------------------------------------------------------

#[test]
fn p_machine_maxproposedeffects_s6_no_second_home_for_the_same_bound() {
    let all_ids: Vec<String> = params::prepare()
        .into_iter()
        .chain(params::cut())
        .chain(params::effects())
        .chain(params::narrate())
        .chain(params::produce())
        .map(|r| r.id.to_string())
        .collect();
    let homes: Vec<&String> = all_ids
        .iter()
        .filter(|id| id.contains("maxProposedEffects"))
        .collect();
    assert_eq!(homes.len(), 1, "one bound must have one row, found {homes:?}");
    assert_eq!(homes[0], "P.machine.maxProposedEffects");
}
