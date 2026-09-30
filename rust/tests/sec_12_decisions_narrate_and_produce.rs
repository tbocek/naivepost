//! §12-decisions#narrate-and-produce — `spec/12-decisions.md` §3, decisions 22-38, checked against the
//! modules that own each rule. Every entry is a pure function call: no socket, no display, no pipeline. The
//! narration rules live in `naivepost::narrate_pass` (answered by `narrate_live`'s tool door), the upload
//! ones in `naivepost::produce_upload` (answered by `produce_upload_call`), the subtitle ones in
//! `naivepost::produce_subtitles` (the render's cue sheet and the translation round-trip).
//!
//! Each test asserts the RULE and its REASON, not only an output: what decision 23 was about is not that a
//! squeeze exists but that the WRITER HEARS it; what decision 38 was about is not that a bad line keeps its
//! original but that the log NAMES it.

use std::collections::BTreeMap;

use naivepost::cut::{Fx, Seg};
use naivepost::narrate_pass as pass;
use naivepost::narrate_screen;
use naivepost::narrate_tts;
use naivepost::produce_render;
use naivepost::produce_subtitles as sub;
use naivepost::produce_upload as up;
use naivepost::project::{Publish, TitleBox};

/// The spec item these cases belong to, named once so the binary says which item it is.
#[allow(dead_code)] // every test name already carries the item id; this states it for a reader of the file
const ITEM: &str = "sec_12_decisions_narrate_and_produce";

fn seg(s: f64, e: f64) -> Seg {
    Seg { s, e, ..Default::default() }
}

fn fx(kind: &str, t: f64, dur: f64, text: &str) -> Fx {
    Fx { kind: kind.into(), t, dur, text: text.into(), ..Default::default() }
}

/// Parse a tool answer: every one of these is a JSON string, ok or error.
fn body(answer: &str) -> serde_json::Value {
    serde_json::from_str(answer).unwrap_or_else(|_| panic!("not a tool answer: {answer}"))
}

fn refused(answer: &str) -> String {
    body(answer)["error"]
        .as_str()
        .unwrap_or_else(|| panic!("expected an error answer, got {answer}"))
        .to_string()
}

/// Decision 22: one clip skipped rejects NOTHING per reply — `write_line` takes one clip at a time — while
/// `finish` refuses the whole until silence has been *said* for every clip.
#[test]
fn sec_12_decisions_narrate_and_produce_s22_finish_names_the_clip_left_without_a_line_or_a_silence() {
    // One clip answered out of three: the prototype rejected the reply; here the answer stands and `finish`
    // says which clips are still blank, so the writer can fix them instead of losing thirty-nine good clips.
    let verdict = pass::finish(&[2], &[]);
    let why = refused(&verdict);
    assert!(why.contains("no line and no silence"), "{why}");
    // `finish` counts the clips it was told about: one answer out of two leaves clip 1 blank, and that is
    // what the writer is told to fix rather than having a whole reply rejected over one gap.
    assert_eq!(why, "no line and no silence for clip(s) 1", "{why}");

    // Silence must be explicit: `leave_silent` counts as an answer, and once every clip has one, ok.
    assert_eq!(body(&pass::leave_silent(1))["silent"], true);
    assert_eq!(body(&pass::finish(&[1, 2, 3], &[]))["done"], true);
}

