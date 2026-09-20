//! §05-cut#2-model-summary-full-schema-in-01-project-and-filesmd-3 — rows, runs, cells, folds, x↔time.
//!
//! Where a recording is drawn, where footage is continuous, what a fold leaves of the picture band and
//! how a pixel meets a second: spec/inventory/cut.md §B's four rules, held as numbers by
//! [`naivepost::timeline`] so that they can be checked without a window.

use std::collections::BTreeMap;

use naivepost::cut::Cut;
use naivepost::timeline::{self as timeline, Cell, Recording, Span};

fn rec(base: &str, start: f64, end: f64) -> Recording {
    Recording { base: base.to_string(), start, end }
}

#[test]
fn sec_05_cut_2_model_summary_full_schema_in_01_project_and_filesmd_3_s1_rows_are_coloured_greedily_in_start_order()
{
    // §B: "Rows = greedy interval colouring of recordings in start order".
    let cut = Cut::default();
    let one = |base: &str, start: f64, end: f64| vec![rec(base, start, end)];

    let two_apart = vec![rec("cam1", 0.0, 60.0), rec("mic", 90.0, 120.0)];
    assert_eq!(timeline::rows_for(&two_apart, &cut), [0, 0], "footage that never meets shares a row");

    let three = vec![rec("cam1", 0.0, 60.0), rec("cam2", 30.0, 90.0), rec("mic", 90.0, 120.0)];
    assert_eq!(timeline::rows_for(&three, &cut), [0, 1, 0]);
    assert_eq!(timeline::row_count(&three, &cut), 2);

    // The order the files were read in is not the order the rows are decided in: start order is.
    let shuffled = vec![rec("mic", 90.0, 120.0), rec("cam2", 30.0, 90.0), rec("cam1", 0.0, 60.0)];
    let rows = timeline::rows_for(&shuffled, &cut);
    assert_eq!(rows[2], 0, "cam1 still row 0");
    assert_eq!(rows[1], 1, "cam2 still row 1");
    assert_eq!(rows[0], 0, "mic still row 0");

    // `nrows` is a floor under the count and moves nobody (§B: "nRows floor keeps an emptied bottom
    // row until its ✕").
    let floored = Cut { nrows: 3, ..Default::default() };
    assert_eq!(timeline::row_count(&three, &floored), 3);
    assert_eq!(timeline::rows_for(&three, &floored), [0, 1, 0]);

    // Nothing recorded is nothing to colour.
    assert!(timeline::rows_for(&[], &cut).is_empty());
    assert_eq!(timeline::row_count(&one("cam1", 0.0, 1.0), &Cut::default()), 1);
}

#[test]
fn sec_05_cut_2_model_summary_full_schema_in_01_project_and_filesmd_3_s2_a_pin_puts_a_recording_on_the_row_the_file_named()
{
    // §B: "pins (cut.json rows) applied first"; §3: `rows`: which row a recording is on.
    let cut = Cut { rows: BTreeMap::from([("cam2".to_string(), 1)]), ..Default::default() };
    let recordings = vec![rec("cam1", 0.0, 60.0), rec("cam2", 30.0, 90.0)];
    assert_eq!(
        timeline::rows_for(&recordings, &cut),
        [0, 1],
        "cam2 goes where the file said even though row 0 was free when it was asked"
    );

    // The pin costs one row, not the whole colouring: what could have had row 0 after it still does.
    let three = vec![rec("cam1", 0.0, 60.0), rec("cam2", 30.0, 90.0), rec("mic", 70.0, 80.0)];
    assert_eq!(
        timeline::rows_for(&three, &cut),
        [0, 1, 0],
        "row 0 is free again by 70 s: cam1 ends at 60 and the pin costs one row, not the colouring"
    );

    let after = vec![rec("cam1", 0.0, 60.0), rec("cam2", 30.0, 90.0), rec("mic", 95.0, 120.0)];
    assert_eq!(timeline::rows_for(&after, &cut), [0, 1, 0], "row 0 is free again by 95 s");

    // A pin may overlap what already sits on that row: the editor put it there knowing. cam2 is pinned
    // to row 0, where cam1 already runs, and cam1 keeps the row it was given first — a pin moves its
    // own recording, not everything already on the way.
    let pinned_over = Cut { rows: BTreeMap::from([("cam2".to_string(), 0)]), ..Default::default() };
    assert_eq!(timeline::rows_for(&recordings, &pinned_over), [1, 0]);
}

#[test]
fn sec_05_cut_2_model_summary_full_schema_in_01_project_and_filesmd_3_s3_filmed_runs_merge_touching_footage() {
    // §B: "Filmed runs = merged recording spans".
    let runs = |list: &[(&str, f64, f64)]| {
        timeline::filmed_runs(&list.iter().map(|(b, s, e)| rec(b, *s, *e)).collect::<Vec<_>>())
    };

    assert_eq!(runs(&[("a", 0.0, 10.0), ("b", 10.0, 20.0)]), vec![(0.0, 20.0)], "touching is one run");
    assert_eq!(
        runs(&[("a", 0.0, 10.0), ("b", 12.0, 20.0)]),
        vec![(0.0, 10.0), (12.0, 20.0)],
        "a gap is a gap"
    );
    assert_eq!(runs(&[("a", 0.0, 15.0), ("b", 10.0, 20.0)]), vec![(0.0, 20.0)], "overlapping merges");

    // Unsorted input, and a run inside another: still one pass to the merged answer.
    assert_eq!(runs(&[("b", 10.0, 20.0), ("a", 0.0, 30.0)]), vec![(0.0, 30.0)]);
    assert_eq!(
        runs(&[("c", 40.0, 50.0), ("a", 0.0, 10.0), ("b", 8.0, 12.0)]),
        vec![(0.0, 12.0), (40.0, 50.0)]
    );

    // A recording with no length is not footage.
    assert_eq!(runs(&[("a", 5.0, 5.0)]), Vec::<(f64, f64)>::new());
    assert_eq!(timeline::filmed_runs(&[]), Vec::<(f64, f64)>::new());
}

