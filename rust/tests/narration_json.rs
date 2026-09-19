//! §01-project-and-files#4-narratenarrationjson — the narration file's record, the order
//! it is kept in, and the TTS cache key that has to outlive versions.

use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use naivepost::cut::Seg;
use naivepost::layout::Tree;
use naivepost::narration::{self, Entry, Narration, Silent};

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "naivepost-narration-{}-{tag}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

/// A project folder inside a throwaway root, holding the given file text (or no file).
fn with_file(tag: &str, text: Option<&str>) -> (PathBuf, Tree) {
    let root = temp_dir(tag);
    let dir = root.join("demo.naivepost");
    std::fs::create_dir_all(&dir).unwrap();
    if let Some(text) = text {
        std::fs::create_dir_all(dir.join("narrate")).unwrap();
        std::fs::write(dir.join("narrate/narration.json"), text).unwrap();
    }
    (root, Tree::new(&dir).unwrap())
}

fn read(t: &Tree) -> String {
    std::fs::read_to_string(t.narration_json()).unwrap()
}

/// §4's example, verbatim.
const SPEC_NARRATION: &str = r#"{"entries": [{"s": 1.5, "e": 34.7, "at": 2.0, "text": "We start with …", "emotion": "calm", "pos": "", "roll": 0}],
 "silent": [{"s": 40.0, "e": 65.0}]}"#;

/// One line over one clip, as the cut hands it over.
fn entry(s: f64, e: f64, text: &str) -> Entry {
    Entry { s, e, text: text.to_string(), ..Default::default() }
}

#[test]
fn sec_01_project_and_files_4_narratenarrationjson_the_specs_example_round_trips() {
    let (_root, t) = with_file("spec-example", Some(SPEC_NARRATION));
    let n = narration::load(&t).expect("the spec's own file loads");

    assert_eq!(n.entries.len(), 1);
    let line = &n.entries[0];
    assert_eq!((line.s, line.e), (1.5, 34.7));
    assert_eq!(line.at, 2.0);
    assert_eq!(line.text, "We start with …");
    assert_eq!(line.emotion, "calm");
    // An empty placement is the bottom and a zero roll is no re-roll: both read as the
    // default they are, whichever way the file spelled them.
    assert_eq!(line.pos, "");
    assert_eq!(line.roll, 0);

    assert_eq!(n.silent, [Silent { s: 40.0, e: 65.0 }]);
    assert!(n.is_silent(40.0, 65.0));
    assert!(!n.is_silent(1.5, 34.7));

    // One writer, and what it writes reads back the same.
    narration::save(&n, &t).unwrap();
    assert_eq!(narration::load(&t).unwrap(), n);
}

#[test]
fn sec_01_project_and_files_4_narratenarrationjson_bounds_are_copied_verbatim_from_the_cut() {
    // The clip's bounds come off the cut and are not re-derived, so an odd pair — one
    // that a Split or a folded stretch left behind — must survive untouched.
    let seg = Seg { s: 12.34, e: 12.75, ..Default::default() };
    let mut line = entry(seg.s, seg.e, "a short beat");
    line.at = 0.41;
    let n = Narration { entries: vec![line.clone()], silent: vec![] };

    let (_root, t) = with_file("verbatim", None);
    narration::save(&n, &t).unwrap();
    let back = narration::load(&t).unwrap();
    assert_eq!(back.entries[0], line);
    assert_eq!((back.entries[0].s, back.entries[0].e), (12.34, 12.75));

    // A silent marker is bounds too, and only bounds: a cut segment's seconds land on it
    // with nothing else carried over.
    let marked = Narration { entries: vec![], silent: vec![Silent { s: seg.s, e: seg.e }] };
    narration::save(&marked, &t).unwrap();
    assert_eq!(narration::load(&t).unwrap(), marked);
    // The cut's own seconds survive the trip through this file unchanged.
    let mut from_cut = entry(seg.s, seg.e, "");
    from_cut.at = seg.s;
    assert_eq!((from_cut.s, from_cut.e), (seg.s, seg.e));
}