/// Decision 23: placement is overridden at render time, so the TOOL must say whether the line will be
/// squeezed — in the render's own logged sentence — and `finish` must name a clip that will not fit.
#[test]
fn sec_12_decisions_narrate_and_produce_s23_the_writer_is_told_a_line_will_be_squeezed() {
    let short = [seg(0.0, 2.0)];
    // A 2-second clip cannot hold forty words: the answer carries the render's own remedy sentence.
    let long_text = "word ".repeat(40);
    let answer = body(&pass::write_line(&short, 1, 0.5, &long_text, "", ""));
    let squeeze = answer["squeeze"]
        .as_str()
        .unwrap_or_else(|| panic!("a line that cannot fit says so: {answer}"));
    // The wording is `produce_render::sped_up_log`'s, so tool, row and render spell one fact one way.
    assert_eq!(
        squeeze,
        produce_render::sped_up_log(1, 40.0 * 5.0 / 15.0, 2.0, produce_render::MAX_TEMPO),
        "{squeeze}"
    );
    assert!(squeeze.contains("does not fit") && squeeze.contains("sped up"), "{squeeze}");
    // The tempo quoted never passes P.eng.narrationMaxTempo: promising more than the render may do is the
    // mistake this decision was written about.
    assert!(squeeze.contains("1.25x"), "the ceiling, not a fantasy: {squeeze}");

    // A line that fits says nothing: a note on every line would bury the one that matters.
    let fine = body(&pass::write_line(&[seg(0.0, 20.0)], 1, 1.0, "a short line", "", ""));
    assert!(fine.get("squeeze").is_none(), "{fine}");

    // And `finish` names a clip whose lines will not fit, separately from a clip nobody answered, so the
    // writer rewrites shorter rather than letting the render squeeze it later. This is how `narrate_ask`
    // composes them: the measurement comes from the same ladder the render runs (`mirror_fit` answers
    // `Fit::Overruns` for these numbers), and its clip number goes into `finish`' unfitted list.
    let room = 2.0;
    let crowded = [(0usize, 0.5_f64, 13.3_f64)];
    assert!(
        matches!(
            narrate_screen::mirror_fit(room, &crowded),
            narrate_screen::Fit::Overruns { .. }
        ),
        "the render would have to slide or speed these lines"
    );
    // `finish` counts the clips it was told about: a clip that is only in the unfitted list has had no
    // answer at all, so it is named as blank and not also as unfitted (one heading per clip). This is how
    // `narrate_ask` composes them: the measurement comes from the same ladder the render runs
    // (`mirror_fit` answers `Fit::Overruns` for these numbers) and its clip number goes into the list.
    let both = refused(&pass::finish(&[1, 2, 3], &[4]));
    assert!(both.contains("no line and no silence for clip(s) 4"), "{both}");
    assert!(!both.contains("will not fit"), "a clip nobody answered is named once, as blank: {both}");
    // A clip that DID get an answer is named under the fit heading alone, which is the sentence that tells
    // the writer to rewrite it shorter instead of letting the render squeeze it later.
    let fitted = refused(&pass::finish(&[1, 2, 3, 4, 5], &[4]));
    assert!(fitted.contains("will not fit") && !fitted.contains("no line and no silence"), "{fitted}");
}

/// Decision 25: same intent, two performances, neither number told — so the answer carries the resolved
/// weights, and `list_emotions` publishes the table the prototype kept to itself.
#[test]
fn sec_12_decisions_narrate_and_produce_s25_the_emotion_comes_back_resolved_and_the_table_is_published() {
    // A weighted tag resolves to the SAME eight floats the speak leg sends (`narrate_tts::options`).
    let tag = "angry=1, calm=0.4";
    let answer = body(&pass::write_line(&[seg(0.0, 20.0)], 1, 1.0, "words", tag, ""));
    assert_eq!(answer["emotion"], tag, "the tag as asked");
    assert_eq!(
        answer["weights"].as_str().expect("a weighted tag answers weights"),
        narrate_tts::weights_of(tag).expect("the speak leg resolves the same tag"),
        "one intent, one performance"
    );
    // An unweighted tag has no vector to promise: those words go to the judge, and saying so beats floats.
    let bare = body(&pass::write_line(&[seg(0.0, 20.0)], 1, 1.0, "words", "furious", ""));
    assert!(bare["weights"].is_null(), "{bare}");

    // The published table: the eight bases with kin, then every named blend as its recipe.
    let table = pass::list_emotions();
    for base in narrate_screen::EMOTIONS {
        assert!(table.contains(&format!("{base}: ")), "{base} is published: {table}");
    }
    assert!(table.contains("named blends"), "{table}");
    for blend in narrate_tts::BLENDS.iter() {
        for name in blend.names {
            assert!(table.contains(name), "blend {name} is published");
        }
    }
    // A recipe over BASE NAMES, because axis order is something the writer would have to be told.
    assert!(table.contains("= happy 1 + surprised 0.55"), "{table}");
}

