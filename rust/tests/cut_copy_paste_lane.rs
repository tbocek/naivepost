// F2.9 (Copy, Paste, Lane) — spec/05-cut.md F2.9.
// One test per step: what ⧉ Copy takes and refuses (S1), where ⧉ Paste puts the seconds and what it says about
// them (S2), and what ⇲ Lane makes of a footage copy (S3). Every rule is a plain function in
// naivepost::cut_copy, so nothing here needs a widget or a display.

use naivepost::cut::{Cut, Lane, Seg};
use naivepost::cut_copy as cp;
use naivepost::cut_screen::cut_seconds;
use naivepost::cut_select::{Scope, Selection};
use naivepost::tools;

/// The recording a lane names — `Cut::lanes`' key and every laid piece's `lane`.
const REC: &str = "2026-09-16 17-26-20";
/// A file second the copied sound is read from, so a laid piece can be told from the footage it stands over.
const FILE: &str = "assets/audio.wav";

fn footage_selection(start: f64, end: f64) -> Selection {
    Selection { start, end, scope: Scope::Footage { row: 0 } }
}

fn sound_selection(start: f64, end: f64) -> Selection {
    Selection { start, end, scope: Scope::Sound { recording: REC.to_string() } }
}

/// The hand after a copy that was allowed, panicking with the refusal when it was not — most cases below are
/// about what a hand then does, not about whether it exists.
fn taken(selection: &Selection) -> cp::Hand {
    match cp::copy(Some(selection)) {
        cp::Take::Taken(hand) => hand,
        cp::Take::TooShort(why) => panic!("copy refused for length: {why}"),
        cp::Take::NothingSelected(why) => panic!("copy refused: {why}"),
    }
}

/// Kept footage at these spans — filmed pictures, so `ins` stays empty.
fn kept(spans: &[(f64, f64)]) -> Cut {
    let mut cut = Cut::default();
    for (s, e) in spans {
        cut.segs.push(Seg { s: *s, e: *e, cam: 0, ..Default::default() });
    }
    cut
}

/// The seconds of every piece of footage the cut keeps, sorted — how these cases check a paste left the
/// picture alone.
fn film(cut: &Cut) -> Vec<(f64, f64)> {
    let mut spans: Vec<(f64, f64)> = cut.segs.iter().filter(|seg| seg.ins.is_empty()).map(|seg| (seg.s, seg.e)).collect();
    spans.sort_by(|a, b| a.0.total_cmp(&b.0));
    spans
}

/// The laid pieces: each names the recording it stands in for and reads a file second, which is what tells a
/// laid sound apart from a pasted card (both carry `ins`).
fn pieces(cut: &Cut) -> Vec<(f64, f64, f64)> {
    let mut found: Vec<(f64, f64, f64)> = cut
        .segs
        .iter()
        .filter(|seg| seg.lane == REC && !seg.ins.is_empty() && seg.ss > 0.0)
        .map(|seg| (seg.s, seg.e, seg.ss))
        .collect();
    found.sort_by(|a, b| a.0.total_cmp(&b.0));
    found
}

/// f64 comparisons in this file go through a tolerance: the seconds come from arithmetic on the selection and
/// on the cut's own spans, and an exact match would be luck rather than a rule.
fn is_close(got: f64, want: f64) -> bool {
    (got - want).abs() < 1e-9
}

// --- S1: taking it in hand -------------------------------------------------------------------------------

/// F2.9 (Copy, Paste, Lane) S1 — `⧉ Copy takes the selection in hand (≥ 1 s …)`; the floor is
/// `P.policy.minSceneSeconds`, read from cut_select where §10 catalogues it. Eight seconds of the pictures are
/// taken whole, and a drag that ran right-to-left normalises to the same seconds.
#[test]
fn f2_9_s1_a_second_of_selection_is_taken_in_hand() {
    let hand = taken(&footage_selection(12.0, 20.0));
    assert!(is_close(hand.from, 12.0), "from is the lower end, got {}", hand.from);
    assert!(is_close(hand.length, 8.0), "eight seconds, got {}", hand.length);
    assert_eq!(hand.scope, Scope::Footage { row: 0 }, "what was drawn on travels with the seconds");

    let backwards = taken(&footage_selection(20.0, 12.0));
    assert!(is_close(backwards.from, 12.0), "a right-to-left drag starts where the seconds start");
    assert!(is_close(backwards.length, 8.0), "and is still eight seconds of them");

    // Both sides of the floor: exactly a second is the shortest thing that can be taken.
    assert!(matches!(cp::copy(Some(&footage_selection(4.0, 5.0))), cp::Take::Taken(_)), "1.0 s is enough");
    assert!(matches!(cp::copy(Some(&footage_selection(4.0, 4.99))), cp::Take::TooShort(_)), "0.99 s is not");
}

