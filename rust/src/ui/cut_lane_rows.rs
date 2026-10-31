//! §05-cut#1-screen — the effects lane's rows and the preview panel redrawn from them. Every number comes
//! from `fx_lane`'s view-model (`lane_layout`, `paused_scene`, `playing_scene`, `plate_for`); the UI only
//! pushes state into named widgets.

use adw::prelude::*;
use gtk4 as gtk;

use crate::cut;
use crate::cut_screen;
use crate::fx_aspect;
use crate::fx_band;
use crate::fx_lane;
use crate::ui::window as ui;

thread_local! {
    /// Set while [`refresh_effects_lane`] pushes state into the widgets, so its own property writes cannot re-enter it.
    static LANE_REFRESH: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };

    /// Whether the lane's CSS classes have been loaded once (loading per refresh would stack providers).
    static LANE_CSS_LOADED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// The lane's CSS, built from `fx_lane::bar_colour`: one class per kind plus the staying zoom's own and the
/// two outline classes the held overlay and a box under the hand wear.
fn install_lane_css_once() {
    if LANE_CSS_LOADED.with(|cell| cell.get()) {
        return;
    }
    let mut css = String::new();
    for (class, kind, staying) in [
        ("fx-kind-zoom", cut::EffectKind::Zoom, false),
        ("fx-kind-zoom-staying", cut::EffectKind::Zoom, true),
        ("fx-kind-speed", cut::EffectKind::Speed, false),
        ("fx-kind-text", cut::EffectKind::Text, false),
        ("fx-kind-svg", cut::EffectKind::Svg, false),
        ("fx-kind-volume", cut::EffectKind::Volume, false),
        ("fx-kind-label", cut::EffectKind::Label, false),
    ] {
        let (r, g, b) = fx_lane::bar_colour(kind, staying);
        // The SAME alpha the painted surface uses (`fx_lane::bar_wash_alpha`): §A.2 spells a bar as a wash over the
        // pictures, and CSS and cairo must not disagree.
        let a = fx_lane::bar_wash_alpha(kind);
        css.push_str(&format!(
            ".{class} {{ background-color: rgba({}, {}, {}, {a}); }}\n",
            (r * 255.0).round() as i32,
            (g * 255.0).round() as i32,
            (b * 255.0).round() as i32
        ));
    }
    let violet = fx_lane::HELD_OUTLINE;
    css.push_str(&format!(
        ".held-effect {{ border: 2px dashed rgba({}, {}, {}, 1.0); }}\n",
        (violet[0] * 255.0).round() as i32,
        (violet[1] * 255.0).round() as i32,
        (violet[2] * 255.0).round() as i32
    ));
    css.push_str(".drawing-box { border: 1px dashed #ffffff; }\n");
    // F3.8 S3: the kill target is invisible (the cross is `cut_lane_paint`'s ink); without this rule adwaita draws a button box over it.
    css.push_str(".fx-kill-target { background: none; border: none; box-shadow: none; outline: none; padding: 0; min-width: 0; min-height: 0; }\n");
    css.push_str(&super::cut_debt_tail::css());
    css.push_str(&super::cut_preview_frame::css());

    let provider = gtk::CssProvider::new();
    provider.load_from_string(&css);
    gtk::style_context_add_provider_for_display(
        &gtk::gdk::Display::default().expect("a display to style"),
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
    LANE_CSS_LOADED.with(|cell| cell.set(true));
}

/// This window's effects-lane holder, resolved through its own content: GTK does not destroy a window on
/// `close()`, so searching from the window object can walk into a closed window's tree and append into it
/// (`gtk_box_append: assertion 'gtk_widget_get_parent (child) == NULL' failed`).
pub fn effects_lane_box(window: &adw::ApplicationWindow) -> Option<gtk::Box> {
    let content = window.content()?;
    ui::find_widget_by_name(&content, "effects-lane")?
        .downcast()
        .ok()
}

/// One of this window's preview widgets, scoped like [`effects_lane_box`].
pub(crate) fn preview_widget(window: &adw::ApplicationWindow, name: &str) -> Option<gtk::Widget> {
    let content = window.content()?;
    ui::find_widget_by_name(&content, name)
}

/// Redraw the effects lane and the preview panel from this window's cut, held effect and red line.
///
/// Build-once-then-update-properties (F2.10): children are appended only when their count changed and
/// removed FROM THEIR HOLDER. Invisible overlays are hidden by opacity; adding and removing widgets
/// mid-refresh trips `gtk_box_append`'s parent assertion.
pub fn refresh_effects_lane(window: &adw::ApplicationWindow) {
    // The band costs a row per camera AND per cut lane, and an effect adds a row to the lane, so the surfaces
    // are sized to the cut first: a `DrawingArea` clips its draw callback to its allocation.
    crate::ui::cut_tracks::fit_band_height_for_window(
        window,
        &crate::ui::cut_view::band_scene_for(window),
    );
    if LANE_REFRESH.with(|cell| cell.get()) {
        return;
    }
    LANE_REFRESH.with(|cell| cell.set(true));
    install_lane_css_once();

    let cut_ = ui::review_cut_of(window);
    let bars = fx_lane::lane_layout(&cut_.fx);
    let rows = fx_lane::row_count(&cut_.fx);
    let t = ui::paste_line(window);
    // The held effect is identified by its index in the cut's list; one not on this cut holds no overlay.
    let mut held_index = ui::held_effect().and_then(|held| {
        cut_.fx
            .iter()
            .position(|effect| effect.kind == held.kind && effect.t == held.t)
    });
    let mut scene = fx_lane::paused_scene(&cut_.fx, t, held_index, false);
    // F3.8 S2 (`Hold drops when the line walks > 1/24 s off the band`): refresh is where the page learns the
    // line moved. 1/24 s is one frame at 24 fps; the slack is because the clock keeps running while something
    // is in hand, so a band whose edge crosses the line by a frame must not vanish mid-drag.
    if let Some(index) = held_index {
        if let Some(effect) = cut_.fx.get(index) {
            if fx_band::hold_drops(effect.t, effect.dur, t) {
                ui::set_held_effect(None);
                held_index = None;
            }
        }
    }
    // `paused_scene` leaves the record in hand OUT of `overlays`, so it is added back here as the full-alpha
    // outlined one (index from `held_index`).
    if let Some(index) = held_index {
        if let Some(effect) = cut_.fx.get(index) {
            if effect.effect_kind().is_some_and(fx_lane::drawn_paused) {
                scene
                    .overlays
                    .push((index, effect.effect_kind().expect("filtered above"), 1.0));
            }
        }
    }

    if let Some(lane) = effects_lane_box(window) {
        sync_lane_rows(&lane, rows.max(1));
        for bar in &bars {
            let Some(row_box) = lane_row_box(&lane, bar.row) else {
                continue;
            };
            let bar_widget = ensure_bar(&row_box, &format!("fx-bar-{}", bar.index), |slot| {
                // F3.8 S3: the bar is an OVERLAY so its ✕ sits OVER the middle of the wash (§A.9); a box put the cross past the bar's edge.
                let overlay = gtk::Overlay::new();
                overlay.set_size_request(48, slot);
                // F3.8 S1/S2/S3: the bar's OWN drag gesture, attached once per bar widget (a second controller would
                // answer one press twice). The rules live in `fx_band`.
                crate::ui::cut_band_press::wire_bar(window, bar.index, overlay.as_ref());
                overlay
            });
            // F3.8 S1/S3: the drawn width comes from the record's length, 48 px only as a FLOOR. The grip
            // (`fx_band::GRIP_MIN_PX` 30) and ✕ (`fx_band::KILL_MIN_PX` 32) thresholds are against the on-screen width,
            // so a fixed width would show affordances the press rules refuse. Re-set on EVERY refresh because
            // `ensure_named_child` reuses the widget; `slot` stays the height (0 before allocation).
            let width = cut_bar_width_px(cut_.fx[bar.index].dur);
            let drawn = width.max(48.0).round();
            bar_widget.set_size_request(drawn as i32, bar_widget.height().max(1));
            // §2 / inventory §A: the bar's INK (wash at `fx_lane::bar_wash_alpha`, the kind's mark, short line §A.8),
            // re-armed each refresh with the numbers just computed so paint and grip/✕ rules cannot disagree.
            let slot_h = fx_lane::lane_height_px(rows.max(1)) / rows.max(1) as f64;
            crate::ui::cut_lane_paint::attach(
                bar_widget.as_ref(),
                bar.index,
                &crate::ui::cut_lane_paint::view_for(&cut_.fx[bar.index], drawn, slot_h),
            );
            crate::ui::cut_debt_tail::wear_tail(
                bar_widget.as_ref(),
                &cut_,
                bar.index,
                t,
                &crate::ui::page_recordings(),
            );
            set_class(&bar_widget, "fx-kind-zoom-staying", bar.staying);
            if !bar.staying {
                set_class(
                    &bar_widget,
                    &format!("fx-kind-{}", spell_kind(bar.kind)),
                    true,
                );
            }
            // F3.8 S3: the ✕ lives INSIDE the bar it kills, named after the record so a test can click it. It exists
            // only while the bar is wide enough for one clear target away from its grips (§A.9: `≥ 32 px`). It is
            // created ONCE per record index with its handler (`ensure_named_button` reuses the widget), so the click
            // is connected once. `width` is the number the bar was just sized with.
            let kill_name = format!("fx-remove-{}", bar.index);
            if fx_band::kill_open(width) {
                let win_kill = window.clone();
                let index = bar.index;
                let kill = ensure_kill_target(&bar_widget, &kill_name, move |slot| {
                    // The cross a person SEES is `cut_lane_paint`'s ink; this button is the invisible hit target over it,
                    // gated by the same `fx_band::kill_open(width)`, so neither can offer a kill the other refuses.
                    let mark = gtk::Button::with_label("");
                    mark.add_css_class("fx-kill-target");
                    mark.set_halign(gtk::Align::Center);
                    mark.set_valign(gtk::Align::Fill);
                    mark.set_has_frame(false);
                    mark.set_can_target(true);
                    mark.set_size_request(fx_band::KILL_MIN_PX as i32, slot);
                    mark.connect_clicked(move |_| {
                        // F2.7 ✕ badges: the effect's cross is the `effect` badge, fired through the one badge door.
                        let said = crate::ui::cut_verb_press::press_badge(
                            &win_kill,
                            crate::cut_verbs::Badge::Effect,
                            index,
                        );
                        if let Some(status_line) = ui::find_status(win_kill.upcast_ref()) {
                            status_line.set_text(&said);
                        }
                    });
                    mark
                });
                kill.set_tooltip_text(Some(
                    "remove this effect \u{2014} \u{21b6} Undo takes it back",
                ));
            } else if let Some(existing) = bar_widget
                .first_child()
                .and_then(|node| walk_named(node, &kill_name))
            {
                // Drop the ✕ from the bar that carries it when the bar shrank under the threshold. Its parent is the
                // bar's Overlay, so this unparents.
                existing.unparent();
            }
        }
        let live: Vec<String> = bars
            .iter()
            .map(|bar| format!("fx-bar-{}", bar.index))
            .collect();
        for row in 0..rows.max(1) {
            if let Some(row_box) = lane_row_box(&lane, row) {
                let mut node = row_box.first_child();
                while let Some(child) = node {
                    let next = child.next_sibling();
                    if !live.contains(&child.widget_name().to_string()) {
                        row_box.remove(&child);
                    }
                    node = next;
                }
            }
        }
    }

    if let Some(rect) = preview_widget(window, "preview-camera-rect") {
        // F3.1 S1: while a zoom is armed the camera layer goes DOWN so the whole source shows; opacity is added
        // AND removed off `armed`.
        let armed = ui::zoom_armed();
        rect.set_opacity(if armed || scene.rect_stroke_px <= 0.0 {
            0.0
        } else {
            1.0
        });
    }
    crate::ui::cut_preview_frame::repaint(window);
    if let Some(stack) =
        preview_widget(window, "preview-overlays").and_then(|w| w.downcast::<gtk::Box>().ok())
    {
        crate::ui::cut_preview_panel::sync_overlay_stack(&stack, &scene, held_index);
    }
    if let Some(panel) = preview_widget(window, "preview-panel") {
        // F3.1 S1: the arm's own mark on the panel, as `set_camera_layer_down` applies; a refresh must paint it too.
        set_class(&panel, "zoom-camera-down", ui::zoom_armed());
        // The room is the panel's own width once laid out. Before layout GTK reports 0 ("not measured yet"), so
        // the natural size stands in; reading 0 as no room would hide the plate from headless checks (`fx_lane::room_px`).
        let (minimum, natural) = panel.preferred_size();
        let asked = natural.width().max(minimum.width());
        let room = fx_lane::room_px(panel.width() as f64, asked.max(0) as f64);
        // The plate says the debt of the record under the red line.
        let said = fx_lane::plate_for(fx_lane::plate_debt_at(&cut_.fx, t), room);
        if let Some(plate) =
            preview_widget(window, "preview-plate").and_then(|w| w.downcast::<gtk::Label>().ok())
        {
            match said {
                Some(text) => {
                    plate.set_text(&text);
                    plate.set_visible(true);
                }
                None => plate.set_visible(false),
            }
        }
    }

    LANE_REFRESH.with(|cell| cell.set(false));
}

fn spell_kind(kind: cut::EffectKind) -> &'static str {
    match kind {
        cut::EffectKind::Zoom => "zoom",
        cut::EffectKind::Speed => "speed",
        cut::EffectKind::Text => "text",
        cut::EffectKind::Svg => "svg",
        cut::EffectKind::Volume => "volume",
        cut::EffectKind::Label => "label",
    }
}

