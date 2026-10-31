//! F3.10 Speeds proposed by the model (after the cut) — `spec/06-effects.md` F3.10, steps S1–S5.

use crate::cut::Fx;
use crate::cut_captions;
use crate::cut_speed;
use crate::tools;
use crate::tools::clips::Clips;

// --- S1: the one call ------------------------------------------------------------------------------------------------

/// F3.10 S4 (`same-rate stretches nearer than P.policy.speedGapSeconds merge`): four seconds of
/// ordinary footage is far enough apart to read as two runs rather than one with a hiccup in it.
pub const GAP_SECONDS: f64 = 4.0;

/// F3.10 S1 (`the footage total`): the line that tells the model how much there is to save, so
/// "longer dull stretch → higher rate" has a denominator.
pub const FOOTAGE_HEADER: &str = "FOOTAGE:";

/// F3.10 S4 (`a speed effect over the clip`, ramp `min(1, d/4)`): what a proposed speed asks for at
/// each edge before F3.3's arithmetic decides whether it can be built.
pub const PROPOSED_RAMP_SECONDS: f64 = 1.0;

/// F3.10 S3 (`rate ≈ 1? yes → ignored`): how near to 1 a rate has to be to say nothing.
pub const NEUTRAL_TOLERANCE: f64 = 0.01;

/// F3.10 S1: how much footage the request is about — the clips' own seconds added up, before any
/// rate shortens them.
pub fn footage_total(clips: &Clips) -> f64 {
    clips.clips().iter().map(|clip| clip.length).sum()
}

/// F3.10 S1 (`briefs note captions ("…, N caption(s) -- runs at 1") and the footage total`): the
/// whole request, one call for the entire cut.
pub fn brief(clips: &Clips, footage: f64) -> String {
    let mut lines = vec![format!(
        "{FOOTAGE_HEADER} {footage:.0} seconds over {} clips.",
        clips.clips().len()
    )];
    lines.push(String::new());
    lines.push(cut_captions::CLIPS_HEADER.to_string());
    for clip in clips.clips() {
        lines.push(noted_clip_line(clips, clip.n, clip.length));
    }
    lines.join("\n")
}

/// One clip's brief line, marked when it carries captions.
pub fn noted_clip_line(clips: &Clips, n: u32, length: f64) -> String {
    let mut line = cut_captions::clip_line(n, length);
    let captions = clips.captions_of(n).len();
    if captions > 0 {
        line.push_str(&format!(", {captions} caption(s) -- runs at 1"));
    }
    line
}

// --- S2/S3: the answer's shape and its three outcomes ----------------------------------------------------------------

/// F3.10 S2 (`tools: set_clip_speed(clip, rate)`): one clip's proposed rate.
#[derive(Debug, Clone, PartialEq)]
pub struct Call {
    pub clip: u32,
    pub rate: f64,
}

/// What one call came to, in this pass and the decorations'. `Refused` costs no round; only [`Reply::fault`] does.
#[derive(Debug, Clone, PartialEq)]
pub enum Answer {
    Placed(Fx),
    Ignored,
    Refused(String),
}

/// F3.10 S2/S5: a whole reply.
#[derive(Debug, Clone, PartialEq)]
pub struct Reply {
    pub effects: Vec<Fx>,
    pub answers: Vec<Answer>,
    pub fault: Option<String>,
}

impl Reply {
    pub fn usable(&self) -> bool {
        self.fault.is_none()
    }
}

/// F3.10 S3 (`rate ≈ 1?`): is this rate the footage's own speed, so that saying it changes nothing?
pub fn is_neutral(rate: f64) -> bool {
    (rate - 1.0).abs() < NEUTRAL_TOLERANCE
}

/// F3.10 S4 (`a speed effect over the clip`): the fades a proposed speed gets.
pub fn ramp(rate: f64, dur: f64) -> (f64, f64) {
    let asked = PROPOSED_RAMP_SECONDS.min(dur / 4.0);
    cut_speed::ramps(rate, dur, asked, asked)
}

/// F3.10 S2–S5: the reply turned into effects.
pub fn place(clips: &Clips, calls: &[Call]) -> Reply {
    let mut reply = place_each(clips, calls, |call| call.clip, place_one);
    reply.effects = merge(&reply.effects);
    reply
}

