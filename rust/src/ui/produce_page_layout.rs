//! §08-produce#1-screen encoder settings: label + control pairs, three per line on the first two lines and two on each after, each label directly left of its control.
//! Two grids because the picture has two; every pair owns its own label and control columns (else GTK drew a label over the previous dropdown), and one size group aligns the first labels.

use adw::prelude::*;
use gtk4 as gtk;

use crate::produce_screen as screen;
use crate::ui::produce_page::{read_state, row_widget, ProduceState};

/// The picture's lines by §1 item: **11** two dropdown lines, **12** Subtitles · Translate, **13**/**14** Quality beside Frame timing, then Channels and Frame edges. Game audio gets its own line (a slider ran past the column); it is left out when narration is off.
const LINES: [&[&str]; 6] = [
    &["Container", "Codec", "Preset"],
    &["Resolution", "Frame rate", "Audio"],
    &["Subtitles", "Translate"],
    &["Game audio"],
    &["Quality (CRF)", "Frame timing"],
    &["Channels", "Frame edges"],
];

const DROPDOWN_LINES: usize = 2;

/// Build the settings into `settings` with the page's current values already in them, so `refresh` can rebuild rows without a repaint landing on a default. Rows come from `settings_rows_shown` (Game audio is absent when narration is off, §F4.8).
pub(crate) fn build_settings(settings: &gtk::Box) {
    let s = read_state();
    build_settings_from(settings, &s, &screen::settings_rows_shown());
}

/// The same rows seeded from `s`, showing `shown`: the page's grid and the Profiles window's (§03-shell F0.14 S9), which shows every row and reads back with [`rows_from`].
pub(crate) fn build_settings_from(settings: &gtk::Box, s: &ProduceState, shown: &[&str]) {
    let first_labels = gtk::SizeGroup::new(gtk::SizeGroupMode::Horizontal);
    let (dropdowns, rest) = LINES.split_at(DROPDOWN_LINES);
    for lines in [dropdowns, rest] {
        let grid = gtk::Grid::new();
        grid.set_row_spacing(6);
        grid.set_column_spacing(8);
        let lines = lines
            .iter()
            .map(|rows| rows.iter().filter(|row| shown.iter().any(|r| r == *row)));
        for (line, rows) in lines
            .filter(|rows| rows.clone().next().is_some())
            .enumerate()
        {
            for (pair, name) in rows.enumerate() {
                let pair = pair as i32;
                let label = setting_label(name);
                if pair == 0 {
                    first_labels.add_widget(&label);
                }
                if pair > 0 {
                    label.set_margin_start(12);
                }
                grid.attach(&label, 2 * pair, line as i32, 1, 1);
                grid.attach(&control(s, name), 2 * pair + 1, line as i32, 1, 1);
            }
        }
        settings.append(&grid);
    }
}

fn control(s: &ProduceState, name: &str) -> gtk::Widget {
    match name {
        "Game audio" | "Quality (CRF)" => {
            let (low, high, step, value) = if name == "Game audio" {
                (0.0, 1.0, 0.02, s.game_volume)
            } else {
                (screen::CRF_MIN as f64, screen::CRF_MAX as f64, 1.0, s.crf)
            };
            let scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, low, high, step);
            scale.set_widget_name(&row_widget(name));
            scale.set_draw_value(name == "Quality (CRF)");
            scale.set_value_pos(gtk::PositionType::Right);
            scale.set_size_request(140, -1);
            scale.set_value(value);
            scale.set_tooltip_text(Some(screen::tooltip(name)));
            scale.upcast()
        }
        "Frame timing" | "Channels" | "Frame edges" => {
            let (tick_label, widget, on) = match name {
                "Frame timing" => (screen::VFR_TICK, "tick-peak-rate-vfr", s.vfr),
                "Channels" => (screen::MONO_TICK, "tick-mono", s.mono),
                _ => (screen::EDGES_TICK, "tick-blurred", s.blurred_edges),
            };
            let tick = gtk::CheckButton::with_label(tick_label);
            tick.set_widget_name(widget);
            tick.set_active(on);
            tick.set_tooltip_text(Some(screen::tooltip(name)));
            tick.upcast()
        }
        "Translate" => translate_menu(s).upcast(),
        _ => {
            let picker = gtk::DropDown::from_strings(screen::options(name).unwrap_or(&[]));
            picker.set_widget_name(&row_widget(name));
            picker.set_halign(gtk::Align::Start);
            if let Some(at) = screen::options(name)
                .and_then(|list| list.iter().position(|v| *v == held_choice(s, name)))
            {
                picker.set_selected(at as u32);
            }
            picker.set_tooltip_text(Some(screen::tooltip(name)));
            picker.upcast()
        }
    }
}

