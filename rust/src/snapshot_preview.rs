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
const ZOOM_CX: f64 = 0.5;
const ZOOM_CY: f64 = 0.5;
const SURROUND_SHARE: f64 = 1.0 / 16.0;
const ZOOM_HF: f64 = 1.0 - SURROUND_SHARE;

pub fn caption_box() -> fx_text::Box_ {
    fx_text::LOWER_THIRD
}

pub fn staged_cut() -> cut::Cut {
    cut::Cut {
        segs: vec![cut::Seg {
            s: 0.0,
            e: 300.0,
            cam: 0,
            ..Default::default()
        }],
        fx: vec![
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
pub(crate) fn paint(window: &adw::ApplicationWindow, dir: &Path, out: &Path, screen: &str) -> bool {
    if screen == "06-preview" {
        let staged = staged_cut();
        ui::seed_review_cut(window, &staged);
        ui::set_session_recordings(&[crate::timeline::Recording {
            base: "session-tape".to_string(),
            start: 0.0,
            end: 300.0,
        }]);
        seed_snapshot_frame(dir, "session-tape", LINE_SECONDS);
        ui::cut_picture::set_preview_camera_base("session-tape");
        ui::cut_picture::set_snapshot_dir(dir);
        ui::set_line_position(window, cut_line::LinePos { t: LINE_SECONDS });
        ui::set_playhead(window, LINE_SECONDS);
        // The caption through its real doors: arm ❝ Text, land a box on the panel, type, Apply. Apply writes through
        // `record_edit`, so this is the caption the lane and the frame both already hold.
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
        let _ = ui::cut_picture::refresh(window);
        // §2's second image is the FRAME, not the page: painted on its own at 960x540 through the preview's own
        // painters (dim, stroked rect, caption, stop's still) so each of them actually reads, beside the window
        // shot as `<name>-frame.png`. Paused, because the spec's picture is the red line standing on a second.
        let still = ui::cut_picture::frame_shown();
        // §2's second image is the FRAME, not the page: painted on its own at 960x540 through the preview's own
        // painters (dim, stroked rect, caption, stop's still) so each of them actually reads.
        let picture =
            crate::snapshot_face::seed_structured_frame(dir, "session-tape", LINE_SECONDS)
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
