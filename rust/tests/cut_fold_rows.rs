// F2.11 (Folds and rows) — spec/05-cut.md F2.11, with spec/inventory/cut.md §B normative for what a fold is.
// One test per branch of the two steps: which stretches may be folded, what a press does to one, where its badge
// sits (S1), and the two ✕ that take rows away (S2). Every rule is a plain function in naivepost::cut_fold or
// naivepost::timeline, so nothing here needs a widget or a display.

use naivepost::cut::{Cut, Lane, Seg, Snapshot};
use naivepost::cut_fold as fold;
use naivepost::cut_screen::GUTTER_PX;
use naivepost::timeline::{self, filmed_runs, Recording, Span};

/// The one camera of the fixture: 0–120 filmed, so `filmed_runs` answers a single run.
const CAM: &str = "17-25-06";

/// Pixels per second for the badge tests — §1's reading of `P.eng.pixelsPerSecond`, 4 px/s at 1×, which is what
/// draws the fixture's 10 s hole 40 px wide (a badge fits) and a 0.5 s gap 2 px wide (it does not). Seconds are what
/// the model stores, so most assertions below ask in seconds; PPS appears only where a rule is drawn in pixels.
const PPS: f64 = 4.0;

fn recording(base: &str, start: f64, end: f64) -> Recording {
    Recording { base: base.to_string(), start, end }
}

/// One camera that filmed 0–120.
fn one() -> Vec<Recording> {
    vec![recording(CAM, 0.0, 120.0)]
}

/// Kept footage from `s` to `e`.
fn clip(s: f64, e: f64) -> Seg {
    Seg { s, e, ..Default::default() }
}

/// A card at `at`: it owns no footage of its own.
fn card(at: f64) -> Seg {
    Seg { s: at, e: at, ins: "assets/sting.mp4".into(), dur: 3.0, ..Default::default() }
}

/// The fixture's cut: two kept clips out of 0–120, which drops a head (0–10), a hole (30–40) and a tail (60–120).
fn dropped() -> Cut {
    let mut cut = Cut::default();
    cut.segs.push(clip(10.0, 30.0));
    cut.segs.push(clip(40.0, 60.0));
    cut
}

/// The laid-out band a badge is placed on: the runs, the cut's own folds, and the gutter every span starts after.
fn band(cut: &Cut, recordings: &[Recording]) -> Span {
    let runs = filmed_runs(recordings);
    Span::new(timeline::cells(&runs, &cut.folds), PPS, GUTTER_PX)
}

/// The badge over `gap`, or none — most badge assertions are about one gap among three.
fn badge_at(badges: &[fold::Badge], gap: (f64, f64)) -> Option<fold::Badge> {
    badges.iter().copied().find(|badge| badge.gap == gap)
}

fn is_close(got: f64, want: f64) -> bool {
    (got - want).abs() < 1e-9
}

// --- S1: which stretches a fold may cover -------------------------------------------------------------------

/// F2.11 (Folds and rows) S1 — `Folds cover stretches the cut drops (holes between kept clips; each filmed run's
/// head and tail)`: exactly those three, in time order, and nothing else.
#[test]
fn f2_11_s1_a_fold_covers_what_the_cut_drops() {
    let runs = filmed_runs(&one());
    assert_eq!(runs, vec![(0.0, 120.0)], "one recording is one run");

    let cut = dropped();
    assert_eq!(fold::dropped_gaps(&runs, &cut.segs), vec![(0.0, 10.0), (30.0, 40.0), (60.0, 120.0)]);
}

/// F2.11 (Folds and rows) S1 — `not unfilmed time, which is never laid out`: a fold is offered over seconds the
/// page draws, so nothing beyond a recording's end is a gap, and a project with nothing filmed has no gaps at all.
#[test]
fn f2_11_s1_unfilmed_time_is_never_a_gap() {
    let runs = filmed_runs(&one());
    let mut cut = Cut::default();
    // One clip at the very start: everything after it is dropped, and that is one tail gap — not a gap per second.
    cut.segs.push(clip(0.0, 10.0));
    let gaps = fold::dropped_gaps(&runs, &cut.segs);
    assert_eq!(gaps, vec![(10.0, 120.0)]);
    assert!(gaps.iter().all(|(_, end)| *end <= 120.0), "nothing past the last filmed second: {gaps:?}");
    assert!(gaps.iter().all(|(start, _)| *start >= 0.0), "and nothing before the first");

    // Nothing filmed: the page has no tape to fold.
    let empty = filmed_runs(&[]);
    assert_eq!(fold::dropped_gaps(&empty, &cut.segs), Vec::<(f64, f64)>::new());
    assert_eq!(fold::dropped_gaps(&[], &[]), Vec::<(f64, f64)>::new());
}

