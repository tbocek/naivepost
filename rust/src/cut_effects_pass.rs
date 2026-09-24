//! F3.11 Decorations proposed by the model (after the cut) — `spec/06-effects.md` F3.11, steps S1–S5.
//!
//! One call over every kept clip asks for zooms, one stop and some volume, told what the captions and the speeds
//! already did to each clip; the answer comes back as `add_effect(clip, kind, start, end, gain?)` calls closed by
//! `finish`. This module owns what happens around those calls: what the request says (S1), that a clip outside the list
//! throws the whole reply away while an unknown kind or an empty span is skipped without ceremony (S2, S3), the app's
//! own defaults for a decoration nobody sized by hand (S4), and the lines the log gets when the pass succeeds or gives
//! up (S5).
//!
//! The three passes of §F3 are deliberately shaped alike — [`crate::cut_captions`] places captions, [`crate::cut_speed_pass`]
//! rates, this one decorations — so a caller drives them with the same `Call` / `Answer` / `Reply` reading and the same
//! one-retry budget. What differs is where a bad call lands: F3.9's out-of-batch clip loses the reply, F3.10's fast rate
//! over captions refuses one call, and here BOTH happen, because a clip number the model invented says the answer is not
//! about this cut while a `blur` in place of a `zoom` only says that one line is unusable.
//! `rust/src/ui/window.rs` renders only Prepare today, so nothing here draws; a proposed decoration appears on the lane
//! as an ordinary zoom / stop / volume effect, each with its own screen (§F3.1, §F3.3, §F3.6).

use crate::cut::Fx;
use crate::cut_captions;
use crate::cut_speed_pass;
use crate::fx_zoom;
use crate::tools::clips::Clips;

// --- S1: the one call over every clip ----------------------------------------------------------------------------------

/// F3.11 S1 (`one call over every clip · told the captions and speeds`): the whole request. Each clip's line carries
/// what has already been decided about it — a caption means it runs at 1, a rate means the picture is already moving —
/// because both change what a decoration should be ("never zoom past captions").
///
/// The caption note is spelled exactly as [`crate::cut_speed_pass::brief`] spells it and the rate note exactly as the
/// prototype's does. Two small format sites rather than one shared helper: each section of the spec quotes its own
/// brief wording, so a helper would have to be parameterised by which section is asking — more machinery than text.
pub fn brief(clips: &Clips, speeds: &[(u32, f64)]) -> String {
    let mut lines = vec![cut_captions::CLIPS_HEADER.to_string()];
    for clip in clips.clips() {
        let mut line = cut_captions::clip_line(clip.n, clip.length);
        let captions = clips.captions_of(clip.n).len();
        if captions > 0 {
            line.push_str(&format!(", {captions} caption(s) -- runs at 1"));
        }
        // `{rate:g}` in the prototype's sense: a whole rate loses its decimals (2 → "2x"), a fraction keeps them
        // (1.5 → "1.5x"). A rate of 1 or none says nothing about the picture, so it is not mentioned at all.
        if let Some((_, rate)) = speeds.iter().find(|(n, _)| *n == clip.n) {
            if *rate > 0.0 && *rate != 1.0 {
                line.push_str(&format!(", plays at {}", trim_number(*rate)));
                line.push('x');
            }
        }
        lines.push(line);
    }
    lines.join("\n")
}

/// A number with no trailing zeros and no `.0` — what `{:%g}` prints, spelled here because Rust has no `%g`. Used for
/// the rate in a clip's line, where "plays at 2x" reads better than "plays at 2.0x".
fn trim_number(value: f64) -> String {
    let mut text = format!("{value}");
    if text.contains('.') {
        while text.ends_with('0') {
            text.pop();
        }
        if text.ends_with('.') {
            text.pop();
        }
    }
    text
}

// --- S2/S3: the call and its outcomes -----------------------------------------------------------------------------------

/// F3.11 S2 (`tools: add_effect(clip, kind zoom|stop|volume, start, end, gain?)`): one proposed decoration — a clip
/// number, one of three kinds, two offsets inside that clip, and volume's gain when it has one. `gain` stays an
/// `Option`: an omitted field and a 1 are the same ask ("nothing"), which S3 ignores, while an explicit 0 is silence
/// and is kept.
#[derive(Debug, Clone, PartialEq)]
pub struct Call {
    pub clip: u32,
    pub kind: String,
    pub start: f64,
    pub end: f64,
    pub gain: Option<f64>,
}

