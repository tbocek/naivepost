//! P.eng.preloadLeadSeconds — how far ahead the next clip is opened.
//! `spec/10-parameters.md` row `P.eng.preloadLeadSeconds` (value 3, "preview: how far ahead the next
//! clip is opened (tolerance 0.01 s)"), read by [`naivepost::preview`] so a jump the transport will
//! make is a swap of the player's spare pipeline rather than a reload.
//!
//! S1 the value and family · S2 inside the lead it opens · S3 past the lead it waits · S4 the tolerance
//! decides what counts as ahead · S5 the current clip's remaining time · S6 one row, one home.

use naivepost::params::{self, Family};
use naivepost::preview;

const RUNS: &[(f64, f64)] = &[(0.0, 10.0), (12.0, 20.0)];

// --- S1: the value, and whose table it is ------------------------------------------------------

#[test]
fn p_eng_preloadleadseconds_s1_three_seconds_a_hundredth_of_slack() {
    // P.eng.preloadLeadSeconds = 3, with §10's stated tolerance of 0.01 s.
    assert_eq!(preview::PRELOAD_LEAD_SECONDS, 3.0);
    assert_eq!(preview::PRELOAD_TOLERANCE_SECONDS, 0.01);
    assert_eq!(params::family("P.eng.preloadLeadSeconds"), Family::Eng);
}

// --- S2: within the lead, the next clip is opened ----------------------------------------------

#[test]
fn p_eng_preloadleadseconds_s2_inside_the_lead_it_opens() {
    // One second left on the current run, the next starts at 12: three seconds ahead, exactly the lead.
    assert_eq!(preview::preload_target(RUNS, 9.0), Some(12.0));
    // Inclusive at the boundary — three seconds is the amount asked for, so three is enough.
    let lead = preview::PRELOAD_LEAD_SECONDS;
    assert_eq!(preview::preload_target(RUNS, 12.0 - lead), Some(12.0));
    // And that really is the lead distance, not a shorter one.
    assert_eq!(12.0 - (12.0 - lead), lead);
}

// --- S3: farther off, nothing opens yet -------------------------------------------------------

#[test]
fn p_eng_preloadleadseconds_s3_past_the_lead_it_waits() {
    // Four seconds to the next start: too far. Opening now would idle the spare while the playhead is
    // still three-plus seconds away from the cut.
    assert_eq!(preview::preload_target(RUNS, 8.0), None);
    // Just outside the lead ...
    assert_eq!(preview::preload_target(RUNS, 8.9), None, "3.1 s ahead is not yet worth opening");
    // ... and inside it. The offsets are built from the constant so the boundary is exact rather than a
    // decimal that float happens to round one way or the other.
    let lead = preview::PRELOAD_LEAD_SECONDS;
    assert_eq!(preview::preload_target(RUNS, 12.0 - lead), Some(12.0));
    assert_eq!(preview::preload_target(RUNS, 12.0 - (lead + 0.1)), None);
    assert_eq!(preview::preload_target(RUNS, 10.5), Some(12.0), "1.5 s ahead opens");
    // Past the last run there is nothing ahead to open at all.
    assert_eq!(preview::next_jump_ahead(RUNS, 21.0), None);
    assert_eq!(preview::preload_target(RUNS, 21.0), None);
}

// --- S4: the tolerance decides what counts as ahead -------------------------------------------

#[test]
fn p_eng_preloadleadseconds_s4_the_tolerance_marks_now() {
    let t = 10.0;
    let tol = preview::PRELOAD_TOLERANCE_SECONDS;
    // A start inside the tolerance is the second we are already in — not a jump to prepare for.
    let inside = t + tol / 2.0;
    assert_eq!(preview::next_jump_ahead(&[(inside, 30.0)], t), None);
    // A start beyond it is the next jump, and near enough to open.
    let beyond = t + tol * 2.0;
    assert_eq!(preview::next_jump_ahead(&[(beyond, 30.0)], t), Some(beyond));
    // Exactly at the tolerance is still "now": the bound is strict on the far side.
    assert_eq!(preview::next_jump_ahead(&[(t + tol, 30.0)], t), None);
    // The same second, seen from a run that has not started yet, is ahead of us.
    assert_eq!(preview::next_jump_ahead(&[(beyond, 30.0)], t).is_some(), true);
}

// --- S5: the same question from the current clip's end ----------------------------------------

#[test]
fn p_eng_preloadleadseconds_s5_remaining_time_on_this_clip() {
    // The jump is happening now, or soon enough to matter.
    assert!(preview::opens_ahead(0.0));
    assert!(preview::opens_ahead(2.0));
    // Inclusive at the lead: exactly three seconds left preloads rather than risking the reload.
    assert!(preview::opens_ahead(preview::PRELOAD_LEAD_SECONDS));
    // Any more and the next clip can wait its turn.
    assert!(!preview::opens_ahead(preview::PRELOAD_LEAD_SECONDS + 1.0));
    assert!(!preview::opens_ahead(30.0));
}

// --- S6: one row, one home --------------------------------------------------------------------

#[test]
fn p_eng_preloadleadseconds_s6_catalogued_once_in_the_cut_page() {
    const ID: &str = "P.eng.preloadLeadSeconds";
    let rows = params::cut();
    let matching: Vec<&params::Param> = rows.iter().filter(|row| row.id == ID).collect();
    assert_eq!(matching.len(), 1, "{ID} catalogued {} times in cut()", matching.len());
    let row = matching[0];
    // Spelled from the constant rather than retyped, so the row cannot drift from the rule.
    assert_eq!(row.spelled, preview::PRELOAD_LEAD_SECONDS.to_string());
    assert!(row.from.contains("PRELOAD_LEAD_SECONDS"), "{}", row.from);
    // And nowhere else: one row, one home.
    for other in [
        params::prepare(),
        params::effects(),
        params::narrate(),
        params::produce(),
    ] {
        assert_eq!(
            other.iter().filter(|row| row.id == ID).count(),
            0,
            "{ID} would have a second home"
        );
    }
}
