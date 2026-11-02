//! F2.10 cameras and hearing — the page half of the flow: the seams the badges and rows call, and
//! the row list itself.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use gtk4 as gtk;

use crate::cut::Cut;
use crate::cut_cam;
use crate::cut_fold;
use crate::cut_hear;
use crate::cut_verbs::{self, Badge};
#[allow(unused_imports)]
use crate::ui::cut_folds::{press_lane_cross, press_row_cross, CROSS_GLYPH};
use crate::ui::cut_page_state::{CutPage, PAGE as CUT_PAGE};
use crate::ui::cut_verb_press::press_badge;
use crate::ui::window::{
    find_status, find_widget_by_name, line_position, newest_review_cut, record_edit,
    set_watched_row, watched_row,
};

/// F2.10: a named `gtk::ToggleButton` in this window — the lens badge, a speaker badge or a gutter
/// switch.
pub fn toggle_button(window: &adw::ApplicationWindow, name: &str) -> Option<gtk::ToggleButton> {
    find_widget_by_name(window.upcast_ref(), name)?
        .downcast()
        .ok()
}

/// F2.10 S3: this window's [`cut_cam::Watch`], created on first ask so a page that was never
/// clicked still has an empty one to hand to ▶.
pub fn camera_watch() -> Rc<RefCell<cut_cam::Watch>> {
    CUT_PAGE.with(
        |CutPage {
             cam_watches: slots, ..
         }| {
            let mut slots = slots.borrow_mut();
            if let Some(latest) = slots.last() {
                return latest.clone();
            }
            let fresh = Rc::new(RefCell::new(cut_cam::Watch::default()));
            slots.push(fresh.clone());
            fresh
        },
    )
}

/// The session second the red line stands on — the `t` every F2.10 sentence is about.
fn line_second() -> f64 {
    line_position().t
}

fn badge_scene() -> Option<usize> {
    cut_cam::held_or_line_scene(
        &newest_review_cut(),
        Some(line_second()),
        crate::ui::cut_hand::held_clip_or_edge().as_ref(),
    )
}

/// §9 (`While an effect form is open, the preview MUST show the lens camera`): which row the
/// picture panel draws.
pub fn preview_row() -> usize {
    let cut_ = newest_review_cut();
    let lens = badge_scene()
        .map(|scene| cut_cam::scene_row(&cut_.segs[scene]))
        .unwrap_or(0);
    crate::effect_overlap::preview_camera(
        super::cut_zoom_hand::zoom_form_open().is_some()
            || super::cut_speed_hand::speed_form_open().is_some()
            || super::cut_volume_hand::volume_form_open().is_some()
            || super::cut_text_hand::text_form_open().is_some(),
        crate::ui::watched_row().map(|watched| watched as i32),
        lens,
    ) as usize
}

/// F2.10 S3 (`if the line is in a kept scene shown from another row, status says ONCE …`): the seam
/// a row's name plate forwards to.
pub fn press_watch_row(window: &adw::ApplicationWindow, row: i32) -> Option<String> {
    let cut_ = newest_review_cut();
    let at = line_second();
    let said = {
        let watch = camera_watch();
        let mut watch = watch.borrow_mut();
        cut_cam::click_row(&mut watch, row, &cut_.segs, Some(at))
    };
    // Keep F2.4's slot in step: whatever the picture band click sets, a row click means the same
    // thing about which row this window is looking at.
    set_watched_row(Some(row.max(0) as usize));
    if let Some(line) = &said {
        if let Some(status) = find_status(window.upcast_ref()) {
            status.set_text(line);
        }
    }
    // The dashed outline moves with the watch, so the rows are rebuilt rather than merely queued.
    refresh_camera_rows(window);
    // F2.10 S3: watching a row IS what the preview shows, so the frame follows the watch.
    crate::ui::cut_picture::refresh(window);
    said
}

// --- F2.10: drawing the rows --------------------------------------------------------------------------

pub const LENS_GLYPH: &str = "\u{1f50d}";
pub const SPEAKER_GLYPH: &str = "\u{1f508}";

