//! The band's drawing passes, each from the scene alone: ruler, bars, marks, plates and the lanes.

use crate::ui::cut_colours::{
    BAND_GROUND, EDGE_AMBER, GUTTER_EDGE, GUTTER_GROUND, INSERT_VIOLET, JOIN_FRAME, JOIN_WASH,
    KEPT_BORDER, KEPT_EDGE, KEPT_TINT, KEPT_TINT_ALPHA, LANE_BORDER, LANE_GROUND, MARK_IN_GREEN,
    MARK_OUT_RED, OVER_EATEN, RULER_INK, SELECTION_BLUE, WATCH_OUTLINE,
};
use crate::ui::cut_tracks::*;
use crate::{cut, cut_screen, cut_trim, fx_lane, timeline};

/// Each cut lane as its own row below the cameras (§F2.12 S5). The PRESS surfaces (`bands_for`) deliberately
/// stop at the camera rows: F2.9/F2.10 own what a click on a row means.
pub(crate) fn draw_lane_rows(cr: &cairo::Context, scene: &Scene, rows: Rows, tape: f64) {
    for (index, lane) in scene.lanes.iter().enumerate() {
        let top = rows.lane_top(scene, index);
        // Past the gutter: a lane row filling from x = 0 paints over the rail and its sound switch reads as on the lane.
        let left = cut_screen::GUTTER_PX;
        fill(
            cr,
            LANE_GROUND,
            left,
            top,
            (tape - left).max(0.0),
            rows.row_h,
            1.0,
        );
        ink(cr, LANE_BORDER);
        cr.rectangle(left, top, (tape - left).max(0.0), 1.0);
        let _ = cr.fill();
        let (x0, width) = band_span_px(scene, lane.at, lane.at + lane.dur);
        if width > 0.0 {
            fill(
                cr,
                INSERT_VIOLET,
                x0,
                top + 2.0,
                width,
                rows.row_h - 4.0,
                0.75,
            );
        }
        cr.select_font_face("Sans", cairo::FontSlant::Normal, cairo::FontWeight::Normal);
        cr.set_font_size(9.0);
        ink(cr, (0.9, 0.9, 0.92));
        let plate = scene
            .lane_plates
            .get(index)
            .map(String::as_str)
            .unwrap_or(lane.name.as_str());
        text_at(cr, left + 2.0, top + 10.0, plate);
    }
}

/// §B's gutter: the first 30 px, inked from the ruler's bottom edge down (`draw_ruler` prints its first label at x = 2).
/// Painted LAST (see [`paint_at`]): several painters fill from x = 0 and would paint back over the rail. Keep the call order.
pub(crate) fn draw_gutter(cr: &cairo::Context, rows: Rows, height: f64) {
    fill(
        cr,
        GUTTER_GROUND,
        0.0,
        rows.selection,
        cut_screen::GUTTER_PX,
        (height - rows.selection).max(0.0),
        1.0,
    );
    ink(cr, GUTTER_EDGE);
    cr.rectangle(
        cut_screen::GUTTER_PX - 1.0,
        rows.selection,
        1.0,
        (height - rows.selection).max(0.0),
    );
    let _ = cr.fill();
}

/// F2.9 S2: the pieces a sound paste laid over the kept footage, with the painter [`crate::ui::cut_trim_press::paint_laid_pieces_in`].
pub(crate) fn draw_laid_pieces(cr: &cairo::Context, scene: &Scene, rows: Rows, tape: f64) {
    let _ = cr.save();
    cr.translate(cut_screen::GUTTER_PX, 0.0);
    crate::ui::cut_trim_press::paint_laid_pieces_in(
        cr,
        tape,
        &cut::Cut {
            segs: scene.segs.clone(),
            ..Default::default()
        },
        scene.pps,
        rows.pictures,
        rows.row_h,
    );
    let _ = cr.restore();
}

