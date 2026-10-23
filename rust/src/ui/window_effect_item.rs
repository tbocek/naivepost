//! §06-effects#1-record: the effect dropdown's record door. Split out of `window.rs`, which is past its
//! size budget; the door is re-exported through `window` so every caller keeps the path it had.
//!
//! The body is unchanged from where it lived: six kinds arm a gesture or open a form in their own module,
//! and only the rest lay down a fresh record here.

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

/// §06-effects#1-record: record one effect of this kind on this window's cut.
///
/// One call to `fx_record::record_into` (which lays down a record carrying exactly the columns §1 gives
/// the kind), then the same save path `save_insert_cut` uses, then `record_edit` so F2.13's Undo covers
/// the addition — never `seed_review_cut`, which would publish without recording and leave the new bar
/// un-undoable. Returns the sentence the status line shows.
pub fn press_effect_item(window: &adw::ApplicationWindow, kind: cut::EffectKind) -> String {
    // F3.1 S1: a Zoom is NOT recorded here. The dropdown's ⊕ Zoom entry ARMS a drag — the box someone draws is
    // what becomes the zoom, through `press_zoom_item` and then `zoom_drag_ended`. Recording one on the click would
    // put a whole effect on the cut before anyone has said where, which is why this flow was unreachable from the UI.
    // Every other kind still records straight through, unchanged.
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
    // F3.6 S1: Volume is NOT recorded here either. 🔊 Volume opens a FORM — the band's own seconds, or two
    // from the line, with the percent still to be given. The generic path below lays down a record of
    // `NEW_EFFECT_SECONDS` width at whatever gain the struct defaults to, which is why none of §F3.6's S2-S5
    // (the 200 % default, the percent field, the 0.1 s floor, "heard even while paused") was reachable from
    // the UI before this branch existed. A volume has no box and nothing to arm: it works on SECONDS.
    if kind == cut::EffectKind::Volume {
        return press_volume_item(window);
    }
    // F3.7 S1: Label is NOT recorded here either. 🏷 Label opens a FORM — the band's own seconds, or two from
    // the line, with the NAME still to be given. The generic path below lays down a `NEW_EFFECT_SECONDS` record
    // with no name at all, and a label without a name is refused (§F3.7: "nothing is placed until then"), which
    // is why none of §F3.7's S2-S5 (the empty-name refusal, the 0.4 s floor, the tag that is drawn but never
    // rendered) was reachable from the UI before this branch existed.
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
