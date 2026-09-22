// F4.2 The narration call — spec/07-narrate.md F4.2, one test per step or branch of it and of
// spec/02-services.md §3.8's Narration table. Every rule is a plain function in naivepost::narrate_pass, so
// nothing here needs a display, a pipeline or a server: the briefs are strings, the tools answer strings, and
// the flow facts are numbers.

use naivepost::cut::{Fx, Seg};
use naivepost::narrate_pass as pass;
use naivepost::narrate_screen;
use naivepost::roles;
use naivepost::textfmt::SessionLine;

/// The transcript's own lane name: what `session.tsv` writes in the source column.
const MIC: &str = "2026-09-16 17-26-20";

fn seg(s: f64, e: f64) -> Seg {
    Seg { s, e, ..Default::default() }
}

fn row(start: f64, end: f64, source: &str, who: &str, text: &str) -> SessionLine {
    SessionLine { start, end, source: source.into(), who: who.into(), text: text.into() }
}

fn effect(kind: &str, t: f64, dur: f64, text: &str) -> Fx {
    Fx { kind: kind.into(), t, dur, text: text.into(), ..Default::default() }
}

/// P.policy.narrationMinWords, P.policy.narrationMaxWords, P.policy.narrationWordsPerSecond.
#[test]
fn f4_2_s1_a_clip_is_offered_under_a_third_of_speaking_rate_between_the_floor_and_the_ceiling() {
    // The floor: even a five-second clip can take a short one.
    assert_eq!(pass::word_ceiling(5.0), 8);
    // The linear middle: 0.75 words per second of what the viewer watches, read as whole words.
    assert_eq!(pass::word_ceiling(12.0), 9);
    assert_eq!(pass::word_ceiling(40.0), 30);
    // The ceiling binds past forty seconds, so a two-minute clip does not get two minutes of talking.
    assert_eq!(pass::word_ceiling(60.0), 30);
    assert_eq!(pass::WORDS_PER_SECOND, 0.75);

    // The seconds that count are the on-screen ones: a clip at 2x affords half as many words as its session
    // length, because that is how long the line has to be spoken in.
    let fast = Seg { s: 20.0, e: 38.0, rate: 2.0, ..Default::default() };
    assert_eq!(pass::on_screen(&fast), 9.0);
    assert_eq!(pass::word_ceiling(pass::on_screen(&fast)), 8, "seven words round down to the floor");

    // A spliced insert runs for its own `dur` and nothing else.
    let card = Seg { s: 40.0, e: 40.0, dur: 8.0, ..Default::default() };
    assert_eq!(pass::on_screen(&card), 8.0);
}

/// P.policy.narrationMinWords / narrationMaxWords in the sentence the heading prints.
#[test]
fn f4_2_s2_a_heading_names_the_bounds_how_long_it_plays_and_the_word_budget() {
    // A clip at 2x: both facts are printed, because a line stamped [+16s] of it arrives eight seconds into
    // what the viewer watches. The dash between the bounds is an en dash (U+2013).
    let fast = Seg { s: 20.0, e: 38.0, rate: 2.0, ..Default::default() };
    assert_eq!(
        pass::clip_heading(7, &fast),
        "CLIP 7: 20.0\u{2013}38.0 (18 s of footage at 2x, 9 s on screen, at most 8 words -- fewer is better, none is fine)"
    );

    // A clip at its own speed says nothing about a rate: thirty seconds, and the budget follows them.
    assert_eq!(
        pass::clip_heading(1, &seg(0.0, 30.0)),
        "CLIP 1: 0.0\u{2013}30.0 (30 s, at most 22 words -- fewer is better, none is fine)"
    );

    // A rate of exactly 1 is the footage's own clock and is not mentioned either.
    let own = Seg { s: 0.0, e: 9.0, rate: 1.0, ..Default::default() };
    assert_eq!(pass::plays(&own), "9 s");
}

