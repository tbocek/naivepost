//! **F0.13 Tests** (spec/03-shell.md §5) — what the Settings dialog's Test buttons and "Test All"
//! decide: a press reads what is TYPED, spins, then marks ✓/✗ with the verdict as its tooltip and
//! mirrors the line into the main log as "settings: …", and each of the six kinds of test (LLM, LLM
//! vision, ffmpeg, firefox, audio.cpp, sd.cpp) has its own pass rule.
//!
//! Nothing here talks to a server or runs a binary — [`naivepost::checks`] is handed what one
//! answered and returns the verdict, which is what lets every rule be checked in a container with no
//! servers on it.
//!
//! This item cites no `P.*` parameter and no `tool:*`.

use naivepost::checks::{self, AUDIO_ROWS};
use naivepost::services::{self, AudioModel};

fn model(id: &str, family: &str, task: &str) -> AudioModel {
    AudioModel { id: id.to_string(), family: family.to_string(), task: task.to_string() }
}

/// S1: the press reads the box, not the file — §5's "reads what is TYPED" — and spins before it has
/// an answer to show.
#[test]
fn f0_13_s1_a_test_reads_what_is_typed_and_spins_before_it_answers() {
    let mut row = checks::TestRow::new("LLM");
    row.finish(Ok("the old address was fine".to_string()));

    row.press("http://box:8080/v1");
    assert_eq!(row.typed, "http://box:8080/v1", "S1: the value in the box at the press is the value tested");
    assert!(row.spinning(), "S1: the spinner shows while it waits");
    assert_eq!(row.mark(), None, "S1: an old ✓ cannot survive a new address");
    assert_eq!(row.tooltip(), None);
    assert_eq!(row.log(), None, "S1: nothing is mirrored until there is a verdict");

    row.finish(Ok("qwen answered in 1.5 s: \"ok\"".to_string()));
    assert!(!row.spinning());
    assert_eq!(row.mark(), Some('✓'));
    assert_eq!(row.tooltip().as_deref(), Some("qwen answered in 1.5 s: \"ok\""));
    assert!(row.log().unwrap().starts_with(checks::LOG_PREFIX));
}

/// S1's other half: a failure is marked and its reason is both the tooltip and the log line, since ✗
/// alone never says which of the reasons it is.
#[test]
fn f0_13_s2_a_failure_is_marked_and_its_verdict_is_the_tooltip() {
    let mut row = checks::TestRow::new("ffmpeg");
    row.press("");
    row.finish(Err("not on PATH".to_string()));

    assert_eq!(row.mark(), Some('✗'));
    assert_eq!(row.tooltip().as_deref(), Some("not on PATH"));
    assert_eq!(row.log().as_deref(), Some("settings: ffmpeg -- not on PATH"));

    assert_eq!(checks::log_line("LLM", &Ok("ok".to_string())), "settings: LLM: ok");
    assert_eq!(checks::log_line("LLM", &Err("refused".to_string())), "settings: LLM -- refused");
}

/// S2: the LLM test is one completion of sixteen tokens with thinking off, and its verdict is the
/// answer plus the wait.
#[test]
fn f0_13_s3_the_llm_test_asks_for_one_completion_and_says_so() {
    assert_eq!(checks::LLM_TEST_MAX_TOKENS, 16);
    assert_eq!(checks::LLM_TEST_SECONDS, 60);

    let body = checks::llm_test_body("qwen");
    assert_eq!(body["max_tokens"], 16);
    assert_eq!(body["enable_thinking"], false);
    assert_eq!(body["chat_template_kwargs"]["enable_thinking"], false);
    assert!(body.get("stream").is_none(), "S2: nothing to watch arrive");
    assert_eq!(body["messages"][0]["content"], "Reply with the single word: ok");

    assert_eq!(checks::llm_verdict("qwen", 1.5, "ok"), "qwen answered in 1.5 s: \"ok\"");
}

/// S2: the vision test shows a generated 48×48 red square and passes only if the model saw red — a
/// text-only model passes the text test and fails minutes into a run.
#[test]
fn f0_13_s4_the_vision_test_shows_a_red_square_and_only_passes_on_red() {
    assert_eq!(checks::VISION_TEST_SECONDS, 120);

    let png = checks::red_square_png();
    // The size is in the IHDR body: bytes 16..20 of a PNG are the width, big-endian.
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n", "S2: the probe is a real PNG, not a header");
    assert_eq!(&png[16..20], &48u32.to_be_bytes(), "S2: 48 px across");
    assert_eq!(&png[20..24], &48u32.to_be_bytes(), "S2: 48 px down");
    assert!(checks::vision_probe_url().starts_with("data:image/png;base64,"));

    assert!(checks::vision_verdict("Red.").is_ok());
    assert!(checks::vision_verdict("It is red").is_ok());
    let reason = checks::vision_verdict("blue").unwrap_err();
    // §5's sentence: the model needs a vision model served with its mmproj/vision file.
    assert!(reason.contains("not seeing the image") && reason.contains("mmproj"), "{reason}");
}