/// F2.11 (Folds and rows) S1 — an insert covers no footage: a card laid over the tape gave up nothing, so the
/// seconds under it were never dropped and have no seam to fold to.
#[test]
fn f2_11_s1_a_card_drops_no_footage() {
    let runs = filmed_runs(&one());
    let mut cut = Cut::default();
    cut.segs.push(card(50.0));

    assert_eq!(fold::dropped_gaps(&runs, &cut.segs), vec![(0.0, 120.0)], "the whole run is still there to fold");

    // A card between two kept clips leaves one hole, not a gap on each side of the card.
    let mut both = Cut::default();
    both.segs.push(clip(0.0, 30.0));
    both.segs.push(card(50.0));
    both.segs.push(clip(70.0, 120.0));
    assert_eq!(fold::dropped_gaps(&runs, &both.segs), vec![(30.0, 70.0)]);
}

// --- S1: one gap, one press --------------------------------------------------------------------------------

/// F2.11 (Folds and rows) S1 — `− in such a gap folds it to a seam`: the fold is stored (it is what `cut.json`
/// writes), the page says which stretch went away, and the laid-out band gives that stretch no width: every second
/// of the hole answers the one x where the fold begins.
#[test]
fn f2_11_s1_the_first_badge_folds_a_gap_to_a_seam() {
    let recordings = one();
    let runs = filmed_runs(&recordings);
    let mut cut = dropped();

    let said = fold::toggle_fold(&mut cut, (30.0, 40.0));
    assert_eq!(said, "folded 0:30 \u{2013} 0:40 (10.0 s)", "the page's own reading: unpadded minutes, cut_hear::scene_clock");
    assert_eq!(cut.folds, vec![[30.0, 40.0]], "and it is stored for the next open");
    assert!(fold::is_folded(&cut.folds, (30.0, 40.0)));

    // The seam: a folded cell has zero width, so two seconds inside the fold share one x.
    let cells = timeline::cells(&runs, &cut.folds);
    assert!(cells.iter().any(|cell| cell.folded && is_close(cell.start, 30.0) && is_close(cell.end, 40.0)), "{cells:?}");
    let span = band(&cut, &recordings);
    assert!(is_close(span.x_of(32.0), span.x_of(38.0)), "both seconds are at the seam");
    assert!(span.x_of(32.0) > span.x_of(29.0), "which is where the footage before it ended");
}

/// F2.11 (Folds and rows) S1 — `+ on the seam unfolds`: the same press opens what it closed, says so in the same
/// sentence's other half, and leaves no fold behind for a later open to find.
#[test]
fn f2_11_s1_a_second_press_opens_it_again() {
    let recordings = one();
    let runs = filmed_runs(&recordings);
    let mut cut = dropped();
    fold::toggle_fold(&mut cut, (30.0, 40.0));

    assert_eq!(fold::toggle_fold(&mut cut, (30.0, 40.0)), "unfolded 0:30 \u{2013} 0:40 (10.0 s)");
    assert!(cut.folds.is_empty(), "nothing left folded");
    assert!(!fold::is_folded(&cut.folds, (30.0, 40.0)));
    assert!(timeline::cells(&runs, &cut.folds).iter().all(|cell| !cell.folded), "the band lays the gap out again");
}

