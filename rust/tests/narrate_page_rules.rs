// §07-narrate#5-rules — spec/07-narrate.md §5, one test per clause and in §5's order.
//
// §5 is a paragraph of standing rules rather than a flow, so each test proves the rule holds through the module
// that owns it: the record sorts itself, the refit moves lines onto the cut's numbers, the preview asks the render
// its sound questions. Only the clauses nothing else had written down live in naivepost::narrate_rules, and those
// are exercised here too — a rule restated twice is a rule that can be changed in one place only.

use std::path::{Path, PathBuf};

use naivepost::cut::{Cut, Fx, Seg};
use naivepost::cut_hear;
use naivepost::narrate_data::{self, Take};
use naivepost::narrate_off;
use naivepost::narrate_preview as view;
use naivepost::narrate_rules as rules;
use naivepost::narrate_screen as page;
use naivepost::narration::{self, Entry, Narration, Silent};
use naivepost::params;
use naivepost::run::{self, Transport};
use naivepost::shell::{self, Page};

fn tree(tag: &str) -> (PathBuf, naivepost::layout::Tree) {
    let root = std::env::temp_dir().join(format!(
        "naivepost-narraterules-{tag}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    let dir = root.join("demo.naivepost");
    std::fs::create_dir_all(&dir).unwrap();
    (root, naivepost::layout::Tree::new(&dir).unwrap())
}

fn seg(s: f64, e: f64) -> Seg {
    Seg {
        s,
        e,
        ..Default::default()
    }
}

fn line(s: f64, e: f64, at: f64, text: &str) -> Entry {
    Entry {
        s,
        e,
        at,
        text: text.into(),
        ..Default::default()
    }
}

// --- clause 1: entries always sorted -------------------------------------------------------------------------

/// §07-narrate#5-rules (`Entries always sorted`): playing order is by clip then placement, and both directions of
/// the file put it back — [`narration::Narration::sort`] is the one rule.
#[test]
fn sec_07_narrate_5_rules_s1_entries_are_always_sorted() {
    let (root, tree) = tree("s1");

    // Written out of order: a later clip first, and two lines on one clip with their placements swapped.
    let record = Narration {
        entries: vec![
            line(20.0, 30.0, 0.0, "the second clip"),
            line(1.5, 10.0, 3.0, "later in the clip"),
            line(1.5, 10.0, 1.0, "earlier in the clip"),
        ],
        silent: vec![],
    };
    narration::save(&record, &tree).unwrap();

    // Sorted on the way out, so the file itself is in playing order.
    let saved = narration::load(&tree).unwrap();
    let order: Vec<(f64, f64)> = saved.entries.iter().map(|e| (e.s, e.at)).collect();
    assert_eq!(order, vec![(1.5, 1.0), (1.5, 3.0), (20.0, 0.0)]);

    // And a hand-edited file is put back on the way in rather than left for every reader to notice.
    std::fs::write(
        tree.narration_json(),
        r#"{"entries": [{"s": 20.0, "e": 30.0, "text": "second"},
                      {"s": 1.5, "e": 10.0, "at": 3.0, "text": "late"},
                      {"s": 1.5, "e": 10.0, "at": 1.0, "text": "early"}]}"#,
    )
    .unwrap();
    let again = narration::load(&tree).unwrap();
    let order: Vec<(&str, f64)> = again
        .entries
        .iter()
        .map(|entry| (entry.text.as_str(), entry.at))
        .collect();
    assert_eq!(
        order,
        vec![("early", 1.0), ("late", 3.0), ("second", 0.0)],
        "a line's window runs to the next entry, so an unsorted file misplaces every line after it"
    );

    let _ = std::fs::remove_dir_all(&root);
}

// --- clause 2: a line's clip bounds are the cut's numbers -------------------------------------------------------

