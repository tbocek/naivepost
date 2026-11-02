//! §05-cut#1-screen — the form column's idle readings and the timeline's bands.
//!
//! The eight rows the page shows when nothing is being placed, the arithmetic behind the two lengths
//! it reads out, whether the picture band has a frame to draw at any zoom, and the heights the bands
//! are laid out with — all of it answered in [`naivepost::cut_screen`], so none of it needs a window.

use naivepost::cut::{Cut, Seg};
use naivepost::cut_screen;
use naivepost::project::Project;
use naivepost::shell::Page;
use naivepost::shell::{self, Shell};

/// A kept stretch of footage at some playback speed. `rate` 0 is what a hand-made scene carries:
/// only produceSegs writes one (spec/inventory/cut.md §B).
fn footage(start: f64, end: f64, rate: f64) -> Seg {
    Seg {
        s: start,
        e: end,
        rate,
        ..Default::default()
    }
}

#[test]
fn sec_05_cut_1_screen_s6_the_idle_form_reads_the_session_seven_ways() {
    // §A: "Idle rows: Aspect ratio …, Playhead …, Selection [marks], Cut [total], Cut at 1× [totalRaw],
    // Source [totalSrc], Segments [totalSegs]" — seven rows, this order (the thumbnail size is a grip on the
    // tracks, not a reading).
    let rows = cut_screen::idle_readouts("", 0.0, None, &Cut::default(), 0.0);
    let labels: Vec<&str> = rows.iter().map(|row| row.label).collect();
    assert_eq!(
        labels,
        [
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
    assert_eq!(
        value("Aspect ratio"),
        cut_screen::ASPECT_DEFAULT,
        "an unset aspect still has a shape"
    );
    assert_eq!(value("Selection"), "", "a reading, not a prompt");
}

#[test]
fn sec_05_cut_1_screen_s7_cut_and_cut_at_1x_disagree_by_the_speed_effects() {
    // Cut [total] runs at the cut's own speeds; Cut at 1× [totalRaw] is the same stretches plain.
    let cut = Cut {
        segs: vec![
            footage(0.0, 10.0, 2.0),
            Seg {
                ins: "card.svg".into(),
                dur: 3.0,
                ..Default::default()
            },
        ],
        ..Default::default()
    };
    assert_eq!(
        cut_screen::cut_seconds(&cut),
        8.0,
        "five seconds at 2\u{00d7}, plus the card's three"
    );
    assert_eq!(cut_screen::cut_seconds_at_1x(&cut), 13.0);
    assert_eq!(cut_screen::segment_count(&cut), 2);

    let rows = cut_screen::idle_readouts("9:16", 75.0, Some((12.0, 20.0)), &cut, 3600.0);
    let value = |label: &str| {
        rows.iter()
            .find(|row| row.label == label)
            .unwrap()
            .value
            .clone()
    };
    assert_eq!(value("Cut"), "00:08");
    assert_eq!(value("Cut at 1\u{00d7}"), "00:13");
    assert_eq!(
        value("Aspect ratio"),
        "9:16",
        "what the cut stores is what the row reads"
    );
    assert_eq!(
        value("Playhead"),
        "01:15.0",
        "the preview's clock face, mm:ss.d"
    );
    assert_eq!(
        value("Source"),
        "60:00",
        "mm_ss lets the minutes run past an hour"
    );
    assert_eq!(value("Selection"), "00:12 \u{2013} 00:20");
}

#[test]
fn sec_05_cut_1_screen_s11_the_cut_readout_says_there_is_nothing_to_cut_without_footage() {
    // `spec/inventory/cut.md` §A: the bottom bar reads "nothing to cut — no source on Inputs is marked
    // as footage" when no source carries the footage mark. A listened-to-only recording is not a cut.
    let shell = Shell {
        page: Page::Cut,
        ..Default::default()
    };
    let narration = naivepost::narration::Narration::default();
    let mut project = Project::default();
    project.sources = vec![naivepost::project::Source {
        path: "/tmp/only-a-recording.mkv".into(),
        footage: false,
        ..Default::default()
    }];
    assert_eq!(
        shell.inputs(None, &project, &Cut::default(), &narration),
        shell::NOTHING_TO_CUT,
        "the row says what is missing instead of counting nothing"
    );

    // Mark it footage and the row goes back to counting clips.
    project.sources[0].footage = true;
    let said = shell.inputs(None, &project, &Cut::default(), &narration);
    assert_ne!(
        said,
        shell::NOTHING_TO_CUT,
        "there IS footage now: {said:?}"
    );
    assert_eq!(
        said, "0 clip(s)",
        "an uncut session still counts its zero clips"
    );
    let two = Cut {
        segs: vec![footage(0.0, 10.0, 1.0), footage(20.0, 30.0, 1.0)],
        ..Default::default()
    };
    assert_eq!(shell.inputs(None, &project, &two, &narration), "2 clip(s)");
}

#[test]
fn sec_05_cut_1_screen_s10_the_bands_and_the_red_line_are_the_heights_the_page_draws_with() {
    // §A's Tracks paragraph and §1's "Red playhead: 2 px line on its own layer across both bands".
    assert_eq!(cut_screen::GUTTER_PX, 30.0);
    assert_eq!(cut_screen::RULER_PX, 18.0);
    assert_eq!(cut_screen::SELECTION_BAND_PX, 22.0);
    assert_eq!(
        cut_screen::EFFECT_ROW_PX,
        26.0,
        "one row per overlapping group"
    );
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