#[test]
fn sec_01_project_and_files_4_narratenarrationjson_at_is_on_the_recordings_own_clock_and_the_render_divides_by_the_rate() {
    // `at` is seconds into the clip on the recording's clock, rate ignored: whatever the
    // clip plays at, the line stays on the same picture (§4).
    assert_eq!(narration::output_seconds(2.0, 1.0), 2.0);
    // At rate 2 the clip's output seconds run twice as fast, so the same picture is
    // reached at half of them.
    assert_eq!(narration::output_seconds(2.0, 2.0), 1.0);
    assert_eq!(narration::output_seconds(3.0, 0.5), 6.0);
    // A stop is a still frame: nothing to divide by, so the line keeps its own seconds
    // rather than being flung off the clip.
    assert_eq!(narration::output_seconds(2.0, 0.0), 2.0);
    assert_eq!(narration::output_seconds(2.0, -1.0), 2.0);

    // The field is stored as asked: no rate is kept anywhere near it.
    let mut line = entry(0.0, 10.0, "on the chest");
    line.at = 4.5;
    let (_root, t) = with_file("own-clock", None);
    narration::save(&Narration { entries: vec![line], silent: vec![] }, &t).unwrap();
    assert_eq!(narration::load(&t).unwrap().entries[0].at, 4.5);
}

#[test]
fn sec_01_project_and_files_4_narratenarrationjson_empty_text_is_deliberately_silent_and_still_written() {
    // "" is an answer, not a gap: the line was deleted on purpose and stays a row so it
    // is not re-written by the next rebuild. So `text` is never omitted.
    let silent_line = entry(1.5, 34.7, "");
    let n = Narration { entries: vec![silent_line.clone()], silent: vec![] };
    let (_root, t) = with_file("empty-text", None);
    narration::save(&n, &t).unwrap();

    let text = read(&t);
    assert!(text.contains("\"text\": \"\""), "{text}");
    let back = narration::load(&t).unwrap();
    assert_eq!(back.entries.len(), 1);
    assert_eq!(back.entries[0], silent_line);
    // Distinguishable from no entry at all: the clip is still named by its bounds.
    assert!(!back.entries.is_empty());
    assert!(back.has_line(1.5, 34.7));

    // And a clip with no line is simply absent.
    let none = Narration::default();
    narration::save(&none, &t).unwrap();
    assert!(!narration::load(&t).unwrap().has_line(1.5, 34.7));
}

#[test]
fn sec_01_project_and_files_4_narratenarrationjson_emotion_and_pos_are_written_and_roll_salts_only_the_key() {
    let mut line = entry(1.5, 34.7, "We start with …");
    line.emotion = "calm".to_string();
    line.pos = "top".to_string();
    let n = Narration { entries: vec![line.clone()], silent: vec![] };
    let (_root, t) = with_file("emotion-pos", None);
    narration::save(&n, &t).unwrap();

    // A delivery tag is always written — the absence of one is not a delivery.
    let text = read(&t);
    assert!(text.contains("\"emotion\": \"calm\""), "{text}");
    assert!(text.contains("\"pos\": \"top\""), "{text}");
    assert_eq!(narration::load(&t).unwrap(), n);

    for pos in ["center", ""] {
        let mut one = line.clone();
        one.pos = pos.to_string();
        let n = Narration { entries: vec![one.clone()], silent: vec![] };
        narration::save(&n, &t).unwrap();
        let text = read(&t);
        if pos.is_empty() {
            // Omitted, and it reads back as the same bottom placement. §4's example shows
            // `"pos": ""` because it is listing the keys; the file this build writes
            // follows the prototype's omitempty so old files stay byte-identical.
            assert!(!text.contains("\"pos\""), "{text}");
        } else {
            assert!(text.contains(&format!("\"pos\": \"{pos}\"")), "{text}");
        }
        assert_eq!(narration::load(&t).unwrap().entries[0], one);
    }

    // `roll` reaches the file only when it says something, and never changes where a
    // line sits or what it says — only which take is spoken (see the key tests).
    let mut rolled = line.clone();
    rolled.roll = 3;
    let n = Narration { entries: vec![rolled.clone()], silent: vec![] };
    narration::save(&n, &t).unwrap();
    assert!(read(&t).contains("\"roll\": 3"), "{}", read(&t));
    assert_eq!(narration::load(&t).unwrap().entries[0], rolled);

    let unrolled = Narration { entries: vec![line.clone()], silent: vec![] };
    narration::save(&unrolled, &t).unwrap();
    assert!(!read(&t).contains("\"roll\""), "{}", read(&t));
    assert_eq!(narration::load(&t).unwrap().entries[0], line);

    // A roll moves the take and nothing else.
    let key = |e: &Entry| narration::tts_key(e, None, None);
    assert_ne!(key(&line), key(&rolled));
    assert_eq!(line.text, rolled.text);
    assert_eq!((line.s, line.e, line.at), (rolled.s, rolled.e, rolled.at));
}

