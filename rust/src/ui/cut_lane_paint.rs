//! §06-effects#2-the-lane-and-the-preview — the INK on a lane bar.
//!
//! The lane's rows and their CSS classes were drawn by `cut_lane_rows`, but a CSS rectangle says only "an effect
//! is somewhere here": nothing in it can be read. inventory/effects.md §A gives every kind something to say on its
//! own bar — a zoom its seconds, a speed `×N` plus its sound suffix or its seconds, a text the truncated words,
//! a volume the percentage, a label its name (§A.8) — and §2 colours them as a WASH over the pictures under the
//! lane rather than as solid blocks. So the bar grows one painted surface that carries the wash, the mark and the
//! caption, while everything the tests already read stays exactly where it was: the `fx-bar-<i>` box keeps its
//! `fx-kind-*` class, its width and its ✕ child, and this surface is appended UNDER them.
//!
//! Nothing here composes a rule. The colour and its alpha come from [`crate::fx_lane`] (`bar_colour`,
//! `bar_wash_alpha`), the rate spelling from [`crate::cut_speed::rate_label`], the percentage from
//! [`crate::fx_volume::percent_label`]. [`paint_bar`] is pure cairo over a [`BarView`] so a test can paint to an
//! image surface and sample pixels; [`attach`] is the one widget half.

use adw::prelude::*;
use gtk4 as gtk;

use crate::cut::{EffectKind, Fx};
use crate::cut_speed;
use crate::fx_band;
use crate::fx_lane;
use crate::fx_record;
use crate::fx_volume;

/// What one bar has to say, worked out from the record before any cairo runs. A plain struct so the painter is a
/// function of data: a test builds these and samples the pixels without a window.
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

/// The view for one record at one drawn size. `width` is what the refresh actually sized the bar to (the floored
/// number, not the raw seconds), because the ink has to fit the pixels a person is looking at.
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

/// The glyph that tells a bar's kind at a glance, chosen from the same words the dropdown and the status lines use
/// (`fx_record::label_of`) so a person never meets two spellings of one kind. A stop gets its own because §A.2
/// draws it as a different thing ("a still on the frame"), even though its `kind` is still `speed`.
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

/// The bar's own short line, per inventory/effects.md §A.8 read down to what fits on a bar: the seconds, the rate,
/// the words, the percentage, the name — no `at m:ss` (the bar's place on the lane IS its second) and no fades.
///
/// - zoom: `X.Xs`, §A.1's `label "X.Xs"`; a staying one says so with its colour, not with words.
/// - speed: `×N` + §A.3's sound suffix, or `X.Xs` when the rate is 1 (nothing was done to the clock) and for a
///   stop, whose seconds are the news (`\u{23f8}` already says which).
/// - text: the words, on one line and truncated; svg: the file's name, since §A.5's words ARE its file.
/// - volume: the percentage through [`fx_volume::percent_label`]; label: the name, capped at `fx_label::NAME_MAX_CHARS`.
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
        Some(EffectKind::Label) => effect.text.chars().take(crate::fx_label::NAME_MAX_CHARS).collect(),
        None => String::new(),
    }
}

/// Words onto one line and short enough to read at a glance: newlines become spaces (a bar is one row tall) and
/// anything past [`CAPTION_MAX_CHARS`] is cut with an ellipsis, which is §A.4's "the words truncated".
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

/// How many characters a bar prints before it truncates. Not a `P.` value (§10 gives the lane none): a bar is
/// ~200 px wide at the page's own 4 px/s and this is what fits in that at the font size below.
pub const CAPTION_MAX_CHARS: usize = 22;

/// The font size the mark and the caption are set at, and the ink they are set in. Both belong to the bar, not to
/// the lane: the row is 26 px (`cut_screen::EFFECT_ROW_PX`) and white is the one ink legible over every wash.
const INK_SIZE: f64 = 11.0;
const MARK_SIZE: f64 = 12.0;
const INK: [f64; 3] = [1.0, 1.0, 1.0];

/// Half the drawn ✕, in px — the glyph's arm reaches this far from the bar's middle. Not a `P.` value: §10 gives
/// the kill only `killMin 32`, which is [`fx_band::KILL_MIN_PX`] and is reached through, never restated here. This
/// is purely how big the mark looks inside a 26 px row, so it grows with the row and stops at 7 px.
pub const CROSS_HALF_PX_MAX: f64 = 7.0;

