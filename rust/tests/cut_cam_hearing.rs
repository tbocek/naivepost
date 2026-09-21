// F2.10 (Cameras and hearing) — spec/05-cut.md F2.10.
// One test per step: which row a scene's picture comes from (S1), whether this scene hears a lane — per scene
// from its badge, or for the whole cut from the gutter (S2), and what watching a row does to the preview (S3).
// Every rule is a plain function in naivepost::cut_cam / naivepost::cut_hear, so nothing here needs a widget or
// a display. The two recordings are laid out like spec/img/05-rows.png: one camera's file at 0–37 and a second
// overlapping it from 20, which is what puts them on two rows.

use std::collections::BTreeMap;

use naivepost::cut::{Cut, Seg};
use naivepost::cut_cam as cam;
use naivepost::cut_hear as hear;
use naivepost::timeline::{rows_for, Recording};

/// The first camera: 0–37, the figure's "camera 1 (17-25-06, 0:00–0:37)".
const CAM_ONE: &str = "17-25-06";
/// The second camera, whose shift correction the name plate carries (**2**).
const CAM_TWO: &str = "17-25-45";

/// The two recordings of the figure, overlapping from 20 — **3**, "both cameras filmed it".
fn pair() -> Vec<Recording> {
    vec![
        Recording { base: CAM_ONE.to_string(), start: 0.0, end: 37.0 },
        Recording { base: CAM_TWO.to_string(), start: 20.0, end: 37.0 },
    ]
}

/// Kept footage taken from `row`.
fn film(s: f64, e: f64, row: i32) -> Seg {
    Seg { s, e, cam: row, ..Default::default() }
}

/// A card at the line: it owns no footage and answers to no row.
fn card(at: f64) -> Seg {
    Seg { s: at, e: at, ins: "assets/sting.mp4".into(), dur: 3.0, ..Default::default() }
}

/// The rows the figure's pair gets colouring-free: greedy colouring in start order puts the overlap lower.
fn two_rows(cut: &Cut) -> Vec<usize> {
    rows_for(&pair(), cut)
}

// --- S1: which row a scene's picture comes from -------------------------------------------------------------

/// F2.10 (Cameras and hearing) S1 — `🔍 lens badge: which row its picture comes from`: a scene drawn across the
/// rows lights exactly one badge, the row named by its `cam`.
#[test]
fn f2_10_s1_the_lens_badge_lights_on_the_row_a_scene_is_shown_from() {
    let scene = film(6.0, 12.0, 1);
    assert_eq!(cam::scene_row(&scene), 1, "the badge reads the scene's own row");
    assert!(cam::shown_from(cam::scene_row(&scene), 1), "lit on the row it is shown from");
    assert!(!cam::shown_from(cam::scene_row(&scene), 0), "dark on the row above");
    assert!(!cam::shown_from(cam::scene_row(&scene), 2), "and dark on the row below");
}

/// F2.10 (Cameras and hearing) S1 — `the scene at m:ss is shown from <cam> now`: picking a row takes the scene's
/// picture there, and the page says which camera that was by name rather than by number.
#[test]
fn f2_10_s1_picking_a_row_says_which_camera_the_scene_uses() {
    let mut cut = Cut::default();
    cut.segs.push(film(6.0, 12.0, 0));
    let rows = two_rows(&cut);

    let said = cam::show_scene_from(&mut cut, 0, 1, &pair(), &rows).expect("a kept scene has a picture to move");
    assert_eq!(said, "the scene at 0:06 is shown from 17-25-45 now", "the spec's own sentence, m:ss unpadded");

    // And the badges follow what was written: one row lit, the row it left dark.
    let scene = &cut.segs[0];
    assert_eq!(cam::scene_row(scene), 1);
    assert!(cam::shown_from(cam::scene_row(scene), 1));
    assert!(!cam::shown_from(cam::scene_row(scene), 0));
}