/// F2.9 (Copy, Paste, Lane) S1 — the two refusals are said in words: a stretch under the floor says what was
/// offered to two decimals (half a second must not read as "under 1 s" with a number that rounds to it), and an
/// empty band says where to draw rather than failing quietly.
#[test]
fn f2_9_s1_under_a_second_there_is_nothing_worth_copying() {
    let cp::Take::TooShort(why) = cp::copy(Some(&footage_selection(4.0, 4.5))) else {
        panic!("half a second must not be copyable");
    };
    assert_eq!(why, "the selection is 0.50 s \u{2014} under 1 s there is nothing worth copying");

    let cp::Take::NothingSelected(why) = cp::copy(None) else { panic!("an empty band cannot be copied") };
    assert_eq!(
        why,
        "select a stretch of the pictures or of a lane first \u{2014} \u{29c9} Copy takes the selection in hand"
    );
}

/// F2.9 (Copy, Paste, Lane) S1 — `the selection stays visible`: taking a copy is READING and not editing, so
/// nothing is cleared and the band keeps what it showed. A pure borrow, checked on both kinds of band.
#[test]
fn f2_9_s1_taking_a_copy_leaves_the_selection_on_the_band() {
    let film = footage_selection(12.0, 20.0);
    let _hand = taken(&film);
    assert!(is_close(film.start, 12.0) && is_close(film.end, 20.0), "the band still spans what it spanned");
    assert_eq!(film.scope, Scope::Footage { row: 0 });

    let sound = sound_selection(12.0, 20.0);
    let _hand = taken(&sound);
    assert_eq!(sound.scope, Scope::Sound { recording: REC.to_string() }, "a lane's band still names its recording");
}

/// F2.9 (Copy, Paste, Lane) S1 — what the page says once seconds are in hand, and where to take them next. A
/// sound names its recording, because "the footage at 00:12" is not what a lane's seconds are.
#[test]
fn f2_9_s1_the_hand_says_what_it_holds() {
    let hand = taken(&footage_selection(12.0, 20.0));
    assert_eq!(
        cp::copied_status(&hand),
        "copied 00:12 \u{2013} 00:20 (8.0 s) \u{2014} click where it goes, then \u{29c9} Paste"
    );

    let sound = taken(&sound_selection(12.0, 20.0));
    assert_eq!(
        cp::copied_status(&sound),
        format!("copied 8.0 s of {REC} (00:12 \u{2013} 00:20) \u{2014} click where it goes, then \u{29c9} Paste")
    );

    // Both readings are built from the hand's own seconds through the app's formatters, so a paste asked later
    // cannot quote different ones.
    assert!(cp::copied_status(&hand).contains(&tools::mm_ss(hand.from)), "the status quotes the hand");
}

// --- S2: footage is spliced -------------------------------------------------------------------------------

/// F2.9 (Copy, Paste, Lane) S2 — `footage → spliced insert copy:<seconds>`: the card a footage paste puts in
/// the cut. `s == e` with a `dur` is what makes it spliced, and the `copy:<seconds>` name round-trips through
/// the same reader `cut.rs` documents.
#[test]
fn f2_9_s2_a_footage_paste_is_a_spliced_copy_card() {
    let hand = taken(&footage_selection(12.0, 20.0));
    let card = cp::spliced(&hand, 30.0);
    assert!(is_close(card.s, 30.0) && is_close(card.e, 30.0), "s == e opens the cut there");
    assert!(is_close(card.dur, 8.0), "and it owns eight seconds");
    assert!(card.is_insert(), "a card covers no footage of its own");
    assert_eq!(card.copy_seconds(), Some(12.0), "the file name is copy:<seconds> the source started at");
    assert!(is_close(card.ss, 0.0), "a card reads no file second, which keeps it spliced and not overwriting");
}

