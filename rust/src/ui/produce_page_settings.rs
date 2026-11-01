//! The Produce page's encoder rows: one setting stored, profile rows applied, and what is kept in the project.

use crate::produce_flow::codec_name;
use crate::produce_screen as screen;
use crate::project::{Codec, Container, Subtitles};
use crate::ui::narrate_page::say;
use crate::ui::produce_page::*;

pub(crate) fn store_setting(row: &str, value: &str) -> String {
    mutate(|s| store_setting_on(s, row, value))
}

/// One setting on a page state, shared by a hand on the page and a profile's `## produce` rows.
pub(crate) fn store_setting_on(s: &mut ProduceState, row: &str, value: &str) -> String {
    match row {
        "Container" => {
            let container = parse_container(value);
            let codec = parse_codec(&s.codec);
            let subtitles = parse_subtitles(&s.subtitles);
            let (forced_codec, forced_subs) =
                screen::apply_container_rules(container, codec, subtitles);
            s.container = container_name(container).to_string();
            s.codec = codec_name(forced_codec).to_string();
            s.subtitles = subtitles_name(forced_subs).to_string();
            if forced_codec != codec || forced_subs != subtitles {
                format!(
                    "container \u{2192}{} — it forces {} and no subtitle track",
                    s.container,
                    codec_name(forced_codec)
                )
            } else {
                format!("container \u{2192}{}", s.container)
            }
        }
        "Codec" => {
            s.codec = codec_name(parse_codec(value)).to_string();
            format!("codec {}", s.codec)
        }
        "Subtitles" => {
            s.subtitles = subtitles_name(parse_subtitles(value)).to_string();
            format!("subtitles {}", s.subtitles)
        }
        "Game audio" => {
            s.game_volume = value.parse().unwrap_or(s.game_volume);
            format!("game audio {:.2}", s.game_volume)
        }
        "Quality (CRF)" => {
            s.crf = value.parse().unwrap_or(s.crf);
            format!("quality crf {:.0}", s.crf)
        }
        other => {
            set_plain_row(s, other, value);
            format!("{other} {value}")
        }
    }
}

/// One setting changed on the row. Container carries §A's forcing rule (webm forces vp9); what got forced is said.
pub fn set_setting(window: &adw::ApplicationWindow, row: &str, value: &str) -> String {
    let said = store_setting(row, value);
    keep_settings();
    // The save dialog opens on this row's own offer, so an unanswered chooser still leaves the press naming it (`produce_press_wiring::asked_path`).
    if row == "Container" {
        crate::ui::produce_press_wiring::answer_path(None);
    }
    refresh(window);
    say(window, &said)
}

/// A scale moved: stored without [`refresh`], which would rebuild the grid and end the drag after its first step.
pub(crate) fn set_scale(window: &adw::ApplicationWindow, row: &str, value: &str) -> String {
    let said = store_setting(row, value);
    keep_settings();
    say(window, &said)
}

/// One of the three TICK rows changed (§A); separate from `set_setting`, as a tick has no value string.
pub fn set_toggle(window: &adw::ApplicationWindow, row: &str, on: bool) -> String {
    mutate(|s| match row {
        "Frame timing" => s.vfr = on,
        "Channels" => s.mono = on,
        "Frame edges" => s.blurred_edges = on,
        _ => (),
    });
    keep_settings();
    refresh(window);
    say(window, &format!("{row} {}", if on { "on" } else { "off" }))
}

/// F0.14 S9: a profile's `## produce` rows on the encoder settings, each as a hand would set it (ticks take on/off,
/// `Translate` takes `none` or comma-separated languages). Unlisted rows keep the project's value.
pub fn apply_rows(
    produce: &crate::project::Produce,
    rows: &[(String, String)],
) -> crate::project::Produce {
    let mut s = read_state_default();
    set_encoder_fields(&mut s, produce);
    for (row, value) in rows {
        let on = value.trim() == "on";
        match row.as_str() {
            "Frame timing" => s.vfr = on,
            "Channels" => s.mono = on,
            "Frame edges" => s.blurred_edges = on,
            "Translate" => {
                s.translate.clear();
                for language in value
                    .split(',')
                    .map(str::trim)
                    .filter(|l| !l.is_empty() && *l != "none")
                {
                    crate::ui::produce_languages::apply(&mut s, language, true);
                }
            }
            _ => {
                store_setting_on(&mut s, row, value.trim());
            }
        }
    }
    produce_settings(&s)
}

