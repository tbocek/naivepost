//! §02-services#5 — the request bodies §5 confirms, and sd.cpp's waits.
//!
//! These are shape tests: what a body looks like on the wire is exactly what a server acts on, so
//! each one compares whole `serde_json::Value`s (a string compare would also pin key order, which
//! no server reads) and checks the keys that must NOT be there.

use std::time::Duration;

use serde_json::{json, Value};

use naivepost::bodies::{self, ServerPath};
use naivepost::services::{self, Kind};

/// The path an upload answered with, in every body below.
fn uploaded() -> ServerPath {
    ServerPath::from_upload("/srv/uploads/take.wav").expect("an absolute path is a server path")
}

#[test]
fn sec_02_services_5_details_confirmed_against_the_code_verification_pass_s1_task_run_bodies() {
    // ASR: the words and the language they were spoken in.
    assert_eq!(
        bodies::asr_body("qwen3-asr", &uploaded(), "pl"),
        json!({
            "model": "qwen3-asr",
            "request": { "audio": "/srv/uploads/take.wav", "language": "pl" },
        })
    );

    // Diarization: who spoke when has no language, so the key is not there at all — an empty one
    // would be a server's problem to interpret.
    let diar = bodies::diar_body("sortformer-diar", &uploaded());
    assert_eq!(
        diar,
        json!({ "model": "sortformer-diar", "request": { "audio": "/srv/uploads/take.wav" } })
    );
    let diar_request = diar["request"].as_object().expect("a request object");
    assert!(!diar_request.contains_key("language"), "{diar_request:?}");
    assert_eq!(diar_request.len(), 1, "{diar_request:?}");

    // Alignment: the text is what the words are placed against, so it travels with the audio.
    assert_eq!(
        bodies::align_body("qwen3-aligner", &uploaded(), "one two", "pl"),
        json!({
            "model": "qwen3-aligner",
            "request": {
                "audio": "/srv/uploads/take.wav",
                "text": "one two",
                "language": "pl",
            },
        })
    );

    // The `audio` field is the server's path and never a local one, which is what makes
    // ServerPath's only constructor the whole of the rule: an empty answer and a relative one are
    // not paths, so no body can be built carrying them.
    assert_eq!(ServerPath::from_upload(""), None);
    assert_eq!(ServerPath::from_upload("relative/thing.wav"), None);
    assert_eq!(ServerPath::from_upload("   "), None);
    // Whitespace around a reply is the server's formatting, not part of the path.
    assert_eq!(
        ServerPath::from_upload("/srv/uploads/x.wav\n").map(|p| p.as_str().to_string()),
        Some("/srv/uploads/x.wav".to_string())
    );
}

#[test]
fn sec_02_services_5_details_confirmed_against_the_code_verification_pass_s3_img_gen_body() {
    let empty = bodies::img_gen_body("a red square", None, None, None, 0, &[], None);
    assert_eq!(
        empty,
        json!({ "prompt": "a red square", "seed": 0, "auto_resize_ref_image": true })
    );

    // A seed of 0 and a negative one are seeds like any other, so both are sent: dropping them
    // would make a thumbnail redraw differently from the way it was drawn.
    assert_eq!(empty["seed"], json!(0));
    let negative = bodies::img_gen_body("x", None, None, None, -1, &[], None);
    assert_eq!(negative["seed"], json!(-1));

    // The four optional keys are omitted rather than sent empty — sd.cpp reads "" as an
    // instruction.
    for key in [
        "negative_prompt",
        "width",
        "height",
        "output_format",
        "ref_images",
        "init_image",
    ] {
        assert!(
            !empty.as_object().expect("an object").contains_key(key),
            "{key} must not be sent when there is nothing for it"
        );
    }

    let full = bodies::img_gen_body(
        "a red square",
        Some("blurry"),
        Some(1280),
        Some(720),
        7,
        &[
            "data:image/png;base64,AAA".to_string(),
            "data:image/png;base64,BBB".to_string(),
        ],
        Some("png"),
    );
    assert_eq!(full["negative_prompt"], json!("blurry"));
    assert_eq!(full["width"], json!(1280));
    assert_eq!(full["height"], json!(720));
    assert_eq!(full["output_format"], json!("png"));

    // References, never `init_image`: an edit model is conditioned on them, which is what lets it
    // keep a face and change the background. Checked over the serialised body because that is the
    // only place a stray key could hide.
    let serialized = serde_json::to_string(&full).expect("a body that serialises");
    assert_eq!(
        full["ref_images"],
        json!(["data:image/png;base64,AAA", "data:image/png;base64,BBB"])
    );
    assert!(!serialized.contains("init_image"), "{serialized}");

    // auto_resize_ref_image is always there — including with no references at all, so the key a
    // server reads is the same key on every request.
    assert_eq!(full["auto_resize_ref_image"], json!(true));
    assert!(empty
        .as_object()
        .expect("an object")
        .contains_key("auto_resize_ref_image"));
}

