//! §10-parameters#4-prompts S10 over a socket: the new `policy` prompt is really sent.
//!
//! The derivation's whole point is that a person's User Context reaches a model. Everything else in
//! `tests/suite/prompts_catalogue.rs` proves the catalogue; this presses the run bar's ▶ on a real window
//! (F0.7's door: a press derives a context nobody derived from), with the settings file pointing at a
//! loopback fake, and reads what arrived: the shipped policy
//! wording, thinking off as §4 says, and `set_policy` offered. The session policy changing is the
//! proof the answer was applied rather than only sent.

#![allow(dead_code)]

#[path = "captions_wire_harness.rs"]
mod harness;

use crate::common;
use adw::prelude::*;
use common::{fixture_dir, settle};
use harness::*;
use naivepost::{prompts, ui};
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

/// Point this process's audio.cpp address at a loopback fake, IN THE ALREADY PINNED settings root: ▶ needs
/// a live server or it refuses at F1.1's preflight before the policy door runs. Only `audio_server` moves —
/// `XDG_CONFIG_HOME` stays where `fixture(...)` put it, so `point_at` and the S11 prompt file still resolve.
fn serve_audio() {
    let paths =
        naivepost::settings::from_environment().expect("the pinned settings folder resolves");
    let mut conf = naivepost::settings::read(&paths).expect("settings read back");
    conf.audio_server = common::fake_audio_server();
    naivepost::settings::save(&paths, &conf).expect("the conf points at the fake server");
}

/// F0.7's "on the next ▶" door through the widget: the press queues the derivation ahead of the step body,
/// which lands on the main loop, so the wait is for the fact (this context is now current) rather than for a
/// guess of how many turns it takes. Then \u23f9 leaves the bar idle for the next check.
fn press_play_and_wait(window: &adw::ApplicationWindow, context: &str) {
    ui::play_button(window)
        .expect("the run bar has a \u{25b6}")
        .emit_by_name::<()>("clicked", &[]);
    common::wait_until("the policy ask landed", || {
        ui::policy_is_current(window, context)
    });
    ui::stop_button(window)
        .expect("the run bar has a \u{23f9}")
        .emit_by_name::<()>("clicked", &[]);
    settle();
}

/// Every widget case runs inside ONE application's activate hook: GTK may only have its main loop
/// started by the thread that will run it, and cargo runs `#[test]` functions on a worker.
fn check_the_play_press_dials_the_policy_prompt(app: &adw::Application) {
    // The scenario root: pinning `XDG_CONFIG_HOME`, `XDG_DATA_HOME` and the cwd is the whole point of
    // calling it, and the guard keeps the folders alive until the test's end.
    let _fixture = fixture("s10-policy-wire");
    let llm = FakeLlm::start(vec![set_policy_call("cutMode", "words")]);
    let addr = point_at(&llm.url());
    serve_audio();

    let project = naivepost::project::load(&fixture_dir()).expect("the fixture loads");
    let window = ui::build_window(app, &project, "Prepare");
    window.present();
    settle();

    // A context typed but never derived from: exactly what a person leaves behind, and F0.7's reason
    // for deriving on the next ▶.
    ui::set_session_context(&window, "cut on the words, not the model");
    assert!(
        !ui::policy_is_current(&window, "cut on the words, not the model"),
        "a fresh window has derived nothing"
    );
    let before = ui::session_policy(&window).cut_mode.value;

    press_play_and_wait(&window, "cut on the words, not the model");

    let seen = llm.next_seen();
    assert_eq!(
        seen.path, "/v1/chat/completions",
        "the derivation dialled the chat path at {addr}"
    );
    let sent: serde_json::Value = serde_json::from_str(&seen.body).expect("the ask sent JSON");
    // §4: policy is NOT one of the four thinking-on prompts.
    assert_eq!(
        sent["enable_thinking"],
        json!(false),
        "F0.7 S2 asks the policy prompt thinking-off"
    );
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
    assert_ne!(
        before, after,
        "the model's proposal landed in the session policy"
    );
    assert!(
        ui::policy_is_current(&window, "cut on the words, not the model"),
        "the tracker recorded the derivation, so the next press costs no wire"
    );
}

/// S11: the open project's profile section for `policy` (§03-shell §5b) rides on the same live dial, after the
/// shipped wording, read through `settings::prompt_text`, the door `policy_call::wording` uses.
fn check_an_edited_policy_prompt_wins_on_the_wire(app: &adw::Application) {
    let marker = "FROM-THE-PROFILE: keep every cut on a word edge.";
    let paths =
        naivepost::settings::from_environment().expect("the pinned settings folder resolves");
    naivepost::profile::save(
        &paths,
        "Policy probe",
        &format!("A lecture.\n## policy\n{marker}\n"),
    )
    .expect("the profile is on this machine");

    // A second scenario root would re-point the process-wide env under S10's window, so this case
    // reuses the one pinned above and only moves two things: the address (still a loopback fake) and
    // the User Context, which is what §6 keys the cache on — a different context cannot be answered
    // from S10's cached round.
    let llm = FakeLlm::start(vec![set_policy_call("captionsPass", "true")]);
    point_at(&llm.url());
    serve_audio();
    let project = naivepost::project::load(&fixture_dir()).expect("the fixture loads");
    let window = ui::build_window(app, &project, "Prepare");
    window.present();
    settle();
    // The open project uses the profile, on disk and in the window's session alike.
    let dir = naivepost::startup::open_dir();
    let mut on_disk = naivepost::project::load(&dir).unwrap_or_default();
    on_disk.profile = Some(naivepost::project::ProfileUse {
        name: "Policy probe".into(),
        values: Default::default(),
        ..Default::default()
    });
    naivepost::project::save(&on_disk, &dir).expect("the project names its profile");
    ui::session_handle(&window).borrow_mut().profile = on_disk.profile.clone();
    // A different context from S10's so the LLM cache (keyed on the exact request) cannot answer here.
    ui::set_session_context(&window, "burn in captions for the whole film");
    press_play_and_wait(&window, "burn in captions for the whole film");

    let seen = llm.next_seen();
    assert!(
        seen.body.contains(marker),
        "the profile's policy section went out with the request"
    );
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&seen.body).expect("JSON")["enable_thinking"],
        json!(false),
        "an edited wording does not change §4's thinking flag"
    );
    // The section IS the step's prompt for this project: the shipped wording is not sent beside it.
    let shipped = prompts::shipped_text("policy").expect("the policy prompt ships");
    let first_line = shipped.lines().next().expect("a first line");
    assert!(
        !seen.body.contains(&first_line.replace('"', "\\\"")),
        "the profile's section replaced the shipped policy wording"
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
    common::one_app_round(|app| {
        check_the_play_press_dials_the_policy_prompt(app);
        RAN_S10.store(true, std::sync::atomic::Ordering::SeqCst);
        check_an_edited_policy_prompt_wins_on_the_wire(app);
        RAN_S11.store(true, std::sync::atomic::Ordering::SeqCst);
    });
}

#[test]
fn sec_10_parameters_4_prompts_s10_pressing_play_sends_the_policy_prompt_over_the_wire() {
    window_round();
    assert!(
        RAN_S10.load(std::sync::atomic::Ordering::SeqCst),
        "the wire check never ran"
    );
}

#[test]
fn sec_10_parameters_4_prompts_s11_an_edited_policy_prompt_is_the_one_sent() {
    window_round();
    assert!(
        RAN_S11.load(std::sync::atomic::Ordering::SeqCst),
        "the edited-wording check never ran"
    );
}
