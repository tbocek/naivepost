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
    assert!((band::zoom_step(1.0, 1.0) - 1.0).abs() < 1e-12, "fit is the floor: never less");
    assert!((band::zoom_step(4.0, 1.0) - 3.2).abs() < 1e-9, "a down-notch divides by the ratio");
    // Two notches compound the way the spec's ratio says.
    assert!((band::zoom_step(1.0, -2.0) - 1.5625).abs() < 1e-9);
    // Past fit the px/s grows by the same ratio, and stops growing at 200 px/s however hard the wheel
    // is rolled: a wider-than-max view would draw peaks the cache does not hold.
    let mut zoom = 1.0;
    for _ in 0..60 {
        zoom = band::zoom_step(zoom, -1.0);
    }
    assert_eq!(band::px_per_second(zoom, 60.0, 300.0), screen::BAND_MAX_PPS);
    assert_eq!(band::px_per_second(1000.0, 60.0, 300.0), screen::BAND_MAX_PPS, "the cap holds");
    // A shift-notch pans an eighth of the view.
    assert_eq!(band::pan_by(300.0), 37.5);
    // Clamping keeps the left edge inside: never negative, never past the last screenful of file.
    assert_eq!(band::clamp_view(-40.0, 2.0, 60.0, 300.0), 0.0);
    let span = band::content_px(2.0, 60.0, 300.0);
    assert_eq!(span, 600.0, "2× of 5 px/s over 60 s");
    assert_eq!(band::clamp_view(9999.0, 2.0, 60.0, 300.0), span - 300.0);
    assert_eq!(band::clamp_view(120.0, 2.0, 60.0, 300.0), 120.0, "an honest offset stands");
    // At fit the whole recording IS the view, so there is nothing to pan to.
    assert_eq!(band::clamp_view(50.0, 1.0, 60.0, 300.0), 0.0);
    // The scrollbar question follows the same arithmetic: absent at fit, present past it (§1's "own
    // scrollbar once the wheel zooms past fit").
    assert!(!band::needs_scrollbar(1.0, 60.0, 300.0));
    assert!(band::needs_scrollbar(band::zoom_step(1.0, -1.0), 60.0, 300.0));
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
        TakePress::Select { from: 50.0, to: 54.0 }
    );
    // Backwards drags name their seconds in order, because a take's seconds have no direction.
    assert_eq!(
        band::drag_or_click(54.0, 50.0, 0.0),
        TakePress::Select { from: 50.0, to: 54.0 }
    );
    // With a start second (the view's own origin) the seconds arrive shifted by it.
    assert_eq!(
        band::drag_or_click(10.0, 20.0, 100.0),
        TakePress::Select { from: 110.0, to: 120.0 }
    );
    // The floor a take must clear, and the sentence when it does not.
    assert!(!screen::can_make_take((4.0, 4.3)));
    assert!(screen::can_make_take((4.0, 4.4)));
    assert!(screen::take_too_short(0.3).contains("0.3"));
    assert!(screen::take_too_short(0.3).contains("0.4"), "P.eng.takeMinSeconds");
}

// --- S4: what a take shows -------------------------------------------------------

/// §07-narrate#1-screen (`each take shaded, labelled with its duration alone ("4.2s") when wider than
/// 34 px`): the label rule is a width test, the text is the shared duration spelling, and a take partly
/// scrolled off paints only the part on screen.
#[test]
fn sec_07_narrate_1_screen_s4_a_wide_take_reads_its_duration_alone() {
    assert!(!band::take_shows_label(33.0));
    assert!(!band::take_shows_label(34.0), "wider THAN 34 px, so 34 is not");
    assert!(band::take_shows_label(35.0));
    // The duration alone — no range, no seconds word, one decimal like the status line's.
    assert_eq!(band::label_in((10.0, 14.2)), "4.2s");
    assert_eq!(band::label_in((0.0, 0.4)), "0.4s", "the shortest legal take");
    // Where a take is drawn: fully visible, then clipped at each edge of the view.
    let pps = band::px_per_second(1.0, 60.0, 300.0);
    assert_eq!(band::take_box((2.0, 10.0), 0.0, pps, 300.0), (10.0, 40.0));
    assert_eq!(band::take_box((0.0, 20.0), 50.0, pps, 300.0), (0.0, 50.0), "left edge clipped");
    assert_eq!(band::take_box((50.0, 60.0), 0.0, pps, 30.0), (250.0, 0.0), "past the right edge");
    assert_eq!(band::take_box((0.0, 4.0), 250.0, pps, 300.0), (0.0, 0.0), "scrolled past the left edge");    // The plan the painter draws, from state alone: labels appear only where they fit.
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
    assert_eq!(voices::sanitised_base(Path::new("/x/my take (2).wav")), "my-take-2");
    assert_eq!(
        voices::sanitised_base(Path::new("/x/Studio Take_01 FINAL.mkv")),
        "Studio-Take-01-FINAL",
        "an underscore is punctuation to a file name this app writes"
    );
    assert_eq!(voices::sanitised_base(Path::new("/x/.hidden.wav")), "hidden");
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
    for name in ["warm.wav", "brisk.wav", "notes.txt", "sub.wav", "warm-2.wav"] {
        std::fs::write(dir.join(name), b"x").expect("write");
    }
    std::fs::create_dir(dir.join("a-folder.wav")).expect("a directory named like a wav");
    assert_eq!(
        voices::list_wavs(&dir),
        ["a-folder.wav", "brisk.wav", "sub.wav", "warm-2.wav", "warm.wav"]
    );
    std::fs::remove_dir_all(&dir).expect("clean up");
}
