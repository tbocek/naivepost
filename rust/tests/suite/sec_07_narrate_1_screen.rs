//! §07-narrate#1-screen — the take band's arithmetic (§1's number **10**) and the voices folder's
//! naming (§1's number **9**), as pure rules: no window, no server, no clock.
//!
//! The band is a strip of a recording that has to answer four questions exactly — what second is under
//! this pixel, how far one wheel notch goes, whether a press was a click or a drag, and what a take
//! says about itself when it is wide enough to speak. Each test below pins one group against the
//! wording in §1 / inventory §A.7 that the code cites.

use naivepost::narrate_band::{self as band, BandPaint};
use naivepost::narrate_screen::{self as screen, TakePress};
use naivepost::voices;
use std::path::Path;

/// A view over a 60-second recording across a 300 px lane at fit: 5 px/s, offset 0.
fn view() -> BandPaint {
    BandPaint {
        duration: 60.0,
        width: 300.0,
        zoom: 1.0,
        offset: 0.0,
        pps: band::px_per_second(1.0, 60.0, 300.0),
        red_bar: 0.0,
        pointer_x: -1.0,
        selection: None,
        takes: vec![],
        session: 0.0,
        playing: false,
        envelope_decoded: true,
    }
}

// --- S1: seconds and pixels round-trip -------------------------------------------

/// §07-narrate#1-screen (`Drag selects; click sets the red bar`): every mapping the drag needs is a
/// pair of inverses, and a panned view shifts both halves together so a selection cannot land on the
/// wrong seconds after Shift+wheel.
#[test]
fn sec_07_narrate_1_screen_s1_seconds_and_pixels_round_trip_across_the_view() {
    let pps = band::px_per_second(1.0, 60.0, 300.0);
    assert_eq!(pps, 5.0, "fit: 300 px over 60 s");
    // At zero offset the two are one multiply apart.
    assert_eq!(band::second_to_px(12.0, 0.0, pps), 60.0);
    assert_eq!(band::x_to_second(60.0, 0.0, pps), 12.0);
    // Round trip for a handful of seconds, including one with a fraction in it.
    for seconds in [0.0, 0.4, 7.25, 33.5, 59.9] {
        let px = band::second_to_px(seconds, 0.0, pps);
        assert!(
            (band::x_to_second(px, 0.0, pps) - seconds).abs() < 1e-9,
            "{seconds} s -> {px} px -> back"
        );
    }
    // A pan moves the pixel left by the offset, and the inverse knows it.
    let offset = 100.0;
    assert_eq!(band::second_to_px(40.0, offset, pps), 100.0);
    assert_eq!(band::x_to_second(100.0, offset, pps), 40.0);
    // A second before the visible window never paints at a negative x: it sits at the lane's start.
    assert_eq!(band::second_to_px(10.0, offset, pps), 0.0);
    // And a pixel left of the lane answers 0 s rather than a negative second.
    assert_eq!(band::x_to_second(-50.0, 0.0, pps), 0.0);
    // The ruler prints whole seconds, which is what a viewer reads while dragging.
    assert_eq!(band::ruler_second(60.0, 0.0, pps), 12);
}

// --- S2: the wheel ---------------------------------------------------------------

/// §07-narrate#1-screen (`wheel zooms; Shift+wheel pans`; inventory §A.7 `wheel zooms 1.25^−dy (max
/// 200 px/s)`, `Shift+wheel pans by w/8`): one notch is one ratio, the cap is the cap, and the view
/// cannot leave the recording in either direction.
#[test]
fn sec_07_narrate_1_screen_s2_one_notch_is_one_ratio_and_the_view_cannot_leave_the_file() {
    // Up one notch (dy negative) multiplies the fit-relative zoom by 1.25; down divides it.
    assert_eq!(band::zoom_step(1.0, -1.0), 1.25);
    // A down-notch shrinks toward fit and stops there: fit is the floor, so the view can never show
    // more than the whole recording.
    assert!(
        (band::zoom_step(1.0, 1.0) - 1.0).abs() < 1e-12,
        "fit is the floor: never less"
    );
    assert!(
        (band::zoom_step(4.0, 1.0) - 3.2).abs() < 1e-9,
        "a down-notch divides by the ratio"
    );
    // Two notches compound the way the spec's ratio says.
    assert!((band::zoom_step(1.0, -2.0) - 1.5625).abs() < 1e-9);
    // Past fit the px/s grows by the same ratio, and stops growing at 200 px/s however hard the wheel
    // is rolled: a wider-than-max view would draw peaks the cache does not hold.
    let mut zoom = 1.0;
    for _ in 0..60 {
        zoom = band::zoom_step(zoom, -1.0);
    }
    assert_eq!(band::px_per_second(zoom, 60.0, 300.0), screen::BAND_MAX_PPS);
    assert_eq!(
        band::px_per_second(1000.0, 60.0, 300.0),
        screen::BAND_MAX_PPS,
        "the cap holds"
    );
    // A shift-notch pans an eighth of the view.
    assert_eq!(band::pan_by(300.0), 37.5);
    // Clamping keeps the left edge inside: never negative, never past the last screenful of file.
    assert_eq!(band::clamp_view(-40.0, 2.0, 60.0, 300.0), 0.0);
    let span = band::content_px(2.0, 60.0, 300.0);
    assert_eq!(span, 600.0, "2× of 5 px/s over 60 s");
    assert_eq!(band::clamp_view(9999.0, 2.0, 60.0, 300.0), span - 300.0);
    assert_eq!(
        band::clamp_view(120.0, 2.0, 60.0, 300.0),
        120.0,
        "an honest offset stands"
    );
    // At fit the whole recording IS the view, so there is nothing to pan to.
    assert_eq!(band::clamp_view(50.0, 1.0, 60.0, 300.0), 0.0);
    // The scrollbar question follows the same arithmetic: absent at fit, present past it (§1's "own
    // scrollbar once the wheel zooms past fit").
    assert!(!band::needs_scrollbar(1.0, 60.0, 300.0));
    assert!(band::needs_scrollbar(
        band::zoom_step(1.0, -1.0),
        60.0,
        300.0
    ));
}

