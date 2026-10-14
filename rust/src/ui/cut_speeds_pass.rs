//! F3.10 S1–S5 (`⏩ Speeds`) — the pass's own half of the shell: read the page, telephone, write back.
//!
//! Split out of `crate::ui::window`, which is past its size budget. No rule lives here: every decision —
//! membership, the caption refusal, rate ≈ 1, the gap merge and both log lines — belongs to
//! [`crate::cut_speed_pass`], and the wire leg to [`crate::speeds_ask`]. What this module owns is the seam
//! between them and the page: which clips the batch is made of, which captions are already on them, how many
//! rounds §S4 allows, and where an answer is written so one ↶ takes it back.
//!
//! The two public doors differ by ONE thing: [`run_speeds_pass`] dials for real, while
//! [`run_speeds_pass_with_reply`] hands the same calls back each round so a test can exercise §S4's retry and
//! §S5's merge without a socket. Both run through the one private body, so they cannot drift apart.

use adw::prelude::*;

use crate::policy;
use crate::ui::window;

/// F3.10 S1–S5: run the speeds pass over this window's newest cut and say what happened.
///
/// Rule-free like [`window::run_captions_pass`]: every decision lives in [`crate::cut_speed_pass`]. This seam
/// reads the page, hands the rules their inputs (the kept clips AND the captions already on them, because S1's
/// brief notes them and S2 refuses a fast rate over one), and writes back what they answer. Placed speeds go
/// through `record_edit` and NEVER `seed_review_cut`, so one ↶ takes the whole pass back. Each round TELEPHONES
/// through `ask_speeds` below — §F3.10 S4's second round is a fresh ask, not the same reply handed to the rules
/// twice.
pub fn run_speeds_pass(window: &adw::ApplicationWindow) -> String {
    place_speeds_reply(window, &|round, batch| ask_speeds(window, batch, round))
}

/// F3.10 S2/S4/S5 with a scripted reply: the same route the pass takes, minus the telephone. A test feeds the
/// calls here and asserts placement against real page state, exactly as `run_captions_pass_with_reply` does.
pub fn run_speeds_pass_with_reply(
    window: &adw::ApplicationWindow,
    calls: &[crate::cut_speed_pass::Call],
) -> String {
    place_speeds_reply(window, &|_, _| calls.to_vec())
}

/// F3.10 S1's leg: the ask over the batch this pass just built, for this round of it. Returns no calls when
/// there is nothing to ask about or nowhere to ask it — the same shape `window::ask_captions` keeps, so a
/// missing project folder is said in the log rather than thrown.
fn ask_speeds(
    window: &adw::ApplicationWindow,
    batch: &crate::tools::clips::Clips,
    attempt: u32,
) -> Vec<crate::cut_speed_pass::Call> {
    let Some(tree) = window::narrate_session_tree() else {
        window::log_line("!!! speed: no project folder to ask from -- nothing asked");
        return Vec::new();
    };
    let clips: Vec<(u32, f64)> = batch
        .clips()
        .iter()
        .map(|clip| (clip.n, clip.length))
        .collect();
    // Which clips carry captions, and how many, comes off the batch itself, so the brief says the same
    // thing the refusal will — §F3.10 S1's `N caption(s) -- runs at 1`.
    let captioned: Vec<(u32, usize)> = batch
        .clips()
        .iter()
        .map(|clip| (clip.n, batch.captions_of(clip.n).len()))
        .filter(|(_, count)| *count > 0)
        .collect();
    // §9 (`the caption, speed and decoration briefs name no camera or lane`): named once
    // there is more than one row filming or more than one recording heard.
    let cut_ = window::newest_review_cut();
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
        &tree,
        &window::user_context(window),
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

/// `ask` is the telephone: called once per round with the batch as it stands, so a retry asks again rather than
/// re-reading an answer that was just refused. The scripted door passes a closure that hands back the same
/// calls every time, which is what lets a test exercise the two-round rule without a socket.
fn place_speeds_reply(
    window: &adw::ApplicationWindow,
    ask: &dyn Fn(u32, &crate::tools::clips::Clips) -> Vec<crate::cut_speed_pass::Call>,
) -> String {
    if !policy::pass_runs(&window::session_policy(window), policy::Pass::Speeds) {
        let said = "speeds are off \u{2014} the context ruled them out".to_string();
        if let Some(status_line) = window::find_status(window.upcast_ref()) {
            status_line.set_text(&said);
        }
        return said;
    }
    let mut cut_ = window::newest_review_cut();
    // The clips the pass asks about are the VISIBLE SEGMENTS of the cut, numbered from 1 in play order, each
    // carrying its own session start -- which is why this uses `Clips` triples rather than F3.9's
    // `(number, length)` sum. NOT `cut_play::kept_runs`: that helper MERGES touching segments into runs for
    // preloading ("no gap means no jump to preload for"), and one merged run would be one clip to the model,
    // so a cut of three adjoining segments would answer to a single 30 s clip and every rate the model named
    // for clip 2 or 3 would fall outside the batch.
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
    // S1's brief and S2's refusal read the captions that are ALREADY on the cut, so they must be in the batch.
    // A text effect hanging across a clip boundary is CLAMPED to the overlap rather than skipped: dropping it
    // would hide a caption from the refusal rule and let a clip the viewer sees as captioned be sped up.
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

    // Two rounds at most, decided by the same rule F3.9 uses: one rejection owes one retry, a third ask never
    // happens. A fault is an UNUSABLE reply (a clip number never given); a per-call refusal is unwelcome, not
    // unusable, and does not cost the round.
    let mut placed: Option<Vec<crate::cut::Fx>> = None;
    let mut refusals: Vec<String> = Vec::new();
    for attempt in 1..=2u32 {
        // S4: each round asks afresh. A fault is an UNUSABLE reply (a clip number never given); a per-call
        // refusal is unwelcome, not unusable, and does not cost the round.
        let reply = crate::cut_speed_pass::place(&batch, &ask(attempt, &batch));
        for answer in &reply.answers {
            if let crate::cut_speed_pass::Answer::Refused(reason) = answer {
                // S2: the refusal MUST be said. Silently dropping a rate would leave the model believing its
                // speed is in the cut.
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
                window::save_insert_cut(&cut_);
                window::record_edit(window, &cut_);
                window::refresh_effects_lane(window);
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
    if let Some(status_line) = window::find_status(window.upcast_ref()) {
        status_line.set_text(&said);
    }
    window::log_line(&said);
    said
}