/// P.machine.narrationContextSeconds — the ±4 s window, and who each row is shown as.
#[test]
fn f4_2_s3_a_clip_reaches_four_seconds_each_way_for_lines_and_labels_what_it_shows() {
    let clips = [seg(100.0, 120.0)];
    // `source` is the recording's own name — a basename or a `project:`-relative path; the label is decided
    // on that, and an extension is part of it (the transcript fixer answers with the same rule).
    let rows = [
        // Ends 0.1 s before the window's edge, so it is still inside it: `end > s - 4`. Its own start is ten
        // seconds before the clip's, and that is the offset printed — a line just outside goes negative,
        // which is exactly what it is.
        row(90.0, 96.1, "game.wav", "", "before the clip"),
        // Ends exactly on the edge and past it: excluded, both of them.
        row(90.0, 96.0, "game.wav", "", "on the edge"),
        row(88.0, 95.9, "game.wav", "", "outside"),
        // The narrator's own microphone is named by role, an event is not a person, and nobody who reached a
        // diariser at all is SPEAKER.
        row(106.0, 108.0, "project:mic.mkv", "", "my own voice"),
        row(110.0, 111.0, "game.wav", "EVENT", "an explosion"),
        row(118.0, 119.0, "game.wav", "", "unnamed speaker"),
    ];
    let brief = pass::brief(&clips, &rows, &[], "mic.mkv");

    assert!(brief.contains("  [-10s] SPEAKER: before the clip\n"), "{brief}");
    assert!(!brief.contains("on the edge"), "{brief}");
    assert!(!brief.contains("outside"), "{brief}");
    assert!(brief.contains("  [+6s] NARRATOR: my own voice\n"), "{brief}");
    assert!(brief.contains("  [+10s] EVENT: an explosion\n"), "{brief}");
    // Offsets are printed from the clip's own start, with the sign saying which way they run.
    assert!(brief.contains("  [+18s] SPEAKER: unnamed speaker\n"), "{brief}");
    assert!(!brief.contains(pass::NO_LINES), "a clip with rows over it says nothing about silence");

    // The same sentence appears only when nothing qualified — and the row count is what decides, not whether
    // any row exists at all.
    let quiet = pass::brief(&clips, &[], &[], MIC);
    assert!(quiet.contains(pass::NO_LINES), "{quiet}");
}

/// F4.2 (`inserts described …`): an insert's block is the note and nothing else, query kept.
#[test]
fn f4_2_s4_an_insert_is_described_by_its_file_and_parameters_rather_than_by_the_session() {
    let card = Seg { s: 40.0, e: 40.0, dur: 6.0, ins: "project:tier.svg?S=Dust II".into(), ..Default::default() };
    let rows = [row(38.0, 42.0, "game.wav", "", "said under what the card covers")];
    let brief = pass::brief(&[card], &rows, &[], MIC);

    assert!(brief.contains("\"tier.svg?S=Dust II\""), "{brief}");
    assert!(brief.contains("not footage: a graphic or clip inserted here"), "{brief}");
    assert!(brief.contains("Narrate what is ON it, or say nothing -- there is nothing from the session to describe, and inventing one would caption the wrong picture)"), "{brief}");
    // The lines around it are a description of whatever it covered — the one thing the viewer will not be
    // looking at — so none of them ride on the block, not even as the "no lines" sentence.
    assert!(!brief.contains("said under what the card covers"), "{brief}");
    assert!(!brief.contains(pass::NO_LINES), "{brief}");
}

/// F4.2 (`effects as "[+Ns] MARKED: name" / "[+Ns] CAPTION: text"`): only a label and a caption are read here.
#[test]
fn f4_2_s5_a_marked_moment_and_a_caption_are_read_but_an_effect_that_changes_the_picture_is_not() {
    let clips = [seg(100.0, 120.0)];
    let fx = [
        effect("label", 102.0, 3.0, "boss fight"),
        effect("text", 104.0, 3.0, "a caption the viewer reads"),
        // Three kinds that say something about the footage the writer has no way to speak to.
        effect("zoom", 106.0, 3.0, "the reveal"),
        effect("volume", 108.0, 3.0, "quieter"),
        effect("speed", 110.0, 3.0, "faster"),
        // And a label with no word in it is not the editor's word for anything.
        effect("label", 112.0, 3.0, "   "),
    ];
    let brief = pass::brief(&clips, &[], &fx, MIC);

    assert!(brief.contains("  [+2s] MARKED: boss fight\n"), "{brief}");
    assert!(brief.contains("  [+4s] CAPTION: a caption the viewer reads\n"), "{brief}");
    for absent in ["the reveal", "quieter", "faster", "MARKED:"] {
        let hits = brief.matches(absent).count();
        let expected = if absent == "MARKED:" { 1 } else { 0 };
        assert_eq!(hits, expected, "{absent} in {brief}");
    }
    // A mark over the clip *before* is not this clip's moment.
    let elsewhere = [effect("label", 90.0, 3.0, "the previous scene")];
    assert!(!pass::brief(&clips, &[], &elsewhere, MIC).contains("previous scene"));
}