pub(crate) fn draw_ruler(cr: &cairo::Context, scene: &Scene, tape: f64) {
    let step = tick_step_seconds(scene.pps);
    // The ticks count footage, not the session clock, so a night between two recordings is no jump in the numbers.
    // Each at its second's PAINTED x (after the gutter, through the folds); a tick inside a fold is dropped.
    let runs = scene.runs();
    let filmed = timeline::filmed_seconds(&runs, scene.tape_end());
    let mut last = f64::NEG_INFINITY;
    let marks: Vec<(f64, String)> = cut_trim::ruler_ticks(filmed, scene.pps, step)
        .into_iter()
        .filter_map(|(x, label)| {
            let painted = band_x(scene, timeline::clock_at(&runs, x / scene.pps));
            let fresh = painted > last + 1.0;
            last = last.max(painted);
            (fresh && painted <= tape).then_some((painted, label))
        })
        .collect();
    ink(cr, RULER_INK);
    for (x, _) in &marks {
        cr.rectangle(*x, 0.0, 1.0, cut_trim::RULER_H);
        let _ = cr.fill();
    }
    labels(cr, &marks);
}

pub(crate) fn labels(cr: &cairo::Context, ticks: &[(f64, String)]) {
    cr.select_font_face("Sans", cairo::FontSlant::Normal, cairo::FontWeight::Normal);
    cr.set_font_size(9.0);
    ink(cr, RULER_INK);
    // Past a fold two ticks can land close together: the second keeps its line and gives up its label.
    let mut free_from = f64::NEG_INFINITY;
    for (x, label) in ticks {
        if *x >= free_from {
            text_at(cr, x + 2.0, cut_trim::RULER_H - 4.0, label);
            free_from = x + cr.text_extents(label).map_or(40.0, |seen| seen.width()) + 6.0;
        }
    }
}

pub(crate) fn draw_band_ground(cr: &cairo::Context, scene: &Scene, rows: Rows, tape: f64) {
    for (from, to) in scene.runs() {
        let (x0, width) = band_span_px(scene, from, to);
        if x0 >= tape || x0 + width < 0.0 {
            continue;
        }
        fill(
            cr,
            BAND_GROUND,
            x0,
            rows.selection,
            width,
            SELECTION_BAND_PX,
            1.0,
        );
    }
}

/// Green bar per kept scene (§1's badge 25, `go/cut_selband.go`): the scene the badges are about (held, else under the
/// line) is fuller and taller, with a light grip at each end. Boxes come from [`band_boxes`]; the ✕ is `cut_band_marks`'.
pub(crate) fn draw_kept_bars(cr: &cairo::Context, scene: &Scene, rows: Rows, tape: f64) {
    let current = scene.marks.hear.as_ref().map(|hear| hear.index);
    for (index, (x, w)) in band_boxes(scene).into_iter().enumerate() {
        if scene.segs[index].is_insert() || x >= tape || x + w < 0.0 {
            continue;
        }
        let (inset, alpha) = if current == Some(index) {
            (2.0, 0.5)
        } else {
            (4.0, 0.4)
        };
        fill(
            cr,
            KEPT_TINT,
            x,
            rows.selection + inset,
            w.max(1.0),
            SELECTION_BAND_PX - 2.0 * inset,
            alpha,
        );
        for grip in [x, x + w] {
            fill(
                cr,
                KEPT_BORDER,
                grip - 1.5,
                rows.selection,
                3.0,
                SELECTION_BAND_PX,
                1.0,
            );
        }
    }
}

/// The kept scenes over the pictures (`go/cut.go` 4154-4193): their stretch of ITS row tinted green, the wave strip fainter.
pub(crate) fn draw_kept_tint(cr: &cairo::Context, scene: &Scene, rows: Rows, tape: f64) {
    let film = (rows.row_h - row_wave_px()).max(0.0);
    for (index, (x, w)) in band_boxes(scene).into_iter().enumerate() {
        let seg = &scene.segs[index];
        if seg.is_insert() || x >= tape || x + w < 0.0 {
            continue;
        }
        let top = rows.pictures + seg.cam.max(0) as f64 * rows.row_h;
        fill(cr, KEPT_TINT, x, top, w, film, KEPT_TINT_ALPHA);
        fill(cr, KEPT_TINT, x, top + film, w, row_wave_px(), 0.16);
        for edge in [x, x + w] {
            fill(cr, KEPT_EDGE, edge - 1.0, top, 2.0, rows.row_h, 1.0);
        }
    }
}