fn lane_heard_in_line_scene(cut_: &Cut, lane: &str, scene: Option<usize>) -> bool {
    match scene {
        Some(index) => cut_
            .segs
            .get(index)
            .map(|seg| seg.hears(lane))
            .unwrap_or(true),
        None => true,
    }
}

/// Rebuild `camera-rows` from this window's own cut. Rebuilding rather than patching keeps the
/// watched row's dashed outline STATE: it is added when the watch says so, never painted on luck.
fn session_recordings(cut_: &Cut) -> Vec<crate::timeline::Recording> {
    let spans = crate::ui::cut_folds::page_recordings();
    if spans.is_empty() {
        return crate::timeline::kept_footage_recordings(cut_);
    }
    spans
}

pub fn row_spans(cut_: &Cut) -> Vec<crate::timeline::Recording> {
    let held = {
        let seeded = crate::ui::cut_folds::page_recordings();
        if seeded.is_empty() {
            crate::timeline::kept_footage_recordings(cut_)
        } else {
            seeded
        }
    };
    // A row held open by the floor (`cut.nrows`, §F2.11 S2) is a row the page draws and numbers, so
    // the spans it holds must survive this filter or the emptied bottom row's ✕ is asked about a
    // phantom instead of the row it was drawn over. The floor only ADDS rows, never removes them.
    let rows_now: Vec<String> = crate::timeline::kept_footage_recordings(cut_)
        .iter()
        .map(|span| span.base.clone())
        .chain(cut_.lanes.iter().map(|lane| lane.name.clone()))
        .chain((0..cut_.nrows.max(0)).map(|row| format!("held-row-{row}")))
        .collect();
    let held: Vec<crate::timeline::Recording> = held
        .into_iter()
        .filter(|span| rows_now.contains(&span.base))
        .collect();
    // Nothing in the session list names a camera this cut shows — a declared take whose file the
    // cut never reads.
    if held.is_empty() {
        return crate::ui::cut_folds::page_recordings();
    }
    held
}

/// The rail row a gutter control belongs to, in px: one row pitch per row above it, measured with
/// the same thumbnail height the band paints at (`cut_tracks::thumb_px`, which the 🖼 ladder moves),
/// so the switch and the row it mutes cannot drift apart.
fn gutter_row_top(row: usize) -> i32 {
    crate::ui::cut_tracks::rows_pitch_px() * (row as i32 + 1)
}

fn rail_child(rail: &gtk::Box, name: &str) -> Option<gtk::Widget> {
    rail.observe_children()
        .iter::<glib::Object>()
        .flatten()
        .filter_map(|child| child.downcast::<gtk::Widget>().ok())
        .find(|child| child.widget_name() == name)
}

