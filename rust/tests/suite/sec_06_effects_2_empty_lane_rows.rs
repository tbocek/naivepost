//! §06-effects#2-the-lane-and-the-preview — §2's `lane one row deep even when empty`, as band geometry.
//!
//! The gutter's reach and the lane's seam are arrangement details `spec/img/06-lane.png` shows, not facts this
//! item's text states, so they are judged against the picture and not sampled here. `fx_lane::row_count(&[]) == 1`
//! is already pinned in `cut_six_lane_preview.rs`; what nothing pinned was the height that follows from it.

use naivepost::{cut_trim, fx_lane, ui::cut_band_rows, ui::cut_tracks};

#[test]
fn sec_06_effects_2_the_lane_and_the_preview_s3_an_empty_lane_costs_one_row_of_band_height() {
    let lane_top = cut_band_rows::lane_top_px(cut_trim::RULER_H, cut_tracks::SELECTION_BAND_PX);
    assert_eq!(
        fx_lane::row_count(&[]),
        1,
        "an empty lane is still one row deep (§2)"
    );
    assert_eq!(
        cut_band_rows::pictures_top_px(lane_top, fx_lane::row_count(&[]) as f64),
        cut_trim::RULER_H + cut_tracks::SELECTION_BAND_PX + fx_lane::lane_height_px(1),
        "the picture rows begin one lane row under the selection band"
    );
}