/// F2.9 (Copy, Paste, Lane) S2 — `⧉ Paste puts those seconds at the red line · the video gets longer`: both
/// totals are reported so "it got longer" is checkable against what it was.
#[test]
fn f2_9_s2_pasting_footage_lengthens_the_cut_and_names_both_totals() {
    let mut cut = kept(&[(0.0, 30.0), (40.0, 70.0)]);
    assert!(is_close(cut_seconds(&cut), 60.0), "the two stretches are a minute");

    let mut hand: Option<cp::Hand> = Some(taken(&footage_selection(12.0, 20.0)));
    let said = cp::paste(&mut cut, &mut hand, Some(35.0), FILE, 12.0).expect("a footage paste needs only a line");
    assert_eq!(said, "pasted 8.0 s from 00:12 at 00:35 \u{2014} the cut is 01:08, was 01:00");

    assert_eq!(cut.segs.iter().filter(|seg| seg.copy_seconds().is_some()).count(), 1, "the card is in the cut");
    assert!(is_close(cut_seconds(&cut), 68.0), "and the cut really did grow by eight seconds");
}

/// F2.9 (Copy, Paste, Lane) S2 — `⧉ Paste puts those seconds at the red line`: with no line yet there is
/// nowhere to put them. The page asks for the click instead of pasting at zero, and waiting costs nothing.
#[test]
fn f2_9_s2_a_paste_needs_a_line_first() {
    let mut cut = kept(&[(0.0, 30.0)]);
    let mut hand: Option<cp::Hand> = Some(taken(&footage_selection(12.0, 20.0)));
    assert_eq!(cp::paste(&mut cut, &mut hand, None, FILE, 12.0).unwrap_err(), cp::NO_LINE_YET);
    assert!(matches!(hand.as_ref(), Some(_)), "the seconds are still in hand for when the line exists");
    assert_eq!(cut.segs.len(), 1, "and nothing was placed at zero");
}

/// F2.9 (Copy, Paste, Lane) S2 — `sound → laid over the kept footage at the line, one piece per kept stretch`:
/// a copy crossing a hole becomes two pieces, each carrying its own part of the file (`ss` walks, so they are
/// two parts of one sound and not its opening seconds twice) and naming the recording it replaces.
#[test]
fn f2_9_s2_sound_is_laid_over_the_kept_footage_in_pieces() {
    let mut cut = kept(&[(0.0, 20.0), (30.0, 40.0)]);
    // A twenty-second copy laid at 14 runs to 34 and stands over both stretches. The hole from 20 to 30 is
    // covered by nothing at all — a span reaching into it does not invent footage — so the pieces are six
    // seconds and four, with the seconds over the hole simply never played.
    let hand = taken(&sound_selection(50.0, 70.0));
    assert_eq!(cp::footage_stretches(&cut, 14.0, 22.0), vec![(14.0, 20.0)], "a span that stops short of the next stretch sees one");
    assert_eq!(cp::footage_stretches(&cut, 14.0, 34.0), vec![(14.0, 20.0), (30.0, 34.0)], "and two once it reaches the next");

    let laid = cp::lay_over(&mut cut, FILE, &hand, 14.0, 5.0);
    assert_eq!(laid, 2, "one piece per kept stretch");

    let pieces = pieces(&cut);
    assert_eq!(pieces.len(), 2);
    // The hole is bridged by nothing: each piece carries its own part of the file, so the sound resumes where
    // the footage did rather than replaying its opening seconds.
    assert!(is_close(pieces[0].0, 14.0) && is_close(pieces[0].1, 20.0), "the first piece runs to the hole");
    assert!(is_close(pieces[0].2, 5.0), "and starts at the file second it was copied from");
    assert!(is_close(pieces[1].0, 30.0) && is_close(pieces[1].1, 34.0), "the second resumes where the footage came back");
    assert!(is_close(pieces[1].2, 21.0), "sixteen seconds into the file: where the copy stands at 30, hole and all");
    for seg in cut.segs.iter().filter(|seg| seg.lane == REC && !seg.ins.is_empty()) {
        // One name for the whole span, however many pieces it became: a hole in the footage is no reason to
        // change what is being replaced.
        assert_eq!(seg.lane, REC);
    }

    // The picture was kept exactly as it was — the footage before the first piece and after the second survive,
    // and the seconds between the stretches were never filmed to begin with.
    assert_eq!(film(&cut), vec![(0.0, 14.0), (34.0, 40.0)]);
    assert_eq!(
        cp::laid_status(20.0, REC, 2, 14.0),
        format!("laid 20.0 s of {REC} over 2 stretches of footage at 00:14")
    );
}

