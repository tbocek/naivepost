//! §07-narrate#6 — Details confirmed against the code (verification pass).
//!
//! One test per bullet of §6 (`spec/07-narrate.md:248-257`), named
//! `sec_07_narrate_6_details_confirmed_against_the_code_verification_pass_sN…`: the previous narration's
//! log line and the captions short-circuit, what an unusable reply says and how entries are matched, what
//! the Inputs row and its tooltip read, the re-roll and sample refusals, the reference build's log, how an
//! added file is named, what the take band states, and the preview's hold bound.
//!
//! Ids cited: `P.machine.narrationContextSeconds`, `P.policy.narrationMinWords`/`MaxWords`,
//! `P.eng.takeMinSeconds`, `P.eng.narrationMaxExtendSeconds`, `tool:ffmpeg.pcm`.

use naivepost::cut::Seg;
use naivepost::narrate_data;
use naivepost::narrate_details as det;
use naivepost::narrate_pass as pass;
use naivepost::narrate_preview::{self, Take, Tick};
use naivepost::narrate_rules as rules;
use naivepost::narrate_screen as screen;
use naivepost::narrate_reply as reply;
use naivepost::narration::{Entry, Narration, Silent};
use naivepost::params::Param;

const ITEM: &str = "sec_07_narrate_6_details_confirmed_against_the_code_verification_pass";

fn seg(s: f64, e: f64) -> Seg {
    let mut clip = Seg::default();
    clip.s = s;
    clip.e = e;
    clip.dur = e - s;
    clip.ins = "media/clip.mp4".to_string();
    clip
}

fn entry(s: f64, e: f64, at: f64, text: &str) -> Entry {
    let mut line = Entry::default();
    line.s = s;
    line.e = e;
    line.at = at;
    line.text = text.to_string();
    line
}

/// §10's own spelling of one id, from the catalogue.
fn spelled(rows: &[Param], id: &str) -> Option<String> {
    rows.iter().find(|row| row.id == id).map(|row| row.spelled.clone())
}

// ---- s1 [F4.1]: previous file kept once a valid narration came back ---------------

#[test]
fn sec_07_narrate_6_details_confirmed_against_the_code_verification_pass_s1_previous_and_silent_and_captions()
{
    assert_eq!(ITEM, "sec_07_narrate_6_details_confirmed_against_the_code_verification_pass");
    // §6: the log names where the overwritten narration is kept. Four leading spaces because it hangs
    // under the step's own line rather than starting one.
    assert_eq!(
        det::previous_kept_log("proj.naivepost/narrate/narration.prev.json"),
        "    the narration it replaced is kept at proj.naivepost/narrate/narration.prev.json"
    );

    // The file copy itself is F4.1's round (narrate_data::keep_previous needs a project tree, which this
    // headless test does not build). What §6 confirms about the replacement is that the deliberate silence
    // goes with it: a silent marker alone says "nothing said here", and one real entry on the same bounds
    // makes it a clip with a line — the marker never outlives the narration it was written against.
    let record = Narration {
        entries: vec![entry(0.0, 10.0, 0.0, "the replacement's own line")],
        silent: vec![Silent { s: 0.0, e: 10.0 }],
    };
    assert!(record.is_silent(0.0, 10.0));
    assert!(record.has_line(0.0, 10.0));

    // §6 [F4.1]: a captions-only voice still opens and closes the speaking job so the bar finishes — it
    // speaks nothing (rules::speaks_anything is false) yet the bar is driven by narrate_pass::progress,
    // which counts clips, not lines. progress(2,2) is the close.
    assert!(rules::voice_is_captions(naivepost::narrate_screen::CAPTIONS));
    assert!(!rules::speaks_anything(naivepost::narrate_screen::CAPTIONS, false));
    assert_eq!(pass::progress(2, 2), "writing 2/2 clips");
}

// ---- s2 [F4.2] validation: every clip answered, entries matched forward only -------

