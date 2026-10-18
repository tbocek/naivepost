//! F4.6 Choose the voice and build the reference — `spec/07-narrate.md`, S1–S5 as plain rules.
//!
//! Each step's branch is asserted here without a display: what a picked voice resolves to, what the
//! automatic pick may take from diarized turns, what ffmpeg is asked for, which wav header the speech
//! server can read, and when the built reference is thrown away. The page's own wire is proven in
//! `voice_ref_wire_widgets.rs`.

use naivepost::narrate_data;
use naivepost::narrate_screen::CAPTIONS;
use naivepost::voice_ref::{self, Piece, ReferencePlan, VoiceChoice};

#[allow(dead_code)] // every test binary compiles this whole module; a helper it does not call is not a warning here
mod common;

fn files(names: &[&str]) -> Vec<String> {
    names.iter().map(|n| n.to_string()).collect()
}

// --- S1: the three kinds of answer, and the two refusals -------------------------------------

#[test]
fn f4_6_s1_captions_only_speaks_nothing_and_reports_the_switch() {
    let choice = voice_ref::pick_voice(CAPTIONS, 2, &files(&["warm.wav"]), "/voices");
    assert_eq!(
        choice,
        VoiceChoice::Silent(voice_ref::SILENT_SWITCH.to_string()),
        "captions is not a refusal and not a voice: it is said as 'written and timed, never spoken'"
    );
    assert!(
        voice_ref::SILENT_SWITCH.contains("never spoken"),
        "the sentence says plainly that nothing will be spoken"
    );
}

#[test]
fn f4_6_s1_an_untagged_narrator_names_the_prepare_step() {
    // Two slots have recordings; a third was remembered from an older project.
    let choice = voice_ref::pick_voice("narrator3", 2, &files(&[]), "/voices");
    let VoiceChoice::Refused(why) = choice else {
        panic!("an untagged slot must refuse, got {choice:?}");
    };
    assert_eq!(
        why,
        "narrator 3 is not tagged on the Prepare step \u{2014} tag a recording, or pick another voice",
        "S1's refusal is printed verbatim, naming where to fix it"
    );
}

#[test]
fn f4_6_s1_a_vanished_voice_file_names_its_folder() {
    let choice = voice_ref::pick_voice("gone", 1, &files(&["warm.wav"]), "/mnt/models/audiocpp/voices");
    let VoiceChoice::Refused(why) = choice else {
        panic!("a file that is not in the folder must refuse, got {choice:?}");
    };
    assert_eq!(
        why,
        "voice gone is no longer in /mnt/models/audiocpp/voices \u{2014} pick another",
        "the refusal names the folder it looked in, so the person can go look too"
    );
}

#[test]
fn f4_6_s1_a_tagged_narrator_and_a_folder_file_are_both_accepted_with_their_own_report() {
    assert_eq!(
        voice_ref::pick_voice("narrator2", 2, &files(&[]), "/voices"),
        VoiceChoice::Chosen {
            id: "narrator2".into(),
            said: voice_ref::narrator_switch(2),
        },
        "a tagged slot is chosen and reported as re-cut from the recording"
    );
    assert_eq!(
        voice_ref::pick_voice("warm.wav", 1, &files(&["warm.wav"]), "/voices"),
        VoiceChoice::Chosen {
            id: "warm.wav".into(),
            said: voice_ref::file_switch("warm.wav"),
        },
        "a voices-folder file is chosen and reported with the sample ▶ that plays it"
    );
    assert!(voice_ref::narrator_switch(1).contains("re-cut from the recording"));
    assert!(voice_ref::file_switch("x").contains("\u{25b6} beside the sample"));
}

#[test]
fn f4_6_s1_narrator_slot_ids_parse_and_a_file_name_does_not() {
    assert_eq!(voice_ref::narrator_slot("narrator1"), Some(1));
    assert_eq!(voice_ref::narrator_slot("narrator4"), Some(4));
    assert_eq!(voice_ref::narrator_slot("narrator0"), Some(0));
    assert_eq!(voice_ref::narrator_slot("narratorX"), None);
    assert_eq!(voice_ref::narrator_slot("warm.wav"), None);
    // Slot 0 is not a slot the picker offers, so it refuses rather than choosing nothing.
    assert!(matches!(
        voice_ref::pick_voice("narrator0", 2, &files(&[]), "/v"),
        VoiceChoice::Refused(_)
    ));
}

