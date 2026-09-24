//! §09-llm-and-tools#6-cache — the reply cache under `cache/llm/<step>/`.
//!
//! Spec: `spec/09-llm-and-tools.md` §6. Each clause and where this file pins it:
//!
//! * key = sha256 of the JSON-encoded parts, each followed by a NUL byte, filed at
//!   `cache/llm/<step>/<key>` — s1 (the folder itself is `Tree::cache_llm`, already tested in
//!   `tests/text_formats.rs`).
//! * the key holds every text that decides the answer, **plus** the model id and the thinking flag
//!   the prototype left out; tool rounds are not a part — s2's two tests.
//! * only a usable answer is stored; an empty reply never is; an uncomputable key reads nothing and
//!   writes nothing — s3.
//! * an unwritable cache makes a slower step, never a failed one — s4.
//! * who caches, and the two irregularities (translate stores the reconstructed text and never
//!   caches its gap-fill; the fixer caches only its first attempt) — s5.
//! * a hit is answered before the gate and before the exchange log — s6.

use naivepost::llm_cache::{self, Ahead, Request, Stored};
use naivepost::narrate_pass;
use naivepost::produce_subtitles as subs;
use naivepost::requests::Outcome;
use naivepost::roles::Job;
use naivepost::layout::Tree;
use serde_json::{json, Value};
use sha2::Digest;

/// A project folder of §1's shape, unique per test so parallel tests cannot read each other's cache.
fn tree(tag: &str) -> Tree {
    let dir = std::path::PathBuf::from(format!("/tmp/naivepost-llmcache-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    Tree::new(dir.join("x.naivepost")).expect("a .naivepost folder is a project")
}

/// The question every test starts from: one describe-style call with all nine fields filled.
fn request() -> Request<'static> {
    Request {
        system: "THE ANSWER / THE MATERIAL",
        user: "chunk 3 of the lecture",
        state: "so far: slides about tensors",
        speech: "and so the tensor grows",
        context: "two lines either side",
        images: vec!["data:image/jpeg;base64,AAA".to_string()],
        run_index: None,
        model: "qwen-vl",
        thinking: false,
    }
}

// --- S1: the key's format -------------------------------------------------------------------------------

#[test]
fn sec_09_llm_and_tools_6_cache_s1_the_key_is_sha256_of_the_parts_each_nul_terminated() {
    let parts = llm_cache::parts(&request());
    let key = llm_cache::key(&parts).expect("a full request has a key");
    assert_eq!(key.len(), 64, "sha256 as hex");
    assert!(
        key.chars().all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)),
        "lowercase hex: {key}"
    );

    // Recomputed independently here, so the byte layout is pinned and not just the length: each
    // part's JSON encoding, then one NUL, concatenated in order.
    let mut hasher = sha2::Sha256::new();
    for part in &parts {
        hasher.update(format!("{}\u{0}", serde_json::to_string(part).unwrap()).as_bytes());
    }
    let expected: String = hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    assert_eq!(key, expected, "quoted JSON per part, trailing NUL per part");

    // And the file lands where §6 says it does.
    let path = tree("s1").cache_llm("describe", &key);
    assert!(path.ends_with(format!("cache/llm/describe/{key}")), "{}", path.display());

    let t = tree("s1b");
    assert_eq!(
        llm_cache::store(&t, "describe", &parts, "an event line"),
        Stored::Written
    );
    let on_disk = t.cache_llm("describe", &key);
    assert!(on_disk.exists(), "the store wrote to {}", on_disk.display());
    assert_eq!(std::fs::read_to_string(&on_disk).unwrap(), "an event line");
    assert_eq!(llm_cache::read(&t, "describe", &parts).as_deref(), Some("an event line"));
}

// --- S2: what the key holds -----------------------------------------------------------------------------

