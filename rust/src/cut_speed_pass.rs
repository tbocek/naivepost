//! F3.10 Speeds proposed by the model (after the cut) — `spec/06-effects.md` F3.10, steps S1–S5.
//!
//! ▶ after the captions pass: ONE call over the whole cut asks for a rate per clip and comes back as
//! `set_clip_speed(clip, rate)` calls closed by `finish`. This module owns what happens around those calls — what the
//! request says (S1), that a fast rate on a captioned clip is refused *for that call* while everything else in the
//! reply still lands (S2), that a rate of about 1 and a rate nobody sent are simply not answers (S3), the speed record
//! a real rate becomes (S4), same-rate stretches nearer than P.policy.speedGapSeconds merging into one (S4), and the
//! two lines the log gets — `>>> speed: …` when there is something to say, `!!! speed: …` when the pass gave up (S5).
//!
//! The name says *pass* because [`crate::cut_speed`] holds F3.3's by-hand rules — what a ramp can be built, what rate
//! covers a second, what a label reads — and this module reuses them rather than restating them: it is the same speed
//! effect whether a person placed it with ⏩ or a model proposed it. Nor does it re-implement §3.7's tool:
//! [`Clips::set_clip_speed`] already refuses to hurry a captioned clip and already clamps the rate, so this pass asks
//! that question of the tool and reports what the tool answered.
//! `rust/src/ui/window.rs` renders only Prepare today, so nothing here draws; a proposed speed appears on the lane as
//! an ordinary speed effect, which has its own screen (§F3.3), not one of its own.

use crate::cut::Fx;
use crate::cut_captions;
use crate::cut_speed;
use crate::tools;
use crate::tools::clips::Clips;

// --- S1: the one call ------------------------------------------------------------------------------------------------

/// F3.10 S4 (`same-rate stretches nearer than P.policy.speedGapSeconds merge`): four seconds of ordinary footage is
/// far enough apart to read as two runs rather than one with a hiccup in it. The prototype's reason is concrete — a
/// cut once came back with two ×4 runs a second apart, which on screen stutters and on the lane jams two badges
/// together — so the island of normal speed between them goes too.
pub const GAP_SECONDS: f64 = 4.0;

/// F3.10 S1 (`the footage total`): the line that tells the model how much there is to save, so "longer dull stretch →
/// higher rate" has a denominator. The prototype's own wording.
pub const FOOTAGE_HEADER: &str = "FOOTAGE:";

/// F3.10 S4 (`a speed effect over the clip`, ramp `min(1, d/4)`): what a proposed speed asks for at each edge before
/// F3.3's arithmetic decides whether it can be built. `// effects.proposedRampSeconds` — §10 gives this no `P.` row,
/// so it takes the bare prefix of the rule that reads it, as [`crate::fx_volume`]'s and [`cut_captions`]'s rows do.
pub const PROPOSED_RAMP_SECONDS: f64 = 1.0;

/// F3.10 S3 (`rate ≈ 1? yes → ignored`): how near to 1 a rate has to be to say nothing. `// effects.neutralRateTolerance`,
/// again with no `P.` row of its own. A tenth over is a rounding artifact from a model that meant "leave this clip
/// alone"; half as fast is slow motion and means something, so the window is around 1 rather than a band above it.
pub const NEUTRAL_TOLERANCE: f64 = 0.01;

/// F3.10 S1: how much footage the request is about — the clips' own seconds added up, before any rate shortens them.
pub fn footage_total(clips: &Clips) -> f64 {
    clips.clips().iter().map(|clip| clip.length).sum()
}

