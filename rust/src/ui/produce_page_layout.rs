//! §08-produce#1-screen, the encoder settings: "laid out as label + control pairs in this order, left to
//! right and then down, several to a line". This module builds those rows and decides where each pair lands,
//! which is arithmetic over widths rather than a widget rule. Split out because `produce_page.rs` is at its
//! size budget.
//!
//! The spec states no per-line count, so the grouping comes from `spec/img/08-produce.png`: three
//! dropdown pairs on the first line, three on the second, then two each. That falls out of one budget
//! rather than a hand-written table — see [`LINE_COLUMNS`] and [`slots_for`]. A budget of FOUR cells held
//! only TWO dropdown pairs to a line (label + control is 2 cells), which is what the shot showed and what
//! the picture does not: six cells is what makes a line hold three.

use adw::prelude::*;
use gtk4 as gtk;

use crate::produce_screen as screen;
use crate::ui::produce_page::{read_state, row_widget, ProduceState};

/// Grid columns one line holds. TWO CELLS PER PAIR (label + control) × THREE pairs: that is what
/// `spec/img/08-produce.png` draws inside its ~620 px right column — Container/Codec/Preset, then
/// Resolution/Frame rate/Audio, then Subtitles/Translate, Quality (CRF)/Frame timing, Channels/Frame edges.
///
/// Four cells was HALF that: a dropdown pair costs 2 cells, so four cells fit two pairs and the page drew
/// Container|Codec, Preset|Resolution, Frame rate|Audio — nothing like the picture. Six cells is the same
/// grouping at the width the column actually has, and a fourth dropdown pair cannot squeeze onto a line
/// because there is no room left for it.
pub const LINE_COLUMNS: i32 = 6;

/// Slots one line holds — three, which is what `spec/img/08-produce.png` draws for the dropdown rows.
pub const SLOTS_PER_LINE: i32 = 3;

/// Grid columns one slot of a line is worth: six cells over three slots.
const PAIR_SLOTS: i32 = LINE_COLUMNS / SLOTS_PER_LINE;

/// The grid column a pair's LABEL sits in: `slot * PAIR_SLOTS` out of the six cells a line holds, shifted
/// back one cell for the pairs after the first. Their labels are right-aligned and shorter than the widest
/// one ("Preset:" against "Frame rate:"), so each starts one cell earlier and its control gets that cell
/// back — without it three dropdowns to a line measure ~57 px wider than the ~620 px column can give.
pub fn label_column(slot: i32) -> i32 {
    let slot = slot.clamp(0, SLOTS_PER_LINE - 1);
    (slot * PAIR_SLOTS - if slot == 0 { 0 } else { 1 }).max(0)
}

/// Where a pair's control ENDS: the grid column after it, which is where the next slot's label begins
/// (see [`label_column`]). A wide pair therefore reaches across the slot after its own, and a pair in the
/// last slot runs to the grid's end.
pub fn control_end(slot: i32) -> i32 {
    ((slot + 1).min(SLOTS_PER_LINE)) * PAIR_SLOTS
}

/// Where the next pair goes: `(row, column)` in the grid, after taking `row`/`col` as they stand and
/// wrapping to a fresh line when the pair would not fit. `col` is always a line start or a mid-line gap
/// left by the pairs already placed, so one wrap test is enough — no pair is wider than a line.
/// A wide pair (a slider, the Translate row) takes TWO of the three slots, so only one slot is left on its
/// line; every other pair takes one.
pub fn place_pair(at: (i32, i32), pair_row: &str) -> (i32, i32) {
    let (row, col) = at;
    if col + slots_for(pair_row) > SLOTS_PER_LINE {
        (row + 1, 0)
    } else {
        (row, col)
    }
}

/// How many of a line's slots a pair occupies: two for the wide rows, one for a dropdown or a tick. This is
/// the only place that decides which rows are wide — `spec/img/08-produce.png` gives a slider, and the row of
/// per-language ticks, about twice a dropdown's room.
fn slots_for(row: &str) -> i32 {
    match row {
        "Game audio" | "Quality (CRF)" | "Translate" => 2,
        _ => 1,
    }
}

