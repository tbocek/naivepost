//! F2.7 (Add, Split, Remove, ⌦) — the rules, with no window in sight.
//!
//! Every sentence pinned here is the exact text `spec/05-cut.md` F2.7 says the status line shows, because
//! a refusal that names the wrong thing is a worse failure than one that never fired: the person has to
//! work out which of the two they got.

use naivepost::cut::{Fx, Seg};
use naivepost::cut_select::{Scope, Selection, ANY_ROW, MIN_SECONDS};
use naivepost::cut_verbs::{self, Outcome};

fn footage(s: f64, e: f64, cam: i32) -> Seg {
    Seg {
        s,
        e,
        cam,
        ..Default::default()
    }
}

fn card(s: f64, e: f64, ins: &str) -> Seg {
    Seg {
        s,
        e,
        dur: e - s,
        ins: ins.to_string(),
        ..Default::default()
    }
}

fn band(scope: Scope, start: f64, end: f64) -> Selection {
    Selection {
        start,
        end,
        scope,
    }
}

fn applied(outcome: Outcome) -> (String, Vec<Seg>, bool, bool) {
    match outcome {
        Outcome::Applied {
            status,
            segs,
            keeps_selection,
            into_hand,
        } => (status.clone(), segs.clone(), keeps_selection, into_hand),
        Outcome::Refused(reason) => panic!("expected an applied verb, got a refusal: {reason}"),
    }
}

fn refused(outcome: Outcome) -> String {
    match outcome {
        Outcome::Refused(reason) => reason,
        Outcome::Applied { status, .. } => panic!("expected a refusal, got: {status}"),
    }
}

// --- ＋ Add -----------------------------------------------------------------------------------------

/// The band becomes one kept scene on its own row; nothing else in the cut moves.
#[test]
fn f2_7_s1_add_keeps_the_band_as_one_scene_per_filmed_run() {
    // Two cameras filmed the same stretch; adding over 10..20 keeps one scene per run and takes the span
    // off the row that is not being added to.
    let segs = vec![footage(0.0, 100.0, 0), footage(0.0, 100.0, 1)];
    let sel = band(Scope::Footage { row: 0 }, 10.0, 20.0);
    let outcome = cut_verbs::add(Some(&sel), &segs, &[]);
    let (status, out, keeps_selection, into_hand) = applied(outcome);

    assert_eq!(
        status,
        "added on cam 0, and taken off the other camera \u{2014} \u{21b6} Undo (Ctrl+Z) takes it back"
    );
    assert_eq!(out.len(), 5, "the kept scene plus the remainders of both runs");
    let kept: Vec<(i32, f64, f64)> = out
        .iter()
        .filter(|seg| (seg.s - 10.0).abs() < 1e-9 && (seg.e - 20.0).abs() < 1e-9)
        .map(|seg| (seg.cam, seg.s, seg.e))
        .collect();
    assert_eq!(kept, vec![(0, 10.0, 20.0)], "one scene per filmed run inside the band");
    let outside: Vec<(f64, f64)> = out
        .iter()
        .filter(|seg| !((seg.s - 10.0).abs() < 1e-9 && (seg.e - 20.0).abs() < 1e-9))
        .map(|seg| (seg.s, seg.e))
        .collect();
    assert_eq!(
        outside,
        vec![(0.0, 10.0), (0.0, 10.0), (20.0, 100.0), (20.0, 100.0)],
        "both runs keep what lies outside the band"
    );
    assert!(!keeps_selection, "Add spends the selection");
    assert!(!into_hand, "Add puts nothing in the hand");
}

/// A ruler band covers every camera, so each row gets its own kept scene over the same seconds.
#[test]
fn f2_7_s1_a_ruler_band_adds_to_every_row_the_cut_shows() {
    let segs = vec![footage(0.0, 60.0, 0), footage(0.0, 60.0, 1)];
    let sel = band(Scope::Footage { row: ANY_ROW }, 10.0, 20.0);
    let (_, out, _, _) = applied(cut_verbs::add(Some(&sel), &segs, &[]));

    let kept: Vec<(i32, f64, f64)> = out
        .iter()
        .filter(|seg| (seg.s - 10.0).abs() < 1e-9 && (seg.e - 20.0).abs() < 1e-9)
        .map(|seg| (seg.cam, seg.s, seg.e))
        .collect();
    assert_eq!(kept, vec![(0, 10.0, 20.0), (1, 10.0, 20.0)]);
}

