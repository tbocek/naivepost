// F4.4 Speak a line (TTS) — spec/07-narrate.md F4.4, steps S1–S6 and every branch of its flowchart.
// One line in, one wav out: the reference has to be on disk, the server healthy and able to clone, the
// reference uploaded again for this line, one POST whose options carry the emotion as eight floats or as
// words for the judge, and a reply that is only a take if it is a 200 of at least 1000 bytes. Every rule is
// a plain function in naivepost::narrate_tts, so nothing here opens a socket or needs a display — the two
// tests that touch a folder use a throwaway one.

use std::path::{Path, PathBuf};

use naivepost::bodies::{self, ServerPath};
use naivepost::checks;
use naivepost::layout::Tree;
use naivepost::narrate_screen::EMOTIONS;
use naivepost::narrate_tts as tts;
use naivepost::narration::{self, Entry};
use naivepost::services::AudioModel;

fn model(id: &str, family: &str, task: &str) -> AudioModel {
    AudioModel { id: id.into(), family: family.into(), task: task.into() }
}

/// The path an upload handed back for this line — the only spelling a request may carry.
fn uploaded() -> ServerPath {
    ServerPath::from_upload("/srv/uploads/voice_ref.wav\n").expect("a server path starts at the root")
}

#[allow(dead_code)]
fn entry(text: &str, emotion: &str) -> Entry {
    Entry { s: 0.0, e: 10.0, text: text.into(), emotion: emotion.into(), ..Default::default() }
}