pub(crate) fn draw_effects_lane(cr: &cairo::Context, scene: &Scene, rows: Rows, tape: f64) {
    // A bar under the span floor reserves no row (§2); painted after the row's owners it stays visible.
    let mut bars = fx_lane::lane_layout(&scene.fx);
    bars.sort_by_key(|bar| !bar.reserves_a_row);
    for bar in bars {
        let (from, to) = scene.fx[bar.index].spans();
        let y = crate::ui::cut_band_rows::lane_row_top_px(rows.effects, bar.row);
        let (x0, width) = band_span_px(scene, from, to);
        if x0 >= tape || x0 + width < 0.0 {
            continue;
        }
        // Same painter as the Cut page's own bars (`cut_lane_paint`), clipped to the bar's painted span.
        let view = crate::ui::cut_lane_paint::view_for(
            &scene.fx[bar.index],
            width.max(1.0),
            fx_lane::lane_height_px(1),
        );
        let _ = cr.save();
        cr.rectangle(x0, y, width.max(0.0), fx_lane::lane_height_px(1));
        cr.clip();
        cr.translate(x0, y);
        crate::ui::cut_lane_paint::paint_bar(cr, &view);
        let _ = cr.restore();
    }
    // §2's lower edge is DASHED (`spec/img/06-lane.png`), on the lane's LAST pixel row (`rows.pictures - 1`): §05's decode
    // gate samples the next row. `- 0.5` at width 1 covers exactly one pixel row; clear the pattern after or cairo hands it to every later stroke.
    ink(cr, LANE_BORDER);
    cr.set_line_width(1.0);
    cr.set_dash(&LANE_DASH_PX, 0.0);
    cr.move_to(cut_screen::GUTTER_PX, rows.pictures - 0.5);
    cr.line_to(tape, rows.pictures - 0.5);
    let _ = cr.stroke();
    cr.set_dash(&[], 0.0);
}

pub(crate) fn draw_recording_edges(cr: &cairo::Context, scene: &Scene, rows: Rows, tape: f64) {
    let film = (rows.row_h - row_wave_px()).max(1.0);
    for (index, recording) in scene.recordings.iter().enumerate() {
        let top = rows.pictures + scene.row_of(index) as f64 * rows.row_h;
        let (x0, x1) = (
            band_x(scene, recording.start) + 1.0,
            band_x(scene, recording.end) - 1.0,
        );
        if x1 < 0.0 || x0 > tape {
            continue;
        }
        edge_mark(cr, x0, 1.0, top, film, x1 - x0);
        if x1 - x0 > 3.0 {
            edge_mark(cr, x1, -1.0, top, film, x1 - x0);
        }
    }
}

pub(crate) fn edge_mark(cr: &cairo::Context, x: f64, dir: f64, top: f64, h: f64, room: f64) {
    let w = (room / 2.0).clamp(1.0, 9.0);
    let _ = cr.save();
    cr.rectangle(x.min(x + dir * w), top, w, h);
    cr.clip();
    cr.set_source_rgba(EDGE_AMBER.0, EDGE_AMBER.1, EDGE_AMBER.2, 0.7);
    cr.set_line_width(1.5);
    let mut dy = -w;
    while dy < h {
        cr.move_to(x, top + dy + w);
        cr.line_to(x + dir * w, top + dy);
        dy += 5.0;
    }
    let _ = cr.stroke();
    let _ = cr.restore();
    ink(cr, EDGE_AMBER);
    cr.set_line_width(2.0);
    cr.move_to(x, top);
    cr.line_to(x, top + h);
    let _ = cr.stroke();
}

