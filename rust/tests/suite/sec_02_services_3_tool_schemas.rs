//! §02-services §3, the tool catalogue: each tool is offered WITH its arguments as a JSON schema. A llama.cpp
//! server builds the reply grammar from the offer, and a tool offered by name alone comes back with `{}`
//! (probed on the user's server 2026-10-25: `judge_text` answered `{}` bare and `{"changed":true,…}` with a
//! schema), so every tool the first rewrite offered reached its executor with no arguments.

use naivepost::exchanges::{Message, Mode, Part};
use naivepost::tools::Tool;

#[test]
fn sec_02_services_3_every_tool_is_offered_with_its_arguments() {
    for tool in Tool::all().into_iter().chain([Tool::JudgeText]) {
        let schema = tool.parameters();
        assert_eq!(
            schema["type"],
            "object",
            "{}: an object of arguments",
            tool.name()
        );
        let properties = schema["properties"].as_object().expect("properties");
        for required in schema["required"].as_array().expect("a required list") {
            let name = required.as_str().expect("a name");
            assert!(
                properties.contains_key(name),
                "{}: `{name}` is required and declared",
                tool.name()
            );
        }
    }
    // §3's tables, spot-checked: the arguments the executors read by these names.
    let props = |tool: Tool| -> Vec<String> {
        tool.parameters()["properties"]
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect()
    };
    assert_eq!(props(Tool::RecordEvent), ["calm", "frame", "text"]);
    assert_eq!(props(Tool::SetPolicy), ["because", "field", "value"]);
    assert_eq!(
        props(Tool::WriteLine),
        ["at", "clip", "emotion", "pos", "text"]
    );
    assert_eq!(props(Tool::JudgeText), ["changed", "what"]);

    // And the body carries it.
    let messages = vec![Message {
        role: "user".to_string(),
        parts: vec![Part::Text("hello".to_string())],
    }];
    let body = naivepost::llm_request::body(
        "describe",
        "a-model",
        &messages,
        Mode::Execute,
        Some(&[Tool::RecordEvent, Tool::Finish]),
        true,
    )
    .expect("a body");
    let offered = body["tools"].as_array().expect("tools offered");
    assert_eq!(offered[0]["function"]["name"], "record_event");
    assert_eq!(
        offered[0]["function"]["parameters"]["properties"]["frame"]["type"], "integer",
        "the offer carries the schema: {body}"
    );
}