/// What one call came to. `Refused` is §F3.11's "skipped": the reply stays usable and its other calls land. Only a clip
/// outside the list produces a [`Reply::fault`].
#[derive(Debug, Clone, PartialEq)]
pub enum Answer {
    /// A decoration as placed, with the app's defaults on it.
    Placed(Fx),
    /// A volume asking for no change — nothing to do and nothing to say about it.
    Ignored,
    /// The tool's reason this line is unusable: an unknown kind, or a span that is empty inside the clip.
    Refused(String),
}

/// F3.11 S2/S5: a whole reply. `fault` is the only thing that costs a retry ([`cut_captions::retries`] — one more round,
/// then [`no_answer`]).
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

/// F3.11 S2–S5: the reply turned into effects.
///
/// Membership first and over the whole answer — [`cut_captions::clip_outside`] words it exactly as §F3.11 does, `clip N
/// is not one of the clips given (1 to M)`, and it is the same sentence F3.9 and F3.10 produce from the same function:
/// three passes, one formatter, so the number the model has to correct cannot be spelled three ways. A hit means nothing
/// else in the reply is placed and no tool call is made.
///
/// Everything after that is per call. A volume with no gain or a gain of 1 is [`Answer::Ignored`] before the tool is
/// reached: §F3.11 says "gain of 1 or none ignored", and [`Clips::add_effect`] answers that case with an error sentence,
/// which would read to the model as a fault it has to fix when nothing is wrong. Any other error out of the tool —
/// `blur`, or a span that clamps to nothing — is §F3.11's "skipped": [`Answer::Refused`], no effect, reply still usable.
pub fn place(clips: &Clips, calls: &[Call]) -> Reply {
    let count = clips.clips().len() as u32;
    for call in calls {
        if let Some(problem) = cut_captions::clip_outside(call.clip, 1, count) {
            return Reply { effects: Vec::new(), answers: Vec::new(), fault: Some(problem) };
        }
    }

    // The tool's own state, cloned so this stays a function of its arguments: the caller's clips keep their captions,
    // speeds and previously placed effects, and the clamping reads the lengths it was given.
    let mut batch = clips.clone();
    let mut answers = Vec::new();
    let mut effects = Vec::new();
    for call in calls {
        if is_no_change(call) {
            answers.push(Answer::Ignored);
            continue;
        }
        // No box is sent: §F3.11's default is the frame's middle, and a model that wants somewhere else aims through
        // `get_frames` and sends one — which this pass does not offer (§3.7 lists `box?`, the flowchart offers zoom /
        // stop / volume), so the centred default is what every proposed zoom gets.
        let answer = batch.add_effect(call.clip, &call.kind, call.start, call.end, call.gain, None);
        let Ok(body) = serde_json::from_str::<serde_json::Value>(&answer) else {
            answers.push(Answer::Refused(answer.clone()));
            continue;
        };
        if let Some(reason) = body.get("error").and_then(|e| e.as_str()) {
            answers.push(Answer::Refused(reason.to_string()));
            continue;
        }
        let (Some(kind), Some(span)) = (body.get("kind").and_then(|v| v.as_str()), span_of(&body)) else {
            answers.push(Answer::Refused(answer.clone()));
            continue;
        };
        let gain = body.get("gain").and_then(|v| v.as_f64());
        // The clip's own start: the state's session second for it, not a sum of lengths (see cut_speed_pass::place).
        let at = batch.clips().iter().find(|c| c.n == call.clip).expect("S2 checked membership").start;
        let fx = defaults(kind, at, span, gain);
        answers.push(Answer::Placed(fx.clone()));
        effects.push(fx);
    }
    Reply { effects, answers, fault: None }
}

/// F3.11 S3 (`gain of 1 or none ignored`): does this volume ask for a change at all? Asked before the tool runs so the
/// answer is silence rather than the tool's refusal sentence. Only volume has a field that can mean "nothing" — a zoom
/// and a stop always do something.
fn is_no_change(call: &Call) -> bool {
    call.kind == "volume" && matches!(call.gain, None | Some(1.0))
}