#[test]
fn sec_07_narrate_6_details_confirmed_against_the_code_verification_pass_s2_validation_forward_only() {
    assert_eq!(ITEM, "sec_07_narrate_6_details_confirmed_against_the_code_verification_pass");
    // Half a second of slack is all a reply gets, because the bounds are echoed from the request rather
    // than estimated by the model.
    assert_eq!(reply::ENTRY_MATCH_TOLERANCE_SECONDS, 0.5);

    // A skipped clip and an entry that matches nothing (§6's two sentences), plus the all-empty refusal.
    let segs = [seg(0.0, 10.0), seg(10.0, 20.0), seg(20.0, 30.0)];
    assert_eq!(
        reply::clip_without_entry(2, &segs),
        "clip 3 (20.0-30.0) got no entry"
    );
    assert_eq!(pass::missing_clip_fault(3, 20.0, 30.0), "clip 3 (20.0-30.0) got no entry");
    assert_eq!(reply::all_silent_fault(), "every clip came back with no line at all");
    assert_eq!(pass::no_line_fault(), reply::all_silent_fault());
    assert_eq!(
        reply::unmatched_entry_fault(12.4, 18.9),
        "an entry says 12.4-18.9, which matches no clip (or is out of order)"
    );

    // Forward-only: the fault says "or is out of order" because a reply that walks back to an earlier clip
    // matches nothing and is refused rather than quietly re-sorted.
    assert!(reply::unmatched_entry_fault(0.0, 10.0).contains("out of order"));

    // The refusal the run logs: attempt number plus the fault, so a retry reads as a redo of that reason.
    assert_eq!(
        pass::rejected_log(2, &pass::missing_clip_fault(3, 20.0, 30.0)),
        ">>> narration attempt 2 rejected: clip 3 (20.0-30.0) got no entry"
    );

    // Every line has to survive the word budget (§10's floor and ceiling per second of video).
    assert_eq!(pass::word_ceiling(5.0), 8);
    assert_eq!(pass::word_ceiling(120.0), 30);
    let rows = naivepost::params::narrate();
    assert_eq!(spelled(&rows, "P.policy.narrationMinWords").as_deref(), Some("8"));
    assert_eq!(spelled(&rows, "P.policy.narrationMaxWords").as_deref(), Some("30"));
    assert_eq!(pass::MIN_WORDS as f64 * pass::WORDS_PER_SECOND / pass::WORDS_PER_SECOND, 8.0);

    // Several entries on one clip play in offset order whatever order they arrived in — put back by
    // Narration::sort on every read and write.
    let mut record = Narration::default();
    record.entries = vec![
        entry(10.0, 20.0, 1.0, "the later clip"),
        entry(0.0, 10.0, 4.0, "said second in the first clip"),
        entry(0.0, 10.0, 1.0, "said first"),
    ];
    record.sort();
    let at: Vec<f64> = record.entries.iter().map(|line| line.at).collect();
    assert_eq!(at, vec![1.0, 4.0, 1.0]);

    // §6's "with tools, these are finish's answers": a reply is refused before anything is written, so the
    // faults above are all a caller can get. entry_at keeps a placement the model gave and defaults to the
    // clip's head when it gave none.
    assert_eq!(reply::entry_at(&entry(0.0, 10.0, 2.5, "placed"), &seg(0.0, 10.0)), 2.5);
    assert_eq!(reply::entry_at(&entry(0.0, 10.0, 0.0, ""), &seg(0.0, 10.0)), 0.0);
}

// ---- s3: the Inputs tooltip names its inputs -------------------------------------