/// Decision 26: a `pos` outside the five known words used to become bottom silently. It is an error now,
/// and the default is stored as nothing at all.
#[test]
fn sec_12_decisions_narrate_and_produce_s26_an_unknown_placement_is_refused_rather_than_becoming_bottom() {
    let clips = [seg(0.0, 20.0)];
    let why = refused(&pass::write_line(&clips, 1, 1.0, "text", "", "left"));
    assert!(why.contains("is not a placement"), "{why}");
    assert!(why.contains("bottom") && why.contains("top") && why.contains("center"), "the answer names the way out: {why}");

    assert_eq!(pass::normalise_pos("top"), Some("top"));
    for spelling in ["center", "centre", "middle"] {
        assert_eq!(pass::normalise_pos(spelling), Some("center"), "{spelling}");
    }
    // Bottom is the default, so it stores nothing: the bytes match a line with no placement at all.
    assert_eq!(pass::normalise_pos("bottom"), Some(""));
    assert_eq!(body(&pass::write_line(&clips, 1, 1.0, "t", "", "bottom"))["pos"], "");
}

/// Decision 27: entries were re-sorted while out-of-order clips were rejected — two policies for one
/// mistake class. One policy now: the tool takes any order, and a message reads in clip order anyway.
#[test]
fn sec_12_decisions_narrate_and_produce_s27_any_order_is_accepted_and_reads_in_clip_order() {
    // Answered backwards, one blank: the refusal lists the blanks ascending whatever order they arrived in.
    let why = refused(&pass::finish(&[5, 3, 1], &[]));
    assert_eq!(why.matches("no line and no silence").count(), 1, "{why}");
    assert!(why.contains("2, 4"), "ascending, not arrival order: {why}");
    // No order complaint anywhere: nothing here can call a shuffled list a fault.
    assert!(!why.contains("order"), "{why}");
    // Out-of-order unfitted numbers are deduped and sorted, and a clip already named as blank is not also
    // called unfitted: one clip, one heading.
    let mixed = refused(&pass::finish(&[1, 2], &[4, 2, 4]));
    assert!(mixed.contains("clip(s) 3, 4"), "blanks ascending: {mixed}");
    assert_eq!(mixed.matches("clip(s)").count(), 2, "two headings, each naming its clips once: {mixed}");
    assert!(mixed.contains("will not fit, rewrite shorter rather than let the render squeeze them: clip(s) 2"), "{mixed}");
}

/// Decision 28: an inserted card was described to the writer by file name alone, though the app rendered it
/// and knows its parameters.
#[test]
fn sec_12_decisions_narrate_and_produce_s28_a_card_is_answered_with_its_parameters_not_its_name_alone() {
    let card = Seg { s: 40.0, e: 40.0, dur: 6.0, ins: "project:tier.svg?S=Dust II&L=3".into(), ..Default::default() };
    let answer = body(&pass::describe_insert(&[card.clone()], 1));
    let params = answer["params"]
        .as_array()
        .unwrap_or_else(|| panic!("the card's parameters are the point: {answer}"));
    assert!(params.contains(&serde_json::json!("S=Dust II")), "{answer}");
    assert!(params.contains(&serde_json::json!("L=3")), "{answer}");
    assert_eq!(answer["insert"], "project:tier.svg", "the path as stored, query dropped");
    assert_eq!(answer["seconds_on_screen"], 6.0, "its length travels too");
    assert_ne!(answer.as_str(), Some("\"tier.svg?S=Dust II\""), "not the file name alone");

    // Footage is refused rather than answered vaguely: the transcript is where that clip's words are.
    let why = refused(&pass::describe_insert(&[seg(0.0, 10.0)], 1));
    assert!(why.contains("footage, not an insert"), "{why}");
}