#[test]
fn sec_01_project_and_files_4_narratenarrationjson_entries_are_sorted_by_s_then_at() {
    // A hand-edited time may reorder the list; everything downstream leans on playing
    // order — a line's window runs to the next entry.
    let (_root, t) = with_file(
        "sorted",
        Some(r#"{"entries":[
            {"s": 20.0, "e": 30.0, "text": "later", "emotion": ""},
            {"s": 1.5, "e": 10.0, "at": 3.0, "text": "same clip, second line", "emotion": ""},
            {"s": 1.5, "e": 10.0, "at": 1.0, "text": "same clip, first line", "emotion": ""}]}"#),
    );
    let n = narration::load(&t).unwrap();
    assert_eq!(
        n.entries.iter().map(|e| e.text.as_str()).collect::<Vec<_>>(),
        ["same clip, first line", "same clip, second line", "later"]
    );

    // And a save keeps that order, so the file on disk is in playing order too.
    narration::save(&n, &t).unwrap();
    let text = read(&t);
    assert!(text.find("first line").unwrap() < text.find("second line").unwrap(), "{text}");
    assert!(text.find("second line").unwrap() < text.find("\"later\"").unwrap(), "{text}");
    // Saving an out-of-order cut puts it back as well.
    let mut shuffled = n.clone();
    shuffled.entries.reverse();
    narration::save(&shuffled, &t).unwrap();
    assert_eq!(narration::load(&t).unwrap(), n);
}

#[test]
fn sec_01_project_and_files_4_narratenarrationjson_silent_clips_are_kept_by_bounds() {
    let n = Narration {
        entries: vec![entry(1.5, 34.7, "said")],
        silent: vec![Silent { s: 40.0, e: 65.0 }, Silent { s: 70.0, e: 72.5 }],
    };
    let (_root, t) = with_file("silent", None);
    narration::save(&n, &t).unwrap();

    // Bounds and nothing else — no asset, no lane, no rate.
    let text = read(&t);
    assert!(text.contains("\"silent\""), "{text}");
    assert!(text.contains("40.0") && text.contains("65.0"), "{text}");
    for key in ["\"src\"", "\"dur\"", "\"ins\"", "\"at\"", "\"lane\""] {
        assert!(!text.contains(key), "{key} written by a silent clip\n{text}");
    }
    assert_eq!(narration::load(&t).unwrap(), n);

    // Nothing marked silent is not said at all: the key goes.
    let plain = Narration { entries: vec![entry(1.5, 34.7, "said")], silent: vec![] };
    narration::save(&plain, &t).unwrap();
    assert!(!read(&t).contains("\"silent\""), "{}", read(&t));

    // A marker is about the clip, so a line on it is what tells the two apart.
    let both = Narration {
        entries: vec![entry(40.0, 65.0, "back again")],
        silent: vec![Silent { s: 40.0, e: 65.0 }],
    };
    assert!(both.is_silent(40.0, 65.0) && both.has_line(40.0, 65.0));
}

#[test]
fn sec_01_project_and_files_4_narratenarrationjson_the_tts_cache_key_is_stable_across_versions() {
    let line = Entry {
        s: 1.5,
        e: 34.7,
        text: "We start with …".to_string(),
        emotion: "calm".to_string(),
        ..Default::default()
    };
    // The whole point of the prefix: this exact string named this take before the voice
    // picker and the re-roll existed, and names it still.
    assert_eq!(narration::tts_key(&line, None, None), "25e0.85|We start with …|calm");
    // The project's own voice adds nothing, so a project narrated back then does not
    // re-speak every line the first time it opens.
    assert_eq!(narration::tts_key(&line, Some("own"), None), "25e0.85|We start with …|calm");
    // Any other voice must: a different speaker is not the old speaker's take.
    assert_eq!(
        narration::tts_key(&line, Some("aria"), None),
        "25e0.85|aria|We start with …|calm"
    );

    let mut rolled = line.clone();
    rolled.roll = 2;
    assert_eq!(narration::tts_key(&rolled, None, None), "25e0.85|2#We start with …|calm");
    // Both salts, each once, prefix first.
    assert_eq!(
        narration::tts_key(&rolled, Some("aria"), None),
        "25e0.85|2#aria|We start with …|calm"
    );

    // A vector replaces the tag in the key — a stronger blend is a different performance
    // of the same words — and is written to three decimals, since this string becomes a
    // filename.
    assert_eq!(
        narration::tts_key(&line, None, Some([0.0, 0.165, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0])),
        "25e0.85|We start with …|0,0.165,0,0,0,0,0,0"
    );
    // The tail a weight scales lands on such numbers; the rounding is what keeps one
    // take under one name.
    assert_eq!(
        narration::tts_key(&line, None, Some([0.16499999999999998, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0])),
        "25e0.85|We start with …|0.165,0,0,0,0,0,0,1"
    );
    // A negative axis keeps its sign; a zero one never grows a "-0".
    assert!(narration::tts_key(&line, None, Some([-0.25, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]))
        .ends_with("|-0.25,0,0,0,0,0,0,0"));

    // Nothing else about the line is in the key: where it sits is not how it sounds.
    let mut elsewhere = line.clone();
    elsewhere.s = 900.0;
    elsewhere.e = 901.0;
    elsewhere.at = 5.0;
    assert_eq!(narration::tts_key(&elsewhere, None, None), narration::tts_key(&line, None, None));
}

