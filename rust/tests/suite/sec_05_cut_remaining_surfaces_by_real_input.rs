//! §05-cut through the pointer and the keyboard, the surfaces `sec_05_cut_every_surface_by_real_input` leaves:
//! a card held by a double or a right click (F2.12 S6), the zoom box drawn on the preview (F3.1 S2), the
//! scrollbar's gear at high zoom (§1, "one screenful is always 40 px of drag"), the wheel over the transport
//! bar (§1), and every toolbar verb and the run bar's ▶ pressed by a real click (F2.5-F2.9, F2.12 S1, F2.14),
//! so an overlay, an insensitive face or the wrong widget under the pointer cannot hide behind `emit_clicked`.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use adw::prelude::*;
use naivepost::ui;

use crate::common;
use crate::common::real_input::{
    drain, real_click, real_click_centre, real_double_click, real_drag, real_key, real_scroll,
};

static RAN: AtomicBool = AtomicBool::new(false);

/// The recording the cut is made of: 24 s at 25 fps.
const A: &str = "2026-10-18 19-21-52";
/// A long recording later the same evening: 300 s, so the band at full zoom is far wider than its bar.
const B: &str = "2026-10-18 19-30-00";

fn clip(at: &Path, video: &str, audio: &str, out: &Path) {
    let made = Command::new("ffmpeg")
        .args(["-v", "error", "-y", "-f", "lavfi", "-i", video])
        .args(["-f", "lavfi", "-i", audio, "-shortest"])
        .arg(out)
        .status()
        .expect("ffmpeg runs");
    assert!(made.success(), "the test clip {} was written", at.display());
}

