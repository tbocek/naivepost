//! F1.6 S6's live judge: two frames of one recording shown to the decision model (Settings → Decision), which
//! answers through `tool:judge_text` whether the text on screen changed between them. Modelled on
//! [`crate::policy_call`] (the other ask that belongs to no job): the shipped `textgate` prompt through the
//! settings read path, thinking off, one dial through [`crate::llm_retry_leg`], and every answer filed in the
//! LLM cache under the two pictures' own bytes, so a rerun of Prepare asks nothing again.

use crate::exchanges::{Message, Part};
use crate::layout::Tree;
use crate::llm_cache;
use crate::llm_request;
use crate::services::{self, Server};
use crate::settings::{self, Conf, Paths};
use crate::text_gate::STEP;
use crate::tools::Tool;

/// Did the text change between the frames at `a` and `b` seconds into `lane`'s file? Shown the 896-wide copies
/// F1.7 sends (`describe_scale::ensure` made them). An answer without `judge_text` is a failure, said with what
/// the model wrote: the gate keeps the pixel pass's change rather than guessing from prose.
pub fn judge(
    tree: &Tree,
    paths: &Paths,
    conf: &Conf,
    lane: &str,
    base: f64,
    a: f64,
    b: f64,
) -> Result<bool, String> {
    let model = conf.decider();
    let dir = tree.llm_frames_dir(lane);
    let read = |at: f64| {
        let name = crate::describe::scaled_name(base, at);
        std::fs::read(dir.join(&name))
            .map_err(|_| format!("{}: no scaled frame {name} to judge", dir.display()))
    };
    let (first, second) = (read(a)?, read(b)?);
    let urls = [
        llm_request::data_url(&first, "image/jpeg"),
        llm_request::data_url(&second, "image/jpeg"),
    ];
    let system = wording(paths);
    let user = format!("FIRST is {a:.2} s into the recording, SECOND is {b:.2} s.");
    let parts = llm_cache::parts(&llm_cache::Request {
        system: &system,
        user: &user,
        state: "",
        speech: "",
        context: "",
        images: urls.to_vec(),
        run_index: None,
        model: &model,
        thinking: crate::prompts::thinking(STEP),
    });
    if let Some(cached) = llm_cache::read(tree, STEP, &parts) {
        crate::request_timing::record_cached(tree, log_row(&model));
        return changed_of(&cached);
    }
    let messages = messages(&system, &user, &urls);
    let endpoint = services::llm_endpoint(conf);
    let offered: Vec<Tool> = crate::tools::text_gate_tools();
    let tools = crate::llm_tools_offer::tools_for(&endpoint.url, &offered).ok_or_else(|| {
        format!(
            "{model} at {} refused the tools field, and the gate asks through judge_text",
            endpoint.url
        )
    })?;
    let body = llm_request::body(
        STEP,
        &model,
        &messages,
        crate::prompts::mode_of(STEP),
        Some(&tools),
        true,
    )?
    .to_string();
    let sent = crate::llm_retry_leg::dial(
        STEP,
        tree,
        &log_row(&model),
        &endpoint,
        &body,
        &crate::cancel_leg::cancel_check_now(),
        &|_| {},
        &crate::llm_retry_leg::nap,
    )
    .answer
    .map_err(|why| format!("{STEP} call to {model}: {why}"))?;
    if !sent.ok() {
        return Err(format!(
            "{STEP} call to {model} answered {}: {}",
            sent.status,
            sent.body.trim()
        ));
    }
    let args = judge_args(&model, &sent.content_type, &sent.body)?;
    let verdict = changed_of(&args)?;
    llm_cache::store(tree, STEP, &parts, &args);
    Ok(verdict)
}

