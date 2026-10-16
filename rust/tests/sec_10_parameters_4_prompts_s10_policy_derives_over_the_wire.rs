//! §10-parameters#4-prompts S10 over a socket: the new `policy` prompt is really sent.
//!
//! The derivation's whole point is that a person's User Context reaches a model. Everything else in
//! `tests/prompts_catalogue.rs` proves the catalogue; this presses the run bar's ⚙ on a real window,
//! with the settings file pointing at a loopback fake, and reads what arrived: the shipped policy
//! wording, thinking off as §4 says, and `set_policy` offered. The session policy changing is the
//! proof the answer was applied rather than only sent.

#![allow(dead_code)]

#[path = "captions_wire_harness.rs"]
mod harness;

use harness::*;
#[allow(dead_code)] // every test binary compiles this whole module; a helper it does not call is not a warning here
mod common;
use adw::prelude::*;
use gtk4 as gtk;
use naivepost::{prompts, ui};
use common::{fixture_dir, settle};
use serde_json::json;

/// A `set_policy` tool call in the shape a real server answers a tool conversation with — the same
/// arguments `tool:set_policy` validates against (§03's catalogue), so the round accepts it.
fn set_policy_call(field: &str, value: &str) -> String {
    json!({
        "choices": [{"message": {"content": null, "tool_calls": [{
            "id": "call_1",
            "type": "function",
            "function": {"name": "set_policy", "arguments": json!({
                "field": field, "value": value, "because": "the context said so"
            }).to_string()}
        }]}}]
    })
    .to_string()
}

fn button(window: &adw::ApplicationWindow, name: &str) -> gtk::Button {
    ui::line_step_button(window, name).unwrap_or_else(|| panic!("the shell carries {name}"))
}

/// Every widget case runs inside ONE application's activate hook: GTK may only have its main loop
/// started by the thread that will run it, and cargo runs `#[test]` functions on a worker.
fn check_the_gear_dials_the_policy_prompt(app: &adw::Application) {
    // The scenario root: pinning `XDG_CONFIG_HOME`, `XDG_DATA_HOME` and the cwd is the whole point of
    // calling it, and the guard keeps the folders alive until the test's end.
    let _fixture = fixture("s10-policy-wire");
    let llm = FakeLlm::start(vec![set_policy_call("cutMode", "words")]);
    let addr = point_at(&llm.url());

    let project = naivepost::project::load(&fixture_dir()).expect("the fixture loads");
    let window = ui::build_window(app, &project, "Prepare");
    window.present();
    settle();

    // A context typed but never derived from: exactly what a person leaves behind, and F0.7 S5's reason
    // for deriving on the way into the form.
    ui::set_session_context(&window, "cut on the words, not the model");
    assert!(
        !ui::policy_is_current(&window, "cut on the words, not the model"),
        "a fresh window has derived nothing"
    );
    let before = ui::session_policy(&window).cut_mode.value;

    button(&window, "policy-button").emit_by_name::<()>("clicked", &[]);
    settle();

    let seen = llm.next_seen();
    assert_eq!(seen.path, "/v1/chat/completions", "the derivation dialled the chat path at {addr}");
    let sent: serde_json::Value = serde_json::from_str(&seen.body).expect("the ask sent JSON");
    // §4: policy is NOT one of the four thinking-on prompts.
    assert_eq!(sent["enable_thinking"], json!(false), "F0.7 S2 asks the policy prompt thinking-off");
    assert_eq!(
        sent["chat_template_kwargs"]["enable_thinking"],
        json!(false),
        "both spellings of the switch must say off"
    );
    let body = sent.to_string();
    let shipped = prompts::shipped_text("policy").expect("the policy prompt ships");
    // A sentence from the shipped wording, so this asserts the file rode out rather than a paraphrase.
    let line: String = shipped
        .lines()
        .map(str::trim)
        .find(|l| l.len() > 24 && !l.starts_with('#') && !l.starts_with('-'))
        .expect("the shipped policy wording has a sentence to look for")
        .to_string();
    assert!(
        body.contains(&line),
        "the shipped policy wording went in the system message: {line:?}"
    );
    assert!(body.contains("set_policy"), "set_policy was offered");
    assert!(body.contains("get_context"), "get_context was offered");
    for tool in naivepost::tools::policy_tools() {
        assert!(
            body.contains(&format!("\"{}\"", tool.name())),
            "{tool:?} is one of the tools the request offers"
        );
    }
    // The context itself travelled as the user turn.
    assert!(
        seen.body.contains("cut on the words, not the model"),
        "the User Context rode out with the prompt"
    );
    // And the answer was APPLIED, not just read: cutMode moved off its default.
    let after = ui::session_policy(&window).cut_mode.value;
    assert_ne!(before, after, "the model's proposal landed in the session policy");
    assert!(
        ui::policy_is_current(&window, "cut on the words, not the model"),
        "the tracker recorded the derivation, so the next press costs no wire"
    );
}