/// F4.2 (`+ the speech addendum, when the clips play what people said out loud`): the premise, not a guess.
#[test]
fn f4_2_s6_speech_is_heard_only_when_a_scene_that_has_a_say_over_it_keeps_its_lane() {
    let spoken = [row(10.0, 12.0, "game.wav", "Ada", "said out loud")];
    let scene = seg(0.0, 20.0);

    assert!(pass::speech_heard(&[scene.clone()], &spoken, ""), "a scene that keeps the lane plays it");

    // A `quiet` entry is a lane name — the recording's basename, which is what `session.tsv` writes in its
    // source column too — so silencing it is what stops the line being heard.
    let hushed = Seg { quiet: vec!["game.wav".into()], ..scene.clone() };
    assert!(!pass::speech_heard(&[hushed], &spoken, ""), "a scene that silences the lane plays nothing");

    // An insert hears nothing: it replaces the footage that carried the line.
    let card = Seg { s: 0.0, e: 0.0, dur: 20.0, ins: "project:tier.svg".into(), ..Default::default() };
    assert!(!pass::speech_heard(&[card], &spoken, ""), "a card over the line plays nothing");

    // The narrator's own microphone never survives — that is the whole point of having one. The row names it
    // as a path and the project names it by its basename, and either spelling is the same recording.
    let mine = [row(10.0, 12.0, "project:mic.mkv", "Nina", "my own voice")];
    assert!(!pass::speech_heard(&[scene.clone()], &mine, "mic.mkv"), "the narrator's mic is not speech in the video");
    assert!(pass::speech_heard(&[scene.clone()], &mine, ""), "with no microphone named it is somebody else's");

    // A picture event is not somebody talking.
    let event = [row(10.0, 12.0, "game.wav", "EVENT", "an explosion")];
    assert!(!pass::speech_heard(&[scene], &event, ""));
}

/// F4.2 S1 (`SYSTEM house rules + the "narrate" prompt + …`): each addendum rides on its own.
#[test]
fn f4_2_s7_the_two_addenda_ride_on_the_prompt_each_on_its_own() {
    let both = pass::system("PROMPT", true, true);
    assert!(both.starts_with("PROMPT\n\n"), "the prompt is sent as it was given: {both}");
    assert_eq!(both, format!("PROMPT\n\n{}\n\n{}", pass::SPEECH_ADDENDUM, pass::CAPTIONS_ADDENDUM));

    assert_eq!(pass::system("PROMPT", true, false), format!("PROMPT\n\n{}", pass::SPEECH_ADDENDUM));
    assert_eq!(pass::system("PROMPT", false, true), format!("PROMPT\n\n{}", pass::CAPTIONS_ADDENDUM));
    assert_eq!(pass::system("PROMPT", false, false), "PROMPT");

    // The sentences F4.2 quotes are in the bytes it says they are.
    assert!(pass::SPEECH_ADDENDUM.starts_with("THIS VIDEO PLAYS WHAT PEOPLE SAID OUT LOUD."));
    assert!(pass::SPEECH_ADDENDUM.contains("never say one back: set it up before it lands, or react after it"));
    assert!(pass::CAPTIONS_ADDENDUM.contains("- Write for the eye, not the ear."));
    assert!(pass::CAPTIONS_ADDENDUM.contains("\"emotion\" means nothing with no voice. Leave it \"\"."));
    assert!(pass::CAPTIONS_ADDENDUM.contains("a \"pos\": where the caption sits on the picture"));
}

/// F4.2 S2 (`USER User Context (+ the speech rule) + "THE CLIPS…" + per clip …`).
#[test]
fn f4_2_s8_the_second_message_is_context_then_the_speech_rule_then_the_clips() {
    let clips = [seg(0.0, 9.0)];
    let rows = [row(1.0, 2.0, "game.wav", "Ada", "hello")];

    // No context: no block to be about, and no speech rule either — it is only ever about a context that
    // exists. The message starts at the header.
    let bare = pass::user_message("   ", &clips, &rows, &[], "");
    assert!(bare.starts_with(pass::CLIPS_HEADER), "{bare}");
    assert!(!bare.contains("USER CONTEXT"), "{bare}");
    assert!(!bare.contains(pass::CTX_SPEECH), "{bare}");

    let full = pass::user_message("  keep the swearing  ", &clips, &rows, &[], "");
    assert!(full.starts_with("USER CONTEXT -- written by the person who made this recording"), "{full}");
    // Trimmed: what the person wrote, not the whitespace around it.
    assert!(full.contains("\nkeep the swearing\n\n"), "{full}");
    let order = [
        "USER CONTEXT",
        "keep the swearing",
        "The speech is content unless the user context above says otherwise",
        pass::CLIPS_HEADER,
        "CLIP 1: ",
        "hello",
    ];
    let mut at = 0usize;
    for needle in order {
        let found = full[at..]
            .find(needle)
            .unwrap_or_else(|| panic!("{needle:?} missing or out of order in {full}"));
        at += found + needle.len();
    }
}

