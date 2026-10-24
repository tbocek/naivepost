//! The F3 effect screens (`06-svg` ... `06-lane`), each staged through the doors its own widgets fire rather
//! than by poking state, so the shot shows what a press earned. Split out of `snapshot.rs`, which is past the
//! size another staging block fits in; every rule below lives in the module whose screen it draws.

use adw::prelude::*;

use crate::cut;
use crate::cut_line;
use crate::ui;

/// Paint one of the effect screens onto `window`. Returns true when this module owned `screen`.
/// `_out` is unused here: the one screen of this group whose subject cannot live inside a window shot (`06-lane`)
/// has its file written later, at the frame-taking step in `snapshot::run`, which is the first moment the window
/// has a render node to crop — see [`crate::snapshot_lane::paint_track_area`].
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
        // No Apply here: Apply closes the form, and the screen F3.5 names IS the form. The record it would
        // write is proven by `tests/suite/svg_drawing_widgets.rs`; the shot shows what the user is being asked.
        return true;
    }

    // F3.1's screen is the zoom form OPEN (`spec/img/06-zoom.png`), and like `06-volume`/`06-svg` the shot
    // must reach it through the doors the widgets fire — arm (⊕ Zoom), a box on the preview, the form that
    // lands — rather than by poking state. The fixture seeds no red line of its own, so without these two
    // lines the arm refuses (§F3.1 S1: "click a track first") and the shot was the bare Cut page with no
    // form in it at all.
    if screen == "06-zoom" {
        ui::note_place(true);
        ui::note_zoom_place(true);
        ui::set_line_position(window, cut_line::LinePos { t: 81.0 });
        let said = ui::press_zoom_item(window);
        assert!(
            ui::zoom_armed(),
            "snapshot: 06-zoom could not arm -- {said}"
        );
        // A drawn box, fired through the panel's own gesture exactly as `tests/suite/zoom_drag_widgets.rs` does:
        // begin at the press point, release with the OFFSET that signal carries.
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
    // dropdown row, which is what `tests/suite/speed_form_widgets.rs` presses. Without this the shot was the
    // bare Cut page. The spec's own caption for that image reads `Speed ×0.5 on a selected stretch`, so
    // it is §F3.3's hand form, and nothing about §F3.10's pass belongs in this shot.
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
    // and a zoom together. No `spec/img/06-flows.png` exists, so this is a program-side screen like `06-speeds`
    // and `06-decorations`, driven through the page's own doors and never by poking state:
    //   1. a kept two-clip cut (the fixture keeps one clip and `fx: []`, which answers "nothing to ask about");
    //   2. the three `run_*_pass_with_reply` doors — scripted for the TELEPHONE only. Every rule after the reply
    //      (membership, the caption refusal, the app's own decoration defaults) and every write (`record_edit`,
    //      the lane repaint, the status row) is the pass's own, called exactly as the run after the cut calls it;
    //   3. the count is read back off the page, not off the returned sentences, so a shot that printed nothing
    //      reaching the cut would fail rather than print a number anyway.
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
        // starts them (§2's run bar holds only ▶ / lucky / ⏹ / the bar and the readouts), and a pass that placed
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
    // pass's sentence on the status row. `spec/img` has no speed-pass image (the file named `06-speed.png`
    // is §F3.3's hand form), so this is a program-side screen like `05-suggest` / `06-lane`, driven the
    // same way — through the page's own doors, never by poking state:
    //   1. a kept two-clip cut so the pass has a batch to ask about (the fixture's `cut.json` keeps one
    //      clip and `fx: []`, which answers "nothing to ask about");
    //   2. `run_speeds_pass_with_reply` — the scripted door for the TELEPHONE only. Every rule after the
    //      reply (membership, the caption refusal, the gap merge, both log lines) and every write
    //      (`record_edit`, the lane repaint, the status row) is the pass's own, called exactly as the
    //      button calls it;
    //   3. the lane, the camera rows and the readouts repaint inside that call, as a click does.
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
        // Read back from the page rather than from the returned sentence: a shot that printed only what the
        // call handed back would still print it if nothing reached the cut or the status row.
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
    //   1. the same kept two-clip cut (the fixture keeps one clip and `fx: []`, which answers "nothing to
    //      ask about");
    //   2. `run_effects_pass_with_reply` — the scripted door for the TELEPHONE only, answering all three
    //      kinds at once so the lane shows each of their colours: a zoom on clip 1, a stop on clip 2, a
    //      volume with a gain on clip 2's tail. The model names no centre and no ramp because §F3.11 asks it
    //      for none — the app places those. Every rule after the reply (membership, the app's own defaults,
    //      both log lines) and every write (`record_edit`, the lane repaint, the status row) is the pass's
    //      own, called exactly as the ✨ Decorations button calls it;
    //   3. the lane and the readouts repaint inside that call, as a click does.
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
        // Read back from the page rather than from the returned sentence: a shot that printed only what the
        // call handed back would still print it if nothing reached the cut or the status row.
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

    // the dropdown row fires (`tests/suite/volume_form_widgets.rs` presses exactly this) rather than poking state.
    if screen == "06-volume" {
        // A marked stretch, which is what §F3.6's own heading reads: the spec's picture says
        // `Volume 1:21 – 1:25`, four seconds, and a band is how a person gets exactly that (a bare line
        // gives the form `LINE_SECONDS` from the line instead). Fired through the page's selection seam,
        // the same one a drag leaves behind — not by poking state.
        ui::draw_selection(
            window,
            crate::cut_select::Surface::SelectionBand,
            None,
            81.0,
            85.0,
        );
        let said = ui::press_volume_item(window);
        // No Apply here either: Apply closes the form, and the screen §F3.6 names IS the form. The record
        // it would write, and that one ↶ takes back, is proven by `tests/suite/volume_form_widgets.rs`.
        let _ = said;
        return true;
    }

    // F3.7's screen is the label form OPEN (`spec/img/06-label.png`), so the shot drives the same door the
    // dropdown row fires (`tests/suite/label_form_widgets.rs` presses exactly this) rather than poking state.
    if screen == "06-label" {
        // A placed line 81 s in, so the heading names that moment -- spelled by `fx_label::form_title`,
        // never hardcoded here. The Name field shows empty because the name is the question the form asks:
        // an unnamed mark is refused at Apply ("nothing is placed until then"), so prefilling one would put
        // a word on the lane nobody typed.
        ui::note_place(true);
        ui::set_line_position(window, cut_line::LinePos { t: 81.0 });
        let said = ui::press_label_item(window);
        // No Apply here either: Apply closes the form, and the screen §F3.7 names IS the form. The mark it
        // would write, and that one ↶ takes back, is proven by `tests/suite/label_form_widgets.rs`.
        let _ = said;
        return true;
    }

    // §06-effects#2's Effect-menu screen (`spec/img/06-effect-menu.png`): the ✚ Effect dropdown OPEN, showing
    // its six rows in `fx_lane::MENU_ORDER`. Nothing drew it before -- the screen name existed so the toolbar
    // guard would accept it, and the shot came out byte-identical to a bare Cut page, popover never popped.
    if screen == "06-effect-menu" {
        let rows = crate::snapshot_effect_menu::stage(window);
        let want = crate::snapshot_effect_menu::labels_in_menu_order();
        // Compare the ORDER of the labels, read off the widget tree, against the order the spec gives. The row
        // strings carry names too (the caller prints them), but the spec's claim is about the sequence a person
        // reads, so that is what is asserted here — after the header, which is not one of the six.
        let got: Vec<String> = rows
            .iter()
            .skip(1)
            .map(|row| row.split(": ").nth(1).unwrap_or("").to_string())
            .collect();
        assert_eq!(
            got, want,
            "snapshot: 06-effect-menu's popover is not in fx_lane::MENU_ORDER"
        );
        // A popover is its own surface, so it is not in the window's render node: the shot is painted from the
        // dropdown's own box at the frame-taking step (see `snapshot::run`'s `menu_only` branch).
        crate::snapshot_effect_menu::note_shot_sidecar();
        println!("snapshot: 06-effect-menu | {}", rows.join(" | "));
        return true;
    }

    // §F3.4's own picture (`spec/img/06-text.png`) is TWO things at once: the caption ON the preview, across the
    // lower third, and the `Text at m:ss` form beside it with the words in its field. The staging lives in
    // `snapshot_text` (see its header): the spec's state spans two live states -- the placed record Apply writes,
    // and the open form Apply closes -- so it is more than one call, and this file is past the size for it.
    if screen == "06-text" {
        let said = crate::snapshot_text::stage(window, dir);
        println!("snapshot: 06-text | {}", said.join(" | "));
        return true;
    }

    // its `cut/cut.json` carries `fx: []`, so every shot of the Cut page draws an empty lane. Seeded here
    // the way `05-fold` seeds its hole -- the shared fixture file untouched -- and driven through the same
    // doors the widgets fire (`tests/suite/band_hold_widgets.rs` presses exactly these) rather than by poking
    // state: a lane painted from a shortcut would not be the lane the app draws.
    if screen == "06-lane" {
        // One staging for both images of this screen — the window shot and the 1240 x 200 lane beside it: §2's
        // list (a zoom, a staying zoom, a ×0.5 scene-sound speed, a caption, a volume, a label) at the same
        // 4 px/s the spec's own lane is drawn with, so every bar is wide enough to carry its mark, its caption
        // and its grips/✕ (`fx_band::GRIP_MIN_PX` / `KILL_MIN_PX`). See `snapshot_lane::staged_cut`.
        let lane = crate::snapshot_lane::staged_cut();
        ui::seed_review_cut(window, &lane);
        ui::set_line_position(window, cut_line::LinePos { t: 140.0 });
        // Pick the first band UP through the press seam (a middle grab, 20 px of travel), so the shot shows
        // the held state: the white outline, the line walked to the band's start, and the
        // "Zoom picked up" sentence the lane prints.
        ui::press_effect_bar(window, 0, 100.0, 200.0, 20.0, false);
        // The plate needs a real frame under it and the line to stand on it: the demo fixture ships no
        // extracted frames, so one PNG is written where the page lists them (the door `05-cut`, `06-preview`
        // and `06-text` use). Without it the panel paints its dimmed camera rect over an empty plate and the
        // held zoom has nothing to frame -- the shot's whole point. The line is the pick-up's own second
        // (the band's start, 5.0 s), NOT the pre-pick-up 140.0 above: S1 walks the line to the band's start,
        // and a held effect whose band no longer covers the line would be dropped by the very next refresh.
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
        // surface. The wave strip needs the envelope cache; the picture row already has what it needs — the frames
        // this fixture ships under `prepare/inputs/frames/session-tape`.
        // The CROP itself cannot happen here — the window has not been mapped yet at this point in `run`, so it has
        // no render node; it happens at the frame-taking step, beside the `lane` subject door.
        if let Err(why) = crate::snapshot_lane::stage_envelope(dir, "session-tape") {
            eprintln!("snapshot: 06-lane wave cache | {why}");
        }
        // THIS is the shot asking for film, not the live page: §7 forbids a decode inside a draw because a person
        // would feel it, and a one-shot snapshot process has nobody waiting. Asked before the repaint below so the
        // pass that produces this shot's pixels is the one that pays for it.
        ui::cut_tracks::allow_frame_decoding();
        // Repaint through the page's own doors, exactly as a press's refresh does, so the plate, the dimming
        // and the zoom's rect are the app's answer to the state above rather than a half-painted frame.
        ui::cut_view::repaint_page(window);
        let painted = ui::cut_picture::refresh(window);
        println!(
            "snapshot: 06-lane | held zoom: {} | frame painted: {painted}",
            ui::held_effect().is_some()
        );
        return true;
    }

    false
}
