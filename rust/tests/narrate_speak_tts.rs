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