/// The gate's two messages: the `textgate` wording as system, then the stamps line and FIRST and SECOND.
fn messages(system: &str, user: &str, urls: &[String; 2]) -> Vec<Message> {
    vec![
        Message {
            role: "system".to_string(),
            parts: vec![Part::Text(system.to_string())],
        },
        Message {
            role: "user".to_string(),
            parts: vec![
                Part::Text(user.to_string()),
                Part::Text("FIRST:".to_string()),
                Part::Image(urls[0].clone()),
                Part::Text("SECOND:".to_string()),
                Part::Image(urls[1].clone()),
            ],
        },
    ]
}

/// The `judge_text` call's arguments out of a reply, or what the model wrote instead.
fn judge_args(model: &str, content_type: &str, body: &str) -> Result<String, String> {
    let reply =
        llm_request::reply_from(content_type, body).map_err(|why| format!("{STEP}: {why}"))?;
    reply
        .tool_calls
        .iter()
        .find(|call| call.name == "judge_text")
        .map(|call| call.args.clone())
        .ok_or_else(|| {
            format!(
                "{model} answered without judge_text: {}",
                reply.answer().chars().take(120).collect::<String>()
            )
        })
}

/// The Decision row's Test (§03-shell §5): the gate's own request, prompt, tool and all, with the probe's red
/// square as both FIRST and SECOND. Two identical pictures have no text that changed, so a model fit to be the
/// gate answers through `judge_text` with `changed` false; prose, a `true` or a refused call each say what the
/// gate would get from it. `ask` posts one chat body and returns the reply's JSON (the dialog's own leg).
pub fn probe(
    conf: &Conf,
    paths: Option<&Paths>,
    ask: &dyn Fn(&str, Option<u64>) -> Result<String, String>,
) -> Result<String, String> {
    let model = conf.decider();
    if model.is_empty() {
        return Err(
            "no model id -- the text gate asks the Decision box, else the Model box".to_string(),
        );
    }
    let system = match paths {
        Some(paths) => wording(paths),
        None => crate::prompts::shipped_text(STEP).unwrap_or_default(),
    };
    let square = crate::checks::vision_probe_url();
    let urls = [square.clone(), square];
    let body = llm_request::body(
        STEP,
        &model,
        &messages(
            &system,
            "FIRST is 0.00 s into the recording, SECOND is 1.00 s.",
            &urls,
        ),
        crate::prompts::mode_of(STEP),
        Some(&crate::tools::text_gate_tools()),
        // Not streamed: the dialog's leg hands back the body without its content type, so the reply is asked
        // for as one JSON envelope. Everything else is the gate's own request.
        false,
    )?
    .to_string();
    let began = std::time::Instant::now();
    let reply = ask(&body, Some(crate::model_list::probes()[1].seconds))?;
    let ms = began.elapsed().as_millis();
    if changed_of(&judge_args(&model, "application/json", &reply)?)? {
        return Err(format!(
            "{model} says two identical frames differ -- as the text gate it would split every scene"
        ));
    }
    Ok(format!(
        "{model} judged two identical frames the same through judge_text in {ms} ms"
    ))
}

/// `judge_text`'s arguments: `{"changed": true|false, "what": "…"}`; a `changed` that is not a boolean is refused.
pub fn changed_of(args: &str) -> Result<bool, String> {
    let value: serde_json::Value = serde_json::from_str(args)
        .map_err(|why| format!("judge_text arguments are not JSON ({why}): {args}"))?;
    value
        .get("changed")
        .and_then(serde_json::Value::as_bool)
        .ok_or_else(|| format!("judge_text without a true/false \"changed\": {args}"))
}

/// The `textgate` wording, through the settings read path so an edit on the bench wins.
fn wording(paths: &Paths) -> String {
    let fallback = crate::prompts::shipped_text(STEP).unwrap_or_default();
    settings::prompt_text(paths, STEP, &fallback).unwrap_or(fallback)
}

/// The timed `requests.tsv` row and exchange page this ask is filed under.
fn log_row(model: &str) -> crate::requests::Request {
    let mut row = crate::server_leg::base_row(Server::Llm, model, "POST", "", STEP, STEP);
    row.job = STEP.to_string();
    row
}
