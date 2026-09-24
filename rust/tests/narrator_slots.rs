//! P.policy.narratorSlots — how many people a session can name.
//! `spec/10-parameters.md` row `P.policy.narratorSlots` (default 4; prototype `narratorSlots`), read
//! by [`naivepost::run`] when ▶ freezes the sources and by [`naivepost::sources`] when a row is
//! tagged or a project file is loaded.
//!
//! S1 the default and its family · S2 the bound as a predicate · S3 the run snapshot carries exactly
//! that many slots · S4 voice-id naming stops at the same number · S5 the cycle visits each slot once
//! then has none to give · S6 a slot past the bound is stripped on load.

use naivepost::narrate_rules as rules;
use naivepost::project::{Project, Source};
use naivepost::params::{self, Family};
use naivepost::run::{self, NARRATOR_SLOTS};
use naivepost::sources;

fn row(path: &str, footage: bool, narrator: u32) -> Source {
    Source {
        path: path.into(),
        footage,
        narrator,
        ..Default::default()
    }
}

/// One untagged row per slot, plus one extra — the case with no room left.
fn untagged(count: usize) -> Project {
    Project {
        sources: (0..count)
            .map(|i| row(&format!("voice{i}.wav"), false, 0))
            .collect(),
        ..Default::default()
    }
}

// --- S1: the default, and where the id sits ----------------------------------------------------

#[test]
fn p_policy_narratorslots_s1_four_slots_and_the_policy_family() {
    // P.policy.narratorSlots = 4: what fits on a row as a digit, and what a group recording tends
    // to hold. Voices beyond it stay untagged and are still transcribed.
    assert_eq!(NARRATOR_SLOTS, 4);
    assert_eq!(params::family("P.policy.narratorSlots"), Family::Policy);
    // Unrowed by design: §04#4 does not name this id (and prepare_parameters.rs pins that list by
    // order), nor does §07#4, whose foreign-row whitelist would reject it. The number lives with the
    // rule that reads it, in run::NARRATOR_SLOTS.
    assert!(
        params::find("P.policy.narratorSlots").is_none(),
        "unrowed by design; see NARRATOR_SLOTS' doc comment"
    );
    // A sibling policy value IS rowed, so the omission reads as deliberate rather than missing.
    let caption_floor = params::narrate()
        .into_iter()
        .chain(params::cut())
        .find(|param| param.id == "P.policy.captionMinSeconds");
    assert!(caption_floor.is_some(), "another P.policy.* id has a row");
}

// --- S2: the bound as a predicate ------------------------------------------------------------

#[test]
fn p_policy_narratorslots_s2_only_these_numbers_are_slots() {
    for slot in 1..=NARRATOR_SLOTS as u32 {
        assert!(run::is_slot(slot), "{slot} is a slot");
    }
    // Untagged is not a slot; neither is anything past the last one.
    assert!(!run::is_slot(0));
    assert!(!run::is_slot(NARRATOR_SLOTS as u32 + 1));
    assert!(!run::is_slot(99));
    // And the index into the snapshot's list follows the same bound.
    assert_eq!(run::slot_index(1), Some(0));
    assert_eq!(run::slot_index(NARRATOR_SLOTS as u32), Some(NARRATOR_SLOTS - 1));
    assert_eq!(run::slot_index(0), None);
    assert_eq!(run::slot_index(NARRATOR_SLOTS as u32 + 1), None);
}

// --- S3: the frozen snapshot has exactly that many slots --------------------------------------

#[test]
fn p_policy_narratorslots_s3_the_snapshot_holds_one_place_per_slot() {
    let project = Project {
        sources: vec![
            row("cam.mkv", true, 1),
            row("mic.wav", false, 3),
            row("screen.mkv", false, 0),
        ],
        ..Default::default()
    };
    let snap = run::snapshot_sources(&project);
    // Four places, whoever happens to hold them — a run always has the whole set to answer from.
    assert_eq!(snap.narrators.len(), NARRATOR_SLOTS);
    assert_eq!(snap.narrators[0].as_deref(), Some("cam.mkv"));
    assert_eq!(snap.narrators[1], None);
    assert_eq!(snap.narrators[2].as_deref(), Some("mic.wav"));
    assert_eq!(snap.narrators[NARRATOR_SLOTS - 1], None);
    // A wild slot in the file takes nothing at all rather than writing out of bounds.
    let wild = Project {
        sources: vec![row("odd.wav", false, NARRATOR_SLOTS as u32 + 2)],
        ..Default::default()
    };
    let frozen = run::snapshot_sources(&wild);
    assert_eq!(frozen.narrators.len(), NARRATOR_SLOTS, "still four places");
    assert!(
        frozen.narrators.iter().all(Option::is_none),
        "a slot past the bound holds nobody"
    );
}

// --- S4: the voice-id spelling stops at the same number ---------------------------------------

#[test]
fn p_policy_narratorslots_s4_voice_ids_name_no_slot_past_the_bound() {
    // `own` is slot 1 — the project's own voice, before slots were numbered.
    assert_eq!(rules::narrator_slot("own"), Some(1));
    assert_eq!(rules::narrator_slot("narrator2"), Some(2));
    // The last named id is the last slot, spelled from the constant so the two cannot drift apart.
    assert_eq!(
        rules::narrator_slot(&format!("narrator{NARRATOR_SLOTS}")),
        Some(NARRATOR_SLOTS)
    );
    // One past it names nothing, which is why the picker goes empty rather than inventing a voice.
    assert_eq!(rules::narrator_slot(&format!("narrator{}", NARRATOR_SLOTS + 1)), None);
    assert_eq!(rules::narrator_slot("aria"), None);
}

// --- S5: the cycle gives each slot once, then has none left -----------------------------------

#[test]
fn p_policy_narratorslots_s5_the_cycle_visits_every_slot_once_then_none() {
    // Five rows for four slots: the fifth gets nothing, because there is no fifth slot to give.
    let mut project = untagged(NARRATOR_SLOTS + 1);
    let given: Vec<u32> = (0..=NARRATOR_SLOTS)
        .map(|index| sources::cycle_narrator(&mut project, index))
        .collect();
    let expected: Vec<u32> = (1..=NARRATOR_SLOTS as u32).chain([0]).collect();
    assert_eq!(given, expected, "cycling free slots 1..N then none");
    // Nobody was moved to make room: each holder is still the row that was given the slot.
    for slot in 1..=NARRATOR_SLOTS as u32 {
        assert_eq!(sources::slot_holder(&project, slot), Some((slot - 1) as usize));
    }
    assert_eq!(project.sources[NARRATOR_SLOTS].narrator, 0);
}

// --- S6: a slot past the bound is stripped on load --------------------------------------------

#[test]
fn p_policy_narratorslots_s6_a_slot_past_the_bound_is_untagged_on_load() {
    let too_many = NARRATOR_SLOTS as u32 + 1;
    let mut project = Project {
        sources: vec![row("cam.mkv", true, 1), row("extra.wav", false, too_many)],
        ..Default::default()
    };
    let mut report: Vec<String> = Vec::new();
    sources::clean_on_load(&mut project, &mut report);
    // The row keeps its file and loses the tag: a slot that does not exist cannot be held.
    assert_eq!(project.sources[1].narrator, 0);
    assert!(
        report
            .iter()
            .any(|line| line.contains(&format!("slot {too_many} is taken"))),
        "{report:?}"
    );
    // Slot 1 itself is untouched — only the impossible tag was removed.
    assert_eq!(project.sources[0].narrator, 1);
}
