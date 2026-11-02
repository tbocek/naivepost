//! §07-narrate#1-screen — the Narrate page's widgets, in the order §1 numbers them.
//!
//! The toolbar's groups and what its ＋ is drawn with, the two clocks, what a row says after its time, the six
//! things one ▶ can mean, the take band's arithmetic and the voice picker's rows: all of it is answered in
//! [`naivepost::narrate_screen`], so all of it is checked here without a window. The widgets are the page's
//! business; they hold no rule (spec/00-principles.md §5).

use crate::common;
use common::seg as clip;
use naivepost::narrate_screen::{self as page, BoxRole, Fit, RowStatus};
use naivepost::narration::{self, Entry};
use naivepost::tools;

/// A line on the clip `s..e`, starting `at` seconds into it.
fn line(s: f64, e: f64, at: f64, words: &str) -> Entry {
    Entry {
        s,
        e,
        at,
        text: words.to_string(),
        ..Default::default()
    }
}

// --- S1: the toolbar (§1's numbers 3–8) -------------------------------------------------------------------

// --- S2: the two clocks and the outputs readout (§1's numbers 5, 22) ---------------------------------------

/// §07-narrate#1-screen — `clock “session · cut/length”`, and `Outputs: “narrate/ — narration.json, the voice
/// reference and the synthesis cache”`.
#[test]
fn sec_07_narrate_1_screen_s2_the_clock_reads_two_places_and_the_outputs_name_three_things() {
    // Session second, then position in the finished video over its length — both faces `tools::mm_ss`'s.
    let line = page::clock_line(65.0, 30.0, 300.0);
    assert_eq!(
        line,
        format!(
            "{} \u{b7} {}/{}",
            tools::mm_ss(65.0),
            tools::mm_ss(30.0),
            tools::mm_ss(300.0)
        )
    );
    assert_eq!(line, "01:05 \u{b7} 00:30/05:00");

    // Two clocks because the edit removed seconds: the same moment reads differently on each.
    let middle = page::clock_line(200.0, 40.0, 300.0);
    assert!(middle.starts_with("03:20 \u{b7} "), "{middle}");

    assert_eq!(
        page::outputs_readout(),
        "narrate/ \u{2014} narration.json, the voice reference and the synthesis cache"
    );
}

// --- S4: the text box, read and written (§1's number 20) ----------------------------------------------------

/// §07-narrate#1-screen — `Text box: `[emotion] words`, `[top|center|bottom] words` for captions; may carry the
/// line's second in the tag as `@N` (`[excited @65] Weee` moves the line …) — write-only: the time field owns the
/// number, the box never prints it back`. P.policy.narrationEmotionTag.
#[test]
fn sec_07_narrate_1_screen_s4_the_box_reads_a_tag_and_never_writes_the_second_back() {
    let parsed = page::parse_box("[excited @65] Weee");
    assert_eq!(parsed.tag, "excited");
    assert_eq!(parsed.at, Some(65.0));
    assert_eq!(parsed.body, "Weee");
    assert_eq!(parsed.role, BoxRole::Emotion);

    // Every placement spelling §B.4 lists makes a caption, and a caption carries no emotion.
    for spelling in ["top", "center", "centre", "middle", "bottom"] {
        let caption = page::parse_box(&format!("[{spelling}] words"));
        assert_eq!(caption.role, BoxRole::Caption, "{spelling}");
        assert_eq!(
            caption.tag, "",
            "{spelling}: the placement is not an emotion"
        );
        assert_eq!(caption.body, "words");
    }

    // A weighted mix stays one tag: the weights belong to the names, not to a second.
    let weighted = page::parse_box("[happy=0.8, surprised=0.4] words");
    assert_eq!((weighted.role, weighted.at), (BoxRole::Emotion, None));
    assert_eq!(weighted.tag, "happy=0.8, surprised=0.4");

    // No tag at all: the whole box is words, and nothing moves.
    let bare = page::parse_box("just words");
    assert_eq!(
        (bare.role, bare.at, bare.body.as_str()),
        (BoxRole::None, None, "just words")
    );

    // Write-only: the second is dropped on the way out even though it was read on the way in.
    let moved = Entry {
        s: 0.0,
        e: 90.0,
        at: 65.0,
        text: "Weee".into(),
        emotion: "excited".into(),
        ..Default::default()
    };
    assert_eq!(page::write_box(&moved), "[excited] Weee");
    assert!(
        !page::write_box(&moved).contains('@'),
        "the time field owns the number"
    );

    // A caption writes its placement, and a silent clip's box stays empty rather than showing a bare tag.
    let caption = Entry {
        text: "words".into(),
        pos: "top".into(),
        ..Default::default()
    };
    assert_eq!(page::write_box(&caption), "[top] words");
    assert_eq!(page::write_box(&Entry::default()), "");
}