/// §3.8's `write_line` refusals: one sentence per argument, each naming what was sent.
#[test]
fn f4_2_s9_write_line_refuses_a_clip_a_placement_and_an_emotion_in_that_order() {
    let clips = [seg(0.0, 10.0), seg(10.0, 38.0)];

    // The clip first: nothing else about the call matters if it is not one of the ones given.
    let outside = pass::write_line(&clips, 6, 1.0, "text", "", "");
    assert!(outside.contains("\"error\""), "{outside}");
    assert!(outside.contains("clip 6 is not one of the clips given (1 to 2)"), "{outside}");

    // Then the placement, naming the word sent.
    let pos = pass::write_line(&clips, 1, 1.0, "text", "", "left");
    assert!(pos.contains("is not a placement"), "{pos}");
    // The sentence quotes the word it refused. Quoting costs escaping inside the JSON envelope, so what is
    // pinned is that the word arrives whole and marked as a word — not which of the two escape layers got
    // there first.
    assert!(pos.contains("left"), "{pos}");
    // One sentence per call: the refusal stops at the placement, so the model is never told two things to
    // change and gets one of them wrong. The sentence names the word sent and nothing about the emotion —
    // which means it must not name the three it knows either, since "center" would be such a word.
    let both = pass::write_line(&clips, 1, 1.0, "text", "banana", "left");
    assert!(both.contains("is not a placement"), "{both}");
    assert!(!both.contains("banana"), "{both}");
    // "emotion" is the word the next sentence down would use, and it must not appear in this one — so neither
    // may the eight bases that sentence goes on to list.
    assert!(!both.contains("emotion"), "{both}");
    assert!(!both.contains("melancholic"), "{both}");

    // Then the emotion, naming it. A placement is refused before an emotion even when both are wrong, so the
    // model gets one fix per round rather than two chances to guess again. `bottom` is a known placement and
    // therefore the control: only its argument may be what fails.
    let emo = pass::write_line(&clips, 1, 1.0, "text", "banana", "bottom");
    assert!(emo.contains("banana"), "{emo}");
    assert!(emo.contains("not in the vocabulary"), "{emo}");
    assert!(!emo.contains("placement"), "{emo}");
    // And it comes before a wrong placement's own sentence would matter: with both arguments wrong, §3.8's
    // order says the placement is what gets answered first.

    // A good call is answered with the line as it will play: where it landed after clamping, how long the
    // clip runs, and the ceiling it must stay under. No packing numbers — those are F4.3's.
    let answer: serde_json::Value =
        serde_json::from_str(&pass::write_line(&clips, 2, 30.0, "one two three", "happy", "top")).unwrap();
    assert_eq!(answer["clip"], 2);
    // The clip runs 28 s on screen, so an offset in its last second is pulled back to where it can be spoken.
    assert_eq!(answer["at"], 27.0);
    assert_eq!(answer["seconds_on_screen"], 28.0);
    assert_eq!(answer["at_most_words"], 21);
    assert_eq!(answer["words"], 3);
    assert_eq!(answer["pos"], "top");
    for packed in ["lead", "gap", "tail", "tempo", "extend"] {
        assert!(!answer.to_string().contains(packed), "F4.3's arithmetic leaked: {answer}");
    }

    // No placement at all is the default, not a refusal: `pos?` is optional in §3.8.
    let bare: serde_json::Value =
        serde_json::from_str(&pass::write_line(&clips, 1, 2.0, "one", "", "")).unwrap();
    assert_eq!(bare["pos"], "");
}

/// F4.2 (`at clamped inside the clip, never its last second · pos normalised`).
#[test]
fn f4_2_s10_an_offset_never_lands_in_the_last_second_and_a_placement_has_five_spellings() {
    assert_eq!(pass::at_offset(-2.0, 10.0), 0.0, "never negative");
    assert_eq!(pass::at_offset(9.4, 10.0), 9.0, "never in the final second");
    assert_eq!(pass::at_offset(0.4, 0.5), 0.0, "a clip shorter than a second starts at its start");
    assert_eq!(pass::at_offset(3.0, 10.0), 3.0, "and an offset that fits is not moved");

    assert_eq!(pass::normalise_pos("top"), Some("top"));
    assert_eq!(pass::normalise_pos("Center"), Some("center"));
    assert_eq!(pass::normalise_pos("CENTRE"), Some("center"));
    assert_eq!(pass::normalise_pos(" middle "), Some("center"));
    // Bottom IS the empty string: the record stores the default as nothing at all.
    assert_eq!(pass::normalise_pos("bottom"), Some(""));
    // No placement at all is not a refusal — it says "bottom", which is what `pos?` means when it is absent.
    // What is refused is a word that names some other corner of the picture.
    assert_eq!(pass::normalise_pos(""), None);
    assert_eq!(pass::normalise_pos("left"), None);
}

