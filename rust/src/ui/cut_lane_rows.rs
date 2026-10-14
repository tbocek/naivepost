//! §05-cut#1-screen — the effects lane's rows and the preview panel redrawn from them. Split out of
//! `window.rs`, which is past its size budget; its doors are re-exported through `window`.
//!
//! Every number on these surfaces comes from `fx_lane`'s view-model (`lane_layout`, `paused_scene`,
//! `playing_scene`, `plate_for`); nothing here composes a colour, an alpha, a row or a threshold, so the
//! rules cannot drift from the tests that pin them. The UI only pushes state into named widgets.

use adw::prelude::*;
use gtk4 as gtk;

use crate::cut;
use crate::cut_screen;
use crate::fx_aspect;
use crate::fx_band;
use crate::fx_lane;
use crate::ui::window as ui;

thread_local! {
    /// Set while [`refresh_effects_lane`] is pushing state into the widgets, so a property write made by
    // the refresh cannot be mistaken for a user action and re-enter itself — the same guard shape as
    // `CAMERA_REFRESH` (F2.10).
    static LANE_REFRESH: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };

    /// Whether the lane's CSS classes have been loaded once. The crate installs no display-wide provider
    /// except the scoped ones each surface needs; the lane's six kind colours are one of them, loaded a
    /// single time because loading per refresh would stack providers.
    static LANE_CSS_LOADED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// The lane's CSS, built from `fx_lane::bar_colour` rather than hand-written hex, so the palette in the
