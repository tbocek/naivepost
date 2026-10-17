//! F5.6 S4: the three frames that fill the image row when nothing was chosen and no frame was picked.
//!
//! §F5.6 says "3 frames evenly spread over the kept footage". Only the caller holds the live cut, so this
//! module works in moments on the PRODUCED clock and the page turns a moment into whatever it has on disk;
//! [`crate::produce_upload::walk`] just puts what it is given into the row.

use crate::cut::Seg;

/// How many frames the empty row is filled with — the number §F5.6's log line names.
pub const SPREAD_COUNT: usize = 3;

/// A segment's length in OUTPUT seconds: rate applied once (`narration::output_seconds`' own rule), with
/// the splice's explicit `dur` winning over the footage span when it has one. Same arithmetic the Produce
/// page uses for clip starts, so a moment here and a moment there mean the same instant.
fn output_length(seg: &Seg) -> f64 {
    let raw = if seg.dur > 0.0 { seg.dur } else { (seg.e - seg.s).max(0.0) };
    crate::narration::output_seconds(raw, seg.rate)
}

/// The kept FOOTAGE stretches on the produced clock, as `(start, end)` pairs: inserts contribute no
/// footage (§F5.6 S1's brief rule, reused here), and a stretch of no length contributes nothing either, so
/// a freeze frame cannot be one of the three pictures of the video.
pub fn kept_footage(segs: &[Seg]) -> Vec<(f64, f64)> {
    let mut spans = Vec::with_capacity(segs.len());
    let mut at = 0.0;
    for seg in segs {
        let kept = output_length(seg);
        if seg.ins.is_empty() && kept > 0.0 {
            spans.push((at, at + kept));
        }
        at += kept;
    }
    spans
}

/// The `SPREAD_COUNT` moments evenly spread over the kept footage: the middles of three equal bands over the
/// whole kept span, which is what "evenly spread" means for a video whose clips have different lengths —
/// three per clip would show the start of the video three times over on a session that keeps one long take.
/// Empty when nothing was kept, which is the caller's signal to draw from words alone.
pub fn spread_moments(segs: &[Seg]) -> Vec<f64> {
    let spans = kept_footage(segs);
    let total: f64 = spans.iter().map(|(from, to)| to - from).sum();
    if total <= 0.0 {
        return Vec::new();
    }
    // Flatten the kept stretches onto one continuous ruler, then read each band middle back off it: a
    // moment inside the second clip is that clip's start plus how far into the kept total it sits.
    let mut out = Vec::with_capacity(SPREAD_COUNT);
    for band in 0..SPREAD_COUNT {
        let want = total * (2.0 * band as f64 + 1.0) / (2.0 * SPREAD_COUNT as f64);
        let mut walked = 0.0;
        for (from, to) in &spans {
            let len = to - from;
            if want < walked + len {
                out.push(from + (want - walked));
                break;
            }
            walked += len;
        }
    }
    out
}

/// The nearest extracted moment to `at`, reusing the ONE nearest rule (§3.9/decision 32) so a spread frame
/// and a picked frame resolve the same way.
pub fn frame_at(moments: &[f64], at: f64) -> Option<f64> {
    crate::produce_upload::nearest(moments, at)
}

/// §3.9's `get_frames(clip, at?)`: the candidate frames inside one clip, so the picture is chosen by
/// LOOKING rather than by position (the prototype showed the writer none and picked band middles itself).
/// With `at` given the answer narrows to a window around that moment, and the window is the PROJECT's
/// `P.project.frameInterval` (`interval`, passed in by the caller) — that is how far apart two frames
/// ever are, so a window of one interval always holds a neighbour. A fixed stop here would name frames
/// wider or nearer than the ones this session actually extracted.
/// An unknown span answers everything rather than nothing: showing no frames would read as an empty video.
pub fn get_frames(
    moments: &[f64],
    from: f64,
    to: f64,
    at: Option<f64>,
    interval: f64,
) -> Vec<f64> {
    let inside = moments.iter().copied().filter(|at| *at >= from && *at < to).collect::<Vec<f64>>();
    let Some(at) = at else {
        return inside;
    };
    let half = interval;
    let mut window = inside
        .into_iter()
        .filter(|moment| (moment - at).abs() <= half)
        .collect::<Vec<f64>>();
    window.sort_by(|a, b| (a - at).abs().partial_cmp(&(b - at).abs()).unwrap_or(std::cmp::Ordering::Equal));
    window.truncate(SPREAD_COUNT);
    window
}

/// The row entry a moment becomes in `publish.frames`. The `frame:` spelling is the one the model's own
/// `THUMBNAIL: frame: clip n +s` line and `produce_details` use, so a spread frame looks like any other row
/// entry to everything downstream.
pub fn row_entry(at: f64) -> String {
    format!("frame:{at:.1}")
}

/// The three row entries for a cut: [`spread_moments`] rendered through [`row_entry`].
pub fn spread_rows(segs: &[Seg]) -> Vec<String> {
    spread_moments(segs).iter().map(|at| row_entry(*at)).collect()
}