// --- S5: the `@N` tag moves the line (§1's number 20) --------------------------------------------------------

/// §07-narrate#1-screen — `` `[excited @65] Weee` moves the line, clamped out of the clip's last second``.
#[test]
fn sec_07_narrate_1_screen_s5_the_tag_second_moves_the_line_but_not_into_the_last_second() {
    // A second inside the clip moves it and nothing is reported.
    assert_eq!(page::tag_moves_line(60.0, 90.0, 65.0, 61.0), (5.0, false));

    // The last second belongs to §5's `at never in the last second`, and a tag naming it is refused: the line stays
    // where it was and the page says the move did not happen.
    assert_eq!(page::tag_moves_line(60.0, 90.0, 89.7, 61.0), (61.0, true));

    // So does a second outside the clip — writing it back is what stops the box reading as if it had moved.
    assert_eq!(page::tag_moves_line(60.0, 90.0, 120.0, 61.0), (61.0, true));
    assert_eq!(page::tag_moves_line(60.0, 90.0, 12.0, 61.0), (61.0, true));

    // The clip's own first second is a legal placement.
    assert_eq!(page::tag_moves_line(60.0, 90.0, 60.0, 61.0), (0.0, false));
}

// --- S6: the row's status (§1's numbers 14, 15) ---------------------------------------------------------------

/// §07-narrate#1-screen — `status (“– end time”, “(~)” while estimated, “(no line — this clip plays on its own
/// audio)”, “(caption — the viewer reads it; never spoken)”)`.
#[test]
fn sec_07_narrate_1_screen_s6_the_row_says_where_the_line_ends_and_whether_that_is_a_guess() {
    // Estimated until the take exists. The question is asked of `narration::tts_file`, so it is the same answer the
    // speaker will use and not this page's own idea of a speaking rate.
    let spoken = line(60.0, 90.0, 4.0, "words");
    let key = narration::tts_key(&spoken, None, None);
    assert!(
        !narration::tts_file(&key).is_empty(),
        "the take's name always exists; the file need not"
    );

    assert_eq!(page::status(&spoken, false), RowStatus::Estimated(64.0));
    // The end is a session second — the row's own time entry reads the same clock — so the estimate carries the
    // clip's head (60 s) plus the placement inside it, and never prints a bare 4 s.
    assert_eq!(
        page::status_line(page::status(&spoken, false)),
        "\u{2013} 01:04.0 (~)"
    );
    assert_eq!(
        page::status_line(page::status(&spoken, true)),
        "\u{2013} 01:04.0"
    );

    // A deliberate silence and a caption answer before the take question: neither will ever have one.
    let silent = line(60.0, 90.0, 0.0, "");
    assert_eq!(page::status(&silent, false), RowStatus::Silent);
    assert_eq!(
        page::status_line(RowStatus::Silent),
        "(no line \u{2014} this clip plays on its own audio)"
    );
    let caption = Entry {
        pos: "top".into(),
        ..line(60.0, 90.0, 0.0, "words")
    };
    assert_eq!(page::status(&caption, false), RowStatus::Caption);
    assert_eq!(
        page::status_line(RowStatus::Caption),
        "(caption \u{2014} the viewer reads it; never spoken)"
    );
}

// --- S7: the fit warnings (§1's number 21) ---------------------------------------------------------------------

/// §07-narrate#1-screen — `warnings “⚠ ~N s of speech, M s before the clip ends|the next line” and “⚠ this clip's
/// lines run N s past it — the render will have them moved earlier[ and sped up]”`, `and the row marked red`.
#[test]
fn sec_07_narrate_1_screen_s7_both_warnings_are_said_and_both_mark_the_row_red() {
    let before_end = page::fit_warning(Fit::Tight {
        speech: 6.0,
        before: 4.0,
        next_line: false,
    })
    .unwrap();
    assert_eq!(
        before_end,
        "\u{26a0} ~6 s of speech, 4 s before the clip ends"
    );
    let before_next = page::fit_warning(Fit::Tight {
        speech: 6.0,
        before: 4.0,
        next_line: true,
    })
    .unwrap();
    assert_eq!(
        before_next,
        "\u{26a0} ~6 s of speech, 4 s before the next line"
    );

    // The second warning names what the render will do, and only says "sped up" when it will.
    let moved = page::fit_warning(Fit::Overruns {
        past: 3.0,
        sped_up: false,
    })
    .unwrap();
    assert_eq!(
        moved,
        "\u{26a0} this clip's lines run 3 s past it \u{2014} the render will have them moved earlier"
    );
    let sped = page::fit_warning(Fit::Overruns {
        past: 3.0,
        sped_up: true,
    })
    .unwrap();
    assert!(sped.ends_with("moved earlier and sped up"), "{sped}");

    // A line that fits says nothing and leaves the row its normal colour; either warning marks it.
    assert_eq!(page::fit_warning(Fit::Fits), None);
    assert!(!page::row_is_red(Fit::Fits));
    assert!(page::row_is_red(Fit::Tight {
        speech: 6.0,
        before: 4.0,
        next_line: false
    }));
    assert!(page::row_is_red(Fit::Overruns {
        past: 3.0,
        sped_up: false
    }));
}

