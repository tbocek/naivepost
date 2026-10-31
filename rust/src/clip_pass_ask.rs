//! The model round the speed and decoration passes share: one offered tool, a cached answer, and one repair.

use crate::exchanges::{Message, Part};
use crate::layout::Tree;
use crate::llm_cache;
use crate::llm_request;
use crate::requests::{Request, Service};
use crate::services;
use crate::settings::Conf;
use crate::tools::clips::Clips;
use crate::tools::Tool;

/// What one pass asked and how it reads the answer.
pub struct Pass<'a, C> {
    pub step: &'a str,
    pub tool: Tool,
    pub tools: Vec<Tool>,
    pub model: &'a str,
    pub system: &'a str,
    pub user: &'a str,
    pub attempt: u32,
    pub batch: Clips,
    pub parse: &'a dyn Fn(&str) -> Vec<C>,
    pub call_of_args: &'a dyn Fn(&serde_json::Value) -> Option<C>,
}

/// What came back: the calls, or the reason there are none.
#[derive(Debug, Clone, PartialEq)]
pub struct Asked<C> {
    pub calls: Vec<C>,
    pub failed: bool,
    pub reason: String,
}

impl<C> Asked<C> {
    pub fn answered(calls: Vec<C>) -> Self {
        Self {
            calls,
            failed: false,
            reason: String::new(),
        }
    }

    pub fn failed(reason: String) -> Self {
        Self {
            calls: Vec::new(),
            failed: true,
            reason,
        }
    }
}

