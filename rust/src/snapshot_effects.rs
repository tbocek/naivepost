//! The F3 effect screens (`06-svg` ... `06-lane`), each staged through the doors its own widgets fire rather
//! than by poking state, so the shot shows what a press earned. Split out of `snapshot.rs`, which is past the
//! size another staging block fits in; every rule below lives in the module whose screen it draws.

use adw::prelude::*;

use crate::cut;
use crate::cut_line;
use crate::ui;

/// Paint one of the effect screens onto `window`.
pub fn paint(
    window: &adw::ApplicationWindow,
    dir: &std::path::Path,
    _out: &std::path::Path,
    screen: &str,
) -> bool {
    // F3.5's screen is the drawing form OPEN, so the shot must drive the same doors the widgets use
    // (`tests/suite/svg_drawing_widgets.rs` fires exactly these) rather than poke state: a picture painted
    // from a shortcut would not be the picture the app draws.
    if screen == "06-svg" {
        // A line exists and points at 81 s, so the heading reads `SVG at 1:21` -- spelled by
        // `fx_svg::form_title`, never hardcoded here.
        ui::note_place(true);
        ui::set_line_position(window, cut_line::LinePos { t: 81.0 });
        // File first, as S1/S2 order it; then a DRAWN box (not the click default) lands and opens the
        // form, which is the state the spec's FORM node describes.
        ui::svg_chosen(window, Some("assets/logo.svg"));
        ui::svg_drag_ended(window, (24.0, 24.0), (150.0, 96.0));
        return true;
    }

    // F3.1's screen is the zoom form OPEN (`spec/img/06-zoom.png`), and like `06-volume`/`06-svg` the shot
    // must reach it through the doors the widgets fire — arm (⊕ Zoom), a box on the preview, the form that
    // lands — rather than by poking state.
    if screen == "06-zoom" {
        ui::note_place(true);
        ui::note_zoom_place(true);
        ui::set_line_position(window, cut_line::LinePos { t: 81.0 });
        let said = ui::press_zoom_item(window);
        assert!(
            ui::zoom_armed(),
            "snapshot: 06-zoom could not arm -- {said}"
        );
        let gesture =
            ui::zoom_gesture(window).expect("the page wired a drag onto the preview panel");
        gesture.emit_by_name::<()>("drag-begin", &[&120.0f64, &90.0f64]);
        gesture.emit_by_name::<()>("drag-end", &[&520.0f64, &330.0f64]);
        assert!(
            ui::zoom_form_open().is_some(),
            "snapshot: 06-zoom drew no form"
        );
        // No Apply here either: Apply closes the form, and the screen §F3.1 names IS the form.
        return true;
    }

    // §F3.3's screen is the speed form OPEN (`spec/img/06-speed.png`): with only a line and no marked
    // band the answer is a stop at the line (§F3.2 S1), so the shot places a line and presses the
    // dropdown row, which is what `tests/suite/speed_form_widgets.rs` presses.
    if screen == "06-speed" {
        ui::note_place(true);
        ui::set_line_position(window, cut_line::LinePos { t: 81.0 });
        let said = ui::press_speed_item(window);
        assert!(
            ui::speed_form_open().is_some(),
            "snapshot: 06-speed drew no form -- {said}"
        );
        return true;
    }

    // §06-effects#3's own screen: the Cut page after all THREE post-cut passes have placed what the model was
    // answered with — ✐ Captions (F3.9), ⏩ Speeds (F3.10) and ✨ Decorations (F3.11) — so the one shot shows the
    // run's three pass jobs having each done its job (no buttons: §F2.14 S2b gives the passes none), and the
    // lane showing a caption, a fast stretch
    // and a zoom together.
    if screen == "06-flows" {
        let two = cut::Cut {
            segs: vec![
                cut::Seg {
                    s: 0.0,
                    e: 20.0,
                    cam: 0,
                    ..Default::default()
                },
                cut::Seg {
                    s: 20.0,
                    e: 40.0,
                    cam: 0,
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        ui::seed_review_cut(window, &two);
        let captions = ui::run_captions_pass_with_reply(
            window,
            &[ui::CaptionBatch {
                clips: vec![(1, 20.0), (2, 20.0)],
                calls: vec![crate::cut_captions::Call {
                    clip: 1,
                    start: 2.0,
                    end: 5.0,
                    text: "the reveal".to_string(),
                }],
            }],
        );
        let speeds = ui::run_speeds_pass_with_reply(
            window,
            &[crate::cut_speed_pass::Call { clip: 2, rate: 1.5 }],
        );
        let decorations = ui::run_effects_pass_with_reply(
            window,
            &[crate::cut_effects_pass::Call {
                clip: 1,
                kind: "zoom".to_string(),
                start: 8.0,
                end: 14.0,
                gain: None,
            }],
        );
        let landed: Vec<String> = ui::review_cut_of(window)
            .fx
            .iter()
            .map(|fx| format!("{}@{:.0}", fx.kind, fx.t))
            .collect();
        // Three passes, three records — a pass that silently placed nothing would otherwise print a shot of the
        // bare page and still exit 0, which is how a broken wire hides from a snapshot recipe.
        assert_eq!(
            landed.len(),
            3,
            "snapshot: 06-flows placed {landed:?}, expected a caption, a speed and a zoom"
        );
        // The three job buttons themselves: this screen exists to show THEM working, so a shot where one of
        // The three passes ran as jobs of the cut, so what this screen records is what they PLACED: no control
        // starts them (§2's run bar holds only ▶ / ⏹ / the bar and the readouts), and a pass that placed
        // nothing would otherwise print a bare page and still exit 0.
        let shown = ui::find_status(window.upcast_ref())
            .map(|row| row.text().to_string())
            .unwrap_or_default();
        println!(
            "snapshot: 06-flows | {} bar(s): {} | status: {shown} | {captions} / {speeds} / {decorations}",
            landed.len(),
            landed.join(" ")
        );
        return true;
    }

    // §F3.10's own screen: the Cut page AFTER a ⏩ Speeds pass placed what the model asked for, with that
    // pass's sentence on the status row.
    if screen == "06-speeds" {
        let two = cut::Cut {
            segs: vec![
                cut::Seg {
                    s: 0.0,
                    e: 20.0,
                    cam: 0,
                    ..Default::default()
                },
                cut::Seg {
                    s: 20.0,
                    e: 40.0,
                    cam: 0,
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        ui::seed_review_cut(window, &two);
        let said = ui::run_speeds_pass_with_reply(
            window,
            &[crate::cut_speed_pass::Call { clip: 2, rate: 1.5 }],
        );
        let landed = ui::review_cut_of(window)
            .fx
            .iter()
            .filter(|fx| fx.kind == "speed")
            .count();
        let shown = ui::find_status(window.upcast_ref())
            .map(|row| row.text().to_string())
            .unwrap_or_default();
        println!("snapshot: 06-speeds | {landed} speed bar(s) | status: {shown} | {said}");
        return true;
    }

    // §F3.11's pass screen, beside `06-speeds` and built exactly as it is: no `spec/img/06-decorations.png`
    // exists, so this is a program-side screen driven through the page's own doors — never by poking state:
    if screen == "06-decorations" {
        let two = cut::Cut {
            segs: vec![
                cut::Seg {
                    s: 0.0,
                    e: 20.0,
                    cam: 0,
                    ..Default::default()
                },
                cut::Seg {
                    s: 20.0,
                    e: 40.0,
                    cam: 0,
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        ui::seed_review_cut(window, &two);
        let said = ui::run_effects_pass_with_reply(
            window,
            &[
                crate::cut_effects_pass::Call {
                    clip: 1,
                    kind: "zoom".to_string(),
                    start: 2.0,
                    end: 6.0,
                    gain: None,
                },
                crate::cut_effects_pass::Call {
                    clip: 2,
                    kind: "stop".to_string(),
                    start: 1.0,
                    end: 4.0,
                    gain: None,
                },
                crate::cut_effects_pass::Call {
                    clip: 2,
                    kind: "volume".to_string(),
                    start: 8.0,
                    end: 14.0,
                    gain: Some(0.25),
                },
            ],
        );
        let landed: Vec<String> = ui::review_cut_of(window)
            .fx
            .iter()
            .map(|fx| format!("{}@{:.0}", fx.kind, fx.t))
            .collect();
        let shown = ui::find_status(window.upcast_ref())
            .map(|row| row.text().to_string())
            .unwrap_or_default();
        println!(
            "snapshot: 06-decorations | {} bar(s): {} | status: {shown} | {said}",
            landed.len(),
            landed.join(" ")
        );
        return true;
    }

    if screen == "06-volume" {
        // A marked stretch, which is what §F3.6's own heading reads: the spec's picture says
        // `Volume 1:21 – 1:25`, four seconds, and a band is how a person gets exactly that (a bare line
        // gives the form `LINE_SECONDS` from the line instead).
        ui::draw_selection(
            window,
            crate::cut_select::Surface::SelectionBand,
            None,
            81.0,
            85.0,
        );
        let said = ui::press_volume_item(window);
        // No Apply here either: Apply closes the form, and the screen §F3.6 names IS the form.
        let _ = said;
        return true;
    }

    if screen == "06-label" {
        // A placed line 81 s in, so the heading names that moment -- spelled by `fx_label::form_title`,
        // never hardcoded here.
        ui::note_place(true);
        ui::set_line_position(window, cut_line::LinePos { t: 81.0 });
        let said = ui::press_label_item(window);
        // No Apply here either: Apply closes the form, and the screen §F3.7 names IS the form.
        let _ = said;
        return true;
    }

    // §06-effects#2's Effect-menu screen (`spec/img/06-effect-menu.png`): the ✚ Effect dropdown OPEN, showing
    // its six rows in `fx_lane::MENU_ORDER`.
    if screen == "06-effect-menu" {
        // §06 answer 3(b) is a picture of the PAGE with the menu over it — thumbnails, waveforms and a framed
        // preview all visible around the popup.
        let menu_cut = crate::cut::Cut {
            segs: vec![crate::cut::Seg {
                s: 0.0,
                e: 120.0,
                cam: 0,
                ..Default::default()
            }],
            ..Default::default()
        };
        ui::seed_review_cut(window, &menu_cut);
        ui::set_session_recordings(&[crate::timeline::Recording {
            base: "session-tape".to_string(),
            start: 0.0,
            end: 120.0,
        }]);
        ui::set_line_position(window, cut_line::LinePos { t: 5.0 });
        crate::snapshot_face::seed_snapshot_frame(dir, "session-tape", 5.0);
        ui::cut_picture::set_preview_camera_base("session-tape");
        ui::cut_picture::set_snapshot_dir(dir);
        ui::refresh_camera_rows(window);
        // The envelope cache, so the row's wave strip has peaks to draw rather than a flat baseline (the same
        // door `06-lane` stages through; harmless when the wave build cannot run).
        if let Err(why) = crate::snapshot_lane::stage_envelope(dir, "session-tape") {
            eprintln!("snapshot: 06-effect-menu wave cache | {why}");
        }
        ui::cut_tracks::allow_frame_decoding();
        ui::cut_view::repaint_page(window);
        let _ = ui::cut_picture::refresh(window);
        let rows = crate::snapshot_effect_menu::stage(window);
        let want = crate::snapshot_effect_menu::labels_in_menu_order();
        // Compare the ORDER of the labels, read off the widget tree, against the order the spec gives.
        let got: Vec<String> = rows
            .iter()
            .skip(1)
            .map(|row| row.split(": ").nth(1).unwrap_or("").to_string())
            .collect();
        assert_eq!(
            got, want,
            "snapshot: 06-effect-menu's popover is not in fx_lane::MENU_ORDER"
        );
        crate::snapshot_effect_menu::note_shot_sidecar();
        println!("snapshot: 06-effect-menu | {}", rows.join(" | "));
        return true;
    }

    // §F3.4's own picture (`spec/img/06-text.png`) is TWO things at once: the caption ON the preview, across the
    // lower third, and the `Text at m:ss` form beside it with the words in its field.
    if screen == "06-text" {
        let said = crate::snapshot_text::stage(window, dir);
        println!("snapshot: 06-text | {}", said.join(" | "));
        return true;
    }

    if screen == "06-lane" {
        // One staging for both images of this screen — the window shot and the 1240 x 200 lane beside it: §2's
        // list (a zoom, a staying zoom, a ×0.5 scene-sound speed, a caption, a volume, a label) at the same
        // 4 px/s the spec's own lane is drawn with, so every bar is wide enough to carry its mark, its caption
        // and its grips/✕ (`fx_band::GRIP_MIN_PX` / `KILL_MIN_PX`).
        let mut lane = crate::snapshot_lane::staged_cut();
        // §2's gutter holds a sound switch per cut LANE (`refresh_camera_rows` draws one per `cut.lanes`),
        // and `spec/img/06-lane.png` shows those round 🔊 buttons at each row's left edge — so the shot needs
        // a lane or the gutter has nothing to show.
        lane.lanes = vec![crate::cut::Lane {
            name: "mic-lav".to_string(),
            src: String::new(),
            at: 0.0,
            off: 0.0,
            dur: 300.0,
        }];
        ui::seed_review_cut(window, &lane);
        ui::set_line_position(window, cut_line::LinePos { t: 140.0 });
        ui::press_effect_bar(window, 0, 100.0, 200.0, 20.0, false);
        crate::snapshot_face::seed_snapshot_frame(dir, "session-tape", 5.0);
        ui::cut_picture::set_preview_camera_base("session-tape");
        ui::cut_picture::set_snapshot_dir(dir);
        ui::set_session_recordings(&[crate::timeline::Recording {
            base: "session-tape".to_string(),
            start: 0.0,
            end: 300.0,
        }]);
        ui::set_playhead(window, 5.0);
        // §2's picture is the TRACK STACK as a user sees it, and its gutter controls are widgets, so the asked
        // file is cropped from the page (`spec/QUESTIONS.md` answer 3(a)) rather than repainted on a bare cairo
        // surface.
        if let Err(why) = crate::snapshot_lane::stage_envelope(dir, "session-tape") {
            eprintln!("snapshot: 06-lane wave cache | {why}");
        }
        // THIS is the shot asking for film, not the live page: §7 forbids a decode inside a draw because a person
        // would feel it, and a one-shot snapshot process has nobody waiting.
        ui::cut_tracks::allow_frame_decoding();
        ui::cut_view::repaint_page(window);
        // §2's gutter shows a round 🔊 sound switch at each lane's left edge, and those are built by the ROW
        // pass (`refresh_camera_rows`), which the band repaint above does not run.
        ui::refresh_camera_rows(window);
        let painted = ui::cut_picture::refresh(window);
        println!(
            "snapshot: 06-lane | held zoom: {} | frame painted: {painted}",
            ui::held_effect().is_some()
        );
        return true;
    }

    false
}
