//! P.policy.keepSwearing — the captions pass's cleaning rule about the speaker's own words.
//! `spec/10-parameters.md` row `P.policy.keepSwearing` (default true, "caption cleaning rule (from
//! the prompt)", prototype `captionSystem`). The rule is wording the model reads, not arithmetic the
//! app applies ([`naivepost::cut_captions`], F3.9).
//!
//! S1 the default · S2 on keeps the words · S3 off drops only that promise · S4 the constant matches
//! the shipped prompt · S5 one catalogue row · S6 the user's own words still reach the model.

use std::path::Path;

use naivepost::cut_captions;
use naivepost::params::{self, Family};
use naivepost::project;

// --- S1: the default --------------------------------------------------------------------------

#[test]
fn p_policy_keepswearing_s1_kept_by_default() {
    // P.policy.keepSwearing = true: the speaker's words stay as spoken.
    assert!(cut_captions::KEEP_SWEARING_DEFAULT);
    let policy = project::Policy::default();
    assert!(policy.keep_swearing.value);
    assert_eq!(policy.keep_swearing.origin, project::Origin::Default);
    assert_eq!(params::family("P.policy.keepSwearing"), Family::Policy);
}

// --- S2: on, the rule says the swearing stays --------------------------------------------------

#[test]
fn p_policy_keepswearing_s2_on_promises_the_words_stay() {
    let rule = cut_captions::cleaning_rule(true);
    assert!(rule.contains("the swearing kept"), "{rule}");
    assert!(rule.contains("subtitler"), "{rule}");
    // And it still cleans what a subtitler cleans — the toggle is one clause of this sentence.
    assert!(rule.contains("no stutters"), "{rule}");
    assert!(rule.contains("no repeated words"), "{rule}");
    assert!(rule.contains("sentence case"), "{rule}");
}

// --- S3: off, only that promise goes ----------------------------------------------------------

#[test]
fn p_policy_keepswearing_s3_off_drops_only_the_promise() {
    // §10 gives no censor style, so "off" is the app no longer telling the model the words stay —
    // not a bleep scheme invented in the app.
    let rule = cut_captions::cleaning_rule(false);
    assert!(!rule.contains("swear"), "{rule}");
    assert!(!rule.contains("kept"), "{rule}");
    // Everything else about cleaning the words is untouched.
    assert!(rule.contains("no stutters"), "{rule}");
    assert!(rule.contains("no repeated words"), "{rule}");
    assert!(rule.contains("sentence case"), "{rule}");
    assert!(rule.contains("subtitler"), "{rule}");
    // Off is strictly shorter: one clause was removed, nothing added.
    assert!(rule.len() < cut_captions::cleaning_rule(true).len());
}

// --- S4: the constant cannot drift from the shipped prompt -------------------------------------

#[test]
fn p_policy_keepswearing_s4_the_rule_is_the_shipped_captions_prompt() {
    // The prompt key whose wording this policy rides in.
    let key = "captions";
    assert!(params::family("P.policy.keepSwearing") == Family::Policy);
    let file = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("spec")
        // `shipped_file` answers relative to the spec tree root, hence the `spec/` above.
        .join(naivepost::prompts::shipped_file(key).expect("captions has a shipped file"));
    let shipped = std::fs::read_to_string(&file).unwrap_or_else(|e| panic!("{file:?}: {e}"));
    // Both halves of the sentence appear in the shipped prompt and in the constant the code holds.
    for clause in ["the swearing kept", "no repeated words, sentence case"] {
        assert!(shipped.contains(clause), "shipped prompt lacks {clause:?}");
        assert!(
            cut_captions::CLEANING_RULE.contains(clause),
            "CLEANING_RULE lacks {clause:?}"
        );
    }
}

// --- S5: one row, one home --------------------------------------------------------------------

#[test]
fn p_policy_keepswearing_s5_catalogued_once_spelled_true() {
    // Same shape as effect_parameters.rs's `anywhere`: across the three lists, exactly one match.
    let mut found = params::prepare()
        .into_iter()
        .chain(params::cut())
        .chain(params::effects())
        .filter(|row| row.id == "P.policy.keepSwearing")
        .collect::<Vec<_>>();
    assert_eq!(found.len(), 1, "P.policy.keepSwearing catalogued {} times", found.len());
    let row = found.pop().unwrap();
    // Spelled as §10 spells its booleans, read from the project's own default rather than retyped.
    assert_eq!(row.spelled, "true");
    assert!(row.from.contains("keep_swearing"), "{}", row.from);
}

// --- S6: what the model actually reads --------------------------------------------------------

#[test]
fn p_policy_keepswearing_s6_the_context_reaches_the_request() {
    // The policy form records the choice; the User Context is what the captions request carries. Both
    // paths have to agree, so the context line must survive into the message verbatim.
    let msg = cut_captions::message(
        "keep the swearing",
        &[(1, 10.0)],
        &[(1, 0.5, "what was said over the clip")],
    );
    assert!(msg.contains("keep the swearing"), "{msg}");
    assert!(msg.contains(cut_captions::CLIPS_HEADER), "{msg}");
    assert!(msg.contains("[+0.5s] what was said over the clip"), "{msg}");
}