fn project(root: &Path) -> PathBuf {
    let dir = root.join("remaining.naivepost");
    std::fs::create_dir_all(dir.join("sources")).unwrap();
    let a = dir.join("sources").join(format!("{A}.mkv"));
    clip(
        &a,
        "testsrc=size=160x90:rate=25:duration=24",
        "sine=frequency=440:duration=24",
        &a,
    );
    let b = dir.join("sources").join(format!("{B}.mkv"));
    clip(
        &b,
        "testsrc=size=64x36:rate=5:duration=300",
        "sine=frequency=330:duration=300",
        &b,
    );
    std::fs::write(
        dir.join("naivepost.json"),
        format!(
            r#"{{"sources": [{{"path": "project:sources/{A}.mkv", "footage": true, "narrator": 1, "sepvoice": false, "tracks": []}}, {{"path": "project:sources/{B}.mkv", "footage": true, "narrator": 1, "sepvoice": false, "tracks": []}}]}}"#
        ),
    )
    .unwrap();
    std::fs::create_dir_all(dir.join("cut")).unwrap();
    std::fs::write(
        dir.join("cut/cut.json"),
        r#"{"segs": [{"s": 1.0, "e": 6.0}, {"s": 8.0, "e": 13.0}, {"s": 13.0, "e": 13.0, "dur": 2.0, "ins": "assets/tier.svg"}, {"s": 15.0, "e": 20.0}]}"#,
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

fn x_of(window: &adw::ApplicationWindow, t: f64) -> f64 {
    let scene = ui::cut_view::band_scene_for(window);
    ui::cut_tracks::band_x(&scene, t) - ui::cut_view::origin()
}

fn near(a: f64, b: f64, by: f64) -> bool {
    (a - b).abs() <= by
}

fn to_start(window: &adw::ApplicationWindow) {
    ui::cut_view::set_origin(0.0, f64::MAX, 1.0);
    ui::cut_view::repaint_page(window);
    common::settle();
}

/// The widget named `name` that is on screen now: several forms each carry their own ✕.
fn shown_named(root: &gtk4::Widget, name: &str) -> Option<gtk4::Widget> {
    if root.widget_name() == name && root.is_mapped() && root.width() > 0 {
        return Some(root.clone());
    }
    let mut child = root.first_child();
    while let Some(widget) = child {
        if let Some(found) = shown_named(&widget, name) {
            return Some(found);
        }
        child = widget.next_sibling();
    }
    None
}

fn button(window: &adw::ApplicationWindow, name: &str) -> gtk4::Button {
    ui::line_step_button(window, name).unwrap_or_else(|| panic!("the Cut page has {name}"))
}

#[test]
fn sec_05_cut_the_remaining_surfaces_answer_real_input() {
    common::pin_private_cwd();
    std::env::remove_var("NAIVEPOST_PLAYER");
    let root = common::temp_dir("cut-real-input-rest");
    let dir = project(&root);
    common::one_app_round(move |app| {
        let loaded = naivepost::project::load(&dir).expect("the project loads");
        let window = ui::window::build_window_at(app, &loaded, "Cut", &dir);
        window.set_default_size(1240, 900);
        window.present();
        common::settle();
        common::settle_workers();
        let band = common::widget_in(&window, "track-band").expect("the tracks band");
        // Zoomed so a second is wide enough to aim at, scrolled back to the start.
        for _ in 0..4 {
            button(&window, "zoom-in-button").emit_clicked();
            common::settle_workers();
        }
        to_start(&window);
        let bands = ui::cut_tracks::bands_for(naivepost::fx_lane::row_count(
            &ui::review_cut_of(&window).fx,
        ));
        let picture_y = bands.first_picture + bands.row_height / 2.0;
        let ruler_y = (bands.ruler.0 + bands.ruler.1) / 2.0;

        // F2.12 S6: a double left click on the card holds it, and Insert becomes Edit.
        real_double_click(&band, x_of(&window, 14.0), picture_y, 1);
        let held = ui::held_clip().unwrap_or_else(|| {
            panic!(
                "a real double click on the card held it (status \"{}\")",
                status(&window)
            )
        });
        assert_eq!(held.ins, "assets/tier.svg", "the card under the pointer");
        assert_eq!(
            button(&window, "insert-button").label().unwrap_or_default(),
            naivepost::cut_insert::edit_verb(true),
            "Insert reads Edit under a held card"
        );
        real_key(&window, "Escape");
        assert!(ui::held_now().is_none(), "Esc put the card down");
        // ...and a right click holds it on its first press.
        real_click(&band, x_of(&window, 14.0), picture_y, 3);
        assert_eq!(
            ui::held_clip().map(|seg| seg.ins).as_deref(),
            Some("assets/tier.svg"),
            "a real right click on the card held it (status \"{}\")",
            status(&window)
        );
        real_key(&window, "Escape");
        assert!(ui::held_now().is_none(), "Esc put the card down again");

        // §1: the wheel over the transport bar steps the line one frame a notch.
        real_click(&band, x_of(&window, 3.0), picture_y, 1);
        real_key(&window, "Escape");
        let transport = common::widget_in(&window, "group-transport").expect("the transport bar");
        let at = line(&window);
        real_scroll(
            &transport,
            10.0,
            f64::from(transport.height()) / 2.0,
            1,
            &[],
        );
        assert!(
            near(line(&window), at + 0.04, 0.011),
            "a wheel notch down over the transport stepped one frame: {at} -> {}",
            line(&window)
        );
        real_scroll(
            &transport,
            10.0,
            f64::from(transport.height()) / 2.0,
            -1,
            &[],
        );
        assert!(
            near(line(&window), at, 0.011),
            "and a notch up stepped back: {}",
            line(&window)
        );

        // F3.1 S1-S2: ⊕ Zoom picked from the Effect menu by real clicks arms the zoom; a left drag on the preview,
        // the line on a clip, then draws the box and opens its form.
        let menu = common::widget_in(&window, "effect-button").expect("the Effect menu button");
        real_click_centre(&menu);
        let item = common::widget_in(&window, "effect-item-zoom")
            .or_else(|| {
                menu.downcast_ref::<gtk4::MenuButton>()
                    .and_then(|m| m.popover())
                    .and_then(|p| common::find_in(p.upcast_ref(), "effect-item-zoom"))
            })
            .expect("the menu's Zoom item");
        real_click_centre(&item);
        assert!(
            !menu
                .downcast_ref::<gtk4::MenuButton>()
                .and_then(|m| m.popover())
                .is_some_and(|p| p.is_visible()),
            "picking an item closed the Effect menu"
        );
        assert!(
            ui::zoom_armed(),
            "a real click on \u{2295} Zoom armed it (status \"{}\")",
            status(&window)
        );
        let preview = common::widget_in(&window, "preview-panel").expect("the preview");
        let (w, h) = (f64::from(preview.width()), f64::from(preview.height()));
        real_drag(&preview, (w * 0.3, h * 0.3), (w * 0.7, h * 0.7), 1);
        assert!(
            ui::zoom_form_open().is_some(),
            "a real drag on the preview opened the zoom form (status \"{}\")",
            status(&window)
        );
        // §A: the column's scroller holds EITHER the idle rows OR a form body.
        assert!(
            !common::widget_in(&window, "cut-form").is_some_and(|rows| rows.is_visible()),
            "the open zoom form stood in for the idle readings"
        );
        let close = shown_named(window.upcast_ref(), "cut-form-close")
            .expect("the form's \u{2715} on screen");
        real_click_centre(&close);
        assert!(ui::zoom_form_open().is_none(), "✕ closed the form");

        // The verbs, each by a real click on its button.
        // F2.7 Split at the line (3 s, inside clip 1).
        to_start(&window);
        real_click(&band, x_of(&window, 3.0), picture_y, 1);
        real_key(&window, "Escape");
        let before = segs(&window);
        real_click_centre(&button(&window, "split-button"));
        let split = segs(&window);
        assert!(
            split.len() == before.len() + 1 && split.iter().any(|seg| near(seg.1, 3.0, 0.05)),
            "Split cut clip 1 at the line: {split:?} (status \"{}\")",
            status(&window)
        );
        real_key(&window, "ctrl+z");
        assert_eq!(segs(&window), before, "Ctrl+Z joined it again");

        // F2.7 Remove over a selection inside clip 2.
        // The view can follow the line between two presses; the drag is measured on a view at its start.
        to_start(&window);
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
        real_click_centre(&button(&window, "remove-button"));
        let removed = segs(&window);
        assert!(
            removed.len() == before.len() + 1
                && removed.iter().any(|seg| near(seg.1, 9.5, 0.05))
                && removed.iter().any(|seg| near(seg.0, 11.0, 0.05)),
            "Remove took the selection out: {removed:?} (status \"{}\")",
            status(&window)
        );
        real_key(&window, "ctrl+z");
        assert_eq!(segs(&window), before, "Ctrl+Z put it back");

        // F2.7 Add over a selection on footage the cut left out (6.5..7.5 s).
        // The view can follow the line between two presses; the drag is measured on a view at its start.
        to_start(&window);
        real_drag(
            &band,
            (x_of(&window, 6.5), ruler_y),
            (x_of(&window, 7.5), ruler_y),
            1,
        );
        assert!(
            ui::selection(&window).is_some(),
            "the drag drew a selection"
        );
        real_click_centre(&button(&window, "add-button"));
        let added = segs(&window);
        assert!(
            added != before
                && added
                    .iter()
                    .any(|seg| seg.0 <= 6.6 && seg.1 >= 7.4 && seg.1 > seg.0),
            "Add put 6.5..7.5 s in the cut: {added:?} (status \"{}\")",
            status(&window)
        );
        real_key(&window, "ctrl+z");
        assert_eq!(segs(&window), before, "Ctrl+Z took it out again");

        // F2.9 Copy, then Paste at the line, then a Lane for the copy.
        // The view can follow the line between two presses; the drag is measured on a view at its start.
        to_start(&window);
        real_drag(
            &band,
            (x_of(&window, 9.0), ruler_y),
            (x_of(&window, 11.0), ruler_y),
            1,
        );
        real_click_centre(&button(&window, "copy-button"));
        assert!(
            status(&window).starts_with("copied"),
            "Copy took the selection in hand: \"{}\"",
            status(&window)
        );
        real_click(&band, x_of(&window, 17.0), picture_y, 1);
        real_click_centre(&button(&window, "paste-button"));
        assert!(
            status(&window).starts_with("pasted"),
            "Paste put the copy at the line: \"{}\"",
            status(&window)
        );
        assert_ne!(segs(&window), before, "and the cut changed");
        real_key(&window, "ctrl+z");
        assert_eq!(segs(&window), before, "Ctrl+Z undid the paste");
        let lanes = ui::review_lanes(&window).len();
        // The view can follow the line between two presses; the drag is measured on a view at its start.
        to_start(&window);
        real_drag(
            &band,
            (x_of(&window, 9.0), ruler_y),
            (x_of(&window, 11.0), ruler_y),
            1,
        );
        real_click_centre(&button(&window, "copy-button"));
        real_click_centre(&button(&window, "lane-button"));
        assert_eq!(
            ui::review_lanes(&window).len(),
            lanes + 1,
            "Lane gave the copy a row (status \"{}\")",
            status(&window)
        );
        real_key(&window, "ctrl+z");
        real_key(&window, "Escape");

        // F2.12 S1: Insert with the line on footage raises its chooser, once.
        to_start(&window);
        real_click(&band, x_of(&window, 3.0), picture_y, 1);
        real_key(&window, "Escape");
        let asked = ui::insert_asks_for_test();
        real_click_centre(&button(&window, "insert-button"));
        assert_eq!(
            ui::insert_asks_for_test(),
            asked + 1,
            "a real click on Insert raised one chooser (status \"{}\")",
            status(&window)
        );
        drain(Duration::from_millis(50), Duration::from_millis(300));
        // The chooser is modal: a real Escape on it answers Cancel and gives the page its input back.
        let chooser = gtk4::Window::list_toplevels()
            .into_iter()
            .filter_map(|w| w.downcast::<gtk4::Window>().ok())
            .find(|w| {
                w.is_visible()
                    && w.upcast_ref::<gtk4::Widget>() != window.upcast_ref::<gtk4::Widget>()
            })
            .expect("Insert's chooser is on screen");
        real_key(&chooser, "Escape");
        drain(Duration::from_millis(50), Duration::from_millis(300));
        assert!(!chooser.is_visible(), "Escape closed the chooser");

        // §1 gear: zoomed all the way in on 324 s of tape the bar is far shorter than the band, and one
        // screenful is 40 px of drag rather than a few pixels.
        for _ in 0..20 {
            button(&window, "zoom-in-button").emit_clicked();
        }
        common::settle();
        to_start(&window);
        let bar = common::widget_in(&window, "track-scrollbar").expect("the scrollbar");
        assert!(
            bar.is_visible() && bar.width() > 0,
            "zoomed in, the bar shows"
        );
        let view = ui::cut_view::view_px(ui::cut_view::surface_width(&window));
        let origin = ui::cut_view::origin();
        let mid = f64::from(bar.height()) / 2.0;
        real_drag(&bar, (6.0, mid), (46.0, mid), 1);
        let moved = ui::cut_view::origin() - origin;
        assert!(
            near(moved, view, view * 0.25),
            "40 px of drag on the geared bar crossed one screenful ({view:.0} px): moved {moved:.0} px"
        );

        // F2.14: the run bar's ▶ on the Cut page is Suggest, and a real click starts it.
        to_start(&window);
        let play = ui::play_button(&window).expect("the run bar's \u{25b6}");
        let logs = ui::window_logs().len();
        real_click_centre(&play);
        common::settle_workers();
        assert!(
            ui::window_logs().len() > logs || !status(&window).is_empty(),
            "a real click on \u{25b6} reached Suggest: status \"{}\"",
            status(&window)
        );
        common::settle();

        // §11 §2: once a preview started, the bar's ▶ is that preview's transport until ⏹. The page's ▶ plays the
        // recording; the bar's ▶ then pauses what plays (the line AND the player), plays it on, and ⏹ ends it.
        let record = ui::play_recording_button(&window).expect("the page's \u{25b6}");
        real_click_centre(&record);
        drain(Duration::from_millis(100), Duration::from_millis(600));
        let shows = |what: &str, playing: bool| {
            let transport = ui::preview_player(&window).transport;
            assert!(
                transport.playing == playing && naivepost::play_built_in::playing() == playing,
                "{what}: transport playing {} and the player playing {}, wanted {playing} (status \"{}\")",
                transport.playing,
                naivepost::play_built_in::playing(),
                status(&window)
            );
        };
        shows("a real click on the page's \u{25b6}", true);
        real_click_centre(&play);
        drain(Duration::from_millis(100), Duration::from_millis(600));
        shows("the bar's \u{25b6} over a playing preview", false);
        real_click_centre(&play);
        drain(Duration::from_millis(100), Duration::from_millis(600));
        shows("the bar's \u{25b6} over a paused preview", true);
        let stop = ui::stop_button(&window).expect("the run bar's \u{23f9}");
        real_click_centre(&stop);
        drain(Duration::from_millis(100), Duration::from_millis(600));
        assert!(
            !ui::preview_player(&window).transport.started && !naivepost::play_built_in::playing(),
            "a real click on \u{23f9} ended the preview: started {}, the player playing {} (status \"{}\")",
            ui::preview_player(&window).transport.started,
            naivepost::play_built_in::playing(),
            status(&window)
        );

        RAN.store(true, Ordering::SeqCst);
        app.quit();
    });
    assert!(RAN.load(Ordering::SeqCst), "the round never ran");
}
