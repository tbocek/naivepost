//! F3.10 S1–S5 (`⏩ Speeds`) — the pass's own half of the shell: read the page, telephone, write back.
//!
//! No rule lives here: every decision belongs to [`crate::cut_speed_pass`], the wire leg to [`crate::speeds_ask`]. This module is the seam to the page: which clips make the batch, which captions are on them, how many rounds §S4 allows,
//! and where an answer is written so one ↶ takes it back. [`run_speeds_pass`] dials for real; [`run_speeds_pass_with_reply`] hands the same calls back each round for tests; both run one private body.

use crate::policy;
use crate::ui::window;

/// F3.10 S1–S5: run the speeds pass over this window's newest cut and say what happened.
///
/// Rule-free like [`window::run_captions_pass`]. This seam reads the page, hands the rules their inputs (the kept clips AND the captions already on them: S1's brief notes them and S2 refuses a fast rate over one),
/// and writes back what they answer. Placed speeds go through `record_edit`, NEVER `seed_review_cut`, so one ↶ takes the pass back. Each round asks afresh through `ask_speeds` (§F3.10 S4).
pub fn run_speeds_pass(window: &adw::ApplicationWindow) -> String {
    let inputs = window::pass_inputs(window);
    let cut_ = window::newest_review_cut();
    let out = walk_speeds(&inputs, &cut_);
    window::land_pass(window, &out);
    out.said
}

/// The speeds pass over `cut_`, asking for real, with nothing read off the window: what a worker runs.
pub fn walk_speeds(inputs: &window::PassInputs, cut_: &crate::cut::Cut) -> window::PassOut {
    walk_speeds_with(inputs, cut_, &|round, batch| {
        ask_speeds(inputs, cut_, batch, round)
    })
}

/// F3.10 S2/S4/S5 with a scripted reply: the same route minus the telephone, as `run_captions_pass_with_reply`.
pub fn run_speeds_pass_with_reply(
    window: &adw::ApplicationWindow,
    calls: &[crate::cut_speed_pass::Call],
) -> String {
    let inputs = window::pass_inputs(window);
    let out = walk_speeds_with(&inputs, &window::newest_review_cut(), &|_, _| {
        calls.to_vec()
    });
    window::land_pass(window, &out);
    out.said
}

/// F3.10 S1's leg: the ask over the batch just built, for this round. No calls when there is nothing to ask about or nowhere to ask it (as `window::ask_captions`).
fn ask_speeds(
    inputs: &window::PassInputs,
    cut_: &crate::cut::Cut,
    batch: &crate::tools::clips::Clips,
    attempt: u32,
) -> Vec<crate::cut_speed_pass::Call> {
    let Some(tree) = inputs.tree.as_ref() else {
        window::log_line("!!! speed: no project folder to ask from -- nothing asked");
        return Vec::new();
    };
    let clips: Vec<(u32, f64)> = batch
        .clips()
        .iter()
        .map(|clip| (clip.n, clip.length))
        .collect();
    // Which clips carry captions comes off the batch itself, so the brief says what the refusal will (§F3.10 S1's `N caption(s) -- runs at 1`).
    let captioned: Vec<(u32, usize)> = batch
        .clips()
        .iter()
        .map(|clip| (clip.n, batch.captions_of(clip.n).len()))
        .filter(|(_, count)| *count > 0)
        .collect();
    // §9 (`the caption, speed and decoration briefs name no camera or lane`): named once there is more than one row filming or recording heard.
    let mut rows: Vec<String> = cut_
        .segs
        .iter()
        .filter(|scene| scene.ins.is_empty())
        .map(|scene| (crate::cut_cam::scene_row(scene) + 1).to_string())
        .collect();
    rows.sort_unstable();
    rows.dedup();
    let lanes: Vec<String> = cut_.lanes.iter().map(|lane| lane.name.clone()).collect();
    let asked = crate::speeds_ask::ask_named(
        tree,
        &inputs.context,
        &clips,
        &captioned,
        &rows,
        &lanes,
        attempt,
        &crate::cancel_leg::cancel_check_now(),
    );
    if asked.failed {
        window::log_line(&format!("!!! speed: {reason}", reason = asked.reason));
    }
    asked.calls
}

/// `ask` is called once per round with the batch as it stands, so a retry asks again; the scripted door hands back the same calls every time to exercise the two-round rule.
fn walk_speeds_with(
    inputs: &window::PassInputs,
    cut_: &crate::cut::Cut,
    ask: &dyn Fn(u32, &crate::tools::clips::Clips) -> Vec<crate::cut_speed_pass::Call>,
) -> window::PassOut {
    if !policy::pass_runs(&inputs.policy, policy::Pass::Speeds) {
        return window::PassOut {
            said: "speeds are off \u{2014} the context ruled them out".to_string(),
            cut: None,
        };
    }
    let mut cut_ = cut_.clone();
    let mut changed = false;
    // The clips are the VISIBLE SEGMENTS of the cut, numbered from 1 in play order with their session starts (`Clips` triples, not F3.9's sum). NOT `cut_play::kept_runs`: it MERGES touching segments for preloading,
    // so three adjoining segments would be one clip to the model and every rate for clip 2 or 3 would fall outside the batch.
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
    let mut batch = crate::tools::clips::Clips::new(&triples);
    // S1's brief and S2's refusal read the captions ALREADY on the cut, so they must be in the batch. A text effect across a clip boundary is CLAMPED to the overlap, not skipped,
    // or a caption would hide from the refusal rule and a captioned clip be sped up.
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
    let footage = crate::cut_speed_pass::footage_total(&batch);

    // Two rounds at most, as F3.9: one rejection owes one retry. A fault is an UNUSABLE reply (a clip number never given); a per-call refusal does not cost the round.
    let mut placed: Option<Vec<crate::cut::Fx>> = None;
    let mut refusals: Vec<String> = Vec::new();
    for attempt in 1..=2u32 {
        let reply = crate::cut_speed_pass::place(&batch, &ask(attempt, &batch));
        for answer in &reply.answers {
            if let crate::cut_speed_pass::Answer::Refused(reason) = answer {
                // S2: the refusal MUST be said, or the model believes its speed is in the cut.
                refusals.push(reason.clone());
            }
        }
        if reply.usable() {
            placed = Some(reply.effects.clone());
            break;
        }
        if let Some(problem) = &reply.fault {
            window::log_line(&crate::cut_speed_pass::rejected(problem));
        }
        if !crate::cut_captions::retries(attempt) {
            break;
        }
    }
    let mut said = match placed {
        Some(effects) => {
            if effects.is_empty() {
                // Nothing fast was proposed, which is a right answer, not a failure.
                "nothing runs fast \u{2014} every clip plays at 1".to_string()
            } else {
                cut_.fx.extend(effects.iter().cloned());
                changed = true;
                crate::cut_speed_pass::runs_fast(&batch, &effects, footage)
            }
        }
        None => {
            // §F3.10's own line, spelled by the module; a cut with every clip at 1 is a cut, not a failed run.
            crate::cut_speed_pass::no_answer()
        }
    };
    for reason in refusals {
        // Each refused rate is surfaced so the user sees WHICH clip the model wanted to hurry and why not.
        said.push_str(" \u{2014} ");
        said.push_str(&reason);
        window::log_line(&reason);
    }
    window::log_line(&said);
    window::PassOut {
        said,
        cut: changed.then_some(cut_),
    }
}
