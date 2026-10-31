//! What the Speeds and Decorations passes share between the page and their rules.

use crate::cut::{Cut, Fx};
use crate::cut_speed_pass::{Answer, Reply};
use crate::tools::clips::Clips;
use crate::ui::window;

/// The clips a pass asks about: the VISIBLE SEGMENTS of the cut, numbered from 1 in play order with their session
/// starts. NOT `cut_play::kept_runs`: it MERGES touching segments for preloading, so three adjoining segments would be
/// one clip to the model and every clip number above 1 would fall outside the batch.
///
/// The captions already on the cut go in too, since the briefs and the refusal rule read them. One hanging across a
/// clip boundary is CLAMPED to the overlap, not skipped, or it would hide from the rules that read it.
pub(crate) fn visible_clips(cut_: &Cut) -> Clips {
    let mut segs: Vec<(f64, f64)> = cut_
        .segs
        .iter()
        .filter(|seg| seg.e > seg.s)
        .map(|seg| (seg.s, seg.e))
        .collect();
    segs.sort_by(|a, b| a.0.total_cmp(&b.0));
    let triples: Vec<(u32, f64, f64)> = segs
        .iter()
        .enumerate()
        .map(|(i, (start, end))| (i as u32 + 1, *start, end - start))
        .collect();
    let mut batch = Clips::new(&triples);
    for fx in cut_.fx.iter().filter(|fx| fx.kind == "text") {
        let clips_now: Vec<(u32, f64, f64)> = batch
            .clips()
            .iter()
            .map(|clip| (clip.n, clip.start, clip.length))
            .collect();
        for (n, start, length) in clips_now {
            let from = fx.t.max(start);
            let to = (fx.t + fx.dur).min(start + length);
            if to - from > 0.0 {
                // Whether a caption fits is not this pass's question, so the tool's answer is not acted on.
                let _ = batch.add_caption(n, from - start, to - start, &fx.text);
            }
        }
    }
    batch
}

/// §9 (`the caption, speed and decoration briefs name no camera or lane`): the camera rows and heard lanes the brief
/// names once there is more than one of either to tell apart.
pub(crate) fn rows_and_lanes(cut_: &Cut) -> (Vec<String>, Vec<String>) {
    let mut rows: Vec<String> = cut_
        .segs
        .iter()
        .filter(|scene| scene.ins.is_empty())
        .map(|scene| (crate::cut_cam::scene_row(scene) + 1).to_string())
        .collect();
    rows.sort_unstable();
    rows.dedup();
    let lanes = cut_.lanes.iter().map(|lane| lane.name.clone()).collect();
    (rows, lanes)
}

/// Two rounds at most, as F3.9: one rejection owes one retry, a third ask never happens. A fault, or what `over_limit`
/// says of a reply's effects, is an UNUSABLE reply and costs the round; a per-call refusal does not. Answers with the
/// effects to place, if any round gave some, and every refusal, which the caller must say.
pub(crate) fn rounds(
    round: &dyn Fn(u32) -> Reply,
    over_limit: &dyn Fn(&[Fx]) -> Option<String>,
    rejected: fn(&str) -> String,
) -> (Option<Vec<Fx>>, Vec<String>) {
    let mut refused = Vec::new();
    for attempt in 1..=2u32 {
        let reply = round(attempt);
        for answer in &reply.answers {
            if let Answer::Refused(reason) = answer {
                refused.push(reason.clone());
            }
        }
        let problem = reply.fault.or_else(|| over_limit(&reply.effects));
        let Some(problem) = problem else {
            return (Some(reply.effects), refused);
        };
        window::log_line(&rejected(&problem));
        if !crate::cut_captions::retries(attempt) {
            break;
        }
    }
    (None, refused)
}

/// A refused call is SAID, in the sentence and the log: dropped silently, the lane shows less than the reply proposed
/// with no reason printed anywhere.
pub(crate) fn say_refusals(said: &mut String, refused: Vec<String>) {
    for reason in refused {
        said.push_str(" \u{2014} ");
        said.push_str(&reason);
        window::log_line(&reason);
    }
}