/// The narration's game audio level, set on the Narrate page (§07 §1) and kept with the encoder settings.
pub fn set_game_volume(value: f64) {
    mutate(|s| s.game_volume = value.clamp(0.0, 1.0));
    keep_settings();
}

pub(crate) fn encoder_state(produce: &crate::project::Produce) -> ProduceState {
    let mut s = read_state_default();
    set_encoder_fields(&mut s, produce);
    s
}

pub(crate) fn encoder_profile() -> Option<String> {
    let used = crate::ui::window::SESSION
        .with(|slots| slots.borrow().last().cloned())
        .and_then(|session| session.borrow().profile.clone())?;
    let paths = crate::settings::from_environment()?;
    let template = crate::profile::parse(&crate::profile::load(&paths, &used.name).ok()?).ok()?;
    template
        .produce
        .iter()
        .any(|(row, _)| row != "Game audio")
        .then_some(used.name)
}

pub fn show_encoder(produce: &crate::project::Produce) {
    mutate(|s| set_encoder_fields(s, produce));
    if let Some(window) = held_window() {
        refresh(&window);
    }
}

pub(crate) fn set_encoder_fields(s: &mut ProduceState, p: &crate::project::Produce) {
    s.container = container_name(p.container).to_string();
    s.codec = codec_name(p.codec).to_string();
    s.preset = crate::produce_render::preset_name(p.preset).to_string();
    s.resolution = match p.resolution {
        crate::project::Resolution::P720 => "720p",
        crate::project::Resolution::P1080 => "1080p",
        crate::project::Resolution::Original => "original",
    }
    .to_string();
    s.frame_rate = crate::produce_render::frame_rate_name(p.frame_rate).to_string();
    s.audio = p.audio_kbps.to_string();
    s.subtitles = subtitles_name(p.subtitles).to_string();
    s.translate = p.translate.clone();
    s.game_volume = p.game_volume;
    s.crf = f64::from(p.crf);
    s.vfr = p.vfr;
    s.mono = p.mono;
    s.blurred_edges = p.blurred_edges;
}

pub(crate) fn set_plain_row(s: &mut ProduceState, row: &str, value: &str) {
    match row {
        "Preset" => s.preset = value.to_string(),
        "Resolution" => s.resolution = value.to_string(),
        "Frame rate" => s.frame_rate = value.to_string(),
        "Audio" => s.audio = value.to_string(),
        _ => {}
    }
}

pub(crate) fn parse_container(value: &str) -> Container {
    match value {
        "mkv" => Container::Mkv,
        "webm" => Container::Webm,
        _ => Container::Mp4,
    }
}

pub(crate) fn parse_codec(value: &str) -> Codec {
    match value {
        "h265" => Codec::H265,
        "vp9" => Codec::Vp9,
        _ => Codec::H264,
    }
}

pub(crate) fn parse_subtitles(value: &str) -> Subtitles {
    match value {
        "burned in" => Subtitles::BurnedIn,
        "track in file" => Subtitles::TrackInFile,
        _ => Subtitles::None,
    }
}

/// The encoder rows as the project keeps them (`naivepost.json`'s `produce`, §01 §3), written on every change.
/// A failed write is said on the log; the row keeps the choice.
pub(crate) fn keep_settings() {
    if let Err(why) = keep_produce(&produce_settings(&read_state())) {
        crate::ui::window::log_line(&format!("!!! the encoder settings were not kept: {why}"));
    }
}

/// The thumbnail's words kept in the window's session so autosave and close write them. Kept in the project's
/// `publish` record, not `publish.json`, which a RUN wrote and whose existence says the upload text was written.
pub(crate) fn keep_page_publish() {
    keep_record();
    reprint_picture();
}

/// The page onto the project's record, which wins when the project opens. `own` is the page's outright, as the merge only adds it.
pub(crate) fn keep_record() {
    let Some(session) =
        crate::ui::window::PLAY_SESSION.with(|slots| slots.borrow().last().cloned())
    else {
        return;
    };
    let page = crate::ui::produce_snapshot::publish_of_state(&read_state());
    let own = page.own;
    let held = session.borrow().publish.clone().unwrap_or_default();
    let mut merged = crate::ui::produce_snapshot::merge_publish(Some(page), &held);
    merged.own = own;
    session.borrow_mut().publish = Some(merged);
}