/// F3.8 S1/S2: how wide a bar on this lane is, in pixels, for a record of `dur` seconds: its length times
/// the page's pixels-per-second. The thresholds ([`fx_band::grab_at`], [`fx_band::kill_open`]) are in pixels
/// (§A.9): a grip is a reach for the hand, not a length of film.
pub fn cut_bar_width_px(dur: f64) -> f64 {
    dur * ui::SELECT_SURFACE_PPS
}

/// F3.8 S3: the same build-once rule as [`ensure_bar`], for the bar itself, which is an Overlay.
fn ensure_bar<F: FnOnce(i32) -> gtk::Overlay>(
    holder: &gtk::Box,
    name: &str,
    make: F,
) -> gtk::Overlay {
    if let Some(found) = holder
        .first_child()
        .and_then(|node| walk_named(node, name))
        .and_then(|w| w.downcast::<gtk::Overlay>().ok())
    {
        return found;
    }
    let made = make(holder.height().max(1));
    made.set_widget_name(name);
    holder.append(&made);
    made
}

/// F3.8 S3: the same build-once rule for the kill target; `add_overlay`, not `append`, or GTK treats it as a second main child.
fn ensure_kill_target<F: FnOnce(i32) -> gtk::Button>(
    bar: &gtk::Overlay,
    name: &str,
    make: F,
) -> gtk::Button {
    if let Some(found) = bar
        .first_child()
        .and_then(|node| walk_named(node, name))
        .and_then(|w| w.downcast::<gtk::Button>().ok())
    {
        return found;
    }
    let made = make(bar.size_request().1.max(1));
    made.set_widget_name(name);
    bar.add_overlay(&made);
    made
}