#[test]
fn sec_02_services_5_details_confirmed_against_the_code_verification_pass_s4_sd_waits() {
    let wait = |kind| bodies::sd_waits(kind).unwrap_or_else(|| panic!("{kind:?} has a wait"));
    let seconds = |kind: Kind| wait(kind).timeout.as_secs();

    // Fifteen seconds is the slowest a healthy server takes to describe itself; sixty covers a
    // submit that has to load the model first.
    assert_eq!(seconds(Kind::Capabilities), 15);
    assert_eq!(seconds(Kind::SubmitImage), 60);
    // A poll is asked once a second: the answer is a status line, so faster buys nothing and slower
    // makes a finished thumbnail appear late.
    assert_eq!(seconds(Kind::PollImage), 30);
    assert_eq!(wait(Kind::PollImage).interval, Some(Duration::from_secs(1)));
    // Cancel is best effort — the card goes back either way.
    assert_eq!(seconds(Kind::CancelImage), 10);

    // Only sd.cpp has these waits; the audio and LLM kinds are governed by §1 instead.
    for kind in [
        Kind::Chat,
        Kind::ListModels,
        Kind::Health,
        Kind::Upload,
        Kind::RunTask,
        Kind::Speech,
        Kind::UnloadAll,
    ] {
        assert_eq!(bodies::sd_waits(kind), None, "{kind:?} is not sd.cpp");
    }

    // §1's table unchanged by §5's: no client timeout on the work, twenty seconds for housekeeping.
    // The two tables are kept apart because they answer different questions — "may this run for an
    // hour" versus "how long until sd.cpp has said something".
    let untimed = [
        Kind::Chat,
        Kind::ListModels,
        Kind::Health,
        Kind::Upload,
        Kind::RunTask,
        Kind::Speech,
        Kind::Capabilities,
        Kind::SubmitImage,
        Kind::PollImage,
        Kind::CancelImage,
    ];
    for kind in untimed {
        assert_eq!(services::timeout_for(kind), None, "{kind:?}");
    }
    assert_eq!(
        services::timeout_for(Kind::UnloadAll),
        Some(Duration::from_secs(20))
    );

    // A wait's interval belongs to a poll only.
    let intervals: Vec<Option<Duration>> = [
        Kind::Capabilities,
        Kind::SubmitImage,
        Kind::PollImage,
        Kind::CancelImage,
    ]
    .iter()
    .map(|kind| wait(*kind).interval)
    .collect();
    assert_eq!(
        intervals,
        vec![None, None, Some(Duration::from_secs(1)), None]
    );

    // And the bodies above are what those kinds carry: RunTask and Speech and SubmitImage all have
    // a body builder here, so a kind without one cannot be sent by accident.
    let _: Value = bodies::img_gen_body("x", None, None, None, 1, &[], None);
}

// ---- the settings tests ----

use naivepost::checks::{self, AUDIO_ROWS};
use naivepost::services::AudioModel;

fn model(id: &str, family: &str, task: &str) -> AudioModel {
    AudioModel {
        id: id.to_string(),
        family: family.to_string(),
        task: task.to_string(),
    }
}

