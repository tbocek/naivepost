//! F3.9 Captions proposed by the model (after the cut) — `spec/06-effects.md` F3.9, steps S1–S5.
//!
//! ▶ after the cut: the kept clips go to the model in batches of P.machine.captionBatch and come back as
//! `add_caption(clip, start, end, text)` calls closed by `finish`. What this module owns is everything *around*
//! those calls — how many clips one request carries (S1), what the message says (S2), that a clip number outside
//! the batch throws away the whole reply and costs exactly one retry (S4), that a caption under P.policy.
//! captionMinSeconds or without words is dropped without a word (S5), the fades F3.9 asks for, and the line the
//! log gets when a batch keeps failing (S6).
//!
//! What it deliberately does not own: §3.7's tool itself. [`crate::tools::clips::Clips::add_caption`] already
//! clamps an offset into its clip and answers with the caption as placed, and its errors are what the model is
//! told; this module decides what the *app* does with each answer. Nor does it place a session second from thin
//! air: the model only ever names offsets inside the clip it was shown, and turning one into a session second is
//! this module's arithmetic — the rule that a model is never asked to compute (`spec/00-principles.md`).
//! `rust/src/ui/window.rs` renders the pass through [`crate::ui::run_captions_pass`]: it batches the kept
//! clips, asks (or is handed the reply by a test), places through [`place`], and the accepted records land on
//! the lane via `refresh_effects_lane`. The pass is gated by `Pass::Captions` (`policy::pass_runs`), which is
//! what greys the ✐ Captions control off when the context ruled captions out.

use crate::cut::Fx;
use crate::fx_text;
use crate::tools::clips::Clips;

// --- S1: how many clips one request carries ------------------------------------------------------------------------

/// F3.9 S1 (`clips in batches of P.machine.captionBatch`): five per request. Five is a batch small enough that
/// the model still remembers which clip it is answering about — every number it sends is a clip *number* — and
/// large enough that a session's hundred clips are twenty requests rather than a hundred.
pub const BATCH: usize = 5;

/// F3.9 S2 (`message = User Context + "THE CLIPS, AND WHAT WAS SAID OVER EACH:"`): the header of the clip list,
/// §F3.9's own wording in caps because it is the line that separates what was said from what is being asked.
pub const CLIPS_HEADER: &str = "THE CLIPS, AND WHAT WAS SAID OVER EACH:";

/// F3.9 S1: the batches one run asks about, as 1-based INCLUSIVE clip-number ranges — five clips each, a short
/// final batch, and no request at all for nothing to caption (an empty cut is not worth a call). The range is what
/// S4's refusal names and what S6's log line prints, so it is computed once here rather than re-derived twice.
pub fn batches(count: usize) -> Vec<(u32, u32)> {
    let mut ranges = Vec::new();
    let mut first = 1u32;
    while (first as usize) <= count {
        let last = (first as usize + BATCH - 1).min(count) as u32;
        ranges.push((first, last));
        first = last + 1;
    }
    ranges
}

// --- the cleaning rule the captions pass is told (P.policy.keepSwearing) -------------------------

/// `P.policy.keepSwearing` (default true): the captions pass keeps the speaker's swearing. The
/// prototype carried this as part of `captionSystem` — the whole captions prompt — because the rule
/// rides in the wording rather than in any arithmetic (§10 names that constant for exactly this row).
pub const KEEP_SWEARING_DEFAULT: bool = true;

/// The cleaning sentence as the shipped prompt states it (`spec/prompts/captions.md`), copied so a
/// test can pin the two against each other and neither can drift.
pub const CLEANING_RULE: &str = "Clean the words as a subtitler would: no ehm, no ehh, no stutters \
                                (\"I I\" is \"I\"), no repeated words, sentence case, the swearing kept.";

/// The clause the toggle moves. Kept separate from [`CLEANING_RULE`] so turning the policy off removes
/// one promise rather than rewriting the sentence by hand at the call site.
const SWEARING_CLAUSE: &str = ", the swearing kept";

