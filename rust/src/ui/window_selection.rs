//! §05-cut#1-screen — the selection band, the held effect and its live form, and the preview's own
//! volume. Split out of `window.rs`, which is past its size budget; its doors are re-exported through
//! `window`.
//!
//! The rules are `cut_select`'s, `fx_band`'s and `cut_hear`'s; what is here is the per-window state a
//! press leaves behind, the widgets it repaints, and the sentence it prints.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use gtk4 as gtk;

use crate::cut;

use crate::cut_hear;
use crate::cut_line;
use crate::cut_select;
use crate::cut_verbs;
use crate::fx_band;
use crate::fx_record;
use crate::fx_zoom;
use crate::narration::Narration;
use crate::preview;
use crate::ui::cut_cameras::refresh_camera_rows;
use crate::ui::cut_form_frame;
use crate::ui::cut_label_hand::{close_label_form, label_form_open, show_label_form};
use crate::ui::cut_speed_hand::{close_speed_form, show_speed_form, speed_form_open};
use crate::ui::cut_svg::{
    close_svg_form, press_svg_esc, release_svg_arm_only, show_svg_form, svg_armed, svg_form_open,
};
use crate::ui::cut_text_hand::{
    close_text_form, press_text_esc, refresh_aspect_readout, release_text_arm_only, show_text_form,
    text_armed, text_form_open,
};
use crate::ui::cut_volume_hand::{close_volume_form, show_volume_form, volume_form_open};
use crate::ui::window as ui;
use crate::ui::window::refresh_effects_lane;

