//! F3.10 S1–S5 (`⏩ Speeds`) — the pass's own half of the shell: read the page, telephone, write back.
//!
//! No rule lives here: every decision belongs to [`crate::cut_speed_pass`], the wire leg to [`crate::speeds_ask`]. This module is the seam to the page: which clips make the batch, which captions are on them, how many rounds §S4 allows,
//! and where an answer is written so one ↶ takes it back. [`run_speeds_pass`] dials for real; [`run_speeds_pass_with_reply`] hands the same calls back each round for tests; both run one private body.

use crate::policy;
use crate::ui::clip_pass;
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
    let (rows, lanes) = clip_pass::rows_and_lanes(cut_);
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
    let batch = clip_pass::visible_clips(&cut_);
    let footage = crate::cut_speed_pass::footage_total(&batch);

    let (placed, refusals) = clip_pass::rounds(
        &|attempt| crate::cut_speed_pass::place(&batch, &ask(attempt, &batch)),
        &|_| None,
        crate::cut_speed_pass::rejected,
    );
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
    clip_pass::say_refusals(&mut said, refusals);
    window::log_line(&said);
    window::PassOut {
        said,
        cut: changed.then_some(cut_),
    }
}
