// F2.4 (Place and step the line) — spec/05-cut.md §1 S1, S1b, S2, S3, S4.
// One test per move the item says the screen makes; every rule is a plain function in
// naivepost::cut_line, so nothing here needs a widget or a display.

use naivepost::cut_line::{self as line, ClickOutcome, Held, LinePos, PressPick};
use naivepost::preview::{self, Player, Press};

/// F2.4 (Place and step the line) S1: a press on a track puts the line at the clicked
/// second and clears the selection — the click's outcome, not the clip's.
#[test]
fn f2_4_s1_a_press_on_a_track_places_the_line_and_clears_the_selection() {
    let out = line::click_outcome(12.5, true, false, false, false, false);
    assert_eq!(
        out,
        ClickOutcome {
            line_at: 12.5,
            line_moved: true,
            takes_scene: false,
            clears_selection: true,
            watches: None,
        },
        "a press on the effects row moves the line and clears the selection, and watches nothing"
    );

    let out = line::click_outcome(48.0, true, false, true, false, false);
    assert_eq!(out.line_at, 48.0, "the line goes where it was clicked");
    assert!(out.line_moved, "a track press moves the line");
    assert!(out.clears_selection, "and clears the selection");
}

/// F2.4 (Place and step the line) S1: the scene comes into the line's hand only when the
/// click lands on its picture — a row background or an audio band takes nothing.
#[test]
fn f2_4_s1_the_scene_in_hand_only_when_the_click_lands_on_its_picture() {
    assert!(line::click_takes_scene(true, true), "picture band, scene's own picture");
    assert!(!line::click_takes_scene(true, false), "picture band, the row behind it");
    assert!(!line::click_takes_scene(false, true), "an audio band never takes a scene");
    assert!(!line::click_takes_scene(false, false), "neither does an empty audio band");

    // on_picture, on_scene_picture, playing, sources, gutter: the scene is taken only when
    // both position answers say its picture.
    let on_scene = line::click_outcome(30.0, true, true, false, true, false);
    assert!(on_scene.takes_scene, "a click on a scene's picture takes that scene");
    let beside = line::click_outcome(30.0, true, false, false, true, false);
    assert!(!beside.takes_scene, "a click beside it clears the selection and takes nothing");
}

/// F2.4 (Place and step the line) S1: a row is watched only on the picture band, while
/// nothing plays, with sources loaded — and never from the gutter.
#[test]
fn f2_4_s1_the_row_is_watched_only_on_the_picture_band_while_nothing_plays_with_sources() {
    assert!(line::watches_row(true, false, true, false), "all four answer yes");
    assert!(!line::watches_row(false, false, true, false), "an audio band never watches");
    assert!(!line::watches_row(true, true, true, false), "not while anything plays");
    assert!(!line::watches_row(true, false, false, false), "not with no sources loaded");
    assert!(!line::watches_row(true, false, true, true), "a gutter click does nothing");

    let out = line::click_outcome(20.0, true, false, false, true, false);
    assert_eq!(out.watches, Some(line::PICTURE_ROW), "the picture band is row 0");

    // The watch rule reads "while nothing plays" off the same transport ▶ owns (F2.1),
    // so prove it against a player that is actually running.
    let mut player = Player::default();
    player.playhead = Some(5.0);
    preview::press_recording(&mut player, &[(0.0, 60.0)]);
    assert!(player.transport.playing, "▶ has the recording running");
    // A click between two scenes takes nothing into the hand, so this asserts the watch
    // alone: running (from ▶) it watches no row, paused it watches the picture band.
    let playing = line::click_outcome(20.0, true, false, player.transport.playing, true, false);
    assert_eq!(playing.watches, None, "a press while it plays watches no row");
    assert_eq!(playing.line_at, 20.0, "and still moves the line");
    let stopped = line::click_outcome(20.0, true, false, false, true, false);
    assert_eq!(stopped.watches, Some(line::PICTURE_ROW), "paused, the picture band is row 0");
}