/// F2.11 (Folds and rows) S1 — a fold is matched to its gap BY OVERLAP (`spec/inventory/cut.md` §B): trimming the
/// clip beside a folded hole moves the hole's edge, and a fold remembered by its ends would be lost to the trim.
#[test]
fn f2_11_s1_a_fold_survives_the_gap_moving() {
    let mut cut = dropped();
    fold::toggle_fold(&mut cut, (30.0, 40.0));

    // The later clip trimmed back to start at 36: the hole is now (30, 36), not the folded (30, 40).
    let mut trimmed = dropped();
    trimmed.segs[1] = clip(36.0, 60.0);
    trimmed.folds = cut.folds.clone();
    let runs = filmed_runs(&one());
    assert_eq!(fold::dropped_gaps(&runs, &trimmed.segs), vec![(0.0, 10.0), (30.0, 36.0), (60.0, 120.0)]);
    assert!(fold::is_folded(&trimmed.folds, (30.0, 36.0)), "the fold still answers for that hole");

    // And a fold does NOT reach a gap it merely touches at one end: 40 is where this fold ends and 45 is another
    // gap's own business.
    assert!(!fold::is_folded(&[[30.0, 40.0]], (40.0, 50.0)), "touching at a border is not overlapping");
}

/// F2.11 (Folds and rows) S1 — `A fold is a view, never an edit … but it IS saved`: `folds` survives the file, and
/// an undo step does not touch it, because where someone was working is not something ↶ takes back.
#[test]
fn f2_11_s1_a_fold_is_saved_and_never_an_edit() {
    let mut cut = dropped();
    fold::toggle_fold(&mut cut, (30.0, 40.0));

    // Saved: the same shape `cut.json` holds, read back whole.
    let json = serde_json::to_string(&cut).expect("a cut always writes");
    assert!(json.contains("\"folds\""), "the key is written: {json}");
    let back: Cut = serde_json::from_str(&json).expect("and read back");
    assert_eq!(back.folds, vec![[30.0, 40.0]]);

    // Not an edit: a snapshot taken before the fold and restored after it leaves the fold exactly where it was.
    let mut cut = dropped();
    let before = Snapshot::of(&cut);
    fold::toggle_fold(&mut cut, (30.0, 40.0));
    before.restore(&mut cut);
    assert_eq!(cut.folds, vec![[30.0, 40.0]], "↶ did not unfold it");

    // And the press changed nothing an edit would move, so a drag has nothing new to measure against: folds are
    // never used to measure one.
    let plain = dropped();
    let mut moved = dropped();
    fold::toggle_fold(&mut moved, (30.0, 40.0));
    assert_eq!(moved.segs, plain.segs);
    assert_eq!(moved.fx, plain.fx);
    assert_eq!(moved.rows, plain.rows);
    assert_eq!(moved.shift, plain.shift);
    assert_eq!(moved.nrows, plain.nrows);
}

// --- S1: the gutter badge ----------------------------------------------------------------------------------

/// F2.11 (Folds and rows) S1 — `never over a whole recording`: a gap that IS a filmed run is not a fold worth
/// saving; folding it would hide a recording that has its own ways off the page.
#[test]
fn f2_11_s1_a_whole_recording_is_not_a_fold() {
    let recordings = one();
    let runs = filmed_runs(&recordings);
    // Nothing kept out of the recording: its one gap IS the recording.
    let nobody_cut = Cut::default();
    assert!(fold::whole_run((0.0, 120.0), &runs, &nobody_cut.segs), "the gap is the recording");

    // Covering a run is not enough: a cut's own head and tail cover their run by definition, and refusing to fold
    // those would leave every cut with nothing foldable around its first clip.
    let cut = dropped();
    assert!(!fold::whole_run((0.0, 10.0), &runs, &cut.segs), "a head beside kept footage is a hole");
    assert!(!fold::whole_run((60.0, 120.0), &runs, &cut.segs), "and so is a tail");

    // So the gutter badge folds nothing and counts zero rather than claiming a seam.
    let mut whole = Cut::default();
    let said = fold::fold_all(&mut whole, &[(0.0, 120.0)], &runs, &[]);
    assert_eq!(said, "folded 0 seams");
    assert!(whole.folds.is_empty());
}

