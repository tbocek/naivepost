//! F2.10 cameras and hearing — the page half of the flow: the seams the badges and rows call,
//! and the row list itself.
//!
//! Split out of `window.rs` unchanged: the rules live in [`crate::cut_cam`] and [`crate::cut_hear`],
//! and what is here is only the state a press leaves behind and the widgets that show it — the lens
//! badge, the speaker badges, the gutter switches and the per-row crosses. No decision of its own.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use gtk4 as gtk;

use crate::cut::Cut;
use crate::cut_cam;
use crate::cut_fold;
use crate::cut_hear;
use crate::cut_verbs::{self, Badge};
use crate::ui::cut_verb_press::press_badge;
#[allow(unused_imports)] // kept for the seams' own callers; the crosses now fire through `press_badge`
use crate::ui::cut_folds::{press_lane_cross, press_row_cross, CROSS_GLYPH};
use crate::ui::window::{
    find_status, find_widget_by_name, held_segment, line_position, newest_review_cut, record_edit,
    set_watched_row, watched_row,
};

// F2.10 S3: this window's watch, newest slot last like [`REVIEW_CUTS`] so one window's click cannot
// answer another's ▶. The rule itself is [`cut_cam::Watch`]; this only remembers which one is live.
thread_local! {
    static CAM_WATCHES: std::cell::RefCell<Vec<Rc<RefCell<cut_cam::Watch>>>> =
        const { std::cell::RefCell::new(Vec::new()) };

    /// The watched-row frame's CSS, installed on the first refresh that needs it and never again.
    static WATCHED_CSS_LOADED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// F2.10: a named `gtk::ToggleButton` in this window — the lens badge, a speaker badge or a gutter
/// switch. `line_step_button` only downcasts to a plain Button, so the toggle half needs its own finder;
/// it is the same walk, and the same stable names the page drew.
pub fn toggle_button(window: &adw::ApplicationWindow, name: &str) -> Option<gtk::ToggleButton> {
    find_widget_by_name(window.upcast_ref(), name)?.downcast().ok()
}

/// F2.10 S3: this window's [`cut_cam::Watch`], created on first ask so a page that was never clicked
/// still has an empty one to hand to ▶.
pub fn camera_watch(window: &adw::ApplicationWindow) -> Rc<RefCell<cut_cam::Watch>> {
    let _ = window;
    CAM_WATCHES.with(|slots| {
        let mut slots = slots.borrow_mut();
        if let Some(latest) = slots.last() {
            return latest.clone();
        }
        let fresh = Rc::new(RefCell::new(cut_cam::Watch::default()));
        slots.push(fresh.clone());
        fresh
    })
}

/// The session second the red line stands on — the `t` every F2.10 sentence is about.
///
/// This is [`line_position`]'s number and nothing else: the line's own state, restored from
/// `cut/line.json` at open and moved by F2.4's steps. No second line position is invented here, so the
/// status says the same moment the ruler draws.
fn line_second(window: &adw::ApplicationWindow) -> f64 {
    line_position(window).t
}

/// The scene a lens or speaker badge means: the HELD clip if one is in hand, else the kept scene the line
/// stands in (both footage — an insert has no picture to re-source and no hearing of its own). `None` when
/// neither exists, which is what makes a badge refuse instead of editing scene 0.
fn badge_scene(window: &adw::ApplicationWindow) -> Option<usize> {
    cut_cam::held_or_line_scene(
        &newest_review_cut(),
        Some(line_second(window)),
        held_segment().as_ref(),
    )
}

/// F2.10 S3 (`if the line is in a kept scene shown from another row, status says ONCE …`): the seam a
/// row's name plate forwards to. [`cut_cam::click_row`] owns the whole decision, including
/// whether the "the cut shows camera M here" sentence is owed; this prints what comes back.
pub fn press_watch_row(window: &adw::ApplicationWindow, row: i32) -> Option<String> {
    let cut_ = newest_review_cut();
    let at = line_second(window);
    let said = {
        let watch = camera_watch(window);
        let mut watch = watch.borrow_mut();
        cut_cam::click_row(&mut watch, row, &cut_.segs, Some(at))
    };
    // Keep F2.4's slot in step: whatever the picture band click sets, a row click means the same thing
    // about which row this window is looking at.
    set_watched_row(Some(row.max(0) as usize));
    if let Some(line) = &said {
        if let Some(status) = find_status(window.upcast_ref()) {
            status.set_text(line);
        }
    }
    // The dashed outline moves with the watch, so the rows are rebuilt rather than merely queued.
    refresh_camera_rows(window);
    // F2.10 S3: watching a row IS what the preview shows, so the frame follows the watch. Which recording
    // and which second that means is [`cut_picture`]’s decision, not this module’s.
    crate::ui::cut_picture::refresh(window);
    said
}

// --- F2.10: drawing the rows --------------------------------------------------------------------------

/// The 🔍 glyph, spelled once so the badge and its tooltip cannot drift apart. (The headless container
/// has no icon theme, so — as with F2.5's "preview volume" word for a speaker icon — the glyph is drawn
/// as text rather than looked up.)
pub const LENS_GLYPH: &str = "\u{1f50d}";
/// The 🔈 glyph for a lane's speaker badge, spelled once for the same reason.
pub const SPEAKER_GLYPH: &str = "\u{1f508}";

/// Is a lane heard in the scene under the line? An unlisted lane is heard (`spec/inventory/cut.md` §A
/// rule 10: `quiet` lists the silent ones), and with no kept scene under the line there is nothing to
/// be silent in, so the badge reads heard and greyed rather than lying about a scene that isn't there.
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

/// Rebuild `camera-rows` from this window's own cut. One row per coloured row of the timeline
/// (`timeline::row_count`, which is the greedy colouring's answer plus `Cut::nrows`' floor — no second
/// colouring here), each holding, in this order: the clickable name plate (F2.10 S3), the lens badge
/// (S1), one speaker badge per lane (S2) and that lane's gutter switch (S2). Every widget takes its
/// state from the same functions the logic tests call, and every press goes through the same
/// `press_*` seam a test fires, so the drawing cannot disagree with the rule.
///
/// Called from `refresh_selection_readout` (which every draw, nudge and clear already calls) and after
/// each of this item's presses. Rebuilding rather than patching keeps the watched row's dashed outline
/// STATE: it is added when the watch says so, never painted on luck.
/// Which recordings the row list lays out: what this session filmed, when the page has been told (the
/// session slot `set_session_recordings` / [`crate::ui::cut_session`] fill); the kept clips' spans are the
/// answer for a page nobody seeded — and they must be laid out through `timeline::rows_for`, which is what
/// puts an overlap on its own row. Deriving them here rather than reading a slot keeps the row list, the
/// lens cycle and the hearing badges all asking the SAME colouring.
fn session_recordings(cut_: &Cut) -> Vec<crate::timeline::Recording> {
    let spans = crate::ui::cut_folds::page_recordings();
    if spans.is_empty() {
        return crate::timeline::kept_footage_recordings(cut_);
    }
    spans
}

/// The spans this window's row list colours: the page's session spans where a session was named, else what
/// the kept clips imply — MINUS any span that names neither a camera a kept scene shows nor a lane still in
/// the cut. A lane's ✕ takes its row back (§F2.11), and a phantom row would otherwise keep the row above it
/// from counting as the emptied bottom one. Both the drawing and `press_row_cross` ask here, so the ✕'s
/// sensitivity and its answer cannot disagree.
pub fn row_spans(cut_: &Cut) -> Vec<crate::timeline::Recording> {
    let held = {
        let seeded = crate::ui::cut_folds::page_recordings();
        if seeded.is_empty() {
            crate::timeline::kept_footage_recordings(cut_)
        } else {
            seeded
        }
    };
    // A row held open by the floor (`cut.nrows`, §F2.11 S2) is a row the page draws and numbers, so the
    // spans it holds must survive this filter or the emptied bottom row's ✕ is asked about a phantom instead
    // of the row it was drawn over. The floor only ADDS rows, never removes them.
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
    // Nothing in the session list names a camera this cut shows — a declared take whose file the cut never
    // reads. The rows are then what the session says they are: dropping every span would leave the page no
    // rows at all rather than the rows it was told about.
    if held.is_empty() {
        return crate::ui::cut_folds::page_recordings();
    }
    held
}

pub fn refresh_camera_rows(window: &adw::ApplicationWindow) {
    let Some(rows_box) = camera_rows_box(window) else {
        return;
    };
    let cut_ = newest_review_cut();
    // The spans to colour — see [`row_spans`]: the session's list, with a lane's row dropped once its ✕ took
    // the lane, which is what puts an overlap on its own row here too.
    let recordings = row_spans(&cut_);
    let placed = crate::timeline::rows_for(&recordings, &cut_);
    // The same pair the scene 🔍 below asks about, hoisted so the row list and the badge cannot disagree
    // about which camera filmed what.
    let placed_recordings = recordings.clone();
    let placed_rows = placed.clone();
    let total = crate::timeline::row_count(&recordings, &cut_);
    // The rows this window has already drawn, keyed by their widget name. A row is built ONCE and then
    // only re-styled: `find_widget_by_name` matches on GTK's *widget name*, which every control here
    // carries (`set_widget_name`), so a rebuild that appended a second `watch-row-1` would leave the
    // first one — the widget a test actually finds — frozen at its old label. Growing the list to the
    // row count and never shrinking it also keeps §B's emptied bottom row on screen until its ✕ lands
    // with F2.11, rather than dropping a row the user can still click.
    let lanes: Vec<String> = cut_.lanes.iter().map(|lane| lane.name.clone()).collect();
    // F2.10 S2: the badges answer about a CHOSEN scene — the one being held, when something is held — and
    // only fall back to wherever the red line happens to stand. A badge that could read only the line's own
    // scene made "<base> is silent in the scene at m:ss" speak of a scene nobody picked.
    let scene = badge_scene(window);
    let at = line_second(window);
    let watched = watched_row();

    let drawn = rows_box.observe_children().n_items() as usize;
    // The guard is set for the whole pass below: every `set_active` in it is a STATE PUSH, not a press.
    CAMERA_REFRESH.with(|cell| cell.set(true));
    // The dashed outline the spec's **4** asks for has to be painted, not merely named: GTK draws no
    // class of its own, so a `watched-row` class with no CSS behind it is invisible. A frame around the
    // row box is what reads as "this row is the one the preview shows", and it is installed once here
    // rather than in a stylesheet because nothing else in this module adds display-wide CSS.
    #[expect(
        deprecated,
        reason = "the 4.10/4.12 replacements have no drop-in for a display-wide provider"
    )]
    {
        if WATCHED_CSS_LOADED.with(|cell| !cell.get()) {
            let watched_box = gtk::CssProvider::new();
            watched_box.load_from_data(".watched-row { border: 2px dashed #3a63c8; }");
            gtk::StyleContext::add_provider_for_display(
                &gtk::gdk::Display::default().expect("a display to style"),
                &watched_box,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
            WATCHED_CSS_LOADED.with(|cell| cell.set(true));
        }
    }
    for index in 0..total.max(drawn) {
        let row = index as i32;
        // F2.11 S2: is this the bottom row and empty? Asked of `cut_fold::row_is_empty` so the ✕'s
        // sensitivity IS the rule — a greyed cross means "this row has footage / is the last one",
        // never a guess made at draw time.
        let cross_live = index + 1 == total && cut_fold::row_is_empty(index, &recordings, &placed, &cut_.segs);

        // Reuse the row this window already drew rather than appending a second copy: `find_widget_by_name`
        // matches GTK's widget name, so a duplicate `camera-row-1` would leave the FIRST one — the
        // widget a test finds by that name — frozen at its old label while a hidden twin carried the
        // new state. Building once and re-styling is what keeps the on-screen plate and the rule the
        // same object.
        let holder = match find_widget_by_name(rows_box.upcast_ref(), &format!("camera-row-{}", index + 1))
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
            // **4** in `spec/img/05-rows.png`: the watched row is outlined, not filled. BOTH branches
            // run on every pass, so the outline can move OFF a row as well as onto one.
            holder.add_css_class("watched-row");
        } else {
            holder.remove_css_class("watched-row");
        }
        // The holder is reused so one row answers to one name; its CONTROLS, however, have to be made
        // fresh each pass (`gtk::Box::append` refuses a child that already has a parent, and the
        // labels, glyphs and toggle faces all carry this pass's state). Drain the old set first, then
        // build — which is why the block below appends without checking.
        while let Some(old) = holder.first_child() {
            holder.remove(&old);
        }

        // The name plate: what lies on this row, which part of that file it shows, and the shift
        // correction the page was opened with (**2**'s "−19.00 s"). A row with nothing on it still
        // gets its plate, named by `cut_cam::row_name`'s fallback, because an empty bottom row is a
        // real row until its ✕ (§B) and an unnamed row cannot be watched back.
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

        // S1's lens control now lives on the SCENE (see the `scene-lens-<index>` buttons below), so the row
        // header carries no lens badge: a button on the row prints "the scene at m:ss is shown from <cam>
        // now" for a scene the row button never named, which is what this move fixes.

        // S2: one speaker badge per lane, plus that lane's gutter switch beside it.
        for lane in lanes.iter() {
            // Owned by the closures below, which outlive this row's build.
            let lane = lane.clone();
            let heard = lane_heard_in_line_scene(&cut_, &lane, scene);
            // The lane name each closure below owns its own copy of, so no closure borrows a loop
            // variable that dies at the next row.
            let badge = gtk::ToggleButton::new();
            badge.set_widget_name(&format!("speaker-badge-{lane}"));
            badge.set_label(SPEAKER_GLYPH);
            // Pressed = silent, so the badge reads as struck-through sound when the scene does not hear.
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
                    toggled.set_active(false);
                }
            });
            holder.append(&badge);

            let gutter = gtk::ToggleButton::new();
            gutter.set_widget_name(&format!("gutter-switch-{lane}"));
            gutter.set_label("\u{25ac}");
            // OFF (not pressed) when the lane is silent EVERYWHERE, on while any scene still hears it —
            // the same reading `cut_hear::toggle_lane_all` acts on, so the switch never claims a state
            // the next press contradicts.
            gutter.set_active(cut_hear::lane_is_heard_anywhere(&cut_, &[lane.as_str()]));
            gutter.set_tooltip_text(Some(&format!("{lane} for the whole cut")));
            let win = window.clone();
            gutter.connect_toggled(move |switch| {
                if refreshing() {
                    return;
                }
                press_gutter_switch(&win, lane.as_str());
                // The switch's own face is the whole-cut answer, which `toggle_lane_all` just wrote;
                // re-read it so a half-silenced row that went fully silent shows exactly that.
                let fresh = newest_review_cut();
                switch.set_active(cut_hear::lane_is_heard_anywhere(&fresh, &[lane.as_str()]));
            });
            holder.append(&gutter);
        }
        // F2.11 S2 (`An emptied bottom row stays until its \u{2715}`): the cross on the emptied bottom
        // row, sensitive only where `cut_fold::kill_row` would answer — so a live ✕ IS the promise that
        // pressing it takes the row, and a greyed one is today's refusal, not a missing feature.
        // DECISION: the cross is BUILT ONCE per row name and afterwards only re-sensitised, exactly like
        // the plate/badges above it. Re-appending it each pass would leave the earlier child holding the
        // old sensitivity under the same widget name, which is the F2.10 lesson this function already
        // records for `watch-row-1`.
        let found = find_widget_by_name(rows_box.upcast_ref(), &format!("row-cross-{}", index + 1))
            .and_then(|found| found.downcast::<gtk::Button>().ok());
        let cross = match found {
            Some(existing) => existing,
            None => {
                let fresh = gtk::Button::new();
                fresh.set_widget_name(&format!("row-cross-{}", index + 1));
                fresh.set_label(CROSS_GLYPH);
                // F2.7 ✕ badges: the row's cross IS the `empty row` badge, so it fires through the one
                // badge door (`cut_verb_press::press_badge`), which forwards to `press_row_cross` — the
                // sentence and the state stay F2.11's, and `cut_verbs::Badge` gains a real caller.
                fresh.set_tooltip_text(Some(cut_verbs::badge_action(Badge::EmptyRow)));
                let win = window.clone();
                fresh.connect_clicked(move |_| {
                    press_badge(&win, Badge::EmptyRow, index);
                });
                holder.append(&fresh);
                fresh
            }
        };
        // Property-only update: the RULE decides sensitivity, nothing is decided at draw time.
        cross.set_sensitive(cross_live);
        cross.set_tooltip_text(Some(if cross_live {
            "this row is empty \u{2014} take it off the band"
        } else {
            "this row still has footage \u{2014} nothing to remove here"
        }));

        // F2.11 S2 (`a cut lane's \u{2715} removes the lane, its pins, shift, pictures and sound`):
        // one cross per cut lane, named by the lane so a test fires the lane it is looking at. Built
        // once per lane name and never re-appended; a lane removed by a press simply stops being redrawn
        // because its name no longer comes from `cut.lanes`.
        for lane in &lanes {
            let cross = match find_widget_by_name(rows_box.upcast_ref(), &format!("lane-cross-{lane}"))
                .and_then(|found| found.downcast::<gtk::Button>().ok())
            {
                Some(existing) => existing,
                None => {
                    let fresh = gtk::Button::new();
                    fresh.set_widget_name(&format!("lane-cross-{lane}"));
                    fresh.set_label(CROSS_GLYPH);
                    // F2.7 ✕ badges: a cut lane's cross is the `cut lane` badge. Its ORDER in `cut.lanes` is
                    // the target `press_badge` asks for, so it is taken here, where the lane is known, rather
                    // than guessed from the name later.
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
            // The cross goes with its lane: still listed -> still pressable; gone from `cut.lanes` (the
            // press took it) -> insensitive rather than left firing at a lane that no longer exists.
            cross.set_sensitive(cut_.lanes.iter().any(|kept| &kept.name == lane));
        }
        // NOT re-appended: `holder` is already inside `rows_box` — it was put there when first built.
        // Appending it again is the `gtk_box_append: assertion 'gtk_widget_get_parent (child) == NULL'
        // failed` that shows up on every refresh after the first.

        // F2.7 ✕ badges, the green bar's turn: the band is cairo-painted, so a scene's ✕ cannot live on the
        // bar itself — it sits in that scene's row, in the row list, named `scene-cross-<scene index>` after
        // its place in `cut.segs` so a test fires the scene it is looking at. DECISION (spec silent on the
        // exact seat): the row of the camera that shows the scene is the one seat that is always drawn, always
        // labelled and already inside the row this scene belongs to.
        for (index, seg) in cut_.segs.iter().enumerate() {
            // Footage on THIS row only: an inserted card shows no camera's pictures, so it has no scene ✕,
            // and a scene living on another row gets its ✕ over there, not here.
            let mine = seg.ins.is_empty() && seg.cam == row;
            let name = format!("scene-cross-{index}");
            let found = find_widget_by_name(rows_box.upcast_ref(), &name)
                .and_then(|found| found.downcast::<gtk::Button>().ok());
            if !mine {
                // Put it away when the scene moved off this row or left the cut, so a greyed ✕ never sits on
                // a row that no longer shows that scene.
                if let Some(stale) = found {
                    if let Some(parent) = stale.parent().and_then(|node| node.downcast::<gtk::Box>().ok()) {
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
            // Sensitivity IS the rule: a bar too narrow to carry a ✕ clear of its grips offers none, asked of
            // the same threshold the effect ✕ uses so the two can never disagree about width. A narrow scene
            // keeps its button and goes INSENSITIVE rather than being un-drawn: an absent control is
            // indistinguishable from a bug, a greyed one reads as today's refusal.
            let live = crate::ui::cut_verb_press::scene_cross_live(seg.e - seg.s);
            cross.set_sensitive(live);

            // F2.10 S1: the 🔍 sits with ITS SCENE, beside that scene's ✕, because the question it answers is
            // about one scene ("the scene at m:ss is shown from <cam> now") and a row-header badge could only
            // answer it for whatever scene the line happened to sit in. Named `scene-lens-<index>` after its
            // place in `cut.segs`, so a test fires the scene it is looking at; created once and re-styled in
            // place like the cross, and greyed (never hidden) when there is nowhere to switch to.
            let lens_name = format!("scene-lens-{index}");
            let found_lens = find_widget_by_name(rows_box.upcast_ref(), &lens_name)
                .and_then(|found| found.downcast::<gtk::Button>().ok());
            if !mine {
                if let Some(stale) = found_lens {
                    if let Some(parent) = stale.parent().and_then(|node| node.downcast::<gtk::Box>().ok()) {
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
            // The tooltip says what a press would DO, which for a cycle is the NEXT camera rather than the
            // current one — reading today's answer off the button would tell the person nothing new.
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
    CAMERA_REFRESH.with(|cell| cell.set(false));
}

// F2.10: set while [`refresh_camera_rows`] is pushing state INTO the widgets, so a `set_active`
// made by the refresh cannot be mistaken for a user press and re-enter the press that asked for the
// refresh. Without this the chain `press_* -> refresh -> set_active -> connect_toggled -> press_*`
// never ends.
thread_local! {
    static CAMERA_REFRESH: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Is a camera-row refresh in progress right now? Every toggle handler starts with this and returns if set.
fn refreshing() -> bool {
    CAMERA_REFRESH.with(|cell| cell.get())
}

/// F2.10 S1 (`🔍 lens badge: which row its picture comes from`): the seam a SCENE's 🔍 forwards to.
///
/// A press CYCLES to the next row that filmed the scene's whole span (see [`cut_cam::filming_rows`] for why
/// partial coverage is not an option, and [`cut_cam::next_filming_row`] for the cycle). DECISION the spec
/// leaves open — it names one badge per scene and no menu: cycling is what lets a single button reach every
/// camera that filmed those seconds; a menu is not what a round badge invites.
pub fn press_lens_scene(window: &adw::ApplicationWindow, scene: usize) -> Option<String> {
    let mut cut_ = newest_review_cut();
    let seg = cut_.segs.get(scene).filter(|seg| seg.ins.is_empty())?;
    let recordings = session_recordings(&cut_);
    let rows = crate::timeline::rows_for(&recordings, &cut_);
    let filming = cut_cam::filming_rows(seg, &recordings, &rows);
    let Some(row) = cut_cam::next_filming_row(seg.cam, &filming) else {
        // The refusal says WHERE the person is and why there is nowhere to go, rather than doing nothing.
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
/// to. With no kept scene under the line there is no scene to be silent in, so nothing is printed and
/// nothing changes.
pub fn press_speaker_badge(window: &adw::ApplicationWindow, lane: &str) -> Option<String> {
    let scene = badge_scene(window)?;
    let mut cut_ = newest_review_cut();
    let said = cut_hear::toggle_heard(&mut cut_, scene, lane)?;
    record_edit(window, &cut_);
    if let Some(status) = find_status(window.upcast_ref()) {
        status.set_text(&said);
    }
    refresh_camera_rows(window);
    Some(said)
}

/// F2.10 S2 (`the gutter switch toggles a lane for the whole cut`): the seam the gutter forwards to.
/// The switch stands for every recording sharing that lane's name, so `lanes` is that name once and
/// `name` is what the status calls it — the same string, because today one switch speaks for one
/// recording. [`cut_hear::toggle_lane_all`] answers even when there is nothing to change ("… is in no
/// scene yet"), so this always has a sentence to print.
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

/// F2.10 S3: ▶ hands the preview back — the seam [`hand_preview_back`] is reached through, kept separate
/// so a test can count how many times the watch was released without a real ▶.
pub fn hand_preview_back(window: &adw::ApplicationWindow) -> Option<String> {
    let was = {
        let watch = camera_watch(window);
        let mut watch = watch.borrow_mut();
        watch.play_hands_back()
    };
    if !was {
        return None;
    }
    // The row's own name comes from this window's recordings, so the sentence names a camera rather
    // than a number; `row_name` falls back to "row N" when nothing is laid out yet.
    let cut_ = newest_review_cut();
    // F2.10: the SESSION's recordings, from the open project's own footage rows — a second camera that
    // filmed the same seconds only gets its own row if it is in this list at all.
    let recordings = session_recordings(&cut_);
    let rows = crate::timeline::rows_for(&recordings, &cut_);
    let named = watched_row()
        .map(|row| cut_cam::row_name(row as i32, &recordings, &rows))
        .unwrap_or_else(|| "the watched row".to_string());
    set_watched_row(None);
    refresh_camera_rows(window);
    // And the panel goes back to the cut's own picture for the same reason a watch moved it.
    crate::ui::cut_picture::refresh(window);
    Some(format!("\u{25b6} plays the cut \u{2014} {named} no longer watched"))
}

/// F2.10: the box holding this window's camera rows, by its stable name — `None` on a page that drew
/// none. Every F2.10 press repaints through it, and Task 2 of this item builds its contents here.
pub fn camera_rows_box(window: &adw::ApplicationWindow) -> Option<gtk::Box> {
    // Scoped to this window's OWN content, exactly as [`fold_badges_box`] is. Searching from the window
    // object can walk into a previously closed window's tree (GTK does not destroy it on `close()`), and
    // then a refresh appends a row into that other window's box — the
    // `gtk_box_append: assertion 'gtk_widget_get_parent (child) == NULL' failed` this guards against.
    let content = window.content()?;
    find_widget_by_name(&content, "camera-rows")?
        .downcast()
        .ok()
}
