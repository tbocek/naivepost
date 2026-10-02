//! The one badge the `03-settings` snapshot paints, and nothing else.
//!
//! §03's picture of the Settings dialog shows a passed ffmpeg test, but a snapshot presses no buttons:
//! badges only get painted by a press. So the ffmpeg row is asked once here, through the same door a click
//! goes through ([`naivepost::ui::settings::press_test`] is what `wire_buttons` wires every Test button to)
//! and painted by [`naivepost::ui::settings::paint_badge`]. Nothing is invented — the verdict is
//! [`crate::checks::ffmpeg_verdict`]'s own sentence, built from the `-filters`/`-encoders` listings of the
//! ffmpeg this machine actually has, so the shot reports a real build rather than an ideal one.

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
///
/// §03's photograph is of a machine with its models named and its parallel slots set. A snapshot reads
/// no `$XDG_CONFIG_HOME`, so this conf is built here and painted into the dialog by
/// [`crate::ui::settings::build_from`] — it is never written anywhere, and every value is one §5 or §6
/// names: the audio ids are the shipped defaults the app would use for each task, and the slot counts
/// show that the spin holds a number distinct from the id beside it.
pub fn pictured_conf() -> crate::settings::Conf {
    let mut conf = crate::settings::Conf::default();
    // The four audio.cpp ids §2 names for their tasks, so the rows read as a configured machine.
    conf.tts_model = "index-tts2".to_string();
    conf.asr_model = "qwen3-asr".to_string();
    conf.diar_model = "sortformer-diar".to_string();
    conf.sep_model = "bs-roformer".to_string();
    conf.align_model = "qwen3-aligner".to_string();
    // Two counts above 1, which is also what proves a spin is not just another copy of the box: absent
    // means 1 (§6), so anything else had to have been set.
    conf.slots.tts = 2;
    conf.slots.llm = 4;
    conf
}

/// Ask the ffmpeg row what it makes of this machine's binary and stage the answer.
pub fn stage_ffmpeg_row() {
    // The box names the command so `resolve` looks it up on PATH; `resolve("")` deliberately answers None
    // (an empty box means "use PATH" to the verdict, not to the lookup).
    let Some(program) = crate::settings_probe_rows::resolve("ffmpeg") else {
        return; // no ffmpeg on this machine at all: leave the mark blank rather than fake one
    };
    let shown = program.display().to_string();
    let probe = crate::services::ffprobe_for(&shown, "ffprobe");
    let ffprobe_beside = std::path::Path::new(&probe).is_file() || on_path("ffprobe").is_some();
    let listing = |args: &[&str]| {
        std::process::Command::new(&program)
            .args(args)
            .output()
            .ok()
            .filter(|out| out.status.success())
            .map(|out| String::from_utf8_lossy(&out.stdout).to_string())
    };
    let verdict = checks::ffmpeg_verdict(
        Some(&shown),
        ffprobe_beside,
        &listing(&["-version"]).unwrap_or_default(),
        listing(&["-hide_banner", "-filters"]).as_deref(),
        listing(&["-hide_banner", "-encoders"]).as_deref(),
    );
    // A pass needs every filter and encoder the pipeline uses. This container's ffmpeg has no rubberband
    // (the spec's picture came from a build that did), so the row genuinely fails here. Only a PASS is
    // painted: §5 puts a ✗'s reason in its tooltip, and a tooltip cannot be photographed, so a failing mark
    // would leave an unexplained ✗ in a static image. No mark beats a misleading one.
    if verdict.is_err() {
        return;
    }
    PHASE.with(|held| *held.borrow_mut() = Some(ui::settings::press_test("ffmpeg", &shown, verdict)));
}

/// Paint the staged verdict onto the dialog's `badge-ffmpeg`. Silent when nothing was staged.
pub fn apply_staged_badge() {
    let staged = PHASE.with(|held| held.borrow_mut().take());
    let (Some(phase), Some(dialog)) = (
        staged,
        SETTINGS.with(|held| held.borrow().clone()),
    ) else {
        return;
    };
    let badge = crate::ui::window::find_widget_by_name(dialog.upcast_ref(), "badge-ffmpeg")
        .and_then(|widget| widget.downcast::<gtk::Label>().ok());
    if let Some(badge) = badge {
        ui::settings::paint_badge(&badge, &phase);
    }
}

/// A bare command on `PATH`, asked the way [`crate::settings_probe_rows::resolve`] asks it.
fn on_path(name: &str) -> Option<std::path::PathBuf> {
    let path = std::env::var("PATH").ok()?;
    std::env::split_paths(&path).find_map(|dir| {
        let candidate = dir.join(name);
        candidate.is_file().then_some(candidate)
    })
}