/// The two washes §1 names over the picture: yellow over a recording the join pass took out entire (`retakes.tsv`
/// `whole`, F1.10; flagged, not an error), rose under a SPEED effect.
pub(crate) fn draw_join_washes(cr: &cairo::Context, scene: &Scene, rows: Rows, tape: f64) {
    let film = (rows.row_h - row_wave_px()).max(1.0);
    for (index, recording) in scene.recordings.iter().enumerate() {
        if !scene.marks.whole.contains(&recording.base) {
            continue;
        }
        let (x0, w) = band_span_px(scene, recording.start, recording.end);
        if x0 >= tape || x0 + w < 0.0 || w <= 2.0 {
            continue;
        }
        let top = rows.pictures + scene.row_of(index) as f64 * rows.row_h;
        fill(cr, JOIN_WASH, x0, top, w, film, 0.3);
        ink(cr, JOIN_FRAME);
        cr.set_line_width(2.0);
        cr.rectangle(x0 + 1.0, top + 1.0, w - 2.0, film - 2.0);
        let _ = cr.stroke();
    }
    let height = (rows.row_h * scene.row_count.max(1) as f64).max(rows.row_h);
    let rose = fx_lane::bar_colour(crate::cut::EffectKind::Speed, false);
    for effect in &scene.fx {
        if effect.effect_kind() != Some(crate::cut::EffectKind::Speed) {
            continue;
        }
        let (from, to) = effect.spans();
        let (x0, width) = band_span_px(scene, from, to);
        if x0 >= tape || x0 + width < 0.0 {
            continue;
        }
        fill(cr, rose, x0, rows.pictures, width, height, 0.35);
    }
}

pub(crate) fn draw_watched_row(cr: &cairo::Context, scene: &Scene, rows: Rows, tape: f64) {
    let Some(row) = scene.watched_row else { return };
    let y = rows.pictures + row.max(0) as f64 * rows.row_h;
    // Own rects rather than a stroked rectangle: a stroke straddles the path, so half falls outside the band for the bottom row.
    ink(cr, WATCH_OUTLINE);
    let from = band_x(scene, scene.runs().first().map(|run| run.0).unwrap_or(0.0));
    let to = band_x(scene, scene.tape_end()).min(tape);
    let mut x = from;
    while x < to {
        cr.rectangle(x, y, 12.0_f64.min(to - x), 2.0);
        cr.rectangle(x, y + rows.row_h - 2.0, 12.0_f64.min(to - x), 2.0);
        x += 20.0;
    }
    let _ = cr.fill();
}

pub(crate) fn draw_selection(cr: &cairo::Context, scene: &Scene, rows: Rows, tape: f64) {
    let Some((from, to)) = scene.selection else {
        return;
    };
    let (x0, width) = band_span_px(scene, from, to);
    if x0 >= tape || x0 + width < 0.0 {
        return;
    }
    fill(
        cr,
        SELECTION_BLUE,
        x0,
        rows.selection,
        width.max(1.0),
        SELECTION_BAND_PX,
        0.6,
    );
    fill(
        cr,
        SELECTION_BLUE,
        x0,
        rows.pictures,
        width.max(1.0),
        rows.row_h * scene.row_count.max(1) as f64,
        0.35,
    );
    ink(cr, SELECTION_BLUE);
    for x in [x0, band_x(scene, to)] {
        cr.rectangle(x - 1.0, rows.selection, 2.5, SELECTION_BAND_PX);
        let _ = cr.fill();
    }
}

pub(crate) fn draw_insert_bands(cr: &cairo::Context, scene: &Scene, rows: Rows, tape: f64) {
    let height = (rows.row_h * scene.row_count.max(1) as f64).max(rows.row_h);
    let drawn = band_boxes(scene);
    let boxes: Vec<cut_trim::Box_> = clip_boxes_shifted(&scene.segs, &scene.shift, scene.pps)
        .into_iter()
        .map(|b| cut_trim::Box_ {
            x: drawn[b.index].0,
            w: if scene.segs[b.index].is_overwrite_insert() {
                // An over card carries no `dur`, so `band_boxes` measured it zero-wide; its own span is what was eaten.
                ((scene.segs[b.index].e - scene.segs[b.index].s) * scene.pps).max(2.0)
            } else {
                drawn[b.index].1
            },
            index: b.index,
            insert: b.insert || !scene.segs[b.index].ins.is_empty(),
        })
        .collect();
    for b in boxes.iter().filter(|b| b.insert) {
        if b.x >= tape || b.x + b.w < 0.0 {
            continue;
        }
        if scene.segs[b.index].is_overwrite_insert() {
            fill(
                cr,
                OVER_EATEN,
                b.x,
                rows.pictures,
                b.w.max(2.0),
                height,
                1.0,
            );
        }
        fill(
            cr,
            INSERT_VIOLET,
            b.x,
            rows.pictures,
            b.w.max(2.0),
            height,
            0.6,
        );
        if scene.segs[b.index].s == scene.segs[b.index].e {
            ink(cr, (0.35, 0.2, 0.5));
            for step in 0..4 {
                cr.rectangle(b.x + step as f64 * 4.0, rows.pictures, 1.5, height);
                let _ = cr.fill();
            }
        }
        cr.select_font_face("Sans", cairo::FontSlant::Normal, cairo::FontWeight::Normal);
        cr.set_font_size(8.0);
        ink(cr, (0.2, 0.1, 0.3));
        let name = scene.segs[b.index]
            .ins
            .rsplit('/')
            .next()
            .unwrap_or("card")
            .to_string();
        text_at(cr, b.x + 2.0, rows.pictures + 9.0, &name);
    }
}

