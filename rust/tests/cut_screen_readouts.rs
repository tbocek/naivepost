//! §05-cut#1-screen — the form column's idle readings and the timeline's bands.
//!
//! The eight rows the page shows when nothing is being placed, the arithmetic behind the two lengths
//! it reads out, whether the picture band has a frame to draw at any zoom, and the heights the bands
//! are laid out with — all of it answered in [`naivepost::cut_screen`], so none of it needs a window.

use naivepost::cut::{Cut, Seg};
use naivepost::cut_screen;

/// A kept stretch of footage at some playback speed. `rate` 0 is what a hand-made scene carries:
/// only produceSegs writes one (spec/inventory/cut.md §B).
fn footage(start: f64, end: f64, rate: f64) -> Seg {
    Seg { s: start, e: end, rate, ..Default::default() }
}

#[test]
fn sec_05_cut_1_screen_s6_the_idle_form_reads_the_session_eight_ways() {
    // §A: "Idle rows: Thumbnails …, Aspect ratio …, Playhead …, Selection [marks], Cut [total],
    // Cut at 1× [totalRaw], Source [totalSrc], Segments [totalSegs]" — eight rows, this order.
    let rows = cut_screen::idle_readouts(64, "", 0.0, None, &Cut::default(), 0.0);
    let labels: Vec<&str> = rows.iter().map(|row| row.label).collect();
    assert_eq!(
        labels,
        [
            "Thumbnails",
            "Aspect ratio",
            "Playhead",
            "Selection",
            "Cut",
            "Cut at 1\u{00d7}",
            "Source",
            "Segments"
        ]
    );

    let value = |label: &str| {
        rows.iter()
            .find(|row| row.label == label)
            .unwrap_or_else(|| panic!("no {label} row"))
            .value
            .clone()
    };
    assert_eq!(value("Segments"), "0");
    assert_eq!(value("Cut"), "00:00");
    assert_eq!(value("Thumbnails"), "64 px");
    assert_eq!(value("Aspect ratio"), cut_screen::ASPECT_DEFAULT, "an unset aspect still has a shape");
    assert_eq!(value("Selection"), "", "a reading, not a prompt");

    // §A supplies a tooltip for the thumbnail buttons and nothing else.
    let tip = |label: &str| {
        rows.iter().find(|row| row.label == label).unwrap().tip.to_string()
    };
    assert_eq!(tip("Thumbnails"), "smaller|larger thumbnails on the tracks");
    assert_eq!(tip("Segments"), "");
}

#[test]
fn sec_05_cut_1_screen_s7_cut_and_cut_at_1x_disagree_by_the_speed_effects() {
    // Cut [total] runs at the cut's own speeds; Cut at 1× [totalRaw] is the same stretches plain.
    let cut = Cut {
        segs: vec![
            footage(0.0, 10.0, 2.0),
            Seg { ins: "card.svg".into(), dur: 3.0, ..Default::default() },
        ],
        ..Default::default()
    };
    assert_eq!(cut_screen::cut_seconds(&cut), 8.0, "five seconds at 2\u{00d7}, plus the card's three");
    assert_eq!(cut_screen::cut_seconds_at_1x(&cut), 13.0);
    assert_eq!(cut_screen::segment_count(&cut), 2);

    let rows = cut_screen::idle_readouts(64, "9:16", 75.0, Some((12.0, 20.0)), &cut, 3600.0);
    let value = |label: &str| {
        rows.iter().find(|row| row.label == label).unwrap().value.clone()
    };
    assert_eq!(value("Cut"), "00:08");
    assert_eq!(value("Cut at 1\u{00d7}"), "00:13");
    assert_eq!(value("Aspect ratio"), "9:16", "what the cut stores is what the row reads");
    assert_eq!(value("Playhead"), "01:15");
    assert_eq!(value("Source"), "60:00", "mm_ss lets the minutes run past an hour");
    assert_eq!(value("Selection"), "00:12 \u{2013} 00:20");
}

#[test]
fn sec_05_cut_1_screen_s8_the_thumbnail_ladder_steps_by_a_third_and_clamps() {
    // §A: "×3/4, ×4/3, clamp 40..160". The steps are one ratio in both directions, so the ladder is
    // reversible and neither end can be overshot by holding the button.
    let mut px = cut_screen::THUMB_AT_OPEN;
    for _ in 0..4 {
        px = cut_screen::thumb_up(px);
        assert!(px <= cut_screen::THUMB_MAX, "{px} over the ceiling");
    }
    assert_eq!(px, cut_screen::THUMB_MAX);

    let mut px = cut_screen::THUMB_AT_OPEN;
    for _ in 0..4 {
        px = cut_screen::thumb_down(px);
        assert!(px >= cut_screen::THUMB_MIN, "{px} under the floor");
    }
    assert_eq!(px, cut_screen::THUMB_MIN);

    assert_eq!(cut_screen::thumb_up(cut_screen::THUMB_MAX), cut_screen::THUMB_MAX);
    assert_eq!(cut_screen::thumb_down(cut_screen::THUMB_MIN), cut_screen::THUMB_MIN);
    // One step each way is the size you started with, within a px of rounding.
    let back = cut_screen::thumb_down(cut_screen::thumb_up(64));
    assert!((back as i32 - 64).abs() <= 1, "{back} after one step each way");
}

