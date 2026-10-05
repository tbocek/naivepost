//! F2.1 S3/S4 — the recording preview's own tick, driven as data rather than through a window.
//!
//! `preview` holds the rules (`play_advance`, `walk_on`, `walk_fold`); [`naivepost::ui::cut_record_tick::step`]
//! sequences them per tick over a page. A table page stands in for the window so the sequence is checkable
//! without GTK, exactly as `cut_play_leg`'s tests drive their tick the same way.

use std::cell::RefCell;

use naivepost::cut::{Cut, Seg};
use naivepost::preview;
use naivepost::ui::cut_record_tick::{self, RecordingPreviewPage, TablePage};

/// One tick at §D's rate: 100 ms.
const TICK: f64 = preview::TICK_MS as f64 / 1000.0;

fn tape() -> Vec<(f64, f64)> {
    vec![(0.0, 120.0)]
}

#[test]
fn f2_1_s3_the_line_walks_straight_through_a_stretch_the_cut_removed() {
    // The cut drops 30..60; under ▶ that stretch still plays, so the line crosses it without a jump.
    let page = TablePage {
        playing: true,
        head: RefCell::new(Some(29.95)),
        cut: RefCell::new(Cut {
            segs: vec![
                Seg { s: 0.0, e: 30.0, ..Default::default() },
                Seg { s: 60.0, e: 90.0, ..Default::default() },
            ],
            ..Default::default()
        }),
        said: RefCell::new(Vec::new()),
    };
    cut_record_tick::step(&page, &tape(), TICK);
    assert_eq!(
        page.playhead(),
        Some(30.05),
        "the line crossed the removed stretch's start and kept walking"
    );
    for _ in 0..300 {
        cut_record_tick::step(&page, &tape(), TICK);
    }
    let head = page.playhead().expect("a line after 30 s of ticks");
    assert!(head > 60.0, "and walked out the far side of the hole: {head}");
    assert!(head < 61.0, "one tick at a time, never skipping ahead: {head}");
}

#[test]
fn f2_1_s3_at_a_recordings_end_the_line_walks_on_to_the_next_one() {
    // Two takes with a gap between: at the first one's end the line goes to the next one's start.
    let runs = vec![(0.0, 10.0), (20.0, 30.0)];
    let page = TablePage {
        playing: true,
        head: RefCell::new(Some(9.95)),
        cut: RefCell::new(Cut::default()),
        said: RefCell::new(Vec::new()),
    };
    cut_record_tick::step(&page, &runs, TICK);
    assert_eq!(page.playhead(), Some(20.0), "walked on to the next recording's start");

    // A camera still rolling at that second keeps the line where it is — nothing to walk on to.
    let overlapping = vec![(0.0, 10.0), (5.0, 30.0)];
    let rolling = TablePage {
        playing: true,
        head: RefCell::new(Some(9.95)),
        cut: RefCell::new(Cut::default()),
        said: RefCell::new(Vec::new()),
    };
    cut_record_tick::step(&rolling, &overlapping, TICK);
    assert!(
        page_at(&rolling) < 10.1,
        "the other camera covers this second, so the line keeps walking inside the footage instead of \
         jumping to a start: {:?}",
        rolling.playhead()
    );
}

/// A table page's line, unwrapped for the comparisons that only need its value.
fn page_at(page: &TablePage) -> f64 {
    page.playhead().expect("a line after one tick")
}

#[test]
fn f2_1_s3_a_fold_the_line_reaches_opens_and_says_unfolded() {
    let page = TablePage {
        playing: true,
        head: RefCell::new(Some(39.95)),
        cut: RefCell::new(Cut { folds: vec![[40.0, 50.0]], ..Default::default() }),
        said: RefCell::new(Vec::new()),
    };
    let said = cut_record_tick::step(&page, &tape(), TICK).expect("the fold the line ran into answers");
    assert_eq!(said, preview::unfolded_status(40.0), "§B's sentence, verbatim");
    assert!(
        !page.cut().folds.iter().any(|fold| fold[0] == 40.0),
        "the seam is open now: {:?}",
        page.cut().folds
    );
}

#[test]
fn f2_1_s4_the_tick_moves_the_line_ten_times_a_second() {
    assert_eq!(preview::follows_per_second(), 10, "S4: ten times a second");
    let page = TablePage {
        playing: true,
        head: RefCell::new(Some(0.0)),
        cut: RefCell::new(Cut::default()),
        said: RefCell::new(Vec::new()),
    };
    cut_record_tick::step(&page, &tape(), TICK);
    assert!(
        (page.playhead().unwrap() - 0.1).abs() < 1e-9,
        "one tick moves the line exactly one tenth of a second: {:?}",
        page.playhead()
    );
    // The face is session time to a tenth (§D), and empty until somebody has a line at all.
    assert_eq!(preview::clock(Some(65.34)), "01:05.3", "mm:ss.d of session time");
    assert_eq!(preview::clock(None), "--:--.-", "a page nobody clicked has no time");
    // Between ticks the face smooths forward but never more than one tick, so it cannot jump back.
    assert!(preview::live_clock(10.0, 40, 1.0) > 10.0, "smoothed ahead between ticks");
    assert!((preview::live_clock(10.0, 900, 1.0) - 10.1).abs() < 1e-9, "capped at one tick");
}
