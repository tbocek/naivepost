//! The Produce page's handlers: every widget connected to its press, once per window.

use crate::produce_screen as screen;
use crate::ui::narrate_page::say;
use crate::ui::produce_page::*;
use crate::ui::produce_page_state::{ProducePage, PAGE};
use adw::prelude::*;
use gtk4 as gtk;

/// Attach every handler, from `build_window` after `set_content`. `build()` runs once per window and each publishes
/// ITS OWN root, so calling this again for a second window is safe.
pub fn wire(window: &adw::ApplicationWindow) {
    // The page is the opened project's, loaded whole: the state is one per process, so a later project kept the first one's.
    if let Ok(tree) = crate::layout::Tree::new(crate::startup::open_dir()) {
        set_state(load_page(&tree));
    }
    // A profile edited while this project was closed still sets its encoder rows, which are greyed here as the profile's.
    crate::ui::profile_form::follow_profile_produce();
    // Hold the WINDOW: the slot row looks up its per-slot buttons by name from the window root.
    PAGE.with(|ProducePage { root: held, .. }| *held.borrow_mut() = Some(window.clone().upcast()));
    let win = window.clone();
    for name in [
        "add-image-button",
        "title-suggest",
        "thumbnail-export",
        "thumbnail-redraw",
        "transcode-save",
        "transcode-again",
    ] {
        if let Some(button) = crate::ui::line_step_button(&win, name) {
            let w = win.clone();
            let owned = name.to_string();
            button.connect_clicked(move |_| match owned.as_str() {
                "add-image-button" => {
                    crate::ui::produce_choosers::open_image_chooser(&w, None);
                }
                // The five presses that call a model, sd.cpp or ffmpeg walk on a worker (§03-shell §1): the button greys until the sentence lands.
                "title-suggest" => {
                    crate::ui::produce_press_wiring::reword_from_button(&w);
                }
                "thumbnail-export" => {
                    crate::ui::produce_choosers::open_export_dialog(&w);
                }
                "thumbnail-redraw" => {
                    crate::ui::produce_press_wiring::redraw_from_button(&w);
                }
                "transcode-save" => {
                    crate::ui::produce_choosers::open_save_video_dialog(&w);
                }
                _ => {
                    crate::ui::produce_press_wiring::transcode_from_button(&w);
                }
            });
        }
    }
    // Wired before the slot buttons: `refresh` below rebuilds the grid whole.
    wire_settings(&win);
    let text_win = win.clone();
    crate::ui::produce_text::wire_text(
        &win,
        std::rc::Rc::new(|| {
            PAGE.with(
                |ProducePage {
                     settings_guard: guard,
                     ..
                 }| guard.get(),
            )
        }),
        std::rc::Rc::new(move |field, text| {
            set_text_field(field, text);
            refresh(&text_win);
        }),
    );
    crate::ui::produce_words_press::install_words_gesture(&win);
    // `draw_slots` rebuilds the slot widgets whole, so `refresh` installs again for the new ones.
    crate::ui::produce_paint::install(&win);
    refresh(&win);
}

/// Attach every settings control to `set_setting` / `set_toggle` (§1 items 11–14): bail when the guard says the
/// emission is our own repaint, read the widget, hand it to the seam that owns the rule.
pub(crate) fn wire_settings(window: &adw::ApplicationWindow) {
    for row in [
        "Container",
        "Codec",
        "Preset",
        "Resolution",
        "Frame rate",
        "Audio",
        "Subtitles",
    ] {
        let Some(picker) =
            widget_in(window, &row_widget(row)).and_then(|w| w.downcast::<gtk::DropDown>().ok())
        else {
            continue;
        };
        let w = window.clone();
        let row = row.to_string();
        picker.connect_selected_notify(move |picker| {
            if PAGE.with(
                |ProducePage {
                     settings_guard: guard,
                     ..
                 }| guard.get(),
            ) {
                return;
            }
            let at = picker.selected() as usize;
            let Some(value) = screen::options(&row).and_then(|list| list.get(at)).copied() else {
                return;
            };
            set_setting(&w, &row, value);
        });
    }
    for row in ["Game audio", "Quality (CRF)"] {
        let Some(scale) =
            widget_in(window, &row_widget(row)).and_then(|w| w.downcast::<gtk::Scale>().ok())
        else {
            continue;
        };
        let w = window.clone();
        let row = row.to_string();
        scale.connect_value_changed(move |scale| {
            if PAGE.with(
                |ProducePage {
                     settings_guard: guard,
                     ..
                 }| guard.get(),
            ) {
                return;
            }
            set_scale(&w, &row, &scale.value().to_string());
        });
    }
    for (row, name) in [
        ("Channels", "tick-mono"),
        ("Frame edges", "tick-blurred"),
    ] {
        let Some(tick) =
            widget_in(window, name).and_then(|w| w.downcast::<gtk::CheckButton>().ok())
        else {
            continue;
        };
        let w = window.clone();
        let row = row.to_string();
        tick.connect_toggled(move |tick| {
            if PAGE.with(
                |ProducePage {
                     settings_guard: guard,
                     ..
                 }| guard.get(),
            ) {
                return;
            }
            set_toggle(&w, &row, tick.is_active());
        });
    }
    let w = window.clone();
    crate::ui::produce_languages::wire_ticks(
        window,
        std::rc::Rc::new(|| {
            PAGE.with(
                |ProducePage {
                     settings_guard: guard,
                     ..
                 }| guard.get(),
            )
        }),
        std::rc::Rc::new(move |language, on| {
            mutate(|s| crate::ui::produce_languages::apply(s, language, on));
            keep_settings();
            refresh(&w);
            let said = crate::ui::produce_languages::status_line(language, on);
            say(&w, &said);
        }),
    );
}

/// The row's widget name, the page's ONE spelling: "Quality (CRF)" → `setting-quality--crf`.
pub fn row_widget(row: &str) -> String {
    format!(
        "setting-{}",
        row.to_lowercase()
            .replace(['(', ')', ' '], "-")
            .trim_matches('-')
    )
}
