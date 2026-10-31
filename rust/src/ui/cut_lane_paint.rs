//! §06-effects#2-the-lane-and-the-preview — the INK on a lane bar.

use adw::prelude::*;
use gtk4 as gtk;

use crate::cut::{EffectKind, Fx};
use crate::cut_speed;
use crate::fx_band;
use crate::fx_lane;
use crate::fx_record;
use crate::fx_volume;

#[derive(Debug, Clone, PartialEq)]
pub struct BarView {
    pub kind: EffectKind,
    /// A reframing that holds: §2's other colour, and the only kind that has one.
    pub staying: bool,
    pub width: f64,
    pub height: f64,
    /// The short line the bar prints — see [`caption_of`].
    pub text: String,
    /// The kind's glyph at the bar's left edge — see [`mark_of`].
    pub mark: &'static str,
}

pub fn view_for(effect: &Fx, width: f64, height: f64) -> BarView {
    let kind = effect.effect_kind().unwrap_or(EffectKind::Label);
    let staying = fx_lane::staying_is_a_different_effect(kind) && effect.stay;
    BarView {
        kind,
        staying,
        width,
        height,
        text: caption_of(effect),
        mark: mark_of(effect),
    }
}

pub fn mark_of(effect: &Fx) -> &'static str {
    match effect.effect_kind() {
        Some(EffectKind::Zoom) => "\u{2295}",
        Some(EffectKind::Speed) if fx_record::is_stop(effect) => "\u{23f8}",
        Some(EffectKind::Speed) => "\u{23e9}",
        Some(EffectKind::Text) => "\u{275d}",
        Some(EffectKind::Svg) => "\u{25a8}",
        Some(EffectKind::Volume) => "\u{1f50a}",
        Some(EffectKind::Label) => "\u{1f3f7}",
        None => "",
    }
}

pub fn caption_of(effect: &Fx) -> String {
    match effect.effect_kind() {
        Some(EffectKind::Zoom) => format!("{:.1}s", effect.dur),
        Some(EffectKind::Speed) if fx_record::is_stop(effect) => format!("{:.1}s", effect.dur),
        Some(EffectKind::Speed) if (effect.rate - 1.0).abs() < f64::EPSILON => {
            format!("{:.1}s", effect.dur)
        }
        Some(EffectKind::Speed) => {
            // §A.3's lane column verbatim: "" nothing, "pitched", "· sound 1\u{00d7}", the scene's longer tail,
            // "silent". A rate of nought has no sound answer (§A.2's cost note says so), so it says none either.
            let suffix = match effect.snd.as_str() {
                "" | "mute" if !effect.snd.is_empty() && effect.rate > 0.0 => " silent",
                "pitched" => " pitched",
                "own" => " \u{00b7} sound 1\u{00d7}",
                "scene" => " \u{00b7} sound 1\u{00d7} to the scene's end",
                _ => "",
            };
            format!("{}{}", cut_speed::rate_label(effect.rate), suffix)
        }
        Some(EffectKind::Text) => one_line(&effect.text),
        Some(EffectKind::Svg) => std::path::Path::new(&effect.src)
            .file_name()
            .and_then(|name| name.to_str())
            .map(one_line)
            .unwrap_or_default(),
        Some(EffectKind::Volume) => fx_volume::percent_label(effect),
        Some(EffectKind::Label) => effect
            .text
            .chars()
            .take(crate::fx_label::NAME_MAX_CHARS)
            .collect(),
        None => String::new(),
    }
}

fn one_line(words: &str) -> String {
    let flat: String = words
        .chars()
        .map(|c| if c == '\n' || c == '\r' { ' ' } else { c })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if flat.chars().count() <= CAPTION_MAX_CHARS {
        return flat;
    }
    let head: String = flat.chars().take(CAPTION_MAX_CHARS - 1).collect();
    format!("{head}\u{2026}")
}

pub const CAPTION_MAX_CHARS: usize = 22;

const INK_SIZE: f64 = 11.0;
const MARK_SIZE: f64 = 12.0;
const INK: [f64; 3] = [1.0, 1.0, 1.0];

pub const CROSS_HALF_PX_MAX: f64 = 7.0;

const GRIP_TICK_PX: f64 = 2.0;