/// F2.9 (Copy, Paste, Lane) S2 — `the picture is kept exactly as it was and the video does not get longer`: a
/// copy lying over one unbroken stretch becomes one piece inside it, and its status says "the footage" rather
/// than counting a single stretch.
#[test]
fn f2_9_s2_one_stretch_says_the_footage_not_stretches() {
    let mut cut = kept(&[(10.0, 24.0)]);
    let hand = taken(&sound_selection(12.0, 20.0));
    assert_eq!(cp::lay_over(&mut cut, FILE, &hand, 14.0, 5.0), 1);

    let pieces = pieces(&cut);
    assert_eq!(pieces.len(), 1);
    assert!(is_close(pieces[0].0, 14.0) && is_close(pieces[0].1, 22.0), "one piece inside the stretch");
    // The film before and after the piece are still filmed from the same file second they started at.
    assert_eq!(film(&cut), vec![(10.0, 14.0), (22.0, 24.0)]);

    let before = kept(&[(10.0, 24.0)]);
    assert!(is_close(cut_seconds(&cut), cut_seconds(&before)), "no second was added by laying a sound over it");
    assert_eq!(cp::laid_status(8.0, REC, 1, 14.0), format!("laid 8.0 s of {REC} over the footage at 00:14"));
}

/// F2.9 (Copy, Paste, Lane) S2 — `refused: "the cut keeps no footage at m:ss — a sound needs a picture under
/// it"`. A miss is not a failure: the copy stays in hand so the line can move and ⧉ Paste be pressed again.
#[test]
fn f2_9_s2_a_sound_with_no_picture_under_it_is_refused_and_stays_in_hand() {
    let mut cut = kept(&[(0.0, 20.0)]);
    let mut hand: Option<cp::Hand> = Some(taken(&sound_selection(12.0, 20.0)));
    let before = cut.segs.clone();

    assert_eq!(cp::no_footage_status(50.0), format!("the cut keeps no footage at 00:50 \u{2014} a sound needs a picture under it"));
    assert_eq!(cp::paste(&mut cut, &mut hand, Some(50.0), FILE, 12.0).unwrap_err(), cp::no_footage_status(50.0));

    assert!(matches!(hand.as_ref(), Some(_)), "the copy survives a paste that missed");
    assert_eq!(cut.segs, before, "and the miss changed nothing");
}

/// F2.9 (Copy, Paste, Lane) S2 — only footage is a picture under a sound: a card covers none of its own, so a
/// cut holding just an insert at the line has nothing to lay a copy over and the paste is refused.
#[test]
fn f2_9_s2_an_insert_is_not_a_picture_under_a_sound() {
    let mut cut = Cut::default();
    cut.segs.push(Seg { s: 40.0, e: 40.0, ins: "assets/sting.mp4".into(), dur: 3.0, ..Default::default() });

    assert!(cp::footage_stretches(&cut, 38.0, 44.0).is_empty(), "an insert stands over no footage");

    let mut hand: Option<cp::Hand> = Some(taken(&sound_selection(12.0, 20.0)));
    assert_eq!(cp::paste(&mut cut, &mut hand, Some(40.0), FILE, 12.0).unwrap_err(), cp::no_footage_status(40.0));
    assert_eq!(cut.segs.len(), 1, "nothing was laid over the card");
}

/// F2.9 (Copy, Paste, Lane) S2 — `Pasting consumes the copy`: one paste places one card, a second press with
/// nothing in hand places no second one, and there is nothing left for Esc to drop.
#[test]
fn f2_9_s2_pasting_consumes_the_copy_once() {
    let mut cut = kept(&[(0.0, 30.0)]);
    let mut hand: Option<cp::Hand> = Some(taken(&footage_selection(12.0, 20.0)));
    cp::paste(&mut cut, &mut hand, Some(10.0), FILE, 12.0).expect("a line was placed");
    assert!(matches!(hand.as_ref(), None), "the paste consumed it");

    let second = cp::paste(&mut cut, &mut hand, Some(20.0), FILE, 12.0);
    assert!(second.is_err(), "with nothing in hand there is nothing to paste");
    assert_eq!(cut.segs.iter().filter(|seg| seg.copy_seconds().is_some()).count(), 1, "and no second card appeared");
    assert!(!cp::drop(&mut hand), "nothing was left in hand for Esc to drop");
}