/// S2: "Fetch models" is `GET /v1/models` → dropdown, and "Use" copies the id into Model — so the
/// only question a list answers is whether the configured id is on the server.
#[test]
fn f0_13_s5_fetch_models_lists_the_server_and_use_adopts_the_id() {
    assert_eq!(checks::LIST_MODELS_LLM_USES, ["Fetch models", "sd.cpp fallback probe"]);

    let served = vec!["llama-3".to_string(), "qwen".to_string()];
    assert!(checks::llm_model_listed(&served, "qwen").is_ok());

    let reason = checks::llm_model_listed(&served, "gpt").unwrap_err();
    assert!(reason.contains("llama-3") && reason.contains("qwen"), "S2: the failure names what IS served: {reason}");

    // No default model id exists to fall back to, so an empty box is its own answer.
    assert!(checks::llm_model_listed(&served, "").unwrap_err().contains("Fetch models"));
}

/// The ffmpeg listing §5's scan reads: one component per line, name in the second field.
fn listing(with_rubberband: bool) -> String {
    let mut lines = "Filters:\n".to_string();
    for (name, present) in [
        ("atempo", true),
        ("amix", true),
        ("adelay", true),
        ("alimiter", true),
        ("loudnorm", true),
        ("subtitles", true),
        ("rubberband", with_rubberband),
    ] {
        if present {
            lines.push_str(&format!(" T.C {name} audio\n"));
        }
    }
    lines
}

/// S2: ffmpeg is checked as one build that has what the steps ask for — the seven filters and four
/// encoders §5 names, with a missing binary reported before a missing component.
#[test]
fn f0_13_s6_ffmpeg_is_checked_as_one_build_that_has_what_the_steps_ask() {
    assert_eq!(
        checks::FF_FILTERS,
        ["rubberband", "subtitles", "loudnorm", "atempo", "amix", "adelay", "alimiter"]
    );
    assert_eq!(checks::FF_ENCODERS, ["libx264", "libx265", "aac", "libopus"]);

    let encoders = "Encoders:\n V....D libx264\n V....D libx265\n A....D aac\n A....D libopus\n";
    let full = checks::ffmpeg_verdict(
        Some("/usr/bin/ffmpeg"),
        true,
        "ffmpeg version 7.1",
        Some(&listing(true)),
        Some(encoders),
    )
    .expect("a complete build passes");
    assert!(full.contains("7.1") && full.contains("/usr/bin/ffmpeg"), "{full}");

    // A build missing two components names exactly those two and nothing else.
    let lacking = checks::ffmpeg_verdict(
        Some("/usr/bin/ffmpeg"),
        true,
        "ffmpeg version 7.1",
        Some(&listing(false)),
        Some("Encoders:\n V....D libx264\n A....D aac\n A....D libopus\n"),
    )
    .unwrap_err();
    assert!(lacking.contains("rubberband") && lacking.contains("libx265"), "{lacking}");
    assert!(!lacking.contains("libx264") && !lacking.contains("atempo"), "{lacking}");

    // The diagnosis order: a missing binary and a missing ffprobe are both fixed outside this app, so
    // neither is reported as a build option.
    let absent = checks::ffmpeg_verdict(None, false, "", None, None).unwrap_err();
    assert!(absent.contains("PATH"), "{absent}");
    let no_probe = checks::ffmpeg_verdict(Some("/opt/ffmpeg"), false, "7.1", None, None).unwrap_err();
    assert!(no_probe.contains("ffprobe") && !no_probe.contains("rubberband"), "{no_probe}");

    // A listing that was never read counts as nothing having been checked.
    assert_eq!(checks::ffmpeg_missing("", &["rubberband"]), vec!["rubberband"]);
}

/// S2: firefox "off" is a success — it is the box saying the model gets no search — while a live
/// firefox has to be driven, not merely asked its version.
#[test]
fn f0_13_s7_firefox_off_is_a_pass_and_a_live_firefox_has_to_be_driven() {
    let off = checks::firefox_verdict("off", None, None).expect("off is a choice, not a failure");
    assert!(off.contains("no web search"), "{off}");
    // The box is read trimmed and case-insensitively: "Off " typed by hand means the same thing.
    assert!(checks::firefox_verdict("Off  ", None, None).is_ok());

    let driven = checks::firefox_verdict("/usr/bin/firefox", Some("Firefox 141.0\n"), Some(3))
        .expect("version plus a real search");
    assert!(driven.contains("Firefox 141.0") && driven.contains('3'), "{driven}");

    let stalled = checks::firefox_verdict("/usr/bin/firefox", Some("Firefox 141.0\n"), None).unwrap_err();
    assert!(stalled.contains("could not be driven headless"), "{stalled}");
    assert!(checks::firefox_verdict("/usr/bin/firefox", None, Some(1)).is_err());
}

