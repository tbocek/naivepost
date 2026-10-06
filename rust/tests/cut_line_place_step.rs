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

// --- S4: the file on disk (persistence half) -----------------------------------------------------

/// A throwaway project root per test, so nothing collides and nothing outlives the test.
fn temp_root(tag: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("naivepost-line-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("temp root");
    root
}

/// F2.4 S4: writing the line puts it at `cut/line.json` under the project root, byte for byte as
/// `line_json` formats it, and reading gives back exactly what was written.
#[test]
fn f2_4_s4_the_line_round_trips_through_cut_line_json() {
    let root = temp_root("roundtrip");
    let pos = line::LinePos { t: 123.5 };

    line::write_line(&root, pos).expect("the write lands");

    let file = root.join("cut").join("line.json");
    assert!(file.is_file(), "the line lives at cut/line.json, not somewhere else");
    assert_eq!(
        std::fs::read_to_string(&file).expect("readable"),
        line::line_json(pos),
        "the bytes on disk are exactly what line_json writes"
    );
    assert_eq!(line::read_line(&root), Some(pos), "and read back identically");

    std::fs::remove_dir_all(&root).ok();
}

/// F2.4 S4 error paths: no file at all, and a file full of nonsense, both mean "nothing to restore"
/// rather than an error — a project with no line yet is the normal case.
#[test]
fn f2_4_s4_a_missing_or_corrupt_line_file_reads_as_nothing() {
    let empty = temp_root("missing");
    assert_eq!(line::read_line(&empty), None, "no file is not an error, it is no position");

    let corrupt = temp_root("corrupt");
    std::fs::create_dir_all(corrupt.join("cut")).unwrap();
    std::fs::write(corrupt.join("cut").join("line.json"), b"not json at all").unwrap();
    assert_eq!(line::read_line(&corrupt), None, "bad bytes parse to nothing, same answer");

    // ...and restore agrees: nothing comes back when there is nothing saved.
    assert_eq!(line::restore(&corrupt, &[(0.0, 600.0)]), None);

    std::fs::remove_dir_all(&empty).ok();
    std::fs::remove_dir_all(&corrupt).ok();
}

/// F2.4 S4: while the line moves, at most one write a second (line::LINE_WRITE_MS = 1000). The file
/// keeps the last WRITTEN position through the throttled ticks, then takes the new one a second on.
#[test]
fn f2_4_s4_the_writer_throttles_moves_to_one_write_a_second() {
    let root = temp_root("throttle");
    let mut writer = line::LineWriter::default();

    assert!(writer.note_move(line::LinePos { t: 10.0 }, &root, 0), "the first move writes");
    assert_eq!(line::read_line(&root), Some(line::LinePos { t: 10.0 }));

    // Ten ticks over the next 900 ms: none of them reach the disk.
    for tick in 1..10 {
        assert!(
            !writer.note_move(line::LinePos { t: 10.0 + tick as f64 }, &root, tick * 100),
            "tick {tick} is inside the second and writes nothing"
        );
    }
    assert_eq!(
        line::read_line(&root),
        Some(line::LinePos { t: 10.0 }),
        "the file still holds the first position through the throttled ticks"
    );

    assert!(
        writer.note_move(line::LinePos { t: 11.0 }, &root, 1000),
        "a second on, the latest position lands"
    );
    assert_eq!(line::read_line(&root), Some(line::LinePos { t: 11.0 }));
    assert_eq!(writer.last_write_ms(), Some(1000), "and the writer remembers when it wrote");

    std::fs::remove_dir_all(&root).ok();
}

/// F2.4 S4: closing flushes whatever is owed, immediately, even inside the throttled second.
#[test]
fn f2_4_s4_flushing_on_close_beats_the_rate_limit() {
    let root = temp_root("flush");
    let mut writer = line::LineWriter::default();

    writer.note_move(line::LinePos { t: 42.0 }, &root, 0);
    assert!(
        writer.flush(line::LinePos { t: 42.7 }, &root, 50),
        "50 ms after a write is far inside the second, and close still writes"
    );
    assert_eq!(
        line::read_line(&root),
        Some(line::LinePos { t: 42.7 }),
        "so the position closed on is the position on disk"
    );

    std::fs::remove_dir_all(&root).ok();
}

/// F2.4 S4: restoring keeps a line a recording still covers and drops one that sits in a gap — a cue to
/// a second nobody filmed would leave the page showing nothing.
#[test]
fn f2_4_s4_restore_keeps_only_a_line_a_recording_still_covers() {
    let covered = temp_root("covered");
    line::write_line(&covered, line::LinePos { t: 30.0 }).unwrap();
    assert_eq!(
        line::restore(&covered, &[(0.0, 100.0), (200.0, 300.0)]),
        Some(line::LinePos { t: 30.0 }),
        "a recording covers second 30, so the line comes back"
    );

    let stranded = temp_root("stranded");
    line::write_line(&stranded, line::LinePos { t: 150.0 }).unwrap();
    assert_eq!(
        line::restore(&stranded, &[(0.0, 100.0), (200.0, 300.0)]),
        None,
        "second 150 lies in the gap between recordings, so nothing is restored"
    );

    // The end of a span is exclusive, matching restores_line: standing ON the end is past it.
    let edge = temp_root("edge");
    line::write_line(&edge, line::LinePos { t: 100.0 }).unwrap();
    assert_eq!(line::restore(&edge, &[(0.0, 100.0)]), None, "t == end is not covered");

    std::fs::remove_dir_all(&covered).ok();
    std::fs::remove_dir_all(&stranded).ok();
    std::fs::remove_dir_all(&edge).ok();
}

/// F2.4 S1b/S3 through the UI's held-state seam: `held_now` answers in the order the spec steps —
/// edge, then clip, then effect — so a frame step nudges the narrowest thing in hand rather than the
/// largest. This is the state `wire_line_keys` reads instead of the hardcoded `None` that stood there
/// before, which made S2's nudge branch unreachable from any real key press.
#[test]
fn f2_4_s3_held_now_answers_edge_then_clip_then_effect() {
    use naivepost::ui;

    // Nothing in hand: no hold, so arrows do nothing and a step belongs to the line.
    ui::set_held_edge(None);
    ui::set_held_clip(None);
    ui::set_held_effect(None);
    assert_eq!(ui::held_now(), None, "an empty hand holds nothing");

    // An effect alone is the last of the three.
    ui::set_held_effect(Some(naivepost::cut::Fx::default()));
    assert_eq!(ui::held_now(), Some(Held::Effect), "only an effect held -> Effect");

    // A clip outranks it: S2 lists edge, clip, effect in that order.
    ui::set_held_clip(Some(naivepost::cut::Seg::default()));
    assert_eq!(ui::held_now(), Some(Held::Clip), "a clip outranks an effect");

    // And an edge outranks both -- the tightest thing in hand is what the frames move.
    ui::set_held_edge(Some(naivepost::cut::Seg::default()));
    assert_eq!(ui::held_now(), Some(Held::Edge), "an edge outranks clip and effect");

    ui::set_held_edge(None);
    ui::set_held_clip(None);
    ui::set_held_effect(None);
}

/// F2.4 S1b: what the first left press picked becomes which slot the hold lands in. Edge goes to the
/// edge slot (so a frame step moves only that border); a border or whole-clip pick takes the clip.
/// F2.4 S1: the row that starts being watched is THE ROW CLICKED, not always the first one.
#[test]
fn f2_4_s1_the_watched_row_is_the_row_clicked() {
    // Three camera rows: each answers with its own number when all four S1 conditions hold.
    for row in 0..3usize {
        let out = line::click_outcome_in_row(12.0 + row as f64, true, false, false, true, false, row);
        assert_eq!(out.watches, Some(row), "row {row} is the row that gets watched");
        assert!(out.line_moved, "and the line moves wherever that row's press landed");
    }

    // Ground that is not a picture row (a wave strip, a lane, the ruler, the effects lane) watches nothing,
    // whatever row number comes with it -- `on_picture` is what decides, per `watches_row`.
    assert_eq!(line::watches_index(false, false, true, false, 1), None, "not a picture band");
    assert_eq!(line::watches_index(true, true, true, false, 1), None, "not while something plays");
    assert_eq!(line::watches_index(true, false, false, false, 1), None, "not with no sources");
    assert_eq!(line::watches_index(true, false, true, true, 1), None, "never from the gutter");

    // A gutter press on a real row: no line moved, nothing watched, selection untouched.
    let gutter = line::click_outcome_in_row(99.0, true, true, false, true, true, 2);
    assert!(!gutter.line_moved, "the gutter moves no line");
    assert_eq!(gutter.watches, None, "and watches nothing");
    assert!(!gutter.clears_selection, "and touches no selection");
}

/// F2.4 S1b: the 12 px reach is measured off the box the band DRAWS -- hand corrections applied and scrolled
/// -- so a slid recording's edge is grabbed where it shows, not where `cut.json` says it was.
#[test]
fn f2_4_s1b_the_reach_is_measured_off_the_painted_box() {
    use naivepost::cut::Seg;
    use naivepost::cut_trim;
    use std::collections::BTreeMap;

    let pps = 8.0;
    let origin = 40.0; // the view has been scrolled 40 px into the tape
    let segs = vec![Seg { s: 10.0, e: 30.0, cam: 0, ..Default::default() }];
    let shift: BTreeMap<String, f64> = [("cam0".to_string(), 5.0)].into_iter().collect();

    // The painted geometry: shifted by the hand correction, then translated by the scroll.
    let boxes = naivepost::ui::cut_tracks::clip_boxes_shifted(&segs, &shift, pps);
    assert_eq!(boxes.len(), 1, "one kept scene draws one box");
    let left = boxes[0].x - origin;
    // `clip_boxes_shifted` maps a scene's own seconds through `timeline::session_at`, which SUBTRACTS the
    // hand correction, so a +5 s slide draws the box five seconds of tape to the LEFT (40 px at this zoom,
    // not 80) -- and that is the point: the un-shifted position is nowhere near the drawn one, which is what
    // the old placeholder geometry got wrong.
    assert!(
        (boxes[0].x - (5.0 * pps)).abs() < 1e-9,
        "the box sits at the shifted second times the zoom: {}",
        boxes[0].x
    );
    assert!(
        (left - (5.0 * pps - origin)).abs() < 1e-9,
        "on screen it starts at {left}, after the {origin} px scroll"
    );

    // Exactly at the drawn edge, and at EDGE_REACH_PX off it: the edge is picked up.
    assert_eq!(line::first_press_pick(0.0, true), PressPick::Edge, "on the drawn edge");
    assert_eq!(
        line::first_press_pick(line::EDGE_REACH_PX, true),
        PressPick::Edge,
        "within the 12 px reach of the DRAWN edge"
    );
    // A point 40 px inside that same drawn box: a border, not an edge -- and the point that WOULD have been
    // within reach of the un-shifted, un-scrolled box now answers differently, which is the whole rule.
    let inside_x = left + 40.0;
    let edge_px = (inside_x - left).min((inside_x - (left + boxes[0].w)).abs());
    assert!(edge_px > line::EDGE_REACH_PX, "40 px in is past the reach ({edge_px} px)");
    assert_eq!(line::first_press_pick(edge_px, true), PressPick::Border, "inside but clear: a border");
    // And the reach follows the shift and the scroll: a press at where the box would sit with NEITHER is
    // outside the drawn box entirely, so it cannot pick up an edge nobody can see.
    let stale = 10.0 * pps; // no shift, no scroll: the placeholder geometry's answer
    let stale_edge = (stale - left).min((stale - (left + boxes[0].w)).abs());
    assert!(
        stale_edge > line::EDGE_REACH_PX,
        "the placeholder geometry's edge is nowhere near the drawn one ({stale_edge} px)"
    );
    assert_eq!(line::first_press_pick(stale_edge, false), PressPick::Clip, "outside the drawn box entirely");
    let _ = cut_trim::EDGE_GRAB_PX; // the ordinary reach, which is NOT the one S1b uses
}

/// F2.4 S2: the step's width is the RECORDING's frame rate, read off the file by the real ffprobe shape --
/// parsed here, probed through a stub leg, so no test dials a binary.
#[test]
fn f2_4_s2_the_step_uses_the_probed_rate() {
    use naivepost::cut_fps;

    assert!((cut_fps::parse_rate("30000/1001").unwrap() - (30000.0 / 1001.0)).abs() < 1e-6, "NTSC rate");
    assert_eq!(cut_fps::parse_rate("25"), Some(25.0), "an integer rate");
    assert_eq!(cut_fps::parse_rate("25.000000\n"), Some(25.0), "ffprobe's padded form");
    assert_eq!(cut_fps::parse_rate("0/0"), None, "no video stream");
    assert_eq!(cut_fps::parse_rate(""), None, "nothing at all");
    assert_eq!(cut_fps::parse_rate("n/a"), None, "garbage");

    // The probe goes through the caller's leg, exactly as `source_probe::audio_streams` does.
    let good: &naivepost::source_probe::Tool =
        &|_tool: &str, _args: &[&str]| Ok("29970/1001\n".to_string());
    let asked = cut_fps::probe("ffprobe", std::path::Path::new("lecture.mkv"), good);
    assert!(asked.is_some_and(|r| (r - (29970.0 / 1001.0)).abs() < 1e-6), "the stub's answer: {asked:?}");

    let junk: &naivepost::source_probe::Tool =
        &|_tool: &str, _args: &[&str]| Ok("bogus".to_string());
    assert_eq!(cut_fps::probe("ffprobe", std::path::Path::new("x.mkv"), junk), None, "unparsable");

    let failed: &naivepost::source_probe::Tool =
        &|_tool: &str, _args: &[&str]| Err("ffprobe is not installed".to_string());
    assert_eq!(cut_fps::probe("ffprobe", std::path::Path::new("x.mkv"), failed), None, "no tool");

    // The fallback the page steps at when there is nothing to ask, and the arithmetic it feeds.
    assert_eq!(line::FALLBACK_FPS, 25.0, "// the prototype session's rate");
    let stepped = line::step_line(10.0, 1, 30000.0 / 1001.0);
    assert!(
        (stepped - (10.0 + 1001.0 / 30000.0)).abs() < 1e-12,
        "one frame of 29.97 footage is 1001/30000 s, not 1/25: {stepped}"
    );
}

/// F2.4 S4 (the pure half): a remembered line that some recording still covers comes back, and the view
/// scrolls so the line lands INSIDE it. The widget half is in tests/cut_line_place_step_widgets.rs.
#[test]
fn f2_4_s4_a_restored_line_cues_and_scrolls() {
    use naivepost::ui::cut_view;

    let dir = std::env::temp_dir().join(format!("cut-line-s4-scroll-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("cut")).expect("cut dir");
    let runs = vec![(0.0f64, 300.0)];

    line::write_line(&dir, LinePos { t: 250.0 }).expect("saved at 250 s");
    let restored = line::restore(&dir, &runs).expect("a recording covers 250 s");
    assert_eq!(restored.t, 250.0, "the position came back");

    // The view starts at zero and cannot show 250 s: revealing the playhead must move the origin.
    let pps = 4.0;
    let view = 400.0;
    let total = cut_view::total_px(300.0, pps);
    cut_view::set_origin(0.0, total, view);
    assert_eq!(cut_view::origin(), 0.0, "starts at the head of the tape");
    let after = cut_view::reveal_playhead(restored.t * pps, total, view);
    assert!(after > 0.0, "the view scrolled off zero: origin {after}");
    assert!(
        after <= restored.t * pps && restored.t * pps <= after + view,
        "the restored line is inside the view: origin {after}, line at {}, view {view}",
        restored.t * pps
    );

    // A line no recording covers stays refused, so nothing is cued or scrolled for it.
    line::write_line(&dir, LinePos { t: 400.0 }).expect("saved past the end");
    assert_eq!(line::restore(&dir, &runs), None, "nothing filmed at 400 s");
    let _ = std::fs::remove_dir_all(&dir);
}

/// F2.4 S1b: the wider reach is tried only where the spec says it applies -- on the picture band, clear
/// of the green and of the effects lane. A press already on a scene's own picture is S1's click, so the
/// reach must not run for it, or every ordinary click would fight the green for the hand.
#[test]
fn f2_4_s1b_the_reach_is_only_tried_clear_of_the_green() {
    assert!(
        line::press_tries_reach(true, false, false),
        "a bare picture-row press is what the reach exists for"
    );
    assert!(
        !line::press_tries_reach(true, false, true),
        "a press on the green is S1's click, not a reach grab"
    );
    assert!(
        !line::press_tries_reach(false, false, false),
        "a wave strip, a lane or the ruler is not the picture band"
    );
    assert!(
        !line::press_tries_reach(true, true, false),
        "the gutter never reached the tape"
    );
}

/// F2.4 S1: a press ABOVE the picture row is not a picture press, so nothing is watched. The placeholder
/// strip answers this through the same `surface_at` the painted band uses, which is why its handler can pass
/// the press's real y instead of assuming every click landed on a row.
#[test]
fn f2_4_s1_a_press_above_the_picture_row_is_not_a_picture_press() {
    use naivepost::cut_select::{self, Surface};

    let bands = cut_select::PLACEHOLDER_BANDS;
    let in_ruler = (bands.ruler.0 + bands.ruler.1) / 2.0;
    assert_eq!(
        cut_select::surface_at(in_ruler, &bands, 1),
        Surface::Ruler,
        "the strip's ruler band answers the ruler, not a picture row"
    );
    assert_eq!(
        cut_select::surface_at(bands.first_picture + 1.0, &bands, 1),
        Surface::PictureRow(0),
        "and one pixel into the row IS the row"
    );
    assert!(
        !line::watches_row(false, false, true, false),
        "so a ruler press watches nothing: the rule's own answer for a non-picture press"
    );
}

/// F2.4 S4: "cued to that frame and scrolled into view" is only a real claim when the tape does NOT fit.
/// At the open zoom a short session fits and `revealPlayhead` deliberately leaves the view alone; at a zoom
/// where the line lies past the right edge, revealing it must move the origin. The widget half lives in
/// tests/cut_line_place_step_widgets.rs.
#[test]
fn f2_4_s4_a_line_far_along_a_zoomed_tape_scrolls_into_view() {
    use naivepost::ui::cut_view;

    let pps = 24.0;
    let view = 450.0;
    let filmed = 70.0;
    let total = cut_view::total_px(filmed, pps);
    assert!(
        !cut_view::fits(total, view),
        "at {pps} px/s a {filmed} s tape is {total} px wide against a {view} px view, so it does not fit"
    );
    cut_view::set_origin(0.0, total, view);
    assert_eq!(cut_view::origin(), 0.0, "starts at the head of the tape");

    let line_px = 65.0 * pps;
    assert!(line_px > view, "the line starts off the right edge of the view");
    let after = cut_view::reveal_playhead(line_px, total, view);
    assert!(after > 0.0, "revealing it scrolled the view off zero: origin {after}");
    assert!(
        after <= line_px && line_px <= after + view,
        "and the line is inside the view: origin {after}, line {line_px}, view {view}"
    );
    cut_view::set_origin(0.0, total, view);
}

/// The 12 px reach is `// layout.lineReachPx`, `cut_line::EDGE_REACH_PX`.
#[test]
fn f2_4_s1b_apply_pick_puts_the_edge_in_the_edge_slot_and_the_clip_in_the_clip_slot() {
    use naivepost::ui;

    let seg = naivepost::cut::Seg { s: 10.0, e: 20.0, ..Default::default() };
    ui::set_held_edge(None);
    ui::set_held_clip(None);

    // Within the 12 px reach -> the edge is what the hand holds, not the clip.
    ui::apply_pick(line::first_press_pick(6.0, true), Some(seg.clone()));
    assert!(ui::held_edge().is_some(), "an edge pick fills the edge slot");
    assert!(ui::held_clip().is_none(), "and leaves the clip alone");
    assert_eq!(ui::held_now(), Some(Held::Edge));

    // Further in: a whole-clip/border pick takes the clip, and the edge slot is left clear.
    ui::set_held_edge(None);
    ui::apply_pick(line::first_press_pick(80.0, true), Some(seg.clone()));
    assert!(ui::held_clip().is_some(), "a border/clip pick fills the clip slot");
    assert!(ui::held_edge().is_none(), "and not the edge slot");
    assert_eq!(ui::held_now(), Some(Held::Clip));

    ui::set_held_edge(None);
    ui::set_held_clip(None);
}