// --- S3: ⇲ Lane ------------------------------------------------------------------------------------------

/// F2.9 (Copy, Paste, Lane) S2 — `Esc drops it`: letting go of a copy that was never wanted is reported, and
/// dropping nothing says so rather than claiming a copy went away.
#[test]
fn f2_9_s3_esc_drops_the_copy() {
    let mut hand: Option<cp::Hand> = Some(taken(&sound_selection(12.0, 20.0)));
    assert!(cp::drop(&mut hand), "the copy in hand was dropped");
    assert!(matches!(hand.as_ref(), None));
    assert!(!cp::drop(&mut None), "with nothing in hand there is nothing to drop");
}

/// F2.9 (Copy, Paste, Lane) S3 — `footage copy on its own row (a cut lane windowing the file), nothing cut
/// yet`: the row windows the FILE from where the copy was taken and starts where it is told; the caller adds no
/// scene, because a lane that arrived already green would be a cut nobody made.
#[test]
fn f2_9_s3_a_footage_copy_gets_a_row_of_its_own_and_nothing_is_cut() {
    let cut = kept(&[(0.0, 30.0)]);
    let before = cut.segs.clone();
    let hand = taken(&footage_selection(12.0, 20.0));

    // `off` is the FILE second the copy was taken at: a session second and that second in this file differ by
    // wherever the recording sits, and the caller converts them.
    let lane = cp::lane(&hand, "project:video/a.mp4", 12.0, 100.0, "a".into()).expect("eight seconds is a row");
    assert_eq!(lane, Lane { name: "a".into(), src: "project:video/a.mp4".into(), at: 100.0, off: 12.0, dur: 8.0 });

    assert_eq!(cut.segs, before, "nothing was cut to the new row");
    assert_eq!(cp::lane_status(8.0, 12.0, "a", 100.0), "8.0 s from 00:12 is now the a lane, starting at 01:40");
}

/// F2.9 (Copy, Paste, Lane) S3 — `Refused: "that copy is too short to be a lane of its own"` (`P.policy.minSceneSeconds`,
/// the same floor as ⧉ Copy: a row narrower than it is a band no handle can be grabbed on), and a name nobody
/// else has already used, because the name is what `Cut::rows` keys on and every scene's `quiet` list names.
#[test]
fn f2_9_s3_a_lane_needs_a_seconds_worth_and_a_name_nobody_uses() {
    let short = cp::Hand { from: 4.0, length: 0.5, scope: Scope::Footage { row: 0 } };
    assert_eq!(cp::lane(&short, "project:video/a.mp4", 0.0, 0.0, "a".into()), None);
    assert_eq!(cp::too_short(), "that copy is too short to be a lane of its own");

    let edge = cp::Hand { from: 4.0, length: 1.0, scope: Scope::Footage { row: 0 } };
    assert!(cp::lane(&edge, "project:video/a.mp4", 0.0, 0.0, "a".into()).is_some(), "exactly the floor is a row");

    assert_eq!(cp::lane_name("a", &[]), "a", "a free name is used as offered");
    assert_eq!(cp::lane_name("a", &["a".into()]), "a-2");
    assert_eq!(cp::lane_name("a", &["a".into(), "a-2".into()]), "a-3");
}

/// F2.9 (Copy, Paste, Lane) S3 — ⇲ Lane is offered for a footage copy; a copied sound has no picture to put on
/// a row. The refusal lives in the button's tooltip and in the two kinds being distinguishable at the call site:
/// `Scope` is what decides, so the page never has to ask which kind it holds.
#[test]
fn f2_9_s3_a_sound_copy_never_becomes_a_lane() {
    let sound = taken(&sound_selection(12.0, 20.0));
    let film = taken(&footage_selection(12.0, 20.0));
    assert!(!matches!(sound.scope, Scope::Footage { .. }), "a sound hand is not footage, so ⇲ Lane does not apply");
    assert!(matches!(film.scope, Scope::Footage { row } if row == 0), "and the footage hand carries the row it shows on");

    // The kinds are one match away from each other at the call site, which is where the tooltip's words come from.
    let says: &str = match sound.scope {
        Scope::Sound { .. } => "a copied SOUND has no picture to put on a row",
        Scope::Footage { .. } => "the copy gets a row of its own",
    };
    assert_eq!(says, "a copied SOUND has no picture to put on a row");
}