/// F2.11 (Folds and rows) S1 — `the gutter badge folds/unfolds all`: one press for every gap the cut drops, the
/// count read back from what was actually stored, the same press clearing them, and the singular said properly.
/// Nothing is cut here, so no floor applies — not even `P.policy.minSceneSeconds`, which bounds a scene kept on a row
/// rather than what a fold may hide: the whole tape goes and comes back.
#[test]
fn f2_11_s1_the_gutter_badge_folds_and_unfolds_everything() {
    let recordings = one();
    let runs = filmed_runs(&recordings);
    let cut = dropped();
    // The fixture's three gaps are all holes inside the run — none is a whole recording — so all three fold.
    let gaps = fold::dropped_gaps(&runs, &cut.segs);
    assert_eq!(gaps.len(), 3);

    let segs = dropped().segs;
    let mut cut = dropped();
    assert_eq!(fold::fold_all(&mut cut, &gaps, &runs, &segs), "folded 3 seams");
    assert_eq!(cut.folds.len(), 3);

    // Press again: the switch read as ON, so this one opens everything.
    assert_eq!(fold::fold_all(&mut cut, &gaps, &runs, &segs), "unfolded 3 seams");
    assert!(cut.folds.is_empty());

    // One gap is one seam, not one seams; and a whole recording among them is not counted.
    let one_segs = dropped().segs;
    let mut one_gap = dropped();
    assert_eq!(fold::fold_all(&mut one_gap, &[(30.0, 40.0)], &runs, &one_segs), "folded 1 seam");

    // Two cameras: one was cut into, the other nobody touched. The gutter badge folds the holes around the kept
    // clips and leaves the untouched recording alone — that one has its own ways off the page.
    let two = vec![recording(CAM, 0.0, 120.0), recording("17-25-45", 200.0, 320.0)];
    let runs_two = filmed_runs(&two);
    let mut mixed = dropped();
    let gaps = fold::dropped_gaps(&runs_two, &mixed.segs);
    assert_eq!(gaps.len(), 4, "three holes in the cut camera and the whole of the other");
    let mixed_segs = mixed.segs.clone();
    let said = fold::fold_all(&mut mixed, &gaps, &runs_two, &mixed_segs);
    assert_eq!(said, "folded 3 seams", "the untouched recording is skipped");
}

// --- S1: where the badges go -------------------------------------------------------------------------------

/// F2.11 (Folds and rows) S1 — `Gaps under 20 px get no badge` (`// foldMin 20 (spec/inventory/cut.md §D) — §10
/// has no `P.*` row behind it — at 4 px/s it asks whether the − can be drawn, not how much footage may go — so the
/// constant lives in cut_fold::FOLD_MIN_PX and this test converts it to seconds. A folded gap is exempt: its + sits
/// on a seam that has no width to run out of.
#[test]
fn f2_11_s1_a_gap_with_no_room_gets_no_badge() {
    let runs = filmed_runs(&one());
    let mut cut = Cut::default();
    // 0.5 s at 4 px/s is 2 px — no room for a −; 6 s is 24 px, just over the floor.
    cut.segs.push(clip(0.5, 30.0));
    let span = band(&cut, &one());
    assert!(is_close(span.x_of(6.5) - span.x_of(0.5), PPS * 6.0));

    let tight = fold::badges(&cut.folds, &runs, &cut.segs, &span);
    assert_eq!(badge_at(&tight, (0.0, 0.5)), None, "a 2 px gap is not offered a badge");
    assert!(badge_at(&tight, (30.0, 120.0)).is_some(), "the roomy tail still is");

    // Folded, the same narrow stretch gets its + back — and it sits on the seam.
    let mut folded = cut.clone();
    folded.folds.push([0.0, 0.5]);
    let span_folded = band(&folded, &one());
    let with_fold = fold::badges(&folded.folds, &runs, &folded.segs, &span_folded);
    let badge = badge_at(&with_fold, (0.0, 0.5)).expect("a folded gap always shows its +");
    assert!(badge.folded);
    assert!(is_close(badge.x, span_folded.x_of(0.5)), "on the seam, where the footage after it starts");

    // And exactly at the floor there is room: `<` and not `<=`.
    let mut edge = Cut::default();
    edge.segs.push(clip(fold::FOLD_MIN_PX / PPS, 30.0));
    let edge_span = band(&edge, &one());
    assert!(badge_at(&fold::badges(&edge.folds, &runs, &edge.segs, &edge_span), (0.0, fold::FOLD_MIN_PX / PPS)).is_some());
}

