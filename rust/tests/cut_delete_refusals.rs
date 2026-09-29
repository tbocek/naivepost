// §05-cut#8-details-confirmed-against-the-code-verification-pass — bullet 1 (Refusals).
//
// Every sentence here is normative in spec/05-cut.md §8 and spec/inventory/cut.md §E, so each test compares a
// whole string rather than a substring: a reworded refusal passes a `contains` check and still fails the spec.
// The rules themselves belong to earlier items (F2.9 owns Copy/Paste/Lane); what this file pins is that the
// refusals are the ones written down, and that two of them leave the copy in hand rather than swallowing it.

use naivepost::cut::{Cut, Seg};
use naivepost::cut_copy as cp;
use naivepost::cut_delete as del;
use naivepost::cut_select::{Scope, Selection};

#[allow(dead_code)] // every test binary compiles this whole module
mod common;
use common::{clip};

const REC: &str = "2026-09-16 17-26-20";
const GONE: &str = "2026-01-01 09-00-00";
const FILE: &str = "assets/audio.wav";

fn footage(start: f64, end: f64) -> Selection {
    Selection { start, end, scope: Scope::Footage { row: 0 } }
}

fn sound(start: f64, end: f64) -> Selection {
    Selection { start, end, scope: Scope::Sound { recording: REC.into() } }
}

/// §05-cut#8-details-confirmed-against-the-code-verification-pass — `Copy refuses with both of its sentences`:
/// nothing marked at all, and a band under the one-second floor. The floor is `// P.policy.minSceneSeconds`,
/// read from cut_select where §10's row is catalogued.
#[test]
fn sec_05_cut_8_details_confirmed_against_the_code_verification_pass_s1_copy_refuses_both_ways() {
    assert_eq!(
        cp::copy(None),
        cp::Take::NothingSelected(
            "select a stretch of the pictures or of a lane first \u{2014} \u{29c9} Copy takes the selection in hand",
        )
    );

    // P.policy.minSceneSeconds — half a second, spelled to two decimals so it cannot read as "under 0.5 s".
    assert_eq!(
        cp::copy(Some(&footage(10.0, 10.5))),
        cp::Take::TooShort("the selection is 0.50 s \u{2014} under 1 s there is nothing worth copying".to_string())
    );
    // A right-to-left drag of the same width is refused for its length, never for its sign.
    assert_eq!(
        cp::copy(Some(&footage(10.5, 10.0))),
        cp::Take::TooShort("the selection is 0.50 s \u{2014} under 1 s there is nothing worth copying".to_string())
    );
    // Exactly the floor is not under it.
    assert!(matches!(cp::copy(Some(&footage(10.0, 11.0))), cp::Take::Taken(_)));
}

/// §05-cut#8-details-confirmed-against-the-code-verification-pass — `A sound copy names its lane in the status`:
/// "copied X s of <lane> (a – b) — click where it goes, then ⧉ Paste". A lane's seconds are not "the footage at
/// 4:10", so the recording has to be in the sentence.
#[test]
fn sec_05_cut_8_details_confirmed_against_the_code_verification_pass_s1_sound_copy_names_its_lane() {
    let cp::Take::Taken(hand) = cp::copy(Some(&sound(65.0, 81.0))) else { panic!("eight seconds are copyable") };
    let (from, to) = (naivepost::tools::mm_ss(65.0), naivepost::tools::mm_ss(81.0));
    assert_eq!(
        cp::copied_status(&hand),
        format!("copied 16.0 s of {REC} ({from} \u{2013} {to}) \u{2014} click where it goes, then \u{29c9} Paste")
    );
    // The footage sentence is a different shape — seconds first, no lane.
    let cp::Take::Taken(pictures) = cp::copy(Some(&footage(65.0, 81.0))) else { panic!("eight seconds are copyable") };
    assert_eq!(
        cp::copied_status(&pictures),
        format!("copied {from} \u{2013} {to} (16.0 s) \u{2014} click where it goes, then \u{29c9} Paste")
    );
}

/// §05-cut#8-details-confirmed-against-the-code-verification-pass — `Lane refuses with its three sentences`: no
/// line yet, a line nothing is running at, and a copy too short for a row. Each names the missing thing rather
/// than failing quietly, and each is ⇲ Lane's own wording, not ⧉ Paste's.
#[test]
fn sec_05_cut_8_details_confirmed_against_the_code_verification_pass_s1_lane_refusals() {
    let filmed = [(0.0, 30.0), (40.0, 90.0)];
    let cp::Take::Taken(hand) = cp::copy(Some(&footage(10.0, 20.0))) else { panic!("ten seconds are copyable") };

    // No red line: the page has never been clicked, so "where the lane starts" has no answer.
    assert_eq!(
        cp::lane_start(None, &hand, &filmed),
        cp::LaneStart::Refusal("click the timeline where the new lane starts first".to_string())
    );
    // The line stands in a hole between two recordings — nothing was filmed there.
    assert_eq!(
        cp::lane_start(Some(35.0), &hand, &filmed),
        cp::LaneStart::Refusal(format!("nothing is rolling at {} any more", naivepost::tools::mm_ss(35.0)))
    );
    // A copy barely over the floor makes a row, one under it does not.
    // P.policy.minSceneSeconds
    let short = cp::Hand { length: 0.5, ..hand.clone() };
    assert_eq!(
        cp::lane_start(Some(45.0), &short, &filmed),
        cp::LaneStart::Refusal("that copy is too short to be a lane of its own".to_string())
    );
    // Where all three are answered, the line is the start.
    assert_eq!(cp::lane_start(Some(45.0), &hand, &filmed), cp::LaneStart::Start(45.0));
    // A recording's first second is rolling; its last one is not — the run's end is exclusive.
    assert_eq!(cp::lane_start(Some(90.0), &hand, &filmed), cp::LaneStart::Refusal(cp::not_rolling(90.0)));
    assert_eq!(cp::lane_start(Some(40.0), &hand, &filmed), cp::LaneStart::Start(40.0));
}

