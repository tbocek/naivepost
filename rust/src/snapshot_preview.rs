//! The `06-preview` screen (§06-effects#2): the preview DURING a zoom and a caption. The shared fixture
//! cannot show either record, so they are staged here the way `06-lane` stages its bars, and the caption
//! lands through ❝ Text's own doors rather than being pushed in. Split out of `snapshot.rs`, which is past
//! the size another staging block fits in.

use std::path::Path;

use adw::prelude::*;
use gtk4 as gtk;

use crate::cut;
use crate::cut_line;
use crate::fx_text;
use crate::ui;

use crate::snapshot_face::seed_snapshot_frame;

/// The staged zoom's camera, as fractions of the source frame: `hf` is the height fraction and the rect takes the
/// same share of the width, so these three numbers are the whole crop (§2's `camera_rect_px`).
///
/// Near-full-frame and centred, because §2's own picture shows the crop filling the plate with only thin bars left
/// outside it: a tighter crop than the picture shows dims away most of the frame, which is what an earlier shot of
/// this screen did (a rect in the upper left, wide dim margins and a separate dark band below).
///
/// `hf` is a fraction of the FITTED footage, and the plate's room is asked in those same px
/// ([`crate::fx_lane::room_px`] against [`crate::fx_lane::PLATE_MIN_PX`]), so the surround this crop leaves is what
/// decides whether §2's paired plate is drawn at all. The footage in a headless shot is the staged frame —
/// [`crate::snapshot_face::STRUCTURED_FRAME_W`] x [`crate::snapshot_face::STRUCTURED_FRAME_H`], and the plate is told
/// that shape (`note_plate_shape`) rather than left on the declared panel — so "thin bars" is a number here: a
/// SURROUND_SHARE of the frame's own width and height outside the rect, which on a 960 px frame leaves the plate four
/// times its minimum room while still dimming something, as §2's half-paused picture does.
const ZOOM_CX: f64 = 0.5;
const ZOOM_CY: f64 = 0.5;
/// How much of the frame's width and height stays OUTSIDE the crop: one eighth, i.e. `hf` = 7/8.
const SURROUND_SHARE: f64 = 0.125;
const ZOOM_HF: f64 = 1.0 - SURROUND_SHARE;

/// The caption's box, as fractions of the FINISHED frame (§F3.4 stores box fractions, and the painter lays them out
/// on the fitted footage). Width and height are NOT the §F3.4 S2 default: `fx_text::fit` fits the words into whatever
/// box they are given, and a default 0.8 x 0.16 box at this plate's shape wraps "Ethereum in one lecture" onto two
/// lines, the second of which falls below the crop and is dimmed with everything outside it. §2's picture shows the
/// sentence on ONE line inside the crop, so the box is one line wide and shallow, and it is centred
/// `CAPTION_IN_CROP` of the way DOWN THE CROP rather than down the frame — which is where the spec's words sit, wholly
/// inside the stroked rect. A hand gives a caption its box by dragging one; this shot drags one too (see the
/// `text_drag_ended` call in `paint`), it just drags it where §2's picture has it.
const CAPTION_WF: f64 = 0.62;
const CAPTION_HF: f64 = 0.09;
/// How far down the CROP the caption's centre sits. Two things set it, and the two ends of the crop are taken:
/// the plate §2 pairs with the stroked rect is laid at the rect's OWN middle, so a caption centred there sits under
/// the plate's box; and §2's picture shows the words across the crop's lower third — a third of the way up from its
/// bottom edge, which is the same 2/3 mark read from the top.
const CAPTION_IN_CROP: f64 = 2.0 / 3.0;