/// The span §3.7's tool reported placing (clamped into the clip), as `(from, to)`.
fn span_of(body: &serde_json::Value) -> Option<(f64, f64)> {
    let span = body.get("span")?.as_array()?;
    Some((span.first()?.as_f64()?, span.get(1)?.as_f64()?))
}

// --- S4: the app's defaults ----------------------------------------------------------------------------------------------

/// F3.11 S4 (`zoom centred at 60 % height`): how tall a proposed zoom's region is, as a share of the frame.
/// `// effects.proposedZoomHeight` — §10 gives this no `P.` row, so it takes the bare prefix of the rule that reads it,
/// as F3.10's two rows do. Sixty per cent is the punch-in that keeps a speaker in shot; the tool's own default box is
/// half the frame and belongs to §3.7's report, not to what this app writes into `cut.json`.
pub const ZOOM_HEIGHT: f64 = 0.6;

/// F3.11 S4 (`stop 2 s, fades min(0.3, d/4)`): the cap on a proposed stop's fade at each edge.
/// `// effects.proposedStopFadeSeconds`. This is **not** P.policy.effectDefaultFades' "stop 0.5" — that is §F3.3's value
/// for a stop someone placed by hand from ⏩ Speed, and this flow's own text says 0.3. A frame held for two seconds wants
/// the softer edge; the difference is the spec's, not a drift.
pub const STOP_FADE_CAP_SECONDS: f64 = 0.3;

/// F3.11 S4 (`glides min(1, d/3)`): the glide a proposed zoom gets — [`fx_zoom::GLIDE_SECONDS`], P.policy.effectDefaultFades'
/// zoom value, shrunk to fit a short zoom. A camera that arrives without gliding is a jump cut, which is the same reason
/// §F3.1 gives a hand-placed one the same default.
pub fn glide(dur: f64) -> f64 {
    fx_zoom::GLIDE_SECONDS.min(dur / 3.0)
}

/// F3.11 S4 (`stop … fades min(0.3, d/4)`): a proposed stop's fade at each edge.
pub fn stop_fade(dur: f64) -> f64 {
    STOP_FADE_CAP_SECONDS.min(dur / 4.0)
}

/// F3.11 S4 (`volume ramps min(1, d/4)`): a proposed volume's ramp at each edge — the same 1 s cap F3.10 gives a
/// proposed speed, read from [`cut_speed_pass::PROPOSED_RAMP_SECONDS`] so one number is one decision, with this flow's
/// quarter rather than the speed pass's (both §F3 texts say `d/4`).
pub fn ramp(dur: f64) -> f64 {
    cut_speed_pass::PROPOSED_RAMP_SECONDS.min(dur / 4.0)
}

/// F3.11 S4 (`the app's defaults`): the record a placed decoration becomes, given the span §3.7 reported and the clip's
/// session second. The tool says WHERE an effect landed; what it fades like, how tall its region is, and whether it is a
/// stop or a speed are this app's rules — which is why none of the three uses the `fade` the tool reports (that one is
/// §3.7's own `min(0.15, d/2)`, a report about the tool's answer, not what goes in `cut.json`).
///
/// The three kinds and their records:
/// - `zoom` → centred (`cx`/`cy` 0.5) at [`ZOOM_HEIGHT`] with S4's glide.
/// - `stop` → a **speed whose rate is 0** (`fx_record::is_stop` says so), because §06#1's table has no stop kind: the picture
///   stands still while its footage runs on under it. Its own default length, 2 s — [`cut_speed::STOP_SECONDS`],
///   P.policy.effectDefaultSeconds ("stop/speed/volume/label 2") — is what a stop is asked for when nothing says
///   otherwise; this pass cannot apply it to a span, because the flowchart skips an empty one instead of padding it —
///   which is why the prototype's `if d <= 0 { d = 2 }` was unreachable behind its own parse.
/// - `volume` → the gain as sent, kept even at 0: silence is a decision someone can mean (`fx_record::gain_is_silence`).
pub fn defaults(kind: &str, at: f64, span: (f64, f64), gain: Option<f64>) -> Fx {
    let dur = span.1 - span.0;
    let t = at + span.0;
    match kind {
        "zoom" => {
            let glide = glide(dur);
            Fx {
                kind: "zoom".into(),
                t,
                dur,
                cx: Some(0.5),
                cy: Some(0.5),
                hf: Some(ZOOM_HEIGHT),
                trans: glide,
                tout: glide,
                ..Default::default()
            }
        }
        "stop" => {
            let fade = stop_fade(dur);
            Fx { kind: "speed".into(), t, dur, rate: 0.0, trans: fade, tout: fade, ..Default::default() }
        }
        _ => {
            let ramp = ramp(dur);
            Fx {
                kind: "volume".into(),
                t,
                dur,
                gain: gain.unwrap_or(1.0),
                trans: ramp,
                tout: ramp,
                ..Default::default()
            }
        }
    }
}