#[test]
fn sec_05_cut_2_model_summary_full_schema_in_01_project_and_filesmd_3_s4_a_fold_is_a_cell_of_zero_width() {
    // §B: "cells = runs cut at folded gaps; spans carry pixel origins (gutter first; a folded cell has
    // zero width)". At 2 px/s, [0,60] with [20,30] folded is 40 s + 30 s of footage = 140 px, plus the
    // 30 px gutter.
    let span = Span::new(timeline::cells(&[(0.0, 60.0)], &[[20.0, 30.0]]), 2.0, 30.0);
    assert_eq!(
        span.cells,
        vec![
            Cell { start: 0.0, end: 20.0, folded: false },
            Cell { start: 20.0, end: 30.0, folded: true },
            Cell { start: 30.0, end: 60.0, folded: false },
        ]
    );
    // The band's width is the gutter plus what it draws, so a fold takes its ten seconds out of it:
    // 30 px of gutter and (20 + 30) s at 2 px/s.
    assert_eq!(span.total_px(), 130.0, "the fold costs the band its ten seconds");

    // No folds: one cell per run, and the width is the footage.
    let plain = Span::new(timeline::cells(&[(0.0, 60.0)], &[]), 2.0, 30.0);
    assert_eq!(plain.cells, vec![Cell { start: 0.0, end: 60.0, folded: false }]);
    assert_eq!(plain.total_px(), 150.0);

    // A fold over a whole run folds the run; two folds leave three stretches and two badges.
    let whole = Span::new(timeline::cells(&[(0.0, 60.0)], &[[0.0, 60.0]]), 2.0, 30.0);
    assert!(whole.cells.iter().all(|cell| cell.folded), "the run is out of the way");
    assert_eq!(whole.total_px(), 30.0, "only the gutter is left to draw from");

    let two = timeline::cells(&[(0.0, 100.0)], &[[20.0, 30.0], [60.0, 70.0]]);
    assert_eq!(two.iter().filter(|cell| cell.folded).count(), 2);
    assert_eq!(two.iter().filter(|cell| !cell.folded).map(|c| c.end - c.start).sum::<f64>(), 80.0);

    // A fold across the end of a run cuts it, not past it.
    let edge = timeline::cells(&[(0.0, 30.0)], &[[20.0, 90.0]]);
    assert_eq!(edge.last(), Some(&Cell { start: 20.0, end: 30.0, folded: true }));
    assert_eq!(edge.first(), Some(&Cell { start: 0.0, end: 20.0, folded: false }));
}

#[test]
fn sec_05_cut_2_model_summary_full_schema_in_01_project_and_filesmd_3_s5_x_and_time_meet_at_the_seam_from_the_later_take()
{
    // §B: "x↔time: xOf walks spans …; tAt is the inverse, half-open on the right (the seam second
    // belongs to the later take); left of the first run → its start."
    let span = Span::new(timeline::cells(&[(0.0, 10.0), (20.0, 40.0)], &[]), 4.0, 30.0);

    for t in [0.0, 2.5, 9.75, 20.0, 33.3, 39.99] {
        let back = span.t_at(span.x_of(t));
        assert!((back - t).abs() < 1e-9, "{t} went to {} and came back as {back}", span.x_of(t));
    }

    // The x of a second is measured from the gutter, and one run's end is the next run's x.
    assert_eq!(span.x_of(0.0), 30.0);
    let seam = span.x_of(10.0);
    assert_eq!(seam, 70.0, "ten seconds at 4 px/s after the gutter");
    // Half-open on the right (§B): x=70 is where the later take starts, so it reads as that take's
    // first second — which is what makes a press exactly on a seam land in the footage you can see.
    assert_eq!(span.t_at(70.0), 20.0, "the seam second belongs to the later take");
    assert_eq!(span.t_at(69.0), 9.75, "before it is still the earlier take");
    assert_eq!(span.x_of(10.0), span.x_of(20.0), "one pixel, two takes, and the later one answers");

    // Left of everything is the first run's start; past the end is the band's width. The gap between
    // the runs is not footage, so it is not drawn and not paid for: 10 s + 20 s at 4 px/s after the
    // gutter (§B draws cells, and a cell is only ever a piece of a run).
    assert_eq!(span.t_at(0.0), 0.0);
    let end = span.total_px();
    assert_eq!(end, 30.0 + 30.0 * 4.0, "only filmed seconds take width");
    assert_eq!(span.x_of(9999.0), end);

    // A folded stretch: its seam is where it starts, and a press on that x lands in the footage after
    // the fold — the take the person can actually see.
    let folded = Span::new(timeline::cells(&[(0.0, 60.0)], &[[20.0, 30.0]]), 2.0, 30.0);
    assert_eq!(folded.x_of(25.0), 70.0, "every second of the fold shares its seam");
    assert_eq!(folded.x_of(20.0), 70.0);
    assert_eq!(folded.t_at(70.0), 30.0, "the fold is passed over, not answered");
    assert_eq!(folded.t_at(69.0), 19.5);

    // The floor fits the filmed length: 40 s of footage across 800 px is 20 px/s (§2).
    let runs = timeline::filmed_runs(&[rec("a", 0.0, 10.0), rec("b", 20.0, 50.0)]);
    assert_eq!(timeline::floor_pps(&runs, 800.0), 20.0);
}