pub fn refresh_camera_rows(window: &adw::ApplicationWindow) {
    let Some(rows_box) = camera_rows_box(window) else {
        return;
    };
    let cut_ = newest_review_cut();
    let recordings = row_spans(&cut_);
    let placed = crate::timeline::rows_for(&recordings, &cut_);
    let placed_recordings = recordings.clone();
    let placed_rows = placed.clone();
    let total = crate::timeline::row_count(&recordings, &cut_);
    let lanes: Vec<String> = cut_.lanes.iter().map(|lane| lane.name.clone()).collect();
    // F2.10 S2: the badges answer about a CHOSEN scene.
    let scene = badge_scene();
    let at = line_second();
    let watched = watched_row();

    let drawn = rows_box.observe_children().n_items() as usize;
    CUT_PAGE.with(
        |CutPage {
             camera_refresh: cell,
             ..
         }| cell.set(true),
    );
    // The dashed outline the spec's **4** asks for has to be painted, not merely named: GTK draws no
    // class of its own, so a `watched-row` class with no CSS behind it is invisible. A frame around the
    // row box is what reads as "this row is the one the preview shows", and it is installed once here
    // rather than in a stylesheet because nothing else in this module adds display-wide CSS.
    #[expect(
        deprecated,
        reason = "the 4.10/4.12 replacements have no drop-in for a display-wide provider"
    )]
    {
        if CUT_PAGE.with(
            |CutPage {
                 watched_css_loaded: cell,
                 ..
             }| !cell.get(),
        ) {
            let watched_box = gtk::CssProvider::new();
            watched_box.load_from_data(".watched-row { border: 2px dashed #3a63c8; }");
            gtk::StyleContext::add_provider_for_display(
                &gtk::gdk::Display::default().expect("a display to style"),
                &watched_box,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
            CUT_PAGE.with(
                |CutPage {
                     watched_css_loaded: cell,
                     ..
                 }| cell.set(true),
            );
        }
    }
    for index in 0..total.max(drawn) {
        let row = index as i32;
        // F2.11 S2: is this the bottom row and empty?
        let cross_live =
            index + 1 == total && cut_fold::row_is_empty(index, &recordings, &placed, &cut_.segs);

        // Reuse the row this window already drew rather than appending a second copy: `find_widget_by_name`
        // matches GTK's widget name, so a duplicate `camera-row-1` would leave the FIRST one — the
        // widget a test finds by that name — frozen at its old label while a hidden twin carried the
        // new state. Building once and re-styling is what keeps the on-screen plate and the rule the
        // same object.
        let holder =
            match find_widget_by_name(rows_box.upcast_ref(), &format!("camera-row-{}", index + 1))
                .and_then(|found| found.downcast::<gtk::Box>().ok())
            {
                Some(existing) => existing,
                None => {
                    let fresh = gtk::Box::new(gtk::Orientation::Horizontal, 6);
                    fresh.set_widget_name(&format!("camera-row-{}", index + 1));
                    rows_box.append(&fresh);
                    fresh
                }
            };
        if watched == Some(index) {
            holder.add_css_class("watched-row");
        } else {
            holder.remove_css_class("watched-row");
        }
        while let Some(old) = holder.first_child() {
            holder.remove(&old);
        }

        let occupant = recordings
            .iter()
            .zip(&placed)
            .find(|(_, placed_row)| **placed_row as i32 == row);
        let (plate, base) = match occupant {
            Some((rec, _)) => {
                let shift = cut_.shift.get(&rec.base).copied().unwrap_or(0.0);
                (cut_cam::name_plate(&rec.base, shift, 0.0), rec.base.clone())
            }
            None => (cut_cam::row_name(row, &[], &[]), String::new()),
        };
        let plate_button = gtk::Button::with_label(&plate);
        plate_button.set_widget_name(&format!("watch-row-{}", index + 1));
        plate_button.set_tooltip_text(Some(&format!(
            "watch {} in the preview \u{2014} \u{25b6} hands the preview back to the cut",
            if base.is_empty() {
                plate.as_str()
            } else {
                base.as_str()
            }
        )));
        let win = window.clone();
        plate_button.connect_clicked(move |_| {
            press_watch_row(&win, row);
        });
        holder.append(&plate_button);

        // S1's lens control now lives on the SCENE (see the `scene-lens-<index>` buttons below), so
        // the row header carries no lens badge: a button on the row prints "the scene at m:ss is
        // shown from <cam> now" for a scene the row button never named, which is what this move
        // fixes.

        for lane in lanes.iter() {
            let lane = lane.clone();
            let heard = lane_heard_in_line_scene(&cut_, &lane, scene);
            let badge = gtk::ToggleButton::new();
            badge.set_widget_name(&format!("speaker-badge-{lane}"));
            badge.set_label(SPEAKER_GLYPH);
            badge.set_active(!heard);
            badge.set_sensitive(scene.is_some());
            badge.set_tooltip_text(Some(&cut_hear::hush_status(
                lane.as_str(),
                heard,
                scene.map(|i| cut_.segs[i].s).unwrap_or(at),
            )));
            let win = window.clone();
            let badge_lane = lane.clone();
            badge.connect_toggled(move |toggled| {
                if refreshing() {
                    return;
                }
                if press_speaker_badge(&win, &badge_lane).is_none() {
                    restate_face(toggled, false);
                }
            });
            holder.append(&badge);

            let gutter = gtk::ToggleButton::new();
            gutter.set_widget_name(&format!("gutter-switch-{lane}"));
            gutter.set_label(SPEAKER_GLYPH);
            gutter.add_css_class("round");
            let heard_whole_cut = cut_hear::lane_is_heard_anywhere(&cut_, &[lane.as_str()]);
            gutter.set_active(heard_whole_cut);
            gutter.set_tooltip_text(Some(&format!("{lane} for the whole cut")));
            let win = window.clone();
            let lane_switch = lane.clone();
            let switch_name = format!("gutter-switch-{lane_switch}");
            gutter.connect_toggled(move |switch| {
                if refreshing() {
                    return;
                }
                press_gutter_switch(&win, lane_switch.as_str());
                let fresh = newest_review_cut();
                restate_face(
                    switch,
                    cut_hear::lane_is_heard_anywhere(&fresh, &[lane.as_str()]),
                );
            });
            // §B's gutter: the whole-cut switch lives in the band's 30 px rail, not in the row
            // list, and it is found rail-locally rather than by a name search.
            gutter.set_margin_top(gutter_row_top(index));
            gutter.set_halign(gtk::Align::Start);
            let rail = crate::ui::cut_tracks::gutter_switches_box(window);
            // Build once, re-state after: a switch already on the rail keeps its one identity,
            // exactly as `watch-row-N` does in the row list.
            let kept = rail
                .as_ref()
                .and_then(|rail| rail_child(rail, &switch_name))
                .and_then(|found| found.downcast::<gtk::ToggleButton>().ok());
            match (kept, rail.as_ref()) {
                (Some(existing), _) => existing.set_active(heard_whole_cut),
                (None, Some(rail)) => rail.append(&gutter),
                // A page built without a band keeps the switch beside its plate.
                (None, None) => holder.append(&gutter),
            }
        }
        // §05-cut §1's per-row sound switch: the gutter holds "per-lane and per-row sound
        // switches", and a camera row's own sound (the scene's `mute`, S2) had none, so the row's
        // sound could only be switched scene by scene.
        if let Some(layer) = crate::ui::cut_tracks::gutter_row_switches(window) {
            let switch_name = format!("row-switch-{}", index + 1);
            let heard = cut_hear::row_is_heard_anywhere(&cut_, row);
            let y = crate::ui::cut_tracks::row_wave_top(index)
                + (crate::ui::cut_tracks::row_wave_px() - ROW_SWITCH_PX) / 2.0;
            match find_widget_by_name(layer.upcast_ref(), &switch_name)
                .and_then(|found| found.downcast::<gtk::ToggleButton>().ok())
            {
                Some(existing) => {
                    existing.set_active(heard);
                    layer.move_(&existing, 2.0, y);
                }
                None => {
                    let switch = gtk::ToggleButton::new();
                    switch.set_widget_name(&switch_name);
                    switch.set_label(SPEAKER_GLYPH);
                    switch.add_css_class("round");
                    switch.set_active(heard);
                    let name = if base.is_empty() {
                        plate.clone()
                    } else {
                        base.clone()
                    };
                    switch.set_tooltip_text(Some(&format!("{name}'s own sound for the whole cut")));
                    let win = window.clone();
                    switch.connect_toggled(move |switch| {
                        if refreshing() {
                            return;
                        }
                        press_row_switch(&win, row, &name);
                        restate_face(
                            switch,
                            cut_hear::row_is_heard_anywhere(&newest_review_cut(), row),
                        );
                    });
                    layer.put(&switch, 2.0, y);
                }
            }
        }
        // F2.11 S2 (`An emptied bottom row stays until its \u{2715}`): the cross on the emptied
        // bottom row, sensitive only where `cut_fold::kill_row` would answer.
        let rail = crate::ui::cut_tracks::gutter_switches_box(window);
        let found = rail
            .as_ref()
            .and_then(|rail| rail_child(rail, &format!("row-cross-{}", index + 1)))
            .and_then(|found| found.downcast::<gtk::Button>().ok())
            .or_else(|| {
                find_widget_by_name(rows_box.upcast_ref(), &format!("row-cross-{}", index + 1))
                    .and_then(|found| found.downcast::<gtk::Button>().ok())
            });
        let cross = match found {
            Some(existing) => existing,
            None => {
                let fresh = gtk::Button::new();
                fresh.set_widget_name(&format!("row-cross-{}", index + 1));
                fresh.set_label(CROSS_GLYPH);
                // F2.7 ✕ badges: the row's cross IS the `empty row` badge, so it fires through the
                // one badge door (`cut_verb_press::press_badge`), which forwards to
                // `press_row_cross`.
                fresh.set_tooltip_text(Some(cut_verbs::badge_action(Badge::EmptyRow)));
                let win = window.clone();
                fresh.connect_clicked(move |_| {
                    press_badge(&win, Badge::EmptyRow, index);
                });
                fresh.set_margin_top(gutter_row_top(index));
                fresh.set_halign(gtk::Align::Start);
                match rail.as_ref() {
                    Some(rail) => rail.append(&fresh),
                    None => holder.append(&fresh),
                }
                fresh
            }
        };
        cross.set_sensitive(cross_live);
        cross.set_tooltip_text(Some(if cross_live {
            "this row is empty \u{2014} take it off the band"
        } else {
            "this row still has footage \u{2014} nothing to remove here"
        }));

        for lane in &lanes {
            let cross =
                match find_widget_by_name(rows_box.upcast_ref(), &format!("lane-cross-{lane}"))
                    .and_then(|found| found.downcast::<gtk::Button>().ok())
                {
                    Some(existing) => existing,
                    None => {
                        let fresh = gtk::Button::new();
                        fresh.set_widget_name(&format!("lane-cross-{lane}"));
                        fresh.set_label(CROSS_GLYPH);
                        // F2.7 ✕ badges: a cut lane's cross is the `cut lane` badge.
                        let lane_index = cut_
                            .lanes
                            .iter()
                            .position(|kept| kept.name == *lane)
                            .unwrap_or(usize::MAX);
                        fresh.set_tooltip_text(Some(cut_verbs::badge_action(Badge::CutLane)));
                        let win = window.clone();
                        fresh.connect_clicked(move |_| {
                            press_badge(&win, Badge::CutLane, lane_index);
                        });
                        holder.append(&fresh);
                        fresh
                    }
                };
            cross.set_sensitive(cut_.lanes.iter().any(|kept| &kept.name == lane));
        }
        // NOT re-appended: `holder` is already inside `rows_box` — it was put there when first built.
        // Appending it again is the `gtk_box_append: assertion 'gtk_widget_get_parent (child) == NULL'
        // failed` that shows up on every refresh after the first.

        // F2.7 ✕ badges, the green bar's turn: the band is cairo-painted, so a scene's ✕ cannot
        // live on the bar itself.
        for (index, seg) in cut_.segs.iter().enumerate() {
            let mine = seg.ins.is_empty() && seg.cam == row;
            let name = format!("scene-cross-{index}");
            let found = find_widget_by_name(rows_box.upcast_ref(), &name)
                .and_then(|found| found.downcast::<gtk::Button>().ok());
            if !mine {
                if let Some(stale) = found {
                    if let Some(parent) = stale
                        .parent()
                        .and_then(|node| node.downcast::<gtk::Box>().ok())
                    {
                        parent.remove(&stale);
                    }
                }
                continue;
            }
            let cross = match found {
                Some(existing) => existing,
                None => {
                    let fresh = gtk::Button::new();
                    fresh.set_widget_name(&name);
                    fresh.set_label(CROSS_GLYPH);
                    fresh.set_tooltip_text(Some(cut_verbs::badge_action(Badge::GreenBar)));
                    let win = window.clone();
                    fresh.connect_clicked(move |_| {
                        press_badge(&win, Badge::GreenBar, index);
                    });
                    holder.append(&fresh);
                    fresh
                }
            };
            // Sensitivity IS the rule: a bar too narrow to carry a ✕ clear of its grips offers
            // none, asked of the same threshold the effect ✕ uses so the two can never disagree
            // about width.
            let live = crate::ui::cut_verb_press::scene_cross_live(seg.e - seg.s);
            cross.set_sensitive(live);

            // F2.10 S1: the 🔍 sits with ITS SCENE, beside that scene's ✕, because the question it
            // answers is about one scene ("the scene at m:ss is shown from <cam> now") and a
            // row-header badge could only answer it for whatever scene the line happened to sit in.
            let lens_name = format!("scene-lens-{index}");
            let found_lens = find_widget_by_name(rows_box.upcast_ref(), &lens_name)
                .and_then(|found| found.downcast::<gtk::Button>().ok());
            if !mine {
                if let Some(stale) = found_lens {
                    if let Some(parent) = stale
                        .parent()
                        .and_then(|node| node.downcast::<gtk::Box>().ok())
                    {
                        parent.remove(&stale);
                    }
                }
                continue;
            }
            let lens = match found_lens {
                Some(existing) => existing,
                None => {
                    let fresh = gtk::Button::new();
                    fresh.set_widget_name(&lens_name);
                    fresh.set_label(LENS_GLYPH);
                    let win = window.clone();
                    fresh.connect_clicked(move |_| {
                        press_lens_scene(&win, index);
                    });
                    holder.append(&fresh);
                    fresh
                }
            };
            // The tooltip says what a press would DO, which for a cycle is the NEXT camera rather
            // than the current one.
            let filming = cut_cam::filming_rows(seg, &placed_recordings, &placed_rows);
            let tip = match cut_cam::next_filming_row(seg.cam, &filming) {
                Some(row) => format!(
                    "{LENS_GLYPH} show the scene at {} from {} instead",
                    cut_hear::scene_clock(seg.s),
                    cut_cam::row_name(row, &placed_recordings, &placed_rows)
                ),
                None => format!(
                    "no other camera filmed the scene at {}",
                    cut_hear::scene_clock(seg.s)
                ),
            };
            lens.set_tooltip_text(Some(&tip));
            lens.set_sensitive(cut_cam::next_filming_row(seg.cam, &filming).is_some());
        }
    }
    CUT_PAGE.with(
        |CutPage {
             camera_refresh: cell,
             ..
         }| cell.set(false),
    );
}

