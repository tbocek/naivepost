//! §06-effects#8 bullet 4: the one question the encode path asks about a planned
//! 1× read head — may this run's sound be read back at all?
//!
//! The two halves of the answer stay where they are owned: the 0.05 s worth-dipping
//! threshold in [`crate::produce_details::drifts`] and the card / held frame / no
//! recording rule in [`crate::produce_details::run_closes`]. This module only asks
//! both, because the encode path needs ONE yes before it spends two `afade`s on a
//! join nobody would hear.

use crate::fx_lane;
use crate::produce_details as details;

/// Does the read head open? §8: it opens only where the sound would drift by at
/// least `machine.driftSeconds` AND the run is not closed by what the clip is.
///
/// `planned` is the footage's own seconds, `actual` the seconds the picture will
/// show — the pair [`crate::produce_render::read_head_plan`] names a run for.
pub fn opens(planned: f64, actual: f64, source_is_footage: bool, on_recording: bool) -> bool {
    source_is_footage
        && on_recording
        && details::drifts(planned, actual)
        && fx_lane::debt(planned, actual).abs() >= fx_lane::DEBT_TAIL_MIN_SECONDS
}

/// The source question, spelled once: an insert (a card, a copy) films nothing,
/// plain footage does. Same test [`crate::cut::Seg::insert_asset`] would make.
pub fn source_is_footage(seg: &crate::cut::Seg) -> bool {
    seg.ins.is_empty()
}

/// Is the clip on tape? `runs` are the filmed stretches, the same
/// [`crate::timeline::filmed_runs`] output the Cut page reads its debt tail from,
/// so the preview and the render answer the question with one arithmetic.
pub fn on_recording(seg: &crate::cut::Seg, runs: &[(f64, f64)]) -> bool {
    runs.iter().any(|(start, end)| start < &seg.e && seg.s < *end)
}
