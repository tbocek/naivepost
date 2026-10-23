//! §08-produce#5-rules S2, pressed: "stale ones deleted first" seen as content — the ticked language's old
//! file is gone and the one beside the new video holds this run's cues.

const CASE: &str = "replaced";
#[path = "sec_08_produce_5_rules_press_shared.rs"]
mod press;

#[test]
fn sec_08_produce_5_rules_s2_a_stale_sidecar_of_the_ticked_language_is_replaced_not_left_beside_the_new_video(
) {
    press::run_case();
}