// --- S8: the row's ▶ (§1's number 16) ----------------------------------------------------------------------------

// --- S9: adding a line (§1's numbers 4, 18) ------------------------------------------------------------------------

/// §07-narrate#1-screen — `＋ a line at the playhead`, and the row's ＋ (`add a line below`).
#[test]
fn sec_07_narrate_1_screen_s9_a_line_is_added_where_the_cut_can_carry_it() {
    let clips = [clip(0.0, 30.0), clip(40.0, 60.0)];

    // Between clips: the cut has nothing to narrate there.
    let between = page::add_at_playhead(35.0, &clips, &[]).unwrap_err();
    assert_eq!(
        between,
        "the playhead is between clips \u{2014} the cut has nothing to narrate here"
    );

    // Within a second of an existing line: not a new line, the same one, and the page jumps to it.
    let near = page::add_at_playhead(12.0, &clips, &[line(0.0, 30.0, 11.5, "words")]).unwrap_err();
    assert!(
        near.starts_with("a line already starts here \u{2014}"),
        "{near}"
    );
    assert_eq!(page::ADD_NEAR_SECONDS, 1.0); // F4.7's "within 1 s of a line"

    // Inside a speaking line: it says until when, which is the second to add after. With no take yet that end is
    // \u{a7}C.2's estimate \u{2014} 15 characters a second plus the tail \u{2014} so a hundred characters run from
    // 15 s to 21.87 s and a press at 20 s falls inside them.
    let long = "w".repeat(100);
    let busy = page::add_at_playhead(20.0, &clips, &[line(0.0, 30.0, 15.0, &long)]).unwrap_err();
    assert_eq!(
        busy,
        "a line is speaking here until 00:21.8 \u{2014} add after it"
    );
    assert_eq!(page::SPEECH_CHARS_PER_SECOND, 15.0); // \u{a7}C.2's default rate
    assert_eq!(page::SPEECH_TAIL_SECONDS, 0.2); // P.eng.narrationTailSeconds

    // The honest opposite: five characters are spoken by 15.5 s, so the seconds after that are free ground again and
    // \uff0b takes them. An estimate that never expired would refuse the rest of every clip.
    assert_eq!(
        page::add_at_playhead(20.0, &clips, &[line(0.0, 30.0, 15.0, "words")]),
        Ok(20.0)
    );

    // Before a line has started speaking is free ground too: its window begins at its own second.
    assert_eq!(
        page::add_at_playhead(12.0, &clips, &[line(0.0, 30.0, 15.0, &long)]),
        Ok(12.0)
    );

    // A line on another clip says nothing about these seconds.
    assert_eq!(
        page::add_at_playhead(12.0, &clips, &[line(40.0, 60.0, 2.0, "words")]),
        Ok(12.0)
    );

    // §F4.7's free add is "a line HERE": the pressed second, not the clip's head — the clip only says
    // there is something to narrate at it. Past every line in the clip that second is still the playhead.
    assert_eq!(
        page::add_at_playhead(25.0, &clips, &[line(0.0, 30.0, 2.0, "words")]),
        Ok(25.0)
    );
    assert_eq!(
        page::add_at_playhead(45.0, &clips, &[line(0.0, 30.0, 2.0, "words")]),
        Ok(45.0)
    );

    // ...bounded by the clip's last second, which can never hold a line start (§F4.7's move rule): a
    // press at a clip's very end lands a second earlier rather than outside the clip.
    assert_eq!(page::add_at_playhead(59.8, &clips, &[]), Ok(59.0));

    // The next line's second bounds the estimate: this press is a second clear of both starts and inside the first
    // line's window, so it is refused \u{2014} with the second to add after named as the later line's own start.
    let capped = page::add_at_playhead(
        16.4,
        &clips,
        &[line(0.0, 30.0, 15.0, &long), line(0.0, 30.0, 20.0, "more")],
    )
    .unwrap_err();
    assert_eq!(
        capped,
        "a line is speaking here until 00:20.0 \u{2014} add after it"
    );

    // The row's ＋: half a second after the line above, refused when the clip ends first.
    assert_eq!(page::add_below(8.0, 20.0), Ok(8.5));
    assert_eq!(page::ADD_BELOW_SECONDS, 0.5);
    assert_eq!(
        page::add_below(8.6, 10.0),
        Err("no room after this line \u{2014} the clip ends first".to_string())
    );
}