/// §3.8's `list_emotions` / the emotion column of `write_line`: the vocabulary, and nothing else.
#[test]
fn f4_2_s11_an_emotion_is_a_base_or_one_of_its_kin_and_the_table_shows_both() {
    assert!(pass::emotion_known(""), "a line with no emotion is what a captions-only project writes");
    for base in narrate_screen::EMOTIONS {
        assert!(pass::emotion_known(base), "{base} is one of the eight");
    }
    assert!(pass::emotion_known("furious"), "someone writing \"furious\" means angry");
    assert!(pass::emotion_known("DEADPAN"), "and a spelling is not a refusal");
    assert!(pass::emotion_known("happy=0.8"), "a weight rides on the name");
    assert!(pass::emotion_known("surprised=0.4, calm=0.2"), "two named bases are one vector");
    assert!(!pass::emotion_known("banana"));
    assert!(!pass::emotion_known("happy=banana"), "a weight that is not a number is not a weight");
    assert!(!pass::emotion_known("happy, banana"), "one unknown word refuses the tag");

    // The eight bases in §1's order, each with its kin.
    let bases: Vec<&str> = pass::EMOTION_KIN.iter().map(|(base, _)| *base).collect();
    assert_eq!(bases, narrate_screen::EMOTIONS.to_vec());

    let table = pass::list_emotions();
    let lines: Vec<&str> = table.lines().collect();
    assert_eq!(lines.len(), 8, "{table}");
    assert!(lines[0].starts_with("happy: ") && lines[0].contains("joyful"), "{}", lines[0]);
    assert!(lines[7].contains("matter-of-fact") && lines[7].contains("neutral"), "{}", lines[7]);
}

/// F4.2's last paragraph: three attempts, thinking off after a call that wrote nothing, progress only ever
/// forward, and never from the reply cache.
#[test]
fn f4_2_s12_the_call_runs_three_times_with_thinking_that_stops_and_a_bar_that_only_moves_forward() {
    assert_eq!(pass::attempts(), 3);
    assert_eq!(pass::attempts(), roles::LLM_ATTEMPTS, "one count for every LLM job");

    // Thinking is on to begin with and stops after an attempt that wrote nothing; it never comes back on.
    assert!(pass::thinking_after(true, true), "a call that wrote something may still think");
    assert!(!pass::thinking_after(true, false), "a call spent entirely thinking does not get to again");
    assert!(!pass::thinking_after(false, true), "and once off it stays off");
    let log = pass::thinking_off_log();
    assert!(log.contains("wrote nothing \u{2014} asking again with thinking off"), "{log}");
    assert!(log.starts_with(">>> narrate:"), "{log}");

    // Between two attempts the reason is said back, numbered from one.
    assert_eq!(
        pass::rejected_log(2, "clip 3 (20.0-30.0) got no entry"),
        ">>> narration attempt 2 rejected: clip 3 (20.0-30.0) got no entry"
    );

    // The bar counts clips as they close, out of the clips asked about — and a retry never moves it back.
    assert_eq!(pass::progress(3, 9), "writing 3/9 clips");
    assert_eq!(pass::progress_forward(7, 1), 7, "a retry that has written one clip is not work undone");
    assert_eq!(pass::progress_forward(2, 5), 5);

    // Never served from the reply cache: the TTS cache is the cache.
    assert!(!pass::served_from_cache());

    // Two rejections that are not about the shape of the reply at all.
    assert_eq!(pass::no_line_fault(), "every clip came back with no line at all");
    assert_eq!(pass::missing_clip_fault(3, 20.0, 30.0), "clip 3 (20.0-30.0) got no entry");

    // `leave_silent` is the answer that silence has to be said: ok, naming the clip and nothing else.
    let silent: serde_json::Value = serde_json::from_str(&pass::leave_silent(4)).unwrap();
    assert_eq!(silent["clip"], 4);
    assert_eq!(silent.get("error"), None, "{}", silent);
}
