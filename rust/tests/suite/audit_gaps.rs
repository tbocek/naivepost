//! §12-decisions#5-gaps-this-audit-closed — the eight catalogue additions and the four prototype
//! behaviours marked defects, checked against the code that closed them.
//!
//! §5 has no numbered steps, so the tests below are numbered by the claim under test: s1-s4 the two
//! halves of the first claim (the additions exist; jobs name reads; write replies carry more than ok),
//! s5-s7 the four defects. Every assertion takes its evidence from a live call rather than from the
//! audit prose, as `tests/suite/hands_off.rs` does.

use naivepost::audit_gaps::{self as gaps, Defect};
use naivepost::narration::{self, Entry, Narration, Silent};
use naivepost::produce_details;
use naivepost::tools::Tool;

// --- claim 1a: the seven added tools are registered and reachable ---------------------------------

// --- claim 1b: the box argument on add_effect -----------------------------------------------------

// --- claim 1c: every covered job gets a read tool --------------------------------------------------

// --- claim 1d: write replies carry what the app made ----------------------------------------------

// --- defect 1: the silent list survives -----------------------------------------------------------

#[test]
fn sec_12_decisions_5_gaps_this_audit_closed_s5_the_users_silent_list_survives_a_rewrite() {
    let silent = vec![Silent { s: 40.0, e: 65.0 }, Silent { s: 100.0, e: 120.0 }];
    let before = vec![Entry {
        s: 40.0,
        e: 65.0,
        at: 1.0,
        text: "old words".into(),
        ..Default::default()
    }];
    let after = vec![
        Entry {
            s: 0.0,
            e: 20.0,
            at: 0.5,
            text: "brand new opening".into(),
            ..Default::default()
        },
        Entry {
            s: 70.0,
            e: 90.0,
            at: 2.0,
            text: "and a different close".into(),
            ..Default::default()
        },
    ];

    let kept = gaps::silent_survives_entry_rewrite(&silent, &before, &after);
    assert_eq!(kept, silent, "the rewrite touched nothing the user curated");

    // Kept by bounds: the narration still answers true for those clips afterwards.
    let record = Narration {
        entries: after.clone(),
        silent: kept.clone(),
    };
    assert!(record.is_silent(40.0, 65.0));
    assert!(record.is_silent(100.0, 120.0));
    assert!(
        !record.is_silent(0.0, 20.0),
        "a clip nobody silenced is not silent"
    );
    // And the entries really did change underneath — survival is not because nothing happened.
    assert_ne!(record.entries, before);

    assert!(Defect::SilentListWipe.must_fix(), "§5 marks this MUST fix");
    assert!(Defect::SilentListWipe.decided_in().contains("F4.1"));
    assert!(!Defect::SilentListWipe.closed_by().is_empty());
}

// --- defect 2: the placement tag never travels ----------------------------------------------------

// --- defects 3 and 4 -----------------------------------------------------------------------------

#[test]
fn sec_12_decisions_5_gaps_this_audit_closed_s7_title_reprints_and_at_has_one_clock() {
    // F5.6: reprint whenever the text changes, until the picture is edited by hand.
    assert!(gaps::reprint_title_on_change(
        "first title",
        "second title",
        false
    ));
    assert!(
        !gaps::reprint_title_on_change("same", "same", false),
        "nothing changed"
    );
    assert!(
        !gaps::reprint_title_on_change("first", "second", true),
        "the hand got there first"
    );
    // `youtube_title_redraws_picture` is
    // deliberately false — a YouTube title is metadata, so fixing its typo must not reprint the
    // picture. Both are why §5's defect is about the *thumbnail's own* title changing, which is what
    // `reprint_title_on_change` above decides; neither live predicate contradicts F5.6.
    assert!(
        !produce_details::youtube_title_redraws_picture(),
        "a metadata edit never redraws"
    );

    // A sped-up clip keeps the line on the same picture: output seconds shrink with the rate.
    assert!(narration::output_seconds(4.0, 2.0) < narration::output_seconds(4.0, 1.0));
    // A stop divides nothing: the line stays where its own clock puts it.
    assert_eq!(narration::output_seconds(3.0, 0.0), 3.0);
    assert!(
        Defect::AtThreeReadings.decided_in().contains("own clock"),
        "the row says the line keeps its own clock"
    );
}

// --- coverage and cited ids ------------------------------------------------------------------------

#[test]
fn sec_12_decisions_5_gaps_this_audit_closed_cited_ids_answer_from_the_catalogue() {
    fn row(id: &str) -> naivepost::params::Param {
        let mut found = naivepost::params::prepare()
            .into_iter()
            .chain(naivepost::params::cut())
            .chain(naivepost::params::effects())
            .chain(naivepost::params_narrate::rows())
            .chain(naivepost::params::produce())
            .filter(|row| row.id == id)
            .collect::<Vec<_>>();
        assert_eq!(found.len(), 1, "{id} catalogued {} times", found.len());
        found.pop().unwrap()
    }
    for id in gaps::CITED_PARAMS {
        let _ = row(id);
    }
    let known = [
        Tool::GetFrames,
        Tool::SpeechAround,
        Tool::FlagLine,
        Tool::GetWords,
        Tool::CutStatus,
        Tool::ListEmotions,
        Tool::DescribeInsert,
        Tool::AddEffect,
        Tool::SetPolicy,
    ];
    for name in gaps::CITED_TOOLS {
        assert!(
            known.iter().any(|t| t.name() == name),
            "{name} is not a known tool"
        );
    }
}
