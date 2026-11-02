//! §F3.9 / §F3.10 / §F3.11 — the ✐ Captions, ⏩ Speeds and ✨ Decorations passes that run AFTER the cut, and
//! the policy switch each one reads before it is asked. Split out of `window.rs`, which is past its size budget;
//! its doors are re-exported through `window`. None of them has a control: F3.9-F3.11 are jobs of the run after
//! the cut (`cut_suggest_land` → `cut_suggest_passes::run_after_cut`), not buttons on the run bar.

use crate::cut_play;
use crate::hand_edit;
use crate::policy;

use adw::prelude::*;

use crate::ui::window as ui;
use crate::ui::window::refresh_effects_lane;
use crate::ui::window_page_state::{WindowPage, PAGE as WINDOW_PAGE};

/// F3.9 S4's second door: what the model answered for one batch.
pub struct CaptionBatch {
    /// `(clip number, length)` for the clips this batch was about, in clip order.
    pub clips: Vec<(u32, f64)>,
    /// What came back: the `add_caption` calls, before any validation.
    pub calls: Vec<crate::cut_captions::Call>,
}

/// F3.10's wording — the name the job is called by in the log.
#[allow(dead_code)]
pub const SPEEDS_PASS_LABEL: &str = "\u{23e9} Speeds";

/// What the three passes read off the window before they run, so their walks need nothing else from it and can
/// run on a worker (§03-shell §1, "The window thread never waits"): the policy whose switches gate them, the User
/// Context their briefs carry, the project folder they ask from, and the sources the captions' words come from.
#[derive(Debug, Clone)]
pub struct PassInputs {
    pub policy: crate::project::Policy,
    pub context: String,
    pub tree: Option<crate::layout::Tree>,
    pub sources: Vec<String>,
}

/// Read [`PassInputs`] off the window, on the window thread.
pub fn pass_inputs() -> PassInputs {
    PassInputs {
        policy: crate::ui::window_policy::live_session_policy(),
        context: ui::user_context(),
        tree: ui::narrate_session_tree(),
        sources: ui::session_sources(),
    }
}

/// Where a pass's walk ended: the sentence for the status line, and the cut to write when it placed something.
#[derive(Debug, Clone)]
pub struct PassOut {
    pub said: String,
    pub cut: Option<crate::cut::Cut>,
}

/// Land one pass's walk on the window: the cut written and recorded as its own undo step (one ↶ takes the whole
/// pass back), the effects lane redrawn, the sentence on the status line.
pub fn land_pass(window: &adw::ApplicationWindow, out: &PassOut) {
    if let Some(cut_) = &out.cut {
        ui::save_insert_cut(cut_);
        ui::record_edit(window, cut_);
        refresh_effects_lane(window);
    }
    if let Some(status_line) = ui::find_status(window.upcast_ref()) {
        status_line.set_text(&out.said);
    }
}

/// F3.9 S1–S6: run the captions pass over this window's newest cut and say what happened.
pub fn run_captions_pass(window: &adw::ApplicationWindow) -> String {
    let inputs = pass_inputs();
    let out = walk_captions(&inputs, &ui::newest_review_cut());
    land_pass(window, &out);
    out.said
}

/// The captions pass over `cut_`, asking for real, with nothing read off the window: what a worker runs.
pub fn walk_captions(inputs: &PassInputs, cut_: &crate::cut::Cut) -> PassOut {
    if !policy::pass_runs(&inputs.policy, policy::Pass::Captions) {
        return PassOut {
            said: "captions are off \u{2014} the context ruled them out".to_string(),
            cut: None,
        };
    }
    // The clips the pass asks about are the KEPT runs of the cut, numbered from 1 in play order: the model is
    // shown one clip at a time and cannot know where it sits in the session, which is why it answers in offsets
    // and this module turns those into seconds (`spec/00-principles.md`).
    let runs = cut_play::kept_runs(&cut_.segs);
    if runs.is_empty() {
        return PassOut {
            said: "nothing kept to caption".to_string(),
            cut: None,
        };
    }
    let ranges = crate::cut_captions::batches(runs.len());
    let mut placed_total = 0usize;
    let mut skipped_ranges: Vec<String> = Vec::new();
    let mut cut_ = cut_.clone();
    for (first, last) in ranges {
        let count = (last - first + 1) as usize;
        let clips: Vec<(u32, f64)> = runs
            .iter()
            .skip((first - 1) as usize)
            .take(count)
            .map(|(start, end)| ((first..=last).next().unwrap_or(first), *end - *start))
            .enumerate()
            .map(|(i, (_, length))| (first + i as u32, length))
            .collect();
        let mut accepted: Option<Vec<crate::cut::Fx>> = None;
        for runs_so_far in 1..=2u32 {
            let reply = ask_captions(inputs, &clips, &runs, runs_so_far);
            match crate::cut_captions::place(&clips, &reply) {
                crate::cut_captions::Reply::Accepted(fx) => {
                    accepted = Some(fx);
                    break;
                }
                crate::cut_captions::Reply::Rejected(problem) => {
                    ui::log_line(&format!(">>> captions: {problem}"));
                    // `retries(runs)` says whether another round is owed: true after ONE rejection only.
                    if !crate::cut_captions::retries(runs_so_far) {
                        break;
                    }
                }
            }
        }
        match accepted {
            Some(fx) => {
                placed_total += fx.len();
                cut_.fx.extend(fx);
            }
            None => skipped_ranges.push(format!("{first}\u{2013}{last}")),
        }
    }
    let changed = placed_total > 0 || !skipped_ranges.is_empty();
    let mut said = format!("{} caption(s) placed", placed_total);
    for range in skipped_ranges {
        // §F3.9's own sentence, verbatim, once per batch that would not answer.
        let (a, b) = range.split_once('\u{2013}').unwrap_or(("0", "0"));
        let line = crate::cut_captions::skipped(a.parse().unwrap_or(0), b.parse().unwrap_or(0));
        ui::log_line(&line);
        said.push_str(" \u{2014} ");
        said.push_str(&line);
    }
    PassOut {
        said,
        cut: changed.then_some(cut_),
    }
}