// --- S10: re-roll, remove and the time field (§1's numbers 14, 17–19) ---------------------------------------------

/// §07-narrate#1-screen — `↻ re-roll (new take, same words)`, `🗑 remove (clip plays its own audio)`, and the time
/// entry (`a time in a gap is refused and written back`).
#[test]
fn sec_07_narrate_1_screen_s10_re_roll_and_removal_say_what_happened_to_the_clip() {
    // §6: "clip N has no line to re-roll" — a draw needs something to draw again of.
    assert_eq!(
        page::re_roll_refusal(3, false),
        Some("clip 3 has no line to re-roll".to_string())
    );
    assert_eq!(page::re_roll_refusal(3, true), None);

    // Bare "line removed" while the clip still speaks; the longer sentence, and the silent marker behind it, only
    // when this was its last line.
    assert_eq!(page::remove_line(true, 65.0), "line removed");
    assert_eq!(
        page::remove_line(false, 65.0),
        "line removed \u{2014} the clip at 1:05 plays its own audio"
    );

    // The field's face is the preview clock's, so a row and the transport cannot disagree about a tenth.
    assert_eq!(page::time_field(65.0), "01:05.0");

    // A time in a gap names the typed second, the clip's bounds and where the line stayed.
    let refused = page::time_refused(35.0, 0.0, 30.0, 12.0);
    assert_eq!(
        refused,
        "00:35 is outside the cut \u{2014} this line stays in its clip (00:00\u{2013}00:30), at 00:12.0"
    );
}

// --- S11: the take band (§1's number 10) -----------------------------------------------------------------------------

// --- S12: the voice picker and the sample (§1's numbers 7, 9, 11–13) --------------------------------------------------

// --- S13: the box's tooltip (§1's number 20) ---------------------------------------------------------------------------

// --- S14: the numbers themselves (§10, as §1 uses them) ------------------------------------------------------