/// The narration this window holds, newest slot -- the handle a flow re-seeds after it re-read the
/// folder. Crate-visible because only the flows that own a re-read may replace the lines.
pub(crate) fn held_narration() -> Option<Rc<std::cell::RefCell<Narration>>> {
    HELD_NARRATION.with(|held| held.borrow().last().cloned())
}
thread_local! {
    /// The narration this window holds, newest last — the same newest-slot rule as [`WINDOW_SHELLS`].
    /// S5's refit and S3's flush are two different switches over one set of lines, so the lines live
    /// here rather than in whatever a single switch happened to read from disk.
    pub(crate) static HELD_NARRATION: std::cell::RefCell<Vec<Rc<RefCell<Narration>>>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

// The app's ONE preview volume (F2.5 S6), held per window beside [`PREVIEW_PLAYERS`] with the same
// newest-slot rule. It is a `PreviewVolume` and not a bare number because the type is what keeps the
// slider's 0..100 and the property's 0..1 from drifting apart, and because the tooltip promises every
// slider shows the same setting — two stored copies would be two answers.
thread_local! {
    pub(crate) static PREVIEW_VOLUMES: std::cell::RefCell<Vec<Rc<std::cell::RefCell<cut_hear::PreviewVolume>>>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

// This window's selection band (F2.6), newest-slot rule as everywhere else here. `None` is no band:
// the page then reads the empty marks and every verb that needs a selection is off.
thread_local! {
    pub(crate) static SELECTED: std::cell::RefCell<Vec<Rc<std::cell::RefCell<Option<cut_select::Selection>>>>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

// The surface the current band was drawn on, kept beside [`SELECTED`] so a resize or a move stays on
// the ground it started from. S1 scopes a band by what it was drawn on, and a later drag on a lane
// must not silently turn a footage selection into a sound one (or the reverse) mid-adjust.
thread_local! {
    pub(crate) static SELECT_SURFACES: std::cell::RefCell<Vec<Rc<std::cell::RefCell<Option<cut_select::Surface>>>>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// F2.6 S1: a left-drag from `from` to `to` on `surface` selected what it was drawn on. The rule is
/// [`cut_select::draw`]'s alone — including its refusal for the effects lane, which answers `None` and
/// leaves this window with no band rather than a half-made one. Returns the same answer it stored.
pub fn draw_selection(
    window: &adw::ApplicationWindow,
    surface: cut_select::Surface,
    recording: Option<&str>,
    from: f64,
    to: f64,
) -> Option<cut_select::Selection> {
    let band = cut_select::draw(surface, recording, from, to);
    SELECTED.with(|slots| {
        if let Some(slot) = slots.borrow().last() {
            *slot.borrow_mut() = band.clone();
        }
    });
    SELECT_SURFACES.with(|slots| {
        if let Some(slot) = slots.borrow().last() {
            *slot.borrow_mut() = Some(surface);
        }
    });
    refresh_selection_readout(window);
    band
}

/// F2.6: this window's band, or `None`. Read by the readout, the verbs and every later flow that acts
/// on a selection — all of them through this one value, so none can hold a stale copy.
pub fn selection(window: &adw::ApplicationWindow) -> Option<cut_select::Selection> {
    let _ = window;
    SELECTED.with(|slots| slots.borrow().last().and_then(|s| s.borrow().clone()))
}

/// The surface the current band came from, defaulting to the ruler when nothing has been drawn yet.
pub fn selection_surface(window: &adw::ApplicationWindow) -> cut_select::Surface {
    let _ = window;
    SELECT_SURFACES
        .with(|slots| slots.borrow().last().and_then(|s| *s.borrow()))
        .unwrap_or(cut_select::Surface::Ruler)
}

/// F2.6 S2: the cross. Routes through [`cut_select::clear`] and returns whether there was anything to
/// clear, which is also what turns the button insensitive again — a press on nothing reports nothing.
pub fn clear_selection(window: &adw::ApplicationWindow) -> bool {
    let had = selection(window).is_some();
    SELECTED.with(|slots| {
        if let Some(slot) = slots.borrow().last() {
            *slot.borrow_mut() = cut_select::clear(selection(window));
        }
    });
    refresh_selection_readout(window);
    had
}

/// F2.6 S2: drag one part of the band to `to` (session seconds) at `pps` pixels-per-second. The snap
/// marks are rebuilt each call from this window's cut, its lanes and the playhead, because the borders a
/// handle may snap to change as the cut changes — a cached list would go stale against an edit.
///
/// `Part::Outside` moves nothing: a press clear of the band starts a new selection instead, which is
/// S1's job, not a nudge.
pub fn nudge_selection(window: &adw::ApplicationWindow, part: cut_select::Part, to: f64, pps: f64) {
    let Some(band) = selection(window) else {
        return;
    };
    let (cut_, lanes) = selection_snap_inputs(window);
    let marks = cut_select::snap_marks(&cut_, &lanes, preview_playhead(window));
    let moved = match part {
        cut_select::Part::Start => cut_select::resize(&band, false, to, &marks, pps),
        cut_select::Part::End => cut_select::resize(&band, true, to, &marks, pps),
        cut_select::Part::Middle => cut_select::move_band(&band, to, &marks, pps),
        cut_select::Part::Outside => return,
    };
    SELECTED.with(|slots| {
        if let Some(slot) = slots.borrow().last() {
            *slot.borrow_mut() = Some(moved);
        }
    });
    refresh_selection_readout(window);
}

/// F2.6 S4: the verbs' state for the band there is now — Add/Split/Remove greyed for a sound selection,
/// Copy and Insert re-aimed at it. The ＋ Add / | Split / － Remove buttons F2.7 added read THIS answer
/// (`refresh_verb_buttons`), so the page's greying and the logic tests' assertions cannot drift apart;
/// ⧉ Copy / Insert are still not drawn (F2.9/F0.6 rounds).
pub fn selection_verbs(window: &adw::ApplicationWindow) -> cut_select::Verbs {
    let (cut_, _) = selection_snap_inputs(window);
    // "is there a recording to cut": a cut with no segment has nothing to split, which is the only
    // thing S4 asks of the page beyond the band itself.
    let has_footage = !cut_.segs.is_empty();
    cut_select::verbs(selection(window).as_ref(), has_footage)
}

/// What this window can offer the snap-mark builder. The cut model round owns where a real `Cut` comes
/// from; until then the page's own review-cut slot stands in, so `nudge_selection` snaps against real
/// borders whenever one has been seeded and against nothing otherwise.
fn selection_snap_inputs(_window: &adw::ApplicationWindow) -> (cut::Cut, Vec<cut::Lane>) {
    let cut_ = ui::REVIEW_CUTS.with(|slots| {
        slots
            .borrow()
            .last()
            .map(|slot| slot.borrow().clone())
            .unwrap_or_default()
    });
    let lanes = cut_.lanes.clone();
    (cut_, lanes)
}

/// F2.7: which of the three buttons is live right now, as (add, split, remove).
///
/// | Split stays live with no band on purpose: F2.6's `verbs()` answers `false` for everything when
/// there is no selection, but F2.7 gives a band-less Split a real job — one border at the red line,
/// right half in hand — so greying it would take away the verb the spec says works there. An empty case
/// explains itself through the refusal sentence instead. With a band up, `verbs()` decides all three,
/// which is what greys them for a recording's sound.
pub fn verb_buttons_state(window: &adw::ApplicationWindow) -> (bool, bool, bool) {
    let verbs = selection_verbs(window);
    let split_live = match selection(window) {
        Some(_) => verbs.split,
        None => true,
    };
    (verbs.add, split_live, verbs.remove)
}

/// F2.7: set the three verb buttons from this window's live state, so a greyed button is always today's
/// answer and never a leftover from an earlier band. Runs from `refresh_selection_readout`, which every
/// draw, nudge and clear already calls — one refresh path for the whole selection row.
pub(crate) fn refresh_verb_buttons(window: &adw::ApplicationWindow) {
    let (add, split, remove) = verb_buttons_state(window);
    let sensitivity = [add, split, remove];
    for ((name, _, _), live) in cut_verbs::BUTTONS.iter().zip(sensitivity) {
        if let Some(button) = ui::line_step_button(window, name) {
            button.set_sensitive(live);
        }
    }
    // ＋ Add wears its sound warning as a tooltip while a sound band makes it the wrong button.
    if let Some(button) = ui::line_step_button(window, "add-button") {
        let tip = cut_verbs::add_tip_now(selection(window).as_ref())
            .unwrap_or_else(|| cut_verbs::BUTTONS[0].2.to_string());
        button.set_tooltip_text(Some(&tip));
    }
    // F2.2 S1: ▶✂ greys with no clips, so every write that adds or removes one re-checks it here too.
    crate::ui::cut_cut_play::paint(window);
    // F2.3 S1: and the same for ▶✂✂, which additionally wears ⏸ only while a review is the running preview.
    crate::ui::cut_review_play::paint(window);
}

/// F2.7 S3: write the band into the readout and set the cross's sensitivity. The readout IS the band's
/// own marks (`cut_select::marks`), not a second copy, so the two cannot disagree with the handle being
/// dragged. The tent-of-a-second form comes from `preview::clock`, the Cut page's one clock face.
pub(crate) fn refresh_selection_readout(window: &adw::ApplicationWindow) {
    let band = selection(window);
    let text = match cut_select::marks(band.as_ref()) {
        Some((start, end)) => format!(
            "{} {} \u{2013} {}",
            cut_select::READOUT_PREFIX,
            preview::clock(Some(start)),
            preview::clock(Some(end))
        ),
        None => cut_select::READOUT_NONE.to_string(),
    };
    if let Some(label) = ui::find_widget_by_name(window.upcast_ref(), "selection-readout") {
        if let Ok(label) = label.downcast::<gtk::Label>() {
            label.set_text(&text);
        }
    }
    if let Some(button) = ui::find_widget_by_name(window.upcast_ref(), "clear-selection") {
        if let Ok(button) = button.downcast::<gtk::Button>() {
            button.set_sensitive(band.is_some());
        }
    }
    // F3.2: the Aspect ratio row rides this same path, so a pick, an Undo or a Clear all move it with the
    // cut rather than leaving whatever shape was last painted.
    refresh_aspect_readout(window);
    refresh_verb_buttons(window);
    // F2.12: Insert rides the same refresh path — greyed with nowhere to go, and wearing "Edit" while a card
    // is held. A greyed or mislabelled Insert is always today's answer, never a leftover from an earlier band.
    ui::refresh_insert_button(window);
    // F2.9: the copy buttons ride the same refresh path, so a band drawn, nudged or cleared updates them
    // without a second place to remember.
    ui::refresh_copy_buttons(window);
    // §05-cut#1-screen: and so do the four history buttons — one refresh path sets every greyed
    // control on the page, from the rule that owns each of them.
    ui::refresh_history_buttons(window);
    // F2.10: and the camera rows, whose badges, plates and gutter switches read the same live cut on
    // every draw, nudge and clear — a badge showing a state the next press contradicts is a lie.
    refresh_camera_rows(window);
    // F2.6 S3: the in/out marks are PAINTED marks on the band, so every change to the band queues a repaint
    // of it — the painter reads this window's band through `cut_view::band_scene_for` and draws its blue
    // span and its in/out flags from that, so the marks follow a draw, a nudge or a clear with no second
    // copy of the numbers anywhere.
    crate::ui::cut_view::repaint_page(window);
    // §06-effects#2-the-lane-and-the-preview: the lane and the preview read the same live cut on every
    // draw, nudge and clear, so a bar or an overlay never lags the record it stands for.
    refresh_effects_lane(window);
}

/// The playhead the snap marks are built around (F2.6 S2 lists the line among the snap targets).
pub fn preview_playhead(window: &adw::ApplicationWindow) -> f64 {
    ui::live_player(window)
        .map(|player| player.borrow().playhead.unwrap_or(0.0))
        .unwrap_or(0.0)
}

/// This window's drag gesture, so a test can fire it the way GTK does. `None` on a page that drew none.
pub fn selection_gesture(window: &adw::ApplicationWindow) -> Option<gtk::GestureDrag> {
    let _ = window;
    SELECT_GESTURES.with(|cell| cell.borrow().clone())
}
thread_local! {
    pub(crate) static SELECT_GESTURES: std::cell::RefCell<Option<gtk::GestureDrag>> =
        const { std::cell::RefCell::new(None) };
}

/// F2.6 S1: what ground the current drag started on, or `None` when no press has been remembered yet.
pub fn pressed_surface(window: &adw::ApplicationWindow) -> Option<cut_select::Surface> {
    let _ = window;
    SELECT_SURFACES.with(|slots| slots.borrow().last().and_then(|s| *s.borrow()))
}

/// F2.6 S1: remember the surface a press landed on for this window's drag.
pub(crate) fn set_pressed_surface(surface: cut_select::Surface) {
    SELECT_SURFACES.with(|slots| {
        if let Some(slot) = slots.borrow().last() {
            *slot.borrow_mut() = Some(surface);
        }
    });
}

/// F3.8 S5 (`Forms are live`): close whichever per-kind form this page has open, so ✎ Edit from a band cannot
/// leave two forms showing at once. Each kind's own Cancel keeps its own wording; this only hides the panel.
pub(crate) fn close_any_effect_form(window: &adw::ApplicationWindow) {
    if ui::zoom_form_open().is_some() {
        ui::set_zoom_form(None);
        if let Some(holder) = ui::zoom_form_box(window) {
            holder.set_visible(false);
        }
    }
    if speed_form_open().is_some() {
        close_speed_form(window);
    }
    if text_form_open().is_some() {
        close_text_form(window);
    }
    if svg_form_open().is_some() {
        close_svg_form(window);
    }
    if volume_form_open().is_some() {
        close_volume_form(window);
    }
    if label_form_open().is_some() {
        close_label_form(window);
    }
    cut_form_frame::set_pinned(window, None);
}

/// F3.8 S1 (`a click: the form opens`): the ONE door from a band to the form of its kind. Every kind in §06
/// that has a form routes here — Zoom, Speed, Text, SVG, Volume and Label each got one in their own round; a
/// kind with no form returns `None` so the caller prints nothing rather than inventing a panel. Kept as a
/// dispatch rather than five calls at the press site so "click a band → its form" cannot half-work: adding a
/// seventh kind without a form shows up here as a missing arm.
pub fn open_effect_form(window: &adw::ApplicationWindow, fx: &cut::Fx) -> Option<&'static str> {
    let kind = fx.effect_kind()?;
    match kind {
        cut::EffectKind::Zoom => {
            // The zoom form takes a `fx_zoom::Form`, not the stored record: rebuild the six fields from it so
            // the door works for a band clicked rather than a box just dragged.
            let form = fx_zoom::Form {
                at: fx.t,
                dur: fx.dur,
                stay: fx.stay,
                trans: fx.trans,
                tout: fx.tout,
                curve: fx.ease.clone(),
                // The zoom form holds its box as plain fractions; a record with no box shows the centre, the
                // same stand-in `zoom_drag_ended` leaves when the hand never sized one.
                cx: fx.cx.unwrap_or(0.5),
                cy: fx.cy.unwrap_or(0.5),
                hf: fx.hf.unwrap_or(0.5),
                // The camera row is the page's own answer, not a field of the record: `ui::press_row` is what
                // every other zoom opener uses.
                row: ui::press_row(window),
            };
            ui::show_zoom_form(window, &form);
            Some("zoom")
        }
        cut::EffectKind::Speed => {
            show_speed_form(window, fx);
            Some("speed")
        }
        cut::EffectKind::Text => {
            show_text_form(window, fx);
            Some("text")
        }
        cut::EffectKind::Svg => {
            show_svg_form(window, fx);
            Some("svg")
        }
        cut::EffectKind::Volume => {
            show_volume_form(window, fx);
            Some("volume")
        }
        cut::EffectKind::Label => {
            show_label_form(window, fx);
            Some("label")
        }
    }
}

/// F3.8 S1/S2/S3: a press on one bar of the effects lane, answered by the rules in [`fx_band`] and nothing
/// else. The seam a real widget (and a test firing by name) goes through: `index` is the record's place in the
/// cut's `fx` list, which is what the widget is named after (`fx-bar-<index>`), `x_px` where in the bar the
/// press landed, `width_px` the bar's drawn width, `travel_px` how far the pointer went, and `over_kill`
/// whether it was on the ✕.
///
/// Named `press_effect_bar`, NOT `press_band`: this file already owns `press_band` / `set_press_band`, which
/// pick which RECORDING a press refers to (§05). The two are unrelated and must not be conflated.
pub fn press_effect_bar(
    window: &adw::ApplicationWindow,
    index: usize,
    x_px: f64,
    width_px: f64,
    travel_px: f64,
    over_kill: bool,
) -> fx_band::Press {
    let cut_ = ui::review_cut_of(window);
    let Some(fx) = cut_.fx.get(index).cloned() else {
        return fx_band::Press::Dropped;
    };
    // Where the hand grabbed decides whether a drag slides or resizes; `fx_band` answers it from the same
    // pixel reach the lane draws with.
    let grab = fx_band::grab_at(x_px, width_px);
    let answer = fx_band::press_band(fx.t, width_px, travel_px, over_kill, true, grab);
    match answer {
        fx_band::Press::ClickForm => {
            open_effect_form(window, &fx);
            cut_form_frame::set_pinned(
                window,
                Some(&format!(
                    "edit {}",
                    fx_record::label_of(fx.effect_kind().unwrap_or(cut::EffectKind::Zoom))
                )),
            );
        }
        fx_band::Press::Hold { t, from_end: _ } => {
            ui::set_held_effect(Some(fx.clone()));
            // S1: `picked up` puts the line on the band's start. `ui::set_line_position` also marks that a line
            // exists, which is what makes the placement known to every other flow.
            ui::set_line_position(window, cut_line::LinePos { t });
            ui::refresh_insert_button(window);
            refresh_effects_lane(window);
            let said = fx_band::picked_up(fx_record::label_of(
                fx.effect_kind().unwrap_or(cut::EffectKind::Zoom),
            ));
            if let Some(status_line) = ui::find_status(window.upcast_ref()) {
                status_line.set_text(&said);
            }
            ui::log_line(&said);
        }
        fx_band::Press::Kill => {
            let said = remove_effect_at(window, index);
            if let Some(status_line) = ui::find_status(window.upcast_ref()) {
                status_line.set_text(&said);
            }
            ui::log_line(&said);
        }
        fx_band::Press::Dropped => {
            ui::set_held_effect(None);
            refresh_effects_lane(window);
        }
    }
    answer
}

/// F3.8 S3: take ONE record out of the cut by its index. Goes through `ui::record_edit`, NEVER `ui::seed_review_cut`:
/// the removal must sit on the history so F2.13's ↶ puts the effect back.
pub fn remove_effect_at(window: &adw::ApplicationWindow, index: usize) -> String {
    let mut cut_ = ui::review_cut_of(window);
    if index >= cut_.fx.len() {
        return fx_band::GONE.to_string();
    }
    let taken = cut_.fx.remove(index);
    ui::save_insert_cut(&cut_);
    ui::record_edit(window, &cut_);
    // The thing in hand was this record: holding a removed effect would leave the lane asking for an overlay of
    // nothing.
    if ui::held_effect().is_some_and(|held| held.kind == taken.kind && held.t == taken.t) {
        ui::set_held_effect(None);
    }
    refresh_effects_lane(window);
    ui::refresh_insert_button(window);
    let label = fx_record::label_of(taken.effect_kind().unwrap_or(cut::EffectKind::Zoom));
    let said = fx_band::removed(label);
    ui::log_line(&said);
    said
}

/// F3.8 S3 (`⌦ with an effect held removes it`): the keyboard's kill. The held record is found in the LIVE
/// cut the same way the lane finds it (kind + start), never by remembering an index across edits.
pub fn remove_held_effect(window: &adw::ApplicationWindow) -> String {
    let Some(held) = ui::held_effect() else {
        return fx_band::GONE.to_string();
    };
    let cut_ = ui::review_cut_of(window);
    let Some(index) = fx_band::refind(&cut_.fx, &held.kind, held.t) else {
        return fx_band::GONE.to_string();
    };
    remove_effect_at(window, index)
}

/// F3.8 S2 (`Frame steps and ←/→ nudge a held effect unsnapped`): the keyboard's move of the record in
/// hand. `fx_band::nudged` IS `effect_rules::nudge`, so there is no snap in this path by construction — the
/// keyboard moves a band, it does not stretch one, and a nudge walked past a segment end stays exactly that
/// many seconds off it.
pub fn nudge_held_effect(window: &adw::ApplicationWindow, delta: f64) -> String {
    let Some(held) = ui::held_effect() else {
        return fx_band::GONE.to_string();
    };
    let mut cut_ = ui::review_cut_of(window);
    let Some(index) = fx_band::refind(&cut_.fx, &held.kind, held.t) else {
        return fx_band::GONE.to_string();
    };
    let (t, dur) = fx_band::nudged(&held, delta);
    cut_.fx[index].t = t;
    cut_.fx[index].dur = dur;
    // §7's mid-drag rule: a drag tick that happens while a gesture is in flight is refused by `cut_save` itself,
    // so the file waits for the release. A keyboard nudge is a whole act of the hand and writes.
    ui::save_insert_cut(&cut_);
    ui::record_edit(window, &cut_);
    ui::set_held_effect(Some(cut_.fx[index].clone()));
    // Same rule as `drag_held_band`: the line rides with the band it is sitting on, so a nudge never leaves
    // the held record "behind the viewer" for the refresh to give up.
    ui::set_line_position(window, cut_line::LinePos { t });
    refresh_effects_lane(window);
    let said = fx_band::moved(fx_record::label_of(
        held.effect_kind().unwrap_or(cut::EffectKind::Zoom),
    ));
    if let Some(status_line) = ui::find_status(window.upcast_ref()) {
        status_line.set_text(&said);
    }
    ui::log_line(&said);
    said
}

/// F3.8 S2: a drag of the record in hand, on the lane. `from_end == None` slides the WHOLE band, snapped
/// against `marks`; `Some(end)` moves that end only. `marks` MUST arrive with the moved effect's own two ends
/// filtered out (§A.9: "the held effect's own ends excluded") — a band that snaps to itself cannot be dragged
/// once it lines up with its own start. The page builds those marks; this seam takes them as given so the rule
/// stays testable without a rendered timeline.
pub fn drag_held_band(
    window: &adw::ApplicationWindow,
    to: f64,
    marks: &[f64],
    pps: f64,
    session_end: f64,
    from_end: Option<bool>,
) -> String {
    let Some(held) = ui::held_effect() else {
        return fx_band::GONE.to_string();
    };
    let mut cut_ = ui::review_cut_of(window);
    let Some(index) = fx_band::refind(&cut_.fx, &held.kind, held.t) else {
        return fx_band::GONE.to_string();
    };
    let (t, dur) = match from_end {
        None => fx_band::move_whole(&held, to, marks, pps, session_end),
        Some(end) => fx_band::resize_end(&held, end, to),
    };
    cut_.fx[index].t = t;
    cut_.fx[index].dur = dur;
    // §7's `cut.json is written by one function`: the write is refused while a gesture is in flight
    // ([`crate::cut_save::gesture_in_flight`]), so a drag tick updates state and the release writes the answer.
    ui::save_insert_cut(&cut_);
    // §7's one Undo per drag: while a bar gesture is in flight the FIRST tick spends the history entry and the
    // rest ride on it (a mouse-move loop would otherwise cost one step per pixel). A direct call with no
    // gesture in flight keeps recording every time, which is what `band_hold_widgets` pins.
    if !crate::ui::cut_band_press::band_drag_in_flight()
        || crate::ui::cut_band_press::take_history_step()
    {
        ui::record_edit(window, &cut_);
    } else {
        ui::seed_review_cut(window, &cut_);
    }
    ui::set_held_effect(Some(cut_.fx[index].clone()));
    // S1 put the line on the band's start when the band was picked up; a drag that moves the band carries the
    // line along with it, or the refresh below would read the OLD line against the NEW position and drop the
    // hold as "walked off the band".
    ui::set_line_position(window, cut_line::LinePos { t });
    refresh_effects_lane(window);
    let label = fx_record::label_of(held.effect_kind().unwrap_or(cut::EffectKind::Zoom));
    let said = match from_end {
        None => fx_band::moved(label),
        Some(_) => fx_band::resized(label),
    };
    if let Some(status_line) = ui::find_status(window.upcast_ref()) {
        status_line.set_text(&said);
    }
    ui::log_line(&said);
    said
}

/// F3.8 S4 (`✎ Edit: found by kind and start · the box from the live effect`): reopen the form of the record
/// in hand. A miss says so and changes NOTHING — the sentence's own promise. On a hit the form is opened from
/// the LIVE record, and the four box fractions go through [`fx_band::live_box`] with whatever the form or the
/// hold carried as a snapshot: the argument exists to make the live read visible at the call site, because a
/// form that redrew from its own earlier copy would overwrite a box someone moved since it opened.
pub fn press_edit_effect(window: &adw::ApplicationWindow) -> String {
    let Some(held) = ui::held_effect() else {
        return fx_band::GONE.to_string();
    };
    let cut_ = ui::review_cut_of(window);
    let Some(index) = fx_band::refind(&cut_.fx, &held.kind, held.t) else {
        ui::log_line(fx_band::GONE);
        return fx_band::GONE.to_string();
    };
    let live = &cut_.fx[index];
    // What the form last showed, as the stale side of the comparison. `held` is the copy that went into the
    // hand, so its fractions ARE the snapshot: they may be older than `live`'s.
    let stale = (
        held.cx.unwrap_or(0.0),
        held.cy.unwrap_or(0.0),
        held.wf.unwrap_or(0.0),
        held.hf.unwrap_or(0.0),
    );
    let (cx, cy, wf, hf) = fx_band::live_box(live, Some(stale));
    let mut shown = live.clone();
    shown.cx = cx;
    shown.cy = cy;
    shown.wf = wf;
    shown.hf = hf;
    ui::set_held_effect(Some(shown.clone()));
    // One form on the page at a time: opening this record's form over another would leave both visible.
    close_any_effect_form(window);
    open_effect_form(window, &shown);
    cut_form_frame::set_pinned(
        window,
        Some(&format!(
            "edit {}",
            fx_record::label_of(shown.effect_kind().unwrap_or(cut::EffectKind::Zoom))
        )),
    );
    let said = format!(
        "editing {} \u{2014} {}",
        fx_record::label_of(shown.effect_kind().unwrap_or(cut::EffectKind::Zoom)),
        fx_band::KEPT_AS_YOU_TYPE
    );
    if let Some(status_line) = ui::find_status(window.upcast_ref()) {
        status_line.set_text(&said);
    }
    ui::log_line(&said);
    said
}

/// F3.8 S3 (`Esc drops the hold and disarms ("cancelled" for a bare disarm)`): Escape when an effect is in
/// hand. Returns `None` when this window holds nothing, so the key travels on to whichever form or arm does
/// own it — with nothing held the per-kind controllers keep their precedence, and this one only claims the
/// leftover (see `window.rs`: it is wired FIRST among the Esc controllers, which is what lets it disarm below).
///
/// Both halves of S3 happen here, because claiming the key means no other Esc handler runs after this one:
/// the hold goes, and every hand-armed drag comes down with it. One word covers both — `fx_band::CANCELLED`
/// is the bare-disarm sentence §A.9 spells, and the armed kinds' own sentences belong to their own Cancel.
pub fn press_band_esc(window: &adw::ApplicationWindow) -> Option<String> {
    let _ = ui::held_effect()?;
    // The arms come down BEFORE the hold is cleared, so a door that refuses (a caption form this window does
    // not own) still leaves the press having done what it said: nothing is claimed and then half-done.
    if ui::zoom_armed() {
        let _ = ui::press_zoom_esc(window);
    }
    if text_armed() && press_text_esc(window).is_none() {
        // This window did not draw the caption form, so its own door left the arm up; S3's "disarms" half
        // needs the arm gone regardless of which window raised it.
        release_text_arm_only();
    }
    if svg_armed() && press_svg_esc(window).is_none() {
        release_svg_arm_only();
    }
    ui::set_held_effect(None);
    // A held effect plus an OPEN form is out of S3's way: this press drops the hold and the arms, and the form
    // takes the next Esc (or the heading's ✕, which forwards to the same doors). The spec names no other shape.
    refresh_effects_lane(window);
    ui::refresh_insert_button(window);
    Some(fx_band::CANCELLED.to_string())
}

// F3.8 S5 (`Forms are live: first answer pushes Undo ... refusals reset that`): the visit-level bookkeeping
// every live form shares. One entry per visit, held here rather than in each field, because "↶ takes the whole
// edit back" means the visit, not the keystroke.
thread_local! {
    pub(crate) static LIVE_FORM: std::cell::RefCell<fx_band::LiveForm> =
        std::cell::RefCell::new(fx_band::LiveForm::default());
}

/// F3.8 S5: does THIS accepted answer take the visit's single Undo entry? True the first time, false for
/// every change after it. Called by the Apply seams right before they write.
pub fn live_form_answer_is_undo(_window: &adw::ApplicationWindow) -> bool {
    LIVE_FORM.with(|cell| cell.borrow_mut().first_answer())
}

/// F3.8 S5: a refused answer wrote nothing, so it gives the Undo entry back and the next accepted answer is
/// again the first. Called on every Apply refusal path.
pub fn live_form_refused() {
    LIVE_FORM.with(|cell| cell.borrow_mut().refusal_resets());
}

/// F2.6 S1: a press on the effects lane -- the only ground that refuses a selection. With an effect in
/// hand the press PUTS IT DOWN (that is what the lane is for); with nothing held it does nothing. Either
/// way NO band is drawn, so the existing selection stays exactly as it was. Callable without a widget so
/// the rule is testable on its own; the gesture handler just calls this when `surface_at` says `FxLane`.
pub fn press_fx_lane(window: &adw::ApplicationWindow) -> cut_select::FxLanePress {
    let _ = window;
    let answer = cut_select::fx_lane_press(ui::held_effect().is_some());
    match answer {
        cut_select::FxLanePress::PutsHeldEffectDown => {
            ui::set_held_effect(None);
            ui::log_line("effect put down on the effects lane");
            // The preview's held overlay goes with it: a dashed outline around nothing would claim an
            // effect is still in hand.
            refresh_effects_lane(window);
        }
        cut_select::FxLanePress::Nothing => {}
    }
    answer
}

/// The surface the placeholder track area draws today, and how many rows of it exist. §7's `a selection is of
/// what it was drawn on` is what this answers: the strip is a 48 px box holding a ruler, ONE picture row with
/// its wave strip, and the effects lane along the bottom, so every band is reachable by y — read off that split
/// rather than off the painted band's taller geometry, which would answer most of this box as "past the last
/// row". Both press paths on this widget go through here ([`crate::ui::cut_line_press::press_strip`] and
/// `wire_select_surface`'s drag) so a click and a drag cannot disagree about a y.
pub(crate) fn placeholder_surface_bands() -> (cut_select::SurfaceBands, usize) {
    (
        cut_select::PLACEHOLDER_BANDS,
        crate::ui::cut_line_press::STRIP_PICTURE_ROWS,
    )
}

/// This window's ✕ Clear selection button (F2.6 S2), looked up by its stable name.
pub fn clear_selection_button(window: &adw::ApplicationWindow) -> Option<gtk::Button> {
    ui::find_widget_by_name(window.upcast_ref(), "clear-selection")?
        .downcast()
        .ok()
}

/// This window's Selection readout label (F2.6 S3).
pub fn selection_readout(window: &adw::ApplicationWindow) -> Option<gtk::Label> {
    ui::find_widget_by_name(window.upcast_ref(), "selection-readout")?
        .downcast()
        .ok()
}

/// This window's drag surface (F2.6 S1).
pub fn select_surface(window: &adw::ApplicationWindow) -> Option<gtk::DrawingArea> {
    ui::find_widget_by_name(window.upcast_ref(), "select-surface")?
        .downcast()
        .ok()
}

/// F2.5 S6: set this window's one preview volume from a slider's percent (0..100) and push the result
/// into the live player, so every preview of this window shares the one number. Returns the gain written
/// (`0..=1`), which is what the caller would read back — the clamp lives in [`cut_hear::clamp_volume`],
/// never here.
pub fn set_preview_volume(window: &adw::ApplicationWindow, percent: f64) -> f64 {
    let _ = window;
    let volume = PREVIEW_VOLUMES.with(|slots| {
        slots
            .borrow()
            .last()
            .cloned()
            .unwrap_or_else(|| Rc::new(std::cell::RefCell::new(cut_hear::PreviewVolume::new())))
    });
    volume.borrow_mut().set_percent(percent);
    let gain = volume.borrow().value();
    if let Some(player) = ui::live_player(window) {
        player.borrow_mut().volume = Some(gain);
    }
    gain
}

/// F2.5 S6: the one number, as a gain — what every preview multiplies its sound by, and the value
/// [`cut_hear::mix_at`] is handed. A window that never showed a slider reads the default full.
pub fn preview_volume(window: &adw::ApplicationWindow) -> f64 {
    let _ = window;
    PREVIEW_VOLUMES.with(|slots| {
        slots
            .borrow()
            .last()
            .map(|v| v.borrow().value())
            .unwrap_or(cut_hear::VOLUME_DEFAULT)
    })
}

/// F2.5: ask the hearing rules what this tick means and store the answer on this window's player.
///
/// The page calls this instead of computing anything: `cut_hear::mix_at` decides which lanes start,
/// whether the footage's own sound is muted and how loud and fast to play, at this window's one
/// preview volume. Nothing here re-decides a part of that answer; it only lands `footage_muted` where a
/// later round's pipeline can read it.
pub fn apply_mix(
    window: &adw::ApplicationWindow,
    segs: &[cut::Seg],
    lanes: &[cut::Lane],
    fx: &[cut::Fx],
    t: f64,
) -> cut_hear::Mix {
    let mut mix = cut_hear::mix_at(segs, lanes, fx, t, preview_volume(window));
    // F2.12 S7 (`card sound is its own pipeline`): while a SPLICED card plays the footage is held on the wall
    // clock, so its pipeline may not carry on under a frozen picture — `card_hold_advance` says the line does
    // not move, and the same rule mutes the footage here. The card's OWN sound is what plays instead, which is
    // why this mutes the footage and never the card.
    let held_by_card = crate::ui::cut_card_preview::card_hearing(
        &cut::Cut {
            segs: segs.to_vec(),
            ..Default::default()
        },
        t,
    )
    .is_some_and(|hearing| hearing.footage_held);
    if held_by_card {
        mix.footage_muted = true;
    }
    // §05-cut#8 bullet 2: ONE line per CHANGE of the footage's own mute, naming the reason. Asked with the
    // answer the player held a tick ago and before it is overwritten below, which is what keeps a 100 ms tick
    // from printing the same mute eight times. The wording is `preview::sound_change`'s; this door only says
    // WHEN, and the reason is which of the two causes this tick's silence has.
    crate::ui::preview_sound::report_mute_change(
        crate::ui::preview_sound::heard_before(window),
        crate::ui::preview_sound::heard_now(&mix),
        held_by_card,
        &|line| ui::log_line(line),
    );
    // §06-effects#2 (`the two 1× sound answers cannot be previewed`, §9's `it SHOULD say "sound at 1× in the
    // render only"`): a speed asking for its sound at 1× to the effect's or the scene's end needs a clock this
    // preview does not have, so it plays the sound with the picture and says so. Asked of the MIX, which is where
    // the question belongs — that way a speed reaching the cut by any door (this form, a loaded project, F3.10's
    // speeds pass) gets the sentence, not only the one placed by hand. One line per entry: the notice is asked
    // against what the last tick was told, exactly as the mute line above is.
    crate::ui::preview_sound::report_sound_clock_change(
        crate::ui::preview_sound::sound_clock_noticed(),
        mix.sound_needs_two_clocks,
        crate::ui::preview_sound::unpreviewable_snd_under(fx, t).unwrap_or(""),
        &|line| ui::log_line(line),
    );
    if let Some(player) = ui::live_player(window) {
        player.borrow_mut().footage_muted = mix.footage_muted;
    }
    mix
}