/// One pair attached: the label at `col`, the control beside it spanning the rest of the pair, and the
/// cursor advanced past both. Every caller keeps its own widget names exactly as they were — this moves
/// nothing but the coordinates.
pub fn attach_pair<W: AsRef<gtk::Widget>, C: AsRef<gtk::Widget>>(
    grid: &gtk::Grid,
    at: &mut (i32, i32),
    label: &W,
    control: &C,
    row: &str,
) {
    // The wrap decision belongs HERE: the caller hands over the cursor and gets it back advanced, so there
    // is one place that decides whether a pair fits on the current line.
    let started_line = at.0;
    *at = place_pair(*at, row);
    let (line, slot) = *at;
    // Where the label goes and where the control stops: both follow from the slot, so a line's three pairs
    // always fill it edge to edge — a wide pair ends where the NEXT slot would have begun, and a pair in the
    // last slot runs to the grid's end, which is what makes a lone slider fill the row.
    let label_at = label_column(slot);
    // A label must not push its column wider than its word: see [`setting_label`].
    label.as_ref().set_hexpand(false);
    grid.attach(label.as_ref(), label_at, line, 1, 1);
    grid.attach(
        control.as_ref(),
        label_at + 1,
        line,
        control_end(slot) - label_at - 1,
        1,
    );
    at.1 = slot + slots_for(row);
    // The line only moves when a pair wraps, and the caller's `row` follows `at.0`, so one attach never
    // touches two grid lines.
    debug_assert!(
        line >= started_line,
        "a pair never spans two grid lines: {row}"
    );
}

/// Give every LABEL in the grid the same width — as much of it as the column can spare.
///
/// A `Grid` sizes a column from the widest cell in it, and with three pairs to a line inside a ~620 px
/// column those natural widths are what overflowed it (Preset ended at 1292 px against a column ending at
/// 1246): GTK hands an expanding control the slack FIRST and only then widens a label's column to its own
/// word. Labels are the cheap cells — they can be shortened without losing anything a person needs — so each
/// is pinned to [`LABEL_PX`] and the controls divide up everything left. Called by [`build_settings`] once
/// every pair is attached, because only then does the grid hold all its labels.
pub const LABEL_PX: i32 = 78;

pub fn even_label_columns(grid: &gtk::Grid) {
    let mut child = grid.first_child();
    while let Some(node) = child {
        if node.downcast_ref::<gtk::Label>().is_some() {
            node.set_size_request(LABEL_PX, -1);
        }
        child = node.next_sibling();
    }
}