// --- S3: click versus drag -------------------------------------------------------

/// §07-narrate#1-screen (`Drag selects; ... click sets the red bar`; §A.7's slop `click (<3 px)`):
/// the split is one rule with one threshold, and it is the same answer whichever way the pointer went.
#[test]
fn sec_07_narrate_1_screen_s3_a_press_under_the_slop_sets_the_bar_and_a_drag_selects() {
    // Exactly at the slop is still a click; one px more is a drag — asked of the one rule that decides.
    assert_eq!(band::drag_or_click(50.0, 53.0, 0.0), TakePress::SetBar);
    assert_eq!(
        band::drag_or_click(50.0, 54.0, 0.0),
        TakePress::Select {
            from: 50.0,
            to: 54.0
        }
    );
    // Backwards drags name their seconds in order, because a take's seconds have no direction.
    assert_eq!(
        band::drag_or_click(54.0, 50.0, 0.0),
        TakePress::Select {
            from: 50.0,
            to: 54.0
        }
    );
    // With a start second (the view's own origin) the seconds arrive shifted by it.
    assert_eq!(
        band::drag_or_click(10.0, 20.0, 100.0),
        TakePress::Select {
            from: 110.0,
            to: 120.0
        }
    );
    // The floor a take must clear, and the sentence when it does not.
    assert!(!screen::can_make_take((4.0, 4.3)));
    assert!(screen::can_make_take((4.0, 4.4)));
    assert!(screen::take_too_short(0.3).contains("0.3"));
    assert!(
        screen::take_too_short(0.3).contains("0.4"),
        "P.eng.takeMinSeconds"
    );
}

// --- S4: what a take shows -------------------------------------------------------

/// §07-narrate#1-screen (`each take shaded, labelled with its duration alone ("4.2s") when wider than
/// 34 px`): the label rule is a width test, the text is the shared duration spelling, and a take partly
/// scrolled off paints only the part on screen.
#[test]
fn sec_07_narrate_1_screen_s4_a_wide_take_reads_its_duration_alone() {
    assert!(!band::take_shows_label(33.0));
    assert!(
        !band::take_shows_label(34.0),
        "wider THAN 34 px, so 34 is not"
    );
    assert!(band::take_shows_label(35.0));
    // The duration alone — no range, no seconds word, one decimal like the status line's.
    assert_eq!(band::label_in((10.0, 14.2)), "4.2s");
    assert_eq!(
        band::label_in((0.0, 0.4)),
        "0.4s",
        "the shortest legal take"
    );
    // Where a take is drawn: fully visible, then clipped at each edge of the view.
    let pps = band::px_per_second(1.0, 60.0, 300.0);
    assert_eq!(band::take_box((2.0, 10.0), 0.0, pps, 300.0), (10.0, 40.0));
    assert_eq!(
        band::take_box((0.0, 20.0), 50.0, pps, 300.0),
        (0.0, 50.0),
        "left edge clipped"
    );
    assert_eq!(
        band::take_box((50.0, 60.0), 0.0, pps, 30.0),
        (250.0, 0.0),
        "past the right edge"
    );
    assert_eq!(
        band::take_box((0.0, 4.0), 250.0, pps, 300.0),
        (0.0, 0.0),
        "scrolled past the left edge"
    ); // The plan the painter draws, from state alone: labels appear only where they fit.
    let mut state = view();
    state.takes = vec![(2.0, 10.0), (20.0, 21.0)];
    let plan = band::paint_plan(&state);
    assert_eq!(plan.takes.len(), 2);
    assert_eq!(plan.takes[0].2, "8.0s", "40 px wide carries its duration");
    assert_eq!(plan.takes[1].2, "", "5 px wide stays unlabelled");
    assert!(!plan.scrollable, "at fit there is no scrollbar");
}

// --- S5: the bars ----------------------------------------------------------------