#[test]
fn sec_07_narrate_6_details_confirmed_against_the_code_verification_pass_s3_inputs_tooltip() {
    assert_eq!(ITEM, "sec_07_narrate_6_details_confirmed_against_the_code_verification_pass");
    // The row: N clip(s) · mm:ss, and the two counts ▶ would fix after a ⚠.
    assert_eq!(det::clips_line(2, 120.0), "2 clips \u{b7} 02:00");
    assert_eq!(det::clips_line(1, 5.0), "1 clip \u{b7} 00:05");
    assert_eq!(det::stale_mark(3, 2), "3 clips unwritten, 2 lines off the cut");
    assert_eq!(det::stale_mark(1, 0), "1 clip unwritten");
    assert_eq!(det::stale_mark(0, 1), "1 line off the cut");
    assert_eq!(det::stale_mark(0, 0), "");
    assert_eq!(
        det::inputs_line(4, 95.0, 3, 2, true, true),
        "4 clips \u{b7} 01:35 \u{b7} \u{26a0} 3 clips unwritten, 2 lines off the cut"
    );
    // A missing timeline is a missing input, so it goes on the line rather than into the tooltip.
    assert_eq!(
        det::inputs_line(4, 95.0, 0, 0, false, true),
        "4 clips \u{b7} 01:35 \u{b7} no timeline"
    );
    assert_eq!(det::inputs_line(0, 0.0, 0, 0, true, false), det::NO_CUT_LINE);

    // The tooltip rows, each in §6's own words. The transcript row quotes the ± rule with its §10 number.
    assert_eq!(det::CONTEXT_WINDOW_ID, "P.machine.narrationContextSeconds");
    assert_eq!(pass::CONTEXT_SECONDS, 4.0);
    assert_eq!(
        det::timeline_line(688).unwrap(),
        "prepare/transcript/session.tsv \u{2014} 688 lines; the ones falling inside a clip (\u{b1}4 s) go with that clip"
    );
    assert_eq!(det::timeline_line(0), None);
    assert_eq!(det::cut_detail(2, 120.0), "cut/cut.json \u{2014} 2 clips, 02:00 of video to write for");
    assert_eq!(
        det::voice_line("Mira", -1.5),
        "Spoken by Mira at -1.5 semitones (narrate/voice_ref.wav)"
    );

    // The whole tooltip, in §6's order: the cut file, why it is stale and what fixes it, the transcript,
    // the session context verbatim, the voice. Rows separated by a blank line.
    let tip = det::inputs_tooltip(2, 120.0, "the cut changed", 688, "a demo of the app", Some(("Mira", -1.5)));
    let rows: Vec<&str> = tip.split("\n\n").collect();
    assert_eq!(rows.len(), 5, "{tip}");
    assert_eq!(rows[0], det::cut_detail(2, 120.0));
    assert_eq!(rows[1], "\u{26a0} the cut changed \u{2014} \u{25b6} writes the narration again");
    assert_eq!(rows[2], det::timeline_line(688).unwrap());
    assert_eq!(
        rows[3],
        "Session context (Describe), sent with the narration:\na demo of the app"
    );
    assert_eq!(rows[4], det::voice_line("Mira", -1.5));

    // With no cut there is no clip row and no voice: an empty tooltip rather than a row of guesses.
    assert_eq!(det::inputs_tooltip(0, 0.0, "", 0, "", None), "");

    // The page's own readout is the same row (§1). It writes the counts parenthetically — "4 clip(s)",
    // "2 line(s)" — where det::inputs_line writes them as the prototype's plural helper does ("4 clips",
    // "2 lines"). Same shape, same ⚠, same "no timeline"; only that spelling differs, so the two are
    // pinned side by side instead of asserted equal.
    assert_eq!(
        screen::inputs_readout(4, 95.0, 3, 2, true, true),
        "4 clip(s) \u{b7} 01:35 \u{b7} \u{26a0} 3 clip(s) unwritten, 2 line(s) off the cut"
    );
    assert_eq!(screen::inputs_readout(4, 95.0, 0, 0, false, true), "4 clip(s) \u{b7} 01:35 \u{b7} no timeline");
    assert_eq!(screen::inputs_readout(0, 0.0, 0, 0, true, false), det::NO_CUT_LINE);
    // Outputs names the three things this page owns.
    assert_eq!(
        screen::outputs_readout(),
        "narrate/ \u{2014} narration.json, the voice reference and the synthesis cache"
    );
}

// ---- s4: re-roll and sample refusals ---------------------------------------------