/// Find `name` anywhere under `node` (the ✕ sits inside the bar, beyond [`find_in_holder`]'s sibling scan).
fn walk_named(node: gtk::Widget, name: &str) -> Option<gtk::Widget> {
    if node.widget_name() == name {
        return Some(node);
    }
    let mut child = node.first_child();
    while let Some(inner) = child {
        if let Some(found) = walk_named(inner.clone(), name) {
            return Some(found);
        }
        child = inner.next_sibling();
    }
    None
}

/// Add or remove a CSS class by state, so an outlined widget is always today's answer.
pub(crate) fn set_class<W: AsRef<gtk::Widget>>(widget: &W, class: &str, on: bool) {
    let widget = widget.as_ref();
    if on {
        widget.add_css_class(class);
    } else {
        widget.remove_css_class(class);
    }
}

fn lane_row_box(lane: &gtk::Box, row: usize) -> Option<gtk::Box> {
    find_in_holder(lane.as_ref(), &format!("fx-lane-row-{row}"))
}

/// Find a direct child of `holder` by name. Direct only: a tree-wide search can land on a closed window's copy.
pub(crate) fn find_in_holder(holder: &gtk::Widget, name: &str) -> Option<gtk::Box> {
    let mut child = holder.first_child();
    while let Some(node) = child {
        if node.widget_name() == name {
            return node.downcast::<gtk::Box>().ok();
        }
        child = node.next_sibling();
    }
    None
}

