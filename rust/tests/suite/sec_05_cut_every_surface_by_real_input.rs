//! §05-cut through the pointer and the keyboard: every surface of the Cut page driven with REAL input
//! (`common::real_input`: xdotool on the test's X display, GTK's own event route), and each effect asserted on
//! the page's state. The signal-emitting tests drive one gesture alone and skip the competition a real press
//! meets on the band; these do not.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use adw::prelude::*;
use naivepost::ui;

use crate::common;
use crate::common::real_input::{drain, real_click, real_drag, real_key, real_scroll};

static RAN: AtomicBool = AtomicBool::new(false);

/// One stamped recording, 24 s at 25 fps.
const A: &str = "2026-10-18 19-21-52";

fn project(root: &Path) -> PathBuf {
    let dir = root.join("surfaces.naivepost");
    std::fs::create_dir_all(dir.join("sources")).unwrap();
    let made = Command::new("ffmpeg")
        .args(["-v", "error", "-y", "-f", "lavfi", "-i"])
        .arg("testsrc=size=160x90:rate=25:duration=24")
        .args([
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:duration=24",
            "-shortest",
        ])
        .arg(dir.join("sources").join(format!("{A}.mkv")))
        .status()
        .expect("ffmpeg runs");
    assert!(made.success(), "the test clip was written");
    std::fs::write(
        dir.join("naivepost.json"),
        format!(
            r#"{{"sources": [{{"path": "project:sources/{A}.mkv", "footage": true, "narrator": 1, "sepvoice": false, "tracks": []}}]}}"#
        ),
    )
    .unwrap();
    std::fs::create_dir_all(dir.join("cut")).unwrap();
    std::fs::write(
        dir.join("cut/cut.json"),
        r#"{"segs": [{"s": 1.0, "e": 6.0}, {"s": 8.0, "e": 13.0}, {"s": 15.0, "e": 20.0}]}"#,
    )
    .unwrap();
    std::fs::write(dir.join("cut/line.json"), r#"{"t": 2.0}"#).unwrap();
    dir
}

fn line(window: &adw::ApplicationWindow) -> f64 {
    ui::line_position(window).t
}

fn segs(window: &adw::ApplicationWindow) -> Vec<(f64, f64)> {
    ui::review_cut_of(window)
        .segs
        .iter()
        .map(|seg| (seg.s, seg.e))
        .collect()
}

fn status(window: &adw::ApplicationWindow) -> String {
    ui::state(window).status
}

/// The band's x for second `t`, where a press reads it back.
fn x_of(window: &adw::ApplicationWindow, t: f64) -> f64 {
    let scene = ui::cut_view::band_scene_for(window);
    ui::cut_tracks::band_x(&scene, t) - ui::cut_view::origin()
}

fn near(a: f64, b: f64, by: f64) -> bool {
    (a - b).abs() <= by
}

#[test]
fn sec_05_cut_every_surface_answers_real_input() {
    common::pin_private_cwd();
    std::env::remove_var("NAIVEPOST_PLAYER");
    let root = common::temp_dir("cut-real-input");
    let dir = project(&root);
    common::one_app_round(move |app| {
        let loaded = naivepost::project::load(&dir).expect("the project loads");
        let window = ui::window::build_window_at(app, &loaded, "Cut", &dir);
        window.set_default_size(1240, 900);
        window.present();
        common::settle();
        common::settle_workers();
        let band = common::widget_in(&window, "track-band").expect("the tracks band");
        // Zoomed so a second is wide enough to aim at.
        for _ in 0..3 {
            ui::line_step_button(&window, "zoom-in-button")
                .expect("zoom +")
                .emit_clicked();
            common::settle_workers();
        }
        // + zooms about the view's centre, so scroll back to the start, where the seconds below are.
        ui::cut_view::set_origin(0.0, f64::MAX, 1.0);
        ui::cut_view::repaint_page(&window);
        common::settle();
        // The rows as the page lays them out: an effects lane is always drawn, even an empty one.
        let bands = ui::cut_tracks::bands_for(naivepost::fx_lane::row_count(
            &ui::review_cut_of(&window).fx,
        ));
        let picture_y = bands.first_picture + bands.row_height / 2.0;
        let ruler_y = (bands.ruler.0 + bands.ruler.1) / 2.0;

        // F2.4 S1: a left click on the picture row puts the line there.
        real_click(&band, x_of(&window, 3.0), picture_y, 1);
        assert!(
            near(line(&window), 3.0, 0.2),
            "click: line at {}",
            line(&window)
        );

        // F2.4 S1 on a scrolled band: the press is measured with the scroll the band is drawn with. (The
        // first rewrite subtracted the scroll, so once the band was scrolled a click landed seconds away.)
        ui::cut_view::set_origin(300.0, f64::MAX, 1.0);
        ui::cut_view::repaint_page(&window);
        common::settle();
        real_click(&band, x_of(&window, 10.0), picture_y, 1);
        assert!(
            near(line(&window), 10.0, 0.2),
            "a click on the scrolled band: line at {}",
            line(&window)
        );
        ui::cut_view::set_origin(0.0, f64::MAX, 1.0);
        ui::cut_view::repaint_page(&window);
        common::settle();
        real_click(&band, x_of(&window, 3.0), picture_y, 1);

        // F2.8 S3: a right click never moves the line.
        real_click(&band, x_of(&window, 9.0), picture_y, 3);
        assert!(
            near(line(&window), 3.0, 0.2),
            "right click moved the line to {}",
            line(&window)
        );

        // F2.6 S1: a left drag on the ruler draws a selection over the dragged seconds.
        real_drag(
            &band,
            (x_of(&window, 9.0), ruler_y),
            (x_of(&window, 11.0), ruler_y),
            1,
        );
        let selected = ui::selection(&window).expect("a drag on the ruler draws a selection");
        assert!(
            near(selected.start, 9.0, 0.3) && near(selected.end, 11.0, 0.3),
            "selection {}..{} (status \"{}\")",
            selected.start,
            selected.end,
            status(&window)
        );
        // Esc and a click elsewhere: the click clears it (F2.4 S1).
        real_click(&band, x_of(&window, 4.0), picture_y, 1);
        assert!(
            ui::selection(&window).is_none(),
            "a click clears the selection"
        );

        // F2.8 S1: the pointer shows what a press would take. Near clip 1's end border (6 s), on the pictures and
        // on its green bar, the resize arrow; over the middle of the bar, the open hand (a drag moves the scene);
        // over plain picture ground, the page's own pointer. The first rewrite never changed it.
        let bar_y = (bands.selection.0 + bands.selection.1) / 2.0;
        let cursor =
            |band: &gtk4::Widget| band.cursor().and_then(|c| c.name()).map(|n| n.to_string());
        crate::common::real_input::real_hover(&band, x_of(&window, 6.0) + 2.0, picture_y);
        assert_eq!(
            cursor(&band).as_deref(),
            Some("col-resize"),
            "near a border on the pictures"
        );
        crate::common::real_input::real_hover(&band, x_of(&window, 6.0) - 2.0, bar_y);
        assert_eq!(
            cursor(&band).as_deref(),
            Some("col-resize"),
            "near a border on the green bar"
        );
        // The bar's ✕ sits in its middle: a button, so the pointing hand; the bar beside it, the open hand.
        crate::common::real_input::real_hover(&band, x_of(&window, 3.5), bar_y);
        assert_eq!(
            cursor(&band).as_deref(),
            Some("pointer"),
            "over the green bar's ✕"
        );
        crate::common::real_input::real_hover(&band, x_of(&window, 2.0), bar_y);
        assert_eq!(
            cursor(&band).as_deref(),
            Some("grab"),
            "over the green bar beside its ✕"
        );
        crate::common::real_input::real_hover(&band, x_of(&window, 3.5), picture_y);
        assert_eq!(cursor(&band), None, "over plain picture ground");

        // F2.8 S1: a left drag on clip 1's end border (6 s) resizes it.
        real_drag(
            &band,
            (x_of(&window, 6.0), picture_y),
            (x_of(&window, 7.0), picture_y),
            1,
        );
        let after = segs(&window);
        assert!(
            near(after[0].1, 7.0, 0.15),
            "a border drag moved clip 1's end to {:?} (status \"{}\")",
            after,
            status(&window)
        );
        // Ctrl+Z puts it back, Ctrl+Y redoes it.
        real_key(&window, "ctrl+z");
        assert!(
            near(segs(&window)[0].1, 6.0, 0.01),
            "Ctrl+Z: {:?}",
            segs(&window)
        );
        real_key(&window, "ctrl+y");
        assert!(
            near(segs(&window)[0].1, 7.0, 0.15),
            "Ctrl+Y: {:?}",
            segs(&window)
        );
        real_key(&window, "ctrl+z");

        // F2.8 S2: a right drag in the middle of clip 2 slides it, length kept.
        let pps = ui::cut_view::band_scene_for(&window).pps;
        let box_before = ui::cut_view::band_clip_boxes_for_test(&window)[1];
        real_drag(
            &band,
            (x_of(&window, 10.5), picture_y),
            (x_of(&window, 11.5), picture_y),
            3,
        );
        // A slide is a placement on the session clock (the cut's shift), the clip's own in and out unchanged.
        let box_after = ui::cut_view::band_clip_boxes_for_test(&window)[1];
        assert!(
            near(box_after.x - box_before.x, pps, pps * 0.2) && near(box_after.w, box_before.w, 0.5),
            "a right drag slid clip 2 by one second, length kept: {box_before:?} -> {box_after:?} (status \"{}\")",
            status(&window)
        );
        // One Undo for the whole slide, and Redo brings back where the hand left it, not its first tick.
        real_key(&window, "ctrl+z");
        let undone = ui::cut_view::band_clip_boxes_for_test(&window)[1];
        real_key(&window, "ctrl+y");
        let redone = ui::cut_view::band_clip_boxes_for_test(&window)[1];
        assert!(
            near(undone.x, box_before.x, 0.5) && near(redone.x, box_after.x, 0.5),
            "Ctrl+Z put clip 2 back ({undone:?}) and Ctrl+Y slid it again ({redone:?})"
        );
        real_key(&window, "ctrl+z");

        // F2.4 S1/S2: a click on a clip puts it in hand, and `.` then nudges THAT clip one frame (the line
        // stays); Esc puts it down, and from then on `.` steps the line one frame forward and `,` one back.
        real_click(&band, x_of(&window, 4.0), picture_y, 1);
        assert!(
            ui::held_clip().is_some(),
            "a click on clip 1 put it in hand"
        );
        let at = line(&window);
        real_key(&window, "period");
        assert!(
            near(segs(&window)[0].0, 1.04, 0.011) && near(line(&window), at, 0.011),
            "`.` with a clip in hand nudged it a frame: {:?}, line {}",
            segs(&window),
            line(&window)
        );
        real_key(&window, "ctrl+z");
        real_key(&window, "Escape");
        assert!(
            ui::held_now().is_none(),
            "Esc put the clip down (status \"{}\")",
            status(&window)
        );
        // Esc's other layers by a real key: an armed ⊕ Zoom comes down (its own controller answers, which the
        // copy's Esc used to swallow), and a selection is cleared.
        ui::set_zoom_armed(true);
        real_key(&window, "Escape");
        assert!(
            !ui::zoom_armed(),
            "Esc released the zoom arm (status \"{}\")",
            status(&window)
        );
        real_drag(
            &band,
            (x_of(&window, 9.0), ruler_y),
            (x_of(&window, 11.0), ruler_y),
            1,
        );
        assert!(
            ui::selection(&window).is_some(),
            "a ruler drag drew a selection"
        );
        real_key(&window, "Escape");
        assert!(
            ui::selection(&window).is_none(),
            "Esc cleared the selection"
        );
        real_click(&band, x_of(&window, 7.0), picture_y, 1);
        // F2.4 S1b may pick the nearest clip up from the gap; Esc puts it down, so the steps below move the line.
        real_key(&window, "Escape");
        assert!(ui::held_now().is_none(), "nothing is in hand");

        let at = line(&window);
        real_key(&window, "period");
        assert!(
            near(line(&window), at + 0.04, 0.011),
            "`.` stepped to {} from {at}",
            line(&window)
        );
        real_key(&window, "comma");
        assert!(
            near(line(&window), at, 0.011),
            "`,` stepped back to {} from {at}",
            line(&window)
        );

        // §1 wheel over the tracks: a notch up zooms in, Shift pans.
        // The zoom keeps the second under the cursor where it is: 4 s stays under the pointer.
        let pps = ui::cut_view::band_scene_for(&window).pps;
        let cursor = x_of(&window, 4.0);
        real_scroll(&band, cursor, picture_y, -1, &[]);
        assert!(
            ui::cut_view::band_scene_for(&window).pps > pps && near(x_of(&window, 4.0), cursor, 2.0),
            "a wheel notch over the tracks zooms in around the cursor ({pps} -> {} px/s, 4 s from x {cursor} \
             to {})",
            ui::cut_view::band_scene_for(&window).pps,
            x_of(&window, 4.0)
        );
        // Shift+wheel down pans later, as a scrolled list does.
        let origin = ui::cut_view::origin();
        real_scroll(&band, x_of(&window, 4.0), picture_y, 1, &["shift"]);
        assert!(
            ui::cut_view::origin() > origin,
            "Shift+wheel down over the tracks pans later ({origin} -> {})",
            ui::cut_view::origin()
        );

        // §1 wheel over the preview steps frames.
        let preview = common::widget_in(&window, "preview-panel").expect("the preview");
        let at = line(&window);
        real_scroll(&preview, 40.0, 40.0, 1, &[]);
        assert!(
            !near(line(&window), at, 0.001),
            "a wheel notch over the preview steps the line"
        );

        // F2.6 S1 + Delete: a selection inside clip 2 and Delete takes it out of the cut. The wheel steps
        // above zoomed and panned, so scroll back to where 9.5 s is on screen first.
        ui::cut_view::set_origin(0.0, f64::MAX, 1.0);
        ui::cut_view::repaint_page(&window);
        common::settle();
        let before = segs(&window).len();
        real_drag(
            &band,
            (x_of(&window, 9.5), ruler_y),
            (x_of(&window, 11.0), ruler_y),
            1,
        );
        assert!(
            ui::selection(&window).is_some(),
            "the drag drew a selection"
        );
        real_key(&window, "Delete");
        let left = segs(&window);
        assert!(
            left.len() == before + 1 && near(left[1].1, 9.5, 0.05) && near(left[2].0, 11.0, 0.05),
            "Delete took the selection out of the cut: {:?} (status \"{}\")",
            segs(&window),
            status(&window)
        );
        real_key(&window, "ctrl+z");

        // ⌦ with a clip in hand: a click on clip 2 takes it, Delete takes it out of the cut and puts it down.
        real_click(&band, x_of(&window, 12.0), picture_y, 1);
        assert!(
            ui::held_clip().is_some(),
            "a click on clip 2 put it in hand"
        );
        real_key(&window, "Delete");
        assert_eq!(
            segs(&window),
            vec![(1.0, 6.0), (15.0, 20.0)],
            "Delete removed the clip in hand (status \"{}\")",
            status(&window)
        );
        assert!(ui::held_now().is_none(), "and the hand is empty");
        real_key(&window, "ctrl+z");
        assert_eq!(segs(&window).len(), 3, "Ctrl+Z put clip 2 back");

        // The scrollbar: dragging its thumb scrolls the band while zoomed in.
        // 24 s at this zoom is wider than the view, so the bar is up.
        let bar = common::widget_in(&window, "track-scrollbar").expect("the band's scrollbar");
        assert!(
            bar.is_visible() && bar.width() > 0,
            "zoomed in, the scrollbar shows"
        );
        let origin = ui::cut_view::origin();
        let mid = f64::from(bar.height()) / 2.0;
        real_drag(&bar, (10.0, mid), (200.0, mid), 1);
        assert!(
            ui::cut_view::origin() > origin,
            "a scrollbar drag scrolls the band ({origin} -> {})",
            ui::cut_view::origin()
        );

        // F2.1 / F2.4 S1 during playback: a click while the recording plays moves the line there and playback
        // goes on from it. The line starts near the view's left so the view does not follow it away meanwhile.
        ui::cut_view::set_origin(0.0, f64::MAX, 1.0);
        ui::cut_view::repaint_page(&window);
        real_click(&band, x_of(&window, 2.0), picture_y, 1);
        let rec = ui::play_recording_button(&window).expect("\u{25b6}");
        real_click(&rec, 10.0, 10.0, 1);
        drain(Duration::from_millis(300), Duration::from_secs(1));
        // The page animates while it plays, so the helpers' settling runs to its cap: the line is checked
        // against the time that really passed since the click went in.
        let before = line(&window);
        let clicked = std::time::Instant::now();
        real_click(&band, x_of(&window, 9.0), picture_y, 1);
        let playing_at = line(&window);
        let passed = clicked.elapsed().as_secs_f64();
        drain(Duration::from_millis(600), Duration::from_millis(600));
        let later = line(&window);
        assert!(
            playing_at >= 8.9 && playing_at <= 9.0 + passed + 0.3 && later > playing_at,
            "a click while playing moves the line and playback goes on from there: {before} -> {playing_at} \
             after {passed:.1} s, then {later}"
        );
        if let Some(stop) = ui::stop_button(&window) {
            real_click(&stop, 10.0, 10.0, 1);
        }

        // §A: the zoom buttons zoom about the red line, so it stays in view. A line near the right edge stays
        // where it stands on screen; a line scrolled off screen comes to the middle. The first rewrite zoomed
        // about the middle of the view, and a few presses of + pushed the line off screen.
        let zoom_in = ui::line_step_button(&window, "zoom-in-button").expect("zoom +");
        let width = band.width() as f64;
        real_click(&band, width - 60.0, picture_y, 1);
        let line_x = |w: &adw::ApplicationWindow| x_of(w, line(w));
        let stood = line_x(&window);
        for _ in 0..3 {
            real_click(&zoom_in, 6.0, 6.0, 1);
        }
        common::settle();
        assert!(
            (line_x(&window) - stood).abs() < 3.0,
            "the line stays where it stood through + + +: {stood:.0} -> {:.0}",
            line_x(&window)
        );
        real_scroll(&band, 300.0, picture_y, 8, &["shift"]);
        common::settle();
        assert!(
            line_x(&window) < 0.0 || line_x(&window) > width,
            "scrolled the line off screen: {:.0}",
            line_x(&window)
        );
        real_click(&zoom_in, 6.0, 6.0, 1);
        common::settle();
        let gutter = ui::cut_view::gutter_px();
        let middle = gutter + (width - gutter) / 2.0;
        assert!(
            (line_x(&window) - middle).abs() < 40.0,
            "a press of + brings the line back to the middle: {:.0} (middle {middle:.0})",
            line_x(&window)
        );

        RAN.store(true, Ordering::SeqCst);
        app.quit();
    });
    assert!(RAN.load(Ordering::SeqCst), "the round never ran");
}