#[test]
fn sec_07_narrate_6_details_confirmed_against_the_code_verification_pass_s4_reroll_and_sample_refusals() {
    assert_eq!(ITEM, "sec_07_narrate_6_details_confirmed_against_the_code_verification_pass");
    // §6's own sentences, one per reason. A re-roll with nothing to re-roll is the page's; overwriting a
    // take somebody heard needs the tick, and the tick never defaults on (§5).
    assert_eq!(screen::re_roll_refusal(3, false), Some("clip 3 has no line to re-roll".to_string()));
    assert_eq!(screen::re_roll_refusal(3, true), None);
    assert_eq!(det::re_roll_refusal(true, false), screen::replace_refusal(true, false));
    assert_eq!(
        det::re_roll_refusal(true, false).unwrap(),
        "this narration was already heard or edited \u{2014} tick \u{201c}Replace\u{201d} to write another take over it"
    );
    assert_eq!(det::re_roll_refusal(false, false), None);
    assert_eq!(det::re_roll_refusal(true, true), None);
    assert!(!det::replace_tick_default());

    // ↻ accepted, and the captions case where nothing has a take at all.
    assert_eq!(det::new_take_status(0), "line 1: new take, speaking it");
    assert_eq!(
        det::re_roll_captions_refusal(),
        "no audio is chosen \u{2014} a caption has no take to re-roll"
    );

    // The four ways ▶ Sample refuses — each wants a different click, so each is its own sentence, and none
    // of them opens a dialog.
    assert_eq!(det::pick_a_voice_refusal(), "pick a voice first");
    assert_eq!(
        det::no_voice_to_sample_refusal(),
        "no audio is chosen \u{2014} there is no voice to sample"
    );
    assert_eq!(det::empty_sample_refusal(), "type a sample sentence to hear the voice");
    assert_eq!(det::busy_sample_status(), "still synthesizing the last sample\u{2026}");

    // Its three playing states: the row's ▶ is what resumes or starts over, hence the icons.
    assert_eq!(det::sample_status(), "synthesizing the sample\u{2026}");
    assert_eq!(det::sample_playing_status(), "sample playing");
    assert_eq!(
        det::sample_paused_status(),
        "sample paused \u{2014} \u{25b6} resumes, \u{23f9} starts over"
    );
    assert_eq!(det::sample_stopped_status(), "sample stopped");

    // The log a sample leaves before anything can go wrong, and after: which voice at what pitch in what
    // words, then what came back. A line's first take is not named; its second one is.
    assert_eq!(
        det::sample_log("Mira", -1.5, None, "one sentence"),
        ">>> sample: Mira at -1.5 semitones \u{2014} \"one sentence\""
    );
    assert_eq!(
        det::sample_log("Mira", 0.0, Some(1), "one sentence"),
        ">>> sample take 2: Mira at +0.0 semitones \u{2014} \"one sentence\""
    );
    assert_eq!(
        det::sample_file_log("voice_ref-1.wav", 61_440, Some("0.4s")),
        "    sample: voice_ref-1.wav (60 kB, spoken in 0.4s)"
    );
    assert_eq!(
        det::sample_file_log("voice_ref-1.wav", 61_440, None),
        "    sample: voice_ref-1.wav (60 kB, spoken earlier)"
    );
    // A server that answers with a header and no audio is the failure the page cannot show — so the log says
    // how few bytes came back.
    assert_eq!(
        det::sample_too_small_log("voice_ref-1.wav", 44),
        "!!! sample: voice_ref-1.wav is 44 bytes \u{2014} no audio came back"
    );
    // The cold-start sentence §1 gives this one without a line number.
    assert_eq!(screen::speaking_line(), "synthesizing\u{2026} (first line after a cold start also loads the model)");
}

// ---- s5: the reference build's log ------------------------------------------------

#[test]
fn sec_07_narrate_6_details_confirmed_against_the_code_verification_pass_s5_reference_build_log() {
    assert_eq!(ITEM, "sec_07_narrate_6_details_confirmed_against_the_code_verification_pass");
    // §6 writes the line as "X s, N words | N hand-picked take(s) from <base>": the bar separates the two
    // kinds of reference, because they fail differently — an automatic one that sounds wrong is a ranking
    // to overrule by hand, a hand-picked one is seconds to re-pick. So exactly one half appears.
    assert_eq!(
        det::reference_build_log(13.4, 41, 0, "rec-2025-08-14"),
        ">>> voice reference built: 13.4 s, 41 words from rec-2025-08-14"
    );
    assert_eq!(
        det::reference_build_log(13.4, 41, 3, "rec-2025-08-14"),
        ">>> voice reference built: 13.4 s, 3 hand-picked take(s) from rec-2025-08-14"
    );

    // A pasted recording that is not mono PCM yet is converted by ffmpeg (tool:ffmpeg.pcm); this page only
    // says why.
    assert_eq!(
        det::reference_convert_log("guest.wav"),
        "guest.wav is not mono PCM \u{2014} converting it with ffmpeg"
    );

    // The automatic ranking behind the line — the speaker with the most turn time, solo turns (nobody else
    // within P.eng.refPadSeconds), narrowed to their words and ranked by words said with length as
    // tie-break, takes under P.eng.refMinWordsPerSecond dropped unless that leaves none — is F4.6's
    // decision and deliberately not re-implemented or restated here. Those ids stay out of the catalogue on
    // purpose (see params::narrate()'s comment on the five automatic-reference values), which this asserts
    // so a later round cannot catalogue one by accident.
    let rows = naivepost::params::narrate();
    assert_eq!(spelled(&rows, "P.machine.refWantSeconds"), None);
    assert_eq!(spelled(&rows, "P.eng.refPadSeconds"), None);
    assert_eq!(spelled(&rows, "P.eng.refMinWordsPerSecond"), None);
    // The one rule that does read the wanted length is the take band's own status line.
    assert_eq!(det::REFERENCE_WANTED_SECONDS, 14.0);
}