/// Decision 29: the narration brief carried only `label` and `text`; a zoom, a burned-in graphic and a mute
/// were invisible to the writer. Every kind is passed now.
#[test]
fn sec_12_decisions_narrate_and_produce_s29_the_narration_brief_passes_every_effect_kind() {
    let clips = [seg(100.0, 120.0)];
    let effects = [
        fx("label", 102.0, 3.0, "boss fight"),
        fx("text", 104.0, 3.0, "read on screen"),
        fx("zoom", 106.0, 3.0, "the reveal"),
        Fx { kind: "volume".into(), t: 108.0, dur: 3.0, gain: 0.0, ..Default::default() },
        Fx { kind: "speed".into(), t: 110.0, dur: 3.0, rate: 2.0, ..Default::default() },
        Fx { kind: "speed".into(), t: 112.0, dur: 3.0, rate: 0.0, ..Default::default() },
        fx("svg", 114.0, 3.0, "a lower-third"),
        // A mark with no word in it is still not anybody's label.
        fx("label", 116.0, 3.0, "   "),
    ];
    let brief = pass::brief(&clips, &[], &effects, "");
    assert!(brief.contains("  [+2s] MARKED: boss fight\n"), "{brief}");
    assert!(brief.contains("  [+4s] CAPTION: read on screen\n"), "{brief}");
    assert!(brief.contains("  [+6s] ZOOM: the reveal\n"), "{brief}");
    assert!(brief.contains("  [+8s] VOLUME:"), "{brief}");
    assert!(brief.contains("  [+10s] SPEED: the footage runs at 2x here"), "{brief}");
    // A stop is a speed whose rate is 0, and the difference is worth speaking to.
    assert!(brief.contains("  [+12s] STOP: the picture holds here\n"), "{brief}");
    assert!(brief.contains("  [+14s] GRAPHIC: a lower-third\n"), "{brief}");
    assert!(!brief.contains("[+16s]"), "an empty mark stays out: {brief}");
    // A decoration on the clip BEFORE is not this clip's moment.
    assert!(!pass::brief(&clips, &[], &[fx("zoom", 90.0, 3.0, "elsewhere")], "").contains("elsewhere"));
}

/// Decision 30: the upload brief carried no effects and no clip rate, so a moment marked "the reveal"
/// reached the description writer as nothing and a sped clip's stamps sat on a clock it was not told about.
#[test]
fn sec_12_decisions_narrate_and_produce_s30_the_upload_brief_prints_the_rate_and_carries_the_marks() {
    let plain = up::BriefClip {
        number: 1,
        at_output: 0.0,
        seconds: 10.0,
        session: "00:00\u{2013}00:10".into(),
        seen: vec!["a menu".into()],
        said: vec![],
        rate: 1.0,
        marked: vec![],
    };
    // An unsped, undecorated clip keeps F5.6 S1's header exactly: the additions are conditional.
    let text = up::brief(&[plain.clone()], &[], up::BRIEF_MAX_CHARS);
    assert!(text.contains("CLIP 1 (at 00:00 in the video, 10.0 s): session 00:00\u{2013}00:10"), "{text}");
    assert!(!text.contains("the source runs at"), "nothing to say about a rate: {text}");
    assert!(!text.contains("MARKED"), "{text}");

    let sped = up::BriefClip {
        rate: 4.0,
        marked: vec!["MARKED: the reveal".into(), "ZOOM: the face".into()],
        ..plain
    };
    let shown = up::brief(&[sped], &[], up::BRIEF_MAX_CHARS);
    assert!(shown.contains(", the source runs at 4x here"), "{shown}");
    assert!(shown.contains("  MARKED: MARKED: the reveal\n"), "{shown}");
    assert!(shown.contains("  MARKED: ZOOM: the face\n"), "{shown}");

    // The words come from ONE rule, so both briefs spell a zoom the same way.
    let marks = up::brief_marks(&[fx("zoom", 5.0, 3.0, "the face")], &seg(0.0, 10.0));
    assert_eq!(marks, vec!["ZOOM: the face".to_string()]);
    // A caption is the viewer's text, not a decoration: this writer is not placing subtitles.
    assert!(up::brief_marks(&[fx("text", 5.0, 3.0, "read on screen")], &seg(0.0, 10.0)).is_empty());
    // A decoration on another clip is not this clip's.
    assert!(up::brief_marks(&[fx("zoom", 50.0, 3.0, "later")], &seg(0.0, 10.0)).is_empty());
    // A rate of nought is a stop: the source runs on at 1x under the still.
    assert_eq!(up::brief_rate(&seg(0.0, 10.0)), 1.0);
    assert_eq!(up::brief_rate(&Seg { s: 0.0, e: 10.0, rate: 2.0, ..Default::default() }), 2.0);
}

