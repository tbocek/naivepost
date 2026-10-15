//! §08-produce#5-rules S2, pressed: a run with nothing to say says so verbatim, writes no sheet, and its
//! stale-sidecar sweep still ran — by exact name, so a language nobody ticked keeps its file.

const CASE: &str = "silent";
#[path = "sec_08_produce_5_rules_press_shared.rs"]
mod press;

#[test]
fn sec_08_produce_5_rules_s2_the_press_that_has_nothing_to_say_says_so_and_sweeps_anyway() {
    press::run_case();
}
