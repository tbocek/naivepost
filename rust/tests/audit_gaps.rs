//! §12-decisions#5-gaps-this-audit-closed — the eight catalogue additions and the four prototype
//! behaviours marked defects, checked against the code that closed them.
//!
//! §5 has no numbered steps, so the tests below are numbered by the claim under test: s1-s4 the two
//! halves of the first claim (the additions exist; jobs name reads; write replies carry more than ok),
//! s5-s7 the four defects. Every assertion takes its evidence from a live call rather than from the
//! audit prose, as `tests/hands_off.rs` does.

use naivepost::audit_gaps::{self as gaps, AdditionKind, Defect};
use naivepost::cut::Seg;
use naivepost::narration::{self, Entry, Narration, Silent};
use naivepost::narrate_pass;
use naivepost::produce_details;
use naivepost::produce_subtitles;
use naivepost::roles::Job;
use naivepost::tools::clips::Clips;
use naivepost::tools::cutpass::Plan;
use naivepost::tools::Tool;

/// Parse a tool reply's fields, panicking with the body when it is not JSON.
fn body(reply: &str) -> serde_json::Value {
    serde_json::from_str(reply).unwrap_or_else(|_| panic!("not JSON: {reply}"))
}

// --- claim 1a: the seven added tools are registered and reachable ---------------------------------

#[test]
fn sec_12_decisions_5_gaps_this_audit_closed_s1_the_added_tools_are_registered_and_reachable() {
    let all_jobs = gaps::all_covered_jobs();
    for name in [
        "get_frames",
        "speech_around",
        "flag_line",
        "get_words",
        "cut_status",
        "list_emotions",
        "describe_insert",
    ] {
        let offered_to = all_jobs
            .iter()
            .filter(|j| naivepost::tools::offered(**j).iter().any(|t| t.name() == name))
            .collect::<Vec<_>>();
        assert!(
            !offered_to.is_empty(),
            "{name} is catalogued but offered to no job — the gap it closed is still open"
        );
    }
    // And each addition row names something real.
    let rows = gaps::additions();
    assert_eq!(rows.len(), 8);
    let tool_rows: Vec<&str> = rows
        .iter()
        .filter(|r| r.kind == AdditionKind::Tool)
        .map(|r| r.name)
        .collect();
    assert_eq!(tool_rows.len(), 7, "seven tools plus one argument");
    assert_eq!(
        rows.iter()
            .filter(|r| r.kind == AdditionKind::Argument)
            .count(),
        1
    );
}

// --- claim 1b: the box argument on add_effect -----------------------------------------------------

#[test]
fn sec_12_decisions_5_gaps_this_audit_closed_s2_add_effect_takes_a_box_and_reports_it() {
    let mut batch = Clips::new(&[(1, 0.0, 30.0)]);
    let aimed = body(&batch.add_effect(1, "zoom", 2.0, 6.0, None, Some((0.3, 0.2))));
    assert_eq!(aimed["box"][0].as_f64().unwrap(), 0.3);
    assert_eq!(aimed["box"][1].as_f64().unwrap(), 0.2);

    // No box: the frame's middle, reported rather than assumed — the punch-in every zoom used to be.
    let blind = body(&batch.add_effect(1, "zoom", 2.0, 6.0, None, None));
    assert_eq!(
        blind["box"][0].as_f64().unwrap(),
        naivepost::tools::clips::DEFAULT_ZOOM_BOX.0
    );
    assert_eq!(
        blind["box"][1].as_f64().unwrap(),
        naivepost::tools::clips::DEFAULT_ZOOM_BOX.1
    );

    // The argument row exists and says why it was needed.
    let arg = gaps::additions()
        .into_iter()
        .find(|r| r.kind == AdditionKind::Argument)
        .unwrap();
    assert_eq!(arg.name, "box");
    assert!(arg.why.contains("middle"), "{}", arg.why);
}

// --- claim 1c: every covered job gets a read tool --------------------------------------------------

#[test]
fn sec_12_decisions_5_gaps_this_audit_closed_s3_every_job_names_reads_alongside_writes() {
    for job in gaps::all_covered_jobs() {
        let offered = naivepost::tools::offered(job);
        assert!(
            gaps::job_names_reads(job),
            "{job:?} has {} tools and not one of them is a read: {:?}",
            offered.len(),
            offered.iter().map(|t| t.name()).collect::<Vec<_>>()
        );
        // More than one, so the brief is not the only thing the model can lean on.
        assert!(
            gaps::read_count(job) >= 2,
            "{job:?} gets only {} read(s)",
            gaps::read_count(job)
        );
    }
    // The policy pair §5 names as the only read the old jobs had really is context + set_policy.
    let pair = gaps::policy_pair();
    assert_eq!(pair.len(), 2);
    assert!(pair.contains(&Tool::GetContext));
    assert!(pair.contains(&Tool::SetPolicy));
    // The audio-only jobs stay excluded: they are single task calls, not briefed conversations.
    assert!(naivepost::tools::offered(Job::Asr).is_empty());
    assert!(!gaps::all_covered_jobs().contains(&Job::Asr));
}