/// The plain single-row case: one row touched, no other row to steal from, so the sentence says only
/// "added" and names nothing.
#[test]
fn f2_7_s1_add_on_a_single_row_names_no_camera() {
    let segs = vec![footage(0.0, 100.0, 0)];
    let sel = band(Scope::Footage { row: 0 }, 10.0, 20.0);
    let (status, _, _, _) = applied(cut_verbs::add(Some(&sel), &segs, &[]));
    assert_eq!(
        status,
        "added \u{2014} \u{21b6} Undo (Ctrl+Z) takes it back"
    );
}

/// Naming the camera is what tells the person where the pictures came from when more than one row was
/// covered and no other row lost anything.
#[test]
fn f2_7_s1_add_names_the_camera_when_more_than_one_row_is_touched() {
    let segs = vec![footage(0.0, 60.0, 0), footage(0.0, 60.0, 1)];
    // Row 1 alone: two rows exist, only one is added to, nothing stolen.
    let sel = band(Scope::Footage { row: 1 }, 10.0, 20.0);
    let (status, _, _, _) = applied(cut_verbs::add(Some(&sel), &segs, &[]));
    assert!(
        status.starts_with("added on "),
        "a multi-row project must say which camera: {status}"
    );
    assert!(status.ends_with("\u{21b6} Undo (Ctrl+Z) takes it back"));
}

/// The theft case: adding on this row took coverage off the other camera, and the sentence has to admit it.
#[test]
fn f2_7_s1_add_admits_when_it_takes_a_scene_off_the_other_camera() {
    // Cam 1 is being added to and cam 0 holds these seconds: cam 0's scene is punched in two, and the
    // sentence has to say the other camera lost it.
    let segs = vec![footage(0.0, 60.0, 0), footage(30.0, 90.0, 1)];
    let sel = band(Scope::Footage { row: 1 }, 10.0, 20.0);
    // The camera named is the one that now shows these seconds; cam 0's scene is what got punched out.
    let (status, _, _, _) = applied(cut_verbs::add(Some(&sel), &segs, &[]));
    assert_eq!(
        status,
        "added on cam 1, and taken off the other camera \u{2014} \u{21b6} Undo (Ctrl+Z) takes it back"
    );
}

#[test]
fn f2_7_s1_add_with_no_band_points_at_the_tracks() {
    let segs = vec![footage(0.0, 60.0, 0)];
    assert_eq!(
        refused(cut_verbs::add(None, &segs, &[])),
        "drag a region on a track first"
    );
}

/// P.policy.minSceneSeconds — the 1 s floor is Add's, so the refusal quotes the length it was given.
#[test]
fn f2_7_s1_add_refuses_under_min_scene_seconds() {
    // P.policy.minSceneSeconds
    let segs = vec![footage(0.0, 60.0, 0)];
    let sel = band(Scope::Footage { row: 0 }, 10.0, 10.5);
    assert_eq!(
        refused(cut_verbs::add(Some(&sel), &segs, &[])),
        "nothing to add: 0.5 s selected, a scene is 1 s or more"
    );
}

/// A sound selection is refused by name: the recording is in the sentence so it says what was drawn on.
#[test]
fn f2_7_s1_add_refuses_a_sound_band_by_name() {
    let segs = vec![footage(0.0, 60.0, 0)];
    let sel = band(Scope::Sound { recording: "lecture".into() }, 10.0, 30.0);
    assert_eq!(
        refused(cut_verbs::add(Some(&sel), &segs, &[])),
        "\u{ff0b} Add keeps footage \u{2014} the selection is lecture's sound"
    );
}

