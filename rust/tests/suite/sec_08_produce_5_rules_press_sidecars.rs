//! §08-produce#5-rules S2, pressed: the run with something to say writes both sidecar files for every
//! ticked language, and no cue in them is empty.
//!
//! The press — the wire half of the item — is `sec_08_produce_5_rules_press_shared.rs`, compiled here as its
//! own binary because one process affords exactly one press on the real ▶.

const CASE: &str = "sidecars";
#[path = "sec_08_produce_5_rules_press_shared.rs"]
mod press;

#[test]
fn sec_08_produce_5_rules_s2_the_press_writes_both_sidecar_files_for_every_language_ticked() {
    press::run_case();
}