/// The LLM and vision rows: the smallest request that proves an id and a key, and a picture a
/// text-only model cannot answer.
#[test]
fn sec_02_services_5_details_confirmed_against_the_code_verification_pass_s5_llm_and_vision_rows() {
    // One completion of sixteen tokens, thinking forced off in every place a server reads it: a
    // thinking model would spend the sixteen on reasoning and answer nothing.
    let body = checks::llm_test_body("some-model");
    assert_eq!(body["max_tokens"], json!(16));
    assert_eq!(checks::LLM_TEST_MAX_TOKENS, 16);
    assert_eq!(body["enable_thinking"], json!(false));
    assert_eq!(
        body["chat_template_kwargs"]["enable_thinking"],
        json!(false)
    );
    assert_eq!(body["model"], json!("some-model"));
    // A text answer is slow only because the server is loading; an image prompt also loads the
    // vision encoder, so it gets twice as long.
    assert_eq!(
        (checks::LLM_TEST_SECONDS, checks::VISION_TEST_SECONDS),
        (60, 120)
    );

    // The probe really is a 48 px solid red square: signature, IHDR read back, and the stored
    // IDAT inflated by hand (a stored-deflate block is length + complement + bytes).
    let png = checks::red_square_png();
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    assert_eq!(&png[12..16], b"IHDR");
    let ihdr = &png[16..29];
    assert_eq!(u32::from_be_bytes(ihdr[0..4].try_into().unwrap()), 48);
    assert_eq!(u32::from_be_bytes(ihdr[4..8].try_into().unwrap()), 48);
    assert_eq!((ihdr[8], ihdr[9]), (8, 2), "8-bit truecolour");

    let idat = png_at(&png, b"IDAT");
    assert_eq!(idat[..2], [0x78, 0x01], "zlib stream");
    let raw = stored_deflate(&idat[2..]);
    assert_eq!(
        raw.len(),
        48 * (1 + 48 * 3),
        "one filter byte plus 48 pixels per row"
    );
    for row in 0..48usize {
        let start = row * 145;
        assert_eq!(raw[start], 0, "row {row} uses no filter");
        assert_eq!(
            &raw[start + 1..start + 7],
            &[0xDC, 0x00, 0x00, 0xDC, 0x00, 0x00]
        );
        assert!(
            raw[start + 1..start + 145]
                .chunks(3)
                .all(|p| p == [0xDC, 0x00, 0x00]),
            "row {row} is not solid red"
        );
    }

    // The URL carries exactly those bytes.
    let url = checks::vision_probe_url();
    assert!(url.starts_with("data:image/png;base64,"), "{}", &url[..40]);
    assert_eq!(from_base64(url.split_once(',').unwrap().1), png);

    // "red" anywhere, in any case — models answer "Red.", "It is red." and "crimson (red)" alike.
    assert!(checks::vision_verdict("Crimson? red.").is_ok());
    let err = checks::vision_verdict("blue").expect_err("a wrong colour must fail");
    assert!(err.contains("blue"), "{err}");
}

/// The four audio.cpp rows and the aligner: one id each, checked against what the server declares.
#[test]
fn sec_02_services_5_details_confirmed_against_the_code_verification_pass_s6_audio_rows() {
    // An empty catalogue is its own verdict — healthy on /health and serving nothing is the usual
    // sign of a server that started without its models.
    let err = checks::tts_endpoint_verdict(&[]).expect_err("nothing served");
    assert!(err.contains("no models"), "{err}");

    let by_family = [model("other", "index_tts2", "tts")];
    assert!(checks::tts_endpoint_verdict(&by_family).is_ok());
    let by_task = [model("clone-thing", "some", "clon")];
    let ok = checks::tts_endpoint_verdict(&by_task).expect("task clon can narrate");
    assert!(ok.contains("clone-thing"), "{ok}");

    let step1 = [model("step-1", "old_family", "tts")];
    let err = checks::tts_endpoint_verdict(&step1).expect_err("step-1 cannot clone");
    assert!(err.contains("step-1"), "it names what is served: {err}");

    // One id per button, and the declared task is what decides.
    let catalogue = [
        model("qwen3-asr", "nemotron", "asr"),
        model("mislabelled", "x", "diar"),
    ];
    let absent = checks::model_verdict(
        &catalogue,
        "nope-asr",
        naivepost::services::Need {
            task: "asr",
            step: "Prepare",
        },
    )
    .expect_err("not served");
    assert!(absent.contains("nope-asr"), "{absent}");
    let wrong = checks::model_verdict(
        &catalogue,
        "mislabelled",
        naivepost::services::Need {
            task: "asr",
            step: "Prepare",
        },
    )
    .expect_err("declared for another task");
    assert!(
        wrong.contains("diar"),
        "it says what the server declares: {wrong}"
    );
    assert!(checks::model_verdict(
        &catalogue,
        "qwen3-asr",
        naivepost::services::Need {
            task: "asr",
            step: "Prepare"
        }
    )
    .is_ok());
    // A blank declared task passes: plenty of servers list ids with no task at all.
    let blank = [model("bs-roformer", "bs", "")];
    assert!(checks::model_verdict(
        &blank,
        "bs-roformer",
        naivepost::services::Need {
            task: "sep",
            step: "Prepare"
        }
    )
    .is_ok());
    // The four buttons this covers, each with the task it checks.
    assert_eq!(AUDIO_ROWS.len(), 4);

    // The aligner is chosen by task: a model whose id says nothing but is declared `align` is used…
    let aligned = [
        model("whatever-1", "f", "align"),
        model("other", "f", "asr"),
    ];
    let ok = checks::aligner_verdict("", &aligned).expect("task align is enough");
    assert!(ok.contains("whatever-1"), "{ok}");
    // …an id in the box that the server does not declare for `align` is a miss…
    let miss = checks::aligner_verdict("other", &aligned).expect_err("not an aligner");
    assert!(miss.contains("other"), "{miss}");
    // …and having no aligner at all is not a failure: the waveform decides the cuts instead.
    let none = checks::aligner_verdict("", &[model("asr-only", "f", "asr")])
        .expect("no aligner is a working setup");
    assert!(none.contains("waveform"), "{none}");
}