/// §07-narrate#5-rules (`a line's clip bounds = the cut's numbers`): [`shell::refit`] moves lines onto the cut as
/// it is now, keeping the second they speak over rather than their offset into a clip that has moved.
#[test]
fn sec_07_narrate_5_rules_s2_a_line_holds_its_clips_numbers() {
    // The same clip: not claimed as moved, however often the page is opened.
    let clips = [seg(10.0, 20.0)];
    let mut entries = vec![line(10.0, 20.0, 5.0, "one line")];
    assert_eq!(shell::refit(&clips, &mut entries), (0, 0));
    assert_eq!((entries[0].s, entries[0].at), (10.0, 5.0));

    // The clip moved: the line follows, and `at` is re-based so it stays over the same moment of the recording.
    // The new clip still brackets the old one — `clip_for` picks by overlap, so a cut that removed the line's
    // video outright would leave it an orphan rather than move it (the case below).
    let moved = [seg(15.0, 40.0)];
    let mut entries = vec![line(10.0, 20.0, 5.0, "one line")];
    assert_eq!(shell::refit(&moved, &mut entries), (1, 0));
    assert_eq!(entries[0].s, 15.0);
    assert_eq!(entries[0].e, 40.0);
    // 15.0 was the session second; over a clip that now starts at 15 it is still 15.0 in — right at its head.
    assert_eq!(entries[0].at, 0.0, "the line kept its place against the video");
    assert_eq!(
        shell::refit_sentence(1, 0),
        "the cut moved \u{2014} 1 line(s) followed their clips"
    );

    // A clip taken out from under a line: the words survive and the count says so rather than hushing it. The cut
    // has to keep something for `clip_for` to answer at all — with no overlap left anywhere, the line is an orphan.
    let gone = [seg(10.0, 20.0), seg(100.0, 110.0)];
    let mut entries = vec![line(50.0, 60.0, 5.0, "orphaned")];
    assert_eq!(shell::refit(&gone, &mut entries), (0, 1));
    assert_eq!(
        (entries[0].s, entries[0].e, entries[0].at),
        (50.0, 60.0, 5.0),
        "deleting narration because a clip went away is worse than a line that needs a look"
    );
    assert_eq!(
        shell::refit_sentence(0, 1),
        "the cut moved \u{2014} 0 line(s) followed their clips, 1 sit on video the cut no longer has"
    );
}

// --- clause 3: `at` never in the last second --------------------------------------------------------------------

/// §07-narrate#5-rules (`at never in the last second`): a line starting there has no room to be heard before the
/// cut moves on, so both doors into a placement clamp at the clip's last second.
#[test]
fn sec_07_narrate_5_rules_s3_a_line_never_starts_in_the_last_second() {
    // A tag naming a second inside the last one moves the line to the last second that can be kept, and says the
    // requested second could not be kept rather than hushing it.
    let (at, clamped) = page::tag_moves_line(10.0, 20.0, 19.0, 2.0);
    assert_eq!((at, clamped), (19.0, false), "19.0 is the last keepable second");
    let (at, clamped) = page::tag_moves_line(10.0, 20.0, 19.5, 2.0);
    assert!(clamped, "a second in the clip's last second is refused: {at}");

    // A second outside the clip moves the line nowhere and keeps its current placement.
    let (at, clamped) = page::tag_moves_line(10.0, 20.0, 40.0, 2.0);
    assert_eq!((at, clamped), (2.0, true));

    // ＋ on a row: half a second after the line above, unless that lands in the clip's last second.
    assert_eq!(page::add_below(12.0, 20.0), Ok(12.5));
    let refused = page::add_below(19.0, 20.0);
    assert!(
        refused
            .as_ref()
            .is_err_and(|why| why.contains("no room after this line")),
        "{refused:?}"
    );
}

// --- clause 4: empty text is a deliberate answer -----------------------------------------------------------------