/// F2.10 (Cameras and hearing) S1 — what a status calls a row: the recording lying on it, or its number counting
/// from one. A row can be empty (an emptied bottom row survives until its ✕), and "the cut shows  here" is not
/// a sentence, so the fallback names the row instead of saying nothing.
#[test]
fn f2_10_s1_a_row_with_no_recording_is_called_its_number() {
    let rows = two_rows(&Cut::default());
    assert_eq!(cam::row_name(0, &pair(), &rows), CAM_ONE, "the recording on the row speaks for it");
    assert_eq!(cam::row_name(1, &pair(), &rows), CAM_TWO);

    // A third row nothing was laid on — the ✕ has not been pressed yet.
    assert_eq!(cam::row_name(2, &pair(), &rows), "row 3", "one-based, because that is how the page numbers rows");
    assert_eq!(cam::row_name(0, &[], &[]), "row 1", "and a page with no recordings at all still has a row 1");
}

/// F2.10 (Cameras and hearing) S1 — an insert has no picture of its own to be shown from a row: asking it which
/// camera it uses has no answer, so nothing is written and nothing is claimed.
#[test]
fn f2_10_s1_an_insert_has_no_picture_to_show_from_a_row() {
    let mut cut = Cut::default();
    cut.segs.push(card(40.0));
    let before = cut.segs[0].cam;
    let rows = two_rows(&cut);

    assert_eq!(cam::show_scene_from(&mut cut, 0, 1, &pair(), &rows), None, "a card answers to no row");
    assert_eq!(cut.segs[0].cam, before, "and its cam stays as it was rather than becoming a number nothing reads");

    // A press aimed at a scene the cut does not hold changes nothing either.
    assert_eq!(cam::show_scene_from(&mut cut, 9, 1, &pair(), &rows), None);
}

/// F2.10 (Cameras and hearing) S1 — **2** in the figure: "camera 2 (17-25-45) with its shift correction
/// −19.00 s in the name plate". The shift is spelled out because a camera moved by hand looks exactly like one
/// whose file says it started there, and ` from <m:ss>` is the only thing saying which part of a file a lane row
/// is showing.
#[test]
fn f2_10_s1_the_name_plate_shows_a_shift_correction() {
    let plate = cam::name_plate(CAM_TWO, -19.0, 0.0);
    assert!(plate.starts_with(CAM_TWO), "the plate speaks for the recording: {plate}");
    assert!(plate.ends_with("-19.00 s"), "signed and to two decimals, so it reads as a correction: {plate}");

    // No correction, no mention: an untouched camera's plate is its name and nothing else.
    assert_eq!(cam::name_plate(CAM_ONE, 0.0, 0.0), CAM_ONE);
    // A lane cut from a recording says which part of it the row shows.
    assert_eq!(cam::name_plate(CAM_TWO, 0.0, 60.0), "17-25-45 from 1:00");
    // Both at once, in the order the page writes them.
    assert_eq!(cam::name_plate(CAM_TWO, 2.5, 60.0), "17-25-45 from 1:00 +2.50 s");
}

/// F2.10 (Cameras and hearing) — the rewrite's own rule from the Prototype paragraph: a row-drag pins ONLY the
/// recording dragged, so an overlap made by hand gets a row of its own instead of being drawn under its
/// neighbour, whose overlapped seconds could then be neither seen nor picked.
#[test]
fn f2_10_s1_a_drag_pins_only_what_it_dragged() {
    let mut before = BTreeMap::new();
    before.insert("older".to_string(), 3);

    let pinned = cam::pin_for_drag(CAM_TWO, 0, &before);
    assert_eq!(pinned.get(CAM_TWO), Some(&0), "the dragged recording is pinned where it was dropped");
    assert_eq!(pinned.get("older"), Some(&3), "every other pin is carried over untouched");
    assert_eq!(pinned.len(), 2, "and a drag adds no pin for anything else — the prototype pinned the whole row");

    // (a) A drag that creates an overlap: the second recording is dropped onto the first's row. Pinning only
    // what was dragged lets the colouring re-run, and the new overlap gets a row of its own — which is what the
    // same overlap gets when it comes from the file names instead.
    let mut cut = Cut::default();
    cut.rows = cam::pin_for_drag(CAM_TWO, 0, &BTreeMap::new());
    let rows = rows_for(&pair(), &cut);
    assert_eq!(rows[1], 0, "the recording dragged is pinned to the row it was dropped on");
    assert_ne!(rows[0], rows[1], "and the overlap made by the drag gets a row of its own: {rows:?}");

    // (b) A pin read from the file is still taken LITERAL — the editor moved that recording knowing what was
    // under it — even when it overlaps what already sits there.
    let mut literal = BTreeMap::new();
    literal.insert(CAM_ONE.to_string(), 1);
    literal.insert(CAM_TWO.to_string(), 1);
    let mut cut = Cut::default();
    cut.rows = literal;
    let rows = rows_for(&pair(), &cut);
    assert_eq!(rows, vec![1, 1], "both stay on the row the file named them, overlap and all");
}