/// P.policy.snapToleranceSeconds — inside the reach an end moves to the candidate, outside it does not.
#[test]
fn f2_7_s1_add_snaps_within_five_seconds_and_not_beyond() {
    // P.policy.snapToleranceSeconds
    assert_eq!(cut_verbs::snap_end(10.0, &[14.9]), 14.9);
    assert_eq!(cut_verbs::snap_end(10.0, &[15.1]), 10.0);
    // Nearest wins when two candidates are both in reach.
    assert_eq!(cut_verbs::snap_end(10.0, &[13.0, 11.5]), 11.5);

    let segs = vec![footage(0.0, 100.0, 0)];
    let sel = band(Scope::Footage { row: 0 }, 10.0, 20.0);
    let (_, out, _, _) = applied(cut_verbs::add(Some(&sel), &segs, &[9.2, 20.8]));
    let kept = out
        .iter()
        .find(|seg| seg.s > 9.0 && seg.e < 21.0 && seg.ins.is_empty() && (seg.e - seg.s) > 5.0)
        .expect("the kept scene");
    assert_eq!((kept.s, kept.e), (9.2, 20.8), "both ends snapped");
}

// --- | Split ----------------------------------------------------------------------------------------

/// Spec rule 14: the right half carries `split`, so coalescing cannot join it back.
#[test]
fn f2_7_s2_split_marks_the_right_half_so_coalescing_keeps_its_hands_off() {
    let segs = vec![footage(0.0, 100.0, 0)];
    let sel = band(Scope::Footage { row: 0 }, 10.0, 20.0);
    let (status, out, keeps_selection, into_hand) = applied(cut_verbs::split(Some(&sel), 0.0, &segs));

    assert_eq!(
        status,
        "split at 00:10 and 00:20 \u{2014} 3 scenes, was 1"
    );
    assert_eq!(out.len(), 3);
    assert!(!out[0].split, "the piece before the first border is untouched");
    assert!(out[1].split, "the middle piece starts at a Split border");
    assert!(out[2].split, "the right half starts at a Split border too");
    assert!(keeps_selection, "Split leaves the band up on purpose");
    assert!(!into_hand);
}

/// A border already there is not drawn, and the sentence does not claim it: only the new end is named.
#[test]
fn f2_7_s2_split_names_only_the_borders_actually_drawn() {
    // 0..10 and 10..100: the band's left end sits on an existing edge.
    let segs = vec![footage(0.0, 10.0, 0), footage(10.0, 100.0, 0)];
    let sel = band(Scope::Footage { row: 0 }, 10.0, 30.0);
    let (status, _, _, _) = applied(cut_verbs::split(Some(&sel), 0.0, &segs));
    assert_eq!(
        status,
        "split at 00:30 \u{2014} 3 scenes, was 2",
        "one border made, so one time named"
    );
}

/// Nothing is removed by a split — the count only ever goes up.
#[test]
fn f2_7_s2_split_removes_nothing() {
    let segs = vec![footage(0.0, 50.0, 0), footage(50.0, 100.0, 0)];
    let sel = band(Scope::Footage { row: 0 }, 20.0, 80.0);
    let (_, out, _, _) = applied(cut_verbs::split(Some(&sel), 0.0, &segs));
    let total: f64 = out.iter().map(|seg| seg.e - seg.s).sum();
    assert!((total - 100.0).abs() < 1e-9, "a split cuts, it never subtracts");
}

/// No selection: one border at the red line and the right half lands in the hand.
#[test]
fn f2_7_s2_split_at_the_line_with_no_selection_takes_the_right_half_in_hand() {
    let segs = vec![footage(0.0, 100.0, 0)];
    let (status, out, keeps_selection, into_hand) = applied(cut_verbs::split(None, 40.0, &segs));
    assert_eq!(status, "split at 00:40 \u{2014} 2 scenes, was 1");
    assert_eq!(out.len(), 2);
    assert!(out[1].split);
    assert!(!keeps_selection, "there was no band to keep");
    assert!(into_hand, "the right half is what the person asked to move next");
}