/// The settings grid: one labelled control per row, drawn from `settings_rows_shown` so the
/// Game-audio row is ABSENT when narration is off (§F4.8), each widget named by `row_widget`.
/// Build the settings rows with the values the page currently holds already IN them. Doing it here, at
/// creation, is what lets `refresh` rebuild the grid whole without a repaint ever landing on a control
/// that shows a default instead of the state — and it needs no guard, because nothing is wired yet.
pub(crate) fn build_settings(grid: &gtk::Grid, mut row: i32) -> i32 {
    let s = read_state();
    // The grid cursor: §1 lays the rows out as label + control PAIRS "left to right and then down, several
    // to a line", so a line holds more than one pair and [`place_pair`] decides when one will not fit.
    // `row` stays the line number; `at.1` is where the next pair's label goes on it.
    let mut at = (row, 0);
    for name in screen::settings_rows_shown(s.narration_off) {
        let name: &str = name;
        match name {
            "Game audio" => {
                let scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 1.0, 0.02);
                scale.set_widget_name(&row_widget("Game audio"));
                scale.set_draw_value(false);
                // No `hexpand`: the grid gives the slider its room by COLUMN SPAN (two slots), and a widget
                // that also asks to expand takes every spare pixel on the line, which once pushed the grid
                // past the column's edge.
                scale.set_halign(gtk::Align::Fill);
                scale.set_value(s.game_volume);
                scale.set_tooltip_text(Some(screen::tooltip("Game audio")));
                attach_pair(grid, &mut at, &setting_label("Game audio"), &scale, name);
            }
            "Quality (CRF)" => {
                let scale = gtk::Scale::with_range(
                    gtk::Orientation::Horizontal,
                    screen::CRF_MIN as f64,
                    screen::CRF_MAX as f64,
                    1.0,
                );
                scale.set_widget_name(&row_widget("Quality (CRF)"));
                scale.set_halign(gtk::Align::Fill);
                scale.set_value(s.crf);
                scale.set_tooltip_text(Some(screen::tooltip("Quality (CRF)")));
                attach_pair(grid, &mut at, &setting_label("Quality (CRF)"), &scale, name);
            }
            "Frame timing" | "Channels" | "Frame edges" => {
                // Three tick rows: the tick carries §A's own word, not the row's name.
                let (tick_label, widget) = match name {
                    "Frame timing" => (screen::VFR_TICK, "tick-peak-rate-vfr"),
                    "Channels" => (screen::MONO_TICK, "tick-mono"),
                    _ => (screen::EDGES_TICK, "tick-blurred"),
                };
                let tick = gtk::CheckButton::with_label(tick_label);
                tick.set_widget_name(widget);
                tick.set_active(match name {
                    "Frame timing" => s.vfr,
                    "Channels" => s.mono,
                    _ => s.blurred_edges,
                });
                tick.set_tooltip_text(Some(screen::tooltip(name)));
                attach_pair(grid, &mut at, &setting_label(name), &tick, name);
            }
            "Translate" => {
                // One tick per offered language, built from `translate_options` so the session's own
                // language never appears as a row at all.
                let ticks = gtk::Box::new(gtk::Orientation::Horizontal, 8);
                // The row is ONE `setting-*` control holding its per-language ticks, so counting the
                // drawn `setting-*` names counts Translate as a row like every other one.
                ticks.set_widget_name(&row_widget("Translate"));
                for language in screen::translate_options(&s.session_language) {
                    let tick = gtk::CheckButton::with_label(language);
                    tick.set_widget_name(&format!("translate-tick-{language}"));
                    tick.set_active(s.translate.iter().any(|t| t == language));
                    tick.set_tooltip_text(Some(screen::tooltip("Translate")));
                    ticks.append(&tick);
                }
                attach_pair(grid, &mut at, &setting_label("Translate"), &ticks, name);
            }
            other => {
                let choices = screen::options(name).unwrap_or(&[]);
                let picker = gtk::DropDown::from_strings(choices);
                picker.set_widget_name(&row_widget(name));
                // A `Grid` hands a line's extra width to whichever cells ask for it, so with three dropdowns
                // on one line they each stretch and the line runs past the column (Preset ended at 1311 px
                // against a column ending at 1246). The picture shows them at their own size instead.
                picker.set_hexpand(false);
                // And no wider than its widest option: an expanding dropdown takes whatever the line gives it.
                picker.set_halign(gtk::Align::Start);
                if let Some(at_choice) = screen::options(name)
                    .and_then(|list| list.iter().position(|v| *v == held_choice(&s, name)))
                {
                    picker.set_selected(at_choice as u32);
                }
                picker.set_tooltip_text(Some(screen::tooltip(name)));
                attach_pair(grid, &mut at, &setting_label(other), &picker, name);
            }
        }
        row = at.0;
    }
    even_label_columns(grid);
    // One past the last line the pairs used, which is what callers stack below the grid.
    row + 1
}

/// The value the page holds for one dropdown row, so [`build_settings`] can seed the picker.
fn held_choice<'a>(s: &'a ProduceState, row: &str) -> &'a str {
    match row {
        "Container" => s.container.as_str(),
        "Codec" => s.codec.as_str(),
        "Preset" => s.preset.as_str(),
        "Resolution" => s.resolution.as_str(),
        "Frame rate" => s.frame_rate.as_str(),
        "Audio" => s.audio.as_str(),
        "Subtitles" => s.subtitles.as_str(),
        _ => "",
    }
}

fn setting_label(text: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(&format!("{text}:")));
    label.set_xalign(1.0);
    // Three pairs to a line inside a ~620 px column leaves no room for the labels to push: a `Grid` gives an
    // expanding control its EXTRA width first and only then widens the label's column to its natural width,
    // so with three dropdowns asking to expand the grid ran past the column's edge (Preset ended at 1311 px
    // against a column ending at 1246). A label that never asks to grow is what keeps the line inside.
    label.set_hexpand(false);
    label.add_css_class("dim-label");
    label
}