// ---- s6: "Add file…" never overwrites --------------------------------------------

#[test]
fn sec_07_narrate_6_details_confirmed_against_the_code_verification_pass_s6_add_file_never_overwrites() {
    assert_eq!(ITEM, "sec_07_narrate_6_details_confirmed_against_the_code_verification_pass");
    // §6: the sanitised base name first, and only a collision adds an index — starting at -2, because the
    // file name IS the voice id in every project's cache keys.
    assert_eq!(det::added_file_name("guest", &[]), "guest");
    let one = vec!["guest".to_string()];
    assert_eq!(det::added_file_name("guest", &one), "guest-2");
    let two = vec!["guest".to_string(), "guest-2".to_string()];
    assert_eq!(det::added_file_name("guest", &two), "guest-3");
    // A name nobody took is not indexed even when its neighbours are.
    let gaps = vec!["guest-2".to_string()];
    assert_eq!(det::added_file_name("guest", &gaps), "guest");

    // The sanitiser that produced the stem: letters, digits, '-', '_' and '.' survive, everything else
    // becomes '-', both ends lose what they gained — a slash would write the file somewhere else entirely.
    assert_eq!(narrate_data::sanitize_voice_id("My Voice 2025"), "My-Voice-2025");
    assert_eq!(narrate_data::sanitize_voice_id("/tmp/secret/rec one.wav"), "tmp-secret-rec-one.wav");

    // The names all live in the folder §4 lists.
    let files = narrate_data::DATA_FILES;
    assert_eq!(files.len(), 9);
    assert_eq!(&files[..3], ["narration.json", "narration.prev.json", "voice.txt"]);
    assert!(files.contains(&"voice_ref.wav"));
}

// ---- s7: the take band's messages -------------------------------------------------

#[test]
fn sec_07_narrate_6_details_confirmed_against_the_code_verification_pass_s7_take_band_messages() {
    assert_eq!(ITEM, "sec_07_narrate_6_details_confirmed_against_the_code_verification_pass");
    // §6's seven sentences, verbatim. The full-width ＋ and － are the buttons' own glyphs.
    assert_eq!(
        det::take_add_hint(),
        "drag across the wave first \u{2014} \u{ff0b} makes the selection a take"
    );
    assert_eq!(
        det::take_remove_hint(),
        "drag across the takes you want gone \u{2014} \u{ff0d} removes those seconds"
    );
    assert_eq!(det::nothing_picked(), "nothing is picked in those seconds");
    assert_eq!(
        det::no_takes_left(),
        "no takes left \u{2014} the seconds are chosen for you again on the next line spoken"
    );
    assert_eq!(det::no_takes_yet(), "no takes yet \u{2014} drag across the wave and press \u{ff0b}");
    assert_eq!(det::takes_played(), "takes played");
    assert_eq!(det::takes_stopped(), "stopped playing the takes");

    // While they play: how many, for how long, from where — and the one case a listener would misread as a
    // resume, a recording with nothing picked after it.
    assert_eq!(det::playing_takes(3, 27.4, 61.0), "playing 3 take(s), 27.4 s from 01:01");
    assert_eq!(
        det::playing_takes(0, 0.0, 5.0),
        "playing the recording from 00:05 \u{2014} nothing is picked after it"
    );

    // After a ＋ or －: the total is in it because it is the number that decides whether this is a voice,
    // judged against the length the automatic pick aims for.
    let status = det::commit_status("picked", 3, 12.7, det::REFERENCE_WANTED_SECONDS);
    assert_eq!(
        status,
        "picked \u{2014} 3 take(s), 12.7 s of reference (14 s is plenty). Re-cut on the next line spoken."
    );

    // A take under P.eng.takeMinSeconds is a breath rather than a voice; the row's own floor is §10's.
    assert_eq!(screen::TAKE_MIN_SECONDS, 0.4);
    assert_eq!(spelled(&naivepost::params::narrate(), "P.eng.takeMinSeconds").as_deref(), Some("0.4"));

    // takes.json is keyed by the RECORDING's base name, not the narrator's slot, so re-tagging never moves
    // anyone's takes. save_takes / drop_reference own the file; this pins the key they write it under.
    assert_eq!(
        narrate_data::take_base(std::path::Path::new("/media/rec-2025-08-14.wav")),
        "rec-2025-08-14"
    );
    assert_eq!(
        narrate_data::take_base(std::path::Path::new("project:/media/rec-2025-08-14.wav")),
        "rec-2025-08-14"
    );
}