pub fn run<C>(tree: &Tree, conf: &Conf, cancel: &dyn Fn() -> bool, pass: &Pass<C>) -> Asked<C> {
    let (model, system, user) = (pass.model, pass.system, pass.user);
    // A retry asks with the same two messages, so it needs its own cache slot: without one the second round
    // would be served the reply that was just rejected (§6).
    let parts = cache_parts(system, user, model, pass.attempt);
    if let Some(cached) = llm_cache::read(tree, pass.step, &parts) {
        // §09 §10: the cached answer writes its own `cache` line, with no time on the wire; the row is
        // the record that this ask cost nothing, not a second count of the request that earned it.
        crate::request_timing::record_cached(tree, base_row(pass.step, model));
        return Asked::answered((pass.parse)(&cached));
    }
    let base = base_row(pass.step, model);
    let endpoint = services::llm_endpoint(conf);
    let offered = pass.tools.clone();
    // How to build the body for one offer; `None` means no `tools` key, which is what §2 S5 resends to a server
    // that refused the field. The cache slot and the timed row belong to whichever body goes out first.
    let build = |offer: Option<&[Tool]>| {
        Ok(llm_request::body(
            pass.step,
            model,
            &two_messages(system, user),
            // §4's flag for this prompt, read from the one table (§10 §4: thinking off outside
            // textedit/cut/narrate/youtube).
            crate::prompts::mode_of(pass.step),
            offer,
            true,
        )?
        .to_string())
    };
    // One live ask is ONE request on the wire. This dial IS §2's first round and honours S5 itself: a refusal of
    // the `tools` FIELD gets the identical body again without it, said once, remembered so no later ask re-offers.
    let sent = match crate::llm_tool_refusal_leg::dial_honouring_refusal(
        pass.step,
        &endpoint.url,
        Some(&offered),
        &build,
        &|body: &str| {
            crate::llm_retry_leg::chat(
                pass.step,
                tree,
                &base,
                &endpoint,
                body,
                cancel,
                &crate::ui::window::log_line,
            )
        },
        &crate::ui::window::log_line,
    ) {
        Ok(sent) => sent,
        // The ask itself failed. §F3.11's two rounds count rejections of ANSWERS, so a wire failure spends none of
        // them: a server that is not there answers the same way however often it is called.
        Err(reason) => return Asked::failed(reason),
    };
    let reply = match llm_request::reply_from(&sent.content_type, &sent.body) {
        Ok(reply) => reply,
        Err(_) => {
            // Neither JSON nor a stream: nothing to place, read as "the model proposed nothing" — the same ending
            // an empty EFFECTS document has (§F3.11 does not fail a cut over an unreadable decoration list).
            return Asked::answered(Vec::new());
        }
    };
    if reply.tool_calls.is_empty() {
        if crate::llm_tools_offer::tools_for(&endpoint.url, &offered).is_none() {
            let text = reply.answer().to_string();
            if !text.trim().is_empty() {
                llm_cache::store(tree, pass.step, &parts, &text);
            }
            return Asked::answered((pass.parse)(&text));
        }
        // §3's answered question on this item: the tools ARE on the wire, and a reply carrying a JSON document
        // instead of calling `add_effect` has not answered. Ask again, once, with the correction in the
        // conversation (§3's repair); a second prose answer is this pass's failure, said by name.
        let went_out = std::time::Instant::now();
        let dial = |body: &str| {
            crate::llm_retry_leg::chat(
                pass.step,
                tree,
                &base,
                &endpoint,
                body,
                cancel,
                &crate::ui::window::log_line,
            )
            .map(|sent| (sent.content_type, sent.body))
        };
        let second = match crate::clip_rules_tools_leg::ask_again_for_calls(
            pass.step,
            pass.tool.name(),
            two_messages(system, user),
            &reply,
            &crate::ui::window::log_line,
            &|conversation| {
                Ok(llm_request::body(
                    pass.step,
                    model,
                    conversation,
                    crate::prompts::mode_of(pass.step),
                    Some(&offered),
                    true,
                )?
                .to_string())
            },
            &dial,
        ) {
            Ok(second) => second,
            Err(reason) => return Asked::failed(reason),
        };
        // That retry was a request, and §09 §10 wants every request on the record; the repair leg owns the dial,
        // so the row is written here (`chat` files rows only for what IT dials -- one line per request).
        crate::clip_rules_tools_leg::record_repair_row(tree, base_row(pass.step, model), went_out);
        let calls: Vec<C> = second
            .tool_calls
            .iter()
            .filter(|call| call.name == pass.tool.name())
            .filter_map(|call| {
                let args = serde_json::from_str::<serde_json::Value>(&call.args).ok()?;
                (pass.call_of_args)(&args)
            })
            .collect();
        // §6: the ANSWERING round's envelope is what is kept, so an unchanged batch costs no socket.
        if crate::clip_rules_tools_leg::asks_for_anything(&second) {
            llm_cache::store(
                tree,
                pass.step,
                &parts,
                &crate::clip_rules_tools_leg::answering_envelope(&second).to_string(),
            );
        }
        return Asked::answered(calls);
    }
    // §2's loop from here: each call is RUN against the batch, so a refusal comes back as the tool's own sentence and is SAID.
    let mut batch = pass.batch.clone();
    // The frames a model may aim at are only this far apart: §3.7's `get_frames` walks the batch's
    // grid, and a default 1 s spread would name pictures this session never extracted.
    for line in crate::clip_rules_tools_leg::answer_calls(&reply.tool_calls, &mut batch) {
        crate::ui::window::log_line(&line);
    }
    let calls_out: Vec<C> = reply
        .tool_calls
        .iter()
        .filter(|call| call.name == pass.tool.name())
        .filter_map(|call| {
            let args = serde_json::from_str::<serde_json::Value>(&call.args).ok()?;
            (pass.call_of_args)(&args)
        })
        .collect();
    // §6: the ANSWERING round's envelope is what is kept, so an unchanged batch costs no socket when the cut is
    // asked again. The repair path above stores this same shape; a first reply that already called the tool must
    // too, or the cache is only half a cache and the next identical ask dials the model a second time.
    if crate::clip_rules_tools_leg::asks_for_anything(&reply) {
        llm_cache::store(
            tree,
            pass.step,
            &parts,
            &crate::clip_rules_tools_leg::answering_envelope(&reply).to_string(),
        );
    }
    // A call this pass can already answer ENDS the ask: the model has its result, so no second request goes out
    // (§F3.11 counts batches by rows in `requests.tsv`).
    Asked::answered(calls_out)
}

pub fn two_messages(system: &str, user: &str) -> Vec<Message> {
    vec![
        Message {
            role: "system".to_string(),
            parts: vec![Part::Text(system.to_string())],
        },
        Message {
            role: "user".to_string(),
            parts: vec![Part::Text(user.to_string())],
        },
    ]
}

/// The cache key's parts. A retry has its own slot: it would otherwise be served the reply just rejected (§6).
pub fn cache_parts(system: &str, user: &str, model: &str, attempt: u32) -> Vec<serde_json::Value> {
    let state = if attempt <= 1 {
        system.to_string()
    } else {
        format!("{system}\u{21bb}{attempt}")
    };
    llm_cache::parts(&llm_cache::Request {
        system,
        user,
        state: &state,
        speech: "",
        context: "",
        images: Vec::new(),
        run_index: None,
        model,
        thinking: false,
    })
}

fn base_row(step: &str, model: &str) -> Request {
    Request {
        started: String::new(),
        run: String::new(),
        step: "cut".to_string(),
        job: step.to_string(),
        service: Service::Llm,
        model: model.to_string(),
        kind: "chat".to_string(),
        ..Default::default()
    }
}