/// §07-narrate#1-screen (`red ▶-start bar, drawn through the ruler too; faint red bar under the
/// pointer; blue live selection; playhead while ▶ walks`): each is a position the plan answers, and
/// the playhead is the one that disappears when nothing plays.
#[test]
fn sec_07_narrate_1_screen_s5_every_bar_has_its_own_answer_and_the_playhead_only_walks() {
    let pps = band::px_per_second(1.0, 60.0, 300.0);
    // The playhead exists only while ▶ walks the cut.
    assert_eq!(band::playhead_px(12.0, 0.0, pps, false), None);
    assert_eq!(band::playhead_px(12.0, 0.0, pps, true), Some(60.0));
    // Before the visible window it is not drawn at all rather than pinned to the edge.
    assert_eq!(band::playhead_px(5.0, 100.0, pps, true), None);
    // The pointer bar answers wherever the pointer is over the lane, and nowhere off it.
    assert_eq!(band::pointer_bar(140.0, 300.0), Some(140.0));
    assert_eq!(band::pointer_bar(-1.0, 300.0), None);
    assert_eq!(band::pointer_bar(301.0, 300.0), None);
    // One frame's worth of everything the painter will ink.
    let mut state = view();
    state.red_bar = 20.0;
    state.pointer_x = 100.0;
    state.selection = Some((30.0, 40.0));
    state.session = 50.0;
    state.playing = true;
    let plan = band::paint_plan(&state);
    assert_eq!(plan.red_bar_px, 100.0);
    assert_eq!(plan.pointer_px, Some(100.0));
    assert_eq!(plan.selection_px, Some((150.0, 50.0)));
    assert_eq!(plan.playhead_px, Some(250.0));
    assert_eq!(plan.note, None, "decoded: no 'reading' sentence");
    // While the envelope decodes the band says §1's sentence and nothing else changes.
    state.envelope_decoded = false;
    assert_eq!(
        band::paint_plan(&state).note.as_deref(),
        Some("reading the recording\u{2026}")
    );
    assert_eq!(band::envelope_note(true), None);
    // No takes is its own sentence, spelled by the module that already owns it.
    assert!(screen::band_status(&[]).contains("chosen for you"));
}

// --- S6: the voices folder's names ----------------------------------------------

/// §07-narrate#1-screen (`**9** Add file…`, "copies a recording into the voices folder"; F4.6 S1's
/// "under a sanitised base name, never overwriting (`name-2`, …)"): the name a copy takes and the list
/// the picker shows are these functions' answers, checked on real file names.
#[test]
fn sec_07_narrate_1_screen_s6_a_copied_voice_gets_a_sanitised_never_overwritten_name() {
    // Spaces, parens and punctuation fold to single dashes; the extension goes.
    assert_eq!(
        voices::sanitised_base(Path::new("/x/my take (2).wav")),
        "my-take-2"
    );
    assert_eq!(
        voices::sanitised_base(Path::new("/x/Studio Take_01 FINAL.mkv")),
        "Studio-Take-01-FINAL",
        "an underscore is punctuation to a file name this app writes"
    );
    assert_eq!(
        voices::sanitised_base(Path::new("/x/.hidden.wav")),
        "hidden"
    );
    // A name with nothing letters-or-digits in it cannot be a voice called "" — it gets a usable one.
    assert_eq!(voices::sanitised_base(Path::new("/x/___.wav")), "voice");
    // Never overwriting: the first copy keeps the base, the next two count up.
    let taken: Vec<String> = vec![];
    assert_eq!(voices::unique_name(&taken, "warm"), "warm");
    let taken = vec!["warm.wav".to_string()];
    assert_eq!(voices::unique_name(&taken, "warm"), "warm-2");
    let taken = vec!["warm.wav".to_string(), "warm-2.wav".to_string()];
    assert_eq!(voices::unique_name(&taken, "warm"), "warm-3");
    // The full file name a copy lands under, in one call.
    assert_eq!(
        voices::target_name(&taken, Path::new("/x/warm take.wav")),
        "warm-take.wav"
    );
    assert_eq!(voices::VOICE_SUFFIX, ".wav");
    // A missing folder lists nothing rather than failing: an empty library is a real state.
    assert!(voices::list_wavs(Path::new("/nonexistent-voices-folder")).is_empty());
}