// ---- s8: the preview's bounds -----------------------------------------------------

#[test]
fn sec_07_narrate_6_details_confirmed_against_the_code_verification_pass_s8_preview_bounds() {
    assert_eq!(ITEM, "sec_07_narrate_6_details_confirmed_against_the_code_verification_pass");
    // A gap is held only while the speaking line is within its clip's end + narrationMaxExtend; past the
    // last clip both players pause; a line's wav starts at the offset into it.
    let segs = [seg(0.0, 10.0), seg(12.0, 20.0)];
    let entries = [entry(0.0, 10.0, 1.0, "first"), entry(12.0, 20.0, 0.5, "second")];
    let taken = [Take::Exists, Take::Exists];

    // Entering a line whose take exists starts its wav at the offset into it. `speaking` is what the tick
    // reported last time, and it only suppresses the answer for the seconds BEFORE the line begins: once
    // the head is inside the line's own seconds the caller is told again, with the offset from there —
    // which is what lets a seek into a playing line re-cue it instead of leaving it silent.
    assert!(matches!(
        narrate_preview::tick(3.0, &segs, &entries, &taken, None),
        Tick::Speak { line: 0, offset: 2.0 }
    ));
    assert!(matches!(
        narrate_preview::tick(3.5, &segs, &entries, &taken, Some(0)),
        Tick::Speak { line: 0, offset: 2.5 }
    ));
    // Idle is a clip with no line over it, or a second before any line's own start.
    assert!(matches!(
        narrate_preview::tick(13.0, &segs, &entries[..1], &taken, None),
        Tick::Idle
    ));
    // No take yet: the picture waits and the line is synthesized rather than skipped past; a take that
    // already failed runs the clip mute instead of asking again.
    let none = [Take::Missing, Take::Missing];
    assert!(matches!(
        narrate_preview::tick(3.0, &segs, &entries, &none, None),
        Tick::Synthesize { line: 0 }
    ));
    let failed = [Take::Failed, Take::Failed];
    assert!(matches!(
        narrate_preview::tick(3.0, &segs, &entries, &failed, None),
        Tick::Mute { line: 0 }
    ));
    // A gap the cut removed is skipped forward to where it starts again...
    assert!(matches!(
        narrate_preview::tick(11.0, &segs, &entries, &taken, None),
        Tick::SkipTo(12.0)
    ));
    // ...unless a line is still speaking within the bound, which holds the boundary rather than dropping
    // the middle of a sentence.
    assert!(matches!(
        narrate_preview::tick(11.0, &segs, &entries, &taken, Some(0)),
        Tick::Hold
    ));
    // Past the last clip: both players pause.
    assert!(matches!(narrate_preview::tick(25.0, &segs, &entries, &taken, None), Tick::Pause));

    // The bound is P.eng.narrationMaxExtendSeconds and one value, not a copy per module.
    assert_eq!(narrate_preview::MAX_EXTEND_SECONDS, 4.0);
    assert_eq!(det::MAX_EXTEND_SECONDS, narrate_preview::MAX_EXTEND_SECONDS);
    assert_eq!(det::preview_hold_bound(), det::MAX_EXTEND_SECONDS);

    // ⏹ stops both players and hands ▶ back to the run; a clip no recording covers speaks alone.
    let stopped = narrate_preview::stop();
    assert!(stopped.picture_paused && stopped.voice_paused);
    assert!(!stopped.started);
    assert_eq!(stopped.play_button, "\u{25b6}");
    assert!(screen::speaks_alone_off_the_tick(true));
    assert!(!screen::speaks_alone_off_the_tick(false));
}
