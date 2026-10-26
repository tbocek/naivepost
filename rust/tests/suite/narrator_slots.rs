//! P.policy.narratorSlots — how many people a session can name.
//! `spec/10-parameters.md` row `P.policy.narratorSlots` (default 4; prototype `narratorSlots`), read
//! by [`naivepost::run`] when ▶ freezes the sources and by [`naivepost::sources`] when a row is
//! tagged or a project file is loaded.
//!
//! S1 the default and its family · S2 the bound as a predicate · S3 the run snapshot carries exactly
//! that many slots · S4 voice-id naming stops at the same number · S5 the cycle visits each slot once
//! then has none to give · S6 a slot past the bound is stripped on load.

use crate::common;
use common::source_row as row;
use naivepost::narrate_rules as rules;
use naivepost::params::{self, Family};
use naivepost::project::Project;
use naivepost::run::{self, NARRATOR_SLOTS};
use naivepost::sources;

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
    assert_eq!(
        run::slot_index(NARRATOR_SLOTS as u32),
        Some(NARRATOR_SLOTS - 1)
    );
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
    assert_eq!(
        rules::narrator_slot(&format!("narrator{}", NARRATOR_SLOTS + 1)),
        None
    );
    assert_eq!(rules::narrator_slot("aria"), None);
}

// --- S5: the cycle walks every voice and comes back to 1, never to none ------------------------

#[test]
fn p_policy_narratorslots_s5_the_cycle_walks_every_voice_then_back_to_one() {
    // One row clicked over and over from voice 1: 2, … the last voice, then 1 again. There is no "no
    // voice": every recording is somebody. Voices are not exclusive, so a row on 1 beside it changes
    // nothing about where the cycle goes.
    let mut project = untagged(2);
    project.sources[0].narrator = 1;
    project.sources[1].narrator = 1;
    let given: Vec<u32> = (0..NARRATOR_SLOTS)
        .map(|_| sources::cycle_narrator(&mut project, 0))
        .collect();
    let expected: Vec<u32> = (2..=NARRATOR_SLOTS as u32).chain([1]).collect();
    assert_eq!(given, expected, "cycling 1..N and back to 1");
    assert_eq!(
        project.sources[1].narrator, 1,
        "the other row keeps its voice"
    );
}

// --- S6: a voice past the bound, or an older file's 0, is voice 1 on load ----------------------

#[test]
fn p_policy_narratorslots_s6_a_slot_past_the_bound_is_voice_one_on_load() {
    let too_many = NARRATOR_SLOTS as u32 + 1;
    let mut project = Project {
        sources: vec![
            row("cam.mkv", true, 1),
            row("extra.wav", false, too_many),
            row("older.wav", false, 0),
        ],
        ..Default::default()
    };
    let mut report: Vec<String> = Vec::new();
    sources::clean_on_load(&mut project, &mut report);
    // An older file's 0 ("no voice") is voice 1 now, and unsaid: it is no mistake of the person's.
    assert_eq!(project.sources[2].narrator, 1, "a 0 reads as voice 1");
    assert!(
        !report.iter().any(|line| line.contains("older.wav")),
        "the 0 goes unsaid: {report:?}"
    );
    // The row keeps its file and goes to voice 1: a voice that does not exist cannot be held, and there
    // is no "no voice" to put it in.
    assert_eq!(project.sources[1].narrator, 1);
    assert!(
        report
            .iter()
            .any(|line| line.contains(&format!("voice {too_many} is past the last one"))),
        "{report:?}"
    );
    // Slot 1 itself is untouched — only the impossible tag was removed.
    assert_eq!(project.sources[0].narrator, 1);
}

// --- the voice number on a video is not a narrator microphone ----------------------------------

#[test]
fn p_policy_narratorslots_s7_a_video_with_a_voice_is_film_not_a_narrator_mic() {
    // A number says which voice speaks in a recording. Only an AUDIO recording with one is a narrator
    // microphone (heard by the app, not played by the video): the first rewrite read a tagged camera
    // recording that way too, and left its words out of the joins and the retake marking.
    let project = Project {
        sources: vec![
            row("cam.mkv", true, 1),
            row("cam2.mkv", true, 1),
            row("mic.wav", false, 1),
            row("room.wav", false, 0),
        ],
        ..Default::default()
    };
    let mic: Vec<bool> = project
        .sources
        .iter()
        .map(sources::is_narrator_mic)
        .collect();
    assert_eq!(mic, [false, false, true, false]);
    // Voice 1's recording is its dedicated microphone when there is one, else the first with that number.
    assert_eq!(sources::narrator_of(&project, 1), Some("mic.wav"));
    let cameras_only = Project {
        sources: vec![row("cam.mkv", true, 1), row("cam2.mkv", true, 1)],
        ..Default::default()
    };
    assert_eq!(sources::narrator_of(&cameras_only, 1), Some("cam.mkv"));
    // The transcript step writes commentary only for the microphone.
    let stage = naivepost::fix_call::stage(&project);
    let commentary: Vec<&str> = stage
        .tasks
        .iter()
        .filter(|task| task.args.len() >= 3)
        .map(|task| task.args[2].as_str())
        .collect();
    assert_eq!(commentary, ["0", "0", "1", "0"]);
}