/// §07-narrate#5-rules (`empty text is a deliberate answer`): `""` is a choice the page spells out and every
/// speaking step respects, not an unfinished line.
#[test]
fn sec_07_narrate_5_rules_s4_empty_text_is_an_answer_not_a_blank() {
    let blank = line(10.0, 20.0, 0.0, "");
    assert_eq!(page::status(&blank, false), page::RowStatus::Silent);
    // A take under a silent line does not unsay it: the row still reads as silence.
    assert_eq!(page::status(&blank, true), page::RowStatus::Silent);

    let caption = Entry {
        pos: "top".into(),
        ..line(10.0, 20.0, 0.0, "read this")
    };
    assert_eq!(page::status(&caption, false), page::RowStatus::Caption);

    // En dash U+2013 as the inventory writes it.
    assert_eq!(
        page::status_line(page::RowStatus::Silent),
        "(no line \u{2014} this clip plays on its own audio)"
    );

    // The marker behind a removed last line, and the seam that keeps silence out of every request.
    let record = Narration {
        entries: vec![],
        silent: vec![Silent { s: 10.0, e: 20.0 }],
    };
    assert!(record.is_silent(10.0, 20.0));
    assert!(!record.is_silent(30.0, 40.0));

    let spoken = line(10.0, 20.0, 0.0, "words");
    let kept = narrate_off::lines_to_speak(false, &[blank.clone(), spoken.clone()]);
    assert_eq!(kept, vec![spoken], "silence is never sent to the voice");
}

// --- clause 5: preview plays the cut, sound equals the render's ---------------------------------------------------

/// §07-narrate#5-rules (`preview plays the cut, sound equals the render's`): the preview asks [`cut_hear`] the same
/// two questions the render asks — which lanes are hushed here, what rate runs under this second.
#[test]
fn sec_07_narrate_5_rules_s5_the_preview_hears_what_the_render_will_hear() {
    let quiet = Seg {
        s: 10.0,
        e: 20.0,
        quiet: vec!["mic.mkv".into()],
        ..Default::default()
    };
    let plain = seg(30.0, 40.0);
    let cut = Cut {
        segs: vec![quiet.clone(), plain.clone()],
        ..Default::default()
    };

    // Same scene, same answer as the owner gives it — and nothing outside every clip.
    for t in [12.0, 19.9, 35.0, 45.0] {
        let heard = view::lanes_heard(&cut, t);
        let want: Vec<String> = match cut_hear::scene_at(&cut.segs, t)
            .and_then(|scene| cut.segs.get(scene))
        {
            Some(scene) => cut_hear::hush(scene).to_vec(),
            None => Vec::new(),
        };
        assert_eq!(heard, want, "the preview hushed differently at {t}");
    }
    assert_eq!(view::lanes_heard(&cut, 12.0), vec!["mic.mkv".to_string()]);
    assert!(view::lanes_heard(&cut, 45.0).is_empty());

    // And the rate: one question, one owner, so a hurried clip is hurried in both.
    let fast = vec![Fx {
        kind: "speed".into(),
        t: 10.0,
        dur: 10.0,
        rate: 2.0,
        ..Default::default()
    }];
    for t in [5.0, 12.0, 25.0] {
        assert_eq!(view::seek_rate(&fast, t), cut_hear::rate_under(&fast, t));
    }
    assert_eq!(view::seek_rate(&fast, 12.0), 2.0);
    assert_eq!(view::seek_rate(&fast, 5.0), 1.0);
}

// --- clause 6: boundary held while a line speaks -------------------------------------------------------------------