// --- S2: what a scene hears -------------------------------------------------------------------------------

/// F2.10 (Cameras and hearing) S2 — `🔈 speaker badge per lane`: one press silences one lane in one scene, and
/// the status is F2.5's own sentence because both flows say the same thing.
#[test]
fn f2_10_s2_a_speaker_badge_silences_one_lane_in_one_scene() {
    let mut cut = Cut::default();
    cut.segs.push(film(40.0, 50.0, 0));
    cut.segs.push(film(60.0, 70.0, 0));

    assert_eq!(hear::toggle_heard(&mut cut, 0, CAM_TWO), Some(format!("{CAM_TWO} is silent in the scene at 0:40")));

    let scene = &cut.segs[0];
    assert!(!scene.hears(CAM_TWO), "the scene no longer hears that lane");
    assert_eq!(hear::hush(scene), [CAM_TWO.to_string()], "and its quiet list is what says so");
    assert!(scene.hears(CAM_ONE), "another lane keeps its say");
    // The other scene was not pressed and hears everything, as it did.
    assert!(cut.segs[1].hears(CAM_TWO), "a badge speaks for one scene only");
}

/// F2.10 (Cameras and hearing) S2 — the same press again turns the lane back on and says so in the same
/// sentence's other half, rather than leaving a silence with no way out of it.
#[test]
fn f2_10_s2_the_second_press_says_it_is_heard_again() {
    let mut cut = Cut::default();
    cut.segs.push(film(40.0, 50.0, 0));

    hear::toggle_heard(&mut cut, 0, CAM_TWO);
    assert_eq!(hear::toggle_heard(&mut cut, 0, CAM_TWO), Some(format!("{CAM_TWO} is heard in the scene at 0:40")));

    let scene = &cut.segs[0];
    assert!(scene.hears(CAM_TWO), "heard again");
    assert!(scene.quiet.is_empty(), "and nothing left behind claiming otherwise");
}

/// F2.10 (Cameras and hearing) S2 — `quiet` is a SET, compared as a set and written fresh per press
/// (`spec/10-parameters.md` §5 rule 10): re-silencing a lane that was already listed cannot duplicate it, and
/// un-listing one leaves no hole.
#[test]
fn f2_10_s2_quiet_is_a_set_written_fresh_each_press() {
    let mut cut = Cut::default();
    cut.segs.push(film(40.0, 50.0, 0));
    // A file that listed the same lane twice, which is exactly what a set must survive.
    cut.segs[0].quiet = vec![CAM_TWO.to_string(), CAM_TWO.to_string()];

    // Pressing it as though silencing: heard is false, so the press brings it back — once, and entirely.
    hear::toggle_heard(&mut cut, 0, CAM_TWO);
    assert_eq!(cut.segs[0].quiet, Vec::<String>::new(), "un-listing leaves no hole and no duplicate");

    // Two lanes silenced in order: the list holds each once, whatever order they arrived in.
    hear::toggle_heard(&mut cut, 0, CAM_TWO);
    hear::toggle_heard(&mut cut, 0, CAM_ONE);
    let mut quiet = cut.segs[0].quiet.clone();
    quiet.sort();
    let mut want = vec![CAM_ONE.to_string(), CAM_TWO.to_string()];
    want.sort();
    assert_eq!(quiet, want, "compared as a set, never by position");

    hear::toggle_heard(&mut cut, 0, CAM_TWO);
    quiet = cut.segs[0].quiet.clone();
    quiet.sort();
    assert_eq!(quiet, vec![CAM_ONE.to_string()], "and the lane that was not pressed is still silent");
}