#[test]
fn f2_7_s2_split_refusals() {
    let segs = vec![footage(0.0, 100.0, 0)];

    // A sound band cannot be cut as footage.
    let sound = band(Scope::Sound { recording: "mic".into() }, 10.0, 20.0);
    assert_eq!(
        refused(cut_verbs::split(Some(&sound), 0.0, &segs)),
        "| Split cuts footage \u{2014} the selection is mic's sound"
    );

    // A band over ground the cut keeps none of.
    let empty = band(Scope::Footage { row: 0 }, 200.0, 230.0);
    assert_eq!(
        refused(cut_verbs::split(Some(&empty), 0.0, &segs)),
        "nothing to split: the cut keeps nothing between 03:20 and 03:50"
    );

    // The red line parked where nothing is kept.
    assert_eq!(
        refused(cut_verbs::split(None, 300.0, &segs)),
        "| Split cuts at the red line \u{2014} click a track to put it somewhere"
    );
}

/// P.eng.minPieceSeconds — a band narrower than two halves cannot be split.
#[test]
fn f2_7_s2_split_refuses_a_band_narrower_than_two_halves() {
    // P.eng.minPieceSeconds
    let segs = vec![footage(0.0, 100.0, 0)];
    let narrow = band(Scope::Footage { row: 0 }, 10.0, 10.06);
    assert_eq!(
        refused(cut_verbs::split(Some(&narrow), 0.0, &segs)),
        "nothing to split at 00:10 \u{2013} 00:10"
    );
    assert!(MIN_SECONDS > 0.0, "the floor is a real number");
}

// --- － Remove --------------------------------------------------------------------------------------

/// P.eng.minPieceSeconds: the remainders survive, exactly the selection goes.
#[test]
fn f2_7_s3_remove_drops_exactly_the_selection_and_keeps_remainders_of_at_least_a_frame() {
    // P.eng.minPieceSeconds
    // Two scenes before the band, so the removal trims one and drops nothing in the middle: the count
    // did NOT rise, which is what makes this the plain "N scene(s), was M" sentence rather than the
    // "scene it went through is two now" one.
    let segs = vec![footage(0.0, 10.0, 0), footage(20.0, 100.0, 0)];
    let sel = band(Scope::Footage { row: 0 }, 10.0, 30.0);
    let (status, out, keeps_selection, into_hand) = applied(cut_verbs::remove(Some(&sel), &segs));

    assert_eq!(
        status,
        "removed 10.0 s \u{2014} 2 scene(s), was 2 (\u{21b6} Undo takes it back)"
    );
    assert_eq!(out.len(), 2);
    assert_eq!((out[0].s, out[0].e), (0.0, 10.0));
    assert_eq!((out[1].s, out[1].e), (30.0, 100.0));
    assert!(!keeps_selection);
    assert!(!into_hand);
}

/// A sliver under a frame is dropped with the selection rather than left to flicker.
#[test]
fn f2_7_s3_a_remainder_shorter_than_a_frame_does_not_survive() {
    // P.eng.minPieceSeconds
    let segs = vec![footage(0.0, 20.0, 0)];
    let sel = band(Scope::Footage { row: 0 }, 0.02, 10.0);
    let (_, out, _, _) = applied(cut_verbs::remove(Some(&sel), &segs));
    assert!(
        out.iter().all(|seg| (seg.e - seg.s) >= MIN_SECONDS),
        "no sub-frame piece is left behind: {out:?}"
    );
}

/// The count rose — one scene went through the gap and came out as two — so the sentence says that instead.
#[test]
fn f2_7_s3_remove_reports_a_scene_split_in_two() {
    let segs = vec![footage(0.0, 100.0, 0)];
    let sel = band(Scope::Footage { row: 0 }, 30.0, 40.0);
    let (status, _, _, _) = applied(cut_verbs::remove(Some(&sel), &segs));
    assert_eq!(
        status,
        "removed 10.0 s \u{2014} the scene it went through is two now (\u{21b6} Undo takes it back)"
    );
}

#[test]
fn f2_7_s3_remove_refusals() {
    let segs = vec![footage(0.0, 100.0, 0)];
    assert_eq!(
        refused(cut_verbs::remove(None, &segs)),
        "drag a region on a track first"
    );

    let sound = band(Scope::Sound { recording: "lecture".into() }, 10.0, 20.0);
    assert_eq!(
        refused(cut_verbs::remove(Some(&sound), &segs)),
        "\u{ff0d} Remove drops footage \u{2014} the selection is lecture's sound"
    );

    let empty = band(Scope::Footage { row: 0 }, 200.0, 230.0);
    assert_eq!(
        refused(cut_verbs::remove(Some(&empty), &segs)),
        "nothing to remove: the cut keeps nothing between 03:20 and 03:50"
    );
}

