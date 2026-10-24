//! tool:set_policy, end to end: what `policy` in `naivepost.json` holds after a live derivation.
//!
//! §00's paragraph reads "Filled two ways: a value a model derived from the User Context via
//! `tool:set_policy` …, else the shipped default. There is no form: to change a value, the user says so in
//! the User Context, and the log says what the model set and why." What this file adds over the other policy
//! tests is the FILE afterwards, with the two facts nothing else states: a `source: "user"` value already on
//! disk — the shape the deleted policy form used to write — is replaced by what the derivation returns, and a
//! field nobody proposed keeps its shipped default with `source: "default"`. The rest belongs elsewhere: the
//! log line's shape is `tests/f0_7_s4_each_value_is_logged.rs`, the wire value arriving as `model` beside its
//! `because` is `tests/f0_7_s2_a_live_set_policy_answer_lands_as_model_with_its_because.rs`, and the verdict
//! shapes the tool returns are `tests/suite/tool_set_policy.rs`.
#![allow(dead_code)]

#[path = "policy_live_harness.rs"]
mod harness;

use harness::*;
use naivepost::project::{self, Field, MarkingPass, Origin, Project};

/// The `because` the model sends with its call. Nothing here asserts about it — the reason surviving to the
/// policy is `f0_7_s2_a_live_set_policy_answer_lands_as_model_with_its_because`'s fact — but a real
/// `set_policy` argument has to carry one.
const BECAUSE: &str = "the speaker read a script and restarted after every stumble";

/// A session whose `naivepost.json` holds a hand-set `markingPass` — the shape the deleted policy form used
/// to write, still loadable and holding nothing back now. Saved through `project::save`, the program's own
/// writer, so the bytes on disk are the documented ones.
fn session_with_hand_set_marking(tag: &str) -> Fixture {
    let fixture = fixture(tag);
    let mut project = Project {
        context: fixture.context.clone(),
        ..Project::default()
    };
    project.policy.marking_pass = Field {
        value: MarkingPass::Retakes,
        origin: Origin::User,
        because: None,
    };
    project::save(&project, &fixture.session).expect("the project file is written");
    fixture
}

/// The two ways §00 lists, read back off the project file after a derivation: the model's value stands where
/// an old file said the person had chosen otherwise, and everything untouched is still the shipped default.
#[test]
fn tool_set_policy_a_derived_value_is_what_naivepost_json_holds_afterwards() {
    let fixture = session_with_hand_set_marking("jsonhold");
    let llm = FakeLlm::start(vec![set_policy("markingPass", "joins", BECAUSE)]);
    point_at(&llm.url());

    // `derive_lively` reads the starting policy off `<session>/naivepost.json`, dials the fake, and lets
    // `policy::derive` apply what came back — the same door `ui::window_policy::derive_policy` runs.
    let run = derive_lively(&fixture, &fixture.context);

    // Persist and read it back through the program's own writer and reader, then assert the FILE's shape.
    let project = Project {
        context: fixture.context.clone(),
        policy: run.policy.clone(),
        ..Project::default()
    };
    project::save(&project, &fixture.session).expect("the derived policy is saved");
    let loaded = project::load(&fixture.session).expect("the project loads back");
    // F0.7 stores whatever the User Context speaks to, with no precedence left for a hand-set field to claim.
    assert_eq!(
        loaded.policy.marking_pass.value,
        MarkingPass::Joins,
        "the derived value replaced the hand-set one that was on disk"
    );
    let on_disk = serde_json::to_value(&loaded.policy).expect("a Policy serialises");
    assert_eq!(on_disk["markingPass"]["value"].as_str(), Some("joins"));
    // The second way a value arrives: nobody spoke to `cutMode`, so the shipped default stands, marked as
    // such. Two ways, and no third.
    assert_eq!(
        loaded.policy.cut_mode.value,
        naivepost::project::CutMode::Model,
        "an untouched field keeps §10's default"
    );
    assert_eq!(
        on_disk["cutMode"]["source"].as_str(),
        Some("default"),
        "{on_disk}"
    );
}
