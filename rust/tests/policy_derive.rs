//! F0.7 (Derive the editing policy) — the rules, with no window and no server in sight.
//!
//! What is pinned here is the SHAPE of what gets stored, not only which value won: a derived field has
//! to arrive with `source: model` AND its `because`, because S5's whole point is that a proposal can be
//! judged rather than merely read. Every assertion below therefore round-trips the real
//! [`project::Policy`] through `serde_json` and reads the JSON back, so a reason dropped on the way to
//! disk fails this test even when the in-memory value looks right.

use naivepost::policy::{self, Applied, DerivationRequest, Pass, Proposal};
use naivepost::project::{self, CutMode, MarkingPass, Origin, Policy};

/// A proposal as it arrives over `tool:set_policy`: three strings, unvalidated.
fn proposal(field: &str, value: &str, because: &str) -> Proposal {
    Proposal {
        field: field.to_string(),
        value: value.to_string(),
        because: because.to_string(),
    }
}

/// Serialise a policy and hand back the JSON for one field, so assertions read the STORED shape.
fn stored(policy: &Policy, field: &str) -> serde_json::Value {
    let all = serde_json::to_value(policy).expect("a Policy serialises");
    all.get("policy")
        .cloned()
        .unwrap_or(all)
        .get(field)
        .cloned()
        .unwrap_or(serde_json::Value::Null)
}

// ---- S1: empty context → every field default -------------------------------------------------

#[test]
fn f0_7_s1_an_empty_context_leaves_every_field_at_its_default() {
    // The prototype's blank-project behaviour: retakes marking, model cut, every pass on. Returned as a
    // whole Policy because with no context there is nothing to derive — the caller stops here.
    let defaults = policy::defaults();
    assert_eq!(defaults.marking_pass.value, MarkingPass::Retakes); // P.policy.markingPass
    assert_eq!(defaults.cut_mode.value, CutMode::Model); // P.policy.cutMode
    assert!(defaults.captions_pass.value); // P.policy.captionsPass
    assert!(defaults.speed_pass.value); // P.policy.speedPass
    assert!(defaults.decorations_pass.value); // P.policy.decorationsPass

    // And each carries `default` as its source, not `model`: nothing was derived, so nothing may claim
    // to have been. This is what lets a later derivation still write them.
    for origin in [
        defaults.marking_pass.origin,
        defaults.cut_mode.origin,
        defaults.captions_pass.origin,
        defaults.speed_pass.origin,
        defaults.decorations_pass.origin,
    ] {
        assert_eq!(origin, Origin::Default, "an undriven field reports Default, not Model");
    }
}

#[test]
fn f0_7_s1_whitespace_counts_as_no_context_so_the_derivation_is_skipped() {
    // A context of spaces is the same absence of information; asking a model to derive a style from it
    // invents one out of nothing.
    assert!(policy::is_empty(""));
    assert!(policy::is_empty("   \n\t "));
    assert!(!policy::is_empty("A twelve-minute lecture on bouldering"), "real prose is not empty");
}

// ---- S2: the request shape -------------------------------------------------------------------

#[test]
fn f0_7_s2_the_request_offers_only_get_context_and_set_policy_with_thinking_off() {
    let request: DerivationRequest = policy::request("The weekly blockchain lecture");
    assert_eq!(request.prompt, "policy", "S2 names the prompt \"policy\"");
    assert!(
        !request.thinking,
        "thinking off — read from prompts::thinking, the one table of the thirteen keys' flags"
    );

    // Exactly the two tools §3.11 gives this flow: the context and the fields it may set, then the setter.
    let names: Vec<String> = request.tools.iter().map(|tool| tool.name().to_string()).collect();
    assert_eq!(
        names,
        vec!["get_context".to_string(), "set_policy".to_string()],
        "// tool:get_context · tool:set_policy — nothing else is offered, so the model cannot reach a tool \
         this flow never sanctioned"
    );
    assert_eq!(request.context, "The weekly blockchain lecture", "the context itself is the user turn");
}

#[test]
fn f0_7_s2_the_tool_schemas_come_from_the_loops_own_builder() {
    let request = policy::request("anything");
    let schemas = request.tool_schemas();
    assert_eq!(schemas.len(), 2, "one schema per offered tool");
    for schema in &schemas {
        assert_eq!(schema["type"], "function", "{schema}");
        let name = schema["function"]["name"].as_str().unwrap_or_default();
        assert!(
            name == "get_context" || name == "set_policy",
            "no other tool reaches the wire: {name}"
        );
    }
}

