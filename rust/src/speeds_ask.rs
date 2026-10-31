//! F3.10 S1's live half: the speed ask itself.

use crate::clip_pass_ask;
use crate::cut_speed_pass::{Call, GAP_SECONDS};
use crate::layout::Tree;
use crate::settings::Conf;
use crate::tools::Tool;
use std::path::PathBuf;

/// The step name this job's cache entries and timed rows carry.
pub const STEP: &str = "speed";

/// The answer shape §`spec/prompts/system.md`'s `speed` row spells, with the two rules that row states.
pub const SPEEDS_SYSTEM: &str = concat!(
    "Answer in JSON only: {\"speeds\":[{\"clip\":<n>,\"rate\":<x>}]} \u{2014} clip is the clip number from \
     the list, rate > 1. A clip that already has a caption cannot be sped up; leave it out.\n",
    "Strict JSON and nothing else: no prose, no code fence, no comment."
);

pub fn speeds_system(context: &str) -> String {
    clip_pass_ask::system(STEP, context)
}

/// The gap two same-rate stretches may come before they are one run.
fn gap_seconds() -> f64 {
    crate::params::cut()
        .into_iter()
        .find(|row| row.id == "P.policy.speedGapSeconds")
        .and_then(|row| row.spelled.trim().parse::<f64>().ok())
        .filter(|gap| gap.is_finite() && *gap > 0.0)
        .unwrap_or(GAP_SECONDS)
}

/// F3.10 S1: the user message — every clip with its own seconds, which of them already carry captions, the
/// footage the cut comes to, and the rules a rate has to respect.
pub fn user_message(
    clips: &[(u32, f64)],
    captioned: &[(u32, usize)],
    total_seconds: f64,
) -> String {
    user_message_over(clips, captioned, total_seconds, &[], &[])
}

/// §9 (`With more than one camera row or lane the cut brief names the rows and lanes`): the
/// speed ask with the session's camera rows and heard lanes named above the clip list.
pub fn user_message_over(
    clips: &[(u32, f64)],
    captioned: &[(u32, usize)],
    total_seconds: f64,
    rows: &[String],
    lanes: &[String],
) -> String {
    let gap = gap_seconds();
    let mut lines = vec![clip_pass_ask::footage_line(total_seconds, clips.len())];
    lines.extend(crate::effect_overlap::brief_lines(rows, lanes));
    lines.push(String::new());
    lines.push(crate::cut_captions::CLIPS_HEADER.to_string());
    for (number, length) in clips {
        let mut line = crate::cut_captions::clip_line(*number, *length);
        if let Some((_, count)) = captioned.iter().find(|(clip, _)| clip == number) {
            line.push_str(&format!(", {count} caption(s) -- runs at 1"));
        }
        lines.push(line);
    }
    lines.push(String::new());
    lines.push(format!(
        "Total video {total_seconds:.0} s of footage. Speed only what would bore, and answer with the clips \
         you would speed and nothing else \u{2014} a cut with nothing dull in it gets no rates at all."
    ));
    lines.push(
        "- Only rates above 1. Below 1 costs seconds instead of saving them, and is not this job."
            .to_string(),
    );
    lines.push(
        "- Never a timestamp and never a second count: a clip is one rate from end to end."
            .to_string(),
    );
    lines.push("- A clip marked \"captions\" plays at 1 whatever else is true: words on screen at 4 are gone before they are read.".to_string());
    lines.push(format!(
        "- Two stretches at the same rate nearer than {gap} s apart are one stretch, so give them one rate."
    ));
    lines.join("\n")
}

/// The chat body: thinking OFF (§4 does not list `speed` among the four prompts that think), and this job's
/// tools offered — `set_clip_speed` for the work and `finish` to close the reply.
pub fn chat_body(model: &str, system: &str, user: &str) -> Result<String, String> {
    clip_pass_ask::conversation_body(
        STEP,
        model,
        &clip_pass_ask::two_messages(system, user),
        Some(&clip_pass_ask::offered_with(Tool::SetClipSpeed)),
    )
}