fn refreshing() -> bool {
    CUT_PAGE.with(
        |CutPage {
             camera_refresh: cell,
             ..
         }| cell.get(),
    )
}

/// Put a switch's face back to what the cut now says, inside the same guard a refresh holds, so the
/// restate can never be taken for a person's press and fire the switch's handler again.
fn restate_face(switch: &gtk::ToggleButton, active: bool) {
    CUT_PAGE.with(
        |CutPage {
             camera_refresh: cell,
             ..
         }| {
            let was = cell.replace(true);
            switch.set_active(active);
            cell.set(was);
        },
    );
}

/// F2.10 S1 (`🔍 lens badge: which row its picture comes from`): the seam a SCENE's 🔍 forwards to.
pub fn press_lens_scene(window: &adw::ApplicationWindow, scene: usize) -> Option<String> {
    let mut cut_ = newest_review_cut();
    let seg = cut_.segs.get(scene).filter(|seg| seg.ins.is_empty())?;
    let recordings = session_recordings(&cut_);
    let rows = crate::timeline::rows_for(&recordings, &cut_);
    let filming = cut_cam::filming_rows(seg, &recordings, &rows);
    let Some(row) = cut_cam::next_filming_row(seg.cam, &filming) else {
        // The refusal says WHERE the person is and why there is nowhere to go, rather than doing
        // nothing.
        let said = format!(
            "the scene at {} is the only footage there \u{2014} nothing else filmed those seconds",
            cut_hear::scene_clock(seg.s)
        );
        if let Some(status) = find_status(window.upcast_ref()) {
            status.set_text(&said);
        }
        return None;
    };
    let said = cut_cam::show_scene_from(&mut cut_, scene, row, &recordings, &rows)?;
    crate::ui::window::save_insert_cut(&cut_);
    record_edit(window, &cut_);
    if let Some(status) = find_status(window.upcast_ref()) {
        status.set_text(&said);
    }
    refresh_camera_rows(window);
    Some(said)
}