// --- ⌦ / Delete / BackSpace -------------------------------------------------------------------------

/// The order is the rule: effect first, then clip, then selection, then the scene under the line. Each
/// pair below offers two candidates and checks which one the sentence names.
#[test]
fn f2_7_s4_delete_takes_held_effect_then_clip_then_selection_then_scene_under_line() {
    let segs = vec![footage(0.0, 100.0, 0)];
    let effect = Fx {
        kind: "zoom".into(),
        t: 12.0,
        ..Default::default()
    };
    let card_seg = card(10.0, 20.0, "card:intro.svg");
    let sel = band(Scope::Footage { row: 0 }, 10.0, 20.0);
    let scene = Some(&segs[0]);

    // Effect outranks everything held.
    let (status, _, _, _) = applied(cut_verbs::delete_verb(
        Some(&effect),
        Some(&card_seg),
        Some(&sel),
        scene,
        &segs,
    ));
    assert_eq!(
        status,
        "removed the effect \u{2014} \u{21b6} Undo (Ctrl+Z) takes it back"
    );

    // With the effect gone the held card is next, ahead of the band.
    let (status, _, _, _) = applied(cut_verbs::delete_verb(
        None,
        Some(&card_seg),
        Some(&sel),
        scene,
        &segs,
    ));
    assert_eq!(
        status,
        "removed the card \u{2014} card:intro.svg (\u{21b6} Undo takes it back)",
        "⌦ on a held card is the only way a spliced insert can be removed"
    );

    // With nothing held, the selection answers before the line does.
    let (status, _, _, _) = applied(cut_verbs::delete_verb(None, None, Some(&sel), scene, &segs));
    assert_eq!(status, "removed \u{2014} 1 segment(s), was 1");

    // Only the line left: the scene under it goes.
    let (status, _, _, _) = applied(cut_verbs::delete_verb(None, None, None, scene, &segs));
    assert_eq!(
        status,
        "removed the scene under the line \u{2014} \u{21b6} Undo (Ctrl+Z) takes it back"
    );
}

/// A sound selection still refuses ⌦: dropping footage cannot be aimed at a lane's seconds, even with a
/// kept scene sitting under the line. That refusal is §8's own sentence, reached through `cut_delete`.
#[test]
fn f2_7_s4_delete_refuses_a_sound_selection_even_with_a_scene_under_the_line() {
    let segs = vec![footage(0.0, 100.0, 0)];
    let sound = band(Scope::Sound { recording: "mic".into() }, 10.0, 20.0);
    assert_eq!(
        refused(cut_verbs::delete_verb(None, None, Some(&sound), Some(&segs[0]), &segs)),
        "\u{2326} drops footage \u{2014} the selection is mic's sound"
    );
}

#[test]
fn f2_7_s4_delete_refuses_with_nothing_in_hand() {
    let segs: Vec<Seg> = vec![];
    assert_eq!(
        refused(cut_verbs::delete_verb(None, None, None, None, &segs)),
        "nothing selected \u{2014} click a kept scene, or drag a region on a track"
    );
}

// --- the ✕ badges -----------------------------------------------------------------------------------

#[test]
fn f2_7_s5_the_five_badge_places() {
    use cut_verbs::{badge_action, Badge, BADGE_TARGETS};
    assert_eq!(BADGE_TARGETS.len(), 5);
    assert_eq!(
        BADGE_TARGETS,
        [
            Badge::GreenBar,
            Badge::Selection,
            Badge::CutLane,
            Badge::EmptyRow,
            Badge::Effect
        ]
    );
    assert_eq!(badge_action(Badge::GreenBar), "drop that scene");
    assert_eq!(
        badge_action(Badge::Selection),
        "clear the selection \u{2014} nothing is removed"
    );
    assert_eq!(badge_action(Badge::CutLane), "take this lane away");
    assert_eq!(badge_action(Badge::EmptyRow), "drop this row");
    assert_eq!(badge_action(Badge::Effect), "put this effect down");
}