/// No Save button: 600 ms after the last keystroke, and again on close if a write is still owed.
#[test]
fn sec_02_services_5_details_confirmed_against_the_code_verification_pass_s8_no_save_button() {
    // P.policy.ttsLanguage lives in the file this writes; there is no Save to press for it.
    let ms = checks::CONF_SAVE_WAIT;
    assert_eq!(ms, Duration::from_millis(600));

    let mut state = checks::SaveState::default();
    assert!(!state.writable(ms * 2), "nothing typed, nothing owed");
    state.touched(Duration::ZERO);
    assert_eq!(state.due_at(), Some(ms));
    assert!(
        !state.writable(ms - Duration::from_millis(1)),
        "the beat has not elapsed"
    );
    assert!(state.writable(ms));

    // A second keystroke pushes the deadline back — one write per pause, not one per letter.
    state.touched(Duration::from_millis(300));
    assert_eq!(state.due_at(), Some(Duration::from_millis(900)));
    assert!(
        !state.writable(ms),
        "the first beat is not the due time any more"
    );
    assert!(state.writable(Duration::from_millis(900)));

    // Closing writes when a write is owed, and never twice.
    let mut closing = checks::SaveState::default();
    closing.touched(Duration::ZERO);
    assert!(closing.close());
    assert!(!closing.close(), "the debt is paid");
    let mut idle = checks::SaveState::default();
    assert!(!idle.close(), "nothing typed, nothing to write");

    // A save invalidates the cached TTS id and the "already listening on" note: both belong to the
    // server being replaced.
    let mut draft = checks::Draft {
        state: checks::SaveState::default(),
        cached_tts_model: "index-tts2".to_string(),
        audio_noted: "127.0.0.1:8765".to_string(),
    };
    assert!(!draft.save(ms - Duration::from_millis(1)), "not yet");
    assert_eq!(
        draft.cached_tts_model, "index-tts2",
        "a keystroke invalidates nothing"
    );
    draft.state.touched(Duration::ZERO);
    assert!(draft.save(ms));
    assert!(draft.cached_tts_model.is_empty());
    assert!(draft.audio_noted.is_empty());
    assert!(!draft.save(ms * 2), "nothing owed after a save");

    // Closing is the other end of the same rule, and clears the same two things.
    let mut draft = checks::Draft {
        state: checks::SaveState::default(),
        cached_tts_model: "index-tts2".to_string(),
        audio_noted: "127.0.0.1:8765".to_string(),
    };
    draft.state.touched(Duration::ZERO);
    assert!(draft.close());
    assert!(draft.cached_tts_model.is_empty() && draft.audio_noted.is_empty());
    assert!(!draft.close());
}

/// The body of the PNG chunk named by `kind`, which this test walks rather than trusting.
fn png_at(png: &[u8], kind: &[u8; 4]) -> Vec<u8> {
    let at = png
        .windows(4)
        .position(|w| w == kind)
        .expect("the chunk is there");
    let len = u32::from_be_bytes(png[at - 4..at].try_into().unwrap()) as usize;
    png[at + 4..at + 4 + len].to_vec()
}

/// The bytes of a stored-deflate stream: block header, length + one's complement, then the bytes.
/// Only valid for the uncompressed blocks `red_square_png` writes, which is all this needs.
fn stored_deflate(blocks: &[u8]) -> Vec<u8> {
    let mut at = 0;
    let mut raw = Vec::new();
    loop {
        let last = blocks[at] & 1 == 1;
        let len = u16::from_le_bytes(blocks[at + 1..at + 3].try_into().unwrap()) as usize;
        let complement = !u16::from_le_bytes(blocks[at + 3..at + 5].try_into().unwrap());
        assert_eq!(
            len as u16, complement,
            "a stored block carries its own length twice"
        );
        raw.extend_from_slice(&blocks[at + 5..at + 5 + len]);
        at += 5 + len;
        if last {
            return raw;
        }
    }
}

/// Standard-alphabet base64, decoded by hand so the test does not agree with the encoder's own
/// mistake.
fn from_base64(text: &str) -> Vec<u8> {
    let alphabet = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = Vec::new();
    let mut buffer = 0u32;
    let mut bits = 0;
    for char in text.bytes().filter(|byte| *byte != b'=') {
        let value = alphabet
            .iter()
            .position(|a| *a == char)
            .expect("a base64 character") as u32;
        buffer = (buffer << 6) | value;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
        }
    }
    out
}