/// What the model is told about cleaning the words.
///
/// On (the default): the whole rule, including that the swearing stays. Off: the same sentence with
/// only that clause dropped — the stutter, repeat and sentence-case cleaning are untouched, because the
/// policy says nothing about them. Decided here because §10 leaves what "off" does unstated: the only
/// honest off is the app no longer promising the model the words stay. Inventing a censor style would
/// put a rule in the app that no spec asked for.
pub fn cleaning_rule(keep_swearing: bool) -> String {
    if keep_swearing {
        return CLEANING_RULE.to_string();
    }
    CLEANING_RULE.replace(SWEARING_CLAUSE, "")
}

// --- S2: the message -----------------------------------------------------------------------------------------------

/// F3.9 S2 (`"CLIP n: X s long"`): one clip, in its own seconds. One decimal because that is §F3.9's example
/// (`CLIP 1: 14.2 s long`) and because a tenth of a second is finer than anyone captions to.
pub fn clip_line(n: u32, length: f64) -> String {
    format!("CLIP {n}: {length:.1} s long")
}

/// F3.9 S2 (`the lines as offsets`, `[+0.4s] …`): what was said, timed from the START OF ITS CLIP. Never a session
/// second: the model is shown one clip at a time and cannot know where that clip sits in the session, so a session
/// second here would be an unanswerable number — and the standing rule is that a model is never asked for a
/// timestamp it would have to compute. The `+` says which way the offset runs.
pub fn offset_line(offset: f64, text: &str) -> String {
    format!("[+{offset:.1}s] {text}")
}

/// F3.9 S2: the whole request — the User Context first (it is the reason captions are wanted at all: §F3.9's "the
/// context asks"), then the header, then each clip's line followed by what was said over it, in clip order and
/// offset order.
///
/// Two decisions the spec leaves open, both about not wasting the model's attention: an empty User Context adds
/// nothing at all, not a blank block (a message that opens with whitespace reads as a mistake to be ignored); and
/// words belonging to a clip this batch does not contain are left out rather than sent unlabelled.
pub fn message(user_context: &str, clips: &[(u32, f64)], words: &[(u32, f64, &str)]) -> String {
    let given: Vec<u32> = clips.iter().map(|(n, _)| *n).collect();
    let mut lines: Vec<String> = Vec::new();
    if !user_context.trim().is_empty() {
        lines.push(user_context.trim().to_string());
        lines.push(String::new());
    }
    lines.push(CLIPS_HEADER.to_string());
    for (n, length) in clips {
        lines.push(clip_line(*n, *length));
        for (clip, offset, text) in words.iter().filter(|(clip, _, _)| given.contains(clip)) {
            if clip == n {
                lines.push(offset_line(*offset, text));
            }
        }
    }
    lines.join("\n")
}

// --- S3: the answer's shape -----------------------------------------------------------------------------------------

/// F3.9 S3 (`tools: add_caption(clip, start, end, text)`): one caption as the model asked for it — a clip NUMBER
/// and two offsets inside that clip, never seconds of the session. `finish` closes the reply; [`Call`] is what the
/// page collects from the tool calls before this module sees them.
#[derive(Debug, Clone, PartialEq)]
pub struct Call {
    pub clip: u32,
    pub start: f64,
    pub end: f64,
    pub text: String,
}

// --- S4/S5: what the app does with the reply --------------------------------------------------------------------------

/// F3.9 S4 (`the whole reply rejected, retried once`): either every caption in the reply was placed, or none was —
/// a batch is one answer about a known set of clips, so half of it cannot be trusted. The string is §F3.6's own
/// sentence in the flowchart's red box: what the model got wrong, and the range it should have stayed inside.
#[derive(Debug, Clone, PartialEq)]
pub enum Reply {
    /// Every caption that survived S5's floor, as session-relative records.
    Accepted(Vec<Fx>),
    /// The clip number that gave the game away — the whole reply is thrown away and this is said to it.
    Rejected(String),
}

/// F3.9 S4 (`"clip N is not one of the clips given (a to b)"`): the membership check, done by the pass rather than
/// by §3.7's tool. [`Clips::add_caption`] answers `clip N is not in this batch -- it holds M clip(s)`, which
/// is a fine thing to tell a model about one call; F3.9's rejection is about the REPLY and names the batch by its
/// range, because that is the number the model has to correct (a caption for clip 11 sent into the batch 6–10 means
/// it counted from the wrong end).
pub fn clip_outside(clip: u32, first: u32, last: u32) -> Option<String> {
    if (first..=last).contains(&clip) {
        return None;
    }
    Some(format!("clip {clip} is not one of the clips given ({first} to {last})"))
}