/// A project folder inside a throwaway root, with `narrate/` present or absent as the test needs.
fn temp_tree(tag: &str) -> (PathBuf, Tree) {
    let root = std::env::temp_dir().join(format!("naivepost-tts-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let dir = root.join("demo.naivepost");
    std::fs::create_dir_all(&dir).expect("a throwaway project folder");
    (root, Tree::new(&dir).expect("a .naivepost folder is a project"))
}

/// The five keys, sorted: what the body is made of, without depending on serde_json's key ordering.
fn keys(body: &serde_json::Value) -> Vec<&str> {
    let mut some: Vec<&str> = body.as_object().expect("an object").keys().map(String::as_str).collect();
    some.sort_unstable();
    some
}

/// S1: nothing is asked of a server before there is a voice to clone.
#[test]
fn f4_4_s1_the_reference_must_be_on_disk_before_anything_is_asked() {
    let (root, tree) = temp_tree("s1");
    let problem = tts::reference_problem(&tree).expect("a project with no reference cannot speak");
    assert!(problem.contains("narrate/voice_ref.wav"), "{problem}");
    // And where to go: the file is F4.6's to build, not this call's to invent.
    assert!(problem.contains("F4.6"), "{problem}");

    std::fs::create_dir_all(tree.voice_ref_wav().parent().expect("narrate/ has a parent")).unwrap();
    std::fs::write(tree.voice_ref_wav(), b"RIFF").unwrap();
    assert_eq!(tts::reference_problem(&tree), None, "the reference the model clones from is on disk");

    let _ = std::fs::remove_dir_all(&root);
}

/// S2: two questions in order — is anything answering `/health`, and can what it serves clone a voice.
#[test]
fn f4_4_s2_health_first_then_a_model_that_can_clone() {
    // Unhealthy: the model list is nobody's business yet, so that is the only thing said.
    let down = tts::server_problem(false, &[model("index-tts2", "index_tts2", "clon")])
        .expect("a server that never answered is a failure");
    assert!(down.contains("/health"), "{down}");
    assert!(down.contains(&checks::HEALTH_SECONDS.to_string()), "{down}");

    // Healthy and serving nothing.
    let empty = tts::server_problem(true, &[]).expect("nothing served cannot narrate");
    assert!(empty.contains("serving no models"), "{empty}");

    // The interesting one: a server on the right port serving only step-1 models answers every request and
    // still cannot narrate. This is the same verdict the Settings row gives (checks::tts_endpoint_verdict),
    // so the two can never disagree about whether a box will speak.
    let step1 = [model("step1-tts", "step1", "tts")];
    let refused = tts::server_problem(true, &step1).expect("step-1 cannot clone");
    assert!(refused.contains("clone"), "{refused}");
    assert_eq!(Some(refused.clone()), checks::tts_endpoint_verdict(&step1).err());

    // Either declaration passes: a server can name the same model by family or by task.
    assert_eq!(tts::server_problem(true, &[model("index-tts2", "index_tts2", "tts")]), None);
    assert_eq!(tts::server_problem(true, &[model("something-else", "other", "clon")]), None);
}

/// S3: a remembered server path dies with a server restart, so the reference goes up again per line.
#[test]
fn f4_4_s3_the_reference_goes_up_again_for_every_line() {
    assert!(bodies::VOICE_REF_REUPLOADED_EVERY_LINE);

    let body = tts::request("index-tts2", "hello", &uploaded(), "en", "", 7);
    let sent = body["voice_ref"].as_str().expect("a voice_ref");
    assert_eq!(sent, "/srv/uploads/voice_ref.wav", "the path the upload returned: {sent}");
    // Never a local path, and never a remembered one dressed up as a server path.
    assert!(!sent.contains("narrate/"), "{sent} is a folder on this machine");
    assert_ne!(sent, "narrate/voice_ref.wav");
}

/// S4: the five keys, the language, and the emotion riding inside `options` — never at the top level.
#[test]
fn f4_4_s4_the_request_carries_the_five_keys_and_the_emotion_rides_in_options() {
    // P.policy.ttsLanguage
    let body = tts::request("index-tts2", "zdanie", &uploaded(), "pl", "angry=1", 12345);
    assert_eq!(keys(&body), ["input", "language", "model", "options", "voice_ref"]);

    // The project's language, the same one ASR and alignment send: a model told to speak English reads
    // Polish words as English spelling.
    assert_eq!(body["language"], "pl");
    assert_ne!(body["language"], "en", "the prototype's hard-coded answer must not win here");
    // With no language in the project there is nothing to say how the words are pronounced, and then the
    // fallback is the honest guess.
    assert_eq!(tts::language(""), tts::LANGUAGE_FALLBACK);
    assert_eq!(tts::LANGUAGE_FALLBACK, "en");
    assert_eq!(tts::language("  pl  "), "pl", "and a real language is never overridden");

    // The seed makes a take re-speakable, and it rides inside options as a string.
    assert_eq!(body["options"]["seed"], "12345");

    // A weighted tag whose names are all known becomes eight floats at full strength.
    let vector = body["options"]["emotion_vector"].as_str().expect("a vector");
    let axes: Vec<&str> = vector.split(',').collect();
    assert_eq!(axes.len(), 8, "{vector}");
    assert_eq!(axes[0], "0");
    assert_eq!(axes[1], "1", "angry is the second axis");
    assert_eq!(body["options"]["emotion_alpha"], "1", "an exact weight is not diluted");
    assert_eq!(body["options"].get("use_emotion_text"), None, "exactly one branch is sent");
    assert_eq!(body["options"].get("emotion_text"), None);

    // A bare word and an unknown one both go to the judge, at the judge's alpha.
    for tag in ["angry", "smug"] {
        let options = tts::options(tag, 3);
        assert_eq!(options["use_emotion_text"], "true", "{tag}");
        assert_eq!(options["emotion_text"], tag, "{tag}");
        assert_eq!(options["emotion_alpha"], tts::EMOTION_ALPHA, "{tag}");
        assert_eq!(options.get("emotion_vector"), None, "{tag}");
    }

    // No emotion at all: neither branch, just the alpha and the seed.
    let plain = tts::options("", 9);
    assert_eq!(plain["emotion_alpha"], tts::EMOTION_ALPHA);
    assert_eq!(plain["seed"], "9");
    assert_eq!(plain.as_object().expect("an object").len(), 2, "{plain}");

    // The emotion belongs in options: the endpoint drops unknown top-level fields.
    assert_eq!(body.get("emotion"), None, "{body}");
    assert_eq!(body.get("seed"), None, "{body}");
}

/// S5: a take is a 200 of at least 1000 bytes; everything else is named and nothing is filed as audio.
#[test]
fn f4_4_s5_only_a_real_wav_is_a_take() {
    assert_eq!(tts::MIN_WAV_BYTES, 1000);
    assert_eq!(tts::reply_problem(200, &vec![7u8; 1000]), None, "exactly the minimum is a wav");

    let short = tts::reply_problem(200, &vec![7u8; 999]).expect("999 bytes is not a wav");
    assert!(short.contains("999"), "{short}");
    assert!(short.contains("not a wav"), "{short}");

    // A server's own error is the diagnosis: "no room" and "not served" want opposite fixes.
    let broken = tts::reply_problem(500, b"model exploded").expect("a 500 is a failure");
    assert!(broken.contains("500"), "{broken}");
    assert!(broken.contains("model exploded"), "{broken}");

    // The body is quoted as far as it is useful and no further.
    let long = tts::reply_problem(500, &vec![b'x'; 400]).expect("a 500 is a failure");
    let quoted = long.rsplit(": ").next().expect("the quoted body");
    assert_eq!(quoted.chars().count(), 300, "{quoted}");

    let (root, tree) = temp_tree("s5");
    let key = "25e0.85|hello|";
    let wav = tts::write_take(&tree, key, &[7u8; 1024]).expect("a real reply is written");
    assert!(wav.exists(), "{} was not written", wav.display());
    assert_eq!(wav.parent().and_then(Path::file_name), Some("tts".as_ref()), "{wav:?}");
    let stem = wav.file_stem().and_then(|s| s.to_str()).expect("a file name").to_string();
    assert_eq!(stem, narration::tts_file(key), "the take is named by its key's hash");
    assert_eq!(stem.len(), 16, "{stem}");
    assert_eq!(stem, wav.file_name().and_then(|n| n.to_str()).unwrap().trim_end_matches(".wav"));
    assert_eq!(tts::take_path(&tree, key), wav);

    // And a short reply never reaches the folder: a JSON error filed as a take would be replayed forever.
    let folder = wav.parent().expect("a take sits in a folder");
    let before = std::fs::read_dir(folder).map(|d| d.count()).unwrap_or(0);
    let err = tts::write_take(&tree, "another key", b"no room").expect_err("ten bytes is not a wav");
    assert!(err.contains("not a wav"), "{err}");
    let after = std::fs::read_dir(folder).map(|d| d.count()).unwrap_or(0);
    assert_eq!(before, after, "the refusal left nothing behind: {after} of {before}");

    let _ = std::fs::remove_dir_all(&root);
}

/// S6: eight bases with their kin, 21 named blends, weights 0..1 — and an unknown name is a route, not an error.
#[test]
fn f4_4_s6_the_vocabulary_is_eight_bases_their_kin_and_21_blends_and_nothing_is_an_error() {
    assert_eq!(tts::BLENDS.len(), 21);

    // Every base names its own axis, and one base alone lights only that one.
    for (index, base) in EMOTIONS.iter().enumerate() {
        assert_eq!(tts::base(base), Some(index), "{base} is axis {index}");
        let vector = tts::emotion_vector(&format!("{base}=1")).unwrap_or_else(|| panic!("{base}=1 resolves"));
        for (axis, weight) in vector.iter().enumerate() {
            assert_eq!(*weight, if axis == index { 1.0 } else { 0.0 }, "{base} lit {axis}");
        }
    }

    // A kin word is one of the eight, and a blend is a recipe over them rather than a synonym.
    assert_eq!(tts::base("FURIOUS "), Some(1), "someone writing \"furious\" means angry");
    let excited = tts::emotion_vector("excited=1").expect("a named blend resolves");
    assert_eq!((excited[0], excited[6]), (1.0, 0.55), "excited is happy plus surprise");
    assert!(tts::blend("excited").is_some());
    assert!(tts::blend("banana").is_none());

    // Weights are 0..1: a tag that shouts is not louder than the loudest ask, and one that inverts is not
    // the opposite emotion. Clamped to nothing, it asks for nothing — and then the judge reads the line.
    assert_eq!(tts::emotion_vector("angry=2").map(|v| v[1]), Some(1.0));
    assert_eq!(tts::emotion_vector("angry=-1"), None);

    // Named twice: the louder ask wins.
    assert_eq!(tts::emotion_vector("sad=0.3, sad=0.8").map(|v| v[2]), Some(0.8));

    // A blend scaled by a weight lands on numbers like 0.16499999999999998; the wire wants three decimals.
    let joined = tts::options("playful=0.3", 1)["emotion_vector"].as_str().expect("a vector").to_string();
    assert!(!joined.contains("9999"), "{joined} carries a float's tail");
    for axis in joined.split(',') {
        let decimals = axis.split('.').nth(1).map_or(0, str::len);
        assert!(decimals <= 3, "{axis} has more than three decimals");
    }

    // Every one of these is the judge's branch rather than an error: S6 says unknown names go to the judge
    // as words, never that a line goes unsaid.
    assert_eq!(tts::emotion_vector(""), None);
    assert_eq!(tts::emotion_vector("banana=0.5"), None);
    assert_eq!(tts::emotion_vector("angry=banana"), None, "a weight that is not a number is not a weight");
    assert_eq!(tts::emotion_vector("angry=0"), None, "\"angry=0\" asks for nothing");
    assert_eq!(tts::emotion_vector("angry"), None, "an unweighted word goes through the judge anyway");

    // The judge sees the tag with the weights taken off, so a name this client does not know still arrives as
    // a word it can score.
    assert_eq!(tts::emotion_text("wistful=1, smug=0.3"), "wistful, smug");
    assert_eq!(tts::emotion_text("deadpan"), "deadpan");
}

// --- the flow itself: `speak_line` runs S1-S5 in order and stops at the first refusal ---------------

/// A reference on disk, so S1 passes and the later steps are the ones being tested.
fn with_reference(tree: &Tree) {
    std::fs::create_dir_all(tree.voice_ref_wav().parent().expect("narrate/ has a parent")).unwrap();
    std::fs::write(tree.voice_ref_wav(), b"RIFF fake reference").unwrap();
}

/// A server that is up and serving something able to clone, so S2 passes.
fn good_models() -> Vec<AudioModel> {
    vec![model("index-tts2", "index_tts2", "clon")]
}

/// The counters each closure bumps, so "never reached" is an asserted fact rather than a hope.
#[derive(Default)]
struct Calls {
    uploads: std::cell::Cell<u32>,
    posts: std::cell::Cell<u32>,
}

/// Run one line through the flow with counting closures and a scripted reply.
fn run(
    tree: &Tree,
    emotion: &str,
    language: &str,
    healthy: bool,
    models: &[AudioModel],
    upload_ok: bool,
    reply: Result<(u16, Vec<u8>), String>,
) -> (tts::Outcome, Calls, serde_json::Value) {
    let calls = Calls::default();
    let mut captured = serde_json::Value::Null;
    let key = format!("key|{emotion}|{}", language);
    let seed = narration::tts_seed(&key);
    let outcome = tts::speak_line(
        tree,
        "one sentence to speak",
        emotion,
        seed,
        &key,
        language,
        healthy,
        models,
        "index-tts2",
        |_tree| {
            calls.uploads.set(calls.uploads.get() + 1);
            if upload_ok {
                Ok(uploaded())
            } else {
                Err("the upload was refused by the server".to_string())
            }
        },
        |body| {
            calls.posts.set(calls.posts.get() + 1);
            captured = body.clone();
            reply
        },
    );
    (outcome, calls, captured)
}

/// S7: the refusal ORDER. A missing reference stops before the server is touched at all; an unhealthy
/// server stops before the request goes out. Each step's own sentence is what comes back.
#[test]
fn f4_4_s7_the_steps_run_in_order_and_a_refusal_touches_nothing_after_it() {
    // No reference on disk: not even the upload runs, though the server would have said yes.
    let (_root, tree) = temp_tree("s7-no-ref");
    let (outcome, calls, body) = run(
        &tree,
        "calm",
        "pl",
        true,
        &good_models(),
        true,
        Ok((200, vec![9_u8; 4000])),
    );
    assert_eq!(calls.uploads.get(), 0, "S1 refused before any upload");
    assert_eq!(calls.posts.get(), 0, "S1 refused before any speech call");
    assert!(body.is_null(), "nothing was ever built to send");
    let why = outcome.refused().expect("a missing reference is a refusal").to_string();
    assert!(why.contains("voice_ref.wav"), "{why} must name what is missing");
    assert_eq!(outcome.wav(), None, "no take from a refusal");

    // Reference present but the server is down: still no upload, no request.
    with_reference(&tree);
    let (outcome, calls, _body) = run(
        &tree,
        "calm",
        "pl",
        false,
        &[],
        true,
        Ok((200, vec![9_u8; 4000])),
    );
    assert_eq!(calls.uploads.get(), 0, "S2 refused before uploading the reference");
    assert_eq!(calls.posts.get(), 0, "S2 refused before the speech call");
    let why = outcome.refused().expect("an unhealthy server is a refusal").to_string();
    assert!(why.contains("/health"), "{why} must name the probe that failed");

    // Healthy but serving nothing that can clone: same short-circuit, different sentence.
    let (outcome, calls, _body) = run(
        &tree,
        "calm",
        "pl",
        true,
        &[model("whisper", "whisper", "asr")],
        true,
        Ok((200, vec![9_u8; 4000])),
    );
    assert_eq!(calls.uploads.get(), 0, "a model that cannot clone stops before the upload");
    let why = outcome.refused().expect("no cloner is a refusal").to_string();
    assert!(why.contains("clone"), "{why}: {why}");

    let _ = std::fs::remove_dir_all(_root);
}

/// S3: the reference goes up again for EVERY line. Nothing caches a server path across calls, because a
/// restart forgets it and a remembered path names nothing — `bodies::VOICE_REF_REUPLOADED_EVERY_LINE`.
#[test]
fn f4_4_s8_the_reference_is_uploaded_for_every_single_line() {
    assert!(
        bodies::VOICE_REF_REUPLOADED_EVERY_LINE,
        "the rule this test pins is the constant stating it"
    );
    let (root, tree) = temp_tree("s8-reupload");
    with_reference(&tree);

    for line in 1..=3 {
        let (outcome, calls, _body) = run(
            &tree,
            "calm",
            "en",
            true,
            &good_models(),
            true,
            Ok((200, vec![9_u8; 2000])),
        );
        assert_eq!(
            calls.uploads.get(),
            1,
            "line {line}: exactly one upload per speak, never zero and never reused"
        );
        assert_eq!(calls.posts.get(), 1, "line {line}: one speech call after the upload");
        assert!(outcome.wav().is_some(), "line {line} should have produced a take");
    }

    let _ = std::fs::remove_dir_all(root);
}

/// S4: the body that reaches the wire is `request`'s shape, the emotion rides inside `options`, and the
/// language is the project's own.
#[test]
fn f4_4_s9_the_body_that_reaches_the_wire_is_the_request_shape() {
    // P.policy.ttsLanguage
    let (root, tree) = temp_tree("s9-body");
    with_reference(&tree);

    // A known weighted tag: eight floats, alpha 1 (alpha is a multiplier over the weights, so a vector
    // arriving at 0.85 would not be the weight that was written).
    let (_o, _c, body) = run(
        &tree,
        "angry=1, happy=0.4",
        "pl",
        true,
        &good_models(),
        true,
        Ok((200, vec![9_u8; 2000])),
    );
    assert_eq!(keys(&body), ["input", "language", "model", "options", "voice_ref"]);
    assert_eq!(body["language"], "pl", "the project's language, not a hard-coded \"en\"");
    assert_eq!(body["model"], "index-tts2");
    assert_eq!(body["input"], "one sentence to speak");
    assert_eq!(body["voice_ref"], "/srv/uploads/voice_ref.wav", "the path THIS line uploaded");
    let options = body["options"].as_object().expect("options is an object");
    assert_eq!(options["emotion_alpha"], "1", "a vector is spoken at alpha 1");
    assert!(
        options.contains_key("emotion_vector"),
        "a known weighted tag becomes eight floats: {options:?}"
    );
    let floats: Vec<&str> = options["emotion_vector"]
        .as_str()
        .expect("the vector is a comma string")
        .split(',')
        .collect();
    assert_eq!(floats.len(), 8, "eight axes: {:?}", options["emotion_vector"]);
    assert!(
        !options.contains_key("use_emotion_text"),
        "the two branches are never both sent: {options:?}"
    );

    // An unknown word: the judge gets it as words, at EMOTION_ALPHA.
    let (_o, _c, body) = run(
        &tree,
        "deadpan",
        "pl",
        true,
        &good_models(),
        true,
        Ok((200, vec![9_u8; 2000])),
    );
    let options = body["options"].as_object().expect("options is an object");
    assert_eq!(options["use_emotion_text"], "true");
    assert_eq!(options["emotion_text"], "deadpan");
    assert_eq!(options["emotion_alpha"], "0.85");
    assert!(
        !options.contains_key("emotion_vector"),
        "an unknown name is never guessed into a vector: {options:?}"
    );

    // An empty project language falls back to LANGUAGE_FALLBACK rather than sending "".
    let (_o, _c, body) = run(
        &tree,
        "calm",
        "",
        true,
        &good_models(),
        true,
        Ok((200, vec![9_u8; 2000])),
    );
    assert_eq!(body["language"], tts::LANGUAGE_FALLBACK, "an unset project language falls back");

    let _ = std::fs::remove_dir_all(root);
}

/// S5: only a 200 of at least MIN_WAV_BYTES is a take, and the file lands where `take_path` says.
#[test]
fn f4_4_s10_only_a_real_reply_becomes_a_file_and_it_lands_where_take_path_says() {
    let (root, tree) = temp_tree("s10-reply");
    with_reference(&tree);

    // A 200 one byte short of the floor: refused, and nothing written.
    let (outcome, _calls, _b) = run(
        &tree,
        "calm",
        "en",
        true,
        &good_models(),
        true,
        Ok((200, vec![9_u8; tts::MIN_WAV_BYTES - 1])),
    );
    assert!(outcome.wav().is_none(), "999 bytes is not a wav");
    let why = outcome.refused().expect("short reply refused").to_string();
    assert!(why.contains("999"), "{why} should quote the size it got");
    assert_eq!(count_takes(&tree), 0, "a refused reply leaves no file behind");

    // A 500 with plenty of bytes: still refused, still nothing written.
    let (outcome, _calls, _b) = run(
        &tree,
        "calm",
        "en",
        true,
        &good_models(),
        true,
        Ok((500, vec![9_u8; 5000])),
    );
    assert!(outcome.wav().is_none(), "a 500 is never a take");
    let why = outcome.refused().expect("a 500 is a refusal").to_string();
    assert!(why.contains("500"), "{why} should quote the status");
    assert_eq!(count_takes(&tree), 0, "the error page was not filed as audio");

    // A real reply: the take exists at exactly the path `take_path` names, with the bytes it carried.
    let bytes = vec![9_u8; tts::MIN_WAV_BYTES + 500];
    let key = "key|calm|en".to_string();
    let seed = narration::tts_seed(&key);
    let outcome = tts::speak_line(
        &tree,
        "one sentence to speak",
        "calm",
        seed,
        &key,
        "en",
        true,
        &good_models(),
        "index-tts2",
        |_| Ok(uploaded()),
        |_| Ok((200, bytes.clone())),
    );
    let file = outcome.wav().expect("a good reply is a take").to_path_buf();
    assert_eq!(file, tts::take_path(&tree, &key), "written where take_path points");
    assert_eq!(file.parent().unwrap(), tts::take_path(&tree, "x").parent().unwrap(), "under narrate/tts/");
    assert_eq!(std::fs::read(&file).expect("the take is readable"), bytes);
    assert_eq!(count_takes(&tree), 1);
    assert_eq!(outcome.refused(), None);

    let _ = std::fs::remove_dir_all(root);
}

/// How many wavs sit in the take folder. Its path is derived from a real take rather than by rebuilding
/// `narrate/tts/` here, because `Tree::narrate_dir` is private and this test should not need it.
fn count_takes(tree: &Tree) -> usize {
    let sample = tts::take_path(tree, "count-probe");
    let folder = sample.parent().expect("a take path has a folder");
    std::fs::read_dir(folder)
        .map(|entries| entries.count())
        .unwrap_or(0)
}

/// S3's failure leg: an upload that does not answer is the end of the line — no speech call, no file.
#[test]
fn f4_4_s11_a_refused_upload_never_reaches_the_speech_call() {
    let (root, tree) = temp_tree("s11-upload-fail");
    with_reference(&tree);

    let (outcome, calls, body) = run(
        &tree,
        "excited",
        "en",
        true,
        &good_models(),
        false,
        Ok((200, vec![9_u8; 4000])),
    );
    assert_eq!(calls.uploads.get(), 1, "the upload was attempted");
    assert_eq!(calls.posts.get(), 0, "and its failure stopped the flow there");
    assert!(body.is_null(), "no request body was ever built");
    let why = outcome.refused().expect("a refused upload is a refusal").to_string();
    assert!(why.contains("upload"), "{why} must name the step that failed");
    assert_eq!(count_takes(&tree), 0, "nothing was written for a line that never went out");

    let _ = std::fs::remove_dir_all(root);
}