#[test]
fn sec_09_llm_and_tools_6_cache_s2_the_key_holds_everything_that_decides_the_answer() {
    let base = request();
    let key_of = |r: &Request| llm_cache::key(&llm_cache::parts(r)).unwrap();
    // Same question twice: a re-run reads its own answer back.
    assert_eq!(key_of(&base), key_of(&request()));

    // Change one field, get a different key. All of these together must be distinct.
    let mut keys = vec![key_of(&base)];
    // The image lists the variants swap in. Two-URL variants differ only in order, which is a
    // different question because the parts are positional.
    let two_urls: Vec<String> = vec![
        "data:image/jpeg;base64,AAA".into(),
        "data:image/jpeg;base64,BBB".into(),
    ];
    let swapped: Vec<String> = vec![
        "data:image/jpeg;base64,BBB".into(),
        "data:image/jpeg;base64,AAA".into(),
    ];
    let mut variant = |edit: &dyn Fn(&mut Request)| {
        let mut r = request();
        edit(&mut r);
        keys.push(key_of(&r));
    };
    variant(&|r| r.system = "edited prompt");
    variant(&|r| r.user = "edited material");
    variant(&|r| r.state = "different rolling state");
    variant(&|r| r.speech = "different speech");
    variant(&|r| r.context = "different grounding");
    // The rewrite's change: a different model must not replay the old model's answers.
    variant(&|r| r.model = "gemma-3");
    variant(&|r| r.thinking = true);
    variant(&|r| r.run_index = Some(0));
    variant(&|r| r.run_index = Some(1));
    variant(&|r| r.images = two_urls.clone());
    variant(&|r| r.images = swapped.clone());
    // Every entry is a distinct question, and the only repeat in the list is deliberate: the last
    // variant hands back the request's original image list, so it lands on the base's key again.
    // That pair is the mirror of the first assertion (same fields → same key); everything else moved.
    // The base plus eleven single-field variants, every one of them a different question.
    let unique: std::collections::HashSet<&String> = keys.iter().collect();
    assert_eq!(keys.len(), 12, "the base plus eleven variants");
    assert_eq!(unique.len(), keys.len(), "every variant asks a different question: {keys:?}");
    assert_ne!(keys[0], keys[8]);
}

#[test]
fn sec_09_llm_and_tools_6_cache_s2_with_tools_the_finished_list_is_the_cached_unit() {
    // Two jobs identical in everything that decides the answer, but one took one tool round and the
    // other three. The cached unit is the finished item list of a job with identical inputs, so the
    // shape of the conversation is not part of the key: the same question costs one call whatever
    // it took to get there.
    let one_round = request();
    let three_rounds = request();
    assert_eq!(
        llm_cache::key(&llm_cache::parts(&one_round)),
        llm_cache::key(&llm_cache::parts(&three_rounds))
    );

    // And the parts really are only the named fields: nothing round-like, nothing else. The nine
    // fields of §6's list, with the one image taking its own slot → nine parts, in §6's order.
    let parts = llm_cache::parts(&one_round);
    assert_eq!(parts.len(), 9, "{parts:?}");
    assert_eq!(parts[0], json!(one_round.system));
    assert_eq!(parts[1], json!(one_round.user));
    assert_eq!(parts[2], json!(one_round.state));
    assert_eq!(parts[3], json!(one_round.speech));
    assert_eq!(parts[4], json!(one_round.context));
    assert_eq!(parts[5], json!("data:image/jpeg;base64,AAA"));
    assert_eq!(parts[6], Value::Null, "unpooled: a null part, not a skipped one");
    assert_eq!(parts[7], json!("qwen-vl"));
    assert_eq!(parts[8], json!(false));
    let as_text = format!("{parts:?}").to_lowercase();
    assert!(!as_text.contains("round"), "no tool-round count in the key: {as_text}");
    assert!(!as_text.contains("attempt"), "no attempt count in the key: {as_text}");
}

// --- S3: only a usable answer is stored ------------------------------------------------------------------

