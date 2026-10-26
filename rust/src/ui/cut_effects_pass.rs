//! F3.11 S1–S5 (`✨ Decorations`) — the pass's own half of the shell: read the page, telephone, write back.
//!
//! Split out of `crate::ui::window`, which is past its size budget. No rule lives here: every decision —
//! membership, the per-call skips, the app's defaults for an undersized decoration and both log lines —
//! belongs to [`crate::cut_effects_pass`], and the wire leg to [`crate::effects_ask`]. What this module owns
//! is the seam between them and the page: which clips the batch is made of, which captions and rates are
//! already on them, how many rounds §S4 allows, and where an answer is written so one ↶ takes it back.
//!
//! The two public doors differ by ONE thing: [`run_effects_pass`] dials for real, while
//! [`run_effects_pass_with_reply`] hands the same calls back each round so a test can exercise §S2's
//! whole-reply rejection and §S3's skips without a socket. Both run through the one private body, so they
//! cannot drift apart.

use crate::policy;
use crate::tools::clips::Clips;
use crate::ui::window;

/// F3.11 S1–S5: run the decorations pass over this window's newest cut and say what happened.
///
/// Rule-free like [`window::run_captions_pass`] and [`crate::ui::cut_speeds_pass::run_speeds_pass`]: every
/// decision lives in [`crate::cut_effects_pass`]. This seam reads the page, hands the rules their inputs (the
/// kept clips AND the captions and rates already on them, because S1's brief names both), and writes back what
/// they answer. Each round TELEPHONES through `ask_effects` below — §F3.11's second round is a fresh ask, not the
/// same rejected reply handed to the rules twice.
pub fn run_effects_pass(window: &adw::ApplicationWindow) -> String {
    let inputs = window::pass_inputs(window);
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

/// F3.11 S2/S3/S5 with a scripted reply: the same route the pass takes, minus the telephone. A test feeds the
/// `add_effect` calls here and asserts placement against real page state, exactly as
/// `run_speeds_pass_with_reply` does.
pub fn run_effects_pass_with_reply(
    window: &adw::ApplicationWindow,
    calls: &[crate::cut_effects_pass::Call],
) -> String {
    let inputs = window::pass_inputs(window);
    let out = walk_effects_with(&inputs, &window::newest_review_cut(), &|_, _| {
        calls.to_vec()
    });
    window::land_pass(window, &out);
    out.said
}

/// F3.11 S1's leg: the ask over the batch this pass just built, for this round of it. Returns no calls when
/// there is nothing to ask about or nowhere to ask it — the shape `cut_speeds_pass::ask_speeds` keeps, so a
/// missing project folder is said in the log rather than thrown.
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
    // §9 (`the caption, speed and decoration briefs name no camera or lane`): the batch is
    // told in words which cameras filmed it and which recordings are heard, but only once
    // there is more than one of either to tell apart — see [`crate::effect_overlap::brief_lines`].
    let mut rows: Vec<String> = cut_
        .segs
        .iter()
        .filter(|scene| scene.ins.is_empty())
        .map(|scene| (crate::cut_cam::scene_row(scene) + 1).to_string())
        .collect();
    rows.sort_unstable();
    rows.dedup();
    let lanes: Vec<String> = cut_.lanes.iter().map(|lane| lane.name.clone()).collect();
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

/// `ask` is the telephone: called once per round with the batch as it stands, so a retry asks again rather than
/// re-reading an answer that was just rejected. The scripted door passes a closure that hands back the same
/// calls every time, which is what lets a test exercise the two-round rule without a socket.
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
    // S1's batch: the visible segments, numbered from 1, with the captions already on the cut clamped onto
    // them (see [`decoration_batch`]). The rates come from the same batch, so the brief and the refusal rules
    // read one picture of the cut rather than two that can disagree.
    let batch = decoration_batch(&cut_);

    // Two rounds at most, decided by the same rule F3.9 uses: one rejection owes one retry, a third ask never
    // happens. A fault is an UNUSABLE reply (a clip number never given); a per-call skip (an unknown kind, an
    // empty span) is unwelcome, not unusable, and costs no round.
    let mut placed: Option<Vec<crate::cut::Fx>> = None;
    // How many decorations this pass ended up placing, kept for the density sentence below (`None` is a pass
    // that never got an usable answer, which has nothing to weigh against the guide).
    let mut placed_count: Option<usize> = None;
    let mut skipped: Vec<String> = Vec::new();
    for attempt in 1..=2u32 {
        // S4: each round asks afresh, so a retry never reads back the reply it is retrying.
        let reply = crate::cut_effects_pass::place(&batch, &ask(attempt, &batch));
        for answer in &reply.answers {
            if let crate::cut_effects_pass::Answer::Refused(reason) = answer {
                // S3: a skipped line is SAID. Dropping it silently would leave the user counting bars on the
                // lane and finding fewer than the reply proposed, with no reason printed anywhere.
                skipped.push(reason.clone());
            }
        }
        if reply.usable() {
            // P.machine.maxProposedEffects is compared HERE, against what this reply would add:
            // a runaway answer is unusable in the same way a bad clip number is, so it costs a
            // round and its reason reaches the log. The density guide says; only this cap refuses.
            let verdict = crate::cut_effects_pass::effect_cap_allows_reply(&reply.effects);
            if !verdict.allowed {
                window::log_line(&crate::cut_effects_pass::rejected(&verdict.reason));
            } else {
                placed = Some(reply.effects.clone());
                break;
            }
        }
        if let Some(problem) = &reply.fault {
            window::log_line(&crate::cut_effects_pass::rejected(problem));
        }
        if !crate::cut_captions::retries(attempt) {
            break;
        }
    }
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
    for reason in skipped {
        said.push_str(" \u{2014} ");
        said.push_str(&reason);
        window::log_line(&reason);
    }
    // §06-effects#6's "decorations density (prompt)", reported against what this pass actually placed.
    // REPORTED, never trimmed: the User Context outranks the default ("Asked for more, write more",
    // `spec/prompts/effects.md`) and the hard bound is `P.machine.maxProposedEffects`, which was already
    // applied above. A reply inside the guide says nothing extra -- the row keeps its own news.
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

/// F3.11 S1: the clips the decorations pass asks about — the VISIBLE SEGMENTS of the cut, numbered from 1 in
/// play order, each carrying its own session start, with the captions already on the cut clamped onto them.
/// Same reasoning as in `cut_speeds_pass`, restated because a reader of this pass should not have to open the
/// other one to know why: NOT `cut_play::kept_runs`, which MERGES touching segments for preloading, so a cut
/// of three adjoining segments would be one clip to the model and every clip number above 1 the model named
/// would fall outside the batch.
fn decoration_batch(cut_: &crate::cut::Cut) -> Clips {
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
    // S1's brief names the captions per clip ("never zoom past captions"), so a caption hanging across a clip
    // boundary is CLAMPED to the overlap rather than skipped: dropping it would hide a caption the viewer
    // sees from the rule that reads it.
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

/// F3.11 S1 (`told the captions and speeds`): the rate each clip already plays at, read off the speed effects
/// the F3.10 pass (or a hand-placed ⏩) put on the cut. A clip with no speed effect is absent rather than
/// listed at 1: [`crate::cut_effects_pass::brief`] says nothing about a picture that is not moving, and a
/// stop (rate 0) IS worth saying, so only "no effect" and "rate 1" are left out.
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