pub fn parse_speeds(reply: &str) -> Vec<Call> {
    clip_pass_ask::parse(reply, Tool::SetClipSpeed, "speeds", &call_of_args)
}

/// One `{clip, rate}` pair, from either spelling.
pub fn call_of_args(value: &serde_json::Value) -> Option<Call> {
    Some(Call {
        clip: clip_pass_ask::clip_number(value)?,
        rate: clip_pass_ask::number(value, "rate")?,
    })
}

pub type Asked = clip_pass_ask::Asked<Call>;

/// One live speed ask: build the two messages, honour the cache, dial, read the reply.
pub fn ask(
    tree: &Tree,
    context: &str,
    clips: &[(u32, f64)],
    captioned: &[(u32, usize)],
    attempt: u32,
    cancel: &dyn Fn() -> bool,
) -> Asked {
    ask_named(tree, context, clips, captioned, &[], &[], attempt, cancel)
}

/// [`ask`] with the session's camera rows and heard lanes named in the brief (§9: the passes
/// know nothing of cameras and lanes until there is more than one of either to tell apart).
#[allow(clippy::too_many_arguments)] // the ask carries the brief's rows and lanes beside its clips
pub fn ask_named(
    tree: &Tree,
    context: &str,
    clips: &[(u32, f64)],
    captioned: &[(u32, usize)],
    rows: &[String],
    lanes: &[String],
    attempt: u32,
    cancel: &dyn Fn() -> bool,
) -> Asked {
    match clip_pass_ask::settings_with_address() {
        Ok(conf) => ask_at_over(
            tree, &conf, context, clips, captioned, rows, lanes, attempt, cancel,
        ),
        Err(reason) => Asked::failed(reason),
    }
}

/// [`ask`] with the settings already read, for a caller that resolved them itself.
pub fn ask_at(
    tree: &Tree,
    conf: &Conf,
    context: &str,
    clips: &[(u32, f64)],
    captioned: &[(u32, usize)],
    attempt: u32,
    cancel: &dyn Fn() -> bool,
) -> Asked {
    ask_at_over(
        tree,
        conf,
        context,
        clips,
        captioned,
        &[],
        &[],
        attempt,
        cancel,
    )
}

/// [`ask_at`] with the session's camera rows and heard lanes named in the brief (§9).
#[allow(clippy::too_many_arguments)] // as [`ask_named`]: the brief's rows and lanes ride along
pub fn ask_at_over(
    tree: &Tree,
    conf: &Conf,
    context: &str,
    clips: &[(u32, f64)],
    captioned: &[(u32, usize)],
    rows: &[String],
    lanes: &[String],
    attempt: u32,
    cancel: &dyn Fn() -> bool,
) -> Asked {
    if clips.is_empty() {
        return Asked::answered(Vec::new());
    }
    let total: f64 = clips.iter().map(|(_, length)| length).sum();
    // The refusal a captioned clip earns comes back as the tool's own sentence, so it has to be REACHED.
    let mut batch = crate::clip_rules_tools_leg::batch_of(clips);
    for (clip, count) in captioned {
        for index in 0..*count {
            batch.add_caption(*clip, 0.0, 0.5, &format!("caption {index}"));
        }
    }
    clip_pass_ask::run(
        tree,
        conf,
        cancel,
        &clip_pass_ask::Pass {
            step: STEP,
            tool: Tool::SetClipSpeed,
            tools: clip_pass_ask::offered_with(Tool::SetClipSpeed),
            model: &conf.model,
            system: &speeds_system(context),
            user: &user_message_over(clips, captioned, total, rows, lanes),
            attempt,
            batch: batch,
            parse: &parse_speeds,
            call_of_args: &call_of_args,
        },
    )
}

pub fn cache_file_for_test(
    tree: &Tree,
    context: &str,
    clips: &[(u32, f64)],
    captioned: &[(u32, usize)],
    model: &str,
    attempt: u32,
) -> PathBuf {
    let total: f64 = clips.iter().map(|(_, length)| length).sum();
    clip_pass_ask::cache_file(
        tree,
        STEP,
        &speeds_system(context),
        &user_message(clips, captioned, total),
        model,
        attempt,
    )
}