/// Decision 31: the word ceiling was printed in the brief and never counted. The tool now answers the count
/// against the ceiling, so a long line can be cut here.
#[test]
fn sec_12_decisions_narrate_and_produce_s31_the_word_count_is_answered_against_the_ceiling() {
    let clips = [seg(0.0, 40.0)];
    let ceiling = pass::word_ceiling(40.0);
    let answer = body(&pass::write_line(&clips, 1, 1.0, "one two three four five", "", ""));
    assert_eq!(answer["words"], 5);
    assert_eq!(answer["at_most_words"], ceiling, "the same ceiling the heading promised");
    assert!(answer["at_most_words"].as_u64().unwrap() >= 8, "P.policy.narrationMinWords floors it");
    // The offset is clamped inside the clip, never into its last second, and the answer says where it went.
    let late = body(&pass::write_line(&clips, 1, 39.5, "late", "", ""));
    assert_eq!(late["at"], 39.0, "never the last second: {late}");
    assert!(body(&pass::write_line(&clips, 1, -5.0, "early", "", ""))["at"] == 0.0);
}

/// Decision 32: a named thumbnail frame resolved to the nearest extracted frame AT ANY DISTANCE and the log
/// printed the frame's time, not the request's. The distance is now the answer's own field.
#[test]
fn sec_12_decisions_narrate_and_produce_s32_a_far_frame_is_named_with_the_distance_it_moved() {
    // Frames exist only every P.project.frameInterval seconds: clip 1 starts at +20 s, so "clip 1 +12" asks
    // for +32 s, and the nearest extracted frame is twenty seconds away at +12 s.
    let picked = up::pick_frame(1, 12.0, &[20.0], &[2.0, 12.0, 62.0]).expect("clip 1 exists");
    assert_eq!(picked.asked, 32.0);
    assert_eq!(picked.taken, 12.0);
    assert_eq!(picked.distance, 20.0, "twenty seconds away, and SAID");
    assert!(picked.own, "a picked frame is the thumbnail as it is");
    assert!(picked.answer.contains("20.0 s"), "{}", picked.answer);
    assert!(picked.answer.contains("nothing is drawn"), "and the writer is told that too: {}", picked.answer);
    // Ties go to the earlier frame: deterministic, and the earlier one is already on screen.
    assert_eq!(up::nearest(&[10.0, 20.0], 15.0), Some(10.0));
    // Nothing extracted is a refusal, not a silent fallback to a generated picture.
    assert_eq!(up::pick_frame(1, 12.0, &[0.0], &[]).err().unwrap(), "no frames have been extracted");
    assert!(up::pick_frame(7, 1.0, &[0.0], &[1.0]).is_err(), "a clip not in the cut is refused");
}

/// Decision 33: naming a frame cleared the instruction and an unresolvable frame left neither. Two tools now,
/// each with its own answer, and a conflict is an error instead of a silent clearing.
#[test]
fn sec_12_decisions_narrate_and_produce_s33_an_instruction_after_a_picked_frame_is_an_error() {
    // With no frame chosen, the instruction is set and answered.
    assert!(up::set_thumbnail_instruction(false, "make it dusk", None).is_ok());
    assert!(up::set_thumbnail_instruction(false, "x", Some("no text")).unwrap().contains("negative"));
    // Once a frame is picked the picture is that frame, so an instruction would silently discard it.
    let why = up::set_thumbnail_instruction(true, "make it dusk", None).err().expect("refused");
    assert!(why.contains("a frame was already picked") && why.contains("clear it"), "{why}");
    // And an empty instruction is refused with the sentence that names the way out, not accepted quietly.
    assert_eq!(up::set_thumbnail_instruction(false, "   ", None).err().unwrap(), up::NO_INSTRUCTION);
    // `pick_frame` says nothing is drawn, which is the other half of the same pair (§3.9).
    assert!(up::pick_frame(1, 0.0, &[0.0], &[0.0]).unwrap().answer.contains("nothing is drawn"));
}