/// F3.10 S1 (`briefs note captions ("…, N caption(s) -- runs at 1") and the footage total`): the whole request, one
/// call for the entire cut. A clip carrying captions is marked as running at 1 in its own line, which is the rule S2
/// will enforce — said to the model first rather than only refused afterwards.
///
/// The header is [`cut_captions::CLIPS_HEADER`], reused rather than restated: the prototype spelled this pass's list
/// "…SAID AND SHOWN…" and the captions pass's "…SAID OVER EACH:", two strings for one thing, and §F3.10 quotes no
/// header of its own while §F3.9 does. One header per app is the plainer reading.
pub fn brief(clips: &Clips, footage: f64) -> String {
    let mut lines = vec![format!(
        "{FOOTAGE_HEADER} {footage:.0} seconds over {} clips.",
        clips.clips().len()
    )];
    lines.push(String::new());
    lines.push(cut_captions::CLIPS_HEADER.to_string());
    for clip in clips.clips() {
        let mut line = cut_captions::clip_line(clip.n, clip.length);
        let captions = clips.captions_of(clip.n).len();
        if captions > 0 {
            line.push_str(&format!(", {captions} caption(s) -- runs at 1"));
        }
        lines.push(line);
    }
    lines.join("\n")
}

// --- S2/S3: the answer's shape and its three outcomes ----------------------------------------------------------------

/// F3.10 S2 (`tools: set_clip_speed(clip, rate)`): one clip's proposed rate. No seconds arrive with it — a speed is
/// asked for over a whole clip, which is why there is no start/end pair to validate the way F3.9's captions had.
#[derive(Debug, Clone, PartialEq)]
pub struct Call {
    pub clip: u32,
    pub rate: f64,
}

/// What one call came to. `Refused` is not a failed reply: §F3.10's flowchart sends a fast rate on a captioned clip to
/// its own red box, per call, and nothing here says the batch is thrown away (contrast [`cut_captions::place`], where
/// one clip number outside the batch loses everything). The refusal goes back to the model as the tool's own sentence,
/// so it learns why that clip runs at 1.
#[derive(Debug, Clone, PartialEq)]
pub enum Answer {
    /// A speed effect over the clip, fades and all.
    Placed(Fx),
    /// Rate ≈ 1, or no rate at all — nothing to do and nothing to say about it.
    Ignored,
    /// The tool's reason this clip will not be hurried.
    Refused(String),
}

/// F3.10 S2/S5: a whole reply. `fault` is the only thing that costs a retry — an unusable answer (a clip number that
/// was never given) rather than an unwelcome one (a captioned clip). The prototype's loop appends the fault to the
/// conversation and puts the same call again, up to two rounds ([`cut_captions::retries`]).
#[derive(Debug, Clone, PartialEq)]
pub struct Reply {
    pub effects: Vec<Fx>,
    pub answers: Vec<Answer>,
    pub fault: Option<String>,
}

impl Reply {
    /// Whether the reply is worth placing at all.
    pub fn usable(&self) -> bool {
        self.fault.is_none()
    }
}

/// F3.10 S3 (`rate ≈ 1?`): is this rate the footage's own speed, so that saying it changes nothing? Checked before
/// S2's caption test on purpose: a rate of 1 over a captioned clip is not a refusal, it is silence, and answering it
/// with "this clip cannot be sped up" would tell the model something false.
pub fn is_neutral(rate: f64) -> bool {
    (rate - 1.0).abs() < NEUTRAL_TOLERANCE
}

/// F3.10 S4 (`a speed effect over the clip`): the fades a proposed speed gets — [`PROPOSED_RAMP_SECONDS`] capped by a
/// quarter of the clip, then handed to [`cut_speed::ramps`] so F3.3's rule about what can actually be built decides
/// whether there are any. A ×4 run over half a minute cannot afford a stair at either edge (the least ramp it can pay
/// is `P.eng.rampStepSeconds · √4` = 1.2 s, more than the 1 s asked for), so it gets a hard change of speed, which is
/// what the render would have fallen back to anyway.
pub fn ramp(rate: f64, dur: f64) -> (f64, f64) {
    let asked = PROPOSED_RAMP_SECONDS.min(dur / 4.0);
    cut_speed::ramps(rate, dur, asked, asked)
}

