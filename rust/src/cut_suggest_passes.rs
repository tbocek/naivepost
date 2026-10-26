//! F2.14 §S2b's second half — the three passes that run AFTER the cut: captions (F3.9), speeds (F3.10),
//! decorations (F3.11), and only the ones the policy switches on.
//!
//! The flowchart puts them between a `finish_cut` that passed and the walk-back's landing: `M2 → WB`. Until
//! now nothing called them from the Cut page's ▶, so a suggestion stopped at the scenes and the model was
//! never asked for the decorations it is supposed to propose after a cut. This module is that call, kept out
//! of `ui::window` (past its size budget) and out of `cut_suggest_land` (which owns the write, not the ask).
//!
//! Two rules shape it, both from the spec's own words:
//! - **A pass that is off is never called** — "a context that says 'no captions, no speed changes, no
//!   effects' means those jobs are never called, not called and told to return nothing". So [`order`] filters
//!   before anything is dialed; the doors' own grey-out check stays where it is (the run-bar buttons need it),
//!   but it is not what makes this correct.
//! - **Nothing is invented here.** Each door calls its real endpoint and places what came back through
//!   `record_edit`; headless, with no endpoint wired, they say their own "no usable answer" line and place
//!   nothing. That sentence is logged as it stands — a second wording for the same failure would be a second
//!   thing to keep in step.

use crate::policy::{self, Pass};
use crate::project::Policy;
use crate::suggest;

/// The three passes in the order §S2b lists them — captions, then speeds, then decorations — minus every one
/// the policy switched off. Pure: a test can enumerate all eight switch combinations without a window.
pub fn order(policy_: &Policy) -> Vec<Pass> {
    [Pass::Captions, Pass::Speeds, Pass::Decorations]
        .into_iter()
        .filter(|pass| policy::pass_runs(policy_, *pass))
        .collect()
}

/// Ask each switched-on pass, in order, over the cut that just landed, and answer §S3's effect line.
///
/// Every door returns the sentence it said (and prints it on the status line itself), so the sentences go to
/// the log unchanged and the page keeps saying what the pass actually reported. The count is what the cut
/// holds afterwards minus what it held before — the passes place through `record_edit`, so the cut is the
/// only honest place to read the result; a pass that refused, found nothing or was unreachable adds nothing.
pub fn run_after_cut(
    window: &adw::ApplicationWindow,
    policy_: &Policy,
    mut log: impl FnMut(&str),
) -> Vec<String> {
    let passes = order(policy_);
    if passes.is_empty() {
        // Nothing to ask: not a refusal, not a zero-count line. The walk-back already logged the effects it
        // clamped against the footage, and adding "…and 0 effect(s)" here would claim a pass ran.
        return Vec::new();
    }
    let before = crate::ui::review_cut_of(window).fx.len();
    let mut lines: Vec<String> = Vec::new();
    for pass in passes {
        let said = match pass {
            Pass::Captions => crate::ui::run_captions_pass(window),
            Pass::Speeds => crate::ui::run_speeds_pass(window),
            Pass::Decorations => crate::ui::run_effects_pass(window),
        };
        if !said.is_empty() {
            log(&said);
        }
    }
    let after = crate::ui::review_cut_of(window).fx.len();
    // Only ever a gain: a pass places effects, it does not remove them, and a negative count would mean the
    // cut changed underneath for another reason — which is not this line's story to tell.
    let landed = after.saturating_sub(before);
    // The §S3 line is printed only when it counts something this call placed: `press_model` already logged
    // the walk-back's own effect line for the clamped list, and repeating it with the same number would say
    // twice over that the same effects exist.
    if landed > 0 {
        lines.push(suggest::effects_line(landed));
    }
    lines
}

/// [`run_after_cut`]'s asks, walked off the window (§03-shell §1: Suggest's run asks on a worker): each pass
/// over the cut the one before it left, its sentence said as it finishes, exactly as `run_after_cut` says it.
/// Nothing is written: [`land_after_cut`] lands the answers on the window thread, one undo step per pass.
pub fn walk_after_cut(
    inputs: &crate::ui::window::PassInputs,
    cut_: &crate::cut::Cut,
    mut log: impl FnMut(&str),
) -> Vec<crate::ui::window::PassOut> {
    let mut cut_ = cut_.clone();
    let mut outs = Vec::new();
    for pass in order(&inputs.policy) {
        let out = match pass {
            Pass::Captions => crate::ui::window::walk_captions(inputs, &cut_),
            Pass::Speeds => crate::ui::cut_speeds_pass::walk_speeds(inputs, &cut_),
            Pass::Decorations => crate::ui::cut_effects_pass::walk_effects(inputs, &cut_),
        };
        if !out.said.is_empty() {
            log(&out.said);
        }
        if let Some(placed) = &out.cut {
            cut_ = placed.clone();
        }
        outs.push(out);
    }
    outs
}

/// Land what [`walk_after_cut`] walked, in its order, and answer §S3's effect line as [`run_after_cut`] does:
/// what the cut holds afterwards minus what it held before.
pub fn land_after_cut(
    window: &adw::ApplicationWindow,
    outs: &[crate::ui::window::PassOut],
) -> Vec<String> {
    if outs.is_empty() {
        return Vec::new();
    }
    let before = crate::ui::review_cut_of(window).fx.len();
    for out in outs {
        crate::ui::window::land_pass(window, out);
    }
    let after = crate::ui::review_cut_of(window).fx.len();
    let landed = after.saturating_sub(before);
    if landed > 0 {
        vec![suggest::effects_line(landed)]
    } else {
        Vec::new()
    }
}