/// F2.10 S2 (`🔈 speaker badge per lane: does this scene hear that lane`): the seam a badge forwards
/// to.
pub fn press_speaker_badge(window: &adw::ApplicationWindow, lane: &str) -> Option<String> {
    let scene = badge_scene()?;
    let mut cut_ = newest_review_cut();
    let said = cut_hear::toggle_heard(&mut cut_, scene, lane)?;
    record_edit(window, &cut_);
    if let Some(status) = find_status(window.upcast_ref()) {
        status.set_text(&said);
    }
    refresh_camera_rows(window);
    Some(said)
}

/// F2.10 S2 for the scene's OWN sound: the speaker badge on the badge scene's wave strip, off or on
/// for that scene (its `mute`, the property F2.5 S2 hushes the footage by).
pub fn press_scene_sound(window: &adw::ApplicationWindow) -> Option<String> {
    let scene = badge_scene()?;
    let mut cut_ = newest_review_cut();
    let recordings = session_recordings(&cut_);
    let rows = crate::timeline::rows_for(&recordings, &cut_);
    let seg = cut_.segs.get_mut(scene).filter(|seg| seg.ins.is_empty())?;
    seg.mute = !seg.mute;
    let said = cut_hear::hush_status(
        &cut_cam::row_name(seg.cam, &recordings, &rows),
        !seg.mute,
        seg.s,
    );
    record_edit(window, &cut_);
    if let Some(status) = find_status(window.upcast_ref()) {
        status.set_text(&said);
    }
    refresh_camera_rows(window);
    Some(said)
}