/// S11: the per-machine wording wins over the shipped file on the same live dial, and Reset still has
/// something to go back to. The edit is written where §04 says a prompt edit lives —
/// `<config>/naivepost/prompts/policy.txt` — and read back through `settings::prompt_text`, the door
/// `policy_call::wording` uses, so this proves the live path reads the override rather than the tree.
fn check_an_edited_policy_prompt_wins_on_the_wire(app: &adw::Application) {
    let marker = "EDITED-ON-THIS-MACHINE: keep every cut on a word edge.";
    let paths = naivepost::settings::from_environment().expect("the pinned settings folder resolves");
    let shipped = prompts::shipped_text("policy").expect("the policy prompt ships");
    std::fs::create_dir_all(paths.prompts_dir()).expect("the prompts folder");
    std::fs::write(paths.prompt_file("policy"), format!("{marker}\n{shipped}")).expect("edit written");

    // A second scenario root would re-point the process-wide env under S10's window, so this case
    // reuses the one pinned above and only moves two things: the address (still a loopback fake) and
    // the User Context, which is what §6 keys the cache on — a different context cannot be answered
    // from S10's cached round.
    let llm = FakeLlm::start(vec![set_policy_call("captionsPass", "true")]);
    point_at(&llm.url());
    let project = naivepost::project::load(&fixture_dir()).expect("the fixture loads");
    let window = ui::build_window(app, &project, "Prepare");
    window.present();
    settle();
    // A different context from S10's so the LLM cache (keyed on the exact request) cannot answer here.
    ui::set_session_context(&window, "burn in captions for the whole film");
    button(&window, "policy-button").emit_by_name::<()>("clicked", &[]);
    settle();

    let seen = llm.next_seen();
    assert!(
        seen.body.contains(marker),
        "this machine's edited policy wording is what went out"
    );
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&seen.body).expect("JSON")["enable_thinking"],
        json!(false),
        "an edited wording does not change §4's thinking flag"
    );
    // Reset's target survives the edit: the shipped file is still what the catalogue hands back.
    assert_eq!(
        prompts::shipped_text("policy").as_deref(),
        Some(shipped.as_str()),
        "Reset still has the built-in wording to put back"
    );
    // S1's own default is `true`, so "landed" here is proven by the ORIGIN the proposal wrote, not by a
    // value flip: a model-sourced field is what a derivation looks like in the state.
    let after = ui::session_policy(&window).captions_pass.clone();
    assert_eq!(
        after.origin,
        naivepost::project::Origin::Model,
        "the model's proposal landed on this session (value {}), so the edited prompt was the one answered",
        after.value
    );
}

static RAN_S10: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static RAN_S11: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn window_round() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(|app| {
            check_the_gear_dials_the_policy_prompt(app);
            RAN_S10.store(true, std::sync::atomic::Ordering::SeqCst);
            check_an_edited_policy_prompt_wins_on_the_wire(app);
            RAN_S11.store(true, std::sync::atomic::Ordering::SeqCst);
            app.quit();
        });
        app.run_with_args::<String>(&[]);
    });
}

#[test]
fn sec_10_parameters_4_prompts_s10_pressing_the_gear_sends_the_policy_prompt_over_the_wire() {
    window_round();
    assert!(RAN_S10.load(std::sync::atomic::Ordering::SeqCst), "the wire check never ran");
}

#[test]
fn sec_10_parameters_4_prompts_s11_an_edited_policy_prompt_is_the_one_sent() {
    window_round();
    assert!(RAN_S11.load(std::sync::atomic::Ordering::SeqCst), "the edited-wording check never ran");
}
