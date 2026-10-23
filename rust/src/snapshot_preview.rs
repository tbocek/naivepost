//! The `06-preview` screen (§06-effects#2): the preview DURING a zoom and a caption. The shared fixture
//! cannot show either record, so they are staged here the way `06-lane` stages its bars, and the caption
//! lands through ❝ Text's own doors rather than being pushed in. Split out of `snapshot.rs`, which is past
//! the size another staging block fits in.

use std::path::Path;

use adw::prelude::*;
use gtk4 as gtk;

use crate::cut;
use crate::cut_line;
use crate::ui;

use crate::snapshot_face::seed_snapshot_frame;

/// The staged zoom's camera, as fractions of the source frame: off-centre left and tighter than the whole frame,
/// which is what gives §2's picture a crop with surround to dim (see the note where they are written).
const ZOOM_CX: f64 = 0.34;
const ZOOM_CY: f64 = 0.42;
const ZOOM_HF: f64 = 0.62;

/// Paint the preview mid-zoom-and-caption onto `window`. Returns true when this module owned `screen`.
/// `out` is the PNG this run writes; §2's own frame image goes beside it as `<name>-frame.png`.
pub(crate) fn paint(window: &adw::ApplicationWindow, dir: &Path, out: &Path, screen: &str) -> bool {
    // §06-effects#2's own screen (`spec/img/06-preview.png`): the preview DURING a zoom and a caption at
    // 1:22 — the camera rect framing the lecturer, the caption fitted into its box, the plate a delayed
    // sound earns. The fixture cannot show any of it (its `cut/cut.json` has `fx: []`), so the two records
    // are seeded here the way `06-lane` seeds its bars, and the caption is then placed through ❝ Text's own
    // doors rather than pushed in: what the panel paints must be what the app would paint.
    if screen == "06-preview" {
        let staged = cut::Cut {
            segs: vec![cut::Seg {
                s: 0.0,
                e: 300.0,
                cam: 0,
                ..Default::default()
            }],
            fx: vec![
                // A zoom standing at the line's second, framed off-centre and tighter than the full frame:
                // `fx_zoom::SETTLED` is hf 1.0, which hides nothing, so the dim would have no bands outside its
                // rect and the white stroke would fall on the panel's own edge. hf is the height FRACTION of
                // the footage and the rect takes the same share of the width, so it cannot go smaller for free:
                // under `fx_lane::PLATE_MIN_PX` of width `fx_lane::room_px` answers "no room", the debt plate
                // hides, and §06's own caption for this screen — "the caption is fitted into its box" — needs
                // that plate in the shot. Off-centre so the rect reads as a crop, not as a border.
                // `hf` is the height FRACTION of the SOURCE, and §2's picture crops a head-and-shoulders shot out
                // of the left half of the frame (the lecturer stands left of centre there), so the centre sits left
                // of the middle and well under half the height. At hf 0.6 the rect filled nearly the whole plate,
                // almost nothing outside it was left to dim, and the shot read as a border, not as a crop. Named
                // because the caption's box below is measured against this rect, not against a remembered pixel.
                cut::Fx {
                    kind: "zoom".to_string(),
                    t: 78.0,
                    dur: 12.0,
                    trans: 2.0,
                    cx: Some(ZOOM_CX),
                    cy: Some(ZOOM_CY),
                    hf: Some(ZOOM_HF),
                    ..Default::default()
                },
                // A speed whose sound runs to the scene's end: the one answer that draws the dashed debt
                // tail (§2), with enough debt on it to clear `fx_lane::DEBT_TAIL_MIN_SECONDS`. `trans` is
                // spelled out because a default of 1 s would make the first second of the record a fade, and
                // the plate's debt is read at the line's second (82.0), five seconds into this bar.
                cut::Fx {
                    kind: "speed".to_string(),
                    t: 77.0,
                    dur: 6.0,
                    trans: 0.0,
                    tout: 0.0,
                    rate: 0.5,
                    snd: "scene".to_string(),
                    ..Default::default()
                },
                // And an effect-end one beside it, which gets NO tail: the difference is the point.
                cut::Fx {
                    kind: "speed".to_string(),
                    t: 96.0,
                    dur: 6.0,
                    trans: 0.0,
                    tout: 0.0,
                    rate: 0.5,
                    snd: "own".to_string(),
                    ..Default::default()
                },
                // And a STOP standing on the spec's own second (1:22), which is what §2's "a stop's still:
                // rendered from the scene's own camera, fitted on the footage's transform" is about: the frame
                // under the line becomes a held picture instead of a bare white outline. `rate: 0.0` IS the stop
                // (§06#1: a stop is a speed whose rate is nought), and its seconds straddle 82.0 so the line falls
                // inside it.
                cut::Fx {
                    kind: "speed".to_string(),
                    t: 81.0,
                    dur: 3.0,
                    trans: 0.0,
                    tout: 0.0,
                    rate: 0.0,
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        ui::seed_review_cut(window, &staged);
        ui::set_session_recordings(&[crate::timeline::Recording {
            base: "session-tape".to_string(),
            start: 0.0,
            end: 300.0,
        }]);
        // Badge **1** needs a frame to show, and the demo fixture ships no extracted frames, so the shot
        // writes one real PNG into the folder the page lists them from — the same door `05-cut` uses.
        seed_snapshot_frame(dir, "session-tape", 82.0);
        ui::cut_picture::set_preview_camera_base("session-tape");
        // Without the folder the frame lives in, `refresh` has nothing to decode and the panel paints its
        // rect over an empty plate (`05-cut` sets the same door).
        ui::cut_picture::set_snapshot_dir(dir);
        // The spec's second: 1:22, four seconds into the zoom and inside the scene-end speed's seconds.
        ui::set_line_position(window, cut_line::LinePos { t: 82.0 });
        ui::set_playhead(window, 82.0);
        // The plate's two conditions are both asked of the page in `refresh_effects_lane`: the room
        // (`fx_lane::room_px` × `PLATE_MIN_PX`) and the debt (`fx_lane::plate_debt_at` over this cut at the
        // line's second). Nothing extra is poked here — the scene-end speed staged above is what makes the
        // debt true at 1:22 (5 s of its 6 s run by then), so the sentence arrives through the same door a
        // real session's line does.
        // The caption through its real doors: arm ❝ Text, land a box on the panel, type, Apply. Apply
        // writes through `record_edit`, so this is the caption the lane and the frame both already hold.
        // Dragged with the line at 81 s, not 82 s: a caption's first 0.3 s is its fade-in
        // (`fx_lane::visibility_at` answers 0 at the exact second it starts), so one placed ON 1:22 would be
        // painted at alpha 0 in the very shot §06 shows it visible in. Landing it a second earlier puts the
        // whole fade BEFORE the spec's second, so 1:22 sees the finished caption — the drag still decides the
        // seconds, as it does for a hand.
        // The box is dragged in the PANEL's px, not the shot's — `text_drag_ended` measures against
        // `PREVIEW_PANEL_W/H` (240 x 135 headless) and stores fractions of it, so these numbers are panel px.
        // They land the caption low inside the camera rect (§F3.4 puts a caption on the OUTPUT frame; §2's picture
        // shows the words inside the crop, across its lower third), wide and shallow so the sentence fits on one
        // line as it does there — a narrow box wraps it to two and the second line falls outside the rect.
        ui::set_line_position(window, cut_line::LinePos { t: 81.0 });
        ui::note_place(true);
        let landed = ui::press_text_item(window);
        let _ = landed;
        ui::text_drag_ended(window, (40.0, 50.0), (128.0, 61.0));
        // The drag landed the box on the FORM, and Apply is what copies it onto the record — so the form's own
        // box has to be moved before Apply: a click (and any drag that GTK's headless panel cannot measure) leaves
        // §F3.4 S2's default lower third (cx 0.5, cy 0.78) on the form, which at a tight camera rect falls BELOW
        // the crop and gets dimmed with everything outside it. §2's picture shows the words inside the crop, low
        // across it, so the box goes there through `nudge_text_box` (§F3.4 S5's own door), where its 10 px snap to
        // the finished frame's centre line is what lands it ON that line — as it would for a hand.
        // The box the caption Apply writes: `text_drag_ended` only puts the dragged rectangle on the FORM, and
        // Apply copies the form's box onto the record, so the box is set here through §F3.4 S5's own nudge door.
        // Two numbers, both asked of the doors rather than guessed: the default lower third (cx 0.5, cy 0.78) lands
        // BELOW a tight camera rect, where the painter dims the words with everything outside the crop, while §2's
        // picture shows them inside it, in its own lower third — 70% of the way down the CROP, not the frame.
        let caption_cy = ZOOM_CY - ZOOM_HF / 2.0 + 0.7 * ZOOM_HF;
        let panel_px_per_fraction = crate::ui::cut_zoom_hand::PREVIEW_PANEL_H;
        let moved_box = ui::nudge_text_box(
            window,
            None,
            (
                0.0,
                (caption_cy - crate::fx_text::LOWER_THIRD.cy) * panel_px_per_fraction + 11.0,
            ),
        );
        println!("snapshot: 06-preview | caption box: {moved_box}");
        let after = ui::review_cut_of(window);
        println!(
            "snapshot: 06-preview | staged cut: {} records",
            after.fx.len()
        );
        if let Some(view) =
            crate::ui::window::find_widget_by_name(window.upcast_ref(), "text-field-words")
                .and_then(|w| w.downcast::<gtk::TextView>().ok())
        {
            view.buffer().insert_at_cursor("Ethereum in one lecture");
        }
        let said = ui::press_text_apply(window);
        println!("snapshot: 06-preview | {said}");
        // Back to the spec's own second now that the caption is on the cut: everything after this line reads
        // the plate, the debt and the camera at 1:22, which is where §06's screenshot stands.
        ui::set_line_position(window, cut_line::LinePos { t: 82.0 });
        ui::refresh_effects_lane(window);
        // The shot is taken with NOTHING in hand: §2 gives the full-alpha-plus-dashed-outline treatment to the
        // held record only, and a frozen dashed outline would claim a hand that is not there. What makes the
        // words show complete at 1:22 is the landing a second earlier (above) — their real alpha is 1 there,
        // so the painter needs no special case to draw the fitting §S4 promises.
        // The frame's own picture too: `cut_picture::refresh` is the only door that decides which recorded
        // second sits behind the dim, so without it the panel paints its rect over an empty plate.
        let _ = ui::cut_picture::refresh(window);
        // §2's second image is the FRAME, not the page: painted on its own at 960x540 through the preview's own
        // painters (dim, stroked rect, caption, stop's still) so each of them actually reads, beside the window
        // shot as `<name>-frame.png`. Paused, because the spec's picture is the red line standing on a second.
        let still = ui::cut_picture::frame_shown();
        // §2's second image is the FRAME, not the page: painted on its own at 960x540 through the preview's own
        // painters (dim, stroked rect, caption, stop's still) so each of them actually reads.
        // The still has to be shown at the camera's OWN box -- "rendered from the scene's own camera, fitted on
        // the footage's transform" -- which only means anything if the frame behind it has structure: over a flat
        // staged colour a crop of it and all of it are the same pixels. So this screen stages the structured frame
        // (three bands + one bright block) and paints THAT; `frame_shown()` stays the fallback for when nothing
        // could be written, so the shot still lands rather than going out blank.
        let picture = crate::snapshot_face::seed_structured_frame(dir, "session-tape", 82.0)
            .or_else(|| still.clone());
        match crate::snapshot_preview_frame::write_png(
            &staged_with_caption(window),
            82.0,
            picture.as_deref(),
            false,
            &out.with_extension("frame.png"),
        ) {
            Ok(said) => println!("snapshot: 06-preview | {said}"),
            Err(why) => eprintln!("snapshot: 06-preview frame image | {why}"),
        }
        crate::ui::cut_view::repaint_page(window);
        return true;
    }

    false
}

/// The cut the preview shot shows, read back from the window AFTER the caption was placed through ❝ Text's own
/// doors — so the frame image carries the record the app wrote, not a second copy of the staging.
fn staged_with_caption(window: &adw::ApplicationWindow) -> cut::Cut {
    ui::review_cut_of(window)
}