/// The encoder settings kept in both places: the window's session (written whole by autosave and close) and the file.
/// The file alone let the next autosave put back the settings the window opened with.
pub(crate) fn keep_produce(produce: &crate::project::Produce) -> Result<(), String> {
    if let Some(session) =
        crate::ui::window::PLAY_SESSION.with(|slots| slots.borrow().last().cloned())
    {
        session.borrow_mut().produce = produce.clone();
    }
    let dir = crate::startup::open_dir();
    let mut project = crate::project::load(&dir).map_err(|why| why.to_string())?;
    project.produce = produce.clone();
    crate::project::save(&project, &dir).map_err(|why| why.to_string())
}

pub(crate) fn language_name(code: &str) -> String {
    match code {
        "en" => "English",
        "de" => "German",
        "fr" => "French",
        other => other,
    }
    .to_string()
}

pub(crate) fn subtitles_name(s: Subtitles) -> &'static str {
    match s {
        Subtitles::BurnedIn => "burned in",
        Subtitles::TrackInFile => "track in file",
        Subtitles::None => "none in the video",
    }
}

pub fn container_word() -> String {
    container_name(parse_container(&read_state().container)).to_string()
}

pub fn container_of_row() -> Container {
    parse_container(&read_state().container)
}

pub fn rendered_video() -> std::path::PathBuf {
    let dir = crate::startup::open_dir();
    match crate::layout::Tree::new(dir) {
        Ok(tree) => tree.final_video(container_word().as_str()),
        Err(_) => std::path::PathBuf::from("produce/final.mp4"),
    }
}

/// The page's row as the settings struct the transcode log reads; the page holds the row's truth until a run saves it.
pub fn row_settings() -> crate::project::Produce {
    produce_settings(&read_state())
}

pub(crate) fn produce_settings(s: &ProduceState) -> crate::project::Produce {
    let defaults = crate::project::Produce::default();
    crate::project::Produce {
        container: parse_container(&s.container),
        codec: parse_codec(&s.codec),
        resolution: match s.resolution.as_str() {
            "720p" => crate::project::Resolution::P720,
            "original" => crate::project::Resolution::Original,
            _ => crate::project::Resolution::P1080,
        },
        // Every row reaches the render: a shown choice the encode ignores would lie about the file.
        preset: [
            crate::project::Preset::Ultrafast,
            crate::project::Preset::Veryfast,
            crate::project::Preset::Fast,
            crate::project::Preset::Medium,
            crate::project::Preset::Slow,
            crate::project::Preset::VerySlow,
        ]
        .into_iter()
        .find(|preset| crate::produce_render::preset_name(*preset) == s.preset)
        .unwrap_or(defaults.preset),
        frame_rate: [
            crate::project::FrameRate::Source,
            crate::project::FrameRate::F60,
            crate::project::FrameRate::F30,
            crate::project::FrameRate::F24,
        ]
        .into_iter()
        .find(|rate| crate::produce_render::frame_rate_name(*rate) == s.frame_rate)
        .unwrap_or(defaults.frame_rate),
        audio_kbps: s.audio.parse().unwrap_or(defaults.audio_kbps),
        subtitles: parse_subtitles(&s.subtitles),
        game_volume: s.game_volume,
        crf: s.crf as u32,
        vfr: s.vfr,
        mono: s.mono,
        blurred_edges: s.blurred_edges,
        // The ticked languages travel with the settings (§F5.4 S7/S9 write one sidecar per language, in this order).
        translate: s.translate.clone(),
        ..Default::default()
    }
}

/// Store one typed box's whole text under the widget's own name. Verbatim: §1 gives no trimming rule for the four
/// boxes, and their spacing belongs to whoever wrote them.
pub fn set_text_field(field: &str, text: &str) -> String {
    let stored = mutate(|s| {
        match field {
            "edit-instruction" => {
                s.instruction = text.to_string();
            }
            "negative-prompt" => {
                s.negative = text.to_string();
            }
            "description-box" => {
                s.description = text.to_string();
            }
            "title-entry" => {
                s.title = text.to_string();
            }
            _ => return false,
        }
        true
    });
    if stored {
        keep_record();
        "".to_string()
    } else {
        "no such text box on this page".to_string()
    }
}