/// F2.11 (Folds and rows) S1 — `badges sit just inside the neighbouring bars, not mid-gap`: the gaps that open and
/// close the tape have their far end at the edge of the page, so their badge goes into the bar beside it; an
/// interior hole keeps its own middle. The reach is `cut_fold::BADGE_INSIDE_PX` of pixels converted to that clip's
/// seconds before drawing, and the plate is `cut_fold::BADGE_PLATE_PX` wide, so nothing lands within a plate's width
/// of either end — which also keeps a badge off the grip at the far end of a short clip.
#[test]
fn f2_11_s1_a_badge_sits_inside_the_neighbouring_bar() {
    let recordings = one();
    let runs = filmed_runs(&recordings);
    let cut = dropped();
    let span = band(&cut, &recordings);
    let badges = fold::badges(&cut.folds, &runs, &cut.segs, &span);
    assert_eq!(badges.len(), 3);

    // The head gap (0–10): its badge is inside the clip that starts at 10.
    let head = badge_at(&badges, (0.0, 10.0)).expect("the tape's own head");
    let bar_start = span.x_of(10.0);
    assert!(head.x > bar_start, "inside the bar, not on its grip: {}", head.x);
    assert!(is_close(head.x, span.x_of(10.0 + fold::BADGE_INSIDE_PX / PPS)), "a reach into a 20 s clip");

    // The tail gap (60–120): inside the clip that ends at 60.
    let tail = badge_at(&badges, (60.0, 120.0)).expect("the tape's own tail");
    let bar_end = span.x_of(60.0);
    assert!(tail.x < bar_end, "inside the clip before it: {}", tail.x);
    assert!(is_close(tail.x, span.x_of(60.0 - fold::BADGE_INSIDE_PX / PPS)), "a reach in from its end");

    // The interior hole keeps the middle of the SECONDS between the two clips — the one part of a gap a press can
    // only mean one thing on. Asked in seconds and then drawn, so a fold elsewhere in the run moves pixels and not
    // this badge: averaged from x it would sit wherever the neighbours happened to be laid out.
    let hole = badge_at(&badges, (30.0, 40.0)).expect("an interior hole");
    assert!(is_close(hole.x, span.x_of(35.0)), "mid-gap: {}", hole.x);

    for badge in &badges {
        assert!(badge.x >= fold::BADGE_PLATE_PX, "{} off the front edge", badge.x);
        assert!(badge.x <= span.total_px() - fold::BADGE_PLATE_PX, "{} off the back edge", badge.x);
    }

    // And a fold elsewhere in the run moves every pixel after it without moving this badge off its own middle.
    let mut folded_head = dropped();
    folded_head.folds.push([0.0, 10.0]);
    let folded_span = band(&folded_head, &recordings);
    let after = fold::badges(&folded_head.folds, &runs, &folded_head.segs, &folded_span);
    let still = badge_at(&after, (30.0, 40.0)).expect("the hole is still there");
    assert!(is_close(still.x, folded_span.x_of(35.0)), "still its own middle: {}", still.x);
}

/// F2.11 (Folds and rows) S1 — a fold takes width and nothing else: the band gets shorter by exactly the folded
/// seconds at the band's own `P.eng.pixelsPerSecond`, a press on a seam belongs to the take after it (§B's half-open inverse), and no rule here
/// measures anything with a fold.
#[test]
fn f2_11_s1_a_fold_takes_no_width_and_chooses_nobody() {
    let recordings = one();
    let cut = dropped();
    let open = band(&cut, &recordings);

    let mut folded = cut.clone();
    fold::toggle_fold(&mut folded, (30.0, 40.0));
    let closed = band(&folded, &recordings);

    assert!(is_close(open.total_px() - closed.total_px(), 10.0 * PPS), "ten seconds of width, no more");
    // The second after the seam is footage, and a press there belongs to it — not to the fold that hides nothing
    // of what the person can see.
    let just_after = closed.x_of(41.0);
    let t = closed.t_at(just_after);
    assert!(t >= 40.0, "a press at the seam reads the later take, got {t}");

    // And nothing a drag measures moved: the fold is a view over the same cut.
    assert_eq!(folded.segs, cut.segs);
    assert_eq!(folded.rows, cut.rows);
    assert_eq!(folded.aspect, cut.aspect);
}