/// The caption's box in fractions, derived from the staged camera above rather than from a remembered pixel, so the
/// two cannot drift: move the zoom and the caption follows it inside the new rect.
///
/// A `fn` and not a `const` because this file has no `fx_text` items to compare a `const` against at compile time;
/// nothing about it depends on the window, so every caller gets the same box.
pub fn caption_box() -> fx_text::Box_ {
    fx_text::Box_ {
        cx: 0.5,
        cy: (ZOOM_CY - ZOOM_HF / 2.0) + CAPTION_IN_CROP * ZOOM_HF,
        wf: CAPTION_WF,
        hf: CAPTION_HF,
    }
}

/// The cut this shot stands on, BEFORE the caption is placed: the zoom, the two speeds (one scene-end, one
/// effect-end) and the stop, at the second §06's screenshot names. A `pub fn` so a check can measure the same scene
/// the shot paints — the rect, the plate's debt and the caption's box all follow from these records — instead of
/// writing a second copy of them that could drift.
///
/// §06-effects#2's own screen (`spec/img/06-preview.png`) is the preview DURING a zoom and a caption at 1:22: the
/// camera rect framing the lecturer, the caption fitted into its box, the plate a delayed sound earns. The fixture
/// cannot show any of it (its `cut/cut.json` has `fx: []`), so the records are staged here the way `06-lane` stages
/// its bars, and the caption is then placed through ❝ Text's own doors rather than pushed in.
pub fn staged_cut() -> cut::Cut {
    cut::Cut {
        segs: vec![cut::Seg {
            s: 0.0,
            e: 300.0,
            cam: 0,
            ..Default::default()
        }],
        fx: vec![
            // A zoom standing at the line's second, gliding in over 2 s so the line lands on the settled crop and
            // not mid-glide. Its fractions are [`ZOOM_CX`]/[`ZOOM_CY`]/[`ZOOM_HF`], and the rect takes the same share
            // of the width as of the height, so `fx_zoom::SETTLED`'s 1.0 is the one answer that hides nothing: with
            // nothing outside the rect to dim, `fx_lane::room_px` never runs short and the debt plate — which §2
            // pairs with the stroked rect, and which is why `ZOOM_HF` stops just under 1.0 — would never be drawn.
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
            // A speed whose sound runs to the scene's end: the one answer that draws the dashed debt tail (§2),
            // with enough debt on it to clear `fx_lane::DEBT_TAIL_MIN_SECONDS`. `trans` is spelled out because a
            // default of 1 s would make the first second of the record a fade, and the plate's debt is read at the
            // line's second (82.0), five seconds into this bar.
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
            // And a STOP standing on the spec's own second (1:22), which is what §2's "a stop's still: rendered from
            // the scene's own camera, fitted on the footage's transform" is about: the frame under the line becomes
            // a held picture instead of a bare white outline. `rate: 0.0` IS the stop (§06#1: a stop is a speed
            // whose rate is nought), and its seconds straddle 82.0 so the line falls inside it.
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
    }
}

/// The second §06's screenshot stands on: 1:22, four seconds into the staged zoom and inside the stop's seconds.
pub const LINE_SECONDS: f64 = 82.0;

/// The second this screen's frame image is painted at. The caption is placed through the same doors at the same
/// second, so the page shot and the frame image cannot show one record in two places.
pub const SHOT_SECONDS: f64 = LINE_SECONDS;



/// Paint the preview mid-zoom-and-caption onto `window`. Returns true when this module owned `screen`.
/// `out` is the PNG this run writes; §2's own frame image goes beside it as `<name>-frame.png`.
pub(crate) fn paint(window: &adw::ApplicationWindow, dir: &Path, out: &Path, screen: &str) -> bool {
    if screen == "06-preview" {
        let staged = staged_cut();
        ui::seed_review_cut(window, &staged);
        ui::set_session_recordings(&[crate::timeline::Recording {
            base: "session-tape".to_string(),
            start: 0.0,
            end: 300.0,
        }]);
        // Badge **1** needs a frame to show, and the demo fixture ships no extracted frames, so the shot
        // writes one real PNG into the folder the page lists them from — the same door `05-cut` uses.
        // Badge **1** needs a frame to show, and the demo fixture ships no extracted frames, so the shot
        // writes one real PNG into the folder the page lists them from — the same door `05-cut` uses.
        seed_snapshot_frame(dir, "session-tape", LINE_SECONDS);
        ui::cut_picture::set_preview_camera_base("session-tape");
        // Without the folder the frame lives in, `refresh` has nothing to decode and the panel paints its
        // rect over an empty plate (`05-cut` sets the same door).
        ui::cut_picture::set_snapshot_dir(dir);
        // The spec's second: 1:22, four seconds into the zoom and inside the scene-end speed's seconds.
        ui::set_line_position(window, cut_line::LinePos { t: LINE_SECONDS });
        ui::set_playhead(window, LINE_SECONDS);
        // The caption through its real doors: arm ❝ Text, land a box on the panel, type, Apply. Apply writes through
        // `record_edit`, so this is the caption the lane and the frame both already hold.
        //
        // Dragged with the line at 81 s, not 82 s: a caption's first 0.3 s is its fade-in
        // (`fx_lane::visibility_at` answers 0 at the exact second it starts), so one placed ON 1:22 would be painted
        // at alpha 0 in the very shot §06 shows it visible in. Landing it a second earlier puts the whole fade BEFORE
        // the spec's second, so 1:22 sees the finished caption — the drag still decides the seconds, as it does for a
        // hand.
        //
        // The box is a real DRAG, in the panel's own px (`text_drag_ended` measures against `PREVIEW_PANEL_W/H`,
        // 240 x 135 headless, and stores fractions of that frame): its corners are the §2 crop's own, taken from
        // [`ZOOM_CX`]/[`ZOOM_HF`]/[`caption_box`] as fractions of the panel, so the caption lands inside the crop the
        // zoom takes rather than at a remembered pixel. Wide and shallow on purpose — a narrow box wraps the sentence
        // to two lines and the second line falls outside the rect.
        //
        // …but the fractions are only read back to px against the plate's AGREED shape (`cut_picture::plate_shape`,
        // which the page's own refresh fills from the first frame it can measure), so the page's plate is told what
        // this shot's picture actually is first: the structured frame `snapshot_preview_frame` decodes for its own
        // 960x540 image, at the same second. Without that the plate falls back to the declared panel shape and the
        // same fractions become different pixels in the two images — which is how a caption ends up inside the rect in
        // one and running past its edge in the other.
        let staged_frame =
            crate::snapshot_face::seed_structured_frame(dir, "session-tape", LINE_SECONDS);
        if let Some(shape) = staged_frame
            .as_deref()
            .and_then(|path| gtk::gdk_pixbuf::Pixbuf::from_file(path).ok())
            .map(|pixbuf| (pixbuf.width(), pixbuf.height()))
        {
            ui::cut_picture::note_plate_shape(window, shape);
        }
        ui::set_line_position(window, cut_line::LinePos { t: 81.0 });
        ui::note_place(true);
        let landed = ui::press_text_item(window);
        let _ = landed;
        let panel = (
            crate::ui::cut_zoom_hand::PREVIEW_PANEL_W,
            crate::ui::cut_zoom_hand::PREVIEW_PANEL_H,
        );
        let box_px = caption_box().to_px(panel.0, panel.1);
        let landed_box = ui::text_drag_ended(
            window,
            (box_px.0, box_px.1),
            (box_px.0 + box_px.2, box_px.1 + box_px.3),
        );
        println!("snapshot: 06-preview | caption box: {landed_box}");
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
        ui::set_line_position(window, cut_line::LinePos { t: LINE_SECONDS });
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
        let picture = crate::snapshot_face::seed_structured_frame(dir, "session-tape", LINE_SECONDS)
            .or_else(|| still.clone());
        match crate::snapshot_preview_frame::write_png(
            &staged_with_caption(window),
            SHOT_SECONDS,
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