pub fn press_lens_row(window: &adw::ApplicationWindow, row: usize) -> Option<String> {
    let scene = badge_scene()?;
    let mut cut_ = newest_review_cut();
    if cut_.segs.get(scene)?.cam == row as i32 {
        return None;
    }
    let recordings = session_recordings(&cut_);
    let rows = crate::timeline::rows_for(&recordings, &cut_);
    let said = cut_cam::show_scene_from(&mut cut_, scene, row as i32, &recordings, &rows)?;
    crate::ui::window::save_insert_cut(&cut_);
    record_edit(window, &cut_);
    if let Some(status) = find_status(window.upcast_ref()) {
        status.set_text(&said);
    }
    refresh_camera_rows(window);
    Some(said)
}

/// F2.10 S2 (`the gutter switch toggles a lane for the whole cut`): the seam the gutter forwards
/// to.
pub fn press_row_switch(window: &adw::ApplicationWindow, row: i32, name: &str) -> String {
    let mut cut_ = newest_review_cut();
    let said = cut_hear::toggle_row_all(&mut cut_, row, name);
    record_edit(window, &cut_);
    if let Some(status) = find_status(window.upcast_ref()) {
        status.set_text(&said);
    }
    refresh_camera_rows(window);
    said
}

/// A round gutter switch's height (`cut-gutter-col button.round`: 22 px).
const ROW_SWITCH_PX: f64 = 22.0;