#[test]
fn f4_6_s1_switching_keeps_the_synthesized_audio_of_the_old_voice() {
    // The switch keeps what was synthesized because the KEY carries the voice: the same line under two
    // voices lands on two different cache paths, so neither overwrites the other and both stay playable.
    use naivepost::narration::{tts_key, Entry};
    let entry = Entry {
        s: 0.0,
        e: 8.0,
        at: 0.5,
        text: "The chain moved to proof of stake.".into(),
        emotion: String::new(),
        pos: String::new(),
        roll: 0,
    };
    let before = tts_key(&entry, Some("narrator1"), None);
    let after = tts_key(&entry, Some("narrator2"), None);
    assert_ne!(before, after, "the voice is inside the key, so a new voice cannot clobber the old take");
    // And the sample cache is keyed the same way by name.
    let root = std::env::temp_dir().join(format!("np-f46-key-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("session.naivepost")).expect("session");
    let tree = naivepost::layout::Tree::new(&root.join("session.naivepost")).expect("tree");
    let one = narrate_data::sample_path(&tree, "narrator1", &before);
    let two = narrate_data::sample_path(&tree, "narrator2", &after);
    assert_ne!(one, two, "two voices get two sample files");
    assert_eq!(
        one,
        narrate_data::sample_path(&tree, "narrator1", &before),
        "the same voice and text name the same file, which is what makes the cache a cache"
    );
    let _ = std::fs::remove_dir_all(&root);
}

// --- S2: hand-picked wins; otherwise the floors, the gap, the cap ---------------------------

#[test]
fn f4_6_s2_hand_picked_takes_win_without_diarization() {
    // No turns at all: a hand-picked take still builds, because S2 says the takes win outright.
    let (count, span) = (Words::flat(0, None).counter(), Words::flat(0, None).spanner());
    let plan = voice_ref::plan_reference(&[(4.0, 12.0)], &[], &count, &span, 1);
    assert_eq!(
        plan,
        ReferencePlan::HandPicked(vec![Piece { start: 4.0, end: 12.0 }]),
        "hand-picked takes need no diarization and no cap"
    );
    // Even one far past the wanted length: the cap is the AUTOMATIC pick's, not the person's.
    let long = voice_ref::plan_reference(&[(0.0, 90.0)], &[], &count, &span, 1);
    match long {
        ReferencePlan::HandPicked(pieces) => {
            assert_eq!(pieces.iter().map(Piece::seconds).sum::<f64>(), 90.0);
        }
        other => panic!("a hand-picked take must never be capped or refused, got {other:?}"),
    }
}

#[test]
fn f4_6_s2_automatic_pick_respects_the_five_second_floor_the_two_second_gap_and_1_5_words_per_second()
{
    // P.eng.refMinTakeSeconds 5.0: a 4 s spoken span is too short, a 6 s one passes.
    let words = Words::flat(40, Some((0.0, 4.0)));
    let short = voice_ref::plan_reference(&[], &[(0.0, 4.0, 1)], &words.counter(), &words.spanner(), 1);
    assert_eq!(
        short,
        ReferencePlan::Unavailable(voice_ref::NO_CLEAN_STRETCH.to_string()),
        "P.eng.refMinTakeSeconds: below the floor there is nothing clean to take"
    );
    let dense = Words::flat(30, Some((0.0, 6.0)));
    let ok = voice_ref::plan_reference(&[], &[(0.0, 6.0, 1)], &dense.counter(), &dense.spanner(), 1);
    assert_eq!(
        ok,
        ReferencePlan::Automatic(vec![Piece { start: 0.0, end: 6.0 }]),
        "a solo turn over the floor with enough words is taken whole"
    );

    // Other-speaker clearance 2 s: somebody else talking within 2 s of the window disqualifies it...
    let crowded = voice_ref::plan_reference(
        &[],
        &[(0.0, 6.0, 1), (7.0, 9.0, 2)],
        &dense.counter(),
        &dense.spanner(),
        1,
    );
    assert_eq!(
        crowded,
        ReferencePlan::Unavailable(voice_ref::NO_CLEAN_STRETCH.to_string()),
        "1 s of clearance is inside the 2 s the spec asks for, so the stretch is not solo"
    );
    // ...while 2 s exactly clears it.
    let clear = voice_ref::plan_reference(&[], &[(0.0, 6.0, 1), (8.0, 9.0, 2)], &dense.counter(), &dense.spanner(), 1);
    assert_eq!(
        clear,
        ReferencePlan::Automatic(vec![Piece { start: 0.0, end: 6.0 }]),
        "at exactly REF_PAD_SECONDS the neighbour is outside the window's guard"
    );

    // P.eng.refMinWordsPerSecond 1.5: 6 s needs 9 words; 8 is too sparse to clone from — but §6 drops a
    // thin take only when that leaves SOME candidate, so with nothing else on offer the thin one is kept.
    let thin = Words::flat(8, Some((0.0, 6.0)));
    let sparse = voice_ref::plan_reference(&[], &[(0.0, 6.0, 1)], &thin.counter(), &thin.spanner(), 1);
    assert_eq!(
        sparse,
        ReferencePlan::Automatic(vec![Piece { start: 0.0, end: 6.0 }]),
        "a thin take is dropped when anything denser competes, and kept rather than refused when it is all \
         the diarization offers"
    );
    assert_eq!(
        voice_ref::REF_MIN_TAKE_SECONDS, 5.0,
        "P.eng.refMinTakeSeconds spells 5"
    );
    assert_eq!(
        voice_ref::REF_MIN_WORDS_PER_SECOND, 1.5,
        "P.eng.refMinWordsPerSecond spells 1.5"
    );
}

#[test]
fn f4_6_s2_the_pick_stops_at_fourteen_seconds_and_three_pieces() {
    // Four long solo turns: the cap is REF_TAKE_MAX 3 pieces, and the wanted length is 14 s.
    let turns = [
        (0.0, 8.0, 1u32),
        (20.0, 28.0, 1),
        (40.0, 48.0, 1),
        (60.0, 68.0, 1),
    ];
    let words = Words::flat(40, None);
    let plan = voice_ref::plan_reference(&[], &turns, &words.counter(), &words.spanner(), 1);
    let ReferencePlan::Automatic(pieces) = plan else {
        panic!("four clean turns must produce an automatic plan, got {plan:?}");
    };
    assert!(
        pieces.len() <= voice_ref::REF_TAKE_MAX,
        "P.machine.refTakeMax 3: never stitch more than three pieces, got {}",
        pieces.len()
    );
    let total: f64 = pieces.iter().map(Piece::seconds).sum();
    assert!(
        total <= voice_ref::REF_WANT_SECONDS + 8.0,
        "P.machine.refWantSeconds 14 is the aim; overshooting by one piece is how the aim is met, \
         but four pieces' worth (32 s) would not be: {total}"
    );
    assert_eq!(voice_ref::REF_TAKE_MAX, 3, "P.machine.refTakeMax spells 3");
    assert_eq!(
        voice_ref::REF_WANT_SECONDS, 14.0,
        "P.machine.refWantSeconds spells 14, shared with the status line's 'plenty' mark"
    );
    // Returned in playing order, not in longest-first order.
    let mut sorted = pieces.clone();
    sorted.sort_by(|a, b| a.start.total_cmp(&b.start));
    assert_eq!(pieces, sorted, "the pieces come back in time order so the cut reads forwards");
}

#[test]
fn f4_6_s2_no_diarization_is_said_as_run_prepare_or_pick_by_hand() {
    let words = Words::flat(0, None);
    let plan = voice_ref::plan_reference(&[], &[], &words.counter(), &words.spanner(), 1);
    assert_eq!(
        plan,
        ReferencePlan::Unavailable(
            "no diarization for X -- run Prepare, or pick the seconds by hand under the video".to_string()
        ),
        "S2's no-diarization refusal names both ways out"
    );
    assert_eq!(
        voice_ref::nothing_tagged(2),
        "nothing is tagged as narrator 2 on the Prepare step",
        "and the nothing-tagged sentence names the slot"
    );
}

#[test]
fn f4_6_s2_no_clean_solo_stretch_is_said() {
    // Diarization exists; every turn fails a floor.
    let turns = [(0.0, 2.0, 1u32), (3.0, 4.0, 2)];
    let words = Words::flat(100, None);
    let plan = voice_ref::plan_reference(&[], &turns, &words.counter(), &words.spanner(), 1);
    assert_eq!(
        plan,
        ReferencePlan::Unavailable(voice_ref::NO_CLEAN_STRETCH.to_string()),
        "\"no clean solo stretch found\" is what a diarized-but-unusable session gets"
    );
}

// --- S2's ranking: words said, length only as the tie-break (§07-narrate §4's reference line) ----------

/// §07-narrate#4-parameters-used — P.eng.refPadSeconds is read by the solo test itself: a neighbour 1.9 s
/// away sits inside the pad and disqualifies the stretch, while one at exactly 2.0 s does not.
#[test]
fn sec_07_narrate_4_parameters_used_ref_pad_seconds_guards_the_solo_window()
{
    assert_eq!(voice_ref::REF_PAD_SECONDS, 2.0, "P.eng.refPadSeconds spells 2");
    // 1.9 s of clearance: inside the pad, so nobody can say the window is solo.
    let words = Words::flat(30, Some((0.0, 6.0)));
    let crowded = voice_ref::plan_reference(
        &[],
        &[(0.0, 6.0, 1u32), (7.9, 9.0, 2)],
        &words.counter(),
        &words.spanner(),
        1,
    );
    assert_eq!(
        crowded,
        ReferencePlan::Unavailable(voice_ref::NO_CLEAN_STRETCH.to_string()),
        "1.9 s is inside P.eng.refPadSeconds, so the take would carry the other speaker's edge"
    );
    // Exactly 2.0 s: outside it, and the same turn is usable.
    let clear = voice_ref::plan_reference(
        &[],
        &[(0.0, 6.0, 1u32), (8.0, 9.0, 2)],
        &words.counter(),
        &words.spanner(),
        1,
    );
    assert_eq!(
        clear,
        ReferencePlan::Automatic(vec![Piece { start: 0.0, end: 6.0 }]),
        "the pad is inclusive at its own value"
    );
}

/// §07-narrate#4-parameters-used — the automatic ranking goes by the WORDS said in each solo stretch, with
/// that stretch's own length breaking a tie. Both candidates here clear `refMinWordsPerSecond`, so the floor
/// cannot be what decides between them.
#[test]
fn sec_07_narrate_4_parameters_used_ref_ranking_is_words_said_with_length_as_tie_break()
{
    // A 6 s turn with 30 words against an 8 s turn with 15: both are dense enough (8 s needs 12), but the
    // shorter one says twice as much, so it is the FIRST piece taken. The answer is re-sorted into time
    // order before it comes back (§F4.6's cut reads forwards), so what proves the ranking here is which of
    // the two a cap of ONE keeps — the same rule, seen where the order cannot hide it.
    let turns = [(0.0, 8.0, 1u32), (20.0, 26.0, 1)];
    // The denser turn is the SHORT one: words decide, and each stretch keeps its own span.
    let words = Words::flat(15, None);
    let counter = words.per_turn(&[(20.0, 30)]);
    let plan = voice_ref::plan_reference(&[], &turns, &counter, &words.spanner(), 1);
    let ReferencePlan::Automatic(pieces) = plan else {
        panic!("two clean turns must produce a plan, got {plan:?}");
    };
    assert_eq!(
        pieces.len(),
        2,
        "neither turn reaches P.machine.refWantSeconds alone, so both are offered: {pieces:?}"
    );
    let wanted = voice_ref::REF_WANT_SECONDS;
    // Both stretches are longer than the wanted length on their own, so whichever ranks FIRST is the only
    // piece the loop takes: the stop happens before a second piece can be offered.
    let dense_span = |from: f64, until: f64| Some((from, until));
    let counter = Words::flat(15, None).per_turn(&[(wanted + 20.0, 30)]);
    let capped = voice_ref::plan_reference(
        &[],
        &[
            (0.0, wanted + 4.0, 1u32),
            (wanted + 20.0, wanted + 26.0, 1),
        ],
        &counter,
        &dense_span,
        1,
    );
    let ReferencePlan::Automatic(capped) = capped else {
        panic!("two clean turns must produce a plan, got {capped:?}");
    };
    assert_eq!(
        capped,
        vec![Piece { start: wanted + 20.0, end: wanted + 26.0 }],
        "the denser 6 s turn is taken first even though it is the shorter one, so it is the one a stop at \
         the wanted length keeps"
    );

    // Equal word counts fall back to length, and the tie is proved by what a stop at the wanted length
    // KEEPS: three solo turns all saying 27 words — an 18 s one that alone passes P.machine.refWantSeconds,
    // a 6 s one that would win if length were still the ranking, and a 14 s one. If the short turn ranked
    // first it would be the only piece taken; instead the 18 s turn does.
    let words = Words::flat(27, None);
    let tied = voice_ref::plan_reference(
        &[],
        &[(0.0, 6.0, 1u32), (20.0, 38.0, 1), (60.0, 74.0, 1)],
        &words.counter(),
        &words.spanner(),
        1,
    );
    let ReferencePlan::Automatic(pieces) = tied else {
        panic!("three clean turns must produce a plan, got {tied:?}");
    };
    assert_eq!(
        pieces,
        vec![Piece { start: 20.0, end: 38.0 }],
        "equal word counts, so length decides: the 18 s turn outranks the 6 s one and is the only piece \
         needed to reach the wanted length"
    );
}

// --- S3: the level-and-shift leg, the header rule, the rename ------------------------------------------

#[test]
fn f4_6_s3_the_base_is_leveled_to_minus_16_lufs_and_written_mono_48k() {
    let args = voice_ref::level_args("/tmp/take.wav", "/tmp/base.wav");
    let joined = args.join(" ");
    assert!(
        joined.contains("loudnorm=I=-16:TP=-1.5:LRA=7"),
        "the filter is §10's P.eng.refLoudness spelled straight through: {joined}"
    );
    assert!(joined.contains("-ac 1"), "mono, because a clone wants one voice not a stereo field");
    assert!(joined.contains("-ar 48000"), "48 kHz, the rate the server accepts");
    assert!(joined.contains("pcm_s16le"), "plain pcm, not a compressed codec");
    assert!(joined.contains("-vn"), "any video stream in the source is dropped");
    assert_eq!(args.last().map(String::as_str), Some("/tmp/base.wav"));
}

#[test]
fn f4_6_s3_the_pitch_shift_keeps_the_formants() {
    let args = voice_ref::shift_args("/tmp/base.wav", "/tmp/served.wav", 3.0);
    let joined = args.join(" ");
    assert!(
        joined.contains("rubberband=") && joined.contains("formant=y"),
        "formants preserved: a shift is a different speaker, not the same one transposed: {joined}"
    );
    // +3 semitones is 2^(3/12) ~= 1.189207.
    assert!(
        joined.contains("pitch=1.189207"),
        "the slider's semitones become rubberband's linear ratio: {joined}"
    );
    let down = voice_ref::shift_args("/tmp/base.wav", "/tmp/served.wav", -6.0);
    assert!(
        down.join(" ").contains("pitch=0.707107"),
        "-6 semitones is one octave down: {}",
        down.join(" ")
    );
}

#[test]
fn f4_6_s3_a_header_the_server_cannot_read_needs_re_cut_and_is_named() {
    let dir = std::env::temp_dir().join(format!("np-f46-hdr-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");

    // A plain PCM wav at 48 kHz: readable as it stands.
    let plain = dir.join("plain.wav");
    std::fs::write(&plain, wav_bytes(1 /*PCM*/, 48_000)).expect("write plain");
    assert_eq!(voice_ref::wav_header(&plain), Some((1, 48_000)));
    assert!(!voice_ref::needs_re_cut(&plain), "48 kHz PCM is exactly what the server reads");

    // WAVE_FORMAT_EXTENSIBLE (0xFFFE): refused.
    let ext = dir.join("ext.wav");
    std::fs::write(&ext, wav_bytes(0xFFFE, 48_000)).expect("write extensible");
    assert!(voice_ref::needs_re_cut(&ext), "the extensible tag is the one the server chokes on");

    // Above 48 kHz: also refused, because ffmpeg wrote the extensible header for it.
    let hi = dir.join("hi.wav");
    std::fs::write(&hi, wav_bytes(1, 96_000)).expect("write high rate");
    assert!(voice_ref::needs_re_cut(&hi), "96 kHz is over the ceiling");

    // Not a wav at all: also needs cutting, and the notice names the file.
    let mp3 = dir.join("voice.mp3");
    std::fs::write(&mp3, b"ID3\x03not a wav").expect("write non-wav");
    assert_eq!(voice_ref::wav_header(&mp3), None);
    assert!(voice_ref::needs_re_cut(&mp3));
    assert_eq!(
        voice_ref::recut_notice("voice.mp3"),
        "voice reference voice.mp3 is not a wav the server reads -- cutting it again",
        "S3's notice is logged verbatim, naming the file"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn f4_6_s3_a_ref_without_a_base_is_renamed_to_the_base() {
    let root = std::env::temp_dir().join(format!("np-f46-rename-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("session.naivepost/narrate")).expect("narrate/");
    let tree = naivepost::layout::Tree::new(&root.join("session.naivepost")).expect("tree");
    let served = narrate_data::served_reference(&tree);
    let base = narrate_data::base_reference(&tree);
    std::fs::write(&served, wav_bytes(1, 48_000)).expect("write a ref with no base");

    let renamed = voice_ref::adopt_existing_reference(&tree);
    assert_eq!(renamed.as_deref(), Some(base.as_path()), "the ref becomes the base");
    assert!(base.exists() && !served.exists(), "and it moved rather than being copied");

    // Nothing to do when the base is already there, or when neither file exists.
    assert_eq!(voice_ref::adopt_existing_reference(&tree), None, "a base present means nothing to adopt");
    let _ = std::fs::remove_file(&base);
    assert_eq!(voice_ref::adopt_existing_reference(&tree), None, "no files at all is not a rename");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn f4_6_s3_the_build_runs_the_configured_binary_and_names_it_when_it_fails() {
    // The seam is the configured ffmpeg path: point it at a script that records its argv and writes
    // a real wav, then check the program actually ran it and landed both files.
    let root = std::env::temp_dir().join(format!("np-f46-leg-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("session.naivepost/narrate")).expect("narrate/");
    let tree = naivepost::layout::Tree::new(&root.join("session.naivepost")).expect("tree");
    let fake = root.join("fake-ffmpeg.sh");
    let log = root.join("argv.log");
    // A script that records argv and writes whatever output path was passed last, which is how both
    // ffmpeg passes end here.
    common::write_script(
        &fake,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" >> '{}'\nfor a in \"$@\"; do out=\"$a\"; done\nprintf 'x' > \"$out\"\n",
            log.display()
        ),
    )
    .expect("fake ffmpeg written");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).expect("+x");
    }
    let source = root.join("take.wav");
    std::fs::write(&source, wav_bytes(1, 48_000)).expect("source wav");

    let built = voice_ref::build(&tree, &source.to_string_lossy(), &fake.to_string_lossy(), 2.0)
        .expect("the fake binary succeeds, so the build succeeds");
    assert!(built.base.exists(), "voice_ref_base.wav was written by the leg");
    assert!(built.served.exists(), "voice_ref.wav was written by the leg");
    let argv = std::fs::read_to_string(&log).expect("the fake recorded its argv");
    assert!(argv.contains("loudnorm=I=-16"), "the level pass really ran: {argv}");
    assert!(argv.contains("rubberband="), "the pitch pass really ran: {argv}");

    // A missing binary is named, not swallowed.
    let err = voice_ref::build(&tree, "/nonexistent/take.wav", "/definitely/not/here", 0.0)
        .expect_err("a program that is not there must fail");
    assert!(
        err.contains("/definitely/not/here"),
        "the error names the program it tried to run: {err}"
    );
    let _ = std::fs::remove_dir_all(&root);
}

// --- S4: the take floor, the merge, the invalidation ---------------------------------------

#[test]
fn f4_6_s4_a_take_needs_0_4_seconds_and_says_the_length_it_got() {
    assert!(!naivepost::narrate_screen::can_make_take((3.0, 3.2)));
    // 0.4 exactly is the floor; 3.0 + 0.4 lands a hair under in binary, so test just over it.
    assert!(naivepost::narrate_screen::can_make_take((3.0, 3.4001)));
    let said = naivepost::narrate_screen::take_too_short(0.2);
    assert!(said.contains("0.2") && said.contains("0.4"), "{said}");
}

#[test]
fn f4_6_s4_touching_takes_merge_and_a_subtraction_splits() {
    let merged = naivepost::narrate_screen::add_takes(&[(0.0, 5.0)], (5.0, 9.0));
    assert_eq!(merged, vec![(0.0, 9.0)], "touching takes are one take");
    let split = naivepost::narrate_screen::remove_takes(&[(0.0, 9.0)], (3.0, 5.0));
    assert_eq!(split, vec![(0.0, 3.0), (5.0, 9.0)], "taking the middle out leaves two takes");
}

#[test]
fn f4_6_s4_changing_takes_pitch_or_voice_drops_the_shifted_reference() {
    let root = std::env::temp_dir().join(format!("np-f46-drop-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("session.naivepost/narrate")).expect("narrate/");
    let tree = naivepost::layout::Tree::new(&root.join("session.naivepost")).expect("tree");
    let base = narrate_data::base_reference(&tree);
    let served = narrate_data::served_reference(&tree);
    std::fs::write(&base, b"base").expect("base");
    std::fs::write(&served, b"served").expect("served");

    // Pitch: only the shifted copy goes; the base survives to be re-shifted.
    assert_eq!(
        voice_ref::invalidate_for_change(&tree, voice_ref::Changed::Pitch),
        1,
        "a pitch change costs one file"
    );
    assert!(base.exists() && !served.exists(), "the base stays for the re-shift");

    std::fs::write(&served, b"served").expect("served again");
    // Takes: the base was cut from the old seconds, so both go.
    assert_eq!(
        voice_ref::invalidate_for_change(&tree, voice_ref::Changed::Takes),
        2,
        "a take change costs both files"
    );
    assert!(!base.exists() && !served.exists());

    std::fs::write(&base, b"base").expect("base again");
    std::fs::write(&served, b"served").expect("served again");
    // Voice: same rule as takes -- the base came from something else entirely.
    assert_eq!(
        voice_ref::invalidate_for_change(&tree, voice_ref::Changed::Voice),
        2,
        "a voice change costs both files"
    );
    let _ = std::fs::remove_dir_all(&root);
}

// --- S5: the sample cache is keyed by voice and text --------------------------------------

#[test]
fn f4_6_s5_the_sample_is_cached_per_voice_and_text() {
    let root = std::env::temp_dir().join(format!("np-f46-sample-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("session.naivepost")).expect("session");
    let tree = naivepost::layout::Tree::new(&root.join("session.naivepost")).expect("tree");

    let key_a = "warm|This is the voice the narration will be spoken in.";
    let key_b = "warm|A different sentence in the same voice.";
    let first = narrate_data::sample_path(&tree, "warm.wav", key_a);
    let again = narrate_data::sample_path(&tree, "warm.wav", key_a);
    let other_text = narrate_data::sample_path(&tree, "warm.wav", key_b);
    let other_voice = narrate_data::sample_path(&tree, "cold.wav", key_a);
    assert_eq!(first, again, "same voice and text is the same file, so hearing it twice costs nothing");
    assert_ne!(first, other_text, "different words are a different sample");
    assert_ne!(first, other_voice, "a different voice is a different sample even on the same words");
    assert!(
        first.file_name().unwrap().to_string_lossy().starts_with("warm.wav_"),
        "the file name carries the voice so two voices' samples sit side by side: {:?}",
        first.file_name()
    );
    assert_eq!(
        narrate_data::sample_hex(key_a).len(),
        12,
        "the hash half is six bytes of SHA-1 as hex"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// A minimal RIFF/WAVE header with the given format tag and sample rate, plus a data chunk. Only the
/// header matters to these tests; the payload is filler.
fn wav_bytes(format_tag: u16, rate: u32) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&100u32.to_le_bytes());
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&format_tag.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * 2u32).to_le_bytes()); // byte rate
    out.extend_from_slice(&2u16.to_le_bytes()); // block align
    out.extend_from_slice(&16u16.to_le_bytes()); // bits
    out.extend_from_slice(b"data");
    out.extend_from_slice(&4u32.to_le_bytes());
    out.extend_from_slice(&[0u8; 4]);
    out
}

// --- S1/S2/S3 as the program runs them: recut_for_voice ------------------------------------

/// Write a fake ffmpeg that records every call's argv and writes its last argument, and point a
/// settings file at it. Returns (script, argv log).
fn fake_ffmpeg(dir: &std::path::Path) -> (std::path::PathBuf, std::path::PathBuf) {
    let script = dir.join("fake-ffmpeg.sh");
    let log = dir.join("argv.log");
    // A script that records argv and writes whatever output path was passed last.
    common::write_script(
        &script,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" >> '{}'\nfor a in \"$@\"; do out=\"$a\"; done\nprintf 'x' > \"$out\"\n",
            log.display()
        ),
    )
    .expect("fake ffmpeg written");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).expect("+x");
    }
    (script, log)
}

/// The session folder a fake ffmpeg was built for: the fake lives INSIDE the session so one
/// `remove_dir_all` of the root takes the script, its argv log, the recording and the reference with
/// it. The session itself is `root/session.naivepost`.
pub fn recut_root(tag: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("np-f46-recut-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("session.naivepost")).expect("session folder");
    root
}
/// turns and transcript words S2 reads. `tagged` decides whether the project names a recording at all.
fn recut_session(root: &std::path::Path, tagged: bool, with_turns: bool) -> (naivepost::layout::Tree, String) {
    let session = root.join("session.naivepost");
    std::fs::create_dir_all(&session).expect("session folder");
    // The recording lives OUTSIDE the project on purpose: a project path is `project:` + root-relative
    // and the source here is a recording on the machine, so the test uses an absolute temp path.
    let take = root.join("narrator-mic.wav");
    std::fs::write(&take, wav_bytes(1, 48_000)).expect("the recording the reference is cut from");
    let tree = naivepost::layout::Tree::new(&session).expect("tree");
    let mut project = naivepost::project::Project::default();
    if tagged {
        project.sources.push(naivepost::project::Source {
            path: take.to_string_lossy().into_owned(),
            footage: false,
            narrator: 1,
            ..Default::default()
        });
    }
    let source = take.to_string_lossy().into_owned();
    naivepost::project::save(&project, tree.dir()).expect("naivepost.json written");
    if with_turns {
        let turns = vec![
            naivepost::requests::Turn { start_sample: 0, end_sample: 96_000, speaker_id: 1 },
            naivepost::requests::Turn { start_sample: 200_000, end_sample: 296_000, speaker_id: 1 },
        ];
        naivepost::requests::write_turns(&tree, "narrator-mic", &turns).expect("turns.json written");
    }
    (tree, source)
}

/// The source the session's project tagged as narrator 1, read back the way the leg reads it.
#[allow(dead_code)] // compiled by every test in this binary; the untagged scenario asserts through the call site instead
fn take_of(tree: &naivepost::layout::Tree) -> String {
    naivepost::project::load(tree.dir())
        .expect("project")
        .sources
        .iter()
        .find(|s| s.narrator == 1)
        .map(|s| s.path.clone())
        .unwrap_or_default()
}

/// What a word source answers about one window: how many words fall inside it, and what span those words
/// actually occupy (`None` = the diarizer guessed over silence).
#[derive(Clone, Copy)]
struct Words {
    count: usize,
    span: Option<(f64, f64)>,
}

impl Words {
    /// Every window answers the same pair — enough for a rule test that only cares about one turn.
    const fn flat(count: usize, span: Option<(f64, f64)>) -> Self {
        Self { count, span }
    }

    /// The counter half of the pair `plan_reference` is given.
    fn counter(self) -> impl Fn(f64, f64) -> usize {
        move |_, _| self.count
    }

    /// The narrowing half. See [`Words::counter`].
    fn spanner(self) -> impl Fn(f64, f64) -> Option<(f64, f64)> {
        move |from, until| self.span.or(Some((from, until)))
    }

    /// A source whose answer depends on which window is asked about: `(turn, count)` pairs, matched on the
    // turn's own start, so a ranking test can say which stretch said more.
    fn per_turn(self, answers: &[(f64, usize)]) -> impl Fn(f64, f64) -> usize {
        let answers: Vec<(f64, usize)> = answers.to_vec();
        move |from, _until| {
            answers
                .iter()
                .find(|(start, _)| (*start - from).abs() < f64::EPSILON)
                .map(|(_, count)| *count)
                .unwrap_or(self.count)
        }
    }
}

/// 6 s per line at 10 words clears P.eng.refMinWordsPerSecond (1.5) by a wide margin.
const WORDS_PER_LINE: usize = 10;

fn words_for_window(from: f64, until: f64) -> usize {
    let seconds = (until - from).max(0.0);
    (seconds * WORDS_PER_LINE as f64) as usize
}

/// The recut tests' narrowing answer: the diarizer's turn stands as spoken, words filling it edge to
/// edge. Those tests are about which turns are chosen and how they are cut, not about the narrowing.
fn full_span(from: f64, until: f64) -> Option<(f64, f64)> {
    Some((from, until))
}

#[test]
fn f4_6_s1_s3_the_missing_reference_is_cut_from_the_recorded_seconds() {
    use naivepost::narrate_data;
    let root = recut_root("build");
    let (tree, source) = recut_session(&root, true, true);
    let (script, log) = fake_ffmpeg(&root);

    let built = voice_ref::recut_for_voice(
        &tree,
        &script.to_string_lossy(),
        &source,
        &[
            (0.0, 6.0, 1u32),
            (12.5, 18.5, 1),
        ],
        words_for_window,
        full_span,
    )
    .expect("a tagged recording with clean solo turns builds");
    assert!(built.base.exists(), "the base was written at {}", built.base.display());
    assert!(built.served.exists(), "the served copy was written at {}", built.served.display());

    let argv = std::fs::read_to_string(&log).expect("argv recorded");
    // The fake recorded one argument per line, so a call is read by joining with spaces first.
    let joined = argv.replace('\n', " ");
    assert!(
        joined.contains("-ss 0.000 -to 6.000"),
        "the first piece was cut at the seconds S2 chose: {joined}"
    );
    assert!(
        joined.contains("-ss 12.500 -to 18.500"),
        "and the second: {joined}"
    );
    assert!(
        joined.contains("loudnorm=I=-16"),
        "each pass levelled to P.eng.refLoudness: {joined}"
    );
    assert!(
        joined.contains("-f concat -safe 0"),
        "two pieces were stitched by ffmpeg's concat demuxer: {joined}"
    );
    assert!(
        joined.contains("-f concat -safe 0"),
        "two pieces were stitched by ffmpeg's concat demuxer: {joined}"
    );
    // The working files live out of the server's way, under narrate/reference/.
    let part = narrate_data::reference_part(&tree, 0);
    assert!(
        part.display().to_string().contains("narrate/reference/"),
        "the levelled piece is a working file under narrate/reference/: {:?}",
        part.file_name()
    );
    let _ = std::fs::remove_dir_all(tree.dir());
    let _ = std::fs::remove_file(&script);
    let _ = std::fs::remove_file(&log);
}

#[test]
fn f4_6_s1_an_untagged_narrator_is_answered_with_s2s_own_sentence_and_no_file_is_cut() {
    let root = recut_root("untagged");
    let (tree, _source) = recut_session(&root, false, true);
    let (script, log) = fake_ffmpeg(&root);
    // The scenario's project has NO narrator-1 source at all: nothing was tagged, so the build cannot
    // name a recording and answers S2's own sentence.
    assert_eq!(
        naivepost::narrate_data::read_voice(&tree),
        naivepost::narrate_data::OWN_VOICE,
        "this scenario starts with nothing tagged as a narrator"
    );

    let err = voice_ref::recut_for_voice(
        &tree,
        &script.to_string_lossy(),
        "/media/never-tagged.wav",
        &[(0.0, 6.0, 1u32)],
        words_for_window,
        full_span,
    )
    .expect_err("a project that tagged no recording cannot have a reference cut");
    assert_eq!(
        err,
        voice_ref::nothing_tagged(1),
        "the refusal is S2's own sentence, not a paraphrase of it"
    );
    assert!(
        !tree.voice_ref_base_wav().exists() && !tree.voice_ref_wav().exists(),
        "a refusal cuts nothing"
    );
    let _ = std::fs::remove_dir_all(tree.dir());
    let _ = std::fs::remove_file(&script);
    let _ = std::fs::remove_file(&log);
}

#[test]
fn f4_6_s2_no_diarization_reaches_the_build_as_s2s_run_prepare_sentence() {
    let root = recut_root("nodiar");
    let (tree, source) = recut_session(&root, true, false);
    let (script, log) = fake_ffmpeg(&root);

    let err = voice_ref::recut_for_voice(
        &tree,
        &script.to_string_lossy(),
        &source,
        &[],
        words_for_window,
        full_span,
    )
    .expect_err("no turns.json means nothing to cut automatically");
    assert_eq!(
        err,
        voice_ref::NO_DIARIZATION,
        "\"run Prepare, or pick the seconds by hand under the video\" is what the build says too"
    );
    let _ = std::fs::remove_dir_all(tree.dir());
    let _ = std::fs::remove_file(&script);
    let _ = std::fs::remove_file(&log);
}

#[test]
fn f4_6_s3_a_lone_voice_ref_wav_is_adopted_as_the_base_before_anything_is_cut() {
    let root = recut_root("adopt");
    let (tree, source) = recut_session(&root, true, true);
    let (script, _log) = fake_ffmpeg(&root);
    let served = naivepost::narrate_data::served_reference(&tree);
    std::fs::create_dir_all(served.parent().expect("narrate/")).expect("narrate made");
    std::fs::write(&served, b"old bytes from an earlier build").expect("lone ref written");

    let built = voice_ref::recut_for_voice(
        &tree,
        &script.to_string_lossy(),
        &source,
        &[(0.0, 6.0, 1u32)],
        words_for_window,
        full_span,
    )
    .expect("one clean piece needs no concat and builds straight onto the base");
    assert!(
        built.said.iter().any(|s| s.contains("no base") && s.contains("renamed")),
        "the rename is said, so the log shows the history was kept: {:?}",
        built.said
    );
    // The base is now the levelled one (the fake wrote 'x'), so the old file is gone from `served`'s
    // old place only by the rename -- and the base it became is what the level pass overwrote. The
    // point S3 makes is that a lone ref is never silently discarded: it is named and moved first.
    assert!(built.base.exists() && built.served.exists());
    let _ = std::fs::remove_dir_all(tree.dir());
    let _ = std::fs::remove_file(&script);
}

#[test]
fn f4_6_s4_the_build_shifts_the_base_at_the_pitch_the_project_holds() {
    let root = recut_root("pitch");
    let (tree, source) = recut_session(&root, true, true);
    let (script, log) = fake_ffmpeg(&root);
    naivepost::narrate_data::write_pitch(&tree, 3.0).expect("pitch written");

    voice_ref::recut_for_voice(
        &tree,
        &script.to_string_lossy(),
        &source,
        &[(0.0, 6.0, 1u32)],
        words_for_window,
        full_span,
    )
    .expect("build");
    let argv = std::fs::read_to_string(&log).expect("argv recorded");
    assert!(
        argv.contains("rubberband=") && argv.contains("pitch=1.189207"),
        "the served copy is the base at the slider's +3 semitones, formants kept: {argv}"
    );
    let _ = std::fs::remove_dir_all(tree.dir());
    let _ = std::fs::remove_file(&script);
}

// --- S1 / §A.7: which voice the take band is drawn for ---------------------------------------
//
// The band's own lane geometry has a widget test (`sec_07_narrate_1_screen_widgets`); what lives here is
// the QUESTION behind it — does this voice have anything to select from — because that question is answered
// from the project, not from `voice_ref_base.wav`: §F4.6 S4 deletes the built reference on every take,
// pitch or voice change, so a rule keyed to that file hides the band on the very ＋ press that made a take.

/// A session with one narrator-1 row pointing at `take`, and no built reference anywhere: the state right
/// after Prepare tagged a recording and before any line was spoken.
fn band_session(root: &std::path::Path, take: &std::path::Path) -> naivepost::layout::Tree {
    let session = root.join("session.naivepost");
    std::fs::create_dir_all(&session).expect("session folder");
    let tree = naivepost::layout::Tree::new(&session).expect("tree");
    let mut project = naivepost::project::Project::default();
    project.sources.push(naivepost::project::Source {
        path: take.to_string_lossy().into_owned(),
        footage: false,
        narrator: 1,
        ..Default::default()
    });
    naivepost::project::save(&project, tree.dir()).expect("naivepost.json written");
    tree
}

#[test]
fn f4_6_s1_the_take_band_shows_only_for_a_voice_with_a_recording_captions_and_a_missing_file_get_none() {
    let root = std::env::temp_dir().join(format!("np-f46-band-a-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("scratch folder");
    let take = root.join("narrator-mic.wav");
    std::fs::write(&take, wav_bytes(1, 48_000)).expect("the tagged recording");
    let tree = band_session(&root, &take);
    let project = naivepost::project::load(tree.dir()).expect("the project the test wrote");

    // Captions speak nothing, so there is nothing to drag a take out of, whatever is on disk.
    assert!(
        !naivepost::narrate_band_face::recording_exists(&tree, &project, CAPTIONS),
        "captions-only gets no take band (§A.7)"
    );
    // An empty voice id is the same choice spelled differently (the picker stores "" for captions).
    assert!(
        !naivepost::narrate_band_face::recording_exists(&tree, &project, ""),
        "an unset voice is captions, so no band"
    );
    // A voices-folder id whose file went away: §S1's "voice X is no longer in DIR", asked of the folder.
    assert!(
        !naivepost::narrate_band_face::recording_exists(&tree, &project, "gone"),
        "a voices-folder id with no .wav in the folder has no recording to draw"
    );
    // The tagged row exists but its file does not: still no band, which is §A.7's "with a recording".
    let missing_root = root.join("elsewhere");
    std::fs::create_dir_all(&missing_root).expect("folder");
    let missing = band_session(&missing_root, &missing_root.join("vanished.wav"));
    let missing_project = naivepost::project::load(missing.dir()).expect("project");
    assert!(
        !naivepost::narrate_band_face::recording_exists(&missing, &missing_project, "narrator1"),
        "a tagged row whose file is gone answers 'no recording', not 'empty band'"
    );
    // And the same slot WITH its file is the positive case the next test measures.
    assert!(
        naivepost::narrate_band_face::recording_exists(&tree, &project, "narrator1"),
        "the slot Prepare tagged, with its file on disk, is exactly §A.7's case for showing the band"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn f4_6_s1_the_take_band_shows_only_for_a_voice_with_a_recording_spans_the_recording_it_is_drawn_across() {
    let root = std::env::temp_dir().join(format!("np-f46-band-b-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("scratch folder");
    let take = root.join("narrator-mic.wav");
    // 30 s of 8 kHz mono PCM: a real header and real sample count, so the seconds come from the file.
    naivepost::snapshot::write_silence_wav(&take, 8_000, 30 * 8_000).expect("the recording");
    let tree = band_session(&root, &take);
    let project = naivepost::project::load(tree.dir()).expect("the project the test wrote");
    // No built reference at all: S4 has not run, and §F4.6 S4 would delete it again anyway, so the band
    // must measure the RECORDING rather than answer None because `narrate/` is empty.
    // The session folder first: `band_seconds` asks `ui::narrate_band::recording_seconds()` before it looks
    // at anything it is handed, and that door reads the RUNNING program's session (`startup::session_dir`
    // off the cwd), which a unit test cannot stand up — an earlier round's snapshot leaves a 45 s reference
    // in `rust/session.naivepost/`, and here it would be read as this test's own.
    let session = std::env::current_dir().unwrap().join("session.naivepost");
    let reference = naivepost::narrate_data::base_reference(
        &naivepost::layout::Tree::new(&session).expect("the program's session folder"),
    );
    let _ = std::fs::remove_file(&reference);
    assert!(
        naivepost::voice_ref::wav_header(&reference).is_none(),
        "this run has no built reference, which is the state being tested"
    );
    // The band's own seconds, asked the way the painter asks them: `band_seconds` is given the tree and
    // project explicitly, so it cannot pick up a reference left in another test's scratch folder.
    let seconds = naivepost::narrate_band_face::band_seconds(&tree, &project, "narrator1")
        .expect("the recording itself sizes the band");
    assert!(
        (seconds - 30.0).abs() < 0.5,
        "the band spans the recording's own 30 s, got {seconds}"
    );
    // Captions still get nothing, even with a recording sitting under the slot.
    assert!(
        naivepost::narrate_band_face::band_seconds(&tree, &project, CAPTIONS).is_none(),
        "captions have no band to size"
    );
    let _ = std::fs::remove_dir_all(&root);
}
