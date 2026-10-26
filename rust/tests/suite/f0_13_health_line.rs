//! §03-shell.md F0.13: the audio row's verdict says how long `/health` took, in the words the row prints.

use naivepost::{checks, services};

const ITEM: &str = "f0_13";

#[test]
fn f0_13_s18_the_audio_row_says_how_long_health_took() {
    let models = vec![services::AudioModel {
        id: "indextts2".to_string(),
        family: "index_tts2".to_string(),
        task: "clon".to_string(),
    }];
    let line =
        checks::audio_health_verdict(412, &models).expect("{ITEM} S18: a healthy server passes");
    assert!(
        line.contains("healthy in 412 ms"),
        "{ITEM} S18: the verdict spells the measured milliseconds: {line:?}"
    );
    assert!(
        line.contains("indextts2"),
        "{ITEM} S18: and names the voice it will narrate with: {line:?}"
    );
    // Zero is what the stand-in used to answer with; a timed leg can still be that fast, so the rule
    // this pins is the WORDING, which only a real measurement can fill in.
    assert!(
        checks::audio_health_verdict(0, &[]).is_err(),
        "{ITEM} S18: health without a catalogue is still a refusal"
    );
}