/// §07-narrate#5-rules (`boundary held while a line speaks`): the gap after a clip is held only while the speaking
/// line is within its clip's end + P.eng.narrationMaxExtendSeconds — the same ceiling that lets the render grow a
/// clip at all.
// P.eng.narrationMaxExtendSeconds
#[test]
fn sec_07_narrate_5_rules_s6_a_boundary_is_held_while_a_line_speaks() {
    // The row's bound and the render's ceiling are one number, or the preview runs on early.
    let row: f64 = params::narrate()
        .into_iter()
        .find(|param| param.id == "P.eng.narrationMaxExtendSeconds")
        .expect("the hold has a catalogue row")
        .spelled
        .parse()
        .unwrap();
    assert_eq!(row, view::MAX_EXTEND_SECONDS);
    assert_eq!(view::MAX_EXTEND_SECONDS, 4.0);

    let clips = [seg(10.0, 20.0), seg(50.0, 60.0)];
    let entries = [line(10.0, 20.0, 8.0, "a line still speaking into the gap")];

    // Within the ceiling: hold rather than jump away from the middle of a sentence.
    assert_eq!(view::tick(22.0, &clips, &entries, &[], Some(0)), view::Tick::Hold);
    assert_eq!(
        view::tick(23.9, &clips, &entries, &[], Some(0)),
        view::Tick::Hold
    );
    // Past it: the render would not have grown that far, so skip to what is next.
    assert_eq!(
        view::tick(24.1, &clips, &entries, &[], Some(0)),
        view::Tick::SkipTo(50.0)
    );
    // Past the last clip both players stop rather than skipping forward forever.
    assert_eq!(
        view::tick(70.0, &clips, &entries, &[], Some(0)),
        view::Tick::Pause
    );
    // And with nothing speaking there is no hold to give: an empty gap is jumped.
    assert_eq!(
        view::tick(22.0, &clips, &entries, &[], None),
        view::Tick::SkipTo(50.0)
    );
}

// --- clause 7: a hold resumes at the line's start -------------------------------------------------------------------

/// §07-narrate#5-rules (`a hold resumes at the line's start`): a take that arrived moves the picture to where the
/// line is spoken; a failure carries on from where it stopped.
#[test]
fn sec_07_narrate_5_rules_s7_a_hold_resumes_at_the_line_start() {
    // The hold froze the picture at 12.5 and the line starts at 20.0: waiting for a take is not watching a still,
    // so it resumes where the words begin rather than replaying seven seconds of silence.
    assert_eq!(view::resume_after_synthesis(12.5, 20.0, true), 20.0);
    // Failure has no new line to land on, so the picture simply carries on from where it stopped.
    assert_eq!(view::resume_after_synthesis(12.5, 20.0, false), 12.5);
}

// --- clause 8: failed synthesis sticky until retried ------------------------------------------------------------------

/// §07-narrate#5-rules (`failed synthesis sticky until retried`): the failure is remembered by take rather than by
/// row, so the same bad take stays mute however often the cut runs past it while a re-edit gets a fresh chance.
#[test]
fn sec_07_narrate_5_rules_s8_a_failed_take_stays_mute_until_its_own_button() {
    let mut failed = view::Failed::default();
    assert!(!failed.holds("a.wav"));
    failed.add("a.wav");
    assert!(failed.holds("a.wav"));
    assert!(!failed.holds("b.wav"), "one failure does not mute the page");

    // Re-adding is not double-counted: one retry clears it once and then it is gone.
    failed.add("a.wav");
    assert!(failed.retry("a.wav"));
    assert!(!failed.holds("a.wav"));
    assert!(!failed.retry("a.wav"), "there was nothing left to clear");

    // The two sentences the log gets, pinned: 1-based row numbers, and a sticky silent failure is named every time
    // it comes around with the row's own ▶ as the way out.
    assert_eq!(
        view::sticky_failed(2),
        "line 3 failed to synthesize \u{2014} see log; its \u{25b6} retries"
    );
    assert_eq!(
        view::failed_playing_on(2),
        "line 3 failed -- see log; playing on without it"
    );

    // Keyed on the wav: re-editing the line names a different take, which is its own fresh chance.
    let before = narration::tts_key(&line(1.0, 9.0, 0.0, "as written"), None, None);
    let after = narration::tts_key(&line(1.0, 9.0, 0.0, "re-written"), None, None);
    assert_ne!(before, after);
    let mut once = view::Failed::default();
    once.add(&narration::tts_file(&before));
    assert!(once.holds(&narration::tts_file(&before)));
    assert!(!once.holds(&narration::tts_file(&after)));
}

// --- clause 9: captions-only short-circuits everything that speaks -----------------------------------------------------