// --- S2: rows ----------------------------------------------------------------------------------------------

/// Two cameras filmed at once and were laid on two rows; a floor above that holds a third row nobody is on.
fn two_cameras(nrows: i32) -> (Vec<Recording>, Cut) {
    let mut cut = Cut::default();
    cut.segs.push(clip(5.0, 15.0));
    cut.nrows = nrows;
    (vec![recording(CAM, 0.0, 60.0), recording("17-25-45", 30.0, 60.0)], cut)
}

/// F2.11 (Folds and rows) S2 — `An emptied bottom row stays until its ✕` (`spec/inventory/cut.md` §B rule 11:
/// "nRows holds an emptied bottom row"): the floor is what keeps a row nobody is on, and it is drawn empty rather
/// than vanishing between one edit and the next. That floor is `P.policy.minSceneSeconds` — the bound a kept scene
/// has to clear, here spent on a row with nothing left on it.
#[test]
fn f2_11_s2_an_emptied_bottom_row_stays_until_its_cross() {
    let (recordings, cut) = two_cameras(3);
    let rows = timeline::rows_for(&recordings, &cut);
    assert_eq!(rows, vec![0, 1], "two cameras, two rows");

    assert_eq!(fold::bottom_row_survives(&recordings, &cut), 3, "the floor holds a third row open");
    assert!(fold::row_is_empty(2, &recordings, &rows, &cut.segs), "and that row is empty");
    assert!(!fold::row_is_empty(0, &recordings, &rows, &cut.segs), "the cameras' rows are not");
}

/// F2.11 (Folds and rows) S2 — the ✕ takes an emptied bottom row away: the count comes down one,
/// `P.policy.minSceneSeconds` is spent so the row may then hold half a second, and a pin that sat above the row names
/// the row it is on now.
#[test]
fn f2_11_s2_the_cross_takes_the_row_and_spends_the_floor() {
    // Pinned up onto row 2 so that taking row 2 away has somewhere for it to come down to.
    let (recordings, mut cut) = two_cameras(3);
    cut.rows.insert("17-25-45".to_string(), 2);
    let rows = timeline::rows_for(&recordings, &cut);
    assert_eq!(rows, vec![0, 2], "the pin is taken literally");
    assert!(fold::row_is_empty(2, &recordings, &rows, &cut.segs) == false, "the pinned camera is on it");

    // Row 1 is the empty one now: nothing coloured and nothing shown from it.
    assert!(fold::row_is_empty(1, &recordings, &rows, &cut.segs));
    let said = fold::kill_row(&mut cut, 1, &recordings, &rows).expect("an empty row has a ✕");
    assert_eq!(said, "removed the empty row 2", "one-based, as the page numbers rows");

    assert_eq!(cut.nrows, 2, "the floor it was held by is spent");
    assert_eq!(cut.rows.get("17-25-45"), Some(&1), "and the camera above came down with it");
    assert_eq!(fold::bottom_row_survives(&recordings, &cut), 2);
}

/// F2.11 (Folds and rows) S2 — only an EMPTY row has this way off the band: a row with a recording on it, or one a
/// kept scene is shown from, refuses and changes nothing, because every exit for footage says what happens to the
/// footage and this deliberately does not. It reads footage rather than length, so `P.policy.minSceneSeconds` never
/// comes into the refusal: a row holding less than it still refuses.
#[test]
fn f2_11_s2_a_row_with_footage_refuses() {
    // A row a recording was coloured on.
    let (recordings, cut) = two_cameras(3);
    let rows = timeline::rows_for(&recordings, &cut);
    let mut held = cut.clone();
    assert_eq!(fold::kill_row(&mut held, 0, &recordings, &rows), None, "camera 1 is on it");
    assert_eq!((held.rows.clone(), held.nrows), (cut.rows.clone(), cut.nrows), "and nothing moved");

    // A row whose only claim is a scene shown from it — no recording coloured there.
    let mut shot = Cut::default();
    shot.segs.push(Seg { s: 5.0, e: 15.0, cam: 1, ..Default::default() });
    shot.nrows = 2;
    let flat = vec![recording(CAM, 0.0, 60.0)];
    let one_row = timeline::rows_for(&flat, &shot);
    assert_eq!(one_row, vec![0]);
    assert_eq!(fold::kill_row(&mut shot, 1, &flat, &one_row), None, "a scene is shown from row 2");

    // A row above every drawn one is not a row the page shows, so it is not this ✕ either: `row_is_empty` only
    // asks whether anything is ON a row, and nothing is on row 8.
    let (recordings, cut) = two_cameras(2);
    let rows = timeline::rows_for(&recordings, &cut);
    assert!(fold::row_is_empty(7, &recordings, &rows, &cut.segs), "nothing is on it");
    let mut beyond = cut.clone();
    let said = fold::kill_row(&mut beyond, 7, &recordings, &rows);
    assert_eq!(said, None, "and a row that is not drawn has no ✕ over it");
}