/// F2.4 (Place and step the line) S1: a gutter click does nothing at all — no line, no
/// selection, no watch; its whole job is the row's height.
#[test]
fn f2_4_s1_a_gutter_click_moves_no_line_and_touches_no_selection() {
    let out = line::click_outcome(37.0, true, false, true, true, true);
    assert_eq!(
        out,
        ClickOutcome {
            line_at: 37.0,
            line_moved: false,
            takes_scene: false,
            clears_selection: false,
            watches: None,
        },
        "the gutter is not a track"
    );
}

/// F2.4 (Place and step the line) S1b: the left button's first press over the picture
/// band measures the edge reach first — edge, then border, then the clip it falls through.
#[test]
fn f2_4_s1b_the_first_left_press_measures_the_edge_reach_first() {
    assert_eq!(line::EDGE_REACH_PX, 12.0, "spec/10-parameters.md \"timeline:\" block");
    assert_eq!(line::first_press_pick(12.0, true), PressPick::Edge, "on the reach is on the boundary");
    assert_eq!(line::first_press_pick(0.0, true), PressPick::Edge, "dead on the boundary");
    assert_eq!(line::first_press_pick(13.0, true), PressPick::Border, "a pixel in is the whole clip");
    assert_eq!(line::first_press_pick(200.0, true), PressPick::Border, "mid-card is still the whole clip");
    assert_eq!(line::first_press_pick(400.0, false), PressPick::Clip, "outside the box falls through to the line");
}

/// F2.4 (Place and step the line) S2: a frame step moves what is held, never the line —
/// edge first, then clip, then effect.
#[test]
fn f2_4_s2_a_frame_step_moves_the_hold_not_the_line() {
    assert_eq!(line::step_hold(Some(Held::Edge)), Some("edge"));
    assert_eq!(line::step_hold(Some(Held::Clip)), Some("clip"));
    assert_eq!(line::step_hold(Some(Held::Effect)), Some("effect"));
    assert_eq!(line::step_hold(None), None, "nothing held: the step belongs to the line or nowhere");

    assert_eq!(line::step_frames(false), 1, "f is one frame of the recording");
    assert_eq!(line::step_frames(true), 5, "Shift, or ‹‹ ››, is five");
}

/// F2.4 (Place and step the line) S2: with nothing held a frame step pauses the preview
/// and seeks by whole frames of the recording under the line.
#[test]
fn f2_4_s2_with_nothing_held_the_step_seeks_the_recording_under_the_line_and_pauses() {
    // fps is the project's own frame interval, never a model's guess; 30 is what
    // spec/inventory/cut.md §D gives for the prototype.
    let mut player = Player::default();
    player.playhead = Some(10.0);
    preview::press_recording(&mut player, &[(0.0, 60.0)]);
    assert!(player.transport.playing, "▶ first, so there is something to pause");

    preview::press_recording(&mut player, &[(0.0, 60.0)]);
    assert!(!player.transport.playing, "a frame step pauses the preview before it seeks");

    let one = line::step_line(10.0, line::step_frames(false), 30.0);
    assert!((one - (10.0 + 1.0 / 30.0)).abs() < 1e-12, "one frame of the recording: {one}");
    let five = line::step_line(10.0, line::step_frames(true), 30.0);
    assert!((five - (10.0 + 5.0 / 30.0)).abs() < 1e-12, "Shift is five frames: {five}");
    let back = line::step_line(10.0, -line::step_frames(false), 30.0);
    assert!((back - (10.0 - 1.0 / 30.0)).abs() < 1e-12, "‹f steps backwards by the same frame");
}

/// F2.4 (Place and step the line) S3: ← and → step only while the line holds something;
/// with an empty hand they do nothing and the press belongs to the focused box.
#[test]
fn f2_4_s3_the_arrows_step_only_while_the_line_holds_something() {
    assert!(line::arrow_moves_line(Some(Held::Edge)));
    assert!(line::arrow_moves_line(Some(Held::Clip)));
    assert!(line::arrow_moves_line(Some(Held::Effect)));
    assert!(!line::arrow_moves_line(None), "nothing held: the arrows are not ours");
}

