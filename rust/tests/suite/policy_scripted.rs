//! A script's own words decide nothing about the edit, and a scripted recording cuts from its words.

use naivepost::policy::{cut_mode_of, read_target_length};
use naivepost::project::{CutMode, Field, MarkingPass, Origin, Policy};

const SCRIPTED: &str = "No target length.\n\n****SCRIPT START****\nIt was stolen in about 20 minutes.\n****SCRIPT STOP****\n";

#[test]
fn a_length_inside_the_script_is_not_a_target() {
    let mut policy = Policy::default();
    let was = policy.target_length_seconds.value;
    read_target_length(&mut policy, SCRIPTED);
    assert_eq!(policy.target_length_seconds.value, was);
}

#[test]
fn a_length_outside_the_script_is_read() {
    let mut policy = Policy::default();
    read_target_length(
        &mut policy,
        "About 12 minutes.\n\n****SCRIPT START****\nIn 20 minutes.\n****SCRIPT STOP****\n",
    );
    assert_eq!(policy.target_length_seconds.value, 720.0);
}

fn with(marking: MarkingPass, mode: CutMode, origin: Origin) -> Policy {
    let mut policy = Policy::default();
    policy.marking_pass.value = marking;
    policy.cut_mode = Field {
        value: mode,
        origin,
        ..policy.cut_mode.clone()
    };
    policy
}

#[test]
fn joins_cut_from_the_words_unless_someone_chose() {
    assert_eq!(
        cut_mode_of(&with(MarkingPass::Joins, CutMode::Model, Origin::Default)),
        CutMode::Words
    );
    assert_eq!(
        cut_mode_of(&with(MarkingPass::Joins, CutMode::Model, Origin::Model)),
        CutMode::Model
    );
    assert_eq!(
        cut_mode_of(&with(MarkingPass::Joins, CutMode::Model, Origin::User)),
        CutMode::Model
    );
    assert_eq!(
        cut_mode_of(&with(MarkingPass::Retakes, CutMode::Model, Origin::Default)),
        CutMode::Model
    );
}