// ---- S3: validation, storage with the reason --------------------------------------------------

#[test]
fn f0_7_s3_a_valid_proposal_is_stored_as_model_with_its_because() {
    let mut policy = Policy::default();
    let outcome = policy::apply(
        &mut policy,
        &proposal("markingPass", "joins", "the footage was shot in one take per slide"),
    );
    assert_eq!(outcome, Applied::Set);
    assert_eq!(policy.marking_pass.value, MarkingPass::Joins); // P.policy.markingPass
    assert_eq!(policy.marking_pass.origin, Origin::Model);

    // The stored shape, camelCase key + value/source/because, exactly what the form reads back.
    let json = serde_json::to_string(&policy).expect("serialises");
    assert!(json.contains("\"markingPass\""), "{json}");
    let stored = stored(&policy, "markingPass");
    assert_eq!(stored["value"], "joins", "{stored}");
    assert_eq!(stored["source"], "model", "{stored}");
    assert_eq!(
        stored["because"], "the footage was shot in one take per slide",
        "the reason survives the save; without it S5 shows a value nobody can judge"
    );
}

#[test]
fn f0_7_s3_a_switch_takes_on_or_off_and_either_json_spelling() {
    // P.policy.captionsPass / speedPass / decorationsPass accept both spellings: an unquoted JSON
    // boolean reaching the tool call is not the model's mistake.
    for (field, expected) in [
        ("captionsPass", false), // P.policy.captionsPass
        ("speedPass", true), // P.policy.speedPass
        ("decorationsPass", false), // P.policy.decorationsPass
    ] {
        let mut policy = Policy::default();
        let value = if expected { "on" } else { "off" };
        assert_eq!(
            policy::apply(&mut policy, &proposal(field, value, "asked for it")),
            Applied::Set,
            "{field}={value}"
        );
        let stored = stored(&policy, field);
        assert_eq!(stored["value"], expected, "{stored}");
        assert_eq!(stored["source"], "model", "{stored}");
    }

    let mut policy = Policy::default();
    assert_eq!(
        policy::apply(&mut policy, &proposal("speedPass", "false", "no time-lapse wanted")),
        Applied::Set
    );
    assert!(!policy.speed_pass.value, "JSON `false` reads as off"); // P.policy.speedPass
}

#[test]
fn f0_7_s3_an_unknown_field_is_refused_naming_it_and_listing_the_real_ones() {
    let mut policy = Policy::default();
    let before = policy.clone();
    let outcome = policy::apply(&mut policy, &proposal("stylePreset", "lecture", "sounds academic"));
    let say = match &outcome {
        Applied::Refused(say) => say.clone(),
        other => panic!("an unknown field must refuse, got {other:?}"),
    };
    assert!(say.starts_with("stylePreset is not a policy field"), "{say}");
    // Listing the real ones is what lets the model correct itself instead of guessing again.
    for field in ["markingPass", "cutMode", "captionsPass", "speedPass", "decorationsPass"] {
        assert!(say.contains(field), "the refusal should list {field}: {say}");
    }
    assert_eq!(policy, before, "a refused proposal changes nothing at all");
}

#[test]
fn f0_7_s3_an_out_of_range_value_is_refused_naming_the_field_and_its_range() {
    let mut policy = Policy::default();
    let before = policy.clone();
    let outcome = policy::apply(&mut policy, &proposal("cutMode", "vibes", "go with my gut")); // P.policy.cutMode
    let say = match &outcome {
        Applied::Refused(say) => say.clone(),
        other => panic!("an out-of-range value must refuse, got {other:?}"),
    };
    assert!(say.contains("out of range"), "{say}");
    assert!(say.contains("cutMode"), "the refusal names which field: {say}");
    assert!(say.contains("words") && say.contains("model"), "and what it takes: {say}");
    assert_eq!(policy, before, "still untouched");
}