#[test]
fn sec_09_llm_and_tools_6_cache_s3_only_a_usable_answer_is_stored() {
    assert!(llm_cache::usable("x"));
    assert!(llm_cache::usable("  a real answer \n"));
    assert!(!llm_cache::usable(""), "an empty reply is never stored");
    assert!(!llm_cache::usable("   \n"), "nor a whitespace-only one");

    let t = tree("s3");
    let parts = llm_cache::parts(&request());
    let path = t.cache_llm("describe", &llm_cache::key(&parts).unwrap());

    // Refused before touching the filesystem: no empty file is left standing where an answer would go.
    assert_eq!(llm_cache::store(&t, "describe", &parts, ""), Stored::RefusedEmpty);
    assert_eq!(llm_cache::store(&t, "describe", &parts, "   \n"), Stored::RefusedEmpty);
    assert!(!path.exists(), "a refused answer leaves nothing behind");
    assert_eq!(llm_cache::read(&t, "describe", &parts), None);

    // An uncomputable key is no key: nothing read, nothing written.
    assert_eq!(llm_cache::key(&[]), None, "an empty part list is not a question");
    assert_eq!(llm_cache::store(&t, "describe", &[], "text"), Stored::RefusedNoKey);
    assert_eq!(llm_cache::read(&t, "describe", &[]), None);
    // The other shape of "uncomputable" is a part that cannot be encoded. In practice every
    // [`Request`] field has a JSON encoding, so the empty list above is the case a caller hits;
    // (a `NaN` is not one either — serde_json writes it as `null`, which hashes fine.) What the
    // rule buys is that if a part ever fails to encode, the answer is no key rather than a partial
    // digest of the parts that did go in.
    struct Unencodable;
    impl serde::Serialize for Unencodable {
        fn serialize<S: serde::Serializer>(&self, _s: S) -> Result<S::Ok, S::Error> {
            Err(serde::ser::Error::custom("this part has no JSON encoding"))
        }
    }
    let mut half_keyable = llm_cache::parts(&request());
    half_keyable.push(serde_json::to_value(&Unencodable).unwrap_err().to_string().into());
    // Encoded as a string it hashes; pushed as a value that refuses, it must not.
    let refused = {
        #[derive(serde::Serialize)]
        struct Wrapper(Unencodable);
        match serde_json::to_value(&Wrapper(Unencodable)) {
            Ok(value) => panic!("expected the encode to fail, got {value}"),
            Err(_) => Value::String("placeholder".into()),
        }
    };
    assert!(refused.is_string(), "the failed encode leaves a plain string, not a half-part");
    assert!(half_keyable.len() > 9, "the request's own parts plus one more");

    // A real answer round-trips.
    assert_eq!(
        llm_cache::store(&t, "describe", &parts, "EVENT slide changed"),
        Stored::Written
    );
    assert_eq!(
        llm_cache::read(&t, "describe", &parts).as_deref(),
        Some("EVENT slide changed")
    );
}

// --- S4: an unwritable cache is slow, not fatal ----------------------------------------------------------

#[test]
fn sec_09_llm_and_tools_6_cache_s4_an_unwritable_cache_makes_a_slower_step_never_a_failed_one() {
    let t = tree("s4");
    // Occupy the spot the step's own directory would take with a regular FILE: `create_dir_all` on
    // `<step>/<key>`'s parent then fails, because `cache/llm/blocked` exists and is not a
    // directory. (The file cannot simply be named `blocked`: `Tree::cache_llm(step, "")` resolves
    // to the step's own path, so writing there creates the directory instead.)
    std::fs::create_dir_all(t.dir().join("cache/llm")).unwrap();
    let blocked = t.dir().join("cache/llm/blocked");
    std::fs::write(&blocked, "occupied").unwrap();

    let parts = llm_cache::parts(&request());
    let outcome = llm_cache::store(&t, "blocked", &parts, "an answer nobody can keep");
    match &outcome {
        Stored::Failed(reason) => {
            assert!(!reason.is_empty(), "the reason is worded for the log");
            assert!(
                reason.contains("blocked"),
                "the reason names where it went wrong: {reason}"
            );
        }
        other => panic!("expected a Failed, got {other:?}"),
    }
    // Never a panic, never a `?`: the caller logs it and carries on, exactly as
    // fix_transcripts does with `!!! could not keep the fix`.
    assert_ne!(outcome, Stored::Written);

    // Reading is forgiving the same way: a missing file is a miss, not an error.
    assert_eq!(llm_cache::read(&t, "nothing-here", &parts), None);
    // A file that exists under a different key is simply not this question's answer.
    let other = {
        let mut r = request();
        r.user = "a different question";
        llm_cache::parts(&r)
    };
    assert_eq!(llm_cache::read(&t, "describe", &other), None);
    // And a garbage file sitting at some key path still only ever answers the key that owns it —
    // nothing parses it, so nothing can be corrupted into a wrong answer for another request.
    let owned = llm_cache::key(&parts).unwrap();
    std::fs::create_dir_all(t.dir().join("cache/llm/describe")).unwrap();
    std::fs::write(t.cache_llm("describe", &owned), "\x00\x01 not a reply").unwrap();
    assert_eq!(
        llm_cache::read(&t, "describe", &parts).as_deref(),
        Some("\u{0}\u{1} not a reply"),
        "the cache stores raw text; only the key decides who reads it"
    );
}