/// F2.10 (Cameras and hearing) S2 — a row's gutter switch stands for EVERY recording on that row, so it reads as
/// off only when none of them is heard anywhere: a half-silenced row still reads as heard, and pressing it
/// finishes the job rather than flipping anything back on.
#[test]
fn f2_10_s2_a_row_switch_stands_for_every_recording_on_its_row() {
    let mut cut = Cut::default();
    cut.segs.push(film(40.0, 50.0, 0));
    cut.segs.push(film(60.0, 70.0, 0));
    let row = [CAM_ONE, CAM_TWO];

    assert!(hear::lane_is_heard_anywhere(&cut, &row), "everything heard, so the switch is on");

    // Silence one of the two in one scene: the row is still heard — here and everywhere else.
    hear::toggle_heard(&mut cut, 0, CAM_ONE);
    assert!(hear::lane_is_heard_anywhere(&cut, &row), "half-silenced is still heard");

    // And the switch finishes it: both lanes silent in every scene that could hear them.
    let said = hear::toggle_lane_all(&mut cut, &row, CAM_ONE);
    assert!(said.contains("off for the whole cut"), "{said}");
    assert!(!hear::lane_is_heard_anywhere(&cut, &row), "the row is off now, in every scene");
    for seg in &cut.segs {
        assert!(!seg.hears(CAM_ONE) && !seg.hears(CAM_TWO), "{seg:?} still hears part of the row");
    }

    // A recording outside the row is heard by exactly the same rule — nothing lists it — so a switch about it
    // would have its own work to do and reads as ON, not as this row's silence.
    assert!(hear::lane_is_heard_anywhere(&cut, &["18-01-01 09-00-00"]));
}

/// F2.10 (Cameras and hearing) S2 — `the gutter switch toggles a lane for the whole cut`: both directions name
/// how many scenes moved, and the count is what the cut actually shows afterwards.
#[test]
fn f2_10_s2_the_gutter_switch_speaks_for_the_whole_cut() {
    let mut cut = Cut::default();
    cut.segs.push(film(40.0, 50.0, 0));
    cut.segs.push(film(60.0, 70.0, 0));

    assert_eq!(hear::toggle_lane_all(&mut cut, &[CAM_TWO], CAM_TWO), format!("{CAM_TWO} off for the whole cut \u{2014} 2 scene(s) changed"));
    for seg in &cut.segs {
        assert!(!seg.hears(CAM_TWO), "{seg:?} was left hearing it");
    }

    assert_eq!(hear::toggle_lane_all(&mut cut, &[CAM_TWO], CAM_TWO), format!("{CAM_TWO} is on for the whole cut \u{2014} every scene hears it (2 changed)"));
    for seg in &cut.segs {
        assert!(seg.hears(CAM_TWO), "{seg:?} was left silent");
    }

    // Only the row's own lanes move: another camera keeps its say, so a switch never silences the page.
    let mut cut = Cut::default();
    cut.segs.push(film(40.0, 50.0, 0));
    hear::toggle_lane_all(&mut cut, &[CAM_TWO], CAM_TWO);
    assert!(cut.segs[0].hears(CAM_ONE), "the first camera was never in question");
}

