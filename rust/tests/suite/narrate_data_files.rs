//! §07-narrate#3-data — the files in `narrate/` besides the record: the previous generation,
//! the two one-line settings, the take set, the two reference wavs and the sample cache. One
//! test per file in §3's list, so every name the spec spells is exercised through the app's own
//! accessors.

use std::path::{Path, PathBuf};

use naivepost::narrate_data::{
    self, base_reference, clean_takes, drop_reference, keep_previous, load_takes, previous,
    read_pitch, read_voice, reference_needs_shift, sample_hex, sample_path, sanitize_voice_id,
    save_takes, served_reference, take_base, write_pitch, write_voice, DATA_FILES, OWN_VOICE, Take,
};
use naivepost::narrate_screen::{self, CAPTIONS};
use naivepost::narration::{self, Entry, Narration};
use naivepost::layout::Tree;

fn tree(tag: &str) -> (PathBuf, Tree) {
    let root = std::env::temp_dir().join(format!(
        "naivepost-narratedata-{tag}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    let dir = root.join("demo.naivepost");
    std::fs::create_dir_all(&dir).unwrap();
    (root, Tree::new(&dir).unwrap())
}

fn name(path: &Path) -> String {
    path.file_name().unwrap().to_string_lossy().into_owned()
}

fn entry(s: f64, text: &str) -> Entry {
    Entry {
        s,
        e: s + 10.0,
        text: text.into(),
        ..Default::default()
    }
}

/// §07-narrate#3-data S1 — the folder holds exactly the files the spec lists.
#[test]
fn sec_07_narrate_3_data_s1_the_folder_holds_exactly_the_files_the_spec_lists() {
    let (root, tree) = tree("s1");

    // §3's own list, in §3's order.
    assert_eq!(DATA_FILES.len(), 9);
    assert_eq!(
        DATA_FILES,
        [
            "narration.json",
            "narration.prev.json",
            "voice.txt",
            "pitch.txt",
            "takes.json",
            "voice_ref_base.wav",
            "voice_ref.wav",
            "tts/*.wav",
            "samples/*.wav",
        ]
    );

    // Every one of them is spelled the same way by the app's accessors, so a rename in
    // crate::layout fails here rather than turning up as a missing file later.
    let files = [
        name(&tree.narration_json()),
        name(&tree.narration_prev_json()),
        name(&tree.voice_txt()),
        name(&tree.pitch_txt()),
        name(&tree.takes_json()),
        name(&tree.voice_ref_base_wav()),
        name(&tree.voice_ref_wav()),
    ];
    assert_eq!(&files[..7], &DATA_FILES[..7]);

    // The two globs: the directory each one lives in, named by the same accessors. §3 spells
    // them `tts/*.wav`, so the pattern is rebuilt from the folder name plus that suffix.
    let tts = tree.tts_wav("0123456789abcdef");
    let sample = tree.sample_wav("aria", "a1b2c3d4e5f6");
    assert_eq!(
        format!("{}/*.wav", name(tts.parent().unwrap())),
        DATA_FILES[7]
    );
    assert_eq!(
        format!("{}/*.wav", name(sample.parent().unwrap())),
        DATA_FILES[8]
    );

    let _ = std::fs::remove_dir_all(&root);
}

/// §07-narrate#3-data S2 — narration.prev.json is one generation deep.
#[test]
fn sec_07_narrate_3_data_s2_the_previous_generation_is_one_deep_and_its_copy_never_moves() {
    let (root, tree) = tree("s2");

    // Nothing to keep yet, and that is not a failure: no file appears.
    assert_eq!(keep_previous(&tree), Ok(false));
    assert!(!tree.narration_prev_json().exists());

    let first = Narration {
        entries: vec![entry(1.5, "we open on the desktop")],
        silent: vec![],
    };
    narration::save(&first, &tree).unwrap();
    let record_before = std::fs::read(tree.narration_json()).unwrap();

    assert_eq!(keep_previous(&tree), Ok(true));
    assert_eq!(
        std::fs::read(tree.narration_prev_json()).unwrap(),
        std::fs::read(tree.narration_json()).unwrap()
    );
    // The copy never moves the record it was made from.
    assert_eq!(std::fs::read(tree.narration_json()).unwrap(), record_before);

    let second = Narration {
        entries: vec![entry(1.5, "we open on the desktop"), entry(20.0, "then the menu")],
        silent: vec![],
    };
    narration::save(&second, &tree).unwrap();
    assert_eq!(keep_previous(&tree), Ok(true));

    // One generation: prev now holds what the SECOND run overwrote — two lines, not one — and
    // nothing older is left anywhere in the folder.
    let kept = previous(&tree).expect("the generation before last");
    assert_eq!(kept.entries.len(), 2);
    assert_eq!(kept.entries[1].text, "then the menu");
    let prev_files: Vec<String> = std::fs::read_dir(tree.dir().join("narrate"))
        .unwrap()
        .filter_map(|row| row.ok())
        .map(|row| name(&row.path()))
        .filter(|file| file.ends_with(".prev.json"))
        .collect();
    assert_eq!(prev_files, vec!["narration.prev.json".to_string()]);

    // A half-written previous generation must never fail the run that is about to overwrite.
    // Writing it directly needs the folder, which `narration::save` is what creates.
    std::fs::write(tree.narration_prev_json(), b"not json").unwrap();
    assert!(previous(&tree).is_none());

    // A tree that was never narrated has no previous generation at all.
    let (bare_root, bare_tree) = {
        let root = std::env::temp_dir().join(format!(
            "naivepost-narratedata-s2-bare-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let dir = root.join("demo.naivepost");
        std::fs::create_dir_all(&dir).unwrap();
        (root, Tree::new(&dir).unwrap())
    };
    assert!(previous(&bare_tree).is_none());
    let _ = std::fs::remove_dir_all(&bare_root);
    let _ = std::fs::remove_dir_all(&root);
}

/// §07-narrate#3-data S3 — voice.txt holds the id, and reads back trimmed.
#[test]
fn sec_07_narrate_3_data_s3_voice_txt_holds_the_id_and_reads_back_trimmed() {
    let (root, tree) = tree("s3");

    // A project that never picked: the session's own narrator.
    assert_eq!(read_voice(&tree), OWN_VOICE);
    assert_eq!(OWN_VOICE, "own");

    write_voice(&tree, "aria").unwrap();
    assert_eq!(std::fs::read_to_string(tree.voice_txt()).unwrap(), "aria");
    assert_eq!(read_voice(&tree), "aria");

    // A hand-edited file still names the same voice.
    std::fs::write(tree.voice_txt(), "  aria  \n").unwrap();
    assert_eq!(read_voice(&tree), "aria");
    std::fs::write(tree.voice_txt(), "   \n").unwrap();
    assert_eq!(read_voice(&tree), OWN_VOICE, "blank means nobody picked");

    // The id that short-circuits everything that speaks round-trips too.
    write_voice(&tree, CAPTIONS).unwrap();
    assert_eq!(read_voice(&tree), CAPTIONS);
    assert_eq!(CAPTIONS, "captions");
    assert_eq!(name(&tree.voice_txt()), "voice.txt");

    let _ = std::fs::remove_dir_all(&root);
}

/// §07-narrate#3-data S4 — an id that is not a file name is made into one.
#[test]
fn sec_07_narrate_3_data_s4_an_id_that_is_not_a_file_name_is_made_into_one() {
    // The id is pasted into samples/<id>_<hash>.wav and read back out of voice.txt, so a slash
    // would write somewhere else entirely.
    let escaped = sanitize_voice_id("../../etc/passwd");
    assert!(!escaped.contains('/'), "{escaped} still has a path separator");
    assert_eq!(Path::new(&escaped).parent().unwrap(), Path::new(""));

    assert_eq!(sanitize_voice_id("///"), "voice");
    assert_eq!(sanitize_voice_id("-._-._"), "voice");
    assert_eq!(sanitize_voice_id(""), "voice");

    // Letters stay, the space between them becomes a separator.
    assert_eq!(sanitize_voice_id("Ana María"), "Ana-Mar-a");
    assert_eq!(sanitize_voice_id("aria_2.wav"), "aria_2.wav", "a clean id is left alone");
}

/// §07-narrate#3-data S5 — pitch.txt: one decimal, clamped to half an octave.
// P.eng.pitchRangeSemitones
#[test]
fn sec_07_narrate_3_data_s5_pitch_is_one_decimal_clamped_to_half_an_octave() {
    let (root, tree) = tree("s5");

    assert_eq!(read_pitch(&tree), 0.0, "absent means unshifted");
    // The folder is the record's to create (§1's spine); a settings file is never written first.
    narration::save(&Narration::default(), &tree).unwrap();
    std::fs::write(tree.pitch_txt(), b"nonsense").unwrap();
    assert_eq!(read_pitch(&tree), 0.0, "unparsable is not a shift");

    write_pitch(&tree, 2.25).unwrap();
    let text = std::fs::read_to_string(tree.pitch_txt()).unwrap();
    let (_, fraction) = text.split_once('.').expect("one decimal written");
    assert_eq!(fraction.len(), 1, "`{text}` is not one decimal place");
    // What survives the round trip is the written value clamped, not the number asked for: the
    // file holds one decimal, so a quarter tone is rounded to the nearest half.
    assert_eq!(read_pitch(&tree), 2.2);

    // A stored value outside the slider's ends still means "shift it that way", up to the ends.
    std::fs::write(tree.pitch_txt(), b"99").unwrap();
    assert_eq!(read_pitch(&tree), narrate_screen::PITCH_MAX_SEMITONES);
    std::fs::write(tree.pitch_txt(), b"-99").unwrap();
    assert_eq!(read_pitch(&tree), narrate_screen::PITCH_MIN_SEMITONES);
    assert_eq!(name(&tree.pitch_txt()), "pitch.txt");

    let _ = std::fs::remove_dir_all(&root);
}

/// §07-narrate#3-data S6 — takes.json is keyed by the recording and cleaned on the way in.
// P.eng.takeMinSeconds
#[test]
fn sec_07_narrate_3_data_s6_takes_are_keyed_by_the_recording_and_cleaned_on_the_way_in() {
    let (root, tree) = tree("s6");
    assert!(load_takes(&tree).unwrap().is_empty(), "no file is no takes");

    save_takes(
        &tree,
        "session.mp4",
        &[
            Take { s: 4.0, e: 7.0 },
            Take { s: 40.0, e: 40.05 },
        ],
    )
    .unwrap();
    let text = std::fs::read_to_string(tree.takes_json()).unwrap();
    assert!(text.contains("4.0"));
    // The 50 ms take never reaches the file — P.eng.takeMinSeconds is applied on the way in.
    assert!(!text.contains("40.05"), "a take below the floor was stored: {text}");

    // A second recording keeps the first's key; takes belong to the recording, not the slot.
    save_takes(&tree, "second.mp4", &[Take { s: 1.0, e: 9.0 }]).unwrap();
    let all = load_takes(&tree).unwrap();
    assert_eq!(all.len(), 2);
    assert_eq!(all["session.mp4"], vec![Take { s: 4.0, e: 7.0 }]);
    assert_eq!(take_base(Path::new("/x/session.mp4")), "session");

    // An empty set is no takes, not a take of no seconds — the key goes, the other stays.
    save_takes(&tree, "session.mp4", &[Take { s: 2.0, e: 2.1 }]).unwrap();
    let all = load_takes(&tree).unwrap();
    assert_eq!(all.len(), 1);
    assert!(all.get("session.mp4").is_none());
    assert_eq!(all["second.mp4"], vec![Take { s: 1.0, e: 9.0 }]);

    // The clean-up itself: sorted, merged, and nothing before the recording starts.
    let cleaned = clean_takes(&[
        Take { s: 3.0, e: 6.0 },
        Take { s: 1.0, e: 4.0 },
        Take { s: -5.0, e: 2.0 },
    ]);
    assert_eq!(cleaned, vec![Take { s: 1.0, e: 6.0 }]);
    assert_eq!(name(&tree.takes_json()), "takes.json");

    let _ = std::fs::remove_dir_all(&root);
}

/// §07-narrate#3-data S7 — the reference the server reads is the one that goes.
#[test]
fn sec_07_narrate_3_data_s7_the_reference_the_server_reads_is_the_one_that_goes() {
    let (root, tree) = tree("s7");
    assert_eq!(name(&served_reference(&tree)), "voice_ref.wav");
    assert_eq!(name(&base_reference(&tree)), "voice_ref_base.wav");

    std::fs::create_dir_all(tree.dir().join("narrate")).unwrap();
    let seed = |tree: &Tree| {
        std::fs::write(base_reference(tree), b"RIFFbase").unwrap();
        std::fs::write(served_reference(tree), b"RIFFshifted").unwrap();
    };

    // The pitch slider's move: the shifted copy is stale, the base is still the person.
    seed(&tree);
    assert_eq!(drop_reference(&tree, true), 1);
    assert!(!served_reference(&tree).exists());
    assert!(base_reference(&tree).exists());
    assert!(reference_needs_shift(&base_reference(&tree), &served_reference(&tree)));

    // Nothing there to drop is not a failure and counts as nothing dropped.
    assert_eq!(drop_reference(&tree, true), 0);

    // A take set or a slot voice makes the base itself wrong.
    assert_eq!(drop_reference(&tree, false), 1);
    assert!(!base_reference(&tree).exists());
    assert!(!reference_needs_shift(&base_reference(&tree), &served_reference(&tree)));

    // With both files present the server is already reading the copy it wants.
    seed(&tree);
    assert!(!reference_needs_shift(&base_reference(&tree), &served_reference(&tree)));

    let _ = std::fs::remove_dir_all(&root);
}

/// §07-narrate#3-data S8 — samples are named by voice and the hash of their key.
#[test]
fn sec_07_narrate_3_data_s8_samples_are_named_by_voice_and_the_hash_of_their_key() {
    let hex = sample_hex("hello");
    assert_eq!(hex.len(), 12, "six bytes of digest as hex");
    assert!(
        hex.chars().all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)),
        "{hex} is not lowercase hex"
    );
    assert_eq!(sample_hex("hello"), hex, "the same key names the same file");
    assert_ne!(sample_hex("hello."), hex);

    // The take cache's name is the same digest cut to eight bytes: assert the shared prefix
    // rather than a hard-coded digest.
    assert!(narration::tts_file("hello").starts_with(&hex));

    let (root, tree) = tree("s8");
    let path = sample_path(&tree, "aria", "hello");
    assert_eq!(path, tree.sample_wav("aria", &sample_hex("hello")));
    assert_eq!(name(&path), format!("aria_{hex}.wav"));
    assert_eq!(name(path.parent().unwrap()), "samples");

    let _ = std::fs::remove_dir_all(&root);
}

/// §07-narrate#3-data S9 — the record and its cache key stay what crate::narration owns.
#[test]
fn sec_07_narrate_3_data_s9_the_record_and_its_cache_key_stay_what_narration_owns() {
    let (root, tree) = tree("s9");

    // Written out of order, saved through crate::narration, kept by keep_previous as it stands.
    let record = Narration {
        entries: vec![entry(20.0, "then the menu"), entry(1.5, "we open on the desktop")],
        silent: vec![],
    };
    narration::save(&record, &tree).unwrap();
    assert_eq!(keep_previous(&tree), Ok(true));

    let kept = previous(&tree).expect("the copy of a saved record");
    assert_eq!(kept.entries[0].text, "we open on the desktop", "sorted by (s, at)");
    assert_eq!(
        std::fs::read(tree.narration_prev_json()).unwrap(),
        std::fs::read(tree.narration_json()).unwrap()
    );

    // And a line re-read through this module's folder still names its take the same way: one
    // implementation for §3's tts/*.wav and §01 §4, not two.
    let line = narration::load(&tree).unwrap().entries.remove(0);
    let key = narration::tts_key(&line, None, None);
    assert!(key.starts_with("25e0.85|"), "{key} is not the shipped key shape");
    let file = narration::tts_file(&key);
    assert_eq!(file.len(), 16);
    assert_eq!(sample_hex(&key)[..], file[..12], "one digest, cut twice");
    assert!(narration::tts_seed(&key) > 0 || narration::tts_seed(&key) == 0);
    assert_eq!(
        tree.tts_wav(&file),
        tree.dir().join("narrate/tts").join(format!("{file}.wav"))
    );

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn sec_07_narrate_3_data_s10_the_module_adds_no_path_of_its_own() {
    // Every path this module hands back came out of the Tree it was given, so §1's folder
    // layout stays crate::layout's alone.
    let (root, tree) = tree("s10");
    let dir = tree.dir();
    for path in [
        base_reference(&tree),
        served_reference(&tree),
        sample_path(&tree, "aria", "hello"),
        tree.takes_json(),
        tree.voice_txt(),
        tree.pitch_txt(),
        tree.narration_prev_json(),
    ] {
        assert!(path.starts_with(dir), "{} outside the project", path.display());
    }
    // The write helpers land in the same place their readers look.
    write_voice(&tree, "aria").unwrap();
    write_pitch(&tree, 1.5).unwrap();
    save_takes(&tree, "session.mp4", &[Take { s: 0.0, e: 5.0 }]).unwrap();
    assert!(tree.voice_txt().exists() && tree.pitch_txt().exists() && tree.takes_json().exists());
    assert_eq!(read_voice(&tree), "aria");
    assert_eq!(read_pitch(&tree), 1.5);
    assert_eq!(load_takes(&tree).unwrap().len(), 1);
    let _ = narrate_data::OWN_VOICE;

    let _ = std::fs::remove_dir_all(&root);
}
