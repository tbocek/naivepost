//! F2.10 (Cameras and hearing) — the ORDER clause of the row colouring, which `spec/05-cut.md` spells out:
//! "a recording goes on the lowest-numbered row free for its whole span (row 1 at the top, each further row
//! below the one before)". Two halves to that sentence and nothing pinned either before this file:
//!
//! * **lowest-numbered free** — a row is free for a span only when NOTHING already placed on it overlaps that
//!   span, so a later recording may drop back onto an earlier row once the earlier one has stopped; and
//! * **row 1 at the top** — the index the colouring answers with IS the top-to-bottom order, index 0 being
//!   the first row a status calls "row 1" (`cut_cam::row_name`).
//!
//! The arithmetic is `timeline::rows_for`; the naming is `cut_cam::row_name`. No widget, no display.

use naivepost::cut::Cut;
use naivepost::cut_cam::row_name;
use naivepost::timeline::{row_count, rows_for, Recording};

fn tape(base: &str, start: f64, end: f64) -> Recording {
    Recording {
        base: base.to_string(),
        start,
        end,
    }
}

/// F2.10 S1: "free for its WHOLE span". A 0-10, B 5-15 shares A's row's seconds, so B takes the next row; C
/// 12-30 overlaps B (12-15) but NOT A, so the lowest row free for all of 12-30 is A's — the colouring reuses
/// a row as soon as nothing ON it overlaps, rather than always stacking downwards.
#[test]
fn f2_10_s1_a_row_is_only_free_for_its_whole_span() {
    let cut = Cut::default();
    let recordings = vec![
        tape("cam-a", 0.0, 10.0),
        tape("cam-b", 5.0, 15.0),
        tape("cam-c", 12.0, 30.0),
    ];
    let rows = rows_for(&recordings, &cut);
    println!("SPAN a[0,10) b[5,15) c[12,30) -> rows {rows:?}");
    assert_eq!(
        rows,
        vec![0, 1, 0],
        "c overlaps b but not a, so the lowest row free for ALL of 12-30 is a's row 0"
    );
    assert_eq!(
        row_count(&recordings, &cut),
        2,
        "two rows were needed, and the page draws exactly those two"
    );
}

/// F2.10 S1: spans that merely TOUCH are not an overlap — the timeline reads a span half-open (`start < to &&
/// end > from`), so one camera's take ending at 10 and another starting at 10 share a row, and moving the
/// first by a hair pushes the second down. This is what keeps one camera's files on one row (§B).
#[test]
fn f2_10_s1_touching_spans_share_a_row() {
    let cut = Cut::default();
    let touching = vec![tape("cam-a", 0.0, 10.0), tape("cam-b", 10.0, 20.0)];
    let rows = rows_for(&touching, &cut);
    println!("TOUCH a[0,10) b[10,20) -> rows {rows:?}");
    assert_eq!(
        rows,
        vec![0, 0],
        "they touch at 10 and do not overlap: one camera's files share a row"
    );

    let overlapping = vec![tape("cam-a", 0.0, 10.0001), tape("cam-b", 10.0, 20.0)];
    let shifted = rows_for(&overlapping, &cut);
    println!("OVERLAP a[0,10.0001) b[10,20) -> rows {shifted:?}");
    assert_eq!(
        shifted,
        vec![0, 1],
        "one tenth of a millisecond of overlap is still both cameras filming at once"
    );
}

/// F2.10 S1: three cameras filmed together cost three rows — each new recording finds every existing row
/// busy and opens the next one — and a fourth that overlaps none drops back to the lowest free row instead of
/// widening the band.
#[test]
fn f2_10_s1_the_third_overlap_costs_one_more_row() {
    let cut = Cut::default();
    let three = vec![
        tape("cam-a", 0.0, 30.0),
        tape("cam-b", 5.0, 25.0),
        tape("cam-c", 10.0, 20.0),
    ];
    let rows = rows_for(&three, &cut);
    println!("THREE-WAY -> rows {rows:?}");
    assert_eq!(
        rows,
        vec![0, 1, 2],
        "each of the three overlaps both others, so each needs its own row"
    );
    assert_eq!(row_count(&three, &cut), 3, "the band is three rows deep");

    let mut four = three.clone();
    four.push(tape("cam-d", 40.0, 50.0));
    let after = rows_for(&four, &cut);
    println!("FOURTH (40-50, overlaps nothing) -> rows {after:?}");
    assert_eq!(
        after,
        vec![0, 1, 2, 0],
        "the fourth goes back to the LOWEST-numbered free row, not below the deepest one"
    );
    assert_eq!(
        row_count(&four, &cut),
        3,
        "and the band stays three rows deep"
    );
}

/// F2.10 S1 ("row 1 at the top, each further row below the one before"): the index the colouring answers with
/// is the drawing order, and the status line names rows from one, so the recording on index 0 is the one a
/// status calls "row 1".
#[test]
fn f2_10_s1_row_one_is_the_top_row() {
    let cut = Cut::default();
    let recordings = vec![
        tape("cam-a", 0.0, 30.0),
        tape("cam-b", 5.0, 25.0),
        tape("cam-c", 10.0, 20.0),
    ];
    let rows = rows_for(&recordings, &cut);
    let named: Vec<String> = (0..row_count(&recordings, &cut))
        .map(|index| format!("{index}:{}", row_name(index as i32, &recordings, &rows)))
        .collect();
    println!("NAMES top-down -> {}", named.join(" | "));
    // Index 0 holds the FIRST camera (the earliest start wins the top row) and is called row 1.
    assert_eq!(
        rows[0], 0,
        "the earliest recording sits on index 0, the top row"
    );
    assert_eq!(
        named,
        vec!["0:cam-a", "1:cam-b", "2:cam-c"],
        "the numbering runs 1..=n top-down and names each row by the recording lying on it"
    );
    // An empty row still gets its number, counting from one: "the cut shows  here" is not a sentence.
    assert_eq!(row_name(0, &[], &[]), "row 1");
    assert_eq!(row_name(2, &[], &[]), "row 3");
}