pub(crate) fn draw_in_out_marks(cr: &cairo::Context, scene: &Scene, rows: Rows, tape: f64) {
    let Some((from, to)) = scene.selection else {
        return;
    };
    let height = rows.pictures + rows.row_h;
    // No clamp to the tape: `band_x` runs linearly past the last filmed second.
    for (x, colour) in [
        (band_x(scene, from), MARK_IN_GREEN),
        (band_x(scene, to), MARK_OUT_RED),
    ] {
        if x < 0.0 || x > tape {
            continue;
        }
        fill(cr, colour, x, 0.0, MARK_LINE_PX, height, 1.0);
        cr.set_source_rgb(colour.0, colour.1, colour.2);
        cr.move_to(x, 0.0);
        cr.line_to(x + FLAG_PX, 4.0);
        cr.line_to(x, 8.0);
        cr.close_path();
        let _ = cr.fill();
    }
}

pub(crate) fn draw_name_plates(cr: &cairo::Context, scene: &Scene, rows: Rows) {
    cr.select_font_face("Sans", cairo::FontSlant::Normal, cairo::FontWeight::Normal);
    cr.set_font_size(9.0);
    // Past the gutter, not at x = 2: the dark rail is painted AFTER this pass and would eat the name. Each line on its own
    // light backing, one under the other (dark ink read as clipped letters on dark frames; one y overprinted a correction).
    // One plate per ROW (§1), naming the recording at the view's left edge, so a scrolled band names what is under the plate.
    let rows_drawn = (0..scene.recordings.len())
        .map(|index| scene.row_of(index) + 1)
        .max()
        .unwrap_or(0);
    for row in 0..rows_drawn {
        let on_row: Vec<&timeline::Recording> = scene
            .recordings
            .iter()
            .enumerate()
            .filter(|(index, _)| scene.row_of(*index) == row)
            .map(|(_, recording)| recording)
            .collect();
        let mut on_row = on_row;
        on_row.sort_by(|a, b| {
            a.start
                .partial_cmp(&b.start)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let Some(recording) = on_row
            .iter()
            .find(|recording| band_x(scene, recording.end) > scene.origin)
            .or(on_row.last())
        else {
            continue;
        };
        let top = rows.pictures + row as f64 * rows.row_h;
        for (line, word) in plates(scene, recording).iter().enumerate() {
            let y = top + 10.0 + line as f64 * PLATE_LINE_PX;
            let x = cut_screen::GUTTER_PX + 2.0;
            if let Ok(extent) = cr.text_extents(word) {
                cr.set_source_rgba(0.0, 0.0, 0.0, 0.55);
                cr.rectangle(x - 2.0, y - 9.0, extent.x_advance() + 4.0, PLATE_LINE_PX);
                let _ = cr.fill();
            }
            ink(cr, (0.95, 0.95, 0.95));
            text_at(cr, x, y, word);
        }
    }
}

pub(crate) fn text_at(cr: &cairo::Context, x: f64, y: f64, text: &str) {
    cr.move_to(x, y);
    let _ = cr.show_text(text);
}