/// F3.9 S4 (`retried once`): a rejected reply goes out again, and only once more. `runs` counts the attempts made
/// at this batch so far, so after one rejection the retry is asked for (`runs == 1`) and after two it is not — the
/// third attempt never happens and S6's line is what is left. [`crate::roles::TOOL_ROUNDS`] budgets the tool rounds
/// *inside* one request; this is about a whole reply being unusable, so the two numbers stay separate.
pub fn retries(runs: u32) -> bool {
    runs == 1
}

/// F3.9 S4 + S5 (`yes → placed · fades min 0.3, d/4` / `no → skipped`): the reply turned into effects.
///
/// A clip number outside this batch rejects everything, including captions that were fine — collected first and
/// dropped whole, never streamed into the cut as they are read. Everything else goes through §3.7's tool, which
/// clamps offsets into the clip and refuses a span under [`CAPTION_MIN_SECONDS`] (P.policy.captionMinSeconds)
/// or empty words; those two refusals are SKIPPED silently (§F3.9: "a caption under the floor or with no words is
/// silently skipped") while their valid neighbours in the same reply still land. Any other error from the tool — a
/// clip that vanished, say — is not silence but a fault, so it rejects the reply like S4 does.
pub fn place(clips_in: &[(u32, f64)], calls: &[Call]) -> Reply {
    let (first, last) = match (clips_in.first(), clips_in.last()) {
        (Some((first, _)), Some((last, _))) => (*first, *last),
        // Nothing was given, so nothing can be right about it. Named as a range of noughts because that is what
        // the model would have to be told: there were no clips to caption.
        _ => return Reply::Rejected("no clips were given (0 to 0)".to_string()),
    };
    for call in calls {
        if let Some(reason) = clip_outside(call.clip, first, last) {
            return Reply::Rejected(reason);
        }
    }

    // The tool's own state: a batch of clips with the batch's lengths, so its clamping is real clamping. Each
    // clip's session second comes from [`session_start`] — the sum of what came before — because this signature
    // carries lengths only; a caller that knows the real positions reads them through [`on_the_timeline_at`].
    let mut batch = Clips::new(
        &clips_in.iter().map(|(n, _)| (*n, session_start(clips_in, *n), length_of(clips_in, *n))).collect::<Vec<_>>(),
    );
    let mut placed = Vec::new();
    for call in calls {
        let answer = batch.add_caption(call.clip, call.start, call.end, &call.text);
        if !is_ok(&answer) {
            // S5's two reasons are silence; anything else is the reply being wrong about the batch.
            if is_skippable(&answer) {
                continue;
            }
            return Reply::Rejected(answer);
        }
        let Some((span, text)) = placed_caption(&answer) else {
            return Reply::Rejected(answer);
        };
        placed.push(on_the_timeline(clips_in, call.clip, span, &text));
    }
    Reply::Accepted(placed)
}

/// The length one clip number was given. `position` in the batch is a lookup this module already performs once per
/// clip when it builds [`Clips`], and a missing clip cannot reach here (S4 rejected the reply before that).
fn length_of(clips_in: &[(u32, f64)], clip: u32) -> f64 {
    clips_in.iter().find(|(n, _)| *n == clip).map(|(_, length)| *length).unwrap_or_default()
}

/// F3.9 S5 (`fades min 0.3, d/4`): a caption's fade at each edge — P.policy.effectDefaultFades' text value (0.3)
/// capped by a quarter of the caption's own length, so a short caption cannot spend most of itself fading in and
/// out. This is F3.9's number for captions it proposes, read from [`fx_text::FADE_SECONDS`] rather than written
/// again; it is NOT [`FADE_SECONDS`] (0.15), which is §3.7's *tool* fade reported back inside the tool's own
/// answer, nor [`fx_text`]'s default for a caption placed by hand from a form.
pub fn fade(dur: f64) -> f64 {
    fx_text::FADE_SECONDS.min(dur / 4.0)
}

