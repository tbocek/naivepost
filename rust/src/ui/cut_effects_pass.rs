//! F3.11 S1–S5 (`✨ Decorations`) — the pass's own half of the shell: read the page, telephone, write back.

use crate::policy;
use crate::tools::clips::Clips;
use crate::ui::clip_pass;
use crate::ui::window;

/// F3.11 S1–S5.
pub fn run_effects_pass(window: &adw::ApplicationWindow) -> String {
    let inputs = window::pass_inputs();
    let cut_ = window::newest_review_cut();
    let out = walk_effects(&inputs, &cut_);
    window::land_pass(window, &out);
    out.said
}

/// The decorations pass over `cut_`, asking for real, with nothing read off the window: what a worker runs.
pub fn walk_effects(inputs: &window::PassInputs, cut_: &crate::cut::Cut) -> window::PassOut {
    walk_effects_with(inputs, cut_, &|round, batch| {
        ask_effects(inputs, cut_, batch, round)
    })
}

pub fn run_effects_pass_with_reply(
    window: &adw::ApplicationWindow,
    calls: &[crate::cut_effects_pass::Call],
) -> String {
    let inputs = window::pass_inputs();
    let out = walk_effects_with(&inputs, &window::newest_review_cut(), &|_, _| {
        calls.to_vec()
    });
    window::land_pass(window, &out);
    out.said
}

fn ask_effects(
    inputs: &window::PassInputs,
    cut_: &crate::cut::Cut,
    batch: &Clips,
    attempt: u32,
) -> Vec<crate::cut_effects_pass::Call> {
    let Some(tree) = inputs.tree.as_ref() else {
        window::log_line("!!! effects: no project folder to ask from -- nothing asked");
        return Vec::new();
    };
    // S1 says the call is told the captions AND the speeds, and both come off the batch itself so the brief
    // cannot disagree with the rules that read it. `effects_ask::user_message` lays the brief out.
    let rates = clip_rates(batch);
    let (rows, lanes) = clip_pass::rows_and_lanes(cut_);
    let asked = crate::effects_ask::ask_named(
        tree,
        &inputs.context,
        batch,
        &rates,
        &rows,
        &lanes,
        attempt,
        &crate::cancel_leg::cancel_check_now(),
    );
    if asked.failed {
        window::log_line(&format!("!!! effects: {reason}", reason = asked.reason));
    }
    asked.calls
}

fn walk_effects_with(
    inputs: &window::PassInputs,
    cut_: &crate::cut::Cut,
    ask: &dyn Fn(u32, &Clips) -> Vec<crate::cut_effects_pass::Call>,
) -> window::PassOut {
    if !policy::pass_runs(&inputs.policy, policy::Pass::Decorations) {
        return window::PassOut {
            said: "decorations are off \u{2014} the context ruled them out".to_string(),
            cut: None,
        };
    }
    let mut cut_ = cut_.clone();
    let mut changed = false;
    let batch = clip_pass::visible_clips(&cut_);
    // How many decorations this pass ended up placing, kept for the density sentence below (`None` is a pass
    // that never got an usable answer, which has nothing to weigh against the guide).
    let mut placed_count: Option<usize> = None;
    // P.machine.maxProposedEffects: a runaway answer is unusable in the same way a bad clip number is. The density
    // guide only says; this cap refuses.
    let (placed, skipped) = clip_pass::rounds(
        &|attempt| crate::cut_effects_pass::place(&batch, &ask(attempt, &batch)),
        &|effects| {
            let verdict = crate::cut_effects_pass::effect_cap_allows_reply(effects);
            (!verdict.allowed).then_some(verdict.reason)
        },
        crate::cut_effects_pass::rejected,
    );
    let mut said = match placed {
        Some(effects) => {
            if effects.is_empty() {
                // Nothing landed, which is a right answer to a cut that wants no decoration (every gain was 1,
                // every span empty) rather than a failed run -- so it is not [`cut_effects_pass::no_answer`]'s
                // sentence, which promises the model was asked and gave nothing usable.
                placed_count = Some(0);
                "nothing proposed \u{2014} the cut stands as it is".to_string()
            } else {
                placed_count = Some(effects.len());
                cut_.fx.extend(effects.iter().cloned());
                changed = true;
                crate::cut_effects_pass::placed(effects.len())
            }
        }
        None => crate::cut_effects_pass::no_answer(),
    };
    clip_pass::say_refusals(&mut said, skipped);
    // §06-effects#6's "decorations density (prompt)", reported against what this pass actually placed.
    if let Some(count) = placed_count {
        // The finished length is the cut's own total, through the same door the page's "Cut" readout uses
        // (`cut_screen::cut_seconds`), so the seconds the guide is quoted against are the seconds the screen shows.
        let seconds = crate::cut_screen::cut_seconds(&cut_);
        let (low, high) = crate::cut_effects_pass::expected_decorations(seconds);
        if crate::cut_effects_pass::above_density_guide(count, seconds) {
            said.push_str(&format!(
                " \u{2014} past the guide: {} ({}-{high} for {})",
                crate::cut_effects_pass::DENSITY_GUIDE,
                low,
                crate::tools::mm_ss(seconds)
            ));
        }
    }
    window::log_line(&said);
    window::PassOut {
        said,
        cut: changed.then_some(cut_),
    }
}

fn clip_rates(batch: &Clips) -> Vec<(u32, f64)> {
    let mut rates: Vec<(u32, f64)> = Vec::new();
    for clip in batch.clips() {
        // The clip's own rate field, set by `set_clip_speed` or by a speed effect whose span covers it. A stop
        // reports 0 and a fast clip its rate; anything else stays unlisted.
        if clip.rate != 1.0 {
            rates.push((clip.n, clip.rate));
        }
    }
    rates
}