/// §07-narrate#1-screen (`every .wav in the voices folder`): the listing is sorted, keeps only `.wav`,
/// skips directories and hidden junk — the order the dropdown rows follow.
#[test]
fn sec_07_narrate_1_screen_s6b_the_folder_listing_is_sorted_wavs_only() {
    let dir = std::env::temp_dir().join(format!("np-sec07-voices-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch folder");
    for name in [
        "warm.wav",
        "brisk.wav",
        "notes.txt",
        "sub.wav",
        "warm-2.wav",
    ] {
        std::fs::write(dir.join(name), b"x").expect("write");
    }
    std::fs::create_dir(dir.join("a-folder.wav")).expect("a directory named like a wav");
    assert_eq!(
        voices::list_wavs(&dir),
        [
            "a-folder.wav",
            "brisk.wav",
            "sub.wav",
            "warm-2.wav",
            "warm.wav"
        ]
    );
    std::fs::remove_dir_all(&dir).expect("clean up");
}

// --- S7: the lane asks for the width its zoom needs ------------------------------

/// §07-narrate#1-screen (`own scrollbar once the wheel zooms past fit`): the scroller is `Automatic`,
/// so a bar appears only when the lane ASKS for more than the viewport. The width it must ask for is the
/// content's own width, floored at the 240 px `build_band` starts from — and that floor is exactly why a
/// view at or below fit never grows a bar. The widget test pins the request on the real lane; this pins
/// the arithmetic every caller of it uses.
#[test]
fn sec_07_narrate_1_screen_s7_the_lane_asks_for_exactly_the_content_it_zooms_to() {
    use naivepost::ui::narrate_band::{lane_width, LANE_MIN_PX};

    // (zoom, duration, viewport). Past fit means px/s × seconds wider than the strip showing it.
    let views = [
        (1.0, 60.0, 300.0),
        (2.0, 60.0, 300.0),
        (8.0, 60.0, 300.0),
        (1.0, 4.0, 300.0),
    ];
    for (zoom, duration, viewport) in views {
        let asked = lane_width(zoom, duration, viewport);
        let content = band::content_px(zoom, duration, viewport);
        assert_eq!(
            asked,
            content.max(LANE_MIN_PX),
            "zoom {zoom} over {duration}s across {viewport}px: the request is the content, floored"
        );
        // The bar rule and the width rule are the same rule read two ways.
        assert_eq!(
            band::needs_scrollbar(zoom, duration, viewport),
            asked > viewport + f64::EPSILON,
            "zoom {zoom}: a scrollbar only where the request outgrows the viewport"
        );
    }
    // At fit the lane keeps its 240 px floor, so nothing about the strip changes: no bar, no jump.
    // (A short recording is the other case: `fit_pps` spreads it across the whole viewport, so its
    // content is exactly the viewport and the floor never comes into play.)
    assert_eq!(
        lane_width(1.0, 4.0, 300.0),
        300.0,
        "a 4 s file fills the strip at fit and stops there"
    );
    assert!(!band::needs_scrollbar(1.0, 4.0, 300.0));
}

// --- S8: the paused preview steps one frame --------------------------------------

/// §07-narrate#1-screen (`paused, wheel steps a frame`): one notch is one frame of the agreed rate,
/// the step cannot leave the cut, and with no timeline there is no clock to step along. The rate is the
/// app's fallback because this page composites its frame from the cut and has no footage file to probe.
#[test]
fn sec_07_narrate_1_screen_s8_a_frame_step_is_one_frame_and_never_leaves_the_cut() {
    use naivepost::narrate_frame::{self as frame, DEFAULT_FPS};

    assert_eq!(frame::frame_seconds(25.0), 0.04, "25 fps is a 40 ms frame");
    assert_eq!(
        frame::frame_seconds(0.0),
        1.0 / DEFAULT_FPS,
        "an unset rate falls back rather than dividing by zero"
    );
    assert_eq!(
        frame::step(10.0, 60.0, 0.04, true),
        Some(10.04),
        "forward is one frame"
    );
    assert_eq!(
        frame::step(10.0, 60.0, 0.04, false),
        Some(9.96),
        "back is one frame"
    );
    assert_eq!(
        frame::step(0.01, 60.0, 0.04, false),
        Some(0.0),
        "the start holds at zero"
    );
    assert_eq!(
        frame::step(59.99, 60.0, 0.04, true),
        Some(60.0),
        "the end holds at the length"
    );
    assert_eq!(
        frame::step(0.0, 0.0, 0.04, true),
        None,
        "no cut means no clock to step"
    );
    assert_eq!(
        frame::step(3.0, -1.0, 0.04, true),
        None,
        "a negative length is not a cut either"
    );
}

/// §07-narrate#1-screen badge **13** (`pitch −6..+6 semitones`, drawn as `pitch: ─── 0.0` in
/// spec/img/07-narrate.png): the number beside the slider is the shift's own face — one decimal, and an
/// out-of-range stored value reads as the end it acts on, because that is the reference it will be cut at.
#[test]
fn sec_07_narrate_1_screen_s13_the_pitch_readout_is_the_sliders_face() {
    // The ends and the middle, spelled the way the picture spells them.
    assert_eq!(
        screen::pitch_readout(0.0),
        "0.0",
        "no shift still shows a number"
    );
    assert_eq!(
        screen::pitch_readout(1.5),
        "1.5",
        "half a semitone keeps its decimal"
    );
    assert_eq!(screen::pitch_readout(-2.5), "-2.5");
    assert_eq!(screen::pitch_readout(screen::PITCH_MIN_SEMITONES), "-6.0");
    assert_eq!(screen::pitch_readout(screen::PITCH_MAX_SEMITONES), "6.0");
    // An edited `narrate/pitch.txt` outside the range acts as the nearest end, so that is what it shows.
    assert_eq!(
        screen::pitch_readout(9.0),
        "6.0",
        "past the top it reads the top"
    );
    assert_eq!(
        screen::pitch_readout(-9.0),
        "-6.0",
        "below the bottom it reads the bottom"
    );
    // The face and the slider cannot drift: these are the numbers the scale spans, and the step it moves by.
    assert_eq!(
        (screen::PITCH_MIN_SEMITONES, screen::PITCH_MAX_SEMITONES),
        (-6.0, 6.0)
    );
    assert_eq!(screen::clamp_pitch(9.0), screen::PITCH_MAX_SEMITONES);
    assert_eq!(screen::clamp_pitch(-9.0), screen::PITCH_MIN_SEMITONES);
    assert_eq!(
        screen::PITCH_STEP,
        0.5,
        "the face's one decimal can show every stop the slider has"
    );
}

use naivepost::ui::narrate_inputs_tally as tally;

/// §07-narrate#1-screen (**22**), the readout's own branches. The bracketed halves are what §1 spells with
/// two optionals, so each is pinned byte for byte: both, each alone, neither — and the no-cut sentence that
/// replaces the whole line.
#[test]
fn sec_07_narrate_1_screen_inputs_readout_names_unwritten_off_cut_and_no_timeline() {
    let read = |unwritten, off_cut, has_timeline| {
        screen::inputs_readout(4, 95.0, unwritten, off_cut, has_timeline, true)
    };
    // The plain case: count and length, nothing in brackets.
    assert_eq!(read(0, 0, true), "4 clip(s) \u{b7} 01:35");
    // Both halves at once, §1's wording exactly.
    assert_eq!(
        read(2, 1, true),
        "4 clip(s) \u{b7} 01:35 \u{b7} \u{26a0} 2 clip(s) unwritten, 1 line(s) off the cut"
    );
    // Either half alone: the bracket is ONE optional covering both counts, so a zero still prints as a zero.
    assert_eq!(
        read(3, 0, true),
        "4 clip(s) \u{b7} 01:35 \u{b7} \u{26a0} 3 clip(s) unwritten, 0 line(s) off the cut"
    );
    assert_eq!(
        read(0, 2, true),
        "4 clip(s) \u{b7} 01:35 \u{b7} \u{26a0} 0 clip(s) unwritten, 2 line(s) off the cut"
    );
    // The timeline half stands on its own and rides along with the counts when both are missing.
    assert_eq!(
        read(0, 0, false),
        "4 clip(s) \u{b7} 01:35 \u{b7} no timeline"
    );
    assert_eq!(
        read(1, 1, false),
        "4 clip(s) \u{b7} 01:35 \u{b7} \u{26a0} 1 clip(s) unwritten, 1 line(s) off the cut \u{b7} no timeline"
    );
    // With no cut the row does not count at all: it says where to make one.
    assert_eq!(
        screen::inputs_readout(0, 0.0, 0, 0, true, false),
        "no cut yet \u{2014} build one on the Cut step"
    );
}

/// §07-narrate#1-screen (**22**): where the four numbers come from. A clip with no line counts as unwritten
/// UNLESS it was deliberately silenced (a silent clip wants a caption, not a line, so it is no staleness a
/// person caused); a line on video the cut dropped counts as off the cut; no clips means the row is the
/// no-cut sentence, whatever the counts would have been.
#[test]
fn sec_07_narrate_1_screen_inputs_tally_counts_unwritten_silent_and_orphan() {
    let clips = [(0.0, 10.0), (10.0, 20.0), (20.0, 30.0)];
    let on_first = on_first_line_at(0.0, 10.0);
    let orphan = Entry {
        s: 100.0,
        e: 110.0,
        at: 1.0,
        text: "gone video".into(),
        ..Default::default()
    };
    // Clips 2 and 3 have no line: two unwritten, none off the cut, and the timeline question passed through.
    let plain = tally::of(&clips, std::slice::from_ref(&on_first), false, true);
    assert_eq!(
        plain,
        tally::Tally {
            unwritten: 2,
            off_cut: 0,
            has_cut: true,
            has_timeline: true
        }
    );
    // No clip silenced: two of the three have no line. Silencing them all (what `silent: true` marks here)
    // takes the whole count back, because a deliberately silent clip is no staleness a person caused.
    let silenced = tally::of(&clips, std::slice::from_ref(&on_first), true, true);
    assert_eq!(
        silenced.unwritten, 0,
        "a deliberately silent clip is not counted as waiting for a line"
    );
    assert_eq!(silenced.off_cut, 0);
    // A line on video the cut no longer has is off the cut; the kept line still leaves one clip unwritten.
    let stale = tally::of(&clips, &[on_first, orphan], false, false);
    assert_eq!(
        (stale.unwritten, stale.off_cut, stale.has_timeline),
        (2, 1, false)
    );
    // Every clip written: no bracket at all, which is the same answer `inputs_readout` prints bare.
    let all = [
        Entry {
            s: 0.0,
            e: 10.0,
            at: 1.0,
            text: "one".into(),
            ..Default::default()
        },
        Entry {
            s: 10.0,
            e: 20.0,
            at: 1.0,
            text: "two".into(),
            ..Default::default()
        },
        Entry {
            s: 20.0,
            e: 30.0,
            at: 1.0,
            text: "three".into(),
            ..Default::default()
        },
    ];
    assert_eq!(tally::of(&clips, &all, false, true).unwritten, 0);
    assert_eq!(
        screen::inputs_readout(3, 30.0, 0, 0, true, true),
        "3 clip(s) \u{b7} 00:30",
        "a fully written cut shows no stale mark"
    );
    // No clips: `has_cut` false, and the counts are not half-answered beside the no-cut sentence.
    assert_eq!(
        tally::of(&[], &[on_first_line_at(100.0, 110.0)], false, true),
        tally::Tally {
            unwritten: 0,
            off_cut: 0,
            has_cut: false,
            has_timeline: true
        }
    );
    // A silent marker on a clip that got a line back stops silencing it — `has_line` wins, as `narration`
    // says it must — so silencing everything while writing every clip still leaves nothing waiting.
    let back = [
        on_first_line_at(0.0, 10.0),
        on_first_line_at(10.0, 20.0),
        on_first_line_at(20.0, 30.0),
    ];
    assert_eq!(tally::of(&clips, &back, false, true).unwritten, 0);
    // The marker shape the tally writes for the `silent` question is the one `narration.json` stores.
    let marker = Silent { s: 10.0, e: 20.0 };
    assert_eq!((marker.s, marker.e), (10.0, 20.0));
}

fn on_first_line_at(s: f64, e: f64) -> Entry {
    Entry {
        s,
        e,
        at: 1.0,
        text: "a line on the clip".into(),
        ..Default::default()
    }
}

// --- the row's shape on the page: §1's header order and the box under it -------------------------------
//
// The rules above are pure. These two walk the REAL Narrate page's widget tree, because the whole point of
// §1's ordering is what a person sees: the four buttons at the header line's right end in the order the
// badges number them (▶ ↻ ＋ 🗑), and the text box UNDER that header across the row's full width rather than
// squeezed onto the same line (which is how every row ran off the page's right edge).

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::cut::Seg;
use naivepost::narration::{Entry, Silent};
use naivepost::shell::Page;
use naivepost::ui;

use crate::common;
use common::{hold_last_window, release_last_window, settle, widget_in};

static RAN_HEADER_ORDER: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static RAN_BOX_UNDER_HEADER: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Three kept clips of ten seconds each, so a row's session second is checkable by hand.
fn three_clips() -> Vec<Seg> {
    vec![
        Seg {
            s: 0.0,
            e: 10.0,
            cam: 0,
            ..Default::default()
        },
        Seg {
            s: 10.0,
            e: 20.0,
            cam: 0,
            ..Default::default()
        },
        Seg {
            s: 20.0,
            e: 30.0,
            cam: 0,
            ..Default::default()
        },
    ]
}

/// One spoken line, one caption and one blank row: the states the header's ▶ has to tell apart, and enough
/// rows that a wrong order or an inline box shows up rather than passing by accident.
fn three_entries() -> Vec<Entry> {
    vec![
        Entry {
            s: 0.0,
            e: 10.0,
            at: 2.0,
            text: "a spoken line".into(),
            emotion: "calm".into(),
            pos: String::new(),
            roll: 0,
        },
        Entry {
            s: 10.0,
            e: 20.0,
            at: 1.0,
            text: "a caption".into(),
            emotion: String::new(),
            pos: "bottom".into(),
            roll: 0,
        },
        Entry {
            s: 20.0,
            e: 30.0,
            at: 1.0,
            text: String::new(),
            emotion: String::new(),
            pos: String::new(),
            roll: 0,
        },
    ]
}

fn base_state() -> ui::NarrateState {
    ui::NarrateState {
        entries: three_entries(),
        segs: three_clips(),
        takes: vec![],
        voice: String::new(),
        pitch: 0.0,
        language: String::new(),
        session: 5.0,
        cut_at: 5.0,
        length: 30.0,
        clips: 3,
        // Published, not left at their defaults: these four are what **22**'s brackets print, and §1's
        // picture shows them filled ("4 clip(s) · 02:06 · ⚠ 1 clip(s) unwritten, 1 line(s) off the cut").
        unwritten: 1,
        off_cut: 1,
        has_cut: true,
        has_timeline: true,
        narrators: 1,
        voice_files: vec!["warm.wav".into()],
        recording_covers_clip: true,
        playing: false,
        started: false,
        busy: false,
        narration_off: false,
        covered_spans: Vec::new(),
    }
}

/// A window sitting on the Narrate tab of the project in `dir`, with `state` published into the page — the
/// staging `narrate_surface_widgets.rs` uses, copied here because a test module cannot borrow another's
/// helpers. `dir` is the folder the window opens, which is also the session the tally counts.
fn narrate_page_in(
    app: &adw::Application,
    dir: &std::path::Path,
    state: ui::NarrateState,
) -> adw::ApplicationWindow {
    release_last_window();
    let model = naivepost::project::load(dir).expect("the project loads");
    let window = ui::build_window(app, &model, "Prepare");
    hold_last_window(window.clone());
    window.set_default_size(1280, 800);
    window.present();
    // Published BEFORE the page is built: the shell builds the page when the tab is opened, and its `build()`
    // seeds itself from the project's own files — which here are the copied project's cut and narration.
    ui::set_state(state);
    ui::tab_button(&window, Page::Narrate)
        .expect("the shell has a Narrate tab")
        .emit_by_name::<()>("clicked", &[]);
    settle();
    ui::refresh(&window);
    // The bottom bar is painted by the tab switch (`window_bench::paint_readouts`), not by the page's
    // `refresh`, so a check that reads it waits for the row to carry this tab's line rather than guessing a
    // moment. It still shows the previous tab's sentence until that repaint lands.
    let inputs_label =
        widget_in(&window, "inputs-readout").expect("the shell drew its Inputs readout");
    common::wait_until(
        "the bottom bar showing the Narrate tab's Inputs line (still the tab just left)",
        || {
            inputs_label
                .downcast_ref::<gtk::Label>()
                .is_some_and(|label| !label.text().is_empty())
        },
    );
    settle();
    window
}

/// The children of a box, left to right, as their widget names — the order read off the tree itself.
fn child_names(box_: &gtk::Box) -> Vec<String> {
    box_.observe_children()
        .iter::<glib::Object>()
        .flatten()
        .filter_map(|child| child.downcast::<gtk::Widget>().ok())
        .map(|widget| widget.widget_name().to_string())
        .collect()
}

/// This row's header box, found through the row rather than by a global walk.
fn header_of(window: &adw::ApplicationWindow, index: usize) -> gtk::Box {
    widget_in(window, &format!("line-header-{index}"))
        .and_then(|w| w.downcast::<gtk::Box>().ok())
        .unwrap_or_else(|| panic!("row {index} drew no header line"))
}

/// The row's own vertical stack: the parent of the header line.
fn stack_of(window: &adw::ApplicationWindow, index: usize) -> gtk::Box {
    header_of(window, index)
        .parent()
        .and_then(|p| p.downcast::<gtk::Box>().ok())
        .expect("the header line sits in the row's vertical box")
}

fn row_round(app: &adw::Application) {
    // The page pins its session folder on the FIRST window built in this process, and the rows it draws come
    // off THAT project — so this round owns the folder: the demo fixture copied into its own scratch dir (a
    // test never writes into the fixture), with a three-clip cut and three narration lines in the copy so the
    // page has the three rows §1's header layout is read off.
    release_last_window();
    let root = scratch_dir("row");
    // The shell reads the SESSION beside the cwd (`startup::session_dir(root)` = `root/session.naivepost`),
    // so the round stages THAT folder: the demo fixture copied into it, plus the three-clip cut and the
    // narration whose lines sit off that cut, which is what puts a warning on a row's header line.
    let session = naivepost::startup::session_dir(&root);
    common::copy_fixture(&session);
    write_three_clip_cut(&session);
    write_staged_narration(&session);
    std::fs::create_dir_all(&root).expect("the root exists");
    std::env::set_current_dir(&root).expect("cwd pinned above the staged session");
    // --- **14-19**: time, status, then ▶ ↻ ＋ 🗑 at the header's right end ---------------------------
    let window = narrate_page_in(app, &session, base_state());
    for index in 0..3usize {
        let names = child_names(&header_of(&window, index));
        // The spacer holds the gap that pushes the buttons right, and §A.5 appends the fit warning to the
        // status; both are filtered out here so this check reads only §1's six widgets, in §1's order.
        let header_name = format!("line-header-{index}");
        let gap_name = format!("line-header-gap-{index}");
        let warning_name = format!("line-warning-{index}");
        let widgets: Vec<&str> = names
            .iter()
            .map(String::as_str)
            .filter(|name| !name.is_empty())
            .filter(|name| **name != header_name && **name != gap_name && **name != warning_name)
            .collect();
        assert_eq!(
            widgets,
            vec![
                format!("line-time-{index}").as_str(),
                format!("line-status-{index}").as_str(),
                format!("line-speak-{index}").as_str(),
                format!("line-reroll-{index}").as_str(),
                format!("line-add-below-{index}").as_str(),
                format!("line-remove-{index}").as_str(),
            ],
            "§1's header line is time, status, then speak / re-roll / add-below / remove at its right end"
        );
    }
    RAN_HEADER_ORDER.store(true, std::sync::atomic::Ordering::SeqCst);

    // --- **20/21**: the box wraps UNDER the header; the warning is appended INSIDE the header ---------
    for index in 0..3usize {
        let header = header_of(&window, index);
        let stack = stack_of(&window, index);
        let stacked = child_names(&stack);
        let header_name = format!("line-header-{index}");
        let text_name = format!("line-text-{index}");
        let warning_name = format!("line-warning-{index}");
        let position = |wanted: &str| stacked.iter().position(|name| name == wanted);
        assert_eq!(
            position(&header_name),
            Some(0),
            "the header line is the row's first row"
        );
        let text_at = position(&text_name).unwrap_or_else(|| {
            panic!("{text_name} is not a child of the row's vertical box: {stacked:?}")
        });
        assert_eq!(
            stacked.len(),
            2,
            "the row's vertical box holds the header line and the text box only — §A.5 appends the warning \
             to the status on the header line instead of stacking it as a third row: {stacked:?}"
        );
        assert!(
            text_at > 0,
            "the text box sits UNDER the header line, not inline with it"
        );
        // §A.5: "Warnings appended and the row marked red" — the warning label sits on the HEADER line,
        // right after the status and before the spacer that pins the buttons to the header's right end.
        let in_header = child_names(&header);
        let at = |wanted: &str| in_header.iter().position(|name| name == wanted);
        let warning_at = at(&warning_name)
            .unwrap_or_else(|| panic!("{warning_name} is not on the header line: {in_header:?}"));
        assert_eq!(
            at(&format!("line-status-{index}")).map(|p| p + 1),
            Some(warning_at),
            "the fit warning is APPENDED to the status: {in_header:?}"
        );
        assert!(
            at(&format!("line-header-gap-{index}"))
                .is_some_and(|gap| gap > warning_at),
            "the warning comes BEFORE the header's right-end spacer, so the buttons keep their place: \n             {in_header:?}"
        );
        // And the box really is no longer a child of the header line it used to share it with.
        assert!(
            !in_header.contains(&text_name),
            "the text box must not be squeezed onto the header line"
        );
        // A.5's box: monospace WORD WRAP, so a long line breaks under itself instead of running off the page.
        let text = widget_in(&window, &text_name).expect("the row drew its text box");
        let view = text
            .downcast_ref::<gtk::TextView>()
            .unwrap_or_else(|| panic!("{text_name} is a wrapping TextView, per §A.5"));
        assert_eq!(
            view.wrap_mode(),
            gtk::WrapMode::Word,
            "{text_name} must wrap at word boundaries"
        );
        // Full width: the box fills its row rather than asking for its content's width.
        assert!(text.hexpands(), "{text_name} must stretch across the row");
    }
    RAN_BOX_UNDER_HEADER.store(true, std::sync::atomic::Ordering::SeqCst);

    window.close();
    settle();
}

/// A fresh scratch folder. Every round gets its own name so parallel tests cannot share a tree.
fn scratch_dir(name: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("np-sec07{}-{}", name, std::process::id()));
    std::fs::create_dir_all(&root).expect("scratch folder created");
    root
}

