//! F3.11 S1's live half: the decorations ask itself.

use crate::clip_pass_ask;
use crate::cut_effects_pass::{self, Call};
use crate::layout::Tree;
use crate::settings::Conf;
use crate::tools::clips::Clips;
use crate::tools::Tool;
use std::path::PathBuf;

pub const STEP: &str = "effects";

pub const EFFECTS_SYSTEM: &str = concat!(
    "Answer in JSON only: {\"fx\":[{\"clip\":<n>,\"kind\":\"zoom|stop|volume\",\"start\":<sec>,\"end\":<sec>,\
     \"gain\":<x>}]} \u{2014} clip is the clip number from the list, seconds are from that clip's start and \
     stay inside it, gain belongs to volume alone (1 as recorded, 0 silent).\n",
    "Few and deliberate: about three or four across five minutes unless the context says otherwise. A zoom \
     runs two to four seconds. One stop per video. Volume for level, ducking or muting. Never zoom past \
     captions.\n",
    "Strict JSON and nothing else: no prose, no code fence, no comment."
);

pub fn effects_system(context: &str) -> String {
    clip_pass_ask::system(STEP, context)
}

pub fn user_message(clips: &Clips, speeds: &[(u32, f64)]) -> String {
    user_message_over(clips, speeds, &[], &[])
}

pub fn user_message_over(
    clips: &Clips,
    speeds: &[(u32, f64)],
    rows: &[String],
    lanes: &[String],
) -> String {
    let total: f64 = clips.clips().iter().map(|clip| clip.length).sum();
    let mut lines = vec![clip_pass_ask::footage_line(total, clips.clips().len())];
    lines.push(String::new());
    lines.push(cut_effects_pass::brief_over(clips, speeds, rows, lanes));
    lines.push(String::new());
    lines.push(
        "Decorate only what needs it, and answer with those decorations and nothing else \u{2014} a cut that \
         wants no decoration gets an empty list."
            .to_string(),
    );
    lines.push("- Few and deliberate: about three or four across five minutes of finished video, each with a \
                reason you could say out loud. The User Context outranks that count."
                .to_string());
    lines.push("- A zoom runs two to four seconds. One stop per video, a second or two on the beat the video \
                is about.".to_string());
    lines.push("- volume sets how loud those seconds are: 1 as recorded, 0 silent. Use it for a stretch \
                recorded too quiet or too loud, for ducking a background under a line that matters, and for \
                muting what the context says is not to be heard.".to_string());
    lines.push("- Every effect names its clip and its seconds FROM THAT CLIP'S START, inside the clip. Never \
                a session timestamp.".to_string());
    lines.push("- A clip marked \"captions\" has words the viewer is reading: do not zoom past them. A clip \
                that plays at a rate was kept to be skipped through \u{2014} nothing to zoom onto there.".to_string());
    lines.join("\n")
}

pub fn chat_body(model: &str, system: &str, user: &str) -> Result<String, String> {
    // No `prompt_cache_key`: §1's request contract names model, the two messages, `tools` when the job has
    // them, and the sampling five — and `llm_request::body` is the one place a chat request is built (§09 §5).
    clip_pass_ask::conversation_body(
        STEP,
        model,
        &clip_pass_ask::two_messages(system, user),
        Some(&clip_pass_ask::offered_with(Tool::AddEffect)),
    )
}

pub fn parse_effects(reply: &str) -> Vec<Call> {
    clip_pass_ask::parse(reply, Tool::AddEffect, "fx", &call_of_args)
}

/// One `{clip, kind, start, end, gain?}` entry, from either spelling.
pub fn call_of_args(value: &serde_json::Value) -> Option<Call> {
    let kind = value
        .get("kind")
        .and_then(|kind| kind.as_str())
        .map(str::trim)
        .filter(|kind| !kind.is_empty())?
        .to_string();
    Some(Call {
        clip: clip_pass_ask::clip_number(value)?,
        kind,
        start: clip_pass_ask::number(value, "start")?,
        end: clip_pass_ask::number(value, "end")?,
        gain: clip_pass_ask::number(value, "gain"),
    })
}

pub type Asked = clip_pass_ask::Asked<Call>;

pub fn ask(
    tree: &Tree,
    context: &str,
    clips: &Clips,
    speeds: &[(u32, f64)],
    attempt: u32,
    cancel: &dyn Fn() -> bool,
) -> Asked {
    ask_named(tree, context, clips, speeds, &[], &[], attempt, cancel)
}

/// [`ask`] with the session's camera rows and heard lanes named in the brief (§9).
#[allow(clippy::too_many_arguments)] // the ask carries the brief's rows and lanes beside its clips
pub fn ask_named(
    tree: &Tree,
    context: &str,
    clips: &Clips,
    speeds: &[(u32, f64)],
    rows: &[String],
    lanes: &[String],
    attempt: u32,
    cancel: &dyn Fn() -> bool,
) -> Asked {
    match clip_pass_ask::settings_with_address() {
        Ok(conf) => ask_at_over(
            tree, &conf, context, clips, speeds, rows, lanes, attempt, cancel,
        ),
        Err(reason) => Asked::failed(reason),
    }
}

/// [`ask`] with the settings already read, for a caller that resolved them itself.
pub fn ask_at(
    tree: &Tree,
    conf: &Conf,
    context: &str,
    clips: &Clips,
    speeds: &[(u32, f64)],
    attempt: u32,
    cancel: &dyn Fn() -> bool,
) -> Asked {
    ask_at_over(
        tree,
        conf,
        context,
        clips,
        speeds,
        &[],
        &[],
        attempt,
        cancel,
    )
}

#[allow(clippy::too_many_arguments)] // as [`ask_named`]: the brief's rows and lanes ride along
pub fn ask_at_over(
    tree: &Tree,
    conf: &Conf,
    context: &str,
    clips: &Clips,
    speeds: &[(u32, f64)],
    rows: &[String],
    lanes: &[String],
    attempt: u32,
    cancel: &dyn Fn() -> bool,
) -> Asked {
    if clips.clips().is_empty() {
        // Nothing to ask about is not a failure and not worth a request.
        return Asked::answered(Vec::new());
    }
    clip_pass_ask::run(
        tree,
        conf,
        cancel,
        &clip_pass_ask::Pass {
            step: STEP,
            tool: Tool::AddEffect,
            tools: clip_pass_ask::offered_with(Tool::AddEffect),
            model: &conf.model,
            system: &effects_system(context),
            user: &user_message_over(clips, speeds, rows, lanes),
            attempt,
            batch: clips.clone(),
            parse: &parse_effects,
            call_of_args: &call_of_args,
        },
    )
}

pub fn cache_file_for_test(
    tree: &Tree,
    context: &str,
    clips: &Clips,
    speeds: &[(u32, f64)],
    model: &str,
    attempt: u32,
) -> PathBuf {
    clip_pass_ask::cache_file(
        tree,
        STEP,
        &effects_system(context),
        &user_message(clips, speeds),
        model,
        attempt,
    )
}
