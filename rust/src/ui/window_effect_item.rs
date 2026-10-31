//! §06-effects#1-record: the effect dropdown's record door. Split out of `window.rs`, which is past its
//! size budget; the door is re-exported through `window` so every caller keeps the path it had.

use crate::cut::{self};
use crate::fx_record;
use crate::ui::cut_label_hand::press_label_item;
use crate::ui::cut_speed_hand::press_speed_item;
use crate::ui::cut_svg::press_svg_item;
use crate::ui::cut_text_hand::press_text_item;
use crate::ui::cut_volume_hand::press_volume_item;
use crate::ui::cut_zoom_hand::press_zoom_item;
use crate::ui::window::{
    paste_line, record_edit, refresh_effects_lane, review_cut_of, save_insert_cut,
};

pub fn press_effect_item(window: &adw::ApplicationWindow, kind: cut::EffectKind) -> String {
    // F3.1 S1: a Zoom is NOT recorded here. The dropdown's ⊕ Zoom entry ARMS a drag — the box someone draws is
    // what becomes the zoom, through `press_zoom_item` and then `zoom_drag_ended`. Recording one on the click would
    // put a whole effect on the cut before anyone has said where, which is why this flow was unreachable from the UI.
    if kind == cut::EffectKind::Zoom {
        return press_zoom_item(window);
    }
    // F3.3 S1: Speed is NOT recorded here either. ⏩ Speed opens a FORM — the band's own seconds, or a stop at
    // the line, with rate / sound / length / two fades / Curve still to be settled. The generic path below lays
    // down a fresh record of `NEW_EFFECT_SECONDS` width and no rate answer, which is why none of `cut_speed`'s
    // S1-S4 flow was reachable from the UI before this branch existed.
    if kind == cut::EffectKind::Speed {
        return press_speed_item(window);
    }
    // F3.4 S1: Text is NOT recorded here either. ❝ Text ARMS a gesture -- the box someone clicks or drags on
    // the picture is what becomes the caption, through `press_text_item` and then `text_drag_ended`. The
    // generic path below would lay down a record before anyone has said where the words go or WHAT they say,
    // which is why none of §F3.4's S1-S3 was reachable from the UI before this branch existed.
    if kind == cut::EffectKind::Text {
        return press_text_item(window);
    }
    // F3.5 S1: SVG is NOT recorded here either. The square-with-a-pen entry asks for the FILE first and only
    // then arms -- the chooser comes before the gesture, because a drawing with no ink cannot be drawn. The
    // generic path below would lay down a record before anyone has chosen a file or drawn a box, which is why
    // none of section F3.5's S1-S4 was reachable from the UI before this branch existed.
    if kind == cut::EffectKind::Svg {
        return press_svg_item(window);
    }
    if kind == cut::EffectKind::Volume {
        return press_volume_item(window);
    }
    if kind == cut::EffectKind::Label {
        return press_label_item(window);
    }
    let mut cut_ = review_cut_of(window);
    // The red line, read the way Paste reads it: the live preview's playhead if one is going, else the
    // saved line position. No new position store — the line already has one owner.
    let at = paste_line(window);
    fx_record::record_into(&mut cut_, kind, at, fx_record::NEW_EFFECT_SECONDS);
    save_insert_cut(&cut_);
    record_edit(window, &cut_);
    // Draw the new bar in the same turn, so the lane never lags the thing that was just added.
    refresh_effects_lane(window);
    fx_record::recorded_status(kind)
}