// --- S5: what the log says ------------------------------------------------------------------------------------------------

// --- the cap on what one reply may propose -------------------------------------------------------

/// P.machine.maxProposedEffects — the ceiling on how many effects one cut reply may propose.
/// `cut_effects_pass::MAX_PROPOSED_EFFECTS`, the prototype's `fxMaxProposed`.
///
/// A thousand is deliberately not a shape: §10 says so itself ("effectively none: the prompt decides"),
/// and §F3.11 keeps the real limit as the prompt's own wording, "few and deliberate: three or four
/// across five minutes". So this number never curates an edit — it bounds a runaway reply, the answer
/// that streams ten thousand zooms because the model lost the thread. That is also why it lives here
/// rather than in the policy home: nobody tunes it per project.
pub const MAX_PROPOSED_EFFECTS: usize = 1000;

/// How much one kind of effect counts against [`MAX_PROPOSED_EFFECTS`].
///
/// Zero for a speed, one for everything else. The exemption is [`cut_suggest::exempt_from_decorations_cap`]
/// and its reason is 05-cut#8: a rate the cut asked for decides *how long the segment runs*, so letting
/// the caption count decide whether the cap is blown would let the decoration pass hold the cut's shape
/// to ransom. Every other kind adds something on top and is charged one.
pub fn counts_against_effect_cap(kind: crate::cut::EffectKind) -> usize {
    if crate::cut_suggest::exempt_from_decorations_cap(kind) {
        0
    } else {
        1
    }
}

/// What a proposed list counts against the cap.
///
/// An effect whose `kind` string this build does not recognise is charged one rather than waved through:
/// a cap that cannot see a kind is no cap at all, and charging the unknown is the conservative side of
/// the mistake — it trims a decoration nobody can name instead of silently lifting the bound.
pub fn proposed_effect_count(fx: &[crate::cut::Fx]) -> usize {
    fx.iter()
        .map(|effect| match effect.effect_kind() {
            Some(kind) => counts_against_effect_cap(kind),
            None => 1,
        })
        .sum()
}

/// Whether a counted reply fits under the cap.
///
/// Inclusive: a reply of exactly the cap is a reply the cap permits, which is the reading §10's word
/// "cap" carries and the boundary a retune must not shift by accident.
pub fn effect_cap_allows(count: usize) -> bool {
    count <= MAX_PROPOSED_EFFECTS
}

/// F3.11 S5 (`"!!! effects: no usable answer -- the cut stands without them"`): the pass gave up after its one retry and
/// the cut keeps no decorations. ASCII `--` this time, verified against `spec/06-effects.md`'s bytes — as in F3.9's
/// captions line and unlike F3.10's two, which use an em dash; each flow quotes its own punctuation and this file does
/// not quietly unify them. The second half is the promise: a plain cut, and a whole one.
pub fn no_answer() -> String {
    "!!! effects: no usable answer -- the cut stands without them".to_string()
}

/// F3.11 S5: what the pass says when it did answer. Only [`no_answer`] is quoted by §F3.11; this line and
/// [`rejected`] are the prototype's, kept because a run that adds three decorations should say so rather than be silent.
pub fn placed(count: usize) -> String {
    format!(">>> effects: {count} decoration(s)")
}

/// F3.11 S5 (`two rounds fail`, the fault appended and the call put again): what is logged between the attempts — the
/// reason the reply was unusable, said back before it is asked for once more. [`cut_captions::retries`] decides whether
/// another attempt is owed; a third never is.
pub fn rejected(problem: &str) -> String {
    format!(">>> effects rejected: {problem}")
}