/// F3.10 S2–S5: the reply turned into effects.
///
/// Membership first and over the whole answer — a clip number outside `1..=count` means the model counted from the
/// wrong end and nothing else in the reply can be trusted, which is what costs the retry ([`cut_captions::clip_outside`]
/// words it exactly as §F3.10 does, with the range `(1 to M)`). Everything else is per call: ignored, refused, or a
/// speed over the clip at the rate §3.7's tool reports (clamped by `cutpass::apply_rate`, so this pass cannot put an
/// unrenderable rate in the cut), then merged by S4's rule.
///
/// There is **no length gate** here, and no fault for a reply that proposes nothing: "no clip runs fast" is a right
/// answer to a cut with nothing dull in it, and how long the video ends up is the cut pass's arithmetic, not this one's.
pub fn place(clips: &Clips, calls: &[Call]) -> Reply {
    let count = clips.clips().len() as u32;
    for call in calls {
        if let Some(problem) = cut_captions::clip_outside(call.clip, 1, count) {
            return Reply { effects: Vec::new(), answers: Vec::new(), fault: Some(problem) };
        }
    }

    // The tool's own state, cloned so this stays a function of its arguments: the caller's clips keep their captions
    // and rates untouched, and the refusal rule reads the captions it was given.
    let mut batch = clips.clone();
    let mut answers = Vec::new();
    let mut effects = Vec::new();
    for call in calls {
        // No rate at all. An omitted JSON number and a 0 arrive identically, and reading either as a freeze would hand
        // a still to every reply that forgot to name a speed — so this is silence, not a stop, and not even a refusal.
        if call.rate <= 0.0 || is_neutral(call.rate) {
            answers.push(Answer::Ignored);
            continue;
        }
        let answer = batch.set_clip_speed(call.clip, call.rate);
        let Some(body) = serde_json::from_str::<serde_json::Value>(&answer).ok() else {
            answers.push(Answer::Refused(answer.clone()));
            continue;
        };
        if let Some(reason) = body.get("error").and_then(|e| e.as_str()) {
            answers.push(Answer::Refused(reason.to_string()));
            continue;
        }
        // The rate as APPLIED: §3.7 clamps what will not render, and a speed record holding the asked-for number would
        // promise seconds the render never produces.
        let applied = body.get("applied").and_then(|v| v.as_f64()).unwrap_or(call.rate);
        let clip = &batch.clips()[batch.clips().iter().position(|c| c.n == call.clip).expect("S2 checked membership")];
        // The clip's OWN start, which is the state's session second for it. Not [`cut_captions::session_start`]'s sum:
        // that exists because F3.9's `(number, length)` pairs carry no position, and a sum of lengths is wrong as soon
        // as a clip has been trimmed or already sped up.
        let (trans, tout) = ramp(applied, clip.length);
        let fx = Fx {
            kind: "speed".into(),
            t: clip.start,
            dur: clip.length,
            rate: applied,
            trans,
            tout,
            ..Default::default()
        };
        answers.push(Answer::Placed(fx.clone()));
        effects.push(fx);
    }
    Reply { effects: merge(&effects), answers, fault: None }
}

// --- S4: same-rate stretches nearer than the gap are one stretch -----------------------------------------------------