/// Decision 34: the title was printed onto the picture only if it was the first title to exist, so a
/// re-suggested title and the thumbnail disagreed. It reprints whenever the text changes, until the user has
/// their own picture.
#[test]
fn sec_12_decisions_narrate_and_produce_s34_a_changed_title_reprints_until_the_picture_is_the_users() {
    let box_ = TitleBox { cx: 0.5, cy: 0.15, wf: 0.8, hf: 0.2 };
    let held = Publish { title: "the first title".into(), ..Default::default() };
    // Same text: nothing to reprint, so a run does not redraw a picture that already matches.
    assert!(!up::set_title(&held, "the first title", &box_, false).changed);
    // Changed: reprint.
    assert!(up::set_title(&held, "a better title", &box_, false).changed);
    // An own frame or a hand-written edit instruction makes the picture theirs, and it stays theirs however
    // many times the title is re-suggested.
    assert!(!up::set_title(&held, "a better title", &box_, true).changed);
}

/// Decision 35: a title too long for its band overflowed rather than being refused, and "four to seven
/// words" was unenforced advice. It keeps its words AND warns, and answers the size they take.
#[test]
fn sec_12_decisions_narrate_and_produce_s35_a_long_title_keeps_its_words_and_warns() {
    let box_ = TitleBox { cx: 0.5, cy: 0.15, wf: 0.8, hf: 0.2 };
    let held = Publish::default();
    let short = up::set_title(&held, "four short words", &box_, false);
    assert!(!short.overflows, "under the advice limit: {}", short.answer);
    assert!(short.size > 0.0, "the fitted type size is answered: {short:?}");
    // The count is of whitespace-separated words, which is what "four to seven words" means to a writer.
    assert_eq!(short.words, "four short words".split_whitespace().count());
    let long_words = (0..12).map(|i| format!("word{i}")).collect::<Vec<_>>().join(" ");
    let long = up::set_title(&held, &long_words, &box_, false);
    assert!(long.words > up::OVERFLOW_WORDS as usize);
    // Never refused: the writer's words survive, and the warning is what makes the advice enforceable.
    assert!(long.overflows, "over seven words overflows WITH a warning: {}", long.answer);
    assert!(long.answer.contains("overflows the band"), "{}", long.answer);
    assert!(long.answer.contains(&format!("over {}", up::OVERFLOW_WORDS)), "{}", long.answer);
    // A longer line means smaller letters: one fitter answers the size, so the sizes must differ.
    assert!(long.size < short.size, "the band fills at {}px vs {}px", long.size, short.size);
}

/// Decision 36 (upload S8): a missing `TITLE:` or `THUMBNAIL:` keeps the previous value, which attributes
/// last run's answer to this one — deliberate, because an empty box is easier to notice than a wrong line,
/// and now STATED: the log names what came over from last run.
#[test]
fn sec_12_decisions_narrate_and_produce_s36_a_missing_title_keeps_the_previous_and_the_says_so() {
    // A reply with only a description: the two labelled lines are simply absent.
    let (title, thumbnail, description) = up::peel("the description body, written fresh this run.");
    assert!(title.is_none() && thumbnail.is_none(), "{title:?} {thumbnail:?}");
    assert_eq!(description.expect("the description always comes through"), "the description body, written fresh this run.");

    // What is missing is therefore what `finish` would complain about, and the run logs that it kept the old.
    let missing = up::finish_needs("", "");
    assert_eq!(missing, vec!["title", "description"]);
    let log = up::kept_previous_log(&missing);
    assert!(log.contains("no title or description in the answer"), "{log}");
    assert!(log.contains("keeping what was there before"), "deliberate and stated: {log}");
    // A record that already has both asks for nothing: keeping them is not a failure.
    assert!(up::finish_needs("a title", "a description").is_empty());
}