/// How far inside each end of the bar the grip tick is drawn, in px. The reach a press needs to count as a grip is
/// [`cut_select::GRIP_PX`] (read through, not restated); this is only where the ink that SHOWS the grip sits.
const GRIP_TICK_PX: f64 = 2.0;

/// Paint one bar into `cr`, in its own pixels (0,0 at its top-left). Pure cairo over a [`BarView`]: no widget, no
/// window, so a test can hand it an image surface and read the pixels back.
///
/// Order matters and is §A's: the wash first (it is what makes a bar a band over the pictures), then the tag's
/// tick and dashed line for a label, then the ink last so nothing washes the words out. The dash list is reset
/// before returning — cairo's dash is per-context, and a refresh loop would otherwise dash the next bar too.
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

    // F3.8 S1/S3 — the two press affordances a bar has to SHOW, gated by the same doors the press is answered
    // against so the ink can never promise a target the rules refuse (or hide one it honours):
    //   · end grips when the bar is ≥ `fx_band::GRIP_MIN_PX` wide — one tick inside each end;
    //   · the ✕ in the bar's MIDDLE when it is ≥ `fx_band::KILL_MIN_PX` wide.
    // They go on LAST of all, because both are marks meant to be read ON TOP of whatever the bar prints: an earlier
    // round pinned that the middle pixel of a zoom bar is its wash (`sec_06_effects_2_lane_painted`, which samples
    // `w/2, h/2`), and that claim now belongs to §A.9's kill — "the ✕ in the middle" IS what sits there once a bar
    // is wide enough for one. That test names the off-centre spot it samples instead; `f3_8_kill_and_grip_ink`
    // pins the cross being centred. A label is §A.7's tag beside a dashed line and carries no kill of its own.
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
            draw_cross(cr, w / 2.0, h / 2.0, (h / 2.0 - 2.0).clamp(2.0, CROSS_HALF_PX_MAX));
        }
    }
}

/// The kill mark: two crossed strokes centred on (`cx`, `cy`). White over any wash (§A's bars are all mid-tone),
/// round caps so the arms read as a glyph and not as a scratch. This is the ONLY place the cross is drawn: the
/// widget lane puts a transparent button over this same middle, so the page and the cairo snapshot cannot show
/// two different crosses.
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

/// How wide a label's tag is, in px. Not a `P.` value (§10 gives the lane none): §A.7 calls it a tag beside a
/// dashed line, and this is the stub that reads as one at `cut_screen::EFFECT_ROW_PX` tall.
pub const LABEL_TAG_PX: f64 = 14.0;

/// Ink centred vertically in the bar, at `size`. Errors are ignored the way the preview painter ignores them:
/// a cairo failure means the target is already unusable, and half a bar is no worse than none.
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

/// The painted surface's widget name, suffixed with the record's index by [`attach`] so a test can ask for
/// "the ink of bar #i" the same way it asks for its ✕.
pub const PAINT_SUFFIX: &str = "fx-bar-paint";

/// The bar's ink on an image surface a test can read pixels from — the door `narrate_band::painted_surface_for_test`
/// is for that lane. A GTK drawing area hands cairo its own target, so the only way to prove a bar was INKED and
/// not merely named by a CSS class is to run [`paint_bar`] over a surface and sample it.
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
///
/// Build-once like everything else in a refresh path: the surface is APPENDED before the ✕ so the button stays the
/// last child (a GTK box lays children out in order, and the ✕ must keep its place at the bar's middle), and its
/// draw func is replaced each time rather than connected again — a second controller/draw func would answer one
/// paint twice. The numbers come from the caller, which already computed them for the box, so the ink and the
/// affordances can never disagree about the bar's size.
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
        // The allocation wins over the request: before layout GTK reports 0 for the area, and painting nothing
        // then is what left the old CSS-only lane blank until someone resized the window.
        let shown = BarView {
            width: if width > 0 { width as f64 } else { painted.width },
            height: if height > 0 { height as f64 } else { painted.height },
            ..painted.clone()
        };
        paint_bar(cr, &shown);
    });
}

/// Direct-child lookup, scoped to this bar for the reason every lookup here is scoped: a closed GTK window is not
/// destroyed, so a tree-wide search can answer for another window's copy.
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