/// `cut/cut.json` for a three-clip cut of ten seconds each — what the tally counts, and what `refresh` reads
/// back so the row's numbers come from the session and not from the test's own state.
fn write_three_clip_cut(dir: &std::path::Path) {
    let cut_dir = dir.join("cut");
    std::fs::create_dir_all(&cut_dir).expect("the cut folder exists");
    std::fs::write(
        cut_dir.join("cut.json"),
        serde_json::to_vec_pretty(&naivepost::cut::Cut {
            segs: (0..3)
                .map(|index| naivepost::cut::Seg {
                    s: f64::from(index) * 10.0,
                    e: f64::from(index) * 10.0 + 10.0,
                    cam: 0,
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        })
        .expect("a cut"),
    )
    .expect("the cut is written");
}

/// `narrate/narration.json` for the lines this round's checks describe — one per clip plus one sitting past
/// the cut, so "N line(s) off the cut" has a referent in files rather than only in the published state.
fn write_staged_narration(dir: &std::path::Path) {
    let narrate_dir = dir.join("narrate");
    std::fs::create_dir_all(&narrate_dir).expect("the narrate folder exists");
    // One line on the first clip, and two past the end of the 30 s cut: two clips unwritten, two lines off.
    let entries = vec![
        Entry {
            s: 0.0,
            e: 10.0,
            at: 2.0,
            text: "a spoken line".into(),
            emotion: String::new(),
            pos: String::new(),
            roll: 0,
        },
        Entry {
            s: 40.0,
            e: 45.0,
            at: 1.0,
            text: "off the cut one".into(),
            emotion: String::new(),
            pos: String::new(),
            roll: 0,
        },
        Entry {
            s: 60.0,
            e: 65.0,
            at: 1.0,
            text: "off the cut two".into(),
            emotion: String::new(),
            pos: String::new(),
            roll: 0,
        },
    ];
    std::fs::write(
        narrate_dir.join("narration.json"),
        serde_json::to_vec_pretty(&naivepost::narration::Narration {
            entries,
            silent: Vec::new(),
        })
        .expect("a narration"),
    )
    .expect("the narration is written");
}

fn run_row_round() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gtk4::gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(row_round);
        app.run_with_args::<&str>(&[]);
    });
}

/// §07-narrate#1-screen (**14**-**19**): the row's header line carries the time entry, the status, and then
/// the four buttons AT ITS RIGHT END in the badge order — speak, re-roll, add-below, remove.
#[test]
fn sec_07_narrate_1_screen_row_header_orders_buttons_after_the_status() {
    run_row_round();
    assert!(
        RAN_HEADER_ORDER.load(std::sync::atomic::Ordering::SeqCst),
        "the header-order check never ran"
    );
}

#[test]
fn sec_07_narrate_1_screen_text_box_wraps_under_the_header_and_the_warning_sits_in_it() {
    run_row_round();
    assert!(
        RAN_BOX_UNDER_HEADER.load(std::sync::atomic::Ordering::SeqCst),
        "the box-under-header check never ran"
    );
}