#[test]
fn f0_7_s3_one_bad_proposal_does_not_undo_the_good_ones_in_the_same_batch() {
    // Failure is specific and local: three land, one does not, and exactly three are landed afterwards.
    let mut policy = Policy::default();
    let batch = [
        proposal("markingPass", "none", "nothing to mark, the edit is by hand"), // P.policy.markingPass
        proposal("cutMode", "words", "no model needed for a text-driven cut"), // P.policy.cutMode
        proposal("captionsPass", "off", "burnt into the slides already"), // P.policy.captionsPass
        proposal("unknownSwitch", "on", "made up"),
    ];
    let outcomes: Vec<Applied> = batch
        .iter()
        .map(|item| policy::apply(&mut policy, item))
        .collect();
    assert_eq!(outcomes[..3], [Applied::Set, Applied::Set, Applied::Set]);
    assert!(matches!(outcomes[3], Applied::Refused(_)));
    assert_eq!(policy.marking_pass.value, MarkingPass::None);
    assert_eq!(policy.cut_mode.value, CutMode::Words);
    assert!(!policy.captions_pass.value);
}

// ---- S4: hand-set values are never overwritten -----------------------------------------------

#[test]
fn f0_7_s4_a_hand_set_field_keeps_its_old_value_against_a_later_derivation() {
    let mut policy = Policy::default();
    policy.marking_pass = project_field_joins_user(); // P.policy.markingPass, set by hand

    let outcome = policy::apply(
        &mut policy,
        &proposal("markingPass", "none", "the context says skip marking"),
    );
    assert_eq!(
        outcome,
        Applied::KeptUser,
        "S4: the answer is not Refused — nothing was wrong with the proposal, the person simply outranks it"
    );
    assert_eq!(
        policy.marking_pass.value,
        MarkingPass::Joins,
        "the OLD value survives, byte for byte"
    );
    assert_eq!(policy.marking_pass.origin, Origin::User, "and still says who set it");
    assert_eq!(
        policy.marking_pass.because, None,
        "the model's reason is NOT written onto a field the model did not end up setting"
    );
}

#[test]
fn f0_7_s4_the_rule_holds_per_field_not_for_the_whole_policy() {
    // One hand-set field does not freeze the others: the person touched markingPass, not the passes.
    let mut policy = Policy::default();
    policy.marking_pass = project_field_joins_user();
    assert_eq!(
        policy::apply(&mut policy, &proposal("speedPass", "off", "no ramping")),
        Applied::Set
    );
    assert!(!policy.speed_pass.value, "the untouched switch still took the derivation"); // P.policy.speedPass
    assert_eq!(policy.marking_pass.value, MarkingPass::Joins, "the touched one did not");
}

#[test]
fn f0_7_s4_the_predicate_is_the_shared_one_not_a_second_copy() {
    // Same predicate hands_off uses, so the rule lives once across the tree.
    use naivepost::decision_homes;
    use naivepost::hands_off;
    assert!(!decision_homes::overridable_by_derivation(Origin::User));
    assert!(decision_homes::overridable_by_derivation(Origin::Model));
    assert!(decision_homes::overridable_by_derivation(Origin::Default));
    assert_eq!(
        hands_off::model_may_set_pipeline(Origin::User),
        decision_homes::overridable_by_derivation(Origin::User)
    );
}

/// A markingPass field set by hand, built through the real type so this file never fakes the shape.
fn project_field_joins_user() -> naivepost::project::Field<MarkingPass> {
    naivepost::project::Field {
        value: MarkingPass::Joins,
        origin: Origin::User,
        because: None,
    }
}

// ---- S5: the form reads exactly the five settable fields --------------------------------------

#[test]
fn f0_7_s5_the_form_reads_exactly_the_five_fields_that_can_be_set() {
    let rows = policy::derived_fields();
    let names: Vec<&str> = rows.iter().map(|row| row.field).collect();
    assert_eq!(
        names,
        vec![
            "markingPass",
            "cutMode",
            "captionsPass",
            "speedPass",
            "decorationsPass"
        ],
        "one list drives both validation and the form, so nothing is settable-but-hidden or shown-but-\
         unsettable"
    );
    // Each row answers to its own P.policy id, which is what the form prints beside the field.
    let ids: Vec<&str> = rows.iter().map(|row| row.id).collect();
    assert_eq!(
        ids,
        vec![
            "P.policy.markingPass",
            "P.policy.cutMode",
            "P.policy.captionsPass",
            "P.policy.speedPass",
            "P.policy.decorationsPass"
        ]
    );
    // And every row's value list matches what `apply` accepts — re-checking through apply, not by eye.
    for row in rows {
        for value in row.values {
            let mut probe = Policy::default();
            assert_eq!(
                policy::apply(&mut probe, &proposal(row.field, value, "range check")),
                Applied::Set,
                "{}={} is listed as accepted but refused",
                row.field,
                value
            );
        }
    }
}

