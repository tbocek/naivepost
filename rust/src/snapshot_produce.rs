//! The `08-produce` screen (§08-produce#1): images in the row, an instruction written, settings at their
//! §A defaults. The shared fixture carries no `produce/publish/` files, so the state is published through
//! the page's own door rather than poked. Split out of `snapshot.rs`, which is past the size another staging
//! block fits in; `set_setting_row` is the one place a §A row name maps to a state field.

use crate::ui;

/// Publish the shot's publish state onto `window`. Returns true when this module owned `screen`.
pub(crate) fn paint(window: &adw::ApplicationWindow, screen: &str) -> bool {
    if screen == "08-produce" {
        let mut state = ui::produce_page::read_state();
        state.frames = (1..=3)
            .map(|n| format!("project:produce/publish/frame-{n}.jpg"))
            .collect();
        state.instruction = "Make the speaker mid-gesture at the whiteboard, keep the room dark \u{2014} \
no words, no logos.".into();
        state.negative = "watermarks, lettering, extra limbs".into();
        state.title = "Ethereum, one lecture at a time".into();
        state.thumb_title = state.title.clone();
        // One marked box, so the shot shows the text-overlay editor's ✎ chip rather than an
        // empty chip row (§A: "press its ✎ to reword or remove it").
        state.texts = vec![crate::project::TextMark {
            cx: 0.5,
            cy: 0.25,
            wf: 1.0,
            hf: 0.4,
            text: "one lecture at a time".into(),
        }];
        // §1 prints the title across the picture, so the shot's picture carries its words.
        state.title_seeded = true;
        // The crop box the base wears: §A draws it only when the base's shape differs from the
        // video's, and both are unknown in the fixture, so a bare shot shows no crop box at all.
        state.base_shape = Some((1080, 1920));
        state.video_shape = (1920, 1080);
        state.description = "Lecture two of the series: gas, smart contracts and accounts, with the \
architecture behind them.
".into();
        // §A's Default column, spelled from the rules module so the shot agrees with the tests.
        for (row, value) in crate::produce_screen::defaults() {
            match row {
                "Container" | "Codec" | "Preset" | "Resolution" | "Frame rate" | "Audio"
                | "Subtitles" => set_setting_row(&mut state, row, value),
                _ => (),
            }
        }
        state.game_volume = 0.22;
        state.crf = 24.0;
        state.blurred_edges = true;
        state.clips = 7;
        state.seconds = 254.0;
        state.to_speak = 5;
        state.publish_written = true;
        state.out_files = 9;
        state.out_bytes = 41 * 1024 * 1024;
        ui::produce_page::set_state(state);
        ui::produce_page::refresh(window);
        return true;
    }

    false
}


/// Write one encoder row's default into the page's state, by the row name §A gives it. The page keeps
/// its settings as spelled strings, so this is the one place that maps a §A row to its field.
fn set_setting_row(state: &mut ui::produce_page::ProduceState, row: &str, value: &str) {
    match row {
        "Container" => state.container = value.to_string(),
        "Codec" => state.codec = value.to_string(),
        "Preset" => state.preset = value.to_string(),
        "Resolution" => state.resolution = value.to_string(),
        "Frame rate" => state.frame_rate = value.to_string(),
        "Audio" => state.audio = value.to_string(),
        "Subtitles" => state.subtitles = value.to_string(),
        _ => (),
    }
}
