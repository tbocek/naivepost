//! §06-effects#2-the-lane-and-the-preview — where the effects lane's ROWS sit in the band, spelled once.
//!
//! §2's Rows sentence places the lane and orders its rows: the lane "sits between the selection band above and
//! the picture rows below", and its rows are "stacked top to bottom inside the lane", an effect going on "the
//! topmost row where it fits". `cut_tracks::rows_of` already did that arithmetic inline; these three doors are
//! the same sums with names, so a test can ask where a row is without rebuilding the band's layout, and the
//! painter and the reader cannot drift apart.

use crate::fx_lane;

/// The lane's first pixel: under the ruler and the selection band, which are both above it (§2).
pub fn lane_top_px(ruler_h: f64, selection_h: f64) -> f64 {
    ruler_h + selection_h
}

/// Where one lane row begins. Row 0 is the TOP row, so a bigger row number is a bigger y — which is what makes
/// "the topmost row where it fits" mean the smallest painted y rather than the last one drawn (§2).
pub fn lane_row_top_px(lane_top: f64, row: usize) -> f64 {
    lane_top + row as f64 * fx_lane::lane_height_px(1)
}

/// Where the picture rows begin: after every row the lane needed (§2's "picture rows below"). One row deep
/// when empty is `fx_lane::row_count`'s promise, so an empty lane still costs its 26 px here.
pub fn pictures_top_px(lane_top: f64, lane_rows: f64) -> f64 {
    lane_top + lane_rows * fx_lane::lane_height_px(1)
}