/// F2.10 (Cameras and hearing) S2 — a cut with nothing in it has no scene to answer for a lane, and the page
/// says so instead of reporting a change it did not make. The prototype's own guard is `len(ed.segs) == 0`
/// (gui/cut_hear.go:258): a recording nobody has cut yet is exactly what "cut something first" means.
#[test]
fn f2_10_s2_a_lane_in_no_scene_says_so_and_changes_nothing() {
    let mut empty = Cut::default();
    assert_eq!(hear::toggle_lane_all(&mut empty, &[CAM_TWO], CAM_TWO), format!("{CAM_TWO} is in no scene yet \u{2014} cut something first"));
    assert!(empty.segs.is_empty(), "and the refusal invented no scene to hear it with");

    // A recording no scene was cut with is not silent anywhere either: `quiet` names what a scene does NOT hear,
    // so a lane nobody lists is heard — which is the whole of F2.5 S1's rule, and why silencing a lane means
    // WRITING it into every scene rather than assuming the page already knows.
    let mut cut = Cut::default();
    cut.segs.push(film(40.0, 50.0, 0));
    let stranger = "18-01-01 09-00-00";
    assert!(hear::lane_is_heard_anywhere(&cut, &[stranger]), "no scene says otherwise about it");
    assert!(!hear::all_silent(&cut, &[stranger]), "and silence is only ever what was written down");

    // So its switch reads ON, and pressing it does the work: every kept scene lists it, counted honestly.
    let before: Vec<Vec<String>> = cut.segs.iter().map(|seg| seg.quiet.clone()).collect();
    assert!(before.iter().all(Vec::is_empty), "no scene named anything yet");
    assert_eq!(hear::toggle_lane_all(&mut cut, &[stranger], stranger), format!("{stranger} off for the whole cut \u{2014} 1 scene(s) changed"));
    assert!(!cut.segs[0].hears(stranger), "and now the scene says so");
}

/// F2.10 (Cameras and hearing) S2 — an insert's hearing is F2.5 S1's rule and neither control moves it: `hush`
/// reads a card as hearing everything, so writing a list there would silence nothing while looking as if it did.
#[test]
fn f2_10_s2_an_inserts_hearing_is_not_the_switches_business() {
    let mut cut = Cut::default();
    cut.segs.push(film(40.0, 50.0, 0));
    cut.segs.push(card(60.0));

    // The badge on the card reports what is already true and writes nothing.
    let said = hear::toggle_heard(&mut cut, 1, CAM_TWO).expect("the scene index exists");
    assert_eq!(said, format!("{CAM_TWO} is heard in the scene at 1:00"), "no change, so no silence claimed");
    assert!(cut.segs[1].quiet.is_empty(), "and the card's quiet list stayed empty");

    // The gutter switch moves the kept scene and leaves the card out of the count it reports.
    let off = hear::toggle_lane_all(&mut cut, &[CAM_TWO], CAM_TWO);
    assert_eq!(off, format!("{CAM_TWO} off for the whole cut \u{2014} 1 scene(s) changed"), "one kept scene, not two");
    assert!(cut.segs[1].quiet.is_empty(), "a card was never asked which lanes it hears");
    assert!(!cut.segs[0].hears(CAM_TWO), "the footage did move");
}

// --- S3: watching a row -----------------------------------------------------------------------------------

/// F2.10 (Cameras and hearing) S3 — `A click on a row watches it in the preview until ▶ takes the preview back
/// to the cut`: **5** is the watched row, not the cut's, and ▶ hands it back exactly once.
#[test]
fn f2_10_s3_a_click_watches_the_row_and_the_preview_follows() {
    let mut watch = cam::Watch::default();
    assert_eq!(watch.preview_shows(0), 0, "with nothing watched the preview shows the cut");

    cam::click_row(&mut watch, 1, &[], None);
    assert_eq!(watch.row, Some(1), "a click always watches, whatever else it says");
    assert_eq!(watch.preview_shows(0), 1, "**5** the preview shows the watched row, not the cut");

    assert!(watch.play_hands_back(), "▶ took the preview back to the cut");
    assert_eq!(watch.row, None);
    assert_eq!(watch.preview_shows(0), 0, "and the cut is what the preview shows again");
    assert!(!watch.play_hands_back(), "with nothing watched there was nothing to hand back");

    // Watching the row the cut already shows is still a watch — it just has nothing to explain.
    cam::click_row(&mut watch, 0, &[film(6.0, 12.0, 0)], Some(8.0));
    assert_eq!(watch.preview_shows(2), 0);
}

