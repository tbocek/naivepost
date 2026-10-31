//! F5.6 S4: the three frames that fill the image row when nothing was chosen and no frame was picked.
//!
//! §F5.6 says "3 frames evenly spread over the kept footage". Only the caller holds the live cut, so this works in moments on the PRODUCED clock and the page turns a moment into what it has on disk; [`crate::produce_upload::walk`] just puts them in the row.

use crate::cut::Seg;

/// How many frames the empty row is filled with — the number §F5.6's log line names.
pub const SPREAD_COUNT: usize = 3;

/// A segment's length in OUTPUT seconds: rate applied once (`narration::output_seconds`' rule), the splice's explicit `dur` winning over the footage span; same arithmetic as the Produce page's clip starts.
fn output_length(seg: &Seg) -> f64 {
    let raw = if seg.dur > 0.0 {
        seg.dur
    } else {
        (seg.e - seg.s).max(0.0)
    };
    crate::narration::output_seconds(raw, seg.rate)
}

/// The `SPREAD_COUNT` moments evenly spread over the kept footage: the middles of equal bands over the whole kept span (three per clip would show the start three times on one long take). Empty when nothing was kept.
pub fn spread_moments(segs: &[Seg]) -> Vec<f64> {
    let kept: Vec<(&Seg, f64)> = segs
        .iter()
        .filter(|seg| seg.ins.is_empty())
        .map(|seg| (seg, output_length(seg)))
        .filter(|(_, len)| *len > 0.0)
        .collect();
    let total: f64 = kept.iter().map(|(_, len)| len).sum();
    if total <= 0.0 {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(SPREAD_COUNT);
    for band in 0..SPREAD_COUNT {
        let want = total * (2.0 * band as f64 + 1.0) / (2.0 * SPREAD_COUNT as f64);
        let mut walked = 0.0;
        for (seg, len) in &kept {
            if want < walked + len {
                // The produced seconds into this clip, back onto its recording's seconds.
                let footage = (seg.e - seg.s).max(0.0);
                out.push(seg.s + (want - walked) * footage / len);
                break;
            }
            walked += len;
        }
    }
    out
}

/// The nearest extracted moment to `at`, by the ONE nearest rule (§3.9/decision 32), so a spread frame and a picked frame resolve alike.
pub fn frame_at(moments: &[f64], at: f64) -> Option<f64> {
    crate::produce_upload::nearest(moments, at)
}

/// §3.9's `get_frames(clip, at?)`: the candidate frames inside one clip, so the picture is chosen by LOOKING. With `at` the answer narrows to a window of the PROJECT's `P.project.frameInterval` (`interval`, from the caller),
/// which always holds a neighbour; a fixed stop would name frames wider or nearer than the session extracted. An unknown span answers everything: no frames would read as an empty video.
pub fn get_frames(moments: &[f64], from: f64, to: f64, at: Option<f64>, interval: f64) -> Vec<f64> {
    let inside = moments
        .iter()
        .copied()
        .filter(|at| *at >= from && *at < to)
        .collect::<Vec<f64>>();
    let Some(at) = at else {
        return inside;
    };
    let half = interval;
    let mut window = inside
        .into_iter()
        .filter(|moment| (moment - at).abs() <= half)
        .collect::<Vec<f64>>();
    window.sort_by(|a, b| {
        (a - at)
            .abs()
            .partial_cmp(&(b - at).abs())
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    window.truncate(SPREAD_COUNT);
    window
}

/// The row entry a moment becomes in `publish.frames`: the `frame:` spelling the model's `THUMBNAIL: frame: clip n +s` line and `produce_details` use.
pub fn row_entry(at: f64) -> String {
    format!("frame:{at:.1}")
}

/// The three row entries for a cut: [`spread_moments`] rendered through [`row_entry`].
pub fn spread_rows(segs: &[Seg]) -> Vec<String> {
    spread_moments(segs)
        .iter()
        .map(|at| row_entry(*at))
        .collect()
}