// --- claim 1d: write replies carry what the app made ----------------------------------------------

#[test]
fn sec_12_decisions_5_gaps_this_audit_closed_s4_write_replies_carry_more_than_ok() {
    // A real cut answer: the edges as placed, how far they moved, what a mark will take out.
    let mut plan = Plan::new(600.0, 300.0).with_snap_points(vec![10.0, 40.5]);
    let added = plan.add_segment(10.4, 40.2, "the demo");
    assert!(gaps::result_carries_more_than_ok(&added), "{added}");
    let added_body = body(&added);
    assert!(added_body.get("span").is_some());
    assert!(added_body.get("moved").is_some());
    assert!(added_body.get("survives_min_scene").is_some());
    // P.policy.minSceneSeconds is what that flag answers about.
    assert!(added_body.get("footage").is_some());
    assert!(added_body.get("window").is_some());

    // A real caption answer: clamped span, the fade applied, whether it moved.
    let mut batch = Clips::new(&[(1, 0.0, 30.0)]);
    let caption = batch.add_caption(1, 2.0, 8.0, "placed, not just accepted");
    assert!(gaps::result_carries_more_than_ok(&caption), "{caption}");
    let cap = body(&caption);
    assert!(cap.get("fade").is_some());
    assert!(cap.get("clamped").is_some());

    // A real narration answer: the offset as placed and the word ceiling it must stay under.
    let seg = Seg { s: 0.0, e: 30.0, ..Default::default() };
    let line = narrate_pass::write_line(&[seg], 1, 2.0, "one two three four five six", "calm", "");
    assert!(gaps::result_carries_more_than_ok(&line), "{line}");
    let written = body(&line);
    assert!(written.get("at_most_words").is_some());
    assert!(written.get("words").is_some());

    // And the failure shape stays one sentence under one field — a refusal is an answer, not a crash.
    let err = body(&naivepost::tools::error("clip 9 is not in this batch"));
    assert!(!gaps::result_carries_more_than_ok(
        &naivepost::tools::error("nope")
    ));
    assert_eq!(err.as_object().unwrap().len(), 1);
    assert!(err["error"].as_str().unwrap().contains("batch"));
}

// --- defect 1: the silent list survives -----------------------------------------------------------

#[test]
fn sec_12_decisions_5_gaps_this_audit_closed_s5_the_users_silent_list_survives_a_rewrite() {
    let silent = vec![Silent { s: 40.0, e: 65.0 }, Silent { s: 100.0, e: 120.0 }];
    let before = vec![Entry { s: 40.0, e: 65.0, at: 1.0, text: "old words".into(), ..Default::default() }];
    let after = vec![
        Entry { s: 0.0, e: 20.0, at: 0.5, text: "brand new opening".into(), ..Default::default() },
        Entry { s: 70.0, e: 90.0, at: 2.0, text: "and a different close".into(), ..Default::default() },
    ];

    let kept = gaps::silent_survives_entry_rewrite(&silent, &before, &after);
    assert_eq!(kept, silent, "the rewrite touched nothing the user curated");

    // Kept by bounds: the narration still answers true for those clips afterwards.
    let record = Narration { entries: after.clone(), silent: kept.clone() };
    assert!(record.is_silent(40.0, 65.0));
    assert!(record.is_silent(100.0, 120.0));
    assert!(!record.is_silent(0.0, 20.0), "a clip nobody silenced is not silent");
    // And the entries really did change underneath — survival is not because nothing happened.
    assert_ne!(record.entries, before);

    assert!(Defect::SilentListWipe.must_fix(), "§5 marks this MUST fix");
    assert!(Defect::SilentListWipe.decided_in().contains("F4.1"));
    assert!(!Defect::SilentListWipe.closed_by().is_empty());
}

// --- defect 2: the placement tag never travels ----------------------------------------------------

