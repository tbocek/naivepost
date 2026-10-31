//! The one badge the `03-settings` snapshot paints, and nothing else.

use adw::prelude::*;
use gtk4 as gtk;

use crate::checks;
use crate::ui;

thread_local! {
    /// The settings dialog the snapshot drew, kept so the paint cycle can reach its badges.
    static SETTINGS: std::cell::RefCell<Option<adw::Window>> = const { std::cell::RefCell::new(None) };

    /// The verdict [`stage_ffmpeg_row`] worked out, applied by [`apply_staged_badge`] once the window is
    /// mapped: at stage time the label has no allocation yet and a write to it would be lost.
    static PHASE: std::cell::RefCell<Option<checks::Phase>> = const { std::cell::RefCell::new(None) };
}

/// Keep the dialog reachable for [`apply_staged_badge`], which runs after the map.
pub fn remember(dialog: &adw::Window) {
    SETTINGS.with(|held| *held.borrow_mut() = Some(dialog.clone()));
}

/// The settings the `03-settings` picture shows, in the boxes.
pub fn pictured_conf() -> crate::settings::Conf {
    let mut conf = crate::settings::Conf::default();
    // The four audio.cpp ids §2 names for their tasks, so the rows read as a configured machine.
    conf.tts_model = "index-tts2".to_string();
    conf.asr_model = "qwen3-asr".to_string();
    conf.diar_model = "sortformer-diar".to_string();
    conf.sep_model = "bs-roformer".to_string();
    conf.align_model = "qwen3-aligner".to_string();
    // §5's picture is of a configured machine, and the Writing block has to show its own two ids: with
    // both boxes on their placeholders the Model row reads as unset, and "Fetch models" exists precisely to
    // fill that box, so the shot shows the id it would have produced.
    conf.server = "http://127.0.0.1:8731".to_string();
    conf.model = "qwen2.5vl:7b".to_string();
    conf
}

/// Ask the ffmpeg row what it makes of this machine's binary and stage the answer. The box is empty, as
/// §5's picture shows it, so this is the same test a person's press makes: `ffmpeg` on PATH.
pub fn stage_ffmpeg_row() {
    let verdict = crate::settings_probe_rows::ffmpeg_row("");
    if verdict.is_err() {
        return;
    }
    PHASE.with(|held| *held.borrow_mut() = Some(ui::settings::press_test("ffmpeg", "", verdict)));
}

/// Stage what "Fetch models" would have produced, from the ids §2 names for this machine.
pub fn stage_model_list(dialog: &adw::Window) {
    let conf = pictured_conf();
    let ids: Vec<String> = [
        conf.model.clone(),
        conf.tts_model.clone(),
        conf.asr_model.clone(),
        conf.diar_model.clone(),
        conf.sep_model.clone(),
    ]
    .into_iter()
    .filter(|id| !id.is_empty())
    .collect();
    // Staged only when the listing would really pass: an unexplained ✓ is worse than a blank mark, and
    // §5 keeps a failing verdict in a tooltip no photograph can show.
    let Ok(said) = crate::model_list::list_verdict(&ids) else {
        return;
    };
    let Some(list) = crate::ui::window::find_widget_by_name(dialog.upcast_ref(), "model-list")
        .and_then(|widget| widget.downcast::<gtk::DropDown>().ok())
    else {
        return;
    };
    let shown: Vec<&str> = ids.iter().map(String::as_str).collect();
    list.set_model(Some(&gtk::StringList::new(&shown)));
    // Start on the first id, which is what a listing with something to pick looks like; Use still needs its
    // own press before the Model box changes (§9).
    list.set_selected(0);
    let phase = ui::settings::press_test(ui::settings::FETCH_KEY, "", Ok(said));
    let mark = crate::ui::window::find_widget_by_name(
        dialog.upcast_ref(),
        &format!("badge-{}", ui::settings::FETCH_KEY),
    )
    .and_then(|widget| widget.downcast::<gtk::Label>().ok());
    if let Some(mark) = mark {
        ui::settings::paint_badge(&mark, &phase);
    }
}

/// Paint the staged verdict onto the dialog's `badge-ffmpeg`. Silent when nothing was staged.
pub fn apply_staged_badge() {
    let staged = PHASE.with(|held| held.borrow_mut().take());
    let (Some(phase), Some(dialog)) = (staged, SETTINGS.with(|held| held.borrow().clone())) else {
        return;
    };
    let badge = crate::ui::window::find_widget_by_name(dialog.upcast_ref(), "badge-ffmpeg")
        .and_then(|widget| widget.downcast::<gtk::Label>().ok());
    if let Some(badge) = badge {
        ui::settings::paint_badge(&badge, &phase);
    }
}