pub fn paint_bar(cr: &cairo::Context, view: &BarView) {
    let (r, g, b) = fx_lane::bar_colour(view.kind, view.staying);
    let alpha = fx_lane::bar_wash_alpha(view.kind);
    let (w, h) = (view.width.max(1.0), view.height.max(1.0));

    if view.kind == EffectKind::Label {
        // §A.7: a TAG, not a block — a narrow filled stub with a tick and a dashed line across the rest, because a
        // label changes nothing in the video and a full bar would claim otherwise.
        cr.set_source_rgba(r, g, b, alpha);
        cr.rectangle(0.0, 0.0, w.min(LABEL_TAG_PX), h);
        let _ = cr.fill();
        cr.set_source_rgba(INK[0], INK[1], INK[2], 0.75);
        cr.set_line_width(1.0);
        cr.move_to(w.min(LABEL_TAG_PX) + 2.0, h / 2.0);
        cr.line_to(w, h / 2.0);
        cr.set_dash(&[4.0, 3.0], 0.0);
        let _ = cr.stroke();
        cr.set_dash(&[], 0.0);
    } else {
        cr.set_source_rgba(r, g, b, alpha);
        cr.rectangle(0.0, 0.0, w, h);
        let _ = cr.fill();
        // The bar's own edge, a shade darker than its fill: these are the pixels a press takes as a grip
        // (`fx_band::grab_at` reads the same width the box was sized with), so the border marks where the grab
        // begins instead of leaving a soft wash with no visible start or end.
        cr.set_source_rgba(r * 0.6, g * 0.6, b * 0.6, 1.0);
        cr.set_line_width(1.0);
        cr.rectangle(0.5, 0.5, w - 1.0, h - 1.0);
        let _ = cr.stroke();
    }

    // The mark at the left, then the caption after it. Under `fx_band::GRIP_MIN_PX` there is no room for both —
    // the same threshold the grip rule uses, so the ink cannot promise text the bar has no pixels for — and a bar
    // that narrow prints its glyph alone.
    let mut x = 3.0;
    if !view.mark.is_empty() {
        draw_text(cr, view.mark, x, h, MARK_SIZE);
        x += text_width(cr, view.mark, MARK_SIZE) + 4.0;
    }
    if !view.text.is_empty() && w >= fx_band::GRIP_MIN_PX {
        draw_text(cr, &view.text, x, h, INK_SIZE);
    }

    if view.kind != EffectKind::Label {
        if fx_band::grips_open(view.width) {
            cr.set_source_rgba(r * 0.6, g * 0.6, b * 0.6, 1.0);
            cr.set_line_width(1.0);
            for edge in [GRIP_TICK_PX, w - GRIP_TICK_PX - 1.0] {
                cr.move_to(edge, 2.0);
                cr.line_to(edge, h - 2.0);
            }
            let _ = cr.stroke();
        }
        if fx_band::kill_open(view.width) {
            draw_cross(
                cr,
                w / 2.0,
                h / 2.0,
                (h / 2.0 - 2.0).clamp(2.0, CROSS_HALF_PX_MAX),
            );
        }
    }
}

fn draw_cross(cr: &cairo::Context, cx: f64, cy: f64, half: f64) {
    cr.set_source_rgba(1.0, 1.0, 1.0, 1.0);
    cr.set_line_width(1.5);
    cr.set_line_cap(cairo::LineCap::Round);
    cr.move_to(cx - half, cy - half);
    cr.line_to(cx + half, cy + half);
    cr.move_to(cx + half, cy - half);
    cr.line_to(cx - half, cy + half);
    let _ = cr.stroke();
    cr.set_line_cap(cairo::LineCap::Butt);
}

pub const LABEL_TAG_PX: f64 = 14.0;

fn draw_text(cr: &cairo::Context, words: &str, x: f64, height: f64, size: f64) {
    cr.set_source_rgba(INK[0], INK[1], INK[2], 1.0);
    cr.select_font_face("Sans", cairo::FontSlant::Normal, cairo::FontWeight::Bold);
    cr.set_font_size(size);
    if let Ok(extents) = cr.text_extents(words) {
        let middle = height / 2.0 - extents.height() / 2.0 - extents.y_bearing();
        cr.move_to(x, middle);
        let _ = cr.show_text(words);
    }
}

/// Width `words` takes at `size`, or 0 when cairo cannot say. Used only to place the caption after the mark.
fn text_width(cr: &cairo::Context, words: &str, size: f64) -> f64 {
    cr.select_font_face("Sans", cairo::FontSlant::Normal, cairo::FontWeight::Bold);
    cr.set_font_size(size);
    cr.text_extents(words).map(|e| e.width()).unwrap_or(0.0)
}

pub const PAINT_SUFFIX: &str = "fx-bar-paint";

pub fn painted_surface_for_test(view: &BarView) -> Result<cairo::ImageSurface, cairo::Error> {
    let surface = cairo::ImageSurface::create(
        cairo::Format::ARgb32,
        view.width.max(1.0) as i32,
        view.height.max(1.0) as i32,
    )?;
    {
        let cr = cairo::Context::new(&surface)?;
        paint_bar(&cr, view);
    }
    Ok(surface)
}

/// Give this bar its ink, building the surface once and re-arming its draw func on every refresh.
pub fn attach(bar: &gtk::Widget, index: usize, view: &BarView) {
    let name = format!("{PAINT_SUFFIX}-{index}");
    let surface = match bar.first_child().and_then(|node| find_named(node, &name)) {
        Some(found) => found,
        None => {
            // The ink is the bar's MAIN child (the bar is a `gtk::Overlay`, so the ✕ can sit over the middle of
            // this wash instead of after it), and it is set only while the overlay has no main child yet.
            let made = gtk::DrawingArea::new();
            made.set_widget_name(&name);
            // The ink is not a target: every press on a bar belongs to the bar's own drag gesture and to its ✕,
            // so this surface must never swallow one. A `DrawingArea` with no gesture controller takes no input,
            // and saying so keeps it true if a controller is ever added here.
            made.set_can_target(false);
            match bar.downcast_ref::<gtk::Overlay>() {
                // gtk4-rs 0.11 spells the overlay's main child `child`/`set_child` (GTK 4.2 names).
                Some(overlay) if overlay.child().is_none() => overlay.set_child(Some(&made)),
                _ => {
                    if let Some(holder) = bar.downcast_ref::<gtk::Box>() {
                        holder.append(&made);
                    }
                }
            }
            made
        }
    };
    surface.set_size_request(view.width.max(1.0) as i32, view.height.max(1.0) as i32);
    let painted = view.clone();
    surface.set_draw_func(move |_, cr, width, height| {
        let shown = BarView {
            width: if width > 0 {
                width as f64
            } else {
                painted.width
            },
            height: if height > 0 {
                height as f64
            } else {
                painted.height
            },
            ..painted.clone()
        };
        paint_bar(cr, &shown);
    });
}

fn find_named(start: gtk::Widget, name: &str) -> Option<gtk::DrawingArea> {
    let mut node = Some(start);
    while let Some(child) = node {
        if child.widget_name() == name {
            return child.downcast::<gtk::DrawingArea>().ok();
        }
        node = child.next_sibling();
    }
    None
}