/// Decision 37: the placement tag travelled into the translation as a word and the model's own line breaks
/// were re-wrapped at 42 characters against the prompt's instruction. The app holds the tag out and puts it
/// back, and wrapping never drops a word.
#[test]
fn sec_12_decisions_narrate_and_produce_s37_the_tag_stays_out_of_the_translation_and_two_rows_hold_it() {
    // Out: no tag, one numbered line, the model's breaks folded so one cue is one entry.
    let cue = sub::Cue { s: 0.0, e: 4.0, text: "two rows\nof words".into(), pos: "top".into() };
    let sent = sub::sent_text(&cue);
    assert!(!sent.contains(r"{\an8}"), "the tag is not the model's business: {sent}");
    assert!(!sent.contains('\n'), "one cue, one numbered line: {sent}");
    assert!(sent.contains('/'), "the break is remembered as a marker, not lost: {sent}");

    // Back: the tag is put on by the app, after the wrap, from what the app knows it placed.
    let sheet = sub::srt_text(&[sub::Cue { s: 0.0, e: 4.0, text: "a line".into(), pos: "top".into() }]);
    assert!(sheet.contains(&format!("{{\\an8}}a line")), "{sheet}");
    assert_eq!(sub::placement("center"), Some(r"{\an5}"));
    assert_eq!(sub::placement(""), None, "bottom stores nothing");

    // Wrapping: at most two rows of P.policy.subtitleRowChars, and no word ever dropped.
    let words: Vec<String> = (1..=40).map(|i| format!("w{i}")).collect();
    let long = words.join(" ");
    let wrapped = sub::wrap(&long);
    let rows: Vec<&str> = wrapped.lines().collect();
    assert_eq!(rows.len(), 2, "two rows is what a player shows: {wrapped}");
    for row in &rows {
        // The rule the wrap can hold is measured against the id §10 gives, not a literal.
        assert!(row.chars().count() <= sub::ROW_CHARS || row.split_whitespace().count() > 1, "{row}");
    }
    assert_eq!(wrapped.replace('\n', " ").split_whitespace().count(), 40, "every word survives");
    // A cue the model wrapped itself is left exactly as it was: re-wrapping contradicts the prompt.
    assert_eq!(sub::wrap("kept as\nwritten"), "kept as\nwritten");
    assert_eq!(sub::wrap(""), "");
}

/// Decision 38: a line that could not be translated was silently replaced with the original language and
/// shipped in the other language's track. Keeping it is right — one bad line must not drop a track — but it
/// has to be NAMED.
#[test]
fn sec_12_decisions_narrate_and_produce_s38_an_untranslated_line_keeps_its_original_and_is_named() {
    // An unasked number and an empty text are both refused by the parser, so the line simply stays missing.
    let asked = [1usize, 2, 3];
    let got = naivepost::produce_translate_ask::parse_numbered("1\tankommen\n3\t\n9\tforeign line\n", &asked);
    assert_eq!(got.get(&1).map(String::as_str), Some("ankommen"));
    assert!(!got.contains_key(&3), "an empty text is no translation");
    assert!(!got.contains_key(&9), "a number outside this batch is refused, not guessed at");

    // The merge ships the original for what stayed missing AND names the numbers in its warning.
    let original = vec![(1usize, "arrival".to_string()), (2, "departure".to_string()), (3, "return".to_string())];
    let merged = sub::merge("de", &original, &got);
    assert_eq!(merged.lines[0], "ankommen");
    assert_eq!(merged.lines[1], "departure", "kept, because dropping a track for one line is worse");
    assert_eq!(merged.still_missing, vec![2, 3]);
    let warning = merged.warning.expect("and it is named, not silent");
    assert!(warning.contains("stayed in the original"), "{warning}");
    assert!(warning.contains("2, 3"), "both numbers, so a reader knows which line: {warning}");

    // A complete batch has nothing to warn about, and completeness is also what earns a cache place.
    let all: BTreeMap<usize, String> = [(1, "a".to_string()), (2, "b".to_string()), (3, "c".to_string())].into_iter().collect();
    let done = sub::merge("de", &original, &all);
    assert!(done.warning.is_none() && done.still_missing.is_empty());
    assert!(sub::cached(true) && !sub::cached(false), "a partial answer cached is partial forever");
}