/// F3.9 S5 (`placed`): the record a caption becomes — a text effect at the SESSION second its clip starts at plus
/// the offset the model gave, for as long as the clamped span, with S5's fades and no box (so [`Fx::centre`] puts
/// it in the lower third, as every other caption sits). `kind` is `"text"`, which is what `cut.json` holds for a
/// caption whether a person typed it or a model proposed it: one record, two ways of arriving.
pub fn on_the_timeline(clips_in: &[(u32, f64)], clip: u32, span: (f64, f64), text: &str) -> Fx {
    let dur = span.1 - span.0;
    Fx {
        kind: "text".into(),
        t: session_start(clips_in, clip) + span.0,
        dur,
        trans: fade(dur),
        tout: fade(dur),
        text: text.to_string(),
        ..Default::default()
    }
}

/// F3.9 S5: where a clip sits in the session, from the `(number, length)` pairs a caller has when it knows only
/// how long each clip is: the sum of what came before, at 1×. A caption's second is therefore always computed here
/// and never asked for — §F3.9's whole point about offsets — and a caller that knows the real positions passes them
/// through [`on_the_timeline_at`] instead of trusting a sum (a sped-up or trimmed clip does not add up to its own
/// length). Summing by hand rather than with `Iterator::sum`: an empty prefix sums to NEGATIVE zero there, which
/// would print as `-0` in a message and compare equal-but-hairily under the first caption's second.
pub fn session_start(clips_in: &[(u32, f64)], clip: u32) -> f64 {
    let mut seconds = 0.0f64;
    for (_, length) in clips_in.iter().take_while(|(n, _)| *n != clip) {
        seconds += length;
    }
    seconds
}

/// F3.9 S5: the same record at a session second the caller supplies — `(number, session start, length)` triples,
/// the shape [`Clips`] itself takes. Separate from [`session_start`] so a caller with real positions never gets a
/// sum-of-lengths answer by accident.
pub fn on_the_timeline_at(start: f64, span: (f64, f64), text: &str) -> Fx {
    let dur = span.1 - span.0;
    Fx {
        kind: "text".into(),
        t: start + span.0,
        dur,
        trans: fade(dur),
        tout: fade(dur),
        text: text.to_string(),
        ..Default::default()
    }
}

// --- S6: the batch that keeps failing --------------------------------------------------------------------------------

/// F3.9 S6 (`"!!! captions: clips a–b skipped -- the cut stands without them"`): what the log says when a batch has
/// been asked twice and both replies were rejected. En dash between the clip numbers, as every range in this app
/// prints one; `--` before the clause, as [`crate::rescan`] and [`crate::align`] write their `!!!` lines. The
/// sentence's job is to say what is missing AND that nothing else was lost: captions are worth having and not worth
/// a failed run over.
pub fn skipped(first: u32, last: u32) -> String {
    format!("!!! captions: clips {first}\u{2013}{last} skipped -- the cut stands without them")
}

// --- helpers ----------------------------------------------------------------------------------------------------------

/// Whether §3.7's tool answered with success rather than with its `{}` error shape. Read as JSON, because both of
/// its answers are JSON and matching on a prefix would break the moment a sentence starts like a key.
fn is_ok(answer: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(answer).is_ok_and(|body| body.get("error").is_none())
}

/// The two refusals F3.9 wants silence for, recognised by the words §3.7's tool uses for them — under the floor, or
/// no words in it. Both name themselves, which is what lets this be a check rather than a guess: the floor's error
/// says the caption "is dropped, so move it inside the clip's …", and the empty one is "the caption has no words in
/// it". Read out of the `{}` shape [`tools::error`] writes, so a sentence merely quoting those words elsewhere in a
/// success body cannot silence a real fault.
fn is_skippable(answer: &str) -> bool {
    let Ok(body) = serde_json::from_str::<serde_json::Value>(answer) else {
        return false;
    };
    let Some(reason) = body.get("error").and_then(|e| e.as_str()) else {
        return false;
    };
    reason.contains("dropped") || reason.contains("no words in it")
}

/// What the tool reported placing: the clamped span and the words it will draw, as `(span, text)`.
fn placed_caption(answer: &str) -> Option<((f64, f64), String)> {
    let body = serde_json::from_str::<serde_json::Value>(answer).ok()?;
    let span = body.get("span")?.as_array()?;
    let from = span.first()?.as_f64()?;
    let to = span.get(1)?.as_f64()?;
    Some(((from, to), body.get("text")?.as_str()?.to_string()))
}