/// F3.9 S2/S3: ask one batch of clips of the model, over HTTP at the address the settings hold
/// (re-read per call).
fn ask_captions(
    inputs: &PassInputs,
    clips: &[(u32, f64)],
    runs: &[(f64, f64)],
    attempt: u32,
) -> Vec<crate::cut_captions::Call> {
    let Some(tree) = inputs.tree.as_ref() else {
        ui::log_line("!!! captions: no project folder to read words from -- nothing asked");
        return Vec::new();
    };
    let (words, times) = hand_edit::session_words(tree, &inputs.sources);
    let offsets = crate::captions_ask::offsets_in_clips(runs, &words, &times);
    let asked = crate::captions_ask::ask(
        tree,
        &inputs.context,
        clips,
        &offsets,
        attempt,
        &crate::cancel_leg::cancel_check_now(),
    );
    if asked.failed {
        ui::log_line(&format!("!!! captions: {reason}", reason = asked.reason));
    }
    asked.calls
}
/// F3.9 S4/S5/S6 with a scripted reply: the same route the pass takes, minus the telephone.
pub fn run_captions_pass_with_reply(
    window: &adw::ApplicationWindow,
    batches: &[CaptionBatch],
) -> String {
    if !policy::pass_runs(
        &crate::ui::window_policy::live_session_policy(),
        policy::Pass::Captions,
    ) {
        return "captions are off \u{2014} the context ruled them out".to_string();
    }
    let mut placed_total = 0usize;
    let mut skipped_ranges: Vec<(u32, u32)> = Vec::new();
    let mut cut_ = ui::newest_review_cut();
    for batch in batches {
        let (first, last) = (
            batch.clips.first().map(|(n, _)| *n).unwrap_or(0),
            batch.clips.last().map(|(n, _)| *n).unwrap_or(0),
        );
        let mut accepted: Option<Vec<crate::cut::Fx>> = None;
        for runs_so_far in 1..=2u32 {
            match crate::cut_captions::place(&batch.clips, &batch.calls) {
                crate::cut_captions::Reply::Accepted(fx) => {
                    accepted = Some(fx);
                    break;
                }
                crate::cut_captions::Reply::Rejected(problem) => {
                    ui::log_line(&format!(">>> captions: {problem}"));
                    if !crate::cut_captions::retries(runs_so_far) {
                        break;
                    }
                }
            }
        }
        match accepted {
            Some(fx) => {
                placed_total += fx.len();
                cut_.fx.extend(fx);
            }
            None => skipped_ranges.push((first, last)),
        }
    }
    if placed_total > 0 || !skipped_ranges.is_empty() {
        ui::save_insert_cut(&cut_);
        ui::record_edit(window, &cut_);
        refresh_effects_lane(window);
    }
    let mut said = format!("{} caption(s) placed", placed_total);
    for (a, b) in skipped_ranges {
        let line = crate::cut_captions::skipped(a, b);
        ui::log_line(&line);
        said.push_str(" \u{2014} ");
        said.push_str(&line);
    }
    if let Some(status_line) = ui::find_status(window.upcast_ref()) {
        status_line.set_text(&said);
    }
    said
}
/// F3.9: set this session's captions gate directly, the same way `set_marking_pass` sets its switch.
pub fn set_captions_pass_for_test(on: bool) {
    if let Some(session) =
        WINDOW_PAGE.with(|WindowPage { session: slots, .. }| slots.borrow().last().cloned())
    {
        session.borrow_mut().policy.captions_pass.value = on;
    }
}
/// F3.10: move this session's speeds gate directly, the way [`set_captions_pass_for_test`] moves its own.
pub fn set_speeds_pass_for_test(on: bool) {
    if let Some(session) =
        WINDOW_PAGE.with(|WindowPage { session: slots, .. }| slots.borrow().last().cloned())
    {
        session.borrow_mut().policy.speed_pass.value = on;
    }
}
/// F3.11: move this session's decorations gate directly, the way [`set_captions_pass_for_test`] moves its own.
pub fn set_decorations_pass_for_test(on: bool) {
    if let Some(session) =
        WINDOW_PAGE.with(|WindowPage { session: slots, .. }| slots.borrow().last().cloned())
    {
        session.borrow_mut().policy.decorations_pass.value = on;
    }
}