#[test]
fn sec_01_project_and_files_4_narratenarrationjson_the_cache_file_is_the_first_eight_sha1_bytes_and_the_seed_the_next_three() {
    // sha1("abc") = a9993e364706816aba3e25717850c26c9cd0d89d, computed with sha1sum.
    assert_eq!(narration::tts_file("abc"), "a9993e364706816a");
    // Bytes 8..12 of that digest, big-endian: ba3e2571.
    assert_eq!(narration::tts_seed("abc"), 0xba3e2571);
    assert_eq!(narration::tts_seed("abc"), 3_124_634_993);

    // The spec's own line, by key and by name: one filename, one seed.
    let line = Entry {
        s: 1.5,
        e: 34.7,
        text: "We start with …".to_string(),
        emotion: "calm".to_string(),
        ..Default::default()
    };
    let key = narration::tts_key(&line, None, None);
    assert_eq!(narration::tts_file(&key), "327ca7508680a158");
    assert_eq!(narration::tts_seed(&key), 954_416_494);

    // The cache is the point: one key, one name, however often it is asked.
    assert_eq!(narration::tts_file(&key), narration::tts_file(&narration::tts_key(&line, None, None)));
    assert_eq!(narration::tts_seed(&key), narration::tts_seed(&key));

    // And a re-roll moves both — same words, different draw.
    let mut rolled = line.clone();
    rolled.roll = 1;
    let rolled_key = narration::tts_key(&rolled, None, None);
    assert_ne!(narration::tts_file(&rolled_key), narration::tts_file(&key));
    assert_ne!(narration::tts_seed(&rolled_key), narration::tts_seed(&key));

    // A different voice likewise, without touching the old speaker's names.
    let aria = narration::tts_key(&line, Some("aria"), None);
    assert_ne!(narration::tts_file(&aria), narration::tts_file(&key));
}

#[test]
fn sec_01_project_and_files_4_narratenarrationjson_a_missing_file_is_no_narration_and_one_function_writes_it() {
    let (_root, t) = with_file("io", None);
    // No file yet: nothing was said, which is not an error — and no narrate/ directory.
    assert_eq!(narration::load(&t).unwrap(), Narration::default());
    assert!(!t.narration_json().exists());

    let n = Narration { entries: vec![entry(1.5, 34.7, "We start with …")], silent: vec![] };
    narration::save(&n, &t).unwrap();
    assert_eq!(narration::load(&t).unwrap(), n);

    // §1: files are 0644, and the directory was made by the one writer.
    let mode = std::fs::metadata(t.narration_json()).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o644);

    // An empty narration still says so.
    narration::save(&Narration::default(), &t).unwrap();
    assert!(read(&t).contains("\"entries\": []"), "{}", read(&t));

    // A file that is not JSON is an error naming the path, never a silent empty narration.
    std::fs::write(t.narration_json(), "{ not json").unwrap();
    let err = narration::load(&t).unwrap_err();
    assert!(err.contains("narration.json"), "{err}");

    // Byte-stable, which is what lets §2's autosave skip a write that changed nothing.
    let n = Narration {
        entries: vec![entry(1.5, 34.7, "one"), entry(40.0, 65.0, "two")],
        silent: vec![Silent { s: 70.0, e: 72.5 }],
    };
    narration::save(&n, &t).unwrap();
    let first = read(&t);
    narration::save(&n, &t).unwrap();
    assert_eq!(read(&t), first);
}