/// §05-cut#8-details-confirmed-against-the-code-verification-pass — `Paste refuses a recording that left the
/// session`, and leaves the copy in hand to be aimed again: "<base> is not in the session any more — the copied
/// sound has nowhere to come from".
#[test]
fn sec_05_cut_8_details_confirmed_against_the_code_verification_pass_s1_paste_refuses_a_gone_recording() {
    let mut cut = Cut { segs: vec![clip(0.0, 60.0)], ..Default::default() };
    let cp::Take::Taken(held) = cp::copy(Some(&sound(10.0, 20.0))) else { panic!("ten seconds are copyable") };
    let mut hand = Some(held.clone());
    let before = cut.segs.clone();

    assert_eq!(
        cp::paste(&mut cut, &mut hand, Some(5.0), FILE, 10.0, &[GONE.into()]).unwrap_err(),
        format!("{REC} is not in the session any more \u{2014} the copied sound has nowhere to come from")
    );
    // Refused twice over: nothing was laid, and the hand still holds the seconds.
    assert_eq!(cut.segs, before, "a refused paste leaves the cut alone");
    assert!(hand.is_some(), "a paste with nowhere to go leaves the copy in hand");

    // The same call with the recording back in the session lays it and consumes the copy.
    let mut gone = Some(cp::Hand { length: 10.0, ..held });
    let said = cp::paste(&mut cut, &mut gone, Some(5.0), FILE, 10.0, &[REC.into()])
        .expect("the file is in the session now");
    assert_eq!(said, format!("laid 10.0 s of {REC} over the footage at {}", naivepost::tools::mm_ss(5.0)));
    assert!(gone.is_none(), "a paste that happened consumes the copy");

    // And the sentence names whichever recording went missing.
    assert_eq!(
        cp::no_source(GONE),
        format!("{GONE} is not in the session any more \u{2014} the copied sound has nowhere to come from")
    );
}

/// §05-cut#8-details-confirmed-against-the-code-verification-pass — `A sound copy starting before its recording
/// is read from the lane's first second`: the laid pieces carry file seconds, not session seconds, and nothing
/// earlier than the recording's start exists on that lane to play.
#[test]
fn sec_05_cut_8_details_confirmed_against_the_code_verification_pass_s1_a_copy_before_its_recording_starts() {
    // The recording covers 20–60 of the session; the band was dragged from 10, five seconds too early.
    let rec_start = 20.0;
    // The band was dragged from ten seconds before the recording began: it starts at the lane's first second.
    let cp::Take::Taken(hand) = cp::copy(Some(&sound(10.0, 30.0))) else { panic!("twenty seconds are copyable") };
    // What the caller passes after converting session→file seconds: clamped to the lane's own beginning.
    let file_seconds = (hand.from - rec_start).max(0.0);
    assert_eq!(file_seconds, 0.0, "nothing earlier on that lane plays");

    let mut cut = Cut { segs: vec![clip(0.0, 60.0)], ..Default::default() };
    let mut hand = Some(hand);
    cp::paste(&mut cut, &mut hand, Some(25.0), FILE, file_seconds, &[REC.into()]).expect("footage is under it");
    let laid = cut.segs.iter().find(|seg| !seg.ins.is_empty()).expect("the paste laid a piece");
    // The paste starts at the clamped second, so the piece opens on the file's first second: nothing earlier
    // than the recording's start was copied, and nothing earlier can be played.
    assert_eq!(laid.ss, 0.0, "the span opens at the lane's first second");
    assert_eq!(laid.lane, REC, "every piece stands in for the same recording");

    // A copy that begins inside the recording keeps its own offset instead of restarting the file.
    let cp::Take::Taken(mid) = cp::copy(Some(&sound(35.0, 45.0))) else { panic!("ten seconds are copyable") };
    assert_eq!((mid.from - rec_start).max(0.0), 15.0);
}

/// §05-cut#8-details-confirmed-against-the-code-verification-pass — `⌦ refuses with both of its sentences`: a
/// selection drawn on a lane's sound, and a red line standing where the cut keeps nothing.
#[test]
fn sec_05_cut_8_details_confirmed_against_the_code_verification_pass_s1_delete_refusals() {
    // A lane's seconds are not the pictures' seconds.
    let audio = Scope::Sound { recording: REC.into() };
    assert_eq!(
        del::refuses(Some(&audio), Some(&clip(0.0, 30.0))),
        Some(format!("\u{2326} drops footage \u{2014} the selection is {REC}'s sound"))
    );
    // Nothing under the line — a removed stretch answers None through scene_at, so nothing is re-derived here.
    let segs = [clip(0.0, 30.0)];
    assert!(del::scene_under(&segs, 45.0).is_none());
    assert_eq!(
        del::refuses(None, del::scene_under(&segs, 45.0)),
        Some("the playhead is not on a kept scene \u{2014} click a green one, or drag a region".to_string())
    );
    // A sound selection outranks a scene under the line: it was touched last and cannot be honoured.
    assert!(del::refuses(Some(&audio), del::scene_under(&segs, 10.0)).is_some());

    // Footage held, or a scene under the line: ⌦ may go ahead and says nothing.
    let pictures = Scope::Footage { row: 0 };
    assert_eq!(del::refuses(Some(&pictures), del::scene_under(&segs, 10.0)), None);
}