/// §07-narrate#1-screen — the page's own values, catalogued. §10 is a list of ids rather than of numbers, so what this
/// pins is that the two cannot drift: every row `params::narrate()` carries spells the value its owner constant holds
/// and names that owner, including the four §10 lists only inside its prose line (`Narrate take band: ruler 12, lane
/// 56, max 200 px/s, click slop 3 px`; §07's `Engineering: tick 100 ms · seek debounce 120 ms · autosave 400 ms`),
/// which carry no `P.` id and say so by their prefix.
#[test]
fn sec_07_narrate_1_screen_s14_the_numbers_the_page_draws_with_are_catalogued() {
    let row = |id: &str| {
        naivepost::params_narrate::rows()
            .into_iter()
            .find(|param| param.id == id)
            .unwrap_or_else(|| panic!("no catalogue row for {id}"))
    };

    // P.eng.takeMinSeconds — the shortest hand-picked take, spelled as §10 spells it.
    assert_eq!(row("P.eng.takeMinSeconds").spelled, "0.4");
    assert_eq!(
        row("P.eng.takeMinSeconds").from,
        "narrate_screen::TAKE_MIN_SECONDS"
    );

    // P.eng.pitchRangeSemitones: one row for the whole range, so it spells both ends and the step between them.
    let pitch = row("P.eng.pitchRangeSemitones");
    assert_eq!(pitch.spelled, "-6..+6, step 0.5");
    assert_eq!(
        pitch.from,
        "narrate_screen::PITCH_MIN_SEMITONES + PITCH_MAX_SEMITONES + PITCH_STEP"
    );

    // P.eng.narrationTailSeconds — the tail that makes "speaking until mm:ss" name the end of a line.
    assert_eq!(row("P.eng.narrationTailSeconds").spelled, "0.2");
    assert_eq!(
        row("P.eng.narrationTailSeconds").from,
        "narrate_screen::SPEECH_TAIL_SECONDS"
    );

    // The band's geometry and the two debounces: §10 lists these by area, so their ids carry no `P.` prefix and each
    // names the constant it reads rather than a number of its own.
    for (id, from) in [
        ("narrate.bandRulerPx", "narrate_screen::BAND_RULER_PX"),
        ("narrate.bandLanePx", "narrate_screen::BAND_LANE_PX"),
        ("narrate.bandMaxPps", "narrate_screen::BAND_MAX_PPS"),
        ("narrate.clickSlopPx", "narrate_screen::CLICK_SLOP_PX"),
        ("narrate.takeLabelPx", "narrate_screen::TAKE_LABEL_MIN_PX"),
        ("narrate.tickMs", "narrate_screen::TICK_MS"),
        ("narrate.seekDebounceMs", "narrate_screen::SEEK_DEBOUNCE_MS"),
        (
            "narrate.speechCharsPerSecond",
            "narrate_screen::SPEECH_CHARS_PER_SECOND + CHAR_RATE_MIN + CHAR_RATE_MAX",
        ),
        ("narrate.addNearSeconds", "narrate_screen::ADD_NEAR_SECONDS"),
    ] {
        assert_eq!(row(id).from, from, "{id}");
    }

    // The values themselves, as the page draws with them (§10:133's line and §1's own numbers).
    let spelled = |id: &str| row(id).spelled;
    assert_eq!(spelled("narrate.bandRulerPx"), "12");
    assert_eq!(spelled("narrate.bandLanePx"), "56");
    assert_eq!(spelled("narrate.bandMaxPps"), "200");
    assert_eq!(spelled("narrate.clickSlopPx"), "3");
    assert_eq!(spelled("narrate.takeLabelPx"), "34");
    assert_eq!(spelled("narrate.tickMs"), "100");
    assert_eq!(spelled("narrate.seekDebounceMs"), "120");
    // The autosave is the shell's timer, so its row spells the duration rather than a copy of it.
    assert_eq!(spelled("narrate.autosaveMs"), "400");
    assert_eq!(row("narrate.autosaveMs").from, "shell::NARRATION_AUTOSAVE");

    // The one row §1 needs that §10 has no id for stays absent rather than invented: the ＋'s half-second gap is
    // F4.7's, so it does not pretend to be tuned here. F4.3's fitting trio now HAS ids and rows.
    let ids: Vec<&str> = naivepost::params_narrate::rows()
        .iter()
        .map(|param| param.id)
        .collect();
    assert!(!ids.contains(&"narrate.addGapSeconds"), "{ids:?}");
    for fitted in [
        "P.eng.narrationLeadSeconds",
        "P.eng.narrationGapSeconds",
        "P.eng.narrationMaxTempo",
    ] {
        assert!(
            ids.contains(&fitted),
            "{fitted} is read by the render's ladder: {ids:?}"
        );
    }
    // The page's own rows are `P.eng.` or `narrate.`; the five extra ones are the narration flow's, which §10
    // puts in §1's machine table and §2's policy table rather than among the engineering constants
    // (§10:60, 87, 89, 90, and `P.policy.ttsLanguage` in §2).
    let foreign: Vec<&str> = ids
        .iter()
        .filter(|id| !id.starts_with("P.eng.") && !id.starts_with("narrate."))
        .copied()
        .collect();
    assert_eq!(
        foreign,
        [
            "P.machine.narrationContextSeconds",
            "P.policy.narrationMinWords",
            "P.policy.narrationMaxWords",
            "P.policy.narrationWordsPerSecond",
            "P.policy.ttsLanguage",
            // F4.1 S4's toggle: §07 names it in the flow diagram, not in its own §4 list, so it is
            // carried here with that reason rather than being read as a row from another chapter.
            "P.policy.narrationRewrite",
            // The automatic reference's two aim values, which §10 files in §1's machine table.
            "P.machine.refWantSeconds",
            "P.machine.refTakeMax"
        ],
        "{foreign:?}"
    );
}

// --- S15: the lines column's empty state (§1's number 20, `no narration lines yet`) -------------------

/// §07-narrate#1-screen — with nothing written the column still says so. An empty list box reads as a panel
/// that failed to fill; the note is what makes it read as "nothing yet", and it names the ＋ that changes it.
#[test]
fn sec_07_narrate_1_screen_s15_the_empty_lines_column_says_so_and_names_the_button_that_fills_it() {
    let note = page::no_lines_note();
    assert!(
        note.starts_with("no narration lines yet"),
        "the empty state must open by saying there are no lines, got {note:?}"
    );
    // The full-width plus is the same glyph the page draws its add-line buttons with, so the note points at
    // the control the reader can actually press rather than at an icon they have to recognise.
    assert!(
        note.contains('\u{ff0b}'),
        "the note must name the \u{ff0b} that puts a line at the playhead, got {note:?}"
    );
    assert!(
        note.contains("playhead"),
        "the note must say where the new line lands, got {note:?}"
    );
}
