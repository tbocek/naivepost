//! §08-produce#5-rules S2, pressed: webm carries no subtitle track, so after the mux the run says where its
//! subtitles actually are, and the files it points at exist beside the video.

const CASE: &str = "webm";
#[path = "sec_08_produce_5_rules_press_shared.rs"]
mod press;

#[test]
fn sec_08_produce_5_rules_s2_the_press_with_a_webm_encode_says_where_its_subtitles_are() {
    press::run_case();
}