/// file and the palette on screen cannot disagree. One class per kind plus the staying zoom's own, and the
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
        css.push_str(&format!(
            ".{class} {{ background-color: rgba({}, {}, {}, 0.85); }}\n",
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
    css.push_str(&super::cut_debt_tail::css());
    css.push_str(&super::cut_preview_frame::css());
    // F0.4 S2: the gears' turning rule, installed with the rest so one provider carries both.
    css.push_str(&super::lucky_run::gears_css());

    let provider = gtk::CssProvider::new();
    provider.load_from_string(&css);
    gtk::style_context_add_provider_for_display(
        &gtk::gdk::Display::default().expect("a display to style"),
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
    LANE_CSS_LOADED.with(|cell| cell.set(true));
}

/// This window's effects-lane holder, resolved through its own content so a closed window's surviving
/// tree cannot answer for a live one — the same scoping `camera_rows_box` and `fold_badges_box` use: GTK
/// does not destroy a window on `close()`, so searching from the window object can walk into that other
/// tree and append into it (the `gtk_box_append: assertion 'gtk_widget_get_parent (child) == NULL' failed`
/// this guards against).
pub fn effects_lane_box(window: &adw::ApplicationWindow) -> Option<gtk::Box> {
    let content = window.content()?;
    ui::find_widget_by_name(&content, "effects-lane")?
        .downcast()
        .ok()
}

/// One of this window's preview widgets, scoped to its own content for the reason stated on
/// [`effects_lane_box`]. Every preview lookup goes through here rather than the window object.
pub(crate) fn preview_widget(window: &adw::ApplicationWindow, name: &str) -> Option<gtk::Widget> {
    let content = window.content()?;
    ui::find_widget_by_name(&content, name)
}

/// Redraw the effects lane and the preview panel from this window's cut, its held effect and the red line.
///
/// Build-once-then-update-properties (F2.10's rule): children are appended only when the count they stand
/// for changed, and always removed FROM THEIR HOLDER rather than by a tree-wide name search. Invisible
/// overlays stay in the tree and are hidden by opacity — adding and removing widgets mid-refresh is what
/// trips `gtk_box_append`'s parent assertion.
pub fn refresh_effects_lane(window: &adw::ApplicationWindow) {
    // §1's band costs a row per camera AND a row per cut lane, and an effect adds a row to the effects lane
    // inside it, so the band's surfaces are sized to the cut before anything is redrawn. `install` could only
    // size them for the empty page it was called on, and a `DrawingArea` clips its draw callback to its
    // allocation — an added row painted below the old height simply never appeared.
    crate::ui::cut_tracks::fit_band_height_for_window(window, &crate::ui::cut_view::band_scene_for(window));
    if LANE_REFRESH.with(|cell| cell.get()) {
        return;
    }
    LANE_REFRESH.with(|cell| cell.set(true));
    install_lane_css_once();

    let cut_ = ui::review_cut_of(window);
    let bars = fx_lane::lane_layout(&cut_.fx);
    let rows = fx_lane::row_count(&cut_.fx);
    let t = ui::paste_line(window);
    // The held effect is identified by its index in the cut's list, so the view can name the widget after
    // the record it stands for. A held effect not on this cut holds no overlay.
    let mut held_index = ui::held_effect().and_then(|held| {
        cut_.fx
            .iter()
            .position(|effect| effect.kind == held.kind && effect.t == held.t)
    });
    let mut scene = fx_lane::paused_scene(&cut_.fx, t, held_index, false);
    // F3.8 S2 (`Hold drops when the line walks > 1/24 s off the band`): the refresh path is where the page
    // learns the line has moved, so it is also where a hold that has walked away is given up. 1/24 s is one
    // frame at 24 fps, and the slack exists because the clock keeps running while something is in hand: a
    // band whose edge crosses the line by a single frame must not vanish from under the pointer mid-drag.
    if let Some(index) = held_index {
        if let Some(effect) = cut_.fx.get(index) {
            if fx_band::hold_drops(effect.t, effect.dur, t) {
                ui::set_held_effect(None);
                held_index = None;
            }
        }
    }
    // `paused_scene` deliberately leaves the record in hand OUT of `overlays` (it is being judged, not
    // watched), so the view adds it back here as the full-alpha outlined one. Its index comes from
    // `held_index`, which was resolved against this cut's own list above — a held effect that is not on
    // this cut draws no overlay either way.
    if let Some(index) = held_index {
        if let Some(effect) = cut_.fx.get(index) {
            if effect.effect_kind().is_some_and(fx_lane::drawn_paused) {
                scene.overlays.push((index, effect.effect_kind().expect("filtered above"), 1.0));
            }
        }
    }

    if let Some(lane) = effects_lane_box(window) {
        sync_lane_rows(&lane, rows.max(1));
        for bar in &bars {
            let Some(row_box) = lane_row_box(&lane, bar.row) else { continue };
            let bar_widget = ensure_named_child(&row_box, &format!("fx-bar-{}", bar.index), |slot| {
                let box_ = gtk::Box::new(gtk::Orientation::Horizontal, 0);
                box_.set_size_request(48, slot);
                // F3.8 S1/S2/S3: the bar's OWN drag gesture, attached once per bar widget for the same reason
                // the ✕ is connected once — a refresh that added a second controller would answer one press
                // twice. The rules it forwards to live in `fx_band`; this is only the wire.
                crate::ui::cut_band_press::wire_bar(window, bar.index, &box_);
                box_
            });
            // F3.8 S1/S3: the bar's DRAWN width comes from the record's own length, with 48 px only as a
            // FLOOR. The grip threshold (`fx_band::GRIP_MIN_PX` 30) and the ✕ threshold
            // (`fx_band::KILL_MIN_PX` 32) are stated against the width of the bar on screen, so a fixed width
            // makes the affordances the user sees disagree with the rule the press applies: a long record would
            // look grabbable in its middle while `grab_at` called it an edge, and a short one could show a ✕
            // the rules forbid. Re-set on EVERY refresh because `ensure_named_child` reuses the widget and a
            // nudge or a resize changes `dur` under it; `slot` stays the height (never read back — it is 0
            // before allocation). 48 px remains the floor so a record under 12 s is still big enough to click.
            let width = cut_bar_width_px(cut_.fx[bar.index].dur);
            let drawn = width.max(48.0).round();
            bar_widget.set_size_request(drawn as i32, bar_widget.height().max(1));
            crate::ui::cut_debt_tail::wear_tail(&bar_widget, &cut_, bar.index, t, &crate::ui::page_recordings());
            set_class(&bar_widget, "fx-kind-zoom-staying", bar.staying);
            if !bar.staying {
                set_class(&bar_widget, &format!("fx-kind-{}", spell_kind(bar.kind)), true);
            }
            // F3.8 S3: the ✕ lives INSIDE the bar it kills, named after the record rather than the row, so a
            // test can find "the kill button of effect #i" and click it. It exists only while the bar is wide
            // enough to carry one clear target away from its own grips (§A.9: `≥ 32 px`), and being added AND
            // removed off that width keeps a narrow bar from ever offering a ✕ that overlaps an edge grip.
            // The button is created ONCE per record index and its handler travels with it: `ensure_named_button`
            // returns the same widget on every refresh, so the click below is connected exactly once and cannot
            // stack a second removal on one press.
            // `width` is the SAME number the bar was just sized with above, so the drawn ✕ and the press rule
            // can never disagree about whether this bar has room for one.
            let kill_name = format!("fx-remove-{}", bar.index);
            if fx_band::kill_open(width) {
                let kill = ensure_named_button(&bar_widget, &kill_name, |slot| {
                    let mark = gtk::Button::with_label("\u{2715}");
                    mark.set_size_request(slot.min(20), slot);
                    mark
                });
                kill.set_tooltip_text(Some(
                    "remove this effect \u{2014} \u{21b6} Undo takes it back",
                ));
                let win_kill = window.clone();
                let index = bar.index;
                kill.connect_clicked(move |_| {
                    // F2.7 ✕ badges: the effect's cross is the `effect` badge, so it fires through the one
                    // badge door; the door forwards to `remove_effect_at`, whose sentence the fold rounds
                    // already pin, and prints it.
                    let said = crate::ui::cut_verb_press::press_badge(
                        &win_kill,
                        crate::cut_verbs::Badge::Effect,
                        index,
                    );
                    if let Some(status_line) = ui::find_status(win_kill.upcast_ref()) {
                        status_line.set_text(&said);
                    }
                });
            } else if let Some(existing) = bar_widget
                .first_child()
                .and_then(|node| walk_named(node, &kill_name))
            {
                // Drop the ✕ from its holder (never a tree-wide search) when the bar has shrunk under the
                // threshold; a stale ✕ would be a kill button nobody asked for.
                if let Some(parent) = existing.parent() {
                    if let Ok(parent_box) = parent.downcast::<gtk::Box>() {
                        parent_box.remove(&existing);
                    }
                }
            }
        }
        // A record that went away (an undo) takes its bar with it: drop any bar whose index is no longer
        // in this cut's list, from the row that holds it.
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
        // F3.1 S1: while a zoom is armed the camera layer goes DOWN so the whole source shows — a box cannot
        // be drawn against a frame that is already cropped. The opacity is added AND removed off `armed`, so a
        // disarmed page always gets its stroke back.
        let armed = ui::zoom_armed();
        rect.set_opacity(if armed || scene.rect_stroke_px <= 0.0 {
            0.0
        } else {
            1.0
        });
    }
    crate::ui::cut_preview_frame::repaint(window);
    if let Some(stack) = preview_widget(window, "preview-overlays")
        .and_then(|w| w.downcast::<gtk::Box>().ok())
    {
        crate::ui::cut_preview_panel::sync_overlay_stack(&stack, &scene, held_index);
    }
    if let Some(panel) = preview_widget(window, "preview-panel") {
        // F3.1 S1: the arm's own mark on the panel. The same rule `set_camera_layer_down` applies on an
        // arm/disarm without rebuilding anything: a full refresh has to paint the arm too, and both paths must
        // agree or a refresh could undo the arm's look.
        set_class(&panel, "zoom-camera-down", ui::zoom_armed());
        // The room is the panel's own width once laid out. Before layout GTK reports 0 for it, which is
        // "not measured yet" rather than "no room", so the natural size it asks its parent for stands in:
        // reading an unrealized 0 as no room would hide the plate from every headless check while a real
        // window shows it (`fx_lane::room_px`).
        let (minimum, natural) = panel.preferred_size();
        let asked = natural.width().max(minimum.width());
        let room = fx_lane::room_px(panel.width() as f64, asked.max(0) as f64);
        // What the plate says is the debt of the record under the red line, not a placeholder.
        let said = fx_lane::plate_for(fx_lane::plate_debt_at(&cut_.fx, t), room);
        if let Some(plate) = preview_widget(window, "preview-plate")
            .and_then(|w| w.downcast::<gtk::Label>().ok())
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

/// `EffectKind` back to its lower-case spelling for the CSS class name.
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

/// F3.8 S1/S2 (`ends are grips when the band is ≥ 30 px` / `an end, band ≥ 30 px`): how wide a bar on this
/// page's lane is, in pixels, for one record of `dur` seconds. The lane is drawn at the same zoom as the
/// tracks above it, so a bar's width is its length times the page's pixels-per-second — and every threshold
/// that decides what a press on it means ([`fx_band::grab_at`], [`fx_band::kill_open`]) is in pixels for the
/// same reason §A.9 gives: a grip is a reach for the hand, not a length of film.
pub fn cut_bar_width_px(dur: f64) -> f64 {
    dur * ui::SELECT_SURFACE_PPS
}

/// Get the named child of `holder`, creating it with `make` when it is not there yet.
fn ensure_named_child<F: FnOnce(i32) -> gtk::Box>(holder: &gtk::Box, name: &str, make: F) -> gtk::Box {
    if let Some(found) = find_in_holder(holder.as_ref(), name) {
        return found;
    }
    let made = make(holder.height().max(1));
    made.set_widget_name(name);
    holder.append(&made);
    made
}

/// F3.8 S3: the same build-once rule as [`ensure_named_child`], for a widget that is not a box — the ✕ in a
/// bar is a Button, and the two cannot share one helper without an upcast at every call site.
fn ensure_named_button<F: FnOnce(i32) -> gtk::Button>(holder: &gtk::Box, name: &str, make: F) -> gtk::Button {
    if let Some(found) = holder
        .first_child()
        .and_then(|node| walk_named(node, name))
        .and_then(|w| w.downcast::<gtk::Button>().ok())
    {
        return found;
    }
    let made = make(holder.height().max(1));
    made.set_widget_name(name);
    holder.append(&made);
    made
}

/// Find `name` anywhere under `node` (the ✕ sits inside the bar, so the flat sibling scan of
/// [`find_in_holder`] cannot reach it).
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

/// Add or remove a CSS class by state — added AND removed, so an outlined widget is always today's answer.
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

/// Find a direct child of `holder` by name. Direct only: a tree-wide search from a holder can land on a
/// different window's copy, since a closed GTK window is not destroyed.
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

/// Make the lane hold exactly `rows` row boxes, adding the missing ones and dropping the extra FROM THE
/// HOLDER. Height is `layout.effectRowPx` per row, so an empty lane is still one row deep.
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

/// The form column as the page sits idle (§1's "Form column", §A's "Idle rows"). A Grid named
/// `cut-form`: one row per reading, label in the left column and its value in the right, each value
/// named off `cut_screen::readout_widget` so a test reads the same eight rows the spec lists.
///
/// Two of the rows carry controls rather than a plain value: Thumbnails has the 🖼− / 🖼+ ladder
/// (40..160 px, one third at a step — `cut_screen::thumb_down`/`thumb_up`) and Aspect ratio has the
/// F3.2 dropdown (`aspect-choice`, listed from `fx_aspect::ASPECTS`). The thumbnail buttons write
/// through `set_thumb_px`; the dropdown writes through `press_aspect` below, and both repaint the row
/// they changed, so the readout is never a copy that drifted from the control.
pub(crate) fn cut_form_column() -> gtk::Grid {
    let form = gtk::Grid::new();
    form.set_widget_name("cut-form");
    form.set_row_spacing(2);
    form.set_column_spacing(8);
    form.set_halign(gtk::Align::Start);
    // What the readings show for a page with no cut yet: zeros and the open-at thumbnail size. The
    // real values arrive with the rounds that own them; this fills the rows so none is blank.
    let cut_ = crate::cut::Cut::default();
    let rows = cut_screen::idle_readouts(
        cut_screen::THUMB_AT_OPEN,
        cut_screen::ASPECT_DEFAULT,
        0.0,
        None,
        &cut_,
        0.0,
    );
    for (index, row) in rows.iter().enumerate() {
        let name = cut_screen::readout_widget(row.label);
        let key = gtk::Label::new(Some(&format!("{}:", row.label)));
        key.set_xalign(0.0);
        key.add_css_class("dim-label");
        key.set_widget_name(&format!("{name}-key"));
        let value = if row.tip.is_empty() {
            // No control on this row: the value is the whole of it.
            let value = gtk::Label::new(Some(&row.value));
            value.set_xalign(0.0);
            value.upcast::<gtk::Widget>()
        } else {
            // Thumbnails: the ladder sits beside the number it drives.
            let line = gtk::Box::new(gtk::Orientation::Horizontal, 4);
            let minus = gtk::Button::with_label("\u{1f5bc}\u{2212}");
            minus.set_widget_name("thumb-minus");
            minus.set_tooltip_text(Some(row.tip));
            let plus = gtk::Button::with_label("\u{1f5bc}+");
            plus.set_widget_name("thumb-plus");
            plus.set_tooltip_text(Some(row.tip));
            let value = gtk::Label::new(Some(&row.value));
            value.set_xalign(0.0);
            line.append(&minus);
            line.append(&value);
            line.append(&plus);
            line.upcast::<gtk::Widget>()
        };
        value.set_widget_name(&name);
        form.attach(&key, 0, index as i32, 1, 1);
        form.attach(&value, 1, index as i32, 1, 1);
    }
    // F3.2: the Aspect ratio row's dropdown, listing `fx_aspect::ASPECTS` — the five shapes §B gives, in
    // its order (source first, then tallest to widest). Not a local list: the spec's set is spelled once in
    // that module, and this control has no business inventing 4:3 or 21:9. Its tooltip is §B's whole
    // sentence (`DROPDOWN_HELP`), which is where "9:16 is a vertical short" and what the preview outline
    // means both get said. The starting selection reads the LIVE cut rather than always index 0, so the
    // control never shows a shape the cut does not have; an unset aspect is source, which is index 0.
    if let Some(aspect_row) = cut_screen::IDLE_FORM_ROWS
        .iter()
        .position(|l| *l == "Aspect ratio")
    {
        let choice = gtk::DropDown::from_strings(&fx_aspect::ASPECTS);
        choice.set_widget_name("aspect-choice");
        choice.set_tooltip_text(Some(fx_aspect::DROPDOWN_HELP));
        let start = fx_aspect::ASPECTS
            .iter()
            .position(|s| *s == cut_.aspect)
            .unwrap_or(0);
        choice.set_selected(start as u32);
        form.attach(&choice, 2, aspect_row as i32, 1, 1);
    }
    form
}