#[test]
fn sec_05_cut_1_screen_s9_the_picture_band_counts_the_seconds_between_two_pictures() {
    // §1: "New: on the 250 ms grid restarted at each scene change (F1.6) the band has a picture at
    // every zoom; prototype: one frame every Freq seconds, so zoomed in the band shows black between
    // them." The row that makes the grid is `P.eng.frameGridSeconds` = 0.25, which is
    // prepare::EXTRACTION_GRID; the slot is what has to fit inside it.
    let slot = |thumb: u32, aspect: &str| cut_screen::slot_seconds(thumb, aspect, cut_screen::ZOOM_MAX);
    // A 64 px row of 16:9 footage is 113.8 px wide; at 4 px a second one thumbnail slot covers
    // nearly half a minute of the session, which is why zooming in is what makes the grid matter.
    assert!((cut_screen::slot_seconds(64, "16:9", 4.0) - 28.444).abs() < 0.01);

    // The grid's own rule, held as the number it is: one frame per 250 ms of footage, restarted at
    // every scene change, and finer than the Freq spacing the prototype drew its band from.
    assert_eq!(naivepost::prepare::EXTRACTION_GRID, 0.25); // P.eng.frameGridSeconds
    assert!(naivepost::prepare::EXTRACTION_GRID < naivepost::prepare::FREQ_DEFAULT);

    // Smaller rows and portrait shapes need the finest grid least, so they are the ones that clear it.
    assert!(slot(cut_screen::THUMB_MIN, "9:16") < slot(cut_screen::THUMB_AT_OPEN, "16:9"));

    // At the ceiling a 64 px 16:9 row wants a frame every 0.47 s of footage, finer than
    // P.eng.frameGridSeconds already is — so this page does not claim the grid covers it, and says so
    // as a number rather than as a bool that could only be wished true. Zooming in makes a slot cover
    // LESS footage (28.4 s at the opening 4 px/s), which is exactly where §1 says the prototype showed
    // black between its one-frame-per-Freq pictures.
    assert!(!cut_screen::picture_at_every_zoom(cut_screen::ZOOM_MAX, cut_screen::THUMB_AT_OPEN, "16:9"));
    let at_ceiling = slot(cut_screen::THUMB_AT_OPEN, "16:9");
    assert!(at_ceiling > naivepost::prepare::EXTRACTION_GRID && at_ceiling < 0.5, "{at_ceiling} s a slot");
    assert!(
        cut_screen::slot_seconds(64, "16:9", cut_screen::ZOOM_AT_OPEN) > at_ceiling,
        "a slot shrinks as the zoom deepens"
    );

    // An aspect the page cannot parse falls back to the shipped shape rather than dividing by nothing.
    assert_eq!(slot_seconds_unparsed(), cut_screen::slot_seconds(64, "16:9", 240.0));
    assert_eq!(cut_screen::slot_seconds(64, "16:9", 0.0), 0.0);
}

/// The value an unreadable aspect falls back to — the same as the shipped one's.
fn slot_seconds_unparsed() -> f64 {
    cut_screen::slot_seconds(64, "", 240.0)
}

#[test]
fn sec_05_cut_1_screen_s10_the_bands_and_the_red_line_are_the_heights_the_page_draws_with() {
    // §A's Tracks paragraph and §1's "Red playhead: 2 px line on its own layer across both bands".
    assert_eq!(cut_screen::GUTTER_PX, 30.0);
    assert_eq!(cut_screen::RULER_PX, 18.0);
    assert_eq!(cut_screen::SELECTION_BAND_PX, 22.0);
    assert_eq!(cut_screen::EFFECT_ROW_PX, 26.0, "one row per overlapping group");
    assert_eq!(cut_screen::WAVE_LANE_PX, 30.0);
    assert_eq!(cut_screen::ROW_GAP_PX, 3.0);
    assert_eq!(cut_screen::RED_LINE_PX, 2.0);
    assert_eq!(cut_screen::SCROLLBAR_DRAG_PX, 40.0);

    // What you grab is taller than what you read.
    assert!(cut_screen::SELECTION_BAND_PX > cut_screen::RULER_PX);

    // "scrollbar (hidden when everything fits…)" — and exactly at the fit it is hidden.
    assert!(cut_screen::scrollbar_shown(5000.0, 1200.0));
    assert!(!cut_screen::scrollbar_shown(1000.0, 1200.0));
    assert!(!cut_screen::scrollbar_shown(1200.0, 1200.0));
}