fn place_one(batch: &mut Clips, call: &Call) -> Answer {
    if call.rate <= 0.0 || is_neutral(call.rate) {
        return Answer::Ignored;
    }
    let answer = batch.set_clip_speed(call.clip, call.rate);
    let body = match tool_body(&answer) {
        Ok(body) => body,
        Err(reason) => return Answer::Refused(reason),
    };
    // The rate as APPLIED: §3.7 clamps what will not render, and a speed record holding the
    // asked-for number would promise seconds the render never produces.
    let applied = body
        .get("applied")
        .and_then(|v| v.as_f64())
        .unwrap_or(call.rate);
    let clip = batch
        .clips()
        .iter()
        .find(|c| c.n == call.clip)
        .expect("S2 checked membership");
    let (trans, tout) = ramp(applied, clip.length);
    Answer::Placed(Fx {
        kind: "speed".into(),
        t: clip.start,
        dur: clip.length,
        rate: applied,
        trans,
        tout,
        ..Default::default()
    })
}

/// Both passes' walk over a reply: a clip outside the list loses the whole reply, then each call lands by `one`
/// on a clone of the state, so the caller's clips stay untouched.
pub fn place_each<C>(
    clips: &Clips,
    calls: &[C],
    clip_of: fn(&C) -> u32,
    one: fn(&mut Clips, &C) -> Answer,
) -> Reply {
    let count = clips.clips().len() as u32;
    for call in calls {
        if let Some(problem) = cut_captions::clip_outside(clip_of(call), 1, count) {
            return Reply {
                effects: Vec::new(),
                answers: Vec::new(),
                fault: Some(problem),
            };
        }
    }
    let mut batch = clips.clone();
    let mut answers = Vec::new();
    let mut effects = Vec::new();
    for call in calls {
        let answer = one(&mut batch, call);
        if let Answer::Placed(fx) = &answer {
            effects.push(fx.clone());
        }
        answers.push(answer);
    }
    Reply {
        effects,
        answers,
        fault: None,
    }
}

/// A tool's answer read as JSON; `Err` is the sentence to refuse the call with.
pub fn tool_body(answer: &str) -> Result<serde_json::Value, String> {
    let Ok(body) = serde_json::from_str::<serde_json::Value>(answer) else {
        return Err(answer.to_string());
    };
    if let Some(reason) = body.get("error").and_then(|e| e.as_str()) {
        return Err(reason.to_string());
    }
    Ok(body)
}

// --- S4: same-rate stretches nearer than the gap are one stretch -----------------------------------------------------

/// F3.10 S4 (`same-rate stretches nearer than P.policy.speedGapSeconds merged`): fold runs at ONE
/// rate that come within [`GAP_SECONDS`] of each other into a single run, taking the later
/// stretch's fade out.
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
                // §7: sound answers do not merge.
                if prev.rate == fx.rate
                    && prev.snd == fx.snd
                    && fx.t - (prev.t + prev.dur) < GAP_SECONDS =>
            {
                let end = (prev.t + prev.dur).max(fx.t + fx.dur);
                prev.dur = end - prev.t;
                prev.tout = fx.tout;
            }
            _ => out.push(fx),
        }
    }

    // Everything that was not a speed, in the order it arrived.
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
        // What the clip is worth on screen: its own seconds at the rate covering it.
        seconds += clip.length / cut_speed::applied_rate(rate);
    }
    format!(
        ">>> speed: {fast} clip(s) run fast \u{2014} {} of video from {} of footage",
        tools::mm_ss(seconds),
        tools::mm_ss(footage)
    )
}

/// F3.10 S5 (`"!!! speed: no usable answer — every clip plays at 1"`): the pass gave up after its
/// one retry, and the cut carries no speeds at all.
pub fn no_answer() -> String {
    "!!! speed: no usable answer \u{2014} every clip plays at 1".to_string()
}

/// F3.10 S5 (`two rounds fail`, with the fault appended and the call put again): what is logged
/// between the two attempts.
pub fn rejected(problem: &str) -> String {
    format!(">>> speed rejected: {problem}")
}
