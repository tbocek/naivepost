//! §F3.9 / §F3.10 / §F3.11 — the ✐ Captions, ⏩ Speeds and ✨ Decorations passes that run AFTER the cut, and
//! the policy switch each one reads before it is asked. Split out of `window.rs`, which is past its size budget;
//! its doors are re-exported through `window`. None of them has a control: F3.9-F3.11 are jobs of the run after
//! the cut (`cut_suggest_land` → `cut_suggest_passes::run_after_cut`), not buttons on the run bar.
//!
//! Deliberately rule-free: every decision lives in `cut_captions`, `speeds_ask` and `effects_ask`; what is
//! here gathers this window's inputs, runs the pass, and says what came back.

use crate::cut_play;
use crate::hand_edit;
use crate::policy;

use adw::prelude::*;

use crate::ui::window as ui;
use crate::ui::window::refresh_effects_lane;

/// F3.9 S4's second door: what the model answered for one batch. `place` is a pure function of the reply, so
/// the page can either collect calls from a live tool round or be handed them by a test that never dials a model
/// — the same split `text_drag_ended_with_source` makes for F3.4.
pub struct CaptionBatch {
    /// `(clip number, length)` for the clips this batch was about, in clip order.
    pub clips: Vec<(u32, f64)>,
    /// What came back: the `add_caption` calls, before any validation.
    pub calls: Vec<crate::cut_captions::Call>,
}

/// F3.10's wording — the name the job is called by in the log. No control carries it: F3.10 runs as a job of
/// the run after the cut, so nothing outside this module reads it.
#[allow(dead_code)]
pub const SPEEDS_PASS_LABEL: &str = "\u{23e9} Speeds";