/// S2: audio.cpp answers with its health and its catalogue, each model box checks its id against its
/// task, and the aligner row passes when there is no aligner at all.
#[test]
fn f0_13_s8_audio_answers_with_its_health_its_catalogue_and_its_aligner() {
    let healthy = checks::audio_health_verdict(12, &[model("indextts2", "index_tts2", "clon")])
        .expect("healthy and able to narrate");
    assert!(healthy.starts_with("healthy in 12 ms, will narrate with"), "{healthy}");
    assert!(healthy.contains("indextts2"), "{healthy}");
    assert_eq!(
        checks::audio_health_verdict(12, &[]).unwrap_err(),
        "healthy, but serving no models"
    );

    // The four model boxes, one task each — the labels are what the dialog lists.
    assert_eq!(
        AUDIO_ROWS,
        [("TTS model", "clon"), ("ASR", "asr"), ("Diarization", "diar"), ("Separation", "sep")]
    );
    let catalogue = [model("whisper", "whisper", "asr"), model("punc", "", "diar")];
    assert!(checks::model_verdict(&catalogue, "whisper", naivepost::services::Need { task: "asr", step: "Prepare" }).is_ok());
    // A wrong task is the interesting failure: a catalogue entry copied from another model.
    let wrong = checks::model_verdict(&catalogue, "whisper", naivepost::services::Need { task: "clon", step: "Prepare" }).unwrap_err();
    assert!(wrong.contains("asr") && wrong.contains("clon"), "{wrong}");
    // A blank declared task passes: many servers list an id with no task at all.
    assert!(checks::model_verdict(&catalogue, "punc", naivepost::services::Need { task: "diar", step: "Prepare" }).is_ok());

    // No aligner is a working setup, not a failure: the joins fall back to the waveform.
    let none = checks::aligner_verdict("", &[]).expect("none is a success");
    assert!(none.contains("waveform") && none.contains("breath"), "{none}");

    // A configured id the server does not serve fails, naming what it does list.
    let missing = checks::aligner_verdict("qwen3-aligner", &[model("whisper", "whisper", "asr")])
        .unwrap_err();
    assert!(missing.contains("whisper"), "{missing}");

    // Several aligners: the preferred one is named, and it is the same choice the run makes — a row
    // that went green while the pipeline used something else would be worse than no row.
    let several = [model("other-align", "x", "align"), model(services::ALIGN_PREFERENCE, "y", "align")];
    let chosen = checks::aligner_verdict("", &several).expect("one of them aligns");
    assert!(chosen.contains(services::ALIGN_PREFERENCE), "{chosen}");
    assert_eq!(services::pick_aligner("", &several).as_deref(), Some(services::ALIGN_PREFERENCE));
}

/// S2: sd.cpp's capabilities say which weights are loaded, and a port that only answers `/v1/models`
/// is called out as not sd-server.
#[test]
fn f0_13_s9_sd_cpp_is_called_out_when_the_port_is_only_openai_shaped() {
    let caps = checks::SdCaps { weights: "flux".to_string() };
    assert_eq!(checks::sd_verdict(Ok(&caps)).as_deref(), Ok("flux is loaded and can draw"));

    let blank = checks::SdCaps { weights: "   ".into() };
    assert!(checks::sd_verdict(Ok(&blank)).unwrap_err().contains("no weights"));

    // The two failures are fixed differently, so they are not one message.
    let openai = checks::sd_verdict(Err(checks::SdProbe::OpenAiOnly)).unwrap_err();
    assert!(openai.contains("not sd-server"), "{openai}");
    let dark = checks::sd_verdict(Err(checks::SdProbe::Unreachable)).unwrap_err();
    assert!(dark.contains("/sdcpp/v1/capabilities"), "{dark}");
}

/// "Test All": every row's own verdict, in the order the rows are on screen — nothing short-circuits,
/// or one early failure hides the rest and sends the user through the dialog ten times.
#[test]
fn f0_13_s10_test_all_runs_every_row_and_reports_each() {
    let rows = checks::test_all(&[
        ("LLM", Ok("qwen answered in 1.5 s: \"ok\"".to_string())),
        ("ffmpeg", Err("not on PATH".to_string())),
        ("sd.cpp", Ok("flux is loaded and can draw".to_string())),
    ]);

    assert_eq!(
        rows.iter().map(|row| (row.name.as_str(), row.ok)).collect::<Vec<_>>(),
        vec![("LLM", true), ("ffmpeg", false), ("sd.cpp", true)]
    );
    // The failing row's reason survives: the ✓/✗ is the summary, the verdict is the answer.
    assert_eq!(rows[1].why, "not on PATH");
}