#[test]
fn sec_12_decisions_5_gaps_this_audit_closed_s6_the_placement_tag_is_held_out_and_put_back() {
    // The tag exists and is the app's to place.
    assert_eq!(produce_subtitles::placement("top"), Some(r"{\an8}"));
    assert_eq!(produce_subtitles::placement("center"), Some(r"{\an5}"));
    assert_eq!(produce_subtitles::placement("bottom"), None, "bottom needs no tag");
    assert!(gaps::tag_is_app_owned("top"));
    assert!(gaps::tag_is_app_owned("bottom"));
    assert!(gaps::tag_is_app_owned(""), "empty is the default placement");
    assert!(!gaps::tag_is_app_owned("sideways"), "an unknown word is not owned");

    // What goes out for translation carries no tag: the numbered request is plain words.
    let outbound = produce_subtitles::numbered(&[(1, "hello world".to_string())]);
    assert!(!outbound.contains(r"\an"), "{outbound}");
    assert!(outbound.contains("hello world"), "{outbound}");

    // What comes back composed as a cue does carry it — put back by the app, off the pos field.
    let cue = format!("{}translated words", produce_subtitles::placement("top").unwrap());
    assert!(cue.contains(r"{\an8}"), "{cue}");

    assert!(Defect::PlacementTagTravels.must_fix(), "§5 marks this MUST fix");
    assert!(Defect::PlacementTagTravels.decided_in().contains("3.10"));
}

// --- defects 3 and 4 -----------------------------------------------------------------------------

#[test]
fn sec_12_decisions_5_gaps_this_audit_closed_s7_title_reprints_and_at_has_one_clock() {
    // F5.6: reprint whenever the text changes, until the picture is edited by hand.
    assert!(gaps::reprint_title_on_change("first title", "second title", false));
    assert!(!gaps::reprint_title_on_change("same", "same", false), "nothing changed");
    assert!(!gaps::reprint_title_on_change("first", "second", true), "the hand got there first");
    // Cross-checked against the live predicates. `prints_words_first` is the order rule (words print
    // before a chosen frame only when the text gate is open), and `youtube_title_redraws_picture` is
    // deliberately false — a YouTube title is metadata, so fixing its typo must not reprint the
    // picture. Both are why §5's defect is about the *thumbnail's own* title changing, which is what
    // `reprint_title_on_change` above decides; neither live predicate contradicts F5.6.
    assert!(!produce_details::youtube_title_redraws_picture(), "a metadata edit never redraws");
    assert!(produce_details::prints_words_first(true, false));
    assert!(!produce_details::prints_words_first(false, false));

    // One clock, read once: lands_at is output_seconds whatever the rate.
    for (at, rate) in [(2.0, 1.0), (2.0, 4.0), (10.0, 2.5), (1.5, 0.0)] {
        assert_eq!(
            gaps::lands_at(at, rate),
            narration::output_seconds(at, rate),
            "at={at} rate={rate}"
        );
    }
    // A sped-up clip keeps the line on the same picture: output seconds shrink with the rate.
    assert!(gaps::lands_at(4.0, 2.0) < gaps::lands_at(4.0, 1.0));
    // A stop divides nothing: the line stays where its own clock puts it.
    assert_eq!(gaps::lands_at(3.0, 0.0), 3.0);
    assert_eq!(Defect::AtThreeReadings.decided_in().contains("own clock"), true);
}

// --- coverage and cited ids ------------------------------------------------------------------------

#[test]
fn sec_12_decisions_5_gaps_this_audit_closed_tables_cover_every_claim() {
    let rows = gaps::additions();
    assert_eq!(rows.len(), 8);
    for row in &rows {
        assert!(!row.name.is_empty(), "{row:?}");
        assert!(!row.why.is_empty(), "{row:?}");
    }
    // Exactly two MUST fixes, exactly where §5 marks them.
    let fixes: Vec<Defect> = Defect::all().into_iter().filter(|d| d.must_fix()).collect();
    assert_eq!(fixes, vec![Defect::SilentListWipe, Defect::PlacementTagTravels]);
    assert!(!Defect::TitlePrintedOnce.must_fix(), "decided, not mandated");
    assert!(!Defect::AtThreeReadings.must_fix(), "decided, not mandated");
    for d in Defect::all() {
        assert!(!d.decided_in().is_empty(), "{d:?}");
        assert!(!d.closed_by().is_empty(), "{d:?}");
    }
}

#[test]
fn sec_12_decisions_5_gaps_this_audit_closed_cited_ids_answer_from_the_catalogue() {
    fn row(id: &str) -> naivepost::params::Param {
        let mut found = naivepost::params::prepare()
            .into_iter()
            .chain(naivepost::params::cut())
            .chain(naivepost::params::effects())
            .chain(naivepost::params::narrate())
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
        assert!(known.iter().any(|t| t.name() == name), "{name} is not a known tool");
    }
}