/// §A's Translate: "menu of ticks". The button names the ticked languages or "none"; the popover holds one `translate-tick-<language>` per offered language.
fn translate_menu(s: &ProduceState) -> gtk::MenuButton {
    let menu = gtk::MenuButton::new();
    menu.set_widget_name(&row_widget("Translate"));
    menu.set_label(&translate_label(&s.translate, &s.session_language));
    menu.set_halign(gtk::Align::Start);
    menu.set_tooltip_text(Some(screen::tooltip("Translate")));
    let ticks = gtk::Box::new(gtk::Orientation::Vertical, 4);
    for language in screen::translate_options(&s.session_language) {
        let tick = gtk::CheckButton::with_label(language);
        tick.set_widget_name(&format!("translate-tick-{language}"));
        tick.set_active(s.translate.iter().any(|t| t == language));
        tick.set_tooltip_text(Some(screen::tooltip("Translate")));
        ticks.append(&tick);
    }
    let popover = gtk::Popover::new();
    popover.set_child(Some(&ticks));
    menu.set_popover(Some(&popover));
    menu
}

/// What the Translate button reads: the ticked languages in order, or "none". The video's own spoken language is never named (§F5.4 S3 drops it).
pub fn translate_label(ticked: &[String], spoken: &str) -> String {
    let own = crate::roles::language_code(spoken);
    let shown: Vec<&str> = ticked
        .iter()
        .filter(|language| own.is_empty() || crate::roles::language_code(language) != own)
        .map(String::as_str)
        .collect();
    if shown.is_empty() {
        "none".to_string()
    } else {
        shown.join(", ")
    }
}

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
    label.add_css_class("dim-label");
    label
}

/// The rows as a built grid holds them, spelled as a profile's `## produce` section: what the Profiles window saves.
pub(crate) fn rows_from(settings: &gtk::Widget) -> Vec<(String, String)> {
    let find = |name: &str| find_in(settings, name);
    screen::defaults()
        .iter()
        .filter_map(|(row, _)| {
            let value = match *row {
                "Frame timing" | "Channels" | "Frame edges" => {
                    let tick = match *row {
                        "Frame timing" => "tick-peak-rate-vfr",
                        "Channels" => "tick-mono",
                        _ => "tick-blurred",
                    };
                    let on = find(tick)?.downcast::<gtk::CheckButton>().ok()?.is_active();
                    (if on { "on" } else { "off" }).to_string()
                }
                "Translate" => {
                    find(&row_widget(row))?;
                    let ticked: Vec<&str> = screen::TRANSLATE_LANGUAGES
                        .into_iter()
                        .filter(|language| {
                            find(&format!("translate-tick-{language}"))
                                .and_then(|w| w.downcast::<gtk::CheckButton>().ok())
                                .is_some_and(|tick| tick.is_active())
                        })
                        .collect();
                    if ticked.is_empty() {
                        "none".to_string()
                    } else {
                        ticked.join(", ")
                    }
                }
                "Game audio" => format!(
                    "{:.2}",
                    find(&row_widget(row))?
                        .downcast::<gtk::Scale>()
                        .ok()?
                        .value()
                ),
                "Quality (CRF)" => format!(
                    "{:.0}",
                    find(&row_widget(row))?
                        .downcast::<gtk::Scale>()
                        .ok()?
                        .value()
                ),
                _ => find(&row_widget(row))?
                    .downcast::<gtk::DropDown>()
                    .ok()?
                    .selected_item()?
                    .downcast::<gtk::StringObject>()
                    .ok()?
                    .string()
                    .to_string(),
            };
            Some((row.to_string(), value))
        })
        .collect()
}

fn find_in(node: &gtk::Widget, name: &str) -> Option<gtk::Widget> {
    if node.widget_name() == name {
        return Some(node.clone());
    }
    let mut child = node.first_child();
    while let Some(current) = child {
        if let Some(found) = find_in(&current, name) {
            return Some(found);
        }
        child = current.next_sibling();
    }
    // A popover (the Translate ticks) is not among a menu button's children until it opens.
    node.downcast_ref::<gtk::MenuButton>()
        .and_then(|menu| menu.popover())
        .and_then(|popover| find_in(popover.upcast_ref(), name))
}

/// The same rows as flowing pairs for the Profiles window (§03-shell F0.14 S9), every item as wide as the widest. The Produce page keeps its fixed lines ([`build_settings_from`]).
pub(crate) fn build_settings_flow(settings: &gtk::Box, s: &ProduceState, shown: &[&str]) {
    let flow = gtk::FlowBox::new();
    flow.set_widget_name("produce-flow");
    flow.set_selection_mode(gtk::SelectionMode::None);
    flow.set_homogeneous(true);
    flow.set_min_children_per_line(1);
    flow.set_max_children_per_line(6);
    flow.set_row_spacing(6);
    flow.set_column_spacing(18);
    flow.set_hexpand(true);
    let labels = gtk::SizeGroup::new(gtk::SizeGroupMode::Horizontal);
    for name in shown {
        let pair = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let label = setting_label(name);
        labels.add_widget(&label);
        pair.append(&label);
        pair.append(&control(s, name));
        flow.insert(&pair, -1);
    }
    let mut child = flow.first_child();
    while let Some(current) = child {
        current.set_focusable(false);
        child = current.next_sibling();
    }
    settings.append(&flow);
}