/// F2.10 (Cameras and hearing) S3 — **7** `watching camera 2 — the cut shows camera 1 here; ▶ plays the cut`:
/// said when the line stands in a kept scene whose picture comes from another row, counting rows the way the
/// page numbers them.
#[test]
fn f2_10_s3_the_status_says_what_the_cut_shows_there() {
    let segs = [film(6.0, 12.0, 0)];
    let mut watch = cam::Watch::default();

    assert_eq!(
        cam::click_row(&mut watch, 1, &segs, Some(8.0)),
        Some("watching camera 2 \u{2014} the cut shows camera 1 here; \u{25b6} plays the cut".to_string())
    );
}

/// F2.10 (Cameras and hearing) S3 — nothing to explain when there is no surprise: clicking the row the line's
/// scene is already shown from, and clicking with the line over no kept scene at all, both stay silent. The
/// watch is set either way, because a click always means "show me this row".
#[test]
fn f2_10_s3_the_same_camera_is_said_without_a_word() {
    let segs = [film(6.0, 12.0, 1)];

    let mut same = cam::Watch::default();
    assert_eq!(cam::click_row(&mut same, 1, &segs, Some(8.0)), None, "the row already shown needs no word");
    assert_eq!(same.row, Some(1), "and it is still the row being watched");

    // The line over a gap: no kept scene, so nothing about this second contradicts the click.
    let mut gap = cam::Watch::default();
    assert_eq!(cam::click_row(&mut gap, 0, &segs, Some(30.0)), None);
    assert_eq!(gap.row, Some(0));

    // No line yet — the page has never been clicked on the timeline (F2.4 owns where the line comes from).
    let mut idle = cam::Watch::default();
    assert_eq!(cam::click_row(&mut idle, 1, &segs, None), None);
    assert_eq!(idle.row, Some(1));
}

/// F2.10 (Cameras and hearing) S3 — `status says ONCE`: the line moves and the page redraws, and a status the
/// same click re-answers on every frame drowns everything else the page has to say. A fresh click arms it again.
#[test]
fn f2_10_s3_the_word_is_said_once() {
    let segs = [film(6.0, 12.0, 0)];
    let mut watch = cam::Watch::default();

    assert!(cam::click_row(&mut watch, 1, &segs, Some(8.0)).is_some(), "said the first time");
    assert!(watch.said, "and marked as said");
    assert_eq!(cam::watch_status(&mut watch, &segs, Some(9.0)), None, "the line moved; the word was already spent");
    assert_eq!(cam::watch_status(&mut watch, &segs, Some(10.0)), None, "again");

    // A second click on the same row is a fresh question, and gets a fresh answer.
    assert!(cam::click_row(&mut watch, 1, &segs, Some(8.0)).is_some(), "a new click says it again");
    // ▶ disarms it too: with nothing watched there is no sentence to keep repeating.
    watch.play_hands_back();
    assert_eq!(cam::watch_status(&mut watch, &segs, Some(8.0)), None);
}

/// F2.10 (Cameras and hearing) S3 — a card at the line owns no kept scene and answers to no row, so watching a
/// row over one contradicts nothing and says nothing.
#[test]
fn f2_10_s3_a_card_at_the_line_never_says_the_cut_shows_another_camera() {
    let segs = [card(40.0)];
    let mut watch = cam::Watch::default();

    assert_eq!(cam::click_row(&mut watch, 1, &segs, Some(40.0)), None, "a card is not a kept scene shown from elsewhere");
    assert_eq!(watch.row, Some(1), "the row is watched all the same");

    // A kept scene further away still contradicts nothing at this second: only where the line stands counts.
    let both = [film(6.0, 12.0, 0), card(40.0)];
    let mut watch = cam::Watch::default();
    assert_eq!(cam::click_row(&mut watch, 1, &both, Some(40.0)), None);
}