// ---- the consequences: which flows the policy turns on ---------------------------------------

#[test]
fn f0_7_the_pipeline_fields_decide_which_flows_run() {
    let mut policy = Policy::default();
    assert!(policy::pass_runs(&policy, Pass::Captions)); // F3.9 asked
    assert!(policy::pass_runs(&policy, Pass::Speeds)); // F3.10 asked
    assert!(policy::pass_runs(&policy, Pass::Decorations)); // F3.11 asked

    policy::apply(&mut policy, &proposal("captionsPass", "off", "already burnt in"));
    assert!(
        !policy::pass_runs(&policy, Pass::Captions),
        "off means never asked, not asked-and-ignored — the rewrite asks no job the context rules out"
    );
    assert!(policy::pass_runs(&policy, Pass::Speeds), "and only that one");

    assert_eq!(
        policy::marking_pass_of(&policy),
        MarkingPass::Retakes,
        "unchanged by a pass switch"
    );
    policy::apply(&mut policy, &proposal("cutMode", "words", "text-driven"));
    assert_eq!(policy::cut_mode_of(&policy), CutMode::Words); // F2.14 takes the words path
}

#[test]
fn f0_7_a_changed_marking_pass_invalidates_the_marks_only_when_it_actually_changes() {
    // The marks are `retakes.tsv` and `final.txt` (layout::Tree::final_txt / ::retakes_tsv): they hold
    // what the previous pass decided, so a DIFFERENT pass would read them as its own output and skip its
    // work. Same value ⇒ nothing goes stale, and a re-derive that agrees must not throw away good work.
    assert!(policy::invalidated_by_marking_change(MarkingPass::Retakes, MarkingPass::Joins));
    assert!(policy::invalidated_by_marking_change(MarkingPass::None, MarkingPass::Retakes));
    assert!(!policy::invalidated_by_marking_change(MarkingPass::Retakes, MarkingPass::Retakes));
    assert!(!policy::invalidated_by_marking_change(MarkingPass::None, MarkingPass::None));
}


// ---- the derivation flow: S1's stop, S3's retry loop, S4's protection -------------------------
//
// `derive` takes its model as an argument, so these tests never touch a server: each installs a stub
// that answers a fixed script and counts how many rounds it was asked. The count is what makes S1's
// "stop" checkable — an empty context that quietly asked anyway would show up here.

/// A scripted model. `asked` counts the rounds; each round pops one answer, and an exhausted script
/// answers nothing so a test only sees the rounds it planned.
struct Scripted {
    asked: std::cell::Cell<usize>,
    rounds: std::cell::RefCell<std::vec::IntoIter<Vec<Proposal>>>,
}

impl Scripted {
    fn new(rounds: Vec<Vec<Proposal>>) -> Self {
        Self {
            asked: std::cell::Cell::new(0),
            rounds: std::cell::RefCell::new(rounds.into_iter()),
        }
    }
    /// Always refuses the same way — the stubborn-model case.
    fn stubborn() -> Self {
        Self::new(Vec::new())
    }
    fn call(&mut self, req: &policy::DerivationRequest, refusals: &[String]) -> Vec<Proposal> {
        self.asked.set(self.asked.get() + 1);
        if self.rounds.borrow().len() == 0 {
            // No script left: keep refusing with the same bad value so the cap is what ends it.
            return vec![proposal("markingPass", "sideways", "I insist")];
        }
        let _ = (req, refusals);
        self.rounds.borrow_mut().next().unwrap_or_default()
    }
}

#[test]
fn f0_7_s1_an_empty_context_never_calls_the_model() {
    let mut model = Scripted::new(vec![vec![proposal("cutMode", "words", "should not be asked")]]);
    let mut policy = Policy::default();
    let out = policy::derive(&mut policy, "   \n ", &mut |r, f| model.call(r, f));
    assert_eq!(model.asked.get(), 0, "an empty context must not ask a model");
    assert!(out.used_defaults, "S1 reports the defaults were used: {out:?}");
    assert!(out.applied.is_empty() && out.refusals.is_empty(), "{out:?}");
    // What stands is the whole default set, field by field.
    assert_eq!(policy.marking_pass.value, MarkingPass::Retakes); // P.policy.markingPass
    assert_eq!(policy.cut_mode.value, CutMode::Model); // P.policy.cutMode
    assert!(policy.captions_pass.value); // P.policy.captionsPass
    assert!(policy.speed_pass.value); // P.policy.speedPass
    assert!(policy.decorations_pass.value); // P.policy.decorationsPass
}