/// §07-narrate#5-rules (`captions-only short-circuits everything that speaks`): the picker's first row is an id,
/// and it is the id every step that could speak checks.
#[test]
fn sec_07_narrate_5_rules_s9_a_captions_only_voice_speaks_nothing() {
    assert_eq!(page::CAPTIONS, "captions");
    assert!(rules::voice_is_captions(page::CAPTIONS));
    assert!(!rules::voice_is_captions("aria"));

    // Two ways a video ends up with nothing spoken — F4.6's voice and F4.8's tick — and each answers on its own.
    assert!(!rules::speaks_anything(page::CAPTIONS, false));
    assert!(!rules::speaks_anything("aria", true));
    assert!(rules::speaks_anything("aria", false));

    // The id is the first row of the picker, so nothing has to special-case the label.
    let options = page::voice_options(0, &[]);
    assert_eq!(options[0].id, page::CAPTIONS);
}

// --- clause 10: the run bar is the preview's transport only once started -------------------------------------------------

/// §07-narrate#5-rules (`the run bar is the preview's transport only once the preview started`): ▶ belongs to the
/// step until a preview has actually been started, and stays the preview's until ⏹ ends it.
#[test]
fn sec_07_narrate_5_rules_s10_the_run_bar_belongs_to_the_preview_once_it_started() {
    // Nothing started: ▶ is the step, and ⏹ has nothing to end.
    let idle = Transport::default();
    assert!(!rules::bar_serves_the_preview(idle));
    assert!(!run::controls(&None, Some(idle)).stop_sensitive);

    // Playing: the transport's, and paused mid-preview it still is — ⏸ has to be able to resume what is parked.
    let playing = Transport {
        playing: true,
        started: true,
    };
    let cued = Transport {
        playing: false,
        started: true,
    };
    assert!(rules::bar_serves_the_preview(playing));
    assert!(rules::bar_serves_the_preview(cued));
    assert!(cued.cued(), "started and paused is what ⏹ still has to end");
    assert!(run::controls(&None, Some(cued)).stop_sensitive);

    // Which pages have a transport at all is the other half of the same clause.
    assert_eq!(run::transport_for(Page::Narrate, playing), Some(playing));
    assert_eq!(run::transport_for(Page::Cut, cued), Some(cued));
    assert!(run::transport_for(Page::Prepare, playing).is_none());
    assert!(run::transport_for(Page::Produce, playing).is_none());
}

// --- clause 11: cache keys stable, nothing deletes old wavs --------------------------------------------------------------

/// §07-narrate#5-rules (`cache keys stable, nothing deletes old wavs`): a take is never cleaned up, because the key
/// is frozen and derived from what was spoken — so the only files this page removes are the built reference and its
/// base.
#[test]
fn sec_07_narrate_5_rules_s11_nothing_here_deletes_a_take() {
    let (root, tree) = tree("s11");

    // A take: spoken audio, never swept.
    assert!(rules::deletes_a_spoken_take(&tree.tts_wav("0123456789abcdef")));

    // Everything the invalidation does remove, and the record — none of them takes.
    for path in [
        tree.voice_ref_wav(),
        tree.voice_ref_base_wav(),
        tree.sample_wav("aria", "a1b2c3d4e5f6"),
        tree.narration_json(),
    ] {
        assert!(
            !rules::deletes_a_spoken_take(&path),
            "{} is not a take",
            path.display()
        );
    }

    // The key that makes deleting unnecessary: frozen, and named by what was spoken.
    let entry = Entry {
        s: 1.5,
        e: 34.7,
        text: "We start with \u{2026}".to_string(),
        emotion: "calm".to_string(),
        ..Default::default()
    };
    let key = narration::tts_key(&entry, None, None);
    assert_eq!(key, "25e0.85|We start with \u{2026}|calm");
    let file = narration::tts_file(&key);
    assert_eq!(file.len(), 16, "{file}");
    assert!(
        file.chars().all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)),
        "{file} is not lowercase hex"
    );

    // A line nobody re-rolled keeps the key it had before the voice picker and the re-roll existed; rolling salts
    // it, which is a new take rather than an old one to throw away.
    let rolled = narration::tts_key(
        &Entry {
            roll: 3,
            ..entry
        },
        None,
        None,
    );
    assert!(rolled.contains("|3#"), "{rolled}");
    assert_ne!(narration::tts_file(&key), narration::tts_file(&rolled));

    let _: &Path = tree.dir();
    let _ = std::fs::remove_dir_all(&root);
}