pub fn press_gutter_switch(window: &adw::ApplicationWindow, lane: &str) -> String {
    let mut cut_ = newest_review_cut();
    let said = cut_hear::toggle_lane_all(&mut cut_, &[lane], lane);
    record_edit(window, &cut_);
    if let Some(status) = find_status(window.upcast_ref()) {
        status.set_text(&said);
    }
    refresh_camera_rows(window);
    said
}

/// F2.10 S3: ▶ hands the preview back — the seam [`hand_preview_back`] is reached through, kept
/// separate so a test can count how many times the watch was released without a real ▶.
pub fn hand_preview_back(window: &adw::ApplicationWindow) -> Option<String> {
    let was = {
        let watch = camera_watch();
        let mut watch = watch.borrow_mut();
        watch.play_hands_back()
    };
    if !was {
        return None;
    }
    // The row's own name comes from this window's recordings, so the sentence names a camera rather
    // than a number.
    let cut_ = newest_review_cut();
    // F2.10: the SESSION's recordings, from the open project's own footage rows.
    let recordings = session_recordings(&cut_);
    let rows = crate::timeline::rows_for(&recordings, &cut_);
    let named = watched_row()
        .map(|row| cut_cam::row_name(row as i32, &recordings, &rows))
        .unwrap_or_else(|| "the watched row".to_string());
    set_watched_row(None);
    refresh_camera_rows(window);
    crate::ui::cut_picture::refresh(window);
    Some(format!(
        "\u{25b6} plays the cut \u{2014} {named} no longer watched"
    ))
}

/// F2.10: the box holding this window's camera rows, by its stable name — `None` on a page that
/// drew none.
pub fn camera_rows_box(window: &adw::ApplicationWindow) -> Option<gtk::Box> {
    let content = window.content()?;
    find_widget_by_name(&content, "camera-rows")?
        .downcast()
        .ok()
}