/// Make the lane hold exactly `rows` row boxes, dropping extras FROM THE HOLDER. Height is
/// `layout.effectRowPx` per row, so an empty lane is still one row deep.
fn sync_lane_rows(lane: &gtk::Box, rows: usize) {
    let height = fx_lane::lane_height_px(rows) / rows.max(1) as f64;
    lane.set_size_request(-1, (height * rows as f64).ceil() as i32);
    let existing = lane.observe_children().n_items() as usize;
    for index in existing..rows {
        let row_box = gtk::Box::new(gtk::Orientation::Horizontal, 2);
        row_box.set_widget_name(&format!("fx-lane-row-{index}"));
        row_box.set_size_request(-1, height.ceil() as i32);
        lane.append(&row_box);
    }
    for _ in rows..existing {
        if let Some(last) = lane.last_child() {
            lane.remove(&last);
        }
    }
    // Re-set the height every pass: the row count can change without any row being added or removed.
    let mut child = lane.first_child();
    while let Some(node) = child {
        node.set_size_request(-1, height.ceil() as i32);
        child = node.next_sibling();
    }
}

/// The form column as the page sits idle (§1's "Form column", §A's "Idle rows"). A Grid named `cut-form`,
/// one row per reading, each value named off `cut_screen::readout_widget` so a test reads the same seven rows.
///
/// Aspect ratio carries the F3.2 dropdown (`aspect-choice`, from `fx_aspect::ASPECTS`), which writes through
/// `press_aspect` and repaints its row, so the readout never drifts from the control. The thumbnail size is the
/// grip at the bottom of the picture rows (§1 badge 2, `cut_band_trim`).
pub(crate) fn cut_form_column() -> gtk::Grid {
    let form = gtk::Grid::new();
    form.set_widget_name("cut-form");
    form.set_row_spacing(2);
    // The grid fills its pane: pinned to `Start` there was no right edge for a value to sit against.
    form.set_halign(gtk::Align::Fill);
    form.set_hexpand(true);
    form.set_column_homogeneous(false);
    form.set_column_spacing(8);
    // A page with no cut yet shows zeros so no row is blank.
    let cut_ = crate::cut::Cut::default();
    let rows = cut_screen::idle_readouts(cut_screen::ASPECT_DEFAULT, 0.0, None, &cut_, 0.0);
    for (index, row) in rows.iter().enumerate() {
        let name = cut_screen::readout_widget(row.label);
        let key = gtk::Label::new(Some(&format!("{}:", row.label)));
        key.set_xalign(0.0);
        key.add_css_class("dim-label");
        key.set_widget_name(&format!("{name}-key"));
        // The value reads at the RIGHT end (§1). GtkGrid has no per-column expand flag: a column grows when its
        // widgets expand, so the value cells carry it.
        let value = gtk::Label::new(Some(&row.value));
        value.set_xalign(1.0);
        value.set_hexpand(true);
        value.set_widget_name(&name);
        form.attach(&key, 0, index as i32, 1, 1);
        form.attach(&value, 1, index as i32, 1, 1);
    }
    // F3.2: the Aspect ratio dropdown lists `fx_aspect::ASPECTS` (§B's five shapes, source first), not a local
    // list. Its tooltip is §B's `DROPDOWN_HELP`. The selection reads the LIVE cut; an unset aspect is source (index 0).
    //
    // It goes BESIDE the readout in a third column: `refresh_aspect_readout` (`cut_text_hand.rs`) repaints that
    // label on every Undo/Revert/Clear and `tests/suite/aspect_choice_widgets.rs` reads it, so the control
    // must not replace the label. Every plain value expands to the pane's right edge (§1); this cell is readout + dropdown.
    if let Some(aspect_row) = cut_screen::IDLE_FORM_ROWS
        .iter()
        .position(|l| *l == "Aspect ratio")
        .map(|row| row as i32)
    {
        let choice = gtk::DropDown::from_strings(&fx_aspect::ASPECTS);
        choice.set_widget_name("aspect-choice");
        choice.set_tooltip_text(Some(fx_aspect::DROPDOWN_HELP));
        let start = fx_aspect::ASPECTS
            .iter()
            .position(|s| *s == cut_.aspect)
            .unwrap_or(0);
        choice.set_selected(start as u32);
        // The readout and its control become ONE value cell; the label keeps its name so the repaint and tests find it.
        let aspect_name = cut_screen::readout_widget("Aspect ratio");
        let held = form
            .observe_children()
            .iter::<glib::Object>()
            .filter_map(Result::ok)
            .filter_map(|child| child.downcast::<gtk::Widget>().ok())
            .find(|widget| widget.widget_name() == aspect_name);
        if let Some(label) = held {
            form.remove(&label);
            let cell = gtk::Box::new(gtk::Orientation::Horizontal, 6);
            cell.set_hexpand(true);
            cell.set_halign(gtk::Align::End);
            cell.append(&label);
            cell.append(&choice);
            form.attach(&cell, 1, aspect_row, 1, 1);
        } else {
            form.attach(&choice, 1, aspect_row, 1, 1);
        }
    }
    form
}