#[test]
fn f0_7_s3_a_refusal_is_handed_back_and_the_model_gets_another_round() {
    let mut model = Scripted::new(vec![
        vec![proposal("markingPass", "sideways", "as I read it")],
        vec![proposal("markingPass", "joins", "the seams are what matter")],
    ]);
    let mut policy = Policy::default();
    let out = policy::derive(&mut policy, "one take per slide", &mut |r, f| model.call(r, f));
    assert_eq!(model.asked.get(), 2, "the refusal bought exactly one more round");
    assert_eq!(out.refusals.len(), 1, "{:?}", out.refusals);
    assert!(
        out.refusals[0].contains("sideways") && out.refusals[0].contains("out of range"),
        "the refusal names the bad value: {:?}",
        out.refusals[0]
    );
    // The corrected proposal landed, with the model as its source. // P.policy.markingPass
    assert_eq!(policy.marking_pass.value, MarkingPass::Joins);
    assert_eq!(policy.marking_pass.origin, Origin::Model);
    assert!(out.applied.contains(&("markingPass", Applied::Set)));
}

#[test]
fn f0_7_s3_refusals_stop_at_the_round_cap() {
    // A model that never corrects itself must end, not spin against the server forever.
    let mut model = Scripted::stubborn();
    let mut policy = Policy::default();
    let out = policy::derive(&mut policy, "anything at all", &mut |r, f| model.call(r, f));
    assert_eq!(
        out.refusals.len(),
        policy::DERIVE_ROUNDS as usize,
        "the loop is bounded by DERIVE_ROUNDS, not open-ended"
    );
    assert!(out.applied.is_empty());
    assert_eq!(
        policy.marking_pass.value,
        MarkingPass::Retakes,
        "a refused derivation leaves the default standing" // P.policy.markingPass
    );
}

#[test]
fn f0_7_s4_a_user_field_survives_a_derivation_that_names_it() {
    let mut policy = Policy::default();
    policy.cut_mode = project::Field {
        value: CutMode::Words,
        origin: Origin::User,
        because: None,
    };
    let mut model = Scripted::new(vec![vec![proposal("cutMode", "model", "it decides better")]]);
    let out = policy::derive(&mut policy, "let the model choose", &mut |r, f| model.call(r, f));
    assert!(
        out.applied.contains(&("cutMode", Applied::KeptUser)),
        "S4 reports the keep: {:?}",
        out.applied
    );
    assert_eq!(
        (policy.cut_mode.value, policy.cut_mode.origin),
        (CutMode::Words, Origin::User),
        "a hand-set field is untouched by a later derivation" // P.policy.cutMode
    );
    assert!(out.refusals.is_empty(), "a keep is not an error: {:?}", out.refusals);
}

#[test]
fn f0_7_s5_a_changed_marking_pass_invalidates_the_marks() {
    let before = Policy::default(); // markingPass retakes
    let mut after = Policy::default();
    assert!(
        policy::invalidated_marks(&before, &after).is_empty(),
        "an unchanged pass throws nothing away"
    );
    after.marking_pass = project::Field {
        value: MarkingPass::None,
        origin: Origin::Model,
        because: Some("no retakes in this session".into()),
    };
    assert_eq!(
        policy::invalidated_marks(&before, &after),
        vec!["retakes.tsv", "final.txt"],
        "// P.policy.markingPass changed: both marks go, so Prepare re-runs its marking pass"
    );
}

#[test]
fn f0_7_tracker_a_never_derived_context_needs_a_derivation() {
    assert!(
        policy::Tracker::new().needs_derive("anything"),
        "nothing derived yet means the first question is yes"
    );
    let mut tracker = policy::Tracker::new();
    tracker.mark_derived("one take per slide");
    assert!(
        !tracker.needs_derive("one take per slide"),
        "the same context needs no second derivation"
    );
    assert!(
        tracker.needs_derive("one take per slide "),
        "an edited context — even by a trailing space — is a change"
    );
    tracker.forget();
    assert!(tracker.needs_derive("one take per slide"), "forget resets it");
}