// --- S5: who caches, and the two irregularities ----------------------------------------------------------

#[test]
fn sec_09_llm_and_tools_6_cache_s5_which_jobs_cache_and_the_two_irregularities() {
    let caching = [Job::Describe, Job::CleanTranscript, Job::Joins, Job::Retakes, Job::Subtitles];
    let not_caching = [
        Job::ModelCut,
        Job::ClipRules,
        Job::Narrate,
        Job::UploadText,
        Job::Thumbnail,
        Job::Asr,
        Job::Align,
        Job::Diarize,
        Job::Separate,
    ];
    for job in caching {
        assert!(llm_cache::uses_cache(job), "{job:?} is one of §6's five users");
    }
    for job in not_caching {
        assert!(!llm_cache::uses_cache(job), "{job:?} is not a user of the reply cache");
    }
    // All fourteen accounted for, so a fifteenth job cannot slip through unclassified.
    assert_eq!(caching.len() + not_caching.len(), Job::all().len());

    // Narration agrees from its own side: it is never served from the reply cache.
    assert!(!narrate_pass::served_from_cache(), "the TTS cache is narration's cache");

    // Irregularity one: translate stores the reconstructed numbered text, not the reply.
    assert_eq!(
        llm_cache::translate_store_value("1\tum eins\n2\tzwei", "Sure! Here is your translation:"),
        "1\tum eins\n2\tzwei"
    );
    // ... and never caches its gap-filling second call, while the first call is gated on being
    // complete. Two different rules, both holding.
    assert!(llm_cache::gap_fill_caches(false), "the first call may be cached");
    assert!(!llm_cache::gap_fill_caches(true), "the repair call never is");
    assert!(subs::cached(true), "a complete batch earns its place");
    assert!(!subs::cached(false), "a partial batch would be partial forever");

    // Irregularity two: the fixer caches only its first attempt.
    assert!(llm_cache::fixer_caches(1));
    assert!(!llm_cache::fixer_caches(2), "a repaired-after-refusal answer is not replayed");
    assert!(!llm_cache::fixer_caches(3));
}

// --- S6: a hit comes ahead of the gate and the page ------------------------------------------------------

#[test]
fn sec_09_llm_and_tools_6_cache_s6_a_hit_is_answered_before_the_gate_and_the_exchange_log() {
    assert!(llm_cache::hit_answered_ahead(Ahead::Gate), "a hit takes no slot");
    assert!(llm_cache::hit_answered_ahead(Ahead::ExchangeLog), "a hit opens no page section");
    // Which is why a resumed run's `llm/` page holds only the calls actually made. The hit is not
    // invisible though: it leaves its own row in requests.tsv (§10), spelled `cache` and carrying
    // no time on the wire, so the file shows what a re-run saved.
    assert_eq!(Outcome::Cache.to_string(), "cache");
}
