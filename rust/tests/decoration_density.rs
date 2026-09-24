//! P.policy.decorationDensity — how many decorations the effects pass is asked for.
//! `spec/10-parameters.md` row `P.policy.decorationDensity` (default "three or four per five
//! minutes", "from the effects prompt", prototype `fxRules`). The rule is wording the model reads, not
//! a number the app multiplies anything by ([`naivepost::cut_effects_pass`], F3.11).
//!
//! S1 the default as code holds it · S2 the shipped prompt says the same · S3 the band scales and never
//! asks for nothing · S4 the guide is advisory, the cap is the bound · S5 the context outranks the
//! default · S6 where the id lives.

use std::path::Path;

use naivepost::cut_effects_pass::{self as fx};
use naivepost::params::{self, Family};
use naivepost::prompts;

/// The shipped effects prompt, read from the spec tree.
fn shipped_effects() -> String {
    let file = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("spec")
        // `shipped_file` answers relative to the spec root, hence the `spec/` above.
        .join(prompts::shipped_file("effects").expect("effects has a shipped file"));
    std::fs::read_to_string(&file).unwrap_or_else(|e| panic!("{file:?}: {e}"))
}

// --- S1: the default --------------------------------------------------------------------------

#[test]
fn p_policy_decorationdensity_three_or_four_per_five_minutes() {
    // §10 spells the default "three or four per five minutes"; that is what the constants hold.
    assert_eq!(fx::DENSITY_PER_FIVE_MINUTES, (3, 4));
    assert_eq!(fx::FIVE_MINUTES_SECONDS, 300.0);
    assert!(fx::DENSITY_GUIDE.contains("three or four"), "{}", fx::DENSITY_GUIDE);
    assert!(fx::DENSITY_GUIDE.contains("five minutes"), "{}", fx::DENSITY_GUIDE);
}

// --- S2: the constant cannot drift from the shipped prompt -------------------------------------

#[test]
fn p_policy_decorationdensity_the_guide_is_the_shipped_effects_prompt() {
    let shipped = shipped_effects();
    assert!(
        shipped.contains("Few and deliberate: three or four across five minutes of finished video"),
        "the shipped prompt no longer says the default §10 records"
    );
    // And the clause the code quotes is in there verbatim, so neither can move alone.
    assert!(
        shipped.contains(fx::DENSITY_GUIDE),
        "DENSITY_GUIDE is not a clause of prompts/effects.md: {}",
        fx::DENSITY_GUIDE
    );
}

// --- S3: the band scales, and never asks for nothing -------------------------------------------

#[test]
fn p_policy_decorationdensity_the_band_scales_with_the_edit() {
    assert_eq!(fx::expected_decorations(300.0), (3, 4));
    assert_eq!(fx::expected_decorations(600.0), (6, 8));
    // A short edit still gets one: the prompt's own sentence ends "Not one on every clip, and not none".
    assert_eq!(fx::expected_decorations(60.0), (1, 1));
    assert_eq!(fx::expected_decorations(1.0).0, 1, "never zero while there is a picture");
    // Nothing rendered asks for nothing.
    assert_eq!(fx::expected_decorations(0.0), (0, 0));
    assert_eq!(fx::expected_decorations(-5.0), (0, 0));
    // Low never exceeds high at any length.
    for seconds in [30.0, 90.0, 300.0, 450.0, 1800.0] {
        let (low, high) = fx::expected_decorations(seconds);
        assert!(low <= high, "{seconds} s gives {low} > {high}");
    }
}

// --- S4: over the guide is said, not cut ------------------------------------------------------

#[test]
fn p_policy_decorationdensity_the_cap_not_the_guide_bounds_a_reply() {
    // Past the top of the band ...
    assert!(fx::above_density_guide(12, 300.0));
    // ... and still perfectly allowed by the app's bound. Being over the guide removes nothing.
    assert!(fx::effect_cap_allows(12), "the guide must not trim an edit");
    // The enforced id is the runaway-reply bound: P.machine.maxProposedEffects
    assert!(fx::effect_cap_allows(fx::MAX_PROPOSED_EFFECTS));
    assert!(!fx::effect_cap_allows(fx::MAX_PROPOSED_EFFECTS + 1));
    // At the top of the band it is not "above" the guide either way.
    assert!(!fx::above_density_guide(4, 300.0));
    assert!(fx::above_density_guide(5, 300.0));
}

// --- S5: the User Context outranks the default -------------------------------------------------

#[test]
fn p_policy_decorationdensity_the_context_outranks_the_default() {
    let shipped = shipped_effects();
    assert!(shipped.contains("the USER CONTEXT outranks it"), "prompt changed shape");
    assert!(shipped.contains("Asked for more, write more"), "prompt changed shape");
    // So the value is a guide rather than a limit: fifty decorations is far over the band for a
    // five-minute edit, yet nothing refuses them — the cap is nowhere near.
    assert!(fx::above_density_guide(50, 300.0));
    assert!(fx::effect_cap_allows(50), "the density must not act as a cap");
}

// --- S6: where the id lives -------------------------------------------------------------------

#[test]
fn p_policy_decorationdensity_family_is_policy_and_no_row_exists() {
    // The family resolves from the prefix even though the id has no row.
    assert_eq!(params::family("P.policy.decorationDensity"), Family::Policy);
    // No section carries it: effect_parameters.rs s5 pins that the density is prompt wording, not a
    // value the app multiplies anything by, and asserts no catalogued id contains "density". Adding a
    // row here would break that test rather than improve this one.
    let rows = params::prepare()
        .into_iter()
        .chain(params::cut())
        .chain(params::effects())
        .chain(params::narrate())
        .chain(params::produce());
    assert_eq!(
        rows.filter(|row| row.id == "P.policy.decorationDensity")
            .count(),
        0,
        "the density is deliberately unrowed; see DENSITY_GUIDE's doc comment"
    );
}