/// F3.9 S1–S6: run the captions pass over this window's newest cut and say what happened.
///
/// Deliberately rule-free: every decision lives in [`crate::cut_captions`] (`batches`, `place`, `retries`,
/// `skipped`). This seam only reads the page, feeds the rules, and writes what they answer. Accepted captions go
/// through `ui::record_edit` and NEVER `seed_review_cut`, so one ↶ takes the whole pass back exactly as every other
/// Apply path does; a batch whose reply was rejected twice logs `cut_captions::skipped` and the run carries on,
/// because captions are worth having and not worth failing a run over.
pub fn run_captions_pass(window: &adw::ApplicationWindow) -> String {
    if !policy::pass_runs(&ui::session_policy(window), policy::Pass::Captions) {
        let said = "captions are off \u{2014} the context ruled them out".to_string();
        if let Some(status_line) = ui::find_status(window.upcast_ref()) {
            status_line.set_text(&said);
        }
        return said;
    }
    // The clips the pass asks about are the KEPT runs of the cut, numbered from 1 in play order: the model is
    // shown one clip at a time and cannot know where it sits in the session, which is why it answers in offsets
    // and this module turns those into seconds (`spec/00-principles.md`).
    let runs = cut_play::kept_runs(&ui::review_cut_segs(window));
    if runs.is_empty() {
        let said = "nothing kept to caption".to_string();
        if let Some(status_line) = ui::find_status(window.upcast_ref()) {
            status_line.set_text(&said);
        }
        return said;
    }
    let ranges = crate::cut_captions::batches(runs.len());
    let mut placed_total = 0usize;
    let mut skipped_ranges: Vec<String> = Vec::new();
    let mut cut_ = ui::newest_review_cut();
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
        // One attempt, then the retry `retries` allows, then give up on THIS batch only.
        let mut accepted: Option<Vec<crate::cut::Fx>> = None;
        for runs_so_far in 1..=2u32 {
            let reply = ask_captions(
                window,
                &ui::user_context(window),
                &clips,
                &runs,
                runs_so_far,
            );
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
    if placed_total > 0 || !skipped_ranges.is_empty() {
        ui::save_insert_cut(&cut_);
        ui::record_edit(window, &cut_);
        refresh_effects_lane(window);
    }
    let mut said = format!("{} caption(s) placed", placed_total);
    for range in skipped_ranges {
        // §F3.9's own sentence, verbatim, once per batch that would not answer.
        let (a, b) = range.split_once('\u{2013}').unwrap_or(("0", "0"));
        let line = crate::cut_captions::skipped(a.parse().unwrap_or(0), b.parse().unwrap_or(0));
        ui::log_line(&line);
        said.push_str(" \u{2014} ");
        said.push_str(&line);
    }
    if let Some(status_line) = ui::find_status(window.upcast_ref()) {
        status_line.set_text(&said);
    }
    ui::log_line(&said);
    said
}
/// F3.9 S2/S3: ask one batch of clips of the model, over HTTP at the address the settings hold
/// (re-read per call). `captions_ask::ask` returns the captions it asked for as clip-relative calls,
/// which `cut_captions::place` then lets §3.7's `add_caption` tool judge. The word lines are read off
/// disk (`hand_edit::session_words`) and turned into offsets inside each kept clip by
/// `captions_ask::offsets_in_clips`, so the model never sees a session second. A wire or parse failure
/// is logged where it happened (§00-principles: name the reason) and answers with no calls, leaving this
/// batch to its own retry/skip rule.
fn ask_captions(
    window: &adw::ApplicationWindow,
    context: &str,
    clips: &[(u32, f64)],
    runs: &[(f64, f64)],
    attempt: u32,
) -> Vec<crate::cut_captions::Call> {
    let Some(tree) = ui::narrate_session_tree() else {
        ui::log_line("!!! captions: no project folder to read words from -- nothing asked");
        return Vec::new();
    };
    let (words, times) = hand_edit::session_words(&tree, &ui::session_sources(window));
    let offsets = crate::captions_ask::offsets_in_clips(runs, &words, &times);
    let asked = crate::captions_ask::ask(
        &tree,
        context,
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
/// F3.9 S4/S5/S6 with a scripted reply: the same route the pass takes, minus the telephone. Feeding a batch
/// through here exercises the whole rule set (membership, floor, fades, the two-retry limit) against real page
/// state, which is what lets the widget test fire a click and read the lane back.
pub fn run_captions_pass_with_reply(
    window: &adw::ApplicationWindow,
    batches: &[CaptionBatch],
) -> String {
    if !policy::pass_runs(&ui::session_policy(window), policy::Pass::Captions) {
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
/// F3.9: set this session's captions gate directly, the same way `set_marking_pass` sets its switch. A test
/// needs to move the gate AFTER the window exists (a build-time value can be replaced before the handler sees
/// it), and the refusal must follow the very field the pass reads rather than a second copy of the decision.
/// F3.9: move this session's captions gate directly. `_for_test` because nothing in the program calls it —
/// the switches are written by `policy::apply` (F0.7) and read by `policy::pass_runs`; a test that wants the
/// gate off has to move the field itself, and saying so in the name keeps it from being mistaken for a door.
pub fn set_captions_pass_for_test(window: &adw::ApplicationWindow, on: bool) {
    let _ = window;
    if let Some(session) = ui::SESSION.with(|slots| slots.borrow().last().cloned()) {
        session.borrow_mut().policy.captions_pass.value = on;
    }
}
/// F3.10: move this session's speeds gate directly, the way [`set_captions_pass_for_test`] moves its own.
pub fn set_speeds_pass_for_test(window: &adw::ApplicationWindow, on: bool) {
    let _ = window;
    if let Some(session) = ui::SESSION.with(|slots| slots.borrow().last().cloned()) {
        session.borrow_mut().policy.speed_pass.value = on;
    }
}
/// F3.11: move this session's decorations gate directly, the way [`set_captions_pass_for_test`] moves its own.
pub fn set_decorations_pass_for_test(window: &adw::ApplicationWindow, on: bool) {
    let _ = window;
    if let Some(session) = ui::SESSION.with(|slots| slots.borrow().last().cloned()) {
        session.borrow_mut().policy.decorations_pass.value = on;
    }
}