// --- clause 12: hand-picked takes never re-ranked -------------------------------------------------------------------------

/// §07-narrate#5-rules (`hand-picked takes never re-ranked`): chosen seconds win outright — no cap, no
/// diarization, no ordering by quality. P.eng.takeMinSeconds is the only floor they meet.
// P.eng.takeMinSeconds
#[test]
fn sec_07_narrate_5_rules_s12_hand_picked_takes_are_never_re_ranked() {
    assert!(!rules::takes_are_hand_picked(&[]));
    assert!(rules::takes_are_hand_picked(&[Take { s: 4.0, e: 9.0 }]));

    // Unsorted, overlapping and partly too short: the clean-up puts them in time order and merges what touches —
    // it never decides one take is worth more than another.
    let kept = narrate_data::clean_takes(&[
        Take { s: 8.0, e: 20.0 },
        Take { s: 2.0, e: 3.0 },
        Take { s: 7.0, e: 9.0 },
        Take { s: 40.0, e: 40.1 },
    ]);
    assert_eq!(
        kept,
        vec![Take { s: 2.0, e: 3.0 }, Take { s: 7.0, e: 20.0 }],
        "a twelve-second take does not overtake a one-second one; time order is the only order"
    );
    assert!(rules::takes_are_hand_picked(&kept));

    // The floor is the seconds floor and nothing else: 0.4 s of speech or it goes, however good it sounds.
    assert_eq!(page::TAKE_MIN_SECONDS, 0.4);
    assert!(narrate_data::clean_takes(&[Take { s: 1.0, e: 1.3 }]).is_empty());
}

// --- clause 13: a voice not on offer leaves the picker empty ---------------------------------------------------------------

/// §07-narrate#5-rules (`a voice not on offer leaves the picker empty rather than pointing at the wrong speaker`):
/// no row is shown rather than row one, and the two ways it happened are named apart.
#[test]
fn sec_07_narrate_5_rules_s13_a_voice_not_on_offer_leaves_the_picker_empty() {
    assert_eq!(rules::narrator_slot("own"), Some(1));
    assert_eq!(rules::narrator_slot("narrator3"), Some(3));
    assert_eq!(rules::narrator_slot("narrator5"), None, "only four slots exist");
    assert_eq!(rules::narrator_slot("aria"), None);
    assert_eq!(rules::narrator_slot(""), None);

    let options = page::voice_options(2, &["aria"]);
    let ids: Vec<&str> = options.iter().map(|option| option.id.as_str()).collect();
    assert_eq!(ids, vec!["captions", "narrator1", "narrator2", "aria"]);

    assert_eq!(rules::picker_row(&options, "aria"), Some(3));
    assert_eq!(rules::picker_row(&options, "narrator1"), Some(1));
    // A project written before slots were numbered stores `own`, which names the same recording slot 1 holds.
    assert_eq!(rules::picker_row(&options, "own"), Some(1));
    // The clause itself: nothing on offer is that voice, so show nothing — never row one.
    assert_eq!(rules::picker_row(&options, "gone"), None);

    // Which of the two sentences to say, since they are fixed in different places: Prepare's tagging, or the folder.
    assert_eq!(
        rules::missing_voice("narrator3", "/voices"),
        "narrator 3 is not tagged on the Prepare step \u{2014} tag a recording, or pick another voice"
    );
    assert_eq!(
        rules::missing_voice("gone", "/voices"),
        "voice \"gone\" is no longer in /voices \u{2014} pick another"
    );
    assert_ne!(
        rules::missing_voice("narrator3", "/voices"),
        rules::missing_voice("gone", "/voices")
    );
}