/// F3.10 S4 (`same-rate stretches nearer than P.policy.speedGapSeconds merged`): fold runs at ONE rate that come
/// within [`GAP_SECONDS`] of each other into a single run, taking the later stretch's fade out. Only speeds are folded
/// and only same-rate ones — a ×2 and a ×4 two seconds apart are two decisions, not one — and only ones that hear
/// the same thing (§7: sound answers do not merge), and everything else in the
/// list survives untouched and in its own order, so this can be handed a whole cut's effects.
///
/// The bound is "nearer than", so a gap of exactly four seconds does NOT merge; compared with `<` rather than `<=`.
/// Floats from arithmetic can land a hair either side of that, which is the usual risk in the timeline's numbers and
/// why the app's own seconds come from word edges rather than from sums.
pub fn merge(fxs: &[Fx]) -> Vec<Fx> {
    let mut speeds: Vec<Fx> = fxs
        .iter()
        .filter(|fx| fx.effect_kind() == Some(crate::cut::EffectKind::Speed) && fx.rate > 0.0)
        .cloned()
        .collect();
    speeds.sort_by(|a, b| a.t.total_cmp(&b.t));

    let mut out: Vec<Fx> = Vec::new();
    for fx in speeds {
        match out.last_mut() {
            Some(prev)
                // §7: sound answers do not merge. Two stretches at one rate whose sound differs are two decisions —
                // ×2 with the footage's own pitch and ×2 reading the room at 1× do not become one stretch.
                if prev.rate == fx.rate
                    && prev.snd == fx.snd
                    && fx.t - (prev.t + prev.dur) < GAP_SECONDS =>
            {
                let end = (prev.t + prev.dur).max(fx.t + fx.dur);
                prev.dur = end - prev.t;
                // The fade out belongs to the stretch that ends last, which is the one being joined on.
                prev.tout = fx.tout;
            }
            _ => out.push(fx),
        }
    }

    // Everything that was not a speed, in the order it arrived. Sorting those would reorder a caption against the
    // zoom it was written for, and this function has no business doing that.
    let rest = fxs
        .iter()
        .filter(|fx| !(fx.effect_kind() == Some(crate::cut::EffectKind::Speed) && fx.rate > 0.0))
        .cloned();
    out.extend(rest);
    out
}

// --- S5: what the log says ---------------------------------------------------------------------------------------------

/// F3.10 S5 (`Log ">>> speed: N clip(s) run fast — a of video from b of footage"`): the pass's one line of success,
/// with an EM dash (U+2014) — verified against `spec/06-effects.md`'s bytes, and unlike F3.9's fallback line, which
/// spells its break `--`. The two clock times come from [`tools::mm_ss`], which pads: 40 s reads `00:40`, not the
/// prototype's unpadded `0:40`, because every other status line in this app prints that way.
///
/// N counts CLIPS that run fast, judged at each clip's middle through [`cut_speed::rate_at`] — the app's one "what rate
/// covers this second" rule, so a merged stretch and a lone one are read the same way. The prototype counted the
/// merged stretches instead, which prints a smaller number than the sentence claims to mean: "2 clip(s) run fast" is
/// about clips. A clip whose rate came back at 1 (ignored, or refused) is not running fast and is not counted.
pub fn runs_fast(clips: &Clips, effects: &[Fx], footage: f64) -> String {
    let merged = merge(effects);
    let mut seconds = 0.0;
    let mut fast = 0usize;
    for clip in clips.clips() {
        let rate = cut_speed::rate_at(&merged, clip.start + clip.length / 2.0);
        if (rate - 1.0).abs() > NEUTRAL_TOLERANCE {
            fast += 1;
        }
        // What the clip is worth on screen: its own seconds at the rate covering it. A rate of 1 — no effect there —
        // contributes its length, which is why this adds up to `footage` when nothing runs fast.
        seconds += clip.length / cut_speed::applied_rate(rate);
    }
    format!(
        ">>> speed: {fast} clip(s) run fast \u{2014} {} of video from {} of footage",
        tools::mm_ss(seconds),
        tools::mm_ss(footage)
    )
}

/// F3.10 S5 (`"!!! speed: no usable answer — every clip plays at 1"`): the pass gave up after its one retry, and the
/// cut carries no speeds at all. Em dash again (per §F3.10's bytes), and the second half is the promise that matters:
/// a longer video, and a whole one.
pub fn no_answer() -> String {
    format!("!!! speed: no usable answer \u{2014} every clip plays at 1")
}

/// F3.10 S5 (`two rounds fail`, with the fault appended and the call put again): what is logged between the two
/// attempts — the reason the reply was unusable, said back before it is asked for once more. [`cut_captions::retries`]
/// decides whether another attempt is owed; a third never is.
pub fn rejected(problem: &str) -> String {
    format!(">>> speed rejected: {problem}")
}