/// F2.11 (Folds and rows) S2 — the last row survives being empty: a page with no rows is not a page, so the ✕ that
/// takes an emptied bottom row away has nothing to take when it is the only one there. That holds whatever
/// `P.policy.minSceneSeconds` would otherwise allow — a cut with no rows has nowhere for a clip to go.
#[test]
fn f2_11_s2_the_last_row_survives_being_empty() {
    let mut cut = Cut::default();
    cut.nrows = 1;
    let nothing: Vec<Recording> = vec![];
    assert_eq!(fold::bottom_row_survives(&nothing, &cut), 1, "one row is still drawn");

    let rows = timeline::rows_for(&nothing, &cut);
    assert!(fold::row_is_empty(0, &nothing, &rows, &cut.segs), "and it is empty");
    assert_eq!(fold::kill_row(&mut cut, 0, &nothing, &rows), None, "but it is the last row");
    assert_eq!(cut.nrows, 1, "spent by nothing");
}

/// F2.11 (Folds and rows) S2 — `a cut lane's ✕ removes the lane, its pins, shift, pictures and sound`: everything
/// that spoke for that row goes with it. Folding a lane's clips to nothing cuts no scene, so
/// `P.policy.minSceneSeconds` never enters — which is what lets a whole row go. What is deliberately left alone is every scene's `quiet` list naming it:
/// a lane that no longer exists silences nothing, and rewriting saved scenes to change nothing heard would move
/// bytes the autosave compares.
#[test]
fn f2_11_s2_a_lanes_cross_takes_everything_with_it() {
    const LANE: &str = "Second pass";
    let mut cut = Cut::default();
    cut.lanes.push(Lane { name: LANE.to_string(), src: "project:media/pass.mp4".into(), at: 0.0, off: 0.0, dur: 30.0 });
    cut.rows.insert(LANE.to_string(), 1);
    cut.shift.insert(LANE.to_string(), -19.0);
    // The lane's picture, its sound laid over footage, and another lane's business.
    cut.segs.push(Seg { s: 5.0, e: 15.0, ins: "project:media/pass.mp4".into(), dur: 10.0, ..Default::default() });
    cut.segs.push(Seg { s: 20.0, e: 25.0, lane: LANE.to_string(), quiet: vec![LANE.to_string()], ..Default::default() });
    cut.segs.push(Seg { s: 40.0, e: 50.0, lane: "Desk mic".to_string(), quiet: vec![LANE.to_string(), "Desk mic".to_string()], ..Default::default() });

    assert_eq!(fold::remove_lane(&mut cut, LANE), "removed the Second pass lane");
    assert!(cut.lanes.is_empty(), "the lane is gone");
    assert!(!cut.rows.contains_key(LANE), "its pin with it");
    assert!(!cut.shift.contains_key(LANE), "and its shift correction");
    assert_eq!(cut.segs.len(), 1, "its pictures and its sound laid over footage too");

    let kept = &cut.segs[0];
    assert_eq!(kept.lane, "Desk mic", "another lane's scene stays");
    assert_eq!(kept.quiet, vec![LANE.to_string(), "Desk mic".to_string()], "and its quiet list is left as saved");

    // A name nothing holds says the same sentence and changes nothing.
    let before = cut.clone();
    assert_eq!(fold::remove_lane(&mut cut, "Already gone"), "removed the Already gone lane");
    assert_eq!(cut.lanes, before.lanes);
    assert_eq!(cut.segs, before.segs);
}