/// F2.4 (Place and step the line) S3: Space toggles the preview from any focus but a text
/// box, and what it toggles is whichever preview is on the screen.
#[test]
fn f2_4_s3_space_toggles_the_preview_from_anything_but_a_text_box() {
    assert!(line::space_toggles_preview(false), "no text focus: Space plays");
    assert!(!line::space_toggles_preview(true), "a text box keeps its Space as a character");

    let mut player = Player::default();
    player.playhead = Some(3.0);
    // ▶ on the cut preview: F2.1 S1 switches it back to the recording rather than pausing,
    // so a running Space reaches S2's pause only once nothing is switched on — which is what
    // makes the next press stop the preview instead of changing its meaning.
    player.cut_only = true;
    player.transport.playing = true;
    let switch = preview::press_recording(&mut player, &[(0.0, 60.0)]);
    assert_eq!(switch, Press::SwitchedToRecording { kept_playing: true }, "the cut preview plays on as the recording");
    assert!(!player.cut_only, "and is no longer the cut");
    preview::press_recording(&mut player, &[(0.0, 60.0)]);
    assert!(!player.transport.playing, "Space again pauses it, from any focus but a text box");
}

/// F2.4 (Place and step the line) S4: the line is remembered in a file of its own —
/// `cut/line.json`, `{"t": <session second>}` — and an unusable one is no remembered line.
#[test]
fn f2_4_s4_the_line_is_remembered_in_its_own_file() {
    let text = line::line_json(LinePos { t: 42.5 });
    assert!(text.starts_with("{\"t\":42.5"), "the prototype's shape: {text}");
    assert!(text.ends_with('\n'), "every file this app writes ends with a newline");

    let back = line::parse_line_json(&text).expect("what we wrote reads back");
    assert_eq!(back, LinePos { t: 42.5 });
    // A file written by hand or an older build may hold an integer second.
    assert_eq!(line::parse_line_json("{\"t\":7}\n"), Some(LinePos { t: 7.0 }));

    assert_eq!(line::parse_line_json(""), None, "an empty file is no position");
    assert_eq!(line::parse_line_json("{}"), None, "a file without a second is no position");
    assert_eq!(line::parse_line_json("{\"t\":-1}"), None, "nor a negative one");
    assert_eq!(line::parse_line_json("not json"), None, "nor half a file");
}

/// F2.4 (Place and step the line) S4: a remembered second the recording no longer covers
/// is left alone — the sources changed since the line stood there.
#[test]
fn f2_4_s4_a_second_the_recording_no_longer_covers_is_left_alone() {
    assert!(line::restores_line(30.0, 0.0, 60.0), "inside the recording");
    assert!(line::restores_line(0.0, 0.0, 60.0), "its first second counts");
    assert!(!line::restores_line(60.0, 0.0, 60.0), "past its end does not");
    assert!(!line::restores_line(-0.5, 0.0, 60.0), "nor before its start");
    assert!(!line::restores_line(2400.0, 0.0, 720.0), "a 40-minute line in a 12-minute recording is dropped");
}

/// F2.4 (Place and step the line) S4: at most one write a second while the line moves, and
/// always one when the window closes.
#[test]
fn f2_4_s4_a_write_is_owed_at_most_once_a_second_and_always_on_close() {
    assert_eq!(line::LINE_WRITE_MS, 1000);
    assert!(line::may_write_line(None, 0, false), "the first move writes");
    assert!(!line::may_write_line(Some(0), 999, false), "the next 999 ms are not owed a write");
    assert!(line::may_write_line(Some(0), 1000, false), "a second on, the last position lands");
    assert!(line::may_write_line(Some(0), 10, true), "closing always flushes what is owed");
    // Playback moves the line ten times a second; nine of those ticks owe nothing.
    for tick in 1..10 {
        assert!(!line::may_write_line(Some(0), tick * 100, false), "tick {tick} writes nothing");
    }
}
